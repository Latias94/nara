use std::{
    fs::File,
    io::{self, Write},
    num::NonZeroU32,
    path::Path,
    process::ExitCode,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    fs::{
        CapabilityRights, DirectoryCapability, FsError, FsOperation, HostCapabilityOptions,
        TrustMode,
    },
    project_host::{HeadlessRunOutcome, ProjectCandidateError},
};
use nara_reference_game::project_headless_run;

fn main() -> ExitCode {
    let root = match open_project_root() {
        Ok(root) => root,
        Err(error) => {
            emit_diagnostics(error.diagnostics());
            return ExitCode::FAILURE;
        }
    };
    let mut run = project_headless_run(root, NonZeroU32::new(3).unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(snapshot) => {
                println!(
                    "tick={} enemy_hp={}",
                    snapshot.tick, snapshot.enemy_hit_points
                );
                return ExitCode::SUCCESS;
            }
            HeadlessRunOutcome::Failed => {
                emit_diagnostics(report.diagnostics());
                return ExitCode::FAILURE;
            }
            HeadlessRunOutcome::CleanupIncomplete if Instant::now() < deadline => {
                std::thread::yield_now();
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                emit_diagnostics(report.diagnostics());
                return ExitCode::FAILURE;
            }
        }
    }
}

fn emit_diagnostics(report: &nara::diagnostic::DiagnosticReport) {
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    let _ = write_diagnostics(&mut stderr, report);
}

fn write_diagnostics(
    writer: &mut impl Write,
    report: &nara::diagnostic::DiagnosticReport,
) -> io::Result<()> {
    for diagnostic in report.iter() {
        writeln!(
            writer,
            "{}: {}",
            diagnostic.code().as_str(),
            diagnostic.summary().as_str()
        )?;
    }
    Ok(())
}

fn open_project_root() -> Result<DirectoryCapability, ProjectCandidateError> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = host_directory(project_root).map_err(|source| {
        ProjectCandidateError::from_manifest_authority(FsError::Io {
            operation: FsOperation::OpenDirectory,
            source,
        })
    })?;
    DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
    .map_err(ProjectCandidateError::from_manifest_authority)
}

fn portable_trust() -> TrustMode {
    if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    }
}

fn host_directory(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }

    #[cfg(unix)]
    {
        File::open(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_failures_use_the_structured_cli_sink() {
        let error = ProjectCandidateError::from_manifest_authority(FsError::Io {
            operation: FsOperation::OpenDirectory,
            source: io::Error::new(io::ErrorKind::NotFound, "sensitive host path"),
        });
        let mut output = Vec::new();

        write_diagnostics(&mut output, error.diagnostics()).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "project.manifest.host-io: Project manifest host I/O failed\n"
        );
    }
}
