use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

type WakeCallback = dyn Fn() -> bool + Send + Sync + 'static;

/// Coalesced, authority-free notification that private supervised Play work changed.
///
/// The signal carries no attempt, process, or lifecycle value. The UI thread must drain the
/// retained owner and apply its generation-stamped observations through the normal Host seam.
#[derive(Clone)]
pub(crate) struct SupervisedPlayWake {
    inner: Arc<SupervisedPlayWakeInner>,
}

struct SupervisedPlayWakeInner {
    pending: AtomicBool,
    callback: Box<WakeCallback>,
}

impl SupervisedPlayWake {
    pub(crate) fn new(callback: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(SupervisedPlayWakeInner {
                pending: AtomicBool::new(false),
                callback: Box::new(callback),
            }),
        }
    }

    pub(crate) fn notify(&self) {
        if self
            .inner
            .pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        if !(self.inner.callback)() {
            self.inner.pending.store(false, Ordering::Release);
        }
    }

    pub(crate) fn acknowledge(&self) {
        self.inner.pending.store(false, Ordering::Release);
    }

    #[cfg(test)]
    pub(crate) fn pending(&self) -> bool {
        self.inner.pending.load(Ordering::Acquire)
    }
}

impl Default for SupervisedPlayWake {
    fn default() -> Self {
        Self::new(|| false)
    }
}

impl std::fmt::Debug for SupervisedPlayWake {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SupervisedPlayWake")
            .field("pending", &self.inner.pending.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn notifications_coalesce_until_the_ui_acknowledges_the_event() {
        let calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = Arc::clone(&calls);
        let wake = SupervisedPlayWake::new(move || {
            callback_calls.fetch_add(1, Ordering::Relaxed);
            true
        });

        wake.notify();
        wake.notify();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(wake.pending());

        wake.acknowledge();
        wake.notify();
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn failed_delivery_rearms_the_signal() {
        let calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = Arc::clone(&calls);
        let wake = SupervisedPlayWake::new(move || {
            callback_calls.fetch_add(1, Ordering::Relaxed);
            false
        });

        wake.notify();
        wake.notify();

        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert!(!wake.pending());
    }
}
