use std::time::{Duration, Instant};

use nara::app::{FixedTime, RuntimeTimeSettings};

pub(super) struct FrameClock {
    last_frame: Instant,
    period: Duration,
    max_delta: Duration,
}

#[derive(Debug, thiserror::Error)]
#[error("Server frame elapsed {elapsed:?} exceeds the configured clamp {max_delta:?}")]
pub(super) struct FrameOverrun {
    elapsed: Duration,
    max_delta: Duration,
}

impl FrameClock {
    pub(super) fn new(last_frame: Instant, fixed: FixedTime, time: RuntimeTimeSettings) -> Self {
        let max_delta = time.max_delta();
        let minimum_scale = if fixed.max_steps_per_frame() == 1 {
            0.5
        } else {
            1.0
        };
        let wake_scale = (f64::from(time.time_scale()) / minimum_scale).max(1.0);
        let period = fixed
            .timestep()
            .div_f64(wake_scale)
            .min(max_delta / 2)
            .max(Duration::from_nanos(1));
        Self {
            last_frame,
            period,
            max_delta,
        }
    }

    pub(super) fn remaining(&self, now: Instant) -> Duration {
        self.period
            .saturating_sub(now.duration_since(self.last_frame))
    }

    pub(super) fn cap_period(&mut self, maximum: Duration) {
        self.period = self.period.min(maximum);
    }

    #[cfg(nara_cooked_player)]
    pub(super) fn reset(&mut self, now: Instant) {
        self.last_frame = now;
    }

    pub(super) fn elapsed(&self, now: Instant) -> Result<Duration, FrameOverrun> {
        let elapsed = now.duration_since(self.last_frame);
        if elapsed > self.max_delta {
            return Err(FrameOverrun {
                elapsed,
                max_delta: self.max_delta,
            });
        }
        Ok(elapsed)
    }

    pub(super) fn take_due(&mut self, now: Instant) -> Result<Option<Duration>, FrameOverrun> {
        let elapsed = self.elapsed(now)?;
        if elapsed < self.period {
            return Ok(None);
        }
        self.last_frame = now;
        Ok(Some(elapsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(start: Instant, timestep: Duration, max_delta: Duration) -> FrameClock {
        FrameClock::new(
            start,
            FixedTime::new(timestep).unwrap(),
            RuntimeTimeSettings::new(false, 1.0, max_delta).unwrap(),
        )
    }

    #[test]
    fn tight_clamp_allows_substep_frames_without_changing_the_supplied_elapsed_time() {
        let start = Instant::now();
        let mut clock = clock(start, Duration::from_millis(20), Duration::from_millis(10));
        assert_eq!(clock.remaining(start), Duration::from_millis(5));
        assert_eq!(
            clock.take_due(start + Duration::from_millis(4)).unwrap(),
            None
        );
        assert_eq!(
            clock.take_due(start + Duration::from_millis(7)).unwrap(),
            Some(Duration::from_millis(7))
        );
        assert_eq!(
            clock.take_due(start + Duration::from_millis(13)).unwrap(),
            Some(Duration::from_millis(6))
        );
    }

    #[test]
    fn overdue_intervals_are_rejected_without_advancing_the_baseline() {
        let start = Instant::now();
        let max_delta = Duration::from_millis(250);
        let mut clock = clock(start, Duration::from_millis(20), max_delta);
        assert_eq!(clock.remaining(start), Duration::from_millis(20));
        assert_eq!(clock.take_due(start + max_delta).unwrap(), Some(max_delta));
        let baseline = clock.last_frame;
        let elapsed = max_delta + Duration::from_nanos(1);
        let error = clock.take_due(baseline + elapsed).unwrap_err();
        assert_eq!(error.elapsed, elapsed);
        assert_eq!(clock.last_frame, baseline);
    }

    #[test]
    fn shorter_drain_wakes_preserve_the_original_elapsed_interval() {
        let start = Instant::now();
        let mut clock = clock(start, Duration::from_secs(20), Duration::from_secs(60));
        let before_drain = start + Duration::from_millis(37);
        assert_eq!(clock.take_due(before_drain).unwrap(), None);
        clock.cap_period(Duration::from_millis(50));
        assert_eq!(clock.remaining(before_drain), Duration::from_millis(13));
        assert_eq!(
            clock.take_due(start + Duration::from_millis(51)).unwrap(),
            Some(Duration::from_millis(51))
        );
    }

    #[test]
    fn fast_scale_samples_before_the_fixed_step_budget_is_spent() {
        let start = Instant::now();
        let fixed = FixedTime::new(Duration::from_millis(20)).unwrap();
        let time = RuntimeTimeSettings::default().with_time_scale(8.0).unwrap();
        let clock = FrameClock::new(start, fixed, time);
        assert_eq!(clock.remaining(start), Duration::from_micros(2500));
        let one_step = fixed.with_max_steps_per_frame(1).unwrap();
        let clock = FrameClock::new(start, one_step, time);
        assert_eq!(clock.remaining(start), Duration::from_micros(1250));
    }
}
