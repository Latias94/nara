//! Independent public-surface reference game.

mod components;
mod systems;

use std::{error::Error, fmt, time::Duration};

use nara::{
    app::{
        AddPluginsError, AppRunError, PluginCategory, PluginDeclaration, PluginDefinition,
        PluginError, PluginId, PluginPreflightContext, PluginSchemaProviderId,
    },
    ecs::schedule::IntoScheduleConfigs,
    fs::FileCapability,
    gameplay::GameplayCommandPlugin,
    prelude::{
        App, ComponentRegistry, CoreStage, FixedTime, FixedUpdateSet, MinimalPlugins,
        PersistentComponentProvider, Plugin, RuntimeTimeSettings, StartupStage, Vec2, World,
    },
    project_host::{
        ProjectCandidateError, ProjectRuntimePlugins, ProjectSettingsCandidate,
        ingest_project_manifest, project_runtime_plugins,
    },
    reflect::{
        COMPONENT_REGISTRY_PLUGIN_REQUIREMENT, ComponentRegistryError,
        ComponentSchemaProviderDefinition,
    },
};

pub use components::{Enemy, Player, Projectile, RuntimeOnlyTag, Weapon};

pub const REFERENCE_GAME_PLUGIN_ID: PluginId = PluginId::new("reference-game.gameplay");
pub const REFERENCE_GAME_SCHEMA_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("reference-game.components");
pub const REFERENCE_GAME_SCHEMA_PROVIDER: ComponentSchemaProviderDefinition =
    ComponentSchemaProviderDefinition::new(
        REFERENCE_GAME_SCHEMA_PROVIDER_ID,
        nara::reflect::ComponentSchemaProviderBindingId::new("reference-game.components.native", 1),
        register_reference_game_components,
    );
pub const REFERENCE_GAME_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_GAME_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
        .provides_schema(&[REFERENCE_GAME_SCHEMA_PROVIDER_ID]);

#[derive(Debug, Default, Clone, Copy)]
pub struct ReferenceGamePlugin;

impl Plugin for ReferenceGamePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &REFERENCE_GAME_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        let registry = context
            .get_structural_resource::<ComponentRegistry>()
            .ok_or_else(component_registry_unavailable)?;
        validate_reference_game_components(registry).map_err(|error| {
            PluginError::component_registration(
                REFERENCE_GAME_PLUGIN_ID,
                REFERENCE_GAME_SCHEMA_PROVIDER_ID.as_str(),
                error,
            )
        })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        {
            let world = app.world_mut()?;
            let mut registry = world
                .get_resource_mut::<ComponentRegistry>()
                .ok_or_else(component_registry_unavailable)?;
            register_reference_game_components(&mut registry).map_err(|error| {
                PluginError::component_registration(
                    REFERENCE_GAME_PLUGIN_ID,
                    REFERENCE_GAME_SCHEMA_PROVIDER_ID.as_str(),
                    error,
                )
            })?;
        }

        app.add_systems(StartupStage::Scene, systems::seed_tracer)?
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

#[must_use]
pub fn plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ReferenceGamePlugin>()
}

/// Adds the reference game after the semantic gameplay-command ingress in a project plan.
#[must_use]
pub fn runtime_plugins(candidate: &ProjectSettingsCandidate) -> ProjectRuntimePlugins {
    project_runtime_plugins(candidate).insert_after::<GameplayCommandPlugin>(plugin())
}

fn component_registry_unavailable() -> PluginError {
    PluginError::component_registration(
        REFERENCE_GAME_PLUGIN_ID,
        "component-schema-catalog",
        "component registry resource is unavailable",
    )
}

pub fn register_reference_game_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    validate_reference_game_components(registry)?;
    register_component::<Player>(registry)?;
    register_component::<Enemy>(registry)?;
    register_component::<Weapon>(registry)?;
    register_component::<Projectile>(registry)
}

fn validate_reference_game_components(
    registry: &ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    registry.validate_persistent_component::<Player>()?;
    registry.validate_persistent_component::<Enemy>()?;
    registry.validate_persistent_component::<Weapon>()?;
    registry.validate_persistent_component::<Projectile>()
}

fn register_component<T>(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError>
where
    T: PersistentComponentProvider,
{
    registry.register_persistent_component::<T>().map(|_| ())
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracerSnapshot {
    pub tick: u64,
    pub fixed_timestep: Duration,
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
            fixed_timestep: FixedTime::DEFAULT_TIMESTEP,
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
            fixed_timestep: FixedTime::DEFAULT_TIMESTEP,
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
        let fixed_time = world
            .get_resource::<FixedTime>()
            .ok_or(ReferenceGameError::MissingResource("FixedTime"))?;

        Ok(Self {
            tick: fixed_time.tick(),
            fixed_timestep: fixed_time.timestep(),
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
    Composition(AddPluginsError),
    Plugin(PluginError),
    Run(AppRunError),
    Project(ProjectCandidateError),
    MissingResource(&'static str),
    MissingComponent(&'static str),
    DuplicateComponent(&'static str),
    UnexpectedFixedSteps { expected: u32, actual: u32 },
}

impl fmt::Display for ReferenceGameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Composition(error) => {
                write!(formatter, "reference-game composition failed: {error}")
            }
            Self::Plugin(error) => write!(formatter, "reference-game plugin setup failed: {error}"),
            Self::Run(error) => write!(formatter, "reference-game frame failed: {error}"),
            Self::Project(error) => write!(formatter, "reference-game project is invalid: {error}"),
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
            Self::Composition(error) => Some(error),
            Self::Plugin(error) => Some(error),
            Self::Run(error) => Some(error),
            Self::Project(error) => Some(error),
            Self::MissingResource(_)
            | Self::MissingComponent(_)
            | Self::DuplicateComponent(_)
            | Self::UnexpectedFixedSteps { .. } => None,
        }
    }
}

impl From<AddPluginsError> for ReferenceGameError {
    fn from(error: AddPluginsError) -> Self {
        Self::Composition(error)
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

impl From<ProjectCandidateError> for ReferenceGameError {
    fn from(error: ProjectCandidateError) -> Self {
        Self::Project(error)
    }
}

pub fn run_headless_ticks(ticks: u32) -> Result<TracerSnapshot, ReferenceGameError> {
    run_headless_ticks_with_time(ticks, None)
}

pub fn run_headless_ticks_from_manifest(
    manifest: &FileCapability,
    profile: Option<&str>,
    ticks: u32,
) -> Result<TracerSnapshot, ReferenceGameError> {
    let candidate = ingest_project_manifest(manifest, profile)?;
    let runtime = candidate.settings().runtime;
    run_headless_ticks_with_time(
        ticks,
        Some((runtime.runtime_time_settings(), runtime.fixed_time())),
    )
}

fn run_headless_ticks_with_time(
    ticks: u32,
    time: Option<(RuntimeTimeSettings, FixedTime)>,
) -> Result<TracerSnapshot, ReferenceGameError> {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, ReferenceGamePlugin))?;
    if let Some((runtime_time, fixed_time)) = time {
        app.insert_resource(runtime_time)?;
        app.insert_resource(fixed_time)?;
    }

    let startup = app.run_once(Duration::ZERO)?;
    if startup.status.fixed_steps != 0 {
        return Err(ReferenceGameError::UnexpectedFixedSteps {
            expected: 0,
            actual: startup.status.fixed_steps,
        });
    }

    let fixed_timestep = app.world().resource::<FixedTime>().timestep();
    for _ in 0..ticks {
        let outcome = app.run_once(fixed_timestep)?;
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
