use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use egui::{TextureId, Vec2, ViewportId, epaint::Primitive};
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor, wgpu};
use nara::{
    project_host::{EditorPreviewCandidate, EditorPreviewHit},
    render::{Extent2d, RenderWindowPacket},
    render_wgpu::__private::{
        WgpuEditorCompositeReceipt, WgpuEditorDeviceScope, WgpuEditorExecution,
        WgpuEditorExecutionError, WgpuSubmittedImageReceipt,
    },
    scene::SceneAuthoringRevision,
    tooling::{
        EditorAction, EditorActionBatch, EditorDocumentId, EditorFrameModel, EditorNavigationAction,
    },
    tooling_egui::EguiSceneViewInput,
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
    scene_presenter: EditorScenePresenter,
    requested_scene_extent: Option<Extent2d>,
    scene_view_notice: Option<&'static str>,
    gpu_replacement_attempted: bool,
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
            scene_presenter: EditorScenePresenter::new(),
            requested_scene_extent: None,
            scene_view_notice: None,
            gpu_replacement_attempted: false,
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
        self.scene_presenter.retire_gpu_binding();
        self.pending_textures.clear();
        self.context = egui::Context::default();
        let (gpu, renderer, egui_state) =
            create_editor_gpu_namespace(&self.context, Arc::clone(&window))?;
        self.gpu = Some(gpu);
        self.renderer = Some(renderer);
        self.egui_state = Some(egui_state);
        set_requested_scene_extent(
            &mut self.application,
            &mut self.requested_scene_extent,
            None,
        );
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
            WindowEvent::Resized(_) => {
                if let (Some(gpu), Some(renderer)) = (self.gpu.as_mut(), self.renderer.as_mut()) {
                    self.scene_presenter.retire_target(gpu, renderer);
                }
                set_requested_scene_extent(
                    &mut self.application,
                    &mut self.requested_scene_extent,
                    None,
                );
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop)?,
            WindowEvent::Destroyed => {
                self.scene_presenter.retire_gpu_binding();
                set_requested_scene_extent(
                    &mut self.application,
                    &mut self.requested_scene_extent,
                    None,
                );
                self.pending_textures.clear();
                self.renderer = None;
                self.gpu = None;
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
        let editor_frame = self.application.current_frame();
        let mut pick_stamp = self
            .scene_presenter
            .pick_stamp(&editor_frame, self.requested_scene_extent);
        let candidate = self.application.take_scene_preview_candidate();
        let mut scene_retry_pending = false;
        let prepared_scene = match candidate {
            Some(candidate) => match self.scene_presenter.prepare(
                self.gpu
                    .as_mut()
                    .expect("an installed native window has one Editor GPU owner"),
                self.renderer
                    .as_mut()
                    .expect("an installed native window has one egui renderer"),
                window_packet,
                candidate,
            ) {
                Ok(prepared) => Some(prepared),
                Err(EditorRunError::Gpu(error)) if error.is_deferred() => {
                    scene_retry_pending = true;
                    pick_stamp = None;
                    None
                }
                Err(error) => return Err(error),
            },
            None => None,
        };
        let mut scene_view_input = self.scene_presenter.scene_view_input(
            &editor_frame,
            self.requested_scene_extent,
            prepared_scene.as_ref(),
            pick_stamp,
            scene_retry_pending,
        );
        if let Some(notice) = self.scene_view_notice.take() {
            scene_view_input = scene_view_input.with_notice(notice);
        }
        let mut panel_output = None;
        let output = self.context.run_ui(raw_input, |ui| {
            panel_output = Some(self.application.show_with_scene_view(ui, scene_view_input));
        });
        let (mut batch, scene_view_output) = panel_output
            .expect("the Editor panel always returns one output")
            .into_parts();
        if let Some(uv) = scene_view_output.pick_uv() {
            match pick_stamp.map(|stamp| EditorScenePickRequest { stamp, uv }) {
                Some(request) => match self.scene_presenter.resolve_pick(request) {
                    Ok(entity) => {
                        batch = append_editor_action(
                            batch,
                            EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                                document: request.stamp.document,
                                entity,
                            }),
                        );
                    }
                    Err(()) => self.scene_view_notice = Some("Scene View input became stale"),
                },
                None => self.scene_view_notice = Some("Scene View input is unavailable"),
            }
        }
        let desired_scene_extent = physical_scene_extent(
            scene_view_output.desired_size_points(),
            output.pixels_per_point,
        );
        let camera_changed = scene_view_output.camera_changed();
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
        let sampled_scene = prepared_scene
            .as_ref()
            .is_some_and(|scene| primitives_sample_texture(&primitives, scene.texture_id));
        let pending_textures = &mut self.pending_textures;
        let mut freed_textures = Vec::new();
        let presentation = gpu.present_window(
            window_packet,
            editor_frame_index,
            prepared_scene.as_ref().map(|scene| scene.submission),
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
                if let Some(prepared_scene) = prepared_scene.as_ref() {
                    if !sampled_scene {
                        return Err(WgpuEditorExecutionError::toolkit_paint(
                            "egui did not sample the exact Editor Scene texture",
                        ));
                    }
                    frame.record_scene_sample(prepared_scene.texture_registration_generation)?;
                }
                freed_textures.extend(pending_textures.free.drain());
                Ok(())
            },
            || window.pre_present_notify(),
        );
        for id in freed_textures {
            renderer.renderer.free_texture(&id);
        }
        let presentation = presentation.map_err(platform_gpu_error)?;
        if presentation.window().is_some() {
            self.gpu_replacement_attempted = false;
        }
        match (prepared_scene, presentation.composite()) {
            (Some(prepared), Some(receipt)) => {
                self.scene_presenter.promote(prepared, receipt)?;
            }
            (Some(_), None) if presentation.window().is_none() => {
                scene_retry_pending = true;
                self.scene_presenter.retire_presentation();
            }
            (Some(_), None) => {
                return Err(EditorRunError::Platform(
                    "the Editor window presented without the exact Scene sample".to_owned(),
                ));
            }
            (None, Some(_)) => {
                return Err(EditorRunError::Platform(
                    "the Editor GPU published a Scene receipt without a candidate".to_owned(),
                ));
            }
            (None, None) if scene_retry_pending || pick_stamp.is_none() => {
                self.scene_presenter.retire_presentation();
            }
            (None, None) => {}
        }

        if desired_scene_extent != self.requested_scene_extent {
            self.scene_presenter.retire_target(gpu, renderer);
            set_requested_scene_extent(
                &mut self.application,
                &mut self.requested_scene_extent,
                desired_scene_extent,
            );
            scene_retry_pending = true;
        }

        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        let update = self.application.submit_and_drive(batch, delta);
        if update.exit_ready() {
            event_loop.exit();
            return Ok(());
        }
        if update.actions_observed()
            || scene_retry_pending
            || camera_changed
            || repaint_delay.is_zero()
        {
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
        self.scene_presenter.retire_gpu_binding();
        set_requested_scene_extent(
            &mut self.application,
            &mut self.requested_scene_extent,
            None,
        );
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
                if self.gpu_replacement_attempted {
                    self.fail(event_loop, error);
                    return;
                }
                self.gpu_replacement_attempted = true;
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

struct PreparedEditorScene {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    geometry_revision: u64,
    hits: Vec<EditorPreviewHit>,
    submission: WgpuSubmittedImageReceipt,
    texture_id: TextureId,
    texture_registration_generation: u64,
}

struct PresentedEditorScene {
    stamp: EditorScenePickStamp,
    hits: Vec<EditorPreviewHit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EditorScenePickStamp {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    geometry_revision: u64,
    receipt: WgpuEditorCompositeReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct EditorScenePickRequest {
    stamp: EditorScenePickStamp,
    uv: [f32; 2],
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

    fn matches_presented(self, receipt: WgpuEditorCompositeReceipt) -> bool {
        self.matches(receipt.image())
            && self.registration_generation == receipt.texture_registration_generation()
    }
}

struct EditorScenePresenter {
    texture_registration_generation: u64,
    binding: Option<EditorSceneTextureBinding>,
    presented: Option<PresentedEditorScene>,
}

impl EditorScenePresenter {
    fn new() -> Self {
        Self {
            texture_registration_generation: 0,
            binding: None,
            presented: None,
        }
    }

    fn prepare(
        &mut self,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut EditorEguiRenderer,
        window: RenderWindowPacket,
        candidate: EditorPreviewCandidate,
    ) -> Result<PreparedEditorScene, EditorRunError> {
        if !renderer.matches(gpu) {
            return Err(EditorRunError::Platform(
                "the Scene renderer does not belong to the live GPU namespace".to_owned(),
            ));
        }
        let (document, revision, geometry_revision, packet, sprite_batches, hits) =
            candidate.into_parts();
        let submission = gpu
            .submit_sprite_image(window, packet, sprite_batches)
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
            document,
            revision,
            geometry_revision,
            hits,
            submission,
            texture_id: binding.texture_id,
            texture_registration_generation: binding.registration_generation,
        })
    }

    fn pick_stamp(
        &self,
        frame: &EditorFrameModel,
        requested_extent: Option<Extent2d>,
    ) -> Option<EditorScenePickStamp> {
        let presented = self.presented.as_ref()?;
        let binding = self.binding?;
        let extent = presented.stamp.receipt.image().target().extent();
        if requested_extent != Some(extent)
            || !binding.matches_presented(presented.stamp.receipt)
            || !scene_matches_frame(frame, presented.stamp.document, presented.stamp.revision)
        {
            return None;
        }
        Some(presented.stamp)
    }

    fn scene_view_input(
        &self,
        frame: &EditorFrameModel,
        requested_extent: Option<Extent2d>,
        prepared: Option<&PreparedEditorScene>,
        pick_stamp: Option<EditorScenePickStamp>,
        retry_pending: bool,
    ) -> EguiSceneViewInput {
        if retry_pending {
            return EguiSceneViewInput::resizing();
        }
        if let Some(prepared) = prepared {
            let extent = prepared.submission.target().extent();
            if requested_extent == Some(extent)
                && scene_matches_frame(frame, prepared.document, prepared.revision)
            {
                let extent = [extent.width, extent.height];
                return if pick_stamp.is_some() {
                    EguiSceneViewInput::presented(prepared.texture_id, extent)
                } else {
                    EguiSceneViewInput::previewing(prepared.texture_id, extent)
                };
            }
        }
        if let Some(stamp) = pick_stamp {
            let presented = self
                .presented
                .as_ref()
                .expect("pickable Scene View state owns one presented lease");
            debug_assert_eq!(presented.stamp, stamp);
            let binding = self
                .binding
                .expect("pickable Scene View state owns one texture binding");
            let extent = stamp.receipt.image().target().extent();
            return EguiSceneViewInput::presented(
                binding.texture_id,
                [extent.width, extent.height],
            );
        }
        if requested_extent.is_some() {
            EguiSceneViewInput::resizing()
        } else {
            EguiSceneViewInput::unavailable()
        }
    }

    fn resolve_pick(
        &self,
        request: EditorScenePickRequest,
    ) -> Result<Option<nara::scene::SceneEntityId>, ()> {
        let presented = self.presented.as_ref().ok_or(())?;
        if presented.stamp != request.stamp
            || !request.uv.into_iter().all(f32::is_finite)
            || request
                .uv
                .into_iter()
                .any(|value| !(0.0..=1.0).contains(&value))
        {
            return Err(());
        }
        let clip = [request.uv[0] * 2.0 - 1.0, 1.0 - request.uv[1] * 2.0];
        Ok(presented
            .hits
            .iter()
            .rev()
            .find(|hit| hit.contains_clip_point(clip))
            .map(|hit| hit.entity().clone()))
    }

    fn promote(
        &mut self,
        prepared: PreparedEditorScene,
        receipt: WgpuEditorCompositeReceipt,
    ) -> Result<(), EditorRunError> {
        if receipt.image() != prepared.submission
            || receipt.texture_registration_generation() != prepared.texture_registration_generation
        {
            return Err(EditorRunError::Platform(
                "the Editor Scene receipt does not match the prepared candidate".to_owned(),
            ));
        }
        self.presented = Some(PresentedEditorScene {
            stamp: EditorScenePickStamp {
                document: prepared.document,
                revision: prepared.revision,
                geometry_revision: prepared.geometry_revision,
                receipt,
            },
            hits: prepared.hits,
        });
        Ok(())
    }

    fn retire_presentation(&mut self) {
        self.presented = None;
    }

    fn retire_gpu_binding(&mut self) {
        self.binding = None;
        self.presented = None;
    }

    fn retire_target(&mut self, gpu: &mut WgpuEditorExecution, renderer: &mut EditorEguiRenderer) {
        gpu.retire_image_target();
        self.presented = None;
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

fn scene_matches_frame(
    frame: &EditorFrameModel,
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
) -> bool {
    frame
        .workspace()
        .active_scene
        .as_ref()
        .is_some_and(|scene| scene.document == document && scene.revision == revision)
}

fn append_editor_action(batch: EditorActionBatch, action: EditorAction) -> EditorActionBatch {
    let revision = batch.frame_revision();
    let mut actions = batch.into_actions();
    actions.push(action);
    EditorActionBatch::new(revision, actions)
}

fn set_requested_scene_extent(
    application: &mut EditorApplication,
    requested_extent: &mut Option<Extent2d>,
    extent: Option<Extent2d>,
) {
    *requested_extent = extent;
    application.set_scene_preview_extent(extent);
}

fn physical_scene_extent(points: Vec2, pixels_per_point: f32) -> Option<Extent2d> {
    if !points.is_finite() || !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
        return None;
    }
    let width = (points.x * pixels_per_point).round() as u32;
    let height = (points.y * pixels_per_point).round() as u32;
    Extent2d::new(width, height)
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
