//! Acceptance limits for the finite v1 consumer: 256 KiB, 512 operations, 128 ticks and 256 commands.
//!
//! Actual acquisition and state refunds remain covered by `src/runtime_content/input/tests.rs`,
//! including `escaped_bindings_retain_the_actual_acquisition_and_refund_only_on_last_drop` and
//! `paused_queued_input_retains_only_its_state_charge_after_the_original_map_is_replaced`.
//! Stop retains an observable World until its owner is consumed; these controls do not claim an
//! independently observable allocation refund or exercise failed preparation custody.

use std::{any::type_name, time::Duration};

use nara_engine::{
    app::{
        RuntimeControl, RuntimeControlRequestResult, RuntimeControlStatus, RuntimeScopeError,
        RuntimeState, RuntimeWorldAccessError,
    },
    gameplay::{GameplayCommandQueue, GameplayCommandSource, GameplayCommandSourceSequence},
    input::{
        AxisValue, ButtonInputDriverError, ButtonTransitionPhase, InputIngressError, InputTimeline,
        SemanticActions, apply_gamepad_axis_input, apply_gamepad_button_input,
    },
};

use super::{
    last_command,
    recording::{
        Control, Harness, ImportError, RawOperation, RawRecording, SemanticRecording, driver,
    },
    runtime::{Bindings, GameState, Seat, fixed_nanos, held, state},
    trajectory::complete_trajectory,
};

const ENCODED_BYTES: usize = 256 * 1024;
const OPERATIONS: usize = 512;
const TICKS: usize = 128;
const COMMANDS: usize = 256;

#[test]
fn stop_with_held_controller_and_pending_local_commands_refuses_late_input_and_allows_fresh_runtime()
 {
    let mut raw = complete_trajectory();
    let staged = raw
        .operations
        .iter()
        .position(|operation| {
            matches!(operation, RawOperation::Frame { checkpoint, .. } if checkpoint == "staged-two-seats")
        })
        .unwrap();
    raw.operations.truncate(staged + 1);
    raw.operations.retain(|operation| {
        !matches!(
            operation,
            RawOperation::Key { .. } | RawOperation::External { .. }
        )
    });

    let mut live = Harness::new(Bindings::OriginalV1, false, false);
    live.play(&raw);
    let connection = live.connections[&0].clone();
    let generation = live.runtime().generation();
    assert_eq!(state(live.runtime()), GameState::default());
    assert!(live.capture().is_empty());
    assert_eq!(
        held(
            live.runtime().world().resource::<SemanticActions>(),
            Seat::Controller
        ),
        (0.5, true)
    );
    let pending = live
        .runtime()
        .world()
        .resource::<GameplayCommandQueue>()
        .stats();
    assert_eq!(pending.accepted, 2);
    assert_eq!(pending.pending_commands, 2);
    assert!(pending.pending_bytes > 0);
    assert_eq!(pending.closed_through_tick, 0);
    assert_eq!(pending.acknowledged_through_tick, 0);

    let RuntimeControlRequestResult::Accepted(stop) =
        live.started.runtime.request_control(RuntimeControl::Stop)
    else {
        panic!("the held-input runtime must accept Stop");
    };
    let stopping = live
        .started
        .runtime
        .drive(Duration::from_nanos(fixed_nanos(4)))
        .unwrap();
    assert_eq!(stopping.state(), RuntimeState::Stopping);
    assert!(stopping.frame().is_none());
    let timeline = live.runtime().world().resource::<InputTimeline>();
    let retained = (
        timeline.entries().copied().collect::<Vec<_>>(),
        timeline.cursors(),
    );
    let late = live
        .started
        .runtime
        .with_driver_scope(|scope| {
            (
                apply_gamepad_axis_input(scope, &connection, 0, AxisValue::ZERO),
                apply_gamepad_button_input(scope, &connection, 0, ButtonTransitionPhase::Released),
            )
        })
        .unwrap();
    assert!(late.post_operation_fault.is_none());
    let refusal = ButtonInputDriverError::WorldAccess(RuntimeWorldAccessError::PortUnavailable {
        type_name: type_name::<InputTimeline>(),
        state: RuntimeState::Stopping,
    });
    assert_eq!(late.output, (Err(refusal), Err(refusal)));
    let timeline = live.runtime().world().resource::<InputTimeline>();
    assert_eq!(
        (
            timeline.entries().copied().collect::<Vec<_>>(),
            timeline.cursors()
        ),
        retained
    );
    assert_eq!(
        live.runtime()
            .world()
            .resource::<GameplayCommandQueue>()
            .stats(),
        pending
    );

    let stopped = live
        .started
        .runtime
        .drive(Duration::from_nanos(fixed_nanos(4)))
        .unwrap();
    assert_eq!(stopped.state(), RuntimeState::Stopped);
    assert!(stopped.frame().is_none());
    assert_eq!(
        live.started.runtime.control_status(stop),
        Some(RuntimeControlStatus::Applied)
    );
    assert!(live.runtime().close_evidence().is_empty());
    assert_eq!(state(live.runtime()), GameState::default());
    assert!(live.capture().is_empty());
    assert_eq!(
        live.runtime()
            .world()
            .resource::<GameplayCommandQueue>()
            .stats(),
        pending
    );
    assert_eq!(
        live.started
            .runtime
            .with_driver_scope(|scope| apply_gamepad_axis_input(
                scope,
                &connection,
                0,
                AxisValue::ZERO
            ))
            .unwrap_err(),
        RuntimeScopeError::Unavailable {
            state: RuntimeState::Stopped
        }
    );
    live.finish();

    let mut fresh = Harness::new(Bindings::OriginalV1, false, false);
    assert_ne!(fresh.runtime().generation(), generation);
    assert_eq!(state(fresh.runtime()), GameState::default());
    assert_eq!(
        held(
            fresh.runtime().world().resource::<SemanticActions>(),
            Seat::Controller
        ),
        (0.0, false)
    );
    assert_eq!(
        fresh
            .runtime()
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        0
    );
    fresh.play(&raw);
    let replacement = &fresh.connections[&0];
    assert_eq!(replacement.raw_label(), connection.raw_label());
    assert_eq!(
        replacement.connection_generation(),
        connection.connection_generation()
    );
    assert_ne!(replacement, &connection);
    let timeline = fresh.runtime().world().resource::<InputTimeline>();
    let retained = (
        timeline.entries().copied().collect::<Vec<_>>(),
        timeline.cursors(),
    );
    let before = fresh
        .runtime()
        .world()
        .resource::<GameplayCommandQueue>()
        .stats();
    assert_eq!(
        driver(&mut fresh.started.runtime, |scope| {
            apply_gamepad_axis_input(scope, &connection, 0, AxisValue::ZERO)
        }),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::ForeignGamepadConnection
        ))
    );
    let timeline = fresh.runtime().world().resource::<InputTimeline>();
    assert_eq!(
        (
            timeline.entries().copied().collect::<Vec<_>>(),
            timeline.cursors()
        ),
        retained
    );
    assert_eq!(
        fresh
            .runtime()
            .world()
            .resource::<GameplayCommandQueue>()
            .stats(),
        before
    );
    fresh.drive(
        fixed_nanos(1),
        1,
        "fresh-controller-consumes-only-fresh-local-commands",
    );
    assert_eq!(fresh.capture()[0].commands.len(), 2);
    assert!(
        fresh.capture()[0]
            .commands
            .iter()
            .all(|command| { command.source() == &GameplayCommandSource::LocalAction })
    );
    let game = state(fresh.runtime());
    assert_eq!(game.players[0], GameState::default().players[0]);
    assert_eq!(game.players[1].held_axis, 0.5);
    assert!(game.players[1].firing);
    assert_eq!(game.players[1].position, -1.5);
    assert_eq!(game.players[1].shots, 1);
    assert_eq!(game.players[1].commands, 2);
    assert_eq!(
        fresh
            .runtime()
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .pending_commands,
        0
    );
    fresh.finish();
}

#[test]
fn raw_load_accepts_exact_finite_limits_and_refuses_overflow_or_bad_tail_before_playback() {
    let raw = complete_trajectory();
    let padding = RawOperation::Frame {
        real_delta_nanos: 0,
        steps: 0,
        checkpoint: "operation-limit-padding".to_owned(),
    };
    let mut encoded = raw.encode();
    assert!(encoded.len() < ENCODED_BYTES);
    encoded.resize(ENCODED_BYTES, b' ');
    assert_eq!(
        RawRecording::load(&encoded, Bindings::OriginalV1),
        Ok(raw.clone())
    );
    encoded.push(b' ');

    let mut at_operations = raw.clone();
    assert!(at_operations.operations.len() < OPERATIONS);
    at_operations.operations.resize(OPERATIONS, padding.clone());
    assert_eq!(
        RawRecording::load(&at_operations.encode(), Bindings::OriginalV1),
        Ok(at_operations.clone())
    );
    let mut bad_tail = at_operations.clone();
    *bad_tail.operations.last_mut().unwrap() = RawOperation::Control {
        control: Control::Pause,
    };
    at_operations.operations.push(padding);

    let mut at_ticks = raw.clone();
    let mut ticks = at_ticks
        .operations
        .iter()
        .filter_map(|operation| match operation {
            RawOperation::Frame { steps, .. } => Some(*steps as usize),
            _ => None,
        })
        .sum::<usize>();
    assert!(ticks < TICKS);
    while ticks < TICKS {
        let steps = (TICKS - ticks).min(4) as u32;
        at_ticks.operations.push(RawOperation::Frame {
            real_delta_nanos: fixed_nanos(steps),
            steps,
            checkpoint: "tick-limit-padding".to_owned(),
        });
        ticks += steps as usize;
    }
    assert_eq!(
        RawRecording::load(&at_ticks.encode(), Bindings::OriginalV1),
        Ok(at_ticks.clone())
    );
    at_ticks.operations.push(RawOperation::Frame {
        real_delta_nanos: fixed_nanos(1),
        steps: 1,
        checkpoint: "one-tick-over-limit".to_owned(),
    });

    let mut consumer = Harness::new(Bindings::OriginalV1, false, false);
    let before = consumer
        .runtime()
        .world()
        .resource::<GameplayCommandQueue>()
        .stats();
    let timeline = consumer.runtime().world().resource::<InputTimeline>();
    let retained = (
        timeline.entries().copied().collect::<Vec<_>>(),
        timeline.cursors(),
    );
    for (expected, bytes) in [
        (ImportError::Budget, encoded),
        (ImportError::Budget, at_operations.encode()),
        (ImportError::Ordering, at_ticks.encode()),
        (ImportError::Ordering, bad_tail.encode()),
    ] {
        assert_eq!(
            RawRecording::load(&bytes, Bindings::OriginalV1).map(|loaded| consumer.play(&loaded)),
            Err(expected)
        );
        assert_eq!(
            consumer
                .runtime()
                .world()
                .resource::<GameplayCommandQueue>()
                .stats(),
            before
        );
        assert_eq!(state(consumer.runtime()), GameState::default());
        assert!(consumer.capture().is_empty());
        assert!(consumer.frames.is_empty());
        let timeline = consumer.runtime().world().resource::<InputTimeline>();
        assert_eq!(
            (
                timeline.entries().copied().collect::<Vec<_>>(),
                timeline.cursors()
            ),
            retained
        );
    }
    consumer.drive(fixed_nanos(1), 1, "raw-refusals-leave-no-command-prefix");
    assert!(consumer.capture()[0].commands.is_empty());
    assert_eq!(
        state(consumer.runtime()).players,
        GameState::default().players
    );
    assert_eq!(
        consumer
            .runtime()
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        0
    );
    consumer.finish();
}

#[test]
fn semantic_load_bounds_the_complete_artifact_before_admitting_any_command_prefix() {
    let raw = complete_trajectory();
    let mut live = Harness::new(Bindings::OriginalV1, false, false);
    live.play(&raw);
    let semantic = SemanticRecording::capture(live.capture());
    live.finish();

    let mut encoded = semantic.encode();
    assert!(encoded.len() < ENCODED_BYTES);
    encoded.resize(ENCODED_BYTES, b' ');
    assert_eq!(SemanticRecording::load(&encoded), Ok(semantic.clone()));
    encoded.push(b' ');

    let mut at_ticks = semantic.clone();
    assert!(at_ticks.ticks.len() < TICKS);
    let mut padding = at_ticks.ticks.last().unwrap().clone();
    padding.commands.clear();
    while at_ticks.ticks.len() < TICKS {
        padding.state.tick = at_ticks.ticks.len() as u64 + 1;
        at_ticks.ticks.push(padding.clone());
    }
    assert_eq!(
        SemanticRecording::load(&at_ticks.encode()),
        Ok(at_ticks.clone())
    );
    padding.state.tick += 1;
    at_ticks.ticks.push(padding);

    let mut at_commands = semantic.clone();
    let count = at_commands
        .ticks
        .iter()
        .map(|tick| tick.commands.len())
        .sum::<usize>();
    assert!(count > 0 && count < COMMANDS);
    let tick = at_commands
        .ticks
        .iter_mut()
        .rev()
        .find(|tick| !tick.commands.is_empty())
        .unwrap();
    let mut command = tick.commands.last().unwrap().clone();
    for ordinal in count + 1..=COMMANDS {
        command.ordinal = ordinal as u64;
        command.original_sequence =
            GameplayCommandSourceSequence::new(command.original_sequence.get() + 1).unwrap();
        tick.commands.push(command.clone());
    }
    assert_eq!(
        SemanticRecording::load(&at_commands.encode()),
        Ok(at_commands.clone())
    );
    let mut bad_tail = at_commands.clone();
    last_command(&mut bad_tail).ordinal += 1;
    let mut over_commands = at_commands.clone();
    command.ordinal += 1;
    command.original_sequence =
        GameplayCommandSourceSequence::new(command.original_sequence.get() + 1).unwrap();
    over_commands
        .ticks
        .iter_mut()
        .rev()
        .find(|tick| !tick.commands.is_empty())
        .unwrap()
        .commands
        .push(command);
    let mut bad_encoding_tail = semantic.encode();
    bad_encoding_tail.push(b'!');

    let mut replay = Harness::new(Bindings::ReboundV1, true, false);
    let before = replay
        .runtime()
        .world()
        .resource::<GameplayCommandQueue>()
        .stats();
    for (expected, bytes) in [
        (ImportError::Budget, encoded),
        (ImportError::Budget, at_ticks.encode()),
        (ImportError::Budget, over_commands.encode()),
        (ImportError::Ordering, bad_tail.encode()),
        (ImportError::Encoding, bad_encoding_tail),
    ] {
        assert_eq!(replay.import_semantic(&bytes), Err(expected));
        assert_eq!(
            replay
                .runtime()
                .world()
                .resource::<GameplayCommandQueue>()
                .stats(),
            before
        );
        assert_eq!(state(replay.runtime()), GameState::default());
        assert!(replay.capture().is_empty());
    }

    assert_eq!(
        replay.import_semantic(&at_commands.encode()),
        Ok(at_commands)
    );
    let queued = replay
        .runtime()
        .world()
        .resource::<GameplayCommandQueue>()
        .stats();
    assert_eq!(queued.accepted, COMMANDS as u64);
    assert_eq!(queued.pending_commands, COMMANDS);
    assert_eq!(state(replay.runtime()), GameState::default());
    assert!(replay.capture().is_empty());
    replay.play_controls(&raw);
    assert_eq!(replay.capture().len(), semantic.ticks.len());
    assert_eq!(
        replay
            .capture()
            .iter()
            .map(|tick| tick.commands.len())
            .sum::<usize>(),
        COMMANDS
    );
    let consumed = replay
        .runtime()
        .world()
        .resource::<GameplayCommandQueue>()
        .stats();
    assert_eq!(consumed.pending_commands, 0);
    assert_eq!(consumed.retained_commands, 0);
    assert_eq!(consumed.retained_bytes, 0);
    replay.finish();
}
