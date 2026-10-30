use std::fmt::{self, Debug, Formatter};

use nara_asset::AssetSourceRoot;
use nara_ecs::Resource;
use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::ModifyKind,
    event::{CreateKind, RemoveKind},
};

use crate::{AssetWatchError, AssetWatchEvent, queue::AssetWatchEventSender};

#[derive(Resource)]
pub struct AssetWatcher {
    watcher: Option<RecommendedWatcher>,
    source_root: AssetSourceRoot,
    #[cfg(test)]
    callback_lifetime: std::sync::Weak<()>,
}

impl AssetWatcher {
    /// Starts native watching with an immutable native-root and logical-scope binding.
    pub fn watch_recursive(
        root: &AssetSourceRoot,
        sender: AssetWatchEventSender,
    ) -> Result<Self, AssetWatchError> {
        #[cfg(test)]
        let callback_lifetime = std::sync::Arc::new(());
        #[cfg(test)]
        let callback_observer = std::sync::Arc::downgrade(&callback_lifetime);
        let mut watcher = notify::recommended_watcher(move |result| {
            #[cfg(test)]
            let _callback_lifetime = &callback_lifetime;
            match result {
                Ok(event) => admit_notify_event(&sender, event),
                Err(_) => sender.record_backend_failure(),
            }
        })
        .map_err(|error| AssetWatchError::Notify(error.to_string()))?;
        watcher
            .watch(root.root(), RecursiveMode::Recursive)
            .map_err(|error| AssetWatchError::Notify(error.to_string()))?;
        Ok(Self {
            watcher: Some(watcher),
            source_root: root.clone(),
            #[cfg(test)]
            callback_lifetime: callback_observer,
        })
    }

    pub(crate) fn stop_for_rescan(&mut self) {
        self.watcher.take();
    }

    pub(crate) fn source_root(&self) -> &AssetSourceRoot {
        &self.source_root
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.watcher.is_some()
    }

    #[cfg(test)]
    pub(crate) fn callback_lifetime(&self) -> std::sync::Weak<()> {
        self.callback_lifetime.clone()
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
