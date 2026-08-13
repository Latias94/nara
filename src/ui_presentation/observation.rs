use std::sync::Arc;

use nara_ecs::{Resource, World};
use nara_ui::{UiPresentationReceipt, advanced::UiFrame};
use nara_ui_render::__private::UiAcceptedBackendPresentation;
use nara_ui_render::{UiBackendSubmissionIdentity, UiPaintPacket, UiPresentationBackendGeneration};

/// Immutable facts copied from the concrete WGPU transaction that submitted one UI packet.
#[cfg(feature = "render-wgpu")]
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentedUiWgpuFacts(nara_render_wgpu::__private::WgpuSubmittedUiPacketFacts);

#[cfg(feature = "render-wgpu")]
impl PresentedUiWgpuFacts {
    #[must_use]
    pub const fn app_frame_index(self) -> u64 {
        self.0.app_frame_index()
    }

    #[must_use]
    pub const fn render_transaction_frame_index(self) -> u64 {
        self.0.render_transaction_frame_index()
    }

    #[must_use]
    pub const fn primitive_count(self) -> u64 {
        self.0.primitive_count()
    }

    #[must_use]
    pub const fn image_count(self) -> u64 {
        self.0.image_count()
    }

    #[must_use]
    pub const fn glyph_primitive_count(self) -> u64 {
        self.0.glyph_primitive_count()
    }

    #[must_use]
    pub const fn glyph_page_count(self) -> u64 {
        self.0.glyph_page_count()
    }

    #[must_use]
    pub const fn submitted_quad_count(self) -> u64 {
        self.0.submitted_quad_count()
    }

    #[must_use]
    pub const fn submitted_glyph_quad_count(self) -> u64 {
        self.0.submitted_glyph_quad_count()
    }

    #[must_use]
    pub const fn resident_texture_binding_count(self) -> u64 {
        self.0.resident_texture_binding_count()
    }
}

/// Read-only evidence for the exact packet whose backend acceptance advanced a UI receipt.
///
/// The presentation bridge is the sole publisher. Consumers receive cloned `Arc` handles to the
/// immutable packet and frame; they cannot substitute a newer compiled candidate or mint a receipt.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct PresentedUiObservation {
    receipt: UiPresentationReceipt,
    submission: UiBackendSubmissionIdentity,
    backend_generation: UiPresentationBackendGeneration,
    packet: Arc<UiPaintPacket>,
    #[cfg(feature = "render-wgpu")]
    wgpu: Option<PresentedUiWgpuFacts>,
}

impl PresentedUiObservation {
    #[must_use]
    pub const fn receipt(&self) -> UiPresentationReceipt {
        self.receipt
    }

    #[must_use]
    pub const fn submission(&self) -> UiBackendSubmissionIdentity {
        self.submission
    }

    #[must_use]
    pub const fn backend_generation(&self) -> UiPresentationBackendGeneration {
        self.backend_generation
    }

    #[must_use]
    pub fn packet(&self) -> Arc<UiPaintPacket> {
        Arc::clone(&self.packet)
    }

    #[must_use]
    pub fn frame(&self) -> Arc<UiFrame> {
        self.packet.frame()
    }

    #[cfg(feature = "render-wgpu")]
    #[must_use]
    pub const fn wgpu(&self) -> Option<PresentedUiWgpuFacts> {
        self.wgpu
    }
}

#[derive(Debug, Default, Resource)]
pub(super) struct PresentedUiObservationStore(Option<PresentedUiObservation>);

impl PresentedUiObservationStore {
    pub(super) fn publish(&mut self, observation: PresentedUiObservation) {
        self.0 = Some(observation);
    }
}

/// Returns the latest owner-produced presented observation, if one has committed.
#[doc(hidden)]
#[must_use]
pub fn latest_presented_ui_observation(world: &World) -> Option<PresentedUiObservation> {
    world
        .get_resource::<PresentedUiObservationStore>()
        .and_then(|store| store.0.clone())
}

pub(super) fn stage_presented_ui_observation(
    _world: &World,
    accepted: &UiAcceptedBackendPresentation,
) -> Result<StagedPresentedUiObservation, String> {
    #[cfg(feature = "render-wgpu")]
    let wgpu = match _world.get_resource::<nara_render_wgpu::WgpuRenderBackend>() {
        Some(backend)
            if accepted.backend_generation().backend_instance_id() == backend.instance_id()
                && accepted.backend_generation().device_epoch() == backend.device_epoch() =>
        {
            let facts = nara_render_wgpu::__private::submitted_ui_packet_facts(
                backend,
                accepted.identity(),
                accepted.backend_generation(),
            )
            .ok_or_else(|| {
                format!(
                    "WGPU submitted no facts for accepted UI attempt {}",
                    accepted.identity().admission_id().get()
                )
            })?;
            Some(PresentedUiWgpuFacts(facts))
        }
        Some(_) | None => None,
    };

    Ok(StagedPresentedUiObservation {
        submission: accepted.identity(),
        backend_generation: accepted.backend_generation(),
        packet: Arc::clone(accepted.packet()),
        #[cfg(feature = "render-wgpu")]
        wgpu,
    })
}

pub(super) struct StagedPresentedUiObservation {
    submission: UiBackendSubmissionIdentity,
    backend_generation: UiPresentationBackendGeneration,
    packet: Arc<UiPaintPacket>,
    #[cfg(feature = "render-wgpu")]
    wgpu: Option<PresentedUiWgpuFacts>,
}

impl StagedPresentedUiObservation {
    pub(super) fn commit(self, receipt: UiPresentationReceipt) -> PresentedUiObservation {
        debug_assert!(self.submission.matches_receipt(receipt));
        PresentedUiObservation {
            receipt,
            submission: self.submission,
            backend_generation: self.backend_generation,
            packet: self.packet,
            #[cfg(feature = "render-wgpu")]
            wgpu: self.wgpu,
        }
    }
}
