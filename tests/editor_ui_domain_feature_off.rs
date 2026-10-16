#![cfg(all(
    feature = "tooling",
    feature = "runtime-2d",
    feature = "serde",
    not(feature = "runtime-ui")
))]

use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara::{
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode},
    project_host::{EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress},
    reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{
        PrefabDocument, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentKind,
        EditorDocumentMutation, EditorProjectBrowserAction, PrefabCompositionCommand,
    },
};

#[test]
fn feature_off_editor_preserves_ui_structure_gate_for_retained_data_catalogs() {
    static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);
    let root = std::env::temp_dir().join(format!(
        "nara-ui-domain-feature-off-{}-{}",
        std::process::id(),
        NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    for directory in ["assets", "scenes", "prefabs", "schema"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    fs::write(root.join("nara.toml"), "schema_version = 1\n[project]\nname = \"Feature-off UI gate\"\n[paths]\nassets = \"assets\"\nscenes = \"scenes\"\nprefabs = \"prefabs\"\n[startup]\ndefault_scene = \"startup.scene.json\"\n[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n[runtime]\npreset = \"local-headless\"\n").unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/generic-editor/image-import/schema/authoring.bundle.json"),
        root.join("schema/authoring.bundle.json"),
    )
    .unwrap();
    fs::write(
        root.join("scenes/startup.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let entity = SceneEntityId::new("ui-root").unwrap();
    let component = ComponentTypeId::new("nara.ui.UiRoot");
    let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(entity.clone())
        .with_component(
            component.clone(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("layer", ComponentValue::String("base".into()))]),
            ),
        )]);
    fs::write(
        root.join("prefabs/ui.prefab.json"),
        prefab.to_json_string().unwrap(),
    )
    .unwrap();
    #[cfg(windows)]
    let directory = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(&root)
            .unwrap()
    };
    #[cfg(unix)]
    let directory = fs::File::open(&root).unwrap();
    let capability = DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap();
    let mut host = EditorProjectOpenHost::new();
    let mut attempt = host.begin(capability, None);
    let mut editor = loop {
        match host.advance(attempt) {
            EditorProjectOpenProgress::Pending(next) => attempt = next,
            EditorProjectOpenProgress::Opened(editor) => break editor,
            EditorProjectOpenProgress::Rejected(failure) => {
                panic!("generic open failed: {:?}", failure.diagnostics())
            }
        }
    };
    for _ in 0..128 {
        if !editor.current_frame().project_browser().has_pending_work() {
            break;
        }
        let _ = editor.drive_editor_frame(Duration::ZERO);
    }
    let frame = editor.current_frame();
    let opened = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::Open {
                    relative_path: "prefabs/ui.prefab.json".into(),
                },
            )],
        ))
        .unwrap();
    let _ = editor.drive_editor_frame(Duration::ZERO);
    let frame = editor.current_frame();
    let model = frame
        .workspace()
        .active_scene
        .as_ref()
        .expect("the browser publishes the retained Prefab");
    assert_eq!(
        model.kind,
        EditorDocumentKind::Prefab,
        "{:?}",
        opened.outcomes()
    );
    let document = model.document;
    let revision = model.revision;
    let history = model.editor.inspector.history;
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ComposePrefab {
                    document,
                    base_revision: revision,
                    command: PrefabCompositionCommand::RemoveComponent { entity, component },
                },
            )],
        ))
        .unwrap();
    let accepted =
        matches!(&report.outcomes()[0], EditorActionOutcome::Workspace(report) if report.applied);
    let _ = editor.drive_editor_frame(Duration::ZERO);
    let frame = editor.current_frame();
    let model = frame.workspace().active_scene.as_ref().unwrap();
    let unchanged = model.revision == revision && model.editor.inspector.history == history;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if matches!(
            editor.advance_close(),
            EditorProjectCloseProgress::Complete(_)
        ) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "editor physical close did not complete"
        );
        std::thread::yield_now();
    }
    drop(editor);
    let resolved = root.canonicalize().unwrap();
    assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    assert!(
        resolved
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("nara-ui-domain-feature-off-")
    );
    fs::remove_dir_all(resolved).unwrap();
    assert!(
        !accepted,
        "disabling runtime-ui must not admit removal of a retained UI root"
    );
    assert!(
        unchanged,
        "the specialized UI boundary must preserve revision and history"
    );
}
