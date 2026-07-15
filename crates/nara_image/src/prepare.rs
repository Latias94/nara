use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::{AssetStates, Assets, Handle, ImportArtifactDigest, LoadState, SourceHash};
use nara_ecs::{Res, ResMut, Resource};
use nara_render::{
    PreparedRenderResources, RenderPrepareApplyResult, RenderPrepareInvalidationReason,
    RenderPrepareInvalidations, RenderResourceKey, RenderResourceKind, RenderResourceSnapshot,
};

use crate::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedImageResource {
    extent: ImageExtent,
    format: ImageFormat,
    color_space: ImageColorSpace,
    source_hash: SourceHash,
    artifact_hash: ImportArtifactDigest,
    pixel_len: usize,
}

impl PreparedImageResource {
    #[must_use]
    pub fn from_image(image: &ImageAsset) -> Self {
        Self {
            extent: image.extent(),
            format: image.format(),
            color_space: image.color_space(),
            source_hash: image.source().source_hash(),
            artifact_hash: image.source().artifact().key().digest(),
            pixel_len: image.pixels().len(),
        }
    }

    #[must_use]
    pub const fn extent(&self) -> ImageExtent {
        self.extent
    }

    #[must_use]
    pub const fn format(&self) -> ImageFormat {
        self.format
    }

    #[must_use]
    pub const fn color_space(&self) -> ImageColorSpace {
        self.color_space
    }

    #[must_use]
    pub const fn source_hash(&self) -> SourceHash {
        self.source_hash
    }

    #[must_use]
    pub const fn artifact_hash(&self) -> ImportArtifactDigest {
        self.artifact_hash
    }

    #[must_use]
    pub const fn pixel_len(&self) -> usize {
        self.pixel_len
    }
}

#[derive(Debug, Default, Resource)]
pub struct ImagePrepareStats {
    pub prepared: u32,
    pub removed: u32,
    pub skipped_missing_state: u32,
    pub skipped_not_loaded: u32,
    pub stale_results: u32,
}

#[derive(Debug, Default)]
pub struct ImagePreparePlugin;

pub const IMAGE_PREPARE_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.image.prepare");
pub const IMAGE_PREPARE_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(IMAGE_PREPARE_PLUGIN_ID, nara_app::PluginCategory::Render);

impl Plugin for ImagePreparePlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &IMAGE_PREPARE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Assets<ImageAsset>>()?;
        app.init_resource::<AssetStates>()?;
        app.init_resource::<PreparedRenderResources<PreparedImageResource>>()?;
        app.init_resource::<RenderPrepareInvalidations>()?;
        app.init_resource::<ImagePrepareStats>()?;
        app.add_systems(CoreStage::Prepare, prepare_images)?;
        Ok(())
    }
}

pub fn prepare_images(
    images: Res<Assets<ImageAsset>>,
    states: Res<AssetStates>,
    mut prepared_images: ResMut<PreparedRenderResources<PreparedImageResource>>,
    mut invalidations: ResMut<RenderPrepareInvalidations>,
    mut stats: ResMut<ImagePrepareStats>,
) {
    *stats = ImagePrepareStats::default();
    let removed_keys = prepared_images
        .keys()
        .filter(|key| key.kind() == RenderResourceKind::IMAGE_2D)
        .filter(|key| {
            let handle = Handle::<ImageAsset>::new(key.asset_id());
            images.get(handle).is_none()
                || states
                    .state(key.asset_id())
                    .is_none_or(|state| state.load_state() == &LoadState::Removed)
        })
        .collect::<Vec<_>>();
    for key in removed_keys {
        if prepared_images
            .remove(
                key,
                &mut invalidations,
                RenderPrepareInvalidationReason::AssetRemoved,
            )
            .is_some()
        {
            stats.removed += 1;
        }
    }

    for (handle, image) in images.iter() {
        let Some(state) = states.state(handle.id()) else {
            stats.skipped_missing_state += 1;
            continue;
        };
        if state.load_state() != &LoadState::Loaded {
            stats.skipped_not_loaded += 1;
            continue;
        }

        let key = image_resource_key(handle);
        let snapshot =
            RenderResourceSnapshot::new(key, state.version(), image_descriptor_hash(image));

        prepared_images.invalidate_if_snapshot_changed(
            snapshot,
            &mut invalidations,
            RenderPrepareInvalidationReason::DescriptorChanged,
        );

        if !prepared_images.needs_prepare(snapshot) {
            continue;
        }

        match prepared_images.insert_ready(snapshot, PreparedImageResource::from_image(image)) {
            RenderPrepareApplyResult::Applied => stats.prepared += 1,
            RenderPrepareApplyResult::DiscardedStale { .. } => stats.stale_results += 1,
        }
    }
}

#[must_use]
pub fn image_resource_key(handle: nara_asset::Handle<ImageAsset>) -> RenderResourceKey {
    RenderResourceKey::for_asset(handle, RenderResourceKind::IMAGE_2D)
}

#[must_use]
pub fn image_descriptor_hash(image: &ImageAsset) -> ImportArtifactDigest {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&image.source().artifact().key().digest().as_bytes());
    bytes.extend_from_slice(&image.source().source_hash().as_bytes());
    bytes.extend_from_slice(&image.extent().width.to_le_bytes());
    bytes.extend_from_slice(&image.extent().height.to_le_bytes());
    bytes.push(image_format_tag(image.format()));
    bytes.push(image_color_space_tag(image.color_space()));
    bytes.extend_from_slice(&(image.pixels().len() as u64).to_le_bytes());
    ImportArtifactDigest::from_bytes(bytes)
}

fn image_format_tag(format: ImageFormat) -> u8 {
    match format {
        ImageFormat::Rgba8 => 1,
    }
}

fn image_color_space_tag(color_space: ImageColorSpace) -> u8 {
    match color_space {
        ImageColorSpace::Srgb => 1,
        ImageColorSpace::Linear => 2,
    }
}
