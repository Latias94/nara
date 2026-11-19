//! Physical input and ordinary activation for one external Player PID, thread, HWND and title.

use std::{
    thread,
    time::{Duration, Instant},
};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    UI::{
        Input::KeyboardAndMouse::{
            GetAsyncKeyState, GetKeyboardLayout, INPUT, INPUT_0, INPUT_KEYBOARD, IsWindowEnabled,
            KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW,
            SendInput, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        },
        WindowsAndMessaging::{
            EnumWindows, GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo, GetWindowTextW,
            GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, PostMessageW,
            SetForegroundWindow, WM_CLOSE,
        },
    },
};

use super::process::POLL;

pub const ENTER: u16 = 0x1c;
pub const ESCAPE: u16 = 0x01;
pub const D: u16 = 0x20;
pub const E: u16 = 0x12;
pub const K: u16 = 0x25;
pub const T: u16 = 0x14;
pub const X: u16 = 0x2d;
const KEYS: [u16; 16] = [
    ENTER, ESCAPE, D, E, K, T, X, 0x1e, 0x11, 0x1f, 0x10, 0x32, 0x26, 0x13, 0x30, 0x22,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
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

    fn foreground() -> Option<Self> {
        Self::observe(unsafe { GetForegroundWindow() })
    }

    pub fn find(process: u32, title: &str) -> Result<Option<Self>, String> {
        struct Search<'a> {
            process: u32,
            title: &'a str,
            visits: usize,
            matches: Vec<Window>,
        }
        unsafe extern "system" fn visit(handle: HWND, data: LPARAM) -> i32 {
            // SAFETY: EnumWindows synchronously borrows this live, exclusive Search.
            let search = unsafe { &mut *(data as *mut Search<'_>) };
            search.visits += 1;
            if search.visits > 16_384 {
                return 0;
            }
            if let Some(window) = Window::observe(handle)
                && window.process == search.process
            {
                let mut text = [0_u16; 1024];
                let length =
                    unsafe { GetWindowTextW(handle, text.as_mut_ptr(), text.len() as i32) };
                if length > 0 && String::from_utf16_lossy(&text[..length as usize]) == search.title
                {
                    search.matches.push(window);
                    if search.matches.len() > 1 {
                        return 0;
                    }
                }
            }
            1
        }
        if process == 0 || process == std::process::id() || title.is_empty() || title.len() >= 1024
        {
            return Err("expected an external Player PID and an exact bounded title".into());
        }
        let mut search = Search {
            process,
            title,
            visits: 0,
            matches: Vec::new(),
        };
        // SAFETY: Search and its borrowed title outlive the synchronous enumeration.
        let completed =
            unsafe { EnumWindows(Some(visit), (&mut search as *mut Search<'_>) as LPARAM) };
        if completed == 0 || search.matches.len() > 1 {
            return Err(format!(
                "exact external window enumeration failed: pid={process} matches={} visits={}",
                search.matches.len(),
                search.visits,
            ));
        }
        Ok(search.matches.pop())
    }

    pub fn exists(self) -> bool {
        unsafe { IsWindow(self.handle) != 0 }
    }

    fn verify(self, title: &str) -> Result<(), String> {
        if Self::observe(self.handle) != Some(self)
            || Self::find(self.process, title)? != Some(self)
        {
            return Err(format!(
                "original external PID/thread/HWND/title changed: {self:?}"
            ));
        }
        Ok(())
    }

    fn interactive(self) -> bool {
        unsafe {
            IsWindowVisible(self.handle) != 0
                && IsWindowEnabled(self.handle) != 0
                && IsIconic(self.handle) == 0
        }
    }

    fn keyboard_focus(self) -> bool {
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        let observed = unsafe { GetGUIThreadInfo(self.thread, &mut info) };
        observed != 0 && info.hwndFocus == self.handle && Self::foreground() == Some(self)
    }

    fn activate(self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            if Self::observe(self.handle) != Some(self) {
                return Err("ordinary activation target lost its original identity".into());
            }
            if Self::foreground() == Some(self) {
                return Ok(());
            }
            unsafe { SetForegroundWindow(self.handle) };
            if Instant::now() >= deadline {
                return Err(format!(
                    "ordinary activation timed out: target={self:?} foreground={:?}",
                    Self::foreground(),
                ));
            }
            thread::sleep(POLL);
        }
    }

    pub fn close(self, title: &str) -> Result<(), String> {
        self.verify(title)?;
        if unsafe { PostMessageW(self.handle, WM_CLOSE, 0, 0) } == 0 {
            return Err(format!(
                "owned WM_CLOSE failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}

pub struct Input {
    original: Option<Window>,
    target: Option<Window>,
    title: String,
    held: Vec<(u16, bool)>,
}

impl Input {
    pub fn new(title: &str) -> Self {
        Self {
            original: Window::foreground(),
            target: None,
            title: title.to_owned(),
            held: Vec::new(),
        }
    }

    pub fn bind(&mut self, window: Window) {
        assert!(self.target.is_none() && self.held.is_empty());
        window.verify(&self.title).unwrap();
        for scan in KEYS {
            assert!(
                !key_down(scan, window.thread).unwrap(),
                "acceptance key starts Down: {scan:#x}"
            );
        }
        for key in [VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN] {
            assert!(
                unsafe { GetAsyncKeyState(i32::from(key)) } >= 0,
                "physical modifier keys must be Up before native acceptance",
            );
        }
        self.target = Some(window);
    }

    #[track_caller]
    pub fn check(&self) {
        let target = self
            .target
            .expect("the original external Player window is bound");
        target.verify(&self.title).unwrap();
        assert_eq!(
            Window::foreground(),
            Some(target),
            "exact original foreground HWND changed"
        );
        assert!(
            target.interactive(),
            "original Player window is not interactive: {target:?}"
        );
        assert!(
            target.keyboard_focus(),
            "original Player thread lost keyboard focus: {target:?}"
        );
    }

    pub fn try_focus(&self) -> bool {
        let target = self
            .target
            .expect("the original external Player window is bound");
        assert_eq!(
            Window::observe(target.handle),
            Some(target),
            "activation target lost its original identity"
        );
        if !target.interactive() {
            return false;
        }
        if Window::foreground() != Some(target) {
            unsafe { SetForegroundWindow(target.handle) };
        }
        target.keyboard_focus()
    }

    pub fn press(&mut self, scan: u16) {
        self.check();
        assert!(KEYS.contains(&scan) && !self.held.iter().any(|(held, _)| *held == scan));
        let thread = self.target.unwrap().thread;
        assert!(
            !key_down(scan, thread).unwrap(),
            "owned key must initially be Up: {scan:#x}"
        );
        send(scan, false).unwrap();
        self.held.push((scan, false));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !key_down(scan, thread).unwrap() {
            self.check();
            assert!(
                Instant::now() < deadline,
                "owned scan Down was never observed: {scan:#x}"
            );
            thread::sleep(POLL);
        }
    }

    pub fn release(&mut self, scan: u16) {
        self.check();
        let release_sent = self
            .held
            .iter_mut()
            .find_map(|(held, sent)| (*held == scan).then_some(sent))
            .expect("only an owned key can be released");
        assert!(
            !*release_sent,
            "each owned key has exactly one Up transition"
        );
        send(scan, true).unwrap();
        *release_sent = true;
        let thread = self.target.unwrap().thread;
        let deadline = Instant::now() + Duration::from_secs(2);
        while key_down(scan, thread).unwrap() {
            assert!(
                Instant::now() < deadline,
                "owned scan Up was never observed: {scan:#x}"
            );
            thread::sleep(POLL);
        }
        self.held.retain(|(held, _)| *held != scan);
        self.check();
    }

    pub fn down(&self, scan: u16) -> bool {
        self.check();
        key_down(scan, self.target.unwrap().thread).unwrap()
    }

    pub fn finish(&mut self) -> Result<(), String> {
        let mut failures = Vec::new();
        let thread = self.target.map_or(0, |window| window.thread);
        for (scan, sent) in self.held.iter_mut().rev() {
            if !*sent {
                match send(*scan, true) {
                    Ok(()) => *sent = true,
                    Err(error) => failures.push(error),
                }
            }
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while self
            .held
            .iter()
            .any(|(scan, _)| key_down(*scan, thread).unwrap_or(true))
            && Instant::now() < deadline
        {
            thread::sleep(POLL);
        }
        self.held
            .retain(|(scan, _)| key_down(*scan, thread).unwrap_or(true));
        if !self.held.is_empty() {
            failures.push("an owned native key did not return physically Up".into());
        }
        if let Some(original) = self.original.take()
            && Window::observe(original.handle) == Some(original)
            && let Err(error) = original.activate(Duration::from_secs(3))
        {
            failures.push(error);
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("cooked round physical-input cleanup: {error}");
        }
    }
}

fn key_down(scan: u16, thread: u32) -> Result<bool, String> {
    let layout = unsafe { GetKeyboardLayout(thread) };
    let key = unsafe { MapVirtualKeyExW(u32::from(scan), MAPVK_VSC_TO_VK_EX, layout) };
    if key == 0 {
        return Err(format!("scan code has no native virtual key: {scan:#x}"));
    }
    Ok(unsafe { GetAsyncKeyState(key as i32) < 0 })
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
    // SAFETY: One initialized keyboard INPUT remains live during this synchronous call.
    if unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) } == 1 {
        Ok(())
    } else {
        Err(format!(
            "SendInput scan={scan:#x} up={up}: {}",
            std::io::Error::last_os_error()
        ))
    }
}
