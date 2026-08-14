//! Wgpu backend for backend-neutral nara render data.

#[cfg(all(feature = "ui-submitter", test))]
use std::sync::Arc;

mod backend;
mod error;
mod frame;
mod plugin;
mod presented;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
mod quad;
#[cfg(feature = "sprite-submitter")]
mod sprite;
mod surface;
mod telemetry;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
mod texture;
#[cfg(feature = "ui-submitter")]
mod ui;

use crate::backend::render_backend_state;
#[cfg(all(feature = "ui-submitter", test))]
use crate::frame::WgpuUiSubmissionState;
use crate::frame::{WgpuFrameCaptureError, WgpuFramePacketSlot};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::quad::{
    WgpuQuadBatch, WgpuQuadBatchBuffer, WgpuQuadPipelineDrawRef, draw_quad_batch_buffers,
};
use crate::surface::{SurfaceDropReason, WgpuSurfaceState};
use nara_app::{
    __RuntimeDriverPort, App, CoreStage, FrameExecutionStart, Plugin, PluginError,
    PluginShutdownContext, RuntimeDriverScope, RuntimeFault, RuntimeFaultKind,
    RuntimeFaultReporter, RuntimeGeneration, RuntimeState, RuntimeWorldAccessError,
};
#[cfg(test)]
use nara_ecs::Resource;
use nara_ecs::{Query, Res, ResMut, schedule::IntoScheduleConfigs, system::NonSendMarker};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_material::AlphaMode2d;
#[cfg(test)]
use nara_render::Extent2d;
#[cfg(all(feature = "ui-submitter", test))]
use nara_render::RenderFrameState;
use nara_render::{
    Color, ExtractedViews, FrameStats, RenderBackendState, RenderBackendStatus, RenderFrame,
    RenderFrameSkipReason, build_render_frame_packet,
};
#[cfg(all(test, any(feature = "sprite-submitter", feature = "ui-submitter")))]
use nara_render::{RenderTarget, ViewportRect};
#[cfg(all(feature = "ui-submitter", test))]
use nara_ui_render::__private::UiPreparedBackendSubmission;
#[cfg(all(feature = "ui-submitter", test))]
use nara_ui_render::__private::{UiBackendSubmission, UiSubmittedBackendSubmission};
#[cfg(feature = "ui-submitter")]
use nara_ui_render::__private::{UiBackendSubmissionReader, UiPresentationOutcomes};
#[cfg(all(feature = "ui-submitter", test))]
use nara_ui_render::{
    UiPaintPacket, UiPresentationAttemptId, UiPresentationCapabilityGeneration,
    UiPresentationEpoch, UiPresentationOutcome, UiPresentationRuntimeGeneration,
};
#[cfg(feature = "ui-submitter")]
use nara_ui_render::{UiPresentationBackendFaultDomain, UiPresentationBackendGeneration};
use nara_window::{
    PrimaryWindowId, Window, WindowId,
    backend::{BackendWindowHandles, WindowSurfaceRetirementDriver, WindowSurfaceRetirementError},
};

pub use crate::backend::{WgpuBackendState, WgpuFrameTransactionStats, WgpuRenderBackend};
pub use crate::error::WgpuRenderError;
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
    pub use crate::presented::WgpuPresentedFrameReceipt;

    #[cfg(feature = "ui-submitter")]
    pub use crate::backend::WgpuSubmittedUiFacts;

    #[cfg(feature = "ui-submitter")]
    #[must_use]
    pub fn submitted_ui_facts(
        backend: &crate::WgpuRenderBackend,
        submission: nara_ui_render::UiBackendSubmissionIdentity,
        backend_generation: nara_ui_render::UiPresentationBackendGeneration,
    ) -> Option<WgpuSubmittedUiFacts> {
        backend.submitted_ui_facts(submission, backend_generation)
    }

    /// Returns the exact surface presentation published by the current render transaction.
    #[must_use]
    pub fn presented_frame_this_transaction(
        backend: &crate::WgpuRenderBackend,
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
        app.init_resource::<WgpuRenderBackend>()?;
        app.init_resource::<WgpuFramePacketSlot>()?;
        app.init_resource::<RenderBackendStatus>()?;
        let world = app.world_mut()?;
        if let Some(driver) = world.get_resource::<WindowSurfaceRetirementDriver>() {
            return Err(PluginError::SetupFailed {
                plugin: WGPU_RENDER_PLUGIN_ID,
                message: format!(
                    "window surface retirement driver is already owned by {}",
                    driver.driver()
                ),
            });
        }
        world.insert_resource(WindowSurfaceRetirementDriver::new(
            WGPU_RENDER_BACKEND,
            retire_wgpu_window_surfaces,
        ));
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
                frame::WgpuFrameSet::Submit,
                frame::WgpuFrameSet::CompleteUi,
            )
                .chain()
                .after(nara_render::begin_render_frame),
        )?;
        app.add_systems(
            CoreStage::Render,
            (
                reset_wgpu_frame_packet.in_set(frame::WgpuFrameSet::Reset),
                capture_wgpu_frame_packet.in_set(frame::WgpuFrameSet::Seal),
                render_wgpu_surfaces.in_set(frame::WgpuFrameSet::Submit),
            ),
        )?;
        app.register_plugin_shutdown_obligation(WGPU_BACKEND_SHUTDOWN_OBLIGATION)?;
        Ok(())
    }

    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        if let Some(mut backend) = context.world_mut().get_resource_mut::<WgpuRenderBackend>() {
            backend
                .clear_gpu_resources(SurfaceDropReason::BackendCleanup)
                .map_err(|_| PluginError::SetupFailed {
                    plugin: WGPU_RENDER_PLUGIN_ID,
                    message: "failed to retire a window surface during shutdown".to_owned(),
                })?;
        }
        Ok(())
    }
}

fn retire_wgpu_window_surfaces(
    scope: &mut RuntimeDriverScope<'_>,
    window_ids: &[WindowId],
) -> Result<(), WindowSurfaceRetirementError> {
    match scope.__apply_port::<WgpuRenderBackend>(WgpuSurfaceRetirementRequest(window_ids.to_vec()))
    {
        Ok(result) => result.map_err(|_| WindowSurfaceRetirementError::DriverFailed {
            driver: WGPU_RENDER_BACKEND,
        }),
        // The backend resource owns every live surface. Its absence means Drop already released
        // and acknowledged those owners, so the window authority can finish retirement.
        Err(RuntimeWorldAccessError::MissingResource { .. }) => Ok(()),
        Err(_) => Err(WindowSurfaceRetirementError::DriverFailed {
            driver: WGPU_RENDER_BACKEND,
        }),
    }
}

pub struct WgpuSurfaceRetirementRequest(Vec<WindowId>);

impl __RuntimeDriverPort for WgpuRenderBackend {
    type Input = WgpuSurfaceRetirementRequest;
    type Output = Result<(), WgpuRenderError>;

    fn accepts_driver_state(state: RuntimeState) -> bool {
        matches!(
            state,
            RuntimeState::Running
                | RuntimeState::Paused
                | RuntimeState::Faulted
                | RuntimeState::Stopping
                | RuntimeState::CloseIncomplete
        )
    }

    fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output {
        self.retire_targets(&input.0, SurfaceDropReason::TargetShutdown)
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

fn reset_wgpu_frame_packet(mut slot: ResMut<WgpuFramePacketSlot>) {
    slot.reset();
}

#[cfg(feature = "sprite-submitter")]
fn collect_wgpu_sprite_payload(
    sprite_batches: Res<nara_sprite_render::SpriteBatches>,
    mut slot: ResMut<WgpuFramePacketSlot>,
) {
    slot.collect_sprite(sprite_batches.clone());
}

#[cfg(feature = "ui-submitter")]
fn collect_wgpu_ui_payload(
    mut submissions: UiBackendSubmissionReader,
    mut slot: ResMut<WgpuFramePacketSlot>,
) {
    if !slot.accepts_ui_collection() {
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
) {
    let Some(generation) = generation.map(|generation| *generation) else {
        slot.reject_capture(WgpuFrameCaptureError::MissingRuntimeGeneration);
        return;
    };
    let primary_window_id = primary_window_id.map(|primary| primary.0);
    if !slot.payload_matches_topology(
        &views,
        #[cfg(feature = "ui-submitter")]
        primary_window_id,
    ) {
        slot.reject_capture(WgpuFrameCaptureError::BatchTopologyMismatch);
        return;
    }
    let captured = build_render_frame_packet(
        generation,
        frame.index,
        primary_window_id,
        windows.iter(),
        &views,
    );
    slot.seal(
        #[cfg(feature = "ui-submitter")]
        primary_window_id,
        captured,
        frame_start.frame(),
        frame_start.started_at(),
    );
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
}

fn render_wgpu_surfaces(
    mut backend: ResMut<WgpuRenderBackend>,
    handles: Option<Res<BackendWindowHandles>>,
    mut packet_slot: ResMut<WgpuFramePacketSlot>,
    mut frame: ResMut<RenderFrame>,
    mut stats: ResMut<FrameStats>,
    mut status: ResMut<RenderBackendStatus>,
    faults: Option<Res<RuntimeFaultReporter>>,
    _main_thread: NonSendMarker,
) {
    backend.begin_frame_transaction(frame.index, None);
    #[cfg(feature = "ui-submitter")]
    let mut ui_submission_state = packet_slot.take_ui_submissions();
    let capture = packet_slot.take_capture();
    let result = match capture {
        Some(Ok(Some(packet))) => backend.render_packet(
            handles.as_deref(),
            packet,
            #[cfg(feature = "ui-submitter")]
            ui_submission_state.as_mut(),
            &mut frame,
            &mut stats,
            &mut status,
        ),
        Some(Ok(None)) | None => {
            stats.draw_calls = 0;
            stats.sprites = 0;
            status.clear_skip();
            status.mark_skipped_with_message(
                frame.index,
                RenderFrameSkipReason::NoViews,
                "no admitted render frame packet",
            );
            frame.mark_skipped();
            Ok(())
        }
        Some(Err(error)) => {
            backend.reject_captured_packet();
            stats.draw_calls = 0;
            stats.sprites = 0;
            status.clear_skip();
            status.mark_skipped_with_message(
                frame.index,
                RenderFrameSkipReason::InvalidTopology,
                error.to_string(),
            );
            if let Some(faults) = faults.as_deref() {
                faults.report(RuntimeFault::engine(
                    RuntimeFaultKind::System,
                    "nara.render.frame-packet",
                ));
            }
            frame.mark_skipped();
            Err(WgpuRenderError::ContentRejected {
                message: error.to_string(),
            })
        }
    };
    #[cfg(feature = "ui-submitter")]
    let device_fault_generation = result.as_ref().err().and_then(|error| {
        (frame::classify_ui_backend_fault(error) == UiPresentationBackendFaultDomain::Device)
            .then(|| {
                UiPresentationBackendGeneration::new(backend.instance_id(), backend.device_epoch())
            })
            .flatten()
    });
    #[cfg(feature = "ui-submitter")]
    if let Some(ui_submission_state) = ui_submission_state {
        packet_slot.finish_ui(
            ui_submission_state,
            result.clone(),
            frame.state,
            device_fault_generation,
            status
                .last_skip()
                .and_then(|skip| skip.message())
                .map(str::to_owned),
        );
    }
    if let Err(error) = result {
        handle_wgpu_render_error(
            &mut backend,
            error,
            frame.index,
            &mut status,
            faults.as_deref(),
        );
        frame.mark_skipped();
    }
}

#[cfg(feature = "ui-submitter")]
fn complete_wgpu_ui_presentation(
    mut packet_slot: ResMut<WgpuFramePacketSlot>,
    mut outcomes: ResMut<UiPresentationOutcomes>,
) {
    packet_slot.complete_ui(&mut outcomes);
}

fn handle_wgpu_render_error(
    backend: &mut WgpuRenderBackend,
    error: WgpuRenderError,
    frame_index: u64,
    status: &mut RenderBackendStatus,
    faults: Option<&RuntimeFaultReporter>,
) {
    if error.is_attempt_fault() {
        status.clear_skip();
        status.mark_skipped_with_message(
            frame_index,
            RenderFrameSkipReason::InvalidTopology,
            error.to_string(),
        );
        if let Some(faults) = faults {
            faults.report(RuntimeFault::engine(
                RuntimeFaultKind::System,
                "nara.render.frame-packet",
            ));
        }
        return;
    }

    if error.is_deferred() {
        status.clear_skip();
        status.mark_skipped_with_message(
            frame_index,
            RenderFrameSkipReason::Deferred,
            error.to_string(),
        );
        return;
    }

    if error.is_content_rejection() {
        status.clear_skip();
        status.mark_skipped_with_message(
            frame_index,
            RenderFrameSkipReason::ContentRejected,
            error.to_string(),
        );
        return;
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
        return;
    }

    let message = backend.mark_error(&error);
    status.mark_unavailable(WGPU_RENDER_BACKEND, message.clone());
    status.mark_skipped_with_message(frame_index, RenderFrameSkipReason::BackendError, message);
    if let Some(faults) = faults {
        faults.report(RuntimeFault::engine(
            RuntimeFaultKind::RequiredService,
            "nara.render-wgpu.frame",
        ));
    }
}

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
fn required_alpha_modes(batches: &[WgpuQuadBatch]) -> Vec<AlphaMode2d> {
    let mut modes = Vec::new();
    for mode in batches
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .map(|batch| batch.material.alpha_mode)
    {
        if !modes.contains(&mode) {
            modes.push(mode);
        }
    }
    modes
}

fn render_acquired_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    window_id: WindowId,
    surface_state: &WgpuSurfaceState,
    surface_texture: wgpu::SurfaceTexture,
    viewport: nara_render::ViewportRect,
    clear_color: Color,
    draw: &PreparedSubmitterDraw,
) -> Result<(), WgpuRenderError> {
    #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
    let _ = viewport;
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
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nara_wgpu_surface_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color_to_wgpu(clear_color)),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        pass.set_viewport(
            viewport.physical_x as f32,
            viewport.physical_y as f32,
            viewport.physical_width as f32,
            viewport.physical_height as f32,
            0.0,
            1.0,
        );
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        draw_quad_batch_buffers(&mut pass, &draw.pipelines, &draw.buffers, viewport);
        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        let _ = (&mut pass, draw);
    }
    queue.submit([encoder.finish()]);
    queue.present(surface_texture);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "sprite-submitter")]
    use nara_ecs::Entity;
    use nara_ecs::{
        Commands,
        system::{IntoSystem, System},
    };

    fn admit_runtime(app: App) -> nara_app::RuntimeCandidate {
        nara_app::RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                app.seal().unwrap(),
                nara_app::RuntimeObligationLedger::new(),
                nara_app::RuntimeClosePolicy::default(),
            )
            .unwrap()
    }

    fn install_render_foundation(app: &mut App, with_window: bool) {
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            nara_transform::TransformPlugin,
            nara_render::RenderPlugin,
        ))
        .unwrap();
        if with_window {
            app.add_plugins(nara_window::WindowPlugin::default())
                .unwrap();
        }
    }

    fn install_test_plugins(app: &mut App, with_window: bool) {
        install_render_foundation(app, with_window);
        #[cfg(feature = "ui-submitter")]
        app.add_plugins((
            nara_image::ImagePreparePlugin,
            nara_ui_render::UiRenderPlugin,
        ))
        .unwrap();
        app.add_plugins(WgpuRenderPlugin).unwrap();
        #[cfg(feature = "ui-submitter")]
        app.add_plugins(WgpuUiSubmitterPlugin).unwrap();
    }

    #[cfg(feature = "sprite-submitter")]
    #[derive(Debug, Default, Resource)]
    struct UninstalledSpriteContributionObservation {
        source_instances: usize,
        collected_instances: Option<usize>,
    }

    #[cfg(feature = "sprite-submitter")]
    fn inject_sprite_domain_work(
        mut views: ResMut<ExtractedViews>,
        mut sprites: ResMut<nara_sprite_render::ExtractedSprites>,
    ) {
        views.clear();
        views.push(nara_render::ExtractedView {
            camera_entity: Entity::PLACEHOLDER,
            target: nara_render::RenderTarget::PrimaryWindow,
            viewport: nara_render::ViewportRect::new(0, 0, 16, 16).unwrap(),
            world_position: nara_core::Vec2::ZERO,
            viewport_height: 16.0,
            order: 0,
            clear_color: nara_core::Color::BLACK,
        });
        sprites.clear();
        sprites.push(nara_sprite_render::ExtractedSprite {
            entity: Entity::PLACEHOLDER,
            source_order: 0,
            kind: nara_sprite_render::ExtractedSpriteKind::Sprite,
            material: nara_sprite_render::ExtractedSpriteMaterial::from_color(
                nara_core::Color::WHITE,
            ),
            texture_region: nara_sprite_render::TextureUvRect::FULL,
            world_center: nara_core::Vec2::ZERO,
            world_x_axis: nara_core::Vec2::X,
            world_y_axis: nara_core::Vec2::Y,
            color: nara_core::Color::WHITE,
            layer: 0,
            sort_key: 0,
        });
    }

    #[cfg(feature = "sprite-submitter")]
    fn observe_uninstalled_sprite_contribution(
        batches: Res<nara_sprite_render::SpriteBatches>,
        slot: Res<WgpuFramePacketSlot>,
        mut observation: ResMut<UninstalledSpriteContributionObservation>,
        mut views: ResMut<ExtractedViews>,
    ) {
        observation.source_instances = batches.total_instances();
        observation.collected_instances = slot.collected_sprite_instances();
        views.clear();
    }

    #[cfg(feature = "ui-submitter")]
    #[derive(Debug, Default, Resource)]
    struct UninstalledUiContributionObservation {
        packet_count: usize,
        submission_count: usize,
        submissions: Vec<UiBackendSubmission>,
        outcomes: Vec<UiPresentationOutcome>,
    }

    #[cfg(feature = "ui-submitter")]
    fn observe_undrained_ui_backend_frame(
        mut reader: UiBackendSubmissionReader,
        mut observation: ResMut<UninstalledUiContributionObservation>,
    ) {
        let frame = reader
            .take()
            .expect("the uninstalled contribution must leave the backend frame available");
        let (packets, submissions) = frame.into_parts();
        observation.packet_count = packets.len();
        observation.submission_count = submissions.len();
        observation.submissions = submissions;
    }

    #[cfg(feature = "ui-submitter")]
    fn reject_observed_ui_backend_frame(
        mut observation: ResMut<UninstalledUiContributionObservation>,
        mut outcomes: ResMut<UiPresentationOutcomes>,
    ) {
        for submission in std::mem::take(&mut observation.submissions) {
            outcomes.commit_backend(submission.content_rejected(
                "the isolation test deliberately rejected the undrained frame".to_owned(),
            ));
        }
    }

    #[cfg(feature = "ui-submitter")]
    fn record_ui_isolation_outcomes(world: &mut nara_ecs::World) {
        let outcomes = nara_ui_render::drain_ui_presentation_outcomes(world)
            .expect("the test-owned rejection must release every backend permit");
        world
            .resource_mut::<UninstalledUiContributionObservation>()
            .outcomes = outcomes;
    }

    #[derive(Debug, Default, Resource)]
    struct SecondCameraInjection(bool);

    fn inject_second_camera_after_capture(
        mut commands: Commands,
        mut injection: ResMut<SecondCameraInjection>,
    ) {
        if injection.0 {
            return;
        }
        injection.0 = true;
        commands.spawn((
            nara_render::Camera2d::default(),
            nara_transform::Transform2d::default(),
        ));
    }

    #[test]
    fn plugin_installs_backend_and_render_resources() {
        let mut world = nara_ecs::World::new();
        world.init_resource::<WgpuRenderBackend>();
        assert!(world.contains_resource::<WgpuRenderBackend>());

        let mut app = App::new();
        install_test_plugins(&mut app, false);

        assert!(app.world().contains_resource::<WgpuRenderBackend>());
        assert!(app.world().contains_resource::<ExtractedViews>());
        assert!(app.world().contains_resource::<RenderFrame>());
        assert!(app.world().contains_resource::<FrameStats>());
        assert!(app.world().contains_resource::<RenderBackendStatus>());
        assert_eq!(
            app.world()
                .resource::<WindowSurfaceRetirementDriver>()
                .driver(),
            WGPU_RENDER_BACKEND
        );
    }

    #[cfg(feature = "sprite-submitter")]
    #[test]
    fn compiled_but_uninstalled_sprite_contribution_does_not_collect_domain_batches() {
        let mut app = App::new();
        install_render_foundation(&mut app, false);
        app.add_plugins((
            nara_image::ImagePreparePlugin,
            nara_sprite_render::SpriteRenderPlugin,
        ))
        .unwrap();
        app.add_plugins(WgpuRenderPlugin).unwrap();
        app.insert_resource(UninstalledSpriteContributionObservation::default())
            .unwrap();
        app.add_systems(CoreStage::Prepare, inject_sprite_domain_work)
            .unwrap();
        app.add_systems(
            CoreStage::Render,
            observe_uninstalled_sprite_contribution
                .after(frame::WgpuFrameSet::CollectSprite)
                .before(frame::WgpuFrameSet::Seal),
        )
        .unwrap();
        let candidate = admit_runtime(app);
        let mut runtime = candidate.complete_startup().unwrap().promote();

        runtime.drive(std::time::Duration::ZERO).unwrap();

        let observation = runtime
            .world()
            .resource::<UninstalledSpriteContributionObservation>();
        assert_eq!(observation.source_instances, 1);
        assert_eq!(observation.collected_instances, None);
        let stats = runtime.world().resource::<FrameStats>();
        assert_eq!(stats.draw_calls, 0);
        assert_eq!(stats.sprites, 0);
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn compiled_but_uninstalled_ui_contribution_does_not_drain_the_backend_frame() {
        use nara_ui_kernel::UiSurfaceId;
        use nara_ui_render::__private::{
            UiPresentationRoute, replace_ui_backend_frame, replace_ui_presentation_routes,
            snapshot_ui_paint_packets,
        };

        let mut app = App::new();
        install_render_foundation(&mut app, false);
        app.add_plugins((
            nara_image::ImagePreparePlugin,
            nara_ui_render::UiRenderPlugin,
        ))
        .unwrap();
        app.add_plugins(WgpuRenderPlugin).unwrap();
        app.insert_resource(UninstalledUiContributionObservation::default())
            .unwrap();
        app.add_systems(
            CoreStage::Render,
            (
                observe_undrained_ui_backend_frame,
                reject_observed_ui_backend_frame,
                record_ui_isolation_outcomes,
            )
                .chain()
                .after(frame::WgpuFrameSet::Submit),
        )
        .unwrap();

        let surface_id = UiSurfaceId::new(1).unwrap();
        let world = app.world_mut().unwrap();
        replace_ui_presentation_routes(
            world,
            [UiPresentationRoute::new(surface_id, WindowId::PRIMARY).unwrap()],
        )
        .unwrap();
        nara_ui_render::compile_ui_paint_packets(world, &[ui_outcome_frame()]).unwrap();
        let (mut planned, _) = snapshot_ui_paint_packets(world).unwrap().into_parts();
        let packet = planned
            .pop()
            .expect("the planner must publish one exact backend packet");
        assert!(
            replace_ui_backend_frame(
                world,
                vec![Arc::clone(&packet)],
                vec![prepared_ui_attempt(packet, 61, 41)],
            )
            .unwrap()
            .is_empty()
        );

        let candidate = admit_runtime(app);
        let mut runtime = candidate.complete_startup().unwrap().promote();
        runtime.drive(std::time::Duration::ZERO).unwrap();

        let observation = runtime
            .world()
            .resource::<UninstalledUiContributionObservation>();
        assert_eq!(observation.packet_count, 1);
        assert_eq!(observation.submission_count, 1);
        assert!(observation.submissions.is_empty());
        assert!(matches!(
            observation.outcomes.as_slice(),
            [UiPresentationOutcome::ContentRejected { attempt, .. }]
                if attempt.admission_id().is_some_and(|id| id.get() == 61)
        ));
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn blocked_ui_collection_keeps_the_existing_transaction_and_next_frame_available() {
        use nara_ui_kernel::UiSurfaceId;
        use nara_ui_render::__private::{
            UiPresentationRoute, replace_ui_backend_frame, replace_ui_presentation_routes,
            snapshot_ui_paint_packets,
        };

        let mut app = App::new();
        install_render_foundation(&mut app, false);
        app.add_plugins((
            nara_image::ImagePreparePlugin,
            nara_ui_render::UiRenderPlugin,
        ))
        .unwrap();
        app.add_plugins(WgpuRenderPlugin).unwrap();

        let world = app.world_mut().unwrap();
        let surface_id = UiSurfaceId::new(1).unwrap();
        replace_ui_presentation_routes(
            world,
            [UiPresentationRoute::new(surface_id, WindowId::PRIMARY).unwrap()],
        )
        .unwrap();
        nara_ui_render::compile_ui_paint_packets(world, &[ui_outcome_frame()]).unwrap();
        let (mut planned, _) = snapshot_ui_paint_packets(world).unwrap().into_parts();
        let packet = planned.pop().unwrap();
        replace_ui_backend_frame(
            world,
            vec![Arc::clone(&packet)],
            vec![prepared_ui_attempt(packet, 71, 51)],
        )
        .unwrap();
        let existing_submission = {
            let mut outcomes = world.resource_mut::<UiPresentationOutcomes>();
            admit_ui_attempt(
                &mut outcomes,
                prepared_ui_attempt(ui_outcome_packet(), 70, 50),
            )
        };
        world
            .resource_mut::<WgpuFramePacketSlot>()
            .collect_ui(Vec::new(), vec![existing_submission]);

        let mut collect = IntoSystem::into_system(collect_wgpu_ui_payload);
        collect.initialize(world);
        collect.run((), world).unwrap();
        assert_eq!(
            nara_ui_render::drain_ui_presentation_outcomes(world).unwrap_err(),
            nara_ui_render::UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
                count: 1,
            },
            "blocked collection must preserve the existing outcome reservation",
        );

        let existing = world
            .resource_mut::<WgpuFramePacketSlot>()
            .take_ui_submissions()
            .unwrap();
        let WgpuUiSubmissionState::Prepared(existing) = existing else {
            panic!("the blocked transaction must retain its prepared state");
        };
        {
            let mut outcomes = world.resource_mut::<UiPresentationOutcomes>();
            for submission in existing {
                outcomes.commit_backend(
                    submission.content_rejected("test transaction retired".to_owned()),
                );
            }
        }
        let retired = nara_ui_render::drain_ui_presentation_outcomes(world).unwrap();
        assert!(matches!(
            retired.as_slice(),
            [UiPresentationOutcome::ContentRejected { attempt, .. }]
                if attempt.admission_id().is_some_and(|id| id.get() == 70)
        ));

        fn take_backend_frame(
            mut reader: UiBackendSubmissionReader,
        ) -> Result<
            nara_ui_render::__private::UiBackendPresentationFrame,
            nara_ui_render::UiPresentationIntegrationError,
        > {
            reader.take()
        }

        let mut take = IntoSystem::into_system(take_backend_frame);
        take.initialize(world);
        let next = take
            .run((), world)
            .unwrap()
            .expect("blocked collection must leave the next backend frame available");
        let (packets, submissions) = next.into_parts();
        assert_eq!(packets.len(), 1);
        assert!(matches!(
            submissions.as_slice(),
            [submission]
                if submission.submission().identity().admission_id().get() == 71
        ));
        {
            let mut outcomes = world.resource_mut::<UiPresentationOutcomes>();
            for submission in submissions {
                outcomes
                    .commit_backend(submission.content_rejected("test frame retired".to_owned()));
            }
        }
        let retired = nara_ui_render::drain_ui_presentation_outcomes(world).unwrap();
        assert!(matches!(
            retired.as_slice(),
            [UiPresentationOutcome::ContentRejected { attempt, .. }]
                if attempt.admission_id().is_some_and(|id| id.get() == 71)
        ));
    }

    #[test]
    fn plugin_installation_reconstructs_the_backend_instance_for_each_app() {
        fn backend_instance_id() -> u64 {
            let mut app = App::new();
            install_test_plugins(&mut app, false);
            app.world().resource::<WgpuRenderBackend>().instance_id()
        }

        assert_ne!(backend_instance_id(), backend_instance_id());
    }

    #[test]
    fn managed_frame_keeps_the_backend_terminal_state_after_render_schedule_completion() {
        let mut app = App::new();
        install_test_plugins(&mut app, false);
        let candidate = admit_runtime(app);
        let mut runtime = candidate.complete_startup().unwrap().promote();

        runtime.drive(std::time::Duration::ZERO).unwrap();

        assert_eq!(
            runtime.world().resource::<RenderFrame>().state,
            nara_render::RenderFrameState::Skipped,
            "the renderer's terminal state must not be overwritten by a second begin system",
        );
    }

    #[test]
    fn exact_fixed_step_does_not_require_app_and_render_frame_counters_to_match() {
        let mut app = App::new();
        install_test_plugins(&mut app, false);
        let candidate = admit_runtime(app);
        let mut runtime = candidate.complete_startup().unwrap().promote();

        runtime.drive(std::time::Duration::ZERO).unwrap();
        assert!(matches!(
            runtime.request_control(nara_app::RuntimeControl::Pause),
            nara_app::RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(std::time::Duration::ZERO).unwrap();
        assert!(matches!(
            runtime.request_control(nara_app::RuntimeControl::StepFixedTick),
            nara_app::RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(std::time::Duration::ZERO).unwrap();
        runtime.drive(std::time::Duration::ZERO).unwrap();

        let app_frame = runtime.world().resource::<FrameExecutionStart>().frame();
        let render_frame = runtime.world().resource::<RenderFrame>().index;
        assert_eq!(app_frame, render_frame + 1);
        assert_eq!(runtime.state(), RuntimeState::Paused);
        assert_eq!(runtime.fault(), None);
    }

    #[test]
    fn captured_topology_is_immutable_and_next_frame_rejects_before_surface_work() {
        let mut app = App::new();
        install_test_plugins(&mut app, true);
        app.insert_resource(SecondCameraInjection::default())
            .unwrap();
        app.world_mut().unwrap().spawn((
            nara_render::Camera2d::default(),
            nara_transform::Transform2d::default(),
        ));
        app.add_systems(
            CoreStage::Render,
            inject_second_camera_after_capture
                .after(capture_wgpu_frame_packet)
                .before(render_wgpu_surfaces),
        )
        .unwrap();
        let candidate = admit_runtime(app);
        let mut runtime = candidate.complete_startup().unwrap().promote();

        runtime.drive(std::time::Duration::ZERO).unwrap();
        let first_frame = *runtime.world().resource::<RenderFrame>();
        let first = runtime
            .world()
            .resource::<WgpuRenderBackend>()
            .frame_transaction_stats();
        assert_eq!(
            first.render_transaction_frame_index(),
            Some(first_frame.index)
        );
        assert_eq!(first.packet_admissions(), 1);
        assert_eq!(first.packet_rejections(), 0);
        assert_eq!(first.surface_acquire_attempts(), 0);
        assert_eq!(first.surface_acquires(), 0);
        assert_eq!(first.queue_submissions(), 0);
        assert_eq!(first.presents(), 0);

        let failure = runtime.drive(std::time::Duration::ZERO).unwrap_err();
        assert_eq!(failure.fault().kind(), RuntimeFaultKind::System);
        assert_eq!(failure.fault().source(), "nara.render.frame-packet");
        let rejected_frame = *runtime.world().resource::<RenderFrame>();
        let rejected = runtime
            .world()
            .resource::<WgpuRenderBackend>()
            .frame_transaction_stats();
        assert_eq!(
            rejected.render_transaction_frame_index(),
            Some(rejected_frame.index)
        );
        assert_eq!(rejected.packet_admissions(), 0);
        assert_eq!(rejected.packet_rejections(), 1);
        assert_eq!(rejected.surface_acquire_attempts(), 0);
        assert_eq!(rejected.surface_acquires(), 0);
        assert_eq!(rejected.queue_submissions(), 0);
        assert_eq!(rejected.presents(), 0);
    }

    #[test]
    fn retirement_driver_accepts_an_already_dropped_backend_resource() {
        let mut app = App::new();
        install_test_plugins(&mut app, false);
        let retirement_driver = *app.world().resource::<WindowSurfaceRetirementDriver>();
        assert!(
            app.world_mut()
                .unwrap()
                .remove_resource::<WgpuRenderBackend>()
                .is_some()
        );
        let candidate = admit_runtime(app);
        let mut runtime = candidate.complete_startup().unwrap().promote();

        runtime
            .with_driver_scope(|scope| {
                retirement_driver.retire_targets(scope, &[WindowId::PRIMARY])
            })
            .unwrap()
            .unwrap();
    }

    #[test]
    fn surface_policy_distinguishes_resize_reconfigure_and_loss() {
        let current = Extent2d::new(320, 180).unwrap();
        let resized = Extent2d::new(640, 360).unwrap();

        assert_eq!(
            surface_resize_action(
                current,
                Extent2d {
                    width: 0,
                    height: 180,
                },
            ),
            SurfaceResizeAction::SkipZeroSized
        );
        assert_eq!(
            surface_resize_action(current, current),
            SurfaceResizeAction::Unchanged
        );
        assert_eq!(
            surface_resize_action(current, resized),
            SurfaceResizeAction::Reconfigure(resized)
        );
        assert_eq!(
            surface_acquire_policy(SurfaceTextureStatus::Outdated),
            SurfaceAcquireAction::Reconfigure
        );
        assert_eq!(
            surface_acquire_policy(SurfaceTextureStatus::Lost),
            SurfaceAcquireAction::RecreateSurface
        );
    }

    #[test]
    fn configured_present_mode_reports_the_exact_wgpu_mode() {
        let cases = [
            (
                wgpu::PresentMode::AutoVsync,
                WgpuConfiguredPresentMode::AutoVsync,
                "auto-vsync",
            ),
            (
                wgpu::PresentMode::AutoNoVsync,
                WgpuConfiguredPresentMode::AutoNoVsync,
                "auto-no-vsync",
            ),
            (
                wgpu::PresentMode::Fifo,
                WgpuConfiguredPresentMode::Fifo,
                "fifo",
            ),
            (
                wgpu::PresentMode::FifoRelaxed,
                WgpuConfiguredPresentMode::FifoRelaxed,
                "fifo-relaxed",
            ),
            (
                wgpu::PresentMode::Immediate,
                WgpuConfiguredPresentMode::Immediate,
                "immediate",
            ),
            (
                wgpu::PresentMode::Mailbox,
                WgpuConfiguredPresentMode::Mailbox,
                "mailbox",
            ),
        ];

        for (wgpu_mode, expected, label) in cases {
            let configured = WgpuConfiguredPresentMode::from_wgpu(wgpu_mode);
            assert_eq!(configured, expected);
            assert_eq!(configured.as_str(), label);
        }
    }

    #[test]
    fn render_system_is_pinned_to_the_main_thread_executor() {
        let mut system = IntoSystem::into_system(render_wgpu_surfaces);
        system.initialize(&mut nara_ecs::World::new());

        assert!(!system.is_send());
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[test]
    fn enabled_submitter_input_has_no_quad_work() {
        assert!(
            frame::WgpuFramePayload::default()
                .lower_draw(
                    RenderTarget::PrimaryWindow,
                    ViewportRect::new(0, 0, 1, 1).unwrap(),
                )
                .unwrap()
                .batches
                .is_empty()
        );
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[test]
    fn closed_first_party_plan_orders_content_before_ui() {
        let content = WgpuQuadBatch {
            material: crate::quad::WgpuQuadMaterialKey {
                texture: crate::quad::WgpuQuadTextureKey::Fallback,
                sampler: nara_material::SamplerDescriptor::default(),
                alpha_mode: nara_material::AlphaMode2d::Blend,
            },
            texture: crate::quad::WgpuQuadTexture::Fallback,
            scissor: None,
            instances: Vec::new(),
            counts_as_sprites: true,
        };
        let ui = WgpuQuadBatch {
            counts_as_sprites: false,
            ..content.clone()
        };

        let ordered = nara_render::__private::assemble_first_party_draws(vec![content], vec![ui]);

        assert!(ordered[0].counts_as_sprites);
        assert!(!ordered[1].counts_as_sprites);
    }

    #[test]
    fn upload_deferral_skips_one_frame_without_poisoning_the_backend() {
        let mut backend = WgpuRenderBackend::default();
        let initial_backend_state = backend.state();
        let mut status = RenderBackendStatus::default();
        status.mark_ready(WGPU_RENDER_BACKEND);

        handle_wgpu_render_error(
            &mut backend,
            WgpuRenderError::UploadDeferred {
                message: "glyph upload budget exhausted".to_owned(),
            },
            7,
            &mut status,
            None,
        );

        assert_eq!(backend.state(), initial_backend_state);
        assert_eq!(status.state(), RenderBackendState::Ready);
        assert_eq!(status.last_error(), None);
        let skip = status.last_skip().unwrap();
        assert_eq!(skip.frame_index(), 7);
        assert_eq!(skip.reason(), RenderFrameSkipReason::Deferred);
    }

    #[test]
    fn rejected_content_skips_one_frame_without_poisoning_the_backend() {
        let mut backend = WgpuRenderBackend::default();
        let initial_backend_state = backend.state();
        let mut status = RenderBackendStatus::default();
        status.mark_ready(WGPU_RENDER_BACKEND);

        handle_wgpu_render_error(
            &mut backend,
            WgpuRenderError::ContentRejected {
                message: "quad instance capacity exceeded".to_owned(),
            },
            11,
            &mut status,
            None,
        );

        assert_eq!(backend.state(), initial_backend_state);
        assert_eq!(status.state(), RenderBackendState::Ready);
        assert_eq!(status.last_error(), None);
        let skip = status.last_skip().unwrap();
        assert_eq!(skip.frame_index(), 11);
        assert_eq!(skip.reason(), RenderFrameSkipReason::ContentRejected);
    }

    #[test]
    fn target_fault_does_not_retire_the_backend_or_device_generation() {
        let mut backend = WgpuRenderBackend::default();
        let instance_id = backend.instance_id();
        let device_epoch = backend.device_epoch();
        let initial_state = backend.state();
        let mut status = RenderBackendStatus::default();
        status.mark_ready(WGPU_RENDER_BACKEND);

        handle_wgpu_render_error(
            &mut backend,
            WgpuRenderError::SurfaceMissing {
                window_id: WindowId::PRIMARY,
            },
            12,
            &mut status,
            None,
        );

        assert_eq!(backend.instance_id(), instance_id);
        assert_eq!(backend.device_epoch(), device_epoch);
        assert_eq!(backend.state(), initial_state);
        assert_eq!(status.last_error(), None);
        assert_eq!(
            status.last_skip().unwrap().reason(),
            RenderFrameSkipReason::SurfaceUnavailable
        );
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn impossible_ui_packet_state_is_scoped_to_the_attempt() {
        let mut backend = WgpuRenderBackend::default();
        let mut status = RenderBackendStatus::default();
        status.mark_ready(WGPU_RENDER_BACKEND);

        handle_wgpu_render_error(
            &mut backend,
            WgpuRenderError::UiPacketInvariant {
                message: "missing planner-owned image index".to_owned(),
            },
            12,
            &mut status,
            None,
        );

        assert_eq!(backend.state(), WgpuBackendState::Uninitialized);
        assert_eq!(status.state(), RenderBackendState::Ready);
        assert_eq!(status.last_error(), None);
        let skip = status.last_skip().unwrap();
        assert_eq!(skip.frame_index(), 12);
        assert_eq!(skip.reason(), RenderFrameSkipReason::InvalidTopology);
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn ui_outcomes_bind_submission_and_retry_to_the_exact_backend_generation() {
        let packet = ui_outcome_packet();
        let backend_generation = UiPresentationBackendGeneration::new(17, 3).unwrap();
        let mut outcomes = UiPresentationOutcomes::default();
        let submitted = submitted_ui_attempt(
            &mut outcomes,
            Arc::clone(&packet),
            41,
            29,
            backend_generation,
        );

        frame::publish_ui_outcomes(
            &mut outcomes,
            WgpuUiSubmissionState::Submitted(vec![submitted]),
            &Ok(()),
            RenderFrameState::Submitted,
            None,
            None,
        );
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        let UiPresentationOutcome::Presented(commit) = published.remove(0) else {
            panic!("a submitted packet must issue one presentation commit");
        };
        assert_eq!(commit.attempt().admission_id().unwrap().get(), 41);
        assert_eq!(commit.presentation_epoch().get(), 29);
        assert_eq!(commit.backend_generation(), backend_generation);
        let accepted = nara_ui_render::__private::discard_ui_backend_acceptance(commit);
        assert!(Arc::ptr_eq(accepted.packet(), &packet));

        let mut outcomes = UiPresentationOutcomes::default();
        let deferred = Err(WgpuRenderError::UploadDeferred {
            message: "shared upload budget exhausted".to_owned(),
        });
        let submitted = submitted_ui_attempt(
            &mut outcomes,
            Arc::clone(&packet),
            42,
            30,
            backend_generation,
        );
        frame::publish_ui_outcomes(
            &mut outcomes,
            WgpuUiSubmissionState::Submitted(vec![submitted]),
            &deferred,
            RenderFrameState::Skipped,
            None,
            None,
        );
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        let UiPresentationOutcome::Deferred { retry, .. } = published.remove(0) else {
            panic!("retryable upload work must retain an exact retry identity");
        };
        assert_eq!(retry.attempt().admission_id().unwrap().get(), 42);
        assert_eq!(retry.backend_generation(), backend_generation);
        let retry = nara_ui_render::__private::retry_deferred(retry);
        assert_eq!(
            retry.required_backend_generation(),
            Some(backend_generation)
        );
        let mut retry_outcomes = UiPresentationOutcomes::default();
        let mut retry_state =
            WgpuUiSubmissionState::Prepared(vec![admit_ui_attempt(&mut retry_outcomes, retry)]);
        assert!(matches!(
            retry_state.bind_backend(UiPresentationBackendGeneration::new(17, 4).unwrap()),
            Err(WgpuRenderError::UiSubmissionRejected { .. })
        ));
        retry_state
            .bind_backend(backend_generation)
            .expect("a rejected generation bind must retain the exact retry authority");
        assert!(matches!(retry_state, WgpuUiSubmissionState::Submitted(_)));

        let mut outcomes = UiPresentationOutcomes::default();
        let prepared = prepared_ui_attempt(packet, 43, 31);
        let prepared = admit_ui_attempt(&mut outcomes, prepared);
        frame::publish_ui_outcomes(
            &mut outcomes,
            WgpuUiSubmissionState::Prepared(vec![prepared]),
            &Ok(()),
            RenderFrameState::Skipped,
            None,
            Some("the surface is unavailable"),
        );
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert!(matches!(
            published.remove(0),
            UiPresentationOutcome::BackendFault {
                backend_generation: None,
                domain: UiPresentationBackendFaultDomain::Target,
                ..
            }
        ));
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn ui_outcomes_preserve_exact_fault_domains_for_prepared_and_submitted_attempts() {
        let packet = ui_outcome_packet();
        let backend_generation = UiPresentationBackendGeneration::new(17, 3).unwrap();

        let mut outcomes = UiPresentationOutcomes::default();
        let prepared = admit_ui_attempt(
            &mut outcomes,
            prepared_ui_attempt(Arc::clone(&packet), 44, 32),
        );
        frame::publish_ui_outcomes(
            &mut outcomes,
            WgpuUiSubmissionState::Prepared(vec![prepared]),
            &Err(WgpuRenderError::ContentRejected {
                message: "invalid content".to_owned(),
            }),
            RenderFrameState::Skipped,
            None,
            None,
        );
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert!(matches!(
            published.remove(0),
            UiPresentationOutcome::ContentRejected { attempt, .. }
                if attempt.admission_id().is_some_and(|id| id.get() == 44)
        ));
        assert!(published.is_empty());

        let mut outcomes = UiPresentationOutcomes::default();
        let prepared = admit_ui_attempt(
            &mut outcomes,
            prepared_ui_attempt(Arc::clone(&packet), 45, 33),
        );
        frame::publish_ui_outcomes(
            &mut outcomes,
            WgpuUiSubmissionState::Prepared(vec![prepared]),
            &Err(WgpuRenderError::DeviceLost),
            RenderFrameState::Skipped,
            Some(backend_generation),
            None,
        );
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert!(matches!(
            published.remove(0),
            UiPresentationOutcome::BackendFault {
                attempt: Some(attempt),
                backend_generation: Some(generation),
                domain: UiPresentationBackendFaultDomain::Device,
                ..
            } if attempt.admission_id().is_some_and(|id| id.get() == 45)
                && generation == backend_generation
        ));
        assert!(published.is_empty());

        let mut outcomes = UiPresentationOutcomes::default();
        let submitted = submitted_ui_attempt(
            &mut outcomes,
            Arc::clone(&packet),
            46,
            34,
            backend_generation,
        );
        frame::publish_ui_outcomes(
            &mut outcomes,
            WgpuUiSubmissionState::Submitted(vec![submitted]),
            &Err(WgpuRenderError::SurfaceMissing {
                window_id: WindowId::PRIMARY,
            }),
            RenderFrameState::Skipped,
            None,
            None,
        );
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert!(matches!(
            published.remove(0),
            UiPresentationOutcome::BackendFault {
                attempt: Some(attempt),
                backend_generation: Some(generation),
                domain: UiPresentationBackendFaultDomain::Target,
                ..
            } if attempt.admission_id().is_some_and(|id| id.get() == 46)
                && generation == backend_generation
        ));
        assert!(published.is_empty());

        let mut outcomes = UiPresentationOutcomes::default();
        let submitted = submitted_ui_attempt(
            &mut outcomes,
            Arc::clone(&packet),
            47,
            35,
            backend_generation,
        );
        frame::publish_ui_outcomes(
            &mut outcomes,
            WgpuUiSubmissionState::Submitted(vec![submitted]),
            &Err(WgpuRenderError::ContentRejected {
                message: "late content rejection".to_owned(),
            }),
            RenderFrameState::Skipped,
            None,
            None,
        );
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert!(matches!(
            published.remove(0),
            UiPresentationOutcome::BackendFault {
                attempt: Some(attempt),
                backend_generation: Some(generation),
                domain: UiPresentationBackendFaultDomain::Attempt,
                ..
            } if attempt.admission_id().is_some_and(|id| id.get() == 47)
                && generation == backend_generation
        ));
        assert!(published.is_empty());

        let mut outcomes = UiPresentationOutcomes::default();
        let prepared = admit_ui_attempt(
            &mut outcomes,
            prepared_ui_attempt(Arc::clone(&packet), 48, 36),
        );
        frame::publish_ui_outcomes(
            &mut outcomes,
            WgpuUiSubmissionState::Prepared(vec![prepared]),
            &Err(WgpuRenderError::UiSubmissionRejected {
                message: "prepared submission mismatch".to_owned(),
            }),
            RenderFrameState::Skipped,
            Some(backend_generation),
            None,
        );
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert!(matches!(
            published.remove(0),
            UiPresentationOutcome::BackendFault {
                attempt: Some(attempt),
                backend_generation: None,
                domain: UiPresentationBackendFaultDomain::Attempt,
                ..
            } if attempt.admission_id().is_some_and(|id| id.get() == 48)
        ));
        assert!(published.is_empty());

        let mut outcomes = UiPresentationOutcomes::default();
        let submitted = submitted_ui_attempt(
            &mut outcomes,
            Arc::clone(&packet),
            49,
            37,
            backend_generation,
        );
        frame::publish_ui_outcomes(
            &mut outcomes,
            WgpuUiSubmissionState::Submitted(vec![submitted]),
            &Ok(()),
            RenderFrameState::Skipped,
            None,
            Some("surface submit was skipped"),
        );
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert!(matches!(
            published.remove(0),
            UiPresentationOutcome::BackendFault {
                attempt: Some(attempt),
                backend_generation: Some(generation),
                domain: UiPresentationBackendFaultDomain::Target,
                ..
            } if attempt.admission_id().is_some_and(|id| id.get() == 49)
                && generation == backend_generation
        ));
        assert!(published.is_empty());

        let mut outcomes = UiPresentationOutcomes::default();
        let submitted = submitted_ui_attempt(&mut outcomes, packet, 50, 38, backend_generation);
        frame::publish_ui_outcomes(
            &mut outcomes,
            WgpuUiSubmissionState::Submitted(vec![submitted]),
            &Err(WgpuRenderError::BackendUnavailable),
            RenderFrameState::Skipped,
            None,
            None,
        );
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert!(matches!(
            published.remove(0),
            UiPresentationOutcome::BackendFault {
                attempt: Some(attempt),
                backend_generation: Some(generation),
                domain: UiPresentationBackendFaultDomain::Device,
                ..
            } if attempt.admission_id().is_some_and(|id| id.get() == 50)
                && generation == backend_generation
        ));
        assert!(published.is_empty());
    }

    #[cfg(feature = "ui-submitter")]
    fn prepared_ui_attempt(
        packet: Arc<UiPaintPacket>,
        admission_id: u64,
        presentation_epoch: u64,
    ) -> UiPreparedBackendSubmission {
        nara_ui_render::__private::prepare_backend_submission(
            UiPresentationAttemptId::new(admission_id).unwrap(),
            packet,
            UiPresentationEpoch::new(presentation_epoch).unwrap(),
            UiPresentationRuntimeGeneration::new(7).unwrap(),
            UiPresentationCapabilityGeneration::new(11).unwrap(),
        )
    }

    #[cfg(feature = "ui-submitter")]
    fn submitted_ui_attempt(
        outcomes: &mut UiPresentationOutcomes,
        packet: Arc<UiPaintPacket>,
        admission_id: u64,
        presentation_epoch: u64,
        backend_generation: UiPresentationBackendGeneration,
    ) -> UiSubmittedBackendSubmission {
        let admitted = admit_ui_attempt(
            outcomes,
            prepared_ui_attempt(packet, admission_id, presentation_epoch),
        );
        nara_ui_render::__private::bind_backend_submission_batch(vec![admitted], backend_generation)
            .unwrap()
            .remove(0)
    }

    #[cfg(feature = "ui-submitter")]
    fn admit_ui_attempt(
        outcomes: &mut UiPresentationOutcomes,
        submission: UiPreparedBackendSubmission,
    ) -> UiBackendSubmission {
        let mut permits = outcomes
            .reserve_backend(std::slice::from_ref(&submission))
            .unwrap();
        UiBackendSubmission::pair(submission, permits.remove(0)).unwrap()
    }

    #[cfg(feature = "ui-submitter")]
    fn ui_outcome_frame() -> Arc<nara_ui_kernel::UiFrame> {
        use nara_text::bundled_font_face;
        use nara_ui_kernel::{
            UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiLayer, UiLocalKey,
            UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiText, UiTheme, UiVisual,
        };

        let snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime("wgpu-outcome", "root").unwrap(),
                    UiLayer::Base,
                    UiStyle::fill(),
                )
                .with_child(
                    UiNode::new(UiStyle::absolute(0.0, 0.0, 16.0, 16.0))
                        .with_key(UiLocalKey::runtime("wgpu-outcome", "node").unwrap())
                        .with_visual(UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)))
                        .with_text(UiText::new("UI", 12.0).unwrap()),
                ),
            )
            .build()
            .unwrap();
        let environment =
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 16.0, 16.0, 1.0, 1, 1, 1, 1).unwrap();
        UiCompiler::new(bundled_font_face().unwrap(), Default::default())
            .unwrap()
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap()
    }

    #[cfg(feature = "ui-submitter")]
    fn ui_outcome_packet_for(target: RenderTarget) -> Arc<UiPaintPacket> {
        nara_ui_render::UiPresentationPlanner::default()
            .plan(
                ui_outcome_frame(),
                target,
                &nara_ui_render::UiPreparedImages::default(),
            )
            .unwrap()
    }

    #[cfg(feature = "ui-submitter")]
    fn ui_outcome_packet() -> Arc<UiPaintPacket> {
        ui_outcome_packet_for(RenderTarget::Window(WindowId::PRIMARY))
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn payload_facts_bind_glyph_work_to_one_exact_ui_packet() {
        let packet = ui_outcome_packet();
        let identity = packet.identity();
        let payload = frame::WgpuFramePayload {
            primary_window_id: Some(WindowId::PRIMARY),
            #[cfg(feature = "sprite-submitter")]
            sprite_batches: None,
            ui_packets: vec![packet],
        };

        let facts = payload.ui_facts(RenderTarget::Window(WindowId::PRIMARY));

        assert_eq!(facts.packet_count, 1);
        assert_eq!(facts.packet_identity, Some(identity));
        assert!(facts.primitives > 0);
        assert!(facts.glyph_primitives > 0);
        assert!(facts.glyph_pages > 0);
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn payload_facts_follow_the_same_window_target_as_the_submitter() {
        let first_target = RenderTarget::Window(WindowId::new(101));
        let second_target = RenderTarget::Window(WindowId::new(102));
        let first_packet = ui_outcome_packet_for(first_target);
        let first_identity = first_packet.identity();
        let second_packet = ui_outcome_packet_for(second_target);
        let payload = frame::WgpuFramePayload {
            primary_window_id: Some(WindowId::PRIMARY),
            #[cfg(feature = "sprite-submitter")]
            sprite_batches: None,
            ui_packets: vec![first_packet, second_packet],
        };

        let facts = payload.ui_facts(first_target);

        assert_eq!(facts.packet_count, 1);
        assert_eq!(facts.packet_identity, Some(first_identity));
        assert!(facts.primitives > 0);
        assert!(facts.glyph_primitives > 0);
        assert!(facts.glyph_pages > 0);
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn submitted_packet_facts_preserve_distinct_app_and_render_identities() {
        let packet = ui_outcome_packet();
        let lowering = crate::ui::lower_ui_packets(
            std::slice::from_ref(&packet),
            RenderTarget::Window(WindowId::PRIMARY),
            Some(WindowId::PRIMARY),
            ViewportRect::new(0, 0, 16, 16).unwrap(),
        )
        .unwrap();
        let prepared = lowering.packet_facts;
        let mut outcomes = UiPresentationOutcomes::default();
        let backend_generation = UiPresentationBackendGeneration::new(77, 3).unwrap();
        let submitted = submitted_ui_attempt(
            &mut outcomes,
            Arc::clone(&packet),
            81,
            91,
            backend_generation,
        );
        let state = frame::WgpuUiSubmissionState::Submitted(vec![submitted]);
        let bound = state
            .bound_submissions()
            .first()
            .expect("one submitted attempt should remain available");
        let mut backend = WgpuRenderBackend::default();
        let facts = WgpuRenderBackend::prepare_submitted_ui_facts(
            state.bound_submissions(),
            &prepared,
            backend::WgpuSubmittedFrameFacts::new(10, 9, 4, 3, 4096, 2048),
        )
        .unwrap();
        backend.publish_submitted_ui_facts(facts);
        let facts = backend
            .submitted_ui_facts(bound.submission().identity(), backend_generation)
            .expect("the exact submitted packet should retain its first-pass lowering facts");

        assert_eq!(facts.submission(), bound.submission().identity());
        assert_eq!(facts.backend_generation(), backend_generation);
        assert_eq!(facts.app_frame_index(), 10);
        assert_eq!(facts.render_transaction_frame_index(), 9);
        assert!(facts.primitive_count() > 0);
        assert!(facts.glyph_primitive_count() > 0);
        assert!(facts.glyph_page_count() > 0);
        assert!(facts.submitted_quad_count() > 0);
        assert!(facts.submitted_glyph_quad_count() > 0);
        assert!(facts.referenced_texture_binding_count() > 0);
        assert_eq!(facts.frame_submitted_sprite_count(), 4);
        assert_eq!(facts.frame_submitted_image_sprite_count(), 3);
        assert_eq!(facts.submitted_texture_bytes(), 4096);
        assert_eq!(facts.submitted_instance_buffer_bytes(), 2048);
        assert_eq!(facts.submitted_total_bytes(), 6144);
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn submitted_facts_remain_bound_to_each_exact_submission() {
        let packet = ui_outcome_packet();
        let lowering = crate::ui::lower_ui_packets(
            std::slice::from_ref(&packet),
            RenderTarget::Window(WindowId::PRIMARY),
            Some(WindowId::PRIMARY),
            ViewportRect::new(0, 0, 16, 16).unwrap(),
        )
        .unwrap();
        let mut outcomes = UiPresentationOutcomes::default();
        let backend_generation = UiPresentationBackendGeneration::new(77, 3).unwrap();
        let prepared = vec![
            prepared_ui_attempt(Arc::clone(&packet), 81, 91),
            prepared_ui_attempt(Arc::clone(&packet), 82, 92),
        ];
        let mut permits = outcomes.reserve_backend(&prepared).unwrap();
        let admitted = prepared
            .into_iter()
            .zip(permits.drain(..))
            .map(|(submission, permit)| UiBackendSubmission::pair(submission, permit).unwrap())
            .collect::<Vec<_>>();
        let submitted =
            nara_ui_render::__private::bind_backend_submission_batch(admitted, backend_generation)
                .unwrap();
        let first_identity = submitted[0].submission().identity();
        let second_identity = submitted[1].submission().identity();
        let mut backend = WgpuRenderBackend::default();
        backend.begin_frame_transaction(9, Some(10));
        let facts = WgpuRenderBackend::prepare_submitted_ui_facts(
            &submitted,
            &lowering.packet_facts,
            backend::WgpuSubmittedFrameFacts::new(10, 9, 4, 3, 4096, 2048),
        )
        .unwrap();
        backend.publish_submitted_ui_facts(facts);

        let first = backend
            .submitted_ui_facts(first_identity, backend_generation)
            .expect("the earlier exact submission should remain queryable");
        let second = backend
            .submitted_ui_facts(second_identity, backend_generation)
            .expect("the later exact submission should remain queryable");

        assert_eq!(first.submission(), first_identity);
        assert_eq!(first.app_frame_index(), 10);
        assert_eq!(first.render_transaction_frame_index(), 9);
        assert_eq!(first.frame_submitted_sprite_count(), 4);
        assert_eq!(first.frame_submitted_image_sprite_count(), 3);
        assert_eq!(second.submission(), second_identity);
        assert_eq!(second.app_frame_index(), 10);
        assert_eq!(second.render_transaction_frame_index(), 9);
        assert_eq!(second.frame_submitted_sprite_count(), 4);
        assert_eq!(second.frame_submitted_image_sprite_count(), 3);

        let foreign_generation = UiPresentationBackendGeneration::new(77, 4).unwrap();
        assert!(
            backend
                .submitted_ui_facts(first_identity, foreign_generation)
                .is_none()
        );
        assert!(
            backend
                .submitted_ui_facts(second_identity, foreign_generation)
                .is_none()
        );
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn ui_submission_lowering_mismatch_is_rejected_before_submission() {
        let packet = ui_outcome_packet();
        let mut outcomes = UiPresentationOutcomes::default();
        let backend_generation = UiPresentationBackendGeneration::new(77, 3).unwrap();
        let submitted = submitted_ui_attempt(
            &mut outcomes,
            Arc::clone(&packet),
            81,
            91,
            backend_generation,
        );
        let submissions = vec![submitted];

        let error = WgpuRenderBackend::prepare_submitted_ui_facts(
            &submissions,
            &[],
            backend::WgpuSubmittedFrameFacts::new(10, 9, 4, 3, 4096, 2048),
        )
        .expect_err("a bound submission without an exact lowered packet must fail closed");

        assert!(matches!(
            error,
            WgpuRenderError::UiSubmissionRejected { .. }
        ));
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn duplicate_lowered_packet_facts_are_rejected_before_submission() {
        let packet = ui_outcome_packet();
        let lowering = crate::ui::lower_ui_packets(
            &[Arc::clone(&packet), Arc::clone(&packet)],
            RenderTarget::Window(WindowId::PRIMARY),
            Some(WindowId::PRIMARY),
            ViewportRect::new(0, 0, 16, 16).unwrap(),
        )
        .unwrap();
        let mut outcomes = UiPresentationOutcomes::default();
        let backend_generation = UiPresentationBackendGeneration::new(77, 3).unwrap();
        let submitted = submitted_ui_attempt(&mut outcomes, packet, 81, 91, backend_generation);
        let submissions = vec![submitted];

        let error = WgpuRenderBackend::prepare_submitted_ui_facts(
            &submissions,
            &lowering.packet_facts,
            backend::WgpuSubmittedFrameFacts::new(10, 9, 4, 3, 4096, 2048),
        )
        .expect_err("duplicate lowered packet facts must fail closed");

        assert!(matches!(
            error,
            WgpuRenderError::UiSubmissionRejected { .. }
        ));
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn lowered_packet_for_a_foreign_target_is_rejected_before_submission() {
        let submitted_target = RenderTarget::Window(WindowId::new(101));
        let foreign_target = RenderTarget::Window(WindowId::new(102));
        let packet = ui_outcome_packet_for(submitted_target);
        let foreign_packet = ui_outcome_packet_for(foreign_target);
        let lowering = crate::ui::lower_ui_packets(
            std::slice::from_ref(&foreign_packet),
            foreign_target,
            Some(WindowId::PRIMARY),
            ViewportRect::new(0, 0, 16, 16).unwrap(),
        )
        .unwrap();
        let mut outcomes = UiPresentationOutcomes::default();
        let backend_generation = UiPresentationBackendGeneration::new(77, 3).unwrap();
        let submitted = submitted_ui_attempt(&mut outcomes, packet, 81, 91, backend_generation);
        let submissions = vec![submitted];

        let error = WgpuRenderBackend::prepare_submitted_ui_facts(
            &submissions,
            &lowering.packet_facts,
            backend::WgpuSubmittedFrameFacts::new(10, 9, 4, 3, 4096, 2048),
        )
        .expect_err("a lowered packet for another target must fail closed");

        assert!(matches!(
            error,
            WgpuRenderError::UiSubmissionRejected { .. }
        ));
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn transaction_facts_only_name_an_identity_for_one_frame_wide_packet() {
        let packet = ui_outcome_packet();
        let facts = frame::WgpuUiPayloadFacts {
            packet_count: 1,
            packet_identity: Some(packet.identity()),
            primitives: 3,
            glyph_primitives: 2,
            glyph_pages: 1,
        };
        let mut transaction = WgpuFrameTransactionStats::default();

        transaction.record_ui_payload(facts);
        assert_eq!(transaction.ui_packet_identity(), Some(packet.identity()));

        transaction.record_ui_payload(frame::WgpuUiPayloadFacts::default());
        assert_eq!(transaction.ui_packet_identity(), Some(packet.identity()));

        transaction.record_ui_payload(facts);
        assert_eq!(transaction.ui_packet_count(), 2);
        assert_eq!(transaction.ui_packet_identity(), None);
        assert_eq!(transaction.ui_primitives(), 6);
        assert_eq!(transaction.ui_glyph_primitives(), 4);
        assert_eq!(transaction.ui_glyph_pages(), 2);
    }
}
