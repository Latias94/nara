use nara::ecs::schedule::IntoScheduleConfigs;
use nara::{
    app::{
        CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginError,
        PluginId, PluginProductCapability,
    },
    asset::{AssetServer, Handle},
    core::{Color, Vec2},
    ecs::{Commands, Component, Entity, Query, Res, ResMut, Resource},
    image::ImageAsset,
    render::Camera2d,
    scene::Parent,
    sprite::Sprite,
    transform::Transform2d,
    ui::{UiNode, UiPanel, UiRoot, UiStyle, UiVal},
};

use crate::{
    Enemy, Player, Projectile, ReferenceWaveCaptureSet, WaveOutcome, WaveSnapshot, WaveSpawn,
    input::install_desktop_input,
    resources::{DesktopInputGate, WaveState},
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

const PLAYER_TEXTURE_PATH: &str = "textures/player.png";
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
        {
            let world = app.world_mut()?;
            world.insert_resource(DesktopInputGate);
            world
                .get_resource_mut::<WaveState>()
                .ok_or_else(|| setup_error("wave state is unavailable"))?
                .wait_for_input();
        }
        let player_texture = {
            let world = app.world_mut()?;
            let Some(mut server) = world.get_resource_mut::<AssetServer>() else {
                return Err(setup_error("desktop asset server is unavailable"));
            };
            server
                .reserve::<ImageAsset>(PLAYER_TEXTURE_PATH)
                .map_err(|_| setup_error("desktop player texture identity was rejected"))?
        };
        app.insert_resource(ReferenceDesktopAssets { player_texture })?
            .insert_resource(ReferenceHudProjection::default())?;
        spawn_desktop_view(app)?;
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
    player_texture: Handle<ImageAsset>,
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

fn spawn_desktop_view(app: &mut nara::prelude::App) -> Result<(), PluginError> {
    let world = app.world_mut()?;
    world.spawn(Camera2d {
        viewport_height: 18.0,
        clear_color: Some(Color::rgba(0.035, 0.047, 0.055, 1.0)),
        ..Camera2d::default()
    });

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
                Sprite::from_texture(assets.player_texture, Vec2::splat(0.9))
                    .with_tint(Color::rgba(0.22, 0.78, 0.95, 1.0))
                    .with_sort_key(20),
            ))
        } else if let Some(enemy) = enemy {
            let active =
                spawn.is_some_and(|spawn| spawn.tick <= snapshot.tick) && enemy.hit_points > 0;
            Some((
                enemy.position,
                Sprite::from_texture(
                    assets.player_texture,
                    if active {
                        Vec2::splat(0.78)
                    } else {
                        Vec2::ZERO
                    },
                )
                .with_tint(Color::rgba(0.95, 0.27, 0.31, 1.0))
                .with_sort_key(10),
            ))
        } else {
            projectile.map(|projectile| {
                (
                    projectile.position,
                    Sprite::from_color(Vec2::splat(0.24), Color::rgba(1.0, 0.83, 0.28, 1.0))
                        .with_sort_key(30),
                )
            })
        };
        let Some((position, sprite)) = presentation else {
            continue;
        };
        commands.entity(entity).insert((
            Transform2d {
                translation: position,
                ..Transform2d::IDENTITY
            },
            sprite,
        ));
    }
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
