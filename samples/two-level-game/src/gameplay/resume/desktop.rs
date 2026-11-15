//! Concrete game reconstruction through the original native product and managed command owner.

use std::time::{Duration, Instant};

use nara::{
    app::{RuntimeControl, RuntimeControlStatus, RuntimeState},
    input::KeyboardHold,
    project_host::{DesktopControlError, DesktopRun, DesktopRunProgress, WinitInputActivation},
};

use super::{
    GameResumeError, ResumeControl, capture_desktop_resume, fresh_candidate, restore_command,
};
use crate::{
    GameSnapshot, movement_keyboard_bindings,
    resume::{ResumeState, encode_resume},
};

const POLL: Duration = Duration::from_millis(2);

/// Reconstructs one fresh isolated desktop game without revealing or activating its window.
///
/// The deadline bounds cooperative polling; it cannot interrupt a driver, callback or native API.
/// After time or command admission, a failure may leave applied state: retain this original run
/// for physical close rather than retrying restoration in a partially changed candidate.
/// Successful canonical capture does not prove a restored presentation. The caller must observe
/// its original presentation receipt before adopting the route and revealing this same window.
/// Pending reconstruction explicitly requests full paused frames, including under Wait cadence,
/// so asynchronous Scene results can be integrated by the original App stages.
pub fn restore_desktop_resume(
    run: &mut DesktopRun,
    state: &ResumeState,
    deadline: Instant,
) -> Result<(), GameResumeError> {
    let bytes = encode_resume(state)?;
    check_deadline(deadline)?;
    if run.input_activation() != Some(WinitInputActivation::Isolated) {
        return Err(refuse(
            "desktop restoration requires an isolated native candidate",
        ));
    }
    run.capture(fresh_candidate)??;
    let holds = run.capture_keyboard_holds(&movement_keyboard_bindings())?;
    if holds.iter().any(|hold| hold.held) {
        return Err(refuse(
            "desktop restoration requires an empty native held inventory",
        ));
    }
    run.capture(|world| {
        let control = world.resource::<ResumeControl>();
        if control.configuration != state.compatibility.configuration {
            return Err(refuse(
                "resume configuration is incompatible with the desktop candidate",
            ));
        }
        let registry = nara::reflect::component_registry(world)
            .ok_or_else(|| refuse("candidate Schema is unavailable"))?;
        let schema = registry
            .schema_composition_fingerprint()
            .map_err(|error| refuse(error.to_string()))?;
        if schema.as_bytes() != state.compatibility.schema {
            return Err(refuse(
                "resume Schema is incompatible with the desktop candidate",
            ));
        }
        Ok(())
    })??;
    run.restore_time_continuation(state.runtime_time.to_continuation()?)?;
    paused_maintenance(run, deadline)?;
    check_deadline(deadline)?;
    run.submit(restore_command(bytes)?)?;
    let ticket = run.request_control(RuntimeControl::StepFixedTick)?;
    loop {
        poll(run, deadline)?;
        match run.control_status(ticket) {
            Some(RuntimeControlStatus::Applied) => break,
            Some(RuntimeControlStatus::Pending) => {}
            status => {
                return Err(refuse(format!(
                    "desktop reconstruction Step failed: {status:?}"
                )));
            }
        }
    }
    loop {
        paused_maintenance(run, deadline)?;
        let snapshot = run.capture(|world| world.resource::<GameSnapshot>().clone())?;
        if let Some(error) = snapshot.error {
            return Err(refuse(error));
        }
        if !snapshot.restoring {
            break;
        }
    }
    check_deadline(deadline)?;
    let holds = movement_keyboard_bindings()
        .into_iter()
        .zip(state.held)
        .map(|(binding, held)| KeyboardHold { binding, held })
        .collect();
    run.restore_keyboard_holds(holds)?;
    paused_maintenance(run, deadline)?;
    let actual = capture_desktop_resume(run)?;
    if encode_resume(&actual)? != encode_resume(state)? {
        return Err(refuse(
            "desktop reconstruction differs from the complete canonical state",
        ));
    }
    check_deadline(deadline)
}

fn paused_maintenance(run: &mut DesktopRun, deadline: Instant) -> Result<(), GameResumeError> {
    check_deadline(deadline)?;
    run.request_paused_maintenance()?;
    loop {
        poll(run, deadline)?;
        match run.capture(|_| ()) {
            Ok(()) => return Ok(()),
            Err(DesktopControlError::NotPaused) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn poll(run: &mut DesktopRun, deadline: Instant) -> Result<(), GameResumeError> {
    check_deadline(deadline)?;
    match run.poll(POLL.min(deadline.saturating_duration_since(Instant::now()))) {
        DesktopRunProgress::Running if run.state() == Some(RuntimeState::Paused) => {
            check_deadline(deadline)
        }
        progress => Err(refuse(format!(
            "desktop candidate stopped during reconstruction: {progress:?}"
        ))),
    }
}

fn check_deadline(deadline: Instant) -> Result<(), GameResumeError> {
    if Instant::now() >= deadline {
        return Err(refuse(
            "desktop reconstruction reached its cooperative deadline",
        ));
    }
    Ok(())
}

fn refuse(message: impl Into<String>) -> GameResumeError {
    GameResumeError::Ineligible(message.into())
}
