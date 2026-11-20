use std::{
    sync::Mutex,
    thread::{self, ThreadId},
    time::Duration,
};

use gilrs::{Axis, Button, ev::Code};
use nara_app::{
    RuntimeCandidateRetirementState, RuntimeClosePolicy, RuntimeInstance, RuntimeState,
    StartupStage,
};
use nara_input::{
    ButtonTransitionPhase, InputEventKind, InputLimits, InputPlugin, InputSeatId, InputWindowEvent,
    InputWindowId,
};

use super::*;

mod handoff;

const WINDOW: InputWindowId = InputWindowId::new(7);
const ROUTE: GamepadRoute = GamepadRoute::new(WINDOW, InputSeatId::PRIMARY);

fn code() -> Code {
    Button::South.to_nec().unwrap()
}

fn press(button: Button) -> EventType {
    EventType::ButtonPressed(button, code())
}
fn release(button: Button) -> EventType {
    EventType::ButtonReleased(button, code())
}
fn axis(value: f32) -> EventType {
    EventType::AxisChanged(Axis::LeftStickX, value, code())
}

struct Fixture {
    bridge: Bridge,
    devices: GilrsDevices,
    timeline: InputTimeline,
}

impl Fixture {
    fn new(max_devices: usize) -> Self {
        let (bridge, devices) = Bridge::new(max_devices, Some(ROUTE));
        let mut timeline = InputTimeline::default();
        timeline.connect_endpoint(WINDOW, ROUTE.seat).unwrap();
        timeline
            .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::Focused(true))
            .unwrap();
        Self {
            bridge,
            devices,
            timeline,
        }
    }

    fn connect(&mut self, id: usize, state: PhysicalState) {
        self.bridge
            .connected(id, "Controller", state, &mut self.devices)
            .unwrap();
    }

    fn poll_boundary(&mut self, drained: bool) {
        self.bridge
            .reconcile(&self.devices, &mut self.timeline, drained)
            .unwrap();
    }

    fn event(&mut self, id: usize, event: EventType) {
        self.bridge.event(id, event, &mut self.timeline).unwrap();
    }

    fn kinds(&self) -> Vec<InputEventKind> {
        self.timeline.entries().map(|e| e.kind()).collect()
    }

    fn presses(&self) -> usize {
        self.kinds()
            .iter()
            .filter(|kind| {
                matches!(
                    kind,
                    InputEventKind::GamepadButton {
                        phase: ButtonTransitionPhase::Pressed,
                        ..
                    }
                )
            })
            .count()
    }

    fn samples(&self) -> Vec<f32> {
        self.kinds()
            .iter()
            .filter_map(|kind| match kind {
                InputEventKind::GamepadAxis { value, .. } => Some(value.get()),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn startup_snapshot_and_queued_events_require_a_drain_boundary_without_replaying_holds() {
    let mut f = Fixture::new(2);
    let mut snapshot = PhysicalState::default();
    snapshot.buttons[GamepadButton::South.code() as usize] = true;
    snapshot.axes[GamepadAxis::LeftStickX.code() as usize] =
        nara_input::AxisValue::new(0.8).unwrap();
    f.connect(0, snapshot);
    f.event(0, press(Button::East));
    f.poll_boundary(false);
    assert!(
        !f.kinds()
            .iter()
            .any(|k| matches!(k, InputEventKind::GamepadConnected { .. }))
    );
    f.poll_boundary(true);
    f.event(0, press(Button::South));
    f.event(0, press(Button::East));
    f.event(0, axis(0.9));
    assert_eq!(f.presses(), 0);
    assert!(f.samples().is_empty());
    f.event(0, release(Button::South));
    f.event(0, release(Button::East));
    f.event(0, axis(0.0));
    f.event(0, press(Button::South));
    f.event(0, axis(-0.7));
    assert_eq!(f.presses(), 1);
    assert_eq!(f.samples(), [-0.7]);
    assert_eq!(f.devices.connected().len(), 1);
    assert_eq!(f.devices.connected()[0].name(), "Controller");
}

#[test]
fn fresh_unknown_buttons_work_and_changed_repeat_and_duplicates_do_not_double_inject() {
    let mut f = Fixture::new(1);
    f.connect(0, PhysicalState::default());
    f.poll_boundary(true);
    f.event(0, press(Button::South));
    f.event(0, EventType::ButtonChanged(Button::South, 1.0, code()));
    f.event(0, EventType::ButtonRepeated(Button::South, code()));
    f.event(0, press(Button::South));
    f.event(0, press(Button::Unknown));
    f.event(0, axis(0.5));
    f.event(0, axis(0.5));
    f.event(0, axis(0.0));
    f.event(0, release(Button::South));
    assert_eq!(f.presses(), 1);
    assert_eq!(f.samples(), [0.5, 0.0]);
    assert_eq!(
        f.kinds()
            .iter()
            .filter(|k| matches!(
                k,
                InputEventKind::GamepadButton {
                    phase: ButtonTransitionPhase::Released,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(
        f.timeline
            .entries()
            .filter(|e| matches!(e.kind(), InputEventKind::GamepadButton { .. }))
            .all(|e| e.route().window() == WINDOW && e.route().seat() == ROUTE.seat)
    );
}

#[test]
fn rapid_focus_loss_and_regain_cannot_readopt_known_buttons_or_axes() {
    let mut f = Fixture::new(1);
    f.connect(0, PhysicalState::default());
    f.poll_boundary(true);
    f.event(0, press(Button::South));
    f.event(0, axis(0.6));
    let first = f.timeline.endpoint_observation(WINDOW, ROUTE.seat).unwrap();
    f.timeline
        .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::Focused(false))
        .unwrap();
    f.timeline
        .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::Focused(true))
        .unwrap();
    let current = f.timeline.endpoint_observation(WINDOW, ROUTE.seat).unwrap();
    assert_eq!(first.incarnation(), current.incarnation());
    assert!(current.focused());
    assert!(current.invalidated_through() > first.invalidated_through());
    f.poll_boundary(false);
    f.event(0, press(Button::South));
    f.event(0, axis(0.8));
    f.poll_boundary(true);
    f.event(0, press(Button::South));
    f.event(0, axis(0.9));
    assert_eq!(f.presses(), 1);
    assert_eq!(f.samples(), [0.6, 0.0]);
    f.event(0, release(Button::South));
    f.event(0, axis(0.0));
    f.event(0, press(Button::South));
    f.event(0, axis(0.7));
    assert_eq!(f.presses(), 2);
    assert_eq!(f.samples(), [0.6, 0.0, 0.7]);
}

#[test]
fn same_seat_focus_round_trip_between_polls_interrupts_axis_continuity() {
    for seat in [ROUTE.seat, InputSeatId::new(2)] {
        let mut f = Fixture::new(1);
        let other = InputWindowId::new(8);
        f.timeline.connect_endpoint(other, seat).unwrap();
        f.connect(0, PhysicalState::default());
        f.poll_boundary(true);
        f.event(0, axis(0.6));
        let first = f.timeline.endpoint_observation(WINDOW, ROUTE.seat).unwrap();
        f.timeline
            .admit_window_event(other, seat, InputWindowEvent::Focused(true))
            .unwrap();
        f.timeline
            .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::Focused(true))
            .unwrap();
        let restored = f.timeline.endpoint_observation(WINDOW, ROUTE.seat).unwrap();
        assert_eq!(restored.incarnation(), first.incarnation());
        assert!(restored.focused());

        f.poll_boundary(false);
        f.event(0, axis(0.8));
        if seat == ROUTE.seat {
            assert!(restored.invalidated_through() > first.invalidated_through());
            assert_eq!(f.samples(), [0.6, 0.0]);
            f.poll_boundary(true);
            f.event(0, axis(0.9));
            assert_eq!(f.samples(), [0.6, 0.0]);
            f.event(0, axis(0.0));
            f.event(0, axis(0.7));
            assert_eq!(f.samples(), [0.6, 0.0, 0.7]);
        } else {
            assert_eq!(restored.invalidated_through(), first.invalidated_through());
            assert_eq!(f.samples(), [0.6, 0.8]);
        }
    }
}

#[test]
fn no_endpoint_or_unfocused_endpoint_never_creates_an_implicit_route() {
    let (mut bridge, mut devices) = Bridge::new(1, Some(ROUTE));
    let mut timeline = InputTimeline::default();
    bridge
        .connected(0, "Pad", PhysicalState::default(), &mut devices)
        .unwrap();
    bridge.reconcile(&devices, &mut timeline, true).unwrap();
    timeline.connect_endpoint(WINDOW, ROUTE.seat).unwrap();
    bridge.reconcile(&devices, &mut timeline, true).unwrap();
    bridge
        .event(0, press(Button::South), &mut timeline)
        .unwrap();
    assert_eq!(timeline.entries().count(), 0);
    timeline
        .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::Focused(true))
        .unwrap();
    bridge.reconcile(&devices, &mut timeline, true).unwrap();
    bridge
        .event(0, press(Button::South), &mut timeline)
        .unwrap();
    assert!(
        !timeline
            .entries()
            .any(|e| matches!(e.kind(), InputEventKind::GamepadButton { .. }))
    );
}

#[test]
fn explicit_two_seat_assignment_and_same_route_unassign_reassign_preserve_barriers() {
    let mut f = Fixture::new(2);
    let second = GamepadRoute::new(InputWindowId::new(8), InputSeatId::new(2));
    f.timeline
        .connect_endpoint(second.window, second.seat)
        .unwrap();
    f.timeline
        .admit_window_event(second.window, second.seat, InputWindowEvent::Focused(true))
        .unwrap();
    f.connect(0, PhysicalState::default());
    f.connect(1, PhysicalState::default());
    let first_token = f.devices.connected()[0].token();
    let second_token = f.devices.connected()[1].token();
    f.devices.assign(&second_token, second).unwrap();
    f.poll_boundary(true);
    f.event(0, press(Button::South));
    f.event(1, press(Button::South));
    let seats: Vec<_> = f
        .timeline
        .entries()
        .filter(|e| {
            matches!(
                e.kind(),
                InputEventKind::GamepadButton {
                    phase: ButtonTransitionPhase::Pressed,
                    ..
                }
            )
        })
        .map(|e| e.route().seat())
        .collect();
    assert_eq!(seats, [ROUTE.seat, second.seat]);
    f.devices.unassign(&first_token).unwrap();
    f.devices.assign(&first_token, ROUTE).unwrap();
    f.poll_boundary(false);
    f.poll_boundary(true);
    f.event(0, press(Button::South));
    assert_eq!(f.presses(), 2);
    f.event(0, release(Button::South));
    f.event(0, press(Button::South));
    assert_eq!(f.presses(), 3);
    f.devices.unassign(&second_token).unwrap();
    f.poll_boundary(true);
    f.event(1, release(Button::South));
    f.event(1, press(Button::South));
    assert_eq!(f.presses(), 3);
    f.devices.assign(&first_token, second).unwrap();
    f.poll_boundary(true);
    f.event(0, press(Button::South));
    assert_eq!(f.presses(), 3);
    f.event(0, release(Button::South));
    f.event(0, press(Button::South));
    assert_eq!(
        f.timeline.entries().last().unwrap().route().seat(),
        second.seat
    );
}

#[test]
fn duplicate_connected_preserves_original_token_assignment_and_held_state() {
    let mut f = Fixture::new(1);
    f.connect(usize::MAX, PhysicalState::default());
    let token = f.devices.connected()[0].token();
    f.devices.unassign(&token).unwrap();
    f.connect(usize::MAX, PhysicalState::default());
    assert_eq!(f.devices.connected()[0].token(), token);
    assert_eq!(f.devices.connected()[0].route(), None);
    f.devices.assign(&token, ROUTE).unwrap();
    f.poll_boundary(true);
    f.event(usize::MAX, press(Button::South));
    f.connect(usize::MAX, PhysicalState::default());
    f.poll_boundary(true);
    assert_eq!(f.devices.connected()[0].token(), token);
    assert_eq!(f.devices.connected()[0].route(), Some(ROUTE));
    assert_eq!(
        f.kinds()
            .iter()
            .filter(|k| matches!(k, InputEventKind::GamepadConnected { .. }))
            .count(),
        1
    );
    f.timeline
        .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::Focused(false))
        .unwrap();
    f.timeline
        .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::Focused(true))
        .unwrap();
    f.poll_boundary(true);
    f.event(usize::MAX, press(Button::South));
    assert_eq!(f.presses(), 1);
    f.event(usize::MAX, release(Button::South));
    f.event(usize::MAX, press(Button::South));
    assert_eq!(f.presses(), 2);
}

#[test]
fn reconnect_renews_tokens_and_generations_and_refuses_stale_or_foreign_assignment() {
    let mut f = Fixture::new(1);
    f.connect(0, PhysicalState::default());
    f.poll_boundary(true);
    let old = f.devices.connected()[0].token();
    f.event(0, press(Button::South));
    f.event(0, axis(0.4));
    f.bridge
        .disconnected(0, &mut f.devices, &mut f.timeline)
        .unwrap();
    assert!(f.devices.connected().is_empty());
    assert_eq!(f.samples(), [0.4, 0.0]);
    let mut reconnected = PhysicalState::default();
    reconnected.buttons[GamepadButton::South.code() as usize] = true;
    reconnected.axes[GamepadAxis::LeftStickX.code() as usize] =
        nara_input::AxisValue::new(0.4).unwrap();
    f.connect(0, reconnected);
    f.poll_boundary(true);
    f.event(0, press(Button::South));
    f.event(0, axis(0.7));
    assert_eq!(f.presses(), 1);
    assert_eq!(f.samples(), [0.4, 0.0]);
    let new = f.devices.connected()[0].token();
    assert_ne!(old, new);
    assert_eq!(
        f.devices.assign(&old, ROUTE),
        Err(GamepadAssignmentError::StaleToken)
    );
    let mut foreign = Fixture::new(1);
    foreign.connect(0, PhysicalState::default());
    assert_ne!(new, foreign.devices.connected()[0].token());
    assert_eq!(
        foreign.devices.unassign(&new),
        Err(GamepadAssignmentError::ForeignToken)
    );
    let generations: Vec<_> = f
        .kinds()
        .iter()
        .filter_map(|kind| match kind {
            InputEventKind::GamepadConnected {
                gamepad,
                connection_generation,
            } => Some((*gamepad, *connection_generation)),
            _ => None,
        })
        .collect();
    assert_eq!(generations[0].0, generations[1].0);
    assert_ne!(generations[0].1, generations[1].1);
    f.event(0, release(Button::South));
    f.event(0, axis(0.0));
    f.event(0, press(Button::South));
    assert_eq!(f.presses(), 2);
}

#[test]
fn endpoint_retirement_and_replacement_keep_old_holds_suppressed() {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, nara_ecs::ScheduleLabel)]
    struct ReplaceEndpoint;

    let mut f = Fixture::new(1);
    f.connect(0, PhysicalState::default());
    f.poll_boundary(true);
    f.event(0, press(Button::South));
    let incarnation = f
        .timeline
        .endpoint_observation(WINDOW, ROUTE.seat)
        .unwrap()
        .incarnation();
    f.timeline
        .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::SurfaceRetired)
        .unwrap();
    f.poll_boundary(false);
    let mut app = App::new();
    app.insert_resource(f.timeline).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    app.add_systems(
        ReplaceEndpoint,
        move |mut timeline: ResMut<InputTimeline>| {
            timeline.connect_endpoint(WINDOW, ROUTE.seat).unwrap();
            timeline
                .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::Focused(true))
                .unwrap();
            assert_ne!(
                timeline
                    .endpoint_observation(WINDOW, ROUTE.seat)
                    .unwrap()
                    .incarnation(),
                incarnation
            );
            f.bridge.reconcile(&f.devices, &mut timeline, true).unwrap();
            f.bridge
                .event(0, press(Button::South), &mut timeline)
                .unwrap();
            assert!(!timeline.entries().any(|e| matches!(
                e.kind(),
                InputEventKind::GamepadButton {
                    phase: ButtonTransitionPhase::Pressed,
                    ..
                }
            )));
            f.bridge
                .event(0, release(Button::South), &mut timeline)
                .unwrap();
            f.bridge
                .event(0, press(Button::South), &mut timeline)
                .unwrap();
            assert!(timeline.entries().any(|e| matches!(
                e.kind(),
                InputEventKind::GamepadButton {
                    phase: ButtonTransitionPhase::Pressed,
                    ..
                }
            )));
            f.bridge.close(&mut timeline).unwrap();
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    runtime.run_schedule(ReplaceEndpoint).unwrap();
    runtime.finish().unwrap();
}

#[test]
fn finite_device_and_input_capacity_fail_without_losing_accepted_terminals() {
    let mut f = Fixture::new(1);
    f.connect(0, PhysicalState::default());
    let token = f.devices.connected()[0].token();
    assert!(matches!(
        f.bridge
            .connected(1, "Extra", PhysicalState::default(), &mut f.devices),
        Err(BridgeError::DeviceCapacity)
    ));
    assert_eq!(f.devices.connected()[0].token(), token);
    f.timeline = InputTimeline::with_limits(InputLimits::new(32, 4, 4, 1).unwrap()).unwrap();
    f.timeline.connect_endpoint(WINDOW, ROUTE.seat).unwrap();
    f.timeline
        .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::Focused(true))
        .unwrap();
    f.poll_boundary(true);
    f.event(0, press(Button::South));
    assert!(matches!(
        f.bridge.event(0, press(Button::East), &mut f.timeline),
        Err(BridgeError::Ingress(
            InputIngressError::MemberCapacityExceeded { limit: 1 }
        ))
    ));
    f.bridge.close(&mut f.timeline).unwrap();
    let kinds = f.kinds();
    assert!(matches!(
        kinds[kinds.len() - 2],
        InputEventKind::GamepadButton {
            button: 0,
            phase: ButtonTransitionPhase::Released,
            ..
        }
    ));
    assert!(matches!(
        kinds.last(),
        Some(InputEventKind::GamepadDisconnected { .. })
    ));
    let before = f.timeline.entries().count();
    f.bridge.close(&mut f.timeline).unwrap();
    assert_eq!(f.timeline.entries().count(), before);
}

#[test]
fn zero_gamepad_admission_and_invalid_axis_and_inventory_fail_explicitly() {
    let mut f = Fixture::new(1);
    f.timeline =
        InputTimeline::with_limits(InputLimits::default().with_gamepads(0).unwrap()).unwrap();
    f.timeline.connect_endpoint(WINDOW, ROUTE.seat).unwrap();
    f.timeline
        .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::Focused(true))
        .unwrap();
    f.connect(0, PhysicalState::default());
    assert!(matches!(
        f.bridge.reconcile(&f.devices, &mut f.timeline, true),
        Err(BridgeError::Ingress(
            InputIngressError::GamepadCapacityExceeded { limit: 0 }
        ))
    ));
    assert!(matches!(
        f.bridge.event(0, axis(f32::NAN), &mut f.timeline),
        Err(BridgeError::Axis(_))
    ));
    let (_, replacement) = Bridge::new(1, None);
    assert!(matches!(
        f.bridge.reconcile(&replacement, &mut f.timeline, true),
        Err(BridgeError::InventoryReplaced)
    ));
}

#[test]
fn portable_codes_map_deliberately_and_configuration_has_no_native_side_effects() {
    use crate::controls::{BUTTONS, button_from_native};

    assert_eq!(
        button_from_native(Button::LeftTrigger),
        Some(GamepadButton::LeftShoulder)
    );
    assert_eq!(
        button_from_native(Button::LeftTrigger2),
        Some(GamepadButton::LeftTrigger)
    );
    for (index, &(button, native)) in BUTTONS.iter().enumerate() {
        assert_eq!(usize::from(button.code()), index);
        assert_eq!(GamepadButton::from_code(button.code()), Some(button));
        assert_eq!(button_from_native(native), Some(button));
    }
    for (native, expected_code) in [
        (Button::South, 0),
        (Button::RightThumb, 12),
        (Button::DPadUp, 13),
        (Button::DPadDown, 14),
        (Button::DPadLeft, 15),
        (Button::DPadRight, 16),
    ] {
        assert_eq!(button_from_native(native).unwrap().code(), expected_code);
    }
    assert_eq!(GamepadButton::from_code(u16::MAX), None);
    for &axis in GamepadAxis::ALL {
        assert_eq!(GamepadAxis::from_native(axis.native()), Some(axis));
    }
    assert_eq!(button_from_native(Button::Unknown), None);
    assert_eq!(GamepadAxis::from_native(Axis::Unknown), None);
    let plugin = GilrsInputPlugin::new(Some(ROUTE));
    assert_eq!(plugin.default_route, Some(ROUTE));
    assert_eq!(plugin.max_events_per_frame, 256);
    let mut app = App::new();
    app.add_plugin(InputPlugin).unwrap();
    assert!(
        app.add_plugin(GilrsInputPlugin {
            max_events_per_frame: 0,
            ..plugin
        })
        .is_err()
    );
}

pub(super) struct DropProbe(Arc<Mutex<Vec<ThreadId>>>);

impl Drop for DropProbe {
    fn drop(&mut self) {
        self.0.lock().unwrap().push(thread::current().id());
    }
}

struct CloseProbePlugin {
    drops: Arc<Mutex<Vec<ThreadId>>>,
    fail_stage: Option<FailurePoint>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FailurePoint {
    Startup,
    Runtime,
}

impl Plugin for CloseProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let (mut bridge, mut devices) = Bridge::new(1, Some(ROUTE));
        {
            let mut timeline = app.world_mut()?.resource_mut::<InputTimeline>();
            timeline.connect_endpoint(WINDOW, ROUTE.seat).unwrap();
            timeline
                .admit_window_event(WINDOW, ROUTE.seat, InputWindowEvent::Focused(true))
                .unwrap();
            bridge
                .connected(0, "Probe", PhysicalState::default(), &mut devices)
                .unwrap();
            bridge.reconcile(&devices, &mut timeline, true).unwrap();
            bridge
                .event(0, press(Button::South), &mut timeline)
                .unwrap();
        }
        let native = Rc::new(RefCell::new(NativeState {
            gilrs: None,
            bridge,
            budget: 1,
            drop_probe: Some(DropProbe(Arc::clone(&self.drops))),
        }));
        app.register_plugin_runtime_close_participant(
            CLOSE,
            RuntimeCloseParticipantId::new("nara.gilrs.native"),
            NativeClose(Rc::clone(&native)),
        )?;
        app.insert_resource(devices)?;
        app.world_mut()?.insert_non_send(NativeGilrs(native));
        match self.fail_stage {
            Some(FailurePoint::Startup) => {
                app.add_systems(StartupStage::Platform, injected_failure)?;
            }
            Some(FailurePoint::Runtime) => {
                app.add_systems(CoreStage::First, injected_failure)?;
            }
            None => {}
        }
        Ok(())
    }
}

fn injected_failure() -> Result<(), BevyError> {
    Err(RuntimeFaultDetail::new(
        "gilrs.test-failure",
        "Injected gamepad test failure",
        "nara_gilrs",
    )
    .unwrap()
    .into_bevy_error())
}

#[test]
fn original_close_participant_retires_on_configured_startup_runtime_failure_and_ordinary_close() {
    let owner_thread = thread::current().id();
    for (configured_only, stage) in [
        (true, None),
        (false, Some(FailurePoint::Startup)),
        (false, Some(FailurePoint::Runtime)),
        (false, None),
    ] {
        let drops = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        app.add_plugin(CloseProbePlugin {
            drops: Arc::clone(&drops),
            fail_stage: stage,
        })
        .unwrap();
        if configured_only {
            let original = app
                .world_mut()
                .unwrap()
                .remove_non_send::<NativeGilrs>()
                .unwrap();
            let mut retirement = app.begin_retirement(RuntimeClosePolicy::new(Duration::ZERO));
            assert_eq!(
                retirement.drive_retirement(),
                RuntimeCandidateRetirementState::Retired
            );
            let world = retirement.world().unwrap();
            assert!(world.resource::<GilrsDevices>().closed);
            assert!(matches!(
                world
                    .resource::<InputTimeline>()
                    .entries()
                    .last()
                    .unwrap()
                    .kind(),
                InputEventKind::GamepadDisconnected { .. }
            ));
            assert!(original.0.borrow().drop_probe.is_none());
        } else if stage == Some(FailurePoint::Startup) {
            assert!(RuntimeInstance::start(app).is_err());
        } else {
            let mut runtime = RuntimeInstance::start(app).unwrap();
            if stage.is_some() {
                assert!(runtime.drive(Duration::ZERO).is_err());
                assert_eq!(runtime.state(), RuntimeState::Faulted);
            }
            runtime.finish().unwrap();
        }
        assert_eq!(*drops.lock().unwrap(), [owner_thread]);
    }
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "Initializes native Windows WGI and drops the original Gilrs context; run explicitly on Windows"]
fn native_windows_initialization_and_close_require_no_controller() {
    let mut app = App::new();
    app.add_plugin(InputPlugin).unwrap();
    app.add_plugin(GilrsInputPlugin::new(None)).unwrap();
    let original = Rc::clone(&app.world().non_send::<NativeGilrs>().0);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let devices = runtime.world().resource::<GilrsDevices>().connected().len();
    assert!(
        runtime
            .world()
            .resource::<GilrsDevices>()
            .connected()
            .iter()
            .all(|d| d.route().is_none())
    );
    runtime.finish().unwrap();
    assert!(original.borrow().gilrs.is_none());
    println!("GILRS_NATIVE_OK devices={devices} native_context_dropped=true force_feedback=false");
}
