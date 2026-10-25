//! Fixture-only trusted direct-child custody. No descendant containment or hard Drop bound.

use super::wire;
use nara_editor_panel_module_consumer::snap_local_position::{SnapInput, SnapPlan};
use std::{
    io::{self, Read, Write},
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const STDOUT_PROBE_BYTES: usize = wire::RESPONSE_BYTES + 1;
const STDERR_PROBE_BYTES: usize = wire::STDERR_BYTES + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Spawn,
    Io,
    WorkerPanic,
    Cancelled,
    TimedOut,
    StdoutLimit,
    StderrLimit,
    Protocol,
    NonzeroExit,
    CleanupIncomplete,
    Semantic(wire::Refusal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    Busy,
    InvalidPath,
    SequenceExhausted,
    ThreadSpawn,
    Poisoned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cleanup {
    NoChild,
    Reaped {
        exit_code: Option<i32>,
        io_joined: bool,
    },
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Completion {
    pub request: RequestId,
    pub result: Result<SnapPlan, Failure>,
    pub cleanup: Cleanup,
    pub elapsed: Duration,
    pub spawned_after: Option<Duration>,
    pub reaped_after: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct RequestId {
    token: Arc<u64>,
}
impl PartialEq for RequestId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.token, &other.token)
    }
}
impl Eq for RequestId {}

#[derive(Default)]
pub struct WorkerGate {
    pub entered: AtomicBool,
    pub release: AtomicBool,
}

#[derive(Default)]
struct Progress {
    cancel: AtomicBool,
    issue: AtomicU8,
    cleanup_incomplete: AtomicBool,
}

struct Job {
    request: RequestId,
    input: Option<SnapInput>,
    progress: Arc<Progress>,
    worker: Option<JoinHandle<Physical>>,
    completion: Option<Completion>,
}

pub struct Lane {
    executable: PathBuf,
    cwd: PathBuf,
    timeout: Duration,
    sequence: u64,
    job: Option<Job>,
    poisoned: bool,
    pub spawn_gate: Option<Arc<WorkerGate>>,
    pub io_return_gate: Option<Arc<WorkerGate>>,
}

impl Lane {
    pub fn new(executable: PathBuf, cwd: PathBuf, timeout: Duration) -> Result<Self, Admission> {
        if !executable.is_absolute() || !cwd.is_absolute() {
            return Err(Admission::InvalidPath);
        }
        Ok(Self {
            executable,
            cwd,
            timeout,
            sequence: 0,
            job: None,
            poisoned: false,
            spawn_gate: None,
            io_return_gate: None,
        })
    }

    pub fn start(&mut self, input: SnapInput, mode: &'static str) -> Result<RequestId, Admission> {
        if self.poisoned {
            return Err(Admission::Poisoned);
        }
        if self.job.is_some() {
            return Err(Admission::Busy);
        }
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(Admission::SequenceExhausted)?;
        let started = Instant::now();
        let deadline = started
            .checked_add(self.timeout)
            .ok_or(Admission::SequenceExhausted)?;
        let bytes = wire::request(sequence, input.grid(), input.position());
        let request = RequestId {
            token: Arc::new(sequence),
        };
        let progress = Arc::new(Progress::default());
        let shared = Arc::clone(&progress);
        let (executable, cwd, gate, io_return_gate) = (
            self.executable.clone(),
            self.cwd.clone(),
            self.spawn_gate.take(),
            self.io_return_gate.take(),
        );
        let worker = thread::Builder::new()
            .name("snap-child-supervisor".into())
            .spawn(move || {
                supervise(
                    executable,
                    cwd,
                    mode,
                    bytes,
                    sequence,
                    started,
                    deadline,
                    shared,
                    gate,
                    io_return_gate,
                )
            })
            .map_err(|_| Admission::ThreadSpawn)?;
        self.sequence = sequence;
        self.job = Some(Job {
            request: request.clone(),
            input: Some(input),
            progress,
            worker: Some(worker),
            completion: None,
        });
        Ok(request)
    }

    /// Revokes this exact result, even when the worker finished before the parent polled it.
    pub fn cancel(&mut self, request: &RequestId) -> bool {
        let Some(job) = &mut self.job else {
            return false;
        };
        if job.request != *request {
            return false;
        }
        job.progress.cancel.store(true, Ordering::Release);
        if let Some(result) = &mut job.completion
            && result.result.is_ok()
        {
            result.result = Err(Failure::Cancelled);
        }
        true
    }

    pub fn cleanup_incomplete(&self) -> bool {
        self.job
            .as_ref()
            .is_some_and(|job| job.progress.cleanup_incomplete.load(Ordering::Acquire))
    }

    pub fn poll(&mut self) -> bool {
        let Some(job) = &mut self.job else {
            return false;
        };
        if job
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            let worker = job
                .worker
                .take()
                .expect("a finished worker remains owned until joined");
            let physical = worker.join().unwrap_or(Physical {
                result: Err(Failure::WorkerPanic),
                cleanup: Cleanup::Unknown,
                elapsed: Duration::ZERO,
                spawned_after: None,
                reaped_after: None,
            });
            self.poisoned |= physical.cleanup == Cleanup::Unknown;
            let result = physical.result.and_then(|position| {
                if job.progress.cancel.load(Ordering::Acquire) {
                    Err(Failure::Cancelled)
                } else {
                    job.input
                        .take()
                        .ok_or(Failure::WorkerPanic)?
                        .proposal(position)
                        .map_err(|_| Failure::Protocol)
                }
            });
            job.completion = Some(Completion {
                request: job.request.clone(),
                result,
                cleanup: physical.cleanup,
                elapsed: physical.elapsed,
                spawned_after: physical.spawned_after,
                reaped_after: physical.reaped_after,
            });
        }
        job.completion.is_some()
    }

    pub fn take(&mut self, request: &RequestId) -> Option<Completion> {
        self.poll();
        if !self
            .job
            .as_ref()
            .is_some_and(|job| job.request == *request && job.completion.is_some())
        {
            return None;
        }
        self.job.take()?.completion
    }
}

impl Drop for Lane {
    fn drop(&mut self) {
        if let Some(job) = &mut self.job {
            job.progress.cancel.store(true, Ordering::Release);
            if let Some(worker) = job.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

struct Bytes<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

fn read_bounded<const N: usize>(
    mut pipe: impl Read,
    progress: Arc<Progress>,
    issue: u8,
) -> Result<Bytes<N>, Failure> {
    let mut output = Bytes {
        bytes: [0; N],
        len: 0,
    };
    loop {
        match pipe.read(&mut output.bytes[output.len..]) {
            Ok(0) => return Ok(output),
            Ok(count) => output.len += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                progress.issue.store(3, Ordering::Release);
                return Err(Failure::Io);
            }
        }
        if output.len == N {
            progress.issue.store(issue, Ordering::Release);
            return Err(if issue == 1 {
                Failure::StdoutLimit
            } else {
                Failure::StderrLimit
            });
        }
    }
}

struct Custody {
    child: Child,
    reaped: bool,
    stdout: Option<JoinHandle<Result<Bytes<STDOUT_PROBE_BYTES>, Failure>>>,
    stderr: Option<JoinHandle<Result<Bytes<STDERR_PROBE_BYTES>, Failure>>>,
    stdin: Option<JoinHandle<Result<(), Failure>>>,
}

impl Custody {
    fn returned(&self) -> bool {
        self.stdout.as_ref().is_none_or(|h| h.is_finished())
            && self.stderr.as_ref().is_none_or(|h| h.is_finished())
            && self.stdin.as_ref().is_none_or(|h| h.is_finished())
    }
    fn join_io(&mut self) -> Result<Bytes<STDOUT_PROBE_BYTES>, Failure> {
        let stdout = self
            .stdout
            .take()
            .map(|h| h.join().unwrap_or(Err(Failure::WorkerPanic)))
            .unwrap_or(Err(Failure::Io));
        let stderr = self
            .stderr
            .take()
            .map(|h| h.join().unwrap_or(Err(Failure::WorkerPanic)))
            .unwrap_or(Err(Failure::Io));
        let stdin = self
            .stdin
            .take()
            .map(|h| h.join().unwrap_or(Err(Failure::WorkerPanic)))
            .unwrap_or(Err(Failure::Io));
        // Every join runs before propagating any one stream's failure.
        if matches!(stdout, Err(Failure::WorkerPanic))
            || matches!(stderr, Err(Failure::WorkerPanic))
            || matches!(stdin, Err(Failure::WorkerPanic))
        {
            return Err(Failure::WorkerPanic);
        }
        stderr?;
        stdin?;
        stdout
    }
}

impl Drop for Custody {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = self.join_io();
    }
}

struct Physical {
    result: Result<[f64; 2], Failure>,
    cleanup: Cleanup,
    elapsed: Duration,
    spawned_after: Option<Duration>,
    reaped_after: Option<Duration>,
}

#[allow(clippy::too_many_arguments)]
fn supervise(
    executable: PathBuf,
    cwd: PathBuf,
    mode: &str,
    bytes: [u8; wire::REQUEST_BYTES],
    sequence: u64,
    started: Instant,
    deadline: Instant,
    progress: Arc<Progress>,
    gate: Option<Arc<WorkerGate>>,
    io_return_gate: Option<Arc<WorkerGate>>,
) -> Physical {
    let mut command = Command::new(executable);
    command
        .arg(mode)
        .current_dir(cwd)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
        for name in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            return Physical {
                result: Err(Failure::Spawn),
                cleanup: Cleanup::NoChild,
                elapsed: started.elapsed(),
                spawned_after: None,
                reaped_after: None,
            };
        }
    };
    let spawned_after = started.elapsed();
    let mut custody = Custody {
        child,
        reaped: false,
        stdout: None,
        stderr: None,
        stdin: None,
    };
    if let Some(gate) = gate {
        gate.entered.store(true, Ordering::Release);
        while !gate.release.load(Ordering::Acquire)
            && !progress.cancel.load(Ordering::Acquire)
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(1));
        }
    }
    let mut failure = None;
    if let Some(pipe) = custody.child.stdout.take() {
        let shared = Arc::clone(&progress);
        match thread::Builder::new()
            .name("snap-child-stdout".into())
            .spawn(move || {
                let result = read_bounded::<STDOUT_PROBE_BYTES>(pipe, Arc::clone(&shared), 1);
                if let Some(gate) = io_return_gate {
                    gate.entered.store(true, Ordering::Release);
                    while !gate.release.load(Ordering::Acquire)
                        && !shared.cancel.load(Ordering::Acquire)
                    {
                        thread::sleep(Duration::from_millis(1));
                    }
                }
                result
            }) {
            Ok(handle) => custody.stdout = Some(handle),
            Err(_) => failure = Some(Failure::Io),
        }
    } else {
        failure = Some(Failure::Io);
    }
    if let Some(pipe) = custody.child.stderr.take() {
        let shared = Arc::clone(&progress);
        match thread::Builder::new()
            .name("snap-child-stderr".into())
            .spawn(move || read_bounded::<STDERR_PROBE_BYTES>(pipe, shared, 2))
        {
            Ok(handle) => custody.stderr = Some(handle),
            Err(_) => failure = Some(Failure::Io),
        }
    } else {
        failure = Some(Failure::Io);
    }
    if let Some(mut pipe) = custody.child.stdin.take() {
        let inject_panic = mode == "panic-stdin";
        match thread::Builder::new()
            .name("snap-child-stdin".into())
            .spawn(move || {
                assert!(!inject_panic, "controlled stdin worker panic");
                pipe.write_all(&bytes).map_err(|_| Failure::Io)
            }) {
            Ok(handle) => custody.stdin = Some(handle),
            Err(_) => failure = Some(Failure::Io),
        }
    } else {
        failure = Some(Failure::Io);
    }
    let mut exit: Option<ExitStatus> = None;
    let mut exited_at = None;
    let mut killed_at = None;
    loop {
        let now = Instant::now();
        let issue = match progress.issue.load(Ordering::Acquire) {
            1 => Some(Failure::StdoutLimit),
            2 => Some(Failure::StderrLimit),
            3 => Some(Failure::Io),
            _ => None,
        };
        if failure.is_none() {
            failure = issue;
        }
        if failure.is_none() && progress.cancel.load(Ordering::Acquire) {
            failure = Some(Failure::Cancelled);
        }
        if failure.is_none() && now >= deadline {
            failure = Some(Failure::TimedOut);
        }
        if failure.is_some() && exit.is_none() && killed_at.is_none() {
            let _ = custody.child.kill();
            killed_at = Some(now);
        }
        if exit.is_none() {
            match custody.child.try_wait() {
                Ok(Some(status)) => {
                    exit = Some(status);
                    exited_at = Some(now);
                    custody.reaped = true;
                }
                Err(_) => {
                    progress.cleanup_incomplete.store(true, Ordering::Release);
                    failure.get_or_insert(Failure::CleanupIncomplete);
                }
                _ => {}
            }
        }
        if let Some(status) = exit {
            if custody.returned() {
                let output = custody.join_io();
                let result = match (failure, output) {
                    (_, Err(Failure::WorkerPanic)) => Err(Failure::WorkerPanic),
                    (Some(error), _) => Err(error),
                    (None, Err(error)) => Err(error),
                    (None, Ok(_)) if !status.success() => Err(Failure::NonzeroExit),
                    (None, Ok(out)) => wire::read_response(&out.bytes[..out.len], sequence)
                        .map_err(|_| Failure::Protocol)
                        .and_then(|value| value.map_err(Failure::Semantic)),
                };
                drop(custody);
                return Physical {
                    result,
                    cleanup: Cleanup::Reaped {
                        exit_code: status.code(),
                        io_joined: true,
                    },
                    elapsed: started.elapsed(),
                    spawned_after: Some(spawned_after),
                    reaped_after: exited_at.map(|at| at.duration_since(started)),
                };
            }
            if exited_at.is_some_and(|at| now.duration_since(at) >= Duration::from_millis(500)) {
                progress.cleanup_incomplete.store(true, Ordering::Release);
                failure.get_or_insert(Failure::CleanupIncomplete);
            }
        } else if killed_at.is_some_and(|at| now.duration_since(at) >= Duration::from_secs(1)) {
            progress.cleanup_incomplete.store(true, Ordering::Release);
        }
        thread::sleep(Duration::from_millis(1));
    }
}
