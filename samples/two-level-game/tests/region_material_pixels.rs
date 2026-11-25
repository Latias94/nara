#![cfg(all(windows, feature = "desktop", feature = "development"))]
#![forbid(unsafe_code)]

#[allow(
    dead_code,
    reason = "reuse the ordinary Game and its physical close fixture"
)]
#[path = "support/resume_gameplay.rs"]
mod support;

#[path = "support/region_material_pixels/capture.rs"]
mod capture;
#[path = "support/region_material_pixels/gpu.rs"]
mod gpu;
#[path = "support/region_material_pixels/oracle.rs"]
mod oracle;

use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    thread,
    time::{Duration, Instant},
};

use nara::render_wgpu::WgpuRenderHost;
use oracle::RoomVisual::{Closed, Collected, Open};
use support::{DEADLINE, Game, ProjectCopy, TRAVEL, action, movement};
use two_level_game::{SideRoomPhase, levels::LevelId, product_recipe};

const SIDE_ROOM: &str = "two-level-game.side-room";
const LOOKOUT: &str = "two-level-game.lookout";

#[test]
#[ignore = "requires Windows hardware DX12; run serially with WGPU_BACKEND=dx12 and NARA_WGPU_FORCE_FALLBACK=0"]
fn original_region_material_packets_draw_and_survive_actual_room_and_runtime_retirement() {
    assert_eq!(std::env::var("WGPU_BACKEND").as_deref(), Ok("dx12"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let expected = oracle::Expected::new();
    let mut host = WgpuRenderHost::offscreen();
    let result = catch_unwind(AssertUnwindSafe(|| qualify(&mut host, &expected)));
    let retired = gpu::retire(&mut host);
    drop(host);
    if let Err(panic) = result {
        if let Err(error) = retired {
            eprintln!("original region Material Host cleanup also failed: {error}");
        }
        resume_unwind(panic);
    }
    retired.expect("the original public GPU Host must physically retire");
}

fn qualify(host: &mut WgpuRenderHost, expected: &oracle::Expected) {
    let project = ProjectCopy::new();
    let path = project.root().join("nara.toml");
    let manifest = fs::read_to_string(&path).unwrap();
    let selection = "requested = [\"runtime-2d\"]";
    assert_eq!(manifest.matches(selection).count(), 1);
    fs::write(
        &path,
        manifest.replace(
            selection,
            "requested = [\"runtime-2d\", \"presentation-2d\", \"render-wgpu\"]",
        ),
    )
    .unwrap();
    let recipe = product_recipe()
        .add_plugin::<capture::CapturePlugin>()
        .unwrap();
    let mut game = Game::from_project_with_recipe(project.root(), recipe);
    let primary = game.snapshot().current_instance;
    let live = host.create_producer().unwrap();
    let replay = host.create_producer().unwrap();
    let mut host_frame = 0;
    let mut retained = Vec::new();

    let startup = capture::sample(&mut game, 0, 0);
    assert!(startup.publication.is_none() && startup.binding.is_none());
    assert!(startup.floor.is_none());
    expected.check(
        &gpu::draw(host, &live, &startup.input, &mut host_frame),
        [Closed, Closed],
        LevelId::Meadow,
    );

    game.command(action(SIDE_ROOM));
    settle_rooms(&mut game, [SideRoomPhase::Open, SideRoomPhase::Closed]);
    let opened = capture::sample(&mut game, 1, 1);
    let publication = opened
        .publication
        .expect("one real resident Material publication");
    let binding = opened.binding.as_ref().unwrap().clone();
    let image = opened.image;
    let floor = opened
        .floor
        .as_ref()
        .expect("the original resident TileSet floor");
    let floor_publication = floor.publication;
    let floor_receipt = floor.receipt.clone();
    let tileset = floor.tileset.clone();
    let pixels = gpu::draw(host, &live, &opened.input, &mut host_frame);
    expected.check(&pixels, [Open, Closed], LevelId::Meadow);
    retained.push((opened, pixels, [Open, Closed]));

    game.command(action(LOOKOUT));
    settle_rooms(&mut game, [SideRoomPhase::Open, SideRoomPhase::Open]);
    let both = capture::sample(&mut game, 2, 2);
    assert_eq!(both.publication, Some(publication));
    assert_eq!(both.binding.as_ref(), Some(&binding));
    assert_eq!(both.image, image);
    assert_eq!(both.floor.as_ref().unwrap().publication, floor_publication);
    assert_eq!(both.floor.as_ref().unwrap().tileset, tileset);
    assert_eq!(
        both.floor.as_ref().unwrap().receipt.handle(),
        floor_receipt.handle()
    );
    let pixels = gpu::draw(host, &live, &both.input, &mut host_frame);
    expected.check(&pixels, [Open, Open], LevelId::Meadow);
    retained.push((both, pixels, [Open, Open]));

    game.command(movement(2, true));
    for _ in 0..24 {
        if game.snapshot().side_room == SideRoomPhase::RelicFound {
            break;
        }
        game.step();
    }
    let found = game.command(movement(2, false));
    assert_eq!(found.side_room, SideRoomPhase::RelicFound);
    assert_eq!(found.lookout, SideRoomPhase::Open);
    assert_eq!(found.current_instance, primary);
    game.command(movement(1, true));
    for _ in 0..10 {
        game.step();
    }
    let away = game.command(movement(1, false));
    assert!(
        away.player.x > -90.0,
        "the original D10 ticks must clear the relic and floor pixel oracles: {away:?}"
    );
    assert_eq!(away.side_room, SideRoomPhase::RelicFound);
    assert_eq!(away.lookout, SideRoomPhase::Open);
    let found = capture::sample(&mut game, 1, 2);
    assert_eq!(found.publication, Some(publication));
    assert_eq!(found.binding.as_ref(), Some(&binding));
    assert_eq!(found.image, image);
    assert_eq!(found.floor.as_ref().unwrap().publication, floor_publication);
    assert_eq!(found.floor.as_ref().unwrap().tileset, tileset);
    let pixels = gpu::draw(host, &live, &found.input, &mut host_frame);
    expected.check(&pixels, [Collected, Open], LevelId::Meadow);
    retained.push((found, pixels, [Collected, Open]));

    let closed = game.command(action(SIDE_ROOM));
    assert_eq!(closed.current_instance, primary);
    assert_eq!(closed.side_room, SideRoomPhase::Closed);
    assert_eq!(closed.lookout, SideRoomPhase::Open);
    let closed = capture::sample(&mut game, 1, 1);
    assert_eq!(closed.publication, Some(publication));
    assert_eq!(closed.binding.as_ref(), Some(&binding));
    assert_eq!(closed.image, image);
    assert_eq!(
        closed.floor.as_ref().unwrap().publication,
        floor_publication
    );
    assert_eq!(closed.floor.as_ref().unwrap().tileset, tileset);
    let pixels = gpu::draw(host, &live, &closed.input, &mut host_frame);
    expected.check(&pixels, [Closed, Open], LevelId::Meadow);
    retained.push((closed, pixels, [Closed, Open]));

    let traveled = game.command(action(TRAVEL));
    assert_eq!(traveled.level.unwrap().level(), LevelId::Cavern);
    assert_ne!(traveled.current_instance, primary);
    assert_eq!(traveled.side_room, SideRoomPhase::Closed);
    assert_eq!(traveled.lookout, SideRoomPhase::Closed);
    let traveled = capture::sample(&mut game, 0, 0);
    assert!(traveled.publication.is_none() && traveled.binding.is_none());
    assert!(traveled.image.is_none());
    assert!(traveled.floor.is_none());
    capture::assert_released(&game, publication.0, image.unwrap().0, &floor_receipt);
    expected.check(
        &gpu::draw(host, &live, &traveled.input, &mut host_frame),
        [Closed, Closed],
        LevelId::Cavern,
    );
    game.close();
    assert!(game.session.is_closed());

    assert_eq!(retained.len(), 4);
    for (frame, original, rooms) in &retained {
        assert_eq!(frame.binding.as_ref(), Some(&binding));
        assert_eq!(frame.publication, Some(publication));
        let floor = frame.floor.as_ref().unwrap();
        assert_eq!(floor.publication, floor_publication);
        assert_eq!(floor.receipt.handle(), floor_receipt.handle());
        assert_eq!(floor.tileset, tileset);
        assert_eq!(
            floor.tileset.image_content().unwrap().pixels(),
            two_level_game::region_material::SOURCE_PIXEL.repeat(4)
        );
        let pixels = gpu::draw(host, &replay, &frame.input, &mut host_frame);
        expected.check(&pixels, *rooms, LevelId::Meadow);
        assert_eq!(
            &pixels, original,
            "the immutable original room packet changed after retirement"
        );
    }
    for producer in [&live, &replay] {
        let prefix = host
            .retire_producer(producer)
            .expect("original submitted producer prefix");
        assert!(prefix.poll().unwrap());
    }
    println!(
        "region_material_pixels: shared_publication=true shared_tileset=true floor_cells_per_room=8 independent_close=true actual_travel_unload=true runtime_retired=true retained_original_replays={} expected_linear_rgba8={:?} expected_floor_linear_rgba8={:?}",
        retained.len(),
        expected.graded,
        expected.floors,
    );
}

fn settle_rooms(game: &mut Game, expected: [SideRoomPhase; 2]) {
    let deadline = Instant::now() + DEADLINE;
    loop {
        let snapshot = game.snapshot();
        assert!(snapshot.error.is_none(), "{snapshot:?}");
        if [snapshot.side_room, snapshot.lookout] == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "room preparation exceeded its deadline: {snapshot:?}"
        );
        game.maintenance();
        thread::sleep(Duration::from_millis(1));
    }
}
