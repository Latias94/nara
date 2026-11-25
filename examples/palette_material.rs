use nara::ecs::{With, schedule::IntoScheduleConfigs};
use nara::hierarchy::{
    HierarchyCommandsExt, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner,
};
use nara::material::{
    Material2dDeclaration, Material2dLibrary, MaterialParameters, MaterialTextureDeclaration,
};
use nara::project_host::{DesktopRun, DesktopRunOptions};
use nara::{advanced_prelude::*, backend_prelude::*};
use std::{collections::BTreeMap, error::Error, sync::Arc};

#[path = "support/desktop_run.rs"]
mod desktop_run;
#[path = "support/procedural_image.rs"]
mod procedural_image;

const MATERIAL_ID: &str = "example.palette";

#[derive(MaterialParameters)]
struct PaletteParameters {
    #[material(id = "palette.offset")]
    x: f32,
    #[material(id = "palette.strength")]
    y: f32,
    #[material(id = "palette.reserved-z")]
    z: f32,
    #[material(id = "palette.reserved-w")]
    w: f32,
}

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
        WgpuStockSpritePlugin,
    ))?;
    let mut materials = Material2dLibrary::default();
    materials.declare(Material2dDeclaration::from_parameters::<PaletteParameters>(
        MATERIAL_ID,
        1,
        AlphaMode2d::Blend,
        vec![MaterialTextureDeclaration::new(
            "palette",
            "material_texture",
            1,
        )?],
    )?)?;
    let shader_path = nara::asset::AssetPath::new("examples/support/palette_material.wgsl")?;
    let read_shader = |path: &nara::asset::AssetPath| match path.as_str() {
        "examples/support/palette_material.wgsl" => Ok(Arc::<str>::from(include_str!(
            "support/palette_material.wgsl"
        ))),
        _ => Err(format!("unknown palette shader source {path}")),
    };
    let prepared =
        materials.prepare_update(MATERIAL_ID, std::slice::from_ref(&shader_path), read_shader)?;
    materials.publish(&prepared, read_shader)?;
    app.insert_resource(materials)?;
    app.add_systems(StartupStage::Scene, setup_scene)?;
    app.add_systems(
        CoreStage::FixedUpdate,
        rotate_palette_sprites.in_set(nara::app::FixedUpdateSet::Simulate),
    )?;
    let runtime = nara::app::RuntimeInstance::start(app)?;
    desktop_run::finish_desktop_run(DesktopRun::from_runtime(runtime, DesktopRunOptions::new()))?;
    Ok(())
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
    materials: Res<Material2dLibrary>,
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
    let generation = materials
        .current(MATERIAL_ID)
        .expect("the palette generation is published at startup");
    for (x, offset) in [(-140.0, 0.0), (140.0, 1.0)] {
        entities.push(
            commands
                .spawn((
                    PaletteMotion,
                    Sprite::from_texture(indices, Vec2::new(200.0, 200.0))
                        .with_sampler(SamplerDescriptor::NEAREST_CLAMP),
                    generation
                        .bind(
                            &PaletteParameters {
                                x: offset,
                                y: 1.0,
                                z: 0.0,
                                w: 0.0,
                            },
                            BTreeMap::from([("palette".to_owned(), palette)]),
                        )
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
