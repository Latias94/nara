use std::{
    env,
    ffi::OsStr,
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
    BUILD_ROOT_ENV, TEST_NAME,
    material_workspace::{COOKER, cooker_directory},
    workspace::{Temporary, Tools},
};

pub struct OwnedProcess {
    process: Option<SupervisedProcessOwner>,
    directory: Option<Temporary>,
}

impl OwnedProcess {
    pub fn build(directory: Temporary, tools: &Tools, project: &Path) -> Self {
        let command = SupervisedProcessCommand::new(env::current_exe().unwrap(), &directory.root)
            .arguments(["--exact", TEST_NAME, "--ignored", "--nocapture"])
            .environment(BUILD_ROOT_ENV, project);
        Self::tool(directory, tools, command)
    }

    pub fn cook(directory: Temporary, tools: &Tools, project: &Path, output: &Path) -> Self {
        let executable =
            cooker_directory(&directory.root).join(format!("{COOKER}{}", env::consts::EXE_SUFFIX));
        let command = SupervisedProcessCommand::new(executable, &directory.root).arguments([
            project.as_os_str(),
            output.as_os_str(),
            tools.cargo.as_os_str(),
            tools.rustc.as_os_str(),
            OsStr::new(&tools.host),
            tools.target.as_os_str(),
        ]);
        Self::tool(directory, tools, command)
    }

    fn tool(directory: Temporary, tools: &Tools, command: SupervisedProcessCommand) -> Self {
        let command = command
            .environment("CARGO", &tools.cargo)
            .environment("RUSTC", &tools.rustc)
            .environment("RUSTUP_TOOLCHAIN", "1.95.0")
            .environment("CARGO_TARGET_DIR", &tools.target)
            .environment("CARGO_BUILD_JOBS", "1")
            .environment("CARGO_NET_OFFLINE", "true");
        Self {
            process: Some(SupervisedProcessOwner::spawn(&command, log_limits()).unwrap()),
            directory: Some(directory),
        }
    }

    pub fn player(directory: Temporary, executable: &Path, cwd: &Path) -> Self {
        let file = host_fs::readable_file(executable).unwrap();
        let parent = host_fs::readable_directory(executable.parent().unwrap()).unwrap();
        let authority = ExactExecutableLaunchAuthority::from_verified_handles(
            file.duplicate_restrictive_executable_host_handle().unwrap(),
            parent.duplicate_restrictive_host_handle().unwrap(),
        )
        .unwrap();
        drop((file, parent));
        let command =
            SupervisedProductProcessCommand::new(SupervisedProductEnvironmentPolicy::Sealed, cwd);
        Self {
            process: Some(
                SupervisedProcessOwner::spawn_exact(authority, &command, log_limits()).unwrap(),
            ),
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
                assert!(!output.loss(), "finite Palette process output was lost");
                let bytes = match output.stream() {
                    ProcessOutputStream::Stdout => &mut stdout,
                    ProcessOutputStream::Stderr => &mut stderr,
                };
                assert!(bytes.len() + output.bytes().len() <= 256 * 1024);
                bytes.extend_from_slice(output.bytes());
            }
            assert!(
                Instant::now() < deadline,
                "Palette process deadline; stdout: {}; stderr: {}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr),
            );
            if self.owner().state() != SupervisedProcessState::Complete {
                thread::sleep(Duration::from_millis(10));
            }
        }
        let receipt = self.owner().cleanup_receipt().unwrap();
        assert!(receipt.containment_empty());
        assert!(receipt.readers_stopped());
        assert!(receipt.output_drained());
        assert!(
            receipt.exit().success(),
            "Palette process failed: {:?}; stdout: {}; stderr: {}",
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
            let deadline = Instant::now() + Duration::from_secs(10);
            while self.owner().cleanup_receipt().is_none() && Instant::now() < deadline {
                if self.owner_mut().advance(Instant::now()).is_err() {
                    let _ = self.owner_mut().retry_cleanup(Instant::now());
                }
                thread::sleep(Duration::from_millis(10));
            }
            if self.owner().cleanup_receipt().is_some() {
                return;
            }
            let _ = self.owner_mut().retry_cleanup(Instant::now());
        }
        let process = self.process.take().unwrap();
        let directory = self.directory.take().unwrap();
        eprintln!(
            "Palette cooked delivery retains process {} and directory {} until test-process exit",
            process.process_id(),
            directory.root.display(),
        );
        std::mem::forget((process, directory));
        if !thread::panicking() {
            panic!("Palette cooked delivery physical cleanup remains incomplete");
        }
    }
}

fn log_limits() -> ProcessLogLimits {
    ProcessLogLimits::try_from_counts(128, 256 * 1024).unwrap()
}
