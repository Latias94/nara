use std::sync::Arc;

use nara_asset::Handle;
use nara_core::Mat3;
use nara_ecs::Entity;
use nara_render::{
    __private::{
        EditorRenderGeneration, build_editor_render_frame_packet, editor_viewport_image_target,
    },
    Color, Extent2d, ExtractedView, ExtractedViews, RenderFramePacket, RenderImage2d, RenderTarget,
    ViewLoadOp, ViewMapping2d, ViewportRect,
};
use nara_scene::{SceneAuthoringRevision, SceneOccurrenceKey, SourceDocumentKey};
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
use super::authoring_runtime::EditorCanvasImageSources;

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
    occurrence: SceneOccurrenceKey,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct EditorUiPreviewKey {
    source: SourceDocumentKey,
    occurrence: SceneOccurrenceKey,
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
            occurrence: SceneOccurrenceKey::new("nara.editor", "ui-canvas")
                .expect("the Editor Canvas declares a nonempty logical occurrence"),
            compiler,
            planner: UiPresentationPlanner::default(),
            render_generation,
            target: RenderImage2d::allocate_handle()?,
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
        source: &SourceDocumentKey,
        revision: SceneAuthoringRevision,
        gesture: Option<EditorGestureStamp>,
        projection: &UiAuthoringProjection,
        images: &EditorAuthoringImageCatalogReceipt,
        assets: EditorCanvasImageSources<'_>,
        profile: Option<AcceptedPrimaryUiSurfaceProfile>,
    ) -> UiAuthoringCanvasProjection {
        let image_generation = images.generation();
        let unchanged_source = self.key.as_ref().is_some_and(|key| {
            &key.source == source
                && key.occurrence == self.occurrence
                && key.document == document
                && key.revision == revision
                && key.gesture == gesture
                && key.image_generation == image_generation
                && key.profile == profile
        });
        if unchanged_source && !self.rebuild_requested {
            return self.projection.clone();
        }
        let key = EditorUiPreviewKey {
            source: source.clone(),
            occurrence: self.occurrence.clone(),
            document,
            revision,
            gesture,
            image_generation,
            profile,
        };
        self.key = Some(key.clone());
        self.rebuild_requested = false;
        let Some(profile) = profile else {
            self.candidate = None;
            self.geometry_generation = None;
            self.projection = UiAuthoringCanvasProjection::Unavailable(
                UiAuthoringCanvasUnavailableReason::PrimarySurfaceProfileMissing,
            );
            return self.projection.clone();
        };

        let candidate = if unchanged_source {
            self.try_rebuild(key, profile)
        } else {
            self.try_prepare(key, projection, images, assets, profile)
        };
        match candidate {
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
        key: EditorUiPreviewKey,
        projection: &UiAuthoringProjection,
        images: &EditorAuthoringImageCatalogReceipt,
        assets: EditorCanvasImageSources<'_>,
        profile: AcceptedPrimaryUiSurfaceProfile,
    ) -> Result<EditorUiPreviewCandidate, UiPreviewError> {
        let transform = profile
            .direct_transform()
            .ok_or(UiPreviewError::InvalidProfile)?;
        let compile_generation = self
            .next_compile_generation
            .checked_add(1)
            .ok_or(UiPreviewError::CompileGenerationExhausted)?;
        self.next_compile_generation = compile_generation;
        let request = UiAuthoringCanvasCompileRequest::new(transform, compile_generation);
        let canvas_candidate = assets
            .with_decode_context(|context| {
                self.compiler.compile(
                    projection,
                    &key.source,
                    &key.occurrence,
                    compile_generation,
                    request,
                    context,
                )
            })
            .map_err(|_| UiPreviewError::ImageContextChanged)?
            .map_err(UiPreviewError::Compile)?;

        let target = RenderTarget::Image(self.target);
        let paint_packet = self.planner.plan(
            Arc::clone(canvas_candidate.frame()),
            target,
            images.prepared_images(),
        )?;
        self.try_render_candidate(
            key,
            compile_generation,
            canvas_candidate.canvas().clone(),
            paint_packet,
            profile,
        )
    }

    fn try_rebuild(
        &mut self,
        key: EditorUiPreviewKey,
        profile: AcceptedPrimaryUiSurfaceProfile,
    ) -> Result<EditorUiPreviewCandidate, UiPreviewError> {
        let (
            Some(geometry_generation),
            UiAuthoringCanvasProjection::Ready(canvas),
            Some(paint_packet),
        ) = (
            self.geometry_generation,
            &self.projection,
            self.planner.planned(RenderTarget::Image(self.target)),
        )
        else {
            return Err(UiPreviewError::PreparedCanvasMissing);
        };
        self.try_render_candidate(
            key,
            geometry_generation,
            canvas.clone(),
            paint_packet,
            profile,
        )
    }

    fn try_render_candidate(
        &mut self,
        key: EditorUiPreviewKey,
        geometry_generation: u64,
        canvas: UiAuthoringCanvasReady,
        paint_packet: Arc<UiPaintPacket>,
        profile: AcceptedPrimaryUiSurfaceProfile,
    ) -> Result<EditorUiPreviewCandidate, UiPreviewError> {
        let logical_size = profile
            .direct_transform()
            .ok_or(UiPreviewError::InvalidProfile)?
            .authored_logical_size();
        let (logical_width, logical_height) = profile.logical_extent();
        let extent =
            Extent2d::new(logical_width, logical_height).ok_or(UiPreviewError::InvalidProfile)?;
        let target = RenderTarget::Image(self.target);
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
            document: key.document,
            revision: key.revision,
            gesture: key.gesture,
            geometry_generation,
            render_packet,
            paint_packet,
            canvas,
        })
    }

    pub(super) fn take_candidate(&mut self) -> Option<EditorUiPreviewCandidate> {
        let candidate = self.candidate.take();
        // An empty take requests another render transaction without editing the source geometry.
        if candidate.is_none() && matches!(&self.projection, UiAuthoringCanvasProjection::Ready(_))
        {
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
    #[error("the Editor Canvas requested an Image identity not yet published by its owner")]
    ImageContextChanged,
    #[error(transparent)]
    TargetIdentity(#[from] nara_asset::AssetError),
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
    #[error("the Editor UI Canvas has no retained preparation to rebuild")]
    PreparedCanvasMissing,
}
