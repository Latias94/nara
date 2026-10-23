//! Application lifecycle and plugin orchestration for nara.
//!
//! Configure an [`App`], consume it with [`RuntimeInstance::start`], and advance the runtime with
//! [`RuntimeInstance::drive`]. Embedded hosts request controls and keep driving until physical close
//! completes; finite headless programs can consume the runtime with [`RuntimeInstance::finish`].
//! Staged admission and publication remain public only for first-party cross-crate Host composition.

use std::{
    collections::BTreeSet,
    fmt::{self, Display, Formatter},
    num::NonZeroU32,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    time::{Duration, Instant},
};

use nara_ecs::{
    Resource, World,
    bevy_ecs::error::{ErrorHandler, FallbackErrorHandler},
    bevy_ecs::observer::IntoObserver,
    bevy_ecs::schedule::{InternedScheduleLabel, InternedSystemSet, ScheduleBuildSettings},
    bevy_ecs::system::ScheduleSystem,
    schedule::{IntoScheduleConfigs, Schedule, ScheduleLabel, SystemSet},
};
use thiserror::Error;

mod failure_metadata;
mod plugin;
mod runtime;

pub use plugin::{
    AddPluginsError, EditedPluginGroup, EditedPluginGroupMarker, Plugin, PluginCategory,
    PluginCommitError, PluginConfigurationFingerprint, PluginDeclaration, PluginDefinition,
    PluginDefinitionId, PluginDefinitionKey, PluginDomainError, PluginDomainErrorError,
    PluginError, PluginFailure, PluginFailureReport, PluginGroup, PluginGroupBuilder,
    PluginGroupId, PluginHook, PluginHookMutation, PluginId, PluginInstantiationError,
    PluginLifecycleState, PluginPlan, PluginPlanEntry, PluginPlanError, PluginPlanFingerprint,
    PluginPreflightContext, PluginPreflightResource, PluginPrepareError, PluginProductCapability,
    PluginSchemaProviderId, PluginShutdownContext, PluginShutdownError, PluginShutdownObligationId,
    PluginSlot, PluginSlotId, PluginSlotPresence, Plugins, ReplayablePlugins, ResolvedPluginGroup,
    RetainedPluginInstantiationFailure, RuntimeConstructionError, RuntimeConstructionFailure,
    SealedApp,
};
pub use runtime::*;

#[doc(hidden)]
pub mod __private {
    use nara_ecs::schedule::ScheduleLabel;

    /// Engine-owned transient retirement after all frame or exact-step observers have completed.
    /// This is not a public gameplay ordering anchor.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
    pub struct FrameCleanup;
}

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
    /// Public schedule-label anchor for one authoritative fixed-tick transaction.
    ///
    /// External systems may register in this schedule. Before each entry, [`FixedTime`] exposes the
    /// tick, delta, and elapsed time for the tick being simulated. A normal frame skips the schedule
    /// when no fixed step is due, and may run it multiple times when catching up; an exact fixed step
    /// runs it once.
    ///
    /// [`App::seal`] validates this schedule's graph, requires automatic deferred insertion, and
    /// restores final deferred application. Consequently, ordinary deferred commands are visible at
    /// declared set boundaries and all remaining deferred commands are applied before the schedule
    /// completes. An explicit ignore-deferred relation opts out of that visibility contract.
    ///
    /// System and run-condition errors follow the App's configured error policy. Managed-runtime
    /// escalation is a separate [`RuntimeInstance`] contract. World change trackers and frame
    /// transients are retained until the enclosing frame or exact-step transaction completes.
    FixedUpdate,
    Update,
    PostUpdate,
    Extract,
    Prepare,
    Queue,
    Sort,
    Render,
    /// Public schedule-label anchor for frame-end observation and cleanup.
    ///
    /// This schedule runs after the renderer has completed its current-frame work and before
    /// [`CoreStage::Last`]. It still runs while the runtime is paused, so platform and diagnostic
    /// adapters may observe a skipped render or retire frame-transient state without advancing
    /// simulation time. Systems registered here must not assume that a render submission occurred.
    ///
    /// [`App::seal`] applies the same automatic deferred-command and final-flush validation used by
    /// [`CoreStage::FixedUpdate`]. Deferred commands become visible only at the schedule's
    /// configured boundaries and all remaining commands are applied before the schedule
    /// completes. Errors follow the App's configured policy; managed-runtime escalation remains a
    /// separate [`RuntimeInstance`] contract.
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

fn is_built_in_schedule(schedule: InternedScheduleLabel) -> bool {
    StartupStage::ALL
        .into_iter()
        .any(|stage| stage.intern() == schedule)
        || CoreStage::ALL
            .into_iter()
            .any(|stage| stage.intern() == schedule)
        || __private::FrameCleanup.intern() == schedule
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum FixedUpdateSet {
    /// Admit tick-scoped inputs and prepare simulation data.
    ///
    /// This engine-owned phase is not a first-playable public ordering anchor.
    Prepare,
    /// Public joinable phase for authoritative fixed-step simulation.
    ///
    /// The phase begins after [`FixedUpdateSet::Prepare`] completes and its ordinary deferred
    /// commands are applied. It inherits [`CoreStage::FixedUpdate`]'s run/skip and error behavior,
    /// has no additional run condition, and retains frame transients. Ordinary deferred commands
    /// produced by members are applied before [`FixedUpdateSet::Finalize`] begins.
    ///
    /// Membership does not order peers inside this phase. Extensions must declare any additional
    /// semantic relation they require.
    Simulate,
    /// Publish tick-scoped outcomes after simulation commands are flushed.
    ///
    /// This engine-owned phase is not a first-playable public ordering anchor.
    Finalize,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ScheduleCompatibilityError {
    #[error("public schedule {schedule:?} requires automatic deferred insertion")]
    AutomaticDeferredInsertionDisabled { schedule: CoreStage },
}

/// An installed schedule failed graph admission before any Startup systems could execute.
///
/// Every installed graph is admitted, including custom schedules without public ordering anchors.
/// Anchor-specific deferred-command policy remains a separate [`ScheduleCompatibilityError`].
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ScheduleGraphError {
    #[error("schedule {schedule:?} failed to build: {message}")]
    BuildFailed {
        schedule: InternedScheduleLabel,
        message: String,
    },
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
    #[error("managed runtime faulted: kind={kind:?}, source={fault_source}")]
    ManagedRuntime {
        kind: RuntimeFaultKind,
        fault_source: &'static str,
        detail: Option<RuntimeFaultDetail>,
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
    pub const fn managed_runtime(kind: RuntimeFaultKind, source: &'static str) -> Self {
        Self::ManagedRuntime {
            kind,
            fault_source: source,
            detail: None,
        }
    }

    /// Converts a canonical runtime fault into a managed-runtime app error without losing its
    /// validated diagnostic metadata.
    #[must_use]
    pub const fn managed_runtime_fault(fault: &RuntimeFault) -> Self {
        Self::ManagedRuntime {
            kind: fault.kind(),
            fault_source: fault.source(),
            detail: fault.detail(),
        }
    }

    /// Returns validated engine-owned diagnostic metadata retained by a runtime failure.
    #[must_use]
    pub fn runtime_fault_detail(&self) -> Option<RuntimeFaultDetail> {
        self.runtime_fault_parts().and_then(|(_, _, detail)| detail)
    }

    pub(crate) fn runtime_fault_parts(
        &self,
    ) -> Option<(RuntimeFaultKind, &'static str, Option<RuntimeFaultDetail>)> {
        match self {
            Self::ManagedRuntime {
                kind,
                fault_source,
                detail,
            } => Some((*kind, *fault_source, *detail)),
            Self::RunnerTeardown { prior, teardown } => prior
                .runtime_fault_parts()
                .or_else(|| teardown.runtime_fault_parts()),
            Self::Plugin { .. } | Self::Runner { .. } | Self::Time { .. } => None,
        }
    }

    #[must_use]
    pub const fn plugin_error(&self) -> Option<&PluginError> {
        match self {
            Self::Plugin { error, .. } => Some(error),
            Self::RunnerTeardown { prior, teardown } => match prior.plugin_error() {
                Some(error) => Some(error),
                None => teardown.plugin_error(),
            },
            Self::Runner { .. } | Self::Time { .. } | Self::ManagedRuntime { .. } => None,
        }
    }

    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        match self {
            Self::Plugin { report, .. } => report.as_deref(),
            Self::RunnerTeardown { prior, teardown } => prior
                .plugin_failure_report()
                .or_else(|| teardown.plugin_failure_report()),
            Self::Runner { .. } | Self::Time { .. } | Self::ManagedRuntime { .. } => None,
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

/// Process-local start boundary for the currently executing App frame.
///
/// The App publishes this resource after the frame clock plan succeeds and before any
/// [`CoreStage`] schedule runs. The timestamp includes frame-plan work but excludes one-time
/// startup. It exists for correlated diagnostics and must not enter persistent data or gameplay
/// decisions.
#[derive(Clone, Copy, Debug, Resource)]
pub struct FrameExecutionStart {
    frame: u64,
    started_at: Instant,
}

impl FrameExecutionStart {
    const fn new(frame: u64, started_at: Instant) -> Self {
        Self { frame, started_at }
    }

    #[must_use]
    pub const fn frame(self) -> u64 {
        self.frame
    }

    #[must_use]
    pub const fn started_at(self) -> Instant {
        self.started_at
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

    fn begin_exact_step(&self) -> Result<Self, FixedTimeError> {
        let tick = self
            .tick
            .checked_add(1)
            .ok_or(FixedTimeError::TickOverflow {
                tick: self.tick,
                attempted_steps: 1,
            })?;
        let elapsed =
            self.elapsed
                .checked_add(self.timestep)
                .ok_or(FixedTimeError::ElapsedOverflow {
                    elapsed: self.elapsed,
                    timestep: self.timestep,
                    attempted_steps: 1,
                })?;
        let mut exact = *self;
        exact.delta = self.timestep;
        exact.elapsed = elapsed;
        exact.tick = tick;
        exact.steps_this_frame = 1;
        exact.capped_this_frame = false;
        exact.discarded_this_frame = Duration::ZERO;
        exact.frame_active = true;
        Ok(exact)
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

    fn finish_exact_step(&mut self) {
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
    fn from_world(
        world: &World,
        real_delta: Duration,
        force_paused: bool,
    ) -> Result<Self, TimeFrameError> {
        let mut settings = *required_frame_resource::<RuntimeTimeSettings>(
            world,
            TimeFrameResource::RuntimeTimeSettings,
        )?;
        if force_paused {
            settings.set_paused(true);
        }
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
    plugin_id: PluginId,
    shutdown_complete: bool,
}

struct InstalledRuntimeAuthorityValidator {
    plugin: PluginId,
    validator: Arc<runtime::__RuntimeAuthorityValidator>,
}

/// Configures one thread-bound World and its managed execution definition.
///
/// Build a fresh App on the thread that will drive and retire it. Replayable plugin definitions
/// and immutable configuration can be transferred before App construction.
pub struct App {
    owner: runtime::RuntimeOwner,
    plugin_entries: Vec<PluginPlanEntry>,
    plugin_definition_witnesses: Vec<plugin::PluginDefinitionWitness>,
    plugin_groups: Vec<ResolvedPluginGroup>,
    disabled_plugin_slots: BTreeSet<PluginSlotId>,
    plugin_plan_fingerprint: PluginPlanFingerprint,
    registered_shutdown_obligations: BTreeSet<(PluginId, PluginShutdownObligationId)>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    #[must_use]
    pub fn new() -> Self {
        Self {
            owner: runtime::RuntimeOwner::new(),
            plugin_entries: Vec::new(),
            plugin_definition_witnesses: Vec::new(),
            plugin_groups: Vec::new(),
            disabled_plugin_slots: BTreeSet::new(),
            plugin_plan_fingerprint: plugin::empty_plan_fingerprint(),
            registered_shutdown_obligations: BTreeSet::new(),
        }
    }

    pub(crate) fn new_with_runtime_obligations(
        runtime_obligations: RuntimeObligationLedger,
    ) -> Self {
        let mut app = Self::new();
        app.owner.obligations = runtime_obligations;
        app
    }

    #[must_use]
    pub fn world(&self) -> &World {
        &self.owner.world
    }

    #[must_use]
    pub fn plugin_lifecycle_state(&self) -> PluginLifecycleState {
        self.owner.plugin_lifecycle
    }

    #[must_use]
    pub fn plugin_failure_report(&self) -> Option<&PluginFailureReport> {
        self.owner.plugin_failure_report.as_ref()
    }

    pub fn world_mut(&mut self) -> Result<&mut World, PluginError> {
        self.ensure_mutation_allowed()?;
        Ok(&mut self.owner.world)
    }

    /// Installs the one hidden runtime authority validator owned by a domain plugin.
    ///
    /// This is product plumbing rather than a public extension bus. The callback is invoked at
    /// App and managed schedule boundaries and receives the immutable World view plus an admitted
    /// runtime generation when one exists. Domain state and fault ownership remain inside the
    /// registering crate.
    #[doc(hidden)]
    pub fn __install_runtime_authority_validator(
        &mut self,
        plugin: PluginId,
        validator: Arc<runtime::__RuntimeAuthorityValidator>,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_mutation_allowed()?;
        if self.owner.runtime_authority_validator.is_some() {
            return Err(PluginError::SetupFailed {
                plugin,
                message: "a runtime authority validator is already installed".to_owned(),
            });
        }
        self.owner.runtime_authority_validator =
            Some(InstalledRuntimeAuthorityValidator { plugin, validator });
        Ok(self)
    }

    pub fn insert_resource<R: Resource>(&mut self, resource: R) -> Result<&mut Self, PluginError> {
        self.ensure_mutation_allowed()?;
        self.owner.world.insert_resource(resource);
        Ok(self)
    }

    pub fn init_resource<R>(&mut self) -> Result<&mut Self, PluginError>
    where
        R: Resource + nara_ecs::bevy_ecs::world::FromWorld,
    {
        self.ensure_mutation_allowed()?;
        self.owner.world.init_resource::<R>();
        Ok(self)
    }

    /// Registers systems with Nara's schedule set.
    ///
    /// Managed runtimes route unhandled schedule, run-condition, and default command failures into
    /// the sticky runtime fault channel. Admission binds the canonical reporter and one engine
    /// error handler before startup. Later bridge mutation faults closed.
    /// Explicit per-system or per-command handlers remain caller-owned error policy.
    pub fn add_systems<M>(
        &mut self,
        schedule: impl ScheduleLabel,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_configuration_mutation_allowed()?;
        self.owner.schedules.add_systems(schedule, systems);
        Ok(self)
    }

    /// Registers an observer whose unhandled failures enter the managed runtime fault channel.
    ///
    /// Nara materializes registered observers after the final managed error authority is known.
    /// Admission binds each observer to its reserved runtime route before startup and content checks.
    /// Registered observers become active only at that first execution or admission boundary, so
    /// they do not observe events triggered while the App is still being configured. Callers that
    /// need a different error policy should build and drive that observer outside Nara's managed
    /// fault contract.
    pub fn add_observer<M>(
        &mut self,
        observer: impl IntoObserver<M>,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_configuration_mutation_allowed()?;
        self.owner.pending_observers.push(observer.into_observer());
        Ok(self)
    }

    pub fn configure_sets<M>(
        &mut self,
        schedule: impl ScheduleLabel,
        sets: impl IntoScheduleConfigs<InternedSystemSet, M>,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_configuration_mutation_allowed()?;
        self.owner.schedules.configure_sets(schedule, sets);
        Ok(self)
    }

    /// Requires an installed owner's readiness before advancing simulation.
    ///
    /// Conditions read the World once at frame entry. If any condition returns `false`, the frame
    /// uses the existing paused policy: real time, tasks, rendering and cleanup advance, while
    /// virtual/fixed time and PreUpdate/FixedUpdate/Update/PostUpdate remain suspended. Readiness
    /// restored during that frame takes effect on the next frame. Exact stepping is also refused.
    ///
    /// An owner must queue irreversible transitions before the next frame boundary rather than
    /// changing readiness midway through simulation. Conditions own no mutable World or history.
    pub fn add_simulation_condition(
        &mut self,
        condition: fn(&World) -> bool,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_configuration_mutation_allowed()?;
        self.owner.simulation_conditions.push(condition);
        Ok(self)
    }

    pub fn init_schedule(
        &mut self,
        schedule: impl ScheduleLabel,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_configuration_mutation_allowed()?;
        self.owner.schedules.entry(schedule);
        Ok(self)
    }

    /// Replaces the build settings for a registered or newly initialized schedule.
    ///
    /// Every installed graph is built fallibly at sealing. Public-anchor schedules also remain
    /// subject to Nara's deferred-command compatibility policy. This is the controlled configuration
    /// path for build policy; raw mutable access is reserved for custom schedules so callers cannot
    /// replace a built-in executor, graph, or build-pass inventory.
    pub fn set_schedule_build_settings(
        &mut self,
        schedule: impl ScheduleLabel,
        settings: ScheduleBuildSettings,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_configuration_mutation_allowed()?;
        self.owner
            .schedules
            .entry(schedule)
            .set_build_settings(settings);
        Ok(self)
    }

    /// Sets the final deferred-application policy before sealing.
    ///
    /// Seal reasserts final deferred application for public-anchor schedules.
    pub fn set_schedule_apply_final_deferred(
        &mut self,
        schedule: impl ScheduleLabel,
        apply_final_deferred: bool,
    ) -> Result<&mut Self, PluginError> {
        self.ensure_configuration_mutation_allowed()?;
        self.owner
            .schedules
            .entry(schedule)
            .set_apply_final_deferred(apply_final_deferred);
        Ok(self)
    }

    #[must_use]
    pub fn get_schedule(&self, schedule: impl ScheduleLabel) -> Option<&Schedule> {
        self.owner.schedules.get(schedule)
    }

    /// Returns raw mutable access to a custom schedule before App sealing.
    ///
    /// Engine-owned startup and frame schedules must be configured through [`App::add_systems`],
    /// [`App::configure_sets`], and the controlled schedule policy methods. Exposing their complete
    /// [`Schedule`] would allow replacing the executor, graph, and build passes after Nara installs
    /// its semantic anchors.
    pub fn get_schedule_mut(
        &mut self,
        schedule: impl ScheduleLabel,
    ) -> Result<Option<&mut Schedule>, PluginError> {
        self.ensure_configuration_mutation_allowed()?;
        let schedule = schedule.intern();
        if is_built_in_schedule(schedule) {
            return Err(PluginError::RawBuiltInScheduleMutationForbidden);
        }
        Ok(self.owner.schedules.get_mut(schedule))
    }

    pub fn add_plugins<M>(
        &mut self,
        plugins: impl Plugins<M>,
    ) -> Result<&mut Self, AddPluginsError> {
        plugin::install_plugins(self, plugins)?;
        Ok(self)
    }

    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> Result<&mut Self, AddPluginsError> {
        self.add_plugins(plugin)
    }

    #[must_use]
    pub fn has_plugin(&self, id: PluginId) -> bool {
        self.plugin_entries
            .iter()
            .any(|entry| entry.plugin_id() == id)
    }

    pub fn installed_plugins(&self) -> impl Iterator<Item = PluginDeclaration> + '_ {
        self.plugin_entries.iter().map(|entry| *entry.declaration())
    }

    pub fn installed_plugin_entries(&self) -> impl Iterator<Item = &PluginPlanEntry> {
        self.plugin_entries.iter()
    }

    pub fn installed_plugin_groups(&self) -> impl Iterator<Item = &ResolvedPluginGroup> {
        self.plugin_groups.iter()
    }

    #[must_use]
    pub const fn configuration_fingerprint(&self) -> PluginPlanFingerprint {
        self.plugin_plan_fingerprint
    }

    pub fn register_plugin_shutdown_obligation(
        &mut self,
        obligation: PluginShutdownObligationId,
    ) -> Result<&mut Self, PluginError> {
        let Some((plugin, PluginHook::Build)) = self.owner.active_plugin_hook else {
            return Err(PluginError::ShutdownObligationOutsideBuild);
        };
        let declared = self
            .plugin_entries
            .iter()
            .find(|entry| entry.plugin_id() == plugin)
            .is_some_and(|entry| {
                entry
                    .declaration()
                    .shutdown_obligations
                    .contains(&obligation)
            });
        if !declared {
            let error = PluginError::UndeclaredShutdownObligation { plugin, obligation };
            self.poison(plugin, PluginHook::Build, error.clone());
            return Err(error);
        }
        if !self
            .registered_shutdown_obligations
            .insert((plugin, obligation))
        {
            let error = PluginError::DuplicateShutdownObligation { plugin, obligation };
            self.poison(plugin, PluginHook::Build, error.clone());
            return Err(error);
        }
        Ok(self)
    }

    /// Takes custody of a close participant, even if plugin registration is rejected.
    ///
    /// Rejected participants remain owned by this App's retirement protocol; registration never
    /// starts their callbacks. The standalone [`RuntimeObligationLedger::register`] instead
    /// returns the participant when its caller has not transferred runtime cleanup responsibility.
    pub fn register_plugin_runtime_close_participant<P>(
        &mut self,
        obligation: PluginShutdownObligationId,
        participant_id: RuntimeCloseParticipantId,
        participant: P,
    ) -> Result<&mut Self, PluginError>
    where
        P: RuntimeCloseParticipant,
    {
        let Some((plugin, PluginHook::Build)) = self.owner.active_plugin_hook else {
            self.owner
                .obligations
                .retain_for_retirement(participant_id, participant);
            return Err(PluginError::ShutdownObligationOutsideBuild);
        };
        if let Err(error) = self.register_plugin_shutdown_obligation(obligation) {
            self.owner
                .obligations
                .retain_for_retirement(participant_id, participant);
            return Err(error);
        }
        if let Err(failure) = self.owner.obligations.register(participant_id, participant) {
            let (ledger_error, participant) = failure.into_parts();
            self.owner
                .obligations
                .retain_for_retirement(participant_id, participant);
            let participant_id = match ledger_error {
                RuntimeObligationLedgerError::Duplicate { id } => id,
            };
            let error = PluginError::DuplicateRuntimeCloseParticipant {
                plugin,
                obligation,
                participant_id,
            };
            self.poison(plugin, PluginHook::Build, error.clone());
            return Err(error);
        }
        Ok(self)
    }

    /// Freezes configuration and admits every installed schedule graph without executing startup.
    ///
    /// Ordinary execution consumes configuration directly through [`RuntimeInstance::start`].
    #[doc(hidden)]
    pub fn seal(mut self) -> Result<SealedApp, PluginCommitError> {
        if let Err(error) = self.seal_internal() {
            return Err(PluginCommitError::recorded(
                error,
                self.plugin_failure_report()
                    .and_then(PluginFailureReport::primary)
                    .cloned(),
            ));
        }
        Ok(SealedApp { app: self })
    }

    pub(crate) fn seal_internal(&mut self) -> Result<(), PluginError> {
        if self.owner.active_plugin_hook.is_some() {
            return Err(PluginError::FinishReentered);
        }
        match self.owner.plugin_lifecycle {
            PluginLifecycleState::Ready => return Ok(()),
            PluginLifecycleState::Poisoned => return Err(self.primary_plugin_error()),
            PluginLifecycleState::ShuttingDown | PluginLifecycleState::ShutdownComplete => {
                return Err(PluginError::LifecycleShutdown);
            }
            PluginLifecycleState::Finishing => {
                return Err(PluginError::FinishReentered);
            }
            PluginLifecycleState::Configuring => {}
        }

        for entry in &self.plugin_entries {
            for obligation in entry.declaration().shutdown_obligations {
                if !self
                    .registered_shutdown_obligations
                    .contains(&(entry.plugin_id(), *obligation))
                {
                    let error = PluginError::MissingShutdownObligation {
                        plugin: entry.plugin_id(),
                        obligation: *obligation,
                    };
                    self.poison(entry.plugin_id(), PluginHook::Build, error.clone());
                    self.owner.shutdown_plugins_internal();
                    return Err(error);
                }
            }
        }

        self.owner.plugin_lifecycle = PluginLifecycleState::Finishing;
        for index in 0..self.owner.plugins.len() {
            let plugin = Arc::clone(&self.owner.plugins[index].plugin);
            let plugin_id = self.owner.plugins[index].plugin_id;
            self.owner.active_plugin_hook = Some((plugin_id, PluginHook::Finish));
            let result = catch_unwind(AssertUnwindSafe(|| plugin.finish(self)))
                .map_err(|_| PluginError::HookPanicked {
                    plugin: plugin_id,
                    hook: PluginHook::Finish,
                })
                .and_then(|result| result);
            self.owner.active_plugin_hook = None;
            if let Err(error) = result {
                self.poison(plugin_id, PluginHook::Finish, error);
                break;
            }
            if self.owner.plugin_lifecycle == PluginLifecycleState::Poisoned {
                break;
            }
        }

        if self.owner.plugin_lifecycle == PluginLifecycleState::Finishing {
            if let Err(error) = self
                .validate_public_schedule_compatibility()
                .and_then(|()| self.initialize_installed_schedules())
            {
                self.owner.shutdown_plugins_internal();
                return Err(error);
            }
            let authority_failure =
                self.owner
                    .runtime_authority_validator
                    .as_ref()
                    .and_then(|owner| {
                        (owner.validator)(&self.owner.world, None)
                            .err()
                            .map(|fault| (owner.plugin, fault))
                    });
            if let Some((plugin, fault)) = authority_failure {
                let error = PluginError::SetupFailed {
                    plugin,
                    message: format!(
                        "runtime authority validation failed: kind={:?}, source={}",
                        fault.kind(),
                        fault.source()
                    ),
                };
                self.poison(plugin, PluginHook::Finish, error.clone());
                self.owner.shutdown_plugins_internal();
                return Err(error);
            }
            self.owner.plugin_lifecycle = PluginLifecycleState::Ready;
            return Ok(());
        }

        self.owner.shutdown_plugins_internal();
        Err(self.primary_plugin_error())
    }

    fn validate_public_schedule_compatibility(&mut self) -> Result<(), PluginError> {
        let state = &mut *self.owner;
        for schedule in [CoreStage::FixedUpdate, CoreStage::Cleanup] {
            let public_schedule = state
                .schedules
                .get_mut(schedule)
                .expect("public schedules remain registered for the App lifetime");
            let build_settings = public_schedule.get_build_settings();
            if !build_settings.auto_insert_apply_deferred {
                return Err(
                    ScheduleCompatibilityError::AutomaticDeferredInsertionDisabled { schedule }
                        .into(),
                );
            }
            public_schedule.set_build_settings(build_settings);
            public_schedule.set_apply_final_deferred(true);
        }
        Ok(())
    }

    fn initialize_installed_schedules(&mut self) -> Result<(), PluginError> {
        let state = &mut *self.owner;
        for (_, schedule) in state.schedules.iter_mut() {
            if let Err(error) = schedule.initialize(&mut state.world) {
                return Err(ScheduleGraphError::BuildFailed {
                    schedule: schedule.label(),
                    message: error.to_string(schedule.graph(), &state.world),
                }
                .into());
            }
        }
        Ok(())
    }

    fn ensure_mutation_allowed(&self) -> Result<(), PluginError> {
        match self.owner.plugin_lifecycle {
            PluginLifecycleState::Poisoned => Err(self.primary_plugin_error()),
            PluginLifecycleState::ShuttingDown | PluginLifecycleState::ShutdownComplete => {
                Err(PluginError::LifecycleShutdown)
            }
            PluginLifecycleState::Configuring
            | PluginLifecycleState::Finishing
            | PluginLifecycleState::Ready => Ok(()),
        }
    }

    fn ensure_configuration_mutation_allowed(&self) -> Result<(), PluginError> {
        self.ensure_mutation_allowed()?;
        if self.owner.plugin_lifecycle == PluginLifecycleState::Ready {
            Err(PluginError::AppSealed)
        } else {
            Ok(())
        }
    }

    fn primary_plugin_error(&self) -> PluginError {
        self.owner
            .plugin_failure_report
            .as_ref()
            .and_then(|report| report.primary.as_ref())
            .map(|failure| failure.error().clone())
            .unwrap_or(PluginError::LifecyclePoisoned)
    }

    fn poison(&mut self, plugin: PluginId, hook: PluginHook, error: PluginError) {
        let report = self
            .owner
            .plugin_failure_report
            .get_or_insert_with(|| PluginFailureReport {
                primary: None,
                shutdown_failures: Vec::new(),
                shutdown_complete: false,
            });
        if report.primary.is_none() {
            report.primary = Some(plugin::failure(plugin, hook, error));
        }
        self.owner.plugin_lifecycle = PluginLifecycleState::Poisoned;
    }

    pub(crate) fn reject_hook_composition_mutation(&mut self) -> Result<(), PluginError> {
        if let Some((plugin, hook)) = self.owner.active_plugin_hook {
            let error = PluginError::HookMutationForbidden {
                plugin,
                hook,
                mutation: PluginHookMutation::PluginMembership,
            };
            self.poison(plugin, hook, error.clone());
            return Err(error);
        }
        self.ensure_configuration_mutation_allowed()
    }

    pub(crate) fn composition_prefix(&self) -> plugin::CompositionPrefix {
        plugin::prefix_from_parts(
            self.plugin_entries.clone(),
            self.plugin_definition_witnesses.clone(),
            self.plugin_groups.clone(),
            self.disabled_plugin_slots.clone(),
        )
    }

    pub(crate) fn commit_plugin_batch(
        &mut self,
        batch: plugin::PluginCommitBatch,
    ) -> Result<(), plugin::PluginCommitError> {
        self.reject_hook_composition_mutation()?;
        if self.owner.plugin_lifecycle != PluginLifecycleState::Configuring {
            return Err(PluginError::AppSealed.into());
        }
        if batch.prefix_len != self.plugin_entries.len()
            || batch.entries.len() != batch.witnesses.len()
            || batch.entries.len().saturating_sub(batch.prefix_len) != batch.prepared.len()
        {
            return Err(PluginError::LifecyclePoisoned.into());
        }

        for (committed_in_batch, ((entry, witness), plugin)) in batch.entries[batch.prefix_len..]
            .iter()
            .zip(&batch.witnesses[batch.prefix_len..])
            .zip(batch.prepared)
            .enumerate()
        {
            let plugin_id = entry.plugin_id();
            self.owner.active_plugin_hook = Some((plugin_id, PluginHook::Preflight));
            let context = plugin::preflight_context(&batch.entries, &self.owner.world);
            let preflight = catch_unwind(AssertUnwindSafe(|| plugin.preflight(&context)));
            self.owner.active_plugin_hook = None;
            match preflight {
                Err(_) => {
                    let error = PluginError::HookPanicked {
                        plugin: plugin_id,
                        hook: PluginHook::Preflight,
                    };
                    self.poison(plugin_id, PluginHook::Preflight, error.clone());
                    self.owner.shutdown_plugins_internal();
                    return Err(plugin::PluginCommitError::invoked(
                        plugin_id,
                        PluginHook::Preflight,
                        error,
                    ));
                }
                Ok(Err(error)) if committed_in_batch == 0 => {
                    return Err(plugin::PluginCommitError::invoked(
                        plugin_id,
                        PluginHook::Preflight,
                        error,
                    ));
                }
                Ok(Err(error)) => {
                    let error = PluginError::CommittedPreflightRejected {
                        plugin: plugin_id,
                        source: Box::new(error),
                    };
                    self.poison(plugin_id, PluginHook::Preflight, error.clone());
                    self.owner.shutdown_plugins_internal();
                    return Err(plugin::PluginCommitError::invoked(
                        plugin_id,
                        PluginHook::Preflight,
                        error,
                    ));
                }
                Ok(Ok(())) => {}
            }

            self.plugin_entries.push(entry.clone());
            self.plugin_definition_witnesses.push(witness.clone());
            self.owner.plugins.push(InstalledPlugin {
                plugin: Arc::clone(&plugin),
                plugin_id,
                shutdown_complete: false,
            });
            self.owner.active_plugin_hook = Some((plugin_id, PluginHook::Build));
            let build = catch_unwind(AssertUnwindSafe(|| plugin.build(self)))
                .map_err(|_| PluginError::HookPanicked {
                    plugin: plugin_id,
                    hook: PluginHook::Build,
                })
                .and_then(|result| result);
            self.owner.active_plugin_hook = None;
            if let Err(error) = build {
                self.poison(plugin_id, PluginHook::Build, error.clone());
            }
            if self.owner.plugin_lifecycle == PluginLifecycleState::Poisoned {
                let error = self.primary_plugin_error();
                self.owner.shutdown_plugins_internal();
                return Err(plugin::PluginCommitError::recorded(
                    error,
                    self.plugin_failure_report()
                        .and_then(|report| report.primary())
                        .cloned(),
                ));
            }
        }

        self.plugin_entries = batch.entries;
        self.plugin_definition_witnesses = batch.witnesses;
        self.plugin_groups = batch.groups;
        self.disabled_plugin_slots = batch.disabled_slots;
        self.plugin_plan_fingerprint = batch.fingerprint;
        Ok(())
    }
}

impl runtime::RuntimeOwnerState {
    fn materialize_pending_observers(&mut self, error_handler: ErrorHandler) {
        for observer in std::mem::take(&mut self.pending_observers) {
            self.world.spawn(observer.with_error_handler(error_handler));
        }
        self.world.flush();
    }

    pub(crate) fn bind_managed_runtime_error_handler(&mut self, error_handler: ErrorHandler) {
        self.world
            .insert_resource(FallbackErrorHandler(error_handler));
        self.runtime_error_handler = Some(error_handler);
        self.runtime_fault_bridge = runtime::RuntimeFaultBridgeIdentity::capture(&self.world);
        self.materialize_pending_observers(error_handler);
    }

    fn shutdown_plugins(&mut self) -> Result<(), PluginShutdownError> {
        if self.active_plugin_hook.is_some() {
            return Err(PluginShutdownError::HookActive);
        }
        self.shutdown_plugins_internal();
        if let Some(report) = &self.plugin_failure_report
            && !report.shutdown_failures.is_empty()
        {
            return Err(PluginShutdownError::Failure(Box::new(report.clone())));
        }
        Ok(())
    }

    fn shutdown_plugins_internal(&mut self) {
        if self.plugin_lifecycle == PluginLifecycleState::ShuttingDown {
            return;
        }

        let preserve_poisoned = self.plugin_lifecycle == PluginLifecycleState::Poisoned
            || self
                .plugin_failure_report
                .as_ref()
                .is_some_and(|report| report.primary.is_some());
        self.plugin_lifecycle = PluginLifecycleState::ShuttingDown;

        for index in (0..self.plugins.len()).rev() {
            if self.plugins[index].shutdown_complete {
                continue;
            }

            self.plugins[index].shutdown_complete = true;
            let plugin = Arc::clone(&self.plugins[index].plugin);
            let plugin_id = self.plugins[index].plugin_id;
            let result = {
                let mut context = plugin::shutdown_context(&mut self.world);
                catch_unwind(AssertUnwindSafe(|| plugin.shutdown(&mut context)))
                    .map_err(|_| PluginError::HookPanicked {
                        plugin: plugin_id,
                        hook: PluginHook::Shutdown,
                    })
                    .and_then(|result| result)
            };
            if let Err(error) = result {
                let report =
                    self.plugin_failure_report
                        .get_or_insert_with(|| PluginFailureReport {
                            primary: None,
                            shutdown_failures: Vec::new(),
                            shutdown_complete: false,
                        });
                report.shutdown_failures.push(plugin::failure(
                    plugin_id,
                    PluginHook::Shutdown,
                    error,
                ));
            }
        }

        if let Some(report) = &mut self.plugin_failure_report {
            report.shutdown_complete = true;
        }
        self.plugin_lifecycle = if preserve_poisoned {
            PluginLifecycleState::Poisoned
        } else {
            PluginLifecycleState::ShutdownComplete
        };
    }

    pub(crate) fn complete_startup_once(&mut self) -> Result<(), AppRunError> {
        if !self.started {
            for stage in StartupStage::ALL {
                self.run_managed_schedule(stage)?;
            }
            self.started = true;
        }

        Ok(())
    }

    pub(crate) fn prepare_managed_runtime(&mut self) -> Result<(), AppRunError> {
        {
            let Some(mut settings) = self.world.get_resource_mut::<RuntimeTimeSettings>() else {
                return Err(TimeFrameError::MissingResource {
                    resource: TimeFrameResource::RuntimeTimeSettings,
                }
                .into());
            };
            settings.set_paused(false);
        }
        TimeFramePlan::from_world(&self.world, Duration::ZERO, false)?;
        Ok(())
    }

    fn run_managed_schedule(&mut self, schedule: impl ScheduleLabel) -> Result<(), AppRunError> {
        self.validate_managed_runtime_authority()
            .map_err(|fault| AppRunError::managed_runtime_fault(&fault))?;
        let epoch = self
            .begin_fault_bridge_epoch()
            .map_err(|fault| AppRunError::managed_runtime_fault(&fault))?;
        if let Some(schedule) = self.schedules.get_mut(schedule) {
            schedule.run(&mut self.world);
        }
        self.validate_managed_runtime_authority()
            .map_err(|fault| AppRunError::managed_runtime_fault(&fault))?;
        self.validate_fault_bridge_epoch(epoch)
            .map_err(|fault| AppRunError::managed_runtime_fault(&fault))
    }

    pub(crate) fn validate_managed_runtime_authority(&self) -> Result<(), runtime::RuntimeFault> {
        let generation = self.managed_runtime_generation.ok_or_else(|| {
            runtime::record_runtime_fault_bridge_authority_fault(&self.runtime_fault_reporter)
        })?;
        runtime::validate_managed_runtime_authority(
            &self.world,
            &self.runtime_fault_reporter,
            self.runtime_error_handler,
            self.runtime_fault_bridge,
            generation,
            self.runtime_authority_validator
                .as_ref()
                .map(|owner| &owner.validator),
        )
    }

    pub(crate) fn run_managed_frame(
        &mut self,
        real_delta: Duration,
    ) -> Result<AppFrameOutcome, AppRunError> {
        self.run_frame_transaction(real_delta, false)
    }

    pub(crate) fn run_paused_frame(
        &mut self,
        real_delta: Duration,
    ) -> Result<AppFrameOutcome, AppRunError> {
        self.run_frame_transaction(real_delta, true)
    }

    fn run_frame_transaction(
        &mut self,
        real_delta: Duration,
        paused_stages_only: bool,
    ) -> Result<AppFrameOutcome, AppRunError> {
        debug_assert!(self.started, "managed frames require completed startup");
        self.validate_managed_runtime_authority()
            .map_err(|fault| AppRunError::managed_runtime_fault(&fault))?;

        let paused_stages_only = paused_stages_only || !self.simulation_is_ready();
        let frame_started_at = Instant::now();
        let time_frame_plan =
            TimeFramePlan::from_world(&self.world, real_delta, paused_stages_only)?;
        self.world.insert_resource(FrameExecutionStart::new(
            time_frame_plan.real_time.frame,
            frame_started_at,
        ));
        time_frame_plan.commit(&mut self.world);
        let mut fixed_time = time_frame_plan.fixed_time;
        let mut frame_status = None;

        for stage in CoreStage::ALL {
            if stage == CoreStage::FixedUpdate {
                for _ in 0..time_frame_plan.fixed_steps_to_run {
                    fixed_time.advance_tick();
                    *self.world.resource_mut::<FixedTime>() = fixed_time;
                    self.run_managed_schedule(stage)?;
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
            } else if !paused_stages_only || stage_runs_while_paused(stage) {
                self.run_managed_schedule(stage)?;
            }
        }

        let status = frame_status.expect("the fixed-update stage always exists");
        *self.world.resource_mut::<RuntimeFrameStatus>() = status;
        self.run_managed_schedule(__private::FrameCleanup)?;
        let exit = self.world.resource_mut::<AppExitRequests>().take();
        self.world.clear_trackers();
        Ok(AppFrameOutcome { exit, status })
    }

    pub(crate) fn run_exact_fixed_tick(
        &mut self,
        real_delta: Duration,
    ) -> Result<AppFrameOutcome, AppRunError> {
        debug_assert!(self.started, "exact stepping requires completed startup");
        self.validate_managed_runtime_authority()
            .map_err(|fault| AppRunError::managed_runtime_fault(&fault))?;
        required_frame_resource::<RuntimeTimeSettings>(
            &self.world,
            TimeFrameResource::RuntimeTimeSettings,
        )?;
        let real_time =
            required_frame_resource::<RealTime>(&self.world, TimeFrameResource::RealTime)?
                .plan_advance(real_delta)?;
        let current_fixed_time =
            *required_frame_resource::<FixedTime>(&self.world, TimeFrameResource::FixedTime)?;
        let virtual_delta = current_fixed_time.timestep();
        let virtual_time =
            required_frame_resource::<VirtualTime>(&self.world, TimeFrameResource::VirtualTime)?
                .plan_advance(virtual_delta)?;
        required_frame_resource::<RenderTime>(&self.world, TimeFrameResource::RenderTime)?;
        required_frame_resource::<RuntimeFrameStatus>(
            &self.world,
            TimeFrameResource::RuntimeFrameStatus,
        )?;
        required_frame_resource::<AppExitRequests>(
            &self.world,
            TimeFrameResource::AppExitRequests,
        )?;
        let mut fixed_time = current_fixed_time.begin_exact_step()?;

        *self.world.resource_mut::<RealTime>() = real_time;
        *self.world.resource_mut::<VirtualTime>() = virtual_time;
        *self.world.resource_mut::<FixedTime>() = fixed_time;

        self.run_managed_schedule(CoreStage::FixedUpdate)?;

        fixed_time.finish_exact_step();
        *self.world.resource_mut::<FixedTime>() = fixed_time;
        let status = RuntimeFrameStatus {
            frame: real_time.frame,
            real_delta,
            virtual_delta,
            real_delta_clamped: false,
            fixed_steps: 1,
            fixed_steps_capped: false,
            fixed_tick: fixed_time.tick(),
            fixed_elapsed: fixed_time.elapsed(),
            fixed_remainder: fixed_time.remainder(),
            fixed_debt: fixed_time.debt(),
            fixed_discarded: Duration::ZERO,
        };
        *self.world.resource_mut::<RuntimeFrameStatus>() = status;
        self.run_managed_schedule(__private::FrameCleanup)?;
        let exit = self.world.resource_mut::<AppExitRequests>().take();
        self.world.clear_trackers();
        Ok(AppFrameOutcome { exit, status })
    }
}

fn stage_runs_while_paused(stage: CoreStage) -> bool {
    matches!(
        stage,
        CoreStage::First
            | CoreStage::TaskUpdate
            | CoreStage::Extract
            | CoreStage::Prepare
            | CoreStage::Queue
            | CoreStage::Sort
            | CoreStage::Render
            | CoreStage::Cleanup
            | CoreStage::Last
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_ecs::{
        Commands, Component, DetectChanges, DetectChangesMut, Query, Ref, Res, ResMut, Resource,
        bevy_ecs::error::{BevyError, ErrorContext},
        bevy_ecs::lifecycle::RemovedComponents,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    fn run_frame(app: &mut App, delta: Duration) -> Result<AppFrameOutcome, AppRunError> {
        app.seal_internal()
            .map_err(|error| AppRunError::plugin(error, app.plugin_failure_report().cloned()))?;
        app.owner.admit_configuration_for_tests();
        app.owner.complete_startup_once()?;
        app.owner.run_managed_frame(delta)
    }

    #[derive(Debug, Default, Resource)]
    struct Frames(u32);

    #[derive(Debug, Default, Resource)]
    struct StartupRuns(u32);

    #[derive(Debug, Default, Resource)]
    struct FixedObservations(Vec<(u64, Duration, Duration)>);

    #[derive(Debug, Default, Resource)]
    struct FrameStatusObservations(Vec<(u64, u32, u64, Duration, f32)>);

    #[derive(Debug, Default, Resource)]
    struct FrameExecutionStartObservations(Vec<(u64, u64, bool)>);

    #[derive(Debug, Default, Resource)]
    struct RemovalCount(u32);

    #[derive(Debug, Default, Resource)]
    struct ExactStepTrackerObservations {
        changed: usize,
        removed: usize,
        fixed_runs: usize,
    }

    #[derive(Debug, Default, Resource)]
    struct ExactStepAuthorityRevision(u64);

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

    #[derive(Debug, Default, PartialEq, Eq, Resource)]
    struct ManagedFaultBridgeProbe {
        update_runs: usize,
        last_runs: usize,
    }

    #[derive(Debug, Error)]
    #[error("managed handler probe failure")]
    struct ManagedHandlerProbeError;

    static CONFIGURED_HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn count_managed_update(mut probe: ResMut<ManagedFaultBridgeProbe>) {
        probe.update_runs += 1;
    }

    fn count_managed_last(mut probe: ResMut<ManagedFaultBridgeProbe>) {
        probe.last_runs += 1;
    }

    fn ignore_managed_bevy_error(_error: BevyError, _context: ErrorContext) {}

    fn count_managed_bevy_error(_error: BevyError, _context: ErrorContext) {
        CONFIGURED_HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
    }

    fn fail_managed_system() -> Result<(), BevyError> {
        Err(BevyError::error(ManagedHandlerProbeError))
    }

    fn replace_managed_reporter_and_report(world: &mut World) {
        let replacement = RuntimeFaultReporter::new();
        assert!(replacement.report(RuntimeFault::engine(
            RuntimeFaultKind::System,
            "nara.test.replaced-direct-reporter",
        )));
        world.insert_resource(replacement);
        world.resource_mut::<ManagedFaultBridgeProbe>().update_runs += 1;
    }

    fn replace_and_restore_managed_reporter(
        mut reporter: ResMut<RuntimeFaultReporter>,
        mut probe: ResMut<ManagedFaultBridgeProbe>,
    ) {
        let canonical = reporter.clone();
        *reporter = RuntimeFaultReporter::new();
        assert!(reporter.report(RuntimeFault::engine(
            RuntimeFaultKind::System,
            "nara.test.temporary-direct-reporter",
        )));
        *reporter = canonical;
        probe.update_runs += 1;
    }

    fn replace_and_restore_managed_error_handler(
        mut handler: ResMut<FallbackErrorHandler>,
        mut probe: ResMut<ManagedFaultBridgeProbe>,
    ) {
        let canonical = *handler;
        *handler = FallbackErrorHandler(ignore_managed_bevy_error);
        *handler = canonical;
        probe.update_runs += 1;
    }

    fn managed_fault_bridge_app() -> App {
        let mut app = App::new();
        app.init_resource::<ManagedFaultBridgeProbe>()
            .unwrap()
            .add_systems(CoreStage::Update, count_managed_update)
            .unwrap()
            .add_systems(CoreStage::Last, count_managed_last)
            .unwrap();
        app
    }

    fn assert_managed_fault_bridge_error(error: RuntimeDriveError) {
        assert_eq!(
            error.fault().kind(),
            RuntimeFaultKind::FaultReporterAuthority
        );
        assert_eq!(error.fault().source(), "nara.app.runtime-fault-reporter");
    }

    const RAW_APP_CLOSE_PLUGIN_ID: PluginId = PluginId::new("nara.test.raw-app-close");
    const RAW_APP_CLOSE_OBLIGATION: PluginShutdownObligationId =
        PluginShutdownObligationId::new("nara.test.raw-app-close");
    const RAW_APP_CLOSE_PARTICIPANT_ID: RuntimeCloseParticipantId =
        RuntimeCloseParticipantId::new("nara.test.raw-app-close");
    const RAW_APP_CLOSE_PLUGIN_DECLARATION: PluginDeclaration =
        PluginDeclaration::new(RAW_APP_CLOSE_PLUGIN_ID, PluginCategory::Runtime)
            .shutdown_obligations(&[RAW_APP_CLOSE_OBLIGATION]);

    const FIRST_COLLIDING_CLOSE_PLUGIN_ID: PluginId =
        PluginId::new("nara.test.first-colliding-close");
    const SECOND_COLLIDING_CLOSE_PLUGIN_ID: PluginId =
        PluginId::new("nara.test.second-colliding-close");
    const FIRST_COLLIDING_CLOSE_OBLIGATION: PluginShutdownObligationId =
        PluginShutdownObligationId::new("nara.test.first-colliding-close");
    const SECOND_COLLIDING_CLOSE_OBLIGATION: PluginShutdownObligationId =
        PluginShutdownObligationId::new("nara.test.second-colliding-close");
    const COLLIDING_CLOSE_PARTICIPANT_ID: RuntimeCloseParticipantId =
        RuntimeCloseParticipantId::new("nara.test.colliding-close-participant");
    const FIRST_COLLIDING_CLOSE_PLUGIN_DECLARATION: PluginDeclaration =
        PluginDeclaration::new(FIRST_COLLIDING_CLOSE_PLUGIN_ID, PluginCategory::Runtime)
            .shutdown_obligations(&[FIRST_COLLIDING_CLOSE_OBLIGATION]);
    const SECOND_COLLIDING_CLOSE_PLUGIN_DECLARATION: PluginDeclaration =
        PluginDeclaration::new(SECOND_COLLIDING_CLOSE_PLUGIN_ID, PluginCategory::Runtime)
            .shutdown_obligations(&[SECOND_COLLIDING_CLOSE_OBLIGATION]);

    #[derive(Debug)]
    struct RawAppClosePlugin {
        begins: Arc<AtomicUsize>,
        polls: Arc<AtomicUsize>,
    }

    impl Plugin for RawAppClosePlugin {
        fn declaration() -> &'static PluginDeclaration {
            &RAW_APP_CLOSE_PLUGIN_DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.register_plugin_runtime_close_participant(
                RAW_APP_CLOSE_OBLIGATION,
                RAW_APP_CLOSE_PARTICIPANT_ID,
                RawAppCloseParticipant {
                    begins: Arc::clone(&self.begins),
                    polls: Arc::clone(&self.polls),
                },
            )?;
            Ok(())
        }
    }

    struct RawAppCloseParticipant {
        begins: Arc<AtomicUsize>,
        polls: Arc<AtomicUsize>,
    }

    impl RuntimeCloseParticipant for RawAppCloseParticipant {
        fn begin_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.begins.fetch_add(1, Ordering::SeqCst);
            Ok(RuntimeCloseProgress::Pending)
        }

        fn poll_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            let polls = self.polls.fetch_add(1, Ordering::SeqCst);
            Ok(if polls == 0 {
                RuntimeCloseProgress::Pending
            } else {
                RuntimeCloseProgress::Complete
            })
        }
    }

    struct FirstCollidingClosePlugin {
        begins: Arc<AtomicUsize>,
        polls: Arc<AtomicUsize>,
    }

    #[derive(Clone, Default)]
    struct ControlledClose {
        ready: Arc<AtomicBool>,
        begins: Arc<AtomicUsize>,
        polls: Arc<AtomicUsize>,
    }

    impl RuntimeCloseParticipant for ControlledClose {
        fn begin_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.begins.fetch_add(1, Ordering::SeqCst);
            Ok(if self.ready.load(Ordering::SeqCst) {
                RuntimeCloseProgress::Complete
            } else {
                RuntimeCloseProgress::Pending
            })
        }

        fn poll_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Ok(if self.ready.load(Ordering::SeqCst) {
                RuntimeCloseProgress::Complete
            } else {
                RuntimeCloseProgress::Pending
            })
        }
    }

    struct ControlledClosePlugin {
        close: ControlledClose,
        reject_build: bool,
    }

    impl Plugin for ControlledClosePlugin {
        fn declaration() -> &'static PluginDeclaration {
            &RAW_APP_CLOSE_PLUGIN_DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.register_plugin_runtime_close_participant(
                RAW_APP_CLOSE_OBLIGATION,
                RAW_APP_CLOSE_PARTICIPANT_ID,
                self.close.clone(),
            )?;
            if self.reject_build {
                return Err(PluginError::SetupFailed {
                    plugin: RAW_APP_CLOSE_PLUGIN_ID,
                    message: "injected build failure after close registration".to_owned(),
                });
            }
            Ok(())
        }
    }

    impl Plugin for FirstCollidingClosePlugin {
        fn declaration() -> &'static PluginDeclaration {
            &FIRST_COLLIDING_CLOSE_PLUGIN_DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.register_plugin_runtime_close_participant(
                FIRST_COLLIDING_CLOSE_OBLIGATION,
                COLLIDING_CLOSE_PARTICIPANT_ID,
                RawAppCloseParticipant {
                    begins: Arc::clone(&self.begins),
                    polls: Arc::clone(&self.polls),
                },
            )?;
            Ok(())
        }
    }

    struct SecondCollidingClosePlugin {
        begins: Arc<AtomicUsize>,
        polls: Arc<AtomicUsize>,
    }

    impl Plugin for SecondCollidingClosePlugin {
        fn declaration() -> &'static PluginDeclaration {
            &SECOND_COLLIDING_CLOSE_PLUGIN_DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.register_plugin_runtime_close_participant(
                SECOND_COLLIDING_CLOSE_OBLIGATION,
                COLLIDING_CLOSE_PARTICIPANT_ID,
                RawAppCloseParticipant {
                    begins: Arc::clone(&self.begins),
                    polls: Arc::clone(&self.polls),
                },
            )?;
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

    fn observe_frame_execution_start(
        start: Res<FrameExecutionStart>,
        real_time: Res<RealTime>,
        mut observations: ResMut<FrameExecutionStartObservations>,
    ) {
        observations.0.push((
            start.frame(),
            real_time.frame,
            start.started_at() <= Instant::now(),
        ));
    }

    fn record_removals(
        mut removed: RemovedComponents<Tracked>,
        mut removal_count: ResMut<RemovalCount>,
    ) {
        removal_count.0 += u32::try_from(removed.read().count()).unwrap();
    }

    fn observe_exact_step_trackers(
        tracked: Query<Ref<Tracked>>,
        mut removed: RemovedComponents<Tracked>,
        mut observations: ResMut<ExactStepTrackerObservations>,
    ) {
        observations.changed = tracked.iter().filter(DetectChanges::is_changed).count();
        observations.removed = removed.read().count();
        observations.fixed_runs += 1;
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
            run_frame(app, real_delta).unwrap_err(),
            AppRunError::Time { error: expected }
        );

        assert_eq!(time_state(app), before);
        assert!(app.owner.started);
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

    fn request_exit(mut requests: ResMut<AppExitRequests>) {
        requests.request_exit();
    }

    #[test]
    fn update_runs_startup_once_and_update_every_frame() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.add_systems(StartupStage::Core, spawn_entity).unwrap();
        app.add_systems(CoreStage::Update, (spawn_entity, count_frame))
            .unwrap();

        run_frame(&mut app, Duration::ZERO).unwrap();
        run_frame(&mut app, Duration::ZERO).unwrap();

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

        let outcome = run_frame(&mut app, FixedTime::DEFAULT_TIMESTEP).unwrap();

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
    fn frame_execution_start_is_published_before_the_first_stage() {
        let mut app = App::new();
        app.insert_resource(FrameExecutionStartObservations::default())
            .unwrap();
        app.add_systems(CoreStage::First, observe_frame_execution_start)
            .unwrap();
        assert!(!app.world().contains_resource::<FrameExecutionStart>());

        run_frame(&mut app, Duration::ZERO).unwrap();

        assert_eq!(
            app.world().resource::<FrameExecutionStartObservations>().0,
            vec![(1, 1, true)]
        );
    }

    #[test]
    fn fixed_clock_advances_before_each_fixed_schedule_iteration() {
        let mut app = App::new();
        app.insert_resource(FixedObservations::default()).unwrap();
        app.add_systems(CoreStage::FixedUpdate, observe_fixed_time)
            .unwrap();

        run_frame(&mut app, FixedTime::DEFAULT_TIMESTEP * 3).unwrap();

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

        run_frame(&mut app, FixedTime::DEFAULT_TIMESTEP).unwrap();

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
        run_frame(&mut app, step * 3 + step / 2).unwrap();

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

        run_frame(&mut app, Duration::ZERO).unwrap();

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
    fn exact_step_rotates_trackers_and_the_next_paused_drive_keeps_rotating_them() {
        let mut app = App::new();
        app.insert_resource(ExactStepTrackerObservations::default())
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, observe_exact_step_trackers)
            .unwrap();
        let candidate = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                app.seal().unwrap(),
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap();
        let mut runtime = candidate.complete_startup().unwrap().promote();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::Paused);

        let (changed_entity, removed_entity) = {
            let world = runtime.world_mut_for_tests();
            let changed_entity = world.spawn(Tracked).id();
            let removed_entity = world.spawn(Tracked).id();
            world.entity_mut(removed_entity).remove::<Tracked>();
            (changed_entity, removed_entity)
        };
        let step = match runtime.request_control(RuntimeControl::StepFixedTick) {
            RuntimeControlRequestResult::Accepted(ticket) => ticket,
            RuntimeControlRequestResult::Rejected(rejection) => {
                panic!("paused runtime rejected exact step: {rejection:?}")
            }
        };

        let outcome = runtime.drive(Duration::ZERO).unwrap();

        assert_eq!(outcome.state(), RuntimeState::Paused);
        assert_eq!(outcome.frame().unwrap().status.fixed_steps, 1);
        assert_eq!(
            runtime.control_status(step),
            Some(RuntimeControlStatus::Applied)
        );
        let observations = runtime.world().resource::<ExactStepTrackerObservations>();
        assert_eq!(observations.changed, 1);
        assert_eq!(observations.removed, 1);
        assert_eq!(observations.fixed_runs, 1);
        assert!(
            !runtime
                .world()
                .entity(changed_entity)
                .get_ref::<Tracked>()
                .unwrap()
                .is_changed()
        );
        assert!(runtime.world().removed::<Tracked>().next().is_none());
        assert!(runtime.world().get_entity(removed_entity).is_ok());

        let paused_removed_entity = {
            let world = runtime.world_mut_for_tests();
            world
                .entity_mut(changed_entity)
                .get_mut::<Tracked>()
                .unwrap()
                .set_changed();
            let removed_entity = world.spawn(Tracked).id();
            world.entity_mut(removed_entity).remove::<Tracked>();
            removed_entity
        };
        assert!(
            runtime
                .world()
                .entity(changed_entity)
                .get_ref::<Tracked>()
                .unwrap()
                .is_changed()
        );
        assert_eq!(runtime.world().removed::<Tracked>().count(), 1);

        let paused_outcome = runtime.drive(Duration::ZERO).unwrap();

        assert_eq!(paused_outcome.state(), RuntimeState::Paused);
        assert_eq!(paused_outcome.frame().unwrap().status.fixed_steps, 0);
        assert_eq!(
            runtime
                .world()
                .resource::<ExactStepTrackerObservations>()
                .fixed_runs,
            1
        );
        assert!(
            !runtime
                .world()
                .entity(changed_entity)
                .get_ref::<Tracked>()
                .unwrap()
                .is_changed()
        );
        assert!(runtime.world().removed::<Tracked>().next().is_none());
        assert!(runtime.world().get_entity(paused_removed_entity).is_ok());
    }

    #[test]
    fn exact_step_rejects_authority_drift_before_committing_time_state() {
        const AUTHORITY_PLUGIN_ID: PluginId = PluginId::new("nara.test.exact-step-authority");

        let mut app = App::new();
        app.insert_resource(ExactStepAuthorityRevision::default())
            .unwrap();
        app.__install_runtime_authority_validator(
            AUTHORITY_PLUGIN_ID,
            Arc::new(|world, _generation| {
                if world
                    .get_resource::<ExactStepAuthorityRevision>()
                    .is_some_and(|revision| revision.0 == 0)
                {
                    Ok(())
                } else {
                    Err(RuntimeFault::engine(
                        RuntimeFaultKind::RuntimeAuthority,
                        "nara.test.exact-step-authority",
                    ))
                }
            }),
        )
        .unwrap();
        let candidate = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                app.seal().unwrap(),
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap();
        let mut runtime = candidate.complete_startup().unwrap().promote();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::Paused);

        let real_time = *runtime.world().resource::<RealTime>();
        let virtual_time = *runtime.world().resource::<VirtualTime>();
        let fixed_time = *runtime.world().resource::<FixedTime>();
        let frame_status = *runtime.world().resource::<RuntimeFrameStatus>();
        runtime
            .world_mut_for_tests()
            .resource_mut::<ExactStepAuthorityRevision>()
            .0 = 1;
        let ticket = match runtime.request_control(RuntimeControl::StepFixedTick) {
            RuntimeControlRequestResult::Accepted(ticket) => ticket,
            RuntimeControlRequestResult::Rejected(rejection) => {
                panic!("paused runtime rejected exact step: {rejection:?}")
            }
        };

        let failure = runtime.drive(Duration::from_millis(7)).unwrap_err();

        assert_eq!(failure.fault().kind(), RuntimeFaultKind::RuntimeAuthority);
        assert_eq!(runtime.state(), RuntimeState::Faulted);
        assert_eq!(
            runtime.control_status(ticket),
            Some(RuntimeControlStatus::Failed(
                RuntimeControlFailure::RuntimeFaulted
            ))
        );
        assert_eq!(*runtime.world().resource::<RealTime>(), real_time);
        assert_eq!(*runtime.world().resource::<VirtualTime>(), virtual_time);
        assert_eq!(*runtime.world().resource::<FixedTime>(), fixed_time);
        assert_eq!(
            *runtime.world().resource::<RuntimeFrameStatus>(),
            frame_status
        );
    }

    #[test]
    fn paused_frame_advances_real_time_but_not_virtual_or_fixed_time() {
        let mut app = App::new();
        app.insert_resource(Frames::default()).unwrap();
        app.insert_resource(RuntimeTimeSettings::default().with_paused(true))
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();

        let outcome = run_frame(&mut app, FixedTime::DEFAULT_TIMESTEP).unwrap();

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

        let outcome = run_frame(&mut app, FixedTime::DEFAULT_TIMESTEP * 2).unwrap();

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

        let outcome = run_frame(&mut app, Duration::from_secs(1)).unwrap();

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

        run_frame(&mut app, FixedTime::DEFAULT_TIMESTEP / 2).unwrap();

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
        run_frame(&mut app, step * 5 + step / 2).unwrap();

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
        run_frame(&mut app, step * 5 + step / 2).unwrap();

        let fixed = app.world().resource::<FixedTime>();
        assert_eq!(fixed.tick(), 2);
        assert_eq!(fixed.debt(), step * 3);
        assert_eq!(fixed.remainder(), step / 2);
        assert_eq!(fixed.discarded_this_frame(), Duration::ZERO);
        let alpha = app.world().resource::<RenderTime>().interpolation_alpha;
        assert!((alpha - 0.5).abs() < f32::EPSILON);

        run_frame(&mut app, Duration::ZERO).unwrap();

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
        let error = run_frame(&mut app, step * 5).unwrap_err();

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

        run_frame(&mut app, step * 4).unwrap();
        assert_eq!(app.world().resource::<FixedTime>().tick(), 2);
        assert_eq!(app.world().resource::<FixedTime>().debt(), step * 2);
    }

    #[test]
    fn startup_pause_applies_to_the_first_frame() {
        let mut app = App::new();
        app.insert_resource(StartupRuns::default()).unwrap();
        app.insert_resource(Frames::default()).unwrap();
        app.add_systems(StartupStage::Core, pause_on_startup)
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();
        let step = FixedTime::DEFAULT_TIMESTEP;

        run_frame(&mut app, step).unwrap();

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
        app.add_systems(StartupStage::Core, set_timestep_on_startup)
            .unwrap();
        app.add_systems(CoreStage::FixedUpdate, count_frame)
            .unwrap();
        let step = FixedTime::DEFAULT_TIMESTEP;

        run_frame(&mut app, step * 2).unwrap();

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
        app.add_systems(StartupStage::Core, configure_preserve_debt_on_startup)
            .unwrap();
        app.add_systems(CoreStage::Update, count_frame).unwrap();
        let step = FixedTime::DEFAULT_TIMESTEP;

        assert_eq!(
            run_frame(&mut app, step * 5).unwrap_err(),
            AppRunError::Time {
                error: TimeFrameError::Fixed(FixedTimeError::CatchUpDebtExceeded {
                    pending_steps: 5,
                    max_steps_per_frame: 2,
                    max_debt_steps: 2,
                }),
            }
        );

        assert!(app.owner.started);
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
        run_frame(&mut app, step * 5).unwrap();

        assert_eq!(app.world().resource::<StartupRuns>().0, 1);
        assert_eq!(app.world().resource::<Frames>().0, 1);
    }

    #[test]
    fn missing_startup_time_resource_is_structured_and_retryable() {
        let mut app = App::new();
        app.insert_resource(StartupRuns::default()).unwrap();
        app.insert_resource(Frames::default()).unwrap();
        app.add_systems(StartupStage::Core, remove_fixed_time_on_startup)
            .unwrap();
        app.add_systems(CoreStage::Update, count_frame).unwrap();

        assert_eq!(
            run_frame(&mut app, FixedTime::DEFAULT_TIMESTEP).unwrap_err(),
            AppRunError::Time {
                error: TimeFrameError::MissingResource {
                    resource: TimeFrameResource::FixedTime,
                },
            }
        );
        assert!(app.owner.started);
        assert_eq!(app.world().resource::<StartupRuns>().0, 1);
        assert_eq!(app.world().resource::<Frames>().0, 0);
        assert_eq!(*app.world().resource::<RealTime>(), RealTime::default());
        assert_eq!(
            *app.world().resource::<VirtualTime>(),
            VirtualTime::default()
        );

        app.insert_resource(FixedTime::default()).unwrap();
        run_frame(&mut app, FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert_eq!(app.world().resource::<StartupRuns>().0, 1);
        assert_eq!(app.world().resource::<Frames>().0, 1);
    }

    #[test]
    fn time_frame_plan_rejects_every_clock_overflow_without_state_changes() {
        let mut scaled_delta_app = App::new();
        run_frame(&mut scaled_delta_app, Duration::ZERO).unwrap();
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
        run_frame(&mut real_elapsed_app, Duration::ZERO).unwrap();
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
        run_frame(&mut real_frame_app, Duration::ZERO).unwrap();
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
        run_frame(&mut virtual_elapsed_app, Duration::ZERO).unwrap();
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
        run_frame(&mut virtual_frame_app, Duration::ZERO).unwrap();
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
        run_frame(&mut fixed_pending_app, Duration::ZERO).unwrap();
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
            run_frame(&mut tick_app, step * 2).unwrap_err(),
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
            run_frame(&mut elapsed_app, step * 2).unwrap_err(),
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
        run_frame(&mut app, step * 5).unwrap();
        assert_eq!(app.world().resource::<FixedTime>().debt(), step * 3);

        app.insert_resource(RuntimeTimeSettings::default().with_paused(true))
            .unwrap();
        run_frame(&mut app, Duration::ZERO).unwrap();
        assert_eq!(app.world().resource::<FixedTime>().tick(), 2);
        assert_eq!(app.world().resource::<FixedTime>().debt(), step * 3);

        app.insert_resource(RuntimeTimeSettings::default().with_time_scale(0.0).unwrap())
            .unwrap();
        run_frame(&mut app, Duration::ZERO).unwrap();
        assert_eq!(app.world().resource::<FixedTime>().tick(), 2);
        assert_eq!(app.world().resource::<FixedTime>().debt(), step * 3);

        app.insert_resource(RuntimeTimeSettings::default()).unwrap();
        run_frame(&mut app, Duration::ZERO).unwrap();
        assert_eq!(app.world().resource::<FixedTime>().tick(), 4);
        assert_eq!(app.world().resource::<FixedTime>().debt(), step);
    }

    #[test]
    fn app_exit_request_is_reported_in_frame_outcome() {
        let mut app = App::new();
        app.add_systems(CoreStage::Update, request_exit).unwrap();

        let outcome = run_frame(&mut app, Duration::ZERO).unwrap();

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

        run_frame(&mut app, FixedTime::DEFAULT_TIMESTEP).unwrap();

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

        run_frame(&mut app, Duration::ZERO).unwrap();

        assert_eq!(
            app.world().resource::<Order>().0,
            [
                "extract", "prepare", "queue", "sort", "render", "cleanup", "last"
            ]
        );
    }

    #[test]
    fn seal_reasserts_the_automatic_deferred_build_pass() {
        #[derive(Component)]
        struct DeferredBeforeSimulate;

        #[derive(Default, Resource)]
        struct BoundaryObserved(bool);

        fn emit_boundary(mut commands: Commands) {
            commands.spawn(DeferredBeforeSimulate);
        }

        fn observe_boundary(
            deferred: Query<&DeferredBeforeSimulate>,
            mut observed: ResMut<BoundaryObserved>,
        ) {
            observed.0 = !deferred.is_empty();
        }

        let mut app = App::new();
        app.init_resource::<BoundaryObserved>().unwrap();
        app.add_systems(
            CoreStage::FixedUpdate,
            emit_boundary.before(FixedUpdateSet::Simulate),
        )
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            observe_boundary.in_set(FixedUpdateSet::Simulate),
        )
        .unwrap();
        app.owner
            .schedules
            .get_mut(CoreStage::FixedUpdate)
            .expect("FixedUpdate is an engine-owned schedule")
            .remove_build_pass::<nara_ecs::bevy_ecs::schedule::passes::AutoInsertApplyDeferredPass>(
            );

        run_frame(&mut app, FixedTime::DEFAULT_TIMESTEP).unwrap();

        assert!(app.world().resource::<BoundaryObserved>().0);
    }

    #[test]
    fn managed_app_rejects_removed_replaced_or_reinserted_fault_reporter() {
        for mutation in 0..3 {
            let app = managed_fault_bridge_app();
            let mut runtime = RuntimeInstance::start(app).unwrap();
            runtime.drive(Duration::ZERO).unwrap();
            let canonical = runtime.world().resource::<RuntimeFaultReporter>().clone();

            let world = runtime.world_mut_for_tests();
            match mutation {
                0 => {
                    world.remove_resource::<RuntimeFaultReporter>();
                }
                1 => {
                    world.insert_resource(RuntimeFaultReporter::new());
                }
                2 => {
                    world.remove_resource::<RuntimeFaultReporter>();
                    world.insert_resource(canonical.clone());
                }
                _ => unreachable!(),
            }

            assert_managed_fault_bridge_error(runtime.drive(Duration::ZERO).unwrap_err());
            assert_eq!(
                *runtime.world().resource::<ManagedFaultBridgeProbe>(),
                ManagedFaultBridgeProbe {
                    update_runs: 1,
                    last_runs: 1,
                }
            );
            assert_eq!(
                canonical.fault().unwrap().kind(),
                RuntimeFaultKind::FaultReporterAuthority
            );
            assert_managed_fault_bridge_error(runtime.drive(Duration::ZERO).unwrap_err());
        }
    }

    #[test]
    fn managed_app_rejects_removed_replaced_or_reinserted_error_handler() {
        for mutation in 0..3 {
            let app = managed_fault_bridge_app();
            let mut runtime = RuntimeInstance::start(app).unwrap();
            runtime.drive(Duration::ZERO).unwrap();
            let canonical = *runtime.world().resource::<FallbackErrorHandler>();
            let reporter = runtime.world().resource::<RuntimeFaultReporter>().clone();

            let world = runtime.world_mut_for_tests();
            match mutation {
                0 => {
                    world.remove_resource::<FallbackErrorHandler>();
                }
                1 => {
                    world.insert_resource(FallbackErrorHandler(ignore_managed_bevy_error));
                }
                2 => {
                    world.remove_resource::<FallbackErrorHandler>();
                    world.insert_resource(canonical);
                }
                _ => unreachable!(),
            }

            assert_managed_fault_bridge_error(runtime.drive(Duration::ZERO).unwrap_err());
            assert_eq!(
                *runtime.world().resource::<ManagedFaultBridgeProbe>(),
                ManagedFaultBridgeProbe {
                    update_runs: 1,
                    last_runs: 1,
                }
            );
            assert_eq!(
                reporter.fault().unwrap().kind(),
                RuntimeFaultKind::FaultReporterAuthority
            );
            assert_managed_fault_bridge_error(runtime.drive(Duration::ZERO).unwrap_err());
        }
    }

    #[test]
    fn managed_admission_owns_the_error_handler_before_startup() {
        CONFIGURED_HANDLER_CALLS.store(0, Ordering::SeqCst);
        let mut app = managed_fault_bridge_app();
        app.add_systems(CoreStage::Update, fail_managed_system)
            .unwrap();
        app.insert_resource(FallbackErrorHandler(count_managed_bevy_error))
            .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();

        let error = runtime.drive(Duration::ZERO).unwrap_err();

        assert_eq!(error.fault().kind(), RuntimeFaultKind::System);
        assert_eq!(CONFIGURED_HANDLER_CALLS.load(Ordering::SeqCst), 0);
        assert_eq!(
            runtime
                .world()
                .resource::<ManagedFaultBridgeProbe>()
                .last_runs,
            0
        );
    }

    #[test]
    fn managed_schedule_rejects_in_place_fault_bridge_mutation_after_restore() {
        for mutation in 0..2 {
            let mut app = App::new();
            app.init_resource::<ManagedFaultBridgeProbe>().unwrap();
            match mutation {
                0 => app
                    .add_systems(CoreStage::Update, replace_and_restore_managed_reporter)
                    .unwrap(),
                1 => app
                    .add_systems(CoreStage::Update, replace_and_restore_managed_error_handler)
                    .unwrap(),
                _ => unreachable!(),
            };
            app.add_systems(CoreStage::Last, count_managed_last)
                .unwrap();
            let mut runtime = RuntimeInstance::start(app).unwrap();
            let canonical = runtime.world().resource::<RuntimeFaultReporter>().clone();

            assert_managed_fault_bridge_error(runtime.drive(Duration::ZERO).unwrap_err());
            assert_eq!(
                *runtime.world().resource::<ManagedFaultBridgeProbe>(),
                ManagedFaultBridgeProbe {
                    update_runs: 1,
                    last_runs: 0,
                }
            );
            assert_eq!(
                canonical.fault().unwrap().kind(),
                RuntimeFaultKind::FaultReporterAuthority
            );
            assert_managed_fault_bridge_error(runtime.drive(Duration::ZERO).unwrap_err());
            assert_eq!(
                runtime
                    .world()
                    .resource::<ManagedFaultBridgeProbe>()
                    .update_runs,
                1
            );
        }
    }

    #[test]
    fn managed_schedule_cannot_hide_a_fault_behind_a_replaced_world_reporter() {
        let mut app = App::new();
        app.init_resource::<ManagedFaultBridgeProbe>()
            .unwrap()
            .add_systems(CoreStage::Update, replace_managed_reporter_and_report)
            .unwrap()
            .add_systems(CoreStage::Last, count_managed_last)
            .unwrap();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        let canonical = runtime.world().resource::<RuntimeFaultReporter>().clone();

        assert_managed_fault_bridge_error(runtime.drive(Duration::ZERO).unwrap_err());
        assert_eq!(
            *runtime.world().resource::<ManagedFaultBridgeProbe>(),
            ManagedFaultBridgeProbe {
                update_runs: 1,
                last_runs: 0,
            }
        );
        assert_eq!(
            canonical.fault().unwrap().kind(),
            RuntimeFaultKind::FaultReporterAuthority
        );
        assert_managed_fault_bridge_error(runtime.drive(Duration::ZERO).unwrap_err());
        assert_eq!(
            runtime
                .world()
                .resource::<ManagedFaultBridgeProbe>()
                .update_runs,
            1
        );
    }

    #[test]
    fn dropping_configuration_waits_for_one_registered_close_attempt() {
        let begins = Arc::new(AtomicUsize::new(0));
        let polls = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugin(RawAppClosePlugin {
            begins: Arc::clone(&begins),
            polls: Arc::clone(&polls),
        })
        .unwrap();

        drop(app);

        assert_eq!(begins.load(Ordering::SeqCst), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert_eq!(runtime_quarantine_status().current_thread_retained(), 0);
    }

    #[test]
    fn ordinary_start_moves_the_configured_owner_and_closes_it_once() {
        let mut app = App::new();
        let identity = app.world().id();
        let close = ControlledClose::default();
        close.ready.store(true, Ordering::SeqCst);
        app.add_plugin(ControlledClosePlugin {
            close: close.clone(),
            reject_build: false,
        })
        .unwrap();
        app.init_resource::<Frames>().unwrap();
        app.add_systems(CoreStage::Update, count_frame).unwrap();

        let mut runtime = RuntimeInstance::start(app).unwrap();
        assert_eq!(identity, runtime.world().id());
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.world().resource::<Frames>().0, 1);
        assert!(matches!(
            runtime.request_control(RuntimeControl::Stop),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        runtime.finish().unwrap();
        assert_eq!(close.begins.load(Ordering::SeqCst), 1);
        assert_eq!(close.polls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn finish_polls_one_close_attempt_until_complete() {
        let begins = Arc::new(AtomicUsize::new(0));
        let polls = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugin(RawAppClosePlugin {
            begins: Arc::clone(&begins),
            polls: Arc::clone(&polls),
        })
        .unwrap();

        RuntimeInstance::start(app).unwrap().finish().unwrap();

        assert_eq!(begins.load(Ordering::SeqCst), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        assert_eq!(runtime_quarantine_status().current_thread_retained(), 0);
    }

    #[test]
    fn finish_returns_the_owner_after_one_incomplete_attempt() {
        let close = ControlledClose::default();
        let mut app = App::new();
        app.add_plugin(ControlledClosePlugin {
            close: close.clone(),
            reject_build: false,
        })
        .unwrap();
        let runtime = RuntimeInstance::start_with_options(
            app,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap();

        let mut retirement = runtime.finish().unwrap_err();

        assert_eq!(
            retirement.retirement_state(),
            RuntimeCandidateRetirementState::RetirementIncomplete
        );
        assert_eq!(close.begins.load(Ordering::SeqCst), 1);
        assert_eq!(close.polls.load(Ordering::SeqCst), 1);
        close.ready.store(true, Ordering::SeqCst);
        assert_eq!(
            retirement.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
        assert_eq!(close.begins.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn finish_succeeds_after_retrying_a_historical_close_timeout() {
        let close = ControlledClose::default();
        let mut app = App::new();
        app.add_plugin(ControlledClosePlugin {
            close: close.clone(),
            reject_build: false,
        })
        .unwrap();
        let mut runtime = RuntimeInstance::start_with_options(
            app,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap();
        runtime.request_control(RuntimeControl::Stop);
        runtime.drive(Duration::ZERO).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
        close.ready.store(true, Ordering::SeqCst);

        let evidence = runtime.finish().unwrap();

        assert!(
            evidence
                .causes()
                .contains(&RuntimeCloseCause::DeadlineExceeded)
        );
        assert!(!evidence.plugin_shutdown_failed());
        assert_eq!(close.begins.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn ordinary_start_failure_completes_one_close_attempt_before_returning() {
        let begins = Arc::new(AtomicUsize::new(0));
        let polls = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugin(RawAppClosePlugin {
            begins: Arc::clone(&begins),
            polls: Arc::clone(&polls),
        })
        .unwrap();
        app.add_systems(StartupStage::Runtime, fail_managed_system)
            .unwrap();

        let failure = RuntimeInstance::start(app).unwrap_err();

        assert!(matches!(failure.error(), RuntimeStartError::Startup(_)));
        assert_eq!(
            failure.retirement_state(),
            RuntimeCandidateRetirementState::Retired
        );
        assert_eq!(begins.load(Ordering::SeqCst), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 2);
        drop(failure);
        assert_eq!(runtime_quarantine_status().current_thread_retained(), 0);
    }

    #[test]
    fn ordinary_startup_failure_retains_retryable_close_ownership() {
        let close = ControlledClose::default();
        let mut app = App::new();
        app.add_plugin(ControlledClosePlugin {
            close: close.clone(),
            reject_build: false,
        })
        .unwrap();
        app.add_systems(
            StartupStage::Runtime,
            |reporter: Res<RuntimeFaultReporter>| {
                reporter.report(RuntimeFault::engine(
                    RuntimeFaultKind::System,
                    "nara.test.ordinary-startup-failure",
                ));
            },
        )
        .unwrap();

        let mut failure = RuntimeInstance::start_with_options(
            app,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap_err();
        assert!(matches!(failure.error(), RuntimeStartError::Startup(_)));
        assert_eq!(
            failure.retirement_state(),
            RuntimeCandidateRetirementState::RetirementIncomplete
        );
        assert_eq!(close.begins.load(Ordering::SeqCst), 1);
        close.ready.store(true, Ordering::SeqCst);
        assert_eq!(
            failure.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
        assert_eq!(close.begins.load(Ordering::SeqCst), 1);
        let polls = close.polls.load(Ordering::SeqCst);
        drop(failure);
        assert_eq!(close.polls.load(Ordering::SeqCst), polls);
    }

    #[test]
    fn ordinary_build_failure_retains_the_registered_close_participant() {
        let close = ControlledClose::default();
        let mut app = App::new();
        assert!(
            app.add_plugin(ControlledClosePlugin {
                close: close.clone(),
                reject_build: true,
            })
            .is_err()
        );
        assert_eq!(close.begins.load(Ordering::SeqCst), 0);

        let mut failure = RuntimeInstance::start_with_options(
            app,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap_err();
        assert!(matches!(failure.error(), RuntimeStartError::Preparation(_)));
        assert_eq!(
            failure.retirement_state(),
            RuntimeCandidateRetirementState::RetirementIncomplete
        );
        close.ready.store(true, Ordering::SeqCst);
        assert_eq!(
            failure.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
        assert_eq!(close.begins.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn runtime_close_participant_collision_reports_exact_error_and_retires_both_owners() {
        let first_begins = Arc::new(AtomicUsize::new(0));
        let first_polls = Arc::new(AtomicUsize::new(0));
        let second_begins = Arc::new(AtomicUsize::new(0));
        let second_polls = Arc::new(AtomicUsize::new(0));
        let mut app = App::new();
        app.add_plugin(FirstCollidingClosePlugin {
            begins: Arc::clone(&first_begins),
            polls: Arc::clone(&first_polls),
        })
        .unwrap();

        let error = match app.add_plugin(SecondCollidingClosePlugin {
            begins: Arc::clone(&second_begins),
            polls: Arc::clone(&second_polls),
        }) {
            Ok(_) => panic!("colliding runtime close participant must reject the second plugin"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            AddPluginsError::Plugin(error) if matches!(error.error(), PluginError::DuplicateRuntimeCloseParticipant {
                plugin: SECOND_COLLIDING_CLOSE_PLUGIN_ID,
                obligation: SECOND_COLLIDING_CLOSE_OBLIGATION,
                participant_id: COLLIDING_CLOSE_PARTICIPANT_ID,
            })
        ));
        assert_eq!(first_begins.load(Ordering::SeqCst), 0);
        assert_eq!(second_begins.load(Ordering::SeqCst), 0);

        drop(app);

        assert_eq!(first_begins.load(Ordering::SeqCst), 1);
        assert_eq!(first_polls.load(Ordering::SeqCst), 2);
        assert_eq!(second_begins.load(Ordering::SeqCst), 1);
        assert_eq!(second_polls.load(Ordering::SeqCst), 2);
        assert_eq!(runtime_quarantine_status().current_thread_retained(), 0);
    }
}
