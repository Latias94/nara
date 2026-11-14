use std::{
    env,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use engine::project_tooling::{
    host_fs,
    process::{
        ExactExecutableLaunchAuthority, ProcessLogLimits, ProcessOutputStream,
        SupervisedProcessCommand, SupervisedProcessOwner, SupervisedProcessState,
        SupervisedProductEnvironmentPolicy, SupervisedProductProcessCommand,
    },
};

use super::{
    EXPORT_PARENT_ENV, PROJECT_ENV, TEST_NAME,
    workspace::{Temporary, Tools},
};

const POLL_INTERVAL: Duration = Duration::from_millis(10);
const CLEANUP_LIMIT: Duration = Duration::from_secs(10);

pub struct OwnedProcess {
    process: Option<SupervisedProcessOwner>,
    directory: Option<Temporary>,
}

impl OwnedProcess {
    pub fn export(directory: Temporary, tools: &Tools, project: &Path, parent: &Path) -> Self {
        let command = SupervisedProcessCommand::new(env::current_exe().unwrap(), &directory.root)
            .arguments(["--exact", TEST_NAME, "--ignored", "--nocapture"])
            .environment(PROJECT_ENV, project)
            .environment(EXPORT_PARENT_ENV, parent)
            .environment("CARGO", &tools.cargo)
            .environment("RUSTC", &tools.rustc)
            .environment("RUSTUP_TOOLCHAIN", "1.95.0")
            .environment("CARGO_TARGET_DIR", &tools.target)
            .environment("CARGO_BUILD_JOBS", "1")
            .environment("CARGO_NET_OFFLINE", "true");
        let process = SupervisedProcessOwner::spawn(&command, log_limits()).unwrap();
        Self {
            process: Some(process),
            directory: Some(directory),
        }
    }

    pub fn headless(directory: Temporary, published: &Path, cwd: &Path) -> Self {
        let executable = published
            .join("bin")
            .join(format!("headless{}", env::consts::EXE_SUFFIX));
        let file = host_fs::readable_file(&executable).unwrap();
        let binary_directory = host_fs::readable_directory(executable.parent().unwrap()).unwrap();
        let authority = ExactExecutableLaunchAuthority::from_verified_handles(
            file.duplicate_restrictive_executable_host_handle().unwrap(),
            binary_directory
                .duplicate_restrictive_host_handle()
                .unwrap(),
        )
        .unwrap();
        drop((file, binary_directory));
        let command =
            SupervisedProductProcessCommand::new(SupervisedProductEnvironmentPolicy::Sealed, cwd);
        let process =
            SupervisedProcessOwner::spawn_exact(authority, &command, log_limits()).unwrap();
        Self {
            process: Some(process),
            directory: Some(directory),
        }
    }

    pub fn run(mut self, timeout: Duration) -> (Temporary, String) {
        let deadline = Instant::now() + timeout;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        while self.owner().state() != SupervisedProcessState::Complete {
            let (_, outputs) = self
                .owner_mut()
                .advance_with_output(Instant::now())
                .unwrap();
            for output in outputs {
                assert!(!output.loss(), "finite product output was lost");
                let bytes = match output.stream() {
                    ProcessOutputStream::Stdout => &mut stdout,
                    ProcessOutputStream::Stderr => &mut stderr,
                };
                assert!(bytes.len() + output.bytes().len() <= 256 * 1024);
                bytes.extend_from_slice(output.bytes());
            }
            assert!(
                Instant::now() < deadline,
                "native process deadline exceeded; stdout: {}; stderr: {}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr),
            );
            if self.owner().state() != SupervisedProcessState::Complete {
                thread::sleep(POLL_INTERVAL);
            }
        }
        let receipt = self.owner().cleanup_receipt().unwrap();
        assert!(receipt.containment_empty());
        assert!(receipt.readers_stopped());
        assert!(receipt.output_drained());
        assert!(
            receipt.exit().success(),
            "native process failed: {:?}; stdout: {}; stderr: {}",
            receipt.exit(),
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr),
        );
        assert_eq!(receipt.exit().code(), Some(0));
        (
            self.directory.take().unwrap(),
            String::from_utf8(stdout).unwrap(),
        )
    }

    fn owner(&self) -> &SupervisedProcessOwner {
        self.process.as_ref().unwrap()
    }

    fn owner_mut(&mut self) -> &mut SupervisedProcessOwner {
        self.process.as_mut().unwrap()
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if self.owner().cleanup_receipt().is_some() {
            return;
        }
        let _ = self.owner_mut().request_terminate(Instant::now());
        for _ in 0..2 {
            let deadline = Instant::now() + CLEANUP_LIMIT;
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
        let process = self.process.take().unwrap();
        let directory = self.directory.take().unwrap();
        eprintln!(
            "Palette source Export retains process {} and directory {} until test-process exit",
            process.process_id(),
            directory.root.display(),
        );
        std::mem::forget((process, directory));
        if !thread::panicking() {
            panic!("Palette source Export physical cleanup remains incomplete");
        }
    }
}

fn log_limits() -> ProcessLogLimits {
    ProcessLogLimits::try_from_counts(128, 256 * 1024).unwrap()
}
