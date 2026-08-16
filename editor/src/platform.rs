use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use egui::{Color32, Pos2, Rect, TextureId, Vec2, ViewportId, epaint::Primitive};
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor, wgpu};
use nara::{
    asset::{AssetId, Handle},
    ecs::Entity,
    render::{
        __private::{EditorRenderGeneration, build_editor_render_frame_packet},
        Color, ExtractedView, ExtractedViews, RenderImage2d, RenderTarget, RenderWindowPacket,
        ViewportRect,
    },
    render_wgpu::__private::{
        WgpuEditorDeviceScope, WgpuEditorExecution, WgpuEditorExecutionError,
        WgpuSubmittedImageReceipt,
    },
    window::{
        PresentMode, WindowId as NaraWindowId, WindowResolution, backend::WindowHandleProvider,
    },
};
use thiserror::Error;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    error::EventLoopError,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use crate::{EditorApplication, EditorApplicationOpenError, EditorLaunchOptions};

const EDITOR_TITLE: &str = "Nara Editor";
const INITIAL_WIDTH: f64 = 1200.0;
const INITIAL_HEIGHT: f64 = 800.0;
const MIN_WIDTH: f64 = 720.0;
const MIN_HEIGHT: f64 = 520.0;
const CLEAR_COLOR: [f32; 4] = [0.055, 0.06, 0.075, 1.0];
const TRACER_EXTENT: u32 = 64;

#[derive(Debug, Error)]
pub enum EditorRunError {
    #[error("{0}")]
    Usage(String),
    #[error(transparent)]
    Open(#[from] EditorApplicationOpenError),
    #[error("failed to create or drive the native event loop: {0}")]
    EventLoop(String),
    #[error("native Editor platform failed: {0}")]
    Platform(String),
    #[error("native Editor GPU execution failed: {0}")]
    Gpu(#[source] WgpuEditorExecutionError),
}

impl EditorRunError {
    #[must_use]
    pub fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    #[must_use]
    pub fn diagnostics(&self) -> Option<&nara::diagnostic::DiagnosticReport> {
        match self {
            Self::Open(error) => error.diagnostics(),
            Self::Usage(_) | Self::EventLoop(_) | Self::Platform(_) | Self::Gpu(_) => None,
        }
    }

    fn requires_gpu_replacement(&self) -> bool {
        matches!(self, Self::Gpu(error) if error.requires_owner_replacement())
    }
}

pub fn run(options: EditorLaunchOptions) -> Result<(), EditorRunError> {
    let application = EditorApplication::open(options)?;
    let event_loop = EventLoop::new().map_err(event_loop_error)?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut host = NativeEditorHost::new(application)?;
    if let Err(error) = event_loop.run_app(&mut host) {
        host.terminal_error
            .get_or_insert_with(|| event_loop_error(error));
    }
    match host.terminal_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn event_loop_error(error: EventLoopError) -> EditorRunError {
    EditorRunError::EventLoop(error.to_string())
}

struct NativeEditorHost {
    application: EditorApplication,
    context: egui::Context,
    window: Option<Arc<Window>>,
    egui_state: Option<egui_winit::State>,
    gpu: Option<WgpuEditorExecution>,
    renderer: Option<EditorEguiRenderer>,
    pending_textures: egui::TexturesDelta,
    scene_tracer: EditorSceneTracer,
    editor_frame_index: u64,
    last_frame: Instant,
    repaint_deadline: Option<Instant>,
    terminal_error: Option<EditorRunError>,
}

impl NativeEditorHost {
    fn new(application: EditorApplication) -> Result<Self, EditorRunError> {
        Ok(Self {
            application,
            context: egui::Context::default(),
            window: None,
            egui_state: None,
            gpu: None,
            renderer: None,
            pending_textures: egui::TexturesDelta::default(),
            scene_tracer: EditorSceneTracer::new()?,
            editor_frame_index: 0,
            last_frame: Instant::now(),
            repaint_deadline: None,
            terminal_error: None,
        })
    }

    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), EditorRunError> {
        if self.window.is_some() {
            return Ok(());
        }
        let attributes = Window::default_attributes()
            .with_title(EDITOR_TITLE)
            .with_inner_size(LogicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT))
            .with_min_inner_size(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| EditorRunError::Platform(error.to_string()))?,
        );
        let (gpu, renderer, egui_state) =
            create_editor_gpu_namespace(&self.context, Arc::clone(&window))?;

        window.request_redraw();
        self.window = Some(window);
        self.egui_state = Some(egui_state);
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.last_frame = Instant::now();
        Ok(())
    }

    fn replace_gpu_execution(&mut self) -> Result<(), EditorRunError> {
        let window =
            self.window.as_ref().cloned().ok_or_else(|| {
                EditorRunError::Platform("Editor window is unavailable".to_owned())
            })?;
        self.renderer = None;
        self.gpu = None;
        self.scene_tracer.retire_gpu_binding();
        self.pending_textures.clear();
        self.context = egui::Context::default();
        let (gpu, renderer, egui_state) =
            create_editor_gpu_namespace(&self.context, Arc::clone(&window))?;
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.egui_state = Some(egui_state);
        self.last_frame = Instant::now();
        window.request_redraw();
        Ok(())
    }

    fn handle_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) -> Result<(), EditorRunError> {
        let Some(window) = self.window.as_ref() else {
            return Ok(());
        };
        if window.id() != window_id {
            return Ok(());
        }

        if self
            .egui_state
            .as_mut()
            .expect("an installed native window has egui platform state")
            .on_window_event(window, &event)
            .repaint
        {
            window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => {
                let update = self.application.request_exit();
                if update.exit_ready() {
                    event_loop.exit();
                } else {
                    window.request_redraw();
                }
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    if let (Some(gpu), Some(renderer)) = (self.gpu.as_mut(), self.renderer.as_mut())
                    {
                        self.scene_tracer.retire_target(gpu, renderer);
                    }
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop)?,
            WindowEvent::Destroyed => {
                self.window = None;
                self.egui_state = None;
            }
            _ => {}
        }
        Ok(())
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<(), EditorRunError> {
        let window = self
            .window
            .as_ref()
            .expect("redraw is emitted only for the installed window");
        let raw_input = self
            .egui_state
            .as_mut()
            .expect("an installed native window has egui platform state")
            .take_egui_input(window);
        let window_packet = editor_window_packet(window);
        if window_packet.resolution.physical_width == 0
            || window_packet.resolution.physical_height == 0
        {
            return Ok(());
        }
        let prepared_scene = self.scene_tracer.prepare(
            self.gpu
                .as_mut()
                .expect("an installed native window has one Editor GPU owner"),
            self.renderer
                .as_mut()
                .expect("an installed native window has one egui renderer"),
            window_packet,
        )?;
        let mut batch = None;
        let output = self.context.run_ui(raw_input, |ui| {
            batch = Some(self.application.show(ui));
            let sample_rect = Rect::from_min_size(ui.max_rect().left_top(), Vec2::splat(1.0));
            ui.painter().image(
                prepared_scene.texture_id,
                sample_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        });
        let repaint_delay = output
            .viewport_output
            .get(&ViewportId::ROOT)
            .map_or(Duration::MAX, |viewport| viewport.repaint_delay);
        self.egui_state
            .as_mut()
            .expect("an installed native window has egui platform state")
            .handle_platform_output_with_event_loop(window, event_loop, output.platform_output);
        let primitives = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        self.pending_textures.append(output.textures_delta);
        self.editor_frame_index = self
            .editor_frame_index
            .checked_add(1)
            .ok_or_else(|| EditorRunError::Platform("Editor frame index exhausted".to_owned()))?;
        let editor_frame_index = self.editor_frame_index;
        let gpu = self
            .gpu
            .as_mut()
            .expect("an installed native window has one Editor GPU owner");
        let renderer = self
            .renderer
            .as_mut()
            .expect("an installed native window has one egui renderer");
        if !renderer.matches(gpu) {
            return Err(EditorRunError::Platform(
                "the egui renderer does not belong to the live Editor GPU namespace".to_owned(),
            ));
        }
        let sampled_scene = primitives_sample_texture(&primitives, prepared_scene.texture_id);
        let pending_textures = &mut self.pending_textures;
        let mut freed_textures = Vec::new();
        let presentation = gpu.present_window(
            window_packet,
            editor_frame_index,
            Some(prepared_scene.submission),
            |frame| {
                if renderer.surface_format != frame.surface_format() {
                    return Err(WgpuEditorExecutionError::toolkit_paint(
                        "the Editor surface format changed outside the renderer namespace",
                    ));
                }
                let extent = frame.surface_extent();
                let screen = ScreenDescriptor {
                    size_in_pixels: [extent.width, extent.height],
                    pixels_per_point: output.pixels_per_point,
                };
                let command_buffers = frame.encode(|device, queue, encoder, target_view| {
                    for (id, image_deltas) in pending_textures.set.drain() {
                        for image_delta in image_deltas {
                            renderer
                                .renderer
                                .update_texture(device, queue, id, &image_delta);
                        }
                    }
                    let command_buffers = renderer.renderer.update_buffers(
                        device,
                        queue,
                        encoder,
                        &primitives,
                        &screen,
                    );
                    let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("nara_editor_egui_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: target_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: CLEAR_COLOR[0] as f64,
                                    g: CLEAR_COLOR[1] as f64,
                                    b: CLEAR_COLOR[2] as f64,
                                    a: CLEAR_COLOR[3] as f64,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    renderer
                        .renderer
                        .render(&mut pass.forget_lifetime(), &primitives, &screen);
                    command_buffers
                });
                frame.prepend_command_buffers(command_buffers);
                if !sampled_scene {
                    return Err(WgpuEditorExecutionError::toolkit_paint(
                        "egui did not sample the exact Editor Scene tracer texture",
                    ));
                }
                frame.record_scene_sample(prepared_scene.texture_registration_generation)?;
                freed_textures.extend(pending_textures.free.drain());
                Ok(())
            },
            || window.pre_present_notify(),
        );
        for id in freed_textures {
            renderer.renderer.free_texture(&id);
        }
        let presentation = presentation.map_err(platform_gpu_error)?;
        if presentation.window().is_some() && presentation.composite().is_none() {
            return Err(EditorRunError::Platform(
                "the Editor window presented without the exact Scene tracer sample".to_owned(),
            ));
        }

        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        let update = self.application.submit_and_drive(
            batch.expect("the Editor panel always returns one action batch"),
            delta,
        );
        if update.exit_ready() {
            event_loop.exit();
            return Ok(());
        }
        if update.actions_observed() || repaint_delay.is_zero() {
            window.request_redraw();
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if repaint_delay < Duration::MAX {
            let deadline = now + repaint_delay;
            self.repaint_deadline = Some(deadline);
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
        Ok(())
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: EditorRunError) {
        self.terminal_error.get_or_insert(error);
        event_loop.exit();
    }
}

impl ApplicationHandler for NativeEditorHost {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. })
            && self.repaint_deadline.take().is_some()
        {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.initialize(event_loop) {
            self.fail(event_loop, error);
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        let _ = event_loop;
        self.renderer = None;
        self.gpu = None;
        self.scene_tracer.retire_gpu_binding();
        self.pending_textures.clear();
        self.window = None;
        self.egui_state = None;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Err(error) = self.handle_window_event(event_loop, window_id, event) {
            if error.requires_gpu_replacement() {
                if let Err(replacement_error) = self.replace_gpu_execution() {
                    self.fail(event_loop, replacement_error);
                }
            } else {
                self.fail(event_loop, error);
            }
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {}
}

#[derive(Clone, Copy)]
struct PreparedEditorScene {
    submission: WgpuSubmittedImageReceipt,
    texture_id: TextureId,
    texture_registration_generation: u64,
}

#[derive(Clone, Copy)]
struct EditorSceneTextureBinding {
    texture_id: TextureId,
    target_generation: u64,
    backend_instance_id: u64,
    device_epoch: u64,
    registration_generation: u64,
}

impl EditorSceneTextureBinding {
    fn matches(self, submission: WgpuSubmittedImageReceipt) -> bool {
        self.target_generation == submission.target_generation()
            && self.backend_instance_id == submission.backend_instance_id()
            && self.device_epoch == submission.device_epoch()
    }
}

struct EditorSceneTracer {
    generation: EditorRenderGeneration,
    image: Handle<RenderImage2d>,
    frame_index: u64,
    texture_registration_generation: u64,
    binding: Option<EditorSceneTextureBinding>,
}

impl EditorSceneTracer {
    fn new() -> Result<Self, EditorRunError> {
        let generation = EditorRenderGeneration::try_acquire().ok_or_else(|| {
            EditorRunError::Platform("Editor preview render generations are exhausted".to_owned())
        })?;
        Ok(Self {
            generation,
            image: Handle::new(AssetId::from_raw(1)),
            frame_index: 0,
            texture_registration_generation: 0,
            binding: None,
        })
    }

    fn prepare(
        &mut self,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut EditorEguiRenderer,
        window: RenderWindowPacket,
    ) -> Result<PreparedEditorScene, EditorRunError> {
        if !renderer.matches(gpu) {
            return Err(EditorRunError::Platform(
                "the Scene tracer renderer does not belong to the live GPU namespace".to_owned(),
            ));
        }
        self.frame_index = self.frame_index.checked_add(1).ok_or_else(|| {
            EditorRunError::Platform("Scene tracer frame index exhausted".to_owned())
        })?;
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(self.image),
            viewport: ViewportRect::new(0, 0, TRACER_EXTENT, TRACER_EXTENT)
                .expect("the fixed Editor tracer extent is non-zero"),
            world_position: nara::core::Vec2::ZERO,
            viewport_height: TRACER_EXTENT as f32,
            order: 0,
            clear_color: Color::rgba(0.16, 0.29, 0.42, 1.0),
        });
        let packet = build_editor_render_frame_packet(self.generation, self.frame_index, &views)
            .map_err(|error| EditorRunError::Platform(error.to_string()))?
            .ok_or_else(|| {
                EditorRunError::Platform("the Scene tracer produced no render packet".to_owned())
            })?;
        let submission = gpu
            .submit_image(window, packet)
            .map_err(platform_gpu_error)?;

        let binding = match self.binding.filter(|binding| binding.matches(submission)) {
            Some(binding) => binding,
            None => {
                let registration_generation = self
                    .texture_registration_generation
                    .checked_add(1)
                    .ok_or_else(|| {
                    EditorRunError::Platform(
                        "Scene texture registration generation exhausted".to_owned(),
                    )
                })?;
                let existing_texture_id = self.binding.map(|binding| binding.texture_id);
                let texture_id = gpu
                    .with_scene_texture(window, submission, |scope, texture| {
                        match existing_texture_id {
                            Some(texture_id) => {
                                renderer.renderer.update_egui_texture_from_wgpu_texture(
                                    scope.device(),
                                    texture.view(),
                                    wgpu::FilterMode::Linear,
                                    texture_id,
                                );
                                texture_id
                            }
                            None => renderer.renderer.register_native_texture(
                                scope.device(),
                                texture.view(),
                                wgpu::FilterMode::Linear,
                            ),
                        }
                    })
                    .map_err(platform_gpu_error)?;
                let binding = EditorSceneTextureBinding {
                    texture_id,
                    target_generation: submission.target_generation(),
                    backend_instance_id: submission.backend_instance_id(),
                    device_epoch: submission.device_epoch(),
                    registration_generation,
                };
                self.texture_registration_generation = registration_generation;
                self.binding = Some(binding);
                binding
            }
        };

        Ok(PreparedEditorScene {
            submission,
            texture_id: binding.texture_id,
            texture_registration_generation: binding.registration_generation,
        })
    }

    fn retire_gpu_binding(&mut self) {
        self.binding = None;
    }

    fn retire_target(&mut self, gpu: &mut WgpuEditorExecution, renderer: &mut EditorEguiRenderer) {
        gpu.retire_image_target();
        if let Some(binding) = self.binding.take() {
            renderer.renderer.free_texture(&binding.texture_id);
        }
    }
}

fn primitives_sample_texture(primitives: &[egui::ClippedPrimitive], texture_id: TextureId) -> bool {
    primitives.iter().any(|primitive| {
        matches!(
            &primitive.primitive,
            Primitive::Mesh(mesh) if mesh.texture_id == texture_id
        )
    })
}

struct EditorEguiRenderer {
    renderer: Renderer,
    backend_instance_id: u64,
    device_epoch: u64,
    surface_format: wgpu::TextureFormat,
}

impl EditorEguiRenderer {
    fn new(scope: &WgpuEditorDeviceScope<'_>) -> Self {
        Self {
            renderer: Renderer::new(
                scope.device(),
                scope.surface_format(),
                RendererOptions::default(),
            ),
            backend_instance_id: scope.backend_instance_id(),
            device_epoch: scope.device_epoch(),
            surface_format: scope.surface_format(),
        }
    }

    fn matches(&self, gpu: &WgpuEditorExecution) -> bool {
        self.backend_instance_id == gpu.backend_instance_id()
            && self.device_epoch == gpu.device_epoch()
    }
}

fn create_editor_gpu_namespace(
    context: &egui::Context,
    window: Arc<Window>,
) -> Result<(WgpuEditorExecution, EditorEguiRenderer, egui_winit::State), EditorRunError> {
    let window_packet = editor_window_packet(window.as_ref());
    let mut gpu = WgpuEditorExecution::new(
        NaraWindowId::PRIMARY,
        WindowHandleProvider::new(Arc::clone(&window)),
    )
    .map_err(platform_gpu_error)?;
    let (renderer, max_texture_side) = gpu
        .with_device(window_packet, |scope| {
            (
                EditorEguiRenderer::new(&scope),
                scope.device().limits().max_texture_dimension_2d as usize,
            )
        })
        .map_err(platform_gpu_error)?;
    let egui_state = egui_winit::State::new(
        context.clone(),
        ViewportId::ROOT,
        window.as_ref(),
        Some(window.scale_factor() as f32),
        window.theme(),
        Some(max_texture_side),
    );
    Ok((gpu, renderer, egui_state))
}

fn editor_window_packet(window: &Window) -> RenderWindowPacket {
    let size = window.inner_size();
    RenderWindowPacket {
        id: NaraWindowId::PRIMARY,
        resolution: WindowResolution::new(size.width, size.height)
            .with_scale_factor(window.scale_factor()),
        present_mode: PresentMode::AutoVsync,
    }
}

fn platform_gpu_error(error: WgpuEditorExecutionError) -> EditorRunError {
    EditorRunError::Gpu(error)
}
