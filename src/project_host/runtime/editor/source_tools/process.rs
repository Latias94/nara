//! Bounded IO and physical custody of one trusted direct child.
//!
//! `run` and its cleanup belong in the existing Tasks IO worker. Cleanup can remain pending
//! indefinitely; this module provides neither descendant containment nor a hard Drop deadline.

use nara_tasks::TaskCancellationToken;
use std::{
    io::{self, Read, Write},
    process::{Child, Command, ExitStatus, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const REQUEST_BYTES: usize = 64 * 1024;
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
        // Admission normally enforces this bound. Refuse oversized input before acquiring a child.
        if input.len() > REQUEST_BYTES {
            return Err(ProcessFailure::Io);
        }
        let timeout = command.timeout();
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
        #[cfg(test)]
        if let Some(gate) = gates.before_spawn {
            gate.wait();
        }
        // Even a cancelled held acquisition must transfer its late child into custody.
        let child = child_command.spawn().map_err(|_| ProcessFailure::Spawn)?;
        let mut custody = Custody::new(child);
        let mut failure = stop_reason(&token, started, timeout);
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
        custody.stdout = match custody.child.stdout.take() {
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
        custody.stderr = match custody.child.stderr.take() {
            Some(pipe) => PipeWorker::spawn("nara-source-stderr", move || {
                read_bounded::<STDERR_PROBE_BYTES>(pipe, ProcessFailure::StderrLimit)
            }),
            None => PipeWorker::absent(),
        };
        custody.stdin = match custody.child.stdin.take() {
            Some(mut pipe) => PipeWorker::spawn("nara-source-stdin", move || {
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

struct Custody {
    child: Child,
    // Preserve the complete status, including signal exits whose code() is None.
    exit_status: Option<ExitStatus>,
    kill_sent: bool,
    stdout: PipeWorker<Bytes<STDOUT_PROBE_BYTES>>,
    stderr: PipeWorker<Bytes<STDERR_PROBE_BYTES>>,
    stdin: PipeWorker<()>,
}

impl Custody {
    fn new(child: Child) -> Self {
        Self {
            child,
            exit_status: None,
            kill_sent: false,
            stdout: PipeWorker::absent(),
            stderr: PipeWorker::absent(),
            stdin: PipeWorker::absent(),
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
