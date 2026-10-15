use std::time::Duration;

use nara_app::{
    __RuntimeDriverPort, RuntimeDriveError, RuntimeDriveOutcome, RuntimeInstance,
    RuntimeWorldAccessError,
};
use nara_ui_render::__private::{UiBackendPresentationFrame, UiCompletedBackendOutcome};
use nara_ui_render::{UiPaintCompileStats, UiPresentationIntegrationError};

use super::*;

pub(super) struct BridgeRuntime(Option<RuntimeInstance>);

impl BridgeRuntime {
    pub(super) fn start(mut app: App) -> Self {
        app.insert_resource(BridgeTestDriver)
            .unwrap()
            .__register_driver_port::<BridgeTestDriver>(
                BridgeTestDriver::apply_driver_input_in_world,
            )
            .unwrap();
        Self(Some(
            RuntimeInstance::start(app).expect("the bridge test runtime should start"),
        ))
    }

    pub(super) fn world(&self) -> &World {
        self.0.as_ref().unwrap().world()
    }

    pub(super) fn update(&mut self) -> Result<RuntimeDriveOutcome, RuntimeDriveError> {
        self.0.as_mut().unwrap().drive(Duration::ZERO)
    }

    pub(super) fn resize_primary_window(&mut self, resolution: nara_window::WindowResolution) {
        self.0
            .as_mut()
            .unwrap()
            .with_driver_scope(|scope| {
                nara_window::push_window_driver_event(
                    scope,
                    nara_window::WindowEvent::Resized {
                        window_id: nara_window::WindowId::PRIMARY,
                        resolution,
                    },
                )
            })
            .unwrap()
            .unwrap();
    }

    fn apply(&mut self, operation: BridgeTestOperation) -> BridgeTestResult {
        self.0
            .as_mut()
            .unwrap()
            .with_driver_scope(|scope| scope.__apply_port::<BridgeTestDriver>(operation))
            .unwrap()
            .unwrap()
    }

    pub(super) fn set_backend_mode(&mut self, mode: FakeBackendMode) {
        assert!(matches!(
            self.apply(BridgeTestOperation::SetBackendMode(mode)),
            BridgeTestResult::Done
        ));
    }

    pub(super) fn set_visual(&mut self, entity: Entity, visual: UiVisual) {
        assert!(matches!(
            self.apply(BridgeTestOperation::SetVisual(entity, visual)),
            BridgeTestResult::Done
        ));
    }

    pub(super) fn remove_primary_window(&mut self) -> nara_window::Window {
        let BridgeTestResult::RemovedWindow(window) =
            self.apply(BridgeTestOperation::RemovePrimaryWindow)
        else {
            unreachable!("the primary-window removal must return its retained value")
        };
        window
    }

    pub(super) fn restore_primary_window(&mut self, window: nara_window::Window) {
        assert!(matches!(
            self.apply(BridgeTestOperation::RestorePrimaryWindow(window)),
            BridgeTestResult::Done
        ));
    }

    pub(super) fn retire_surface(
        &mut self,
        surface: UiSurfaceId,
    ) -> Result<Option<nara_ui::UiSurfaceRetirement>, nara_ui::UiAdapterError> {
        let BridgeTestResult::Retirement(result) =
            self.apply(BridgeTestOperation::RetireSurface(surface))
        else {
            unreachable!("surface retirement must return its exact result")
        };
        result
    }

    pub(super) fn compile_packets(
        &mut self,
        frames: Vec<Arc<nara_ui::advanced::UiFrame>>,
    ) -> Result<UiPaintCompileStats, UiPresentationIntegrationError> {
        let BridgeTestResult::Compilation(result) =
            self.apply(BridgeTestOperation::CompilePackets(frames))
        else {
            unreachable!("packet compilation must return its exact result")
        };
        result
    }

    pub(super) fn take_backend_frame(
        &mut self,
    ) -> Result<UiBackendPresentationFrame, UiPresentationIntegrationError> {
        let BridgeTestResult::BackendFrame(result) =
            self.apply(BridgeTestOperation::TakeBackendFrame)
        else {
            unreachable!("backend extraction must return its move-only frame")
        };
        result
    }

    pub(super) fn commit_backend_outcome(
        &mut self,
        outcome: UiCompletedBackendOutcome,
    ) -> Result<(), UiCompletedBackendOutcome> {
        let BridgeTestResult::BackendOutcome(result) =
            self.apply(BridgeTestOperation::CommitBackendOutcome(outcome))
        else {
            unreachable!("backend completion must preserve its exact outcome")
        };
        result
    }

    pub(super) fn commit_presentations(&mut self) {
        assert!(matches!(
            self.apply(BridgeTestOperation::CommitPresentations),
            BridgeTestResult::Done
        ));
    }
}

impl Drop for BridgeRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.0.take() {
            runtime
                .finish()
                .expect("the bridge test runtime should close completely");
        }
    }
}

// Test injection remains a closed list of adapter operations, without a mutable World callback.
enum BridgeTestOperation {
    SetBackendMode(FakeBackendMode),
    SetVisual(Entity, UiVisual),
    RemovePrimaryWindow,
    RestorePrimaryWindow(nara_window::Window),
    RetireSurface(UiSurfaceId),
    CompilePackets(Vec<Arc<nara_ui::advanced::UiFrame>>),
    TakeBackendFrame,
    CommitBackendOutcome(UiCompletedBackendOutcome),
    CommitPresentations,
}

enum BridgeTestResult {
    Done,
    RemovedWindow(nara_window::Window),
    Retirement(Result<Option<nara_ui::UiSurfaceRetirement>, nara_ui::UiAdapterError>),
    Compilation(Result<UiPaintCompileStats, UiPresentationIntegrationError>),
    BackendFrame(Result<UiBackendPresentationFrame, UiPresentationIntegrationError>),
    BackendOutcome(Result<(), UiCompletedBackendOutcome>),
}

#[derive(Resource)]
struct BridgeTestDriver;

impl __RuntimeDriverPort for BridgeTestDriver {
    type Input = BridgeTestOperation;
    type Output = BridgeTestResult;

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        unreachable!("the bridge fixture coordinates concrete adapter resources")
    }
}

impl BridgeTestDriver {
    fn apply_driver_input_in_world(
        world: &mut World,
        operation: BridgeTestOperation,
    ) -> Result<BridgeTestResult, (RuntimeWorldAccessError, BridgeTestOperation)> {
        Ok(match operation {
            BridgeTestOperation::SetBackendMode(mode) => {
                *world.resource_mut::<FakeBackendMode>() = mode;
                BridgeTestResult::Done
            }
            BridgeTestOperation::SetVisual(entity, visual) => {
                world.entity_mut(entity).insert(visual);
                BridgeTestResult::Done
            }
            BridgeTestOperation::RemovePrimaryWindow => {
                let mut query = world.query::<(Entity, &nara_window::Window)>();
                let (entity, window) = query
                    .iter(world)
                    .find(|(_, window)| window.id == nara_window::WindowId::PRIMARY)
                    .expect("the primary window should remain installed");
                let window = window.clone();
                assert!(world.despawn(entity));
                BridgeTestResult::RemovedWindow(window)
            }
            BridgeTestOperation::RestorePrimaryWindow(window) => {
                world.spawn((window, nara_window::PrimaryWindow));
                BridgeTestResult::Done
            }
            BridgeTestOperation::RetireSurface(surface) => {
                BridgeTestResult::Retirement(nara_ui::retire_ui_surface(world, surface))
            }
            BridgeTestOperation::CompilePackets(frames) => BridgeTestResult::Compilation(
                nara_ui_render::compile_ui_paint_packets(world, &frames),
            ),
            BridgeTestOperation::TakeBackendFrame => BridgeTestResult::BackendFrame(
                nara_ui_render::__private::take_ui_backend_presentation_frame(world),
            ),
            BridgeTestOperation::CommitBackendOutcome(outcome) => BridgeTestResult::BackendOutcome(
                nara_ui_render::__private::commit_ui_backend_outcome(world, outcome),
            ),
            BridgeTestOperation::CommitPresentations => {
                commit_runtime_ui_presentations(world);
                BridgeTestResult::Done
            }
        })
    }
}
