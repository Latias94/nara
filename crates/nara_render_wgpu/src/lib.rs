//! Wgpu backend for backend-neutral nara render data.

mod allocation;
mod backend;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
mod budget;
mod canvas;
mod device;
mod editor_execution;
mod error;
mod frame;
mod observation;
mod plugin;
mod presented;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
mod quad;
#[cfg(all(test, any(feature = "sprite-submitter", feature = "ui-submitter")))]
mod readback_tests;
mod retirement;
mod runtime;
#[cfg(feature = "sprite-submitter")]
mod sprite;
#[cfg(all(test, feature = "sprite-submitter"))]
mod sprite_probe;
mod surface;
mod telemetry;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
mod texture;
#[cfg(feature = "ui-submitter")]
mod ui;
#[cfg(all(test, feature = "sprite-submitter"))]
mod view_probe;

use crate::backend::render_backend_state;
use crate::frame::{WgpuFrameCaptureError, WgpuFramePacketSlot};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::quad::{
    WgpuQuadBatch, WgpuQuadBatchBuffer, WgpuQuadPipelineDrawRef, draw_quad_batch_buffers,
};
use crate::surface::{SurfaceDropReason, WgpuSurfaceState};
use nara_app::{
    App, CoreStage, FrameExecutionStart, Plugin, PluginError, RuntimeFault, RuntimeFaultKind,
    RuntimeFaultReporter, RuntimeGeneration,
};
use nara_ecs::{Query, Res, ResMut, schedule::IntoScheduleConfigs};
use nara_render::{
    ExtractedViews, FrameStats, RenderBackendState, RenderBackendStatus, RenderFrame,
    RenderFrameSkipReason, build_render_frame_packet_with_world_canvas,
};
#[cfg(feature = "ui-submitter")]
use nara_ui_render::__private::UiBackendSubmissionReader;
#[cfg(feature = "ui-submitter")]
use nara_ui_render::{UiPresentationBackendFaultDomain, UiPresentationBackendGeneration};
use nara_window::{
    PrimaryWindowId, Window, WindowId,
    backend::{BackendWindowHandles, WindowPresentationRequirement},
};

pub use crate::backend::{WgpuBackendState, WgpuFrameTransactionStats, WgpuRenderBackend};
pub use crate::error::WgpuRenderError;
pub use crate::observation::WgpuRenderObservation;
#[cfg(feature = "sprite-submitter")]
pub use crate::plugin::{
    WGPU_SPRITE_SUBMITTER_PLUGIN_DECLARATION, WGPU_SPRITE_SUBMITTER_PLUGIN_ID,
    WgpuSpriteSubmitterPlugin,
};
#[cfg(feature = "ui-submitter")]
pub use crate::plugin::{
    WGPU_UI_SUBMITTER_PLUGIN_DECLARATION, WGPU_UI_SUBMITTER_PLUGIN_ID, WgpuUiSubmitterPlugin,
};
pub use crate::telemetry::{
    MAX_ADAPTER_SUMMARY_FIELD_BYTES, MAX_BUFFERED_FRAME_COMPLETIONS, MAX_PENDING_FRAME_COMPLETIONS,
    WgpuAdapterSummary, WgpuBackendTelemetryConfig, WgpuFrameCompletionSample,
    WgpuFrameCompletionStats, WgpuGpuResourceStats, WgpuTelemetryConfigError,
};

pub use crate::surface::{
    SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus, WgpuConfiguredPresentMode,
    choose_present_mode, clear_color_to_wgpu, map_present_mode, surface_acquire_policy,
    surface_resize_action,
};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
pub use crate::texture::WgpuTextureCacheStats as WgpuRenderTextureCacheStats;

#[doc(hidden)]
pub mod __private {
    #[cfg(feature = "editor-imgui")]
    pub use crate::editor_execution::{
        WgpuEditorImguiFrame, WgpuEditorImguiRenderer, WgpuEditorImguiRendererFailure,
    };
    pub use crate::{
        editor_execution::{
            WgpuEditorDeviceScope, WgpuEditorExecution, WgpuEditorExecutionError,
            WgpuEditorFrameOutcome, WgpuEditorReservedViewportTexture, WgpuEditorViewId,
            WgpuEditorViewportIdentity, WgpuEditorViewportTargetActivation,
            WgpuEditorViewportTargetReservation, WgpuEditorWindowFrame,
        },
        presented::{
            WgpuEditorCompositeReceipt, WgpuEditorWindowPresentedReceipt,
            WgpuPresentedFrameReceipt, WgpuSubmittedImageReceipt,
        },
        retirement::{WgpuGpuRetirement, WgpuGpuRetirementError},
        runtime::{
            WgpuRuntimeCapture, WgpuRuntimeCaptureFailure, WgpuRuntimeCaptureIdentity,
            WgpuRuntimeCompletion, WgpuRuntimeConnection, WgpuRuntimeExchangeError,
            WgpuRuntimeResult, WgpuRuntimeResultFailure, cancel_runtime_frame,
            complete_runtime_frame, take_runtime_frame,
        },
    };

    #[cfg(feature = "ui-submitter")]
    pub use crate::backend::WgpuSubmittedUiFacts;

    #[cfg(feature = "ui-submitter")]
    pub use crate::editor_execution::{
        WgpuEditorUiImageSubmission, WgpuEditorUiSubmission, WgpuEditorUiSubmitFailure,
    };

    #[cfg(feature = "ui-submitter")]
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

#[cfg(feature = "ui-submitter")]
fn saturating_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuRenderPlugin;

pub const WGPU_RENDER_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.render-wgpu");
pub const WGPU_BACKEND_SHUTDOWN_OBLIGATION: nara_app::PluginShutdownObligationId =
    nara_app::PluginShutdownObligationId::new("nara.render-wgpu.backend");
const WGPU_RENDER_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("render-wgpu")];
const WGPU_RENDER_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[nara_render::RENDER_PLUGIN_ID];
pub const WGPU_RENDER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(WGPU_RENDER_PLUGIN_ID, nara_app::PluginCategory::Backend)
        .requires_plugins(WGPU_RENDER_PLUGIN_REQUIREMENTS)
        .requires_product_capabilities(WGPU_RENDER_PRODUCT_REQUIREMENTS)
        .shutdown_obligations(&[WGPU_BACKEND_SHUTDOWN_OBLIGATION]);

impl Plugin for WgpuRenderPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &WGPU_RENDER_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<WgpuRenderObservation>()?;
        app.init_resource::<WgpuFramePacketSlot>()?;
        app.init_resource::<RenderBackendStatus>()?;
        let world = app.world_mut()?;
        if let Some(requirement) = world.get_resource::<WindowPresentationRequirement>() {
            return Err(PluginError::SetupFailed {
                plugin: WGPU_RENDER_PLUGIN_ID,
                message: format!(
                    "native window presentation is already required by {}",
                    requirement.backend()
                ),
            });
        }
        world.insert_resource(WindowPresentationRequirement::new(WGPU_RENDER_BACKEND));
        world
            .resource_mut::<RenderBackendStatus>()
            .mark_state(WGPU_RENDER_BACKEND, RenderBackendState::Uninitialized);
        app.configure_sets(
            CoreStage::Render,
            (
                frame::WgpuFrameSet::Reset,
                frame::WgpuFrameSet::CollectSprite,
                frame::WgpuFrameSet::CollectUi,
                frame::WgpuFrameSet::Seal,
            )
                .chain()
                .after(nara_render::begin_render_frame),
        )?;
        app.add_systems(
            CoreStage::Render,
            (
                reset_wgpu_frame_packet.in_set(frame::WgpuFrameSet::Reset),
                capture_wgpu_frame_packet.in_set(frame::WgpuFrameSet::Seal),
            ),
        )?;
        runtime::install(app)?;
        Ok(())
    }
}

impl PreparedSubmitterDraw {
    fn instance_buffer_bytes(&self) -> u64 {
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        {
            self.buffers.iter().fold(0_u64, |total, buffer| {
                total.saturating_add(buffer.logical_bytes())
            })
        }
        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        {
            0
        }
    }
}

fn reset_wgpu_frame_packet(
    mut slot: ResMut<WgpuFramePacketSlot>,
    connection: Res<runtime::WgpuRuntimeConnection>,
    authority: Res<runtime::ConnectionAuthority>,
    faults: Option<Res<RuntimeFaultReporter>>,
) {
    if !authority.is_current(&connection) {
        if let Some(faults) = faults {
            faults.report(RuntimeFault::engine(
                RuntimeFaultKind::RequiredService,
                "nara.render-wgpu.connection",
            ));
        }
        return;
    }
    if connection.can_capture_from(&authority) {
        slot.reset();
    }
}

#[cfg(feature = "sprite-submitter")]
fn collect_wgpu_sprite_payload(
    sprite_batches: Res<nara_sprite_render::__private::SpriteBatches>,
    mut slot: ResMut<WgpuFramePacketSlot>,
    connection: Res<runtime::WgpuRuntimeConnection>,
    authority: Res<runtime::ConnectionAuthority>,
) {
    if connection.can_capture_from(&authority) {
        slot.collect_sprite(sprite_batches.clone());
    }
}

#[cfg(feature = "ui-submitter")]
fn collect_wgpu_ui_payload(
    mut submissions: UiBackendSubmissionReader,
    mut slot: ResMut<WgpuFramePacketSlot>,
    connection: Res<runtime::WgpuRuntimeConnection>,
    authority: Res<runtime::ConnectionAuthority>,
) {
    if !connection.can_capture_from(&authority) || !slot.accepts_ui_collection() {
        return;
    }
    match submissions.take() {
        Ok(frame) => {
            let (packets, submissions) = frame.into_parts();
            slot.collect_ui(packets, submissions);
        }
        Err(error) => slot.reject_ui_collection(error),
    }
}

fn capture_wgpu_frame_packet(
    generation: Option<Res<RuntimeGeneration>>,
    frame_start: Res<FrameExecutionStart>,
    frame: Res<RenderFrame>,
    primary_window_id: Option<Res<PrimaryWindowId>>,
    windows: Query<&Window>,
    views: Res<ExtractedViews>,
    mut slot: ResMut<WgpuFramePacketSlot>,
    mut connection: ResMut<runtime::WgpuRuntimeConnection>,
    authority: Res<runtime::ConnectionAuthority>,
) {
    if !connection.can_capture_from(&authority) {
        return;
    }
    let primary_window_id = primary_window_id.map(|primary| primary.0);
    let window = primary_window_id
        .and_then(|id| windows.iter().find(|window| window.id == id))
        .map(|window| nara_render::RenderWindowPacket {
            id: window.id,
            resolution: window.resolution,
            present_mode: window.present_mode,
        });
    let Some(generation) = generation.map(|generation| *generation) else {
        slot.reject_capture(WgpuFrameCaptureError::MissingRuntimeGeneration);
        publish_wgpu_capture(
            &mut slot,
            &mut connection,
            *frame,
            window,
            views.world_canvas(),
        );
        return;
    };
    if !slot.payload_matches_topology(
        &views,
        #[cfg(feature = "ui-submitter")]
        primary_window_id,
    ) {
        slot.reject_capture(WgpuFrameCaptureError::BatchTopologyMismatch);
        publish_wgpu_capture(
            &mut slot,
            &mut connection,
            *frame,
            window,
            views.world_canvas(),
        );
        return;
    }
    let captured = build_render_frame_packet_with_world_canvas(
        generation,
        frame.index,
        primary_window_id,
        windows.iter(),
        &views,
        views.world_canvas(),
    );
    slot.seal(
        #[cfg(feature = "ui-submitter")]
        primary_window_id,
        captured,
        frame_start.frame(),
        frame_start.started_at(),
    );
    publish_wgpu_capture(
        &mut slot,
        &mut connection,
        *frame,
        window,
        views.world_canvas(),
    );
}

fn publish_wgpu_capture(
    slot: &mut WgpuFramePacketSlot,
    connection: &mut runtime::WgpuRuntimeConnection,
    frame: RenderFrame,
    window: Option<nara_render::RenderWindowPacket>,
    canvas: Option<nara_render::WorldCanvas>,
) {
    connection.publish(runtime::PendingFrame {
        captured: slot.take_capture().unwrap_or(Ok(None)),
        frame,
        window,
        canvas,
        #[cfg(feature = "ui-submitter")]
        ui: slot.take_ui_submissions(),
    });
}

#[derive(Debug, Default)]
struct PreparedSubmitterDraw {
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    pipelines: Vec<WgpuQuadPipelineDrawRef>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    buffers: Vec<WgpuQuadBatchBuffer>,
    #[cfg(feature = "ui-submitter")]
    ui_packets: Vec<ui::WgpuLoweredUiPacketFacts>,
    draw_calls: u32,
    sprites: u32,
    #[cfg(feature = "ui-submitter")]
    image_sprites: u32,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    instance_reservation: Option<budget::InstanceReservation>,
}

#[derive(Debug)]
struct PreparedViewDraw {
    viewport: nara_render::ViewportRect,
    load: nara_render::ViewLoadOp,
    draw: PreparedSubmitterDraw,
}

#[derive(Debug, Default)]
struct PreparedTargetDraw {
    views: Vec<PreparedViewDraw>,
    canvas: Option<canvas::PreparedWorldCanvas>,
    draw_calls: u32,
    sprites: u32,
    #[cfg(feature = "ui-submitter")]
    image_sprites: u32,
    #[cfg(feature = "ui-submitter")]
    ui_packets: Vec<ui::WgpuLoweredUiPacketFacts>,
}

impl PreparedTargetDraw {
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn commit_instance_reservations(&mut self) {
        for view in &mut self.views {
            if let Some(reservation) = view.draw.instance_reservation.as_mut() {
                reservation.commit();
            }
        }
    }

    fn retain_images_until_submission_complete(&self, queue: &wgpu::Queue) {
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        for view in &self.views {
            for buffer in &view.draw.buffers {
                buffer.retain_images_until_submission_complete(queue);
            }
        }
        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        let _ = queue;
    }

    fn push(
        &mut self,
        viewport: nara_render::ViewportRect,
        load: nara_render::ViewLoadOp,
        mut draw: PreparedSubmitterDraw,
    ) {
        self.draw_calls = self.draw_calls.saturating_add(draw.draw_calls);
        self.sprites = self.sprites.saturating_add(draw.sprites);
        #[cfg(feature = "ui-submitter")]
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

/// Encodes the complete ordered transaction. Runtime, Editor, and native readback share this path.
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
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        draw_quad_batch_buffers(
            &mut pass,
            &view.draw.pipelines,
            &view.draw.buffers,
            view.viewport,
        );
    }
}

impl WgpuRenderBackend {
    /// Executes one original CPU capture on this concrete local GPU owner.
    ///
    /// The returned move-only result must be reconciled with the original Runtime even if its
    /// state changed while native execution was in progress. Native execution never borrows World.
    #[doc(hidden)]
    pub fn execute_runtime_frame(
        &mut self,
        handles: &BackendWindowHandles,
        mut capture: runtime::WgpuRuntimeCapture,
    ) -> runtime::WgpuRuntimeResult {
        if !capture.allows_execution() || self.runtime_close_started() {
            return runtime::cancel_runtime_frame(capture);
        }
        let mut frame = capture.pending.frame;
        let mut stats = FrameStats::default();
        let mut status = RenderBackendStatus::default();
        status.mark_state(WGPU_RENDER_BACKEND, render_backend_state(self.state()));
        self.validate_world_presentation_current(capture.pending.window, capture.pending.canvas);
        self.begin_frame_transaction(frame.index, None);
        #[cfg(feature = "ui-submitter")]
        let mut ui = capture.pending.ui.take();
        let mut fault = None;
        self.begin_runtime_execution();
        let result = match std::mem::replace(&mut capture.pending.captured, Ok(None)) {
            Ok(Some(packet)) => self.render_packet(
                Some(handles),
                packet,
                #[cfg(feature = "ui-submitter")]
                ui.as_mut(),
                &mut frame,
                &mut stats,
                &mut status,
            ),
            Ok(None) => {
                status.mark_skipped_with_message(
                    frame.index,
                    RenderFrameSkipReason::NoViews,
                    "no admitted render frame packet",
                );
                frame.mark_skipped();
                Ok(())
            }
            Err(error) => {
                self.reject_captured_packet();
                status.mark_skipped_with_message(
                    frame.index,
                    RenderFrameSkipReason::InvalidTopology,
                    error.to_string(),
                );
                fault = Some(RuntimeFault::engine(
                    RuntimeFaultKind::System,
                    "nara.render.frame-packet",
                ));
                frame.mark_skipped();
                Err(WgpuRenderError::ContentRejected {
                    message: error.to_string(),
                })
            }
        };
        let presented_world = self.take_presented_world();
        #[cfg(feature = "ui-submitter")]
        if let Some(ui) = ui {
            let device_fault_generation = result.as_ref().err().and_then(|error| {
                (frame::classify_ui_backend_fault(error)
                    == UiPresentationBackendFaultDomain::Device)
                    .then(|| {
                        UiPresentationBackendGeneration::new(
                            self.instance_id(),
                            self.device_epoch(),
                        )
                    })
                    .flatten()
            });
            let completion = frame::ui_completion(
                &ui,
                &result,
                frame.state,
                device_fault_generation,
                status.last_skip().and_then(|skip| skip.message()),
            );
            frame::complete_ui_submissions_into(ui, completion, &mut capture.completed);
        }
        if let Err(error) = result {
            let render_fault = handle_wgpu_render_error(self, error, frame.index, &mut status);
            fault = fault.or(render_fault);
            frame.mark_skipped();
        }
        let observation = self.take_observation();
        capture.into_result(frame, stats, status, observation, presented_world, fault)
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

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
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
    queue.submit([encoder.finish()]);
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    draw.commit_instance_reservations();
    draw.retain_images_until_submission_complete(queue);
    queue.present(surface_texture);
    Ok(())
}

#[cfg(test)]
mod tests;
