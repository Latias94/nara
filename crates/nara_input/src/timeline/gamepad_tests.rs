use nara_app::__RuntimeDriverPort;
use nara_core::Vec2;

use super::*;
use crate::{
    ActionMap, ActionMaps, AxisValue, ButtonDriverInput, ButtonTransitionPhase, InputBinding,
    KeyCode, MouseButton, semantic::ActionStaging,
};

const WINDOW: InputWindowId = InputWindowId::new(1);
const SEAT: InputSeatId = InputSeatId::PRIMARY;

fn value(raw: f32) -> AxisValue {
    AxisValue::new(raw).unwrap()
}

fn timeline(gamepads: usize, members: usize) -> InputTimeline {
    let limits = InputLimits::new(48, 4, 4, members)
        .unwrap()
        .with_gamepads(gamepads)
        .unwrap();
    let mut timeline = InputTimeline::with_limits(limits).unwrap();
    timeline.connect_endpoint(WINDOW, SEAT).unwrap();
    timeline
}

fn drain(timeline: &mut InputTimeline) {
    timeline.finish_projection();
    timeline.publish_observations(&mut InputFrameObservations::default());
    timeline.finalize_ui_routing();
    timeline.stage_actions(&ActionStaging::default());
    assert!(timeline.take_deferred_action_inputs().is_empty());
    timeline.finish_frame_observations();
    timeline.reclaim_completed();
    assert_eq!(timeline.retained_len(), 0);
}

fn staging() -> ActionStaging {
    let mut staging = ActionStaging::default();
    staging.observe_maps(&ActionMaps::default());
    staging
}

fn snapshot(timeline: &InputTimeline) -> (Vec<InputTimelineEntry>, u64, usize, usize, u64) {
    (
        timeline.entries().copied().collect(),
        timeline.capacity.next_sequence(),
        timeline.capacity.obligations(),
        timeline.topology.members.iter().flatten().count(),
        timeline.topology.next_identity().unwrap(),
    )
}

#[test]
fn gamepad_capacity_is_finite_can_be_disabled_and_refuses_without_mutation() {
    assert_eq!(InputLimits::default().gamepads(), 64);
    assert_eq!(InputLimits::new(48, 4, 4, 4).unwrap().gamepads(), 64);
    assert_eq!(
        InputLimits::default().with_gamepads(usize::MAX),
        Err(InputConfigurationError::CapacityOverflow),
    );
    let mut disabled = timeline(0, 4);
    let before = snapshot(&disabled);
    assert_eq!(
        disabled.connect_gamepad(WINDOW, SEAT, 0),
        Err(InputIngressError::GamepadCapacityExceeded { limit: 0 }),
    );
    assert_eq!(snapshot(&disabled), before);
    assert!(disabled.topology.gamepads.is_empty());
    disabled
        .admit_keyboard(WINDOW, SEAT, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();

    let mut limited = timeline(1, 4);
    limited.connect_gamepad(WINDOW, SEAT, 0).unwrap();
    let before = snapshot(&limited);
    assert_eq!(
        limited.connect_gamepad(WINDOW, SEAT, 1),
        Err(InputIngressError::GamepadCapacityExceeded { limit: 1 }),
    );
    assert_eq!(snapshot(&limited), before);
    assert_eq!(limited.topology.gamepads.len(), 1);
}

#[test]
fn connection_checks_its_original_owner_endpoint_and_nonreused_generation() {
    let mut owner = timeline(2, 4);
    let unknown = InputWindowId::new(99);
    let before = snapshot(&owner);
    assert_eq!(
        owner.connect_gamepad(unknown, SEAT, 4),
        Err(InputIngressError::UnknownEndpoint {
            window: unknown,
            seat: SEAT
        }),
    );
    assert_eq!(snapshot(&owner), before);
    let connection = owner.connect_gamepad(WINDOW, SEAT, 4).unwrap();
    let retained_token = connection.clone();
    assert_eq!(connection, retained_token);
    assert_eq!(
        (
            connection.window(),
            connection.seat(),
            connection.raw_label()
        ),
        (WINDOW, SEAT, 4)
    );
    let mut foreign = timeline(2, 4);
    let foreign_connection = foreign.connect_gamepad(WINDOW, SEAT, 4).unwrap();
    assert_eq!(
        connection.connection_generation(),
        foreign_connection.connection_generation()
    );
    assert_ne!(connection, foreign_connection);
    let before = snapshot(&foreign);
    assert_eq!(
        foreign.admit_gamepad_button(&connection, 1, ButtonTransitionPhase::Pressed),
        Err(InputIngressError::ForeignGamepadConnection)
    );
    assert_eq!(
        foreign.admit_gamepad_axis(&connection, 1, value(0.5)),
        Err(InputIngressError::ForeignGamepadConnection)
    );
    assert_eq!(
        foreign.disconnect_gamepad(&connection),
        Err(InputIngressError::ForeignGamepadConnection)
    );
    assert_eq!(snapshot(&foreign), before);

    owner
        .admit_gamepad_button(&connection, 1, ButtonTransitionPhase::Pressed)
        .unwrap();
    owner.disconnect_gamepad(&connection).unwrap();
    let replacement = owner.connect_gamepad(WINDOW, SEAT, 4).unwrap();
    assert!(replacement.connection_generation() > connection.connection_generation());
    let before = snapshot(&owner);
    let expired = InputIngressError::GamepadConnectionExpired {
        connection_generation: connection.connection_generation(),
    };
    assert_eq!(
        owner.admit_gamepad_button(&retained_token, 1, ButtonTransitionPhase::Released),
        Err(expired)
    );
    assert_eq!(
        owner.admit_gamepad_axis(&retained_token, 1, value(0.5)),
        Err(expired)
    );
    assert_eq!(owner.disconnect_gamepad(&retained_token), Err(expired));
    assert_eq!(snapshot(&owner), before);
    assert!(
        owner
            .admit_gamepad_button(&replacement, 1, ButtonTransitionPhase::Released)
            .unwrap()
            .is_ignored()
    );
    drain(&mut owner);
    owner
        .admit_window_event(WINDOW, SEAT, InputWindowEvent::SurfaceRetired)
        .unwrap();
    let before = snapshot(&owner);
    assert!(matches!(
        owner.admit_gamepad_axis(&replacement, 1, value(0.5)),
        Err(InputIngressError::EndpointRetiring { .. })
    ));
    assert_eq!(snapshot(&owner), before);
    drain(&mut owner);
    owner.connect_endpoint(WINDOW, SEAT).unwrap();
    let next = owner.connect_gamepad(WINDOW, SEAT, 4).unwrap();
    let before = snapshot(&owner);
    assert_eq!(
        owner.admit_gamepad_button(&replacement, 1, ButtonTransitionPhase::Pressed),
        Err(InputIngressError::GamepadConnectionExpired {
            connection_generation: replacement.connection_generation()
        })
    );
    assert_eq!(snapshot(&owner), before);
    assert!(next.connection_generation() > replacement.connection_generation());
}

#[test]
fn identical_labels_and_buttons_on_one_seat_have_independent_members() {
    let mut timeline = timeline(2, 4);
    let first = timeline.connect_gamepad(WINDOW, SEAT, 7).unwrap();
    let second = timeline.connect_gamepad(WINDOW, SEAT, 7).unwrap();
    let first_press = timeline
        .admit_gamepad_button(&first, 3, ButtonTransitionPhase::Pressed)
        .unwrap();
    let second_press = timeline
        .admit_gamepad_button(&second, 3, ButtonTransitionPhase::Pressed)
        .unwrap();
    assert_ne!(first_press.first_sequence(), second_press.first_sequence());
    assert_eq!(
        timeline
            .topology
            .members
            .iter()
            .flatten()
            .filter(|member| member.pressed)
            .count(),
        2
    );
    timeline.disconnect_gamepad(&first).unwrap();
    let survivors = timeline
        .topology
        .members
        .iter()
        .flatten()
        .filter(|member| member.pressed)
        .collect::<Vec<_>>();
    assert_eq!(survivors.len(), 1);
    assert_eq!(
        survivors[0].gamepad.unwrap().0,
        second.connection_generation()
    );
    timeline
        .admit_gamepad_button(&second, 3, ButtonTransitionPhase::Released)
        .unwrap();
    assert_eq!(
        timeline.entries().last().unwrap().press_sequence(),
        second_press.first_sequence()
    );
}

#[test]
fn axes_keep_original_membership_through_changes_and_end_exactly_at_neutral() {
    let mut timeline = timeline(1, 4);
    let connection = timeline.connect_gamepad(WINDOW, SEAT, 1).unwrap();
    assert!(
        timeline
            .admit_gamepad_axis(&connection, 2, AxisValue::ZERO)
            .unwrap()
            .is_ignored()
    );
    assert_eq!(timeline.topology.members.iter().flatten().count(), 0);
    let first = timeline
        .admit_gamepad_axis(&connection, 2, value(-0.5))
        .unwrap();
    let before = snapshot(&timeline);
    assert!(
        timeline
            .admit_gamepad_axis(&connection, 2, value(-0.5))
            .unwrap()
            .is_ignored()
    );
    assert_eq!(snapshot(&timeline), before);
    for sample in [-1.0, 0.25, 1.0, f32::from_bits(1)] {
        timeline
            .admit_gamepad_axis(&connection, 2, value(sample))
            .unwrap();
        let entry = *timeline.entries().last().unwrap();
        assert_eq!(entry.press_sequence(), first.first_sequence());
        assert_eq!(entry.action_input(), Some(InputBinding::GamepadAxis(2)));
        assert_eq!(entry.axis_value(), Some(value(sample)));
        assert_eq!(entry.action_transition(), None);
        let active = timeline
            .topology
            .members
            .iter()
            .flatten()
            .filter(|member| member.pressed)
            .collect::<Vec<_>>();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].axis_value, entry.axis_value());
    }
    timeline
        .admit_gamepad_button(&connection, 2, ButtonTransitionPhase::Pressed)
        .unwrap();
    let button = *timeline.entries().last().unwrap();
    assert_eq!(
        button.action_transition(),
        Some((
            InputBinding::GamepadButton(2),
            ButtonTransitionPhase::Pressed
        ))
    );
    assert_eq!(button.axis_value(), None);
    timeline
        .admit_gamepad_axis(&connection, 2, AxisValue::ZERO)
        .unwrap();
    let neutral = *timeline.entries().last().unwrap();
    assert_eq!(neutral.press_sequence(), first.first_sequence());
    assert_eq!(neutral.axis_value(), Some(AxisValue::ZERO));
    assert_eq!(
        neutral.gamepad_connection_generation(),
        Some(connection.connection_generation())
    );
    assert_eq!(
        neutral.action_provenance().gamepad_connection_generation(),
        Some(connection.connection_generation())
    );
    assert_eq!(
        timeline
            .topology
            .members
            .iter()
            .flatten()
            .filter(|member| member.pressed)
            .count(),
        1
    );
    let next = timeline
        .admit_gamepad_axis(&connection, 2, value(0.5))
        .unwrap();
    assert!(next.first_sequence() > first.first_sequence());
    assert_eq!(
        timeline.entries().last().unwrap().press_sequence(),
        next.first_sequence()
    );
}

#[test]
fn saturated_log_still_disconnects_in_press_order_and_preserves_other_sources() {
    let mut timeline = timeline(2, 8);
    timeline
        .connect_pointer(WINDOW, SEAT, InputPointerId::PRIMARY)
        .unwrap();
    let first = timeline.connect_gamepad(WINDOW, SEAT, 0).unwrap();
    let second = timeline.connect_gamepad(WINDOW, SEAT, 1).unwrap();
    let axis = timeline.admit_gamepad_axis(&first, 1, value(0.75)).unwrap();
    timeline
        .admit_gamepad_button(&second, 2, ButtonTransitionPhase::Pressed)
        .unwrap();
    let button = timeline
        .admit_gamepad_button(&first, 2, ButtonTransitionPhase::Pressed)
        .unwrap();
    timeline
        .admit_keyboard(WINDOW, SEAT, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    timeline
        .admit_mouse_button(
            WINDOW,
            SEAT,
            InputPointerId::PRIMARY,
            ButtonDriverInput::Press(MouseButton::Left),
        )
        .unwrap();
    let mut refused = false;
    for index in 0..timeline.limits().events() {
        let before = snapshot(&timeline);
        let current = timeline
            .topology
            .members
            .iter()
            .flatten()
            .find(|member| member.pressed && member.input == InputBinding::GamepadAxis(1))
            .unwrap()
            .axis_value;
        let sample = if index % 2 == 0 {
            value(0.25)
        } else {
            value(0.5)
        };
        if let Err(error) = timeline.admit_gamepad_axis(&first, 1, sample) {
            assert!(matches!(error, InputIngressError::CapacityExceeded { .. }));
            assert_eq!(snapshot(&timeline), before);
            assert_eq!(
                timeline
                    .topology
                    .members
                    .iter()
                    .flatten()
                    .find(|member| member.pressed && member.input == InputBinding::GamepadAxis(1))
                    .unwrap()
                    .axis_value,
                current
            );
            refused = true;
            break;
        }
    }
    assert!(refused);
    assert_eq!(
        timeline.capacity.retained() + timeline.capacity.obligations(),
        timeline.limits().events()
    );
    let before = timeline.retained_len();
    assert_eq!(timeline.disconnect_gamepad(&first).unwrap().admitted(), 3);
    let terminal = timeline.entries().skip(before).copied().collect::<Vec<_>>();
    assert_eq!(terminal[0].press_sequence(), axis.first_sequence());
    assert_eq!(terminal[0].axis_value(), Some(AxisValue::ZERO));
    assert_eq!(terminal[1].press_sequence(), button.first_sequence());
    assert_eq!(
        terminal[1].action_transition(),
        Some((
            InputBinding::GamepadButton(2),
            ButtonTransitionPhase::Released
        ))
    );
    assert!(matches!(
        terminal[2].kind(),
        InputEventKind::GamepadDisconnected { .. }
    ));
    assert!(terminal.iter().all(|entry| entry.invalidation() == Some(InputInvalidationReason::GamepadDisconnected)));
    assert_eq!(
        timeline
            .topology
            .members
            .iter()
            .flatten()
            .filter(|member| member.pressed)
            .count(),
        3
    );
    let before = timeline.retained_len();
    timeline
        .admit_window_event(WINDOW, SEAT, InputWindowEvent::SurfaceRetired)
        .unwrap();
    assert_eq!(
        timeline
            .entries()
            .skip(before)
            .filter(|entry| matches!(entry.kind(), InputEventKind::GamepadDisconnected { .. }))
            .count(),
        1
    );
    assert_eq!(
        timeline
            .topology
            .members
            .iter()
            .flatten()
            .filter(|member| member.pressed)
            .count(),
        0
    );
    assert!(timeline.retained_len() <= timeline.limits().events());
}

#[test]
fn refused_member_never_acquires_a_terminal_and_leaves_accepted_axis_unchanged() {
    let mut timeline = timeline(1, 1);
    let connection = timeline.connect_gamepad(WINDOW, SEAT, 0).unwrap();
    timeline
        .admit_gamepad_axis(&connection, 0, value(-0.75))
        .unwrap();
    let before = snapshot(&timeline);
    assert_eq!(
        timeline.admit_gamepad_button(&connection, 1, ButtonTransitionPhase::Pressed),
        Err(InputIngressError::MemberCapacityExceeded { limit: 1 })
    );
    assert_eq!(
        timeline.admit_gamepad_axis(&connection, 1, value(0.5)),
        Err(InputIngressError::MemberCapacityExceeded { limit: 1 })
    );
    assert_eq!(snapshot(&timeline), before);
    assert_eq!(
        timeline
            .topology
            .members
            .iter()
            .flatten()
            .next()
            .unwrap()
            .axis_value,
        Some(value(-0.75))
    );
    assert_eq!(
        timeline.disconnect_gamepad(&connection).unwrap().admitted(),
        2
    );
    assert!(
        !timeline
            .entries()
            .any(|entry| entry.action_input() == Some(InputBinding::GamepadButton(1)))
    );
}

#[test]
fn focus_and_endpoint_retirement_neutralize_only_their_window_and_seat() {
    let mut timeline = timeline(3, 8);
    let second_seat = InputSeatId::new(2);
    let second_window = InputWindowId::new(2);
    timeline.connect_endpoint(WINDOW, second_seat).unwrap();
    timeline.connect_endpoint(second_window, SEAT).unwrap();
    let first = timeline.connect_gamepad(WINDOW, SEAT, 0).unwrap();
    let second = timeline.connect_gamepad(WINDOW, second_seat, 0).unwrap();
    let third = timeline.connect_gamepad(second_window, SEAT, 0).unwrap();
    for connection in [&first, &second, &third] {
        timeline
            .admit_gamepad_axis(connection, 0, value(-0.5))
            .unwrap();
        timeline
            .admit_gamepad_button(connection, 0, ButtonTransitionPhase::Pressed)
            .unwrap();
    }
    drain(&mut timeline);
    let loss = timeline
        .admit_window_event(WINDOW, SEAT, InputWindowEvent::Focused(false))
        .unwrap();
    assert_eq!(loss.admitted(), 3);
    let entries = timeline.entries().copied().collect::<Vec<_>>();
    assert_eq!(entries[0].axis_value(), Some(AxisValue::ZERO));
    assert_eq!(
        entries[1].action_transition(),
        Some((
            InputBinding::GamepadButton(0),
            ButtonTransitionPhase::Released
        ))
    );
    assert_eq!(
        timeline
            .topology
            .members
            .iter()
            .flatten()
            .filter(|member| member.pressed)
            .count(),
        4
    );
    assert_eq!(
        timeline
            .topology
            .gamepads
            .iter()
            .flatten()
            .filter(|gamepad| !gamepad.retiring)
            .count(),
        3
    );
    let before = timeline.retained_len();
    assert_eq!(
        timeline
            .retire_presented_endpoint(WINDOW, second_seat, None)
            .unwrap()
            .admission()
            .admitted(),
        4
    );
    let terminal = timeline.entries().skip(before).copied().collect::<Vec<_>>();
    assert!(
        terminal
            .iter()
            .all(|entry| entry.route().window() == WINDOW && entry.route().seat() == second_seat)
    );
    assert_eq!(terminal[0].axis_value(), Some(AxisValue::ZERO));
    assert!(matches!(
        terminal[2].kind(),
        InputEventKind::GamepadDisconnected { .. }
    ));
    assert!(matches!(terminal[3].kind(), InputEventKind::SurfaceRetired));
    assert_eq!(
        timeline
            .topology
            .members
            .iter()
            .flatten()
            .filter(|member| member.pressed)
            .count(),
        2
    );
    assert!(
        timeline
            .admit_gamepad_axis(&third, 0, AxisValue::ZERO)
            .is_ok()
    );
    assert!(
        timeline
            .admit_gamepad_button(&first, 0, ButtonTransitionPhase::Pressed)
            .is_ok()
    );
}

#[test]
fn retired_connection_slots_wait_for_actions_and_actual_frame_copy_retirement() {
    let mut timeline = timeline(1, 1);
    let connection = timeline.connect_gamepad(WINDOW, SEAT, 0).unwrap();
    let copied_token = connection.clone();
    let pressed = timeline
        .admit_gamepad_axis(&connection, 0, value(0.5))
        .unwrap();
    timeline.disconnect_gamepad(&connection).unwrap();
    let staging = staging();
    timeline.finish_projection();
    let mut observations = InputFrameObservations::default();
    timeline.publish_observations(&mut observations);
    timeline.finalize_ui_routing();
    timeline.stage_actions(&staging);
    observations.clear();
    timeline.finish_frame_observations();
    timeline.reclaim_completed();
    assert_eq!(
        timeline.cursors().oldest_deferred_action(),
        pressed.first_sequence()
    );
    assert_eq!(
        timeline.connect_gamepad(WINDOW, SEAT, 0),
        Err(InputIngressError::GamepadCapacityExceeded { limit: 1 })
    );
    let delivered = timeline.take_deferred_action_inputs();
    assert_eq!(delivered.len(), 2);
    assert_eq!(
        delivered[0].0.press_sequence(),
        delivered[1].0.press_sequence()
    );
    assert_eq!(delivered[1].0.axis_value(), Some(AxisValue::ZERO));
    timeline.reclaim_completed();
    assert_eq!(timeline.topology.gamepads.iter().flatten().count(), 1);
    assert_eq!(timeline.topology.members.iter().flatten().count(), 1);
    drop(delivered);
    timeline.finish_frame_observations();
    timeline.reclaim_completed();
    assert_eq!(timeline.retained_len(), 0);
    assert_eq!(timeline.topology.gamepads.iter().flatten().count(), 0);
    assert_eq!(timeline.topology.members.iter().flatten().count(), 0);
    let replacement = timeline.connect_gamepad(WINDOW, SEAT, 0).unwrap();
    assert_ne!(copied_token, replacement);
    assert!(replacement.connection_generation() > copied_token.connection_generation());
}

fn route(epoch: u64) -> InputPresentedRoute {
    InputPresentedRoute::new(
        WINDOW,
        InputSurfaceId::new(1),
        SEAT,
        InputPresentationEpoch::new(epoch).unwrap(),
        InputSlotGeneration::new(1).unwrap(),
        InputRouteGeneration::new(epoch).unwrap(),
        InputGeometryGeneration::new(epoch).unwrap(),
    )
}

#[test]
fn changed_axes_and_neutral_wait_for_original_ui_disposition_after_route_replacement() {
    for consumed in [false, true] {
        let mut timeline = timeline(1, 2);
        let connection = timeline.connect_gamepad(WINDOW, SEAT, 0).unwrap();
        timeline.publish_presented_route(route(1)).unwrap();
        let pressed = timeline
            .admit_gamepad_axis(&connection, 0, value(0.25))
            .unwrap();
        timeline.publish_presented_route(route(2)).unwrap();
        timeline
            .admit_gamepad_axis(&connection, 0, value(0.75))
            .unwrap();
        timeline.disconnect_gamepad(&connection).unwrap();
        let staging = staging();
        timeline.stage_actions(&staging);
        assert!(timeline.take_deferred_action_inputs().is_empty());
        assert_eq!(
            timeline.cursors().oldest_deferred_action(),
            pressed.first_sequence()
        );
        if consumed {
            timeline
                .__finalize_ui_entry(
                    pressed.first_sequence().unwrap(),
                    InputUiDisposition::Consumed,
                )
                .unwrap();
        }
        timeline.finalize_ui_routing();
        let delivered = timeline.take_deferred_action_inputs();
        assert_eq!(delivered.len(), if consumed { 0 } else { 3 });
        for (entry, _) in &delivered {
            assert_eq!(entry.press_sequence(), pressed.first_sequence());
            assert_eq!(
                entry
                    .action_provenance()
                    .press_route()
                    .unwrap()
                    .route_generation(),
                Some(route(1).route_generation())
            );
        }
        if !consumed {
            assert!(delivered[0].0.route().is_ui_routable());
            assert!(
                delivered
                    .iter()
                    .skip(1)
                    .all(|(entry, _)| !entry.route().is_ui_routable())
            );
            assert_eq!(delivered[2].0.axis_value(), Some(AxisValue::ZERO));
        }
    }
}

#[test]
fn action_staging_selects_the_entry_seat_and_finishes_missing_maps() {
    let mut timeline = timeline(2, 4);
    let second_seat = InputSeatId::new(2);
    timeline.connect_endpoint(WINDOW, second_seat).unwrap();
    let first = timeline.connect_gamepad(WINDOW, SEAT, 0).unwrap();
    let second = timeline.connect_gamepad(WINDOW, second_seat, 0).unwrap();
    timeline
        .admit_gamepad_button(&first, 0, ButtonTransitionPhase::Pressed)
        .unwrap();
    timeline.admit_gamepad_axis(&second, 0, value(0.5)).unwrap();
    let mut maps = ActionMaps::default();
    maps.insert(second_seat, ActionMap::default()).unwrap();
    let mut staging = ActionStaging::default();
    staging.observe_maps(&maps);
    timeline.stage_actions(&staging);
    let entries = timeline.take_deferred_action_inputs();
    assert_eq!(entries.len(), 2);
    for (entry, revision) in entries {
        assert!(std::sync::Arc::ptr_eq(
            &revision,
            staging.revision_for_seat(entry.route().seat()).unwrap()
        ));
    }
    maps.remove(second_seat);
    staging.observe_maps(&maps);
    timeline
        .admit_gamepad_axis(&second, 0, AxisValue::ZERO)
        .unwrap();
    timeline.stage_actions(&staging);
    assert!(timeline.take_deferred_action_inputs().is_empty());
    assert_eq!(timeline.cursors().oldest_deferred_action(), None);
    drain(&mut timeline);
}

#[test]
fn terminal_sequence_headroom_includes_connections_and_axis_neutralization() {
    let mut timeline = InputTimeline::default();
    let window = InputWindowId::UNSPECIFIED;
    let connection = timeline.connect_gamepad(window, SEAT, 0).unwrap();
    timeline
        .admit_gamepad_button(&connection, 1, ButtonTransitionPhase::Pressed)
        .unwrap();
    timeline
        .admit_gamepad_axis(&connection, 0, value(0.5))
        .unwrap();
    drain(&mut timeline);
    let obligations = timeline.capacity.obligations();
    assert_eq!(obligations, 6);
    timeline
        .capacity
        .set_last_sequence(u64::MAX - obligations as u64)
        .unwrap();
    let before = snapshot(&timeline);
    assert!(matches!(
        timeline.admit_gamepad_axis(&connection, 0, value(1.0)),
        Err(InputIngressError::SequenceExhausted { .. })
    ));
    assert_eq!(snapshot(&timeline), before);
    assert_eq!(
        timeline.disconnect_gamepad(&connection).unwrap().admitted(),
        3
    );
    for event in [
        InputWindowEvent::Focused(false),
        InputWindowEvent::CloseRequested,
        InputWindowEvent::SurfaceRetired,
    ] {
        assert_eq!(
            timeline
                .admit_window_event(window, SEAT, event)
                .unwrap()
                .admitted(),
            1
        );
    }
    assert_eq!(timeline.capacity.next_sequence(), u64::MAX);
    assert_eq!(timeline.capacity.obligations(), 0);
}

#[test]
fn driver_connections_capture_origin_and_preserve_keyboard_and_mouse_outputs() {
    let mut timeline = InputTimeline::default();
    let output = timeline
        .apply_driver_input(__InputDriverEvent::ConnectSyntheticGamepad {
            window: InputWindowId::UNSPECIFIED,
            seat: SEAT,
            gamepad: 5,
        })
        .unwrap();
    assert_eq!(output.admission.admitted(), 1);
    let connection = output.gamepad_connection.unwrap();
    for event in [
        __InputDriverEvent::GamepadButton {
            connection: connection.clone(),
            button: 0,
            phase: ButtonTransitionPhase::Pressed,
        },
        __InputDriverEvent::GamepadAxis {
            connection: connection.clone(),
            axis: 0,
            value: value(0.5),
        },
        __InputDriverEvent::DisconnectGamepad {
            connection: connection.clone(),
        },
    ] {
        let output = timeline.apply_driver_input(event).unwrap();
        assert!(output.gamepad_connection.is_none());
        assert!(output.released_keys.is_empty() && output.released_mouse_buttons.is_empty());
    }
    assert!(
        timeline
            .entries()
            .all(|entry| entry.origin() == InputEventOrigin::Synthetic)
    );
    assert!(
        timeline
            .entries()
            .all(|entry| entry.gamepad_connection_generation()
                == Some(connection.connection_generation()))
    );
    timeline
        .apply_driver_input(__InputDriverEvent::SyntheticKeyboard {
            input: ButtonDriverInput::Press(KeyCode::Enter),
        })
        .unwrap();
    timeline
        .apply_driver_input(__InputDriverEvent::SyntheticMouse {
            input: ButtonDriverInput::Press(MouseButton::Left),
        })
        .unwrap();
    timeline
        .apply_driver_input(__InputDriverEvent::SyntheticPointer {
            input: crate::PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
        })
        .unwrap();
    let output = timeline
        .apply_driver_input(__InputDriverEvent::Window {
            window: InputWindowId::UNSPECIFIED,
            seat: SEAT,
            event: InputWindowEvent::SurfaceRetired,
        })
        .unwrap();
    assert_eq!(output.released_keys, [KeyCode::Enter]);
    assert_eq!(output.released_mouse_buttons, [MouseButton::Left]);
    assert!(output.gamepad_connection.is_none());
}
