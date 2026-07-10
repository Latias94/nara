//! Application lifecycle and plugin orchestration for nara.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Display, Formatter},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    time::Duration,
};

use nara_ecs::{
    Resource, World,
    schedule::{InternedSystemSet, IntoScheduleConfigs, Schedule, ScheduleLabel, SystemSet},
    system::ScheduleSystem,
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ScheduleLabel)]
pub enum StartupStage {
    Core,
    Platform,
    Runtime,
    Scene,
    Tooling,
}

impl StartupStage {
    pub const ALL: [Self; 5] = [
        Self::Core,
        Self::Platform,
        Self::Runtime,
        Self::Scene,
        Self::Tooling,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ScheduleLabel)]
pub enum CoreStage {
    First,
    TaskUpdate,
    PreUpdate,
    FixedUpdate,
    Update,
    PostUpdate,
    Extract,
    Prepare,
    Queue,
    Sort,
    Render,
    Cleanup,
    Last,
}

impl CoreStage {
    pub const ALL: [Self; 13] = [
        Self::First,
        Self::TaskUpdate,
        Self::PreUpdate,
        Self::FixedUpdate,
        Self::Update,
        Self::PostUpdate,
        Self::Extract,
        Self::Prepare,
        Self::Queue,
        Self::Sort,
        Self::Render,
        Self::Cleanup,
        Self::Last,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum TaskUpdateSet {
    Poll,
    CoalesceAssetChanges,
    SpawnAssetJobs,
    ApplyAssetResults,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginId(&'static str);

impl PluginId {
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl Display for PluginId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginCapability(&'static str);

impl PluginCapability {
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl Display for PluginCapability {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PluginGroupId(&'static str);

impl PluginGroupId {
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl Display for PluginGroupId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PluginCategory {
    Core,
    Asset,
    Runtime,
    Render,
    Platform,
    Tooling,
    Backend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginMetadata {
    pub id: PluginId,
    pub category: PluginCategory,
    pub provides: &'static [PluginCapability],
    pub requires_plugins: &'static [PluginId],
    pub requires_capabilities: &'static [PluginCapability],
    pub conflicts: &'static [PluginId],
    pub unique: bool,
}

impl PluginMetadata {
    #[must_use]
    pub const fn new(id: PluginId, category: PluginCategory) -> Self {
        Self {
            id,
            category,
            provides: &[],
            requires_plugins: &[],
            requires_capabilities: &[],
            conflicts: &[],
            unique: true,
        }
    }

    #[must_use]
    pub const fn provides(mut self, capabilities: &'static [PluginCapability]) -> Self {
        self.provides = capabilities;
        self
    }

    #[must_use]
    pub const fn requires_plugins(mut self, plugins: &'static [PluginId]) -> Self {
        self.requires_plugins = plugins;
        self
    }

    #[must_use]
    pub const fn requires_capabilities(
        mut self,
        capabilities: &'static [PluginCapability],
    ) -> Self {
        self.requires_capabilities = capabilities;
        self
    }

    #[must_use]
    pub const fn conflicts(mut self, plugins: &'static [PluginId]) -> Self {
        self.conflicts = plugins;
        self
    }

    #[must_use]
    pub const fn non_unique(mut self) -> Self {
        self.unique = false;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginGroupMetadata {
    pub id: PluginGroupId,
    pub plugins: &'static [PluginId],
}

impl PluginGroupMetadata {
    #[must_use]
    pub const fn new(id: PluginGroupId, plugins: &'static [PluginId]) -> Self {
        Self { id, plugins }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginLifecycleState {
    Configuring,
    Finishing,
    Ready,
    Poisoned,
    Cleaning,
    Cleaned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginHook {
    Metadata,
    Preflight,
    Build,
    Finish,
    Cleanup,
}

impl Display for PluginHook {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Metadata => "metadata",
            Self::Preflight => "preflight",
            Self::Build => "build",
            Self::Finish => "finish",
            Self::Cleanup => "cleanup",
        };
        formatter.write_str(name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginFailureSubject {
    Plugin(PluginId),
    Group(PluginGroupId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginFailure {
    subject: PluginFailureSubject,
    hook: PluginHook,
    error: PluginError,
}

impl PluginFailure {
    #[must_use]
    pub const fn subject(&self) -> PluginFailureSubject {
        self.subject
    }

    #[must_use]
    pub const fn hook(&self) -> PluginHook {
        self.hook
    }

    #[must_use]
    pub const fn error(&self) -> &PluginError {
        &self.error
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginFailureReport {
    primary: Option<PluginFailure>,
    cleanup_failures: Vec<PluginFailure>,
    cleanup_complete: bool,
}

impl PluginFailureReport {
    #[must_use]
    pub const fn primary(&self) -> Option<&PluginFailure> {
        self.primary.as_ref()
    }

    #[must_use]
    pub fn cleanup_failures(&self) -> &[PluginFailure] {
        &self.cleanup_failures
    }

    #[must_use]
    pub const fn cleanup_complete(&self) -> bool {
        self.cleanup_complete
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PluginCleanupError {
    #[error("plugin cleanup cannot run while a committed plugin hook is active")]
    HookActive,
    #[error("plugin lifecycle or cleanup failed")]
    Failure(Box<PluginFailureReport>),
}

pub struct PluginCleanupContext<'world> {
    world: &'world mut World,
}

impl PluginCleanupContext<'_> {
    #[must_use]
    pub fn world(&self) -> &World {
        self.world
    }

    #[must_use]
    pub fn world_mut(&mut self) -> &mut World {
        self.world
    }
}

pub trait Plugin: Send + Sync + 'static {
    fn metadata(&self) -> PluginMetadata;

    fn preflight(&self, _app: &App) -> Result<(), PluginError> {
        Ok(())
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError>;

    fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn cleanup(&self, _context: &mut PluginCleanupContext<'_>) -> Result<(), PluginError> {
        Ok(())
    }

    fn plugin_id(&self) -> PluginId {
        self.metadata().id
    }
}

pub trait PluginGroup: Send + Sync + 'static {
    fn metadata(&self) -> PluginGroupMetadata;

    fn preflight(&self, _app: &App) -> Result<(), PluginError> {
        Ok(())
    }

    fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError>;
}

pub struct PluginGroupBuilder<'app> {
    app: &'app mut App,
}

impl PluginGroupBuilder<'_> {
    #[must_use]
    pub fn app(&self) -> &App {
        self.app
    }

    pub fn add_plugin(&mut self, plugin: impl Plugin) -> Result<&mut Self, PluginError> {
        self.app.add_plugin(plugin)?;
        Ok(self)
    }

    pub fn add_plugin_if_missing(&mut self, plugin: impl Plugin) -> Result<&mut Self, PluginError> {
        self.app.add_plugin_if_missing(plugin)?;
        Ok(self)
    }

    pub fn add_plugins(&mut self, group: impl PluginGroup) -> Result<&mut Self, PluginError> {
        self.app.add_plugins(group)?;
        Ok(self)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PluginError {
    #[error("duplicate plugin: {plugin}")]
    Duplicate { plugin: PluginId },
    #[error("plugins cannot be added after plugin finishing has started: {plugin}")]
    AddedAfterFinish { plugin: PluginId },
    #[error("plugin groups cannot be added after plugin finishing has started: {group}")]
    GroupAddedAfterFinish { group: PluginGroupId },
    #[error("duplicate plugin group: {group}")]
    DuplicateGroup { group: PluginGroupId },
    #[error("plugin {plugin} requires missing prerequisite plugin {prerequisite}")]
    MissingPluginPrerequisite {
        plugin: PluginId,
        prerequisite: PluginId,
    },
    #[error("plugin {plugin} requires missing capability {capability}")]
    MissingCapabilityPrerequisite {
        plugin: PluginId,
        capability: PluginCapability,
    },
    #[error("plugin {plugin} conflicts with installed plugin {conflict}")]
    ConflictingPlugin {
        plugin: PluginId,
        conflict: PluginId,
    },
    #[error("plugin dependency cycle while installing {plugin}: {chain:?}")]
    DependencyCycle {
        plugin: PluginId,
        chain: Vec<PluginId>,
    },
    #[error("plugin group dependency cycle while installing {group}: {chain:?}")]
    GroupDependencyCycle {
        group: PluginGroupId,
        chain: Vec<PluginGroupId>,
    },
    #[error("plugin {plugin} failed to initialize: {message}")]
    SetupFailed { plugin: PluginId, message: String },
    #[error("plugin {plugin} failed to register component {component}: {message}")]
    ComponentRegistrationFailed {
        plugin: PluginId,
        component: String,
        message: String,
    },
    #[error("plugin metadata hook panicked")]
    MetadataPanicked,
    #[error("plugin group metadata hook panicked")]
    GroupMetadataPanicked,
    #[error("plugin {plugin} panicked during {hook}")]
    HookPanicked { plugin: PluginId, hook: PluginHook },
    #[error("plugin group {group} panicked during {hook}")]
    GroupHookPanicked {
        group: PluginGroupId,
        hook: PluginHook,
    },
    #[error("app lifecycle is already cleaned")]
    LifecycleCleaned,
    #[error("app plugin lifecycle is poisoned")]
    LifecyclePoisoned,
    #[error("plugin finishing cannot be re-entered")]
    FinishReentered,
}

impl PluginError {
    #[must_use]
    pub fn component_registration(
        plugin: PluginId,
        component: impl Into<String>,
        error: impl Display,
    ) -> Self {
        Self::ComponentRegistrationFailed {
            plugin,
            component: component.into(),
            message: error.to_string(),
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AppRunError {
    #[error("plugin lifecycle failed: {error}")]
    Plugin {
        error: PluginError,
        report: Option<Box<PluginFailureReport>>,
    },
    #[error("app runner failed: {message}")]
    Runner { message: String },
    #[error("app shutdown reported plugin cleanup failures")]
    Shutdown {
        prior: Option<Box<AppRunError>>,
        report: Box<PluginFailureReport>,
    },
}

impl AppRunError {
    #[must_use]
    pub fn plugin(error: PluginError, report: Option<PluginFailureReport>) -> Self {
        Self::Plugin {
            error,
            report: report.map(Box::new),
        }
    }

    #[must_use]
    pub fn runner(message: impl Into<String>) -> Self {
        Self::Runner {
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn plugin_error(&self) -> Option<&PluginError> {
        match self {
            Self::Plugin { error, .. } => Some(error),
            Self::Runner { .. } | Self::Shutdown { .. } => None,
        }
    }

    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        match self {
            Self::Plugin { report, .. } => report.as_deref(),
            Self::Shutdown { report, .. } => Some(report.as_ref()),
            Self::Runner { .. } => None,
        }
    }
}

impl From<PluginError> for AppRunError {
    fn from(error: PluginError) -> Self {
        Self::plugin(error, None)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AppExit {
    #[default]
    Success,
    Requested,
}

pub type RunnerFn = Box<dyn FnOnce(&mut App) -> Result<AppExit, AppRunError>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Resource)]
pub struct RealTime {
    pub delta: Duration,
    pub elapsed: Duration,
    pub frame: u64,
}

impl Default for RealTime {
    fn default() -> Self {
        Self {
            delta: Duration::ZERO,
            elapsed: Duration::ZERO,
            frame: 0,
        }
    }
}

impl RealTime {
    pub fn advance(&mut self, delta: Duration) {
        self.delta = delta;
        self.elapsed = self.elapsed.checked_add(delta).unwrap_or(Duration::MAX);
        self.frame += 1;
    }

    #[must_use]
    pub fn delta_seconds(&self) -> f32 {
        self.delta.as_secs_f32()
    }

    #[must_use]
    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed.as_secs_f64()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Resource)]
pub struct VirtualTime {
    pub delta: Duration,
    pub elapsed: Duration,
    pub frame: u64,
}

impl Default for VirtualTime {
    fn default() -> Self {
        Self {
            delta: Duration::ZERO,
            elapsed: Duration::ZERO,
            frame: 0,
        }
    }
}

impl VirtualTime {
    pub fn advance(&mut self, delta: Duration) {
        self.delta = delta;
        self.elapsed = self.elapsed.checked_add(delta).unwrap_or(Duration::MAX);
        self.frame += 1;
    }

    #[must_use]
    pub fn delta_seconds(&self) -> f32 {
        self.delta.as_secs_f32()
    }

    #[must_use]
    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed.as_secs_f64()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Resource)]
pub struct RuntimeTimeSettings {
    pub paused: bool,
    pub time_scale: f32,
    pub max_delta: Duration,
}

impl Default for RuntimeTimeSettings {
    fn default() -> Self {
        Self {
            paused: false,
            time_scale: 1.0,
            max_delta: Duration::from_millis(250),
        }
    }
}

impl RuntimeTimeSettings {
    #[must_use]
    pub fn with_paused(mut self, paused: bool) -> Self {
        self.paused = paused;
        self
    }

    #[must_use]
    pub fn with_time_scale(mut self, time_scale: f32) -> Self {
        self.time_scale = time_scale;
        self
    }

    #[must_use]
    pub fn with_max_delta(mut self, max_delta: Duration) -> Self {
        self.max_delta = max_delta;
        self
    }

    fn virtual_delta(&self, real_delta: Duration) -> (Duration, bool) {
        let clamped = real_delta.min(self.max_delta);
        let was_clamped = clamped != real_delta;
        if self.paused {
            return (Duration::ZERO, was_clamped);
        }
        let scale = if self.time_scale.is_finite() {
            self.time_scale.max(0.0)
        } else {
            0.0
        };
        (
            Duration::from_secs_f64(clamped.as_secs_f64() * f64::from(scale)),
            was_clamped,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Resource)]
pub struct RenderTime {
    pub interpolation_alpha: f32,
    pub overstep: Duration,
}

impl Default for RenderTime {
    fn default() -> Self {
        Self {
            interpolation_alpha: 0.0,
            overstep: Duration::ZERO,
        }
    }
}

impl RenderTime {
    fn update_from_fixed(&mut self, fixed: &FixedTime) {
        self.overstep = fixed.overstep;
        self.interpolation_alpha = if fixed.timestep.is_zero() {
            0.0
        } else {
            (fixed.overstep.as_secs_f64() / fixed.timestep.as_secs_f64()) as f32
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct FixedTime {
    timestep: Duration,
    max_steps_per_frame: u32,
    accumulated: Duration,
    overstep: Duration,
    steps_this_frame: u32,
    capped_this_frame: bool,
}

impl Default for FixedTime {
    fn default() -> Self {
        Self {
            timestep: Self::DEFAULT_TIMESTEP,
            max_steps_per_frame: Self::DEFAULT_MAX_STEPS_PER_FRAME,
            accumulated: Duration::ZERO,
            overstep: Duration::ZERO,
            steps_this_frame: 0,
            capped_this_frame: false,
        }
    }
}

impl FixedTime {
    pub const DEFAULT_TIMESTEP: Duration = Duration::from_nanos(16_666_667);
    pub const DEFAULT_MAX_STEPS_PER_FRAME: u32 = 5;

    #[must_use]
    pub fn new(timestep: Duration) -> Self {
        Self {
            timestep,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_max_steps_per_frame(mut self, max_steps_per_frame: u32) -> Self {
        self.max_steps_per_frame = max_steps_per_frame;
        self
    }

    #[must_use]
    pub fn timestep(&self) -> Duration {
        self.timestep
    }

    #[must_use]
    pub fn max_steps_per_frame(&self) -> u32 {
        self.max_steps_per_frame
    }

    #[must_use]
    pub fn accumulated(&self) -> Duration {
        self.accumulated
    }

    #[must_use]
    pub fn overstep(&self) -> Duration {
        self.overstep
    }

    #[must_use]
    pub fn steps_this_frame(&self) -> u32 {
        self.steps_this_frame
    }

    #[must_use]
    pub fn capped_this_frame(&self) -> bool {
        self.capped_this_frame
    }

    fn begin_frame(&mut self, delta: Duration) -> u32 {
        self.accumulated = self.accumulated.checked_add(delta).unwrap_or(Duration::MAX);

        let mut steps = 0;
        while self.accumulated >= self.timestep && steps < self.max_steps_per_frame {
            self.accumulated -= self.timestep;
            steps += 1;
        }

        self.steps_this_frame = steps;
        self.overstep = self.accumulated;
        self.capped_this_frame =
            self.accumulated >= self.timestep && steps >= self.max_steps_per_frame;
        steps
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub struct RuntimeFrameStatus {
    pub frame: u64,
    pub real_delta: Duration,
    pub virtual_delta: Duration,
    pub real_delta_clamped: bool,
    pub fixed_steps: u32,
    pub fixed_steps_capped: bool,
    pub fixed_overstep: Duration,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AppFrameOutcome {
    pub exit: Option<AppExit>,
    pub status: RuntimeFrameStatus,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub struct AppExitRequests {
    requested: Option<AppExit>,
}

impl AppExitRequests {
    pub fn request(&mut self, exit: AppExit) {
        self.requested = Some(exit);
    }

    pub fn request_success(&mut self) {
        self.request(AppExit::Success);
    }

    pub fn request_exit(&mut self) {
        self.request(AppExit::Requested);
    }

    #[must_use]
    pub const fn requested(&self) -> Option<AppExit> {
        self.requested
    }

    fn take(&mut self) -> Option<AppExit> {
        self.requested.take()
    }
}

struct InstalledPlugin {
    plugin: Arc<dyn Plugin>,
    metadata: PluginMetadata,
    cleanup_complete: bool,
}

pub struct App {
    world: World,
    startup_schedules: BTreeMap<StartupStage, Schedule>,
    schedules: BTreeMap<CoreStage, Schedule>,
    runner: Option<RunnerFn>,
    plugins: Vec<InstalledPlugin>,
    plugin_install_counts: BTreeMap<PluginId, usize>,
    plugin_metadata: BTreeMap<PluginId, PluginMetadata>,
    provided_capabilities: BTreeSet<PluginCapability>,
    plugin_groups: BTreeMap<PluginGroupId, PluginGroupMetadata>,
    plugin_lifecycle: PluginLifecycleState,
    plugin_failure_report: Option<PluginFailureReport>,
    installing_plugins: Vec<PluginId>,
    installing_plugin_groups: Vec<PluginGroupId>,
    committed_hook_depth: usize,
    started: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.cleanup_plugins_internal();
    }
}

impl App {
    #[must_use]
    pub fn new() -> Self {
        let mut world = World::new();
        world.insert_resource(RealTime::default());
        world.insert_resource(VirtualTime::default());
        world.insert_resource(RuntimeTimeSettings::default());
        world.insert_resource(FixedTime::default());
        world.insert_resource(RenderTime::default());
        world.insert_resource(RuntimeFrameStatus::default());
        world.insert_resource(AppExitRequests::default());

        let startup_schedules = StartupStage::ALL
            .into_iter()
            .map(|stage| (stage, Schedule::new(stage)))
            .collect();
        let schedules = CoreStage::ALL
            .into_iter()
            .map(|stage| (stage, Schedule::new(stage)))
            .collect();

        Self {
            world,
            startup_schedules,
            schedules,
            runner: Some(Box::new(default_runner)),
            plugins: Vec::new(),
            plugin_install_counts: BTreeMap::new(),
            plugin_metadata: BTreeMap::new(),
            provided_capabilities: BTreeSet::new(),
            plugin_groups: BTreeMap::new(),
            plugin_lifecycle: PluginLifecycleState::Configuring,
            plugin_failure_report: None,
            installing_plugins: Vec::new(),
            installing_plugin_groups: Vec::new(),
            committed_hook_depth: 0,
            started: false,
        }
    }

    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    #[must_use]
    pub const fn plugin_lifecycle_state(&self) -> PluginLifecycleState {
        self.plugin_lifecycle
    }

    #[must_use]
    pub const fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.plugin_failure_report.as_ref()
    }

    pub fn world_mut(&mut self) -> Result<&mut World, PluginError> {
        self.ensure_mutation_allowed()?;
        Ok(&mut self.world)
    }

    pub fn insert_resource<R: Resource>(&mut self, resource: R) -> Result<&mut Self, PluginError> {
        self.ensure_mutation_allowed()?;
        self.world.insert_resource(resource);
        Ok(self)
    }

    pub fn init_resource<R>(&mut self) -> Result<&mut Self, PluginError>
    where
        R: Resource + nara_ecs::world::FromWorld,
    {
        self.ensure_mutation_allowed()?;
        self.world.init_resource::<R>();
        Ok(self)
    }

    pub fn add_startup_systems<M>(
        &mut self,
        stage: StartupStage,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_mutation_allowed()?;
        self.startup_schedule_mut(stage).add_systems(systems);
        Ok(self)
    }

    pub fn add_systems<M>(
        &mut self,
        stage: CoreStage,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_mutation_allowed()?;
        self.schedule_mut(stage).add_systems(systems);
        Ok(self)
    }

    pub fn configure_sets<M>(
        &mut self,
        stage: CoreStage,
        sets: impl IntoScheduleConfigs<InternedSystemSet, M>,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_mutation_allowed()?;
        self.schedule_mut(stage).configure_sets(sets);
        Ok(self)
    }

    pub fn set_runner(
        &mut self,
        runner: impl FnOnce(&mut App) -> Result<AppExit, AppRunError> + 'static,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_mutation_allowed()?;
        self.runner = Some(Box::new(runner));
        Ok(self)
    }

    pub fn add_plugin(&mut self, plugin: impl Plugin) -> Result<&mut Self, PluginError> {
        self.add_plugin_internal(plugin, false)
    }

    pub fn add_plugin_if_missing(&mut self, plugin: impl Plugin) -> Result<&mut Self, PluginError> {
        self.add_plugin_internal(plugin, true)
    }

    pub fn add_plugins(&mut self, group: impl PluginGroup) -> Result<&mut Self, PluginError> {
        if self.plugin_lifecycle == PluginLifecycleState::Poisoned {
            return Err(self.primary_plugin_error());
        }
        if self.plugin_lifecycle == PluginLifecycleState::Cleaned
            || self.plugin_lifecycle == PluginLifecycleState::Cleaning
        {
            return Err(PluginError::LifecycleCleaned);
        }

        let metadata = catch_unwind(AssertUnwindSafe(|| group.metadata()))
            .map_err(|_| PluginError::GroupMetadataPanicked)?;
        if self.plugin_lifecycle != PluginLifecycleState::Configuring {
            return Err(PluginError::GroupAddedAfterFinish { group: metadata.id });
        }
        if self.plugin_groups.contains_key(&metadata.id) {
            return Err(PluginError::DuplicateGroup { group: metadata.id });
        }
        if let Some(cycle_start) = self
            .installing_plugin_groups
            .iter()
            .position(|group_id| *group_id == metadata.id)
        {
            let mut chain = self.installing_plugin_groups[cycle_start..].to_vec();
            chain.push(metadata.id);
            return Err(PluginError::GroupDependencyCycle {
                group: metadata.id,
                chain,
            });
        }

        catch_unwind(AssertUnwindSafe(|| group.preflight(self))).map_err(|_| {
            PluginError::GroupHookPanicked {
                group: metadata.id,
                hook: PluginHook::Preflight,
            }
        })??;

        self.installing_plugin_groups.push(metadata.id);
        self.committed_hook_depth += 1;
        let build_result = catch_unwind(AssertUnwindSafe(|| {
            let mut builder = PluginGroupBuilder { app: self };
            group.build(&mut builder)
        }))
        .map_err(|_| PluginError::GroupHookPanicked {
            group: metadata.id,
            hook: PluginHook::Build,
        })
        .and_then(|result| result);
        self.committed_hook_depth -= 1;
        self.installing_plugin_groups.pop();

        if let Err(error) = build_result {
            self.poison(
                PluginFailureSubject::Group(metadata.id),
                PluginHook::Build,
                error,
            );
        }
        self.cleanup_after_outermost_failure();
        if self.plugin_lifecycle == PluginLifecycleState::Poisoned {
            return Err(self.primary_plugin_error());
        }

        self.plugin_groups.insert(metadata.id, metadata);
        Ok(self)
    }

    #[must_use]
    pub fn has_plugin(&self, id: PluginId) -> bool {
        self.plugin_install_counts.contains_key(&id)
    }

    #[must_use]
    pub fn has_capability(&self, capability: PluginCapability) -> bool {
        self.provided_capabilities.contains(&capability)
    }

    pub fn installed_plugins(&self) -> impl Iterator<Item = PluginMetadata> + '_ {
        self.plugin_metadata.values().copied()
    }

    pub fn installed_plugin_groups(&self) -> impl Iterator<Item = PluginGroupMetadata> + '_ {
        self.plugin_groups.values().copied()
    }

    pub fn require_plugin(
        &self,
        plugin: PluginId,
        prerequisite: PluginId,
    ) -> Result<(), PluginError> {
        if self.has_plugin(prerequisite) {
            Ok(())
        } else {
            Err(PluginError::MissingPluginPrerequisite {
                plugin,
                prerequisite,
            })
        }
    }

    pub fn require_capability(
        &self,
        plugin: PluginId,
        capability: PluginCapability,
    ) -> Result<(), PluginError> {
        if self.has_capability(capability) {
            Ok(())
        } else {
            Err(PluginError::MissingCapabilityPrerequisite { plugin, capability })
        }
    }

    fn check_plugin_metadata(&self, metadata: PluginMetadata) -> Result<(), PluginError> {
        for prerequisite in metadata.requires_plugins {
            self.require_plugin(metadata.id, *prerequisite)?;
        }
        for capability in metadata.requires_capabilities {
            self.require_capability(metadata.id, *capability)?;
        }
        for conflict in metadata.conflicts {
            if self.has_plugin(*conflict) {
                return Err(PluginError::ConflictingPlugin {
                    plugin: metadata.id,
                    conflict: *conflict,
                });
            }
        }
        for installed in self.plugin_metadata.values() {
            if installed.conflicts.contains(&metadata.id) {
                return Err(PluginError::ConflictingPlugin {
                    plugin: metadata.id,
                    conflict: installed.id,
                });
            }
        }
        Ok(())
    }

    pub fn finish_plugins(&mut self) -> Result<&mut Self, PluginError> {
        if self.committed_hook_depth != 0 {
            return Err(PluginError::FinishReentered);
        }
        match self.plugin_lifecycle {
            PluginLifecycleState::Ready => return Ok(self),
            PluginLifecycleState::Poisoned => return Err(self.primary_plugin_error()),
            PluginLifecycleState::Cleaning | PluginLifecycleState::Cleaned => {
                return Err(PluginError::LifecycleCleaned);
            }
            PluginLifecycleState::Finishing => {
                return Err(PluginError::FinishReentered);
            }
            PluginLifecycleState::Configuring => {}
        }

        self.plugin_lifecycle = PluginLifecycleState::Finishing;
        for index in 0..self.plugins.len() {
            let plugin = Arc::clone(&self.plugins[index].plugin);
            let plugin_id = self.plugins[index].metadata.id;
            self.committed_hook_depth += 1;
            let result = catch_unwind(AssertUnwindSafe(|| plugin.finish(self)))
                .map_err(|_| PluginError::HookPanicked {
                    plugin: plugin_id,
                    hook: PluginHook::Finish,
                })
                .and_then(|result| result);
            self.committed_hook_depth -= 1;
            if let Err(error) = result {
                self.poison(
                    PluginFailureSubject::Plugin(plugin_id),
                    PluginHook::Finish,
                    error,
                );
                break;
            }
        }

        if self.plugin_lifecycle == PluginLifecycleState::Finishing {
            self.plugin_lifecycle = PluginLifecycleState::Ready;
            return Ok(self);
        }

        self.cleanup_plugins_internal();
        Err(self.primary_plugin_error())
    }

    pub fn cleanup_plugins(&mut self) -> Result<(), PluginCleanupError> {
        if self.committed_hook_depth != 0 {
            return Err(PluginCleanupError::HookActive);
        }
        self.cleanup_plugins_internal();
        if let Some(report) = &self.plugin_failure_report
            && (report.primary.is_some() || !report.cleanup_failures.is_empty())
        {
            return Err(PluginCleanupError::Failure(Box::new(report.clone())));
        }
        Ok(())
    }

    fn add_plugin_internal<P>(
        &mut self,
        plugin: P,
        skip_if_installed: bool,
    ) -> Result<&mut Self, PluginError>
    where
        P: Plugin,
    {
        if self.plugin_lifecycle == PluginLifecycleState::Poisoned {
            return Err(self.primary_plugin_error());
        }
        if self.plugin_lifecycle == PluginLifecycleState::Cleaned
            || self.plugin_lifecycle == PluginLifecycleState::Cleaning
        {
            return Err(PluginError::LifecycleCleaned);
        }

        let metadata = catch_unwind(AssertUnwindSafe(|| plugin.metadata()))
            .map_err(|_| PluginError::MetadataPanicked)?;
        if self.plugin_lifecycle != PluginLifecycleState::Configuring {
            return Err(PluginError::AddedAfterFinish {
                plugin: metadata.id,
            });
        }
        if let Some(cycle_start) = self
            .installing_plugins
            .iter()
            .position(|plugin_id| *plugin_id == metadata.id)
        {
            let mut chain = self.installing_plugins[cycle_start..].to_vec();
            chain.push(metadata.id);
            return Err(PluginError::DependencyCycle {
                plugin: metadata.id,
                chain,
            });
        }
        if metadata.unique && self.has_plugin(metadata.id) {
            if skip_if_installed {
                return Ok(self);
            }
            return Err(PluginError::Duplicate {
                plugin: metadata.id,
            });
        }

        self.check_plugin_metadata(metadata)?;
        catch_unwind(AssertUnwindSafe(|| plugin.preflight(self))).map_err(|_| {
            PluginError::HookPanicked {
                plugin: metadata.id,
                hook: PluginHook::Preflight,
            }
        })??;

        let plugin: Arc<dyn Plugin> = Arc::new(plugin);
        let committed_index = self.plugins.len();
        self.plugins.push(InstalledPlugin {
            plugin: Arc::clone(&plugin),
            metadata,
            cleanup_complete: false,
        });
        self.installing_plugins.push(metadata.id);
        self.committed_hook_depth += 1;

        let build_result = catch_unwind(AssertUnwindSafe(|| plugin.build(self)))
            .map_err(|_| PluginError::HookPanicked {
                plugin: metadata.id,
                hook: PluginHook::Build,
            })
            .and_then(|result| result);

        self.committed_hook_depth -= 1;
        self.installing_plugins.pop();
        let committed = self.plugins.remove(committed_index);
        self.plugins.push(committed);

        if let Err(error) = build_result {
            self.poison(
                PluginFailureSubject::Plugin(metadata.id),
                PluginHook::Build,
                error,
            );
        }

        if self.plugin_lifecycle != PluginLifecycleState::Poisoned {
            *self.plugin_install_counts.entry(metadata.id).or_default() += 1;
            self.plugin_metadata.entry(metadata.id).or_insert(metadata);
            for capability in metadata.provides {
                self.provided_capabilities.insert(*capability);
            }
        }

        self.cleanup_after_outermost_failure();
        if self.plugin_lifecycle == PluginLifecycleState::Poisoned {
            return Err(self.primary_plugin_error());
        }
        Ok(self)
    }

    fn ensure_mutation_allowed(&self) -> Result<(), PluginError> {
        match self.plugin_lifecycle {
            PluginLifecycleState::Poisoned => Err(self.primary_plugin_error()),
            PluginLifecycleState::Cleaning | PluginLifecycleState::Cleaned => {
                Err(PluginError::LifecycleCleaned)
            }
            PluginLifecycleState::Configuring
            | PluginLifecycleState::Finishing
            | PluginLifecycleState::Ready => Ok(()),
        }
    }

    fn primary_plugin_error(&self) -> PluginError {
        self.plugin_failure_report
            .as_ref()
            .and_then(|report| report.primary.as_ref())
            .map(|failure| failure.error.clone())
            .unwrap_or(PluginError::LifecyclePoisoned)
    }

    fn poison(&mut self, subject: PluginFailureSubject, hook: PluginHook, error: PluginError) {
        let report = self
            .plugin_failure_report
            .get_or_insert_with(|| PluginFailureReport {
                primary: None,
                cleanup_failures: Vec::new(),
                cleanup_complete: false,
            });
        if report.primary.is_none() {
            report.primary = Some(PluginFailure {
                subject,
                hook,
                error,
            });
        }
        self.plugin_lifecycle = PluginLifecycleState::Poisoned;
    }

    fn cleanup_after_outermost_failure(&mut self) {
        if self.committed_hook_depth == 0 && self.plugin_lifecycle == PluginLifecycleState::Poisoned
        {
            self.cleanup_plugins_internal();
        }
    }

    fn cleanup_plugins_internal(&mut self) {
        if self.plugin_lifecycle == PluginLifecycleState::Cleaning {
            return;
        }

        let preserve_poisoned = self.plugin_lifecycle == PluginLifecycleState::Poisoned
            || self
                .plugin_failure_report
                .as_ref()
                .is_some_and(|report| report.primary.is_some());
        self.plugin_lifecycle = PluginLifecycleState::Cleaning;

        for index in (0..self.plugins.len()).rev() {
            if self.plugins[index].cleanup_complete {
                continue;
            }

            self.plugins[index].cleanup_complete = true;
            let plugin = Arc::clone(&self.plugins[index].plugin);
            let plugin_id = self.plugins[index].metadata.id;
            let result = {
                let mut context = PluginCleanupContext {
                    world: &mut self.world,
                };
                catch_unwind(AssertUnwindSafe(|| plugin.cleanup(&mut context)))
                    .map_err(|_| PluginError::HookPanicked {
                        plugin: plugin_id,
                        hook: PluginHook::Cleanup,
                    })
                    .and_then(|result| result)
            };
            if let Err(error) = result {
                let report =
                    self.plugin_failure_report
                        .get_or_insert_with(|| PluginFailureReport {
                            primary: None,
                            cleanup_failures: Vec::new(),
                            cleanup_complete: false,
                        });
                report.cleanup_failures.push(PluginFailure {
                    subject: PluginFailureSubject::Plugin(plugin_id),
                    hook: PluginHook::Cleanup,
                    error,
                });
            }
        }

        if let Some(report) = &mut self.plugin_failure_report {
            report.cleanup_complete = true;
        }
        self.plugin_lifecycle = if preserve_poisoned {
            PluginLifecycleState::Poisoned
        } else {
            PluginLifecycleState::Cleaned
        };
    }

    pub fn run(mut self) -> Result<AppExit, AppRunError> {
        if let Err(error) = self.finish_plugins() {
            return Err(AppRunError::plugin(
                error,
                self.plugin_failure_report.clone(),
            ));
        }
        let runner = self
            .runner
            .take()
            .unwrap_or_else(|| Box::new(default_runner));
        let run_result = runner(&mut self);
        match self.cleanup_plugins() {
            Ok(()) => run_result,
            Err(PluginCleanupError::Failure(report)) => Err(AppRunError::Shutdown {
                prior: run_result.err().map(Box::new),
                report,
            }),
            Err(PluginCleanupError::HookActive) => match run_result {
                Err(error) => Err(error),
                Ok(_) => Err(AppRunError::runner(
                    "runner returned while a plugin hook was still active",
                )),
            },
        }
    }

    pub fn run_once(&mut self, real_delta: Duration) -> Result<AppFrameOutcome, AppRunError> {
        if let Err(error) = self.finish_plugins() {
            return Err(AppRunError::plugin(
                error,
                self.plugin_failure_report.clone(),
            ));
        }

        if !self.started {
            for stage in StartupStage::ALL {
                if let Some(schedule) = self.startup_schedules.get_mut(&stage) {
                    schedule.run(&mut self.world);
                }
            }
            self.started = true;
        }

        self.world.resource_mut::<RealTime>().advance(real_delta);
        let settings = *self.world.resource::<RuntimeTimeSettings>();
        let (virtual_delta, real_delta_clamped) = settings.virtual_delta(real_delta);
        self.world
            .resource_mut::<VirtualTime>()
            .advance(virtual_delta);
        let fixed_steps = self
            .world
            .resource_mut::<FixedTime>()
            .begin_frame(virtual_delta);
        let fixed = *self.world.resource::<FixedTime>();
        self.world
            .resource_mut::<RenderTime>()
            .update_from_fixed(&fixed);

        for stage in CoreStage::ALL {
            if let Some(schedule) = self.schedules.get_mut(&stage) {
                if stage == CoreStage::FixedUpdate {
                    for _ in 0..fixed_steps {
                        schedule.run(&mut self.world);
                    }
                } else {
                    schedule.run(&mut self.world);
                }
            }
        }

        let fixed = *self.world.resource::<FixedTime>();
        let status = RuntimeFrameStatus {
            frame: self.world.resource::<RealTime>().frame,
            real_delta,
            virtual_delta,
            real_delta_clamped,
            fixed_steps,
            fixed_steps_capped: fixed.capped_this_frame(),
            fixed_overstep: fixed.overstep(),
        };
        *self.world.resource_mut::<RuntimeFrameStatus>() = status;
        let exit = self.world.resource_mut::<AppExitRequests>().take();
        Ok(AppFrameOutcome { exit, status })
    }

    pub fn update(&mut self) -> Result<AppFrameOutcome, AppRunError> {
        self.run_once(Duration::ZERO)
    }

    fn startup_schedule_mut(&mut self, stage: StartupStage) -> &mut Schedule {
        self.startup_schedules
            .entry(stage)
            .or_insert_with(|| Schedule::new(stage))
    }

    fn schedule_mut(&mut self, stage: CoreStage) -> &mut Schedule {
        self.schedules
            .entry(stage)
            .or_insert_with(|| Schedule::new(stage))
    }
}

fn default_runner(app: &mut App) -> Result<AppExit, AppRunError> {
    let outcome = app.run_once(Duration::ZERO)?;
    Ok(outcome.exit.unwrap_or(AppExit::Success))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_ecs::{Commands, Component, ResMut, Resource};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    #[derive(Debug, Default, Resource)]
    struct Frames(u32);

    #[derive(Debug, Component)]
    struct Spawned;

    #[derive(Debug, Default, Resource)]
    struct Order(Vec<&'static str>);

    #[derive(Debug, Default, Resource)]
    struct PluginBuildCount(u32);

    const COUNTING_PLUGIN_ID: PluginId = PluginId::new("nara.test.counting");
    const FAILING_PLUGIN_ID: PluginId = PluginId::new("nara.test.failing");
    const MISSING_PLUGIN_ID: PluginId = PluginId::new("nara.test.missing");
    const COUNTING_CAPABILITY: PluginCapability = PluginCapability::new("nara.test.counting");
    const CAPABILITY_PLUGIN_ID: PluginId = PluginId::new("nara.test.capability");
    const COUNTING_GROUP_ID: PluginGroupId = PluginGroupId::new("nara.test.group");
    const COMMITTED_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.committed_failure");
    const FINISH_A_PLUGIN_ID: PluginId = PluginId::new("nara.test.finish_a");
    const FINISH_B_PLUGIN_ID: PluginId = PluginId::new("nara.test.finish_b");
    const FINISH_C_PLUGIN_ID: PluginId = PluginId::new("nara.test.finish_c");
    const PREFLIGHT_PLUGIN_ID: PluginId = PluginId::new("nara.test.preflight");
    const PROBE_A_PLUGIN_ID: PluginId = PluginId::new("nara.test.probe_a");
    const PROBE_B_PLUGIN_ID: PluginId = PluginId::new("nara.test.probe_b");
    const PROBE_C_PLUGIN_ID: PluginId = PluginId::new("nara.test.probe_c");
    const CYCLE_A_PLUGIN_ID: PluginId = PluginId::new("nara.test.cycle_a");
    const CYCLE_B_PLUGIN_ID: PluginId = PluginId::new("nara.test.cycle_b");
    const NESTED_PLUGIN_ID: PluginId = PluginId::new("nara.test.nested");
    const IGNORE_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.ignore_failure");
    const PARTIAL_GROUP_ID: PluginGroupId = PluginGroupId::new("nara.test.partial_group");
    const CYCLE_A_GROUP_ID: PluginGroupId = PluginGroupId::new("nara.test.cycle_a_group");
    const CYCLE_B_GROUP_ID: PluginGroupId = PluginGroupId::new("nara.test.cycle_b_group");
    const CONFLICT_A_PLUGIN_ID: PluginId = PluginId::new("nara.test.conflict_a");
    const CONFLICT_B_PLUGIN_ID: PluginId = PluginId::new("nara.test.conflict_b");

    #[derive(Debug, Default, Clone, Copy)]
    struct CountingPlugin;

    impl Plugin for CountingPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(COUNTING_PLUGIN_ID, PluginCategory::Core)
                .provides(&[COUNTING_CAPABILITY])
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            if !app.world().contains_resource::<PluginBuildCount>() {
                app.insert_resource(PluginBuildCount::default())?;
            }
            app.world_mut()?.resource_mut::<PluginBuildCount>().0 += 1;
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct FailingPlugin;

    impl Plugin for FailingPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(FAILING_PLUGIN_ID, PluginCategory::Core)
                .requires_plugins(&[MISSING_PLUGIN_ID])
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.require_plugin(self.plugin_id(), MISSING_PLUGIN_ID)
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct CapabilityPlugin;

    impl Plugin for CapabilityPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(CAPABILITY_PLUGIN_ID, PluginCategory::Core)
                .requires_capabilities(&[COUNTING_CAPABILITY])
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct ConflictAPlugin;

    impl Plugin for ConflictAPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(CONFLICT_A_PLUGIN_ID, PluginCategory::Core)
                .conflicts(&[CONFLICT_B_PLUGIN_ID])
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct ConflictBPlugin;

    impl Plugin for ConflictBPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(CONFLICT_B_PLUGIN_ID, PluginCategory::Core)
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct CountingGroup;

    impl PluginGroup for CountingGroup {
        fn metadata(&self) -> PluginGroupMetadata {
            PluginGroupMetadata::new(COUNTING_GROUP_ID, &[COUNTING_PLUGIN_ID])
        }

        fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
            group.add_plugin_if_missing(CountingPlugin)?;
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct CommittedFailurePlugin;

    impl Plugin for CommittedFailurePlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(COMMITTED_FAILURE_PLUGIN_ID, PluginCategory::Core)
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.insert_resource(PluginBuildCount(41))?;
            Err(PluginError::SetupFailed {
                plugin: COMMITTED_FAILURE_PLUGIN_ID,
                message: "committed build failed".into(),
            })
        }
    }

    #[derive(Clone)]
    struct FinishOrderPlugin {
        id: PluginId,
        cleanup_order: Arc<Mutex<Vec<PluginId>>>,
        fail_finish: bool,
    }

    impl Plugin for FinishOrderPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(self.id, PluginCategory::Core)
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            Ok(())
        }

        fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
            if self.fail_finish {
                Err(PluginError::SetupFailed {
                    plugin: self.id,
                    message: "finish failed".into(),
                })
            } else {
                Ok(())
            }
        }

        fn cleanup(&self, _context: &mut PluginCleanupContext<'_>) -> Result<(), PluginError> {
            self.cleanup_order.lock().unwrap().push(self.id);
            Ok(())
        }
    }

    #[derive(Clone)]
    struct RetryablePreflightPlugin {
        allowed: Arc<AtomicBool>,
    }

    impl Plugin for RetryablePreflightPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(PREFLIGHT_PLUGIN_ID, PluginCategory::Core)
        }

        fn preflight(&self, _app: &App) -> Result<(), PluginError> {
            if self.allowed.load(Ordering::SeqCst) {
                Ok(())
            } else {
                Err(PluginError::SetupFailed {
                    plugin: PREFLIGHT_PLUGIN_ID,
                    message: "preflight rejected".into(),
                })
            }
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.insert_resource(PluginBuildCount(7))?;
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct ProbeBehavior {
        panic_preflight: bool,
        panic_build: bool,
        fail_finish: bool,
        panic_finish: bool,
        fail_cleanup: bool,
        panic_cleanup: bool,
    }

    #[derive(Clone)]
    struct LifecycleProbePlugin {
        id: PluginId,
        behavior: ProbeBehavior,
        trace: Arc<Mutex<Vec<(PluginId, PluginHook)>>>,
    }

    impl Plugin for LifecycleProbePlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(self.id, PluginCategory::Core)
        }

        fn preflight(&self, _app: &App) -> Result<(), PluginError> {
            assert!(!self.behavior.panic_preflight, "preflight panic");
            Ok(())
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            self.trace
                .lock()
                .unwrap()
                .push((self.id, PluginHook::Build));
            assert!(!self.behavior.panic_build, "build panic");
            Ok(())
        }

        fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
            self.trace
                .lock()
                .unwrap()
                .push((self.id, PluginHook::Finish));
            assert!(!self.behavior.panic_finish, "finish panic");
            if self.behavior.fail_finish {
                Err(PluginError::SetupFailed {
                    plugin: self.id,
                    message: "finish rejected".into(),
                })
            } else {
                Ok(())
            }
        }

        fn cleanup(&self, _context: &mut PluginCleanupContext<'_>) -> Result<(), PluginError> {
            self.trace
                .lock()
                .unwrap()
                .push((self.id, PluginHook::Cleanup));
            assert!(!self.behavior.panic_cleanup, "cleanup panic");
            if self.behavior.fail_cleanup {
                Err(PluginError::SetupFailed {
                    plugin: self.id,
                    message: "cleanup rejected".into(),
                })
            } else {
                Ok(())
            }
        }
    }

    #[derive(Clone)]
    struct NestedProbePlugin {
        trace: Arc<Mutex<Vec<(PluginId, PluginHook)>>>,
    }

    impl Plugin for NestedProbePlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(NESTED_PLUGIN_ID, PluginCategory::Core)
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.add_plugin(LifecycleProbePlugin {
                id: PROBE_B_PLUGIN_ID,
                behavior: ProbeBehavior::default(),
                trace: Arc::clone(&self.trace),
            })?;
            self.trace
                .lock()
                .unwrap()
                .push((NESTED_PLUGIN_ID, PluginHook::Build));
            Ok(())
        }

        fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
            self.trace
                .lock()
                .unwrap()
                .push((NESTED_PLUGIN_ID, PluginHook::Finish));
            Ok(())
        }

        fn cleanup(&self, _context: &mut PluginCleanupContext<'_>) -> Result<(), PluginError> {
            self.trace
                .lock()
                .unwrap()
                .push((NESTED_PLUGIN_ID, PluginHook::Cleanup));
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct IgnoreNestedFailurePlugin;

    impl Plugin for IgnoreNestedFailurePlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(IGNORE_FAILURE_PLUGIN_ID, PluginCategory::Core)
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            assert!(app.add_plugin(CommittedFailurePlugin).is_err());
            let Err(error) = app.insert_resource(Frames::default()) else {
                panic!("nested failure must poison before outer build continues");
            };
            assert_eq!(
                error,
                PluginError::SetupFailed {
                    plugin: COMMITTED_FAILURE_PLUGIN_ID,
                    message: "committed build failed".into(),
                }
            );
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct LifecycleReentryPlugin;

    impl Plugin for LifecycleReentryPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(PROBE_C_PLUGIN_ID, PluginCategory::Core)
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            let Err(finish_error) = app.finish_plugins() else {
                panic!("finish reentry should be rejected");
            };
            assert_eq!(finish_error, PluginError::FinishReentered);
            assert_eq!(
                app.cleanup_plugins().unwrap_err(),
                PluginCleanupError::HookActive
            );
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct FinishReentryPlugin;

    impl Plugin for FinishReentryPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(PROBE_B_PLUGIN_ID, PluginCategory::Core)
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            Ok(())
        }

        fn finish(&self, app: &mut App) -> Result<(), PluginError> {
            let Err(finish_error) = app.finish_plugins() else {
                panic!("finish hook reentry should be rejected");
            };
            assert_eq!(finish_error, PluginError::FinishReentered);
            assert_eq!(
                app.cleanup_plugins().unwrap_err(),
                PluginCleanupError::HookActive
            );
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct CycleAPlugin;

    impl Plugin for CycleAPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(CYCLE_A_PLUGIN_ID, PluginCategory::Core)
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.add_plugin_if_missing(CycleBPlugin)?;
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct CycleBPlugin;

    impl Plugin for CycleBPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(CYCLE_B_PLUGIN_ID, PluginCategory::Core)
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.add_plugin_if_missing(CycleAPlugin)?;
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct PartialGroup;

    impl PluginGroup for PartialGroup {
        fn metadata(&self) -> PluginGroupMetadata {
            PluginGroupMetadata::new(
                PARTIAL_GROUP_ID,
                &[COUNTING_PLUGIN_ID, COMMITTED_FAILURE_PLUGIN_ID],
            )
        }

        fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
            group.add_plugin(CountingPlugin)?;
            group.add_plugin(CommittedFailurePlugin)?;
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct CycleAGroup;

    impl PluginGroup for CycleAGroup {
        fn metadata(&self) -> PluginGroupMetadata {
            PluginGroupMetadata::new(CYCLE_A_GROUP_ID, &[])
        }

        fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
            group.add_plugins(CycleBGroup)?;
            Ok(())
        }
    }

    #[derive(Debug, Default, Clone, Copy)]
    struct CycleBGroup;

    impl PluginGroup for CycleBGroup {
        fn metadata(&self) -> PluginGroupMetadata {
            PluginGroupMetadata::new(CYCLE_B_GROUP_ID, &[])
        }

        fn build(&self, group: &mut PluginGroupBuilder<'_>) -> Result<(), PluginError> {
            group.add_plugins(CycleAGroup)?;
            Ok(())
        }
    }

    fn spawn_entity(mut commands: Commands) {
        commands.spawn(Spawned);
    }

    fn count_frame(mut frames: ResMut<Frames>) {
        frames.0 += 1;
    }

    fn push_first(mut order: ResMut<Order>) {
        order.0.push("first");
    }

    fn push_task_update(mut order: ResMut<Order>) {
        order.0.push("task_update");
    }

    fn push_pre_update(mut order: ResMut<Order>) {
        order.0.push("pre_update");
    }

    fn push_fixed_update(mut order: ResMut<Order>) {
        order.0.push("fixed_update");
    }

    fn push_update(mut order: ResMut<Order>) {
        order.0.push("update");
    }

    fn push_extract(mut order: ResMut<Order>) {
        order.0.push("extract");
    }

    fn push_prepare(mut order: ResMut<Order>) {
        order.0.push("prepare");
    }

    fn push_queue(mut order: ResMut<Order>) {
        order.0.push("queue");
    }

    fn push_sort(mut order: ResMut<Order>) {
        order.0.push("sort");
    }

    fn push_render(mut order: ResMut<Order>) {
        order.0.push("render");
    }

    fn push_cleanup(mut order: ResMut<Order>) {
        order.0.push("cleanup");
    }

    fn push_last(mut order: ResMut<Order>) {
        order.0.push("last");
    }

    fn push_task_poll(mut order: ResMut<Order>) {
        order.0.push("task_poll");
    }

    fn push_task_coalesce(mut order: ResMut<Order>) {
        order.0.push("task_coalesce");
    }

    fn push_task_spawn(mut order: ResMut<Order>) {
        order.0.push("task_spawn");
    }

    fn push_task_apply(mut order: ResMut<Order>) {
        order.0.push("task_apply");
    }

    fn request_exit(mut requests: ResMut<AppExitRequests>) {
        requests.request_exit();
    }

    #[test]
    fn update_runs_startup_once_and_update_every_frame() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.add_startup_systems(StartupStage::Core, spawn_entity)
            .unwrap();
        app.add_systems(CoreStage::Update, (spawn_entity, count_frame))
            .unwrap();

        app.update().unwrap();
        app.update().unwrap();

        let spawned_count = {
            let world = app.world_mut().unwrap();
            let mut query = world.query::<&Spawned>();
            query.iter(world).count()
        };

        assert_eq!(spawned_count, 3);
        assert_eq!(app.world().resource::<Frames>().0, 2);
    }

    #[test]
    fn run_once_advances_time_and_runs_fixed_update_when_due() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();

        let outcome = app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        let real_time = app.world().resource::<RealTime>();
        let virtual_time = app.world().resource::<VirtualTime>();
        assert_eq!(real_time.frame, 1);
        assert_eq!(real_time.delta, FixedTime::DEFAULT_TIMESTEP);
        assert_eq!(virtual_time.delta, FixedTime::DEFAULT_TIMESTEP);
        assert_eq!(app.world().resource::<Frames>().0, 1);
        assert_eq!(app.world().resource::<FixedTime>().steps_this_frame(), 1);
        assert_eq!(outcome.status.fixed_steps, 1);
        assert_eq!(outcome.exit, None);
    }

    #[test]
    fn paused_frame_advances_real_time_but_not_virtual_or_fixed_time() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.insert_resource(RuntimeTimeSettings::default().with_paused(true))
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();

        let outcome = app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            app.world().resource::<RealTime>().delta,
            FixedTime::DEFAULT_TIMESTEP
        );
        assert_eq!(app.world().resource::<VirtualTime>().delta, Duration::ZERO);
        assert_eq!(app.world().resource::<Frames>().0, 0);
        assert_eq!(outcome.status.fixed_steps, 0);
    }

    #[test]
    fn time_scale_changes_virtual_delta_and_fixed_accumulation() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.insert_resource(RuntimeTimeSettings::default().with_time_scale(0.5))
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();

        let outcome = app.run_once(FixedTime::DEFAULT_TIMESTEP * 2).unwrap();

        assert_eq!(
            app.world().resource::<VirtualTime>().delta,
            FixedTime::DEFAULT_TIMESTEP
        );
        assert_eq!(app.world().resource::<Frames>().0, 1);
        assert_eq!(outcome.status.fixed_steps, 1);
    }

    #[test]
    fn max_delta_clamps_large_real_elapsed_time() {
        let mut app = App::new();
        app.insert_resource(
            RuntimeTimeSettings::default().with_max_delta(Duration::from_millis(1)),
        )
        .unwrap();

        let outcome = app.run_once(Duration::from_secs(1)).unwrap();

        assert_eq!(
            app.world().resource::<RealTime>().delta,
            Duration::from_secs(1)
        );
        assert_eq!(
            app.world().resource::<VirtualTime>().delta,
            Duration::from_millis(1)
        );
        assert!(outcome.status.real_delta_clamped);
    }

    #[test]
    fn fixed_update_does_not_run_until_accumulator_is_due() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP / 2).unwrap();

        assert_eq!(app.world().resource::<Frames>().0, 0);
        assert_eq!(app.world().resource::<FixedTime>().steps_this_frame(), 0);
    }

    #[test]
    fn fixed_update_limits_catch_up_ticks() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.insert_resource(FixedTime::default().with_max_steps_per_frame(2))
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP * 5).unwrap();

        assert_eq!(app.world().resource::<Frames>().0, 2);
        assert_eq!(app.world().resource::<FixedTime>().steps_this_frame(), 2);
        assert!(app.world().resource::<FixedTime>().accumulated() >= FixedTime::DEFAULT_TIMESTEP);
        assert!(app.world().resource::<FixedTime>().capped_this_frame());
        assert!(
            app.world()
                .resource::<RuntimeFrameStatus>()
                .fixed_steps_capped
        );
    }

    #[test]
    fn app_exit_request_is_reported_in_frame_outcome() {
        let mut app = App::new();
        app.add_systems(CoreStage::Update, request_exit).unwrap();

        let outcome = app.run_once(Duration::ZERO).unwrap();

        assert_eq!(outcome.exit, Some(AppExit::Requested));
        assert_eq!(app.world().resource::<AppExitRequests>().requested(), None);
    }

    #[test]
    fn first_pre_fixed_and_update_run_in_order() {
        let mut app = App::new();
        app.insert_resource(Order::default()).unwrap();
        app.add_systems(CoreStage::First, push_first).unwrap();
        app.add_systems(CoreStage::TaskUpdate, push_task_update)
            .unwrap();
        app.add_systems(CoreStage::PreUpdate, push_pre_update)
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, push_fixed_update)
            .unwrap();
        app.add_systems(CoreStage::Update, push_update).unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            app.world().resource::<Order>().0,
            [
                "first",
                "task_update",
                "pre_update",
                "fixed_update",
                "update"
            ]
        );
    }

    #[test]
    fn task_update_sets_run_in_order() {
        let mut app = App::new();
        app.insert_resource(Order::default()).unwrap();
        app.configure_sets(
            CoreStage::TaskUpdate,
            (
                TaskUpdateSet::Poll,
                TaskUpdateSet::CoalesceAssetChanges,
                TaskUpdateSet::SpawnAssetJobs,
                TaskUpdateSet::ApplyAssetResults,
            )
                .chain(),
        )
        .unwrap();
        app.add_systems(
            CoreStage::TaskUpdate,
            push_task_apply.in_set(TaskUpdateSet::ApplyAssetResults),
        )
        .unwrap();
        app.add_systems(
            CoreStage::TaskUpdate,
            push_task_spawn.in_set(TaskUpdateSet::SpawnAssetJobs),
        )
        .unwrap();
        app.add_systems(
            CoreStage::TaskUpdate,
            push_task_poll.in_set(TaskUpdateSet::Poll),
        )
        .unwrap();
        app.add_systems(
            CoreStage::TaskUpdate,
            push_task_coalesce.in_set(TaskUpdateSet::CoalesceAssetChanges),
        )
        .unwrap();

        app.run_once(Duration::ZERO).unwrap();

        assert_eq!(
            app.world().resource::<Order>().0,
            ["task_poll", "task_coalesce", "task_spawn", "task_apply"]
        );
    }

    #[test]
    fn render_pipeline_stages_run_in_order() {
        let mut app = App::new();
        app.insert_resource(Order::default()).unwrap();
        app.add_systems(CoreStage::Extract, push_extract).unwrap();
        app.add_systems(CoreStage::Prepare, push_prepare).unwrap();
        app.add_systems(CoreStage::Queue, push_queue).unwrap();
        app.add_systems(CoreStage::Sort, push_sort).unwrap();
        app.add_systems(CoreStage::Render, push_render).unwrap();
        app.add_systems(CoreStage::Cleanup, push_cleanup).unwrap();
        app.add_systems(CoreStage::Last, push_last).unwrap();

        app.run_once(Duration::ZERO).unwrap();

        assert_eq!(
            app.world().resource::<Order>().0,
            [
                "extract", "prepare", "queue", "sort", "render", "cleanup", "last"
            ]
        );
    }

    #[test]
    fn run_consumes_app_with_custom_runner() {
        let mut app = App::new();
        app.set_runner(|app| {
            app.run_once(Duration::ZERO)?;
            Ok(AppExit::Requested)
        })
        .unwrap();

        assert_eq!(app.run().unwrap(), AppExit::Requested);
    }

    #[test]
    fn runner_failure_is_reported_without_panic() {
        let mut app = App::new();
        app.set_runner(|_app| Err(AppRunError::runner("window creation failed")))
            .unwrap();

        assert_eq!(
            app.run().unwrap_err(),
            AppRunError::runner("window creation failed")
        );
    }

    #[test]
    fn run_preserves_runner_error_when_cleanup_also_fails() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new();
        app.add_plugin(LifecycleProbePlugin {
            id: PROBE_A_PLUGIN_ID,
            behavior: ProbeBehavior {
                fail_cleanup: true,
                ..ProbeBehavior::default()
            },
            trace: Arc::clone(&trace),
        })
        .unwrap();
        app.set_runner(|_app| Err(AppRunError::runner("runner failed")))
            .unwrap();

        let error = app.run().unwrap_err();

        let AppRunError::Shutdown { prior, report } = error else {
            panic!("runner and cleanup failures should be returned together");
        };
        assert_eq!(
            prior.as_deref(),
            Some(&AppRunError::runner("runner failed"))
        );
        assert!(report.primary().is_none());
        assert_eq!(report.cleanup_failures().len(), 1);
        assert_eq!(
            trace
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, hook)| *hook == PluginHook::Cleanup)
                .count(),
            1
        );
    }

    #[test]
    fn failed_plugin_build_is_reported_without_registering_plugin() {
        let mut app = App::new();

        let Err(error) = app.add_plugin(FailingPlugin) else {
            panic!("failing plugin should return an installation error");
        };
        assert_eq!(
            error,
            PluginError::MissingPluginPrerequisite {
                plugin: FAILING_PLUGIN_ID,
                prerequisite: MISSING_PLUGIN_ID
            }
        );
        assert!(!app.has_plugin(FAILING_PLUGIN_ID));
    }

    #[test]
    fn add_plugin_if_missing_skips_duplicate_without_rebuilding() {
        let mut app = App::new();

        app.add_plugin_if_missing(CountingPlugin).unwrap();
        app.add_plugin_if_missing(CountingPlugin).unwrap();

        assert!(app.has_plugin(COUNTING_PLUGIN_ID));
        assert_eq!(app.world().resource::<PluginBuildCount>().0, 1);
    }

    #[test]
    fn add_plugin_rejects_duplicate_stable_plugin_id() {
        let mut app = App::new();

        app.add_plugin(CountingPlugin).unwrap();
        let Err(error) = app.add_plugin(CountingPlugin) else {
            panic!("duplicate stable plugin id should be rejected");
        };

        assert_eq!(
            error,
            PluginError::Duplicate {
                plugin: COUNTING_PLUGIN_ID
            }
        );
        assert_eq!(app.world().resource::<PluginBuildCount>().0, 1);
    }

    #[test]
    fn plugin_requirements_can_target_capabilities() {
        let mut app = App::new();

        let Err(error) = app.add_plugin(CapabilityPlugin) else {
            panic!("capability plugin should require missing capability");
        };
        assert_eq!(
            error,
            PluginError::MissingCapabilityPrerequisite {
                plugin: CAPABILITY_PLUGIN_ID,
                capability: COUNTING_CAPABILITY,
            }
        );

        app.add_plugin(CountingPlugin).unwrap();
        app.add_plugin(CapabilityPlugin).unwrap();

        assert!(app.has_capability(COUNTING_CAPABILITY));
        assert!(app.has_plugin(CAPABILITY_PLUGIN_ID));
    }

    #[test]
    fn plugin_conflicts_are_rejected_independent_of_install_order() {
        let mut declared_first = App::new();
        declared_first.add_plugin(ConflictAPlugin).unwrap();
        let Err(reverse_error) = declared_first.add_plugin(ConflictBPlugin) else {
            panic!("installed plugin conflict declaration must reject a later plugin");
        };
        assert_eq!(
            reverse_error,
            PluginError::ConflictingPlugin {
                plugin: CONFLICT_B_PLUGIN_ID,
                conflict: CONFLICT_A_PLUGIN_ID,
            }
        );
        assert_eq!(
            declared_first.plugin_lifecycle_state(),
            PluginLifecycleState::Configuring
        );

        let mut declared_second = App::new();
        declared_second.add_plugin(ConflictBPlugin).unwrap();
        let Err(forward_error) = declared_second.add_plugin(ConflictAPlugin) else {
            panic!("new plugin conflict declaration must reject an installed plugin");
        };
        assert_eq!(
            forward_error,
            PluginError::ConflictingPlugin {
                plugin: CONFLICT_A_PLUGIN_ID,
                conflict: CONFLICT_B_PLUGIN_ID,
            }
        );
        assert_eq!(
            declared_second.plugin_lifecycle_state(),
            PluginLifecycleState::Configuring
        );
    }

    #[test]
    fn plugin_groups_are_recorded_with_stable_membership() {
        let mut app = App::new();

        app.add_plugins(CountingGroup).unwrap();

        assert!(app.has_plugin(COUNTING_PLUGIN_ID));
        let groups = app.installed_plugin_groups().collect::<Vec<_>>();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, COUNTING_GROUP_ID);
        assert_eq!(groups[0].plugins, &[COUNTING_PLUGIN_ID]);
    }

    #[test]
    fn add_plugin_if_missing_rejects_install_after_finish() {
        let mut app = App::new();
        app.finish_plugins().unwrap();

        let Err(error) = app.add_plugin_if_missing(CountingPlugin) else {
            panic!("installing after finish should return an error");
        };
        assert_eq!(
            error,
            PluginError::AddedAfterFinish {
                plugin: COUNTING_PLUGIN_ID
            }
        );
    }

    #[test]
    fn committed_build_failure_prevents_later_frame_execution() {
        let mut app = App::new();

        let Err(expected) = app.add_plugin(CommittedFailurePlugin) else {
            panic!("committed build failure should be reported");
        };

        assert_eq!(
            expected,
            PluginError::SetupFailed {
                plugin: COMMITTED_FAILURE_PLUGIN_ID,
                message: "committed build failed".into(),
            }
        );
        let run_error = app.run_once(Duration::ZERO).unwrap_err();
        assert_eq!(run_error.plugin_error(), Some(&expected));
        assert!(run_error.plugin_failure_report().is_some());
    }

    #[test]
    fn finish_failure_retains_reverse_once_only_cleanup() {
        let cleanup_order = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.add_systems(CoreStage::Update, count_frame).unwrap();
        for (id, fail_finish) in [
            (FINISH_A_PLUGIN_ID, false),
            (FINISH_B_PLUGIN_ID, true),
            (FINISH_C_PLUGIN_ID, false),
        ] {
            app.add_plugin(FinishOrderPlugin {
                id,
                cleanup_order: Arc::clone(&cleanup_order),
                fail_finish,
            })
            .unwrap();
        }

        let Err(expected) = app.finish_plugins() else {
            panic!("finish failure should be reported");
        };
        assert_eq!(
            expected,
            PluginError::SetupFailed {
                plugin: FINISH_B_PLUGIN_ID,
                message: "finish failed".into(),
            }
        );
        let Err(repeated) = app.finish_plugins() else {
            panic!("poisoned app should retain the first failure");
        };
        assert_eq!(repeated, expected);
        let run_error = app.run_once(Duration::ZERO).unwrap_err();
        assert_eq!(run_error.plugin_error(), Some(&expected));
        assert!(run_error.plugin_failure_report().is_some());
        assert_eq!(app.world().resource::<Frames>().0, 0);
        drop(app);

        assert_eq!(
            *cleanup_order.lock().unwrap(),
            [FINISH_C_PLUGIN_ID, FINISH_B_PLUGIN_ID, FINISH_A_PLUGIN_ID]
        );
    }

    #[test]
    fn finish_panic_poisoning_cleans_all_plugins_in_reverse_order() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new();
        for (id, behavior) in [
            (PROBE_A_PLUGIN_ID, ProbeBehavior::default()),
            (
                PROBE_B_PLUGIN_ID,
                ProbeBehavior {
                    panic_finish: true,
                    ..ProbeBehavior::default()
                },
            ),
            (PROBE_C_PLUGIN_ID, ProbeBehavior::default()),
        ] {
            app.add_plugin(LifecycleProbePlugin {
                id,
                behavior,
                trace: Arc::clone(&trace),
            })
            .unwrap();
        }

        let Err(error) = app.finish_plugins() else {
            panic!("finish panic should be isolated and poison the app");
        };

        assert_eq!(
            error,
            PluginError::HookPanicked {
                plugin: PROBE_B_PLUGIN_ID,
                hook: PluginHook::Finish,
            }
        );
        let report = app.plugin_failure_report().unwrap();
        assert_eq!(report.primary().unwrap().error(), &error);
        assert!(report.cleanup_complete());
        let cleanup_order = trace
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(plugin, hook)| (*hook == PluginHook::Cleanup).then_some(*plugin))
            .collect::<Vec<_>>();
        assert_eq!(
            cleanup_order,
            [PROBE_C_PLUGIN_ID, PROBE_B_PLUGIN_ID, PROBE_A_PLUGIN_ID]
        );
    }

    #[test]
    fn preflight_rejection_is_retryable_and_does_not_mutate() {
        let allowed = Arc::new(AtomicBool::new(false));
        let plugin = RetryablePreflightPlugin {
            allowed: Arc::clone(&allowed),
        };
        let mut app = App::new();

        let Err(error) = app.add_plugin(plugin.clone()) else {
            panic!("preflight should reject the first attempt");
        };
        assert_eq!(
            error,
            PluginError::SetupFailed {
                plugin: PREFLIGHT_PLUGIN_ID,
                message: "preflight rejected".into(),
            }
        );
        assert_eq!(
            app.plugin_lifecycle_state(),
            PluginLifecycleState::Configuring
        );
        assert!(app.plugin_failure_report().is_none());
        assert!(!app.world().contains_resource::<PluginBuildCount>());

        allowed.store(true, Ordering::SeqCst);
        app.add_plugin(plugin).unwrap();

        assert!(app.has_plugin(PREFLIGHT_PLUGIN_ID));
        assert_eq!(app.world().resource::<PluginBuildCount>().0, 7);
    }

    #[test]
    fn preflight_panic_is_retryable_and_does_not_commit_plugin() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new();

        let Err(error) = app.add_plugin(LifecycleProbePlugin {
            id: PROBE_A_PLUGIN_ID,
            behavior: ProbeBehavior {
                panic_preflight: true,
                ..ProbeBehavior::default()
            },
            trace: Arc::clone(&trace),
        }) else {
            panic!("preflight panic should be isolated");
        };

        assert_eq!(
            error,
            PluginError::HookPanicked {
                plugin: PROBE_A_PLUGIN_ID,
                hook: PluginHook::Preflight,
            }
        );
        assert_eq!(
            app.plugin_lifecycle_state(),
            PluginLifecycleState::Configuring
        );
        assert!(app.plugin_failure_report().is_none());
        assert!(trace.lock().unwrap().is_empty());

        app.add_plugin(LifecycleProbePlugin {
            id: PROBE_A_PLUGIN_ID,
            behavior: ProbeBehavior::default(),
            trace,
        })
        .unwrap();
        assert!(app.has_plugin(PROBE_A_PLUGIN_ID));
    }

    #[test]
    fn build_panic_poisoning_retains_error_and_cleans_current_plugin() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new();
        let plugin = LifecycleProbePlugin {
            id: PROBE_A_PLUGIN_ID,
            behavior: ProbeBehavior {
                panic_build: true,
                ..ProbeBehavior::default()
            },
            trace: Arc::clone(&trace),
        };

        let Err(expected) = app.add_plugin(plugin) else {
            panic!("build panic should be isolated and reported");
        };
        assert_eq!(
            expected,
            PluginError::HookPanicked {
                plugin: PROBE_A_PLUGIN_ID,
                hook: PluginHook::Build,
            }
        );
        assert_eq!(app.plugin_lifecycle_state(), PluginLifecycleState::Poisoned);
        let report = app.plugin_failure_report().unwrap().clone();
        assert_eq!(report.primary().unwrap().error(), &expected);
        assert!(report.cleanup_complete());
        assert_eq!(
            *trace.lock().unwrap(),
            [
                (PROBE_A_PLUGIN_ID, PluginHook::Build),
                (PROBE_A_PLUGIN_ID, PluginHook::Cleanup),
            ]
        );

        let Err(world_error) = app.world_mut() else {
            panic!("poisoned app must reject mutable world access");
        };
        assert_eq!(world_error, expected);
        let Err(resource_error) = app.insert_resource(Frames::default()) else {
            panic!("poisoned app must reject resource insertion");
        };
        assert_eq!(resource_error, expected);
        let update_error = app.update().unwrap_err();
        assert_eq!(update_error.plugin_error(), Some(&expected));
        assert_eq!(update_error.plugin_failure_report(), Some(&report));
        assert_eq!(
            app.cleanup_plugins().unwrap_err(),
            PluginCleanupError::Failure(Box::new(report))
        );
        assert_eq!(trace.lock().unwrap().len(), 2);
    }

    #[test]
    fn poisoned_app_rejects_every_public_mutation_and_consuming_run() {
        macro_rules! assert_primary_error {
            ($operation:expr, $expected:expr) => {
                match $operation {
                    Err(error) => assert_eq!(error, $expected),
                    Ok(_) => panic!("poisoned app accepted a mutable lifecycle operation"),
                }
            };
        }

        let mut app = App::new();
        let Err(expected) = app.add_plugin(CommittedFailurePlugin) else {
            panic!("committed build failure should poison the app");
        };

        assert_primary_error!(app.world_mut(), expected);
        assert_primary_error!(app.insert_resource(Frames::default()), expected);
        assert_primary_error!(app.init_resource::<Frames>(), expected);
        assert_primary_error!(
            app.add_startup_systems(StartupStage::Core, spawn_entity),
            expected
        );
        assert_primary_error!(app.add_systems(CoreStage::Update, count_frame), expected);
        assert_primary_error!(
            app.configure_sets(CoreStage::TaskUpdate, TaskUpdateSet::Poll),
            expected
        );
        assert_primary_error!(app.set_runner(|_| Ok(AppExit::Success)), expected);
        assert_primary_error!(app.add_plugin(CountingPlugin), expected);
        assert_primary_error!(app.add_plugins(CountingGroup), expected);
        assert_primary_error!(app.finish_plugins(), expected);
        assert_eq!(
            app.run_once(Duration::ZERO).unwrap_err().plugin_error(),
            Some(&expected)
        );
        assert_eq!(app.update().unwrap_err().plugin_error(), Some(&expected));

        let mut consuming_app = App::new();
        let Err(consuming_expected) = consuming_app.add_plugin(CommittedFailurePlugin) else {
            panic!("committed build failure should poison the consuming app");
        };
        let run_error = consuming_app.run().unwrap_err();
        assert_eq!(run_error.plugin_error(), Some(&consuming_expected));
        assert!(run_error.plugin_failure_report().is_some());
    }

    #[test]
    fn dependency_cycle_poisoning_is_bounded_and_inspectable() {
        let mut app = App::new();

        let Err(error) = app.add_plugin(CycleAPlugin) else {
            panic!("recursive plugin dependency should fail");
        };

        assert_eq!(
            error,
            PluginError::DependencyCycle {
                plugin: CYCLE_A_PLUGIN_ID,
                chain: vec![CYCLE_A_PLUGIN_ID, CYCLE_B_PLUGIN_ID, CYCLE_A_PLUGIN_ID],
            }
        );
        assert_eq!(app.plugin_lifecycle_state(), PluginLifecycleState::Poisoned);
        let report = app.plugin_failure_report().unwrap();
        assert_eq!(
            report.primary().unwrap().subject(),
            PluginFailureSubject::Plugin(CYCLE_B_PLUGIN_ID)
        );
        assert_eq!(report.primary().unwrap().hook(), PluginHook::Build);
        assert!(report.cleanup_complete());
    }

    #[test]
    fn successful_nested_plugins_finish_dependencies_first_and_cleanup_dependents_first() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new();
        app.add_plugin(NestedProbePlugin {
            trace: Arc::clone(&trace),
        })
        .unwrap();

        app.finish_plugins().unwrap();
        app.cleanup_plugins().unwrap();

        assert_eq!(
            *trace.lock().unwrap(),
            [
                (PROBE_B_PLUGIN_ID, PluginHook::Build),
                (NESTED_PLUGIN_ID, PluginHook::Build),
                (PROBE_B_PLUGIN_ID, PluginHook::Finish),
                (NESTED_PLUGIN_ID, PluginHook::Finish),
                (NESTED_PLUGIN_ID, PluginHook::Cleanup),
                (PROBE_B_PLUGIN_ID, PluginHook::Cleanup),
            ]
        );
    }

    #[test]
    fn ignored_nested_failure_still_preserves_first_error_and_poisoning() {
        let mut app = App::new();

        let Err(error) = app.add_plugin(IgnoreNestedFailurePlugin) else {
            panic!("ignored nested failure must still fail outer installation");
        };

        assert_eq!(
            error,
            PluginError::SetupFailed {
                plugin: COMMITTED_FAILURE_PLUGIN_ID,
                message: "committed build failed".into(),
            }
        );
        assert_eq!(app.plugin_lifecycle_state(), PluginLifecycleState::Poisoned);
        assert!(!app.has_plugin(IGNORE_FAILURE_PLUGIN_ID));
        let report = app.plugin_failure_report().unwrap();
        assert_eq!(
            report.primary().unwrap().subject(),
            PluginFailureSubject::Plugin(COMMITTED_FAILURE_PLUGIN_ID)
        );
        assert!(report.cleanup_complete());
    }

    #[test]
    fn plugin_group_dependency_cycle_is_bounded_and_poisoning() {
        let mut app = App::new();

        let Err(error) = app.add_plugins(CycleAGroup) else {
            panic!("recursive plugin group dependency should fail");
        };

        assert_eq!(
            error,
            PluginError::GroupDependencyCycle {
                group: CYCLE_A_GROUP_ID,
                chain: vec![CYCLE_A_GROUP_ID, CYCLE_B_GROUP_ID, CYCLE_A_GROUP_ID],
            }
        );
        assert_eq!(app.plugin_lifecycle_state(), PluginLifecycleState::Poisoned);
        let report = app.plugin_failure_report().unwrap();
        assert_eq!(
            report.primary().unwrap().subject(),
            PluginFailureSubject::Group(CYCLE_B_GROUP_ID)
        );
        assert_eq!(report.primary().unwrap().hook(), PluginHook::Build);
    }

    #[test]
    fn lifecycle_control_reentry_is_rejected_without_poisoning() {
        let mut app = App::new();

        app.add_plugin(LifecycleReentryPlugin).unwrap();

        assert_eq!(
            app.plugin_lifecycle_state(),
            PluginLifecycleState::Configuring
        );
        assert!(app.plugin_failure_report().is_none());
        assert!(app.has_plugin(PROBE_C_PLUGIN_ID));
    }

    #[test]
    fn lifecycle_control_reentry_from_finish_is_rejected() {
        let mut app = App::new();
        app.add_plugin(FinishReentryPlugin).unwrap();

        app.finish_plugins().unwrap();

        assert_eq!(app.plugin_lifecycle_state(), PluginLifecycleState::Ready);
        assert!(app.plugin_failure_report().is_none());
    }

    #[test]
    fn partial_plugin_group_failure_poisoning_does_not_publish_group() {
        let mut app = App::new();

        let Err(error) = app.add_plugins(PartialGroup) else {
            panic!("partial plugin group should fail");
        };

        assert_eq!(
            error,
            PluginError::SetupFailed {
                plugin: COMMITTED_FAILURE_PLUGIN_ID,
                message: "committed build failed".into(),
            }
        );
        assert_eq!(app.plugin_lifecycle_state(), PluginLifecycleState::Poisoned);
        assert!(
            app.installed_plugin_groups()
                .all(|group| group.id != PARTIAL_GROUP_ID)
        );
        assert!(app.plugin_failure_report().unwrap().cleanup_complete());
    }

    #[test]
    fn cleanup_failures_and_panics_are_aggregated_without_stopping_cleanup() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new();
        for (id, behavior) in [
            (
                PROBE_A_PLUGIN_ID,
                ProbeBehavior {
                    fail_cleanup: true,
                    ..ProbeBehavior::default()
                },
            ),
            (
                PROBE_B_PLUGIN_ID,
                ProbeBehavior {
                    panic_cleanup: true,
                    ..ProbeBehavior::default()
                },
            ),
            (PROBE_C_PLUGIN_ID, ProbeBehavior::default()),
        ] {
            app.add_plugin(LifecycleProbePlugin {
                id,
                behavior,
                trace: Arc::clone(&trace),
            })
            .unwrap();
        }

        let PluginCleanupError::Failure(report) = app.cleanup_plugins().unwrap_err() else {
            panic!("completed cleanup failures should return their report");
        };

        assert!(report.primary().is_none());
        assert!(report.cleanup_complete());
        assert_eq!(report.cleanup_failures().len(), 2);
        assert_eq!(
            report.cleanup_failures()[0].subject(),
            PluginFailureSubject::Plugin(PROBE_B_PLUGIN_ID)
        );
        assert_eq!(
            report.cleanup_failures()[0].error(),
            &PluginError::HookPanicked {
                plugin: PROBE_B_PLUGIN_ID,
                hook: PluginHook::Cleanup,
            }
        );
        assert_eq!(
            report.cleanup_failures()[1].subject(),
            PluginFailureSubject::Plugin(PROBE_A_PLUGIN_ID)
        );
        assert_eq!(app.plugin_lifecycle_state(), PluginLifecycleState::Cleaned);

        let cleanup_order = trace
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(plugin, hook)| (*hook == PluginHook::Cleanup).then_some(*plugin))
            .collect::<Vec<_>>();
        assert_eq!(
            cleanup_order,
            [PROBE_C_PLUGIN_ID, PROBE_B_PLUGIN_ID, PROBE_A_PLUGIN_ID]
        );
        assert_eq!(
            app.cleanup_plugins().unwrap_err(),
            PluginCleanupError::Failure(report)
        );
        assert_eq!(
            trace
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, hook)| *hook == PluginHook::Cleanup)
                .count(),
            3
        );
    }

    #[test]
    fn cleanup_failure_does_not_replace_finish_failure() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new();
        app.add_plugin(LifecycleProbePlugin {
            id: PROBE_A_PLUGIN_ID,
            behavior: ProbeBehavior {
                fail_finish: true,
                fail_cleanup: true,
                ..ProbeBehavior::default()
            },
            trace,
        })
        .unwrap();

        let Err(expected) = app.finish_plugins() else {
            panic!("finish failure should poison the app");
        };
        let report = app.plugin_failure_report().unwrap();

        assert_eq!(
            expected,
            PluginError::SetupFailed {
                plugin: PROBE_A_PLUGIN_ID,
                message: "finish rejected".into(),
            }
        );
        assert_eq!(report.primary().unwrap().error(), &expected);
        assert_eq!(report.primary().unwrap().hook(), PluginHook::Finish);
        assert_eq!(report.cleanup_failures().len(), 1);
        assert_eq!(
            report.cleanup_failures()[0].error(),
            &PluginError::SetupFailed {
                plugin: PROBE_A_PLUGIN_ID,
                message: "cleanup rejected".into(),
            }
        );
    }

    #[test]
    fn drop_during_unwind_is_not_aborted_by_cleanup_panic() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let observed_trace = Arc::clone(&trace);

        let unwind = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut app = App::new();
            app.add_plugin(LifecycleProbePlugin {
                id: PROBE_A_PLUGIN_ID,
                behavior: ProbeBehavior {
                    panic_cleanup: true,
                    ..ProbeBehavior::default()
                },
                trace,
            })
            .unwrap();
            panic!("outer unwind");
        }));

        assert!(unwind.is_err());
        assert!(
            observed_trace
                .lock()
                .unwrap()
                .contains(&(PROBE_A_PLUGIN_ID, PluginHook::Cleanup))
        );
    }
}
