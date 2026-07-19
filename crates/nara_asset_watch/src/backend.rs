use std::{
    fmt::{self, Debug, Formatter},
    path::Path,
};

use nara_ecs::Resource;
use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher, event::ModifyKind,
    event::RenameMode,
};

use crate::{AssetWatchError, AssetWatchEvent, queue::AssetWatchEventSender};

#[derive(Resource)]
pub struct AssetWatcher {
    watcher: Option<RecommendedWatcher>,
}

impl AssetWatcher {
    pub fn watch_recursive(
        root: impl AsRef<Path>,
        sender: AssetWatchEventSender,
    ) -> Result<Self, AssetWatchError> {
        let mut watcher = notify::recommended_watcher(move |result| match result {
            Ok(event) => admit_notify_event(&sender, event),
            Err(_) => sender.record_backend_failure(),
        })
        .map_err(|error| AssetWatchError::Notify(error.to_string()))?;
        watcher
            .watch(root.as_ref(), RecursiveMode::Recursive)
            .map_err(|error| AssetWatchError::Notify(error.to_string()))?;
        Ok(Self {
            watcher: Some(watcher),
        })
    }

    pub(crate) fn stop_for_rescan(&mut self) {
        self.watcher.take();
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.watcher.is_some()
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
        EventKind::Remove(_) if !event.paths.is_empty() => Ok(event
            .paths
            .into_iter()
            .map(AssetWatchEvent::removed)
            .collect()),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() == 2 => {
            Ok(vec![AssetWatchEvent::renamed(
                event.paths[0].clone(),
                event.paths[1].clone(),
            )])
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) if !event.paths.is_empty() => {
            Ok(event
                .paths
                .into_iter()
                .map(AssetWatchEvent::removed)
                .collect())
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) if !event.paths.is_empty() => Ok(event
            .paths
            .into_iter()
            .map(AssetWatchEvent::modified)
            .collect()),
        EventKind::Modify(ModifyKind::Name(_)) => Err(()),
        EventKind::Create(_) | EventKind::Modify(_) if !event.paths.is_empty() => Ok(event
            .paths
            .into_iter()
            .map(AssetWatchEvent::modified)
            .collect()),
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => Err(()),
        _ => Ok(Vec::new()),
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
