use nara_app::RuntimeGeneration;
use nara_window::WindowId;

/// Exact process-local facts for one WGPU surface submission and present call.
///
/// The receipt proves that the backend submitted commands and invoked present for the surface
/// lease identified here. It does not claim GPU completion, display scan-out, or user visibility.
/// Construction is crate-private so product verification cannot forge backend acceptance.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuPresentedFrameReceipt {
    runtime_generation: RuntimeGeneration,
    window_id: WindowId,
    surface_generation: u64,
    backend_instance_id: u64,
    device_epoch: u64,
    app_frame_index: u64,
    render_transaction_frame_index: u64,
}

impl WgpuPresentedFrameReceipt {
    pub(crate) const fn new(
        runtime_generation: RuntimeGeneration,
        window_id: WindowId,
        surface_generation: u64,
        backend_instance_id: u64,
        device_epoch: u64,
        app_frame_index: u64,
        render_transaction_frame_index: u64,
    ) -> Self {
        Self {
            runtime_generation,
            window_id,
            surface_generation,
            backend_instance_id,
            device_epoch,
            app_frame_index,
            render_transaction_frame_index,
        }
    }

    #[must_use]
    pub const fn runtime_generation(self) -> RuntimeGeneration {
        self.runtime_generation
    }

    #[must_use]
    pub const fn window_id(self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn surface_generation(self) -> u64 {
        self.surface_generation
    }

    #[must_use]
    pub const fn backend_instance_id(self) -> u64 {
        self.backend_instance_id
    }

    #[must_use]
    pub const fn device_epoch(self) -> u64 {
        self.device_epoch
    }

    #[must_use]
    pub const fn app_frame_index(self) -> u64 {
        self.app_frame_index
    }

    #[must_use]
    pub const fn render_transaction_frame_index(self) -> u64 {
        self.render_transaction_frame_index
    }
}
