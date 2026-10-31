//! Exact pixel ownership, independently of producer configuration and interaction permissions.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use crate::{frame::WgpuFramePayload, presented::WgpuSubmittedImageReceipt};
use nara_render::RenderFramePacket;

static NEXT_DISPLAY_OCCURRENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub(crate) struct OutputBudget {
    limit: AtomicU64,
    retained: AtomicU64,
}

impl Default for OutputBudget {
    fn default() -> Self {
        Self::new(512 * 1024 * 1024)
    }
}

impl OutputBudget {
    pub(crate) fn new(limit: u64) -> Self {
        Self {
            limit: AtomicU64::new(limit),
            retained: AtomicU64::new(0),
        }
    }
    pub(crate) fn set_limit(&self, limit: u64) {
        self.limit.store(limit, Ordering::Release);
    }
    pub(crate) fn retained(&self) -> u64 {
        self.retained.load(Ordering::Acquire)
    }
    pub(crate) fn reserve(self: &Arc<Self>, bytes: u64) -> Option<OutputReservation> {
        self.retained
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes)
                    .filter(|total| *total <= self.limit.load(Ordering::Acquire))
            })
            .ok()?;
        Some(OutputReservation {
            budget: Arc::clone(self),
            bytes,
        })
    }
}

/// Logical native allocation accounting follows the last lease or actual submitted use.
#[derive(Debug)]
pub(crate) struct OutputReservation {
    budget: Arc<OutputBudget>,
    bytes: u64,
}

impl Drop for OutputReservation {
    fn drop(&mut self) {
        self.budget.retained.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

/// The immutable CPU drawing inputs needed to realize an image on another device generation.
///
/// This owner contains captured geometry and content snapshots, never a live asset resolver or
/// Runtime UI completion permit. Retain it explicitly if device reconstruction is required.
#[derive(Debug)]
pub struct WgpuImageInput {
    pub(crate) packet: RenderFramePacket,
    pub(crate) payload: WgpuFramePayload,
}

impl WgpuImageInput {
    #[must_use]
    pub fn new(packet: RenderFramePacket) -> Self {
        Self {
            packet,
            payload: WgpuFramePayload::default(),
        }
    }

    #[must_use]
    pub const fn packet(&self) -> &RenderFramePacket {
        &self.packet
    }

    #[cfg(feature = "sprite-submitter")]
    #[doc(hidden)]
    #[must_use]
    pub fn with_sprites(mut self, sprites: nara_sprite_render::__private::SpriteBatches) -> Self {
        self.payload.sprite_batches = Some(sprites);
        self
    }

    #[cfg(feature = "ui-submitter")]
    #[must_use]
    pub fn with_ui(mut self, packets: Vec<Arc<nara_ui_render::UiPaintPacket>>) -> Self {
        self.payload.ui_packets = packets;
        self
    }
}

#[derive(Debug)]
pub(crate) struct WgpuImageStorage {
    pub(crate) epoch_lost: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) receipt: WgpuSubmittedImageReceipt,
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) _reservation: OutputReservation,
}

/// One exact, immutable physical image version published by a WGPU Host.
///
/// Cloning retains these pixels, including through producer resize or retirement. A successor
/// render never writes this storage. A retained image grants neither Runtime input authority nor
/// device-loss recovery; keep [`WgpuImageInput`] separately for reconstruction.
#[derive(Clone, Debug)]
pub struct WgpuImageOutput(pub(crate) Arc<WgpuImageStorage>);

impl WgpuImageOutput {
    /// Reports whether this retained allocation still belongs to its original live device epoch.
    #[must_use]
    pub fn is_current(&self) -> bool {
        !self.0.epoch_lost.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn receipt(&self) -> WgpuSubmittedImageReceipt {
        self.0.receipt
    }

    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.0.texture.format()
    }

    #[must_use]
    pub fn extent(&self) -> nara_render::Extent2d {
        self.receipt().target().extent()
    }
}

impl PartialEq for WgpuImageOutput {
    fn eq(&self, other: &Self) -> bool {
        self.receipt() == other.receipt()
    }
}

impl Eq for WgpuImageOutput {}

/// Nominal identity of one exact output's display placement, issued by its owning Host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WgpuDisplayOccurrence(u64);

impl WgpuDisplayOccurrence {
    /// Process-local fact for renderer-neutral display correlation; not persistent identity.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A retained output selected for one display occurrence.
///
/// Repaint the same occurrence by cloning this owner. Request another occurrence from the Host
/// for a mirror, even when both placements sample the same pixels in the same window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WgpuImageDisplay {
    pub(crate) output: WgpuImageOutput,
    pub(crate) occurrence: WgpuDisplayOccurrence,
}

impl WgpuImageDisplay {
    pub(crate) fn issue(output: WgpuImageOutput) -> Self {
        let serial = NEXT_DISPLAY_OCCURRENCE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .unwrap_or_else(|_| std::process::abort());
        Self {
            output,
            occurrence: WgpuDisplayOccurrence(serial),
        }
    }

    #[must_use]
    pub const fn occurrence(&self) -> WgpuDisplayOccurrence {
        self.occurrence
    }

    #[must_use]
    pub const fn output(&self) -> &WgpuImageOutput {
        &self.output
    }

    #[must_use]
    pub fn receipt(&self) -> WgpuSubmittedImageReceipt {
        self.output.receipt()
    }
}
