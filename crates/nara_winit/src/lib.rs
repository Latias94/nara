//! Winit runner and native window adapter for nara.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use nara_app::{App, AppExit, AppRunError, Plugin, PluginError};
use nara_input::{ButtonInput, InputPlugin, KeyCode, MouseButton, PointerState};
use nara_window::{
    Window, WindowEvent, WindowId, WindowPlugin, WindowResolution,
    backend::{BackendWindowHandles, RawWindowHandleProvider},
    push_window_event,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
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

/// Installs a winit-owned desktop runner.
#[derive(Debug, Clone)]
pub struct WinitPlugin {
    runner: WinitRunner,
}

impl WinitPlugin {
    #[must_use]
    pub fn new(runner: WinitRunner) -> Self {
        Self { runner }
    }
}

impl Default for WinitPlugin {
    fn default() -> Self {
        Self {
            runner: WinitRunner::default(),
        }
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
        app.set_runner(move |app| runner.run(app));
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

    pub fn run(self, app: App) -> Result<AppExit, AppRunError> {
        let event_loop = EventLoop::new().map_err(|error| {
            AppRunError::runner(format!("failed to create winit event loop: {error}"))
        })?;
        event_loop.set_control_flow(self.control_flow.into_winit());

        let mut state = WinitApp::new(app);
        let run_result = event_loop.run_app(&mut state);

        if let Some(error) = state.failure {
            return Err(error);
        }

        run_result
            .map_err(|error| AppRunError::runner(format!("winit event loop failed: {error}")))?;

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

struct WinitApp {
    app: App,
    nara_windows_by_winit: HashMap<WinitWindowId, WindowId>,
    platform_windows: HashMap<WindowId, Arc<WinitWindow>>,
    last_frame: Instant,
    exit: AppExit,
    failure: Option<AppRunError>,
}

impl WinitApp {
    fn new(app: App) -> Self {
        Self {
            app,
            nara_windows_by_winit: HashMap::new(),
            platform_windows: HashMap::new(),
            last_frame: Instant::now(),
            exit: AppExit::Success,
            failure: None,
        }
    }

    fn create_primary_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), AppRunError> {
        if !self.platform_windows.is_empty() {
            return Ok(());
        }

        let window = configured_primary_window(&mut self.app);
        let window_id = window.id;
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
        let provider = raw_handle_provider(platform_window.clone())?;

        self.nara_windows_by_winit
            .insert(platform_window.id(), window_id);
        self.platform_windows
            .insert(window_id, platform_window.clone());
        self.app
            .world_mut()
            .resource_mut::<BackendWindowHandles>()
            .insert(window_id, provider);
        push_window_event(self.app.world_mut(), WindowEvent::Created { window_id });

        Ok(())
    }

    fn run_frame(&mut self, delta: Duration, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.app.run_once(delta) {
            self.fail(event_loop, error);
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
    ) {
        let Some(window_id) = self.nara_windows_by_winit.get(&winit_window_id).copied() else {
            return;
        };

        match event {
            WinitWindowEvent::CloseRequested => {
                push_window_event(
                    self.app.world_mut(),
                    WindowEvent::CloseRequested { window_id },
                );
                self.exit = AppExit::Requested;
                event_loop.exit();
            }
            WinitWindowEvent::Destroyed => {
                push_window_event(self.app.world_mut(), WindowEvent::Closed { window_id });
                self.platform_windows.remove(&window_id);
                self.app
                    .world_mut()
                    .resource_mut::<BackendWindowHandles>()
                    .remove(window_id);
            }
            WinitWindowEvent::Resized(size) => {
                let resolution = WindowResolution::new(size.width, size.height);
                push_window_event(
                    self.app.world_mut(),
                    WindowEvent::Resized {
                        window_id,
                        resolution,
                    },
                );
            }
            WinitWindowEvent::Focused(focused) => {
                push_window_event(
                    self.app.world_mut(),
                    WindowEvent::Focused { window_id, focused },
                );
            }
            WinitWindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                push_window_event(
                    self.app.world_mut(),
                    WindowEvent::ScaleFactorChanged {
                        window_id,
                        scale_factor,
                    },
                );
            }
            WinitWindowEvent::RedrawRequested => {
                push_window_event(
                    self.app.world_mut(),
                    WindowEvent::RedrawRequested { window_id },
                );
            }
            WinitWindowEvent::KeyboardInput { event, .. } => {
                apply_keyboard_input(self.app.world_mut(), &event);
            }
            WinitWindowEvent::MouseInput { state, button, .. } => {
                apply_mouse_input(self.app.world_mut(), state, button);
            }
            WinitWindowEvent::CursorMoved { position, .. } => {
                apply_cursor_moved(self.app.world_mut(), position.x, position.y);
            }
            WinitWindowEvent::CursorLeft { .. } => {
                apply_cursor_left(self.app.world_mut());
            }
            _ => {}
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: AppRunError) {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        event_loop.exit();
    }
}

impl ApplicationHandler for WinitApp {
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
        self.handle_window_event(event_loop, window_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        self.run_frame(delta, event_loop);
    }
}

fn configured_primary_window(app: &mut App) -> Window {
    let mut query = app.world_mut().query::<&Window>();
    query
        .iter(app.world())
        .next()
        .cloned()
        .unwrap_or_else(Window::default)
}

fn raw_handle_provider(window: Arc<WinitWindow>) -> Result<RawWindowHandleProvider, AppRunError> {
    let window_handle = window
        .window_handle()
        .map_err(|error| {
            AppRunError::runner(format!("failed to read native window handle: {error}"))
        })?
        .as_raw();
    let display_handle = window
        .display_handle()
        .map_err(|error| {
            AppRunError::runner(format!("failed to read native display handle: {error}"))
        })?
        .as_raw();

    // SAFETY: the Arc keeps the platform window alive for at least as long as the
    // raw handles are registered in BackendWindowHandles.
    Ok(unsafe { RawWindowHandleProvider::new(window_handle, display_handle, window) })
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
    use nara_window::WindowEvents;

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
}
