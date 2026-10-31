use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use nara::{
    app::AppExit,
    fs::DirectoryCapability,
    project_host::{
        DesktopPresentationControl, DesktopPresentationObservation, DesktopRun, DesktopRunIntent,
        DesktopRunOptions,
    },
    render_wgpu::WgpuBackendTelemetryConfig,
    ui_render::advanced::latest_presented_ui_observation,
};
use nara_reference_game::desktop_wave_recipe;

mod evidence;
mod measurement;

use evidence::ArmedCandidate;
pub(crate) use evidence::CandidateSmokeEvidence;
use measurement::{CandidateCompletionSample, CandidateMeasurement};

const CANDIDATE_SMOKE_DEADLINE: Duration = Duration::from_secs(30);
const CANDIDATE_DEADLINE_EXCEEDED: &str =
    "desktop candidate did not complete its bounded product measurement";

pub(crate) type CandidateSmokeEvidenceSink = Arc<Mutex<Option<CandidateSmokeEvidence>>>;

pub(crate) fn candidate_smoke_run(
    project_root: DirectoryCapability,
) -> (DesktopRun, CandidateSmokeEvidenceSink) {
    let evidence = Arc::new(Mutex::new(None));
    let mut measurement = CandidateSmokeState {
        deadline: None,
        evidence: Arc::clone(&evidence),
        phase: CandidateSmokePhase::AwaitPresentedReady,
        sealing: false,
    };
    let execution = DesktopRunOptions::new()
        .with_telemetry(WgpuBackendTelemetryConfig::default())
        .with_presentation_observer(move |observation| match measurement.observe(&observation) {
            Ok(control) => control,
            Err(message) => {
                eprintln!("reference-game.desktop.candidate-measurement-failed: {message}");
                measurement.phase = CandidateSmokePhase::Failed;
                DesktopPresentationControl::Exit(AppExit::Requested)
            }
        });
    (
        DesktopRun::new(
            project_root,
            DesktopRunIntent::new()
                .with_profile("desktop")
                .with_recipe(desktop_wave_recipe().expect("the desktop recipe is statically valid"))
                .with_execution(execution),
        ),
        evidence,
    )
}

/// Measurement state belongs to the native action's CPU observer, independently of World frames.
struct CandidateSmokeState {
    deadline: Option<Instant>,
    evidence: CandidateSmokeEvidenceSink,
    phase: CandidateSmokePhase,
    sealing: bool,
}

enum CandidateSmokePhase {
    AwaitPresentedReady,
    Measuring {
        armed: Box<ArmedCandidate>,
        measurement: CandidateMeasurement,
    },
    Complete,
    Failed,
}

impl CandidateSmokeState {
    fn observe(
        &mut self,
        observation: &DesktopPresentationObservation<'_>,
    ) -> Result<DesktopPresentationControl, &'static str> {
        match self.phase {
            CandidateSmokePhase::Complete => return Ok(DesktopPresentationControl::Continue),
            CandidateSmokePhase::Failed => {
                return Ok(DesktopPresentationControl::Exit(AppExit::Requested));
            }
            _ => {}
        }
        let deadline = *self.deadline.get_or_insert_with(|| {
            observation
                .now()
                .checked_add(CANDIDATE_SMOKE_DEADLINE)
                .unwrap_or(observation.now())
        });
        remaining_candidate_time(deadline, observation.now())?;
        if observation.frame_completion_stats().lost_samples() != 0 {
            return Err("desktop candidate lost a GPU completion during product measurement");
        }

        let newly_armed = self.register_product_presentation(observation)?;
        self.collect_completion_samples(observation, newly_armed)?;

        if let Some(report) = observation.telemetry_seal() {
            if !self.sealing {
                return Err("desktop candidate exited before its bounded measurement sealed");
            }
            let CandidateSmokePhase::Measuring { armed, measurement } = &self.phase else {
                return Err("desktop candidate telemetry sealed without an armed measurement");
            };
            let (presented_completion, samples) = measurement
                .seal_input()
                .ok_or("desktop candidate telemetry sealed before its measurement completed")?;
            let evidence = CandidateSmokeEvidence::capture(
                report,
                armed.as_ref().clone(),
                presented_completion,
                samples,
            )?;
            *self
                .evidence
                .lock()
                .map_err(|_| "desktop candidate UI observation sink was poisoned")? =
                Some(evidence);
            self.phase = CandidateSmokePhase::Complete;
            return Ok(DesktopPresentationControl::Exit(AppExit::Success));
        }

        if !self.sealing
            && matches!(&self.phase, CandidateSmokePhase::Measuring { measurement, .. }
                if measurement.seal_input().is_some())
        {
            self.sealing = true;
            // The same original deadline covers submission stop, asynchronous polling and the
            // final report. No Last system submits another frame or blocks on a GPU wait.
            return Ok(DesktopPresentationControl::SealTelemetryAndExit {
                exit: AppExit::Success,
                deadline,
            });
        }
        Ok(DesktopPresentationControl::Continue)
    }

    fn register_product_presentation(
        &mut self,
        observation: &DesktopPresentationObservation<'_>,
    ) -> Result<bool, &'static str> {
        if self.sealing || observation.presented_frame().is_none() {
            return Ok(false);
        }
        let Some(world) = observation.world() else {
            return Ok(false);
        };
        let Some(presented) = latest_presented_ui_observation(world) else {
            return Ok(false);
        };
        let Some(candidate) = ArmedCandidate::capture(world, presented, observation)? else {
            return Ok(false);
        };
        match &mut self.phase {
            CandidateSmokePhase::AwaitPresentedReady => {
                let measurement = CandidateMeasurement::arm(candidate.identity);
                self.phase = CandidateSmokePhase::Measuring {
                    armed: Box::new(candidate),
                    measurement,
                };
                Ok(true)
            }
            CandidateSmokePhase::Measuring { measurement, .. } => {
                measurement.admit_product_frame(candidate.identity)?;
                Ok(false)
            }
            CandidateSmokePhase::Complete | CandidateSmokePhase::Failed => Ok(false),
        }
    }

    fn collect_completion_samples(
        &mut self,
        observation: &DesktopPresentationObservation<'_>,
        newly_armed: bool,
    ) -> Result<(), &'static str> {
        let CandidateSmokePhase::Measuring { measurement, .. } = &mut self.phase else {
            return Ok(());
        };
        let identity = measurement.identity();
        if observation.wgpu().instance_id() != identity.backend_instance_id()
            || observation.wgpu().device_epoch() != identity.device_epoch()
            || observation.frame_completion_stats().device_epoch() != identity.device_epoch()
            || observation.gpu_resource_stats().device_epoch() != identity.device_epoch()
        {
            return Err("desktop candidate WGPU generation changed after measurement armed");
        }
        let samples = observation.completion_samples().iter().map(|sample| {
            CandidateCompletionSample::new(
                sample.device_epoch(),
                sample.app_frame_index(),
                sample.render_transaction_frame_index(),
                sample.duration_ns(),
            )
        });
        if newly_armed {
            return measurement.observe_arm_boundary(samples);
        }
        // Sealing callbacks still validate the final tail for duplicates and foreign identities.
        // The seal report retains these same samples; replaying that aggregate would double count.
        for sample in samples {
            measurement.observe(sample)?;
        }
        Ok(())
    }
}

fn remaining_candidate_time(deadline: Instant, now: Instant) -> Result<Duration, &'static str> {
    deadline
        .checked_duration_since(now)
        .filter(|remaining| !remaining.is_zero())
        .ok_or(CANDIDATE_DEADLINE_EXCEEDED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_seal_uses_only_the_remaining_candidate_budget() {
        let now = Instant::now();

        assert_eq!(
            remaining_candidate_time(now + Duration::from_millis(7), now).unwrap(),
            Duration::from_millis(7)
        );
        assert_eq!(
            remaining_candidate_time(now, now),
            Err(CANDIDATE_DEADLINE_EXCEEDED)
        );
        assert_eq!(
            remaining_candidate_time(now, now + Duration::from_nanos(1)),
            Err(CANDIDATE_DEADLINE_EXCEEDED)
        );
    }
}
