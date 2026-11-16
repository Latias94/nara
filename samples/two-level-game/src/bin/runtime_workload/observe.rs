//! Read-only measurements of the ordinary, continuously running Travel game.
//!
//! Acceptance is fixed before measurement: at least 20 unique initialized presentations,
//! at most 4,096 original presentations and completion samples, at least three observed
//! Scene-instance changes, and the verified Travel end state. The caller owns the 60-second
//! run deadline, sequential Runtime/native lifetimes, and physical native retirement.
//! Quantiles describe correlated observations from one run; they are not production tail
//! guarantees, optimization comparisons, GPU execution times, or scanout measurements.

use std::{
    collections::{BTreeMap, BTreeSet},
    panic::{AssertUnwindSafe, catch_unwind},
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::CurrentScene,
    app::{AppExit, FixedTime, FrameExecutionStart, RuntimeGeneration},
    project_host::{
        DesktopPresentationControl, DesktopPresentationObservation, DesktopTelemetrySealOutcome,
    },
    render_wgpu::{WgpuBackendState, WgpuFrameCompletionStats, WgpuGpuResourceStats},
    window::{
        WindowId,
        backend::{BackendWindowHandles, WindowTargetPhase},
    },
};
use serde::Serialize;
use serde_json::{Value, json};
use two_level_game::{
    GamePhase, GameSnapshot,
    levels::{LevelId, LevelKey},
};

use super::process_cost::{self, ProcessCost};
use super::timing::{NAMESPACE_PHASE_NAMES, RUNTIME_PHASE_NAMES, RuntimeFrameTiming};

pub(super) const MAX_SAMPLES: usize = 4_096;
const MIN_INITIALIZED_PRESENTATIONS: usize = 20;
const MIN_SCENE_INSTANCE_CHANGES: usize = 3;
const SEAL_TIMEOUT: Duration = Duration::from_secs(5);

type CompletionKey = (u64, u64, u64, u64, u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(super) struct PresentationIdentity {
    pub(super) runtime_generation: u64,
    pub(super) app_frame: u64,
    pub(super) render_frame: u64,
    pub(super) window: u64,
    pub(super) surface_generation: u64,
    pub(super) backend_instance: u64,
    pub(super) device_epoch: u64,
}

impl PresentationIdentity {
    fn completion_key(self) -> CompletionKey {
        (
            self.runtime_generation,
            self.backend_instance,
            self.app_frame,
            self.render_frame,
            self.device_epoch,
        )
    }
}

/// Deduplicates complete original identities before applying the finite presentation allowance.
pub(super) fn retain_presentation(
    identities: &mut BTreeSet<PresentationIdentity>,
    identity: PresentationIdentity,
) -> Result<bool, String> {
    if identity.runtime_generation == 0
        || identity.window == 0
        || identity.surface_generation == 0
        || identity.backend_instance == 0
        || identity.device_epoch == 0
    {
        return Err("successful presentation contains an uninitialized owner identity".into());
    }
    if identities.contains(&identity) {
        return Ok(false);
    }
    if identities.len() >= MAX_SAMPLES {
        return Err("finite original presentation allowance exceeded (4096)".into());
    }
    identities.insert(identity);
    Ok(true)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FrameClass {
    Startup,
    InitializedBaseline,
    MovingGameplay,
    SceneTransition,
    StableGameplay,
}

/// Classifies adjacent actual presented snapshots, without assigning internal engine stages.
pub(super) fn classify(
    previous: Option<&GameSnapshot>,
    current: &GameSnapshot,
) -> Result<FrameClass, String> {
    validate_snapshot(current)?;
    if !current.initialized {
        return Ok(FrameClass::Startup);
    }
    if current.phase != GamePhase::Running || current.restoring {
        return Ok(FrameClass::SceneTransition);
    }
    let Some(previous) = previous.filter(|snapshot| snapshot.initialized) else {
        return Ok(FrameClass::InitializedBaseline);
    };
    validate_snapshot(previous)?;
    if previous.phase != GamePhase::Running
        || previous.restoring
        || previous.level != current.level
        || previous.current_instance != current.current_instance
        || previous.travel_count != current.travel_count
        || previous.restarts != current.restarts
    {
        Ok(FrameClass::SceneTransition)
    } else if previous.player != current.player {
        Ok(FrameClass::MovingGameplay)
    } else {
        Ok(FrameClass::StableGameplay)
    }
}

fn validate_snapshot(snapshot: &GameSnapshot) -> Result<(), String> {
    if let Some(error) = &snapshot.error {
        return Err(format!("ordinary Travel gameplay failed: {error}"));
    }
    if !snapshot.player.is_finite() {
        return Err("presented gameplay position is not finite".into());
    }
    if snapshot.initialized && (snapshot.level.is_none() || snapshot.current_instance.is_none()) {
        return Err("initialized gameplay has no current level or Scene instance".into());
    }
    if snapshot.phase == GamePhase::Degraded {
        return Err("presented ordinary Travel gameplay is degraded".into());
    }
    Ok(())
}

/// Checks the actual script's final facts; Retry increments restarts, not travel_count.
pub(super) fn travel_completed(snapshot: &GameSnapshot) -> Result<bool, String> {
    validate_snapshot(snapshot)?;
    if !snapshot.verified {
        return Ok(false);
    }
    if !snapshot.initialized
        || snapshot.phase != GamePhase::Running
        || snapshot.restoring
        || snapshot.level != Some(LevelKey::main(LevelId::Meadow))
        || snapshot.player.to_array() != [-120.0, 0.0]
        || snapshot.score != 30
        || snapshot.collected != 3
        || snapshot.visible_pickups != 1
        || !snapshot.markers.iter().map(|id| id.get()).eq([1])
        || snapshot.travel_count != 4
        || snapshot.restarts != 1
        || snapshot.saves != 0
        || snapshot.restores != 0
        || snapshot.scene_members != 6
        || snapshot.held != [false; 4]
    {
        return Err("verified presentation disagrees with the final ordinary Travel facts".into());
    }
    Ok(true)
}

#[derive(Serialize)]
struct Frame {
    #[serde(flatten)]
    identity: PresentationIdentity,
    initialized: bool,
    classification: FrameClass,
    post_host_since_first_observation_ns: u64,
    app_to_post_host_wall_ns: Option<u64>,
    runtime_wall: Option<RuntimeFrameTiming>,
    issued_native_work: Value,
    surface_extent: [u32; 2],
    gameplay: Value,
}

#[derive(Clone, Copy)]
struct ProcessPoint {
    post_host_at: Instant,
    capture_started_at: Instant,
    capture_finished_at: Instant,
    cost: ProcessCost,
}

impl ProcessPoint {
    fn capture(post_host_at: Instant) -> Result<Self, String> {
        let capture_started_at = Instant::now();
        let cost = process_cost::capture()?;
        Ok(Self {
            post_host_at,
            capture_started_at,
            capture_finished_at: Instant::now(),
            cost,
        })
    }

    fn boundary(self) -> Result<Value, String> {
        Ok(json!({
            "post_host_to_counter_read_start_ns": elapsed_ns(self.post_host_at, self.capture_started_at)?,
            "counter_read_wall_ns": elapsed_ns(self.capture_started_at, self.capture_finished_at)?,
        }))
    }
}

pub(super) struct Collector {
    entry_started_at: Instant,
    run_index: usize,
    observer_callbacks: u64,
    identities: BTreeSet<PresentationIdentity>,
    frames: Vec<Frame>,
    completions: BTreeMap<CompletionKey, u64>,
    previous: Option<GameSnapshot>,
    visited: BTreeSet<LevelKey>,
    scene_instance_changes: usize,
    first_observation: Option<Instant>,
    first_presentation_at: Option<Instant>,
    last_observation: Option<Instant>,
    first_measured_start: Option<Instant>,
    last_measured_post_host: Option<Instant>,
    process_begin: Option<ProcessPoint>,
    process_end: Option<ProcessPoint>,
    adapter: Option<Value>,
    target: Option<BackendWindowHandles>,
    failure: Option<String>,
    completed: bool,
    seal_requested: bool,
    seal_deadline: Option<Instant>,
    seal_outcome: Option<DesktopTelemetrySealOutcome>,
    seal_report: Option<Value>,
}

impl Collector {
    pub(super) fn new(run_index: usize, entry_started_at: Instant) -> Self {
        Self {
            entry_started_at,
            run_index,
            observer_callbacks: 0,
            identities: BTreeSet::new(),
            frames: Vec::new(),
            completions: BTreeMap::new(),
            previous: None,
            visited: BTreeSet::new(),
            scene_instance_changes: 0,
            first_observation: None,
            first_presentation_at: None,
            last_observation: None,
            first_measured_start: None,
            last_measured_post_host: None,
            process_begin: None,
            process_end: None,
            adapter: None,
            target: None,
            failure: None,
            completed: false,
            seal_requested: false,
            seal_deadline: None,
            seal_outcome: None,
            seal_report: None,
        }
    }

    pub(super) fn error(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    pub(super) fn completed(&self) -> bool {
        self.completed && self.failure.is_none()
    }

    pub(super) fn request_seal(&mut self) {
        if !self.completed() {
            self.failure.get_or_insert_with(|| {
                "telemetry seal requested before a correct verified Travel presentation".into()
            });
        } else {
            self.seal_requested = true;
        }
    }

    pub(super) fn target(&self) -> Option<BackendWindowHandles> {
        self.target.clone()
    }

    pub(super) fn seal_complete(&self) -> bool {
        self.failure.is_none()
            && self.seal_requested
            && self.seal_outcome == Some(DesktopTelemetrySealOutcome::Complete)
    }

    /// Converts both returned errors and unwinding panics into an observer exit veto.
    pub(super) fn observe(
        &mut self,
        observation: DesktopPresentationObservation<'_>,
    ) -> DesktopPresentationControl {
        let result = catch_unwind(AssertUnwindSafe(|| -> Result<_, String> {
            self.record(&observation)?;
            if self.failure.is_some() {
                return Ok(DesktopPresentationControl::Exit(AppExit::Requested));
            }
            if self.seal_requested && self.seal_outcome.is_none() {
                let deadline = match self.seal_deadline {
                    Some(deadline) => deadline,
                    None => {
                        let deadline = observation
                            .now()
                            .checked_add(SEAL_TIMEOUT)
                            .ok_or("telemetry seal deadline overflowed")?;
                        self.seal_deadline = Some(deadline);
                        deadline
                    }
                };
                Ok(DesktopPresentationControl::SealTelemetryAndExit {
                    exit: AppExit::Success,
                    deadline,
                })
            } else {
                Ok(DesktopPresentationControl::Continue)
            }
        }));
        let error = match result {
            Ok(Ok(control)) => return control,
            Ok(Err(error)) => error,
            Err(payload) => {
                let detail = match payload.downcast::<String>() {
                    Ok(message) => *message,
                    Err(payload) => match payload.downcast::<&'static str>() {
                        Ok(message) => (*message).to_owned(),
                        Err(payload) => {
                            std::mem::forget(payload);
                            "non-string panic payload".to_owned()
                        }
                    },
                };
                format!("ordinary workload observer panicked: {detail}")
            }
        };
        self.failure.get_or_insert(error);
        DesktopPresentationControl::Exit(AppExit::Requested)
    }

    fn record(&mut self, observation: &DesktopPresentationObservation<'_>) -> Result<(), String> {
        self.observer_callbacks = self
            .observer_callbacks
            .checked_add(1)
            .ok_or("original observer callback counter overflowed")?;
        let now = observation.now();
        if self.last_observation.is_some_and(|previous| now < previous) {
            return Err("original post-Host observation timestamp regressed".into());
        }
        self.first_observation.get_or_insert(now);
        self.last_observation = Some(now);
        self.record_telemetry(observation)?;
        if let Some(world) = observation.world() {
            if self.target.is_none() {
                self.target = world.get_resource::<BackendWindowHandles>().cloned();
            }
            if let Some(snapshot) = world.get_resource::<GameSnapshot>() {
                validate_snapshot(snapshot)?;
            }
        }
        let Some(receipt) = observation.presented_frame() else {
            return Ok(());
        };
        let identity = PresentationIdentity {
            runtime_generation: receipt.runtime_generation().get(),
            app_frame: receipt.app_frame_index(),
            render_frame: receipt.render_transaction_frame_index(),
            window: receipt.window_id().get(),
            surface_generation: receipt.surface_generation(),
            backend_instance: receipt.backend_instance_id(),
            device_epoch: receipt.device_epoch(),
        };
        if !retain_presentation(&mut self.identities, identity)? {
            return Ok(());
        }
        if let Some(previous) = self.frames.last()
            && (identity.runtime_generation != previous.identity.runtime_generation
                || identity.window != previous.identity.window
                || identity.backend_instance != previous.identity.backend_instance
                || identity.device_epoch != previous.identity.device_epoch
                || identity.app_frame <= previous.identity.app_frame
                || identity.render_frame <= previous.identity.render_frame)
        {
            return Err(
                "ordinary original presentation owners changed or frame identities regressed"
                    .into(),
            );
        }
        let world = observation
            .world()
            .ok_or("presented original World is unavailable")?;
        let runtime = world
            .get_resource::<RuntimeGeneration>()
            .ok_or("presented original Runtime identity is unavailable")?;
        let backend = observation.wgpu();
        let transaction = backend.frame_transaction_stats();
        if receipt.runtime_generation() != observation.runtime_generation()
            || receipt.runtime_generation() != *runtime
            || receipt.window_id() != WindowId::PRIMARY
            || backend.state() != WgpuBackendState::Ready
            || backend.instance_id() != identity.backend_instance
            || backend.device_epoch() != identity.device_epoch
            || transaction.app_frame_index() != Some(identity.app_frame)
            || transaction.render_transaction_frame_index() != Some(identity.render_frame)
            || transaction.queue_submissions() == 0
            || transaction.presents() == 0
        {
            return Err("presentation is not the exact original Runtime/App/render/window/device transaction".into());
        }
        let handles = self
            .target
            .as_ref()
            .ok_or("original native window registry is unavailable")?;
        let target = handles
            .snapshot(receipt.window_id())
            .map_err(|error| error.to_string())?;
        if target.phase != WindowTargetPhase::Active
            || target.fault.is_some()
            || !target.surface_active
            || !target.provider_present
            || target.native_destroyed
            || target.surface_generation != Some(identity.surface_generation)
        {
            return Err(
                "successful receipt does not match its original live native surface".into(),
            );
        }
        let extent = backend
            .configured_surface_extent(receipt.window_id())
            .ok_or("presented surface extent is unavailable")?;
        let adapter = observation
            .adapter_summary()
            .ok_or("original adapter summary is unavailable")?;
        if adapter.backend() != "dx12"
            || !matches!(adapter.device_type(), "discrete-gpu" | "integrated-gpu")
        {
            return Err(
                "ordinary native workload requires an observed hardware DX12 adapter".into(),
            );
        }
        let adapter = json!({
            "name": adapter.name(), "vendor": adapter.vendor(), "device": adapter.device(),
            "device_type": adapter.device_type(), "backend": adapter.backend(),
            "driver": adapter.driver(), "driver_info": adapter.driver_info(),
        });
        if self
            .adapter
            .as_ref()
            .is_some_and(|previous| previous != &adapter)
        {
            return Err("original adapter summary changed during the Travel run".into());
        }
        self.adapter.get_or_insert(adapter);
        let snapshot = world
            .get_resource::<GameSnapshot>()
            .ok_or("presented GameSnapshot is unavailable")?;
        let classification = classify(self.previous.as_ref(), snapshot)?;
        let completed = travel_completed(snapshot)?;
        let start = world.get_resource::<FrameExecutionStart>().copied();
        let matching_start = start.filter(|start| start.frame() == identity.app_frame);
        let app_to_post_host_wall_ns = matching_start
            .map(|start| elapsed_ns(start.started_at(), now))
            .transpose()?;
        let runtime_wall = transaction
            .runtime_timings()
            .map(|phases| RuntimeFrameTiming::capture(phases, app_to_post_host_wall_ns))
            .transpose()?;
        if snapshot.initialized {
            if runtime_wall.is_none() {
                return Err(
                    "initialized presentation lacks its original Runtime wall observations".into(),
                );
            }
            let start = matching_start
                .ok_or("initialized presentation has no matching original App frame start")?;
            let fixed = world
                .get_resource::<FixedTime>()
                .ok_or("original fixed clock is unavailable")?;
            if snapshot.tick != fixed.tick() {
                return Err(
                    "presented gameplay snapshot disagrees with its original fixed clock".into(),
                );
            }
            let current = CurrentScene::get(world)
                .ok_or("initialized presentation has no original current Scene receipt")?;
            if snapshot.current_instance != Some(current.receipt().instance_id())
                || snapshot.scene_members != current.receipt().entity_ids().len()
            {
                return Err(
                    "presented gameplay snapshot disagrees with its original current Scene receipt"
                        .into(),
                );
            }
            let stats = observation.frame_completion_stats();
            let resources = observation.gpu_resource_stats();
            if !stats.enabled()
                || !resources.tracking_enabled()
                || stats.device_epoch() != identity.device_epoch
                || resources.device_epoch() != identity.device_epoch
            {
                return Err(
                    "original presentation telemetry was not enabled for the matching device epoch"
                        .into(),
                );
            }
            if self.process_begin.is_none() || completed {
                let point = ProcessPoint::capture(now)?;
                self.process_begin.get_or_insert(point);
                self.process_end = Some(point);
            }
            self.first_measured_start.get_or_insert(start.started_at());
            self.last_measured_post_host = Some(now);
            if let Some(level) = snapshot.level {
                self.visited.insert(level);
            }
            if self.previous.as_ref().is_some_and(|previous| {
                previous.initialized && previous.current_instance != snapshot.current_instance
            }) {
                self.scene_instance_changes += 1;
            }
        }
        let first = self
            .first_observation
            .ok_or("initial observer timestamp is unavailable")?;
        self.first_presentation_at.get_or_insert(now);
        self.frames.push(Frame {
            identity,
            initialized: snapshot.initialized,
            classification,
            post_host_since_first_observation_ns: elapsed_ns(first, now)?,
            app_to_post_host_wall_ns,
            runtime_wall,
            issued_native_work: super::timing::native_work(transaction.native_work()),
            surface_extent: [extent.width, extent.height],
            gameplay: snapshot_value(snapshot),
        });
        self.previous = Some(snapshot.clone());
        self.completed = completed;
        Ok(())
    }

    fn record_telemetry(
        &mut self,
        observation: &DesktopPresentationObservation<'_>,
    ) -> Result<(), String> {
        let completion = observation.frame_completion_stats();
        let resources = observation.gpu_resource_stats();
        if completion.lost_samples() != 0 || resources.lost_retirements() != 0 {
            return Err("original completion/resource telemetry lost evidence".into());
        }
        let runtime = observation.runtime_generation().get();
        let backend = observation.wgpu().instance_id();
        for sample in observation.completion_samples() {
            if backend == 0 || sample.device_epoch() == 0 {
                return Err("original completion has no backend/device owner".into());
            }
            let key = (
                runtime,
                backend,
                sample.app_frame_index(),
                sample.render_transaction_frame_index(),
                sample.device_epoch(),
            );
            if self.completions.contains_key(&key) {
                return Err("original completion sample was delivered twice".into());
            }
            if self.completions.len() >= MAX_SAMPLES {
                return Err("finite original completion allowance exceeded (4096)".into());
            }
            self.completions.insert(key, sample.duration_ns());
        }
        let Some(seal) = observation.telemetry_seal() else {
            return Ok(());
        };
        self.seal_outcome = Some(seal.outcome());
        let completion = seal.frame_completion_stats();
        let resources = seal.gpu_resource_stats();
        self.seal_report = Some(json!({
            "outcome": format!("{:?}", seal.outcome()),
            "runtime_generation": runtime,
            "backend_instance": seal.backend_instance_id(),
            "device_epoch": seal.device_epoch(),
            "completion": completion_value(completion),
            "resources": resource_value(resources),
            "seal_audit_samples": seal.completion_samples().len(),
            "scope": "original explicit pre-retirement seal; audit samples overlap incremental batches and are not counted twice",
        }));
        if !self.seal_requested
            || seal.outcome() != DesktopTelemetrySealOutcome::Complete
            || seal.backend_instance_id() != backend
            || seal.device_epoch() == 0
            || completion.device_epoch() != seal.device_epoch()
            || resources.device_epoch() != seal.device_epoch()
            || !completion.enabled()
            || !resources.tracking_enabled()
            || completion.lost_samples() != 0
            || completion.pending_samples() != 0
            || completion.buffered_samples() != 0
            || resources.lost_retirements() != 0
            || resources.pending_retirements() != 0
            || completion.completed_samples() != self.completions.len() as u64
        {
            return Err("original explicit telemetry seal is incomplete, changed owners, or lost completion/resource evidence".into());
        }
        for sample in seal.completion_samples() {
            let key = (
                runtime,
                backend,
                sample.app_frame_index(),
                sample.render_transaction_frame_index(),
                sample.device_epoch(),
            );
            if self.completions.get(&key) != Some(&sample.duration_ns()) {
                return Err(
                    "original seal audit sample disagrees with its exact incremental completion"
                        .into(),
                );
            }
        }
        Ok(())
    }

    /// Summarizes retained evidence after the caller completes original native retirement.
    pub(super) fn finish(&self) -> Result<Value, String> {
        if let Some(error) = self.error() {
            return Err(error.to_owned());
        }
        if !self.completed() || !self.seal_complete() {
            return Err(
                "ordinary Travel and its explicitly requested telemetry seal did not complete"
                    .into(),
            );
        }
        let registry = self
            .target
            .as_ref()
            .ok_or("missing original native registry")?;
        let target = registry
            .snapshot(WindowId::PRIMARY)
            .map_err(|error| error.to_string())?;
        if target.phase != WindowTargetPhase::NativeDestroyed
            || target.fault.is_some()
            || target.surface_active
            || target.provider_present
            || !target.native_destroyed
        {
            return Err(
                "finish requires the original native target to be physically retired".into(),
            );
        }
        let initialized = self.frames.iter().filter(|frame| frame.initialized).count();
        let moving = self
            .frames
            .iter()
            .filter(|frame| frame.classification == FrameClass::MovingGameplay)
            .count();
        if initialized < MIN_INITIALIZED_PRESENTATIONS
            || self.scene_instance_changes < MIN_SCENE_INSTANCE_CHANGES
            || moving == 0
            || !LevelId::ALL
                .into_iter()
                .all(|level| self.visited.iter().any(|key| key.level() == level))
        {
            return Err(format!(
                "insufficient actual Travel coverage: initialized={initialized}, moving={moving}, scene_instance_changes={}",
                self.scene_instance_changes
            ));
        }
        let final_snapshot = self
            .previous
            .as_ref()
            .ok_or("missing final presented snapshot")?;
        if !travel_completed(final_snapshot)? {
            return Err("final presented Travel snapshot is not verified".into());
        }
        let mut wall = Vec::with_capacity(initialized);
        let mut completion = Vec::with_capacity(initialized);
        let mut matched = BTreeMap::new();
        for frame in &self.frames {
            let key = frame.identity.completion_key();
            matched.insert(key, frame.identity);
            if frame.initialized {
                wall.push(
                    frame
                        .app_to_post_host_wall_ns
                        .ok_or("missing exact initialized App-to-post-Host duration")?,
                );
                completion.push(*self.completions.get(&key).ok_or_else(|| {
                    format!(
                        "initialized App/render/device frame {}/{}/{} has no exact original completion",
                        frame.identity.app_frame,
                        frame.identity.render_frame,
                        frame.identity.device_epoch,
                    )
                })?);
            }
        }
        let original_completions = self
            .completions
            .iter()
            .map(|(key, duration)| {
                json!({
                    "runtime_generation": key.0, "backend_instance": key.1,
                    "app_frame": key.2, "render_frame": key.3, "device_epoch": key.4,
                    "frame_start_to_completion_observation_ns": duration,
                    "matching_presentation": matched.get(key),
                })
            })
            .collect::<Vec<_>>();
        let begin = self
            .process_begin
            .ok_or("missing first measured process sample")?;
        let end = self
            .process_end
            .ok_or("missing last measured process sample")?;
        if Some(end.post_host_at) != self.last_measured_post_host {
            return Err("last process point does not match the last measured presentation".into());
        }
        let mut process = process_deltas(begin.cost, end.cost)?;
        process["wall_ns"] = json!(elapsed_ns(
            begin.capture_started_at,
            end.capture_started_at
        )?);
        process["post_host_interval_wall_ns"] =
            json!(elapsed_ns(begin.post_host_at, end.post_host_at)?);
        process["observed_frame_intervals"] = json!(initialized - 1);
        process["first_boundary"] = begin.boundary()?;
        process["last_boundary"] = end.boundary()?;
        let mut classes = Vec::new();
        for class in [
            FrameClass::InitializedBaseline,
            FrameClass::MovingGameplay,
            FrameClass::SceneTransition,
            FrameClass::StableGameplay,
        ] {
            let values = self
                .frames
                .iter()
                .filter(|frame| frame.classification == class)
                .filter_map(|frame| frame.app_to_post_host_wall_ns)
                .collect::<Vec<_>>();
            classes.push(json!({
                "classification": class, "count": values.len(),
                "app_to_post_host_wall_distribution_ns": if values.is_empty() { None } else { Some(distribution(&values)?) },
            }));
        }
        Ok(json!({
            "run_index": self.run_index,
            "scenario": "travel",
            "adapter": self.adapter.as_ref().ok_or("missing observed hardware adapter")?,
            "acceptance": {
                "minimum_unique_initialized_presentations": MIN_INITIALIZED_PRESENTATIONS,
                "maximum_unique_presentations_and_completion_samples": MAX_SAMPLES,
                "minimum_observed_scene_instance_changes": MIN_SCENE_INSTANCE_CHANGES,
                "maximum_run_seconds_enforced_by_caller": 60,
                "requires_observed_movement_and_both_levels": true,
                "requires_hardware_dx12": true,
                "timing_quantiles_are_descriptive": true,
            },
            "observer_callbacks": self.observer_callbacks,
            "unique_presentations": self.frames.len(),
            "initialized_presentations": initialized,
            "startup_presentations": self.frames.len() - initialized,
            "startup_scope": "uninitialized original successful presentations remain in the trace and are excluded from initialized distributions and process measurement boundaries; a missing matching App start is null",
            "observed_scene_instance_changes": self.scene_instance_changes,
            "observed_level_instances": self.visited,
            "classification_scope": "adjacent unique presented snapshots only; moving means changed player position within the same running Scene; transitions mean observed phase/restoring/level/Scene-instance/travel/restart changes; no internal stage duration is inferred",
            "classifications": classes,
            "observed_trace": self.frames,
            "entry_to_first_presentation_observation_wall_ns": elapsed_ns(self.entry_started_at, self.first_presentation_at.ok_or("missing first original presentation")?)?,
            "entry_scope": "from run_one entry before source root/recipe/collector construction to the original first post-Host observation; caller/native waits included; excludes process loading and later gameplay/cleanup",
            "first_presentation": self.frames.first(),
            "first_initialized_presentation": self.frames.iter().find(|frame| frame.initialized),
            "runtime_phase_order": RUNTIME_PHASE_NAMES,
            "namespace_phase_order": NAMESPACE_PHASE_NAMES,
            "namespace_phase_scope": "disjoint returned native calls inside the matching namespace preparation span; Adapter/Device request spans are absent on reuse, not measured zero; contains native/driver waits; sum and other_namespace_wall_ns equal only that parent phase and must not be added to it",
            "runtime_phase_scope": "disjoint original Runtime namespace preparation, Surface acquisition, candidate preparation, program execution and Surface presentation operations within the matching App frame; includes native/driver/error-scope waits; residual is other App work and unmeasured gaps, not CPU time or inferred wait time; observations never grant presentation authority",
            "final_snapshot": snapshot_value(final_snapshot),
            "gameplay_verified": true,
            "app_to_post_host_wall_ns": wall,
            "app_to_post_host_wall_distribution_ns": distribution(&wall)?,
            "app_to_post_host_wall_scope": "matching original FrameExecutionStart.started_at through original post-Host observation.now; App/native Host/submit wall including waits and poll work; not CPU execution, GPU execution, presentation cadence, or scanout",
            "measured_wall_ns": elapsed_ns(self.first_measured_start.ok_or("missing measurement start")?, self.last_measured_post_host.ok_or("missing measurement end")?)?,
            "original_completion_samples": original_completions,
            "matched_initialized_completion_samples": completion.len(),
            "unmatched_completion_samples": self.completions.keys().filter(|key| !matched.contains_key(*key)).count(),
            "frame_start_to_completion_observation_ns": completion,
            "frame_start_to_completion_observation_distribution_ns": distribution(&completion)?,
            "completion_scope": "original App frame start to original queue-completion callback observation, including CPU/queue/wait/poll delay; not GPU execution or scanout; original samples expose App/render/device identities, with Runtime/backend supplied by the original observer and window/surface only by a unique matching presentation",
            "quantile_scope": "nearest-rank empirical quantiles of correlated observations from this single run; no production P99, independent-sample, or performance improvement claim",
            "process_cost": process,
            "telemetry_seal": self.seal_report,
        }))
    }
}

fn snapshot_value(snapshot: &GameSnapshot) -> Value {
    json!({
        "tick": snapshot.tick, "logical_tick": snapshot.logical_tick,
        "logical_elapsed_nanos": snapshot.logical_elapsed_nanos,
        "level": snapshot.level, "player": snapshot.player.to_array(),
        "score": snapshot.score, "collected": snapshot.collected,
        "visible_pickups": snapshot.visible_pickups, "markers": snapshot.markers,
        "phase": format!("{:?}", snapshot.phase),
        "current_instance": snapshot.current_instance.map(|id| id.get()),
        "travel_count": snapshot.travel_count, "restarts": snapshot.restarts,
        "saves": snapshot.saves, "restores": snapshot.restores,
        "initialized": snapshot.initialized, "verified": snapshot.verified,
        "scene_members": snapshot.scene_members, "held": snapshot.held,
        "restoring": snapshot.restoring, "error": snapshot.error,
    })
}

fn completion_value(stats: WgpuFrameCompletionStats) -> Value {
    json!({
        "enabled": stats.enabled(), "device_epoch": stats.device_epoch(),
        "pending_samples": stats.pending_samples(), "buffered_samples": stats.buffered_samples(),
        "completed_samples": stats.completed_samples(), "lost_samples": stats.lost_samples(),
    })
}

fn resource_value(stats: WgpuGpuResourceStats) -> Value {
    json!({
        "tracking_enabled": stats.tracking_enabled(), "device_epoch": stats.device_epoch(),
        "pending_retirements": stats.pending_retirements(), "lost_retirements": stats.lost_retirements(),
        "current_texture_bytes": stats.current_texture_bytes(),
        "current_instance_buffer_bytes": stats.current_instance_buffer_bytes(),
        "current_total_bytes": stats.current_total_bytes(),
        "peak_texture_bytes": stats.peak_texture_bytes(),
        "peak_instance_buffer_bytes": stats.peak_instance_buffer_bytes(),
        "peak_total_bytes": stats.peak_total_bytes(),
        "scope": "original backend logical tracked payloads in this device epoch; not complete GPU residency, driver allocation size, or leak evidence",
    })
}

pub(super) fn duration_ns(duration: Duration) -> Result<u64, String> {
    u64::try_from(duration.as_nanos())
        .map_err(|_| "duration exceeded finite nanosecond representation".into())
}

fn elapsed_ns(start: Instant, end: Instant) -> Result<u64, String> {
    duration_ns(
        end.checked_duration_since(start)
            .ok_or("measurement timestamp regressed")?,
    )
}

/// Empirical nearest-rank quantiles of a nonempty finite set of integral nanosecond readings.
pub(super) fn distribution(values: &[u64]) -> Result<Value, String> {
    if values.is_empty() || values.len() > MAX_SAMPLES {
        return Err("distribution requires 1..=4096 finite nanosecond readings".into());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    let percentile = |percent: usize| sorted[(count * percent).div_ceil(100) - 1];
    Ok(json!({
        "count": count, "min": sorted[0], "p50": percentile(50),
        "p95": percentile(95), "p99": percentile(99), "max": sorted[count - 1],
        "percentile_method": "nearest-rank",
    }))
}

/// Checked whole-process counter deltas and separate point memory readings.
pub(super) fn process_deltas(before: ProcessCost, after: ProcessCost) -> Result<Value, String> {
    let user_ns = after
        .user_ns
        .checked_sub(before.user_ns)
        .ok_or("process user CPU counter regressed")?;
    let kernel_ns = after
        .kernel_ns
        .checked_sub(before.kernel_ns)
        .ok_or("process kernel CPU counter regressed")?;
    let total_cpu_ns = user_ns
        .checked_add(kernel_ns)
        .ok_or("process CPU total overflowed")?;
    Ok(json!({
        "user_ns": user_ns, "kernel_ns": kernel_ns, "total_cpu_ns": total_cpu_ns,
        "scope": "whole-process CPU between counter reads at the first and last unique initialized post-Host presentations; includes observer, native polling, driver, and background threads; excludes earlier startup and later seal/native cleanup; not per-frame CPU or GPU execution",
        "initial_working_set_bytes": before.working_set_bytes,
        "final_working_set_bytes": after.working_set_bytes,
        "initial_private_bytes": before.private_bytes,
        "final_private_bytes": after.private_bytes,
        "memory_scope": "whole-process point samples only; no peak, retained-UI allocation, per-frame allocation, GPU residency, or leak claim; sequential runs share loaded code and driver caches",
        "counter_read_scope": "CPU and memory are read by separate native calls, not an atomic process snapshot; boundary read costs and post-Host lags are reported separately",
    }))
}
