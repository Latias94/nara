//! Wgpu backend for backend-neutral nara render data.

mod allocation;
mod backend;
#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
mod budget;
mod candidate;
mod canvas;
#[cfg(all(
    feature = "winit",
    any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd"
    )
))]
pub mod desktop;
mod device;
mod error;
mod frame;
mod graph;
mod host;
#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
mod material_textures;
mod observation;
mod output;
mod plugin;
mod policy;
mod preparation;
mod presentation;
mod presented;
mod program;
#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
mod quad;
#[cfg(all(test, any(feature = "sprite-render", feature = "hud-runtime")))]
mod readback_tests;
mod requirements;
mod retirement;
mod runtime;
mod runtime_timing;
#[cfg(feature = "sprite-render")]
mod sprite;
#[cfg(all(test, feature = "sprite-render"))]
mod sprite_probe;
mod stock_policy;
#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
mod stock_quad;
mod submission;
mod surface;
mod telemetry;
#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
mod texture;
mod texture_cache;
#[cfg(feature = "hud-runtime")]
mod ui;
#[cfg(all(test, feature = "sprite-render"))]
mod view_probe;
#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
mod view_uniform;
mod work;

use crate::gpu::backend::{WgpuRenderBackend, render_backend_state};
#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
use crate::gpu::quad::WgpuQuadBatch;
use crate::gpu::surface::SurfaceDropReason;
use crate::{RenderBackendStatus, RenderFrameSkipReason};
use nara_app::{RuntimeFault, RuntimeFaultKind};

pub use crate::gpu::backend::{WgpuBackendState, WgpuFrameTransactionStats};
pub use crate::gpu::candidate::{
    WgpuCandidateSubmitFailure, WgpuPublishedFrame, WgpuRenderCandidate,
};
pub use crate::gpu::device::{GpuContext, GpuDeviceErrors};
pub use crate::gpu::error::WgpuRenderError;
pub use crate::gpu::frame::WgpuPolicyFrameReader;
pub use crate::gpu::frame::{
    WgpuCapturedPolicyBatch, WgpuFrameCaptureError, WgpuFrameSet as WgpuPolicyCaptureSet,
    take_captured_policy_batch,
};
pub use crate::gpu::graph::{
    AccessRole, GraphError, GraphLimits, LimitKind, TextureAlpha, TextureInterpretation,
    TextureTransfer,
};
pub use crate::gpu::host::{
    WgpuImageTargetActivation, WgpuImageTargetIdentity, WgpuImageTargetReservation, WgpuRenderHost,
    WgpuRenderHostError, WgpuRenderProducer, WgpuRenderProducerId,
};
#[cfg(feature = "sprite-render")]
pub use crate::gpu::material_textures::WgpuMaterialTextureBinding;
pub use crate::gpu::observation::WgpuRenderObservation;
pub use crate::gpu::output::WgpuImageOutput;
pub use crate::gpu::plugin::{
    WGPU_CAPTURE_PLUGIN_DECLARATION, WGPU_CAPTURE_PLUGIN_ID, WGPU_STOCK_CAPTURE_PLUGIN_DECLARATION,
    WGPU_STOCK_CAPTURE_PLUGIN_ID, WGPU_STOCK_POLICY_PLUGIN_DECLARATION,
    WGPU_STOCK_POLICY_PLUGIN_ID, WGPU_WINDOW_PRESENTATION_PLUGIN_DECLARATION,
    WGPU_WINDOW_PRESENTATION_PLUGIN_ID, WgpuCapturePlugin, WgpuPolicyFrameWriter,
    WgpuStockCapturePlugin, WgpuStockCaptureSet, WgpuStockFrame, WgpuStockFrameReader,
    WgpuStockPolicyPlugin, WgpuWindowPresentationPlugin, install_wgpu_policy,
};
#[cfg(feature = "sprite-render")]
pub use crate::gpu::plugin::{
    WGPU_STOCK_SPRITE_PLUGIN_DECLARATION, WGPU_STOCK_SPRITE_PLUGIN_ID, WgpuStockSpritePlugin,
};
#[cfg(feature = "hud-runtime")]
pub use crate::gpu::plugin::{
    WGPU_STOCK_UI_PLUGIN_DECLARATION, WGPU_STOCK_UI_PLUGIN_ID, WgpuStockUiPlugin,
};
#[cfg(feature = "hud-runtime")]
pub use crate::gpu::policy::WgpuUiProgramBinding;
pub use crate::gpu::policy::{
    WgpuPolicyBindingContext, WgpuPolicyBuildContext, WgpuPolicyDefinition, WgpuPolicyFrameFacts,
    WgpuPolicyInstallation, WgpuPolicyInstallationId, WgpuPolicyProgram, WgpuRenderInput,
    WgpuRenderPolicy,
};
pub use crate::gpu::presented::WgpuSubmittedImageReceipt;
pub use crate::gpu::program::{
    RenderPass, WgpuFeatureAllocation, WgpuFeatureDeclaration, WgpuFeatureDefinition,
    WgpuFeatureEncodeContext, WgpuFeatureInstance, WgpuFeaturePreflightContext,
    WgpuFeaturePrepareContext, WgpuNativeResources, WgpuOperationResources, WgpuProgramBuffer,
    WgpuProgramBufferOutput, WgpuProgramOutput, WgpuProgramOutputs, WgpuProgramRead,
    WgpuProgramTexture, WgpuProgramTextureDescriptor, WgpuProgramTextureOutput, WgpuProgramWrite,
    WgpuRenderFeature, WgpuRenderProgram, WgpuRenderProgramBuilder, WgpuRenderProgramError,
};
pub use crate::gpu::requirements::{WgpuDeviceRequirements, WgpuDeviceRequirementsError};
pub use crate::gpu::runtime_timing::{
    WgpuNamespacePhase, WgpuNamespaceTimings, WgpuRuntimeFramePhase, WgpuRuntimeFrameTimings,
};
#[cfg(feature = "hud-runtime")]
pub use crate::gpu::stock_policy::WgpuCompositeColor;
pub use crate::gpu::stock_policy::{
    WgpuSceneColor, WgpuStockBindings, WgpuStockCapture, WgpuStockCaptureAdapter, WgpuStockPolicy,
    WgpuStockProgramBuilder, WgpuStockStages, WgpuWorldColor,
};
pub use crate::gpu::submission::{WgpuSubmissionPrefix, WgpuSubmissionPrefixError};
pub use crate::gpu::telemetry::{
    MAX_ADAPTER_SUMMARY_FIELD_BYTES, MAX_BUFFERED_FRAME_COMPLETIONS, MAX_PENDING_FRAME_COMPLETIONS,
    WgpuAdapterSummary, WgpuBackendTelemetryConfig, WgpuFrameCompletionSample,
    WgpuFrameCompletionStats, WgpuGpuResourceStats, WgpuTelemetryConfigError,
};
pub use crate::gpu::texture_cache::TextureCacheStats;
pub use crate::gpu::work::WgpuNativeWorkStats;
#[cfg(feature = "hud-runtime")]
pub use crate::gpu::{host::WgpuUiSubmission, plugin::WgpuPolicyUiCaptureFailure};

pub use crate::gpu::surface::{
    SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus, WgpuConfiguredPresentMode,
    choose_present_mode, clear_color_to_wgpu, map_present_mode, surface_acquire_policy,
    surface_resize_action,
};
#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
pub use crate::gpu::texture::WgpuTextureCacheStats as WgpuRenderTextureCacheStats;
#[cfg(feature = "hud-runtime")]
pub use crate::gpu::ui::{
    UiCapture as WgpuUiCapture, UiFeature as WgpuUiFeature, UiReport as WgpuUiReport,
};

#[doc(hidden)]
pub mod execution {
    pub use crate::gpu::host::{WgpuRuntimeHostFacts, WgpuRuntimeMaintenance};
    pub use crate::gpu::{
        host::{
            WgpuImageTargetActivation, WgpuImageTargetIdentity, WgpuImageTargetReservation,
            WgpuRenderHost, WgpuRenderHostError, WgpuRenderProducer, WgpuRenderProducerId,
        },
        output::WgpuImageOutput,
        presented::{WgpuPresentedFrameReceipt, WgpuSubmittedImageReceipt},
        runtime::{
            WgpuRuntimeCapture, WgpuRuntimeCaptureFailure, WgpuRuntimeCaptureIdentity,
            WgpuRuntimeCompletion, WgpuRuntimeConnection, WgpuRuntimeExchangeError,
            WgpuRuntimeResult, WgpuRuntimeResultFailure, cancel_runtime_frame,
            complete_runtime_frame, take_runtime_frame,
        },
    };

    #[cfg(feature = "hud-runtime")]
    pub use crate::gpu::backend::WgpuSubmittedUiFacts;

    #[cfg(feature = "hud-runtime")]
    pub use crate::gpu::host::WgpuUiSubmission;

    #[cfg(feature = "hud-runtime")]
    #[must_use]
    pub fn submitted_ui_facts(
        backend: &crate::gpu::WgpuRenderObservation,
        submission: crate::hud::UiBackendSubmissionIdentity,
        backend_generation: crate::hud::UiPresentationBackendGeneration,
    ) -> Option<WgpuSubmittedUiFacts> {
        backend.submitted_ui_facts(submission, backend_generation)
    }

    /// Returns the exact surface presentation published by the current render transaction.
    #[must_use]
    pub fn presented_frame_this_transaction(
        backend: &crate::gpu::WgpuRenderObservation,
    ) -> Option<WgpuPresentedFrameReceipt> {
        backend.presented_frame_this_transaction()
    }
}

const WGPU_RENDER_BACKEND: &str = "wgpu";

#[cfg(feature = "hud-runtime")]
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

#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
fn required_quad_pipeline_keys(
    batches: &[WgpuQuadBatch],
    format: wgpu::TextureFormat,
) -> Vec<crate::gpu::quad::WgpuQuadPipelineKey> {
    let mut keys = Vec::new();
    for key in batches
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .map(|batch| crate::gpu::quad::WgpuQuadPipelineKey {
            format,
            alpha_mode: batch.material.alpha_mode,
            material: batch.material_extension.as_ref().map(|material| {
                crate::gpu::quad::WgpuMaterial2dPipelineKey::new(material.binding.generation())
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
