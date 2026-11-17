//! Finite ordinary-versus-development-witness desktop comparison.
//!
//! Both recipes use the original continuous Runtime and native presentation owner. The reported
//! frame span includes App and native Host/submission wall time, including waits; it is neither
//! CPU execution time nor GPU execution or scanout time. Telemetry is enabled equally in both
//! recipes. Run on an interactive Windows desktop without keyboard input or window manipulation.
//! Poll deadlines are cooperative and cannot interrupt native calls or game callbacks.
//! Startup diagnostics observe original elapsed operations before running-frame warmup. Sequential
//! runs share this process and recover the same physical event loop; they are not cold processes.
//!
//! Format v4 retains running/startup evidence and compares three 300 ms paused intervals after
//! 100 ms cooperative settling: Poll, Wait with explicit periodic 2 ms observer deadlines, and
//! Wait without an observer deadline. Order alternates by run. Complete original `encode_resume`
//! bytes, unique original presentation receipts, actual observer callbacks, native polls,
//! App/fixed/logical counters and quantized whole-process CPU remain separate observations.
//! Idle callers allow the remaining interval as native wait; other controls retain short polling.
//! A separate absolute 100 ms observer deadline allows a 1 s caller wait and must be observed by
//! 250 ms without changing original App, presentation or canonical state. Separate controls retain
//! exact Step, full Wait maintenance, Resume/Pause and final telemetry/native retirement.
//! Acceptance requires at least 80% fewer aggregate Wait App frames than Poll and 90% fewer
//! callbacks and native polls without a deadline than with periodic observer deadlines. These
//! observations do not establish stable CPU savings, power use or a universal presentation policy.

#![deny(unsafe_op_in_unsafe_fn)]

#[path = "runtime_cost/idle.rs"]
mod idle;
#[path = "runtime_cost/process_cost.rs"]
mod process_cost;
#[path = "runtime_cost/mod.rs"]
mod runtime_cost;

use std::{
    cell::RefCell,
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
    process::ExitCode,
    rc::Rc,
    time::{Duration, Instant},
};

use nara::{
    app::{
        AppExit, FixedTime, FrameExecutionStart, RuntimeControl, RuntimeControlStatus, RuntimeState,
    },
    input::KeyboardHold,
    project_host::{
        DesktopControlError, DesktopRun, DesktopRunIntent, DesktopRunOptions, DesktopRunProgress,
        DesktopStartupPhase, DesktopStartupTimings, DesktopTelemetrySealOutcome, WinitEventLoop,
        WinitInputActivation,
    },
    render_wgpu::WgpuBackendTelemetryConfig,
    window::{WindowId, backend::WindowTargetPhase},
};
use serde_json::{Value, json};
use two_level_game::{
    GamePhase, GameSnapshot, capture_desktop_resume,
    development::presentation::DevelopmentPresentationPlugin, movement_keyboard_bindings,
};

use runtime_cost::{Collector, ns};

const TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(2);
const IDLE: Duration = Duration::from_millis(300);

#[derive(Clone, Copy)]
struct Options {
    pairs: usize,
    frames: usize,
    warmup: usize,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut options = Self {
            pairs: 3,
            frames: 120,
            warmup: 30,
        };
        let mut seen = [false; 3];
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            let (index, target, maximum) =
                match flag.as_str() {
                    "--pairs" => (0, &mut options.pairs, 6),
                    "--frames" => (1, &mut options.frames, 1_000),
                    "--warmup" => (2, &mut options.warmup, 300),
                    _ => return Err(
                        "usage: runtime_cost [--pairs 1..6] [--frames 1..1000] [--warmup 1..300]"
                            .into(),
                    ),
                };
            if seen[index] {
                return Err(format!("duplicate option {flag}"));
            }
            seen[index] = true;
            *target = args
                .next()
                .ok_or_else(|| format!("missing value for {flag}"))?
                .parse()
                .map_err(|_| format!("invalid integer for {flag}"))?;
            if *target == 0 || *target > maximum {
                return Err(format!("{flag} must be in 1..={maximum}"));
            }
        }
        if !cfg!(target_os = "windows") {
            return Err(
                "runtime_cost currently requires Windows native keyboard activation".into(),
            );
        }
        Ok(options)
    }
}

fn main() -> ExitCode {
    let result = Options::parse().and_then(compare);
    match result {
        Ok(report) => match write_report(&report) {
            Ok(()) if report["paused_cadence"]["accepted"] == true => ExitCode::SUCCESS,
            Ok(()) => {
                eprintln!(
                    "runtime_cost: paused cadence or observer deadline gate failed; observations retained in the report"
                );
                ExitCode::FAILURE
            }
            Err(error) => {
                eprintln!("runtime_cost: write report: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("runtime_cost: {error}");
            ExitCode::FAILURE
        }
    }
}

fn write_report(report: &Value) -> Result<(), String> {
    let text = serde_json::to_string(report).map_err(|error| error.to_string())?;
    if let Some(path) = std::env::var_os("NARA_RUNTIME_COST_OUTPUT") {
        let path = std::path::PathBuf::from(path);
        if !path.is_absolute() {
            return Err("NARA_RUNTIME_COST_OUTPUT must be an explicit absolute new file".into());
        }
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("create runtime cost result: {error}"))?;
        output
            .write_all(text.as_bytes())
            .and_then(|()| output.write_all(b"\n"))
            .map_err(|error| format!("write runtime cost result: {error}"))?;
    } else {
        println!("{text}");
    }
    Ok(())
}

fn compare(options: Options) -> Result<Value, String> {
    let mut event_loop = None;
    let mut reports = Vec::with_capacity(options.pairs * 2);
    let mut workload = None;
    let mut adapter = None;
    for pair in 0..options.pairs {
        let order = if pair % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        };
        for witness in order {
            eprintln!(
                "runtime_cost: pair={} recipe={} starting",
                pair + 1,
                if witness { "witness" } else { "plain" }
            );
            let (physical, mut report) =
                run_one(event_loop.take(), witness, options, reports.len())?;
            event_loop = Some(physical);
            if workload
                .as_ref()
                .is_some_and(|expected| expected != &report["workload"])
                || adapter
                    .as_ref()
                    .is_some_and(|expected| expected != &report["adapter"])
            {
                return Err(
                    "sequential runs changed semantic workload, adapter, or surface extent".into(),
                );
            }
            workload.get_or_insert_with(|| report["workload"].clone());
            adapter.get_or_insert_with(|| report["adapter"].clone());
            report["pair"] = json!(pair + 1);
            report["run"] = json!(reports.len() + 1);
            reports.push(report);
        }
    }
    drop(event_loop);
    let paused_cadence = idle::summarize(&reports)?;
    Ok(json!({
        "format": "two-level-runtime-cost/v4",
        "pairs": options.pairs,
        "warmup_accepted_frames": options.warmup,
        "sample_accepted_frames": options.frames,
        "frame_wall_definition": "FrameExecutionStart.started_at to post-Host observation.now; App plus native Host/submit wall including waits, not CPU execution/GPU execution/scanout",
        "gpu_completion_definition": "original App frame start to queue-completion callback observation, not GPU execution/scanout",
        "telemetry_enabled_both": true,
        "startup_timings_enabled_both": true,
        "startup_sampling_definition": "additive v2 diagnostics; each run has new original Runtime/native owners in one process and recovers the same physical event loop; loaded code and driver caches are shared, not independent cold-process starts",
        "running_manual_step_or_delta": false,
        "paused_interval_manual_step_or_delta": false,
        "paused_control_validation_fixed_steps_per_run": 1,
        "same_physical_event_loop": true,
        "same_workload_adapter_extent": true,
        "paused_cadence": paused_cadence,
        "runs": reports,
    }))
}

struct Experiment {
    run: Option<DesktopRun>,
    observed: Rc<RefCell<Collector>>,
    startup: Option<DesktopStartupTimings>,
}

impl Experiment {
    fn run(&mut self) -> &mut DesktopRun {
        self.run
            .as_mut()
            .expect("original DesktopRun remains retained")
    }

    fn poll(&mut self, deadline: Instant) -> Result<(), String> {
        self.poll_with_wait_cap(deadline, POLL).map(|_| ())
    }

    fn poll_with_wait_cap(
        &mut self,
        deadline: Instant,
        max_wait: Duration,
    ) -> Result<Duration, String> {
        if Instant::now() >= deadline {
            return Err("original desktop phase exceeded its cooperative deadline".into());
        }
        if let Some(error) = &self.observed.borrow().failure {
            return Err(error.clone());
        }
        let started = Instant::now();
        let progress = self
            .run()
            .poll(max_wait.min(deadline.saturating_duration_since(started)));
        let wall = started.elapsed();
        if progress != DesktopRunProgress::Running {
            return Err(format!(
                "original desktop stopped outside retirement: {progress:?}"
            ));
        }
        Ok(wall)
    }

    fn settle(&mut self, deadline: Instant) -> Result<GameSnapshot, String> {
        loop {
            self.poll(deadline)?;
            match self
                .run()
                .capture(|world| world.resource::<GameSnapshot>().clone())
            {
                Ok(snapshot) => {
                    if let Some(error) = &snapshot.error {
                        return Err(error.clone());
                    }
                    if snapshot.initialized
                        && snapshot.phase == GamePhase::Running
                        && !snapshot.restoring
                    {
                        return Ok(snapshot);
                    }
                    self.run()
                        .request_paused_maintenance()
                        .map_err(|error| error.to_string())?;
                }
                Err(DesktopControlError::NotPaused) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    fn maintenance(&mut self, deadline: Instant) -> Result<GameSnapshot, String> {
        self.run()
            .request_paused_maintenance()
            .map_err(|error| error.to_string())?;
        self.settle(deadline)
    }

    fn control(&mut self, control: RuntimeControl) -> Result<(), String> {
        let expected = match control {
            RuntimeControl::Pause => RuntimeState::Paused,
            RuntimeControl::Resume => RuntimeState::Running,
            RuntimeControl::StepFixedTick => RuntimeState::Paused,
            _ => return Err("cost comparison admits only Pause, Resume and StepFixedTick".into()),
        };
        let ticket = self
            .run()
            .request_control(control)
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            self.poll(deadline)?;
            match self.run().control_status(ticket) {
                Some(RuntimeControlStatus::Applied) => break,
                Some(RuntimeControlStatus::Pending) => {}
                status => return Err(format!("original control did not apply: {status:?}")),
            }
        }
        if self.run().state() != Some(expected) {
            return Err("applied control disagrees with original Runtime state".into());
        }
        Ok(())
    }

    fn measure(&mut self, options: Options, run_index: usize) -> Result<Value, String> {
        let deadline = Instant::now() + TIMEOUT;
        let initial_settle_started = Instant::now();
        self.settle(deadline)?;
        let initial_settle_wall = initial_settle_started.elapsed();
        let initial_startup = self
            .run()
            .startup_timings()
            .ok_or("original desktop startup timing was not enabled")?;
        let neutral_started = Instant::now();
        let bindings = movement_keyboard_bindings(self.run()).map_err(|error| error.to_string())?;
        let holds = bindings
            .into_iter()
            .map(|binding| KeyboardHold {
                binding,
                held: false,
            })
            .collect();
        self.run()
            .restore_keyboard_holds(holds)
            .map_err(|error| error.to_string())?;
        let baseline = self.maintenance(deadline)?;
        let neutral_hold_and_maintenance_wall = neutral_started.elapsed();
        let canonical = capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        if canonical.held != [false; 4] || baseline.tick != 0 || canonical.clock.ticks != 0 {
            return Err("initial neutral paused workload was not fresh".into());
        }
        let startup = self
            .run()
            .startup_timings()
            .ok_or("original desktop startup timing was not retained")?;
        for phase in DesktopStartupPhase::ALL {
            let completed = startup
                .get(phase)
                .ok_or("neutral source desktop is missing an original startup observation")?;
            if initial_startup
                .get(phase)
                .is_some_and(|initial| initial != completed)
            {
                return Err("neutral maintenance replaced a completed startup observation".into());
            }
        }
        self.startup = Some(startup);
        self.verify_startup_timings()?;
        let phases = DesktopStartupPhase::ALL
            .into_iter()
            .map(|phase| {
                let scope = match phase {
                    DesktopStartupPhase::ContentPreparation => {
                        "original source check, plan creation and begin-start admission"
                    }
                    DesktopStartupPhase::RuntimeStartup => {
                        "complete original Runtime startup attempt before native transfer"
                    }
                    DesktopStartupPhase::NativeSessionConstruction => {
                        "initial Pause request, presentation driver and original WinitSession construction"
                    }
                    DesktopStartupPhase::FirstNativePoll => {
                        "first nonblocking native poll, including window, device and render work it performs"
                    }
                    DesktopStartupPhase::NativePollingToFirstPresentation => {
                        "sum of original native poll calls through first actual presentation, including the remainder of that poll and excluding caller gaps; overlaps FirstNativePoll"
                    }
                };
                Ok(json!({
                    "phase": format!("{phase:?}"),
                    "optional_ns": startup.get(phase).map(ns).transpose()?,
                    "scope": scope,
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let startup_report = json!({
            "scope": "caller-observed original operation elapsed wall time, including existing waits; not raw CPU or GPU execution, scanout, or an exact total startup partition",
            "observation_point": "completed original snapshot copied after neutral keyboard inventory, paused maintenance and canonical capture; earlier completed phases are unchanged from initial initialized paused settle; before reveal, input activation, Resume and running-frame warmup",
            "missing_phase_definition": "null optional_ns means the phase was not reached or observed; zero is a measured completed duration",
            "outcome_authority": "diagnostic snapshot only; original progress, complete paused capture, presentation and physical retirement APIs remain authoritative",
            "phases": phases,
            "public_call_boundaries": {
                "initial_settle_ns": ns(initial_settle_wall)?,
                "neutral_hold_and_maintenance_ns": ns(neutral_hold_and_maintenance_wall)?,
                "scope": "initial settle includes original cooperative poll/capture/maintenance until initialized paused state; the separate neutral interval includes restore_keyboard_holds and paused maintenance, excluding later canonical capture; public spans overlap the internal phases and must not be summed with them",
            },
        });
        self.observed.borrow_mut().baseline = Some(baseline);
        self.run().reveal().map_err(|error| error.to_string())?;
        self.maintenance(deadline)?;
        self.run()
            .request_input_activation()
            .map_err(|error| error.to_string())?;
        loop {
            if let Some(error) = self.run().input_activation_error() {
                return Err(error.to_string());
            }
            match self.run().input_activation() {
                Some(WinitInputActivation::Active) => {
                    self.settle(deadline)?;
                    break;
                }
                Some(WinitInputActivation::AwaitingFocus) => self.poll(deadline)?,
                state => {
                    return Err(format!(
                        "neutral activation requires external intervention: {state:?}"
                    ));
                }
            }
        }
        self.control(RuntimeControl::Resume)?;
        self.observed.borrow_mut().collect_running = true;
        let deadline = Instant::now() + TIMEOUT;
        let mut polls = 0_u64;
        while self.observed.borrow().sample_count() < options.frames {
            self.poll(deadline)?;
            polls += 1;
            if self.run().input_activation() != Some(WinitInputActivation::Active) {
                return Err("native input activation changed during the no-input workload".into());
            }
        }
        self.observed.borrow_mut().collect_running = false;
        self.control(RuntimeControl::Pause)?;
        self.settle(Instant::now() + TIMEOUT)?;
        let paused_cadence = self.measure_paused_cadence(run_index)?;
        let final_state = capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        if final_state.held != [false; 4]
            || final_state.player != canonical.player
            || final_state.progress != canonical.progress
            || final_state.counters != canonical.counters
        {
            return Err("no-input game changed its semantic workload".into());
        }
        self.verify_startup_timings()?;
        let mut report = self.observed.borrow().report()?;
        report["startup"] = startup_report;
        report["workload"]["schema"] = json!(canonical.compatibility.schema);
        report["workload"]["level_stamps"] = json!(canonical.compatibility.levels);
        report["workload"]["timestep_nanos"] = json!(canonical.clock.timestep_nanos);
        report["workload"]["runtime_time_policy"] = json!(canonical.runtime_time);
        report["recipe_configuration"] = json!(canonical.compatibility.configuration);
        report["running_poll_calls"] = json!(polls);
        report["paused_cadence"] = paused_cadence;
        Ok(report)
    }

    fn verify_startup_timings(&mut self) -> Result<(), String> {
        let expected = self
            .startup
            .ok_or("missing original startup timing snapshot")?;
        if self.run().startup_timings() != Some(expected) {
            return Err(
                "later desktop operations changed the original startup timing snapshot".into(),
            );
        }
        Ok(())
    }

    fn counters(&mut self) -> Result<(u64, u64, u64), String> {
        self.run()
            .capture(|world| {
                (
                    world.resource::<FrameExecutionStart>().frame(),
                    world.resource::<FixedTime>().tick(),
                    world.resource::<GameSnapshot>().logical_tick,
                )
            })
            .map_err(|error| error.to_string())
    }

    fn retire(&mut self, success: bool) -> Result<WinitEventLoop, String> {
        if success {
            self.observed.borrow_mut().seal_requested = true;
            self.run()
                .request_paused_maintenance()
                .map_err(|error| error.to_string())?;
        } else {
            self.run().request_close();
        }
        let deadline = Instant::now() + TIMEOUT;
        let mut retries = 0;
        loop {
            if Instant::now() >= deadline {
                return Err("native cleanup remains pending".into());
            }
            match self.run().poll(POLL) {
                DesktopRunProgress::Completed(exit) if !success || exit == AppExit::Success => {
                    break;
                }
                DesktopRunProgress::Retiring | DesktopRunProgress::Running => {}
                DesktopRunProgress::CleanupIncomplete if retries < 2 => {
                    retries += 1;
                    self.run().execute();
                }
                progress => {
                    return Err(format!(
                        "original cleanup did not complete normally: {progress:?}"
                    ));
                }
            }
        }
        let target = self.observed.borrow().target.clone();
        if let Some(target) = target {
            let target = target
                .snapshot(WindowId::PRIMARY)
                .map_err(|error| error.to_string())?;
            if target.phase != WindowTargetPhase::NativeDestroyed
                || target.fault.is_some()
                || target.surface_active
                || target.provider_present
                || !target.native_destroyed
            {
                return Err(format!(
                    "original native target is not physically retired: {target:?}"
                ));
            }
        } else if success {
            return Err("successful run did not observe the original native registry".into());
        }
        if success {
            let observed = self.observed.borrow();
            if observed.seal_outcome != Some(DesktopTelemetrySealOutcome::Complete) {
                return Err(format!(
                    "telemetry seal was not complete: {:?}",
                    observed.seal_outcome
                ));
            }
            if let Some(error) = &observed.failure {
                return Err(error.clone());
            }
            observed.validate_completions()?;
        }
        self.run()
            .take_event_loop()
            .ok_or_else(|| "original physical event loop was not recovered".into())
    }
}

impl Drop for Experiment {
    fn drop(&mut self) {
        if let Some(run) = self.run.take()
            && !matches!(
                run.progress(),
                DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
            )
        {
            std::mem::forget(run);
            eprintln!("runtime_cost: original live DesktopRun retained until process exit");
        }
    }
}

fn run_one(
    event_loop: Option<WinitEventLoop>,
    witness: bool,
    options: Options,
    run_index: usize,
) -> Result<(WinitEventLoop, Value), String> {
    let root = two_level_game::open_project_root().map_err(|error| error.to_string())?;
    let recipe = two_level_game::product_recipe();
    let recipe = if witness {
        recipe
            .add_plugin::<DevelopmentPresentationPlugin>()
            .map_err(|error| error.to_string())?
    } else {
        recipe
    };
    let observed = Rc::new(RefCell::new(Collector::new(
        witness,
        options.warmup,
        options.frames,
    )));
    let observer = Rc::clone(&observed);
    let mut execution = DesktopRunOptions::new()
        .with_startup_timings()
        .with_initial_pause()
        .with_initial_visibility(false)
        .with_input_isolation(true)
        .with_telemetry(WgpuBackendTelemetryConfig::default())
        .with_presentation_observer(move |observation| observer.borrow_mut().observe(observation));
    if let Some(event_loop) = event_loop {
        execution = execution.with_event_loop(event_loop);
    }
    let mut experiment = Experiment {
        run: Some(DesktopRun::new(
            root,
            DesktopRunIntent::new()
                .with_profile("desktop")
                .with_recipe(recipe)
                .with_execution(execution),
        )),
        observed,
        startup: None,
    };
    let measured = catch_unwind(AssertUnwindSafe(|| experiment.measure(options, run_index)))
        .unwrap_or_else(|_| Err("measurement panicked; closing original desktop owners".into()));
    let cleanup = catch_unwind(AssertUnwindSafe(|| experiment.retire(measured.is_ok())))
        .unwrap_or_else(|_| Err("native cleanup panicked; retaining original live owners".into()));
    let event_loop = match cleanup {
        Ok(event_loop) => event_loop,
        Err(error) => {
            let fallback = catch_unwind(AssertUnwindSafe(|| experiment.retire(false)))
                .unwrap_or_else(|_| Err("explicit cleanup retry panicked".into()));
            let cleanup_error = match fallback {
                Ok(event_loop) => {
                    drop(event_loop);
                    "explicit close completed".to_owned()
                }
                Err(cleanup) => format!("explicit close retained: {cleanup}"),
            };
            let measurement_error = measured
                .as_ref()
                .err()
                .map(String::as_str)
                .unwrap_or("measurement complete");
            return Err(format!("{error}; {cleanup_error}; {measurement_error}"));
        }
    };
    let mut report = measured?;
    experiment.verify_startup_timings()?;
    report["startup"]["snapshot_unchanged_through_cleanup"] = json!(true);
    experiment.observed.borrow().add_completions(&mut report)?;
    report["cleanup"] = json!({ "normal_exit": true, "telemetry_sealed": true,
        "native_destroyed": true, "surface_retired": true, "provider_released": true,
        "physical_event_loop_recovered": true });
    Ok((event_loop, report))
}
