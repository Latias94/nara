use egui::{TextureId, Vec2, epaint::Primitive};
use egui_wgpu::wgpu;
use nara::{
    asset::{AssetId, AssetVersion, Handle, ImportArtifactDigest, SourceHash},
    image::ImageColorSpace,
    prelude::Vec2 as NaraVec2,
    project_host::{
        EditorAcceptedPreview, EditorGameStartActivation, EditorGameStartRequest,
        EditorGameViewCandidate, EditorGameViewCandidateIdentity, EditorGameViewCompletion,
        EditorPresentationImage, EditorPresentedToken, EditorPreviewCandidate,
        EditorPreviewPresentation, EditorUiPreviewCandidate,
    },
    render::{
        __private::{editor_viewport_image_target, map_editor_game_view_packet},
        DisplayMapping2d, DisplaySampling, DisplayScale, Extent2d, RenderFrameSourceGeneration,
        RenderFrameTarget, RenderImagePacket, RenderWindowPacket,
    },
    render_wgpu::__private::{
        WgpuEditorCompositeReceipt, WgpuEditorExecution, WgpuEditorExecutionError,
        WgpuEditorUiImageSubmission, WgpuEditorViewportIdentity,
        WgpuEditorViewportTargetActivation, WgpuSubmittedImageReceipt,
    },
    scene::{SceneAuthoringRevision, SceneEntityId},
    sprite_render::__private::SpriteBatches,
    tooling::{
        EditorAction, EditorActionBatch, EditorDocumentId, EditorFrameModel, EditorGestureStamp,
    },
    tooling_egui::{EguiGameViewInput, EguiSceneViewInput, EguiUiCanvasInput},
};

use super::{EditorApplication, EditorEguiRenderer, EditorRunError, platform_gpu_error};

const EDITOR_GAME_VIEW_TARGET_ID: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
enum EditorViewportSource {
    Scene,
    Game(EditorGameStartRequest),
    UiPrefab {
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
    },
}

#[derive(Clone, Copy)]
struct EditorViewportTextureBinding {
    texture_id: TextureId,
    viewport: WgpuEditorViewportIdentity,
    registration_generation: u64,
    target: RenderImagePacket,
    source: EditorViewportSource,
    display_scale: DisplayScale,
}

impl EditorViewportTextureBinding {
    fn matches(self, submission: WgpuSubmittedImageReceipt) -> bool {
        self.viewport.matches_submission(submission)
    }

    fn matches_presented(self, receipt: WgpuEditorCompositeReceipt) -> bool {
        self.matches(receipt.image())
    }
}

struct PreparedEditorViewportTarget {
    activation: WgpuEditorViewportTargetActivation,
    binding: EditorViewportTextureBinding,
}

pub(super) struct EditorViewportSlot {
    texture_registration_generation: u64,
    binding: Option<EditorViewportTextureBinding>,
    game_display_scale: DisplayScale,
}

impl EditorViewportSlot {
    pub(super) const fn new() -> Self {
        Self {
            texture_registration_generation: 0,
            binding: None,
            game_display_scale: DisplayScale::Fit,
        }
    }

    pub(super) fn has_binding(&self) -> bool {
        self.binding.is_some()
    }

    pub(super) const fn game_display_scale(&self) -> DisplayScale {
        self.game_display_scale
    }

    pub(super) fn game_display_scale_pending(&self) -> bool {
        self.game_binding()
            .is_some_and(|binding| binding.display_scale != self.game_display_scale)
    }

    pub(super) fn request_game_display_scale(&mut self, scale: DisplayScale) -> bool {
        let changed = self.game_display_scale != scale;
        self.game_display_scale = scale;
        changed
    }

    fn display_scale(&self, source: EditorViewportSource) -> DisplayScale {
        if matches!(source, EditorViewportSource::Game(_)) {
            self.game_display_scale
        } else {
            DisplayScale::Fit
        }
    }

    pub(super) fn matches_prepared(&self, prepared: &PreparedEditorViewportFrame) -> bool {
        self.binding.is_some_and(|binding| {
            binding.matches(prepared.image())
                && binding.registration_generation == prepared.texture_registration_generation()
        })
    }

    pub(super) fn is_game(&self) -> bool {
        self.game_binding().is_some()
    }

    pub(super) fn is_scene(&self) -> bool {
        self.scene_binding().is_some()
    }

    pub(super) fn is_ui_prefab(
        &self,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
    ) -> bool {
        self.ui_prefab_binding(document, revision).is_some()
    }

    pub(super) fn is_ui_prefab_binding(&self) -> bool {
        self.binding
            .is_some_and(|binding| matches!(binding.source, EditorViewportSource::UiPrefab { .. }))
    }

    fn game_binding(&self) -> Option<EditorViewportTextureBinding> {
        self.binding
            .filter(|binding| matches!(binding.source, EditorViewportSource::Game(_)))
    }

    fn scene_binding(&self) -> Option<EditorViewportTextureBinding> {
        self.binding
            .filter(|binding| binding.source == EditorViewportSource::Scene)
    }

    fn ui_prefab_binding(
        &self,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
    ) -> Option<EditorViewportTextureBinding> {
        self.binding.filter(|binding| {
            binding.source == EditorViewportSource::UiPrefab { document, revision }
        })
    }

    fn prepare_target(
        &self,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut EditorEguiRenderer,
        editor_window: RenderWindowPacket,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
        source: EditorViewportSource,
    ) -> Result<PreparedEditorViewportTarget, EditorRunError> {
        if !renderer.matches(gpu) {
            return Err(EditorRunError::Platform(
                "the viewport renderer does not belong to the live GPU namespace".to_owned(),
            ));
        }
        let registration_generation = self
            .texture_registration_generation
            .checked_add(1)
            .ok_or_else(|| {
                EditorRunError::Platform(
                    "viewport texture registration generation exhausted".to_owned(),
                )
            })?;
        let display_scale = self.display_scale(source);
        let filter = match display_scale.sampling() {
            DisplaySampling::Linear => wgpu::FilterMode::Linear,
            DisplaySampling::Nearest => wgpu::FilterMode::Nearest,
        };
        let (reservation, texture_id) = gpu
            .reserve_viewport_target_with_texture(
                editor_window,
                source_generation,
                target,
                |scope, texture| {
                    renderer.renderer.register_native_texture(
                        scope.device(),
                        texture.view(),
                        filter,
                    )
                },
            )
            .map_err(platform_gpu_error)?;
        let viewport = reservation.identity();
        let activation = match gpu.prepare_viewport_target_activation(reservation) {
            Ok(activation) => activation,
            Err(error) => {
                renderer.renderer.free_texture(&texture_id);
                return Err(platform_gpu_error(error));
            }
        };
        Ok(PreparedEditorViewportTarget {
            activation,
            binding: EditorViewportTextureBinding {
                texture_id,
                viewport,
                registration_generation,
                target,
                source,
                display_scale,
            },
        })
    }

    fn cancel_prepared_target(
        renderer: &mut EditorEguiRenderer,
        prepared: PreparedEditorViewportTarget,
    ) {
        renderer.renderer.free_texture(&prepared.binding.texture_id);
    }

    fn commit_target(
        &mut self,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut EditorEguiRenderer,
        prepared: PreparedEditorViewportTarget,
    ) {
        gpu.commit_viewport_target_activation(prepared.activation);
        if let Some(previous) = self.binding.replace(prepared.binding) {
            renderer.renderer.free_texture(&previous.texture_id);
        }
        self.texture_registration_generation = prepared.binding.registration_generation;
    }

    fn ensure_target(
        &mut self,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut EditorEguiRenderer,
        editor_window: RenderWindowPacket,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
        source: EditorViewportSource,
    ) -> Result<EditorViewportTextureBinding, EditorRunError> {
        if let Some(binding) = self.binding {
            if binding.source != source {
                return Err(EditorRunError::Platform(
                    "the previous viewport source was not retired before replacement".to_owned(),
                ));
            }
            if gpu.viewport_target_matches(source_generation, target) {
                if binding.display_scale == self.display_scale(source) {
                    return Ok(binding);
                }
                let prepared = self.prepare_target(
                    gpu,
                    renderer,
                    editor_window,
                    source_generation,
                    target,
                    source,
                )?;
                self.commit_target(gpu, renderer, prepared);
                return Ok(self
                    .binding
                    .expect("a policy change replaces the native texture binding"));
            }
            return Err(EditorRunError::Platform(
                "the active viewport binding no longer matches its GPU target".to_owned(),
            ));
        }
        let prepared = self.prepare_target(
            gpu,
            renderer,
            editor_window,
            source_generation,
            target,
            source,
        )?;
        self.commit_target(gpu, renderer, prepared);
        Ok(self
            .binding
            .expect("committing a viewport target installs one texture binding"))
    }

    pub(super) fn retire_gpu_binding(&mut self) {
        self.binding = None;
    }

    pub(super) fn retire_target(
        &mut self,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut EditorEguiRenderer,
    ) {
        let Some(binding) = self.binding.take() else {
            return;
        };
        gpu.retire_viewport_target();
        renderer.renderer.free_texture(&binding.texture_id);
    }
}

pub(super) struct PreparedEditorGameStart {
    application: EditorGameStartActivation,
    target: PreparedEditorViewportTarget,
}

enum EditorGameImageSubmission {
    Plain(WgpuSubmittedImageReceipt),
    RuntimeUi(WgpuEditorUiImageSubmission),
}

impl EditorGameImageSubmission {
    const fn image(&self) -> WgpuSubmittedImageReceipt {
        match self {
            Self::Plain(image) => *image,
            Self::RuntimeUi(submission) => submission.image(),
        }
    }

    fn presented(
        self,
        identity: EditorGameViewCandidateIdentity,
        receipt: WgpuEditorCompositeReceipt,
    ) -> Result<EditorGameViewCompletion, Self> {
        match self {
            Self::Plain(image) if receipt.image() == image => {
                Ok(EditorGameViewCompletion::without_ui(identity))
            }
            Self::Plain(image) => Err(Self::Plain(image)),
            Self::RuntimeUi(submission) => submission
                .presented(receipt)
                .map(|outcomes| EditorGameViewCompletion::with_ui(identity, outcomes))
                .map_err(Self::RuntimeUi),
        }
    }

    fn deferred(
        self,
        identity: EditorGameViewCandidateIdentity,
        message: String,
    ) -> EditorGameViewCompletion {
        match self {
            Self::Plain(_) => EditorGameViewCompletion::without_ui_deferred(identity),
            Self::RuntimeUi(submission) => {
                EditorGameViewCompletion::with_ui_deferred(identity, submission.deferred(message))
            }
        }
    }

    fn failed(
        self,
        identity: EditorGameViewCandidateIdentity,
        error: &WgpuEditorExecutionError,
    ) -> EditorGameViewCompletion {
        match self {
            Self::Plain(_) => EditorGameViewCompletion::without_ui_failed(identity),
            Self::RuntimeUi(submission) => {
                EditorGameViewCompletion::with_ui_failed(identity, submission.failed(error))
            }
        }
    }
}

pub(super) struct PreparedEditorGame {
    identity: EditorGameViewCandidateIdentity,
    submission: EditorGameImageSubmission,
    texture_id: TextureId,
    texture_registration_generation: u64,
    canary_facts: Option<EditorGameCandidateFacts>,
}

impl PreparedEditorGame {
    const fn image(&self) -> WgpuSubmittedImageReceipt {
        self.submission.image()
    }
}

pub(super) enum PreparedEditorViewportFrame {
    Scene(PreparedEditorScene),
    Game(PreparedEditorGame),
    UiCanvas(PreparedEditorUiCanvas),
}

impl PreparedEditorViewportFrame {
    pub(super) const fn image(&self) -> WgpuSubmittedImageReceipt {
        match self {
            Self::Scene(scene) => scene.submission,
            Self::Game(game) => game.image(),
            Self::UiCanvas(ui) => ui.submission,
        }
    }

    pub(super) const fn texture_id(&self) -> TextureId {
        match self {
            Self::Scene(scene) => scene.texture_id,
            Self::Game(game) => game.texture_id,
            Self::UiCanvas(ui) => ui.texture_id,
        }
    }

    pub(super) const fn texture_registration_generation(&self) -> u64 {
        match self {
            Self::Scene(scene) => scene.texture_registration_generation,
            Self::Game(game) => game.texture_registration_generation,
            Self::UiCanvas(ui) => ui.texture_registration_generation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EditorUiCanvasIdentity {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    geometry_generation: u64,
    gesture: Option<EditorGestureStamp>,
}

impl EditorUiCanvasIdentity {
    pub(super) const fn new(
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        gesture: Option<EditorGestureStamp>,
        geometry_generation: u64,
    ) -> Self {
        Self {
            document,
            revision,
            geometry_generation,
            gesture,
        }
    }
}

pub(super) struct PreparedEditorUiCanvas {
    identity: EditorUiCanvasIdentity,
    canvas: nara::ui::__private::UiAuthoringCanvasReady,
    images: Box<[EditorUiCanvasImageFact]>,
    submission: WgpuSubmittedImageReceipt,
    texture_id: TextureId,
    texture_registration_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EditorUiCanvasImageFact {
    id: String,
    asset_version: AssetVersion,
    descriptor_hash: ImportArtifactDigest,
    source_hash: SourceHash,
    color_space: ImageColorSpace,
}

impl EditorUiCanvasImageFact {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) const fn asset_version(&self) -> AssetVersion {
        self.asset_version
    }

    pub(super) const fn descriptor_hash(&self) -> ImportArtifactDigest {
        self.descriptor_hash
    }

    pub(super) const fn source_hash(&self) -> SourceHash {
        self.source_hash
    }

    pub(super) const fn color_space(&self) -> ImageColorSpace {
        self.color_space
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EditorUiCanvasPickStamp {
    identity: EditorUiCanvasIdentity,
    receipt: WgpuEditorCompositeReceipt,
}

impl EditorUiCanvasPickStamp {
    pub(super) const fn document(self) -> EditorDocumentId {
        self.identity.document
    }

    pub(super) const fn revision(self) -> SceneAuthoringRevision {
        self.identity.revision
    }

    pub(super) const fn gesture(self) -> Option<EditorGestureStamp> {
        self.identity.gesture
    }

    pub(super) const fn receipt(self) -> WgpuEditorCompositeReceipt {
        self.receipt
    }

    fn matches_current(self, current: Option<EditorUiCanvasIdentity>) -> bool {
        current == Some(self.identity)
    }
}

pub(super) struct EditorUiCanvasPresentedObservation<'a> {
    stamp: EditorUiCanvasPickStamp,
    canvas: &'a nara::ui::__private::UiAuthoringCanvasReady,
    images: &'a [EditorUiCanvasImageFact],
}

impl EditorUiCanvasPresentedObservation<'_> {
    pub(super) const fn stamp(&self) -> EditorUiCanvasPickStamp {
        self.stamp
    }

    pub(super) fn fact_count(&self) -> usize {
        self.canvas.facts().len()
    }

    pub(super) const fn images(&self) -> &[EditorUiCanvasImageFact] {
        self.images
    }

    pub(super) fn entity_origin(&self, entity: &SceneEntityId) -> Option<[f32; 2]> {
        let fact = self
            .canvas
            .facts()
            .iter()
            .find(|fact| fact.entity() == entity && fact.visible())?;
        let origin = fact.rect().origin();
        Some([origin.x(), origin.y()])
    }

    pub(super) fn entity_center_uv(&self, entity: &SceneEntityId) -> Option<[f32; 2]> {
        let fact = self
            .canvas
            .facts()
            .iter()
            .find(|fact| fact.entity() == entity && fact.visible())?;
        let rect = fact.rect();
        let center = [
            (rect.origin().x() + rect.max().x()) * 0.5,
            (rect.origin().y() + rect.max().y()) * 0.5,
        ];
        let transform = self.canvas.content_transform();
        let physical = transform.physical_surface_size();
        let content_origin = transform.physical_content_origin();
        let scale = transform.content_scale_factor();
        let uv = [
            (content_origin.x() + center[0] * scale) / physical.width(),
            (content_origin.y() + center[1] * scale) / physical.height(),
        ];
        is_normalized_viewport_uv(uv).then_some(uv)
    }
}

struct PresentedEditorUiCanvas {
    stamp: EditorUiCanvasPickStamp,
    canvas: nara::ui::__private::UiAuthoringCanvasReady,
    images: Box<[EditorUiCanvasImageFact]>,
}

pub(super) struct EditorUiCanvasPresenter {
    capture_image_facts: bool,
    presented: Option<PresentedEditorUiCanvas>,
}

impl EditorUiCanvasPresenter {
    pub(super) const fn new(capture_image_facts: bool) -> Self {
        Self {
            capture_image_facts,
            presented: None,
        }
    }

    pub(super) fn prepare(
        &mut self,
        slot: &mut EditorViewportSlot,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut EditorEguiRenderer,
        window: RenderWindowPacket,
        candidate: EditorUiPreviewCandidate,
    ) -> Result<PreparedEditorUiCanvas, EditorRunError> {
        let (document, revision, gesture, geometry_generation, packet, paint_packet, canvas) =
            candidate.into_parts();
        let identity =
            EditorUiCanvasIdentity::new(document, revision, gesture, geometry_generation);
        let images = if self.capture_image_facts {
            paint_packet
                .images()
                .iter()
                .map(|image| {
                    let snapshot = image.snapshot();
                    EditorUiCanvasImageFact {
                        id: image.id().as_str().to_owned(),
                        asset_version: snapshot.identity().asset_version(),
                        descriptor_hash: snapshot.identity().descriptor_hash(),
                        source_hash: snapshot.resource().source_hash(),
                        color_space: snapshot.resource().color_space(),
                    }
                })
                .collect::<Vec<_>>()
                .into_boxed_slice()
        } else {
            Box::default()
        };
        let RenderFrameTarget::Image(target) = packet.target() else {
            return Err(EditorRunError::Platform(
                "the UI Canvas candidate did not retain its image target".to_owned(),
            ));
        };
        let logical = canvas.logical_size();
        let extent = target.extent();
        if logical.width() != extent.width as f32 || logical.height() != extent.height as f32 {
            return Err(EditorRunError::Platform(
                "the UI Canvas logical extent does not match its image target".to_owned(),
            ));
        }
        let binding = slot.ensure_target(
            gpu,
            renderer,
            window,
            packet.source_generation(),
            target,
            EditorViewportSource::UiPrefab { document, revision },
        )?;
        let submission = gpu
            .submit_authoring_ui_image(window, packet, paint_packet)
            .map_err(platform_gpu_error)?;
        if !binding.matches(submission) {
            return Err(EditorRunError::Platform(
                "the submitted UI Canvas did not match its committed viewport reservation"
                    .to_owned(),
            ));
        }
        Ok(PreparedEditorUiCanvas {
            identity,
            canvas,
            images,
            submission,
            texture_id: binding.texture_id,
            texture_registration_generation: binding.registration_generation,
        })
    }

    pub(super) fn view_input(
        &self,
        slot: &EditorViewportSlot,
        prepared: Option<&PreparedEditorUiCanvas>,
        current: Option<EditorUiCanvasIdentity>,
    ) -> EguiUiCanvasInput {
        if let Some(prepared) = prepared {
            let extent = prepared.submission.target().extent();
            return EguiUiCanvasInput::previewing(
                prepared.texture_id,
                [extent.width, extent.height],
            );
        }
        let Some(presented) = self.presented.as_ref() else {
            return EguiUiCanvasInput::unavailable();
        };
        let Some(binding) = slot.ui_prefab_binding(
            presented.stamp.identity.document,
            presented.stamp.identity.revision,
        ) else {
            return EguiUiCanvasInput::unavailable();
        };
        if !binding.matches_presented(presented.stamp.receipt) {
            return EguiUiCanvasInput::unavailable();
        }
        let extent = presented.stamp.receipt.image().target().extent();
        if presented.stamp.matches_current(current) {
            EguiUiCanvasInput::presented(binding.texture_id, [extent.width, extent.height])
        } else {
            EguiUiCanvasInput::previewing(binding.texture_id, [extent.width, extent.height])
        }
    }

    pub(super) fn pick_stamp(
        &self,
        slot: &EditorViewportSlot,
        current: Option<EditorUiCanvasIdentity>,
    ) -> Option<EditorUiCanvasPickStamp> {
        let presented = self.presented.as_ref()?;
        if !presented.stamp.matches_current(current) {
            return None;
        }
        let binding = slot.ui_prefab_binding(
            presented.stamp.identity.document,
            presented.stamp.identity.revision,
        )?;
        binding
            .matches_presented(presented.stamp.receipt)
            .then_some(presented.stamp)
    }

    pub(super) fn presented_observation(
        &self,
        slot: &EditorViewportSlot,
        current: Option<EditorUiCanvasIdentity>,
    ) -> Option<EditorUiCanvasPresentedObservation<'_>> {
        let stamp = self.pick_stamp(slot, current)?;
        let presented = self.presented.as_ref()?;
        (presented.stamp == stamp).then_some(EditorUiCanvasPresentedObservation {
            stamp,
            canvas: &presented.canvas,
            images: &presented.images,
        })
    }

    pub(super) fn resolve_pick(
        &self,
        request: EditorUiCanvasPickRequest,
    ) -> Result<Option<nara::scene::SceneEntityId>, ()> {
        let presented = self.presented.as_ref().ok_or(())?;
        if presented.stamp != request.stamp || !is_normalized_viewport_uv(request.uv) {
            return Err(());
        }
        Ok(presented.canvas.source_at_surface_uv(request.uv).cloned())
    }

    pub(super) fn promote(
        &mut self,
        prepared: PreparedEditorUiCanvas,
        receipt: WgpuEditorCompositeReceipt,
    ) -> Result<(), EditorRunError> {
        if receipt.image() != prepared.submission {
            return Err(EditorRunError::Platform(
                "the Editor UI Canvas receipt does not match the prepared candidate".to_owned(),
            ));
        }
        self.presented = Some(PresentedEditorUiCanvas {
            stamp: EditorUiCanvasPickStamp {
                identity: prepared.identity,
                receipt,
            },
            canvas: prepared.canvas,
            images: prepared.images,
        });
        Ok(())
    }

    pub(super) fn retire_presentation(&mut self) {
        self.presented = None;
    }

    pub(super) fn retire_gpu_binding(&mut self) {
        self.retire_presentation();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EditorUiCanvasPickRequest {
    pub(super) stamp: EditorUiCanvasPickStamp,
    pub(super) uv: [f32; 2],
}

struct PresentedEditorGame {
    identity: EditorGameViewCandidateIdentity,
    receipt: WgpuEditorCompositeReceipt,
    canary_facts: Option<EditorGameCandidateFacts>,
}

#[derive(Debug)]
struct EditorGameCandidateFacts {
    total_sprite_instances: usize,
    tracked_center: Option<NaraVec2>,
    runtime_ui_present: bool,
    runtime_ui_blocks_unhandled_input: bool,
}

impl EditorGameCandidateFacts {
    fn new(
        sprites: Option<&SpriteBatches>,
        runtime_ui_present: bool,
        runtime_ui_blocks_unhandled_input: bool,
        tracked_sprite_sort_key: i32,
    ) -> Self {
        let total_sprite_instances = sprites.map_or(0, SpriteBatches::total_instances);
        let tracked_center = sprites.and_then(|sprites| {
            let mut centers = sprites
                .as_slice()
                .iter()
                .filter(|batch| batch.sort_key == tracked_sprite_sort_key)
                .flat_map(|batch| batch.instances.iter().map(|instance| instance.center));
            let center = centers.next()?;
            centers.next().is_none().then_some(center)
        });
        Self {
            total_sprite_instances,
            tracked_center,
            runtime_ui_present,
            runtime_ui_blocks_unhandled_input,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct EditorGamePresentedObservation<'a> {
    stamp: EditorGamePresentedStamp,
    content: &'a EditorGameCandidateFacts,
}

impl EditorGamePresentedObservation<'_> {
    pub(super) const fn stamp(&self) -> EditorGamePresentedStamp {
        self.stamp
    }

    pub(super) const fn runtime_frame_index(&self) -> u64 {
        self.stamp.identity.frame_index()
    }

    pub(super) const fn receipt(&self) -> WgpuEditorCompositeReceipt {
        self.stamp.receipt
    }

    pub(super) fn total_sprite_instances(&self) -> usize {
        self.content.total_sprite_instances
    }

    pub(super) const fn runtime_ui_present(&self) -> bool {
        self.content.runtime_ui_present
    }

    pub(super) const fn runtime_ui_blocks_unhandled_input(&self) -> bool {
        self.content.runtime_ui_blocks_unhandled_input
    }

    pub(super) const fn tracked_center(&self) -> Option<NaraVec2> {
        self.content.tracked_center
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EditorGamePresentedStamp {
    identity: EditorGameViewCandidateIdentity,
    receipt: WgpuEditorCompositeReceipt,
}

impl EditorGamePresentedStamp {
    pub(super) const fn identity(self) -> EditorGameViewCandidateIdentity {
        self.identity
    }

    pub(super) const fn logical_window(self) -> RenderWindowPacket {
        self.identity.start().logical_window()
    }

    pub(super) const fn runtime_generation(self) -> u64 {
        self.identity.runtime_generation().get()
    }

    pub(super) const fn source_handoff_generation(self) -> u64 {
        self.receipt.image().source_handoff_generation()
    }

    pub(super) const fn target_generation(self) -> u64 {
        self.receipt.image().target_generation()
    }

    pub(super) const fn geometry_generation(self) -> u64 {
        self.identity.start().geometry_generation()
    }
}

enum EditorGamePresenterState {
    Empty,
    Presented(PresentedEditorGame),
    CompletionRejected(EditorGameViewCompletion),
}

pub(super) enum EditorGamePrepareOutcome {
    Prepared(PreparedEditorGame),
    Deferred,
    ContentRejected,
}

pub(super) struct EditorGamePresenter {
    state: EditorGamePresenterState,
}

impl EditorGamePresenter {
    pub(super) const fn new() -> Self {
        Self {
            state: EditorGamePresenterState::Empty,
        }
    }

    pub(super) fn prepare_start(
        &self,
        application: &mut EditorApplication,
        slot: &EditorViewportSlot,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut EditorEguiRenderer,
        editor_window: RenderWindowPacket,
        request: EditorGameStartRequest,
    ) -> Result<PreparedEditorGameStart, EditorRunError> {
        if slot.is_game() {
            return Err(EditorRunError::Platform(
                "a Game viewport target is already active".to_owned(),
            ));
        }
        let (source_generation, target) = game_view_target(request)?;
        let target = slot.prepare_target(
            gpu,
            renderer,
            editor_window,
            source_generation,
            target,
            EditorViewportSource::Game(request),
        )?;
        let application_activation = match application.prepare_game_start_activation(request) {
            Ok(activation) => activation,
            Err(error) => {
                EditorViewportSlot::cancel_prepared_target(renderer, target);
                return Err(EditorRunError::Platform(format!(
                    "the Game Runtime start reservation was rejected: {error}"
                )));
            }
        };
        Ok(PreparedEditorGameStart {
            application: application_activation,
            target,
        })
    }

    pub(super) fn ensure_target(
        &self,
        slot: &mut EditorViewportSlot,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut EditorEguiRenderer,
        editor_window: RenderWindowPacket,
        request: EditorGameStartRequest,
    ) -> Result<(), EditorRunError> {
        let (source_generation, target) = game_view_target(request)?;
        let _ = slot.ensure_target(
            gpu,
            renderer,
            editor_window,
            source_generation,
            target,
            EditorViewportSource::Game(request),
        )?;
        Ok(())
    }

    pub(super) fn commit_start(
        &mut self,
        application: &mut EditorApplication,
        slot: &mut EditorViewportSlot,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut EditorEguiRenderer,
        scene: &mut EditorScenePresenter,
        prepared: PreparedEditorGameStart,
    ) {
        slot.commit_target(gpu, renderer, prepared.target);
        scene.retire_presentation();
        application.commit_game_start_activation(prepared.application);
        self.state = EditorGamePresenterState::Empty;
    }

    pub(super) fn retire_gpu_binding(&mut self) {
        self.retire_presentation();
    }

    pub(super) fn retire_presentation(&mut self) {
        if matches!(self.state, EditorGamePresenterState::Presented(_)) {
            self.state = EditorGamePresenterState::Empty;
        }
    }

    pub(super) fn prepare_candidate(
        &mut self,
        application: &mut EditorApplication,
        slot: &EditorViewportSlot,
        gpu: &mut WgpuEditorExecution,
        editor_window: RenderWindowPacket,
        candidate: EditorGameViewCandidate,
        canary_sprite_sort_key: Option<i32>,
    ) -> Result<EditorGamePrepareOutcome, EditorRunError> {
        if let EditorGamePresenterState::CompletionRejected(completion) = &self.state {
            return Err(EditorRunError::Platform(format!(
                "the Game presenter retained a completion rejected by its Runtime: {:?}",
                completion.identity()
            )));
        }
        let (identity, packet, sprites, runtime_ui) =
            candidate.into_parts().into_submission_parts();
        let canary_facts = canary_sprite_sort_key.map(|tracked_sprite_sort_key| {
            EditorGameCandidateFacts::new(
                sprites.as_ref(),
                runtime_ui.as_ref().is_some_and(|frame| frame.has_packets()),
                runtime_ui
                    .as_ref()
                    .is_some_and(|frame| frame.blocks_unhandled_input()),
                tracked_sprite_sort_key,
            )
        });
        let Some(binding) = slot
            .binding
            .filter(|binding| matches!(binding.source, EditorViewportSource::Game(_)))
        else {
            let completion = match runtime_ui {
                Some(runtime_ui) => EditorGameViewCompletion::with_ui_failed(
                    identity,
                    WgpuEditorExecution::reject_editor_ui_frame(
                        runtime_ui,
                        "the Game candidate has no active viewport texture binding",
                    ),
                ),
                None => EditorGameViewCompletion::without_ui_failed(identity),
            };
            self.return_completion(application, completion)?;
            return Err(EditorRunError::Platform(
                "a Game candidate has no active viewport texture binding".to_owned(),
            ));
        };
        if binding.source != EditorViewportSource::Game(identity.start()) {
            let completion = match runtime_ui {
                Some(runtime_ui) => EditorGameViewCompletion::with_ui_failed(
                    identity,
                    WgpuEditorExecution::reject_editor_ui_frame(
                        runtime_ui,
                        "the Game candidate belongs to a foreign Runtime start",
                    ),
                ),
                None => EditorGameViewCompletion::without_ui_failed(identity),
            };
            self.return_completion(application, completion)?;
            return Err(EditorRunError::Platform(
                "the Game candidate belongs to a foreign Runtime start".to_owned(),
            ));
        }
        let packet = match map_editor_game_view_packet(packet, binding.target) {
            Ok(packet) => packet,
            Err(error) => {
                let completion = match runtime_ui {
                    Some(runtime_ui) => EditorGameViewCompletion::with_ui_failed(
                        identity,
                        WgpuEditorExecution::reject_editor_ui_frame(runtime_ui, error.to_string()),
                    ),
                    None => EditorGameViewCompletion::without_ui_failed(identity),
                };
                self.return_completion(application, completion)?;
                return Err(EditorRunError::Platform(error.to_string()));
            }
        };

        let submission = match (sprites, runtime_ui) {
            (Some(sprites), Some(runtime_ui)) => {
                match gpu.submit_sprite_ui_image(editor_window, packet, sprites, runtime_ui) {
                    Ok(submission) => EditorGameImageSubmission::RuntimeUi(submission),
                    Err(failure) => {
                        let (error, outcomes) = failure.complete();
                        let completion = if error.is_deferred() {
                            EditorGameViewCompletion::with_ui_deferred(identity, outcomes)
                        } else if error.is_content_rejected() {
                            EditorGameViewCompletion::with_ui_content_rejected(identity, outcomes)
                        } else {
                            EditorGameViewCompletion::with_ui_failed(identity, outcomes)
                        };
                        self.return_completion(application, completion)?;
                        return classify_game_prepare_failure(error);
                    }
                }
            }
            (Some(sprites), None) => {
                match gpu.submit_sprite_image(editor_window, packet, sprites) {
                    Ok(submission) => EditorGameImageSubmission::Plain(submission),
                    Err(error) => {
                        self.return_completion(
                            application,
                            plain_completion_for_error(identity, &error),
                        )?;
                        return classify_game_prepare_failure(error);
                    }
                }
            }
            (None, Some(runtime_ui)) => {
                match gpu.submit_ui_image(editor_window, packet, runtime_ui) {
                    Ok(submission) => EditorGameImageSubmission::RuntimeUi(submission),
                    Err(failure) => {
                        let (error, outcomes) = failure.complete();
                        let completion = if error.is_deferred() {
                            EditorGameViewCompletion::with_ui_deferred(identity, outcomes)
                        } else if error.is_content_rejected() {
                            EditorGameViewCompletion::with_ui_content_rejected(identity, outcomes)
                        } else {
                            EditorGameViewCompletion::with_ui_failed(identity, outcomes)
                        };
                        self.return_completion(application, completion)?;
                        return classify_game_prepare_failure(error);
                    }
                }
            }
            (None, None) => match gpu.submit_image(editor_window, packet) {
                Ok(submission) => EditorGameImageSubmission::Plain(submission),
                Err(error) => {
                    self.return_completion(
                        application,
                        plain_completion_for_error(identity, &error),
                    )?;
                    return classify_game_prepare_failure(error);
                }
            },
        };
        if !binding.matches(submission.image()) {
            let error = WgpuEditorExecutionError::StaleImageSubmission;
            let completion = submission.failed(identity, &error);
            self.return_completion(application, completion)?;
            return Err(platform_gpu_error(error));
        }
        Ok(EditorGamePrepareOutcome::Prepared(PreparedEditorGame {
            identity,
            submission,
            texture_id: binding.texture_id,
            texture_registration_generation: binding.registration_generation,
            canary_facts,
        }))
    }

    pub(super) fn promote(
        &mut self,
        application: &mut EditorApplication,
        slot: &EditorViewportSlot,
        prepared: PreparedEditorGame,
        receipt: WgpuEditorCompositeReceipt,
    ) -> Result<(), EditorRunError> {
        let PreparedEditorGame {
            identity,
            submission,
            texture_id: _,
            texture_registration_generation,
            canary_facts,
        } = prepared;
        let Some(binding) = slot
            .game_binding()
            .filter(|binding| binding.source == EditorViewportSource::Game(identity.start()))
        else {
            let error = WgpuEditorExecutionError::StaleImageSubmission;
            let completion = submission.failed(identity, &error);
            self.return_completion(application, completion)?;
            return Err(EditorRunError::Platform(
                "the Game composite has no matching active viewport binding".to_owned(),
            ));
        };
        if !binding.matches_presented(receipt)
            || receipt.image() != submission.image()
            || binding.registration_generation != texture_registration_generation
        {
            let error = WgpuEditorExecutionError::StaleImageSubmission;
            let completion = submission.failed(identity, &error);
            self.return_completion(application, completion)?;
            return Err(platform_gpu_error(error));
        }
        let completion = submission
            .presented(identity, receipt)
            .map_err(|submission| {
                let error = WgpuEditorExecutionError::StaleImageSubmission;
                let completion = submission.failed(identity, &error);
                self.state = EditorGamePresenterState::CompletionRejected(completion);
                platform_gpu_error(error)
            })?;
        self.return_completion(application, completion)?;
        self.state = EditorGamePresenterState::Presented(PresentedEditorGame {
            identity,
            receipt,
            canary_facts,
        });
        Ok(())
    }

    pub(super) fn defer(
        &mut self,
        application: &mut EditorApplication,
        prepared: PreparedEditorGame,
        message: String,
    ) -> Result<(), EditorRunError> {
        let completion = prepared.submission.deferred(prepared.identity, message);
        self.return_completion(application, completion)
    }

    pub(super) fn fail_prepared(
        &mut self,
        application: &mut EditorApplication,
        prepared: PreparedEditorGame,
        error: &WgpuEditorExecutionError,
    ) -> Result<(), EditorRunError> {
        let completion = prepared.submission.failed(prepared.identity, error);
        self.return_completion(application, completion)
    }

    pub(super) fn view_input(
        &self,
        slot: &EditorViewportSlot,
        frame: &EditorFrameModel,
        prepared: Option<&PreparedEditorGame>,
    ) -> EguiGameViewInput {
        if let Some(prepared) = prepared {
            let extent = prepared.image().target().extent();
            return EguiGameViewInput::updating(prepared.texture_id, [extent.width, extent.height]);
        }
        let EditorGamePresenterState::Presented(presented) = &self.state else {
            return EguiGameViewInput::waiting();
        };
        let Some(binding) = slot.game_binding() else {
            return EguiGameViewInput::waiting();
        };
        if !binding.matches_presented(presented.receipt)
            || binding.source != EditorViewportSource::Game(presented.identity.start())
            || frame.play().generation() != Some(presented.identity.runtime_generation().get())
        {
            return EguiGameViewInput::waiting();
        }
        let extent = presented.receipt.image().target().extent();
        EguiGameViewInput::presented(binding.texture_id, [extent.width, extent.height])
    }

    pub(super) fn input_stamp(&self) -> Option<EditorGamePresentedStamp> {
        match &self.state {
            EditorGamePresenterState::Presented(presented) => Some(EditorGamePresentedStamp {
                identity: presented.identity,
                receipt: presented.receipt,
            }),
            EditorGamePresenterState::Empty | EditorGamePresenterState::CompletionRejected(_) => {
                None
            }
        }
    }

    pub(super) fn presented_observation(&self) -> Option<EditorGamePresentedObservation<'_>> {
        match &self.state {
            EditorGamePresenterState::Presented(presented) => {
                presented
                    .canary_facts
                    .as_ref()
                    .map(|content| EditorGamePresentedObservation {
                        stamp: EditorGamePresentedStamp {
                            identity: presented.identity,
                            receipt: presented.receipt,
                        },
                        content,
                    })
            }
            EditorGamePresenterState::Empty | EditorGamePresenterState::CompletionRejected(_) => {
                None
            }
        }
    }

    fn return_completion(
        &mut self,
        application: &mut EditorApplication,
        completion: EditorGameViewCompletion,
    ) -> Result<(), EditorRunError> {
        match application.complete_game_view_candidate(completion) {
            Ok(()) => Ok(()),
            Err(completion) => {
                self.state = EditorGamePresenterState::CompletionRejected(completion);
                Err(EditorRunError::Platform(
                    "the owning Runtime rejected its exact Game View completion".to_owned(),
                ))
            }
        }
    }
}

fn classify_game_prepare_failure(
    error: WgpuEditorExecutionError,
) -> Result<EditorGamePrepareOutcome, EditorRunError> {
    if error.is_deferred() {
        Ok(EditorGamePrepareOutcome::Deferred)
    } else if error.is_content_rejected() {
        Ok(EditorGamePrepareOutcome::ContentRejected)
    } else {
        Err(platform_gpu_error(error))
    }
}

fn plain_completion_for_error(
    identity: EditorGameViewCandidateIdentity,
    error: &WgpuEditorExecutionError,
) -> EditorGameViewCompletion {
    if error.is_deferred() {
        EditorGameViewCompletion::without_ui_deferred(identity)
    } else if error.is_content_rejected() {
        EditorGameViewCompletion::without_ui_content_rejected(identity)
    } else {
        EditorGameViewCompletion::without_ui_failed(identity)
    }
}

fn game_view_target(
    request: EditorGameStartRequest,
) -> Result<(RenderFrameSourceGeneration, RenderImagePacket), EditorRunError> {
    let logical_window = request.logical_window();
    let extent = Extent2d::new(
        logical_window.resolution.physical_width,
        logical_window.resolution.physical_height,
    )
    .ok_or_else(|| {
        EditorRunError::Platform("the Game logical window has zero extent".to_owned())
    })?;
    let target = editor_viewport_image_target(
        Handle::new(AssetId::from_raw(EDITOR_GAME_VIEW_TARGET_ID)),
        extent,
    )
    .map_err(|error| EditorRunError::Platform(error.to_string()))?;
    Ok((
        RenderFrameSourceGeneration::from(request.runtime_generation()),
        target,
    ))
}

pub(super) struct PreparedEditorScene {
    accepted: EditorAcceptedPreview,
    submission: WgpuSubmittedImageReceipt,
    texture_id: TextureId,
    texture_registration_generation: u64,
}

struct PresentedEditorScene {
    stamp: EditorScenePickStamp,
    receipt: WgpuEditorCompositeReceipt,
    mapping: DisplayMapping2d,
}

pub(super) type EditorScenePickStamp = EditorPresentedToken;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EditorScenePickRequest {
    pub(super) stamp: EditorScenePickStamp,
    pub(super) point: [f32; 2],
    pub(super) mapping: DisplayMapping2d,
}

pub(super) struct EditorScenePresenter {
    authority: EditorPreviewPresentation,
    presented: Option<PresentedEditorScene>,
}

impl EditorScenePresenter {
    pub(super) fn new() -> Self {
        Self {
            authority: EditorPreviewPresentation::new(),
            presented: None,
        }
    }

    pub(super) fn prepare(
        &mut self,
        slot: &mut EditorViewportSlot,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut EditorEguiRenderer,
        window: RenderWindowPacket,
        candidate: EditorPreviewCandidate,
    ) -> Result<PreparedEditorScene, EditorRunError> {
        if !renderer.matches(gpu) {
            return Err(EditorRunError::Platform(
                "the Scene renderer does not belong to the live GPU namespace".to_owned(),
            ));
        }
        let (accepted, packet, sprite_batches) = self
            .authority
            .accept(candidate)
            .map_err(|error| EditorRunError::Platform(error.to_string()))?;
        let RenderFrameTarget::Image(target) = packet.target() else {
            return Err(EditorRunError::Platform(
                "the Scene preview candidate did not retain its image target".to_owned(),
            ));
        };
        let source_generation = packet.source_generation();
        let binding = slot.ensure_target(
            gpu,
            renderer,
            window,
            source_generation,
            target,
            EditorViewportSource::Scene,
        )?;
        let submission = gpu
            .submit_sprite_image(window, packet, sprite_batches)
            .map_err(platform_gpu_error)?;
        if !binding.matches(submission) {
            return Err(EditorRunError::Platform(
                "the submitted Scene frame did not match its committed viewport reservation"
                    .to_owned(),
            ));
        }

        Ok(PreparedEditorScene {
            accepted,
            submission,
            texture_id: binding.texture_id,
            texture_registration_generation: binding.registration_generation,
        })
    }

    pub(super) fn pick_stamp(
        &mut self,
        slot: &EditorViewportSlot,
        frame: &EditorFrameModel,
        requested_extent: Option<Extent2d>,
    ) -> Option<EditorScenePickStamp> {
        let presented = self.presented.as_ref()?;
        let binding = slot
            .binding
            .filter(|binding| binding.source == EditorViewportSource::Scene)?;
        let extent = presented.receipt.image().target().extent();
        if requested_extent != Some(extent) || !binding.matches_presented(presented.receipt) {
            return None;
        }
        let scene = frame.workspace().active_scene.as_ref()?;
        self.authority.token(
            scene.document,
            scene.revision,
            presentation_image(presented.receipt.image()),
            presented.mapping,
        )
    }

    pub(super) fn scene_view_input(
        &self,
        slot: &EditorViewportSlot,
        frame: &EditorFrameModel,
        requested_extent: Option<Extent2d>,
        prepared: Option<&PreparedEditorScene>,
        pick_stamp: Option<EditorScenePickStamp>,
        retry_pending: bool,
    ) -> EguiSceneViewInput {
        if retry_pending {
            return EguiSceneViewInput::resizing();
        }
        if let Some(prepared) = prepared {
            let extent = prepared.submission.target().extent();
            if requested_extent == Some(extent)
                && scene_matches_frame(
                    frame,
                    prepared.accepted.document(),
                    prepared.accepted.revision(),
                )
            {
                let extent = [extent.width, extent.height];
                return if pick_stamp.is_some() {
                    EguiSceneViewInput::presented(prepared.texture_id, extent)
                } else {
                    EguiSceneViewInput::previewing(prepared.texture_id, extent)
                };
            }
        }
        if let Some(stamp) = pick_stamp {
            let presented = self
                .presented
                .as_ref()
                .expect("pickable Scene View state owns one presented lease");
            debug_assert_eq!(presented.stamp, stamp);
            let binding = self
                .presented
                .as_ref()
                .and_then(|_| slot.scene_binding())
                .expect("pickable Scene View state owns one texture binding");
            let extent = presented.receipt.image().target().extent();
            return EguiSceneViewInput::presented(
                binding.texture_id,
                [extent.width, extent.height],
            );
        }
        if requested_extent.is_some() {
            EguiSceneViewInput::resizing()
        } else {
            EguiSceneViewInput::unavailable()
        }
    }

    pub(super) fn resolve_pick(
        &self,
        request: EditorScenePickRequest,
    ) -> Result<(EditorDocumentId, Option<nara::scene::SceneEntityId>), ()> {
        let presented = self.presented.as_ref().ok_or(())?;
        if presented.stamp != request.stamp || presented.mapping != request.mapping {
            return Err(());
        }
        let document = self.authority.document(request.stamp).ok_or(())?;
        let entity = self
            .authority
            .pick(request.stamp, request.point)
            .map_err(|_| ())?;
        Ok((document, entity))
    }

    pub(super) fn promote(
        &mut self,
        prepared: PreparedEditorScene,
        receipt: WgpuEditorCompositeReceipt,
        mapping: DisplayMapping2d,
    ) -> Result<(), EditorRunError> {
        if receipt.image() != prepared.submission {
            return Err(EditorRunError::Platform(
                "the Editor Scene receipt does not match the prepared candidate".to_owned(),
            ));
        }
        let stamp = self
            .authority
            .finish(
                prepared.accepted,
                presentation_image(receipt.image()),
                mapping,
            )
            .map_err(|error| EditorRunError::Platform(error.to_string()))?;
        self.presented = Some(PresentedEditorScene {
            stamp,
            receipt,
            mapping,
        });
        Ok(())
    }

    pub(super) fn retire_presentation(&mut self) {
        self.authority.retire();
        self.presented = None;
    }

    pub(super) fn retire_gpu_binding(&mut self) {
        self.authority.retire();
        self.presented = None;
    }
}

fn presentation_image(image: WgpuSubmittedImageReceipt) -> EditorPresentationImage {
    EditorPresentationImage {
        target: image.target(),
        target_generation: image.target_generation(),
        source_generation: image.source_generation(),
        source_handoff_generation: image.source_handoff_generation(),
        backend_instance_id: image.backend_instance_id(),
        device_epoch: image.device_epoch(),
        frame_index: image.render_transaction_frame_index(),
    }
}

fn is_normalized_viewport_uv(uv: [f32; 2]) -> bool {
    uv.into_iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
}

pub(super) fn primitives_sample_texture(
    primitives: &[egui::ClippedPrimitive],
    texture_id: TextureId,
) -> bool {
    primitives.iter().any(|primitive| {
        matches!(
            &primitive.primitive,
            Primitive::Mesh(mesh) if mesh.texture_id == texture_id
        )
    })
}

fn scene_matches_frame(
    frame: &EditorFrameModel,
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
) -> bool {
    frame
        .workspace()
        .active_scene
        .as_ref()
        .is_some_and(|scene| scene.document == document && scene.revision == revision)
}

pub(super) fn append_editor_action(
    batch: EditorActionBatch,
    action: EditorAction,
) -> EditorActionBatch {
    let revision = batch.frame_revision();
    let mut actions = batch.into_actions();
    actions.push(action);
    EditorActionBatch::new(revision, actions)
}

pub(super) fn set_requested_scene_extent(
    application: &mut EditorApplication,
    requested_extent: &mut Option<Extent2d>,
    extent: Option<Extent2d>,
) {
    *requested_extent = extent;
    application.set_scene_preview_extent(extent);
}

pub(super) fn physical_viewport_extent(points: Vec2, pixels_per_point: f32) -> Option<Extent2d> {
    if !points.is_finite() || !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return None;
    }
    let width = (points.x * pixels_per_point).round() as u32;
    let height = (points.y * pixels_per_point).round() as u32;
    Extent2d::new(width, height)
}
