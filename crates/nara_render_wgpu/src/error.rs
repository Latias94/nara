use nara_window::{WindowId, backend::WindowTargetError};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WgpuRenderError {
    #[error("wgpu backend is not ready")]
    BackendNotReady,
    #[error("wgpu backend is unavailable and must be reconstructed")]
    BackendUnavailable,
    #[error("wgpu adapter unavailable: {message}")]
    AdapterUnavailable { message: String },
    #[error("wgpu device unavailable: {message}")]
    DeviceUnavailable { message: String },
    #[error("wgpu device was lost")]
    DeviceLost,
    #[error("wgpu device epoch was exhausted")]
    DeviceEpochExhausted,
    #[error("wgpu telemetry completion wait timed out")]
    TelemetryCompletionWaitTimeout,
    #[error("wgpu telemetry completion wait failed")]
    TelemetryCompletionWaitFailed,
    #[error("render frame generation {actual} does not match active generation {expected}")]
    StaleFrameGeneration { expected: u64, actual: u64 },
    #[error("render frame {frame_index} was already consumed")]
    FrameAlreadyConsumed { frame_index: u64 },
    #[error("render resources changed after preparation; retry on the next frame")]
    ResourceChangedAfterPrepare,
    #[error("wgpu surface creation failed for window {window_id:?}: {message}")]
    SurfaceCreation {
        window_id: WindowId,
        message: String,
    },
    #[error("wgpu surface is missing for window {window_id:?}")]
    SurfaceMissing { window_id: WindowId },
    #[error("wgpu surface unsupported for window {window_id:?}")]
    SurfaceUnsupported { window_id: WindowId },
    #[error("wgpu surface for window {window_id:?} has no configuration")]
    SurfaceUnconfigured { window_id: WindowId },
    #[error("wgpu surface validation error for window {window_id:?}")]
    SurfaceValidation { window_id: WindowId },
    #[error("wgpu window target lifecycle error: {0}")]
    WindowTargetLifecycle(#[from] WindowTargetError),
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[error("wgpu quad pipeline is missing for format {format} and alpha mode {alpha_mode}")]
    QuadPipelineMissing { format: String, alpha_mode: String },
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[error("wgpu quad texture error: {message}")]
    QuadTexture { message: String },
}

impl WgpuRenderError {
    #[must_use]
    pub(crate) const fn is_packet_admission_rejection(&self) -> bool {
        matches!(
            self,
            Self::StaleFrameGeneration { .. } | Self::FrameAlreadyConsumed { .. }
        )
    }

    #[must_use]
    pub(crate) const fn is_transient_resource_rejection(&self) -> bool {
        matches!(self, Self::ResourceChangedAfterPrepare)
    }
}
