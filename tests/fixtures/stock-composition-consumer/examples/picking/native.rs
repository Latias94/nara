#![cfg(windows)]
#![deny(unsafe_op_in_unsafe_fn)]

//! Borrowed automation of this process's original Winit window, after DPI initialization.
//!
//! The caller owns display selection and the DesktopRun lifecycle. Use this probe on the
//! window's thread while it is live, then call `verify_destroyed` only after retirement.
//! Moving a window requests native events; it does not establish presentation or picking.

use std::{io, mem::size_of, ptr};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONULL, MONITORINFO,
        MonitorFromWindow,
    },
    System::Threading::{GetCurrentProcessId, GetCurrentThreadId},
    UI::{
        HiDpi::{
            DPI_AWARENESS_PER_MONITOR_AWARE, GetAwarenessFromDpiAwarenessContext, GetDpiForMonitor,
            GetDpiForWindow, GetThreadDpiAwarenessContext, MDT_EFFECTIVE_DPI,
        },
        WindowsAndMessaging::{
            FindWindowW, GetClientRect, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
            IsWindow, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
        },
    },
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Monitor {
    /// Physical desktop bounds: left, top, right, bottom; maximum edges are exclusive.
    pub physical: [i32; 4],
    /// Physical work area in the same desktop coordinates.
    pub work: [i32; 4],
    pub dpi: u32,
    pub scale: f64,
    id: HMONITOR,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// Physical client extent, excluding the native frame.
    pub size: [u32; 2],
    /// Physical outer-window origin in desktop coordinates, including the native frame.
    pub position: [i32; 2],
    pub scale: f64,
    pub dpi: u32,
    /// Index in this probe's original monitor snapshot.
    pub monitor: usize,
}

pub struct NativeProbe {
    window: HWND,
    title: Vec<u16>,
    monitors: Vec<Monitor>,
}

impl NativeProbe {
    /// Find only an exact, process-unique title owned by the calling process and thread.
    ///
    /// Call after Winit has initialized per-monitor DPI awareness and created the window.
    /// A title collision with another process is a failure, never an automation target.
    pub fn find_exact(title: &str) -> Result<Self, String> {
        require_per_monitor_dpi()?;
        if title.is_empty() || title.contains('\0') {
            return Err("the native probe requires a nonempty title without NUL characters".into());
        }
        let mut title: Vec<u16> = title.encode_utf16().collect();
        title.push(0);
        i32::try_from(title.len() + 1).map_err(|_| "native window title is too long")?;
        // SAFETY: title is NUL-terminated UTF-16 and lives through this synchronous call;
        // a null class searches top-level windows without changing any of them.
        let window = unsafe { FindWindowW(ptr::null(), title.as_ptr()) };
        if window.is_null() {
            return Err("FindWindowW did not find the requested original window".into());
        }
        let mut probe = Self {
            window,
            title,
            monitors: Vec::new(),
        };
        probe.validate_window()?;
        probe.monitors = monitor_inventory()?;
        Ok(probe)
    }

    /// The immutable inventory captured at discovery; this performs no native operation.
    pub fn monitors(&self) -> &[Monitor] {
        &self.monitors
    }

    pub fn current_index(&self) -> Result<usize, String> {
        self.validate_window()?;
        // SAFETY: the original borrowed HWND was just validated on its owning thread.
        // MONITOR_DEFAULTTONULL reports absence instead of substituting another display.
        let monitor = unsafe { MonitorFromWindow(self.window, MONITOR_DEFAULTTONULL) };
        self.monitors
            .iter()
            .position(|candidate| candidate.id == monitor)
            .ok_or_else(|| {
                "the original window is not on a monitor in the original inventory".into()
            })
    }

    pub fn geometry(&self) -> Result<Geometry, String> {
        self.validate_window()?;
        let mut client = RECT::default();
        // SAFETY: the original HWND is validated and client is writable for one RECT.
        if unsafe { GetClientRect(self.window, &mut client) } == 0 {
            return Err(native_error("GetClientRect"));
        }
        let size = [
            u32::try_from(i64::from(client.right) - i64::from(client.left))
                .map_err(|_| "native client width is invalid")?,
            u32::try_from(i64::from(client.bottom) - i64::from(client.top))
                .map_err(|_| "native client height is invalid")?,
        ];
        if size.contains(&0) {
            return Err("the original native client has an empty physical extent".into());
        }
        let outer = self.outer_rect()?;
        self.validate_window()?;
        // SAFETY: this is the same validated original HWND, queried without mutation.
        let dpi = unsafe { GetDpiForWindow(self.window) };
        if dpi == 0 {
            return Err("GetDpiForWindow returned no DPI for the original window".into());
        }
        Ok(Geometry {
            size,
            position: [outer.left, outer.top],
            scale: f64::from(dpi) / 96.0,
            dpi,
            monitor: self.current_index()?,
        })
    }

    /// Center the original window in the selected work area without activation or resizing.
    ///
    /// Winit still owns any DPI-change resize and event processing. Observe subsequent real
    /// geometry and presentations in the caller; a successful request alone proves neither.
    pub fn move_to(&self, index: usize) -> Result<(), String> {
        self.validate_window()?;
        let monitor = self.monitors.get(index).ok_or_else(|| {
            format!("native monitor index {index} is outside the original inventory")
        })?;
        if read_monitor(monitor.id)? != *monitor {
            return Err("native monitor geometry or DPI changed since discovery".into());
        }
        let outer = self.outer_rect()?;
        let width = i64::from(outer.right) - i64::from(outer.left);
        let height = i64::from(outer.bottom) - i64::from(outer.top);
        let [left, top, right, bottom] = monitor.work.map(i64::from);
        if width <= 0 || height <= 0 || width > right - left || height > bottom - top {
            return Err(
                "the original native window does not fit the selected monitor work area".into(),
            );
        }
        let x = i32::try_from(left + (right - left - width) / 2)
            .map_err(|_| "native move x coordinate is out of range")?;
        let y = i32::try_from(top + (bottom - top - height) / 2)
            .map_err(|_| "native move y coordinate is out of range")?;
        self.validate_window()?;
        // SAFETY: only the original process/thread/title-validated HWND is repositioned.
        // NOZORDER ignores the null insertion handle; NOSIZE ignores the zero extents.
        // Winit retains ownership and receives normal native move/DPI notifications.
        if unsafe {
            SetWindowPos(
                self.window,
                ptr::null_mut(),
                x,
                y,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
            )
        } == 0
        {
            return Err(native_error("SetWindowPos"));
        }
        Ok(())
    }

    /// Request a physical client extent through the original HWND's normal resize path.
    pub fn resize_client(&self, size: [u32; 2]) -> Result<bool, String> {
        let current = self.geometry()?.size;
        if size == current {
            return Ok(false);
        }
        let outer = self.outer_rect()?;
        let width = i32::try_from(
            i64::from(outer.right) - i64::from(outer.left) + i64::from(size[0])
                - i64::from(current[0]),
        )
        .map_err(|_| "native outer width is out of range")?;
        let height = i32::try_from(
            i64::from(outer.bottom) - i64::from(outer.top) + i64::from(size[1])
                - i64::from(current[1]),
        )
        .map_err(|_| "native outer height is out of range")?;
        self.validate_window()?;
        // SAFETY: the same process/thread/title-validated HWND receives an ordinary resize.
        // NOMOVE and NOZORDER ignore the position and null insertion handle.
        if unsafe {
            SetWindowPos(
                self.window,
                ptr::null_mut(),
                0,
                0,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
            )
        } == 0
        {
            return Err(native_error("SetWindowPos"));
        }
        Ok(true)
    }

    /// Verify destruction after the original DesktopRun has completely retired.
    ///
    /// This queries only the saved HWND. It never searches for a successor, closes a window,
    /// or retains an owner that could prevent native destruction.
    pub fn verify_destroyed(&self) -> Result<(), String> {
        // SAFETY: IsWindow accepts stale HWND values and does not dereference a Rust pointer
        // or mutate a native window. A recycled live handle conservatively fails this check.
        if unsafe { IsWindow(self.window) } != 0 {
            return Err(
                "the original native HWND is still live after DesktopRun retirement".into(),
            );
        }
        Ok(())
    }

    fn outer_rect(&self) -> Result<RECT, String> {
        self.validate_window()?;
        let mut rectangle = RECT::default();
        // SAFETY: the original HWND is validated and rectangle is writable for one RECT.
        if unsafe { GetWindowRect(self.window, &mut rectangle) } == 0 {
            return Err(native_error("GetWindowRect"));
        }
        Ok(rectangle)
    }

    fn validate_window(&self) -> Result<(), String> {
        require_per_monitor_dpi()?;
        // SAFETY: IsWindow permits an invalid/stale borrowed handle and only queries it.
        if unsafe { IsWindow(self.window) } == 0 {
            return Err("the original native HWND is no longer live".into());
        }
        let mut pid = 0;
        // SAFETY: pid is writable, and this query accepts the original borrowed HWND.
        let thread = unsafe { GetWindowThreadProcessId(self.window, &mut pid) };
        // SAFETY: these identity queries have no pointer inputs or ownership effects.
        let own_pid = unsafe { GetCurrentProcessId() };
        // SAFETY: this returns the identity of the calling thread without mutation.
        let own_thread = unsafe { GetCurrentThreadId() };
        if thread == 0 || pid != own_pid || thread != own_thread {
            return Err(
                "the original HWND must belong to this process and the calling window thread"
                    .into(),
            );
        }
        let mut actual = vec![0; self.title.len() + 1];
        // SAFETY: actual is writable UTF-16 storage; discovery checked that its capacity
        // fits i32. No window text is changed, and this query runs on the owning thread.
        let count =
            unsafe { GetWindowTextW(self.window, actual.as_mut_ptr(), actual.len() as i32) };
        if count <= 0 || actual[..count as usize] != self.title[..self.title.len() - 1] {
            return Err("the original native HWND no longer has the exact requested title".into());
        }
        Ok(())
    }
}

fn require_per_monitor_dpi() -> Result<(), String> {
    // SAFETY: both functions only query the current thread's existing awareness context.
    let awareness = unsafe { GetAwarenessFromDpiAwarenessContext(GetThreadDpiAwarenessContext()) };
    if awareness != DPI_AWARENESS_PER_MONITOR_AWARE {
        return Err(
            "initialize Winit per-monitor DPI awareness before using the native probe".into(),
        );
    }
    Ok(())
}

fn monitor_inventory() -> Result<Vec<Monitor>, String> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: EnumDisplayMonitors calls collect_monitor synchronously with this exclusive
    // Vec pointer. The Vec outlives enumeration; null HDC/clip enumerates desktop monitors.
    if unsafe {
        EnumDisplayMonitors(
            ptr::null_mut(),
            ptr::null(),
            Some(collect_monitor),
            (&mut handles as *mut Vec<HMONITOR>) as LPARAM,
        )
    } == 0
    {
        return Err(native_error("EnumDisplayMonitors"));
    }
    if handles.is_empty() {
        return Err("EnumDisplayMonitors returned no actual desktop monitors".into());
    }
    let mut monitors = handles
        .into_iter()
        .map(read_monitor)
        .collect::<Result<Vec<_>, _>>()?;
    monitors.sort_by_key(|monitor| monitor.physical);
    Ok(monitors)
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _rectangle: *mut RECT,
    data: LPARAM,
) -> i32 {
    // SAFETY: monitor_inventory supplies an exclusive Vec pointer for this synchronous
    // callback. It remains live and is not accessed elsewhere until enumeration returns.
    let handles = unsafe { &mut *(data as *mut Vec<HMONITOR>) };
    handles.push(monitor);
    1
}

fn read_monitor(id: HMONITOR) -> Result<Monitor, String> {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    // SAFETY: id was returned by native enumeration; info has the required structure size
    // and writable storage. A removed monitor is reported as an API failure.
    if unsafe { GetMonitorInfoW(id, &mut info) } == 0 {
        return Err(native_error("GetMonitorInfoW"));
    }
    let physical = rect_edges(info.rcMonitor);
    let work = rect_edges(info.rcWork);
    if physical[0] >= physical[2]
        || physical[1] >= physical[3]
        || work[0] >= work[2]
        || work[1] >= work[3]
    {
        return Err("native monitor or work area has an empty physical extent".into());
    }
    let mut x = 0;
    let mut y = 0;
    // SAFETY: the enumerated monitor is queried after DPI initialization, and x/y are
    // writable u32 outputs. EFFECTIVE_DPI asks Windows for its actual configured scaling.
    let result = unsafe { GetDpiForMonitor(id, MDT_EFFECTIVE_DPI, &mut x, &mut y) };
    if result < 0 || x == 0 || x != y {
        return Err(format!(
            "GetDpiForMonitor(EFFECTIVE) failed: HRESULT={result:#x}, dpi=[{x}, {y}]"
        ));
    }
    Ok(Monitor {
        physical,
        work,
        dpi: x,
        scale: f64::from(x) / 96.0,
        id,
    })
}

fn rect_edges(rectangle: RECT) -> [i32; 4] {
    [
        rectangle.left,
        rectangle.top,
        rectangle.right,
        rectangle.bottom,
    ]
}

fn native_error(operation: &str) -> String {
    format!("{operation} failed: {}", io::Error::last_os_error())
}
