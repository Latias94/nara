use std::{
    cell::Cell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use nara_app::{
    App, CoreStage, RuntimeCloseCause, RuntimeCloseContext, RuntimeCloseErrorDisposition,
    RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
    RuntimeCloseParticipantPhase, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl,
    RuntimeControlFailure, RuntimeControlRequestResult, RuntimeControlStatus, RuntimeInstance,
    RuntimeObligationLedger, RuntimeState,
};
use nara_ecs::{Local, ResMut, Resource};

const PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.consumer.stop-retention-owner");
const BLOCKED: &str = "nara.consumer.stop-retention-blocked";

#[derive(Debug, Default)]
struct CloseState {
    released: Cell<bool>,
    calls: Cell<usize>,
    polls: Cell<usize>,
    closes: Cell<usize>,
    drops: Cell<usize>,
}

struct RetryableClose(Rc<CloseState>);

impl RuntimeCloseParticipant for RetryableClose {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.calls.set(self.0.calls.get() + 1);
        if !self.0.released.get() {
            return Err(RuntimeCloseParticipantError::retryable(BLOCKED));
        }
        self.0.closes.set(self.0.closes.get() + 1);
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.polls.set(self.0.polls.get() + 1);
        self.begin_close(context)
    }
}

impl Drop for RetryableClose {
    fn drop(&mut self) {
        self.0.drops.set(self.0.drops.get() + 1);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
struct Counter {
    frames: usize,
    local: usize,
}

fn start(effects: Arc<AtomicUsize>, stride: usize, close: Rc<CloseState>) -> RuntimeInstance {
    let mut app = App::new();
    app.init_resource::<Counter>().unwrap();
    app.add_systems(
        CoreStage::Update,
        move |mut counter: ResMut<Counter>, mut sequence: Local<usize>| {
            *sequence += 1;
            counter.frames += 1;
            counter.local = *sequence;
            effects.fetch_add(stride, Ordering::SeqCst);
        },
    )
    .unwrap();
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(PARTICIPANT, RetryableClose(close))
        .unwrap();
    RuntimeInstance::start_with_options(app, obligations, RuntimeClosePolicy::default()).unwrap()
}

fn status(
    runtime: &RuntimeInstance,
    request: RuntimeControlRequestResult,
) -> Option<RuntimeControlStatus> {
    match request {
        RuntimeControlRequestResult::Accepted(ticket) => runtime.control_status(ticket),
        RuntimeControlRequestResult::Rejected(_) => None,
    }
}

#[test]
fn original_stop_survives_retry_history_while_an_independent_runtime_keeps_its_systems() {
    let first_close = Rc::new(CloseState {
        released: Cell::new(true),
        ..CloseState::default()
    });
    let second_close = Rc::new(CloseState {
        released: Cell::new(true),
        ..CloseState::default()
    });
    let first_effects = Arc::new(AtomicUsize::new(0));
    let second_effects = Arc::new(AtomicUsize::new(0));
    let mut first = start(Arc::clone(&first_effects), 3, Rc::clone(&first_close));
    let mut second = start(Arc::clone(&second_effects), 7, Rc::clone(&second_close));
    let mut drives = vec![first.drive(Duration::ZERO), second.drive(Duration::ZERO)];
    first_close.released.set(false);
    let stop = first.request_control(RuntimeControl::Stop);
    let original_pending = status(&first, stop);
    let queued_stop = first.request_control(RuntimeControl::Stop);
    drives.push(first.drive(Duration::ZERO));
    drives.push(first.drive(Duration::ZERO));
    let original_result = status(&first, stop);
    let initial_evidence = first.close_evidence().clone();
    let mut pending = Vec::new();
    let mut repeated = Vec::new();
    let mut attempts = Vec::new();
    let mut survivor = Vec::new();
    let mut foreign = Vec::new();
    for attempt in 0..=48 {
        if attempt == 48 {
            first_close.released.set(true);
        }
        let retry = first.request_control(RuntimeControl::RetryClose);
        pending.push(status(&first, retry));
        drives.push(first.drive(Duration::ZERO));
        pending.push(status(&first, retry));
        let reissued = first.request_control(RuntimeControl::Stop);
        repeated.push((reissued, status(&first, reissued)));
        pending.push(status(&first, retry));
        foreign.extend([status(&second, retry), status(&second, reissued)]);
        drives.push(first.drive(Duration::ZERO));
        attempts.push((
            retry,
            first.state(),
            status(&first, retry),
            first.world().get_resource::<Counter>().copied(),
            (
                first_close.calls.get(),
                first_close.polls.get(),
                first_close.closes.get(),
                first_close.drops.get(),
            ),
        ));
        drives.push(second.drive(Duration::ZERO));
        survivor.push((
            second.state(),
            second.world().get_resource::<Counter>().copied(),
            second_effects.load(Ordering::SeqCst),
        ));
    }
    let history: Vec<_> = attempts
        .iter()
        .map(|(retry, ..)| status(&first, *retry))
        .collect();
    let final_stop = status(&first, stop);
    let independent_close = (
        second_close.calls.get(),
        second_close.polls.get(),
        second_close.closes.get(),
        second_close.drops.get(),
    );
    let second_stop = second.request_control(RuntimeControl::Stop);
    drives.push(second.drive(Duration::ZERO));
    drives.push(second.drive(Duration::ZERO));
    let second_result = (second.state(), status(&second, second_stop));
    let second_counter = second.world().get_resource::<Counter>().copied();
    foreign.extend([status(&second, stop), status(&first, second_stop)]);
    let first_finished = first.finish();
    let second_finished = second.finish();

    let first_evidence = first_finished.unwrap();
    let second_evidence = second_finished.unwrap();
    for drive in drives {
        drive.unwrap();
    }
    let RuntimeControlRequestResult::Accepted(stop_ticket) = stop else {
        panic!("original Stop was rejected: {stop:?}");
    };
    let RuntimeControlRequestResult::Accepted(second_ticket) = second_stop else {
        panic!("independent Stop was rejected: {second_stop:?}");
    };
    let incomplete = Some(RuntimeControlStatus::Failed(
        RuntimeControlFailure::CloseIncomplete,
    ));
    assert_eq!(original_pending, Some(RuntimeControlStatus::Pending));
    assert_eq!(queued_stop, stop);
    assert_eq!(original_result, incomplete);
    assert_eq!(final_stop, incomplete);
    assert_ne!(stop_ticket.generation(), second_ticket.generation());
    assert_eq!(stop_ticket.get(), second_ticket.get());
    for observed in pending {
        assert_eq!(observed, Some(RuntimeControlStatus::Pending));
    }
    for (request, observed) in repeated {
        assert_eq!(request, stop);
        assert_eq!(observed, incomplete);
    }
    for (index, (retry, state, observed, counter, close)) in attempts.iter().enumerate() {
        let RuntimeControlRequestResult::Accepted(ticket) = retry else {
            panic!("RetryClose {index} was rejected: {retry:?}");
        };
        assert_ne!(*ticket, stop_ticket);
        let complete = index == 48;
        assert_eq!(
            *state,
            if complete {
                RuntimeState::Stopped
            } else {
                RuntimeState::CloseIncomplete
            }
        );
        assert_eq!(
            *observed,
            if complete {
                Some(RuntimeControlStatus::Applied)
            } else {
                incomplete
            }
        );
        assert_eq!(
            *counter,
            Some(Counter {
                frames: 1,
                local: 1
            })
        );
        assert_eq!(*close, (index + 2, 0, usize::from(complete), 0));
        let retained = if index < 17 { None } else { *observed };
        assert_eq!(
            history[index], retained,
            "ordinary history at retry {index}"
        );
    }
    for (index, (state, counter, effects)) in survivor.into_iter().enumerate() {
        assert_eq!(state, RuntimeState::Running);
        assert_eq!(
            counter,
            Some(Counter {
                frames: index + 2,
                local: index + 2
            })
        );
        assert_eq!(effects, 7 * (index + 2));
    }
    assert!(foreign.into_iter().all(|observed| observed.is_none()));
    assert_eq!(independent_close, (0, 0, 0, 0));
    assert_eq!(
        second_result,
        (RuntimeState::Stopped, Some(RuntimeControlStatus::Applied))
    );
    assert_eq!(
        second_counter,
        Some(Counter {
            frames: 50,
            local: 50
        })
    );
    assert_eq!(first_effects.load(Ordering::SeqCst), 3);
    assert_eq!(second_effects.load(Ordering::SeqCst), 350);
    assert_eq!(
        (
            first_close.calls.get(),
            first_close.closes.get(),
            first_close.drops.get()
        ),
        (50, 1, 1)
    );
    assert_eq!(
        (
            second_close.calls.get(),
            second_close.polls.get(),
            second_close.closes.get(),
            second_close.drops.get()
        ),
        (1, 0, 1, 1)
    );
    assert_eq!(
        initial_evidence.causes(),
        &[RuntimeCloseCause::ParticipantError {
            participant: PARTICIPANT,
            phase: RuntimeCloseParticipantPhase::Begin,
            code: BLOCKED,
            disposition: RuntimeCloseErrorDisposition::Retryable,
        }]
    );
    assert_eq!(initial_evidence.pending_participants().len(), 1);
    assert_eq!(
        initial_evidence.pending_participants()[0].participant(),
        PARTICIPANT
    );
    assert_eq!(
        initial_evidence.pending_participants()[0].phase(),
        RuntimeCloseParticipantPhase::Begin
    );
    assert_eq!(first_evidence.causes(), initial_evidence.causes());
    assert!(first_evidence.pending_participants().is_empty());
    assert!(!first_evidence.has_terminal_failure());
    assert!(second_evidence.is_empty());
}
