//! Native acquisition of one suspended exact direct child, owned by the finite run's custody.

use std::{
    cmp::Ordering,
    ffi::{OsStr, OsString, c_void},
    fs::File,
    io,
    mem::{size_of, size_of_val},
    os::windows::{
        ffi::OsStrExt,
        io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    path::Path,
    process::ExitStatus,
    ptr::{null, null_mut},
};

use windows_sys::Win32::{
    Foundation::{
        HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Globalization::{CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN, CompareStringOrdinal},
    Security::SECURITY_ATTRIBUTES,
    System::{
        Pipes::CreatePipe,
        Threading::{
            CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
            INFINITE, InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW,
            TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};

use crate::project_host::process_image::{ExactProcessImage, encode_command_line};

const WINDOWS_WIDE_LIMIT: usize = 32_767;

/// There is deliberately no independent cleanup owner or descendant containment here.
/// The caller installs this child in `Custody` before verification or primary-thread resume.
pub(super) struct ExactChild {
    process: OwnedHandle,
    primary_thread: Option<OwnedHandle>,
    pub(super) stdin: Option<File>,
    pub(super) stdout: Option<File>,
    pub(super) stderr: Option<File>,
}

impl ExactChild {
    pub(super) fn verify_and_resume(&mut self, image: &ExactProcessImage) -> io::Result<()> {
        image.verify_mapping(self.process.as_handle())?;
        let thread = self.primary_thread.as_ref().ok_or_else(|| {
            io::Error::other("the exact finite child has no suspended primary thread")
        })?;
        // SAFETY: this is the live primary thread returned by CREATE_SUSPENDED.
        match unsafe { ResumeThread(raw_handle(thread)) } {
            1 => {
                self.primary_thread.take();
                Ok(())
            }
            u32::MAX => Err(io::Error::last_os_error()),
            count => Err(io::Error::other(format!(
                "ResumeThread(exact finite child) observed unexpected suspend count {count}"
            ))),
        }
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.wait_for(0)
    }

    pub(super) fn wait(&mut self) -> io::Result<ExitStatus> {
        self.wait_for(INFINITE)?.ok_or_else(|| {
            io::Error::other("the infinite exact finite child wait unexpectedly timed out")
        })
    }

    fn wait_for(&self, timeout: u32) -> io::Result<Option<ExitStatus>> {
        // SAFETY: the owned process handle remains live throughout the wait and exit query.
        match unsafe { WaitForSingleObject(raw_handle(&self.process), timeout) } {
            WAIT_OBJECT_0 => {
                let mut code = 0;
                // SAFETY: the handle is live and `code` points to writable u32 storage.
                if unsafe { GetExitCodeProcess(raw_handle(&self.process), &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                // Preserve the complete Windows exit value, including a genuine exit code 259.
                Ok(Some(ExitStatus::from_raw(code)))
            }
            WAIT_TIMEOUT => Ok(None),
            WAIT_FAILED => Err(io::Error::last_os_error()),
            status => Err(io::Error::other(format!(
                "WaitForSingleObject(exact finite child) returned unexpected status {status:#x}"
            ))),
        }
    }

    pub(super) fn kill(&mut self) -> io::Result<()> {
        // SAFETY: the handle is the owned direct child; no Job or descendant operation is used.
        if unsafe { TerminateProcess(raw_handle(&self.process), 1) } != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if matches!(self.try_wait(), Ok(Some(_))) {
            Ok(())
        } else {
            Err(error)
        }
    }
}

/// All fallible setup precedes CreateProcessW. Success always transfers the actual child,
/// still suspended, to the caller; mapping verification and resume happen inside its custody.
pub(super) fn spawn(
    image: &ExactProcessImage,
    arguments: &[OsString],
    working_directory: &Path,
    environment: &[(OsString, OsString)],
) -> io::Result<ExactChild> {
    let application = terminated_wide(image.path().as_os_str())?;
    let directory = terminated_wide(working_directory.as_os_str())?;
    let mut command_line = encode_command_line(image.path(), arguments)?;
    let environment = environment_block(environment)?;
    let (stdin, child_stdin) = anonymous_pipe(true)?;
    let (stdout, child_stdout) = anonymous_pipe(false)?;
    let (stderr, child_stderr) = anonymous_pipe(false)?;
    // Convert parent endpoints before acquisition so no fallible pipe setup follows creation.
    let stdin = File::from(stdin);
    let stdout = File::from(stdout);
    let stderr = File::from(stderr);
    let inherited = [
        raw_handle(&child_stdin),
        raw_handle(&child_stdout),
        raw_handle(&child_stderr),
    ];
    let attributes = HandleList::new(&inherited)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = inherited[0];
    startup.StartupInfo.hStdOutput = inherited[1];
    startup.StartupInfo.hStdError = inherited[2];
    startup.lpAttributeList = attributes.pointer;
    let mut information = PROCESS_INFORMATION::default();
    // SAFETY: every pointer and the inherited-handle list remain live through creation. Parent
    // pipe handles are non-inheritable. The explicit double-NUL environment never inherits the
    // ambient environment. The primary thread cannot execute before the caller verifies its image.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            environment.as_ptr().cast::<c_void>(),
            directory.as_ptr(),
            (&startup as *const STARTUPINFOEXW).cast(),
            &mut information,
        )
    };
    if created == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful CreateProcessW transfers two valid uniquely owned handles. Nothing
    // fallible or allocating occurs between acquisition and transferring the child to Custody.
    let process = unsafe { OwnedHandle::from_raw_handle(information.hProcess.cast()) };
    // SAFETY: hThread is the other uniquely owned handle from that same successful creation.
    let primary_thread = unsafe { OwnedHandle::from_raw_handle(information.hThread.cast()) };
    Ok(ExactChild {
        process,
        primary_thread: Some(primary_thread),
        stdin: Some(stdin),
        stdout: Some(stdout),
        stderr: Some(stderr),
    })
}

fn anonymous_pipe(parent_writes: bool) -> io::Result<(OwnedHandle, OwnedHandle)> {
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let mut read = null_mut();
    let mut write = null_mut();
    // SAFETY: output handles and the security descriptor are valid for the complete call.
    if unsafe { CreatePipe(&mut read, &mut write, &security, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful CreatePipe transfers two valid uniquely owned handles.
    let read = unsafe { OwnedHandle::from_raw_handle(read.cast()) };
    // SAFETY: this is the second handle transferred by the successful CreatePipe.
    let write = unsafe { OwnedHandle::from_raw_handle(write.cast()) };
    let (parent, child) = if parent_writes {
        (write, read)
    } else {
        (read, write)
    };
    // SAFETY: only the inheritance bit of the live parent endpoint is changed.
    if unsafe { SetHandleInformation(raw_handle(&parent), HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((parent, child))
}

struct HandleList {
    _storage: Box<[usize]>,
    pointer: *mut c_void,
}

impl HandleList {
    // The supplied handle slice must remain live until CreateProcessW returns.
    fn new(handles: &[HANDLE; 3]) -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: the documented size query writes only the required byte count.
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut bytes) };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut storage = vec![0_usize; bytes.div_ceil(size_of::<usize>())].into_boxed_slice();
        let pointer = storage.as_mut_ptr().cast();
        // SAFETY: the pointer-aligned allocation holds at least the requested byte count.
        if unsafe { InitializeProcThreadAttributeList(pointer, 1, 0, &mut bytes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let list = Self {
            _storage: storage,
            pointer,
        };
        // SAFETY: the list is initialized and handles contains exactly the three live, distinct
        // inheritable child endpoints. The caller retains the slice and endpoints through spawn.
        if unsafe {
            UpdateProcThreadAttribute(
                pointer,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                size_of_val(handles),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(list)
    }
}

impl Drop for HandleList {
    fn drop(&mut self) {
        // SAFETY: initialization succeeded and the backing storage is still live.
        unsafe { DeleteProcThreadAttributeList(self.pointer) };
    }
}

fn terminated_wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut units = checked_wide(value)?;
    units.push(0);
    Ok(units)
}

fn checked_wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let units = value.encode_wide().collect::<Vec<_>>();
    if units.contains(&0) || units.len() >= WINDOWS_WIDE_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a Windows launch value contains NUL or exceeds its UTF-16 bound",
        ));
    }
    Ok(units)
}

fn environment_block(environment: &[(OsString, OsString)]) -> io::Result<Vec<u16>> {
    let mut entries = Vec::with_capacity(environment.len());
    for (name, value) in environment {
        let name = checked_wide(name)?;
        if name.is_empty() || name.contains(&(b'=' as u16)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a Windows environment name must be non-empty and contain no '='",
            ));
        }
        entries.push((name, checked_wide(value)?));
    }
    entries.sort_by(|(left, _), (right, _)| compare_names(left, right));
    if entries
        .windows(2)
        .any(|pair| compare_names(&pair[0].0, &pair[1].0) == Ordering::Equal)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the explicit Windows environment contains duplicate names",
        ));
    }
    let mut block = Vec::new();
    for (name, value) in entries {
        block.extend(name);
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
        if block.len() >= WINDOWS_WIDE_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the explicit Windows environment exceeds 32,767 UTF-16 code units",
            ));
        }
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

fn compare_names(left: &[u16], right: &[u16]) -> Ordering {
    // SAFETY: checked_wide bounds both live slices to fewer than 32,767 units.
    match unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            left.len() as i32,
            right.as_ptr(),
            right.len() as i32,
            1,
        )
    } {
        CSTR_LESS_THAN => Ordering::Less,
        CSTR_EQUAL => Ordering::Equal,
        CSTR_GREATER_THAN => Ordering::Greater,
        _ => left.cmp(right),
    }
}

fn raw_handle(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle().cast()
}
