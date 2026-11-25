//! The collection game's reusable Cargo plugin and authored configuration.

#[cfg(feature = "desktop")]
mod input;
mod walking;

use std::collections::{BTreeMap, HashSet};

use nara::{
    ProductConfiguration, ProductRecipe, ProductRecipeError, SchemaContribution,
    advanced_prelude::add_startup_scene_activation_systems,
    app::{
        PluginCategory, PluginDeclaration, PluginError, PluginId, PluginSchemaProviderId,
        RuntimeFaultReporter, StartupStage,
    },
    ecs::{Commands, Entity, Or, With, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs},
    prelude::{
        App, Color, Component, CoreStage, FixedTime, FixedUpdateSet, PersistentComponent, Plugin,
        Query, Res, ResMut, Resource, Transform2d, Vec2,
    },
    reflect::{
        ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaProviderSourceError,
    },
    render::Camera2d,
    scene::TRANSFORM_COMPONENTS_PLUGIN_ID,
    scene::fixed_spatial::{
        FIXED_SPATIAL_PLUGIN_ID, FixedSpatialSnapshot2d, add_fixed_transform_input_systems,
        add_fixed_transform_projection_systems,
    },
    sprite::Sprite,
};
use walking::{WalkingState, capture_movement_parents, collect_targets, move_player};

const PLUGIN_ID: PluginId = PluginId::new("collect-game.gameplay");
const PROVIDER_ID: PluginSchemaProviderId = PluginSchemaProviderId::new("collect-game.components");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("collect-game"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("collect-game.native", 2),
    catalog,
    current_components::register,
)
.with_predecessor(predecessor_catalog);
const DECLARATION: PluginDeclaration = PluginDeclaration::new(PLUGIN_ID, PluginCategory::Runtime)
    .requires_plugins(&[
        nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID,
        TRANSFORM_COMPONENTS_PLUGIN_ID,
        FIXED_SPATIAL_PLUGIN_ID,
        nara::advanced_prelude::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
    ])
    .provides_schema(&[PROVIDER_ID]);
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

nara::persistent_components! { mod current_components = [PlayerConfig, Collectible]; }

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
    walking: WalkingState,
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
        let faults = app.world().resource::<RuntimeFaultReporter>().clone();
        app.insert_resource(GameState::default())?
            .insert_resource(GameSnapshot::default())?
            .init_resource::<CollectionInput>()?
            .init_resource::<CollectionScript>()?;
        add_startup_scene_activation_systems(app, initialize)?;
        add_fixed_transform_input_systems(app, capture_movement_parents)?;
        app.add_systems(
            CoreStage::FixedUpdate,
            (consume_controls, move_player)
                .chain()
                .in_set(FixedUpdateSet::Simulate),
        )?;
        add_fixed_transform_projection_systems(app, collect_targets.before(capture))?;
        app.add_systems(
            CoreStage::FixedUpdate,
            capture
                .in_set(FixedUpdateSet::Finalize)
                .run_if(move || faults.fault().is_none()),
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
    current.components = current_components::schemas().map_err(|_| {
        ComponentSchemaProviderSourceError::new("collect-game.current-catalog-invalid")
    })?;
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

fn consume_controls(
    time: Res<FixedTime>,
    mut input: ResMut<CollectionInput>,
    mut script: ResMut<CollectionScript>,
    mut state: ResMut<GameState>,
    mut transforms: Query<&mut Transform2d>,
    mut sprites: Query<&mut Sprite>,
    mut cameras: Query<&mut Camera2d>,
) {
    let scheduled = script.0.remove(&time.tick()).unwrap_or_default();
    for control in input.0.drain(..).chain(scheduled) {
        match control {
            CollectionControl::Move { direction, pressed } => {
                state.held[direction as usize] = pressed;
            }
            CollectionControl::Reset => {
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
        }
    }
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
#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug)]
pub enum CollectionControl {
    Move { direction: Direction, pressed: bool },
    Reset,
}

#[derive(Default, Resource)]
struct CollectionInput(Vec<CollectionControl>);

impl CollectionInput {
    fn push(&mut self, control: CollectionControl) -> Result<(), BevyError> {
        const MAX_PENDING_CONTROLS: usize = 4_096;
        if self.0.len() == MAX_PENDING_CONTROLS {
            return Err(BevyError::error("too many pending collection controls"));
        }
        self.0.push(control);
        Ok(())
    }
}

#[derive(Clone, Default, Resource)]
struct CollectionScript(BTreeMap<u64, Vec<CollectionControl>>);

impl ProductConfiguration for CollectionScript {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        for (tick, controls) in &self.0 {
            for control in controls {
                output.extend_from_slice(&tick.to_le_bytes());
                match *control {
                    CollectionControl::Move { direction, pressed } => {
                        output.extend_from_slice(&[0, direction as u8, u8::from(pressed)]);
                    }
                    CollectionControl::Reset => output.extend_from_slice(&[1, 0, 0]),
                }
            }
        }
    }
}

struct CollectionScriptPlugin(CollectionScript);

impl Plugin for CollectionScriptPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("collect-game.script"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let script = self.0.clone();
        app.add_systems(StartupStage::Runtime, move |mut commands: Commands| {
            commands.insert_resource(script.clone());
        })?;
        Ok(())
    }
}

pub fn with_collection_script(
    recipe: ProductRecipe,
    controls: impl IntoIterator<Item = (u64, CollectionControl)>,
) -> Result<ProductRecipe, ProductRecipeError> {
    let mut script = CollectionScript::default();
    for (tick, control) in controls {
        script.0.entry(tick).or_default().push(control);
    }
    if script.0.is_empty() {
        return Ok(recipe);
    }
    recipe.add_configured_plugin(script, |script| CollectionScriptPlugin(script.clone()))
}

#[must_use]
pub const fn movement(direction: Direction, pressed: bool) -> CollectionControl {
    CollectionControl::Move { direction, pressed }
}

#[must_use]
pub const fn reset() -> CollectionControl {
    CollectionControl::Reset
}

/// A short reproducible walk through the authored level; optional reset proves restoration.
#[must_use]
pub fn demonstration_commands(with_reset: bool) -> Vec<(u64, CollectionControl)> {
    let mut controls = vec![
        (1, movement(Direction::Right, true)),
        (61, movement(Direction::Right, false)),
    ];
    if with_reset {
        controls.push((65, reset()));
    }
    controls
}

#[cfg(feature = "desktop")]
pub use input::DesktopInputPlugin;

#[cfg(test)]
mod inventory_tests;
