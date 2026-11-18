use super::*;

#[derive(Clone, Copy)]
enum Handoff {
    Focus,
    Reassign,
    Reconnect,
}

#[derive(Default, nara_ecs::Resource)]
struct Frames(Vec<Vec<nara_input::InputTimelineEntry>>, Option<String>);

fn qualify(handoff: Handoff) {
    let other = GamepadRoute::new(InputWindowId::new(8), InputSeatId::new(1));
    let mut timeline =
        InputTimeline::with_limits(InputLimits::default().with_gamepads(1).unwrap()).unwrap();
    for route in [ROUTE, other] {
        timeline.connect_endpoint(route.window, route.seat).unwrap();
        timeline
            .admit_window_event(route.window, route.seat, InputWindowEvent::Focused(true))
            .unwrap();
    }
    let (mut bridge, mut devices) = Bridge::new(1, Some(ROUTE));
    let mut app = App::new();
    app.insert_resource(timeline).unwrap();
    app.insert_resource(Frames::default()).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    let mut frame = 0;
    app.add_systems(
        CoreStage::First,
        (move |mut timeline: ResMut<InputTimeline>,
               mut frames: ResMut<Frames>|
              -> Result<(), BevyError> {
            let before = timeline
                .entries()
                .map(|entry| entry.sequence())
                .max()
                .unwrap_or(0);
            match frame {
                0 => {
                    for route in [ROUTE, other] {
                        timeline
                            .admit_window_event(
                                route.window,
                                route.seat,
                                InputWindowEvent::Focused(true),
                            )
                            .unwrap();
                    }
                    bridge
                        .connected(0, "Controller", PhysicalState::default(), &mut devices)
                        .unwrap();
                }
                1 => match handoff {
                    Handoff::Focus => {
                        for focused in [false, true] {
                            timeline
                                .admit_window_event(
                                    WINDOW,
                                    ROUTE.seat,
                                    InputWindowEvent::Focused(focused),
                                )
                                .unwrap();
                        }
                    }
                    Handoff::Reassign => {
                        let token = devices.connected()[0].token();
                        devices.assign(&token, other).unwrap();
                    }
                    Handoff::Reconnect => {
                        let old = devices.connected()[0].token();
                        bridge.disconnected(0, &mut devices, &mut timeline).unwrap();
                        let mut physical = PhysicalState::default();
                        physical.buttons[GamepadButton::South.code() as usize] = true;
                        physical.axes[GamepadAxis::LeftStickX.code() as usize] =
                            nara_input::AxisValue::new(0.7).unwrap();
                        bridge
                            .connected(0, "Controller", physical, &mut devices)
                            .unwrap();
                        assert_ne!(devices.connected()[0].token(), old);
                    }
                },
                _ => {}
            }
            bridge
                .reconcile(&devices, &mut timeline, true)
                .map_err(|error| {
                    frames.1 = Some(error.to_string());
                    RuntimeFaultDetail::new(
                        "gilrs.handoff",
                        "Controller handoff failed",
                        "nara_gilrs",
                    )
                    .unwrap()
                    .into_bevy_error()
                })?;
            match frame {
                0 | 2 => {
                    bridge
                        .event(0, press(Button::South), &mut timeline)
                        .unwrap();
                    bridge.event(0, axis(0.7), &mut timeline).unwrap();
                }
                3 => {
                    bridge
                        .event(0, release(Button::South), &mut timeline)
                        .unwrap();
                    bridge.event(0, axis(0.0), &mut timeline).unwrap();
                    bridge
                        .event(0, press(Button::South), &mut timeline)
                        .unwrap();
                    bridge.event(0, axis(0.6), &mut timeline).unwrap();
                }
                4 => bridge.close(&mut timeline).unwrap(),
                _ => {}
            }
            frames.0.push(
                timeline
                    .entries()
                    .copied()
                    .filter(|entry| entry.sequence() > before)
                    .collect(),
            );
            frame += 1;
            Ok(())
        })
        .before(InputSet::ProjectTimeline),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    for _ in 0..5 {
        if let Err(error) = runtime.drive(Duration::ZERO) {
            let bridge_error = runtime.world().resource::<Frames>().1.clone();
            runtime.finish().unwrap();
            panic!("ordinary controller handoff faulted: {error}; {bridge_error:?}");
        }
    }
    let frames = runtime.world().resource::<Frames>().0.clone();
    runtime.finish().unwrap();
    let generation = |frame: usize| {
        frames[frame].iter().find_map(|entry| match entry.kind() {
            InputEventKind::GamepadConnected {
                connection_generation,
                ..
            } => Some(connection_generation),
            _ => None,
        })
    };
    let original =
        generation(0).unwrap_or_else(|| panic!("missing initial connection: {frames:?}"));
    assert_eq!(generation(1), None);
    assert!(frames[1].iter().any(|entry| matches!(entry.kind(),
        InputEventKind::GamepadDisconnected { connection_generation, .. } if connection_generation == original
    )));
    assert!(frames[1].iter().any(|entry| matches!(
        entry.kind(),
        InputEventKind::GamepadButton {
            phase: ButtonTransitionPhase::Released,
            ..
        }
    )));
    assert!(generation(2).unwrap() > original);
    assert!(!frames[2].iter().any(|entry| matches!(
        entry.kind(),
        InputEventKind::GamepadButton { .. } | InputEventKind::GamepadAxis { .. }
    )));
    let presses: Vec<_> = frames[3]
        .iter()
        .filter(|entry| {
            matches!(
                entry.kind(),
                InputEventKind::GamepadButton {
                    phase: ButtonTransitionPhase::Pressed,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(presses.len(), 1);
    assert_eq!(
        presses[0].route().window(),
        if matches!(handoff, Handoff::Reassign) {
            other.window
        } else {
            WINDOW
        }
    );
    assert!(frames[3].iter().any(|entry| matches!(entry.kind(),
        InputEventKind::GamepadAxis { value, .. } if value.get() == 0.6
    )));
    assert!(
        frames[4]
            .iter()
            .any(|entry| matches!(entry.kind(), InputEventKind::GamepadDisconnected { .. }))
    );
}

#[test]
fn one_slot_focus_handoff_waits_for_original_input_consumers() {
    qualify(Handoff::Focus);
}

#[test]
fn one_slot_route_handoff_waits_for_original_input_consumers() {
    qualify(Handoff::Reassign);
}

#[test]
fn one_slot_native_reconnect_waits_for_original_input_consumers() {
    qualify(Handoff::Reconnect);
}
