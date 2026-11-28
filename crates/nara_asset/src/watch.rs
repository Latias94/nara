//! Native source notifications and opt-in refresh of loaded PNG assets.

use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use nara_app::{
    App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginShutdownContext,
};
use nara_ecs::{Res, ResMut, Resource, bevy_ecs::error::Result};
use notify::{
    Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{AccessKind, AccessMode, ModifyKind},
};

use crate::image::{IMAGE_PLUGIN_ID, ImageAsset, ImageSourceDirectory};
use crate::{AssetEvents, AssetServer, AssetStates, Assets, LoadState};

const QUIET_INTERVAL: Duration = Duration::from_millis(20);
const MAX_BATCH_INTERVAL: Duration = Duration::from_millis(100);
const REMOUNT_RETRY_INTERVAL: Duration = Duration::from_millis(200);

/// Backoff for a single revision blocked by a sharing error or an incomplete asset pair.
pub const RELOAD_RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(40),
    Duration::from_millis(80),
    Duration::from_millis(160),
];

/// Reports filesystem failures that may resolve without another source notification.
pub fn is_transient_reload_error(mut error: &(dyn Error + 'static)) -> bool {
    loop {
        #[cfg(feature = "authoring")]
        if matches!(
            error.downcast_ref::<crate::AssetScanError>(),
            Some(
                crate::AssetScanError::MissingSidecar { .. }
                    | crate::AssetScanError::OrphanSidecar { .. }
            )
        ) {
            return true;
        }
        #[cfg(windows)]
        if error
            .downcast_ref::<io::Error>()
            .is_some_and(|error| matches!(error.raw_os_error(), Some(32 | 33)))
        {
            return true;
        }
        let Some(source) = error.source() else {
            return false;
        };
        error = source;
    }
}

/// Watches a native asset directory and coalesces changes until it is quiet or the batch expires.
/// Dropping the watcher stops observation.
#[derive(Resource)]
pub struct AssetWatcher {
    root: PathBuf,
    changes: Arc<Mutex<PendingChange>>,
    watches: Mutex<DirectoryWatches>,
}

impl AssetWatcher {
    pub fn new(directory: impl AsRef<Path>) -> notify::Result<Self> {
        let directory = std::path::absolute(directory).map_err(notify::Error::io)?;
        let mut anchor = directory.parent().unwrap_or(&directory);
        loop {
            match fs::canonicalize(anchor) {
                Ok(canonical) => {
                    return Self::start(canonical, directory_suffix(&directory, anchor)?);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    anchor = anchor.parent().ok_or_else(|| notify::Error::io(error))?;
                }
                Err(error) => return Err(notify::Error::io(error).add_path(anchor.to_owned())),
            }
        }
    }

    /// Watches a configured asset directory below a project root, even before it exists.
    /// Only the asset directory is watched recursively. The directories leading to it are
    /// watched shallowly so that creating or replacing any part of the path is observed.
    pub fn under(project_root: &Path, directory: &Path) -> notify::Result<Self> {
        let suffix = directory_suffix(directory, project_root)?;
        if suffix.as_os_str().is_empty() {
            return Err(notify::Error::generic(
                "the asset directory must be below the project root",
            )
            .add_path(directory.to_owned()));
        }
        let anchor = fs::canonicalize(project_root)
            .map_err(|error| notify::Error::io(error).add_path(project_root.to_owned()))?;
        Self::start(anchor, suffix)
    }

    fn start(anchor: PathBuf, suffix: &Path) -> notify::Result<Self> {
        let root = anchor.join(suffix);
        let changes = Arc::new(Mutex::new(PendingChange::default()));
        let signal = Arc::clone(&changes);
        let target = root.clone();
        let mut watcher = RecommendedWatcher::new(
            move |event| {
                if let Some(remount) = relevant_change(event, &target) {
                    signal.lock().unwrap().record(Instant::now(), remount);
                }
            },
            Config::default().with_follow_symlinks(false),
        )?;
        let mode = if root == anchor {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        watcher.watch(&anchor, mode)?;
        let mut watches = DirectoryWatches {
            anchor,
            mounted: Vec::new(),
            watcher,
        };
        watches.remount(&root)?;
        Ok(Self {
            root,
            changes,
            watches: Mutex::new(watches),
        })
    }

    /// Checks for a ready batch without consuming it or touching the filesystem.
    pub fn change_is_ready(&self) -> bool {
        self.changes.lock().unwrap().ready(Instant::now())
    }

    /// Takes one ready batch. Notifications received after the take form the next batch.
    /// A directory creation or replacement may reconfigure native watches here. A failed
    /// remount leaves a batch pending for retry.
    pub fn take_changed(&self) -> notify::Result<bool> {
        let mut watches = self.watches.lock()?;
        let change = self.changes.lock().unwrap().take(Instant::now());
        if change == Some(true)
            && let Err(error) = watches.remount(&self.root)
        {
            self.changes.lock().unwrap().retry_remount(Instant::now());
            return Err(error);
        }
        Ok(change.is_some())
    }
}

#[derive(Default)]
struct PendingChange {
    batch: Option<ChangeBatch>,
    retry_at: Option<Instant>,
}

struct ChangeBatch {
    started: Instant,
    deadline: Instant,
    remount: bool,
}

impl PendingChange {
    fn record(&mut self, now: Instant, remount: bool) {
        let batch = self.batch.get_or_insert(ChangeBatch {
            started: now,
            deadline: now,
            remount: false,
        });
        batch.deadline = (now + QUIET_INTERVAL).min(batch.started + MAX_BATCH_INTERVAL);
        batch.remount |= remount;
    }

    fn retry_remount(&mut self, now: Instant) {
        self.record(now, true);
        self.retry_at = Some(now + REMOUNT_RETRY_INTERVAL);
    }

    fn ready(&self, now: Instant) -> bool {
        self.batch
            .as_ref()
            .is_some_and(|batch| now >= batch.deadline)
            && self.retry_at.is_none_or(|retry_at| now >= retry_at)
    }

    fn take(&mut self, now: Instant) -> Option<bool> {
        if !self.ready(now) {
            return None;
        }
        self.retry_at = None;
        self.batch.take().map(|batch| batch.remount)
    }
}

fn relevant_change(event: notify::Result<notify::Event>, root: &Path) -> Option<bool> {
    let (paths, topology) = match event {
        Ok(event) if event.need_rescan() => return Some(true),
        Ok(event) => {
            if event.kind.is_access()
                && event.kind != EventKind::Access(AccessKind::Close(AccessMode::Write))
            {
                return None;
            }
            let topology = matches!(
                event.kind,
                EventKind::Create(_)
                    | EventKind::Remove(_)
                    | EventKind::Modify(ModifyKind::Name(_))
            );
            (event.paths, topology)
        }
        Err(error) if error.paths.is_empty() => return Some(true),
        Err(error) => (error.paths, true),
    };
    if topology && paths.iter().any(|path| root.starts_with(path)) {
        Some(true)
    } else if paths.iter().any(|path| {
        path.strip_prefix(root).is_ok_and(|relative| {
            !relative.as_os_str().is_empty()
                && relative
                    .components()
                    .all(|part| !part.as_os_str().as_encoded_bytes().starts_with(b"."))
        })
    }) {
        Some(false)
    } else {
        None
    }
}

struct DirectoryWatches {
    anchor: PathBuf,
    mounted: Vec<PathBuf>,
    watcher: RecommendedWatcher,
}

impl DirectoryWatches {
    fn remount(&mut self, root: &Path) -> notify::Result<()> {
        for path in self.mounted.drain(..).rev() {
            let _ = self.watcher.unwatch(&path);
        }
        let mut path = self.anchor.clone();
        for component in directory_suffix(root, &self.anchor)?.components() {
            path.push(component);
            if !watchable_directory(&path)? {
                break;
            }
            let mode = if path == root {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            if let Err(error) = self.watcher.watch(&path, mode) {
                if !watchable_directory(&path)? {
                    break;
                }
                return Err(error.add_path(path));
            }
            self.mounted.push(path.clone());
        }
        Ok(())
    }
}

fn directory_suffix<'a>(directory: &'a Path, anchor: &Path) -> notify::Result<&'a Path> {
    directory
        .strip_prefix(anchor)
        .ok()
        .filter(|suffix| {
            suffix
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        })
        .ok_or_else(|| {
            notify::Error::generic("the asset directory must be within its watch root")
                .add_path(directory.to_owned())
        })
}

fn watchable_directory(path: &Path) -> notify::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.is_dir() && !metadata.file_type().is_symlink()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(notify::Error::io(error).add_path(path.to_owned())),
    }
}

/// Refreshes loaded source images after a native change and retains usable pixels on failure.
/// Insert [`ImageSourceDirectory`] before adding this plugin to select a directory other than
/// `assets`.
#[derive(Debug, Default, Clone, Copy)]
pub struct AssetWatchPlugin;

pub const ASSET_WATCH_PLUGIN_ID: PluginId = PluginId::new("nara.asset-watch");
pub const ASSET_WATCH_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(ASSET_WATCH_PLUGIN_ID, PluginCategory::Asset)
        .requires_plugins(&[IMAGE_PLUGIN_ID]);

impl Plugin for AssetWatchPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &ASSET_WATCH_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> std::result::Result<(), PluginError> {
        if !app.world().contains_resource::<ImageSourceDirectory>() {
            let directory = ImageSourceDirectory::new("assets").map_err(watch_setup_error)?;
            app.insert_resource(directory)?;
        }
        let directory = app.world().resource::<ImageSourceDirectory>();
        let watcher = AssetWatcher::new(directory.path()).map_err(watch_setup_error)?;
        app.insert_resource(watcher)?;
        app.add_systems(CoreStage::TaskUpdate, refresh_images)?;
        Ok(())
    }

    fn shutdown(
        &self,
        context: &mut PluginShutdownContext<'_>,
    ) -> std::result::Result<(), PluginError> {
        context.world_mut().remove_resource::<AssetWatcher>();
        Ok(())
    }
}

fn watch_setup_error(error: impl std::fmt::Display) -> PluginError {
    PluginError::SetupFailed {
        plugin: ASSET_WATCH_PLUGIN_ID,
        message: error.to_string(),
    }
}

fn refresh_images(
    watcher: Res<AssetWatcher>,
    directory: Res<ImageSourceDirectory>,
    server: Res<AssetServer>,
    mut images: ResMut<Assets<ImageAsset>>,
    mut states: ResMut<AssetStates>,
    mut events: ResMut<AssetEvents>,
) -> Result {
    if watcher.root != directory.path() {
        return Ok(());
    }
    match watcher.take_changed() {
        Ok(true) => {}
        Ok(false) => return Ok(()),
        // On Windows, notify may report a directory it cannot open as missing.
        Err(error) if matches!(error.kind, notify::ErrorKind::PathNotFound) => return Ok(()),
        Err(error) if matches!(&error.kind, notify::ErrorKind::Io(source) if is_transient_reload_error(source)) =>
        {
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    }
    let loaded: Vec<_> = images
        .iter()
        .filter_map(|(handle, image)| {
            let path = server.path(handle.id())?;
            let state = states.state(handle.id())?;
            (states.published_slot_revision(handle.id()) == Some(&images.slot_revision(handle)))
                .then_some((
                    handle,
                    path,
                    image.color_space(),
                    image.content_hash(),
                    state.load_state() == &LoadState::Loaded,
                ))
        })
        .collect();
    for (handle, path, color_space, old_hash, was_loaded) in loaded {
        match directory.read_image(path, color_space) {
            Ok(image) => {
                if image.content_hash() != old_hash || !was_loaded {
                    images.commit_loaded(handle, image, &mut states, &mut events)?;
                }
            }
            Err(error) => {
                images.record_reload_failure(
                    handle,
                    &mut states,
                    &mut events,
                    error.to_string(),
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
