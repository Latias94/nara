#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

#[cfg(windows)]
use nara::tooling::{
    EditorDocumentKind, EditorDocumentOpenResult, EditorProjectBrowserAction,
    EditorProjectBrowserStatus, EditorProjectSourceInspectReason, EditorProjectSourceKind,
    EditorProjectSourceOpenCapability,
};
use nara::{
    fs::ContentDigest,
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{AuthoringFieldEdit, SceneEntityId},
    tooling::{
        EditorAction, EditorActionBatch, EditorAuthoringReadiness, EditorDocumentMutation,
        EditorNavigationAction, EditorPersistenceAction, EditorPersistenceResult, EditorPlayAction,
        EditorPlayOperation, EditorPlayOperationResult, EditorPlayRejection, EditorPlayState,
        EditorProjectContentView, EditorRuntimeAvailability, EditorRuntimeUnavailableReason,
        EditorWorkspaceIntent, EditorWorkspaceIntentAction, SceneInspectorReadOnlyReason,
    },
};
use nara_editor::{EditorApplication, EditorApplicationState, EditorLaunchOptions};
use serde_json::Value;

const SIGNAL_ENTITY: &str = "signal-node";
const NAME_COMPONENT: &str = "nara.scene.Name";
const PULSE_COMPONENT: &str = "studio.signal.Pulse";
const EDITED_NAME: &str = "Edited without compiling project Rust";
static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

#[cfg(windows)]
#[test]
fn standalone_project_browser_opens_external_documents_and_retains_last_good_refresh() {
    let project = GuardedGenericProject::new();
    assert!(
        fs::read_to_string(project.unbuildable_source())
            .unwrap()
            .contains("compile_error!")
    );

    let mut application = open_ordinary(project.root());
    let ready = drive_browser_to_settled(&mut application);
    let initial = ready.project_browser().snapshot().unwrap();
    assert_browser_entry(
        initial,
        "worlds/external.scene.json",
        EditorProjectSourceKind::Scene,
    );
    assert_browser_entry(
        initial,
        "blueprints/external.prefab.json",
        EditorProjectSourceKind::Prefab,
    );
    assert_browser_entry(
        initial,
        "media/icon.png",
        EditorProjectSourceKind::AssetSource,
    );
    assert_browser_entry(
        initial,
        "media/icon.png.meta",
        EditorProjectSourceKind::AssetMeta,
    );
    // Local cooperative browsing admits hard links without granting document or import authority.
    for path in ["media/linked.bin", "media/linked-copy.bin"] {
        assert_browser_entry(initial, path, EditorProjectSourceKind::AssetSource);
    }
    for path in [
        "media/icon.png",
        "media/icon.png.meta",
        "media/linked.bin",
        "media/linked-copy.bin",
    ] {
        assert_eq!(
            initial.entry(path).unwrap().open_capability(),
            EditorProjectSourceOpenCapability::InspectOnly(
                EditorProjectSourceInspectReason::ReadOnlySource
            )
        );
    }
    assert!(!initial.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .relative_source()
            .is_some_and(|path| path.starts_with("media/linked"))
    }));
    assert_browser_entry(initial, "scripts/tool.rs", EditorProjectSourceKind::Script);
    assert_browser_entry(
        initial,
        "worlds/notes.txt",
        EditorProjectSourceKind::Unknown,
    );
    assert!(initial.entry(".git/ignored.scene.json").is_none());
    assert!(initial.entry("target/ignored.prefab.json").is_none());

    let requested_scene = dispatch(
        &mut application,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
            relative_path: "worlds/external.scene.json".into(),
        }),
    );
    let opened_scene = wait_document_open(&mut application, requested_scene);
    let external_scene_document = opened_scene
        .workspace()
        .documents
        .iter()
        .find(|document| {
            document.kind == EditorDocumentKind::Scene
                && document
                    .source_binding
                    .as_ref()
                    .is_some_and(|binding| binding.locator() == "worlds/external.scene.json")
        })
        .expect("the browser must publish the closure-external Scene")
        .document;
    assert_eq!(
        opened_scene.workspace().active_document,
        Some(external_scene_document)
    );

    let requested_prefab = dispatch(
        &mut application,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Open {
            relative_path: "blueprints/external.prefab.json".into(),
        }),
    );
    let opened_prefab = wait_document_open(&mut application, requested_prefab);
    let external_scene = opened_prefab
        .workspace()
        .documents
        .iter()
        .find(|document| {
            document.kind == EditorDocumentKind::Scene
                && document
                    .source_binding
                    .as_ref()
                    .is_some_and(|binding| binding.locator() == "worlds/external.scene.json")
        })
        .expect("opening the Prefab must retain the independent Scene lane");
    let external_prefab_document = opened_prefab
        .workspace()
        .documents
        .iter()
        .find(|document| {
            document.kind == EditorDocumentKind::Prefab
                && document
                    .source_binding
                    .as_ref()
                    .is_some_and(|binding| binding.locator() == "blueprints/external.prefab.json")
        })
        .expect("the browser must publish the closure-external Prefab")
        .document;
    assert!(!external_scene.active);
    assert_eq!(
        opened_prefab.workspace().active_document,
        Some(external_prefab_document)
    );

    let first_snapshot = opened_prefab.project_browser().snapshot().unwrap();
    let first_status = opened_prefab.project_browser().status();
    let first_generation = first_snapshot.generation();
    let first_entries = first_snapshot.entries();
    let first_diagnostics = first_snapshot.diagnostics();
    let first_usage = first_snapshot.usage();
    let refresh_requested = dispatch(
        &mut application,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Refresh),
    );
    assert_eq!(
        refresh_requested
            .project_browser()
            .snapshot()
            .unwrap()
            .generation(),
        first_generation
    );
    let refreshed = drive_browser_to_settled(&mut application);
    let refreshed_snapshot = refreshed.project_browser().snapshot().unwrap();
    assert!(refreshed_snapshot.generation() > first_generation);
    assert_eq!(refreshed.project_browser().status(), first_status);
    assert_eq!(refreshed_snapshot.entries(), first_entries);
    assert_eq!(refreshed_snapshot.diagnostics(), first_diagnostics);
    assert_eq!(refreshed_snapshot.usage(), first_usage);

    let retained_generation = refreshed_snapshot.generation();
    let retained_entries = refreshed_snapshot.entries();
    let retained_diagnostics = refreshed_snapshot.diagnostics();
    let retained_usage = refreshed_snapshot.usage();
    let manifest_path = project.root().join("nara.toml");
    let mut changed_manifest = fs::read(&manifest_path).unwrap();
    changed_manifest.extend_from_slice(b"\n# Changed after the project authority was admitted.\n");
    fs::write(&manifest_path, changed_manifest).unwrap();
    let _ = dispatch(
        &mut application,
        EditorAction::ProjectBrowser(EditorProjectBrowserAction::Refresh),
    );
    let failed = drive_browser_to_idle(&mut application);
    assert_eq!(
        failed.project_browser().status(),
        EditorProjectBrowserStatus::Unavailable
    );
    let retained = failed.project_browser().snapshot().unwrap();
    assert_eq!(retained.generation(), retained_generation);
    assert_eq!(retained.entries(), retained_entries);
    assert_eq!(retained.diagnostics(), retained_diagnostics);
    assert_eq!(retained.usage(), retained_usage);
    let diagnostic = failed
        .project_browser()
        .failure()
        .expect("the failed refresh must publish one safe diagnostic");
    assert_eq!(
        diagnostic.code(),
        "project.source-index.manifest-lineage-changed"
    );
    let root = project.root().display().to_string();
    for text in [
        diagnostic.code(),
        diagnostic.summary(),
        diagnostic.impact(),
        diagnostic.repair_hint(),
        diagnostic.relative_source().unwrap_or_default(),
    ] {
        assert!(!text.contains(&root));
        assert!(!text.contains("FileIdentity"));
    }
    assert!(
        failed
            .workspace()
            .document(external_scene_document)
            .is_some()
    );
    assert!(
        failed
            .workspace()
            .document(external_prefab_document)
            .is_some()
    );
}

#[test]
fn ordinary_generic_open_preserves_degraded_source_through_reopen_and_recovery() {
    let project = GuardedGenericProject::new();
    let original_bundle = fs::read(project.bundle()).unwrap();
    let original_scene = fs::read(project.scene()).unwrap();
    let original_pulse = component_record(&project.scene(), SIGNAL_ENTITY, PULSE_COMPONENT);
    assert_eq!(
        original_pulse["value"]["value"]["amplitude"],
        serde_json::json!({
            "type": "map",
            "value": { "peak": { "type": "f64", "value": 0.75 } }
        }),
        "the degraded fixture must require unsupported structured-field semantics"
    );
    assert!(
        fs::read_to_string(project.unbuildable_source())
            .unwrap()
            .contains("compile_error!")
    );

    let mut application = open_ordinary(project.root());
    assert_eq!(
        application.state(),
        EditorApplicationState::AuthoringDegraded
    );
    let opened = current_frame(&application);
    assert_eq!(
        opened.authoring_readiness(),
        EditorAuthoringReadiness::Degraded
    );
    assert_eq!(
        opened.runtime_availability(),
        EditorRuntimeAvailability::Unavailable(
            EditorRuntimeUnavailableReason::SupervisedPlayContractMissing,
        )
    );
    assert_eq!(
        opened.project_content(),
        EditorProjectContentView::NotInstalled
    );
    assert!(has_diagnostic(&opened, "scene.authoring-known-unbound"));
    assert!(!has_diagnostic(&opened, "scene.authoring-unknown-schema"));
    let original_manifest = application
        .active_authoring_unavailable_record_manifest()
        .expect("the degraded Scene must publish its complete preservation manifest");
    assert_eq!(original_manifest.total_count(), 1);

    let selected = select_signal_node(&mut application);
    let selected_entity = selected
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap();
    let name = selected_entity
        .components
        .iter()
        .find(|component| component.component == ComponentTypeId::new(NAME_COMPONENT))
        .unwrap();
    let pulse = selected_entity
        .components
        .iter()
        .find(|component| component.component == ComponentTypeId::new(PULSE_COMPONENT))
        .unwrap();
    assert!(name.access.is_editable());
    assert_eq!(pulse.document_version, ComponentSchemaVersion::ONE);
    assert_eq!(pulse.schema_version, ComponentSchemaVersion::ONE);
    assert_eq!(
        pulse.access.read_only_reason(),
        Some(SceneInspectorReadOnlyReason::KnownUnbound)
    );
    assert!(pulse.fields.is_empty());

    let scene = selected.workspace().active_scene.as_ref().unwrap();
    let history = selected
        .workspace()
        .document(scene.document)
        .unwrap()
        .history;
    let rejected = dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::ApplyAuthoringFieldEdit {
            document: scene.document,
            base_revision: scene.revision,
            edit: AuthoringFieldEdit::single(
                SceneEntityId::new(SIGNAL_ENTITY).unwrap(),
                ComponentTypeId::new(PULSE_COMPONENT),
                ComponentSchemaVersion::ONE,
                ComponentFieldId::new("amplitude"),
                ComponentValue::map([("peak", ComponentValue::f64(1.0).unwrap())]),
            )
            .unwrap(),
        }),
    );
    let rejected_scene = rejected.workspace().active_scene.as_ref().unwrap();
    assert_eq!(rejected_scene.revision, scene.revision);
    assert_eq!(rejected_scene.dirty, scene.dirty);
    assert_eq!(
        rejected
            .workspace()
            .document(scene.document)
            .unwrap()
            .history,
        history
    );
    assert_eq!(fs::read(project.scene()).unwrap(), original_scene);
    assert_eq!(
        application.active_authoring_unavailable_record_manifest(),
        Some(original_manifest.clone())
    );
    let edited = dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::ApplyAuthoringFieldEdit {
            document: scene.document,
            base_revision: scene.revision,
            edit: AuthoringFieldEdit::single(
                SceneEntityId::new(SIGNAL_ENTITY).unwrap(),
                ComponentTypeId::new(NAME_COMPONENT),
                ComponentSchemaVersion::ONE,
                ComponentFieldId::new("value"),
                ComponentValue::String(EDITED_NAME.to_owned()),
            )
            .unwrap(),
        }),
    );
    let edited_scene = edited.workspace().active_scene.as_ref().unwrap();
    let original_document = edited_scene.document;
    let edited_revision = edited_scene.revision;
    assert!(edited_scene.dirty);
    assert_ne!(edited_revision, scene.revision);
    assert_eq!(fs::read(project.scene()).unwrap(), original_scene);

    let saved = dispatch(
        &mut application,
        EditorAction::Persistence(EditorPersistenceAction::Save {
            document: original_document,
            base_revision: edited_revision,
        }),
    );
    assert!(matches!(
        saved.persistence().result(),
        Some(EditorPersistenceResult::Saved {
            document,
            revision,
            ..
        }) if document == original_document && revision == edited_revision
    ));
    let saved_scene = saved.workspace().active_scene.as_ref().unwrap();
    assert!(!saved_scene.dirty);
    assert_eq!(saved_scene.saved_revision, edited_revision);
    assert_eq!(
        component_record(&project.scene(), SIGNAL_ENTITY, PULSE_COMPONENT),
        original_pulse
    );
    assert_eq!(
        component_value(&project.scene(), SIGNAL_ENTITY, NAME_COMPONENT),
        Value::String(EDITED_NAME.to_owned())
    );
    assert_eq!(
        application.active_authoring_unavailable_record_manifest(),
        Some(original_manifest.clone())
    );

    let _ = dispatch(
        &mut application,
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
            attempt: saved
                .persistence()
                .attempt()
                .expect("the observed persistence result is terminal"),
        }),
    );
    let closed = dispatch(
        &mut application,
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
            EditorWorkspaceIntent::CloseDocument {
                document: original_document,
            },
        )),
    );
    assert!(closed.workspace().active_document.is_none());
    let ready_to_reopen = dispatch(
        &mut application,
        EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::AcknowledgeResult),
    );
    assert_eq!(
        ready_to_reopen.reopen_target(),
        nara::tooling::EditorReopenTarget::Unavailable
    );
    drop(application);

    let mut application = open_ordinary(project.root());
    let reopened = current_frame(&application);
    let reopened_document = reopened.workspace().active_document.unwrap();
    let reopened_scene = reopened.workspace().active_scene.as_ref().unwrap();
    assert!(!reopened_scene.dirty);
    assert_eq!(reopened_scene.revision, reopened_scene.saved_revision);
    assert_eq!(
        component_record(&project.scene(), SIGNAL_ENTITY, PULSE_COMPONENT),
        original_pulse
    );
    assert_eq!(
        application.active_authoring_unavailable_record_manifest(),
        Some(original_manifest.clone())
    );

    let before_play_revision = reopened_scene.revision;
    let play = dispatch(
        &mut application,
        EditorAction::Play(EditorPlayAction::Play {
            document: reopened_document,
            base_revision: before_play_revision,
        }),
    );
    assert_eq!(
        application.state(),
        EditorApplicationState::AuthoringDegraded
    );
    assert_eq!(play.play().state(), EditorPlayState::Empty);
    assert_eq!(play.play().generation(), None);
    assert_eq!(
        play.play().result(),
        Some(EditorPlayOperationResult::Rejected {
            operation: EditorPlayOperation::Play,
            reason: EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::SupervisedPlayContractMissing,
            ),
        })
    );
    assert_eq!(
        play.project_content(),
        EditorProjectContentView::NotInstalled
    );
    assert_eq!(
        play.workspace().active_scene.as_ref().unwrap().revision,
        before_play_revision
    );
    let _ = select_signal_node(&mut application);
    let saved_bytes = fs::read(project.scene()).unwrap();
    drop(application);

    fs::remove_file(project.bundle()).unwrap();
    let mut recovery = open_ordinary(project.root());
    assert_eq!(recovery.state(), EditorApplicationState::Recovery);
    assert!(recovery.current_frame().is_none());
    assert_eq!(fs::read(project.scene()).unwrap(), saved_bytes);
    fs::write(project.bundle(), &original_bundle).unwrap();
    assert!(recovery.retry_project_open());
    assert_eq!(recovery.state(), EditorApplicationState::OpeningInitial);
    drive_open_to_terminal(&mut recovery);
    assert_eq!(recovery.state(), EditorApplicationState::AuthoringDegraded);
    assert_eq!(
        component_value(&project.scene(), SIGNAL_ENTITY, NAME_COMPONENT),
        Value::String(EDITED_NAME.to_owned())
    );
    assert_eq!(
        component_record(&project.scene(), SIGNAL_ENTITY, PULSE_COMPONENT),
        original_pulse
    );
    assert_eq!(
        recovery.active_authoring_unavailable_record_manifest(),
        Some(original_manifest)
    );
    drop(recovery);

    let mut hostile: Value = serde_json::from_slice(&saved_bytes).unwrap();
    *component_record_mut(&mut hostile, SIGNAL_ENTITY, PULSE_COMPONENT)
        .get_mut("value")
        .unwrap() = serde_json::json!({ "type": "future_value", "value": 7 });
    let hostile_bytes = serde_json::to_vec_pretty(&hostile).unwrap();
    fs::write(project.scene(), &hostile_bytes).unwrap();
    let hostile_digest = ContentDigest::of_bytes(&hostile_bytes);
    let mut rejected = open_ordinary(project.root());
    assert_eq!(rejected.state(), EditorApplicationState::Recovery);
    assert!(rejected.current_frame().is_none());
    let retained_hostile = fs::read(project.scene()).unwrap();
    assert_eq!(retained_hostile, hostile_bytes);
    assert_eq!(ContentDigest::of_bytes(&retained_hostile), hostile_digest);
    let mut exit_ready = rejected.request_exit().exit_ready();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !exit_ready {
        assert!(
            std::time::Instant::now() < deadline,
            "Recovery did not retire its Open Host"
        );
        exit_ready = rejected.advance_project_open().exit_ready();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(rejected.current_frame().is_none());
}

fn dispatch(
    application: &mut EditorApplication,
    action: EditorAction,
) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    let frame = current_frame(application);
    let update = application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [action]),
        Duration::ZERO,
    );
    assert!(update.update().actions_observed());
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while current_frame(application)
        .persistence()
        .operation()
        .is_some()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "persistence did not settle"
        );
        let frame = current_frame(application);
        application.submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    current_frame(application)
}

fn select_signal_node(
    application: &mut EditorApplication,
) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    let frame = current_frame(application);
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    dispatch(
        application,
        EditorAction::Navigate(EditorNavigationAction::SelectEntity {
            document: scene.document,
            entity: Some(SceneEntityId::new(SIGNAL_ENTITY).unwrap()),
        }),
    )
}

fn open_ordinary(project_root: &Path) -> EditorApplication {
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(project_root).with_profile("desktop"));
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    assert!(application.current_frame().is_none());
    let _ = application.native_shell_ready();
    drive_open_to_terminal(&mut application);
    application
}

fn drive_open_to_terminal(application: &mut EditorApplication) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        match application.state() {
            EditorApplicationState::AuthoringReady
            | EditorApplicationState::AuthoringDegraded
            | EditorApplicationState::Recovery => return,
            EditorApplicationState::OpeningInitial | EditorApplicationState::OpeningReplacement => {
                let _ = application.advance_project_open();
            }
            EditorApplicationState::ProjectManager => {
                panic!("the native shell must arm the project-open request")
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the bounded project-open state machine did not reach a terminal state")
}

fn current_frame(
    application: &EditorApplication,
) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    application
        .current_frame()
        .expect("a published authoring project owns one Editor frame")
}

#[cfg(windows)]
fn wait_document_open(
    application: &mut EditorApplication,
    requested: std::sync::Arc<nara::tooling::EditorFrameModel>,
) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    // A fast worker can already have completed in submit_and_drive's first frame.
    let request = requested
        .document_open()
        .pending()
        .map(|(request, _, _)| request)
        .or_else(|| {
            requested
                .document_open()
                .result()
                .map(|result| result.request())
        })
        .expect("Browser Open must expose its accepted request");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let frame = current_frame(application);
        if let Some(result) = frame.document_open().result() {
            assert_eq!(result.request(), request);
            assert!(!frame.document_open().has_pending_work());
            let EditorDocumentOpenResult::Opened { document, .. } = result else {
                panic!("Browser Open did not publish the requested document: {result:?}");
            };
            assert!(frame.workspace().document(document).is_some());
            let acknowledged = dispatch(
                application,
                EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
                    request,
                }),
            );
            assert!(acknowledged.document_open().result().is_none());
            assert!(!acknowledged.document_open().has_pending_work());
            return acknowledged;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Browser Open did not settle: {:?}",
            frame.document_open()
        );
        application.submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(windows)]
fn drive_browser_to_settled(
    application: &mut EditorApplication,
) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    let frame = drive_browser_to_idle(application);
    assert!(matches!(
        frame.project_browser().status(),
        EditorProjectBrowserStatus::Ready | EditorProjectBrowserStatus::ReadyWithDiagnostics
    ));
    frame
}

#[cfg(windows)]
fn drive_browser_to_idle(
    application: &mut EditorApplication,
) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let frame = current_frame(application);
        if !frame.project_browser().has_pending_work() {
            return frame;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the bounded Project Browser did not settle"
        );
        let _ = application
            .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(windows)]
fn assert_browser_entry(
    snapshot: &nara::tooling::EditorProjectBrowserSnapshot,
    relative_path: &str,
    kind: EditorProjectSourceKind,
) {
    assert_eq!(snapshot.entry(relative_path).unwrap().kind(), kind);
}

fn has_diagnostic(frame: &nara::tooling::EditorFrameModel, code: &str) -> bool {
    frame
        .diagnostics()
        .iter()
        .any(|entry| entry.code().as_str() == code)
}

fn component_record(path: &Path, entity: &str, component: &str) -> Value {
    let source: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    component_record_ref(&source, entity, component).clone()
}

fn component_value(path: &Path, entity: &str, component: &str) -> Value {
    component_record(path, entity, component)
        .get("value")
        .and_then(|value| value.get("value"))
        .cloned()
        .unwrap()
}

fn component_record_ref<'a>(source: &'a Value, entity: &str, component: &str) -> &'a Value {
    source["payload"]["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["id"] == entity)
        .unwrap()["components"]
        .get(component)
        .unwrap()
}

fn component_record_mut<'a>(source: &'a mut Value, entity: &str, component: &str) -> &'a mut Value {
    source["payload"]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|candidate| candidate["id"] == entity)
        .unwrap()["components"]
        .get_mut(component)
        .unwrap()
}

struct GuardedGenericProject {
    parent: PathBuf,
    root: PathBuf,
}

impl GuardedGenericProject {
    fn new() -> Self {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generic-editor/known-unbound");
        let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
        let parent = std::env::temp_dir().join(format!(
            "nara-editor-generic-open-{}-{sequence}",
            std::process::id()
        ));
        let root = parent.join("project");
        for directory in [
            ".git",
            "blueprints",
            "catalogs",
            "media",
            "scripts",
            "src",
            "target",
            "worlds",
        ] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        for relative in [
            "Cargo.toml",
            "nara.toml",
            "blueprints/external.prefab.json",
            "catalogs/authoring.bundle.json",
            "media/icon.png",
            "media/icon.png.meta",
            "scripts/tool.rs",
            "src/lib.rs",
            "worlds/entry.scene.json",
            "worlds/external.scene.json",
            "worlds/notes.txt",
        ] {
            fs::copy(source.join(relative), root.join(relative)).unwrap();
        }
        fs::write(root.join(".git/ignored.scene.json"), "{}\n").unwrap();
        fs::write(root.join("target/ignored.prefab.json"), "{}\n").unwrap();
        fs::write(root.join("media/linked.bin"), "linked\n").unwrap();
        fs::hard_link(
            root.join("media/linked.bin"),
            root.join("media/linked-copy.bin"),
        )
        .unwrap();
        Self { parent, root }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn bundle(&self) -> PathBuf {
        self.root.join("catalogs/authoring.bundle.json")
    }

    fn scene(&self) -> PathBuf {
        self.root.join("worlds/entry.scene.json")
    }

    fn unbuildable_source(&self) -> PathBuf {
        self.root.join("src/lib.rs")
    }
}

impl Drop for GuardedGenericProject {
    fn drop(&mut self) {
        let temporary_root = std::env::temp_dir().canonicalize().unwrap();
        let parent = self.parent.canonicalize().unwrap();
        assert!(parent.starts_with(&temporary_root));
        assert!(
            parent
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("nara-editor-generic-open-"))
        );
        fs::remove_dir_all(&parent).unwrap();
    }
}
