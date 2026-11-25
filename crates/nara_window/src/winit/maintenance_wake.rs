use super::{EventLoopProxy, SessionWakeId, WinitUserEvent};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod tests;

/// A cloneable maintenance-only hint for one original native session.
///
/// This handle may be sent to another thread. It carries no work, Runtime, window or graphics
/// authority. The original consumer remains responsible for checking its pending work.
#[derive(Clone)]
pub struct WinitMaintenanceWake {
    proxy: EventLoopProxy<WinitUserEvent>,
    wake_id: SessionWakeId,
    state: Arc<MaintenanceWakeState>,
}

impl WinitMaintenanceWake {
    pub(super) fn new(
        proxy: EventLoopProxy<WinitUserEvent>,
        wake_id: SessionWakeId,
        state: Arc<MaintenanceWakeState>,
    ) -> Self {
        Self {
            proxy,
            wake_id,
            state,
        }
    }

    /// Accepts a coalesced hint to service the original native session's maintenance boundary.
    ///
    /// `true` means the hint was queued or merged into an already pending hint. It does not
    /// acknowledge work completion or promise that the session will remain live. At most one
    /// event is queued until the original handler acknowledges it. No App frame is requested,
    /// and elapsed time and paused capture eligibility are preserved.
    ///
    /// Returns `false` after original shutdown starts or native delivery is refused. All clones
    /// then remain revoked, including after the physical pump moves to another session.
    #[must_use]
    pub fn notify(&self) -> bool {
        self.state.notify(|| {
            self.proxy
                .send_event(WinitUserEvent::MaintenanceWake(self.wake_id))
                .is_ok()
        })
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
enum WakeStatus {
    #[default]
    Idle,
    Pending,
    Revoked,
}

#[derive(Default)]
pub(super) struct MaintenanceWakeState {
    status: Mutex<WakeStatus>,
    #[cfg(test)]
    acknowledgements: AtomicUsize,
}

impl MaintenanceWakeState {
    pub(super) fn notify(&self, send: impl FnOnce() -> bool) -> bool {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        match *status {
            WakeStatus::Revoked => false,
            WakeStatus::Pending => true,
            WakeStatus::Idle => {
                if send() {
                    *status = WakeStatus::Pending;
                    true
                } else {
                    *status = WakeStatus::Revoked;
                    false
                }
            }
        }
    }

    pub(super) fn acknowledge(&self) -> bool {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if *status == WakeStatus::Pending {
            *status = WakeStatus::Idle;
            #[cfg(test)]
            self.acknowledgements.fetch_add(1, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    pub(super) fn revoke(&self) {
        *self
            .status
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = WakeStatus::Revoked;
    }

    pub(super) fn is_live(&self) -> bool {
        *self
            .status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            != WakeStatus::Revoked
    }

    #[cfg(test)]
    pub(super) fn is_pending(&self) -> bool {
        *self
            .status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            == WakeStatus::Pending
    }

    #[cfg(test)]
    pub(super) fn acknowledgements(&self) -> usize {
        self.acknowledgements.load(Ordering::SeqCst)
    }
}
