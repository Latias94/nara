//! Public renamed-root consumption of an owner-local dependent close aggregate.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    thread::{self, ThreadId},
    time::Duration,
};

use engine::app::{
    App, RuntimeCandidateRetirementState, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeClosePolicy,
    RuntimeCloseProgress, RuntimeInstance, RuntimeObligationLedger,
};

struct Observation {
    ready: Cell<bool>,
    provider_live: Cell<bool>,
    accepting: Cell<bool>,
    owner_thread: ThreadId,
    events: RefCell<Vec<&'static str>>,
}

// A tool/SDK adapter can supply this controller without knowing runtime ledger identities.
struct LocalController(Rc<Observation>);

impl LocalController {
    fn poll(&mut self) -> bool {
        assert_eq!(self.0.owner_thread, thread::current().id());
        assert!(!self.0.accepting.get());
        assert!(self.0.provider_live.get());
        self.0.ready.get()
    }
}

impl Drop for LocalController {
    fn drop(&mut self) {
        assert_eq!(self.0.owner_thread, thread::current().id());
        self.0.events.borrow_mut().push("controller.drop");
    }
}

struct Provider(Rc<Observation>);

impl Provider {
    fn release(&mut self) {
        assert!(self.0.ready.get());
        self.0.provider_live.set(false);
        self.0.events.borrow_mut().push("provider.release");
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        assert_eq!(self.0.owner_thread, thread::current().id());
        self.0.events.borrow_mut().push("provider.drop");
    }
}

struct SessionClose {
    controller: LocalController,
    provider: Provider,
    completed_error: bool,
}

impl RuntimeCloseParticipant for SessionClose {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.controller.0.accepting.set(false);
        self.controller.0.events.borrow_mut().push("quiesce");
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        if !self.controller.poll() {
            return Ok(RuntimeCloseProgress::Pending);
        }
        self.provider.release();
        Ok(if self.completed_error {
            RuntimeCloseProgress::CompleteWithError {
                code: "external.local-close.completed-error",
            }
        } else {
            RuntimeCloseProgress::Complete
        })
    }
}

#[test]
fn local_controller_closes_through_the_renamed_public_runtime_without_early_provider_release() {
    for completed_error in [false, true] {
        let observation = Rc::new(Observation {
            ready: Cell::new(false),
            provider_live: Cell::new(true),
            accepting: Cell::new(true),
            owner_thread: thread::current().id(),
            events: RefCell::new(Vec::new()),
        });
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("external.local-close"),
                SessionClose {
                    controller: LocalController(Rc::clone(&observation)),
                    provider: Provider(Rc::clone(&observation)),
                    completed_error,
                },
            )
            .unwrap();
        let mut retirement = RuntimeInstance::start_with_options(
            App::new(),
            obligations,
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap()
        .finish()
        .unwrap_err();
        let pending = retirement.retirement_state();
        let held = observation.provider_live.get();
        let before_completion = observation.events.borrow().clone();
        observation.ready.set(true);
        let completed = retirement.drive_retirement();
        let evidence = retirement.close_evidence().clone();
        drop(retirement);

        assert_eq!(
            pending,
            RuntimeCandidateRetirementState::RetirementIncomplete
        );
        assert!(held);
        assert_eq!(before_completion, ["quiesce"]);
        assert_eq!(completed, RuntimeCandidateRetirementState::Retired);
        assert_eq!(evidence.has_terminal_failure(), completed_error);
        assert!(evidence.pending_participants().is_empty());
        assert!(!observation.provider_live.get());
        assert_eq!(
            *observation.events.borrow(),
            [
                "quiesce",
                "provider.release",
                "controller.drop",
                "provider.drop"
            ]
        );
    }
}
