use std::time::{Duration, Instant};

use crate::WindowPlugin;
use nara_app::{App, RuntimeInstance, RuntimeState};
use nara_ecs::bevy_ecs::message::{Message, Messages};
use nara_input::InputPlugin;
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, Force, Touch},
};

use super::*;
use crate::winit::{
    DEFAULT_NATIVE_RETIREMENT_BUDGET, NoPresentationDriver, WinitControlFlow, WinitShutdownState,
};

fn messages<T: Message + Clone>(app: &WinitApp) -> Vec<T> {
    app.runtime
        .world()
        .resource::<Messages<T>>()
        .get_cursor()
        .read(app.runtime.world().resource::<Messages<T>>())
        .cloned()
        .collect()
}

#[test]
fn mapped_callbacks_request_frames_and_deliver_typed_input_through_the_runtime() {
    let mut config = App::new();
    config.add_plugin(WindowPlugin::default()).unwrap();
    config.add_plugin(InputPlugin).unwrap();
    let mut app = WinitApp::new(
        RuntimeInstance::start(config).unwrap(),
        Box::new(NoPresentationDriver),
        WinitControlFlow::Wait,
        DEFAULT_NATIVE_RETIREMENT_BUDGET,
    );
    let window = WindowId::PRIMARY;
    let input_window = InputWindowId::new(window.get());
    let device_id = DeviceId::dummy();
    app.connect_window_input(window).unwrap();
    app.handle_mapped_window_event(window, WinitWindowEvent::Focused(true))
        .unwrap();
    app.handle_mapped_window_event(
        window,
        WinitWindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(22.0, 39.0),
        },
    )
    .unwrap();
    for (state, text, synthetic) in [
        (ElementState::Pressed, "é", false),
        (ElementState::Pressed, "é", false),
        (ElementState::Released, "released", false),
        (ElementState::Pressed, "synthetic", true),
    ] {
        app.apply_text_input(window, state, Some(text), synthetic)
            .unwrap();
    }
    let contact = Touch {
        device_id,
        phase: WinitTouchPhase::Started,
        location: PhysicalPosition::new(3.0, 6.0),
        force: Some(Force::Normalized(0.5)),
        id: 27,
    };
    for event in [
        WinitWindowEvent::Ime(Ime::Enabled),
        WinitWindowEvent::Ime(Ime::Preedit("文".into(), Some((3, 3)))),
        WinitWindowEvent::Ime(Ime::Commit("文".into())),
        WinitWindowEvent::MouseWheel {
            device_id,
            delta: MouseScrollDelta::LineDelta(1.0, -2.0),
            phase: WinitTouchPhase::Moved,
        },
        WinitWindowEvent::MouseWheel {
            device_id,
            delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(4.0, 8.0)),
            phase: WinitTouchPhase::Ended,
        },
        WinitWindowEvent::Touch(contact),
    ] {
        app.frame_requested = false;
        app.handle_mapped_window_event(window, event).unwrap();
        assert!(app.frame_requested);
    }
    app.handle_mapped_window_event(window, WinitWindowEvent::Focused(false))
        .unwrap();
    app.apply_text_input(window, ElementState::Pressed, Some("unfocused"), false)
        .unwrap();
    app.handle_mapped_window_event(
        window,
        WinitWindowEvent::Touch(Touch {
            phase: WinitTouchPhase::Ended,
            ..contact
        }),
    )
    .unwrap();
    app.run_frame_transition(Duration::ZERO);
    assert!(app.is_healthy_live());

    let text = messages::<TextInput>(&app);
    assert_eq!(
        text.iter()
            .map(|input| input.text.as_str())
            .collect::<Vec<_>>(),
        ["é", "é", "文"]
    );
    assert!(text.iter().all(|input| input.window == input_window));
    let ime = messages::<ImeEvent>(&app);
    assert_eq!(
        ime[1].kind,
        ImeEventKind::Preedit {
            text: "文".into(),
            cursor: Some((3, 3)),
        }
    );
    assert_eq!(ime.last().unwrap().kind, ImeEventKind::Disabled);
    let wheel = messages::<nara_input::MouseWheel>(&app);
    assert_eq!(wheel.len(), 2);
    assert_eq!(wheel[0].position, Some(Vec2::new(22.0, 39.0)));
    assert_eq!(wheel[0].delta, Vec2::new(1.0, -2.0));
    assert_eq!(wheel[0].unit, MouseWheelUnit::Lines);
    assert_eq!(wheel[1].delta, Vec2::new(4.0, 8.0));
    assert_eq!(wheel[1].unit, MouseWheelUnit::Pixels);
    assert_eq!(wheel[1].phase, TouchPhase::Ended);
    let touch = messages::<TouchInput>(&app);
    assert_eq!(touch.len(), 2);
    assert_eq!(touch[0].phase, TouchPhase::Started);
    assert_eq!(
        touch[1],
        TouchInput {
            phase: TouchPhase::Cancelled,
            ..touch[0]
        }
    );
    assert_eq!(touch[0].position, Vec2::new(3.0, 6.0));
    assert_eq!(touch[0].force, Some(0.5));

    app.begin_shutdown_transition(Instant::now());
    for _ in 0..8 {
        if app.shutdown == WinitShutdownState::Complete {
            break;
        }
        app.poll_native_shutdown_once(Instant::now());
    }
    assert_eq!(app.runtime.state(), RuntimeState::Stopped);
}

#[test]
fn physical_samples_scale_fractional_coordinates_to_logical_units() {
    assert_eq!(
        logical_position(PhysicalPosition::new(-15.0, 7.5), 1.5),
        Vec2::new(-10.0, 5.0)
    );
}
