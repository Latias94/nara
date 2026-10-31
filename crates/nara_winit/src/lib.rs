//! Winit runner and native window adapter for nara.

#[cfg(feature = "accessibility")]
mod accessibility;
#[cfg(all(test, target_os = "windows"))]
mod native_tests;
mod window_mode;

#[cfg(feature = "accessibility")]
pub use accessibility::{
    WinitAccessibilityActionRequest, WinitAccessibilityEndpointRetirement, WinitAccessibilityError,
    WinitAccessibilityPublisher, WinitAccessibilityPublisherId, WinitAccessibilityReceipt,
    WinitAccessibilityRetirementReceipt, WinitAccessibilitySuspension,
    WinitAccessibilitySuspensionReceipt, WinitAccessibilityUpdate,
};

#[doc(hidden)]
pub mod __private {
    #[cfg(feature = "platform-accessibility")]
    pub use super::accessibility::__private::*;

    use super::{AppExit, AppRunError, BackendWindowHandles, Instant, RuntimeInstance, WindowId};

    /// Local presentation custody supplied by the concrete desktop assembly.
    ///
    /// Implementations retain refused owned results and continue servicing logical close after
    /// capture admission stops. Physical retirement must remain driveable after Runtime Stop.
    pub trait __WinitPresentationDriver {
        fn maintain(
            &mut self,
            runtime: &mut RuntimeInstance,
            handles: &BackendWindowHandles,
            now: Instant,
        ) -> Result<__WinitPresentationProgress, AppRunError>;

        /// `true` acknowledges actual graphics, submission and surface retirement.
        fn retire(
            &mut self,
            runtime: &mut RuntimeInstance,
            handles: &BackendWindowHandles,
            window_ids: &[WindowId],
            now: Instant,
        ) -> Result<bool, AppRunError>;
    }

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct __WinitPresentationProgress {
        pub exit: Option<AppExit>,
        pub next_wake: Option<Instant>,
    }
}

use __private::{__WinitPresentationDriver, __WinitPresentationProgress};

#[cfg(feature = "platform-accessibility")]
use accessibility::WinitPlatformAccessibilitySession;

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use nara_app::{
    AppExit, AppRunError, RuntimeControl, RuntimeControlRequestResult, RuntimeDriveError,
    RuntimeDriverScope, RuntimeFault, RuntimeInstance, RuntimeScopeError, RuntimeState,
};
use nara_input::{
    ButtonDriverInput, ButtonInputDriverError, InputIngressError, InputPointerId, InputSeatId,
    InputTimeline, InputWindowEvent, InputWindowId, KeyCode, MouseButton, PointerDriverInput,
    apply_keyboard_driver_input_at, apply_mouse_driver_input_at, apply_pointer_driver_input_at,
    apply_window_input_driver_event, connect_input_endpoint, connect_input_pointer,
};
use nara_window::{
    Window, WindowEvent, WindowId, WindowResolution,
    backend::{
        BackendWindowHandles, WindowHandleProvider, WindowPresentationRequirement,
        WindowTargetPhase,
    },
    push_window_driver_event,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{
        ElementState, MouseButton as WinitMouseButton, RawKeyEvent, WindowEvent as WinitWindowEvent,
    },
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{KeyCode as WinitKeyCode, PhysicalKey},
    window::{Window as WinitWindow, WindowId as WinitWindowId},
};

#[cfg(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
))]
use winit::platform::pump_events::{EventLoopExtPumpEvents, PumpStatus};

const DEFAULT_NATIVE_RETIREMENT_BUDGET: Duration = Duration::from_secs(5);
const RUNTIME_CLOSE_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventLoopDirective {
    None,
    Poll,
    Wait,
    WaitUntil(Instant),
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WinitShutdownState {
    Running,
    Retiring,
    Complete,
}

#[derive(Debug)]
enum WinitUserEvent {
    #[cfg(feature = "platform-accessibility")]
    AccessibilityWake(WindowId),
    #[cfg(all(test, target_os = "windows"))]
    NativeTest(native_tests::NativeTestEvent),
}

impl WinitShutdownState {
    const fn is_started(self) -> bool {
        !matches!(self, Self::Running)
    }
}

/// Native event-loop runner configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinitRunner {
    pub control_flow: WinitControlFlow,
}

impl WinitRunner {
    #[must_use]
    pub fn new(control_flow: WinitControlFlow) -> Self {
        Self { control_flow }
    }

    /// Runs an explicit platform-only session with no presentation owner.
    ///
    /// A Runtime declaring a presentation requirement needs concrete presentation assembly
    /// instead. An incomplete return retains all owners and must be driven again with `execute`.
    #[must_use]
    pub fn run(self, runtime: RuntimeInstance) -> WinitSession {
        let expects_presentation = runtime
            .world()
            .contains_resource::<WindowPresentationRequirement>();
        let mut app = WinitApp::new(
            runtime,
            Box::new(NoPresentationDriver),
            self.control_flow,
            DEFAULT_NATIVE_RETIREMENT_BUDGET,
        );
        if expects_presentation {
            app.record_primary_failure(AppRunError::runner(
                "platform-only Winit run cannot own an expected presentation backend",
            ));
        }
        WinitSession::start(app)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn __run_with_presentation(
        self,
        runtime: RuntimeInstance,
        driver: Box<dyn __WinitPresentationDriver>,
        native_retirement_budget: Duration,
    ) -> WinitSession {
        WinitSession::start(WinitApp::new(
            runtime,
            driver,
            self.control_flow,
            native_retirement_budget,
        ))
    }
}

/// The result of one finite retirement drive, or of the initial native run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinitRunProgress {
    Retired,
    Incomplete,
}

/// A thread-bound native aggregate retaining the original event pump and target authority.
///
/// `execute` runs the application initially. After an incomplete exit it only advances retirement;
/// it never recreates a window or resumes simulation. Dropping an unfinished session makes one
/// finite close attempt, then aborts if native or Runtime custody still cannot be released.
#[must_use = "an incomplete native session must retain and retire its owners"]
pub struct WinitSession {
    app: Option<WinitApp>,
    event_loop: Option<EventLoop<WinitUserEvent>>,
    executed: bool,
}

/// Terminal result and stopped Runtime, available only after all native owners have retired.
#[derive(Debug)]
pub struct WinitRetiredSession {
    pub runtime: RuntimeInstance,
    pub result: Result<AppExit, AppRunError>,
}

impl std::fmt::Debug for WinitSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WinitSession")
            .field("runtime", &self.app.as_ref().map(|app| &app.runtime))
            .field("retired", &self.is_retired())
            .field("retains_event_loop", &self.event_loop.is_some())
            .finish_non_exhaustive()
    }
}

impl WinitSession {
    fn start(mut app: WinitApp) -> Self {
        let event_loop = if app.primary_failure.is_none() {
            match EventLoop::<WinitUserEvent>::with_user_event().build() {
                Ok(event_loop) => {
                    event_loop.set_control_flow(app.control_flow.into_winit());
                    app.event_proxy = Some(event_loop.create_proxy());
                    Some(event_loop)
                }
                Err(error) => {
                    app.record_primary_failure(AppRunError::runner(format!(
                        "failed to create winit event loop: {error}"
                    )));
                    None
                }
            }
        } else {
            None
        };
        let mut session = Self {
            app: Some(app),
            event_loop,
            executed: false,
        };
        session.execute();
        session
    }

    #[must_use]
    pub fn runtime(&self) -> &RuntimeInstance {
        // The extraction slot is emptied only while consuming a retired session.
        &self
            .app
            .as_ref()
            .expect("live session owns its Runtime")
            .runtime
    }

    #[must_use]
    pub fn is_retired(&self) -> bool {
        self.app
            .as_ref()
            .is_none_or(|app| app.shutdown == WinitShutdownState::Complete)
    }

    /// Starts the initial run, or retries retirement under the original native affinity.
    pub fn execute(&mut self) -> WinitRunProgress {
        let Some(app) = self.app.as_mut() else {
            return WinitRunProgress::Retired;
        };
        if app.shutdown == WinitShutdownState::Complete {
            return WinitRunProgress::Retired;
        }
        if self.executed {
            app.prepare_retirement_retry(Instant::now());
        } else if app.primary_failure.is_some()
            || !matches!(
                app.runtime.state(),
                RuntimeState::Running | RuntimeState::Paused
            )
        {
            app.begin_shutdown_transition(Instant::now());
        }
        self.executed = true;

        loop {
            if let Some(event_loop) = self.event_loop.as_mut() {
                #[cfg(any(
                    target_os = "windows",
                    target_os = "macos",
                    target_os = "linux",
                    target_os = "android",
                    target_os = "freebsd",
                    target_os = "dragonfly",
                    target_os = "netbsd",
                    target_os = "openbsd"
                ))]
                {
                    app.boundary_serviced = false;
                    let status = event_loop.pump_app_events(app.pump_timeout(Instant::now()), app);
                    if !app.boundary_serviced {
                        if app.shutdown.is_started() {
                            app.poll_native_shutdown_once(Instant::now());
                        } else {
                            match app.maintain_transition(Instant::now()) {
                                EventLoopDirective::Poll => {
                                    event_loop.set_control_flow(ControlFlow::Poll)
                                }
                                EventLoopDirective::Wait => {
                                    event_loop.set_control_flow(ControlFlow::Wait)
                                }
                                EventLoopDirective::WaitUntil(wake) => {
                                    event_loop.set_control_flow(ControlFlow::WaitUntil(wake))
                                }
                                EventLoopDirective::None | EventLoopDirective::Exit => {}
                            }
                        }
                    }
                    if let PumpStatus::Exit(code) = status
                        && app.shutdown != WinitShutdownState::Complete
                    {
                        app.record_primary_failure(AppRunError::runner(format!(
                            "winit event pump exited before controlled retirement (code {code})"
                        )));
                        app.begin_shutdown_transition(Instant::now());
                        return if app.shutdown == WinitShutdownState::Complete {
                            WinitRunProgress::Retired
                        } else {
                            WinitRunProgress::Incomplete
                        };
                    }
                }
                #[cfg(not(any(
                    target_os = "windows",
                    target_os = "macos",
                    target_os = "linux",
                    target_os = "android",
                    target_os = "freebsd",
                    target_os = "dragonfly",
                    target_os = "netbsd",
                    target_os = "openbsd"
                )))]
                {
                    let _ = event_loop;
                    app.record_primary_failure(AppRunError::runner(
                        "retained Winit event pumping is unavailable on this platform",
                    ));
                    app.begin_shutdown_transition(Instant::now());
                    self.event_loop = None;
                }
            } else {
                app.begin_shutdown_transition(Instant::now());
                app.poll_native_shutdown_once(Instant::now());
            }

            if app.shutdown == WinitShutdownState::Complete {
                return WinitRunProgress::Retired;
            }
            if app.retirement_drive_incomplete(Instant::now()) {
                return WinitRunProgress::Incomplete;
            }
            if self.event_loop.is_none() {
                std::thread::sleep(
                    app.pump_timeout(Instant::now())
                        .unwrap_or(RUNTIME_CLOSE_POLL_INTERVAL),
                );
            }
        }
    }

    /// Extracts the final result without exposing an unfinished Runtime/native aggregate.
    pub fn into_retired(mut self) -> Result<WinitRetiredSession, Self> {
        if !self.is_retired() {
            return Err(self);
        }
        let Some(app) = self.app.take() else {
            return Err(self);
        };
        let result = app.failure().map_or(Ok(app.exit), Err);
        Ok(WinitRetiredSession {
            runtime: app.runtime,
            result,
        })
    }
}

impl Drop for WinitSession {
    fn drop(&mut self) {
        if self.is_retired() {
            return;
        }
        // Runtime quarantine cannot adopt this local driver, original pump and native owners.
        if std::thread::panicking() {
            std::process::abort();
        }
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(app) = self.app.as_mut() {
                app.begin_shutdown_transition(Instant::now());
            }
            self.execute()
        }));
        if attempt.is_err() || !self.is_retired() {
            std::process::abort();
        }
    }
}

/// An explicit platform-only control, never selected by inspecting missing render resources.
struct NoPresentationDriver;

impl __WinitPresentationDriver for NoPresentationDriver {
    fn maintain(
        &mut self,
        _runtime: &mut RuntimeInstance,
        _handles: &BackendWindowHandles,
        _now: Instant,
    ) -> Result<__WinitPresentationProgress, AppRunError> {
        Ok(__WinitPresentationProgress::default())
    }

    fn retire(
        &mut self,
        _runtime: &mut RuntimeInstance,
        _handles: &BackendWindowHandles,
        _window_ids: &[WindowId],
        _now: Instant,
    ) -> Result<bool, AppRunError> {
        Ok(true)
    }
}

impl Default for WinitRunner {
    fn default() -> Self {
        Self {
            control_flow: WinitControlFlow::Poll,
        }
    }
}

/// Event-loop scheduling mode exposed without leaking winit types into nara APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinitControlFlow {
    /// Drive Runtime frames continuously at native service boundaries.
    Poll,
    /// Drive frames for native window/input demand; service presentation wakes without a frame.
    Wait,
}

impl WinitControlFlow {
    fn into_winit(self) -> ControlFlow {
        match self {
            Self::Poll => ControlFlow::Poll,
            Self::Wait => ControlFlow::Wait,
        }
    }
}

struct WinitApp {
    runtime: RuntimeInstance,
    presentation: Box<dyn __WinitPresentationDriver>,
    nara_windows_by_winit: HashMap<WinitWindowId, WindowId>,
    platform_windows: HashMap<WindowId, Arc<WinitWindow>>,
    owned_window_ids: BTreeSet<WindowId>,
    last_frame: Instant,
    frame_requested: bool,
    exit: AppExit,
    primary_failure: Option<AppRunError>,
    presentation_failure: Option<AppRunError>,
    runtime_close_failure: Option<AppRunError>,
    native_retirement_failure: Option<AppRunError>,
    backend_windows: BackendWindowHandles,
    shutdown: WinitShutdownState,
    control_flow: WinitControlFlow,
    native_retirement_budget: Duration,
    retirement_deadline: Option<Instant>,
    presentation_next_wake: Option<Instant>,
    presentation_retired: bool,
    providers_released: bool,
    boundary_serviced: bool,
    runtime_close_incomplete_observed: bool,
    input_gate: WinitInputGate,
    input_window_id: WindowId,
    event_proxy: Option<EventLoopProxy<WinitUserEvent>>,
    #[cfg(feature = "platform-accessibility")]
    accessibility_activation: accessibility::__private::WinitAccessibilityActivation,
    #[cfg(feature = "platform-accessibility")]
    accessibility_sessions: HashMap<WindowId, WinitPlatformAccessibilitySession>,
}

#[derive(Debug)]
struct WinitInputGate {
    focused: bool,
    suppressed_keys: HashSet<KeyCode>,
    suppressed_mouse_buttons: HashSet<MouseButton>,
}

impl Default for WinitInputGate {
    fn default() -> Self {
        Self::reset_for_focus(true)
    }
}

impl WinitInputGate {
    fn reset_for_focus(focused: bool) -> Self {
        Self {
            focused,
            suppressed_keys: HashSet::new(),
            suppressed_mouse_buttons: HashSet::new(),
        }
    }

    fn lose_focus(
        &mut self,
        released_keys: impl IntoIterator<Item = KeyCode>,
        released_mouse_buttons: impl IntoIterator<Item = MouseButton>,
    ) {
        self.focused = false;
        self.suppressed_keys.extend(released_keys);
        self.suppress_mouse_buttons(released_mouse_buttons);
    }

    fn suppress_mouse_buttons(
        &mut self,
        released_mouse_buttons: impl IntoIterator<Item = MouseButton>,
    ) {
        self.suppressed_mouse_buttons.extend(released_mouse_buttons);
    }

    fn gain_focus(&mut self) {
        self.focused = true;
    }

    fn keyboard_input(
        &mut self,
        key: KeyCode,
        state: ElementState,
        repeat: bool,
    ) -> Option<ButtonDriverInput<KeyCode>> {
        match state {
            ElementState::Released => {
                self.suppressed_keys.remove(&key);
                self.focused.then_some(ButtonDriverInput::Release(key))
            }
            ElementState::Pressed
                if self.focused && !repeat && !self.suppressed_keys.contains(&key) =>
            {
                Some(ButtonDriverInput::Press(key))
            }
            ElementState::Pressed => None,
        }
    }

    fn mouse_input(
        &mut self,
        button: MouseButton,
        state: ElementState,
    ) -> Option<ButtonDriverInput<MouseButton>> {
        match state {
            ElementState::Released => {
                self.suppressed_mouse_buttons.remove(&button);
                self.focused.then_some(ButtonDriverInput::Release(button))
            }
            ElementState::Pressed
                if self.focused && !self.suppressed_mouse_buttons.contains(&button) =>
            {
                Some(ButtonDriverInput::Press(button))
            }
            ElementState::Pressed => None,
        }
    }
}

impl WinitApp {
    fn new(
        runtime: RuntimeInstance,
        presentation: Box<dyn __WinitPresentationDriver>,
        control_flow: WinitControlFlow,
        native_retirement_budget: Duration,
    ) -> Self {
        let mut primary_failure = None;
        #[cfg(feature = "platform-accessibility")]
        let accessibility_activation =
            match accessibility::__private::capture_winit_accessibility_activation(runtime.world())
            {
                accessibility::__private::WinitAccessibilityActivation::Partial(error) => {
                    primary_failure = Some(AppRunError::runner(format!(
                        "Winit accessibility contribution is partial: {error}"
                    )));
                    accessibility::__private::WinitAccessibilityActivation::Disabled
                }
                activation => activation,
            };
        let backend_windows = runtime
            .world()
            .get_resource::<BackendWindowHandles>()
            .cloned()
            .unwrap_or_else(|| {
                primary_failure.get_or_insert_with(|| {
                    AppRunError::runner("backend window authority is missing")
                });
                // No native target exists at construction. This private empty authority cannot
                // replace a registered authority or establish presentation retirement.
                BackendWindowHandles::default()
            });
        Self {
            runtime,
            presentation,
            nara_windows_by_winit: HashMap::new(),
            platform_windows: HashMap::new(),
            owned_window_ids: BTreeSet::new(),
            last_frame: Instant::now(),
            frame_requested: true,
            exit: AppExit::Success,
            primary_failure,
            presentation_failure: None,
            runtime_close_failure: None,
            native_retirement_failure: None,
            backend_windows,
            shutdown: WinitShutdownState::Running,
            control_flow,
            native_retirement_budget,
            retirement_deadline: None,
            presentation_next_wake: None,
            presentation_retired: false,
            providers_released: false,
            boundary_serviced: false,
            runtime_close_incomplete_observed: false,
            input_gate: WinitInputGate::default(),
            input_window_id: WindowId::PRIMARY,
            event_proxy: None,
            #[cfg(feature = "platform-accessibility")]
            accessibility_activation,
            #[cfg(feature = "platform-accessibility")]
            accessibility_sessions: HashMap::new(),
        }
    }

    fn with_driver_scope<R: Copy, E>(
        &mut self,
        operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> Result<R, E>,
    ) -> Result<R, AppRunError> {
        with_runtime_driver_scope(&mut self.runtime, operation)
    }

    fn create_primary_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppRunError> {
        if self.shutdown.is_started() || !self.platform_windows.is_empty() {
            return Ok(());
        }

        let window = configured_primary_window(&self.runtime)?;
        let window_id = window.id;
        if self.backend_windows.is_registered(window_id) {
            return Err(AppRunError::runner(
                "native window target is already registered",
            ));
        }
        let attributes = WinitWindow::default_attributes()
            .with_title(window.title.clone())
            .with_inner_size(PhysicalSize::new(
                window.resolution.physical_width,
                window.resolution.physical_height,
            ))
            .with_resizable(window.resizable)
            .with_fullscreen(window_mode::initial_fullscreen(
                &window,
                event_loop.primary_monitor(),
            )?);
        #[cfg(feature = "platform-accessibility")]
        let attributes = if self.accessibility_activation.is_enabled() {
            attributes.with_visible(false)
        } else {
            attributes
        };

        let platform_window = event_loop.create_window(attributes).map_err(|error| {
            AppRunError::runner(format!("failed to create native window: {error}"))
        })?;
        let platform_window = Arc::new(platform_window);
        // Retain every created native target even if a subsequent registration or input operation
        // fails. Its mapping survives until the actual Destroyed callback on this pump.
        self.nara_windows_by_winit
            .insert(platform_window.id(), window_id);
        self.platform_windows
            .insert(window_id, platform_window.clone());
        self.backend_windows
            .insert(
                window_id,
                WindowHandleProvider::new(platform_window.clone()),
            )
            .map_err(|_| AppRunError::runner("native window target is already registered"))?;
        self.owned_window_ids.insert(window_id);
        #[cfg(feature = "platform-accessibility")]
        if self.accessibility_activation.is_enabled() {
            let event_proxy = self
                .event_proxy
                .as_ref()
                .cloned()
                .ok_or_else(|| AppRunError::runner("winit event proxy is unavailable"))?;
            let session = WinitPlatformAccessibilitySession::new(
                event_loop,
                &platform_window,
                window_id,
                event_proxy,
            );
            self.accessibility_sessions.insert(window_id, session);
            platform_window.set_visible(true);
        }
        self.connect_window_input(window_id)?;
        self.with_driver_scope(|scope| {
            push_window_driver_event(scope, WindowEvent::Created { window_id })
        })?;

        Ok(())
    }

    fn connect_window_input(&mut self, window_id: WindowId) -> Result<(), AppRunError> {
        if !self.input_timeline_installed() {
            return Ok(());
        }
        // The synchronous native owner connects once, before registered callbacks can run.
        // Shutdown retires this endpoint and never recreates a window in the same session.
        let window = InputWindowId::new(window_id.get());
        self.with_driver_scope(|scope| {
            connect_input_endpoint(scope, window, InputSeatId::PRIMARY)?;
            connect_input_pointer(scope, window, InputSeatId::PRIMARY, InputPointerId::PRIMARY)
        })
    }

    fn run_frame(&mut self, delta: Duration, event_loop: &ActiveEventLoop) {
        let directive = self.run_frame_transition(delta);
        apply_event_loop_directive(event_loop, directive);
    }

    fn run_frame_transition(&mut self, delta: Duration) -> EventLoopDirective {
        self.frame_requested = false;
        let outcome = self.runtime.drive(delta);
        let now = Instant::now();
        // Return original presentation/UI facts before interpreting success, failure or exit.
        let presentation_shutdown = self.maintain_presentation(now);
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                self.record_primary_failure(runtime_drive_error(error));
                return self.begin_shutdown_transition(now);
            }
        };

        if presentation_shutdown {
            return self.begin_shutdown_transition(now);
        }

        if let Some(exit) = outcome.frame().and_then(|frame| frame.exit) {
            self.exit = exit;
            return self.begin_shutdown_transition(now);
        }

        if matches!(
            outcome.state(),
            RuntimeState::Stopping | RuntimeState::CloseIncomplete | RuntimeState::Stopped
        ) {
            return self.begin_shutdown_transition(now);
        }

        #[cfg(feature = "platform-accessibility")]
        if let Err(error) = self.synchronize_platform_accessibility() {
            self.record_primary_failure(error);
            return self.begin_shutdown_transition(now);
        }

        if self.control_flow == WinitControlFlow::Poll {
            for window in self.platform_windows.values() {
                window.request_redraw();
            }
        }
        self.running_directive()
    }

    fn maintain_transition(&mut self, now: Instant) -> EventLoopDirective {
        let shutdown_requested = self.maintain_presentation(now);
        if self.runtime.state() == RuntimeState::Faulted
            && let Some(fault) = self.runtime.fault()
        {
            self.record_primary_failure(runtime_fault_error(fault));
        }
        if shutdown_requested
            || !matches!(
                self.runtime.state(),
                RuntimeState::Running | RuntimeState::Paused
            )
        {
            self.begin_shutdown_transition(now)
        } else {
            self.running_directive()
        }
    }

    fn maintain_presentation(&mut self, now: Instant) -> bool {
        self.boundary_serviced = true;
        match self
            .presentation
            .maintain(&mut self.runtime, &self.backend_windows, now)
        {
            Ok(progress) => {
                self.presentation_next_wake = progress.next_wake;
                if let Some(exit) = progress.exit {
                    self.exit = exit;
                    return true;
                }
                false
            }
            Err(error) => {
                self.presentation_failure.get_or_insert(error);
                true
            }
        }
    }

    fn running_directive(&self) -> EventLoopDirective {
        match (self.control_flow, self.presentation_next_wake) {
            (WinitControlFlow::Poll, _) => EventLoopDirective::Poll,
            (WinitControlFlow::Wait, Some(wake)) => EventLoopDirective::WaitUntil(wake),
            (WinitControlFlow::Wait, None) => EventLoopDirective::Wait,
        }
    }

    #[cfg(feature = "platform-accessibility")]
    fn synchronize_platform_accessibility(&mut self) -> Result<(), AppRunError> {
        if !self.accessibility_activation.is_enabled() {
            return Ok(());
        }
        let outcome = self
            .runtime
            .with_driver_scope(accessibility::__private::take_winit_accessibility_updates)
            .map_err(runtime_scope_error)?;
        let updates = outcome
            .output
            .map_err(|_| AppRunError::runner("runtime accessibility extraction failed"))?;
        for update in updates {
            let window_id = update.window_id();
            let session = self
                .accessibility_sessions
                .get_mut(&window_id)
                .ok_or_else(|| {
                    AppRunError::runner(format!(
                        "native accessibility update targets unavailable window {}",
                        window_id.get()
                    ))
                })?;
            session.apply_update(update).map_err(|error| {
                AppRunError::runner(format!(
                    "failed to publish native accessibility state: {error}"
                ))
            })?;
        }
        if let Some(fault) = outcome.post_operation_fault {
            return Err(runtime_fault_error(&fault));
        }
        Ok(())
    }

    #[cfg(feature = "platform-accessibility")]
    fn process_accessibility_window_event(
        &mut self,
        winit_window_id: WinitWindowId,
        event: &WinitWindowEvent,
    ) {
        if !self.accessibility_activation.is_enabled() {
            return;
        }
        let Some(window_id) = self.nara_windows_by_winit.get(&winit_window_id).copied() else {
            return;
        };
        let Some(window) = self.platform_windows.get(&window_id) else {
            return;
        };
        let Some(session) = self.accessibility_sessions.get_mut(&window_id) else {
            return;
        };
        session.process_window_event(window, event);
    }

    #[cfg(feature = "platform-accessibility")]
    fn handle_accessibility_wake(&mut self, window_id: WindowId) -> Result<(), AppRunError> {
        if !self.accessibility_activation.is_enabled() {
            return Ok(());
        }
        let Some(session) = self.accessibility_sessions.get(&window_id) else {
            return Ok(());
        };
        let batch = session.drain_inputs().map_err(|error| {
            AppRunError::runner(format!(
                "failed to drain native accessibility input: {error}"
            ))
        })?;
        self.with_driver_scope(|scope| {
            accessibility::__private::queue_winit_accessibility_inputs(scope, batch)
        })
    }

    fn handle_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        winit_window_id: WinitWindowId,
        event: WinitWindowEvent,
    ) -> Result<(), AppRunError> {
        #[cfg(feature = "platform-accessibility")]
        self.process_accessibility_window_event(winit_window_id, &event);
        if matches!(&event, WinitWindowEvent::Destroyed) {
            let directive = self.handle_destroyed_window(winit_window_id, Instant::now())?;
            apply_event_loop_directive(event_loop, directive);
            return Ok(());
        }
        let Some(window_id) = self.nara_windows_by_winit.get(&winit_window_id).copied() else {
            return Ok(());
        };
        if !should_process_window_event_during_shutdown(self.shutdown, &event) {
            return Ok(());
        }
        self.frame_requested |= window_event_requests_frame(&event);
        self.input_window_id = window_id;

        match event {
            WinitWindowEvent::CloseRequested => {
                if self.input_timeline_installed() {
                    self.with_driver_scope(|scope| {
                        apply_window_input_driver_event(
                            scope,
                            InputWindowId::new(window_id.get()),
                            InputSeatId::PRIMARY,
                            InputWindowEvent::CloseRequested,
                        )
                        .map(|_| ())
                    })?;
                }
                self.with_driver_scope(|scope| {
                    push_window_driver_event(scope, WindowEvent::CloseRequested { window_id })
                })?;
            }
            WinitWindowEvent::Resized(size) => {
                let scale_factor = self
                    .platform_windows
                    .get(&window_id)
                    .map_or(1.0, |window| window.scale_factor());
                let resolution =
                    WindowResolution::new(size.width, size.height).with_scale_factor(scale_factor);
                let logical_size = nara_core::Vec2::new(
                    size.width as f32 / scale_factor as f32,
                    size.height as f32 / scale_factor as f32,
                );
                if self.input_timeline_installed() {
                    self.with_driver_scope(|scope| {
                        apply_window_input_driver_event(
                            scope,
                            InputWindowId::new(window_id.get()),
                            InputSeatId::PRIMARY,
                            InputWindowEvent::Resized { logical_size },
                        )
                        .map(|_| ())
                    })?;
                }
                self.with_driver_scope(|scope| {
                    push_window_driver_event(
                        scope,
                        WindowEvent::Resized {
                            window_id,
                            resolution,
                        },
                    )
                })?;
            }
            WinitWindowEvent::Focused(focused) => {
                self.apply_focus_driver_event(window_id, focused)?;
            }
            WinitWindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if self.input_timeline_installed() {
                    self.with_driver_scope(|scope| {
                        apply_window_input_driver_event(
                            scope,
                            InputWindowId::new(window_id.get()),
                            InputSeatId::PRIMARY,
                            InputWindowEvent::ScaleFactorChanged { scale_factor },
                        )
                        .map(|_| ())
                    })?;
                }
                self.with_driver_scope(|scope| {
                    push_window_driver_event(
                        scope,
                        WindowEvent::ScaleFactorChanged {
                            window_id,
                            scale_factor,
                        },
                    )
                })?;
            }
            WinitWindowEvent::RedrawRequested => {
                self.with_driver_scope(|scope| {
                    push_window_driver_event(scope, WindowEvent::RedrawRequested { window_id })
                })?;
            }
            WinitWindowEvent::KeyboardInput { event, .. } => {
                self.apply_physical_keyboard_driver_event(
                    RawKeyEvent {
                        physical_key: event.physical_key,
                        state: event.state,
                    },
                    event.repeat,
                )?;
            }
            WinitWindowEvent::MouseInput { state, button, .. } => {
                let button = convert_mouse_button(button);
                if self.input_timeline_installed()
                    && let Some(input) = self.mouse_driver_input(state, button)
                {
                    self.with_driver_scope(|scope| {
                        apply_mouse_driver_input_at(
                            scope,
                            InputWindowId::new(window_id.get()),
                            InputSeatId::PRIMARY,
                            InputPointerId::PRIMARY,
                            input,
                        )
                        .map(|_| ())
                    })?;
                }
            }
            WinitWindowEvent::CursorMoved { position, .. } => {
                if !self.input_timeline_installed() {
                    return Ok(());
                }
                let scale_factor = self
                    .platform_windows
                    .get(&window_id)
                    .map_or(1.0, |window| window.scale_factor());
                let position = nara_core::Vec2::new(
                    position.x as f32 / scale_factor as f32,
                    position.y as f32 / scale_factor as f32,
                );
                self.with_driver_scope(|scope| {
                    apply_pointer_driver_input_at(
                        scope,
                        InputWindowId::new(window_id.get()),
                        InputSeatId::PRIMARY,
                        InputPointerId::PRIMARY,
                        PointerDriverInput::Moved(position),
                    )
                    .map(|_| ())
                })?;
            }
            WinitWindowEvent::CursorLeft { .. } if self.input_timeline_installed() => {
                let outcome = self
                    .runtime
                    .with_driver_scope(|scope| {
                        apply_pointer_driver_input_at(
                            scope,
                            InputWindowId::new(window_id.get()),
                            InputSeatId::PRIMARY,
                            InputPointerId::PRIMARY,
                            PointerDriverInput::Left,
                        )
                    })
                    .map_err(runtime_scope_error)?;
                let (_, released_mouse_buttons) = outcome
                    .output
                    .map_err(|_| AppRunError::runner("runtime pointer release failed"))?;
                self.input_gate
                    .suppress_mouse_buttons(released_mouse_buttons);
                if let Some(fault) = outcome.post_operation_fault {
                    return Err(runtime_fault_error(&fault));
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn apply_physical_keyboard_driver_event(
        &mut self,
        event: RawKeyEvent,
        repeat: bool,
    ) -> Result<bool, AppRunError> {
        if !self.input_timeline_installed() {
            return Ok(false);
        }
        let Some(key) = convert_physical_key(event.physical_key) else {
            return Ok(false);
        };
        let Some(input) = self.input_gate.keyboard_input(key, event.state, repeat) else {
            return Ok(false);
        };
        let input_window_id = InputWindowId::new(self.input_window_id.get());
        self.with_driver_scope(|scope| {
            apply_keyboard_driver_input_at(scope, input_window_id, InputSeatId::PRIMARY, input)
                .map(|_| ())
        })?;
        Ok(true)
    }

    fn apply_focus_driver_event(
        &mut self,
        window_id: WindowId,
        focused: bool,
    ) -> Result<(), AppRunError> {
        if !self.input_timeline_installed() {
            if focused {
                self.input_gate.gain_focus();
            } else {
                self.input_gate.lose_focus([], []);
            }
            return self.with_driver_scope(|scope| {
                push_window_driver_event(scope, WindowEvent::Focused { window_id, focused })
            });
        }
        if focused {
            self.input_gate.gain_focus();
        } else {
            let outcome = self
                .runtime
                .with_driver_scope(|scope| {
                    apply_window_input_driver_event(
                        scope,
                        InputWindowId::new(window_id.get()),
                        InputSeatId::PRIMARY,
                        InputWindowEvent::Focused(false),
                    )
                })
                .map_err(runtime_scope_error)?;
            let (_, released_keys, released_mouse_buttons) = outcome
                .output
                .map_err(|_| AppRunError::runner("runtime focus release failed"))?;
            self.input_gate
                .lose_focus(released_keys, released_mouse_buttons);
            if let Some(fault) = outcome.post_operation_fault {
                return Err(runtime_fault_error(&fault));
            }
        }
        if focused {
            self.with_driver_scope(|scope| {
                apply_window_input_driver_event(
                    scope,
                    InputWindowId::new(window_id.get()),
                    InputSeatId::PRIMARY,
                    InputWindowEvent::Focused(true),
                )
                .map(|_| ())
            })?;
        }
        self.with_driver_scope(|scope| {
            push_window_driver_event(scope, WindowEvent::Focused { window_id, focused })
        })
    }

    fn mouse_driver_input(
        &mut self,
        state: ElementState,
        button: MouseButton,
    ) -> Option<ButtonDriverInput<MouseButton>> {
        self.input_gate.mouse_input(button, state)
    }

    fn input_timeline_installed(&self) -> bool {
        self.runtime.world().contains_resource::<InputTimeline>()
    }

    fn handle_destroyed_window(
        &mut self,
        winit_window_id: WinitWindowId,
        now: Instant,
    ) -> Result<EventLoopDirective, AppRunError> {
        let Some(window_id) = self.nara_windows_by_winit.get(&winit_window_id).copied() else {
            return Ok(EventLoopDirective::None);
        };
        let registered = self.owned_window_ids.contains(&window_id);
        let externally_destroyed = if registered {
            self.backend_windows
                .snapshot(window_id)
                .map_err(|_| AppRunError::runner("native window target is not registered"))?
                .phase
                != WindowTargetPhase::ProviderReleased
        } else {
            true
        };
        if registered {
            self.backend_windows
                .mark_native_destroyed(window_id)
                .map_err(|_| AppRunError::runner("native window target is not registered"))?;
        }
        if externally_destroyed {
            let error =
                AppRunError::runner("native window was destroyed before controlled retirement");
            if self.shutdown.is_started() {
                self.record_native_retirement_failure(error);
            } else {
                self.record_primary_failure(error);
            }
        }
        if externally_destroyed && !self.shutdown.is_started() {
            if self.input_timeline_installed() {
                if let Err(error) =
                    self.with_driver_scope(|scope| retire_window_input(scope, window_id))
                {
                    self.record_primary_failure(error);
                }
            }
            if let Err(error) = self.with_driver_scope(|scope| {
                push_window_driver_event(scope, WindowEvent::Closed { window_id })
            }) {
                self.record_primary_failure(error);
            }
        }
        self.platform_windows.remove(&window_id);
        self.nara_windows_by_winit.remove(&winit_window_id);
        #[cfg(feature = "platform-accessibility")]
        self.accessibility_sessions.remove(&window_id);

        if self.shutdown.is_started() {
            Ok(self.poll_native_shutdown_transition(now))
        } else {
            Ok(self.begin_shutdown_transition(now))
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: AppRunError) {
        self.record_primary_failure(error);
        self.begin_shutdown(event_loop);
    }

    fn record_primary_failure(&mut self, error: AppRunError) {
        self.primary_failure.get_or_insert(error);
    }

    fn record_runtime_close_failure(&mut self, error: AppRunError) {
        self.runtime_close_failure.get_or_insert(error);
    }

    fn record_native_retirement_failure(&mut self, error: AppRunError) {
        self.native_retirement_failure.get_or_insert(error);
    }

    fn failure(&self) -> Option<AppRunError> {
        let terminal_close_failure = self
            .runtime_close_result()
            .err()
            .filter(|error| self.runtime_close_failure.as_ref() != Some(error));
        [
            self.primary_failure.clone(),
            self.presentation_failure.clone(),
            self.runtime_close_failure.clone(),
            terminal_close_failure,
            self.native_retirement_failure.clone(),
        ]
        .into_iter()
        .flatten()
        .reduce(AppRunError::runner_teardown)
    }

    fn begin_shutdown(&mut self, event_loop: &ActiveEventLoop) {
        let directive = self.begin_shutdown_transition(Instant::now());
        apply_event_loop_directive(event_loop, directive);
    }

    fn begin_shutdown_transition(&mut self, now: Instant) -> EventLoopDirective {
        match self.shutdown {
            WinitShutdownState::Complete => return EventLoopDirective::Exit,
            WinitShutdownState::Retiring => return self.poll_native_shutdown_transition(now),
            WinitShutdownState::Running => {
                self.shutdown = WinitShutdownState::Retiring;
                self.retirement_deadline = Some(
                    now.checked_add(self.native_retirement_budget)
                        .unwrap_or(now),
                );
                let window_ids = self.owned_window_ids.iter().copied().collect::<Vec<_>>();
                if self
                    .backend_windows
                    .request_retirements(&window_ids)
                    .is_err()
                {
                    self.record_native_retirement_failure(AppRunError::runner(
                        "owned native window target is not registered",
                    ));
                }
                if let Err(error) = retire_runtime_input(&mut self.runtime, &window_ids) {
                    self.record_primary_failure(error);
                }
                if let Err(error) = self.begin_runtime_close() {
                    self.record_runtime_close_failure(error);
                }
            }
        }
        self.poll_native_shutdown_once(now)
    }

    fn prepare_retirement_retry(&mut self, now: Instant) {
        if self.shutdown == WinitShutdownState::Running {
            self.begin_shutdown_transition(now);
        }
        self.retirement_deadline = Some(
            now.checked_add(self.native_retirement_budget)
                .unwrap_or(now),
        );
        if self.runtime.state() == RuntimeState::CloseIncomplete {
            match self.runtime.request_control(RuntimeControl::RetryClose) {
                RuntimeControlRequestResult::Accepted(_) => {
                    self.runtime_close_incomplete_observed = false;
                }
                RuntimeControlRequestResult::Rejected(_) => self.record_runtime_close_failure(
                    AppRunError::runner("managed runtime rejected explicit close retry"),
                ),
            }
        }
    }

    fn poll_native_shutdown(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        let directive = self.poll_native_shutdown_once(now);
        apply_event_loop_directive(event_loop, directive);
    }

    fn poll_native_shutdown_once(&mut self, now: Instant) -> EventLoopDirective {
        if self.shutdown == WinitShutdownState::Complete {
            return EventLoopDirective::Exit;
        }
        if self.shutdown == WinitShutdownState::Running {
            return EventLoopDirective::None;
        }
        if let Err(error) = self.poll_runtime_close(now) {
            self.record_runtime_close_failure(error);
        }
        if !self.presentation_retired {
            let window_ids = self.owned_window_ids.iter().copied().collect::<Vec<_>>();
            match self.presentation.retire(
                &mut self.runtime,
                &self.backend_windows,
                &window_ids,
                now,
            ) {
                Ok(retired) => self.presentation_retired = retired,
                Err(error) => self.record_native_retirement_failure(error),
            }
        }
        if self.presentation_retired && !self.providers_released {
            self.release_native_providers();
        }
        self.poll_native_shutdown_transition(now)
    }

    fn release_native_providers(&mut self) {
        let mut released = true;
        for window_id in self.owned_window_ids.iter().copied().collect::<Vec<_>>() {
            if self.backend_windows.release_provider(window_id).is_ok() {
                #[cfg(feature = "platform-accessibility")]
                self.accessibility_sessions.remove(&window_id);
                // On Windows the last Arc Drop posts a destruction message. The Winit mapping
                // deliberately survives this point until that message has actually been pumped.
                self.platform_windows.remove(&window_id);
            } else {
                released = false;
                self.record_native_retirement_failure(AppRunError::runner(
                    "presentation driver acknowledged retirement before surface owner release",
                ));
            }
        }
        // A native creation followed by registration refusal still belongs to this session, but
        // must never retire or acknowledge another adapter's registry entry.
        self.platform_windows
            .retain(|window_id, _| self.owned_window_ids.contains(window_id));
        self.providers_released = released;
    }

    fn poll_native_shutdown_transition(&mut self, now: Instant) -> EventLoopDirective {
        if self.shutdown == WinitShutdownState::Complete {
            return EventLoopDirective::Exit;
        }
        if self.shutdown == WinitShutdownState::Running {
            return EventLoopDirective::None;
        }
        let native_destroyed = self
            .backend_windows
            .all_native_destroyed(self.owned_window_ids.iter().copied());
        match native_destroyed {
            Ok(true)
                if self.nara_windows_by_winit.is_empty()
                    && self.presentation_retired
                    && self.providers_released
                    && self.runtime.state() == RuntimeState::Stopped =>
            {
                if let Err(error) = self.runtime_close_result() {
                    self.record_runtime_close_failure(error);
                }
                self.shutdown = WinitShutdownState::Complete;
                return EventLoopDirective::Exit;
            }
            Err(_) => self.record_native_retirement_failure(AppRunError::runner(
                "native window target disappeared during shutdown",
            )),
            Ok(_) => {}
        }
        EventLoopDirective::WaitUntil(self.retirement_wake(now))
    }

    fn retirement_wake(&self, now: Instant) -> Instant {
        let mut wake = now.checked_add(RUNTIME_CLOSE_POLL_INTERVAL).unwrap_or(now);
        if let Some(deadline) = self.retirement_deadline {
            wake = wake.min(deadline);
        }
        if let Some(presentation_wake) = self.presentation_next_wake {
            wake = wake.min(presentation_wake);
        }
        wake
    }

    fn pump_timeout(&self, now: Instant) -> Option<Duration> {
        if self.shutdown.is_started() {
            Some(self.retirement_wake(now).saturating_duration_since(now))
        } else {
            match (self.control_flow, self.presentation_next_wake) {
                (WinitControlFlow::Poll, _) => Some(Duration::ZERO),
                (WinitControlFlow::Wait, Some(wake)) => Some(wake.saturating_duration_since(now)),
                (WinitControlFlow::Wait, None) => None,
            }
        }
    }

    fn retirement_drive_incomplete(&mut self, now: Instant) -> bool {
        if self.shutdown != WinitShutdownState::Retiring {
            return false;
        }
        let expired = self
            .retirement_deadline
            .is_some_and(|deadline| now >= deadline);
        if expired {
            self.record_native_retirement_failure(AppRunError::runner(
                "native session retirement budget expired",
            ));
        }
        let close_incomplete = self.runtime.state() == RuntimeState::CloseIncomplete;
        if close_incomplete {
            self.record_runtime_close_incomplete();
        }
        expired || close_incomplete
    }

    fn begin_runtime_close(&mut self) -> Result<(), AppRunError> {
        if self.runtime.state() == RuntimeState::CloseIncomplete {
            self.record_runtime_close_incomplete();
            return Ok(());
        }
        if self.runtime.state() == RuntimeState::Stopping {
            return Ok(());
        }
        match self.runtime.request_control(RuntimeControl::Stop) {
            RuntimeControlRequestResult::Accepted(_) => Ok(()),
            RuntimeControlRequestResult::Rejected(_)
                if self.runtime.state() == RuntimeState::Stopped =>
            {
                self.runtime_close_result()
            }
            RuntimeControlRequestResult::Rejected(_) => Err(AppRunError::runner(
                "managed runtime rejected platform shutdown",
            )),
        }
    }

    fn poll_runtime_close(&mut self, now: Instant) -> Result<(), AppRunError> {
        if matches!(
            self.runtime.state(),
            RuntimeState::Stopping
                | RuntimeState::Running
                | RuntimeState::Paused
                | RuntimeState::Faulted
                | RuntimeState::CloseIncomplete
        ) {
            // CloseIncomplete is driven only after an explicit RetryClose request. Without that
            // request it remains observational; maintenance still runs through the local driver.
            if self.runtime.state() != RuntimeState::CloseIncomplete
                || !self.runtime_close_incomplete_observed
            {
                let outcome = self.runtime.drive(Duration::ZERO);
                self.maintain_presentation(Instant::now());
                outcome.map_err(runtime_drive_error)?;
            } else {
                self.maintain_presentation(now);
            }
        } else {
            self.maintain_presentation(now);
        }
        if self.runtime.state() == RuntimeState::CloseIncomplete {
            self.record_runtime_close_incomplete();
            return Ok(());
        }
        match self.runtime.state() {
            RuntimeState::Stopped => self.runtime_close_result(),
            RuntimeState::Stopping => Ok(()),
            _ => Err(AppRunError::runner(
                "managed runtime did not enter platform shutdown",
            )),
        }
    }

    fn runtime_close_result(&self) -> Result<(), AppRunError> {
        if self.runtime.close_evidence().has_terminal_failure() {
            Err(AppRunError::runner("managed runtime close failed"))
        } else {
            Ok(())
        }
    }

    fn record_runtime_close_incomplete(&mut self) {
        if self.runtime_close_incomplete_observed {
            return;
        }
        self.runtime_close_incomplete_observed = true;
        self.record_runtime_close_failure(AppRunError::runner(
            "managed runtime close is incomplete",
        ));
    }
}

fn apply_event_loop_directive(event_loop: &ActiveEventLoop, directive: EventLoopDirective) {
    match directive {
        EventLoopDirective::None => {}
        EventLoopDirective::Poll => event_loop.set_control_flow(ControlFlow::Poll),
        EventLoopDirective::Wait => event_loop.set_control_flow(ControlFlow::Wait),
        EventLoopDirective::WaitUntil(deadline) => {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
        EventLoopDirective::Exit => event_loop.exit(),
    }
}

impl ApplicationHandler<WinitUserEvent> for WinitApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.shutdown.is_started() {
            self.frame_requested = true;
        }
        if let Err(error) = self.create_primary_window(event_loop) {
            self.fail(event_loop, error);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WinitWindowId,
        event: WinitWindowEvent,
    ) {
        if let Err(error) = self.handle_window_event(event_loop, window_id, event) {
            self.fail(event_loop, error);
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: WinitUserEvent) {
        match event {
            #[cfg(feature = "platform-accessibility")]
            WinitUserEvent::AccessibilityWake(window_id) => {
                if !self.shutdown.is_started() {
                    self.frame_requested = true;
                }
                if let Err(error) = self.handle_accessibility_wake(window_id) {
                    self.fail(_event_loop, error);
                }
            }
            #[cfg(all(test, target_os = "windows"))]
            WinitUserEvent::NativeTest(event) => {
                native_tests::handle_event(self, _event_loop, event)
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.shutdown.is_started() {
            self.poll_native_shutdown(event_loop, Instant::now());
            return;
        }
        let now = Instant::now();
        if self.control_flow == WinitControlFlow::Wait && !self.frame_requested {
            let directive = self.maintain_transition(now);
            apply_event_loop_directive(event_loop, directive);
            return;
        }
        let delta = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        self.run_frame(delta, event_loop);
    }
}

fn window_event_requests_frame(event: &WinitWindowEvent) -> bool {
    matches!(
        event,
        WinitWindowEvent::CloseRequested
            | WinitWindowEvent::Resized(_)
            | WinitWindowEvent::Focused(_)
            | WinitWindowEvent::ScaleFactorChanged { .. }
            | WinitWindowEvent::RedrawRequested
            | WinitWindowEvent::KeyboardInput { .. }
            | WinitWindowEvent::MouseInput { .. }
            | WinitWindowEvent::CursorMoved { .. }
            | WinitWindowEvent::CursorLeft { .. }
    )
}

fn should_process_window_event_during_shutdown(
    shutdown: WinitShutdownState,
    event: &WinitWindowEvent,
) -> bool {
    !shutdown.is_started() || matches!(event, WinitWindowEvent::Destroyed)
}

fn retire_window_input(
    scope: &mut RuntimeDriverScope<'_>,
    window_id: WindowId,
) -> Result<(), ButtonInputDriverError> {
    match apply_window_input_driver_event(
        scope,
        InputWindowId::new(window_id.get()),
        InputSeatId::PRIMARY,
        InputWindowEvent::SurfaceRetired,
    ) {
        // A failed connection or an already reclaimed retirement has no accepted input to end.
        // Neither may prevent the native owner from releasing its remaining physical resources.
        Ok(_)
        | Err(ButtonInputDriverError::Timeline(InputIngressError::UnknownEndpoint { .. })) => {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn retire_runtime_input(
    runtime: &mut RuntimeInstance,
    owned_window_ids: &[WindowId],
) -> Result<(), AppRunError> {
    if runtime.state() != RuntimeState::Stopped
        && runtime.world().contains_resource::<InputTimeline>()
    {
        let outcome = runtime
            .with_driver_scope(|scope| {
                for window_id in owned_window_ids.iter().copied() {
                    retire_window_input(scope, window_id)?;
                }
                Ok::<(), ButtonInputDriverError>(())
            })
            .map_err(runtime_scope_error)?;
        outcome
            .output
            .map_err(|_| AppRunError::runner("input route retirement failed"))?;
        if let Some(fault) = outcome.post_operation_fault {
            return Err(runtime_fault_error(&fault));
        }
    }
    Ok(())
}

fn configured_primary_window(runtime: &RuntimeInstance) -> Result<Window, AppRunError> {
    let mut windows = runtime
        .world()
        .iter_entities()
        .filter_map(|entity| entity.get::<Window>().cloned());
    let Some(window) = windows.next() else {
        return Err(AppRunError::runner(
            "managed runtime has no configured window target",
        ));
    };
    if windows.next().is_some() {
        return Err(AppRunError::runner(
            "managed runtime has more than one configured window target",
        ));
    }
    Ok(window)
}

// This convenience is limited to copyable driver facts. Owned extraction and retirement above
// reconcile their outputs explicitly before returning a post-operation fault.
fn with_runtime_driver_scope<R: Copy, E>(
    runtime: &mut RuntimeInstance,
    operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> Result<R, E>,
) -> Result<R, AppRunError> {
    let outcome = runtime
        .with_driver_scope(operation)
        .map_err(runtime_scope_error)?;
    let output = outcome
        .output
        .map_err(|_| AppRunError::runner("runtime driver operation failed"))?;
    if let Some(fault) = outcome.post_operation_fault {
        return Err(runtime_fault_error(&fault));
    }
    Ok(output)
}

fn runtime_drive_error(error: RuntimeDriveError) -> AppRunError {
    runtime_fault_error(error.fault())
}

fn runtime_scope_error(error: RuntimeScopeError) -> AppRunError {
    match error {
        RuntimeScopeError::Unavailable { .. } => {
            AppRunError::runner("runtime driver scope is unavailable")
        }
        RuntimeScopeError::Faulted { fault } => runtime_fault_error(&fault),
    }
}

fn runtime_fault_error(fault: &RuntimeFault) -> AppRunError {
    AppRunError::managed_runtime_fault(fault)
}

#[must_use]
pub fn convert_physical_key(key: PhysicalKey) -> Option<KeyCode> {
    match key {
        PhysicalKey::Code(code) => convert_key_code(code),
        PhysicalKey::Unidentified(_) => None,
    }
}

#[must_use]
pub fn convert_key_code(code: WinitKeyCode) -> Option<KeyCode> {
    let key = match code {
        WinitKeyCode::Escape => KeyCode::Escape,
        WinitKeyCode::Space => KeyCode::Space,
        WinitKeyCode::Enter => KeyCode::Enter,
        WinitKeyCode::ArrowUp => KeyCode::ArrowUp,
        WinitKeyCode::ArrowDown => KeyCode::ArrowDown,
        WinitKeyCode::ArrowLeft => KeyCode::ArrowLeft,
        WinitKeyCode::ArrowRight => KeyCode::ArrowRight,
        WinitKeyCode::KeyA => KeyCode::Character('a'),
        WinitKeyCode::KeyB => KeyCode::Character('b'),
        WinitKeyCode::KeyC => KeyCode::Character('c'),
        WinitKeyCode::KeyD => KeyCode::Character('d'),
        WinitKeyCode::KeyE => KeyCode::Character('e'),
        WinitKeyCode::KeyF => KeyCode::Character('f'),
        WinitKeyCode::KeyG => KeyCode::Character('g'),
        WinitKeyCode::KeyH => KeyCode::Character('h'),
        WinitKeyCode::KeyI => KeyCode::Character('i'),
        WinitKeyCode::KeyJ => KeyCode::Character('j'),
        WinitKeyCode::KeyK => KeyCode::Character('k'),
        WinitKeyCode::KeyL => KeyCode::Character('l'),
        WinitKeyCode::KeyM => KeyCode::Character('m'),
        WinitKeyCode::KeyN => KeyCode::Character('n'),
        WinitKeyCode::KeyO => KeyCode::Character('o'),
        WinitKeyCode::KeyP => KeyCode::Character('p'),
        WinitKeyCode::KeyQ => KeyCode::Character('q'),
        WinitKeyCode::KeyR => KeyCode::Character('r'),
        WinitKeyCode::KeyS => KeyCode::Character('s'),
        WinitKeyCode::KeyT => KeyCode::Character('t'),
        WinitKeyCode::KeyU => KeyCode::Character('u'),
        WinitKeyCode::KeyV => KeyCode::Character('v'),
        WinitKeyCode::KeyW => KeyCode::Character('w'),
        WinitKeyCode::KeyX => KeyCode::Character('x'),
        WinitKeyCode::KeyY => KeyCode::Character('y'),
        WinitKeyCode::KeyZ => KeyCode::Character('z'),
        WinitKeyCode::Digit0 => KeyCode::Character('0'),
        WinitKeyCode::Digit1 => KeyCode::Character('1'),
        WinitKeyCode::Digit2 => KeyCode::Character('2'),
        WinitKeyCode::Digit3 => KeyCode::Character('3'),
        WinitKeyCode::Digit4 => KeyCode::Character('4'),
        WinitKeyCode::Digit5 => KeyCode::Character('5'),
        WinitKeyCode::Digit6 => KeyCode::Character('6'),
        WinitKeyCode::Digit7 => KeyCode::Character('7'),
        WinitKeyCode::Digit8 => KeyCode::Character('8'),
        WinitKeyCode::Digit9 => KeyCode::Character('9'),
        _ => return None,
    };

    Some(key)
}

#[must_use]
pub fn convert_mouse_button(button: WinitMouseButton) -> MouseButton {
    match button {
        WinitMouseButton::Left => MouseButton::Left,
        WinitMouseButton::Right => MouseButton::Right,
        WinitMouseButton::Middle => MouseButton::Middle,
        WinitMouseButton::Back => MouseButton::Other(4),
        WinitMouseButton::Forward => MouseButton::Other(5),
        WinitMouseButton::Other(button) => MouseButton::Other(button),
    }
}

#[cfg(test)]
mod tests;
