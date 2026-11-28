use std::{
    error::Error,
    time::{Duration, Instant},
};

use nara::{
    app::{FrameExecutionStart, FrameIntent, FrameIntentSet},
    asset::{
        NpakEntryKind, RuntimeAssetMode,
        image::SamplerDescriptor,
        source::{MemoryAssetSource, asset_id_path},
    },
    ecs::{Single, bevy_ecs::error::Result as SystemResult, schedule::IntoScheduleConfigs},
    input::ActionPhase,
    physics2d::{TILEMAP_ONE_WAY_GROUP, TilemapColliders2d},
    prelude::*,
    render_wgpu::desktop::DesktopPresentation,
};

const TILESET: &str = "0193a100-1111-7000-8000-000000000041";
const ATLAS: &str = "0193a100-1111-7000-8000-000000000042";

#[derive(Component)]
struct Jumper {
    controller: PlatformerCharacter,
    timers: PlatformerJumpTimers,
    speed: f32,
    grounded: bool,
}

#[derive(Default, Resource)]
struct Controls {
    horizontal: f32,
    jump: bool,
}

#[derive(Resource)]
struct DemoRun {
    deadline: Instant,
    jumped: bool,
    climbed_slope: bool,
    landed_at: Option<u64>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let verify = std::env::args().nth(1).as_deref() == Some("--verify");
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins::new("Nara Platformer — A/D or arrows, Space to jump")
            .with_window_size(960, 600),
    )?;
    app.set_asset_source(tile_assets()?)?;
    app.init_resource::<Controls>()?
        .add_systems(Startup, setup)?
        .add_systems(FrameIntent, controls.in_set(FrameIntentSet::Collect))?
        .add_systems(
            FixedUpdate,
            jump.in_set(FixedUpdateSet::Simulate)
                .after(Physics2dSet::Step),
        )?;
    if verify {
        app.insert_resource(DemoRun {
            deadline: Instant::now() + Duration::from_secs(12),
            jumped: false,
            climbed_slope: false,
            landed_at: None,
        })?
        .add_systems(
            FixedUpdate,
            demo_input.in_set(FixedUpdateSet::Simulate).before(jump),
        )?
        .add_systems(Last, verify_presentation)?;
    }
    let outcome = app.run()?;
    if verify && outcome != AppExit::Success {
        return Err("platformer presentation ended before verification".into());
    }
    Ok(())
}

fn tile_assets() -> Result<RuntimeAssetSource, Box<dyn Error>> {
    let atlas = ATLAS.parse()?;
    let tileset = TILESET.parse()?;
    let a = ImageAsset::new(
        ImageExtent::new(2, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![62, 83, 128, 255, 38, 201, 165, 255],
    )?;
    let t = TileSetAsset {
        image: atlas,
        layout: TileAtlasLayout::grid(Vec2::ONE, 2, 1),
        sampler: SamplerDescriptor::NEAREST_CLAMP,
        tiles: vec![
            TileDefinition {
                solid: true,
                ..Default::default()
            },
            TileDefinition {
                one_way: true,
                ..Default::default()
            },
        ],
    };
    let mut assets = MemoryAssetSource::new();
    assets.insert(&asset_id_path(atlas), a.encode_cooked()?)?;
    assets.insert(&asset_id_path(tileset), t.encode_cooked()?)?;
    Ok(RuntimeAssetSource::from_memory_with_kinds(
        RuntimeAssetMode::Cooked,
        assets,
        [
            (atlas, NpakEntryKind::Image),
            (tileset, NpakEntryKind::TileSet),
        ],
    ))
}

fn setup(world: &mut World) -> SystemResult {
    use AxisDirection::{Negative, Positive};
    let movement = ActionId::new("horizontal")?;
    let jump = ActionId::new("jump")?;
    world.insert_resource(ActionMaps::new(ActionMap::from_bindings([
        ActionBinding::key_axis(movement.clone(), KeyCode::KeyA, Negative),
        ActionBinding::key_axis(movement.clone(), KeyCode::ArrowLeft, Negative),
        ActionBinding::key_axis(movement.clone(), KeyCode::KeyD, Positive),
        ActionBinding::key_axis(movement, KeyCode::ArrowRight, Positive),
        ActionBinding::key(jump, KeyCode::Space),
        ActionBinding::key(ActionId::new("exit")?, KeyCode::Escape),
    ])?));
    let tileset = TILESET.parse()?;
    load_asset_sync::<TileSetAsset>(world, tileset)?;
    load_asset_sync::<ImageAsset>(world, ATLAS.parse()?)?;
    let mut cells = vec![0; 24 * 6];
    cells[..24].fill(1);
    cells[24..35].fill(1);
    cells[37..48].fill(1);
    cells[3 * 24 + 9..3 * 24 + 15].fill(2);
    cells[5 * 24 + 15..5 * 24 + 19].fill(2);
    world.spawn(TilemapSource {
        tileset,
        tile_size: Vec2::ONE,
        width: 24,
        height: 6,
        origin_x: -12,
        origin_y: -4,
        layer: 0,
        cells: cells.into(),
    });
    let mut slope = Transform2d::from_xy(-5.5, -1.7);
    slope.rotation = 0.34;
    world.spawn((
        slope,
        Collider2d::Box {
            half_extents: Vec2::new(1.65, 0.20),
        },
        Sprite::from_color(Vec2::new(3.3, 0.4), Color::rgb(0.24, 0.38, 0.58)),
    ));
    world.spawn((
        Camera2d {
            viewport_height: 12.0,
            clear_color: Some(Color::rgb(0.018, 0.024, 0.045)),
            ..Default::default()
        },
        Transform2d::default(),
    ));
    let controller = PlatformerCharacter::new(
        Collider2d::CapsuleY {
            half_height: 0.22,
            radius: 0.28,
        },
        PlatformerConfig {
            snap_to_ground: Some(0.24),
            one_way_layer: Some(TILEMAP_ONE_WAY_GROUP),
            ..Default::default()
        },
    )?;
    world.spawn((
        Jumper {
            controller,
            timers: jump_timers(),
            speed: 0.0,
            grounded: false,
        },
        Transform2d::from_xy(-8.0, -0.5),
        Sprite::from_color(Vec2::new(0.56, 1.0), Color::rgb(0.95, 0.67, 0.26)).with_layer(1),
    ));
    Ok(())
}

fn jump_timers() -> PlatformerJumpTimers {
    PlatformerJumpTimers::new(Duration::from_millis(110), Duration::from_millis(120))
}

fn controls(
    actions: Res<ActionState>,
    mut controls: ResMut<Controls>,
    mut exit: ResMut<AppExitRequests>,
) {
    controls.horizontal = actions.axis("horizontal").clamp(-1.0, 1.0);
    for event in actions
        .edges()
        .iter()
        .filter(|event| event.phase == ActionPhase::Started)
    {
        match event.action.as_str() {
            "jump" => controls.jump = true,
            "exit" => exit.request_exit(),
            _ => {}
        }
    }
}

fn jump(
    mut controls: ResMut<Controls>,
    physics: Res<PhysicsWorld2d>,
    time: Res<FixedTime>,
    demo: Option<ResMut<DemoRun>>,
    mut player: Single<(&mut Jumper, &mut Transform2d)>,
) -> SystemResult {
    let (player, pose) = &mut *player;
    if pose.translation.y < -6.0 || pose.translation.x.abs() > 12.0 {
        pose.translation = Vec2::new(-8.0, -0.5);
        player.speed = 0.0;
        player.grounded = false;
        player.timers = jump_timers();
    }
    let step = time.delta().as_secs_f32();
    let grounded = player.grounded;
    if player
        .timers
        .tick(time.delta(), grounded, std::mem::take(&mut controls.jump))
    {
        player.speed = 9.6;
    } else {
        player.speed = (player.speed - 19.0 * step).max(-18.0);
    }
    let motion = physics.move_platformer(
        &player.controller,
        PhysicsPose2d::new(pose.translation, 0.0),
        Vec2::new(controls.horizontal * 5.6, player.speed) * step,
        time.delta(),
        PhysicsQueryFilter2d::default(),
    )?;
    pose.translation += motion.translation;
    player.grounded = motion.grounded && player.speed <= 0.0;
    if player.grounded || motion.hit_ceiling {
        player.speed = 0.0;
    }
    if let Some(mut demo) = demo {
        demo.climbed_slope |=
            motion.grounded && motion.slope_angle.is_some_and(|angle| angle > 0.2);
    }
    Ok(())
}

fn demo_input(
    mut controls: ResMut<Controls>,
    mut demo: ResMut<DemoRun>,
    player: Single<(&Jumper, &Transform2d)>,
) {
    let (player, pose) = *player;
    let x = pose.translation.x;
    if !demo.jumped && demo.climbed_slope && x >= -4.9 && player.grounded {
        demo.jumped = true;
        controls.jump = true;
    }
    controls.horizontal = if (player.grounded || demo.jumped) && x < -0.75 {
        1.0
    } else {
        0.0
    };
}

fn verify_presentation(
    presentation: Res<DesktopPresentation>,
    time: Res<FrameExecutionStart>,
    mut demo: ResMut<DemoRun>,
    player: Single<(&Jumper, &Transform2d)>,
    maps: Query<(&Tilemap, &TilemapColliders2d)>,
    colliders: Query<&CompoundCollider2d>,
    mut exit: ResMut<AppExitRequests>,
) {
    let (player, pose) = *player;
    if Instant::now() >= demo.deadline {
        eprintln!(
            "Platformer replay timed out at {:?} (slope={}, jumped={}, grounded={})",
            pose.translation, demo.climbed_slope, demo.jumped, player.grounded
        );
        exit.request_exit();
        return;
    }
    if demo.jumped && player.grounded && pose.translation.y > 0.4 {
        demo.landed_at.get_or_insert(time.frame());
    }
    if let Some(frame) = presentation.latest()
        && demo
            .landed_at
            .is_some_and(|index| frame.frame_index >= index)
        && let Ok((tiles, map)) = maps.single()
    {
        let rectangles = map
            .entities()
            .iter()
            .filter_map(|&entity| colliders.get(entity).ok())
            .map(|collider| collider.boxes().len())
            .sum::<usize>();
        if rectangles > 0 && rectangles < tiles.cell_count() {
            println!(
                "NARA_PLATFORMER slope=true one_way_landing=true tiles={} rectangles={rectangles} presented={}x{}",
                tiles.cell_count(),
                frame.extent.width,
                frame.extent.height
            );
            exit.request_success();
        }
    }
}
