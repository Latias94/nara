//! Independent registrations currently declare no completion dependency.
//!
//! These tests characterize that contract; they do not assert a safety defect in a backend.
//! The resource is a safe, observable stand-in for a provider needed during consumer close.

use std::{
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use nara_app::{
    App, RuntimeCandidateRetirementState as RetirementState, RuntimeCloseCause,
    RuntimeCloseContext, RuntimeCloseErrorDisposition, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseParticipantPhase,
    RuntimeClosePolicy, RuntimeCloseProgress, RuntimeInstance, RuntimeObligationLedger,
    RuntimeRetirement,
};

const PROVIDER: RuntimeCloseParticipantId = RuntimeCloseParticipantId::new("r4.provider");
const CONSUMER: RuntimeCloseParticipantId = RuntimeCloseParticipantId::new("r4.consumer");
const CODE: &str = "r4.consumer.close-error";

#[derive(Clone, Copy)]
enum Mode {
    Pending,
    TerminalIncomplete,
    CompleteWithError,
}

#[derive(Default)]
struct State {
    resource: Option<Vec<u8>>,
    events: Vec<&'static str>,
    ready: bool,
    consumer_complete: bool,
    consumer_drops: usize,
    released_early: bool,
}

type Shared = Arc<Mutex<State>>;

struct Provider(Shared);
struct Consumer(Shared, Mode);

impl RuntimeCloseParticipant for Provider {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let mut state = self.0.lock().unwrap();
        state
            .events
            .extend(["provider.quiesce", "provider.release"]);
        state.released_early = !state.consumer_complete;
        drop(state.resource.take());
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        unreachable!("provider completes in begin_close")
    }
}

impl RuntimeCloseParticipant for Consumer {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.lock().unwrap().events.push("consumer.quiesce");
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let mut state = self.0.lock().unwrap();
        let event = if state.resource.as_ref().map(Vec::len) == Some(3) {
            "consumer.poll.live"
        } else {
            "consumer.poll.released"
        };
        state.events.push(event);
        match self.1 {
            Mode::Pending if !state.ready => Ok(RuntimeCloseProgress::Pending),
            Mode::TerminalIncomplete => Err(RuntimeCloseParticipantError::terminal(CODE)),
            Mode::Pending => {
                state.consumer_complete = true;
                Ok(RuntimeCloseProgress::Complete)
            }
            Mode::CompleteWithError => {
                state.consumer_complete = true;
                Ok(RuntimeCloseProgress::CompleteWithError { code: CODE })
            }
        }
    }
}

impl Drop for Consumer {
    fn drop(&mut self) {
        self.0.lock().unwrap().consumer_drops += 1;
    }
}

fn observe(mode: Mode) -> (RuntimeRetirement, Shared) {
    let state = Arc::new(Mutex::new(State {
        resource: Some(vec![1, 2, 3]),
        ..State::default()
    }));
    let mut ledger = RuntimeObligationLedger::new();
    // Reverse callbacks visit the consumer first; no dependency is declared by this API.
    ledger
        .register(PROVIDER, Provider(Arc::clone(&state)))
        .unwrap();
    ledger
        .register(CONSUMER, Consumer(Arc::clone(&state), mode))
        .unwrap();
    let runtime = RuntimeInstance::start_with_options(
        App::new(),
        ledger,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap();
    (runtime.finish().unwrap_err(), state)
}

fn assert_unfinished(retirement: &RuntimeRetirement) {
    assert_eq!(
        retirement.retirement_state(),
        RetirementState::RetirementIncomplete
    );
    assert!(
        retirement
            .close_evidence()
            .pending_participants()
            .iter()
            .any(|pending| pending.participant() == CONSUMER
                && pending.phase() == RuntimeCloseParticipantPhase::Poll)
    );
}

fn assert_error(retirement: &RuntimeRetirement) {
    assert!(retirement.close_evidence().has_terminal_failure());
    assert!(
        retirement
            .close_evidence()
            .causes()
            .contains(&RuntimeCloseCause::ParticipantError {
                participant: CONSUMER,
                phase: RuntimeCloseParticipantPhase::Poll,
                code: CODE,
                disposition: RuntimeCloseErrorDisposition::Terminal,
            })
    );
}

fn assert_current_order(state: &State) {
    assert_eq!(
        state.events,
        [
            "consumer.quiesce",
            "consumer.poll.live",
            "provider.quiesce",
            "provider.release"
        ]
    );
    assert!(state.resource.is_none());
    assert_eq!(state.consumer_drops, 0);
}

#[test]
fn independent_pending_participant_does_not_gate_another_participants_release() {
    let (mut retirement, state) = observe(Mode::Pending);
    let before_retry = retirement.retirement_state();
    let first_events = state.lock().unwrap().events.clone();
    let released_early = state.lock().unwrap().released_early;
    retirement.drive_retirement();
    let last_event = state.lock().unwrap().events.last().copied();
    // Drain before assertions, so a failing assertion cannot strand an unfinished owner.
    state.lock().unwrap().ready = true;
    let after_retry = retirement.drive_retirement();
    let no_pending = retirement
        .close_evidence()
        .pending_participants()
        .is_empty();
    let terminal_error = retirement.close_evidence().has_terminal_failure();
    drop(retirement);

    assert_eq!(before_retry, RetirementState::RetirementIncomplete);
    assert_eq!(
        first_events,
        [
            "consumer.quiesce",
            "consumer.poll.live",
            "provider.quiesce",
            "provider.release"
        ]
    );
    assert!(released_early);
    assert_eq!(last_event, Some("consumer.poll.released"));
    assert_eq!(after_retry, RetirementState::Retired);
    assert!(no_pending);
    assert!(!terminal_error);
    assert_eq!(state.lock().unwrap().consumer_drops, 1);
}

#[test]
fn complete_with_error_allows_release_and_retains_error() {
    let (retirement, state) = observe(Mode::CompleteWithError);
    assert_eq!(retirement.retirement_state(), RetirementState::Retired);
    assert!(
        retirement
            .close_evidence()
            .pending_participants()
            .is_empty()
    );
    assert_error(&retirement);
    {
        let state = state.lock().unwrap();
        assert_current_order(&state);
        assert!(state.consumer_complete);
        assert!(!state.released_early);
    }
    drop(retirement);
    assert_eq!(state.lock().unwrap().consumer_drops, 1);
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

#[test]
fn terminal_incomplete_retains_custody_but_does_not_gate_an_independent_provider() {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "terminal_child", "--ignored", "--nocapture"])
        .env("NARA_R4_TERMINAL_CHILD", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = ChildGuard(command.spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "terminal probe child timed out");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(status.success(), "terminal probe failed: {status}");
}

#[test]
#[ignore = "disposable child for physically incomplete terminal custody"]
fn terminal_child() {
    assert_eq!(std::env::var("NARA_R4_TERMINAL_CHILD").as_deref(), Ok("1"));
    let (mut retirement, state) = observe(Mode::TerminalIncomplete);
    assert_unfinished(&retirement);
    assert_error(&retirement);
    let events = {
        let state = state.lock().unwrap();
        assert_current_order(&state);
        assert!(state.released_early);
        assert!(!state.consumer_complete);
        state.events.clone()
    };
    state.lock().unwrap().ready = true;
    retirement.drive_retirement();
    assert_unfinished(&retirement);
    assert_error(&retirement);
    {
        let state = state.lock().unwrap();
        assert_eq!(state.events, events);
        assert_eq!(state.consumer_drops, 0);
    }
    // No destructor can drain terminal-incomplete custody. Exit only this disposable process.
    // This observes retained responsibility; it is not evidence of physical cleanup.
    std::process::exit(0);
}
