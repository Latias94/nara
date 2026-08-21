use std::{
    cmp::Ordering,
    collections::VecDeque,
    ffi::{OsStr, OsString, c_void},
    io,
    mem::{size_of, size_of_val},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
        process::ExitStatusExt,
    },
    path::Path,
    ptr::{null, null_mut},
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering as AtomicOrdering},
    },
    thread::{self, JoinHandle},
};

use windows_sys::Win32::{
    Foundation::{
        ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF, ERROR_IO_PENDING,
        ERROR_NOT_FOUND, ERROR_OPERATION_ABORTED, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED,
        GENERIC_READ, GENERIC_WRITE, HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE,
        SetHandleInformation, WAIT_FAILED, WAIT_OBJECT_0,
    },
    Globalization::{CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN, CompareStringOrdinal},
    Security::{
        Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom},
        SECURITY_ATTRIBUTES,
    },
    Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
        OPEN_EXISTING, PIPE_ACCESS_DUPLEX, PIPE_ACCESS_INBOUND, ReadFile, WriteFile,
    },
    System::{
        IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
        JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, CreatePipe, PIPE_READMODE_BYTE,
            PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT, PeekNamedPipe,
        },
        Threading::{
            CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateEventW, CreateProcessW,
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
            INFINITE, InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            PROC_THREAD_ATTRIBUTE_JOB_LIST, PROCESS_INFORMATION, ResetEvent, STARTF_USESTDHANDLES,
            STARTUPINFOEXW, UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};

use super::{PlatformOutput, SupervisedProcessCommand, SupervisedProcessStream};

const WINDOWS_COMMAND_LINE_LIMIT: usize = 32_767;
const WINDOWS_ENVIRONMENT_LIMIT: usize = 32_767;
const PIPE_BUFFER_BYTES: u32 = 256 * 1024;
const OUTPUT_READ_CHUNK_BYTES: usize = 16 * 1024;
const OUTPUT_QUEUE_RECORD_LIMIT: usize = 512;
const OUTPUT_QUEUE_BYTE_LIMIT: usize = 256 * 1024;
const OUTPUT_POLL_RECORD_BUDGET: usize = 64;
const PIPE_NAME_ATTEMPTS: usize = 4;
const TERMINATE_EXIT_CODE: u32 = 0x4e41_5201;
const FORCE_KILL_EXIT_CODE: u32 = 0x4e41_5202;

pub(super) struct SpawnedProcess {
    pub(super) process: PlatformProcess,
    pub(super) bootstrap: Option<BootstrapChannel>,
}

pub(super) struct PlatformProcess {
    // The Job is deliberately dropped before the process handle. Kill-on-close therefore remains a
    // fail-safe even when an explicit cleanup receipt was not reached.
    job: OwnedHandle,
    process: Option<OwnedHandle>,
    exit_code: Option<u32>,
    output: Arc<OutputQueue>,
    output_poll_budget: usize,
    stdout_reader: OutputReaderOwner,
    stderr_reader: OutputReaderOwner,
}

impl PlatformProcess {
    pub(super) fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        if let Some(code) = self.exit_code {
            return Ok(Some(std::process::ExitStatus::from_raw(code)));
        }
        let process = self.process.as_ref().ok_or_else(|| {
            io::Error::other("the Windows direct-child process handle was retired without an exit")
        })?;

        // SAFETY: `process` is an owned process handle and remains live for the call.
        match unsafe { WaitForSingleObject(raw_handle(process), 0) } {
            WAIT_OBJECT_0 => {
                let code = process_exit_code(process)?;
                self.exit_code = Some(code);
                self.process.take();
                Ok(Some(std::process::ExitStatus::from_raw(code)))
            }
            windows_sys::Win32::Foundation::WAIT_TIMEOUT => Ok(None),
            WAIT_FAILED => Err(last_os_error("WaitForSingleObject(process)")),
            value => Err(io::Error::other(format!(
                "WaitForSingleObject(process) returned unexpected status {value:#x}"
            ))),
        }
    }

    pub(super) fn request_terminate(&mut self) -> io::Result<()> {
        // Windows v1 has no cooperative post-READY control channel. Termination is therefore a
        // bounded Job operation, not a misleading direct-child-only graceful signal.
        terminate_job(&self.job, TERMINATE_EXIT_CODE)
    }

    pub(super) fn force_kill(&mut self) -> io::Result<()> {
        terminate_job(&self.job, FORCE_KILL_EXIT_CODE)
    }

    pub(super) fn try_output(&mut self) -> io::Result<Option<PlatformOutput>> {
        self.stdout_reader.join_if_finished(&self.output);
        self.stderr_reader.join_if_finished(&self.output);
        if self.output_poll_budget == 0 {
            self.output_poll_budget = OUTPUT_POLL_RECORD_BUDGET;
            return Ok(None);
        }
        match self.output.pop()? {
            Some(output) => {
                self.output_poll_budget -= 1;
                Ok(Some(output))
            }
            None => {
                self.output_poll_budget = OUTPUT_POLL_RECORD_BUDGET;
                Ok(None)
            }
        }
    }

    pub(super) fn cancel_output(&mut self) -> io::Result<()> {
        let stdout = self.stdout_reader.cancel();
        let stderr = self.stderr_reader.cancel();
        stdout.and(stderr)
    }

    pub(super) fn readers_stopped(&mut self) -> bool {
        self.stdout_reader.join_if_finished(&self.output);
        self.stderr_reader.join_if_finished(&self.output);
        self.stdout_reader.joined() && self.stderr_reader.joined() && self.output.is_drained()
    }

    pub(super) fn containment_empty(&self) -> io::Result<bool> {
        Ok(job_active_processes(&self.job)? == 0)
    }

    pub(super) fn fail_safe_terminate(&mut self) {
        let _ = self.cancel_output();
        let _ = terminate_job(&self.job, FORCE_KILL_EXIT_CODE);
    }
}

impl Drop for PlatformProcess {
    fn drop(&mut self) {
        // Explicit receipts are the only clean-stop evidence. Drop still cancels readers and
        // invokes the Job's kill-on-close fail-safe for every non-retired handle state.
        self.fail_safe_terminate();
    }
}

pub(super) struct BootstrapChannel {
    parent: OwnedHandle,
    event: OwnedHandle,
}

impl BootstrapChannel {
    pub(super) fn write_all(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
            let written = overlapped_write(&self.parent, &self.event, &bytes[..length as usize])?;
            if written == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "the supervised bootstrap pipe accepted zero bytes",
                ));
            }
            bytes = &bytes[written..];
        }
        Ok(())
    }

    pub(super) fn try_read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }

        let mut available = 0_u32;
        // SAFETY: `self.parent` is a live named-pipe handle; all optional output pointers are null
        // except `available`, which points to initialized writable storage.
        let peeked = unsafe {
            PeekNamedPipe(
                raw_handle(&self.parent),
                null_mut(),
                0,
                null_mut(),
                &mut available,
                null_mut(),
            )
        };
        if peeked == 0 {
            return bootstrap_pipe_error("PeekNamedPipe(bootstrap)");
        }
        if available == 0 {
            return Ok(0);
        }

        let length = bytes
            .len()
            .min(usize::try_from(available).unwrap_or(usize::MAX));
        overlapped_read(&self.parent, &self.event, &mut bytes[..length])
    }
}

pub(super) fn spawn(command: &SupervisedProcessCommand) -> io::Result<SpawnedProcess> {
    let application = wide_path(command.executable(), "executable")?;
    let current_directory = wide_path(command.working_directory(), "working directory")?;
    let mut command_line = encode_command_line(command.executable(), command.arguments_ref())?;
    let environment = environment_block(command.environment_ref())?;

    let job = create_kill_on_close_job()?;
    let stdout = create_named_pipe_pair(PipeKind::Output, "stdout")?;
    let stderr = create_named_pipe_pair(PipeKind::Output, "stderr")?;
    let (bootstrap, bootstrap_child) = if command.bootstrap_requested() {
        let pair = create_named_pipe_pair(PipeKind::Bootstrap, "bootstrap")?;
        (
            Some(BootstrapChannel {
                parent: pair.parent,
                event: create_event()?,
            }),
            Some(pair.child),
        )
    } else {
        (None, None)
    };
    let stdin = match bootstrap_child {
        Some(handle) => handle,
        None => create_eof_stdin()?,
    };

    set_inheritable(&stdin, true)?;
    set_inheritable(&stdout.child, true)?;
    set_inheritable(&stderr.child, true)?;
    let inherited_handles = [
        raw_handle(&stdin),
        raw_handle(&stdout.child),
        raw_handle(&stderr.child),
    ];
    validate_inherited_handles(&inherited_handles)?;

    let job_handles = [raw_handle(&job)];
    let mut attributes = ProcThreadAttributeList::new(2)?;
    attributes.update(PROC_THREAD_ATTRIBUTE_JOB_LIST, &job_handles)?;
    attributes.update(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &inherited_handles)?;

    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb =
        u32::try_from(size_of::<STARTUPINFOEXW>()).expect("STARTUPINFOEXW size fits in u32");
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    // A game command reserves the duplex bootstrap endpoint as standard input. This gives the
    // product a child-local handle without putting a locator in argv or the environment; Cargo
    // receives an explicit EOF-only input pipe instead.
    startup.StartupInfo.hStdInput = raw_handle(&stdin);
    startup.StartupInfo.hStdOutput = raw_handle(&stdout.child);
    startup.StartupInfo.hStdError = raw_handle(&stderr.child);
    startup.lpAttributeList = attributes.as_ptr();

    let mut creation_flags = EXTENDED_STARTUPINFO_PRESENT;
    if environment.is_some() {
        creation_flags |= CREATE_UNICODE_ENVIRONMENT;
    }
    if !command.bootstrap_requested() {
        creation_flags |= CREATE_NO_WINDOW;
    }

    let mut process_information = PROCESS_INFORMATION::default();
    // SAFETY: every pointer refers to live, correctly sized storage for the duration of the call.
    // The attribute values and exact inheritable-handle allowlist remain alive until CreateProcessW
    // returns. PROC_THREAD_ATTRIBUTE_JOB_LIST makes Job assignment part of creation, so no project
    // instruction can run outside containment. No CREATE_SUSPENDED/late assignment fallback exists.
    let created = unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            1,
            creation_flags,
            environment
                .as_ref()
                .map_or(null(), |block| block.as_ptr().cast::<c_void>()),
            current_directory.as_ptr(),
            (&startup as *const STARTUPINFOEXW).cast(),
            &mut process_information,
        )
    };
    if created == 0 {
        return Err(last_os_error("CreateProcessW"));
    }

    let process = owned_handle(
        process_information.hProcess,
        "CreateProcessW process handle",
    )?;
    let thread = owned_handle(process_information.hThread, "CreateProcessW thread handle")?;
    drop(thread);
    drop(stdin);
    drop(stdout.child);
    drop(stderr.child);
    drop(attributes);

    let output = Arc::new(OutputQueue::new());
    let stdout_reader = match OutputReaderOwner::spawn(
        SupervisedProcessStream::Stdout,
        stdout.parent,
        Arc::clone(&output),
    ) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = terminate_job(&job, FORCE_KILL_EXIT_CODE);
            // SAFETY: `process` is a live process handle. The bounded wait is best-effort rollback,
            // not a cleanup receipt.
            unsafe {
                WaitForSingleObject(raw_handle(&process), 500);
            }
            return Err(error);
        }
    };
    let stderr_reader = match OutputReaderOwner::spawn(
        SupervisedProcessStream::Stderr,
        stderr.parent,
        Arc::clone(&output),
    ) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = stdout_reader.cancel();
            let _ = terminate_job(&job, FORCE_KILL_EXIT_CODE);
            // SAFETY: `process` is a live process handle. The bounded wait is best-effort rollback,
            // not a cleanup receipt.
            unsafe {
                WaitForSingleObject(raw_handle(&process), 500);
            }
            return Err(error);
        }
    };

    Ok(SpawnedProcess {
        process: PlatformProcess {
            job,
            process: Some(process),
            exit_code: None,
            output,
            output_poll_budget: OUTPUT_POLL_RECORD_BUDGET,
            stdout_reader,
            stderr_reader,
        },
        bootstrap,
    })
}

struct OutputReaderOwner {
    control: Option<Arc<ReaderControl>>,
    stopped: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl OutputReaderOwner {
    fn spawn(
        stream: SupervisedProcessStream,
        handle: OwnedHandle,
        output: Arc<OutputQueue>,
    ) -> io::Result<Self> {
        let control = Arc::new(ReaderControl {
            handle,
            issuing: Mutex::new(()),
            cancelled: AtomicBool::new(false),
        });
        let stopped = Arc::new(AtomicBool::new(false));
        let thread_control = Arc::clone(&control);
        let thread_stopped = Arc::clone(&stopped);
        let name = match stream {
            SupervisedProcessStream::Stdout => "nara-supervised-stdout",
            SupervisedProcessStream::Stderr => "nara-supervised-stderr",
        };
        let join = thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let _stopped = ReaderStoppedGuard(thread_stopped);
                if let Err(error) = drain_output_pipe(stream, thread_control, &output) {
                    output.record_error(error);
                }
            })?;
        Ok(Self {
            control: Some(control),
            stopped,
            join: Some(join),
        })
    }

    fn cancel(&self) -> io::Result<()> {
        self.control
            .as_ref()
            .map_or(Ok(()), |control| control.cancel())
    }

    fn join_if_finished(&mut self, output: &OutputQueue) {
        if !self.stopped.load(AtomicOrdering::Acquire) {
            return;
        }
        let Some(join) = self.join.take() else {
            return;
        };
        if join.join().is_err() {
            output.record_error(io::Error::other(
                "a supervised process output reader panicked",
            ));
        }
        self.control.take();
    }

    fn joined(&self) -> bool {
        self.join.is_none()
    }
}

struct ReaderStoppedGuard(Arc<AtomicBool>);

impl Drop for ReaderStoppedGuard {
    fn drop(&mut self) {
        self.0.store(true, AtomicOrdering::Release);
    }
}

struct ReaderControl {
    handle: OwnedHandle,
    issuing: Mutex<()>,
    cancelled: AtomicBool,
}

impl ReaderControl {
    fn cancel(&self) -> io::Result<()> {
        let _issuing = lock_unpoisoned(&self.issuing);
        self.cancelled.store(true, AtomicOrdering::Release);
        // SAFETY: the handle remains owned by `self`; a null OVERLAPPED requests cancellation of all
        // operations issued for this exact handle. The issuing mutex closes the race in which
        // cancellation could otherwise run immediately before a new ReadFile call.
        let cancelled = unsafe { CancelIoEx(raw_handle(&self.handle), null()) };
        if cancelled != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_NOT_FOUND as i32) {
            Ok(())
        } else {
            Err(with_operation("CancelIoEx(output)", error))
        }
    }
}

fn drain_output_pipe(
    stream: SupervisedProcessStream,
    control: Arc<ReaderControl>,
    output: &OutputQueue,
) -> io::Result<()> {
    let event = create_event()?;
    let mut buffer = vec![0_u8; OUTPUT_READ_CHUNK_BYTES];
    loop {
        match cancellable_overlapped_read(&control, &event, &mut buffer) {
            Ok(0) => return Ok(()),
            Ok(read) => output.push(stream, buffer[..read].to_vec()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}

fn cancellable_overlapped_read(
    control: &ReaderControl,
    event: &OwnedHandle,
    bytes: &mut [u8],
) -> io::Result<usize> {
    let issuing = lock_unpoisoned(&control.issuing);
    if control.cancelled.load(AtomicOrdering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "the supervised output reader was cancelled",
        ));
    }
    reset_event(event)?;
    let mut overlapped = Box::new(OVERLAPPED {
        hEvent: raw_handle(event),
        ..Default::default()
    });
    let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    let mut transferred = 0_u32;
    // SAFETY: the pipe, buffer, and pinned-by-Box OVERLAPPED storage remain live until completion is
    // observed below. The issuing mutex prevents cancellation from racing before this operation is
    // installed.
    let started = unsafe {
        ReadFile(
            raw_handle(&control.handle),
            bytes.as_mut_ptr(),
            length,
            &mut transferred,
            overlapped.as_mut(),
        )
    };
    if started != 0 {
        return Ok(transferred as usize);
    }
    let error = io::Error::last_os_error();
    if is_pipe_eof(&error) {
        return Ok(0);
    }
    if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
        return Err(with_operation("ReadFile(output)", error));
    }
    drop(issuing);

    wait_for_overlapped(
        &control.handle,
        event,
        &overlapped,
        "ReadFile(output)",
        OverlappedEof::Zero,
    )
}

struct OutputQueue {
    state: Mutex<OutputQueueState>,
}

impl OutputQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new(OutputQueueState::default()),
        }
    }

    fn push(&self, stream: SupervisedProcessStream, mut bytes: Vec<u8>) {
        let mut state = lock_unpoisoned(&self.state);
        if bytes.len() > OUTPUT_QUEUE_BYTE_LIMIT {
            let dropped = bytes.len() - OUTPUT_QUEUE_BYTE_LIMIT;
            bytes.drain(..dropped);
            state.record_drop(stream, dropped);
        }
        while state.records.len() >= OUTPUT_QUEUE_RECORD_LIMIT
            || state.retained_bytes.saturating_add(bytes.len()) > OUTPUT_QUEUE_BYTE_LIMIT
        {
            let Some(evicted) = state.records.pop_front() else {
                break;
            };
            state.retained_bytes = state.retained_bytes.saturating_sub(evicted.bytes.len());
            state.record_drop(evicted.stream, evicted.bytes.len());
        }
        state.retained_bytes = state.retained_bytes.saturating_add(bytes.len());
        state.records.push_back(PlatformOutput {
            stream,
            bytes,
            loss: false,
        });
    }

    fn record_error(&self, error: io::Error) {
        let mut state = lock_unpoisoned(&self.state);
        if state.error.is_none() {
            state.error = Some(error);
        }
    }

    fn pop(&self) -> io::Result<Option<PlatformOutput>> {
        let mut state = lock_unpoisoned(&self.state);
        if state.dropped_stdout > 0 {
            let dropped = std::mem::take(&mut state.dropped_stdout);
            return Ok(Some(overflow_marker(
                SupervisedProcessStream::Stdout,
                dropped,
            )));
        }
        if state.dropped_stderr > 0 {
            let dropped = std::mem::take(&mut state.dropped_stderr);
            return Ok(Some(overflow_marker(
                SupervisedProcessStream::Stderr,
                dropped,
            )));
        }
        if let Some(output) = state.records.pop_front() {
            state.retained_bytes = state.retained_bytes.saturating_sub(output.bytes.len());
            return Ok(Some(output));
        }
        if let Some(error) = state.error.take() {
            return Err(error);
        }
        Ok(None)
    }

    fn is_drained(&self) -> bool {
        let state = lock_unpoisoned(&self.state);
        state.records.is_empty()
            && state.dropped_stdout == 0
            && state.dropped_stderr == 0
            && state.error.is_none()
    }
}

#[derive(Default)]
struct OutputQueueState {
    records: VecDeque<PlatformOutput>,
    retained_bytes: usize,
    dropped_stdout: u64,
    dropped_stderr: u64,
    error: Option<io::Error>,
}

impl OutputQueueState {
    fn record_drop(&mut self, stream: SupervisedProcessStream, bytes: usize) {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        let target = match stream {
            SupervisedProcessStream::Stdout => &mut self.dropped_stdout,
            SupervisedProcessStream::Stderr => &mut self.dropped_stderr,
        };
        *target = target.saturating_add(bytes);
    }
}

fn overflow_marker(stream: SupervisedProcessStream, dropped: u64) -> PlatformOutput {
    let stream_name = match stream {
        SupervisedProcessStream::Stdout => "stdout",
        SupervisedProcessStream::Stderr => "stderr",
    };
    PlatformOutput {
        stream,
        bytes: format!(
            "[nara-editor: dropped {dropped} {stream_name} bytes before log projection]\n"
        )
        .into_bytes(),
        loss: true,
    }
}

enum PipeKind {
    Output,
    Bootstrap,
}

struct PipePair {
    parent: OwnedHandle,
    child: OwnedHandle,
}

fn create_named_pipe_pair(kind: PipeKind, label: &str) -> io::Result<PipePair> {
    let mut last_collision = None;
    for _ in 0..PIPE_NAME_ATTEMPTS {
        let name = random_pipe_name(label)?;
        let (open_mode, child_access, output_buffer, input_buffer) = match kind {
            PipeKind::Output => (
                PIPE_ACCESS_INBOUND | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                GENERIC_WRITE,
                0,
                PIPE_BUFFER_BYTES,
            ),
            PipeKind::Bootstrap => (
                PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
                GENERIC_READ | GENERIC_WRITE,
                PIPE_BUFFER_BYTES,
                PIPE_BUFFER_BYTES,
            ),
        };
        // SAFETY: `name` is NUL-terminated and all scalar flags satisfy CreateNamedPipeW. The null
        // security attributes intentionally produce a non-inheritable parent endpoint.
        let parent_raw = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                open_mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                output_buffer,
                input_buffer,
                0,
                null(),
            )
        };
        if parent_raw == INVALID_HANDLE_VALUE {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
                last_collision = Some(error);
                continue;
            }
            return Err(with_operation("CreateNamedPipeW", error));
        }
        let parent = owned_handle(parent_raw, "CreateNamedPipeW")?;
        set_inheritable(&parent, false)?;
        let connection = begin_named_pipe_connection(&parent)?;

        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
                .expect("SECURITY_ATTRIBUTES size fits in u32"),
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        // SAFETY: the pipe name is live and NUL-terminated. `attributes` is correctly initialized,
        // and the server instance remains owned until the child endpoint is connected.
        let child_raw = unsafe {
            CreateFileW(
                name.as_ptr(),
                child_access,
                0,
                &mut attributes,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };
        if child_raw == INVALID_HANDLE_VALUE {
            return Err(last_os_error("CreateFileW(private pipe client)"));
        }
        let child = owned_handle(child_raw, "CreateFileW(private pipe client)")?;
        set_inheritable(&child, true)?;
        connection.complete(&parent)?;
        return Ok(PipePair { parent, child });
    }

    Err(with_operation(
        "CreateNamedPipeW(unique private name)",
        last_collision.unwrap_or_else(|| io::Error::from_raw_os_error(ERROR_ACCESS_DENIED as i32)),
    ))
}

struct PendingPipeConnection {
    pipe: HANDLE,
    event: OwnedHandle,
    overlapped: Box<OVERLAPPED>,
    pending: bool,
}

impl PendingPipeConnection {
    fn complete(mut self, pipe: &OwnedHandle) -> io::Result<()> {
        if self.pending {
            wait_for_overlapped(
                pipe,
                &self.event,
                &self.overlapped,
                "ConnectNamedPipe",
                OverlappedEof::Zero,
            )?;
            self.pending = false;
        }
        Ok(())
    }
}

impl Drop for PendingPipeConnection {
    fn drop(&mut self) {
        if !self.pending {
            return;
        }
        // SAFETY: `pipe` is borrowed from the still-live parent endpoint in
        // `create_named_pipe_pair`; cancellation and the event wait complete the kernel operation
        // before the boxed OVERLAPPED storage is dropped.
        unsafe {
            CancelIoEx(self.pipe, null());
            WaitForSingleObject(raw_handle(&self.event), INFINITE);
            let mut transferred = 0_u32;
            GetOverlappedResult(self.pipe, self.overlapped.as_ref(), &mut transferred, 0);
        }
    }
}

fn begin_named_pipe_connection(pipe: &OwnedHandle) -> io::Result<PendingPipeConnection> {
    let event = create_event()?;
    let mut overlapped = Box::new(OVERLAPPED {
        hEvent: raw_handle(&event),
        ..Default::default()
    });
    // SAFETY: the server pipe, event, and boxed OVERLAPPED remain live until the connection is
    // completed after the synchronous child CreateFileW call below.
    let connected = unsafe { ConnectNamedPipe(raw_handle(pipe), overlapped.as_mut()) };
    if connected != 0 {
        return Ok(PendingPipeConnection {
            pipe: raw_handle(pipe),
            event,
            overlapped,
            pending: false,
        });
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(code) if code == ERROR_PIPE_CONNECTED as i32 => Ok(PendingPipeConnection {
            pipe: raw_handle(pipe),
            event,
            overlapped,
            pending: false,
        }),
        Some(code) if code == ERROR_IO_PENDING as i32 => Ok(PendingPipeConnection {
            pipe: raw_handle(pipe),
            event,
            overlapped,
            pending: true,
        }),
        _ => Err(with_operation("ConnectNamedPipe", error)),
    }
}

fn random_pipe_name(label: &str) -> io::Result<Vec<u16>> {
    let mut nonce = [0_u8; 16];
    // SAFETY: a null algorithm handle is required with BCRYPT_USE_SYSTEM_PREFERRED_RNG, and `nonce`
    // is writable for exactly the supplied length.
    let status = unsafe {
        BCryptGenRandom(
            null_mut(),
            nonce.as_mut_ptr(),
            u32::try_from(nonce.len()).expect("nonce length fits in u32"),
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(io::Error::other(format!(
            "BCryptGenRandom failed with NTSTATUS {status:#x}"
        )));
    }
    let mut token = String::with_capacity(nonce.len() * 2);
    for byte in nonce {
        use std::fmt::Write as _;
        write!(&mut token, "{byte:02x}").expect("writing to a String is infallible");
    }
    let name = format!(r"\\.\pipe\nara-editor-supervised-{label}-{token}");
    Ok(nul_terminated_wide(OsStr::new(&name), "private pipe name")?)
}

fn create_eof_stdin() -> io::Result<OwnedHandle> {
    let mut read = null_mut();
    let mut write = null_mut();
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
            .expect("SECURITY_ATTRIBUTES size fits in u32"),
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    // SAFETY: output pointers and SECURITY_ATTRIBUTES are valid for the call.
    let created = unsafe { CreatePipe(&mut read, &mut write, &mut attributes, 0) };
    if created == 0 {
        return Err(last_os_error("CreatePipe(stdin EOF)"));
    }
    let read = owned_handle(read, "CreatePipe(stdin read)")?;
    let write = owned_handle(write, "CreatePipe(stdin write)")?;
    set_inheritable(&read, true)?;
    set_inheritable(&write, false)?;
    drop(write);
    Ok(read)
}

fn create_kill_on_close_job() -> io::Result<OwnedHandle> {
    // SAFETY: null security attributes and name request one private, non-inheritable Job.
    let job = unsafe { CreateJobObjectW(null(), null()) };
    let job = owned_handle(job, "CreateJobObjectW")?;
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    // SAFETY: `job` is valid and `limits` has the exact structure required by the selected class.
    let configured = unsafe {
        SetInformationJobObject(
            raw_handle(&job),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            u32::try_from(size_of_val(&limits)).expect("Job limit structure size fits in u32"),
        )
    };
    if configured == 0 {
        return Err(last_os_error(
            "SetInformationJobObject(kill-on-close, non-breakaway)",
        ));
    }
    Ok(job)
}

fn terminate_job(job: &OwnedHandle, exit_code: u32) -> io::Result<()> {
    // SAFETY: `job` is an owned Job handle and remains live for the call.
    if unsafe { TerminateJobObject(raw_handle(job), exit_code) } == 0 {
        let error = io::Error::last_os_error();
        if job_active_processes(job)? != 0 {
            return Err(with_operation("TerminateJobObject", error));
        }
    }
    Ok(())
}

fn job_active_processes(job: &OwnedHandle) -> io::Result<u32> {
    let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    let mut returned = 0_u32;
    // SAFETY: `job` is live and `accounting` is writable for the exact selected information class.
    let queried = unsafe {
        QueryInformationJobObject(
            raw_handle(job),
            JobObjectBasicAccountingInformation,
            (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
            u32::try_from(size_of_val(&accounting))
                .expect("Job accounting structure size fits in u32"),
            &mut returned,
        )
    };
    if queried == 0 {
        return Err(last_os_error("QueryInformationJobObject(accounting)"));
    }
    if returned != 0
        && returned
            != u32::try_from(size_of_val(&accounting))
                .expect("Job accounting structure size fits in u32")
    {
        return Err(io::Error::other(format!(
            "QueryInformationJobObject returned unexpected accounting size {returned}"
        )));
    }
    Ok(accounting.ActiveProcesses)
}

fn process_exit_code(process: &OwnedHandle) -> io::Result<u32> {
    let mut code = 0_u32;
    // SAFETY: `process` is live and `code` is writable for one u32.
    if unsafe { GetExitCodeProcess(raw_handle(process), &mut code) } == 0 {
        Err(last_os_error("GetExitCodeProcess"))
    } else {
        Ok(code)
    }
}

struct ProcThreadAttributeList {
    _storage: Box<[usize]>,
    pointer: *mut c_void,
}

impl ProcThreadAttributeList {
    fn new(attribute_count: u32) -> io::Result<Self> {
        let mut bytes = 0_usize;
        // SAFETY: the documented sizing call accepts a null list and writes only `bytes`.
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), attribute_count, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(last_os_error(
                "InitializeProcThreadAttributeList(size query)",
            ));
        }
        let words = bytes.div_ceil(size_of::<usize>());
        let mut storage = vec![0_usize; words].into_boxed_slice();
        let pointer = storage.as_mut_ptr().cast::<c_void>();
        // SAFETY: `storage` is pointer-aligned and at least the byte count returned by the sizing
        // call. It remains owned by the wrapper until DeleteProcThreadAttributeList runs.
        if unsafe { InitializeProcThreadAttributeList(pointer, attribute_count, 0, &mut bytes) }
            == 0
        {
            return Err(last_os_error("InitializeProcThreadAttributeList"));
        }
        Ok(Self {
            _storage: storage,
            pointer,
        })
    }

    fn update<T>(&mut self, attribute: u32, values: &[T]) -> io::Result<()> {
        if values.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "process startup attributes cannot contain an empty value list",
            ));
        }
        // SAFETY: the list is initialized, and `values` remains live and immutable through the
        // subsequent CreateProcessW call in `spawn`.
        let updated = unsafe {
            UpdateProcThreadAttribute(
                self.pointer,
                0,
                attribute as usize,
                values.as_ptr().cast::<c_void>(),
                size_of_val(values),
                null_mut(),
                null(),
            )
        };
        if updated == 0 {
            Err(last_os_error("UpdateProcThreadAttribute"))
        } else {
            Ok(())
        }
    }

    fn as_ptr(&self) -> *mut c_void {
        self.pointer
    }
}

impl Drop for ProcThreadAttributeList {
    fn drop(&mut self) {
        // SAFETY: `pointer` was initialized successfully and has not been deleted yet.
        unsafe { DeleteProcThreadAttributeList(self.pointer) };
    }
}

fn encode_command_line(executable: &Path, arguments: &[OsString]) -> io::Result<Vec<u16>> {
    let mut encoded = Vec::new();
    append_windows_argument(&mut encoded, executable.as_os_str())?;
    for argument in arguments {
        encoded.push(b' ' as u16);
        append_windows_argument(&mut encoded, argument)?;
    }
    if encoded.len() >= WINDOWS_COMMAND_LINE_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the encoded Windows command line exceeds 32,767 UTF-16 code units",
        ));
    }
    encoded.push(0);
    Ok(encoded)
}

fn append_windows_argument(output: &mut Vec<u16>, argument: &OsStr) -> io::Result<()> {
    let units = wide_without_nul(argument, "process argument")?;
    let quoted = units.is_empty()
        || units.iter().any(|unit| {
            *unit == u16::from(b' ') || *unit == u16::from(b'\t') || *unit == u16::from(b'"')
        });
    if !quoted {
        output.extend(units);
        return Ok(());
    }

    output.push(b'"' as u16);
    let mut backslashes = 0_usize;
    for unit in units {
        if unit == b'\\' as u16 {
            backslashes += 1;
            continue;
        }
        if unit == b'"' as u16 {
            output.extend(std::iter::repeat_n(
                b'\\' as u16,
                backslashes.saturating_mul(2).saturating_add(1),
            ));
            output.push(unit);
        } else {
            output.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
            output.push(unit);
        }
        backslashes = 0;
    }
    output.extend(std::iter::repeat_n(
        b'\\' as u16,
        backslashes.saturating_mul(2),
    ));
    output.push(b'"' as u16);
    Ok(())
}

fn environment_block(overrides: &[(OsString, OsString)]) -> io::Result<Option<Vec<u16>>> {
    if overrides.is_empty() {
        return Ok(None);
    }

    let mut entries: Vec<(OsString, OsString)> = Vec::new();
    for (name, value) in std::env::vars_os() {
        if let Some(existing) = entries
            .iter_mut()
            .find(|(existing_name, _)| environment_names_equal(existing_name, &name))
        {
            *existing = (name, value);
        } else {
            entries.push((name, value));
        }
    }
    for (name, value) in overrides {
        validate_environment_name(name)?;
        wide_without_nul(value, "environment value")?;
        entries.retain(|(existing, _)| !environment_names_equal(existing, name));
        entries.push((name.clone(), value.clone()));
    }
    entries.sort_by(|(left, _), (right, _)| compare_environment_names(left, right));

    let mut block = Vec::new();
    for (name, value) in entries {
        block.extend(wide_without_nul(&name, "environment name")?);
        block.push(b'=' as u16);
        block.extend(wide_without_nul(&value, "environment value")?);
        block.push(0);
    }
    block.push(0);
    if block.len() > WINDOWS_ENVIRONMENT_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the encoded Windows environment exceeds 32,767 UTF-16 code units",
        ));
    }
    Ok(Some(block))
}

fn validate_environment_name(name: &OsStr) -> io::Result<()> {
    let units = wide_without_nul(name, "environment name")?;
    if units.is_empty() || units.contains(&(b'=' as u16)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a Windows environment variable name must be non-empty and contain no '='",
        ));
    }
    Ok(())
}

fn environment_names_equal(left: &OsStr, right: &OsStr) -> bool {
    compare_environment_names(left, right) == Ordering::Equal
}

fn compare_environment_names(left: &OsStr, right: &OsStr) -> Ordering {
    let left = left.encode_wide().collect::<Vec<_>>();
    let right = right.encode_wide().collect::<Vec<_>>();
    let left_len = i32::try_from(left.len()).unwrap_or(i32::MAX);
    let right_len = i32::try_from(right.len()).unwrap_or(i32::MAX);
    // SAFETY: both pointers are valid for their exact, bounded lengths. Ignore-case ordinal
    // comparison is the ordering required for a CreateProcessW environment block.
    match unsafe { CompareStringOrdinal(left.as_ptr(), left_len, right.as_ptr(), right_len, 1) } {
        CSTR_LESS_THAN => Ordering::Less,
        CSTR_EQUAL => Ordering::Equal,
        CSTR_GREATER_THAN => Ordering::Greater,
        _ => left.cmp(&right),
    }
}

fn wide_path(path: &Path, label: &'static str) -> io::Result<Vec<u16>> {
    nul_terminated_wide(path.as_os_str(), label)
}

fn nul_terminated_wide(value: &OsStr, label: &'static str) -> io::Result<Vec<u16>> {
    let mut units = wide_without_nul(value, label)?;
    if units.len() >= WINDOWS_COMMAND_LINE_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("the Windows {label} exceeds 32,767 UTF-16 code units"),
        ));
    }
    units.push(0);
    Ok(units)
}

fn wide_without_nul(value: &OsStr, label: &'static str) -> io::Result<Vec<u16>> {
    let units = value.encode_wide().collect::<Vec<_>>();
    if units.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("the Windows {label} contains an interior NUL"),
        ));
    }
    Ok(units)
}

fn overlapped_read(
    handle: &OwnedHandle,
    event: &OwnedHandle,
    bytes: &mut [u8],
) -> io::Result<usize> {
    reset_event(event)?;
    let mut overlapped = Box::new(OVERLAPPED {
        hEvent: raw_handle(event),
        ..Default::default()
    });
    let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    let mut transferred = 0_u32;
    // SAFETY: handle, buffer, event, and boxed OVERLAPPED remain live until completion below.
    let started = unsafe {
        ReadFile(
            raw_handle(handle),
            bytes.as_mut_ptr(),
            length,
            &mut transferred,
            overlapped.as_mut(),
        )
    };
    if started != 0 {
        return Ok(transferred as usize);
    }
    let error = io::Error::last_os_error();
    if is_pipe_eof(&error) {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the supervised bootstrap channel closed before a complete reply",
        ));
    }
    if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
        return Err(with_operation("ReadFile(bootstrap)", error));
    }
    wait_for_overlapped(
        handle,
        event,
        &overlapped,
        "ReadFile(bootstrap)",
        OverlappedEof::Unexpected,
    )
}

fn overlapped_write(handle: &OwnedHandle, event: &OwnedHandle, bytes: &[u8]) -> io::Result<usize> {
    reset_event(event)?;
    let mut overlapped = Box::new(OVERLAPPED {
        hEvent: raw_handle(event),
        ..Default::default()
    });
    let length = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    let mut transferred = 0_u32;
    // SAFETY: handle, buffer, event, and boxed OVERLAPPED remain live until completion below.
    let started = unsafe {
        WriteFile(
            raw_handle(handle),
            bytes.as_ptr(),
            length,
            &mut transferred,
            overlapped.as_mut(),
        )
    };
    if started != 0 {
        return Ok(transferred as usize);
    }
    let error = io::Error::last_os_error();
    if is_pipe_eof(&error) {
        return Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "the supervised bootstrap channel is closed",
        ));
    }
    if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
        return Err(with_operation("WriteFile(bootstrap)", error));
    }
    wait_for_overlapped(
        handle,
        event,
        &overlapped,
        "WriteFile(bootstrap)",
        OverlappedEof::Broken,
    )
}

#[derive(Clone, Copy)]
enum OverlappedEof {
    Zero,
    Unexpected,
    Broken,
}

fn wait_for_overlapped(
    handle: &OwnedHandle,
    event: &OwnedHandle,
    overlapped: &OVERLAPPED,
    operation: &'static str,
    eof: OverlappedEof,
) -> io::Result<usize> {
    // SAFETY: `event` remains live while waiting.
    match unsafe { WaitForSingleObject(raw_handle(event), INFINITE) } {
        WAIT_OBJECT_0 => {}
        WAIT_FAILED => return Err(last_os_error("WaitForSingleObject(overlapped I/O)")),
        value => {
            return Err(io::Error::other(format!(
                "WaitForSingleObject(overlapped I/O) returned unexpected status {value:#x}"
            )));
        }
    }

    let mut transferred = 0_u32;
    // SAFETY: the exact handle and OVERLAPPED used to issue the operation remain live; the event has
    // signaled, so this non-waiting query cannot outlive caller-owned storage.
    let completed =
        unsafe { GetOverlappedResult(raw_handle(handle), overlapped, &mut transferred, 0) };
    if completed != 0 {
        return Ok(transferred as usize);
    }
    let error = io::Error::last_os_error();
    if is_pipe_eof(&error) {
        return match eof {
            OverlappedEof::Zero => Ok(0),
            OverlappedEof::Unexpected => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the supervised bootstrap channel closed before a complete reply",
            )),
            OverlappedEof::Broken => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the supervised bootstrap channel is closed",
            )),
        };
    }
    if error.raw_os_error() == Some(ERROR_OPERATION_ABORTED as i32) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            format!("{operation} was cancelled"),
        ));
    }
    Err(with_operation(operation, error))
}

fn create_event() -> io::Result<OwnedHandle> {
    // SAFETY: null security attributes/name request one private manual-reset event.
    let event = unsafe { CreateEventW(null(), 1, 0, null()) };
    owned_handle(event, "CreateEventW")
}

fn reset_event(event: &OwnedHandle) -> io::Result<()> {
    // SAFETY: `event` is an owned event handle.
    if unsafe { ResetEvent(raw_handle(event)) } == 0 {
        Err(last_os_error("ResetEvent"))
    } else {
        Ok(())
    }
}

fn set_inheritable(handle: &OwnedHandle, inheritable: bool) -> io::Result<()> {
    // SAFETY: `handle` is live; the mask changes only HANDLE_FLAG_INHERIT.
    let changed = unsafe {
        SetHandleInformation(
            raw_handle(handle),
            HANDLE_FLAG_INHERIT,
            if inheritable { HANDLE_FLAG_INHERIT } else { 0 },
        )
    };
    if changed == 0 {
        Err(last_os_error("SetHandleInformation(inherit flag)"))
    } else {
        Ok(())
    }
}

fn validate_inherited_handles(handles: &[HANDLE]) -> io::Result<()> {
    for (index, handle) in handles.iter().enumerate() {
        if handle.is_null() || *handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the inherited handle allowlist contains an invalid handle",
            ));
        }
        if handles[..index].contains(handle) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "the inherited handle allowlist contains a duplicate handle",
            ));
        }
    }
    Ok(())
}

fn is_pipe_eof(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code)
            if code == ERROR_BROKEN_PIPE as i32
                || code == ERROR_HANDLE_EOF as i32
                || code == ERROR_PIPE_NOT_CONNECTED as i32
    )
}

fn bootstrap_pipe_error(operation: &'static str) -> io::Result<usize> {
    let error = io::Error::last_os_error();
    if is_pipe_eof(&error) {
        Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the supervised bootstrap channel closed before READY",
        ))
    } else {
        Err(with_operation(operation, error))
    }
}

fn owned_handle(raw: HANDLE, operation: &'static str) -> io::Result<OwnedHandle> {
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        return Err(last_os_error(operation));
    }
    // SAFETY: success from the creating Win32 call transfers one uniquely owned handle to Rust.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw as RawHandle) })
}

fn raw_handle(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle() as HANDLE
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn last_os_error(operation: &'static str) -> io::Error {
    with_operation(operation, io::Error::last_os_error())
}

fn with_operation(operation: &'static str, error: io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{operation} failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_argument_encoder_handles_empty_quotes_and_trailing_backslashes() {
        assert_eq!(encode_argument("plain"), "plain");
        assert_eq!(encode_argument(""), "\"\"");
        assert_eq!(encode_argument("two words"), "\"two words\"");
        assert_eq!(encode_argument("quote\"inside"), "\"quote\\\"inside\"");
        assert_eq!(
            encode_argument("directory with space\\"),
            "\"directory with space\\\\\""
        );
    }

    #[test]
    fn windows_command_line_always_contains_an_exact_argv_zero() {
        let encoded = encode_command_line(
            Path::new(r"C:\Program Files\Nara\desktop.exe"),
            &[OsString::from("--fixed"), OsString::from("two words")],
        )
        .expect("the fixed command line is encodable");
        assert_eq!(
            String::from_utf16(&encoded[..encoded.len() - 1])
                .expect("the test command line is valid UTF-16"),
            r#""C:\Program Files\Nara\desktop.exe" --fixed "two words""#
        );
    }

    #[test]
    fn environment_overrides_use_windows_ordinal_case_insensitive_ordering() {
        assert!(environment_names_equal(
            OsStr::new("CARGO_HOME"),
            OsStr::new("cargo_home")
        ));
        assert_eq!(
            compare_environment_names(OsStr::new("A_PATH"), OsStr::new("b_path")),
            Ordering::Less
        );
    }

    fn encode_argument(argument: &str) -> String {
        let mut encoded = Vec::new();
        append_windows_argument(&mut encoded, OsStr::new(argument))
            .expect("the test argument is encodable");
        String::from_utf16(&encoded).expect("the test argument is valid UTF-16")
    }
}
