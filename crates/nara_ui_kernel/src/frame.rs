use std::sync::Arc;

use nara_text::TextLayout;
use thiserror::Error;

use crate::{
    UiActionId, UiColor, UiCornerRadii, UiImageId, UiKey, UiLayer, UiPhysicalRect, UiPoint, UiRect,
    UiRevision, UiRootKey, UiSemanticRole, UiSize, UiSnapPolicy, UiSurfaceContentTransform,
    UiSurfaceId,
};

const MAX_LOGICAL_EXTENT: f32 = 16_384.0;
const MAX_SCALE_FACTOR: f32 = 8.0;

/// Complete immutable logical and presentation environment used by one compilation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiEnvironment {
    surface_id: UiSurfaceId,
    content_transform: UiSurfaceContentTransform,
    generations: UiSurfaceGenerations,
    theme_revision: UiRevision,
    font_revision: UiRevision,
    snap_policy: UiSnapPolicy,
}

/// Exact Surface Slot generations carried by every candidate, receipt and input route.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiSurfaceGenerations {
    slot: UiRevision,
    route: UiRevision,
    geometry: UiRevision,
    presentation: UiRevision,
}

impl UiSurfaceGenerations {
    pub fn new(
        slot: u64,
        route: u64,
        geometry: u64,
        presentation: u64,
    ) -> Result<Self, UiEnvironmentError> {
        Ok(Self {
            slot: revision("slot_generation", slot)?,
            route: revision("route_revision", route)?,
            geometry: revision("geometry_generation", geometry)?,
            presentation: revision("presentation_revision", presentation)?,
        })
    }

    #[must_use]
    pub const fn slot_generation(self) -> UiRevision {
        self.slot
    }

    #[must_use]
    pub const fn route_revision(self) -> UiRevision {
        self.route
    }

    #[must_use]
    pub const fn geometry_generation(self) -> UiRevision {
        self.geometry
    }

    #[must_use]
    pub const fn presentation_revision(self) -> UiRevision {
        self.presentation
    }
}

impl UiEnvironment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        surface_id: UiSurfaceId,
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
        route_revision: u64,
        presentation_revision: u64,
        theme_revision: u64,
        font_revision: u64,
    ) -> Result<Self, UiEnvironmentError> {
        if !logical_width.is_finite()
            || !logical_height.is_finite()
            || logical_width <= 0.0
            || logical_height <= 0.0
            || logical_width > MAX_LOGICAL_EXTENT
            || logical_height > MAX_LOGICAL_EXTENT
        {
            return Err(UiEnvironmentError::InvalidLogicalSize {
                width: logical_width,
                height: logical_height,
            });
        }
        if !scale_factor.is_finite() || scale_factor <= 0.0 || scale_factor > MAX_SCALE_FACTOR {
            return Err(UiEnvironmentError::InvalidScaleFactor {
                value: scale_factor,
            });
        }
        let logical_size =
            UiSize::new(logical_width, logical_height).expect("validated positive logical size");
        let content_transform = UiSurfaceContentTransform::direct(logical_size, scale_factor)
            .map_err(|_| UiEnvironmentError::InvalidScaleFactor {
                value: scale_factor,
            })?;
        // The direct convenience path represents one fixed synthetic slot. Callers that own a
        // real Host lifecycle must use `for_surface` with explicit generations.
        Self::for_surface(
            surface_id,
            content_transform,
            UiSurfaceGenerations::new(
                1,
                route_revision,
                presentation_revision,
                presentation_revision,
            )?,
            theme_revision,
            font_revision,
        )
    }

    /// Builds an environment from one exact Host-owned Surface Slot generation.
    pub fn for_surface(
        surface_id: UiSurfaceId,
        content_transform: UiSurfaceContentTransform,
        generations: UiSurfaceGenerations,
        theme_revision: u64,
        font_revision: u64,
    ) -> Result<Self, UiEnvironmentError> {
        Ok(Self {
            surface_id,
            content_transform,
            generations,
            theme_revision: revision("theme_revision", theme_revision)?,
            font_revision: revision("font_revision", font_revision)?,
            snap_policy: UiSnapPolicy::NearestEdge,
        })
    }

    #[must_use]
    pub const fn with_snap_policy(mut self, policy: UiSnapPolicy) -> Self {
        self.snap_policy = policy;
        self
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn logical_size(self) -> UiSize {
        self.content_transform.authored_logical_size()
    }

    #[must_use]
    pub const fn scale_factor(self) -> f32 {
        self.content_transform.content_scale_factor()
    }

    #[must_use]
    pub const fn content_transform(self) -> UiSurfaceContentTransform {
        self.content_transform
    }

    #[must_use]
    pub const fn generations(self) -> UiSurfaceGenerations {
        self.generations
    }

    #[must_use]
    pub const fn slot_generation(self) -> UiRevision {
        self.generations.slot_generation()
    }

    #[must_use]
    pub const fn route_revision(self) -> UiRevision {
        self.generations.route_revision()
    }

    #[must_use]
    pub const fn geometry_generation(self) -> UiRevision {
        self.generations.geometry_generation()
    }

    #[must_use]
    pub const fn presentation_revision(self) -> UiRevision {
        self.generations.presentation_revision()
    }

    #[must_use]
    pub const fn theme_revision(self) -> UiRevision {
        self.theme_revision
    }

    #[must_use]
    pub const fn font_revision(self) -> UiRevision {
        self.font_revision
    }

    #[must_use]
    pub const fn snap_policy(self) -> UiSnapPolicy {
        self.snap_policy
    }

    pub(crate) fn snap(self, rect: UiRect) -> UiPhysicalRect {
        self.physical_rect(rect).unwrap_or_default()
    }

    #[must_use]
    pub fn physical_rect(self, rect: UiRect) -> Option<UiPhysicalRect> {
        self.content_transform
            .authored_rect_to_physical(rect, self.snap_policy)
    }
}

fn revision(field: &'static str, value: u64) -> Result<UiRevision, UiEnvironmentError> {
    UiRevision::new(value).ok_or(UiEnvironmentError::ZeroRevision { field })
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum UiEnvironmentError {
    #[error("logical UI size {width} x {height} is invalid")]
    InvalidLogicalSize { width: f32, height: f32 },
    #[error("UI scale factor {value} is invalid")]
    InvalidScaleFactor { value: f32 },
    #[error("UI environment revision {field} must be non-zero")]
    ZeroRevision { field: &'static str },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiFrameRevision(u64);

impl UiFrameRevision {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Resolved border carried directly by an immutable paint primitive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiResolvedBorder {
    width: f32,
    color: UiColor,
}

/// Renderer-neutral resolved brush retained by a paint primitive.
#[derive(Clone, Debug, PartialEq)]
pub enum UiResolvedBrush {
    Solid(UiColor),
    Image { image: UiImageId, tint: UiColor },
}

impl UiResolvedBorder {
    pub(crate) const fn new(width: f32, color: UiColor) -> Self {
        Self { width, color }
    }

    #[must_use]
    pub const fn width(self) -> f32 {
        self.width
    }

    #[must_use]
    pub const fn color(self) -> UiColor {
        self.color
    }
}

/// One resolved layout fact shared by all frame projections.
#[derive(Clone, Debug, PartialEq)]
pub struct UiLayoutFact {
    pub(crate) key: Option<UiKey>,
    pub(crate) route_parent: Option<UiKey>,
    pub(crate) root_key: UiRootKey,
    pub(crate) layer: UiLayer,
    pub(crate) structural_index: usize,
    pub(crate) local_z: i32,
    pub(crate) rect: UiRect,
    pub(crate) physical_rect: UiPhysicalRect,
    pub(crate) clip_rect: Option<UiRect>,
    pub(crate) visible: bool,
}

impl UiLayoutFact {
    #[must_use]
    pub const fn key(&self) -> Option<&UiKey> {
        self.key.as_ref()
    }

    /// Returns the nearest stable-key ancestor used for routed-event propagation.
    #[must_use]
    pub const fn route_parent(&self) -> Option<&UiKey> {
        self.route_parent.as_ref()
    }

    #[must_use]
    pub const fn root_key(&self) -> &UiRootKey {
        &self.root_key
    }

    #[must_use]
    pub const fn layer(&self) -> UiLayer {
        self.layer
    }

    #[must_use]
    pub const fn structural_index(&self) -> usize {
        self.structural_index
    }

    #[must_use]
    pub const fn local_z(&self) -> i32 {
        self.local_z
    }

    #[must_use]
    pub const fn rect(&self) -> UiRect {
        self.rect
    }

    #[must_use]
    pub const fn physical_rect(&self) -> UiPhysicalRect {
        self.physical_rect
    }

    #[must_use]
    pub const fn clip_rect(&self) -> Option<UiRect> {
        self.clip_rect
    }

    #[must_use]
    pub const fn visible(&self) -> bool {
        self.visible
    }
}

/// One item in the forward paint stream. Text retains its exact immutable shaped layout.
#[derive(Clone, Debug, PartialEq)]
pub enum UiPaintPrimitive {
    Rect {
        key: Option<UiKey>,
        rect: UiRect,
        physical_rect: UiPhysicalRect,
        clip_rect: Option<UiRect>,
        brush: Option<UiResolvedBrush>,
        border: Option<UiResolvedBorder>,
        radii: UiCornerRadii,
    },
    Text {
        key: Option<UiKey>,
        rect: UiRect,
        physical_rect: UiPhysicalRect,
        clip_rect: Option<UiRect>,
        color: UiColor,
        layout: Arc<TextLayout>,
    },
}

impl UiPaintPrimitive {
    #[must_use]
    pub const fn key(&self) -> Option<&UiKey> {
        match self {
            Self::Rect { key, .. } | Self::Text { key, .. } => key.as_ref(),
        }
    }

    #[must_use]
    pub const fn rect(&self) -> UiRect {
        match self {
            Self::Rect { rect, .. } | Self::Text { rect, .. } => *rect,
        }
    }

    #[must_use]
    pub const fn clip_rect(&self) -> Option<UiRect> {
        match self {
            Self::Rect { clip_rect, .. } | Self::Text { clip_rect, .. } => *clip_rect,
        }
    }
}

/// One entry in the reverse hit index.
#[derive(Clone, Debug, PartialEq)]
pub struct UiHitRegion {
    pub(crate) key: UiKey,
    pub(crate) rect: UiRect,
    pub(crate) clip_rect: Option<UiRect>,
    pub(crate) enabled: bool,
}

impl UiHitRegion {
    #[must_use]
    pub const fn key(&self) -> &UiKey {
        &self.key
    }

    #[must_use]
    pub const fn rect(&self) -> UiRect {
        self.rect
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn contains(&self, point: UiPoint) -> bool {
        self.enabled
            && self.rect.contains(point)
            && self.clip_rect.is_none_or(|clip| clip.contains(point))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiFocusFact {
    pub(crate) key: UiKey,
    pub(crate) rect: UiRect,
    pub(crate) enabled: bool,
}

impl UiFocusFact {
    #[must_use]
    pub const fn key(&self) -> &UiKey {
        &self.key
    }

    #[must_use]
    pub const fn rect(&self) -> UiRect {
        self.rect
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiSemanticNode {
    pub(crate) key: UiKey,
    pub(crate) parent: Option<UiKey>,
    pub(crate) role: UiSemanticRole,
    pub(crate) name: String,
    pub(crate) value: Option<String>,
    pub(crate) enabled: bool,
    pub(crate) selected: Option<bool>,
    pub(crate) expanded: Option<bool>,
    pub(crate) actions: Vec<UiActionId>,
    pub(crate) rect: UiRect,
    pub(crate) clip_rect: Option<UiRect>,
}

impl UiSemanticNode {
    #[must_use]
    pub const fn key(&self) -> &UiKey {
        &self.key
    }

    #[must_use]
    pub const fn parent(&self) -> Option<&UiKey> {
        self.parent.as_ref()
    }

    #[must_use]
    pub const fn role(&self) -> UiSemanticRole {
        self.role
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn selected(&self) -> Option<bool> {
        self.selected
    }

    #[must_use]
    pub const fn expanded(&self) -> Option<bool> {
        self.expanded
    }

    #[must_use]
    pub fn actions(&self) -> &[UiActionId] {
        &self.actions
    }

    #[must_use]
    pub const fn rect(&self) -> UiRect {
        self.rect
    }

    #[must_use]
    pub const fn clip_rect(&self) -> Option<UiRect> {
        self.clip_rect
    }

    pub(crate) fn visible_rect(&self) -> Option<UiRect> {
        self.clip_rect
            .map_or(Some(self.rect), |clip| self.rect.intersect(clip))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRootDiagnostic {
    DuplicateNodeKey { key: UiKey },
    MissingRequiredKey { structural_index: usize },
    DepthLimitExceeded { maximum: usize },
    AuthoringRejected { message: String },
    KeyRejected { message: String },
    InvalidStyle { message: String },
    TextRejected { message: String },
    LayoutRejected { message: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiRootStatus {
    Published,
    RetainedVisual,
    Absent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiRootOutcome {
    pub(crate) key: UiRootKey,
    pub(crate) layer: UiLayer,
    pub(crate) status: UiRootStatus,
    pub(crate) diagnostic: Option<UiRootDiagnostic>,
}

impl UiRootOutcome {
    #[must_use]
    pub const fn key(&self) -> &UiRootKey {
        &self.key
    }

    #[must_use]
    pub const fn layer(&self) -> UiLayer {
        self.layer
    }

    #[must_use]
    pub const fn status(&self) -> UiRootStatus {
        self.status
    }

    #[must_use]
    pub const fn diagnostic(&self) -> Option<&UiRootDiagnostic> {
        self.diagnostic.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiDiagnostic {
    Root {
        key: UiRootKey,
        detail: UiRootDiagnostic,
    },
    Truncated {
        omitted: usize,
    },
}

/// One immutable authority shared by paint, hit, focus, and semantics.
#[derive(Debug, PartialEq)]
pub struct UiFrame {
    pub(crate) revision: UiFrameRevision,
    pub(crate) source_revision: UiRevision,
    pub(crate) environment: UiEnvironment,
    pub(crate) root_outcomes: Vec<UiRootOutcome>,
    pub(crate) layouts: Vec<UiLayoutFact>,
    pub(crate) paint: Vec<UiPaintPrimitive>,
    pub(crate) hits: Vec<UiHitRegion>,
    pub(crate) focus: Vec<UiFocusFact>,
    pub(crate) semantics: Vec<UiSemanticNode>,
    pub(crate) diagnostics: Vec<UiDiagnostic>,
}

impl UiFrame {
    #[must_use]
    pub const fn revision(&self) -> UiFrameRevision {
        self.revision
    }

    #[must_use]
    pub const fn source_revision(&self) -> UiRevision {
        self.source_revision
    }

    #[must_use]
    pub const fn environment(&self) -> UiEnvironment {
        self.environment
    }

    #[must_use]
    pub fn root_outcomes(&self) -> &[UiRootOutcome] {
        &self.root_outcomes
    }

    #[must_use]
    pub fn layouts(&self) -> &[UiLayoutFact] {
        &self.layouts
    }

    #[must_use]
    pub fn paint(&self) -> &[UiPaintPrimitive] {
        &self.paint
    }

    #[must_use]
    pub fn hit_regions(&self) -> &[UiHitRegion] {
        &self.hits
    }

    #[must_use]
    pub fn focus_order(&self) -> &[UiFocusFact] {
        &self.focus
    }

    #[must_use]
    pub fn semantics(&self) -> &[UiSemanticNode] {
        &self.semantics
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[UiDiagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub fn hit_test(&self, point: UiPoint) -> Option<&UiHitRegion> {
        self.hits.iter().find(|region| region.contains(point))
    }

    /// Returns whether an active modal root requires unhandled input to stop at this frame.
    ///
    /// This is a read-only fact of the immutable frame. It does not grant route, presentation, or
    /// interaction authority to the caller.
    #[must_use]
    pub fn blocks_unhandled_input(&self) -> bool {
        self.active_modal_root().is_some()
    }

    pub(crate) fn active_modal_root(&self) -> Option<&UiRootKey> {
        self.root_outcomes
            .iter()
            .rev()
            .find(|outcome| {
                outcome.layer == UiLayer::Modal && outcome.status != UiRootStatus::Absent
            })
            .map(|outcome| &outcome.key)
    }
}

/// Evidence that the exact immutable frame packet was accepted and submitted by a presenter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiPresentationReceipt {
    pub(crate) surface_id: UiSurfaceId,
    pub(crate) frame_revision: UiFrameRevision,
    pub(crate) generations: UiSurfaceGenerations,
    pub(crate) presentation_epoch: u64,
}

impl UiPresentationReceipt {
    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn frame_revision(self) -> UiFrameRevision {
        self.frame_revision
    }

    #[must_use]
    pub const fn generations(self) -> UiSurfaceGenerations {
        self.generations
    }

    #[must_use]
    pub const fn slot_generation(self) -> UiRevision {
        self.generations.slot_generation()
    }

    #[must_use]
    pub const fn route_revision(self) -> UiRevision {
        self.generations.route_revision()
    }

    #[must_use]
    pub const fn geometry_generation(self) -> UiRevision {
        self.generations.geometry_generation()
    }

    #[must_use]
    pub const fn presentation_revision(self) -> UiRevision {
        self.generations.presentation_revision()
    }

    #[must_use]
    pub const fn presentation_epoch(self) -> u64 {
        self.presentation_epoch
    }
}
