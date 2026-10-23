use std::{collections::HashMap, sync::Arc};

use nara_render::{RenderResourceKey, RenderTarget};
use nara_text::{
    FontFace, FontFaceId, TextEngine, TextEngineCreateError, TextLimits, bundled_font_face,
};
use nara_ui_kernel::{UiFrame, UiSurfaceId};
use thiserror::Error;

use crate::packet::UiPreparedBackendSubmission;
use crate::{
    UiBackendSubmissionIdentity, UiPaintPacket, UiPaintPacketIdentity, UiPresentationAttemptId,
    UiPresentationAttemptIdentity, UiPresentationLimits, UiPresentationLimitsError,
    atlas::{GlyphAtlas, UiAtlasPlanError},
};

mod images;
mod lowering;

pub use images::UiPreparedImages;
pub(crate) use images::collect_referenced_image_ids;
use lowering::{lower_packet, preflight_content};

#[derive(Clone)]
struct TargetPresentationState {
    surface_id: UiSurfaceId,
    atlas: GlyphAtlas,
    planned: Arc<UiPaintPacket>,
    last_presented: Option<Arc<UiPaintPacket>>,
    active_submission: Option<UiBackendSubmissionIdentity>,
}

/// Target-private, failure-atomic planner for resource-complete UI packets.
pub struct UiPresentationPlanner {
    limits: UiPresentationLimits,
    text: TextEngine,
    targets: HashMap<RenderTarget, TargetPresentationState>,
}

impl UiPresentationPlanner {
    pub fn new(limits: UiPresentationLimits) -> Result<Self, UiPresentationPlannerCreateError> {
        let face = bundled_font_face()
            .map_err(|error| UiPresentationPlannerCreateError::BundledFont(error.to_string()))?;
        Self::with_font(face, limits)
    }

    /// Shares an admitted immutable face with the caller's shaping engine.
    pub fn with_font(
        face: FontFace,
        limits: UiPresentationLimits,
    ) -> Result<Self, UiPresentationPlannerCreateError> {
        let limits = limits.validate()?;
        let text = TextEngine::new(face, TextLimits::default())?;
        Ok(Self {
            limits,
            text,
            targets: HashMap::new(),
        })
    }

    #[must_use]
    pub const fn font_face_id(&self) -> FontFaceId {
        self.text.font_face_id()
    }

    pub fn plan(
        &mut self,
        frame: Arc<UiFrame>,
        target: RenderTarget,
        images: &UiPreparedImages,
    ) -> Result<Arc<UiPaintPacket>, UiPresentationContentError> {
        let identity = UiPaintPacketIdentity::from_frame(&frame);
        let surface_id = identity.surface_id();
        if let Some(existing_surface) = self
            .targets
            .get(&target)
            .map(|state| state.surface_id)
            .filter(|existing| *existing != surface_id)
        {
            return Err(UiPresentationContentError::TargetAlreadyOwned {
                target,
                existing_surface,
                requested_surface: surface_id,
            });
        }
        if let Some(active_submission) = self
            .targets
            .get(&target)
            .and_then(|state| state.active_submission)
        {
            return Err(UiPresentationContentError::SubmissionStillActive {
                target,
                admission_id: active_submission.admission_id(),
            });
        }
        let preflight = preflight_content(&frame, images, self.limits)?;
        if let Some(key) = preflight
            .requested_glyphs()
            .iter()
            .find(|key| key.face_id() != self.text.font_face_id())
        {
            return Err(UiPresentationContentError::FontFaceMismatch {
                expected: self.text.font_face_id(),
                actual: key.face_id(),
            });
        }
        let current_atlas = self
            .targets
            .get(&target)
            .map_or_else(GlyphAtlas::default, |state| state.atlas.clone());
        let staged_atlas = current_atlas
            .stage(
                frame.environment().font_revision().get(),
                preflight.requested_glyphs(),
                &mut self.text,
                self.limits,
            )
            .map_err(UiPresentationContentError::from_atlas)?;
        let packet = lower_packet(
            Arc::clone(&frame),
            target,
            images,
            &staged_atlas,
            self.limits,
        )?;

        if !self.targets.contains_key(&target) {
            self.targets
                .try_reserve(1)
                .map_err(|_| UiPresentationContentError::AllocationFailed)?;
        }

        let last_presented = self
            .targets
            .get(&target)
            .and_then(|state| state.last_presented.as_ref().map(Arc::clone));
        self.targets.insert(
            target,
            TargetPresentationState {
                surface_id,
                atlas: staged_atlas,
                planned: Arc::clone(&packet),
                last_presented,
                active_submission: None,
            },
        );
        Ok(packet)
    }

    #[must_use]
    pub fn planned(&self, target: RenderTarget) -> Option<Arc<UiPaintPacket>> {
        self.targets
            .get(&target)
            .map(|state| Arc::clone(&state.planned))
    }

    #[must_use]
    pub fn last_presented(&self, target: RenderTarget) -> Option<Arc<UiPaintPacket>> {
        self.targets
            .get(&target)
            .and_then(|state| state.last_presented.as_ref().map(Arc::clone))
    }

    pub fn retire_target(&mut self, target: RenderTarget) -> Option<Arc<UiPaintPacket>> {
        if self
            .targets
            .get(&target)
            .is_some_and(|state| state.active_submission.is_some())
        {
            return None;
        }
        self.targets.remove(&target).map(|state| state.planned)
    }

    pub(crate) fn retire_surface_target(
        &mut self,
        surface_id: UiSurfaceId,
        target: RenderTarget,
    ) -> Option<Arc<UiPaintPacket>> {
        let owned_by_surface = self
            .targets
            .get(&target)
            .is_some_and(|state| state.surface_id == surface_id);
        owned_by_surface
            .then(|| self.retire_target(target))
            .flatten()
    }

    pub(crate) fn replace_backend_submissions(
        &mut self,
        submissions: &[UiPreparedBackendSubmission],
        displaced: &[UiPreparedBackendSubmission],
    ) -> Result<(), UiPresentationAttemptId> {
        for submission in submissions {
            let identity = submission.identity();
            let Some(state) = self.targets.get(&identity.target()) else {
                return Err(identity.admission_id());
            };
            let active_is_replaceable = state.active_submission.is_none_or(|active| {
                active == identity || displaced.iter().any(|queued| queued.identity() == active)
            });
            if state.surface_id != identity.packet().surface_id()
                || state.planned.identity() != identity.packet()
                || !Arc::ptr_eq(&state.planned, submission.packet())
                || !active_is_replaceable
            {
                return Err(identity.admission_id());
            }
        }
        for submission in submissions {
            let identity = submission.identity();
            self.targets
                .get_mut(&identity.target())
                .expect("all planner submissions were validated before staging")
                .active_submission = Some(identity);
        }
        Ok(())
    }

    /// Commits one backend-accepted packet after `replace_backend_submissions` has reserved the
    /// exact planner state. The active-submission lock makes a missing or mismatched target an
    /// internal invariant violation rather than a recoverable post-submit outcome.
    pub(crate) fn commit_presented(
        &mut self,
        identity: UiBackendSubmissionIdentity,
        packet: Arc<UiPaintPacket>,
    ) {
        let state = self
            .targets
            .get_mut(&identity.target())
            .expect("a backend acceptance must retain its staged planner target");
        assert_eq!(
            state.active_submission,
            Some(identity),
            "a backend acceptance must retain its active planner submission"
        );
        assert_eq!(
            state.surface_id,
            identity.packet().surface_id(),
            "a backend acceptance must retain its staged surface"
        );
        assert_eq!(
            state.planned.identity(),
            identity.packet(),
            "a backend acceptance must retain its staged packet identity"
        );
        assert!(
            Arc::ptr_eq(&state.planned, &packet),
            "a backend acceptance must retain its staged packet allocation"
        );
        state.last_presented = Some(packet);
        state.active_submission = None;
    }

    pub(crate) fn abort_planned(
        &mut self,
        attempt: UiPresentationAttemptIdentity,
    ) -> UiPlannedPacketAbortOutcome {
        let Some(target) = attempt.target() else {
            return UiPlannedPacketAbortOutcome::IgnoredUnrouted;
        };
        let last_presented = match self.targets.get(&target) {
            Some(state)
                if attempt.backend_submission().is_some_and(|identity| {
                    state.active_submission == Some(identity)
                        && state.planned.identity() == identity.packet()
                }) || (attempt.backend_submission().is_none()
                    && state.active_submission.is_none()
                    && state.planned.identity() == attempt.packet()) =>
            {
                state.last_presented.as_ref().map(Arc::clone)
            }
            _ => return UiPlannedPacketAbortOutcome::IgnoredStale,
        };
        if let Some(last_presented) = last_presented {
            let state = self
                .targets
                .get_mut(&target)
                .expect("the exact planner target was just validated");
            state.planned = last_presented;
            state.active_submission = None;
            UiPlannedPacketAbortOutcome::RestoredLastPresented
        } else {
            self.targets.remove(&target);
            UiPlannedPacketAbortOutcome::RemovedUnpresentedTarget
        }
    }

    pub(crate) fn abort_surface_target(
        &mut self,
        surface_id: UiSurfaceId,
        target: RenderTarget,
    ) -> UiPlannedPacketAbortOutcome {
        let last_presented = match self.targets.get(&target) {
            Some(state) if state.surface_id == surface_id && state.active_submission.is_none() => {
                state.last_presented.as_ref().map(Arc::clone)
            }
            _ => return UiPlannedPacketAbortOutcome::IgnoredStale,
        };
        if let Some(last_presented) = last_presented {
            let state = self
                .targets
                .get_mut(&target)
                .expect("the exact planner target was just validated");
            state.planned = last_presented;
            UiPlannedPacketAbortOutcome::RestoredLastPresented
        } else {
            self.targets.remove(&target);
            UiPlannedPacketAbortOutcome::RemovedUnpresentedTarget
        }
    }

    pub(crate) fn snapshot_planned_packets(&self) -> Vec<Arc<UiPaintPacket>> {
        let mut packets = self
            .targets
            .values()
            .map(|state| Arc::clone(&state.planned))
            .collect::<Vec<_>>();
        sort_packets(&mut packets);
        packets
    }

    pub(crate) fn snapshot_presented_packets(&self) -> Vec<Arc<UiPaintPacket>> {
        let mut packets = self
            .targets
            .values()
            .filter_map(|state| state.last_presented.as_ref().map(Arc::clone))
            .collect::<Vec<_>>();
        sort_packets(&mut packets);
        packets
    }
}

/// Planner result after a prepared or submitted packet is terminated without presentation.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiPlannedPacketAbortOutcome {
    RestoredLastPresented,
    RemovedUnpresentedTarget,
    IgnoredStale,
    IgnoredUnrouted,
}

fn sort_packets(packets: &mut [Arc<UiPaintPacket>]) {
    packets.sort_by_key(|packet| {
        (
            packet.identity().surface_id(),
            render_target_order_key(packet.target()),
        )
    });
}

fn render_target_order_key(target: RenderTarget) -> (u8, u64) {
    match target {
        RenderTarget::PrimaryWindow => (0, 0),
        RenderTarget::Window(window) => (1, window.get()),
        RenderTarget::Image(image) => (2, image.id().raw()),
    }
}

impl Default for UiPresentationPlanner {
    fn default() -> Self {
        Self::new(UiPresentationLimits::default())
            .expect("the default UI presentation limits and bundled font are valid")
    }
}

#[derive(Debug, Error)]
pub enum UiPresentationPlannerCreateError {
    #[error(transparent)]
    InvalidLimits(#[from] UiPresentationLimitsError),
    #[error("bundled UI font could not be admitted: {0}")]
    BundledFont(String),
    #[error(transparent)]
    TextEngine(#[from] TextEngineCreateError),
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiPresentationContentError {
    #[error(
        "UI presentation target {target:?} is already owned by surface {existing_surface:?}, so surface {requested_surface:?} cannot mutate it"
    )]
    TargetAlreadyOwned {
        target: RenderTarget,
        existing_surface: UiSurfaceId,
        requested_surface: UiSurfaceId,
    },
    #[error(
        "UI presentation target {target:?} still owns active backend admission {admission_id:?}"
    )]
    SubmissionStillActive {
        target: RenderTarget,
        admission_id: UiPresentationAttemptId,
    },
    #[error(
        "UI presentation target {target:?} is assigned to both surface {first_surface:?} and surface {second_surface:?}"
    )]
    TargetRouteConflict {
        target: RenderTarget,
        first_surface: UiSurfaceId,
        second_surface: UiSurfaceId,
    },
    #[error("UI surface {surface_id:?} appears more than once in one presentation batch")]
    DuplicateSurfaceFrame { surface_id: UiSurfaceId },
    #[error("UI image `{id}` has no exact prepared image snapshot")]
    MissingImage { id: String },
    #[error("UI image `{id}` resolves ambiguously to {first:?} and {second:?}")]
    AmbiguousImage {
        id: String,
        first: RenderResourceKey,
        second: RenderResourceKey,
    },
    #[error("the UI target plan exceeds its {limit} referenced-image limit")]
    ReferencedImageLimitExceeded { limit: usize },
    #[error("the UI target plan exceeds its {limit} referenced-glyph limit")]
    ReferencedGlyphLimitExceeded { limit: usize },
    #[error("the UI target plan exceeds its {limit} expanded-primitive limit")]
    PrimitiveLimitExceeded { limit: usize },
    #[error("the glyph cache exceeds its {limit} entry limit")]
    GlyphEntryLimitExceeded { limit: usize },
    #[error("the UI target plan exceeds its {limit} new-glyph limit")]
    NewGlyphLimitExceeded { limit: usize },
    #[error("glyph rasterization failed: {message}")]
    GlyphRasterizationFailed { message: String },
    #[error("UI frame glyph face {actual:?} does not match planner face {expected:?}")]
    FontFaceMismatch {
        expected: FontFaceId,
        actual: FontFaceId,
    },
    #[error("glyph raster {width}x{height} exceeds the {limit} pixel extent limit")]
    GlyphExtentLimitExceeded { width: u32, height: u32, limit: u32 },
    #[error("the glyph atlas exceeds its {limit} page limit")]
    AtlasPageLimitExceeded { limit: usize },
    #[error("the glyph atlas exceeds its {limit} byte retention limit")]
    AtlasByteLimitExceeded { limit: usize },
    #[error("the staged glyph atlas exceeds its {limit} byte mutation limit")]
    StagedAtlasByteLimitExceeded { limit: usize },
    #[error("UI corner radii must be uniform in the first-party presentation baseline")]
    NonUniformCornerRadius,
    #[error("UI corner radius {actual} exceeds the {limit} physical-pixel limit")]
    CornerRadiusLimitExceeded { actual: u32, limit: u32 },
    #[error("the UI packet exceeds its {limit} retained-byte limit")]
    PacketRetentionLimitExceeded { limit: usize },
    #[error("the UI paint packet exceeds the supported image-index range")]
    ImageIndexRangeExceeded,
    #[error("the staged UI presentation allocation failed within admitted hard limits")]
    AllocationFailed,
    #[error("UI presentation arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("the glyph atlas generation identity was exhausted")]
    GenerationExhausted,
    #[error("glyph atlas page {page_index} is missing")]
    MissingAtlasPage { page_index: u16 },
}

impl UiPresentationContentError {
    fn from_atlas(error: UiAtlasPlanError) -> Self {
        match error {
            UiAtlasPlanError::GlyphEntryLimitExceeded { limit } => {
                Self::GlyphEntryLimitExceeded { limit }
            }
            UiAtlasPlanError::NewGlyphLimitExceeded { limit } => {
                Self::NewGlyphLimitExceeded { limit }
            }
            UiAtlasPlanError::RasterizationFailed(message) => {
                Self::GlyphRasterizationFailed { message }
            }
            UiAtlasPlanError::GlyphExtentLimitExceeded {
                width,
                height,
                limit,
            } => Self::GlyphExtentLimitExceeded {
                width,
                height,
                limit,
            },
            UiAtlasPlanError::AtlasPageLimitExceeded { limit } => {
                Self::AtlasPageLimitExceeded { limit }
            }
            UiAtlasPlanError::AtlasByteLimitExceeded { limit } => {
                Self::AtlasByteLimitExceeded { limit }
            }
            UiAtlasPlanError::StagedAtlasByteLimitExceeded { limit } => {
                Self::StagedAtlasByteLimitExceeded { limit }
            }
            UiAtlasPlanError::AllocationFailed => Self::AllocationFailed,
            UiAtlasPlanError::ArithmeticOverflow => Self::ArithmeticOverflow,
            UiAtlasPlanError::GenerationExhausted => Self::GenerationExhausted,
            UiAtlasPlanError::MissingPage { page_index } => Self::MissingAtlasPage { page_index },
        }
    }
}
