use std::io;

use super::{PlatformOutput, SupervisedProcessCommand, SupervisedProcessStream};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::{
    collections::VecDeque,
    ffi::{c_long, c_uint},
    io::{Read, Write},
    os::{
        fd::OwnedFd,
        unix::{net::UnixStream, process::CommandExt},
    },
    process::{Child, ChildStderr, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use rustix::{
    event::{PollFd, PollFlags, poll},
    fs::{OFlags, fcntl_getfl, fcntl_setfl},
    process::{Pid, Signal, kill_process_group, test_kill_process_group},
};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MAX_PENDING_OUTPUT_RECORDS: usize = 512;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MAX_PENDING_OUTPUT_BYTES: usize = 256 * 1024;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const MAX_READ_BYTES_PER_STREAM_PER_WAKE: usize = 64 * 1024;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const OUTPUT_READ_CHUNK_BYTES: usize = 8 * 1024;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const OUTPUT_POLL_RECORD_BUDGET: usize = 64;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const SPAWN_ROLLBACK_WAIT: Duration = Duration::from_millis(500);
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const SPAWN_ROLLBACK_POLL: Duration = Duration::from_millis(10);
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const SYS_CLOSE_RANGE_X86_64: c_long = 436;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const CLOSE_RANGE_CLOEXEC: c_uint = 1 << 2;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
unsafe extern "C" {
    fn syscall(number: c_long, ...) -> c_long;
}

pub(super) struct SpawnedProcess {
    pub(super) process: PlatformProcess,
    pub(super) bootstrap: Option<BootstrapChannel>,
}

pub(super) struct BootstrapChannel {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    stream: UnixStream,
}

impl BootstrapChannel {
    pub(super) fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return self.stream.write_all(bytes);
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let _ = bytes;
            Err(unsupported_platform())
        }
    }

    pub(super) fn try_read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return match self.stream.read(bytes) {
                Ok(0) => Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the supervised bootstrap endpoint closed",
                )),
                Ok(read) => Ok(read),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(0),
                Err(error) => Err(error),
            };
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let _ = bytes;
            Err(unsupported_platform())
        }
    }
}

pub(super) struct PlatformProcess {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    child: Option<Child>,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    process_group: Pid,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    direct_child_reaped: bool,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output: Arc<Mutex<OutputQueue>>,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output_poll_budget: usize,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output_cancel: UnixStream,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output_cancel_requested: bool,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    output_reader: Option<JoinHandle<()>>,
}

impl PlatformProcess {
    pub(super) fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let child = self.child.as_mut().ok_or_else(process_owner_retired)?;
            let status = child.try_wait()?;
            if status.is_some() {
                self.direct_child_reaped = true;
            }
            return Ok(status);
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn request_terminate(&mut self) -> io::Result<()> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return signal_process_group(self.process_group, Signal::TERM);
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn force_kill(&mut self) -> io::Result<()> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return signal_process_group(self.process_group, Signal::KILL);
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn try_output(&mut self) -> io::Result<Option<PlatformOutput>> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if self.output_poll_budget == 0 {
                self.output_poll_budget = OUTPUT_POLL_RECORD_BUDGET;
                return Ok(None);
            }
            let mut output = self.output.lock().map_err(output_lock_poisoned)?;
            return match output.pop()? {
                Some(output) => {
                    self.output_poll_budget -= 1;
                    Ok(Some(output))
                }
                None => {
                    self.output_poll_budget = OUTPUT_POLL_RECORD_BUDGET;
                    Ok(None)
                }
            };
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn cancel_output(&mut self) -> io::Result<()> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if self.output_cancel_requested {
                return Ok(());
            }
            match self.output_cancel.write(&[1]) {
                Ok(1) => {
                    self.output_cancel_requested = true;
                    Ok(())
                }
                Ok(_) => Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "the supervised process output cancellation endpoint closed",
                )),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::BrokenPipe
                    ) =>
                {
                    self.output_cancel_requested = true;
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn readers_stopped(&mut self) -> bool {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let (stopped, pending_output) = self
                .output
                .lock()
                .map(|output| {
                    (
                        output.stopped,
                        !output.records.is_empty()
                            || output.dropped_stdout_bytes != 0
                            || output.dropped_stderr_bytes != 0
                            || output.failure.is_some(),
                    )
                })
                .unwrap_or((false, true));
            if !stopped || pending_output {
                return false;
            }
            let Some(reader) = self.output_reader.take() else {
                return true;
            };
            if reader.join().is_ok() {
                return true;
            }
            if let Ok(mut output) = self.output.lock() {
                output.record_failure(io::Error::other(
                    "the supervised process output reader panicked",
                ));
            }
            false
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            false
        }
    }

    pub(super) fn containment_empty(&self) -> io::Result<bool> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            if !self.direct_child_reaped {
                return Ok(false);
            }
            return match test_kill_process_group(self.process_group) {
                Ok(()) => Ok(false),
                Err(rustix::io::Errno::SRCH) => Ok(true),
                Err(error) => Err(error.into()),
            };
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            Err(unsupported_platform())
        }
    }

    pub(super) fn fail_safe_terminate(&mut self) {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let _ = signal_process_group(self.process_group, Signal::KILL);
            let _ = self.cancel_output();
            let Some(mut child) = self.child.take() else {
                return;
            };
            if child.try_wait().ok().flatten().is_some() {
                self.direct_child_reaped = true;
                return;
            }
            let _ = thread::Builder::new()
                .name("nara-supervised-process-reaper".to_owned())
                .spawn(move || {
                    let _ = child.wait();
                });
        }
    }
}

pub(super) fn spawn(command: &SupervisedProcessCommand) -> io::Result<SpawnedProcess> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return spawn_supported(command);
    }

    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = command;
        Err(unsupported_platform())
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn spawn_supported(command: &SupervisedProcessCommand) -> io::Result<SpawnedProcess> {
    let (bootstrap, child_bootstrap) = if command.bootstrap_requested() {
        let (parent, child) = UnixStream::pair()?;
        parent.set_nonblocking(true)?;
        let child: OwnedFd = child.into();
        (
            Some(BootstrapChannel { stream: parent }),
            Stdio::from(child),
        )
    } else {
        (None, Stdio::null())
    };
    let (output_cancel, output_cancel_reader) = UnixStream::pair()?;
    output_cancel.set_nonblocking(true)?;
    output_cancel_reader.set_nonblocking(true)?;

    let mut native = Command::new(command.executable());
    native
        .args(command.arguments_ref())
        .current_dir(command.working_directory())
        .envs(
            command
                .environment_ref()
                .iter()
                .map(|(name, value)| (name, value)),
        )
        .stdin(child_bootstrap)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    native.process_group(0);
    // SAFETY: The child-side hook performs one async-signal-safe syscall and returns. Marking every
    // descriptor above stderr close-on-exec preserves Command's private error pipe until exec while
    // proving that only stdin/bootstrap, stdout, and stderr enter project code.
    unsafe {
        native.pre_exec(close_non_stdio_on_exec);
    }

    let mut child = native.spawn()?;
    let process_group = Pid::from_child(&child);
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => return spawn_setup_failed(child, process_group, "stdout pipe was not created"),
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => return spawn_setup_failed(child, process_group, "stderr pipe was not created"),
    };

    if let Err(error) = make_nonblocking(&stdout).and_then(|()| make_nonblocking(&stderr)) {
        terminate_spawn_failure(child, process_group);
        return Err(error);
    }
    let output = Arc::new(Mutex::new(OutputQueue::default()));
    let reader_output = Arc::clone(&output);
    let output_reader = match thread::Builder::new()
        .name("nara-supervised-output".to_owned())
        .spawn(move || run_output_reader(stdout, stderr, output_cancel_reader, reader_output))
    {
        Ok(reader) => reader,
        Err(error) => {
            terminate_spawn_failure(child, process_group);
            return Err(error);
        }
    };

    Ok(SpawnedProcess {
        process: PlatformProcess {
            child: Some(child),
            process_group,
            direct_child_reaped: false,
            output,
            output_poll_budget: OUTPUT_POLL_RECORD_BUDGET,
            output_cancel,
            output_cancel_requested: false,
            output_reader: Some(output_reader),
        },
        bootstrap,
    })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn spawn_setup_failed(
    child: Child,
    process_group: Pid,
    message: &'static str,
) -> io::Result<SpawnedProcess> {
    terminate_spawn_failure(child, process_group);
    Err(io::Error::other(message))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn terminate_spawn_failure(mut child: Child, process_group: Pid) {
    let _ = signal_process_group(process_group, Signal::KILL);
    let _ = child.kill();
    let deadline = Instant::now() + SPAWN_ROLLBACK_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => thread::sleep(SPAWN_ROLLBACK_POLL),
            Ok(None) | Err(_) => break,
        }
    }
    let _ = thread::Builder::new()
        .name("nara-supervised-spawn-rollback-reaper".to_owned())
        .spawn(move || {
            let _ = child.wait();
        });
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn make_nonblocking(fd: &impl std::os::fd::AsFd) -> io::Result<()> {
    let flags = fcntl_getfl(fd).map_err(io::Error::from)?;
    fcntl_setfl(fd, flags | OFlags::NONBLOCK).map_err(io::Error::from)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn signal_process_group(process_group: Pid, signal: Signal) -> io::Result<()> {
    match kill_process_group(process_group, signal) {
        Ok(()) | Err(rustix::io::Errno::SRCH) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn close_non_stdio_on_exec() -> io::Result<()> {
    // SAFETY: `syscall` receives the Linux x86-64 close_range number and integer-only arguments.
    // The hook runs after fork and before exec, where this syscall is async-signal-safe.
    let result = unsafe { syscall(SYS_CLOSE_RANGE_X86_64, 3_u32, u32::MAX, CLOSE_RANGE_CLOEXEC) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn run_output_reader(
    mut stdout: ChildStdout,
    mut stderr: ChildStderr,
    mut cancel: UnixStream,
    output: Arc<Mutex<OutputQueue>>,
) {
    let result = read_output_until_cancelled(&mut stdout, &mut stderr, &mut cancel, &output);
    if let Ok(mut output) = output.lock() {
        if let Err(error) = result {
            output.record_failure(error);
        }
        output.stopped = true;
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn read_output_until_cancelled(
    stdout: &mut ChildStdout,
    stderr: &mut ChildStderr,
    cancel: &mut UnixStream,
    output: &Arc<Mutex<OutputQueue>>,
) -> io::Result<()> {
    let mut stdout_open = true;
    let mut stderr_open = true;

    loop {
        if !stdout_open && !stderr_open {
            return Ok(());
        }

        let mut poll_fds = Vec::with_capacity(3);
        let cancel_index = poll_fds.len();
        poll_fds.push(PollFd::new(
            &*cancel,
            PollFlags::IN | PollFlags::HUP | PollFlags::ERR,
        ));
        let stdout_index = stdout_open.then(|| {
            let index = poll_fds.len();
            poll_fds.push(PollFd::new(
                &*stdout,
                PollFlags::IN | PollFlags::HUP | PollFlags::ERR,
            ));
            index
        });
        let stderr_index = stderr_open.then(|| {
            let index = poll_fds.len();
            poll_fds.push(PollFd::new(
                &*stderr,
                PollFlags::IN | PollFlags::HUP | PollFlags::ERR,
            ));
            index
        });

        match poll(&mut poll_fds, None) {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
        let cancel_events = poll_fds[cancel_index].revents();
        let stdout_events = stdout_index.map(|index| poll_fds[index].revents());
        let stderr_events = stderr_index.map(|index| poll_fds[index].revents());
        drop(poll_fds);

        if cancel_events.intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR) {
            let mut discard = [0_u8; 32];
            let _ = cancel.read(&mut discard);
            if stdout_open && !drain_stream(stdout, SupervisedProcessStream::Stdout, output)? {
                record_cancelled_stream(output, SupervisedProcessStream::Stdout)?;
            }
            if stderr_open && !drain_stream(stderr, SupervisedProcessStream::Stderr, output)? {
                record_cancelled_stream(output, SupervisedProcessStream::Stderr)?;
            }
            return Ok(());
        }
        if stdout_events.is_some_and(|events| events.contains(PollFlags::NVAL))
            || stderr_events.is_some_and(|events| events.contains(PollFlags::NVAL))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a supervised process output descriptor became invalid",
            ));
        }
        if stdout_events.is_some_and(output_ready) {
            stdout_open = !drain_stream(stdout, SupervisedProcessStream::Stdout, output)?;
        }
        if stderr_events.is_some_and(output_ready) {
            stderr_open = !drain_stream(stderr, SupervisedProcessStream::Stderr, output)?;
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn output_ready(events: PollFlags) -> bool {
    events.intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn drain_stream(
    reader: &mut impl Read,
    stream: SupervisedProcessStream,
    output: &Arc<Mutex<OutputQueue>>,
) -> io::Result<bool> {
    let mut bytes_read = 0;
    let mut buffer = [0_u8; OUTPUT_READ_CHUNK_BYTES];
    while bytes_read < MAX_READ_BYTES_PER_STREAM_PER_WAKE {
        let remaining = MAX_READ_BYTES_PER_STREAM_PER_WAKE - bytes_read;
        let read_limit = remaining.min(buffer.len());
        match reader.read(&mut buffer[..read_limit]) {
            Ok(0) => return Ok(true),
            Ok(read) => {
                bytes_read += read;
                output
                    .lock()
                    .map_err(output_lock_poisoned)?
                    .push(PlatformOutput {
                        stream,
                        bytes: buffer[..read].to_vec(),
                        loss: false,
                    });
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[derive(Default)]
struct OutputQueue {
    records: VecDeque<PlatformOutput>,
    retained_bytes: usize,
    dropped_stdout_bytes: u64,
    dropped_stderr_bytes: u64,
    failure: Option<io::Error>,
    stopped: bool,
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl OutputQueue {
    fn push(&mut self, output: PlatformOutput) {
        let bytes = output.bytes.len();
        if self.records.len() >= MAX_PENDING_OUTPUT_RECORDS
            || self.retained_bytes.saturating_add(bytes) > MAX_PENDING_OUTPUT_BYTES
        {
            let dropped = u64::try_from(bytes).unwrap_or(u64::MAX);
            match output.stream {
                SupervisedProcessStream::Stdout => {
                    self.dropped_stdout_bytes = self.dropped_stdout_bytes.saturating_add(dropped);
                }
                SupervisedProcessStream::Stderr => {
                    self.dropped_stderr_bytes = self.dropped_stderr_bytes.saturating_add(dropped);
                }
            }
            return;
        }
        self.retained_bytes += bytes;
        self.records.push_back(output);
    }

    fn pop(&mut self) -> io::Result<Option<PlatformOutput>> {
        if let Some(output) = self.records.pop_front() {
            self.retained_bytes -= output.bytes.len();
            return Ok(Some(output));
        }
        if self.dropped_stdout_bytes != 0 {
            let dropped = std::mem::take(&mut self.dropped_stdout_bytes);
            return Ok(Some(dropped_output(
                SupervisedProcessStream::Stdout,
                dropped,
            )));
        }
        if self.dropped_stderr_bytes != 0 {
            let dropped = std::mem::take(&mut self.dropped_stderr_bytes);
            return Ok(Some(dropped_output(
                SupervisedProcessStream::Stderr,
                dropped,
            )));
        }
        if let Some(error) = self.failure.take() {
            return Err(error);
        }
        Ok(None)
    }

    fn record_failure(&mut self, error: io::Error) {
        if self.failure.is_none() {
            self.failure = Some(error);
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn dropped_output(stream: SupervisedProcessStream, dropped: u64) -> PlatformOutput {
    let name = match stream {
        SupervisedProcessStream::Stdout => "stdout",
        SupervisedProcessStream::Stderr => "stderr",
    };
    PlatformOutput {
        stream,
        bytes: format!("[nara-editor: dropped {dropped} bytes from {name}]\n").into_bytes(),
        loss: true,
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn record_cancelled_stream(
    output: &Arc<Mutex<OutputQueue>>,
    stream: SupervisedProcessStream,
) -> io::Result<()> {
    let name = match stream {
        SupervisedProcessStream::Stdout => "stdout",
        SupervisedProcessStream::Stderr => "stderr",
    };
    output
        .lock()
        .map_err(output_lock_poisoned)?
        .push(PlatformOutput {
            stream,
            bytes: format!(
                "[nara-editor: {name} capture was cancelled before EOF; remaining bytes were not retained]\n"
            )
            .into_bytes(),
            loss: true,
        });
    Ok(())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn output_lock_poisoned<T>(_error: std::sync::PoisonError<T>) -> io::Error {
    io::Error::other("the supervised process output queue was poisoned")
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn process_owner_retired() -> io::Error {
    io::Error::new(
        io::ErrorKind::BrokenPipe,
        "the supervised process direct-child owner was already retired",
    )
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn unsupported_platform() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "supervised Unix processes are supported only on Linux x86-64",
    )
}
