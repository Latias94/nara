#![cfg(feature = "local-player")]
#![forbid(unsafe_code)]

#[allow(
    dead_code,
    reason = "reuse the production game and physical close fixture"
)]
#[path = "support/resume_gameplay.rs"]
mod support;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs, thread,
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::CurrentScene,
    app::{CoreStage, PluginCategory, PluginDeclaration, PluginId},
    asset::{
        AssetEvents, AssetProductRef, AssetRecord, AssetRef, AssetServer, AssetSlotRevision,
        AssetSourceKind, AssetStates, AssetVersion, Assets, Handle, ProjectAssetDatabase,
    },
    ecs::{
        World,
        bevy_ecs::{lifecycle::HookContext, world::DeferredWorld},
        schedule::IntoScheduleConfigs,
    },
    gameplay::{GameplayCommandBatch, GameplayCommandSet},
    identity::{EntityLookup, SceneEntityId, SpawnedSceneInstance, WorldIdentityDomain},
    image::ImageAsset,
    material::{
        Material2d, Material2dAsset, Material2dBinding, Material2dSource, SamplerDescriptor,
    },
    prelude::{App, Component, Plugin, PluginError, Resource, Transform2d, Vec2},
    project_host::unload_project_scene_addition,
    scene::{SceneEntitySource, retire_and_despawn_scene_instance},
    sprite::Sprite,
    tilemap::{
        TileAtlasLayout, TileSet, TileSetAssets, TileSetSource, TileSetSourceFileLimits, Tilemap,
    },
};
use support::{DEADLINE, Game, MARKER, MAX_WALK_STEPS, ProjectCopy, TRAVEL, action, movement};
use two_level_game::{
    GamePhase, GameSnapshot, RunOptions, SideRoomPhase, capture_resume, configured_recipe,
    levels::{LevelId, LevelKey},
    region_material, region_tileset,
    side_room::{LOOKOUT_RELIC, RELIC, image_product},
};

const SIDE_ROOM: &str = "two-level-game.side-room";
const LOOKOUT: &str = "two-level-game.lookout";
const RELIC_PIXEL: [u8; 4] = region_material::SOURCE_PIXEL;

#[test]
fn a_live_side_room_takes_input_collects_a_relic_and_unloads_only_its_own_image() {
    let mut game = Game::source();
    let before = game.snapshot();
    let primary_image = resident(&game, LevelId::Meadow.image_product()).unwrap();
    assert!(resident(&game, image_product()).is_none());

    game.submit(action(SIDE_ROOM));
    game.submit(movement(2, true));
    assert_eq!(game.session.step().unwrap(), None);
    let loading = game.snapshot();
    assert_eq!(loading.side_room, SideRoomPhase::Preparing);
    assert_eq!(loading.phase, GamePhase::Running);
    assert_eq!(loading.current_instance, before.current_instance);
    assert!(loading.player.y > before.player.y);
    assert!(resident(&game, image_product()).is_none());
    let open = wait_until_prepared(&mut game);
    assert_eq!(open.side_room, SideRoomPhase::Open);
    assert_eq!(open.current_instance, before.current_instance);
    assert_eq!(
        resident(&game, LevelId::Meadow.image_product()),
        Some(primary_image)
    );
    assert!(
        capture_resume(&game.session)
            .unwrap_err()
            .to_string()
            .contains("close the side room")
    );

    let (receipt, relic) = room_receipt(&game, RELIC);
    let (badge, version) = resident(&game, image_product()).unwrap();
    let pixels = game
        .session
        .observe(|world| {
            let EntityLookup::Resolved(entity) = receipt.resolve(world, &relic) else {
                panic!("the relic is live")
            };
            assert_eq!(
                world.get::<Sprite>(entity).unwrap().material.image,
                Some(badge)
            );
            world
                .resource::<Assets<ImageAsset>>()
                .get(badge)
                .unwrap()
                .share_retained()
                .unwrap()
        })
        .unwrap();
    assert_eq!(pixels.pixels(), RELIC_PIXEL.repeat(4));
    for _ in 0..24 {
        if game.step().side_room == SideRoomPhase::RelicFound {
            break;
        }
    }
    assert_eq!(
        game.command(movement(2, false)).side_room,
        SideRoomPhase::RelicFound
    );
    let closed = game.command(action(SIDE_ROOM));
    assert_eq!(closed.side_room, SideRoomPhase::Closed);
    assert_eq!(closed.current_instance, before.current_instance);
    assert_eq!(closed.score, before.score);
    assert!(resident(&game, image_product()).is_none());
    assert_eq!(
        resident(&game, LevelId::Meadow.image_product()),
        Some(primary_image)
    );
    game.session
        .observe(|world| {
            assert!(matches!(
                receipt.resolve(world, &relic),
                EntityLookup::Tombstoned(_)
            ));
            assert_eq!(
                CurrentScene::get(world).unwrap().receipt().instance_id(),
                before.current_instance.unwrap()
            );
        })
        .unwrap();
    assert_eq!(pixels.pixels(), RELIC_PIXEL.repeat(4));
    drop(pixels);
    game.capture();

    game.submit(action(SIDE_ROOM));
    assert_eq!(game.session.step().unwrap(), None);
    assert_eq!(
        wait_until_prepared(&mut game).side_room,
        SideRoomPhase::Open
    );
    let (fresh, _) = room_receipt(&game, RELIC);
    assert_ne!(fresh.instance_id(), receipt.instance_id());
    let (reopened, next_version) = resident(&game, image_product()).unwrap();
    assert_eq!(reopened, badge);
    assert!(next_version > version);
    game.command(action(SIDE_ROOM));
    game.close();
}

#[test]
fn damaged_deferred_pixels_leave_meadow_running_and_a_later_request_can_recover() {
    let source = ProjectCopy::new();
    let mut game = Game::from_project(source.root());
    let before = game.snapshot();
    let path = source
        .root()
        .join("assets")
        .join(two_level_game::side_room::IMAGE_PATH);
    let valid = fs::read(&path).unwrap();
    fs::write(&path, b"broken optional room image").unwrap();
    game.submit(action(SIDE_ROOM));
    game.submit(movement(2, true));
    assert_eq!(game.session.step().unwrap(), None);
    let rejected = wait_until_prepared(&mut game);
    assert!(
        rejected
            .error
            .as_deref()
            .unwrap()
            .contains("image-import-failed"),
        "{rejected:?}"
    );
    assert_eq!(rejected.side_room, SideRoomPhase::Closed);
    assert_eq!(rejected.phase, GamePhase::Running);
    assert_eq!(rejected.current_instance, before.current_instance);
    assert!(rejected.player.y > before.player.y);
    assert!(resident(&game, image_product()).is_none());
    fs::write(path, valid).unwrap();

    let shared_path = source.root().join("assets/textures/meadow.png");
    let original = fs::read(&shared_path).unwrap();
    let shared = resident(&game, LevelId::Meadow.image_product()).unwrap();
    fs::copy(
        source.root().join("assets/textures/marker.png"),
        &shared_path,
    )
    .unwrap();
    game.submit(action(SIDE_ROOM));
    assert_eq!(game.session.step().unwrap(), None);
    let conflict = wait_until_prepared(&mut game);
    assert!(
        conflict
            .error
            .as_deref()
            .unwrap()
            .contains("image-ownership-changed"),
        "{conflict:?}"
    );
    assert_eq!(conflict.side_room, SideRoomPhase::Closed);
    assert_eq!(conflict.current_instance, before.current_instance);
    assert_eq!(
        resident(&game, LevelId::Meadow.image_product()),
        Some(shared)
    );
    assert!(resident(&game, image_product()).is_none());
    fs::write(shared_path, original).unwrap();

    game.submit(movement(2, false));
    game.submit(action(SIDE_ROOM));
    assert_eq!(game.session.step().unwrap(), None);
    assert_eq!(
        wait_until_prepared(&mut game).side_room,
        SideRoomPhase::Open
    );
    assert_eq!(
        game.command(action(SIDE_ROOM)).side_room,
        SideRoomPhase::Closed
    );
    game.close();
}

#[test]
fn both_rooms_share_images_but_collect_and_unload_independently() {
    use SideRoomPhase::{Closed, Open, RelicFound};

    let mut game = Game::source();
    let before = game.snapshot();
    let meadow = LevelId::Meadow.image_product();
    let primary_image = resident(&game, meadow).unwrap();
    assert!(resident(&game, image_product()).is_none());
    let (side, side_relic) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
    let relic_image = resident(&game, image_product()).unwrap();
    let (lookout, lookout_relic) =
        open_room(&mut game, LOOKOUT, LOOKOUT_RELIC, |state| state.lookout);
    assert_ne!(side.instance_id(), lookout.instance_id());
    for receipt in [&side, &lookout] {
        assert_ne!(Some(receipt.instance_id()), before.current_instance);
    }
    assert_eq!(game.snapshot().current_instance, before.current_instance);
    assert_eq!(room_phases(&game.snapshot()), (Open, Open));
    assert_eq!(resident(&game, image_product()), Some(relic_image));
    assert_eq!(resident(&game, meadow), Some(primary_image));
    assert_eq!(
        sprite_facts(&game, &side, &side_relic),
        (Vec2::new(-120.0, 48.0), Some(relic_image.0), 1.0)
    );
    assert_eq!(
        sprite_facts(&game, &lookout, &lookout_relic),
        (Vec2::new(-120.0, -48.0), Some(relic_image.0), 1.0)
    );
    for (receipt, sign) in [(&side, "side-room-sign"), (&lookout, "lookout-sign")] {
        assert_eq!(
            sprite_facts(&game, receipt, &SceneEntityId::new(sign).unwrap()).1,
            Some(primary_image.0)
        );
    }

    let north = collect_relic(&mut game, 2, |state| state.side_room);
    assert_eq!(room_phases(&north), (RelicFound, Open));
    assert_eq!(sprite_facts(&game, &side, &side_relic).2, 0.0);
    assert_eq!(sprite_facts(&game, &lookout, &lookout_relic).2, 1.0);
    let south = collect_relic(&mut game, 3, |state| state.lookout);
    assert_eq!(room_phases(&south), (RelicFound, RelicFound));
    assert_eq!(south.score, before.score);
    assert_eq!(sprite_facts(&game, &lookout, &lookout_relic).2, 0.0);

    let closed = game.command(action(SIDE_ROOM));
    assert_eq!(room_phases(&closed), (Closed, RelicFound));
    assert_eq!(closed.current_instance, before.current_instance);
    assert_retired(&game, &side);
    assert_eq!(room_receipt(&game, LOOKOUT_RELIC).0, lookout);
    assert_eq!(resident(&game, image_product()), Some(relic_image));
    assert_eq!(resident(&game, meadow), Some(primary_image));
    assert!(
        capture_resume(&game.session)
            .unwrap_err()
            .to_string()
            .contains("close the lookout")
    );

    assert_eq!(
        room_phases(&game.command(action(LOOKOUT))),
        (Closed, Closed)
    );
    assert_retired(&game, &lookout);
    assert!(resident(&game, image_product()).is_none());
    assert_eq!(resident(&game, meadow), Some(primary_image));
    game.close();
}

#[test]
fn both_rooms_share_one_authored_tileset_until_either_last_user_closes() {
    for close_side_first in [true, false] {
        let mut game = Game::source();
        let before = game.snapshot();
        let primary_image = resident(&game, LevelId::Meadow.image_product()).unwrap();
        assert!(resident_tileset(&game).is_none());
        let (side, _) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
        let publication = resident_tileset(&game).unwrap();
        let tileset = room_tileset(&game, &side, region_tileset::SIDE_ROOM_FLOOR);
        let image = tileset.image_content().unwrap().clone();
        let relic_image = resident(&game, image_product()).unwrap();
        let (lookout, _) = open_room(&mut game, LOOKOUT, LOOKOUT_RELIC, |state| state.lookout);
        assert_eq!(resident_tileset(&game), Some(publication.clone()));
        assert_eq!(
            room_tileset(&game, &lookout, region_tileset::LOOKOUT_FLOOR),
            tileset
        );
        assert_eq!(image.handle(), relic_image.0);
        assert_eq!(image.version(), relic_image.1);
        assert_eq!(image.product(), image_product());
        assert_eq!(image.pixels(), RELIC_PIXEL.repeat(4));
        let record = game
            .session
            .observe(|world| {
                assert_eq!(world.resource::<Assets<TileSet>>().len(), 1);
                world
                    .resource::<ProjectAssetDatabase>()
                    .record_for_stable_id(region_tileset::tileset_product().source())
                    .unwrap()
                    .clone()
            })
            .unwrap();

        let (first, last, retired, survivor, floor) = if close_side_first {
            (
                SIDE_ROOM,
                LOOKOUT,
                &side,
                &lookout,
                region_tileset::LOOKOUT_FLOOR,
            )
        } else {
            (
                LOOKOUT,
                SIDE_ROOM,
                &lookout,
                &side,
                region_tileset::SIDE_ROOM_FLOOR,
            )
        };
        game.command(action(first));
        assert_retired(&game, retired);
        assert_eq!(resident_tileset(&game), Some(publication.clone()));
        let retained = room_tileset(&game, survivor, floor);
        assert_eq!(retained, tileset);
        assert!(retained.image_content().unwrap().same_allocation(&image));
        assert_eq!(resident(&game, image_product()), Some(relic_image));
        game.session
            .observe(|world| {
                assert_eq!(
                    world
                        .resource::<ProjectAssetDatabase>()
                        .record_for_path(record.path()),
                    Some(&record)
                );
            })
            .unwrap();

        let closed = game.command(action(last));
        assert_eq!(
            room_phases(&closed),
            (SideRoomPhase::Closed, SideRoomPhase::Closed)
        );
        assert_eq!(closed.current_instance, before.current_instance);
        assert_retired(&game, survivor);
        assert!(resident_tileset(&game).is_none());
        assert!(resident(&game, image_product()).is_none());
        assert_eq!(
            resident(&game, LevelId::Meadow.image_product()),
            Some(primary_image)
        );
        game.session
            .observe(|world| {
                let assets = world.resource::<Assets<TileSet>>();
                let states = world.resource::<AssetStates>();
                assert!(assets.get(publication.0).is_none());
                assert!(
                    states
                        .state(publication.0.id())
                        .unwrap()
                        .usable_version()
                        .is_none()
                );
                assert!(
                    world
                        .resource::<TileSetAssets>()
                        .content_receipt(publication.0, assets, states)
                        .is_none()
                );
                let database = world.resource::<ProjectAssetDatabase>();
                assert!(database.record_for_stable_id(record.stable_id()).is_none());
                assert!(database.record_for_path(record.path()).is_none());
                assert!(!image.is_current(
                    world.resource::<AssetServer>(),
                    states,
                    world.resource::<Assets<ImageAsset>>()
                ));
            })
            .unwrap();
        assert_eq!(image.pixels(), RELIC_PIXEL.repeat(4));
        assert_eq!(tileset.atlas(), Some(region_tileset::atlas()));

        let (reopened, _) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
        let next = resident_tileset(&game).unwrap();
        assert_eq!(next.0, publication.0);
        assert_ne!(next.1, publication.1);
        assert!(next.2 > publication.2);
        let fresh = room_tileset(&game, &reopened, region_tileset::SIDE_ROOM_FLOOR);
        assert!(fresh.image_content().unwrap().version() > image.version());
        assert_eq!(image.pixels(), RELIC_PIXEL.repeat(4));
        game.command(action(SIDE_ROOM));
        assert!(resident_tileset(&game).is_none());
        game.close();
    }
}

#[test]
fn parented_relic_uses_its_completed_world_position_for_pickup() {
    let source = ProjectCopy::new();
    let path = source.root().join("scenes/side-room.scene.json");
    let mut scene: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let entities = scene["payload"]["entities"].as_array_mut().unwrap();
    for (id, x, y) in [
        (region_tileset::SIDE_ROOM_FLOOR, 0.0, 28.0),
        (RELIC, -120.0, 24.0),
    ] {
        let entity = entities
            .iter_mut()
            .find(|entity| entity["id"] == id)
            .unwrap();
        let position = &mut entity["components"]["nara.transform.Transform2d"]["value"]["value"]["translation"]
            ["value"];
        position["x"]["value"] = serde_json::json!(x);
        position["y"]["value"] = serde_json::json!(y);
    }
    let placement = scene["payload"]["placements"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|placement| placement["target"]["entity"]["entity"] == RELIC)
        .unwrap();
    placement["placement"]["parent"] = serde_json::json!({
        "kind": "entity",
        "target": { "through": [], "entity": { "kind": "source_entity", "entity": region_tileset::SIDE_ROOM_FLOOR } },
    });
    fs::write(path, serde_json::to_vec_pretty(&scene).unwrap()).unwrap();

    let mut game = Game::from_project(source.root());
    let (room, relic) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
    let local = Vec2::new(-120.0, 24.0);
    let target = Vec2::new(-120.0, 52.0);
    assert_eq!(sprite_facts(&game, &room, &relic).0, local);
    assert_eq!(
        game.session
            .observe(|world| {
                let EntityLookup::Resolved(entity) = room.resolve(world, &relic) else {
                    panic!("the relic must resolve in the added room");
                };
                nara::transform::historical_spatial_snapshot(world)
                    .unwrap()
                    .global(entity)
                    .unwrap()
                    .translation()
            })
            .unwrap(),
        target
    );

    let mut player = game.command(movement(2, true));
    for _ in 0..MAX_WALK_STEPS {
        if player.player.y >= local.y {
            break;
        }
        player = game.step();
    }
    assert!(player.player.distance(local) < 5.0, "{player:?}");
    assert_eq!(player.side_room, SideRoomPhase::Open);
    assert_eq!(sprite_facts(&game, &room, &relic).2, 1.0);
    for _ in 0..MAX_WALK_STEPS {
        if player.side_room == SideRoomPhase::RelicFound {
            break;
        }
        player = game.step();
    }
    assert_eq!(player.side_room, SideRoomPhase::RelicFound);
    assert!(player.player.distance(target) <= 10.0, "{player:?}");
    assert_eq!(sprite_facts(&game, &room, &relic).2, 0.0);
    game.close();
}

#[test]
fn last_tileset_user_unload_refuses_before_retirement_when_asset_events_are_scoped_out() {
    let project = ProjectCopy::new();
    for (file, entity, component) in [
        ("side-room.scene.json", RELIC, "nara.material2d"),
        (
            "lookout.scene.json",
            region_tileset::LOOKOUT_FLOOR,
            "nara.tilemap.Tilemap",
        ),
    ] {
        let path = project.root().join("scenes").join(file);
        let mut scene: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let entity = scene["payload"]["entities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|record| record["id"] == entity)
            .unwrap();
        assert!(
            entity["components"]
                .as_object_mut()
                .unwrap()
                .remove(component)
                .is_some()
        );
        fs::write(path, serde_json::to_vec_pretty(&scene).unwrap()).unwrap();
    }

    let recipe = configured_recipe(RunOptions::default())
        .add_plugin::<RetirementProbe>()
        .unwrap();
    let mut game = Game::from_project_with_recipe(project.root(), recipe);
    let (lookout, _) = open_room(&mut game, LOOKOUT, LOOKOUT_RELIC, |state| state.lookout);
    assert!(resident_tileset(&game).is_none());
    let (side, _) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
    let before = game.snapshot();
    let atlas = resident_tileset(&game).unwrap();
    let image = resident(&game, image_product()).unwrap();
    let material = resident_material(&game).unwrap();
    let retained = room_tileset(&game, &side, region_tileset::SIDE_ROOM_FLOOR);

    let refused = game.command(action(UNLOAD_WITHOUT_ASSET_EVENTS));
    assert_eq!(room_phases(&refused), room_phases(&before));
    assert_eq!(refused.current_instance, before.current_instance);
    assert_eq!(room_receipt(&game, RELIC).0, side);
    assert_eq!(room_receipt(&game, LOOKOUT_RELIC).0, lookout);
    assert_eq!(resident_tileset(&game), Some(atlas));
    assert_eq!(resident(&game, image_product()), Some(image));
    assert_eq!(resident_material(&game), Some(material));

    assert_eq!(
        game.command(action(SIDE_ROOM)).side_room,
        SideRoomPhase::Closed
    );
    assert_retired(&game, &side);
    assert!(resident_tileset(&game).is_none());
    assert_eq!(resident(&game, image_product()), Some(image));
    assert_eq!(resident_material(&game), Some(material));
    assert_eq!(room_receipt(&game, LOOKOUT_RELIC).0, lookout);
    assert_eq!(
        retained.image_content().unwrap().pixels(),
        RELIC_PIXEL.repeat(4)
    );

    assert_eq!(game.command(action(LOOKOUT)).lookout, SideRoomPhase::Closed);
    assert_retired(&game, &lookout);
    assert!(resident(&game, image_product()).is_none());
    assert!(resident_material(&game).is_none());
    game.close();
}

#[test]
fn changed_live_tileset_rejects_another_room_without_publication_and_repair_retries() {
    let project = ProjectCopy::new();
    let path = project
        .root()
        .join("assets")
        .join(region_tileset::TILESET_PATH);
    let original = fs::read(&path).unwrap();
    let source =
        TileSetSource::decode_ron_bytes(&original, TileSetSourceFileLimits::default()).unwrap();
    let mut game = Game::from_project(project.root());
    let (side, _) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
    let publication = resident_tileset(&game).unwrap();
    let tileset = room_tileset(&game, &side, region_tileset::SIDE_ROOM_FLOOR);
    let material = resident_material(&game).unwrap();
    let relic_image = resident(&game, image_product()).unwrap();
    let primary_image = resident(&game, LevelId::Meadow.image_product()).unwrap();

    for change in ["sampler", "tint", "layout"] {
        let mut changed = source.clone();
        match change {
            "sampler" => changed.sampler = SamplerDescriptor::LINEAR_CLAMP,
            "tint" => changed.tint.r = 0.5,
            "layout" => changed.atlas = TileAtlasLayout::grid(Vec2::new(0.5, 2.0), 4, 1),
            _ => unreachable!(),
        }
        changed
            .validate_image_extent(tileset.image_content().unwrap().extent())
            .unwrap();
        let before = game.snapshot();
        let (entities, identities, catalog) = game
            .session
            .observe(|world| {
                (
                    world
                        .iter_entities()
                        .map(|entity| entity.id())
                        .collect::<BTreeSet<_>>(),
                    world.resource::<WorldIdentityDomain>().stats(),
                    world.resource::<ProjectAssetDatabase>().clone(),
                )
            })
            .unwrap();
        fs::write(&path, changed.encode_ron_bytes().unwrap()).unwrap();

        game.submit(action(LOOKOUT));
        assert_eq!(game.session.step().unwrap(), None);
        let rejected = wait_for(&mut game, |state| state.lookout != SideRoomPhase::Preparing);
        assert_eq!(
            room_phases(&rejected),
            (SideRoomPhase::Open, SideRoomPhase::Closed)
        );
        assert_eq!(rejected.phase, GamePhase::Running);
        assert_eq!(rejected.current_instance, before.current_instance);
        assert_eq!(rejected.player, before.player);
        assert_eq!(rejected.score, before.score);
        assert_eq!(rejected.travel_count, before.travel_count);
        assert!(
            rejected.error.as_deref().is_some_and(|error| {
                error.contains("lookout activation rejected")
                    && error.contains("project.scene.tileset-preparation-failed")
            }),
            "{change}: {rejected:?}"
        );
        game.session
            .observe(|world| {
                assert_eq!(
                    world
                        .iter_entities()
                        .map(|entity| entity.id())
                        .collect::<BTreeSet<_>>(),
                    entities
                );
                assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identities);
                assert_eq!(world.resource::<ProjectAssetDatabase>(), &catalog);
            })
            .unwrap();
        assert_eq!(room_receipt(&game, RELIC).0, side);
        assert_eq!(resident_tileset(&game), Some(publication.clone()));
        assert_eq!(
            room_tileset(&game, &side, region_tileset::SIDE_ROOM_FLOOR),
            tileset
        );
        assert_eq!(resident_material(&game), Some(material));
        assert_eq!(resident(&game, image_product()), Some(relic_image));
        assert_eq!(
            resident(&game, LevelId::Meadow.image_product()),
            Some(primary_image)
        );

        fs::write(&path, &original).unwrap();
        let (lookout, _) = open_room(&mut game, LOOKOUT, LOOKOUT_RELIC, |state| state.lookout);
        assert_eq!(resident_tileset(&game), Some(publication.clone()));
        assert_eq!(
            room_tileset(&game, &lookout, region_tileset::LOOKOUT_FLOOR),
            tileset
        );
        game.command(action(LOOKOUT));
        assert_retired(&game, &lookout);
        assert_eq!(resident_tileset(&game), Some(publication.clone()));
    }
    game.command(action(SIDE_ROOM));
    assert!(resident_tileset(&game).is_none());
    game.close();
}

#[test]
fn an_invalid_persisted_tile_index_rejects_either_room_before_mutating_the_live_group() {
    for invalid_side in [true, false] {
        let project = ProjectCopy::new();
        let mut game = Game::from_project(project.root());
        let (command, relic, scene, floor) = if invalid_side {
            (
                SIDE_ROOM,
                RELIC,
                "side-room.scene.json",
                region_tileset::SIDE_ROOM_FLOOR,
            )
        } else {
            (
                LOOKOUT,
                LOOKOUT_RELIC,
                "lookout.scene.json",
                region_tileset::LOOKOUT_FLOOR,
            )
        };
        let (incumbent_command, incumbent_relic, incumbent_floor) = if invalid_side {
            (LOOKOUT, LOOKOUT_RELIC, region_tileset::LOOKOUT_FLOOR)
        } else {
            (SIDE_ROOM, RELIC, region_tileset::SIDE_ROOM_FLOOR)
        };
        let phase: fn(&GameSnapshot) -> SideRoomPhase = if invalid_side {
            |state| state.side_room
        } else {
            |state| state.lookout
        };
        let incumbent_phase: fn(&GameSnapshot) -> SideRoomPhase = if invalid_side {
            |state| state.lookout
        } else {
            |state| state.side_room
        };
        let (incumbent, _) = open_room(
            &mut game,
            incumbent_command,
            incumbent_relic,
            incumbent_phase,
        );
        let tileset = room_tileset(&game, &incumbent, incumbent_floor);
        let publication = resident_tileset(&game).unwrap();
        let material = resident_material(&game).unwrap();
        let primary_image = resident(&game, LevelId::Meadow.image_product()).unwrap();
        let relic_image = resident(&game, image_product()).unwrap();
        let before = game.snapshot();
        let (entities, identities, catalog) = game
            .session
            .observe(|world| {
                (
                    world
                        .iter_entities()
                        .map(|entity| entity.id())
                        .collect::<BTreeSet<_>>(),
                    world.resource::<WorldIdentityDomain>().stats(),
                    world.resource::<ProjectAssetDatabase>().clone(),
                )
            })
            .unwrap();
        let path = project.root().join("scenes").join(scene);
        let original = fs::read(&path).unwrap();
        let mut invalid: serde_json::Value = serde_json::from_slice(&original).unwrap();
        let floor_entity = invalid["payload"]["entities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entity| entity["id"] == floor)
            .unwrap();
        floor_entity["components"]["nara.tilemap.Tilemap"]["value"]["value"]["cells"]["value"][0]
            ["value"]["cell"]["value"]["tile"]["value"] = serde_json::json!(4);
        fs::write(&path, serde_json::to_vec_pretty(&invalid).unwrap()).unwrap();

        game.submit(action(command));
        assert_eq!(game.session.step().unwrap(), None);
        let rejected = wait_for(&mut game, |state| phase(state) != SideRoomPhase::Preparing);
        assert_eq!(phase(&rejected), SideRoomPhase::Closed);
        assert_eq!(incumbent_phase(&rejected), SideRoomPhase::Open);
        assert_eq!(rejected.phase, GamePhase::Running);
        assert_eq!(rejected.current_instance, before.current_instance);
        assert_eq!(rejected.player, before.player);
        assert_eq!(rejected.score, before.score);
        assert_eq!(rejected.travel_count, before.travel_count);
        assert!(
            rejected.error.as_deref().is_some_and(|error| {
                error.contains("preparation rejected")
                    && error.contains("project.content.tile-index-out-of-range")
            }),
            "{rejected:?}"
        );
        game.session
            .observe(|world| {
                assert_eq!(
                    world
                        .iter_entities()
                        .map(|entity| entity.id())
                        .collect::<BTreeSet<_>>(),
                    entities
                );
                assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identities);
                assert_eq!(world.resource::<ProjectAssetDatabase>(), &catalog);
            })
            .unwrap();
        assert_eq!(room_receipt(&game, incumbent_relic).0, incumbent);
        assert_eq!(resident_tileset(&game), Some(publication.clone()));
        assert_eq!(room_tileset(&game, &incumbent, incumbent_floor), tileset);
        assert_eq!(resident_material(&game), Some(material));
        assert_eq!(resident(&game, image_product()), Some(relic_image));
        assert_eq!(
            resident(&game, LevelId::Meadow.image_product()),
            Some(primary_image)
        );

        fs::write(path, original).unwrap();
        let (repaired, _) = open_room(&mut game, command, relic, phase);
        assert_eq!(resident_tileset(&game), Some(publication));
        assert_eq!(room_tileset(&game, &repaired, floor), tileset);
        game.command(action(command));
        game.command(action(incumbent_command));
        assert!(resident_tileset(&game).is_none());
        game.close();
    }
}

#[test]
fn both_rooms_share_one_authored_material_until_either_last_user_closes() {
    for close_side_first in [true, false] {
        let mut game = Game::source();
        let primary_image = resident(&game, region_material::palette_product()).unwrap();
        assert!(resident_material(&game).is_none());
        let (side, side_relic) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
        let publication = resident_material(&game).unwrap();
        let binding = room_material(&game, &side, &side_relic);
        let generation = binding.generation().clone();
        let shader = generation.shader().clone();
        let (lookout, lookout_relic) =
            open_room(&mut game, LOOKOUT, LOOKOUT_RELIC, |state| state.lookout);
        assert_eq!(resident_material(&game), Some(publication));
        assert_eq!(room_material(&game, &lookout, &lookout_relic), binding);
        assert_eq!(
            binding.textures()[region_material::PALETTE_ID],
            primary_image.0
        );
        assert_eq!(
            binding.generation().declaration(),
            &region_material::declaration()
        );
        assert_eq!(binding.parameters(), &region_material::parameters());
        let uniform = region_material::GRADE
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(binding.uniform_bytes(), uniform);
        assert_eq!(shader.user_source().trim(), region_material::SHADER.trim());
        let relic_image = resident(&game, image_product()).unwrap();
        let (pixels, palette, record) = game
            .session
            .observe(|world| {
                let images = world.resource::<Assets<ImageAsset>>();
                (
                    images.get(relic_image.0).unwrap().share_retained().unwrap(),
                    images
                        .get(primary_image.0)
                        .unwrap()
                        .share_retained()
                        .unwrap(),
                    world
                        .resource::<ProjectAssetDatabase>()
                        .record_for_stable_id(region_material::material_product().source())
                        .unwrap()
                        .clone(),
                )
            })
            .unwrap();
        assert_eq!(pixels.pixels(), RELIC_PIXEL.repeat(4));
        assert_eq!(&palette.pixels()[..4], &region_material::PALETTE_PIXEL);

        let (first, last, retired, survivor, relic) = if close_side_first {
            (SIDE_ROOM, LOOKOUT, &side, &lookout, &lookout_relic)
        } else {
            (LOOKOUT, SIDE_ROOM, &lookout, &side, &side_relic)
        };
        game.command(action(first));
        assert_retired(&game, retired);
        assert_eq!(resident_material(&game), Some(publication));
        assert_eq!(resident(&game, image_product()), Some(relic_image));
        assert_eq!(room_material(&game, survivor, relic), binding);
        game.session
            .observe(|world| {
                assert_eq!(
                    world
                        .resource::<ProjectAssetDatabase>()
                        .record_for_path(record.path()),
                    Some(&record)
                );
            })
            .unwrap();

        let closed = game.command(action(last));
        assert_eq!(
            room_phases(&closed),
            (SideRoomPhase::Closed, SideRoomPhase::Closed)
        );
        assert_retired(&game, survivor);
        assert!(resident_material(&game).is_none());
        assert!(resident(&game, image_product()).is_none());
        assert_eq!(
            resident(&game, region_material::palette_product()),
            Some(primary_image)
        );
        game.session
            .observe(|world| {
                assert!(
                    world
                        .resource::<Assets<Material2dAsset>>()
                        .get(publication.0)
                        .is_none()
                );
                assert!(
                    world
                        .resource::<AssetStates>()
                        .state(publication.0.id())
                        .unwrap()
                        .usable_version()
                        .is_none()
                );
                let database = world.resource::<ProjectAssetDatabase>();
                assert!(database.record_for_stable_id(record.stable_id()).is_none());
                assert!(database.record_for_path(record.path()).is_none());
            })
            .unwrap();
        let retained = generation
            .bind_values(region_material::parameters(), binding.textures().clone())
            .unwrap();
        assert_eq!(retained, binding);
        assert_eq!(retained.generation().shader(), &shader);
        assert_eq!(retained.uniform_bytes(), uniform);
        assert_eq!(pixels.pixels(), RELIC_PIXEL.repeat(4));
        assert_eq!(&palette.pixels()[..4], &region_material::PALETTE_PIXEL);

        let (reopened, relic) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
        let next = resident_material(&game).unwrap();
        assert_eq!(next.0, publication.0);
        assert!(next.1 > publication.1);
        let fresh = room_material(&game, &reopened, &relic);
        assert_ne!(fresh.generation(), retained.generation());
        assert_eq!(retained.generation().shader(), &shader);
        game.command(action(SIDE_ROOM));
        assert!(resident_material(&game).is_none());
        game.close();
    }
}

#[test]
fn changed_live_shader_or_material_rejects_another_room_and_repair_retries_exactly() {
    let source = ProjectCopy::new();
    let mut game = Game::from_project(source.root());
    let (side, side_relic) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
    let publication = resident_material(&game).unwrap();
    let binding = room_material(&game, &side, &side_relic);
    let relic_image = resident(&game, image_product()).unwrap();
    let primary_image = resident(&game, region_material::palette_product()).unwrap();

    for relative in [region_material::SHADER_PATH, region_material::MATERIAL_PATH] {
        let path = source.root().join("assets").join(relative);
        let original = fs::read(&path).unwrap();
        let changed = if relative == region_material::SHADER_PATH {
            let shader = String::from_utf8(original.clone()).unwrap();
            let replacement = shader.replace("sampled.rgb *", "sampled.bgr *");
            assert_ne!(replacement, shader);
            replacement.into_bytes()
        } else {
            let mut material: serde_json::Value = serde_json::from_slice(&original).unwrap();
            material["params"][region_material::GRADE_ID]["vec4"][0] = serde_json::json!(0.5);
            let replacement = serde_json::to_vec_pretty(&material).unwrap();
            assert!(Material2dSource::decode(&replacement).is_ok());
            replacement
        };
        let before = game.snapshot();
        let (entities, identities, catalog) = game
            .session
            .observe(|world| {
                (
                    world
                        .iter_entities()
                        .map(|entity| entity.id())
                        .collect::<BTreeSet<_>>(),
                    world.resource::<WorldIdentityDomain>().stats(),
                    world.resource::<ProjectAssetDatabase>().clone(),
                )
            })
            .unwrap();
        fs::write(&path, changed).unwrap();

        game.submit(action(LOOKOUT));
        assert_eq!(game.session.step().unwrap(), None);
        let rejected = wait_for(&mut game, |state| state.lookout != SideRoomPhase::Preparing);
        assert_eq!(
            room_phases(&rejected),
            (SideRoomPhase::Open, SideRoomPhase::Closed)
        );
        assert_eq!(rejected.phase, GamePhase::Running);
        assert_eq!(rejected.current_instance, before.current_instance);
        assert_eq!(rejected.player, before.player);
        assert_eq!(rejected.score, before.score);
        assert_eq!(rejected.travel_count, before.travel_count);
        assert!(
            rejected.error.as_deref().is_some_and(|error| {
                error.contains("lookout activation rejected")
                    && error.contains("project.scene.material-ownership-changed")
            }),
            "{relative}: {rejected:?}"
        );
        game.session
            .observe(|world| {
                assert_eq!(
                    world
                        .iter_entities()
                        .map(|entity| entity.id())
                        .collect::<BTreeSet<_>>(),
                    entities
                );
                assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identities);
                assert_eq!(world.resource::<ProjectAssetDatabase>(), &catalog);
            })
            .unwrap();
        assert_eq!(room_receipt(&game, RELIC).0, side);
        assert_eq!(resident_material(&game), Some(publication));
        assert_eq!(room_material(&game, &side, &side_relic), binding);
        assert_eq!(resident(&game, image_product()), Some(relic_image));
        assert_eq!(
            resident(&game, region_material::palette_product()),
            Some(primary_image)
        );

        fs::write(path, original).unwrap();
        let (lookout, lookout_relic) =
            open_room(&mut game, LOOKOUT, LOOKOUT_RELIC, |state| state.lookout);
        assert_eq!(resident_material(&game), Some(publication));
        assert_eq!(room_material(&game, &lookout, &lookout_relic), binding);
        let closed = game.command(action(LOOKOUT));
        assert_eq!(
            room_phases(&closed),
            (SideRoomPhase::Open, SideRoomPhase::Closed)
        );
        assert_retired(&game, &lookout);
        assert_eq!(resident_material(&game), Some(publication));
    }
    game.command(action(SIDE_ROOM));
    assert!(resident_material(&game).is_none());
    game.close();
}

#[test]
fn a_missing_shared_image_catalog_record_rejects_a_second_room_without_publication() {
    use SideRoomPhase::{Closed, Open};

    let recipe = configured_recipe(RunOptions::default())
        .add_plugin::<RetirementProbe>()
        .unwrap();
    let mut game = Game::with_recipe(recipe);
    let before = game.snapshot();
    let meadow = LevelId::Meadow.image_product();
    let primary_image = resident(&game, meadow).unwrap();
    let (side, side_relic) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
    let relic_image = resident(&game, image_product()).unwrap();
    let (entities, identities, catalog) = game
        .session
        .observe(|world| {
            (
                world
                    .iter_entities()
                    .map(|entity| entity.id())
                    .collect::<BTreeSet<_>>(),
                world.resource::<WorldIdentityDomain>().stats(),
                world.resource::<ProjectAssetDatabase>().clone(),
            )
        })
        .unwrap();
    let record = catalog
        .record_for_stable_id(image_product().source())
        .unwrap()
        .clone();
    assert_eq!(record.source_kind(), &AssetSourceKind::Image);
    let mut missing_catalog = catalog.clone();
    assert!(missing_catalog.remove_exact(&record));
    game.command(action(REMOVE_ROOM_IMAGE_RECORD));
    game.session
        .observe(|world| {
            assert_eq!(
                world.resource::<RetirementProbe>().removed_image.as_ref(),
                Some(&record)
            );
            assert_eq!(world.resource::<ProjectAssetDatabase>(), &missing_catalog);
        })
        .unwrap();

    game.submit(action(LOOKOUT));
    assert_eq!(game.session.step().unwrap(), None);
    let rejected = wait_for(&mut game, |state| state.lookout != SideRoomPhase::Preparing);
    assert_eq!(room_phases(&rejected), (Open, Closed));
    assert_eq!(rejected.phase, GamePhase::Running);
    assert_eq!(rejected.current_instance, before.current_instance);
    assert!(
        rejected.error.as_deref().is_some_and(|error| {
            error.contains("lookout activation rejected")
                && error.contains("project.scene.asset-catalog-changed")
        }),
        "{rejected:?}"
    );
    game.session
        .observe(|world| {
            assert_eq!(
                world
                    .iter_entities()
                    .map(|entity| entity.id())
                    .collect::<BTreeSet<_>>(),
                entities
            );
            assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identities);
            assert_eq!(world.resource::<ProjectAssetDatabase>(), &missing_catalog);
        })
        .unwrap();
    assert_eq!(room_receipt(&game, RELIC).0, side);
    assert_eq!(
        sprite_facts(&game, &side, &side_relic).1,
        Some(relic_image.0)
    );
    assert_eq!(resident(&game, image_product()), Some(relic_image));
    assert_eq!(resident(&game, meadow), Some(primary_image));

    game.command(action(RESTORE_ROOM_IMAGE_RECORD));
    game.session
        .observe(|world| assert_eq!(world.resource::<ProjectAssetDatabase>(), &catalog))
        .unwrap();
    let (lookout, lookout_relic) =
        open_room(&mut game, LOOKOUT, LOOKOUT_RELIC, |state| state.lookout);
    assert_eq!(room_phases(&game.snapshot()), (Open, Open));
    assert_eq!(
        sprite_facts(&game, &lookout, &lookout_relic).1,
        Some(relic_image.0)
    );
    assert_eq!(resident(&game, image_product()), Some(relic_image));
    game.command(action(LOOKOUT));
    assert_eq!(resident(&game, image_product()), Some(relic_image));
    assert_eq!(
        room_phases(&game.command(action(SIDE_ROOM))),
        (Closed, Closed)
    );
    for receipt in [&side, &lookout] {
        assert_retired(&game, receipt);
    }
    assert!(resident(&game, image_product()).is_none());
    game.close();
}

#[test]
fn host_close_releases_shared_images_after_another_room_is_retired_directly() {
    use SideRoomPhase::{Closed, Open};

    let recipe = configured_recipe(RunOptions::default())
        .add_plugin::<RetirementProbe>()
        .unwrap();
    let mut game = Game::with_recipe(recipe);
    let before = game.snapshot();
    let meadow = LevelId::Meadow.image_product();
    let primary_image = resident(&game, meadow).unwrap();
    let (entities, catalog, active_instances, active_entities) = game
        .session
        .observe(|world| {
            let identities = world.resource::<WorldIdentityDomain>().stats();
            (
                world
                    .iter_entities()
                    .map(|entity| entity.id())
                    .collect::<BTreeSet<_>>(),
                world.resource::<ProjectAssetDatabase>().clone(),
                identities.active_scene_instances,
                identities.active_scene_entities,
            )
        })
        .unwrap();
    let (side, side_relic) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
    let (lookout, lookout_relic) =
        open_room(&mut game, LOOKOUT, LOOKOUT_RELIC, |state| state.lookout);
    let relic_image = resident(&game, image_product()).unwrap();
    for (receipt, relic) in [(&side, &side_relic), (&lookout, &lookout_relic)] {
        assert_eq!(sprite_facts(&game, receipt, relic).1, Some(relic_image.0));
    }
    let (record, pixels) = game
        .session
        .observe(|world| {
            (
                world
                    .resource::<ProjectAssetDatabase>()
                    .record_for_stable_id(image_product().source())
                    .unwrap()
                    .clone(),
                world
                    .resource::<Assets<ImageAsset>>()
                    .get(relic_image.0)
                    .unwrap()
                    .share_retained()
                    .unwrap(),
            )
        })
        .unwrap();

    let retired = game.command(action(RETIRE_SIDE_ROOM));
    assert_eq!(room_phases(&retired), (Open, Open));
    assert_retired(&game, &side);
    assert_eq!(room_receipt(&game, LOOKOUT_RELIC).0, lookout);
    assert_eq!(resident(&game, image_product()), Some(relic_image));
    game.session
        .observe(|world| {
            assert_eq!(
                world.resource::<RetirementProbe>().retired.as_ref(),
                Some(&side)
            );
            assert_eq!(
                world
                    .resource::<ProjectAssetDatabase>()
                    .record_for_path(record.path()),
                Some(&record)
            );
        })
        .unwrap();

    let closed = game.command(action(LOOKOUT));
    assert_eq!(room_phases(&closed), (Open, Closed));
    assert_eq!(closed.current_instance, before.current_instance);
    assert_retired(&game, &lookout);
    assert!(resident(&game, image_product()).is_none());
    assert_eq!(resident(&game, meadow), Some(primary_image));
    game.session
        .observe(|world| {
            let database = world.resource::<ProjectAssetDatabase>();
            assert!(database.record_for_stable_id(record.stable_id()).is_none());
            assert!(database.record_for_path(record.path()).is_none());
            assert!(
                world
                    .resource::<Assets<ImageAsset>>()
                    .get(relic_image.0)
                    .is_none()
            );
            assert_eq!(
                world
                    .iter_entities()
                    .map(|entity| entity.id())
                    .collect::<BTreeSet<_>>(),
                entities
            );
        })
        .unwrap();
    assert_eq!(pixels.pixels(), RELIC_PIXEL.repeat(4));

    let closed = game.command(action(SIDE_ROOM));
    assert_eq!(room_phases(&closed), (Closed, Closed));
    assert_eq!(closed.scene_members, before.scene_members);
    assert_eq!(closed.current_instance, before.current_instance);
    assert_retired(&game, &side);
    assert!(resident(&game, image_product()).is_none());
    assert_eq!(resident(&game, meadow), Some(primary_image));
    game.session
        .observe(|world| {
            let identities = world.resource::<WorldIdentityDomain>().stats();
            assert_eq!(identities.active_scene_instances, active_instances);
            assert_eq!(identities.active_scene_entities, active_entities);
            assert_eq!(world.resource::<ProjectAssetDatabase>(), &catalog);
            assert!(
                world
                    .resource::<Assets<ImageAsset>>()
                    .get(relic_image.0)
                    .is_none()
            );
            assert_eq!(
                world
                    .iter_entities()
                    .map(|entity| entity.id())
                    .collect::<BTreeSet<_>>(),
                entities
            );
        })
        .unwrap();
    assert_eq!(pixels.pixels(), RELIC_PIXEL.repeat(4));
    drop(pixels);
    game.close();
}

#[test]
fn travel_rejection_keeps_both_rooms_and_successful_travel_and_restore_retire_them() {
    use SideRoomPhase::{Closed, Open, RelicFound};

    let source = ProjectCopy::new();
    let recipe = configured_recipe(RunOptions {
        save_path: Some(source.save_path()),
        ..RunOptions::default()
    })
    .add_plugin::<RetirementProbe>()
    .unwrap();
    let mut game = Game::from_project_with_recipe(source.root(), recipe);
    let (side, _) = open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
    let (lookout, _) = open_room(&mut game, LOOKOUT, LOOKOUT_RELIC, |state| state.lookout);
    collect_relic(&mut game, 2, |state| state.side_room);
    let incumbent = game.command(action(MARKER));
    assert_eq!(incumbent.markers.len(), 1);
    let relic_image = resident(&game, image_product()).unwrap();
    let meadow = LevelId::Meadow.image_product();
    let primary_image = resident(&game, meadow).unwrap();
    let material = resident_material(&game).unwrap();
    let tileset_publication = resident_tileset(&game).unwrap();
    let tileset = room_tileset(&game, &side, region_tileset::SIDE_ROOM_FLOOR);
    let retained_image = tileset.image_content().unwrap().clone();
    assert_eq!(
        room_tileset(&game, &lookout, region_tileset::LOOKOUT_FLOOR),
        tileset
    );
    let assert_retained = |game: &Game, state: GameSnapshot| {
        assert_eq!(state.level, incumbent.level);
        assert_eq!(state.current_instance, incumbent.current_instance);
        assert_eq!(state.player, incumbent.player);
        assert_eq!(state.travel_count, 0);
        assert_eq!(state.markers, incumbent.markers);
        assert_eq!(room_phases(&state), (RelicFound, Open));
        assert_eq!(room_receipt(game, RELIC).0, side);
        assert_eq!(room_receipt(game, LOOKOUT_RELIC).0, lookout);
        assert_eq!(resident(game, image_product()), Some(relic_image));
        assert_eq!(resident(game, meadow), Some(primary_image));
        assert_eq!(resident_material(game), Some(material));
        assert_eq!(resident_tileset(game), Some(tileset_publication.clone()));
        assert_eq!(
            room_tileset(game, &side, region_tileset::SIDE_ROOM_FLOOR),
            tileset
        );
        assert_eq!(
            room_tileset(game, &lookout, region_tileset::LOOKOUT_FLOOR),
            tileset
        );
    };

    let saved = game.command(action("two-level-game.save"));
    assert_eq!(saved.saves, incumbent.saves + 1);
    let room_saves = saved.saves;
    assert_retained(&game, saved);

    let cavern_file = source.root().join("scenes/cavern.scene.json");
    let cavern_bytes = fs::read(&cavern_file).unwrap();
    fs::write(&cavern_file, b"{ damaged destination").unwrap();
    let rejected = rejected_travel(&mut game, "level preparation rejected");
    assert_retained(&game, rejected);
    fs::write(cavern_file, cavern_bytes).unwrap();

    game.command(action(TOGGLE_RETIREMENT_BLOCK));
    let rejected = rejected_travel(&mut game, "level commit rejected");
    assert_retained(&game, rejected);
    game.command(action(TOGGLE_RETIREMENT_BLOCK));

    let cavern = Some(LevelKey::main(LevelId::Cavern));
    let entered = game.command(action(TRAVEL));
    assert_eq!(entered.level, cavern);
    assert_eq!(entered.travel_count, 1);
    assert_eq!(room_phases(&entered), (Closed, Closed));
    assert_ne!(entered.current_instance, incumbent.current_instance);
    assert!(entered.markers.is_empty());
    for receipt in [&side, &lookout] {
        assert_retired(&game, receipt);
    }
    assert!(resident(&game, image_product()).is_none());
    assert!(resident(&game, meadow).is_none());
    assert!(resident_material(&game).is_none());
    assert!(resident_tileset(&game).is_none());
    assert_eq!(retained_image.pixels(), RELIC_PIXEL.repeat(4));
    game.session
        .observe(|world| {
            assert!(!retained_image.is_current(
                world.resource::<AssetServer>(),
                world.resource::<AssetStates>(),
                world.resource::<Assets<ImageAsset>>()
            ));
            assert!(
                world
                    .resource::<Assets<TileSet>>()
                    .get(tileset_publication.0)
                    .is_none()
            );
        })
        .unwrap();
    assert_eq!(
        game.command(action("two-level-game.save")).saves,
        room_saves + 1
    );

    let returned = game.command(action(TRAVEL));
    assert_eq!(returned.level, incumbent.level);
    assert_eq!(returned.travel_count, 2);
    assert_eq!(room_phases(&returned), (Closed, Closed));
    assert_eq!(returned.markers, incumbent.markers);
    let (new_side, new_side_relic) =
        open_room(&mut game, SIDE_ROOM, RELIC, |state| state.side_room);
    let (new_lookout, new_lookout_relic) =
        open_room(&mut game, LOOKOUT, LOOKOUT_RELIC, |state| state.lookout);
    assert_ne!(new_side.instance_id(), side.instance_id());
    assert_ne!(new_lookout.instance_id(), lookout.instance_id());
    assert_eq!(sprite_facts(&game, &new_side, &new_side_relic).2, 1.0);
    assert_eq!(sprite_facts(&game, &new_lookout, &new_lookout_relic).2, 1.0);
    let reopened_material = resident_material(&game).unwrap();
    assert_eq!(reopened_material.0, material.0);
    assert!(reopened_material.1 > material.1);
    let reopened_tileset = resident_tileset(&game).unwrap();
    assert_eq!(reopened_tileset.0, tileset_publication.0);
    assert_ne!(reopened_tileset.1, tileset_publication.1);
    assert!(reopened_tileset.2 > tileset_publication.2);
    assert_eq!(
        room_tileset(&game, &new_side, region_tileset::SIDE_ROOM_FLOOR),
        room_tileset(&game, &new_lookout, region_tileset::LOOKOUT_FLOOR)
    );
    assert_eq!(retained_image.pixels(), RELIC_PIXEL.repeat(4));
    assert_eq!(
        room_material(&game, &new_side, &new_side_relic),
        room_material(&game, &new_lookout, &new_lookout_relic)
    );

    let restored = game.command(action("two-level-game.restore"));
    assert_eq!(restored.level, cavern);
    assert_eq!(restored.travel_count, 3);
    assert_eq!(restored.restores, 1);
    assert_eq!(restored.saves, room_saves + 1);
    assert_eq!(room_phases(&restored), (Closed, Closed));
    for receipt in [&new_side, &new_lookout] {
        assert_retired(&game, receipt);
    }
    assert!(resident(&game, image_product()).is_none());
    assert!(resident_material(&game).is_none());
    assert!(resident_tileset(&game).is_none());
    assert_eq!(retained_image.pixels(), RELIC_PIXEL.repeat(4));
    game.close();
}

const TOGGLE_RETIREMENT_BLOCK: &str = "two-level-game.test-toggle-retirement-block";
const REMOVE_ROOM_IMAGE_RECORD: &str = "two-level-game.test-remove-room-image-record";
const RESTORE_ROOM_IMAGE_RECORD: &str = "two-level-game.test-restore-room-image-record";
const RETIRE_SIDE_ROOM: &str = "two-level-game.test-retire-side-room";
const UNLOAD_WITHOUT_ASSET_EVENTS: &str = "two-level-game.test-unload-without-asset-events";

#[derive(Default, Resource)]
struct RetirementProbe {
    removed_image: Option<AssetRecord>,
    retired: Option<SpawnedSceneInstance>,
}

impl Plugin for RetirementProbe {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test-room-retirement"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[PluginId::new("two-level-game.exploration")]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<RetirementProbe>()?;
        app.add_systems(
            CoreStage::FixedUpdate,
            (toggle_retirement_block, edit_room_residency)
                .chain()
                .in_set(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}

fn edit_room_residency(world: &mut World) {
    let commands = world
        .resource::<GameplayCommandBatch>()
        .commands()
        .iter()
        .map(|command| command.command_type().as_str().to_owned())
        .collect::<Vec<_>>();
    for command in commands {
        match command.as_str() {
            REMOVE_ROOM_IMAGE_RECORD => {
                assert!(world.resource::<RetirementProbe>().removed_image.is_none());
                let record = world
                    .resource::<ProjectAssetDatabase>()
                    .record_for_stable_id(image_product().source())
                    .unwrap()
                    .clone();
                assert_eq!(record.source_kind(), &AssetSourceKind::Image);
                assert!(
                    world
                        .resource_mut::<ProjectAssetDatabase>()
                        .remove_exact(&record)
                );
                world.resource_mut::<RetirementProbe>().removed_image = Some(record);
            }
            RESTORE_ROOM_IMAGE_RECORD => {
                let record = world
                    .resource_mut::<RetirementProbe>()
                    .removed_image
                    .take()
                    .unwrap();
                world
                    .resource_mut::<ProjectAssetDatabase>()
                    .insert(record)
                    .unwrap();
            }
            RETIRE_SIDE_ROOM => {
                let receipt = original_side_room(world);
                assert_eq!(
                    retire_and_despawn_scene_instance(world, &receipt).unwrap(),
                    receipt.len()
                );
                world.resource_mut::<RetirementProbe>().retired = Some(receipt);
            }
            UNLOAD_WITHOUT_ASSET_EVENTS => {
                let receipt = original_side_room(world);
                let identities = world.resource::<WorldIdentityDomain>().stats();
                let catalog = world.resource::<ProjectAssetDatabase>().clone();
                world.resource_scope(|world, events: nara::ecs::Mut<AssetEvents>| {
                    assert!(!world.contains_resource::<AssetEvents>());
                    let before = events.len();
                    let report = unload_project_scene_addition(world, &receipt).unwrap_err();
                    assert!(report.iter().any(|entry| {
                        entry.code().as_str() == "project.scene.residency-unavailable"
                    }));
                    assert_eq!(events.len(), before);
                    for id in receipt.entity_ids() {
                        assert!(matches!(
                            receipt.resolve(world, id),
                            EntityLookup::Resolved(_)
                        ));
                    }
                    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identities);
                    assert_eq!(world.resource::<ProjectAssetDatabase>(), &catalog);
                });
            }
            _ => {}
        }
    }
}

fn original_side_room(world: &World) -> SpawnedSceneInstance {
    let instance = world
        .iter_entities()
        .filter_map(|entity| entity.get::<SceneEntitySource>())
        .find(|source| source.entity_id.as_str() == RELIC)
        .unwrap()
        .instance_id;
    world
        .resource::<WorldIdentityDomain>()
        .active_scene_instance(world, instance)
        .unwrap()
}

#[derive(Component)]
#[component(on_remove = retirement_hook)]
struct RetirementBlock;

fn retirement_hook(_: DeferredWorld<'_>, _: HookContext) {}

fn toggle_retirement_block(world: &mut World) {
    if !world
        .resource::<GameplayCommandBatch>()
        .commands()
        .iter()
        .any(|command| command.command_type().as_str() == TOGGLE_RETIREMENT_BLOCK)
    {
        return;
    }
    let lookout = world
        .iter_entities()
        .find(|entity| {
            entity
                .get::<SceneEntitySource>()
                .is_some_and(|source| source.entity_id.as_str() == LOOKOUT_RELIC)
        })
        .unwrap()
        .id();
    let mut lookout = world.entity_mut(lookout);
    if lookout.contains::<RetirementBlock>() {
        lookout.remove::<RetirementBlock>();
    } else {
        lookout.insert(RetirementBlock);
    }
}

fn room_phases(state: &GameSnapshot) -> (SideRoomPhase, SideRoomPhase) {
    (state.side_room, state.lookout)
}

fn open_room(
    game: &mut Game,
    command: &str,
    relic: &str,
    phase: fn(&GameSnapshot) -> SideRoomPhase,
) -> (SpawnedSceneInstance, SceneEntityId) {
    game.submit(action(command));
    assert_eq!(game.session.step().unwrap(), None);
    let opened = wait_for(game, |state| phase(state) != SideRoomPhase::Preparing);
    assert!(opened.error.is_none(), "{opened:?}");
    assert_eq!(phase(&opened), SideRoomPhase::Open);
    room_receipt(game, relic)
}

fn collect_relic(
    game: &mut Game,
    direction: i64,
    phase: fn(&GameSnapshot) -> SideRoomPhase,
) -> GameSnapshot {
    game.submit(movement(direction, true));
    for _ in 0..MAX_WALK_STEPS {
        if phase(&game.step()) == SideRoomPhase::RelicFound {
            return game.command(movement(direction, false));
        }
    }
    panic!(
        "the explorer did not reach the relic: {:?}",
        game.snapshot()
    );
}

fn rejected_travel(game: &mut Game, expected: &str) -> GameSnapshot {
    game.submit(action(TRAVEL));
    assert_eq!(game.session.step().unwrap(), None);
    let rejected = wait_for(game, |state| state.phase == GamePhase::Running);
    assert!(
        rejected
            .error
            .as_deref()
            .is_some_and(|error| error.contains(expected)),
        "{rejected:?}"
    );
    rejected
}

fn sprite_facts(
    game: &Game,
    receipt: &SpawnedSceneInstance,
    id: &SceneEntityId,
) -> (Vec2, Option<Handle<ImageAsset>>, f32) {
    game.session
        .observe(|world| {
            let EntityLookup::Resolved(entity) = receipt.resolve(world, id) else {
                panic!("{id} must be live in its own Scene instance")
            };
            let transform = world.get::<Transform2d>(entity).unwrap();
            let sprite = world.get::<Sprite>(entity).unwrap();
            (
                transform.translation,
                sprite.material.image,
                sprite.material.tint.a,
            )
        })
        .unwrap()
}

fn assert_retired(game: &Game, receipt: &SpawnedSceneInstance) {
    game.session
        .observe(|world| {
            for id in receipt.entity_ids() {
                assert!(matches!(
                    receipt.resolve(world, id),
                    EntityLookup::Tombstoned(_)
                ));
            }
        })
        .unwrap();
}

fn wait_until_prepared(game: &mut Game) -> GameSnapshot {
    wait_for(game, |state| state.side_room != SideRoomPhase::Preparing)
}

fn wait_for(game: &mut Game, finished: impl Fn(&GameSnapshot) -> bool) -> GameSnapshot {
    let deadline = Instant::now() + DEADLINE;
    loop {
        let snapshot = game.snapshot();
        if finished(&snapshot) {
            return snapshot;
        }
        assert!(Instant::now() < deadline, "{snapshot:?}");
        game.maintenance();
        thread::sleep(Duration::from_millis(1));
    }
}

fn resident(game: &Game, product: AssetProductRef) -> Option<(Handle<ImageAsset>, AssetVersion)> {
    game.session
        .observe(|world| {
            let (handle, _) = world
                .resource::<Assets<ImageAsset>>()
                .iter()
                .find(|(_, image)| image.product() == Some(product))?;
            Some((
                handle,
                world
                    .resource::<AssetStates>()
                    .state(handle.id())?
                    .usable_version()?,
            ))
        })
        .unwrap()
}

fn resident_tileset(game: &Game) -> Option<(Handle<TileSet>, AssetSlotRevision, AssetVersion)> {
    game.session
        .observe(|world| {
            let handle = world
                .resource::<AssetServer>()
                .get_product_handle::<TileSet>(region_tileset::tileset_product())?;
            let tilesets = world.resource::<Assets<TileSet>>();
            tilesets.get(handle)?;
            Some((
                handle,
                tilesets.slot_revision(handle),
                world
                    .resource::<AssetStates>()
                    .state(handle.id())?
                    .usable_version()?,
            ))
        })
        .unwrap()
}

fn room_tileset(game: &Game, receipt: &SpawnedSceneInstance, floor: &str) -> TileSet {
    let publication = resident_tileset(game).unwrap();
    let (origin, cells) = match floor {
        region_tileset::SIDE_ROOM_FLOOR => (
            region_tileset::SIDE_ROOM_ORIGIN,
            region_tileset::side_room_cells(),
        ),
        region_tileset::LOOKOUT_FLOOR => (
            region_tileset::LOOKOUT_ORIGIN,
            region_tileset::lookout_cells(),
        ),
        _ => panic!("the floor must belong to an authored region"),
    };
    game.session
        .observe(|world| {
            let EntityLookup::Resolved(entity) =
                receipt.resolve(world, &SceneEntityId::new(floor).unwrap())
            else {
                panic!("the authored region floor must remain live")
            };
            let transform = world.get::<Transform2d>(entity).unwrap();
            assert_eq!(transform.translation, origin);
            assert_eq!(transform.rotation, 0.0);
            assert_eq!(transform.scale, Vec2::ONE);
            assert!(world.get::<Sprite>(entity).is_none());
            let tilemap = world.get::<Tilemap>(entity).unwrap();
            assert_eq!(tilemap.tileset, Some(publication.0));
            assert_eq!(tilemap.tile_size, region_tileset::TILE_SIZE);
            assert_eq!(tilemap.layer.index, region_tileset::FLOOR_LAYER);
            assert_eq!(tilemap.sort_key, 0);
            assert_eq!(tilemap.y_sort_anchor, None);
            assert_eq!(tilemap.cell_count(), cells.len());
            for (coord, cell) in cells {
                assert_eq!(tilemap.get_cell(coord), Some(&cell));
            }
            let tileset = world
                .resource::<Assets<TileSet>>()
                .get(publication.0)
                .unwrap();
            let expected = region_tileset::descriptor();
            assert_eq!(tileset.atlas(), Some(expected.atlas()));
            assert_eq!(tileset.tile_size(), expected.atlas().tile_size);
            assert_eq!(tileset.material().sampler, expected.sampler());
            assert_eq!(tileset.material().alpha_mode, expected.alpha_mode());
            assert_eq!(tileset.material().tint, expected.tint());
            let image = tileset.image_content().unwrap();
            assert_eq!(image.product(), expected.image_product());
            assert_eq!(tileset.material().image, Some(image.handle()));
            assert_eq!(image.pixels(), RELIC_PIXEL.repeat(4));
            assert!(image.is_current(
                world.resource::<AssetServer>(),
                world.resource::<AssetStates>(),
                world.resource::<Assets<ImageAsset>>()
            ));
            tileset.clone()
        })
        .unwrap()
}

fn resident_material(game: &Game) -> Option<(Handle<Material2dAsset>, AssetVersion)> {
    game.session
        .observe(|world| {
            let (handle, _) = world
                .resource::<Assets<Material2dAsset>>()
                .iter()
                .find(|(_, material)| material.product() == region_material::material_product())?;
            Some((
                handle,
                world
                    .resource::<AssetStates>()
                    .state(handle.id())?
                    .usable_version()?,
            ))
        })
        .unwrap()
}

fn room_material(
    game: &Game,
    receipt: &SpawnedSceneInstance,
    relic: &SceneEntityId,
) -> Material2dBinding {
    let publication = resident_material(game).unwrap();
    game.session
        .observe(|world| {
            let EntityLookup::Resolved(entity) = receipt.resolve(world, relic) else {
                panic!("the authored relic must remain live")
            };
            let material = world.get::<Material2d>(entity).unwrap();
            assert_eq!(material.handle(), publication.0);
            assert_eq!(material.declaration_id(), region_material::MATERIAL_ID);
            assert_eq!(
                material.declaration_version(),
                region_material::MATERIAL_VERSION
            );
            assert_eq!(material.parameters(), &region_material::parameters());
            assert!(material.texture_overrides().is_empty());
            let asset = world
                .resource::<Assets<Material2dAsset>>()
                .get(material.handle())
                .unwrap();
            assert_eq!(asset.product(), region_material::material_product());
            assert_eq!(
                asset.generation().declaration(),
                &region_material::declaration()
            );
            assert_eq!(
                asset.default_parameters(),
                &region_material::default_parameters()
            );
            assert_eq!(
                asset.textures(),
                &BTreeMap::from([(
                    region_material::PALETTE_ID.to_owned(),
                    AssetRef::Product(region_material::palette_product()),
                )])
            );
            let server = world.resource::<AssetServer>();
            material
                .bind(asset, |reference| {
                    let AssetRef::Product(product) = reference else {
                        panic!("resident Material textures must select complete Products")
                    };
                    server.get_product_handle::<ImageAsset>(*product)
                })
                .unwrap()
        })
        .unwrap()
}

fn room_receipt(game: &Game, relic: &str) -> (SpawnedSceneInstance, SceneEntityId) {
    game.session
        .observe(|world| {
            let relic = SceneEntityId::new(relic).unwrap();
            let source = world
                .iter_entities()
                .filter_map(|entity| entity.get::<SceneEntitySource>())
                .find(|source| source.entity_id == relic)
                .expect("the authored room is active");
            let receipt = world
                .resource::<WorldIdentityDomain>()
                .active_scene_instance(world, source.instance_id)
                .unwrap();
            assert_eq!(receipt.len(), 3);
            (receipt, relic)
        })
        .unwrap()
}
