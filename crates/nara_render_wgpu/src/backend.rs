use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use nara_ecs::{Query, Resource};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_material::AlphaMode2d;
use nara_render::{
    Color, Extent2d, ExtractedViews, FrameStats, RenderBackendState, RenderBackendStatus,
    RenderFrame, RenderFrameSkipReason, RenderPassStep,
};
use nara_window::{
    Window, WindowId,
    backend::{BackendWindowHandles, WindowTargetError},
};

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::quad::{
    WgpuQuadPipeline, WgpuQuadPipelineDrawRef, WgpuQuadPipelineKey, create_quad_batch_buffers,
    create_quad_pipeline, create_quad_texture_bind_group_layout, quad_batch_draw_stats,
};
use crate::surface::{
    SurfaceDropReason, SurfaceResizeAction, WgpuSurfaceState, configure_surface, create_surface,
    surface_extent, surface_resize_action,
};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::texture::WgpuSpriteTextureCache;
use crate::{
    PreparedSubmitterDraw, SubmitterInputs, WGPU_RENDER_BACKEND, WgpuRenderError,
    build_wgpu_render_pass_plan, render_acquired_texture,
};

#[derive(Debug, Default)]
struct DeviceLossSignal(AtomicBool);

impl DeviceLossSignal {
    fn mark_lost(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn take_lost(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WgpuBackendState {
    #[default]
    Uninitialized,
    Initializing,
    Ready,
    Unavailable,
}

#[derive(Debug, Default, Resource)]
pub struct WgpuRenderBackend {
    state: WgpuBackendState,
    instance: Option<wgpu::Instance>,
    adapter: Option<wgpu::Adapter>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    device_loss_signal: Option<Arc<DeviceLossSignal>>,
    surfaces: BTreeMap<WindowId, WgpuSurfaceState>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_texture_bind_group_layout: Option<wgpu::BindGroupLayout>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_pipelines: Vec<WgpuQuadPipeline>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_textures: WgpuSpriteTextureCache,
    last_error: Option<String>,
}

impl WgpuRenderBackend {
    #[must_use]
    pub const fn state(&self) -> WgpuBackendState {
        self.state
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    #[must_use]
    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    #[must_use]
    pub fn configured_surface_extent(&self, window_id: WindowId) -> Option<Extent2d> {
        self.surfaces
            .get(&window_id)
            .filter(|surface| surface.config.is_some())
            .map(|surface| surface.size)
    }

    pub(super) fn clear_gpu_resources(
        &mut self,
        reason: SurfaceDropReason,
    ) -> Result<(), WgpuRenderError> {
        let window_ids = self.surfaces.keys().copied().collect::<Vec<_>>();
        let retirement_result = self.retire_targets(&window_ids, reason);
        self.device_loss_signal = None;
        self.queue = None;
        self.device = None;
        self.adapter = None;
        self.instance = None;
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        {
            self.quad_texture_bind_group_layout = None;
            self.quad_pipelines.clear();
            self.quad_textures.clear();
        }
        retirement_result
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[must_use]
    pub fn texture_count(&self) -> usize {
        self.quad_textures.image_count()
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[must_use]
    pub fn texture_stats(&self) -> crate::WgpuRenderTextureCacheStats {
        self.quad_textures.stats()
    }

    pub(super) fn render_surfaces(
        &mut self,
        handles: Option<&BackendWindowHandles>,
        windows: &Query<&Window>,
        views: &ExtractedViews,
        submitters: SubmitterInputs<'_>,
        primary_window_id: Option<WindowId>,
        frame: &mut RenderFrame,
        stats: &mut FrameStats,
        status: &mut RenderBackendStatus,
    ) -> Result<(), WgpuRenderError> {
        stats.draw_calls = 0;
        stats.sprites = 0;
        self.fail_if_device_lost()?;
        status.mark_state(WGPU_RENDER_BACKEND, render_backend_state(self.state));
        status.clear_skip();

        self.retire_requested_surfaces()?;

        if views.is_empty() {
            frame.mark_skipped();
            status.mark_skipped_with_message(
                frame.index,
                RenderFrameSkipReason::NoViews,
                "no extracted render views",
            );
            return Ok(());
        }

        if self.state == WgpuBackendState::Unavailable {
            let message = self
                .last_error
                .clone()
                .unwrap_or_else(|| WgpuRenderError::BackendUnavailable.to_string());
            frame.mark_skipped();
            status.mark_unavailable(WGPU_RENDER_BACKEND, message.clone());
            status.mark_skipped_with_message(
                frame.index,
                RenderFrameSkipReason::BackendError,
                message,
            );
            return Ok(());
        }

        self.ensure_device()?;
        self.fail_if_device_lost()?;
        status.mark_ready(WGPU_RENDER_BACKEND);
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        self.quad_textures.begin_frame(frame.index);
        let pass_plan = build_wgpu_render_pass_plan(views, submitters);

        let mut submitted_any = false;
        for (view_index, view) in views.as_slice().iter().enumerate() {
            let Some(window_id) = crate::surface::target_window_id(view.target, primary_window_id)
            else {
                continue;
            };
            let Some(window) = windows.iter().find(|window| window.id == window_id) else {
                continue;
            };
            let Some(size) = surface_extent(
                window.resolution.physical_width,
                window.resolution.physical_height,
            ) else {
                continue;
            };

            if !self.surfaces.contains_key(&window_id) {
                let Some(handles) = handles else {
                    continue;
                };
                if !self.create_surface_if_missing(window, handles, size)? {
                    continue;
                }
            }
            self.configure_surface_if_needed(window, size)?;
            if !self
                .surfaces
                .get(&window_id)
                .is_some_and(WgpuSurfaceState::can_acquire_frame)
            {
                continue;
            }
            let view_format = self.surface_view_format(window.id)?;

            let draw =
                self.prepare_submitter_draw(view_index, view_format, submitters, frame.index)?;
            let pass_steps = pass_plan.for_view(view_index).copied().collect::<Vec<_>>();
            if self.render_window(window.id, view.clear_color, &draw, &pass_steps)? {
                stats.draw_calls = stats.draw_calls.saturating_add(draw.draw_calls);
                stats.sprites = stats.sprites.saturating_add(draw.sprites);
                submitted_any = true;
            }
        }

        if submitted_any {
            frame.mark_submitted();
        } else {
            frame.mark_skipped();
            let (reason, message) = if handles.is_none() {
                (
                    RenderFrameSkipReason::SurfaceUnavailable,
                    "backend window handles resource is missing",
                )
            } else {
                (
                    RenderFrameSkipReason::NoRenderableTarget,
                    "no view resolved to an available backend surface",
                )
            };
            status.mark_skipped_with_message(frame.index, reason, message);
        }
        Ok(())
    }

    fn ensure_device(&mut self) -> Result<(), WgpuRenderError> {
        if self.state == WgpuBackendState::Ready {
            return Ok(());
        }
        if self.state == WgpuBackendState::Unavailable {
            return Err(WgpuRenderError::BackendUnavailable);
        }

        self.state = WgpuBackendState::Initializing;
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .map_err(|error| WgpuRenderError::AdapterUnavailable {
                message: error.to_string(),
            })?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|error| WgpuRenderError::DeviceUnavailable {
                    message: error.to_string(),
                })?;
        let device_loss_signal = Arc::new(DeviceLossSignal::default());
        let callback_signal = Arc::clone(&device_loss_signal);
        device.set_device_lost_callback(move |_reason, _message| callback_signal.mark_lost());
        self.instance = Some(instance);
        self.adapter = Some(adapter);
        self.device = Some(device);
        self.queue = Some(queue);
        self.device_loss_signal = Some(device_loss_signal);
        self.state = WgpuBackendState::Ready;
        self.last_error = None;
        Ok(())
    }

    fn fail_if_device_lost(&self) -> Result<(), WgpuRenderError> {
        if self
            .device_loss_signal
            .as_ref()
            .is_some_and(|signal| signal.take_lost())
        {
            Err(WgpuRenderError::DeviceLost)
        } else {
            Ok(())
        }
    }

    fn retire_requested_surfaces(&mut self) -> Result<(), WgpuRenderError> {
        let mut retirements = Vec::new();
        for (window_id, surface) in &self.surfaces {
            if surface.retirement_requested()? {
                retirements.push(*window_id);
            }
        }
        self.retire_targets(&retirements, SurfaceDropReason::TargetShutdown)
    }

    pub(super) fn retire_targets(
        &mut self,
        window_ids: &[WindowId],
        reason: SurfaceDropReason,
    ) -> Result<(), WgpuRenderError> {
        let mut first_error = None;
        for window_id in window_ids {
            if let Err(error) = self.retire_surface(*window_id, reason) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn retire_surface(
        &mut self,
        window_id: WindowId,
        reason: SurfaceDropReason,
    ) -> Result<(), WgpuRenderError> {
        let Some(surface) = self.surfaces.remove(&window_id) else {
            return Ok(());
        };
        surface.retire(reason)
    }

    fn render_window(
        &mut self,
        window_id: WindowId,
        clear_color: Color,
        draw: &PreparedSubmitterDraw,
        pass_steps: &[RenderPassStep],
    ) -> Result<bool, WgpuRenderError> {
        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?
            .clone();
        let queue = self
            .queue
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?
            .clone();
        let surface_state = self
            .surfaces
            .get_mut(&window_id)
            .ok_or(WgpuRenderError::SurfaceMissing { window_id })?;

        match surface_state.surface(window_id)?.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => {
                render_acquired_texture(
                    &device,
                    &queue,
                    window_id,
                    surface_state,
                    texture,
                    clear_color,
                    draw,
                    pass_steps,
                )?;
                Ok(true)
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                drop(texture);
                surface_state.dirty = true;
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                surface_state.dirty = true;
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.retire_surface(window_id, SurfaceDropReason::SurfaceOrDeviceLost)?;
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                Err(WgpuRenderError::SurfaceValidation { window_id })
            }
        }
    }

    fn create_surface_if_missing(
        &mut self,
        window: &Window,
        handles: &BackendWindowHandles,
        size: Extent2d,
    ) -> Result<bool, WgpuRenderError> {
        if !self.surfaces.contains_key(&window.id) {
            let binding = match handles.acquire_surface(window.id) {
                Ok(binding) => binding,
                Err(
                    WindowTargetError::UnknownWindow { .. }
                    | WindowTargetError::SurfaceActivationAfterRetirement { .. },
                ) => {
                    return Ok(false);
                }
                Err(error) => return Err(error.into()),
            };
            let instance = self
                .instance
                .as_ref()
                .ok_or(WgpuRenderError::BackendNotReady)?;
            let (handle_source, target) = binding.into_parts();
            let surface = create_surface(instance, handle_source, window.id)?;
            self.surfaces
                .insert(window.id, WgpuSurfaceState::new(surface, target, size));
        }

        Ok(true)
    }

    fn configure_surface_if_needed(
        &mut self,
        window: &Window,
        size: Extent2d,
    ) -> Result<(), WgpuRenderError> {
        let adapter = self
            .adapter
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        let surface = self
            .surfaces
            .get_mut(&window.id)
            .ok_or(WgpuRenderError::SurfaceMissing {
                window_id: window.id,
            })?;
        if surface_resize_action(surface.size, size) == SurfaceResizeAction::Reconfigure(size)
            || surface.dirty
        {
            configure_surface(
                surface,
                adapter,
                device,
                window.id,
                window.present_mode,
                size,
            )?;
        }
        Ok(())
    }

    fn surface_view_format(
        &self,
        window_id: WindowId,
    ) -> Result<wgpu::TextureFormat, WgpuRenderError> {
        let surface = self
            .surfaces
            .get(&window_id)
            .ok_or(WgpuRenderError::SurfaceMissing { window_id })?;
        let config = surface
            .config
            .as_ref()
            .ok_or(WgpuRenderError::SurfaceUnconfigured { window_id })?;
        Ok(config.format.add_srgb_suffix())
    }

    fn prepare_submitter_draw(
        &mut self,
        view_index: usize,
        view_format: wgpu::TextureFormat,
        submitters: SubmitterInputs<'_>,
        frame_index: u64,
    ) -> Result<PreparedSubmitterDraw, WgpuRenderError> {
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        {
            let batches = submitters.quad_batches(view_index);
            let stats = quad_batch_draw_stats(&batches);
            if batches.is_empty() {
                return Ok(PreparedSubmitterDraw::default());
            }
            let alpha_modes = crate::required_alpha_modes(&batches);
            let texture_layout = self.ensure_quad_texture_bind_group_layout()?;
            for alpha_mode in &alpha_modes {
                self.ensure_quad_pipeline(WgpuQuadPipelineKey {
                    format: view_format,
                    alpha_mode: *alpha_mode,
                })?;
            }
            let pipelines = self.quad_pipeline_draw_refs(view_format, &alpha_modes)?;
            let device = self
                .device
                .as_ref()
                .ok_or(WgpuRenderError::BackendNotReady)?
                .clone();
            let queue = self
                .queue
                .as_ref()
                .ok_or(WgpuRenderError::BackendNotReady)?
                .clone();
            let buffers = create_quad_batch_buffers(
                &device,
                &queue,
                &batches,
                &texture_layout,
                &mut self.quad_textures,
                submitters.images,
                submitters.prepared_images,
                frame_index,
            )
            .map_err(|error| WgpuRenderError::QuadTexture {
                message: error.to_string(),
            })?;
            return Ok(PreparedSubmitterDraw {
                pipelines,
                buffers,
                draw_calls: stats.draw_calls,
                sprites: stats.sprites,
            });
        }

        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        {
            let _ = (view_index, view_format, submitters, frame_index);
            Ok(PreparedSubmitterDraw::default())
        }
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn ensure_quad_texture_bind_group_layout(
        &mut self,
    ) -> Result<wgpu::BindGroupLayout, WgpuRenderError> {
        if let Some(layout) = &self.quad_texture_bind_group_layout {
            return Ok(layout.clone());
        }
        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        let layout = create_quad_texture_bind_group_layout(device);
        self.quad_texture_bind_group_layout = Some(layout.clone());
        Ok(layout)
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn ensure_quad_pipeline(&mut self, key: WgpuQuadPipelineKey) -> Result<(), WgpuRenderError> {
        if self
            .quad_pipelines
            .iter()
            .any(|pipeline| pipeline.key == key)
        {
            return Ok(());
        }
        let texture_layout = self.ensure_quad_texture_bind_group_layout()?;
        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        self.quad_pipelines
            .push(create_quad_pipeline(device, key, &texture_layout));
        Ok(())
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn quad_pipeline_draw_refs(
        &self,
        format: wgpu::TextureFormat,
        alpha_modes: &[AlphaMode2d],
    ) -> Result<Vec<WgpuQuadPipelineDrawRef>, WgpuRenderError> {
        alpha_modes
            .iter()
            .map(|alpha_mode| {
                let key = WgpuQuadPipelineKey {
                    format,
                    alpha_mode: *alpha_mode,
                };
                let pipeline = self
                    .quad_pipelines
                    .iter()
                    .find(|pipeline| pipeline.key == key)
                    .ok_or_else(|| WgpuRenderError::QuadPipelineMissing {
                        format: format!("{format:?}"),
                        alpha_mode: format!("{alpha_mode:?}"),
                    })?;
                Ok(WgpuQuadPipelineDrawRef {
                    alpha_mode: *alpha_mode,
                    pipeline: pipeline.pipeline.clone(),
                })
            })
            .collect()
    }

    pub(super) fn mark_error(&mut self, error: &WgpuRenderError) -> String {
        let cleanup_error = self
            .clear_gpu_resources(SurfaceDropReason::SurfaceOrDeviceLost)
            .err();
        self.state = WgpuBackendState::Unavailable;
        let message = match cleanup_error {
            Some(cleanup_error) => {
                format!("{error}; device invalidation cleanup failed: {cleanup_error}")
            }
            None => error.to_string(),
        };
        self.last_error = Some(message.clone());
        message
    }
}

fn render_backend_state(state: WgpuBackendState) -> RenderBackendState {
    match state {
        WgpuBackendState::Uninitialized => RenderBackendState::Uninitialized,
        WgpuBackendState::Initializing => RenderBackendState::Initializing,
        WgpuBackendState::Ready => RenderBackendState::Ready,
        WgpuBackendState::Unavailable => RenderBackendState::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use nara_ecs::{Schedule, World};
    use nara_render::{ExtractedView, RenderFrameState, RenderTarget, ViewportRect};
    use nara_window::backend::{WindowHandleProvider, WindowTargetPhase};
    use raw_window_handle::{
        DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
    };

    use super::*;

    #[derive(Debug)]
    struct TestWindowSource;

    impl HasWindowHandle for TestWindowSource {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            Err(HandleError::NotSupported)
        }
    }

    impl HasDisplayHandle for TestWindowSource {
        fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
            Err(HandleError::NotSupported)
        }
    }

    fn backend_with_lease_only_surface(
        window_id: WindowId,
    ) -> (WgpuRenderBackend, BackendWindowHandles) {
        let handles = BackendWindowHandles::default();
        handles
            .insert(
                window_id,
                WindowHandleProvider::new(Arc::new(TestWindowSource)),
            )
            .unwrap();
        let binding = handles.acquire_surface(window_id).unwrap();
        let surface = WgpuSurfaceState::lease_only(
            binding,
            Extent2d::new(320, 180).expect("non-zero test extent"),
        );
        let mut backend = WgpuRenderBackend::default();
        backend.surfaces.insert(window_id, surface);
        (backend, handles)
    }

    fn insert_render_resources(world: &mut World, backend: WgpuRenderBackend) {
        world.insert_resource(backend);
        world.init_resource::<ExtractedViews>();
        world.init_resource::<RenderFrame>();
        world.init_resource::<FrameStats>();
        world.init_resource::<RenderBackendStatus>();
    }

    fn run_render_system(world: &mut World) {
        let mut schedule = Schedule::default();
        schedule.add_systems(crate::render_wgpu_surfaces);
        schedule.run(world);
    }

    #[test]
    fn backend_starts_without_surfaces_or_native_state() {
        let backend = WgpuRenderBackend::default();
        assert_eq!(backend.state(), WgpuBackendState::Uninitialized);
        assert_eq!(backend.surface_count(), 0);
        assert_eq!(backend.last_error(), None);
    }

    #[test]
    fn surface_loss_drops_the_live_owner_without_retiring_the_target() {
        let (mut backend, handles) = backend_with_lease_only_surface(WindowId::PRIMARY);

        backend
            .retire_surface(WindowId::PRIMARY, SurfaceDropReason::SurfaceOrDeviceLost)
            .unwrap();

        let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
        assert_eq!(snapshot.phase, WindowTargetPhase::Active);
        assert!(!snapshot.surface_active);
        assert!(snapshot.provider_present);
        assert!(handles.is_surface_target_active(WindowId::PRIMARY));
    }

    #[test]
    fn render_system_device_loss_invalidates_surfaces_and_reports_backend_failure() {
        let (mut backend, handles) = backend_with_lease_only_surface(WindowId::PRIMARY);
        let signal = Arc::new(DeviceLossSignal::default());
        backend.state = WgpuBackendState::Ready;
        backend.device_loss_signal = Some(Arc::clone(&signal));
        let mut world = World::new();
        insert_render_resources(&mut world, backend);
        world.insert_resource(handles.clone());
        signal.mark_lost();

        run_render_system(&mut world);

        let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
        assert_eq!(snapshot.phase, WindowTargetPhase::Active);
        assert!(!snapshot.surface_active);
        assert!(snapshot.provider_present);
        assert!(handles.is_surface_target_active(WindowId::PRIMARY));
        let backend = world.resource::<WgpuRenderBackend>();
        assert_eq!(backend.state(), WgpuBackendState::Unavailable);
        assert_eq!(backend.surface_count(), 0);
        assert_eq!(backend.last_error(), Some("wgpu device was lost"));
        assert!(backend.device_loss_signal.is_none());
        let status = world.resource::<RenderBackendStatus>();
        assert_eq!(status.state(), RenderBackendState::Unavailable);
        assert_eq!(status.last_error(), Some("wgpu device was lost"));
        assert_eq!(
            status.last_skip().map(|skip| skip.reason()),
            Some(RenderFrameSkipReason::BackendError)
        );
        assert_eq!(
            world.resource::<RenderFrame>().state,
            RenderFrameState::Skipped
        );
    }

    #[test]
    fn surface_creation_error_releases_the_binding_for_retry() {
        let handles = BackendWindowHandles::default();
        handles
            .insert(
                WindowId::PRIMARY,
                WindowHandleProvider::new(Arc::new(TestWindowSource)),
            )
            .unwrap();
        let mut backend = WgpuRenderBackend {
            instance: Some(wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            )),
            ..WgpuRenderBackend::default()
        };

        assert!(matches!(
            backend.create_surface_if_missing(
                &Window::default(),
                &handles,
                Extent2d::new(320, 180).unwrap(),
            ),
            Err(WgpuRenderError::SurfaceCreation {
                window_id: WindowId::PRIMARY,
                ..
            })
        ));

        let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
        assert_eq!(backend.surface_count(), 0);
        assert!(!snapshot.surface_active);
        assert!(snapshot.provider_present);
        assert!(handles.acquire_surface(WindowId::PRIMARY).is_ok());
    }

    #[test]
    fn unregistered_render_target_skips_without_poisoning_the_backend() {
        let mut world = World::new();
        let window_entity = world.spawn(Window::default()).id();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: window_entity,
            target: RenderTarget::PrimaryWindow,
            viewport: ViewportRect::new(0, 0, 1280, 720).unwrap(),
            world_position: nara_core::Vec2::ZERO,
            viewport_height: 720.0,
            order: 0,
            clear_color: Color::BLACK,
        });
        let backend = WgpuRenderBackend {
            state: WgpuBackendState::Ready,
            ..WgpuRenderBackend::default()
        };
        insert_render_resources(&mut world, backend);
        world.insert_resource(views);
        world.insert_resource(BackendWindowHandles::default());

        run_render_system(&mut world);

        let backend = world.resource::<WgpuRenderBackend>();
        assert_eq!(backend.state(), WgpuBackendState::Ready);
        assert_eq!(backend.surface_count(), 0);
        assert_eq!(backend.last_error(), None);
        let status = world.resource::<RenderBackendStatus>();
        assert_eq!(status.state(), RenderBackendState::Ready);
        assert_eq!(status.last_error(), None);
        assert_eq!(
            status.last_skip().map(|skip| skip.reason()),
            Some(RenderFrameSkipReason::NoRenderableTarget)
        );
        assert_eq!(
            world.resource::<RenderFrame>().state,
            RenderFrameState::Skipped
        );
    }

    #[test]
    fn scoped_target_retirement_leaves_foreign_surface_live() {
        let owned = WindowId::PRIMARY;
        let foreign = WindowId::new(2);
        let handles = BackendWindowHandles::default();
        for window_id in [owned, foreign] {
            handles
                .insert(
                    window_id,
                    WindowHandleProvider::new(Arc::new(TestWindowSource)),
                )
                .unwrap();
        }
        let mut backend = WgpuRenderBackend::default();
        for window_id in [owned, foreign] {
            backend.surfaces.insert(
                window_id,
                WgpuSurfaceState::lease_only(
                    handles.acquire_surface(window_id).unwrap(),
                    Extent2d::new(320, 180).unwrap(),
                ),
            );
        }
        handles.request_retirement(owned).unwrap();

        backend
            .retire_targets(&[owned], SurfaceDropReason::TargetShutdown)
            .unwrap();

        assert_eq!(
            handles.snapshot(owned).unwrap().phase,
            WindowTargetPhase::SurfaceRetired
        );
        let foreign_snapshot = handles.snapshot(foreign).unwrap();
        assert_eq!(foreign_snapshot.phase, WindowTargetPhase::Active);
        assert!(foreign_snapshot.surface_active);
        assert_eq!(backend.surface_count(), 1);
    }

    #[test]
    fn backend_error_cleanup_discards_partial_instance_initialization() {
        let mut backend = WgpuRenderBackend {
            state: WgpuBackendState::Initializing,
            instance: Some(wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            )),
            ..WgpuRenderBackend::default()
        };

        backend.mark_error(&WgpuRenderError::BackendNotReady);

        assert_eq!(backend.state(), WgpuBackendState::Unavailable);
        assert!(backend.instance.is_none());
        assert!(backend.adapter.is_none());
        assert!(backend.device.is_none());
        assert!(backend.queue.is_none());
    }

    #[test]
    fn unavailable_backend_does_not_reenter_device_initialization() {
        let mut backend = WgpuRenderBackend {
            state: WgpuBackendState::Unavailable,
            last_error: Some("device lost".to_owned()),
            ..WgpuRenderBackend::default()
        };

        assert_eq!(
            backend.ensure_device(),
            Err(WgpuRenderError::BackendUnavailable)
        );
        assert_eq!(backend.state(), WgpuBackendState::Unavailable);
        assert_eq!(backend.last_error(), Some("device lost"));
        assert!(backend.instance.is_none());
        assert!(backend.adapter.is_none());
        assert!(backend.device.is_none());
        assert!(backend.queue.is_none());
    }
}
