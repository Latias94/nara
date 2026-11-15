use std::{
    any::Any,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::fs::ContentDigest;
use serde::Serialize;
use serde_json::{Value, json};
use two_level_game::{
    development::{
        ActiveGeneration, ControlServiceObservation, DevelopmentError, DevelopmentHost,
        DevelopmentPlacement, ExecutionState, InputActivation, RetirementRole,
        protocol::{DesktopPresentation, SemanticCommand},
    },
    levels::{LevelId, LevelKey},
    resume::{ResumeState, encode_resume},
};
use windows_sys::Win32::{
    Foundation::FILETIME,
    System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
};

use crate::acceptance::{assert_observation, expected_held_tick, movement, walk_to_score};

pub const PAIRS: usize = 3;
const IDLE: Duration = Duration::from_millis(500);
const REQUEST_LIMIT: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Periodic,
    Notification,
}

pub fn nanos(duration: Duration) -> u64 {
    duration.as_nanos().try_into().unwrap()
}

pub fn timestamp() -> u64 {
    nanos(SystemTime::now().duration_since(UNIX_EPOCH).unwrap())
}

pub fn panic_message(panic: &(dyn Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_owned()
    } else {
        "non-string panic during the original public Host workload".to_owned()
    }
}

pub fn generation(active: ActiveGeneration) -> Value {
    json!({
        "sequence": active.sequence, "pid": active.pid,
        "executable_digest": format!("{:?}", active.executable),
        "provenance_digest": format!("{:?}", active.provenance),
    })
}

pub fn result_json<T: Serialize>(result: &Result<T, DevelopmentError>) -> Value {
    match result {
        Ok(value) => json!({ "Ok": value }),
        Err(error) => json!({ "Err": {
            "stage": format!("{:?}", error.stage), "message": error.message,
            "display": error.to_string(), "debug": format!("{error:?}"),
        } }),
    }
}

#[derive(Serialize)]
pub struct RequestRecord {
    operation: &'static str,
    started_unix_ns: u64,
    elapsed_ns: u64,
    result: Value,
    control_service: Option<ControlServiceObservation>,
}

#[derive(Serialize)]
pub struct RunRecord {
    ordinal: usize,
    pair: usize,
    position: usize,
    placement: DevelopmentPlacement,
    variant: Variant,
    started_unix_ns: u64,
    pub startup: Value,
    pub active: Option<Value>,
    setup_capture_diagnostics: Vec<ControlServiceObservation>,
    requests: Vec<RequestRecord>,
    idle: Option<IdleRecord>,
    pub before_close: Option<ControlServiceObservation>,
    cleanup: Value,
    pub failures: Vec<String>,
}

impl RunRecord {
    pub fn new(
        ordinal: usize,
        pair: usize,
        position: usize,
        placement: DevelopmentPlacement,
        variant: Variant,
    ) -> Self {
        Self {
            ordinal,
            pair,
            position,
            placement,
            variant,
            started_unix_ns: timestamp(),
            startup: Value::Null,
            active: None,
            setup_capture_diagnostics: Vec::new(),
            requests: Vec::new(),
            idle: None,
            before_close: None,
            cleanup: Value::Null,
            failures: Vec::new(),
        }
    }

    pub fn finish_cleanup(&mut self, host: &DevelopmentHost, elapsed_ns: u64, complete: bool) {
        let mut retirements = Vec::new();
        for retired in host.retirements() {
            let native = retired.native;
            let clean_native = native.exit().success()
                && native.containment_empty()
                && native.readers_stopped()
                && native.output_drained()
                && !native.termination_requested()
                && !native.force_killed();
            if !clean_native || !retired.game_closed || retired.terminated {
                self.failures.push(format!(
                    "original game/native cleanup was not normal: {retired:?}"
                ));
            }
            if retired.role != RetirementRole::Active
                || self.active.as_ref() != Some(&generation(retired.generation))
            {
                self.failures.push(format!(
                    "retirement did not name this exact active child: {retired:?}"
                ));
            }
            match retired.control_service {
                Some(observed)
                    if observed.watcher_started
                        && observed.watcher_joined
                        && !observed.counters_saturated
                        && observed.native_wake_delivery_failures == 0 => {}
                observed => self.failures.push(format!(
                    "original watcher did not join cleanly: {observed:?}"
                )),
            }
            retirements.push(json!({
                "generation": generation(retired.generation), "role": format!("{:?}", retired.role),
                "game_closed": retired.game_closed, "terminated": retired.terminated,
                "control_service": retired.control_service, "native_debug": format!("{native:?}"),
                "native_exit": format!("{:?}", native.exit()), "exit_success": native.exit().success(),
                "containment_empty": native.containment_empty(), "readers_stopped": native.readers_stopped(),
                "output_drained": native.output_drained(), "termination_requested": native.termination_requested(),
                "force_killed": native.force_killed(),
            }));
        }
        if retirements.len() != 1
            || host.active().is_some()
            || host.control_service().is_some()
            || host.cleanup_error().is_some()
        {
            self.failures.push(
                "close must retire one original child and clear the active-only observation"
                    .to_owned(),
            );
        }
        self.cleanup = json!({
            "complete": complete,
            "elapsed_ns": elapsed_ns, "retirements": retirements,
            "active_after_close": host.active().map(generation),
            "control_service_after_close": host.control_service(), "error": host.cleanup_error(),
        });
    }
}

fn request<T: Serialize>(
    host: &mut DevelopmentHost,
    record: &mut RunRecord,
    operation: &'static str,
    action: impl FnOnce(&mut DevelopmentHost) -> Result<T, DevelopmentError>,
) -> T {
    let started_unix_ns = timestamp();
    let started = Instant::now();
    let result = action(host);
    let elapsed_ns = nanos(started.elapsed());
    let control_service = host.control_service();
    record.requests.push(RequestRecord {
        operation,
        started_unix_ns,
        elapsed_ns,
        result: result_json(&result),
        control_service,
    });
    result.unwrap_or_else(|error| {
        panic!("{operation} returned its recorded original result: {error:?}")
    })
}

#[derive(Serialize)]
struct CounterDelta {
    inbox_checks: u64,
    idle_wait_calls: u64,
    idle_wait_nanos: u64,
    hints: u64,
    callbacks: u64,
    coalesced_hints: u64,
    native_wake_delivery_failures: u64,
}

impl CounterDelta {
    fn between(before: ControlServiceObservation, after: ControlServiceObservation) -> Self {
        let difference = |before: u64, after: u64| {
            after
                .checked_sub(before)
                .expect("cumulative diagnostics from this exact original child must not regress")
        };
        Self {
            inbox_checks: difference(before.inbox_checks, after.inbox_checks),
            idle_wait_calls: difference(before.idle_wait_calls, after.idle_wait_calls),
            idle_wait_nanos: difference(before.idle_wait_nanos, after.idle_wait_nanos),
            hints: difference(before.hints, after.hints),
            callbacks: difference(before.callbacks, after.callbacks),
            coalesced_hints: difference(before.coalesced_hints, after.coalesced_hints),
            native_wake_delivery_failures: difference(
                before.native_wake_delivery_failures,
                after.native_wake_delivery_failures,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
struct CpuSnapshot {
    creation_filetime_100ns: u64,
    kernel_ns: u64,
    user_ns: u64,
}

struct ProcessCpu(OwnedHandle);

impl ProcessCpu {
    fn open(active: ActiveGeneration) -> Result<Self, String> {
        // SAFETY: The observed PID requests query-only access without inheriting a handle.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, active.pid) };
        if handle.is_null() {
            return Err(format!(
                "OpenProcess for observed active child {}: {}",
                active.pid,
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: OpenProcess returned one new non-null owned handle, transferred exactly once.
        Ok(Self(unsafe { OwnedHandle::from_raw_handle(handle) }))
    }

    fn capture(&self) -> Result<CpuSnapshot, String> {
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        // SAFETY: The query-only handle remains owned and all output pointers are valid FILETIMEs.
        if unsafe {
            GetProcessTimes(
                self.0.as_raw_handle(),
                &mut created,
                &mut exited,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return Err(format!(
                "GetProcessTimes for the original observed handle: {}",
                std::io::Error::last_os_error()
            ));
        }
        let ticks = |value: FILETIME| {
            (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
        };
        let nanos = |value| {
            ticks(value)
                .checked_mul(100)
                .ok_or_else(|| "observed process CPU counter exceeds u64 nanoseconds".to_owned())
        };
        Ok(CpuSnapshot {
            creation_filetime_100ns: ticks(created),
            kernel_ns: nanos(kernel)?,
            user_ns: nanos(user)?,
        })
    }
}

#[derive(Serialize)]
struct IdleRecord {
    started_unix_ns: u64,
    requested_sleep_ns: u64,
    sleep_wall_ns: u64,
    cpu_observation_wall_ns: u64,
    before_state: ResumeState,
    after_state: ResumeState,
    before_canonical_bytes: Vec<u8>,
    after_canonical_bytes: Vec<u8>,
    before: ControlServiceObservation,
    after: ControlServiceObservation,
    delta: CounterDelta,
    cpu_before: Result<CpuSnapshot, String>,
    cpu_after: Result<CpuSnapshot, String>,
    cpu_kernel_delta_ns: Option<u64>,
    cpu_user_delta_ns: Option<u64>,
}

fn check_presented(presentation: &DesktopPresentation, expected: &ResumeState) {
    assert_eq!(
        presentation.state_digest,
        *ContentDigest::of_bytes(&encode_resume(expected).unwrap()).as_bytes()
    );
    assert!(
        [
            presentation.runtime_generation,
            presentation.app_frame,
            presentation.render_frame,
            presentation.window,
            presentation.surface_generation,
            presentation.backend_instance,
            presentation.device_epoch,
        ]
        .into_iter()
        .all(|value| value > 0),
        "{presentation:?}"
    );
    assert!(matches!(
        presentation.adapter_backend.as_str(),
        "dx12" | "vulkan"
    ));
    assert!(!presentation.adapter_type.is_empty());
    assert_ne!(presentation.adapter_type, "cpu");
}

pub fn exercise(host: &mut DevelopmentHost, record: &mut RunRecord) {
    host.isolate_active_input().unwrap();
    walk_to_score(host, 10);
    record
        .setup_capture_diagnostics
        .push(host.control_service().unwrap());
    host.step(vec![movement(false)]).unwrap();
    host.step(vec![SemanticCommand::Marker]).unwrap();
    host.step(vec![SemanticCommand::Travel]).unwrap();
    walk_to_score(host, 20);
    record
        .setup_capture_diagnostics
        .push(host.control_service().unwrap());
    host.step(vec![SemanticCommand::Marker]).unwrap();
    let basis = request(host, record, "basis_capture", DevelopmentHost::capture);
    assert_eq!(
        basis.progress.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    assert_eq!(basis.progress.score, 20);
    assert_eq!(basis.progress.collected.len(), 2);
    assert_eq!(basis.progress.markers.len(), 2);
    for level in [LevelId::Meadow, LevelId::Cavern] {
        assert!(
            basis
                .progress
                .markers
                .iter()
                .any(|marker| marker.level == LevelKey::main(level))
        );
    }
    assert_eq!(basis.counters.travel_count, 1);
    assert_eq!(basis.held, [false, true, false, false]);
    let canonical = encode_resume(&basis).unwrap();
    if record.placement == DevelopmentPlacement::SourceDesktop {
        let presentation = request(
            host,
            record,
            "presentation_before_idle",
            DevelopmentHost::confirm_active_presentation,
        );
        check_presented(&presentation, &basis);
    }
    let cpu = ProcessCpu::open(host.active().unwrap()).unwrap();
    let before_state = request(
        host,
        record,
        "capture_before_idle",
        DevelopmentHost::capture,
    );
    let before = host.control_service().unwrap();
    let before_canonical_bytes = encode_resume(&before_state).unwrap();
    let cpu_started = Instant::now();
    let cpu_before = cpu.capture();
    let started_unix_ns = timestamp();
    let sleeping = Instant::now();
    thread::sleep(IDLE);
    let sleep_wall_ns = nanos(sleeping.elapsed());
    let cpu_after = cpu.capture();
    let cpu_observation_wall_ns = nanos(cpu_started.elapsed());
    let (cpu_kernel_delta_ns, cpu_user_delta_ns) = match (&cpu_before, &cpu_after) {
        (Ok(before), Ok(after)) => (
            after.kernel_ns.checked_sub(before.kernel_ns),
            after.user_ns.checked_sub(before.user_ns),
        ),
        _ => (None, None),
    };
    let after_state = request(host, record, "capture_after_idle", DevelopmentHost::capture);
    let after = host.control_service().unwrap();
    let after_canonical_bytes = encode_resume(&after_state).unwrap();
    record.idle = Some(IdleRecord {
        started_unix_ns,
        requested_sleep_ns: nanos(IDLE),
        sleep_wall_ns,
        cpu_observation_wall_ns,
        before_state,
        after_state,
        before_canonical_bytes,
        after_canonical_bytes,
        before,
        after,
        delta: CounterDelta::between(before, after),
        cpu_before,
        cpu_after,
        cpu_kernel_delta_ns,
        cpu_user_delta_ns,
    });
    let idle = record.idle.as_ref().unwrap();
    assert_eq!(idle.before_canonical_bytes, canonical);
    assert_eq!(idle.after_canonical_bytes, canonical);
    assert_eq!(idle.after_state.clock.ticks, idle.before_state.clock.ticks);
    let maintenance = request(host, record, "frame", DevelopmentHost::frame);
    assert_observation(&maintenance, &basis);
    let captured = request(
        host,
        record,
        "capture_after_frame",
        DevelopmentHost::capture,
    );
    assert_eq!(encode_resume(&captured).unwrap(), canonical);
    let again = request(host, record, "frame_again", DevelopmentHost::frame);
    assert_observation(&again, &basis);
    assert_eq!(again.runtime_generation, maintenance.runtime_generation);
    assert_eq!(again.engine_tick, maintenance.engine_tick);
    assert!(again.app_frame > maintenance.app_frame);
    if record.placement == DevelopmentPlacement::SourceHeadless {
        assert_eq!(
            again.app_frame,
            maintenance.app_frame.checked_add(1).unwrap()
        );
    }
    let captured = request(
        host,
        record,
        "capture_after_second_frame",
        DevelopmentHost::capture,
    );
    assert_eq!(encode_resume(&captured).unwrap(), canonical);
    let stepped = request(host, record, "step", |host| host.step(Vec::new()));
    let expected = expected_held_tick(&basis, 160.0);
    assert_observation(&stepped, &expected);
    let captured = request(host, record, "capture_after_step", DevelopmentHost::capture);
    assert_eq!(captured.clock.ticks, basis.clock.ticks + 1);
    assert_eq!(
        encode_resume(&captured).unwrap(),
        encode_resume(&expected).unwrap()
    );
    let isolated = request(
        host,
        record,
        "isolate_input",
        DevelopmentHost::isolate_active_input,
    );
    assert_observation(&isolated, &expected);
    let captured = request(
        host,
        record,
        "capture_after_control",
        DevelopmentHost::capture,
    );
    assert_eq!(
        encode_resume(&captured).unwrap(),
        encode_resume(&expected).unwrap()
    );
    if record.placement == DevelopmentPlacement::SourceDesktop {
        let status = request(
            host,
            record,
            "execution_status",
            DevelopmentHost::execution_status,
        );
        assert_eq!(status.state, ExecutionState::Paused);
        assert_eq!(status.input_activation, InputActivation::Isolated);
        assert_observation(&status.observation, &expected);
        let presentation = request(
            host,
            record,
            "presentation_after_step",
            DevelopmentHost::confirm_active_presentation,
        );
        check_presented(&presentation, &expected);
    }
}

pub fn evaluate(runs: &[RunRecord]) -> (Vec<Value>, Vec<String>) {
    let mut failures = Vec::new();
    if runs.len() != PAIRS * 4 {
        failures.push(format!(
            "expected twelve fresh original children, observed {} runs",
            runs.len()
        ));
    }
    for run in runs {
        failures.extend(
            run.failures
                .iter()
                .map(|failure| format!("run {}: {failure}", run.ordinal)),
        );
        if run.requests.len() < 5
            || !run
                .requests
                .iter()
                .any(|request| request.operation == "frame")
            || !run
                .requests
                .iter()
                .any(|request| request.operation == "step")
        {
            failures.push(format!(
                "run {} lacks five ordinary requests including Frame and Step",
                run.ordinal
            ));
        }
        for request in &run.requests {
            if request.elapsed_ns > nanos(REQUEST_LIMIT) {
                failures.push(format!(
                    "run {} {} took {} ns, exceeding the finite 100 ms gate",
                    run.ordinal, request.operation, request.elapsed_ns
                ));
            }
        }
        for observation in run
            .setup_capture_diagnostics
            .iter()
            .copied()
            .chain(
                run.requests
                    .iter()
                    .filter_map(|request| request.control_service),
            )
            .chain(run.before_close)
        {
            if !observation.watcher_started
                || observation.watcher_joined
                || observation.counters_saturated
                || observation.native_wake_delivery_failures != 0
            {
                failures.push(format!(
                    "run {} has invalid active control-service diagnostics: {observation:?}",
                    run.ordinal
                ));
            }
        }
        if run
            .requests
            .iter()
            .any(|request| request.control_service.is_none())
        {
            failures.push(format!(
                "run {} omitted last validated active-response diagnostics",
                run.ordinal
            ));
        }
        if run.before_close.is_none() {
            failures.push(format!(
                "run {} lost the active observation before Close",
                run.ordinal
            ));
        }
        match &run.idle {
            Some(idle) => {
                if idle.sleep_wall_ns < nanos(IDLE) || idle.delta.idle_wait_calls == 0 {
                    failures.push(format!(
                        "run {} did not complete its original 500 ms idle wait",
                        run.ordinal
                    ));
                }
                match (&idle.cpu_before, &idle.cpu_after) {
                    (Ok(before), Ok(after)) if before.creation_filetime_100ns == after.creation_filetime_100ns
                        && before.kernel_ns <= after.kernel_ns && before.user_ns <= after.user_ns => {}
                    observed => failures.push(format!("run {} lacks monotonic CPU observations of one original handle: {observed:?}", run.ordinal)),
                }
            }
            None => failures.push(format!(
                "run {} lacks its complete idle observation",
                run.ordinal
            )),
        }
    }
    let mut summaries = Vec::new();
    for placement in [
        DevelopmentPlacement::SourceHeadless,
        DevelopmentPlacement::SourceDesktop,
    ] {
        let mut totals = [0_u128; 2];
        let mut counts = [0; 2];
        let mut canonical: Option<&[u8]> = None;
        for run in runs.iter().filter(|run| run.placement == placement) {
            let index = usize::from(run.variant == Variant::Notification);
            counts[index] += 1;
            if let Some(idle) = &run.idle {
                totals[index] += u128::from(idle.delta.inbox_checks);
                if canonical
                    .is_some_and(|expected| expected != idle.before_canonical_bytes.as_slice())
                {
                    failures.push(format!(
                        "{placement:?} variants did not execute the same complete game state"
                    ));
                }
                canonical = Some(&idle.before_canonical_bytes);
            }
        }
        let accepted = counts == [PAIRS; 2] && totals[0] > 0 && totals[1] * 10 <= totals[0];
        if !accepted {
            failures.push(format!("{placement:?} requires >=90% fewer aggregate idle inbox checks: periodic={}, notification={}, runs={counts:?}", totals[0], totals[1]));
        }
        summaries.push(json!({
            "placement": placement, "periodic_runs": counts[0], "notification_runs": counts[1],
            "periodic_idle_inbox_checks": u64::try_from(totals[0]).ok(),
            "notification_idle_inbox_checks": u64::try_from(totals[1]).ok(),
            "periodic_idle_inbox_checks_exact_u128": totals[0].to_string(),
            "notification_idle_inbox_checks_exact_u128": totals[1].to_string(),
            "idle_inbox_check_reduction_percent": (totals[0] > 0).then(|| 100.0 * (1.0 - totals[1] as f64 / totals[0] as f64)),
            "aggregate_check_gate_accepted": accepted,
        }));
    }
    (summaries, failures)
}

pub fn method() -> Value {
    json!({
        "pairs_per_placement": PAIRS, "order": "periodic/notification, notification/periodic, periodic/notification",
        "game_processes": PAIRS * 4, "warmup_game_processes": 0,
        "requested_idle_ns_per_run": nanos(IDLE), "ordinary_request_limit_ns": nanos(REQUEST_LIMIT),
        "child_intervals_ms": { "periodic": 2, "notification": 250 },
        "variant_difference": "one exact IDLE_RECONCILE_INTERVAL constant in each owned copied continuity.rs; both use actual native notify watchers",
        "build": "original Fixture, public build_executable, dev profile, pinned 1.95.0, offline, one shared root target, CARGO_BUILD_JOBS=1",
        "counter_window": "last validated Capture response through the next validated Capture response; includes endpoint requests and parent response observation overhead",
        "parent_response_poll_ms": 10, "parent_response_poll_changed": false,
        "latency_window": "public Host call entry through its returned original Result after startup; startup and gameplay preparation are separately excluded",
        "cpu": "GetProcessTimes on one RAII-owned query-only OpenProcess handle from the active generation PID; raw cumulative user/kernel observations around only the idle pause",
        "cpu_limit": "Windows CPU accounting is quantized; no CPU reduction, power or stable tail claim",
        "state": "exact encode_resume bytes across idle and explicit paused maintenance; one ordinary Step must advance exactly one logical tick",
        "frame": "two explicit public Frame calls advance the original FrameExecutionStart sequence (exactly one in quiescent headless), preserve engine/logical ticks and complete canonical state; no synthetic service counter",
        "desktop": "input isolated, no user input, original hardware confirm_active_presentation; receipt is submit/present evidence, not scanout or GPU completion",
        "cleanup": "normal original game Close, final validated watcher join diagnostics, independent native cleanup receipt, then original Fixture removal before create_new output",
        "scope": "finite cooperative acceptance on this idle Windows desktop; synchronous callbacks, drivers and native calls are not interruptible hard real-time deadlines",
        "gain_scope": "aggregate child idle inbox-check reduction; unchanged 10 ms parent response polling prevents an inferred end-to-end latency gain",
    })
}
