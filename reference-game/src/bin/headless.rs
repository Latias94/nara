use std::{
    env,
    ffi::{OsStr, OsString},
    io::{self, Write},
    num::NonZeroU32,
    process::ExitCode,
    time::{Duration, Instant},
};

use nara::{
    diagnostic::{DiagnosticReport, DiagnosticSeverity},
    project_host::{HeadlessRunOutcome, HeadlessRunReport},
};
use nara_reference_game::{
    ReferenceProductObservation, WaveSnapshot,
    bundled_wave_product_run_with_completed_tick_observer,
};

mod startup_marker;
mod support;

use startup_marker::StartupMarker;
use support::project_root::open_project_root;

const DEFAULT_MAXIMUM_TICKS: u32 = 96;
const MAXIMUM_CLI_TICKS: u32 = 256;
const CLI_ARGUMENT_ERROR_CODE: &str = "reference-game.cli.invalid-arguments";
const CLI_ARGUMENT_ERROR_SUMMARY: &str = "Headless arguments are invalid";
const CLEANUP_TIMEOUT_CODE: &str = "reference-game.run.cleanup-timeout";
const CLEANUP_TIMEOUT_SUMMARY: &str = "Headless runtime cleanup exceeded its deadline";
const UI_OBSERVATION_ENV: &str = "NARA_REFERENCE_UI_OBSERVATION";

fn main() -> ExitCode {
    if let Some(exit) = nara::project_tooling::run_project_tool_mode(
        env::args_os().skip(1),
        nara_reference_game::project_recipe,
    ) {
        return exit;
    }
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
    let marker = match StartupMarker::from_environment("headless_first_authoritative_tick") {
        Ok(marker) => std::sync::Arc::new(marker),
        Err(error) => {
            let summary = error.to_string();
            emit_static_error(error.code(), &summary);
            return ExitCode::FAILURE;
        }
    };
    let observer = std::sync::Arc::clone(&marker);
    let mut run =
        bundled_wave_product_run_with_completed_tick_observer(root, maximum_ticks, move |_| {
            let _ = observer.emit();
        });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut observation_failure = None;
    let terminal = drive_to_terminal(
        || {
            let report = run.execute_bounded();
            if matches!(report.outcome(), HeadlessRunOutcome::Completed(_)) {
                let result = report
                    .startup_content_closure()
                    .ok_or_else(|| {
                        io::Error::other(
                            "headless run completed without its admitted startup closure",
                        )
                    })
                    .and_then(|closure| {
                        nara::project_tooling::record_product_observation(
                            nara::project_tooling::ProductObservationTarget::Headless,
                            closure,
                        )
                    });
                if let Err(error) = result {
                    observation_failure = Some(error);
                }
            }
            classify_run_report(report)
        },
        deadline,
        Instant::now,
        || std::thread::park_timeout(Duration::from_millis(1)),
    );
    if let Some(error) = observation_failure {
        emit_static_error(
            "reference-game.product-observation.failed",
            &error.to_string(),
        );
        return ExitCode::FAILURE;
    }
    if matches!(&terminal, CliRunTerminal::Completed(_))
        && let Err(error) = marker.verify_success()
    {
        let summary = error.to_string();
        emit_static_error(error.code(), &summary);
        return ExitCode::FAILURE;
    }
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    let emit_ui_observation = env::var_os(UI_OBSERVATION_ENV).as_deref() == Some(OsStr::new("1"));
    write_terminal(terminal, &mut stdout, &mut stderr, emit_ui_observation)
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

enum CliRunProgress {
    Completed(Box<ReferenceProductObservation>),
    Failed(DiagnosticReport),
    CleanupIncomplete(DiagnosticReport),
}

enum CliRunTerminal {
    Completed(Box<ReferenceProductObservation>),
    Failed(DiagnosticReport),
    CleanupTimedOut(DiagnosticReport),
}

fn classify_run_report(report: HeadlessRunReport<ReferenceProductObservation>) -> CliRunProgress {
    match report.outcome() {
        HeadlessRunOutcome::Completed(snapshot) => {
            CliRunProgress::Completed(Box::new(snapshot.clone()))
        }
        HeadlessRunOutcome::Failed => CliRunProgress::Failed(report.diagnostics().clone()),
        HeadlessRunOutcome::CleanupIncomplete => {
            CliRunProgress::CleanupIncomplete(report.diagnostics().clone())
        }
    }
}

fn drive_to_terminal(
    mut step: impl FnMut() -> CliRunProgress,
    deadline: Instant,
    mut now: impl FnMut() -> Instant,
    mut wait: impl FnMut(),
) -> CliRunTerminal {
    loop {
        match step() {
            CliRunProgress::Completed(snapshot) => return CliRunTerminal::Completed(snapshot),
            CliRunProgress::Failed(report) => return CliRunTerminal::Failed(report),
            CliRunProgress::CleanupIncomplete(_) if now() < deadline => wait(),
            CliRunProgress::CleanupIncomplete(report) => {
                return CliRunTerminal::CleanupTimedOut(report);
            }
        }
    }
}

fn write_terminal(
    terminal: CliRunTerminal,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    emit_ui_observation: bool,
) -> ExitCode {
    match terminal {
        CliRunTerminal::Completed(observation) => {
            write_terminal_success(stdout, stderr, &observation, emit_ui_observation)
        }
        CliRunTerminal::Failed(report) => {
            let _ = write_diagnostics(stderr, &report);
            ExitCode::FAILURE
        }
        CliRunTerminal::CleanupTimedOut(report) => {
            let _ = write_diagnostics(stderr, &report);
            let _ = write_static_error(stderr, CLEANUP_TIMEOUT_CODE, CLEANUP_TIMEOUT_SUMMARY);
            ExitCode::FAILURE
        }
    }
}

fn write_terminal_success(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    observation: &ReferenceProductObservation,
    emit_ui_observation: bool,
) -> ExitCode {
    let snapshot = observation.wave();
    if !snapshot.is_terminal() {
        let _ = write_static_error(
            stderr,
            "reference-game.run.non-terminal",
            "Headless run returned a non-terminal wave",
        );
        return ExitCode::FAILURE;
    }
    let ui = if emit_ui_observation {
        let Some(ui) = observation.ui() else {
            let _ = write_static_error(
                stderr,
                "reference-game.ui-observation.missing",
                "Headless run did not retain its immutable UI observation",
            );
            return ExitCode::FAILURE;
        };
        Some(ui)
    } else {
        None
    };
    if write_success(stdout, snapshot).is_err() {
        let _ = write_static_error(
            stderr,
            "reference-game.cli.stdout-write",
            "Headless result could not be written",
        );
        return ExitCode::FAILURE;
    }
    if let Some(ui) = ui
        && stdout.write_all(ui.to_json_line().as_bytes()).is_err()
    {
        let _ = write_static_error(
            stderr,
            "reference-game.cli.stdout-write",
            "Headless UI observation could not be written",
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

fn emit_static_error(code: &'static str, summary: &str) {
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    let _ = write_static_error(&mut stderr, code, summary);
}

fn write_static_error(
    writer: &mut impl Write,
    code: &'static str,
    summary: &str,
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use nara::{
        fs::{FsError, FsOperation},
        project_host::ProjectCandidateError,
    };

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

        let observation = ReferenceProductObservation::new(snapshot, None);
        let exit = write_terminal_success(&mut stdout, &mut stderr, &observation, false);

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

        let terminal = drive_to_terminal(
            || {
                steps.set(steps.get() + 1);
                CliRunProgress::CleanupIncomplete(DiagnosticReport::default())
            },
            deadline,
            || {
                let read = clock_reads.get();
                clock_reads.set(read + 1);
                if read == 0 { start } else { deadline }
            },
            || waits.set(waits.get() + 1),
        );
        let exit = write_terminal(terminal, &mut stdout, &mut stderr, false);

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
