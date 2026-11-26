//! Immutable game HUD input and direct rendering into the frame's color attachment.

use std::{collections::HashMap, sync::Arc};

use nara_asset::image::ImageAsset;
use nara_ui::{
    FontFace,
    kernel::{UiFrame, UiImageId},
};

#[cfg(all(feature = "hud-runtime", feature = "wgpu"))]
pub(crate) mod atlas;
#[cfg(feature = "hud-runtime")]
mod capture;
#[cfg(all(feature = "hud-runtime", feature = "wgpu"))]
pub(crate) mod paint;
#[cfg(feature = "hud-runtime")]
mod surface;

#[cfg(all(feature = "hud-runtime", feature = "wgpu"))]
pub use crate::gpu::HudPass;
#[cfg(all(feature = "hud-runtime", feature = "wgpu"))]
pub(crate) use capture::capture;
#[cfg(feature = "hud-runtime")]
pub use surface::sync_surface_metrics;

/// The UI compiler's exact frame and the immutable images referenced by its paint.
#[derive(Clone, Debug)]
pub struct HudFrame {
    frame: Arc<UiFrame>,
    images: HashMap<UiImageId, ImageAsset>,
    fonts: Vec<FontFace>,
}

impl HudFrame {
    #[must_use]
    pub fn new(
        frame: Arc<UiFrame>,
        images: impl IntoIterator<Item = (UiImageId, ImageAsset)>,
    ) -> Self {
        Self {
            frame,
            images: images.into_iter().collect(),
            fonts: Vec::new(),
        }
    }

    /// Supplies the font used by the compiler for frames captured outside the runtime.
    #[must_use]
    pub fn with_font(self, font: FontFace) -> Self {
        self.with_fonts(vec![font])
    }

    /// Supplies the compiler's ordered primary and fallback faces for an external frame.
    #[must_use]
    pub fn with_fonts(mut self, fonts: Vec<FontFace>) -> Self {
        self.fonts = fonts;
        self
    }

    #[must_use]
    pub fn frame(&self) -> &Arc<UiFrame> {
        &self.frame
    }

    #[must_use]
    pub fn image(&self, id: &UiImageId) -> Option<&ImageAsset> {
        self.images.get(id)
    }

    #[must_use]
    pub fn fonts(&self) -> &[FontFace] {
        &self.fonts
    }
}

/// HUD data drawn into the submitted target. A native presenter publishes it after presenting
/// that same target, using the presenter's monotonically increasing presentation epoch.
#[derive(Clone, Debug)]
pub struct HudPresentation {
    pub frame: Arc<UiFrame>,
    pub glyph_count: u64,
}

/// A HUD candidate rejected before drawing. The scene may still present.
#[derive(Debug, thiserror::Error)]
pub enum HudDrawError {
    #[error("HUD layout extent differs from the render attachment")]
    ExtentMismatch,
    #[error("could not draw HUD: {0}")]
    Paint(String),
}
