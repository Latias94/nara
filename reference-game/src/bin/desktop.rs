use std::{
    io::{self, Write},
    process::ExitCode,
    time::{Duration, Instant},
};

use nara::{
    diagnostic::{DiagnosticReport, DiagnosticSeverity},
    project_host::DesktopRunOutcome,
};
use nara_reference_game::bundled_desktop_run;

mod support;
use support::project_root::open_project_root;

const CLEANUP_DEADLINE: Duration = Duration::from_secs(5);

fn main() -> ExitCode {
    let project_root = match open_project_root() {
        Ok(project_root) => project_root,
        Err(error) => {
            let diagnostic = first_error_diagnostic(error.diagnostics());
            emit_static_error(&diagnostic.code, &diagnostic.summary);
            return ExitCode::FAILURE;
        }
    };
    let mut run = bundled_desktop_run(project_root);
    drive_desktop_process(
        || lower_run_report(&run.execute()),
        Instant::now,
        std::thread::park_timeout,
        emit_static_error,
    )
    .exit_code()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DesktopProcessStep {
    Completed,
    Failed(ProcessDiagnostic),
    CleanupIncomplete(Option<ProcessDiagnostic>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessDiagnostic {
    code: String,
    summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DesktopProcessExit {
    Success,
    Failure,
}

impl DesktopProcessExit {
    const fn exit_code(self) -> ExitCode {
        match self {
            Self::Success => ExitCode::SUCCESS,
            Self::Failure => ExitCode::FAILURE,
        }
    }
}

fn lower_run_report(report: &nara::project_host::DesktopRunReport) -> DesktopProcessStep {
    match report.outcome() {
        DesktopRunOutcome::Completed(_) => DesktopProcessStep::Completed,
        DesktopRunOutcome::Failed => {
            DesktopProcessStep::Failed(first_error_diagnostic(report.diagnostics()))
        }
        DesktopRunOutcome::CleanupIncomplete => DesktopProcessStep::CleanupIncomplete(
            first_reported_error_diagnostic(report.diagnostics()),
        ),
    }
}

fn first_error_diagnostic(report: &DiagnosticReport) -> ProcessDiagnostic {
    first_reported_error_diagnostic(report).unwrap_or_else(generic_failure_diagnostic)
}

fn first_reported_error_diagnostic(report: &DiagnosticReport) -> Option<ProcessDiagnostic> {
    report
        .iter()
        .find(|diagnostic| diagnostic.severity() == DiagnosticSeverity::Error)
        .map(|diagnostic| ProcessDiagnostic {
            code: diagnostic.code().as_str().to_owned(),
            summary: diagnostic.summary().as_str().to_owned(),
        })
}

fn generic_failure_diagnostic() -> ProcessDiagnostic {
    ProcessDiagnostic {
        code: "reference-game.desktop.failed".to_owned(),
        summary: "Desktop product action failed".to_owned(),
    }
}

fn generic_cleanup_timeout_diagnostic() -> ProcessDiagnostic {
    ProcessDiagnostic {
        code: "reference-game.desktop.shutdown-timeout".to_owned(),
        summary: "Desktop runtime cleanup exceeded its deadline".to_owned(),
    }
}

fn drive_desktop_process(
    mut execute: impl FnMut() -> DesktopProcessStep,
    mut now: impl FnMut() -> Instant,
    mut wait: impl FnMut(Duration),
    mut emit: impl FnMut(&str, &str),
) -> DesktopProcessExit {
    let mut cleanup_deadline = CleanupDeadline::default();
    let mut cleanup_diagnostic = None;
    loop {
        match execute() {
            DesktopProcessStep::Completed => return DesktopProcessExit::Success,
            DesktopProcessStep::Failed(diagnostic) => {
                emit(&diagnostic.code, &diagnostic.summary);
                return DesktopProcessExit::Failure;
            }
            DesktopProcessStep::CleanupIncomplete(diagnostic) => {
                if cleanup_diagnostic.is_none() {
                    cleanup_diagnostic = diagnostic;
                }
                if !cleanup_deadline.has_expired(now()) {
                    wait(Duration::from_millis(1));
                    continue;
                }
                let diagnostic = cleanup_diagnostic
                    .take()
                    .unwrap_or_else(generic_cleanup_timeout_diagnostic);
                emit(&diagnostic.code, &diagnostic.summary);
                return DesktopProcessExit::Failure;
            }
        }
    }
}

#[derive(Debug, Default)]
struct CleanupDeadline {
    deadline: Option<Instant>,
}

impl CleanupDeadline {
    fn has_expired(&mut self, now: Instant) -> bool {
        let deadline = *self
            .deadline
            .get_or_insert_with(|| now.checked_add(CLEANUP_DEADLINE).unwrap_or(now));
        now >= deadline
    }
}

fn emit_static_error(code: &str, summary: &str) {
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    let _ = write_static_error(&mut stderr, code, summary);
}

fn write_static_error(mut writer: impl Write, code: &str, summary: &str) -> Result<(), io::Error> {
    writeln!(writer, "{code}: {summary}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};

    fn diagnostic(code: &str, summary: &str) -> ProcessDiagnostic {
        ProcessDiagnostic {
            code: code.to_owned(),
            summary: summary.to_owned(),
        }
    }

    fn run_script(
        steps: impl IntoIterator<Item = DesktopProcessStep>,
        times: impl IntoIterator<Item = Instant>,
    ) -> (DesktopProcessExit, Vec<(String, String)>, usize) {
        let mut steps = steps.into_iter().collect::<VecDeque<_>>();
        let mut times = times.into_iter().collect::<VecDeque<_>>();
        let emitted = Rc::new(RefCell::new(Vec::new()));
        let emitted_sink = Rc::clone(&emitted);
        let waits = Rc::new(RefCell::new(0));
        let wait_count = Rc::clone(&waits);
        let exit = drive_desktop_process(
            || {
                steps
                    .pop_front()
                    .expect("the process requested an extra step")
            },
            || {
                times
                    .pop_front()
                    .expect("the process requested an extra time")
            },
            |_| *wait_count.borrow_mut() += 1,
            move |code, summary| {
                emitted_sink
                    .borrow_mut()
                    .push((code.to_owned(), summary.to_owned()));
            },
        );
        let emitted = std::mem::take(&mut *emitted.borrow_mut());
        let waits = *waits.borrow();
        (exit, emitted, waits)
    }

    #[test]
    fn first_incomplete_cleanup_gets_a_full_window_after_a_long_gameplay_session() {
        let process_start = Instant::now();
        let first_incomplete = process_start + Duration::from_secs(60);
        let mut deadline = CleanupDeadline::default();

        assert!(!deadline.has_expired(first_incomplete));
        assert_eq!(deadline.deadline, Some(first_incomplete + CLEANUP_DEADLINE));
    }

    #[test]
    fn cleanup_deadline_is_stable_and_expires_at_its_boundary() {
        let first_incomplete = Instant::now();
        let mut deadline = CleanupDeadline::default();

        assert!(!deadline.has_expired(first_incomplete));
        assert!(
            !deadline.has_expired(first_incomplete + CLEANUP_DEADLINE - Duration::from_nanos(1))
        );
        assert!(deadline.has_expired(first_incomplete + CLEANUP_DEADLINE));
    }

    #[test]
    fn normal_completion_has_success_exit_and_no_stderr() {
        let (exit, emitted, waits) = run_script([DesktopProcessStep::Completed], []);

        assert_eq!(exit, DesktopProcessExit::Success);
        assert_eq!(exit.exit_code(), ExitCode::SUCCESS);
        assert!(emitted.is_empty());
        assert_eq!(waits, 0);
    }

    #[test]
    fn startup_and_runtime_failures_keep_their_static_diagnostic_and_nonzero_exit() {
        for (code, summary) in [
            (
                "project.run.startup-failed",
                "Project runtime startup failed",
            ),
            (
                "project.desktop.runner-failed",
                "Desktop project runner failed",
            ),
        ] {
            let (exit, emitted, waits) =
                run_script([DesktopProcessStep::Failed(diagnostic(code, summary))], []);

            assert_eq!(exit, DesktopProcessExit::Failure);
            assert_eq!(exit.exit_code(), ExitCode::FAILURE);
            assert_eq!(emitted, vec![(code.to_owned(), summary.to_owned())]);
            assert_eq!(waits, 0);
        }
    }

    #[test]
    fn incomplete_cleanup_can_finish_inside_its_full_retry_window() {
        let first_incomplete = Instant::now();
        let (exit, emitted, waits) = run_script(
            [
                DesktopProcessStep::CleanupIncomplete(None),
                DesktopProcessStep::Completed,
            ],
            [first_incomplete],
        );

        assert_eq!(exit, DesktopProcessExit::Success);
        assert!(emitted.is_empty());
        assert_eq!(waits, 1);
    }

    #[test]
    fn cleanup_timeout_is_bounded_and_has_a_stable_nonzero_terminal() {
        let first_incomplete = Instant::now();
        let (exit, emitted, waits) = run_script(
            [
                DesktopProcessStep::CleanupIncomplete(None),
                DesktopProcessStep::CleanupIncomplete(None),
            ],
            [first_incomplete, first_incomplete + CLEANUP_DEADLINE],
        );

        assert_eq!(exit, DesktopProcessExit::Failure);
        assert_eq!(exit.exit_code(), ExitCode::FAILURE);
        assert_eq!(waits, 1);
        assert_eq!(
            emitted,
            vec![(
                "reference-game.desktop.shutdown-timeout".to_owned(),
                "Desktop runtime cleanup exceeded its deadline".to_owned(),
            )]
        );
    }

    #[test]
    fn host_shutdown_timeout_survives_incomplete_process_cleanup() {
        let first_incomplete = Instant::now();
        let (exit, emitted, waits) = run_script(
            [
                DesktopProcessStep::CleanupIncomplete(Some(diagnostic(
                    "project.desktop.shutdown-timeout",
                    "Desktop runtime shutdown exceeded its bounded deadline",
                ))),
                DesktopProcessStep::CleanupIncomplete(None),
            ],
            [first_incomplete, first_incomplete + CLEANUP_DEADLINE],
        );

        assert_eq!(exit, DesktopProcessExit::Failure);
        assert_eq!(waits, 1);
        assert_eq!(
            emitted,
            vec![(
                "project.desktop.shutdown-timeout".to_owned(),
                "Desktop runtime shutdown exceeded its bounded deadline".to_owned(),
            )]
        );
    }

    #[test]
    fn participant_runner_failure_survives_incomplete_process_cleanup() {
        let first_incomplete = Instant::now();
        let (exit, emitted, waits) = run_script(
            [
                DesktopProcessStep::CleanupIncomplete(Some(diagnostic(
                    "project.desktop.runner-failed",
                    "Desktop project runner failed",
                ))),
                DesktopProcessStep::CleanupIncomplete(None),
            ],
            [first_incomplete, first_incomplete + CLEANUP_DEADLINE],
        );

        assert_eq!(exit, DesktopProcessExit::Failure);
        assert_eq!(waits, 1);
        assert_eq!(
            emitted,
            vec![(
                "project.desktop.runner-failed".to_owned(),
                "Desktop project runner failed".to_owned(),
            )]
        );
    }

    #[test]
    fn real_startup_failure_report_lowers_to_one_stderr_line_and_nonzero_exit() {
        let project_root = open_project_root().expect("the bundled project root should open");
        let mut run = nara::project_host::DesktopRun::new(
            project_root,
            nara::project_host::DesktopRunIntent::new().with_profile("missing-test-profile"),
        );
        let report = run.execute();
        assert_eq!(report.outcome(), DesktopRunOutcome::Failed);
        let expected = ProcessDiagnostic {
            code: "project.profile.unknown".to_owned(),
            summary: "Requested project profile does not exist".to_owned(),
        };
        assert_eq!(first_error_diagnostic(report.diagnostics()), expected);
        let mut steps = VecDeque::from([lower_run_report(&report)]);
        let mut stderr = Vec::new();

        let exit = drive_desktop_process(
            || steps.pop_front().expect("the process step should exist"),
            Instant::now,
            |_| {},
            |code, summary| write_static_error(&mut stderr, code, summary).unwrap(),
        );

        assert_eq!(exit, DesktopProcessExit::Failure);
        assert_eq!(exit.exit_code(), ExitCode::FAILURE);
        assert_eq!(
            stderr,
            b"project.profile.unknown: Requested project profile does not exist\n"
        );
        assert!(include_str!("desktop.rs").contains("let stderr = io::stderr();"));
    }
}
