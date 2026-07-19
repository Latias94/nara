use std::{
    env,
    ffi::OsString,
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
    diagnostic::{DiagnosticReport, DiagnosticSeverity},
    fs::{
        CapabilityRights, DirectoryCapability, FsError, FsOperation, HostCapabilityOptions,
        TrustMode,
    },
    project_host::{HeadlessRunOutcome, HeadlessRunReport, ProjectCandidateError},
};
use nara_reference_game::{WaveSnapshot, bundled_wave_run};

const DEFAULT_MAXIMUM_TICKS: u32 = 96;
const MAXIMUM_CLI_TICKS: u32 = 256;
const CLI_ARGUMENT_ERROR_CODE: &str = "reference-game.cli.invalid-arguments";
const CLI_ARGUMENT_ERROR_SUMMARY: &str = "Headless arguments are invalid";
const CLEANUP_TIMEOUT_CODE: &str = "reference-game.run.cleanup-timeout";
const CLEANUP_TIMEOUT_SUMMARY: &str = "Headless runtime cleanup exceeded its deadline";

fn main() -> ExitCode {
    let maximum_ticks = match parse_maximum_ticks(env::args_os().skip(1)) {
        Some(maximum_ticks) => maximum_ticks,
        None => {
            emit_static_error(CLI_ARGUMENT_ERROR_CODE, CLI_ARGUMENT_ERROR_SUMMARY);
            return ExitCode::FAILURE;
        }
    };
    let root = match open_project_root() {
        Ok(root) => root,
        Err(error) => {
            emit_diagnostics(error.diagnostics());
            return ExitCode::FAILURE;
        }
    };
    let mut run = bundled_wave_run(root, maximum_ticks);
    let deadline = Instant::now() + Duration::from_secs(5);
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    drive_to_exit(
        || classify_run_report(run.execute_bounded()),
        deadline,
        Instant::now,
        || std::thread::park_timeout(Duration::from_millis(1)),
        &mut stdout,
        &mut stderr,
    )
}

fn parse_maximum_ticks(arguments: impl IntoIterator<Item = OsString>) -> Option<NonZeroU32> {
    let mut arguments = arguments.into_iter();
    let Some(first) = arguments.next() else {
        return NonZeroU32::new(DEFAULT_MAXIMUM_TICKS);
    };
    if first != "--max-ticks" {
        return None;
    }
    let value = arguments.next()?;
    if arguments.next().is_some() {
        return None;
    }
    value
        .to_str()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value <= MAXIMUM_CLI_TICKS)
        .and_then(NonZeroU32::new)
}

enum CliRunStep {
    Completed(WaveSnapshot),
    Failed(DiagnosticReport),
    CleanupIncomplete(DiagnosticReport),
}

fn classify_run_report(report: HeadlessRunReport<WaveSnapshot>) -> CliRunStep {
    match report.outcome() {
        HeadlessRunOutcome::Completed(snapshot) => CliRunStep::Completed(snapshot.clone()),
        HeadlessRunOutcome::Failed => CliRunStep::Failed(report.diagnostics().clone()),
        HeadlessRunOutcome::CleanupIncomplete => {
            CliRunStep::CleanupIncomplete(report.diagnostics().clone())
        }
    }
}

fn drive_to_exit(
    mut step: impl FnMut() -> CliRunStep,
    deadline: Instant,
    mut now: impl FnMut() -> Instant,
    mut wait: impl FnMut(),
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> ExitCode {
    loop {
        match step() {
            CliRunStep::Completed(snapshot) => {
                return write_terminal_success(stdout, stderr, &snapshot);
            }
            CliRunStep::Failed(report) => {
                let _ = write_diagnostics(stderr, &report);
                return ExitCode::FAILURE;
            }
            CliRunStep::CleanupIncomplete(_) if now() < deadline => wait(),
            CliRunStep::CleanupIncomplete(report) => {
                let _ = write_diagnostics(stderr, &report);
                let _ = write_static_error(stderr, CLEANUP_TIMEOUT_CODE, CLEANUP_TIMEOUT_SUMMARY);
                return ExitCode::FAILURE;
            }
        }
    }
}

fn write_terminal_success(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    snapshot: &WaveSnapshot,
) -> ExitCode {
    if !snapshot.is_terminal() {
        let _ = write_static_error(
            stderr,
            "reference-game.run.non-terminal",
            "Headless run returned a non-terminal wave",
        );
        return ExitCode::FAILURE;
    }
    if write_success(stdout, snapshot).is_err() {
        let _ = write_static_error(
            stderr,
            "reference-game.cli.stdout-write",
            "Headless result could not be written",
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn write_success(writer: &mut impl Write, snapshot: &WaveSnapshot) -> io::Result<()> {
    writeln!(
        writer,
        concat!(
            "{{\"schema\":\"nara-reference-game.wave-summary-v1\",",
            "\"outcome\":\"{}\",\"tick\":{},\"score\":{},",
            "\"player_hit_points\":{},\"enemies_remaining\":{},",
            "\"projectiles_remaining\":{}}}"
        ),
        snapshot.outcome.as_str(),
        snapshot.tick,
        snapshot.score,
        snapshot.player.hit_points,
        snapshot.enemies.len(),
        snapshot.projectiles.len(),
    )
}

fn emit_static_error(code: &'static str, summary: &'static str) {
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    let _ = write_static_error(&mut stderr, code, summary);
}

fn write_static_error(
    writer: &mut impl Write,
    code: &'static str,
    summary: &'static str,
) -> io::Result<()> {
    writeln!(writer, "{code}: {summary}")
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
        if diagnostic.severity() != DiagnosticSeverity::Error {
            continue;
        }
        writeln!(
            writer,
            "{}: {}",
            diagnostic.code().as_str(),
            diagnostic.summary().as_str()
        )?;
    }
    Ok(())
}

fn open_project_root() -> Result<DirectoryCapability, Box<ProjectCandidateError>> {
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = host_directory(project_root).map_err(|source| {
        Box::new(ProjectCandidateError::from_manifest_authority(FsError::Io {
            operation: FsOperation::OpenDirectory,
            source,
        }))
    })?;
    DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
    .map_err(|error| Box::new(ProjectCandidateError::from_manifest_authority(error)))
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
    use std::cell::Cell;

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

    #[test]
    fn defeated_wave_uses_the_same_stable_json_schema() {
        let snapshot = WaveSnapshot {
            tick: 4,
            outcome: nara_reference_game::WaveOutcome::Defeated,
            player: nara_reference_game::PlayerSnapshot {
                id: "player".to_owned(),
                position: nara::prelude::Vec2::ZERO,
                hit_points: 0,
            },
            ..WaveSnapshot::default()
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = write_terminal_success(&mut stdout, &mut stderr, &snapshot);

        assert_eq!(exit, ExitCode::SUCCESS);
        assert!(stderr.is_empty());
        assert_eq!(
            String::from_utf8(stdout).unwrap(),
            concat!(
                "{\"schema\":\"nara-reference-game.wave-summary-v1\",",
                "\"outcome\":\"defeated\",\"tick\":4,\"score\":0,",
                "\"player_hit_points\":0,\"enemies_remaining\":0,",
                "\"projectiles_remaining\":0}\n"
            )
        );
    }

    #[test]
    fn incomplete_cleanup_retries_only_until_the_deadline() {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(5);
        let steps = Cell::new(0_u32);
        let clock_reads = Cell::new(0_u32);
        let waits = Cell::new(0_u32);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let exit = drive_to_exit(
            || {
                steps.set(steps.get() + 1);
                CliRunStep::CleanupIncomplete(DiagnosticReport::default())
            },
            deadline,
            || {
                let read = clock_reads.get();
                clock_reads.set(read + 1);
                if read == 0 {
                    start
                } else {
                    deadline
                }
            },
            || waits.set(waits.get() + 1),
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(exit, ExitCode::FAILURE);
        assert_eq!(steps.get(), 2);
        assert_eq!(waits.get(), 1);
        assert!(stdout.is_empty());
        assert_eq!(
            String::from_utf8(stderr).unwrap(),
            "reference-game.run.cleanup-timeout: Headless runtime cleanup exceeded its deadline\n"
        );
    }
}
