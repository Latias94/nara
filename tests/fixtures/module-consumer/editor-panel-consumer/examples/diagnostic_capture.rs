//! Concrete Host-composed execution capture; no subscriber is installed by engine plugins.

#[path = "diagnostic_capture/authoring.rs"]
mod authoring;
#[path = "diagnostic_capture/capture.rs"]
mod capture;

use std::{
    cell::RefCell,
    fs::OpenOptions,
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::ExitCode,
    rc::Rc,
    time::{Duration, Instant},
};

use nara::{
    app::{AppExit, FrameExecutionStart},
    project_host::{
        DesktopControlError, DesktopPresentationControl, DesktopPresentationObservation,
        DesktopRun, DesktopRunIntent, DesktopRunOptions, DesktopRunOutcome, DesktopRunProgress,
        DesktopTelemetrySealOutcome, WinitEventLoop,
    },
    render_wgpu::{WgpuBackendState, WgpuBackendTelemetryConfig, WgpuFrameCompletionSample},
    window::{
        WindowId,
        backend::{BackendWindowHandles, WindowTargetPhase},
    },
};
use serde_json::{Value, json};
use two_level_game::{GameSnapshot, capture_desktop_resume};

use capture::{Capture, GpuIdentity, Limits};

const FRAMES: usize = 12;
const TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(2);

#[derive(Clone, Copy)]
enum Mode {
    Off,
    On,
    Overflow,
    CollectionFailure,
    ExportFailure,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Off => "disabled",
            Self::On => "enabled",
            Self::Overflow => "overflow",
            Self::CollectionFailure => "collection_failure",
            Self::ExportFailure => "export_failure",
        }
    }
}

struct SubmittedFrame {
    identity: GpuIdentity,
    started: Instant,
    post_host: Instant,
    completion: Option<Duration>,
}

#[derive(Default)]
struct ProductFacts {
    frames: Vec<SubmittedFrame>,
    extra_completions: usize,
    target: Option<BackendWindowHandles>,
    adapter: Option<Value>,
    seal_requested: bool,
    seal: Option<DesktopTelemetrySealOutcome>,
    failure: Option<&'static str>,
    final_captured: bool,
}

impl ProductFacts {
    fn observe(
        &mut self,
        observation: &DesktopPresentationObservation<'_>,
    ) -> DesktopPresentationControl {
        if let Err(error) = self.record(observation) {
            self.failure.get_or_insert(error);
        }
        if self.final_captured && !self.seal_requested {
            self.seal_requested = true;
            DesktopPresentationControl::SealTelemetryAndExit {
                exit: AppExit::Success,
                deadline: Instant::now() + TIMEOUT,
            }
        } else {
            DesktopPresentationControl::Continue
        }
    }

    fn record(
        &mut self,
        observation: &DesktopPresentationObservation<'_>,
    ) -> Result<(), &'static str> {
        if let Some(seal) = observation.telemetry_seal() {
            self.seal = Some(seal.outcome());
            if seal.outcome() != DesktopTelemetrySealOutcome::Complete
                || seal.frame_completion_stats().pending_samples() != 0
                || seal.frame_completion_stats().lost_samples() != 0
                || seal.gpu_resource_stats().pending_retirements() != 0
                || seal.gpu_resource_stats().lost_retirements() != 0
            {
                return Err("original telemetry seal was incomplete");
            }
        }
        if let Some(world) = observation.world() {
            if self.target.is_none() {
                self.target = world.get_resource::<BackendWindowHandles>().cloned();
            }
            let game = world
                .get_resource::<GameSnapshot>()
                .ok_or("game observation unavailable")?;
            if game.error.is_some()
                || game.tick != 0
                || game.logical_tick != 0
                || game.held != [false; 4]
            {
                return Err("paused no-input product state changed");
            }
            if let Some(receipt) = observation.presented_frame() {
                let backend = observation.wgpu();
                let transaction = backend.frame_transaction_stats();
                let start = world
                    .get_resource::<FrameExecutionStart>()
                    .ok_or("original frame start unavailable")?;
                if start.frame() != receipt.app_frame_index()
                    || receipt.runtime_generation() != observation.runtime_generation()
                    || backend.state() != WgpuBackendState::Ready
                    || backend.instance_id() != receipt.backend_instance_id()
                    || backend.device_epoch() != receipt.device_epoch()
                    || transaction.app_frame_index() != Some(receipt.app_frame_index())
                    || transaction.render_transaction_frame_index()
                        != Some(receipt.render_transaction_frame_index())
                    || transaction.queue_submissions() == 0
                    || transaction.presents() == 0
                {
                    return Err("receipt does not name the original current transaction");
                }
                let adapter = observation
                    .adapter_summary()
                    .ok_or("original adapter summary unavailable")?;
                let extent = backend
                    .configured_surface_extent(receipt.window_id())
                    .ok_or("original presented extent unavailable")?;
                let adapter = json!({
                    "vendor": adapter.vendor(), "device": adapter.device(),
                    "backend": adapter.backend(), "device_type": adapter.device_type(),
                    "extent": [extent.width, extent.height],
                });
                if self
                    .adapter
                    .as_ref()
                    .is_some_and(|original| original != &adapter)
                {
                    return Err("original adapter or presented extent changed");
                }
                self.adapter.get_or_insert(adapter);
                let identity = GpuIdentity {
                    runtime: receipt.runtime_generation().get(),
                    app_frame: receipt.app_frame_index(),
                    render_frame: receipt.render_transaction_frame_index(),
                    window: receipt.window_id().get(),
                    surface_generation: receipt.surface_generation(),
                    backend_instance: receipt.backend_instance_id(),
                    device_epoch: receipt.device_epoch(),
                };
                if !self
                    .frames
                    .iter()
                    .any(|frame| same_frame(&frame.identity, &identity))
                {
                    if self.frames.len() >= FRAMES {
                        return Err(
                            "finite product admitted an unexpected additional presentation",
                        );
                    }
                    if self.frames.last().is_some_and(|previous| {
                        previous.identity.runtime != identity.runtime
                            || previous.identity.backend_instance != identity.backend_instance
                            || previous.identity.device_epoch != identity.device_epoch
                            || previous.identity.app_frame >= identity.app_frame
                            || previous.identity.render_frame >= identity.render_frame
                    }) {
                        return Err("original presentation identities regressed");
                    }
                    self.frames.push(SubmittedFrame {
                        identity,
                        started: start.started_at(),
                        post_host: observation.now(),
                        completion: None,
                    });
                }
            }
        }
        for sample in observation.completion_samples() {
            self.complete(*sample)?;
        }
        Ok(())
    }

    fn complete(&mut self, sample: WgpuFrameCompletionSample) -> Result<(), &'static str> {
        let Some(frame) = self.frames.iter_mut().find(|frame| {
            frame.identity.app_frame == sample.app_frame_index()
                && frame.identity.render_frame == sample.render_transaction_frame_index()
                && frame.identity.device_epoch == sample.device_epoch()
        }) else {
            self.extra_completions += 1;
            return Ok(());
        };
        if frame
            .completion
            .replace(Duration::from_nanos(sample.duration_ns()))
            .is_some()
        {
            return Err("original completion was delivered twice");
        }
        Ok(())
    }
}

fn same_frame(left: &GpuIdentity, right: &GpuIdentity) -> bool {
    left.runtime == right.runtime
        && left.app_frame == right.app_frame
        && left.render_frame == right.render_frame
        && left.device_epoch == right.device_epoch
}

struct NativeProduct {
    run: Option<DesktopRun>,
    facts: Rc<RefCell<ProductFacts>>,
}

impl NativeProduct {
    fn run(&mut self) -> &mut DesktopRun {
        self.run
            .as_mut()
            .expect("the original desktop owner remains retained")
    }

    fn close(&mut self) -> Result<WinitEventLoop, String> {
        if !matches!(
            self.run().progress(),
            DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
        ) {
            self.run().request_close();
        }
        let deadline = Instant::now() + TIMEOUT;
        let mut retries = 0;
        loop {
            match self.run().poll(POLL) {
                DesktopRunProgress::Completed(_) => break,
                DesktopRunProgress::Running | DesktopRunProgress::Retiring
                    if Instant::now() < deadline => {}
                DesktopRunProgress::CleanupIncomplete
                    if retries < 2 && Instant::now() < deadline =>
                {
                    retries += 1;
                    self.run().execute();
                }
                progress => {
                    return Err(format!(
                        "original native cleanup remains incomplete: {progress:?}"
                    ));
                }
            }
        }
        let target = self.facts.borrow().target.clone();
        if let Some(target) = target {
            let snapshot = target
                .snapshot(WindowId::PRIMARY)
                .map_err(|_| "original native target unavailable")?;
            if snapshot.phase != WindowTargetPhase::NativeDestroyed
                || snapshot.fault.is_some()
                || snapshot.surface_active
                || snapshot.provider_present
                || !snapshot.native_destroyed
            {
                return Err("original native target did not physically retire".into());
            }
        } else if !self.facts.borrow().frames.is_empty() {
            return Err(
                "presented product did not retain its original native target evidence".into(),
            );
        }
        self.run()
            .take_event_loop()
            .ok_or_else(|| "original physical loop was not recovered".into())
    }

    fn execute(&mut self) -> Result<Value, String> {
        let deadline = Instant::now() + TIMEOUT;
        let mut canonical: Option<Value> = None;
        let mut final_canonical = None;
        let mut polls = 0_u64;
        loop {
            if Instant::now() >= deadline {
                return Err("finite desktop product exceeded its cooperative deadline".into());
            }
            let span = tracing::info_span!(target: "nara::capture::host", "native_poll", poll = polls, runtime = tracing::field::Empty);
            let progress = span.in_scope(|| self.run().poll(POLL));
            if let Some(generation) = self.run().runtime_generation() {
                span.record("runtime", generation.get());
            }
            drop(span);
            polls += 1;
            if let Some(failure) = self.facts.borrow().failure {
                return Err(format!("original product observation failed: {failure}"));
            }
            if let DesktopRunProgress::Completed(AppExit::Success) = progress {
                break;
            }
            if progress != DesktopRunProgress::Running && progress != DesktopRunProgress::Retiring {
                if progress == DesktopRunProgress::Failed {
                    let report = self.run().execute();
                    for diagnostic in report.diagnostics().iter().take(8) {
                        eprintln!(
                            "diagnostic_capture: original code={}",
                            diagnostic.code().as_str()
                        );
                    }
                }
                return Err(format!(
                    "original desktop product stopped unsuccessfully: {progress:?}"
                ));
            }
            if progress == DesktopRunProgress::Running && !self.facts.borrow().seal_requested {
                let mut captured = false;
                match capture_desktop_resume(self.run()) {
                    Ok(state) => {
                        captured = true;
                        let value = serde_json::to_value(state)
                            .map_err(|_| "canonical state encoding failed")?;
                        if canonical
                            .as_ref()
                            .is_some_and(|expected| expected != &value)
                        {
                            return Err("complete paused game state changed".into());
                        }
                        canonical.get_or_insert_with(|| value.clone());
                        if self.facts.borrow().frames.len() == FRAMES {
                            final_canonical = Some(value);
                            self.facts.borrow_mut().final_captured = true;
                        }
                    }
                    Err(two_level_game::GameResumeError::Desktop(
                        DesktopControlError::NotPaused,
                    )) => {}
                    Err(_) if self.facts.borrow().frames.is_empty() => {}
                    Err(_) => return Err("original paused capture was refused".into()),
                }
                if captured && self.facts.borrow().frames.len() < FRAMES {
                    self.run()
                        .request_paused_maintenance()
                        .map_err(|_| "original paused maintenance refused")?;
                }
            }
        }
        if self.run().execute().outcome() != DesktopRunOutcome::Completed(AppExit::Success) {
            return Err("original desktop report did not complete".into());
        }
        let facts = self.facts.borrow();
        if facts.frames.len() != FRAMES
            || facts.extra_completions != 0
            || facts.frames.iter().any(|frame| frame.completion.is_none())
            || facts.seal != Some(DesktopTelemetrySealOutcome::Complete)
        {
            return Err("original finite frame/completion/seal oracle was incomplete".into());
        }
        Ok(json!({
            "frames": facts.frames.len(), "completions": facts.frames.len(),
            "game_state": final_canonical.ok_or("no complete final canonical game capture")?,
            "telemetry_sealed": true, "native_polls": polls,
            "adapter": facts.adapter.as_ref().ok_or("original adapter observation missing")?,
        }))
    }
}

impl Drop for NativeProduct {
    fn drop(&mut self) {
        if let Some(run) = self.run.take()
            && !matches!(
                run.progress(),
                DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
            )
        {
            std::mem::forget(run);
            eprintln!(
                "diagnostic_capture: original live desktop owner retained until process exit"
            );
        }
    }
}

fn native(
    capture: &Capture,
    event_loop: Option<WinitEventLoop>,
) -> Result<(WinitEventLoop, Value), String> {
    let facts = Rc::new(RefCell::new(ProductFacts::default()));
    let observer = Rc::clone(&facts);
    let mut options = DesktopRunOptions::new()
        .with_initial_pause()
        .with_initial_visibility(false)
        .with_input_isolation(true)
        .with_control_flow(nara::winit::WinitControlFlow::Wait)
        .with_telemetry(WgpuBackendTelemetryConfig::default())
        .with_presentation_observer(move |observation| observer.borrow_mut().observe(&observation));
    if let Some(event_loop) = event_loop {
        options = options.with_event_loop(event_loop);
    }
    let root = two_level_game::open_project_root().map_err(|_| "game source admission failed")?;
    let mut product = NativeProduct {
        run: Some(DesktopRun::new(
            root,
            DesktopRunIntent::new()
                .with_profile("desktop")
                .with_recipe(two_level_game::product_recipe())
                .with_execution(options),
        )),
        facts,
    };
    let measured = catch_unwind(AssertUnwindSafe(|| product.execute()))
        .unwrap_or_else(|_| Err("original product panicked before cleanup".into()));
    let cleanup = catch_unwind(AssertUnwindSafe(|| product.close()))
        .unwrap_or_else(|_| Err("original product cleanup panicked; live owner retained".into()));
    let physical = cleanup.map_err(|cleanup| {
        format!(
            "{cleanup}; execution: {}",
            measured
                .as_ref()
                .err()
                .map(String::as_str)
                .unwrap_or("completed")
        )
    })?;
    let mut report = measured?;
    let facts = product.facts.borrow();
    for frame in &facts.frames {
        capture.presented(
            frame.started,
            frame
                .post_host
                .checked_duration_since(frame.started)
                .ok_or("original presentation clock regressed")?,
            frame.identity,
        );
        capture.gpu_complete(
            frame.started,
            frame.completion.ok_or("completion missing after close")?,
            GpuIdentity {
                runtime: frame.identity.runtime,
                app_frame: frame.identity.app_frame,
                render_frame: frame.identity.render_frame,
                window: frame.identity.window,
                surface_generation: frame.identity.surface_generation,
                backend_instance: frame.identity.backend_instance,
                device_epoch: frame.identity.device_epoch,
            },
        );
    }
    report["physical_cleanup"] = json!(true);
    Ok((physical, report))
}

fn one(
    mode: Mode,
    run_index: usize,
    output: &Path,
    physical: Option<WinitEventLoop>,
) -> Result<(WinitEventLoop, Value), String> {
    let capture = match mode {
        Mode::Off => Capture::disabled(),
        Mode::Overflow => Capture::new(Limits {
            events: 1,
            active_spans: 1,
            retained_bytes: 4096,
        }),
        _ => Capture::new(Limits {
            events: 4096,
            active_spans: 64,
            retained_bytes: 4 * 1024 * 1024,
        }),
    };
    if matches!(mode, Mode::CollectionFailure) {
        capture.fail();
    }
    let dispatch = capture.dispatch();
    let subscription = tracing::dispatcher::set_default(&dispatch);
    let started = Instant::now();
    let operation =
        tracing::info_span!(target: "nara::capture::host", "host_workflow", outcome = "pending");
    let guard = operation.enter();
    let authoring = authoring::run()?;
    let (physical, game) = native(&capture, physical)?;
    operation.record("outcome", "success");
    drop(guard);
    drop(operation);
    let elapsed = started.elapsed();
    drop(subscription);
    let report = capture.report();
    let complete = report.incomplete == 0 && report.active_spans == 0 && report.faults == 0;
    let valid = match mode {
        Mode::Off => complete && report.events == 0 && report.reserved_bytes == 0,
        Mode::On | Mode::ExportFailure => {
            complete
                && report.events > FRAMES
                && report.dropped_events == 0
                && report.dropped_spans == 0
                && report.rejected_fields == 0
        }
        Mode::Overflow => {
            report.incomplete != 0 && report.dropped_events + report.dropped_spans > 0
        }
        Mode::CollectionFailure => report.incomplete != 0 && report.faults > 0,
    };
    if !valid {
        let partial_exported = capture
            .write_new(&output.join(format!("capture-{run_index}-incomplete.json")))
            .is_ok();
        return Err(format!(
            "capture control observation disagrees after original product cleanup: {report:?}; partial_exported={partial_exported}"
        ));
    }
    let target = output.join(format!("capture-{run_index}.json"));
    if matches!(mode, Mode::ExportFailure) {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .and_then(|mut file| file.write_all(b"reserved\n"))
            .map_err(|_| "export refusal fixture failed")?;
    }
    let exported = capture.write_new(&target);
    if matches!(mode, Mode::ExportFailure) {
        if exported.as_ref().err().map(std::io::Error::kind)
            != Some(std::io::ErrorKind::AlreadyExists)
        {
            return Err("original export refusal was not preserved".into());
        }
    } else if exported.is_err() {
        return Err("capture export failed after original product cleanup".into());
    }
    Ok((
        physical,
        json!({
            "run": run_index, "mode": mode.name(), "elapsed_ns": elapsed.as_nanos(),
            "authoring": authoring, "game": game, "capture_before_export": report,
            "capture": capture.report(),
            "exported": exported.is_ok(), "export_refusal_after_product_cleanup": matches!(mode, Mode::ExportFailure),
        }),
    ))
}

fn execute(output: PathBuf) -> Result<(), String> {
    if !output.is_absolute() {
        return Err("output must be an explicit absolute new directory".into());
    }
    std::fs::create_dir(&output).map_err(|_| "output directory must be new")?;
    let mut physical = None;
    let mut reports = Vec::new();
    let mut canonical = None;
    let mut authoring_product = None;
    let mut adapter = None;
    for (mode, warmup) in [
        (Mode::Off, true),
        (Mode::On, true),
        (Mode::Off, false),
        (Mode::On, false),
        (Mode::On, false),
        (Mode::Off, false),
        (Mode::Off, false),
        (Mode::On, false),
        (Mode::Overflow, false),
        (Mode::CollectionFailure, false),
        (Mode::ExportFailure, false),
    ] {
        let index = reports.len() + 1;
        eprintln!("diagnostic_capture: run={index} mode={}", mode.name());
        let (event_loop, mut report) = one(mode, index, &output, physical.take())?;
        report["excluded_warmup"] = json!(warmup);
        report["overhead_sample"] = json!(!warmup && matches!(mode, Mode::Off | Mode::On));
        physical = Some(event_loop);
        if canonical
            .as_ref()
            .is_some_and(|expected| expected != &report["game"]["game_state"])
        {
            return Err("capture mode changed complete product state".into());
        }
        canonical.get_or_insert_with(|| report["game"]["game_state"].clone());
        if adapter
            .as_ref()
            .is_some_and(|expected| expected != &report["game"]["adapter"])
        {
            return Err("capture mode changed actual adapter or presented extent".into());
        }
        adapter.get_or_insert_with(|| report["game"]["adapter"].clone());
        if authoring_product
            .as_ref()
            .is_some_and(|expected| expected != &report["authoring"]["product"])
        {
            return Err("capture mode changed original authoring outcomes or state".into());
        }
        authoring_product.get_or_insert_with(|| report["authoring"]["product"].clone());
        reports.push(report);
    }
    drop(physical);
    let result = json!({
        "format": "host-execution-capture/v1", "runs": reports,
        "scope": "concrete scoped Host capture; complete authoring and original paused desktop product, same telemetry in all modes; original App-start-to-completion is not GPU execution/scanout; sequential hardware/code/cache variation, not independent cold starts or tail budgets",
        "product_state_agrees_all_modes": true,
    });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("results.json"))
        .map_err(|_| "create final observation record failed after cleanup")?;
    serde_json::to_writer(&mut file, &result)
        .map_err(|_| "write final observation record failed after cleanup")?;
    file.write_all(b"\n")
        .map_err(|_| "finish final observation record failed after cleanup")?;
    Ok(())
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let result = match (args.next(), args.next()) {
        (Some(output), None) => execute(PathBuf::from(output)),
        _ => Err("usage: diagnostic_capture ABSOLUTE_NEW_OUTPUT_DIRECTORY".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("diagnostic_capture: {error}");
            ExitCode::FAILURE
        }
    }
}
