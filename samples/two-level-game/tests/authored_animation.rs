#![cfg(any(feature = "development", feature = "cook"))]
#![forbid(unsafe_code)]

#[allow(
    dead_code,
    reason = "shared game driver also serves other resume tests"
)]
#[path = "support/resume_gameplay.rs"]
mod support;

use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::CurrentScene,
    app::RuntimeState,
    ecs::Entity,
    identity::{EntityLookup, SceneEntityId},
    prelude::Transform2d,
    sprite::Sprite,
};
use serde_json::{Value, json};
use support::{DEADLINE, Game, MARKER, ProjectCopy, TRAVEL, action, movement};
use two_level_game::{
    GamePhase, GameResumeError, capture_resume,
    levels::{AUTHORED_LEVEL_MEMBERS, LevelId, LevelKey},
    request_resume,
    resume::{ResumeState, encode_resume},
    save::{MarkerId, MarkerRecord},
};

const CLIPS_FILE: &str = "assets/animations/explorer.sprite-animations.json";
const TIMESTEP_NANOS: u64 = 20_000_000;

fn clips_file(project: &ProjectCopy) -> PathBuf {
    let root = fs::canonicalize(project.root()).unwrap();
    let source = fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
    let file = fs::canonicalize(project.root().join(CLIPS_FILE)).unwrap();
    assert!(root.is_absolute() && file.starts_with(&root));
    assert_ne!(root, source);
    assert!(!file.starts_with(&source));
    file
}

fn write_clips(project: &ProjectCopy, data: &Value) {
    let file = clips_file(project);
    let bytes = serde_json::to_vec_pretty(data).unwrap();
    fs::write(&file, &bytes).unwrap();
    assert_eq!(fs::read(file).unwrap(), bytes);
}

fn unequal_clips() -> Value {
    json!({
        "format_version": 1,
        "clips": [
            {
                "name": "idle",
                "mode": "once",
                "frames": [
                    { "min": [0.5, 0.0], "size": [0.25, 1.0], "duration_nanos": 100_000_000 }
                ]
            },
            {
                "name": "walk",
                "mode": "loop",
                "frames": [
                    { "min": [0.75, 0.0], "size": [0.25, 1.0], "duration_nanos": 40_000_000 },
                    { "min": [0.25, 0.0], "size": [0.25, 1.0], "duration_nanos": 60_000_000 },
                    { "min": [0.5, 0.0], "size": [0.25, 1.0], "duration_nanos": 80_000_000 }
                ]
            }
        ]
    })
}

fn actual_explorer(game: &Game) -> (Entity, Transform2d, Sprite) {
    let snapshot = game.snapshot();
    game.session
        .observe(|world| {
            let current = CurrentScene::get(world).expect("the game owns its current Scene");
            assert_eq!(
                snapshot.current_instance,
                Some(current.receipt().instance_id())
            );
            let id = SceneEntityId::new("explorer").unwrap();
            let entity = match current.receipt().resolve(world, &id) {
                EntityLookup::Resolved(entity) => entity,
                other => panic!("the current Scene must resolve its explorer: {other:?}"),
            };
            let pose = *world
                .get::<Transform2d>(entity)
                .expect("the explorer has a pose");
            assert_eq!(pose.translation, snapshot.player);
            let sprite = world
                .get::<Sprite>(entity)
                .expect("the explorer has a Sprite")
                .clone();
            assert!(sprite.material.image.is_some());
            (entity, pose, sprite)
        })
        .unwrap()
}

fn assert_column(game: &Game, column: u32) -> Sprite {
    let sprite = actual_explorer(game).2;
    let region = sprite
        .texture_region
        .expect("the actual Sprite must project its authored frame");
    assert_eq!(region.min.to_array(), [column as f32 * 0.25, 0.0]);
    assert_eq!(region.size.to_array(), [0.25, 1.0]);
    sprite
}

fn assert_equivalent(original: &Game, candidate: &Game) -> ResumeState {
    let state = original.capture();
    let restored = candidate.capture();
    assert_eq!(restored, state);
    assert_eq!(
        encode_resume(&restored).unwrap(),
        encode_resume(&state).unwrap(),
        "both games must retain the same complete canonical state"
    );
    let (_, original_pose, mut original_sprite) = actual_explorer(original);
    let (_, candidate_pose, candidate_sprite) = actual_explorer(candidate);
    assert_eq!(candidate_pose, original_pose);
    original_sprite.material.image = candidate_sprite.material.image;
    assert_eq!(candidate_sprite, original_sprite);
    let original_snapshot = original.snapshot();
    let candidate_snapshot = candidate.snapshot();
    assert_eq!(
        candidate_snapshot.visible_pickups,
        original_snapshot.visible_pickups
    );
    assert_eq!(candidate_snapshot.markers, original_snapshot.markers);
    assert_eq!(
        candidate_snapshot.scene_members,
        original_snapshot.scene_members
    );
    state
}

#[test]
fn authored_walk_order_and_unequal_boundaries_project_actual_uvs_and_the_changed_idle() {
    let project = ProjectCopy::new();
    write_clips(&project, &unequal_clips());
    let mut game = Game::from_project(project.root());
    assert_eq!(game.capture().clock.timestep_nanos, TIMESTEP_NANOS);
    let idle = assert_column(&game, 2);
    game.command(movement(1, true));

    for (target_tick, column) in [
        (1, 3),
        (2, 1),
        (3, 1),
        (4, 1),
        (5, 2),
        (6, 2),
        (7, 2),
        (8, 2),
        (9, 3),
    ] {
        for tick in game.tick() + 1..=target_tick {
            assert_eq!(game.step().logical_tick, tick);
        }
        let state = game.capture();
        assert_eq!(state.clock.ticks, target_tick);
        assert_eq!(state.clock.elapsed_nanos, target_tick * TIMESTEP_NANOS);
        assert_eq!(state.held, [false, true, false, false]);
        let sprite = assert_column(&game, column);
        let mut expected = idle.clone();
        expected.texture_region = sprite.texture_region;
        assert_eq!(sprite, expected);
    }

    let position = game.snapshot().player;
    game.command(movement(1, false));
    assert_eq!(game.snapshot().player, position);
    assert_eq!(assert_column(&game, 2), idle);
    game.step();
    assert_eq!(assert_column(&game, 2), idle);
    game.command(movement(1, true));
    assert_eq!(game.capture().clock.elapsed_nanos, 240_000_000);
    assert_column(&game, 1);
    game.close();
}

#[test]
fn authored_one_shot_holds_its_final_frame_and_only_exact_step_advances_paused_time() {
    let project = ProjectCopy::new();
    let mut data = unequal_clips();
    data["clips"][1]["mode"] = json!("once");
    data["clips"][1]["frames"] = json!([
        { "min": [0.75, 0.0], "size": [0.25, 1.0], "duration_nanos": 40_000_000 },
        { "min": [0.5, 0.0], "size": [0.25, 1.0], "duration_nanos": 60_000_000 }
    ]);
    write_clips(&project, &data);
    let mut game = Game::from_project(project.root());
    game.command(movement(1, true));
    let before = game.capture();
    assert_eq!(before.clock.elapsed_nanos, TIMESTEP_NANOS);
    let frozen = game.frozen();
    let sprite = assert_column(&game, 3);

    game.maintenance();
    for delta in [
        Duration::ZERO,
        Duration::from_millis(100),
        Duration::from_secs(1),
    ] {
        assert_eq!(game.session.frame(delta).unwrap(), None);
        assert_eq!(game.session.state(), Some(RuntimeState::Paused));
        assert_eq!(game.frozen(), frozen);
        assert_eq!(assert_column(&game, 3), sprite);
    }

    game.step();
    let after = game.capture();
    assert_eq!(game.tick(), frozen.tick + 1);
    assert!((after.player.translation[0] - before.player.translation[0] - 3.2).abs() < 0.0001);
    let mut expected = before;
    expected.clock.ticks += 1;
    expected.clock.elapsed_nanos += TIMESTEP_NANOS;
    expected.player.translation[0] = after.player.translation[0];
    assert_eq!(after, expected);
    let last = assert_column(&game, 2);
    for target_tick in 3..=9 {
        game.step();
        let state = game.capture();
        assert_eq!(state.clock.ticks, target_tick);
        assert_eq!(state.clock.elapsed_nanos, target_tick * TIMESTEP_NANOS);
        assert_eq!(state.held, [false, true, false, false]);
        assert_eq!(assert_column(&game, 2), last);
    }
    assert_eq!(game.capture().clock.elapsed_nanos, 180_000_000);
    game.close();
}

#[test]
fn fresh_resume_projects_the_nonfirst_unequal_frame_before_continuing_commands_and_collision() {
    let project = ProjectCopy::new();
    write_clips(&project, &unequal_clips());
    let mut original = Game::from_project(project.root());
    original.command(action(MARKER));
    original.command(movement(1, true));
    original.step();
    original.step();
    let captured = original.capture();
    assert_eq!(captured.clock.ticks, 4);
    assert_eq!(captured.clock.elapsed_nanos, 80_000_000);
    assert_eq!(captured.progress.markers.len(), 1);
    assert_column(&original, 1);
    let retained = original.frozen();

    let mut candidate = Game::from_project(project.root());
    let fresh = candidate.frozen();
    assert_eq!(fresh.tick, 0);
    assert_ne!(fresh.generation, retained.generation);
    assert_ne!(fresh.domain, retained.domain);
    request_resume(&mut candidate.session, &captured).expect("ordinary public exact-state resume");
    candidate.settle();
    assert_eq!(candidate.tick(), 1, "resume must not add a gameplay step");
    assert_eq!(candidate.session.state(), Some(RuntimeState::Paused));
    assert_eq!(assert_equivalent(&original, &candidate), captured);
    assert_column(&candidate, 1);
    assert_ne!(candidate.snapshot().current_instance, Some(fresh.instance));
    assert_eq!(
        candidate.session.runtime_generation(),
        Some(fresh.generation)
    );
    assert_eq!(original.frozen(), retained);
    let tick_offset = original.tick() - candidate.tick();

    for column in [2, 2, 2, 2, 3, 3, 1] {
        original.step();
        candidate.step();
        assert_equivalent(&original, &candidate);
        assert_column(&original, column);
        assert_column(&candidate, column);
        assert_eq!(original.tick() - candidate.tick(), tick_offset);
    }
    for _ in 0..40 {
        original.step();
        candidate.step();
        assert_equivalent(&original, &candidate);
    }
    let blocked = original.capture();
    assert_eq!(blocked.progress.score, 10);
    assert_eq!(blocked.progress.collected.len(), 1);
    assert_eq!(original.snapshot().visible_pickups, 1);
    for column in [2, 2, 3] {
        original.step();
        candidate.step();
        let state = assert_equivalent(&original, &candidate);
        assert_eq!(state.player, blocked.player);
        assert_eq!(state.held, [false, true, false, false]);
        assert_eq!(state.progress, blocked.progress);
        assert_column(&original, column);
        assert_column(&candidate, column);
    }

    for game in [&mut original, &mut candidate] {
        game.command(action(MARKER));
        game.command(movement(1, false));
        assert_column(game, 2);
    }
    let marked = assert_equivalent(&original, &candidate);
    assert_eq!(marked.progress.markers.len(), 2);
    let previous_instance = original.snapshot().current_instance;
    for game in [&mut original, &mut candidate] {
        game.command(action(TRAVEL));
        assert_column(game, 2);
    }
    let travelled = assert_equivalent(&original, &candidate);
    assert_ne!(original.snapshot().current_instance, previous_instance);
    assert_eq!(
        travelled.progress.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    assert_eq!(travelled.clock, marked.clock);
    assert_eq!(travelled.progress.collected, marked.progress.collected);
    assert_eq!(travelled.progress.markers, marked.progress.markers);
    assert_eq!(travelled.counters.travel_count, 1);
    for game in [&mut original, &mut candidate] {
        game.command(action("two-level-game.retry"));
        assert_column(game, 2);
        game.command(movement(1, true));
        assert_column(game, 1);
    }
    let continued = assert_equivalent(&original, &candidate);
    assert_eq!(continued.progress, travelled.progress);
    assert_eq!(continued.counters.restarts, 1);
    assert_eq!(original.tick() - candidate.tick(), tick_offset);
    candidate.close();
    original.close();
}

#[test]
fn edited_valid_clips_refuse_original_state_while_the_loaded_resource_remains_usable() {
    let project = ProjectCopy::new();
    let mut data = unequal_clips();
    write_clips(&project, &data);
    let mut original = Game::from_project(project.root());
    original.command(action(MARKER));
    original.command(movement(1, true));
    let captured = original.capture();
    let retained = original.frozen();
    let sprite = assert_column(&original, 1);
    let original_stamp = original
        .session
        .observe(|world| {
            CurrentScene::get(world)
                .unwrap()
                .source_content_stamp()
                .unwrap()
        })
        .unwrap();

    data["clips"][0]["frames"][0]["min"] = json!([0.0, 0.0]);
    data["clips"][1]["frames"][0]["duration_nanos"] = json!(60_000_000);
    write_clips(&project, &data);
    original.maintenance();
    assert_eq!(original.frozen(), retained);
    assert_eq!(assert_column(&original, 1), sprite);

    let mut candidate = Game::from_project(project.root());
    let fresh = candidate.frozen();
    assert_column(&candidate, 0);
    let candidate_stamp = candidate
        .session
        .observe(|world| {
            CurrentScene::get(world)
                .unwrap()
                .source_content_stamp()
                .unwrap()
        })
        .unwrap();
    assert_ne!(
        candidate_stamp.content_digest(),
        original_stamp.content_digest(),
        "the Scene content stamp must include the selected animation bytes"
    );
    let reason = match request_resume(&mut candidate.session, &captured) {
        Err(GameResumeError::Ineligible(reason)) => reason,
        Err(error) => panic!("expected an animation compatibility refusal, got {error:?}"),
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
                    "edited clips were accepted: {snapshot:?}"
                );
                assert!(
                    Instant::now() < deadline,
                    "resume did not finish: {snapshot:?}"
                );
                candidate.maintenance();
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    };
    assert!(
        reason.contains("incompatible") || reason.contains("compatibility"),
        "{reason}"
    );
    assert_eq!(candidate.session.state(), Some(RuntimeState::Paused));
    assert_eq!(
        candidate.session.runtime_generation(),
        Some(fresh.generation)
    );
    assert!(!candidate.snapshot().restoring);
    assert!(matches!(
        capture_resume(&candidate.session),
        Err(GameResumeError::Ineligible(_))
    ));
    assert_eq!(original.frozen(), retained);
    candidate.close();

    for column in [1, 1, 2] {
        original.step();
        assert_column(&original, column);
    }
    let before_retry = original.capture();
    let instance = original.snapshot().current_instance;
    original.command(action("two-level-game.retry"));
    let retried = original.capture();
    assert_ne!(original.snapshot().current_instance, instance);
    assert_eq!(retried.progress, before_retry.progress);
    assert_eq!(retried.counters.restarts, 1);
    assert_column(&original, 2);
    original.close();
}

#[test]
fn malformed_or_missing_selected_clips_refuse_future_travel_without_replacing_the_old_game() {
    for missing in [false, true] {
        let project = ProjectCopy::new();
        let mut data = unequal_clips();
        write_clips(&project, &data);
        let mut game = Game::from_project(project.root());
        game.command(action(MARKER));
        game.command(movement(1, true));
        game.command(movement(1, false));
        let before = game.capture();
        let frozen = game.frozen();
        let actor = actual_explorer(&game);
        assert_column(&game, 2);

        if missing {
            fs::remove_file(clips_file(&project)).unwrap();
            assert!(!project.root().join(CLIPS_FILE).exists());
        } else {
            data["clips"][1]["frames"][0]["duration_nanos"] = json!(0);
            write_clips(&project, &data);
        }
        game.maintenance();
        assert_eq!(game.frozen(), frozen);
        assert_eq!(actual_explorer(&game), actor);

        game.submit(action(TRAVEL));
        assert_eq!(game.session.step().unwrap(), None);
        assert_eq!(game.tick(), frozen.tick + 1);
        let deadline = Instant::now() + DEADLINE;
        let rejected = loop {
            let snapshot = game.snapshot();
            if snapshot.error.is_some() {
                break snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "travel did not refuse: {snapshot:?}"
            );
            game.maintenance();
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(game.session.state(), Some(RuntimeState::Paused));
        assert_eq!(game.tick(), frozen.tick + 1);
        assert_eq!(rejected.phase, GamePhase::Running);
        assert!(!rejected.restoring);
        assert_eq!(rejected.level, Some(before.progress.location.level));
        assert_eq!(rejected.current_instance, Some(frozen.instance));
        assert_eq!(rejected.scene_members, AUTHORED_LEVEL_MEMBERS);
        assert_eq!(rejected.logical_tick, before.clock.ticks);
        assert_eq!(rejected.logical_elapsed_nanos, before.clock.elapsed_nanos);
        assert_eq!(rejected.score, before.progress.score);
        assert_eq!(rejected.collected, before.progress.collected.len());
        assert_eq!(rejected.visible_pickups, 2);
        assert_eq!(rejected.markers, [before.progress.markers[0].id]);
        assert_eq!(rejected.travel_count, before.counters.travel_count);
        assert_eq!(actual_explorer(&game), actor);
        assert!(matches!(
            capture_resume(&game.session),
            Err(GameResumeError::Ineligible(_))
        ));

        game.submit(movement(1, true));
        game.command(action(MARKER));
        let continued = game.capture();
        assert_eq!(game.snapshot().current_instance, Some(frozen.instance));
        assert_eq!(actual_explorer(&game).0, actor.0);
        assert!(
            (continued.player.translation[0] - before.player.translation[0] - 3.2).abs() < 0.0001
        );
        let mut expected = before;
        expected.clock.ticks += 1;
        expected.clock.elapsed_nanos += TIMESTEP_NANOS;
        expected.held = [false, true, false, false];
        expected.player.translation[0] = continued.player.translation[0];
        let marker_id = expected.progress.next_marker_id;
        expected.progress.markers.push(MarkerRecord {
            id: marker_id,
            level: expected.progress.location.level,
            position: actor.1.translation.to_array(),
        });
        expected.progress.next_marker_id = MarkerId::new(marker_id.get() + 1).unwrap();
        assert_eq!(continued, expected);
        assert_column(&game, 1);
        game.close();
    }
}
