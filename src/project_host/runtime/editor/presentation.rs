use std::sync::atomic::{AtomicU64, Ordering};

use nara_render::{
    DisplayMapping2d, RenderFramePacket, RenderFrameSourceGeneration, RenderFrameTarget,
    RenderImagePacket,
};
use nara_scene::{SceneAuthoringRevision, SceneEntityId};
use nara_sprite_render::__private::SpriteBatches;
use nara_tooling::EditorDocumentId;

use super::preview::{EditorPreviewCandidate, EditorPreviewHit};

static NEXT_PRESENTATION_ID: AtomicU64 = AtomicU64::new(1);

/// Opaque authority for geometry from one completed display operation.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorPresentedToken(u64);

/// Backend-neutral image facts. A native adapter obtains these from its actual submitted image;
/// a CPU adapter obtains them from its own target and display command lifecycle.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditorPresentationImage {
    pub target: RenderImagePacket,
    pub target_generation: u64,
    pub source_generation: RenderFrameSourceGeneration,
    pub source_handoff_generation: u64,
    pub backend_instance_id: u64,
    pub device_epoch: u64,
    pub frame_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[doc(hidden)]
pub enum EditorPresentationError {
    #[error("the preview presentation identity space is exhausted")]
    IdentityExhausted,
    #[error("the displayed image does not match the accepted preview")]
    ImageMismatch,
    #[error("the displayed geometry token is no longer current")]
    StaleToken,
}

/// Linear preview content retained while an adapter submits and displays its render packet.
#[doc(hidden)]
pub struct EditorAcceptedPreview {
    owner: u64,
    token: EditorPresentedToken,
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    geometry_revision: u64,
    target: RenderImagePacket,
    source: RenderFrameSourceGeneration,
    frame_index: u64,
    hits: Vec<EditorPreviewHit>,
}

impl EditorAcceptedPreview {
    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        self.document
    }
    #[must_use]
    pub const fn revision(&self) -> SceneAuthoringRevision {
        self.revision
    }
}

struct PresentedPreview {
    content: EditorAcceptedPreview,
    image: EditorPresentationImage,
    mapping: DisplayMapping2d,
    valid: bool,
}

/// The single content/image/display join for Scene preview geometry.
/// Toolkit texture registration and native submission/present checks remain in the adapter.
#[doc(hidden)]
pub struct EditorPreviewPresentation {
    owner: Option<u64>,
    presented: Option<PresentedPreview>,
}

impl Default for EditorPreviewPresentation {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorPreviewPresentation {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            owner: None,
            presented: None,
        }
    }

    pub fn accept(
        &mut self,
        candidate: EditorPreviewCandidate,
    ) -> Result<(EditorAcceptedPreview, RenderFramePacket, SpriteBatches), EditorPresentationError>
    {
        let id = NEXT_PRESENTATION_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| EditorPresentationError::IdentityExhausted)?;
        let owner = *self.owner.get_or_insert(id);
        let (document, revision, geometry_revision, packet, batches, hits) = candidate.into_parts();
        let RenderFrameTarget::Image(target) = packet.target() else {
            return Err(EditorPresentationError::ImageMismatch);
        };
        let accepted = EditorAcceptedPreview {
            owner,
            token: EditorPresentedToken(id),
            document,
            revision,
            geometry_revision,
            target,
            source: packet.source_generation(),
            frame_index: packet.frame_index(),
            hits,
        };
        Ok((accepted, packet, batches))
    }

    /// Call only after the adapter finishes displaying this exact submitted image.
    pub fn finish(
        &mut self,
        accepted: EditorAcceptedPreview,
        image: EditorPresentationImage,
        mapping: DisplayMapping2d,
    ) -> Result<EditorPresentedToken, EditorPresentationError> {
        if self.owner != Some(accepted.owner)
            || image.target != accepted.target
            || image.source_generation != accepted.source
            || image.frame_index != accepted.frame_index
            || mapping.target() != image.target.extent()
            || image.target_generation == 0
            || image.backend_instance_id == 0
            || image.device_epoch == 0
            || image.source_handoff_generation == 0
            || self
                .presented
                .as_ref()
                .is_some_and(|current| current.content.token.0 >= accepted.token.0)
        {
            return Err(EditorPresentationError::ImageMismatch);
        }
        let token = accepted.token;
        self.presented = Some(PresentedPreview {
            content: accepted,
            image,
            mapping,
            valid: true,
        });
        Ok(token)
    }

    /// Resolve authority against current document, geometry, target namespace and display mapping.
    #[must_use]
    pub fn token(
        &mut self,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        image: EditorPresentationImage,
        mapping: DisplayMapping2d,
    ) -> Option<EditorPresentedToken> {
        let current = self.presented.as_mut()?;
        current.valid &= current.content.document == document
            && current.content.revision == revision
            && current.image == image
            && current.mapping == mapping;
        current.valid.then_some(current.content.token)
    }

    #[must_use]
    pub fn document(&self, token: EditorPresentedToken) -> Option<EditorDocumentId> {
        self.presented
            .as_ref()
            .filter(|current| current.content.token == token)
            .map(|current| current.content.document)
    }

    #[must_use]
    pub fn geometry_revision(&self, token: EditorPresentedToken) -> Option<u64> {
        self.presented
            .as_ref()
            .filter(|current| current.content.token == token)
            .map(|current| current.content.geometry_revision)
    }

    pub fn pick(
        &self,
        token: EditorPresentedToken,
        point: [f32; 2],
    ) -> Result<Option<SceneEntityId>, EditorPresentationError> {
        let current = self
            .presented
            .as_ref()
            .filter(|current| current.valid && current.content.token == token)
            .ok_or(EditorPresentationError::StaleToken)?;
        let uv = current
            .mapping
            .panel_to_uv(point)
            .ok_or(EditorPresentationError::StaleToken)?;
        let clip = [uv[0] * 2.0 - 1.0, 1.0 - uv[1] * 2.0];
        Ok(current
            .content
            .hits
            .iter()
            .rev()
            .find(|hit| hit.contains_clip_point(clip))
            .map(|hit| hit.entity().clone()))
    }

    pub fn retire(&mut self) {
        self.presented = None;
    }
}
