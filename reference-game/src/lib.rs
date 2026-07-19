//! Independent public-surface reference game.

mod components;
mod resources;
mod snapshot;
mod systems;

use std::num::NonZeroU32;

use nara::{
    app::{
        PluginCategory, PluginDeclaration, PluginDefinition, PluginError, PluginId,
        PluginPreflightContext, PluginSchemaProviderId,
    },
    ecs::schedule::IntoScheduleConfigs,
    fs::DirectoryCapability,
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandKey,
        GameplayCommandPayload, GameplayCommandPlugin, GameplayCommandSet,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTick,
        GameplayCommandTypeId, GameplayCommandValue,
    },
    image::ImageImportLimits,
    prelude::{
        App, ComponentRegistry, CoreStage, FixedUpdateSet, PersistentComponentProvider, Plugin,
        Resource, Vec2,
    },
    project_host::{
        HeadlessRun, HeadlessRunIntent, ProjectRuntimePlugins, ProjectSettingsCandidate,
        project_runtime_plugins,
    },
    reflect::{
        COMPONENT_REGISTRY_PLUGIN_REQUIREMENT, ComponentRegistryError,
        ComponentSchemaProviderDefinition,
    },
    tilemap::TilemapPlugin,
};

pub use components::{Enemy, Player, Projectile, RuntimeOnlyTag, WaveSpawn, Weapon};
pub use resources::{MovementCommandError, MovementDirection, ProjectileId};
pub use snapshot::{EnemySnapshot, PlayerSnapshot, ProjectileSnapshot, WaveOutcome, WaveSnapshot};

pub const REFERENCE_GAME_PLUGIN_ID: PluginId = PluginId::new("reference-game.gameplay");
pub const REFERENCE_WAVE_PLUGIN_ID: PluginId = PluginId::new("reference-game.wave");
pub const REFERENCE_FIRST_TICK_COMMAND_TYPE: &str = "reference-game.no-op-v1";
pub const REFERENCE_FIRST_TICK_COMMAND_SOURCE: &str = "u26-manual";
pub const REFERENCE_PROJECT_OUTCOME_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.project-outcome");
pub const REFERENCE_GAME_SCHEMA_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("reference-game.components");
pub const REFERENCE_GAME_SCHEMA_PROVIDER: ComponentSchemaProviderDefinition =
    ComponentSchemaProviderDefinition::new(
        REFERENCE_GAME_SCHEMA_PROVIDER_ID,
        nara::reflect::ComponentSchemaProviderBindingId::new("reference-game.components.native", 2),
        register_reference_game_components,
    );
pub const REFERENCE_GAME_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_GAME_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
        .provides_schema(&[REFERENCE_GAME_SCHEMA_PROVIDER_ID]);
const REFERENCE_WAVE_REQUIREMENTS: &[PluginId] = &[
    REFERENCE_GAME_PLUGIN_ID,
    nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
];
const REFERENCE_WAVE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_WAVE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(REFERENCE_WAVE_REQUIREMENTS);
const REFERENCE_PROJECT_OUTCOME_REQUIREMENTS: &[PluginId] = &[
    REFERENCE_GAME_PLUGIN_ID,
    nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
];
const REFERENCE_PROJECT_OUTCOME_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_PROJECT_OUTCOME_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(REFERENCE_PROJECT_OUTCOME_REQUIREMENTS);

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

        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ReferenceWavePlugin;

impl Plugin for ReferenceWavePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &REFERENCE_WAVE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(resources::WaveState::default())?
            .insert_resource(WaveSnapshot::default())?
            .add_systems(
                CoreStage::FixedUpdate,
                systems::begin_wave_tick
                    .in_set(FixedUpdateSet::Simulate)
                    .before(GameplayCommandSet::Consume),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                systems::consume_movement_commands.in_set(GameplayCommandSet::Consume),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                (
                    systems::assign_scene_projectile_ids,
                    systems::validate_wave_topology,
                    systems::move_scene_players,
                    systems::pursue_scene_players,
                    systems::fire_automatic_weapons,
                    systems::move_wave_projectiles,
                    systems::resolve_enemy_contacts,
                    systems::resolve_wave_projectile_hits,
                    systems::retire_expired_entities,
                    systems::evaluate_wave_outcome,
                )
                    .chain()
                    .after(GameplayCommandSet::Consume)
                    .in_set(FixedUpdateSet::Simulate),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                snapshot::capture_wave_snapshot.in_set(GameplayCommandSet::Capture),
            )?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct ReferenceProjectOutcomePlugin;

impl Plugin for ReferenceProjectOutcomePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &REFERENCE_PROJECT_OUTCOME_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ReferenceProjectSnapshot::default())?
            .add_systems(
                CoreStage::FixedUpdate,
                (
                    systems::move_project_players,
                    systems::move_project_enemies,
                    systems::tick_project_weapons,
                )
                    .chain()
                    .in_set(FixedUpdateSet::Simulate),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                systems::observe_project_commands.in_set(GameplayCommandSet::Consume),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                systems::capture_project_snapshot.in_set(GameplayCommandSet::Capture),
            )?;
        Ok(())
    }
}

#[must_use]
pub fn plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ReferenceGamePlugin>()
}

#[must_use]
pub fn wave_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ReferenceWavePlugin>()
}

fn project_outcome_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ReferenceProjectOutcomePlugin>()
}

/// Adds the reference game after the semantic gameplay-command ingress in a project plan.
#[must_use]
pub fn runtime_plugins(candidate: &ProjectSettingsCandidate) -> ProjectRuntimePlugins {
    project_runtime_plugins(candidate)
        .insert_after::<GameplayCommandPlugin>(plugin())
        .insert_after::<ReferenceGamePlugin>(project_outcome_plugin())
}

/// Creates the reference game's project-backed product action.
#[must_use]
pub fn project_headless_run(
    project_root: DirectoryCapability,
    fixed_ticks: NonZeroU32,
) -> HeadlessRun<ReferenceProjectSnapshot> {
    HeadlessRun::new(
        project_root,
        project_headless_intent(fixed_ticks),
        vec![project_first_tick_command()],
    )
}

/// Runs the bundled deterministic wave until a terminal outcome or the fixed-tick limit.
#[must_use]
pub fn bundled_wave_run(
    project_root: DirectoryCapability,
    maximum_fixed_ticks: NonZeroU32,
) -> HeadlessRun<WaveSnapshot> {
    wave_headless_run(project_root, maximum_fixed_ticks, bundled_wave_commands())
}

/// Runs one owned typed semantic-command buffer until the wave terminates or reaches its tick limit.
#[must_use]
pub fn wave_headless_run(
    project_root: DirectoryCapability,
    maximum_fixed_ticks: NonZeroU32,
    commands: Vec<GameplayCommandSubmission>,
) -> HeadlessRun<WaveSnapshot> {
    HeadlessRun::new(
        project_root,
        wave_headless_intent(maximum_fixed_ticks),
        commands,
    )
}

/// Creates the complete wave run intent over the committed project content.
#[must_use]
pub fn wave_headless_intent(maximum_fixed_ticks: NonZeroU32) -> HeadlessRunIntent<WaveSnapshot> {
    HeadlessRunIntent::new(maximum_fixed_ticks)
        .stop_when(WaveSnapshot::is_terminal)
        .configure(nara::image::plugin(ImageImportLimits::default()))
        .disable::<TilemapPlugin>()
        .insert_after::<GameplayCommandPlugin>(plugin())
        .insert_after::<ReferenceGamePlugin>(wave_plugin())
        .with_schema_provider(REFERENCE_GAME_SCHEMA_PROVIDER)
}

/// Returns the semantic movement task bundled into the first headless playable.
#[must_use]
pub fn bundled_wave_commands() -> Vec<GameplayCommandSubmission> {
    vec![
        movement_command(1, 1, MovementDirection::Left)
            .expect("the bundled movement command has non-zero ordering values"),
    ]
}

/// Creates one trusted typed movement command for tests, replay adapters, or AI drivers.
pub fn movement_command(
    tick: u64,
    sequence: u64,
    direction: MovementDirection,
) -> Result<GameplayCommandSubmission, MovementCommandError> {
    let tick = GameplayCommandTick::new(tick).ok_or(MovementCommandError::ZeroTick)?;
    let sequence =
        GameplayCommandSourceSequence::new(sequence).ok_or(MovementCommandError::ZeroSequence)?;
    let (x, y) = direction.velocity();
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert(resources::MOVE_X_FIELD, GameplayCommandValue::I64(x))
        .expect("the engine-owned movement payload is bounded");
    payload
        .insert(resources::MOVE_Y_FIELD, GameplayCommandValue::I64(y))
        .expect("the engine-owned movement payload is bounded");
    Ok(GameplayCommandSubmission::new(
        tick,
        GameplayCommandIngressSource::test(resources::COMMAND_SOURCE)
            .expect("the movement command source is engine-owned and valid"),
        sequence,
        GameplayCommandDraft::new(
            GameplayCommandTypeId::new(resources::MOVE_COMMAND_TYPE)
                .expect("the movement command type is engine-owned and valid"),
        )
        .with_payload(payload),
    ))
}

/// Creates the reference game's project-backed run intent.
#[must_use]
pub fn project_headless_intent(
    fixed_ticks: NonZeroU32,
) -> HeadlessRunIntent<ReferenceProjectSnapshot> {
    HeadlessRunIntent::new(fixed_ticks)
        .configure(nara::image::plugin(ImageImportLimits::default()))
        .disable::<TilemapPlugin>()
        .insert_after::<GameplayCommandPlugin>(plugin())
        .insert_after::<ReferenceGamePlugin>(project_outcome_plugin())
        .with_schema_provider(REFERENCE_GAME_SCHEMA_PROVIDER)
}

/// Returns the first semantic input used by the committed reference task.
#[must_use]
pub fn project_first_tick_command() -> GameplayCommandSubmission {
    GameplayCommandSubmission::new(
        GameplayCommandTick::new(1).expect("the reference command tick is non-zero"),
        GameplayCommandIngressSource::test(REFERENCE_FIRST_TICK_COMMAND_SOURCE)
            .expect("the reference command source is engine-owned and valid"),
        GameplayCommandSourceSequence::new(1).expect("the reference command sequence is non-zero"),
        GameplayCommandDraft::new(
            GameplayCommandTypeId::new(REFERENCE_FIRST_TICK_COMMAND_TYPE)
                .expect("the reference command type is engine-owned and valid"),
        ),
    )
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
    register_component::<WaveSpawn>(registry)?;
    register_component::<Weapon>(registry)?;
    register_component::<Projectile>(registry)
}

fn validate_reference_game_components(
    registry: &ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    registry.validate_persistent_component::<Player>()?;
    registry.validate_persistent_component::<Enemy>()?;
    registry.validate_persistent_component::<WaveSpawn>()?;
    registry.validate_persistent_component::<Weapon>()?;
    registry.validate_persistent_component::<Projectile>()
}

fn register_component<T>(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError>
where
    T: PersistentComponentProvider,
{
    registry.register_persistent_component::<T>().map(|_| ())
}

/// Authoritative project-scene state captured after the latest fixed tick.
#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct ReferenceProjectSnapshot {
    pub tick: u64,
    pub player_position: Vec2,
    pub player_hit_points: i64,
    pub enemy_position: Vec2,
    pub enemy_hit_points: i64,
    pub weapon_remaining_ticks: u64,
    pub commands_seen: u64,
    pub first_command_key: Option<GameplayCommandKey>,
    pub first_command_type: Option<GameplayCommandTypeId>,
    pub runtime_only_entities: u64,
    pub unbound_gameplay_components: u64,
}
