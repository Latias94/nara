use std::{
    fmt::{self, Debug, Formatter},
    sync::{Arc, Mutex},
};

use nara_asset::AssetSourceRoot;
use nara_ecs::Resource;
use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::ModifyKind,
    event::{CreateKind, RemoveKind},
};

use crate::{
    AssetWatchError, AssetWatchEvent, AssetWatchPath, AssetWatchRuntime,
    queue::AssetWatchEventSender,
};

/// A facade for one native session retained independently by its lifecycle owner.
///
/// Drop requests close without waiting. Managed owners retain the original worker until join;
/// standalone callers must poll [`Self::try_close`] to obtain the same completion guarantee.
#[derive(Resource)]
pub struct AssetWatcher {
    pub(crate) session: Arc<Mutex<NativeWatchSession>>,
    pub(crate) runtime: AssetWatchRuntime,
    source_root: AssetSourceRoot,
}

pub(crate) struct NativeWatchSession {
    watcher: Option<RecommendedWatcher>,
    pub(crate) closing: bool,
    pub(crate) joined: bool,
    pub(crate) failure: Option<String>,
    #[cfg(test)]
    callback_lifetime: std::sync::Weak<()>,
}

#[cfg(test)]
pub(crate) type NativeCallbackProbe = Box<dyn FnMut(&notify::Result<Event>) + Send>;

impl AssetWatcher {
    /// Starts an independently owned watcher with an immutable root and logical scope.
    ///
    /// Drop is an unawaited stop. Poll [`Self::try_close`] for an original-worker join on
    /// Windows/Linux. Other platforms keep legacy Drop behavior and cannot certify a join.
    /// A failure after native construction returns [`crate::AssetWatchStartFailure`] in the
    /// error, retaining unfinished cleanup for explicit polling. Managed runtimes should use
    /// [`AssetWatchRuntime::watch_recursive`] instead.
    pub fn watch_recursive(
        root: &AssetSourceRoot,
        sender: AssetWatchEventSender,
    ) -> Result<Self, AssetWatchError> {
        AssetWatchRuntime::standalone().start_standalone(root, sender)
    }

    pub(crate) fn from_session(
        session: Arc<Mutex<NativeWatchSession>>,
        runtime: AssetWatchRuntime,
        root: &AssetSourceRoot,
    ) -> Self {
        Self {
            session,
            runtime,
            source_root: root.clone(),
        }
    }

    /// Requests close and polls the original native worker without blocking for its return.
    ///
    /// `Ok(true)` certifies a successful join. An error with `joined: true` certifies join with
    /// failure; other errors leave cleanup unfinished and retryable. Unsupported platforms
    /// cannot certify worker completion.
    pub fn try_close(&mut self) -> Result<bool, AssetWatchError> {
        self.runtime.close_session(&self.session)
    }

    /// Reports original-worker join, including a join that returned failure.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .joined
    }

    pub(crate) fn stop_for_rescan(&mut self) {
        let _ = self.try_close();
    }

    pub(crate) fn source_root(&self) -> &AssetSourceRoot {
        &self.source_root
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        let session = self
            .session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        !session.closing && session.watcher.is_some()
    }

    #[cfg(test)]
    pub(crate) fn callback_lifetime(&self) -> std::sync::Weak<()> {
        self.session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .callback_lifetime
            .clone()
    }
}

impl Drop for AssetWatcher {
    fn drop(&mut self) {
        let _ = self.try_close();
    }
}

impl NativeWatchSession {
    pub(crate) fn new(
        sender: AssetWatchEventSender,
        #[cfg(test)] mut before_event: Option<NativeCallbackProbe>,
    ) -> Result<Self, AssetWatchError> {
        #[cfg(test)]
        let callback_lifetime = std::sync::Arc::new(());
        #[cfg(test)]
        let callback_observer = std::sync::Arc::downgrade(&callback_lifetime);
        let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            #[cfg(test)]
            let _callback_lifetime = &callback_lifetime;
            #[cfg(test)]
            if let Some(before_event) = before_event.as_mut() {
                before_event(&result);
            }
            match result {
                Ok(event) => admit_notify_event(&sender, event),
                Err(_) => sender.record_backend_failure(),
            }
        })
        .map_err(|error| AssetWatchError::Notify(error.to_string()))?;
        Ok(Self {
            watcher: Some(watcher),
            closing: false,
            joined: false,
            failure: None,
            #[cfg(test)]
            callback_lifetime: callback_observer,
        })
    }

    pub(crate) fn watch(&mut self, path: &AssetWatchPath) -> Result<(), AssetWatchError> {
        self.watcher
            .as_mut()
            .ok_or(AssetWatchError::RuntimeClosing)?
            .watch(
                path.path(),
                if path.recursive() {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                },
            )
            .map_err(|error| AssetWatchError::Notify(error.to_string()))
    }

    pub(crate) fn poll_close(&mut self) -> Result<bool, AssetWatchError> {
        if self.joined {
            return self.failure.as_ref().map_or(Ok(true), |message| {
                Err(AssetWatchError::NativeClose {
                    joined: true,
                    message: message.clone(),
                })
            });
        }
        self.closing = true;
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        {
            let Some(watcher) = self.watcher.as_mut() else {
                return Err(AssetWatchError::NativeClose {
                    joined: false,
                    message: "native worker ownership is unavailable".to_owned(),
                });
            };
            let result = watcher.try_close();
            self.joined = watcher.is_closed();
            if self.joined {
                self.watcher.take();
            }
            match result {
                Ok(_) => Ok(self.joined),
                Err(error) => {
                    let message = error.to_string();
                    if self.joined {
                        self.failure = Some(message.clone());
                    }
                    Err(AssetWatchError::NativeClose {
                        joined: self.joined,
                        message,
                    })
                }
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            self.watcher.take();
            Err(AssetWatchError::ManagedPlatformUnsupported)
        }
    }
}

impl Debug for AssetWatcher {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetWatcher")
            .field("running", &self.is_running())
            .finish()
    }
}

pub(crate) fn watch_events_from_notify(event: Event) -> Result<Vec<AssetWatchEvent>, ()> {
    if event.need_rescan() {
        return Err(());
    }
    match event.kind {
        EventKind::Remove(RemoveKind::File) if !event.paths.is_empty() => Ok(event
            .paths
            .into_iter()
            .map(AssetWatchEvent::removed)
            .collect()),
        // Notify rename events do not prove file scope. A removed directory cannot be statted.
        EventKind::Modify(ModifyKind::Name(_)) => Err(()),
        EventKind::Create(CreateKind::File) | EventKind::Modify(ModifyKind::Data(_))
            if !event.paths.is_empty() =>
        {
            Ok(event
                .paths
                .into_iter()
                .map(AssetWatchEvent::modified)
                .collect())
        }
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => Err(()),
        EventKind::Access(_) => Ok(Vec::new()),
        EventKind::Any | EventKind::Other => Err(()),
    }
}

pub(crate) fn admit_notify_event(sender: &AssetWatchEventSender, event: Event) {
    let discarded_events = event.paths.len().max(1);
    match watch_events_from_notify(event) {
        Ok(events) => {
            let _ = sender.try_send_batch(events);
        }
        Err(()) => sender.record_callback_translation_failure(discarded_events),
    }
}
