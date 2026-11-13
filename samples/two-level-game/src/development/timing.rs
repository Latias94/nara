use std::time::{Duration, Instant};

use nara::project_tooling::cargo_build::CargoBuildPoint;

/// Fixed Host operation boundaries, measured only after timing is explicitly enabled.
///
/// These are elapsed Host calls, including IPC, native polling and waiting performed by the
/// original operation. They do not measure GPU duration or scanout. `StateRestore` includes its
/// verification capture, `ProbeStep` includes its capture and optional presentation check, and
/// `Adoption` includes its source, capture and presentation checks. `CargoBuild` includes its six
/// disjoint checkpoint intervals. Parent totals are inclusive; adding phase totals would count
/// overlapping work more than once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevelopmentTimingPhase {
    /// Source fence capture, build binding and each later source verification call.
    SourceCheck,
    /// The existing Cargo adapter call, including its executable capture and validation.
    CargoBuild,
    /// Adapter entry through BeforeMetadata, including input admission and tool observation.
    CargoAdmissionAndToolObservation,
    /// BeforeMetadata through BeforeBuild, including metadata and input/tool rechecks.
    CargoMetadataAndRechecks,
    /// BeforeBuild through AfterBuild, including compilation, checks, decoding and artifact hash.
    CargoCompilationAndArtifactChecks,
    /// AfterBuild through BeforeCapture, including source/tool/lease and capture limit checks.
    CargoCaptureAdmission,
    /// BeforeCapture through BeforeReturn, including tool rechecks, captured bytes and provenance.
    CargoCapturedBytesAndProvenance,
    /// BeforeReturn through adapter return, including final source/tool/lease/artifact validation.
    CargoFinalValidation,
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
    pub const ALL: [Self; 19] = [
        Self::SourceCheck,
        Self::CargoBuild,
        Self::CargoAdmissionAndToolObservation,
        Self::CargoMetadataAndRechecks,
        Self::CargoCompilationAndArtifactChecks,
        Self::CargoCaptureAdmission,
        Self::CargoCapturedBytesAndProvenance,
        Self::CargoFinalValidation,
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

/// Cumulative observations for completed calls or elapsed Cargo segments at one boundary.
///
/// `count` includes successes and failures. `failures` counts returned `Err` values, including
/// incomplete cleanup drives; it does not reinterpret a successful result's embedded status.
/// A Cargo error belongs to its current segment; earlier completed segments remain successful.
/// Unwinding does not complete the current call or segment. Counters and total elapsed time
/// saturate at their type's maximum rather than wrapping or changing the original result.
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

pub(super) fn measure_cargo<T, E>(
    timings: &mut Option<DevelopmentTimingSnapshot>,
    operation: impl FnOnce(&mut dyn FnMut(CargoBuildPoint)) -> Result<T, E>,
) -> Result<T, E> {
    measure_cargo_with_clock(timings, operation, Instant::now)
}

fn measure_cargo_with_clock<T, E>(
    timings: &mut Option<DevelopmentTimingSnapshot>,
    operation: impl FnOnce(&mut dyn FnMut(CargoBuildPoint)) -> Result<T, E>,
    mut now: impl FnMut() -> Instant,
) -> Result<T, E> {
    let mut started = timings.is_some().then(&mut now);
    let mut phase = DevelopmentTimingPhase::CargoAdmissionAndToolObservation;
    let result = operation(&mut |point| {
        if let (Some(started), Some(timings)) = (started.as_mut(), timings.as_mut()) {
            let finished = now();
            timings.record(phase, finished.duration_since(*started), false);
            *started = finished;
            phase = match point {
                CargoBuildPoint::BeforeMetadata => DevelopmentTimingPhase::CargoMetadataAndRechecks,
                CargoBuildPoint::BeforeBuild => {
                    DevelopmentTimingPhase::CargoCompilationAndArtifactChecks
                }
                CargoBuildPoint::AfterBuild => DevelopmentTimingPhase::CargoCaptureAdmission,
                CargoBuildPoint::BeforeCapture => {
                    DevelopmentTimingPhase::CargoCapturedBytesAndProvenance
                }
                CargoBuildPoint::BeforeReturn => DevelopmentTimingPhase::CargoFinalValidation,
            };
        }
    });
    if let (Some(started), Some(timings)) = (started, timings.as_mut()) {
        timings.record(phase, now().duration_since(started), result.is_err());
    }
    result
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::development::{DevelopmentHost, RetirementStatus};

    const CARGO_POINTS: [CargoBuildPoint; 5] = [
        CargoBuildPoint::BeforeMetadata,
        CargoBuildPoint::BeforeBuild,
        CargoBuildPoint::AfterBuild,
        CargoBuildPoint::BeforeCapture,
        CargoBuildPoint::BeforeReturn,
    ];
    const CARGO_SEGMENTS: [DevelopmentTimingPhase; 6] = [
        DevelopmentTimingPhase::CargoAdmissionAndToolObservation,
        DevelopmentTimingPhase::CargoMetadataAndRechecks,
        DevelopmentTimingPhase::CargoCompilationAndArtifactChecks,
        DevelopmentTimingPhase::CargoCaptureAdmission,
        DevelopmentTimingPhase::CargoCapturedBytesAndProvenance,
        DevelopmentTimingPhase::CargoFinalValidation,
    ];

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
    fn cargo_checkpoints_record_exact_disjoint_intervals_inside_the_total() {
        let origin = Instant::now();
        let mut clock = [0, 3, 8, 15, 26, 39, 56]
            .map(|offset| origin + Duration::from_nanos(offset))
            .into_iter();
        let mut timings = Some(DevelopmentTimingSnapshot::default());
        let result = measure(
            &mut timings,
            DevelopmentTimingPhase::CargoBuild,
            |timings| {
                measure_cargo_with_clock(
                    timings,
                    |checkpoint| {
                        for point in CARGO_POINTS {
                            checkpoint(point);
                        }
                        Ok::<_, &str>(17)
                    },
                    || clock.next().expect("one clock read per elapsed boundary"),
                )
            },
        );
        assert_eq!(result, Ok(17));
        assert_eq!(clock.next(), None);
        let completed = timings.unwrap();
        let mut segment_total = Duration::ZERO;
        for (phase, elapsed) in CARGO_SEGMENTS
            .into_iter()
            .zip([3, 5, 7, 11, 13, 17].map(Duration::from_nanos))
        {
            assert_eq!(
                completed.get(phase),
                DevelopmentPhaseTiming {
                    count: 1,
                    failures: 0,
                    total_elapsed: elapsed,
                    max_elapsed: elapsed,
                },
                "{phase:?}"
            );
            segment_total += elapsed;
        }
        assert_eq!(segment_total, Duration::from_nanos(56));
        let total = completed.get(DevelopmentTimingPhase::CargoBuild);
        assert_eq!((total.count, total.failures), (1, 0));
        assert_eq!(
            completed.get(DevelopmentTimingPhase::SourceCheck),
            DevelopmentPhaseTiming::default()
        );
    }

    #[test]
    fn cargo_errors_finish_only_the_current_segment_and_preserve_the_original_error() {
        let origin = Instant::now();
        for reached in 0..=CARGO_POINTS.len() {
            let mut timings = Some(DevelopmentTimingSnapshot::default());
            let mut clock_reads = 0;
            let failure = Box::new(reached);
            let original_error = std::ptr::from_ref(failure.as_ref());
            let result = measure(
                &mut timings,
                DevelopmentTimingPhase::CargoBuild,
                |timings| {
                    measure_cargo_with_clock(
                        timings,
                        |checkpoint| {
                            for point in CARGO_POINTS.into_iter().take(reached) {
                                checkpoint(point);
                            }
                            Err::<(), _>(failure)
                        },
                        || {
                            let observed = origin + Duration::from_nanos(clock_reads);
                            clock_reads += 1;
                            observed
                        },
                    )
                },
            );
            let returned_error = result.unwrap_err();
            assert_eq!(std::ptr::from_ref(returned_error.as_ref()), original_error);
            assert_eq!(*returned_error, reached);
            assert_eq!(clock_reads, reached as u64 + 2);
            let completed = timings.unwrap();
            for (index, phase) in CARGO_SEGMENTS.into_iter().enumerate() {
                let expected = if index <= reached {
                    DevelopmentPhaseTiming {
                        count: 1,
                        failures: u64::from(index == reached),
                        total_elapsed: Duration::from_nanos(1),
                        max_elapsed: Duration::from_nanos(1),
                    }
                } else {
                    DevelopmentPhaseTiming::default()
                };
                assert_eq!(completed.get(phase), expected, "{reached}: {phase:?}");
            }
            let total = completed.get(DevelopmentTimingPhase::CargoBuild);
            assert_eq!((total.count, total.failures), (1, 1));
        }
    }

    #[test]
    fn disabled_cargo_measurement_never_reads_the_clock_or_enables_observation() {
        for failed in [false, true] {
            let mut timings = None;
            let mut checkpoints = 0;
            let result = measure_cargo_with_clock(
                &mut timings,
                |checkpoint| {
                    for point in CARGO_POINTS {
                        checkpoint(point);
                        checkpoints += 1;
                    }
                    if failed {
                        Err("original failure")
                    } else {
                        Ok(17)
                    }
                },
                || panic!("disabled Cargo timing must never read the clock"),
            );
            assert_eq!(
                result,
                if failed {
                    Err("original failure")
                } else {
                    Ok(17)
                }
            );
            assert_eq!(checkpoints, CARGO_POINTS.len());
            assert_eq!(timings, None);
        }
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
