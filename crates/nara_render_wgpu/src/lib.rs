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
mod history;
mod host;
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
mod work;

use crate::backend::{WgpuRenderBackend, render_backend_state};
#[cfg(test)]
use crate::frame::WgpuFramePacketSlot;
#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
use crate::quad::WgpuQuadBatch;
#[cfg(all(test, any(feature = "stock-sprite", feature = "stock-ui")))]
use crate::quad::{WgpuQuadBatchBuffer, WgpuQuadPipelineDrawRef, draw_quad_batch_buffers};
use crate::surface::SurfaceDropReason;
#[cfg(test)]
use crate::surface::WgpuSurfaceState;
use nara_app::{RuntimeFault, RuntimeFaultKind};
use nara_render::{RenderBackendStatus, RenderFrameSkipReason};
#[cfg(test)]
use nara_window::WindowId;

pub use crate::backend::{WgpuBackendState, WgpuFrameTransactionStats};
#[cfg(feature = "stock-ui")]
pub use crate::candidate::WgpuUiCandidateSubmitFailure;
pub use crate::candidate::{WgpuCandidateSubmitFailure, WgpuPublishedFrame, WgpuRenderCandidate};
pub use crate::error::WgpuRenderError;
pub use crate::frame::WgpuPolicyFrameReader;
pub use crate::frame::{WgpuFrameCaptureError, WgpuFrameSet as WgpuPolicyCaptureSet};
pub use crate::graph::{
    AccessRole, GraphError, GraphLimits, LimitKind, TextureAlpha, TextureTransfer,
};
pub use crate::history::{
    WgpuEmptyHistoryReason, WgpuHistoryBinding, WgpuHistoryBuffer, WgpuHistoryLineage,
    WgpuHistoryReset, WgpuHistoryTexture, WgpuHistoryView,
};
pub use crate::host::{
    WgpuImageTargetActivation, WgpuImageTargetIdentity, WgpuImageTargetReservation, WgpuRenderHost,
    WgpuRenderHostError, WgpuRenderProducer, WgpuRenderProducerId, WgpuWindowDeviceScope,
    WgpuWindowFrame, WgpuWindowFrameOutcome,
};
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
pub use crate::retirement::{WgpuGpuRetirement, WgpuGpuRetirementError};
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
    pub use crate::frame::{
        WgpuCapturedPolicyBatch, WgpuFrameCaptureError, take_captured_policy_batch,
    };
    #[cfg(feature = "editor-imgui")]
    pub use crate::host::{
        WgpuEditorImguiFrame, WgpuEditorImguiRenderer, WgpuEditorImguiRendererFailure,
    };
    pub use crate::host::{WgpuRuntimeHostFacts, WgpuRuntimeMaintenance};
    pub use crate::{
        host::{
            WgpuImageTargetActivation, WgpuImageTargetIdentity, WgpuImageTargetReservation,
            WgpuRenderHost, WgpuRenderHostError, WgpuRenderProducer, WgpuRenderProducerId,
        },
        output::{WgpuDisplayOccurrence, WgpuImageDisplay, WgpuImageOutput},
        presented::{WgpuPresentedFrameReceipt, WgpuSubmittedImageReceipt},
        retirement::{WgpuGpuRetirement, WgpuGpuRetirementError},
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

#[cfg(test)]
impl PreparedSubmitterDraw {
    fn instance_buffer_bytes(&self) -> u64 {
        #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
        {
            self.buffers.iter().fold(0_u64, |total, buffer| {
                total.saturating_add(buffer.logical_bytes())
            })
        }
        #[cfg(not(any(feature = "stock-sprite", feature = "stock-ui")))]
        {
            0
        }
    }
}

// Private legacy quad oracle for native readback, allocation and Runtime custody tests.
// Production execution uses typed programs through WgpuRenderHost.
#[cfg(test)]
#[derive(Debug, Default)]
struct PreparedSubmitterDraw {
    #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
    pipelines: Vec<WgpuQuadPipelineDrawRef>,
    #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
    buffers: Vec<WgpuQuadBatchBuffer>,
    #[cfg(feature = "stock-ui")]
    ui_packets: Vec<ui::WgpuLoweredUiPacketFacts>,
    draw_calls: u32,
    sprites: u32,
    #[cfg(feature = "stock-ui")]
    image_sprites: u32,
    #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
    instance_reservation: Option<budget::InstanceReservation>,
}

#[cfg(test)]
#[derive(Debug)]
struct PreparedViewDraw {
    viewport: nara_render::ViewportRect,
    load: nara_render::ViewLoadOp,
    draw: PreparedSubmitterDraw,
}

#[cfg(test)]
#[derive(Debug, Default)]
struct PreparedTargetDraw {
    views: Vec<PreparedViewDraw>,
    canvas: Option<canvas::PreparedWorldCanvas>,
    draw_calls: u32,
    sprites: u32,
    #[cfg(feature = "stock-ui")]
    image_sprites: u32,
    #[cfg(feature = "stock-ui")]
    ui_packets: Vec<ui::WgpuLoweredUiPacketFacts>,
}

#[cfg(test)]
impl PreparedTargetDraw {
    #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
    fn commit_instance_reservations(&mut self) {
        for view in &mut self.views {
            if let Some(reservation) = view.draw.instance_reservation.as_mut() {
                reservation.commit();
            }
        }
    }

    fn push(
        &mut self,
        viewport: nara_render::ViewportRect,
        load: nara_render::ViewLoadOp,
        mut draw: PreparedSubmitterDraw,
    ) {
        self.draw_calls = self.draw_calls.saturating_add(draw.draw_calls);
        self.sprites = self.sprites.saturating_add(draw.sprites);
        #[cfg(feature = "stock-ui")]
        {
            self.image_sprites = self.image_sprites.saturating_add(draw.image_sprites);
            self.ui_packets.append(&mut draw.ui_packets);
        }
        self.views.push(PreparedViewDraw {
            viewport,
            load,
            draw,
        });
    }

    fn instance_buffer_bytes(&self) -> u64 {
        self.views.iter().fold(0, |total, view| {
            total.saturating_add(view.draw.instance_buffer_bytes())
        })
    }
}

/// Private legacy ordered-draw oracle for native readback and comparison tests.
#[cfg(test)]
fn encode_target_draw(
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    prepared: &PreparedTargetDraw,
) {
    if let Some(canvas) = &prepared.canvas {
        let (world, ui) = prepared.views.split_at(canvas.world_view_count);
        encode_view_draws(encoder, canvas.view(), world);
        canvas.encode_composite(encoder, target);
        encode_view_draws(encoder, target, ui);
    } else {
        encode_view_draws(encoder, target, &prepared.views);
    }
}

#[cfg(test)]
fn encode_view_draws(
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    views: &[PreparedViewDraw],
) {
    for view in views {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nara_ordered_view_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: match view.load {
                        nara_render::ViewLoadOp::Clear(color) => {
                            wgpu::LoadOp::Clear(clear_color_to_wgpu(color))
                        }
                        nara_render::ViewLoadOp::Load => wgpu::LoadOp::Load,
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(
            view.viewport.physical_x as f32,
            view.viewport.physical_y as f32,
            view.viewport.physical_width as f32,
            view.viewport.physical_height as f32,
            0.0,
            1.0,
        );
        #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
        draw_quad_batch_buffers(
            &mut pass,
            &view.draw.pipelines,
            &view.draw.buffers,
            view.viewport,
        );
    }
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
            material: batch
                .material_extension
                .as_ref()
                .map(|material| material.definition),
        })
    {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

/// Private legacy surface-submit oracle used by the test-only backend transaction.
#[cfg(test)]
fn render_acquired_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    window_id: WindowId,
    surface_state: &WgpuSurfaceState,
    surface_texture: wgpu::SurfaceTexture,
    draw: &mut PreparedTargetDraw,
) -> Result<(), WgpuRenderError> {
    let config = surface_state
        .config
        .as_ref()
        .ok_or(WgpuRenderError::SurfaceUnconfigured { window_id })?;
    let view = surface_texture
        .texture
        .create_view(&wgpu::TextureViewDescriptor {
            format: Some(config.format.add_srgb_suffix()),
            ..Default::default()
        });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("nara_wgpu_surface_encoder"),
    });
    encode_target_draw(&mut encoder, &view, draw);
    #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
    draw.commit_instance_reservations();
    queue.submit([encoder.finish()]);
    queue.present(surface_texture);
    Ok(())
}

#[cfg(test)]
mod tests;
