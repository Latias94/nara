use std::error::Error;

use nara::ecs::{With, schedule::IntoScheduleConfigs};
use nara::hierarchy::{
    HierarchyCommandsExt, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner,
};
use nara::{advanced_prelude::*, backend_prelude::*};

#[path = "support/procedural_image.rs"]
mod procedural_image;

#[path = "support/runtime_retirement.rs"]
mod runtime_retirement;
use runtime_retirement::finish_runtime_after_winit;

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins((
        Runtime2dPlugins,
        Presentation2dPlugins,
        WindowPlugin {
            primary_window: Some(Window::new(
                "nara windowed sprites",
                WindowResolution::new(1280, 720),
            )),
        },
        WgpuBackendPlugins,
        WgpuSpriteSubmitterPlugin,
    ))?;
    app.insert_resource(nara::transform::FixedTransformInterpolation)?;
    app.add_systems(StartupStage::Scene, setup_scene)?;
    app.add_systems(
        CoreStage::FixedUpdate,
        rotate_demo_sprite.in_set(nara::app::FixedUpdateSet::Simulate),
    )?;
    app.world_mut()?.insert_resource(AssetServer::new());
    let mut runtime = nara::app::RuntimeInstance::start(app)?;
    let run_result = WinitRunner::default().run(&mut runtime);
    finish_runtime_after_winit(run_result, runtime)
}

#[derive(Component)]
struct DemoMotion;

fn rotate_demo_sprite(
    time: Res<nara::app::FixedTime>,
    mut sprites: Query<&mut Transform2d, With<DemoMotion>>,
) {
    for mut transform in &mut sprites {
        transform.rotation += time.delta().as_secs_f32() * 0.5;
    }
}

fn setup_scene(
    mut commands: Commands,
    mut asset_server: ResMut<AssetServer>,
    mut images: ResMut<Assets<ImageAsset>>,
    mut states: ResMut<AssetStates>,
    mut events: ResMut<AssetEvents>,
) {
    let texture = publish_demo_texture(&mut asset_server, &mut images, &mut states, &mut events);

    let camera = commands
        .spawn((
            Name::new("camera"),
            Transform2d::default(),
            Camera2d {
                clear_color: Some(Color::rgb(0.05, 0.06, 0.07)),
                viewport_height: 720.0,
                ..Camera2d::default()
            },
        ))
        .id();

    let texture_sprite = commands
        .spawn((
            Name::new("procedural texture"),
            Transform2d::from_translation(Vec2::new(-80.0, 20.0)),
            Sprite::from_texture(texture, Vec2::new(80.0, 80.0)).with_layer(1),
        ))
        .id();
    let color_sprite = commands
        .spawn((
            DemoMotion,
            Name::new("rose square"),
            Transform2d {
                translation: Vec2::new(40.0, -20.0),
                rotation: 0.35,
                scale: Vec2::splat(1.0),
            },
            Sprite::from_color(Vec2::new(96.0, 48.0), Color::rgb(0.95, 0.25, 0.35)).with_layer(2),
        ))
        .id();

    let mut tilemap = Tilemap::new(Vec2::new(32.0, 32.0)).with_layer(0);
    for x in -4..=4 {
        tilemap.set_cell(
            TileCoord::new(x, -3),
            TileCell::new(TileIndex::new(x.unsigned_abs()))
                .with_color(Color::rgb(0.18, 0.36, 0.22)),
        );
    }
    let floor = commands
        .spawn((
            Name::new("floor"),
            Transform2d::from_translation(Vec2::new(0.0, 0.0)),
            tilemap,
        ))
        .id();
    commands.apply_hierarchy_batch(
        [camera, texture_sprite, color_sprite, floor]
            .into_iter()
            .map(|entity| HierarchyOperation::Place {
                entity,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            })
            .collect(),
    );
}

fn publish_demo_texture(
    asset_server: &mut AssetServer,
    images: &mut Assets<ImageAsset>,
    states: &mut AssetStates,
    events: &mut AssetEvents,
) -> Handle<ImageAsset> {
    procedural_image::publish_rgba(
        asset_server,
        images,
        states,
        events,
        ImageExtent::new(2, 2),
        ImageColorSpace::Srgb,
        vec![
            255, 255, 255, 255, 40, 180, 255, 255, 255, 80, 120, 255, 30, 40, 60, 255,
        ],
    )
}
