use std::{fmt, time::Duration};

use nara::app::{FixedCatchUpPolicy, FixedTime, RuntimeTimeContinuation, RuntimeTimeSettings};

use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor, value::MapAccessDeserializer},
};

use super::{MAX_TIMESTEP_NANOS, ResumeError};

/// Runtime scheduling settings and pending virtual time, without old clock or frame identities.
///
/// The game bounds the timestep to one second. Both caps retain the engine's complete nonzero
/// `u32` range. Durations use the complete `u64` nanosecond range; values outside that portable
/// representation must be refused by capture rather than truncated or clamped. In particular,
/// the engine's default 250 ms maximum delta is supported without limiting longer native deltas.
/// Pending time is the single source for whole-tick debt and the sub-tick interpolation remainder.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTimeState {
    pub timestep_nanos: u64,
    pub max_steps_per_frame: u32,
    pub max_debt_steps: u32,
    #[serde(deserialize_with = "deserialize_policy")]
    pub catch_up_policy: RuntimeCatchUpPolicy,
    /// Every finite nonnegative `f32` is supported, including both signed zeros.
    pub time_scale: f32,
    /// Nonzero maximum real delta before scaling, independent of the fixed timestep limit.
    pub max_delta_nanos: u64,
    /// May be zero; only `PreserveDebt` bounds the whole-tick quotient by `max_debt_steps`.
    pub pending_nanos: u64,
}

/// Game-owned wire names for the engine's two fixed catch-up policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCatchUpPolicy {
    /// Retained debt is legal after a policy change while paused; the next running frame settles it.
    DiscardExcess,
    /// Whole pending ticks must fit the configured debt cap at a completed frame boundary.
    PreserveDebt,
}

impl RuntimeTimeState {
    /// Encodes actual time-owner facts without truncating a larger native Duration.
    pub fn from_continuation(value: RuntimeTimeContinuation) -> Result<Self, ResumeError> {
        let nanos = |field, duration: Duration| {
            u64::try_from(duration.as_nanos())
                .map_err(|_| ResumeError::RuntimeDurationOutOfRange { field })
        };
        let state = Self {
            timestep_nanos: nanos("timestep", value.timestep())?,
            max_steps_per_frame: value.max_steps_per_frame(),
            max_debt_steps: value.max_debt_steps(),
            catch_up_policy: match value.catch_up_policy() {
                FixedCatchUpPolicy::DiscardExcess => RuntimeCatchUpPolicy::DiscardExcess,
                FixedCatchUpPolicy::PreserveDebt => RuntimeCatchUpPolicy::PreserveDebt,
            },
            time_scale: value.time_scale(),
            max_delta_nanos: nanos("max_delta", value.max_delta())?,
            pending_nanos: nanos("pending", value.pending())?,
        };
        state.validate()?;
        Ok(state)
    }

    /// Reconstructs validated portable data; applying it still requires the original Runtime.
    pub fn to_continuation(self) -> Result<RuntimeTimeContinuation, ResumeError> {
        self.validate()?;
        let invalid = |error: nara::app::TimeSettingsError| {
            ResumeError::InvalidRuntimeTime(error.to_string())
        };
        let settings = RuntimeTimeSettings::new(
            true,
            self.time_scale,
            Duration::from_nanos(self.max_delta_nanos),
        )
        .map_err(invalid)?;
        let fixed = FixedTime::new(Duration::from_nanos(self.timestep_nanos))
            .and_then(|fixed| fixed.with_max_steps_per_frame(self.max_steps_per_frame))
            .and_then(|fixed| fixed.with_max_debt_steps(self.max_debt_steps))
            .map_err(invalid)?
            .with_catch_up_policy(match self.catch_up_policy {
                RuntimeCatchUpPolicy::DiscardExcess => FixedCatchUpPolicy::DiscardExcess,
                RuntimeCatchUpPolicy::PreserveDebt => FixedCatchUpPolicy::PreserveDebt,
            });
        RuntimeTimeContinuation::new(&settings, &fixed, Duration::from_nanos(self.pending_nanos))
            .map_err(|error| ResumeError::InvalidRuntimeTime(error.to_string()))
    }

    pub(super) fn validate(&self) -> Result<(), ResumeError> {
        if self.timestep_nanos == 0 || self.timestep_nanos > MAX_TIMESTEP_NANOS {
            return Err(ResumeError::InvalidTimestep(self.timestep_nanos));
        }
        if self.max_steps_per_frame == 0 {
            return Err(ResumeError::ZeroMaxStepsPerFrame);
        }
        if self.max_debt_steps == 0 {
            return Err(ResumeError::ZeroMaxDebtSteps);
        }
        if !self.time_scale.is_finite() || self.time_scale < 0.0 {
            return Err(ResumeError::InvalidTimeScale {
                value: self.time_scale,
            });
        }
        if self.max_delta_nanos == 0 {
            return Err(ResumeError::ZeroMaxDelta);
        }
        let pending_steps = self.pending_nanos / self.timestep_nanos;
        if self.catch_up_policy == RuntimeCatchUpPolicy::PreserveDebt
            && pending_steps > u64::from(self.max_debt_steps)
        {
            return Err(ResumeError::ExcessPendingDebt {
                pending_steps,
                max_debt_steps: self.max_debt_steps,
            });
        }
        Ok(())
    }
}

fn deserialize_policy<'de, D>(deserializer: D) -> Result<RuntimeCatchUpPolicy, D::Error>
where
    D: Deserializer<'de>,
{
    let policy = String::deserialize(deserializer)?;
    RuntimeCatchUpPolicy::deserialize(serde::de::value::StrDeserializer::new(&policy))
}

pub(super) fn deserialize_object<'de, D>(deserializer: D) -> Result<RuntimeTimeState, D::Error>
where
    D: Deserializer<'de>,
{
    struct TimeObject;

    impl<'de> Visitor<'de> for TimeObject {
        type Value = RuntimeTimeState;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a Runtime time object")
        }

        fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            RuntimeTimeState::deserialize(MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_map(TimeObject)
}
