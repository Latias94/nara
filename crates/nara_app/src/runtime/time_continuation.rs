use crate::{
    RuntimeTimeContinuation, RuntimeTimeContinuationError, RuntimeTimeContinuationRejection,
};

use super::{
    DriverScopeAuthorityPolicy, RuntimeFault, RuntimeFrameKind, RuntimeInstance, RuntimeState,
};

impl RuntimeInstance {
    /// Captures only scheduling phase and policy after a healthy complete paused App frame.
    ///
    /// Game state, pending services and input quiescence remain the caller's separate inventory.
    /// An exact fixed tick must first be followed by full paused maintenance.
    pub fn capture_time_continuation(
        &self,
    ) -> Result<RuntimeTimeContinuation, RuntimeTimeContinuationError> {
        self.check_time_continuation_boundary()?;
        let value = RuntimeTimeContinuation::capture(&self.owner.world)?;
        self.owner
            .validate_managed_runtime_authority()
            .map_err(|fault| self.deliver_time_continuation_fault(fault))?;
        Ok(value)
    }

    /// Initializes pending phase through this Runtime's original time owner.
    ///
    /// The current time policy must match. Identity and real/virtual/fixed history stay local;
    /// only pending phase and derived interpolation change. Complete another paused frame before
    /// capture. A fault after mutation reports `applied: true` and never implies rollback.
    pub fn restore_time_continuation(
        &mut self,
        value: RuntimeTimeContinuation,
    ) -> Result<(), RuntimeTimeContinuationError> {
        self.check_time_continuation_boundary()?;
        let outcome = self
            .owner
            .capture_driver_scope(DriverScopeAuthorityPolicy::Full, |world| value.apply(world))
            .map_err(|fault| self.deliver_time_continuation_fault(fault))?;
        let applied = outcome.output.is_ok();
        if applied {
            self.last_frame_kind = None;
        }
        if let Some(fault) = outcome.post_operation_fault {
            self.execution_fault_delivered.set(true);
            return Err(RuntimeTimeContinuationError::Faulted {
                applied,
                rejection: outcome.output.err(),
                fault,
            });
        }
        outcome.output.map_err(Into::into)
    }

    fn check_time_continuation_boundary(&self) -> Result<(), RuntimeTimeContinuationError> {
        if let Some(fault) = self.fault() {
            return Err(self.deliver_time_continuation_fault(fault.clone()));
        }
        let state = self.state();
        if state != RuntimeState::Paused {
            return Err(RuntimeTimeContinuationRejection::Unavailable { state }.into());
        }
        self.owner
            .validate_managed_runtime_authority()
            .map_err(|fault| self.deliver_time_continuation_fault(fault))?;
        if self.pending_control.is_some() {
            return Err(RuntimeTimeContinuationRejection::ControlPending.into());
        }
        if self.last_frame_kind != Some(RuntimeFrameKind::FullFrame) {
            return Err(RuntimeTimeContinuationRejection::IncompletePausedFrame.into());
        }
        Ok(())
    }

    fn deliver_time_continuation_fault(&self, fault: RuntimeFault) -> RuntimeTimeContinuationError {
        self.execution_fault_delivered.set(true);
        RuntimeTimeContinuationError::Faulted {
            applied: false,
            rejection: None,
            fault,
        }
    }
}
