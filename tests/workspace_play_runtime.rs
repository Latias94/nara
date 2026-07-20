#![cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nara::{
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
        PluginError, PluginId, PluginShutdownObligationId, RuntimeCloseContext,
        RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
        RuntimeCloseProgress,
    },
    gameplay::GameplayCommandPlugin,
    project_host::{EditorProjectIntent, EditorProjectSession},
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
        ScenePatchOperation,
    },
    tooling::{
        EditorPlayCommand, EditorPlayOperation, EditorPlayOperationResult, EditorPlayRejection,
        EditorPlayRequestResult, EditorPlayState, EditorWorkspaceIntent,
        EditorWorkspaceIntentRequestResult,
    },
};
use project_content_fixture::TestProject;

const DELAYED_CLOSE_PLUGIN_ID: PluginId = PluginId::new("nara.test.editor-delayed-close");
const DELAYED_CLOSE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.editor-delayed-close", 1);
const DELAYED_CLOSE_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.editor-delayed-close");
const DELAYED_CLOSE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.editor-delayed-close");
const DELAYED_CLOSE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(DELAYED_CLOSE_PLUGIN_ID, PluginCategory::Service)
        .shutdown_obligations(&[DELAYED_CLOSE_OBLIGATION]);
const BUILD_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.editor-build-failure");
const BUILD_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.editor-build-failure", 1);
const BUILD_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(BUILD_FAILURE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[DELAYED_CLOSE_PLUGIN_ID]);

#[derive(Debug)]
struct DelayedClosePlugin {
    release: Arc<AtomicBool>,
}

impl Plugin for DelayedClosePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DELAYED_CLOSE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_plugin_runtime_close_participant(
            DELAYED_CLOSE_OBLIGATION,
            DELAYED_CLOSE_PARTICIPANT,
            DelayedCloseParticipant {
                release: Arc::clone(&self.release),
            },
        )?;
        Ok(())
    }
}

struct DelayedCloseParticipant {
    release: Arc<AtomicBool>,
}

impl RuntimeCloseParticipant for DelayedCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.progress())
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.progress())
    }
}

impl DelayedCloseParticipant {
    fn progress(&self) -> RuntimeCloseProgress {
        if self.release.load(Ordering::Acquire) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        }
    }
}

#[derive(Debug, Default)]
struct BuildFailurePlugin;

impl Plugin for BuildFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &BUILD_FAILURE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: BUILD_FAILURE_PLUGIN_ID,
            message: "editor Host failure probe".to_owned(),
        })
    }
}

fn delayed_close_definition(release: Arc<AtomicBool>) -> PluginDefinition {
    PluginDefinition::infallible::<DelayedClosePlugin, _>(
        DELAYED_CLOSE_DEFINITION_ID,
        b"editor-delayed-close-v1",
        move || DelayedClosePlugin {
            release: Arc::clone(&release),
        },
    )
}

fn build_failure_definition() -> PluginDefinition {
    PluginDefinition::infallible::<BuildFailurePlugin, _>(
        BUILD_FAILURE_DEFINITION_ID,
        b"editor-build-failure-v1",
        BuildFailurePlugin::default,
    )
}

#[test]
fn editor_host_owns_prepare_start_pause_step_stop_and_fresh_restart() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();

    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    assert_eq!(editor.play_view().state(), EditorPlayState::PreparingPlay);
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy)
    );
    assert_eq!(
        editor.request_play(EditorPlayCommand::Cancel),
        EditorPlayRequestResult::Accepted
    );
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert_eq!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Cancelled {
            operation: EditorPlayOperation::Play,
        })
    );

    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.play_view().state(),
        EditorPlayState::Starting,
        "diagnostics: {:?}",
        editor.diagnostics()
    );

    assert_eq!(
        editor.request_play(EditorPlayCommand::Cancel),
        EditorPlayRequestResult::Accepted
    );
    assert_eq!(editor.play_view().state(), EditorPlayState::RetiringPlay);
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert_eq!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Cancelled {
            operation: EditorPlayOperation::Play,
        })
    );

    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);
    let first_generation = editor.play_view().generation().unwrap();

    assert_eq!(
        editor.request_play(EditorPlayCommand::Pause),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.play_view().state(), EditorPlayState::Paused);
    assert!(matches!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::Pause,
            generation: Some(generation),
        }) if generation == first_generation
    ));

    assert_eq!(
        editor.request_play(EditorPlayCommand::StepFixedTick),
        EditorPlayRequestResult::Accepted
    );
    assert_eq!(editor.play_view().state(), EditorPlayState::Stepping);
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.play_view().state(), EditorPlayState::Paused);
    assert!(matches!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::StepFixedTick,
            generation: Some(generation),
        }) if generation == first_generation
    ));

    assert_eq!(
        editor.request_play(EditorPlayCommand::Restart),
        EditorPlayRequestResult::Accepted
    );
    assert_eq!(editor.play_view().state(), EditorPlayState::Stopping);
    drive_until(&mut editor, EditorPlayState::Running, 32);
    let second_generation = editor.play_view().generation().unwrap();
    assert_ne!(second_generation, first_generation);
    assert!(matches!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::Restart,
            generation: Some(generation),
        }) if generation == second_generation
    ));

    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Empty, 32);
    assert_eq!(editor.play_view().generation(), None);
    assert!(matches!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::Stop,
            ..
        })
    ));
}

#[test]
fn play_without_an_active_document_rejects_without_creating_an_owner() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
    let document = editor.workspace().active_document().unwrap();
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseScene { document }),
        EditorWorkspaceIntentRequestResult::Accepted
    );

    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Rejected(EditorPlayRejection::NoActiveDocument)
    );
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert_eq!(editor.play_view().generation(), None);
}

#[test]
fn runtime_edit_is_generation_and_safe_point_bound() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);

    let play = editor.play_view();
    assert_eq!(
        editor.request_runtime_edit_request(nara::tooling::EditorRuntimeEditRequest {
            generation: play.generation().unwrap().saturating_add(1),
            document_revision: play.current_revision().unwrap(),
            entity: SceneEntityId::new("enemy-anchor/enemy").unwrap(),
            component: ComponentTypeId::new("nara.sprite.Sprite"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("layer"),
            value: ComponentValue::I64(4),
        }),
        Err(nara::tooling::EditorRuntimeEditRejection::StaleGeneration)
    );

    editor
        .request_runtime_edit(
            SceneEntityId::new("enemy-anchor/enemy").unwrap(),
            ComponentTypeId::new("nara.sprite.Sprite"),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("layer"),
            ComponentValue::I64(4),
        )
        .unwrap();
    assert!(matches!(
        editor.runtime_edit_result(),
        Some(nara::tooling::EditorRuntimeEditResult::Pending(_))
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.runtime_edit_result(),
        Some(nara::tooling::EditorRuntimeEditResult::Applied(_))
    ));
    assert!(editor.acknowledge_runtime_edit_result());
    editor
        .request_runtime_edit(
            SceneEntityId::new("enemy-anchor/enemy").unwrap(),
            ComponentTypeId::new("nara.sprite.Sprite"),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("layer"),
            ComponentValue::I64(5),
        )
        .unwrap();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    assert!(matches!(
        editor.runtime_edit_result(),
        Some(nara::tooling::EditorRuntimeEditResult::Cancelled(_))
    ));
    drive_until(&mut editor, EditorPlayState::Empty, 32);
}

#[test]
fn apply_changes_exports_selected_runtime_value_and_marks_runtime_out_of_date() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let transform_id = ComponentTypeId::new("nara.transform.Transform2d");
    let entity_id = SceneEntityId::new("player").unwrap();
    project.write_scene_source(&SceneDocument::new([SceneEntityRecord::new(
        entity_id.clone(),
    )
    .with_component(
        transform_id.clone(),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                (
                    "translation",
                    ComponentValue::map([
                        ("x", ComponentValue::f64(0.0).unwrap()),
                        ("y", ComponentValue::f64(0.0).unwrap()),
                    ]),
                ),
                ("rotation", ComponentValue::f64(0.0).unwrap()),
                (
                    "scale",
                    ComponentValue::map([
                        ("x", ComponentValue::f64(1.0).unwrap()),
                        ("y", ComponentValue::f64(1.0).unwrap()),
                    ]),
                ),
            ]),
        ),
    )]));
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);

    editor
        .request_runtime_edit(
            entity_id.clone(),
            transform_id.clone(),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("translation.x"),
            ComponentValue::f64(42.0).unwrap(),
        )
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    assert!(editor.acknowledge_runtime_edit_result());

    let request =
        nara::tooling::SceneApplyChangesRequest::new(entity_id.clone(), [transform_id.clone()]);
    editor.request_apply_changes(request.clone()).unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    let Some(nara::tooling::EditorApplyChangesResult::Applied(report)) =
        editor.apply_changes_result()
    else {
        panic!(
            "expected an applied change, got {:?}",
            editor.apply_changes_result()
        );
    };
    assert!(report.applied);
    assert_eq!(report.components.len(), 1);
    assert!(editor.play_view().is_out_of_date());
    let document = editor
        .workspace()
        .active_scene()
        .unwrap()
        .session()
        .document();
    let value = &document.entities[0].components[&transform_id].value;
    assert_eq!(
        value
            .get_path(&nara::reflect::ComponentFieldPath::from_fields([
                "translation",
                "x",
            ]))
            .unwrap(),
        &ComponentValue::f64(42.0).unwrap()
    );
    assert_eq!(
        editor.request_apply_changes(request),
        Err(nara::tooling::EditorApplyChangesRejection::Busy)
    );
    assert!(editor.acknowledge_apply_changes_result());
    assert_eq!(
        editor.request_apply_changes(nara::tooling::SceneApplyChangesRequest::new(
            entity_id.clone(),
            [transform_id.clone()],
        )),
        Err(nara::tooling::EditorApplyChangesRejection::RuntimeOutOfDate)
    );

    let stale_generation = editor.play_view().generation().unwrap();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Restart),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);
    assert!(!editor.play_view().is_out_of_date());
    assert_ne!(editor.play_view().generation(), Some(stale_generation));

    let cancelled_request = nara::tooling::SceneApplyChangesRequest::new(entity_id, [transform_id]);
    editor
        .request_apply_changes(cancelled_request.clone())
        .unwrap();

    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    assert_eq!(
        editor.apply_changes_result(),
        Some(&nara::tooling::EditorApplyChangesResult::Cancelled(
            cancelled_request
        ))
    );
    drive_until(&mut editor, EditorPlayState::Empty, 32);
}

#[test]
fn workspace_close_retains_document_through_close_incomplete_and_retry() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let release = Arc::new(AtomicBool::new(false));
    let intent = EditorProjectIntent::new()
        .with_cleanup_timeout(Duration::ZERO)
        .insert_after::<GameplayCommandPlugin>(delayed_close_definition(Arc::clone(&release)));
    let mut editor = EditorProjectSession::open(project.root_capability(), intent).unwrap();
    let document = editor.workspace().active_document().unwrap();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);

    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseScene { document }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::CloseIncomplete, 32);
    assert!(editor.workspace().scene(document).is_some());
    assert!(editor.workspace_intent_view().intent().is_some());

    release.store(true, Ordering::Release);
    assert_eq!(
        editor.request_play(EditorPlayCommand::RetryClose),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Empty, 32);
    assert!(editor.workspace().is_empty());
    assert!(editor.workspace_intent_view().intent().is_none());
}

#[test]
fn apply_changes_rejects_prefab_expansion_and_stale_document_without_mutation() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
    let document = editor.workspace().active_document().unwrap();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);
    let original_revision = editor.workspace().scene(document).unwrap().revision();

    let expanded_request = nara::tooling::SceneApplyChangesRequest::new(
        SceneEntityId::new("enemy-anchor/enemy").unwrap(),
        [ComponentTypeId::new("nara.sprite.Sprite")],
    );
    editor
        .request_apply_changes(expanded_request.clone())
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.apply_changes_result(),
        Some(nara::tooling::EditorApplyChangesResult::Rejected {
            request,
            reason: nara::tooling::EditorApplyChangesRejection::Unsupported,
            ..
        }) if request == &expanded_request
    ));
    assert_eq!(
        editor.workspace().scene(document).unwrap().revision(),
        original_revision
    );
    assert!(editor.acknowledge_apply_changes_result());

    let stale_request = nara::tooling::SceneApplyChangesRequest::new(
        SceneEntityId::new("player").unwrap(),
        [ComponentTypeId::new("nara.sprite.Sprite")],
    );
    editor.request_apply_changes(stale_request).unwrap();
    assert!(
        editor
            .apply_workspace_command(nara::tooling::EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: SceneEntityRecord::new(SceneEntityId::new("later-edit").unwrap()),
                }]),
            })
            .applied
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.apply_changes_result(),
        Some(nara::tooling::EditorApplyChangesResult::Rejected {
            reason: nara::tooling::EditorApplyChangesRejection::StaleDocument,
            ..
        })
    ));

    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Empty, 32);
}

#[test]
fn failed_start_retains_retirement_owner_until_retry_completes() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let release = Arc::new(AtomicBool::new(false));
    let intent = EditorProjectIntent::new()
        .with_cleanup_timeout(Duration::ZERO)
        .insert_after::<GameplayCommandPlugin>(delayed_close_definition(Arc::clone(&release)))
        .insert_after::<DelayedClosePlugin>(build_failure_definition());
    let mut editor = EditorProjectSession::open(project.root_capability(), intent).unwrap();
    let document = editor.workspace().active_document().unwrap();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.play_view().state(), EditorPlayState::Starting);
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.play_view().state(), EditorPlayState::RetiringPlay);
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.play_view().state(),
        EditorPlayState::RetirementIncomplete
    );
    assert!(editor.workspace().scene(document).is_some());
    assert_eq!(editor.play_view().generation(), None);

    release.store(true, Ordering::Release);
    assert_eq!(
        editor.request_play(EditorPlayCommand::RetryRetirement),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Empty, 32);
    assert!(editor.workspace().scene(document).is_some());
    assert!(matches!(
        editor.play_view().result(),
        Some(EditorPlayOperationResult::Failed {
            operation: EditorPlayOperation::Play,
            ..
        })
    ));
}

fn drive_until(
    editor: &mut EditorProjectSession,
    expected: EditorPlayState,
    maximum_frames: usize,
) {
    for _ in 0..maximum_frames {
        if editor.play_view().state() == expected {
            return;
        }
        editor.drive_editor_frame(Duration::ZERO);
    }
    assert_eq!(
        editor.play_view().state(),
        expected,
        "diagnostics: {:?}",
        editor.diagnostics()
    );
}
