//! Optional filesystem watcher adapter for nara assets.

use std::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
};

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::{
    AssetPath, AssetPathError, AssetSourceChange, AssetSourceChangeKind, AssetSourceRoot,
    AssetTaskUpdateSet,
};
use nara_ecs::schedule::IntoScheduleConfigs;

mod backend;
mod observability;
mod path;
mod queue;

pub use backend::AssetWatcher;
pub use observability::{AssetWatchRuntimeState, AssetWatchRuntimeStatus};
pub use queue::{
    AssetWatchEventQueue, AssetWatchEventSender, AssetWatchQueueDrain, AssetWatchQueueLimits,
    AssetWatchQueueSendError, AssetWatchQueueStats,
};

use observability::drain_asset_watch_events;
use path::{logical_path, same_lexical_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetWatchEventKind {
    Modified,
    Removed,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetWatchEvent {
    kind: AssetWatchEventKind,
    path: PathBuf,
    to_path: Option<PathBuf>,
    broad_scope: bool,
}

impl AssetWatchEvent {
    #[must_use]
    /// Constructs an exact-file observation. Use `subtree` when file scope is not proven.
    pub fn modified(path: impl Into<PathBuf>) -> Self {
        let mut path = path.into();
        path.shrink_to_fit();
        Self {
            kind: AssetWatchEventKind::Modified,
            path,
            to_path: None,
            broad_scope: false,
        }
    }

    #[must_use]
    /// Constructs an exact-file removal; this does not infer the type of a missing path.
    pub fn removed(path: impl Into<PathBuf>) -> Self {
        let mut path = path.into();
        path.shrink_to_fit();
        Self {
            kind: AssetWatchEventKind::Removed,
            path,
            to_path: None,
            broad_scope: false,
        }
    }

    #[must_use]
    /// Constructs a rename whose two sides are already known to be exact files.
    pub fn renamed(from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> Self {
        let mut path = from.into();
        path.shrink_to_fit();
        let mut to_path = to.into();
        to_path.shrink_to_fit();
        Self {
            kind: AssetWatchEventKind::Renamed,
            path,
            to_path: Some(to_path),
            broad_scope: false,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> AssetWatchEventKind {
        self.kind
    }

    /// Retains unproven subtree coverage until the Host completes a full reconciliation.
    #[must_use]
    pub fn subtree(path: impl Into<PathBuf>) -> Self {
        Self {
            broad_scope: true,
            ..Self::modified(path)
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn to_path(&self) -> Option<&Path> {
        self.to_path.as_deref()
    }

    fn retained_path_bytes(&self) -> usize {
        self.path
            .capacity()
            .saturating_add(self.to_path.as_ref().map_or(0, PathBuf::capacity))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetWatchError {
    SourceOutsideRoot { path: PathBuf, root: PathBuf },
    NonUtf8Path(PathBuf),
    InvalidLogicalPath(AssetPathError),
    MissingRenameTarget(PathBuf),
    Filesystem(String),
    Notify(String),
    RescanRequired,
}

impl Display for AssetWatchError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceOutsideRoot { path, root } => write!(
                formatter,
                "asset watch path '{}' is outside root '{}'",
                path.display(),
                root.display()
            ),
            Self::NonUtf8Path(path) => {
                write!(
                    formatter,
                    "asset watch path '{}' is not UTF-8",
                    path.display()
                )
            }
            Self::InvalidLogicalPath(error) => Display::fmt(error, formatter),
            Self::MissingRenameTarget(path) => write!(
                formatter,
                "asset watch rename from '{}' has no target path",
                path.display()
            ),
            Self::Filesystem(error) => write!(formatter, "asset watch filesystem error: {error}"),
            Self::Notify(error) => write!(formatter, "asset watch error: {error}"),
            Self::RescanRequired => {
                formatter.write_str("asset watch scope requires a complete source rescan")
            }
        }
    }
}

impl Error for AssetWatchError {}

#[derive(Debug, Default, Clone, Copy)]
pub struct AssetWatchTranslator;

impl AssetWatchTranslator {
    pub fn translate_event(
        &self,
        root: &AssetSourceRoot,
        event: &AssetWatchEvent,
    ) -> Result<Vec<AssetSourceChange>, AssetWatchError> {
        if event.broad_scope
            || same_lexical_path(root.root(), event.path())?
            || event.path().is_dir()
        {
            return Err(AssetWatchError::RescanRequired);
        }
        match event.kind() {
            AssetWatchEventKind::Modified => self
                .translate_path(root, event.path(), AssetSourceChangeKind::Modified)
                .map(|change| vec![change]),
            AssetWatchEventKind::Removed => self
                .translate_path(root, event.path(), AssetSourceChangeKind::Removed)
                .map(|change| vec![change]),
            AssetWatchEventKind::Renamed => {
                let to_path = event
                    .to_path()
                    .ok_or_else(|| AssetWatchError::MissingRenameTarget(event.path.clone()))?;
                self.translate_rename(root, event.path(), to_path)
            }
        }
    }

    fn translate_rename(
        &self,
        root: &AssetSourceRoot,
        from_path: &Path,
        to_path: &Path,
    ) -> Result<Vec<AssetSourceChange>, AssetWatchError> {
        let mut changes = Vec::new();
        let mut outside_error = None;
        for result in [
            self.translate_path(root, from_path, AssetSourceChangeKind::Removed),
            self.translate_path(root, to_path, AssetSourceChangeKind::Modified),
        ] {
            match result {
                Ok(change) => changes.push(change),
                Err(error @ AssetWatchError::SourceOutsideRoot { .. }) => {
                    outside_error.get_or_insert(error);
                }
                Err(error) => return Err(error),
            }
        }

        if changes.is_empty() {
            Err(outside_error.expect("rename translation should have at least one side"))
        } else {
            Ok(changes)
        }
    }

    fn translate_path(
        &self,
        root: &AssetSourceRoot,
        path: &Path,
        source_kind: AssetSourceChangeKind,
    ) -> Result<AssetSourceChange, AssetWatchError> {
        let logical = logical_path(root.root(), path)?;
        let logical = if let Some(source) = logical.as_str().strip_suffix(".meta") {
            AssetPath::new(source).map_err(AssetWatchError::InvalidLogicalPath)?
        } else {
            logical
        };
        let kind = if path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .is_some_and(|file_name| file_name.ends_with(".meta"))
        {
            match source_kind {
                AssetSourceChangeKind::Removed => AssetSourceChangeKind::Removed,
                _ => AssetSourceChangeKind::MetaModified,
            }
        } else {
            source_kind
        };
        Ok(AssetSourceChange::new(logical, kind))
    }
}

/// A receipt for a runtime-owned watcher started before Host source-tree reconciliation.
///
/// Dropping this receipt cancels its exact replacement watcher and leaves the rescan obligation
/// intact. The receipt cannot keep native watching alive after runtime shutdown or supersession.
/// Call `complete` only after reconciliation; the runtime retains events collected during it.
#[must_use]
pub struct AssetWatchRescan {
    world: nara_ecs::world::WorldId,
    root: PathBuf,
    session: Weak<Mutex<Option<PendingWatchRescan>>>,
}

struct PendingWatchRescan {
    queue: AssetWatchEventQueue,
    watcher: AssetWatcher,
}

#[derive(nara_ecs::Resource)]
struct ActiveWatchRescan(Arc<Mutex<Option<PendingWatchRescan>>>);

impl Drop for ActiveWatchRescan {
    fn drop(&mut self) {
        retire_watch_rescan(&self.0);
    }
}

impl Drop for AssetWatchRescan {
    fn drop(&mut self) {
        if let Some(session) = self.session.upgrade() {
            retire_watch_rescan(&session);
        }
    }
}

fn retire_watch_rescan(session: &Mutex<Option<PendingWatchRescan>>) {
    let pending = session
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take();
    // Release native state outside the session lock, including when the receipt is abandoned.
    drop(pending);
}

impl AssetWatchRescan {
    pub fn begin(
        world: &mut nara_ecs::World,
        limits: AssetWatchQueueLimits,
    ) -> Result<Self, AssetWatchError> {
        if !world
            .get_resource::<AssetWatchRuntimeStatus>()
            .is_some_and(|status| status.requires_rescan())
        {
            return Err(AssetWatchError::RescanRequired);
        }
        validate_watch_replacement(world)?;
        world.register_component::<ActiveWatchRescan>();
        let admission = if world.contains_resource::<ActiveWatchRescan>() {
            nara_ecs::__private::validate_resource_scope::<ActiveWatchRescan>(world)
        } else {
            nara_ecs::__private::validate_resource_insertion::<ActiveWatchRescan>(world)
        };
        admission.map_err(|_| AssetWatchError::RescanRequired)?;
        let root = world
            .get_resource::<AssetSourceRoot>()
            .ok_or(AssetWatchError::RescanRequired)?
            .root()
            .to_path_buf();
        let queue = AssetWatchEventQueue::with_limits(limits);
        let watcher = AssetWatcher::watch_recursive(&root, queue.sender())?;
        let session = Arc::new(Mutex::new(Some(PendingWatchRescan { queue, watcher })));
        world.insert_resource(ActiveWatchRescan(Arc::clone(&session)));
        Ok(Self {
            world: world.id(),
            root,
            session: Arc::downgrade(&session),
        })
    }

    /// Acknowledges the Host's completed reconciliation and installs its retained incremental tail.
    pub fn complete(self, world: &mut nara_ecs::World) -> Result<(), AssetWatchError> {
        if world.id() != self.world
            || !world
                .get_resource::<AssetSourceRoot>()
                .is_some_and(|root| root.root() == self.root)
            || !world
                .get_resource::<AssetWatchRuntimeStatus>()
                .is_some_and(|status| status.requires_rescan())
            || !world
                .get_resource::<ActiveWatchRescan>()
                .is_some_and(|active| Weak::ptr_eq(&Arc::downgrade(&active.0), &self.session))
        {
            return Err(AssetWatchError::RescanRequired);
        }
        validate_watch_replacement(world)?;
        let session = self
            .session
            .upgrade()
            .ok_or(AssetWatchError::RescanRequired)?;
        let pending = {
            let mut pending = session
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let replacement = pending.as_ref().ok_or(AssetWatchError::RescanRequired)?;
            if replacement.queue.stats().rescan_required() {
                return Err(AssetWatchError::RescanRequired);
            }
            pending.take().ok_or(AssetWatchError::RescanRequired)?
        };
        world.insert_resource(pending.queue.observer());
        world.insert_resource(pending.queue);
        world.insert_resource(pending.watcher);
        world.insert_resource(AssetWatchRuntimeStatus::default());
        Ok(())
    }
}

fn validate_watch_replacement(world: &mut nara_ecs::World) -> Result<(), AssetWatchError> {
    fn admit<R: nara_ecs::Resource>(world: &mut nara_ecs::World) -> Result<(), AssetWatchError> {
        world.register_component::<R>();
        let result = if world.contains_resource::<R>() {
            nara_ecs::__private::validate_resource_scope::<R>(world)
        } else {
            nara_ecs::__private::validate_resource_insertion::<R>(world)
        };
        result.map_err(|_| AssetWatchError::RescanRequired)
    }
    admit::<AssetWatchEventQueue>(world)?;
    admit::<queue::AssetWatchQueueObserver>(world)?;
    admit::<AssetWatcher>(world)?;
    admit::<AssetWatchRuntimeStatus>(world)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetWatchPlugin {
    root: PathBuf,
    queue_limits: AssetWatchQueueLimits,
}

pub const ASSET_WATCH_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.asset-watch");
pub const ASSET_WATCH_SHUTDOWN_OBLIGATION: nara_app::PluginShutdownObligationId =
    nara_app::PluginShutdownObligationId::new("nara.asset-watch.watcher");
const ASSET_WATCH_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("asset-watch")];
pub const ASSET_WATCH_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(ASSET_WATCH_PLUGIN_ID, nara_app::PluginCategory::Asset)
        .requires_plugins(&[
            nara_asset::ASSET_PLUGIN_ID,
            nara_diagnostic::DIAGNOSTICS_PLUGIN_ID,
        ])
        .requires_product_capabilities(ASSET_WATCH_PRODUCT_REQUIREMENTS)
        .shutdown_obligations(&[ASSET_WATCH_SHUTDOWN_OBLIGATION]);

impl AssetWatchPlugin {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            queue_limits: AssetWatchQueueLimits::default(),
        }
    }

    #[must_use]
    pub const fn with_queue_limits(mut self, queue_limits: AssetWatchQueueLimits) -> Self {
        self.queue_limits = queue_limits;
        self
    }
}

impl Plugin for AssetWatchPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &ASSET_WATCH_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let watch_root = if app.world().contains_resource::<AssetSourceRoot>() {
            let existing_root = app
                .world()
                .resource::<AssetSourceRoot>()
                .root()
                .to_path_buf();
            if !same_lexical_path(&existing_root, &self.root).map_err(|error| {
                PluginError::SetupFailed {
                    plugin: ASSET_WATCH_PLUGIN_ID,
                    message: error.to_string(),
                }
            })? {
                return Err(PluginError::SetupFailed {
                    plugin: ASSET_WATCH_PLUGIN_ID,
                    message: format!(
                        "asset watch root '{}' does not match AssetSourceRoot '{}'",
                        self.root.display(),
                        existing_root.display()
                    ),
                });
            }
            existing_root
        } else {
            app.insert_resource(AssetSourceRoot::new(self.root.clone()))?;
            self.root.clone()
        };

        let queue = AssetWatchEventQueue::with_limits(self.queue_limits);
        let observer = queue.observer();
        let sender = queue.sender();
        let watcher = AssetWatcher::watch_recursive(&watch_root, sender).map_err(|error| {
            PluginError::SetupFailed {
                plugin: ASSET_WATCH_PLUGIN_ID,
                message: error.to_string(),
            }
        })?;
        app.insert_resource(observer)?;
        app.insert_resource(AssetWatchRuntimeStatus::default())?;
        app.insert_resource(queue)?;
        app.insert_resource(watcher)?;
        app.add_systems(
            CoreStage::TaskUpdate,
            drain_asset_watch_events.in_set(AssetTaskUpdateSet::Poll),
        )?;
        app.register_plugin_shutdown_obligation(ASSET_WATCH_SHUTDOWN_OBLIGATION)?;
        Ok(())
    }

    fn shutdown(
        &self,
        context: &mut nara_app::PluginShutdownContext<'_>,
    ) -> Result<(), PluginError> {
        context.world_mut().remove_resource::<ActiveWatchRescan>();
        context.world_mut().remove_resource::<AssetWatcher>();
        Ok(())
    }
}

#[cfg(test)]
mod tests;
