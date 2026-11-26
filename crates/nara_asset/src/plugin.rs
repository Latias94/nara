use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_ecs::{Mut, World};

use crate::{
    Asset, AssetError, AssetEvents, AssetServer, AssetStateError, AssetStates, Assets, Handle,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct AssetPlugin;

pub const ASSET_PLUGIN_ID: PluginId = PluginId::new("nara.asset");
pub const ASSET_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(ASSET_PLUGIN_ID, PluginCategory::Asset);

impl Plugin for AssetPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &ASSET_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<AssetServer>()?;
        app.init_resource::<AssetStates>()?;
        app.init_resource::<AssetEvents>()?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AddAssetError {
    #[error("install AssetPlugin before adding a runtime asset")]
    MissingAssetPlugin,
    #[error(transparent)]
    Reserve(#[from] AssetError),
    #[error(transparent)]
    Publish(#[from] AssetStateError),
}

/// Makes a procedural asset ready for immediate use by the game and renderer.
pub fn add_asset<T: Asset>(world: &mut World, value: T) -> Result<Handle<T>, AddAssetError> {
    ensure_asset_plugin(world)?;
    let handle = world
        .resource_mut::<AssetServer>()
        .reserve_anonymous::<T>()?;
    publish_asset(world, handle, value)?;
    Ok(handle)
}

pub(crate) fn ensure_asset_plugin(world: &World) -> Result<(), AddAssetError> {
    if !world.contains_resource::<AssetServer>()
        || !world.contains_resource::<AssetStates>()
        || !world.contains_resource::<AssetEvents>()
    {
        return Err(AddAssetError::MissingAssetPlugin);
    }
    Ok(())
}

pub(crate) fn publish_asset<T: Asset>(
    world: &mut World,
    handle: Handle<T>,
    value: T,
) -> Result<(), AssetStateError> {
    world.init_resource::<Assets<T>>();
    world.resource_scope(|world, mut assets: Mut<Assets<T>>| {
        world.resource_scope(|world, mut states: Mut<AssetStates>| {
            assets.commit_loaded(
                handle,
                value,
                &mut states,
                &mut world.resource_mut::<AssetEvents>(),
            )
        })
    })?;
    Ok(())
}
