//! Root's concrete stock WGPU connection to the retained native Winit session.

use std::{
    cell::RefCell,
    fmt,
    rc::Rc,
    time::{Duration, Instant},
};

use nara_app::{AppExit, AppRunError, RuntimeGeneration, RuntimeInstance, RuntimeState};
use nara_ecs::World;
use nara_render_wgpu::{
    __private::{
        WgpuPresentedFrameReceipt, WgpuRuntimeExchangeError, WgpuRuntimeResult,
        cancel_runtime_frame, complete_runtime_frame, take_runtime_frame,
    },
    WgpuAdapterSummary, WgpuBackendTelemetryConfig, WgpuFrameCompletionSample,
    WgpuFrameCompletionStats, WgpuGpuResourceStats, WgpuRenderHost, WgpuRenderObservation,
    WgpuRenderProducer,
};
use nara_window::{WindowId, backend::BackendWindowHandles};
use nara_winit::__private::{__WinitPresentationDriver, __WinitPresentationProgress};

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
use super::verification::DesktopFirstPresentVerification;

const MAINTENANCE_POLL_INTERVAL: Duration = Duration::from_millis(2);
pub(super) const DEFAULT_TELEMETRY_SEAL_TIMEOUT: Duration = Duration::from_secs(5);

/// Finite decisions available to a CPU-only desktop presentation observer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopPresentationControl {
    Continue,
    /// Requests exit; `Requested` vetoes `Success`, including an automatic seal success.
    Exit(AppExit),
    /// Stops native submissions immediately, drains the original telemetry, then exits.
    SealTelemetryAndExit {
        exit: AppExit,
        deadline: Instant,
    },
}

/// Read-only facts at the actual post-Host boundary, including maintenance without a frame.
///
/// `world()` is available only while Runtime is Running or Paused. Completion samples are drained
/// once from the original local Host and belong to this callback invocation; their exact App,
/// render and device identities are retained. The final telemetry seal is delivered once, before
/// epoch cleanup; it remains available in the run report. No native GPU handle is exposed.
pub struct DesktopPresentationObservation<'a> {
    generation: RuntimeGeneration,
    now: Instant,
    world: Option<&'a World>,
    wgpu: &'a WgpuRenderObservation,
    presented: Option<WgpuPresentedFrameReceipt>,
    completion_samples: &'a [WgpuFrameCompletionSample],
    telemetry_seal: Option<&'a DesktopTelemetrySealReport>,
    adapter: Option<WgpuAdapterSummary>,
    completion: WgpuFrameCompletionStats,
    resources: WgpuGpuResourceStats,
}

impl<'a> DesktopPresentationObservation<'a> {
    #[must_use]
    pub const fn runtime_generation(&self) -> RuntimeGeneration {
        self.generation
    }

    #[must_use]
    pub const fn now(&self) -> Instant {
        self.now
    }

    #[must_use]
    pub const fn world(&self) -> Option<&'a World> {
        self.world
    }

    #[must_use]
    pub const fn wgpu(&self) -> &'a WgpuRenderObservation {
        self.wgpu
    }

    #[must_use]
    pub const fn presented_frame(&self) -> Option<WgpuPresentedFrameReceipt> {
        self.presented
    }

    #[must_use]
    pub const fn completion_samples(&self) -> &'a [WgpuFrameCompletionSample] {
        self.completion_samples
    }

    #[must_use]
    pub const fn telemetry_seal(&self) -> Option<&'a DesktopTelemetrySealReport> {
        self.telemetry_seal
    }

    #[must_use]
    pub fn adapter_summary(&self) -> Option<&WgpuAdapterSummary> {
        self.adapter.as_ref()
    }

    #[must_use]
    pub const fn frame_completion_stats(&self) -> WgpuFrameCompletionStats {
        self.completion
    }

    #[must_use]
    pub const fn gpu_resource_stats(&self) -> WgpuGpuResourceStats {
        self.resources
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopTelemetrySealOutcome {
    Complete,
    DeadlineExceeded,
    LostEvidence,
    PollFailed,
    EpochChanged,
    TrackingUnavailable,
}

/// Final CPU evidence captured before native retirement can clear the original device epoch.
#[derive(Clone, Debug)]
pub struct DesktopTelemetrySealReport {
    outcome: DesktopTelemetrySealOutcome,
    observation: WgpuRenderObservation,
    completion: WgpuFrameCompletionStats,
    resources: WgpuGpuResourceStats,
    samples: Vec<WgpuFrameCompletionSample>,
    instance_id: u64,
    device_epoch: u64,
    adapter: Option<WgpuAdapterSummary>,
}

impl DesktopTelemetrySealReport {
    #[must_use]
    pub const fn outcome(&self) -> DesktopTelemetrySealOutcome {
        self.outcome
    }

    #[must_use]
    pub const fn observation(&self) -> &WgpuRenderObservation {
        &self.observation
    }

    #[must_use]
    pub const fn frame_completion_stats(&self) -> WgpuFrameCompletionStats {
        self.completion
    }

    #[must_use]
    pub const fn gpu_resource_stats(&self) -> WgpuGpuResourceStats {
        self.resources
    }

    /// Samples drained after submission admission closed, without unbounded run-long retention.
    /// This cumulative audit snapshot overlaps observer batches; do not replay it into the same
    /// incremental measurement collector.
    #[must_use]
    pub fn completion_samples(&self) -> &[WgpuFrameCompletionSample] {
        &self.samples
    }

    #[must_use]
    pub const fn backend_instance_id(&self) -> u64 {
        self.instance_id
    }

    #[must_use]
    pub const fn device_epoch(&self) -> u64 {
        self.device_epoch
    }

    #[must_use]
    pub fn adapter_summary(&self) -> Option<&WgpuAdapterSummary> {
        self.adapter.as_ref()
    }
}

pub(super) type DesktopPresentationObserver =
    Box<dyn FnMut(DesktopPresentationObservation<'_>) -> DesktopPresentationControl>;

#[derive(Default)]
pub(super) struct DesktopPresentationOptions {
    pub(super) telemetry: Option<WgpuBackendTelemetryConfig>,
    pub(super) observer: Option<DesktopPresentationObserver>,
}

impl fmt::Debug for DesktopPresentationOptions {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DesktopPresentationOptions")
            .field("telemetry", &self.telemetry)
            .field("observer_installed", &self.observer.is_some())
            .finish()
    }
}

/// This shared value holds CPU diagnostics only. WinitSession alone owns the live Host.
#[derive(Clone, Default)]
pub(super) struct DesktopPresentationReports(Rc<RefCell<DesktopPresentationReportState>>);

#[derive(Default)]
struct DesktopPresentationReportState {
    telemetry_seal: Option<DesktopTelemetrySealReport>,
    exit: Option<AppExit>,
}

impl DesktopPresentationReports {
    pub(super) fn telemetry_seal(&self) -> Option<DesktopTelemetrySealReport> {
        self.0.borrow().telemetry_seal.clone()
    }

    pub(super) fn reconcile_exit(&self, native_exit: AppExit) -> AppExit {
        // Target retirement returns only a bool to Winit. Preserve a final observer veto even
        // when it arrives through that path rather than a normal maintain progress value.
        merge_exit(Some(native_exit), self.0.borrow().exit).unwrap_or(native_exit)
    }
}

struct TelemetrySeal {
    deadline: Instant,
    exit: Option<AppExit>,
    instance_id: u64,
    epoch: u64,
    samples: Vec<WgpuFrameCompletionSample>,
}

pub(super) struct DesktopPresentationDriver {
    host: WgpuRenderHost,
    producer: Option<WgpuRenderProducer>,
    pending_result: Option<WgpuRuntimeResult>,
    #[cfg(all(feature = "runtime-2d", feature = "serde"))]
    verification: Option<DesktopFirstPresentVerification>,
    observer: Option<DesktopPresentationObserver>,
    reports: DesktopPresentationReports,
    seal: Option<TelemetrySeal>,
    telemetry_enabled: bool,
    retiring: bool,
    attachment_failure: Option<AppRunError>,
    last_observation: WgpuRenderObservation,
    seal_report_delivered: bool,
    #[cfg(feature = "presentation-ui")]
    ui: Option<crate::ui_presentation::post_host::UiPostHostConnection>,
}

impl DesktopPresentationDriver {
    pub(super) fn new(
        runtime: &RuntimeInstance,
        options: DesktopPresentationOptions,
        #[cfg(all(feature = "runtime-2d", feature = "serde"))] verification: Option<
            DesktopFirstPresentVerification,
        >,
        reports: DesktopPresentationReports,
    ) -> Self {
        let mut host = WgpuRenderHost::offscreen();
        let telemetry = options.telemetry;
        if let Some(config) = telemetry {
            host.configure_telemetry(config);
        }
        #[cfg(feature = "presentation-ui")]
        let (ui, attachment_failure) =
            match crate::ui_presentation::post_host::UiPostHostConnection::capture(runtime) {
                Ok(ui) => (ui, None),
                Err(error) => (None, Some(error)),
            };
        #[cfg(not(feature = "presentation-ui"))]
        let _ = runtime;
        #[cfg(not(feature = "presentation-ui"))]
        let attachment_failure = None;
        Self {
            host,
            producer: None,
            pending_result: None,
            #[cfg(all(feature = "runtime-2d", feature = "serde"))]
            verification,
            observer: options.observer,
            reports,
            seal: None,
            telemetry_enabled: telemetry.is_some(),
            retiring: false,
            attachment_failure,
            last_observation: WgpuRenderObservation::default(),
            seal_report_delivered: false,
            #[cfg(feature = "presentation-ui")]
            ui,
        }
    }

    fn complete_pending(
        &mut self,
        runtime: &mut RuntimeInstance,
    ) -> Result<Option<WgpuPresentedFrameReceipt>, AppRunError> {
        let Some(result) = self.pending_result.take() else {
            return Ok(None);
        };
        let observation = result.observation().cloned();
        match complete_runtime_frame(runtime, result) {
            Ok(outcome) => {
                // Output has consumed the original result. Never put it back after a postfault.
                let completion = outcome.output;
                if !completion.retired
                    && let Some(observation) = observation
                {
                    self.last_observation = observation;
                }
                if let Some(fault) = outcome.post_operation_fault {
                    return Err(AppRunError::managed_runtime_fault(&fault));
                }
                if !is_live(runtime) || completion.retired {
                    return Ok(None);
                }
                #[cfg(feature = "presentation-ui")]
                if let Some(ui) = &self.ui {
                    ui.complete(runtime)?;
                }
                Ok(completion.presented)
            }
            Err(failure) => {
                // Scope/port refusal is before consumption and retains that exact result.
                self.pending_result = Some(failure.result);
                Err(exchange_failure(
                    "the original WGPU Runtime result was refused",
                    failure.error,
                    failure.post_operation_fault,
                ))
            }
        }
    }

    fn service_frame(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
    ) -> Result<Option<WgpuPresentedFrameReceipt>, AppRunError> {
        if let Some(error) = self.attachment_failure.take() {
            self.retiring = true;
            return Err(error);
        }
        if self.pending_result.is_some() {
            return self.complete_pending(runtime);
        }
        if self.retiring
            || self.seal.is_some()
            || self.reports.0.borrow().telemetry_seal.is_some()
            || !is_live(runtime)
        {
            return Ok(None);
        }
        let outcome = take_runtime_frame(runtime).map_err(|failure| {
            exchange_failure(
                "WGPU Runtime capture failed",
                failure.error,
                failure.post_operation_fault,
            )
        })?;
        let fault = outcome.post_operation_fault;
        if let Some(capture) = outcome.output {
            if fault.is_some() {
                self.pending_result = Some(cancel_runtime_frame(capture));
            } else {
                if self.producer.is_none() {
                    match self.host.create_producer() {
                        Ok(producer) => self.producer = Some(producer),
                        Err(error) => {
                            // Return the original capture's obligations before reporting failure.
                            self.pending_result = Some(cancel_runtime_frame(capture));
                            let completion = self.complete_pending(runtime);
                            let error = AppRunError::runner(format!(
                                "WGPU Runtime producer creation failed: {error}"
                            ));
                            return Err(match completion {
                                Ok(_) => error,
                                Err(completion_error) => {
                                    AppRunError::runner_teardown(error, completion_error)
                                }
                            });
                        }
                    }
                }
                self.pending_result = Some(
                    self.host.execute_runtime_frame(
                        self.producer
                            .as_ref()
                            .expect("the original Runtime producer was just admitted"),
                        handles,
                        capture,
                    ),
                );
            }
        }
        let completion = self.complete_pending(runtime);
        if let Some(fault) = fault {
            // Reconcile the returned capture/result before delivering its scope fault once.
            return Err(match completion {
                Ok(_) => AppRunError::managed_runtime_fault(&fault),
                Err(error) => with_postfault(error, Some(fault)),
            });
        }
        completion
    }

    fn begin_seal(&mut self, deadline: Instant, exit: Option<AppExit>) {
        if self.seal.is_some() || self.reports.0.borrow().telemetry_seal.is_some() {
            return;
        }
        let facts = self.host.runtime_facts();
        self.seal = Some(TelemetrySeal {
            deadline,
            exit,
            instance_id: facts.backend_instance_id,
            epoch: facts.device_epoch,
            samples: Vec::new(),
        });
    }

    fn poll_seal(
        &mut self,
        now: Instant,
        poll_failed: bool,
        samples: &[WgpuFrameCompletionSample],
    ) -> Option<AppExit> {
        let seal = self.seal.as_mut()?;
        seal.samples.extend_from_slice(samples);
        let facts = self.host.runtime_facts();
        let completion = facts.completion;
        let resources = facts.resources;
        let outcome = if poll_failed {
            Some(DesktopTelemetrySealOutcome::PollFailed)
        } else if facts.backend_instance_id != seal.instance_id
            || facts.device_epoch != seal.epoch
            || completion.device_epoch() != seal.epoch
            || resources.device_epoch() != seal.epoch
        {
            Some(DesktopTelemetrySealOutcome::EpochChanged)
        } else if !completion.enabled() || !resources.tracking_enabled() || seal.epoch == 0 {
            Some(DesktopTelemetrySealOutcome::TrackingUnavailable)
        } else if completion.lost_samples() != 0 || resources.lost_retirements() != 0 {
            Some(DesktopTelemetrySealOutcome::LostEvidence)
        } else if now >= seal.deadline {
            Some(DesktopTelemetrySealOutcome::DeadlineExceeded)
        } else if completion.pending_samples() == 0
            && completion.buffered_samples() == 0
            && resources.pending_retirements() == 0
        {
            Some(DesktopTelemetrySealOutcome::Complete)
        } else {
            None
        };
        let outcome = outcome?;
        let seal = self
            .seal
            .take()
            .expect("a completed telemetry seal retains its original input");
        self.reports.0.borrow_mut().telemetry_seal = Some(DesktopTelemetrySealReport {
            outcome,
            observation: self.last_observation.clone(),
            completion,
            resources,
            samples: seal.samples,
            instance_id: facts.backend_instance_id,
            device_epoch: facts.device_epoch,
            adapter: facts.adapter,
        });
        seal.exit.map(|exit| {
            if outcome == DesktopTelemetrySealOutcome::Complete {
                exit
            } else {
                AppExit::Requested
            }
        })
    }

    fn observe(
        &mut self,
        runtime: &mut RuntimeInstance,
        presented: Option<WgpuPresentedFrameReceipt>,
        now: Instant,
    ) -> Result<__WinitPresentationProgress, AppRunError> {
        let mut progress = __WinitPresentationProgress {
            exit: None,
            next_wake: None,
        };
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        if let Some(verification) = &self.verification {
            let verified = verification.observe(runtime.generation(), presented, now);
            progress.exit = verified.exit;
            progress.next_wake = verified.next_wake;
        }

        let maintenance = self.host.poll_runtime_maintenance();
        let samples = maintenance.samples;
        let now = Instant::now().max(now);
        progress.exit = merge_exit(
            progress.exit,
            self.poll_seal(now, maintenance.error.is_some(), &samples),
        );
        let progress = self.observe_consumer(runtime, presented, now, &samples, progress);
        if let Some(error) = maintenance.error {
            return Err(AppRunError::runner(format!(
                "WGPU telemetry polling failed: {error}"
            )));
        }
        Ok(progress)
    }

    /// Delivers CPU facts and applies the consumer's final veto before returning an exit.
    fn observe_consumer(
        &mut self,
        runtime: &RuntimeInstance,
        presented: Option<WgpuPresentedFrameReceipt>,
        now: Instant,
        samples: &[WgpuFrameCompletionSample],
        mut progress: __WinitPresentationProgress,
    ) -> __WinitPresentationProgress {
        let report = self.reports.telemetry_seal();
        let live = is_live(runtime);
        let facts = self.host.runtime_facts();
        if (report.is_none() || !self.seal_report_delivered)
            && let Some(observer) = &mut self.observer
        {
            let control = observer(DesktopPresentationObservation {
                generation: runtime.generation(),
                now,
                world: live.then(|| runtime.world()),
                wgpu: &self.last_observation,
                presented: presented.filter(|_| live),
                completion_samples: samples,
                telemetry_seal: report.as_ref(),
                adapter: facts.adapter,
                completion: facts.completion,
                resources: facts.resources,
            });
            match control {
                DesktopPresentationControl::Continue => {}
                DesktopPresentationControl::Exit(request) => {
                    progress.exit = merge_exit(progress.exit, Some(request));
                }
                DesktopPresentationControl::SealTelemetryAndExit {
                    exit: request,
                    deadline,
                } => {
                    // The exit decision is authoritative even when sealing already started or
                    // completed. Repeated requests retain the original seal and its deadline.
                    progress.exit = merge_exit(progress.exit, Some(request));
                    if report.is_none() {
                        self.begin_seal(deadline, None);
                    }
                }
            }
            // A finite consumer deadline must keep progressing even under Winit Wait/occlusion.
            progress.next_wake = earlier(
                progress.next_wake,
                now.checked_add(MAINTENANCE_POLL_INTERVAL),
            );
            self.seal_report_delivered = report.is_some();
        }
        if let Some(seal) = &self.seal {
            let wake = now
                .checked_add(MAINTENANCE_POLL_INTERVAL)
                .unwrap_or(now)
                .min(seal.deadline);
            progress.next_wake = earlier(progress.next_wake, Some(wake));
            // No exit can bypass an already admitted finite telemetry seal.
            if let Some(request) = progress.exit.take() {
                let seal = self
                    .seal
                    .as_mut()
                    .expect("the active seal was just observed");
                seal.exit = merge_exit(seal.exit, Some(request));
            }
        }
        let mut reports = self.reports.0.borrow_mut();
        reports.exit = merge_exit(reports.exit, progress.exit);
        progress
    }
}

impl __WinitPresentationDriver for DesktopPresentationDriver {
    fn maintain(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        now: Instant,
    ) -> Result<__WinitPresentationProgress, AppRunError> {
        let frame = self.service_frame(runtime, handles);
        if frame.is_err() {
            self.retiring = true;
        }
        // Deadlines and READY I/O advance even when capture or completion was refused.
        let progress = self.observe(
            runtime,
            frame.as_ref().ok().copied().flatten(),
            Instant::now().max(now),
        );
        match (frame, progress) {
            (Ok(_), progress) => progress,
            (Err(error), Ok(_)) => Err(error),
            (Err(error), Err(observation_error)) => {
                Err(AppRunError::runner_teardown(error, observation_error))
            }
        }
    }

    fn retire(
        &mut self,
        runtime: &mut RuntimeInstance,
        _handles: &BackendWindowHandles,
        window_ids: &[WindowId],
        now: Instant,
    ) -> Result<bool, AppRunError> {
        self.retiring = true;
        let completion = self.complete_pending(runtime);
        if self.telemetry_enabled
            && self.seal.is_none()
            && self.reports.0.borrow().telemetry_seal.is_none()
        {
            self.begin_seal(
                now.checked_add(DEFAULT_TELEMETRY_SEAL_TIMEOUT)
                    .unwrap_or(now),
                None,
            );
        }
        let observed = self.observe(runtime, None, now);
        // Settle original results and observe the seal before whole-Host producer/native close.
        let native = if self.seal.is_some() || self.pending_result.is_some() {
            Ok(false)
        } else {
            self.host
                .poll_runtime_retirement(window_ids)
                .map_err(|error| {
                    AppRunError::runner(format!("WGPU native retirement failed: {error}"))
                })
        };
        completion?;
        observed?;
        Ok(native? && self.pending_result.is_none())
    }
}

fn is_live(runtime: &RuntimeInstance) -> bool {
    matches!(
        runtime.state(),
        RuntimeState::Running | RuntimeState::Paused
    ) && runtime.fault().is_none()
}

fn earlier(left: Option<Instant>, right: Option<Instant>) -> Option<Instant> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, right) => left.or(right),
    }
}

/// A consumer veto or mandatory failed verification must survive any successful completion.
fn merge_exit(current: Option<AppExit>, incoming: Option<AppExit>) -> Option<AppExit> {
    match (current, incoming) {
        (Some(AppExit::Requested), _) | (_, Some(AppExit::Requested)) => Some(AppExit::Requested),
        (Some(AppExit::Success), _) | (_, Some(AppExit::Success)) => Some(AppExit::Success),
        (None, None) => None,
    }
}

fn with_postfault(error: AppRunError, fault: Option<nara_app::RuntimeFault>) -> AppRunError {
    match fault {
        Some(fault) if !contains_fault(&error, &fault) => {
            AppRunError::runner_teardown(error, AppRunError::managed_runtime_fault(&fault))
        }
        Some(_) | None => error,
    }
}

fn contains_fault(error: &AppRunError, fault: &nara_app::RuntimeFault) -> bool {
    match error {
        AppRunError::ManagedRuntime {
            kind,
            fault_source,
            detail,
        } => *kind == fault.kind() && *fault_source == fault.source() && *detail == fault.detail(),
        AppRunError::RunnerTeardown { prior, teardown } => {
            contains_fault(prior, fault) || contains_fault(teardown, fault)
        }
        _ => false,
    }
}

fn exchange_failure(
    context: &'static str,
    error: WgpuRuntimeExchangeError,
    postfault: Option<nara_app::RuntimeFault>,
) -> AppRunError {
    let error = match &error {
        WgpuRuntimeExchangeError::Scope(error) if error.fault().is_some() => {
            AppRunError::managed_runtime_fault(
                error.fault().expect("the scope fault was just observed"),
            )
        }
        _ => AppRunError::runner(format!("{context}: {error}")),
    };
    with_postfault(error, postfault)
}

#[cfg(test)]
mod tests;
