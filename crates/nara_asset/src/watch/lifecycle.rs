use std::{
    fmt::{self, Debug, Formatter},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use nara_app::{
    App, PluginError, PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
};
use nara_ecs::Resource;

#[cfg(test)]
use super::backend::NativeCallbackProbe;
use super::{
    AssetWatchError, AssetWatchEventSender, AssetWatcher,
    backend::{NativeWatchPublisher, NativeWatchSession},
    path::{absolute_lexical, native_watch_paths},
};
use crate::AssetSourceRoot;

/// The shared limit on watcher handles owned by a runtime, including pending replacements.
pub const ASSET_WATCH_NATIVE_INSTANCE_LIMIT: usize = 4;

/// One selected native path attached to the shared watcher for a session.
///
/// Absolute paths must remain within the source root. Relative paths resolve against that root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetWatchPath {
    path: PathBuf,
    recursive: bool,
}

impl AssetWatchPath {
    #[must_use]
    pub fn new(path: PathBuf, recursive: bool) -> Self {
        Self { path, recursive }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub const fn recursive(&self) -> bool {
        self.recursive
    }
}

/// Watcher handle counts for one lifecycle owner.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AssetWatchLifecycleStats {
    active: usize,
    started: u64,
}

impl AssetWatchLifecycleStats {
    /// Watcher handles currently owned by the runtime, including pending replacements.
    #[must_use]
    pub const fn active(self) -> usize {
        self.active
    }

    /// Native instances constructed, including instances whose watch setup failed.
    #[must_use]
    pub const fn started(self) -> u64 {
        self.started
    }
}

/// Concrete native watcher custody registered with the App's existing close protocol.
///
/// Clones share the factory fence and watcher registry. Runtime close stops publication and
/// releases every watcher handle, including those for retained facades and rescan receipts.
#[derive(Clone, Resource)]
pub struct AssetWatchRuntime {
    registry: Arc<Mutex<WatchRegistry>>,
}

/// Live read-only watcher counts retained independently of the World or Host receipt.
#[derive(Clone)]
pub struct AssetWatchRuntimeObserver {
    registry: Arc<Mutex<WatchRegistry>>,
}

impl AssetWatchRuntimeObserver {
    #[must_use]
    pub fn stats(&self) -> AssetWatchLifecycleStats {
        self.registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stats()
    }
}

struct WatchRegistry {
    accepting: bool,
    sessions: Vec<NativeWatchSession>,
    started: u64,
}

struct WatchCloseParticipant(AssetWatchRuntime);

impl AssetWatchRuntime {
    /// Registers the sole close participant for the supplied shutdown obligation.
    ///
    /// Call inside the declaring plugin's build hook, before native construction. Do not
    /// separately register the same shutdown obligation. This returns an owner to retain or
    /// insert as a resource.
    pub fn register(
        app: &mut App,
        obligation: PluginShutdownObligationId,
        participant: RuntimeCloseParticipantId,
    ) -> Result<Self, PluginError> {
        let runtime = Self::new();
        app.register_plugin_runtime_close_participant(
            obligation,
            participant,
            WatchCloseParticipant(runtime.clone()),
        )?;
        Ok(runtime)
    }

    /// Starts a recursive native watch retained by this owner.
    ///
    /// Once Runtime close begins, every clone refuses native acquisition.
    pub fn watch_recursive(
        &self,
        root: &AssetSourceRoot,
        sender: AssetWatchEventSender,
    ) -> Result<AssetWatcher, AssetWatchError> {
        let path = AssetWatchPath::new(absolute_lexical(root.root())?, true);
        self.watch_paths(root, &[path], sender)
    }

    /// Watches selected source paths using one native watcher.
    ///
    /// The root remains the translation namespace; selected paths do not change event routing.
    /// Paths must remain lexically and physically within it. All `watch()` calls use the same
    /// native instance. Partial setup failure releases that handle immediately.
    /// Caller-selected overlap and recursive coverage are left to the concrete consumer.
    pub fn watch_paths(
        &self,
        root: &AssetSourceRoot,
        paths: &[AssetWatchPath],
        sender: AssetWatchEventSender,
    ) -> Result<AssetWatcher, AssetWatchError> {
        self.start(
            root,
            paths,
            sender,
            #[cfg(test)]
            None,
        )
    }

    /// Reads the current watcher handle counts.
    #[must_use]
    pub fn stats(&self) -> AssetWatchLifecycleStats {
        self.registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stats()
    }

    /// Obtains a live retirement observer with no native factory or lifecycle control authority.
    #[must_use]
    pub fn observer(&self) -> AssetWatchRuntimeObserver {
        AssetWatchRuntimeObserver {
            registry: Arc::clone(&self.registry),
        }
    }

    pub(crate) fn new() -> Self {
        Self {
            registry: Arc::new(Mutex::new(WatchRegistry {
                accepting: true,
                sessions: Vec::with_capacity(ASSET_WATCH_NATIVE_INSTANCE_LIMIT),
                started: 0,
            })),
        }
    }

    #[cfg(test)]
    pub(crate) fn watch_paths_with_callback_for_test(
        &self,
        root: &AssetSourceRoot,
        paths: &[AssetWatchPath],
        sender: AssetWatchEventSender,
        before_event: NativeCallbackProbe,
    ) -> Result<AssetWatcher, AssetWatchError> {
        self.start(root, paths, sender, Some(before_event))
    }

    fn start(
        &self,
        root: &AssetSourceRoot,
        paths: &[AssetWatchPath],
        sender: AssetWatchEventSender,
        #[cfg(test)] before_event: Option<NativeCallbackProbe>,
    ) -> Result<AssetWatcher, AssetWatchError> {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !registry.accepting {
            return Err(AssetWatchError::RuntimeClosing);
        }
        if registry.sessions.len() >= ASSET_WATCH_NATIVE_INSTANCE_LIMIT {
            return Err(AssetWatchError::NativeCapacityExceeded {
                limit: ASSET_WATCH_NATIVE_INSTANCE_LIMIT,
            });
        }
        let paths = native_watch_paths(root.root(), paths)?;
        let mut session = NativeWatchSession::new(
            sender,
            #[cfg(test)]
            before_event,
        )?;
        registry.started = registry.started.saturating_add(1);
        paths.iter().try_for_each(|path| session.watch(path))?;
        let publisher = Arc::clone(&session.publisher);
        registry.sessions.push(session);
        Ok(AssetWatcher::from_session(publisher, self.clone(), root))
    }

    pub(crate) fn close_session(&self, publisher: &NativeWatchPublisher) {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(index) = registry
            .sessions
            .iter()
            .position(|session| Arc::ptr_eq(&session.publisher, publisher))
        {
            drop(registry.sessions.swap_remove(index));
        }
    }

    fn close_runtime(&self) {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registry.accepting = false;
        registry.sessions.clear();
    }
}

impl WatchRegistry {
    fn stats(&self) -> AssetWatchLifecycleStats {
        AssetWatchLifecycleStats {
            active: self.sessions.len(),
            started: self.started,
        }
    }
}

impl RuntimeCloseParticipant for WatchCloseParticipant {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.close_runtime();
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.begin_close(context)
    }
}

impl Debug for AssetWatchRuntime {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetWatchRuntime")
            .field("stats", &self.stats())
            .finish()
    }
}

impl Debug for AssetWatchRuntimeObserver {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetWatchRuntimeObserver")
            .field("stats", &self.stats())
            .finish()
    }
}
