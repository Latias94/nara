//! The concrete Root join after the original WGPU result has returned to its CPU owners.

use std::sync::Arc;

use nara_app::{
    __RuntimeDriverPort, App, AppRunError, PluginError, RuntimeInstance, RuntimeState,
    RuntimeWorldAccessError,
};
use nara_ecs::{Resource, World};
use nara_ui_render::__private::{UiPresentationPlannerClaim, presentation_planner_claim};

use super::{UiPresentationAttemptLedger, commit_runtime_ui_presentations};

#[derive(Resource)]
struct UiPostHostPort {
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
    app.insert_resource(UiPostHostPort { owner, planner })?;
    app.__register_driver_port::<UiPostHostPort>(apply)?;
    Ok(())
}

impl __RuntimeDriverPort for UiPostHostPort {
    type Input = Arc<()>;
    type Output = Result<(), AppRunError>;

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        Err(AppRunError::runner(
            "the original UI post-Host operation is unconfigured",
        ))
    }
}

fn apply(
    world: &mut World,
    owner: Arc<()>,
) -> Result<Result<(), AppRunError>, (RuntimeWorldAccessError, Arc<()>)> {
    let Some(port) = world.get_resource::<UiPostHostPort>() else {
        return Err((missing_port(), owner));
    };
    let Some(ledger) = world.get_resource::<UiPresentationAttemptLedger>() else {
        return Err((missing_port(), owner));
    };
    if !Arc::ptr_eq(&port.owner, &owner)
        || !ledger.is_owned_by(&owner)
        || !port.planner.is_current(world)
    {
        return Err((missing_port(), owner));
    }
    commit_runtime_ui_presentations(world);
    Ok(Ok(()))
}

fn missing_port() -> RuntimeWorldAccessError {
    RuntimeWorldAccessError::MissingResource {
        type_name: std::any::type_name::<UiPostHostPort>(),
    }
}

/// Retains the original Root UI owner rather than discovering a replacement on every frame.
pub(crate) struct UiPostHostConnection {
    owner: Arc<()>,
}

impl UiPostHostConnection {
    pub(crate) fn capture(runtime: &RuntimeInstance) -> Result<Option<Self>, AppRunError> {
        if !matches!(
            runtime.state(),
            RuntimeState::Running | RuntimeState::Paused
        ) {
            return Err(AppRunError::runner(
                "UI post-Host attachment requires a live Runtime",
            ));
        }
        let world = runtime.world();
        match world.get_resource::<UiPostHostPort>() {
            Some(port) => Ok(Some(Self {
                owner: Arc::clone(&port.owner),
            })),
            None if world.contains_resource::<UiPresentationAttemptLedger>() => Err(
                AppRunError::runner("the original UI post-Host port is unavailable"),
            ),
            None => Ok(None),
        }
    }

    pub(crate) fn complete(&self, runtime: &mut RuntimeInstance) -> Result<(), AppRunError> {
        // Late Faulted/Stopping results belong to logical close. They cannot advance UI/Input.
        if !matches!(
            runtime.state(),
            RuntimeState::Running | RuntimeState::Paused
        ) {
            return Ok(());
        }
        match runtime.__with_owned_driver_port::<UiPostHostPort>(Arc::clone(&self.owner)) {
            Ok(outcome) => {
                // Invocation already happened even if the enclosing authority subsequently failed.
                let applied = outcome
                    .output
                    .map_err(|(error, _owner)| {
                        AppRunError::runner(format!("UI post-Host result was refused: {error}"))
                    })
                    .and_then(std::convert::identity);
                match (applied, outcome.post_operation_fault) {
                    (Ok(()), None) => Ok(()),
                    (Ok(()), Some(fault)) => Err(AppRunError::managed_runtime_fault(&fault)),
                    (Err(error), None) => Err(error),
                    (Err(error), Some(fault)) => Err(AppRunError::runner_teardown(
                        error,
                        AppRunError::managed_runtime_fault(&fault),
                    )),
                }
            }
            Err((error, _owner)) => Err(AppRunError::runner(format!(
                "UI post-Host scope was refused before invocation: {error}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nara_app::{RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter};
    use nara_hierarchy::{
        HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
    };
    use nara_input::{InputPointerId, InputSeatId, InputWindowId};
    use nara_ui::{
        UiColor, UiNode, UiPresentationReceipt, UiRoot, UiRuntimeKey, UiSafeAreaInsets, UiSize,
        UiSurfaceContentTransform, UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot, UiVisual,
    };
    use nara_ui_render::{
        __private::{
            UiBackendPresentationFrame, UiCompletedBackendOutcome, backend_outcome_receiver,
            bind_backend_submission_batch, commit_ui_backend_outcome,
            take_ui_backend_presentation_frame,
        },
        UiBackendSubmissionIdentity, UiPresentationBackendGeneration,
        UiPresentationIntegrationError,
    };

    use super::*;
    use crate::ui_presentation::{
        NativeAccessibilityPresentationPort, UiPresentationBridgeStats,
        latest_presented_ui_observation,
    };
    use crate::{PresentationUiPlugins, RuntimeUiPlugins};

    const SURFACE: u64 = 79;
    const POST_COMMIT_FAULT: &str = "nara.test.ui-post-host.after-consumption";

    #[derive(Default, Resource)]
    struct CommitNotifications {
        count: u32,
        fault_after_commit: bool,
    }

    fn observe_committed_receipt(
        world: &mut World,
        _receipt: UiPresentationReceipt,
    ) -> Result<(), String> {
        let fault_after_commit = {
            let mut notifications = world.resource_mut::<CommitNotifications>();
            notifications.count += 1;
            notifications.fault_after_commit
        };
        if fault_after_commit {
            world
                .resource::<RuntimeFaultReporter>()
                .report(RuntimeFault::engine(
                    RuntimeFaultKind::RequiredService,
                    POST_COMMIT_FAULT,
                ));
        }
        Ok(())
    }

    #[derive(Resource)]
    struct FixturePort;

    // Closed test-only operations preserve detached original owners; no arbitrary World callback.
    enum FixtureInput {
        Take,
        Return(UiCompletedBackendOutcome),
        ReplaceLedger,
        RestoreLedger(UiPresentationAttemptLedger),
        RemovePort,
        ReplacePort,
        RestorePort(UiPostHostPort),
    }

    enum FixtureOutput {
        Frame(Result<UiBackendPresentationFrame, UiPresentationIntegrationError>),
        Returned(Result<(), UiCompletedBackendOutcome>),
        Ledger(UiPresentationAttemptLedger),
        Port(UiPostHostPort),
        Done,
    }

    impl __RuntimeDriverPort for FixturePort {
        type Input = FixtureInput;
        type Output = FixtureOutput;

        fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
            unreachable!("the fixture uses its registered concrete operation")
        }
    }

    fn apply_fixture(
        world: &mut World,
        input: FixtureInput,
    ) -> Result<FixtureOutput, (RuntimeWorldAccessError, FixtureInput)> {
        Ok(match input {
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
                FixtureOutput::Port(world.remove_resource::<UiPostHostPort>().unwrap())
            }
            FixtureInput::ReplacePort => {
                let original = world.remove_resource::<UiPostHostPort>().unwrap();
                world.insert_resource(UiPostHostPort {
                    owner: Arc::new(()),
                    planner: original.planner.clone(),
                });
                FixtureOutput::Port(original)
            }
            FixtureInput::RestorePort(original) => {
                world.insert_resource(original);
                FixtureOutput::Done
            }
        })
    }

    fn mutate(runtime: &mut RuntimeInstance, input: FixtureInput) -> FixtureOutput {
        let outcome = runtime
            .__with_owned_driver_port::<FixturePort>(input)
            .unwrap_or_else(|(error, _)| panic!("fixture scope was refused: {error}"));
        assert!(outcome.post_operation_fault.is_none());
        outcome
            .output
            .unwrap_or_else(|(error, _)| panic!("fixture operation was refused: {error}"))
    }

    fn fixture(fault_after_commit: bool) -> RuntimeInstance {
        let mut app = App::new();
        app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
            .unwrap();
        app.insert_resource(CommitNotifications {
            count: 0,
            fault_after_commit,
        })
        .unwrap();
        app.insert_resource(NativeAccessibilityPresentationPort {
            publish_presented: observe_committed_receipt,
        })
        .unwrap();
        app.insert_resource(FixturePort).unwrap();
        app.__register_driver_port::<FixturePort>(apply_fixture)
            .unwrap();
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
        assert_eq!(runtime.world().resource::<CommitNotifications>().count, 0);
        assert!(latest_presented_ui_observation(runtime.world()).is_none());
    }

    #[test]
    fn captured_original_post_host_connection_refuses_replaced_ledger_without_consumption() {
        let mut runtime = fixture(false);
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
        let mut runtime = fixture(false);
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
        let mut runtime = fixture(false);
        let connection = UiPostHostConnection::capture(&runtime).unwrap().unwrap();
        return_original_presentation(&mut runtime);
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
        assert_eq!(runtime.world().resource::<CommitNotifications>().count, 1);
        runtime.finish().unwrap();
    }

    #[test]
    fn actual_post_host_join_commits_returned_original_once_without_another_frame() {
        let mut runtime = fixture(false);
        let connection = UiPostHostConnection::capture(&runtime).unwrap().unwrap();
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
        assert_eq!(
            runtime
                .world()
                .resource::<UiPresentationBridgeStats>()
                .presented_packets,
            1
        );
        assert_eq!(runtime.world().resource::<CommitNotifications>().count, 1);
        connection.complete(&mut runtime).unwrap();
        assert_eq!(runtime.world().resource::<CommitNotifications>().count, 1);
        assert_eq!(
            runtime.world().resource::<nara_app::RealTime>().frame,
            app_frame
        );
        runtime.finish().unwrap();
    }

    #[test]
    fn actual_post_host_join_retains_consumed_commit_when_output_has_postfault() {
        let mut runtime = fixture(true);
        let connection = UiPostHostConnection::capture(&runtime).unwrap().unwrap();
        let original_receiver = backend_outcome_receiver(runtime.world()).unwrap();
        let identity = return_original_presentation(&mut runtime);

        let error = connection.complete(&mut runtime).unwrap_err();
        assert!(matches!(
            error,
            AppRunError::ManagedRuntime {
                kind: RuntimeFaultKind::RequiredService,
                fault_source: POST_COMMIT_FAULT,
                ..
            }
        ));
        assert_eq!(
            latest_presented_ui_observation(runtime.world())
                .unwrap()
                .submission(),
            identity
        );
        assert_eq!(
            runtime
                .world()
                .resource::<UiPresentationBridgeStats>()
                .presented_packets,
            1
        );
        assert_eq!(runtime.world().resource::<CommitNotifications>().count, 1);
        assert!(!original_receiver.has_unsettled_backend_results());

        // The Root driver has already consumed its WGPU result before this call. A fault means
        // stop/retire; a later connection service cannot replay the settled UI acceptance.
        assert_eq!(runtime.state(), RuntimeState::Faulted);
        connection.complete(&mut runtime).unwrap();
        assert_eq!(runtime.world().resource::<CommitNotifications>().count, 1);
        assert_eq!(
            runtime
                .world()
                .resource::<UiPresentationBridgeStats>()
                .presented_packets,
            1
        );
        runtime.finish().unwrap();
    }
}
