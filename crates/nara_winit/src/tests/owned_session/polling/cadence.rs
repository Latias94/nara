use super::*;

#[test]
fn poll_to_wait_preserves_settled_paused_frames_and_presentation_deadlines() {
    let app = counted_time_app(false);
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let driver = TestPresentation::new(handles);
    let observations = driver.observations.clone();
    let next_wake = driver.next_wake.clone();
    let mut session = cpu_session(app, WinitControlFlow::Poll);
    session.app.as_mut().unwrap().presentation = Box::new(driver);
    let generation = session.runtime().generation();
    let now = session.app.as_ref().unwrap().last_frame + Duration::from_millis(13);
    let wake = now + Duration::from_secs(5);
    next_wake.set(Some(wake));
    assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
    let pause = accepted(session.request_control(RuntimeControl::Pause));
    assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(pause),
        Some(RuntimeControlStatus::Applied)
    );
    let phase = session.capture_time_continuation().unwrap();
    assert_eq!(phase.pending(), Duration::from_millis(13));
    let paused = counts(&session);
    let before = time_resources(&session);
    let maintained = observations.borrow().maintained_states.len();

    for flow in [
        WinitControlFlow::Poll,
        WinitControlFlow::Wait,
        WinitControlFlow::Wait,
    ] {
        session.set_control_flow(flow).unwrap();
        assert_eq!(session.control_flow(), flow);
        assert_eq!(session.runtime().generation(), generation);
        assert_eq!(session.runtime().state(), RuntimeState::Paused);
        assert!(session.paused_frame_complete());
        assert_eq!(session.capture_time_continuation().unwrap(), phase);
        assert_eq!(counts(&session), paused);
        assert_eq!(time_resources(&session), before);
        let app = session.app.as_ref().unwrap();
        assert!(!app.frame_requested);
        assert_eq!(app.last_frame, now);
        assert_eq!(app.presentation_next_wake, Some(wake));
        assert_eq!(app.retirement_deadline, None);
        assert_eq!(observations.borrow().maintained_states.len(), maintained);
        assert_eq!(
            app.pump_timeout(now),
            Some(if flow == WinitControlFlow::Poll {
                Duration::ZERO
            } else {
                Duration::from_secs(5)
            })
        );
    }

    for seconds in [5, 10] {
        let at = now + Duration::from_secs(seconds);
        let deadline = at + Duration::from_secs(5);
        next_wake.set(Some(deadline));
        assert_eq!(poll_cpu_at(&mut session, at), WinitRunProgress::Running);
        assert_eq!(session.runtime().generation(), generation);
        assert_eq!(session.runtime().state(), RuntimeState::Paused);
        assert_eq!(counts(&session), paused);
        assert_eq!(time_resources(&session), before);
        assert!(session.paused_frame_complete());
        assert_eq!(session.capture_time_continuation().unwrap(), phase);
        let app = session.app.as_ref().unwrap();
        assert!(!app.frame_requested);
        assert_eq!(app.last_frame, now);
        assert_eq!(app.presentation_next_wake, Some(deadline));
        assert_eq!(
            app.running_directive(),
            EventLoopDirective::WaitUntil(deadline)
        );
        assert_eq!(app.pump_timeout(at), Some(Duration::from_secs(5)));
    }
    assert_eq!(
        observations.borrow().maintained_states.len(),
        maintained + 2
    );
    assert!(observations.borrow().retired_states.is_empty());
    next_wake.set(None);
    assert_eq!(
        poll_cpu_at(&mut session, now + Duration::from_secs(15)),
        WinitRunProgress::Running
    );
    assert_eq!(
        observations.borrow().maintained_states.len(),
        maintained + 3
    );
    assert_eq!(counts(&session), paused);
    assert_eq!(time_resources(&session), before);
    assert_eq!(session.capture_time_continuation().unwrap(), phase);
    assert_eq!(session.app.as_ref().unwrap().last_frame, now);
    assert_eq!(
        session.app.as_ref().unwrap().running_directive(),
        EventLoopDirective::Wait
    );
    assert_eq!(session.app.as_ref().unwrap().pump_timeout(now), None);
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert_eq!(
        session.into_retired().unwrap().result,
        Ok(AppExit::Requested)
    );
}

#[test]
fn poll_to_wait_keeps_pending_control_and_the_following_original_paused_frame() {
    for control in [RuntimeControl::Pause, RuntimeControl::StepFixedTick] {
        let mut session = cpu_session(counted_time_app(false), WinitControlFlow::Poll);
        let generation = session.runtime().generation();
        let now = session.app.as_ref().unwrap().last_frame + Duration::from_millis(13);
        assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
        if control == RuntimeControl::StepFixedTick {
            accepted(session.request_control(RuntimeControl::Pause));
            assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
        }
        let before = counts(&session);
        let before_time = time_resources(&session);
        let ticket = accepted(session.request_control(control));
        for flow in [
            WinitControlFlow::Poll,
            WinitControlFlow::Wait,
            WinitControlFlow::Wait,
        ] {
            session.set_control_flow(flow).unwrap();
            assert_eq!(session.control_flow(), flow);
            assert_eq!(session.runtime().generation(), generation);
            assert_eq!(
                session.control_status(ticket),
                Some(RuntimeControlStatus::Pending)
            );
            assert!(!session.paused_frame_complete());
            assert_eq!(counts(&session), before);
            assert_eq!(time_resources(&session), before_time);
            let app = session.app.as_ref().unwrap();
            assert!(app.frame_requested);
            assert_eq!(app.last_frame, now);
            assert_eq!(app.pump_timeout(now), Some(Duration::ZERO));
        }
        assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
        assert_eq!(
            session.control_status(ticket),
            Some(RuntimeControlStatus::Applied)
        );
        assert_eq!(session.runtime().state(), RuntimeState::Paused);
        if control == RuntimeControl::StepFixedTick {
            assert_eq!(
                counts(&session),
                FrameCounts {
                    fixed: before.fixed + 1,
                    ..before
                }
            );
            let stepped = time_resources(&session);
            assert_eq!(stepped.2.tick(), before_time.2.tick() + 1);
            assert_eq!(stepped.2.remainder(), Duration::from_millis(13));
            assert!(!session.paused_frame_complete());
            session.set_control_flow(WinitControlFlow::Wait).unwrap();
            assert!(session.app.as_ref().unwrap().frame_requested);
            assert!(!session.paused_frame_complete());
            assert_eq!(time_resources(&session), stepped);
            assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
        }
        assert_eq!(
            counts(&session),
            FrameCounts {
                tasks: before.tasks + 1,
                fixed: before.fixed + u64::from(control == RuntimeControl::StepFixedTick),
                last: before.last + 1,
            }
        );
        assert!(session.paused_frame_complete());
        let phase = session.capture_time_continuation().unwrap();
        assert_eq!(phase.pending(), Duration::from_millis(13));
        let settled = counts(&session);
        let settled_time = time_resources(&session);
        assert_eq!(
            poll_cpu_at(&mut session, now + Duration::from_secs(60)),
            WinitRunProgress::Running
        );
        assert_eq!(counts(&session), settled);
        assert_eq!(time_resources(&session), settled_time);
        assert_eq!(session.capture_time_continuation().unwrap(), phase);
        session.request_close();
        assert_eq!(session.execute(), WinitRunProgress::Retired);
        assert_eq!(
            session.into_retired().unwrap().result,
            Ok(AppExit::Requested)
        );
    }
}

#[test]
fn poll_to_wait_keeps_explicit_original_phase_maintenance_requested() {
    let mut session = cpu_session(counted_time_app(true), WinitControlFlow::Poll);
    let now = session.app.as_ref().unwrap().last_frame;
    assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
    assert!(session.paused_frame_complete());
    let phase = continuation(&session, Duration::from_millis(13));
    session.restore_time_continuation(phase).unwrap();
    let before = counts(&session);
    let before_time = time_resources(&session);
    let incomplete = RuntimeTimeContinuationError::Rejected(
        RuntimeTimeContinuationRejection::IncompletePausedFrame,
    );
    for _ in 0..2 {
        session.set_control_flow(WinitControlFlow::Wait).unwrap();
        assert_eq!(session.control_flow(), WinitControlFlow::Wait);
        assert!(!session.paused_frame_complete());
        assert_eq!(session.capture_time_continuation(), Err(incomplete.clone()));
        assert_eq!(counts(&session), before);
        assert_eq!(time_resources(&session), before_time);
        let app = session.app.as_ref().unwrap();
        assert!(app.frame_requested);
        assert_eq!(app.last_frame, now);
        assert_eq!(app.pump_timeout(now), Some(Duration::ZERO));
    }
    let maintained_at = now + Duration::from_secs(60);
    assert_eq!(
        poll_cpu_at(&mut session, maintained_at),
        WinitRunProgress::Running
    );
    assert_eq!(
        counts(&session),
        FrameCounts {
            tasks: before.tasks + 1,
            last: before.last + 1,
            ..before
        }
    );
    let maintained = time_resources(&session);
    assert_eq!(maintained.0.delta, Duration::from_secs(60));
    assert_eq!(
        maintained.0.elapsed,
        before_time.0.elapsed + Duration::from_secs(60)
    );
    assert_eq!(maintained.1.elapsed, before_time.1.elapsed);
    assert_eq!(maintained.2, before_time.2);
    assert_eq!(maintained.3, before_time.3);
    assert!(session.paused_frame_complete());
    assert_eq!(session.capture_time_continuation().unwrap(), phase);
    assert_eq!(session.app.as_ref().unwrap().last_frame, maintained_at);
    assert!(!session.app.as_ref().unwrap().frame_requested);
    let settled = counts(&session);
    assert_eq!(
        poll_cpu_at(&mut session, maintained_at + Duration::from_secs(60)),
        WinitRunProgress::Running
    );
    assert_eq!(counts(&session), settled);
    assert_eq!(time_resources(&session), maintained);
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert_eq!(
        session.into_retired().unwrap().result,
        Ok(AppExit::Requested)
    );
}

#[test]
fn poll_after_a_long_wait_settles_resume_and_keeps_the_next_original_fixed_phase() {
    let mut session = cpu_session(counted_time_app(false), WinitControlFlow::Poll);
    let generation = session.runtime().generation();
    let now = session.app.as_ref().unwrap().last_frame + Duration::from_millis(13);
    assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
    accepted(session.request_control(RuntimeControl::Pause));
    assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
    let phase = session.capture_time_continuation().unwrap();
    assert_eq!(phase.pending(), Duration::from_millis(13));
    let before = time_resources(&session);
    let paused = counts(&session);
    session.set_control_flow(WinitControlFlow::Wait).unwrap();
    let resumed_at = now + Duration::from_secs(60);
    assert_eq!(
        poll_cpu_at(&mut session, resumed_at),
        WinitRunProgress::Running
    );
    assert_eq!(time_resources(&session), before);
    assert_eq!(counts(&session), paused);
    assert_eq!(session.capture_time_continuation().unwrap(), phase);

    for _ in 0..2 {
        session.set_control_flow(WinitControlFlow::Poll).unwrap();
        assert_eq!(session.control_flow(), WinitControlFlow::Poll);
        assert_eq!(session.runtime().generation(), generation);
        assert_eq!(session.runtime().state(), RuntimeState::Paused);
        assert!(!session.paused_frame_complete());
        assert_eq!(
            session.capture_time_continuation(),
            Err(RuntimeTimeContinuationError::Rejected(
                RuntimeTimeContinuationRejection::IncompletePausedFrame
            ))
        );
        assert_eq!(time_resources(&session), before);
        assert_eq!(counts(&session), paused);
        let app = session.app.as_ref().unwrap();
        assert!(app.frame_requested);
        assert_eq!(app.last_frame, now);
        assert_eq!(app.pump_timeout(resumed_at), Some(Duration::ZERO));
    }
    let resume = accepted(session.request_control(RuntimeControl::Resume));
    assert_eq!(
        poll_cpu_at(&mut session, resumed_at),
        WinitRunProgress::Running
    );
    assert_eq!(
        session.control_status(resume),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(session.runtime().state(), RuntimeState::Running);
    let resumed = time_resources(&session);
    assert_eq!(resumed.0.delta, Duration::from_secs(60));
    assert_eq!(
        resumed.0.elapsed,
        before.0.elapsed + Duration::from_secs(60)
    );
    assert_eq!(resumed.0.frame, before.0.frame + 1);
    assert_eq!(resumed.1.delta, Duration::ZERO);
    assert_eq!(resumed.1.elapsed, before.1.elapsed);
    assert_eq!(resumed.1.frame, before.1.frame + 1);
    assert_eq!(resumed.2, before.2);
    assert_eq!(resumed.3, before.3);
    assert_eq!(
        counts(&session),
        FrameCounts {
            tasks: paused.tasks + 1,
            last: paused.last + 1,
            ..paused
        }
    );
    assert_eq!(session.app.as_ref().unwrap().last_frame, resumed_at);
    assert!(!session.app.as_ref().unwrap().frame_requested);

    assert_eq!(
        poll_cpu_at(&mut session, resumed_at + Duration::from_millis(7)),
        WinitRunProgress::Running
    );
    let next = time_resources(&session);
    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(next.0.delta, Duration::from_millis(7));
    assert_eq!(next.1.delta, Duration::from_millis(7));
    assert_eq!(next.1.elapsed, Duration::from_millis(20));
    assert_eq!(next.2.tick(), before.2.tick() + 1);
    assert_eq!(next.2.steps_this_frame(), 1);
    assert_eq!(next.2.remainder(), Duration::ZERO);
    assert_eq!(next.2.debt(), Duration::ZERO);
    assert_eq!(
        counts(&session),
        FrameCounts {
            tasks: paused.tasks + 2,
            fixed: paused.fixed + 1,
            last: paused.last + 2,
        }
    );
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert_eq!(
        session.into_retired().unwrap().result,
        Ok(AppExit::Requested)
    );
}

fn assert_cadence_refused(session: &mut WinitSession) {
    let flow = session.control_flow();
    let before = counts(session);
    let before_time = time_resources(session);
    let state = session.runtime().state();
    let fault = session.runtime().fault().cloned();
    let app = session.app.as_ref().unwrap();
    let original = (
        app.last_frame,
        app.frame_requested,
        app.paused_frame_complete,
        app.presentation_next_wake,
        app.retirement_deadline,
        app.shutdown,
        app.cleanup_incomplete,
        app.failure(),
    );
    for requested in [WinitControlFlow::Poll, WinitControlFlow::Wait] {
        assert!(session.set_control_flow(requested).is_err());
        assert_eq!(session.control_flow(), flow);
        assert_eq!(session.runtime().state(), state);
        assert_eq!(session.runtime().fault(), fault.as_ref());
        assert_eq!(counts(session), before);
        assert_eq!(time_resources(session), before_time);
        let app = session.app.as_ref().unwrap();
        assert_eq!(
            (
                app.last_frame,
                app.frame_requested,
                app.paused_frame_complete,
                app.presentation_next_wake,
                app.retirement_deadline,
                app.shutdown,
                app.cleanup_incomplete,
                app.failure(),
            ),
            original
        );
    }
}

#[test]
fn cadence_is_refused_after_an_original_runtime_fault_before_the_next_poll() {
    for flow in [WinitControlFlow::Poll, WinitControlFlow::Wait] {
        let mut session = cpu_session(counted_time_app(true), flow);
        let generation = session.runtime().generation();
        let now = session.app.as_ref().unwrap().last_frame;
        assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
        assert!(session.paused_frame_complete());
        let before = counts(&session);
        let before_time = time_resources(&session);
        let fault = RuntimeFault::engine(
            RuntimeFaultKind::RequiredService,
            "test.winit-cadence-fault",
        );
        session.runtime().fault_reporter().report(fault.clone());
        assert_eq!(session.runtime().state(), RuntimeState::Faulted);
        assert_eq!(
            session.app.as_ref().unwrap().shutdown,
            WinitShutdownState::Running
        );
        assert_cadence_refused(&mut session);
        assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Retired);
        assert_eq!(session.runtime().generation(), generation);
        assert_eq!(counts(&session), before);
        assert_eq!(time_resources(&session), before_time);
        assert_cadence_refused(&mut session);
        let retired = session.into_retired().unwrap();
        assert_eq!(retired.runtime.generation(), generation);
        assert_eq!(retired.result, Err(runtime_fault_error(&fault)));
    }
}

#[test]
fn cadence_refusal_preserves_original_incomplete_retirement_until_explicit_retry() {
    for flow in [WinitControlFlow::Poll, WinitControlFlow::Wait] {
        let released = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(AtomicUsize::new(0));
        let runtime = pending_runtime(
            counted_time_app(true),
            released.clone(),
            polls.clone(),
            Duration::ZERO,
        );
        let generation = runtime.generation();
        let mut session = WinitSession {
            app: Some(platform_app(runtime)),
            event_loop: None,
            executed: false,
        };
        let _release_on_failure = ReleaseOnDrop(released.clone());
        session.set_control_flow(flow).unwrap();
        let now = session.app.as_ref().unwrap().last_frame;
        assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
        let before = counts(&session);
        let before_time = time_resources(&session);
        session.request_close();
        assert!(session.app.as_ref().unwrap().shutdown.is_started());
        let close_polls = polls.load(Ordering::SeqCst);
        assert_cadence_refused(&mut session);
        assert_eq!(polls.load(Ordering::SeqCst), close_polls);
        assert_eq!(session.poll(Duration::ZERO), WinitRunProgress::Incomplete);
        assert_eq!(session.runtime().state(), RuntimeState::CloseIncomplete);
        let deadline = session.app.as_ref().unwrap().retirement_deadline;
        let failure = session.app.as_ref().unwrap().failure().unwrap();
        released.store(true, Ordering::SeqCst);
        let close_polls = polls.load(Ordering::SeqCst);
        assert_cadence_refused(&mut session);
        assert_eq!(session.poll(Duration::ZERO), WinitRunProgress::Incomplete);
        assert_eq!(polls.load(Ordering::SeqCst), close_polls);
        assert_eq!(session.app.as_ref().unwrap().retirement_deadline, deadline);
        assert_eq!(session.execute(), WinitRunProgress::Retired);
        assert!(polls.load(Ordering::SeqCst) > close_polls);
        assert_eq!(session.runtime().generation(), generation);
        assert_eq!(counts(&session), before);
        assert_eq!(time_resources(&session), before_time);
        assert_cadence_refused(&mut session);
        let retired = session.into_retired().unwrap();
        assert_eq!(retired.runtime.generation(), generation);
        assert_eq!(retired.result, Err(failure));
    }
}
