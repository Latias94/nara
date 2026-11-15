//! Bounded known-paused cadence comparison through the retained original desktop owner.
//!
//! Canonical capture is game-owned state, not evidence of a new native presentation. Presentation
//! counts come only from the Collector's independently validated unique original receipts. Settle,
//! maintenance, exact Step and resumed gameplay stay outside the paired idle intervals. The
//! observer may still wake every 2 ms under Wait; process CPU counters are quantized observations,
//! not a stable CPU, GPU, power or background-task scheduling claim.

use std::time::{Duration, Instant};

use nara::{
    app::{FixedTime, RuntimeControl, RuntimeState},
    project_host::{DesktopControlError, WinitInputActivation},
    winit::WinitControlFlow,
};
use serde_json::{Value, json};
use two_level_game::{GameSnapshot, capture_desktop_resume, resume::encode_resume};

use super::{Experiment, IDLE, POLL, TIMEOUT, ns, process_cost};

const SETTLE: Duration = Duration::from_millis(100);

impl Experiment {
    pub(super) fn measure_paused_cadence(&mut self, run_index: usize) -> Result<Value, String> {
        let order = if run_index % 2 == 0 {
            [WinitControlFlow::Poll, WinitControlFlow::Wait]
        } else {
            [WinitControlFlow::Wait, WinitControlFlow::Poll]
        };
        let mut poll = None;
        let mut wait = None;
        for policy in order {
            let interval = self.paused_interval(policy)?;
            match policy {
                WinitControlFlow::Poll => poll = Some(interval),
                WinitControlFlow::Wait => wait = Some(interval),
            }
        }
        let controls = self.validate_paused_controls()?;
        Ok(json!({
            "order": order.map(|policy| format!("{policy:?}")),
            "order_definition": "Poll/Wait on odd numbered runs, Wait/Poll on even numbered runs, independently of plain/witness order",
            "poll": poll.ok_or("missing paused Poll interval")?,
            "wait": wait.ok_or("missing paused Wait interval")?,
            "controls": controls,
        }))
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
    ) -> Result<(Duration, u64), String> {
        self.require_paused_cadence(policy)?;
        let started = Instant::now();
        let end = started + duration;
        let mut polls = 0_u64;
        while Instant::now() < end {
            self.poll(end + POLL)?;
            polls += 1;
            self.require_paused_cadence(policy)?;
        }
        Ok((started.elapsed(), polls))
    }

    fn paused_interval(&mut self, policy: WinitControlFlow) -> Result<Value, String> {
        let switch_state = capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let switch_bytes = encode_resume(&switch_state).map_err(|error| error.to_string())?;
        let switch_counters = self.counters()?;
        let switch_presentations = self.observed.borrow().presentation_count();
        self.run()
            .set_control_flow(policy)
            .map_err(|error| error.to_string())?;
        let (settle_wall, settle_polls) = self.poll_paused_for(SETTLE, policy)?;
        let before_state = capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let before_bytes = encode_resume(&before_state).map_err(|error| error.to_string())?;
        let before = self.counters()?;
        let before_presentations = self.observed.borrow().presentation_count();
        if before_bytes != switch_bytes
            || before.1 != switch_counters.1
            || before.2 != switch_counters.2
        {
            return Err("cadence switch or settling changed complete original paused state".into());
        }

        let cpu_before = process_cost::capture()?;
        let (wall, polls) = self.poll_paused_for(IDLE, policy)?;
        let cpu_after = process_cost::capture()?;
        let after = self.counters()?;
        let after_presentations = self.observed.borrow().presentation_count();
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
        Ok(json!({
            "policy": format!("{policy:?}"),
            "requested_ns": ns(IDLE)?,
            "wall_ns": ns(wall)?,
            "poll_calls": polls,
            "app_frames": after.0.checked_sub(before.0).ok_or("idle App frame counter regressed")?,
            "simulation_ticks": after.1 - before.1,
            "logical_ticks": after.2 - before.2,
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
            "settle": {
                "requested_ns": ns(SETTLE)?,
                "wall_ns": ns(settle_wall)?,
                "poll_calls": settle_polls,
                "app_frames": before.0.checked_sub(switch_counters.0).ok_or("settle App frame counter regressed")?,
                "presentations": before_presentations.checked_sub(switch_presentations).ok_or("settle presentation counter regressed")?,
                "exact_encode_resume_unchanged": true,
                "scope": "cooperative original polling after policy selection, excluded from idle and CPU samples; not a guarantee that all future native demand is absent",
            },
            "process_cost": {
                "scope": "whole-process counters around idle polling, including observer/caller/background/driver CPU and counter sampling overhead; excludes policy settling, canonical captures and subsequent controls; wall_ns covers the polling loop, not the counter calls",
                "user_ns": user_ns,
                "kernel_ns": kernel_ns,
                "total_cpu_ns": user_ns.checked_add(kernel_ns).ok_or("idle process CPU total overflowed")?,
                "limitation": "quantized Windows process counters; 15.625 ms increments were observed in the prior report; no stable CPU reduction claim or GPU execution/power measurement",
            },
        }))
    }

    fn validate_paused_controls(&mut self) -> Result<Value, String> {
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
            "scope": "after both idle intervals; original Wait maintenance, one original StepFixedTick then full paused maintenance, followed by Poll Resume/Pause; excluded from running samples and cadence totals",
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

/// Applies the frame-reduction gate to all completed pairs, independently of CPU quantization.
pub(super) fn summarize(reports: &[Value]) -> Result<Value, String> {
    let mut poll_frames = 0_u64;
    let mut wait_frames = 0_u64;
    for report in reports {
        for (policy, total) in [("poll", &mut poll_frames), ("wait", &mut wait_frames)] {
            let frames = report["paused_cadence"][policy]["app_frames"]
                .as_u64()
                .ok_or("missing original paused App frame observation")?;
            *total = total
                .checked_add(frames)
                .ok_or("aggregate paused App frames overflowed")?;
        }
    }
    let accepted = poll_frames > 0 && u128::from(wait_frames) * 100 <= u128::from(poll_frames) * 20;
    Ok(json!({
        "completed_pairs": reports.len(),
        "poll_app_frames": poll_frames,
        "wait_app_frames": wait_frames,
        "app_frame_reduction_percent": (poll_frames > 0).then(|| 100.0 * (1.0 - wait_frames as f64 / poll_frames as f64)),
        "minimum_app_frame_reduction_percent": 80,
        "accepted": accepted,
        "scope": "sum of App-frame deltas across all paired 300 ms intervals after excluded settles, both recipes; actual cooperative wall spans remain in each interval",
        "observer_maintenance_poll_interval_ns": ns(POLL)?,
        "cpu_claim": "raw quantized process counters only; no stable CPU or power reduction claim",
    }))
}
