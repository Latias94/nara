use nara_render::RenderFrameSourceGeneration;
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
    #[error("wgpu telemetry completion poll failed")]
    TelemetryCompletionPollFailed,
    #[error("render frame generation {actual:?} does not match active generation {expected:?}")]
    StaleFrameGeneration {
        expected: RenderFrameSourceGeneration,
        actual: RenderFrameSourceGeneration,
    },
    #[error("render frame {frame_index} was already consumed")]
    FrameAlreadyConsumed { frame_index: u64 },
    #[error("render packet content was rejected: {message}")]
    ContentRejected { message: String },
    #[cfg(feature = "ui-submitter")]
    #[error("resource-complete UI packet violated the WGPU adapter contract: {message}")]
    UiPacketInvariant { message: String },
    #[cfg(feature = "ui-submitter")]
    #[error("prepared UI backend submission was rejected before submit: {message}")]
    UiSubmissionRejected { message: String },
    #[error("render packet submission was deferred: {message}")]
    UploadDeferred { message: String },
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
    pub(crate) const fn is_attempt_fault(&self) -> bool {
        if self.is_packet_admission_rejection() {
            return true;
        }
        #[cfg(feature = "ui-submitter")]
        if matches!(
            self,
            Self::UiPacketInvariant { .. } | Self::UiSubmissionRejected { .. }
        ) {
            return true;
        }
        false
    }

    #[must_use]
    pub(crate) const fn target_window(&self) -> Option<WindowId> {
        match self {
            Self::SurfaceCreation { window_id, .. }
            | Self::SurfaceMissing { window_id }
            | Self::SurfaceUnsupported { window_id }
            | Self::SurfaceUnconfigured { window_id }
            | Self::SurfaceValidation { window_id } => Some(*window_id),
            Self::WindowTargetLifecycle(error) => Some(match error {
                WindowTargetError::UnknownWindow { window_id }
                | WindowTargetError::DuplicateWindow { window_id }
                | WindowTargetError::SurfaceActivationAfterRetirement { window_id }
                | WindowTargetError::SurfaceAlreadyActive { window_id }
                | WindowTargetError::SurfaceOwnerStillActive { window_id }
                | WindowTargetError::StaleSurfaceBinding { window_id }
                | WindowTargetError::SurfaceGenerationExhausted { window_id }
                | WindowTargetError::ProviderReleaseBeforeSurfaceRetirement { window_id } => {
                    *window_id
                }
            }),
            _ => None,
        }
    }

    #[must_use]
    #[cfg(feature = "ui-submitter")]
    pub(crate) const fn is_target_fault(&self) -> bool {
        self.target_window().is_some()
    }

    #[must_use]
    pub(crate) const fn is_deferred(&self) -> bool {
        matches!(self, Self::UploadDeferred { .. })
    }

    #[must_use]
    pub(crate) const fn is_content_rejection(&self) -> bool {
        matches!(self, Self::ContentRejected { .. })
    }
}
