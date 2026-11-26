use super::*;
use crate::winit::maintenance_wake::MaintenanceWakeState;

struct WakeDriver(Rc<DriverCounts>);

impl WinitPresentationDriver for WakeDriver {
    fn maintain(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        now: Instant,
    ) -> Result<WinitPresentationProgress, AppRunError> {
        self.0.maintained.set(self.0.maintained.get() + 1);
        NoPresentationDriver.maintain(runtime, handles, now)
    }

    fn retire(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        ids: &[WindowId],
        now: Instant,
    ) -> Result<bool, AppRunError> {
        self.0.retired.set(self.0.retired.get() + 1);
        NoPresentationDriver.retire(runtime, handles, ids, now)
    }
}

fn wake_session(event_loop: WinitEventLoop) -> (WinitSession, Rc<DriverCounts>) {
    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: Some(Window::new(
            "nara original maintenance hint",
            WindowResolution::new(320, 180),
        )),
    })
    .unwrap();
    app.insert_resource(RuntimeTimeSettings::default().with_paused(true))
        .unwrap();
    app.init_resource::<FrameCounts>().unwrap();
    app.add_systems(CoreStage::TaskUpdate, |mut counts: ResMut<FrameCounts>| {
        counts.tasks += 1;
    })
    .unwrap();
    app.add_systems(CoreStage::FixedUpdate, |mut counts: ResMut<FrameCounts>| {
        counts.fixed += 1;
    })
    .unwrap();
    app.add_systems(CoreStage::Last, |mut counts: ResMut<FrameCounts>| {
        counts.last += 1;
    })
    .unwrap();
    let counts = Rc::new(DriverCounts::default());
    let session = WinitRunner::new(WinitControlFlow::Poll).start_with_presentation_on(
        RuntimeInstance::start(app).unwrap(),
        Box::new(WakeDriver(counts.clone())),
        RETIREMENT_BUDGET,
        Some(event_loop),
    );
    (session, counts)
}

fn quiet_wait(session: &mut WinitSession) -> Observation {
    session.set_control_flow(WinitControlFlow::Wait).unwrap();
    let deadline = Instant::now() + PHASE_BUDGET;
    let mut stable = observation(session);
    let mut quiet_since = Instant::now();
    loop {
        assert!(
            Instant::now() < deadline,
            "native hint session did not settle"
        );
        poll_live(session);
        let current = observation(session);
        if current != stable {
            stable = current;
            quiet_since = Instant::now();
        } else if quiet_since.elapsed() >= QUIET_INTERVAL {
            assert!(session.paused_frame_complete());
            assert_eq!(
                session.app.as_ref().unwrap().pump_timeout(Instant::now()),
                None
            );
            return stable;
        }
    }
}

fn poll_until_acknowledged(
    session: &mut WinitSession,
    state: &MaintenanceWakeState,
    before: usize,
) {
    let deadline = Instant::now() + PHASE_BUDGET;
    while state.acknowledgements() == before && Instant::now() < deadline {
        poll_live(session);
    }
    assert!(
        state.acknowledgements() > before,
        "original handler did not acknowledge the hint"
    );
    assert!(!state.is_pending());
}

#[test]
#[ignore = "requires an idle interactive Windows desktop; run in its own nextest process"]
fn native_maintenance_hint_wakes_original_wait_without_app_time_or_capture_changes() {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = WinitEventLoop::from_native(builder.build().unwrap());
    let (mut session, driver) = wake_session(event_loop);
    let generation = session.runtime().generation();
    let handles = session.app.as_ref().unwrap().backend_windows.clone();
    let wake = session.maintenance_wake().unwrap();
    let state = session
        .app
        .as_ref()
        .unwrap()
        .maintenance_wake
        .clone()
        .unwrap();
    let mut original_window = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let (native_id, weak) = settle_native_window(&mut session);
        original_window = Some(weak);
        let quiet = quiet_wait(&mut session);
        let phase = session.capture_time_continuation().unwrap();
        let acknowledged = state.acknowledgements();
        let maintained = driver.maintained.get();
        let other_thread = wake.clone();
        let worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            let sent_at = Instant::now();
            (sent_at, other_thread.notify())
        });
        let deadline = Instant::now() + PHASE_BUDGET;
        let mut waited_across_send = Vec::new();
        let mut progress = WinitRunProgress::Running;
        while state.acknowledgements() == acknowledged && Instant::now() < deadline {
            let entered = Instant::now();
            progress = session.poll(Duration::from_secs(1));
            let returned = Instant::now();
            if waited_across_send.len() < 256 {
                waited_across_send.push((entered, returned));
            }
            if progress != WinitRunProgress::Running {
                break;
            }
        }
        let observed_at = Instant::now();
        let (sent_at, accepted) = worker.join().unwrap();
        assert!(accepted);
        assert_eq!(progress, WinitRunProgress::Running);
        assert_eq!(state.acknowledgements(), acknowledged + 1);
        assert!(!state.is_pending());
        assert!(
            waited_across_send
                .iter()
                .any(|(entered, returned)| *entered <= sent_at && sent_at <= *returned),
            "no original native wait covered the other thread's notification"
        );
        assert!(observed_at.saturating_duration_since(sent_at) < Duration::from_millis(500));
        assert!(driver.maintained.get() > maintained);
        assert_eq!(observation(&session), quiet);
        assert!(session.paused_frame_complete());
        assert_eq!(session.capture_time_continuation().unwrap(), phase);

        for _ in 0..3 {
            let acknowledged = state.acknowledgements();
            let other_thread = wake.clone();
            let accepted = std::thread::spawn(move || (0..4096).all(|_| other_thread.notify()))
                .join()
                .unwrap();
            assert!(accepted);
            assert!(state.is_pending());
            poll_until_acknowledged(&mut session, &state, acknowledged);
            assert_eq!(state.acknowledgements(), acknowledged + 1);
            assert_eq!(observation(&session), quiet);
            assert_eq!(session.capture_time_continuation().unwrap(), phase);
        }

        let redraw_at = Instant::now();
        session.app.as_ref().unwrap().platform_windows[&WindowId::PRIMARY].request_redraw();
        let redrawn = wait_for_full_frame(&mut session, quiet);
        assert!(
            redrawn.real.elapsed - quiet.real.elapsed
                >= redraw_at.saturating_duration_since(quiet.last_frame)
        );
        let quiet = quiet_wait(&mut session);
        apply_control(&mut session, RuntimeControl::StepFixedTick);
        wait_for_full_frame(&mut session, quiet);
        let stepped = quiet_wait(&mut session);
        assert_eq!(stepped.fixed_tick, quiet.fixed_tick + 1);
        assert_eq!(stepped.counts.fixed, quiet.counts.fixed + 1);
        assert_eq!(session.runtime().generation(), generation);
        assert_eq!(
            session.app.as_ref().unwrap().platform_windows[&WindowId::PRIMARY].id(),
            native_id
        );
    }));
    session.request_close();
    assert!(!wake.notify());
    assert!(session.maintenance_wake().is_none());
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    if let Err(payload) = outcome {
        resume_unwind(payload);
    }
    assert!(original_window.unwrap().upgrade().is_none());
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::NativeDestroyed
    );
    assert!(driver.retired.get() > 0);
    let retired = session.into_retired().unwrap();
    assert_eq!(retired.runtime.generation(), generation);
    assert_eq!(retired.result, Ok(AppExit::Requested));
    assert!(retired.event_loop.is_some());
    assert!(!wake.notify());
}

#[test]
#[ignore = "requires an idle interactive Windows desktop; run in its own nextest process"]
fn retired_maintenance_wake_rejects_old_routes_and_preserves_new_hints_during_handoff() {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = WinitEventLoop::from_native(builder.build().unwrap());
    let (mut first, _) = wake_session(event_loop);
    let generation = first.runtime().generation();
    let old_wake = first.maintenance_wake().unwrap();
    let old_id = first.app.as_ref().unwrap().session_wake_id.unwrap();
    let old_proxy = first.app.as_ref().unwrap().event_proxy.clone().unwrap();
    let first_handles = first.app.as_ref().unwrap().backend_windows.clone();
    let mut first_window = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        first_window = Some(settle_native_window(&mut first).1);
        quiet_wait(&mut first);
        assert!(old_wake.notify());
        assert!(
            first
                .app
                .as_ref()
                .unwrap()
                .maintenance_wake
                .as_ref()
                .unwrap()
                .is_pending()
        );
        first.request_close();
        assert!(!old_wake.notify());
        assert!(first.maintenance_wake().is_none());
        first.request_close();
    }));
    first.request_close();
    assert_eq!(first.execute(), WinitRunProgress::Retired);
    if let Err(payload) = outcome {
        resume_unwind(payload);
    }
    assert!(first_window.unwrap().upgrade().is_none());
    assert_eq!(
        first_handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::NativeDestroyed
    );
    let retired = first.into_retired().unwrap();
    assert_eq!(retired.result, Ok(AppExit::Requested));
    let event_loop = retired.event_loop.unwrap();
    drop(retired.runtime);
    old_proxy
        .send_event(WinitUserEvent::MaintenanceWake(old_id))
        .unwrap();
    let (mut second, _) = wake_session(event_loop);
    let new_wake = second.maintenance_wake().unwrap();
    let state = second
        .app
        .as_ref()
        .unwrap()
        .maintenance_wake
        .clone()
        .unwrap();
    let second_handles = second.app.as_ref().unwrap().backend_windows.clone();
    let mut second_window = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        assert_ne!(second.runtime().generation(), generation);
        assert_ne!(second.app.as_ref().unwrap().session_wake_id, Some(old_id));
        assert!(new_wake.notify());
        assert!(state.is_pending());
        assert!(second.event_loop.as_ref().unwrap().needs_drain);
        let deadline = Instant::now() + PHASE_BUDGET;
        while second.event_loop.as_ref().unwrap().needs_drain && Instant::now() < deadline {
            poll_live(&mut second);
            assert_eq!(state.acknowledgements(), 0);
            assert!(state.is_pending());
        }
        assert!(!second.event_loop.as_ref().unwrap().needs_drain);
        poll_until_acknowledged(&mut second, &state, 0);
        second_window = Some(settle_native_window(&mut second).1);
        let quiet = quiet_wait(&mut second);
        let phase = second.capture_time_continuation().unwrap();
        let acknowledged = state.acknowledgements();
        let (send, received) = mpsc::channel();
        old_proxy
            .send_event(WinitUserEvent::MaintenanceWake(old_id))
            .unwrap();
        old_proxy
            .send_event(WinitUserEvent::NativeTest(
                NativeTestEvent::ObserveMaintenancePending { pending: send },
            ))
            .unwrap();
        assert!(new_wake.notify());
        assert!(!old_wake.notify());
        poll_until_acknowledged(&mut second, &state, acknowledged);
        assert_eq!(received.try_recv(), Ok(true));
        assert_eq!(state.acknowledgements(), acknowledged + 1);
        assert_eq!(observation(&second), quiet);
        assert_eq!(second.capture_time_continuation().unwrap(), phase);
    }));
    second.request_close();
    assert!(!new_wake.notify());
    assert!(!old_wake.notify());
    assert_eq!(second.execute(), WinitRunProgress::Retired);
    if let Err(payload) = outcome {
        resume_unwind(payload);
    }
    assert!(second_window.unwrap().upgrade().is_none());
    assert_eq!(
        second_handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::NativeDestroyed
    );
    let retired = second.into_retired().unwrap();
    assert_eq!(retired.result, Ok(AppExit::Requested));
    assert!(retired.event_loop.is_some());
    assert!(!old_wake.notify());
    assert!(!new_wake.notify());
}

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own nextest process"]
fn native_start_failure_and_session_drop_leave_no_live_maintenance_handle() {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let event_loop = WinitEventLoop::from_native(builder.build().unwrap());
    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: None,
    })
    .unwrap();
    let mut failed = WinitRunner::new(WinitControlFlow::Wait).start_with_presentation_on(
        RuntimeInstance::start(app).unwrap(),
        Box::new(NoPresentationDriver),
        RETIREMENT_BUDGET,
        Some(event_loop),
    );
    let failed_wake = failed.maintenance_wake();
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        assert!(failed_wake.as_ref().unwrap().notify());
        let deadline = Instant::now() + PHASE_BUDGET;
        while failed.app.as_ref().unwrap().primary_failure.is_none() && Instant::now() < deadline {
            failed.poll(Duration::ZERO);
        }
        assert_eq!(
            failed.app.as_ref().unwrap().primary_failure,
            Some(AppRunError::runner(
                "managed runtime has no configured window target"
            ))
        );
        assert!(!failed_wake.as_ref().unwrap().notify());
        assert!(failed.maintenance_wake().is_none());
        assert!(failed.app.as_ref().unwrap().session_wake_id.is_some());
    }));
    failed.request_close();
    assert_eq!(failed.execute(), WinitRunProgress::Retired);
    if let Err(payload) = outcome {
        resume_unwind(payload);
    }
    let retired = failed.into_retired().unwrap();
    assert_eq!(
        retired.result,
        Err(AppRunError::runner(
            "managed runtime has no configured window target"
        ))
    );
    let event_loop = retired.event_loop.unwrap();
    drop(retired.runtime);
    let (mut session, driver) = wake_session(event_loop);
    let wake = session.maintenance_wake().unwrap();
    let handles = session.app.as_ref().unwrap().backend_windows.clone();
    let mut original_window = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        original_window = Some(settle_native_window(&mut session).1);
        quiet_wait(&mut session);
        assert!(wake.notify());
    }));
    drop(session);
    if let Err(payload) = outcome {
        resume_unwind(payload);
    }
    assert!(!wake.notify());
    assert!(original_window.unwrap().upgrade().is_none());
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::NativeDestroyed
    );
    assert!(driver.retired.get() > 0);
}
