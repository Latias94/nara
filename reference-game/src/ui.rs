use nara::ecs::schedule::IntoScheduleConfigs;
use nara::{
    app::{
        CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginProductCapability,
    },
    asset::{AssetProductRef, AssetServer, Handle, ImportedProductId, StableAssetId},
    core::{Color, Vec2},
    ecs::{Commands, Component, Entity, Query, Res, World},
    hierarchy::{HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter},
    image::ImageAsset,
    material::SamplerDescriptor,
    render::Camera2d,
    sprite::{Sprite, TextureRegion},
    transform::Transform2d,
};

use crate::{
    EnemyRole, Health, ProjectileRole, REFERENCE_RUNTIME_UI_PLUGIN_ID, ReferenceWaveCaptureSet,
    WaveSnapshot, WaveSpawn, input::install_desktop_input, resources::WaveRunOwner,
};

pub const REFERENCE_DESKTOP_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.desktop-projection");

const REFERENCE_DESKTOP_REQUIREMENTS: &[PluginId] = &[
    REFERENCE_RUNTIME_UI_PLUGIN_ID,
    nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
    nara::input::INPUT_PLUGIN_ID,
    nara::image::IMAGE_PLUGIN_ID,
    nara::render::RENDER_PLUGIN_ID,
    nara::sprite::SPRITE_PLUGIN_ID,
];
const REFERENCE_DESKTOP_PRODUCT_REQUIREMENTS: &[PluginProductCapability] = &[
    PluginProductCapability::new("runtime-2d"),
    PluginProductCapability::new("runtime-ui"),
    PluginProductCapability::new("presentation-2d"),
    PluginProductCapability::new("presentation-ui"),
    PluginProductCapability::new("desktop-winit"),
    PluginProductCapability::new("render-wgpu"),
];
const REFERENCE_DESKTOP_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_DESKTOP_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(REFERENCE_DESKTOP_REQUIREMENTS)
        .requires_product_capabilities(REFERENCE_DESKTOP_PRODUCT_REQUIREMENTS);

const REFERENCE_ATLAS_ID: &str = "f840a555-9fca-4ceb-ac1b-e03b55d2f492";
const ATLAS_COLUMNS: u32 = 12;
const ATLAS_ROWS: u32 = 11;
const ATLAS_TILE_PIXELS: f32 = 16.0;
const FLOOR_ATLAS_TILES: [u32; 4] = [14, 40, 57, 59];
const ARENA_COLUMNS: usize = 15;
const ARENA_ROWS: usize = 8;
const ARENA_TILE_SIZE: f32 = 2.0;

#[derive(Debug, Default, Clone, Copy)]
pub struct ReferenceDesktopPlugin;

impl Plugin for ReferenceDesktopPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &REFERENCE_DESKTOP_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut nara::prelude::App) -> Result<(), PluginError> {
        install_desktop_input(app)?;
        let atlas_texture = {
            let world = app.world_mut()?;
            let Some(mut server) = world.get_resource_mut::<AssetServer>() else {
                return Err(setup_error("desktop asset server is unavailable"));
            };
            let source = StableAssetId::parse_str(REFERENCE_ATLAS_ID)
                .map_err(|_| setup_error("desktop atlas source identity was rejected"))?;
            let product = AssetProductRef::new(source, ImportedProductId::PRIMARY)
                .map_err(|_| setup_error("desktop atlas product identity was rejected"))?;
            server
                .reserve_product::<ImageAsset>(product)
                .map_err(|_| setup_error("desktop atlas texture identity was rejected"))?
        };
        spawn_desktop_view(app, atlas_texture)?;
        app.add_systems(
            CoreStage::FixedUpdate,
            project_desktop_sprites.in_set(ReferenceWaveCaptureSet::Presentation),
        )?;
        Ok(())
    }
}

type DesktopProjectionEntity<'a> = (
    Entity,
    Option<&'a nara::scene::SceneEntitySource>,
    Option<&'a EnemyRole>,
    Option<&'a ProjectileRole>,
    Option<&'a WaveSpawn>,
    Option<&'a Health>,
    Option<&'a mut Sprite>,
    Option<&'a ReferenceAuthoredSpriteSize>,
);

#[derive(Debug, Clone, Copy, Component)]
struct ReferenceAuthoredSpriteSize(Vec2);

fn spawn_desktop_view(
    app: &mut nara::prelude::App,
    atlas_texture: Handle<ImageAsset>,
) -> Result<(), PluginError> {
    let world = app.world_mut()?;
    let camera = world
        .spawn((
            Transform2d::default(),
            Camera2d {
                viewport_height: 18.0,
                clear_color: Some(Color::rgba(0.025, 0.035, 0.045, 1.0)),
                ..Camera2d::default()
            },
        ))
        .id();
    let mut placements = vec![root_placement(camera)];
    spawn_arena(world, atlas_texture, &mut placements);
    HierarchyWriter::new(world)
        .apply_batch(&placements)
        .map_err(|_| setup_error("desktop view hierarchy placement failed"))?;
    Ok(())
}

fn spawn_arena(
    world: &mut World,
    atlas_texture: Handle<ImageAsset>,
    placements: &mut Vec<HierarchyOperation>,
) {
    let center_column = (ARENA_COLUMNS.saturating_sub(1)) as f32 * 0.5;
    let center_row = (ARENA_ROWS.saturating_sub(1)) as f32 * 0.5;
    for row in 0..ARENA_ROWS {
        for column in 0..ARENA_COLUMNS {
            let tile = FLOOR_ATLAS_TILES[(row + column) % FLOOR_ATLAS_TILES.len()];
            let position = Vec2::new(
                (column as f32 - center_column) * ARENA_TILE_SIZE,
                (row as f32 - center_row) * ARENA_TILE_SIZE,
            );
            let entity = world
                .spawn((
                    Transform2d::from_translation(position),
                    atlas_sprite(
                        atlas_texture,
                        tile,
                        Vec2::splat(ARENA_TILE_SIZE),
                        Color::rgba(0.48, 0.58, 0.66, 1.0),
                    )
                    .with_layer(-20),
                ))
                .id();
            placements.push(root_placement(entity));
        }
    }

    let arena_width = ARENA_COLUMNS as f32 * ARENA_TILE_SIZE;
    let arena_height = ARENA_ROWS as f32 * ARENA_TILE_SIZE;
    let border_color = Color::rgba(0.16, 0.58, 0.6, 1.0);
    for position in [
        Vec2::new(0.0, -arena_height * 0.5),
        Vec2::new(0.0, arena_height * 0.5),
    ] {
        let entity = world
            .spawn((
                Transform2d::from_translation(position),
                Sprite::from_color(Vec2::new(arena_width, 0.3), border_color).with_layer(-10),
            ))
            .id();
        placements.push(root_placement(entity));
    }
    for position in [
        Vec2::new(-arena_width * 0.5, 0.0),
        Vec2::new(arena_width * 0.5, 0.0),
    ] {
        let entity = world
            .spawn((
                Transform2d::from_translation(position),
                Sprite::from_color(Vec2::new(0.3, arena_height), border_color).with_layer(-10),
            ))
            .id();
        placements.push(root_placement(entity));
    }
}

fn root_placement(entity: Entity) -> HierarchyOperation {
    HierarchyOperation::Place {
        entity,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
    }
}

fn project_desktop_sprites(
    mut commands: Commands,
    owner: Res<WaveRunOwner>,
    snapshot: Res<WaveSnapshot>,
    mut entities: Query<DesktopProjectionEntity<'_>>,
) {
    for (entity, source, enemy, projectile, spawn, health, sprite, authored_size) in &mut entities {
        if enemy.is_some() {
            if source.is_none_or(|source| !owner.owns_scene_entity(entity, source)) {
                continue;
            }
            let Some(mut sprite) = sprite else {
                continue;
            };
            let authored_size = authored_size.map_or_else(
                || {
                    let authored_size = sprite.size;
                    commands
                        .entity(entity)
                        .insert(ReferenceAuthoredSpriteSize(authored_size));
                    authored_size
                },
                |authored_size| authored_size.0,
            );
            let active = spawn.is_some_and(|spawn| spawn.tick <= snapshot.tick)
                && health.is_some_and(|health| health.current > 0);
            sprite.size = if active { authored_size } else { Vec2::ZERO };
            continue;
        }
        if projectile.is_some() && owner.owns_projectile_entity(entity) {
            commands.entity(entity).insert(
                Sprite::from_color(Vec2::new(0.62, 0.24), Color::rgba(1.0, 0.86, 0.2, 1.0))
                    .with_sort_key(30),
            );
        }
    }
}

fn atlas_sprite(atlas_texture: Handle<ImageAsset>, tile: u32, size: Vec2, tint: Color) -> Sprite {
    Sprite::from_texture(atlas_texture, size)
        .with_texture_region(atlas_region(tile))
        .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
        .with_tint(tint)
}

fn atlas_region(tile: u32) -> TextureRegion {
    debug_assert!(tile < ATLAS_COLUMNS * ATLAS_ROWS);
    let column = tile % ATLAS_COLUMNS;
    let row = tile / ATLAS_COLUMNS;
    TextureRegion::from_pixels(
        Vec2::new(
            column as f32 * ATLAS_TILE_PIXELS,
            row as f32 * ATLAS_TILE_PIXELS,
        ),
        Vec2::splat(ATLAS_TILE_PIXELS),
        Vec2::new(
            ATLAS_COLUMNS as f32 * ATLAS_TILE_PIXELS,
            ATLAS_ROWS as f32 * ATLAS_TILE_PIXELS,
        ),
    )
    .expect("the bundled atlas grid is valid")
}

fn setup_error(message: &'static str) -> PluginError {
    PluginError::SetupFailed {
        plugin: REFERENCE_DESKTOP_PLUGIN_ID,
        message: message.to_owned(),
    }
}
