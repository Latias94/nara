//! Optional filesystem watcher adapter for nara assets.

use std::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

use nara_app::{App, CoreStage, Plugin, PluginError, TaskUpdateSet};
use nara_asset::{
    AssetPath, AssetPathError, AssetPlugin, AssetSourceChange, AssetSourceChangeKind,
    AssetSourceChanges, AssetSourceRoot,
};
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher, event::ModifyKind,
    event::RenameMode,
};

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
}

impl AssetWatchEvent {
    #[must_use]
    pub fn modified(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: AssetWatchEventKind::Modified,
            path: path.into(),
            to_path: None,
        }
    }

    #[must_use]
    pub fn removed(path: impl Into<PathBuf>) -> Self {
        Self {
            kind: AssetWatchEventKind::Removed,
            path: path.into(),
            to_path: None,
        }
    }

    #[must_use]
    pub fn renamed(from: impl Into<PathBuf>, to: impl Into<PathBuf>) -> Self {
        Self {
            kind: AssetWatchEventKind::Renamed,
            path: from.into(),
            to_path: Some(to.into()),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> AssetWatchEventKind {
        self.kind
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn to_path(&self) -> Option<&Path> {
        self.to_path.as_deref()
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
                Ok(vec![
                    self.translate_path(root, event.path(), AssetSourceChangeKind::Removed)?,
                    self.translate_path(root, to_path, AssetSourceChangeKind::Modified)?,
                ])
            }
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
            AssetSourceChangeKind::MetaModified
        } else {
            source_kind
        };
        Ok(AssetSourceChange::new(logical, kind))
    }
}

#[derive(Clone, Default, Resource)]
pub struct AssetWatchEventQueue {
    events: Arc<Mutex<Vec<AssetWatchEvent>>>,
}

impl AssetWatchEventQueue {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, event: AssetWatchEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }

    pub fn drain(&self) -> Vec<AssetWatchEvent> {
        self.events
            .lock()
            .map_or_else(|_| Vec::new(), |mut events| std::mem::take(&mut *events))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.events.lock().map_or(0, |events| events.len())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Debug for AssetWatchEventQueue {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetWatchEventQueue")
            .field("len", &self.len())
            .finish()
    }
}

#[derive(Resource)]
pub struct AssetWatcher {
    _watcher: RecommendedWatcher,
}

impl AssetWatcher {
    pub fn watch_recursive(
        root: impl AsRef<Path>,
        queue: AssetWatchEventQueue,
    ) -> Result<Self, AssetWatchError> {
        let queue_for_callback = queue.clone();
        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            if let Ok(event) = result {
                for translated in watch_events_from_notify(event) {
                    queue_for_callback.push(translated);
                }
            }
        })
        .map_err(|error| AssetWatchError::Notify(error.to_string()))?;
        watcher
            .watch(root.as_ref(), RecursiveMode::Recursive)
            .map_err(|error| AssetWatchError::Notify(error.to_string()))?;
        Ok(Self { _watcher: watcher })
    }
}

impl Debug for AssetWatcher {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetWatcher")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetWatchPlugin {
    root: PathBuf,
}

impl AssetWatchPlugin {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl Plugin for AssetWatchPlugin {
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(AssetPlugin)?;
        if !app.world().contains_resource::<AssetSourceRoot>() {
            app.insert_resource(AssetSourceRoot::new(self.root.clone()));
        }

        let queue = AssetWatchEventQueue::new();
        let watcher =
            AssetWatcher::watch_recursive(&self.root, queue.clone()).map_err(|error| {
                PluginError::SetupFailed {
                    plugin: self.name(),
                    message: error.to_string(),
                }
            })?;
        app.insert_resource(queue);
        app.insert_resource(watcher);
        app.add_systems(
            CoreStage::TaskUpdate,
            drain_asset_watch_events.in_set(TaskUpdateSet::Poll),
        );
        Ok(())
    }
}

fn drain_asset_watch_events(
    queue: Res<AssetWatchEventQueue>,
    root: Res<AssetSourceRoot>,
    mut changes: ResMut<AssetSourceChanges>,
) {
    let translator = AssetWatchTranslator;
    for event in queue.drain() {
        if let Ok(translated) = translator.translate_event(&root, &event) {
            for change in translated {
                changes.push(change);
            }
        }
    }
}

fn watch_events_from_notify(event: Event) -> Vec<AssetWatchEvent> {
    match event.kind {
        EventKind::Remove(_) => event
            .paths
            .into_iter()
            .map(AssetWatchEvent::removed)
            .collect(),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() >= 2 => {
            vec![AssetWatchEvent::renamed(
                event.paths[0].clone(),
                event.paths[1].clone(),
            )]
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => event
            .paths
            .into_iter()
            .map(AssetWatchEvent::removed)
            .collect(),
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => event
            .paths
            .into_iter()
            .map(AssetWatchEvent::modified)
            .collect(),
        EventKind::Create(_) | EventKind::Modify(_) => event
            .paths
            .into_iter()
            .map(AssetWatchEvent::modified)
            .collect(),
        _ => Vec::new(),
    }
}

fn logical_path(root: &Path, path: &Path) -> Result<AssetPath, AssetWatchError> {
    let root = absolute_lexical(root)?;
    let path = if path.is_absolute() {
        normalize_lexical(path)
    } else {
        normalize_lexical(root.join(path))
    };
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| AssetWatchError::SourceOutsideRoot {
            path: path.clone(),
            root: root.clone(),
        })?;

    let mut segments = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(segment) => {
                let Some(segment) = segment.to_str() else {
                    return Err(AssetWatchError::NonUtf8Path(path.clone()));
                };
                segments.push(segment);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AssetWatchError::SourceOutsideRoot {
                    path: path.clone(),
                    root,
                });
            }
        }
    }

    AssetPath::new(segments.join("/")).map_err(AssetWatchError::InvalidLogicalPath)
}

fn absolute_lexical(path: &Path) -> Result<PathBuf, AssetWatchError> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| AssetWatchError::Filesystem(error.to_string()))?
            .join(path)
    };
    Ok(normalize_lexical(path))
}

fn normalize_lexical(path: impl AsRef<Path>) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_root() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "nara_asset_watch_test_{}_{}",
            std::process::id(),
            stamp
        ))
    }

    #[test]
    fn source_modify_translates_to_modified_change() {
        let root = temp_root();
        fs::create_dir_all(root.join("textures")).unwrap();
        let source = root.join("textures").join("player.png");
        fs::write(&source, b"png").unwrap();
        let translator = AssetWatchTranslator;

        let changes = translator
            .translate_event(
                &AssetSourceRoot::new(&root),
                &AssetWatchEvent::modified(&source),
            )
            .unwrap();

        assert_eq!(
            changes,
            vec![AssetSourceChange::new(
                AssetPath::new("textures/player.png").unwrap(),
                AssetSourceChangeKind::Modified
            )]
        );
        remove_temp_root(&root);
    }

    #[test]
    fn meta_modify_maps_to_source_meta_change() {
        let root = temp_root();
        fs::create_dir_all(root.join("textures")).unwrap();
        let meta = root.join("textures").join("player.png.meta");
        fs::write(&meta, b"meta").unwrap();
        let translator = AssetWatchTranslator;

        let changes = translator
            .translate_event(
                &AssetSourceRoot::new(&root),
                &AssetWatchEvent::modified(&meta),
            )
            .unwrap();

        assert_eq!(
            changes,
            vec![AssetSourceChange::new(
                AssetPath::new("textures/player.png").unwrap(),
                AssetSourceChangeKind::MetaModified
            )]
        );
        remove_temp_root(&root);
    }

    #[test]
    fn remove_translates_to_removed_change_without_file_existing() {
        let root = temp_root();
        fs::create_dir_all(root.join("textures")).unwrap();
        let source = root.join("textures").join("player.png");
        let translator = AssetWatchTranslator;

        let changes = translator
            .translate_event(
                &AssetSourceRoot::new(&root),
                &AssetWatchEvent::removed(&source),
            )
            .unwrap();

        assert_eq!(
            changes,
            vec![AssetSourceChange::new(
                AssetPath::new("textures/player.png").unwrap(),
                AssetSourceChangeKind::Removed
            )]
        );
        remove_temp_root(&root);
    }

    #[test]
    fn rename_translates_to_remove_and_modify() {
        let root = temp_root();
        fs::create_dir_all(root.join("textures")).unwrap();
        let from = root.join("textures").join("old.png");
        let to = root.join("textures").join("new.png");
        let translator = AssetWatchTranslator;

        let changes = translator
            .translate_event(
                &AssetSourceRoot::new(&root),
                &AssetWatchEvent::renamed(&from, &to),
            )
            .unwrap();

        assert_eq!(
            changes,
            vec![
                AssetSourceChange::new(
                    AssetPath::new("textures/old.png").unwrap(),
                    AssetSourceChangeKind::Removed
                ),
                AssetSourceChange::new(
                    AssetPath::new("textures/new.png").unwrap(),
                    AssetSourceChangeKind::Modified
                )
            ]
        );
        remove_temp_root(&root);
    }

    #[test]
    fn outside_root_path_is_rejected() {
        let root = temp_root();
        let outside = root.with_file_name("outside.png");
        fs::create_dir_all(&root).unwrap();
        let translator = AssetWatchTranslator;

        let error = translator
            .translate_event(
                &AssetSourceRoot::new(&root),
                &AssetWatchEvent::modified(&outside),
            )
            .unwrap_err();

        assert!(matches!(error, AssetWatchError::SourceOutsideRoot { .. }));
        remove_temp_root(&root);
    }

    #[test]
    fn queue_drains_into_source_changes() {
        let root = temp_root();
        fs::create_dir_all(root.join("textures")).unwrap();
        let source = root.join("textures").join("player.png");
        fs::write(&source, b"png").unwrap();
        let queue = AssetWatchEventQueue::new();
        queue.push(AssetWatchEvent::modified(&source));
        let mut changes = AssetSourceChanges::new();

        let translator = AssetWatchTranslator;
        for event in queue.drain() {
            for change in translator
                .translate_event(&AssetSourceRoot::new(&root), &event)
                .unwrap()
            {
                changes.push(change);
            }
        }

        assert_eq!(changes.len(), 1);
        assert!(queue.is_empty());
        remove_temp_root(&root);
    }

    fn remove_temp_root(path: &Path) {
        if let Err(error) = fs::remove_dir_all(path) {
            panic!(
                "failed to remove temp test directory {}: {error}",
                path.display()
            );
        }
    }
}
