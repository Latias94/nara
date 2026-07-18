//! Independent public-surface reference game.

mod components;
mod systems;

use std::{
    error::Error,
    fmt,
    num::NonZeroU32,
    time::{Duration, Instant},
};

use nara::{
    app::{
        AddPluginsError, PluginCategory, PluginDeclaration, PluginDefinition, PluginError,
        PluginId, PluginPreflightContext, PluginSchemaProviderId, RuntimeAdmissionRetirement,
        RuntimeCandidate, RuntimeCandidateFailure, RuntimeCandidateRetirementState,
        RuntimeCloseEvidence, RuntimeControl, RuntimeControlRejection, RuntimeControlRequestResult,
        RuntimeDriveError, RuntimeRetirement, RuntimeState,
    },
    ecs::schedule::IntoScheduleConfigs,
    fs::{DirectoryCapability, FileCapability},
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandKey,
        GameplayCommandPlugin, GameplayCommandSet, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTick, GameplayCommandTypeId,
    },
    image::ImageImportLimits,
    prelude::{
        App, ComponentRegistry, CoreStage, FixedTime, FixedUpdateSet, HeadlessRuntimePlugins,
        PersistentComponentProvider, Plugin, Resource, RuntimeTimeSettings, StartupStage, Vec2,
        World,
    },
    project_host::{
        HeadlessRun, HeadlessRunIntent, ProjectCandidateError, ProjectRuntimePlugins,
        ProjectSettingsCandidate, ingest_project_manifest, project_runtime_plugins,
    },
    reflect::{
        COMPONENT_REGISTRY_PLUGIN_REQUIREMENT, ComponentRegistryError,
        ComponentSchemaProviderDefinition,
    },
    tilemap::TilemapPlugin,
};

pub use components::{Enemy, Player, Projectile, RuntimeOnlyTag, Weapon};

pub const REFERENCE_GAME_PLUGIN_ID: PluginId = PluginId::new("reference-game.gameplay");
pub const REFERENCE_FIRST_TICK_COMMAND_TYPE: &str = "reference-game.no-op-v1";
pub const REFERENCE_FIRST_TICK_COMMAND_SOURCE: &str = "u26-manual";
pub const REFERENCE_TRACER_SEED_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.tracer-seed");
pub const REFERENCE_PROJECT_OUTCOME_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.project-outcome");
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
const REFERENCE_TRACER_SEED_REQUIREMENTS: &[PluginId] = &[REFERENCE_GAME_PLUGIN_ID];
const REFERENCE_TRACER_SEED_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REFERENCE_TRACER_SEED_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(REFERENCE_TRACER_SEED_REQUIREMENTS);
const REFERENCE_PROJECT_OUTCOME_REQUIREMENTS: &[PluginId] =
    &[REFERENCE_GAME_PLUGIN_ID, nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID];
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

        app.add_systems(
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

#[derive(Debug, Default, Clone, Copy)]
pub struct ReferenceTracerSeedPlugin;

impl Plugin for ReferenceTracerSeedPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &REFERENCE_TRACER_SEED_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Scene, systems::seed_tracer)?;
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
        [project_first_tick_command()],
    )
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
        GameplayCommandSourceSequence::new(1)
            .expect("the reference command sequence is non-zero"),
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

#[derive(Debug, Clone, PartialEq)]
pub enum ReferenceGameRuntimeRetirementCause {
    StopRejected { rejection: RuntimeControlRejection },
    DriveFailed { error: RuntimeDriveError },
    DeadlineExceeded,
    CloseIncomplete,
    InvalidState,
}

#[must_use = "runtime retirement errors retain close authority until explicitly retired"]
pub struct ReferenceGameRuntimeRetirementError {
    cause: ReferenceGameRuntimeRetirementCause,
    observed_state: RuntimeState,
    retirement: RuntimeRetirement,
}

impl fmt::Debug for ReferenceGameRuntimeRetirementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReferenceGameRuntimeRetirementError")
            .field("cause", &self.cause)
            .field("observed_state", &self.observed_state)
            .field("retirement", &self.retirement)
            .finish()
    }
}

impl ReferenceGameRuntimeRetirementError {
    fn new(
        cause: ReferenceGameRuntimeRetirementCause,
        runtime: nara::app::RuntimeInstance,
    ) -> Self {
        let observed_state = runtime.state();
        Self {
            cause,
            observed_state,
            retirement: runtime.begin_retirement(),
        }
    }

    #[must_use]
    pub const fn cause(&self) -> &ReferenceGameRuntimeRetirementCause {
        &self.cause
    }

    #[must_use]
    pub const fn observed_state(&self) -> RuntimeState {
        self.observed_state
    }

    #[must_use]
    pub fn retirement_state(&self) -> RuntimeCandidateRetirementState {
        self.retirement.retirement_state()
    }

    #[must_use]
    pub fn close_evidence(&self) -> &RuntimeCloseEvidence {
        self.retirement.close_evidence()
    }

    #[must_use]
    pub const fn retirement(&self) -> &RuntimeRetirement {
        &self.retirement
    }

    pub const fn retirement_mut(&mut self) -> &mut RuntimeRetirement {
        &mut self.retirement
    }

    #[must_use]
    pub fn into_retirement(self) -> RuntimeRetirement {
        self.retirement
    }
}

impl fmt::Display for ReferenceGameRuntimeRetirementError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.cause {
            ReferenceGameRuntimeRetirementCause::StopRejected { rejection } => write!(
                formatter,
                "runtime stop was rejected in state {:?}: {rejection:?}",
                self.observed_state
            ),
            ReferenceGameRuntimeRetirementCause::DriveFailed { error } => write!(
                formatter,
                "runtime retirement drive failed in state {:?}: {error}",
                self.observed_state
            ),
            ReferenceGameRuntimeRetirementCause::DeadlineExceeded => write!(
                formatter,
                "runtime retirement deadline expired in state {:?}",
                self.observed_state
            ),
            ReferenceGameRuntimeRetirementCause::CloseIncomplete => write!(
                formatter,
                "runtime retirement is incomplete in state {:?}",
                self.observed_state
            ),
            ReferenceGameRuntimeRetirementCause::InvalidState => write!(
                formatter,
                "runtime cannot begin retirement from state {:?}",
                self.observed_state
            ),
        }
    }
}

impl Error for ReferenceGameRuntimeRetirementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.cause {
            ReferenceGameRuntimeRetirementCause::DriveFailed { error } => Some(error),
            ReferenceGameRuntimeRetirementCause::StopRejected { .. }
            | ReferenceGameRuntimeRetirementCause::DeadlineExceeded
            | ReferenceGameRuntimeRetirementCause::CloseIncomplete
            | ReferenceGameRuntimeRetirementCause::InvalidState => None,
        }
    }
}

#[derive(Debug)]
pub enum ReferenceGameError {
    Composition(AddPluginsError),
    Plugin(PluginError),
    RuntimeAdmission(RuntimeAdmissionRetirement),
    RuntimeStartup(RuntimeCandidateFailure),
    RuntimeDrive(RuntimeDriveError),
    RuntimeRetirement(ReferenceGameRuntimeRetirementError),
    RuntimeTeardown {
        prior: Box<ReferenceGameError>,
        teardown: ReferenceGameRuntimeRetirementError,
    },
    Project(ProjectCandidateError),
    MissingResource(&'static str),
    MissingComponent(&'static str),
    DuplicateComponent(&'static str),
    UnexpectedFixedSteps {
        expected: u32,
        actual: u32,
    },
}

impl fmt::Display for ReferenceGameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Composition(error) => {
                write!(formatter, "reference-game composition failed: {error}")
            }
            Self::Plugin(error) => write!(formatter, "reference-game plugin setup failed: {error}"),
            Self::RuntimeAdmission(error) => {
                write!(
                    formatter,
                    "reference-game runtime admission failed: {error}"
                )
            }
            Self::RuntimeStartup(error) => {
                write!(formatter, "reference-game runtime startup failed: {error}")
            }
            Self::RuntimeDrive(error) => {
                write!(formatter, "reference-game runtime drive failed: {error}")
            }
            Self::RuntimeRetirement(error) => {
                write!(
                    formatter,
                    "reference-game runtime retirement failed: {error}"
                )
            }
            Self::RuntimeTeardown { prior, teardown } => write!(
                formatter,
                "{prior}; runtime teardown also failed: {teardown}"
            ),
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
            Self::RuntimeAdmission(error) => Some(error),
            Self::RuntimeStartup(error) => Some(error),
            Self::RuntimeDrive(error) => Some(error),
            Self::RuntimeRetirement(error) => Some(error),
            Self::RuntimeTeardown { prior, .. } => Some(prior),
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

impl From<RuntimeCandidateFailure> for ReferenceGameError {
    fn from(error: RuntimeCandidateFailure) -> Self {
        Self::RuntimeStartup(error)
    }
}

impl From<RuntimeDriveError> for ReferenceGameError {
    fn from(error: RuntimeDriveError) -> Self {
        Self::RuntimeDrive(error)
    }
}

impl From<ReferenceGameRuntimeRetirementError> for ReferenceGameError {
    fn from(error: ReferenceGameRuntimeRetirementError) -> Self {
        Self::RuntimeRetirement(error)
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
    app.add_plugins((
        HeadlessRuntimePlugins,
        ReferenceGamePlugin,
        ReferenceTracerSeedPlugin,
    ))?;
    if let Some((runtime_time, fixed_time)) = time {
        app.insert_resource(runtime_time)?;
        app.insert_resource(fixed_time)?;
    }

    let candidate = match RuntimeCandidate::admit(app.seal()?) {
        Ok(candidate) => candidate,
        Err(failure) => return Err(retire_admission_failure(failure)),
    };
    let ready = match candidate.complete_startup() {
        Ok(ready) => ready,
        Err(mut failure) => {
            while failure.retirement_state() == RuntimeCandidateRetirementState::Retiring {
                failure.drive_retirement();
                std::thread::park_timeout(Duration::from_millis(1));
            }
            return Err(ReferenceGameError::RuntimeStartup(failure));
        }
    };
    let mut runtime = ready.promote();
    let fixed_timestep = runtime.world().resource::<FixedTime>().timestep();
    let primary = (|| {
        for _ in 0..ticks {
            let outcome = runtime.drive(fixed_timestep)?;
            let actual = outcome.frame().map_or(0, |frame| frame.status.fixed_steps);
            if actual != 1 {
                return Err(ReferenceGameError::UnexpectedFixedSteps {
                    expected: 1,
                    actual,
                });
            }
        }
        TracerSnapshot::capture(runtime.world())
    })();
    finish_headless_run(primary, runtime)
}

fn finish_headless_run(
    primary: Result<TracerSnapshot, ReferenceGameError>,
    runtime: nara::app::RuntimeInstance,
) -> Result<TracerSnapshot, ReferenceGameError> {
    let teardown = close_runtime(runtime);
    match (primary, teardown) {
        (Ok(snapshot), Ok(())) => Ok(snapshot),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(ReferenceGameError::RuntimeRetirement(error)),
        (Err(prior), Err(teardown)) => Err(ReferenceGameError::RuntimeTeardown {
            prior: Box::new(prior),
            teardown,
        }),
    }
}

fn retire_admission_failure(failure: nara::app::RuntimeAdmissionFailure) -> ReferenceGameError {
    let mut retirement = failure.begin_retirement();
    while retirement.retirement_state() == RuntimeCandidateRetirementState::Retiring {
        retirement.drive_retirement();
        std::thread::park_timeout(Duration::from_millis(1));
    }
    ReferenceGameError::RuntimeAdmission(retirement)
}

fn close_runtime(
    mut runtime: nara::app::RuntimeInstance,
) -> Result<(), ReferenceGameRuntimeRetirementError> {
    match runtime.state() {
        RuntimeState::Running | RuntimeState::Paused | RuntimeState::Faulted => {
            if let RuntimeControlRequestResult::Rejected(rejection) =
                runtime.request_control(RuntimeControl::Stop)
            {
                return Err(ReferenceGameRuntimeRetirementError::new(
                    ReferenceGameRuntimeRetirementCause::StopRejected { rejection },
                    runtime,
                ));
            }
        }
        RuntimeState::Stopping => {}
        RuntimeState::CloseIncomplete => {
            return Err(ReferenceGameRuntimeRetirementError::new(
                ReferenceGameRuntimeRetirementCause::CloseIncomplete,
                runtime,
            ));
        }
        RuntimeState::Stepping => {
            return Err(ReferenceGameRuntimeRetirementError::new(
                ReferenceGameRuntimeRetirementCause::InvalidState,
                runtime,
            ));
        }
        RuntimeState::Stopped => return Ok(()),
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match runtime.state() {
            RuntimeState::Stopped => return Ok(()),
            RuntimeState::CloseIncomplete => {
                return Err(ReferenceGameRuntimeRetirementError::new(
                    ReferenceGameRuntimeRetirementCause::CloseIncomplete,
                    runtime,
                ));
            }
            RuntimeState::Stepping => {
                return Err(ReferenceGameRuntimeRetirementError::new(
                    ReferenceGameRuntimeRetirementCause::InvalidState,
                    runtime,
                ));
            }
            RuntimeState::Running
            | RuntimeState::Paused
            | RuntimeState::Faulted
            | RuntimeState::Stopping => {}
        }

        if Instant::now() >= deadline {
            return Err(ReferenceGameRuntimeRetirementError::new(
                ReferenceGameRuntimeRetirementCause::DeadlineExceeded,
                runtime,
            ));
        }
        if let Err(error) = runtime.drive(Duration::ZERO) {
            return Err(ReferenceGameRuntimeRetirementError::new(
                ReferenceGameRuntimeRetirementCause::DriveFailed { error },
                runtime,
            ));
        }
        if runtime.state() != RuntimeState::Stopped {
            std::thread::park_timeout(Duration::from_millis(1));
        }
    }
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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use nara::app::{
        RuntimeCloseCause, RuntimeCloseContext, RuntimeCloseParticipant,
        RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeClosePolicy,
        RuntimeCloseProgress, RuntimeObligationLedger,
    };

    use super::*;

    struct ReleasedCloseParticipant {
        released: Arc<AtomicBool>,
    }

    impl RuntimeCloseParticipant for ReleasedCloseParticipant {
        fn begin_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            Ok(RuntimeCloseProgress::Pending)
        }

        fn poll_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            Ok(if self.released.load(Ordering::Acquire) {
                RuntimeCloseProgress::Complete
            } else {
                RuntimeCloseProgress::Pending
            })
        }
    }

    fn runtime_with_pending_close(released: Arc<AtomicBool>) -> nara::app::RuntimeInstance {
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("reference-game.test.pending-close"),
                ReleasedCloseParticipant { released },
            )
            .unwrap();
        let candidate = RuntimeCandidate::admit_with(
            App::new().seal().unwrap(),
            obligations,
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap();
        candidate.complete_startup().unwrap().promote()
    }

    #[test]
    fn failed_close_returns_retryable_retirement_owner() {
        let released = Arc::new(AtomicBool::new(false));
        let runtime = runtime_with_pending_close(Arc::clone(&released));

        let mut failure = close_runtime(runtime).unwrap_err();

        assert!(matches!(
            failure.cause(),
            ReferenceGameRuntimeRetirementCause::CloseIncomplete
        ));
        assert_eq!(failure.observed_state(), RuntimeState::CloseIncomplete);
        assert_eq!(
            failure.retirement_state(),
            RuntimeCandidateRetirementState::RetirementIncomplete
        );
        assert!(
            failure
                .close_evidence()
                .causes()
                .contains(&RuntimeCloseCause::DeadlineExceeded)
        );

        released.store(true, Ordering::Release);
        assert_eq!(
            failure.retirement_mut().drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
        let retirement = failure.into_retirement();
        assert_eq!(
            retirement.retirement_state(),
            RuntimeCandidateRetirementState::Retired
        );
    }

    #[test]
    fn primary_and_teardown_failure_preserve_both_and_retirement_authority() {
        let released = Arc::new(AtomicBool::new(false));
        let runtime = runtime_with_pending_close(Arc::clone(&released));
        let primary = ReferenceGameError::UnexpectedFixedSteps {
            expected: 1,
            actual: 0,
        };

        let failure = finish_headless_run(Err(primary), runtime).unwrap_err();

        let ReferenceGameError::RuntimeTeardown {
            prior,
            mut teardown,
        } = failure
        else {
            panic!("primary plus close failure must retain both errors");
        };
        assert!(matches!(
            *prior,
            ReferenceGameError::UnexpectedFixedSteps {
                expected: 1,
                actual: 0
            }
        ));
        assert_eq!(teardown.observed_state(), RuntimeState::CloseIncomplete);
        assert!(
            teardown
                .close_evidence()
                .causes()
                .contains(&RuntimeCloseCause::DeadlineExceeded)
        );

        released.store(true, Ordering::Release);
        assert_eq!(
            teardown.retirement_mut().drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
    }
}
