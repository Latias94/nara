#![forbid(unsafe_code)]

#[path = "support/resume_gameplay.rs"]
mod support;

use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use nara::{
    app::{FixedTime, PluginCategory, PluginDeclaration, PluginId, RuntimeState},
    gameplay::GameplayCommandDraft,
    prelude::{App, Plugin, PluginError},
    project_host::HeadlessSessionError,
};
use support::{
    DEADLINE, Game, INSTANCE, MARKER, MAX_WALK_STEPS, ProjectCopy, TRAVEL, action, movement,
};
use two_level_game::{
    GamePhase, GameResumeError, RunOptions, Scenario, capture_resume, configured_recipe,
    levels::{LevelId, LevelKey},
    product_recipe, request_resume,
    resume::{
        CURRENT_FORMAT_VERSION, GAME_COMPATIBILITY, ResumeError, ResumeState, decode_resume,
        encode_resume,
    },
};

fn collect_near_pickup(game: &mut Game, score: u32) {
    let deadline = Instant::now() + DEADLINE;
    game.command(movement(1, true));
    for _ in 0..MAX_WALK_STEPS {
        if game.snapshot().score == score {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "walking to the authored pickup timed out"
        );
        assert!(game.snapshot().score < score);
        game.step();
    }
    let snapshot = game.snapshot();
    assert_eq!(snapshot.score, score, "{snapshot:?}");
    assert_eq!(snapshot.visible_pickups, 1);
    assert_eq!(snapshot.held, [false, true, false, false]);
}

fn meaningful_game(game: &mut Game) -> ResumeState {
    let anchor = game.snapshot().player.to_array();
    assert_eq!(game.snapshot().level, Some(LevelKey::main(LevelId::Meadow)));
    collect_near_pickup(game, 10);
    assert_eq!(game.command(action(MARKER)).markers[0].get(), 1);
    let cavern = game.command(action(TRAVEL));
    assert_eq!(cavern.level, Some(LevelKey::main(LevelId::Cavern)));
    assert_eq!(cavern.held, [false; 4]);
    assert_eq!(cavern.visible_pickups, 2);
    collect_near_pickup(game, 20);
    assert_eq!(game.command(action(MARKER)).markers[0].get(), 2);
    let second = LevelKey::new(LevelId::Cavern, 1).unwrap();
    assert_eq!(game.command(action(INSTANCE)).level, Some(second));
    collect_near_pickup(game, 30);
    assert_eq!(game.command(action(MARKER)).markers[0].get(), 3);

    let state = game.capture();
    assert_ne!(state.player.translation, anchor);
    assert_eq!(state.held, [false, true, false, false]);
    assert_eq!(state.progress.location.level, second);
    assert_eq!(state.progress.score, 30);
    assert_eq!(state.progress.collected.len(), 3);
    assert_eq!(state.progress.markers.len(), 3);
    assert_eq!(state.progress.next_marker_id.get(), 4);
    for (index, level) in [
        LevelKey::main(LevelId::Meadow),
        LevelKey::main(LevelId::Cavern),
        second,
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            state
                .progress
                .collected
                .iter()
                .any(|pickup| pickup.level == level)
        );
        assert!(
            state
                .progress
                .markers
                .iter()
                .any(|marker| { marker.level == level && marker.id.get() == index as u64 + 1 })
        );
    }
    assert_eq!(state.compatibility.levels.len(), 2);
    assert_eq!(state.counters.travel_count, 2);
    assert_eq!(state.counters.restores, 0);
    assert!(state.clock.ticks > 0);
    assert_eq!(
        state.clock.elapsed_nanos,
        state.clock.ticks * state.clock.timestep_nanos
    );
    state
}

fn restore_candidate(original: &Game, candidate: &mut Game, state: &ResumeState) {
    let retained = original.frozen();
    let fresh = candidate.frozen();
    assert_eq!(fresh.tick, 0);
    assert_ne!(retained.generation, fresh.generation);
    request_resume(&mut candidate.session, state).expect("public reconstruction request");
    assert_eq!(candidate.session.state(), Some(RuntimeState::Paused));
    assert_eq!(
        candidate.tick(),
        1,
        "only the bounded resume command may execute"
    );
    assert!(candidate.snapshot().restoring);
    assert!(matches!(
        capture_resume(&candidate.session),
        Err(GameResumeError::Ineligible(_))
    ));

    let deadline = Instant::now() + DEADLINE;
    while candidate.snapshot().restoring {
        assert_eq!(
            original.frozen(),
            retained,
            "incumbent changed during reconstruction"
        );
        assert!(
            candidate.snapshot().error.is_none(),
            "{:?}",
            candidate.snapshot()
        );
        assert!(
            Instant::now() < deadline,
            "candidate reconstruction timed out: {:?}",
            candidate.snapshot()
        );
        candidate.maintenance();
        std::thread::sleep(Duration::from_millis(1));
    }
    let reconstructed = candidate.settle();
    assert_eq!(reconstructed.phase, GamePhase::Running);
    assert!(reconstructed.error.is_none());
    assert_eq!(candidate.tick(), 1);
    assert_eq!(
        candidate.capture(),
        *state,
        "all semantic state must match before continuation"
    );
    assert_eq!(
        encode_resume(&candidate.capture()).unwrap(),
        encode_resume(state).unwrap()
    );
    let (elapsed, timestep, remainder, debt) = candidate
        .session
        .observe(|world| {
            let time = world.resource::<FixedTime>();
            (
                time.elapsed(),
                time.timestep(),
                time.remainder(),
                time.debt(),
            )
        })
        .unwrap();
    assert_eq!(elapsed, timestep, "the Runtime clock must remain fresh");
    assert_eq!(remainder, Duration::ZERO);
    assert_eq!(debt, Duration::ZERO);
    assert_ne!(candidate.frozen().domain, retained.domain);
    assert_ne!(
        (candidate.frozen().domain, reconstructed.current_instance),
        (retained.domain, Some(retained.instance)),
    );
    assert_ne!(reconstructed.current_instance, Some(fresh.instance));
    assert_eq!(
        candidate.session.runtime_generation(),
        Some(fresh.generation)
    );
    assert_eq!(original.frozen(), retained);
}

fn compare_games(original: &Game, candidate: &Game) -> ResumeState {
    let state = original.capture();
    assert_eq!(candidate.capture(), state);
    assert_eq!(
        encode_resume(&candidate.capture()).unwrap(),
        encode_resume(&state).unwrap()
    );
    let a = original.snapshot();
    let b = candidate.snapshot();
    assert_eq!(a.visible_pickups, b.visible_pickups);
    assert_eq!(a.markers, b.markers);
    assert_eq!(a.scene_members, b.scene_members);
    assert_ne!(a.current_instance, b.current_instance);
    assert_ne!(
        original.session.runtime_generation(),
        candidate.session.runtime_generation()
    );
    state
}

fn command_pair(
    original: &mut Game,
    candidate: &mut Game,
    command: GameplayCommandDraft,
) -> ResumeState {
    original.command(command.clone());
    candidate.command(command);
    compare_games(original, candidate)
}

fn step_pair(original: &mut Game, candidate: &mut Game) -> ResumeState {
    original.step();
    candidate.step();
    compare_games(original, candidate)
}

fn reject_candidate(original: &Game, candidate: &mut Game, state: &ResumeState) -> String {
    let retained = original.frozen();
    let generation = candidate.session.runtime_generation();
    let reason = match request_resume(&mut candidate.session, state) {
        Err(GameResumeError::Ineligible(reason)) => reason,
        Err(error) => panic!("expected a game compatibility refusal, got {error:?}"),
        Ok(()) => {
            let deadline = Instant::now() + DEADLINE;
            loop {
                assert_eq!(original.frozen(), retained);
                let snapshot = candidate.snapshot();
                if let Some(error) = &snapshot.error {
                    break error.clone();
                }
                assert!(
                    snapshot.restoring,
                    "incompatible state was accepted: {snapshot:?}"
                );
                assert!(
                    Instant::now() < deadline,
                    "incompatible candidate did not finish: {snapshot:?}"
                );
                candidate.maintenance();
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    };
    assert_eq!(candidate.session.state(), Some(RuntimeState::Paused));
    assert_eq!(candidate.session.runtime_generation(), generation);
    assert!(
        !candidate.snapshot().restoring,
        "failed reconstruction must clear pending state"
    );
    assert!(matches!(
        capture_resume(&candidate.session),
        Err(GameResumeError::Ineligible(_))
    ));
    assert_eq!(original.frozen(), retained);
    reason
}

#[test]
fn startup_first_travel_and_returned_meadow_share_runtime_content_compatibility() {
    let mut original = Game::source();
    for expected in [LevelId::Meadow, LevelId::Cavern, LevelId::Meadow] {
        assert_eq!(original.snapshot().level, Some(LevelKey::main(expected)));
        let state = original.capture();
        let mut candidate = Game::source();
        restore_candidate(&original, &mut candidate, &state);
        assert_eq!(candidate.capture(), state);
        candidate.close();
        if state.counters.travel_count < 2 {
            original.command(action(TRAVEL));
        }
    }
    original.close();
}

#[test]
fn public_resume_reconstructs_live_three_instance_state_and_continues_every_semantic_step() {
    let mut original = Game::source();
    let captured = meaningful_game(&mut original);
    let bytes = encode_resume(&captured).unwrap();
    let decoded = decode_resume(&bytes).unwrap();
    assert_eq!(decoded, captured);
    assert_eq!(encode_resume(&decoded).unwrap(), bytes);
    let mut reordered = captured.clone();
    reordered.progress.collected.reverse();
    reordered.progress.markers.reverse();
    reordered.compatibility.levels.reverse();
    assert_eq!(encode_resume(&reordered).unwrap(), bytes);

    let mut candidate = Game::source();
    restore_candidate(&original, &mut candidate, &decoded);
    let tick_offset = original.tick() - candidate.tick();
    assert!(tick_offset > 0, "engine time must remain fresh");
    let resumed = step_pair(&mut original, &mut candidate);
    assert!(resumed.player.translation[0] > captured.player.translation[0]);
    assert_eq!(
        resumed.held, captured.held,
        "held input must continue without resubmission"
    );
    assert_eq!(resumed.clock.ticks, captured.clock.ticks + 1);
    let released = command_pair(&mut original, &mut candidate, movement(1, false));
    assert_eq!(released.player, resumed.player);
    assert_eq!(released.held, [false; 4]);

    command_pair(&mut original, &mut candidate, movement(2, true));
    let deadline = Instant::now() + DEADLINE;
    for _ in 0..MAX_WALK_STEPS {
        if original.snapshot().player.y >= 40.0 {
            break;
        }
        assert!(Instant::now() < deadline);
        step_pair(&mut original, &mut candidate);
    }
    assert!((40.0..=44.0).contains(&original.snapshot().player.y));
    command_pair(&mut original, &mut candidate, movement(2, false));
    command_pair(&mut original, &mut candidate, movement(1, true));
    let deadline = Instant::now() + DEADLINE;
    for _ in 0..MAX_WALK_STEPS {
        if original.snapshot().score == 40 {
            break;
        }
        assert!(Instant::now() < deadline);
        step_pair(&mut original, &mut candidate);
    }
    assert_eq!(
        original.snapshot().score,
        40,
        "fresh continuation must collect gem-far"
    );
    assert_eq!(original.snapshot().visible_pickups, 0);
    command_pair(&mut original, &mut candidate, movement(1, false));
    let marked = command_pair(&mut original, &mut candidate, action(MARKER));
    assert_eq!(marked.progress.markers.last().unwrap().id.get(), 4);
    assert_eq!(marked.progress.next_marker_id.get(), 5);

    let main_cavern = command_pair(&mut original, &mut candidate, action(INSTANCE));
    assert_eq!(
        main_cavern.progress.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    assert_eq!(
        original
            .snapshot()
            .markers
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [2]
    );
    let meadow = command_pair(&mut original, &mut candidate, action(TRAVEL));
    assert_eq!(
        meadow.progress.location.level,
        LevelKey::main(LevelId::Meadow)
    );
    assert_eq!(
        original
            .snapshot()
            .markers
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [1]
    );
    let meadow_marker = command_pair(&mut original, &mut candidate, action(MARKER));
    assert_eq!(meadow_marker.progress.markers.last().unwrap().id.get(), 5);
    command_pair(&mut original, &mut candidate, action(TRAVEL));
    let revisited = command_pair(&mut original, &mut candidate, action(INSTANCE));
    assert_eq!(
        revisited.progress.location.level,
        LevelKey::new(LevelId::Cavern, 1).unwrap()
    );
    assert_eq!(
        original
            .snapshot()
            .markers
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [3, 4]
    );
    assert_eq!(original.snapshot().visible_pickups, 0);
    assert_eq!(revisited.progress.score, 40);
    assert_eq!(revisited.counters.travel_count, 6);
    assert_eq!(original.tick() - candidate.tick(), tick_offset);
    candidate.close();
    original.close();
}

#[test]
fn capture_refuses_future_commands_without_consuming_the_original_queue() {
    let mut original = Game::source();
    let state = meaningful_game(&mut original);
    let tick = original.tick();
    let before = original.snapshot();
    let prior_queue = original.queue();
    original.submit_at(tick + 3, action(MARKER));
    let pending = original.queue();
    assert_eq!(pending.accepted, prior_queue.accepted + 1);
    assert_eq!(pending.pending_commands, 1);
    assert_eq!(pending.furthest_pending_tick, Some(tick + 3));
    for _ in 0..2 {
        assert!(matches!(
            capture_resume(&original.session),
            Err(GameResumeError::Ineligible(reason)) if reason.contains("commands")
        ));
        assert_eq!(original.queue(), pending);
        assert_eq!(original.tick(), tick);
        assert_eq!(original.snapshot().player, before.player);
        assert_eq!(original.snapshot().logical_tick, before.logical_tick);
        assert_eq!(original.snapshot().markers, before.markers);
        original.maintenance();
    }
    for expected in [tick + 1, tick + 2] {
        assert_eq!(original.session.step().unwrap(), None);
        assert_eq!(original.tick(), expected);
        assert_eq!(original.queue().pending_commands, 1);
        assert_eq!(original.queue().acknowledged, prior_queue.acknowledged);
        assert!(matches!(
            capture_resume(&original.session),
            Err(GameResumeError::Ineligible(reason)) if reason.contains("commands")
        ));
    }
    original.step();
    let continued = original.capture();
    assert_eq!(original.tick(), tick + 3);
    assert_eq!(original.queue().pending_commands, 0);
    assert_eq!(original.queue().retained_commands, 0);
    assert_eq!(original.queue().acknowledged, prior_queue.acknowledged + 1);
    assert_eq!(continued.clock.ticks, state.clock.ticks + 3);
    assert_eq!(
        continued.progress.markers.len(),
        state.progress.markers.len() + 1
    );
    assert_eq!(
        continued.progress.markers.last().unwrap().id,
        state.progress.next_marker_id
    );
    original.close();
}

#[test]
fn incompatible_wire_schema_and_configuration_preserve_the_paused_original_byte_exactly() {
    let project = ProjectCopy::new();
    let mut original = Game::source();
    let state = meaningful_game(&mut original);
    let retained = original.frozen();

    for (field, version) in [
        ("format_version", CURRENT_FORMAT_VERSION + 1),
        ("game_compatibility", GAME_COMPATIBILITY + 1),
    ] {
        let mut candidate = Game::source();
        let fresh = candidate.frozen();
        let mut wire: serde_json::Value = serde_json::from_slice(&retained.bytes).unwrap();
        wire[field] = serde_json::json!(version);
        let refused = decode_resume(&serde_json::to_vec(&wire).unwrap()).unwrap_err();
        match (field, refused) {
            ("format_version", ResumeError::UnsupportedVersion(actual))
            | ("game_compatibility", ResumeError::UnsupportedGameCompatibility(actual)) => {
                assert_eq!(actual, version)
            }
            (_, error) => panic!("wrong format refusal: {error:?}"),
        }
        assert_eq!(
            candidate.frozen(),
            fresh,
            "invalid wire must not reach a candidate"
        );
        assert_eq!(original.frozen(), retained);
        candidate.close();
    }
    for schema in [true, false] {
        let mut incompatible = state.clone();
        if schema {
            incompatible.compatibility.schema[0] ^= 1;
        } else {
            incompatible.compatibility.configuration[0] ^= 1;
        }
        incompatible.validate().unwrap();
        let mut candidate = Game::source();
        let reason = reject_candidate(&original, &mut candidate, &incompatible);
        assert!(
            reason.contains(if schema { "Schema" } else { "configuration" }),
            "{reason}"
        );
        candidate.close();
        assert_eq!(original.frozen(), retained);
    }
    project.change_fixed_timestep();
    let mut configured_candidate = Game::from_project(project.root());
    let configured = configured_candidate.capture();
    assert_eq!(configured.compatibility.schema, state.compatibility.schema);
    assert_ne!(configured.clock.timestep_nanos, state.clock.timestep_nanos);
    assert_ne!(
        configured.compatibility.configuration,
        state.compatibility.configuration
    );
    let reason = reject_candidate(&original, &mut configured_candidate, &state);
    assert!(reason.contains("configuration"), "{reason}");
    configured_candidate.close();
    assert_eq!(original.frozen(), retained);

    let continued = original.command(movement(1, false));
    assert_eq!(continued.logical_tick, state.clock.ticks + 1);
    assert_eq!(original.capture().progress, state.progress);
    original.close();
}

#[test]
fn changed_cavern_content_in_a_verified_copy_refuses_resume_and_leaves_original_playable() {
    let project = ProjectCopy::new();
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("scenes/cavern.scene.json");
    let source_bytes = fs::read(&source_path).unwrap();
    let mut original = Game::source();
    let state = meaningful_game(&mut original);
    let retained = original.frozen();

    let mut unchanged = Game::from_project(project.root());
    restore_candidate(&original, &mut unchanged, &state);
    unchanged.close();
    project.change_cavern_pickup();
    assert_eq!(fs::read(&source_path).unwrap(), source_bytes);
    assert_ne!(
        fs::read(project.root().join("scenes/cavern.scene.json")).unwrap(),
        source_bytes
    );
    let mut candidate = Game::from_project(project.root());
    let fresh = candidate.capture();
    assert_eq!(fresh.compatibility.schema, state.compatibility.schema);
    assert_eq!(
        fresh.compatibility.configuration,
        state.compatibility.configuration
    );
    let reason = reject_candidate(&original, &mut candidate, &state);
    assert!(reason.contains("content"), "{reason}");
    assert_eq!(original.frozen(), retained);
    let retried = candidate.command(action("two-level-game.retry"));
    assert_eq!(retried.phase, GamePhase::Running);
    assert_eq!(retried.restarts, 1);
    assert!(retried.error.is_none());
    assert!(!retried.restoring);
    assert!(matches!(
        capture_resume(&candidate.session),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("rejected")
    ));
    assert_eq!(original.frozen(), retained);
    let moved = candidate.command(movement(1, true));
    assert!(moved.player.x > retried.player.x);
    assert_eq!(moved.held, [false, true, false, false]);
    assert_eq!(moved.phase, GamePhase::Running);
    assert!(moved.error.is_none());
    assert!(!moved.restoring);
    assert!(matches!(
        capture_resume(&candidate.session),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("rejected")
    ));
    assert_eq!(original.frozen(), retained);
    candidate.close();
    assert_eq!(original.frozen(), retained);
    assert_eq!(fs::read(source_path).unwrap(), source_bytes);
    let continued = original.command(action(MARKER));
    assert_eq!(
        continued
            .markers
            .iter()
            .map(|id| id.get())
            .collect::<Vec<_>>(),
        [3, 4]
    );
    assert_eq!(original.capture().progress.score, state.progress.score);
    original.close();
}

#[test]
fn resume_requires_a_fresh_candidate_with_an_empty_original_command_queue() {
    let mut original = Game::source();
    let state = meaningful_game(&mut original);
    let retained = original.frozen();
    let mut candidate = Game::source();
    candidate.submit(movement(1, true));
    let pending = candidate.queue();
    assert!(matches!(
        request_resume(&mut candidate.session, &state),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("commands")
    ));
    assert_eq!(candidate.tick(), 0);
    assert_eq!(candidate.queue(), pending);
    assert_eq!(candidate.snapshot().held, [false; 4]);
    candidate.step();
    assert_eq!(candidate.capture().held, [false, true, false, false]);
    let nonfresh = candidate.frozen();
    assert!(matches!(
        request_resume(&mut candidate.session, &state),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("fresh")
    ));
    assert_eq!(candidate.frozen(), nonfresh);
    assert_eq!(original.frozen(), retained);
    candidate.close();
    original.command(movement(1, false));
    assert_eq!(original.capture().clock.ticks, state.clock.ticks + 1);
    original.close();
}

#[derive(Default)]
struct UnknownResumeService;

impl Plugin for UnknownResumeService {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test-unsupported-resume-service"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[test]
fn capture_refuses_running_sessions_and_additional_custom_plugins() {
    let mut game = Game::source();
    let retained = game.frozen();
    assert_eq!(game.session.resume().unwrap(), None);
    assert!(matches!(
        capture_resume(&game.session),
        Err(GameResumeError::Session(HeadlessSessionError::NotPaused))
    ));
    assert_eq!(game.session.pause().unwrap(), None);
    assert_eq!(game.frozen(), retained);
    game.close();

    let recipe = product_recipe()
        .add_plugin::<UnknownResumeService>()
        .unwrap();
    let mut unsupported = Game::with_recipe(recipe);
    let tick = unsupported.tick();
    let queue = unsupported.queue();
    assert!(matches!(
        capture_resume(&unsupported.session),
        Err(GameResumeError::Ineligible(reason))
            if reason.contains("unsupported services")
                && reason.contains("two-level-game.test-unsupported-resume-service")
    ));
    assert_eq!(unsupported.tick(), tick);
    assert_eq!(unsupported.queue(), queue);
    let continued = unsupported.command(movement(1, true));
    assert!(continued.player.x > -120.0);
    assert_eq!(continued.phase, GamePhase::Running);
    assert!(continued.error.is_none());
    assert!(matches!(
        capture_resume(&unsupported.session),
        Err(GameResumeError::Ineligible(reason))
            if reason.contains("two-level-game.test-unsupported-resume-service")
    ));
    unsupported.close();
}

#[test]
fn capture_refuses_fractional_runtime_time_without_discarding_it() {
    let mut game = Game::source();
    game.command(movement(1, true));
    let state = game.capture();
    let tick = game.tick();
    let fraction = Duration::from_nanos(1);
    assert_eq!(game.session.resume().unwrap(), None);
    assert_eq!(game.session.frame(fraction).unwrap(), None);
    assert_eq!(game.session.pause().unwrap(), None);
    assert_eq!(game.tick(), tick);
    assert_eq!(game.snapshot().logical_tick, state.clock.ticks);
    assert_eq!(game.snapshot().player.to_array(), state.player.translation);
    let time = game
        .session
        .observe(|world| *world.resource::<FixedTime>())
        .unwrap();
    assert_eq!(time.remainder(), fraction);
    assert_eq!(time.debt(), Duration::ZERO);
    let queue = game.queue();
    assert!(matches!(
        capture_resume(&game.session),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("remainder")
    ));
    assert_eq!(game.queue(), queue);
    assert_eq!(
        game.session
            .observe(|world| *world.resource::<FixedTime>())
            .unwrap(),
        time
    );

    assert_eq!(game.session.resume().unwrap(), None);
    assert_eq!(
        game.session.frame(time.timestep() - fraction).unwrap(),
        None
    );
    assert_eq!(game.session.pause().unwrap(), None);
    game.settle();
    let continued = game.capture();
    assert_eq!(game.tick(), tick + 1);
    assert_eq!(continued.clock.ticks, state.clock.ticks + 1);
    assert_eq!(continued.held, state.held);
    assert!(continued.player.translation[0] > state.player.translation[0]);
    game.close();
}

#[test]
fn capture_refuses_an_open_save_slot_without_disabling_ordinary_gameplay() {
    let project = ProjectCopy::new();
    let recipe = configured_recipe(RunOptions {
        scenario: Scenario::Interactive,
        save_path: Some(project.save_path()),
    });
    let mut game = Game::from_project_with_recipe(project.root(), recipe);
    let tick = game.tick();
    let queue = game.queue();
    assert!(matches!(
        capture_resume(&game.session),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("SaveSlot")
    ));
    assert_eq!(game.tick(), tick);
    assert_eq!(game.queue(), queue);
    let continued = game.command(movement(1, true));
    assert_eq!(continued.phase, GamePhase::Running);
    assert!(continued.error.is_none());
    assert!(continued.player.x > -120.0);
    assert!(matches!(
        capture_resume(&game.session),
        Err(GameResumeError::Ineligible(reason)) if reason.contains("SaveSlot")
    ));
    game.close();
}
