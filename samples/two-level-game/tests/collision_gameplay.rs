#![cfg(any(feature = "development", feature = "cook"))]
#![forbid(unsafe_code)]

#[allow(
    dead_code,
    reason = "shared game driver also serves other resume tests"
)]
#[path = "support/resume_gameplay.rs"]
mod support;

use std::{f32::consts::FRAC_PI_6, time::Duration};

use nara::{
    advanced_prelude::CurrentScene,
    app::RuntimeState,
    identity::{EntityLookup, SceneEntityId},
    prelude::{Transform2d, Vec2},
    sprite::Sprite,
};
use support::{Game, MARKER, TRAVEL, action, movement};
use two_level_game::{
    CollisionBox, GamePhase, GameResumeError, capture_resume,
    levels::{LevelId, LevelKey},
    request_resume,
    resume::{ResumeState, decode_resume, encode_resume},
};

const MARGIN: f32 = 0.01;
const ACTOR_HALF_EXTENT: f32 = 9.0;
const ROOM_HALF_WIDTH: f32 = 289.0;
const ROOM_HALF_HEIGHT: f32 = 159.0;
const INNER_HALF_WIDTH: f32 = 6.0;
const INNER_TOP: f32 = 24.0;
const CONTACT_TOLERANCE: f32 = 0.001;
const TIMESTEP_NANOS: u64 = 20_000_000;
const STEP_DISTANCE: f32 = 3.2;
const MAX_CONTACT_STEPS: usize = 192;

fn assert_near(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= CONTACT_TOLERANCE,
        "expected {expected} +/- {CONTACT_TOLERANCE}, got {actual}"
    );
}

fn assert_scene_geometry(game: &Game, inner_x: f32) {
    assert_eq!(game.snapshot().scene_members, 11);
    game.session
        .observe(|world| {
            let current = CurrentScene::get(world).expect("the game owns its current Scene");
            for (id, translation, half_extents) in [
                ("wall-west", [-ROOM_HALF_WIDTH - 5.0, 0.0], [5.0, 164.0]),
                ("wall-east", [ROOM_HALF_WIDTH + 5.0, 0.0], [5.0, 164.0]),
                ("wall-north", [0.0, ROOM_HALF_HEIGHT + 5.0], [299.0, 5.0]),
                ("wall-south", [0.0, -ROOM_HALF_HEIGHT - 5.0], [299.0, 5.0]),
                ("wall-inner", [inner_x, -12.0], [INNER_HALF_WIDTH, 36.0]),
            ] {
                let id = SceneEntityId::new(id).unwrap();
                let entity = match current.receipt().resolve(world, &id) {
                    EntityLookup::Resolved(entity) => entity,
                    other => panic!("the current Scene must resolve wall {id:?}: {other:?}"),
                };
                assert_eq!(
                    world.get::<Transform2d>(entity),
                    Some(&Transform2d {
                        translation: Vec2::from_array(translation),
                        rotation: 0.0,
                        scale: Vec2::ONE,
                        z: 0.0,
                    })
                );
                assert_eq!(
                    world.get::<CollisionBox>(entity),
                    Some(&CollisionBox {
                        center: Vec2::ZERO,
                        half_width: half_extents[0],
                        half_height: half_extents[1],
                    })
                );
            }
        })
        .unwrap();
}

fn assert_actor(game: &Game) -> (Transform2d, Sprite) {
    let state = game.capture();
    let snapshot = game.snapshot();
    let (pose, definition, sprite) = game
        .session
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
            (
                *world
                    .get::<Transform2d>(entity)
                    .expect("the explorer has a pose"),
                *world
                    .get::<CollisionBox>(entity)
                    .expect("the explorer has collision geometry"),
                world
                    .get::<Sprite>(entity)
                    .expect("the explorer has a Sprite")
                    .clone(),
            )
        })
        .unwrap();
    assert_eq!(definition.center, Vec2::ZERO);
    assert_eq!(definition.half_width, ACTOR_HALF_EXTENT);
    assert_eq!(definition.half_height, ACTOR_HALF_EXTENT);
    assert_eq!(pose.translation, snapshot.player);
    assert_eq!(pose.translation.to_array(), state.player.translation);
    assert_eq!(pose.rotation, state.player.rotation);
    assert_eq!(pose.scale.to_array(), state.player.scale);
    let (sin, cos) = pose.rotation.sin_cos();
    let half_width =
        ACTOR_HALF_EXTENT * (pose.scale.x.abs() * cos.abs() + pose.scale.y.abs() * sin.abs());
    let half_height =
        ACTOR_HALF_EXTENT * (pose.scale.x.abs() * sin.abs() + pose.scale.y.abs() * cos.abs());
    assert!(
        pose.translation.x.abs() + half_width + MARGIN <= ROOM_HALF_WIDTH + CONTACT_TOLERANCE,
        "the actual actor must remain inside the east and west walls: {pose:?}"
    );
    assert!(
        pose.translation.y.abs() + half_height + MARGIN <= ROOM_HALF_HEIGHT + CONTACT_TOLERANCE,
        "the actual actor must remain inside the north and south walls: {pose:?}"
    );
    let walking = state.held[0] != state.held[1] || state.held[2] != state.held[3];
    let column = if walking {
        1 + (state.clock.elapsed_nanos / 100_000_000) % 3
    } else {
        0
    };
    let region = sprite
        .texture_region
        .expect("the actual explorer projects an atlas frame");
    assert_eq!(region.min.to_array(), [column as f32 * 0.25, 0.0]);
    assert_eq!(region.size.to_array(), [0.25, 1.0]);
    (pose, sprite)
}

fn advance_until(
    game: &mut Game,
    destination: &str,
    reached: impl Fn(Vec2) -> bool,
) -> Transform2d {
    for _ in 0..MAX_CONTACT_STEPS {
        let pose = assert_actor(game).0;
        if reached(pose.translation) {
            return pose;
        }
        game.step();
    }
    panic!(
        "did not reach {destination} within {MAX_CONTACT_STEPS} fixed steps: {:?}",
        game.snapshot()
    );
}

fn reach_inner_wall(game: &mut Game, inner_x: f32, score: u32) -> ResumeState {
    let initial = assert_actor(game).0;
    assert_eq!(initial.translation.to_array(), [-120.0, 0.0]);
    assert_eq!(initial.rotation, 0.0);
    assert_eq!(initial.scale, Vec2::ONE);
    game.command(movement(1, true));
    let expected_x = inner_x - INNER_HALF_WIDTH - ACTOR_HALF_EXTENT - MARGIN;
    let contact = advance_until(game, "the current level's inner wall", |position| {
        (position.x - expected_x).abs() <= CONTACT_TOLERANCE
    });
    assert_near(contact.translation.x, expected_x);
    assert_near(contact.translation.y, 0.0);
    let state = game.capture();
    assert_eq!(state.progress.score, score);
    assert_eq!(game.snapshot().visible_pickups, 1);
    assert_eq!(state.held, [false, true, false, false]);
    assert!(state.progress.collected.iter().any(|pickup| {
        pickup.level == state.progress.location.level
            && pickup.entity == SceneEntityId::new("gem-near").unwrap()
    }));
    state
}

fn assert_equivalent(original: &Game, candidate: &Game) -> ResumeState {
    let state = original.capture();
    assert_eq!(
        encode_resume(&candidate.capture()).unwrap(),
        encode_resume(&state).unwrap(),
        "fresh and original games must retain the same complete canonical state"
    );
    let original_actor = assert_actor(original);
    let candidate_actor = assert_actor(candidate);
    assert_eq!(
        candidate_actor.1.texture_region,
        original_actor.1.texture_region
    );
    assert_eq!(
        candidate.snapshot().visible_pickups,
        original.snapshot().visible_pickups
    );
    assert_eq!(candidate.snapshot().markers, original.snapshot().markers);
    assert_eq!(
        candidate.snapshot().scene_members,
        original.snapshot().scene_members
    );
    state
}

#[test]
fn meadow_wall_blocks_held_motion_slides_around_its_top_and_contains_the_actual_actor() {
    let mut game = Game::source();
    assert_scene_geometry(&game, 0.0);
    let blocked = reach_inner_wall(&mut game, 0.0, 10);
    let contact_x = -INNER_HALF_WIDTH - ACTOR_HALF_EXTENT - MARGIN;
    let frozen = game.frozen();
    let blocked_sprite = assert_actor(&game).1;

    game.maintenance();
    for delta in [
        Duration::ZERO,
        Duration::from_millis(100),
        Duration::from_secs(1),
    ] {
        assert_eq!(game.session.frame(delta).unwrap(), None);
        assert_eq!(game.session.state(), Some(RuntimeState::Paused));
        assert_eq!(game.frozen(), frozen);
        assert_eq!(assert_actor(&game).1, blocked_sprite);
    }

    game.step();
    let stepped = game.capture();
    assert_eq!(game.tick(), frozen.tick + 1);
    assert_eq!(stepped.clock.ticks, blocked.clock.ticks + 1);
    assert_eq!(
        stepped.clock.elapsed_nanos,
        blocked.clock.elapsed_nanos + TIMESTEP_NANOS
    );
    assert_eq!(stepped.clock.timestep_nanos, TIMESTEP_NANOS);
    assert_eq!(stepped.progress, blocked.progress);
    assert_eq!(stepped.counters, blocked.counters);
    assert_eq!(stepped.held, [false, true, false, false]);
    assert_near(assert_actor(&game).0.translation.x, contact_x);

    let mut walk_columns = [false; 3];
    for _ in 0..15 {
        let snapshot = game.step();
        let (pose, sprite) = assert_actor(&game);
        assert_near(pose.translation.x, contact_x);
        assert_near(pose.translation.y, 0.0);
        assert_eq!(snapshot.score, 10);
        assert_eq!(snapshot.held, [false, true, false, false]);
        let region = sprite.texture_region.unwrap();
        let column = (region.min.x * 4.0) as usize;
        assert!(
            (1..=3).contains(&column),
            "blocked intent must keep walking UVs"
        );
        walk_columns[column - 1] = true;
    }
    assert_eq!(walk_columns, [true; 3]);
    let before_slide = game.capture();
    let diagonal = game.command(movement(2, true));
    assert_near(assert_actor(&game).0.translation.x, contact_x);
    assert_near(diagonal.player.y, STEP_DISTANCE / 2.0_f32.sqrt());
    assert_eq!(diagonal.held, [false, true, true, false]);
    let rounded = advance_until(
        &mut game,
        "the free passage above the Meadow wall",
        |position| position.x > INNER_HALF_WIDTH + ACTOR_HALF_EXTENT + MARGIN && position.y > 60.0,
    );
    assert!(rounded.translation.y > INNER_TOP + ACTOR_HALF_EXTENT + MARGIN);
    assert_eq!(game.capture().progress, before_slide.progress);

    game.command(movement(2, false));
    let east_x = ROOM_HALF_WIDTH - ACTOR_HALF_EXTENT - MARGIN;
    let east = advance_until(&mut game, "the east room wall", |position| {
        (position.x - east_x).abs() <= CONTACT_TOLERANCE
    });
    assert_near(east.translation.x, east_x);
    assert_near(east.translation.y, rounded.translation.y);
    assert_eq!(
        game.snapshot().score,
        10,
        "the high route must avoid the far gem"
    );

    game.submit(movement(1, false));
    game.command(movement(2, true));
    let north_y = ROOM_HALF_HEIGHT - ACTOR_HALF_EXTENT - MARGIN;
    let north = advance_until(&mut game, "the north room wall", |position| {
        (position.y - north_y).abs() <= CONTACT_TOLERANCE
    });
    assert_near(north.translation.y, north_y);
    assert_near(north.translation.x, east_x);

    game.submit(movement(2, false));
    game.command(movement(0, true));
    let west = advance_until(&mut game, "the west room wall", |position| {
        (position.x + east_x).abs() <= CONTACT_TOLERANCE
    });
    assert_near(west.translation.x, -east_x);
    assert_near(west.translation.y, north_y);

    game.submit(movement(0, false));
    game.command(movement(3, true));
    let south = advance_until(&mut game, "the south room wall", |position| {
        (position.y + north_y).abs() <= CONTACT_TOLERANCE
    });
    assert_near(south.translation.x, -east_x);
    assert_near(south.translation.y, -north_y);
    assert_eq!(game.capture().progress, before_slide.progress);
    game.close();
}

#[test]
fn cavern_uses_its_own_wall_and_retry_replaces_the_scene_at_the_retained_checkpoint() {
    let mut game = Game::source();
    reach_inner_wall(&mut game, 0.0, 10);
    game.command(action(MARKER));
    let before_travel = game.capture();
    let meadow_instance = game.snapshot().current_instance;
    let runtime_tick = game.tick();
    let arrived = game.command(action(TRAVEL));
    assert_eq!(game.tick(), runtime_tick + 1);
    assert_eq!(arrived.level, Some(LevelKey::main(LevelId::Cavern)));
    assert_ne!(arrived.current_instance, meadow_instance);
    assert_eq!(arrived.held, [false; 4]);
    assert_eq!(arrived.markers.len(), 0);
    let travelled = game.capture();
    assert_eq!(travelled.clock, before_travel.clock);
    assert_eq!(travelled.progress.score, 10);
    assert_eq!(
        travelled.progress.collected,
        before_travel.progress.collected
    );
    assert_eq!(travelled.progress.markers, before_travel.progress.markers);
    assert_eq!(travelled.counters.travel_count, 1);
    assert_scene_geometry(&game, 20.0);

    let blocked = reach_inner_wall(&mut game, 20.0, 20);
    assert_near(blocked.player.translation[0], 4.99);
    assert!(
        blocked.player.translation[0] > 0.0,
        "the old Meadow wall must be absent"
    );
    assert_eq!(blocked.progress.collected.len(), 2);
    assert_eq!(blocked.progress.markers, before_travel.progress.markers);
    game.command(action(MARKER));
    let before_retry = game.capture();
    let cavern_instance = game.snapshot().current_instance;
    let tick = game.tick();
    let restarted = game.command(action("two-level-game.retry"));
    let retried = game.capture();
    assert_eq!(game.tick(), tick + 1);
    assert_ne!(restarted.current_instance, cavern_instance);
    assert_eq!(restarted.level, Some(LevelKey::main(LevelId::Cavern)));
    assert_eq!(retried.player.translation, [-120.0, 0.0]);
    assert_eq!(retried.held, [false; 4]);
    assert_eq!(retried.progress, before_retry.progress);
    assert_eq!(retried.clock.ticks, before_retry.clock.ticks + 1);
    assert_eq!(
        retried.clock.elapsed_nanos,
        before_retry.clock.elapsed_nanos + TIMESTEP_NANOS
    );
    assert_eq!(
        retried.counters.restarts,
        before_retry.counters.restarts + 1
    );
    assert_eq!(retried.counters.travel_count, 1);
    assert_eq!(restarted.markers, vec![before_retry.progress.markers[1].id]);
    assert_scene_geometry(&game, 20.0);
    assert_actor(&game);

    let blocked_again = reach_inner_wall(&mut game, 20.0, 20);
    assert_eq!(blocked_again.progress, before_retry.progress);
    assert_near(blocked_again.player.translation[0], 4.99);
    game.close();
}

#[test]
fn fresh_source_resume_reconstructs_blocked_intent_pose_and_uv_before_identical_continuation() {
    let mut original = Game::source();
    reach_inner_wall(&mut original, 0.0, 10);
    original.command(action(MARKER));
    let captured = original.capture();
    let contact_x = -INNER_HALF_WIDTH - ACTOR_HALF_EXTENT - MARGIN;
    assert_near(captured.player.translation[0], contact_x);
    assert_eq!(captured.held, [false, true, false, false]);
    assert_eq!(captured.progress.markers.len(), 1);
    assert_near(captured.progress.markers[0].position[0], contact_x);
    assert!(captured.clock.ticks > 0);
    assert_eq!(
        captured.clock.elapsed_nanos,
        captured.clock.ticks * TIMESTEP_NANOS
    );
    let captured_bytes = encode_resume(&captured).unwrap();
    let retained = original.frozen();
    let original_sprite = assert_actor(&original).1;

    let mut candidate = Game::source();
    let fresh = candidate.frozen();
    assert_eq!(fresh.tick, 0);
    assert_ne!(fresh.generation, retained.generation);
    assert_ne!(fresh.domain, retained.domain);
    request_resume(&mut candidate.session, &captured)
        .expect("the public blocked state must resume");
    candidate.settle();
    assert_eq!(candidate.session.state(), Some(RuntimeState::Paused));
    assert_eq!(
        candidate.tick(),
        1,
        "reconstruction must not add a gameplay step"
    );
    assert_eq!(encode_resume(&candidate.capture()).unwrap(), captured_bytes);
    assert_eq!(
        assert_actor(&candidate).1.texture_region,
        original_sprite.texture_region
    );
    assert_ne!(candidate.snapshot().current_instance, Some(fresh.instance));
    assert_eq!(
        candidate.session.runtime_generation(),
        Some(fresh.generation)
    );
    assert_eq!(original.frozen(), retained);
    assert_scene_geometry(&candidate, 0.0);
    assert_equivalent(&original, &candidate);
    let tick_offset = original.tick() - candidate.tick();

    for _ in 0..6 {
        original.step();
        candidate.step();
        let state = assert_equivalent(&original, &candidate);
        assert_near(state.player.translation[0], contact_x);
        assert_near(state.player.translation[1], 0.0);
        assert_eq!(state.held, [false, true, false, false]);
        assert_eq!(original.tick() - candidate.tick(), tick_offset);
    }

    original.command(movement(2, true));
    candidate.command(movement(2, true));
    let sliding = assert_equivalent(&original, &candidate);
    assert_near(sliding.player.translation[0], contact_x);
    assert!(sliding.player.translation[1] > captured.player.translation[1]);
    for _ in 0..MAX_CONTACT_STEPS {
        if original.snapshot().player.x > 20.0 {
            break;
        }
        original.step();
        candidate.step();
        let state = assert_equivalent(&original, &candidate);
        assert_eq!(state.progress.score, 10);
        assert_eq!(original.tick() - candidate.tick(), tick_offset);
    }
    let rounded = assert_equivalent(&original, &candidate);
    assert!(
        rounded.player.translation[0] > 20.0
            && rounded.player.translation[1] > INNER_TOP + ACTOR_HALF_EXTENT + MARGIN,
        "both games must round the wall within {MAX_CONTACT_STEPS} steps: {rounded:?}"
    );
    original.command(movement(2, false));
    candidate.command(movement(2, false));
    let free = assert_equivalent(&original, &candidate);
    assert_near(
        free.player.translation[0] - rounded.player.translation[0],
        STEP_DISTANCE,
    );
    for _ in 0..4 {
        let before = original.capture();
        original.step();
        candidate.step();
        let state = assert_equivalent(&original, &candidate);
        assert_near(
            state.player.translation[0] - before.player.translation[0],
            STEP_DISTANCE,
        );
        assert_near(state.player.translation[1], before.player.translation[1]);
        assert_eq!(state.progress, captured.progress);
        assert_eq!(original.tick() - candidate.tick(), tick_offset);
    }
    candidate.close();
    original.close();
}

#[test]
fn overlapping_and_degenerate_wire_valid_resume_poses_refuse_before_time_or_command_admission() {
    let mut original = Game::source();
    original.command(action(MARKER));
    original.command(movement(1, true));
    let retained = original.frozen();
    let mut overlapping = original.capture();
    overlapping.player.translation = [0.0, -12.0];
    overlapping.runtime_time.pending_nanos = 7_000_000;
    overlapping.runtime_time.max_delta_nanos = 100_000_000;
    overlapping
        .validate()
        .expect("wall overlap is valid resume wire data");
    let overlapping = decode_resume(&encode_resume(&overlapping).unwrap()).unwrap();
    let mut candidate = Game::source();
    let fresh = candidate.frozen();
    assert_ne!(overlapping.runtime_time, candidate.capture().runtime_time);
    let reason = match request_resume(&mut candidate.session, &overlapping) {
        Err(GameResumeError::Ineligible(reason)) => reason,
        other => panic!("an overlapping pose must refuse before admission: {other:?}"),
    };
    assert!(reason.contains("overlaps a static wall"), "{reason}");
    assert_eq!(candidate.frozen(), fresh);
    assert_eq!(candidate.session.state(), Some(RuntimeState::Paused));
    assert_eq!(candidate.tick(), 0);

    let mut degenerate = overlapping;
    degenerate.player.translation = [-120.0, 0.0];
    degenerate.player.scale = [0.0, 1.0];
    degenerate
        .validate()
        .expect("zero scale remains valid resume wire data");
    let degenerate = decode_resume(&encode_resume(&degenerate).unwrap()).unwrap();
    let reason = match request_resume(&mut candidate.session, &degenerate) {
        Err(GameResumeError::Ineligible(reason)) => reason,
        other => panic!("a degenerate pose must refuse before admission: {other:?}"),
    };
    assert!(reason.contains("nonsingular"), "{reason}");
    assert_eq!(candidate.frozen(), fresh);
    assert_eq!(candidate.session.state(), Some(RuntimeState::Paused));
    assert_eq!(candidate.tick(), 0);
    assert_eq!(original.frozen(), retained);
    assert_actor(&candidate);
    candidate.close();
    original.close();
}

#[test]
fn fresh_source_resume_accepts_mirrored_nonuniform_rotated_geometry_and_moves_its_full_shape() {
    let mut original = Game::source();
    let retained = original.frozen();
    let mut state = original.capture();
    state.player.translation = [-80.0, 80.0];
    state.player.rotation = FRAC_PI_6;
    state.player.scale = [-1.5, 0.75];
    state.held = [false, true, false, false];
    state
        .validate()
        .expect("the mirrored rotated pose is valid resume wire data");
    let mut candidate = Game::source();
    request_resume(&mut candidate.session, &state).expect("a free affine actor pose must resume");
    candidate.settle();
    assert_eq!(candidate.tick(), 1);
    assert_eq!(
        encode_resume(&candidate.capture()).unwrap(),
        encode_resume(&state).unwrap()
    );
    let pose = assert_actor(&candidate).0;
    assert_eq!(pose.translation.to_array(), state.player.translation);
    assert_eq!(pose.rotation, FRAC_PI_6);
    assert_eq!(pose.scale.to_array(), [-1.5, 0.75]);
    assert_scene_geometry(&candidate, 0.0);
    assert_eq!(original.frozen(), retained);

    candidate.step();
    let moved = candidate.capture();
    let actual = assert_actor(&candidate).0;
    assert_near(actual.translation.x - pose.translation.x, STEP_DISTANCE);
    assert_near(actual.translation.y, pose.translation.y);
    assert_eq!(actual.rotation, pose.rotation);
    assert_eq!(actual.scale, pose.scale);
    assert_eq!(moved.clock.ticks, state.clock.ticks + 1);
    assert_eq!(
        moved.clock.elapsed_nanos,
        state.clock.elapsed_nanos + TIMESTEP_NANOS
    );
    assert_eq!(moved.progress, state.progress);
    let (sin, cos) = FRAC_PI_6.sin_cos();
    let expected_half_width = ACTOR_HALF_EXTENT * (1.5 * cos.abs() + 0.75 * sin.abs());
    let expected_x = ROOM_HALF_WIDTH - expected_half_width - MARGIN;
    let contact = advance_until(
        &mut candidate,
        "the east wall with the full mirrored RS shape",
        |position| (position.x - expected_x).abs() <= CONTACT_TOLERANCE,
    );
    assert_near(contact.translation.x, expected_x);
    assert_near(contact.translation.y, 80.0);
    assert_eq!(contact.rotation, FRAC_PI_6);
    assert_eq!(contact.scale.to_array(), [-1.5, 0.75]);
    assert!(contact.translation.x < ROOM_HALF_WIDTH - ACTOR_HALF_EXTENT - 1.0);
    let blocked = candidate.capture();
    candidate.step();
    let continued = candidate.capture();
    assert_near(assert_actor(&candidate).0.translation.x, expected_x);
    assert_eq!(continued.held, [false, true, false, false]);
    assert_eq!(continued.clock.ticks, blocked.clock.ticks + 1);
    assert_eq!(continued.progress, state.progress);
    candidate.close();
    original.close();
}

#[test]
fn rotated_diagonal_movement_rounds_the_wall_and_remains_capturable_after_contact() {
    let mut original = Game::source();
    let mut state = original.capture();
    state.player.translation = [-30.0, -12.0];
    state.player.rotation = 0.01;
    state.held = [false, true, false, true];
    request_resume(&mut original.session, &state).unwrap();
    original.settle();
    assert_eq!(
        encode_resume(&original.capture()).unwrap(),
        encode_resume(&state).unwrap()
    );

    let (sin, cos) = state.player.rotation.sin_cos();
    let horizontal_extent = ACTOR_HALF_EXTENT * (sin.abs() + cos.abs());
    for step in 1..=9 {
        let snapshot = original.step();
        let captured = capture_resume(&original.session).unwrap_or_else(|error| {
            panic!(
                "rotated diagonal step {step} must remain capturable at {:?}: {error}",
                snapshot.player
            )
        });
        assert_eq!(captured.player.translation, snapshot.player.to_array());
        assert_eq!(captured.player.rotation, state.player.rotation);
        assert_eq!(captured.clock.ticks, state.clock.ticks + step);
        assert!(
            snapshot.player.x + horizontal_extent <= -INNER_HALF_WIDTH + CONTACT_TOLERANCE,
            "the rotated explorer must stay to the west of the inner wall: {snapshot:?}"
        );
    }

    let contact = original.capture();
    let mut restored = Game::source();
    request_resume(&mut restored.session, &contact).unwrap();
    restored.settle();
    assert_eq!(
        encode_resume(&restored.capture()).unwrap(),
        encode_resume(&contact).unwrap()
    );

    for step in 10..=36 {
        original.step();
        restored.step();
        let continued = assert_equivalent(&original, &restored);
        assert_eq!(continued.clock.ticks, state.clock.ticks + step);
        assert_eq!(continued.held, state.held);
        if step == 13 {
            assert!(continued.player.translation[1] < contact.player.translation[1] - 4.0);
        }
    }
    let rounded = original.capture();
    assert!(
        rounded.player.translation[0] > INNER_HALF_WIDTH + horizontal_extent
            && rounded.player.translation[1] + horizontal_extent < -48.0,
        "held diagonal motion must round the south end of the wall: {:?}",
        rounded.player
    );
    restored.close();
    original.close();
}

#[test]
fn an_admitted_small_actor_stops_at_the_persistable_center_boundary_without_degrading() {
    let mut original = Game::source();
    let mut state = original.capture();
    state.player.translation = [279.0, 80.0];
    state.player.scale = [0.5, 0.5];
    state.held = [false, true, false, false];
    state.validate().unwrap();
    let mut candidate = Game::source();
    request_resume(&mut candidate.session, &state).unwrap();
    candidate.settle();
    assert_eq!(
        encode_resume(&candidate.capture()).unwrap(),
        encode_resume(&state).unwrap()
    );
    for index in 1..=6 {
        candidate.step();
        let stopped = candidate.capture();
        assert_eq!(candidate.snapshot().phase, GamePhase::Running);
        assert_eq!(stopped.player.translation, [280.0, 80.0]);
        assert_eq!(stopped.player.scale, state.player.scale);
        assert_eq!(stopped.held, state.held);
        assert_eq!(stopped.clock.ticks, state.clock.ticks + index);
        assert_eq!(stopped.progress, state.progress);
        assert!(candidate.snapshot().error.is_none());
    }
    candidate.close();
    original.close();
}
