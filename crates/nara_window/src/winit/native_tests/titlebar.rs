//! Actual SendInput title-bar dragging of one original HWND across a real mixed-DPI pair.
//!
//! A bounded helper supplies mouse input while Windows owns the modal move loop. This is
//! injected OS input rather than human-hand or GPU presentation evidence. The final two drags
//! also require explicit InnerSizeWriter overrides in both directions.

use super::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::{
        Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow},
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_ABSOLUTE,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK,
            MOUSEINPUT, SendInput, VK_LBUTTON, VK_RBUTTON,
        },
        WindowsAndMessaging::{
            GetClientRect, GetForegroundWindow, GetPhysicalCursorPos, GetSystemMetrics,
            GetWindowRect, HTCAPTION, IsWindow, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
            SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SMTO_ABORTIFHUNG, SMTO_BLOCK,
            SendMessageTimeoutW, SendMessageW, SetForegroundWindow, SetPhysicalCursorPos,
            WM_NCHITTEST, WindowFromPoint,
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

const EXTENT: [f64; 2] = [360.0, 240.0];
const OVERRIDE: [f64; 2] = [424.0, 272.0];
const LEGS: usize = 5;
const QUIET: Duration = Duration::from_millis(300);
const LEG_BUDGET: Duration = Duration::from_secs(7);
const INPUT_BUDGET: Duration = Duration::from_secs(4);
const TEST_NAME: &str = "native_tests::titlebar::native_mixed_dpi_titlebar_drag_and_retirement";
const CURSOR_OWNED: &str = "NARA_TITLEBAR_CURSOR_OWNED";
const BUTTON_OWNED: &str = "NARA_TITLEBAR_BUTTON_OWNED";
const BUTTON_RELEASED: &str = "NARA_TITLEBAR_BUTTON_RELEASED";
const INPUT_RESTORED: &str = "NARA_TITLEBAR_INPUT_RESTORED";

fn notify_parent(message: &str) -> Result<(), String> {
    let mut output = std::io::stdout().lock();
    writeln!(output, "\n{message}")
        .and_then(|()| output.flush())
        .map_err(|error| format!("titlebar input ownership notification: {error}"))
}

fn native_hwnd(window: &WinitWindow) -> Result<usize, String> {
    match window
        .window_handle()
        .map_err(|error| error.to_string())?
        .as_raw()
    {
        RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get() as usize),
        other => Err(format!("expected the original Win32 HWND, got {other:?}")),
    }
}

fn cursor_position() -> Result<[i32; 2], String> {
    let mut point = POINT::default();
    // SAFETY: GetPhysicalCursorPos receives writable storage and is independent of DPI awareness.
    if unsafe { GetPhysicalCursorPos(&mut point) } == 0 {
        return Err(format!(
            "GetPhysicalCursorPos: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok([point.x, point.y])
}

fn mouse_buttons_up() -> bool {
    // SAFETY: Both virtual-key codes are valid; only the current high-bit state is read.
    unsafe {
        GetAsyncKeyState(i32::from(VK_LBUTTON)) >= 0 && GetAsyncKeyState(i32::from(VK_RBUTTON)) >= 0
    }
}

fn mouse_input(flags: MOUSE_EVENT_FLAGS, position: [i32; 2]) -> Result<(), String> {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: position[0],
                dy: position[1],
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0x4e415241,
            },
        },
    };
    // SAFETY: The tag matches the initialized union member and INPUT has the advertised size.
    if unsafe { SendInput(1, &input, size_of::<INPUT>() as i32) } != 1 {
        return Err(format!(
            "SendInput flags={flags:#x}: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

struct MouseRestore {
    original: [i32; 2],
    moved: bool,
    pressed: bool,
}

impl MouseRestore {
    fn restore(&mut self) -> Result<(), String> {
        if self.pressed {
            mouse_input(MOUSEEVENTF_LEFTUP, [0; 2])?;
            self.pressed = false;
            notify_parent(BUTTON_RELEASED)?;
        }
        if self.moved {
            // SAFETY: These are the physical desktop coordinates captured before test input.
            if unsafe { SetPhysicalCursorPos(self.original[0], self.original[1]) } == 0 {
                return Err(format!(
                    "SetPhysicalCursorPos cleanup: {}",
                    std::io::Error::last_os_error()
                ));
            }
            if cursor_position()? != self.original {
                return Err("the original cursor position was not restored".into());
            }
            self.moved = false;
            notify_parent(INPUT_RESTORED)?;
        }
        Ok(())
    }
}

impl Drop for MouseRestore {
    fn drop(&mut self) {
        if let Err(error) = self.restore() {
            eprintln!("titlebar input cleanup failed: {error}");
        }
    }
}

fn input_pause(cancel: &AtomicBool, deadline: Instant, duration: Duration) -> Result<(), String> {
    let until = Instant::now() + duration;
    loop {
        if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
            return Err("titlebar input cancelled or exceeded its finite deadline".into());
        }
        if Instant::now() >= until {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn hit_test_position(position: [i32; 2]) -> Result<isize, String> {
    let x = i16::try_from(position[0]).map_err(|_| "caption x exceeds WM_NCHITTEST coordinates")?;
    let y = i16::try_from(position[1]).map_err(|_| "caption y exceeds WM_NCHITTEST coordinates")?;
    Ok((u32::from(x as u16) | (u32::from(y as u16) << 16)) as i32 as isize)
}

fn drag_input(
    native: usize,
    caption: [i32; 2],
    destination: [i32; 2],
    original_cursor: [i32; 2],
    cancel: &AtomicBool,
) -> Result<(), String> {
    let hwnd = native as HWND;
    let deadline = Instant::now() + INPUT_BUDGET;
    if !mouse_buttons_up() {
        return Err(
            "refusing titlebar injection while an existing left/right button is held".into(),
        );
    }
    // SAFETY: The borrowed original HWND is used only for native queries; its owner retains it.
    let desktop = unsafe {
        [
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        ]
    };
    if desktop[2] <= 1 || desktop[3] <= 1 {
        return Err(format!("invalid physical virtual desktop: {desktop:?}"));
    }
    let move_cursor = |position: [i32; 2]| -> Result<(), String> {
        let mut normalized = [0; 2];
        for axis in 0..2 {
            let offset = i64::from(position[axis]) - i64::from(desktop[axis]);
            let extent = i64::from(desktop[axis + 2]);
            if !(0..extent).contains(&offset) {
                return Err(format!(
                    "cursor target {position:?} lies outside desktop {desktop:?}"
                ));
            }
            normalized[axis] = ((offset * 65536 + 32768) / extent) as i32;
        }
        mouse_input(
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
            normalized,
        )
    };
    let mut restore = MouseRestore {
        original: original_cursor,
        moved: false,
        pressed: false,
    };
    input_pause(cancel, deadline, Duration::ZERO)?;
    // SAFETY: These queries do not transfer or destroy the original HWND.
    if unsafe { IsWindow(hwnd) } == 0 || unsafe { GetForegroundWindow() } != hwnd {
        return Err("the original HWND is not the live foreground window before drag".into());
    }
    restore.moved = true;
    notify_parent(CURSOR_OWNED)?;
    move_cursor(caption)?;
    input_pause(cancel, deadline, Duration::from_millis(100))?;
    let actual = cursor_position()?;
    if actual != caption {
        return Err(format!(
            "caption cursor mismatch: requested={caption:?}, actual={actual:?}"
        ));
    }
    let point = POINT {
        x: actual[0],
        y: actual[1],
    };
    let mut hit = 0;
    // SAFETY: The query carries packed screen coordinates and writable result storage. A finite
    // timeout prevents the helper from depending on an unresponsive owner thread.
    let responded = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_NCHITTEST,
            0,
            hit_test_position(actual)?,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            200,
            &mut hit,
        )
    };
    // SAFETY: WindowFromPoint reads a value POINT without retaining storage.
    if responded == 0 || hit != HTCAPTION as usize || unsafe { WindowFromPoint(point) } != hwnd {
        return Err(format!(
            "original HWND caption validation failed: response={responded}, hit={hit}"
        ));
    }
    if !mouse_buttons_up() {
        return Err("an existing left/right button became held before titlebar mouse-down".into());
    }
    restore.pressed = true;
    notify_parent(BUTTON_OWNED)?;
    mouse_input(MOUSEEVENTF_LEFTDOWN, [0; 2])?;
    eprintln!(
        "SendInput titlebar down HWND={native:#x}, caption={caption:?}, destination={destination:?}, desktop={desktop:?}"
    );
    input_pause(cancel, deadline, Duration::from_millis(100))?;
    for step in 1..=40 {
        input_pause(cancel, deadline, Duration::from_millis(20))?;
        // SAFETY: The original HWND remains owned by the pumping test, and the key code is valid.
        if unsafe { IsWindow(hwnd) } == 0
            || unsafe { GetForegroundWindow() } != hwnd
            || unsafe { GetAsyncKeyState(i32::from(VK_RBUTTON)) } < 0
            || unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON)) } >= 0
        {
            return Err("original window or held-button state changed during titlebar drag".into());
        }
        let position = std::array::from_fn(|axis| {
            (i64::from(caption[axis])
                + (i64::from(destination[axis]) - i64::from(caption[axis])) * step / 40)
                as i32
        });
        move_cursor(position)?;
        if step % 10 == 0 {
            let mut outer = RECT::default();
            let mut client = RECT::default();
            // SAFETY: Both RECT pointers reference writable local storage; the other calls query HWND.
            let facts = unsafe {
                (
                    GetWindowRect(hwnd, &mut outer),
                    GetClientRect(hwnd, &mut client),
                    GetDpiForWindow(hwnd),
                    MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST),
                )
            };
            eprintln!(
                "held step={step}, cursor={:?}, native_calls=({},{}), outer={:?}, client={:?}, dpi={}, monitor={:?}",
                cursor_position()?,
                facts.0,
                facts.1,
                [outer.left, outer.top, outer.right, outer.bottom],
                [client.left, client.top, client.right, client.bottom],
                facts.2,
                facts.3
            );
        }
    }
    input_pause(cancel, deadline, Duration::from_millis(120))?;
    if cursor_position()? != destination {
        return Err(format!(
            "held cursor did not reach physical destination {destination:?}"
        ));
    }
    mouse_input(MOUSEEVENTF_LEFTUP, [0; 2])?;
    restore.pressed = false;
    input_pause(cancel, deadline, Duration::from_millis(100))?;
    if !mouse_buttons_up() {
        return Err("mouse buttons remain held after injected titlebar release".into());
    }
    notify_parent(BUTTON_RELEASED)?;
    restore.restore()?;
    eprintln!(
        "SendInput titlebar released HWND={native:#x}; cursor restored to {original_cursor:?}"
    );
    Ok(())
}

struct DragTask {
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), String>>>,
}

impl DragTask {
    fn stop(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    fn finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }

    fn join(&mut self) -> Result<(), String> {
        self.worker
            .take()
            .ok_or("missing titlebar input worker")?
            .join()
            .map_err(|_| "titlebar input worker panicked".to_owned())?
    }
}

impl Drop for DragTask {
    fn drop(&mut self) {
        self.stop();
        if self.worker.is_some()
            && let Err(error) = self.join()
        {
            eprintln!("titlebar worker cleanup: {error}");
        }
    }
}

struct DragMonitor {
    monitor: MonitorHandle,
    work: [i32; 4],
}

#[derive(Debug, PartialEq)]
struct DragGeometry {
    position: PhysicalPosition<i32>,
    outer: PhysicalSize<u32>,
    inner: PhysicalSize<u32>,
    scale: f64,
    dpi: u32,
    monitor: usize,
    native_outer: [i32; 4],
    native_client: [i32; 4],
}

struct TitlebarProbe {
    state: WinitApp,
    original: Option<(WinitWindowId, usize, Weak<WinitWindow>)>,
    monitors: Vec<DragMonitor>,
    pair: [usize; 2],
    original_cursor: [i32; 2],
    leg: usize,
    started: Instant,
    quiet_since: Instant,
    last: Option<DragGeometry>,
    events: [usize; 3],
    writer_calls: usize,
    destroyed: usize,
    task: Option<DragTask>,
    failure: Option<String>,
}

impl TitlebarProbe {
    fn expected_extent(&self) -> [f64; 2] {
        if self.leg == 3 { OVERRIDE } else { EXTENT }
    }

    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let window = self
            .state
            .platform_windows
            .get(&WindowId::PRIMARY)
            .ok_or("missing original titlebar window")?;
        let native = native_hwnd(window)?;
        self.original = Some((window.id(), native, Arc::downgrade(window)));
        for monitor in event_loop.available_monitors() {
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            // SAFETY: Winit supplies the live monitor handle; MONITORINFO has the correct size.
            if unsafe { GetMonitorInfoW(monitor.hmonitor() as _, &mut info) } == 0 {
                return Err(format!(
                    "GetMonitorInfoW: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let work = [
                info.rcWork.left,
                info.rcWork.top,
                info.rcWork.right,
                info.rcWork.bottom,
            ];
            eprintln!(
                "titlebar monitor={:?}, native={:?}, physical_position={:?}, physical_size={:?}, work={work:?}, scale={}",
                monitor.name(),
                monitor.hmonitor(),
                monitor.position(),
                monitor.size(),
                monitor.scale_factor()
            );
            self.monitors.push(DragMonitor { monitor, work });
        }
        let source = self
            .monitors
            .iter()
            .position(|m| Some(m.monitor.clone()) == window.current_monitor())
            .ok_or("original HWND has no inventoried current monitor")?;
        let target = self
            .monitors
            .iter()
            .position(|m| {
                (m.monitor.scale_factor() - self.monitors[source].monitor.scale_factor()).abs()
                    > 0.001
            })
            .ok_or("requires two real monitors with different DPI; no mixed-DPI pair found")?;
        self.pair = [source, target];
        let [left, top, right, bottom] = self.monitors[source].work;
        let scale = self.monitors[source].monitor.scale_factor();
        let width = (EXTENT[0] * scale).ceil() as i32 + 64;
        let height = (EXTENT[1] * scale).ceil() as i32 + 96;
        if right - left < width || bottom - top < height {
            return Err("source work area cannot contain the titlebar probe".into());
        }
        let _ = window.request_inner_size(LogicalSize::new(EXTENT[0], EXTENT[1]));
        window.set_outer_position(PhysicalPosition::new(
            left + (right - left - width) / 2,
            top + (bottom - top - height) / 2,
        ));
        window.focus_window();
        // SAFETY: The original HWND is retained on its owner thread; failure is checked before injection.
        let foreground = unsafe { SetForegroundWindow(native as HWND) };
        eprintln!(
            "titlebar original HWND={native:#x}, source={:?}, target={:?}, SetForegroundWindow={foreground}",
            self.monitors[source].monitor.name(),
            self.monitors[target].monitor.name()
        );
        self.started = Instant::now();
        self.quiet_since = self.started;
        Ok(())
    }

    fn start_drag(&mut self) -> Result<(), String> {
        let window = self
            .state
            .platform_windows
            .get(&WindowId::PRIMARY)
            .ok_or("original window disappeared")?;
        let (_, native, _) = self
            .original
            .as_ref()
            .ok_or("missing original HWND identity")?;
        if native_hwnd(window)? != *native {
            return Err("titlebar drag replaced the original HWND".into());
        }
        let outer = window.outer_position().map_err(|error| error.to_string())?;
        let inner = window.inner_position().map_err(|error| error.to_string())?;
        let x = outer.x + window.outer_size().width as i32 / 2;
        let mut caption = None;
        for y in (outer.y + 4..inner.y).step_by(2) {
            let position = [x, y];
            // SAFETY: A synchronous hit-test on this thread's original window uses packed screen coordinates.
            if unsafe {
                SendMessageW(
                    *native as HWND,
                    WM_NCHITTEST,
                    0,
                    hit_test_position(position)?,
                )
            } == HTCAPTION as isize
            {
                caption = Some(position);
                break;
            }
        }
        let caption =
            caption.ok_or("the original decorated HWND has no validated caption grab point")?;
        let target = &self.monitors[self.pair[self.leg % 2]];
        let scale_ratio = target.monitor.scale_factor() / window.scale_factor();
        let outer_size = window.outer_size();
        let width = (f64::from(outer_size.width) * scale_ratio).ceil() as i32;
        let height = (f64::from(outer_size.height) * scale_ratio).ceil() as i32;
        let [left, top, right, bottom] = target.work;
        if right - left < width + 64 || bottom - top < height + 64 {
            return Err(format!(
                "target work area too small for titlebar drag: {:?}",
                target.work
            ));
        }
        let destination = [
            left + (right - left) / 2,
            top + (bottom - top - height) / 2
                + (f64::from(caption[1] - outer.y) * scale_ratio).round() as i32,
        ];
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let native = *native;
        let original_cursor = self.original_cursor;
        let worker = thread::Builder::new()
            .name("nara-titlebar-input".into())
            .spawn(move || {
                drag_input(
                    native,
                    caption,
                    destination,
                    original_cursor,
                    &worker_cancel,
                )
            })
            .map_err(|error| format!("start titlebar input worker: {error}"))?;
        self.task = Some(DragTask {
            cancel,
            worker: Some(worker),
        });
        self.started = Instant::now();
        self.quiet_since = self.started;
        self.last = None;
        self.events = [0; 3];
        self.writer_calls = 0;
        eprintln!(
            "titlebar leg={}, original HWND={native:#x}, target={:?}, target_work={:?}",
            self.leg,
            target.monitor.name(),
            target.work
        );
        Ok(())
    }

    fn observe(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        if self.started.elapsed() >= LEG_BUDGET {
            self.failure.get_or_insert(format!(
                "titlebar leg={} exceeded its finite deadline",
                self.leg
            ));
        }
        if self.failure.is_some() || self.state.shutdown.is_started() {
            if let Some(task) = &self.task {
                task.stop();
            }
        }
        if self.task.as_ref().is_some_and(DragTask::finished) {
            let mut task = self.task.take().unwrap();
            if let Err(error) = task.join() {
                self.failure.get_or_insert(error);
            }
            self.quiet_since = Instant::now();
        }
        if self.failure.is_some() {
            if self.task.is_none() {
                self.state.begin_shutdown(event_loop);
            }
            return Ok(());
        }
        if self.state.shutdown.is_started() || self.leg == LEGS {
            return Ok(());
        }
        let window = self
            .state
            .platform_windows
            .get(&WindowId::PRIMARY)
            .ok_or("original titlebar HWND disappeared")?;
        let (id, native, _) = self
            .original
            .as_ref()
            .ok_or("missing original HWND identity")?;
        if window.id() != *id || native_hwnd(window)? != *native {
            return Err("the original Winit window/HWND identity changed".into());
        }
        let mut outer = RECT::default();
        let mut client = RECT::default();
        // SAFETY: Native queries use the retained original HWND and writable RECT storage.
        let (outer_ok, client_ok, dpi, monitor) = unsafe {
            (
                GetWindowRect(*native as HWND, &mut outer),
                GetClientRect(*native as HWND, &mut client),
                GetDpiForWindow(*native as HWND),
                MonitorFromWindow(*native as HWND, MONITOR_DEFAULTTONEAREST),
            )
        };
        if outer_ok == 0 || client_ok == 0 || dpi == 0 {
            return Err(format!(
                "original HWND geometry query failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        let geometry = DragGeometry {
            position: window.outer_position().map_err(|error| error.to_string())?,
            outer: window.outer_size(),
            inner: window.inner_size(),
            scale: window.scale_factor(),
            dpi,
            monitor: monitor as usize,
            native_outer: [outer.left, outer.top, outer.right, outer.bottom],
            native_client: [client.left, client.top, client.right, client.bottom],
        };
        if self.last.as_ref() != Some(&geometry) {
            eprintln!("titlebar leg={} geometry={geometry:?}", self.leg);
            self.last = Some(geometry);
            self.quiet_since = Instant::now();
        }
        if self.task.is_some() || self.quiet_since.elapsed() < QUIET {
            return Ok(());
        }
        let geometry = self.last.as_ref().unwrap();
        let target = &self.monitors[self.pair[self.leg % 2]].monitor;
        let extent = self.expected_extent();
        let logical = geometry.inner.to_logical::<f64>(geometry.scale);
        let resolution = configured_primary_window(&self.state.runtime)
            .map_err(|error| error.to_string())?
            .resolution;
        let valid = geometry.monitor == target.hmonitor() as usize
            && window.current_monitor().as_ref() == Some(target)
            && (geometry.scale - target.scale_factor()).abs() < 0.001
            && (f64::from(geometry.dpi) / 96.0 - geometry.scale).abs() < 0.001
            && (logical.width - extent[0]).abs() <= 3.0
            && (logical.height - extent[1]).abs() <= 3.0
            && client.right - client.left == geometry.inner.width as i32
            && client.bottom - client.top == geometry.inner.height as i32
            && resolution.physical_width == geometry.inner.width
            && resolution.physical_height == geometry.inner.height
            && (resolution.scale_factor - geometry.scale).abs() < 0.001
            && !window.is_maximized()
            && (self.leg == 0 || self.events.iter().all(|count| *count > 0))
            && (self.leg < 3 || self.writer_calls > 0);
        eprintln!(
            "titlebar settled leg={}, target={:?}, geometry={geometry:?}, logical={logical:?}, Runtime={resolution:?}, Moved/ScaleFactorChanged/Resized={:?}, valid={valid}",
            self.leg,
            target.name(),
            self.events
        );
        if !valid {
            return Err(format!(
                "titlebar leg={} failed original geometry/DPI/Runtime/no-inflation checks",
                self.leg
            ));
        }
        self.leg += 1;
        if self.leg < LEGS {
            self.start_drag()?;
        } else {
            self.state.begin_shutdown(event_loop);
        }
        Ok(())
    }
}

impl ApplicationHandler<WinitUserEvent> for TitlebarProbe {
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
        {
            if matches!(&event, WinitWindowEvent::Destroyed) {
                self.destroyed += 1;
            }
            if self.leg < LEGS && !self.state.shutdown.is_started() {
                let index = match &mut event {
                    WinitWindowEvent::Moved(_) => Some(0),
                    WinitWindowEvent::ScaleFactorChanged {
                        scale_factor,
                        inner_size_writer,
                    } => {
                        if self.leg >= 3 {
                            let extent = self.expected_extent();
                            let size = LogicalSize::new(extent[0], extent[1])
                                .to_physical::<u32>(*scale_factor);
                            match inner_size_writer.request_inner_size(size) {
                                Ok(()) => self.writer_calls += 1,
                                Err(error) => {
                                    self.failure.get_or_insert(format!(
                                        "titlebar InnerSizeWriter: {error}"
                                    ));
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
                    if total <= 128 {
                        eprintln!("titlebar leg={} native_event={event:?}", self.leg);
                    }
                    if total > 256 {
                        self.failure
                            .get_or_insert("native titlebar geometry event flood".into());
                    }
                }
            }
        }
        if self.failure.is_some()
            || matches!(
                &event,
                WinitWindowEvent::CloseRequested | WinitWindowEvent::Destroyed
            )
        {
            if let Some(task) = &self.task {
                task.stop();
            }
        }
        self.state.window_event(event_loop, id, event);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: WinitUserEvent) {
        self.state.user_event(event_loop, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.state.about_to_wait(event_loop);
        if self.original.is_some()
            && let Err(error) = self.observe(event_loop)
        {
            self.failure = Some(error);
            if let Some(task) = &self.task {
                task.stop();
            } else {
                self.state.begin_shutdown(event_loop);
            }
        }
    }
}

#[test]
#[ignore = "injects actual OS mouse input; requires an idle interactive Windows desktop and two mixed-DPI monitors; run alone"]
fn native_mixed_dpi_titlebar_drag_and_retirement() {
    assert!(
        mouse_buttons_up(),
        "refusing injection while an existing left/right mouse button is held"
    );
    let original_cursor = cursor_position().unwrap();
    if std::env::var_os("NARA_WINIT_TITLEBAR_CHILD").is_none() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                TEST_NAME,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("NARA_WINIT_TITLEBAR_CHILD", "1")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let output = child.stdout.take().unwrap();
        let owned = Arc::new([AtomicBool::new(false), AtomicBool::new(false)]);
        let child_owned = owned.clone();
        let reader = thread::spawn(move || -> std::io::Result<()> {
            for line in BufReader::new(output).lines() {
                let line = line?;
                match line.as_str() {
                    CURSOR_OWNED => child_owned[0].store(true, Ordering::Release),
                    BUTTON_OWNED => child_owned[1].store(true, Ordering::Release),
                    BUTTON_RELEASED => child_owned[1].store(false, Ordering::Release),
                    INPUT_RESTORED => {
                        child_owned[0].store(false, Ordering::Release);
                        child_owned[1].store(false, Ordering::Release);
                    }
                    _ => {}
                }
                println!("{line}");
            }
            Ok(())
        });
        let deadline = Instant::now() + Duration::from_secs(40);
        let (status, timed_out) = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break (status, false);
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                break (child.wait().unwrap(), true);
            }
            thread::sleep(Duration::from_millis(10));
        };
        let output_result = reader.join();
        let mut restore = MouseRestore {
            original: original_cursor,
            moved: owned[0].load(Ordering::Acquire),
            pressed: owned[1].load(Ordering::Acquire),
        };
        let cleanup = restore.restore();
        assert!(
            !timed_out,
            "native titlebar child hung; input_cleanup={cleanup:?}; native retirement is unproven"
        );
        assert!(
            cleanup.is_ok(),
            "parent titlebar input cleanup: {cleanup:?}"
        );
        output_result
            .expect("titlebar ownership reader panicked")
            .expect("titlebar ownership pipe failed");
        assert!(status.success(), "native titlebar child failed: {status}");
        return;
    }
    let mut builder = EventLoop::<WinitUserEvent>::with_user_event();
    builder.with_any_thread(true);
    let mut event_loop = builder.build().unwrap();
    let mut app = App::new();
    app.add_plugin(WindowPlugin {
        primary_window: Some(Window::new(
            "nara injected OS titlebar drag",
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
    let mut probe = TitlebarProbe {
        state,
        original: None,
        monitors: Vec::new(),
        pair: [0; 2],
        original_cursor,
        leg: 0,
        started: now,
        quiet_since: now,
        last: None,
        events: [0; 3],
        writer_calls: 0,
        destroyed: 0,
        task: None,
        failure: None,
    };
    let deadline = now + Duration::from_secs(25);
    loop {
        if let PumpStatus::Exit(_) =
            event_loop.pump_app_events(Some(Duration::from_millis(10)), &mut probe)
        {
            probe
                .failure
                .get_or_insert("the original titlebar native pump exited prematurely".into());
            break;
        }
        if Instant::now() >= deadline {
            probe
                .failure
                .get_or_insert("titlebar probe exceeded its finite deadline".into());
            if let Some(task) = &probe.task {
                task.stop();
            } else {
                probe.state.begin_shutdown_transition(Instant::now());
            }
        }
        if probe.state.shutdown == WinitShutdownState::Complete
            || probe.state.retirement_drive_incomplete(Instant::now())
        {
            break;
        }
    }
    if let Some(mut task) = probe.task.take() {
        task.stop();
        if let Err(error) = task.join() {
            probe.failure.get_or_insert(error);
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
    if let Some((_, _, weak)) = &probe.original {
        let target = state.backend_windows.snapshot(WindowId::PRIMARY).unwrap();
        assert_eq!(target.phase, WindowTargetPhase::NativeDestroyed);
        assert_eq!(target.fault, None);
        assert!(!target.provider_present);
        assert!(!target.surface_active);
        assert!(weak.upgrade().is_none());
        assert_eq!(
            probe.destroyed, 1,
            "the original HWND must deliver its actual Destroyed event"
        );
    }
    assert!(
        state.failure().is_none(),
        "native titlebar shutdown failed: {:?}",
        state.failure()
    );
    let retired = session.into_retired().unwrap();
    assert!(retired.event_loop.is_some());
    assert_eq!(retired.result, Ok(AppExit::Success));
    assert!(
        probe.failure.is_none(),
        "native titlebar observation failed after retirement: {:?}",
        probe.failure
    );
    assert_eq!(
        probe.leg, LEGS,
        "the original HWND did not complete default and explicit-size OS titlebar drag directions"
    );
    assert!(mouse_buttons_up(), "an injected mouse button remained held");
    assert_eq!(cursor_position().unwrap(), original_cursor);
}
