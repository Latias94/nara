use std::time::{Duration, Instant};

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
    pub(super) fn new(last_frame: Instant, timestep: Duration, max_delta: Duration) -> Self {
        let period = timestep.min((max_delta / 2).max(Duration::from_nanos(1)));
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

    #[test]
    fn tight_clamp_allows_substep_frames_without_changing_the_supplied_elapsed_time() {
        let start = Instant::now();
        let mut clock =
            FrameClock::new(start, Duration::from_millis(20), Duration::from_millis(10));
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
        let mut clock = FrameClock::new(start, Duration::from_millis(20), max_delta);
        assert_eq!(clock.remaining(start), Duration::from_millis(20));
        assert_eq!(clock.take_due(start + max_delta).unwrap(), Some(max_delta));
        let baseline = clock.last_frame;
        let elapsed = max_delta + Duration::from_nanos(1);
        let error = clock.take_due(baseline + elapsed).unwrap_err();
        assert_eq!(error.elapsed, elapsed);
        assert_eq!(clock.last_frame, baseline);
    }
}
