//! Independent public-surface reference game.

mod components;
#[cfg(feature = "desktop")]
mod input;
mod resources;
mod runtime_ui;
mod snapshot;
mod systems;
#[cfg(feature = "desktop")]
mod ui;
mod ui_observation;
mod wave_input;

use std::num::{NonZeroU32, NonZeroU64};

use nara::{
    ProductRecipe, ProductRecipeError, SchemaContribution,
    advanced_prelude::add_startup_scene_activation_systems,
    app::{
        PluginCategory, PluginDeclaration, PluginError, PluginId, PluginSchemaProviderId,
        RuntimeFaultReporter, StartupStage,
    },
    ecs::{SystemSet, schedule::IntoScheduleConfigs},
    fs::DirectoryCapability,
    prelude::{
        App, ComponentRegistryBuilder, CoreStage, FixedUpdateSet, PersistentComponentProvider,
        Plugin, Resource, Vec2,
    },
    project_host::{HeadlessRun, HeadlessRunIntent},
    reflect::{
        ComponentCatalogFileLimits, ComponentCodecError, ComponentFieldPath,
        ComponentRegistryError, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaProviderDefinition, ComponentSchemaProviderSourceError,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    },
    scene::{
        TRANSFORM_COMPONENTS_PLUGIN_ID,
        fixed_spatial::{
            FIXED_SPATIAL_PLUGIN_ID, FixedSpatialPlugin, add_fixed_transform_projection_systems,
        },
    },
};

#[cfg(feature = "desktop")]
use nara::project_host::DesktopRunIntent;

pub use components::{
    EnemyRole, Health, InitialHealth, InitialVelocity2d, PlayerRole, ProjectileDamage,
    ProjectileLifetime, ProjectileRole, RuntimeOnlyTag, Velocity2d, WaveSpawn, Weapon,
    WeaponCooldown,
};
pub use resources::{
    MovementCommandError, MovementDirection, ProjectileId, RetryCommandError, WaveRetryPhase,
    WaveRetryRejection, WaveRetryStatus, WaveRunGeneration,
};
pub use runtime_ui::{
    REFERENCE_RETRY_ACTION, REFERENCE_RUNTIME_UI_PLUGIN_ID, REFERENCE_UI_SURFACE_ID,
    ReferenceHudProjection, ReferenceRuntimeUiPlugin,
};
pub use snapshot::{
    EnemySnapshot, PlayerSnapshot, ProjectileSnapshot, WaveOutcome, WaveSnapshot,
    WaveSnapshotContent,
};
#[cfg(feature = "desktop")]
pub use ui::{REFERENCE_DESKTOP_PLUGIN_ID, ReferenceDesktopPlugin};
pub use ui_observation::{
    ReferenceProductObservation, ReferenceUiObservation, ReferenceUiPhysicalRect,
    ReferenceUiPresentationObservation, ReferenceUiRect,
};
pub use wave_input::{WaveCommand, wave_retry_request_count, with_wave_script};
use wave_input::{WaveInput, WaveIntent, finish_wave_input, prepare_wave_input};

pub const REFERENCE_GAME_PLUGIN_ID: PluginId = PluginId::new("reference-game.gameplay");
pub const REFERENCE_WAVE_PLUGIN_ID: PluginId = PluginId::new("reference-game.wave");
pub const REFERENCE_FIRST_TICK_COMMAND_TYPE: &str = "reference-game.no-op-v1";
pub const REFERENCE_FIRST_TICK_COMMAND_SOURCE: &str = "u26-manual";
pub const REFERENCE_PROJECT_OUTCOME_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.project-outcome");
pub const REFERENCE_GAME_SCHEMA_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("reference-game.components");
pub const REFERENCE_GAME_SCHEMA_OWNER_ID: ComponentSchemaOwnerId =
    ComponentSchemaOwnerId::new("reference-game.components");
pub const REFERENCE_GAME_SCHEMA_PROVIDER: ComponentSchemaProviderDefinition =
    ComponentSchemaProviderDefinition::with_validation(
        REFERENCE_GAME_SCHEMA_OWNER_ID,
        REFERENCE_GAME_SCHEMA_PROVIDER_ID,
        nara::reflect::ComponentSchemaProviderBindingId::new("reference-game.components.native", 5),
        reference_game_schema_v5,
        validate_reference_game_components,
        register_reference_game_components,
    )
    .with_predecessor(reference_game_schema_v4);
const REFERENCE_GAME_REQUIREMENTS: &[PluginId] = &[
    nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID,
    TRANSFORM_COMPONENTS_PLUGIN_ID,
    nara::advanced_prelude::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
];
pub const REFERENCE_GAME_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_GAME_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(REFERENCE_GAME_REQUIREMENTS)
        .provides_schema(&[REFERENCE_GAME_SCHEMA_PROVIDER_ID]);
const REFERENCE_WAVE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_WAVE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[REFERENCE_GAME_PLUGIN_ID, FIXED_SPATIAL_PLUGIN_ID]);
const REFERENCE_PROJECT_OUTCOME_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_PROJECT_OUTCOME_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[REFERENCE_GAME_PLUGIN_ID, FIXED_SPATIAL_PLUGIN_ID]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum ReferenceWaveSet {
    Consume,
    Capture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
enum ReferenceWaveCaptureSet {
    Snapshot,
    Presentation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
enum ReferenceSpatialSet {
    Mutate,
    Resolve,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ReferenceGamePlugin;

impl Plugin for ReferenceGamePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &REFERENCE_GAME_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        {
            let world = app.world_mut()?;
            world.register_component::<Health>();
            world.register_component::<Velocity2d>();
            world.register_component::<WeaponCooldown>();
            world.register_component::<ProjectileRole>();
            world.register_component::<ProjectileDamage>();
            world.register_component::<ProjectileLifetime>();
            world.register_component::<ProjectileId>();
            world.register_component::<systems::ProjectileSpawnTick>();
        }
        app.insert_resource(resources::WaveState::default())?
            .insert_resource(resources::MovementIntent::default())?
            .insert_resource(WaveRunGeneration::default())?
            .insert_resource(WaveRetryStatus::default())?
            .insert_resource(WaveSnapshot::default())?;
        add_startup_scene_activation_systems(app, systems::initialize_reference_run)?;
        app.configure_sets(
            CoreStage::FixedUpdate,
            (
                ReferenceSpatialSet::Mutate.in_set(FixedUpdateSet::Simulate),
                ReferenceSpatialSet::Resolve
                    .after(ReferenceSpatialSet::Mutate)
                    .before(FixedUpdateSet::Finalize),
            ),
        )?;
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
        let faults = app.world().resource::<RuntimeFaultReporter>().clone();
        let capture_faults = faults.clone();
        app.init_resource::<WaveInput>()?
            .configure_sets(
                CoreStage::FixedUpdate,
                (
                    ReferenceWaveSet::Consume.in_set(FixedUpdateSet::Simulate),
                    ReferenceWaveSet::Capture
                        .in_set(FixedUpdateSet::Finalize)
                        .run_if(move || capture_faults.fault().is_none()),
                ),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                (
                    prepare_wave_input.in_set(FixedUpdateSet::Prepare),
                    finish_wave_input
                        .in_set(FixedUpdateSet::Finalize)
                        .after(ReferenceWaveSet::Capture)
                        .run_if(move || faults.fault().is_none()),
                ),
            )?
            .configure_sets(
                CoreStage::FixedUpdate,
                (
                    ReferenceWaveCaptureSet::Snapshot,
                    ReferenceWaveCaptureSet::Presentation,
                )
                    .chain()
                    .in_set(ReferenceWaveSet::Capture),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                systems::begin_wave_tick
                    .in_set(FixedUpdateSet::Simulate)
                    .before(ReferenceWaveSet::Consume),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                (
                    systems::consume_retry_commands,
                    systems::consume_movement_commands,
                )
                    .chain()
                    .in_set(ReferenceWaveSet::Consume),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                (
                    systems::validate_wave_topology,
                    systems::pursue_scene_players,
                    systems::move_scene_players,
                    systems::move_wave_projectiles,
                )
                    .chain()
                    .after(ReferenceWaveSet::Consume)
                    .in_set(ReferenceSpatialSet::Mutate),
            )?;
        add_fixed_transform_projection_systems(
            app,
            (
                systems::fire_automatic_weapons,
                systems::resolve_enemy_contacts,
                systems::resolve_wave_projectile_hits,
                systems::retire_expired_entities,
                systems::evaluate_wave_outcome,
            )
                .chain()
                .in_set(ReferenceSpatialSet::Resolve),
        )?;
        app.add_systems(
            CoreStage::FixedUpdate,
            snapshot::capture_wave_snapshot.in_set(ReferenceWaveCaptureSet::Snapshot),
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
        let faults = app.world().resource::<RuntimeFaultReporter>().clone();
        app.insert_resource(ReferenceProjectSnapshot::default())?
            .add_systems(StartupStage::Runtime, systems::seed_project_first_tick)?
            .add_systems(
                CoreStage::FixedUpdate,
                (
                    systems::move_project_players,
                    systems::move_project_enemies,
                    systems::tick_project_weapons,
                )
                    .chain()
                    .in_set(ReferenceSpatialSet::Mutate),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                systems::observe_project_first_tick.in_set(FixedUpdateSet::Simulate),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                systems::capture_project_snapshot
                    .in_set(FixedUpdateSet::Finalize)
                    .run_if(move || faults.fault().is_none()),
            )?;
        Ok(())
    }
}

/// Contributes the reference game's systems and persistent schema as one definition.
pub fn reference_game_contribution()
-> Result<SchemaContribution<ReferenceGamePlugin>, ProductRecipeError> {
    SchemaContribution::<ReferenceGamePlugin>::for_default([REFERENCE_GAME_SCHEMA_PROVIDER])
}

/// Creates the project outcome recipe used by the file-backed reference task.
///
/// The returned recipe is pure Rust data. It does not open the project or acquire runtime
/// authority; the Host owns those operations when the recipe is passed to a product action.
pub fn project_recipe() -> Result<ProductRecipe, ProductRecipeError> {
    ProductRecipe::new()
        .add_plugin::<FixedSpatialPlugin>()?
        .add_contribution(reference_game_contribution()?)?
        .add_plugin::<ReferenceProjectOutcomePlugin>()
}

/// Creates the deterministic wave recipe shared by the headless and desktop launch paths.
pub fn wave_recipe() -> Result<ProductRecipe, ProductRecipeError> {
    ProductRecipe::new()
        .add_plugin::<FixedSpatialPlugin>()?
        .add_contribution(reference_game_contribution()?)?
        .add_plugin::<ReferenceWavePlugin>()?
        .add_plugin::<ReferenceRuntimeUiPlugin>()
}

#[cfg(feature = "desktop")]
/// Creates the desktop wave recipe by adding the presentation contribution to the wave recipe.
pub fn desktop_wave_recipe() -> Result<ProductRecipe, ProductRecipeError> {
    wave_recipe()?.add_plugin::<ReferenceDesktopPlugin>()
}

fn project_recipe_or_panic() -> ProductRecipe {
    project_recipe().expect("the reference project recipe is statically valid")
}

#[cfg(feature = "desktop")]
fn desktop_wave_recipe_or_panic() -> ProductRecipe {
    desktop_wave_recipe().expect("the reference desktop recipe is statically valid")
}

/// Creates the reference game's project-backed product action.
#[must_use]
pub fn project_headless_run(
    project_root: DirectoryCapability,
    fixed_ticks: NonZeroU32,
) -> HeadlessRun<ReferenceProjectSnapshot> {
    HeadlessRun::from_recipe(project_root, project_recipe_or_panic(), fixed_ticks)
}

/// Runs the bundled deterministic wave until a terminal outcome or the fixed-tick limit.
#[must_use]
pub fn bundled_wave_run(
    project_root: DirectoryCapability,
    maximum_fixed_ticks: NonZeroU32,
) -> HeadlessRun<WaveSnapshot> {
    wave_headless_run(project_root, maximum_fixed_ticks, bundled_wave_commands())
}

/// Runs the bundled deterministic wave and observes completed ticks considered by its terminal
/// predicate.
///
/// This remains a reference-game-only measurement hook. The observer runs only after the Host has
/// completed the fixed tick and captured the wave snapshot, before the terminal predicate decides
/// whether to stop the run. An early App exit intentionally does not invoke this observer.
#[doc(hidden)]
#[must_use]
pub fn bundled_wave_run_with_completed_tick_observer(
    project_root: DirectoryCapability,
    maximum_fixed_ticks: NonZeroU32,
    observer: impl Fn(&WaveSnapshot) + Send + 'static,
) -> HeadlessRun<WaveSnapshot> {
    wave_headless_run_with_completed_tick_observer(
        project_root,
        maximum_fixed_ticks,
        bundled_wave_commands(),
        observer,
    )
}

/// Composes the wave and its bundled movement script for deterministic headless play.
pub fn bundled_wave_recipe() -> Result<ProductRecipe, ProductRecipeError> {
    scripted_wave_recipe(bundled_wave_commands())
}

/// Runs one owned typed semantic-command buffer until the wave terminates or reaches its tick limit.
#[must_use]
pub fn wave_headless_run(
    project_root: DirectoryCapability,
    maximum_fixed_ticks: NonZeroU32,
    commands: Vec<WaveCommand>,
) -> HeadlessRun<WaveSnapshot> {
    HeadlessRun::new(
        project_root,
        HeadlessRunIntent::new(maximum_fixed_ticks)
            .with_recipe(
                scripted_wave_recipe(commands).expect("the reference wave recipe is valid"),
            )
            .stop_when(WaveSnapshot::is_terminal),
    )
}

fn wave_headless_run_with_completed_tick_observer(
    project_root: DirectoryCapability,
    maximum_fixed_ticks: NonZeroU32,
    commands: Vec<WaveCommand>,
    observer: impl Fn(&WaveSnapshot) + Send + 'static,
) -> HeadlessRun<WaveSnapshot> {
    HeadlessRun::new(
        project_root,
        HeadlessRunIntent::new(maximum_fixed_ticks)
            .with_recipe(
                scripted_wave_recipe(commands).expect("the reference wave recipe is valid"),
            )
            .stop_when(move |snapshot: &WaveSnapshot| {
                observer(snapshot);
                snapshot.is_terminal()
            }),
    )
}

/// Creates the desktop profile intent over the same committed wave content closure.
#[cfg(feature = "desktop")]
#[must_use]
pub fn wave_desktop_intent() -> DesktopRunIntent {
    DesktopRunIntent::new()
        .with_profile("desktop")
        .with_recipe(desktop_wave_recipe_or_panic())
}

/// Creates the complete wave run intent over the committed project content.
#[must_use]
pub fn wave_headless_intent(maximum_fixed_ticks: NonZeroU32) -> HeadlessRunIntent<WaveSnapshot> {
    HeadlessRunIntent::new(maximum_fixed_ticks)
        .with_recipe(wave_recipe().expect("the reference wave recipe is valid"))
        .stop_when(WaveSnapshot::is_terminal)
}

fn scripted_wave_recipe(commands: Vec<WaveCommand>) -> Result<ProductRecipe, ProductRecipeError> {
    with_wave_script(wave_recipe()?, commands)
}

/// Returns the semantic movement task bundled into the first headless playable.
#[must_use]
pub fn bundled_wave_commands() -> Vec<WaveCommand> {
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
) -> Result<WaveCommand, MovementCommandError> {
    let tick = NonZeroU64::new(tick).ok_or(MovementCommandError::ZeroTick)?;
    let sequence = NonZeroU64::new(sequence).ok_or(MovementCommandError::ZeroSequence)?;
    Ok(WaveCommand::new(
        tick,
        sequence,
        WaveIntent::Move {
            direction,
            pressed: true,
        },
    ))
}

/// Creates one trusted semantic Retry command for adapters, tests, replay, or AI drivers.
pub fn retry_command(tick: u64, sequence: u64) -> Result<WaveCommand, RetryCommandError> {
    let tick = NonZeroU64::new(tick).ok_or(RetryCommandError::ZeroTick)?;
    let sequence = NonZeroU64::new(sequence).ok_or(RetryCommandError::ZeroSequence)?;
    Ok(WaveCommand::new(
        tick,
        sequence,
        WaveIntent::Retry { ui_sequence: None },
    ))
}

/// Creates the reference game's project-backed run intent.
#[must_use]
pub fn project_headless_intent(
    fixed_ticks: NonZeroU32,
) -> HeadlessRunIntent<ReferenceProjectSnapshot> {
    HeadlessRunIntent::new(fixed_ticks).with_recipe(project_recipe_or_panic())
}

fn reference_game_schema_v1() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError>
{
    ComponentSchemaCatalog::from_json_bytes(include_bytes!("../schema/component-schema-v1.json"))
        .map_err(|_| ComponentSchemaProviderSourceError::new("reference-game-schema-v1-invalid"))
}

fn reference_game_schema_v2() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError>
{
    let predecessor = reference_game_schema_v1()?;
    ComponentSchemaCatalog::from_json_bytes_with_predecessor(
        include_bytes!("../schema/component-schema-v2.json"),
        &predecessor,
        ComponentCatalogFileLimits::default(),
    )
    .map_err(|_| ComponentSchemaProviderSourceError::new("reference-game-schema-v2-invalid"))
}

fn reference_game_schema_v3() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError>
{
    let predecessor = reference_game_schema_v2()?;
    ComponentSchemaCatalog::from_json_bytes_with_predecessor(
        include_bytes!("../schema/component-schema-v3.json"),
        &predecessor,
        ComponentCatalogFileLimits::default(),
    )
    .map_err(|_| ComponentSchemaProviderSourceError::new("reference-game-schema-v3-invalid"))
}

fn reference_game_schema_v4() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError>
{
    let predecessor = reference_game_schema_v3()?;
    ComponentSchemaCatalog::from_json_bytes_with_predecessor(
        include_bytes!("../schema/component-schema-v4.json"),
        &predecessor,
        ComponentCatalogFileLimits::default(),
    )
    .map_err(|_| ComponentSchemaProviderSourceError::new("reference-game-schema-v4-invalid"))
}

fn reference_game_schema_v5() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError>
{
    let predecessor = reference_game_schema_v4()?;
    ComponentSchemaCatalog::from_json_bytes_with_predecessor(
        include_bytes!("../schema/component-schema-v5.json"),
        &predecessor,
        ComponentCatalogFileLimits::default(),
    )
    .map_err(|_| ComponentSchemaProviderSourceError::new("reference-game-schema-v5-invalid"))
}

pub fn register_reference_game_components(
    registry: &mut ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError> {
    validate_reference_game_components(registry)?;
    register_component::<PlayerRole>(registry)?;
    register_component::<EnemyRole>(registry)?;
    register_component::<InitialHealth>(registry)?;
    register_component::<InitialVelocity2d>(registry)?;
    register_component::<WaveSpawn>(registry)?;
    register_component::<Weapon>(registry)?;
    registry.register_component_migration(
        &ComponentTypeId::new("reference_game.InitialVelocity2d"),
        ComponentSchemaVersion::ONE,
        ComponentSchemaVersion::new(2).expect("the velocity schema version is non-zero"),
        migrate_initial_velocity_v1_to_v2,
    )?;
    registry
        .register_component_migration(
            &ComponentTypeId::new("reference_game.Weapon"),
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion::new(2).expect("the weapon schema version is non-zero"),
            migrate_weapon_v1_to_v2,
        )
        .map(|_| ())
}

fn migrate_initial_velocity_v1_to_v2(
    value: ComponentValue,
) -> Result<ComponentValue, ComponentCodecError> {
    nara::reflect::validate_component_value(
        &InitialVelocity2d::persistent_component_schema(),
        &value,
    )?;
    Ok(value)
}

fn migrate_weapon_v1_to_v2(
    mut value: ComponentValue,
) -> Result<ComponentValue, ComponentCodecError> {
    value
        .remove_path(&ComponentFieldPath::from_fields(["remaining-ticks"]))
        .map_err(|error| {
            ComponentCodecError::invalid_field("remaining-ticks", error.to_string())
        })?;
    Ok(value)
}

fn validate_reference_game_components(
    registry: &ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError> {
    registry.validate_persistent_component::<PlayerRole>()?;
    registry.validate_persistent_component::<EnemyRole>()?;
    registry.validate_persistent_component::<InitialHealth>()?;
    registry.validate_persistent_component::<InitialVelocity2d>()?;
    registry.validate_persistent_component::<WaveSpawn>()?;
    registry.validate_persistent_component::<Weapon>()
}

fn register_component<T>(
    registry: &mut ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError>
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
    pub first_command_tick: Option<u64>,
    pub first_command_source: Option<&'static str>,
    pub first_command_type: Option<&'static str>,
    pub runtime_only_entities: u64,
    pub unbound_gameplay_components: u64,
}
