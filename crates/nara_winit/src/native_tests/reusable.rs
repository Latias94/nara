//! One process, one physical loop, sequential independently started Runtime generations.

use super::*;
use std::{cell::RefCell, rc::Rc, sync::mpsc::Receiver};

#[derive(Default)]
struct Observations {
    window: Option<WinitWindowId>,
    stale_wake_accepted: Option<bool>,
    timed_out: bool,
}

struct SequentialDriver {
    proxy: EventLoopProxy<WinitUserEvent>,
    observe_window: Sender<WinitWindowId>,
    window: Receiver<WinitWindowId>,
    old_wake: Option<(EventLoopProxy<WinitUserEvent>, SessionWakeId, Sender<bool>)>,
    wake_result: Receiver<bool>,
    observations: Rc<RefCell<Observations>>,
    sent: bool,
    expects_stale_wake: bool,
    deadline: Instant,
}

impl __WinitPresentationDriver for SequentialDriver {
    fn maintain(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        now: Instant,
    ) -> Result<__WinitPresentationProgress, AppRunError> {
        if !matches!(
            runtime.state(),
            RuntimeState::Running | RuntimeState::Paused
        ) {
            return Ok(__WinitPresentationProgress::default());
        }
        if !self.sent && handles.is_registered(WindowId::PRIMARY) {
            self.proxy
                .send_event(WinitUserEvent::NativeTest(NativeTestEvent::ObserveWindow {
                    window: self.observe_window.clone(),
                }))
                .map_err(|_| AppRunError::runner("native observation proxy closed"))?;
            if let Some((proxy, wake_id, accepted)) = self.old_wake.take() {
                // Send only after the new window exists. The handoff drain cannot consume this
                // late old-proxy wake; the current App must reject the old physical-pump ticket.
                #[cfg(feature = "platform-accessibility")]
                proxy
                    .send_event(WinitUserEvent::AccessibilityWake(wake_id))
                    .map_err(|_| AppRunError::runner("old accessibility proxy closed"))?;
                proxy
                    .send_event(WinitUserEvent::NativeTest(NativeTestEvent::ProbeWake {
                        wake_id,
                        accepted,
                    }))
                    .map_err(|_| AppRunError::runner("old native proxy closed"))?;
            }
            self.sent = true;
        }
        let mut observations = self.observations.borrow_mut();
        if let Ok(window) = self.window.try_recv() {
            observations.window = Some(window);
        }
        if let Ok(accepted) = self.wake_result.try_recv() {
            observations.stale_wake_accepted = Some(accepted);
        }
        let observed = observations.window.is_some()
            && (!self.expects_stale_wake || observations.stale_wake_accepted.is_some());
        if now >= self.deadline {
            observations.timed_out = true;
        }
        Ok(__WinitPresentationProgress {
            exit: (observed || observations.timed_out).then_some(AppExit::Requested),
            next_wake: Some((now + Duration::from_millis(10)).min(self.deadline)),
        })
    }

    fn retire(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        ids: &[WindowId],
        now: Instant,
    ) -> Result<bool, AppRunError> {
        NoPresentationDriver.retire(runtime, handles, ids, now)
    }
}

fn observed_session(
    event_loop: WinitEventLoop,
    old_wake: Option<(EventLoopProxy<WinitUserEvent>, SessionWakeId)>,
) -> WinitSession {
    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: Some(Window::new(
            "nara sequential physical pump",
            WindowResolution::new(320, 180),
        )),
    })
    .unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let (observe_window, window) = mpsc::channel();
    let (accepted, wake_result) = mpsc::channel();
    let observations = Rc::new(RefCell::new(Observations::default()));
    let driver = SequentialDriver {
        proxy: event_loop.event_loop.create_proxy(),
        observe_window,
        window,
        expects_stale_wake: old_wake.is_some(),
        old_wake: old_wake.map(|(proxy, wake_id)| (proxy, wake_id, accepted)),
        wake_result,
        observations: observations.clone(),
        sent: false,
        deadline: Instant::now() + Duration::from_secs(3),
    };
    let session = WinitRunner::new(WinitControlFlow::Wait).__run_with_presentation_on(
        RuntimeInstance::start(app).unwrap(),
        Box::new(driver),
        Duration::from_millis(500),
        Some(event_loop),
    );
    assert!(session.is_retired());
    assert_eq!(session.runtime().state(), RuntimeState::Stopped);
    assert!(
        session
            .app
            .as_ref()
            .unwrap()
            .nara_windows_by_winit
            .is_empty()
    );
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::NativeDestroyed
    );
    let observations = observations.borrow();
    assert!(!observations.timed_out);
    assert!(observations.window.is_some());
    assert!(
        observations
            .stale_wake_accepted
            .is_none_or(|accepted| !accepted)
    );
    session
}

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own nextest process"]
fn retired_pump_runs_sequential_apps_and_rejects_queued_and_late_old_proxy_events() {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = WinitEventLoop::from_native(builder.build().unwrap());
    let first = observed_session(event_loop, None);
    let generation = first.runtime().generation();
    let app = first.app.as_ref().unwrap();
    let old_proxy = app.event_proxy.clone().unwrap();
    let old_wake_id = app.session_wake_id.unwrap();
    let retired = first.into_retired().unwrap();
    assert_eq!(retired.result, Ok(AppExit::Requested));
    let event_loop = retired.event_loop.unwrap();
    drop(retired.runtime);

    // If dispatched to the next App, this event would retain its new window and exit its pump.
    // It must be discarded by the no-Runtime handoff handler before any new native creation.
    let (retained, unexpected_window) = mpsc::channel();
    old_proxy
        .send_event(WinitUserEvent::NativeTest(
            NativeTestEvent::RetainWindowAndExit { window: retained },
        ))
        .unwrap();
    let second = observed_session(event_loop, Some((old_proxy, old_wake_id)));
    assert_ne!(second.runtime().generation(), generation);
    assert!(unexpected_window.try_recv().is_err());
    assert_ne!(
        second.app.as_ref().unwrap().session_wake_id,
        Some(old_wake_id)
    );
    let retired = second.into_retired().unwrap();
    assert_eq!(retired.result, Ok(AppExit::Requested));
    assert!(retired.event_loop.is_some());
}

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own nextest process"]
fn supplied_unused_pump_survives_initial_failure_and_runs_the_next_app() {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = WinitEventLoop::from_native(builder.build().unwrap());
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let generation = runtime.generation();
    let session = WinitRunner::default().run_on(event_loop, runtime);
    assert!(session.is_retired());
    assert_eq!(session.runtime().generation(), generation);
    assert!(!session.event_loop.as_ref().unwrap().resumed);
    assert!(session.app.as_ref().unwrap().event_proxy.is_none());
    let retired = session.into_retired().unwrap();
    assert_eq!(
        retired.result,
        Err(AppRunError::runner("backend window authority is missing"))
    );
    let event_loop = retired.event_loop.unwrap();
    drop(retired.runtime);
    let session = observed_session(event_loop, None);
    assert_ne!(session.runtime().generation(), generation);
    assert_eq!(
        session.into_retired().unwrap().result,
        Ok(AppExit::Requested)
    );
}

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own nextest process"]
fn bounded_polls_keep_the_original_hidden_paused_window_until_reveal_and_close() {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = WinitEventLoop::from_native(builder.build().unwrap());
    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: Some(Window::new(
            "nara hidden polling session",
            WindowResolution::new(320, 180),
        )),
    })
    .unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let runtime = RuntimeInstance::start(app).unwrap();
    let generation = runtime.generation();
    let mut session = WinitRunner::new(WinitControlFlow::Wait)
        .with_initial_visibility(false)
        .__start_with_presentation_on(
            runtime,
            Box::new(NoPresentationDriver),
            Duration::from_secs(1),
            Some(event_loop),
        );
    assert!(!session.executed);
    assert!(session.app.as_ref().unwrap().platform_windows.is_empty());
    assert!(session.reveal().is_err());
    let RuntimeControlRequestResult::Accepted(pause) =
        session.request_control(RuntimeControl::Pause)
    else {
        panic!("the unexecuted original Runtime must accept Pause");
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while !session.paused_frame_complete() && Instant::now() < deadline {
        assert_eq!(
            session.poll(Duration::from_millis(5)),
            WinitRunProgress::Running
        );
    }
    assert!(session.paused_frame_complete());
    assert_eq!(
        session.control_status(pause),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(session.runtime().generation(), generation);
    let native_id = {
        let app = session.app.as_ref().unwrap();
        let window = app.platform_windows.get(&WindowId::PRIMARY).unwrap();
        assert_eq!(window.is_visible(), Some(false));
        assert!(!window.has_focus());
        window.id()
    };
    for _ in 0..3 {
        assert_eq!(session.poll(Duration::ZERO), WinitRunProgress::Running);
        assert_eq!(session.runtime().generation(), generation);
        assert_eq!(session.runtime().state(), RuntimeState::Paused);
    }
    session.reveal().unwrap();
    assert!(!session.paused_frame_complete());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !session.paused_frame_complete() && Instant::now() < deadline {
        assert_eq!(
            session.poll(Duration::from_millis(5)),
            WinitRunProgress::Running
        );
    }
    assert!(session.paused_frame_complete());
    {
        let app = session.app.as_ref().unwrap();
        let window = app.platform_windows.get(&WindowId::PRIMARY).unwrap();
        assert_eq!(window.id(), native_id);
        assert_eq!(window.is_visible(), Some(true));
        assert_eq!(app.owned_window_ids, BTreeSet::from([WindowId::PRIMARY]));
    }
    session.request_close();
    assert!(!session.paused_frame_complete());
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    session.request_close();
    assert_eq!(session.poll(Duration::ZERO), WinitRunProgress::Retired);
    assert!(session.reveal().is_err());
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::NativeDestroyed
    );
    let retired = session.into_retired().unwrap();
    assert_eq!(retired.runtime.generation(), generation);
    assert_eq!(retired.result, Ok(AppExit::Requested));
    assert!(retired.event_loop.is_some());
}

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own nextest process"]
fn hidden_fullscreen_is_deferred_until_the_original_window_is_revealed() {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = WinitEventLoop::from_native(builder.build().unwrap());
    let mut app = App::new();
    let mut window = Window::new("nara deferred fullscreen", WindowResolution::new(320, 180));
    window.mode = nara_window::WindowMode::BorderlessFullscreen;
    app.add_plugin(WindowPlugin {
        primary_window: Some(window),
    })
    .unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let runtime = RuntimeInstance::start(app).unwrap();
    let mut session = WinitRunner::new(WinitControlFlow::Wait)
        .with_initial_visibility(false)
        .__start_with_presentation_on(
            runtime,
            Box::new(NoPresentationDriver),
            Duration::from_secs(2),
            Some(event_loop),
        );
    assert!(matches!(
        session.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let deadline = Instant::now() + Duration::from_secs(3);
    while !session.paused_frame_complete() && Instant::now() < deadline {
        assert_eq!(
            session.poll(Duration::from_millis(5)),
            WinitRunProgress::Running
        );
    }
    assert!(session.paused_frame_complete());
    let native_id = {
        let app = session.app.as_ref().unwrap();
        let native = app.platform_windows.get(&WindowId::PRIMARY).unwrap();
        assert_eq!(native.is_visible(), Some(false));
        assert!(!native.has_focus());
        assert!(native.fullscreen().is_none());
        assert!(app.deferred_fullscreen.is_some());
        native.id()
    };
    session.reveal().unwrap();
    {
        let app = session.app.as_ref().unwrap();
        let native = app.platform_windows.get(&WindowId::PRIMARY).unwrap();
        assert_eq!(native.id(), native_id);
        assert!(native.fullscreen().is_some());
        assert_eq!(native.is_visible(), Some(true));
        assert!(app.deferred_fullscreen.is_none());
    }
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::NativeDestroyed
    );
    assert_eq!(
        session.into_retired().unwrap().result,
        Ok(AppExit::Requested)
    );
}

#[derive(Default, nara_ecs::Resource)]
struct NativeKeyboardMovement {
    held: bool,
    position: u64,
    stops: u64,
}

fn native_keyboard_binding() -> nara_input::KeyboardActionBinding {
    nara_input::KeyboardActionBinding {
        key: KeyCode::Character('d'),
        action: nara_input::ActionId::new("test.native-keyboard-d").unwrap(),
        context: nara_input::ActionContext::gameplay(),
    }
}

fn native_keyboard_session(binding: nara_input::KeyboardActionBinding) -> WinitSession {
    use nara_ecs::{Res, ResMut, schedule::IntoScheduleConfigs};
    use nara_gameplay::{GameplayCommandBatch, GameplayCommandSet};
    use nara_input::{ActionBinding, ActionMap, ActionMapBuilder, ActionMapLimits, ActionMaps};

    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = WinitEventLoop::from_native(builder.build().unwrap());
    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: Some(Window::new(
            "nara keyboard handoff",
            WindowResolution::new(320, 180),
        )),
    })
    .unwrap();
    app.insert_resource(nara_app::RuntimeTimeSettings::default().with_paused(true))
        .unwrap();
    app.add_plugin(nara_input::InputPlugin).unwrap();
    app.add_plugin(nara_gameplay::GameplayCommandPlugin::default())
        .unwrap();
    let mut map = ActionMapBuilder::new(ActionMapLimits::default());
    map.declare_context(binding.context.clone(), true).unwrap();
    map.bind(ActionBinding::key(binding.action.clone(), binding.key))
        .unwrap();
    app.insert_resource(ActionMaps::new(
        ActionMap::new(
            map.compile().unwrap().into_bindings(),
            nara_core::ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap(),
    ))
    .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<nara_gameplay::ActionCommandMap>()
        .bind(nara_gameplay::ActionCommandBinding::new(
            binding.action,
            nara_input::ActionPhase::Released,
            nara_gameplay::GameplayCommandTypeId::new("test.native-keyboard-stop").unwrap(),
        ))
        .unwrap();
    app.insert_resource(NativeKeyboardMovement {
        held: true,
        ..NativeKeyboardMovement::default()
    })
    .unwrap();
    app.add_systems(
        nara_app::CoreStage::FixedUpdate,
        (|batch: Res<GameplayCommandBatch>, mut movement: ResMut<NativeKeyboardMovement>| {
            for command in batch.commands() {
                if command.command_type()
                    == &nara_gameplay::GameplayCommandTypeId::new("test.native-keyboard-stop")
                        .unwrap()
                {
                    movement.held = false;
                    movement.stops += 1;
                }
            }
            if movement.held {
                movement.position += 1;
            }
        })
        .in_set(GameplayCommandSet::Consume),
    )
    .unwrap();
    WinitRunner::new(WinitControlFlow::Wait)
        .with_initial_visibility(false)
        .with_input_isolation(true)
        .__start_with_presentation_on(
            RuntimeInstance::start(app).unwrap(),
            Box::new(BoundedPlatformDriver {
                deadline: Instant::now() + Duration::from_secs(15),
            }),
            Duration::from_secs(1),
            Some(event_loop),
        )
}

fn poll_native_keyboard_until(
    session: &mut WinitSession,
    condition: impl Fn(&WinitSession) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(3);
    for _ in 0..512 {
        if condition(session) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native keyboard deadline expired: {:?}",
            session.input_activation()
        );
        assert_eq!(
            session.poll(Duration::from_millis(5)),
            WinitRunProgress::Running
        );
    }
    panic!(
        "native keyboard poll budget exhausted: {:?}",
        session.input_activation()
    );
}

fn keyboard_hwnd(window: &WinitWindow) -> windows_sys::Win32::Foundation::HWND {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match window.window_handle().unwrap().as_raw() {
        RawWindowHandle::Win32(handle) => handle.hwnd.get() as windows_sys::Win32::Foundation::HWND,
        handle => panic!("expected original Win32 keyboard window, got {handle:?}"),
    }
}

fn native_d_is_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetKeyboardLayout, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW,
    };
    let vk = unsafe { MapVirtualKeyExW(0x20, MAPVK_VSC_TO_VK_EX, GetKeyboardLayout(0)) };
    assert_ne!(vk, 0, "the test requires a mapped physical D key");
    unsafe { GetAsyncKeyState(vk as i32) < 0 }
}

fn send_native_d(up: bool) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, SendInput,
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: 0x20,
                dwFlags: KEYEVENTF_SCANCODE | if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) == 1 }
}

struct NativeDRelease(bool);

impl NativeDRelease {
    fn press() -> Self {
        let release = Self(true);
        assert!(send_native_d(false), "SendInput Down failed");
        release
    }

    fn release(&mut self) {
        assert!(send_native_d(true), "SendInput Up cleanup failed");
        self.0 = false;
    }
}

impl Drop for NativeDRelease {
    fn drop(&mut self) {
        if self.0 && !send_native_d(true) {
            eprintln!("native keyboard test could not release its injected D key");
        }
    }
}

fn restore_native_d(session: &mut WinitSession) {
    let binding = native_keyboard_binding();
    poll_native_keyboard_until(session, |session| {
        session
            .capture_keyboard_holds(std::slice::from_ref(&binding))
            .is_ok()
    });
    let admission = session
        .restore_keyboard_holds(vec![nara_input::KeyboardHold {
            binding: binding.clone(),
            held: true,
        }])
        .unwrap();
    assert_eq!(admission.admitted(), 1);
    poll_native_keyboard_until(session, |session| {
        session
            .capture_keyboard_holds(std::slice::from_ref(&binding))
            .is_ok_and(|holds| holds[0].held)
    });
}

fn step_native_keyboard(session: &mut WinitSession) {
    let RuntimeControlRequestResult::Accepted(step) =
        session.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("the original native keyboard owner must accept Step");
    };
    poll_native_keyboard_until(session, |session| {
        session.control_status(step) == Some(RuntimeControlStatus::Applied)
            && session.paused_frame_complete()
    });
    let movement = session
        .runtime()
        .world()
        .resource::<NativeKeyboardMovement>();
    assert!(!movement.held);
    assert_eq!(movement.position, 0);
    assert_eq!(movement.stops, 1);
    assert_eq!(session.runtime().state(), RuntimeState::Paused);
}

fn retire_native_keyboard(mut session: WinitSession, original: Arc<WinitWindow>) {
    let generation = session.runtime().generation();
    let weak = Arc::downgrade(&original);
    drop(original);
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert!(weak.upgrade().is_none());
    let retired = session.into_retired().unwrap();
    assert_eq!(retired.runtime.generation(), generation);
    assert_eq!(retired.result, Ok(AppExit::Requested));
}

#[test]
#[ignore = "requires an interactive Windows desktop; run serially in its own nextest process"]
fn native_keyboard_release_gap_requires_one_original_stop_step_before_activation() {
    let mut session = native_keyboard_session(native_keyboard_binding());
    let generation = session.runtime().generation();
    restore_native_d(&mut session);
    let window = session.app.as_ref().unwrap().platform_windows[&WindowId::PRIMARY].clone();
    assert_eq!(window.is_visible(), Some(false));
    assert!(
        !native_d_is_down(),
        "physical D must be up for the release-gap control"
    );
    assert_eq!(
        session.request_input_activation(),
        Err(WinitKeyboardContinuationError::Unavailable)
    );
    assert_eq!(session.input_activation(), WinitInputActivation::Isolated);
    session.reveal().unwrap();
    poll_native_keyboard_until(&mut session, WinitSession::paused_frame_complete);
    assert!(matches!(
        session.request_input_activation().unwrap(),
        WinitInputActivation::AwaitingFocus | WinitInputActivation::ReconciliationPending
    ));
    poll_native_keyboard_until(&mut session, |session| {
        session.input_activation() == WinitInputActivation::ReconciliationPending
    });
    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(session.runtime().state(), RuntimeState::Paused);
    assert!(
        session
            .runtime()
            .world()
            .resource::<NativeKeyboardMovement>()
            .held
    );
    assert!(matches!(
        session.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Rejected(_)
    ));
    assert!(
        session
            .runtime()
            .world()
            .resource::<InputTimeline>()
            .entries()
            .any(|entry| {
                matches!(
                    entry.kind(),
                    nara_input::InputEventKind::Key {
                        key: KeyCode::Character('d'),
                        phase: nara_input::ButtonTransitionPhase::Released,
                    }
                ) && entry.invalidation()
                    == Some(nara_input::InputInvalidationReason::ActivationMismatch)
                    && entry.origin() == nara_input::InputEventOrigin::Platform
                    && entry.member_origin() == Some(nara_input::InputEventOrigin::Restored)
            })
    );
    step_native_keyboard(&mut session);
    assert_eq!(session.input_activation(), WinitInputActivation::Active);
    assert!(
        !session
            .capture_keyboard_holds(&[native_keyboard_binding()])
            .unwrap()[0]
            .held
    );
    assert!(!native_d_is_down());
    retire_native_keyboard(session, window);
}

#[test]
#[ignore = "injects a matched native D Down/Up; requires an interactive Windows desktop and serial execution"]
fn native_posted_key_up_releases_a_physically_confirmed_restored_member_once() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, PostMessageW, WM_KEYUP,
    };

    let mut session = native_keyboard_session(native_keyboard_binding());
    let generation = session.runtime().generation();
    restore_native_d(&mut session);
    let window = session.app.as_ref().unwrap().platform_windows[&WindowId::PRIMARY].clone();
    session.reveal().unwrap();
    window.focus_window();
    let hwnd = keyboard_hwnd(&window);
    poll_native_keyboard_until(&mut session, |session| {
        session.paused_frame_complete()
            && window.has_focus()
            && unsafe { GetForegroundWindow() == hwnd }
    });
    assert_eq!(session.input_activation(), WinitInputActivation::Isolated);
    assert!(
        !native_d_is_down(),
        "physical D must be up before injecting the matched test hold"
    );
    let mut release = NativeDRelease::press();
    poll_native_keyboard_until(&mut session, |session| {
        session.paused_frame_complete() && native_d_is_down()
    });
    session.request_input_activation().unwrap();
    poll_native_keyboard_until(&mut session, |session| {
        session.input_activation() == WinitInputActivation::Active
    });
    assert!(native_d_is_down());
    assert!(
        session
            .capture_keyboard_holds(&[native_keyboard_binding()])
            .unwrap()[0]
            .held
    );
    assert!(
        session
            .runtime()
            .world()
            .resource::<nara_input::SemanticActions>()
            .pressed(
                InputSeatId::PRIMARY,
                &native_keyboard_binding().action,
                &native_keyboard_binding().context,
            )
    );
    assert_eq!(
        session
            .runtime()
            .world()
            .resource::<nara_gameplay::GameplayCommandQueue>()
            .stats()
            .accepted,
        0
    );
    assert_ne!(
        unsafe { PostMessageW(hwnd, WM_KEYUP, 0x44, 0xc0200001u32 as isize) },
        0
    );
    poll_native_keyboard_until(&mut session, |session| {
        !session
            .runtime()
            .world()
            .resource::<nara_input::ButtonInput<KeyCode>>()
            .pressed(KeyCode::Character('d'))
    });
    assert!(
        session
            .runtime()
            .world()
            .resource::<InputTimeline>()
            .entries()
            .any(|entry| {
                matches!(
                    entry.kind(),
                    nara_input::InputEventKind::Key {
                        key: KeyCode::Character('d'),
                        phase: nara_input::ButtonTransitionPhase::Released,
                    }
                ) && entry.origin() == nara_input::InputEventOrigin::Platform
                    && entry.member_origin() == Some(nara_input::InputEventOrigin::Restored)
                    && entry.invalidation().is_none()
            })
    );
    step_native_keyboard(&mut session);
    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(
        session
            .runtime()
            .world()
            .resource::<nara_gameplay::GameplayCommandQueue>()
            .stats()
            .accepted,
        1
    );
    assert!(
        !session
            .runtime()
            .world()
            .resource::<nara_input::SemanticActions>()
            .pressed(
                InputSeatId::PRIMARY,
                &native_keyboard_binding().action,
                &native_keyboard_binding().context,
            )
    );
    release.release();
    poll_native_keyboard_until(&mut session, |session| {
        session.paused_frame_complete() && !native_d_is_down()
    });
    assert_eq!(
        session
            .runtime()
            .world()
            .resource::<NativeKeyboardMovement>()
            .stops,
        1
    );
    retire_native_keyboard(session, window);
}

#[test]
#[ignore = "requires an interactive Windows desktop; run serially in its own nextest process"]
fn unsupported_native_keyboard_query_keeps_the_original_candidate_paused() {
    let binding = nara_input::KeyboardActionBinding {
        key: KeyCode::Character('é'),
        ..native_keyboard_binding()
    };
    assert!(crate::keyboard_continuation::physical_key(binding.key).is_none());
    let mut session = native_keyboard_session(binding.clone());
    poll_native_keyboard_until(&mut session, |session| {
        session
            .capture_keyboard_holds(std::slice::from_ref(&binding))
            .is_ok_and(|holds| !holds[0].held)
    });
    session
        .restore_keyboard_holds(vec![nara_input::KeyboardHold {
            binding: binding.clone(),
            held: true,
        }])
        .unwrap();
    poll_native_keyboard_until(&mut session, |session| {
        session
            .capture_keyboard_holds(std::slice::from_ref(&binding))
            .is_ok_and(|holds| holds[0].held)
    });
    let window = session.app.as_ref().unwrap().platform_windows[&WindowId::PRIMARY].clone();
    session.reveal().unwrap();
    poll_native_keyboard_until(&mut session, WinitSession::paused_frame_complete);
    assert_eq!(
        session.request_input_activation(),
        Err(WinitKeyboardContinuationError::Unsupported)
    );
    assert_eq!(
        session.input_activation(),
        WinitInputActivation::Unavailable
    );
    assert_eq!(
        session.input_activation_error(),
        Some(&WinitKeyboardContinuationError::Unsupported)
    );
    assert!(matches!(
        session.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Rejected(_)
    ));
    assert!(matches!(
        session.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Rejected(_)
    ));
    assert_eq!(session.runtime().state(), RuntimeState::Paused);
    assert_eq!(
        session
            .runtime()
            .world()
            .resource::<NativeKeyboardMovement>()
            .position,
        0
    );
    retire_native_keyboard(session, window);
}
