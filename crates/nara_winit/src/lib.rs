//! Winit runner and native window adapter for nara.

#[cfg(feature = "accessibility")]
mod accessibility;
mod window_mode;

#[cfg(feature = "accessibility")]
pub use accessibility::{
    WinitAccessibilityActionRequest, WinitAccessibilityEndpointRetirement, WinitAccessibilityError,
    WinitAccessibilityPublisher, WinitAccessibilityPublisherId, WinitAccessibilityReceipt,
    WinitAccessibilityRetirementReceipt, WinitAccessibilitySuspension,
    WinitAccessibilitySuspensionReceipt, WinitAccessibilityUpdate,
};

#[cfg(feature = "platform-accessibility")]
#[doc(hidden)]
pub mod __private {
    pub use super::accessibility::__private::*;
}

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
    ButtonDriverInput, ButtonInputDriverError, InputPointerId, InputSeatId, InputTimeline,
    InputWindowEvent, InputWindowId, KeyCode, MouseButton, PointerDriverInput,
    apply_keyboard_driver_input_at, apply_mouse_driver_input_at, apply_pointer_driver_input_at,
    apply_window_input_driver_event,
};
use nara_window::{
    Window, WindowEvent, WindowId, WindowResolution,
    backend::{BackendWindowHandles, WindowHandleProvider, WindowSurfaceRetirementDriver},
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

const NATIVE_DESTROY_TIMEOUT: Duration = Duration::from_secs(5);
const RUNTIME_CLOSE_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeShutdownAction {
    WaitUntil(Instant),
    Complete,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventLoopDirective {
    None,
    WaitUntil(Instant),
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WinitShutdownState {
    Running,
    WaitingForNative { deadline: Instant },
    Complete,
    Aborted,
}

#[derive(Debug)]
enum WinitUserEvent {
    #[cfg(feature = "platform-accessibility")]
    AccessibilityWake(WindowId),
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

    /// Drives one managed runtime from the native event loop.
    pub fn run(self, runtime: &mut RuntimeInstance) -> Result<AppExit, AppRunError> {
        let event_loop = EventLoop::<WinitUserEvent>::with_user_event()
            .build()
            .map_err(|error| {
                AppRunError::runner(format!("failed to create winit event loop: {error}"))
            })?;
        event_loop.set_control_flow(self.control_flow.into_winit());

        let mut state = WinitApp::with_event_proxy(runtime, event_loop.create_proxy())?;
        let run_result = event_loop.run_app(&mut state);
        if let Err(error) = run_result {
            state.record_primary_failure(AppRunError::runner(format!(
                "winit event loop failed: {error}"
            )));
        }
        state.finish_after_event_loop();

        if let Some(error) = state.take_failure() {
            return Err(error);
        }
        if state.runtime.state() != RuntimeState::Stopped {
            return Err(AppRunError::runner(
                "winit event loop ended before managed runtime shutdown completed",
            ));
        }

        Ok(state.exit)
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
    Poll,
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

struct WinitApp<'runtime> {
    runtime: &'runtime mut RuntimeInstance,
    nara_windows_by_winit: HashMap<WinitWindowId, WindowId>,
    platform_windows: HashMap<WindowId, Arc<WinitWindow>>,
    owned_window_ids: BTreeSet<WindowId>,
    last_frame: Instant,
    exit: AppExit,
    primary_failure: Option<AppRunError>,
    runtime_close_failure: Option<AppRunError>,
    native_retirement_failure: Option<AppRunError>,
    backend_windows: BackendWindowHandles,
    shutdown: WinitShutdownState,
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

impl<'runtime> WinitApp<'runtime> {
    #[cfg(test)]
    fn new(runtime: &'runtime mut RuntimeInstance) -> Result<Self, AppRunError> {
        Self::with_session(runtime)
    }

    fn with_session(runtime: &'runtime mut RuntimeInstance) -> Result<Self, AppRunError> {
        #[cfg(feature = "platform-accessibility")]
        let accessibility_activation =
            match accessibility::__private::capture_winit_accessibility_activation(runtime.world())
            {
                accessibility::__private::WinitAccessibilityActivation::Partial(error) => {
                    return Err(AppRunError::runner(format!(
                        "Winit accessibility contribution is partial: {error}"
                    )));
                }
                activation => activation,
            };
        let backend_windows = runtime
            .world()
            .get_resource::<BackendWindowHandles>()
            .cloned()
            .ok_or_else(|| AppRunError::runner("backend window authority is missing"))?;
        Ok(Self {
            runtime,
            nara_windows_by_winit: HashMap::new(),
            platform_windows: HashMap::new(),
            owned_window_ids: BTreeSet::new(),
            last_frame: Instant::now(),
            exit: AppExit::Success,
            primary_failure: None,
            runtime_close_failure: None,
            native_retirement_failure: None,
            backend_windows,
            shutdown: WinitShutdownState::Running,
            runtime_close_incomplete_observed: false,
            input_gate: WinitInputGate::default(),
            input_window_id: WindowId::PRIMARY,
            event_proxy: None,
            #[cfg(feature = "platform-accessibility")]
            accessibility_activation,
            #[cfg(feature = "platform-accessibility")]
            accessibility_sessions: HashMap::new(),
        })
    }

    fn with_event_proxy(
        runtime: &'runtime mut RuntimeInstance,
        event_proxy: EventLoopProxy<WinitUserEvent>,
    ) -> Result<Self, AppRunError> {
        let mut app = Self::with_session(runtime)?;
        app.event_proxy = Some(event_proxy);
        Ok(app)
    }

    fn with_driver_scope<R: Copy, E>(
        &mut self,
        operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> Result<R, E>,
    ) -> Result<R, AppRunError> {
        with_runtime_driver_scope(self.runtime, operation)
    }

    fn create_primary_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppRunError> {
        if self.shutdown.is_started() || !self.platform_windows.is_empty() {
            return Ok(());
        }

        let window = configured_primary_window(self.runtime)?;
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
        let provider = WindowHandleProvider::new(platform_window.clone());

        self.backend_windows
            .insert(window_id, provider)
            .map_err(|_| AppRunError::runner("native window target is already registered"))?;
        self.owned_window_ids.insert(window_id);
        self.nara_windows_by_winit
            .insert(platform_window.id(), window_id);
        self.platform_windows
            .insert(window_id, platform_window.clone());
        self.with_driver_scope(|scope| {
            push_window_driver_event(scope, WindowEvent::Created { window_id })
        })?;

        Ok(())
    }

    fn run_frame(&mut self, delta: Duration, event_loop: &ActiveEventLoop) {
        let outcome = match self.runtime.drive(delta) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.fail(event_loop, runtime_drive_error(error));
                return;
            }
        };

        #[cfg(feature = "platform-accessibility")]
        if let Err(error) = self.synchronize_platform_accessibility() {
            self.fail(event_loop, error);
            return;
        }

        if let Some(exit) = outcome.frame().and_then(|frame| frame.exit) {
            self.exit = exit;
            self.begin_shutdown(event_loop);
            return;
        }

        if matches!(
            outcome.state(),
            RuntimeState::Stopping | RuntimeState::CloseIncomplete | RuntimeState::Stopped
        ) {
            if outcome.state() == RuntimeState::CloseIncomplete {
                self.record_runtime_close_incomplete();
            }
            self.begin_shutdown(event_loop);
            return;
        }

        for window in self.platform_windows.values() {
            window.request_redraw();
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
        let externally_destroyed = !self.shutdown.is_started();
        self.backend_windows
            .mark_native_destroyed(window_id)
            .map_err(|_| AppRunError::runner("native window target is not registered"))?;
        if externally_destroyed {
            if self.input_timeline_installed() {
                self.with_driver_scope(|scope| {
                    apply_window_input_driver_event(
                        scope,
                        InputWindowId::new(window_id.get()),
                        InputSeatId::PRIMARY,
                        InputWindowEvent::SurfaceRetired,
                    )
                    .map(|_| ())
                })?;
            }
            self.with_driver_scope(|scope| {
                push_window_driver_event(scope, WindowEvent::Closed { window_id })
            })?;
        }
        self.platform_windows.remove(&window_id);
        self.nara_windows_by_winit.remove(&winit_window_id);
        #[cfg(feature = "platform-accessibility")]
        self.accessibility_sessions.remove(&window_id);

        if externally_destroyed {
            self.record_primary_failure(AppRunError::runner(
                "native window was destroyed before controlled retirement",
            ));
            Ok(self.begin_shutdown_transition(now))
        } else {
            Ok(self.poll_native_shutdown_transition(now))
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

    fn take_failure(&mut self) -> Option<AppRunError> {
        [
            self.primary_failure.take(),
            self.runtime_close_failure.take(),
            self.native_retirement_failure.take(),
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
            WinitShutdownState::Complete | WinitShutdownState::Aborted => {
                return EventLoopDirective::Exit;
            }
            WinitShutdownState::WaitingForNative { .. } => {}
            WinitShutdownState::Running => {
                match retire_runtime_targets(
                    self.runtime,
                    &self.backend_windows,
                    &self.owned_window_ids,
                ) {
                    Ok(Some(fault)) => self.record_primary_failure(runtime_fault_error(&fault)),
                    Ok(None) => {}
                    Err(error) => {
                        self.record_native_retirement_failure(error);
                        self.shutdown = WinitShutdownState::Aborted;
                        return EventLoopDirective::Exit;
                    }
                }

                #[cfg(feature = "platform-accessibility")]
                self.accessibility_sessions.clear();
                self.platform_windows.clear();
                if let Err(error) = self.begin_runtime_close() {
                    self.record_runtime_close_failure(error);
                }
                self.shutdown = WinitShutdownState::WaitingForNative {
                    deadline: now + NATIVE_DESTROY_TIMEOUT,
                };
            }
        }
        self.poll_native_shutdown_transition(now)
    }

    fn poll_native_shutdown(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        let directive = self.poll_native_shutdown_once(now);
        apply_event_loop_directive(event_loop, directive);
    }

    fn poll_native_shutdown_once(&mut self, now: Instant) -> EventLoopDirective {
        if matches!(
            self.shutdown,
            WinitShutdownState::Complete | WinitShutdownState::Aborted
        ) {
            return EventLoopDirective::Exit;
        }
        if let Err(error) = self.poll_runtime_close() {
            self.record_runtime_close_failure(error);
        }
        self.poll_native_shutdown_transition(now)
    }

    fn poll_native_shutdown_transition(&mut self, now: Instant) -> EventLoopDirective {
        let deadline = match self.shutdown {
            WinitShutdownState::WaitingForNative { deadline } => deadline,
            WinitShutdownState::Complete | WinitShutdownState::Aborted => {
                return EventLoopDirective::Exit;
            }
            WinitShutdownState::Running => return EventLoopDirective::None,
        };
        match native_shutdown_action(&self.backend_windows, &self.owned_window_ids, now, deadline) {
            Ok(NativeShutdownAction::WaitUntil(deadline)) => {
                let runtime_poll = now
                    .checked_add(RUNTIME_CLOSE_POLL_INTERVAL)
                    .unwrap_or(deadline);
                EventLoopDirective::WaitUntil(deadline.min(runtime_poll))
            }
            Ok(NativeShutdownAction::Complete) => match self.runtime.state() {
                RuntimeState::Stopped => {
                    if let Err(error) = self.runtime_close_result() {
                        self.record_runtime_close_failure(error);
                    }
                    self.shutdown = WinitShutdownState::Complete;
                    EventLoopDirective::Exit
                }
                RuntimeState::Stopping => EventLoopDirective::WaitUntil(
                    now.checked_add(RUNTIME_CLOSE_POLL_INTERVAL).unwrap_or(now),
                ),
                RuntimeState::CloseIncomplete => {
                    self.record_runtime_close_incomplete();
                    self.shutdown = WinitShutdownState::Aborted;
                    EventLoopDirective::Exit
                }
                state => {
                    self.record_runtime_close_failure(AppRunError::runner(format!(
                        "managed runtime remained in {state:?} during platform shutdown"
                    )));
                    self.shutdown = WinitShutdownState::Aborted;
                    EventLoopDirective::Exit
                }
            },
            Ok(NativeShutdownAction::TimedOut) => {
                self.record_native_retirement_failure(AppRunError::runner(
                    "timed out waiting for native window destruction",
                ));
                self.shutdown = WinitShutdownState::Aborted;
                EventLoopDirective::Exit
            }
            Err(_) => {
                self.record_native_retirement_failure(AppRunError::runner(
                    "native window target disappeared during shutdown",
                ));
                self.shutdown = WinitShutdownState::Aborted;
                EventLoopDirective::Exit
            }
        }
    }

    fn finish_after_event_loop(&mut self) {
        if self.shutdown == WinitShutdownState::Complete {
            return;
        }

        let retirement_result =
            retire_runtime_targets(self.runtime, &self.backend_windows, &self.owned_window_ids);
        self.platform_windows.clear();
        self.nara_windows_by_winit.clear();
        self.shutdown = WinitShutdownState::Aborted;
        match self.runtime.state() {
            RuntimeState::Stopping => {}
            RuntimeState::CloseIncomplete => self.record_runtime_close_incomplete(),
            RuntimeState::Stopped => {
                if let Err(error) = self.runtime_close_result() {
                    self.record_runtime_close_failure(error);
                }
            }
            RuntimeState::Running | RuntimeState::Paused | RuntimeState::Faulted => {
                if let Err(error) = self.begin_runtime_close() {
                    self.record_runtime_close_failure(error);
                }
            }
            RuntimeState::Stepping => self.record_runtime_close_failure(AppRunError::runner(
                "managed runtime remained in an in-flight step after the event loop ended",
            )),
        }
        match retirement_result {
            Ok(fault) => {
                if let Some(fault) = fault {
                    self.record_primary_failure(runtime_fault_error(&fault));
                }
                self.record_native_retirement_failure(AppRunError::runner(
                    "winit event loop ended before controlled window retirement",
                ));
            }
            Err(error) => self.record_native_retirement_failure(error),
        }
    }

    fn begin_runtime_close(&mut self) -> Result<(), AppRunError> {
        if self.runtime.state() == RuntimeState::CloseIncomplete {
            self.record_runtime_close_incomplete();
            return Ok(());
        }
        match self.runtime.request_control(RuntimeControl::Stop) {
            RuntimeControlRequestResult::Accepted(_) => self.poll_runtime_close(),
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

    fn poll_runtime_close(&mut self) -> Result<(), AppRunError> {
        if self.runtime_close_incomplete_observed {
            return Ok(());
        }
        if matches!(
            self.runtime.state(),
            RuntimeState::Stopping
                | RuntimeState::Running
                | RuntimeState::Paused
                | RuntimeState::Faulted
        ) {
            self.runtime
                .drive(Duration::ZERO)
                .map_err(runtime_drive_error)?;
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
        EventLoopDirective::WaitUntil(deadline) => {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
        EventLoopDirective::Exit => event_loop.exit(),
    }
}

impl ApplicationHandler<WinitUserEvent> for WinitApp<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
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
                if let Err(error) = self.handle_accessibility_wake(window_id) {
                    self.fail(_event_loop, error);
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.shutdown.is_started() {
            self.poll_native_shutdown(event_loop, Instant::now());
            return;
        }
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        self.run_frame(delta, event_loop);
    }
}

fn native_shutdown_action(
    backend_windows: &BackendWindowHandles,
    owned_window_ids: &BTreeSet<WindowId>,
    now: Instant,
    deadline: Instant,
) -> Result<NativeShutdownAction, nara_window::backend::WindowTargetError> {
    if backend_windows.all_native_destroyed(owned_window_ids.iter().copied())? {
        Ok(NativeShutdownAction::Complete)
    } else if now >= deadline {
        Ok(NativeShutdownAction::TimedOut)
    } else {
        Ok(NativeShutdownAction::WaitUntil(deadline))
    }
}

fn should_process_window_event_during_shutdown(
    shutdown: WinitShutdownState,
    event: &WinitWindowEvent,
) -> bool {
    !shutdown.is_started() || matches!(event, WinitWindowEvent::Destroyed)
}

fn retire_runtime_targets(
    runtime: &mut RuntimeInstance,
    backend_windows: &BackendWindowHandles,
    owned_window_ids: &BTreeSet<WindowId>,
) -> Result<Option<RuntimeFault>, AppRunError> {
    let owned_window_ids = owned_window_ids.iter().copied().collect::<Vec<_>>();
    backend_windows
        .request_retirements(&owned_window_ids)
        .map_err(|_| AppRunError::runner("owned native window target is not registered"))?;
    let mut post_operation_fault = None;
    if runtime.world().contains_resource::<InputTimeline>() {
        let outcome = runtime
            .with_driver_scope(|scope| {
                for window_id in owned_window_ids.iter().copied() {
                    apply_window_input_driver_event(
                        scope,
                        InputWindowId::new(window_id.get()),
                        InputSeatId::PRIMARY,
                        InputWindowEvent::SurfaceRetired,
                    )?;
                }
                Ok::<(), ButtonInputDriverError>(())
            })
            .map_err(runtime_scope_error)?;
        outcome
            .output
            .map_err(|_| AppRunError::runner("input route retirement failed"))?;
        post_operation_fault = outcome.post_operation_fault;
    }
    let retirement_driver = runtime
        .world()
        .get_resource::<WindowSurfaceRetirementDriver>()
        .copied();
    if let Some(retirement_driver) = retirement_driver {
        let outcome = runtime
            .with_driver_scope(|scope| retirement_driver.retire_targets(scope, &owned_window_ids))
            .map_err(runtime_scope_error)?;
        outcome
            .output
            .map_err(|_| AppRunError::runner("renderer failed to retire owned window surfaces"))?;
        post_operation_fault = post_operation_fault.or(outcome.post_operation_fault);
    }
    backend_windows
        .release_retired_providers(owned_window_ids.iter().copied())
        .map_err(|_| {
            AppRunError::runner("renderer did not retire every surface before shutdown")
        })?;
    Ok(post_operation_fault)
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
