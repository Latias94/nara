use std::{num::NonZeroU32, time::Duration};

use bevy_platform::time::Instant;
use nara_ecs::{Resource, World, bevy_ecs::component::Mutable};
use thiserror::Error;

use crate::ScaledTimeRemainder;

#[path = "time_continuation.rs"]
mod continuation;

pub use continuation::{
    RuntimeTimeContinuation, RuntimeTimeContinuationError, RuntimeTimeContinuationRejection,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Resource)]
pub struct RealTime {
    pub delta: Duration,
    pub elapsed: Duration,
    pub frame: u64,
}

impl RealTime {
    fn advance(self, delta: Duration) -> Result<Self, TimeFrameError> {
        Ok(Self {
            delta,
            elapsed: self.elapsed.checked_add(delta).ok_or(
                TimeFrameError::RealElapsedOverflow {
                    elapsed: self.elapsed,
                    delta,
                },
            )?,
            frame: self
                .frame
                .checked_add(1)
                .ok_or(TimeFrameError::RealFrameOverflow { frame: self.frame })?,
        })
    }

    pub fn delta_seconds(&self) -> f32 {
        self.delta.as_secs_f32()
    }

    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed.as_secs_f64()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Resource)]
pub struct VirtualTime {
    pub delta: Duration,
    pub elapsed: Duration,
    pub frame: u64,
}

impl VirtualTime {
    fn advance(self, delta: Duration) -> Result<Self, TimeFrameError> {
        Ok(Self {
            delta,
            elapsed: self.elapsed.checked_add(delta).ok_or(
                TimeFrameError::VirtualElapsedOverflow {
                    elapsed: self.elapsed,
                    delta,
                },
            )?,
            frame: self
                .frame
                .checked_add(1)
                .ok_or(TimeFrameError::VirtualFrameOverflow { frame: self.frame })?,
        })
    }

    pub fn delta_seconds(&self) -> f32 {
        self.delta.as_secs_f32()
    }

    pub fn elapsed_seconds(&self) -> f64 {
        self.elapsed.as_secs_f64()
    }
}

/// Start of an elapsed frame for presentation timing.
#[derive(Clone, Copy, Debug, Resource)]
pub struct FrameExecutionStart {
    frame: u64,
    started_at: Instant,
}

impl FrameExecutionStart {
    pub const fn frame(self) -> u64 {
        self.frame
    }

    pub const fn started_at(self) -> Instant {
        self.started_at
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
    #[error("an explicit fixed step must advance by a non-zero duration")]
    ZeroStep,
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

#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum TimeFrameError {
    #[error("required frame resource is missing: {resource}")]
    MissingResource { resource: &'static str },
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

    pub const fn paused(&self) -> bool {
        self.paused
    }

    pub const fn time_scale(&self) -> f32 {
        self.time_scale
    }

    pub const fn max_delta(&self) -> Duration {
        self.max_delta
    }

    pub const fn with_paused(mut self, paused: bool) -> Self {
        self.set_paused(paused);
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

    pub const fn set_paused(&mut self, paused: bool) {
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
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Resource)]
pub struct RenderTime {
    fixed: FixedTime,
    interpolation_alpha: f32,
}

impl RenderTime {
    pub const fn fixed(&self) -> FixedTime {
        self.fixed
    }

    pub fn remainder(&self) -> Duration {
        self.fixed.remainder()
    }

    pub const fn interpolation_alpha(&self) -> f32 {
        self.interpolation_alpha
    }

    fn update_from_fixed(&mut self, fixed: &FixedTime) {
        self.fixed = *fixed;
        let alpha = fixed.remainder().as_secs_f64() / fixed.timestep.as_secs_f64();
        self.interpolation_alpha = (alpha as f32).min(f32::from_bits(1.0f32.to_bits() - 1));
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FixedCatchUpPolicy {
    #[default]
    DiscardExcess,
    PreserveDebt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct FixedTime {
    timestep: Duration,
    max_steps_per_frame: NonZeroU32,
    max_debt_steps: NonZeroU32,
    catch_up_policy: FixedCatchUpPolicy,
    pending: Duration,
    scaled_remainder: ScaledTimeRemainder,
    delta: Duration,
    elapsed: Duration,
    tick: u64,
    steps_this_frame: u32,
    capped_this_frame: bool,
    discarded_this_frame: Duration,
    frame_active: bool,
}

impl Default for FixedTime {
    fn default() -> Self {
        Self {
            timestep: Self::DEFAULT_TIMESTEP,
            max_steps_per_frame: NonZeroU32::new(Self::DEFAULT_MAX_STEPS_PER_FRAME).unwrap(),
            max_debt_steps: NonZeroU32::new(Self::DEFAULT_MAX_DEBT_STEPS).unwrap(),
            catch_up_policy: FixedCatchUpPolicy::default(),
            pending: Duration::ZERO,
            scaled_remainder: ScaledTimeRemainder::ZERO,
            delta: Duration::ZERO,
            elapsed: Duration::ZERO,
            tick: 0,
            steps_this_frame: 0,
            capped_this_frame: false,
            discarded_this_frame: Duration::ZERO,
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

    pub fn with_max_steps_per_frame(mut self, steps: u32) -> Result<Self, TimeSettingsError> {
        self.set_max_steps_per_frame(steps)?;
        Ok(self)
    }

    pub fn with_max_debt_steps(mut self, steps: u32) -> Result<Self, TimeSettingsError> {
        self.set_max_debt_steps(steps)?;
        Ok(self)
    }

    pub fn with_catch_up_policy(mut self, policy: FixedCatchUpPolicy) -> Self {
        self.catch_up_policy = policy;
        self
    }

    pub const fn timestep(&self) -> Duration {
        self.timestep
    }

    pub const fn max_steps_per_frame(&self) -> u32 {
        self.max_steps_per_frame.get()
    }

    pub const fn max_debt_steps(&self) -> u32 {
        self.max_debt_steps.get()
    }

    pub const fn catch_up_policy(&self) -> FixedCatchUpPolicy {
        self.catch_up_policy
    }

    pub fn remainder(&self) -> Duration {
        duration_remainder(self.pending, self.timestep)
    }

    pub const fn scaled_remainder(&self) -> ScaledTimeRemainder {
        self.scaled_remainder
    }

    pub fn debt(&self) -> Duration {
        self.pending - self.remainder()
    }

    pub const fn delta(&self) -> Duration {
        self.delta
    }

    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub const fn tick(&self) -> u64 {
        self.tick
    }

    pub const fn steps_this_frame(&self) -> u32 {
        self.steps_this_frame
    }

    pub const fn capped_this_frame(&self) -> bool {
        self.capped_this_frame
    }

    pub const fn discarded_this_frame(&self) -> Duration {
        self.discarded_this_frame
    }

    pub fn set_timestep(&mut self, timestep: Duration) -> Result<(), TimeSettingsError> {
        self.ensure_inactive()?;
        if timestep.is_zero() {
            return Err(TimeSettingsError::ZeroFixedTimestep);
        }
        self.timestep = timestep;
        Ok(())
    }

    pub fn set_max_steps_per_frame(&mut self, steps: u32) -> Result<(), TimeSettingsError> {
        self.ensure_inactive()?;
        self.max_steps_per_frame =
            NonZeroU32::new(steps).ok_or(TimeSettingsError::ZeroMaxFixedStepsPerFrame)?;
        Ok(())
    }

    pub fn set_max_debt_steps(&mut self, steps: u32) -> Result<(), TimeSettingsError> {
        self.ensure_inactive()?;
        self.max_debt_steps =
            NonZeroU32::new(steps).ok_or(TimeSettingsError::ZeroMaxFixedDebtSteps)?;
        Ok(())
    }

    pub fn set_catch_up_policy(
        &mut self,
        policy: FixedCatchUpPolicy,
    ) -> Result<(), TimeSettingsError> {
        self.ensure_inactive()?;
        self.catch_up_policy = policy;
        Ok(())
    }

    fn ensure_inactive(&self) -> Result<(), TimeSettingsError> {
        if self.frame_active {
            return Err(TimeSettingsError::FixedFrameActive);
        }
        Ok(())
    }

    fn check_advance(&self, timestep: Duration, steps: u32) -> Result<(), FixedTimeError> {
        self.tick
            .checked_add(u64::from(steps))
            .ok_or(FixedTimeError::TickOverflow {
                tick: self.tick,
                attempted_steps: steps,
            })?;
        timestep
            .checked_mul(steps)
            .and_then(|delta| self.elapsed.checked_add(delta))
            .ok_or(FixedTimeError::ElapsedOverflow {
                elapsed: self.elapsed,
                timestep,
                attempted_steps: steps,
            })?;
        Ok(())
    }

    fn start_frame(&mut self, delta: Duration, enabled: bool) -> Result<u32, FixedTimeError> {
        self.reset_frame();
        if !enabled {
            return Ok(0);
        }
        self.pending =
            self.pending
                .checked_add(delta)
                .ok_or(FixedTimeError::PendingDurationOverflow {
                    pending: self.pending,
                    incoming: delta,
                })?;
        let pending_steps = self.pending.as_nanos() / self.timestep.as_nanos();
        let max_steps_per_frame = self.max_steps_per_frame();
        let max_debt_steps = self.max_debt_steps();
        if self.catch_up_policy == FixedCatchUpPolicy::PreserveDebt
            && pending_steps > u128::from(max_steps_per_frame) + u128::from(max_debt_steps)
        {
            return Err(FixedTimeError::CatchUpDebtExceeded {
                pending_steps,
                max_steps_per_frame,
                max_debt_steps,
            });
        }
        let steps = pending_steps.min(u128::from(max_steps_per_frame)) as u32;
        self.check_advance(self.timestep, steps)?;
        Ok(steps)
    }

    fn reset_frame(&mut self) {
        self.frame_active = true;
        self.delta = Duration::ZERO;
        self.steps_this_frame = 0;
        self.capped_this_frame = false;
        self.discarded_this_frame = Duration::ZERO;
    }

    pub(crate) fn advance_tick(&mut self, timestep: Duration) {
        self.delta = timestep;
        self.elapsed += timestep;
        self.tick += 1;
        self.steps_this_frame += 1;
    }

    pub(crate) fn consume_tick(&mut self) {
        self.pending -= self.timestep;
        self.advance_tick(self.timestep);
    }

    fn finish_frame(&mut self, exact_step: bool) {
        if !exact_step && self.steps_this_frame == self.max_steps_per_frame() {
            let remainder = self.remainder();
            self.capped_this_frame = self.pending > remainder;
            if self.capped_this_frame && self.catch_up_policy == FixedCatchUpPolicy::DiscardExcess {
                self.discarded_this_frame = self.pending - remainder;
                self.pending = remainder;
            }
        }
        self.frame_active = false;
    }
}

fn duration_remainder(duration: Duration, divisor: Duration) -> Duration {
    let nanos = duration.as_nanos() % divisor.as_nanos();
    Duration::new(
        (nanos / 1_000_000_000) as u64,
        (nanos % 1_000_000_000) as u32,
    )
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub struct RuntimeFrameStatus {
    pub frame: u64,
    pub real_delta: Duration,
    pub virtual_delta: Duration,
    pub real_delta_clamped: bool,
    pub simulation_paused: bool,
    pub fixed_steps: u32,
    pub fixed_steps_capped: bool,
    pub fixed_tick: u64,
    pub fixed_elapsed: Duration,
    pub fixed_remainder: Duration,
    pub fixed_debt: Duration,
    pub fixed_discarded: Duration,
}

pub(crate) struct FrameTime {
    real: RealTime,
    virtual_time: VirtualTime,
    pub(crate) fixed: FixedTime,
    pub(crate) steps: u32,
    clamped: bool,
    paused: bool,
    exact_step: Option<Duration>,
}

impl FrameTime {
    pub(crate) fn plan(
        world: &World,
        real_delta: Duration,
        paused: bool,
        exact_step: Option<Duration>,
    ) -> Result<Self, TimeFrameError> {
        let settings = get::<RuntimeTimeSettings>(world)?;
        let real = get::<RealTime>(world)?.advance(real_delta)?;
        let mut fixed = *get::<FixedTime>(world)?;
        let clamped = real_delta.min(settings.max_delta);
        let (virtual_delta, steps) = if let Some(step) = exact_step {
            if step.is_zero() {
                return Err(FixedTimeError::ZeroStep.into());
            }
            fixed.check_advance(step, 1)?;
            fixed.reset_frame();
            fixed.advance_tick(step);
            (step, 1)
        } else if paused || settings.paused {
            (Duration::ZERO, fixed.start_frame(Duration::ZERO, false)?)
        } else {
            let (delta, remainder) = fixed
                .scaled_remainder
                .advance(clamped, settings.time_scale)
                .ok_or(TimeFrameError::VirtualDeltaOverflow {
                    clamped_real_delta: clamped,
                    time_scale: settings.time_scale,
                })?;
            fixed.scaled_remainder = remainder;
            (delta, fixed.start_frame(delta, settings.time_scale > 0.0)?)
        };
        let virtual_time = get::<VirtualTime>(world)?.advance(virtual_delta)?;
        get::<RenderTime>(world)?;
        get::<RuntimeFrameStatus>(world)?;
        get::<crate::AppExitRequests>(world)?;
        Ok(Self {
            real,
            virtual_time,
            fixed,
            steps,
            clamped: exact_step.is_none() && clamped != real_delta,
            paused: exact_step.is_none() && (paused || settings.paused),
            exact_step,
        })
    }

    pub(crate) fn start(&self, world: &mut World, started_at: Instant) {
        *world.resource_mut::<RealTime>() = self.real;
        *world.resource_mut::<VirtualTime>() = self.virtual_time;
        *world.resource_mut::<FixedTime>() = self.fixed;
        if self.exact_step.is_none() {
            world.insert_resource(FrameExecutionStart {
                frame: self.real.frame,
                started_at,
            });
        }
    }

    pub(crate) fn settle(
        &mut self,
        world: &mut World,
    ) -> Result<RuntimeFrameStatus, TimeFrameError> {
        self.fixed.finish_frame(self.exact_step.is_some());
        *get_mut::<FixedTime>(world)? = self.fixed;
        if self.exact_step.is_none() {
            get_mut::<RenderTime>(world)?.update_from_fixed(&self.fixed);
        }
        let status = RuntimeFrameStatus {
            frame: self.real.frame,
            real_delta: self.real.delta,
            virtual_delta: self.virtual_time.delta,
            real_delta_clamped: self.clamped,
            simulation_paused: self.paused,
            fixed_steps: self.fixed.steps_this_frame(),
            fixed_steps_capped: self.fixed.capped_this_frame(),
            fixed_tick: self.fixed.tick(),
            fixed_elapsed: self.fixed.elapsed(),
            fixed_remainder: self.fixed.remainder(),
            fixed_debt: self.fixed.debt(),
            fixed_discarded: self.fixed.discarded_this_frame(),
        };
        *get_mut::<RuntimeFrameStatus>(world)? = status;
        Ok(status)
    }
}

pub(crate) fn get<T: Resource>(world: &World) -> Result<&T, TimeFrameError> {
    world.get_resource().ok_or(TimeFrameError::MissingResource {
        resource: std::any::type_name::<T>(),
    })
}

pub(crate) fn get_mut<T: Resource<Mutability = Mutable>>(
    world: &mut World,
) -> Result<nara_ecs::Mut<'_, T>, TimeFrameError> {
    world
        .get_resource_mut()
        .ok_or(TimeFrameError::MissingResource {
            resource: std::any::type_name::<T>(),
        })
}
