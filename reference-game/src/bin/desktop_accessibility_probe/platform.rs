use std::{
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize, SAFEARRAY,
            },
            Ole::{
                SafeArrayDestroy, SafeArrayGetDim, SafeArrayGetElement, SafeArrayGetElemsize,
                SafeArrayGetLBound, SafeArrayGetUBound,
            },
            Variant::VARIANT,
        },
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationCondition, IUIAutomationElement,
                IUIAutomationInvokePattern, TreeScope_Descendants, UIA_ButtonControlTypeId,
                UIA_ControlTypePropertyId, UIA_InvokePatternId,
            },
            WindowsAndMessaging::{
                EnumWindows, GetClassNameW, GetWindowTextLengthW, GetWindowTextW,
                GetWindowThreadProcessId, IsWindow, IsWindowVisible,
            },
        },
    },
    core::BOOL,
};

use super::evidence::ProbeEvidence;

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const EXPECTED_WINIT_WINDOW_CLASS: &str = "Window Class";
const MAX_RUNTIME_ID_ELEMENTS: usize = 32;

pub(crate) fn spawn(
    evidence: Arc<ProbeEvidence>,
    timeout: Duration,
) -> JoinHandle<Result<(), String>> {
    std::thread::spawn(move || {
        let result = drive_platform_retry(&evidence, timeout);
        if let Err(error) = &result {
            evidence.fail(error.clone());
        }
        result
    })
}

fn drive_platform_retry(evidence: &ProbeEvidence, timeout: Duration) -> Result<(), String> {
    let _apartment = ComApartment::initialize()?;
    let automation: IUIAutomation = unsafe {
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("failed to create UI Automation: {error}"))?
    };
    let deadline = Instant::now()
        .checked_add(timeout)
        .unwrap_or_else(Instant::now);
    let process_id = std::process::id();
    let (hwnd, window_title) = wait_for_window(evidence, deadline)?;
    let window = PlatformWindow::new(automation, hwnd, process_id, window_title)?;
    let retry = wait_for_retry(&window, evidence, deadline)?;
    let retry_runtime_id = runtime_id(&retry)?;
    evidence.request_arm(window.hwnd_identity(), retry_runtime_id.clone())?;
    wait_for_runtime_arm(evidence, deadline)?;
    let retry = wait_for_armed_retry(&window, &retry_runtime_id, evidence, deadline)?;
    let invoke: IUIAutomationInvokePattern = unsafe {
        retry
            .GetCurrentPatternAs(UIA_InvokePatternId)
            .map_err(|error| format!("Retry has no UI Automation Invoke pattern: {error}"))?
    };
    evidence.begin_invoke(window.hwnd_identity(), &retry_runtime_id)?;
    unsafe {
        invoke
            .Invoke()
            .map_err(|error| format!("UI Automation Retry invocation failed: {error}"))?;
    }
    evidence.confirm_invoke()?;
    wait_for_successor_tree(&window, evidence, deadline)
}

fn wait_for_window(evidence: &ProbeEvidence, deadline: Instant) -> Result<(HWND, String), String> {
    loop {
        ensure_running(
            evidence,
            deadline,
            "terminal WGPU frame and native product window",
        )?;
        let Some(title) = evidence.platform_window_title() else {
            std::thread::sleep(POLL_INTERVAL);
            continue;
        };
        match process_window(std::process::id(), &title)? {
            Some(hwnd) => return Ok((hwnd, title)),
            None => std::thread::sleep(POLL_INTERVAL),
        }
    }
}

fn wait_for_retry(
    window: &PlatformWindow,
    evidence: &ProbeEvidence,
    deadline: Instant,
) -> Result<IUIAutomationElement, String> {
    let mut last_error = None;
    loop {
        ensure_running(evidence, deadline, "enabled Retry platform node")?;
        match window.tree() {
            Ok(PlatformTree::RetryReady(element)) => return Ok(element),
            Ok(PlatformTree::RetryPending | PlatformTree::Absent | PlatformTree::Indeterminate) => {
            }
            Err(error) => last_error = Some(error),
        }
        if Instant::now() >= deadline {
            return Err(last_error.map_or_else(
                || "timed out waiting for enabled Retry platform node".to_owned(),
                |error| format!("timed out waiting for Retry platform node: {error}"),
            ));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn wait_for_runtime_arm(evidence: &ProbeEvidence, deadline: Instant) -> Result<(), String> {
    loop {
        ensure_running(evidence, deadline, "runtime-owned Retry journal fence")?;
        if evidence.is_armed() {
            return Ok(());
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn wait_for_armed_retry(
    window: &PlatformWindow,
    expected_runtime_id: &[i32],
    evidence: &ProbeEvidence,
    deadline: Instant,
) -> Result<IUIAutomationElement, String> {
    loop {
        ensure_running(evidence, deadline, "the runtime-armed Retry platform node")?;
        match window.tree()? {
            PlatformTree::RetryReady(element) => {
                let actual_runtime_id = runtime_id(&element)?;
                if actual_runtime_id != expected_runtime_id {
                    return Err(format!(
                        "Retry UI Automation runtime ID changed after arming: expected={expected_runtime_id:?}, actual={actual_runtime_id:?}"
                    ));
                }
                return Ok(element);
            }
            PlatformTree::Indeterminate => std::thread::sleep(POLL_INTERVAL),
            PlatformTree::RetryPending | PlatformTree::Absent => {
                return Err(
                    "the exact Retry platform node disappeared after runtime arming".to_owned(),
                );
            }
        }
    }
}

fn wait_for_successor_tree(
    window: &PlatformWindow,
    evidence: &ProbeEvidence,
    deadline: Instant,
) -> Result<(), String> {
    let mut last_error = None;
    loop {
        ensure_running(evidence, deadline, "successor accessibility tree")?;
        if !evidence.successor_is_presented() {
            std::thread::sleep(POLL_INTERVAL);
            continue;
        }
        match window.tree() {
            Ok(PlatformTree::Absent) => {
                evidence.observe_successor_tree(window.hwnd_identity())?;
                return Ok(());
            }
            Ok(
                PlatformTree::RetryReady(_)
                | PlatformTree::RetryPending
                | PlatformTree::Indeterminate,
            ) => {}
            Err(error) => last_error = Some(error),
        }
        if Instant::now() >= deadline {
            return Err(last_error.map_or_else(
                || "timed out waiting for the successor accessibility tree".to_owned(),
                |error| format!("timed out waiting for successor accessibility tree: {error}"),
            ));
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn ensure_running(
    evidence: &ProbeEvidence,
    deadline: Instant,
    awaited: &'static str,
) -> Result<(), String> {
    if evidence.should_stop() {
        return Err(format!("probe stopped while waiting for {awaited}"));
    }
    if Instant::now() >= deadline {
        return Err(format!("timed out waiting for {awaited}"));
    }
    Ok(())
}

enum PlatformTree {
    RetryReady(IUIAutomationElement),
    RetryPending,
    Absent,
    Indeterminate,
}

struct PlatformWindow {
    automation: IUIAutomation,
    root: IUIAutomationElement,
    all_condition: IUIAutomationCondition,
    button_condition: IUIAutomationCondition,
    hwnd: HWND,
    process_id: u32,
    window_title: String,
}

impl PlatformWindow {
    fn new(
        automation: IUIAutomation,
        hwnd: HWND,
        process_id: u32,
        window_title: String,
    ) -> Result<Self, String> {
        validate_process_window(hwnd, process_id, &window_title)?;
        let root = unsafe { automation.ElementFromHandle(hwnd) }
            .map_err(|error| format!("failed to read the exact UI Automation root: {error}"))?;
        validate_root_handle(&root, hwnd)?;
        let all_condition = unsafe { automation.CreateTrueCondition() }.map_err(|error| {
            format!("failed to create the UI Automation tree condition: {error}")
        })?;
        let button_control_type = VARIANT::from(UIA_ButtonControlTypeId.0);
        let button_condition = unsafe {
            automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &button_control_type)
        }
        .map_err(|error| format!("failed to create the UI Automation button condition: {error}"))?;
        Ok(Self {
            automation,
            root,
            all_condition,
            button_condition,
            hwnd,
            process_id,
            window_title,
        })
    }

    fn hwnd_identity(&self) -> isize {
        hwnd_identity(self.hwnd)
    }

    fn current_root(&self) -> Result<IUIAutomationElement, String> {
        validate_process_window(self.hwnd, self.process_id, &self.window_title)?;
        let root = unsafe { self.automation.ElementFromHandle(self.hwnd) }
            .map_err(|error| format!("failed to read the exact UI Automation root: {error}"))?;
        validate_root_handle(&root, self.hwnd)?;
        let same_root =
            unsafe { self.automation.CompareElements(&self.root, &root) }.map_err(|error| {
                format!("failed to compare the product UI Automation root: {error}")
            })?;
        if !same_root.as_bool() {
            return Err(
                "the product UI Automation root identity changed during the probe".to_owned(),
            );
        }
        Ok(root)
    }

    fn tree(&self) -> Result<PlatformTree, String> {
        let root = self.current_root()?;
        let descendants = unsafe { root.FindAll(TreeScope_Descendants, &self.all_condition) }
            .map_err(|error| format!("failed to query the UI Automation tree: {error}"))?;
        let length = unsafe { descendants.Length() }
            .map_err(|error| format!("failed to read the UI Automation tree length: {error}"))?;
        if length <= 0 {
            return Ok(PlatformTree::Indeterminate);
        }
        let buttons = unsafe { root.FindAll(TreeScope_Descendants, &self.button_condition) }
            .map_err(|error| format!("failed to query UI Automation buttons: {error}"))?;
        let button_count = unsafe { buttons.Length() }
            .map_err(|error| format!("failed to read the UI Automation button count: {error}"))?;
        let mut retry = None;
        let mut pending = false;
        let mut indeterminate = false;
        for index in 0..button_count {
            let Ok(element) = (unsafe { buttons.GetElement(index) }) else {
                indeterminate = true;
                continue;
            };
            let Ok(name) = (unsafe { element.CurrentName() }) else {
                indeterminate = true;
                continue;
            };
            let name = name.to_string();
            if name != "Retry" && name != "Retrying..." {
                continue;
            }
            if name == "Retry" {
                match unsafe { element.CurrentIsEnabled() } {
                    Ok(enabled) if enabled.as_bool() => retry = Some(element),
                    Ok(_) => pending = true,
                    Err(_) => indeterminate = true,
                }
            } else {
                pending = true;
            }
        }
        Ok(if indeterminate {
            PlatformTree::Indeterminate
        } else if let Some(retry) = retry {
            PlatformTree::RetryReady(retry)
        } else if pending {
            PlatformTree::RetryPending
        } else {
            PlatformTree::Absent
        })
    }
}

fn validate_root_handle(root: &IUIAutomationElement, hwnd: HWND) -> Result<(), String> {
    let native_handle = unsafe { root.CurrentNativeWindowHandle() }
        .map_err(|error| format!("failed to validate the UI Automation root HWND: {error}"))?;
    if native_handle != hwnd {
        return Err("UI Automation root no longer owns the exact product HWND".to_owned());
    }
    Ok(())
}

fn runtime_id(element: &IUIAutomationElement) -> Result<Vec<i32>, String> {
    let array = unsafe { element.GetRuntimeId() }
        .map_err(|error| format!("failed to read UI Automation runtime ID: {error}"))?;
    let array = OwnedSafeArray::new(array)?;
    if unsafe { SafeArrayGetDim(array.as_ptr()) } != 1
        || unsafe { SafeArrayGetElemsize(array.as_ptr()) } as usize != size_of::<i32>()
    {
        return Err("UI Automation runtime ID has an unsupported shape".to_owned());
    }
    let lower = unsafe { SafeArrayGetLBound(array.as_ptr(), 1) }
        .map_err(|error| format!("failed to read runtime-ID lower bound: {error}"))?;
    let upper = unsafe { SafeArrayGetUBound(array.as_ptr(), 1) }
        .map_err(|error| format!("failed to read runtime-ID upper bound: {error}"))?;
    let length = upper
        .checked_sub(lower)
        .and_then(|span| span.checked_add(1))
        .and_then(|length| usize::try_from(length).ok())
        .filter(|length| (1..=MAX_RUNTIME_ID_ELEMENTS).contains(length))
        .ok_or_else(|| "UI Automation runtime ID is empty or unbounded".to_owned())?;
    let mut runtime_id = Vec::with_capacity(length);
    for index in lower..=upper {
        let mut value = 0_i32;
        unsafe {
            SafeArrayGetElement(
                array.as_ptr(),
                &raw const index,
                (&raw mut value).cast::<core::ffi::c_void>(),
            )
        }
        .map_err(|error| format!("failed to read runtime-ID element: {error}"))?;
        runtime_id.push(value);
    }
    Ok(runtime_id)
}

struct OwnedSafeArray(*mut SAFEARRAY);

impl OwnedSafeArray {
    fn new(array: *mut SAFEARRAY) -> Result<Self, String> {
        if array.is_null() {
            Err("UI Automation runtime ID was null".to_owned())
        } else {
            Ok(Self(array))
        }
    }

    fn as_ptr(&self) -> *const SAFEARRAY {
        self.0.cast_const()
    }
}

impl Drop for OwnedSafeArray {
    fn drop(&mut self) {
        let _ = unsafe { SafeArrayDestroy(self.0.cast_const()) };
    }
}

struct ProcessWindowSearch<'a> {
    process_id: u32,
    expected_title: &'a str,
    matches: Vec<HWND>,
}

fn validate_process_window(
    hwnd: HWND,
    expected_process_id: u32,
    expected_title: &str,
) -> Result<(), String> {
    if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        return Err("the exact product HWND is no longer alive".to_owned());
    }
    let mut actual_process_id = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut actual_process_id));
    }
    if actual_process_id != expected_process_id
        || !unsafe { IsWindowVisible(hwnd).as_bool() }
        || window_text(hwnd).as_deref() != Some(expected_title)
        || window_class(hwnd).as_deref() != Some(EXPECTED_WINIT_WINDOW_CLASS)
    {
        return Err(format!(
            "the exact product window lifetime changed: hwnd={}, expected_pid={expected_process_id}, actual_pid={actual_process_id}",
            hwnd_identity(hwnd)
        ));
    }
    Ok(())
}

fn process_window(process_id: u32, expected_title: &str) -> Result<Option<HWND>, String> {
    let mut search = ProcessWindowSearch {
        process_id,
        expected_title,
        matches: Vec::with_capacity(2),
    };
    // EnumWindows invokes the callback synchronously, so the stack pointer remains valid.
    unsafe {
        EnumWindows(
            Some(find_process_window),
            LPARAM((&raw mut search).cast::<core::ffi::c_void>() as isize),
        )
    }
    .map_err(|error| format!("failed to enumerate product windows: {error}"))?;
    match search.matches.as_slice() {
        [] => Ok(None),
        [hwnd] => Ok(Some(*hwnd)),
        matches => Err(format!(
            "found {} visible product windows with title {:?} and class {:?}",
            matches.len(),
            expected_title,
            EXPECTED_WINIT_WINDOW_CLASS
        )),
    }
}

unsafe extern "system" fn find_process_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: process_window passes a live ProcessWindowSearch for this synchronous enumeration.
    let search = unsafe { &mut *(lparam.0 as *mut ProcessWindowSearch<'_>) };
    let mut process_id = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
    }
    if process_id == search.process_id
        && unsafe { IsWindowVisible(hwnd).as_bool() }
        && window_text(hwnd).as_deref() == Some(search.expected_title)
        && window_class(hwnd).as_deref() == Some(EXPECTED_WINIT_WINDOW_CLASS)
    {
        search.matches.push(hwnd);
    }
    BOOL(1)
}

fn window_text(hwnd: HWND) -> Option<String> {
    let length = unsafe { GetWindowTextLengthW(hwnd) };
    let buffer_length = usize::try_from(length).ok()?.checked_add(1)?;
    let mut buffer = vec![0_u16; buffer_length];
    let written = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    let written = usize::try_from(written).ok()?;
    String::from_utf16(&buffer[..written]).ok()
}

fn window_class(hwnd: HWND) -> Option<String> {
    let mut buffer = [0_u16; 256];
    let written = unsafe { GetClassNameW(hwnd, &mut buffer) };
    let written = usize::try_from(written).ok()?;
    (written != 0)
        .then(|| String::from_utf16(&buffer[..written]).ok())
        .flatten()
}

fn hwnd_identity(hwnd: HWND) -> isize {
    hwnd.0 as isize
}

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, String> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|error| format!("failed to initialize COM: {error}"))?;
        }
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
