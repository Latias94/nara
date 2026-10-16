use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use dear_imgui_rs::{Context, FontConfig, FontSource, StbTrueTypeFontData};
use dear_imgui_wgpu::{
    ExternalTextureId, WgpuInitInfo, WgpuRenderer,
    multi_viewport::{WgpuPreparedViewportFrame, WinitViewportRoute},
    wgpu,
};
use dear_imgui_winit::{HiDpiMode, WinitPlatform};
use nara::{
    asset::{AssetId, Handle},
    project_host::{
        EditorGameStartRequest, EditorGameViewCandidateIdentity, EditorGameViewCompletion,
        EditorPreviewPresentation, TrustedCompiledEditorProjectIntent,
    },
    render::{
        __private::{editor_viewport_image_target, map_editor_game_view_packet},
        Extent2d, RenderFrameSourceGeneration, RenderFrameTarget, RenderImagePacket,
        RenderWindowPacket,
    },
    render_wgpu::__private::{
        WgpuEditorExecution, WgpuEditorExecutionError, WgpuEditorFrameOutcome,
        WgpuEditorUiImageSubmission, WgpuEditorViewId, WgpuEditorViewportIdentity,
        WgpuEditorWindowFrame, WgpuSubmittedImageReceipt,
    },
    tooling::{EditorAction, EditorActionBatch, EditorPlayAction, EditorPlayState},
    window::{WindowId as NaraWindowId, backend::WindowHandleProvider},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use crate::frontend::imgui::{ImguiEditorFrontend, ImguiViewImage};
use crate::{EditorApplication, EditorLaunchOptions};

use super::{
    EditorGpuRecovery, EditorRunError, earliest_deadline, editor_window_packet,
    explicit_gpu_retry_requested, image_import_poll_deadline, platform_gpu_error,
    play_lifecycle_poll_deadline,
};

mod input;
use super::viewport::EditorGamePresentedStamp;
use input::ImguiInput;

const GAME_TARGET_ID: u64 = 2;

/// Runs the explicitly selected qualification slice without changing the production frontend.
#[doc(hidden)]
pub fn run_imgui_qualification(
    options: EditorLaunchOptions,
    intent: Option<TrustedCompiledEditorProjectIntent>,
) -> Result<(), EditorRunError> {
    let mut application = match intent {
        Some(intent) => EditorApplication::open_trusted_compiled(options, intent)?,
        None => EditorApplication::new(options),
    };
    if std::env::var_os("NARA_IMGUI_QUALIFICATION_CANARY").is_none()
        && let Some(preferences) = crate::workspace::EditorWorkspacePreferences::user_default()
    {
        application
            .panel_workspace_mut()
            .set_preferences(preferences);
    }
    let event_loop =
        EventLoop::new().map_err(|error| EditorRunError::EventLoop(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut host = ImguiQualificationHost {
        application,
        native: None,
        window: None,
        retained: None,
        gpu_recovery: EditorGpuRecovery::Ready,
        error: None,
        last_drive: Instant::now(),
        repaint_deadline: None,
        metrics: super::qualification_metrics::FrontendMetrics::requested("dear-imgui"),
    };
    if let Err(error) = event_loop.run_app(&mut host) {
        host.error
            .get_or_insert_with(|| EditorRunError::EventLoop(error.to_string()));
    }
    let failed = host.error.is_some();
    let shutdown = host.native.as_mut().map_or(Ok(()), |native| {
        if failed {
            native.shutdown_graphics()
        } else {
            native.shutdown()
        }
    });
    if let Some(error) = host.error {
        if let Err(shutdown) = shutdown {
            return Err(imgui_error(format!(
                "{error}; Editor cleanup also failed: {shutdown}"
            )));
        }
        return Err(error);
    }
    shutdown
}

struct ImguiQualificationHost {
    application: EditorApplication,
    native: Option<NativeImgui>,
    window: Option<Arc<Window>>,
    retained: Option<ImguiRetainedState>,
    gpu_recovery: EditorGpuRecovery,
    error: Option<EditorRunError>,
    last_drive: Instant,
    repaint_deadline: Option<Instant>,
    metrics: Option<super::qualification_metrics::FrontendMetrics>,
}

struct ImguiRetainedState {
    frontend: ImguiEditorFrontend,
    layout: String,
    canary: Option<ImguiNativeCanary>,
    frame_index: u64,
    next_view: u64,
}

struct NativeImgui {
    window: Arc<Window>,
    context: Context,
    platform: WinitPlatform,
    renderer: ImguiRenderer,
    gpu: WgpuEditorExecution,
    frontend: ImguiEditorFrontend,
    scene: ImguiTarget,
    game: ImguiTarget,
    scene_authority: EditorPreviewPresentation,
    frame_index: u64,
    last_frame: Instant,
    status: String,
    detachment: &'static str,
    layout: Option<String>,
    next_view: u64,
    scene_extent: Option<Extent2d>,
    renderer_shutdown: bool,
    platform_shutdown: bool,
    rejected_completion: Option<EditorGameViewCompletion>,
    input: ImguiInput,
    game_candidate: Option<EditorGameViewCandidateIdentity>,
    game_source_active: bool,
    canary: Option<ImguiNativeCanary>,
    layout_loaded: bool,
    presented: bool,
    ui_followup: bool,
    text_trace: bool,
    traced_text: String,
    traced_probe_rect: Option<nara::render::DisplayRect>,
    traced_game: Option<serde_json::Value>,
}

struct ImguiNativeCanary {
    started: Instant,
    fail_after_first_present: bool,
    phase: ImguiCanaryPhase,
    primary_frames: u32,
    secondary_frames: u32,
    minimized_secondary_frames: u32,
    returned_frames: u32,
    game_closed_frames: u32,
    scene_closed_frames: u32,
    reopened_frames: u32,
    recovered_frames: u32,
    retired_backend: Option<u64>,
    first_present_ms: Option<u128>,
    stale_receipt_rejected: bool,
    last_game: Option<WgpuSubmittedImageReceipt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImguiCanaryPhase {
    Primary,
    Detached,
    Minimized,
    RestoringPrimary,
    Returned,
    GameClosed,
    GameReopening,
    SceneClosed,
    Reopened,
    Recovering,
    Recovered,
    Stopping,
    Complete,
}

impl ImguiNativeCanary {
    fn requested() -> Option<Self> {
        let mode = std::env::var("NARA_IMGUI_QUALIFICATION_CANARY").ok()?;
        match mode.as_str() {
            "1" | "fail-after-present" => Some(Self {
                started: Instant::now(),
                fail_after_first_present: mode == "fail-after-present",
                phase: ImguiCanaryPhase::Primary,
                primary_frames: 0,
                secondary_frames: 0,
                minimized_secondary_frames: 0,
                returned_frames: 0,
                game_closed_frames: 0,
                scene_closed_frames: 0,
                reopened_frames: 0,
                recovered_frames: 0,
                retired_backend: None,
                first_present_ms: None,
                stale_receipt_rejected: false,
                last_game: None,
            }),
            _ => None,
        }
    }
}

enum ImguiRenderer {
    Docked(WgpuRenderer),
    Native(WinitViewportRoute),
}

enum ImguiPreparedFrame<'frame> {
    Docked(dear_imgui_rs::render::ReconciledFrame<'frame>),
    Native(WgpuPreparedViewportFrame<'frame>),
}

impl ImguiRenderer {
    fn register(&mut self, view: &wgpu::TextureView) -> Result<ExternalTextureId, EditorRunError> {
        match self {
            Self::Docked(renderer) => renderer
                .register_external_texture(view)
                .map_err(imgui_error),
            Self::Native(route) => route.register_external_texture(view).map_err(imgui_error),
        }
    }

    fn unregister(&mut self, texture: ExternalTextureId) -> Result<(), EditorRunError> {
        match self {
            Self::Docked(renderer) => renderer
                .unregister_external_texture(texture)
                .map_err(imgui_error),
            Self::Native(route) => route
                .unregister_external_texture(texture)
                .map_err(imgui_error),
        }
    }

    fn prepare<'frame>(
        &mut self,
        gpu: &mut WgpuEditorExecution,
        window: RenderWindowPacket,
        event_loop: &ActiveEventLoop,
        frame: dear_imgui_rs::FrameToken<'frame>,
        submissions: &[WgpuSubmittedImageReceipt],
    ) -> Result<(ImguiPreparedFrame<'frame>, Vec<WgpuEditorFrameOutcome>), EditorRunError> {
        match self {
            Self::Docked(renderer) => {
                let consumer = renderer.renderer_consumer().map_err(imgui_error)?;
                let pending = frame.try_render(consumer).map_err(imgui_error)?;
                Ok((
                    ImguiPreparedFrame::Docked(
                        renderer.reconcile_frame(pending).map_err(imgui_error)?,
                    ),
                    Vec::new(),
                ))
            }
            Self::Native(route) => gpu
                .prepare_imgui_viewports(window, route, event_loop, frame, submissions)
                .map(|(frame, secondary)| (ImguiPreparedFrame::Native(frame), secondary))
                .map_err(platform_gpu_error),
        }
    }

    fn paint(
        &mut self,
        prepared: ImguiPreparedFrame<'_>,
        frame: &mut WgpuEditorWindowFrame<'_>,
    ) -> Result<(), WgpuEditorExecutionError> {
        let clear = wgpu::Color {
            r: 0.055,
            g: 0.06,
            b: 0.075,
            a: 1.0,
        };
        match (self, prepared) {
            (Self::Docked(renderer), ImguiPreparedFrame::Docked(prepared)) => {
                frame.paint_imgui_docked(renderer, prepared, clear)
            }
            (Self::Native(route), ImguiPreparedFrame::Native(prepared)) => {
                frame.paint_imgui_main(route, prepared, clear)
            }
            _ => Err(WgpuEditorExecutionError::toolkit_paint(
                "prepared frame belongs to another route",
            )),
        }
    }

    fn shutdown(&mut self, context: &mut Context) -> Result<(), EditorRunError> {
        match self {
            Self::Docked(renderer) => renderer.shutdown(context).map_err(imgui_error),
            Self::Native(route) => route.shutdown(context).map_err(imgui_error),
        }
    }
}

#[derive(Default)]
struct ImguiTarget {
    view: Option<WgpuEditorViewId>,
    identity: Option<WgpuEditorViewportIdentity>,
    source: Option<RenderFrameSourceGeneration>,
    target: Option<RenderImagePacket>,
    texture: Option<ExternalTextureId>,
    submitted: Option<WgpuSubmittedImageReceipt>,
}

impl ImguiTarget {
    fn image(&self) -> Option<ImguiViewImage> {
        let texture = self.texture?;
        let extent = self.target?.extent();
        self.submitted?;
        Some(ImguiViewImage {
            texture: texture.texture_id(),
            extent: [extent.width, extent.height],
        })
    }

    fn ensure(
        &mut self,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut ImguiRenderer,
        window: RenderWindowPacket,
        source: RenderFrameSourceGeneration,
        target: RenderImagePacket,
        next_view: &mut u64,
    ) -> Result<(), EditorRunError> {
        if self.source == Some(source) && self.target == Some(target) && self.identity.is_some() {
            return Ok(());
        }
        let view = match self.view {
            Some(view) => view,
            None => {
                *next_view = next_view
                    .checked_add(1)
                    .ok_or_else(|| imgui_error("render view identity exhausted"))?;
                let view = WgpuEditorViewId::new(*next_view);
                self.view = Some(view);
                view
            }
        };
        let (reservation, texture) = gpu
            .reserve_viewport_target_with_texture(view, window, source, target, |_, texture| {
                renderer.register(texture.view())
            })
            .map_err(platform_gpu_error)?;
        let texture = texture?;
        let identity = reservation.identity();
        if let Err(error) = gpu
            .prepare_viewport_target_activation(reservation)
            .map(|activation| {
                gpu.retire_viewport_target(view);
                gpu.commit_viewport_target_activation(activation);
            })
        {
            renderer.unregister(texture)?;
            return Err(platform_gpu_error(error));
        }
        if let Some(old) = self.texture.replace(texture) {
            renderer.unregister(old)?;
        }
        self.identity = Some(identity);
        self.source = Some(source);
        self.target = Some(target);
        self.submitted = None;
        Ok(())
    }

    fn retire(
        &mut self,
        gpu: &mut WgpuEditorExecution,
        renderer: &mut ImguiRenderer,
    ) -> Result<(), EditorRunError> {
        if let Some(texture) = self.texture.take() {
            renderer.unregister(texture)?;
        }
        if let Some(view) = self.view.take() {
            gpu.retire_view(view);
        }
        self.identity = None;
        self.source = None;
        self.target = None;
        self.submitted = None;
        Ok(())
    }
}

struct PendingGameImage {
    identity: EditorGameViewCandidateIdentity,
    image: WgpuSubmittedImageReceipt,
    ui: Option<WgpuEditorUiImageSubmission>,
}

impl PendingGameImage {
    fn finish(
        self,
        receipt: Option<nara::render_wgpu::__private::WgpuEditorCompositeReceipt>,
    ) -> EditorGameViewCompletion {
        match (self.ui, receipt) {
            (Some(ui), Some(receipt)) => match ui.presented(receipt) {
                Ok(outcomes) => EditorGameViewCompletion::with_ui(self.identity, outcomes),
                Err(ui) => EditorGameViewCompletion::with_ui_failed(
                    self.identity,
                    ui.failed(&WgpuEditorExecutionError::StaleImageSubmission),
                ),
            },
            (Some(ui), None) => EditorGameViewCompletion::with_ui_deferred(
                self.identity,
                ui.deferred(
                    "the exact Game image was not presented in the primary window".to_owned(),
                ),
            ),
            (None, Some(_)) => EditorGameViewCompletion::without_ui(self.identity),
            (None, None) => EditorGameViewCompletion::without_ui_deferred(self.identity),
        }
    }

    fn fail(self, error: &EditorRunError) -> EditorGameViewCompletion {
        let toolkit = WgpuEditorExecutionError::toolkit_paint(error.to_string());
        let error = match error {
            EditorRunError::Gpu(error) => error,
            _ => &toolkit,
        };
        match self.ui {
            Some(ui) if error.is_deferred() => {
                EditorGameViewCompletion::with_ui_deferred(self.identity, ui.failed(error))
            }
            Some(ui) if error.is_content_rejected() => {
                EditorGameViewCompletion::with_ui_content_rejected(self.identity, ui.failed(error))
            }
            Some(ui) => EditorGameViewCompletion::with_ui_failed(self.identity, ui.failed(error)),
            None if error.is_deferred() => {
                EditorGameViewCompletion::without_ui_deferred(self.identity)
            }
            None if error.is_content_rejected() => {
                EditorGameViewCompletion::without_ui_content_rejected(self.identity)
            }
            None => EditorGameViewCompletion::without_ui_failed(self.identity),
        }
    }
}

impl NativeImgui {
    fn new(event_loop: &ActiveEventLoop, window: Arc<Window>) -> Result<Self, EditorRunError> {
        // Context teardown may own deferred renderer resources after an initialization error.
        // Declare the engine GPU owner first so those resources retire before it on every return.
        let mut gpu = WgpuEditorExecution::new(
            NaraWindowId::PRIMARY,
            WindowHandleProvider::new(Arc::clone(&window)),
        )
        .map_err(platform_gpu_error)?;
        let mut context = Context::create();
        context
            .set_ini_filename(None::<std::path::PathBuf>)
            .map_err(imgui_error)?;
        context.io_mut().set_config_flags(
            dear_imgui_rs::ConfigFlags::DOCKING_ENABLE
                | dear_imgui_rs::ConfigFlags::NAV_ENABLE_KEYBOARD,
        );
        context
            .font_atlas()
            .add_font(&[FontSource::default_font_with_size(16.0)]);
        if let Some(path) = std::env::var_os("NARA_EDITOR_CJK_FONT") {
            let data = StbTrueTypeFontData::from_file(std::path::PathBuf::from(path))
                .map_err(imgui_error)?;
            context
                .font_atlas()
                .add_font(&[FontSource::stb_truetype_with_size(data, 16.0)
                    .with_config(FontConfig::new().size_pixels(16.0).merge_mode(true))]);
        }
        let mut platform = WinitPlatform::new(&mut context).map_err(imgui_error)?;
        platform
            .attach_window(Arc::clone(&window), HiDpiMode::Default, &mut context)
            .map_err(imgui_error)?;
        let native_detachment = native_detachment_supported(event_loop);
        if native_detachment {
            context.enable_multi_viewport();
            platform
                .enable_viewports(&mut context)
                .map_err(imgui_error)?;
            context.io_mut().set_config_dpi_scale_fonts(true);
            context.io_mut().set_config_dpi_scale_viewports(true);
        }
        let construction_probe = std::env::var("NARA_IMGUI_QUALIFICATION_CANARY").ok();
        if construction_probe.as_deref() == Some("fail-after-platform") {
            return Err(imgui_error(
                "requested qualification failure after platform attachment",
            ));
        }
        let renderer = gpu
            .with_device(editor_window_packet(&window), |scope| {
                WgpuRenderer::new(
                    WgpuInitInfo::new(
                        scope.device().clone(),
                        scope.queue().clone(),
                        scope.surface_format(),
                    )
                    .with_instance(scope.instance().clone())
                    .with_adapter(scope.adapter().clone()),
                    &mut context,
                )
            })
            .map_err(platform_gpu_error)
            .and_then(|renderer| renderer.map_err(imgui_error));
        let mut renderer = match renderer {
            Ok(renderer) => renderer,
            Err(error) => {
                // Renderer construction may submit initialization work before its first frame.
                // Drain it before Context teardown releases any deferred renderer resources.
                if let Err(cleanup) = gpu.wait_for_idle() {
                    return Err(imgui_error(format!(
                        "{error}; initialization GPU drain also failed: {cleanup}"
                    )));
                }
                return Err(error);
            }
        };
        if construction_probe.as_deref() == Some("fail-after-renderer") {
            return Err(retire_failed_imgui_renderer(
                imgui_error("requested qualification failure after renderer creation"),
                &gpu,
                &mut renderer,
                &mut platform,
                &mut context,
            ));
        }
        renderer.set_gamma_mode(dear_imgui_wgpu::GammaMode::Auto);
        let renderer = if native_detachment {
            match WinitViewportRoute::attach(&mut context, &platform, renderer) {
                Ok(route) => ImguiRenderer::Native(route),
                Err(failure) => {
                    let (error, mut renderer) = failure.into_parts();
                    return Err(retire_failed_imgui_renderer(
                        imgui_error(error),
                        &gpu,
                        &mut renderer,
                        &mut platform,
                        &mut context,
                    ));
                }
            }
        } else {
            ImguiRenderer::Docked(renderer)
        };
        Ok(Self {
            window,
            context,
            platform,
            renderer,
            gpu,
            frontend: ImguiEditorFrontend::default(),
            scene: ImguiTarget::default(),
            game: ImguiTarget::default(),
            scene_authority: EditorPreviewPresentation::new(),
            frame_index: 0,
            last_frame: Instant::now(),
            status: "Qualification has not established frontend parity".to_owned(),
            detachment: if native_detachment {
                "Native detachment: experimental; exact secondary-image proof pending"
            } else {
                "Native detachment unavailable on this profile; in-window docking remains enabled"
            },
            layout: None,
            next_view: 0,
            scene_extent: None,
            renderer_shutdown: false,
            platform_shutdown: false,
            rejected_completion: None,
            input: ImguiInput::default(),
            game_candidate: None,
            game_source_active: false,
            canary: ImguiNativeCanary::requested(),
            layout_loaded: false,
            presented: false,
            ui_followup: true,
            text_trace: std::env::var_os("NARA_IMGUI_TEXT_TRACE").is_some(),
            traced_text: String::new(),
            traced_probe_rect: None,
            traced_game: None,
        })
    }

    fn redraw(
        &mut self,
        event_loop: &ActiveEventLoop,
        application: &mut EditorApplication,
    ) -> Result<bool, EditorRunError> {
        self.presented = false;
        if !self.layout_loaded {
            let displays = self
                .window
                .available_monitors()
                .map(|monitor| {
                    let position = monitor.position();
                    let size = monitor.size();
                    crate::workspace::EditorWindowBounds::new(
                        position.x as f32,
                        position.y as f32,
                        size.width as f32,
                        size.height as f32,
                    )
                })
                .collect::<Vec<_>>();
            application
                .panel_workspace_mut()
                .load_preferences("dear-imgui", 1, &displays);
            self.frontend
                .restore_panel_state(application.panel_workspace().layout());
            if let Some(bounds) = application.panel_workspace().layout().window_bounds() {
                self.window
                    .set_outer_position(winit::dpi::PhysicalPosition::new(
                        bounds.x as i32,
                        bounds.y as i32,
                    ));
                let _ = self
                    .window
                    .request_inner_size(winit::dpi::PhysicalSize::new(
                        bounds.width as u32,
                        bounds.height as u32,
                    ));
            }
            if let Some(saved) = application.panel_workspace().layout().toolkit_layout() {
                self.context.load_ini_settings(saved.data());
                self.layout = Some(saved.data().to_owned());
            }
            self.layout_loaded = true;
        }
        let now = Instant::now();
        self.context.io_mut().set_delta_time(
            now.saturating_duration_since(self.last_frame)
                .as_secs_f32()
                .max(f32::EPSILON),
        );
        self.last_frame = now;
        self.frame_index = self
            .frame_index
            .checked_add(1)
            .ok_or_else(|| imgui_error("host frame exhausted"))?;
        let window_packet = editor_window_packet(&self.window);
        self.gpu
            .begin_editor_frame(window_packet, self.frame_index)
            .map_err(platform_gpu_error)?;
        if !self.frontend.scene_open() {
            self.scene.retire(&mut self.gpu, &mut self.renderer)?;
            self.scene_authority.retire();
        } else if let Some(candidate) = application.take_scene_preview_candidate() {
            let (_, packet, sprites) = self
                .scene_authority
                .accept(candidate)
                .map_err(imgui_error)?;
            let RenderFrameTarget::Image(target) = packet.target() else {
                return Err(imgui_error("Scene preview did not retain an image target"));
            };
            self.scene.ensure(
                &mut self.gpu,
                &mut self.renderer,
                window_packet,
                packet.source_generation(),
                target,
                &mut self.next_view,
            )?;
            let image = self
                .gpu
                .submit_sprite_image(
                    self.scene.view.expect("target has a view"),
                    window_packet,
                    packet,
                    sprites,
                )
                .map_err(platform_gpu_error)?;
            self.scene.submitted = Some(image);
        }
        self.prepare_game_start(application, window_packet)?;
        let pending_game = self.submit_game(application, window_packet)?;
        let pending_identity = pending_game.as_ref().map(|pending| pending.identity);
        if let Some(canary) = &mut self.canary {
            if let Some(old) = canary.last_game.filter(|old| {
                self.game.submitted.is_some_and(|new| new != *old)
                    && self
                        .game
                        .identity
                        .is_some_and(|target| target.matches_submission(*old))
            }) {
                let stale = self.gpu.present_window(
                    window_packet,
                    self.frame_index,
                    &[old],
                    |_| panic!("a stale image receipt reached paint"),
                    || {},
                );
                if !matches!(stale, Err(WgpuEditorExecutionError::StaleImageSubmission)) {
                    if let Some(pending) = pending_game {
                        self.return_game_completion(
                            application,
                            pending.fail(&imgui_error("old image receipt was accepted")),
                        )?;
                    }
                    return Err(imgui_error(
                        "a repeated image allocation accepted a superseded receipt",
                    ));
                }
                canary.stale_receipt_rejected = true;
            }
            canary.last_game = self.game.submitted;
        }
        let mut secondary_outcomes = Vec::new();
        let mut secondary_windows = Vec::new();
        let primary_viewport = self.context.main_viewport().id();
        let rendered = (|| {
            self.platform
                .prepare_frame(&mut self.context, &self.window)
                .map_err(imgui_error)?;
            let frame = self.context.begin_frame();
            let output = self
                .frontend
                .show(
                    frame.ui(),
                    application.current_frame().as_deref(),
                    self.scene.image(),
                    self.game.image(),
                    self.detachment,
                    &self.status,
                    self.input.preedit(),
                )
                .map_err(imgui_error)?;
            self.platform
                .prepare_render(frame.ui(), &self.window)
                .map_err(imgui_error)?;
            let submissions = [self.scene.submitted, self.game.submitted]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let (prepared, secondary) = self.renderer.prepare(
                &mut self.gpu,
                window_packet,
                event_loop,
                frame,
                &submissions,
            )?;
            if let ImguiPreparedFrame::Native(prepared) = &prepared {
                secondary_windows.extend(
                    prepared
                        .secondary_report()
                        .presented_frames()
                        .iter()
                        .zip(&secondary)
                        .filter_map(|(native, outcome)| {
                            outcome.window().map(|window| {
                                (native.viewport_id(), native.window_id(), window.window_id())
                            })
                        }),
                );
            }
            secondary_outcomes = secondary;
            let renderer = &mut self.renderer;
            let outcome = self
                .gpu
                .present_window(
                    window_packet,
                    self.frame_index,
                    &submissions,
                    |frame| renderer.paint(prepared, frame),
                    || self.window.pre_present_notify(),
                )
                .map_err(platform_gpu_error)?;
            Ok((outcome, output))
        })();
        let (outcome, mut output) = match rendered {
            Ok(rendered) => rendered,
            Err(error) => {
                if let Some(pending) = pending_game {
                    let exact = secondary_outcomes
                        .iter()
                        .flat_map(WgpuEditorFrameOutcome::composites)
                        .copied()
                        .find(|receipt| receipt.image() == pending.image);
                    let completion = match exact {
                        Some(receipt) => pending.finish(Some(receipt)),
                        None => pending.fail(&error),
                    };
                    self.return_game_completion(application, completion)?;
                }
                return Err(error);
            }
        };
        self.presented = outcome.window().is_some()
            || secondary_outcomes
                .iter()
                .any(|outcome| outcome.window().is_some());
        self.ui_followup = self.context.binding().with_bound_context(|| {
            // SAFETY: the live Context is bound on its UI thread after EndFrame. Its viewport
            // vector and pointees remain owned by Dear ImGui; only scalar state leaves here.
            // The pinned core retires inactive platform windows and viewports on later frames.
            unsafe {
                let context = &*dear_imgui_rs::sys::igGetCurrentContext();
                context.InputEventsQueue.Size > 0
                    || (1..context.Viewports.Size).any(|index| {
                        let viewport = &**context.Viewports.Data.add(index as usize);
                        viewport.LastFrameActive < context.FrameCount
                    })
            }
        }) || self.context.platform_io().viewports_iter().any(|viewport| {
            viewport.platform_request_move()
                || viewport.platform_request_resize()
                || viewport.platform_request_close()
                || !viewport.platform_window_created()
        });
        if self.text_trace {
            if self.traced_probe_rect != output.text_probe_rect {
                self.traced_probe_rect = output.text_probe_rect;
                if let Some(rect) = output.text_probe_rect {
                    println!(
                        "{}",
                        serde_json::json!({"probe_rect": {"min": rect.min, "max": rect.max}, "probe_window_rect": output.text_probe_window_rect.map(|rect| (rect.min, rect.max)), "window_scale": self.window.scale_factor()})
                    );
                }
            }
            if self.traced_text != self.frontend.text_probe() {
                self.traced_text = self.frontend.text_probe().to_owned();
                println!("{}", serde_json::json!({"probe_text": self.traced_text}));
            }
        }
        if let Some(owner) = output.text_owner {
            let active = self
                .context
                .binding()
                .try_with_bound_context(|| {
                    // SAFETY: this live Context is bound on its UI thread and the prepared frame
                    // has been consumed. Only the current numeric item identity leaves this scope.
                    unsafe { dear_imgui_rs::sys::igGetActiveID() }
                })
                .map_err(imgui_error)?;
            if owner.item.raw() != active {
                output.text_owner = None;
            }
        }
        if let Some(pending) = pending_game {
            let receipt = outcome
                .composites()
                .iter()
                .chain(
                    secondary_outcomes
                        .iter()
                        .flat_map(WgpuEditorFrameOutcome::composites),
                )
                .copied()
                .find(|receipt| receipt.image() == pending.image);
            self.return_game_completion(application, pending.finish(receipt))?;
        }
        if pending_identity.is_some() {
            self.game_candidate = pending_identity;
        }
        let game_window = output.game_image.and_then(|image| {
            input::native_window(
                image.viewport,
                self.window.id(),
                primary_viewport,
                &secondary_windows,
            )
            .map(|(_, window)| window)
        });
        let game_receipt = outcome
            .composites()
            .iter()
            .chain(
                secondary_outcomes
                    .iter()
                    .flat_map(WgpuEditorFrameOutcome::composites),
            )
            .copied()
            .find(|receipt| {
                Some(receipt.image()) == self.game.submitted
                    && Some(receipt.window().window_id()) == game_window
            });
        self.input.observe(
            output.game_image,
            output.text_owner,
            self.game_candidate
                .zip(game_receipt)
                .map(|(identity, receipt)| EditorGamePresentedStamp::new(identity, receipt)),
            self.window.id(),
            primary_viewport,
            &secondary_windows,
            game_receipt.is_some(),
            application.ordinary_input_locked(),
        );
        self.input.flush(application)?;
        if self.text_trace {
            let facts = serde_json::json!({
                "play": application.current_frame().map(|frame| format!("{:?}", frame.play().state())),
                "play_rect": output.play_rect.map(|rect| (rect.min, rect.max)),
                "stop_rect": output.stop_rect.map(|rect| (rect.min, rect.max)),
                "game_panel_rect": output.game_panel_rect.map(|rect| (rect.min, rect.max)),
                "scene_panel_rect": output.scene_panel_rect.map(|rect| (rect.min, rect.max)),
                "scene_image_extent": self.scene.image().map(|image| image.extent),
                "scene_view": self.scene.view.map(|view| format!("{view:?}")),
                "scene_presented": outcome.composites().iter().chain(
                    secondary_outcomes.iter().flat_map(WgpuEditorFrameOutcome::composites)
                ).any(|receipt| Some(receipt.image()) == self.scene.submitted),
                "image_rect": output.game_image.map(|image| (
                    [image.rect.min[0] + image.viewport_origin[0], image.rect.min[1] + image.viewport_origin[1]],
                    [image.rect.max[0] + image.viewport_origin[0], image.rect.max[1] + image.viewport_origin[1]],
                )),
                "captured": self.input.is_captured(),
                "dear_mouse_down": self.context.io().mouse_down(dear_imgui_rs::MouseButton::Left),
                "presented": game_receipt.is_some(),
                "game_window": game_receipt.map(|receipt| receipt.window().window_id().get()),
            });
            if self.traced_game.as_ref() != Some(&facts) {
                println!("{}", serde_json::json!({"game_input_probe": facts}));
                self.traced_game = Some(facts);
            }
        }
        self.status = format!(
            "Frame {} | primary presented: {} | exact images: {} | secondary presents: {} (characterization)",
            self.frame_index,
            outcome.window().is_some(),
            outcome.composites().len(),
            secondary_outcomes.len()
        );
        let mut canary_action = None;
        if let Some(canary) = &mut self.canary {
            if canary.started.elapsed() > Duration::from_secs(60) {
                return Err(imgui_error(format!(
                    "native ImGui qualification timed out at phase {:?}: Scene {:?}, Game {:?}, Play {:?}, backend {} (retired {:?})",
                    canary.phase,
                    self.scene.submitted,
                    self.game.submitted,
                    application
                        .current_frame()
                        .map(|frame| frame.play().state()),
                    self.gpu.backend_instance_id(),
                    canary.retired_backend,
                )));
            }
            if outcome.window().is_some() && canary.first_present_ms.is_none() {
                canary.first_present_ms = Some(canary.started.elapsed().as_millis());
                if canary.fail_after_first_present {
                    return Err(imgui_error(
                        "qualification requested failure after the first native presentation",
                    ));
                }
            }
            let scene = self.scene.submitted;
            let game = self.game.submitted;
            let primary_scene = outcome
                .composites()
                .iter()
                .any(|receipt| Some(receipt.image()) == scene);
            let primary_game = outcome
                .composites()
                .iter()
                .any(|receipt| Some(receipt.image()) == game);
            let secondary_game = secondary_outcomes
                .iter()
                .flat_map(WgpuEditorFrameOutcome::composites)
                .any(|receipt| Some(receipt.image()) == game);
            if let Some(frame) = application.current_frame() {
                if frame.play().state() == EditorPlayState::Empty
                    && canary.phase == ImguiCanaryPhase::Primary
                {
                    if let Some(scene) = &frame.workspace().active_scene {
                        canary_action = Some(EditorAction::Play(EditorPlayAction::Play {
                            document: scene.document,
                            base_revision: scene.revision,
                        }));
                    }
                }
            }
            match canary.phase {
                ImguiCanaryPhase::Primary if primary_scene && primary_game => {
                    canary.primary_frames += 1;
                    if canary.primary_frames >= 4 {
                        self.frontend.place_qualification_game(true);
                        canary.phase = ImguiCanaryPhase::Detached;
                    }
                }
                ImguiCanaryPhase::Detached if primary_scene && secondary_game => {
                    canary.secondary_frames += 1;
                    if canary.secondary_frames >= 4 {
                        self.window.set_minimized(true);
                        canary.phase = ImguiCanaryPhase::Minimized;
                    }
                }
                ImguiCanaryPhase::Minimized
                    if secondary_game && self.window.is_minimized() == Some(true) =>
                {
                    canary.minimized_secondary_frames += 1;
                    if canary.minimized_secondary_frames >= 4 {
                        self.window.set_minimized(false);
                        canary.phase = ImguiCanaryPhase::RestoringPrimary;
                    }
                }
                ImguiCanaryPhase::RestoringPrimary
                    if primary_scene
                        && secondary_game
                        && self.window.is_minimized() == Some(false) =>
                {
                    self.frontend.place_qualification_game(false);
                    canary.phase = ImguiCanaryPhase::Returned;
                }
                ImguiCanaryPhase::Returned if primary_scene && primary_game => {
                    canary.returned_frames += 1;
                    if canary.returned_frames >= 4 {
                        if !canary.stale_receipt_rejected {
                            return Err(imgui_error(
                                "the native qualification did not reject an overwritten image receipt",
                            ));
                        }
                        canary.phase = ImguiCanaryPhase::GameClosed;
                        self.frontend.set_qualification_game_open(false);
                    }
                }
                ImguiCanaryPhase::GameClosed
                    if primary_scene
                        && self.game.view.is_none()
                        && self.game_candidate.is_none() =>
                {
                    if !application
                        .current_frame()
                        .is_some_and(|frame| frame.play().state() == EditorPlayState::Running)
                    {
                        return Err(imgui_error("closing Game stopped its Runtime"));
                    }
                    canary.game_closed_frames += 1;
                    if canary.game_closed_frames >= 4 {
                        self.frontend.set_qualification_game_open(true);
                        canary.phase = ImguiCanaryPhase::GameReopening;
                    }
                }
                ImguiCanaryPhase::GameReopening if primary_scene && primary_game => {
                    self.frontend.set_qualification_scene_open(false);
                    canary.phase = ImguiCanaryPhase::SceneClosed;
                }
                ImguiCanaryPhase::SceneClosed if primary_game && self.scene.view.is_none() => {
                    canary.scene_closed_frames += 1;
                    if canary.scene_closed_frames >= 4 {
                        self.frontend.set_qualification_scene_open(true);
                        canary.phase = ImguiCanaryPhase::Reopened;
                    }
                }
                ImguiCanaryPhase::Reopened if primary_scene && primary_game => {
                    canary.reopened_frames += 1;
                    if canary.reopened_frames >= 4 {
                        canary.phase = ImguiCanaryPhase::Recovering;
                        canary.retired_backend =
                            self.game.submitted.map(|image| image.backend_instance_id());
                        self.gpu.wait_for_idle().map_err(platform_gpu_error)?;
                        self.gpu
                            .with_device(window_packet, |scope| scope.device().destroy())
                            .map_err(platform_gpu_error)?;
                    }
                }
                ImguiCanaryPhase::Recovering
                    if primary_scene
                        && primary_game
                        && self.game.submitted.is_some_and(|image| {
                            Some(image.backend_instance_id()) != canary.retired_backend
                        }) =>
                {
                    if !application
                        .current_frame()
                        .is_some_and(|frame| frame.play().state() == EditorPlayState::Paused)
                    {
                        return Err(imgui_error(
                            "GPU replacement did not pause the active Runtime",
                        ));
                    }
                    canary.phase = ImguiCanaryPhase::Recovered;
                    canary_action = Some(EditorAction::Play(EditorPlayAction::Resume));
                }
                ImguiCanaryPhase::Recovered if primary_scene && primary_game => {
                    canary.recovered_frames += 1;
                    if canary.recovered_frames >= 4 {
                        canary.phase = ImguiCanaryPhase::Stopping;
                        canary_action = Some(EditorAction::Play(EditorPlayAction::Stop));
                    }
                }
                ImguiCanaryPhase::Stopping
                    if application.active_game_start_request().is_none()
                        && application.pending_game_start_request().is_none() =>
                {
                    if self.game.submitted.is_some()
                        || self.game_candidate.is_some()
                        || self.game.identity.is_some()
                    {
                        return Err(imgui_error(
                            "Stop retained an interactive Game image lifetime",
                        ));
                    }
                    canary.phase = ImguiCanaryPhase::Complete;
                    return Ok(application.request_exit().exit_ready());
                }
                _ => {}
            }
        }
        if output.save_layout {
            let mut layout = String::new();
            self.context.save_ini_settings(&mut layout);
            self.frontend
                .save_panel_state(application.panel_workspace_mut().layout_mut());
            if let Ok(position) = self.window.outer_position() {
                let size = self.window.inner_size();
                application
                    .panel_workspace_mut()
                    .layout_mut()
                    .set_window_bounds(crate::workspace::EditorWindowBounds::new(
                        position.x as f32,
                        position.y as f32,
                        size.width as f32,
                        size.height as f32,
                    ));
            }
            application
                .panel_workspace_mut()
                .layout_mut()
                .set_toolkit_layout(Some(crate::workspace::EditorToolkitLayout::new(
                    "dear-imgui",
                    1,
                    layout.clone(),
                )));
            application.panel_workspace_mut().save_preferences();
            self.layout = Some(layout);
        }
        if output.restore_layout
            && let Some(layout) = &self.layout
        {
            self.context.load_ini_settings(layout);
            self.frontend
                .restore_panel_state(application.panel_workspace().layout());
        }
        if output.reset_layout {
            self.context.binding().with_bound_context(|| {
                // SAFETY: the prepared frame has been consumed; this live Context owns all
                // settings being cleared. Window placement is reset on the next UI frame.
                unsafe {
                    dear_imgui_rs::sys::igClearIniSettings();
                }
            });
            application.panel_workspace_mut().reset();
            application.panel_workspace_mut().save_preferences();
            self.frontend.reset_layout();
            self.layout = None;
        }
        if output.reset_layout || output.restore_layout {
            self.input.invalidate(false);
            self.input.flush(application)?;
            self.ui_followup = true;
            self.window.request_redraw();
        }
        let extent = output
            .scene_extent
            .and_then(|size| Extent2d::new(size[0].round() as u32, size[1].round() as u32));
        if extent != self.scene_extent {
            self.scene_extent = extent;
            application.set_scene_preview_extent(extent);
            self.window.request_redraw();
        }
        if self.frontend.game_open()
            && application.active_game_start_request().is_some()
            && let Some(extent) = output
                .game_extent
                .and_then(|size| Extent2d::new(size[0].round() as u32, size[1].round() as u32))
            && application.resize_game_view(extent).map_err(imgui_error)?
        {
            self.input.invalidate_after_control(false);
            self.game_candidate = None;
            self.game.submitted = None;
            application.drive_editor_frame(Duration::ZERO, false, true);
            self.window.request_redraw();
        }
        let actions = if let Some(action) = canary_action {
            application
                .current_frame()
                .map(|frame| EditorActionBatch::new(frame.revision(), vec![action]))
        } else {
            output.actions
        };
        if let Some(actions) = actions {
            let dispatch = application.dispatch_editor_actions(actions);
            if self.text_trace
                && let Some(feedback) = dispatch.feedback()
            {
                match feedback {
                    Ok(report) if !report.outcomes().is_empty() => println!(
                        "{}",
                        serde_json::json!({"action_feedback": format!("{:?}", report.outcomes())})
                    ),
                    Err(rejection) => println!(
                        "{}",
                        serde_json::json!({"action_rejection": format!("{rejection:?}")})
                    ),
                    _ => {}
                }
            }
            if dispatch.invalidates_game_input() {
                self.input.invalidate_after_control(true);
                self.game_candidate = None;
            }
            let update = application.drive_editor_frame(
                Duration::ZERO,
                dispatch.actions_observed(),
                dispatch.requires_runtime_drive() || dispatch.requests_step(),
            );
            if update.actions_observed() {
                self.window.request_redraw();
            }
            return Ok(update.exit_ready());
        }
        Ok(false)
    }

    fn prepare_game_start(
        &mut self,
        application: &mut EditorApplication,
        window: RenderWindowPacket,
    ) -> Result<(), EditorRunError> {
        if application.pending_game_start_request().is_none()
            && application.active_game_start_request().is_none()
        {
            self.game_source_active = false;
            if self.game.view.is_some() || self.game_candidate.is_some() {
                self.input.invalidate_after_control(true);
                self.game_candidate = None;
                self.game.retire(&mut self.gpu, &mut self.renderer)?;
            }
            return Ok(());
        }
        if let Some(request) = application.pending_game_start_request() {
            let (source, target) = game_target(request)?;
            if self.frontend.game_open() {
                self.game.ensure(
                    &mut self.gpu,
                    &mut self.renderer,
                    window,
                    source,
                    target,
                    &mut self.next_view,
                )?;
            }
            let activation = application
                .prepare_game_start_activation(request)
                .map_err(imgui_error)?;
            application.commit_game_start_activation(activation);
            self.game_source_active = true;
        }
        if let Some(request) = application.active_game_start_request() {
            if !self.frontend.game_open() {
                self.input.invalidate(false);
                self.input.flush(application)?;
                if self.game_source_active {
                    application
                        .set_game_view_source_active(false, "the Game view was closed")
                        .map_err(imgui_error)?;
                    self.game_source_active = false;
                }
                self.game_candidate = None;
                self.game.retire(&mut self.gpu, &mut self.renderer)?;
                return Ok(());
            }
            if !self.game_source_active {
                application
                    .set_game_view_source_active(true, "the Game view reopened")
                    .map_err(imgui_error)?;
                self.game_source_active = true;
                application.drive_editor_frame(Duration::ZERO, false, true);
            }
            let (source, target) = game_target(request)?;
            self.game.ensure(
                &mut self.gpu,
                &mut self.renderer,
                window,
                source,
                target,
                &mut self.next_view,
            )?;
        }
        Ok(())
    }

    fn submit_game(
        &mut self,
        application: &mut EditorApplication,
        window: RenderWindowPacket,
    ) -> Result<Option<PendingGameImage>, EditorRunError> {
        if self.game.identity.is_none() || application.active_game_start_request().is_none() {
            return Ok(None);
        }
        if self.rejected_completion.is_some() {
            return Err(imgui_error(
                "Runtime rejected its prior exact Game completion",
            ));
        }
        let Some(candidate) = application
            .take_game_view_candidate()
            .map_err(imgui_error)?
        else {
            return Ok(None);
        };
        let (identity, packet, sprites, runtime_ui) =
            candidate.into_parts().into_submission_parts();
        let Some((view, target)) = self.game.view.zip(self.game.target) else {
            let completion = match runtime_ui {
                Some(ui) => EditorGameViewCompletion::with_ui_failed(
                    identity,
                    WgpuEditorExecution::reject_editor_ui_frame(ui, "Game has no live target"),
                ),
                None => EditorGameViewCompletion::without_ui_failed(identity),
            };
            self.return_game_completion(application, completion)?;
            return Err(imgui_error("Game has no live target"));
        };
        let packet = match map_editor_game_view_packet(packet, target) {
            Ok(packet) => packet,
            Err(error) => {
                let completion = match runtime_ui {
                    Some(ui) => EditorGameViewCompletion::with_ui_failed(
                        identity,
                        WgpuEditorExecution::reject_editor_ui_frame(ui, error.to_string()),
                    ),
                    None => EditorGameViewCompletion::without_ui_failed(identity),
                };
                self.return_game_completion(application, completion)?;
                return Err(imgui_error(error));
            }
        };
        let (image, ui) = match (sprites, runtime_ui) {
            (sprites, Some(ui)) => {
                let submitted = match sprites {
                    Some(sprites) => self
                        .gpu
                        .submit_sprite_ui_image(view, window, packet, sprites, ui),
                    None => self.gpu.submit_ui_image(view, window, packet, ui),
                };
                match submitted {
                    Ok(ui) => (ui.image(), Some(ui)),
                    Err(failure) => {
                        let (error, outcomes) = failure.complete();
                        self.return_game_completion(
                            application,
                            EditorGameViewCompletion::with_ui_failed(identity, outcomes),
                        )?;
                        return Err(platform_gpu_error(error));
                    }
                }
            }
            (sprites, None) => {
                let submitted = match sprites {
                    Some(sprites) => self.gpu.submit_sprite_image(view, window, packet, sprites),
                    None => self.gpu.submit_image(view, window, packet),
                };
                match submitted {
                    Ok(image) => (image, None),
                    Err(error) => {
                        self.return_game_completion(
                            application,
                            EditorGameViewCompletion::without_ui_failed(identity),
                        )?;
                        return Err(platform_gpu_error(error));
                    }
                }
            }
        };
        self.game.submitted = Some(image);
        Ok(Some(PendingGameImage {
            identity,
            image,
            ui,
        }))
    }

    fn return_game_completion(
        &mut self,
        application: &mut EditorApplication,
        completion: EditorGameViewCompletion,
    ) -> Result<(), EditorRunError> {
        if let Err(completion) = application.complete_game_view_candidate(completion) {
            let error = imgui_error(format!(
                "Runtime rejected its exact Game completion: {:?}",
                completion.identity()
            ));
            self.rejected_completion = Some(completion);
            return Err(error);
        }
        Ok(())
    }

    fn shutdown(&mut self) -> Result<(), EditorRunError> {
        self.shutdown_graphics()?;
        if let Some(canary) = &self.canary {
            if canary.phase != ImguiCanaryPhase::Complete {
                return Err(imgui_error(
                    "the native qualification closed before its presentation trajectory completed",
                ));
            }
            println!(
                "{{\"schema\":\"nara.editor-imgui-qualification-v1\",\"primary_frames\":{},\"secondary_frames\":{},\"minimized_secondary_frames\":{},\"returned_frames\":{},\"game_closed_frames\":{},\"scene_closed_frames\":{},\"reopened_frames\":{},\"recovered_frames\":{},\"first_present_ms\":{},\"stale_receipt_rejected\":true,\"renderer_then_platform_shutdown\":true,\"native_ime\":\"unverified\"}}",
                canary.primary_frames,
                canary.secondary_frames,
                canary.minimized_secondary_frames,
                canary.returned_frames,
                canary.game_closed_frames,
                canary.scene_closed_frames,
                canary.reopened_frames,
                canary.recovered_frames,
                canary.first_present_ms.unwrap_or(0)
            );
        }
        Ok(())
    }

    fn shutdown_graphics(&mut self) -> Result<(), EditorRunError> {
        self.input.invalidate(true);
        self.context.end_frame();
        let mut failure = self.gpu.wait_for_idle().map_err(platform_gpu_error).err();
        if !self.renderer_shutdown {
            for target in [&mut self.scene, &mut self.game] {
                if let Err(error) = target.retire(&mut self.gpu, &mut self.renderer) {
                    failure.get_or_insert(error);
                }
            }
            match self.renderer.shutdown(&mut self.context) {
                Ok(()) => self.renderer_shutdown = true,
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        if self.platform.viewports_enabled() {
            if let Err(error) = self
                .platform
                .disable_viewports(&mut self.context)
                .map_err(imgui_error)
            {
                failure.get_or_insert(error);
            }
        }
        if !self.platform_shutdown {
            match self
                .platform
                .shutdown(&mut self.context)
                .map_err(imgui_error)
            {
                Ok(()) => self.platform_shutdown = true,
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }
}

impl ImguiQualificationHost {
    fn install_native(&mut self, event_loop: &ActiveEventLoop) -> Result<(), EditorRunError> {
        let window = self
            .window
            .as_ref()
            .cloned()
            .ok_or_else(|| imgui_error("the primary window is unavailable"))?;
        let mut native = NativeImgui::new(event_loop, window)?;
        if let Some(retained) = self.retained.take() {
            native.context.load_ini_settings(&retained.layout);
            native.frontend = retained.frontend;
            native.layout = Some(retained.layout);
            native.canary = retained.canary;
            native.frame_index = retained.frame_index;
            native.next_view = retained.next_view;
            native.layout_loaded = true;
        }
        native
            .window
            .set_title("Nara Editor - Dear ImGui qualification");
        native.window.request_redraw();
        self.native = Some(native);
        self.last_drive = Instant::now();
        self.repaint_deadline = None;
        Ok(())
    }

    fn retire_native(&mut self) {
        self.application.set_scene_preview_extent(None);
        if let Some(mut native) = self.native.take() {
            let mut layout = String::new();
            native.context.save_ini_settings(&mut layout);
            if let Err(error) = native.shutdown_graphics() {
                eprintln!("Nara Editor GPU retirement: {error}");
            }
            self.retained = Some(ImguiRetainedState {
                frontend: std::mem::take(&mut native.frontend),
                layout,
                canary: native.canary.take(),
                frame_index: native.frame_index,
                next_view: native.next_view,
            });
        }
    }

    fn await_explicit_retry(&mut self, error: &EditorRunError) {
        self.retire_native();
        self.gpu_recovery.require_explicit_retry();
        if let Some(window) = &self.window {
            window.set_title(super::EDITOR_GPU_UNAVAILABLE_TITLE);
        }
        eprintln!("Nara Editor GPU recovery paused: {error}");
    }

    fn replace_native(&mut self, event_loop: &ActiveEventLoop) {
        self.retire_native();
        if let Err(error) = self.install_native(event_loop) {
            self.await_explicit_retry(&error);
        }
    }

    fn handle_gpu_failure(
        &mut self,
        event_loop: &ActiveEventLoop,
        error: EditorRunError,
    ) -> Result<(), EditorRunError> {
        if let Some(native) = &mut self.native {
            native.input.invalidate(true);
            native.input.flush(&mut self.application)?;
        }
        if !error.requires_gpu_replacement() {
            if matches!(&error, EditorRunError::Gpu(error) if error.is_deferred() || error.is_content_rejected())
            {
                return Ok(());
            }
            return Err(error);
        }
        if self.application.active_game_start_request().is_some() {
            self.application
                .set_game_view_source_active(false, "the Editor GPU owner was retired")
                .map_err(imgui_error)?;
        }
        let state = self
            .application
            .current_frame()
            .map(|frame| frame.play().state());
        if matches!(
            state,
            Some(EditorPlayState::Running | EditorPlayState::Stepping)
        ) && self.application.active_game_start_request().is_some()
        {
            if !self.gpu_recovery.begin_runtime_pause() {
                self.await_explicit_retry(&error);
                return Ok(());
            }
            self.retire_native();
            self.advance_recovery(event_loop);
        } else if self.gpu_recovery.begin_automatic_replacement() {
            self.replace_native(event_loop);
        } else {
            self.await_explicit_retry(&error);
        }
        Ok(())
    }

    fn advance_recovery(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu_recovery != EditorGpuRecovery::AwaitingRuntimePause {
            return;
        }
        if self.application.drive_gpu_recovery_pause().exit_ready() {
            event_loop.exit();
            return;
        }
        let state = self
            .application
            .current_frame()
            .map(|frame| frame.play().state());
        if state.is_none()
            || matches!(
                state,
                Some(
                    EditorPlayState::Paused
                        | EditorPlayState::Faulted
                        | EditorPlayState::CloseIncomplete
                        | EditorPlayState::Empty
                        | EditorPlayState::RetiringPlay
                        | EditorPlayState::RetirementIncomplete
                )
            )
        {
            self.gpu_recovery.finish_runtime_pause();
            self.replace_native(event_loop);
        }
    }
}

impl ApplicationHandler for ImguiQualificationHost {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.native.is_some() {
            return;
        }
        if self.window.is_none() {
            match event_loop.create_window(
                Window::default_attributes()
                    .with_title("Nara Editor - Dear ImGui qualification")
                    .with_inner_size(LogicalSize::new(1200.0, 800.0)),
            ) {
                Ok(window) => self.window = Some(Arc::new(window)),
                Err(error) => {
                    self.error = Some(imgui_error(error));
                    event_loop.exit();
                    return;
                }
            }
        }
        match self.install_native(event_loop) {
            Ok(()) => {
                self.application.native_shell_ready();
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let is_primary = self
            .window
            .as_ref()
            .is_some_and(|window| window.id() == window_id);
        if is_primary {
            if let WindowEvent::KeyboardInput { event, .. } = &event
                && explicit_gpu_retry_requested(event)
                && self.gpu_recovery.begin_explicit_retry()
            {
                self.replace_native(event_loop);
                return;
            }
            if matches!(event, WindowEvent::CloseRequested) && self.native.is_none() {
                if self.application.request_exit().exit_ready() {
                    event_loop.exit();
                }
                return;
            }
        }
        let Some(native) = &mut self.native else {
            return;
        };
        let is_primary = native.window.id() == window_id;
        let full: Event<()> = Event::WindowEvent {
            window_id,
            event: event.clone(),
        };
        if native.text_trace
            && let WindowEvent::Ime(ime) = &event
        {
            println!(
                "{}",
                serde_json::json!({"native_ime_event": format!("{ime:?}"), "primary": is_primary})
            );
        }
        if native
            .input
            .handle_event(window_id, &event, &native.context)
        {
            if let Err(error) =
                native
                    .platform
                    .handle_event(&mut native.context, &native.window, &full)
            {
                self.error = Some(imgui_error(error));
                event_loop.exit();
                return;
            }
        }
        if let Err(error) = native.input.flush(&mut self.application) {
            self.error = Some(error);
            event_loop.exit();
            return;
        }
        match event {
            WindowEvent::CloseRequested if is_primary => {
                if self.application.request_exit().exit_ready() {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested if is_primary => {
                let started = Instant::now();
                match native.redraw(event_loop, &mut self.application) {
                    Ok(true) => event_loop.exit(),
                    Ok(false) => {
                        if native.presented {
                            self.gpu_recovery.mark_presentation_ready();
                        }
                    }
                    Err(error) => {
                        if let Err(error) = self.handle_gpu_failure(event_loop, error) {
                            self.error = Some(error);
                            event_loop.exit();
                        }
                    }
                }
                if let Some(metrics) = &mut self.metrics {
                    metrics.observe(
                        started,
                        self.native.as_ref().is_some_and(|native| native.presented),
                    );
                }
            }
            _ => native.window.request_redraw(),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last_drive);
        self.last_drive = now;
        let update = self.application.advance_project_open();
        if update.exit_ready() {
            event_loop.exit();
            return;
        }
        let play_before = self
            .application
            .current_frame()
            .map(|frame| frame.play().state());
        let running = play_before.is_some_and(|state| {
            matches!(
                state,
                EditorPlayState::Running
                    | EditorPlayState::Starting
                    | EditorPlayState::Stepping
                    | EditorPlayState::Stopping
                    | EditorPlayState::RetiringPlay
                    | EditorPlayState::PreparingPlay
            )
        });
        if running && let Some(native) = &mut self.native {
            native.input.mark_presented_stale();
        }
        let update = self.application.drive_editor_frame(delta, false, running);
        if update.exit_ready() {
            event_loop.exit();
            return;
        }
        self.advance_recovery(event_loop);
        let play_changed = play_before
            != self
                .application
                .current_frame()
                .map(|frame| frame.play().state());
        let deadline = imgui_next_deadline(&self.application, self.native.as_ref(), now);
        let wake_due = self
            .repaint_deadline
            .is_some_and(|deadline| now >= deadline);
        if (update.actions_observed() || play_changed || wake_due)
            && let Some(native) = &self.native
        {
            self.repaint_deadline = None;
            native.window.request_redraw();
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if let Some(deadline) = deadline {
            self.repaint_deadline = Some(deadline);
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(metrics) = self.metrics.take() {
            metrics.finish(
                self.native
                    .as_mut()
                    .map(|native| (&mut native.gpu, editor_window_packet(&native.window))),
            );
        }
    }
}

fn imgui_next_deadline(
    application: &EditorApplication,
    native: Option<&NativeImgui>,
    now: Instant,
) -> Option<Instant> {
    let mut deadline = application.next_supervised_play_deadline();
    if let Some(frame) = application.current_frame() {
        let state = frame.play().state();
        if state == EditorPlayState::Running {
            deadline = earliest_deadline(deadline, Some(now + Duration::from_millis(16)));
        }
        deadline = earliest_deadline(deadline, play_lifecycle_poll_deadline(now, state));
        deadline = earliest_deadline(
            deadline,
            image_import_poll_deadline(now, frame.image_import_work_pending()),
        );
    }
    if application.game_step_requires_runtime_drive() {
        deadline = earliest_deadline(deadline, Some(now + Duration::from_millis(16)));
    }
    deadline = earliest_deadline(deadline, application.session_close_retry_deadline());
    let Some(native) = native else {
        return deadline;
    };
    if native.canary.is_some() {
        deadline = earliest_deadline(deadline, Some(now + Duration::from_millis(16)));
    }
    if native.ui_followup {
        deadline = earliest_deadline(
            deadline,
            Some(native.last_frame + Duration::from_millis(16)),
        );
    } else if native.context.io().want_text_input() {
        deadline = earliest_deadline(
            deadline,
            Some(native.last_frame + Duration::from_millis(500)),
        );
    }
    deadline
}

fn game_target(
    request: EditorGameStartRequest,
) -> Result<(RenderFrameSourceGeneration, RenderImagePacket), EditorRunError> {
    let resolution = request.logical_window().resolution;
    let extent = Extent2d::new(resolution.physical_width, resolution.physical_height)
        .ok_or_else(|| imgui_error("Game logical window has zero extent"))?;
    let target =
        editor_viewport_image_target(Handle::new(AssetId::from_raw(GAME_TARGET_ID)), extent)
            .map_err(imgui_error)?;
    Ok((
        RenderFrameSourceGeneration::from(request.runtime_generation()),
        target,
    ))
}

fn imgui_error(error: impl std::fmt::Display) -> EditorRunError {
    EditorRunError::Platform(error.to_string())
}

fn retire_failed_imgui_renderer(
    error: EditorRunError,
    gpu: &WgpuEditorExecution,
    renderer: &mut WgpuRenderer,
    platform: &mut WinitPlatform,
    context: &mut Context,
) -> EditorRunError {
    let mut cleanup = gpu.wait_for_idle().map_err(platform_gpu_error).err();
    if let Err(failure) = renderer.shutdown(context).map_err(imgui_error) {
        cleanup.get_or_insert(failure);
    }
    if platform.viewports_enabled()
        && let Err(failure) = platform.disable_viewports(context).map_err(imgui_error)
    {
        cleanup.get_or_insert(failure);
    }
    if let Err(failure) = platform.shutdown(context).map_err(imgui_error) {
        cleanup.get_or_insert(failure);
    }
    match cleanup {
        Some(cleanup) => imgui_error(format!(
            "{error}; initialization cleanup also failed: {cleanup}"
        )),
        None => error,
    }
}

fn native_detachment_supported(event_loop: &ActiveEventLoop) -> bool {
    #[cfg(target_os = "linux")]
    {
        use winit::platform::x11::ActiveEventLoopExtX11;
        event_loop.is_x11()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = event_loop;
        cfg!(any(target_os = "windows", target_os = "macos"))
    }
}
