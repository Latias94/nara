use nara::ecs::{With, schedule::IntoScheduleConfigs};
use nara::hierarchy::{
    HierarchyCommandsExt, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner,
};
use nara::material::{Material2dDefinition, Material2dDefinitions, Material2dInput};
use nara::{advanced_prelude::*, backend_prelude::*};
use std::error::Error;

#[path = "support/procedural_image.rs"]
mod procedural_image;
#[path = "support/runtime_retirement.rs"]
mod runtime_retirement;

static MATERIALS: [Material2dDefinition; 1] = [Material2dDefinition {
    id: "example.palette",
    fragment_shader: include_str!("support/palette_material.wgsl"),
    input: Material2dInput::SPRITE_FRAGMENT_V1,
    alpha_mode: AlphaMode2d::Blend,
}];

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins((
        Runtime2dPlugins,
        Presentation2dPlugins,
        WindowPlugin {
            primary_window: Some(Window::new(
                "Nara palette material",
                WindowResolution::new(800, 480),
            )),
        },
        WgpuBackendPlugins,
        WgpuSpriteSubmitterPlugin,
    ))?;
    app.insert_resource(Material2dDefinitions::new(&MATERIALS)?)?;
    app.insert_resource(nara::transform::FixedTransformInterpolation)?;
    app.add_systems(StartupStage::Scene, setup_scene)?;
    app.add_systems(
        CoreStage::FixedUpdate,
        rotate_palette_sprites.in_set(nara::app::FixedUpdateSet::Simulate),
    )?;
    let mut runtime = nara::app::RuntimeInstance::start(app)?;
    let result = WinitRunner::default().run(&mut runtime);
    runtime_retirement::finish_runtime_after_winit(result, runtime)
}

#[derive(Component)]
struct PaletteMotion;

fn rotate_palette_sprites(
    time: Res<nara::app::FixedTime>,
    mut sprites: Query<&mut Transform2d, With<PaletteMotion>>,
) {
    for mut transform in &mut sprites {
        transform.rotation += time.delta().as_secs_f32() * 0.4;
    }
}

fn setup_scene(
    mut commands: Commands,
    mut asset_server: ResMut<AssetServer>,
    mut images: ResMut<Assets<ImageAsset>>,
    mut states: ResMut<AssetStates>,
    mut events: ResMut<AssetEvents>,
    materials: Res<Material2dDefinitions>,
) {
    let indices = procedural_image::publish_rgba(
        &mut asset_server,
        &mut images,
        &mut states,
        &mut events,
        ImageExtent::new(2, 1),
        ImageColorSpace::Linear,
        vec![0, 0, 0, 255, 255, 0, 0, 255],
    );
    let palette = procedural_image::publish_rgba(
        &mut asset_server,
        &mut images,
        &mut states,
        &mut events,
        ImageExtent::new(2, 1),
        ImageColorSpace::Srgb,
        vec![255, 70, 30, 255, 30, 200, 255, 255],
    );
    let camera = commands
        .spawn((
            Camera2d {
                viewport_height: 480.0,
                ..Default::default()
            },
            Transform2d::default(),
        ))
        .id();
    let mut entities = vec![camera];
    for (x, offset) in [(-140.0, 0.0), (140.0, 1.0)] {
        entities.push(
            commands
                .spawn((
                    PaletteMotion,
                    Sprite::from_texture(indices, Vec2::new(200.0, 200.0))
                        .with_sampler(SamplerDescriptor::NEAREST_CLAMP),
                    materials
                        .instantiate("example.palette", [offset, 1.0, 0.0, 0.0], palette)
                        .unwrap(),
                    Transform2d::from_translation(Vec2::new(x, 0.0)),
                ))
                .id(),
        );
    }
    entities.push(
        commands
            .spawn((
                Sprite::from_color(Vec2::new(160.0, 80.0), Color::rgba(0.2, 0.8, 0.2, 0.5))
                    .with_layer(1),
                Transform2d::from_translation(Vec2::new(0.0, -60.0)),
            ))
            .id(),
    );
    commands.apply_hierarchy_batch(
        entities
            .into_iter()
            .map(|entity| HierarchyOperation::Place {
                entity,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            })
            .collect(),
    );
}
