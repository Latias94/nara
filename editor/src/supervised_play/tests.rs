use std::{
    env,
    num::NonZeroUsize,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use super::{
    ProcessOutputStream,
    log::{ProcessLogLimits, ProcessLogStream},
    process::{
        SupervisedProcessCleanupReceipt, SupervisedProcessCommand, SupervisedProcessOutput,
        SupervisedProcessOwner, SupervisedProcessState,
    },
};

#[test]
fn helper_process() {
    let Ok(mode) = env::var("NARA_SUPERVISED_PROCESS_TEST_MODE") else {
        return;
    };
    match mode.as_str() {
        "exit-success" => {
            println!("helper stdout");
            eprintln!("helper stderr");
        }
        "exit-failure" => std::process::exit(23),
        "wait" => thread::sleep(Duration::from_secs(30)),
        "flood" => {
            for _ in 0..4_096 {
                println!("0123456789abcdef");
                eprintln!("fedcba9876543210");
            }
        }
        "spawn-descendant-and-exit" => {
            let executable = env::current_exe().expect("the test executable is available");
            let descendant = Command::new(executable)
                .args([
                    "--exact",
                    "supervised_play::tests::helper_process",
                    "--nocapture",
                ])
                .env("NARA_SUPERVISED_PROCESS_TEST_MODE", "wait")
                .spawn()
                .expect("the descendant helper starts");
            println!("spawned descendant {}", descendant.id());
            drop(descendant);
        }
        "bootstrap-roundtrip" => child_bootstrap_roundtrip(),
        "sentinel-closed" => assert!(
            !child_sentinel_is_open(),
            "the sentinel process authority crossed the spawn boundary"
        ),
        _ => panic!("unsupported supervised process helper mode"),
    }
}

pub(super) fn helper_command(mode: &str) -> SupervisedProcessCommand {
    let executable = env::current_exe().expect("the test executable is available");
    let working_directory = executable
        .parent()
        .expect("the test executable has a parent")
        .to_path_buf();
    SupervisedProcessCommand::new(executable, working_directory)
        .arguments([
            "--exact",
            "supervised_play::tests::helper_process",
            "--nocapture",
        ])
        .environment("NARA_SUPERVISED_PROCESS_TEST_MODE", mode)
}

fn log_limits() -> ProcessLogLimits {
    ProcessLogLimits::new(
        NonZeroUsize::new(64).expect("the record limit is non-zero"),
        NonZeroUsize::new(4 * 1024).expect("the byte limit is non-zero"),
    )
}

fn drive_to_complete(owner: &mut SupervisedProcessOwner) -> SupervisedProcessCleanupReceipt {
    let deadline = Instant::now() + Duration::from_secs(10);
    while owner.state() != SupervisedProcessState::Complete {
        owner.advance(Instant::now()).expect("process advances");
        assert!(Instant::now() < deadline, "process cleanup timed out");
        thread::sleep(Duration::from_millis(5));
    }
    owner
        .cleanup_receipt()
        .expect("cleanup receipt is published")
}

#[test]
fn supervised_process_captures_both_streams_and_publishes_cleanup_receipt() {
    let mut owner = SupervisedProcessOwner::spawn(&helper_command("exit-success"), log_limits())
        .expect("helper process spawns");
    let receipt = drive_to_complete(&mut owner);

    let text = owner
        .logs()
        .iter()
        .map(|entry| entry.text().into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("helper stdout"));
    assert!(text.contains("helper stderr"));
    assert!(receipt.exit().success());
    assert!(receipt.containment_empty());
    assert!(receipt.readers_stopped());
}

#[test]
fn supervised_process_raw_output_preserves_stream_and_loss_facts() {
    let mut owner = SupervisedProcessOwner::spawn(&helper_command("exit-success"), log_limits())
        .expect("helper process spawns");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut outputs: Vec<SupervisedProcessOutput> = Vec::new();

    while owner.state() != SupervisedProcessState::Complete {
        let (_, mut batch) = owner
            .advance_with_output(Instant::now())
            .expect("process advances with raw output");
        outputs.append(&mut batch);
        assert!(Instant::now() < deadline, "process cleanup timed out");
        thread::sleep(Duration::from_millis(5));
    }

    assert!(outputs.iter().any(|output| {
        output.stream() == ProcessOutputStream::Stdout
            && output
                .bytes()
                .windows(b"helper stdout".len())
                .any(|window| window == b"helper stdout")
    }));
    assert!(outputs.iter().any(|output| {
        output.stream() == ProcessOutputStream::Stderr
            && output
                .bytes()
                .windows(b"helper stderr".len())
                .any(|window| window == b"helper stderr")
    }));
    assert!(outputs.iter().all(|output| !output.loss()));
}

#[test]
fn supervised_process_force_kill_is_explicit_and_reaped() {
    let mut owner = SupervisedProcessOwner::spawn(&helper_command("wait"), log_limits())
        .expect("helper process spawns");
    let start = Instant::now();
    owner.request_terminate(start).expect("terminate begins");
    let receipt = drive_to_complete(&mut owner);
    assert!(receipt.containment_empty());
    assert!(receipt.readers_stopped());
}

#[test]
fn supervised_process_preserves_non_zero_exit_status() {
    let mut owner = SupervisedProcessOwner::spawn(&helper_command("exit-failure"), log_limits())
        .expect("helper process spawns");
    let receipt = drive_to_complete(&mut owner);

    assert!(!receipt.exit().success());
    assert_eq!(receipt.exit().code(), Some(23));
    assert!(receipt.containment_empty());
    assert!(receipt.readers_stopped());
}

#[test]
fn supervised_process_flood_remains_bounded_and_explicit() {
    let mut owner = SupervisedProcessOwner::spawn(&helper_command("flood"), log_limits())
        .expect("helper process spawns");
    let receipt = drive_to_complete(&mut owner);
    let stats = owner.logs().stats();

    assert!(receipt.exit().success());
    assert!(stats.retained_records() <= 64);
    assert!(stats.retained_bytes() <= 4 * 1024);
    assert!(
        stats.dropped_records() > 0
            || stats.truncated_records() > 0
            || owner
                .logs()
                .iter()
                .any(|entry| entry.text().contains("nara-editor: dropped")),
        "overflow must be represented by the bounded projection or platform marker"
    );
    assert!(
        owner
            .logs()
            .iter()
            .any(|entry| entry.stream() == ProcessLogStream::Stdout)
    );
    assert!(
        owner
            .logs()
            .iter()
            .any(|entry| entry.stream() == ProcessLogStream::Stderr)
    );
}

#[test]
fn supervised_process_reaps_a_descendant_that_outlives_the_direct_child() {
    let mut owner =
        SupervisedProcessOwner::spawn(&helper_command("spawn-descendant-and-exit"), log_limits())
            .expect("helper process spawns");
    let receipt = drive_to_complete(&mut owner);
    let text = owner
        .logs()
        .iter()
        .map(|entry| entry.text().into_owned())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("spawned descendant"));
    assert!(receipt.exit().success());
    assert!(receipt.containment_empty());
    assert!(receipt.readers_stopped());
}

#[test]
fn bootstrap_endpoint_is_spawn_bound_and_required() {
    let mut owner = SupervisedProcessOwner::spawn(
        &helper_command("bootstrap-roundtrip").with_bootstrap(),
        log_limits(),
    )
    .expect("bootstrap helper process spawns");
    owner
        .bootstrap_mut()
        .expect("the requested bootstrap endpoint exists")
        .write_all(b"PING")
        .expect("the bootstrap request is written");

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut reply = [0_u8; 4];
    let mut received = 0;
    while received < reply.len() {
        received += owner
            .bootstrap_mut()
            .expect("the bootstrap endpoint remains available before cleanup")
            .try_read(&mut reply[received..])
            .expect("the bootstrap reply is read");
        assert!(Instant::now() < deadline, "bootstrap reply timed out");
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(&reply, b"PONG");
    assert!(drive_to_complete(&mut owner).exit().success());

    let mut without_endpoint =
        SupervisedProcessOwner::spawn(&helper_command("bootstrap-roundtrip"), log_limits())
            .expect("the non-bootstrap helper process spawns");
    assert!(without_endpoint.bootstrap_mut().is_none());
    assert!(!drive_to_complete(&mut without_endpoint).exit().success());
}

#[test]
fn sentinel_process_authority_is_not_inherited() {
    let (_sentinel, sentinel_value) = inheritable_sentinel();
    let command = helper_command("sentinel-closed")
        .environment("NARA_SUPERVISED_PROCESS_SENTINEL", sentinel_value);
    let mut owner =
        SupervisedProcessOwner::spawn(&command, log_limits()).expect("helper process spawns");

    assert!(drive_to_complete(&mut owner).exit().success());
}

#[test]
fn command_contract_rejects_relative_process_authority() {
    let command = SupervisedProcessCommand::new(PathBuf::from("helper"), PathBuf::from("project"));
    assert!(SupervisedProcessOwner::spawn(&command, log_limits()).is_err());
}

#[cfg(windows)]
fn child_bootstrap_roundtrip() {
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        Storage::FileSystem::{ReadFile, WriteFile},
        System::Console::{GetStdHandle, STD_INPUT_HANDLE},
    };

    // SAFETY: the process substrate binds one live duplex bootstrap pipe to standard input.
    let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    assert!(!handle.is_null() && handle != INVALID_HANDLE_VALUE);
    let mut request = [0_u8; 4];
    let mut read = 0_u32;
    // SAFETY: `request` is writable for its exact length and the inherited handle remains live.
    let read_ok = unsafe {
        ReadFile(
            handle,
            request.as_mut_ptr(),
            request.len() as u32,
            &mut read,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(read_ok, 0);
    assert_eq!(read as usize, request.len());
    assert_eq!(&request, b"PING");

    let mut written = 0_u32;
    // SAFETY: the response is readable for its exact length and the duplex handle remains live.
    let write_ok = unsafe {
        WriteFile(
            handle,
            b"PONG".as_ptr(),
            4,
            &mut written,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(write_ok, 0);
    assert_eq!(written, 4);
}

#[cfg(all(unix, target_os = "linux", target_arch = "x86_64"))]
fn child_bootstrap_roundtrip() {
    use std::{
        fs::File,
        io::{Read, Write},
        os::fd::FromRawFd as _,
    };

    // SAFETY: fd 0 is the child-only duplex bootstrap endpoint for this helper process.
    let mut bootstrap = unsafe { File::from_raw_fd(0) };
    let mut request = [0_u8; 4];
    bootstrap
        .read_exact(&mut request)
        .expect("the bootstrap request is read");
    assert_eq!(&request, b"PING");
    bootstrap
        .write_all(b"PONG")
        .expect("the bootstrap response is written");
    bootstrap
        .flush()
        .expect("the bootstrap response is flushed");
}

#[cfg(windows)]
fn inheritable_sentinel() -> (std::fs::File, String) {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

    let owner = std::fs::File::open(env::current_exe().expect("the test executable is available"))
        .expect("the sentinel file is opened");
    // SAFETY: the file handle is live and only its inherit flag is changed for this test.
    let changed = unsafe {
        SetHandleInformation(
            owner.as_raw_handle() as _,
            HANDLE_FLAG_INHERIT,
            HANDLE_FLAG_INHERIT,
        )
    };
    assert_ne!(changed, 0, "the sentinel file handle is marked inheritable");
    let raw = owner.as_raw_handle() as usize;
    (owner, raw.to_string())
}

#[cfg(windows)]
fn child_sentinel_is_open() -> bool {
    use windows_sys::Win32::Storage::FileSystem::{FILE_TYPE_DISK, GetFileType};

    let raw = env::var("NARA_SUPERVISED_PROCESS_SENTINEL")
        .expect("the sentinel value is present")
        .parse::<usize>()
        .expect("the sentinel value is numeric") as *mut core::ffi::c_void;
    // A raw handle number can collide across processes. The sentinel is a disk file, while the
    // exact inherited allowlist contains only standard pipes; checking the object type prevents a
    // coincidental pipe handle with the same numeric value from becoming a false positive.
    // SAFETY: probing the candidate handle is the purpose of this canary; invalid handles return
    // FILE_TYPE_UNKNOWN and do not dereference process memory.
    unsafe { GetFileType(raw) == FILE_TYPE_DISK }
}

#[cfg(all(unix, target_os = "linux", target_arch = "x86_64"))]
fn inheritable_sentinel() -> (std::fs::File, String) {
    use std::os::fd::AsRawFd as _;

    let file = std::fs::File::open(env::current_exe().expect("the test executable is available"))
        .expect("the sentinel file is opened");
    rustix::io::fcntl_setfd(&file, rustix::io::FdFlags::empty())
        .expect("the sentinel fd is marked inheritable");
    let raw = file.as_raw_fd();
    (file, raw.to_string())
}

#[cfg(all(unix, target_os = "linux", target_arch = "x86_64"))]
fn child_sentinel_is_open() -> bool {
    use std::os::fd::BorrowedFd;

    let raw = env::var("NARA_SUPERVISED_PROCESS_SENTINEL")
        .expect("the sentinel value is present")
        .parse::<i32>()
        .expect("the sentinel value is numeric");
    // SAFETY: the value is only borrowed for one fcntl probe and is never closed by this helper.
    let borrowed = unsafe { BorrowedFd::borrow_raw(raw) };
    rustix::io::fcntl_getfd(borrowed).is_ok()
}
