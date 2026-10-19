use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::*;

#[test]
fn fifo_leaf_rejects_without_a_writer_in_both_trust_modes() {
    let root = TestRoot::new();
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        root.path().join("fifo"),
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    fs::write(root.path().join("regular"), b"still readable").unwrap();
    std::os::unix::fs::symlink("regular", root.path().join("symlink")).unwrap();

    for trust in ["trusted", "untrusted"] {
        for route in ["relative", "child"] {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "unix_file_open::fifo_probe_process",
                    "--nocapture",
                ])
                .env("NARA_FIFO_PROBE_ROOT", root.path())
                .env("NARA_FIFO_PROBE_TRUST", trust)
                .env("NARA_FIFO_PROBE_ROUTE", route)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if child.try_wait().unwrap().is_some() {
                    let output = child.wait_with_output().unwrap();
                    assert!(
                        output.status.success(),
                        "{trust}/{route}: {}{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr),
                    );
                    break;
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("{trust}/{route}: opening a FIFO waited for a writer");
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

// A separate process makes a regression in the blocking syscall recoverable by the parent.
#[test]
fn fifo_probe_process() {
    let Some(path) = std::env::var_os("NARA_FIFO_PROBE_ROOT") else {
        return;
    };
    let trust = match std::env::var("NARA_FIFO_PROBE_TRUST").unwrap().as_str() {
        "trusted" => TrustMode::TrustedLocal,
        "untrusted" => TrustMode::Untrusted,
        _ => unreachable!(),
    };
    let capability = DirectoryCapability::from_host_handle(
        host_directory(Path::new(&path)),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, trust),
    )
    .unwrap();
    let open = |name| match std::env::var("NARA_FIFO_PROBE_ROUTE").unwrap().as_str() {
        "relative" => capability.open_file(&relative(name)),
        "child" => capability.open_child_file(&component(name)),
        _ => unreachable!(),
    };
    assert!(matches!(open("fifo"), Err(FsError::NotRegularFile)));
    let file = open("regular").unwrap();
    let mut bytes = Vec::new();
    file.reader().unwrap().read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"still readable");
    assert!(
        open("symlink").is_err(),
        "nonblocking admission must preserve no-follow policy"
    );
}
