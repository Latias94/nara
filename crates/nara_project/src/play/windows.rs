use std::{
    ffi::{OsStr, OsString, c_void},
    io::{self, PipeReader, PipeWriter, Read},
    mem::{offset_of, size_of, size_of_val},
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
        ERROR_BROKEN_PIPE, ERROR_INVALID_PARAMETER, ERROR_MORE_DATA, HANDLE, HANDLE_FLAG_INHERIT,
        SetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::{
        JobObjects::{
            CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_BASIC_PROCESS_ID_LIST,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
            JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Pipes::PeekNamedPipe,
        Threading::{
            CREATE_NO_WINDOW, CreateProcessW, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
            OpenProcess, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST,
            PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW, UpdateProcThreadAttribute,
            WaitForSingleObject,
        },
    },
};

use crate::windows_command_line::encode_command_line;

pub(super) struct Process {
    job: OwnedHandle,
    leader: OwnedHandle,
    terminating_members: Vec<OwnedHandle>,
}

impl Process {
    pub(super) fn leader_exited(&mut self) -> io::Result<bool> {
        exited(&self.leader)
    }

    pub(super) fn reap_if_idle(&mut self) -> io::Result<Option<ExitStatus>> {
        if active_processes(&self.job)? > 0 {
            return Ok(None);
        }
        // Job accounting can reach zero while a terminating descendant's process handle is
        // still unsignaled. Wait for the members we observed before requesting termination.
        for member in &self.terminating_members {
            if !exited(member)? {
                return Ok(None);
            }
        }
        let mut code = 0;
        // SAFETY: The leader was observed exited and `code` is writable for one DWORD.
        check(unsafe { GetExitCodeProcess(self.leader.as_raw_handle(), &mut code) })?;
        Ok(Some(ExitStatus::from_raw(code)))
    }

    pub(super) fn terminate(&mut self, _force: bool) -> io::Result<()> {
        let members = member_handles(&self.job);
        // SAFETY: The non-inheritable Job handle is valid for the termination call.
        let result = unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) };
        match check(result) {
            Err(_) if active_processes(&self.job)? == 0 => {}
            result => result?,
        };
        self.terminating_members.extend(members?);
        Ok(())
    }
}

fn exited(process: &OwnedHandle) -> io::Result<bool> {
    // SAFETY: The owned handle is valid for the nonblocking wait.
    match unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => Err(io::Error::last_os_error()),
    }
}

fn member_handles(job: &OwnedHandle) -> io::Result<Vec<OwnedHandle>> {
    let header_words =
        offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList) / size_of::<usize>();
    let mut slots = (active_processes(job)? as usize).max(1);
    let ids = loop {
        let mut storage = vec![0usize; header_words + slots];
        let size = u32::try_from(size_of_val(storage.as_slice()))
            .map_err(|_| io::ErrorKind::InvalidData)?;
        let list = storage
            .as_mut_ptr()
            .cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
        // SAFETY: The aligned buffer covers the header and `slots` process IDs.
        let result = unsafe {
            QueryInformationJobObject(
                job.as_raw_handle(),
                JobObjectBasicProcessIdList,
                list.cast(),
                size,
                null_mut(),
            )
        };
        match check(result) {
            Ok(()) => {
                // SAFETY: A successful query initialized the header and listed IDs.
                let count = unsafe { (*list).NumberOfProcessIdsInList } as usize;
                storage.truncate(header_words + count.min(slots));
                break storage;
            }
            Err(error) if error.raw_os_error() == Some(ERROR_MORE_DATA as i32) => {
                slots = slots.checked_mul(2).ok_or(io::ErrorKind::InvalidData)?;
            }
            Err(error) => return Err(error),
        }
    };
    let mut members = Vec::new();
    for id in ids.into_iter().skip(header_words) {
        // SAFETY: OpenProcess takes the numeric ID and returns a new handle on success.
        let handle = unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                id as u32,
            )
        };
        if handle.is_null() {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                continue;
            }
            return Err(error);
        }
        // SAFETY: Successful OpenProcess transferred the handle. Recheck membership in case
        // this process exited after enumeration and Windows reused its ID before OpenProcess.
        let process = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut member = 0;
        // SAFETY: Both handles are owned and `member` is writable for one BOOL.
        check(unsafe {
            IsProcessInJob(process.as_raw_handle(), job.as_raw_handle(), &mut member)
        })?;
        if member != 0 {
            members.push(process);
        }
    }
    Ok(members)
}

pub(super) fn spawn(
    directory: &Path,
    executable: &Path,
    arguments: &[OsString],
    stdin: PipeReader,
    stdout: PipeWriter,
    stderr: PipeWriter,
) -> io::Result<Process> {
    let application = wide(executable.as_os_str())?;
    let directory = wide(directory.as_os_str())?;
    let mut command_line = encode_command_line(executable, arguments)?;
    let job = create_job()?;
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
    Ok(Process {
        job,
        leader,
        terminating_members: Vec::new(),
    })
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

fn check(result: i32) -> io::Result<()> {
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
