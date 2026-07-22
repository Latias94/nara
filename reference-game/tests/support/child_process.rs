use std::{
    io::{self, Read},
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, SyncSender, TryRecvError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const STREAM_READ_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct ChildOutputLimits {
    stdout_bytes: usize,
    stderr_bytes: usize,
}

impl ChildOutputLimits {
    #[must_use]
    pub const fn new(stdout_bytes: usize, stderr_bytes: usize) -> Self {
        Self {
            stdout_bytes,
            stderr_bytes,
        }
    }
}

#[derive(Debug)]
pub struct ChildOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
enum CapturedStream {
    Stdout,
    Stderr,
}

impl CapturedStream {
    const fn label(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Debug)]
enum StreamEvent {
    LimitExceeded {
        stream: CapturedStream,
        observed_bytes: u64,
        maximum_bytes: usize,
    },
}

#[derive(Debug)]
struct StreamCapture {
    stream: CapturedStream,
    bytes: Vec<u8>,
    observed_bytes: u64,
    exceeded: bool,
}

pub fn run_child_with_timeout(
    mut command: Command,
    timeout: Duration,
    limits: ChildOutputLimits,
    label: &str,
) -> ChildOutput {
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .expect("the child probe process should start");
    let stdout = child
        .stdout
        .take()
        .expect("the child stdout pipe should open");
    let stderr = child
        .stderr
        .take()
        .expect("the child stderr pipe should open");
    let (sender, receiver) = mpsc::sync_channel(2);
    let stdout_capture = spawn_stream_capture(
        CapturedStream::Stdout,
        stdout,
        limits.stdout_bytes,
        sender.clone(),
    );
    let stderr_capture =
        spawn_stream_capture(CapturedStream::Stderr, stderr, limits.stderr_bytes, sender);
    let deadline = Instant::now()
        .checked_add(timeout)
        .expect("the child probe timeout is bounded");
    let status = loop {
        match receiver.try_recv() {
            Ok(event) => {
                let status = terminate_and_reap(&mut child).unwrap_or_else(|error| {
                    panic!("{label} capture limit reaping failed: {error}")
                });
                let (stdout, stderr) = join_captures(stdout_capture, stderr_capture);
                report_limit_failure(label, event, status, &stdout, &stderr);
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(poll_error) => {
                let status = terminate_and_reap(&mut child)
                    .unwrap_or_else(|error| panic!("{label} poll reaping failed: {error}"));
                let (stdout, stderr) = join_captures(stdout_capture, stderr_capture);
                panic!(
                    "{label} polling failed: {poll_error}; reap={status}; stdout={}; stderr={}",
                    capture_summary(&stdout),
                    capture_summary(&stderr),
                );
            }
        }
        if Instant::now() >= deadline {
            let status = terminate_and_reap(&mut child)
                .unwrap_or_else(|error| panic!("{label} timeout reaping failed: {error}"));
            let (stdout, stderr) = join_captures(stdout_capture, stderr_capture);
            panic!(
                "{label} exceeded {timeout:?} ({status}); stdout={}; stderr={}",
                capture_summary(&stdout),
                capture_summary(&stderr),
            );
        }
        std::thread::park_timeout(Duration::from_millis(10));
    };
    let (stdout, stderr) = join_captures(stdout_capture, stderr_capture);
    if stdout.exceeded {
        report_limit_failure(
            label,
            StreamEvent::LimitExceeded {
                stream: stdout.stream,
                observed_bytes: stdout.observed_bytes,
                maximum_bytes: limits.stdout_bytes,
            },
            status,
            &stdout,
            &stderr,
        );
    }
    if stderr.exceeded {
        report_limit_failure(
            label,
            StreamEvent::LimitExceeded {
                stream: stderr.stream,
                observed_bytes: stderr.observed_bytes,
                maximum_bytes: limits.stderr_bytes,
            },
            status,
            &stdout,
            &stderr,
        );
    }
    ChildOutput {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    }
}

fn spawn_stream_capture<R>(
    stream: CapturedStream,
    reader: R,
    maximum_bytes: usize,
    sender: SyncSender<StreamEvent>,
) -> JoinHandle<io::Result<StreamCapture>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || capture_stream(stream, reader, maximum_bytes, sender))
}

fn capture_stream<R>(
    stream: CapturedStream,
    mut reader: R,
    maximum_bytes: usize,
    sender: SyncSender<StreamEvent>,
) -> io::Result<StreamCapture>
where
    R: Read,
{
    let mut bytes = Vec::with_capacity(maximum_bytes.min(STREAM_READ_BYTES));
    let mut observed_bytes = 0_u64;
    let mut exceeded = false;
    let mut buffer = [0_u8; STREAM_READ_BYTES];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        observed_bytes = observed_bytes.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        let available = maximum_bytes.saturating_sub(bytes.len());
        let retained = count.min(available);
        bytes.extend_from_slice(&buffer[..retained]);
        if !exceeded && retained != count {
            exceeded = true;
            let _ = sender.send(StreamEvent::LimitExceeded {
                stream,
                observed_bytes,
                maximum_bytes,
            });
        }
    }
    Ok(StreamCapture {
        stream,
        bytes,
        observed_bytes,
        exceeded,
    })
}

fn join_captures(
    stdout_capture: JoinHandle<io::Result<StreamCapture>>,
    stderr_capture: JoinHandle<io::Result<StreamCapture>>,
) -> (StreamCapture, StreamCapture) {
    (
        join_capture(stdout_capture, CapturedStream::Stdout),
        join_capture(stderr_capture, CapturedStream::Stderr),
    )
}

fn join_capture(
    capture: JoinHandle<io::Result<StreamCapture>>,
    stream: CapturedStream,
) -> StreamCapture {
    capture
        .join()
        .unwrap_or_else(|_| panic!("the child {} capture thread panicked", stream.label()))
        .unwrap_or_else(|error| panic!("the child {} pipe failed: {error}", stream.label()))
}

fn report_limit_failure(
    label: &str,
    event: StreamEvent,
    status: ExitStatus,
    stdout: &StreamCapture,
    stderr: &StreamCapture,
) -> ! {
    let StreamEvent::LimitExceeded {
        stream,
        observed_bytes,
        maximum_bytes,
    } = event;
    panic!(
        "{label} {} capture exceeded its {maximum_bytes}-byte limit ({observed_bytes} bytes); reap={status}; stdout={}; stderr={}",
        stream.label(),
        capture_summary(stdout),
        capture_summary(stderr),
    );
}

fn terminate_and_reap(child: &mut Child) -> Result<ExitStatus, String> {
    let kill_error = child.kill().err();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::park_timeout(Duration::from_millis(10));
            }
            Ok(None) => {
                return Err(match kill_error {
                    Some(kill_error) => {
                        format!("kill={kill_error}; child remained live past the reap deadline")
                    }
                    None => "child remained live past the reap deadline".to_owned(),
                });
            }
            Err(poll_error) => {
                return Err(match kill_error {
                    Some(kill_error) => format!("kill={kill_error}; poll={poll_error}"),
                    None => format!("poll={poll_error}"),
                });
            }
        }
    }
}

fn capture_summary(capture: &StreamCapture) -> String {
    let text = String::from_utf8_lossy(&capture.bytes);
    if capture.exceeded {
        format!(
            "{text}<captured {} of at least {} bytes>",
            capture.bytes.len(),
            capture.observed_bytes,
        )
    } else {
        text.into_owned()
    }
}
