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
            SendInput, VK_MENU,
        },
        WindowsAndMessaging::{
            EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
            IsWindow, IsWindowVisible, PostMessageW, SetForegroundWindow, WM_CLOSE,
        },
    },
};

use super::process::POLL;

const KEYS: [u16; 12] = [
    0x1e, 0x20, 0x11, 0x1f, 0x12, 0x10, 0x32, 0x25, 0x26, 0x13, B, G,
];
pub const D: u16 = 0x20;
pub const J: u16 = 0x24;
pub const W: u16 = 0x11;
pub const E: u16 = 0x12;
pub const Q: u16 = 0x10;
pub const M: u16 = 0x32;
pub const K: u16 = 0x25;
pub const L: u16 = 0x26;
pub const R: u16 = 0x13;
pub const B: u16 = 0x30;
pub const G: u16 = 0x22;

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

    pub fn exists(self) -> bool {
        unsafe { IsWindow(self.handle) != 0 }
    }

    pub fn find(process: u32, title: &str) -> Result<Option<Self>, String> {
        struct Search<'a> {
            process: u32,
            title: &'a str,
            matches: Vec<Window>,
            visits: usize,
        }
        unsafe extern "system" fn visit(handle: HWND, data: LPARAM) -> i32 {
            // SAFETY: EnumWindows synchronously supplies the live, exclusively borrowed Search.
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
        let mut search = Search {
            process,
            title,
            matches: Vec::new(),
            visits: 0,
        };
        // SAFETY: The callback and its Search remain valid for this synchronous enumeration.
        let completed =
            unsafe { EnumWindows(Some(visit), (&mut search as *mut Search<'_>) as LPARAM) };
        if completed == 0 || search.matches.len() > 1 {
            return Err(format!(
                "unique exact owned window enumeration failed: pid={process} matches={} visits={}",
                search.matches.len(),
                search.visits
            ));
        }
        Ok(search.matches.pop())
    }

    fn verify(self, title: &str) -> Result<(), String> {
        if Self::observe(self.handle) != Some(self)
            || Self::find(self.process, title)? != Some(self)
        {
            return Err(format!(
                "original process/thread/HWND/title no longer matches: {self:?}"
            ));
        }
        Ok(())
    }

    fn focus(self, title: &str) -> Result<(), String> {
        self.verify(title)?;
        activate(self)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.verify(title)?;
            if Self::foreground() == Some(self) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "SetForegroundWindow did not select {self:?}; foreground={:?} visible={} enabled={} iconic={}",
                    Self::foreground(),
                    unsafe { IsWindowVisible(self.handle) },
                    unsafe { IsWindowEnabled(self.handle) },
                    unsafe { IsIconic(self.handle) }
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
    held: Vec<u16>,
}

impl Input {
    pub fn new(title: &str) -> Self {
        let input = Self {
            original: Window::foreground(),
            target: None,
            title: title.to_owned(),
            held: Vec::new(),
        };
        input.assert_all_up(0);
        input
    }

    fn virtual_key(scan: u16, thread: u32) -> i32 {
        let layout = unsafe { GetKeyboardLayout(thread) };
        let key = unsafe { MapVirtualKeyExW(u32::from(scan), MAPVK_VSC_TO_VK_EX, layout) };
        assert_ne!(key, 0, "game scan code must map to a native virtual key");
        key as i32
    }

    fn assert_all_up(&self, thread: u32) {
        for scan in KEYS {
            assert!(
                unsafe { GetAsyncKeyState(Self::virtual_key(scan, thread)) } >= 0,
                "all A/D/W/S/E/Q/M/K/L/R/B/G keys must initially be Up: scan={scan:x}"
            );
        }
    }

    pub fn bind(&mut self, window: Window) {
        assert!(self.held.is_empty());
        self.assert_all_up(window.thread);
        self.target = Some(window);
        window.focus(&self.title).unwrap();
        self.check();
    }

    pub fn check(&self) {
        let target = self.target.expect("matched original native window");
        target.verify(&self.title).unwrap();
        assert_eq!(
            Window::foreground(),
            Some(target),
            "physical input lost the exact original foreground window"
        );
    }

    pub fn press(&mut self, scan: u16) {
        self.check();
        assert!((KEYS.contains(&scan) || scan == J) && !self.held.contains(&scan));
        let key = Self::virtual_key(scan, self.target.unwrap().thread);
        assert!(
            unsafe { GetAsyncKeyState(key) } >= 0,
            "matched key must start Up"
        );
        self.held.push(scan);
        eprintln!(
            "cooked_desktop physical Down scan={scan:#x} vk={key:#x} target={:?}",
            self.target
        );
        assert!(send(scan, false), "SendInput scan Down failed");
        let deadline = Instant::now() + Duration::from_secs(2);
        while unsafe { GetAsyncKeyState(key) } >= 0 {
            self.check();
            assert!(
                Instant::now() < deadline,
                "injected game key never became Down: scan={scan:#x} vk={key:#x} async={:#x}",
                unsafe { GetAsyncKeyState(key) }
            );
            thread::sleep(POLL);
        }
    }

    pub fn release(&mut self) {
        self.check();
        assert!(self.release_owned(), "owned scan Up failed");
        self.check();
    }

    fn release_owned(&mut self) -> bool {
        if self.held.is_empty() {
            return true;
        }
        if let Some(target) = self.target
            && Window::observe(target.handle) == Some(target)
            && target.focus(&self.title).is_err()
        {
            eprintln!("owned key release could not reacquire the original foreground");
        }
        let thread = self.target.map_or(0, |target| target.thread);
        let mut released = true;
        for index in (0..self.held.len()).rev() {
            let scan = self.held[index];
            if !send(scan, true) {
                released = false;
                continue;
            }
            let key = Self::virtual_key(scan, thread);
            let deadline = Instant::now() + Duration::from_secs(2);
            while unsafe { GetAsyncKeyState(key) } < 0 && Instant::now() < deadline {
                thread::sleep(POLL);
            }
            if unsafe { GetAsyncKeyState(key) } < 0 {
                released = false;
            } else {
                self.held.remove(index);
            }
        }
        released
    }

    pub fn restore(&mut self) -> bool {
        let Some(original) = self.original else {
            return true;
        };
        if Window::observe(original.handle) != Some(original) {
            self.original = None;
            return true;
        }
        if let Err(error) = activate(original) {
            eprintln!("restoring the original foreground failed: {error}");
            return false;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if Window::observe(original.handle) != Some(original)
                || Window::foreground() == Some(original)
            {
                self.original = None;
                return true;
            }
            thread::sleep(POLL);
        }
        false
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        if !self.release_owned() {
            eprintln!("owned native key remained Down during physical acceptance cleanup");
        }
        if !self.restore() {
            eprintln!("physical acceptance could not restore the original foreground window");
        }
    }
}

/// Uses Winit's matched Alt transition to request foreground on an observed native window.
fn activate(window: Window) -> Result<(), String> {
    if Window::observe(window.handle) != Some(window) {
        return Err("the observed activation window changed its native identity".into());
    }
    if Window::foreground() == Some(window) {
        return Ok(());
    }
    if unsafe { SetForegroundWindow(window.handle) } != 0 {
        return Ok(());
    }
    if unsafe { GetAsyncKeyState(i32::from(VK_MENU)) } < 0 {
        return Err("Alt must be physically released before native activation".into());
    }
    struct AltRelease;
    impl Drop for AltRelease {
        fn drop(&mut self) {
            if !send(0x38, true) {
                eprintln!("injected Alt release failed during activation cleanup");
            }
        }
    }
    let release = AltRelease;
    if !send(0x38, false) {
        return Err("native activation Alt Down was not accepted".into());
    }
    drop(release);
    if unsafe { GetAsyncKeyState(i32::from(VK_MENU)) } < 0 {
        return Err("injected Alt did not return to Up before native activation".into());
    }
    if Window::observe(window.handle) != Some(window) {
        return Err("the activation target changed during the matched Alt transition".into());
    }
    unsafe { SetForegroundWindow(window.handle) };
    Ok(())
}

fn send(scan: u16, up: bool) -> bool {
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
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) == 1 }
}
