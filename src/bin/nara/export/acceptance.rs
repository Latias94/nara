use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

use nara::{
    fs::FsError,
    project_workflow::{
        ProjectExportBinaryTarget, ProjectExportMemberKind, ProjectExportMemberReceipt,
        ProjectExportPlanReceipt,
    },
};
use thiserror::Error;

use crate::host_fs::{self, HostFileError};

const PROCESS_DEADLINE: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
static NEXT_ACCEPTANCE_DIRECTORY: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
pub(crate) enum ExportAcceptanceError {
    #[error("the export plan is missing its {target:?} executable")]
    MissingExecutable { target: ProjectExportBinaryTarget },
    #[error("failed to open the published {target:?} executable")]
    OpenExecutable {
        target: ProjectExportBinaryTarget,
        #[source]
        source: HostFileError,
    },
    #[error("the published {target:?} executable no longer matches the export plan")]
    ExecutableChanged {
        target: ProjectExportBinaryTarget,
        #[source]
        source: FsError,
    },
    #[error("failed to create the unrelated acceptance working directory")]
    CreateWorkingDirectory(#[source] io::Error),
    #[error("failed to start the published {target:?} executable")]
    Start {
        target: ProjectExportBinaryTarget,
        #[source]
        source: io::Error,
    },
    #[error("failed while waiting for the published {target:?} executable")]
    Wait {
        target: ProjectExportBinaryTarget,
        #[source]
        source: io::Error,
    },
    #[error("the published {target:?} executable exceeded its bounded deadline")]
    TimedOut { target: ProjectExportBinaryTarget },
    #[error("the published {target:?} executable failed with {status}")]
    Failed {
        target: ProjectExportBinaryTarget,
        status: ExitStatus,
    },
}

pub(super) fn accept(
    published_root: &Path,
    plan: &ProjectExportPlanReceipt,
) -> Result<(), ExportAcceptanceError> {
    let working_directory = AcceptanceWorkingDirectory::new()?;
    for (target, arguments) in [
        (ProjectExportBinaryTarget::Headless, &[][..]),
        (
            ProjectExportBinaryTarget::Desktop,
            &["--verify-first-present"][..],
        ),
    ] {
        let member = executable_member(plan, target)?;
        let executable = published_root.join(member.path().as_str());
        host_fs::readable_file(&executable)
            .map_err(|source| ExportAcceptanceError::OpenExecutable { target, source })?
            .verify_digest(member.digest(), member.digest().length())
            .map_err(|source| ExportAcceptanceError::ExecutableChanged { target, source })?;
        run_bounded(target, &executable, arguments, working_directory.path())?;
    }
    Ok(())
}

fn executable_member(
    plan: &ProjectExportPlanReceipt,
    target: ProjectExportBinaryTarget,
) -> Result<&ProjectExportMemberReceipt, ExportAcceptanceError> {
    let kind = match target {
        ProjectExportBinaryTarget::Headless => ProjectExportMemberKind::HeadlessExecutable,
        ProjectExportBinaryTarget::Desktop => ProjectExportMemberKind::DesktopExecutable,
    };
    plan.members()
        .iter()
        .find(|member| member.kind() == kind)
        .ok_or(ExportAcceptanceError::MissingExecutable { target })
}

fn run_bounded(
    target: ProjectExportBinaryTarget,
    executable: &Path,
    arguments: &[&str],
    working_directory: &Path,
) -> Result<(), ExportAcceptanceError> {
    let mut child = Command::new(executable)
        .args(arguments)
        .current_dir(working_directory)
        .spawn()
        .map_err(|source| ExportAcceptanceError::Start { target, source })?;
    let deadline = Instant::now() + PROCESS_DEADLINE;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|source| ExportAcceptanceError::Wait { target, source })?
        {
            return if status.success() {
                Ok(())
            } else {
                Err(ExportAcceptanceError::Failed { target, status })
            };
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ExportAcceptanceError::TimedOut { target });
        }
        thread::sleep(POLL_INTERVAL);
    }
}

struct AcceptanceWorkingDirectory {
    path: PathBuf,
}

impl AcceptanceWorkingDirectory {
    fn new() -> Result<Self, ExportAcceptanceError> {
        for _ in 0..16 {
            let sequence = NEXT_ACCEPTANCE_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nara-export-acceptance-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(ExportAcceptanceError::CreateWorkingDirectory(error));
                }
            }
        }
        Err(ExportAcceptanceError::CreateWorkingDirectory(
            io::Error::new(
                io::ErrorKind::AlreadyExists,
                "bounded acceptance working-directory names are exhausted",
            ),
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for AcceptanceWorkingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}
