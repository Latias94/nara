use std::{
    any::Any,
    fmt::{self, Debug, Formatter},
    sync::Arc,
};

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::{AssetStates, Assets, Handle, ImportArtifactDigest, LoadState, SourceHash};
use nara_ecs::{Res, ResMut, Resource};
use nara_render::{
    PreparedRenderResources, RenderResourceKey, RenderResourceKind, RenderResourceSnapshot,
};

use crate::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat};

#[derive(Clone)]
pub struct PreparedImageResource {
    extent: ImageExtent,
    format: ImageFormat,
    color_space: ImageColorSpace,
    source_hash: SourceHash,
    artifact_hash: ImportArtifactDigest,
    pixels: Arc<Box<[u8]>>,
    retention: Option<Arc<dyn Any + Send + Sync>>,
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
            pixels: image.shared_pixels(),
            retention: image.shared_retention_owner(),
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
    pub fn pixel_len(&self) -> usize {
        self.pixels.len()
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        self.pixels.as_ref().as_ref()
    }

    /// Reports whether the source import's memory-accounting owner is retained by this snapshot.
    #[must_use]
    pub const fn retains_source_allocation(&self) -> bool {
        self.retention.is_some()
    }
}

impl Debug for PreparedImageResource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedImageResource")
            .field("extent", &self.extent)
            .field("format", &self.format)
            .field("color_space", &self.color_space)
            .field("source_hash", &self.source_hash)
            .field("artifact_hash", &self.artifact_hash)
            .field("pixel_len", &self.pixels.len())
            .field("retains_source_allocation", &self.retention.is_some())
            .finish()
    }
}

impl PartialEq for PreparedImageResource {
    fn eq(&self, other: &Self) -> bool {
        self.extent == other.extent
            && self.format == other.format
            && self.color_space == other.color_space
            && self.source_hash == other.source_hash
            && self.artifact_hash == other.artifact_hash
            && self.pixels == other.pixels
            && self.retention.is_some() == other.retention.is_some()
    }
}

impl Eq for PreparedImageResource {}

/// Exact immutable prepared image value retained by a render packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedImageSnapshot {
    identity: RenderResourceSnapshot,
    resource: PreparedImageResource,
}

impl PreparedImageSnapshot {
    #[must_use]
    pub fn new(identity: RenderResourceSnapshot, resource: PreparedImageResource) -> Self {
        Self { identity, resource }
    }

    #[must_use]
    pub const fn identity(&self) -> &RenderResourceSnapshot {
        &self.identity
    }

    #[must_use]
    pub const fn resource(&self) -> &PreparedImageResource {
        &self.resource
    }
}

#[derive(Debug, Default, Resource)]
pub struct ImagePrepareStats {
    pub prepared: u32,
    pub removed: u32,
    pub skipped_missing_state: u32,
    pub skipped_not_loaded: u32,
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
        app.init_resource::<PreparedRenderResources<PreparedImageResource>>()?;
        app.init_resource::<ImagePrepareStats>()?;
        app.add_systems(CoreStage::Prepare, prepare_images)?;
        Ok(())
    }
}

pub fn prepare_images(
    images: Res<Assets<ImageAsset>>,
    states: Res<AssetStates>,
    mut prepared_images: ResMut<PreparedRenderResources<PreparedImageResource>>,
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
        if prepared_images.remove(key).is_some() {
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
        let snapshot = RenderResourceSnapshot::new(
            key,
            state.version(),
            images.slot_revision(handle),
            image_descriptor_hash(image),
        );

        if !prepared_images.needs_prepare(&snapshot) {
            continue;
        }

        prepared_images.insert_ready(snapshot, PreparedImageResource::from_image(image));
        stats.prepared += 1;
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
