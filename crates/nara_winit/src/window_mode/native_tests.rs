use std::{
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use nara_app::{App, AppRunError, RuntimeInstance, RuntimeState};
use nara_window::{
    Window, WindowEvent, WindowId, WindowMode, WindowPlugin, WindowResolution,
    backend::WindowTargetPhase, push_window_driver_event,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent as WinitWindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::pump_events::{EventLoopExtPumpEvents, PumpStatus},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Fullscreen, Window as WinitWindow, WindowId as WinitWindowId},
};

use crate::{
    DEFAULT_NATIVE_RETIREMENT_BUDGET, NoPresentationDriver, WinitApp, WinitControlFlow,
    WinitEventLoop, WinitSession, WinitShutdownState, WinitUserEvent,
};

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own process"]
fn native_windowed_creation_and_retirement() {
    run_native_mode(WindowMode::Windowed, false);
}

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own process"]
fn native_borderless_creation_and_retirement() {
    run_native_mode(WindowMode::BorderlessFullscreen, false);
}

#[test]
#[ignore = "requires an interactive Windows desktop with exclusive video modes; run in its own process"]
fn native_exclusive_creation_and_retirement() {
    run_native_mode(WindowMode::Fullscreen, false);
}

#[test]
#[ignore = "requires an interactive Windows desktop; run in its own process"]
fn native_unsupported_exclusive_rejects_before_creation() {
    run_native_mode(WindowMode::Fullscreen, true);
}

fn run_native_mode(mode: WindowMode, unsupported: bool) {
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let mut event_loop = builder.build().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut window = Window::new(
        "nara native window mode test",
        WindowResolution::new(320, 180),
    );
    window.mode = mode;
    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: Some(window),
    })
    .unwrap();
    let runtime = RuntimeInstance::start(app).unwrap();
    let mut state = WinitApp::new(
        runtime,
        Box::new(NoPresentationDriver),
        WinitControlFlow::Poll,
        DEFAULT_NATIVE_RETIREMENT_BUDGET,
    );
    state.event_proxy = Some(event_loop.create_proxy());
    let mut probe = NativeModeProbe {
        state,
        mode,
        unsupported,
        started: false,
        observed: false,
        expected_fullscreen: None,
        expected_extent: PhysicalSize::new(320, 180),
        native_window: None,
        failure: None,
        deadline: Instant::now() + Duration::from_secs(15),
    };

    loop {
        if let PumpStatus::Exit(_) =
            event_loop.pump_app_events(Some(Duration::from_millis(10)), &mut probe)
        {
            break;
        }
        if probe.state.shutdown == WinitShutdownState::Complete
            || probe.state.retirement_drive_incomplete(Instant::now())
        {
            break;
        }
    }
    let mut event_loop = WinitEventLoop::from_native(event_loop);
    event_loop.resumed = probe.state.native_resumed;
    let mut session = WinitSession {
        app: Some(probe.state),
        event_loop: Some(event_loop),
        executed: true,
    };
    session.execute();
    let state = session.app.as_ref().unwrap();
    let runner_failure = state.failure();

    assert!(
        probe.failure.is_none(),
        "native mode observation failed: {:?}",
        probe.failure
    );
    assert!(
        runner_failure.is_none(),
        "native shutdown failed: {runner_failure:?}"
    );
    assert!(probe.observed, "the requested native mode was not observed");
    assert_eq!(state.runtime.state(), RuntimeState::Stopped);
    assert_eq!(state.shutdown, WinitShutdownState::Complete);
    assert!(state.platform_windows.is_empty());

    if unsupported {
        assert!(!state.backend_windows.is_registered(WindowId::PRIMARY));
        assert!(state.owned_window_ids.is_empty());
        assert!(probe.native_window.is_none());
    } else {
        let target = state.backend_windows.snapshot(WindowId::PRIMARY).unwrap();
        assert_eq!(target.phase, WindowTargetPhase::NativeDestroyed);
        assert_eq!(target.fault, None);
        assert!(!target.provider_present);
        assert!(!target.surface_active);
        assert!(probe.native_window.as_ref().unwrap().upgrade().is_none());
    }
}

struct NativeModeProbe {
    state: WinitApp,
    mode: WindowMode,
    unsupported: bool,
    started: bool,
    observed: bool,
    expected_fullscreen: Option<Fullscreen>,
    expected_extent: PhysicalSize<u32>,
    native_window: Option<Weak<WinitWindow>>,
    failure: Option<String>,
    deadline: Instant,
}

impl NativeModeProbe {
    fn create_and_observe(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let monitor = event_loop
            .primary_monitor()
            .ok_or("the native fixture requires a primary monitor")?;
        match self.mode {
            WindowMode::Windowed => {}
            WindowMode::BorderlessFullscreen => {
                self.expected_extent = monitor.size();
                self.expected_fullscreen = Some(Fullscreen::Borderless(Some(monitor.clone())));
            }
            WindowMode::Fullscreen => {
                let modes: Vec<_> = monitor.video_modes().collect();
                let extent = if self.unsupported {
                    let width = modes
                        .iter()
                        .map(|mode| mode.size().width)
                        .max()
                        .unwrap_or(0)
                        .checked_add(1)
                        .ok_or("the native fixture cannot choose an unsupported extent")?;
                    PhysicalSize::new(width, 1)
                } else {
                    let mode = modes
                        .iter()
                        .find(|mode| mode.size() == monitor.size())
                        .or_else(|| modes.first())
                        .ok_or("the native fixture requires an advertised video mode")?;
                    mode.size()
                };
                if !self.unsupported {
                    let best = modes
                        .iter()
                        .filter(|mode| mode.size() == extent)
                        .max_by_key(|mode| (mode.refresh_rate_millihertz(), mode.bit_depth()))
                        .ok_or("the native fixture has no matching exclusive mode")?;
                    self.expected_fullscreen = Some(Fullscreen::Exclusive(best.clone()));
                }
                self.expected_extent = extent;
                self.state
                    .with_driver_scope(|scope| {
                        push_window_driver_event(
                            scope,
                            WindowEvent::Resized {
                                window_id: WindowId::PRIMARY,
                                resolution: WindowResolution::new(extent.width, extent.height),
                            },
                        )
                    })
                    .map_err(|error| error.to_string())?;
                self.state
                    .runtime
                    .drive(Duration::ZERO)
                    .map_err(|error| error.to_string())?;
            }
        }

        let creation = self.state.create_primary_window(event_loop);
        if self.unsupported {
            let error = creation
                .err()
                .ok_or("unsupported exclusive mode created a native window")?;
            if !matches!(error, AppRunError::Runner { .. })
                || !error
                    .to_string()
                    .contains("no exclusive fullscreen video mode matching")
            {
                return Err(format!("unexpected exclusive startup error: {error}"));
            }
            self.observed = true;
            self.state.begin_shutdown(event_loop);
        } else {
            creation.map_err(|error| error.to_string())?;
            let window = self
                .state
                .platform_windows
                .get(&WindowId::PRIMARY)
                .ok_or("native creation did not retain the primary window")?;
            self.native_window = Some(Arc::downgrade(window));
        }
        Ok(())
    }
}

impl ApplicationHandler<WinitUserEvent> for NativeModeProbe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.state.native_resumed = true;
        if self.started {
            return;
        }
        self.started = true;
        if let Err(error) = self.create_and_observe(event_loop) {
            self.failure = Some(error);
            self.state.begin_shutdown(event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WinitWindowId,
        event: WinitWindowEvent,
    ) {
        self.state.window_event(event_loop, window_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if !self.state.shutdown.is_started() {
            let window = self.state.platform_windows.get(&WindowId::PRIMARY);
            if window.is_some_and(|window| {
                window.fullscreen() == self.expected_fullscreen
                    && window.inner_size() == self.expected_extent
            }) {
                self.observed = true;
                self.state.begin_shutdown(event_loop);
            } else if Instant::now() >= self.deadline {
                self.failure = Some(format!(
                    "requested {:?} at {:?}; observed {:?}",
                    self.expected_fullscreen,
                    self.expected_extent,
                    window.map(|window| (window.fullscreen(), window.inner_size())),
                ));
                self.state.begin_shutdown(event_loop);
            }
        }
        self.state.about_to_wait(event_loop);
    }
}
