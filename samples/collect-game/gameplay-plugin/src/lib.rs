//! The collection game's reusable Cargo plugin and authored configuration.

#[cfg(feature = "desktop")]
mod input;

use std::collections::HashSet;

use nara::{
    ProductRecipeError, SchemaContribution,
    advanced_prelude::add_startup_scene_activation_systems,
    app::{PluginCategory, PluginDeclaration, PluginError, PluginId, PluginSchemaProviderId},
    ecs::{Entity, Or, With, schedule::IntoScheduleConfigs, upstream::error::BevyError},
    gameplay::{
        GameplayCommandBatch, GameplayCommandDraft, GameplayCommandIngressSource,
        GameplayCommandPayload, GameplayCommandSet, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTick, GameplayCommandTypeId,
        GameplayCommandValue,
    },
    prelude::{
        App, Color, Component, CoreStage, FixedTime, FixedUpdateSet, PersistentComponent,
        PersistentComponentProvider, Plugin, Query, Res, ResMut, Resource, Transform2d, Vec2,
        add_fixed_transform_projection_systems,
    },
    reflect::{
        ComponentRegistry, ComponentRegistryError, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
        ComponentSchemaProviderSourceError,
    },
    render::Camera2d,
    sprite::Sprite,
    transform::FixedSpatialSnapshot2d,
};

const PLUGIN_ID: PluginId = PluginId::new("collect-game.gameplay");
const PROVIDER_ID: PluginSchemaProviderId = PluginSchemaProviderId::new("collect-game.components");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("collect-game"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("collect-game.native", 2),
    catalog,
    register,
)
.with_predecessor(predecessor_catalog);
const DECLARATION: PluginDeclaration = PluginDeclaration::new(PLUGIN_ID, PluginCategory::Runtime)
    .requires_plugins(&[
        nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID,
        nara::transform::TRANSFORM_PLUGIN_ID,
        nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
        nara::advanced_prelude::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
    ])
    .provides_schema(&[PROVIDER_ID]);
const MOVE: &str = "collect-game.move-v1";
const RESET: &str = "collect-game.reset-v1";
const VICTORY: Color = Color::rgb(0.06, 0.30, 0.16);

/// Authored movement speed in world units per second.
#[derive(Component, PersistentComponent, Debug, Clone)]
#[nara(
    id = "collect-game.PlayerConfig",
    version = 2,
    alias = "Player configuration",
    authoring_initial,
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct PlayerConfig {
    #[nara(id = "speed", alias = "Movement speed", initial = 240.0)]
    pub speed: f32,
}

/// Authored collection radius; collection progress is runtime-only.
#[derive(Component, PersistentComponent, Debug, Clone)]
#[nara(
    id = "collect-game.Collectible",
    version = 2,
    alias = "Collectible",
    authoring_initial,
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct Collectible {
    #[nara(id = "radius", alias = "Collection radius", initial = 20.0)]
    pub radius: f32,
}

/// Observable game outcome, shared by desktop and bounded headless execution.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct GameSnapshot {
    pub tick: u64,
    pub player: Vec2,
    pub collected: usize,
    pub target_count: usize,
    pub won: bool,
    pub resets: u32,
    pub visible_targets: usize,
    pub victory_color: bool,
}

#[derive(Clone)]
struct Baseline {
    entity: Entity,
    transform: Transform2d,
    tint: Option<Color>,
    clear_color: Option<Option<Color>>,
}

#[derive(Resource, Default)]
struct GameState {
    held: [bool; 4],
    collected: HashSet<Entity>,
    baseline: Vec<Baseline>,
    won: bool,
    resets: u32,
}

#[derive(Default)]
pub struct CollectGamePlugin;

impl Plugin for CollectGamePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(GameState::default())?
            .insert_resource(GameSnapshot::default())?;
        add_startup_scene_activation_systems(app, initialize)?;
        app.add_systems(
            CoreStage::FixedUpdate,
            consume_commands.in_set(GameplayCommandSet::Consume),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            move_player
                .after(GameplayCommandSet::Consume)
                .in_set(FixedUpdateSet::Simulate),
        )?;
        add_fixed_transform_projection_systems(
            app,
            collect_targets.before(GameplayCommandSet::Capture),
        )?;
        app.add_systems(
            CoreStage::FixedUpdate,
            capture.in_set(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}

/// Contributes both native gameplay and its persistent schema through one value.
pub fn contribution() -> Result<SchemaContribution<CollectGamePlugin>, ProductRecipeError> {
    SchemaContribution::for_default([PROVIDER])
}

fn catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    let predecessor = predecessor_catalog()?;
    let mut current = ComponentSchemaCatalog::successor_of(&predecessor)
        .expect("the fixed generation-one sample catalog has a successor");
    current.components = vec![
        PlayerConfig::persistent_component_schema(),
        Collectible::persistent_component_schema(),
    ];
    Ok(current)
}

// The generation-one scalar wire contract predates creation-only metadata.
fn predecessor_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    ComponentSchemaCatalog::from_json_bytes(include_bytes!(
        "../schema/component-schema-generation-1.json"
    ))
    .map_err(|_| {
        ComponentSchemaProviderSourceError::new("collect-game.predecessor-catalog-invalid")
    })
}

fn register(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    registry.register_persistent_component::<PlayerConfig>()?;
    registry.register_persistent_component::<Collectible>()?;
    Ok(())
}

fn initialize(
    mut state: ResMut<GameState>,
    objects: Query<
        (Entity, &Transform2d, Option<&Sprite>, Option<&Camera2d>),
        Or<(With<PlayerConfig>, With<Collectible>, With<Camera2d>)>,
    >,
) {
    state.baseline = objects
        .iter()
        .map(|(entity, transform, sprite, camera)| Baseline {
            entity,
            transform: *transform,
            tint: sprite.map(|sprite| sprite.material.tint),
            clear_color: camera.map(|camera| camera.clear_color),
        })
        .collect();
}

fn consume_commands(
    batch: Res<GameplayCommandBatch>,
    mut state: ResMut<GameState>,
    mut transforms: Query<&mut Transform2d>,
    mut sprites: Query<&mut Sprite>,
    mut cameras: Query<&mut Camera2d>,
) {
    for command in batch.commands() {
        match command.command_type().as_str() {
            MOVE => {
                if let (
                    Some(GameplayCommandValue::I64(direction)),
                    Some(GameplayCommandValue::Bool(pressed)),
                ) = (
                    command.payload().get("direction"),
                    command.payload().get("pressed"),
                ) && let Ok(direction) = usize::try_from(*direction)
                    && let Some(held) = state.held.get_mut(direction)
                {
                    *held = *pressed;
                }
            }
            RESET => {
                for baseline in &state.baseline {
                    if let Ok(mut transform) = transforms.get_mut(baseline.entity) {
                        *transform = baseline.transform;
                    }
                    if let (Some(tint), Ok(mut sprite)) =
                        (baseline.tint, sprites.get_mut(baseline.entity))
                    {
                        sprite.material.tint = tint;
                    }
                    if let (Some(color), Ok(mut camera)) =
                        (baseline.clear_color, cameras.get_mut(baseline.entity))
                    {
                        camera.clear_color = color;
                    }
                }
                state.held = [false; 4];
                state.collected.clear();
                state.won = false;
                state.resets += 1;
                println!("collect_game: reset");
            }
            _ => {}
        }
    }
}

fn move_player(
    time: Res<FixedTime>,
    state: Res<GameState>,
    mut players: Query<(&PlayerConfig, &mut Transform2d)>,
) {
    if state.won {
        return;
    }
    let [left, right, up, down] = state.held.map(|held| if held { 1.0 } else { 0.0 });
    let direction = Vec2::new(right - left, up - down).normalize_or_zero();
    for (config, mut transform) in &mut players {
        let next = (transform.translation + direction * config.speed * time.delta().as_secs_f32())
            .clamp(Vec2::new(-280.0, -160.0), Vec2::new(280.0, 160.0));
        if next != transform.translation {
            transform.translation = next;
        }
    }
}

fn collect_targets(
    mut state: ResMut<GameState>,
    spatial: FixedSpatialSnapshot2d<'_>,
    players: Query<Entity, With<PlayerConfig>>,
    mut targets: Query<(Entity, &Collectible, &mut Sprite)>,
    mut cameras: Query<&mut Camera2d>,
) -> Result<(), BevyError> {
    let Ok(player) = players.single() else {
        return Ok(());
    };
    let player = spatial
        .global(player)
        .map_err(BevyError::error)?
        .translation();
    let total = targets.iter().count();
    for (entity, config, mut sprite) in &mut targets {
        let position = spatial
            .global(entity)
            .map_err(BevyError::error)?
            .translation();
        if !state.collected.contains(&entity) && player.distance(position) <= config.radius {
            state.collected.insert(entity);
            sprite.material.tint.a = 0.0;
            println!("collect_game: collected={}/{total}", state.collected.len());
        }
    }
    if !state.won && total > 0 && state.collected.len() == total {
        state.won = true;
        for mut camera in &mut cameras {
            camera.clear_color = Some(VICTORY);
        }
        println!("collect_game: victory! Press R to reset.");
    }
    Ok(())
}

fn capture(
    time: Res<FixedTime>,
    state: Res<GameState>,
    mut snapshot: ResMut<GameSnapshot>,
    spatial: FixedSpatialSnapshot2d<'_>,
    players: Query<Entity, With<PlayerConfig>>,
    targets: Query<&Sprite, With<Collectible>>,
    cameras: Query<&Camera2d>,
) -> Result<(), BevyError> {
    let player = match players.single() {
        Ok(entity) => spatial
            .global(entity)
            .map_err(BevyError::error)?
            .translation(),
        Err(_) => Vec2::ZERO,
    };
    *snapshot = GameSnapshot {
        tick: time.tick(),
        player,
        collected: state.collected.len(),
        target_count: targets.iter().count(),
        won: state.won,
        resets: state.resets,
        visible_targets: targets
            .iter()
            .filter(|sprite| sprite.material.tint.a > 0.0)
            .count(),
        victory_color: cameras
            .iter()
            .any(|camera| camera.clear_color == Some(VICTORY)),
    };
    Ok(())
}

/// The four semantic directions used by both keys and headless commands.
#[derive(Clone, Copy)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[must_use]
pub fn movement(direction: Direction, pressed: bool) -> GameplayCommandDraft {
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert("direction", GameplayCommandValue::I64(direction as i64))
        .expect("the static direction payload is valid");
    payload
        .insert("pressed", GameplayCommandValue::Bool(pressed))
        .expect("the static button payload is valid");
    GameplayCommandDraft::new(GameplayCommandTypeId::new(MOVE).expect("the movement ID is valid"))
        .with_payload(payload)
}

#[must_use]
pub fn reset() -> GameplayCommandDraft {
    GameplayCommandDraft::new(GameplayCommandTypeId::new(RESET).expect("the reset ID is valid"))
}

/// A short reproducible walk through the authored level; optional reset proves restoration.
#[must_use]
pub fn demonstration_commands(with_reset: bool) -> Vec<GameplayCommandSubmission> {
    let mut drafts = vec![
        (1, movement(Direction::Right, true)),
        (61, movement(Direction::Right, false)),
    ];
    if with_reset {
        drafts.push((65, reset()));
    }
    drafts
        .into_iter()
        .enumerate()
        .map(|(sequence, (tick, draft))| {
            GameplayCommandSubmission::new(
                GameplayCommandTick::new(tick).expect("demonstration ticks are positive"),
                GameplayCommandIngressSource::external("collect-game.demo")
                    .expect("the source ID is valid"),
                GameplayCommandSourceSequence::new(sequence as u64 + 1)
                    .expect("sequences are positive"),
                draft,
            )
        })
        .collect()
}

#[cfg(feature = "desktop")]
pub use input::DesktopInputPlugin;
