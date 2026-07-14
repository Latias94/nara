//! Application lifecycle and plugin orchestration for nara.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{self, Display, Formatter},
    num::NonZeroU32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum FixedUpdateSet {
    /// Admit tick-scoped inputs and prepare simulation data.
    Prepare,
    /// Run authoritative fixed-step simulation.
    Simulate,
    /// Publish tick-scoped outcomes after simulation commands are flushed.
    Finalize,
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

#[derive(Debug, Error, Clone, PartialEq)]
pub enum AppRunError {
    #[error("plugin lifecycle failed: {error}")]
    Plugin {
        error: PluginError,
        report: Option<Box<PluginFailureReport>>,
    },
    #[error("app runner failed: {message}")]
    Runner { message: String },
    #[error("app runner and teardown both failed: prior={prior}; teardown={teardown}")]
    RunnerTeardown {
        prior: Box<AppRunError>,
        teardown: Box<AppRunError>,
    },
    #[error("time frame planning failed: {error}")]
    Time { error: TimeFrameError },
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
    pub fn runner_teardown(prior: AppRunError, teardown: AppRunError) -> Self {
        Self::RunnerTeardown {
            prior: Box::new(prior),
            teardown: Box::new(teardown),
        }
    }

    #[must_use]
    pub const fn time(error: TimeFrameError) -> Self {
        Self::Time { error }
    }

    #[must_use]
    pub const fn plugin_error(&self) -> Option<&PluginError> {
        match self {
            Self::Plugin { error, .. } => Some(error),
            Self::RunnerTeardown { prior, teardown } => match prior.plugin_error() {
                Some(error) => Some(error),
                None => teardown.plugin_error(),
            },
            Self::Runner { .. } | Self::Time { .. } | Self::Shutdown { .. } => None,
        }
    }

    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        match self {
            Self::Plugin { report, .. } => report.as_deref(),
            Self::Shutdown { report, .. } => Some(report.as_ref()),
            Self::RunnerTeardown { prior, teardown } => prior
                .plugin_failure_report()
                .or_else(|| teardown.plugin_failure_report()),
            Self::Runner { .. } | Self::Time { .. } => None,
        }
    }
}

impl From<PluginError> for AppRunError {
    fn from(error: PluginError) -> Self {
        Self::plugin(error, None)
    }
}

impl From<TimeFrameError> for AppRunError {
    fn from(error: TimeFrameError) -> Self {
        Self::time(error)
    }
}

impl From<FixedTimeError> for AppRunError {
    fn from(error: FixedTimeError) -> Self {
        Self::time(TimeFrameError::Fixed(error))
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
    fn plan_advance(&self, delta: Duration) -> Result<Self, TimeFrameError> {
        let elapsed =
            self.elapsed
                .checked_add(delta)
                .ok_or(TimeFrameError::RealElapsedOverflow {
                    elapsed: self.elapsed,
                    delta,
                })?;
        let frame = self
            .frame
            .checked_add(1)
            .ok_or(TimeFrameError::RealFrameOverflow { frame: self.frame })?;
        Ok(Self {
            delta,
            elapsed,
            frame,
        })
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
    fn plan_advance(&self, delta: Duration) -> Result<Self, TimeFrameError> {
        let elapsed =
            self.elapsed
                .checked_add(delta)
                .ok_or(TimeFrameError::VirtualElapsedOverflow {
                    elapsed: self.elapsed,
                    delta,
                })?;
        let frame = self
            .frame
            .checked_add(1)
            .ok_or(TimeFrameError::VirtualFrameOverflow { frame: self.frame })?;
        Ok(Self {
            delta,
            elapsed,
            frame,
        })
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

#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum TimeSettingsError {
    #[error("time scale must be finite and non-negative, got {value}")]
    InvalidTimeScale { value: f32 },
    #[error("maximum real frame delta must be non-zero")]
    ZeroMaxDelta,
    #[error("fixed timestep must be non-zero")]
    ZeroFixedTimestep,
    #[error("maximum fixed steps per frame must be non-zero")]
    ZeroMaxFixedStepsPerFrame,
    #[error("maximum fixed debt steps must be non-zero")]
    ZeroMaxFixedDebtSteps,
    #[error("fixed clock settings cannot change during a fixed frame")]
    FixedFrameActive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FixedTimeError {
    #[error(
        "fixed catch-up would queue {pending_steps} ticks, exceeding per-frame work {max_steps_per_frame} plus debt limit {max_debt_steps}"
    )]
    CatchUpDebtExceeded {
        pending_steps: u128,
        max_steps_per_frame: u32,
        max_debt_steps: u32,
    },
    #[error("fixed pending time overflowed while adding {incoming:?} to {pending:?}")]
    PendingDurationOverflow {
        pending: Duration,
        incoming: Duration,
    },
    #[error("fixed tick {tick} cannot advance by {attempted_steps} steps")]
    TickOverflow { tick: u64, attempted_steps: u32 },
    #[error(
        "fixed elapsed time {elapsed:?} cannot advance by {attempted_steps} steps of {timestep:?}"
    )]
    ElapsedOverflow {
        elapsed: Duration,
        timestep: Duration,
        attempted_steps: u32,
    },
}

/// Identifies a retained resource required to plan and complete an app frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeFrameResource {
    RuntimeTimeSettings,
    RealTime,
    VirtualTime,
    FixedTime,
    RenderTime,
    RuntimeFrameStatus,
    AppExitRequests,
}

impl Display for TimeFrameResource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RuntimeTimeSettings => "RuntimeTimeSettings",
            Self::RealTime => "RealTime",
            Self::VirtualTime => "VirtualTime",
            Self::FixedTime => "FixedTime",
            Self::RenderTime => "RenderTime",
            Self::RuntimeFrameStatus => "RuntimeFrameStatus",
            Self::AppExitRequests => "AppExitRequests",
        })
    }
}

/// A failure to build an atomic clock plan for one app frame.
#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum TimeFrameError {
    #[error("required frame resource is missing: {resource}")]
    MissingResource { resource: TimeFrameResource },
    #[error("real elapsed time {elapsed:?} cannot advance by {delta:?}")]
    RealElapsedOverflow { elapsed: Duration, delta: Duration },
    #[error("real frame counter cannot advance beyond {frame}")]
    RealFrameOverflow { frame: u64 },
    #[error(
        "virtual delta cannot represent real delta {clamped_real_delta:?} scaled by {time_scale}"
    )]
    VirtualDeltaOverflow {
        clamped_real_delta: Duration,
        time_scale: f32,
    },
    #[error("virtual elapsed time {elapsed:?} cannot advance by {delta:?}")]
    VirtualElapsedOverflow { elapsed: Duration, delta: Duration },
    #[error("virtual frame counter cannot advance beyond {frame}")]
    VirtualFrameOverflow { frame: u64 },
    #[error(transparent)]
    Fixed(#[from] FixedTimeError),
}

#[derive(Clone, Copy, Debug, PartialEq, Resource)]
pub struct RuntimeTimeSettings {
    paused: bool,
    time_scale: f32,
    max_delta: Duration,
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
    pub fn new(
        paused: bool,
        time_scale: f32,
        max_delta: Duration,
    ) -> Result<Self, TimeSettingsError> {
        Self::default()
            .with_paused(paused)
            .with_time_scale(time_scale)?
            .with_max_delta(max_delta)
    }

    #[must_use]
    pub fn paused(&self) -> bool {
        self.paused
    }

    #[must_use]
    pub fn time_scale(&self) -> f32 {
        self.time_scale
    }

    #[must_use]
    pub fn max_delta(&self) -> Duration {
        self.max_delta
    }

    #[must_use]
    pub fn with_paused(mut self, paused: bool) -> Self {
        self.paused = paused;
        self
    }

    pub fn with_time_scale(mut self, time_scale: f32) -> Result<Self, TimeSettingsError> {
        self.set_time_scale(time_scale)?;
        Ok(self)
    }

    pub fn with_max_delta(mut self, max_delta: Duration) -> Result<Self, TimeSettingsError> {
        self.set_max_delta(max_delta)?;
        Ok(self)
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn set_time_scale(&mut self, time_scale: f32) -> Result<(), TimeSettingsError> {
        if !time_scale.is_finite() || time_scale < 0.0 {
            return Err(TimeSettingsError::InvalidTimeScale { value: time_scale });
        }
        self.time_scale = time_scale;
        Ok(())
    }

    pub fn set_max_delta(&mut self, max_delta: Duration) -> Result<(), TimeSettingsError> {
        if max_delta.is_zero() {
            return Err(TimeSettingsError::ZeroMaxDelta);
        }
        self.max_delta = max_delta;
        Ok(())
    }

    fn fixed_time_enabled(&self) -> bool {
        !self.paused && self.time_scale > 0.0
    }

    fn plan_virtual_delta(&self, real_delta: Duration) -> Result<(Duration, bool), TimeFrameError> {
        let clamped = real_delta.min(self.max_delta);
        let was_clamped = clamped != real_delta;
        if self.paused {
            return Ok((Duration::ZERO, was_clamped));
        }
        let scaled_seconds = clamped.as_secs_f64() * f64::from(self.time_scale);
        let virtual_delta = Duration::try_from_secs_f64(scaled_seconds).map_err(|_| {
            TimeFrameError::VirtualDeltaOverflow {
                clamped_real_delta: clamped,
                time_scale: self.time_scale,
            }
        })?;
        Ok((virtual_delta, was_clamped))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Resource)]
pub struct RenderTime {
    /// Fraction of the current fixed timestep represented by [`Self::remainder`].
    pub interpolation_alpha: f32,
    /// Sub-tick virtual time left after fixed work and catch-up policy complete.
    pub remainder: Duration,
}

impl Default for RenderTime {
    fn default() -> Self {
        Self {
            interpolation_alpha: 0.0,
            remainder: Duration::ZERO,
        }
    }
}

impl RenderTime {
    fn update_from_fixed(&mut self, fixed: &FixedTime) {
        self.remainder = fixed.remainder;
        let alpha = fixed.remainder.as_secs_f64() / fixed.timestep.as_secs_f64();
        self.interpolation_alpha = (alpha as f32).min(f32::from_bits(1.0f32.to_bits() - 1));
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FixedCatchUpPolicy {
    /// Drop whole pending ticks beyond the per-frame work cap.
    #[default]
    DiscardExcess,
    /// Keep whole pending ticks for later frames, up to the configured debt limit.
    PreserveDebt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct FixedTime {
    timestep: Duration,
    max_steps_per_frame: NonZeroU32,
    max_debt_steps: NonZeroU32,
    catch_up_policy: FixedCatchUpPolicy,
    pending: Duration,
    remainder: Duration,
    debt: Duration,
    delta: Duration,
    elapsed: Duration,
    tick: u64,
    steps_this_frame: u32,
    capped_this_frame: bool,
    discarded_this_frame: Duration,
    advance_enabled: bool,
    frame_active: bool,
}

impl Default for FixedTime {
    fn default() -> Self {
        Self {
            timestep: Self::DEFAULT_TIMESTEP,
            max_steps_per_frame: NonZeroU32::new(Self::DEFAULT_MAX_STEPS_PER_FRAME)
                .expect("the default fixed-step cap is non-zero"),
            max_debt_steps: NonZeroU32::new(Self::DEFAULT_MAX_DEBT_STEPS)
                .expect("the default fixed debt cap is non-zero"),
            catch_up_policy: FixedCatchUpPolicy::default(),
            pending: Duration::ZERO,
            remainder: Duration::ZERO,
            debt: Duration::ZERO,
            delta: Duration::ZERO,
            elapsed: Duration::ZERO,
            tick: 0,
            steps_this_frame: 0,
            capped_this_frame: false,
            discarded_this_frame: Duration::ZERO,
            advance_enabled: true,
            frame_active: false,
        }
    }
}

impl FixedTime {
    pub const DEFAULT_TIMESTEP: Duration = Duration::from_nanos(16_666_667);
    pub const DEFAULT_MAX_STEPS_PER_FRAME: u32 = 5;
    pub const DEFAULT_MAX_DEBT_STEPS: u32 = 120;

    pub fn new(timestep: Duration) -> Result<Self, TimeSettingsError> {
        let mut fixed = Self::default();
        fixed.set_timestep(timestep)?;
        Ok(fixed)
    }

    pub fn with_max_steps_per_frame(
        mut self,
        max_steps_per_frame: u32,
    ) -> Result<Self, TimeSettingsError> {
        self.set_max_steps_per_frame(max_steps_per_frame)?;
        Ok(self)
    }

    pub fn with_max_debt_steps(mut self, max_debt_steps: u32) -> Result<Self, TimeSettingsError> {
        self.set_max_debt_steps(max_debt_steps)?;
        Ok(self)
    }

    #[must_use]
    pub fn with_catch_up_policy(mut self, catch_up_policy: FixedCatchUpPolicy) -> Self {
        self.catch_up_policy = catch_up_policy;
        self
    }

    #[must_use]
    pub fn timestep(&self) -> Duration {
        self.timestep
    }

    #[must_use]
    pub fn max_steps_per_frame(&self) -> u32 {
        self.max_steps_per_frame.get()
    }

    #[must_use]
    pub fn max_debt_steps(&self) -> u32 {
        self.max_debt_steps.get()
    }

    #[must_use]
    pub fn catch_up_policy(&self) -> FixedCatchUpPolicy {
        self.catch_up_policy
    }

    #[must_use]
    pub fn remainder(&self) -> Duration {
        self.remainder
    }

    #[must_use]
    pub fn debt(&self) -> Duration {
        self.debt
    }

    #[must_use]
    pub fn delta(&self) -> Duration {
        self.delta
    }

    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    #[must_use]
    pub fn tick(&self) -> u64 {
        self.tick
    }

    #[must_use]
    pub fn steps_this_frame(&self) -> u32 {
        self.steps_this_frame
    }

    #[must_use]
    pub fn capped_this_frame(&self) -> bool {
        self.capped_this_frame
    }

    #[must_use]
    pub fn discarded_this_frame(&self) -> Duration {
        self.discarded_this_frame
    }

    pub fn set_timestep(&mut self, timestep: Duration) -> Result<(), TimeSettingsError> {
        self.ensure_frame_inactive()?;
        if timestep.is_zero() {
            return Err(TimeSettingsError::ZeroFixedTimestep);
        }
        self.timestep = timestep;
        self.update_pending_parts();
        Ok(())
    }

    pub fn set_max_steps_per_frame(
        &mut self,
        max_steps_per_frame: u32,
    ) -> Result<(), TimeSettingsError> {
        self.ensure_frame_inactive()?;
        self.max_steps_per_frame = NonZeroU32::new(max_steps_per_frame)
            .ok_or(TimeSettingsError::ZeroMaxFixedStepsPerFrame)?;
        Ok(())
    }

    pub fn set_max_debt_steps(&mut self, max_debt_steps: u32) -> Result<(), TimeSettingsError> {
        self.ensure_frame_inactive()?;
        self.max_debt_steps =
            NonZeroU32::new(max_debt_steps).ok_or(TimeSettingsError::ZeroMaxFixedDebtSteps)?;
        Ok(())
    }

    pub fn set_catch_up_policy(
        &mut self,
        catch_up_policy: FixedCatchUpPolicy,
    ) -> Result<(), TimeSettingsError> {
        self.ensure_frame_inactive()?;
        self.catch_up_policy = catch_up_policy;
        Ok(())
    }

    fn ensure_frame_inactive(&self) -> Result<(), TimeSettingsError> {
        if self.frame_active {
            Err(TimeSettingsError::FixedFrameActive)
        } else {
            Ok(())
        }
    }

    fn plan_frame(
        &self,
        delta: Duration,
        advance_enabled: bool,
    ) -> Result<FixedFramePlan, FixedTimeError> {
        let pending = if advance_enabled {
            self.pending
                .checked_add(delta)
                .ok_or(FixedTimeError::PendingDurationOverflow {
                    pending: self.pending,
                    incoming: delta,
                })?
        } else {
            self.pending
        };

        let pending_steps = if advance_enabled {
            pending.as_nanos() / self.timestep.as_nanos()
        } else {
            0
        };
        if advance_enabled && self.catch_up_policy == FixedCatchUpPolicy::PreserveDebt {
            let maximum_pending_steps =
                u128::from(self.max_steps_per_frame.get()) + u128::from(self.max_debt_steps.get());
            if pending_steps > maximum_pending_steps {
                return Err(FixedTimeError::CatchUpDebtExceeded {
                    pending_steps,
                    max_steps_per_frame: self.max_steps_per_frame.get(),
                    max_debt_steps: self.max_debt_steps.get(),
                });
            }
        }

        let steps_to_run = pending_steps.min(u128::from(self.max_steps_per_frame.get())) as u32;
        if self.tick.checked_add(u64::from(steps_to_run)).is_none() {
            return Err(FixedTimeError::TickOverflow {
                tick: self.tick,
                attempted_steps: steps_to_run,
            });
        }
        let elapsed_delta =
            self.timestep
                .checked_mul(steps_to_run)
                .ok_or(FixedTimeError::ElapsedOverflow {
                    elapsed: self.elapsed,
                    timestep: self.timestep,
                    attempted_steps: steps_to_run,
                })?;
        if self.elapsed.checked_add(elapsed_delta).is_none() {
            return Err(FixedTimeError::ElapsedOverflow {
                elapsed: self.elapsed,
                timestep: self.timestep,
                attempted_steps: steps_to_run,
            });
        }

        Ok(FixedFramePlan {
            pending,
            advance_enabled,
            steps_to_run,
        })
    }

    fn begin_frame(&mut self, plan: FixedFramePlan) {
        self.pending = plan.pending;
        self.advance_enabled = plan.advance_enabled;
        self.frame_active = true;
        self.delta = Duration::ZERO;
        self.steps_this_frame = 0;
        self.capped_this_frame = false;
        self.discarded_this_frame = Duration::ZERO;
        self.update_pending_parts();
    }

    fn advance_tick(&mut self) {
        self.pending = self
            .pending
            .checked_sub(self.timestep)
            .expect("fixed pending time is preflighted for the frame");
        self.delta = self.timestep;
        self.elapsed = self
            .elapsed
            .checked_add(self.timestep)
            .expect("fixed elapsed advance is preflighted for the frame");
        self.tick = self
            .tick
            .checked_add(1)
            .expect("fixed tick advance is preflighted for the frame");
        self.steps_this_frame += 1;
        self.update_pending_parts();
    }

    fn finish_frame(&mut self) {
        self.update_pending_parts();
        self.capped_this_frame = self.advance_enabled
            && !self.debt.is_zero()
            && self.steps_this_frame >= self.max_steps_per_frame.get();
        if self.capped_this_frame && self.catch_up_policy == FixedCatchUpPolicy::DiscardExcess {
            self.discarded_this_frame = self.debt;
            self.pending = self.remainder;
            self.debt = Duration::ZERO;
        }
        self.frame_active = false;
    }

    fn update_pending_parts(&mut self) {
        self.remainder = duration_remainder(self.pending, self.timestep);
        self.debt = self.pending.saturating_sub(self.remainder);
    }
}

#[derive(Debug, Clone, Copy)]
struct FixedFramePlan {
    pending: Duration,
    advance_enabled: bool,
    steps_to_run: u32,
}

#[derive(Debug, Clone, Copy)]
struct TimeFramePlan {
    real_time: RealTime,
    virtual_time: VirtualTime,
    fixed_time: FixedTime,
    real_delta: Duration,
    virtual_delta: Duration,
    real_delta_clamped: bool,
    fixed_steps_to_run: u32,
}

impl TimeFramePlan {
    fn from_world(world: &World, real_delta: Duration) -> Result<Self, TimeFrameError> {
        let settings = *required_frame_resource::<RuntimeTimeSettings>(
            world,
            TimeFrameResource::RuntimeTimeSettings,
        )?;
        let real_time = required_frame_resource::<RealTime>(world, TimeFrameResource::RealTime)?
            .plan_advance(real_delta)?;
        let (virtual_delta, real_delta_clamped) = settings.plan_virtual_delta(real_delta)?;
        let virtual_time =
            required_frame_resource::<VirtualTime>(world, TimeFrameResource::VirtualTime)?
                .plan_advance(virtual_delta)?;
        let mut fixed_time =
            *required_frame_resource::<FixedTime>(world, TimeFrameResource::FixedTime)?;
        required_frame_resource::<RenderTime>(world, TimeFrameResource::RenderTime)?;
        required_frame_resource::<RuntimeFrameStatus>(
            world,
            TimeFrameResource::RuntimeFrameStatus,
        )?;
        required_frame_resource::<AppExitRequests>(world, TimeFrameResource::AppExitRequests)?;
        let fixed_frame = fixed_time
            .plan_frame(virtual_delta, settings.fixed_time_enabled())
            .map_err(TimeFrameError::Fixed)?;
        fixed_time.begin_frame(fixed_frame);

        Ok(Self {
            real_time,
            virtual_time,
            fixed_time,
            real_delta,
            virtual_delta,
            real_delta_clamped,
            fixed_steps_to_run: fixed_frame.steps_to_run,
        })
    }

    fn commit(self, world: &mut World) {
        *world.resource_mut::<RealTime>() = self.real_time;
        *world.resource_mut::<VirtualTime>() = self.virtual_time;
        *world.resource_mut::<FixedTime>() = self.fixed_time;
    }
}

fn required_frame_resource<T: Resource>(
    world: &World,
    resource: TimeFrameResource,
) -> Result<&T, TimeFrameError> {
    world
        .get_resource::<T>()
        .ok_or(TimeFrameError::MissingResource { resource })
}

fn duration_remainder(duration: Duration, divisor: Duration) -> Duration {
    let remainder_nanos = duration.as_nanos() % divisor.as_nanos();
    let seconds = u64::try_from(remainder_nanos / 1_000_000_000)
        .expect("a duration remainder always fits in Duration");
    let nanoseconds = u32::try_from(remainder_nanos % 1_000_000_000)
        .expect("subsecond nanoseconds always fit in u32");
    Duration::new(seconds, nanoseconds)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub struct RuntimeFrameStatus {
    pub frame: u64,
    pub real_delta: Duration,
    pub virtual_delta: Duration,
    pub real_delta_clamped: bool,
    pub fixed_steps: u32,
    pub fixed_steps_capped: bool,
    pub fixed_tick: u64,
    pub fixed_elapsed: Duration,
    pub fixed_remainder: Duration,
    pub fixed_debt: Duration,
    pub fixed_discarded: Duration,
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
        let mut schedules: BTreeMap<_, _> = CoreStage::ALL
            .into_iter()
            .map(|stage| (stage, Schedule::new(stage)))
            .collect();
        schedules
            .get_mut(&CoreStage::FixedUpdate)
            .expect("the fixed-update schedule is created above")
            .configure_sets(
                (
                    FixedUpdateSet::Prepare,
                    FixedUpdateSet::Simulate,
                    FixedUpdateSet::Finalize,
                )
                    .chain(),
            );

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

    /// Runs one frame using real elapsed time supplied by the runner.
    ///
    /// Startup is a committed one-time lifecycle phase. The frame clock plan is built from the
    /// resources left by startup; a planning failure does not roll startup back or run it again,
    /// but it commits no clock state, runs no core schedule, and does not clear frame trackers.
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

        let time_frame_plan = TimeFramePlan::from_world(&self.world, real_delta)?;
        time_frame_plan.commit(&mut self.world);
        let mut fixed_time = time_frame_plan.fixed_time;
        let mut frame_status = None;

        for stage in CoreStage::ALL {
            if let Some(schedule) = self.schedules.get_mut(&stage) {
                if stage == CoreStage::FixedUpdate {
                    for _ in 0..time_frame_plan.fixed_steps_to_run {
                        fixed_time.advance_tick();
                        *self.world.resource_mut::<FixedTime>() = fixed_time;
                        schedule.run(&mut self.world);
                    }
                    fixed_time.finish_frame();
                    *self.world.resource_mut::<FixedTime>() = fixed_time;
                    self.world
                        .resource_mut::<RenderTime>()
                        .update_from_fixed(&fixed_time);
                    let status = RuntimeFrameStatus {
                        frame: self.world.resource::<RealTime>().frame,
                        real_delta: time_frame_plan.real_delta,
                        virtual_delta: time_frame_plan.virtual_delta,
                        real_delta_clamped: time_frame_plan.real_delta_clamped,
                        fixed_steps: fixed_time.steps_this_frame(),
                        fixed_steps_capped: fixed_time.capped_this_frame(),
                        fixed_tick: fixed_time.tick(),
                        fixed_elapsed: fixed_time.elapsed(),
                        fixed_remainder: fixed_time.remainder(),
                        fixed_debt: fixed_time.debt(),
                        fixed_discarded: fixed_time.discarded_this_frame(),
                    };
                    *self.world.resource_mut::<RuntimeFrameStatus>() = status;
                    frame_status = Some(status);
                } else {
                    schedule.run(&mut self.world);
                }
            }
        }

        let status = frame_status.expect("the fixed-update stage always exists");
        *self.world.resource_mut::<RuntimeFrameStatus>() = status;
        let exit = self.world.resource_mut::<AppExitRequests>().take();
        self.world.clear_trackers();
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
    use nara_ecs::{
        Commands, Component, DetectChanges, Query, RemovedComponents, Res, ResMut, Resource,
    };
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    #[derive(Debug, Default, Resource)]
    struct Frames(u32);

    #[derive(Debug, Default, Resource)]
    struct StartupRuns(u32);

    #[derive(Debug, Default, Resource)]
    struct FixedObservations(Vec<(u64, Duration, Duration)>);

    #[derive(Debug, Default, Resource)]
    struct FrameStatusObservations(Vec<(u64, u32, u64, Duration, f32)>);

    #[derive(Debug, Default, Resource)]
    struct RemovalCount(u32);

    #[derive(Debug, Component)]
    struct Spawned;

    #[derive(Debug, Component)]
    struct FixedPrepared;

    #[derive(Debug, Component)]
    struct FixedSimulated;

    #[derive(Debug, Component)]
    struct Tracked;

    type TimeStateSnapshot = (
        RealTime,
        VirtualTime,
        FixedTime,
        RenderTime,
        RuntimeFrameStatus,
    );

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

    fn pause_on_startup(
        mut settings: ResMut<RuntimeTimeSettings>,
        mut startup_runs: ResMut<StartupRuns>,
    ) {
        settings.set_paused(true);
        startup_runs.0 += 1;
    }

    fn set_timestep_on_startup(
        mut fixed_time: ResMut<FixedTime>,
        mut startup_runs: ResMut<StartupRuns>,
    ) {
        fixed_time
            .set_timestep(FixedTime::DEFAULT_TIMESTEP * 2)
            .unwrap();
        startup_runs.0 += 1;
    }

    fn configure_preserve_debt_on_startup(
        mut fixed_time: ResMut<FixedTime>,
        mut startup_runs: ResMut<StartupRuns>,
    ) {
        fixed_time.set_max_steps_per_frame(2).unwrap();
        fixed_time.set_max_debt_steps(2).unwrap();
        fixed_time
            .set_catch_up_policy(FixedCatchUpPolicy::PreserveDebt)
            .unwrap();
        startup_runs.0 += 1;
    }

    fn remove_fixed_time_on_startup(world: &mut World) {
        world.resource_mut::<StartupRuns>().0 += 1;
        world.remove_resource::<FixedTime>();
    }

    fn observe_fixed_time(fixed_time: Res<FixedTime>, mut observations: ResMut<FixedObservations>) {
        observations
            .0
            .push((fixed_time.tick(), fixed_time.delta(), fixed_time.elapsed()));
    }

    fn fixed_prepare(mut commands: Commands, mut order: ResMut<Order>) {
        order.0.push("fixed_prepare");
        commands.spawn(FixedPrepared);
    }

    fn fixed_simulate(
        prepared: Query<&FixedPrepared>,
        mut commands: Commands,
        mut order: ResMut<Order>,
    ) {
        assert_eq!(prepared.iter().count(), 1);
        order.0.push("fixed_simulate");
        commands.spawn(FixedSimulated);
    }

    fn fixed_finalize(simulated: Query<&FixedSimulated>, mut order: ResMut<Order>) {
        assert_eq!(simulated.iter().count(), 1);
        order.0.push("fixed_finalize");
    }

    fn observe_frame_status(
        real_time: Res<RealTime>,
        status: Res<RuntimeFrameStatus>,
        render_time: Res<RenderTime>,
        mut observations: ResMut<FrameStatusObservations>,
    ) {
        observations.0.push((
            real_time.frame,
            status.fixed_steps,
            status.fixed_tick,
            status.fixed_remainder,
            render_time.interpolation_alpha,
        ));
    }

    fn record_removals(
        mut removed: RemovedComponents<Tracked>,
        mut removal_count: ResMut<RemovalCount>,
    ) {
        removal_count.0 += u32::try_from(removed.read().count()).unwrap();
    }

    fn time_state(app: &App) -> TimeStateSnapshot {
        (
            *app.world().resource::<RealTime>(),
            *app.world().resource::<VirtualTime>(),
            *app.world().resource::<FixedTime>(),
            *app.world().resource::<RenderTime>(),
            *app.world().resource::<RuntimeFrameStatus>(),
        )
    }

    fn assert_time_frame_error_is_atomic(
        app: &mut App,
        real_delta: Duration,
        expected: TimeFrameError,
    ) {
        let tracked = app.world_mut().unwrap().spawn(Tracked).id();
        let before = time_state(app);
        assert!(
            app.world()
                .entity(tracked)
                .get_ref::<Tracked>()
                .unwrap()
                .is_changed()
        );

        assert_eq!(
            app.run_once(real_delta).unwrap_err(),
            AppRunError::Time { error: expected }
        );

        assert_eq!(time_state(app), before);
        assert!(app.started);
        assert!(
            app.world()
                .entity(tracked)
                .get_ref::<Tracked>()
                .unwrap()
                .is_changed()
        );
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
    fn fixed_clock_advances_before_each_fixed_schedule_iteration() {
        let mut app = App::new();
        app.insert_resource(FixedObservations::default()).unwrap();
        app.add_systems(CoreStage::FixedUpdate, observe_fixed_time)
            .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();

        let step = FixedTime::DEFAULT_TIMESTEP;
        assert_eq!(
            app.world().resource::<FixedObservations>().0,
            [(1, step, step), (2, step, step * 2), (3, step, step * 3),]
        );
    }

    #[test]
    fn fixed_sets_flush_deferred_commands_at_declared_boundaries() {
        let mut app = App::new();
        app.insert_resource(Order::default()).unwrap();
        app.add_systems(
            CoreStage::FixedUpdate,
            fixed_finalize.in_set(FixedUpdateSet::Finalize),
        )
        .unwrap();
        app.add_systems(
            CoreStage::FixedUpdate,
            fixed_simulate.in_set(FixedUpdateSet::Simulate),
        )
        .unwrap();
        app.add_systems(
            CoreStage::FixedUpdate,
            fixed_prepare.in_set(FixedUpdateSet::Prepare),
        )
        .unwrap();

        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(
            app.world().resource::<Order>().0,
            ["fixed_prepare", "fixed_simulate", "fixed_finalize"]
        );
    }

    #[test]
    fn current_fixed_status_is_visible_to_variable_update() {
        let mut app = App::new();
        app.insert_resource(FrameStatusObservations::default())
            .unwrap();
        app.add_systems(CoreStage::Update, observe_frame_status)
            .unwrap();

        let step = FixedTime::DEFAULT_TIMESTEP;
        app.run_once(step * 3 + step / 2).unwrap();

        let observations = &app.world().resource::<FrameStatusObservations>().0;
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].0, 1);
        assert_eq!(observations[0].1, 3);
        assert_eq!(observations[0].2, 3);
        assert_eq!(observations[0].3, step / 2);
        assert!(observations[0].4 < 1.0);
    }

    #[test]
    fn completed_frame_retains_removals_for_systems_then_clears_world_trackers() {
        let mut app = App::new();
        app.insert_resource(RemovalCount::default()).unwrap();
        let (changed_entity, removed_entity) = {
            let world = app.world_mut().unwrap();
            let changed_entity = world.spawn(Tracked).id();
            let removed_entity = world.spawn(Tracked).id();
            world.entity_mut(removed_entity).remove::<Tracked>();
            (changed_entity, removed_entity)
        };
        app.add_systems(CoreStage::Last, record_removals).unwrap();

        app.run_once(Duration::ZERO).unwrap();

        assert_eq!(app.world().resource::<RemovalCount>().0, 1);
        assert!(
            !app.world()
                .entity(changed_entity)
                .get_ref::<Tracked>()
                .unwrap()
                .is_changed()
        );
        assert!(app.world().removed::<Tracked>().next().is_none());
        assert!(app.world().get_entity(removed_entity).is_ok());
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
    fn time_scale_changes_virtual_delta_and_fixed_ticks() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.insert_resource(RuntimeTimeSettings::default().with_time_scale(0.5).unwrap())
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
            RuntimeTimeSettings::default()
                .with_max_delta(Duration::from_millis(1))
                .unwrap(),
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
    fn time_settings_reject_invalid_values_at_construction() {
        assert!(matches!(
            RuntimeTimeSettings::default().with_time_scale(f32::NAN),
            Err(TimeSettingsError::InvalidTimeScale { .. })
        ));
        assert!(matches!(
            RuntimeTimeSettings::default().with_time_scale(f32::INFINITY),
            Err(TimeSettingsError::InvalidTimeScale { .. })
        ));
        assert!(matches!(
            RuntimeTimeSettings::default().with_time_scale(-0.25),
            Err(TimeSettingsError::InvalidTimeScale { .. })
        ));
        assert_eq!(
            RuntimeTimeSettings::default()
                .with_max_delta(Duration::ZERO)
                .unwrap_err(),
            TimeSettingsError::ZeroMaxDelta
        );
        assert_eq!(
            FixedTime::new(Duration::ZERO).unwrap_err(),
            TimeSettingsError::ZeroFixedTimestep
        );
        assert_eq!(
            FixedTime::default()
                .with_max_steps_per_frame(0)
                .unwrap_err(),
            TimeSettingsError::ZeroMaxFixedStepsPerFrame
        );
        assert_eq!(
            FixedTime::default().with_max_debt_steps(0).unwrap_err(),
            TimeSettingsError::ZeroMaxFixedDebtSteps
        );
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
    fn desktop_catch_up_discards_excess_ticks_and_keeps_only_remainder() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.insert_resource(FixedTime::default().with_max_steps_per_frame(2).unwrap())
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();

        let step = FixedTime::DEFAULT_TIMESTEP;
        app.run_once(step * 5 + step / 2).unwrap();

        assert_eq!(app.world().resource::<Frames>().0, 2);
        let fixed = app.world().resource::<FixedTime>();
        assert_eq!(fixed.steps_this_frame(), 2);
        assert_eq!(fixed.debt(), Duration::ZERO);
        assert_eq!(fixed.remainder(), step / 2);
        assert_eq!(fixed.discarded_this_frame(), step * 3);
        assert!(fixed.capped_this_frame());
        let render = app.world().resource::<RenderTime>();
        assert_eq!(render.remainder, step / 2);
        assert!(render.interpolation_alpha < 1.0);
        assert!((render.interpolation_alpha - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn preserve_debt_catch_up_runs_bounded_work_across_frames() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.insert_resource(
            FixedTime::default()
                .with_max_steps_per_frame(2)
                .unwrap()
                .with_catch_up_policy(FixedCatchUpPolicy::PreserveDebt),
        )
        .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();

        let step = FixedTime::DEFAULT_TIMESTEP;
        app.run_once(step * 5 + step / 2).unwrap();

        let fixed = app.world().resource::<FixedTime>();
        assert_eq!(fixed.tick(), 2);
        assert_eq!(fixed.debt(), step * 3);
        assert_eq!(fixed.remainder(), step / 2);
        assert_eq!(fixed.discarded_this_frame(), Duration::ZERO);
        let alpha = app.world().resource::<RenderTime>().interpolation_alpha;
        assert!((alpha - 0.5).abs() < f32::EPSILON);

        app.run_once(Duration::ZERO).unwrap();

        let fixed = app.world().resource::<FixedTime>();
        assert_eq!(fixed.tick(), 4);
        assert_eq!(fixed.debt(), step);
        assert_eq!(fixed.remainder(), step / 2);
        assert_eq!(app.world().resource::<Frames>().0, 4);
    }

    #[test]
    fn preserve_debt_rejects_overload_before_advancing_the_frame() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.insert_resource(
            FixedTime::default()
                .with_max_steps_per_frame(2)
                .unwrap()
                .with_max_debt_steps(2)
                .unwrap()
                .with_catch_up_policy(FixedCatchUpPolicy::PreserveDebt),
        )
        .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();

        let step = FixedTime::DEFAULT_TIMESTEP;
        let error = app.run_once(step * 5).unwrap_err();

        assert_eq!(
            error,
            AppRunError::Time {
                error: TimeFrameError::Fixed(FixedTimeError::CatchUpDebtExceeded {
                    pending_steps: 5,
                    max_steps_per_frame: 2,
                    max_debt_steps: 2,
                }),
            }
        );
        assert_eq!(app.world().resource::<RealTime>().frame, 0);
        assert_eq!(app.world().resource::<FixedTime>().tick(), 0);
        assert_eq!(app.world().resource::<FixedTime>().debt(), Duration::ZERO);
        assert_eq!(app.world().resource::<Frames>().0, 0);

        app.run_once(step * 4).unwrap();
        assert_eq!(app.world().resource::<FixedTime>().tick(), 2);
        assert_eq!(app.world().resource::<FixedTime>().debt(), step * 2);
    }

    #[test]
    fn startup_pause_applies_to_the_first_frame() {
        let mut app = App::new();
        app.insert_resource(StartupRuns::default()).unwrap();
        app.insert_resource(Frames::default()).unwrap();
        app.add_startup_systems(StartupStage::Core, pause_on_startup)
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();
        let step = FixedTime::DEFAULT_TIMESTEP;

        app.run_once(step).unwrap();

        assert_eq!(app.world().resource::<StartupRuns>().0, 1);
        assert_eq!(app.world().resource::<Frames>().0, 0);
        assert_eq!(app.world().resource::<VirtualTime>().delta, Duration::ZERO);
        assert_eq!(app.world().resource::<FixedTime>().tick(), 0);
    }

    #[test]
    fn startup_timestep_applies_to_the_first_frame() {
        let mut app = App::new();
        app.insert_resource(StartupRuns::default()).unwrap();
        app.insert_resource(Frames::default()).unwrap();
        app.add_startup_systems(StartupStage::Core, set_timestep_on_startup)
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();
        let step = FixedTime::DEFAULT_TIMESTEP;

        app.run_once(step * 2).unwrap();

        let fixed_time = app.world().resource::<FixedTime>();
        assert_eq!(app.world().resource::<StartupRuns>().0, 1);
        assert_eq!(app.world().resource::<Frames>().0, 1);
        assert_eq!(fixed_time.timestep(), step * 2);
        assert_eq!(fixed_time.delta(), step * 2);
        assert_eq!(fixed_time.tick(), 1);
    }

    #[test]
    fn startup_time_overload_is_committed_but_does_not_start_a_frame() {
        let mut app = App::new();
        app.insert_resource(StartupRuns::default()).unwrap();
        app.insert_resource(Frames::default()).unwrap();
        app.add_startup_systems(StartupStage::Core, configure_preserve_debt_on_startup)
            .unwrap();
        app.add_systems(CoreStage::Update, count_frame).unwrap();
        let step = FixedTime::DEFAULT_TIMESTEP;

        assert_eq!(
            app.run_once(step * 5).unwrap_err(),
            AppRunError::Time {
                error: TimeFrameError::Fixed(FixedTimeError::CatchUpDebtExceeded {
                    pending_steps: 5,
                    max_steps_per_frame: 2,
                    max_debt_steps: 2,
                }),
            }
        );

        assert!(app.started);
        assert_eq!(app.world().resource::<StartupRuns>().0, 1);
        assert_eq!(app.world().resource::<Frames>().0, 0);
        assert_eq!(*app.world().resource::<RealTime>(), RealTime::default());
        assert_eq!(
            *app.world().resource::<VirtualTime>(),
            VirtualTime::default()
        );
        let fixed_time = app.world().resource::<FixedTime>();
        assert_eq!(fixed_time.tick(), 0);
        assert_eq!(fixed_time.elapsed(), Duration::ZERO);
        assert_eq!(fixed_time.debt(), Duration::ZERO);
        assert!(app.world().resource_ref::<StartupRuns>().is_changed());

        app.world_mut()
            .unwrap()
            .resource_mut::<FixedTime>()
            .set_max_debt_steps(3)
            .unwrap();
        app.run_once(step * 5).unwrap();

        assert_eq!(app.world().resource::<StartupRuns>().0, 1);
        assert_eq!(app.world().resource::<Frames>().0, 1);
    }

    #[test]
    fn missing_startup_time_resource_is_structured_and_retryable() {
        let mut app = App::new();
        app.insert_resource(StartupRuns::default()).unwrap();
        app.insert_resource(Frames::default()).unwrap();
        app.add_startup_systems(StartupStage::Core, remove_fixed_time_on_startup)
            .unwrap();
        app.add_systems(CoreStage::Update, count_frame).unwrap();

        assert_eq!(
            app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap_err(),
            AppRunError::Time {
                error: TimeFrameError::MissingResource {
                    resource: TimeFrameResource::FixedTime,
                },
            }
        );
        assert!(app.started);
        assert_eq!(app.world().resource::<StartupRuns>().0, 1);
        assert_eq!(app.world().resource::<Frames>().0, 0);
        assert_eq!(*app.world().resource::<RealTime>(), RealTime::default());
        assert_eq!(
            *app.world().resource::<VirtualTime>(),
            VirtualTime::default()
        );

        app.insert_resource(FixedTime::default()).unwrap();
        app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(app.world().resource::<StartupRuns>().0, 1);
        assert_eq!(app.world().resource::<Frames>().0, 1);
    }

    #[test]
    fn time_frame_plan_rejects_every_clock_overflow_without_state_changes() {
        let mut scaled_delta_app = App::new();
        scaled_delta_app.run_once(Duration::ZERO).unwrap();
        scaled_delta_app
            .insert_resource(
                RuntimeTimeSettings::default()
                    .with_max_delta(Duration::from_secs(1))
                    .unwrap()
                    .with_time_scale(f32::MAX)
                    .unwrap(),
            )
            .unwrap();
        assert_time_frame_error_is_atomic(
            &mut scaled_delta_app,
            Duration::from_secs(1),
            TimeFrameError::VirtualDeltaOverflow {
                clamped_real_delta: Duration::from_secs(1),
                time_scale: f32::MAX,
            },
        );

        let mut real_elapsed_app = App::new();
        real_elapsed_app.run_once(Duration::ZERO).unwrap();
        *real_elapsed_app
            .world_mut()
            .unwrap()
            .resource_mut::<RealTime>() = RealTime {
            elapsed: Duration::MAX,
            ..RealTime::default()
        };
        assert_time_frame_error_is_atomic(
            &mut real_elapsed_app,
            Duration::from_nanos(1),
            TimeFrameError::RealElapsedOverflow {
                elapsed: Duration::MAX,
                delta: Duration::from_nanos(1),
            },
        );

        let mut real_frame_app = App::new();
        real_frame_app.run_once(Duration::ZERO).unwrap();
        *real_frame_app
            .world_mut()
            .unwrap()
            .resource_mut::<RealTime>() = RealTime {
            frame: u64::MAX,
            ..RealTime::default()
        };
        assert_time_frame_error_is_atomic(
            &mut real_frame_app,
            Duration::ZERO,
            TimeFrameError::RealFrameOverflow { frame: u64::MAX },
        );

        let mut virtual_elapsed_app = App::new();
        virtual_elapsed_app.run_once(Duration::ZERO).unwrap();
        *virtual_elapsed_app
            .world_mut()
            .unwrap()
            .resource_mut::<VirtualTime>() = VirtualTime {
            elapsed: Duration::MAX,
            ..VirtualTime::default()
        };
        assert_time_frame_error_is_atomic(
            &mut virtual_elapsed_app,
            Duration::from_nanos(1),
            TimeFrameError::VirtualElapsedOverflow {
                elapsed: Duration::MAX,
                delta: Duration::from_nanos(1),
            },
        );

        let mut virtual_frame_app = App::new();
        virtual_frame_app.run_once(Duration::ZERO).unwrap();
        *virtual_frame_app
            .world_mut()
            .unwrap()
            .resource_mut::<VirtualTime>() = VirtualTime {
            frame: u64::MAX,
            ..VirtualTime::default()
        };
        assert_time_frame_error_is_atomic(
            &mut virtual_frame_app,
            Duration::ZERO,
            TimeFrameError::VirtualFrameOverflow { frame: u64::MAX },
        );

        let mut fixed_pending_app = App::new();
        fixed_pending_app.run_once(Duration::ZERO).unwrap();
        {
            let mut fixed = fixed_pending_app
                .world_mut()
                .unwrap()
                .resource_mut::<FixedTime>();
            fixed.pending = Duration::MAX;
            fixed.update_pending_parts();
        }
        assert_time_frame_error_is_atomic(
            &mut fixed_pending_app,
            FixedTime::DEFAULT_TIMESTEP,
            TimeFrameError::Fixed(FixedTimeError::PendingDurationOverflow {
                pending: Duration::MAX,
                incoming: FixedTime::DEFAULT_TIMESTEP,
            }),
        );
    }

    #[test]
    fn fixed_clock_rejects_tick_or_elapsed_overflow_before_schedules_run() {
        let step = FixedTime::DEFAULT_TIMESTEP;

        let mut tick_app = App::new();
        tick_app.insert_resource(Frames::default()).unwrap();
        let tick_clock = FixedTime {
            tick: u64::MAX - 1,
            ..FixedTime::default()
        };
        tick_app.insert_resource(tick_clock).unwrap();
        tick_app
            .add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();
        assert_eq!(
            tick_app.run_once(step * 2).unwrap_err(),
            AppRunError::Time {
                error: TimeFrameError::Fixed(FixedTimeError::TickOverflow {
                    tick: u64::MAX - 1,
                    attempted_steps: 2,
                }),
            }
        );
        assert_eq!(tick_app.world().resource::<Frames>().0, 0);
        assert_eq!(tick_app.world().resource::<RealTime>().frame, 0);

        let mut elapsed_app = App::new();
        elapsed_app.insert_resource(Frames::default()).unwrap();
        let elapsed_clock = FixedTime {
            elapsed: Duration::MAX - step,
            ..FixedTime::default()
        };
        elapsed_app.insert_resource(elapsed_clock).unwrap();
        elapsed_app
            .add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();
        assert_eq!(
            elapsed_app.run_once(step * 2).unwrap_err(),
            AppRunError::Time {
                error: TimeFrameError::Fixed(FixedTimeError::ElapsedOverflow {
                    elapsed: Duration::MAX - step,
                    timestep: step,
                    attempted_steps: 2,
                }),
            }
        );
        assert_eq!(elapsed_app.world().resource::<Frames>().0, 0);
        assert_eq!(elapsed_app.world().resource::<RealTime>().frame, 0);
    }

    #[test]
    fn paused_and_zero_scale_frames_preserve_debt_without_running_ticks() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.insert_resource(
            FixedTime::default()
                .with_max_steps_per_frame(2)
                .unwrap()
                .with_catch_up_policy(FixedCatchUpPolicy::PreserveDebt),
        )
        .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();

        let step = FixedTime::DEFAULT_TIMESTEP;
        app.run_once(step * 5).unwrap();
        assert_eq!(app.world().resource::<FixedTime>().debt(), step * 3);

        app.insert_resource(RuntimeTimeSettings::default().with_paused(true))
            .unwrap();
        app.run_once(Duration::ZERO).unwrap();
        assert_eq!(app.world().resource::<FixedTime>().tick(), 2);
        assert_eq!(app.world().resource::<FixedTime>().debt(), step * 3);

        app.insert_resource(RuntimeTimeSettings::default().with_time_scale(0.0).unwrap())
            .unwrap();
        app.run_once(Duration::ZERO).unwrap();
        assert_eq!(app.world().resource::<FixedTime>().tick(), 2);
        assert_eq!(app.world().resource::<FixedTime>().debt(), step * 3);

        app.insert_resource(RuntimeTimeSettings::default()).unwrap();
        app.run_once(Duration::ZERO).unwrap();
        assert_eq!(app.world().resource::<FixedTime>().tick(), 4);
        assert_eq!(app.world().resource::<FixedTime>().debt(), step);
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
    fn run_preserves_runner_teardown_when_plugin_cleanup_also_fails() {
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
        let runner_error = AppRunError::runner_teardown(
            AppRunError::runner("runner failed"),
            AppRunError::runner("native teardown failed"),
        );
        let expected_runner_error = runner_error.clone();
        app.set_runner(move |_app| Err(runner_error)).unwrap();

        let error = app.run().unwrap_err();

        let AppRunError::Shutdown { prior, report } = &error else {
            panic!("runner, teardown, and cleanup failures should remain nested");
        };
        assert_eq!(prior.as_deref(), Some(&expected_runner_error));
        assert!(report.primary().is_none());
        assert_eq!(report.cleanup_failures().len(), 1);
        assert_eq!(error.plugin_failure_report(), Some(report.as_ref()));
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
    fn runner_teardown_accessors_search_both_nested_errors() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let mut report_app = App::new();
        report_app
            .add_plugin(LifecycleProbePlugin {
                id: PROBE_A_PLUGIN_ID,
                behavior: ProbeBehavior {
                    fail_cleanup: true,
                    ..ProbeBehavior::default()
                },
                trace,
            })
            .unwrap();
        let PluginCleanupError::Failure(report) = report_app.cleanup_plugins().unwrap_err() else {
            panic!("test plugin should produce a cleanup report");
        };
        let report = *report;
        let plugin_error = report.cleanup_failures()[0].error().clone();
        let nested_plugin = AppRunError::plugin(plugin_error.clone(), Some(report.clone()));

        for error in [
            AppRunError::runner_teardown(
                nested_plugin.clone(),
                AppRunError::runner("teardown failed"),
            ),
            AppRunError::runner_teardown(
                AppRunError::runner("runner failed"),
                nested_plugin.clone(),
            ),
        ] {
            assert_eq!(error.plugin_error(), Some(&plugin_error));
            assert_eq!(error.plugin_failure_report(), Some(&report));
        }
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
