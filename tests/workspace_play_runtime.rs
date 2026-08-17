#![cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]

#[path = "support/editor_action_driver.rs"]
mod editor_action_driver;
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

use editor_action_driver as editor_driver;
use nara::{
    ProductConfiguration, ProductRecipe,
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginDefinition,
        PluginDefinitionId, PluginError, PluginId, PluginShutdownContext,
        PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
        RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
        StartupStage,
    },
    ecs::{Commands, Query, Res, Resource, schedule::IntoScheduleConfigs},
    project_host::{
        EditorGameViewBridgeFailure, EditorGameViewCompletion, EditorProjectIntent,
        EditorProjectSession,
        advanced::{RuntimeProbe, open_editor_with_probe},
    },
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    render::{Camera2d, Extent2d},
    scene::{
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, SceneEntitySource,
        ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorActionRejection,
        EditorDocumentMutation, EditorNavigationAction, EditorPersistenceAction, EditorPlayCommand,
        EditorPlayFailure, EditorPlayOperation, EditorPlayOperationResult, EditorPlayRejection,
        EditorPlayRequestResult, EditorPlayState, EditorWorkspaceIntent,
        EditorWorkspaceIntentRequestResult,
    },
    transform::{GlobalTransform2d, Transform2d},
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
const BUILD_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(BUILD_FAILURE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[DELAYED_CLOSE_PLUGIN_ID]);
const SHUTDOWN_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.editor-shutdown-failure");
const SHUTDOWN_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SHUTDOWN_FAILURE_PLUGIN_ID, PluginCategory::Runtime);
const RUNTIME_SERVICE_SESSION_PLUGIN_ID: PluginId =
    PluginId::new("nara.test.editor-runtime-service-session");
const RUNTIME_SERVICE_SESSION_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(RUNTIME_SERVICE_SESSION_PLUGIN_ID, PluginCategory::Service);
const SPATIAL_OBSERVATION_PLUGIN_ID: PluginId =
    PluginId::new("nara.test.editor-spatial-observation");
const SPATIAL_OBSERVATION_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.editor-spatial-observation", 1);
const SPATIAL_OBSERVATION_REQUIREMENTS: &[PluginId] = &[nara::transform::TRANSFORM_PLUGIN_ID];
const SPATIAL_OBSERVATION_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SPATIAL_OBSERVATION_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(SPATIAL_OBSERVATION_REQUIREMENTS);
const GAME_VIEW_CAMERA_PLUGIN_ID: PluginId = PluginId::new("nara.test.editor-game-view-camera");
const GAME_VIEW_CAMERA_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(GAME_VIEW_CAMERA_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(&[nara::render::RENDER_PLUGIN_ID]);

static NEXT_RUNTIME_SERVICE_SESSION_ID: AtomicU64 = AtomicU64::new(1);

struct RuntimeServiceSessionConfiguration(mpsc::SyncSender<u64>);

impl ProductConfiguration for RuntimeServiceSessionConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"editor-runtime-service-session-v1");
    }
}

#[derive(Debug)]
struct RuntimeServiceSessionPlugin {
    observations: mpsc::SyncSender<u64>,
}

impl Plugin for RuntimeServiceSessionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RUNTIME_SERVICE_SESSION_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(RuntimeServiceSessionObservations(self.observations.clone()))?
            .add_systems(StartupStage::Runtime, publish_runtime_service_session_id)?;
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct RuntimeServiceSessionObservations(mpsc::SyncSender<u64>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
struct RuntimeServiceSessionId {
    _id: u64,
}

fn publish_runtime_service_session_id(world: &mut nara::ecs::World) {
    let id = NEXT_RUNTIME_SERVICE_SESSION_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("test runtime service-session identities are exhausted");
    world
        .resource::<RuntimeServiceSessionObservations>()
        .0
        .try_send(id)
        .expect("the bounded test service-session observer remains available and within budget");
    world.insert_resource(RuntimeServiceSessionId { _id: id });
}

fn runtime_service_session_recipe(observations: mpsc::SyncSender<u64>) -> ProductRecipe {
    ProductRecipe::new()
        .add_configured_plugin(
            RuntimeServiceSessionConfiguration(observations),
            |configuration: &RuntimeServiceSessionConfiguration| RuntimeServiceSessionPlugin {
                observations: configuration.0.clone(),
            },
        )
        .unwrap()
}

#[derive(Debug)]
struct SpatialObservationPlugin {
    target: SceneEntityId,
    observations: mpsc::SyncSender<nara::prelude::Vec2>,
}

impl Plugin for SpatialObservationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &SPATIAL_OBSERVATION_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(SpatialObservation {
            target: self.target.clone(),
            observations: self.observations.clone(),
        })?
        .add_systems(
            CoreStage::Extract,
            observe_spatial_projection.after(nara::transform::__private::TransformSet::Propagate),
        )?;
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct SpatialObservation {
    target: SceneEntityId,
    observations: mpsc::SyncSender<nara::prelude::Vec2>,
}

fn observe_spatial_projection(
    observation: Res<SpatialObservation>,
    transforms: Query<(&SceneEntitySource, &GlobalTransform2d)>,
) {
    let global = transforms
        .iter()
        .find_map(|(source, global)| (source.entity_id == observation.target).then_some(global))
        .expect("the spatial probe target remains in the active scene");
    observation
        .observations
        .try_send(global.translation())
        .expect("the bounded spatial observation channel remains within its test budget");
}

fn spatial_observation_definition(
    target: SceneEntityId,
    observations: mpsc::SyncSender<nara::prelude::Vec2>,
) -> PluginDefinition {
    PluginDefinition::infallible::<SpatialObservationPlugin, _>(
        SPATIAL_OBSERVATION_DEFINITION_ID,
        b"editor-spatial-observation-v1",
        move || SpatialObservationPlugin {
            target: target.clone(),
            observations: observations.clone(),
        },
    )
}

#[derive(Debug, Default)]
struct GameViewCameraPlugin;

impl Plugin for GameViewCameraPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &GAME_VIEW_CAMERA_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Scene, spawn_game_view_camera)?;
        Ok(())
    }
}

fn spawn_game_view_camera(mut commands: Commands) {
    commands.spawn((Transform2d::default(), Camera2d::default()));
}

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

#[derive(Debug, Default)]
struct ShutdownFailurePlugin;

impl Plugin for ShutdownFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &SHUTDOWN_FAILURE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: SHUTDOWN_FAILURE_PLUGIN_ID,
            message: "editor shutdown failure probe".to_owned(),
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

#[test]
fn editor_frame_publishes_ordered_navigation_and_mutation_only_after_drive() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let revision = frame.workspace().scene(document).unwrap().revision;
    let selected = SceneEntityId::new("enemy-anchor").unwrap();
    let added = SceneEntityId::new("frame-edit").unwrap();

    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(SceneEntityId::new("missing-frame-entity").unwrap()),
                }),
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document,
                    entity: Some(selected.clone()),
                }),
                EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
                    document,
                    base_revision: revision,
                    patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                        entity: SceneEntityRecord::new(added.clone()),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                }),
            ],
        ))
        .unwrap();
    assert!(matches!(
        &report.outcomes()[0],
        EditorActionOutcome::Workspace(report) if !report.applied
    ));
    assert!(report.outcomes()[1..].iter().all(|outcome| {
        matches!(
            outcome,
            EditorActionOutcome::Workspace(report) if report.applied
        )
    }));
    assert!(Arc::ptr_eq(&frame, &editor.current_frame()));

    let next = editor.drive_editor_frame(Duration::ZERO);
    assert_ne!(next.revision(), frame.revision());
    let scene = next.workspace().active_scene.as_ref().unwrap();
    assert_eq!(scene.selection.top_entity(), Some(&selected));
    assert!(
        scene
            .editor
            .inspector
            .entities
            .iter()
            .any(|row| row.id == added)
    );
}

#[test]
fn editor_frame_rejects_a_second_document_mutation_without_undoing_the_first() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let revision = frame.workspace().scene(document).unwrap().revision;

    let patch = |id: &str| {
        ScenePatchDocument::new([ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(SceneEntityId::new(id).unwrap()),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        }])
    };
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
                    document,
                    base_revision: revision,
                    patch: patch("first-frame-edit"),
                }),
                EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
                    document,
                    base_revision: revision,
                    patch: patch("second-frame-edit"),
                }),
            ],
        ))
        .unwrap();
    assert!(matches!(
        &report.outcomes()[0],
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    assert_eq!(
        &report.outcomes()[1],
        &EditorActionOutcome::Rejected(EditorActionRejection::MutationAlreadySubmitted {
            document,
        })
    );
    assert_eq!(
        editor
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                std::iter::empty::<EditorAction>(),
            ))
            .unwrap_err(),
        EditorActionRejection::FrameAlreadyDispatched {
            revision: frame.revision(),
        }
    );

    let next = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                std::iter::empty::<EditorAction>(),
            ))
            .unwrap_err(),
        EditorActionRejection::StaleFrame {
            expected: next.revision(),
            received: frame.revision(),
        }
    );
    let diagnosed = editor.drive_editor_frame(Duration::ZERO);
    assert!(
        diagnosed
            .diagnostics()
            .iter()
            .any(|entry| entry.code().as_str() == "editor.action.stale-frame")
    );
    let rows = &next
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .entities;
    assert!(rows.iter().any(|row| row.id.as_str() == "first-frame-edit"));
    assert!(
        !rows
            .iter()
            .any(|row| row.id.as_str() == "second-frame-edit")
    );
}

#[test]
fn play_does_not_rebase_to_a_later_document_revision_from_the_same_frame() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let revision = frame.workspace().scene(document).unwrap().revision;

    let outcomes = editor_driver::dispatch(
        &mut editor,
        [
            EditorAction::Play(nara::tooling::EditorPlayAction::Play {
                document,
                base_revision: revision,
            }),
            EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
                document,
                base_revision: revision,
                patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: SceneEntityRecord::new(
                        SceneEntityId::new("post-play-frame-edit").unwrap(),
                    ),
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                }]),
            }),
        ],
    );
    assert_eq!(
        outcomes[0],
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)
    );
    assert!(matches!(
        outcomes[1],
        EditorActionOutcome::Workspace(ref report) if report.applied
    ));

    let next = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(next.play().state(), EditorPlayState::Empty);
    assert!(matches!(
        next.play().result(),
        Some(EditorPlayOperationResult::Rejected {
            operation: EditorPlayOperation::Play,
            reason: EditorPlayRejection::StaleDocument,
        })
    ));
    assert!(
        next.workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .entities
            .iter()
            .any(|row| row.id.as_str() == "post-play-frame-edit")
    );
}

#[test]
fn save_completion_becomes_visible_only_in_the_next_editor_frame() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor = EditorProjectSession::open(
        project.writable_root_capability(),
        EditorProjectIntent::new(),
    )
    .unwrap();
    let initial = editor.current_frame();
    let document = initial.workspace().active_document.unwrap();
    let revision = initial.workspace().scene(document).unwrap().revision;
    let mutation = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            initial.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyScenePatch {
                    document,
                    base_revision: revision,
                    patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                        entity: SceneEntityRecord::new(
                            SceneEntityId::new("saved-frame-edit").unwrap(),
                        ),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                },
            )],
        ))
        .unwrap();
    assert!(matches!(
        &mutation.outcomes()[0],
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let dirty = editor.drive_editor_frame(Duration::ZERO);
    let dirty_scene = dirty.workspace().scene(document).unwrap();
    assert!(dirty_scene.dirty);

    let save = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            dirty.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: dirty_scene.revision,
            })],
        ))
        .unwrap();
    assert!(matches!(
        &save.outcomes()[0],
        EditorActionOutcome::Persistence(nara::tooling::EditorPersistenceRequestResult::Accepted)
    ));
    assert!(
        editor
            .current_frame()
            .workspace()
            .scene(document)
            .unwrap()
            .dirty
    );

    let saved = editor.drive_editor_frame(Duration::ZERO);
    assert!(!saved.workspace().scene(document).unwrap().dirty);
}

#[test]
fn editor_host_owns_prepare_start_pause_step_stop_and_fresh_restart() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let (service_sessions, service_session_observations) = mpsc::sync_channel(2);
    let intent =
        EditorProjectIntent::new().with_recipe(runtime_service_session_recipe(service_sessions));
    let mut editor = EditorProjectSession::open(project.root_capability(), intent).unwrap();

    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty
    );
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let base_revision = frame.workspace().scene(document).unwrap().revision;
    let outcomes = editor_driver::dispatch(
        &mut editor,
        [
            EditorAction::Play(nara::tooling::EditorPlayAction::Play {
                document,
                base_revision,
            }),
            EditorAction::Play(nara::tooling::EditorPlayAction::Play {
                document,
                base_revision,
            }),
            EditorAction::Play(nara::tooling::EditorPlayAction::Cancel),
        ],
    );
    assert_eq!(
        outcomes[0],
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)
    );
    assert_eq!(
        outcomes[1],
        EditorActionOutcome::Play(EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy))
    );
    assert_eq!(
        outcomes[2],
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty
    );
    assert_eq!(
        editor.current_frame().play().result(),
        Some(EditorPlayOperationResult::Cancelled {
            operation: EditorPlayOperation::Play,
        })
    );
    assert!(matches!(
        service_session_observations.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Starting,
        "diagnostics: {:?}",
        editor.current_frame().diagnostics()
    );

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Cancel),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty
    );
    assert_eq!(
        editor.current_frame().play().result(),
        Some(EditorPlayOperationResult::Cancelled {
            operation: EditorPlayOperation::Play,
        })
    );
    assert!(matches!(
        service_session_observations.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);
    let first_generation = editor.current_frame().play().generation().unwrap();
    let first_service_session = service_session_observations
        .recv_timeout(Duration::from_secs(1))
        .expect("the first Editor runtime publishes one service session");
    assert!(matches!(
        service_session_observations.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Pause),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Paused
    );
    assert!(matches!(
        editor.current_frame().play().result(),
        Some(EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::Pause,
            generation: Some(generation),
        }) if generation == first_generation
    ));

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::StepFixedTick),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Paused
    );
    assert!(matches!(
        editor.current_frame().play().result(),
        Some(EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::StepFixedTick,
            generation: Some(generation),
        }) if generation == first_generation
    ));

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Restart),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);
    let second_generation = editor.current_frame().play().generation().unwrap();
    let second_service_session = service_session_observations
        .recv_timeout(Duration::from_secs(1))
        .expect("the restarted Editor runtime publishes one service session");
    assert!(matches!(
        service_session_observations.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    assert_ne!(second_generation, first_generation);
    assert_ne!(second_service_session, first_service_session);
    assert!(matches!(
        editor.current_frame().play().result(),
        Some(EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::Restart,
            generation: Some(generation),
        }) if generation == second_generation
    ));

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Empty, 32);
    assert_eq!(editor.current_frame().play().generation(), None);
    assert!(matches!(
        editor.current_frame().play().result(),
        Some(EditorPlayOperationResult::Applied {
            operation: EditorPlayOperation::Stop,
            ..
        })
    ));
}

#[cfg(all(
    feature = "presentation-2d",
    feature = "presentation-ui",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
#[test]
fn editor_game_start_publishes_only_after_the_exact_native_activation() {
    let project = TestProject::with_prefab_startup();
    std::fs::write(
        project.path().join("nara.toml"),
        r#"schema_version = 1

[project]
name = "Editor Game Start"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d"]

[profiles.desktop.capabilities]
requested = [
    "runtime-2d",
    "presentation-2d",
    "desktop-winit",
    "render-wgpu",
]

[profiles.desktop.runtime]
preset = "local-headless"

[profiles.desktop.window]
enabled = true
width = 960
height = 540
resizable = true
"#,
    )
    .unwrap();
    let intent = || {
        EditorProjectIntent::new()
            .with_profile("desktop")
            .with_recipe(
                ProductRecipe::new()
                    .add_plugin::<GameViewCameraPlugin>()
                    .unwrap(),
            )
    };
    let mut first = EditorProjectSession::open(project.root_capability(), intent()).unwrap();
    let mut second = EditorProjectSession::open(project.root_capability(), intent()).unwrap();

    for editor in [&mut first, &mut second] {
        assert_eq!(
            editor_driver::request_play(editor, EditorPlayCommand::Play),
            EditorPlayRequestResult::Accepted
        );
        editor.drive_editor_frame(Duration::ZERO);
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            editor.current_frame().play().state(),
            EditorPlayState::Starting,
            "diagnostics: {:?}",
            editor.current_frame().diagnostics()
        );
        assert_eq!(editor.current_frame().play().generation(), None);
    }

    let first_request = first.pending_game_start_request().unwrap();
    let second_request = second.pending_game_start_request().unwrap();
    assert_ne!(
        first_request.runtime_generation(),
        second_request.runtime_generation()
    );
    assert!(matches!(
        first.prepare_game_start_activation(second_request),
        Err(EditorGameViewBridgeFailure::GameStartRequestMismatch)
    ));
    assert_eq!(first.pending_game_start_request(), Some(first_request));

    let activation = first
        .prepare_game_start_activation(first_request)
        .expect("the exact Runtime/native start identity is accepted");
    assert_eq!(activation.request(), first_request);
    assert!(first.pending_game_start_request().is_none());
    first.commit_game_start_activation(activation);
    first.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        first.current_frame().play().state(),
        EditorPlayState::Running
    );
    assert_eq!(
        first.current_frame().play().generation(),
        Some(first_request.runtime_generation().get())
    );
    first.drive_editor_frame(Duration::ZERO);
    let candidate = first
        .take_game_view_candidate()
        .unwrap()
        .expect("the active Play Runtime publishes one exact Game candidate");
    let candidate_identity = candidate.identity();
    assert_eq!(candidate_identity.start(), first_request);
    drop(candidate);
    first
        .complete_game_view_candidate(EditorGameViewCompletion::without_ui(candidate_identity))
        .expect("the exact candidate completion returns its bridge authority");
    let resized_extent = Extent2d::new(640, 360).unwrap();
    assert!(first.resize_game_view(resized_extent).unwrap());
    let resized_request = first.active_game_start_request().unwrap();
    assert_eq!(resized_request.geometry_generation(), 2);
    assert_eq!(
        resized_request.logical_window().resolution.physical_width,
        resized_extent.width
    );
    assert_eq!(
        resized_request.logical_window().resolution.physical_height,
        resized_extent.height
    );
    first.drive_editor_frame(Duration::ZERO);
    let resized_candidate = first
        .take_game_view_candidate()
        .unwrap()
        .expect("the resized logical Game window publishes one successor candidate");
    assert_eq!(resized_candidate.identity().start(), resized_request);
    let resized_identity = resized_candidate.identity();
    first
        .complete_game_view_candidate(EditorGameViewCompletion::without_ui(resized_identity))
        .unwrap();

    first.drive_editor_frame(Duration::ZERO);
    first
        .set_game_view_source_active(false, "the Scene viewport became active")
        .expect("Scene handoff drains the buffered Game candidate exactly once");
    assert!(first.take_game_view_candidate().unwrap().is_none());
    assert_eq!(
        editor_driver::request_play(&mut first, EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    for _ in 0..8 {
        first.drive_editor_frame(Duration::ZERO);
        if first.current_frame().play().state() == EditorPlayState::Empty {
            break;
        }
    }
    assert_eq!(first.current_frame().play().state(), EditorPlayState::Empty);

    second.abort_pending_game_start(second_request).unwrap();
    for _ in 0..8 {
        second.drive_editor_frame(Duration::ZERO);
        if second.current_frame().play().state() == EditorPlayState::Empty {
            break;
        }
    }
    assert_eq!(
        second.current_frame().play().state(),
        EditorPlayState::Empty
    );
}

#[test]
fn editor_product_recipe_runs_the_same_replayable_runtime_plugin() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let (observations, receiver) = mpsc::sync_channel(1);
    let mut editor = EditorProjectSession::open(
        project.root_capability(),
        EditorProjectIntent::new().with_recipe(runtime_service_session_recipe(observations)),
    )
    .unwrap();

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    for _ in 0..32 {
        if editor.current_frame().play().state() == EditorPlayState::Running {
            break;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::yield_now();
    }

    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Running
    );
    assert!(receiver.recv_timeout(Duration::from_secs(1)).is_ok());
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    for _ in 0..32 {
        if editor.current_frame().play().state() == EditorPlayState::Empty {
            break;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::yield_now();
    }
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty
    );
}

#[test]
fn play_without_an_active_document_rejects_without_creating_an_owner() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
    let document = editor.current_frame().workspace().active_document.unwrap();
    let frame = editor.current_frame();
    let base_revision = frame.workspace().scene(document).unwrap().revision;
    let outcomes = editor_driver::dispatch(
        &mut editor,
        [
            EditorAction::WorkspaceIntent(nara::tooling::EditorWorkspaceIntentAction::Request(
                EditorWorkspaceIntent::CloseScene { document },
            )),
            EditorAction::Play(nara::tooling::EditorPlayAction::Play {
                document,
                base_revision,
            }),
        ],
    );
    assert_eq!(
        outcomes[0],
        EditorActionOutcome::WorkspaceIntent(EditorWorkspaceIntentRequestResult::Accepted)
    );
    assert!(matches!(
        outcomes[1],
        EditorActionOutcome::Rejected(EditorActionRejection::StaleDocumentFact { .. })
    ));
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty
    );
    assert_eq!(editor.current_frame().play().generation(), None);
}

#[test]
fn runtime_edit_is_generation_and_safe_point_bound() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);

    let play = editor.current_frame().play();
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let wrong = nara::tooling::EditorRuntimeEditRequest {
        generation: play.generation().unwrap().saturating_add(1),
        document_revision: play.current_revision().unwrap(),
        entity: SceneEntityId::new("enemy-anchor/enemy").unwrap(),
        component: ComponentTypeId::new("nara.sprite.Sprite"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("layer"),
        value: ComponentValue::I64(4),
    };
    let exact = nara::tooling::EditorRuntimeEditRequest {
        generation: play.generation().unwrap(),
        ..wrong.clone()
    };
    let outcomes = editor_driver::dispatch(
        &mut editor,
        [
            EditorAction::RuntimeEdit {
                document,
                request: wrong,
            },
            EditorAction::RuntimeEdit {
                document,
                request: exact,
            },
            EditorAction::AcknowledgeRuntimeEditResult,
        ],
    );
    assert!(matches!(
        outcomes[0],
        EditorActionOutcome::Rejected(EditorActionRejection::RuntimeGenerationChanged { .. })
    ));
    assert_eq!(outcomes[1], EditorActionOutcome::RuntimeEdit(Ok(())));
    assert_eq!(
        outcomes[2],
        EditorActionOutcome::Acknowledged(nara::tooling::EditorActionAcknowledgement::RuntimeEdit(
            false
        ))
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.current_frame().runtime_edit_result(),
        Some(nara::tooling::EditorRuntimeEditResult::Applied(_))
    ));
    assert!(editor_driver::acknowledge_runtime_edit_result(&mut editor));

    let frame = editor.drive_editor_frame(Duration::ZERO);
    let second = nara::tooling::EditorRuntimeEditRequest {
        generation: frame.play().generation().unwrap(),
        document_revision: frame.play().current_revision().unwrap(),
        entity: SceneEntityId::new("enemy-anchor/enemy").unwrap(),
        component: ComponentTypeId::new("nara.sprite.Sprite"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("layer"),
        value: ComponentValue::I64(5),
    };
    let outcomes = editor_driver::dispatch(
        &mut editor,
        [
            EditorAction::RuntimeEdit {
                document,
                request: second,
            },
            EditorAction::Play(nara::tooling::EditorPlayAction::Stop),
        ],
    );
    assert_eq!(outcomes[0], EditorActionOutcome::RuntimeEdit(Ok(())));
    assert_eq!(
        outcomes[1],
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)
    );
    drive_until(&mut editor, EditorPlayState::Empty, 32);
    assert!(matches!(
        editor.current_frame().runtime_edit_result(),
        Some(nara::tooling::EditorRuntimeEditResult::Cancelled(_))
    ));
}

#[test]
fn paused_editor_edit_refreshes_parented_global_before_extract_and_resume() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let transform_id = ComponentTypeId::new("nara.transform.Transform2d");
    let root_id = SceneEntityId::new("root").unwrap();
    let child_id = SceneEntityId::new("root/child").unwrap();
    project.write_scene_source(&SceneDocument::new(
        [root_id.clone()],
        [
            SceneEntityRecord::new(root_id.clone())
                .with_children([child_id.clone()])
                .with_component(transform_id.clone(), transform_record(10.0, 20.0)),
            SceneEntityRecord::new(child_id.clone())
                .with_component(transform_id.clone(), transform_record(3.0, 4.0)),
        ],
    ));
    let (observations, receiver) = mpsc::sync_channel(128);
    let mut editor = open_editor_with_probe(
        project.root_capability(),
        EditorProjectIntent::new(),
        RuntimeProbe::new(spatial_observation_definition(child_id, observations)),
    )
    .unwrap();

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        latest_spatial_observation(&receiver),
        nara::prelude::Vec2::new(13.0, 24.0)
    );

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Pause),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Paused
    );
    assert_eq!(
        latest_spatial_observation(&receiver),
        nara::prelude::Vec2::new(13.0, 24.0)
    );

    editor_driver::request_runtime_edit(
        &mut editor,
        root_id,
        transform_id,
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("translation.x"),
        ComponentValue::f64(40.0).unwrap(),
    )
    .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.current_frame().runtime_edit_result(),
        Some(nara::tooling::EditorRuntimeEditResult::Applied(_))
    ));
    assert_eq!(
        latest_spatial_observation(&receiver),
        nara::prelude::Vec2::new(13.0, 24.0),
        "the Last-stage edit becomes visible at the next Extract boundary"
    );
    assert!(editor_driver::acknowledge_runtime_edit_result(&mut editor));

    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Paused
    );
    assert_eq!(
        latest_spatial_observation(&receiver),
        nara::prelude::Vec2::new(43.0, 24.0)
    );

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Resume),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Running
    );
    assert_eq!(
        latest_spatial_observation(&receiver),
        nara::prelude::Vec2::new(43.0, 24.0),
        "resume must not expose one frame of the prior global projection"
    );

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Empty, 32);
}

#[test]
fn apply_changes_exports_selected_runtime_value_and_marks_runtime_out_of_date() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let transform_id = ComponentTypeId::new("nara.transform.Transform2d");
    let entity_id = SceneEntityId::new("player").unwrap();
    project.write_scene_source(&SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(entity_id.clone()).with_component(
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
        ),
    ]));
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Pause),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Paused
    );

    let request =
        nara::tooling::SceneApplyChangesRequest::new(entity_id.clone(), [transform_id.clone()]);

    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let base_revision = frame.workspace().scene(document).unwrap().revision;
    let generation = frame.play().generation().unwrap();
    let runtime_edit = nara::tooling::EditorRuntimeEditRequest {
        generation,
        document_revision: base_revision,
        entity: entity_id.clone(),
        component: transform_id.clone(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("translation.x"),
        value: ComponentValue::f64(42.0).unwrap(),
    };
    let outcomes = editor_driver::dispatch(
        &mut editor,
        [
            EditorAction::RuntimeEdit {
                document,
                request: runtime_edit,
            },
            EditorAction::ApplyChanges {
                document,
                base_revision,
                generation,
                request: request.clone(),
            },
        ],
    );
    assert_eq!(outcomes[0], EditorActionOutcome::RuntimeEdit(Ok(())));
    assert_eq!(
        outcomes[1],
        EditorActionOutcome::ApplyChanges(Err(nara::tooling::EditorApplyChangesRejection::Busy))
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Paused
    );
    assert!(editor_driver::acknowledge_runtime_edit_result(&mut editor));
    editor.drive_editor_frame(Duration::ZERO);

    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let base_revision = frame.workspace().scene(document).unwrap().revision;
    let generation = frame.play().generation().unwrap();
    let busy_edit = nara::tooling::EditorRuntimeEditRequest {
        generation,
        document_revision: base_revision,
        entity: entity_id.clone(),
        component: transform_id.clone(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("translation.x"),
        value: ComponentValue::f64(43.0).unwrap(),
    };
    let outcomes = editor_driver::dispatch(
        &mut editor,
        [
            EditorAction::ApplyChanges {
                document,
                base_revision,
                generation,
                request: request.clone(),
            },
            EditorAction::Play(nara::tooling::EditorPlayAction::Resume),
            EditorAction::Play(nara::tooling::EditorPlayAction::StepFixedTick),
            EditorAction::RuntimeEdit {
                document,
                request: busy_edit,
            },
        ],
    );
    assert_eq!(outcomes[0], EditorActionOutcome::ApplyChanges(Ok(())));
    assert_eq!(
        outcomes[1],
        EditorActionOutcome::Play(EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy))
    );
    assert_eq!(
        outcomes[2],
        EditorActionOutcome::Play(EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy))
    );
    assert_eq!(
        outcomes[3],
        EditorActionOutcome::RuntimeEdit(Err(nara::tooling::EditorRuntimeEditRejection::Busy))
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Paused
    );
    let applied_frame = editor.current_frame();
    let Some(nara::tooling::EditorApplyChangesResult::Applied(report)) =
        applied_frame.apply_changes_result()
    else {
        panic!(
            "expected an applied change, got {:?}",
            applied_frame.apply_changes_result()
        );
    };
    assert!(report.applied);
    assert_eq!(report.components.len(), 1);
    assert!(editor.current_frame().play().is_out_of_date());
    let exported_value = report
        .patch
        .as_ref()
        .and_then(|patch| patch.operations.first())
        .and_then(|operation| match operation {
            ScenePatchOperation::ReplaceComponent { value, .. } => value
                .value
                .get("translation")
                .and_then(|translation| translation.get("x")),
            _ => None,
        })
        .expect("Apply Changes should export the edited component value");
    assert_eq!(exported_value, &ComponentValue::f64(42.0).unwrap());
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let base_revision = frame.workspace().scene(document).unwrap().revision;
    let generation = frame.play().generation().unwrap();
    let outcomes = editor_driver::dispatch(
        &mut editor,
        [
            EditorAction::ApplyChanges {
                document,
                base_revision,
                generation,
                request,
            },
            EditorAction::AcknowledgeApplyChangesResult,
        ],
    );
    assert_eq!(
        outcomes[0],
        EditorActionOutcome::ApplyChanges(Err(nara::tooling::EditorApplyChangesRejection::Busy))
    );
    assert_eq!(
        outcomes[1],
        EditorActionOutcome::Acknowledged(
            nara::tooling::EditorActionAcknowledgement::ApplyChanges(true)
        )
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor_driver::request_apply_changes(
            &mut editor,
            nara::tooling::SceneApplyChangesRequest::new(entity_id.clone(), [transform_id.clone()],),
        ),
        Err(nara::tooling::EditorApplyChangesRejection::RuntimeOutOfDate)
    );
    editor.drive_editor_frame(Duration::ZERO);

    let stale_generation = editor.current_frame().play().generation().unwrap();
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Restart),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);
    assert!(!editor.current_frame().play().is_out_of_date());
    assert_ne!(
        editor.current_frame().play().generation(),
        Some(stale_generation)
    );

    let cancelled_request = nara::tooling::SceneApplyChangesRequest::new(entity_id, [transform_id]);
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let outcomes = editor_driver::dispatch(
        &mut editor,
        [
            EditorAction::ApplyChanges {
                document,
                base_revision: frame.workspace().scene(document).unwrap().revision,
                generation: frame.play().generation().unwrap(),
                request: cancelled_request.clone(),
            },
            EditorAction::Play(nara::tooling::EditorPlayAction::Stop),
        ],
    );
    assert_eq!(outcomes[0], EditorActionOutcome::ApplyChanges(Ok(())));
    assert_eq!(
        outcomes[1],
        EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)
    );
    drive_until(&mut editor, EditorPlayState::Empty, 32);
    assert_eq!(
        editor.current_frame().apply_changes_result(),
        Some(&nara::tooling::EditorApplyChangesResult::Cancelled(
            cancelled_request
        ))
    );
}

fn transform_record(x: f64, y: f64) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            (
                "translation",
                ComponentValue::map([
                    ("x", ComponentValue::f64(x).unwrap()),
                    ("y", ComponentValue::f64(y).unwrap()),
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
    )
}

fn latest_spatial_observation(
    receiver: &mpsc::Receiver<nara::prelude::Vec2>,
) -> nara::prelude::Vec2 {
    let mut latest = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("the Editor runtime should publish one spatial observation");
    while let Ok(observation) = receiver.try_recv() {
        latest = observation;
    }
    latest
}

#[test]
fn workspace_close_retains_document_through_close_incomplete_and_retry() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let release = Arc::new(AtomicBool::new(false));
    let intent = EditorProjectIntent::new().with_cleanup_timeout(Duration::ZERO);
    let mut editor = open_editor_with_probe(
        project.root_capability(),
        intent,
        RuntimeProbe::new(delayed_close_definition(Arc::clone(&release))),
    )
    .unwrap();
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);

    assert_eq!(
        editor_driver::request_workspace_intent(
            &mut editor,
            EditorWorkspaceIntent::CloseScene { document },
        ),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::CloseIncomplete, 32);
    assert!(editor.current_frame().workspace().scene(document).is_some());
    assert!(editor.current_frame().workspace_intent().intent().is_some());

    release.store(true, Ordering::Release);
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::RetryClose),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    drive_retryable_close_until_empty(&mut editor, 32);
    assert!(editor.current_frame().workspace().scenes.is_empty());
    assert!(editor.current_frame().workspace_intent().intent().is_none());
}

#[test]
fn direct_stop_retains_close_incomplete_until_retry() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let release = Arc::new(AtomicBool::new(false));
    let intent = EditorProjectIntent::new().with_cleanup_timeout(Duration::ZERO);
    let mut editor = open_editor_with_probe(
        project.root_capability(),
        intent,
        RuntimeProbe::new(delayed_close_definition(Arc::clone(&release))),
    )
    .unwrap();
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::CloseIncomplete, 32);
    assert!(editor.current_frame().workspace().scene(document).is_some());
    assert!(editor.current_frame().play().generation().is_some());
    assert!(matches!(
        editor.current_frame().play().result(),
        Some(EditorPlayOperationResult::Failed {
            operation: EditorPlayOperation::Stop,
            failure: EditorPlayFailure::Close,
        })
    ));

    release.store(true, Ordering::Release);
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::RetryClose),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    drive_retryable_close_until_empty(&mut editor, 32);
    assert!(editor.current_frame().workspace().scene(document).is_some());
    assert_eq!(editor.current_frame().play().generation(), None);
}

#[test]
fn terminal_shutdown_failure_prevents_restart_from_reporting_success() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let recipe = ProductRecipe::new()
        .add_plugin::<ShutdownFailurePlugin>()
        .expect("the shutdown-failure recipe is statically valid");
    let intent = EditorProjectIntent::new().with_recipe(recipe);
    let mut editor = EditorProjectSession::open(project.root_capability(), intent).unwrap();
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);
    let first_generation = editor.current_frame().play().generation();

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Restart),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Empty, 32);
    assert_eq!(editor.current_frame().play().generation(), None);
    assert_ne!(first_generation, editor.current_frame().play().generation());
    assert!(matches!(
        editor.current_frame().play().result(),
        Some(EditorPlayOperationResult::Failed {
            operation: EditorPlayOperation::Restart,
            failure: EditorPlayFailure::Close,
        })
    ));
}

#[test]
fn apply_changes_rejects_prefab_expansion_without_document_mutation() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut editor =
        EditorProjectSession::open(project.root_capability(), EditorProjectIntent::new()).unwrap();
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Running, 32);
    let original_revision = editor
        .current_frame()
        .workspace()
        .scene(document)
        .unwrap()
        .revision;

    let expanded_request = nara::tooling::SceneApplyChangesRequest::new(
        SceneEntityId::new("enemy-anchor/enemy").unwrap(),
        [ComponentTypeId::new("nara.sprite.Sprite")],
    );
    editor_driver::request_apply_changes(&mut editor, expanded_request.clone()).unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.current_frame().apply_changes_result(),
        Some(nara::tooling::EditorApplyChangesResult::Rejected {
            request,
            reason: nara::tooling::EditorApplyChangesRejection::Unsupported,
            ..
        }) if request == &expanded_request
    ));
    assert_eq!(
        editor
            .current_frame()
            .workspace()
            .scene(document)
            .unwrap()
            .revision,
        original_revision
    );
    assert!(editor_driver::acknowledge_apply_changes_result(&mut editor));
    editor.drive_editor_frame(Duration::ZERO);

    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_until(&mut editor, EditorPlayState::Empty, 32);
}

#[test]
fn failed_start_retains_retirement_owner_until_retry_completes() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let release = Arc::new(AtomicBool::new(false));
    let recipe = ProductRecipe::new()
        .add_plugin::<BuildFailurePlugin>()
        .expect("the build-failure recipe is statically valid");
    let intent = EditorProjectIntent::new()
        .with_cleanup_timeout(Duration::ZERO)
        .with_recipe(recipe);
    let mut editor = open_editor_with_probe(
        project.root_capability(),
        intent,
        RuntimeProbe::new(delayed_close_definition(Arc::clone(&release))),
    )
    .unwrap();
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Starting
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::RetiringPlay
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::RetirementIncomplete
    );
    assert!(editor.current_frame().workspace().scene(document).is_some());
    assert_eq!(editor.current_frame().play().generation(), None);

    release.store(true, Ordering::Release);
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::RetryRetirement),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    drive_retryable_close_until_empty(&mut editor, 32);
    assert!(editor.current_frame().workspace().scene(document).is_some());
    assert!(matches!(
        editor.current_frame().play().result(),
        Some(EditorPlayOperationResult::Failed {
            operation: EditorPlayOperation::Play,
            ..
        })
    ));
}

#[test]
fn failed_start_remains_retiring_while_cleanup_is_still_pending() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let release = Arc::new(AtomicBool::new(false));
    let recipe = ProductRecipe::new()
        .add_plugin::<BuildFailurePlugin>()
        .expect("the build-failure recipe is statically valid");
    let intent = EditorProjectIntent::new().with_recipe(recipe);
    let mut editor = open_editor_with_probe(
        project.root_capability(),
        intent,
        RuntimeProbe::new(delayed_close_definition(Arc::clone(&release))),
    )
    .unwrap();
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Starting
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::RetiringPlay
    );

    for _ in 0..2 {
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            editor.current_frame().play().state(),
            EditorPlayState::RetiringPlay
        );
    }

    release.store(true, Ordering::Release);
    drive_until(&mut editor, EditorPlayState::Empty, 32);
    assert!(matches!(
        editor.current_frame().play().result(),
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
        if editor.current_frame().play().state() == expected {
            return;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        editor.current_frame().play().state(),
        expected,
        "diagnostics: {:?}",
        editor.current_frame().diagnostics()
    );
}

fn drive_retryable_close_until_empty(editor: &mut EditorProjectSession, maximum_attempts: usize) {
    for _ in 0..maximum_attempts {
        match editor.current_frame().play().state() {
            EditorPlayState::Empty => return,
            EditorPlayState::RetirementIncomplete => assert_eq!(
                editor_driver::request_play(editor, EditorPlayCommand::RetryRetirement),
                EditorPlayRequestResult::Accepted
            ),
            EditorPlayState::CloseIncomplete => assert_eq!(
                editor_driver::request_play(editor, EditorPlayCommand::RetryClose),
                EditorPlayRequestResult::Accepted
            ),
            _ => {}
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty,
        "diagnostics: {:?}",
        editor.current_frame().diagnostics()
    );
}
