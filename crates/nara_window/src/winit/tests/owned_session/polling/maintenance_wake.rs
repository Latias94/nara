use super::*;
use crate::winit::maintenance_wake::MaintenanceWakeState;

fn install_wake_state(session: &mut WinitSession, id: SessionWakeId) -> Arc<MaintenanceWakeState> {
    let state = Arc::new(MaintenanceWakeState::default());
    let app = session.app.as_mut().unwrap();
    app.session_wake_id = Some(id);
    app.maintenance_wake = Some(state.clone());
    state
}

#[test]
fn maintenance_hint_preserves_paused_capture_clocks_and_original_driver_service() {
    let app = counted_time_app(false);
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let driver = TestPresentation::new(handles);
    let observations = driver.observations.clone();
    let next_wake = driver.next_wake.clone();
    let mut session = cpu_session(app, WinitControlFlow::Wait);
    session.app.as_mut().unwrap().presentation = Box::new(driver);
    let id = SessionWakeId(1);
    let state = install_wake_state(&mut session, id);
    let generation = session.runtime().generation();
    let now = session.app.as_ref().unwrap().last_frame + Duration::from_millis(13);
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
    let clocks = time_resources(&session);
    let maintained = observations.borrow().maintained_states.len();

    for seconds in 1..=3 {
        assert!(state.notify(|| true));
        assert!(state.notify(|| panic!("coalesced hint sent another event")));
        let app = session.app.as_mut().unwrap();
        assert!(app.handle_maintenance_wake(id));
        assert!(!app.handle_maintenance_wake(id));
        assert!(!app.frame_requested);
        assert_eq!(app.last_frame, now);
        assert_eq!(counts(&session), paused);
        assert_eq!(time_resources(&session), clocks);
        assert!(session.paused_frame_complete());
        assert_eq!(session.capture_time_continuation().unwrap(), phase);
        let at = now + Duration::from_secs(seconds);
        let deadline = at + Duration::from_secs(5);
        next_wake.set(Some(deadline));
        assert_eq!(poll_cpu_at(&mut session, at), WinitRunProgress::Running);
        assert_eq!(session.runtime().generation(), generation);
        assert_eq!(counts(&session), paused);
        assert_eq!(time_resources(&session), clocks);
        assert!(session.paused_frame_complete());
        assert_eq!(session.capture_time_continuation().unwrap(), phase);
        let app = session.app.as_ref().unwrap();
        assert_eq!(app.last_frame, now);
        assert_eq!(
            app.running_directive(),
            EventLoopDirective::WaitUntil(deadline)
        );
    }
    assert_eq!(
        observations.borrow().maintained_states.len(),
        maintained + 3
    );
    assert!(observations.borrow().retired_states.is_empty());
    assert!(session.maintenance_wake().is_none());

    let step = accepted(session.request_control(RuntimeControl::StepFixedTick));
    assert!(state.notify(|| true));
    assert!(session.app.as_mut().unwrap().handle_maintenance_wake(id));
    assert!(session.app.as_ref().unwrap().frame_requested);
    assert!(!session.paused_frame_complete());
    assert_eq!(
        session.control_status(step),
        Some(RuntimeControlStatus::Pending)
    );
    assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(counts(&session).fixed, paused.fixed + 1);
    assert_eq!(poll_cpu_at(&mut session, now), WinitRunProgress::Running);
    assert!(session.paused_frame_complete());
    session.request_close();
    assert!(!state.notify(|| true));
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert_eq!(
        session.into_retired().unwrap().result,
        Ok(AppExit::Requested)
    );
}

#[test]
fn stale_or_unbound_maintenance_wakes_cannot_acknowledge_current_pending_work() {
    let mut session = cpu_session(counted_time_app(true), WinitControlFlow::Wait);
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    let id = SessionWakeId(8);
    let state = install_wake_state(&mut session, id);
    let before = counts(&session);
    let clocks = time_resources(&session);
    let phase = session.capture_time_continuation().unwrap();
    assert!(state.notify(|| true));
    let app = session.app.as_mut().unwrap();
    assert!(!app.handle_maintenance_wake(SessionWakeId(7)));
    assert!(!app.frame_requested);
    assert!(state.notify(|| panic!("stale route cleared the current pending hint")));
    assert!(app.handle_maintenance_wake(id));
    assert!(!app.handle_maintenance_wake(id));
    app.session_wake_id = None;
    assert!(state.notify(|| true));
    assert!(!app.handle_maintenance_wake(id));
    assert_eq!(counts(&session), before);
    assert_eq!(time_resources(&session), clocks);
    assert_eq!(session.capture_time_continuation().unwrap(), phase);
    assert!(session.maintenance_wake().is_none());
    session.request_close();
    assert!(!state.acknowledge());
    assert!(!state.notify(|| true));
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert_eq!(
        session.into_retired().unwrap().result,
        Ok(AppExit::Requested)
    );
}

#[test]
fn original_close_failure_and_drop_revoke_pending_wake_state() {
    for failure in [false, true] {
        let mut session = cpu_session(counted_time_app(true), WinitControlFlow::Wait);
        let state = install_wake_state(&mut session, SessionWakeId(1));
        assert!(state.notify(|| true));
        if failure {
            session
                .app
                .as_mut()
                .unwrap()
                .record_primary_failure(AppRunError::runner("native startup failed"));
        } else {
            session.request_close();
        }
        assert!(!state.is_live());
        assert!(!state.acknowledge());
        assert!(!state.notify(|| true));
        assert!(session.maintenance_wake().is_none());
        assert!(
            !session
                .app
                .as_mut()
                .unwrap()
                .handle_maintenance_wake(SessionWakeId(1))
        );
        session.request_close();
        assert_eq!(session.execute(), WinitRunProgress::Retired);
        let retired = session.into_retired().unwrap();
        assert_eq!(retired.runtime.state(), RuntimeState::Stopped);
        assert_eq!(retired.result.is_err(), failure);
    }
    let mut session = cpu_session(counted_time_app(true), WinitControlFlow::Wait);
    let state = install_wake_state(&mut session, SessionWakeId(1));
    assert!(state.notify(|| true));
    drop(session);
    assert!(!state.is_live());
    assert!(!state.acknowledge());
    assert!(!state.notify(|| true));
}
