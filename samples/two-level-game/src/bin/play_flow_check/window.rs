//! Physical scan-code input for one exact process, main thread, HWND and window title.

use std::{
    thread,
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetKeyboardLayout, INPUT, INPUT_0, INPUT_KEYBOARD, IsWindowEnabled,
            KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW,
            SendInput, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        },
        WindowsAndMessaging::{
            EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
            IsWindow, IsWindowVisible, SetForegroundWindow,
        },
    },
};

pub(super) const TITLE: &str = "Meadow / Cavern | Play Flow";
pub(super) const ENTER: u16 = 0x1c;
pub(super) const D: u16 = 0x20;
pub(super) const ESCAPE: u16 = 0x01;
pub(super) const T: u16 = 0x14;
pub(super) const X: u16 = 0x2d;
pub(super) const E: u16 = 0x12;
const KEYS: [u16; 9] = [ENTER, D, ESCAPE, T, X, E, 0x1e, 0x11, 0x1f];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Window {
    handle: HWND,
    process: u32,
    thread: u32,
}

impl Window {
    fn observe(handle: HWND) -> Option<Self> {
        if handle.is_null() || unsafe { IsWindow(handle) } == 0 {
            return None;
        }
        let mut process = 0;
        let thread = unsafe { GetWindowThreadProcessId(handle, &mut process) };
        (process != 0 && thread != 0).then_some(Self {
            handle,
            process,
            thread,
        })
    }

    pub fn find_owned() -> Result<Option<Self>, String> {
        struct Search {
            process: u32,
            thread: u32,
            matches: Vec<Window>,
            visits: usize,
        }
        unsafe extern "system" fn visit(handle: HWND, data: LPARAM) -> i32 {
            // SAFETY: EnumWindows synchronously borrows this live, exclusively held Search.
            let search = unsafe { &mut *(data as *mut Search) };
            search.visits += 1;
            if search.visits > 16_384 {
                return 0;
            }
            if let Some(window) = Window::observe(handle)
                && window.process == search.process
            {
                let mut text = [0_u16; 512];
                let length =
                    unsafe { GetWindowTextW(handle, text.as_mut_ptr(), text.len() as i32) };
                if length > 0 && String::from_utf16_lossy(&text[..length as usize]) == TITLE {
                    search.matches.push(window);
                    if search.matches.len() > 1 || window.thread != search.thread {
                        return 0;
                    }
                }
            }
            1
        }
        let mut search = Search {
            process: std::process::id(),
            thread: unsafe { GetCurrentThreadId() },
            matches: Vec::new(),
            visits: 0,
        };
        // SAFETY: The callback and Search remain valid throughout synchronous enumeration.
        let complete = unsafe { EnumWindows(Some(visit), (&mut search as *mut Search) as LPARAM) };
        if complete == 0 {
            return Err(format!(
                "exact original main-thread window enumeration failed: pid={} matches={} visits={}",
                search.process,
                search.matches.len(),
                search.visits,
            ));
        }
        Ok(search.matches.pop())
    }

    pub fn exists(self) -> bool {
        unsafe { IsWindow(self.handle) != 0 }
    }

    fn check(self) -> Result<(), String> {
        if Self::observe(self.handle) != Some(self) || Self::find_owned()? != Some(self) {
            return Err(format!(
                "original process/main-thread/HWND/title changed: {self:?}"
            ));
        }
        if unsafe { IsWindowVisible(self.handle) } == 0
            || unsafe { IsWindowEnabled(self.handle) } == 0
            || unsafe { IsIconic(self.handle) } != 0
        {
            return Err("the exact original window is not visible, enabled and restored".into());
        }
        Ok(())
    }

    fn activate(self) -> Result<(), String> {
        if Self::observe(self.handle) != Some(self) {
            return Err("activation target lost its original native identity".into());
        }
        if unsafe { GetForegroundWindow() } != self.handle {
            unsafe { SetForegroundWindow(self.handle) };
        }
        Ok(())
    }
}

pub(super) struct Input {
    original: Option<Window>,
    target: Option<Window>,
    held: Vec<(u16, bool)>,
}

impl Input {
    pub fn new() -> Self {
        Self {
            original: Window::observe(unsafe { GetForegroundWindow() }),
            target: None,
            held: Vec::new(),
        }
    }

    pub fn bind(&mut self, window: Window) -> Result<(), String> {
        window.check()?;
        for scan in KEYS {
            if key_down(scan, window.thread)? {
                return Err(format!(
                    "acceptance key must initially be physically Up: scan={scan:#x}"
                ));
            }
        }
        for key in [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN] {
            if unsafe { GetAsyncKeyState(i32::from(key)) } < 0 {
                return Err("native acceptance requires physically released modifier keys".into());
            }
        }
        self.target = Some(window);
        window.activate()
    }

    pub fn check(&self) -> Result<(), String> {
        let target = self.target.ok_or("original native window is not bound")?;
        target.check()?;
        if unsafe { GetForegroundWindow() } != target.handle {
            return Err("physical input lost the exact original foreground HWND".into());
        }
        Ok(())
    }

    pub fn focused(&self) -> bool {
        self.target
            .is_some_and(|target| unsafe { GetForegroundWindow() } == target.handle)
    }

    pub fn down(&self, scan: u16) -> Result<bool, String> {
        key_down(scan, self.target.ok_or("no bound keyboard layout")?.thread)
    }

    pub fn press(&mut self, scan: u16) -> Result<(), String> {
        self.check()?;
        if !KEYS.contains(&scan)
            || self.held.iter().any(|(owned, _)| *owned == scan)
            || self.down(scan)?
        {
            return Err(format!(
                "scan Down requires a distinct owned Up key: {scan:#x}"
            ));
        }
        self.held.push((scan, false));
        send(scan, false)
    }

    pub fn release(&mut self, scan: u16) -> Result<(), String> {
        self.check()?;
        let release_sent = self
            .held
            .iter_mut()
            .find_map(|(owned, release_sent)| (*owned == scan).then_some(release_sent))
            .ok_or_else(|| format!("cannot release an unowned scan code: {scan:#x}"))?;
        if !*release_sent {
            send(scan, true)?;
            *release_sent = true;
        }
        Ok(())
    }

    pub fn acknowledge_release(&mut self, scan: u16) -> Result<(), String> {
        if self.down(scan)? {
            return Err(format!(
                "owned scan code is still physically Down: {scan:#x}"
            ));
        }
        self.held.retain(|(held, _)| *held != scan);
        Ok(())
    }

    pub fn release_owned(&mut self) -> Result<(), String> {
        let mut failures = Vec::new();
        let held = std::mem::take(&mut self.held);
        for (scan, mut release_sent) in held.into_iter().rev() {
            match self.down(scan) {
                Ok(false) => {}
                Ok(true) => {
                    if !release_sent {
                        match send(scan, true) {
                            Ok(()) => release_sent = true,
                            Err(error) => failures.push(error),
                        }
                    }
                    self.held.push((scan, release_sent));
                }
                Err(error) => {
                    failures.push(error);
                    self.held.push((scan, release_sent));
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }

    pub fn finish(&mut self) -> Result<(), String> {
        self.release_owned()?;
        let deadline = Instant::now() + Duration::from_secs(2);
        while self
            .held
            .iter()
            .any(|(scan, _)| self.down(*scan).unwrap_or(true))
        {
            if Instant::now() >= deadline {
                return Err("owned acceptance key did not return physically Up".into());
            }
            thread::sleep(Duration::from_millis(2));
        }
        self.held.clear();
        if let Some(original) = self.original {
            if Window::observe(original.handle) == Some(original) {
                original.activate()?;
                let deadline = Instant::now() + Duration::from_secs(3);
                while Window::observe(original.handle) == Some(original)
                    && unsafe { GetForegroundWindow() } != original.handle
                {
                    if Instant::now() >= deadline {
                        return Err("original foreground window was not restored".into());
                    }
                    thread::sleep(Duration::from_millis(2));
                }
            }
            self.original = None;
        }
        Ok(())
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("play_flow_check physical input cleanup: {error}");
        }
    }
}

fn key_down(scan: u16, thread: u32) -> Result<bool, String> {
    let layout = unsafe { GetKeyboardLayout(thread) };
    let key = unsafe { MapVirtualKeyExW(u32::from(scan), MAPVK_VSC_TO_VK_EX, layout) };
    if key == 0 {
        return Err(format!("native scan code has no virtual key: {scan:#x}"));
    }
    Ok(unsafe { GetAsyncKeyState(key as i32) } < 0)
}

fn send(scan: u16, up: bool) -> Result<(), String> {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: scan,
                dwFlags: KEYEVENTF_SCANCODE | if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    // SAFETY: One initialized keyboard INPUT is borrowed for this synchronous native call.
    if unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) } == 1 {
        Ok(())
    } else {
        Err(format!(
            "SendInput scan={scan:#x} up={up}: {}",
            std::io::Error::last_os_error()
        ))
    }
}
