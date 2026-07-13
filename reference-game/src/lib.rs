//! Independent public-surface reference game.

mod components;
mod systems;

use std::{error::Error, fmt, time::Duration};

use nara::{
    app::{AppRunError, PluginCategory, PluginError, PluginId, PluginMetadata},
    ecs::schedule::IntoScheduleConfigs,
    prelude::{
        App, ComponentRegistry, CoreStage, FixedTime, FixedUpdateSet, MinimalPlugins,
        PersistentComponentProvider, Plugin, StartupStage, Vec2, World,
    },
    reflect::COMPONENT_REGISTRY_PLUGIN_REQUIREMENT,
};

pub use components::{Enemy, Player, Projectile, RuntimeOnlyTag, Weapon};

pub const REFERENCE_GAME_PLUGIN_ID: PluginId = PluginId::new("reference-game.gameplay");

#[derive(Debug, Default, Clone, Copy)]
pub struct ReferenceGamePlugin;

impl Plugin for ReferenceGamePlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(REFERENCE_GAME_PLUGIN_ID, PluginCategory::Runtime)
            .requires_plugins(COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
    }

    fn preflight(&self, app: &App) -> Result<(), PluginError> {
        let registry = app
            .world()
            .get_resource::<ComponentRegistry>()
            .ok_or_else(component_registry_unavailable)?;
        validate_component::<Player>(registry)?;
        validate_component::<Enemy>(registry)?;
        validate_component::<Weapon>(registry)?;
        validate_component::<Projectile>(registry)
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        {
            let world = app.world_mut()?;
            let mut registry = world
                .get_resource_mut::<ComponentRegistry>()
                .ok_or_else(component_registry_unavailable)?;
            register_component::<Player>(&mut registry)?;
            register_component::<Enemy>(&mut registry)?;
            register_component::<Weapon>(&mut registry)?;
            register_component::<Projectile>(&mut registry)?;
        }

        app.add_startup_systems(StartupStage::Scene, systems::seed_tracer)?
            .add_systems(
                CoreStage::FixedUpdate,
                (
                    systems::move_players,
                    systems::move_enemies,
                    systems::tick_weapons,
                    systems::move_projectiles,
                    systems::resolve_projectile_hits,
                )
                    .chain()
                    .in_set(FixedUpdateSet::Simulate),
            )?;
        Ok(())
    }
}

fn component_registry_unavailable() -> PluginError {
    PluginError::component_registration(
        REFERENCE_GAME_PLUGIN_ID,
        "component-schema-catalog",
        "component registry resource is unavailable",
    )
}

fn validate_component<T>(registry: &ComponentRegistry) -> Result<(), PluginError>
where
    T: PersistentComponentProvider,
{
    registry
        .validate_persistent_component::<T>()
        .map_err(|error| {
            let schema = T::persistent_component_schema();
            PluginError::component_registration(
                REFERENCE_GAME_PLUGIN_ID,
                schema.id().as_str(),
                error,
            )
        })
}

fn register_component<T>(registry: &mut ComponentRegistry) -> Result<(), PluginError>
where
    T: PersistentComponentProvider,
{
    registry
        .register_persistent_component::<T>()
        .map(|_| ())
        .map_err(|error| {
            let schema = T::persistent_component_schema();
            PluginError::component_registration(
                REFERENCE_GAME_PLUGIN_ID,
                schema.id().as_str(),
                error,
            )
        })
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracerSnapshot {
    pub tick: u64,
    pub player_position: Vec2,
    pub player_hit_points: i64,
    pub enemy_position: Vec2,
    pub enemy_hit_points: i64,
    pub weapon_remaining_ticks: u64,
    pub projectile_position: Vec2,
    pub projectile_ttl_ticks: u64,
}

impl TracerSnapshot {
    #[must_use]
    pub fn initial() -> Self {
        Self {
            tick: 0,
            player_position: Vec2::ZERO,
            player_hit_points: 20,
            enemy_position: Vec2::new(5.0, 0.0),
            enemy_hit_points: 10,
            weapon_remaining_ticks: 3,
            projectile_position: Vec2::ZERO,
            projectile_ttl_ticks: 4,
        }
    }

    #[must_use]
    pub fn after_three_ticks() -> Self {
        Self {
            tick: 3,
            player_position: Vec2::new(3.0, 0.0),
            player_hit_points: 20,
            enemy_position: Vec2::new(3.5, 0.0),
            enemy_hit_points: 7,
            weapon_remaining_ticks: 0,
            projectile_position: Vec2::new(6.0, 0.0),
            projectile_ttl_ticks: 1,
        }
    }

    pub fn capture(world: &World) -> Result<Self, ReferenceGameError> {
        let player = single_component::<Player>(world, "Player")?;
        let enemy = single_component::<Enemy>(world, "Enemy")?;
        let weapon = single_component::<Weapon>(world, "Weapon")?;
        let projectile = single_component::<Projectile>(world, "Projectile")?;
        single_component::<RuntimeOnlyTag>(world, "RuntimeOnlyTag")?;
        let tick = world
            .get_resource::<FixedTime>()
            .ok_or(ReferenceGameError::MissingResource("FixedTime"))?
            .tick();

        Ok(Self {
            tick,
            player_position: player.position,
            player_hit_points: player.hit_points,
            enemy_position: enemy.position,
            enemy_hit_points: enemy.hit_points,
            weapon_remaining_ticks: weapon.remaining_ticks,
            projectile_position: projectile.position,
            projectile_ttl_ticks: projectile.ttl_ticks,
        })
    }
}

#[derive(Debug)]
pub enum ReferenceGameError {
    Plugin(PluginError),
    Run(AppRunError),
    MissingResource(&'static str),
    MissingComponent(&'static str),
    DuplicateComponent(&'static str),
    UnexpectedFixedSteps { expected: u32, actual: u32 },
}

impl fmt::Display for ReferenceGameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plugin(error) => write!(formatter, "reference-game plugin setup failed: {error}"),
            Self::Run(error) => write!(formatter, "reference-game frame failed: {error}"),
            Self::MissingResource(resource) => {
                write!(
                    formatter,
                    "reference-game world is missing resource {resource}"
                )
            }
            Self::MissingComponent(component) => {
                write!(formatter, "reference-game world is missing {component}")
            }
            Self::DuplicateComponent(component) => {
                write!(
                    formatter,
                    "reference-game world has multiple {component} values"
                )
            }
            Self::UnexpectedFixedSteps { expected, actual } => write!(
                formatter,
                "reference-game frame ran {actual} fixed steps instead of {expected}"
            ),
        }
    }
}

impl Error for ReferenceGameError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Plugin(error) => Some(error),
            Self::Run(error) => Some(error),
            Self::MissingResource(_)
            | Self::MissingComponent(_)
            | Self::DuplicateComponent(_)
            | Self::UnexpectedFixedSteps { .. } => None,
        }
    }
}

impl From<PluginError> for ReferenceGameError {
    fn from(error: PluginError) -> Self {
        Self::Plugin(error)
    }
}

impl From<AppRunError> for ReferenceGameError {
    fn from(error: AppRunError) -> Self {
        Self::Run(error)
    }
}

pub fn run_headless_ticks(ticks: u32) -> Result<TracerSnapshot, ReferenceGameError> {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)?;
    app.add_plugin(ReferenceGamePlugin)?;

    let startup = app.run_once(Duration::ZERO)?;
    if startup.status.fixed_steps != 0 {
        return Err(ReferenceGameError::UnexpectedFixedSteps {
            expected: 0,
            actual: startup.status.fixed_steps,
        });
    }

    for _ in 0..ticks {
        let outcome = app.run_once(FixedTime::DEFAULT_TIMESTEP)?;
        if outcome.status.fixed_steps != 1 {
            return Err(ReferenceGameError::UnexpectedFixedSteps {
                expected: 1,
                actual: outcome.status.fixed_steps,
            });
        }
    }

    TracerSnapshot::capture(app.world())
}

fn single_component<'world, T>(
    world: &'world World,
    label: &'static str,
) -> Result<&'world T, ReferenceGameError>
where
    T: nara::prelude::Component,
{
    let mut matches = world.iter_entities().filter_map(|entity| entity.get::<T>());
    let value = matches
        .next()
        .ok_or(ReferenceGameError::MissingComponent(label))?;
    if matches.next().is_some() {
        return Err(ReferenceGameError::DuplicateComponent(label));
    }
    Ok(value)
}
