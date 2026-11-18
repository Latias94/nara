#![forbid(unsafe_code)]

#[allow(
    dead_code,
    reason = "reuse the production game and physical close fixture"
)]
#[path = "support/resume_gameplay.rs"]
mod support;

use std::{
    fs, thread,
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::CurrentScene,
    asset::{AssetProductRef, AssetStates, AssetVersion, Assets, Handle},
    identity::{EntityLookup, SceneEntityId, SpawnedSceneInstance, WorldIdentityDomain},
    image::ImageAsset,
    scene::SceneEntitySource,
    sprite::Sprite,
};
use support::{DEADLINE, Game, ProjectCopy, action, movement};
use two_level_game::{
    GamePhase, GameSnapshot, SideRoomPhase, capture_resume,
    levels::LevelId,
    side_room::{RELIC, image_product},
};

const SIDE_ROOM: &str = "two-level-game.side-room";
const RELIC_PIXEL: [u8; 4] = [64, 224, 180, 255];

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

    let (receipt, relic) = room_receipt(&game);
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
    let (fresh, _) = room_receipt(&game);
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

fn wait_until_prepared(game: &mut Game) -> GameSnapshot {
    let deadline = Instant::now() + DEADLINE;
    loop {
        let snapshot = game.snapshot();
        if snapshot.side_room != SideRoomPhase::Preparing {
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

fn room_receipt(game: &Game) -> (SpawnedSceneInstance, SceneEntityId) {
    game.session
        .observe(|world| {
            let relic = SceneEntityId::new(RELIC).unwrap();
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
