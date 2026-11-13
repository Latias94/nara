use std::time::{Duration, Instant};

/// Fixed Host operation boundaries, measured only after timing is explicitly enabled.
///
/// These are elapsed Host calls, including IPC, native polling and waiting performed by the
/// original operation. They do not measure GPU duration or scanout. `StateRestore` includes its
/// verification capture, `ProbeStep` includes its capture and optional presentation check, and
/// `Adoption` includes its source, capture and presentation checks. Parent totals are inclusive;
/// adding phase totals would count overlapping work more than once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevelopmentTimingPhase {
    /// Source fence capture, build binding and each later source verification call.
    SourceCheck,
    /// The existing Cargo adapter call, including its executable capture and validation.
    CargoBuild,
    ExecutableInstall,
    /// Original process startup through its READY handshake, before the Host's later observation.
    ChildStartup,
    StateRestore,
    ProbeStep,
    StateCapture,
    Presentation,
    Adoption,
    Activation,
    Pause,
    Resume,
    /// Each process or retained-build cleanup drive and the final store close, not a child count.
    Retirement,
}

impl DevelopmentTimingPhase {
    /// Every fixed phase, including phases that have not been reached in this Host.
    pub const ALL: [Self; 13] = [
        Self::SourceCheck,
        Self::CargoBuild,
        Self::ExecutableInstall,
        Self::ChildStartup,
        Self::StateRestore,
        Self::ProbeStep,
        Self::StateCapture,
        Self::Presentation,
        Self::Adoption,
        Self::Activation,
        Self::Pause,
        Self::Resume,
        Self::Retirement,
    ];
}

/// Cumulative observations for completed calls at one boundary.
///
/// `count` includes successes and failures. `failures` counts returned `Err` values, including
/// incomplete cleanup drives; it does not reinterpret a successful result's embedded status.
/// Unwinding or unfinished calls are not recorded. Counters and total elapsed time saturate at
/// their type's maximum rather than wrapping or changing the original operation's result.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DevelopmentPhaseTiming {
    pub count: u64,
    pub failures: u64,
    pub total_elapsed: Duration,
    pub max_elapsed: Duration,
}

/// A fixed-size copy of observations since timing was first enabled for this Host.
///
/// Subtract successive counts, failures and total elapsed times to attribute an attempt while
/// the cumulative fields remain unsaturated. `max_elapsed` is the lifetime maximum, so a
/// difference of maxima is not an attempt's maximum. Missing phases remain zero. No samples,
/// process identities or executable state are retained here, and these values grant no authority.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DevelopmentTimingSnapshot {
    phases: [DevelopmentPhaseTiming; DevelopmentTimingPhase::ALL.len()],
}

impl DevelopmentTimingSnapshot {
    #[must_use]
    pub const fn get(&self, phase: DevelopmentTimingPhase) -> DevelopmentPhaseTiming {
        self.phases[phase as usize]
    }

    fn record(&mut self, phase: DevelopmentTimingPhase, elapsed: Duration, failed: bool) {
        let timing = &mut self.phases[phase as usize];
        timing.count = timing.count.saturating_add(1);
        timing.failures = timing.failures.saturating_add(u64::from(failed));
        timing.total_elapsed = timing.total_elapsed.saturating_add(elapsed);
        timing.max_elapsed = timing.max_elapsed.max(elapsed);
    }
}

pub(super) struct Timer(Option<Instant>);

impl Timer {
    pub(super) fn start(enabled: bool) -> Self {
        Self(enabled.then(Instant::now))
    }

    pub(super) fn finish(
        self,
        timings: &mut Option<DevelopmentTimingSnapshot>,
        phase: DevelopmentTimingPhase,
        failed: bool,
    ) {
        if let (Some(started), Some(timings)) = (self.0, timings.as_mut()) {
            timings.record(phase, started.elapsed(), failed);
        }
    }
}

pub(super) fn measure<T, E>(
    timings: &mut Option<DevelopmentTimingSnapshot>,
    phase: DevelopmentTimingPhase,
    operation: impl FnOnce(&mut Option<DevelopmentTimingSnapshot>) -> Result<T, E>,
) -> Result<T, E> {
    let timer = Timer::start(timings.is_some());
    let result = operation(timings);
    timer.finish(timings, phase, result.is_err());
    result
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::development::{DevelopmentHost, RetirementStatus};

    #[test]
    fn host_timing_is_opt_in_and_enabling_again_preserves_completed_work() {
        let work_parent = std::env::temp_dir().canonicalize().unwrap();
        let mut host =
            DevelopmentHost::new(Path::new(env!("CARGO_MANIFEST_DIR")), &work_parent).unwrap();
        assert_eq!(host.timing_snapshot(), None);
        assert!(host.capture().is_err());
        assert_eq!(host.timing_snapshot(), None);

        host.enable_timings();
        assert_eq!(
            host.timing_snapshot(),
            Some(DevelopmentTimingSnapshot::default())
        );
        assert!(host.capture().is_err());
        let capture = host
            .timing_snapshot()
            .unwrap()
            .get(DevelopmentTimingPhase::StateCapture);
        assert_eq!((capture.count, capture.failures), (1, 1));
        assert_eq!(host.close(), RetirementStatus::Complete);
        let completed = host.timing_snapshot().unwrap();
        let retirement = completed.get(DevelopmentTimingPhase::Retirement);
        assert_eq!((retirement.count, retirement.failures), (1, 0));
        assert_eq!(
            completed.get(DevelopmentTimingPhase::CargoBuild),
            DevelopmentPhaseTiming::default()
        );
        host.enable_timings();
        assert_eq!(host.timing_snapshot(), Some(completed));
    }

    #[test]
    fn measurement_preserves_results_and_records_only_after_the_operation_returns() {
        let mut disabled = None;
        let mut calls = 0;
        let result = measure(&mut disabled, DevelopmentTimingPhase::CargoBuild, |_| {
            calls += 1;
            Err::<(), _>("original failure")
        });
        assert_eq!(result, Err("original failure"));
        assert_eq!(calls, 1);
        assert_eq!(disabled, None);

        let mut enabled = Some(DevelopmentTimingSnapshot::default());
        let success = measure(
            &mut enabled,
            DevelopmentTimingPhase::CargoBuild,
            |timings| {
                assert_eq!(
                    timings
                        .unwrap()
                        .get(DevelopmentTimingPhase::CargoBuild)
                        .count,
                    0
                );
                Ok::<_, &str>(17)
            },
        );
        assert_eq!(success, Ok(17));
        let failure = measure(
            &mut enabled,
            DevelopmentTimingPhase::CargoBuild,
            |timings| {
                let before = timings.unwrap().get(DevelopmentTimingPhase::CargoBuild);
                assert_eq!((before.count, before.failures), (1, 0));
                Err::<(), _>("same failure")
            },
        );
        assert_eq!(failure, Err("same failure"));
        let completed = enabled.unwrap();
        let build = completed.get(DevelopmentTimingPhase::CargoBuild);
        assert_eq!((build.count, build.failures), (2, 1));
        assert_eq!(
            completed.get(DevelopmentTimingPhase::ChildStartup),
            DevelopmentPhaseTiming::default()
        );
    }

    #[test]
    fn cumulative_totals_keep_all_outcomes_and_the_largest_completed_duration() {
        let mut timings = DevelopmentTimingSnapshot::default();
        timings.record(
            DevelopmentTimingPhase::StateCapture,
            Duration::from_nanos(5),
            false,
        );
        timings.record(
            DevelopmentTimingPhase::StateCapture,
            Duration::from_nanos(2),
            true,
        );
        timings.record(
            DevelopmentTimingPhase::StateCapture,
            Duration::from_nanos(8),
            false,
        );
        assert_eq!(
            timings.get(DevelopmentTimingPhase::StateCapture),
            DevelopmentPhaseTiming {
                count: 3,
                failures: 1,
                total_elapsed: Duration::from_nanos(15),
                max_elapsed: Duration::from_nanos(8),
            }
        );
        assert_eq!(
            timings.get(DevelopmentTimingPhase::Presentation),
            DevelopmentPhaseTiming::default()
        );
    }

    #[test]
    fn exhausted_counters_and_elapsed_totals_saturate_without_wrapping() {
        let mut timings = DevelopmentTimingSnapshot::default();
        timings.phases[DevelopmentTimingPhase::Retirement as usize] = DevelopmentPhaseTiming {
            count: u64::MAX - 1,
            failures: u64::MAX - 1,
            total_elapsed: Duration::MAX - Duration::from_nanos(5),
            max_elapsed: Duration::MAX - Duration::from_nanos(5),
        };
        timings.record(
            DevelopmentTimingPhase::Retirement,
            Duration::from_nanos(10),
            true,
        );
        timings.record(DevelopmentTimingPhase::Retirement, Duration::MAX, true);
        assert_eq!(
            timings.get(DevelopmentTimingPhase::Retirement),
            DevelopmentPhaseTiming {
                count: u64::MAX,
                failures: u64::MAX,
                total_elapsed: Duration::MAX,
                max_elapsed: Duration::MAX,
            }
        );
    }
}
