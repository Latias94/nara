//! Direct WGPU presentation for a native Winit session.

use std::{
    fmt::Display,
    time::{Duration, Instant},
};

use nara_app::{
    App, AppExit, AppRunError, FrameExecutionStart, Plugin, PluginCategory, PluginDeclaration,
    PluginError, PluginId, RuntimeGeneration, RuntimeInstance, RuntimeState,
};
use nara_ecs::{QueryState, Resource, World};
use nara_window::{
    PrimaryWindowId, WINDOW_PLUGIN_ID, Window, WindowId,
    backend::{BackendWindowHandles, WindowPresentationRequirement},
    winit::{WinitPresentationDriver, WinitPresentationProgress, WinitRunner},
};

use crate::{
    Extent2d, ExtractedViews, RENDER_PLUGIN_ID, RenderFramePacket, RenderFramePacketError,
    RenderWindowPacket, WorldInputMapping2d, build_render_frame_packet_with_world_canvas,
    runtime_integration::{
        build_window_overlay_frame_packet, clear_presented_world_frame,
        publish_presented_world_frame,
    },
};

use super::{
    GpuContext, RenderInput, RenderTargetView, Renderer, WgpuNativeWorkStats, WgpuRenderError,
    surface::{DesktopSurface, SurfaceAcquisition},
};

const GPU_POLL_INTERVAL: Duration = Duration::from_millis(2);
const SURFACE_RETRY_INTERVAL: Duration = Duration::from_millis(16);

/// Facts published after the rendered surface texture is handed to WGPU for presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopPresentedFrame {
    pub window: WindowId,
    pub frame_index: u64,
    pub surface_generation: u64,
    pub extent: Extent2d,
    pub present_mode: wgpu::PresentMode,
    pub direct_surface: bool,
    pub hud_glyph_primitives: u64,
    pub work: WgpuNativeWorkStats,
}

/// The last completed native presentation. A skipped frame leaves this observation intact.
#[derive(Debug, Default, Resource)]
pub struct DesktopPresentation {
    latest: Option<DesktopPresentedFrame>,
    last_error: Option<String>,
}

impl DesktopPresentation {
    pub const fn latest(&self) -> Option<DesktopPresentedFrame> {
        self.latest
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

/// Installs the direct WGPU desktop runner and its presentation observation.
#[derive(Debug, Default)]
pub struct DesktopPresentationPlugin;

const ID: PluginId = PluginId::new("nara.winit-runner");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Platform)
    .requires_plugins(&[WINDOW_PLUGIN_ID, RENDER_PLUGIN_ID]);

impl Plugin for DesktopPresentationPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if let Some(owner) = app.world().get_resource::<WindowPresentationRequirement>() {
            return Err(PluginError::SetupFailed {
                plugin: ID,
                message: format!("{} already owns window presentation", owner.backend()),
            });
        }
        app.insert_resource(WindowPresentationRequirement::new("wgpu"))?;
        app.init_resource::<DesktopPresentation>()?;
        app.set_runner(run);
        Ok(())
    }
}

fn run(app: App) -> Result<AppExit, AppRunError> {
    let runtime = RuntimeInstance::start(app)?;
    let mut session = WinitRunner::default().start_with_presentation_on(
        runtime,
        Box::new(DesktopPresentationDriver::default()),
        Duration::from_secs(5),
        None,
    );
    session.execute();
    session
        .into_retired()
        .map_err(|_| AppRunError::runner("could not finish retiring the desktop window"))?
        .result
}

/// Owns the GPU context and the surface lease for one Winit session.
#[derive(Default)]
pub struct DesktopPresentationDriver {
    surface: Option<DesktopSurface>,
    renderer: Option<Renderer>,
    windows: Option<QueryState<&'static Window>>,
    last_frame: Option<u64>,
    retry_at: Option<Instant>,
    retiring: bool,
    device_loss_reported: bool,
    #[cfg(feature = "hud-runtime")]
    hud_surface: Option<nara_ui::UiSurfaceId>,
}

enum FrameProgress {
    Presented,
    Retry,
    Skipped,
}

impl DesktopPresentationDriver {
    fn maintain_frame(
        &mut self,
        world: &mut World,
        handles: &BackendWindowHandles,
        generation: RuntimeGeneration,
        now: Instant,
    ) -> Result<WinitPresentationProgress, AppRunError> {
        if let Some(index) = world
            .get_resource::<FrameExecutionStart>()
            .map(|start| start.frame())
            && self.last_frame != Some(index)
            && self.retry_at.is_none_or(|deadline| now >= deadline)
        {
            self.retry_at = None;
            match self.draw_frame(world, handles, generation, index)? {
                FrameProgress::Presented => {
                    self.last_frame = Some(index);
                    return Ok(WinitPresentationProgress {
                        next_wake: now.checked_add(GPU_POLL_INTERVAL),
                        ..Default::default()
                    });
                }
                FrameProgress::Retry => self.retry_at = now.checked_add(SURFACE_RETRY_INTERVAL),
                FrameProgress::Skipped => self.last_frame = Some(index),
            }
        }
        let mut next_wake = self.retry_at;
        if let Some(renderer) = &mut self.renderer
            && !renderer.poll_gpu().map_err(desktop_error)?.is_queue_empty()
            && let Some(poll) = now.checked_add(GPU_POLL_INTERVAL)
        {
            next_wake = Some(next_wake.map_or(poll, |retry| retry.min(poll)));
        }
        Ok(WinitPresentationProgress {
            next_wake,
            ..Default::default()
        })
    }

    fn draw_frame(
        &mut self,
        world: &mut World,
        handles: &BackendWindowHandles,
        generation: RuntimeGeneration,
        index: u64,
    ) -> Result<FrameProgress, AppRunError> {
        let windows = self.windows.get_or_insert_with(|| world.query());
        let window = windows.single(world).map_err(desktop_error)?;
        let id = window.id;
        if !handles.is_surface_target_active(id) {
            self.discard_surface(world)?;
            return Ok(FrameProgress::Skipped);
        }
        if window.resolution.is_zero_sized() {
            self.clear_display(world);
            return Ok(FrameProgress::Skipped);
        }
        let requested_mode = window.present_mode;
        let Some(packet) =
            capture_window_packet(world, generation, index, window).map_err(desktop_error)?
        else {
            return Ok(FrameProgress::Skipped);
        };
        let extent = packet.target().extent();

        if self
            .surface
            .as_ref()
            .is_some_and(|surface| surface.window != id)
        {
            self.discard_surface(world)?;
        }
        if self.surface.is_none() {
            let instance = self.renderer.as_ref().map_or_else(
                || {
                    wgpu::Instance::new(
                        wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
                    )
                },
                |renderer| renderer.context().instance().clone(),
            );
            let surface = match DesktopSurface::new(&instance, handles, id) {
                Ok(surface) => surface,
                Err(error) => {
                    record_error(world, error);
                    return Ok(FrameProgress::Skipped);
                }
            };
            if self.renderer.is_none() {
                let context =
                    GpuContext::request(instance, Some(&surface.handle)).map_err(desktop_error)?;
                self.renderer = Some(Renderer::new(context));
            }
            self.surface = Some(surface);
        }

        let (Some(surface), Some(renderer)) = (&mut self.surface, &mut self.renderer) else {
            return Err(desktop_error(
                "desktop surface or renderer was not initialized",
            ));
        };
        match surface.configure(renderer.context(), extent, requested_mode) {
            Ok(true) => renderer.check_errors().map_err(desktop_error)?,
            Ok(false) => {}
            Err(error) => {
                record_error(world, error);
                self.discard_surface(world)?;
                return Ok(FrameProgress::Skipped);
            }
        }
        let acquired = match surface.acquire() {
            Ok(SurfaceAcquisition::Ready(acquired)) => acquired,
            Ok(SurfaceAcquisition::Retry) => return Ok(FrameProgress::Retry),
            Ok(SurfaceAcquisition::Occluded) => return Ok(FrameProgress::Skipped),
            Ok(SurfaceAcquisition::Reconfigure) => {
                self.clear_display(world);
                return Ok(FrameProgress::Retry);
            }
            Ok(SurfaceAcquisition::Lost) => {
                self.discard_surface(world)?;
                return Ok(FrameProgress::Retry);
            }
            Err(error) => {
                renderer.check_errors().map_err(desktop_error)?;
                record_error(world, error);
                self.discard_surface(world)?;
                return Ok(FrameProgress::Retry);
            }
        };
        let size = acquired.texture.texture.size();
        if size.width != extent.width || size.height != extent.height {
            surface.invalidate();
            self.clear_display(world);
            return Ok(FrameProgress::Retry);
        }
        let format = acquired.format();
        let present_mode = acquired.present_mode();
        let surface_generation = surface.generation();
        let input = RenderInput::capture(world, packet);
        let view = acquired
            .texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                format: Some(format),
                ..Default::default()
            });
        let report = match renderer.render_to_view(
            &input,
            RenderTargetView {
                view: &view,
                format,
                extent,
            },
        ) {
            Ok(report) => report,
            Err(WgpuRenderError::TargetsBusy) => return Ok(FrameProgress::Retry),
            Err(error @ (WgpuRenderError::Device(_) | WgpuRenderError::DeviceLost)) => {
                return Err(desktop_error(error));
            }
            Err(error) => {
                record_error(world, error);
                return Ok(FrameProgress::Skipped);
            }
        };
        drop(view);
        renderer.context().queue().present(acquired.texture);
        renderer.check_errors().map_err(desktop_error)?;

        #[cfg(feature = "hud-runtime")]
        let (hud_glyph_primitives, hud_error) = {
            let hud = report.hud.as_ref().ok().and_then(Option::as_ref);
            let count = self.publish_hud(world, hud, index)?;
            (count, report.hud.err().map(|error| error.to_string()))
        };
        #[cfg(not(feature = "hud-runtime"))]
        let (hud_glyph_primitives, hud_error) = (0, None);
        publish_presented_world_frame(world, input.packet(), Some(WorldInputMapping2d::identity()));
        let mut observation = world.get_resource_or_init::<DesktopPresentation>();
        observation.last_error = hud_error;
        observation.latest = Some(DesktopPresentedFrame {
            window: id,
            frame_index: index,
            surface_generation,
            extent,
            present_mode,
            direct_surface: input.packet().world_canvas().is_none(),
            hud_glyph_primitives,
            work: report.work,
        });
        Ok(FrameProgress::Presented)
    }

    #[cfg(feature = "hud-runtime")]
    fn publish_hud(
        &mut self,
        world: &mut World,
        hud: Option<&crate::hud::HudPresentation>,
        index: u64,
    ) -> Result<u64, AppRunError> {
        let next = hud.map(|hud| hud.frame.environment().surface_id());
        if self.hud_surface != next {
            self.clear_hud(world);
        }
        let Some(hud) = hud else { return Ok(0) };
        let receipt = nara_ui::presentation::publish_presented_frame(world, &hud.frame, index)
            .map_err(desktop_error)?;
        self.hud_surface = Some(receipt.surface_id());
        Ok(hud.glyph_count)
    }

    #[cfg(feature = "hud-runtime")]
    fn clear_hud(&mut self, world: &mut World) {
        if let Some(surface) = self.hud_surface.take() {
            nara_ui::presentation::clear_presented_ui_frame(world, surface);
        }
    }

    fn clear_display(&mut self, world: &mut World) {
        clear_presented_world_frame(world);
        #[cfg(feature = "hud-runtime")]
        self.clear_hud(world);
    }

    fn discard_surface(&mut self, world: &mut World) -> Result<(), AppRunError> {
        self.clear_display(world);
        if let Some(surface) = self.surface.take() {
            surface.release().map_err(desktop_error)?;
        }
        Ok(())
    }
}

impl WinitPresentationDriver for DesktopPresentationDriver {
    fn maintain(
        &mut self,
        runtime: &mut RuntimeInstance,
        handles: &BackendWindowHandles,
        now: Instant,
    ) -> Result<WinitPresentationProgress, AppRunError> {
        if self.retiring || runtime.state() == RuntimeState::Stopped {
            return Ok(WinitPresentationProgress::default());
        }
        let generation = runtime.generation();
        let outcome = runtime
            .with_driver_scope(|scope| {
                let live = matches!(scope.state(), RuntimeState::Running | RuntimeState::Paused);
                scope.with_world_mut(|world| {
                    let progress = if live {
                        self.maintain_frame(world, handles, generation, now)
                    } else {
                        Ok(WinitPresentationProgress::default())
                    };
                    if !live || progress.is_err() {
                        self.retiring = true;
                        self.device_loss_reported = progress.is_err()
                            && self
                                .renderer
                                .as_ref()
                                .is_some_and(|renderer| renderer.context().is_lost());
                        self.clear_display(world);
                    }
                    progress
                })
            })
            .map_err(|error| {
                error
                    .fault()
                    .map_or_else(|| desktop_error(&error), AppRunError::managed_runtime_fault)
            })?;
        match (outcome.output, outcome.post_operation_fault) {
            (progress, None) => progress,
            (Ok(_), Some(fault)) => Err(AppRunError::managed_runtime_fault(&fault)),
            (Err(error), Some(fault)) => Err(AppRunError::runner_teardown(
                error,
                AppRunError::managed_runtime_fault(&fault),
            )),
        }
    }

    fn retire(
        &mut self,
        runtime: &mut RuntimeInstance,
        _handles: &BackendWindowHandles,
        _window_ids: &[WindowId],
        _now: Instant,
    ) -> Result<bool, AppRunError> {
        self.retiring = true;
        let mut completion = Ok(());
        if let Some(renderer) = &mut self.renderer {
            if !renderer.context().is_lost() {
                match renderer.context().device().poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: Some(GPU_POLL_INTERVAL),
                }) {
                    Ok(_) => {}
                    Err(wgpu::PollError::Timeout) => {
                        if !renderer.context().is_lost() {
                            return Ok(false);
                        }
                    }
                    Err(error) => completion = Err(desktop_error(error)),
                }
            }
            match renderer.check_errors() {
                Ok(()) => {}
                Err(WgpuRenderError::DeviceLost) if self.device_loss_reported => {}
                Err(error) => completion = completion.and(Err(desktop_error(error))),
            }
        }
        if runtime.state() != RuntimeState::Stopped {
            let withdrawn = runtime
                .with_driver_scope(|scope| scope.with_world_mut(|world| self.clear_display(world)))
                .map_err(desktop_error)
                .and_then(|outcome| match outcome.post_operation_fault {
                    Some(fault) => Err(AppRunError::managed_runtime_fault(&fault)),
                    None => Ok(()),
                });
            completion = completion.and(withdrawn);
        }
        let retired = self.surface.take().map(DesktopSurface::release).transpose();
        self.renderer = None;
        completion.and(retired.map_err(desktop_error)).map(|_| true)
    }
}

fn capture_window_packet(
    world: &World,
    generation: RuntimeGeneration,
    index: u64,
    window: &Window,
) -> Result<Option<RenderFramePacket>, RenderFramePacketError> {
    let Some(views) = world.get_resource::<ExtractedViews>() else {
        return Ok(None);
    };
    if cfg!(feature = "hud-runtime") && views.is_empty() {
        return Ok(build_window_overlay_frame_packet(
            generation,
            index,
            RenderWindowPacket {
                id: window.id,
                resolution: window.resolution,
                present_mode: window.present_mode,
            },
            views.captured_time(),
        ));
    }
    build_render_frame_packet_with_world_canvas(
        generation,
        index,
        world
            .get_resource::<PrimaryWindowId>()
            .map(|primary| primary.0),
        [window],
        views,
        views.world_canvas(),
    )
}

fn desktop_error(error: impl Display) -> AppRunError {
    AppRunError::runner(format!("WGPU desktop presentation failed: {error}"))
}

fn record_error(world: &mut World, error: impl Display) {
    world
        .get_resource_or_init::<DesktopPresentation>()
        .last_error = Some(error.to_string());
}

#[cfg(test)]
mod tests;
