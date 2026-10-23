mod commands;
mod content;
#[cfg(feature = "desktop")]
mod input;
mod storage;
mod travel;
mod verification;

use std::path::PathBuf;

use nara::{
    ProductConfiguration, ProductRecipe, SchemaContribution,
    advanced_prelude::{CurrentScene, add_startup_scene_activation_systems},
    app::{PluginCategory, PluginDeclaration, PluginId, PluginSchemaProviderId},
    ecs::{World, schedule::IntoScheduleConfigs, upstream::system::SystemState},
    gameplay::{GameplayCommandSet, add_local_gameplay_command_systems},
    identity::{EntityLookup, SceneEntityId, WorldEntityToken},
    prelude::{App, CoreStage, FixedUpdateSet, Plugin, PluginError, Resource, Transform2d, Vec2},
    reflect::{
        ComponentRegistry, ComponentRegistryError, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
        ComponentSchemaProviderSourceError, PersistentComponentProvider,
    },
    scene::advanced::SceneProductResource,
};

use crate::{
    levels::{LevelId, LevelKey},
    save::{Checkpoint, CheckpointLocation, MarkerId},
};
use content::Content;

pub(super) const PLUGIN_ID: PluginId = PluginId::new("two-level-game.exploration");
const PROVIDER_ID: PluginSchemaProviderId = PluginSchemaProviderId::new("two-level-game.objects");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("two-level-game"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("two-level-game.native", 1),
    catalog,
    register,
);
const DECLARATION: PluginDeclaration = PluginDeclaration::new(PLUGIN_ID, PluginCategory::Runtime)
    .requires_plugins(&[
        nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID,
        nara::transform::TRANSFORM_PLUGIN_ID,
        nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
        nara::advanced_prelude::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
    ])
    .provides_schema(&[PROVIDER_ID])
    .shutdown_obligations(&[storage::OBLIGATION]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum Scenario {
    #[default]
    Interactive,
    Travel,
    Save,
    Restore,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RunOptions {
    pub scenario: Scenario,
    pub save_path: Option<PathBuf>,
}
impl ProductConfiguration for RunOptions {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend(serde_json::to_vec(self).expect("finite game configuration"));
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GamePhase {
    #[default]
    Running,
    Preparing,
    Reconstructing,
    Degraded,
}

#[derive(Debug, Clone, Default, Resource)]
pub struct GameSnapshot {
    pub tick: u64,
    pub level: Option<LevelKey>,
    pub player: Vec2,
    pub score: u32,
    pub collected: usize,
    pub visible_pickups: usize,
    pub markers: Vec<MarkerId>,
    pub travel_count: u32,
    pub restarts: u32,
    pub saves: u32,
    pub restores: u32,
    pub phase: GamePhase,
    pub initialized: bool,
    pub verified: bool,
    pub error: Option<String>,
    pub current_instance: Option<nara::identity::SceneInstanceId>,
    pub scene_members: usize,
}

#[derive(Resource, Clone)]
pub(super) struct Progress(pub Checkpoint);
impl SceneProductResource for Progress {}

#[derive(Resource, Clone)]
struct ActiveLevel {
    pub key: LevelKey,
    pub content: Option<Content>,
}
impl SceneProductResource for ActiveLevel {}

#[derive(Resource, Default)]
pub(super) struct Session {
    held: [bool; 4],
    markers: Vec<WorldEntityToken>,
    phase: GamePhase,
    travel_count: u32,
    restarts: u32,
    saves: u32,
    restores: u32,
    initialized: bool,
    error: Option<String>,
}

struct ExplorationPlugin {
    options: RunOptions,
}
impl Plugin for ExplorationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?
            .register_component::<travel::DynamicMarker>();
        let key = LevelKey::main(LevelId::STARTUP);
        app.insert_resource(Progress(Checkpoint::new(CheckpointLocation {
            level: key,
            anchor: SceneEntityId::new("checkpoint").unwrap(),
        })))?
        .insert_resource(ActiveLevel { key, content: None })?
        .insert_resource(Session::default())?
        .insert_resource(GameSnapshot::default())?
        .insert_resource(travel::Travel::default())?
        .insert_resource(verification::Verification::new(self.options.scenario))?;
        storage::install(app, self.options.save_path.as_deref())?;
        add_startup_scene_activation_systems(app, initialize)?;
        add_local_gameplay_command_systems(app, verification::drive)?;
        app.add_systems(
            CoreStage::FixedUpdate,
            (travel::consume, travel::advance, simulate, capture)
                .chain()
                .in_set(GameplayCommandSet::Consume)
                .before(GameplayCommandSet::Capture)
                .in_set(FixedUpdateSet::Simulate),
        )?;
        app.add_systems(
            CoreStage::FixedUpdate,
            verification::observe_progress.after(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}

#[must_use]
pub fn product_recipe() -> ProductRecipe {
    configured_recipe(RunOptions::default())
}

#[must_use]
pub fn configured_recipe(options: RunOptions) -> ProductRecipe {
    let contribution = SchemaContribution::configured(
        options,
        |options| ExplorationPlugin {
            options: options.clone(),
        },
        [PROVIDER],
    )
    .expect("static game schema and composition");
    let recipe = ProductRecipe::new()
        .add_contribution(contribution)
        .expect("static game recipe");
    #[cfg(feature = "desktop")]
    let recipe = recipe
        .add_plugin::<input::DesktopInputPlugin>()
        .expect("static input plugin");
    recipe
}

fn catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![content::ObjectRole::persistent_component_schema()],
        ..Default::default()
    })
}
fn register(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    registry
        .register_persistent_component::<content::ObjectRole>()
        .map(|_| ())
}

fn initialize(world: &mut World) {
    let content = {
        let mut access = SystemState::<CurrentScene>::new(world);
        match access.get(world) {
            Ok(current) => Content::read(current.source()),
            Err(error) => Err(error.to_string()),
        }
    };
    match content {
        Ok(content) => {
            world.resource_mut::<Progress>().0.location.anchor = content.checkpoint.clone();
            world.resource_mut::<ActiveLevel>().content = Some(content);
            if let Err(error) = travel::reconstruct(world) {
                fail(world, error);
            } else {
                world.resource_mut::<Session>().initialized = true;
                println!(
                    "two_level_game: entered Meadow instance=0; WASD move, E travel, Q instance, M marker, K save, L restore, R retry"
                );
            }
        }
        Err(error) => fail(world, error),
    }
}

fn fail(world: &mut World, error: String) {
    eprintln!("two_level_game: reconstruction degraded: {error}");
    let mut session = world.resource_mut::<Session>();
    session.phase = GamePhase::Degraded;
    session.held = [false; 4];
    session.error = Some(error);
}

fn player(world: &mut World) -> Option<nara::ecs::Entity> {
    let id = world
        .resource::<ActiveLevel>()
        .content
        .as_ref()?
        .player
        .clone();
    let mut access = SystemState::<CurrentScene>::new(world);
    let current = access.get(world).ok()?;
    match current.receipt().resolve(world, &id) {
        EntityLookup::Resolved(entity) => Some(entity),
        _ => None,
    }
}

fn simulate(world: &mut World) {
    if world.resource::<Session>().phase != GamePhase::Running {
        return;
    }
    let Some(player) = player(world) else {
        fail(world, "current explorer identity is unavailable".into());
        return;
    };
    let held = world
        .resource::<Session>()
        .held
        .map(|held| if held { 1.0 } else { 0.0 });
    let direction = Vec2::new(held[1] - held[0], held[2] - held[3]).normalize_or_zero();
    let delta = world
        .resource::<nara::app::FixedTime>()
        .delta()
        .as_secs_f32();
    let Some(mut transform) = world.get_mut::<Transform2d>(player) else {
        fail(world, "current explorer has no transform".into());
        return;
    };
    transform.translation = (transform.translation + direction * 160.0 * delta)
        .clamp(Vec2::new(-280.0, -150.0), Vec2::new(280.0, 150.0));
    let position = transform.translation;
    let level = world.resource::<ActiveLevel>();
    let key = level.key;
    let nearby = level
        .content
        .as_ref()
        .unwrap()
        .pickups
        .iter()
        .filter(|(_, point)| point.distance(position) <= 16.0)
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    for entity in nearby {
        let pickup = crate::save::CollectedPickup { level: key, entity };
        let mut progress = world.resource_mut::<Progress>();
        if !progress.0.collected.contains(&pickup) {
            let id = pickup.entity.clone();
            progress.0.collected.push(pickup);
            progress.0.score = progress
                .0
                .score
                .saturating_add(10)
                .min(crate::save::MAX_SCORE);
            println!(
                "two_level_game: collected={id} level={key:?} score={}",
                progress.0.score
            );
        }
    }
    travel::hide_collected(world);
}

fn capture(world: &mut World) {
    let markers = match travel::observe_markers(world) {
        Ok(markers) => markers,
        Err(error) => {
            fail(world, error);
            Vec::new()
        }
    };
    let player = player(world)
        .and_then(|entity| {
            world
                .get::<Transform2d>(entity)
                .map(|value| value.translation)
        })
        .unwrap_or(Vec2::ZERO);
    let receipt = {
        let mut access = SystemState::<CurrentScene>::new(world);
        access
            .get(world)
            .ok()
            .map(|current| current.receipt().clone())
    };
    let progress = &world.resource::<Progress>().0;
    let level = world.resource::<ActiveLevel>();
    let session = world.resource::<Session>();
    let visible = level.content.as_ref().map_or(0, |content| {
        content
            .pickups
            .keys()
            .filter(|id| {
                receipt
                    .as_ref()
                    .is_some_and(|receipt| match receipt.resolve(world, id) {
                        EntityLookup::Resolved(entity) => world
                            .get::<nara::sprite::Sprite>(entity)
                            .is_some_and(|sprite| sprite.material.tint.a > 0.0),
                        _ => false,
                    })
            })
            .count()
    });
    let snapshot = GameSnapshot {
        tick: world.resource::<nara::app::FixedTime>().tick(),
        level: Some(level.key),
        player,
        score: progress.score,
        collected: progress.collected.len(),
        visible_pickups: visible,
        markers,
        travel_count: session.travel_count,
        restarts: session.restarts,
        saves: session.saves,
        restores: session.restores,
        phase: session.phase,
        initialized: session.initialized,
        verified: world.resource::<verification::Verification>().complete,
        error: session.error.clone().or_else(|| {
            world
                .resource::<verification::Verification>()
                .failure
                .clone()
        }),
        current_instance: receipt.as_ref().map(|receipt| receipt.instance_id()),
        scene_members: receipt
            .as_ref()
            .map_or(0, |receipt| receipt.entity_ids().len()),
    };
    *world.resource_mut::<GameSnapshot>() = snapshot;
}
