#![cfg(any(feature = "development", feature = "cook"))]
#![forbid(unsafe_code)]

#[allow(
    dead_code,
    reason = "shared game driver also serves other resume tests"
)]
#[path = "support/resume_gameplay.rs"]
mod support;

#[path = "support/default_animation.rs"]
mod animation_reference;
use animation_reference::expected_default_frame;

use std::time::Duration;

use nara::{
    advanced_prelude::CurrentScene,
    app::RuntimeState,
    asset::{AssetServer, Assets},
    identity::{EntityLookup, SceneEntityId},
    image::{ImageAsset, ImageColorSpace, ImageExtent},
    material::SamplerDescriptor,
    sprite::Sprite,
};
use support::{Game, MARKER, TRAVEL, action, movement};
use two_level_game::{
    animation::{ExplorerFrame, atlas_product},
    levels::{LevelId, LevelKey},
    request_resume,
};

const TIMESTEP_NANOS: u64 = 20_000_000;

fn assert_frame(game: &Game, expected: ExplorerFrame) -> Sprite {
    let state = game.capture();
    assert_eq!(expected_default_frame(state.clock, state.held), expected);
    let snapshot = game.snapshot();
    let sprite = game
        .session
        .observe(|world| {
            let current = CurrentScene::get(world).expect("the game owns a current Scene");
            assert_eq!(
                snapshot.current_instance,
                Some(current.receipt().instance_id())
            );
            let id = SceneEntityId::new("explorer").unwrap();
            let entity = match current.receipt().resolve(world, &id) {
                EntityLookup::Resolved(entity) => entity,
                other => panic!("the current Scene must resolve its explorer: {other:?}"),
            };
            let sprite = world
                .get::<Sprite>(entity)
                .expect("the explorer has a Sprite");
            let atlas = world
                .resource::<AssetServer>()
                .get_product_handle::<ImageAsset>(atlas_product())
                .expect("the original asset owner admitted the explorer atlas");
            assert_eq!(sprite.material.image, Some(atlas));
            assert_eq!(sprite.material.sampler, SamplerDescriptor::NEAREST_CLAMP);
            let image = world
                .resource::<Assets<ImageAsset>>()
                .get(atlas)
                .expect("the Sprite's actual atlas handle has resident CPU Image content");
            assert_eq!(image.product(), Some(atlas_product()));
            assert_eq!(image.extent(), ImageExtent::new(64, 16));
            assert_eq!(image.color_space(), ImageColorSpace::Srgb);
            sprite.clone()
        })
        .unwrap();
    let region = sprite
        .texture_region
        .expect("the explorer projects an atlas frame");
    let min_x = match expected {
        ExplorerFrame::Idle => 0.0,
        ExplorerFrame::WalkOne => 0.25,
        ExplorerFrame::WalkTwo => 0.5,
        ExplorerFrame::WalkThree => 0.75,
    };
    assert_eq!(region.min.to_array(), [min_x, 0.0]);
    assert_eq!(region.size.to_array(), [0.25, 1.0]);
    assert_eq!(region, expected.texture_region());
    sprite
}

#[test]
fn fixed_ticks_project_every_atlas_frame_and_idle_does_not_restart_the_walk_phase() {
    let mut game = Game::source();
    let initial = game.capture();
    assert_eq!(initial.clock.ticks, 0);
    assert_eq!(initial.clock.elapsed_nanos, 0);
    assert_eq!(initial.clock.timestep_nanos, TIMESTEP_NANOS);
    let idle = assert_frame(&game, ExplorerFrame::Idle);

    game.command(movement(1, true));
    for (target_tick, expected) in [
        (1, ExplorerFrame::WalkOne),
        (4, ExplorerFrame::WalkOne),
        (5, ExplorerFrame::WalkTwo),
        (9, ExplorerFrame::WalkTwo),
        (10, ExplorerFrame::WalkThree),
        (14, ExplorerFrame::WalkThree),
        (15, ExplorerFrame::WalkOne),
    ] {
        for tick in game.tick() + 1..=target_tick {
            let snapshot = game.step();
            assert_eq!(snapshot.logical_tick, tick);
            assert_eq!(snapshot.logical_elapsed_nanos, tick * TIMESTEP_NANOS);
        }
        let state = game.capture();
        assert_eq!(state.clock.ticks, target_tick);
        assert_eq!(state.clock.elapsed_nanos, target_tick * TIMESTEP_NANOS);
        assert_eq!(state.held, [false, true, false, false]);
        assert_eq!(assert_frame(&game, expected).material, idle.material);
    }

    game.command(movement(1, false));
    assert_frame(&game, ExplorerFrame::Idle);
    for _ in 0..3 {
        game.step();
    }
    assert_eq!(game.capture().clock.elapsed_nanos, 380_000_000);
    assert_frame(&game, ExplorerFrame::Idle);
    game.command(movement(1, true));
    assert_eq!(game.capture().clock.elapsed_nanos, 400_000_000);
    assert_frame(&game, ExplorerFrame::WalkTwo);
    game.close();
}

#[test]
fn opposing_axes_cancel_and_diagonal_intent_walks_at_the_normalized_speed() {
    let mut game = Game::source();
    let initial = game.snapshot().player;
    game.submit(movement(0, true));
    let opposed = game.command(movement(1, true));
    assert_eq!(opposed.held, [true, true, false, false]);
    assert_eq!(opposed.player, initial);
    assert_frame(&game, ExplorerFrame::Idle);

    let up = game.command(movement(2, true));
    assert_eq!(up.held, [true, true, true, false]);
    assert_eq!(up.player.x, opposed.player.x);
    assert!((up.player.y - opposed.player.y - 3.2).abs() < 0.0001);
    assert_frame(&game, ExplorerFrame::WalkOne);

    let all_opposed = game.command(movement(3, true));
    assert_eq!(all_opposed.held, [true; 4]);
    assert_eq!(all_opposed.player, up.player);
    assert_frame(&game, ExplorerFrame::Idle);

    let right = game.command(movement(0, false));
    assert_eq!(right.held, [false, true, true, true]);
    assert_eq!(right.player.y, all_opposed.player.y);
    assert!((right.player.x - all_opposed.player.x - 3.2).abs() < 0.0001);
    assert_frame(&game, ExplorerFrame::WalkOne);

    let diagonal = game.command(movement(3, false));
    assert_eq!(diagonal.held, [false, true, true, false]);
    let displacement = diagonal.player - right.player;
    assert!(displacement.x > 0.0 && displacement.y > 0.0);
    assert!((displacement.x - displacement.y).abs() < 0.0001);
    assert!((displacement.length() - 3.2).abs() < 0.0001);
    assert_eq!(diagonal.logical_elapsed_nanos, 100_000_000);
    assert_frame(&game, ExplorerFrame::WalkTwo);
    game.close();
}

#[test]
fn paused_maintenance_preserves_the_complete_game_and_step_advances_one_fixed_tick() {
    let mut game = Game::source();
    game.command(movement(1, true));
    game.command(action(MARKER));
    for _ in 0..2 {
        game.step();
    }
    let before = game.capture();
    assert_eq!(before.clock.elapsed_nanos, 80_000_000);
    assert_eq!(before.progress.markers.len(), 1);
    let frozen = game.frozen();
    let sprite = assert_frame(&game, ExplorerFrame::WalkOne);

    game.maintenance();
    for delta in [
        Duration::ZERO,
        Duration::from_millis(100),
        Duration::from_secs(1),
    ] {
        assert_eq!(game.session.frame(delta).unwrap(), None);
        assert_eq!(game.session.state(), Some(RuntimeState::Paused));
        assert_eq!(game.frozen(), frozen);
        assert_eq!(assert_frame(&game, ExplorerFrame::WalkOne), sprite);
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
    let mut expected_sprite = sprite;
    expected_sprite.texture_region = Some(ExplorerFrame::WalkTwo.texture_region());
    assert_eq!(assert_frame(&game, ExplorerFrame::WalkTwo), expected_sprite);
    game.close();
}

#[test]
fn completed_travel_and_restart_project_idle_and_keep_the_global_walk_phase() {
    let mut game = Game::source();
    game.command(movement(1, true));
    for _ in 0..5 {
        game.step();
    }
    let before_travel = game.capture();
    let previous_instance = game.snapshot().current_instance;
    let atlas = assert_frame(&game, ExplorerFrame::WalkTwo).material.image;
    let tick = game.tick();

    let arrived = game.command(action(TRAVEL));
    let travelled = game.capture();
    assert_eq!(
        game.tick(),
        tick + 1,
        "travel settles without extra fixed ticks"
    );
    assert_eq!(arrived.level, Some(LevelKey::main(LevelId::Cavern)));
    assert_ne!(arrived.current_instance, previous_instance);
    assert_eq!(travelled.clock, before_travel.clock);
    assert_eq!(travelled.held, [false; 4]);
    assert_eq!(travelled.counters.travel_count, 1);
    assert_eq!(
        assert_frame(&game, ExplorerFrame::Idle).material.image,
        atlas
    );

    game.command(movement(1, true));
    assert_eq!(game.capture().clock.elapsed_nanos, 140_000_000);
    assert_frame(&game, ExplorerFrame::WalkTwo);
    let before_restart = game.capture();
    let instance = game.snapshot().current_instance;
    let restarted = game.command(action("two-level-game.retry"));
    let state = game.capture();
    assert_ne!(restarted.current_instance, instance);
    assert_eq!(restarted.level, arrived.level);
    assert_eq!(state.progress, before_restart.progress);
    assert_eq!(state.counters.restarts, 1);
    assert_eq!(state.counters.travel_count, 1);
    assert_eq!(state.clock.ticks, before_restart.clock.ticks + 1);
    assert_eq!(state.clock.elapsed_nanos, 160_000_000);
    assert_eq!(state.held, [false; 4]);
    assert_eq!(
        assert_frame(&game, ExplorerFrame::Idle).material.image,
        atlas
    );

    game.command(movement(1, true));
    assert_eq!(game.capture().clock.elapsed_nanos, 180_000_000);
    assert_frame(&game, ExplorerFrame::WalkTwo);
    game.step();
    assert_eq!(game.capture().clock.elapsed_nanos, 200_000_000);
    assert_frame(&game, ExplorerFrame::WalkThree);
    game.close();
}

#[test]
fn fresh_source_resume_projects_a_nonfirst_walk_frame_before_continuing_the_original() {
    let mut original = Game::source();
    original.command(action(MARKER));
    original.command(movement(1, true));
    for _ in 0..7 {
        original.step();
    }
    let captured = original.capture();
    assert_eq!(captured.clock.ticks, 9);
    assert_eq!(captured.clock.elapsed_nanos, 180_000_000);
    assert_eq!(captured.progress.markers.len(), 1);
    let original_sprite = assert_frame(&original, ExplorerFrame::WalkTwo);
    let retained = original.frozen();

    let mut candidate = Game::source();
    let fresh = candidate.frozen();
    assert_eq!(fresh.tick, 0);
    assert_ne!(fresh.generation, retained.generation);
    assert_ne!(fresh.domain, retained.domain);
    request_resume(&mut candidate.session, &captured).expect("ordinary public exact-state resume");
    candidate.settle();

    assert_eq!(
        candidate.tick(),
        1,
        "only the resume command may have executed"
    );
    assert_eq!(candidate.session.state(), Some(RuntimeState::Paused));
    assert_eq!(candidate.capture(), captured);
    let restored_sprite = assert_frame(&candidate, ExplorerFrame::WalkTwo);
    assert_eq!(
        restored_sprite.texture_region,
        original_sprite.texture_region
    );
    assert_ne!(candidate.snapshot().current_instance, Some(fresh.instance));
    assert_eq!(
        candidate.session.runtime_generation(),
        Some(fresh.generation)
    );
    assert_eq!(original.frozen(), retained);
    let tick_offset = original.tick() - candidate.tick();

    for expected in [
        ExplorerFrame::WalkThree,
        ExplorerFrame::WalkThree,
        ExplorerFrame::WalkThree,
        ExplorerFrame::WalkThree,
        ExplorerFrame::WalkThree,
        ExplorerFrame::WalkOne,
    ] {
        original.step();
        candidate.step();
        assert_eq!(candidate.capture(), original.capture());
        assert_eq!(original.tick() - candidate.tick(), tick_offset);
        let original_sprite = assert_frame(&original, expected);
        let candidate_sprite = assert_frame(&candidate, expected);
        assert_eq!(
            candidate_sprite.texture_region,
            original_sprite.texture_region
        );
    }
    for game in [&mut original, &mut candidate] {
        game.command(movement(1, false));
        assert_frame(game, ExplorerFrame::Idle);
    }
    assert_eq!(candidate.capture(), original.capture());
    candidate.close();
    original.close();
}
