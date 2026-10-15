use std::sync::Arc;

use nara_asset::{AssetId, Handle};
use nara_core::Mat3;
use nara_ecs::Entity;
use nara_render::{
    __private::{
        EditorRenderGeneration, build_editor_render_frame_packet, editor_viewport_image_target,
    },
    Color, Extent2d, ExtractedView, ExtractedViews, RenderFramePacket, RenderImage2d, RenderTarget,
    ViewLoadOp, ViewMapping2d, ViewportRect,
};
use nara_scene::SceneAuthoringRevision;
use nara_tooling::{EditorDocumentId, EditorGestureStamp};
use nara_ui::__private::{
    UiAuthoringCanvasCompileError, UiAuthoringCanvasCompileRequest, UiAuthoringCanvasCompiler,
    UiAuthoringCanvasProjection, UiAuthoringCanvasReady, UiAuthoringCanvasUnavailableReason,
    UiAuthoringDiagnostic, UiAuthoringProjection, UiImageChoiceGeneration,
};
use nara_ui_render::{UiPaintPacket, UiPresentationPlanner};
use thiserror::Error;

use crate::product::AcceptedPrimaryUiSurfaceProfile;

use super::authoring_images::EditorAuthoringImageCatalogReceipt;

const EDITOR_UI_TARGET_ID: u64 = 3;

/// One exact authoring-only UI image candidate owned by the Editor Host.
///
/// The candidate carries no Runtime UI session, reducer, input route, or backend permit. It is
/// eligible for Editor GPU submission only while its document, revision, gesture, and target
/// facts remain current.
#[doc(hidden)]
#[derive(Debug)]
pub struct EditorUiPreviewCandidate {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    gesture: Option<EditorGestureStamp>,
    geometry_generation: u64,
    render_packet: RenderFramePacket,
    paint_packet: Arc<UiPaintPacket>,
    canvas: UiAuthoringCanvasReady,
}

impl EditorUiPreviewCandidate {
    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    #[must_use]
    pub const fn geometry_generation(&self) -> u64 {
        self.geometry_generation
    }

    #[must_use]
    pub const fn canvas(&self) -> &UiAuthoringCanvasReady {
        &self.canvas
    }

    #[doc(hidden)]
    pub fn into_parts(
        self,
    ) -> (
        EditorDocumentId,
        SceneAuthoringRevision,
        Option<EditorGestureStamp>,
        u64,
        RenderFramePacket,
        Arc<UiPaintPacket>,
        UiAuthoringCanvasReady,
    ) {
        (
            self.document,
            self.revision,
            self.gesture,
            self.geometry_generation,
            self.render_packet,
            self.paint_packet,
            self.canvas,
        )
    }
}

/// Private producer for one mutually-exclusive Editor Authoring UI Canvas.
pub(super) struct EditorUiPreviewSession {
    compiler: UiAuthoringCanvasCompiler,
    planner: UiPresentationPlanner,
    render_generation: EditorRenderGeneration,
    target: Handle<RenderImage2d>,
    next_frame_index: u64,
    next_compile_generation: u64,
    key: Option<EditorUiPreviewKey>,
    projection: UiAuthoringCanvasProjection,
    geometry_generation: Option<u64>,
    rebuild_requested: bool,
    candidate: Option<EditorUiPreviewCandidate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EditorUiPreviewKey {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    gesture: Option<EditorGestureStamp>,
    image_generation: UiImageChoiceGeneration,
    profile: Option<AcceptedPrimaryUiSurfaceProfile>,
}

impl std::fmt::Debug for EditorUiPreviewSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EditorUiPreviewSession")
            .field("candidate_ready", &self.candidate.is_some())
            .field("next_frame_index", &self.next_frame_index)
            .field("next_compile_generation", &self.next_compile_generation)
            .field("geometry_generation", &self.geometry_generation)
            .finish_non_exhaustive()
    }
}

impl EditorUiPreviewSession {
    pub(super) fn new() -> Result<Self, UiPreviewError> {
        let compiler = UiAuthoringCanvasCompiler::new()?;
        let render_generation = EditorRenderGeneration::try_acquire()
            .ok_or(UiPreviewError::RenderGenerationExhausted)?;
        Ok(Self {
            compiler,
            planner: UiPresentationPlanner::default(),
            render_generation,
            target: Handle::new(AssetId::from_raw(EDITOR_UI_TARGET_ID)),
            next_frame_index: 0,
            next_compile_generation: 0,
            key: None,
            projection: UiAuthoringCanvasProjection::Unavailable(
                UiAuthoringCanvasUnavailableReason::SourceNotActive,
            ),
            geometry_generation: None,
            rebuild_requested: false,
            candidate: None,
        })
    }

    pub(super) fn prepare(
        &mut self,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        gesture: Option<EditorGestureStamp>,
        projection: &UiAuthoringProjection,
        images: &EditorAuthoringImageCatalogReceipt,
        profile: Option<AcceptedPrimaryUiSurfaceProfile>,
    ) -> UiAuthoringCanvasProjection {
        let key = EditorUiPreviewKey {
            document,
            revision,
            gesture,
            image_generation: images.generation(),
            profile,
        };
        if self.key == Some(key) && !self.rebuild_requested {
            return self.projection.clone();
        }
        self.key = Some(key);
        self.rebuild_requested = false;
        let Some(profile) = profile else {
            self.candidate = None;
            self.geometry_generation = None;
            self.projection = UiAuthoringCanvasProjection::Unavailable(
                UiAuthoringCanvasUnavailableReason::PrimarySurfaceProfileMissing,
            );
            return self.projection.clone();
        };

        match self.try_prepare(document, revision, gesture, projection, images, profile) {
            Ok(candidate) => {
                let canvas = candidate.canvas().clone();
                self.geometry_generation = Some(candidate.geometry_generation());
                self.candidate = Some(candidate);
                self.projection = UiAuthoringCanvasProjection::Ready(canvas);
            }
            Err(_error) => {
                self.candidate = None;
                self.geometry_generation = None;
                self.projection = UiAuthoringCanvasProjection::Rejected(vec![
                    UiAuthoringDiagnostic::canvas_compilation_rejected(),
                ]);
            }
        }
        self.projection.clone()
    }

    fn try_prepare(
        &mut self,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        gesture: Option<EditorGestureStamp>,
        projection: &UiAuthoringProjection,
        images: &EditorAuthoringImageCatalogReceipt,
        profile: AcceptedPrimaryUiSurfaceProfile,
    ) -> Result<EditorUiPreviewCandidate, UiPreviewError> {
        let transform = profile
            .direct_transform()
            .ok_or(UiPreviewError::InvalidProfile)?;
        let logical_size = transform.authored_logical_size();
        let compile_generation = self
            .next_compile_generation
            .checked_add(1)
            .ok_or(UiPreviewError::CompileGenerationExhausted)?;
        self.next_compile_generation = compile_generation;
        let request = UiAuthoringCanvasCompileRequest::new(transform, compile_generation);
        let canvas_candidate = self
            .compiler
            .compile(projection, compile_generation, request)
            .map_err(UiPreviewError::Compile)?;

        let (logical_width, logical_height) = profile.logical_extent();
        let extent =
            Extent2d::new(logical_width, logical_height).ok_or(UiPreviewError::InvalidProfile)?;
        let target = RenderTarget::Image(self.target);
        let paint_packet = self.planner.plan(
            Arc::clone(canvas_candidate.frame()),
            target,
            images.prepared_images(),
        )?;
        let viewport = ViewportRect::from_extent(extent).ok_or(UiPreviewError::InvalidProfile)?;
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: Entity::PLACEHOLDER,
            target,
            viewport,
            mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, logical_size.height(), viewport)
                .ok_or(UiPreviewError::InvalidProfile)?,
            order: 0,
            load: ViewLoadOp::Clear(Color::rgba(0.075, 0.085, 0.11, 1.0)),
        });
        let frame_index = self
            .next_frame_index
            .checked_add(1)
            .ok_or(UiPreviewError::FrameIndexExhausted)?;
        let render_packet = build_editor_render_frame_packet(
            self.render_generation,
            frame_index,
            &views,
            editor_viewport_image_target(self.target, extent)?,
        )?
        .ok_or(UiPreviewError::RenderViewMissing)?;
        self.next_frame_index = frame_index;
        Ok(EditorUiPreviewCandidate {
            document,
            revision,
            gesture,
            geometry_generation: compile_generation,
            render_packet,
            paint_packet,
            canvas: canvas_candidate.canvas().clone(),
        })
    }

    pub(super) fn take_candidate(&mut self) -> Option<EditorUiPreviewCandidate> {
        let candidate = self.candidate.take();
        if matches!(&self.projection, UiAuthoringCanvasProjection::Ready(_)) {
            self.rebuild_requested = true;
        }
        candidate
    }

    pub(super) const fn geometry_generation(&self) -> Option<u64> {
        self.geometry_generation
    }

    pub(super) fn retire(&mut self) {
        let _ = self.planner.retire_target(RenderTarget::Image(self.target));
        self.key = None;
        self.projection = UiAuthoringCanvasProjection::Unavailable(
            UiAuthoringCanvasUnavailableReason::SourceNotActive,
        );
        self.geometry_generation = None;
        self.rebuild_requested = false;
        self.candidate = None;
    }
}

#[derive(Debug, Error)]
pub(super) enum UiPreviewError {
    #[error(transparent)]
    Compile(#[from] UiAuthoringCanvasCompileError),
    #[error(transparent)]
    Presentation(#[from] nara_ui_render::UiPresentationContentError),
    #[error(transparent)]
    Packet(#[from] nara_render::RenderFramePacketError),
    #[error("the Editor UI render generation is exhausted")]
    RenderGenerationExhausted,
    #[error("the Editor UI frame index is exhausted")]
    FrameIndexExhausted,
    #[error("the Editor UI compile generation is exhausted")]
    CompileGenerationExhausted,
    #[error("the project UI profile is invalid for an authoring Canvas")]
    InvalidProfile,
    #[error("the Editor UI render view was not published")]
    RenderViewMissing,
}
