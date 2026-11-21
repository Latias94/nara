//! Portable scheduling phase and its original time-owner application.

use std::time::Duration;

use nara_ecs::World;
use thiserror::Error;

use crate::{
    FixedCatchUpPolicy, FixedTime, RenderTime, RuntimeFault, RuntimeState, RuntimeTimeSettings,
    ScaledTimeRemainder, duration_remainder,
};

/// Fixed scheduling phase and policy, independently of a Runtime's identity or clock history.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeTimeContinuation {
    settings: RuntimeTimeSettings,
    timestep: Duration,
    max_steps_per_frame: u32,
    max_debt_steps: u32,
    catch_up_policy: FixedCatchUpPolicy,
    pending: Duration,
    scaled_remainder: ScaledTimeRemainder,
}

impl RuntimeTimeContinuation {
    /// Constructs a portable phase from validated policy and unconsumed virtual time.
    ///
    /// Only the fixed clock's configuration is read. Its ticks, elapsed time and frame fields
    /// are not copied. `pending` carries completed nanoseconds and `scaled_remainder` carries
    /// the fraction kept for the next elapsed conversion. Pause is imposed by the receiving
    /// managed boundary. PreserveDebt must fit the completed-frame debt allowance.
    pub fn new(
        settings: &RuntimeTimeSettings,
        fixed: &FixedTime,
        pending: Duration,
        scaled_remainder: ScaledTimeRemainder,
    ) -> Result<Self, RuntimeTimeContinuationRejection> {
        let whole_ticks = pending.as_nanos() / fixed.timestep.as_nanos();
        if fixed.catch_up_policy == FixedCatchUpPolicy::PreserveDebt
            && whole_ticks > u128::from(fixed.max_debt_steps.get())
        {
            return Err(RuntimeTimeContinuationRejection::DebtExceeded {
                whole_ticks,
                maximum: fixed.max_debt_steps.get(),
            });
        }
        Ok(Self {
            settings: settings.with_paused(true),
            timestep: fixed.timestep,
            max_steps_per_frame: fixed.max_steps_per_frame.get(),
            max_debt_steps: fixed.max_debt_steps.get(),
            catch_up_policy: fixed.catch_up_policy,
            pending,
            scaled_remainder,
        })
    }

    /// Builds a portable continuation from the supplied time resources.
    ///
    /// PreserveDebt must fit the completed-frame debt allowance. During catch-up, systems can
    /// read the live fraction with [`FixedTime::scaled_remainder`] before the debt settles.
    /// Authorization to capture or restore a managed Runtime belongs to its completed paused
    /// boundary.
    pub fn from_clocks(
        settings: &RuntimeTimeSettings,
        fixed: &FixedTime,
    ) -> Result<Self, RuntimeTimeContinuationRejection> {
        Self::new(settings, fixed, fixed.pending, fixed.scaled_remainder)
    }

    #[must_use]
    pub const fn timestep(self) -> Duration {
        self.timestep
    }

    #[must_use]
    pub const fn max_steps_per_frame(self) -> u32 {
        self.max_steps_per_frame
    }

    #[must_use]
    pub const fn max_debt_steps(self) -> u32 {
        self.max_debt_steps
    }

    #[must_use]
    pub const fn catch_up_policy(self) -> FixedCatchUpPolicy {
        self.catch_up_policy
    }

    #[must_use]
    pub const fn time_scale(self) -> f32 {
        self.settings.time_scale
    }

    #[must_use]
    pub const fn max_delta(self) -> Duration {
        self.settings.max_delta
    }

    #[must_use]
    pub const fn pending(self) -> Duration {
        self.pending
    }

    #[must_use]
    pub const fn scaled_remainder(self) -> ScaledTimeRemainder {
        self.scaled_remainder
    }

    #[must_use]
    pub fn remainder(self) -> Duration {
        duration_remainder(self.pending, self.timestep)
    }

    #[must_use]
    pub fn debt(self) -> Duration {
        self.pending - self.remainder()
    }

    fn matches_policy(self, settings: &RuntimeTimeSettings, fixed: &FixedTime) -> bool {
        self.settings.time_scale.to_bits() == settings.time_scale.to_bits()
            && self.settings.max_delta == settings.max_delta
            && self.timestep == fixed.timestep
            && self.max_steps_per_frame == fixed.max_steps_per_frame.get()
            && self.max_debt_steps == fixed.max_debt_steps.get()
            && self.catch_up_policy == fixed.catch_up_policy
    }

    pub(crate) fn capture(world: &World) -> Result<Self, RuntimeTimeContinuationRejection> {
        let settings = world
            .get_resource::<RuntimeTimeSettings>()
            .ok_or(RuntimeTimeContinuationRejection::MissingTimeResources)?;
        let fixed = world
            .get_resource::<FixedTime>()
            .ok_or(RuntimeTimeContinuationRejection::MissingTimeResources)?;
        if fixed.frame_active {
            return Err(RuntimeTimeContinuationRejection::FrameActive);
        }
        Self::from_clocks(settings, fixed)
    }

    pub(crate) fn apply(self, world: &mut World) -> Result<(), RuntimeTimeContinuationRejection> {
        let settings = world
            .get_resource::<RuntimeTimeSettings>()
            .ok_or(RuntimeTimeContinuationRejection::MissingTimeResources)?;
        let fixed = world
            .get_resource::<FixedTime>()
            .ok_or(RuntimeTimeContinuationRejection::MissingTimeResources)?;
        if fixed.frame_active {
            return Err(RuntimeTimeContinuationRejection::FrameActive);
        }
        if !self.matches_policy(settings, fixed) {
            return Err(RuntimeTimeContinuationRejection::PolicyMismatch);
        }
        if !world.contains_resource::<RenderTime>() {
            return Err(RuntimeTimeContinuationRejection::MissingTimeResources);
        }
        let mut updated = *fixed;
        updated.pending = self.pending;
        updated.scaled_remainder = self.scaled_remainder;
        updated.update_pending_parts();
        let mut render = *world.resource::<RenderTime>();
        render.update_from_fixed(&updated);
        *world.resource_mut::<FixedTime>() = updated;
        *world.resource_mut::<RenderTime>() = render;
        Ok(())
    }
}

impl PartialEq for RuntimeTimeContinuation {
    fn eq(&self, other: &Self) -> bool {
        self.settings.time_scale.to_bits() == other.settings.time_scale.to_bits()
            && self.settings.max_delta == other.settings.max_delta
            && self.timestep == other.timestep
            && self.max_steps_per_frame == other.max_steps_per_frame
            && self.max_debt_steps == other.max_debt_steps
            && self.catch_up_policy == other.catch_up_policy
            && self.pending == other.pending
            && self.scaled_remainder == other.scaled_remainder
    }
}

impl Eq for RuntimeTimeContinuation {}

/// Refusal before any continuation mutation or user schedule executes.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuntimeTimeContinuationRejection {
    #[error("time continuation is unavailable in Runtime state {state:?}")]
    Unavailable { state: RuntimeState },
    #[error("time continuation requires no pending managed control")]
    ControlPending,
    #[error("time continuation requires a completed healthy paused App frame")]
    IncompletePausedFrame,
    #[error("time continuation cannot initialize an active fixed frame")]
    FrameActive,
    #[error("time continuation requires the original time resources")]
    MissingTimeResources,
    #[error("time continuation policy differs from the receiving Runtime")]
    PolicyMismatch,
    #[error("time continuation retains {whole_ticks} whole ticks, exceeding debt cap {maximum}")]
    DebtExceeded { whole_ticks: u128, maximum: u32 },
}

/// Retains whether time initialization happened before a sticky managed fault was observed.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum RuntimeTimeContinuationError {
    #[error(transparent)]
    Rejected(#[from] RuntimeTimeContinuationRejection),
    #[error("managed time continuation faulted (applied={applied})")]
    Faulted {
        applied: bool,
        rejection: Option<RuntimeTimeContinuationRejection>,
        fault: RuntimeFault,
    },
}

impl RuntimeTimeContinuationError {
    #[must_use]
    pub const fn applied(&self) -> bool {
        matches!(self, Self::Faulted { applied: true, .. })
    }

    #[must_use]
    pub fn fault(&self) -> Option<&RuntimeFault> {
        match self {
            Self::Faulted { fault, .. } => Some(fault),
            Self::Rejected(_) => None,
        }
    }
}
