//! Optional filesystem watcher adapter for nara assets.

use std::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
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
    QueueUnavailable,
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
            Self::QueueUnavailable => write!(formatter, "asset watch queue is unavailable"),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetWatchDiagnosticKind {
    Backend,
    Queue,
    Translation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetWatchDiagnostic {
    kind: AssetWatchDiagnosticKind,
    message: String,
    logical_path: Option<AssetPath>,
    context: Option<String>,
}

impl AssetWatchDiagnostic {
    #[must_use]
    pub fn from_error(error: &AssetWatchError) -> Self {
        match error {
            AssetWatchError::SourceOutsideRoot { .. } => Self::new(
                AssetWatchDiagnosticKind::Translation,
                "asset watch path is outside the source root",
            )
            .with_context("outside-root"),
            AssetWatchError::NonUtf8Path(_) => Self::new(
                AssetWatchDiagnosticKind::Translation,
                "asset watch path is not UTF-8",
            )
            .with_context("non-utf8-path"),
            AssetWatchError::InvalidLogicalPath(error) => Self::new(
                AssetWatchDiagnosticKind::Translation,
                format!("asset watch logical path is invalid: {error}"),
            ),
            AssetWatchError::MissingRenameTarget(_) => Self::new(
                AssetWatchDiagnosticKind::Translation,
                "asset watch rename event is missing a target path",
            )
            .with_context("missing-rename-target"),
            AssetWatchError::Filesystem(_) => Self::new(
                AssetWatchDiagnosticKind::Backend,
                "asset watch filesystem error",
            ),
            AssetWatchError::Notify(_) => Self::new(
                AssetWatchDiagnosticKind::Backend,
                "asset watch backend error",
            ),
            AssetWatchError::QueueUnavailable => Self::new(
                AssetWatchDiagnosticKind::Queue,
                "asset watch queue is unavailable",
            ),
        }
    }

    #[must_use]
    pub fn new(kind: AssetWatchDiagnosticKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            logical_path: None,
            context: None,
        }
    }

    #[must_use]
    pub fn with_logical_path(mut self, path: AssetPath) -> Self {
        self.logical_path = Some(path);
        self
    }

    #[must_use]
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    #[must_use]
    pub const fn kind(&self) -> AssetWatchDiagnosticKind {
        self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn logical_path(&self) -> Option<&AssetPath> {
        self.logical_path.as_ref()
    }

    #[must_use]
    pub fn context(&self) -> Option<&str> {
        self.context.as_deref()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
pub struct AssetWatchDiagnostics {
    diagnostics: Vec<AssetWatchDiagnostic>,
}

impl AssetWatchDiagnostics {
    pub fn push(&mut self, diagnostic: AssetWatchDiagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn push_error(&mut self, error: &AssetWatchError) {
        self.push(AssetWatchDiagnostic::from_error(error));
    }

    pub fn clear(&mut self) {
        self.diagnostics.clear();
    }

    #[must_use]
    pub fn as_slice(&self) -> &[AssetWatchDiagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetWatchQueueItem {
    Event(AssetWatchEvent),
    Error(AssetWatchError),
}

#[derive(Clone, Default, Resource)]
pub struct AssetWatchEventQueue {
    items: Arc<Mutex<Vec<AssetWatchQueueItem>>>,
    lock_failed: Arc<AtomicBool>,
}

impl AssetWatchEventQueue {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, event: AssetWatchEvent) -> Result<(), AssetWatchError> {
        self.push_item(AssetWatchQueueItem::Event(event))
    }

    pub fn push_error(&self, error: AssetWatchError) -> Result<(), AssetWatchError> {
        self.push_item(AssetWatchQueueItem::Error(error))
    }

    fn push_item(&self, item: AssetWatchQueueItem) -> Result<(), AssetWatchError> {
        let Ok(mut items) = self.items.lock() else {
            self.lock_failed.store(true, Ordering::Relaxed);
            return Err(AssetWatchError::QueueUnavailable);
        };
        items.push(item);
        Ok(())
    }

    pub fn drain(&self) -> Result<Vec<AssetWatchQueueItem>, AssetWatchError> {
        if self.lock_failed.swap(false, Ordering::Relaxed) {
            return Err(AssetWatchError::QueueUnavailable);
        }
        self.items
            .lock()
            .map(|mut items| std::mem::take(&mut *items))
            .map_err(|_| AssetWatchError::QueueUnavailable)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.lock().map_or(0, |items| items.len())
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
            .field("lock_failed", &self.lock_failed.load(Ordering::Relaxed))
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
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<Event>| match result {
                Ok(event) => {
                    for translated in watch_events_from_notify(event) {
                        let _ = queue_for_callback.push(translated);
                    }
                }
                Err(error) => {
                    let _ =
                        queue_for_callback.push_error(AssetWatchError::Notify(error.to_string()));
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
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.asset-watch"),
            nara_app::PluginCategory::Asset,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(AssetPlugin)?;
        let watch_root = if app.world().contains_resource::<AssetSourceRoot>() {
            let existing_root = app
                .world()
                .resource::<AssetSourceRoot>()
                .root()
                .to_path_buf();
            if !same_lexical_path(&existing_root, &self.root).map_err(|error| {
                PluginError::SetupFailed {
                    plugin: self.plugin_id(),
                    message: error.to_string(),
                }
            })? {
                return Err(PluginError::SetupFailed {
                    plugin: self.plugin_id(),
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

        let queue = AssetWatchEventQueue::new();
        app.init_resource::<AssetWatchDiagnostics>()?;
        let watcher =
            AssetWatcher::watch_recursive(&watch_root, queue.clone()).map_err(|error| {
                PluginError::SetupFailed {
                    plugin: self.plugin_id(),
                    message: error.to_string(),
                }
            })?;
        app.insert_resource(queue)?;
        app.insert_resource(watcher)?;
        app.add_systems(
            CoreStage::TaskUpdate,
            drain_asset_watch_events.in_set(TaskUpdateSet::Poll),
        )?;
        Ok(())
    }
}

fn drain_asset_watch_events(
    queue: Res<AssetWatchEventQueue>,
    root: Res<AssetSourceRoot>,
    mut changes: ResMut<AssetSourceChanges>,
    mut diagnostics: ResMut<AssetWatchDiagnostics>,
) {
    diagnostics.clear();
    let translator = AssetWatchTranslator;
    let items = match queue.drain() {
        Ok(items) => items,
        Err(error) => {
            diagnostics.push_error(&error);
            return;
        }
    };
    for item in items {
        match item {
            AssetWatchQueueItem::Event(event) => match translator.translate_event(&root, &event) {
                Ok(translated) => {
                    for change in translated {
                        changes.push(change);
                    }
                }
                Err(error) => diagnostics.push_error(&error),
            },
            AssetWatchQueueItem::Error(error) => {
                diagnostics.push_error(&error);
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

fn same_lexical_path(left: &Path, right: &Path) -> Result<bool, AssetWatchError> {
    Ok(absolute_lexical(left)? == absolute_lexical(right)?)
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
    use nara_asset::{
        AssetRecord, AssetReloadRequestKind, AssetReloadRequests, AssetSourceKind,
        ProjectAssetDatabase, StableAssetId,
    };
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
    fn meta_remove_maps_to_source_remove_change() {
        let root = temp_root();
        fs::create_dir_all(root.join("textures")).unwrap();
        let meta = root.join("textures").join("player.png.meta");
        let translator = AssetWatchTranslator;

        let changes = translator
            .translate_event(
                &AssetSourceRoot::new(&root),
                &AssetWatchEvent::removed(&meta),
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
    fn rename_from_root_to_outside_keeps_in_root_remove() {
        let root = temp_root();
        fs::create_dir_all(root.join("textures")).unwrap();
        let from = root.join("textures").join("old.png");
        let outside = root.with_file_name("outside.png");
        let translator = AssetWatchTranslator;

        let changes = translator
            .translate_event(
                &AssetSourceRoot::new(&root),
                &AssetWatchEvent::renamed(&from, &outside),
            )
            .unwrap();

        assert_eq!(
            changes,
            vec![AssetSourceChange::new(
                AssetPath::new("textures/old.png").unwrap(),
                AssetSourceChangeKind::Removed
            )]
        );
        remove_temp_root(&root);
    }

    #[test]
    fn rename_from_outside_to_root_keeps_in_root_modify() {
        let root = temp_root();
        fs::create_dir_all(root.join("textures")).unwrap();
        let outside = root.with_file_name("outside.png");
        let to = root.join("textures").join("new.png");
        let translator = AssetWatchTranslator;

        let changes = translator
            .translate_event(
                &AssetSourceRoot::new(&root),
                &AssetWatchEvent::renamed(&outside, &to),
            )
            .unwrap();

        assert_eq!(
            changes,
            vec![AssetSourceChange::new(
                AssetPath::new("textures/new.png").unwrap(),
                AssetSourceChangeKind::Modified
            )]
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
        queue.push(AssetWatchEvent::modified(&source)).unwrap();
        let mut changes = AssetSourceChanges::new();

        let translator = AssetWatchTranslator;
        for item in queue.drain().unwrap() {
            if let AssetWatchQueueItem::Event(event) = item {
                for change in translator
                    .translate_event(&AssetSourceRoot::new(&root), &event)
                    .unwrap()
                {
                    changes.push(change);
                }
            }
        }

        assert_eq!(changes.len(), 1);
        assert!(queue.is_empty());
        remove_temp_root(&root);
    }

    #[test]
    fn queued_watch_events_are_resolved_in_the_same_task_update() {
        let root = temp_root();
        fs::create_dir_all(root.join("textures")).unwrap();
        let source = root.join("textures").join("player.png");
        fs::write(&source, b"png").unwrap();
        let record = AssetRecord::new(
            stable_id(),
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceKind::Image,
        );
        let queue = AssetWatchEventQueue::new();
        queue.push(AssetWatchEvent::modified(&source)).unwrap();
        let mut app = App::new();
        app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
        app.insert_resource(queue).unwrap();
        app.insert_resource(AssetWatchDiagnostics::default())
            .unwrap();
        app.add_plugin(AssetPlugin).unwrap();
        app.world_mut()
            .unwrap()
            .resource_mut::<ProjectAssetDatabase>()
            .insert(record)
            .unwrap();
        app.add_systems(
            CoreStage::TaskUpdate,
            drain_asset_watch_events.in_set(TaskUpdateSet::Poll),
        )
        .unwrap();

        app.update().unwrap();

        let requests = app.world().resource::<AssetReloadRequests>();
        let request = requests.iter().next().unwrap();
        assert_eq!(request.path().as_str(), "textures/player.png");
        assert_eq!(request.request_kind(), AssetReloadRequestKind::LoadOrReload);
        remove_temp_root(&root);
    }

    #[test]
    fn notify_backend_error_is_visible_in_watch_diagnostics() {
        let root = temp_root();
        fs::create_dir_all(&root).unwrap();
        let queue = AssetWatchEventQueue::new();
        queue
            .push_error(AssetWatchError::Notify("backend failed".to_string()))
            .unwrap();
        let mut app = App::new();
        app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
        app.insert_resource(queue).unwrap();
        app.insert_resource(AssetWatchDiagnostics::default())
            .unwrap();
        app.add_plugin(AssetPlugin).unwrap();
        app.add_systems(
            CoreStage::TaskUpdate,
            drain_asset_watch_events.in_set(TaskUpdateSet::Poll),
        )
        .unwrap();

        app.update().unwrap();

        let diagnostics = app.world().resource::<AssetWatchDiagnostics>();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics.as_slice()[0].kind(),
            AssetWatchDiagnosticKind::Backend
        );
        remove_temp_root(&root);
    }

    #[test]
    fn translation_failure_records_diagnostic_without_source_change() {
        let root = temp_root();
        fs::create_dir_all(&root).unwrap();
        let outside = root.with_file_name("outside.png");
        let queue = AssetWatchEventQueue::new();
        queue.push(AssetWatchEvent::modified(&outside)).unwrap();
        let mut app = App::new();
        app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
        app.insert_resource(queue).unwrap();
        app.insert_resource(AssetWatchDiagnostics::default())
            .unwrap();
        app.add_plugin(AssetPlugin).unwrap();
        app.add_systems(
            CoreStage::TaskUpdate,
            drain_asset_watch_events.in_set(TaskUpdateSet::Poll),
        )
        .unwrap();

        app.update().unwrap();

        assert!(app.world().resource::<AssetSourceChanges>().is_empty());
        let diagnostics = app.world().resource::<AssetWatchDiagnostics>();
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics.as_slice()[0];
        assert_eq!(diagnostic.kind(), AssetWatchDiagnosticKind::Translation);
        let outside_path = outside.display().to_string();
        assert!(!diagnostic.message().contains(&outside_path));
        assert!(
            diagnostic
                .context()
                .is_none_or(|context| !context.contains(&outside_path))
        );
        remove_temp_root(&root);
    }

    #[test]
    fn multiple_watch_diagnostics_are_visible_in_one_frame() {
        let root = temp_root();
        fs::create_dir_all(&root).unwrap();
        let queue = AssetWatchEventQueue::new();
        queue
            .push_error(AssetWatchError::Notify("first".to_string()))
            .unwrap();
        queue
            .push_error(AssetWatchError::Notify("second".to_string()))
            .unwrap();
        let mut app = App::new();
        app.insert_resource(AssetSourceRoot::new(&root)).unwrap();
        app.insert_resource(queue).unwrap();
        app.insert_resource(AssetWatchDiagnostics::default())
            .unwrap();
        app.add_plugin(AssetPlugin).unwrap();
        app.add_systems(
            CoreStage::TaskUpdate,
            drain_asset_watch_events.in_set(TaskUpdateSet::Poll),
        )
        .unwrap();

        app.update().unwrap();

        assert_eq!(app.world().resource::<AssetWatchDiagnostics>().len(), 2);
        remove_temp_root(&root);
    }

    #[test]
    fn plugin_rejects_root_that_differs_from_existing_asset_source_root() {
        let configured_root = temp_root();
        let watch_root = configured_root.with_file_name("nara_asset_watch_other_root");
        fs::create_dir_all(&configured_root).unwrap();
        let mut app = App::new();
        app.insert_resource(AssetSourceRoot::new(&configured_root))
            .unwrap();

        let Err(error) = app.add_plugin(AssetWatchPlugin::new(&watch_root)) else {
            panic!("watch plugin should reject a root that differs from AssetSourceRoot");
        };

        assert!(matches!(error, PluginError::SetupFailed { .. }));
        remove_temp_root(&configured_root);
    }

    fn stable_id() -> StableAssetId {
        StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap()
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
