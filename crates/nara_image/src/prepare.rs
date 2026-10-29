use std::{
    collections::BTreeMap,
    fmt::{self, Debug, Formatter},
    sync::Arc,
};

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::{AssetSlotRevision, AssetStates, AssetVersion, Assets, Handle};
use nara_ecs::{Res, ResMut, Resource};
use nara_render::{
    RenderResourceContentHash, RenderResourceKey, RenderResourceKind, RenderResourceSnapshot,
};

use crate::{
    ImageAsset, ImageColorSpace, ImageContentHash, ImageExtent, ImageFormat,
    asset::ImagePixelRetention,
};

#[derive(Clone)]
pub struct PreparedImageResource {
    extent: ImageExtent,
    format: ImageFormat,
    color_space: ImageColorSpace,
    content_hash: ImageContentHash,
    pixels: Arc<Box<[u8]>>,
    retention: Option<ImagePixelRetention>,
}

impl PreparedImageResource {
    #[must_use]
    pub fn from_image(image: &ImageAsset) -> Self {
        Self {
            extent: image.extent(),
            format: image.format(),
            color_space: image.color_space(),
            content_hash: image.content_hash(),
            pixels: image.shared_pixels(),
            retention: image.shared_pixel_retention(),
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
    pub const fn content_hash(&self) -> ImageContentHash {
        self.content_hash
    }

    #[must_use]
    pub fn pixel_len(&self) -> usize {
        self.pixels.len()
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        self.pixels.as_ref().as_ref()
    }

    /// Reports whether this snapshot retains the image allocation's lifetime owner or lease.
    #[must_use]
    pub const fn has_retention_owner(&self) -> bool {
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
            .field("content_hash", &self.content_hash)
            .field("pixel_len", &self.pixels.len())
            .field("has_retention_owner", &self.retention.is_some())
            .finish()
    }
}

impl PartialEq for PreparedImageResource {
    fn eq(&self, other: &Self) -> bool {
        self.extent == other.extent
            && self.format == other.format
            && self.color_space == other.color_space
            && self.content_hash == other.content_hash
            && self.pixels == other.pixels
            && self.retention.is_some() == other.retention.is_some()
    }
}

impl Eq for PreparedImageResource {}

/// Exact immutable prepared image value retained by a render packet.
///
/// Identity and pixels cannot be assembled independently:
///
/// ```compile_fail
/// use nara_image::{PreparedImageResource, PreparedImageSnapshot};
/// use nara_render::RenderResourceSnapshot;
/// fn pair(identity: RenderResourceSnapshot, pixels: PreparedImageResource) -> PreparedImageSnapshot {
///     PreparedImageSnapshot::new(identity, pixels)
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedImageSnapshot {
    identity: RenderResourceSnapshot,
    resource: PreparedImageResource,
}

impl PreparedImageSnapshot {
    /// Captures a code-first image directly from its typed store and usable load state.
    ///
    /// A managed catalog is not required. The returned value retains the exact stored pixels;
    /// later slot replacement cannot change an already captured render packet.
    #[must_use]
    pub fn capture(
        handle: Handle<ImageAsset>,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Option<Self> {
        let version = states.state(handle.id())?.usable_version()?;
        let image = images.get(handle)?;
        Some(Self::from_image(
            handle,
            version,
            images.slot_revision(handle),
            image,
        ))
    }

    pub(super) fn from_image(
        handle: Handle<ImageAsset>,
        version: AssetVersion,
        slot: AssetSlotRevision,
        image: &ImageAsset,
    ) -> Self {
        Self {
            identity: RenderResourceSnapshot::new(
                image_resource_key(handle),
                version,
                slot,
                image_content_hash(image),
            ),
            resource: PreparedImageResource::from_image(image),
        }
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

/// Image-owned prepared values. Each entry retains its identity and pixels together.
///
/// The collection is a cache of captures, not authority over current asset publication.
#[derive(Debug, Default, Resource)]
pub struct PreparedImages {
    images: BTreeMap<RenderResourceKey, PreparedImageSnapshot>,
}

impl PreparedImages {
    pub fn insert(&mut self, snapshot: PreparedImageSnapshot) -> Option<PreparedImageSnapshot> {
        self.images.insert(snapshot.identity().key(), snapshot)
    }

    #[must_use]
    pub fn get(&self, key: RenderResourceKey) -> Option<&PreparedImageSnapshot> {
        self.images.get(&key)
    }

    pub fn keys(&self) -> impl Iterator<Item = RenderResourceKey> + '_ {
        self.images.keys().copied()
    }

    pub fn remove(&mut self, key: RenderResourceKey) -> Option<PreparedImageSnapshot> {
        self.images.remove(&key)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.images.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

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

#[must_use]
pub fn image_resource_key(handle: nara_asset::Handle<ImageAsset>) -> RenderResourceKey {
    RenderResourceKey::for_asset(handle, RenderResourceKind::IMAGE_2D)
}

#[must_use]
pub fn image_content_hash(image: &ImageAsset) -> RenderResourceContentHash {
    RenderResourceContentHash::from_digest(image.content_hash().as_bytes())
}
