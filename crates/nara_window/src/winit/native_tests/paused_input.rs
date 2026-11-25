use super::*;
use nara_app::{CoreStage, RealTime};
use nara_core::{ByteLimit, Vec2};
use nara_ecs::{Res, ResMut, Resource};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapLimits,
    ActionMaps, ActionOutcome, ActionOutcomes, ActionPhase, ActionValue, ButtonTransitionPhase,
    InputBinding, InputEventKind, InputEventOrigin, InputFrameObservations, InputLimits,
    InputPlugin, InputTimelineEntry, apply_claimed_keyboard_driver_input,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow, PostMessageW, WM_MOUSEMOVE},
};

#[derive(Default, Resource)]
struct ObservedInput {
    moves: Vec<(u64, InputTimelineEntry)>,
    actions: Vec<ActionOutcome>,
}

fn observe_input(
    time: Res<RealTime>,
    input: Res<InputFrameObservations>,
    actions: Res<ActionOutcomes>,
    mut observed: ResMut<ObservedInput>,
) {
    observed.moves.extend(
        input
            .as_slice()
            .iter()
            .filter(|entry| entry.kind() == InputEventKind::PointerMoved)
            .map(|entry| (time.frame, *entry)),
    );
    observed.actions.extend_from_slice(actions.as_slice());
}

fn observed_input(session: &WinitSession) -> &ObservedInput {
    session.runtime().world().resource::<ObservedInput>()
}

fn poll_until(
    session: &mut WinitSession,
    deadline: Instant,
    waiting_for: &str,
    ready: impl Fn(&WinitSession) -> bool,
) {
    while !ready(session) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {waiting_for}"
        );
        let progress = session.poll(Duration::from_millis(10));
        let app = session.app.as_ref().unwrap();
        assert!(
            progress == WinitRunProgress::Running && app.is_healthy_live(),
            "native session left Running while waiting for {waiting_for}: {progress:?}, {:?}",
            app.failure(),
        );
    }
}

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own nextest process"]
fn paused_native_motion_preserves_deferred_key() {
    let limits = InputLimits::new(24, 2, 2, 2).unwrap();
    let action = ActionId::new("test.native-paused-key").unwrap();
    let context = ActionContext::gameplay();
    let mut map = ActionMapBuilder::new(ActionMapLimits::default());
    map.declare_context(context.clone(), true).unwrap();
    map.bind(ActionBinding::key(action.clone(), KeyCode::Space))
        .unwrap();
    let map = ActionMap::new(
        map.compile().unwrap().into_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap();

    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: Some(Window::new(
            "nara paused native input",
            WindowResolution::new(320, 180),
        )),
    })
    .unwrap();
    app.insert_resource(InputTimeline::with_limits(limits).unwrap())
        .unwrap();
    app.insert_resource(ActionMaps::new(map)).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    app.init_resource::<ObservedInput>().unwrap();
    app.add_systems(CoreStage::Last, observe_input).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let runtime = RuntimeInstance::start(app).unwrap();
    let generation = runtime.generation();

    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = WinitEventLoop::from_native(builder.build().unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut session = WinitRunner::new(WinitControlFlow::Wait)
        .with_initial_visibility(false)
        .start_with_presentation_on(
            runtime,
            Box::new(BoundedPlatformDriver { deadline }),
            Duration::from_secs(1),
            Some(event_loop),
        );
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let RuntimeControlRequestResult::Accepted(pause) =
            session.request_control(RuntimeControl::Pause)
        else {
            panic!("the original native session must accept Pause");
        };
        poll_until(&mut session, deadline, "the managed pause", |session| {
            session.control_status(pause) == Some(RuntimeControlStatus::Applied)
                && session.paused_frame_complete()
        });

        let app = session.app.as_ref().unwrap();
        let window = &app.platform_windows[&WindowId::PRIMARY];
        assert_eq!(window.is_visible(), Some(false));
        let RawWindowHandle::Win32(handle) = window.window_handle().unwrap().as_raw() else {
            panic!("expected the original Win32 window");
        };
        let hwnd = handle.hwnd.get() as HWND;
        let mut process_id = 0;
        // SAFETY: The session owns this live HWND and process_id is writable.
        assert_ne!(
            unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) },
            0
        );
        assert_eq!(process_id, std::process::id());
        let original_window = Arc::downgrade(window);
        let scale = window.scale_factor() as f32;

        let claim = app.keyboard_continuation.claim.clone().unwrap();
        let input_window = InputWindowId::new(WindowId::PRIMARY.get());
        assert_eq!(claim.window(), input_window);
        let press = session
            .with_driver_scope(|scope| {
                apply_claimed_keyboard_driver_input(
                    scope,
                    claim,
                    ButtonDriverInput::Press(KeyCode::Space),
                )
            })
            .unwrap();
        assert_eq!(press.post_operation_fault, None);
        let press = press.output.unwrap();
        assert_eq!(press.admitted(), 1);
        let press_sequence = press.first_sequence().unwrap();
        poll_until(&mut session, deadline, "the queued key", |session| {
            session.paused_frame_complete()
        });
        let timeline = session.runtime().world().resource::<InputTimeline>();
        assert_eq!(
            timeline.cursors().oldest_deferred_action(),
            Some(press_sequence)
        );
        let original_press = *timeline
            .entries()
            .find(|entry| entry.sequence() == press_sequence)
            .unwrap();
        assert_eq!(
            original_press.kind(),
            InputEventKind::Key {
                key: KeyCode::Space,
                phase: ButtonTransitionPhase::Pressed,
            },
        );
        assert_eq!(original_press.origin(), InputEventOrigin::Synthetic);
        assert_eq!(original_press.press_sequence(), Some(press_sequence));
        assert_eq!(original_press.route().window(), input_window);

        let before_moves = observed_input(&session).moves.len();
        let mut previous_sequence = press_sequence;
        for index in 0..limits.events() + 8 {
            let x = u16::try_from(index + 1).unwrap();
            const OUTSIDE_CLIENT_Y: i16 = -5;
            let position = Vec2::new(f32::from(x) / scale, f32::from(OUTSIDE_CLIENT_Y) / scale);
            let lparam = (u32::from(x) | (u32::from(OUTSIDE_CLIENT_Y as u16) << 16)) as isize;
            let before_frame = session.runtime().world().resource::<RealTime>().frame;
            // SAFETY: The original HWND is still owned on this thread. The message carries scalars.
            assert_ne!(
                unsafe { PostMessageW(hwnd, WM_MOUSEMOVE, 0, lparam) },
                0,
                "PostMessageW failed: {}",
                std::io::Error::last_os_error(),
            );
            let expected_count = before_moves + index + 1;
            poll_until(&mut session, deadline, "posted WM_MOUSEMOVE", |session| {
                session.paused_frame_complete()
                    && observed_input(session).moves.len() >= expected_count
            });
            let moves = &observed_input(&session).moves;
            assert_eq!(moves.len(), expected_count);
            let (frame, entry) = moves[expected_count - 1];
            assert!(frame > before_frame);
            assert!(entry.sequence() > previous_sequence);
            assert_eq!(entry.logical_position(), Some(position));
            assert_eq!(entry.origin(), InputEventOrigin::Platform);
            assert_eq!(entry.route().window(), input_window);
            assert_eq!(entry.route().seat(), InputSeatId::PRIMARY);
            assert_eq!(entry.route().pointer(), Some(InputPointerId::PRIMARY));
            previous_sequence = entry.sequence();
        }
        assert!(observed_input(&session).actions.is_empty());
        let timeline = session.runtime().world().resource::<InputTimeline>();
        assert!(timeline.entries().eq([&original_press]));

        let RuntimeControlRequestResult::Accepted(resume) =
            session.request_control(RuntimeControl::Resume)
        else {
            panic!("the original native session must accept Resume");
        };
        poll_until(&mut session, deadline, "Resume", |session| {
            session.control_status(resume) == Some(RuntimeControlStatus::Applied)
        });
        assert_eq!(session.runtime().state(), RuntimeState::Running);
        let resumed_frame = session.runtime().world().resource::<RealTime>().frame;
        session.set_control_flow(WinitControlFlow::Poll).unwrap();
        poll_until(&mut session, deadline, "two resumed frames", |session| {
            session.runtime().world().resource::<RealTime>().frame >= resumed_frame + 2
        });
        assert_eq!(
            observed_input(&session).actions,
            [ActionOutcome {
                action,
                context,
                binding: InputBinding::Key(KeyCode::Space),
                phase: ActionPhase::Started,
                value: ActionValue::pressed(),
                provenance: original_press.action_provenance(),
            }],
        );
        let app = session.app.as_ref().unwrap();
        assert_eq!(
            Arc::as_ptr(&app.platform_windows[&WindowId::PRIMARY]),
            original_window.as_ptr()
        );
        (hwnd, original_window)
    }));

    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    let (hwnd, original_window) = outcome.unwrap_or_else(|payload| resume_unwind(payload));
    assert!(original_window.upgrade().is_none());
    // SAFETY: IsWindow accepts a previously valid HWND without dereferencing it.
    assert_eq!(unsafe { IsWindow(hwnd) }, 0);
    let target = handles.snapshot(WindowId::PRIMARY).unwrap();
    assert_eq!(target.phase, WindowTargetPhase::NativeDestroyed);
    assert_eq!(target.fault, None);
    let retired = session.into_retired().unwrap();
    assert!(retired.event_loop.is_some());
    assert_eq!(retired.runtime.generation(), generation);
    assert_eq!(retired.runtime.state(), RuntimeState::Stopped);
    assert_eq!(retired.result, Ok(AppExit::Requested));
}
