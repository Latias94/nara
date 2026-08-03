//! Independent public-surface reference game.

mod components;
#[cfg(feature = "desktop")]
mod input;
mod resources;
mod snapshot;
mod systems;
#[cfg(feature = "desktop")]
mod ui;

use std::num::NonZeroU32;

use nara::{
    ProductRecipe, ProductRecipeError, SchemaContribution,
    advanced_prelude::StartupSceneActivationSet,
    app::{
        PluginCategory, PluginDeclaration, PluginDefinition, PluginError, PluginId,
        PluginPreflightContext, PluginSchemaProviderId, StartupStage,
    },
    ecs::{SystemSet, schedule::IntoScheduleConfigs},
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
    project_host::{HeadlessRun, HeadlessRunIntent},
    reflect::{
        ComponentCatalogFileLimits, ComponentCodecError, ComponentFieldPath,
        ComponentRegistryError, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaProviderDefinition, ComponentSchemaProviderSourceError,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    },
    tilemap::TilemapPlugin,
};

#[cfg(feature = "editor")]
use nara::project_host::EditorProjectIntent;
#[cfg(feature = "desktop")]
use nara::project_host::{DesktopRun, DesktopRunIntent};

pub use components::{
    EnemyRole, Health, InitialHealth, InitialVelocity2d, PlayerRole, ProjectileDamage,
    ProjectileLifetime, ProjectileRole, RuntimeOnlyTag, Velocity2d, WaveSpawn, Weapon,
    WeaponCooldown,
};
pub use resources::{
    MovementCommandError, MovementDirection, ProjectileId, RetryCommandError, WaveRetryPhase,
    WaveRetryRejection, WaveRetryStatus, WaveRunGeneration,
};
pub use snapshot::{EnemySnapshot, PlayerSnapshot, ProjectileSnapshot, WaveOutcome, WaveSnapshot};
#[cfg(feature = "desktop")]
pub use ui::{REFERENCE_DESKTOP_PLUGIN_ID, ReferenceDesktopPlugin, ReferenceHudProjection};

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
        nara::reflect::ComponentSchemaProviderBindingId::new("reference-game.components.native", 4),
        reference_game_schema_v4,
        validate_reference_game_components,
        register_reference_game_components,
    )
    .with_predecessor(reference_game_schema_v3);
const REFERENCE_GAME_REQUIREMENTS: &[PluginId] = &[
    nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID,
    nara::transform::TRANSFORM_PLUGIN_ID,
    nara::advanced_prelude::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
];
pub const REFERENCE_GAME_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_GAME_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(REFERENCE_GAME_REQUIREMENTS)
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

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        let registry = nara::reflect::registry_for_plugin_preflight(
            context,
            REFERENCE_GAME_PLUGIN_ID,
            REFERENCE_GAME_SCHEMA_PROVIDER_ID.as_str(),
        )?;
        REFERENCE_GAME_SCHEMA_PROVIDER
            .preflight(registry)
            .map_err(|error| {
                PluginError::component_registration(
                    REFERENCE_GAME_PLUGIN_ID,
                    REFERENCE_GAME_SCHEMA_PROVIDER_ID.as_str(),
                    error,
                )
            })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        nara::reflect::register_schema_provider_for_plugin(
            app,
            REFERENCE_GAME_PLUGIN_ID,
            REFERENCE_GAME_SCHEMA_PROVIDER_ID.as_str(),
            &REFERENCE_GAME_SCHEMA_PROVIDER,
        )?;
        {
            let world = app.world_mut()?;
            world.register_component::<Health>();
            world.register_component::<Velocity2d>();
            world.register_component::<WeaponCooldown>();
            world.register_component::<ProjectileRole>();
            world.register_component::<ProjectileDamage>();
            world.register_component::<ProjectileLifetime>();
            world.register_component::<ProjectileId>();
        }
        app.insert_resource(resources::WaveState::default())?
            .insert_resource(resources::MovementIntent::default())?
            .insert_resource(WaveRunGeneration::default())?
            .insert_resource(WaveRetryStatus::default())?
            .insert_resource(WaveSnapshot::default())?
            .add_systems(
                StartupStage::Runtime,
                systems::initialize_reference_run.in_set(StartupSceneActivationSet),
            )?;
        app.configure_sets(
            CoreStage::FixedUpdate,
            (
                ReferenceSpatialSet::Mutate.in_set(FixedUpdateSet::Simulate),
                ReferenceSpatialSet::Resolve
                    .in_set(FixedUpdateSet::Finalize)
                    .before(GameplayCommandSet::Capture),
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
        app.configure_sets(
            CoreStage::FixedUpdate,
            (
                ReferenceWaveCaptureSet::Snapshot,
                ReferenceWaveCaptureSet::Presentation,
            )
                .chain()
                .in_set(GameplayCommandSet::Capture),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            systems::begin_wave_tick
                .in_set(FixedUpdateSet::Simulate)
                .before(GameplayCommandSet::Consume),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            (
                systems::consume_retry_commands,
                systems::consume_movement_commands,
            )
                .chain()
                .in_set(GameplayCommandSet::Consume),
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
                .after(GameplayCommandSet::Consume)
                .in_set(ReferenceSpatialSet::Mutate),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            (
                systems::fire_automatic_weapons,
                systems::resolve_enemy_contacts,
                systems::resolve_wave_projectile_hits,
                systems::retire_expired_entities,
                systems::evaluate_wave_outcome,
            )
                .chain()
                .in_set(ReferenceSpatialSet::Resolve),
        )?
        .add_systems(
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
        app.insert_resource(ReferenceProjectSnapshot::default())?
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
                systems::observe_project_commands.in_set(GameplayCommandSet::Consume),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                systems::capture_project_snapshot.in_set(GameplayCommandSet::Capture),
            )?;
        Ok(())
    }
}

/// Returns the raw project-outcome definition used by engine-owned plan fixtures.
///
/// Ordinary product code must use [`project_recipe`] instead.
#[doc(hidden)]
#[must_use]
pub fn advanced_project_outcome_plugin_definition() -> PluginDefinition {
    PluginDefinition::for_default::<ReferenceProjectOutcomePlugin>()
}

fn reference_game_contribution()
-> Result<SchemaContribution<ReferenceGamePlugin>, ProductRecipeError> {
    SchemaContribution::<ReferenceGamePlugin>::for_default([REFERENCE_GAME_SCHEMA_PROVIDER])
}

/// Creates the project outcome recipe used by the file-backed reference task.
///
/// The returned recipe is pure Rust data. It does not open the project or acquire runtime
/// authority; the Host owns those operations when the recipe is passed to a product action.
pub fn project_recipe() -> Result<ProductRecipe, ProductRecipeError> {
    ProductRecipe::new()
        .add_contribution(reference_game_contribution()?)?
        .add_plugin::<ReferenceProjectOutcomePlugin>()
}

/// Creates the deterministic wave recipe shared by headless, desktop, and editor paths.
pub fn wave_recipe() -> Result<ProductRecipe, ProductRecipeError> {
    ProductRecipe::new()
        .add_contribution(reference_game_contribution()?)?
        .add_plugin::<ReferenceWavePlugin>()
}

#[cfg(feature = "desktop")]
/// Creates the desktop wave recipe by adding the presentation contribution to the wave recipe.
pub fn desktop_wave_recipe() -> Result<ProductRecipe, ProductRecipeError> {
    wave_recipe()?.add_plugin::<ReferenceDesktopPlugin>()
}

fn project_recipe_or_panic() -> ProductRecipe {
    project_recipe().expect("the reference project recipe is statically valid")
}

fn wave_recipe_or_panic() -> ProductRecipe {
    wave_recipe().expect("the reference wave recipe is statically valid")
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
    HeadlessRun::from_recipe(
        project_root,
        project_recipe_or_panic(),
        fixed_ticks,
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

fn wave_headless_run_with_completed_tick_observer(
    project_root: DirectoryCapability,
    maximum_fixed_ticks: NonZeroU32,
    commands: Vec<GameplayCommandSubmission>,
    observer: impl Fn(&WaveSnapshot) + Send + 'static,
) -> HeadlessRun<WaveSnapshot> {
    HeadlessRun::new(
        project_root,
        wave_headless_intent_with_completed_tick_observer(maximum_fixed_ticks, observer),
        commands,
    )
}

/// Creates the bundled manually playable desktop product action.
#[cfg(feature = "desktop")]
#[must_use]
pub fn bundled_desktop_run(project_root: DirectoryCapability) -> DesktopRun {
    DesktopRun::new(project_root, wave_desktop_intent())
}

/// Creates the desktop profile intent over the same committed wave content closure.
#[cfg(feature = "desktop")]
#[must_use]
pub fn wave_desktop_intent() -> DesktopRunIntent {
    DesktopRunIntent::new()
        .with_profile("desktop")
        .with_recipe(desktop_wave_recipe_or_panic())
}

/// Builds an explicit raw extension path for Nara-owned desktop probes and regression tests.
///
/// Normal product entry points must use [`desktop_wave_recipe`] through [`wave_desktop_intent`].
/// This helper intentionally keeps one-shot plugin definitions out of that ordinary path.
#[cfg(feature = "desktop")]
#[doc(hidden)]
pub fn advanced_wave_desktop_intent_after<P: Plugin>(
    definition: PluginDefinition,
) -> DesktopRunIntent {
    DesktopRunIntent::new()
        .with_profile("desktop")
        .configure(nara::image::plugin(ImageImportLimits::default()))
        .disable::<TilemapPlugin>()
        .insert_after::<GameplayCommandPlugin>(
            PluginDefinition::for_default::<ReferenceGamePlugin>(),
        )
        .insert_after::<ReferenceGamePlugin>(PluginDefinition::for_default::<ReferenceWavePlugin>())
        .insert_after::<ReferenceWavePlugin>(
            PluginDefinition::for_default::<ReferenceDesktopPlugin>(),
        )
        .insert_after::<P>(definition)
        .with_schema_provider(REFERENCE_GAME_SCHEMA_PROVIDER)
}

/// Creates the Editor Play intent over the same committed wave content closure.
#[cfg(feature = "editor")]
#[must_use]
pub fn wave_editor_intent() -> EditorProjectIntent {
    EditorProjectIntent::new().with_recipe(wave_recipe_or_panic())
}

/// Builds an explicit raw extension path for Nara-owned Editor probes and regression tests.
///
/// Normal Editor sessions must use [`wave_editor_intent`], which carries the ordinary wave recipe.
#[cfg(feature = "editor")]
#[doc(hidden)]
pub fn advanced_wave_editor_intent_after<P: Plugin>(
    definition: PluginDefinition,
) -> EditorProjectIntent {
    EditorProjectIntent::new()
        .configure(nara::image::plugin(ImageImportLimits::default()))
        .disable::<TilemapPlugin>()
        .insert_after::<GameplayCommandPlugin>(
            PluginDefinition::for_default::<ReferenceGamePlugin>(),
        )
        .insert_after::<ReferenceGamePlugin>(PluginDefinition::for_default::<ReferenceWavePlugin>())
        .insert_after::<P>(definition)
        .with_schema_provider(REFERENCE_GAME_SCHEMA_PROVIDER)
}

/// Creates the complete wave run intent over the committed project content.
#[must_use]
pub fn wave_headless_intent(maximum_fixed_ticks: NonZeroU32) -> HeadlessRunIntent<WaveSnapshot> {
    base_wave_headless_intent(maximum_fixed_ticks).stop_when(WaveSnapshot::is_terminal)
}

/// Builds an explicit raw extension path for Nara-owned headless probes and regression tests.
///
/// Normal product actions must use [`wave_headless_intent`], which carries the ordinary wave
/// recipe and does not expose raw plugin definitions.
#[doc(hidden)]
pub fn advanced_wave_headless_intent_after<P: Plugin>(
    maximum_fixed_ticks: NonZeroU32,
    definition: PluginDefinition,
) -> HeadlessRunIntent<WaveSnapshot> {
    HeadlessRunIntent::new(maximum_fixed_ticks)
        .configure(nara::image::plugin(ImageImportLimits::default()))
        .disable::<TilemapPlugin>()
        .insert_after::<GameplayCommandPlugin>(
            PluginDefinition::for_default::<ReferenceGamePlugin>(),
        )
        .insert_after::<ReferenceGamePlugin>(PluginDefinition::for_default::<ReferenceWavePlugin>())
        .insert_after::<P>(definition)
        .with_schema_provider(REFERENCE_GAME_SCHEMA_PROVIDER)
        .stop_when(WaveSnapshot::is_terminal)
}

fn wave_headless_intent_with_completed_tick_observer(
    maximum_fixed_ticks: NonZeroU32,
    observer: impl Fn(&WaveSnapshot) + Send + 'static,
) -> HeadlessRunIntent<WaveSnapshot> {
    base_wave_headless_intent(maximum_fixed_ticks).stop_when(move |snapshot| {
        observer(snapshot);
        snapshot.is_terminal()
    })
}

fn base_wave_headless_intent(maximum_fixed_ticks: NonZeroU32) -> HeadlessRunIntent<WaveSnapshot> {
    HeadlessRunIntent::new(maximum_fixed_ticks).with_recipe(wave_recipe_or_panic())
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
    Ok(GameplayCommandSubmission::new(
        tick,
        GameplayCommandIngressSource::test(resources::COMMAND_SOURCE)
            .expect("the movement command source is engine-owned and valid"),
        sequence,
        movement_draft(direction),
    ))
}

/// Creates one trusted semantic Retry command for adapters, tests, replay, or AI drivers.
pub fn retry_command(
    tick: u64,
    sequence: u64,
) -> Result<GameplayCommandSubmission, RetryCommandError> {
    let tick = GameplayCommandTick::new(tick).ok_or(RetryCommandError::ZeroTick)?;
    let sequence =
        GameplayCommandSourceSequence::new(sequence).ok_or(RetryCommandError::ZeroSequence)?;
    Ok(GameplayCommandSubmission::new(
        tick,
        GameplayCommandIngressSource::test(resources::COMMAND_SOURCE)
            .expect("the retry command source is engine-owned and valid"),
        sequence,
        retry_draft(),
    ))
}

fn movement_draft(direction: MovementDirection) -> GameplayCommandDraft {
    movement_intent_draft(direction, true)
}

#[cfg(feature = "desktop")]
fn movement_release_draft(direction: MovementDirection) -> GameplayCommandDraft {
    movement_intent_draft(direction, false)
}

fn movement_intent_draft(direction: MovementDirection, pressed: bool) -> GameplayCommandDraft {
    let (x, y) = direction.velocity();
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert(resources::MOVE_X_FIELD, GameplayCommandValue::I64(x))
        .expect("the engine-owned movement payload is bounded");
    payload
        .insert(resources::MOVE_Y_FIELD, GameplayCommandValue::I64(y))
        .expect("the engine-owned movement payload is bounded");
    payload
        .insert(
            resources::MOVE_PRESSED_FIELD,
            GameplayCommandValue::Bool(pressed),
        )
        .expect("the engine-owned movement payload is bounded");
    GameplayCommandDraft::new(
        GameplayCommandTypeId::new(resources::MOVE_COMMAND_TYPE)
            .expect("the movement command type is engine-owned and valid"),
    )
    .with_payload(payload)
}

fn retry_draft() -> GameplayCommandDraft {
    GameplayCommandDraft::new(
        GameplayCommandTypeId::new(resources::RETRY_COMMAND_TYPE)
            .expect("the retry command type is engine-owned and valid"),
    )
}

/// Creates the reference game's project-backed run intent.
#[must_use]
pub fn project_headless_intent(
    fixed_ticks: NonZeroU32,
) -> HeadlessRunIntent<ReferenceProjectSnapshot> {
    HeadlessRunIntent::new(fixed_ticks).with_recipe(project_recipe_or_panic())
}

/// Builds an explicit raw extension path for Nara-owned headless probes and regression tests.
///
/// Normal project actions must use [`project_headless_intent`] or [`project_headless_run`].
#[doc(hidden)]
pub fn advanced_project_headless_intent_after<P: Plugin>(
    fixed_ticks: NonZeroU32,
    definition: PluginDefinition,
) -> HeadlessRunIntent<ReferenceProjectSnapshot> {
    HeadlessRunIntent::new(fixed_ticks)
        .configure(nara::image::plugin(ImageImportLimits::default()))
        .disable::<TilemapPlugin>()
        .insert_after::<GameplayCommandPlugin>(
            PluginDefinition::for_default::<ReferenceGamePlugin>(),
        )
        .insert_after::<ReferenceGamePlugin>(advanced_project_outcome_plugin_definition())
        .insert_after::<P>(definition)
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

pub fn register_reference_game_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    validate_reference_game_components(registry)?;
    for removed_component in [
        "reference_game.Enemy",
        "reference_game.Player",
        "reference_game.Projectile",
    ] {
        registry.declare_type_tombstone(ComponentTypeId::new(removed_component))?;
    }
    register_component::<PlayerRole>(registry)?;
    register_component::<EnemyRole>(registry)?;
    register_component::<InitialHealth>(registry)?;
    register_component::<InitialVelocity2d>(registry)?;
    register_component::<WaveSpawn>(registry)?;
    register_component::<Weapon>(registry)?;
    registry
        .register_component_migration(
            &ComponentTypeId::new("reference_game.Weapon"),
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion::new(2).expect("the weapon schema version is non-zero"),
            migrate_weapon_v1_to_v2,
        )
        .map(|_| ())
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
    registry: &ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    registry.validate_persistent_component::<PlayerRole>()?;
    registry.validate_persistent_component::<EnemyRole>()?;
    registry.validate_persistent_component::<InitialHealth>()?;
    registry.validate_persistent_component::<InitialVelocity2d>()?;
    registry.validate_persistent_component::<WaveSpawn>()?;
    registry.validate_persistent_component::<Weapon>()
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
