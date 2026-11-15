use std::{
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, ThreadId},
    time::Duration,
};

use nara_app::{
    App, CoreStage, ExecutionProfile, ExecutionProfileError, PluginError,
    RuntimeCandidateRetirementState, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeClosePolicy,
    RuntimeCloseProgress, RuntimeInstance, RuntimeObligationLedger, RuntimeStartError,
    StartupStage, runtime_quarantine_status,
};
use nara_ecs::NonSend;

#[derive(Debug, Clone, Default)]
pub(super) struct OwnerTrace {
    pub(super) startup: Vec<ThreadId>,
    pub(super) update: Vec<ThreadId>,
    pub(super) drops: Vec<ThreadId>,
}

struct ThreadBoundOwner {
    trace: Arc<Mutex<OwnerTrace>>,
    _thread_local: Rc<()>,
}

impl Drop for ThreadBoundOwner {
    fn drop(&mut self) {
        self.trace
            .lock()
            .unwrap()
            .drops
            .push(thread::current().id());
    }
}

fn observe_owner_startup(owner: NonSend<ThreadBoundOwner>) {
    owner
        .trace
        .lock()
        .unwrap()
        .startup
        .push(thread::current().id());
}

fn observe_owner_update(owner: NonSend<ThreadBoundOwner>) {
    owner
        .trace
        .lock()
        .unwrap()
        .update
        .push(thread::current().id());
}

pub(super) fn tracked_app() -> (App, Arc<Mutex<OwnerTrace>>) {
    let trace = Arc::new(Mutex::new(OwnerTrace::default()));
    let mut app = App::new();
    app.world_mut().unwrap().insert_non_send(ThreadBoundOwner {
        trace: Arc::clone(&trace),
        _thread_local: Rc::new(()),
    });
    app.add_systems(StartupStage::Core, observe_owner_startup)
        .unwrap();
    app.add_systems(CoreStage::Update, observe_owner_update)
        .unwrap();
    (app, trace)
}

pub(super) const CLOSE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.execution-profile.host-close");

#[derive(Clone, Default)]
pub(super) struct CloseGate {
    pub(super) released: Arc<AtomicBool>,
    pub(super) begins: Arc<AtomicUsize>,
    pub(super) polls: Arc<AtomicUsize>,
    pub(super) completions: Arc<Mutex<Vec<ThreadId>>>,
}

impl CloseGate {
    fn progress(&self) -> RuntimeCloseProgress {
        if self.released.load(Ordering::Acquire) {
            self.completions
                .lock()
                .unwrap()
                .push(thread::current().id());
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        }
    }

    pub(super) fn obligations(&self) -> RuntimeObligationLedger {
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(CLOSE_PARTICIPANT, self.clone())
            .unwrap();
        obligations
    }
}

impl RuntimeCloseParticipant for CloseGate {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begins.fetch_add(1, Ordering::Relaxed);
        Ok(self.progress())
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.polls.fetch_add(1, Ordering::Relaxed);
        Ok(self.progress())
    }
}

pub(super) fn assert_parallel_refused_before_startup(expected: ExecutionProfileError) {
    let driver = thread::current().id();
    let (mut app, seal_trace) = tracked_app();
    app.set_execution_profile(ExecutionProfile::Parallel)
        .unwrap();
    let seal_failure = app.seal().unwrap_err();
    assert_eq!(
        seal_failure.error(),
        &PluginError::ExecutionProfile(expected)
    );
    let trace = seal_trace.lock().unwrap();
    assert!(trace.startup.is_empty());
    assert!(trace.update.is_empty());
    assert_eq!(trace.drops, [driver]);
    drop(trace);

    let quarantine_before = runtime_quarantine_status().current_thread_retained();
    let gate = CloseGate::default();
    let (mut app, start_trace) = tracked_app();
    app.set_execution_profile(ExecutionProfile::Parallel)
        .unwrap();
    let mut failure = RuntimeInstance::start_with_options(
        app,
        gate.obligations(),
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap_err();
    let correct_error = matches!(
        failure.error(),
        RuntimeStartError::Preparation(PluginError::ExecutionProfile(error)) if error == &expected
    );
    let error = format!("{:?}", failure.error());
    let incomplete = failure.retirement_state();
    let pending = failure.close_evidence().pending_participants().to_vec();
    let before_release = start_trace.lock().unwrap().clone();
    gate.released.store(true, Ordering::Release);
    for _ in 0..4 {
        if failure.retirement_state() == RuntimeCandidateRetirementState::Retired {
            break;
        }
        failure.drive_retirement();
    }
    let retired = failure.retirement_state();
    let evidence = failure.close_evidence().clone();
    drop(failure);

    assert!(correct_error, "unexpected managed startup failure: {error}");
    assert_eq!(
        incomplete,
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].participant(), CLOSE_PARTICIPANT);
    assert!(before_release.startup.is_empty());
    assert!(before_release.update.is_empty());
    assert!(before_release.drops.is_empty());
    assert_eq!(retired, RuntimeCandidateRetirementState::Retired);
    assert!(evidence.pending_participants().is_empty());
    assert!(!evidence.has_terminal_failure());
    assert_eq!(gate.begins.load(Ordering::Relaxed), 1);
    assert!(gate.polls.load(Ordering::Relaxed) > 0);
    assert_eq!(*gate.completions.lock().unwrap(), [driver]);
    let trace = start_trace.lock().unwrap();
    assert!(trace.startup.is_empty());
    assert!(trace.update.is_empty());
    assert_eq!(trace.drops, [driver]);
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        quarantine_before
    );
}
