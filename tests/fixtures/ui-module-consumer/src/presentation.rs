//! Code-first retained Image consumption without installing Runtime, a device or a journal.

use std::{io, sync::Arc};

use nara_asset::{AssetEvents, AssetStates, Assets, Handle, allocate_anonymous_handle};
use nara_image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, PreparedImageSnapshot};
use nara_ui_kernel::{
    UiColor, UiImageId, UiLayer, UiLocalKey, UiNode, UiRoot, UiRootKey, UiStyle, UiVisual,
};
use nara_ui_render::UiPreparedImages;

use crate::StatusLabelError;

/// Top-left red, top-right green, bottom-left blue and bottom-right yellow, in sRGB RGBA8.
pub const SWATCH_RGBA: [u8; 16] = [
    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
];

/// A frontend image name, its original typed handle and its indivisible prepared value.
#[derive(Debug, Clone)]
pub struct CapturedUiImage {
    id: UiImageId,
    handle: Handle<ImageAsset>,
    snapshot: PreparedImageSnapshot,
}

impl CapturedUiImage {
    /// Publishes anonymous procedural pixels through the ordinary typed store before capture.
    pub fn rgba8<R>(
        id: UiImageId,
        extent: ImageExtent,
        color_space: ImageColorSpace,
        pixels: Vec<u8>,
        owner: Arc<R>,
    ) -> Result<Self, StatusLabelError>
    where
        R: Send + Sync + 'static,
    {
        let mut image = ImageAsset::new(extent, ImageFormat::Rgba8, color_space, pixels)?;
        if !image.try_attach_retention_owner(owner) {
            return Err(io::Error::other("procedural image already has a retention owner").into());
        }
        let handle = allocate_anonymous_handle::<ImageAsset>()?;
        let mut images = Assets::default();
        let mut states = AssetStates::default();
        images.commit_loaded(
            handle,
            image,
            &mut states,
            &mut AssetEvents::default(),
            None,
            None,
        )?;
        Self::capture(id, handle, &images, &states)
    }

    /// Captures the caller's exact slot, loader version, content hash, pixels and lifetime owner.
    pub fn capture(
        id: UiImageId,
        handle: Handle<ImageAsset>,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<Self, StatusLabelError> {
        let snapshot = PreparedImageSnapshot::capture(handle, images, states)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "image has no usable pixels"))?;
        Ok(Self {
            id,
            handle,
            snapshot,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &UiImageId {
        &self.id
    }

    #[must_use]
    pub const fn handle(&self) -> Handle<ImageAsset> {
        self.handle
    }

    #[must_use]
    pub const fn snapshot(&self) -> &PreparedImageSnapshot {
        &self.snapshot
    }

    /// Inserts the complete capture; conflicting bindings retain the planner's ambiguity rules.
    pub fn insert_into(&self, images: &mut UiPreparedImages) {
        images.insert(self.id.clone(), self.snapshot.clone());
    }

    #[must_use]
    pub fn prepared_images(&self) -> UiPreparedImages {
        let mut images = UiPreparedImages::new();
        self.insert_into(&mut images);
        images
    }

    #[must_use]
    pub fn authored_node(&self, key: UiLocalKey, style: UiStyle) -> UiNode {
        UiNode::new(style).with_key(key).with_visual(self.visual())
    }

    #[must_use]
    pub fn authored_root(&self, key: UiRootKey, layer: UiLayer, style: UiStyle) -> UiRoot {
        UiRoot::new(key, layer, style).with_visual(self.visual())
    }

    fn visual(&self) -> UiVisual {
        UiVisual::image(self.id.clone(), UiColor::rgba(1.0, 1.0, 1.0, 1.0))
    }
}

/// The same finite four-color input used by the CPU consumer and the real native UI executable.
pub fn capture_swatch<R>(owner: Arc<R>) -> Result<CapturedUiImage, StatusLabelError>
where
    R: Send + Sync + 'static,
{
    CapturedUiImage::rgba8(
        UiImageId::new("independent-ui/procedural-swatch")?,
        ImageExtent::new(2, 2),
        ImageColorSpace::Srgb,
        SWATCH_RGBA.to_vec(),
        owner,
    )
}
