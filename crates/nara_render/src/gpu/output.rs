//! Exact pixel ownership, independently of producer configuration and interaction permissions.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use crate::gpu::presented::WgpuSubmittedImageReceipt;

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
        if bytes != 0 {
            self.retained
                .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                    used.checked_add(bytes)
                        .filter(|total| *total <= self.limit.load(Ordering::Acquire))
                })
                .ok()?;
        }
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

impl OutputReservation {
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl Drop for OutputReservation {
    fn drop(&mut self) {
        self.budget.retained.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

#[derive(Debug)]
pub(crate) struct WgpuImageStorage {
    pub(crate) receipt: WgpuSubmittedImageReceipt,
    pub(crate) resource: Arc<crate::gpu::program::ProgramResourceStorage>,
}

impl WgpuImageStorage {
    pub(crate) fn texture(&self) -> &wgpu::Texture {
        let crate::gpu::program::NativeResource::Texture { texture, .. } = &self.resource.native
        else {
            unreachable!("published image storage is selected only from a texture output")
        };
        texture
    }

    pub(crate) fn view(&self) -> &wgpu::TextureView {
        let crate::gpu::program::NativeResource::Texture { view, .. } = &self.resource.native
        else {
            unreachable!("published image storage is selected only from a texture output")
        };
        view
    }
}

/// One exact, immutable physical image version published by a WGPU Host.
///
/// Cloning retains these pixels, including through producer resize or retirement. A successor
/// render never writes this storage. A retained image grants neither Runtime input authority nor
/// device-loss recovery; keep [`crate::gpu::WgpuRenderInput`] separately for reconstruction.
#[derive(Clone, Debug)]
pub struct WgpuImageOutput(pub(crate) Arc<WgpuImageStorage>);

impl WgpuImageOutput {
    /// Reports whether this retained allocation still belongs to its original live device epoch.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.0.resource.is_current()
    }

    #[must_use]
    pub fn receipt(&self) -> WgpuSubmittedImageReceipt {
        self.0.receipt
    }

    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.0.texture().format()
    }

    /// Observes the exact retained output's admitted shape, transfer and alpha interpretation.
    ///
    /// Native texture format alone does not describe color or alpha meaning. The returned
    /// descriptor is a detached observation; changing it cannot change this output or relabel
    /// its nominal semantic. Typed program imports still require the original marker through
    /// [`Self::texture`], and native sampling still requires this output's live allocation.
    #[must_use]
    pub fn descriptor(&self) -> crate::gpu::WgpuProgramTextureDescriptor {
        self.0.resource.texture_descriptor()
    }

    #[must_use]
    pub fn extent(&self) -> crate::Extent2d {
        self.receipt().target().extent()
    }

    /// Borrows the submitted texture for sampling by another renderer on the same device.
    ///
    /// The sampler must submit after this output on the same queue and retain a clone of
    /// this output until its last GPU use completes. Writing to or destroying the texture
    /// invalidates the output contract. Returns `None` after its device epoch retires.
    #[must_use]
    pub fn native_texture(&self) -> Option<&wgpu::Texture> {
        self.is_current().then(|| self.0.texture())
    }

    /// Retains this exact published texture with its original semantic interpretation.
    ///
    /// The marker must match the selected program output; equal pixels or a same-shaped image
    /// cannot substitute another port meaning. This lease imports through the ordinary program
    /// builder, retaining only this resource rather than the original program or CPU capture.
    pub fn texture<S: 'static>(
        &self,
    ) -> Result<crate::gpu::WgpuProgramTextureOutput<S>, crate::gpu::WgpuRenderProgramError> {
        crate::gpu::program::require_descriptor::<S>(&self.0.resource.descriptor, true)?;
        Ok(crate::gpu::WgpuProgramTextureOutput::new(Arc::clone(
            &self.0.resource,
        )))
    }
}

impl PartialEq for WgpuImageOutput {
    fn eq(&self, other: &Self) -> bool {
        self.receipt() == other.receipt()
    }
}

impl Eq for WgpuImageOutput {}
