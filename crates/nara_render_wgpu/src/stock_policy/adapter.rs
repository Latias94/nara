use std::sync::Arc;

use nara_render::RenderFramePacket;

use crate::{
    WgpuDeviceRequirements, WgpuPolicyDefinition, WgpuPolicyInstallation, WgpuPolicyInstallationId,
    WgpuRenderInput, WgpuRenderPolicy, WgpuRenderProgramError, WgpuStockCapture, WgpuStockPolicy,
};

/// One original typed policy installation and its conversion of complete stock CPU inputs.
///
/// Concrete Scene/Canvas owners can select different policies without knowing their capture type.
/// Erasure occurs once for the complete immutable capture. Drawing and original UI completion
/// remain separate: this adapter never receives a sidecar or submission/presentation authority.
/// Dropping it closes new capture admission; accepted inputs retain their original definitions.
pub struct WgpuStockCaptureAdapter {
    installation: WgpuPolicyInstallationId,
    requirements: WgpuDeviceRequirements,
    capture: Box<
        dyn Fn(
                RenderFramePacket,
                WgpuStockCapture,
            ) -> Result<WgpuRenderInput, WgpuRenderProgramError>
            + Send
            + Sync,
    >,
}

impl std::fmt::Debug for WgpuStockCaptureAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuStockCaptureAdapter")
            .field("installation", &self.installation)
            .field("requirements", &self.requirements)
            .finish_non_exhaustive()
    }
}

impl WgpuStockCaptureAdapter {
    #[must_use]
    pub fn new<P, F>(installation: WgpuPolicyInstallation<P>, capture: F) -> Self
    where
        P: WgpuRenderPolicy,
        F: Fn(WgpuStockCapture) -> Result<Arc<P::Capture>, WgpuRenderProgramError>
            + Send
            + Sync
            + 'static,
    {
        let requirements = installation.definition().device_requirements().clone();
        let id = installation.id();
        Self {
            installation: id,
            requirements,
            capture: Box::new(move |packet, stock| {
                if !installation.is_open() {
                    return Err(WgpuRenderProgramError::PolicyClosed);
                }
                installation.capture(packet, capture(stock)?)
            }),
        }
    }

    /// Creates a fresh default policy owner; it is not a clone of another admission route.
    pub fn stock() -> Result<Self, WgpuRenderProgramError> {
        Ok(Self::new(
            WgpuPolicyDefinition::new(WgpuStockPolicy::new()).install()?,
            |stock| Ok(Arc::new(stock)),
        ))
    }

    #[must_use]
    pub const fn device_requirements(&self) -> &WgpuDeviceRequirements {
        &self.requirements
    }

    pub fn capture(
        &self,
        packet: RenderFramePacket,
        stock: WgpuStockCapture,
    ) -> Result<WgpuRenderInput, WgpuRenderProgramError> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (self.capture)(packet, stock)
        }))
        .unwrap_or(Err(WgpuRenderProgramError::PolicyPanicked))
    }
}
