use std::sync::Arc;

use nara_asset::{
    AssetEvents, AssetId, AssetRecord, AssetServer, AssetSourceKind, AssetStates, Assets, Handle,
};
use nara_core::Vec2;
use nara_ecs::Entity;
use nara_image::{
    ImageAsset, PreparedImageResource, PreparedImageSnapshot, image_descriptor_hash,
    image_resource_key,
};
use nara_render::{
    __private::{EditorRenderGeneration, build_editor_render_frame_packet},
    Color, Extent2d, ExtractedView, ExtractedViews, RenderFramePacket, RenderImage2d,
    RenderResourceSnapshot, RenderTarget, ViewportRect,
};
use nara_scene::SceneAuthoringRevision;
use nara_tooling::{EditorDocumentId, EditorGestureStamp};
use nara_ui::__private::{
    UiAuthoringCanvasCompileError, UiAuthoringCanvasCompileRequest, UiAuthoringCanvasCompiler,
    UiAuthoringCanvasProjection, UiAuthoringCanvasReady, UiAuthoringCanvasUnavailableReason,
    UiAuthoringDiagnostic, UiAuthoringProjection,
};
use nara_ui::UiImageId;
use nara_ui_render::{UiPaintPacket, UiPreparedImages, UiPresentationPlanner};
use thiserror::Error;

use crate::{product::AcceptedPrimaryUiSurfaceProfile, project_host::ProjectContentSnapshot};

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
    prepared_images: UiPreparedImages,
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
    pub(super) fn new(snapshot: &ProjectContentSnapshot) -> Result<Self, UiPreviewError> {
        let prepared_images = prepare_project_images(snapshot)?;
        Self::new_with_prepared_images(prepared_images)
    }

    pub(super) fn new_authoring() -> Result<Self, UiPreviewError> {
        Self::new_with_prepared_images(UiPreparedImages::new())
    }

    fn new_with_prepared_images(prepared_images: UiPreparedImages) -> Result<Self, UiPreviewError> {
        let compiler = UiAuthoringCanvasCompiler::new()?;
        let render_generation = EditorRenderGeneration::try_acquire()
            .ok_or(UiPreviewError::RenderGenerationExhausted)?;
        Ok(Self {
            compiler,
            planner: UiPresentationPlanner::default(),
            prepared_images,
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
        profile: Option<AcceptedPrimaryUiSurfaceProfile>,
    ) -> UiAuthoringCanvasProjection {
        let key = EditorUiPreviewKey {
            document,
            revision,
            gesture,
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

        match self.try_prepare(document, revision, gesture, projection, profile) {
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
            &self.prepared_images,
        )?;
        let viewport = ViewportRect::from_extent(extent).ok_or(UiPreviewError::InvalidProfile)?;
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: Entity::PLACEHOLDER,
            target,
            viewport,
            world_position: Vec2::ZERO,
            viewport_height: logical_size.height(),
            order: 0,
            clear_color: Color::rgba(0.075, 0.085, 0.11, 1.0),
        });
        let frame_index = self
            .next_frame_index
            .checked_add(1)
            .ok_or(UiPreviewError::FrameIndexExhausted)?;
        let render_packet =
            build_editor_render_frame_packet(self.render_generation, frame_index, &views)?
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

fn prepare_project_images(
    snapshot: &ProjectContentSnapshot,
) -> Result<UiPreparedImages, UiPreviewError> {
    let mut imported = Vec::with_capacity(snapshot.images().len());
    for (index, content) in snapshot.images().iter().enumerate() {
        let source = content.image().source();
        let record = AssetRecord::new(
            source.stable_id(),
            source.path().clone(),
            AssetSourceKind::Image,
        );
        let image = snapshot
            .share_image_for_consumer(index)
            .ok_or(UiPreviewError::ImageSnapshotInvalid)?;
        imported.push((record, image));
    }
    prepare_images(imported)
}

fn prepare_images(
    imported: impl IntoIterator<Item = (AssetRecord, ImageAsset)>,
) -> Result<UiPreparedImages, UiPreviewError> {
    let mut server = AssetServer::new();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let mut prepared_images = UiPreparedImages::new();

    for (record, image) in imported {
        let source_hash = image.source().source_hash();
        let artifact_hash = image.source().artifact().key().digest();
        let path = record.path().as_str().to_owned();
        let stable_id = record.stable_id().to_string();
        let handle = server.reserve_record::<ImageAsset>(&record)?;
        images.commit_loaded(
            handle,
            image,
            &mut states,
            &mut events,
            Some(source_hash),
            Some(artifact_hash),
        )?;
        let image = images
            .get(handle)
            .ok_or(UiPreviewError::ImageSnapshotInvalid)?;
        let state = states
            .state(handle.id())
            .ok_or(UiPreviewError::ImageSnapshotInvalid)?;
        let prepared = PreparedImageSnapshot::new(
            RenderResourceSnapshot::new(
                image_resource_key(handle),
                state.version(),
                images.slot_revision(handle),
                image_descriptor_hash(image),
            ),
            PreparedImageResource::from_image(image),
        );
        for alias in [
            path.clone(),
            format!("path:{path}"),
            format!("project://{path}"),
            stable_id.clone(),
            format!("stable:{stable_id}"),
        ] {
            if let Ok(id) = UiImageId::new(alias) {
                prepared_images.insert(id, prepared.clone());
            }
        }
    }
    Ok(prepared_images)
}

#[derive(Debug, Error)]
pub(super) enum UiPreviewError {
    #[error(transparent)]
    Compile(#[from] UiAuthoringCanvasCompileError),
    #[error(transparent)]
    Presentation(#[from] nara_ui_render::UiPresentationContentError),
    #[error(transparent)]
    Packet(#[from] nara_render::RenderFramePacketError),
    #[error(transparent)]
    Asset(#[from] nara_asset::AssetError),
    #[error(transparent)]
    AssetState(#[from] nara_asset::AssetStateError),
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
    #[error("the project image snapshot could not be retained for the Editor UI Canvas")]
    ImageSnapshotInvalid,
}
