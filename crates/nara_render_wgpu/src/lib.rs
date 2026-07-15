//! Wgpu backend for backend-neutral nara render data.

use std::marker::PhantomData;

mod backend;
mod error;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
mod quad;
#[cfg(feature = "sprite-submitter")]
mod sprite;
mod surface;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
mod texture;
#[cfg(feature = "ui-submitter")]
mod ui;

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::quad::{
    WgpuQuadBatch, WgpuQuadBatchBuffer, WgpuQuadPipelineDrawRef, draw_quad_batch_buffers_for_phase,
};
use crate::surface::{SurfaceDropReason, WgpuSurfaceState};
use nara_app::{App, CoreStage, Plugin, PluginError, PluginShutdownContext};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_asset::Assets;
use nara_ecs::{Query, Res, ResMut, schedule::IntoScheduleConfigs, system::NonSendMarker};
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
    RenderFrameSkipReason, RenderPassPlan, RenderPassStep, RenderPhaseInput, begin_render_frame,
    build_render_pass_plan,
};
#[cfg(feature = "sprite-submitter")]
use nara_sprite_render::SpriteBatches;
#[cfg(feature = "ui-submitter")]
use nara_ui_render::UiBatches;
use nara_window::{
    PrimaryWindowId, Window, WindowId,
    backend::{BackendWindowHandles, WindowSurfaceRetirementDriver, WindowSurfaceRetirementError},
};

pub use crate::backend::{WgpuBackendState, WgpuRenderBackend};
pub use crate::error::WgpuRenderError;

pub use crate::surface::{
    SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus, choose_present_mode,
    clear_color_to_wgpu, map_present_mode, surface_acquire_policy, surface_resize_action,
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
            render_wgpu_surfaces.after(begin_render_frame),
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
    world: &mut nara_ecs::World,
    window_ids: &[WindowId],
) -> Result<(), WindowSurfaceRetirementError> {
    let Some(mut backend) = world.get_resource_mut::<WgpuRenderBackend>() else {
        return Ok(());
    };
    backend
        .retire_targets(window_ids, SurfaceDropReason::TargetShutdown)
        .map_err(|_| WindowSurfaceRetirementError::DriverFailed {
            driver: WGPU_RENDER_BACKEND,
        })
}

#[derive(Clone, Copy)]
struct SubmitterInputs<'a> {
    _lifetime: PhantomData<&'a ()>,
    #[cfg(feature = "sprite-submitter")]
    sprite_batches: Option<&'a SpriteBatches>,
    #[cfg(feature = "ui-submitter")]
    ui_batches: Option<&'a UiBatches>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    images: Option<&'a Assets<ImageAsset>>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    prepared_images: Option<&'a PreparedRenderResources<PreparedImageResource>>,
}

impl SubmitterInputs<'_> {
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn quad_batches(self, view_index: usize) -> Vec<WgpuQuadBatch> {
        let mut batches = Vec::new();
        #[cfg(feature = "sprite-submitter")]
        if let Some(sprite_batches) = self.sprite_batches {
            batches.extend(sprite::collect_sprite_quad_batches(
                sprite_batches,
                view_index,
            ));
        }
        #[cfg(feature = "ui-submitter")]
        if let Some(ui_batches) = self.ui_batches {
            batches.extend(ui::collect_ui_quad_batches(ui_batches, view_index));
        }
        batches
    }

    fn phase_inputs(self) -> Vec<RenderPhaseInput> {
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        let mut inputs = Vec::new();
        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        let inputs = Vec::new();
        #[cfg(feature = "sprite-submitter")]
        if let Some(sprite_batches) = self.sprite_batches {
            sprite::append_sprite_phase_inputs(sprite_batches, &mut inputs);
        }
        #[cfg(feature = "ui-submitter")]
        if let Some(ui_batches) = self.ui_batches {
            ui::append_ui_phase_inputs(ui_batches, &mut inputs);
        }
        inputs
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

pub fn render_wgpu_surfaces(
    mut backend: ResMut<WgpuRenderBackend>,
    handles: Option<Res<BackendWindowHandles>>,
    windows: Query<&Window>,
    views: Res<ExtractedViews>,
    #[cfg(feature = "sprite-submitter")] sprite_batches: Option<Res<SpriteBatches>>,
    #[cfg(feature = "ui-submitter")] ui_batches: Option<Res<UiBatches>>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))] images: Option<
        Res<Assets<ImageAsset>>,
    >,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))] prepared_images: Option<
        Res<PreparedRenderResources<PreparedImageResource>>,
    >,
    primary_window_id: Option<Res<PrimaryWindowId>>,
    mut frame: ResMut<RenderFrame>,
    mut stats: ResMut<FrameStats>,
    mut status: ResMut<RenderBackendStatus>,
    _main_thread: NonSendMarker,
) {
    let submitters = SubmitterInputs {
        _lifetime: PhantomData,
        #[cfg(feature = "sprite-submitter")]
        sprite_batches: sprite_batches.as_deref(),
        #[cfg(feature = "ui-submitter")]
        ui_batches: ui_batches.as_deref(),
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        images: images.as_deref(),
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        prepared_images: prepared_images.as_deref(),
    };
    let result = backend.render_surfaces(
        handles.as_deref(),
        &windows,
        &views,
        submitters,
        primary_window_id.map(|resource| resource.0),
        &mut frame,
        &mut stats,
        &mut status,
    );
    if let Err(error) = result {
        let message = backend.mark_error(&error);
        status.mark_unavailable(WGPU_RENDER_BACKEND, message.clone());
        status.mark_skipped_with_message(frame.index, RenderFrameSkipReason::BackendError, message);
        frame.mark_skipped();
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
    clear_color: Color,
    draw: &PreparedSubmitterDraw,
    pass_steps: &[RenderPassStep],
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
        for step in pass_steps {
            if let RenderPassStepLabel::Phase(phase) = step.node.label {
                draw_quad_batch_buffers_for_phase(&mut pass, &draw.pipelines, &draw.buffers, phase);
            }
        }
        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        let _ = (&mut pass, draw, pass_steps);
    }
    queue.submit([encoder.finish()]);
    queue.present(surface_texture);
    Ok(())
}

fn build_wgpu_render_pass_plan(
    views: &ExtractedViews,
    submitters: SubmitterInputs<'_>,
) -> RenderPassPlan {
    build_render_pass_plan(views, submitters.phase_inputs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_ecs::system::{IntoSystem, System};

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
    fn render_system_is_pinned_to_the_main_thread_executor() {
        let mut system = IntoSystem::into_system(render_wgpu_surfaces);
        system.initialize(&mut nara_ecs::World::new());

        assert!(!system.is_send());
    }

    #[test]
    fn base_submitter_input_has_no_phase_work() {
        let inputs = SubmitterInputs {
            _lifetime: PhantomData,
            #[cfg(feature = "sprite-submitter")]
            sprite_batches: None,
            #[cfg(feature = "ui-submitter")]
            ui_batches: None,
            #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
            images: None,
            #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
            prepared_images: None,
        };
        assert!(inputs.phase_inputs().is_empty());
    }
}
