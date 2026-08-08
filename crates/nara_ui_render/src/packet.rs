use std::sync::Arc;

use nara_image::PreparedImageSnapshot;
use nara_render::RenderTarget;
use nara_ui_kernel::{UiColor, UiFrame, UiFrameRevision, UiImageId, UiRevision, UiSurfaceId};

/// Exact identity of one immutable UI presentation attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiPaintPacketIdentity {
    surface_id: UiSurfaceId,
    frame_revision: UiFrameRevision,
    source_revision: UiRevision,
    route_revision: UiRevision,
    presentation_revision: UiRevision,
    theme_revision: UiRevision,
    font_revision: UiRevision,
}

impl UiPaintPacketIdentity {
    pub(crate) fn from_frame(frame: &UiFrame) -> Self {
        let environment = frame.environment();
        Self {
            surface_id: environment.surface_id(),
            frame_revision: frame.revision(),
            source_revision: frame.source_revision(),
            route_revision: environment.route_revision(),
            presentation_revision: environment.presentation_revision(),
            theme_revision: environment.theme_revision(),
            font_revision: environment.font_revision(),
        }
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn frame_revision(self) -> UiFrameRevision {
        self.frame_revision
    }

    #[must_use]
    pub const fn source_revision(self) -> UiRevision {
        self.source_revision
    }

    #[must_use]
    pub const fn route_revision(self) -> UiRevision {
        self.route_revision
    }

    #[must_use]
    pub const fn presentation_revision(self) -> UiRevision {
        self.presentation_revision
    }

    #[must_use]
    pub const fn theme_revision(self) -> UiRevision {
        self.theme_revision
    }

    #[must_use]
    pub const fn font_revision(self) -> UiRevision {
        self.font_revision
    }
}

/// Signed physical rectangle retained before backend viewport clamping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiPacketRect {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl UiPacketRect {
    #[must_use]
    pub const fn new(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> Option<Self> {
        if max_x <= min_x || max_y <= min_y {
            return None;
        }
        Some(Self {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    #[must_use]
    pub const fn min_x(self) -> i32 {
        self.min_x
    }

    #[must_use]
    pub const fn min_y(self) -> i32 {
        self.min_y
    }

    #[must_use]
    pub const fn max_x(self) -> i32 {
        self.max_x
    }

    #[must_use]
    pub const fn max_y(self) -> i32 {
        self.max_y
    }

    #[must_use]
    pub const fn width(self) -> u32 {
        self.max_x.abs_diff(self.min_x)
    }

    #[must_use]
    pub const fn height(self) -> u32 {
        self.max_y.abs_diff(self.min_y)
    }

    #[must_use]
    pub fn intersect(self, other: Self) -> Option<Self> {
        Self::new(
            self.min_x.max(other.min_x),
            self.min_y.max(other.min_y),
            self.max_x.min(other.max_x),
            self.max_y.min(other.max_y),
        )
    }
}

/// Normalized texture coordinates retained by one image or glyph primitive.
///
/// Packet UVs use a top-left origin: `u` grows right and `v` grows down. `top_left` names that
/// origin-relative corner and `extent` is non-negative in packet space. A backend whose shader
/// convention differs converts this coordinate system exactly once at its packet boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPacketUv {
    top_left: [f32; 2],
    extent: [f32; 2],
}

impl UiPacketUv {
    pub const FULL: Self = Self {
        top_left: [0.0, 0.0],
        extent: [1.0, 1.0],
    };

    #[must_use]
    pub(crate) const fn from_top_left(top_left: [f32; 2], extent: [f32; 2]) -> Self {
        Self { top_left, extent }
    }

    #[must_use]
    pub const fn top_left(self) -> [f32; 2] {
        self.top_left
    }

    #[must_use]
    pub const fn extent(self) -> [f32; 2] {
        self.extent
    }
}

/// Stable identity of one immutable CPU glyph-atlas page generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiGlyphAtlasPageKey {
    font_revision: u64,
    page_index: u16,
    generation: u64,
}

impl UiGlyphAtlasPageKey {
    pub(crate) const fn new(font_revision: u64, page_index: u16, generation: u64) -> Self {
        Self {
            font_revision,
            page_index,
            generation,
        }
    }

    #[must_use]
    pub const fn font_revision(self) -> u64 {
        self.font_revision
    }

    #[must_use]
    pub const fn page_index(self) -> u16 {
        self.page_index
    }

    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// Immutable R8 glyph-atlas page retained by a paint packet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiGlyphAtlasPageSnapshot {
    key: UiGlyphAtlasPageKey,
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
}

impl UiGlyphAtlasPageSnapshot {
    pub(crate) fn new(
        key: UiGlyphAtlasPageKey,
        width: u32,
        height: u32,
        pixels: Arc<Vec<u8>>,
    ) -> Self {
        Self {
            key,
            width,
            height,
            pixels,
        }
    }

    #[must_use]
    pub const fn key(&self) -> UiGlyphAtlasPageKey {
        self.key
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[must_use]
    pub fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pixels, &other.pixels)
    }
}

/// One exact prepared image snapshot retained by the packet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiPacketImage {
    id: UiImageId,
    snapshot: PreparedImageSnapshot,
}

impl UiPacketImage {
    pub(crate) fn new(id: UiImageId, snapshot: PreparedImageSnapshot) -> Self {
        Self { id, snapshot }
    }

    #[must_use]
    pub const fn id(&self) -> &UiImageId {
        &self.id
    }

    #[must_use]
    pub const fn snapshot(&self) -> &PreparedImageSnapshot {
        &self.snapshot
    }
}

/// One ordered, resource-indexed primitive in a backend-neutral packet.
#[derive(Clone, Debug, PartialEq)]
pub enum UiPacketPrimitive {
    Solid {
        rect: UiPacketRect,
        clip: Option<UiPacketRect>,
        color: UiColor,
        corner_radii: [f32; 4],
    },
    Image {
        rect: UiPacketRect,
        clip: Option<UiPacketRect>,
        image_index: u32,
        tint: UiColor,
        uv: UiPacketUv,
        corner_radii: [f32; 4],
    },
    Glyph {
        rect: UiPacketRect,
        clip: Option<UiPacketRect>,
        page_index: u16,
        color: UiColor,
        uv: UiPacketUv,
    },
}

impl UiPacketPrimitive {
    #[must_use]
    pub const fn rect(&self) -> UiPacketRect {
        match self {
            Self::Solid { rect, .. } | Self::Image { rect, .. } | Self::Glyph { rect, .. } => *rect,
        }
    }

    #[must_use]
    pub const fn clip(&self) -> Option<UiPacketRect> {
        match self {
            Self::Solid { clip, .. } | Self::Image { clip, .. } | Self::Glyph { clip, .. } => *clip,
        }
    }
}

/// One bounded packet-construction diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiPaintDiagnostic {
    code: &'static str,
    message: String,
}

impl UiPaintDiagnostic {
    pub(crate) fn new(code: &'static str, message: String) -> Self {
        Self { code, message }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Resource-complete immutable paint packet consumed by concrete presentation backends.
#[derive(Clone, Debug)]
pub struct UiPaintPacket {
    identity: UiPaintPacketIdentity,
    target: RenderTarget,
    scale_factor: f32,
    frame: Arc<UiFrame>,
    primitives: Arc<[UiPacketPrimitive]>,
    images: Arc<[UiPacketImage]>,
    glyph_pages: Arc<[UiGlyphAtlasPageSnapshot]>,
    diagnostics: Arc<[UiPaintDiagnostic]>,
}

impl UiPaintPacket {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        identity: UiPaintPacketIdentity,
        target: RenderTarget,
        scale_factor: f32,
        frame: Arc<UiFrame>,
        primitives: Vec<UiPacketPrimitive>,
        images: Vec<UiPacketImage>,
        glyph_pages: Vec<UiGlyphAtlasPageSnapshot>,
        diagnostics: Vec<UiPaintDiagnostic>,
    ) -> Self {
        Self {
            identity,
            target,
            scale_factor,
            frame,
            primitives: primitives.into(),
            images: images.into(),
            glyph_pages: glyph_pages.into(),
            diagnostics: diagnostics.into(),
        }
    }

    #[must_use]
    pub const fn identity(&self) -> UiPaintPacketIdentity {
        self.identity
    }

    #[must_use]
    pub const fn target(&self) -> RenderTarget {
        self.target
    }

    #[must_use]
    pub const fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    #[must_use]
    pub fn frame(&self) -> Arc<UiFrame> {
        Arc::clone(&self.frame)
    }

    #[must_use]
    pub fn primitives(&self) -> &[UiPacketPrimitive] {
        &self.primitives
    }

    #[must_use]
    pub fn images(&self) -> &[UiPacketImage] {
        &self.images
    }

    #[must_use]
    pub fn glyph_pages(&self) -> &[UiGlyphAtlasPageSnapshot] {
        &self.glyph_pages
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[UiPaintDiagnostic] {
        &self.diagnostics
    }
}

/// Closed presentation result classification shared by packet builders and concrete backends.
#[derive(Clone, Debug)]
pub enum UiPresentationOutcome {
    Presented {
        packet: Arc<UiPaintPacket>,
        presentation_epoch: u64,
    },
    ContentRejected {
        attempt: UiPaintPacketIdentity,
        message: String,
    },
    Deferred {
        attempt: UiPaintPacketIdentity,
        message: String,
    },
    BackendFault {
        attempt: Option<UiPaintPacketIdentity>,
        message: String,
    },
}

impl UiPresentationOutcome {
    #[must_use]
    pub fn attempt(&self) -> Option<UiPaintPacketIdentity> {
        match self {
            Self::Presented { packet, .. } => Some(packet.identity()),
            Self::ContentRejected { attempt, .. } | Self::Deferred { attempt, .. } => {
                Some(*attempt)
            }
            Self::BackendFault { attempt, .. } => *attempt,
        }
    }

    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Presented { .. } => None,
            Self::ContentRejected { message, .. }
            | Self::Deferred { message, .. }
            | Self::BackendFault { message, .. } => Some(message),
        }
    }
}
