use super::*;
use nara_app::{CoreStage, RuntimeControlFailure, RuntimeTimeSettings};
use nara_ecs::{ResMut, Resource};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
struct FrameCounts {
    tasks: u64,
    fixed: u64,
    last: u64,
}

fn counted_app(paused: bool) -> App {
    let mut app = window_app();
    app.insert_resource(RuntimeTimeSettings::default().with_paused(paused))
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
    app
}

fn cpu_session(app: App, control_flow: WinitControlFlow) -> WinitSession {
    WinitSession {
        app: Some(WinitApp::new(
            start_runtime(app),
            Box::new(NoPresentationDriver),
            control_flow,
            DEFAULT_NATIVE_RETIREMENT_BUDGET,
        )),
        event_loop: None,
        executed: false,
    }
}

fn poll_cpu(session: &mut WinitSession) -> WinitRunProgress {
    session.poll_with(|app, event_loop, _first_native_pump| {
        assert!(event_loop.is_none());
        app.service_boundary_transition(Instant::now());
        false
    })
}

fn counts(session: &WinitSession) -> FrameCounts {
    *session.runtime().world().resource::<FrameCounts>()
}

fn accepted(result: RuntimeControlRequestResult) -> RuntimeControlTicket {
    match result {
        RuntimeControlRequestResult::Accepted(ticket) => ticket,
        rejected => panic!("expected accepted control, got {rejected:?}"),
    }
}

struct ReleaseOnDrop(Arc<AtomicBool>);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[test]
fn repeated_poll_boundaries_keep_the_original_live_runtime_and_driver() {
    for control_flow in [WinitControlFlow::Poll, WinitControlFlow::Wait] {
        let app = counted_app(false);
        let handles = app.world().resource::<BackendWindowHandles>().clone();
        let driver = TestPresentation::new(handles);
        let observations = driver.observations.clone();
        let mut session = cpu_session(app, control_flow);
        session.app.as_mut().unwrap().presentation = Box::new(driver);
        let generation = session.runtime().generation();
        for iteration in 0..5 {
            assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
            assert_eq!(session.runtime().generation(), generation);
            assert_eq!(session.runtime().state(), RuntimeState::Running);
            assert!(!session.is_retired());
            assert!(!session.paused_frame_complete());
            assert_eq!(observations.borrow().maintained_states.len(), iteration + 1);
            assert!(observations.borrow().retired_states.is_empty());
            assert_eq!(session.app.as_ref().unwrap().retirement_deadline, None);
        }
        assert_eq!(
            counts(&session).last,
            if control_flow == WinitControlFlow::Poll {
                5
            } else {
                1
            }
        );
        session.request_close();
        assert_eq!(session.execute(), WinitRunProgress::Retired);
        assert_eq!(
            session.into_retired().unwrap().result,
            Ok(AppExit::Requested)
        );
    }
}

#[test]
fn unexecuted_start_retains_admission_failure_until_the_first_poll() {
    let calls = Rc::new(Cell::new(0));
    struct StartDriver(Rc<Cell<usize>>);
    impl __WinitPresentationDriver for StartDriver {
        fn maintain(
            &mut self,
            _: &mut RuntimeInstance,
            _: &BackendWindowHandles,
            _: Instant,
        ) -> Result<__WinitPresentationProgress, AppRunError> {
            self.0.set(self.0.get() + 1);
            Ok(__WinitPresentationProgress::default())
        }

        fn retire(
            &mut self,
            _: &mut RuntimeInstance,
            _: &BackendWindowHandles,
            ids: &[WindowId],
            _: Instant,
        ) -> Result<bool, AppRunError> {
            assert!(ids.is_empty());
            Ok(true)
        }
    }
    let runtime = start_runtime(App::new());
    let generation = runtime.generation();
    let mut session = WinitRunner::default()
        .with_initial_visibility(false)
        .__start_with_presentation_on(
            runtime,
            Box::new(StartDriver(calls.clone())),
            Duration::from_secs(1),
            None,
        );
    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(session.runtime().state(), RuntimeState::Running);
    assert_eq!(calls.get(), 0);
    assert!(!session.executed);
    assert!(session.event_loop.is_none());
    assert!(!session.app.as_ref().unwrap().initial_visibility);
    assert!(session.reveal().is_err());
    assert_eq!(session.poll(Duration::ZERO), WinitRunProgress::Retired);
    assert!(calls.get() > 0);
    assert_eq!(session.poll(Duration::ZERO), WinitRunProgress::Retired);
    let retired = session.into_retired().unwrap();
    assert_eq!(retired.runtime.generation(), generation);
    assert_eq!(
        retired.result,
        Err(AppRunError::runner("backend window authority is missing"))
    );
}

#[test]
fn pause_and_exact_step_require_distinct_full_paused_frames_and_wake_wait() {
    let mut session = cpu_session(counted_app(false), WinitControlFlow::Wait);
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    let before_pause = counts(&session);
    assert_eq!(
        session.app.as_ref().unwrap().pump_timeout(Instant::now()),
        None
    );
    let pause = accepted(session.request_control(RuntimeControl::Pause));
    assert_eq!(
        session.control_status(pause),
        Some(RuntimeControlStatus::Pending)
    );
    assert!(!session.paused_frame_complete());
    assert_eq!(
        session.app.as_ref().unwrap().pump_timeout(Instant::now()),
        Some(Duration::ZERO)
    );
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(pause),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(session.runtime().state(), RuntimeState::Paused);
    assert!(session.paused_frame_complete());
    let paused = counts(&session);
    assert_eq!(paused.tasks, before_pause.tasks + 1);
    assert_eq!(paused.last, before_pause.last + 1);
    assert_eq!(paused.fixed, before_pause.fixed);
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(counts(&session), paused);
    assert!(session.paused_frame_complete());
    assert!(matches!(
        session.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Rejected(_)
    ));
    assert!(session.paused_frame_complete());
    let step = accepted(session.request_control(RuntimeControl::StepFixedTick));
    assert!(!session.paused_frame_complete());
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(
        counts(&session),
        FrameCounts {
            fixed: paused.fixed + 1,
            ..paused
        }
    );
    assert!(!session.paused_frame_complete());
    assert_eq!(
        session.app.as_ref().unwrap().running_directive(),
        EventLoopDirective::Poll
    );
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(
        counts(&session),
        FrameCounts {
            tasks: paused.tasks + 1,
            fixed: paused.fixed + 1,
            last: paused.last + 1,
        }
    );
    assert!(session.paused_frame_complete());
    let resume = accepted(session.request_control(RuntimeControl::Resume));
    assert!(!session.paused_frame_complete());
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(resume),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(session.runtime().state(), RuntimeState::Running);
    assert!(!session.paused_frame_complete());
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
}

#[test]
fn initially_paused_exact_step_cannot_authorize_capture_before_maintenance() {
    let mut session = cpu_session(counted_app(true), WinitControlFlow::Wait);
    assert_eq!(session.runtime().state(), RuntimeState::Paused);
    let step = accepted(session.request_control(RuntimeControl::StepFixedTick));
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(
        counts(&session),
        FrameCounts {
            fixed: 1,
            ..FrameCounts::default()
        }
    );
    assert!(!session.paused_frame_complete());
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(
        counts(&session),
        FrameCounts {
            tasks: 1,
            fixed: 1,
            last: 1,
        }
    );
    assert!(session.paused_frame_complete());
    session.request_close();
    assert!(!session.paused_frame_complete());
    assert_eq!(session.execute(), WinitRunProgress::Retired);
}

struct ScopedMaintenance(Rc<Cell<usize>>);

impl __WinitPresentationDriver for ScopedMaintenance {
    fn maintain(
        &mut self,
        runtime: &mut RuntimeInstance,
        _: &BackendWindowHandles,
        _: Instant,
    ) -> Result<__WinitPresentationProgress, AppRunError> {
        if matches!(
            runtime.state(),
            RuntimeState::Running | RuntimeState::Paused
        ) {
            let outcome = runtime
                .with_driver_scope(|_| Box::new(7))
                .map_err(runtime_scope_error)?;
            assert_eq!(*outcome.output, 7);
            if let Some(fault) = outcome.post_operation_fault {
                return Err(runtime_fault_error(&fault));
            }
            self.0.set(self.0.get() + 1);
        }
        Ok(__WinitPresentationProgress::default())
    }

    fn retire(
        &mut self,
        _: &mut RuntimeInstance,
        _: &BackendWindowHandles,
        _: &[WindowId],
        _: Instant,
    ) -> Result<bool, AppRunError> {
        Ok(true)
    }
}

#[test]
fn presentation_scopes_preserve_readiness_and_external_scopes_request_a_real_frame() {
    let mut session = cpu_session(counted_app(true), WinitControlFlow::Wait);
    let reconciled = Rc::new(Cell::new(0));
    session.app.as_mut().unwrap().presentation = Box::new(ScopedMaintenance(reconciled.clone()));
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert!(session.paused_frame_complete());
    let paused = counts(&session);
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(counts(&session), paused);
    assert_eq!(reconciled.get(), 2);
    assert!(session.paused_frame_complete());
    let output = session
        .__with_driver_scope(|_| Box::new("original output".to_owned()))
        .unwrap();
    assert_eq!(*output.output, "original output");
    assert_eq!(output.post_operation_fault, None);
    assert!(!session.paused_frame_complete());
    assert_eq!(
        session.app.as_ref().unwrap().pump_timeout(Instant::now()),
        Some(Duration::ZERO)
    );
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(counts(&session).last, paused.last + 1);
    assert!(session.paused_frame_complete());
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
}

#[test]
fn native_driver_input_invalidates_capture_until_the_original_paused_frame() {
    let mut app = counted_app(true);
    app.add_plugin(InputPlugin).unwrap();
    let mut session = cpu_session(app, WinitControlFlow::Wait);
    session
        .app
        .as_mut()
        .unwrap()
        .connect_window_input(WindowId::PRIMARY)
        .unwrap();
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert!(session.paused_frame_complete());
    assert!(
        session
            .app
            .as_mut()
            .unwrap()
            .apply_physical_keyboard_driver_event(
                RawKeyEvent {
                    physical_key: PhysicalKey::Code(WinitKeyCode::KeyA),
                    state: ElementState::Pressed,
                },
                false,
            )
            .unwrap()
    );
    assert!(!session.paused_frame_complete());
    let before = counts(&session);
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(counts(&session).last, before.last + 1);
    assert!(session.paused_frame_complete());
    session
        .app
        .as_mut()
        .unwrap()
        .apply_focus_driver_event(WindowId::PRIMARY, false)
        .unwrap();
    assert!(!session.paused_frame_complete());
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert!(session.paused_frame_complete());
    session.request_close();
    assert!(
        !session
            .app
            .as_mut()
            .unwrap()
            .apply_physical_keyboard_driver_event(
                RawKeyEvent {
                    physical_key: PhysicalKey::Code(WinitKeyCode::KeyA),
                    state: ElementState::Released,
                },
                false,
            )
            .unwrap()
    );
    assert_eq!(session.execute(), WinitRunProgress::Retired);
}

#[test]
fn input_after_the_full_frame_in_the_same_pump_keeps_capture_invalid() {
    let mut app = counted_app(true);
    app.add_plugin(InputPlugin).unwrap();
    let mut session = cpu_session(app, WinitControlFlow::Wait);
    session
        .app
        .as_mut()
        .unwrap()
        .connect_window_input(WindowId::PRIMARY)
        .unwrap();
    let progress = session.poll_with(|app, event_loop, first_native_pump| {
        assert!(event_loop.is_none());
        assert!(first_native_pump);
        app.service_boundary_transition(Instant::now());
        assert!(app.paused_frame_complete);
        app.apply_focus_driver_event(WindowId::PRIMARY, false)
            .unwrap();
        assert!(!app.paused_frame_complete);
        false
    });
    assert_eq!(progress, WinitRunProgress::Running);
    assert_eq!(counts(&session).last, 1);
    assert!(!session.paused_frame_complete());
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(counts(&session).last, 2);
    assert!(session.paused_frame_complete());
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
}

#[cfg(feature = "platform-accessibility")]
#[test]
fn accessibility_update_reconciliation_does_not_permanently_block_capture() {
    let mut app = counted_app(true);
    accessibility::__private::install_winit_accessibility_contribution(&mut app).unwrap();
    let mut session = cpu_session(app, WinitControlFlow::Wait);
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert!(session.paused_frame_complete());
    let before = counts(&session);
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
    assert_eq!(counts(&session), before);
    assert!(session.paused_frame_complete());
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
}

#[test]
fn driver_scope_retains_owned_success_or_rejection_and_the_original_post_fault() {
    for (reject, immediately_close) in [(false, false), (true, false), (false, true), (true, true)]
    {
        let mut session = cpu_session(counted_app(true), WinitControlFlow::Wait);
        assert_eq!(poll_cpu(&mut session), WinitRunProgress::Running);
        assert!(session.paused_frame_complete());
        let before = counts(&session);
        let fault =
            RuntimeFault::engine(RuntimeFaultKind::RequiredService, "test.winit-scope-output");
        let reporter = session.runtime().fault_reporter();
        let owned = Box::new("original owned result".to_owned());
        let original = std::ptr::from_ref(&*owned);
        let outcome = session
            .__with_driver_scope(|_| {
                reporter.report(fault.clone());
                if reject { Err(owned) } else { Ok(owned) }
            })
            .unwrap();
        let returned = match outcome.output {
            Ok(output) => {
                assert!(!reject);
                output
            }
            Err(output) => {
                assert!(reject);
                output
            }
        };
        assert_eq!(std::ptr::from_ref(&*returned), original);
        assert_eq!(outcome.post_operation_fault, Some(fault.clone()));
        assert_eq!(session.runtime().fault(), Some(&fault));
        assert!(!session.paused_frame_complete());
        assert!(matches!(
            session.request_control(RuntimeControl::Resume),
            RuntimeControlRequestResult::Rejected(_)
        ));
        if immediately_close {
            session.request_close();
        }
        assert_eq!(poll_cpu(&mut session), WinitRunProgress::Retired);
        assert_eq!(counts(&session), before);
        assert_eq!(
            session.into_retired().unwrap().result,
            Err(runtime_fault_error(&fault))
        );
    }
}

#[test]
fn accepted_stop_supersedes_pause_and_fences_driver_input_before_the_next_poll() {
    let mut session = cpu_session(counted_app(false), WinitControlFlow::Wait);
    let pause = accepted(session.request_control(RuntimeControl::Pause));
    let stop = accepted(session.request_control(RuntimeControl::Stop));
    assert_eq!(
        session.control_status(pause),
        Some(RuntimeControlStatus::Failed(
            RuntimeControlFailure::SupersededByStop
        ))
    );
    assert!(session.app.as_ref().unwrap().shutdown.is_started());
    assert!(matches!(
        session.__with_driver_scope(|_| panic!("closed input must not run")),
        Err(RuntimeScopeError::Unavailable { .. })
    ));
    assert!(matches!(
        session.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Rejected(_)
    ));
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert_eq!(
        session.control_status(stop),
        Some(RuntimeControlStatus::Applied)
    );
    let before = counts(&session);
    session.request_close();
    assert_eq!(session.poll(Duration::ZERO), WinitRunProgress::Retired);
    assert_eq!(counts(&session), before);
    assert_eq!(
        session.into_retired().unwrap().result,
        Ok(AppExit::Requested)
    );
}

#[test]
fn incomplete_runtime_close_stays_with_the_original_owner_until_explicit_retry() {
    let released = Arc::new(AtomicBool::new(false));
    let polls = Arc::new(AtomicUsize::new(0));
    let runtime = pending_runtime(
        window_app(),
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
    session.request_close();
    assert_eq!(session.poll(Duration::ZERO), WinitRunProgress::Incomplete);
    assert_eq!(session.runtime().state(), RuntimeState::CloseIncomplete);
    let before = polls.load(Ordering::SeqCst);
    let deadline = session.app.as_ref().unwrap().retirement_deadline;
    let original = session.app.as_ref().unwrap().failure();
    released.store(true, Ordering::SeqCst);
    for _ in 0..3 {
        assert_eq!(session.poll(Duration::ZERO), WinitRunProgress::Incomplete);
        assert_eq!(polls.load(Ordering::SeqCst), before);
        assert_eq!(session.app.as_ref().unwrap().retirement_deadline, deadline);
    }
    session.request_close();
    assert_eq!(session.app.as_ref().unwrap().retirement_deadline, deadline);
    session = session.into_retired().unwrap_err();
    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert!(polls.load(Ordering::SeqCst) > before);
    let retired = session.into_retired().unwrap();
    assert_eq!(retired.runtime.generation(), generation);
    assert_eq!(retired.result, Err(original.unwrap()));
}

#[test]
fn bounded_presentation_cleanup_is_latched_without_renewing_its_budget() {
    let app = window_app();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let driver = TestPresentation::new(handles);
    driver.released.set(false);
    let released = driver.released.clone();
    let observations = driver.observations.clone();
    let mut session = cpu_session(app, WinitControlFlow::Wait);
    {
        let app = session.app.as_mut().unwrap();
        app.presentation = Box::new(driver);
        app.native_retirement_budget = Duration::ZERO;
    }
    session.request_close();
    let progress = session.poll(Duration::ZERO);
    released.set(true);
    assert_eq!(progress, WinitRunProgress::Incomplete);
    assert_eq!(session.runtime().state(), RuntimeState::Stopped);
    let deadline = session.app.as_ref().unwrap().retirement_deadline;
    let retired_calls = observations.borrow().retired_states.len();
    assert_eq!(session.poll(Duration::ZERO), WinitRunProgress::Incomplete);
    assert_eq!(session.app.as_ref().unwrap().retirement_deadline, deadline);
    assert_eq!(observations.borrow().retired_states.len(), retired_calls);
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert!(observations.borrow().retired_states.len() > retired_calls);
    assert_eq!(
        session.into_retired().unwrap().result,
        Err(AppRunError::runner(
            "native session retirement budget expired"
        ))
    );
}

#[test]
fn paused_frame_with_a_presentation_exit_never_authorizes_capture() {
    let app = counted_app(true);
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let mut driver = TestPresentation::new(handles);
    driver.exit = Some(AppExit::Requested);
    let mut session = cpu_session(app, WinitControlFlow::Wait);
    session.app.as_mut().unwrap().presentation = Box::new(driver);
    assert_eq!(poll_cpu(&mut session), WinitRunProgress::Retiring);
    assert!(!session.paused_frame_complete());
    assert_eq!(counts(&session).last, 1);
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert_eq!(
        session.into_retired().unwrap().result,
        Ok(AppExit::Requested)
    );
}

#[test]
fn pump_wait_caps_preserve_immediate_work_and_earlier_owner_deadlines() {
    let cap = Duration::from_millis(5);
    assert_eq!(bounded_pump_timeout(None, Some(cap)), Some(cap));
    assert_eq!(
        bounded_pump_timeout(Some(Duration::from_secs(1)), Some(cap)),
        Some(cap)
    );
    assert_eq!(
        bounded_pump_timeout(Some(Duration::from_millis(1)), Some(cap)),
        Some(Duration::from_millis(1))
    );
    assert_eq!(
        bounded_pump_timeout(Some(Duration::ZERO), Some(cap)),
        Some(Duration::ZERO)
    );
    assert_eq!(bounded_pump_timeout(None, None), None);
    assert_eq!(bounded_pump_timeout(Some(cap), None), Some(cap));
}
