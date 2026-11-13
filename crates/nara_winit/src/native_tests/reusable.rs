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
