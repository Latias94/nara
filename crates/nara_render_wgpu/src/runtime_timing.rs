use std::time::{Duration, Instant};

/// Original managed Runtime calls whose returned wall spans can be observed independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WgpuRuntimeFramePhase {
    /// Preparing the original device namespace and surface authority.
    NamespacePreparation,
    /// Acquiring the original admitted native surface for the Runtime window.
    SurfaceAcquisition,
    /// Preparing the original candidate in its admitted namespace.
    CandidatePreparation,
    /// Executing the original program in its admitted namespace.
    ProgramExecution,
    /// Running the original surface presentation closure, including its validation and publication.
    SurfacePresentation,
}

impl WgpuRuntimeFramePhase {
    /// All five measured calls, in their original Runtime execution order.
    pub const ALL: [Self; 5] = [
        Self::NamespacePreparation,
        Self::SurfaceAcquisition,
        Self::CandidatePreparation,
        Self::ProgramExecution,
        Self::SurfacePresentation,
    ];

    const fn index(self) -> usize {
        match self {
            Self::NamespacePreparation => 0,
            Self::SurfaceAcquisition => 1,
            Self::CandidatePreparation => 2,
            Self::ProgramExecution => 3,
            Self::SurfacePresentation => 4,
        }
    }
}

/// Optional wall spans for five calls within one original managed Runtime transaction.
///
/// The five phases are disjoint within the original `render_runtime_packet` path. A span completes
/// only when its call returns, including a returned error. Dropping an unfinished capture or
/// unwinding does not complete its span, and unreached phases remain absent.
///
/// Each span includes driver waits inside its call. These values are not CPU usage, GPU duration,
/// scanout timing, or success receipts. The phases exclude other frame and setup work and must not
/// be treated as a complete frame duration. Attribution belongs to the containing backend
/// transaction observation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuRuntimeFrameTimings {
    phases: [Option<Duration>; WgpuRuntimeFramePhase::ALL.len()],
}

impl WgpuRuntimeFrameTimings {
    /// Returns a completed call's wall span, including zero, or `None` if it did not return.
    #[must_use]
    pub const fn get(self, phase: WgpuRuntimeFramePhase) -> Option<Duration> {
        self.phases[phase.index()]
    }

    pub(crate) fn record(&mut self, phase: WgpuRuntimeFramePhase, elapsed: Duration) {
        self.phases[phase.index()] = Some(elapsed);
    }
}

#[derive(Debug)]
pub(crate) struct RuntimeFramePhaseCapture {
    timer: Option<RuntimeFramePhaseTimer>,
}

impl RuntimeFramePhaseCapture {
    pub(crate) fn start(enabled: bool) -> Self {
        Self {
            timer: enabled.then(|| RuntimeFramePhaseTimer {
                started_at: Instant::now(),
            }),
        }
    }

    pub(crate) fn finish(self, record: impl FnOnce(Duration)) {
        if let Some(timer) = self.timer {
            record(timer.finish(Instant::now()));
        }
    }
}

#[derive(Debug)]
struct RuntimeFramePhaseTimer {
    started_at: Instant,
}

impl RuntimeFramePhaseTimer {
    fn finish(self, ended_at: Instant) -> Duration {
        ended_at.duration_since(self.started_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WgpuBackendTelemetryConfig, backend::WgpuRenderBackend};

    #[test]
    fn existing_configuration_defaults_require_explicit_phase_timing_opt_in() {
        for config in [
            WgpuBackendTelemetryConfig::default(),
            WgpuBackendTelemetryConfig::new(2, 3).unwrap(),
        ] {
            assert!(!config.runtime_frame_timings_enabled());
            let enabled = config.with_runtime_frame_timings();
            assert!(enabled.runtime_frame_timings_enabled());
            assert_eq!(
                enabled.max_pending_frame_completions(),
                config.max_pending_frame_completions()
            );
            assert_eq!(
                enabled.max_buffered_frame_completions(),
                config.max_buffered_frame_completions()
            );
            assert_eq!(enabled.with_runtime_frame_timings(), enabled);
        }
    }

    #[test]
    fn disabled_capture_has_no_timestamp_and_never_invokes_recording_closure() {
        let mut backend = WgpuRenderBackend::default();
        for config in [
            None,
            Some(WgpuBackendTelemetryConfig::default()),
            Some(WgpuBackendTelemetryConfig::new(2, 3).unwrap()),
        ] {
            if let Some(config) = config {
                backend.configure_telemetry(config);
            }
            backend.begin_frame_transaction(1, None);
            assert_eq!(backend.frame_transaction_stats().runtime_timings(), None);
            for _ in WgpuRuntimeFramePhase::ALL {
                let capture = backend.begin_runtime_frame_phase();
                assert!(capture.timer.is_none());
                capture.finish(|_| panic!("disabled timing must not invoke its recording closure"));
            }
            assert_eq!(backend.frame_transaction_stats().runtime_timings(), None);
        }
    }

    #[test]
    fn all_five_phase_slots_preserve_exact_synthetic_spans_including_zero() {
        assert_eq!(
            WgpuRuntimeFramePhase::ALL,
            [
                WgpuRuntimeFramePhase::NamespacePreparation,
                WgpuRuntimeFramePhase::SurfaceAcquisition,
                WgpuRuntimeFramePhase::CandidatePreparation,
                WgpuRuntimeFramePhase::ProgramExecution,
                WgpuRuntimeFramePhase::SurfacePresentation,
            ]
        );
        let spans = [
            Duration::ZERO,
            Duration::from_nanos(1),
            Duration::from_micros(7),
            Duration::from_millis(3),
            Duration::from_secs(2),
        ];
        let mut timings = WgpuRuntimeFrameTimings::default();
        for phase in WgpuRuntimeFramePhase::ALL {
            assert_eq!(timings.get(phase), None);
        }
        let started_at = Instant::now();
        for (phase, span) in WgpuRuntimeFramePhase::ALL.into_iter().zip(spans) {
            let timer = RuntimeFramePhaseTimer { started_at };
            timings.record(phase, timer.finish(started_at + span));
        }
        for (phase, span) in WgpuRuntimeFramePhase::ALL.into_iter().zip(spans) {
            assert_eq!(timings.get(phase), Some(span));
        }
        let snapshot = timings;
        timings.record(WgpuRuntimeFramePhase::NamespacePreparation, spans[4]);
        assert_eq!(
            snapshot.get(WgpuRuntimeFramePhase::NamespacePreparation),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn every_transaction_begin_resets_timings_and_configuration_applies_at_the_boundary() {
        let mut backend = WgpuRenderBackend::default();
        let config = WgpuBackendTelemetryConfig::new(2, 3).unwrap();
        backend.configure_telemetry(config.with_runtime_frame_timings());
        assert_eq!(backend.frame_transaction_stats().runtime_timings(), None);
        backend.begin_frame_transaction(17, None);
        for phase in WgpuRuntimeFramePhase::ALL {
            let capture = backend.begin_runtime_frame_phase();
            backend.finish_runtime_frame_phase(phase, capture);
        }
        let first = backend.frame_transaction_stats();
        let timings = first.runtime_timings().unwrap();
        for phase in WgpuRuntimeFramePhase::ALL {
            assert!(timings.get(phase).is_some());
        }

        backend.begin_frame_transaction(17, Some(19));
        let packet = backend.frame_transaction_stats();
        assert_eq!(packet.app_frame_index(), Some(19));
        assert_eq!(packet.render_transaction_frame_index(), Some(17));
        assert_eq!(
            packet.runtime_timings(),
            Some(WgpuRuntimeFrameTimings::default())
        );
        let capture = backend.begin_runtime_frame_phase();
        backend.finish_runtime_frame_phase(WgpuRuntimeFramePhase::CandidatePreparation, capture);
        let partial = backend.frame_transaction_stats();

        backend.configure_telemetry(config);
        assert_eq!(backend.frame_transaction_stats(), partial);
        backend.begin_frame_transaction(18, None);
        assert_eq!(backend.frame_transaction_stats().runtime_timings(), None);

        backend.configure_telemetry(config.with_runtime_frame_timings());
        backend.begin_frame_transaction(19, Some(20));
        assert_eq!(
            backend.frame_transaction_stats().runtime_timings(),
            Some(WgpuRuntimeFrameTimings::default())
        );
        assert_eq!(first.app_frame_index(), None);
        assert_eq!(first.render_transaction_frame_index(), Some(17));
        assert_eq!(first.runtime_timings(), Some(timings));
    }

    #[test]
    fn dropping_an_unfinished_capture_leaves_all_phases_absent() {
        let mut backend = WgpuRenderBackend::default();
        backend.configure_telemetry(
            WgpuBackendTelemetryConfig::default().with_runtime_frame_timings(),
        );
        backend.begin_frame_transaction(1, None);
        let initial = backend.frame_transaction_stats();
        for _ in WgpuRuntimeFramePhase::ALL {
            let capture = backend.begin_runtime_frame_phase();
            assert!(capture.timer.is_some());
        }
        assert_eq!(backend.frame_transaction_stats(), initial);
        assert_eq!(
            initial.runtime_timings(),
            Some(WgpuRuntimeFrameTimings::default())
        );
        assert_eq!(backend.presented_frame_this_transaction(), None);
    }

    fn return_phase_error(
        backend: &mut WgpuRenderBackend,
        failed_phase: WgpuRuntimeFramePhase,
    ) -> Result<(), &'static str> {
        for phase in WgpuRuntimeFramePhase::ALL {
            let capture = backend.begin_runtime_frame_phase();
            let result = if phase == failed_phase {
                Err("original phase returned an error")
            } else {
                Ok(())
            };
            backend.finish_runtime_frame_phase(phase, capture);
            result?;
        }
        Ok(())
    }

    #[test]
    fn returned_errors_keep_reached_spans_without_fabricating_submission_or_completion() {
        let mut backend = WgpuRenderBackend::default();
        backend.configure_telemetry(
            WgpuBackendTelemetryConfig::new(2, 3)
                .unwrap()
                .with_runtime_frame_timings(),
        );
        let completion = backend.frame_completion_stats();
        for (failed_index, failed_phase) in WgpuRuntimeFramePhase::ALL.into_iter().enumerate() {
            backend.begin_frame_transaction(7, Some(9));
            assert_eq!(
                return_phase_error(&mut backend, failed_phase),
                Err("original phase returned an error")
            );
            let stats = backend.frame_transaction_stats();
            let timings = stats.runtime_timings().unwrap();
            for (index, phase) in WgpuRuntimeFramePhase::ALL.into_iter().enumerate() {
                assert_eq!(timings.get(phase).is_some(), index <= failed_index);
            }
            assert_eq!(stats.packet_admissions(), 0);
            assert_eq!(stats.packet_rejections(), 0);
            assert_eq!(stats.surface_acquire_attempts(), 0);
            assert_eq!(stats.surface_acquires(), 0);
            assert_eq!(stats.queue_submissions(), 0);
            assert_eq!(stats.presents(), 0);
            assert_eq!(backend.frame_completion_stats(), completion);
            let observation = backend.take_observation();
            assert_eq!(observation.instance_id(), backend.instance_id());
            assert_eq!(observation.device_epoch(), backend.device_epoch());
            assert_eq!(observation.frame_transaction_stats(), stats);
            assert_eq!(observation.presented_frame_this_transaction(), None);
        }
    }

    #[cfg(panic = "unwind")]
    #[test]
    fn unwind_does_not_complete_the_current_or_unreached_phases() {
        let mut backend = WgpuRenderBackend::default();
        backend.configure_telemetry(
            WgpuBackendTelemetryConfig::default().with_runtime_frame_timings(),
        );
        let completion = backend.frame_completion_stats();
        for (unwound_index, unwound_phase) in WgpuRuntimeFramePhase::ALL.into_iter().enumerate() {
            backend.begin_frame_transaction(1, None);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                for phase in WgpuRuntimeFramePhase::ALL {
                    let capture = backend.begin_runtime_frame_phase();
                    if phase == unwound_phase {
                        panic!("original phase unwound");
                    }
                    backend.finish_runtime_frame_phase(phase, capture);
                }
            }));
            assert_eq!(
                result.unwrap_err().downcast_ref::<&str>(),
                Some(&"original phase unwound")
            );
            let stats = backend.frame_transaction_stats();
            let timings = stats.runtime_timings().unwrap();
            for (index, phase) in WgpuRuntimeFramePhase::ALL.into_iter().enumerate() {
                assert_eq!(timings.get(phase).is_some(), index < unwound_index);
            }
            assert_eq!(stats.queue_submissions(), 0);
            assert_eq!(stats.presents(), 0);
            assert_eq!(backend.presented_frame_this_transaction(), None);
            assert_eq!(backend.frame_completion_stats(), completion);
        }
    }
}
