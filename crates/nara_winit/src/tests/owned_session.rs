use super::*;
use nara_app::{
    Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, PluginShutdownContext,
    RealTime, RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeCloseProgress,
};
use nara_input::{InputLimits, InputTimeline};
use nara_window::backend::{
    WindowSurfaceHandleSource, WindowSurfaceLease, WindowTargetFault, WindowTargetPhase,
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

// These sources test registry ordering, not native pump behavior. The separate Windows tests use
// actual windows, the original EventLoop and actual Destroyed delivery.
#[derive(Debug)]
struct TestWindowSource(Arc<Mutex<Vec<&'static str>>>);

impl Drop for TestWindowSource {
    fn drop(&mut self) {
        self.0.lock().unwrap().push("provider");
    }
}

impl HasWindowHandle for TestWindowSource {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Err(HandleError::NotSupported)
    }
}

impl HasDisplayHandle for TestWindowSource {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Err(HandleError::NotSupported)
    }
}

struct TestSurface {
    source: WindowSurfaceHandleSource,
    lease: WindowSurfaceLease,
    events: Arc<Mutex<Vec<&'static str>>>,
}

#[derive(Default)]
struct PresentationObservations {
    maintained_states: Vec<RuntimeState>,
    retired_states: Vec<RuntimeState>,
    target_batches: Vec<Vec<WindowId>>,
}

// Rc intentionally proves that the concrete session does not require a Send/Sync driver.
struct TestPresentation {
    authority: BackendWindowHandles,
    observations: Rc<RefCell<PresentationObservations>>,
    released: Rc<Cell<bool>>,
    surfaces: BTreeMap<WindowId, TestSurface>,
    exit: Option<AppExit>,
    next_wake: Rc<Cell<Option<Instant>>>,
    maintain_error: Option<AppRunError>,
}

impl TestPresentation {
    fn new(authority: BackendWindowHandles) -> Self {
        Self {
            authority,
            observations: Rc::default(),
            released: Rc::new(Cell::new(true)),
            surfaces: BTreeMap::new(),
            exit: None,
            next_wake: Rc::default(),
            maintain_error: None,
        }
    }

    fn add_surface(&mut self, window_id: WindowId, events: Arc<Mutex<Vec<&'static str>>>) {
        let (source, lease) = self
            .authority
            .acquire_surface(window_id)
            .unwrap()
            .into_parts();
        self.surfaces.insert(
            window_id,
            TestSurface {
                source,
                lease,
                events,
            },
        );
    }
}

impl __WinitPresentationDriver for TestPresentation {
    fn maintain(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        _now: Instant,
    ) -> Result<__WinitPresentationProgress, AppRunError> {
        assert!(handles.shares_authority_with(&self.authority));
        self.observations
            .borrow_mut()
            .maintained_states
            .push(runtime.state());
        if let Some(error) = self.maintain_error.take() {
            return Err(error);
        }
        Ok(__WinitPresentationProgress {
            exit: self.exit.take(),
            next_wake: self.next_wake.get(),
        })
    }

    fn retire(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        window_ids: &[WindowId],
        _now: Instant,
    ) -> Result<bool, AppRunError> {
        assert!(handles.shares_authority_with(&self.authority));
        self.observations
            .borrow_mut()
            .retired_states
            .push(runtime.state());
        self.observations
            .borrow_mut()
            .target_batches
            .push(window_ids.to_vec());
        if !self.released.get() {
            return Ok(false);
        }
        for window_id in window_ids {
            if let Some(TestSurface {
                source,
                lease,
                events,
            }) = self.surfaces.remove(window_id)
            {
                events.lock().unwrap().push("surface");
                drop(source);
                lease.confirm_owner_dropped().unwrap();
            }
        }
        Ok(true)
    }
}

fn register_test_provider(
    handles: &BackendWindowHandles,
    id: WindowId,
) -> Arc<Mutex<Vec<&'static str>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    handles
        .insert(
            id,
            WindowHandleProvider::new(Arc::new(TestWindowSource(events.clone()))),
        )
        .unwrap();
    events
}

struct PendingClose {
    released: Arc<AtomicBool>,
    polls: Arc<AtomicUsize>,
}

impl RuntimeCloseParticipant for PendingClose {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Ok(if self.released.load(Ordering::SeqCst) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

fn pending_runtime(
    app: App,
    released: Arc<AtomicBool>,
    polls: Arc<AtomicUsize>,
    timeout: Duration,
) -> RuntimeInstance {
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.winit-pending"),
            PendingClose { released, polls },
        )
        .unwrap();
    RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            obligations,
            RuntimeClosePolicy::new(timeout),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote()
}

struct ShutdownProbe {
    fail: bool,
}
const SHUTDOWN_ID: PluginId = PluginId::new("test.winit-shutdown");
const SHUTDOWN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(SHUTDOWN_ID, PluginCategory::Backend);

impl Plugin for ShutdownProbe {
    fn declaration() -> &'static PluginDeclaration {
        &SHUTDOWN_DECLARATION
    }
    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        context
            .world_mut()
            .insert_resource(BackendWindowHandles::default());
        if self.fail {
            Err(PluginError::SetupFailed {
                plugin: SHUTDOWN_ID,
                message: "injected shutdown failure".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

#[test]
fn input_connection_refusal_preserves_native_provider_retirement() {
    for (endpoints, pointers) in [(1, 2), (2, 1)] {
        let mut app = window_app();
        app.insert_resource(
            InputTimeline::with_limits(InputLimits::new(16, endpoints, pointers, 1).unwrap())
                .unwrap(),
        )
        .unwrap();
        app.add_plugin(InputPlugin).unwrap();
        let handles = app.world().resource::<BackendWindowHandles>().clone();
        let events = register_test_provider(&handles, WindowId::PRIMARY);
        let mut state = platform_app(start_runtime(app));
        state.owned_window_ids.insert(WindowId::PRIMARY);
        assert!(state.connect_window_input(WindowId::PRIMARY).is_err());
        state.begin_shutdown_transition(Instant::now());
        assert_eq!(*events.lock().unwrap(), vec!["provider"]);
        assert_eq!(
            handles.snapshot(WindowId::PRIMARY).unwrap().phase,
            WindowTargetPhase::ProviderReleased
        );
        handles.mark_native_destroyed(WindowId::PRIMARY).unwrap();
        finish_unit_app(state);
    }
}

#[test]
fn construction_failure_closes_without_a_pump_and_preserves_runtime_and_driver() {
    struct ConstructionDriver(Rc<Cell<usize>>);
    impl __WinitPresentationDriver for ConstructionDriver {
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
    let calls = Rc::new(Cell::new(0));
    let session = WinitRunner::default().__run_with_presentation_on(
        runtime,
        Box::new(ConstructionDriver(calls.clone())),
        Duration::from_secs(1),
        None,
    );
    assert!(session.event_loop.is_none());
    assert!(session.is_retired());
    assert_eq!(session.runtime().generation(), generation);
    assert!(calls.get() >= 1);
    let retired = session.into_retired().ok().unwrap();
    assert!(retired.event_loop.is_none());
    assert_eq!(retired.runtime.state(), RuntimeState::Stopped);
    assert_eq!(
        retired.result,
        Err(AppRunError::runner("backend window authority is missing"))
    );
}

#[test]
fn platform_only_run_rejects_an_explicit_presentation_expectation() {
    let mut app = window_app();
    app.insert_resource(WindowPresentationRequirement::new("test.expected"))
        .unwrap();
    let session = WinitRunner::default().run(start_runtime(app));
    assert!(session.event_loop.is_none());
    let retired = session.into_retired().ok().unwrap();
    assert!(
        retired
            .result
            .unwrap_err()
            .to_string()
            .contains("expected presentation backend")
    );
}

#[test]
fn post_host_exit_begins_shutdown_without_another_simulation_frame() {
    let app = window_app();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let mut driver = TestPresentation::new(handles);
    driver.exit = Some(AppExit::Requested);
    let observations = driver.observations.clone();
    let runtime = start_runtime(app);
    let before = runtime.world().resource::<RealTime>().frame;
    let mut state = WinitApp::new(
        runtime,
        Box::new(driver),
        WinitControlFlow::Wait,
        Duration::from_secs(1),
    );
    state.run_frame_transition(Duration::ZERO);
    assert_eq!(state.exit, AppExit::Requested);
    assert!(state.shutdown.is_started());
    assert_eq!(
        state.runtime.world().resource::<RealTime>().frame,
        before + 1
    );
    assert_eq!(
        observations.borrow().maintained_states.first(),
        Some(&RuntimeState::Running)
    );
    finish_unit_app(state);
}

#[test]
fn maintenance_without_capture_arranges_wait_until_without_simulation() {
    let app = window_app();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let now = Instant::now();
    let wake = now + Duration::from_millis(30);
    let driver = TestPresentation::new(handles);
    driver.next_wake.set(Some(wake));
    let requested_wake = driver.next_wake.clone();
    let mut state = WinitApp::new(
        start_runtime(app),
        Box::new(driver),
        WinitControlFlow::Wait,
        Duration::from_secs(1),
    );
    let frame = state.runtime.world().resource::<RealTime>().frame;
    assert!(!state.maintain_presentation(now));
    assert_eq!(
        state.running_directive(),
        EventLoopDirective::WaitUntil(wake)
    );
    assert_eq!(state.pump_timeout(now), Some(Duration::from_millis(30)));
    assert_eq!(state.runtime.world().resource::<RealTime>().frame, frame);
    requested_wake.set(None);
    assert!(!state.maintain_presentation(now));
    assert_eq!(state.running_directive(), EventLoopDirective::Wait);
    assert_eq!(state.pump_timeout(now), None);
    finish_unit_app(state);
}

#[test]
fn runtime_fault_still_services_presentation_before_shutdown() {
    let app = window_app();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let driver = TestPresentation::new(handles);
    let observations = driver.observations.clone();
    let runtime = start_runtime(app);
    runtime.fault_reporter().report(RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "test.winit-drive-fault",
    ));
    let mut state = WinitApp::new(
        runtime,
        Box::new(driver),
        WinitControlFlow::Poll,
        Duration::from_secs(1),
    );
    state.run_frame_transition(Duration::ZERO);
    assert_eq!(
        observations.borrow().maintained_states.first(),
        Some(&RuntimeState::Faulted)
    );
    assert!(matches!(
        state.primary_failure,
        Some(AppRunError::ManagedRuntime {
            fault_source: "test.winit-drive-fault",
            ..
        })
    ));
    finish_unit_app(state);
}

#[test]
fn physical_retirement_uses_original_authority_after_world_replacement_and_stop() {
    let mut app = window_app();
    app.add_plugin(ShutdownProbe { fail: false }).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let events = register_test_provider(&handles, WindowId::PRIMARY);
    let mut driver = TestPresentation::new(handles.clone());
    driver.add_surface(WindowId::PRIMARY, events.clone());
    driver.released.set(false);
    let released = driver.released.clone();
    let observations = driver.observations.clone();
    let mut state = WinitApp::new(
        start_runtime(app),
        Box::new(driver),
        WinitControlFlow::Wait,
        Duration::from_secs(1),
    );
    state.owned_window_ids.insert(WindowId::PRIMARY);
    state.begin_shutdown_transition(Instant::now());
    state.poll_native_shutdown_once(Instant::now());
    assert_eq!(state.runtime.state(), RuntimeState::Stopped);
    assert!(
        !handles.shares_authority_with(state.runtime.world().resource::<BackendWindowHandles>())
    );
    assert!(events.lock().unwrap().is_empty());
    assert!(handles.snapshot(WindowId::PRIMARY).unwrap().surface_active);
    assert_eq!(state.shutdown, WinitShutdownState::Retiring);
    released.set(true);
    state.poll_native_shutdown_once(Instant::now());
    assert_eq!(*events.lock().unwrap(), vec!["surface", "provider"]);
    assert!(
        observations
            .borrow()
            .retired_states
            .contains(&RuntimeState::Stopped)
    );
    assert!(
        observations
            .borrow()
            .maintained_states
            .contains(&RuntimeState::Stopped)
    );
    handles.mark_native_destroyed(WindowId::PRIMARY).unwrap();
    finish_unit_app(state);
}

#[test]
fn destroyed_while_retiring_before_surface_release_remains_a_failure() {
    let app = window_app();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let events = register_test_provider(&handles, WindowId::PRIMARY);
    let mut driver = TestPresentation::new(handles.clone());
    driver.add_surface(WindowId::PRIMARY, events.clone());
    driver.released.set(false);
    let released = driver.released.clone();
    let mut state = WinitApp::new(
        start_runtime(app),
        Box::new(driver),
        WinitControlFlow::Wait,
        Duration::from_secs(1),
    );
    let native_id = WinitWindowId::dummy();
    state.owned_window_ids.insert(WindowId::PRIMARY);
    state
        .nara_windows_by_winit
        .insert(native_id, WindowId::PRIMARY);
    state.begin_shutdown_transition(Instant::now());
    assert_eq!(state.shutdown, WinitShutdownState::Retiring);
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        WindowTargetPhase::RetireRequested
    );

    state
        .handle_destroyed_window(native_id, Instant::now())
        .unwrap();
    state.poll_native_shutdown_once(Instant::now());
    assert_eq!(state.runtime.state(), RuntimeState::Stopped);
    assert_eq!(state.shutdown, WinitShutdownState::Retiring);
    assert!(handles.snapshot(WindowId::PRIMARY).unwrap().surface_active);
    assert!(events.lock().unwrap().is_empty());
    let expected = AppRunError::runner("native window was destroyed before controlled retirement");
    assert_eq!(state.failure(), Some(expected.clone()));
    assert_eq!(state.native_retirement_failure, Some(expected.clone()));

    released.set(true);
    assert_eq!(
        state.poll_native_shutdown_once(Instant::now()),
        EventLoopDirective::Exit
    );
    assert_eq!(state.shutdown, WinitShutdownState::Complete);
    let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
    assert_eq!(snapshot.phase, WindowTargetPhase::NativeDestroyed);
    assert_eq!(snapshot.fault, Some(WindowTargetFault::ExternallyDestroyed));
    assert_eq!(*events.lock().unwrap(), vec!["surface", "provider"]);
    let session = WinitSession {
        app: Some(state),
        event_loop: None,
        executed: true,
    };
    assert_eq!(session.into_retired().unwrap().result, Err(expected));
}

#[test]
fn session_does_not_release_foreign_registered_targets() {
    let app = window_app();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let own_events = register_test_provider(&handles, WindowId::PRIMARY);
    let foreign = WindowId::new(2);
    let foreign_events = register_test_provider(&handles, foreign);
    let mut state = platform_app(start_runtime(app));
    state.owned_window_ids.insert(WindowId::PRIMARY);
    state.begin_shutdown_transition(Instant::now());
    assert_eq!(*own_events.lock().unwrap(), vec!["provider"]);
    assert!(foreign_events.lock().unwrap().is_empty());
    assert_eq!(
        handles.snapshot(foreign).unwrap().phase,
        WindowTargetPhase::Active
    );
    handles.mark_native_destroyed(WindowId::PRIMARY).unwrap();
    finish_unit_app(state);
    handles.request_retirement(foreign).unwrap();
    handles.release_provider(foreign).unwrap();
}

#[test]
fn externally_destroyed_is_sticky_and_duplicate_destroyed_is_ignored() {
    let app = window_app();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let events = register_test_provider(&handles, WindowId::PRIMARY);
    let mut state = platform_app(start_runtime(app));
    let native_id = WinitWindowId::dummy();
    state.owned_window_ids.insert(WindowId::PRIMARY);
    state
        .nara_windows_by_winit
        .insert(native_id, WindowId::PRIMARY);
    state
        .handle_destroyed_window(native_id, Instant::now())
        .unwrap();
    assert!(
        state
            .failure()
            .unwrap()
            .to_string()
            .contains("destroyed before controlled retirement")
    );
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().fault,
        Some(WindowTargetFault::ExternallyDestroyed)
    );
    assert_eq!(*events.lock().unwrap(), vec!["provider"]);
    assert_eq!(
        state
            .handle_destroyed_window(native_id, Instant::now())
            .unwrap(),
        EventLoopDirective::None
    );
    assert_eq!(
        state.runtime.world().resource::<WindowEvents>().as_slice(),
        &[WindowEvent::Closed {
            window_id: WindowId::PRIMARY
        }]
    );
    finish_unit_app(state);
}

#[test]
fn native_budget_is_independent_of_the_original_runtime_close_policy() {
    let released = Arc::new(AtomicBool::new(false));
    let polls = Arc::new(AtomicUsize::new(0));
    let runtime = pending_runtime(
        window_app(),
        released.clone(),
        polls,
        Duration::from_secs(60),
    );
    let mut state = platform_app(runtime);
    state.native_retirement_budget = Duration::ZERO;
    let mut session = WinitSession {
        app: Some(state),
        event_loop: None,
        executed: false,
    };
    assert_eq!(session.execute(), WinitRunProgress::Incomplete);
    assert_eq!(session.runtime().state(), RuntimeState::Stopping);
    let generation = session.runtime().generation();
    let original = session.app.as_ref().unwrap().failure();
    session = session.into_retired().err().unwrap();
    assert_eq!(session.runtime().generation(), generation);
    released.store(true, Ordering::SeqCst);
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    let retired = session.into_retired().ok().unwrap();
    assert_eq!(retired.runtime.state(), RuntimeState::Stopped);
    assert_eq!(retired.result, Err(original.unwrap()));
}

#[test]
fn close_incomplete_requires_retry_close_and_retains_diagnostics() {
    let released = Arc::new(AtomicBool::new(false));
    let polls = Arc::new(AtomicUsize::new(0));
    let runtime = pending_runtime(
        window_app(),
        released.clone(),
        polls.clone(),
        Duration::ZERO,
    );
    let mut state = platform_app(runtime);
    state.begin_shutdown_transition(Instant::now());
    state.poll_native_shutdown_once(Instant::now());
    assert_eq!(state.runtime.state(), RuntimeState::CloseIncomplete);
    let before = polls.load(Ordering::SeqCst);
    state.poll_native_shutdown_once(Instant::now());
    assert_eq!(polls.load(Ordering::SeqCst), before);
    let original = state.failure();
    released.store(true, Ordering::SeqCst);
    let mut session = WinitSession {
        app: Some(state),
        event_loop: None,
        executed: true,
    };
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert!(polls.load(Ordering::SeqCst) > before);
    let retired = session.into_retired().ok().unwrap();
    assert_eq!(retired.runtime.state(), RuntimeState::Stopped);
    assert_eq!(retired.result, Err(original.unwrap()));
}

#[test]
fn failure_observation_does_not_consume_primary_runtime_or_native_errors() {
    let mut app = window_app();
    app.add_plugin(ShutdownProbe { fail: true }).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let mut driver = TestPresentation::new(handles);
    let presentation = AppRunError::runner("presentation maintenance failed");
    driver.maintain_error = Some(presentation.clone());
    let mut state = WinitApp::new(
        start_runtime(app),
        Box::new(driver),
        WinitControlFlow::Poll,
        Duration::from_secs(1),
    );
    let primary = AppRunError::runner("original pump failure");
    let native = AppRunError::runner("native deadline expired");
    state.record_primary_failure(primary.clone());
    state.record_native_retirement_failure(native.clone());
    state.begin_shutdown_transition(Instant::now());
    state.poll_native_shutdown_once(Instant::now());
    let expected = AppRunError::runner_teardown(
        AppRunError::runner_teardown(
            AppRunError::runner_teardown(primary, presentation),
            AppRunError::runner("managed runtime close failed"),
        ),
        native,
    );
    assert_eq!(state.failure(), Some(expected.clone()));
    assert_eq!(state.failure(), Some(expected));
    finish_unit_app(state);
}

#[test]
fn completed_close_error_is_terminal_without_leaving_a_live_runtime() {
    struct CompletedCloseError;
    impl RuntimeCloseParticipant for CompletedCloseError {
        fn begin_close(
            &mut self,
            _: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            Ok(RuntimeCloseProgress::CompleteWithError {
                code: "test.winit-completed-close-error",
            })
        }
        fn poll_close(
            &mut self,
            _: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            panic!("a completed participant must not be polled again")
        }
    }
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("test.winit-completed"),
            CompletedCloseError,
        )
        .unwrap();
    let runtime = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            window_app().seal().unwrap(),
            obligations,
            RuntimeClosePolicy::default(),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote();
    let mut state = platform_app(runtime);
    state.begin_shutdown_transition(Instant::now());
    state.poll_native_shutdown_once(Instant::now());
    assert_eq!(state.runtime.state(), RuntimeState::Stopped);
    assert!(state.runtime.close_evidence().has_terminal_failure());
    assert_eq!(
        state.failure(),
        Some(AppRunError::runner("managed runtime close failed"))
    );
    finish_unit_app(state);
}

#[test]
fn retiring_session_ignores_close_input_and_accepts_destroyed() {
    for state in [WinitShutdownState::Retiring, WinitShutdownState::Complete] {
        assert!(!should_process_window_event_during_shutdown(
            state,
            &WinitWindowEvent::CloseRequested
        ));
        assert!(should_process_window_event_during_shutdown(
            state,
            &WinitWindowEvent::Destroyed
        ));
    }
    assert!(should_process_window_event_during_shutdown(
        WinitShutdownState::Running,
        &WinitWindowEvent::CloseRequested
    ));
}
