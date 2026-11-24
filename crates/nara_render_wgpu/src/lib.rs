//! Wgpu backend for backend-neutral nara render data.

mod allocation;
mod backend;
#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
mod budget;
mod candidate;
mod canvas;
mod device;
mod error;
mod frame;
mod graph;
mod host;
#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
mod material_textures;
mod observation;
mod output;
mod plugin;
mod policy;
mod preparation;
mod presentation;
mod presented;
mod program;
#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
mod quad;
#[cfg(all(test, any(feature = "stock-sprite", feature = "stock-ui")))]
mod readback_tests;
mod requirements;
mod retirement;
mod runtime;
mod runtime_timing;
#[cfg(feature = "stock-sprite")]
mod sprite;
#[cfg(all(test, feature = "stock-sprite"))]
mod sprite_probe;
mod stock_policy;
#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
mod stock_quad;
mod submission;
mod surface;
mod telemetry;
#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
mod texture;
#[cfg(feature = "stock-ui")]
mod ui;
#[cfg(all(test, feature = "stock-sprite"))]
mod view_probe;
#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
mod view_uniform;
mod work;

use crate::backend::{WgpuRenderBackend, render_backend_state};
#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
use crate::quad::WgpuQuadBatch;
use crate::surface::SurfaceDropReason;
use nara_app::{RuntimeFault, RuntimeFaultKind};
use nara_render::{RenderBackendStatus, RenderFrameSkipReason};

pub use crate::backend::{WgpuBackendState, WgpuFrameTransactionStats};
#[cfg(feature = "stock-ui")]
pub use crate::candidate::WgpuUiCandidateSubmitFailure;
pub use crate::candidate::{WgpuCandidateSubmitFailure, WgpuPublishedFrame, WgpuRenderCandidate};
pub use crate::error::WgpuRenderError;
pub use crate::frame::WgpuPolicyFrameReader;
pub use crate::frame::{
    WgpuCapturedPolicyBatch, WgpuFrameCaptureError, WgpuFrameSet as WgpuPolicyCaptureSet,
    take_captured_policy_batch,
};
pub use crate::graph::{
    AccessRole, GraphError, GraphLimits, LimitKind, TextureAlpha, TextureInterpretation,
    TextureTransfer,
};
pub use crate::host::{
    WgpuImageTargetActivation, WgpuImageTargetIdentity, WgpuImageTargetReservation, WgpuRenderHost,
    WgpuRenderHostError, WgpuRenderProducer, WgpuRenderProducerId, WgpuWindowDeviceScope,
    WgpuWindowFrame, WgpuWindowFrameOutcome,
};
#[cfg(feature = "stock-sprite")]
pub use crate::material_textures::WgpuMaterialTextureBinding;
pub use crate::observation::WgpuRenderObservation;
pub use crate::output::{WgpuDisplayOccurrence, WgpuImageDisplay, WgpuImageOutput};
pub use crate::plugin::{
    WGPU_BACKEND_SHUTDOWN_OBLIGATION, WGPU_CAPTURE_PLUGIN_DECLARATION, WGPU_CAPTURE_PLUGIN_ID,
    WGPU_STOCK_CAPTURE_PLUGIN_DECLARATION, WGPU_STOCK_CAPTURE_PLUGIN_ID,
    WGPU_STOCK_POLICY_PLUGIN_DECLARATION, WGPU_STOCK_POLICY_PLUGIN_ID,
    WGPU_WINDOW_PRESENTATION_PLUGIN_DECLARATION, WGPU_WINDOW_PRESENTATION_PLUGIN_ID,
    WgpuCapturePlugin, WgpuPolicyFrameWriter, WgpuStockCapturePlugin, WgpuStockCaptureSet,
    WgpuStockFrame, WgpuStockFrameReader, WgpuStockPolicyPlugin, WgpuWindowPresentationPlugin,
    install_wgpu_policy,
};
#[cfg(feature = "stock-sprite")]
pub use crate::plugin::{
    WGPU_STOCK_SPRITE_PLUGIN_DECLARATION, WGPU_STOCK_SPRITE_PLUGIN_ID, WgpuStockSpritePlugin,
};
#[cfg(feature = "stock-ui")]
pub use crate::plugin::{
    WGPU_STOCK_UI_PLUGIN_DECLARATION, WGPU_STOCK_UI_PLUGIN_ID, WgpuStockUiPlugin,
};
#[cfg(feature = "stock-ui")]
pub use crate::policy::WgpuUiProgramBinding;
pub use crate::policy::{
    WgpuPolicyBindingContext, WgpuPolicyBuildContext, WgpuPolicyDefinition, WgpuPolicyFrameFacts,
    WgpuPolicyInstallation, WgpuPolicyInstallationId, WgpuPolicyProgram, WgpuRenderInput,
    WgpuRenderPolicy,
};
pub use crate::presented::{
    WgpuImageCompositeReceipt, WgpuSubmittedImageReceipt, WgpuWindowPresentedReceipt,
};
pub use crate::program::{
    WgpuFeatureAllocation, WgpuFeatureDeclaration, WgpuFeatureDefinition, WgpuFeatureEncodeContext,
    WgpuFeatureInstance, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuNativeResources, WgpuOperationResources, WgpuProgramBuffer, WgpuProgramBufferOutput,
    WgpuProgramOutput, WgpuProgramOutputs, WgpuProgramRead, WgpuProgramTexture,
    WgpuProgramTextureDescriptor, WgpuProgramTextureOutput, WgpuProgramWrite, WgpuRenderFeature,
    WgpuRenderProgram, WgpuRenderProgramBuilder, WgpuRenderProgramError,
};
pub use crate::requirements::{WgpuDeviceRequirements, WgpuDeviceRequirementsError};
pub use crate::runtime_timing::{
    WgpuNamespacePhase, WgpuNamespaceTimings, WgpuRuntimeFramePhase, WgpuRuntimeFrameTimings,
};
#[cfg(feature = "stock-ui")]
pub use crate::stock_policy::WgpuCompositeColor;
pub use crate::stock_policy::{
    WgpuSceneColor, WgpuStockBindings, WgpuStockCapture, WgpuStockCaptureAdapter, WgpuStockPolicy,
    WgpuStockProgramBuilder, WgpuStockStages, WgpuWorldColor,
};
pub use crate::submission::{WgpuSubmissionPrefix, WgpuSubmissionPrefixError};
pub use crate::telemetry::{
    MAX_ADAPTER_SUMMARY_FIELD_BYTES, MAX_BUFFERED_FRAME_COMPLETIONS, MAX_PENDING_FRAME_COMPLETIONS,
    WgpuAdapterSummary, WgpuBackendTelemetryConfig, WgpuFrameCompletionSample,
    WgpuFrameCompletionStats, WgpuGpuResourceStats, WgpuTelemetryConfigError,
};
pub use crate::work::WgpuNativeWorkStats;
#[cfg(feature = "stock-ui")]
pub use crate::{
    host::{WgpuUiImageSubmission, WgpuUiSubmission, WgpuUiSubmitFailure},
    plugin::WgpuPolicyUiCaptureFailure,
};

pub use crate::surface::{
    SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus, WgpuConfiguredPresentMode,
    choose_present_mode, clear_color_to_wgpu, map_present_mode, surface_acquire_policy,
    surface_resize_action,
};
#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
pub use crate::texture::WgpuTextureCacheStats as WgpuRenderTextureCacheStats;
#[cfg(feature = "stock-ui")]
pub use crate::ui::{
    UiCapture as WgpuUiCapture, UiFeature as WgpuUiFeature, UiReport as WgpuUiReport,
};

#[doc(hidden)]
pub mod __private {
    pub use crate::host::{WgpuRuntimeHostFacts, WgpuRuntimeMaintenance};
    pub use crate::{
        host::{
            WgpuImageTargetActivation, WgpuImageTargetIdentity, WgpuImageTargetReservation,
            WgpuRenderHost, WgpuRenderHostError, WgpuRenderProducer, WgpuRenderProducerId,
        },
        output::{WgpuDisplayOccurrence, WgpuImageDisplay, WgpuImageOutput},
        presented::{WgpuPresentedFrameReceipt, WgpuSubmittedImageReceipt},
        runtime::{
            WgpuRuntimeCapture, WgpuRuntimeCaptureFailure, WgpuRuntimeCaptureIdentity,
            WgpuRuntimeCompletion, WgpuRuntimeConnection, WgpuRuntimeExchangeError,
            WgpuRuntimeResult, WgpuRuntimeResultFailure, cancel_runtime_frame,
            complete_runtime_frame, take_runtime_frame,
        },
    };

    #[cfg(feature = "stock-ui")]
    pub use crate::backend::WgpuSubmittedUiFacts;

    #[cfg(feature = "stock-ui")]
    pub use crate::host::{WgpuUiImageSubmission, WgpuUiSubmission, WgpuUiSubmitFailure};

    #[cfg(feature = "stock-ui")]
    #[must_use]
    pub fn submitted_ui_facts(
        backend: &crate::WgpuRenderObservation,
        submission: nara_ui_render::UiBackendSubmissionIdentity,
        backend_generation: nara_ui_render::UiPresentationBackendGeneration,
    ) -> Option<WgpuSubmittedUiFacts> {
        backend.submitted_ui_facts(submission, backend_generation)
    }

    /// Returns the exact surface presentation published by the current render transaction.
    #[must_use]
    pub fn presented_frame_this_transaction(
        backend: &crate::WgpuRenderObservation,
    ) -> Option<WgpuPresentedFrameReceipt> {
        backend.presented_frame_this_transaction()
    }
}

const WGPU_RENDER_BACKEND: &str = "wgpu";

#[cfg(feature = "stock-ui")]
fn saturating_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn handle_wgpu_render_error(
    backend: &mut WgpuRenderBackend,
    error: WgpuRenderError,
    frame_index: u64,
    status: &mut RenderBackendStatus,
) -> Option<RuntimeFault> {
    if error.is_attempt_fault() {
        status.clear_skip();
        status.mark_skipped_with_message(
            frame_index,
            RenderFrameSkipReason::InvalidTopology,
            error.to_string(),
        );
        return Some(RuntimeFault::engine(
            RuntimeFaultKind::System,
            "nara.render.frame-packet",
        ));
    }

    if error.is_deferred() {
        status.clear_skip();
        status.mark_skipped_with_message(
            frame_index,
            RenderFrameSkipReason::Deferred,
            error.to_string(),
        );
        return None;
    }

    if error.is_content_rejection() {
        status.clear_skip();
        status.mark_skipped_with_message(
            frame_index,
            RenderFrameSkipReason::ContentRejected,
            error.to_string(),
        );
        return None;
    }

    if let Some(window_id) = error.target_window() {
        let message =
            match backend.retire_targets(&[window_id], SurfaceDropReason::SurfaceOrDeviceLost) {
                Ok(()) => error.to_string(),
                Err(retirement_error) => {
                    format!("{error}; target retirement failed: {retirement_error}")
                }
            };
        status.clear_skip();
        status.mark_state(WGPU_RENDER_BACKEND, render_backend_state(backend.state()));
        status.mark_skipped_with_message(
            frame_index,
            RenderFrameSkipReason::SurfaceUnavailable,
            message,
        );
        return None;
    }

    let message = backend.mark_error(&error);
    status.mark_unavailable(WGPU_RENDER_BACKEND, message.clone());
    status.mark_skipped_with_message(frame_index, RenderFrameSkipReason::BackendError, message);
    Some(RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "nara.render-wgpu.frame",
    ))
}

#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
fn required_quad_pipeline_keys(
    batches: &[WgpuQuadBatch],
    format: wgpu::TextureFormat,
) -> Vec<crate::quad::WgpuQuadPipelineKey> {
    let mut keys = Vec::new();
    for key in batches
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .map(|batch| crate::quad::WgpuQuadPipelineKey {
            format,
            alpha_mode: batch.material.alpha_mode,
            material: batch.material_extension.as_ref().map(|material| {
                crate::quad::WgpuMaterial2dPipelineKey::new(material.binding.generation())
            }),
        })
    {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

#[cfg(test)]
mod tests;
