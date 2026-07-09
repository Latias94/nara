//! Application lifecycle and plugin orchestration for nara.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Display, Formatter},
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

pub trait Plugin: Send + Sync + 'static {
    fn metadata(&self) -> PluginMetadata;

    fn build(&self, app: &mut App) -> Result<(), PluginError>;

    fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn cleanup(&self, _app: &mut App) {}

    fn plugin_id(&self) -> PluginId {
        self.metadata().id
    }
}

pub trait PluginGroup: Send + Sync + 'static {
    fn metadata(&self) -> PluginGroupMetadata;

    fn build(&self, app: &mut App) -> Result<(), PluginError>;
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
    #[error("plugin {plugin} failed to initialize: {message}")]
    SetupFailed { plugin: PluginId, message: String },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AppRunError {
    #[error(transparent)]
    Plugin(#[from] PluginError),
    #[error("app runner failed: {message}")]
    Runner { message: String },
}

impl AppRunError {
    #[must_use]
    pub fn runner(message: impl Into<String>) -> Self {
        Self::Runner {
            message: message.into(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AppExit {
    #[default]
    Success,
    Requested,
}

pub type RunnerFn = Box<dyn FnOnce(App) -> Result<AppExit, AppRunError>>;

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

pub struct App {
    world: World,
    startup_schedules: BTreeMap<StartupStage, Schedule>,
    schedules: BTreeMap<CoreStage, Schedule>,
    runner: Option<RunnerFn>,
    plugins: Vec<Box<dyn Plugin>>,
    plugin_install_counts: BTreeMap<PluginId, usize>,
    plugin_metadata: BTreeMap<PluginId, PluginMetadata>,
    provided_capabilities: BTreeSet<PluginCapability>,
    plugin_groups: BTreeMap<PluginGroupId, PluginGroupMetadata>,
    plugins_finished: bool,
    started: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let plugins = std::mem::take(&mut self.plugins);
        for plugin in &plugins {
            plugin.cleanup(self);
        }
        self.plugins = plugins;
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
            plugins_finished: false,
            started: false,
        }
    }

    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    #[must_use]
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn insert_resource<R: Resource>(&mut self, resource: R) -> &mut Self {
        self.world.insert_resource(resource);
        self
    }

    pub fn init_resource<R>(&mut self) -> &mut Self
    where
        R: Resource + nara_ecs::world::FromWorld,
    {
        self.world.init_resource::<R>();
        self
    }

    pub fn add_startup_systems<M>(
        &mut self,
        stage: StartupStage,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        self.startup_schedule_mut(stage).add_systems(systems);
        self
    }

    pub fn add_systems<M>(
        &mut self,
        stage: CoreStage,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        self.schedule_mut(stage).add_systems(systems);
        self
    }

    pub fn configure_sets<M>(
        &mut self,
        stage: CoreStage,
        sets: impl IntoScheduleConfigs<InternedSystemSet, M>,
    ) -> &mut Self {
        self.schedule_mut(stage).configure_sets(sets);
        self
    }

    pub fn set_runner(
        &mut self,
        runner: impl FnOnce(App) -> Result<AppExit, AppRunError> + 'static,
    ) -> &mut Self {
        self.runner = Some(Box::new(runner));
        self
    }

    pub fn add_plugin(&mut self, plugin: impl Plugin) -> Result<&mut Self, PluginError> {
        let metadata = plugin.metadata();
        if self.plugins_finished {
            return Err(PluginError::AddedAfterFinish {
                plugin: metadata.id,
            });
        }
        if metadata.unique && self.has_plugin(metadata.id) {
            return Err(PluginError::Duplicate {
                plugin: metadata.id,
            });
        }

        self.check_plugin_metadata(metadata)?;
        plugin.build(self)?;
        *self.plugin_install_counts.entry(metadata.id).or_default() += 1;
        self.plugin_metadata.entry(metadata.id).or_insert(metadata);
        for capability in metadata.provides {
            self.provided_capabilities.insert(*capability);
        }
        self.plugins.push(Box::new(plugin));
        Ok(self)
    }

    pub fn add_plugin_if_missing(&mut self, plugin: impl Plugin) -> Result<&mut Self, PluginError> {
        let metadata = plugin.metadata();
        if self.plugins_finished {
            return Err(PluginError::AddedAfterFinish {
                plugin: metadata.id,
            });
        }
        if metadata.unique && self.has_plugin(metadata.id) {
            return Ok(self);
        }
        self.add_plugin(plugin)
    }

    pub fn add_plugins(&mut self, group: impl PluginGroup) -> Result<&mut Self, PluginError> {
        let metadata = group.metadata();
        if self.plugins_finished {
            return Err(PluginError::GroupAddedAfterFinish { group: metadata.id });
        }
        if self.plugin_groups.contains_key(&metadata.id) {
            return Err(PluginError::DuplicateGroup { group: metadata.id });
        }
        group.build(self)?;
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
        Ok(())
    }

    pub fn finish_plugins(&mut self) -> Result<&mut Self, PluginError> {
        if self.plugins_finished {
            return Ok(self);
        }

        self.plugins_finished = true;
        let plugins = std::mem::take(&mut self.plugins);
        for plugin in &plugins {
            plugin.finish(self)?;
        }
        self.plugins = plugins;
        Ok(self)
    }

    pub fn run(mut self) -> Result<AppExit, AppRunError> {
        self.finish_plugins()?;
        let runner = self
            .runner
            .take()
            .unwrap_or_else(|| Box::new(default_runner));
        runner(self)
    }

    pub fn run_once(&mut self, real_delta: Duration) -> Result<AppFrameOutcome, AppRunError> {
        self.finish_plugins()?;

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

    pub fn try_update(&mut self) -> Result<AppFrameOutcome, AppRunError> {
        self.run_once(Duration::ZERO)
    }

    pub fn update(&mut self) -> AppFrameOutcome {
        self.try_update()
            .expect("app update failed while running one frame")
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

fn default_runner(mut app: App) -> Result<AppExit, AppRunError> {
    let outcome = app.run_once(Duration::ZERO)?;
    Ok(outcome.exit.unwrap_or(AppExit::Success))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_ecs::{Commands, Component, ResMut, Resource};

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

    #[derive(Debug, Default, Clone, Copy)]
    struct CountingPlugin;

    impl Plugin for CountingPlugin {
        fn metadata(&self) -> PluginMetadata {
            PluginMetadata::new(COUNTING_PLUGIN_ID, PluginCategory::Core)
                .provides(&[COUNTING_CAPABILITY])
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            if !app.world().contains_resource::<PluginBuildCount>() {
                app.insert_resource(PluginBuildCount::default());
            }
            app.world_mut().resource_mut::<PluginBuildCount>().0 += 1;
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
    struct CountingGroup;

    impl PluginGroup for CountingGroup {
        fn metadata(&self) -> PluginGroupMetadata {
            PluginGroupMetadata::new(COUNTING_GROUP_ID, &[COUNTING_PLUGIN_ID])
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.add_plugin_if_missing(CountingPlugin)?;
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
        app.insert_resource(Frames::default())
            .add_startup_systems(StartupStage::Core, spawn_entity)
            .add_systems(CoreStage::Update, (spawn_entity, count_frame));

        app.update();
        app.update();

        let spawned_count = {
            let world = app.world_mut();
            let mut query = world.query::<&Spawned>();
            query.iter(world).count()
        };

        assert_eq!(spawned_count, 3);
        assert_eq!(app.world().resource::<Frames>().0, 2);
    }

    #[test]
    fn run_once_advances_time_and_runs_fixed_update_when_due() {
        let mut app = App::new();
        app.insert_resource(Frames::default())
            .add_systems(CoreStage::FixedUpdate, count_frame);

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
        app.insert_resource(Frames::default())
            .insert_resource(RuntimeTimeSettings::default().with_paused(true))
            .add_systems(CoreStage::FixedUpdate, count_frame);

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
        app.insert_resource(Frames::default())
            .insert_resource(RuntimeTimeSettings::default().with_time_scale(0.5))
            .add_systems(CoreStage::FixedUpdate, count_frame);

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
        );

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
        app.insert_resource(Frames::default())
            .add_systems(CoreStage::FixedUpdate, count_frame);

        app.run_once(FixedTime::DEFAULT_TIMESTEP / 2).unwrap();

        assert_eq!(app.world().resource::<Frames>().0, 0);
        assert_eq!(app.world().resource::<FixedTime>().steps_this_frame(), 0);
    }

    #[test]
    fn fixed_update_limits_catch_up_ticks() {
        let mut app = App::new();
        app.insert_resource(Frames::default())
            .insert_resource(FixedTime::default().with_max_steps_per_frame(2))
            .add_systems(CoreStage::FixedUpdate, count_frame);

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
        app.add_systems(CoreStage::Update, request_exit);

        let outcome = app.run_once(Duration::ZERO).unwrap();

        assert_eq!(outcome.exit, Some(AppExit::Requested));
        assert_eq!(app.world().resource::<AppExitRequests>().requested(), None);
    }

    #[test]
    fn first_pre_fixed_and_update_run_in_order() {
        let mut app = App::new();
        app.insert_resource(Order::default())
            .add_systems(CoreStage::First, push_first)
            .add_systems(CoreStage::TaskUpdate, push_task_update)
            .add_systems(CoreStage::PreUpdate, push_pre_update)
            .add_systems(CoreStage::FixedUpdate, push_fixed_update)
            .add_systems(CoreStage::Update, push_update);

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
        app.insert_resource(Order::default())
            .configure_sets(
                CoreStage::TaskUpdate,
                (
                    TaskUpdateSet::Poll,
                    TaskUpdateSet::CoalesceAssetChanges,
                    TaskUpdateSet::SpawnAssetJobs,
                    TaskUpdateSet::ApplyAssetResults,
                )
                    .chain(),
            )
            .add_systems(
                CoreStage::TaskUpdate,
                push_task_apply.in_set(TaskUpdateSet::ApplyAssetResults),
            )
            .add_systems(
                CoreStage::TaskUpdate,
                push_task_spawn.in_set(TaskUpdateSet::SpawnAssetJobs),
            )
            .add_systems(
                CoreStage::TaskUpdate,
                push_task_poll.in_set(TaskUpdateSet::Poll),
            )
            .add_systems(
                CoreStage::TaskUpdate,
                push_task_coalesce.in_set(TaskUpdateSet::CoalesceAssetChanges),
            );

        app.run_once(Duration::ZERO).unwrap();

        assert_eq!(
            app.world().resource::<Order>().0,
            ["task_poll", "task_coalesce", "task_spawn", "task_apply"]
        );
    }

    #[test]
    fn render_pipeline_stages_run_in_order() {
        let mut app = App::new();
        app.insert_resource(Order::default())
            .add_systems(CoreStage::Extract, push_extract)
            .add_systems(CoreStage::Prepare, push_prepare)
            .add_systems(CoreStage::Queue, push_queue)
            .add_systems(CoreStage::Sort, push_sort)
            .add_systems(CoreStage::Render, push_render)
            .add_systems(CoreStage::Cleanup, push_cleanup)
            .add_systems(CoreStage::Last, push_last);

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
        app.set_runner(|mut app| {
            app.run_once(Duration::ZERO)?;
            Ok(AppExit::Requested)
        });

        assert_eq!(app.run().unwrap(), AppExit::Requested);
    }

    #[test]
    fn runner_failure_is_reported_without_panic() {
        let mut app = App::new();
        app.set_runner(|_app| Err(AppRunError::runner("window creation failed")));

        assert_eq!(
            app.run().unwrap_err(),
            AppRunError::runner("window creation failed")
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
}
