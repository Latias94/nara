use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    thread::{self, ThreadId},
    time::Duration,
};

use nara_app::{
    App, PluginError, PluginShutdownObligationId, RuntimeCandidateRetirementState,
    RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeInstance,
    RuntimeObligationLedger, RuntimeObligationLedgerError,
};

const ID: RuntimeCloseParticipantId = RuntimeCloseParticipantId::new("r4.local-registration");

struct Probe {
    owner_thread: ThreadId,
    ready: Cell<bool>,
    begins: Cell<usize>,
    drops: RefCell<Vec<ThreadId>>,
}

impl Probe {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            owner_thread: thread::current().id(),
            ready: Cell::new(false),
            begins: Cell::new(0),
            drops: RefCell::new(Vec::new()),
        })
    }
}

struct LocalController(Rc<Probe>);

impl Drop for LocalController {
    fn drop(&mut self) {
        self.0.drops.borrow_mut().push(thread::current().id());
    }
}

impl RuntimeCloseParticipant for LocalController {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        assert_eq!(thread::current().id(), self.0.owner_thread);
        self.0.begins.set(self.0.begins.get() + 1);
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        assert_eq!(thread::current().id(), self.0.owner_thread);
        Ok(if self.0.ready.get() {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

fn start(app: App, ledger: RuntimeObligationLedger) -> RuntimeInstance {
    RuntimeInstance::start_with_options(app, ledger, RuntimeClosePolicy::new(Duration::ZERO))
        .unwrap()
}

#[test]
fn duplicate_standalone_registration_returns_the_exact_local_owner_without_callbacks() {
    let first_probe = Probe::new();
    let second_probe = Probe::new();
    let mut first_ledger = RuntimeObligationLedger::new();
    first_ledger
        .register(ID, LocalController(Rc::clone(&first_probe)))
        .unwrap();
    let rejected = first_ledger
        .register(ID, LocalController(Rc::clone(&second_probe)))
        .unwrap_err();
    let (error, controller) = rejected.into_parts();
    let exact_controller =
        Rc::ptr_eq(&controller.0, &second_probe) && !Rc::ptr_eq(&controller.0, &first_probe);
    let begins_before_transfer = [first_probe.begins.get(), second_probe.begins.get()];
    let drops_before_transfer = [
        first_probe.drops.borrow().len(),
        second_probe.drops.borrow().len(),
    ];

    let mut second_ledger = RuntimeObligationLedger::new();
    second_ledger.register(ID, controller).unwrap();
    let mut first = start(App::new(), first_ledger).finish().unwrap_err();
    let begins_after_first_start = [first_probe.begins.get(), second_probe.begins.get()];
    let mut second = start(App::new(), second_ledger).finish().unwrap_err();
    let states = [first.retirement_state(), second.retirement_state()];
    let drops_before_completion = [
        first_probe.drops.borrow().len(),
        second_probe.drops.borrow().len(),
    ];
    first_probe.ready.set(true);
    second_probe.ready.set(true);
    let completed = [first.drive_retirement(), second.drive_retirement()];
    drop((first, second));

    assert_eq!(error, RuntimeObligationLedgerError::Duplicate { id: ID });
    assert!(exact_controller);
    assert_eq!(begins_before_transfer, [0, 0]);
    assert_eq!(begins_after_first_start, [1, 0]);
    assert_eq!(drops_before_transfer, [0, 0]);
    assert_eq!(drops_before_completion, [0, 0]);
    assert_eq!(
        states,
        [RuntimeCandidateRetirementState::RetirementIncomplete; 2]
    );
    assert_eq!(completed, [RuntimeCandidateRetirementState::Retired; 2]);
    for probe in [first_probe, second_probe] {
        assert_eq!(probe.begins.get(), 1);
        assert_eq!(*probe.drops.borrow(), [probe.owner_thread]);
    }
}

#[test]
fn rejected_app_registration_retains_the_local_controller_until_physical_completion() {
    let probe = Probe::new();
    let mut app = App::new();
    let error = app
        .register_plugin_runtime_close_participant(
            PluginShutdownObligationId::new("r4.local-outside-build"),
            ID,
            LocalController(Rc::clone(&probe)),
        )
        .err()
        .expect("registration outside a plugin build rejects");
    let begins_before_start = probe.begins.get();
    let drops_before_start = probe.drops.borrow().len();
    let mut retirement = start(app, RuntimeObligationLedger::new())
        .finish()
        .unwrap_err();
    let pending = retirement.retirement_state();
    let drops_before_completion = probe.drops.borrow().len();
    probe.ready.set(true);
    let completed = retirement.drive_retirement();
    drop(retirement);

    assert_eq!(error, PluginError::ShutdownObligationOutsideBuild);
    assert_eq!(begins_before_start, 0);
    assert_eq!(drops_before_start, 0);
    assert_eq!(drops_before_completion, 0);
    assert_eq!(
        pending,
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert_eq!(completed, RuntimeCandidateRetirementState::Retired);
    assert_eq!(probe.begins.get(), 1);
    assert_eq!(*probe.drops.borrow(), [probe.owner_thread]);
}
