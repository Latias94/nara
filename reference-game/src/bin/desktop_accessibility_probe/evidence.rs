use std::sync::{Mutex, MutexGuard};

use nara::ui::{UiActionId, UiActivationSource, advanced::UiKey};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ProbePhase {
    #[default]
    AwaitingTerminalFrame,
    AwaitingRetryNode,
    ArmRequested,
    Armed,
    InvokeReserved,
    InvokeSucceeded,
    ActionObserved,
    RetryApplied,
    SuccessorPresented,
    PlatformVerified,
    Complete,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProbeActionEvidence {
    pub(crate) sequence: u64,
    pub(crate) input_sequence: Option<u64>,
    pub(crate) surface_id: u64,
    pub(crate) presentation_epoch: u64,
    pub(crate) frame_revision: u64,
    pub(crate) target: UiKey,
    pub(crate) action: UiActionId,
    pub(crate) source: UiActivationSource,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProbeTrace {
    pub(crate) phase: ProbePhase,
    pub(crate) window_title: Option<String>,
    pub(crate) hwnd: Option<isize>,
    pub(crate) retry_runtime_id: Option<Vec<i32>>,
    pub(crate) before_wgpu_frame: Option<u64>,
    pub(crate) journal_floor: Option<u64>,
    pub(crate) armed_frame_revision: Option<u64>,
    pub(crate) armed_target: Option<UiKey>,
    pub(crate) action: Option<ProbeActionEvidence>,
    pub(crate) run_generation: Option<u64>,
    pub(crate) after_wgpu_frame: Option<u64>,
    pub(crate) journal_barrier: Option<u64>,
    pub(crate) failure: Option<String>,
    stop_requested: bool,
}

impl ProbeTrace {
    #[must_use]
    pub(crate) fn is_complete(&self) -> bool {
        self.phase == ProbePhase::Complete && self.failure.is_none()
    }

    #[must_use]
    pub(crate) fn has_failed(&self) -> bool {
        self.phase == ProbePhase::Failed
    }

    #[must_use]
    pub(crate) fn should_stop(&self) -> bool {
        self.stop_requested || self.is_complete() || self.has_failed()
    }
}

#[derive(Debug, Default)]
pub(crate) struct ProbeEvidence {
    trace: Mutex<ProbeTrace>,
}

impl ProbeEvidence {
    fn lock(&self) -> MutexGuard<'_, ProbeTrace> {
        self.trace
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn observe_terminal_frame(&self, window_title: &str, frame_index: u64) {
        let mut trace = self.lock();
        if !matches!(
            trace.phase,
            ProbePhase::AwaitingTerminalFrame | ProbePhase::AwaitingRetryNode
        ) {
            return;
        }
        let title_change = trace
            .window_title
            .as_deref()
            .filter(|current| *current != window_title)
            .map(|current| {
                format!(
                    "primary window title changed before Retry invocation: {current:?} -> {window_title:?}"
                )
            });
        if let Some(message) = title_change {
            fail_locked(&mut trace, message);
            return;
        }
        if trace.window_title.is_none() {
            trace.window_title = Some(window_title.to_owned());
        }
        trace.before_wgpu_frame = Some(frame_index);
        trace.phase = ProbePhase::AwaitingRetryNode;
    }

    pub(crate) fn platform_window_title(&self) -> Option<String> {
        let trace = self.lock();
        (trace.phase == ProbePhase::AwaitingRetryNode)
            .then(|| trace.window_title.clone())
            .flatten()
    }

    pub(crate) fn request_arm(
        &self,
        hwnd: isize,
        retry_runtime_id: Vec<i32>,
    ) -> Result<(), String> {
        let mut trace = self.lock();
        if trace.phase != ProbePhase::AwaitingRetryNode {
            let phase = trace.phase;
            return fail_result(
                &mut trace,
                format!("Retry arming was requested from invalid phase {phase:?}"),
            );
        }
        if hwnd == 0 || retry_runtime_id.is_empty() || trace.before_wgpu_frame.is_none() {
            return fail_result(
                &mut trace,
                "Retry invocation lacked exact window, control, or WGPU baseline identity",
            );
        }
        trace.hwnd = Some(hwnd);
        trace.retry_runtime_id = Some(retry_runtime_id);
        trace.phase = ProbePhase::ArmRequested;
        Ok(())
    }

    pub(crate) fn arm_is_requested(&self) -> bool {
        self.lock().phase == ProbePhase::ArmRequested
    }

    pub(crate) fn arm_runtime(
        &self,
        journal_floor: u64,
        frame_revision: u64,
        target: UiKey,
    ) -> Result<(), String> {
        let mut trace = self.lock();
        if trace.phase != ProbePhase::ArmRequested {
            let phase = trace.phase;
            return fail_result(
                &mut trace,
                format!("runtime armed Retry from invalid phase {phase:?}"),
            );
        }
        if frame_revision == 0 {
            return fail_result(
                &mut trace,
                "runtime Retry arming lacked an exact frame revision or target",
            );
        }
        trace.journal_floor = Some(journal_floor);
        trace.armed_frame_revision = Some(frame_revision);
        trace.armed_target = Some(target);
        trace.phase = ProbePhase::Armed;
        Ok(())
    }

    pub(crate) fn is_armed(&self) -> bool {
        self.lock().phase == ProbePhase::Armed
    }

    pub(crate) fn begin_invoke(&self, hwnd: isize, retry_runtime_id: &[i32]) -> Result<(), String> {
        let mut trace = self.lock();
        if trace.phase != ProbePhase::Armed
            || trace.hwnd != Some(hwnd)
            || trace.retry_runtime_id.as_deref() != Some(retry_runtime_id)
        {
            let phase = trace.phase;
            return fail_result(
                &mut trace,
                format!("UI Automation Invoke began without exact armed identity at {phase:?}"),
            );
        }
        trace.phase = ProbePhase::InvokeReserved;
        Ok(())
    }

    pub(crate) fn confirm_invoke(&self) -> Result<(), String> {
        let mut trace = self.lock();
        if trace.phase != ProbePhase::InvokeReserved {
            let phase = trace.phase;
            return fail_result(
                &mut trace,
                format!("UI Automation Invoke returned from invalid phase {phase:?}"),
            );
        }
        trace.phase = if trace.after_wgpu_frame.is_some() {
            ProbePhase::SuccessorPresented
        } else if trace.run_generation.is_some() {
            ProbePhase::RetryApplied
        } else if trace.action.is_some() {
            ProbePhase::ActionObserved
        } else {
            ProbePhase::InvokeSucceeded
        };
        Ok(())
    }

    pub(crate) fn observe_action(&self, action: ProbeActionEvidence) -> Result<(), String> {
        let mut trace = self.lock();
        if !matches!(
            trace.phase,
            ProbePhase::InvokeReserved | ProbePhase::InvokeSucceeded
        ) || trace.action.is_some()
        {
            let phase = trace.phase;
            return fail_result(
                &mut trace,
                format!(
                    "Retry action arrived outside the one-shot Invoke window at phase {phase:?}"
                ),
            );
        }
        if action.sequence == 0
            || trace
                .journal_floor
                .is_none_or(|floor| action.sequence <= floor)
            || action.surface_id != nara_reference_game::REFERENCE_UI_SURFACE_ID
            || action.presentation_epoch == 0
            || action.frame_revision == 0
            || trace
                .armed_frame_revision
                .is_none_or(|armed| action.frame_revision < armed)
            || trace.armed_target.as_ref() != Some(&action.target)
            || action.action.as_str() != nara_reference_game::REFERENCE_RETRY_ACTION
            || action.source != UiActivationSource::Accessibility
        {
            return fail_result(
                &mut trace,
                format!("Retry action did not match the exact platform route: {action:?}"),
            );
        }
        trace.action = Some(action);
        if trace.phase == ProbePhase::InvokeSucceeded {
            trace.phase = ProbePhase::ActionObserved;
        }
        Ok(())
    }

    pub(crate) fn observe_retry_applied(
        &self,
        generation: u64,
        ui_action_sequence: Option<u64>,
    ) -> Result<(), String> {
        let mut trace = self.lock();
        let expected_sequence = trace.action.as_ref().map(|action| action.sequence);
        if generation != 2 || expected_sequence.is_none() || ui_action_sequence != expected_sequence
        {
            return fail_result(
                &mut trace,
                format!(
                    "Retry application did not retain the exact UI action cause: generation={generation}, expected={expected_sequence:?}, actual={ui_action_sequence:?}"
                ),
            );
        }
        match trace.phase {
            ProbePhase::RetryApplied
            | ProbePhase::SuccessorPresented
            | ProbePhase::PlatformVerified
            | ProbePhase::Complete => {
                if trace.run_generation == Some(generation) {
                    return Ok(());
                }
            }
            ProbePhase::InvokeReserved | ProbePhase::ActionObserved if trace.action.is_some() => {}
            _ => {
                let phase = trace.phase;
                return fail_result(
                    &mut trace,
                    format!(
                        "Retry generation {generation} was applied from invalid phase {phase:?}"
                    ),
                );
            }
        }
        trace.run_generation = Some(generation);
        if trace.phase != ProbePhase::InvokeReserved {
            trace.phase = ProbePhase::RetryApplied;
        }
        Ok(())
    }

    pub(crate) fn observe_successor_frame(&self, frame_index: u64) -> Result<(), String> {
        let mut trace = self.lock();
        match trace.phase {
            ProbePhase::SuccessorPresented
            | ProbePhase::PlatformVerified
            | ProbePhase::Complete => return Ok(()),
            ProbePhase::InvokeReserved | ProbePhase::RetryApplied
                if trace.run_generation.is_some() => {}
            _ => return Ok(()),
        }
        let Some(before) = trace.before_wgpu_frame else {
            let message = "successor presentation lost its WGPU baseline".to_owned();
            fail_locked(&mut trace, message.clone());
            return Err(message);
        };
        if frame_index <= before {
            return Ok(());
        }
        trace.after_wgpu_frame = Some(frame_index);
        if trace.phase != ProbePhase::InvokeReserved {
            trace.phase = ProbePhase::SuccessorPresented;
        }
        Ok(())
    }

    pub(crate) fn successor_is_presented(&self) -> bool {
        matches!(
            self.lock().phase,
            ProbePhase::SuccessorPresented | ProbePhase::PlatformVerified | ProbePhase::Complete
        )
    }

    pub(crate) fn observe_successor_tree(&self, hwnd: isize) -> Result<(), String> {
        let mut trace = self.lock();
        if trace.phase != ProbePhase::SuccessorPresented || trace.hwnd != Some(hwnd) {
            let phase = trace.phase;
            return fail_result(
                &mut trace,
                format!(
                    "successor platform tree did not match the presented window at phase {phase:?}"
                ),
            );
        }
        trace.phase = ProbePhase::PlatformVerified;
        Ok(())
    }

    pub(crate) fn complete_after_journal_barrier(
        &self,
        journal_high_water: u64,
    ) -> Result<(), String> {
        let mut trace = self.lock();
        let action_sequence = trace.action.as_ref().map(|action| action.sequence);
        if trace.phase != ProbePhase::PlatformVerified
            || action_sequence.is_none_or(|sequence| journal_high_water < sequence)
        {
            let phase = trace.phase;
            return fail_result(
                &mut trace,
                format!(
                    "probe completed without a post-platform journal barrier at {phase:?}: action={action_sequence:?}, high_water={journal_high_water}"
                ),
            );
        }
        trace.journal_barrier = Some(journal_high_water);
        trace.phase = ProbePhase::Complete;
        trace.stop_requested = true;
        Ok(())
    }

    pub(crate) fn fail(&self, message: impl Into<String>) {
        fail_locked(&mut self.lock(), message.into());
    }

    pub(crate) fn request_stop(&self) {
        self.lock().stop_requested = true;
    }

    pub(crate) fn should_stop(&self) -> bool {
        self.lock().should_stop()
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.lock().is_complete()
    }

    pub(crate) fn phase(&self) -> ProbePhase {
        self.lock().phase
    }

    pub(crate) fn has_observed_action(&self) -> bool {
        self.lock().action.is_some()
    }

    pub(crate) fn snapshot(&self) -> ProbeTrace {
        self.lock().clone()
    }
}

fn fail_result<T>(trace: &mut ProbeTrace, message: impl Into<String>) -> Result<T, String> {
    let message = message.into();
    fail_locked(trace, message.clone());
    Err(message)
}

fn fail_locked(trace: &mut ProbeTrace, message: String) {
    if trace.phase == ProbePhase::Failed {
        return;
    }
    trace.failure = Some(message);
    trace.phase = ProbePhase::Failed;
    trace.stop_requested = true;
}

#[cfg(test)]
#[path = "../../../tests/support/desktop_accessibility_probe_evidence.rs"]
mod tests;
