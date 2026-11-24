use std::{
    path::Path,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::Duration,
};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::protocol::REQUEST_FILE;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub(super) callbacks: u64,
    pub(super) hints: u64,
    pub(super) coalesced_hints: u64,
    pub(super) native_wake_delivery_failures: u64,
    pub(super) watcher_started: bool,
    /// Notification delivery has stopped and the upstream watcher handle has been released.
    pub(super) watcher_closed: bool,
    pub(super) counters_saturated: bool,
}

pub(super) struct RequestWatch {
    watcher: Option<RecommendedWatcher>,
    shared: Arc<Hints>,
    started: bool,
}

#[derive(Default)]
struct Hints {
    state: Mutex<HintState>,
    changed: Condvar,
    #[cfg(feature = "desktop")]
    dispatch: Mutex<()>,
}

#[derive(Default)]
struct HintState {
    dirty: bool,
    failure: Option<&'static str>,
    closing: bool,
    snapshot: Snapshot,
    #[cfg(feature = "desktop")]
    wake: Option<nara::winit::WinitMaintenanceWake>,
}

impl RequestWatch {
    pub(super) fn new() -> Self {
        Self {
            watcher: None,
            shared: Arc::new(Hints::default()),
            started: false,
        }
    }

    pub(super) fn start(&mut self, path: &Path) -> Result<(), String> {
        if self.started || self.shared.lock().closing {
            return Err("the original control-directory watcher cannot be restarted".into());
        }
        self.started = true;
        let shared = Arc::clone(&self.shared);
        let request = path.join(REQUEST_FILE);
        let watcher = notify::recommended_watcher(move |event| {
            shared.on_event(&request, event);
        })
        .map_err(|_| self.shared.fail("create control-directory watcher failed"))?;
        self.shared.lock().snapshot.watcher_started = true;
        if self
            .watcher
            .insert(watcher)
            .watch(path, RecursiveMode::NonRecursive)
            .is_err()
        {
            let failure = self.shared.fail("register control-directory watch failed");
            self.release_watcher();
            return Err(failure);
        }
        self.shared.lock().dirty = true;
        self.shared.changed.notify_all();
        self.shared.deliver();
        Ok(())
    }

    pub(super) fn acknowledge(&self) -> Result<(), String> {
        let mut state = self.shared.lock();
        state.dirty = false;
        state.result()
    }

    pub(super) fn pending(&self) -> bool {
        self.shared.lock().pending()
    }

    pub(super) fn wait(&self, timeout: Duration) -> Result<(), String> {
        let (state, _) = self
            .shared
            .changed
            .wait_timeout_while(self.shared.lock(), timeout, |state| !state.pending())
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.result()
    }

    #[cfg(feature = "desktop")]
    pub(super) fn bind_native_wake(&self, wake: nara::winit::WinitMaintenanceWake) {
        let pending = {
            let _dispatch = self
                .shared
                .dispatch
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut state = self.shared.lock();
            if state.closing {
                return;
            }
            state.wake = Some(wake);
            state.pending()
        };
        if pending {
            self.shared.deliver();
        }
    }

    pub(super) fn snapshot(&self) -> Snapshot {
        self.shared.lock().snapshot
    }

    pub(super) fn close(&mut self) -> Result<(), String> {
        self.release_watcher();
        self.shared.lock().result()
    }

    fn release_watcher(&mut self) {
        self.shared.begin_close();
        drop(self.watcher.take());
        self.shared.lock().snapshot.watcher_closed = true;
    }
}

impl Drop for RequestWatch {
    fn drop(&mut self) {
        self.release_watcher();
    }
}

impl Hints {
    fn lock(&self) -> MutexGuard<'_, HintState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn on_event(&self, request: &Path, result: notify::Result<Event>) {
        let relevant = result
            .as_ref()
            .map_or(true, |event| request_hint(request, event));
        let dispatch = {
            let mut state = self.lock();
            if state.closing {
                return;
            }
            let snapshot = &mut state.snapshot;
            increment(&mut snapshot.callbacks, &mut snapshot.counters_saturated);
            if !relevant {
                return;
            }
            let dirty = state.dirty;
            let snapshot = &mut state.snapshot;
            increment(&mut snapshot.hints, &mut snapshot.counters_saturated);
            if dirty {
                increment(
                    &mut snapshot.coalesced_hints,
                    &mut snapshot.counters_saturated,
                );
            }
            state.dirty = true;
            let new_failure = result.is_err() && state.failure.is_none();
            if result.is_err() {
                state
                    .failure
                    .get_or_insert("control-directory notification backend failed");
            }
            self.changed.notify_all();
            !dirty || new_failure
        };
        if dispatch {
            self.deliver();
        }
    }

    fn fail(&self, message: &'static str) -> String {
        let message = {
            let mut state = self.lock();
            state.dirty = true;
            *state.failure.get_or_insert(message)
        };
        self.changed.notify_all();
        self.deliver();
        message.into()
    }

    fn begin_close(&self) {
        #[cfg(feature = "desktop")]
        let _dispatch = self
            .dispatch
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        {
            let mut state = self.lock();
            state.closing = true;
            #[cfg(feature = "desktop")]
            {
                state.wake = None;
            }
        }
        self.changed.notify_all();
    }

    fn deliver(&self) {
        #[cfg(feature = "desktop")]
        {
            let _dispatch = self
                .dispatch
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let wake = {
                let state = self.lock();
                if state.closing || !state.pending() {
                    return;
                }
                state.wake.clone()
            };
            if let Some(wake) = wake
                && !wake.notify()
            {
                let mut state = self.lock();
                if !state.closing {
                    let snapshot = &mut state.snapshot;
                    increment(
                        &mut snapshot.native_wake_delivery_failures,
                        &mut snapshot.counters_saturated,
                    );
                    state
                        .failure
                        .get_or_insert("original native maintenance wake delivery failed");
                    state.dirty = true;
                }
                self.changed.notify_all();
            }
        }
    }
}

impl HintState {
    fn pending(&self) -> bool {
        self.dirty || self.failure.is_some()
    }

    fn result(&self) -> Result<(), String> {
        self.failure.map_or(Ok(()), |message| Err(message.into()))
    }
}

fn request_hint(request: &Path, event: &Event) -> bool {
    if event.need_rescan() {
        return true;
    }
    if matches!(event.kind, EventKind::Remove(_))
        && !event.paths.is_empty()
        && !event
            .paths
            .iter()
            .any(|path| Some(path.as_path()) == request.parent())
    {
        return false;
    }
    if !event.paths.is_empty()
        && !event
            .paths
            .iter()
            .any(|path| path == request || Some(path.as_path()) == request.parent())
    {
        return false;
    }
    matches!(
        event.kind,
        EventKind::Any
            | EventKind::Other
            | EventKind::Create(_)
            | EventKind::Modify(_)
            | EventKind::Remove(_)
    )
}

fn increment(value: &mut u64, overflow: &mut bool) {
    if let Some(next) = value.checked_add(1) {
        *value = next;
    } else {
        *overflow = true;
    }
}

#[cfg(test)]
#[path = "request_watch/tests.rs"]
mod tests;
