//! Winit runner and native window adapter for nara.

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::{Duration, Instant},
};

use nara_app::{App, AppExit, AppRunError, Plugin, PluginError};
use nara_input::{ButtonInput, InputPlugin, KeyCode, MouseButton, PointerState};
use nara_window::{
    Window, WindowEvent, WindowId, WindowPlugin, WindowResolution,
    backend::{BackendWindowHandles, WindowHandleProvider, WindowSurfaceRetirementDriver},
    push_window_event,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{
        ElementState, KeyEvent, MouseButton as WinitMouseButton, WindowEvent as WinitWindowEvent,
    },
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode as WinitKeyCode, PhysicalKey},
    window::{Window as WinitWindow, WindowId as WinitWindowId},
};

const NATIVE_DESTROY_TIMEOUT: Duration = Duration::from_secs(5);

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

impl WinitShutdownState {
    const fn is_started(self) -> bool {
        !matches!(self, Self::Running)
    }
}

/// Installs a winit-owned desktop runner.
#[derive(Debug, Clone, Default)]
pub struct WinitPlugin {
    runner: WinitRunner,
}

impl WinitPlugin {
    #[must_use]
    pub fn new(runner: WinitRunner) -> Self {
        Self { runner }
    }
}

impl Plugin for WinitPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.winit"),
            nara_app::PluginCategory::Platform,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(WindowPlugin::default())?;
        app.add_plugin_if_missing(InputPlugin)?;

        let runner = self.runner;
        app.set_runner(move |app| runner.run(app))?;
        Ok(())
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

    fn run(self, app: &mut App) -> Result<AppExit, AppRunError> {
        let event_loop = EventLoop::new().map_err(|error| {
            AppRunError::runner(format!("failed to create winit event loop: {error}"))
        })?;
        event_loop.set_control_flow(self.control_flow.into_winit());

        let mut state = WinitApp::new(app)?;
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

struct WinitApp<'app> {
    app: &'app mut App,
    nara_windows_by_winit: HashMap<WinitWindowId, WindowId>,
    platform_windows: HashMap<WindowId, Arc<WinitWindow>>,
    owned_window_ids: BTreeSet<WindowId>,
    last_frame: Instant,
    exit: AppExit,
    primary_failure: Option<AppRunError>,
    teardown_failure: Option<AppRunError>,
    backend_windows: BackendWindowHandles,
    shutdown: WinitShutdownState,
}

impl<'app> WinitApp<'app> {
    fn new(app: &'app mut App) -> Result<Self, AppRunError> {
        let backend_windows = app
            .world()
            .get_resource::<BackendWindowHandles>()
            .cloned()
            .ok_or_else(|| AppRunError::runner("backend window authority is missing"))?;
        Ok(Self {
            app,
            nara_windows_by_winit: HashMap::new(),
            platform_windows: HashMap::new(),
            owned_window_ids: BTreeSet::new(),
            last_frame: Instant::now(),
            exit: AppExit::Success,
            primary_failure: None,
            teardown_failure: None,
            backend_windows,
            shutdown: WinitShutdownState::Running,
        })
    }

    fn create_primary_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppRunError> {
        if self.shutdown.is_started() || !self.platform_windows.is_empty() {
            return Ok(());
        }

        let window = configured_primary_window(self.app)?;
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
            .with_resizable(window.resizable);

        let platform_window = event_loop.create_window(attributes).map_err(|error| {
            AppRunError::runner(format!("failed to create native window: {error}"))
        })?;
        let platform_window = Arc::new(platform_window);
        let provider = WindowHandleProvider::new(platform_window.clone());

        self.backend_windows
            .insert(window_id, provider)
            .map_err(|_| AppRunError::runner("native window target is already registered"))?;
        self.owned_window_ids.insert(window_id);
        self.nara_windows_by_winit
            .insert(platform_window.id(), window_id);
        self.platform_windows
            .insert(window_id, platform_window.clone());
        push_window_event(self.app.world_mut()?, WindowEvent::Created { window_id });

        Ok(())
    }

    fn run_frame(&mut self, delta: Duration, event_loop: &ActiveEventLoop) {
        let outcome = match self.app.run_once(delta) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.fail(event_loop, error);
                return;
            }
        };

        if let Some(exit) = outcome.exit {
            self.exit = exit;
            self.begin_shutdown(event_loop);
            return;
        }

        for window in self.platform_windows.values() {
            window.request_redraw();
        }
    }

    fn handle_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        winit_window_id: WinitWindowId,
        event: WinitWindowEvent,
    ) -> Result<(), AppRunError> {
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

        match event {
            WinitWindowEvent::CloseRequested => {
                push_window_event(
                    self.app.world_mut()?,
                    WindowEvent::CloseRequested { window_id },
                );
            }
            WinitWindowEvent::Resized(size) => {
                let resolution = WindowResolution::new(size.width, size.height);
                push_window_event(
                    self.app.world_mut()?,
                    WindowEvent::Resized {
                        window_id,
                        resolution,
                    },
                );
            }
            WinitWindowEvent::Focused(focused) => {
                push_window_event(
                    self.app.world_mut()?,
                    WindowEvent::Focused { window_id, focused },
                );
            }
            WinitWindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                push_window_event(
                    self.app.world_mut()?,
                    WindowEvent::ScaleFactorChanged {
                        window_id,
                        scale_factor,
                    },
                );
            }
            WinitWindowEvent::RedrawRequested => {
                push_window_event(
                    self.app.world_mut()?,
                    WindowEvent::RedrawRequested { window_id },
                );
            }
            WinitWindowEvent::KeyboardInput { event, .. } => {
                apply_keyboard_input(self.app.world_mut()?, &event);
            }
            WinitWindowEvent::MouseInput { state, button, .. } => {
                apply_mouse_input(self.app.world_mut()?, state, button);
            }
            WinitWindowEvent::CursorMoved { position, .. } => {
                apply_cursor_moved(self.app.world_mut()?, position.x, position.y);
            }
            WinitWindowEvent::CursorLeft { .. } => {
                apply_cursor_left(self.app.world_mut()?);
            }
            _ => {}
        }

        Ok(())
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
            push_window_event(self.app.world_mut()?, WindowEvent::Closed { window_id });
        }
        self.platform_windows.remove(&window_id);
        self.nara_windows_by_winit.remove(&winit_window_id);

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

    fn record_teardown_failure(&mut self, error: AppRunError) {
        self.teardown_failure.get_or_insert(error);
    }

    fn take_failure(&mut self) -> Option<AppRunError> {
        match (self.primary_failure.take(), self.teardown_failure.take()) {
            (Some(prior), Some(teardown)) => Some(AppRunError::runner_teardown(prior, teardown)),
            (Some(error), None) | (None, Some(error)) => Some(error),
            (None, None) => None,
        }
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
                if let Err(error) =
                    retire_app_targets(self.app, &self.backend_windows, &self.owned_window_ids)
                {
                    self.record_teardown_failure(error);
                    self.shutdown = WinitShutdownState::Aborted;
                    return EventLoopDirective::Exit;
                }

                self.platform_windows.clear();
                self.shutdown = WinitShutdownState::WaitingForNative {
                    deadline: now + NATIVE_DESTROY_TIMEOUT,
                };
            }
        }
        self.poll_native_shutdown_transition(now)
    }

    fn poll_native_shutdown(&mut self, event_loop: &ActiveEventLoop, now: Instant) {
        let directive = self.poll_native_shutdown_transition(now);
        apply_event_loop_directive(event_loop, directive);
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
                EventLoopDirective::WaitUntil(deadline)
            }
            Ok(NativeShutdownAction::Complete) => {
                self.shutdown = WinitShutdownState::Complete;
                EventLoopDirective::Exit
            }
            Ok(NativeShutdownAction::TimedOut) => {
                self.record_teardown_failure(AppRunError::runner(
                    "timed out waiting for native window destruction",
                ));
                self.shutdown = WinitShutdownState::Aborted;
                EventLoopDirective::Exit
            }
            Err(_) => {
                self.record_teardown_failure(AppRunError::runner(
                    "native window target disappeared during shutdown",
                ));
                self.shutdown = WinitShutdownState::Aborted;
                EventLoopDirective::Exit
            }
        }
    }

    fn finish_after_event_loop(&mut self) {
        if self.shutdown == WinitShutdownState::Complete || self.owned_window_ids.is_empty() {
            return;
        }

        let retirement_result =
            retire_app_targets(self.app, &self.backend_windows, &self.owned_window_ids);
        self.platform_windows.clear();
        self.nara_windows_by_winit.clear();
        self.shutdown = WinitShutdownState::Aborted;
        match retirement_result {
            Ok(()) => self.record_teardown_failure(AppRunError::runner(
                "winit event loop ended before controlled window retirement",
            )),
            Err(error) => self.record_teardown_failure(error),
        }
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

impl ApplicationHandler for WinitApp<'_> {
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

fn retire_app_targets(
    app: &mut App,
    backend_windows: &BackendWindowHandles,
    owned_window_ids: &BTreeSet<WindowId>,
) -> Result<(), AppRunError> {
    let owned_window_ids = owned_window_ids.iter().copied().collect::<Vec<_>>();
    backend_windows
        .request_retirements(&owned_window_ids)
        .map_err(|_| AppRunError::runner("owned native window target is not registered"))?;
    let retirement_driver = app
        .world()
        .get_resource::<WindowSurfaceRetirementDriver>()
        .copied();
    if let Some(retirement_driver) = retirement_driver {
        retirement_driver
            .retire_targets(app.world_mut()?, &owned_window_ids)
            .map_err(|_| AppRunError::runner("renderer failed to retire owned window surfaces"))?;
    }
    backend_windows
        .release_retired_providers(owned_window_ids.iter().copied())
        .map_err(|_| AppRunError::runner("renderer did not retire every surface before shutdown"))
}

fn configured_primary_window(app: &mut App) -> Result<Window, AppRunError> {
    let world = app.world_mut()?;
    let mut query = world.query::<&Window>();
    Ok(query
        .iter(world)
        .next()
        .cloned()
        .unwrap_or_else(Window::default))
}

fn apply_keyboard_input(world: &mut nara_ecs::World, event: &KeyEvent) {
    let Some(key_code) = convert_physical_key(event.physical_key) else {
        return;
    };
    let mut input = world.resource_mut::<ButtonInput<KeyCode>>();
    match event.state {
        ElementState::Pressed => input.press(key_code),
        ElementState::Released => input.release(key_code),
    }
}

fn apply_mouse_input(world: &mut nara_ecs::World, state: ElementState, button: WinitMouseButton) {
    let mut input = world.resource_mut::<ButtonInput<MouseButton>>();
    let button = convert_mouse_button(button);
    match state {
        ElementState::Pressed => input.press(button),
        ElementState::Released => input.release(button),
    }
}

fn apply_cursor_moved(world: &mut nara_ecs::World, x: f64, y: f64) {
    let mut pointer = world.resource_mut::<PointerState>();
    pointer.set_position(nara_core::Vec2::new(x as f32, y as f32));
}

fn apply_cursor_left(world: &mut nara_ecs::World) {
    world.resource_mut::<PointerState>().clear_position();
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
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use nara_app::PluginCleanupContext;
    use nara_window::{
        WindowEvents,
        backend::{WindowSurfaceHandleSource, WindowSurfaceLease, WindowSurfaceRetirementError},
    };
    use raw_window_handle::{
        DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
    };

    #[derive(Debug)]
    struct TestWindowSource {
        events: Arc<Mutex<Vec<&'static str>>>,
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
    struct FailingCleanupPlugin;

    impl Plugin for FailingCleanupPlugin {
        fn metadata(&self) -> nara_app::PluginMetadata {
            nara_app::PluginMetadata::new(
                nara_app::PluginId::new("test.cleanup-failure"),
                nara_app::PluginCategory::Backend,
            )
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            Ok(())
        }

        fn cleanup(&self, _context: &mut PluginCleanupContext<'_>) -> Result<(), PluginError> {
            Err(PluginError::SetupFailed {
                plugin: nara_app::PluginId::new("test.cleanup-failure"),
                message: "injected cleanup failure".to_owned(),
            })
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
        world: &mut nara_ecs::World,
        window_ids: &[WindowId],
    ) -> Result<(), WindowSurfaceRetirementError> {
        let Some(mut backend) = world.get_non_send_mut::<FakeSurfaceBackend>() else {
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
    fn plugin_installs_prerequisite_resources() {
        let mut app = App::new();
        app.add_plugin(WinitPlugin::default()).unwrap();
        app.finish_plugins().unwrap();

        assert!(app.world().contains_resource::<WindowEvents>());
        assert!(app.world().contains_resource::<BackendWindowHandles>());
        assert!(app.world().contains_resource::<ButtonInput<KeyCode>>());
        assert!(app.world().contains_resource::<ButtonInput<MouseButton>>());
        assert!(app.world().contains_resource::<PointerState>());
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

        let owned_window_ids = BTreeSet::from([WindowId::PRIMARY]);
        retire_app_targets(&mut app, &handles, &owned_window_ids).unwrap();
        retire_app_targets(&mut app, &handles, &owned_window_ids).unwrap();
        app.cleanup_plugins().unwrap();

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

        let owned_window_ids = BTreeSet::from([WindowId::PRIMARY]);
        retire_app_targets(&mut app, &handles, &owned_window_ids).unwrap();

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
    fn cleanup_failure_does_not_block_safe_target_retirement() {
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
        app.add_plugin(FailingCleanupPlugin).unwrap();

        let owned_window_ids = BTreeSet::from([WindowId::PRIMARY]);
        retire_app_targets(&mut app, &handles, &owned_window_ids).unwrap();

        assert_eq!(*events.lock().unwrap(), vec!["surface", "provider"]);
        assert!(app.cleanup_plugins().is_err());
        assert_eq!(
            handles.snapshot(WindowId::PRIMARY).unwrap().phase,
            nara_window::backend::WindowTargetPhase::ProviderReleased
        );
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
        let mut state = WinitApp::new(&mut app).unwrap();
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
        let mut state = WinitApp::new(&mut app).unwrap();
        let primary = AppRunError::runner("winit event loop failed: os failure");
        let teardown = AppRunError::runner("timed out waiting for native window destruction");

        state.record_teardown_failure(teardown.clone());
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
        let mut state = WinitApp::new(&mut app).unwrap();
        let winit_window_id = WinitWindowId::dummy();
        state
            .nara_windows_by_winit
            .insert(winit_window_id, WindowId::PRIMARY);
        state.owned_window_ids.insert(WindowId::PRIMARY);

        assert_eq!(
            state
                .handle_destroyed_window(winit_window_id, Instant::now())
                .unwrap(),
            EventLoopDirective::Exit
        );

        assert_eq!(state.shutdown, WinitShutdownState::Complete);
        assert_eq!(
            state.primary_failure,
            Some(AppRunError::runner(
                "native window was destroyed before controlled retirement"
            ))
        );
        assert_eq!(state.teardown_failure, None);
        assert!(!state.nara_windows_by_winit.contains_key(&winit_window_id));
        assert_eq!(
            state.app.world().resource::<WindowEvents>().as_slice(),
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
                .app
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
}
