use nara::{
    project_host::{EditorUiAuthoringFrame, EditorUiPreviewCandidate},
    render::{RenderFrameTarget, RenderWindowPacket},
    render_wgpu::__private::{
        WgpuDisplayOccurrence, WgpuEditorCompositeReceipt, WgpuImageDisplay, WgpuRenderHost,
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
    display: WgpuImageDisplay,
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
        gpu: &mut WgpuRenderHost,
        renderer: &mut ImguiRenderer,
        window: RenderWindowPacket,
        candidate: EditorUiPreviewCandidate,
    ) -> Result<(), EditorRunError> {
        let (document, revision, gesture, generation, packet, paint, canvas) =
            candidate.into_parts();
        let RenderFrameTarget::Image(image_target) = packet.target() else {
            return Err(imgui_error("the UI Canvas candidate has no image target"));
        };
        target.ensure(gpu, window, packet.source_generation(), image_target)?;
        let view = target
            .view
            .as_ref()
            .ok_or_else(|| imgui_error("the UI Canvas has no committed render view"))?;
        let output = gpu
            .submit_authoring_ui_image(view, window, packet, paint)
            .map_err(platform_gpu_error)?;
        if !target
            .identity
            .is_some_and(|identity| identity.matches_submission(output.receipt()))
        {
            return Err(imgui_error(
                "the submitted UI Canvas does not match its committed target",
            ));
        }
        let display = gpu.display_image(output).map_err(platform_gpu_error)?;
        target.select_display(renderer, display.clone())?;
        if let Some(presented) = self.presented.as_mut() {
            presented.confirmed = false;
        }
        self.pending = Some(PreparedCanvas {
            identity: CanvasIdentity {
                document,
                revision,
                gesture,
                generation,
            },
            display,
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
            if !target_matches(target, &presented.prepared.display) {
                presented.confirmed = false;
                return Ok(false);
            }
            let receipt = receipts
                .into_iter()
                .find(|receipt| matches_receipt(&presented.prepared.display, *receipt));
            let was_confirmed = presented.confirmed;
            presented.confirmed = receipt.is_some();
            if let Some(receipt) = receipt {
                presented.receipt = receipt;
            }
            return Ok(!was_confirmed && presented.confirmed);
        };
        if !target_matches(target, &pending.display) {
            if let Some(presented) = self.presented.as_mut() {
                let receipt = receipts.into_iter().find(|receipt| {
                    target_matches(target, &presented.prepared.display)
                        && matches_receipt(&presented.prepared.display, *receipt)
                });
                presented.confirmed = receipt.is_some();
                if let Some(receipt) = receipt {
                    presented.receipt = receipt;
                }
            }
            return Ok(false);
        }
        let Some(receipt) = receipts
            .into_iter()
            .find(|receipt| matches_receipt(&pending.display, *receipt))
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
                && matches_receipt(&presented.prepared.display, presented.receipt)
                && target_matches(target, &presented.prepared.display)
        })
    }

    pub(super) fn has_current_submission(
        &self,
        target: &ImguiTarget,
        frame: &EditorUiAuthoringFrame,
    ) -> bool {
        self.presented.as_ref().is_some_and(|presented| {
            presented.prepared.identity.matches(frame)
                && target_matches(target, &presented.prepared.display)
        }) || self.pending.as_ref().is_some_and(|pending| {
            pending.identity.matches(frame) && target_matches(target, &pending.display)
        })
    }

    pub(super) fn matches_occurrence(&self, occurrence: Option<WgpuDisplayOccurrence>) -> bool {
        self.presented.as_ref().is_some_and(|presented| {
            presented.confirmed && Some(presented.prepared.display.occurrence()) == occurrence
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

fn target_matches(target: &ImguiTarget, display: &WgpuImageDisplay) -> bool {
    target.matches_display(display)
}

fn matches_receipt(display: &WgpuImageDisplay, receipt: WgpuEditorCompositeReceipt) -> bool {
    display.receipt() == receipt.image() && display.occurrence() == receipt.occurrence()
}
