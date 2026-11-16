//! Actual gameplay and scene-lifecycle cost observations through the ordinary native Runtime.
//!
//! Three finite Travel scripts retain movement, pickups, markers, reconstruction and original
//! presentation/retirement. Samples are descriptive run-level observations, not a production tail
//! estimate. Native calls and callbacks retain cooperative deadlines.

#![deny(unsafe_op_in_unsafe_fn)]

#[path = "runtime_workload/observe.rs"]
mod observe;
#[path = "runtime_cost/process_cost.rs"]
mod process_cost;
#[path = "runtime_workload/timing.rs"]
mod timing;

use std::{
    cell::{Cell, RefCell},
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    process::ExitCode,
    rc::Rc,
    time::{Duration, Instant},
};

use nara::{
    app::AppExit,
    fs::ContentDigest,
    project_host::{
        DesktopPresentationControl, DesktopRun, DesktopRunIntent, DesktopRunOptions,
        DesktopRunProgress, DesktopStartupPhase, DesktopStartupTimings, WinitEventLoop,
    },
    render_wgpu::WgpuBackendTelemetryConfig,
    window::{WindowId, backend::WindowTargetPhase},
};
use serde_json::{Value, json};
use two_level_game::{RunOptions, Scenario};

use observe::Collector;

const RUNS: usize = 3;
const GAMEPLAY_TIMEOUT: Duration = Duration::from_secs(60);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(2);

fn main() -> ExitCode {
    let fail_observer = match arguments() {
        Ok(fail_observer) => fail_observer,
        Err(error) => {
            eprintln!("runtime_workload: {error}");
            return ExitCode::FAILURE;
        }
    };
    let report = execute(fail_observer);
    let success = report["accepted"] == true;
    if let Err(error) = write_report(&report) {
        eprintln!("runtime_workload: write report: {error}");
        return ExitCode::FAILURE;
    }
    if success {
        println!(
            "RUNTIME_WORKLOAD_OK runs=3 gameplay_verified=true telemetry_complete=true original_native_retired=true"
        );
        ExitCode::SUCCESS
    } else {
        eprintln!("runtime_workload: acceptance failed: {}", report["failure"]);
        ExitCode::FAILURE
    }
}

fn arguments() -> Result<bool, String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let fail_observer = match args.as_slice() {
        [] => false,
        [flag] if flag == "--fail-observer" => true,
        _ => return Err("usage: runtime_workload [--fail-observer] (set NARA_RUNTIME_WORKLOAD_OUTPUT to an absolute new file)".into()),
    };
    if !cfg!(windows) {
        return Err(
            "this measurement requires the qualified Windows native process counters".into(),
        );
    }
    if std::env::var("WGPU_BACKEND").as_deref() != Ok("dx12")
        || std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() != Ok("0")
    {
        return Err("select WGPU_BACKEND=dx12 and NARA_WGPU_FORCE_FALLBACK=0".into());
    }
    Ok(fail_observer)
}

fn write_report(report: &Value) -> Result<(), String> {
    let mut text = serde_json::to_vec(report).map_err(|error| error.to_string())?;
    text.push(b'\n');
    if let Some(path) = std::env::var_os("NARA_RUNTIME_WORKLOAD_OUTPUT") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("NARA_RUNTIME_WORKLOAD_OUTPUT must be absolute".into());
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| error.to_string())?;
        file.write_all(&text).map_err(|error| error.to_string())
    } else {
        std::io::stdout()
            .write_all(&text)
            .map_err(|error| error.to_string())
    }
}

fn execute(fail_observer: bool) -> Value {
    let mut event_loop = None;
    let mut runs = Vec::with_capacity(RUNS);
    let mut failure = None;
    let mut adapter = None;
    for index in 0..RUNS {
        eprintln!("runtime_workload: run={} starting", index + 1);
        let attempt = run_one(event_loop.take(), index + 1, fail_observer);
        match attempt {
            Ok((physical, report)) => {
                event_loop = Some(physical);
                if adapter
                    .as_ref()
                    .is_some_and(|expected| expected != &report["adapter"])
                {
                    failure = Some("adapter changed between complete runs".to_owned());
                }
                adapter.get_or_insert_with(|| report["adapter"].clone());
                runs.push(report);
            }
            Err(report) => {
                failure = Some(report.to_string());
                runs.push(report);
            }
        }
        if failure.is_some() {
            break;
        }
    }
    drop(event_loop);
    json!({
        "format": "two-level-active-runtime-workload/v1",
        "accepted": failure.is_none() && runs.len() == RUNS,
        "failure": failure,
        "injected_observer_failure": fail_observer,
        "method": {
            "runs": RUNS,
            "scenario": "ordinary Scenario::Travel semantic-command script",
            "native_input": "isolated; game script supplies original semantic commands",
            "manual_delta_or_step": false,
            "gameplay_deadline_ms": GAMEPLAY_TIMEOUT.as_millis(),
            "cleanup_deadline_ms": CLEANUP_TIMEOUT.as_millis(),
            "minimum_initialized_presentations": 20,
            "maximum_retained_frames": 4096,
            "native_poll_wait_cap_ms": POLL.as_millis(),
            "fresh_runtime_window_renderer_per_run": true,
            "runtime_phase_wall_observation": true,
            "startup_operation_observation": true,
            "recovered_original_physical_event_loop": true,
            "shared_process_code_driver_filesystem_caches": true,
            "frame_wall_scope": "exact original App frame start to matching post-Host observation; includes native, driver and submission waits, excludes the observer work after that timestamp",
            "completion_scope": "original App frame start to queue completion callback observation; not GPU execution or scanout",
            "statistics_scope": "per-run finite correlated samples; descriptive nearest-rank quantiles, not a production P99, cold-start comparison or speedup",
            "cpu_memory_scope": "whole process with observer and telemetry enabled; CPU delta and point memory include all threads and driver work; not per-frame CPU, retained-owner bytes or a leak bound",
            "performance_pass_threshold": null,
        },
        "provenance": {
            "target_os": std::env::consts::OS,
            "target_arch": std::env::consts::ARCH,
            "package": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
            "release_assertions_disabled": !cfg!(debug_assertions),
            "desktop": cfg!(feature = "desktop"),
            "cost_measurement": cfg!(feature = "cost-measurement"),
            "measurement_source": format!("{:?}", ContentDigest::of_bytes(include_bytes!("runtime_workload.rs"))),
            "observer_source": format!("{:?}", ContentDigest::of_bytes(include_bytes!("runtime_workload/observe.rs"))),
            "gameplay_script_source": format!("{:?}", ContentDigest::of_bytes(include_bytes!("../gameplay/verification.rs"))),
            "process_counter_source": format!("{:?}", ContentDigest::of_bytes(include_bytes!("runtime_cost/process_cost.rs"))),
        },
        "runs": runs,
    })
}

struct Experiment {
    run: Option<DesktopRun>,
    collector: Rc<RefCell<Collector>>,
    observer_panicked: Rc<Cell<bool>>,
    startup: Option<DesktopStartupTimings>,
}

impl Experiment {
    fn run(&mut self) -> &mut DesktopRun {
        self.run.as_mut().expect("original DesktopRun is retained")
    }

    fn measure(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + GAMEPLAY_TIMEOUT;
        loop {
            if self.observer_panicked.get() {
                return Err("registered observer panicked inside its protected boundary".into());
            }
            if let Some(error) = self.collector.borrow().error() {
                return Err(error.to_owned());
            }
            if Instant::now() >= deadline {
                return Err("complete gameplay exceeded its cooperative deadline".into());
            }
            if self.collector.borrow().completed() {
                return Ok(());
            }
            let progress = self.run().poll(POLL);
            if self.startup.is_none() {
                self.startup = self.run().startup_timings().filter(|timings| {
                    timings
                        .get(DesktopStartupPhase::NativePollingToFirstPresentation)
                        .is_some()
                });
            }
            if Instant::now() >= deadline {
                return Err("gameplay poll returned after its cooperative deadline".into());
            }
            if self.observer_panicked.get() {
                return Err("registered observer panicked inside its protected boundary".into());
            }
            if progress != DesktopRunProgress::Running {
                return Err(format!(
                    "ordinary gameplay stopped before complete verified presentation: {progress:?}"
                ));
            }
        }
    }

    fn retire(&mut self, success: bool) -> Result<WinitEventLoop, String> {
        if success {
            self.collector.borrow_mut().request_seal();
        } else {
            self.run().request_close();
        }
        let deadline = Instant::now() + CLEANUP_TIMEOUT;
        let mut retries = 0;
        loop {
            if Instant::now() >= deadline {
                return Err("original native/runtime retirement remains pending".into());
            }
            let progress = self.run().poll(POLL);
            if Instant::now() >= deadline {
                return Err(
                    "original retirement poll returned after its cooperative deadline".into(),
                );
            }
            match progress {
                DesktopRunProgress::Completed(exit) if !success || exit == AppExit::Success => {
                    break;
                }
                DesktopRunProgress::Running | DesktopRunProgress::Retiring => {}
                DesktopRunProgress::CleanupIncomplete if retries < 2 => {
                    retries += 1;
                    self.run().execute();
                }
                progress => {
                    return Err(format!(
                        "original retirement did not complete normally: {progress:?}"
                    ));
                }
            }
        }
        if let Some(target) = self.collector.borrow().target() {
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
                    "original native target remains retained: {target:?}"
                ));
            }
        } else if success {
            return Err("no original native target was observed".into());
        }
        if success && !self.collector.borrow().seal_complete() {
            return Err("original telemetry did not complete before retirement".into());
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
            eprintln!(
                "runtime_workload: original unresolved native owner retained until process exit"
            );
        }
    }
}

fn run_one(
    event_loop: Option<WinitEventLoop>,
    index: usize,
    fail_observer: bool,
) -> Result<(WinitEventLoop, Value), Value> {
    let entry_started_at = Instant::now();
    let root = two_level_game::open_project_root()
        .map_err(|error| json!({"failure": error.to_string()}))?;
    let recipe = two_level_game::configured_recipe(RunOptions {
        scenario: Scenario::Travel,
        save_path: None,
    });
    let collector = Rc::new(RefCell::new(Collector::new(index, entry_started_at)));
    let observer = Rc::clone(&collector);
    let observer_panicked = Rc::new(Cell::new(false));
    let callback_panicked = Rc::clone(&observer_panicked);
    let mut execution = DesktopRunOptions::new()
        .with_input_isolation(true)
        .with_startup_timings()
        .with_telemetry(WgpuBackendTelemetryConfig::default().with_runtime_frame_timings())
        .with_presentation_observer(move |observation| {
            if callback_panicked.get() {
                return DesktopPresentationControl::Exit(AppExit::Requested);
            }
            let inject = fail_observer && observation.presented_frame().is_some();
            match catch_unwind(AssertUnwindSafe(|| {
                let control = observer.borrow_mut().observe(observation);
                if inject {
                    panic!("injected registered workload observer failure");
                }
                control
            })) {
                Ok(control) => control,
                Err(payload) => {
                    callback_panicked.set(true);
                    std::mem::forget(payload);
                    DesktopPresentationControl::Exit(AppExit::Requested)
                }
            }
        });
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
        collector,
        observer_panicked,
        startup: None,
    };
    let measured = catch_unwind(AssertUnwindSafe(|| experiment.measure()))
        .unwrap_or_else(|_| Err("gameplay observer or native callback panicked".into()));
    let startup = experiment.startup;
    let closed = catch_unwind(AssertUnwindSafe(|| experiment.retire(measured.is_ok())))
        .unwrap_or_else(|_| Err("original cleanup panicked".into()));
    let physical = match closed {
        Ok(physical) => physical,
        Err(error) => {
            experiment.run().request_close();
            let retry = catch_unwind(AssertUnwindSafe(|| experiment.retire(false)))
                .unwrap_or_else(|_| Err("original cleanup retry panicked".into()));
            let cleanup = match retry {
                Ok(physical) => {
                    drop(physical);
                    "completed on explicit retry".to_owned()
                }
                Err(error) => format!("unresolved original custody: {error}"),
            };
            return Err(json!({ "run": index, "failure": measured.err(),
                "retirement_failure": error, "cleanup": cleanup }));
        }
    };
    if let Err(error) = measured {
        drop(physical);
        return Err(json!({ "run": index, "failure": error,
            "cleanup": { "native_destroyed": true, "physical_event_loop_recovered": true } }));
    }
    let report = experiment.collector.borrow().finish();
    let mut report = match report {
        Ok(report) => report,
        Err(error) => {
            drop(physical);
            return Err(json!({ "run": index, "failure": error,
                "cleanup": { "native_destroyed": true, "physical_event_loop_recovered": true } }));
        }
    };
    report["cleanup"] = json!({ "normal_exit": true, "telemetry_complete": true,
        "native_destroyed": true, "surface_retired": true, "provider_released": true,
        "physical_event_loop_recovered": true });
    let report =
        timing::finish_startup_report(report, startup, experiment.run().startup_timings())?;
    Ok((physical, report))
}
