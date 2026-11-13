use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use nara_app::{
    App, RuntimeAdmissionReservation, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
};

use super::*;
use crate::project_host::{
    DesktopRunProgress,
    desktop_run::{DesktopRunState, PendingDesktopTerminal},
};

struct PendingClose {
    calls: Arc<AtomicUsize>,
    released: Arc<AtomicBool>,
}

impl RuntimeCloseParticipant for PendingClose {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(if self.released.load(Ordering::Acquire) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begin_close(context)
    }
}

#[test]
fn startup_cleanup_poll_retains_incomplete_budget_until_explicit_execute() {
    let released = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let policy = RuntimeClosePolicy::new(Duration::ZERO);
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.desktop-poll-cleanup"),
            PendingClose {
                calls: Arc::clone(&calls),
                released: Arc::clone(&released),
            },
        )
        .unwrap();
    let runtime = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(App::new().seal().unwrap(), obligations, policy)
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote();
    let owner = CleanupOwner::Candidate(runtime.begin_retirement());
    let mut host = ProjectHost::new(policy);
    host.slot = ProjectHostSlot::Cleaning {
        epoch: 1,
        owner: Box::new(owner),
        task_report_required: false,
    };
    let mut desktop = DesktopRun {
        state: DesktopRunState::Cleaning {
            hosts: vec![host],
            terminal: PendingDesktopTerminal::Failed,
        },
        diagnostics: DiagnosticReport::default(),
        startup_content_closure: None,
        presentation_verification: None,
        telemetry_seal: None,
        event_loop: None,
        close_requested: false,
    };
    assert_eq!(desktop.progress(), DesktopRunProgress::Retiring);
    assert_eq!(
        desktop.poll(Duration::ZERO),
        DesktopRunProgress::CleanupIncomplete
    );
    let before = calls.load(Ordering::Relaxed);
    released.store(true, Ordering::Release);
    for _ in 0..8 {
        assert_eq!(
            desktop.poll(Duration::ZERO),
            DesktopRunProgress::CleanupIncomplete
        );
        assert_eq!(calls.load(Ordering::Relaxed), before);
        assert!(desktop.take_event_loop().is_none());
    }
    assert_eq!(
        desktop.execute().outcome(),
        crate::project_host::DesktopRunOutcome::Failed
    );
    assert!(calls.load(Ordering::Relaxed) > before);
    assert_eq!(desktop.progress(), DesktopRunProgress::Failed);
}
