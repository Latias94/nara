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

use nara::{
    fs::ContentDigest,
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{AuthoringScalarEdit, SceneEntityId},
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

#[test]
fn ordinary_generic_open_preserves_degraded_source_through_reopen_and_recovery() {
    let project = GuardedGenericProject::new();
    let original_bundle = fs::read(project.bundle()).unwrap();
    let original_scene = fs::read(project.scene()).unwrap();
    let original_pulse = component_record(&project.scene(), SIGNAL_ENTITY, PULSE_COMPONENT);
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
            EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
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

    let scene = selected.workspace().active_scene.as_ref().unwrap();
    let edited = dispatch(
        &mut application,
        EditorAction::Mutate(EditorDocumentMutation::ApplyAuthoringScalarEdit {
            document: scene.document,
            base_revision: scene.revision,
            edit: AuthoringScalarEdit::new(
                SceneEntityId::new(SIGNAL_ENTITY).unwrap(),
                ComponentTypeId::new(NAME_COMPONENT),
                ComponentFieldId::new("value"),
                ComponentValue::String(EDITED_NAME.to_owned()),
            ),
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
        EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
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
    let reopened = dispatch(
        &mut application,
        EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target: ready_to_reopen.reopen_target(),
        }),
    );
    let reopened_document = match reopened.persistence().result() {
        Some(EditorPersistenceResult::Opened { document, .. }) => document,
        other => panic!("expected the clean-room Scene to reopen, got {other:?}"),
    };
    assert_ne!(reopened_document, original_document);
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
                EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
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
    assert!(rejected.request_exit().exit_ready());
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
    assert!(update.actions_observed());
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
    for _ in 0..8 {
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
        fs::create_dir_all(root.join("catalogs")).unwrap();
        fs::create_dir_all(root.join("worlds")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        for relative in [
            "nara.toml",
            "catalogs/authoring.bundle.json",
            "worlds/entry.scene.json",
        ] {
            fs::copy(source.join(relative), root.join(relative)).unwrap();
        }
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"must-not-build\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "compile_error!(\"the generic Editor must not compile project Rust\");\n",
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
