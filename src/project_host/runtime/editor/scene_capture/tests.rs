//! Actual indexed documents qualify composition and publication through the Editor owner.

mod composition_failure;

use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

use nara_asset::{AssetPath, AssetRef};
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_reflect::{
    AuthoringSchemaBundle, ComponentFieldId, ComponentRegistry, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue,
};
use nara_scene::{
    PrefabDocument, PrefabDocumentCandidate, PrefabInstance, PrefabProjectionStep,
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, SourceDocumentKey, SourceDocumentKind,
};
use nara_tooling::EditorActionOutcome;
use nara_transform::Transform2d;

use super::*;
use crate::project_host::authoring::{
    AuthoringDocumentKind, AuthoringProjectAdmissionHost, PreparedAuthoringProject,
    project_authoring_adapters,
};
use crate::project_host::runtime::editor::document_open::EditorDocumentOpenRequestResult;

const SECOND: &str = "scenes/second.scene.json";
const LEAF: &str = "prefabs/leaf.prefab.json";
const INITIAL_ROTATION: f32 = 0.25;
const WAIT: Duration = Duration::from_secs(10);
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

struct Fixture {
    root: PathBuf,
    capability: DirectoryCapability,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_scene_capture_workflow_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("nara.toml"),
            r#"schema_version = 1
[project]
name = "Scene capture workflow"
source_universe = '6d57f8c5-867c-5463-b74a-057a8cabe2bc'
native_document_identity = 'project_location'
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "startup.scene.json"
[authoring]
schema_bundle = "schema/authoring.bundle.json"
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d"]
"#,
        )
        .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .unwrap()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("scenes/startup.scene.json"),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join(SECOND),
            SceneDocument::from_ordered_roots([anchor("anchor", "outer.prefab.json")])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("prefabs/outer.prefab.json"),
            PrefabDocument::from_ordered_roots([anchor("nested", "leaf.prefab.json")])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join(LEAF),
            leaf(INITIAL_ROTATION).to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("prefabs/unrelated.prefab.json"),
            leaf(INITIAL_ROTATION).to_json_string().unwrap(),
        )
        .unwrap();
        // Index discovery must not activate an unrelated malformed source.
        fs::write(
            root.join("prefabs/unused.prefab.json"),
            b"invalid unused Prefab",
        )
        .unwrap();
        let capability = open_capability(&root);
        Self { root, capability }
    }

    fn editor(&self) -> EditorProjectSession {
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(self.capability.share(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let adapters = project_authoring_adapters(schema.bundle()).unwrap();
        let receipt = PreparedAuthoringProject::new(schema, adapters)
            .admit_startup_closure()
            .unwrap()
            .publish();
        let mut editor = EditorProjectSession::open(receipt, None).unwrap();
        settle_source_index(&mut editor);
        editor
    }

    fn run(&self, test: impl FnOnce(&mut EditorProjectSession)) {
        let mut editor = self.editor();
        let result = catch_unwind(AssertUnwindSafe(|| test(&mut editor)));
        let deadline = Instant::now() + WAIT;
        loop {
            if matches!(
                editor.advance_close(),
                EditorProjectCloseProgress::Complete(_)
            ) {
                break;
            }
            assert!(Instant::now() < deadline, "the Editor owner did not retire");
            thread::yield_now();
        }
        if let Err(payload) = result {
            resume_unwind(payload);
        }
    }

    fn key(&self, path: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
        SourceDocumentKey::project_location(
            "6d57f8c5-867c-5463-b74a-057a8cabe2bc".parse().unwrap(),
            nara_asset::AssetSourcePath::new(path).unwrap(),
            kind,
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("nara_scene_capture_workflow_")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn open_capability(root: &Path) -> DirectoryCapability {
    #[cfg(windows)]
    let handle = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(root)
            .unwrap()
    };
    #[cfg(target_os = "linux")]
    let handle = fs::File::open(root).unwrap();
    DirectoryCapability::from_host_handle(
        handle,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap()
}

fn anchor(id: &str, path: &str) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(SceneEntityId::new(id).unwrap());
    record.prefab = Some(PrefabInstance {
        source: AssetRef::path(path).unwrap(),
        overrides: None,
    });
    record
}

fn transform_component() -> ComponentTypeId {
    ComponentTypeId::new("nara.transform.Transform2d")
}

fn leaf(rotation: f32) -> PrefabDocument {
    let registry = ComponentRegistry::from_snapshot(
        first_party_authoring_preview_registry(built_in_schema_providers()).unwrap(),
    );
    let component = transform_component();
    let schema = registry.schema(&component).unwrap();
    let ComponentValue::Map(mut fields) = schema.authoring_initial_value().unwrap().clone() else {
        panic!("the built-in Transform2d must expose its authoring fields");
    };
    fields.insert(
        "rotation".into(),
        ComponentValue::f64(f64::from(rotation)).unwrap(),
    );
    PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("body").unwrap(),
    )
    .with_component(
        component,
        SceneComponentRecord::new(schema.version, ComponentValue::Map(fields)),
    )])
}

fn settle_source_index(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + WAIT;
    while editor.project_source_index_work_pending() {
        editor.drive_editor_frame(Duration::ZERO);
        assert!(
            Instant::now() < deadline,
            "the real source index did not settle"
        );
        thread::yield_now();
    }
    assert!(editor.project_source_index_snapshot().is_some());
}

fn open_document(
    editor: &mut EditorProjectSession,
    path: &str,
    kind: AuthoringDocumentKind,
) -> EditorDocumentId {
    settle_source_index(editor);
    let index = editor.project_source_index_snapshot().unwrap();
    match editor.open_indexed_authoring_document(index, path, kind) {
        EditorDocumentOpenRequestResult::Opened { document }
        | EditorDocumentOpenRequestResult::Activated { document } => document,
        result => panic!("actual indexed Open of {path} failed: {result:?}"),
    }
}

fn edit_rotation(editor: &mut EditorProjectSession, document: EditorDocumentId, rotation: f32) {
    let basis = editor.document_query_basis(document).unwrap();
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: SceneEntityId::new("body").unwrap(),
        component: transform_component(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("rotation"),
        value: ComponentValue::f64(f64::from(rotation)).unwrap(),
    }]);
    let prepared = editor.prepare_authoring_edit(&basis, &patch).unwrap();
    let outcome = editor.commit_authoring_edit(prepared);
    let EditorActionOutcome::Workspace(report) = outcome else {
        panic!("a source-owned property edit must report its publication: {outcome:?}");
    };
    assert!(report.applied, "{:?}", report.diagnostics);
}

fn wait_preview(
    editor: &mut EditorProjectSession,
    document: EditorDocumentId,
    previous: Option<&EditorPreviewContentStamp>,
) -> EditorPreviewCandidate {
    let deadline = Instant::now() + WAIT;
    loop {
        editor.drive_editor_frame(Duration::ZERO);
        if let Some(candidate) = editor.take_editor_preview_candidate() {
            let content = candidate.content_stamp();
            if candidate.document() == document
                && previous != Some(&content)
                && editor.current_editor_preview_content_stamp().as_ref() == Some(&content)
            {
                assert_eq!(
                    candidate.revision(),
                    editor.workspace.document_revision(document).unwrap(),
                );
                return candidate;
            }
        }
        assert!(
            Instant::now() < deadline,
            "actual nested preview did not publish: {:?}",
            editor.diagnostics(),
        );
        thread::yield_now();
    }
}

fn assert_native_leaf(fixture: &Fixture, editor: &EditorProjectSession, rotation: f32) {
    let expected_source = fixture.key(SECOND, SourceDocumentKind::Scene);
    let world = editor.authoring_runtime.test_preview_world();
    let records = world
        .iter_entities()
        .filter_map(|entity| {
            let source = entity.get::<SceneEntitySource>()?;
            Some((source, entity.get::<Transform2d>()))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        records.len(),
        3,
        "anchor, nested anchor and native leaf must all survive"
    );
    assert!(
        records
            .iter()
            .all(|(source, _)| source.containing_source() == &expected_source)
    );
    let (source, transform) = records
        .iter()
        .find(|(source, _)| source.provenance().local_entity_id().as_str() == "body")
        .unwrap();
    assert_eq!(
        *transform,
        Some(&Transform2d {
            rotation,
            ..Transform2d::IDENTITY
        }),
    );
    let path = source.provenance().projection_path().unwrap();
    assert_eq!(path.root_anchor().as_str(), "anchor");
    assert_eq!(
        path.steps(),
        &[
            PrefabProjectionStep::SourceEntity {
                source_document: AssetRef::path("outer.prefab.json").unwrap(),
                canonical_source: fixture
                    .key("prefabs/outer.prefab.json", SourceDocumentKind::Prefab),
                entity: SceneEntityId::new("nested").unwrap(),
            },
            PrefabProjectionStep::SourceEntity {
                source_document: AssetRef::path("leaf.prefab.json").unwrap(),
                canonical_source: fixture.key(LEAF, SourceDocumentKind::Prefab),
                entity: SceneEntityId::new("body").unwrap(),
            },
        ],
    );
    let candidate = editor
        .authoring_runtime
        .test_preview_candidate()
        .expect("the accepted Authoring preview retains its actual expanded candidate");
    assert_eq!(candidate.source_key(), &expected_source);
    assert_eq!(candidate.document().entities.len(), 3);
    assert!(
        candidate
            .document()
            .entities
            .iter()
            .all(|record| record.prefab.is_none())
    );
    assert_eq!(
        candidate.provenance().get(&source.entity_id),
        Some(source.provenance()),
    );
}

fn dirty_count(editor: &EditorProjectSession) -> usize {
    editor
        .workspace
        .__document_ids()
        .into_iter()
        .filter(|&id| editor.workspace.document_is_dirty(id) == Some(true))
        .count()
}

#[test]
fn indexed_second_scene_materializes_native_nested_sources_without_flattening_authoring_data() {
    let fixture = Fixture::new();
    let startup_bytes = fs::read(fixture.root.join("scenes/startup.scene.json")).unwrap();
    let second_bytes = fs::read(fixture.root.join(SECOND)).unwrap();
    fixture.run(|editor| {
        let startup = editor.scene_preview_document.unwrap();
        let startup_revision = editor.workspace.document_revision(startup).unwrap();
        assert_eq!(
            editor.authoring_project().unwrap().prefab_sources().count(),
            0
        );
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let candidate = wait_preview(editor, second, None);
        assert!(candidate.preview_omissions().is_empty());
        assert_native_leaf(&fixture, editor, INITIAL_ROTATION);
        assert_eq!(
            editor.current_frame().scene_preview_document(),
            Some(second)
        );
        assert_eq!(
            editor.current_frame().scene_preview_unsaved_dependencies(),
            0
        );
        assert!(
            editor
                .current_frame()
                .scene_preview_recheck_after()
                .is_some()
        );
        assert_eq!(dirty_count(editor), 0);
        assert_eq!(
            editor.workspace.document_revision(startup),
            Some(startup_revision)
        );
        assert!(
            editor
                .workspace
                .scene(second)
                .unwrap()
                .session()
                .document()
                .entities[0]
                .prefab
                .is_some()
        );
        let content = candidate.content_stamp();
        let epoch = editor
            .authoring_runtime
            .preview_world_snapshot()
            .unwrap()
            .domain_id;
        for _ in 0..3 {
            let unchanged = wait_preview(editor, second, None);
            assert_eq!(unchanged.content_stamp(), content);
            assert_eq!(
                editor
                    .authoring_runtime
                    .preview_world_snapshot()
                    .unwrap()
                    .domain_id,
                epoch
            );
        }
        assert_eq!(
            editor.authoring_project().unwrap().prefab_sources().count(),
            0
        );
    });
    assert_eq!(fs::read(fixture.root.join(SECOND)).unwrap(), second_bytes);
    assert_eq!(
        fs::read(fixture.root.join("scenes/startup.scene.json")).unwrap(),
        startup_bytes
    );
}

#[test]
fn second_scene_hydrates_its_image_and_renders_from_the_captured_asset_database() {
    use nara_asset::{AssetMeta, AssetMetaIdentity, AssetSourceKind, StableAssetId};
    use nara_image::ImageImportSettings;
    use nara_sprite::Sprite;
    use nara_tooling::{EditorAction, EditorActionBatch, EditorNavigationAction};

    let fixture = Fixture::new();
    let path = AssetPath::new("second.png").unwrap();
    let stable_id = StableAssetId::parse_str("0de95ce6-5e75-48f8-b9aa-2c789de66494").unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
        fixture.root.join("assets/second.png"),
    )
    .unwrap();
    fs::write(
        fixture.root.join("assets/second.png.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(stable_id, path.clone(), AssetSourceKind::Image),
            ImageImportSettings::default(),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let registry = ComponentRegistry::from_snapshot(
        first_party_authoring_preview_registry(built_in_schema_providers()).unwrap(),
    );
    let component = ComponentTypeId::new("nara.sprite.Sprite");
    let schema = registry.schema(&component).unwrap();
    let ComponentValue::Map(mut fields) = schema.authoring_initial_value().unwrap().clone() else {
        panic!("the Sprite fixture requires declared initial fields");
    };
    let Some(ComponentValue::Map(material)) = fields.get_mut("material") else {
        panic!("the Sprite fixture requires declared material fields");
    };
    material.insert(
        "image".into(),
        ComponentValue::map([
            ("kind", ComponentValue::String("path".into())),
            ("value", ComponentValue::String(path.as_str().into())),
        ]),
    );
    let mut document = leaf(INITIAL_ROTATION);
    document.entities[0].components.insert(
        component,
        SceneComponentRecord::new(schema.version, ComponentValue::Map(fields)),
    );
    fs::write(fixture.root.join(LEAF), document.to_json_string().unwrap()).unwrap();
    let original = fs::read(fixture.root.join(SECOND)).unwrap();
    fixture.run(|editor| {
        assert!(
            editor
                .authoring_project()
                .unwrap()
                .asset_database()
                .is_empty()
        );
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let revision = editor.workspace.document_revision(second).unwrap();
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let deadline = Instant::now() + WAIT;
        loop {
            editor.drive_editor_frame(Duration::ZERO);
            if let Some(candidate) = editor.take_editor_preview_candidate()
                && !candidate.hits().is_empty()
            {
                let image = editor
                    .image_import
                    .last_good(&nara_asset::AssetSourcePath::new("assets/second.png").unwrap())
                    .expect("the captured Image is hydrated");
                assert_eq!(image.stable_id(), stable_id);
                let sprite = editor
                    .authoring_runtime
                    .test_preview_world()
                    .iter_entities()
                    .find_map(|entity| entity.get::<Sprite>())
                    .expect("the real preview contains its Sprite");
                assert_eq!(sprite.material.image, Some(image.handle()));
                assert_eq!(candidate.document(), second);
                assert!(candidate.preview_omissions().is_empty());
                assert_eq!(
                    editor.current_editor_preview_content_stamp(),
                    Some(candidate.content_stamp())
                );
                let hit = &candidate.hits()[0];
                assert!(hit.contains_clip_point([0.0, 0.0]));
                assert_eq!(hit.entity().as_str(), "anchor");
                let selected = hit.entity().clone();
                let frame = editor.current_frame();
                let report = editor
                    .dispatch_editor_actions(EditorActionBatch::new(
                        frame.revision(),
                        [EditorAction::Navigate(
                            EditorNavigationAction::SelectEntity {
                                document: second,
                                entity: Some(selected.clone()),
                            },
                        )],
                    ))
                    .unwrap();
                assert!(matches!(
                    report.outcomes(),
                    [EditorActionOutcome::Workspace(report)] if report.applied
                ));
                assert_eq!(
                    editor
                        .workspace
                        .scene(second)
                        .unwrap()
                        .selection()
                        .top_entity(),
                    Some(&selected)
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "captured Image did not render: {:?}",
                editor.diagnostics()
            );
            thread::yield_now();
        }
        assert_eq!(editor.workspace.document_revision(second), Some(revision));
        assert_eq!(dirty_count(editor), 0);
        assert!(
            editor
                .authoring_project()
                .unwrap()
                .asset_database()
                .is_empty()
        );
    });
    assert_eq!(fs::read(fixture.root.join(SECOND)).unwrap(), original);
}

#[test]
fn unsaved_open_leaf_edit_and_undo_republish_without_editing_or_saving_the_root_scene() {
    let fixture = Fixture::new();
    let disk_leaf = fs::read(fixture.root.join(LEAF)).unwrap();
    let disk_scene = fs::read(fixture.root.join(SECOND)).unwrap();
    fixture.run(|editor| {
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let disk_preview = wait_preview(editor, second, None);
        let second_revision = editor.workspace.document_revision(second).unwrap();
        let prefab = open_document(editor, LEAF, AuthoringDocumentKind::Prefab);
        assert_eq!(editor.scene_preview_document, Some(second));
        assert!(editor.current_editor_preview_content_stamp().is_none());
        let opened = wait_preview(editor, second, Some(&disk_preview.content_stamp()));
        let clean_revision = editor.workspace.document_revision(prefab).unwrap();
        let clean_epoch = editor
            .authoring_runtime
            .preview_world_snapshot()
            .unwrap()
            .domain_id;
        edit_rotation(editor, prefab, 0.75);
        let edited_revision = editor.workspace.document_revision(prefab).unwrap();
        assert_eq!(edited_revision.source_id(), clean_revision.source_id());
        assert_ne!(edited_revision, clean_revision);
        assert!(editor.current_editor_preview_content_stamp().is_none());
        assert_eq!(
            editor.workspace.document_revision(second),
            Some(second_revision)
        );
        assert_eq!(editor.workspace.document_is_dirty(second), Some(false));
        assert_eq!(editor.workspace.document_is_dirty(prefab), Some(true));
        assert_eq!(dirty_count(editor), 1);
        let edited = wait_preview(editor, second, Some(&opened.content_stamp()));
        assert_native_leaf(&fixture, editor, 0.75);
        assert_ne!(
            editor
                .authoring_runtime
                .preview_world_snapshot()
                .unwrap()
                .domain_id,
            clean_epoch
        );
        assert_eq!(
            editor.current_frame().scene_preview_unsaved_dependencies(),
            1
        );
        assert_eq!(fs::read(fixture.root.join(LEAF)).unwrap(), disk_leaf);
        let undo = editor.apply_workspace_command(EditorWorkspaceCommand::Undo {
            document: prefab,
            base_revision: edited_revision,
        });
        assert!(undo.applied, "{:?}", undo.diagnostics);
        assert!(editor.current_editor_preview_content_stamp().is_none());
        let restored = wait_preview(editor, second, Some(&edited.content_stamp()));
        assert_ne!(restored.content_stamp(), opened.content_stamp());
        assert_native_leaf(&fixture, editor, INITIAL_ROTATION);
        assert_eq!(
            editor.workspace.document_revision(second),
            Some(second_revision)
        );
        assert_eq!(dirty_count(editor), 0);
        assert_eq!(
            editor.current_frame().scene_preview_unsaved_dependencies(),
            0
        );
    });
    assert_eq!(fs::read(fixture.root.join(LEAF)).unwrap(), disk_leaf);
    assert_eq!(fs::read(fixture.root.join(SECOND)).unwrap(), disk_scene);
}

struct CompletionGate {
    started: mpsc::Receiver<()>,
    release: Option<mpsc::Sender<()>>,
}

impl CompletionGate {
    fn hold_next(editor: &mut EditorProjectSession) -> Self {
        let (started_tx, started) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        assert!(editor.scene_capture.next_completion_gate.is_none());
        editor.scene_capture.next_completion_gate = Some(Box::new(move || {
            let _ = started_tx.send(());
            release_rx
                .recv_timeout(WAIT)
                .expect("release the completed Scene capture");
        }));
        Self {
            started,
            release: Some(release),
        }
    }

    fn wait(&self) {
        self.started
            .recv_timeout(WAIT)
            .expect("the real Scene capture must finish preparation");
    }

    fn release(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

impl Drop for CompletionGate {
    fn drop(&mut self) {
        self.release();
    }
}

#[test]
fn completed_capture_cannot_authorize_a_newer_open_dependency_revision() {
    let fixture = Fixture::new();
    let disk_leaf = fs::read(fixture.root.join(LEAF)).unwrap();
    fixture.run(|editor| {
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let prefab = open_document(editor, LEAF, AuthoringDocumentKind::Prefab);
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let baseline = wait_preview(editor, second, None);
        let root_revision = editor.workspace.document_revision(second).unwrap();
        let last_good = editor
            .authoring_runtime
            .preview_world_snapshot()
            .unwrap()
            .clone();
        let mut gate = CompletionGate::hold_next(editor);
        edit_rotation(editor, prefab, 0.75);
        editor.drive_editor_frame(Duration::ZERO);
        gate.wait();
        assert!(editor.scene_capture.has_pending_work());
        edit_rotation(editor, prefab, 1.25);
        assert!(editor.current_editor_preview_content_stamp().is_none());
        editor.drive_editor_frame(Duration::ZERO);
        let held_generation = editor.scene_capture.generation;
        for _ in 0..6 {
            let frame = editor.drive_editor_frame(Duration::ZERO);
            assert!(frame.scene_preview_work_pending());
            assert!(editor.current_editor_preview_content_stamp().is_none());
            assert!(editor.take_editor_preview_candidate().is_none());
            assert_eq!(editor.scene_capture.generation, held_generation);
            assert_eq!(
                editor.authoring_runtime.preview_world_snapshot(),
                Some(&last_good)
            );
        }
        gate.release();
        let deadline = Instant::now() + WAIT;
        loop {
            editor.drive_editor_frame(Duration::ZERO);
            if let Some(candidate) = editor.take_editor_preview_candidate() {
                assert_eq!(
                    editor.current_editor_preview_content_stamp().as_ref(),
                    Some(&candidate.content_stamp()),
                );
                assert_ne!(candidate.content_stamp(), baseline.content_stamp());
                assert_native_leaf(&fixture, editor, 1.25);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "replacement capture did not return"
            );
            thread::yield_now();
        }
        assert_eq!(
            editor.workspace.document_revision(second),
            Some(root_revision)
        );
        assert_eq!(
            editor.current_frame().scene_preview_unsaved_dependencies(),
            1
        );
        assert_eq!(dirty_count(editor), 1);
    });
    assert_eq!(fs::read(fixture.root.join(LEAF)).unwrap(), disk_leaf);
}

#[test]
fn completed_capture_revalidates_disk_dependencies_before_the_first_materialization() {
    let fixture = Fixture::new();
    let disk_scene = fs::read(fixture.root.join(SECOND)).unwrap();
    fixture.run(|editor| {
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let root_revision = editor.workspace.document_revision(second).unwrap();
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let baseline = wait_preview(editor, second, None);
        let baseline_content = baseline.content_stamp();
        let last_good = editor.authoring_runtime.preview_world_snapshot().unwrap().clone();
        assert_native_leaf(&fixture, editor, INITIAL_ROTATION);
        assert!(
            editor.prefab_lanes.values().all(|lane| {
                lane.source_key != fixture.key(LEAF, SourceDocumentKind::Prefab)
            }),
            "the changed dependency must be sourced from disk",
        );

        let mut gate = CompletionGate::hold_next(editor);
        fs::write(
            fixture.root.join(LEAF),
            leaf(0.75).to_json_string().unwrap(),
        )
        .unwrap();
        editor.scene_capture.validate_at = Some(Instant::now());
        let deadline = Instant::now() + WAIT;
        loop {
            editor.drive_editor_frame(Duration::ZERO);
            if let Some(candidate) = editor.take_editor_preview_candidate() {
                assert_eq!(candidate.content_stamp(), baseline_content);
                assert_native_leaf(&fixture, editor, INITIAL_ROTATION);
            }
            match gate.started.try_recv() {
                Ok(()) => break,
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("the actual replacement capture did not reach its completion gate");
                }
            }
            assert!(Instant::now() < deadline, "disk-triggered capture did not prepare");
            thread::yield_now();
        }

        // The completed candidate contains 0.75, but its file receipt is about to become stale.
        fs::write(
            fixture.root.join(LEAF),
            leaf(1.25).to_json_string().unwrap(),
        )
        .unwrap();
        assert!(editor.current_editor_preview_content_stamp().is_none());
        assert!(editor.take_editor_preview_candidate().is_none());
        assert_eq!(editor.workspace.document_revision(second), Some(root_revision));
        assert_eq!(editor.authoring_runtime.preview_world_snapshot(), Some(&last_good));
        gate.release();

        let deadline = Instant::now() + WAIT;
        loop {
            editor.drive_editor_frame(Duration::ZERO);
            let current = editor.current_editor_preview_content_stamp();
            if current.is_some() {
                assert_native_leaf(&fixture, editor, 1.25);
            } else {
                assert_eq!(
                    editor.authoring_runtime.preview_world_snapshot(),
                    Some(&last_good),
                    "the obsolete completed candidate must never materialize, even without a valid stamp",
                );
            }
            if let Some(candidate) = editor.take_editor_preview_candidate() {
                assert_eq!(current.as_ref(), Some(&candidate.content_stamp()));
                assert_ne!(candidate.content_stamp(), baseline_content);
                assert_native_leaf(&fixture, editor, 1.25);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "a fresh capture of the changed disk dependency did not publish",
            );
            thread::yield_now();
        }
        assert_eq!(editor.workspace.document_revision(second), Some(root_revision));
        assert_eq!(dirty_count(editor), 0);
        assert_eq!(editor.current_frame().scene_preview_unsaved_dependencies(), 0);
        assert_eq!(fs::read(fixture.root.join(SECOND)).unwrap(), disk_scene);
    });
}

fn settle_persistence(editor: &mut EditorProjectSession) -> EditorPersistenceResult {
    let deadline = Instant::now() + WAIT;
    loop {
        editor.advance_persistence();
        if let Some(result) = editor.persistence_view().result() {
            return result;
        }
        assert!(
            Instant::now() < deadline,
            "actual file persistence did not return"
        );
        thread::yield_now();
    }
}

fn acknowledge_persistence(editor: &mut EditorProjectSession) {
    let attempt = editor.persistence_view().attempt().unwrap();
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }),
        EditorPersistenceRequestResult::Acknowledged { .. },
    ));
}

#[test]
fn saved_dependency_uses_its_current_file_receipt_and_reopen_changes_its_lineage() {
    let fixture = Fixture::new();
    let disk_scene = fs::read(fixture.root.join(SECOND)).unwrap();
    fixture.run(|editor| {
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let prefab = open_document(editor, LEAF, AuthoringDocumentKind::Prefab);
        let root_revision = editor.workspace.document_revision(second).unwrap();
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let initial = wait_preview(editor, second, None);
        edit_rotation(editor, prefab, 0.75);
        let edited = wait_preview(editor, second, Some(&initial.content_stamp()));
        let saved_revision = editor.workspace.document_revision(prefab).unwrap();
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(prefab)
            }),
            EditorPersistenceRequestResult::Accepted { .. },
        ));
        assert!(matches!(
            settle_persistence(editor),
            EditorPersistenceResult::Saved { document, revision, .. }
                if document == prefab && revision == saved_revision,
        ));
        let saved_bytes = fs::read(fixture.root.join(LEAF)).unwrap();
        let saved = PrefabDocumentCandidate::decode_json_bytes(&saved_bytes)
            .expect("Save must write a complete Prefab source");
        assert_eq!(
            saved.to_json_string().unwrap(),
            editor
                .workspace
                .prefab(prefab)
                .unwrap()
                .session()
                .document()
                .to_json_string()
                .unwrap(),
        );
        assert_eq!(editor.workspace.document_is_dirty(prefab), Some(false));
        acknowledge_persistence(editor);
        let after_save = wait_preview(editor, second, Some(&edited.content_stamp()));
        assert_native_leaf(&fixture, editor, 0.75);
        assert_eq!(
            editor.current_frame().scene_preview_unsaved_dependencies(),
            0
        );

        edit_rotation(editor, prefab, 1.25);
        assert!(editor.current_editor_preview_content_stamp().is_none());
        let unsaved = wait_preview(editor, second, Some(&after_save.content_stamp()));
        assert_native_leaf(&fixture, editor, 1.25);
        assert_eq!(fs::read(fixture.root.join(LEAF)).unwrap(), saved_bytes);
        assert_eq!(dirty_count(editor), 1);
        assert_eq!(
            editor.current_frame().scene_preview_unsaved_dependencies(),
            1
        );
        let before_reopen = editor.workspace.document_revision(prefab).unwrap();
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Reopen {
                document: Some(prefab)
            }),
            EditorPersistenceRequestResult::Accepted { .. },
        ));
        assert!(matches!(
            settle_persistence(editor),
            EditorPersistenceResult::Opened { document, .. } if document == prefab,
        ));
        let reopened_revision = editor.workspace.document_revision(prefab).unwrap();
        assert_ne!(reopened_revision.source_id(), before_reopen.source_id());
        assert!(editor.current_editor_preview_content_stamp().is_none());
        acknowledge_persistence(editor);
        let reopened = wait_preview(editor, second, Some(&unsaved.content_stamp()));
        assert_ne!(reopened.content_stamp(), after_save.content_stamp());
        assert_native_leaf(&fixture, editor, 0.75);
        assert_eq!(
            editor.workspace.document_revision(second),
            Some(root_revision)
        );
        assert_eq!(dirty_count(editor), 0);
        assert_eq!(
            editor.current_frame().scene_preview_unsaved_dependencies(),
            0
        );
        assert_eq!(fs::read(fixture.root.join(LEAF)).unwrap(), saved_bytes);
        assert_eq!(fs::read(fixture.root.join(SECOND)).unwrap(), disk_scene);
    });
}

#[test]
fn periodic_source_validation_rebuilds_changed_disk_dependency_with_the_same_root_revision() {
    let fixture = Fixture::new();
    let disk_scene = fs::read(fixture.root.join(SECOND)).unwrap();
    fixture.run(|editor| {
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let root_revision = editor.workspace.document_revision(second).unwrap();
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let original = wait_preview(editor, second, None);
        let original_epoch = editor
            .authoring_runtime
            .preview_world_snapshot()
            .unwrap()
            .domain_id;
        fs::write(
            fixture.root.join(LEAF),
            leaf(1.25).to_json_string().unwrap(),
        )
        .unwrap();
        editor.scene_capture.validate_at = Some(Instant::now());
        let changed = wait_preview(editor, second, Some(&original.content_stamp()));
        assert_ne!(changed.content_stamp(), original.content_stamp());
        assert_native_leaf(&fixture, editor, 1.25);
        assert_ne!(
            editor
                .authoring_runtime
                .preview_world_snapshot()
                .unwrap()
                .domain_id,
            original_epoch
        );
        assert_eq!(
            editor.workspace.document_revision(second),
            Some(root_revision)
        );
        assert_eq!(dirty_count(editor), 0);
        assert_eq!(
            editor.current_frame().scene_preview_unsaved_dependencies(),
            0
        );
        assert_eq!(fs::read(fixture.root.join(SECOND)).unwrap(), disk_scene);
    });
}

#[test]
fn unrelated_open_prefab_edits_keep_the_actual_dependency_stamp_and_projection_epoch() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let initial = wait_preview(editor, second, None);
        let content = initial.content_stamp();
        let epoch = editor
            .authoring_runtime
            .preview_world_snapshot()
            .unwrap()
            .domain_id;
        let unrelated = open_document(
            editor,
            "prefabs/unrelated.prefab.json",
            AuthoringDocumentKind::Prefab,
        );
        assert_eq!(
            editor.current_editor_preview_content_stamp().as_ref(),
            Some(&content)
        );
        edit_rotation(editor, unrelated, 1.25);
        assert_eq!(
            editor.current_editor_preview_content_stamp().as_ref(),
            Some(&content)
        );
        let unchanged = wait_preview(editor, second, None);
        assert_eq!(unchanged.content_stamp(), content);
        assert_eq!(
            editor
                .authoring_runtime
                .preview_world_snapshot()
                .unwrap()
                .domain_id,
            epoch
        );
        assert_native_leaf(&fixture, editor, INITIAL_ROTATION);
        assert_eq!(editor.workspace.document_is_dirty(unrelated), Some(true));
        assert_eq!(editor.workspace.document_is_dirty(second), Some(false));
        assert_eq!(dirty_count(editor), 1);
        assert_eq!(
            editor.current_frame().scene_preview_unsaved_dependencies(),
            0
        );
    });
}

#[test]
fn closing_and_opening_a_selected_dependency_rebinds_the_source_without_changing_the_root() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let second = open_document(editor, SECOND, AuthoringDocumentKind::Scene);
        let prefab = open_document(editor, LEAF, AuthoringDocumentKind::Prefab);
        let root_revision = editor.workspace.document_revision(second).unwrap();
        let original_revision = editor.workspace.document_revision(prefab).unwrap();
        editor.configure_editor_preview(Extent2d::new(320, 180));
        let opened = wait_preview(editor, second, None);
        assert_eq!(
            editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument {
                document: prefab
            }),
            EditorWorkspaceIntentRequestResult::Accepted,
        );
        let deadline = Instant::now() + WAIT;
        while editor.workspace.document_kind(prefab).is_some() {
            editor.advance_workspace_retirement();
            assert!(
                Instant::now() < deadline,
                "the selected dependency did not close"
            );
            thread::yield_now();
        }
        assert_eq!(
            editor.workspace_intent_result,
            Some(EditorWorkspaceIntentResult::Applied {
                intent: EditorWorkspaceIntent::CloseDocument { document: prefab },
            }),
        );
        assert!(editor.acknowledge_workspace_intent_result());
        assert!(editor.current_editor_preview_content_stamp().is_none());
        let disk = wait_preview(editor, second, Some(&opened.content_stamp()));
        assert_native_leaf(&fixture, editor, INITIAL_ROTATION);
        let new_document = open_document(editor, LEAF, AuthoringDocumentKind::Prefab);
        let replacement_revision = editor.workspace.document_revision(new_document).unwrap();
        assert_ne!(new_document, prefab);
        assert_ne!(
            replacement_revision.source_id(),
            original_revision.source_id()
        );
        assert!(editor.current_editor_preview_content_stamp().is_none());
        let reopened = wait_preview(editor, second, Some(&disk.content_stamp()));
        assert_ne!(reopened.content_stamp(), opened.content_stamp());
        assert_native_leaf(&fixture, editor, INITIAL_ROTATION);
        assert_eq!(
            editor.workspace.document_revision(second),
            Some(root_revision)
        );
        assert_eq!(dirty_count(editor), 0);
    });
}
