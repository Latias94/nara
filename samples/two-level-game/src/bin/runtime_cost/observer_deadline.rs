use std::time::{Duration, Instant};

use nara::winit::WinitControlFlow;
use serde_json::{Value, json};
use two_level_game::{capture_desktop_resume, resume::encode_resume};

use super::{Experiment, ObserverSchedule, SETTLE, TIMEOUT, ns};

const OBSERVER_DELAY: Duration = Duration::from_millis(100);
const OBSERVATION_BOUND: Duration = Duration::from_millis(250);
const CALLER_WAIT: Duration = Duration::from_secs(1);

impl Experiment {
    pub(super) fn validate_observer_deadline(&mut self) -> Result<Value, String> {
        self.observed.borrow_mut().observer_schedule = ObserverSchedule::None;
        self.run()
            .set_control_flow(WinitControlFlow::Wait)
            .map_err(|error| error.to_string())?;
        let switch_state = capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let switch_bytes = encode_resume(&switch_state).map_err(|error| error.to_string())?;
        let switch_counters = self.counters()?;
        let switch_callbacks = self.observed.borrow().observer_callback_count();
        let switch_presentations = self.observed.borrow().presentation_count();
        let settle = self.poll_paused_for(SETTLE, WinitControlFlow::Wait)?;
        let before_state = capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let before_bytes = encode_resume(&before_state).map_err(|error| error.to_string())?;
        let before = self.counters()?;
        let before_callbacks = self.observed.borrow().observer_callback_count();
        let before_presentations = self.observed.borrow().presentation_count();
        let telemetry_before = self.observed.borrow().maintenance_observation();
        if before_bytes != switch_bytes
            || before.1 != switch_counters.1
            || before.2 != switch_counters.2
        {
            return Err("observer deadline settling changed complete original paused state".into());
        }

        let started = Instant::now();
        let at = started + OBSERVER_DELAY;
        let phase_deadline = started + TIMEOUT;
        self.observed.borrow_mut().observer_schedule = ObserverSchedule::Deadline {
            at,
            first_requested_at: None,
            observed_at: None,
        };
        let arming_wall = self.poll_with_wait_cap(phase_deadline, Duration::ZERO)?;
        self.require_paused_cadence(WinitControlFlow::Wait)?;
        let armed_callbacks = self.observed.borrow().observer_callback_count();
        let mut wait_polls = 0_u64;
        let mut wait_poll_wall = Duration::ZERO;
        loop {
            let observed = matches!(
                self.observed.borrow().observer_schedule,
                ObserverSchedule::Deadline {
                    observed_at: Some(_),
                    ..
                }
            );
            if observed || started.elapsed() >= OBSERVATION_BOUND {
                break;
            }
            wait_poll_wall += self.poll_with_wait_cap(phase_deadline, CALLER_WAIT)?;
            wait_polls += 1;
            self.require_paused_cadence(WinitControlFlow::Wait)?;
        }
        let wall = started.elapsed();
        let schedule = self.observed.borrow().observer_schedule;
        self.observed.borrow_mut().observer_schedule = ObserverSchedule::None;
        let ObserverSchedule::Deadline {
            at: requested_deadline,
            first_requested_at,
            observed_at,
        } = schedule
        else {
            return Err("original observer deadline measurement was replaced".into());
        };
        let after = self.counters()?;
        let after_callbacks = self.observed.borrow().observer_callback_count();
        let after_presentations = self.observed.borrow().presentation_count();
        let telemetry_after = self.observed.borrow().maintenance_observation();
        let after_state = capture_desktop_resume(self.run()).map_err(|error| error.to_string())?;
        let after_bytes = encode_resume(&after_state).map_err(|error| error.to_string())?;
        self.require_paused_cadence(WinitControlFlow::Wait)?;

        let requested_before_due = requested_deadline == at
            && first_requested_at.is_some_and(|requested| requested >= started && requested < at);
        let observed_after_due = observed_at.is_some_and(|observed| observed >= at);
        let observed_within_bound =
            observed_at.is_some_and(|observed| observed <= started + OBSERVATION_BOUND);
        let canonical_unchanged = before_bytes == after_bytes;
        let app_frames = after
            .0
            .checked_sub(before.0)
            .ok_or("deadline App frame counter regressed")?;
        let simulation_ticks = after
            .1
            .checked_sub(before.1)
            .ok_or("deadline simulation tick regressed")?;
        let logical_ticks = after
            .2
            .checked_sub(before.2)
            .ok_or("deadline logical tick regressed")?;
        let presentations = after_presentations
            .checked_sub(before_presentations)
            .ok_or("deadline presentation counter regressed")?;
        let accepted = requested_before_due
            && observed_after_due
            && observed_within_bound
            && wait_polls > 0
            && canonical_unchanged
            && app_frames == 0
            && simulation_ticks == 0
            && logical_ticks == 0
            && presentations == 0;
        let Value::Object(mut report) = json!({
            "policy": "Wait",
            "requested_deadline_offset_ns": ns(OBSERVER_DELAY)?,
            "maximum_observation_elapsed_ns": ns(OBSERVATION_BOUND)?,
            "caller_native_wait_cap_ns": ns(CALLER_WAIT)?,
            "cooperative_phase_timeout_ns": ns(TIMEOUT)?,
            "first_request_elapsed_ns": first_requested_at.map(|requested| ns(requested.saturating_duration_since(started))).transpose()?,
            "observation_elapsed_ns": observed_at.map(|observed| ns(observed.saturating_duration_since(started))).transpose()?,
            "observation_lateness_ns": observed_at.map(|observed| ns(observed.saturating_duration_since(at))).transpose()?,
            "requested_before_due": requested_before_due,
            "same_absolute_deadline_before_due": requested_before_due,
            "observed_at_or_after_deadline": observed_after_due,
            "observed_within_cooperative_bound": observed_within_bound,
            "returned_continue_when_due": observed_at.is_some(),
            "wall_ns": ns(wall)?,
            "native_poll_wall_ns": ns(arming_wall + wait_poll_wall)?,
            "poll_calls": wait_polls + 1,
            "wait_poll_calls": wait_polls,
            "wait_native_poll_wall_ns": ns(wait_poll_wall)?,
            "observer_callbacks": after_callbacks.checked_sub(before_callbacks).ok_or("deadline observer callback counter regressed")?,
            "wait_observer_callbacks": after_callbacks.checked_sub(armed_callbacks).ok_or("deadline wait observer callback counter regressed")?,
            "start_observer_callback_count": before_callbacks,
            "end_observer_callback_count": after_callbacks,
        }) else {
            unreachable!("the deadline report literal is an object")
        };
        let Value::Object(state) = json!({
            "arming": {
                "poll_calls": 1,
                "caller_native_wait_cap_ns": 0,
                "native_poll_wall_ns": ns(arming_wall)?,
                "observer_callbacks": armed_callbacks.checked_sub(before_callbacks).ok_or("deadline arming observer callback counter regressed")?,
                "scope": "one original nonblocking native poll publishes the absolute observer deadline before the separate 1 s caller-capped wait; no App maintenance request",
            },
            "app_frames": app_frames,
            "simulation_ticks": simulation_ticks,
            "logical_ticks": logical_ticks,
            "presentations": presentations,
            "start_app_frame": before.0,
            "end_app_frame": after.0,
            "start_simulation_tick": before.1,
            "end_simulation_tick": after.1,
            "start_logical_tick": before.2,
            "end_logical_tick": after.2,
            "start_presentation_count": before_presentations,
            "end_presentation_count": after_presentations,
            "canonical_capture_valid": true,
            "exact_encode_resume_unchanged": canonical_unchanged,
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
                "observer_callbacks": before_callbacks.checked_sub(switch_callbacks).ok_or("deadline settle observer callback counter regressed")?,
                "app_frames": before.0.checked_sub(switch_counters.0).ok_or("deadline settle App frame counter regressed")?,
                "presentations": before_presentations.checked_sub(switch_presentations).ok_or("deadline settle presentation counter regressed")?,
                "exact_encode_resume_unchanged": true,
            },
            "accepted": accepted,
            "scope": "separate original observer deadline control after deadline-free Wait settling; excluded from all idle aggregates and CPU samples; deadline offsets use one local Instant origin, callbacks record original observation.now, poll walls include actual native work and waits",
            "cooperative_bound_definition": "first original callback at or after the absolute 100 ms deadline must have observation.now within 250 ms of arming; native calls and callbacks are not interruptible, actual wall overshoot is retained rather than converted into an interval timeout error",
        }) else {
            unreachable!("the deadline state literal is an object")
        };
        report.extend(state);
        Ok(Value::Object(report))
    }
}
