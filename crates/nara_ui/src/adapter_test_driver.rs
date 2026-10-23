use super::*;

use nara_app::{
    __RuntimeDriverPort, RuntimeDriveError, RuntimeDriveOutcome, RuntimeWorldAccessError,
};
use nara_input::{InputAdmission, InputIngressError, InputRouteStamp};

use crate::runtime::{
    UiDeferredPresentationAuthority, UiDeferredPresentationRetry, UiPreparedPresentationAuthority,
};
use crate::{UiActionConsumerError, UiActionSequence, UiSurfaceRetirement, UiSurfaceSuspension};

/// Executes the adapter's actual plugin schedules while keeping mutable execution state private.
pub(super) struct UiTestApp {
    configuration: Option<App>,
    runtime: Option<RuntimeInstance>,
}

impl UiTestApp {
    pub(super) fn new(mut app: App) -> Self {
        app.insert_resource(UiTestDriver).unwrap();
        app.__register_driver_port::<UiTestDriver>(UiTestDriver::apply_driver_input_in_world)
            .unwrap();
        Self {
            configuration: Some(app),
            runtime: None,
        }
    }

    pub(super) fn configuration(&mut self) -> &mut App {
        self.configuration
            .as_mut()
            .expect("configuration must precede execution")
    }

    pub(super) fn configuration_world(&mut self) -> &mut World {
        self.configuration()
            .world_mut()
            .expect("test configuration must permit mutation")
    }

    pub(super) fn into_configuration(mut self) -> App {
        self.configuration
            .take()
            .expect("runtime must not have started")
    }

    pub(super) fn world(&self) -> &World {
        match &self.runtime {
            Some(runtime) => runtime.world(),
            None => self.configuration.as_ref().unwrap().world(),
        }
    }

    pub(super) fn update(&mut self) -> Result<RuntimeDriveOutcome, RuntimeDriveError> {
        if self.runtime.is_none() {
            self.runtime =
                Some(RuntimeInstance::start(self.configuration.take().unwrap()).unwrap());
        }
        self.runtime.as_mut().unwrap().drive(Duration::ZERO)
    }

    fn dispatch(&mut self, command: UiTestCommand) -> UiTestResponse {
        match &mut self.runtime {
            Some(runtime) => runtime
                .with_driver_scope(|scope| scope.__apply_port::<UiTestDriver>(command))
                .map(|outcome| {
                    assert!(outcome.post_operation_fault.is_none());
                    outcome.output
                })
                .unwrap()
                .unwrap(),
            None => command.apply(self.configuration_world()),
        }
    }

    pub(super) fn timeline(&mut self) -> UiTestTimeline<'_> {
        UiTestTimeline(self)
    }

    pub(super) fn acknowledge(
        &mut self,
        cursor: UiActionCursor,
        sequence: UiActionSequence,
    ) -> UiActionCursor {
        let (cursor, result) = self.acknowledge_owned(cursor, sequence);
        result.expect("test action cursor must acknowledge");
        cursor
    }
}

impl Drop for UiTestApp {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime
                .finish()
                .expect("UI test runtime must close completely");
        }
    }
}

#[derive(Resource)]
struct UiTestDriver;

impl __RuntimeDriverPort for UiTestDriver {
    type Input = UiTestCommand;
    type Output = UiTestResponse;

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        unreachable!("the concrete adapter test port coordinates UI-owned resources")
    }
}

impl UiTestDriver {
    fn apply_driver_input_in_world(
        world: &mut World,
        input: UiTestCommand,
    ) -> Result<UiTestResponse, (RuntimeWorldAccessError, UiTestCommand)> {
        Ok(input.apply(world))
    }
}

// The closed operation list admits only concrete UI fixture edits. No command carries a closure,
// World reference, generic resource type, or replacement for managed runtime authority.
macro_rules! ui_test_commands {
    ($($variant:ident => fn $name:ident($($argument:ident: $argument_type:ty),* $(,)?)
        -> $output:ty |$world:ident| $body:block;)*) => {
        enum UiTestCommand { $($variant($($argument_type),*)),* }
        enum UiTestResponse { $($variant($output)),* }

        impl UiTestCommand {
            fn apply(self, world: &mut World) -> UiTestResponse {
                match self {
                    $(Self::$variant($($argument),*) => {
                        let $world = &mut *world;
                        UiTestResponse::$variant($body)
                    }),*
                }
            }
        }

        impl UiTestApp {
            $(pub(super) fn $name(&mut self, $($argument: $argument_type),*) -> $output {
                match self.dispatch(UiTestCommand::$variant($($argument),*)) {
                    UiTestResponse::$variant(output) => output,
                    _ => unreachable!("adapter test command must return its matching response"),
                }
            })*
        }
    };
}

ui_test_commands! {
    InstallSurface => fn install_surface(slot: UiSurfaceSlot) -> Result<(), UiAdapterError> |world| {
        install_ui_surface(world, slot)
    };
    UpdateSurface => fn update_surface(surface: UiSurfaceId, endpoint: UiSurfaceEndpoint, transform: UiSurfaceContentTransform)
        -> Result<UiSurfaceSlot, UiAdapterError> |world| { update_ui_surface_slot(world, surface, endpoint, transform) };
    SuspendSurface => fn suspend_surface(surface: UiSurfaceId, reason: UiSurfaceSuspensionReason)
        -> Result<Option<UiSurfaceSuspension>, UiAdapterError> |world| { suspend_ui_surface(world, surface, reason) };
    RestoreSurface => fn restore_surface(surface: UiSurfaceId, endpoint: UiSurfaceEndpoint, transform: UiSurfaceContentTransform)
        -> Result<UiSurfaceSlot, UiAdapterError> |world| { restore_ui_surface(world, surface, endpoint, transform) };
    RetireSurface => fn retire_surface(surface: UiSurfaceId) -> Result<Option<UiSurfaceRetirement>, UiAdapterError> |world| {
        retire_ui_surface(world, surface)
    };
    PreparePresentation => fn prepare_presentation(frame: Arc<nara_ui_kernel::UiFrame>, epoch: u64)
        -> Result<UiPreparedPresentationAuthority, UiAdapterError> |world| { crate::__private::prepare_ui_presentation(world, frame, epoch) };
    CommitPresentation => fn commit_presentation(prepared: UiPreparedPresentationAuthority) -> UiPresentationReceipt |world| {
        crate::__private::commit_prepared_ui_presentation(world, prepared)
    };
    AbortPresentation => fn abort_presentation(prepared: UiPreparedPresentationAuthority) -> () |world| {
        crate::__private::abort_prepared_ui_presentation(world, prepared)
    };
    DeferPresentation => fn defer_presentation(prepared: UiPreparedPresentationAuthority) -> UiDeferredPresentationAuthority |world| {
        crate::__private::defer_prepared_ui_presentation(world, prepared)
    };
    RetryPresentation => fn retry_presentation(deferred: UiDeferredPresentationAuthority)
        -> Result<UiDeferredPresentationRetry, UiAdapterError> |world| { crate::__private::retry_deferred_ui_presentation(world, deferred) };
    PresentFrame => fn present_frame(frame: Arc<nara_ui_kernel::UiFrame>, epoch: u64) -> Result<UiPresentationReceipt, UiAdapterError> |world| {
        commit_ui_presentation(world, frame, epoch)
    };
    DispatchInput => fn dispatch_input(route: UiRouteStamp, input: UiInputEvent) -> UiRouteDisposition |world| {
        dispatch_presented_ui_input(world, route, input)
    };
    DispatchAccessibility => fn dispatch_accessibility(input: UiAccessibilityInput) -> UiRouteDisposition |world| {
        dispatch_ui_accessibility_input(world, input)
    };
    RegisterConsumer => fn register_consumer(consumer: UiActionConsumerId) -> Result<UiActionCursor, UiActionConsumerError> |world| {
        world.resource_mut::<UiActionJournal>().register_consumer(consumer)
    };
    Acknowledge => fn acknowledge_owned(cursor: UiActionCursor, sequence: UiActionSequence)
        -> (UiActionCursor, Result<(), UiActionConsumerError>) |world| {
        let result = world.resource_mut::<UiActionJournal>().acknowledge(&cursor, sequence);
        (cursor, result)
    };
    ReplaceJournal => fn replace_journal(journal: UiActionJournal) -> () |world| { world.insert_resource(journal); };
    ClearGameplayCapture => fn clear_gameplay_capture() -> () |world| { world.resource_mut::<CapturedGameplayPhases>().0.clear(); };
    SetNode => fn set_node(entity: Entity, node: UiNode) -> () |world| { world.entity_mut(entity).insert(node); };
    SetButton => fn set_button(entity: Entity, button: UiButton) -> () |world| { world.entity_mut(entity).insert(button); };
    SetRoot => fn set_root(entity: Entity, root: UiRoot) -> () |world| { world.entity_mut(entity).insert(root); };
    ReplaceButtonKey => fn replace_button_key(entity: Entity, key: UiRuntimeKey, button: UiButton) -> () |world| { world.entity_mut(entity).insert((key, button)); };
    RemoveVisual => fn remove_visual(entity: Entity) -> () |world| { world.entity_mut(entity).remove::<UiVisual>(); };
    MissingIdentityRoot => fn spawn_missing_identity_root() -> Entity |world| {
        world.spawn((UiRoot::new(UiLayer::Base), UiNode::new(UiStyle::fill()))).id()
    };
    InvalidModal => fn place_invalid_modal(root_local: String, button_local: String) -> () |world| {
        place_invalid_button_modal(world, &root_local, &button_local)
    };
    PlaceEntity => fn place_entity(entity: Entity, placement: HierarchyPlacement) -> Result<(), nara_hierarchy::HierarchyError> |world| {
        HierarchyWriter::new(world).place(entity, placement)
    };
    RemoveEntity => fn remove_entity(entity: Entity, previous: HierarchyPlacement) -> Result<(), nara_hierarchy::HierarchyError> |world| {
        HierarchyWriter::new(world).remove(entity, previous)
    };
    PublishRoute => fn publish_presented_route(route: InputPresentedRoute) -> Result<InputRouteStamp, InputIngressError> |world| {
        world.resource_mut::<InputTimeline>().publish_presented_route(route)
    };
    AdmitKeyboard => fn admit_keyboard(window: InputWindowId, seat: InputSeatId, input: ButtonDriverInput<KeyCode>)
        -> Result<InputAdmission, InputIngressError> |world| { world.resource_mut::<InputTimeline>().admit_keyboard(window, seat, input) };
    AdmitMouse => fn admit_mouse_button(window: InputWindowId, seat: InputSeatId, pointer: InputPointerId, input: ButtonDriverInput<MouseButton>)
        -> Result<InputAdmission, InputIngressError> |world| { world.resource_mut::<InputTimeline>().admit_mouse_button(window, seat, pointer, input) };
    AdmitPointer => fn admit_pointer(window: InputWindowId, seat: InputSeatId, pointer: InputPointerId, input: PointerDriverInput)
        -> Result<InputAdmission, InputIngressError> |world| { world.resource_mut::<InputTimeline>().admit_pointer(window, seat, pointer, input) };
    AdmitWindow => fn admit_window_event(window: InputWindowId, seat: InputSeatId, event: InputWindowEvent)
        -> Result<InputAdmission, InputIngressError> |world| { world.resource_mut::<InputTimeline>().admit_window_event(window, seat, event) };
    AdmitWheel => fn admit_wheel(window: InputWindowId, seat: InputSeatId, pointer: InputPointerId, delta: Vec2)
        -> Result<InputAdmission, InputIngressError> |world| { world.resource_mut::<InputTimeline>().admit_wheel(window, seat, pointer, delta) };
    AdmitGamepad => fn admit_gamepad_button(window: InputWindowId, seat: InputSeatId, gamepad: u16, button: u16, phase: ButtonTransitionPhase)
        -> Result<InputAdmission, InputIngressError> |world| { world.resource_mut::<InputTimeline>().admit_gamepad_button(window, seat, gamepad, button, phase) };
}

pub(super) struct UiTestTimeline<'a>(&'a mut UiTestApp);

impl std::ops::Deref for UiTestTimeline<'_> {
    type Target = UiTestApp;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl std::ops::DerefMut for UiTestTimeline<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl UiTestTimeline<'_> {
    pub(super) fn retained_len(&self) -> usize {
        self.0.world().resource::<InputTimeline>().retained_len()
    }
}
