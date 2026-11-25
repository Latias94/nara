//! One original HWND, real mixed-DPI events and Runtime synchronization, followed by retirement.
//!
//! Move markers are programmatic branch comparisons with the existing cursor position, not
//! human titlebar-drag evidence. No suggested-rectangle position is asserted. Win32 calls use
//! the retained window on its owner thread; Winit supplies the message hook's valid MSG pointer.

use super::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{cell::Cell, process::Command, rc::Rc, sync::Weak};
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    UI::{
        HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI},
        WindowsAndMessaging::{
            MSG, PostMessageW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW,
            SetWindowPos, WM_APP, WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE,
        },
    },
};
use winit::{
    dpi::{LogicalSize, PhysicalPosition},
    event::StartCause,
    monitor::MonitorHandle,
    platform::{
        pump_events::{EventLoopExtPumpEvents, PumpStatus},
        windows::MonitorHandleExtWindows,
    },
};

const MOVE: u32 = WM_APP + 0x4e41;
const MARKED_MOVE: u32 = MOVE + 1;
const BASE: [f64; 2] = [360.0, 240.0];
const OVERRIDE: [f64; 2] = [424.0, 272.0];
const QUIET: Duration = Duration::from_millis(300);
const LEG_BUDGET: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveKind {
    Baseline,
    Control,
    Writer,
    Marker,
}

const LEGS: [(usize, MoveKind, [f64; 2]); 9] = [
    (0, MoveKind::Baseline, BASE),
    (1, MoveKind::Control, BASE),
    (0, MoveKind::Control, BASE),
    (1, MoveKind::Writer, OVERRIDE),
    (0, MoveKind::Writer, BASE),
    (1, MoveKind::Marker, BASE),
    (0, MoveKind::Marker, BASE),
    (1, MoveKind::Marker, BASE),
    (0, MoveKind::Marker, BASE),
];

struct MonitorGeometry {
    monitor: MonitorHandle,
    work: [i32; 4],
}

#[derive(Debug, PartialEq)]
struct Geometry {
    position: PhysicalPosition<i32>,
    outer: PhysicalSize<u32>,
    inner: PhysicalSize<u32>,
    scale: f64,
    dpi: u32,
    monitor: usize,
}

fn hwnd(window: &WinitWindow) -> Result<HWND, String> {
    match window
        .window_handle()
        .map_err(|error| error.to_string())?
        .as_raw()
    {
        RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get() as HWND),
        other => Err(format!("expected the original Win32 HWND, got {other:?}")),
    }
}

struct GeometryProbe {
    state: WinitApp,
    original: Option<(WinitWindowId, Weak<WinitWindow>)>,
    monitors: Vec<MonitorGeometry>,
    pair: [usize; 2],
    leg: usize,
    started: Instant,
    quiet_since: Instant,
    last: Option<Geometry>,
    events: [usize; 3],
    writer_calls: usize,
    move_failed: Rc<Cell<bool>>,
    failure: Option<String>,
}

impl GeometryProbe {
    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let window = self
            .state
            .platform_windows
            .get(&WindowId::PRIMARY)
            .ok_or("the original WinitApp did not create its primary window")?;
        self.original = Some((window.id(), Arc::downgrade(window)));
        for monitor in event_loop.available_monitors() {
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            let (mut dpi_x, mut dpi_y) = (0, 0);
            let native = monitor.hmonitor() as _;
            if unsafe { GetMonitorInfoW(native, &mut info) } == 0 {
                return Err(format!(
                    "GetMonitorInfoW: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let result =
                unsafe { GetDpiForMonitor(native, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
            let dpi = [dpi_x, dpi_y];
            if result < 0 || dpi.contains(&0) {
                return Err(format!("GetDpiForMonitor failed: {result:#x}, dpi={dpi:?}"));
            }
            let r = info.rcMonitor;
            let w = info.rcWork;
            let work = [w.left, w.top, w.right, w.bottom];
            eprintln!(
                "monitor={:?}, native={native:?}, winit_position={:?}, winit_size={:?}, \
                 native_rect={:?}, work={work:?}, dpi={dpi:?}, winit_scale={}",
                monitor.name(),
                monitor.position(),
                monitor.size(),
                [r.left, r.top, r.right, r.bottom],
                monitor.scale_factor(),
            );
            if (monitor.scale_factor() - f64::from(dpi[0]) / 96.0).abs() > 0.001 {
                return Err("Winit and native monitor DPI disagree".into());
            }
            self.monitors.push(MonitorGeometry { monitor, work });
        }
        let source = self
            .monitors
            .iter()
            .position(|m| Some(m.monitor.clone()) == window.current_monitor())
            .ok_or("the original window has no inventoried monitor")?;
        let target = self
            .monitors
            .iter()
            .position(|m| {
                (m.monitor.scale_factor() - self.monitors[source].monitor.scale_factor()).abs()
                    > 0.001
            })
            .ok_or(
                "requires at least two actual monitors with different DPI; no mixed-DPI pair found",
            )?;
        self.pair = [source, target];
        let _ = window.request_inner_size(LogicalSize::new(BASE[0], BASE[1]));
        self.start_leg()
    }

    fn start_leg(&mut self) -> Result<(), String> {
        let (target, kind, extent) = LEGS[self.leg];
        let target = &self.monitors[self.pair[target]];
        let scale = target.monitor.scale_factor();
        let width = (extent[0] * scale).ceil() as i32 + 64;
        let height = (extent[1] * scale).ceil() as i32 + 96;
        let [left, top, right, bottom] = target.work;
        if right - left < width || bottom - top < height {
            return Err(format!(
                "target work area cannot contain the probe: {:?}",
                target.work
            ));
        }
        let x = left + (right - left - width) / 2;
        let y = top + (bottom - top - height) / 2;
        let window = self
            .state
            .platform_windows
            .get(&WindowId::PRIMARY)
            .ok_or("the original native window disappeared")?;
        let native = hwnd(window)?;
        self.started = Instant::now();
        self.quiet_since = self.started;
        self.last = None;
        self.events = [0; 3];
        self.writer_calls = 0;
        eprintln!(
            "leg={} kind={kind:?}, HWND={native:?}, target={:?}, request=({x},{y}), logical={extent:?}",
            self.leg,
            target.monitor.name()
        );
        let message = if kind == MoveKind::Marker {
            MARKED_MOVE
        } else {
            MOVE
        };
        if unsafe { PostMessageW(native, message, x as usize, y as isize) } == 0 {
            return Err(format!("PostMessageW: {}", std::io::Error::last_os_error()));
        }
        Ok(())
    }

    fn observe(&mut self) -> Result<(), String> {
        if self.move_failed.get() {
            return Err("native SetWindowPos failed; see the native trace".into());
        }
        let window = self
            .state
            .platform_windows
            .get(&WindowId::PRIMARY)
            .ok_or("the original native window disappeared")?;
        if Some(window.id()) != self.original.as_ref().map(|original| original.0) {
            return Err("the probe replaced the original native window".into());
        }
        let native = hwnd(window)?;
        let geometry = Geometry {
            position: window.outer_position().map_err(|error| error.to_string())?,
            outer: window.outer_size(),
            inner: window.inner_size(),
            scale: window.scale_factor(),
            dpi: unsafe { GetDpiForWindow(native) },
            monitor: unsafe { MonitorFromWindow(native, MONITOR_DEFAULTTONEAREST) } as usize,
        };
        if self.last.as_ref() != Some(&geometry) {
            eprintln!("leg={} geometry={geometry:?}", self.leg);
            self.quiet_since = Instant::now();
            self.last = Some(geometry);
        }
        let geometry = self.last.as_ref().ok_or("missing native geometry")?;
        let (target, kind, extent) = LEGS[self.leg];
        let monitor = &self.monitors[self.pair[target]].monitor;
        let timed_out = self.started.elapsed() >= LEG_BUDGET;
        if !timed_out && self.quiet_since.elapsed() < QUIET {
            return Ok(());
        }
        let logical = geometry.inner.to_logical::<f64>(geometry.scale);
        let resolution = configured_primary_window(&self.state.runtime)
            .map_err(|error| error.to_string())?
            .resolution;
        let current_monitor = self
            .monitors
            .iter()
            .find(|m| m.monitor.hmonitor() as usize == geometry.monitor)
            .ok_or("the original HWND moved outside the inventoried monitors")?;
        let synchronized = window.current_monitor().as_ref() == Some(&current_monitor.monitor)
            && (geometry.scale - current_monitor.monitor.scale_factor()).abs() < 0.001
            && (f64::from(geometry.dpi) / 96.0 - geometry.scale).abs() < 0.001
            && resolution.physical_width == geometry.inner.width
            && resolution.physical_height == geometry.inner.height
            && (resolution.scale_factor - geometry.scale).abs() < 0.001;
        let requested_geometry = geometry.monitor == monitor.hmonitor() as usize
            && (logical.width - extent[0]).abs() <= 3.0
            && (logical.height - extent[1]).abs() <= 3.0
            && (kind == MoveKind::Baseline || self.events.iter().all(|count| *count > 0))
            && (kind != MoveKind::Writer || self.writer_calls > 0);
        let valid = synchronized && (kind == MoveKind::Marker || requested_geometry);
        if timed_out || !valid {
            return Err(format!(
                "leg={} kind={kind:?}, timed_out={timed_out}, target={:?}, expected_logical={extent:?}, \
                 observed={geometry:?}, logical={logical:?}, Runtime={resolution:?}, \
                 Moved/ScaleFactorChanged/Resized={:?}, writer_calls={}; \
                 Marker is an arbitrary-cursor programmatic comparison, not a human drag reproduction",
                self.leg,
                monitor.name(),
                self.events,
                self.writer_calls,
            ));
        }
        eprintln!(
            "settled leg={} kind={kind:?}, logical={logical:?}, events={:?}, writer_calls={}",
            self.leg, self.events, self.writer_calls
        );
        self.leg += 1;
        if self.leg < LEGS.len() {
            self.start_leg()?;
        }
        Ok(())
    }
}

impl ApplicationHandler<WinitUserEvent> for GeometryProbe {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.state.new_events(event_loop, cause);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.state.resumed(event_loop);
        if self.original.is_none()
            && !self.state.shutdown.is_started()
            && let Err(error) = self.initialize(event_loop)
        {
            self.failure = Some(error);
            self.state.begin_shutdown(event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WinitWindowId,
        mut event: WinitWindowEvent,
    ) {
        if self
            .original
            .as_ref()
            .is_some_and(|original| original.0 == id)
            && self.leg < LEGS.len()
            && !self.state.shutdown.is_started()
        {
            let index = match &mut event {
                WinitWindowEvent::Moved(_) => Some(0),
                WinitWindowEvent::ScaleFactorChanged {
                    scale_factor,
                    inner_size_writer,
                } => {
                    if LEGS[self.leg].1 == MoveKind::Writer {
                        let extent = LEGS[self.leg].2;
                        let size = LogicalSize::new(extent[0], extent[1])
                            .to_physical::<u32>(*scale_factor);
                        match inner_size_writer.request_inner_size(size) {
                            Ok(()) => self.writer_calls += 1,
                            Err(error) => {
                                self.failure
                                    .get_or_insert(format!("InnerSizeWriter: {error}"));
                            }
                        }
                    }
                    Some(1)
                }
                WinitWindowEvent::Resized(_) => Some(2),
                _ => None,
            };
            if let Some(index) = index {
                self.events[index] += 1;
                self.quiet_since = Instant::now();
                let total: usize = self.events.iter().sum();
                if total <= 32 {
                    eprintln!("leg={} native_event={event:?}", self.leg);
                }
                if total > 128 {
                    self.failure.get_or_insert(format!(
                        "leg={} native geometry event flood: {:?}",
                        self.leg, self.events
                    ));
                }
            }
        }
        self.state.window_event(event_loop, id, event);
        if self.failure.is_some() {
            self.state.begin_shutdown(event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: WinitUserEvent) {
        self.state.user_event(event_loop, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.state.about_to_wait(event_loop);
        if self.original.is_some() && !self.state.shutdown.is_started() {
            if let Err(error) = self.observe() {
                self.failure = Some(error);
            }
            if self.failure.is_some() || self.leg == LEGS.len() {
                self.state.begin_shutdown(event_loop);
            }
        }
    }
}

#[test]
#[ignore = "requires two mixed-DPI monitors and an interactive Windows desktop; run in its own nextest process"]
fn native_mixed_dpi_geometry_controls_and_retirement() {
    if std::env::var_os("NARA_WINIT_GEOMETRY_CHILD").is_none() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_tests::geometry::native_mixed_dpi_geometry_controls_and_retirement",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("NARA_WINIT_GEOMETRY_CHILD", "1")
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(35);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "native geometry child failed: {status}");
                return;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!(
                    "native geometry child hung; watchdog terminated it; native retirement is unproven"
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let move_failed = Rc::new(Cell::new(false));
    let hook_failure = move_failed.clone();
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true).with_msg_hook(move |message| {
        let message = unsafe { &*message.cast::<MSG>() };
        if message.message != MOVE && message.message != MARKED_MOVE {
            return false;
        }
        let marker = message.message == MARKED_MOVE;
        unsafe {
            if marker {
                SendMessageW(message.hwnd, WM_ENTERSIZEMOVE, 0, 0);
            }
            if SetWindowPos(
                message.hwnd,
                std::ptr::null_mut(),
                message.wParam as i32,
                message.lParam as i32,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
            ) == 0
            {
                eprintln!("SetWindowPos: {}", std::io::Error::last_os_error());
                hook_failure.set(true);
            }
            if marker {
                SendMessageW(message.hwnd, WM_EXITSIZEMOVE, 0, 0);
            }
        }
        true
    });
    let mut event_loop = builder.build().unwrap();
    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: Some(Window::new(
            "nara mixed-DPI programmatic controls",
            WindowResolution::new(480, 320),
        )),
    })
    .unwrap();
    let mut state = WinitApp::new(
        RuntimeInstance::start(app).unwrap(),
        Box::new(NoPresentationDriver),
        WinitControlFlow::Wait,
        DEFAULT_NATIVE_RETIREMENT_BUDGET,
    );
    state.event_proxy = Some(event_loop.create_proxy());
    let generation = state.runtime.generation();
    let now = Instant::now();
    let mut probe = GeometryProbe {
        state,
        original: None,
        monitors: Vec::new(),
        pair: [0; 2],
        leg: 0,
        started: now,
        quiet_since: now,
        last: None,
        events: [0; 3],
        writer_calls: 0,
        move_failed,
        failure: None,
    };
    let deadline = now + Duration::from_secs(25);
    loop {
        if let PumpStatus::Exit(_) =
            event_loop.pump_app_events(Some(Duration::from_millis(10)), &mut probe)
        {
            probe
                .failure
                .get_or_insert("the original native pump exited prematurely".into());
            break;
        }
        if Instant::now() >= deadline {
            probe
                .failure
                .get_or_insert("geometry probe exceeded its finite deadline".into());
            probe.state.begin_shutdown_transition(Instant::now());
        }
        if probe.state.shutdown == WinitShutdownState::Complete
            || probe.state.retirement_drive_incomplete(Instant::now())
        {
            break;
        }
    }
    probe.state.begin_shutdown_transition(Instant::now());
    let mut event_loop = WinitEventLoop::from_native(event_loop);
    event_loop.resumed = probe.state.native_resumed;
    let mut session = WinitSession {
        app: Some(probe.state),
        event_loop: Some(event_loop),
        executed: true,
    };
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    let state = session.app.as_ref().unwrap();
    assert_eq!(state.runtime.generation(), generation);
    assert_eq!(state.runtime.state(), RuntimeState::Stopped);
    assert_eq!(state.shutdown, WinitShutdownState::Complete);
    assert!(state.platform_windows.is_empty());
    assert!(state.nara_windows_by_winit.is_empty());
    if let Some((_, weak)) = &probe.original {
        let target = state.backend_windows.snapshot(WindowId::PRIMARY).unwrap();
        assert_eq!(target.phase, WindowTargetPhase::NativeDestroyed);
        assert_eq!(target.fault, None);
        assert!(!target.provider_present);
        assert!(!target.surface_active);
        assert!(weak.upgrade().is_none());
    }
    assert!(
        state.failure().is_none(),
        "native shutdown failed: {:?}",
        state.failure()
    );
    let retired = session.into_retired().unwrap();
    assert!(retired.event_loop.is_some());
    assert_eq!(retired.result, Ok(AppExit::Success));
    assert!(
        probe.failure.is_none(),
        "native geometry observation failed after retirement: {:?}",
        probe.failure
    );
    assert_eq!(
        probe.leg,
        LEGS.len(),
        "the complete programmatic trajectory was not observed"
    );
}
