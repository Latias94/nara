//! Native source notifications and opt-in refresh of loaded PNG assets.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use nara_app::{
    App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginShutdownContext,
};
use nara_ecs::{Res, ResMut, Resource, bevy_ecs::error::Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::image::{IMAGE_PLUGIN_ID, ImageAsset, ImageSourceDirectory};
use crate::{AssetEvents, AssetServer, AssetStates, Assets, LoadState};

/// Watches one native directory using `notify`. Dropping the watcher stops observation.
#[derive(Resource)]
pub struct AssetWatcher {
    root: PathBuf,
    changed: Arc<AtomicBool>,
    _watcher: RecommendedWatcher,
}

impl AssetWatcher {
    pub fn new(directory: impl AsRef<Path>) -> notify::Result<Self> {
        let root = directory
            .as_ref()
            .canonicalize()
            .map_err(notify::Error::io)?;
        let changed = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&changed);
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if !event.is_ok_and(|event| event.kind.is_access()) {
                    signal.store(true, Ordering::Relaxed);
                }
            })?;
        watcher.watch(&root, RecursiveMode::Recursive)?;
        Ok(Self {
            root,
            changed,
            _watcher: watcher,
        })
    }

    /// Reports and clears the coalesced change signal.
    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
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
    if !watcher.take_changed() || watcher.root != directory.path() {
        return Ok(());
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
