use nara::ecs::schedule::IntoScheduleConfigs;
use nara::{
    app::{
        CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginError,
        PluginId, PluginProductCapability,
    },
    asset::{AssetServer, Handle},
    core::{Color, Vec2},
    ecs::{Commands, Component, Entity, Query, Res, ResMut, Resource, World},
    image::ImageAsset,
    material::SamplerDescriptor,
    render::Camera2d,
    scene::Parent,
    sprite::{Sprite, TextureRegion},
    transform::Transform2d,
    ui::{UiNode, UiPanel, UiRoot, UiStyle, UiVal},
};

use crate::{
    Enemy, Player, Projectile, ReferenceWaveCaptureSet, WaveOutcome, WaveSnapshot, WaveSpawn,
    input::install_desktop_input,
};

pub const REFERENCE_DESKTOP_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.desktop-projection");

const REFERENCE_DESKTOP_REQUIREMENTS: &[PluginId] = &[
    crate::REFERENCE_WAVE_PLUGIN_ID,
    nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
    nara::input::INPUT_PLUGIN_ID,
    nara::image::IMAGE_PLUGIN_ID,
    nara::render::RENDER_PLUGIN_ID,
    nara::sprite::SPRITE_PLUGIN_ID,
    nara::ui::UI_PLUGIN_ID,
];
const REFERENCE_DESKTOP_PRODUCT_REQUIREMENTS: &[PluginProductCapability] = &[
    PluginProductCapability::new("runtime-2d"),
    PluginProductCapability::new("runtime-ui"),
    PluginProductCapability::new("desktop-winit"),
    PluginProductCapability::new("render-wgpu"),
];
const REFERENCE_DESKTOP_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_DESKTOP_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(REFERENCE_DESKTOP_REQUIREMENTS)
        .requires_product_capabilities(REFERENCE_DESKTOP_PRODUCT_REQUIREMENTS);

const REFERENCE_ATLAS_PATH: &str = "textures/tiny-dungeon.png";
const ATLAS_COLUMNS: u32 = 12;
const ATLAS_ROWS: u32 = 11;
const ATLAS_TILE_PIXELS: f32 = 16.0;
const PLAYER_ATLAS_TILE: u32 = 96;
const ENEMY_ATLAS_TILE: u32 = 110;
const FLOOR_ATLAS_TILES: [u32; 4] = [14, 40, 57, 59];
const ARENA_COLUMNS: usize = 15;
const ARENA_ROWS: usize = 8;
const ARENA_TILE_SIZE: f32 = 2.0;
const PLAYER_MAX_HIT_POINTS: i64 = 20;
const HUD_BAR_WIDTH: f32 = 240.0;

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
            server
                .reserve::<ImageAsset>(REFERENCE_ATLAS_PATH)
                .map_err(|_| setup_error("desktop atlas texture identity was rejected"))?
        };
        app.insert_resource(ReferenceDesktopAssets { atlas_texture })?
            .insert_resource(ReferenceHudProjection::default())?;
        spawn_desktop_view(app, atlas_texture)?;
        app.add_systems(
            CoreStage::FixedUpdate,
            (project_desktop_sprites, project_desktop_hud)
                .in_set(ReferenceWaveCaptureSet::Presentation),
        )?;
        Ok(())
    }
}

#[must_use]
pub fn desktop_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ReferenceDesktopPlugin>()
}

#[derive(Debug, Clone, Copy, Resource)]
struct ReferenceDesktopAssets {
    atlas_texture: Handle<ImageAsset>,
}

type DesktopProjectionEntity<'a> = (
    Entity,
    Option<&'a Player>,
    Option<&'a Enemy>,
    Option<&'a Projectile>,
    Option<&'a WaveSpawn>,
);

#[derive(Debug, Default, Clone, Copy, PartialEq, Resource)]
pub struct ReferenceHudProjection {
    pub health_current: i64,
    pub health_maximum: i64,
    pub health_width: f32,
    pub defeated_enemies: u64,
    pub planned_enemies: u64,
    pub progress_width: f32,
    pub outcome: WaveOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
enum ReferenceHudElement {
    HealthFill,
    ProgressFill,
    Terminal,
}

fn spawn_desktop_view(
    app: &mut nara::prelude::App,
    atlas_texture: Handle<ImageAsset>,
) -> Result<(), PluginError> {
    let world = app.world_mut()?;
    world.spawn(Camera2d {
        viewport_height: 18.0,
        clear_color: Some(Color::rgba(0.025, 0.035, 0.045, 1.0)),
        ..Camera2d::default()
    });
    spawn_arena(world, atlas_texture);

    let health_root = world
        .spawn((
            UiRoot::primary_window(),
            UiNode::new(UiStyle::absolute(24.0, 24.0, HUD_BAR_WIDTH, 16.0)).clipping_children(),
            UiPanel::from_color(Color::rgba(0.12, 0.14, 0.15, 0.94)),
        ))
        .id();
    world.spawn((
        Parent(health_root),
        UiNode::new(UiStyle::absolute(0.0, 0.0, HUD_BAR_WIDTH, 16.0)).with_z_index(1),
        UiPanel::from_color(Color::rgba(0.18, 0.78, 0.42, 1.0)),
        ReferenceHudElement::HealthFill,
    ));

    let progress_root = world
        .spawn((
            UiRoot::primary_window(),
            UiNode::new(UiStyle::absolute(24.0, 48.0, HUD_BAR_WIDTH, 8.0)).clipping_children(),
            UiPanel::from_color(Color::rgba(0.12, 0.14, 0.15, 0.94)),
        ))
        .id();
    world.spawn((
        Parent(progress_root),
        UiNode::new(UiStyle::absolute(0.0, 0.0, 0.0, 8.0)).with_z_index(1),
        UiPanel::from_color(Color::rgba(0.95, 0.69, 0.22, 1.0)),
        ReferenceHudElement::ProgressFill,
    ));

    world.spawn((
        UiRoot::primary_window().with_order(10),
        UiNode::new(UiStyle::absolute(0.0, 280.0, 360.0, 120.0).with_left(UiVal::percent(0.36)))
            .with_z_index(20)
            .with_visible(false),
        UiPanel::from_color(Color::TRANSPARENT),
        ReferenceHudElement::Terminal,
    ));
    Ok(())
}

fn spawn_arena(world: &mut World, atlas_texture: Handle<ImageAsset>) {
    let center_column = (ARENA_COLUMNS.saturating_sub(1)) as f32 * 0.5;
    let center_row = (ARENA_ROWS.saturating_sub(1)) as f32 * 0.5;
    for row in 0..ARENA_ROWS {
        for column in 0..ARENA_COLUMNS {
            let tile = FLOOR_ATLAS_TILES[(row + column) % FLOOR_ATLAS_TILES.len()];
            let position = Vec2::new(
                (column as f32 - center_column) * ARENA_TILE_SIZE,
                (row as f32 - center_row) * ARENA_TILE_SIZE,
            );
            world.spawn((
                Transform2d::from_translation(position),
                atlas_sprite(
                    atlas_texture,
                    tile,
                    Vec2::splat(ARENA_TILE_SIZE),
                    Color::rgba(0.48, 0.58, 0.66, 1.0),
                )
                .with_layer(-20),
            ));
        }
    }

    let arena_width = ARENA_COLUMNS as f32 * ARENA_TILE_SIZE;
    let arena_height = ARENA_ROWS as f32 * ARENA_TILE_SIZE;
    let border_color = Color::rgba(0.16, 0.58, 0.6, 1.0);
    for position in [
        Vec2::new(0.0, -arena_height * 0.5),
        Vec2::new(0.0, arena_height * 0.5),
    ] {
        world.spawn((
            Transform2d::from_translation(position),
            Sprite::from_color(Vec2::new(arena_width, 0.3), border_color).with_layer(-10),
        ));
    }
    for position in [
        Vec2::new(-arena_width * 0.5, 0.0),
        Vec2::new(arena_width * 0.5, 0.0),
    ] {
        world.spawn((
            Transform2d::from_translation(position),
            Sprite::from_color(Vec2::new(0.3, arena_height), border_color).with_layer(-10),
        ));
    }
}

fn project_desktop_sprites(
    mut commands: Commands,
    assets: Res<ReferenceDesktopAssets>,
    snapshot: Res<WaveSnapshot>,
    entities: Query<DesktopProjectionEntity<'_>>,
) {
    for (entity, player, enemy, projectile, spawn) in &entities {
        let presentation = if let Some(player) = player {
            Some((
                player.position,
                0.0,
                atlas_sprite(
                    assets.atlas_texture,
                    PLAYER_ATLAS_TILE,
                    Vec2::splat(1.8),
                    Color::rgba(0.58, 0.84, 1.0, 1.0),
                )
                .with_sort_key(20),
            ))
        } else if let Some(enemy) = enemy {
            let active =
                spawn.is_some_and(|spawn| spawn.tick <= snapshot.tick) && enemy.hit_points > 0;
            Some((
                enemy.position,
                0.0,
                atlas_sprite(
                    assets.atlas_texture,
                    ENEMY_ATLAS_TILE,
                    if active {
                        Vec2::splat(1.65)
                    } else {
                        Vec2::ZERO
                    },
                    Color::rgba(1.0, 0.74, 0.74, 1.0),
                )
                .with_sort_key(10),
            ))
        } else {
            projectile.map(|projectile| {
                (
                    projectile.position,
                    projectile.velocity.y.atan2(projectile.velocity.x),
                    Sprite::from_color(Vec2::new(0.62, 0.24), Color::rgba(1.0, 0.86, 0.2, 1.0))
                        .with_sort_key(30),
                )
            })
        };
        let Some((position, rotation, sprite)) = presentation else {
            continue;
        };
        commands.entity(entity).insert((
            Transform2d {
                translation: position,
                rotation,
                ..Transform2d::IDENTITY
            },
            sprite,
        ));
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

fn project_desktop_hud(
    snapshot: Res<WaveSnapshot>,
    mut projection: ResMut<ReferenceHudProjection>,
    mut elements: Query<(&ReferenceHudElement, &mut UiNode, &mut UiPanel)>,
) {
    let health_current = snapshot.player.hit_points.clamp(0, PLAYER_MAX_HIT_POINTS);
    let health_ratio = health_current as f32 / PLAYER_MAX_HIT_POINTS as f32;
    let progress_ratio = if snapshot.planned_enemies == 0 {
        0.0
    } else {
        (snapshot.defeated_enemies as f32 / snapshot.planned_enemies as f32).clamp(0.0, 1.0)
    };
    *projection = ReferenceHudProjection {
        health_current,
        health_maximum: PLAYER_MAX_HIT_POINTS,
        health_width: HUD_BAR_WIDTH * health_ratio,
        defeated_enemies: snapshot.defeated_enemies,
        planned_enemies: snapshot.planned_enemies,
        progress_width: HUD_BAR_WIDTH * progress_ratio,
        outcome: snapshot.outcome,
    };

    for (element, mut node, mut panel) in &mut elements {
        match element {
            ReferenceHudElement::HealthFill => {
                node.style.width = UiVal::px(projection.health_width);
                panel.material.tint = if health_current == 0 {
                    Color::rgba(0.72, 0.15, 0.18, 1.0)
                } else {
                    Color::rgba(0.18, 0.78, 0.42, 1.0)
                };
            }
            ReferenceHudElement::ProgressFill => {
                node.style.width = UiVal::px(projection.progress_width);
            }
            ReferenceHudElement::Terminal => match snapshot.outcome {
                WaveOutcome::Running => {
                    node.visible = false;
                    panel.material.tint = Color::TRANSPARENT;
                }
                WaveOutcome::Completed => {
                    node.visible = true;
                    node.style.width = UiVal::px(360.0);
                    node.style.height = UiVal::px(120.0);
                    panel.material.tint = Color::rgba(0.12, 0.68, 0.36, 0.92);
                }
                WaveOutcome::Defeated => {
                    node.visible = true;
                    node.style.width = UiVal::px(220.0);
                    node.style.height = UiVal::px(160.0);
                    panel.material.tint = Color::rgba(0.78, 0.16, 0.2, 0.92);
                }
            },
        }
    }
}

fn setup_error(message: &'static str) -> PluginError {
    PluginError::SetupFailed {
        plugin: REFERENCE_DESKTOP_PLUGIN_ID,
        message: message.to_owned(),
    }
}
