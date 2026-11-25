//! Commits UI presentation after the original WGPU result returns to its CPU owners.

use std::sync::Arc;

use crate::hud::presentation::{UiPresentationPlannerClaim, presentation_planner_claim};
use nara_app::{App, AppRunError, PluginError, RuntimeInstance, RuntimeState};
use nara_ecs::{Resource, World};

use super::{UiPresentationAttemptLedger, commit_runtime_ui_presentations};

#[derive(Resource)]
struct UiPostHostAuthority {
    owner: Arc<()>,
    planner: UiPresentationPlannerClaim,
}

pub(super) fn install(app: &mut App) -> Result<(), PluginError> {
    let owner = app
        .world()
        .resource::<UiPresentationAttemptLedger>()
        .owner();
    let planner =
        presentation_planner_claim(app.world()).ok_or_else(|| PluginError::SetupFailed {
            plugin: super::UI_PRESENTATION_BRIDGE_PLUGIN_ID,
            message: "the UI post-Host join requires its original presentation planner".to_owned(),
        })?;
    app.insert_resource(UiPostHostAuthority { owner, planner })?;
    Ok(())
}

fn complete_presentation(world: &mut World, owner: &Arc<()>) -> Result<(), AppRunError> {
    let Some(authority) = world.get_resource::<UiPostHostAuthority>() else {
        return Err(missing_authority());
    };
    let Some(ledger) = world.get_resource::<UiPresentationAttemptLedger>() else {
        return Err(missing_authority());
    };
    if !Arc::ptr_eq(&authority.owner, owner)
        || !ledger.is_owned_by(owner)
        || !authority.planner.is_current(world)
    {
        return Err(missing_authority());
    }
    commit_runtime_ui_presentations(world);
    Ok(())
}

fn missing_authority() -> AppRunError {
    AppRunError::runner("the original UI post-Host presentation authority is unavailable")
}

/// Retains the original UI presentation owner across WGPU host deliveries.
pub struct UiPostHostConnection {
    owner: Arc<()>,
}

impl UiPostHostConnection {
    /// Captures a live runtime's presentation owner when the bridge is installed.
    pub fn capture(runtime: &RuntimeInstance) -> Result<Option<Self>, AppRunError> {
        if !matches!(
            runtime.state(),
            RuntimeState::Running | RuntimeState::Paused
        ) {
            return Err(AppRunError::runner(
                "UI post-Host attachment requires a live Runtime",
            ));
        }
        let world = runtime.world();
        match world.get_resource::<UiPostHostAuthority>() {
            Some(authority) => Ok(Some(Self {
                owner: Arc::clone(&authority.owner),
            })),
            None if world.contains_resource::<UiPresentationAttemptLedger>() => {
                Err(missing_authority())
            }
            None => Ok(None),
        }
    }

    /// Drains backend results for the captured owner after the host returns the WGPU result.
    pub fn complete(&self, runtime: &mut RuntimeInstance) -> Result<(), AppRunError> {
        // Late Faulted/Stopping results belong to logical close. They cannot advance UI/Input.
        if !matches!(
            runtime.state(),
            RuntimeState::Running | RuntimeState::Paused
        ) {
            return Ok(());
        }
        let owner = Arc::clone(&self.owner);
        let outcome = runtime
            .apply_command(move |world: &mut World| complete_presentation(world, &owner))
            .map_err(|error| {
                AppRunError::runner(format!("UI post-Host scope was refused: {error}"))
            })?;
        match (outcome.output, outcome.post_operation_fault) {
            (Ok(()), None) => Ok(()),
            (Ok(()), Some(fault)) => Err(AppRunError::managed_runtime_fault(&fault)),
            (Err(error), None) => Err(error),
            (Err(error), Some(fault)) => Err(AppRunError::runner_teardown(
                error,
                AppRunError::managed_runtime_fault(&fault),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::hud::{
        UiBackendSubmissionIdentity, UiPresentationBackendGeneration,
        UiPresentationIntegrationError,
        presentation::{
            UiBackendPresentationFrame, UiCompletedBackendOutcome, backend_outcome_receiver,
            bind_backend_submission_batch, commit_ui_backend_outcome,
            take_ui_backend_presentation_frame,
        },
    };
    use nara_input::{InputPointerId, InputSeatId, InputWindowId};
    use nara_scene::hierarchy::{
        HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
    };
    use nara_ui::{
        UiColor, UiNode, UiRoot, UiRuntimeKey, UiSafeAreaInsets, UiSize, UiSurfaceContentTransform,
        UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot, UiVisual,
    };

    use super::*;
    use crate::hud::bridge::{
        UiPresentationBridgeStats, latest_presented_ui_observation, tests::install_test_plugins,
    };

    const SURFACE: u64 = 79;

    enum FixtureInput {
        Take,
        Return(UiCompletedBackendOutcome),
        ReplaceLedger,
        RestoreLedger(UiPresentationAttemptLedger),
        RemovePort,
        ReplacePort,
        RestorePort(UiPostHostAuthority),
    }

    enum FixtureOutput {
        Frame(Result<UiBackendPresentationFrame, UiPresentationIntegrationError>),
        Returned(Result<(), UiCompletedBackendOutcome>),
        Ledger(UiPresentationAttemptLedger),
        Port(UiPostHostAuthority),
        Done,
    }

    fn apply_fixture(world: &mut World, input: FixtureInput) -> FixtureOutput {
        match input {
            FixtureInput::Take => FixtureOutput::Frame(take_ui_backend_presentation_frame(world)),
            FixtureInput::Return(outcome) => {
                FixtureOutput::Returned(commit_ui_backend_outcome(world, outcome))
            }
            FixtureInput::ReplaceLedger => {
                let original = world
                    .remove_resource::<UiPresentationAttemptLedger>()
                    .unwrap();
                world.insert_resource(UiPresentationAttemptLedger::default());
                FixtureOutput::Ledger(original)
            }
            FixtureInput::RestoreLedger(original) => {
                world.insert_resource(original);
                FixtureOutput::Done
            }
            FixtureInput::RemovePort => {
                FixtureOutput::Port(world.remove_resource::<UiPostHostAuthority>().unwrap())
            }
            FixtureInput::ReplacePort => {
                let original = world.remove_resource::<UiPostHostAuthority>().unwrap();
                world.insert_resource(UiPostHostAuthority {
                    owner: Arc::new(()),
                    planner: original.planner.clone(),
                });
                FixtureOutput::Port(original)
            }
            FixtureInput::RestorePort(original) => {
                world.insert_resource(original);
                FixtureOutput::Done
            }
        }
    }

    fn mutate(runtime: &mut RuntimeInstance, input: FixtureInput) -> FixtureOutput {
        let outcome = runtime
            .with_driver_scope(|scope| scope.with_world_mut(|world| apply_fixture(world, input)))
            .unwrap();
        assert!(outcome.post_operation_fault.is_none());
        outcome.output
    }

    fn fixture() -> RuntimeInstance {
        let mut app = App::new();
        install_test_plugins(&mut app);
        let world = app.world_mut().unwrap();
        {
            let mut input = world.resource_mut::<nara_input::InputTimeline>();
            input
                .connect_endpoint(InputWindowId::new(1), InputSeatId::PRIMARY)
                .unwrap();
            input
                .connect_pointer(
                    InputWindowId::new(1),
                    InputSeatId::PRIMARY,
                    InputPointerId::PRIMARY,
                )
                .unwrap();
        }
        let primary_exists = world
            .query::<&nara_window::Window>()
            .iter(world)
            .any(|window| window.id == nara_window::WindowId::PRIMARY);
        if !primary_exists {
            world.spawn((
                nara_window::Window::new("post-host", nara_window::WindowResolution::new(320, 180))
                    .with_id(nara_window::WindowId::PRIMARY),
                nara_window::PrimaryWindow,
            ));
        }
        nara_ui::install_ui_surface(
            world,
            UiSurfaceSlot::new(
                UiSurfaceId::new(SURFACE).unwrap(),
                UiSurfaceEndpoint::new(
                    InputWindowId::new(1),
                    InputSeatId::PRIMARY,
                    InputPointerId::PRIMARY,
                ),
                UiSurfaceContentTransform::new(
                    UiSize::new(320.0, 180.0).unwrap(),
                    UiSize::new(320.0, 180.0).unwrap(),
                    UiSafeAreaInsets::default(),
                    1.0,
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        let root = world
            .spawn((
                UiRoot::default(),
                UiNode::default(),
                UiRuntimeKey::new("test.post-host", "root").unwrap(),
                UiVisual::solid(UiColor::rgba(0.2, 0.4, 0.6, 1.0)),
            ))
            .id();
        HierarchyWriter::new(world)
            .apply_batch(&[HierarchyOperation::Place {
                entity: root,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            }])
            .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        runtime
    }

    /// Returns the real original CPU backend obligation after the managed frame has ended.
    /// This fixture issues no WGPU presentation facts and does not require a native GPU.
    fn return_original_presentation(runtime: &mut RuntimeInstance) -> UiBackendSubmissionIdentity {
        let FixtureOutput::Frame(frame) = mutate(runtime, FixtureInput::Take) else {
            unreachable!()
        };
        let (_, submissions) = frame.unwrap().into_parts();
        assert_eq!(submissions.len(), 1);
        let identity = submissions[0].submission().identity();
        let outcome = bind_backend_submission_batch(
            submissions,
            UiPresentationBackendGeneration::new(701, 1).unwrap(),
        )
        .unwrap()
        .pop()
        .unwrap()
        .presented();
        let FixtureOutput::Returned(result) = mutate(runtime, FixtureInput::Return(outcome)) else {
            unreachable!()
        };
        result.unwrap();
        identity
    }

    fn assert_uncommitted(runtime: &RuntimeInstance) {
        assert_eq!(
            runtime
                .world()
                .resource::<UiPresentationBridgeStats>()
                .presented_packets,
            0
        );
        assert!(latest_presented_ui_observation(runtime.world()).is_none());
    }

    #[test]
    fn captured_original_post_host_connection_refuses_replaced_ledger_without_consumption() {
        let mut runtime = fixture();
        let connection = UiPostHostConnection::capture(&runtime).unwrap().unwrap();
        let original_receiver = backend_outcome_receiver(runtime.world()).unwrap();
        let identity = return_original_presentation(&mut runtime);
        let FixtureOutput::Ledger(original) = mutate(&mut runtime, FixtureInput::ReplaceLedger)
        else {
            unreachable!()
        };
        assert_eq!(original.active().count(), 1);

        assert!(connection.complete(&mut runtime).is_err());
        assert_uncommitted(&runtime);
        assert!(original_receiver.has_unsettled_backend_results());
        assert!(original.is_owned_by(&connection.owner));

        assert!(matches!(
            mutate(&mut runtime, FixtureInput::RestoreLedger(original)),
            FixtureOutput::Done
        ));
        connection.complete(&mut runtime).unwrap();
        assert_eq!(
            latest_presented_ui_observation(runtime.world())
                .unwrap()
                .submission(),
            identity
        );
        assert!(!original_receiver.has_unsettled_backend_results());
        runtime.finish().unwrap();
    }

    #[test]
    fn missing_post_host_port_refuses_without_blocking_original_receiver_close() {
        let mut runtime = fixture();
        let connection = UiPostHostConnection::capture(&runtime).unwrap().unwrap();
        let original_receiver = backend_outcome_receiver(runtime.world()).unwrap();
        return_original_presentation(&mut runtime);
        let FixtureOutput::Port(original) = mutate(&mut runtime, FixtureInput::RemovePort) else {
            unreachable!()
        };
        assert!(Arc::ptr_eq(&original.owner, &connection.owner));

        assert!(connection.complete(&mut runtime).is_err());
        assert_uncommitted(&runtime);
        assert!(original_receiver.has_unsettled_backend_results());
        // The new presentation port stays absent through logical close. Its original receiver
        // and existing close port settle all UI/Input obligations independently.
        runtime.finish().unwrap();
        assert!(!original_receiver.has_unsettled_backend_results());
        assert!(original_receiver.drain().unwrap().is_empty());
    }

    #[test]
    fn captured_original_post_host_connection_refuses_replaced_port_owner() {
        let mut runtime = fixture();
        let connection = UiPostHostConnection::capture(&runtime).unwrap().unwrap();
        let identity = return_original_presentation(&mut runtime);
        let FixtureOutput::Port(original) = mutate(&mut runtime, FixtureInput::ReplacePort) else {
            unreachable!()
        };

        assert!(connection.complete(&mut runtime).is_err());
        assert_uncommitted(&runtime);
        assert!(matches!(
            mutate(&mut runtime, FixtureInput::RestorePort(original)),
            FixtureOutput::Done
        ));
        connection.complete(&mut runtime).unwrap();
        assert_eq!(
            latest_presented_ui_observation(runtime.world())
                .unwrap()
                .submission(),
            identity
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn actual_post_host_join_commits_returned_original_once_without_another_frame() {
        let mut runtime = fixture();
        let connection = UiPostHostConnection::capture(&runtime).unwrap().unwrap();
        let original_receiver = backend_outcome_receiver(runtime.world()).unwrap();
        let identity = return_original_presentation(&mut runtime);
        let app_frame = runtime.world().resource::<nara_app::RealTime>().frame;
        assert_uncommitted(&runtime);

        connection.complete(&mut runtime).unwrap();
        let observation = latest_presented_ui_observation(runtime.world()).unwrap();
        assert_eq!(observation.submission(), identity);
        assert!(
            observation.wgpu().is_none(),
            "the CPU fixture cannot issue native WGPU facts"
        );
        connection.complete(&mut runtime).unwrap();
        assert!(!original_receiver.has_unsettled_backend_results());
        assert_eq!(
            runtime
                .world()
                .resource::<UiPresentationBridgeStats>()
                .presented_packets,
            1
        );
        assert_eq!(
            runtime.world().resource::<nara_app::RealTime>().frame,
            app_frame
        );
        runtime.finish().unwrap();
    }
}
