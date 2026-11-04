use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::{AssetStates, Assets};
use nara_ecs::{Res, ResMut, Resource};
use nara_render::{RenderResourceKind, RenderResourceSnapshot};

use super::{
    PreparedImageResource, PreparedImageSnapshot, PreparedImages, image_content_hash,
    image_resource_key,
};
use crate::ImageAsset;

#[derive(Debug, Default, Resource)]
pub struct ImagePrepareStats {
    pub prepared: u32,
    pub removed: u32,
    pub skipped_missing_state: u32,
    pub skipped_unavailable: u32,
}

#[derive(Debug, Default)]
pub struct ImagePreparePlugin;

pub const IMAGE_PREPARE_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.image.prepare");
const IMAGE_PREPARE_PRODUCT_ALTERNATIVES: &[nara_app::PluginProductCapability] = &[
    nara_app::PluginProductCapability::new("presentation-2d"),
    nara_app::PluginProductCapability::new("presentation-ui"),
];
pub const IMAGE_PREPARE_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(IMAGE_PREPARE_PLUGIN_ID, nara_app::PluginCategory::Render)
        .requires_any_product_capabilities(IMAGE_PREPARE_PRODUCT_ALTERNATIVES);

impl Plugin for ImagePreparePlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &IMAGE_PREPARE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Assets<ImageAsset>>()?;
        app.init_resource::<AssetStates>()?;
        app.init_resource::<PreparedImages>()?;
        app.init_resource::<ImagePrepareStats>()?;
        app.add_systems(CoreStage::Prepare, prepare_images)?;
        Ok(())
    }
}

pub fn prepare_images(
    images: Res<Assets<ImageAsset>>,
    states: Res<AssetStates>,
    mut prepared_images: ResMut<PreparedImages>,
    mut stats: ResMut<ImagePrepareStats>,
) {
    *stats = ImagePrepareStats::default();
    let removed_keys = prepared_images
        .keys()
        .filter(|key| key.kind() == RenderResourceKind::IMAGE_2D)
        .filter(|key| {
            !images.contains_id(key.asset_id())
                || states
                    .state(key.asset_id())
                    .is_none_or(|state| state.usable_version().is_none())
        })
        .collect::<Vec<_>>();
    for key in removed_keys {
        if prepared_images.remove(key).is_some() {
            stats.removed += 1;
        }
    }

    for (handle, image) in images.iter() {
        let Some(state) = states.state(handle.id()) else {
            stats.skipped_missing_state += 1;
            continue;
        };
        let Some(version) = state.usable_version() else {
            stats.skipped_unavailable += 1;
            continue;
        };

        let key = image_resource_key(handle);
        let snapshot = RenderResourceSnapshot::new(
            key,
            version,
            images.slot_revision(handle),
            image_content_hash(image),
        );

        if prepared_images
            .get(key)
            .is_some_and(|prepared| prepared.identity() == &snapshot)
        {
            continue;
        }

        prepared_images.insert(PreparedImageSnapshot {
            identity: snapshot,
            resource: PreparedImageResource::from_image(image),
        });
        stats.prepared += 1;
    }
}
