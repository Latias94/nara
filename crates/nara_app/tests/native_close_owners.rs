//! Bounded close-owner comparison: one independently supplied consumer and its local provider.
//! These are safe resource models, not qualification of a native SDK or a public graph API.

use std::{
    cell::{RefCell, RefMut},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    thread::{self, ThreadId},
    time::Duration,
};

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    RuntimeCandidateRetirementState, RuntimeCloseCause, RuntimeCloseContext,
    RuntimeCloseErrorDisposition, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeInstance,
    RuntimeObligationLedger, RuntimeStartError, RuntimeStartFailure, StartupStage,
    drive_runtime_quarantine, runtime_quarantine_status,
};
use nara_ecs::bevy_ecs::error::BevyError;

type Step = Result<RuntimeCloseProgress, RuntimeCloseParticipantError>;
// Public evidence has one aggregate code; the private report retains exact node error identity.
const AGGREGATE_ERROR: &str = "nara.test.native-aggregate.completed-error";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Id {
    Consumer,
    Provider,
    Missing,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reply {
    Pending,
    Retryable,
    Complete,
    CompleteWithError,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Fact {
    node: Id,
    error: RuntimeCloseParticipantError,
    physically_complete: bool,
}

#[derive(Default)]
struct Report(Vec<Fact>);

impl Report {
    fn observe(&mut self, node: Id, step: Step) {
        let (error, physically_complete) = match step {
            Err(error) => (error, false),
            Ok(RuntimeCloseProgress::CompleteWithError { code }) => {
                (RuntimeCloseParticipantError::terminal(code), true)
            }
            _ => return,
        };
        let fact = Fact {
            node,
            error,
            physically_complete,
        };
        if !self.0.contains(&fact) {
            self.0.push(fact);
        }
    }

    fn completion(&self) -> Step {
        Ok(if self.0.iter().any(|fact| fact.physically_complete) {
            RuntimeCloseProgress::CompleteWithError {
                code: AGGREGATE_ERROR,
            }
        } else {
            RuntimeCloseProgress::Complete
        })
    }
}

struct State {
    owner: ThreadId,
    reply: Reply,
    provider_storage: Option<Box<u8>>,
    consumer_live: bool,
    quiet: [bool; 2],
    events: Vec<&'static str>,
}

#[derive(Clone)]
struct Control {
    state: Rc<RefCell<State>>,
    report: Rc<RefCell<Report>>,
}

impl Control {
    fn new(reply: Reply) -> Self {
        Self {
            state: Rc::new(RefCell::new(State {
                owner: thread::current().id(),
                reply,
                provider_storage: Some(Box::new(7)),
                consumer_live: true,
                quiet: [false; 2],
                events: Vec::new(),
            })),
            report: Rc::new(RefCell::new(Report::default())),
        }
    }

    fn local(&self) -> RefMut<'_, State> {
        let state = self.state.borrow_mut();
        assert_eq!(
            state.owner,
            thread::current().id(),
            "owner-thread callback/drop"
        );
        state
    }
}

// Private and object safe; only the two-node sealed prototype erases concrete ownership.
trait Node {
    fn quiesce(&mut self);
    fn poll_release(&mut self) -> Step;
}

mod supplied {
    use super::*;

    // An independent concrete consumer receives a resource capability, not a ledger or IDs.
    pub(super) struct Consumer {
        control: Control,
        pub(super) complete: bool,
    }

    impl Consumer {
        pub(super) fn new(control: Control) -> Self {
            Self {
                control,
                complete: false,
            }
        }
    }

    impl Node for Consumer {
        fn quiesce(&mut self) {
            let mut state = self.control.local();
            state.quiet[0] = true;
            state.events.push("consumer.quiesce");
        }

        fn poll_release(&mut self) -> Step {
            let mut state = self.control.local();
            assert_eq!(
                state.quiet, [true; 2],
                "quiesce all before destructive polling"
            );
            assert_eq!(
                state.provider_storage.as_deref(),
                Some(&7),
                "closing consumer reads provider storage"
            );
            state.events.push("consumer.poll");
            let step = match state.reply {
                Reply::Pending => Ok(RuntimeCloseProgress::Pending),
                Reply::Retryable => Err(RuntimeCloseParticipantError::retryable("consumer.retry")),
                Reply::Terminal => Err(RuntimeCloseParticipantError::terminal("consumer.terminal")),
                Reply::Complete => Ok(RuntimeCloseProgress::Complete),
                Reply::CompleteWithError => Ok(RuntimeCloseProgress::CompleteWithError {
                    code: "consumer.completed-error",
                }),
            };
            if matches!(
                step,
                Ok(RuntimeCloseProgress::Complete | RuntimeCloseProgress::CompleteWithError { .. })
            ) {
                self.complete = true;
                state.consumer_live = false;
                state.events.push("consumer.release");
            }
            step
        }
    }

    impl Drop for Consumer {
        fn drop(&mut self) {
            let mut state = self.control.local();
            // This finite model owns no worker: dropping its capability revokes all future use.
            // Direct terminal comparisons use this fallback; Runtime tests must finish polling.
            state.consumer_live = false;
            state.events.push("consumer.drop");
        }
    }
}

struct Provider {
    control: Control,
    complete: bool,
}

impl Node for Provider {
    fn quiesce(&mut self) {
        let mut state = self.control.local();
        state.quiet[1] = true;
        state.events.push("provider.quiesce");
    }

    fn poll_release(&mut self) -> Step {
        let mut state = self.control.local();
        assert_eq!(state.quiet, [true; 2]);
        assert!(
            !state.consumer_live,
            "provider release requires consumer physical completion"
        );
        state.provider_storage = None;
        state.events.push("provider.release");
        self.complete = true;
        Ok(RuntimeCloseProgress::Complete)
    }
}

impl Drop for Provider {
    fn drop(&mut self) {
        let mut state = self.control.local();
        assert!(
            !state.consumer_live,
            "drop consumer custody before provider custody"
        );
        state.provider_storage = None;
        state.events.push("provider.drop");
    }
}

struct TypedOwner {
    // Field order also makes direct, synchronous model disposal consumer-first.
    consumer: supplied::Consumer,
    provider: Provider,
    report: Rc<RefCell<Report>>,
    terminal: Option<RuntimeCloseParticipantError>,
    begun: bool,
}

impl TypedOwner {
    fn new(consumer: supplied::Consumer, provider: Provider) -> Self {
        let report = provider.control.report.clone();
        Self {
            consumer,
            provider,
            report,
            terminal: None,
            begun: false,
        }
    }

    fn begin(&mut self) {
        if !self.begun {
            self.consumer.quiesce();
            self.provider.quiesce();
            self.begun = true;
        }
    }

    fn poll(&mut self) -> Step {
        if let Some(error) = self.terminal {
            return Err(error);
        }
        if !self.consumer.complete {
            let step = self.consumer.poll_release();
            self.report.borrow_mut().observe(Id::Consumer, step);
            match step {
                Err(error) => {
                    if error.disposition() == RuntimeCloseErrorDisposition::Terminal {
                        self.terminal = Some(error);
                    }
                    return Err(error);
                }
                Ok(RuntimeCloseProgress::Pending) => return step,
                _ => {}
            }
        }
        if !self.provider.complete {
            self.provider.poll_release()?;
        }
        self.report.borrow().completion()
    }
}

fn typed(control: &Control) -> TypedOwner {
    let consumer = supplied::Consumer::new(control.clone());
    let provider = Provider {
        control: control.clone(),
        complete: false,
    };
    TypedOwner::new(consumer, provider)
}

struct Entry {
    id: Id,
    prerequisite: Option<Id>,
    node: Box<dyn Node>,
    complete: bool,
    terminal: Option<RuntimeCloseParticipantError>,
}

fn entries(control: &Control, reverse: bool) -> [Entry; 2] {
    let mut entries = [
        Entry {
            id: Id::Consumer,
            prerequisite: None,
            node: Box::new(supplied::Consumer::new(control.clone())),
            complete: false,
            terminal: None,
        },
        Entry {
            id: Id::Provider,
            prerequisite: Some(Id::Consumer),
            node: Box::new(Provider {
                control: control.clone(),
                complete: false,
            }),
            complete: false,
            terminal: None,
        },
    ];
    if reverse {
        entries.swap(0, 1);
    }
    entries
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Invalid {
    Duplicate,
    Missing,
    Cycle,
}

struct Rejected {
    reason: Invalid,
    entries: [Entry; 2],
}

struct Sealed {
    entries: [Entry; 2],
    report: Rc<RefCell<Report>>,
    begun: bool,
}

impl Sealed {
    fn seal(mut entries: [Entry; 2], report: Rc<RefCell<Report>>) -> Result<Self, Rejected> {
        // Exactly two private nodes: no inferred edges, generic graph, or registration service.
        let reason = if entries[0].id == entries[1].id {
            Some(Invalid::Duplicate)
        } else if entries.iter().any(|entry| {
            entry
                .prerequisite
                .is_some_and(|id| !entries.iter().any(|other| other.id == id))
        }) {
            Some(Invalid::Missing)
        } else if entries
            .iter()
            .any(|entry| entry.prerequisite == Some(entry.id))
            || (entries[0].prerequisite.is_some() && entries[1].prerequisite.is_some())
        {
            Some(Invalid::Cycle)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(Rejected { reason, entries });
        }
        // Canonical dependency order also gives the array consumer-first Drop order,
        // including when the caller declared the provider first.
        if entries[0].prerequisite.is_some() {
            entries.swap(0, 1);
        }
        Ok(Self {
            entries,
            report,
            begun: false,
        })
    }

    fn begin(&mut self) {
        if !self.begun {
            for entry in &mut self.entries {
                entry.node.quiesce();
            }
            self.begun = true;
        }
    }

    fn poll(&mut self) -> Step {
        for index in 0..2 {
            if self.entries[index].complete {
                continue;
            }
            if let Some(error) = self.entries[index].terminal {
                return Err(error);
            }
            if let Some(id) = self.entries[index].prerequisite
                && !self
                    .entries
                    .iter()
                    .any(|entry| entry.id == id && entry.complete)
            {
                continue;
            }
            let entry = &mut self.entries[index];
            let step = entry.node.poll_release();
            self.report.borrow_mut().observe(entry.id, step);
            match step {
                Err(error) => {
                    if error.disposition() == RuntimeCloseErrorDisposition::Terminal {
                        entry.terminal = Some(error);
                    }
                    return Err(error);
                }
                Ok(RuntimeCloseProgress::Pending) => {}
                _ => entry.complete = true,
            }
        }
        if self.entries.iter().all(|entry| entry.complete) {
            self.report.borrow().completion()
        } else {
            Ok(RuntimeCloseProgress::Pending)
        }
    }
}

#[test]
fn typed_and_sealed_gate_the_same_resource_and_retain_exact_errors() {
    for reply in [
        Reply::Pending,
        Reply::Retryable,
        Reply::Complete,
        Reply::CompleteWithError,
        Reply::Terminal,
    ] {
        for reverse in [false, true] {
            let a = Control::new(reply);
            let b = Control::new(reply);
            let mut typed = typed(&a);
            let mut sealed = Sealed::seal(entries(&b, reverse), b.report.clone())
                .unwrap_or_else(|_| panic!("valid two-node declarations"));
            typed.begin();
            sealed.begin();
            let first = typed.poll();
            let other = sealed.poll();
            let first_state = {
                let state = a.local();
                (state.provider_storage.is_some(), state.events.clone())
            };
            let other_state = {
                let state = b.local();
                (state.provider_storage.is_some(), state.events.clone())
            };
            let terminal_repeat = (typed.poll(), sealed.poll());
            let events_after_repeat = (a.local().events.clone(), b.local().events.clone());
            let resumed = if reply != Reply::Terminal {
                a.local().reply = Reply::Complete;
                b.local().reply = Reply::Complete;
                (typed.poll(), sealed.poll())
            } else {
                terminal_repeat
            };
            let a_live = a.local().provider_storage.is_some();
            let b_live = b.local().provider_storage.is_some();
            drop(typed);
            drop(sealed);

            assert_eq!(first, other);
            assert_eq!(first_state, other_state);
            assert_eq!(
                first_state.0,
                matches!(reply, Reply::Pending | Reply::Retryable | Reply::Terminal)
            );
            assert_eq!(
                (a_live, b_live),
                (reply == Reply::Terminal, reply == Reply::Terminal)
            );
            assert_eq!(resumed.0, resumed.1);
            if reply != Reply::Terminal {
                assert!(matches!(
                    resumed.0,
                    Ok(RuntimeCloseProgress::Complete
                        | RuntimeCloseProgress::CompleteWithError { .. })
                ));
            }
            assert_eq!(a.report.borrow().0, b.report.borrow().0);
            let facts = a.report.borrow();
            match reply {
                Reply::Pending | Reply::Complete => assert!(facts.0.is_empty()),
                _ => {
                    let expected = match reply {
                        Reply::Retryable => ("consumer.retry", false),
                        Reply::Terminal => ("consumer.terminal", false),
                        _ => ("consumer.completed-error", true),
                    };
                    assert_eq!(facts.0.len(), 1);
                    assert_eq!(facts.0[0].node, Id::Consumer);
                    assert_eq!(
                        (facts.0[0].error.code(), facts.0[0].physically_complete),
                        expected
                    );
                }
            }
            if reply == Reply::Terminal {
                assert_eq!(terminal_repeat, (first, other));
                assert_eq!(events_after_repeat, (first_state.1, other_state.1));
            }
        }
    }
}

#[test]
fn sealing_rejects_missing_and_cyclic_prerequisites_without_quiescing_or_losing_custody() {
    for reason in [Invalid::Missing, Invalid::Cycle] {
        let control = Control::new(Reply::Complete);
        let mut declarations = entries(&control, false);
        declarations[0].prerequisite = Some(if reason == Invalid::Missing {
            Id::Missing
        } else {
            Id::Provider
        });
        let mut rejected = match Sealed::seal(declarations, control.report.clone()) {
            Err(rejected) => rejected,
            Ok(_) => panic!("invalid prerequisites admitted"),
        };
        let observed = (
            rejected.reason,
            control.local().events.clone(),
            Rc::strong_count(&control.state),
        );
        let held = {
            let state = control.local();
            state.provider_storage.is_some() && state.consumer_live
        };
        let ids = [rejected.entries[0].id, rejected.entries[1].id];
        rejected.entries[0].prerequisite = None;
        let mut recovered = Sealed::seal(rejected.entries, control.report.clone())
            .unwrap_or_else(|_| panic!("repaired declarations"));
        recovered.begin();
        let completed = recovered.poll();
        drop(recovered);

        assert_eq!(observed, (reason, Vec::new(), 3));
        assert!(held);
        assert_eq!(ids, [Id::Consumer, Id::Provider]);
        assert_eq!(completed, Ok(RuntimeCloseProgress::Complete));
    }
}

struct LocalClose(TypedOwner);

impl RuntimeCloseParticipant for LocalClose {
    fn begin_close(&mut self, _: &mut RuntimeCloseContext<'_>) -> Step {
        self.0.begin();
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(&mut self, _: &mut RuntimeCloseContext<'_>) -> Step {
        self.0.poll()
    }
}

fn start(control: &Control, app: App) -> Result<RuntimeInstance, RuntimeStartFailure> {
    let mut ledger = RuntimeObligationLedger::new();
    ledger
        .register(
            RuntimeCloseParticipantId::new("nara.test.local-native-owner"),
            LocalClose(typed(control)),
        )
        .unwrap();
    RuntimeInstance::start_with_options(app, ledger, RuntimeClosePolicy::new(Duration::ZERO))
}

fn assert_released(control: &Control) {
    let state = control.local();
    assert!(state.provider_storage.is_none() && !state.consumer_live);
    assert_eq!(
        state.events,
        [
            "consumer.quiesce",
            "provider.quiesce",
            "consumer.poll",
            "consumer.release",
            "provider.release",
            "consumer.drop",
            "provider.drop"
        ]
    );
}

#[test]
fn incomplete_finish_retains_local_aggregate_until_explicit_retry() {
    for reply in [Reply::Pending, Reply::Retryable] {
        let control = Control::new(reply);
        let mut retirement = start(&control, App::new()).unwrap().finish().unwrap_err();
        let before = (
            retirement.retirement_state(),
            control.local().provider_storage.is_some(),
        );
        let evidence = retirement.close_evidence().clone();
        control.local().reply = Reply::Complete;
        let after = retirement.drive_retirement();
        let retained_evidence = retirement.close_evidence().clone();
        drop(retirement);
        let drained = drive_runtime_quarantine();

        assert_eq!(
            before,
            (RuntimeCandidateRetirementState::RetirementIncomplete, true)
        );
        assert_eq!(after, RuntimeCandidateRetirementState::Retired);
        assert_eq!(evidence.pending_participants().len(), 1);
        assert!(retained_evidence.pending_participants().is_empty());
        assert!(
            evidence
                .causes()
                .iter()
                .all(|cause| retained_evidence.causes().contains(cause))
        );
        assert_eq!(drained.current_thread_retained(), 0);
        assert!(control.local().provider_storage.is_none());
        assert_eq!(
            control
                .local()
                .events
                .iter()
                .filter(|event| **event == "consumer.quiesce")
                .count(),
            1
        );
        if reply == Reply::Retryable {
            assert_eq!(control.report.borrow().0[0].error.code(), "consumer.retry");
        }
    }
}

#[test]
fn normal_drop_completes_the_local_owner_on_its_constructing_thread() {
    let control = Control::new(Reply::Complete);
    drop(start(&control, App::new()).unwrap());
    assert_released(&control);
}

#[test]
fn completed_error_releases_provider_and_keeps_aggregate_and_exact_error_evidence() {
    let control = Control::new(Reply::CompleteWithError);
    let retirement = start(&control, App::new()).unwrap().finish().unwrap_err();
    let state = retirement.retirement_state();
    let evidence = retirement.close_evidence().clone();
    drop(retirement);

    assert_eq!(state, RuntimeCandidateRetirementState::Retired);
    assert!(evidence.pending_participants().is_empty());
    assert!(evidence.causes().iter().any(|cause| matches!(
        cause,
        RuntimeCloseCause::ParticipantError {
            code: AGGREGATE_ERROR,
            disposition: RuntimeCloseErrorDisposition::Terminal,
            ..
        }
    )));
    assert_eq!(
        control.report.borrow().0[0].error.code(),
        "consumer.completed-error"
    );
    assert_released(&control);
}

#[test]
fn incomplete_retirement_and_unwinding_retain_local_custody_until_quarantine_drain() {
    for unwind in [false, true] {
        let baseline = runtime_quarantine_status().current_thread_retained();
        let control = Control::new(Reply::Pending);
        let runtime = start(&control, App::new()).unwrap();
        let caught = if unwind {
            catch_unwind(AssertUnwindSafe(move || {
                let _runtime = runtime;
                panic!("injected owner unwind");
            }))
            .is_err()
        } else {
            drop(runtime.finish().unwrap_err());
            false
        };
        let retained = runtime_quarantine_status().current_thread_retained();
        let live = control.local().provider_storage.is_some();
        let dropped = control.local().events.contains(&"consumer.drop");
        let before_drain = control.local().events.clone();
        control.local().reply = Reply::Complete;
        let drained = drive_runtime_quarantine();
        let after_drain = control.local().events.clone();

        assert_eq!(caught, unwind);
        assert_eq!(retained, baseline + 1);
        assert!(live && !dropped);
        assert_eq!(drained.current_thread_retained(), baseline);
        assert!(control.local().provider_storage.is_none());
        let mut expected = if unwind {
            assert!(
                before_drain.is_empty(),
                "unwind must not invoke close callbacks"
            );
            vec!["consumer.quiesce", "provider.quiesce"]
        } else {
            assert!(before_drain.starts_with(&["consumer.quiesce", "provider.quiesce"]));
            before_drain
        };
        expected.extend([
            "consumer.poll",
            "consumer.release",
            "provider.release",
            "consumer.drop",
            "provider.drop",
        ]);
        assert_eq!(after_drain, expected);
    }
}

struct FailStartup;

impl Plugin for FailStartup {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.local-native-startup-failure"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Runtime, || -> Result<(), BevyError> {
            Err(BevyError::error(std::io::Error::other(
                "injected fallible Startup",
            )))
        })?;
        Ok(())
    }
}

#[test]
fn fallible_startup_retains_local_aggregate_and_provider_until_retry() {
    let control = Control::new(Reply::Pending);
    let mut app = App::new();
    app.add_plugin(FailStartup).unwrap();
    let mut failure = start(&control, app).unwrap_err();
    let startup = matches!(failure.error(), RuntimeStartError::Startup(_));
    let before = (
        failure.retirement_state(),
        control.local().provider_storage.is_some(),
    );
    let drops_before = control.local().events.contains(&"consumer.drop");
    control.local().reply = Reply::Complete;
    let after = failure.drive_retirement();
    drop(failure);
    let drained = drive_runtime_quarantine();

    assert!(startup);
    assert_eq!(
        before,
        (RuntimeCandidateRetirementState::RetirementIncomplete, true)
    );
    assert!(!drops_before);
    assert_eq!(after, RuntimeCandidateRetirementState::Retired);
    assert_eq!(drained.current_thread_retained(), 0);
    assert!(control.local().provider_storage.is_none());
    assert!(
        control
            .local()
            .events
            .ends_with(&["consumer.drop", "provider.drop"])
    );
}
