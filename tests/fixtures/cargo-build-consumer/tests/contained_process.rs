#![cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
#![forbid(unsafe_code)]

use std::{
    env, fs,
    io::{Read, Write},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use engine::project_tooling::{
    host_fs,
    process::{
        ExactExecutableLaunchAuthority, ProcessLogLimits, ProcessOutputStream,
        SupervisedProcessCommand, SupervisedProcessOwner, SupervisedProcessState,
        SupervisedProductEnvironmentPolicy, SupervisedProductProcessCommand,
    },
};

const TEMP_PREFIX: &str = "nara-public-contained-process-";
const MODE_FILE: &str = "fixture-mode";
const FLUSHED_FILE: &str = "output-flushed";
const OBSERVATION_FILE: &str = "product-observation";
const MAX_MARKER_BYTES: usize = 128;
const TOOL_MODE_ENV: &str = "NARA_PUBLIC_CONTAINED_PROCESS_MODE";
const HELPER_ARGUMENTS: [&str; 4] = ["--exact", "helper_process", "--nocapture", "--quiet"];
const PROTOCOL_START: &[u8] = b"nara-public-protocol-start\n";
const PRODUCT_STDOUT: &[u8] = b"public exact product: sealed environment, stdin EOF\n";
const PRODUCT_STDERR: &[u8] = b"public exact product stderr\n";
const WAIT_LIMIT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(5);

#[test]
fn bounded_tool_stdout_survives_delayed_polling_and_diagnostic_overflow() {
    let directory = TestDirectory::new("tool");
    let expected = protocol_bytes();
    let stdout_limit = expected.len() + 4096;
    let limits = ProcessLogLimits::try_from_counts(2, 64).unwrap();
    let command = SupervisedProcessCommand::new(test_executable(), directory.root.clone())
        .arguments(HELPER_ARGUMENTS)
        .environment(TOOL_MODE_ENV, "tool")
        .bounded_stdout(NonZeroUsize::new(stdout_limit).unwrap());
    let mut process = ProcessFixture::new(
        SupervisedProcessOwner::spawn(&command, limits).expect("the public tool helper must spawn"),
        directory,
    );
    let deadline = Instant::now() + WAIT_LIMIT;
    while !process.directory().root.join(FLUSHED_FILE).is_file() {
        assert!(Instant::now() < deadline, "the tool helper did not flush");
        thread::sleep(POLL_INTERVAL);
    }

    let mut stdout = Vec::new();
    let mut stderr_loss = false;
    drive_with_output(process.owner_mut(), |stream, bytes, loss| match stream {
        ProcessOutputStream::Stdout => {
            assert!(!loss, "admitted stdout must survive diagnostic eviction");
            assert!(stdout.len() + bytes.len() <= stdout_limit);
            stdout.extend_from_slice(bytes);
        }
        ProcessOutputStream::Stderr => stderr_loss |= loss,
    });

    let protocol_offset = stdout
        .windows(PROTOCOL_START.len())
        .position(|bytes| bytes == PROTOCOL_START)
        .expect("raw stdout must contain the protocol after the libtest preamble");
    assert_eq!(&stdout[protocol_offset..], expected.as_slice());
    assert!(stderr_loss, "diagnostic queue loss must be explicit");
    let stats = process.owner().logs().stats();
    assert!(stats.retained_records() <= limits.record_limit());
    assert!(stats.retained_bytes() <= limits.byte_limit());
    assert_eq!(stats.retained_records(), process.owner().logs().len());
    assert_eq!(
        stats.retained_bytes(),
        process
            .owner()
            .logs()
            .iter()
            .map(|entry| entry.bytes().len())
            .sum::<usize>(),
    );
    assert!(stats.dropped_bytes() > 0);
    assert!(stats.observed_bytes() > stats.retained_bytes() as u64);
    assert_successful_cleanup(process.owner());
}

#[test]
fn exact_product_consumes_live_capabilities_with_sealed_environment_and_eof_stdin() {
    let directory = TestDirectory::new("sealed");
    let executable = test_executable();
    let file = host_fs::readable_file(&executable).expect("the current executable is readable");
    let executable_directory = host_fs::readable_directory(executable.parent().unwrap())
        .expect("the executable's immediate directory is readable");
    let authority = ExactExecutableLaunchAuthority::from_verified_handles(
        file.duplicate_restrictive_executable_host_handle().unwrap(),
        executable_directory
            .duplicate_restrictive_host_handle()
            .unwrap(),
    )
    .expect("the live executable and its directory must yield exact launch authority");
    drop(file);
    drop(executable_directory);
    let command = SupervisedProductProcessCommand::new(
        SupervisedProductEnvironmentPolicy::Sealed,
        directory.root.clone(),
    )
    .arguments(HELPER_ARGUMENTS);
    let mut process = ProcessFixture::new(
        SupervisedProcessOwner::spawn_exact(
            authority,
            &command,
            ProcessLogLimits::try_from_counts(16, 4096).unwrap(),
        )
        .expect("the exact public product helper must spawn"),
        directory,
    );
    let child_pid = process.owner().process_id();
    assert_ne!(child_pid, 0);
    assert_ne!(child_pid, std::process::id());
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    drive_with_output(process.owner_mut(), |stream, bytes, loss| {
        assert!(!loss, "the finite product output must be complete");
        let target = match stream {
            ProcessOutputStream::Stdout => &mut stdout,
            ProcessOutputStream::Stderr => &mut stderr,
        };
        assert!(target.len() + bytes.len() <= 4096);
        target.extend_from_slice(bytes);
    });

    assert!(stdout.ends_with(PRODUCT_STDOUT));
    assert_eq!(stderr, PRODUCT_STDERR);
    assert_eq!(
        fs::read_to_string(process.directory().root.join(OBSERVATION_FILE)).unwrap(),
        format!("pid={child_pid}\nenvironment=sealed\nstdin=eof\n"),
    );
    assert_eq!(process.owner().process_id(), child_pid);
    assert_successful_cleanup(process.owner());
}

#[test]
fn helper_process() {
    let root = env::current_dir().expect("the test has a working directory");
    let Some(name) = root.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let Some(suffix) = name.strip_prefix(TEMP_PREFIX) else {
        return;
    };
    let Some((pid, nonce)) = suffix.split_once('-') else {
        return;
    };
    if pid.parse::<u32>().is_err() || nonce.parse::<u128>().is_err() {
        return;
    }
    let Some(marker) = read_marker(&root) else {
        return;
    };
    let Some(mode) = marker.strip_prefix(&format!("{name}\n")) else {
        return;
    };
    match mode {
        "tool\n" => {
            assert_eq!(env::var(TOOL_MODE_ENV).as_deref(), Ok("tool"));
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(&protocol_bytes()).unwrap();
            stdout.flush().unwrap();
            let mut stderr = std::io::stderr().lock();
            for _ in 0..256 {
                stderr.write_all(&[b'D'; 4096]).unwrap();
            }
            stderr.flush().unwrap();
            fs::write(root.join(FLUSHED_FILE), b"flushed").unwrap();
        }
        "sealed\n" => {
            assert!(
                env::vars_os().next().is_none(),
                "the sealed product must have an empty environment",
            );
            assert_eq!(std::io::stdin().read(&mut [0_u8; 1]).unwrap(), 0);
            fs::write(
                root.join(OBSERVATION_FILE),
                format!(
                    "pid={}\nenvironment=sealed\nstdin=eof\n",
                    std::process::id()
                ),
            )
            .unwrap();
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(PRODUCT_STDOUT).unwrap();
            stdout.flush().unwrap();
            let mut stderr = std::io::stderr().lock();
            stderr.write_all(PRODUCT_STDERR).unwrap();
            stderr.flush().unwrap();
        }
        _ => return,
    }
    std::process::exit(0);
}

fn protocol_bytes() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(512 * 1024 + 128);
    bytes.extend_from_slice(PROTOCOL_START);
    bytes.extend((0_u8..=255).cycle().take(512 * 1024));
    bytes.extend_from_slice(b"\nnara-public-protocol-end\n");
    bytes
}

fn drive_with_output(
    owner: &mut SupervisedProcessOwner,
    mut consume: impl FnMut(ProcessOutputStream, &[u8], bool),
) {
    let deadline = Instant::now() + WAIT_LIMIT;
    while owner.state() != SupervisedProcessState::Complete {
        let (_, outputs) = owner
            .advance_with_output(Instant::now())
            .expect("the actual process owner must advance");
        for output in outputs {
            consume(output.stream(), output.bytes(), output.loss());
        }
        assert!(Instant::now() < deadline, "physical cleanup timed out");
        if owner.state() != SupervisedProcessState::Complete {
            thread::sleep(POLL_INTERVAL);
        }
    }
}

fn assert_successful_cleanup(owner: &SupervisedProcessOwner) {
    assert_eq!(owner.state(), SupervisedProcessState::Complete);
    let receipt = owner
        .cleanup_receipt()
        .expect("only the original process owner may publish completed cleanup");
    assert!(receipt.exit().success());
    assert_eq!(receipt.exit().code(), Some(0));
    assert!(receipt.containment_empty());
    assert!(receipt.readers_stopped());
    assert!(receipt.output_drained());
    assert!(!receipt.termination_requested());
    assert!(!receipt.force_killed());
}

struct ProcessFixture {
    owner: Option<SupervisedProcessOwner>,
    directory: Option<TestDirectory>,
}

impl ProcessFixture {
    fn new(owner: SupervisedProcessOwner, directory: TestDirectory) -> Self {
        Self {
            owner: Some(owner),
            directory: Some(directory),
        }
    }

    fn owner(&self) -> &SupervisedProcessOwner {
        self.owner.as_ref().unwrap()
    }

    fn owner_mut(&mut self) -> &mut SupervisedProcessOwner {
        self.owner.as_mut().unwrap()
    }

    fn directory(&self) -> &TestDirectory {
        self.directory.as_ref().unwrap()
    }
}

impl Drop for ProcessFixture {
    fn drop(&mut self) {
        if self.owner().cleanup_receipt().is_some() {
            return;
        }
        let _ = self.owner_mut().request_terminate(Instant::now());
        for _ in 0..2 {
            let deadline = Instant::now() + WAIT_LIMIT;
            while self.owner().cleanup_receipt().is_none() && Instant::now() < deadline {
                if self.owner_mut().advance(Instant::now()).is_err() {
                    let _ = self.owner_mut().retry_cleanup(Instant::now());
                }
                thread::sleep(POLL_INTERVAL);
            }
            if self.owner().cleanup_receipt().is_some() {
                return;
            }
            let _ = self.owner_mut().retry_cleanup(Instant::now());
        }
        let owner = self.owner.take().unwrap();
        let directory = self.directory.take().unwrap();
        eprintln!(
            "public process test retains native owner {} and its directory {} until test-process exit",
            owner.process_id(),
            directory.root.display(),
        );
        std::mem::forget((owner, directory));
        if !thread::panicking() {
            panic!("public process test physical cleanup remains incomplete after retry");
        }
    }
}

struct TestDirectory {
    root: PathBuf,
    temporary_root: PathBuf,
    marker: String,
}

impl TestDirectory {
    fn new(mode: &str) -> Self {
        let temporary_root = canonical_host_path(&env::temp_dir());
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let name = format!("{TEMP_PREFIX}{}-{nonce}", std::process::id());
        let root = temporary_root.join(&name);
        let marker = format!("{name}\n{mode}\n");
        assert!(marker.len() <= MAX_MARKER_BYTES);
        fs::create_dir(&root).expect("the fixture must create its own unique temporary directory");
        fs::write(root.join(MODE_FILE), &marker).unwrap();
        Self {
            root,
            temporary_root,
            marker,
        }
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        if !self.root.is_absolute()
            || self.root.parent() != Some(self.temporary_root.as_path())
            || !self
                .root
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(TEMP_PREFIX))
            || !fs::symlink_metadata(&self.root)
                .is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink())
            || read_marker(&self.root).as_deref() != Some(self.marker.as_str())
        {
            return;
        }
        for name in [FLUSHED_FILE, OBSERVATION_FILE, MODE_FILE] {
            let _ = fs::remove_file(self.root.join(name));
        }
        let _ = fs::remove_dir(&self.root);
    }
}

fn read_marker(root: &Path) -> Option<String> {
    let mut marker = String::new();
    fs::File::open(root.join(MODE_FILE))
        .ok()?
        .take(MAX_MARKER_BYTES as u64 + 1)
        .read_to_string(&mut marker)
        .ok()?;
    (marker.len() <= MAX_MARKER_BYTES).then_some(marker)
}

fn test_executable() -> PathBuf {
    canonical_host_path(&env::current_exe().expect("the current test executable is available"))
}

fn canonical_host_path(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("the fixture path must exist");
    #[cfg(windows)]
    if let Some(path) = canonical
        .to_str()
        .and_then(|path| path.strip_prefix(r"\\?\"))
    {
        return match path.strip_prefix(r"UNC\") {
            Some(unc) => PathBuf::from(format!(r"\\{unc}")),
            None => PathBuf::from(path),
        };
    }
    canonical
}
