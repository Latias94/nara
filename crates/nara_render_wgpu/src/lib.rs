//! Wgpu backend for backend-neutral nara render data.

use std::{marker::PhantomData, time::Instant};

mod backend;
mod error;
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

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::quad::{
    WgpuQuadBatch, WgpuQuadBatchBuffer, WgpuQuadPipelineDrawRef, draw_quad_batch_buffers_for_phase,
};
use crate::surface::{SurfaceDropReason, WgpuSurfaceState};
use nara_app::{
    __RuntimeDriverPort, App, CoreStage, FrameExecutionStart, Plugin, PluginError,
    PluginShutdownContext, RuntimeDriverScope, RuntimeFault, RuntimeFaultKind,
    RuntimeFaultReporter, RuntimeGeneration, RuntimeState, RuntimeWorldAccessError,
};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_asset::Assets;
use nara_ecs::{
    Query, Res, ResMut, Resource, schedule::IntoScheduleConfigs, system::NonSendMarker,
};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_image::{ImageAsset, PreparedImageResource};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_material::AlphaMode2d;
#[cfg(test)]
use nara_render::Extent2d;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_render::PreparedRenderResources;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_render::RenderPassStepLabel;
use nara_render::{
    Color, ExtractedViews, FrameStats, RenderBackendState, RenderBackendStatus, RenderFrame,
    RenderFramePacket, RenderFramePacketError, RenderFrameSkipReason, RenderPassStep,
    RenderPhaseInput, build_render_frame_packet,
};
#[cfg(feature = "sprite-submitter")]
use nara_sprite_render::SpriteBatches;
#[cfg(feature = "ui-submitter")]
use nara_ui_render::UiBatches;
use nara_window::{
    PrimaryWindowId, Window, WindowId,
    backend::{BackendWindowHandles, WindowSurfaceRetirementDriver, WindowSurfaceRetirementError},
};

pub use crate::backend::{WgpuBackendState, WgpuFrameTransactionStats, WgpuRenderBackend};
pub use crate::error::WgpuRenderError;
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

const WGPU_RENDER_BACKEND: &str = "wgpu";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuRenderPlugin;

pub const WGPU_RENDER_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.render-wgpu");
pub const WGPU_BACKEND_SHUTDOWN_OBLIGATION: nara_app::PluginShutdownObligationId =
    nara_app::PluginShutdownObligationId::new("nara.render-wgpu.backend");
const WGPU_RENDER_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("render-wgpu")];
pub const WGPU_RENDER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(WGPU_RENDER_PLUGIN_ID, nara_app::PluginCategory::Backend)
        .requires_plugins(&[nara_render::RENDER_PLUGIN_ID])
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
        app.add_systems(
            CoreStage::Render,
            (capture_wgpu_frame_packet, render_wgpu_surfaces)
                .chain()
                .after(nara_render::begin_render_frame),
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

#[derive(Debug, Default)]
struct WgpuFramePayload {
    #[cfg(feature = "sprite-submitter")]
    sprite_batches: Option<SpriteBatches>,
    #[cfg(feature = "ui-submitter")]
    ui_batches: Option<UiBatches>,
}

#[derive(Debug)]
struct WgpuCapturedFrame {
    topology: RenderFramePacket,
    payload: WgpuFramePayload,
    app_frame_index: u64,
    work_started_at: Instant,
}

impl WgpuFramePayload {
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn quad_batches(&self) -> Vec<WgpuQuadBatch> {
        let mut batches = Vec::new();
        #[cfg(feature = "sprite-submitter")]
        if let Some(sprite_batches) = &self.sprite_batches {
            batches.extend(sprite::collect_sprite_quad_batches(sprite_batches, 0));
        }
        #[cfg(feature = "ui-submitter")]
        if let Some(ui_batches) = &self.ui_batches {
            batches.extend(ui::collect_ui_quad_batches(ui_batches, 0));
        }
        batches
    }

    fn phase_inputs(&self) -> Vec<RenderPhaseInput> {
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        let mut inputs = Vec::new();
        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        let inputs = Vec::new();
        #[cfg(feature = "sprite-submitter")]
        if let Some(sprite_batches) = &self.sprite_batches {
            sprite::append_sprite_phase_inputs(sprite_batches, &mut inputs);
        }
        #[cfg(feature = "ui-submitter")]
        if let Some(ui_batches) = &self.ui_batches {
            ui::append_ui_phase_inputs(ui_batches, &mut inputs);
        }
        inputs
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

#[derive(Debug, Default, Resource)]
struct WgpuFramePacketSlot {
    current: Option<Result<Option<WgpuCapturedFrame>, WgpuFrameCaptureError>>,
}

#[derive(Debug, thiserror::Error)]
enum WgpuFrameCaptureError {
    #[error("managed runtime generation is unavailable")]
    MissingRuntimeGeneration,
    #[error(transparent)]
    Topology(#[from] RenderFramePacketError),
    #[error("a render batch does not belong to the admitted view and target")]
    BatchTopologyMismatch,
}

fn capture_wgpu_frame_packet(
    generation: Option<Res<RuntimeGeneration>>,
    frame_start: Res<FrameExecutionStart>,
    frame: Res<RenderFrame>,
    primary_window_id: Option<Res<PrimaryWindowId>>,
    windows: Query<&Window>,
    views: Res<ExtractedViews>,
    #[cfg(feature = "sprite-submitter")] sprite_batches: Option<Res<SpriteBatches>>,
    #[cfg(feature = "ui-submitter")] ui_batches: Option<Res<UiBatches>>,
    mut slot: ResMut<WgpuFramePacketSlot>,
) {
    let Some(generation) = generation.map(|generation| *generation) else {
        slot.current = Some(Err(WgpuFrameCaptureError::MissingRuntimeGeneration));
        return;
    };
    let payload = WgpuFramePayload {
        #[cfg(feature = "sprite-submitter")]
        sprite_batches: sprite_batches.as_deref().cloned(),
        #[cfg(feature = "ui-submitter")]
        ui_batches: ui_batches.as_deref().cloned(),
    };
    if !payload_batches_match_topology(&payload, &views) {
        slot.current = Some(Err(WgpuFrameCaptureError::BatchTopologyMismatch));
        return;
    }
    let phases = payload.phase_inputs();
    slot.current = Some(
        build_render_frame_packet(
            generation,
            frame.index,
            primary_window_id.map(|primary| primary.0),
            windows.iter(),
            &views,
            phases,
        )
        .map(|topology| {
            topology.map(|topology| WgpuCapturedFrame {
                topology,
                payload,
                app_frame_index: frame_start.frame(),
                work_started_at: frame_start.started_at(),
            })
        })
        .map_err(WgpuFrameCaptureError::from),
    );
}

fn payload_batches_match_topology(payload: &WgpuFramePayload, views: &ExtractedViews) -> bool {
    let Some(view) = views.as_slice().first() else {
        return true;
    };
    #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
    let _ = (payload, view);
    #[cfg(feature = "sprite-submitter")]
    if payload.sprite_batches.as_ref().is_some_and(|batches| {
        batches
            .as_slice()
            .iter()
            .any(|batch| batch.view_index != 0 || batch.target != view.target)
    }) {
        return false;
    }
    #[cfg(feature = "ui-submitter")]
    if payload.ui_batches.as_ref().is_some_and(|batches| {
        batches
            .as_slice()
            .iter()
            .any(|batch| batch.view_index != 0 || batch.target != view.target)
    }) {
        return false;
    }
    true
}

#[derive(Clone, Copy)]
struct RenderResourceInputs<'a> {
    _lifetime: PhantomData<&'a ()>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    images: Option<&'a Assets<ImageAsset>>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    prepared_images: Option<&'a PreparedRenderResources<PreparedImageResource>>,
}

impl Default for RenderResourceInputs<'_> {
    fn default() -> Self {
        Self {
            _lifetime: PhantomData,
            #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
            images: None,
            #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
            prepared_images: None,
        }
    }
}

#[derive(Debug, Default)]
struct PreparedSubmitterDraw {
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    pipelines: Vec<WgpuQuadPipelineDrawRef>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    buffers: Vec<WgpuQuadBatchBuffer>,
    draw_calls: u32,
    sprites: u32,
}

fn render_wgpu_surfaces(
    mut backend: ResMut<WgpuRenderBackend>,
    handles: Option<Res<BackendWindowHandles>>,
    mut packet_slot: ResMut<WgpuFramePacketSlot>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))] images: Option<
        Res<Assets<ImageAsset>>,
    >,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))] prepared_images: Option<
        Res<PreparedRenderResources<PreparedImageResource>>,
    >,
    mut frame: ResMut<RenderFrame>,
    mut stats: ResMut<FrameStats>,
    mut status: ResMut<RenderBackendStatus>,
    faults: Option<Res<RuntimeFaultReporter>>,
    _main_thread: NonSendMarker,
) {
    backend.begin_frame_transaction(frame.index);
    let resources = RenderResourceInputs {
        _lifetime: PhantomData,
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        images: images.as_deref(),
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        prepared_images: prepared_images.as_deref(),
    };
    let capture = packet_slot.current.take();
    let result = match capture {
        Some(Ok(Some(packet))) => backend.render_packet(
            handles.as_deref(),
            packet,
            resources,
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
            Ok(())
        }
    };
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

fn handle_wgpu_render_error(
    backend: &mut WgpuRenderBackend,
    error: WgpuRenderError,
    frame_index: u64,
    status: &mut RenderBackendStatus,
    faults: Option<&RuntimeFaultReporter>,
) {
    if error.is_packet_admission_rejection() {
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
    pass_steps: &[RenderPassStep],
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
        for step in pass_steps {
            if let RenderPassStepLabel::Phase(phase) = step.node.label {
                draw_quad_batch_buffers_for_phase(
                    &mut pass,
                    &draw.pipelines,
                    &draw.buffers,
                    phase,
                    viewport,
                );
            }
        }
        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        let _ = (&mut pass, draw, pass_steps);
    }
    queue.submit([encoder.finish()]);
    queue.present(surface_texture);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
        commands.spawn(nara_render::Camera2d::default());
    }

    #[test]
    fn plugin_installs_backend_and_render_resources() {
        let mut world = nara_ecs::World::new();
        world.init_resource::<WgpuRenderBackend>();
        assert!(world.contains_resource::<WgpuRenderBackend>());

        let mut app = App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_render::RenderPlugin,
            WgpuRenderPlugin,
        ))
        .unwrap();

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

    #[test]
    fn plugin_installation_reconstructs_the_backend_instance_for_each_app() {
        fn backend_instance_id() -> u64 {
            let mut app = App::new();
            app.add_plugins((
                nara_reflect::ComponentRegistryPlugin,
                nara_render::RenderPlugin,
                WgpuRenderPlugin,
            ))
            .unwrap();
            app.world().resource::<WgpuRenderBackend>().instance_id()
        }

        assert_ne!(backend_instance_id(), backend_instance_id());
    }

    #[test]
    fn managed_frame_keeps_the_backend_terminal_state_after_render_schedule_completion() {
        let mut app = App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_render::RenderPlugin,
            WgpuRenderPlugin,
        ))
        .unwrap();
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
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_render::RenderPlugin,
            WgpuRenderPlugin,
        ))
        .unwrap();
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
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_window::WindowPlugin::default(),
            nara_render::RenderPlugin,
            WgpuRenderPlugin,
        ))
        .unwrap();
        app.insert_resource(SecondCameraInjection::default())
            .unwrap();
        app.world_mut()
            .unwrap()
            .spawn(nara_render::Camera2d::default());
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
        assert_eq!(first.frame_index(), Some(first_frame.index));
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
        assert_eq!(rejected.frame_index(), Some(rejected_frame.index));
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
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_render::RenderPlugin,
            WgpuRenderPlugin,
        ))
        .unwrap();
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

    #[test]
    fn base_submitter_input_has_no_phase_work() {
        assert!(WgpuFramePayload::default().phase_inputs().is_empty());
    }
}
