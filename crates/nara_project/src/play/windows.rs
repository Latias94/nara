use std::{
    ffi::{OsStr, OsString, c_void},
    io::{self, PipeReader, PipeWriter, Read},
    mem::{size_of, size_of_val},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    path::Path,
    process::ExitStatus,
    ptr::{null, null_mut},
};

use windows_sys::Win32::{
    Foundation::{
        ERROR_BROKEN_PIPE, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    },
    System::{
        JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Pipes::PeekNamedPipe,
        Threading::{
            CREATE_NO_WINDOW, CreateProcessW, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW, UpdateProcThreadAttribute,
            WaitForSingleObject,
        },
    },
};

pub(super) struct Process {
    job: OwnedHandle,
    leader: OwnedHandle,
}

impl Process {
    pub(super) fn leader_exited(&mut self) -> io::Result<bool> {
        // SAFETY: The owned process handle is valid for the nonblocking wait.
        match unsafe { WaitForSingleObject(self.leader.as_raw_handle(), 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub(super) fn reap_if_idle(&mut self) -> io::Result<Option<ExitStatus>> {
        if active_processes(&self.job)? > 0 {
            return Ok(None);
        }
        let mut code = 0;
        // SAFETY: The leader was observed exited and `code` is writable for one DWORD.
        check(unsafe { GetExitCodeProcess(self.leader.as_raw_handle(), &mut code) })?;
        Ok(Some(ExitStatus::from_raw(code)))
    }

    pub(super) fn terminate(&mut self, _force: bool) -> io::Result<()> {
        // SAFETY: The non-inheritable Job handle is valid for the termination call.
        let result = unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) };
        match check(result) {
            Err(_) if active_processes(&self.job)? == 0 => Ok(()),
            result => result,
        }
    }
}

pub(super) fn spawn(
    directory: &Path,
    executable: &Path,
    arguments: &[OsString],
    stdout: PipeWriter,
    stderr: PipeWriter,
) -> io::Result<Process> {
    let application = wide(executable.as_os_str())?;
    let directory = wide(directory.as_os_str())?;
    let mut command_line = command_line(executable, arguments)?;
    let job = create_job()?;
    let (stdin, stdin_writer) = io::pipe()?;
    drop(stdin_writer);
    let inherited = [
        stdin.as_raw_handle(),
        stdout.as_raw_handle(),
        stderr.as_raw_handle(),
    ];
    for handle in inherited {
        // SAFETY: These three child ends remain exclusively owned during process creation.
        check(unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) })?;
    }
    let jobs = [job.as_raw_handle()];
    let attributes = Attributes::new(&jobs, &inherited)?;
    let startup = STARTUPINFOEXW {
        StartupInfo: STARTUPINFOW {
            cb: size_of::<STARTUPINFOEXW>() as u32,
            dwFlags: STARTF_USESTDHANDLES,
            hStdInput: inherited[0],
            hStdOutput: inherited[1],
            hStdError: inherited[2],
            ..Default::default()
        },
        lpAttributeList: attributes.pointer,
    };
    let mut information = PROCESS_INFORMATION::default();
    // SAFETY: All pointers refer to live storage of the required sizes. The attribute list assigns
    // the process to the Job during creation, before any of its instructions can execute.
    check(unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            1,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_NO_WINDOW,
            null(),
            directory.as_ptr(),
            &startup.StartupInfo,
            &mut information,
        )
    })?;
    // SAFETY: Successful CreateProcessW transfers ownership of both handles to this caller.
    let (leader, thread) = unsafe {
        (
            OwnedHandle::from_raw_handle(information.hProcess),
            OwnedHandle::from_raw_handle(information.hThread),
        )
    };
    drop(thread);
    Ok(Process { job, leader })
}

pub(super) fn read_pipe(reader: &mut PipeReader, buffer: &mut [u8]) -> io::Result<usize> {
    let mut available = 0;
    // SAFETY: Peek writes only the available byte count. This is the pipe's sole reader.
    let result = unsafe {
        PeekNamedPipe(
            reader.as_raw_handle(),
            null_mut(),
            0,
            null_mut(),
            &mut available,
            null_mut(),
        )
    };
    if let Err(error) = check(result) {
        return if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
            Ok(0)
        } else {
            Err(error)
        };
    }
    if available == 0 {
        return Err(io::ErrorKind::WouldBlock.into());
    }
    let count = (available as usize).min(buffer.len());
    reader.read(&mut buffer[..count])
}

fn create_job() -> io::Result<OwnedHandle> {
    // SAFETY: Null arguments create an unnamed, non-inheritable Job.
    let job = unsafe { CreateJobObjectW(null(), null()) };
    if job.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: The successful call transferred ownership of the Job handle.
    let job = unsafe { OwnedHandle::from_raw_handle(job) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: `limits` has the layout and size required by this information class.
    check(unsafe {
        SetInformationJobObject(
            job.as_raw_handle(),
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of_val(&limits) as u32,
        )
    })?;
    Ok(job)
}

fn active_processes(job: &OwnedHandle) -> io::Result<u32> {
    let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    // SAFETY: `accounting` is writable and has the size required by this information class.
    check(unsafe {
        QueryInformationJobObject(
            job.as_raw_handle(),
            JobObjectBasicAccountingInformation,
            (&raw mut accounting).cast(),
            size_of_val(&accounting) as u32,
            null_mut(),
        )
    })?;
    Ok(accounting.ActiveProcesses)
}

struct Attributes<'handles> {
    pointer: *mut c_void,
    _storage: Vec<usize>,
    _handles: (&'handles [HANDLE; 1], &'handles [HANDLE; 3]),
}

impl<'handles> Attributes<'handles> {
    fn new(jobs: &'handles [HANDLE; 1], inherited: &'handles [HANDLE; 3]) -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: A null list queries the allocation size into `bytes`.
        unsafe { InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut bytes) };
        if bytes == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut storage = vec![0_usize; bytes.div_ceil(size_of::<usize>())];
        let pointer = storage.as_mut_ptr().cast();
        // SAFETY: The storage is pointer-aligned and holds at least the requested byte count.
        check(unsafe { InitializeProcThreadAttributeList(pointer, 2, 0, &mut bytes) })?;
        let attributes = Self {
            pointer,
            _storage: storage,
            _handles: (jobs, inherited),
        };
        for (kind, handles) in [
            (PROC_THREAD_ATTRIBUTE_JOB_LIST, jobs.as_slice()),
            (PROC_THREAD_ATTRIBUTE_HANDLE_LIST, inherited.as_slice()),
        ] {
            // SAFETY: The initialized list only borrows handle values that outlive it.
            check(unsafe {
                UpdateProcThreadAttribute(
                    pointer,
                    0,
                    kind as usize,
                    handles.as_ptr().cast(),
                    size_of_val(handles),
                    null_mut(),
                    null_mut(),
                )
            })?;
        }
        Ok(attributes)
    }
}

impl Drop for Attributes<'_> {
    fn drop(&mut self) {
        // SAFETY: The list is initialized and its storage is still live.
        unsafe { DeleteProcThreadAttributeList(self.pointer) };
    }
}

fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut units: Vec<_> = value.encode_wide().collect();
    if units.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "process input contains a NUL",
        ));
    }
    units.push(0);
    Ok(units)
}

fn command_line(executable: &Path, arguments: &[OsString]) -> io::Result<Vec<u16>> {
    const QUOTE: u16 = b'"' as u16;
    const BACKSLASH: u16 = b'\\' as u16;

    let mut line = Vec::new();
    for argument in
        std::iter::once(executable.as_os_str()).chain(arguments.iter().map(OsString::as_os_str))
    {
        if !line.is_empty() {
            line.push(b' ' as u16);
        }
        line.push(QUOTE);
        let mut backslashes = 0;
        for unit in argument.encode_wide() {
            match unit {
                0 => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "process argument contains a NUL",
                    ));
                }
                BACKSLASH => backslashes += 1,
                _ => {
                    let count = if unit == QUOTE {
                        backslashes * 2 + 1
                    } else {
                        backslashes
                    };
                    line.extend(std::iter::repeat_n(BACKSLASH, count));
                    line.push(unit);
                    backslashes = 0;
                }
            }
        }
        line.extend(std::iter::repeat_n(BACKSLASH, backslashes * 2));
        line.push(QUOTE);
    }
    if line.len() >= 32_767 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows command line exceeds 32,767 UTF-16 code units",
        ));
    }
    line.push(0);
    Ok(line)
}

fn check(result: i32) -> io::Result<()> {
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
