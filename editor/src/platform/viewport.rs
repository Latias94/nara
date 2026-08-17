use egui::{TextureId, Vec2, epaint::Primitive};
use egui_wgpu::wgpu;
use nara::{
    asset::{AssetId, Handle},
    project_host::{
        EditorGameStartActivation, EditorGameStartRequest, EditorGameViewCandidate,
        EditorGameViewCandidateIdentity, EditorGameViewCompletion, EditorPreviewCandidate,
        EditorPreviewHit,
    },
    render::{
        __private::{editor_viewport_image_target, map_editor_game_view_packet},
        Extent2d, RenderFrameSourceGeneration, RenderFrameTarget, RenderImagePacket,
        RenderWindowPacket,
    },
    render_wgpu::__private::{
        WgpuEditorCompositeReceipt, WgpuEditorExecution, WgpuEditorExecutionError,
        WgpuEditorUiImageSubmission, WgpuEditorViewportIdentity,
        WgpuEditorViewportTargetActivation, WgpuSubmittedImageReceipt,
    },
    scene::SceneAuthoringRevision,
    tooling::{EditorAction, EditorActionBatch, EditorDocumentId, EditorFrameModel},
    tooling_egui::{EguiGameViewInput, EguiSceneViewInput},
};

use super::{EditorApplication, EditorEguiRenderer, EditorRunError, platform_gpu_error};

const EDITOR_GAME_VIEW_TARGET_ID: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
enum EditorViewportSource {
    Scene,
    Game(EditorGameStartRequest),
}

#[derive(Clone, Copy)]
struct EditorViewportTextureBinding {
    texture_id: TextureId,
    viewport: WgpuEditorViewportIdentity,
    registration_generation: u64,
    target: RenderImagePacket,
    source: EditorViewportSource,
}

impl EditorViewportTextureBinding {
    fn matches(self, submission: WgpuSubmittedImageReceipt) -> bool {
        self.viewport.matches_submission(submission)
    }

    fn matches_presented(self, receipt: WgpuEditorCompositeReceipt) -> bool {
        self.matches(receipt.image())
            && self.registration_generation == receipt.texture_registration_generation()
    }
}

struct PreparedEditorViewportTarget {
    activation: WgpuEditorViewportTargetActivation,
    binding: EditorViewportTextureBinding,
}

pub(super) struct EditorViewportSlot {
    texture_registration_generation: u64,
    binding: Option<EditorViewportTextureBinding>,
}

impl EditorViewportSlot {
    pub(super) const fn new() -> Self {
        Self {
            texture_registration_generation: 0,
            binding: None,
        }
    }

    pub(super) fn has_binding(&self) -> bool {
        self.binding.is_some()
    }

    pub(super) fn is_game(&self) -> bool {
        self.game_binding().is_some()
    }

    pub(super) fn is_scene(&self) -> bool {
        self.scene_binding().is_some()
    }

    fn game_binding(&self) -> Option<EditorViewportTextureBinding> {
        self.binding
            .filter(|binding| matches!(binding.source, EditorViewportSource::Game(_)))
    }

    fn scene_binding(&self) -> Option<EditorViewportTextureBinding> {
        self.binding
            .filter(|binding| binding.source == EditorViewportSource::Scene)
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
        let (reservation, texture_id) = gpu
            .reserve_viewport_target_with_texture(
                editor_window,
                source_generation,
                target,
                |scope, texture| {
                    renderer.renderer.register_native_texture(
                        scope.device(),
                        texture.view(),
                        wgpu::FilterMode::Linear,
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
                return Ok(binding);
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
}

impl PreparedEditorGame {
    const fn image(&self) -> WgpuSubmittedImageReceipt {
        self.submission.image()
    }
}

pub(super) enum PreparedEditorViewportFrame {
    Scene(PreparedEditorScene),
    Game(PreparedEditorGame),
}

impl PreparedEditorViewportFrame {
    pub(super) const fn image(&self) -> WgpuSubmittedImageReceipt {
        match self {
            Self::Scene(scene) => scene.submission,
            Self::Game(game) => game.image(),
        }
    }

    pub(super) const fn texture_id(&self) -> TextureId {
        match self {
            Self::Scene(scene) => scene.texture_id,
            Self::Game(game) => game.texture_id,
        }
    }

    pub(super) const fn texture_registration_generation(&self) -> u64 {
        match self {
            Self::Scene(scene) => scene.texture_registration_generation,
            Self::Game(game) => game.texture_registration_generation,
        }
    }
}

struct PresentedEditorGame {
    identity: EditorGameViewCandidateIdentity,
    receipt: WgpuEditorCompositeReceipt,
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
    ) -> Result<EditorGamePrepareOutcome, EditorRunError> {
        if let EditorGamePresenterState::CompletionRejected(completion) = &self.state {
            return Err(EditorRunError::Platform(format!(
                "the Game presenter retained a completion rejected by its Runtime: {:?}",
                completion.identity()
            )));
        }
        let (identity, packet, sprites, runtime_ui) =
            candidate.into_parts().into_submission_parts();
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
        }))
    }

    pub(super) fn promote(
        &mut self,
        application: &mut EditorApplication,
        slot: &EditorViewportSlot,
        prepared: PreparedEditorGame,
        receipt: WgpuEditorCompositeReceipt,
    ) -> Result<(), EditorRunError> {
        let Some(binding) = slot.game_binding().filter(|binding| {
            binding.source == EditorViewportSource::Game(prepared.identity.start())
        }) else {
            let error = WgpuEditorExecutionError::StaleImageSubmission;
            let completion = prepared.submission.failed(prepared.identity, &error);
            self.return_completion(application, completion)?;
            return Err(EditorRunError::Platform(
                "the Game composite has no matching active viewport binding".to_owned(),
            ));
        };
        if !binding.matches_presented(receipt)
            || receipt.image() != prepared.image()
            || receipt.texture_registration_generation() != prepared.texture_registration_generation
        {
            let error = WgpuEditorExecutionError::StaleImageSubmission;
            let completion = prepared.submission.failed(prepared.identity, &error);
            self.return_completion(application, completion)?;
            return Err(platform_gpu_error(error));
        }
        let completion = prepared
            .submission
            .presented(prepared.identity, receipt)
            .map_err(|submission| {
                let error = WgpuEditorExecutionError::StaleImageSubmission;
                let completion = submission.failed(prepared.identity, &error);
                self.state = EditorGamePresenterState::CompletionRejected(completion);
                platform_gpu_error(error)
            })?;
        self.return_completion(application, completion)?;
        self.state = EditorGamePresenterState::Presented(PresentedEditorGame {
            identity: prepared.identity,
            receipt,
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
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    geometry_revision: u64,
    hits: Vec<EditorPreviewHit>,
    submission: WgpuSubmittedImageReceipt,
    texture_id: TextureId,
    texture_registration_generation: u64,
}

struct PresentedEditorScene {
    stamp: EditorScenePickStamp,
    hits: Vec<EditorPreviewHit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EditorScenePickStamp {
    pub(super) document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    geometry_revision: u64,
    receipt: WgpuEditorCompositeReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EditorScenePickRequest {
    pub(super) stamp: EditorScenePickStamp,
    pub(super) uv: [f32; 2],
}

pub(super) struct EditorScenePresenter {
    presented: Option<PresentedEditorScene>,
}

impl EditorScenePresenter {
    pub(super) fn new() -> Self {
        Self { presented: None }
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
        let (document, revision, geometry_revision, packet, sprite_batches, hits) =
            candidate.into_parts();
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
            document,
            revision,
            geometry_revision,
            hits,
            submission,
            texture_id: binding.texture_id,
            texture_registration_generation: binding.registration_generation,
        })
    }

    pub(super) fn pick_stamp(
        &self,
        slot: &EditorViewportSlot,
        frame: &EditorFrameModel,
        requested_extent: Option<Extent2d>,
    ) -> Option<EditorScenePickStamp> {
        let presented = self.presented.as_ref()?;
        let binding = slot
            .binding
            .filter(|binding| binding.source == EditorViewportSource::Scene)?;
        let extent = presented.stamp.receipt.image().target().extent();
        if requested_extent != Some(extent)
            || !binding.matches_presented(presented.stamp.receipt)
            || !scene_matches_frame(frame, presented.stamp.document, presented.stamp.revision)
        {
            return None;
        }
        Some(presented.stamp)
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
                && scene_matches_frame(frame, prepared.document, prepared.revision)
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
            let extent = stamp.receipt.image().target().extent();
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
    ) -> Result<Option<nara::scene::SceneEntityId>, ()> {
        let presented = self.presented.as_ref().ok_or(())?;
        if presented.stamp != request.stamp
            || !request.uv.into_iter().all(f32::is_finite)
            || request
                .uv
                .into_iter()
                .any(|value| !(0.0..=1.0).contains(&value))
        {
            return Err(());
        }
        let clip = [request.uv[0] * 2.0 - 1.0, 1.0 - request.uv[1] * 2.0];
        Ok(presented
            .hits
            .iter()
            .rev()
            .find(|hit| hit.contains_clip_point(clip))
            .map(|hit| hit.entity().clone()))
    }

    pub(super) fn promote(
        &mut self,
        prepared: PreparedEditorScene,
        receipt: WgpuEditorCompositeReceipt,
    ) -> Result<(), EditorRunError> {
        if receipt.image() != prepared.submission
            || receipt.texture_registration_generation() != prepared.texture_registration_generation
        {
            return Err(EditorRunError::Platform(
                "the Editor Scene receipt does not match the prepared candidate".to_owned(),
            ));
        }
        self.presented = Some(PresentedEditorScene {
            stamp: EditorScenePickStamp {
                document: prepared.document,
                revision: prepared.revision,
                geometry_revision: prepared.geometry_revision,
                receipt,
            },
            hits: prepared.hits,
        });
        Ok(())
    }

    pub(super) fn retire_presentation(&mut self) {
        self.presented = None;
    }

    pub(super) fn retire_gpu_binding(&mut self) {
        self.presented = None;
    }
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
