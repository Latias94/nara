//! Bounded known-paused cadence comparison through the retained original desktop owner.
//!
//! Canonical capture is game-owned state, not evidence of a new native presentation. Presentation
//! counts come only from the Collector's independently validated unique original receipts; actual
//! observer callbacks are counted separately. Settle, deadline validation, maintenance, exact Step
//! and resumed gameplay stay outside the three idle intervals. Periodic Wait requests explicit
//! observer deadlines; deadline-free Wait retains independent original GPU/first-present/seal
//! maintenance. Process CPU counters are quantized observations, not a stable CPU or power claim.

#[path = "observer_deadline.rs"]
mod observer_deadline;

use std::time::{Duration, Instant};

use nara::{
    app::{FixedTime, RuntimeControl, RuntimeState},
    project_host::{DesktopControlError, WinitInputActivation},
    winit::WinitControlFlow,
};
use serde_json::{Value, json};
use two_level_game::{GameSnapshot, capture_desktop_resume, resume::encode_resume};

use super::{Experiment, IDLE, POLL, TIMEOUT, ns, process_cost, runtime_cost::ObserverSchedule};

const SETTLE: Duration = Duration::from_millis(100);

#[derive(Clone, Copy)]
enum IdleCondition {
    Poll,
    PeriodicWait,
    DeadlineFreeWait,
}

impl IdleCondition {
    const ALL: [Self; 3] = [Self::Poll, Self::PeriodicWait, Self::DeadlineFreeWait];

    fn name(self) -> &'static str {
        match self {
            Self::Poll => "poll",
            Self::PeriodicWait => "wait_periodic_observer",
            Self::DeadlineFreeWait => "wait_no_observer_deadline",
        }
    }

    fn policy(self) -> WinitControlFlow {
        match self {
            Self::Poll => WinitControlFlow::Poll,
            Self::PeriodicWait | Self::DeadlineFreeWait => WinitControlFlow::Wait,
        }
    }

    fn observer_period(self) -> Option<Duration> {
        match self {
            Self::PeriodicWait => Some(POLL),
            Self::Poll | Self::DeadlineFreeWait => None,
        }
    }
}

struct PollSpan {
    wall: Duration,
    native_poll_wall: Duration,
    calls: u64,
    min_wait_cap: Option<Duration>,
    max_wait_cap: Duration,
}

impl Experiment {
    pub(super) fn measure_paused_cadence(&mut self, run_index: usize) -> Result<Value, String> {
        let mut order = IdleCondition::ALL;
        if run_index % 2 != 0 {
            order.reverse();
        }
        let mut report = json!({
            "order": order.map(IdleCondition::name),
            "order_definition": "Poll, periodic-observer Wait, deadline-free Wait on odd numbered runs; reverse on even numbered runs, independently of plain/witness order",
        });
        for condition in order {
            report[condition.name()] = self.paused_interval(condition)?;
        }
        report["observer_deadline"] = self.validate_observer_deadline()?;
        report["controls"] = self.validate_paused_controls()?;
        Ok(report)
    }

    fn require_paused_cadence(&mut self, policy: WinitControlFlow) -> Result<(), String> {
        if self.run().state() != Some(RuntimeState::Paused)
            || self.run().control_flow() != Some(policy)
            || self.run().input_activation() != Some(WinitInputActivation::Active)
        {
            return Err("paused cadence, Runtime state or native input activation changed".into());
        }
        Ok(())
    }

    fn poll_paused_for(
        &mut self,
        duration: Duration,
        policy: WinitControlFlow,
    ) -> Result<PollSpan, String> {
        self.require_paused_cadence(policy)?;
        let started = Instant::now();
        let end = started + duration;
        let phase_deadline = started + TIMEOUT;
        let mut span = PollSpan {
            wall: Duration::ZERO,
            native_poll_wall: Duration::ZERO,
            calls: 0,
            min_wait_cap: None,
            max_wait_cap: Duration::ZERO,
        };
        loop {
            let remaining = end.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            span.native_poll_wall += self.poll_with_wait_cap(phase_deadline, remaining)?;
            span.calls += 1;
            span.min_wait_cap = Some(
                span.min_wait_cap
                    .map_or(remaining, |cap| cap.min(remaining)),
            );
            span.max_wait_cap = span.max_wait_cap.max(remaining);
            self.require_paused_cadence(policy)?;
        }
        span.wall = started.elapsed();
        Ok(span)
    }

    fn paused_interval(&mut self, condition: IdleCondition) -> Result<Value, String> {
        let policy = condition.policy();
        let switch_state = capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let switch_bytes = encode_resume(&switch_state).map_err(|error| error.to_string())?;
        let switch_counters = self.counters()?;
        let switch_presentations = self.observed.borrow().presentation_count();
        let switch_callbacks = self.observed.borrow().observer_callback_count();
        self.observed.borrow_mut().observer_schedule = condition
            .observer_period()
            .map_or(ObserverSchedule::None, ObserverSchedule::Periodic);
        self.run()
            .set_control_flow(policy)
            .map_err(|error| error.to_string())?;
        let settle = self.poll_paused_for(SETTLE, policy)?;
        let before_state = capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let before_bytes = encode_resume(&before_state).map_err(|error| error.to_string())?;
        let before = self.counters()?;
        let before_presentations = self.observed.borrow().presentation_count();
        let before_callbacks = self.observed.borrow().observer_callback_count();
        let telemetry_before = self.observed.borrow().maintenance_observation();
        if before_bytes != switch_bytes
            || before.1 != switch_counters.1
            || before.2 != switch_counters.2
        {
            return Err("cadence switch or settling changed complete original paused state".into());
        }

        let cpu_before = process_cost::capture()?;
        let idle = self.poll_paused_for(IDLE, policy)?;
        let cpu_after = process_cost::capture()?;
        let after = self.counters()?;
        let after_presentations = self.observed.borrow().presentation_count();
        let after_callbacks = self.observed.borrow().observer_callback_count();
        let telemetry_after = self.observed.borrow().maintenance_observation();
        let after_state = capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let after_bytes = encode_resume(&after_state).map_err(|error| error.to_string())?;
        if before_bytes != after_bytes || before.1 != after.1 || before.2 != after.2 {
            return Err(format!(
                "paused {policy:?} idle changed complete canonical bytes or simulation/logical ticks"
            ));
        }
        self.require_paused_cadence(policy)?;
        let user_ns = cpu_after
            .user_ns
            .checked_sub(cpu_before.user_ns)
            .ok_or("idle process user CPU counter regressed")?;
        let kernel_ns = cpu_after
            .kernel_ns
            .checked_sub(cpu_before.kernel_ns)
            .ok_or("idle process kernel CPU counter regressed")?;
        let Value::Object(mut report) = json!({
            "condition": condition.name(),
            "policy": format!("{policy:?}"),
            "explicit_observer_period_ns": condition.observer_period().map(ns).transpose()?,
            "caller_native_wait_cap_definition": "remaining requested interval on each original poll; no independent 2 ms caller cap",
            "caller_native_wait_cap_min_ns": idle.min_wait_cap.map(ns).transpose()?,
            "caller_native_wait_cap_max_ns": ns(idle.max_wait_cap)?,
            "cooperative_phase_timeout_ns": ns(TIMEOUT)?,
            "requested_ns": ns(IDLE)?,
            "wall_ns": ns(idle.wall)?,
            "native_poll_wall_ns": ns(idle.native_poll_wall)?,
            "poll_calls": idle.calls,
            "observer_callbacks": after_callbacks.checked_sub(before_callbacks).ok_or("idle observer callback counter regressed")?,
            "start_observer_callback_count": before_callbacks,
            "end_observer_callback_count": after_callbacks,
            "observer_callback_definition": "every invocation of the original presentation observer, including maintenance and repeated/no presentation receipts; independent of unique presentations and native poll calls",
            "app_frames": after.0.checked_sub(before.0).ok_or("idle App frame counter regressed")?,
            "simulation_ticks": after.1 - before.1,
            "logical_ticks": after.2 - before.2,
        }) else {
            unreachable!("the idle report literal is an object")
        };
        let Value::Object(state) = json!({
            "start_app_frame": before.0,
            "end_app_frame": after.0,
            "start_simulation_tick": before.1,
            "end_simulation_tick": after.1,
            "start_logical_tick": before.2,
            "end_logical_tick": after.2,
            "presentations": after_presentations.checked_sub(before_presentations).ok_or("idle presentation counter regressed")?,
            "start_presentation_count": before_presentations,
            "end_presentation_count": after_presentations,
            "presentation_definition": "unique original successful receipts validated against current App/render/device transaction; not inferred from canonical state, App frames, poll calls, or scanout",
            "canonical_capture_valid": true,
            "exact_encode_resume_unchanged": true,
            "canonical_byte_count": before_bytes.len(),
            "canonical_before": before_state,
            "canonical_after": after_state,
            "telemetry_before": telemetry_before,
            "telemetry_after": telemetry_after,
            "settle": {
                "requested_ns": ns(SETTLE)?,
                "wall_ns": ns(settle.wall)?,
                "native_poll_wall_ns": ns(settle.native_poll_wall)?,
                "poll_calls": settle.calls,
                "caller_native_wait_cap_min_ns": settle.min_wait_cap.map(ns).transpose()?,
                "caller_native_wait_cap_max_ns": ns(settle.max_wait_cap)?,
                "observer_callbacks": before_callbacks.checked_sub(switch_callbacks).ok_or("settle observer callback counter regressed")?,
                "app_frames": before.0.checked_sub(switch_counters.0).ok_or("settle App frame counter regressed")?,
                "presentations": before_presentations.checked_sub(switch_presentations).ok_or("settle presentation counter regressed")?,
                "exact_encode_resume_unchanged": true,
                "scope": "cooperative original polling after policy and observer selection, using the remaining settle interval as native wait cap; excluded from idle and CPU samples; not a guarantee that all future native demand is absent",
            },
            "process_cost": {
                "scope": "whole-process counters around idle polling, including observer/caller/background/driver CPU and counter sampling overhead; excludes policy settling, canonical captures and subsequent controls; wall_ns covers the polling loop, not the counter calls",
                "user_ns": user_ns,
                "kernel_ns": kernel_ns,
                "total_cpu_ns": user_ns.checked_add(kernel_ns).ok_or("idle process CPU total overflowed")?,
                "limitation": "quantized Windows process counters; 15.625 ms increments were observed in the prior report; no stable CPU reduction claim or GPU execution/power measurement",
            },
        }) else {
            unreachable!("the idle state literal is an object")
        };
        report.extend(state);
        Ok(Value::Object(report))
    }

    fn validate_paused_controls(&mut self) -> Result<Value, String> {
        self.observed.borrow_mut().observer_schedule = ObserverSchedule::None;
        self.run()
            .set_control_flow(WinitControlFlow::Wait)
            .map_err(|error| error.to_string())?;
        self.require_paused_cadence(WinitControlFlow::Wait)?;
        let before = self.counters()?;
        let before_state = capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let before_bytes = encode_resume(&before_state).map_err(|error| error.to_string())?;
        let before_presentations = self.observed.borrow().presentation_count();
        self.maintenance(Instant::now() + TIMEOUT)?;
        let maintained = self.counters()?;
        let maintained_state =
            capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let maintained_bytes =
            encode_resume(&maintained_state).map_err(|error| error.to_string())?;
        let maintained_presentations = self.observed.borrow().presentation_count();
        if maintained.0 <= before.0
            || maintained.1 != before.1
            || maintained.2 != before.2
            || maintained_bytes != before_bytes
            || maintained_presentations <= before_presentations
        {
            return Err(
                "original Wait maintenance did not preserve state and present a new frame".into(),
            );
        }

        self.control(RuntimeControl::StepFixedTick)?;
        let capture_eligible_after_step_poll = match self.run().capture(|_| ()) {
            Err(DesktopControlError::NotPaused) => false,
            Ok(()) => true,
            Err(error) => return Err(error.to_string()),
        };
        self.maintenance(Instant::now() + TIMEOUT)?;
        self.require_paused_cadence(WinitControlFlow::Wait)?;
        let stepped = self.counters()?;
        let stepped_state =
            capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let stepped_presentations = self.observed.borrow().presentation_count();
        let mut expected = maintained_state.clone();
        expected.clock.ticks = expected
            .clock
            .ticks
            .checked_add(1)
            .ok_or("logical tick exhausted")?;
        expected.clock.elapsed_nanos = expected
            .clock
            .elapsed_nanos
            .checked_add(expected.clock.timestep_nanos)
            .ok_or("logical elapsed time exhausted")?;
        if stepped.1
            != maintained
                .1
                .checked_add(1)
                .ok_or("simulation tick exhausted")?
            || stepped.2
                != maintained
                    .2
                    .checked_add(1)
                    .ok_or("logical tick exhausted")?
            || encode_resume(&stepped_state).map_err(|error| error.to_string())?
                != encode_resume(&expected).map_err(|error| error.to_string())?
            || stepped_presentations <= maintained_presentations
        {
            return Err("original Step plus full Wait maintenance did not advance exactly one canonical fixed/logical tick and present".into());
        }

        self.run()
            .set_control_flow(WinitControlFlow::Poll)
            .map_err(|error| error.to_string())?;
        self.require_paused_cadence(WinitControlFlow::Poll)?;
        self.control(RuntimeControl::Resume)?;
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if self.run().input_activation() != Some(WinitInputActivation::Active) {
                return Err(
                    "native input activation changed during resumed no-input gameplay".into(),
                );
            }
            let ticks = self
                .run()
                .observe(|world| {
                    (
                        world.resource::<FixedTime>().tick(),
                        world.resource::<GameSnapshot>().logical_tick,
                    )
                })
                .map_err(|error| error.to_string())?;
            if ticks.0 > stepped.1 && ticks.1 > stepped.2 {
                break;
            }
            self.poll(deadline)?;
        }
        self.control(RuntimeControl::Pause)?;
        self.settle(Instant::now() + TIMEOUT)?;
        self.require_paused_cadence(WinitControlFlow::Poll)?;
        let resumed = self.counters()?;
        let resumed_state =
            capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let resumed_presentations = self.observed.borrow().presentation_count();
        let simulation_ticks = resumed
            .1
            .checked_sub(stepped.1)
            .ok_or("resumed simulation tick regressed")?;
        let logical_ticks = resumed
            .2
            .checked_sub(stepped.2)
            .ok_or("resumed logical tick regressed")?;
        if simulation_ticks == 0
            || simulation_ticks != logical_ticks
            || resumed_presentations <= stepped_presentations
        {
            return Err(
                "original Resume/Pause did not continue fixed/logical gameplay and presentation"
                    .into(),
            );
        }
        Ok(json!({
            "scope": "after all three idle intervals and separate observer deadline validation; original Wait maintenance, one original StepFixedTick then full paused maintenance, followed by Poll Resume/Pause; excluded from running samples and cadence totals",
            "maintenance": {
                "policy": "Wait",
                "app_frames": maintained.0 - before.0,
                "simulation_ticks": maintained.1 - before.1,
                "logical_ticks": maintained.2 - before.2,
                "presentations": maintained_presentations - before_presentations,
                "exact_encode_resume_unchanged": true,
            },
            "step_and_full_maintenance": {
                "policy": "Wait",
                "app_frames": stepped.0.checked_sub(maintained.0).ok_or("Step App frame counter regressed")?,
                "simulation_ticks": stepped.1 - maintained.1,
                "logical_ticks": stepped.2 - maintained.2,
                "presentations": stepped_presentations - maintained_presentations,
                "capture_eligible_after_step_poll": capture_eligible_after_step_poll,
                "step_poll_scope": "one native poll may complete the automatic full frame requested after exact Step; explicit full paused maintenance is still requested afterward",
                "explicit_full_paused_maintenance": true,
                "exact_expected_encode_resume": true,
                "canonical_before": maintained_state,
                "canonical_after": stepped_state,
            },
            "resume_pause": {
                "policy": "Poll",
                "app_frames": resumed.0.checked_sub(stepped.0).ok_or("resumed App frame counter regressed")?,
                "simulation_ticks": simulation_ticks,
                "logical_ticks": logical_ticks,
                "presentations": resumed_presentations - stepped_presentations,
                "canonical_after": resumed_state,
                "final_runtime_state": "Paused",
            },
        }))
    }
}

/// Applies frame, callback, native-poll and deadline gates independently of CPU quantization.
pub(super) fn summarize(reports: &[Value]) -> Result<Value, String> {
    let mut frames = [0_u64; 3];
    let mut callbacks = [0_u64; 3];
    let mut polls = [0_u64; 3];
    let mut deadline_controls_accepted = !reports.is_empty();
    for report in reports {
        for (index, condition) in IdleCondition::ALL.into_iter().enumerate() {
            let interval = &report["paused_cadence"][condition.name()];
            for (metric, total) in [
                ("app_frames", &mut frames[index]),
                ("observer_callbacks", &mut callbacks[index]),
                ("poll_calls", &mut polls[index]),
            ] {
                *total = total
                    .checked_add(interval[metric].as_u64().ok_or_else(|| {
                        format!(
                            "missing original paused {} {metric} observation",
                            condition.name()
                        )
                    })?)
                    .ok_or_else(|| format!("aggregate paused {metric} overflowed"))?;
            }
        }
        deadline_controls_accepted &= report["paused_cadence"]["observer_deadline"]["accepted"]
            .as_bool()
            .ok_or("missing original observer deadline control result")?;
    }
    let reduced_by = |baseline: u64, candidate: u64, percent: u64| {
        baseline > 0
            && u128::from(candidate) * 100 <= u128::from(baseline) * u128::from(100 - percent)
    };
    let reduction_percent = |baseline: u64, candidate: u64| {
        (baseline > 0).then(|| 100.0 * (1.0 - candidate as f64 / baseline as f64))
    };
    let periodic_app_frames_accepted = reduced_by(frames[0], frames[1], 80);
    let deadline_free_app_frames_accepted = reduced_by(frames[0], frames[2], 80);
    let observer_callbacks_accepted = reduced_by(callbacks[1], callbacks[2], 90);
    let native_polls_accepted = reduced_by(polls[1], polls[2], 90);
    let mut conditions = json!({});
    for (index, condition) in IdleCondition::ALL.into_iter().enumerate() {
        conditions[condition.name()] = json!({
            "app_frames": frames[index],
            "observer_callbacks": callbacks[index],
            "poll_calls": polls[index],
        });
    }
    Ok(json!({
        "completed_runs": reports.len(),
        "completed_pairs": reports.len() / 2,
        "conditions": conditions,
        "periodic_wait_app_frame_reduction_percent": reduction_percent(frames[0], frames[1]),
        "deadline_free_wait_app_frame_reduction_percent": reduction_percent(frames[0], frames[2]),
        "minimum_app_frame_reduction_percent": 80,
        "periodic_wait_app_frames_accepted": periodic_app_frames_accepted,
        "deadline_free_wait_app_frames_accepted": deadline_free_app_frames_accepted,
        "observer_callback_reduction_percent": reduction_percent(callbacks[1], callbacks[2]),
        "native_poll_call_reduction_percent": reduction_percent(polls[1], polls[2]),
        "minimum_observer_callback_reduction_percent": 90,
        "minimum_native_poll_call_reduction_percent": 90,
        "observer_callbacks_accepted": observer_callbacks_accepted,
        "native_poll_calls_accepted": native_polls_accepted,
        "observer_deadline_controls_accepted": deadline_controls_accepted,
        "accepted": periodic_app_frames_accepted && deadline_free_app_frames_accepted
            && observer_callbacks_accepted && native_polls_accepted && deadline_controls_accepted,
        "scope": "aggregate original App frames, actual observer callbacks and native poll calls across all three 300 ms conditions per original run, both recipes; excludes settles, separate observer deadline validation and gameplay controls; actual cooperative wall spans remain in each condition",
        "explicit_periodic_observer_period_ns": ns(POLL)?,
        "deadline_free_observer_period_ns": null,
        "caller_idle_wait_cap": "remaining requested 300 ms interval on each native poll; 100 ms settles use their remaining interval; no independent 2 ms caller polling",
        "cooperative_phase_timeout_ns": ns(TIMEOUT)?,
        "pending_gpu_maintenance": {
            "owner": "original desktop presentation and WGPU Host",
            "poll_interval_ns": ns(POLL)?,
            "trigger": "unsettled original submission-prefix custody, pending render result or active telemetry seal; independent of observer deadlines",
            "observed_facts": "each interval retains latest original pending completion/resource telemetry counters and cumulative received completion sample counts before/after; telemetry counters alone do not prove native submission custody is settled",
        },
        "other_original_deadlines": "first-present/READY and telemetry seal remain independent owner requests; Continue withdraws only the observer deadline",
        "cpu_claim": "raw quantized process counters only; no stable CPU or power reduction claim",
    }))
}
