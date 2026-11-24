use std::{
    fmt::{self, Debug, Formatter},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use nara_asset::AssetSourceRoot;
use nara_ecs::Resource;
use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{CreateKind, ModifyKind, RemoveKind},
};

use crate::{
    AssetWatchError, AssetWatchEvent, AssetWatchPath, AssetWatchRuntime,
    queue::AssetWatchEventSender,
};

/// A facade for one native session retained independently by its lifecycle owner.
///
/// Dropping the facade stops event publication and releases the `notify` watcher.
#[derive(Resource)]
pub struct AssetWatcher {
    publisher: NativeWatchPublisher,
    runtime: AssetWatchRuntime,
    source_root: AssetSourceRoot,
}

pub(crate) type NativeWatchPublisher = Arc<Mutex<Option<AssetWatchEventSender>>>;

pub(crate) struct NativeWatchSession {
    watcher: RecommendedWatcher,
    pub(crate) publisher: NativeWatchPublisher,
}

#[cfg(test)]
pub(crate) type NativeCallbackProbe = Box<dyn FnMut(&notify::Result<Event>) + Send>;

impl AssetWatcher {
    /// Starts an independently owned watcher with an immutable root and logical scope.
    ///
    /// Managed runtimes should use [`AssetWatchRuntime::watch_recursive`].
    pub fn watch_recursive(
        root: &AssetSourceRoot,
        sender: AssetWatchEventSender,
    ) -> Result<Self, AssetWatchError> {
        AssetWatchRuntime::new().watch_recursive(root, sender)
    }

    pub(crate) fn from_session(
        publisher: NativeWatchPublisher,
        runtime: AssetWatchRuntime,
        root: &AssetSourceRoot,
    ) -> Self {
        Self {
            publisher,
            runtime,
            source_root: root.clone(),
        }
    }

    /// Stops publication and releases the native watcher. Repeated calls have no effect.
    ///
    /// On Windows and Linux, the backend finishes shutdown independently. Events already in
    /// the queue remain available.
    pub fn close(&mut self) {
        self.runtime.close_session(&self.publisher);
    }

    pub(crate) fn source_root(&self) -> &AssetSourceRoot {
        &self.source_root
    }

    /// Reports whether this session can still publish events.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.publisher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }
}

impl Drop for AssetWatcher {
    fn drop(&mut self) {
        self.close();
    }
}

impl NativeWatchSession {
    pub(crate) fn new(
        sender: AssetWatchEventSender,
        #[cfg(test)] mut before_event: Option<NativeCallbackProbe>,
    ) -> Result<Self, AssetWatchError> {
        let publisher = Arc::new(Mutex::new(Some(sender)));
        let callback_publisher = Arc::clone(&publisher);
        let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            #[cfg(test)]
            if let Some(before_event) = before_event.as_mut() {
                before_event(&result);
            }
            let result = result.map(watch_events_from_notify);
            let publisher = callback_publisher
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(sender) = publisher.as_ref() {
                match result {
                    Ok(events) => admit_watch_events(sender, events),
                    Err(_) => sender.record_backend_failure(),
                }
            }
        })
        .map_err(|error| AssetWatchError::Notify(error.to_string()))?;
        Ok(Self { watcher, publisher })
    }

    pub(crate) fn watch(&mut self, path: &AssetWatchPath) -> Result<(), AssetWatchError> {
        self.watcher
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
}

impl Drop for NativeWatchSession {
    fn drop(&mut self) {
        let sender = self
            .publisher
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        drop(sender);
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

pub(crate) fn watch_events_from_notify(event: Event) -> Result<Vec<AssetWatchEvent>, usize> {
    let discarded_events = event.paths.len().max(1);
    if event.need_rescan() {
        return Err(discarded_events);
    }
    let convert: fn(PathBuf) -> AssetWatchEvent = match event.kind {
        EventKind::Remove(RemoveKind::File) => AssetWatchEvent::removed,
        EventKind::Create(CreateKind::File) | EventKind::Modify(ModifyKind::Data(_)) => {
            AssetWatchEvent::modified
        }
        EventKind::Create(CreateKind::Any) | EventKind::Modify(ModifyKind::Any)
            if event.paths.iter().all(|path| {
                path.is_absolute() && path.symlink_metadata().is_ok_and(|meta| meta.is_file())
            }) =>
        {
            AssetWatchEvent::modified
        }
        EventKind::Access(_) => return Ok(Vec::new()),
        _ => return Err(discarded_events),
    };
    if event.paths.is_empty() {
        return Err(discarded_events);
    }
    Ok(event.paths.into_iter().map(convert).collect())
}

pub(crate) fn admit_watch_events(
    sender: &AssetWatchEventSender,
    events: Result<Vec<AssetWatchEvent>, usize>,
) {
    match events {
        Ok(events) => {
            let _ = sender.try_send_batch(events);
        }
        Err(discarded_events) => sender.record_callback_translation_failure(discarded_events),
    }
}
