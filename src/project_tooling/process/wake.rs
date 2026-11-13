use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

type WakeCallback = dyn Fn() -> bool + Send + Sync + 'static;

/// Coalesced, authority-free notification that supervised process work changed.
///
/// The signal carries no process or lifecycle value. The consumer must drain the retained owner
/// and interpret its observations through the consumer's own lifecycle contract.
#[derive(Clone)]
pub struct SupervisedProcessWake {
    inner: Arc<SupervisedProcessWakeInner>,
}

struct SupervisedProcessWakeInner {
    pending: AtomicBool,
    callback: Box<WakeCallback>,
}

impl SupervisedProcessWake {
    pub fn new(callback: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(SupervisedProcessWakeInner {
                pending: AtomicBool::new(false),
                callback: Box::new(callback),
            }),
        }
    }

    pub fn notify(&self) {
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

    pub fn acknowledge(&self) {
        self.inner.pending.store(false, Ordering::Release);
    }

    /// Observes a coalesced notification, not process progress or physical cleanup completion.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.inner.pending.load(Ordering::Acquire)
    }
}

impl Default for SupervisedProcessWake {
    fn default() -> Self {
        Self::new(|| false)
    }
}

impl std::fmt::Debug for SupervisedProcessWake {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SupervisedProcessWake")
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
        let wake = SupervisedProcessWake::new(move || {
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
        let wake = SupervisedProcessWake::new(move || {
            callback_calls.fetch_add(1, Ordering::Relaxed);
            false
        });

        wake.notify();
        wake.notify();

        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert!(!wake.pending());
    }
}
