use nara::{
    project_host::{EditorUiAuthoringFrame, EditorUiPreviewCandidate},
    render::{RenderFrameTarget, RenderWindowPacket},
    render_wgpu::__private::{
        WgpuEditorCompositeReceipt, WgpuEditorExecution, WgpuSubmittedImageReceipt,
    },
    scene::SceneAuthoringRevision,
    tooling::{EditorAction, EditorDocumentId, EditorGestureStamp, EditorNavigationAction},
    ui::__private::UiAuthoringCanvasReady,
};

use super::{EditorRunError, ImguiRenderer, ImguiTarget, imgui_error, platform_gpu_error};

#[derive(Clone, Copy, PartialEq, Eq)]
struct CanvasIdentity {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    gesture: Option<EditorGestureStamp>,
    generation: u64,
}

impl CanvasIdentity {
    fn matches(self, frame: &EditorUiAuthoringFrame) -> bool {
        self.document == frame.document()
            && self.revision == frame.revision()
            && self.gesture == frame.gesture().map(|gesture| gesture.stamp())
            && Some(self.generation) == frame.canvas_generation()
    }
}

struct PreparedCanvas {
    identity: CanvasIdentity,
    image: WgpuSubmittedImageReceipt,
    canvas: UiAuthoringCanvasReady,
}

struct PresentedCanvas {
    prepared: PreparedCanvas,
    receipt: WgpuEditorCompositeReceipt,
    confirmed: bool,
}

/// Joins a submitted UI Canvas candidate to the exact image sampled by a native surface.
/// Picking retains the displayed candidate's compiled geometry rather than the latest model.
#[derive(Default)]
pub(super) struct ImguiUiCanvasPresenter {
    pending: Option<PreparedCanvas>,
    presented: Option<PresentedCanvas>,
}

impl ImguiUiCanvasPresenter {
    pub(super) fn prepare(
        &mut self,
        target: &mut ImguiTarget,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut ImguiRenderer,
        window: RenderWindowPacket,
        candidate: EditorUiPreviewCandidate,
        next_view: &mut u64,
    ) -> Result<(), EditorRunError> {
        // A new submission may overwrite the same allocation. Its predecessor's receipt no
        // longer certifies the content, even when target and document identities are unchanged.
        self.retire();
        target.submitted = None;
        let (document, revision, gesture, generation, packet, paint, canvas) =
            candidate.into_parts();
        let RenderFrameTarget::Image(image_target) = packet.target() else {
            return Err(imgui_error("the UI Canvas candidate has no image target"));
        };
        target.ensure(
            gpu,
            renderer,
            window,
            packet.source_generation(),
            image_target,
            next_view,
        )?;
        let view = target
            .view
            .ok_or_else(|| imgui_error("the UI Canvas has no committed render view"))?;
        let image = gpu
            .submit_authoring_ui_image(view, window, packet, paint)
            .map_err(platform_gpu_error)?;
        if !target
            .identity
            .is_some_and(|identity| identity.matches_submission(image))
        {
            return Err(imgui_error(
                "the submitted UI Canvas does not match its committed target",
            ));
        }
        target.submitted = Some(image);
        self.pending = Some(PreparedCanvas {
            identity: CanvasIdentity {
                document,
                revision,
                gesture,
                generation,
            },
            image,
            canvas,
        });
        Ok(())
    }

    /// Returns true only when a pending candidate becomes the displayed Canvas.
    /// An unsampled or deferred surface keeps the candidate pending without certifying it.
    pub(super) fn promote(
        &mut self,
        target: &ImguiTarget,
        receipts: impl IntoIterator<Item = WgpuEditorCompositeReceipt>,
    ) -> Result<bool, EditorRunError> {
        let Some(pending) = self.pending.as_ref() else {
            let Some(presented) = self.presented.as_mut() else {
                return Ok(false);
            };
            if !target_matches(target, presented.prepared.image) {
                self.retire();
                return Ok(false);
            }
            let receipt = receipts
                .into_iter()
                .find(|receipt| receipt.image() == presented.prepared.image);
            let was_confirmed = presented.confirmed;
            presented.confirmed = receipt.is_some();
            if let Some(receipt) = receipt {
                presented.receipt = receipt;
            }
            return Ok(!was_confirmed && presented.confirmed);
        };
        if !target_matches(target, pending.image) {
            self.retire();
            return Err(imgui_error(
                "the UI Canvas target changed before presentation confirmation",
            ));
        }
        let Some(receipt) = receipts
            .into_iter()
            .find(|receipt| receipt.image() == pending.image)
        else {
            return Ok(false);
        };
        let prepared = self
            .pending
            .take()
            .expect("the matched Canvas remains pending");
        self.presented = Some(PresentedCanvas {
            prepared,
            receipt,
            confirmed: true,
        });
        Ok(true)
    }

    pub(super) fn is_current(&self, target: &ImguiTarget, frame: &EditorUiAuthoringFrame) -> bool {
        self.presented.as_ref().is_some_and(|presented| {
            presented.confirmed
                && presented.prepared.identity.matches(frame)
                && presented.receipt.image() == presented.prepared.image
                && target_matches(target, presented.prepared.image)
        })
    }

    pub(super) fn has_current_submission(
        &self,
        target: &ImguiTarget,
        frame: &EditorUiAuthoringFrame,
    ) -> bool {
        self.presented.as_ref().is_some_and(|presented| {
            presented.prepared.identity.matches(frame)
                && target_matches(target, presented.prepared.image)
        }) || self.pending.as_ref().is_some_and(|pending| {
            pending.identity.matches(frame) && target_matches(target, pending.image)
        })
    }

    /// Resolves a normalized point using only the exact, still-current displayed candidate.
    /// A point outside authored content is a valid clear-selection request; invalid UVs are not.
    pub(super) fn resolve_pick(
        &self,
        target: &ImguiTarget,
        frame: &EditorUiAuthoringFrame,
        uv: [f32; 2],
    ) -> Option<EditorAction> {
        if !self.is_current(target, frame)
            || !uv
                .into_iter()
                .all(|value| value.is_finite() && (0.0..1.0).contains(&value))
        {
            return None;
        }
        let displayed = self.presented.as_ref()?;
        Some(EditorAction::Navigate(
            EditorNavigationAction::SelectEntity {
                document: displayed.prepared.identity.document,
                entity: displayed.prepared.canvas.source_at_surface_uv(uv).cloned(),
            },
        ))
    }

    pub(super) fn retire(&mut self) {
        self.pending = None;
        self.presented = None;
    }
}

fn target_matches(target: &ImguiTarget, image: WgpuSubmittedImageReceipt) -> bool {
    target.submitted == Some(image)
        && target.texture.is_some()
        && target
            .identity
            .is_some_and(|identity| identity.matches_submission(image))
}
