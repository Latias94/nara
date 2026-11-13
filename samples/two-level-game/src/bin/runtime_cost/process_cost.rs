//! Read-only counters for this measurement process, without native lifecycle authority.

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::{
        ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        },
        Threading::{GetCurrentProcess, GetProcessTimes},
    },
};

#[derive(Clone, Copy)]
pub(super) struct ProcessCost {
    pub(super) user_ns: u64,
    pub(super) kernel_ns: u64,
    pub(super) working_set_bytes: u64,
    pub(super) private_bytes: u64,
}

#[cfg(windows)]
pub(super) fn capture() -> Result<ProcessCost, String> {
    let process = unsafe { GetCurrentProcess() };
    let empty = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user) = (empty, empty, empty, empty);
    if unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) } == 0 {
        return Err(format!(
            "observe original process CPU counters: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut memory = std::mem::MaybeUninit::<PROCESS_MEMORY_COUNTERS_EX>::zeroed();
    let memory_size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    unsafe {
        (*memory.as_mut_ptr()).cb = memory_size;
    }
    if unsafe {
        GetProcessMemoryInfo(
            process,
            memory.as_mut_ptr().cast::<PROCESS_MEMORY_COUNTERS>(),
            memory_size,
        )
    } == 0
    {
        return Err(format!(
            "observe original process memory counters: {}",
            std::io::Error::last_os_error()
        ));
    }
    let memory = unsafe { memory.assume_init() };
    let nanos = |time: FILETIME| {
        ((u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime))
            .checked_mul(100)
            .ok_or_else(|| "process CPU counter exceeded nanosecond representation".to_owned())
    };
    Ok(ProcessCost {
        user_ns: nanos(user)?,
        kernel_ns: nanos(kernel)?,
        working_set_bytes: memory.WorkingSetSize as u64,
        private_bytes: memory.PrivateUsage as u64,
    })
}

#[cfg(not(windows))]
pub(super) fn capture() -> Result<ProcessCost, String> {
    Err("process cost observations currently require Windows".into())
}
