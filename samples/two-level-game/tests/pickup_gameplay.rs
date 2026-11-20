#![cfg(feature = "local-player")]
#![forbid(unsafe_code)]

#[allow(dead_code, reason = "reuse the public headless game driver")]
#[path = "support/resume_gameplay.rs"]
mod support;

use nara::{
    ProductConfiguration,
    advanced_prelude::CurrentScene,
    app::{PluginCategory, PluginDeclaration, PluginId},
    ecs::{Entity, World, schedule::IntoScheduleConfigs},
    gameplay::GameplayCommandSet,
    hierarchy::{HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter},
    identity::{EntityLookup, SceneEntityId, TombstoneCause},
    prelude::{App, CoreStage, FixedTime, FixedUpdateSet, Plugin, PluginError, Transform2d, Vec2},
    scene::retire_and_despawn_scene_entity,
    sprite::Sprite,
    transform::historical_spatial_snapshot,
};
use support::{Game, movement};
use two_level_game::{GameSnapshot, product_recipe};

const NEAR_PICKUP: &str = "gem-near";
const AUTHORED_CONTACT_TICK: u64 = 14;
const SPRITE_RESTORE_TICK: u64 = AUTHORED_CONTACT_TICK + 1;
const MOVED_CONTACT_TICK: u64 = 25;
const AUTHORED_POSITION: Vec2 = Vec2::new(-60.0, 0.0);
const MOVED_LOCAL_POSITION: Vec2 = Vec2::new(95.0, 0.0);
const MOVED_WORLD_POSITION: Vec2 = Vec2::new(-25.0, 0.0);

#[derive(Clone, Copy)]
enum PickupEdit {
    Retire,
    Move,
    DetachSprite,
}

impl ProductConfiguration for PickupEdit {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.push(*self as u8);
    }
}

impl Plugin for PickupEdit {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test-pickup-edit"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[PluginId::new("two-level-game.exploration")]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let edit = *self;
        let mut detached_sprite = None;
        app.add_systems(
            CoreStage::FixedUpdate,
            (move |world: &mut World| edit_pickup(world, edit, &mut detached_sprite))
                .in_set(FixedUpdateSet::Simulate)
                .after(GameplayCommandSet::Consume),
        )?;
        Ok(())
    }
}

fn current_entity(world: &World, id: &str) -> Entity {
    let id = SceneEntityId::new(id).unwrap();
    let current = CurrentScene::get(world).unwrap();
    match current.receipt().resolve(world, &id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("expected live {id}: {lookup:?}"),
    }
}

fn edit_pickup(world: &mut World, edit: PickupEdit, detached_sprite: &mut Option<Sprite>) {
    match (edit, world.resource::<FixedTime>().tick()) {
        (PickupEdit::Retire, 1) => {
            let pickup = current_entity(world, NEAR_PICKUP);
            retire_and_despawn_scene_entity(world, pickup).unwrap();
        }
        (PickupEdit::Move, AUTHORED_CONTACT_TICK) => {
            let pickup = current_entity(world, NEAR_PICKUP);
            let parent = current_entity(world, "checkpoint");
            let from = HierarchyPlacement::current(world, pickup).unwrap();
            let to = HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent });
            HierarchyWriter::new(world)
                .move_entity(pickup, from, to)
                .unwrap();
        }
        (PickupEdit::Move, MOVED_CONTACT_TICK) => {
            let pickup = current_entity(world, NEAR_PICKUP);
            world.get_mut::<Transform2d>(pickup).unwrap().translation = MOVED_LOCAL_POSITION;
        }
        (PickupEdit::DetachSprite, AUTHORED_CONTACT_TICK) => {
            let pickup = current_entity(world, NEAR_PICKUP);
            *detached_sprite = Some(world.entity_mut(pickup).take::<Sprite>().unwrap());
        }
        (PickupEdit::DetachSprite, SPRITE_RESTORE_TICK) => {
            let pickup = current_entity(world, NEAR_PICKUP);
            world
                .entity_mut(pickup)
                .insert(detached_sprite.take().unwrap());
        }
        _ => {}
    }
}

fn game_with_pickup_edit(edit: PickupEdit) -> Game {
    let recipe = product_recipe()
        .add_configured_plugin(edit, |edit| *edit)
        .unwrap();
    Game::with_recipe(recipe)
}

fn walk_to_authored_contact(game: &mut Game) -> GameSnapshot {
    game.submit(movement(1, true));
    for _ in 1..AUTHORED_CONTACT_TICK {
        game.step();
    }
    let snapshot = game.step();
    assert_eq!(snapshot.tick, AUTHORED_CONTACT_TICK);
    assert!(snapshot.player.distance(Vec2::new(-75.2, 0.0)) < 0.001);
    snapshot
}

fn pickup_state(game: &Game) -> (Vec2, Vec2, f32) {
    game.session
        .observe(|world| {
            let entity = current_entity(world, NEAR_PICKUP);
            let local = world.get::<Transform2d>(entity).unwrap().translation;
            let global = historical_spatial_snapshot(world)
                .unwrap()
                .global(entity)
                .unwrap()
                .translation();
            let alpha = world.get::<Sprite>(entity).unwrap().material.tint.a;
            (local, global, alpha)
        })
        .unwrap()
}

#[test]
fn retired_pickup_does_not_score_at_its_authored_position() {
    let mut game = game_with_pickup_edit(PickupEdit::Retire);
    let initial = game.snapshot();
    assert_eq!(initial.visible_pickups, 2);

    let contact = walk_to_authored_contact(&mut game);
    assert_eq!(contact.current_instance, initial.current_instance);
    assert_eq!(
        (contact.score, contact.collected, contact.visible_pickups),
        (0, 0, 1)
    );
    game.session
        .observe(|world| {
            let current = CurrentScene::get(world).unwrap();
            let id = SceneEntityId::new(NEAR_PICKUP).unwrap();
            let EntityLookup::Tombstoned(Some(tombstone)) = current.receipt().resolve(world, &id)
            else {
                panic!("the pickup must be retired from the current Scene");
            };
            assert_eq!(tombstone.cause(), TombstoneCause::Despawned);
        })
        .unwrap();
    game.close();
}

#[test]
fn a_live_pickup_scores_once_after_its_sprite_returns() {
    let mut game = game_with_pickup_edit(PickupEdit::DetachSprite);
    let id = SceneEntityId::new(NEAR_PICKUP).unwrap();
    let (receipt, pickup) = game
        .session
        .observe(|world| {
            let receipt = CurrentScene::get(world).unwrap().receipt().clone();
            (receipt, current_entity(world, NEAR_PICKUP))
        })
        .unwrap();

    let contact = walk_to_authored_contact(&mut game);
    assert_eq!((contact.score, contact.collected), (0, 0));
    game.session
        .observe(|world| {
            assert_eq!(receipt.resolve(world, &id), EntityLookup::Resolved(pickup));
            assert!(world.get::<Sprite>(pickup).is_none());
        })
        .unwrap();

    let restored = game.command(movement(1, false));
    assert_eq!(restored.tick, SPRITE_RESTORE_TICK);
    assert_eq!((restored.score, restored.collected), (10, 1));
    game.session
        .observe(|world| {
            assert_eq!(receipt.resolve(world, &id), EntityLookup::Resolved(pickup));
            assert_eq!(world.get::<Sprite>(pickup).unwrap().material.tint.a, 0.0);
        })
        .unwrap();

    let still_overlapping = game.step();
    assert_eq!(
        (still_overlapping.score, still_overlapping.collected),
        (10, 1)
    );
    game.close();
}

#[test]
fn moved_pickup_is_collected_once_at_its_completed_fixed_world_position() {
    let mut game = game_with_pickup_edit(PickupEdit::Move);
    let original_contact = walk_to_authored_contact(&mut game);
    assert_eq!(
        (
            original_contact.score,
            original_contact.collected,
            original_contact.visible_pickups
        ),
        (0, 0, 2)
    );
    let (local, global, alpha) = pickup_state(&game);
    assert_eq!(local, AUTHORED_POSITION);
    assert_eq!(global, Vec2::new(-180.0, 0.0));
    assert!(alpha > 0.0);

    for _ in (AUTHORED_CONTACT_TICK + 1)..MOVED_CONTACT_TICK {
        game.step();
    }
    let approach = game.snapshot();
    assert_eq!(approach.collected, 0);
    assert!(approach.player.distance(MOVED_WORLD_POSITION) > 16.0);

    let contact = game.step();
    assert_eq!(contact.tick, MOVED_CONTACT_TICK);
    assert!(contact.player.distance(AUTHORED_POSITION) > 16.0);
    assert!(contact.player.distance(MOVED_WORLD_POSITION) <= 16.0);
    assert_eq!(
        (contact.score, contact.collected, contact.visible_pickups),
        (10, 1, 1)
    );
    assert_eq!(
        pickup_state(&game),
        (MOVED_LOCAL_POSITION, MOVED_WORLD_POSITION, 0.0)
    );

    let still_overlapping = game.step();
    assert!(still_overlapping.player.distance(MOVED_WORLD_POSITION) <= 16.0);
    assert_eq!(
        (still_overlapping.score, still_overlapping.collected),
        (10, 1)
    );
    game.close();
}
