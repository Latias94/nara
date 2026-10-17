use dear_imgui_wgpu::ExternalTextureId;
use nara::{
    project_host::{
        EditorAcceptedPreview, EditorPresentationImage, EditorPresentedToken,
        EditorPreviewCandidate, EditorPreviewContentStamp, EditorPreviewPresentation,
    },
    render::{DisplayMapping2d, Extent2d, RenderFrameTarget, RenderWindowPacket},
    render_wgpu::__private::{
        WgpuEditorCompositeReceipt, WgpuEditorExecution, WgpuSubmittedImageReceipt,
    },
    tooling::{EditorAction, EditorFrameModel, EditorNavigationAction},
};

use super::{EditorRunError, ImguiRenderer, ImguiTarget, imgui_error, platform_gpu_error};

struct PreparedScene {
    content: EditorPreviewContentStamp,
    accepted: EditorAcceptedPreview,
    submission: WgpuSubmittedImageReceipt,
    texture: ExternalTextureId,
}

struct PresentedScene {
    content: EditorPreviewContentStamp,
    token: EditorPresentedToken,
    receipt: WgpuEditorCompositeReceipt,
    mapping: DisplayMapping2d,
    texture: ExternalTextureId,
}

#[derive(Default)]
pub(super) struct ImguiScenePresenter {
    authority: EditorPreviewPresentation,
    prepared: Option<PreparedScene>,
    presented: Option<PresentedScene>,
}

impl ImguiScenePresenter {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn prepare(
        &mut self,
        target: &mut ImguiTarget,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut ImguiRenderer,
        window: RenderWindowPacket,
        candidate: EditorPreviewCandidate,
        next_view: &mut u64,
    ) -> Result<(), EditorRunError> {
        let content = candidate.content_stamp();
        if self.prepared.as_ref().is_some_and(|prepared| {
            prepared.content == content
                && matches_target(target, prepared.submission, prepared.texture)
        }) || self.presented.as_ref().is_some_and(|presented| {
            presented.content == content
                && matches_target(target, presented.receipt.image(), presented.texture)
        }) {
            return Ok(());
        }
        self.retire();
        let (accepted, packet, sprites) = self.authority.accept(candidate).map_err(imgui_error)?;
        let RenderFrameTarget::Image(image) = packet.target() else {
            return Err(imgui_error("Scene preview did not retain an image target"));
        };
        target.ensure(
            gpu,
            renderer,
            window,
            packet.source_generation(),
            image,
            next_view,
        )?;
        let view = target
            .view
            .ok_or_else(|| imgui_error("Scene target has no render view"))?;
        let texture = target
            .texture
            .ok_or_else(|| imgui_error("Scene target has no toolkit texture"))?;
        let submission = gpu
            .submit_sprite_image(view, window, packet, sprites)
            .map_err(platform_gpu_error)?;
        if !target
            .identity
            .is_some_and(|identity| identity.matches_submission(submission))
        {
            return Err(imgui_error(
                "Scene submission did not match its committed target",
            ));
        }
        target.submitted = Some(submission);
        self.prepared = Some(PreparedScene {
            content,
            accepted,
            submission,
            texture,
        });
        Ok(())
    }

    pub(super) fn is_current(
        &mut self,
        target: &ImguiTarget,
        frame: &EditorFrameModel,
        requested_extent: Option<Extent2d>,
    ) -> bool {
        let Some(scene) = frame.workspace().active_scene.as_ref() else {
            return false;
        };
        if let Some(prepared) = self.prepared.as_ref() {
            return prepared.accepted.document() == scene.document
                && prepared.accepted.revision() == scene.revision
                && requested_extent == Some(prepared.submission.target().extent())
                && matches_target(target, prepared.submission, prepared.texture);
        }
        let Some(presented) = self.presented.as_ref() else {
            return false;
        };
        requested_extent == Some(presented.receipt.image().target().extent())
            && matches_target(target, presented.receipt.image(), presented.texture)
            && self.authority.token(
                scene.document,
                scene.revision,
                presentation_image(presented.receipt.image()),
                presented.mapping,
            ) == Some(presented.token)
    }

    pub(super) fn is_presented(
        &mut self,
        target: &ImguiTarget,
        frame: &EditorFrameModel,
        requested_extent: Option<Extent2d>,
    ) -> bool {
        self.prepared.is_none() && self.is_current(target, frame, requested_extent)
    }

    /// The caller supplies receipts only for the window that displayed this mapping.
    pub(super) fn promote(
        &mut self,
        target: &ImguiTarget,
        receipts: impl IntoIterator<Item = WgpuEditorCompositeReceipt>,
        mapping: DisplayMapping2d,
    ) -> Result<bool, EditorRunError> {
        let Some(submission) = target.submitted else {
            self.retire();
            return Ok(false);
        };
        let Some(texture) = target.texture else {
            self.retire();
            return Ok(false);
        };
        if !matches_target(target, submission, texture)
            || mapping.target() != submission.target().extent()
        {
            self.retire();
            return Ok(false);
        }
        let Some(receipt) = receipts
            .into_iter()
            .find(|receipt| receipt.image() == submission)
        else {
            self.authority.retire();
            self.presented = None;
            return Ok(false);
        };
        if let Some(prepared) = self.prepared.take() {
            if prepared.submission != receipt.image() || prepared.texture != texture {
                self.retire();
                return Err(imgui_error(
                    "Scene display did not match its accepted preview",
                ));
            }
            let token = self
                .authority
                .finish(
                    prepared.accepted,
                    presentation_image(receipt.image()),
                    mapping,
                )
                .map_err(imgui_error)?;
            self.presented = Some(PresentedScene {
                content: prepared.content,
                token,
                receipt,
                mapping,
                texture,
            });
            return Ok(true);
        }
        let Some(presented) = self.presented.as_mut() else {
            return Ok(false);
        };
        if presented.receipt.image() != receipt.image() || presented.texture != texture {
            self.retire();
            return Ok(false);
        }
        let mapping_changed = presented.mapping != mapping;
        if mapping_changed {
            presented.token = self
                .authority
                .redisplay(
                    presented.token,
                    presentation_image(receipt.image()),
                    mapping,
                )
                .map_err(imgui_error)?;
            presented.mapping = mapping;
        }
        presented.receipt = receipt;
        Ok(mapping_changed)
    }

    pub(super) fn resolve_pick(
        &mut self,
        frame: &EditorFrameModel,
        point: [f32; 2],
        mapping: DisplayMapping2d,
    ) -> Option<EditorAction> {
        let presented = self.presented.as_ref()?;
        if presented.mapping != mapping {
            return None;
        }
        let scene = frame.workspace().active_scene.as_ref()?;
        let token = self.authority.token(
            scene.document,
            scene.revision,
            presentation_image(presented.receipt.image()),
            mapping,
        )?;
        if token != presented.token {
            return None;
        }
        let document = self.authority.document(token)?;
        let entity = self.authority.pick(token, point).ok()?;
        Some(EditorAction::Navigate(
            EditorNavigationAction::SelectEntity { document, entity },
        ))
    }

    pub(super) fn retire(&mut self) {
        self.authority.retire();
        self.prepared = None;
        self.presented = None;
    }
}

fn matches_target(
    target: &ImguiTarget,
    submission: WgpuSubmittedImageReceipt,
    texture: ExternalTextureId,
) -> bool {
    target.submitted == Some(submission)
        && target.texture == Some(texture)
        && target.target == Some(submission.target())
        && target.source == Some(submission.source_generation())
        && target
            .identity
            .is_some_and(|identity| identity.matches_submission(submission))
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
