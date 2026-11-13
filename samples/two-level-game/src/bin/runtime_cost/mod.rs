use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use nara::{
    app::{FixedTime, FrameExecutionStart},
    project_host::{
        DesktopPresentationControl, DesktopPresentationObservation, DesktopTelemetrySealOutcome,
    },
    render_wgpu::WgpuBackendState,
    window::backend::BackendWindowHandles,
};
use serde::Serialize;
use serde_json::{Value, json};
use two_level_game::{GamePhase, GameSnapshot, development::presentation::join_presented};

use super::{
    AppExit, TIMEOUT,
    process_cost::{self, ProcessCost},
};

#[derive(Serialize)]
struct Frame {
    runtime: u64,
    app_frame: u64,
    render_frame: u64,
    window: u64,
    surface_generation: u64,
    backend_instance: u64,
    device_epoch: u64,
    simulation_tick: u64,
    logical_tick: u64,
    frame_wall_ns: u64,
}

type CompletionKey = (u64, u64, u64);

pub(super) struct Collector {
    witness: bool,
    warmup: usize,
    wanted: usize,
    accepted: usize,
    last_presented: Option<(u64, u64, u64)>,
    pub(super) baseline: Option<GameSnapshot>,
    pub(super) collect_running: bool,
    frames: Vec<Frame>,
    completions: BTreeMap<CompletionKey, u64>,
    adapter: Option<Value>,
    first_start: Option<Instant>,
    last_end: Option<Instant>,
    pub(super) target: Option<BackendWindowHandles>,
    pub(super) failure: Option<String>,
    pub(super) seal_requested: bool,
    pub(super) seal_outcome: Option<DesktopTelemetrySealOutcome>,
    witnessed_frames: usize,
    process_begin: Option<(Instant, ProcessCost)>,
    process_end: Option<(Instant, ProcessCost)>,
}

impl Collector {
    pub(super) fn new(witness: bool, warmup: usize, wanted: usize) -> Self {
        Self {
            witness,
            warmup,
            wanted,
            accepted: 0,
            last_presented: None,
            baseline: None,
            collect_running: false,
            frames: Vec::with_capacity(wanted),
            completions: BTreeMap::new(),
            adapter: None,
            first_start: None,
            last_end: None,
            target: None,
            failure: None,
            seal_requested: false,
            seal_outcome: None,
            witnessed_frames: 0,
            process_begin: None,
            process_end: None,
        }
    }

    pub(super) fn sample_count(&self) -> usize {
        self.frames.len()
    }

    pub(super) fn observe(
        &mut self,
        observation: DesktopPresentationObservation<'_>,
    ) -> DesktopPresentationControl {
        let prior_frames = self.frames.len();
        let result = self.record(&observation);
        let result = if self.witness {
            result.and_then(|()| {
                let witness = join_presented(observation)?;
                if self.frames.len() > prior_frames {
                    let witness =
                        witness.ok_or("measured frame has no matching canonical Seal witness")?;
                    let frame = self.frames.last().ok_or("measured frame disappeared")?;
                    if witness.runtime_generation != frame.runtime
                        || witness.app_frame != frame.app_frame
                        || witness.render_frame != frame.render_frame
                        || witness.window != frame.window
                        || witness.surface_generation != frame.surface_generation
                        || witness.backend_instance != frame.backend_instance
                        || witness.device_epoch != frame.device_epoch
                    {
                        return Err(
                            "measured frame disagrees with its original canonical witness".into(),
                        );
                    }
                    self.witnessed_frames += 1;
                }
                Ok(())
            })
        } else {
            result
        };
        if let Err(error) = result {
            self.failure.get_or_insert(error);
        }
        if self.failure.is_some() {
            DesktopPresentationControl::Exit(AppExit::Requested)
        } else if self.seal_requested && self.seal_outcome.is_none() {
            DesktopPresentationControl::SealTelemetryAndExit {
                exit: AppExit::Success,
                deadline: Instant::now() + TIMEOUT,
            }
        } else {
            DesktopPresentationControl::Continue
        }
    }

    fn record(&mut self, observation: &DesktopPresentationObservation<'_>) -> Result<(), String> {
        if let Some(seal) = observation.telemetry_seal() {
            self.seal_outcome = Some(seal.outcome());
            let completion = seal.frame_completion_stats();
            let resources = seal.gpu_resource_stats();
            if completion.lost_samples() != 0
                || completion.pending_samples() != 0
                || resources.lost_retirements() != 0
                || resources.pending_retirements() != 0
            {
                return Err(
                    "telemetry seal lost or retained pending completion/resource evidence".into(),
                );
            }
        }
        for sample in observation.completion_samples() {
            let key = (
                sample.app_frame_index(),
                sample.render_transaction_frame_index(),
                sample.device_epoch(),
            );
            if self.completions.len() >= 4_096 {
                return Err("finite completion sample allowance exceeded".into());
            }
            if self.completions.insert(key, sample.duration_ns()).is_some() {
                return Err("original completion sample was delivered twice".into());
            }
        }
        let Some(world) = observation.world() else {
            return Ok(());
        };
        if self.target.is_none() {
            self.target = world.get_resource::<BackendWindowHandles>().cloned();
        }
        let snapshot = world
            .get_resource::<GameSnapshot>()
            .ok_or("game snapshot is unavailable")?;
        if let Some(baseline) = &self.baseline {
            validate_workload(snapshot, baseline)?;
        }
        if !self.collect_running || self.frames.len() == self.wanted {
            return Ok(());
        }
        let Some(receipt) = observation.presented_frame() else {
            return Ok(());
        };
        let key = (
            receipt.runtime_generation().get(),
            receipt.app_frame_index(),
            receipt.render_transaction_frame_index(),
        );
        if self.last_presented == Some(key) {
            return Ok(());
        }
        if self.last_presented.is_some_and(|previous| {
            key.0 != previous.0 || key.1 <= previous.1 || key.2 <= previous.2
        }) {
            return Err("original successful presentation identities regressed".into());
        }
        let start = *world
            .get_resource::<FrameExecutionStart>()
            .ok_or("original frame start is unavailable")?;
        let backend = observation.wgpu();
        let transaction = backend.frame_transaction_stats();
        if start.frame() != receipt.app_frame_index()
            || observation.runtime_generation() != receipt.runtime_generation()
            || backend.state() != WgpuBackendState::Ready
            || backend.instance_id() != receipt.backend_instance_id()
            || backend.device_epoch() != receipt.device_epoch()
            || transaction.app_frame_index() != Some(start.frame())
            || transaction.render_transaction_frame_index()
                != Some(receipt.render_transaction_frame_index())
            || transaction.queue_submissions() == 0
            || transaction.presents() == 0
        {
            return Err(
                "post-Host receipt is not the exact current original App/render/device transaction"
                    .into(),
            );
        }
        let extent = backend
            .configured_surface_extent(receipt.window_id())
            .ok_or("presented extent is unavailable")?;
        let adapter = observation
            .adapter_summary()
            .ok_or("original adapter summary is unavailable")?;
        let adapter = json!({ "name": adapter.name(), "vendor": adapter.vendor(), "device": adapter.device(),
            "device_type": adapter.device_type(), "backend": adapter.backend(), "driver": adapter.driver(),
            "driver_info": adapter.driver_info(), "surface_extent": [extent.width, extent.height] });
        if self
            .adapter
            .as_ref()
            .is_some_and(|previous| previous != &adapter)
        {
            return Err("adapter or presented extent changed during the no-input workload".into());
        }
        self.adapter.get_or_insert(adapter);
        if self.frames.first().is_some_and(|first| {
            first.runtime != key.0
                || first.window != receipt.window_id().get()
                || first.surface_generation != receipt.surface_generation()
                || first.backend_instance != receipt.backend_instance_id()
                || first.device_epoch != receipt.device_epoch()
        }) {
            return Err("native target or device changed within the measured run".into());
        }
        self.last_presented = Some(key);
        self.accepted += 1;
        if self.accepted <= self.warmup {
            return Ok(());
        }
        if self.frames.is_empty() {
            self.process_begin = Some((Instant::now(), process_cost::capture()?));
        }
        if self.frames.len() + 1 == self.wanted {
            self.process_end = Some((Instant::now(), process_cost::capture()?));
        }
        let tick = world.resource::<FixedTime>().tick();
        self.first_start.get_or_insert(start.started_at());
        self.last_end = Some(observation.now());
        self.frames.push(Frame {
            runtime: key.0,
            app_frame: key.1,
            render_frame: key.2,
            window: receipt.window_id().get(),
            surface_generation: receipt.surface_generation(),
            backend_instance: receipt.backend_instance_id(),
            device_epoch: receipt.device_epoch(),
            simulation_tick: tick,
            logical_tick: snapshot.logical_tick,
            frame_wall_ns: ns(observation
                .now()
                .checked_duration_since(start.started_at())
                .ok_or("frame timestamp regressed")?)?,
        });
        Ok(())
    }

    pub(super) fn report(&self) -> Result<Value, String> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let first = self
            .frames
            .first()
            .ok_or("no measured successful presentation")?;
        let last = self
            .frames
            .last()
            .ok_or("no final measured successful presentation")?;
        if self.frames.len() != self.wanted {
            return Err("finite continuous presentation workload did not complete".into());
        }
        if self.witness && self.witnessed_frames != self.wanted {
            return Err("not every measured frame retained its exact canonical witness".into());
        }
        let baseline = self.baseline.as_ref().ok_or("no initial workload")?;
        let spans = self
            .frames
            .iter()
            .map(|frame| frame.frame_wall_ns)
            .collect::<Vec<_>>();
        let (cpu_started, before) = self
            .process_begin
            .ok_or("missing initial process counters")?;
        let (cpu_finished, after) = self.process_end.ok_or("missing final process counters")?;
        let user_ns = after
            .user_ns
            .checked_sub(before.user_ns)
            .ok_or("process user CPU counter regressed")?;
        let kernel_ns = after
            .kernel_ns
            .checked_sub(before.kernel_ns)
            .ok_or("process kernel CPU counter regressed")?;
        Ok(
            json!({ "recipe": if self.witness { "witness" } else { "plain" },
                "adapter": self.adapter, "warmup_accepted_frames": self.warmup,
                "sample_accepted_frames": self.frames.len(), "frames": self.frames,
                "canonical_witnessed_frames": self.witnessed_frames,
                "process_cost": {
                    "scope": "whole-process CPU between first and last measured post-Host callbacks; includes observer/poll/background/driver CPU, excludes startup/warmup; not GPU execution",
                    "observed_frame_intervals": self.frames.len().saturating_sub(1),
                    "wall_ns": ns(cpu_finished.checked_duration_since(cpu_started).ok_or("process observation time regressed")?)?,
                    "user_ns": user_ns,
                    "kernel_ns": kernel_ns,
                    "total_cpu_ns": user_ns.checked_add(kernel_ns).ok_or("process CPU total overflowed")?,
                    "memory_scope": "whole-process point samples, not per-UI retained bytes or leak proof; sequential runs share loaded code and driver allocations",
                    "initial_working_set_bytes": before.working_set_bytes,
                    "final_working_set_bytes": after.working_set_bytes,
                    "initial_private_bytes": before.private_bytes,
                    "final_private_bytes": after.private_bytes,
                },
                "frame_wall_ns": spans, "frame_wall_distribution_ns": distribution(&spans)?,
                "measured_wall_ns": ns(self.last_end.ok_or("missing measurement end")?.duration_since(self.first_start.ok_or("missing measurement start")?))?,
                "simulation_tick_delta": last.simulation_tick - first.simulation_tick,
                "logical_tick_delta": last.logical_tick - first.logical_tick,
                "workload": { "level": baseline.level, "player": baseline.player.to_array(),
                    "score": baseline.score, "collected": baseline.collected, "visible_pickups": baseline.visible_pickups,
                    "markers": baseline.markers, "scene_members": baseline.scene_members, "held": baseline.held,
                    "game_phase": "running", "external_input_or_state_mutation": false },
                "semantics_unchanged": true,
            }),
        )
    }

    pub(super) fn validate_completions(&self) -> Result<(), String> {
        for frame in &self.frames {
            if !self.completions.contains_key(&(
                frame.app_frame,
                frame.render_frame,
                frame.device_epoch,
            )) {
                return Err(format!(
                    "measured App/render frame {}/{} has no original GPU completion",
                    frame.app_frame, frame.render_frame
                ));
            }
        }
        Ok(())
    }

    pub(super) fn add_completions(&self, report: &mut Value) -> Result<(), String> {
        self.validate_completions()?;
        let spans = self
            .frames
            .iter()
            .map(|frame| {
                self.completions[&(frame.app_frame, frame.render_frame, frame.device_epoch)]
            })
            .collect::<Vec<_>>();
        report["frame_start_to_gpu_completion_ns"] = json!(spans);
        report["frame_start_to_gpu_completion_distribution_ns"] = distribution(&spans)?;
        Ok(())
    }
}

fn validate_workload(snapshot: &GameSnapshot, baseline: &GameSnapshot) -> Result<(), String> {
    if !snapshot.initialized
        || snapshot.restoring
        || snapshot.error.is_some()
        || snapshot.phase != GamePhase::Running
        || snapshot.level != baseline.level
        || snapshot.player != baseline.player
        || snapshot.held != [false; 4]
        || snapshot.score != baseline.score
        || snapshot.collected != baseline.collected
        || snapshot.visible_pickups != baseline.visible_pickups
        || snapshot.markers != baseline.markers
        || snapshot.travel_count != baseline.travel_count
        || snapshot.restarts != baseline.restarts
        || snapshot.saves != baseline.saves
        || snapshot.restores != baseline.restores
        || snapshot.current_instance != baseline.current_instance
        || snapshot.scene_members != baseline.scene_members
    {
        return Err(
            "external input or unexpected semantic mutation invalidated the no-input workload"
                .into(),
        );
    }
    Ok(())
}

pub(super) fn ns(duration: Duration) -> Result<u64, String> {
    u64::try_from(duration.as_nanos())
        .map_err(|_| "duration exceeded finite nanosecond representation".into())
}

fn distribution(values: &[u64]) -> Result<Value, String> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    let percentile = |percent: usize| sorted[(count * percent).div_ceil(100).saturating_sub(1)];
    if count == 0 {
        return Err("cannot summarize empty measurements".into());
    }
    Ok(
        json!({ "count": count, "min": sorted[0], "p50": percentile(50), "p95": percentile(95),
        "p99": percentile(99), "max": sorted[count - 1], "percentile_method": "nearest-rank" }),
    )
}
