//! Bounded direct-child qualification for the sample's two known headless entry operations.

use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

pub(super) fn run(mut command: Command, directory: &Path, label: &str) -> Output {
    const DEADLINE: Duration = Duration::from_secs(90);
    const MAX_OUTPUT: u64 = 512 * 1024;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    let stdout = directory.join(format!("child-{sequence}.stdout"));
    let stderr = directory.join(format!("child-{sequence}.stderr"));
    command.stdout(Stdio::from(File::create_new(&stdout).unwrap()));
    command.stderr(Stdio::from(File::create_new(&stderr).unwrap()));
    let mut child = command.spawn().expect("the headless consumer should start");
    let deadline = Instant::now() + DEADLINE;
    let mut failure = None;
    let status = loop {
        let poll = child.try_wait();
        if let Ok(Some(status)) = poll {
            break status;
        }
        let poll_error = poll.err();
        if poll_error.is_some()
            || Instant::now() >= deadline
            || [&stdout, &stderr]
                .iter()
                .any(|path| fs::metadata(path).is_ok_and(|meta| meta.len() > MAX_OUTPUT))
        {
            failure = Some(format!(
                "{label} failed polling ({poll_error:?}) or exceeded its {DEADLINE:?} external deadline or bounded output"
            ));
            let kill_error = child.kill().err();
            let reap_deadline = Instant::now() + Duration::from_secs(2);
            let status = loop {
                if let Some(status) = child
                    .try_wait()
                    .expect("the timed-out headless consumer should remain waitable")
                {
                    break status;
                }
                assert!(
                    Instant::now() < reap_deadline,
                    "{label} could not be reaped after termination: {kill_error:?}"
                );
                std::thread::sleep(Duration::from_millis(10));
            };
            break status;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if [&stdout, &stderr]
        .iter()
        .any(|path| fs::metadata(path).is_ok_and(|meta| meta.len() > MAX_OUTPUT))
    {
        failure.get_or_insert_with(|| format!("{label} exceeded bounded output before exit"));
    }
    let read = |path: &Path| {
        let mut bytes = Vec::new();
        File::open(path)
            .unwrap()
            .take(MAX_OUTPUT)
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
    };
    let output = Output {
        status,
        stdout: read(&stdout),
        stderr: read(&stderr),
    };
    assert!(
        failure.is_none(),
        "{failure:?}; status={}; stdout={}; stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
