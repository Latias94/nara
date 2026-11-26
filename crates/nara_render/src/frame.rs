use nara_app::RuntimeGeneration;
use nara_window::{Window, WindowId};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

static NEXT_RENDER_SOURCE: AtomicU64 = AtomicU64::new(1);

use crate::{
    CapturedRenderTime, Extent2d, ExtractedView, ExtractedViews, RenderFramePacketError,
    RenderFrameTarget, RenderImagePacket, RenderTarget, RenderWindowPacket, ViewLoadOp,
    WorldCanvas, WorldCanvasFrame,
};

/// Maximum number of world views in one frame for a target.
pub const MAX_VIEWS_PER_TARGET: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum RenderFrameSourceKind {
    Runtime(RuntimeGeneration),
    Independent(u64),
}

/// Exact producer generation for one immutable render-frame packet stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderFrameSourceGeneration(RenderFrameSourceKind);

impl RenderFrameSourceGeneration {
    /// Allocates one non-reused process-local stream without constructing a gameplay Runtime.
    pub fn try_acquire() -> Option<Self> {
        NEXT_RENDER_SOURCE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .ok()
            .map(|value| Self(RenderFrameSourceKind::Independent(value)))
    }
    pub(crate) const fn runtime(generation: RuntimeGeneration) -> Self {
        Self(RenderFrameSourceKind::Runtime(generation))
    }

    #[must_use]
    pub const fn runtime_generation(self) -> Option<RuntimeGeneration> {
        match self.0 {
            RenderFrameSourceKind::Runtime(generation) => Some(generation),
            RenderFrameSourceKind::Independent(_) => None,
        }
    }
}

/// Builds an offscreen frame from an explicit image target and extracted views.
pub fn build_image_render_frame_packet(
    generation: RenderFrameSourceGeneration,
    frame_index: u64,
    views: &ExtractedViews,
    target: RenderImagePacket,
) -> Result<Option<RenderFramePacket>, RenderFramePacketError> {
    build_render_frame_packet_for_source(
        generation,
        frame_index,
        None,
        std::iter::empty(),
        views,
        Some(target),
        None,
    )
}

impl From<RuntimeGeneration> for RenderFrameSourceGeneration {
    fn from(generation: RuntimeGeneration) -> Self {
        Self::runtime(generation)
    }
}

/// Immutable views, target geometry and time for one render frame.
/// A fixed world canvas supplies the mapping from its raster to a window.
/// A window overlay can have no world views.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderFramePacket {
    pub(crate) source_generation: RenderFrameSourceGeneration,
    frame_index: u64,
    pub(crate) target: RenderFrameTarget,
    logical_target: RenderTarget,
    adapted_from_window: Option<WindowId>,
    views: Arc<[ExtractedView]>,
    world_canvas: Option<WorldCanvasFrame>,
    captured_time: Option<CapturedRenderTime>,
}

impl RenderFramePacket {
    #[must_use]
    pub const fn source_generation(&self) -> RenderFrameSourceGeneration {
        self.source_generation
    }
    #[must_use]
    pub const fn frame_index(&self) -> u64 {
        self.frame_index
    }
    #[must_use]
    pub const fn target(&self) -> RenderFrameTarget {
        self.target
    }
    /// The captured destination before any physical image-target adaptation.
    #[must_use]
    pub const fn logical_target(&self) -> RenderTarget {
        self.logical_target
    }

    /// The resolved source window, including after adapting its frame to an image.
    #[must_use]
    pub const fn source_window(&self) -> Option<WindowId> {
        match self.target {
            RenderFrameTarget::Window(window) => Some(window.id),
            RenderFrameTarget::Image(_) => self.adapted_from_window,
        }
    }
    #[must_use]
    pub fn views(&self) -> &[ExtractedView] {
        &self.views
    }

    pub(crate) fn shared_views(&self) -> Arc<[ExtractedView]> {
        Arc::clone(&self.views)
    }
    #[must_use]
    pub const fn world_canvas(&self) -> Option<WorldCanvasFrame> {
        self.world_canvas
    }

    /// The frozen original Extract sample, unavailable for manually supplied views.
    ///
    /// Packet construction and target adaptation never sample current World clocks. The packet's
    /// frame index and source generation retain their existing render producer ownership.
    #[must_use]
    pub const fn captured_time(&self) -> Option<CapturedRenderTime> {
        self.captured_time
    }

    /// Clones this window frame for an image execution target of the same physical extent.
    /// The copy retains the logical target, views, canvas and captured time.
    ///
    /// Returns an error if this packet does not target a window or the image extent differs from
    /// the captured physical window extent, even when a fixed world canvas has a smaller extent.
    pub fn with_image_target(
        &self,
        target: RenderImagePacket,
    ) -> Result<RenderFramePacket, RenderFramePacketError> {
        let RenderFrameTarget::Window(window) = self.target else {
            return Err(RenderFramePacketError::ImageAdaptationRequiresWindowTarget);
        };
        let window_extent = Extent2d::new(
            window.resolution.physical_width,
            window.resolution.physical_height,
        )
        .ok_or(RenderFramePacketError::ImageTargetHasZeroExtent)?;
        if target.extent() != window_extent {
            return Err(RenderFramePacketError::ImageAdaptationExtentMismatch {
                window: window_extent,
                image: target.extent(),
            });
        }
        Ok(Self {
            target: RenderFrameTarget::Image(target),
            adapted_from_window: Some(window.id),
            ..self.clone()
        })
    }
}

/// Captures a window-space overlay after its composer has admitted the original window.
#[must_use]
pub fn build_window_overlay_frame_packet(
    generation: RuntimeGeneration,
    frame_index: u64,
    window: RenderWindowPacket,
    captured_time: Option<CapturedRenderTime>,
) -> Option<RenderFramePacket> {
    if window.resolution.is_zero_sized() {
        return None;
    }
    Some(RenderFramePacket {
        source_generation: RenderFrameSourceGeneration::runtime(generation),
        frame_index,
        target: RenderFrameTarget::Window(window),
        logical_target: RenderTarget::Window(window.id),
        adapted_from_window: None,
        views: Arc::default(),
        world_canvas: None,
        captured_time,
    })
}

/// Builds a frame for one window from its ordered views.
pub fn build_render_frame_packet<'a>(
    generation: RuntimeGeneration,
    frame_index: u64,
    primary_window_id: Option<WindowId>,
    windows: impl IntoIterator<Item = &'a Window>,
    views: &ExtractedViews,
) -> Result<Option<RenderFramePacket>, RenderFramePacketError> {
    build_render_frame_packet_with_world_canvas(
        generation,
        frame_index,
        primary_window_id,
        windows,
        views,
        None,
    )
}

/// Builds a window frame with an optional fixed world raster.
/// World viewports use the raster extent and the overlay uses the native window extent.
pub fn build_render_frame_packet_with_world_canvas<'a>(
    generation: RuntimeGeneration,
    frame_index: u64,
    primary_window_id: Option<WindowId>,
    windows: impl IntoIterator<Item = &'a Window>,
    views: &ExtractedViews,
    world_canvas: Option<WorldCanvas>,
) -> Result<Option<RenderFramePacket>, RenderFramePacketError> {
    build_render_frame_packet_for_source(
        RenderFrameSourceGeneration::runtime(generation),
        frame_index,
        primary_window_id,
        windows,
        views,
        None,
        world_canvas,
    )
}

pub(crate) fn build_render_frame_packet_for_source<'a>(
    source_generation: RenderFrameSourceGeneration,
    frame_index: u64,
    primary_window_id: Option<WindowId>,
    windows: impl IntoIterator<Item = &'a Window>,
    views: &ExtractedViews,
    image_target: Option<RenderImagePacket>,
    world_canvas: Option<WorldCanvas>,
) -> Result<Option<RenderFramePacket>, RenderFramePacketError> {
    let captured_time = views.captured_time();
    let views = views.as_slice();
    let Some(first) = views.first() else {
        return Ok(None);
    };
    if views.len() > MAX_VIEWS_PER_TARGET {
        return Err(RenderFramePacketError::TooManyViews {
            actual: views.len(),
            maximum: MAX_VIEWS_PER_TARGET,
        });
    }
    for (index, view) in views.iter().enumerate() {
        if views[..index].iter().any(|previous| previous.id == view.id) {
            return Err(RenderFramePacketError::DuplicateViewId { id: view.id });
        }
    }
    if views
        .iter()
        .any(|view| !targets_match(first.target, view.target, primary_window_id))
    {
        return Err(RenderFramePacketError::MixedRenderTargets);
    }
    if views.windows(2).any(|pair| {
        (pair[0].order, pair[0].camera_entity.to_bits())
            > (pair[1].order, pair[1].camera_entity.to_bits())
    }) {
        return Err(RenderFramePacketError::UnorderedViews);
    }
    if !matches!(first.load, ViewLoadOp::Clear(_)) {
        return Err(RenderFramePacketError::UninitializedTarget);
    }
    let target = match first.target {
        RenderTarget::Image(image) => {
            let descriptor =
                image_target.ok_or(RenderFramePacketError::MissingImageTargetDescriptor)?;
            if descriptor.image() != image {
                return Err(RenderFramePacketError::ImageTargetMismatch);
            }
            if descriptor.extent().is_empty() {
                return Err(RenderFramePacketError::ImageTargetHasZeroExtent);
            }
            RenderFrameTarget::Image(descriptor)
        }
        RenderTarget::PrimaryWindow | RenderTarget::Window(_) => {
            let mut windows = windows.into_iter();
            let Some(window) = windows.next() else {
                return Err(RenderFramePacketError::UnsupportedWindowCount { actual: 0 });
            };
            let additional = windows.count();
            if additional != 0 {
                return Err(RenderFramePacketError::UnsupportedWindowCount {
                    actual: additional.saturating_add(1),
                });
            }
            let window_id = first
                .target
                .window_id(primary_window_id)
                .expect("window target");
            if window.id != window_id {
                return Err(RenderFramePacketError::TargetWindowMismatch { window_id });
            }
            // Minimized/suspended native windows have no raster target. Explicit camera
            // viewports remain valid authored data, but cannot make this surface renderable.
            if window.resolution.is_zero_sized() {
                return Ok(None);
            }
            RenderFrameTarget::Window(RenderWindowPacket {
                id: window.id,
                resolution: window.resolution,
                present_mode: window.present_mode,
            })
        }
    };
    let world_canvas = match (world_canvas, target) {
        (Some(canvas), RenderFrameTarget::Window(window)) => Some(
            WorldCanvasFrame::new(canvas, window.resolution)
                .ok_or(RenderFramePacketError::InvalidWorldCanvasMapping)?,
        ),
        (Some(_), RenderFrameTarget::Image(_)) => {
            return Err(RenderFramePacketError::WorldCanvasRequiresWindowTarget);
        }
        (None, _) => None,
    };
    let extent = world_canvas.map_or_else(|| target.extent(), |canvas| canvas.extent());
    if views.iter().any(|view| {
        let viewport = view.viewport;
        viewport.extent().is_empty()
            || u64::from(viewport.physical_x) + u64::from(viewport.physical_width)
                > u64::from(extent.width)
            || u64::from(viewport.physical_y) + u64::from(viewport.physical_height)
                > u64::from(extent.height)
    }) {
        return Err(RenderFramePacketError::ViewportOutsideTarget);
    }
    Ok(Some(RenderFramePacket {
        source_generation,
        frame_index,
        target,
        logical_target: first.target,
        adapted_from_window: None,
        views: Arc::from(views),
        world_canvas,
        captured_time,
    }))
}

pub(crate) fn targets_match(
    left: RenderTarget,
    right: RenderTarget,
    primary: Option<WindowId>,
) -> bool {
    match (left, right) {
        (RenderTarget::Image(left), RenderTarget::Image(right)) => left == right,
        (RenderTarget::Image(_), _) | (_, RenderTarget::Image(_)) => false,
        _ => left.window_id(primary) == right.window_id(primary),
    }
}
