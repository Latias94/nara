#![forbid(unsafe_code)]

#[path = "support/headless_session/mod.rs"]
mod support;

use std::{cell::Cell, sync::atomic::Ordering, time::Duration};

use engine::{
    app::{AppExit, RuntimeControlRejection, RuntimeState},
    gameplay::{GameplayCommandQueue, GameplayCommandRejection},
    project_host::{HeadlessSession, HeadlessSessionClose, HeadlessSessionError},
};

#[test]
fn source_session_retains_one_runtime_through_drive_capture_steps_and_resume() {
    let mut session = support::session();
    let captured = Cell::new(false);
    assert!(session.state().is_none());
    assert!(session.runtime_generation().is_none());
    assert!(session.startup_content_closure().is_none());
    assert!(session.capture(|_| captured.set(true)).is_err());
    assert!(!captured.get());

    session.start().unwrap();
    assert_eq!(session.state(), Some(RuntimeState::Running));
    let generation = session.runtime_generation().unwrap();
    let initial = session.observe(support::snapshot).unwrap();
    assert_eq!(initial.generation, generation);
    assert_eq!(initial.tick, 0);
    assert_eq!(initial.frame, 0);
    assert!(matches!(
        session.start(),
        Err(HeadlessSessionError::Unavailable)
    ));
    let content_digest = session.startup_content_closure().unwrap().content_digest();
    assert!(
        !session
            .startup_content_closure()
            .unwrap()
            .members()
            .is_empty()
    );
    assert!(matches!(
        session.capture(|_| captured.set(true)),
        Err(HeadlessSessionError::NotPaused)
    ));
    assert!(!captured.get());

    assert_eq!(session.frame(support::FIXED_DELTA).unwrap(), None);
    assert_eq!(session.frame(support::FIXED_DELTA).unwrap(), None);
    let running = session.observe(support::snapshot).unwrap();
    assert_eq!(running.tick, 2);
    assert_eq!(running.elapsed, support::FIXED_DELTA * 2);
    assert_eq!(running.simulated_ticks, [1, 2]);
    assert_eq!(running.deferred_ticks, running.simulated_ticks);
    assert_eq!(running.last_regular_frame, running.frame);

    assert_eq!(session.pause().unwrap(), None);
    assert_eq!(session.state(), Some(RuntimeState::Paused));
    let paused = session.capture(support::snapshot).unwrap();
    assert_eq!(paused, session.observe(support::snapshot).unwrap());
    assert_eq!(paused.tick, running.tick);
    assert_eq!(paused.elapsed, running.elapsed);
    assert!(paused.frame > running.frame);
    assert_eq!(paused.last_regular_frame, paused.frame);

    for offset in 1..=3_u32 {
        assert_eq!(session.step().unwrap(), None);
        assert_eq!(session.state(), Some(RuntimeState::Paused));
        let stepped = session.capture(support::snapshot).unwrap();
        assert_eq!(stepped.generation, generation);
        assert_eq!(stepped.tick, paused.tick + u64::from(offset));
        assert_eq!(
            stepped.simulated_ticks,
            (1..=stepped.tick).collect::<Vec<_>>()
        );
        assert_eq!(stepped.deferred_ticks, stepped.simulated_ticks);
        assert_eq!(stepped.elapsed, support::FIXED_DELTA * (2 + offset));
        assert_eq!(stepped.last_regular_frame, stepped.frame);
        assert_eq!(stepped.last_observed_tick, stepped.tick);
        assert_eq!(stepped.task_update_ticks.last(), Some(&stepped.tick));
    }

    let stepped = session.capture(support::snapshot).unwrap();
    assert_eq!(session.frame(Duration::from_secs(1)).unwrap(), None);
    let maintained = session.capture(support::snapshot).unwrap();
    assert_eq!(maintained.tick, stepped.tick);
    assert_eq!(maintained.elapsed, stepped.elapsed);
    assert_eq!(maintained.simulated_ticks, stepped.simulated_ticks);
    assert_eq!(maintained.last_regular_frame, maintained.frame);

    assert_eq!(session.resume().unwrap(), None);
    assert_eq!(session.state(), Some(RuntimeState::Running));
    assert_eq!(
        session.observe(support::snapshot).unwrap().tick,
        stepped.tick
    );
    assert!(matches!(
        session.capture(|_| captured.set(true)),
        Err(HeadlessSessionError::NotPaused)
    ));
    assert!(!captured.get());
    assert_eq!(session.frame(support::FIXED_DELTA).unwrap(), None);
    let continued = session.observe(support::snapshot).unwrap();
    assert_eq!(continued.tick, stepped.tick + 1);
    assert_eq!(continued.simulated_ticks, [1, 2, 3, 4, 5, 6]);
    assert_eq!(continued.deferred_ticks, continued.simulated_ticks);
    assert_eq!(session.runtime_generation(), Some(generation));
    assert_eq!(
        session.startup_content_closure().unwrap().content_digest(),
        content_digest
    );
    assert!(!session.diagnostics().has_errors());

    assert_eq!(support::close(&mut session), HeadlessSessionClose::Complete);
    assert!(session.is_closed());
    assert_eq!(session.close(), HeadlessSessionClose::Complete);
    assert!(matches!(
        session.start(),
        Err(HeadlessSessionError::Unavailable)
    ));
    assert!(matches!(
        session.frame(support::FIXED_DELTA),
        Err(HeadlessSessionError::Unavailable)
    ));
    assert!(session.capture(|_| captured.set(true)).is_err());
    assert!(!captured.get());
}

#[test]
fn semantic_submission_and_duplicate_refusal_preserve_the_live_command_owner() {
    let mut session = support::session();
    session.start().unwrap();
    assert_eq!(session.pause().unwrap(), None);
    let generation = session.runtime_generation().unwrap();
    let first = support::adjust(1, 1, 7);
    let first_key = first.key();
    assert_eq!(session.submit(first.clone()).unwrap(), first_key);
    assert!(matches!(
        session.submit(first),
        Err(HeadlessSessionError::CommandRejected(
            GameplayCommandRejection::Duplicate
        ))
    ));
    assert_eq!(session.runtime_generation(), Some(generation));
    assert_eq!(session.state(), Some(RuntimeState::Paused));
    let pending = session
        .observe(|world| world.resource::<GameplayCommandQueue>().stats())
        .unwrap();
    assert_eq!(pending.accepted, 1);
    assert_eq!(pending.rejected_duplicate, 1);
    assert_eq!(pending.pending_commands, 1);
    assert_eq!(pending.lifecycle_faults, 0);

    assert_eq!(session.step().unwrap(), None);
    let first_result = session.capture(support::snapshot).unwrap();
    assert_eq!(first_result.value, 7);
    assert_eq!(
        first_result.consumed.as_slice(),
        std::slice::from_ref(&first_key)
    );
    assert_eq!(first_result.deferred_ticks, [1]);

    let second = support::adjust(2, 2, -3);
    let second_key = second.key();
    assert_eq!(session.submit(second).unwrap(), second_key);
    assert_eq!(session.step().unwrap(), None);
    let second_result = session.capture(support::snapshot).unwrap();
    assert_eq!(second_result.tick, 2);
    assert_eq!(second_result.value, 4);
    assert_eq!(second_result.consumed, [first_key, second_key]);
    let completed = session
        .observe(|world| world.resource::<GameplayCommandQueue>().stats())
        .unwrap();
    assert_eq!(completed.acknowledged, 2);
    assert_eq!(completed.acknowledged_through_tick, 2);
    assert_eq!(completed.retained_commands, 0);
    assert_eq!(completed.quarantined_commands, 0);
    assert_eq!(completed.lifecycle_faults, 0);
    assert!(!session.diagnostics().has_errors());
    assert_eq!(session.resume().unwrap(), None);
    assert_eq!(session.frame(support::FIXED_DELTA).unwrap(), None);
    assert_eq!(session.observe(support::snapshot).unwrap().tick, 3);
    assert_eq!(support::close(&mut session), HeadlessSessionClose::Complete);
}

#[test]
fn startup_pause_does_not_certify_a_completed_capture_frame() {
    let recipe = support::recipe()
        .add_plugin::<support::StartPaused>()
        .unwrap();
    let mut session = HeadlessSession::new(support::project_root(), recipe);
    session.start().unwrap();
    assert_eq!(session.state(), Some(RuntimeState::Paused));
    let initial = session.observe(support::snapshot).unwrap();
    assert_eq!(initial.frame, 0);
    assert_eq!(initial.tick, 0);
    let invoked = Cell::new(false);
    assert!(matches!(
        session.capture(|_| invoked.set(true)),
        Err(HeadlessSessionError::NotPaused)
    ));
    assert!(!invoked.get());

    assert_eq!(session.frame(Duration::ZERO).unwrap(), None);
    let completed = session.capture(support::snapshot).unwrap();
    assert_eq!(completed.frame, 1);
    assert_eq!(completed.last_regular_frame, 1);
    assert_eq!(completed.tick, 0);
    assert!(completed.simulated_ticks.is_empty());
    assert_eq!(session.step().unwrap(), None);
    let stepped = session.capture(support::snapshot).unwrap();
    assert_eq!(stepped.tick, 1);
    assert_eq!(stepped.deferred_ticks, [1]);
    assert_eq!(support::close(&mut session), HeadlessSessionClose::Complete);
}

#[test]
fn a_direct_step_from_startup_pause_completes_task_update_and_last_before_capture() {
    let recipe = support::recipe()
        .add_plugin::<support::StartPaused>()
        .unwrap();
    let mut session = HeadlessSession::new(support::project_root(), recipe);
    session.start().unwrap();
    let initial = session.observe(support::snapshot).unwrap();
    assert_eq!(session.state(), Some(RuntimeState::Paused));
    assert_eq!(initial.tick, 0);
    assert_eq!(initial.frame, 0);
    assert!(initial.task_update_ticks.is_empty());
    let invoked = Cell::new(false);
    assert!(matches!(
        session.capture(|_| invoked.set(true)),
        Err(HeadlessSessionError::NotPaused)
    ));
    assert!(!invoked.get());

    assert_eq!(session.step().unwrap(), None);
    assert_eq!(session.state(), Some(RuntimeState::Paused));
    let completed = session.capture(support::snapshot).unwrap();
    assert_eq!(completed.generation, initial.generation);
    assert_eq!(completed.tick, initial.tick + 1);
    assert_eq!(completed.elapsed, support::FIXED_DELTA);
    assert_eq!(completed.simulated_ticks, [1]);
    assert_eq!(completed.deferred_ticks, [1]);
    assert_eq!(completed.task_update_ticks, [1]);
    assert_eq!(completed.last_observed_tick, completed.tick);
    assert_eq!(completed.last_regular_frame, completed.frame);
    assert_eq!(completed, session.observe(support::snapshot).unwrap());
    assert!(!session.diagnostics().has_errors());
    assert_eq!(support::close(&mut session), HeadlessSessionClose::Complete);
}

#[test]
fn a_refused_step_keeps_the_session_healthy_until_task_update_restores_readiness() {
    let recipe = support::recipe()
        .add_plugin::<support::StartPaused>()
        .unwrap()
        .add_plugin::<support::ReadyAfterTaskUpdate>()
        .unwrap();
    let mut session = HeadlessSession::new(support::project_root(), recipe);
    session.start().unwrap();
    let initial = session.observe(support::snapshot).unwrap();
    assert_eq!(session.state(), Some(RuntimeState::Paused));
    assert!(!session.observe(support::simulation_ready).unwrap());
    let command = support::adjust(1, 1, 9);
    let key = command.key();
    assert_eq!(session.submit(command).unwrap(), key);

    assert!(matches!(
        session.step(),
        Err(HeadlessSessionError::ControlRejected(
            RuntimeControlRejection::SimulationUnavailable
        ))
    ));
    assert_eq!(session.state(), Some(RuntimeState::Paused));
    assert_eq!(session.runtime_generation(), Some(initial.generation));
    assert_eq!(session.observe(support::snapshot).unwrap(), initial);
    assert!(!session.observe(support::simulation_ready).unwrap());
    assert!(!session.diagnostics().has_errors());

    assert_eq!(session.frame(Duration::ZERO).unwrap(), None);
    assert!(session.observe(support::simulation_ready).unwrap());
    let ready = session.capture(support::snapshot).unwrap();
    assert_eq!(ready.tick, 0);
    assert_eq!(ready.value, 0);
    assert!(ready.simulated_ticks.is_empty());
    assert_eq!(ready.task_update_ticks, [0]);
    assert_eq!(ready.last_regular_frame, ready.frame);
    let pending = session
        .observe(|world| world.resource::<GameplayCommandQueue>().stats())
        .unwrap();
    assert_eq!(pending.pending_commands, 1);
    assert_eq!(pending.acknowledged, 0);
    assert_eq!(pending.lifecycle_faults, 0);

    assert_eq!(session.step().unwrap(), None);
    let completed = session.capture(support::snapshot).unwrap();
    assert_eq!(completed.generation, initial.generation);
    assert_eq!(completed.tick, 1);
    assert_eq!(completed.value, 9);
    assert_eq!(completed.consumed, [key]);
    assert_eq!(completed.simulated_ticks, [1]);
    assert_eq!(completed.deferred_ticks, [1]);
    assert_eq!(completed.task_update_ticks, [0, 1]);
    assert_eq!(completed.last_observed_tick, completed.tick);
    assert_eq!(completed.last_regular_frame, completed.frame);
    let acknowledged = session
        .observe(|world| world.resource::<GameplayCommandQueue>().stats())
        .unwrap();
    assert_eq!(acknowledged.acknowledged, 1);
    assert_eq!(acknowledged.retained_commands, 0);
    assert_eq!(acknowledged.quarantined_commands, 0);
    assert_eq!(acknowledged.lifecycle_faults, 0);
    assert!(!session.diagnostics().has_errors());
    assert_eq!(support::close(&mut session), HeadlessSessionClose::Complete);
}

#[test]
fn fresh_source_sessions_have_distinct_runtime_generations_and_fresh_game_state() {
    let mut first = support::session();
    first.start().unwrap();
    let first_generation = first.runtime_generation().unwrap();
    let content = first.startup_content_closure().unwrap().content_digest();
    assert_eq!(
        first
            .submit(support::adjust(1, 1, 23))
            .unwrap()
            .tick()
            .get(),
        1
    );
    assert_eq!(first.frame(support::FIXED_DELTA).unwrap(), None);
    assert_eq!(first.observe(support::snapshot).unwrap().value, 23);
    assert_eq!(support::close(&mut first), HeadlessSessionClose::Complete);

    let mut second = support::session();
    second.start().unwrap();
    let second_generation = second.runtime_generation().unwrap();
    assert_ne!(second_generation, first_generation);
    assert_eq!(
        second.startup_content_closure().unwrap().content_digest(),
        content
    );
    let fresh = second.observe(support::snapshot).unwrap();
    assert_eq!(fresh.generation, second_generation);
    assert_eq!(fresh.tick, 0);
    assert_eq!(fresh.value, 0);
    assert!(fresh.consumed.is_empty());
    assert_eq!(second.frame(support::FIXED_DELTA).unwrap(), None);
    assert_eq!(second.observe(support::snapshot).unwrap().tick, 1);
    assert_eq!(support::close(&mut second), HeadlessSessionClose::Complete);
}

#[test]
fn a_failed_candidate_retires_without_replacing_or_poisoning_the_paused_incumbent() {
    let mut incumbent = support::session();
    incumbent.start().unwrap();
    let initial = incumbent.observe(support::snapshot).unwrap();
    assert!(matches!(
        incumbent.step(),
        Err(HeadlessSessionError::NotPaused)
    ));
    assert_eq!(incumbent.state(), Some(RuntimeState::Running));
    assert_eq!(incumbent.observe(support::snapshot).unwrap(), initial);
    assert!(!incumbent.diagnostics().has_errors());

    let first = support::adjust(1, 1, 41);
    let first_key = first.key();
    assert_eq!(incumbent.submit(first).unwrap(), first_key);
    assert_eq!(incumbent.frame(support::FIXED_DELTA).unwrap(), None);
    assert_eq!(incumbent.pause().unwrap(), None);
    let captured = incumbent.capture(support::snapshot).unwrap();
    assert_eq!(captured.tick, 1);
    assert_eq!(captured.value, 41);
    assert_eq!(captured.deferred_ticks, [1]);
    let cleanup = incumbent.observe(support::cleanup).unwrap();

    let recipe = support::recipe()
        .add_plugin::<support::FailStartup>()
        .unwrap();
    let mut candidate = HeadlessSession::new(support::project_root(), recipe);
    let error = candidate.start().unwrap_err();
    let HeadlessSessionError::Runtime(report) = error else {
        panic!("the candidate must reach and fail its deliberate Startup system");
    };
    assert!(report.has_errors());
    assert!(
        report
            .iter()
            .any(|entry| entry.code().as_str() == support::STARTUP_FAILURE)
    );
    assert_eq!(support::close(&mut candidate), HeadlessSessionClose::Failed);
    assert!(candidate.is_closed());
    assert_eq!(candidate.close(), HeadlessSessionClose::Failed);
    assert!(matches!(
        candidate.start(),
        Err(HeadlessSessionError::Unavailable)
    ));

    assert_eq!(incumbent.state(), Some(RuntimeState::Paused));
    assert_eq!(incumbent.runtime_generation(), Some(captured.generation));
    assert_eq!(incumbent.capture(support::snapshot).unwrap(), captured);
    assert_eq!(cleanup.begins.load(Ordering::SeqCst), 0);
    assert!(!cleanup.released.load(Ordering::SeqCst));
    assert_eq!(cleanup.drops.load(Ordering::SeqCst), 0);
    let second = support::adjust(2, 2, 1);
    let second_key = second.key();
    assert_eq!(incumbent.submit(second).unwrap(), second_key);
    assert_eq!(incumbent.step().unwrap(), None);
    let continued = incumbent.capture(support::snapshot).unwrap();
    assert_eq!(continued.generation, captured.generation);
    assert_eq!(continued.tick, captured.tick + 1);
    assert_eq!(continued.elapsed, captured.elapsed + support::FIXED_DELTA);
    assert_eq!(continued.value, captured.value + 1);
    assert_eq!(continued.consumed, [first_key, second_key]);
    assert_eq!(continued.simulated_ticks, [1, 2]);
    assert_eq!(continued.deferred_ticks, continued.simulated_ticks);
    assert!(!incumbent.diagnostics().has_errors());
    assert_eq!(
        support::close(&mut incumbent),
        HeadlessSessionClose::Complete
    );
    assert!(cleanup.released.load(Ordering::SeqCst));
    assert_eq!(cleanup.drops.load(Ordering::SeqCst), 1);
}

#[test]
fn pending_close_retains_the_original_participant_until_physical_completion() {
    let mut session = support::session();
    session.start().unwrap();
    let cleanup = session.observe(support::cleanup).unwrap();
    cleanup.ready.store(false, Ordering::SeqCst);

    assert_eq!(session.close(), HeadlessSessionClose::Pending);
    assert_eq!(session.close(), HeadlessSessionClose::Pending);
    assert!(!session.is_closed());
    assert_eq!(cleanup.begins.load(Ordering::SeqCst), 1);
    assert!(cleanup.polls.load(Ordering::SeqCst) > 0);
    assert!(!cleanup.released.load(Ordering::SeqCst));
    assert_eq!(cleanup.drops.load(Ordering::SeqCst), 0);
    assert!(matches!(
        session.start(),
        Err(HeadlessSessionError::Unavailable)
    ));
    assert!(matches!(
        session.submit(support::adjust(1, 1, 9)),
        Err(HeadlessSessionError::Unavailable)
    ));
    let invoked = Cell::new(false);
    assert!(session.observe(|_| invoked.set(true)).is_err());
    assert!(session.capture(|_| invoked.set(true)).is_err());
    assert!(!invoked.get());

    cleanup.ready.store(true, Ordering::SeqCst);
    assert_eq!(support::close(&mut session), HeadlessSessionClose::Complete);
    assert!(session.is_closed());
    assert!(cleanup.released.load(Ordering::SeqCst));
    assert_eq!(cleanup.drops.load(Ordering::SeqCst), 1);
    assert_eq!(session.close(), HeadlessSessionClose::Complete);
    assert_eq!(cleanup.begins.load(Ordering::SeqCst), 1);
    assert_eq!(cleanup.drops.load(Ordering::SeqCst), 1);
}

#[test]
fn completed_cleanup_error_remains_terminal_after_physical_release() {
    let mut session = support::session();
    session.start().unwrap();
    let cleanup = session.observe(support::cleanup).unwrap();
    cleanup.fail_release.store(true, Ordering::SeqCst);
    assert_eq!(support::close(&mut session), HeadlessSessionClose::Failed);
    assert!(session.is_closed());
    assert!(cleanup.released.load(Ordering::SeqCst));
    assert_eq!(cleanup.drops.load(Ordering::SeqCst), 1);
    assert!(session.diagnostics().has_errors());
    assert_eq!(session.close(), HeadlessSessionClose::Failed);
    assert!(matches!(
        session.start(),
        Err(HeadlessSessionError::Unavailable)
    ));
    assert_eq!(cleanup.begins.load(Ordering::SeqCst), 1);
    assert_eq!(cleanup.drops.load(Ordering::SeqCst), 1);
}

#[test]
fn execution_failure_keeps_its_diagnostic_and_original_physical_close_owner() {
    let recipe = support::recipe()
        .add_plugin::<support::FailFrame>()
        .unwrap();
    let mut session = HeadlessSession::new(support::project_root(), recipe);
    session.start().unwrap();
    let cleanup = session.observe(support::cleanup).unwrap();
    let generation = session.runtime_generation().unwrap();
    assert!(matches!(
        session.frame(support::FIXED_DELTA),
        Err(HeadlessSessionError::Runtime(_))
    ));
    assert_eq!(session.state(), Some(RuntimeState::Faulted));
    assert_eq!(session.runtime_generation(), Some(generation));
    assert!(format!("{:?}", session.diagnostics()).contains(support::EXECUTION_FAILURE));
    assert!(!cleanup.released.load(Ordering::SeqCst));
    assert_eq!(cleanup.drops.load(Ordering::SeqCst), 0);
    let invoked = Cell::new(false);
    assert!(session.observe(|_| invoked.set(true)).is_err());
    assert!(session.capture(|_| invoked.set(true)).is_err());
    assert!(!invoked.get());
    assert!(matches!(
        session.frame(support::FIXED_DELTA),
        Err(HeadlessSessionError::Unavailable)
    ));
    assert_eq!(support::close(&mut session), HeadlessSessionClose::Failed);
    assert!(session.is_closed());
    assert!(cleanup.released.load(Ordering::SeqCst));
    assert_eq!(cleanup.drops.load(Ordering::SeqCst), 1);
    assert_eq!(session.close(), HeadlessSessionClose::Failed);
}

#[test]
fn a_game_requested_exit_is_returned_without_skipping_its_deferred_command_work() {
    let mut session = support::session();
    session.start().unwrap();
    let command = support::exit(1, 1);
    let key = command.key();
    assert_eq!(session.submit(command).unwrap(), key);
    assert_eq!(
        session.frame(support::FIXED_DELTA).unwrap(),
        Some(engine::app::AppExit::Success)
    );
    let completed = session.observe(support::snapshot).unwrap();
    assert_eq!(completed.tick, 1);
    assert_eq!(completed.consumed, [key]);
    assert_eq!(completed.deferred_ticks, [1]);
    assert_eq!(completed.last_regular_frame, completed.frame);
    assert_eq!(support::close(&mut session), HeadlessSessionClose::Complete);
}

#[test]
fn terminal_exit_from_an_exact_step_refuses_further_execution_input_and_capture() {
    let mut session = support::session();
    session.start().unwrap();
    assert_eq!(session.exit(), None);
    assert_eq!(session.pause().unwrap(), None);
    let generation = session.runtime_generation().unwrap();
    let command = support::exit(1, 1);
    let key = command.key();
    assert_eq!(session.submit(command).unwrap(), key);
    assert_eq!(session.step().unwrap(), Some(AppExit::Success));
    assert_eq!(session.exit(), Some(AppExit::Success));
    assert_eq!(session.state(), Some(RuntimeState::Paused));
    let completed = session.observe(support::snapshot).unwrap();
    assert_eq!(completed.tick, 1);
    assert_eq!(completed.consumed, [key]);
    assert_eq!(completed.deferred_ticks, [1]);

    assert!(matches!(
        session.frame(support::FIXED_DELTA),
        Err(HeadlessSessionError::Unavailable)
    ));
    assert!(matches!(
        session.pause(),
        Err(HeadlessSessionError::Unavailable)
    ));
    assert!(matches!(
        session.step(),
        Err(HeadlessSessionError::Unavailable)
    ));
    assert!(matches!(
        session.resume(),
        Err(HeadlessSessionError::Unavailable)
    ));
    assert!(matches!(
        session.submit(support::adjust(2, 2, 99)),
        Err(HeadlessSessionError::Unavailable)
    ));
    let invoked = Cell::new(false);
    assert!(matches!(
        session.capture(|_| invoked.set(true)),
        Err(HeadlessSessionError::NotPaused)
    ));
    assert!(!invoked.get());
    assert_eq!(session.observe(support::snapshot).unwrap(), completed);
    assert_eq!(session.runtime_generation(), Some(generation));
    assert_eq!(session.exit(), Some(AppExit::Success));
    assert!(!session.diagnostics().has_errors());
    assert_eq!(support::close(&mut session), HeadlessSessionClose::Complete);
    assert!(session.is_closed());
    assert_eq!(session.close(), HeadlessSessionClose::Complete);
    assert_eq!(session.exit(), Some(AppExit::Success));
}
