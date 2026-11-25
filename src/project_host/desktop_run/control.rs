use nara_app::{
    RuntimeControl, RuntimeControlRequestResult, RuntimeControlStatus, RuntimeControlTicket,
    RuntimeGeneration, RuntimeScopeError, RuntimeScopeOutcome, RuntimeState,
    RuntimeTimeContinuation, RuntimeTimeContinuationError,
};
use nara_core::diagnostic::DiagnosticReport;
use nara_ecs::{World, bevy_ecs::system::Command};

use super::{DesktopNativeRun, DesktopRun, DesktopRunState};

/// Rejection or retained fault from one explicit desktop Runtime operation.
#[derive(Debug, thiserror::Error)]
pub enum DesktopControlError {
    #[error("the desktop Runtime is unavailable for this operation")]
    Unavailable,
    #[error("the desktop Runtime has no completed paused App frame")]
    NotPaused,
    #[error("the desktop Runtime control was rejected: {0:?}")]
    ControlRejected(nara_app::RuntimeControlRejection),
    #[error("the desktop managed driver scope refused entry: {0}")]
    DriverScope(RuntimeScopeError),
    #[error("the desktop product authority validation failed")]
    Authority(DiagnosticReport),
    #[error("the original desktop native operation failed: {0}")]
    Native(nara_app::AppRunError),
    #[error(transparent)]
    TimeContinuation(#[from] RuntimeTimeContinuationError),
    #[error(transparent)]
    KeyboardContinuation(#[from] nara_window::winit::WinitKeyboardContinuationError),
}

impl DesktopRun {
    /// Obtains a payload-free wake for this original live native maintenance boundary.
    ///
    /// The cloneable handle requests no App frame and grants no Runtime/input authority.
    /// It is revoked during original retirement and cannot wake a successor session.
    #[must_use]
    pub fn maintenance_wake(&self) -> Option<nara_window::winit::WinitMaintenanceWake> {
        self.live_native().ok()?.session.maintenance_wake()
    }

    /// Returns the cadence of the retained original native session, when constructed.
    #[must_use]
    pub fn control_flow(&self) -> Option<nara_window::winit::WinitControlFlow> {
        match &self.state {
            DesktopRunState::Native(native) => Some(native.session.control_flow()),
            _ => None,
        }
    }

    /// Changes cadence on the original healthy native session without changing Runtime pause.
    ///
    /// Wait preserves native frame demand and presentation maintenance. It does not discover
    /// ECS changes, wake arbitrary task completion, or animate real-time shaders. The caller
    /// selects it only for workloads that can tolerate these limits. Poll requests original
    /// frame work when selected from Wait; neither transition resets elapsed time.
    pub fn set_control_flow(
        &mut self,
        control_flow: nara_window::winit::WinitControlFlow,
    ) -> Result<(), DesktopControlError> {
        self.live_native_mut()?
            .session
            .set_control_flow(control_flow)
            .map_err(DesktopControlError::Native)
    }

    /// Returns the original managed state while the desktop aggregate still retains its Runtime.
    #[must_use]
    pub fn state(&self) -> Option<RuntimeState> {
        match &self.state {
            DesktopRunState::Native(native) => Some(native.session.runtime().state()),
            DesktopRunState::FreshRuntime { runtime, .. } => Some(runtime.state()),
            _ => None,
        }
    }

    /// A process-local observation, not permission to operate on another Runtime or process.
    #[must_use]
    pub fn runtime_generation(&self) -> Option<RuntimeGeneration> {
        match &self.state {
            DesktopRunState::Native(native) => Some(native.session.runtime().generation()),
            DesktopRunState::FreshRuntime { runtime, .. } => Some(runtime.generation()),
            _ => None,
        }
    }

    /// Admits a control to the original Runtime; a later poll applies it at its managed boundary.
    /// The returned ticket retains the actual pending/applied/failed result until retirement.
    pub fn request_control(
        &mut self,
        control: RuntimeControl,
    ) -> Result<RuntimeControlTicket, DesktopControlError> {
        if control == RuntimeControl::Stop {
            let ticket = match self.live_native_mut()?.session.request_control(control) {
                RuntimeControlRequestResult::Accepted(ticket) => ticket,
                RuntimeControlRequestResult::Rejected(reason) => {
                    return Err(DesktopControlError::ControlRejected(reason));
                }
            };
            self.request_close();
            return Ok(ticket);
        }
        match self.live_native_mut()?.session.request_control(control) {
            RuntimeControlRequestResult::Accepted(ticket) => Ok(ticket),
            RuntimeControlRequestResult::Rejected(reason) => {
                Err(DesktopControlError::ControlRejected(reason))
            }
        }
    }

    #[must_use]
    pub fn control_status(&self, ticket: RuntimeControlTicket) -> Option<RuntimeControlStatus> {
        match &self.state {
            DesktopRunState::Native(native) => native.session.control_status(ticket),
            _ => None,
        }
    }

    /// Reads the original healthy published World without exposing mutable runtime authority.
    pub fn observe<R>(
        &self,
        operation: impl FnOnce(&World) -> R,
    ) -> Result<R, DesktopControlError> {
        let native = self.live_native()?;
        let runtime = native.session.runtime();
        #[cfg(all(feature = "runtime-2d", feature = "serde"))]
        if let Some(context) = &native._context {
            return context
                .observe(runtime, operation)
                .map_err(DesktopControlError::Authority);
        }
        Ok(operation(runtime.world()))
    }

    /// Invokes game-owned capture only after the original healthy, complete paused App frame.
    ///
    /// An exact fixed tick alone is insufficient. New controls, gameplay admission or native
    /// input invalidate this boundary until the same Runtime completes paused maintenance.
    /// The game must still validate complete semantic state and its authoritative services.
    pub fn capture<R>(
        &self,
        operation: impl FnOnce(&World) -> R,
    ) -> Result<R, DesktopControlError> {
        if !self.live_native()?.session.paused_frame_complete() {
            return Err(DesktopControlError::NotPaused);
        }
        self.observe(operation)
    }

    /// Captures scheduling phase through the original time owner at this paused boundary.
    pub fn capture_time_continuation(
        &self,
    ) -> Result<RuntimeTimeContinuation, DesktopControlError> {
        self.capture(|_| ())?;
        self.live_native()?
            .session
            .capture_time_continuation()
            .map_err(Into::into)
    }

    /// Initializes phase in the original paused Runtime and requests original maintenance.
    /// Poll until a new full paused frame before game capture or restored presentation.
    pub fn restore_time_continuation(
        &mut self,
        value: RuntimeTimeContinuation,
    ) -> Result<(), DesktopControlError> {
        self.capture(|_| ())?;
        self.live_native_mut()?
            .session
            .restore_time_continuation(value)
            .map_err(Into::into)
    }

    /// Requests one original full paused App frame without advancing a fixed simulation tick.
    /// Poll until capture is eligible; a request alone is not maintenance or presentation proof.
    pub fn request_paused_maintenance(&mut self) -> Result<(), DesktopControlError> {
        self.live_native_mut()?
            .session
            .request_paused_maintenance()
            .map_err(Into::into)
    }

    /// Captures declared keyboard Button contributions through the original Input owner.
    /// Game capture must additionally validate its canonical held state and pending commands.
    pub fn capture_keyboard_holds(
        &self,
        bindings: &[nara_input::KeyboardActionBinding],
    ) -> Result<Vec<nara_input::KeyboardHold>, DesktopControlError> {
        self.capture(|_| ())?;
        self.live_native()?
            .session
            .capture_keyboard_holds(bindings)
            .map_err(Into::into)
    }

    /// Restores new local Input members in a paused candidate with native input isolated.
    /// Complete original maintenance before capture or requesting input activation.
    pub fn restore_keyboard_holds(
        &mut self,
        holds: Vec<nara_input::KeyboardHold>,
    ) -> Result<nara_input::InputAdmission, DesktopControlError> {
        self.capture(|_| ())?;
        self.live_native_mut()?
            .session
            .restore_keyboard_holds(holds)
            .map_err(Into::into)
    }

    /// Freezes ordinary native ingress after validating the exact paused keyboard inventory.
    /// Complete original paused maintenance before final handoff capture. Re-enabling input
    /// requires the same native focus and physical reconciliation as initial activation.
    pub fn isolate_keyboard_input(
        &mut self,
        bindings: &[nara_input::KeyboardActionBinding],
    ) -> Result<Vec<nara_input::KeyboardHold>, DesktopControlError> {
        self.capture(|_| ())?;
        self.live_native_mut()?
            .session
            .isolate_keyboard_input(bindings)
            .map_err(Into::into)
    }

    /// Admits controlled Synthetic keyboard input through the original isolated endpoint.
    /// This is a logical driver admission, not an observation of a physical platform key.
    pub fn admit_isolated_keyboard_input(
        &mut self,
        input: nara_input::ButtonDriverInput<nara_input::KeyCode>,
    ) -> Result<nara_input::InputAdmission, DesktopControlError> {
        self.live_native_mut()?
            .session
            .admit_isolated_keyboard_input(input)
            .map_err(Into::into)
    }

    /// Requests actual focus and physical reconciliation after the caller commits its route.
    /// An unresolved result retains the original paused game and its native input isolation.
    pub fn request_input_activation(
        &mut self,
    ) -> Result<nara_window::winit::WinitInputActivation, DesktopControlError> {
        self.capture(|_| ())?;
        self.live_native_mut()?
            .session
            .request_input_activation()
            .map_err(Into::into)
    }

    #[must_use]
    pub fn input_activation(&self) -> Option<nara_window::winit::WinitInputActivation> {
        match &self.state {
            DesktopRunState::Native(native) => Some(native.session.input_activation()),
            _ => None,
        }
    }

    /// Preserves an asynchronous activation refusal or original post-operation fault.
    #[must_use]
    pub fn input_activation_error(
        &self,
    ) -> Option<&nara_window::winit::WinitKeyboardContinuationError> {
        match &self.state {
            DesktopRunState::Native(native) => native.session.input_activation_error(),
            _ => None,
        }
    }

    /// Applies a game-owned ECS command and schedules an App frame.
    ///
    /// The returned output remains authoritative if the runtime observes a fault afterward.
    pub fn apply_command<C: Command>(
        &mut self,
        command: C,
    ) -> Result<RuntimeScopeOutcome<C::Out>, DesktopControlError> {
        self.live_native_mut()?
            .session
            .apply_command(command)
            .map_err(DesktopControlError::DriverScope)
    }

    /// Reveals only the original live native window selected to start hidden.
    pub fn reveal(&mut self) -> Result<(), DesktopControlError> {
        self.live_native_mut()?
            .session
            .reveal()
            .map_err(DesktopControlError::Native)
    }

    fn live_native(&self) -> Result<&DesktopNativeRun, DesktopControlError> {
        let DesktopRunState::Native(native) = &self.state else {
            return Err(DesktopControlError::Unavailable);
        };
        if self.close_requested
            || !matches!(
                native.progress,
                nara_window::winit::WinitRunProgress::Running
            )
            || native.session.runtime().fault().is_some()
            || !matches!(
                native.session.runtime().state(),
                RuntimeState::Running | RuntimeState::Paused
            )
        {
            return Err(DesktopControlError::Unavailable);
        }
        Ok(native)
    }

    fn live_native_mut(&mut self) -> Result<&mut DesktopNativeRun, DesktopControlError> {
        self.live_native()?;
        match &mut self.state {
            DesktopRunState::Native(native) => Ok(native),
            _ => Err(DesktopControlError::Unavailable),
        }
    }
}
