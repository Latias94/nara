use super::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, PluginShutdownContext,
    RuntimeCandidateRetirementState, RuntimeCloseCause, RuntimeCloseContext,
    RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
    RuntimeClosePolicy, RuntimeCloseProgress, RuntimeDriverScope, RuntimeFaultKind,
    RuntimeObligationLedger,
};
use nara_window::{
    WindowEvents, WindowPlugin,
    backend::{WindowSurfaceHandleSource, WindowSurfaceLease, WindowSurfaceRetirementError},
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

#[derive(Debug)]
struct TestWindowSource {
    events: Arc<Mutex<Vec<&'static str>>>,
}

#[test]
fn runtime_scope_fault_preserves_managed_runtime_identity() {
    let error = runtime_scope_error(RuntimeScopeError::Faulted {
        fault: RuntimeFault::engine(RuntimeFaultKind::RequiredService, "nara.test.winit-service"),
    });

    assert!(matches!(
        error,
        AppRunError::ManagedRuntime {
            kind: RuntimeFaultKind::RequiredService,
            fault_source: "nara.test.winit-service"
        }
    ));
}

#[test]
fn unavailable_runtime_scope_remains_a_runner_error() {
    let error = runtime_scope_error(RuntimeScopeError::Unavailable {
        state: RuntimeState::Stopped,
    });

    assert!(matches!(error, AppRunError::Runner { .. }));
}

impl Drop for TestWindowSource {
    fn drop(&mut self) {
        self.events.lock().unwrap().push("provider");
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

#[derive(Debug)]
struct FakeSurfaceOwner {
    _handle_source: WindowSurfaceHandleSource,
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl Drop for FakeSurfaceOwner {
    fn drop(&mut self) {
        self.events.lock().unwrap().push("surface");
    }
}

#[derive(Debug)]
struct FakeSurfaceState {
    owner: FakeSurfaceOwner,
    lease: WindowSurfaceLease,
}

#[derive(Debug, Default)]
struct FakeSurfaceBackend {
    surfaces: BTreeMap<WindowId, FakeSurfaceState>,
}

#[derive(Debug, Clone, Copy)]
struct FailingShutdownPlugin;

const FAILING_SHUTDOWN_PLUGIN_ID: PluginId = PluginId::new("test.shutdown-failure");
const FAILING_SHUTDOWN_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(FAILING_SHUTDOWN_PLUGIN_ID, PluginCategory::Backend);

impl Plugin for FailingShutdownPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &FAILING_SHUTDOWN_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: FAILING_SHUTDOWN_PLUGIN_ID,
            message: "injected shutdown failure".to_owned(),
        })
    }
}

fn start_runtime(app: App) -> RuntimeInstance {
    let candidate = nara_app::RuntimeCandidate::admit(app.seal().unwrap()).unwrap();
    match candidate.complete_startup() {
        Ok(ready) => ready.promote(),
        Err(failure) => {
            panic!("candidate startup failed: {:?}", failure.fault())
        }
    }
}

struct PendingCloseParticipant {
    released: Arc<std::sync::atomic::AtomicBool>,
    polls: Arc<AtomicUsize>,
}

impl RuntimeCloseParticipant for PendingCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        Ok(if self.released.load(std::sync::atomic::Ordering::SeqCst) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

fn start_runtime_with_pending_close(
    app: App,
    released: Arc<std::sync::atomic::AtomicBool>,
    close_timeout: Duration,
) -> RuntimeInstance {
    start_runtime_with_counted_pending_close(
        app,
        released,
        Arc::new(AtomicUsize::new(0)),
        close_timeout,
    )
}

fn start_runtime_with_counted_pending_close(
    app: App,
    released: Arc<std::sync::atomic::AtomicBool>,
    polls: Arc<AtomicUsize>,
    close_timeout: Duration,
) -> RuntimeInstance {
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.winit-pending"),
            PendingCloseParticipant { released, polls },
        )
        .unwrap();
    let candidate = nara_app::RuntimeCandidate::admit_with(
        app.seal().unwrap(),
        obligations,
        RuntimeClosePolicy::new(close_timeout),
    )
    .unwrap();
    candidate.complete_startup().unwrap().promote()
}

fn stop_runtime(runtime: &mut RuntimeInstance) {
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    if runtime.state() == RuntimeState::Stopping {
        runtime.drive(Duration::ZERO).unwrap();
    }
}

fn install_fake_surface_driver(app: &mut App) {
    let world = app.world_mut().unwrap();
    world.insert_non_send(FakeSurfaceBackend::default());
    world.insert_resource(WindowSurfaceRetirementDriver::new(
        "test.surface",
        retire_fake_surfaces,
    ));
}

fn add_fake_surface(
    app: &mut App,
    handles: &BackendWindowHandles,
    window_id: WindowId,
    events: Arc<Mutex<Vec<&'static str>>>,
) {
    let (handle_source, lease) = handles.acquire_surface(window_id).unwrap().into_parts();
    app.world_mut()
        .unwrap()
        .non_send_mut::<FakeSurfaceBackend>()
        .surfaces
        .insert(
            window_id,
            FakeSurfaceState {
                owner: FakeSurfaceOwner {
                    _handle_source: handle_source,
                    events,
                },
                lease,
            },
        );
}

fn retire_fake_surfaces(
    scope: &mut RuntimeDriverScope<'_>,
    window_ids: &[WindowId],
) -> Result<(), WindowSurfaceRetirementError> {
    let Some(mut backend) = scope
        .get_non_send_resource_mut::<FakeSurfaceBackend>()
        .map_err(|_| WindowSurfaceRetirementError::DriverFailed {
            driver: "test.surface",
        })?
    else {
        return Ok(());
    };
    let mut first_error = None;
    for window_id in window_ids {
        let Some(FakeSurfaceState { owner, lease }) = backend.surfaces.remove(window_id) else {
            continue;
        };
        drop(owner);
        if lease.confirm_owner_dropped().is_err() {
            first_error.get_or_insert(WindowSurfaceRetirementError::DriverFailed {
                driver: "test.surface",
            });
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[test]
fn converts_common_keyboard_codes() {
    assert_eq!(
        convert_key_code(WinitKeyCode::Escape),
        Some(KeyCode::Escape)
    );
    assert_eq!(convert_key_code(WinitKeyCode::Enter), Some(KeyCode::Enter));
    assert_eq!(convert_key_code(WinitKeyCode::Space), Some(KeyCode::Space));
    assert_eq!(
        convert_key_code(WinitKeyCode::ArrowLeft),
        Some(KeyCode::ArrowLeft)
    );
    assert_eq!(
        convert_key_code(WinitKeyCode::KeyA),
        Some(KeyCode::Character('a'))
    );
    assert_eq!(
        convert_key_code(WinitKeyCode::Digit1),
        Some(KeyCode::Character('1'))
    );
}

#[test]
fn ignores_unmapped_keyboard_codes() {
    assert_eq!(convert_key_code(WinitKeyCode::F1), None);
}

#[test]
fn converts_physical_keys() {
    assert_eq!(
        convert_physical_key(PhysicalKey::Code(WinitKeyCode::KeyW)),
        Some(KeyCode::Character('w'))
    );
}

#[test]
fn converts_mouse_buttons() {
    assert_eq!(
        convert_mouse_button(WinitMouseButton::Left),
        MouseButton::Left
    );
    assert_eq!(
        convert_mouse_button(WinitMouseButton::Right),
        MouseButton::Right
    );
    assert_eq!(
        convert_mouse_button(WinitMouseButton::Middle),
        MouseButton::Middle
    );
    assert_eq!(
        convert_mouse_button(WinitMouseButton::Back),
        MouseButton::Other(4)
    );
    assert_eq!(
        convert_mouse_button(WinitMouseButton::Other(9)),
        MouseButton::Other(9)
    );
}

#[test]
fn runner_configuration_does_not_install_runtime_prerequisites() {
    let app = App::new();
    let _runner = WinitRunner::default();

    assert!(!app.world().contains_resource::<WindowEvents>());
    assert!(!app.world().contains_resource::<BackendWindowHandles>());
    assert!(!app.world().contains_resource::<ButtonInput<KeyCode>>());
    assert!(!app.world().contains_resource::<ButtonInput<MouseButton>>());
    assert!(!app.world().contains_resource::<PointerState>());
}

#[test]
fn runner_shutdown_retires_surface_before_releasing_provider() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    handles
        .insert(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::new(TestWindowSource {
                events: Arc::clone(&events),
            })),
        )
        .unwrap();
    install_fake_surface_driver(&mut app);
    add_fake_surface(&mut app, &handles, WindowId::PRIMARY, Arc::clone(&events));

    let mut runtime = start_runtime(app);
    let owned_window_ids = BTreeSet::from([WindowId::PRIMARY]);
    retire_runtime_targets(&mut runtime, &handles, &owned_window_ids).unwrap();
    retire_runtime_targets(&mut runtime, &handles, &owned_window_ids).unwrap();
    stop_runtime(&mut runtime);

    assert_eq!(*events.lock().unwrap(), vec!["surface", "provider"]);
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        nara_window::backend::WindowTargetPhase::ProviderReleased
    );
}

#[test]
fn pending_stop_preserves_surface_retirement_authority_until_the_runner_barrier() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    handles
        .insert(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::new(TestWindowSource {
                events: Arc::clone(&events),
            })),
        )
        .unwrap();
    install_fake_surface_driver(&mut app);
    add_fake_surface(&mut app, &handles, WindowId::PRIMARY, Arc::clone(&events));

    let mut runtime = start_runtime(app);
    let owned_window_ids = BTreeSet::from([WindowId::PRIMARY]);
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));

    let stopping = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(stopping.state(), RuntimeState::Stopping);
    retire_runtime_targets(&mut runtime, &handles, &owned_window_ids).unwrap();
    let stopped = runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(stopped.state(), RuntimeState::Stopped);
    assert_eq!(*events.lock().unwrap(), vec!["surface", "provider"]);
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        nara_window::backend::WindowTargetPhase::ProviderReleased
    );
}

#[test]
fn runner_shutdown_does_not_retire_targets_owned_by_another_adapter() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let owned_events = Arc::new(Mutex::new(Vec::new()));
    let foreign_events = Arc::new(Mutex::new(Vec::new()));
    handles
        .insert(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::new(TestWindowSource {
                events: Arc::clone(&owned_events),
            })),
        )
        .unwrap();
    handles
        .insert(
            WindowId::new(2),
            WindowHandleProvider::new(Arc::new(TestWindowSource {
                events: Arc::clone(&foreign_events),
            })),
        )
        .unwrap();
    install_fake_surface_driver(&mut app);
    add_fake_surface(
        &mut app,
        &handles,
        WindowId::PRIMARY,
        Arc::clone(&owned_events),
    );
    add_fake_surface(
        &mut app,
        &handles,
        WindowId::new(2),
        Arc::clone(&foreign_events),
    );

    let mut runtime = start_runtime(app);
    let owned_window_ids = BTreeSet::from([WindowId::PRIMARY]);
    retire_runtime_targets(&mut runtime, &handles, &owned_window_ids).unwrap();

    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        nara_window::backend::WindowTargetPhase::ProviderReleased
    );
    assert_eq!(
        handles.snapshot(WindowId::new(2)).unwrap().phase,
        nara_window::backend::WindowTargetPhase::Active
    );
    assert!(handles.snapshot(WindowId::new(2)).unwrap().surface_active);
    assert_eq!(*owned_events.lock().unwrap(), vec!["surface", "provider"]);
    assert!(foreign_events.lock().unwrap().is_empty());
}

#[test]
fn shutdown_failure_does_not_block_safe_target_retirement() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    handles
        .insert(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::new(TestWindowSource {
                events: Arc::clone(&events),
            })),
        )
        .unwrap();
    install_fake_surface_driver(&mut app);
    add_fake_surface(&mut app, &handles, WindowId::PRIMARY, Arc::clone(&events));
    app.add_plugin(FailingShutdownPlugin).unwrap();

    let mut runtime = start_runtime(app);
    let mut state = WinitApp::new(&mut runtime).unwrap();
    state.owned_window_ids.insert(WindowId::PRIMARY);
    let now = Instant::now();

    assert!(matches!(
        state.begin_shutdown_transition(now),
        EventLoopDirective::WaitUntil(_)
    ));
    assert_eq!(state.runtime.state(), RuntimeState::Stopping);
    assert_eq!(*events.lock().unwrap(), vec!["surface", "provider"]);
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        nara_window::backend::WindowTargetPhase::ProviderReleased
    );

    let close_failure = state.poll_runtime_close().unwrap_err();
    assert_eq!(
        close_failure,
        AppRunError::runner("managed runtime plugin shutdown failed")
    );
    state.record_runtime_close_failure(close_failure.clone());
    assert_eq!(state.runtime.state(), RuntimeState::Stopped);
    assert!(state.runtime.close_evidence().plugin_shutdown_failed());

    handles.mark_native_destroyed(WindowId::PRIMARY).unwrap();
    assert_eq!(
        state.poll_native_shutdown_transition(Instant::now()),
        EventLoopDirective::Exit
    );
    assert_eq!(state.shutdown, WinitShutdownState::Complete);
    assert_eq!(state.take_failure(), Some(close_failure));
    drop(state);

    assert_eq!(runtime.state(), RuntimeState::Stopped);
}

#[test]
fn event_loop_finish_does_not_replace_the_primary_failure() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    handles
        .insert(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::new(TestWindowSource {
                events: Arc::clone(&events),
            })),
        )
        .unwrap();
    install_fake_surface_driver(&mut app);
    add_fake_surface(&mut app, &handles, WindowId::PRIMARY, Arc::clone(&events));
    let mut runtime = start_runtime(app);
    let mut state = WinitApp::new(&mut runtime).unwrap();
    state.owned_window_ids.insert(WindowId::PRIMARY);
    let primary = AppRunError::runner("primary runner failure");
    state.record_primary_failure(primary.clone());

    state.finish_after_event_loop();

    assert_eq!(
        state.take_failure(),
        Some(AppRunError::runner_teardown(
            primary,
            AppRunError::runner("winit event loop ended before controlled window retirement")
        ))
    );
    assert_eq!(state.shutdown, WinitShutdownState::Aborted);
    assert_eq!(*events.lock().unwrap(), vec!["surface", "provider"]);
    assert_eq!(
        handles.snapshot(WindowId::PRIMARY).unwrap().phase,
        nara_window::backend::WindowTargetPhase::ProviderReleased
    );
    assert!(
        !handles
            .snapshot(WindowId::PRIMARY)
            .unwrap()
            .provider_present
    );
}

#[test]
fn runner_failure_and_native_teardown_failure_remain_distinct() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let mut runtime = start_runtime(app);
    let mut state = WinitApp::new(&mut runtime).unwrap();
    let primary = AppRunError::runner("winit event loop failed: os failure");
    let teardown = AppRunError::runner("timed out waiting for native window destruction");

    state.record_native_retirement_failure(teardown.clone());
    state.record_primary_failure(primary.clone());

    assert_eq!(
        state.take_failure(),
        Some(AppRunError::runner_teardown(primary, teardown))
    );
}

#[test]
fn external_destroyed_event_faults_and_retires_the_owned_target_once() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    handles
        .insert(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::new(TestWindowSource {
                events: Arc::clone(&events),
            })),
        )
        .unwrap();
    install_fake_surface_driver(&mut app);
    add_fake_surface(&mut app, &handles, WindowId::PRIMARY, Arc::clone(&events));
    let mut runtime = start_runtime(app);
    let mut state = WinitApp::new(&mut runtime).unwrap();
    let winit_window_id = WinitWindowId::dummy();
    state
        .nara_windows_by_winit
        .insert(winit_window_id, WindowId::PRIMARY);
    state.owned_window_ids.insert(WindowId::PRIMARY);

    let now = Instant::now();
    assert!(matches!(
        state.handle_destroyed_window(winit_window_id, now).unwrap(),
        EventLoopDirective::WaitUntil(_)
    ));
    assert_eq!(state.runtime.state(), RuntimeState::Stopping);

    state.poll_runtime_close().unwrap();
    assert_eq!(
        state.poll_native_shutdown_transition(now),
        EventLoopDirective::Exit
    );

    assert_eq!(state.shutdown, WinitShutdownState::Complete);
    assert_eq!(
        state.primary_failure,
        Some(AppRunError::runner(
            "native window was destroyed before controlled retirement"
        ))
    );
    assert_eq!(state.runtime_close_failure, None);
    assert_eq!(state.native_retirement_failure, None);
    assert!(!state.nara_windows_by_winit.contains_key(&winit_window_id));
    assert_eq!(
        state.runtime.world().resource::<WindowEvents>().as_slice(),
        &[WindowEvent::Closed {
            window_id: WindowId::PRIMARY,
        }]
    );
    assert_eq!(*events.lock().unwrap(), vec!["surface", "provider"]);
    let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
    assert_eq!(
        snapshot.phase,
        nara_window::backend::WindowTargetPhase::NativeDestroyed
    );
    assert_eq!(
        snapshot.fault,
        Some(nara_window::backend::WindowTargetFault::ExternallyDestroyed)
    );
    assert!(!snapshot.provider_present);

    assert_eq!(
        state
            .handle_destroyed_window(winit_window_id, Instant::now())
            .unwrap(),
        EventLoopDirective::None
    );
    assert_eq!(
        state
            .runtime
            .world()
            .resource::<WindowEvents>()
            .as_slice()
            .len(),
        1
    );
    assert_eq!(*events.lock().unwrap(), vec!["surface", "provider"]);
}

#[test]
fn shutdown_ignores_repeated_close_but_accepts_destroyed() {
    let deadline = Instant::now() + Duration::from_secs(1);
    assert!(should_process_window_event_during_shutdown(
        WinitShutdownState::Running,
        &WinitWindowEvent::CloseRequested
    ));
    assert!(!should_process_window_event_during_shutdown(
        WinitShutdownState::WaitingForNative { deadline },
        &WinitWindowEvent::CloseRequested
    ));
    assert!(!should_process_window_event_during_shutdown(
        WinitShutdownState::Aborted,
        &WinitWindowEvent::CloseRequested
    ));
    assert!(should_process_window_event_during_shutdown(
        WinitShutdownState::WaitingForNative { deadline },
        &WinitWindowEvent::Destroyed
    ));
    assert!(should_process_window_event_during_shutdown(
        WinitShutdownState::Aborted,
        &WinitWindowEvent::Destroyed
    ));
}

#[test]
fn native_shutdown_waits_for_destroyed_and_has_a_finite_timeout() {
    let handles = BackendWindowHandles::default();
    let events = Arc::new(Mutex::new(Vec::new()));
    handles
        .insert(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::new(TestWindowSource {
                events: Arc::clone(&events),
            })),
        )
        .unwrap();
    handles.request_retirement(WindowId::PRIMARY).unwrap();
    handles.release_provider(WindowId::PRIMARY).unwrap();

    let now = Instant::now();
    let deadline = now + Duration::from_secs(1);
    let owned_window_ids = BTreeSet::from([WindowId::PRIMARY]);
    assert_eq!(
        native_shutdown_action(&handles, &owned_window_ids, now, deadline),
        Ok(NativeShutdownAction::WaitUntil(deadline))
    );

    handles.mark_native_destroyed(WindowId::PRIMARY).unwrap();
    assert_eq!(
        native_shutdown_action(&handles, &owned_window_ids, now, deadline),
        Ok(NativeShutdownAction::Complete)
    );

    let pending = BackendWindowHandles::default();
    pending
        .insert(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::new(TestWindowSource { events })),
        )
        .unwrap();
    pending.request_retirement(WindowId::PRIMARY).unwrap();
    pending.release_provider(WindowId::PRIMARY).unwrap();
    assert_eq!(
        native_shutdown_action(&pending, &owned_window_ids, deadline, deadline),
        Ok(NativeShutdownAction::TimedOut)
    );
}

#[test]
fn native_completion_waits_for_runtime_close_before_success() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut runtime =
        start_runtime_with_pending_close(app, released.clone(), Duration::from_secs(5));
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopping);

    let mut state = WinitApp::new(&mut runtime).unwrap();
    let now = Instant::now();
    state.shutdown = WinitShutdownState::WaitingForNative {
        deadline: now + Duration::from_secs(5),
    };
    assert!(matches!(
        state.poll_native_shutdown_transition(now),
        EventLoopDirective::WaitUntil(_)
    ));
    assert_eq!(
        state.shutdown,
        WinitShutdownState::WaitingForNative {
            deadline: now + Duration::from_secs(5),
        }
    );

    released.store(true, std::sync::atomic::Ordering::SeqCst);
    state.poll_runtime_close().unwrap();
    assert_eq!(state.runtime.state(), RuntimeState::Stopped);
    assert_eq!(
        state.poll_native_shutdown_transition(Instant::now()),
        EventLoopDirective::Exit
    );
    assert_eq!(state.shutdown, WinitShutdownState::Complete);
}

#[test]
fn native_completion_aborts_on_incomplete_runtime_close_and_preserves_retirement_owner() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut runtime = start_runtime_with_pending_close(app, released.clone(), Duration::ZERO);
    stop_runtime(&mut runtime);
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);

    let mut state = WinitApp::new(&mut runtime).unwrap();
    let now = Instant::now();
    state.shutdown = WinitShutdownState::WaitingForNative {
        deadline: now + Duration::from_secs(5),
    };

    assert_eq!(
        state.poll_native_shutdown_transition(now),
        EventLoopDirective::Exit
    );
    assert_eq!(state.shutdown, WinitShutdownState::Aborted);
    assert_eq!(state.runtime.state(), RuntimeState::CloseIncomplete);
    assert!(
        state
            .runtime
            .close_evidence()
            .causes()
            .contains(&RuntimeCloseCause::DeadlineExceeded)
    );
    assert_eq!(
        state.take_failure(),
        Some(AppRunError::runner("managed runtime close is incomplete"))
    );
    drop(state);

    let mut retirement = runtime.begin_retirement();
    assert_eq!(
        retirement.retirement_state(),
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert!(
        retirement
            .close_evidence()
            .causes()
            .contains(&RuntimeCloseCause::DeadlineExceeded)
    );
    released.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
}

#[test]
fn native_timeout_preserves_combined_failure_without_false_runtime_stop() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    handles
        .insert(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::new(TestWindowSource { events })),
        )
        .unwrap();
    let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut runtime =
        start_runtime_with_pending_close(app, released.clone(), Duration::from_secs(5));
    stop_runtime(&mut runtime);
    assert_eq!(runtime.state(), RuntimeState::Stopping);

    let mut state = WinitApp::new(&mut runtime).unwrap();
    state.owned_window_ids.insert(WindowId::PRIMARY);
    let now = Instant::now();
    state.shutdown = WinitShutdownState::WaitingForNative { deadline: now };
    let primary = AppRunError::runner("primary runner failure");
    state.record_primary_failure(primary.clone());

    assert_eq!(
        state.poll_native_shutdown_transition(now),
        EventLoopDirective::Exit
    );
    assert_eq!(state.shutdown, WinitShutdownState::Aborted);
    assert_eq!(state.runtime.state(), RuntimeState::Stopping);
    assert_eq!(
        state.take_failure(),
        Some(AppRunError::runner_teardown(
            primary,
            AppRunError::runner("timed out waiting for native window destruction")
        ))
    );
    drop(state);

    assert_eq!(runtime.state(), RuntimeState::Stopping);
    released.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut retirement = runtime.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
}

#[test]
fn close_incomplete_then_native_timeout_preserves_both_failures() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    handles
        .insert(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::new(TestWindowSource {
                events: Arc::new(Mutex::new(Vec::new())),
            })),
        )
        .unwrap();
    let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut runtime = start_runtime_with_pending_close(app, released.clone(), Duration::ZERO);
    stop_runtime(&mut runtime);
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);

    let mut state = WinitApp::new(&mut runtime).unwrap();
    state.owned_window_ids.insert(WindowId::PRIMARY);
    let now = Instant::now();
    state.shutdown = WinitShutdownState::WaitingForNative { deadline: now };

    state.poll_runtime_close().unwrap();
    assert_eq!(
        state.poll_native_shutdown_transition(now),
        EventLoopDirective::Exit
    );
    assert_eq!(
        state.take_failure(),
        Some(AppRunError::runner_teardown(
            AppRunError::runner("managed runtime close is incomplete"),
            AppRunError::runner("timed out waiting for native window destruction")
        ))
    );
    drop(state);

    released.store(true, Ordering::SeqCst);
    let mut retirement = runtime.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
}

#[test]
fn close_incomplete_then_missing_target_preserves_both_failures() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut runtime = start_runtime_with_pending_close(app, released.clone(), Duration::ZERO);
    stop_runtime(&mut runtime);
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);

    let mut state = WinitApp::new(&mut runtime).unwrap();
    state.owned_window_ids.insert(WindowId::PRIMARY);
    let now = Instant::now();
    state.shutdown = WinitShutdownState::WaitingForNative {
        deadline: now + Duration::from_secs(5),
    };

    state.poll_runtime_close().unwrap();
    assert_eq!(
        state.poll_native_shutdown_transition(now),
        EventLoopDirective::Exit
    );
    assert_eq!(
        state.take_failure(),
        Some(AppRunError::runner_teardown(
            AppRunError::runner("managed runtime close is incomplete"),
            AppRunError::runner("native window target disappeared during shutdown")
        ))
    );
    drop(state);

    released.store(true, Ordering::SeqCst);
    let mut retirement = runtime.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
}

#[test]
fn close_incomplete_is_recorded_once_while_native_retirement_continues() {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    let handles = app.world().resource::<BackendWindowHandles>().clone();
    let events = Arc::new(Mutex::new(Vec::new()));
    handles
        .insert(
            WindowId::PRIMARY,
            WindowHandleProvider::new(Arc::new(TestWindowSource { events })),
        )
        .unwrap();
    let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let polls = Arc::new(AtomicUsize::new(0));
    let mut runtime = start_runtime_with_counted_pending_close(
        app,
        released.clone(),
        polls.clone(),
        Duration::ZERO,
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));

    let mut state = WinitApp::new(&mut runtime).unwrap();
    state.owned_window_ids.insert(WindowId::PRIMARY);
    let now = Instant::now();
    state.shutdown = WinitShutdownState::WaitingForNative {
        deadline: now + Duration::from_secs(5),
    };

    state.poll_runtime_close().unwrap();
    assert_eq!(state.runtime.state(), RuntimeState::Stopping);
    assert_eq!(polls.load(Ordering::SeqCst), 0);

    state.poll_runtime_close().unwrap();
    assert_eq!(state.runtime.state(), RuntimeState::CloseIncomplete);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    let first_failure = state
        .runtime_close_failure
        .as_ref()
        .expect("the first incomplete close records teardown evidence")
        as *const AppRunError;
    assert!(matches!(
        state.poll_native_shutdown_transition(now),
        EventLoopDirective::WaitUntil(_)
    ));

    state.poll_runtime_close().unwrap();
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    let retained_failure = state
        .runtime_close_failure
        .as_ref()
        .expect("repeated polling keeps the original close failure")
        as *const AppRunError;
    assert_eq!(retained_failure, first_failure);

    handles.mark_native_destroyed(WindowId::PRIMARY).unwrap();
    assert_eq!(
        state.poll_native_shutdown_transition(Instant::now()),
        EventLoopDirective::Exit
    );
    assert_eq!(state.shutdown, WinitShutdownState::Aborted);
    assert_eq!(state.runtime.state(), RuntimeState::CloseIncomplete);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    drop(state);

    released.store(true, std::sync::atomic::Ordering::SeqCst);
    let mut retirement = runtime.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
}
