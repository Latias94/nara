//! Bounded CPU evidence joining original stock captures to native presentation and completion.

use std::{
    any::Any,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::CurrentScene,
    app::{AppExit, FrameExecutionStart, RuntimeGeneration},
    asset_watch::AssetWatchRuntimeObserver,
    ecs::Resource,
    project_host::{
        DesktopPresentationControl, DesktopPresentationObservation, DesktopTelemetrySealOutcome,
        DesktopTelemetrySealReport,
    },
    render::FrameStats,
    render_wgpu::{
        WgpuAdapterSummary, WgpuBackendState, WgpuFrameCompletionSample, WgpuFrameCompletionStats,
        WgpuGpuResourceStats,
    },
    window::{
        WindowId,
        backend::{BackendWindowHandles, WindowTargetPhase},
    },
};

use super::{
    super::{Step as SourceStep, Workflow},
    FrameFacts,
};

const COMMITS: usize = 8;
const REJECTIONS: usize = 3;
const REPRESENTATIVES: usize = 1 + COMMITS + REJECTIONS;
const OVERALL_TIMEOUT: Duration = Duration::from_secs(150);
const SEAL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Default, Resource)]
pub(super) struct PresentationState(Arc<Mutex<ObservationState>>);

#[derive(Default)]
struct ObservationState {
    latest: Option<FrameFacts>,
    presented: Option<FrameFacts>,
    original: Option<OriginalPresentation>,
    representatives: Vec<Representative>,
    presentations: u64,
    deadline: Option<Instant>,
    seal_deadline: Option<Instant>,
    sealed: Option<SealedTelemetry>,
    failure: Option<String>,
    panic: Option<Box<dyn Any + Send>>,
}

struct OriginalPresentation {
    registry: BackendWindowHandles,
    watcher: AssetWatchRuntimeObserver,
    runtime: RuntimeGeneration,
    window: WindowId,
    surface: u64,
    backend: u64,
    epoch: u64,
    adapter: WgpuAdapterSummary,
}

struct Representative {
    facts: FrameFacts,
    epoch: u64,
    completion: Option<WgpuFrameCompletionSample>,
}

struct SealedTelemetry {
    completion: WgpuFrameCompletionStats,
    resources: WgpuGpuResourceStats,
}

impl PresentationState {
    pub(super) fn protect<T>(&self, operation: impl FnOnce() -> T) -> Option<T> {
        if self.failure().is_some() {
            return None;
        }
        match catch_unwind(AssertUnwindSafe(operation)) {
            Ok(value) => Some(value),
            Err(payload) => {
                let message = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("non-string fixture panic");
                let mut state = self.lock();
                state
                    .failure
                    .get_or_insert_with(|| format!("native fixture callback panicked: {message}"));
                if state.panic.is_none() {
                    state.panic = Some(payload);
                }
                None
            }
        }
    }

    pub(super) fn take_panic(&self) -> Option<Box<dyn Any + Send>> {
        self.lock().panic.take()
    }

    pub(super) fn record(&self, facts: FrameFacts) {
        self.lock().latest = Some(facts);
    }

    pub(super) fn update_is_allowed(&self, workflow: &Workflow) -> bool {
        let state = self.lock();
        if state.failure.is_some() || state.seal_deadline.is_some() {
            return false;
        }
        let Some(current) = &workflow.current else {
            return true;
        };
        state.presented.as_ref().is_some_and(|facts| {
            facts.commits == workflow.commits
                && facts.rejections == workflow.rejections
                && facts.instance == current.instance.instance_id()
                && facts.content == current.stamp
                && facts.material == current.material
                && current
                    .images
                    .get(&current.material.texture)
                    .is_some_and(|image| image.pixels.as_slice() == facts.palette.as_slice())
        })
    }

    pub(super) fn observe(
        &self,
        observation: DesktopPresentationObservation<'_>,
    ) -> DesktopPresentationControl {
        let mut state = self.lock();
        if state.failure.is_some() {
            return DesktopPresentationControl::Exit(AppExit::Requested);
        }
        match state.observe(&observation) {
            Ok(control) => control,
            Err(error) => {
                state.failure = Some(error);
                DesktopPresentationControl::Exit(AppExit::Requested)
            }
        }
    }

    pub(super) fn failure(&self) -> Option<String> {
        self.lock().failure.clone()
    }

    pub(super) fn verify_retired(&self) -> Result<(), String> {
        let state = self.lock();
        let original = state
            .original
            .as_ref()
            .ok_or("the original native presentation registry was never observed")?;
        let snapshot = original
            .registry
            .snapshot(original.window)
            .map_err(|error| format!("the original native target is unavailable: {error}"))?;
        if snapshot.phase != WindowTargetPhase::NativeDestroyed
            || !snapshot.native_destroyed
            || snapshot.provider_present
            || snapshot.surface_active
            || snapshot.surface_generation.is_some()
            || snapshot.fault.is_some()
        {
            return Err(format!(
                "the original native target {:?} did not physically retire: {snapshot:?}",
                original.window,
            ));
        }
        let watcher = original.watcher.stats();
        if watcher.started() == 0
            || watcher.started() != watcher.joined()
            || watcher.active() != 0
            || watcher.retiring() != 0
        {
            return Err(format!(
                "the original native watchers did not retire: {watcher:?}"
            ));
        }
        Ok(())
    }

    pub(super) fn assert_success(&self) {
        let retirement = self.verify_retired();
        let state = self.lock();
        assert_eq!(state.failure, None, "original native presentation failed");
        retirement.expect("the original native registry must prove physical retirement");
        state
            .verify_representatives()
            .expect("every represented state must have its exact completion");
        assert!(state.final_presented(), "Done must actually present");
        assert!(
            state.seal_deadline.is_some(),
            "the consumer must request a seal",
        );
        let original = state
            .original
            .as_ref()
            .expect("the original native identity");
        let sealed = state
            .sealed
            .as_ref()
            .expect("the original Complete telemetry seal");
        println!(
            "native source adapter: name={:?} backend={} type={} vendor={:#x} device={:#x}",
            original.adapter.name(),
            original.adapter.backend(),
            original.adapter.device_type(),
            original.adapter.vendor(),
            original.adapter.device(),
        );
        println!(
            "native source presentation: runtime={} window={} surface={} host={} epoch={} \
             frames={} commits={COMMITS} rejections={REJECTIONS} representatives={} \
             seal=Complete completed={} pending={} buffered={} lost={} \
             retirements={} lost_retirements={} native_destroyed=true watchers_joined={} \
             sprites=1 observer=cpu-only",
            original.runtime.get(),
            original.window.get(),
            original.surface,
            original.backend,
            original.epoch,
            state.presentations,
            state.representatives.len(),
            sealed.completion.completed_samples(),
            sealed.completion.pending_samples(),
            sealed.completion.buffered_samples(),
            sealed.completion.lost_samples(),
            sealed.resources.pending_retirements(),
            sealed.resources.lost_retirements(),
            original.watcher.stats().joined(),
        );
        for representative in &state.representatives {
            let facts = &representative.facts;
            let sample = representative
                .completion
                .expect("the verified exact completion");
            println!(
                "native source representative: commits={} rejections={} step={:?} \
                 app={} render={} epoch={} scene={:?} app_start_to_completion_ns={}",
                facts.commits,
                facts.rejections,
                facts.step,
                facts.app,
                facts.render,
                representative.epoch,
                facts.instance,
                sample.duration_ns(),
            );
        }
    }

    fn lock(&self) -> MutexGuard<'_, ObservationState> {
        match self.0.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state
                    .failure
                    .get_or_insert_with(|| "the presentation state mutex was poisoned".to_owned());
                state
            }
        }
    }
}

impl ObservationState {
    fn observe(
        &mut self,
        observation: &DesktopPresentationObservation<'_>,
    ) -> Result<DesktopPresentationControl, String> {
        let now = observation.now();
        let deadline = *self
            .deadline
            .get_or_insert_with(|| now.checked_add(OVERALL_TIMEOUT).unwrap_or(now));
        if now >= deadline {
            return Err("the original native Source workflow exceeded 150 seconds".to_owned());
        }
        if self.seal_deadline.is_some_and(|deadline| now >= deadline) {
            return Err("the original native telemetry seal exceeded 10 seconds".to_owned());
        }
        if observation.presented_frame().is_some() {
            self.record_presentation(observation)?;
        }
        if let Some(original) = &self.original {
            let backend = observation.wgpu();
            if observation.runtime_generation() != original.runtime
                || backend.instance_id() != original.backend
                || backend.device_epoch() != original.epoch
                || backend.state() != WgpuBackendState::Ready
                || observation.adapter_summary() != Some(&original.adapter)
            {
                return Err(
                    "the original native Runtime, Host, epoch or adapter changed".to_owned(),
                );
            }
            if let Some(world) = observation.world() {
                let registry = world
                    .get_resource::<BackendWindowHandles>()
                    .ok_or("the live native window registry is unavailable")?;
                if !original.registry.shares_authority_with(registry) {
                    return Err("the live native window registry was replaced".to_owned());
                }
            }
            verify_tracking(
                observation.frame_completion_stats(),
                observation.gpu_resource_stats(),
                original.epoch,
            )?;
        }
        for sample in observation.completion_samples() {
            self.complete(*sample)?;
        }
        if let Some(seal) = observation.telemetry_seal() {
            self.record_seal(seal)?;
            return Ok(DesktopPresentationControl::Continue);
        }
        if self.seal_deadline.is_none() && self.final_presented() {
            if !self.all_states_represented() {
                return Err(
                    "Done presented without all eight commits and three refusals".to_owned(),
                );
            }
            let deadline = now.checked_add(SEAL_TIMEOUT).unwrap_or(now);
            self.seal_deadline = Some(deadline);
            return Ok(DesktopPresentationControl::SealTelemetryAndExit {
                exit: AppExit::Success,
                deadline,
            });
        }
        Ok(DesktopPresentationControl::Continue)
    }

    fn record_presentation(
        &mut self,
        observation: &DesktopPresentationObservation<'_>,
    ) -> Result<(), String> {
        if self.seal_deadline.is_some() {
            return Err("the original Host presented after submission admission closed".to_owned());
        }
        let receipt = observation
            .presented_frame()
            .ok_or("the original presentation receipt is unavailable")?;
        let facts = self
            .latest
            .take()
            .ok_or("the presented frame has no stock capture facts")?;
        let backend = observation.wgpu();
        let transaction = backend.frame_transaction_stats();
        let world = observation
            .world()
            .ok_or("the presented frame has no live Runtime World")?;
        let start = world
            .get_resource::<FrameExecutionStart>()
            .ok_or("the original App frame start is unavailable")?;
        let stats = world
            .get_resource::<FrameStats>()
            .ok_or("the original submitted frame statistics are unavailable")?;
        if facts.app != receipt.app_frame_index()
            || facts.app != start.frame()
            || facts.render != receipt.render_transaction_frame_index()
            || facts.runtime != receipt.runtime_generation()
            || facts.runtime != observation.runtime_generation()
            || facts.source.runtime_generation() != Some(facts.runtime)
            || facts.window != receipt.window_id()
            || backend.state() != WgpuBackendState::Ready
            || backend.instance_id() != receipt.backend_instance_id()
            || backend.device_epoch() != receipt.device_epoch()
            || receipt.backend_instance_id() == 0
            || receipt.device_epoch() == 0
            || receipt.surface_generation() == 0
            || transaction.app_frame_index() != Some(facts.app)
            || transaction.render_transaction_frame_index() != Some(facts.render)
            || transaction.queue_submissions() == 0
            || transaction.presents() == 0
            || stats.sprites != 1
        {
            return Err(format!(
                "the original capture does not match its actual presentation: \
                 {facts:?}, {receipt:?}, {transaction:?}, {stats:?}",
            ));
        }
        let scene = CurrentScene::get(world).ok_or("the presented CurrentScene is unavailable")?;
        let workflow = world
            .get_resource::<Workflow>()
            .ok_or("the presented Source workflow is unavailable")?;
        let current = workflow
            .current
            .as_ref()
            .ok_or("the presented resident Scene is unavailable")?;
        if scene.receipt().instance_id() != facts.instance
            || scene.source_content_stamp() != Some(facts.content)
            || current.instance.instance_id() != facts.instance
            || current.stamp != facts.content
            || current.material != facts.material
            || current
                .images
                .get(&facts.material.texture)
                .is_none_or(|image| image.pixels.as_slice() != facts.palette.as_slice())
            || workflow.step != facts.step
            || workflow.commits != facts.commits
            || workflow.rejections != facts.rejections
            || facts.commits > COMMITS
            || facts.rejections > REJECTIONS
        {
            return Err(
                "the presented capture does not name the exact current Scene and workflow"
                    .to_owned(),
            );
        }
        let registry = world
            .get_resource::<BackendWindowHandles>()
            .ok_or("the original native window registry is unavailable")?;
        let snapshot = registry
            .snapshot(receipt.window_id())
            .map_err(|error| format!("the presented native target is unavailable: {error}"))?;
        if snapshot.phase != WindowTargetPhase::Active
            || snapshot.fault.is_some()
            || !snapshot.provider_present
            || !snapshot.surface_active
            || snapshot.native_destroyed
            || snapshot.surface_generation != Some(receipt.surface_generation())
        {
            return Err(format!(
                "the actual presented surface registry does not match: {snapshot:?}",
            ));
        }
        let adapter = observation
            .adapter_summary()
            .ok_or("the original native adapter is unavailable")?;
        if let Some(original) = &self.original {
            if !original.registry.shares_authority_with(registry)
                || original.runtime != facts.runtime
                || original.window != facts.window
                || original.surface != receipt.surface_generation()
                || original.backend != receipt.backend_instance_id()
                || original.epoch != receipt.device_epoch()
                || &original.adapter != adapter
            {
                return Err(
                    "the presentation left the original native target, Host or epoch".to_owned(),
                );
            }
        } else {
            verify_adapter(adapter)?;
            self.original = Some(OriginalPresentation {
                registry: registry.clone(),
                watcher: workflow
                    .observer
                    .clone()
                    .ok_or("the original native watcher observer is unavailable")?,
                runtime: facts.runtime,
                window: facts.window,
                surface: receipt.surface_generation(),
                backend: receipt.backend_instance_id(),
                epoch: receipt.device_epoch(),
                adapter: adapter.clone(),
            });
        }
        self.verify_progress(&facts)?;
        let epoch = receipt.device_epoch();
        if let Some(representative) = self.representatives.iter_mut().find(|representative| {
            representative.facts.commits == facts.commits
                && representative.facts.rejections == facts.rejections
        }) {
            if facts.step == SourceStep::Done {
                *representative = Representative {
                    facts: facts.clone(),
                    epoch,
                    completion: None,
                };
            }
        } else {
            if self.representatives.len() >= REPRESENTATIVES {
                return Err("the finite native representative capacity was exceeded".to_owned());
            }
            self.representatives.push(Representative {
                facts: facts.clone(),
                epoch,
                completion: None,
            });
        }
        self.presentations += 1;
        self.presented = Some(facts);
        Ok(())
    }

    fn verify_progress(&self, facts: &FrameFacts) -> Result<(), String> {
        let Some(previous) = &self.presented else {
            return if facts.commits == 0 && facts.rejections == 0 {
                Ok(())
            } else {
                Err("the startup state advanced before its first actual presentation".to_owned())
            };
        };
        if facts.app <= previous.app || facts.render <= previous.render {
            return Err("the original presented App or render frame did not advance".to_owned());
        }
        if facts.commits == previous.commits
            && (facts.rejections == previous.rejections
                || facts.rejections == previous.rejections + 1)
        {
            if !same_content(facts, previous) {
                return Err(
                    "an unchanged or rejected state lost its actually presented last-good content"
                        .to_owned(),
                );
            }
        } else if facts.commits == previous.commits + 1 && facts.rejections == previous.rejections {
            if facts.instance == previous.instance {
                return Err(
                    "a successful Scene commit retained the previous incarnation".to_owned(),
                );
            }
        } else {
            return Err(
                "the Source workflow skipped an actually presented commit or refusal".to_owned(),
            );
        }
        Ok(())
    }

    fn complete(&mut self, sample: WgpuFrameCompletionSample) -> Result<(), String> {
        if self
            .original
            .as_ref()
            .is_some_and(|original| sample.device_epoch() != original.epoch)
        {
            return Err(
                "a completion sample belongs to a different original device epoch".to_owned(),
            );
        }
        if let Some(representative) = self
            .representatives
            .iter_mut()
            .find(|representative| representative.matches(sample))
            && representative.completion.replace(sample).is_some()
        {
            return Err("an exact representative completion was delivered twice".to_owned());
        }
        Ok(())
    }

    fn record_seal(&mut self, seal: &DesktopTelemetrySealReport) -> Result<(), String> {
        if self.seal_deadline.is_none() || self.sealed.is_some() || !self.final_presented() {
            return Err(
                "the original telemetry sealed without the consumer's presented Done state"
                    .to_owned(),
            );
        }
        let original = self
            .original
            .as_ref()
            .ok_or("the sealed native identity is unavailable")?;
        let final_frame = self
            .presented
            .as_ref()
            .ok_or("the sealed presentation is unavailable")?;
        let backend = seal.observation();
        let transaction = backend.frame_transaction_stats();
        if seal.outcome() != DesktopTelemetrySealOutcome::Complete
            || seal.backend_instance_id() != original.backend
            || seal.device_epoch() != original.epoch
            || seal.adapter_summary() != Some(&original.adapter)
            || backend.instance_id() != original.backend
            || backend.device_epoch() != original.epoch
            || backend.state() != WgpuBackendState::Ready
            || transaction.app_frame_index() != Some(final_frame.app)
            || transaction.render_transaction_frame_index() != Some(final_frame.render)
            || transaction.queue_submissions() == 0
            || transaction.presents() == 0
        {
            return Err(format!(
                "the original final telemetry seal was invalid: {:?}",
                seal.outcome(),
            ));
        }
        let completion = seal.frame_completion_stats();
        let resources = seal.gpu_resource_stats();
        verify_tracking(completion, resources, original.epoch)?;
        if completion.pending_samples() != 0
            || completion.buffered_samples() != 0
            || resources.pending_retirements() != 0
            || completion.completed_samples() < self.representatives.len() as u64
        {
            return Err(
                "the original Complete seal retained pending or missing completion evidence"
                    .to_owned(),
            );
        }
        self.verify_representatives()?;
        self.sealed = Some(SealedTelemetry {
            completion,
            resources,
        });
        Ok(())
    }

    fn all_states_represented(&self) -> bool {
        self.representatives.len() == REPRESENTATIVES
            && (0..=COMMITS).all(|commits| {
                self.representatives
                    .iter()
                    .any(|representative| representative.facts.commits == commits)
            })
            && (0..=REJECTIONS).all(|rejections| {
                self.representatives
                    .iter()
                    .any(|representative| representative.facts.rejections == rejections)
            })
    }

    fn verify_representatives(&self) -> Result<(), String> {
        if !self.all_states_represented() {
            return Err(
                "the original native run did not represent every commit and refusal".to_owned(),
            );
        }
        for representative in &self.representatives {
            let sample = representative.completion.ok_or_else(|| {
                format!(
                    "missing exact completion for commits={} rejections={} app={} render={} epoch={}",
                    representative.facts.commits,
                    representative.facts.rejections,
                    representative.facts.app,
                    representative.facts.render,
                    representative.epoch,
                )
            })?;
            if !representative.matches(sample) {
                return Err(
                    "a retained completion does not match its exact representative".to_owned(),
                );
            }
        }
        Ok(())
    }

    fn final_presented(&self) -> bool {
        self.presented.as_ref().is_some_and(|facts| {
            facts.step == SourceStep::Done
                && facts.commits == COMMITS
                && facts.rejections == REJECTIONS
        })
    }
}

impl Representative {
    fn matches(&self, sample: WgpuFrameCompletionSample) -> bool {
        self.facts.app == sample.app_frame_index()
            && self.facts.render == sample.render_transaction_frame_index()
            && self.epoch == sample.device_epoch()
    }
}

fn same_content(left: &FrameFacts, right: &FrameFacts) -> bool {
    left.instance == right.instance
        && left.content == right.content
        && left.material == right.material
        && left.palette == right.palette
}

fn verify_tracking(
    completion: WgpuFrameCompletionStats,
    resources: WgpuGpuResourceStats,
    epoch: u64,
) -> Result<(), String> {
    if !completion.enabled()
        || !resources.tracking_enabled()
        || completion.device_epoch() != epoch
        || resources.device_epoch() != epoch
        || completion.lost_samples() != 0
        || resources.lost_retirements() != 0
    {
        return Err(format!(
            "the original device epoch lacks exact completion tracking: {completion:?}, {resources:?}",
        ));
    }
    Ok(())
}

fn verify_adapter(adapter: &WgpuAdapterSummary) -> Result<(), String> {
    match std::env::var("WGPU_BACKEND") {
        Ok(requested) => {
            if !requested
                .split(',')
                .any(|backend| backend.trim().eq_ignore_ascii_case(adapter.backend()))
            {
                return Err(format!(
                    "the actual adapter backend {} does not match WGPU_BACKEND={requested}",
                    adapter.backend(),
                ));
            }
        }
        Err(std::env::VarError::NotPresent) => {}
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err("WGPU_BACKEND must contain valid Unicode".to_owned());
        }
    }
    if std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() == Ok("0")
        && adapter.device_type() == "cpu"
    {
        return Err(
            "the local native proof selected a CPU adapter with fallback disabled".to_owned(),
        );
    }
    Ok(())
}
