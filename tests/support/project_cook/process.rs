use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::{
    fs::ContentDigest,
    project_tooling::{
        cargo_build::{
            CapturedCargoExecutable, CargoBuildFailure, CargoBuildProcessCustody,
            CargoBuildRequest, build_executable,
        },
        host_fs,
        process::{
            ExactExecutableLaunchAuthority, ProcessLogLimits, ProcessOutputStream,
            SupervisedProcessCleanupReceipt, SupervisedProcessCommand, SupervisedProcessOwner,
            SupervisedProcessState, SupervisedProductEnvironmentPolicy,
            SupervisedProductProcessCommand,
        },
    },
};

const OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(10);
const OWNER: &str = "generated-project-cook-acceptance";
pub const UNRESOLVED: &str = "cleanup-pending";

pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub receipt: SupervisedProcessCleanupReceipt,
}

impl Output {
    pub fn assert_success(&self) {
        assert!(
            self.receipt.exit().success(),
            "process failed: {:?}\nstdout:\n{}\nstderr:\n{}",
            self.receipt,
            self.stdout,
            self.stderr,
        );
        assert_eq!(self.receipt.exit().code(), Some(0));
    }
}

pub struct Custody {
    pub root: PathBuf,
    remove_on_drop: bool,
    process: Option<SupervisedProcessOwner>,
    build_cleanup: Option<Box<CargoBuildProcessCustody>>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl Custody {
    pub fn create(target: &Path) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = target.join(format!("nara-project-cook-{}-{stamp}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        assert_eq!(root.parent(), Some(target));
        fs::write(root.join("fixture-owner"), OWNER).unwrap();
        Self::new(root, true)
    }

    pub fn worker(root: PathBuf, target: &Path) -> Self {
        assert!(root.is_absolute());
        assert_eq!(root.canonicalize().unwrap(), root);
        assert_eq!(root.parent(), Some(target));
        assert_eq!(
            fs::read(root.join("fixture-owner")).unwrap(),
            OWNER.as_bytes()
        );
        Self::new(root, false)
    }

    fn new(root: PathBuf, remove_on_drop: bool) -> Self {
        Self {
            root,
            remove_on_drop,
            process: None,
            build_cleanup: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    pub fn directory(&self, name: &str) -> PathBuf {
        nara::fs::RelativeComponent::new(name).unwrap();
        let path = self.root.join(name);
        fs::create_dir(&path).unwrap();
        path.canonicalize().unwrap()
    }

    pub fn tool(&mut self, command: SupervisedProcessCommand, timeout: Duration) -> Output {
        assert!(self.process.is_none() && self.build_cleanup.is_none());
        self.process = Some(SupervisedProcessOwner::spawn(&command, log_limits()).unwrap());
        self.finish(timeout)
    }

    pub fn player(&mut self, executable: &Path, cwd: &Path) -> Output {
        assert!(self.process.is_none() && self.build_cleanup.is_none());
        let file = host_fs::readable_file(executable).unwrap();
        let directory = host_fs::readable_directory(executable.parent().unwrap()).unwrap();
        let authority = ExactExecutableLaunchAuthority::from_verified_handles(
            file.duplicate_restrictive_executable_host_handle().unwrap(),
            directory.duplicate_restrictive_host_handle().unwrap(),
        )
        .unwrap();
        drop((file, directory));
        let command =
            SupervisedProductProcessCommand::new(SupervisedProductEnvironmentPolicy::Sealed, cwd)
                .arguments(["--frames", "4096"]);
        self.process =
            Some(SupervisedProcessOwner::spawn_exact(authority, &command, log_limits()).unwrap());
        self.finish(Duration::from_secs(60))
    }

    fn finish(&mut self, timeout: Duration) -> Output {
        let deadline = Instant::now() + timeout;
        self.stdout.clear();
        self.stderr.clear();
        loop {
            let owner = self.process.as_mut().unwrap();
            let (_, output) = owner.advance_with_output(Instant::now()).unwrap();
            for output in output {
                assert!(!output.loss(), "original process reported output loss");
                assert!(
                    output.bytes().len()
                        <= OUTPUT_BYTES.saturating_sub(self.stdout.len() + self.stderr.len())
                );
                let buffer = match output.stream() {
                    ProcessOutputStream::Stdout => &mut self.stdout,
                    ProcessOutputStream::Stderr => &mut self.stderr,
                };
                buffer.extend_from_slice(output.bytes());
            }
            if owner.state() == SupervisedProcessState::Complete {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "process exceeded {timeout:?}; {}",
                self.diagnostics()
            );
            thread::sleep(POLL);
        }
        let owner = self.process.as_ref().unwrap();
        let receipt = owner.cleanup_receipt().unwrap();
        assert!(
            receipt.containment_empty() && receipt.readers_stopped() && receipt.output_drained(),
            "{}",
            self.diagnostics()
        );
        assert!(
            !receipt.termination_requested() && !receipt.force_killed(),
            "{}",
            self.diagnostics()
        );
        assert!(!owner.output_loss_observed(), "{}", self.diagnostics());
        eprintln!(
            "PROJECT_COOK_PROCESS_CLEANUP pid={} receipt={receipt:?}",
            owner.process_id()
        );
        self.process.take();
        Output {
            stdout: String::from_utf8(std::mem::take(&mut self.stdout)).unwrap(),
            stderr: String::from_utf8(std::mem::take(&mut self.stderr)).unwrap(),
            receipt,
        }
    }

    pub fn build(&mut self, request: &CargoBuildRequest) -> CapturedCargoExecutable {
        assert!(self.process.is_none() && self.build_cleanup.is_none());
        let captured = match build_executable(request, |_| Ok(())) {
            Ok(captured) => captured,
            Err(CargoBuildFailure::CleanupIncomplete(custody)) => {
                self.build_cleanup = Some(custody);
                panic!("the original Cargo adapter retained build cleanup custody");
            }
            Err(error) => panic!("explicit generated-project build failed: {error:?}"),
        };
        assert_eq!(captured.provenance().selection(), request.selection());
        assert_eq!(captured.digest(), ContentDigest::of_bytes(captured.bytes()));
        captured
    }

    pub fn assert_live_timeout_cleanup(&mut self) {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        let command = SupervisedProcessCommand::new(std::env::current_exe().unwrap(), &self.root)
            .arguments([
                "--exact",
                "project_cook_waiting_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .environment("NARA_PROJECT_COOK_WAITING_CHILD", "1");
        self.process = Some(SupervisedProcessOwner::spawn(&command, log_limits()).unwrap());
        let original = self.process.as_ref().unwrap().process_id();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let owner = self.process.as_mut().unwrap();
            let (_, output) = owner.advance_with_output(Instant::now()).unwrap();
            for record in output {
                assert!(!record.loss());
                if record.stream() == ProcessOutputStream::Stdout {
                    self.stdout.extend_from_slice(record.bytes());
                }
            }
            if self
                .stdout
                .windows(b"PROJECT_COOK_WAITING_CHILD".len())
                .any(|value| value == b"PROJECT_COOK_WAITING_CHILD")
            {
                break;
            }
            assert!(owner.state() != SupervisedProcessState::Complete && Instant::now() < deadline);
            thread::sleep(POLL);
        }
        let panic = match catch_unwind(AssertUnwindSafe(|| self.finish(Duration::ZERO))) {
            Err(panic) => panic,
            Ok(_) => panic!("the real waiting child must trigger the original timeout"),
        };
        assert!(
            panic
                .downcast_ref::<String>()
                .is_some_and(|message| message.contains("process exceeded"))
        );
        let owner = self
            .process
            .as_mut()
            .expect("unwind must retain the original owner");
        assert_eq!(owner.process_id(), original);
        assert!(owner.cleanup_receipt().is_none());
        owner.request_terminate(Instant::now()).unwrap();
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        while owner.state() != SupervisedProcessState::Complete {
            owner.advance(Instant::now()).unwrap();
            assert!(Instant::now() < deadline);
            thread::sleep(POLL);
        }
        let receipt = owner.cleanup_receipt().unwrap();
        assert!(
            receipt.containment_empty() && receipt.readers_stopped() && receipt.output_drained()
        );
        assert!(receipt.termination_requested());
        assert!(!owner.output_loss_observed());
        eprintln!("PROJECT_COOK_TIMEOUT_CLEANUP pid={original} receipt={receipt:?}");
        self.process.take();
    }

    fn diagnostics(&self) -> String {
        format!(
            "pid={:?} state={:?} receipt={:?}\nstdout:\n{}\nstderr:\n{}",
            self.process
                .as_ref()
                .map(SupervisedProcessOwner::process_id),
            self.process.as_ref().map(SupervisedProcessOwner::state),
            self.process
                .as_ref()
                .and_then(SupervisedProcessOwner::cleanup_receipt),
            String::from_utf8_lossy(&self.stdout[self.stdout.len().saturating_sub(8192)..]),
            String::from_utf8_lossy(&self.stderr[self.stderr.len().saturating_sub(8192)..]),
        )
    }

    fn retire(&mut self) -> bool {
        if let Some(build) = self.build_cleanup.as_mut() {
            let deadline = Instant::now() + CLEANUP_TIMEOUT;
            while build.retry_cleanup().is_err() {
                if Instant::now() >= deadline {
                    return false;
                }
                thread::sleep(POLL);
            }
            self.build_cleanup.take();
        }
        if let Some(owner) = self.process.as_mut() {
            if owner.state() != SupervisedProcessState::Complete {
                let _ = owner.request_terminate(Instant::now());
            }
            for _ in 0..2 {
                let deadline = Instant::now() + CLEANUP_TIMEOUT;
                while owner.state() != SupervisedProcessState::Complete && Instant::now() < deadline
                {
                    if owner.advance(Instant::now()).is_err() {
                        let _ = owner.retry_cleanup(Instant::now());
                    }
                    thread::sleep(POLL);
                }
                if owner.cleanup_receipt().is_some_and(|receipt| {
                    receipt.containment_empty()
                        && receipt.readers_stopped()
                        && receipt.output_drained()
                }) {
                    self.process.take();
                    return true;
                }
                let _ = owner.retry_cleanup(Instant::now());
            }
            return false;
        }
        true
    }
}

impl Drop for Custody {
    fn drop(&mut self) {
        if !self.retire() {
            let message = format!(
                "retained original process/build custody; {}",
                self.diagnostics()
            );
            let _ = fs::write(self.root.join(UNRESOLVED), &message);
            eprintln!("{message}; retained fixture={}", self.root.display());
            drop(self.process.take());
            drop(self.build_cleanup.take());
            if !thread::panicking() {
                panic!("generated-project cleanup remains incomplete");
            }
            return;
        }
        if self.remove_on_drop {
            if thread::panicking() || self.root.join(UNRESOLVED).exists() {
                eprintln!(
                    "retained failed generated-project fixture={}",
                    self.root.display()
                );
            } else {
                remove_owned_tree(&self.root, &self.root);
            }
        }
    }
}

pub fn install(captured: CapturedCargoExecutable, directory: &Path, name: &str) -> PathBuf {
    let path = directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(!path.exists());
    fs::write(&path, captured.bytes()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    assert_eq!(
        host_fs::readable_file(&path)
            .unwrap()
            .digest(512 * 1024 * 1024)
            .unwrap(),
        captured.digest()
    );
    path
}

pub fn remove_owned_tree(owned: &Path, path: &Path) {
    assert!(owned.is_absolute() && owned.canonicalize().unwrap() == owned);
    let selected = path.canonicalize().unwrap();
    assert!(selected == path && selected.starts_with(owned));
    let mut pending = vec![selected.clone()];
    let mut visited = 0;
    while let Some(path) = pending.pop() {
        visited += 1;
        assert!(visited <= 16_384, "bounded generated-project fixture tree");
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(
            !metadata.file_type().is_symlink(),
            "owned fixtures must not contain links"
        );
        assert!(path.starts_with(owned));
        if metadata.is_dir() {
            for entry in fs::read_dir(&path).unwrap() {
                pending.push(entry.unwrap().path());
            }
        } else {
            assert!(metadata.is_file());
        }
    }
    fs::remove_dir_all(selected).unwrap();
}

fn log_limits() -> ProcessLogLimits {
    ProcessLogLimits::try_from_counts(512, OUTPUT_BYTES).unwrap()
}
