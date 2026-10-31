use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use nara_app::{
    App, RuntimeAdmissionReservation, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl,
    RuntimeControlRequestResult, RuntimeInstance, RuntimeObligationLedger,
};

#[derive(Clone, Copy)]
enum CloseFailureMode {
    Pending,
    RetryableParticipant,
    CompletedWithError,
}

struct ControlledCloseParticipant {
    released: Arc<AtomicBool>,
    mode: CloseFailureMode,
}

impl RuntimeCloseParticipant for ControlledCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut nara_app::RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.close_once()
    }

    fn poll_close(
        &mut self,
        _context: &mut nara_app::RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.close_once()
    }
}

impl ControlledCloseParticipant {
    fn close_once(&self) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        if self.released.load(Ordering::Acquire) {
            return Ok(RuntimeCloseProgress::Complete);
        }
        match self.mode {
            CloseFailureMode::Pending => Ok(RuntimeCloseProgress::Pending),
            CloseFailureMode::RetryableParticipant => Err(RuntimeCloseParticipantError::retryable(
                "nara.test.desktop-close-participant",
            )),
            CloseFailureMode::CompletedWithError => Ok(RuntimeCloseProgress::CompleteWithError {
                code: "nara.test.desktop-close-terminal",
            }),
        }
    }
}

fn has_code(report: &DiagnosticReport, code: &str) -> bool {
    report
        .iter()
        .any(|diagnostic| diagnostic.code().as_str() == code)
}

fn diagnostic_field(report: &DiagnosticReport, code: &str, key: &str) -> Option<String> {
    report
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == code)
        .and_then(|diagnostic| {
            diagnostic
                .fields()
                .iter()
                .find(|field| field.key().as_str() == key)
        })
        .map(|field| field.display_value().into_owned())
}

fn actual_close_evidence(mode: CloseFailureMode) -> RuntimeCloseEvidence {
    let released = Arc::new(AtomicBool::new(false));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.desktop-close-evidence"),
            ControlledCloseParticipant {
                released: Arc::clone(&released),
                mode,
            },
        )
        .unwrap();
    let policy = match mode {
        CloseFailureMode::Pending => RuntimeClosePolicy::new(Duration::ZERO),
        CloseFailureMode::RetryableParticipant | CloseFailureMode::CompletedWithError => {
            RuntimeClosePolicy::default()
        }
    };
    let mut runtime = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(App::new().seal().unwrap(), obligations, policy)
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    drive_until_state(&mut runtime, RuntimeState::CloseIncomplete);
    let evidence = runtime.close_evidence().clone();

    released.store(true, Ordering::Release);
    assert!(matches!(
        runtime.request_control(RuntimeControl::RetryClose),
        RuntimeControlRequestResult::Accepted(_)
    ));
    drive_until_state(&mut runtime, RuntimeState::Stopped);
    evidence
}

fn drive_until_state(runtime: &mut RuntimeInstance, expected: RuntimeState) {
    for _ in 0..8 {
        if runtime.state() == expected {
            return;
        }
        runtime.drive(Duration::ZERO).unwrap();
    }
    panic!(
        "runtime remained in {:?} instead of reaching {expected:?}",
        runtime.state()
    );
}

fn deadline_close_evidence() -> RuntimeCloseEvidence {
    let evidence = actual_close_evidence(CloseFailureMode::Pending);
    assert!(
        evidence
            .causes()
            .contains(&RuntimeCloseCause::DeadlineExceeded)
    );
    evidence
}

fn participant_close_evidence() -> RuntimeCloseEvidence {
    let evidence = actual_close_evidence(CloseFailureMode::RetryableParticipant);
    assert!(
        evidence
            .causes()
            .iter()
            .any(|cause| matches!(cause, RuntimeCloseCause::ParticipantError { .. }))
    );
    assert!(
        !evidence
            .causes()
            .contains(&RuntimeCloseCause::DeadlineExceeded)
    );
    evidence
}

#[test]
fn deadline_close_incomplete_reports_shutdown_timeout() {
    let mut diagnostics = DiagnosticReport::default();

    let terminal = desktop_terminal_after_runner(
        Err(AppRunError::runner("managed runtime close is incomplete")),
        RuntimeState::CloseIncomplete,
        &deadline_close_evidence(),
        None,
        &mut diagnostics,
    );

    assert!(matches!(terminal, PendingDesktopTerminal::Failed));
    assert!(has_code(&diagnostics, "project.desktop.shutdown-timeout"));
    assert!(has_code(&diagnostics, "project.desktop.runner-failed"));

    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    {
        let report = DesktopRun {
            state: DesktopRunState::Cleaning {
                hosts: Vec::new(),
                terminal,
            },
            diagnostics,
            startup_content_closure: None,
            presentation_verification: None,
            telemetry_seal: None,
        }
        .report();
        assert_eq!(report.outcome(), DesktopRunOutcome::CleanupIncomplete);
        assert!(has_code(
            report.diagnostics(),
            "project.desktop.shutdown-timeout"
        ));
        assert!(has_code(
            report.diagnostics(),
            "project.desktop.runner-failed"
        ));
    }
}

#[test]
fn participant_close_incomplete_is_not_misclassified_as_timeout() {
    let mut diagnostics = DiagnosticReport::default();

    let terminal = desktop_terminal_after_runner(
        Err(AppRunError::runner(
            "managed runtime close participant failed",
        )),
        RuntimeState::CloseIncomplete,
        &participant_close_evidence(),
        None,
        &mut diagnostics,
    );

    assert!(matches!(terminal, PendingDesktopTerminal::Failed));
    assert!(!has_code(&diagnostics, "project.desktop.shutdown-timeout"));
    assert!(has_code(&diagnostics, "project.desktop.runner-failed"));

    let report = DesktopRun {
        state: terminal.finish(),
        diagnostics,
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        startup_content_closure: None,
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        presentation_verification: None,
        telemetry_seal: None,
    }
    .report();
    assert_eq!(report.outcome(), DesktopRunOutcome::Failed);
    assert!(!has_code(
        report.diagnostics(),
        "project.desktop.shutdown-timeout"
    ));
    assert!(has_code(
        report.diagnostics(),
        "project.desktop.runner-failed"
    ));
}

#[test]
fn only_a_stopped_runtime_can_complete_the_desktop_product_action() {
    let mut stopped_diagnostics = DiagnosticReport::default();
    assert!(matches!(
        desktop_terminal_after_runner(
            Ok(AppExit::Success),
            RuntimeState::Stopped,
            &RuntimeCloseEvidence::default(),
            None,
            &mut stopped_diagnostics,
        ),
        PendingDesktopTerminal::Completed(AppExit::Success)
    ));
    assert!(stopped_diagnostics.is_empty());

    let mut running_diagnostics = DiagnosticReport::default();
    assert!(matches!(
        desktop_terminal_after_runner(
            Ok(AppExit::Success),
            RuntimeState::Running,
            &RuntimeCloseEvidence::default(),
            None,
            &mut running_diagnostics,
        ),
        PendingDesktopTerminal::Failed
    ));
    assert!(has_code(
        &running_diagnostics,
        "project.desktop.runtime-not-stopped"
    ));
}

#[test]
fn stopped_runtime_with_close_error_or_fault_is_not_reported_as_completed() {
    let mut close_error_diagnostics = DiagnosticReport::default();
    let terminal_evidence = terminal_close_evidence();
    assert!(matches!(
        desktop_terminal_after_runner(
            Ok(AppExit::Success),
            RuntimeState::Stopped,
            &terminal_evidence,
            None,
            &mut close_error_diagnostics,
        ),
        PendingDesktopTerminal::Failed
    ));
    assert!(has_code(
        &close_error_diagnostics,
        "project.desktop.cleanup-failed"
    ));

    let mut fault_diagnostics = DiagnosticReport::default();
    assert!(matches!(
        desktop_terminal_after_runner(
            Ok(AppExit::Success),
            RuntimeState::Stopped,
            &RuntimeCloseEvidence::default(),
            Some(&RuntimeFault::engine(
                RuntimeFaultKind::System,
                "nara.test.desktop-stopped-fault"
            )),
            &mut fault_diagnostics,
        ),
        PendingDesktopTerminal::Failed
    ));
    assert!(has_code(
        &fault_diagnostics,
        "project.desktop.runtime-faulted"
    ));
}

fn terminal_close_evidence() -> RuntimeCloseEvidence {
    let released = Arc::new(AtomicBool::new(false));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.desktop-terminal-evidence"),
            ControlledCloseParticipant {
                released,
                mode: CloseFailureMode::CompletedWithError,
            },
        )
        .unwrap();
    stopped_runtime(App::new(), obligations)
        .close_evidence()
        .clone()
}

fn stopped_runtime(app: App, obligations: RuntimeObligationLedger) -> RuntimeInstance {
    let mut runtime = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            obligations,
            RuntimeClosePolicy::default(),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    drive_until_state(&mut runtime, RuntimeState::Stopped);
    runtime
}

#[test]
fn stopped_desktop_fault_keeps_the_real_close_time_identity() {
    struct ShutdownFault;
    impl nara_app::Plugin for ShutdownFault {
        fn declaration() -> &'static nara_app::PluginDeclaration {
            static DECLARATION: nara_app::PluginDeclaration = nara_app::PluginDeclaration::new(
                nara_app::PluginId::new("nara.test.desktop-close-fault"),
                nara_app::PluginCategory::Runtime,
            );
            &DECLARATION
        }
        fn build(&self, _app: &mut App) -> Result<(), nara_app::PluginError> {
            Ok(())
        }
        fn shutdown(
            &self,
            context: &mut nara_app::PluginShutdownContext<'_>,
        ) -> Result<(), nara_app::PluginError> {
            context
                .world()
                .resource::<nara_app::RuntimeFaultReporter>()
                .report(nara_app::RuntimeFault::engine(
                    RuntimeFaultKind::RequiredService,
                    "nara.test.desktop-close-fault",
                ));
            Ok(())
        }
    }
    let mut app = App::new();
    app.add_plugin(ShutdownFault).unwrap();
    let runtime = stopped_runtime(app, RuntimeObligationLedger::new());
    assert!(runtime.close_evidence().is_empty());
    let mut diagnostics = DiagnosticReport::default();
    assert!(matches!(
        desktop_terminal_after_runner(
            Ok(AppExit::Success),
            runtime.state(),
            runtime.close_evidence(),
            runtime.fault(),
            &mut diagnostics
        ),
        PendingDesktopTerminal::Failed
    ));
    assert_eq!(
        diagnostic_field(
            &diagnostics,
            "project.desktop.runtime-faulted",
            "fault-source"
        )
        .as_deref(),
        Some("nara.test.desktop-close-fault")
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity()
                == nara_diagnostic::DiagnosticSeverity::Error)
            .count(),
        1
    );
}

#[test]
fn desktop_runner_failure_preserves_managed_runtime_fault_identity() {
    let managed =
        AppRunError::managed_runtime(RuntimeFaultKind::System, "nara.ecs.fallible-execution");
    let report = desktop_runner_failure_report(&managed);
    assert_eq!(
        diagnostic_field(&report, "project.desktop.runner-failed", "reason").as_deref(),
        Some("managed-runtime")
    );
    assert_eq!(
        diagnostic_field(&report, "project.desktop.runner-failed", "fault-kind").as_deref(),
        Some("system")
    );
    assert_eq!(
        diagnostic_field(&report, "project.desktop.runner-failed", "fault-source").as_deref(),
        Some("nara.ecs.fallible-execution")
    );

    let wrapped = AppRunError::runner_teardown(managed, AppRunError::runner("teardown"));
    let report = desktop_runner_failure_report(&wrapped);
    assert_eq!(
        diagnostic_field(&report, "project.desktop.runner-failed", "reason").as_deref(),
        Some("runner-teardown")
    );
    assert_eq!(
        diagnostic_field(&report, "project.desktop.runner-failed", "fault-kind").as_deref(),
        Some("system")
    );
}
