//! Bounded IO and physical custody of one trusted or approved exact direct child.
//!
//! `run` and its cleanup belong in the existing Tasks IO worker. Cleanup can remain pending
//! indefinitely; this module provides neither descendant containment nor a hard Drop deadline.

use nara_tasks::TaskCancellationToken;
use std::{
    io::{self, Read, Write},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio},
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
mod windows;

use super::managed::ApprovedManagedLaunch;

const REQUEST_BYTES: usize = 64 * 1024;
const BOOTSTRAP_BYTES: usize = 64 * 1024 * 1024 + 20 + 64 * 8;
const STDOUT_PROBE_BYTES: usize = 64 * 1024 + 1;
const STDERR_PROBE_BYTES: usize = 16 * 1024 + 1;
const POLL_INTERVAL: Duration = Duration::from_millis(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProcessFailure {
    Spawn,
    Io,
    WorkerPanic,
    Cancelled,
    TimedOut,
    StdoutLimit,
    StderrLimit,
    NonzeroExit,
    Installation,
}

pub(super) struct ProcessRequest {
    command: super::EditorSourceToolCommand,
    input: Vec<u8>,
    #[cfg(test)]
    gates: ProcessTestGates,
}

impl ProcessRequest {
    pub(super) fn new(command: super::EditorSourceToolCommand, input: Vec<u8>) -> Self {
        Self {
            command,
            input,
            #[cfg(test)]
            gates: ProcessTestGates::default(),
        }
    }

    #[cfg(test)]
    pub(super) fn with_test_gates(mut self, gates: ProcessTestGates) -> Self {
        self.gates = gates;
        self
    }

    /// Returns only after the direct child is reaped and every acquired pipe worker is joined.
    ///
    /// Call only inside the existing Tasks IO closure, never from an owner-thread poll or Drop.
    pub(super) fn run(self, token: TaskCancellationToken) -> Result<Vec<u8>, ProcessFailure> {
        let started = Instant::now();
        let Self {
            command,
            input,
            #[cfg(test)]
            gates,
        } = self;
        // Admission normally enforces these separate budgets. The bootstrap prefix does not
        // consume the ordinary request's 64 KiB allowance.
        if input.len() > REQUEST_BYTES
            || command
                .approved
                .as_ref()
                .is_some_and(|approved| approved.prefix().len() > BOOTSTRAP_BYTES)
        {
            return Err(ProcessFailure::Io);
        }
        let timeout = command.timeout();
        let approved = command.approved.clone();
        #[cfg(test)]
        if let Some(gate) = gates.before_spawn {
            gate.wait();
        }
        if let Some(installed) = &command.installed {
            installed
                .verify()
                .map_err(|_| ProcessFailure::Installation)?;
        }
        if let Some(approved) = approved.as_ref() {
            approved.verify().map_err(|_| ProcessFailure::Spawn)?;
        }
        // Even a cancelled held acquisition must transfer its late child into custody.
        let child = if let Some(approved) = approved.as_ref() {
            #[cfg(windows)]
            {
                FiniteChild::Exact(
                    windows::spawn(
                        approved.image(),
                        command.arguments(),
                        command.working_directory(),
                        approved.environment(),
                    )
                    .map_err(|_| ProcessFailure::Spawn)?,
                )
            }
            #[cfg(not(windows))]
            {
                let _ = approved;
                // Approved execution has no development-command fallback on other platforms.
                return Err(ProcessFailure::Spawn);
            }
        } else {
            let mut child_command = Command::new(command.executable());
            child_command
                .args(command.arguments())
                .current_dir(command.working_directory())
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                child_command.creation_flags(CREATE_NO_WINDOW);
            }
            FiniteChild::Standard(child_command.spawn().map_err(|_| ProcessFailure::Spawn)?)
        };
        let mut custody = Custody::new(child, approved);
        let mut failure = stop_reason(&token, started, timeout);
        #[cfg(windows)]
        if failure.is_none()
            && let FiniteChild::Exact(child) = &mut custody.child
        {
            // Every post-create error (and unwind) is now covered by the same finite custody.
            let image = custody
                ._approved
                .as_ref()
                .expect("exact child authority")
                .image();
            if child.verify_and_resume(image).is_err() {
                failure = Some(ProcessFailure::Spawn);
            }
        }
        failure = failure.or_else(|| stop_reason(&token, started, timeout));
        if failure.is_some() {
            if let Err(error) = custody.stop() {
                failure.get_or_insert(error);
            }
            if let Err(error) = custody.poll_exit() {
                failure.get_or_insert(error);
            }
        }

        #[cfg(test)]
        let io_return_gate = gates.before_io_return;
        custody.stdout = match custody.child.take_stdout() {
            Some(pipe) => PipeWorker::spawn("nara-source-stdout", move || {
                let result = read_bounded::<STDOUT_PROBE_BYTES>(pipe, ProcessFailure::StdoutLimit);
                #[cfg(test)]
                if let Some(gate) = io_return_gate {
                    gate.wait();
                }
                result
            }),
            None => PipeWorker::absent(),
        };
        custody.stderr = match custody.child.take_stderr() {
            Some(pipe) => PipeWorker::spawn("nara-source-stderr", move || {
                read_bounded::<STDERR_PROBE_BYTES>(pipe, ProcessFailure::StderrLimit)
            }),
            None => PipeWorker::absent(),
        };
        let stdin_approved = command.approved.clone();
        custody.stdin = match custody.child.take_stdin() {
            Some(mut pipe) => PipeWorker::spawn("nara-source-stdin", move || {
                if let Some(approved) = stdin_approved.as_ref() {
                    pipe.write_all(approved.prefix())
                        .map_err(|_| ProcessFailure::Io)?;
                }
                // Dropping this sole writer sends EOF after both prefix and ordinary input.
                pipe.write_all(&input).map_err(|_| ProcessFailure::Io)
            }),
            None => PipeWorker::absent(),
        };

        loop {
            custody.stdout.poll();
            custody.stderr.poll();
            custody.stdin.poll();
            if let Some(error) = custody.io_failure() {
                if error == ProcessFailure::WorkerPanic {
                    failure = Some(error);
                } else {
                    failure.get_or_insert(error);
                }
            }
            if failure.is_none() {
                failure = stop_reason(&token, started, timeout);
            }
            if let Err(error) = custody.poll_exit() {
                failure.get_or_insert(error);
            }
            if failure.is_some()
                && let Err(error) = custody.stop()
            {
                failure.get_or_insert(error);
            }
            if let Some(status) = custody.exit_status
                && custody.io_joined()
            {
                if let Some(error) = failure {
                    return Err(error);
                }
                let stdout = custody.stdout.take_result()?;
                custody.stderr.take_result()?;
                custody.stdin.take_result()?;
                if !status.success() {
                    return Err(ProcessFailure::NonzeroExit);
                }
                return Ok(stdout.bytes[..stdout.len].to_vec());
            }
            thread::sleep(POLL_INTERVAL);
        }
    }
}

fn stop_reason(
    token: &TaskCancellationToken,
    started: Instant,
    timeout: Duration,
) -> Option<ProcessFailure> {
    if token.is_cancelled() {
        Some(ProcessFailure::Cancelled)
    } else if started.elapsed() >= timeout {
        Some(ProcessFailure::TimedOut)
    } else {
        None
    }
}

#[cfg(test)]
#[derive(Clone, Default)]
pub(super) struct ProcessTestGates {
    pub(super) before_spawn: Option<Arc<ProcessTestGate>>,
    pub(super) before_io_return: Option<Arc<ProcessTestGate>>,
}

#[cfg(test)]
#[derive(Default)]
pub(super) struct ProcessTestGate {
    pub(super) entered: AtomicBool,
    pub(super) release: AtomicBool,
}

#[cfg(test)]
impl ProcessTestGate {
    fn wait(&self) {
        self.entered.store(true, Ordering::Release);
        // Logical cancellation must not release a deliberately held physical obligation.
        while !self.release.load(Ordering::Acquire) {
            thread::sleep(POLL_INTERVAL);
        }
    }
}

struct Bytes<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

fn read_bounded<const N: usize>(
    mut pipe: impl Read,
    limit_failure: ProcessFailure,
) -> Result<Bytes<N>, ProcessFailure> {
    let mut output = Bytes {
        bytes: [0; N],
        len: 0,
    };
    while output.len < N {
        match pipe.read(&mut output.bytes[output.len..]) {
            Ok(0) => return Ok(output),
            Ok(count) => output.len += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(ProcessFailure::Io),
        }
    }
    // The last byte is a probe, never an accepted or silently truncated response.
    Err(limit_failure)
}

struct PipeWorker<T> {
    handle: Option<JoinHandle<Result<T, ProcessFailure>>>,
    result: Option<Result<T, ProcessFailure>>,
}

impl<T> PipeWorker<T> {
    fn absent() -> Self {
        Self {
            handle: None,
            result: Some(Err(ProcessFailure::Io)),
        }
    }

    fn spawn(name: &str, work: impl FnOnce() -> Result<T, ProcessFailure> + Send + 'static) -> Self
    where
        T: Send + 'static,
    {
        match thread::Builder::new().name(name.into()).spawn(work) {
            Ok(handle) => Self {
                handle: Some(handle),
                result: None,
            },
            Err(_) => Self::absent(),
        }
    }

    fn poll(&mut self) {
        if self
            .handle
            .as_ref()
            .is_some_and(|handle| handle.is_finished())
        {
            self.join();
        }
    }

    fn join(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.result = Some(handle.join().unwrap_or(Err(ProcessFailure::WorkerPanic)));
        }
    }

    fn failure(&self) -> Option<ProcessFailure> {
        self.result
            .as_ref()
            .and_then(|result| result.as_ref().err().copied())
    }

    fn take_result(&mut self) -> Result<T, ProcessFailure> {
        self.join();
        self.result.take().unwrap_or(Err(ProcessFailure::Io))
    }
}

impl<T> Drop for PipeWorker<T> {
    fn drop(&mut self) {
        self.join();
    }
}

enum FiniteChild {
    Standard(Child),
    #[cfg(windows)]
    Exact(windows::ExactChild),
}

impl FiniteChild {
    fn take_stdin(&mut self) -> Option<Pipe<ChildStdin>> {
        match self {
            Self::Standard(child) => child.stdin.take().map(Pipe::Standard),
            #[cfg(windows)]
            Self::Exact(child) => child.stdin.take().map(Pipe::Exact),
        }
    }

    fn take_stdout(&mut self) -> Option<Pipe<ChildStdout>> {
        match self {
            Self::Standard(child) => child.stdout.take().map(Pipe::Standard),
            #[cfg(windows)]
            Self::Exact(child) => child.stdout.take().map(Pipe::Exact),
        }
    }

    fn take_stderr(&mut self) -> Option<Pipe<ChildStderr>> {
        match self {
            Self::Standard(child) => child.stderr.take().map(Pipe::Standard),
            #[cfg(windows)]
            Self::Exact(child) => child.stderr.take().map(Pipe::Exact),
        }
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match self {
            Self::Standard(child) => child.try_wait(),
            #[cfg(windows)]
            Self::Exact(child) => child.try_wait(),
        }
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        match self {
            Self::Standard(child) => child.wait(),
            #[cfg(windows)]
            Self::Exact(child) => child.wait(),
        }
    }

    fn kill(&mut self) -> io::Result<()> {
        match self {
            Self::Standard(child) => child.kill(),
            #[cfg(windows)]
            Self::Exact(child) => child.kill(),
        }
    }
}

enum Pipe<T> {
    Standard(T),
    #[cfg(windows)]
    Exact(std::fs::File),
}

impl<T: Read> Read for Pipe<T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Standard(pipe) => pipe.read(bytes),
            #[cfg(windows)]
            Self::Exact(pipe) => pipe.read(bytes),
        }
    }
}

impl<T: Write> Write for Pipe<T> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match self {
            Self::Standard(pipe) => pipe.write(bytes),
            #[cfg(windows)]
            Self::Exact(pipe) => pipe.write(bytes),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Standard(pipe) => pipe.flush(),
            #[cfg(windows)]
            Self::Exact(pipe) => pipe.flush(),
        }
    }
}

struct Custody {
    child: FiniteChild,
    // Preserve the complete status, including signal exits whose code() is None.
    exit_status: Option<ExitStatus>,
    kill_sent: bool,
    stdout: PipeWorker<Bytes<STDOUT_PROBE_BYTES>>,
    stderr: PipeWorker<Bytes<STDERR_PROBE_BYTES>>,
    stdin: PipeWorker<()>,
    // Retained through physical return, including unwind cleanup and late held acquisition.
    _approved: Option<Arc<ApprovedManagedLaunch>>,
}

impl Custody {
    fn new(child: FiniteChild, approved: Option<Arc<ApprovedManagedLaunch>>) -> Self {
        Self {
            child,
            exit_status: None,
            kill_sent: false,
            stdout: PipeWorker::absent(),
            stderr: PipeWorker::absent(),
            stdin: PipeWorker::absent(),
            _approved: approved,
        }
    }

    fn poll_exit(&mut self) -> Result<(), ProcessFailure> {
        if self.exit_status.is_none() {
            self.exit_status = self.child.try_wait().map_err(|_| ProcessFailure::Io)?;
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), ProcessFailure> {
        if self.exit_status.is_none() && !self.kill_sent {
            self.child.kill().map_err(|_| ProcessFailure::Io)?;
            self.kill_sent = true;
        }
        Ok(())
    }

    fn io_joined(&self) -> bool {
        self.stdout.handle.is_none() && self.stderr.handle.is_none() && self.stdin.handle.is_none()
    }

    fn io_failure(&self) -> Option<ProcessFailure> {
        let failures = [
            self.stdout.failure(),
            self.stderr.failure(),
            self.stdin.failure(),
        ];
        if failures.contains(&Some(ProcessFailure::WorkerPanic)) {
            Some(ProcessFailure::WorkerPanic)
        } else {
            failures.into_iter().flatten().next()
        }
    }
}

impl Drop for Custody {
    fn drop(&mut self) {
        // This guard is created only by run, on the Tasks IO worker. Unwinding must not detach
        // the child or any pipe thread. Failed cleanup keeps that worker physically pending.
        while self.exit_status.is_none() {
            if self.stop().is_ok() {
                if let Ok(status) = self.child.wait() {
                    self.exit_status = Some(status);
                }
            } else {
                let _ = self.poll_exit();
            }
            if self.exit_status.is_none() {
                thread::sleep(POLL_INTERVAL);
            }
        }
        self.stdout.join();
        self.stderr.join();
        self.stdin.join();
    }
}
