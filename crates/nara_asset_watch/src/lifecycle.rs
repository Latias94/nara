use std::{
    fmt::{self, Debug, Display, Formatter},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use nara_app::{
    App, PluginError, PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
};
use nara_asset::AssetSourceRoot;
use nara_ecs::Resource;

use crate::{
    AssetWatchError, AssetWatchEventSender, AssetWatcher, backend::NativeWatchSession,
    path::native_watch_paths,
};

/// The shared budget for current, pending and retiring native watcher instances.
pub const ASSET_WATCH_NATIVE_INSTANCE_LIMIT: usize = 4;

/// One selected native path attached to a single shared original watcher worker.
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

/// Physical native-session counts for one lifecycle owner.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AssetWatchLifecycleStats {
    active: usize,
    retiring: usize,
    started: u64,
    joined: u64,
}

impl AssetWatchLifecycleStats {
    /// Native sessions that have not been asked to close, including pending replacements.
    #[must_use]
    pub const fn active(self) -> usize {
        self.active
    }

    /// Sessions whose close was requested and whose original worker has not been joined.
    #[must_use]
    pub const fn retiring(self) -> usize {
        self.retiring
    }

    /// Native instances constructed, including instances whose watch setup failed.
    #[must_use]
    pub const fn started(self) -> u64 {
        self.started
    }

    /// Original workers actually joined, including failed joins.
    #[must_use]
    pub const fn joined(self) -> u64 {
        self.joined
    }
}

/// Concrete native watcher custody registered with the App's existing close protocol.
///
/// Clones share the factory fence, finite instance budget and strong native-session registry.
/// Each session is registered before `.watch()`. Facade and rescan-receipt lifetimes therefore
/// cannot detach unfinished workers. The close participant owns this registry independently
/// of resources and receipts, and completes only after every original worker has been joined.
#[derive(Clone, Resource)]
pub struct AssetWatchRuntime {
    registry: Arc<Mutex<WatchRegistry>>,
}

/// Live read-only retirement statistics retained independently of the World or Host receipt.
///
/// This exposes no watcher factories or close controls. Runtime close joins and removes every
/// native session even while observers remain alive; their shared state then holds counters only.
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
    sessions: Vec<Arc<Mutex<NativeWatchSession>>>,
    started: u64,
    joined: u64,
    joined_failure: Option<String>,
}

struct WatchCloseParticipant(AssetWatchRuntime);

impl AssetWatchRuntime {
    /// Registers the sole close participant for the supplied shutdown obligation.
    ///
    /// Call inside the declaring plugin's build hook, before native construction. Do not
    /// separately register the same shutdown obligation. This returns an owner to retain or
    /// insert as a resource; it does not insert resources or install polling systems.
    /// Managed original-worker join is supported on Windows/Linux only. Unsupported managed
    /// registration fails before any native acquisition.
    pub fn register(
        app: &mut App,
        obligation: PluginShutdownObligationId,
        participant: RuntimeCloseParticipantId,
    ) -> Result<Self, PluginError> {
        Self::ensure_managed_platform().map_err(|_| {
            PluginError::from(
                nara_app::PluginDomainError::new(
                    "asset-watch.managed-platform-unsupported",
                    "Managed native watcher join requires Windows or Linux",
                )
                .expect("asset watch platform error metadata is source-authored"),
            )
        })?;
        let runtime = Self::standalone();
        app.register_plugin_runtime_close_participant(
            obligation,
            participant,
            WatchCloseParticipant(runtime.clone()),
        )?;
        Ok(runtime)
    }

    /// Starts a native session retained by this owner before watch setup can fail.
    ///
    /// Joined retirements are reaped before capacity is checked. Once Runtime close begins,
    /// every clone refuses native acquisition. A setup error leaves cleanup in this registry;
    /// poll retirements during ordinary operation or drive the Runtime's existing close loop.
    pub fn watch_recursive(
        &self,
        root: &AssetSourceRoot,
        sender: AssetWatchEventSender,
    ) -> Result<AssetWatcher, AssetWatchError> {
        let path = AssetWatchPath::new(crate::path::absolute_lexical(root.root())?, true);
        self.watch_paths(root, &[path], sender)
    }

    /// Watches selected source directories using one original native worker.
    ///
    /// The root remains the translation namespace; selected paths do not change event routing.
    /// Paths must remain lexically and physically within it. All `watch()` calls use the same
    /// native instance. Partial setup failure retires that instance under this close owner.
    /// Caller-selected overlap and recursive coverage are left to the concrete consumer.
    pub fn watch_paths(
        &self,
        root: &AssetSourceRoot,
        paths: &[AssetWatchPath],
        sender: AssetWatchEventSender,
    ) -> Result<AssetWatcher, AssetWatchError> {
        Self::ensure_managed_platform()?;
        self.start(
            root,
            paths,
            sender,
            #[cfg(test)]
            None,
        )
    }

    /// Polls only sessions already closing; active native watching is left running.
    ///
    /// This never waits for a worker to finish. Joined failures remain observable and are
    /// carried into Runtime close as final error evidence rather than a clean completion.
    pub fn poll_retired(&self) -> Result<(), AssetWatchError> {
        self.registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .poll_retired()
    }

    /// Reads counters without polling workers or changing admission.
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

    pub(crate) fn standalone() -> Self {
        Self {
            registry: Arc::new(Mutex::new(WatchRegistry {
                accepting: true,
                sessions: Vec::with_capacity(ASSET_WATCH_NATIVE_INSTANCE_LIMIT),
                started: 0,
                joined: 0,
                joined_failure: None,
            })),
        }
    }

    pub(crate) fn start_standalone(
        &self,
        root: &AssetSourceRoot,
        sender: AssetWatchEventSender,
    ) -> Result<AssetWatcher, AssetWatchError> {
        let path = AssetWatchPath::new(crate::path::absolute_lexical(root.root())?, true);
        self.start(
            root,
            &[path],
            sender,
            #[cfg(test)]
            None,
        )
        .map_err(|error| {
            if self.stats().retiring() == 0 {
                error
            } else {
                AssetWatchError::StartFailure(AssetWatchStartFailure {
                    runtime: self.clone(),
                    message: error.to_string(),
                })
            }
        })
    }

    #[cfg(test)]
    pub(crate) fn watch_paths_with_callback_for_test(
        &self,
        root: &AssetSourceRoot,
        paths: &[AssetWatchPath],
        sender: AssetWatchEventSender,
        before_event: crate::backend::NativeCallbackProbe,
    ) -> Result<AssetWatcher, AssetWatchError> {
        Self::ensure_managed_platform()?;
        self.start(root, paths, sender, Some(before_event))
    }

    fn ensure_managed_platform() -> Result<(), AssetWatchError> {
        if cfg!(any(target_os = "windows", target_os = "linux")) {
            Ok(())
        } else {
            Err(AssetWatchError::ManagedPlatformUnsupported)
        }
    }

    fn start(
        &self,
        root: &AssetSourceRoot,
        paths: &[AssetWatchPath],
        sender: AssetWatchEventSender,
        #[cfg(test)] before_event: Option<crate::backend::NativeCallbackProbe>,
    ) -> Result<AssetWatcher, AssetWatchError> {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !registry.accepting {
            return Err(AssetWatchError::RuntimeClosing);
        }
        registry.poll_retired()?;
        if registry.sessions.len() >= ASSET_WATCH_NATIVE_INSTANCE_LIMIT {
            return Err(AssetWatchError::NativeCapacityExceeded {
                limit: ASSET_WATCH_NATIVE_INSTANCE_LIMIT,
            });
        }
        let paths = native_watch_paths(root.root(), paths)?;
        let session = Arc::new(Mutex::new(NativeWatchSession::new(
            sender,
            #[cfg(test)]
            before_event,
        )?));
        registry.sessions.push(Arc::clone(&session));
        registry.started = registry.started.saturating_add(1);
        let result = {
            let mut session = session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            paths.iter().try_for_each(|path| session.watch(path))
        };
        if let Err(error) = result {
            let _ = session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .poll_close();
            registry.reap_joined();
            return Err(error);
        }
        Ok(AssetWatcher::from_session(session, self.clone(), root))
    }

    pub(crate) fn close_session(
        &self,
        session: &Mutex<NativeWatchSession>,
    ) -> Result<bool, AssetWatchError> {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let result = session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .poll_close();
        registry.reap_joined();
        result
    }

    fn close_runtime(&self) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let mut registry = self
            .registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registry.accepting = false;
        for session in &registry.sessions {
            session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .closing = true;
        }
        let result = registry.poll_retired();
        if registry.sessions.is_empty() {
            return Ok(if registry.joined_failure.is_some() {
                RuntimeCloseProgress::CompleteWithError {
                    code: "asset-watch.native-join-failed",
                }
            } else {
                RuntimeCloseProgress::Complete
            });
        }
        match result {
            Ok(()) => Ok(RuntimeCloseProgress::Pending),
            Err(_) => Err(RuntimeCloseParticipantError::retryable(
                "asset-watch.native-close-unfinished",
            )),
        }
    }
}

impl WatchRegistry {
    fn stats(&self) -> AssetWatchLifecycleStats {
        let mut stats = AssetWatchLifecycleStats {
            started: self.started,
            joined: self.joined,
            ..AssetWatchLifecycleStats::default()
        };
        for session in &self.sessions {
            let session = session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if session.closing {
                stats.retiring += 1;
            } else {
                stats.active += 1;
            }
        }
        stats
    }

    fn poll_retired(&mut self) -> Result<(), AssetWatchError> {
        let mut failure = None;
        for session in &self.sessions {
            let mut session = session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if session.closing {
                if let Err(error) = session.poll_close() {
                    failure.get_or_insert(error);
                }
            }
        }
        self.reap_joined();
        if let Some(message) = &self.joined_failure {
            return Err(AssetWatchError::NativeClose {
                joined: true,
                message: message.clone(),
            });
        }
        failure.map_or(Ok(()), Err)
    }

    fn reap_joined(&mut self) {
        self.sessions.retain(|session| {
            let session = session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !session.joined {
                return true;
            }
            self.joined = self.joined.saturating_add(1);
            if self.joined_failure.is_none() {
                self.joined_failure = session.failure.clone();
            }
            false
        });
    }
}

impl RuntimeCloseParticipant for WatchCloseParticipant {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.close_runtime()
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.close_runtime()
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

/// Owned unfinished cleanup after standalone native watch setup failed.
///
/// Poll [`Self::try_close`] until it returns `Ok(true)` or a joined failure. Dropping this
/// value, including its last clone, requests legacy unawaited stop and cannot certify join.
/// Managed startup instead retains failures in the registered [`AssetWatchRuntime`].
#[derive(Debug, Clone)]
pub struct AssetWatchStartFailure {
    runtime: AssetWatchRuntime,
    message: String,
}

impl AssetWatchStartFailure {
    pub fn try_close(&self) -> Result<bool, AssetWatchError> {
        self.runtime.poll_retired()?;
        Ok(self.is_closed())
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        let stats = self.runtime.stats();
        stats.joined() == stats.started()
    }

    #[must_use]
    pub fn stats(&self) -> AssetWatchLifecycleStats {
        self.runtime.stats()
    }
}

impl PartialEq for AssetWatchStartFailure {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.runtime.registry, &other.runtime.registry)
            && self.message == other.message
    }
}

impl Eq for AssetWatchStartFailure {}

impl Display for AssetWatchStartFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}; native cleanup remains owned", self.message)
    }
}

impl std::error::Error for AssetWatchStartFailure {}
