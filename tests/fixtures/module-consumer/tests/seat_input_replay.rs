//! Independent normalized-input and semantic-command recordings for two game-owned players.
//!
//! This finite game consumer uses the production Input, Runtime and Gameplay owners. It makes
//! no hardware, native presentation, performance or general-purpose replay-format claim.

#[path = "seat_input_replay/boundaries.rs"]
mod boundaries;
#[path = "seat_input_replay/recording.rs"]
mod recording;
#[path = "seat_input_replay/runtime.rs"]
mod runtime;
#[path = "seat_input_replay/trajectory.rs"]
mod trajectory;

use std::time::Duration;

use nara_engine::{
    app::{RuntimeFaultKind, RuntimeState},
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandQueue,
        GameplayCommandRejection, GameplayCommandSource, GameplayCommandTypeId,
        GameplayCommandValue, LocalGameplayCommands,
    },
    input::{ActionPhase, ActionValue, AxisValue, SemanticActionCause, SemanticActions},
};

use recording::{
    Harness, ImportError, RawOperation, RawRecording, SemanticRecording, ValueKind, assert_rebound,
};
use runtime::{
    AI, Bindings, EXTERNAL, FIRE, GameState, Intent, MOVE, MOVE_COMMAND, Player, Producers, REPLAY,
    Seat, Window, command, continue_simulation, fixed_nanos, held, state, submission,
};
use trajectory::complete_trajectory;

fn assert_live_checkpoints(live: &Harness) {
    let staged = live.frame("staged-two-seats");
    assert_eq!(staged.game, GameState::default());
    assert_eq!(staged.semantic, [(1.0, true), (0.5, true)]);
    assert_eq!(staged.pending_commands, 6);
    assert_eq!(
        live.frame("dead-zone-boundary").semantic,
        [(0.0, false), (0.0, false)]
    );
    for checkpoint in [
        "dead-zone-interior",
        "dead-zone-boundary",
        "new-axis-starts-inside-dead-zone",
    ] {
        assert_eq!(
            live.frame(checkpoint).selected_axes[1],
            Some(ActionValue::Axis(AxisValue::ZERO)),
            "an adopted neutral contribution remains a member: {checkpoint}"
        );
    }
    let first = live.frame("first-three-ticks");
    assert_eq!(first.game.tick, 3);
    assert_eq!(first.game.players[0].position, 1.5);
    assert_eq!(first.game.players[1].position, -4.5);
    assert_eq!(
        first.game.players[0].held_axis, 0.25,
        "the last canonical competing source wins"
    );
    assert_eq!(first.game.players[1].held_axis, 0.5);
    assert_eq!(first.game.players[0].shots, 3);
    assert_eq!(first.game.players[1].shots, 3);

    assert_eq!(
        live.frame("summed-contributors").semantic,
        [(0.0, true), (1.0, true)]
    );
    assert_eq!(
        live.frame("opposing-contributors").semantic,
        [(1.0, true), (0.0, true)]
    );
    assert_eq!(
        live.frame("opposing-contributors").selected_axes[1],
        Some(ActionValue::Axis(AxisValue::ZERO))
    );
    let survivor = live.frame("disconnect-preserves-survivor");
    assert_eq!(survivor.semantic, [(0.0, true), (0.5, true)]);
    assert_eq!(survivor.game.players[0].position, 3.5);
    assert_eq!(survivor.game.players[1].position, -10.5);

    let paused = live.frame("paused");
    let pending = live.frame("paused-input");
    let step = live.frame("exact-step-before-resume");
    let resumed = live.frame("resumed-input");
    assert!(paused.paused && pending.paused && step.paused && !resumed.paused);
    assert_eq!(pending.game, paused.game);
    assert_eq!(pending.passes, paused.passes);
    assert_eq!(
        step.passes, paused.passes,
        "exact Step must not resolve actions again"
    );
    assert_eq!(step.edges, paused.edges);
    assert_eq!(step.semantic, paused.semantic);
    assert_eq!(step.game.tick, paused.game.tick + 1);
    assert_eq!(step.game.players[0].held_axis, 0.0);
    assert_eq!(step.game.players[1].held_axis, 0.5);
    assert_eq!(step.game.players[1].position, -12.0);
    assert_eq!(resumed.game, step.game);
    assert_eq!(resumed.passes, step.passes + 1);
    assert_eq!(resumed.semantic, [(1.0, true), (-0.5, true)]);
    assert_eq!(resumed.pending_commands, 2);
    assert_eq!(
        live.frame("resumed-two-ticks").game.players[0].position,
        7.5
    );
    assert_eq!(
        live.frame("resumed-two-ticks").game.players[1].position,
        -9.0
    );

    for checkpoint in [
        "observed-held-rebind",
        "restored-bindings-do-not-adopt-held",
    ] {
        assert_eq!(
            live.frame(checkpoint).semantic[0],
            (0.0, false),
            "{checkpoint}"
        );
        assert_eq!(
            live.frame(checkpoint).semantic[1],
            (-0.5, true),
            "{checkpoint}"
        );
    }
    assert_eq!(live.frame("new-key-member").semantic[0], (1.0, false));
    for checkpoint in [
        "controller-context-disabled",
        "controller-context-enabled",
        "old-axis-after-context-loss",
        "new-axis-starts-inside-dead-zone",
        "controller-map-removed",
        "axis-updated-without-map",
        "original-owned-map-restored",
        "old-axis-after-map-restoration",
    ] {
        assert_eq!(
            live.frame(checkpoint).semantic[1],
            (0.0, false),
            "{checkpoint}"
        );
    }
    for checkpoint in [
        "adopted-zero-contribution-changes",
        "fresh-axis-after-map-restoration",
    ] {
        assert_eq!(
            live.frame(checkpoint).semantic[1],
            (0.5, false),
            "{checkpoint}"
        );
        assert_eq!(
            live.frame(checkpoint).game.players[1].held_axis,
            0.5,
            "{checkpoint}"
        );
    }

    let captured = live.frame("ui-consumed-fire-press");
    let returned = live.frame("ui-capture-returned");
    let released = live.frame("ui-owned-release-does-not-leak");
    assert!(captured.consumed > live.frame("keyboard-ui-capture").consumed);
    assert_eq!(captured.semantic[0], (1.0, false));
    assert_eq!(returned.edges, captured.edges);
    assert_eq!(
        released.edges, captured.edges,
        "UI owns the original member through its release"
    );
    assert!(!released.game.players[0].firing);
    assert!(live.frame("fresh-gameplay-fire").game.players[0].firing);
    assert_eq!(
        live.frame("keyboard-focus-lost").semantic,
        [(0.0, false), (0.5, false)]
    );
    assert_eq!(
        live.frame("keyboard-focus-returned").semantic,
        [(1.0, false), (0.5, false)]
    );
    assert_eq!(
        live.frame("last-controller-disconnected").semantic[1],
        (0.0, false)
    );
    assert_eq!(
        live.frame("same-label-reconnected").semantic[1],
        (0.0, false)
    );
    assert_eq!(
        live.frame("stale-and-foreign-refused").semantic[1],
        (0.0, false)
    );
    let final_state = &live.frame("new-controller-controls-player").game;
    assert_eq!(final_state.tick, 27);
    assert_eq!(final_state.players[0].position, 35.5);
    assert_eq!(final_state.players[1].position, -10.5);
    assert_eq!(final_state.players[0].held_axis, 1.0);
    assert_eq!(final_state.players[1].held_axis, -0.5);
    assert_eq!(final_state.players[0].shots, 11);
    assert_eq!(final_state.players[1].shots, 15);

    let actions = live
        .runtime()
        .world()
        .resource::<runtime::ObservedActions>();
    let initial_moves = actions.edges[..staged.edges]
        .iter()
        .filter(|edge| edge.action == MOVE && edge.phase == ActionPhase::Started)
        .map(|edge| edge.seat)
        .collect::<Vec<_>>();
    assert_eq!(initial_moves, Seat::ALL);
    assert!(
        actions
            .edges
            .iter()
            .any(|edge| edge.phase == ActionPhase::Changed && edge.action == MOVE)
    );
    assert!(
        actions
            .edges
            .iter()
            .any(|edge| edge.cause == SemanticActionCause::MappingChanged)
    );
    assert!(
        actions
            .edges
            .iter()
            .any(|edge| edge.cause == SemanticActionCause::ContextChanged)
    );
    assert!(
        actions
            .edges
            .iter()
            .any(|edge| edge.action == FIRE && edge.seat == Seat::Controller)
    );
}

fn reject_live_controls(replay: &mut Harness) {
    assert_eq!(
        *replay.runtime().world().resource::<Producers>(),
        Producers::RECORDED
    );
    for (source, admitted) in [
        (
            GameplayCommandIngressSource::test("live-control").unwrap(),
            GameplayCommandSource::test("live-control").unwrap(),
        ),
        (
            GameplayCommandIngressSource::ai(AI).unwrap(),
            GameplayCommandSource::ai(AI).unwrap(),
        ),
        (
            GameplayCommandIngressSource::external(EXTERNAL).unwrap(),
            GameplayCommandSource::external(EXTERNAL).unwrap(),
        ),
        (
            GameplayCommandIngressSource::replay("another-recording").unwrap(),
            GameplayCommandSource::replay("another-recording").unwrap(),
        ),
    ] {
        let before = replay
            .runtime()
            .world()
            .resource::<GameplayCommandQueue>()
            .stats();
        let players = state(replay.runtime());
        let result = replay.submit(submission(
            1,
            source,
            1,
            command(
                Player::Copper,
                None,
                Intent::Axis(AxisValue::new(-1.0).unwrap()),
            ),
        ));
        assert_eq!(
            result,
            Err(GameplayCommandRejection::UnexpectedSource { actual: admitted })
        );
        let after = replay
            .runtime()
            .world()
            .resource::<GameplayCommandQueue>()
            .stats();
        assert_eq!(after.accepted, before.accepted);
        assert_eq!(after.pending_commands, before.pending_commands);
        assert_eq!(after.pending_bytes, before.pending_bytes);
        assert_eq!(after.retained_commands, before.retained_commands);
        assert_eq!(after.retained_bytes, before.retained_bytes);
        assert_eq!(after.furthest_pending_tick, before.furthest_pending_tick);
        assert_eq!(after.rejected, before.rejected + 1);
        assert_eq!(state(replay.runtime()), players);
    }
}

#[test]
fn two_seat_raw_and_semantic_artifacts_replay_every_fixed_tick() {
    let recording = complete_trajectory();
    let mut live = Harness::new(Bindings::OriginalV1, false, false);
    live.play(&recording);
    assert_live_checkpoints(&live);
    let original_ticks = live.capture().to_vec();
    let original_frames = live.frames.clone();
    let original_tokens = live.connections.clone();
    let semantic = SemanticRecording::capture(&original_ticks);
    assert_eq!(semantic.ticks.len(), 27);
    assert_eq!(original_ticks[0].commands.len(), 6);
    assert_eq!(
        original_ticks[0]
            .commands
            .iter()
            .map(|entry| entry.source().kind_rank())
            .collect::<Vec<_>>(),
        [0, 0, 0, 0, 3, 4]
    );
    let sources = semantic
        .ticks
        .iter()
        .flat_map(|tick| &tick.commands)
        .map(|entry| &entry.original_source)
        .collect::<Vec<_>>();
    assert!(
        sources
            .iter()
            .any(|source| **source == GameplayCommandSource::LocalAction)
    );
    assert!(
        sources
            .iter()
            .any(|source| **source == GameplayCommandSource::ai(AI).unwrap())
    );
    assert!(
        sources
            .iter()
            .any(|source| **source == GameplayCommandSource::external(EXTERNAL).unwrap())
    );

    let directory = tempfile::tempdir().unwrap();
    let raw_path = directory.path().join("normalized-input-v1.json");
    let semantic_path = directory.path().join("two-player-commands-v1.json");
    std::fs::write(&raw_path, recording.encode()).unwrap();
    std::fs::write(&semantic_path, semantic.encode()).unwrap();
    continue_simulation(&mut live.started.runtime, 4);
    let continued_ticks = live.capture().to_vec();
    live.finish();

    let decoded_raw =
        RawRecording::load(&std::fs::read(&raw_path).unwrap(), Bindings::OriginalV1).unwrap();
    assert_eq!(decoded_raw, recording);
    let mut raw_replay = Harness::new(decoded_raw.bindings, false, false);
    raw_replay.play(&decoded_raw);
    assert_live_checkpoints(&raw_replay);
    assert_eq!(raw_replay.frames, original_frames);
    assert_eq!(raw_replay.capture(), original_ticks);
    for (logical_index, original) in &original_tokens {
        let fresh = &raw_replay.connections[logical_index];
        assert_ne!(
            original, fresh,
            "a recording must obtain connection authority from its new owner"
        );
        assert_eq!(
            original.connection_generation(),
            fresh.connection_generation()
        );
        assert_eq!(original.raw_label(), fresh.raw_label());
    }
    continue_simulation(&mut raw_replay.started.runtime, 4);
    assert_eq!(raw_replay.capture(), continued_ticks);
    raw_replay.finish();

    let semantic_bytes = std::fs::read(&semantic_path).unwrap();
    let mut semantic_replay = Harness::new(Bindings::ReboundV1, true, false);
    assert_rebound(&semantic_replay);
    reject_live_controls(&mut semantic_replay);
    semantic_replay.apply(&RawOperation::Endpoint {
        window: Window::Play,
        seat: Seat::Keyboard,
    });
    semantic_replay.apply(&RawOperation::Key {
        window: Window::Play,
        seat: Seat::Keyboard,
        key: Bindings::ReboundV1.negative(),
        pressed: true,
    });
    semantic_replay.drive(0, 0, "different-physical-bindings");
    assert_eq!(
        held(
            semantic_replay
                .runtime()
                .world()
                .resource::<SemanticActions>(),
            Seat::Keyboard
        ),
        (-1.0, false)
    );
    assert_eq!(state(semantic_replay.runtime()), GameState::default());
    assert_eq!(
        semantic_replay
            .runtime()
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        0
    );
    let loaded = semantic_replay.import_semantic(&semantic_bytes).unwrap();
    assert_eq!(
        loaded, semantic,
        "original mixed-source provenance remains data in the artifact"
    );
    semantic_replay.play_controls(&decoded_raw);
    for ((actual, expected), recorded) in semantic_replay
        .capture()
        .iter()
        .zip(&original_ticks)
        .zip(&loaded.ticks)
    {
        assert_eq!(actual.state, expected.state, "tick {}", expected.state.tick);
        assert_eq!(actual.commands.len(), expected.commands.len());
        assert_eq!(actual.commands.len(), recorded.commands.len());
        for ((actual, expected), recorded) in actual
            .commands
            .iter()
            .zip(&expected.commands)
            .zip(&recorded.commands)
        {
            assert_eq!(
                actual.command(),
                expected.command(),
                "retain the entire captured draft and canonical order"
            );
            assert_eq!(actual.tick(), expected.tick());
            assert_eq!(
                actual.source(),
                &GameplayCommandSource::replay(REPLAY).unwrap()
            );
            assert_eq!(actual.source_sequence().get(), recorded.ordinal);
        }
    }
    assert_eq!(semantic_replay.capture().len(), original_ticks.len());
    assert_eq!(
        semantic_replay
            .runtime()
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .pending_commands,
        0
    );
    continue_simulation(&mut semantic_replay.started.runtime, 4);
    assert_eq!(
        semantic_replay
            .capture()
            .iter()
            .map(|tick| &tick.state)
            .collect::<Vec<_>>(),
        continued_ticks
            .iter()
            .map(|tick| &tick.state)
            .collect::<Vec<_>>()
    );
    semantic_replay.finish();
}

fn changed_semantic(
    recording: &SemanticRecording,
    change: impl FnOnce(&mut SemanticRecording),
) -> Vec<u8> {
    let mut changed = recording.clone();
    change(&mut changed);
    changed.encode()
}

fn last_command(recording: &mut SemanticRecording) -> &mut recording::RecordedCommand {
    recording
        .ticks
        .iter_mut()
        .rev()
        .find_map(|tick| tick.commands.last_mut())
        .unwrap()
}

#[test]
fn recording_import_rejects_incompatible_vocabulary_and_values_before_effects() {
    let raw = complete_trajectory();
    let mut live = Harness::new(Bindings::OriginalV1, false, false);
    live.play(&raw);
    let semantic = SemanticRecording::capture(live.capture());
    live.finish();
    let mut replay = Harness::new(Bindings::ReboundV1, true, false);
    let invalid = vec![
        (
            ImportError::Version,
            changed_semantic(&semantic, |recording| recording.format_version += 1),
        ),
        (
            ImportError::Vocabulary,
            changed_semantic(&semantic, |recording| recording.vocabulary.revision += 1),
        ),
        (
            ImportError::Vocabulary,
            changed_semantic(&semantic, |recording| {
                recording.vocabulary.entries[0].value_kind = ValueKind::Button
            }),
        ),
        (
            ImportError::Identity,
            changed_semantic(&semantic, |recording| recording.players.swap(0, 1)),
        ),
        (
            ImportError::Ordering,
            changed_semantic(&semantic, |recording| last_command(recording).ordinal += 1),
        ),
        (
            ImportError::Identity,
            changed_semantic(&semantic, |recording| {
                last_command(recording).original_source =
                    GameplayCommandSource::ai("unknown-producer").unwrap();
            }),
        ),
        (
            ImportError::Vocabulary,
            changed_semantic(&semantic, |recording| {
                let entry = last_command(recording);
                entry.command = GameplayCommandDraft::new(
                    GameplayCommandTypeId::new("consumer.unknown.v99").unwrap(),
                )
                .with_target(entry.command.target().unwrap().clone())
                .with_payload(entry.command.payload().clone());
            }),
        ),
        (
            ImportError::ValueKind,
            changed_semantic(&semantic, |recording| {
                let entry = last_command(recording);
                entry.command =
                    GameplayCommandDraft::new(GameplayCommandTypeId::new(MOVE_COMMAND).unwrap())
                        .with_target(entry.command.target().unwrap().clone())
                        .with_payload(entry.command.payload().clone());
            }),
        ),
        (
            ImportError::ValueKind,
            changed_semantic(&semantic, |recording| {
                let entry = &mut recording.ticks[0].commands[0];
                let mut payload = entry.command.payload().clone();
                payload
                    .insert("value", GameplayCommandValue::F64(2.0))
                    .unwrap();
                entry.command = GameplayCommandDraft::new(entry.command.command_type().clone())
                    .with_target(entry.command.target().unwrap().clone())
                    .with_payload(payload);
            }),
        ),
        (
            ImportError::Identity,
            changed_semantic(&semantic, |recording| {
                let entry = &mut recording.ticks[0].commands[0];
                let mut payload = entry.command.payload().clone();
                payload
                    .insert("seat", GameplayCommandValue::I64(404))
                    .unwrap();
                entry.command = GameplayCommandDraft::new(entry.command.command_type().clone())
                    .with_target(entry.command.target().unwrap().clone())
                    .with_payload(payload);
            }),
        ),
    ];
    for (expected, bytes) in invalid {
        let before = replay
            .runtime()
            .world()
            .resource::<GameplayCommandQueue>()
            .stats();
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
    let mut unknown_field = serde_json::to_value(&semantic).unwrap();
    unknown_field["unrecognized_extension"] = serde_json::json!(true);
    assert_eq!(
        replay.import_semantic(&serde_json::to_vec(&unknown_field).unwrap()),
        Err(ImportError::Encoding)
    );
    let mut unknown_target = serde_json::to_value(&semantic).unwrap();
    unknown_target["ticks"][0]["commands"][0]["command"]["target"]["Named"] =
        serde_json::json!("player.unknown");
    assert_eq!(
        replay.import_semantic(&serde_json::to_vec(&unknown_target).unwrap()),
        Err(ImportError::Identity)
    );
    replay.drive(fixed_nanos(1), 1, "all-invalid-recordings-had-no-effects");
    assert_eq!(
        state(replay.runtime()).players,
        GameState::default().players
    );
    assert!(replay.capture()[0].commands.is_empty());
    replay.finish();

    assert_eq!(
        RawRecording::load(&raw.encode(), Bindings::ReboundV1),
        Err(ImportError::PhysicalPolicy)
    );
    let mut invalid_axis = serde_json::to_value(&raw).unwrap();
    let operations = invalid_axis["operations"].as_array_mut().unwrap();
    let axis = operations
        .iter_mut()
        .find(|operation| operation["op"] == "axis")
        .unwrap();
    axis["value"] = serde_json::json!(1.25);
    assert_eq!(
        RawRecording::load(
            &serde_json::to_vec(&invalid_axis).unwrap(),
            Bindings::OriginalV1
        ),
        Err(ImportError::Encoding)
    );
    let mut unknown_operation = serde_json::to_value(&raw).unwrap();
    unknown_operation["operations"][0]["op"] = serde_json::json!("native_device_handle");
    assert_eq!(
        RawRecording::load(
            &serde_json::to_vec(&unknown_operation).unwrap(),
            Bindings::OriginalV1
        ),
        Err(ImportError::Encoding)
    );
}

#[test]
fn replay_queue_rejects_actual_local_drafts_before_fixed_gameplay() {
    let mut replay = Harness::new(Bindings::ReboundV1, true, true);
    assert_eq!(
        *replay.runtime().world().resource::<Producers>(),
        Producers::RECORDED
    );
    assert_eq!(
        replay
            .runtime()
            .world()
            .resource::<LocalGameplayCommands>()
            .len(),
        1
    );
    let before = replay
        .runtime()
        .world()
        .resource::<GameplayCommandQueue>()
        .stats();
    assert!(replay.started.runtime.drive(Duration::ZERO).is_err());
    assert_eq!(replay.runtime().state(), RuntimeState::Faulted);
    assert_eq!(
        replay.runtime().fault().unwrap().kind(),
        RuntimeFaultKind::LocalIntentLoss
    );
    assert_eq!(state(replay.runtime()), GameState::default());
    assert!(replay.capture().is_empty());
    assert!(
        replay
            .runtime()
            .world()
            .resource::<LocalGameplayCommands>()
            .is_empty()
    );
    let after = replay
        .runtime()
        .world()
        .resource::<GameplayCommandQueue>()
        .stats();
    assert_eq!(after.accepted, before.accepted);
    assert_eq!(after.retained_commands, 0);
    assert_eq!(after.retained_bytes, 0);
    assert_eq!(after.pending_commands, 0);
    assert_eq!(after.closed_through_tick, 0);
    assert_eq!(after.rejected, before.rejected + 1);
    replay.finish();
}
