use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use dear_imgui_rs::{Context, FontConfig, FontSource, StbTrueTypeFontData};
use dear_imgui_wgpu::{ExternalTextureId, wgpu};
use dear_imgui_winit::{HiDpiMode, WinitPlatform, WinitPlatformAction};
use nara::{
    project_host::{
        EditorGameStartRequest, EditorGameViewCandidateIdentity, EditorGameViewCompletion,
    },
    render::{
        __private::{editor_viewport_image_target, map_editor_game_view_packet},
        Extent2d, RenderFrameSourceGeneration, RenderImagePacket, RenderWindowPacket,
    },
    render_wgpu::__private::{
        WgpuEditorExecution, WgpuEditorExecutionError, WgpuEditorFrameOutcome,
        WgpuEditorGpuRetirement, WgpuEditorImguiFrame, WgpuEditorImguiRenderer,
        WgpuEditorUiImageSubmission, WgpuEditorViewId, WgpuEditorViewportIdentity,
        WgpuEditorWindowFrame, WgpuSubmittedImageReceipt,
    },
    tooling::{EditorActionBatch, EditorPlayState, EditorRuntimeAvailability},
    window::{WindowId as NaraWindowId, backend::WindowHandleProvider},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use crate::frontend::imgui::{ImguiEditorFrontend, ImguiViewImage};
use crate::{EditorApplication, supervised_play::SupervisedPlayWake};

use super::{
    EditorGpuRecovery, EditorRunError, earliest_deadline, editor_window_packet,
    explicit_gpu_retry_requested, image_import_poll_deadline, persistence_poll_deadline,
    platform_gpu_error, play_lifecycle_poll_deadline,
};

#[cfg(all(test, target_os = "windows"))]
mod authoring_workflow_tests;
#[cfg(all(test, target_os = "windows"))]
mod cadence_tests;
mod canvas;
#[cfg(all(test, target_os = "windows"))]
mod canvas_reopen_tests;
#[cfg(test)]
mod canvas_tests;
#[cfg(all(test, target_os = "windows"))]
mod canvas_workflow_tests;
#[cfg(all(test, target_os = "windows"))]
mod external_cadence_tests;
mod input;
#[cfg(all(test, target_os = "windows"))]
mod manager_workflow_tests;
mod repaint;
mod retirement;
#[cfg(all(test, target_os = "windows"))]
mod retirement_tests;
use canvas::ImguiUiCanvasPresenter;
use retirement::{
    NativeClose, NativeCloseErrors, NativeInitializationFailure, NativeRetirementAction,
};
#[cfg(all(test, target_os = "windows"))]
mod game_input_tests;
#[cfg(all(test, target_os = "windows"))]
mod lifecycle_tests;
#[cfg(all(test, target_os = "windows"))]
mod panel_workflow_tests;
mod scene;
#[cfg(all(test, target_os = "windows"))]
mod scene_image_tests;
#[cfg(all(test, target_os = "windows"))]
mod supervised_play_support;
#[cfg(all(test, target_os = "windows"))]
mod supervised_play_tests;
use super::game_input::EditorGamePresentedStamp;
use input::ImguiInput;
use scene::ImguiScenePresenter;

pub(super) fn run_application(
    mut application: EditorApplication,
    frontend: ImguiEditorFrontend,
) -> Result<(), EditorRunError> {
    if let Some(preferences) = crate::workspace::EditorWorkspacePreferences::user_default() {
        application
            .panel_workspace_mut()
            .set_preferences(preferences);
    }
    let event_loop = EventLoop::<EditorUserEvent>::with_user_event()
        .build()
        .map_err(|error| EditorRunError::EventLoop(error.to_string()))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();
    let supervised_play_wake = SupervisedPlayWake::new(move || {
        proxy
            .send_event(EditorUserEvent::SupervisedPlayWake)
            .is_ok()
    });
    application.install_supervised_play_wake(supervised_play_wake.clone());
    let mut host = NativeEditorHost::new(application, supervised_play_wake, frontend);
    if let Err(error) = event_loop.run_app(&mut host) {
        host.error
            .get_or_insert_with(|| EditorRunError::EventLoop(error.to_string()));
    }
    // Ordinary exit has already retired graphics while the event loop was alive. A terminal
    // event-loop failure can still leave an owner; attempt finite local cleanup without claiming
    // that a lost event pump can satisfy arbitrary external close dependencies.
    host.retire_native();
    let shutdown = host
        .retiring
        .as_mut()
        .map_or(Ok(()), NativeImgui::shutdown_graphics);
    if let Some(native) = &host.retiring {
        if !native.close.is_complete() {
            eprintln!(
                "Nara Editor cannot release unfinished native custody after its event loop ended: {shutdown:?}"
            );
            std::process::abort();
        }
        native.publish_render_views(host.application.panel_workspace_mut());
    }
    host.retiring.take();
    if let Err(error) = shutdown {
        host.retirement_errors.record(error);
    }
    editor_run_result(
        host.error,
        &host.retirement_errors,
        host.application.failed_open_close_evidence(),
        host.application.failed_session_close_receipt(),
    )
}

fn editor_run_result(
    primary: Option<EditorRunError>,
    native_cleanup: &NativeCloseErrors,
    project_cleanup: Option<&nara::app::RuntimeCloseEvidence>,
    session_cleanup: Option<&nara::project_host::EditorProjectCloseReceipt>,
) -> Result<(), EditorRunError> {
    let mut cleanup = String::new();
    if !native_cleanup.is_empty() {
        cleanup = format!("native graphics retired with errors: {native_cleanup}");
    }
    if let Some(evidence) = project_cleanup {
        if !cleanup.is_empty() {
            cleanup.push_str("; ");
        }
        cleanup.push_str(&format!(
            "failed project Open retired with cleanup errors: {evidence:?}"
        ));
    }
    if let Some(receipt) = session_cleanup.filter(|receipt| receipt.has_cleanup_failure()) {
        if !cleanup.is_empty() {
            cleanup.push_str("; ");
        }
        cleanup.push_str(&format!(
            "project Session retired with cleanup errors: {receipt:?}"
        ));
    }
    match (primary, cleanup.is_empty()) {
        (Some(primary), false) => Err(EditorRunError::Cleanup {
            primary: Box::new(primary),
            cleanup,
        }),
        (Some(primary), true) => Err(primary),
        (None, false) => Err(imgui_error(cleanup)),
        (None, true) => Ok(()),
    }
}

#[cfg(test)]
mod result_tests {
    use super::*;
    use nara::app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginShutdownContext, RuntimeCandidateRetirementState, RuntimeInstance,
    };

    struct RejectedShutdown;
    impl Plugin for RejectedShutdown {
        fn declaration() -> &'static PluginDeclaration {
            static DECLARATION: PluginDeclaration = PluginDeclaration::new(
                PluginId::new("nara.test.open-result-cleanup"),
                PluginCategory::Runtime,
            );
            &DECLARATION
        }
        fn build(&self, _: &mut App) -> Result<(), PluginError> {
            Ok(())
        }
        fn shutdown(&self, _: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
            Err(PluginError::SetupFailed {
                plugin: Self::declaration().id,
                message: "cleanup probe".to_owned(),
            })
        }
    }

    fn failed_open_cleanup_evidence() -> nara::app::RuntimeCloseEvidence {
        let mut app = App::new();
        app.add_plugin(RejectedShutdown).unwrap();
        let mut retirement = RuntimeInstance::start(app).unwrap().begin_retirement();
        assert_eq!(
            retirement.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
        let evidence = retirement.close_evidence().clone();
        assert!(evidence.has_terminal_failure());
        evidence
    }

    #[test]
    fn physical_cleanup_failure_reaches_editor_result_and_keeps_the_primary_error() {
        let evidence = failed_open_cleanup_evidence();
        let mut native = NativeCloseErrors::default();
        native.record("native cleanup probe");
        let result = editor_run_result(
            Some(EditorRunError::Usage("primary probe".to_owned())),
            &native,
            Some(&evidence),
            None,
        );
        let text = result.as_ref().unwrap_err().to_string();
        assert!(
            matches!(result, Err(EditorRunError::Cleanup { primary, .. }) if matches!(*primary, EditorRunError::Usage(_)))
        );
        for fact in [
            "primary probe",
            "native cleanup probe",
            "failed project Open",
            "PluginShutdown",
        ] {
            assert!(text.contains(fact), "missing {fact}: {text}");
        }
        assert!(
            editor_run_result(None, &NativeCloseErrors::default(), Some(&evidence), None).is_err()
        );
        assert!(editor_run_result(None, &NativeCloseErrors::default(), None, None).is_ok());
        assert!(matches!(
            editor_run_result(
                Some(EditorRunError::Usage("primary only".to_owned())),
                &NativeCloseErrors::default(),
                None,
                None,
            ),
            Err(EditorRunError::Usage(message)) if message == "primary only"
        ));
    }

    #[cfg(any(windows, all(target_os = "linux", target_arch = "x86_64")))]
    #[test]
    fn trusted_session_cleanup_failure_reaches_editor_result_and_preserves_every_cleanup_source() {
        let receipt = crate::app::failed_trusted_session_close_receipt();
        let session_only =
            editor_run_result(None, &NativeCloseErrors::default(), None, Some(&receipt))
                .unwrap_err()
                .to_string();
        for fact in ["project Session", "PluginShutdown"] {
            assert!(
                session_only.contains(fact),
                "missing {fact}: {session_only}"
            );
        }

        let evidence = failed_open_cleanup_evidence();
        let mut native = NativeCloseErrors::default();
        native.record("native cleanup probe");
        let result = editor_run_result(
            Some(EditorRunError::EventLoop(
                "primary event loop probe".to_owned(),
            )),
            &native,
            Some(&evidence),
            Some(&receipt),
        );
        let text = result.as_ref().unwrap_err().to_string();
        assert!(matches!(
            result,
            Err(EditorRunError::Cleanup { primary, .. })
                if matches!(primary.as_ref(), EditorRunError::EventLoop(message)
                    if message == "primary event loop probe")
        ));
        for fact in [
            "primary event loop probe",
            "native cleanup probe",
            "failed project Open",
            "project Session",
        ] {
            assert!(text.contains(fact), "missing {fact}: {text}");
        }
        assert_eq!(text.matches("PluginShutdown").count(), 2, "{text}");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorUserEvent {
    SupervisedPlayWake,
}

struct NativeEditorHost {
    application: EditorApplication,
    supervised_play_wake: SupervisedPlayWake,
    native: Option<NativeImgui>,
    retiring: Option<NativeImgui>,
    retirement_action: NativeRetirementAction,
    retirement_errors: NativeCloseErrors,
    window: Option<Arc<Window>>,
    retained: Option<PendingImguiFrontend>,
    gpu_recovery: EditorGpuRecovery,
    error: Option<EditorRunError>,
    last_drive: Option<Instant>,
    repaint_deadline: Option<Instant>,
}

enum PendingImguiFrontend {
    Fresh(ImguiEditorFrontend),
    Recovered(ImguiRetainedState),
}

struct ImguiRetainedState {
    frontend: ImguiEditorFrontend,
    layout: String,
    saved_layout: Option<crate::workspace::EditorWorkspaceLayout>,
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
    scene_presenter: ImguiScenePresenter,
    canvas: ImguiUiCanvasPresenter,
    workspace_generation: Option<u64>,
    frame_index: u64,
    last_frame: Instant,
    detachment: &'static str,
    layout: Option<crate::workspace::EditorWorkspaceLayout>,
    next_view: u64,
    scene_extent: Option<Extent2d>,
    close: NativeClose,
    rejected_completion: Option<EditorGameViewCompletion>,
    input: ImguiInput,
    game_candidate: Option<EditorGameViewCandidateIdentity>,
    game_source_active: bool,
    layout_loaded: bool,
    presented: bool,
    ui_followup: bool,
    #[cfg(test)]
    lifecycle_frame: Option<(WgpuEditorFrameOutcome, Vec<WgpuEditorFrameOutcome>)>,
    #[cfg(all(test, target_os = "windows"))]
    submitted_game_observation: Option<game_input_tests::SubmittedGameObservation>,
}

struct ImguiRenderer(Option<WgpuEditorImguiRenderer>);

impl ImguiRenderer {
    fn owner(&mut self) -> Result<&mut WgpuEditorImguiRenderer, EditorRunError> {
        self.0
            .as_mut()
            .ok_or_else(|| imgui_error("the Editor renderer has not been initialized"))
    }

    fn register(&mut self, view: &wgpu::TextureView) -> Result<ExternalTextureId, EditorRunError> {
        self.owner()?
            .register_external_texture(view)
            .map_err(platform_gpu_error)
    }

    fn unregister(&mut self, texture: ExternalTextureId) -> Result<(), EditorRunError> {
        self.owner()?
            .unregister_external_texture(texture)
            .map_err(platform_gpu_error)
    }

    fn prepare<'frame>(
        &mut self,
        gpu: &mut WgpuEditorExecution,
        window: RenderWindowPacket,
        event_loop: &ActiveEventLoop,
        frame: dear_imgui_rs::FrameToken<'frame>,
        submissions: &[WgpuSubmittedImageReceipt],
        outcomes: &mut Vec<WgpuEditorFrameOutcome>,
    ) -> Result<WgpuEditorImguiFrame<'frame>, EditorRunError> {
        gpu.prepare_imgui_frame(
            window,
            self.owner()?,
            event_loop,
            frame,
            submissions,
            outcomes,
        )
        .map_err(platform_gpu_error)
    }

    fn paint(
        &mut self,
        prepared: WgpuEditorImguiFrame<'_>,
        frame: &mut WgpuEditorWindowFrame<'_>,
    ) -> Result<(), WgpuEditorExecutionError> {
        let clear = wgpu::Color {
            r: 0.055,
            g: 0.06,
            b: 0.075,
            a: 1.0,
        };
        self.0
            .as_mut()
            .ok_or_else(|| {
                WgpuEditorExecutionError::toolkit_paint("the renderer is uninitialized")
            })?
            .paint(prepared, frame, clear)
    }

    fn shutdown(&mut self, context: &mut Context) -> Result<(), EditorRunError> {
        match self.0.as_mut() {
            Some(renderer) => renderer.shutdown(context).map_err(platform_gpu_error),
            None => Ok(()),
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
        if let Some(texture) = self.texture {
            renderer.unregister(texture)?;
            self.texture = None;
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
    fn new(
        event_loop: &ActiveEventLoop,
        window: Arc<Window>,
    ) -> Result<Self, NativeInitializationFailure> {
        // Context teardown may own deferred renderer resources after an initialization error.
        // Declare the engine GPU owner first so those resources retire before it on every return.
        let gpu = WgpuEditorExecution::new(
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
        let platform = WinitPlatform::new(&mut context).map_err(imgui_error)?;
        let native_detachment = native_detachment_supported(event_loop);
        let mut native = Self {
            window,
            context,
            platform,
            renderer: ImguiRenderer(None),
            gpu,
            frontend: ImguiEditorFrontend::default(),
            scene: ImguiTarget::default(),
            game: ImguiTarget::default(),
            scene_presenter: ImguiScenePresenter::new(),
            canvas: ImguiUiCanvasPresenter::default(),
            workspace_generation: None,
            frame_index: 0,
            last_frame: Instant::now(),
            detachment: if native_detachment {
                "Native detachment: experimental"
            } else {
                "Native detachment unavailable on this profile; in-window docking remains enabled"
            },
            layout: None,
            next_view: 0,
            scene_extent: None,
            close: NativeClose::default(),
            rejected_completion: None,
            input: ImguiInput::default(),
            game_candidate: None,
            game_source_active: false,
            layout_loaded: false,
            presented: false,
            ui_followup: true,
            #[cfg(test)]
            lifecycle_frame: None,
            #[cfg(all(test, target_os = "windows"))]
            submitted_game_observation: None,
        };
        if let Err(error) = native.initialize_graphics(native_detachment) {
            return Err(NativeInitializationFailure::retained(error, native));
        }
        Ok(native)
    }

    fn initialize_graphics(&mut self, native_detachment: bool) -> Result<(), EditorRunError> {
        self.platform
            .attach_window(
                Arc::clone(&self.window),
                HiDpiMode::Default,
                &mut self.context,
            )
            .map_err(imgui_error)?;
        if native_detachment && retirement::enable_native_platform_for_construction() {
            self.context.enable_multi_viewport();
            self.platform
                .enable_viewports(&mut self.context)
                .map_err(imgui_error)?;
            self.context.io_mut().set_config_dpi_scale_fonts(true);
            self.context.io_mut().set_config_dpi_scale_viewports(true);
        }
        let (drain, result) = self
            .gpu
            .with_device(editor_window_packet(&self.window), |scope| {
                let drain = scope.gpu_retirement();
                let renderer = scope.create_imgui_renderer(
                    &mut self.context,
                    native_detachment.then_some(&self.platform),
                );
                (drain, renderer)
            })
            .map_err(platform_gpu_error)?;
        self.close.drain = Some(drain);
        match result {
            Ok(renderer) => self.renderer.0 = Some(renderer),
            Err(failure) => {
                let (error, renderer) = failure.into_parts();
                self.renderer.0 = renderer;
                return Err(platform_gpu_error(error));
            }
        }
        Ok(())
    }

    fn redraw(
        &mut self,
        event_loop: &ActiveEventLoop,
        application: &mut EditorApplication,
    ) -> Result<bool, EditorRunError> {
        let result = self.redraw_frame(
            event_loop,
            application,
            #[cfg(test)]
            |_| {},
        );
        self.publish_render_views(application.panel_workspace_mut());
        result
    }

    #[cfg(test)]
    fn redraw_with_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        application: &mut EditorApplication,
        before_new_frame: impl FnOnce(&mut Context),
    ) -> Result<bool, EditorRunError> {
        let result = self.redraw_frame(event_loop, application, before_new_frame);
        self.publish_render_views(application.panel_workspace_mut());
        result
    }

    fn publish_render_views(&self, workspace: &mut crate::workspace::EditorPanelWorkspace) {
        let bindings = workspace
            .layout()
            .instances()
            .iter()
            .filter_map(|panel| {
                let target = match panel.kind() {
                    "nara.authoring-view" => &self.scene,
                    "nara.game-view" => &self.game,
                    _ => return None,
                };
                let view = target
                    .view
                    .filter(|_| panel.is_visible() && target.identity.is_some())
                    .map(|view| crate::workspace::EditorRenderViewId(view.get()));
                Some((panel.id(), view))
            })
            .collect::<Vec<_>>();
        for (panel, view) in bindings {
            workspace.layout_mut().set_render_view(panel, view);
        }
    }

    fn redraw_frame(
        &mut self,
        event_loop: &ActiveEventLoop,
        application: &mut EditorApplication,
        #[cfg(test)] before_new_frame: impl FnOnce(&mut Context),
    ) -> Result<bool, EditorRunError> {
        self.presented = false;
        #[cfg(test)]
        {
            self.lifecycle_frame = None;
        }
        let generation = application.workspace_generation();
        if self.workspace_generation != generation {
            self.input.invalidate(true);
            self.input.flush(application)?;
            self.scene.retire(&mut self.gpu, &mut self.renderer)?;
            self.scene_presenter.retire();
            self.canvas.retire();
            self.scene_extent = None;
            self.workspace_generation = generation;
        }
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
                self.layout = Some(application.panel_workspace().layout().clone());
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
        let mut canvas_rebuild_requested = false;
        if !self.frontend.scene_open(application.panel_workspace()) {
            self.scene.retire(&mut self.gpu, &mut self.renderer)?;
            self.scene_presenter.retire();
            self.canvas.retire();
        } else if let Some(authoring) = application.current_ui_authoring_frame() {
            self.scene_presenter.retire();
            canvas_rebuild_requested =
                self.submit_ui_canvas(application, &authoring, window_packet)?;
        } else {
            self.canvas.retire();
            if application
                .current_frame()
                .is_none_or(|frame| frame.scene_preview_document().is_none())
            {
                self.scene.retire(&mut self.gpu, &mut self.renderer)?;
                self.scene_presenter.retire();
            } else if let Some(candidate) = application.take_scene_preview_candidate() {
                self.scene_presenter.prepare(
                    &mut self.scene,
                    &mut self.gpu,
                    &mut self.renderer,
                    window_packet,
                    candidate,
                    &mut self.next_view,
                )?;
            }
        }
        self.prepare_game_start(application, window_packet)?;
        let pending_game = self.submit_game(application, window_packet)?;
        let pending_identity = pending_game.as_ref().map(|pending| pending.identity);
        let mut secondary_outcomes = Vec::new();
        let mut secondary_windows = Vec::new();
        let canvas_presented = if let Some(frame) = application.current_ui_authoring_frame() {
            self.canvas.is_current(&self.scene, &frame)
        } else {
            application.current_frame().is_some_and(|frame| {
                self.scene_presenter
                    .is_presented(&self.scene, &frame, self.scene_extent)
            })
        };
        let primary_viewport = self.context.main_viewport().id();
        let mut frontend_output = None;
        let rendered = (|| {
            self.platform
                .prepare_frame(&mut self.context, &self.window)
                .map_err(imgui_error)?;
            #[cfg(test)]
            before_new_frame(&mut self.context);
            let frame = self.context.begin_frame();
            frontend_output = Some(
                self.frontend
                    .show_with_views(
                        frame.ui(),
                        application,
                        self.scene.image(),
                        self.game.image(),
                        self.input.is_captured(),
                        canvas_presented,
                        self.detachment,
                    )
                    .map_err(imgui_error)?,
            );
            self.platform
                .prepare_render(frame.ui(), &self.window)
                .map_err(imgui_error)?;
            let submissions = [self.scene.submitted, self.game.submitted]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
            let prepared = self.renderer.prepare(
                &mut self.gpu,
                window_packet,
                event_loop,
                frame,
                &submissions,
                &mut secondary_outcomes,
            );
            let snapshots = self.platform.window_snapshots().map_err(imgui_error)?;
            for snapshot in snapshots
                .into_iter()
                .filter(|snapshot| !snapshot.target.is_main)
            {
                let window = self
                    .gpu
                    .imgui_window_id(snapshot.target.instance_id)
                    .map_err(platform_gpu_error)?;
                secondary_windows.push((snapshot.target.viewport_id, snapshot.window_id, window));
            }
            let prepared = prepared?;
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
            Ok(outcome)
        })();
        self.presented = secondary_outcomes
            .iter()
            .any(|outcome| outcome.window().is_some());
        let outcome = match rendered {
            Ok(rendered) => rendered,
            Err(error) => {
                self.input.invalidate(false);
                if let Some(output) = frontend_output.take() {
                    let authoring = application.current_ui_authoring_frame();
                    if let Some(batch) = self
                        .frontend
                        .reject_canvas_frame(output, authoring.as_deref())
                    {
                        self.frontend
                            .submit_and_drive(application, batch, Duration::ZERO);
                    }
                }
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
                self.input.flush(application)?;
                return Err(error);
            }
        };
        let mut output = frontend_output.expect("a rendered frame has its frontend output");
        self.presented = outcome.window().is_some()
            || secondary_outcomes
                .iter()
                .any(|outcome| outcome.window().is_some());
        for &(instance, viewport) in &output.panel_windows {
            if let Some((_, window)) = input::native_window(
                viewport,
                self.window.id(),
                primary_viewport,
                &secondary_windows,
            ) {
                application
                    .panel_workspace_mut()
                    .layout_mut()
                    .set_native_window(
                        instance,
                        crate::workspace::EditorNativeWindowId(window.get()),
                    );
            }
        }
        let authoring_window = output.authoring_viewport.and_then(|viewport| {
            input::native_window(
                viewport,
                self.window.id(),
                primary_viewport,
                &secondary_windows,
            )
            .map(|(_, window)| window)
        });
        let authoring_receipts = outcome
            .composites()
            .iter()
            .chain(
                secondary_outcomes
                    .iter()
                    .flat_map(WgpuEditorFrameOutcome::composites),
            )
            .copied()
            .filter(|receipt| Some(receipt.window().window_id()) == authoring_window)
            .collect::<Vec<_>>();
        let scene_promoted = if let Some(mapping) = output.scene_mapping {
            self.scene_presenter.promote(
                &self.scene,
                authoring_receipts.iter().copied(),
                mapping,
            )?
        } else {
            false
        };
        let canvas_promoted = self.canvas.promote(&self.scene, authoring_receipts)?;
        let authoring = application.current_ui_authoring_frame();
        let canvas_confirmed = authoring
            .as_deref()
            .is_some_and(|frame| self.canvas.is_current(&self.scene, frame));
        let canvas_mapping_changed = self.frontend.finish_canvas_presentation(
            &mut output,
            authoring.as_deref(),
            canvas_confirmed,
        );
        self.ui_followup = canvas_rebuild_requested
            || scene_promoted
            || canvas_promoted
            || canvas_mapping_changed
            || repaint::needs_followup(&self.context)
            || self.context.platform_io().viewports_iter().any(|viewport| {
                viewport.platform_request_move()
                    || viewport.platform_request_resize()
                    || viewport.platform_request_close()
                    || !viewport.platform_window_created()
            });
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
            application.active_game_start_request(),
        );
        self.input.flush(application)?;
        #[cfg(test)]
        {
            self.lifecycle_frame = Some((outcome, secondary_outcomes));
        }
        if output.save_layout {
            let mut layout = String::new();
            self.context.save_ini_settings(&mut layout);
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
            self.layout = Some(application.panel_workspace().layout().clone());
        }
        if output.restore_layout
            && let Some(layout) = &self.layout
        {
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
            restore_workspace_layout(
                &mut self.context,
                application.panel_workspace_mut(),
                layout,
                &displays,
            );
            self.frontend.retire_panel_widgets();
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
        if output.reset_layout || output.restore_layout || output.layout_changed {
            self.input.invalidate(false);
            self.input.flush(application)?;
            self.ui_followup = true;
            self.window.request_redraw();
        }
        if output.scene_camera_changed {
            self.window.request_redraw();
        }
        if let Some((point, mapping)) = output.scene_pick_point.zip(output.scene_mapping)
            && let Some(frame) = application.current_frame()
            && let Some(action) = self.scene_presenter.resolve_pick(&frame, point, mapping)
            && let Some(batch) = output.actions.take()
        {
            let revision = batch.frame_revision();
            let mut actions = batch.into_actions();
            actions.push(action);
            output.actions = Some(EditorActionBatch::new(revision, actions));
        }
        if let Some(uv) = output.canvas_pick_uv
            && let Some(frame) = application.current_ui_authoring_frame()
            && let Some(action) = self.canvas.resolve_pick(&self.scene, &frame, uv)
            && let Some(batch) = output.actions.take()
        {
            let revision = batch.frame_revision();
            let mut actions = batch.into_actions();
            actions.push(action);
            output.actions = Some(EditorActionBatch::new(revision, actions));
        }
        let extent = application
            .current_ui_authoring_frame()
            .is_none()
            .then_some(
                output
                    .scene_extent
                    .and_then(|size| Extent2d::new(size[0].round() as u32, size[1].round() as u32)),
            )
            .flatten();
        let game_extent = output
            .game_extent
            .and_then(|size| Extent2d::new(size[0].round() as u32, size[1].round() as u32))
            .filter(|extent| {
                self.frontend.game_open(application.panel_workspace())
                    && application
                        .active_game_start_request()
                        .is_some_and(|start| {
                            start.resizable()
                                && (start.logical_window().resolution.physical_width
                                    != extent.width
                                    || start.logical_window().resolution.physical_height
                                        != extent.height)
                        })
            });
        if game_extent.is_some() {
            // Release the captured route while its exact presented geometry still belongs to
            // the Runtime. Resizing first would reject this old stamp on the next input flush.
            self.input.invalidate(false);
            self.input.flush(application)?;
        }
        let mut local_changed = false;
        let dispatch = self
            .frontend
            .seal_output(&mut output)
            .and_then(|submission| {
                let dispatch = self.frontend.dispatch_submission(application, submission)?;
                if dispatch.invalidates_game_input() {
                    self.input
                        .invalidate_after_control(application.active_game_start_request());
                    self.game_candidate = None;
                }
                Some(dispatch)
            });
        for action in output.browser_actions {
            local_changed |= application.apply_project_browser_local_action(action);
        }
        if let Some(action) = output.shell_action {
            let update = application.submit_shell_action(action);
            if update.exit_ready() {
                if let Some(dispatch) = &dispatch {
                    self.frontend.finish_dispatch(application, dispatch);
                }
                return Ok(true);
            }
            local_changed |= update.actions_observed();
        }
        if let Some(action) = output.project_action {
            let update = application.submit_project_action(action);
            if update.exit_ready() {
                if let Some(dispatch) = &dispatch {
                    self.frontend.finish_dispatch(application, dispatch);
                }
                return Ok(true);
            }
            local_changed |= update.actions_observed();
        }
        if extent != self.scene_extent {
            self.scene_extent = extent;
            application.set_scene_preview_extent(extent);
            self.window.request_redraw();
        }
        let resized = if let Some(extent) = game_extent
            && application.active_game_start_request().is_some()
        {
            let resized = application.resize_game_view(extent).map_err(imgui_error)?;
            if resized {
                self.game_candidate = None;
                self.game.submitted = None;
                self.window.request_redraw();
            }
            resized
        } else {
            false
        };
        if dispatch.is_some() || resized {
            let actions_observed = dispatch
                .as_ref()
                .is_some_and(|dispatch| dispatch.actions_observed());
            let drive_runtime = resized
                || dispatch.as_ref().is_some_and(|dispatch| {
                    dispatch.requires_runtime_drive() || dispatch.requests_step()
                });
            let update = application.drive_editor_frame(
                Duration::ZERO,
                actions_observed || local_changed,
                drive_runtime,
            );
            if let Some(dispatch) = &dispatch {
                self.frontend.finish_dispatch(application, dispatch);
            }
            if update.actions_observed() {
                self.window.request_redraw();
            }
            return Ok(update.exit_ready());
        }
        if local_changed {
            self.window.request_redraw();
        }
        Ok(false)
    }

    fn submit_ui_canvas(
        &mut self,
        application: &mut EditorApplication,
        authoring: &nara::project_host::EditorUiAuthoringFrame,
        window: RenderWindowPacket,
    ) -> Result<bool, EditorRunError> {
        if self.canvas.has_current_submission(&self.scene, authoring) {
            return Ok(false);
        }
        let Some(candidate) = application.take_editor_ui_preview_candidate() else {
            // An empty take on Ready requests a rebuild during the following owner drive.
            return Ok(authoring.projection().is_ok_and(|projection| {
                matches!(
                    projection.canvas(),
                    nara::ui::__private::UiAuthoringCanvasProjection::Ready(_)
                )
            }));
        };
        self.canvas.prepare(
            &mut self.scene,
            &mut self.gpu,
            &mut self.renderer,
            window,
            candidate,
            &mut self.next_view,
        )?;
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
                self.input
                    .invalidate_after_control(application.active_game_start_request());
                self.game_candidate = None;
                self.game.retire(&mut self.gpu, &mut self.renderer)?;
            }
            return Ok(());
        }
        if let Some(request) = application.pending_game_start_request() {
            let (source, target) = game_target(request, &self.game)?;
            if self.frontend.game_open(application.panel_workspace()) {
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
            if !self.frontend.game_open(application.panel_workspace()) {
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
            let (source, target) = game_target(request, &self.game)?;
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
        let world_presentation_revoker = candidate.world_presentation_revoker();
        let (identity, packet, sprites, runtime_ui) =
            candidate.into_parts().into_submission_parts();
        #[cfg(all(test, target_os = "windows"))]
        let content = game_input_tests::GameCandidateContent::new(
            sprites.as_ref(),
            runtime_ui.as_ref().is_some_and(|frame| frame.has_packets()),
        );
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
        if let Some(revoker) = world_presentation_revoker {
            if let Err(error) = self
                .gpu
                .retain_game_world_presentation(view, window, revoker)
            {
                let completion = match runtime_ui {
                    Some(ui) => EditorGameViewCompletion::with_ui_failed(
                        identity,
                        WgpuEditorExecution::reject_editor_ui_frame(ui, error.to_string()),
                    ),
                    None => EditorGameViewCompletion::without_ui_failed(identity),
                };
                self.return_game_completion(application, completion)?;
                return Err(platform_gpu_error(error));
            }
        }
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
        #[cfg(all(test, target_os = "windows"))]
        {
            self.submitted_game_observation = Some(game_input_tests::SubmittedGameObservation {
                image,
                identity,
                content,
            });
        }
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
}

impl NativeEditorHost {
    fn new(
        application: EditorApplication,
        supervised_play_wake: SupervisedPlayWake,
        frontend: ImguiEditorFrontend,
    ) -> Self {
        Self {
            application,
            supervised_play_wake,
            native: None,
            retiring: None,
            retirement_action: NativeRetirementAction::Hold,
            retirement_errors: NativeCloseErrors::default(),
            window: None,
            retained: Some(PendingImguiFrontend::Fresh(frontend)),
            gpu_recovery: EditorGpuRecovery::Ready,
            error: None,
            last_drive: None,
            repaint_deadline: None,
        }
    }

    fn install_native(&mut self, event_loop: &ActiveEventLoop) -> Result<(), EditorRunError> {
        if self.native.is_some() || self.retiring.is_some() {
            return Err(imgui_error("the predecessor native owner has not retired"));
        }
        let window = self
            .window
            .as_ref()
            .cloned()
            .ok_or_else(|| imgui_error("the primary window is unavailable"))?;
        let mut native = match NativeImgui::new(event_loop, window) {
            Ok(native) => native,
            Err(failure) => {
                let (error, native) = failure.into_parts();
                self.retiring = native;
                return Err(error);
            }
        };
        if let Some(retained) = self.retained.take() {
            match retained {
                PendingImguiFrontend::Fresh(frontend) => native.frontend = frontend,
                PendingImguiFrontend::Recovered(retained) => {
                    native.context.load_ini_settings(&retained.layout);
                    native.frontend = retained.frontend;
                    native.layout = retained.saved_layout;
                    native.frame_index = retained.frame_index;
                    native.next_view = retained.next_view;
                    native.layout_loaded = true;
                }
            }
        }
        native.window.set_title(super::EDITOR_TITLE);
        native.window.request_redraw();
        self.native = Some(native);
        self.last_drive = None;
        self.repaint_deadline = None;
        Ok(())
    }

    fn retire_native(&mut self) {
        self.application.set_scene_preview_extent(None);
        if self.retiring.is_none() {
            self.retiring = self.native.take();
        }
    }

    fn drive_native_retirement(&mut self, event_loop: &ActiveEventLoop) {
        if self.retirement_action == NativeRetirementAction::AwaitingExitReady {
            if !self.application.has_pending_exit() {
                // A cancelled/rejected semantic close returns authority to the live frontend.
                self.retirement_action = if self.native.is_some() {
                    NativeRetirementAction::Hold
                } else {
                    NativeRetirementAction::Install
                };
            }
        }
        if let Some(native) = &mut self.retiring {
            if !native.poll_graphics_close() {
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(16),
                ));
                return;
            }
        }
        if let Some(mut native) = self.retiring.take() {
            let layout = native.close.saved_layout.take().unwrap_or_default();
            if !native.close.errors.is_empty() {
                eprintln!("Nara Editor GPU retirement: {}", native.close.errors);
            }
            self.retirement_errors
                .append(std::mem::take(&mut native.close.errors));
            native.publish_render_views(self.application.panel_workspace_mut());
            // A construction failure has no frontend to recover; keep the preceding one.
            if self.retained.is_none() {
                self.retained = Some(PendingImguiFrontend::Recovered(ImguiRetainedState {
                    frontend: std::mem::take(&mut native.frontend),
                    layout,
                    saved_layout: native.layout.take(),
                    frame_index: native.frame_index,
                    next_view: native.next_view,
                }));
            }
        }
        match self.retirement_action {
            NativeRetirementAction::Install | NativeRetirementAction::InstallForExit => {
                self.retirement_action =
                    if self.retirement_action == NativeRetirementAction::InstallForExit {
                        NativeRetirementAction::AwaitingExitReady
                    } else {
                        NativeRetirementAction::Hold
                    };
                if let Err(error) = self.install_native(event_loop) {
                    self.await_explicit_retry(&error);
                }
            }
            NativeRetirementAction::Exit => {
                event_loop.exit();
            }
            NativeRetirementAction::Hold | NativeRetirementAction::AwaitingExitReady => {}
        }
    }

    fn exit_after_native_retirement(&mut self, event_loop: &ActiveEventLoop) {
        if self.application.request_exit().exit_ready() {
            self.retirement_action = NativeRetirementAction::Exit;
            self.retire_native();
        } else {
            self.retirement_action = if matches!(
                self.retirement_action,
                NativeRetirementAction::Install | NativeRetirementAction::InstallForExit
            ) {
                NativeRetirementAction::InstallForExit
            } else {
                NativeRetirementAction::AwaitingExitReady
            };
            if let Some(native) = &self.native {
                native.window.request_redraw();
            }
        }
        self.drive_native_retirement(event_loop);
        if self.retirement_action.needs_exit_maintenance() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
        }
    }

    fn await_explicit_retry(&mut self, error: &EditorRunError) {
        if self.retirement_action == NativeRetirementAction::InstallForExit {
            // Preserve the close decision but stop automatic reconstruction after a failed retry.
            self.retirement_action = NativeRetirementAction::AwaitingExitReady;
        } else if !self.retirement_action.needs_exit_maintenance() {
            self.retirement_action = NativeRetirementAction::Hold;
        }
        self.retire_native();
        self.gpu_recovery.require_explicit_retry();
        if let Some(window) = &self.window {
            window.set_title(super::EDITOR_GPU_UNAVAILABLE_TITLE);
        }
        eprintln!("Nara Editor GPU recovery paused: {error}");
    }

    fn replace_native(&mut self, event_loop: &ActiveEventLoop) {
        if self.retirement_action == NativeRetirementAction::Exit {
            return;
        }
        self.retirement_action = if self.retirement_action.needs_exit_maintenance() {
            NativeRetirementAction::InstallForExit
        } else {
            NativeRetirementAction::Install
        };
        self.retire_native();
        self.drive_native_retirement(event_loop);
    }

    fn handle_gpu_failure(
        &mut self,
        event_loop: &ActiveEventLoop,
        error: EditorRunError,
    ) -> Result<(), EditorRunError> {
        if let Some(native) = &mut self.native {
            native.close.device_lost = matches!(
                &error,
                EditorRunError::Gpu(WgpuEditorExecutionError::Backend(
                    nara::render_wgpu::WgpuRenderError::DeviceLost
                ))
            );
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
            self.exit_after_native_retirement(event_loop);
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

impl ApplicationHandler<EditorUserEvent> for NativeEditorHost {
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: EditorUserEvent) {
        match event {
            EditorUserEvent::SupervisedPlayWake => {
                self.supervised_play_wake.acknowledge();
                let update = self
                    .application
                    .drive_supervised_play_effects(Instant::now());
                if update.exit_ready() {
                    self.exit_after_native_retirement(event_loop);
                    return;
                }
                if update.actions_observed()
                    && let Some(window) = &self.window
                {
                    window.request_redraw();
                    self.repaint_deadline = None;
                    event_loop.set_control_flow(ControlFlow::Wait);
                    return;
                }
                match earliest_deadline(
                    self.repaint_deadline,
                    imgui_next_deadline(&self.application, self.native.as_ref(), Instant::now()),
                ) {
                    Some(deadline) => event_loop.set_control_flow(ControlFlow::WaitUntil(deadline)),
                    None => event_loop.set_control_flow(ControlFlow::Wait),
                }
                if self.retiring.is_some() || self.retirement_action.needs_exit_maintenance() {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(16),
                    ));
                }
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.native.is_some()
            || self.retiring.is_some()
            || self.retirement_action == NativeRetirementAction::Exit
        {
            return;
        }
        if self.window.is_none() {
            match event_loop.create_window(
                Window::default_attributes()
                    .with_title(super::EDITOR_TITLE)
                    .with_inner_size(LogicalSize::new(1200.0, 800.0)),
            ) {
                Ok(window) => self.window = Some(Arc::new(window)),
                Err(error) => {
                    self.error = Some(imgui_error(error));
                    self.exit_after_native_retirement(event_loop);
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
                self.exit_after_native_retirement(event_loop);
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
                self.exit_after_native_retirement(event_loop);
                return;
            }
        }
        let Some(native) = &mut self.native else {
            return;
        };
        let is_primary = native.window.id() == window_id;
        let owned = match native.platform.window_snapshots() {
            Ok(snapshots) => snapshots
                .iter()
                .any(|snapshot| snapshot.window_id == window_id),
            Err(error) => {
                self.error = Some(imgui_error(error));
                self.exit_after_native_retirement(event_loop);
                return;
            }
        };
        if !owned {
            return;
        }
        let forward_to_toolkit = native
            .input
            .handle_event(window_id, &event, &native.context);
        let needs_frame = if forward_to_toolkit {
            match native
                .platform
                .handle_window_event(&mut native.context, window_id, &event)
            {
                Ok(outcome) => {
                    outcome.is_owned()
                        && (outcome.needs_frame
                            || matches!(
                                outcome.platform_action,
                                WinitPlatformAction::ViewportCloseRequested
                                    | WinitPlatformAction::GeometryChanged
                                    | WinitPlatformAction::NativeDestroyed
                            ))
                }
                Err(error) => {
                    self.error = Some(imgui_error(error));
                    self.exit_after_native_retirement(event_loop);
                    return;
                }
            }
        } else {
            true
        };
        if let Err(error) = native.input.flush(&mut self.application) {
            self.error = Some(error);
            self.exit_after_native_retirement(event_loop);
            return;
        }
        match event {
            WindowEvent::CloseRequested if is_primary => {
                self.exit_after_native_retirement(event_loop);
            }
            WindowEvent::RedrawRequested if is_primary => {
                let running_before = self.application.current_frame().is_some_and(|frame| {
                    imgui_play_cadence(frame.play().state(), frame.runtime_availability()).running
                });
                match native.redraw(event_loop, &mut self.application) {
                    Ok(true) => self.exit_after_native_retirement(event_loop),
                    Ok(false) => {
                        if native.presented {
                            self.gpu_recovery.mark_presentation_ready();
                        }
                    }
                    Err(error) => {
                        if let Err(error) = self.handle_gpu_failure(event_loop, error) {
                            self.error = Some(error);
                            self.exit_after_native_retirement(event_loop);
                        }
                    }
                }
                let running_after = self.application.current_frame().is_some_and(|frame| {
                    imgui_play_cadence(frame.play().state(), frame.runtime_availability()).running
                });
                if !running_before || !running_after {
                    // A repaint can accept Resume before the next about_to_wait callback.
                    // Only a continuous Running interval belongs to the simulation clock.
                    self.last_drive = running_after.then(Instant::now);
                }
            }
            _ if needs_frame => native.window.request_redraw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.drive_native_retirement(event_loop);
        if self.retirement_action == NativeRetirementAction::Exit {
            if self.retiring.is_some() {
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(16),
                ));
            }
            return;
        }
        let now = Instant::now();
        let open_update = self.application.advance_project_open();
        if open_update.exit_ready() {
            self.exit_after_native_retirement(event_loop);
            return;
        }
        let play_before = self
            .application
            .current_frame()
            .map(|frame| frame.play().state());
        let cadence = self
            .application
            .current_frame()
            .map(|frame| imgui_play_cadence(frame.play().state(), frame.runtime_availability()))
            .unwrap_or_default();
        let delta = if cadence.running {
            self.last_drive
                .replace(now)
                .map_or(Duration::ZERO, |last| now.saturating_duration_since(last))
        } else {
            self.last_drive = None;
            Duration::ZERO
        };
        let running = cadence.drive_runtime || self.application.game_step_requires_runtime_drive();
        if running && let Some(native) = &mut self.native {
            native.input.mark_presented_stale();
        }
        let update =
            self.application
                .drive_editor_frame(delta, open_update.actions_observed(), running);
        if update.exit_ready() {
            self.exit_after_native_retirement(event_loop);
            return;
        }
        self.advance_recovery(event_loop);
        let play_after = self
            .application
            .current_frame()
            .map(|frame| frame.play().state());
        if self.application.current_frame().is_some_and(|frame| {
            imgui_play_cadence(frame.play().state(), frame.runtime_availability()).running
        }) {
            self.last_drive.get_or_insert_with(Instant::now);
        } else {
            self.last_drive = None;
        }
        let play_changed = play_before != play_after;
        let deadline = imgui_next_deadline(&self.application, self.native.as_ref(), now);
        let wake_due = self
            .repaint_deadline
            .is_some_and(|deadline| now >= deadline);
        let browser_changed = self
            .application
            .current_frame()
            .is_some_and(|frame| frame.project_browser().work_observed());
        if (update.actions_observed() || play_changed || browser_changed || wake_due)
            && let Some(native) = &self.native
        {
            self.repaint_deadline = None;
            native.window.request_redraw();
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if let Some(deadline) = deadline {
            self.repaint_deadline =
                imgui_next_presentation_deadline(&self.application, self.native.as_ref(), now);
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
        }
        if self.retiring.is_some() || self.retirement_action.needs_exit_maintenance() {
            event_loop.set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(16)));
        }
    }
}

fn imgui_next_deadline(
    application: &EditorApplication,
    native: Option<&NativeImgui>,
    now: Instant,
) -> Option<Instant> {
    let mut deadline = earliest_deadline(
        application.next_supervised_play_deadline(),
        imgui_next_presentation_deadline(application, native, now),
    );
    if matches!(
        application.state(),
        crate::EditorApplicationState::OpeningInitial
            | crate::EditorApplicationState::OpeningReplacement
    ) || application.project_open_retirement_pending()
    {
        deadline = earliest_deadline(deadline, Some(now + Duration::from_millis(16)));
    }
    if let Some(frame) = application.current_frame() {
        deadline = earliest_deadline(deadline, imgui_frame_maintenance_deadline(&frame, now));
    }
    deadline = earliest_deadline(deadline, application.session_close_retry_deadline());
    deadline
}

fn imgui_frame_maintenance_deadline(
    frame: &nara::tooling::EditorFrameModel,
    now: Instant,
) -> Option<Instant> {
    let deadline = (frame.project_browser().has_pending_work()
        || frame.document_open().has_pending_work()
        || frame.authoring_retirement_pending()
        || matches!(
            frame.project_content(),
            nara::tooling::EditorProjectContentView::RefreshPending { .. }
        ))
    .then_some(now + Duration::from_millis(16));
    earliest_deadline(
        earliest_deadline(
            deadline,
            play_lifecycle_poll_deadline(now, frame.play().state()),
        ),
        persistence_poll_deadline(now, frame.persistence().operation()),
    )
}

fn imgui_next_presentation_deadline(
    application: &EditorApplication,
    native: Option<&NativeImgui>,
    now: Instant,
) -> Option<Instant> {
    let mut deadline = None;
    if let Some(frame) = application.current_frame() {
        deadline = imgui_frame_presentation_deadline(&frame, now);
    }
    if application.game_step_requires_runtime_drive() {
        deadline = earliest_deadline(deadline, Some(now + Duration::from_millis(16)));
    }
    let Some(native) = native else {
        return deadline;
    };
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

fn imgui_frame_presentation_deadline(
    frame: &nara::tooling::EditorFrameModel,
    now: Instant,
) -> Option<Instant> {
    let running = imgui_play_cadence(frame.play().state(), frame.runtime_availability()).running;
    earliest_deadline(
        running.then_some(now + Duration::from_millis(16)),
        image_import_poll_deadline(
            now,
            frame.image_import_work_pending() || frame.source_asset_import_work_pending(),
        ),
    )
}

#[derive(Default)]
struct ImguiPlayCadence {
    running: bool,
    drive_runtime: bool,
}

fn imgui_play_cadence(
    state: EditorPlayState,
    availability: EditorRuntimeAvailability,
) -> ImguiPlayCadence {
    let embedded = availability == EditorRuntimeAvailability::TrustedInProcess;
    ImguiPlayCadence {
        running: embedded && state == EditorPlayState::Running,
        drive_runtime: embedded
            && matches!(
                state,
                EditorPlayState::Running
                    | EditorPlayState::Starting
                    | EditorPlayState::Stepping
                    | EditorPlayState::Stopping
                    | EditorPlayState::RetiringPlay
                    | EditorPlayState::PreparingPlay
            ),
    }
}

fn restore_workspace_layout(
    context: &mut Context,
    workspace: &mut crate::workspace::EditorPanelWorkspace,
    saved: &crate::workspace::EditorWorkspaceLayout,
    displays: &[crate::workspace::EditorWindowBounds],
) {
    workspace.restore(saved.clone(), "dear-imgui", 1, displays);
    if let Some(toolkit) = workspace.layout().toolkit_layout() {
        context.load_ini_settings(toolkit.data());
    }
}

#[cfg(test)]
mod opening_tests {
    use super::*;
    use crate::EditorLaunchOptions;
    use nara::tooling::EditorAction;

    #[test]
    fn external_running_observation_does_not_drive_embedded_runtime_or_animation() {
        let cadence = imgui_play_cadence(
            EditorPlayState::Running,
            EditorRuntimeAvailability::ExternalSupervised,
        );
        assert!(
            !cadence.running,
            "external Play has its own presentation owner"
        );
        assert!(!cadence.drive_runtime);
    }

    #[test]
    fn embedded_run_and_step_keep_their_runtime_drive() {
        let running = imgui_play_cadence(
            EditorPlayState::Running,
            EditorRuntimeAvailability::TrustedInProcess,
        );
        assert!(running.running);
        assert!(running.drive_runtime);
        let stepping = imgui_play_cadence(
            EditorPlayState::Stepping,
            EditorRuntimeAvailability::TrustedInProcess,
        );
        assert!(!stepping.running);
        assert!(stepping.drive_runtime);
        let paused = imgui_play_cadence(
            EditorPlayState::Paused,
            EditorRuntimeAvailability::TrustedInProcess,
        );
        assert!(!paused.running);
        assert!(!paused.drive_runtime);
    }

    #[test]
    fn restore_layout_recovers_saved_panel_instances_and_visibility_with_the_ini() {
        use crate::workspace::{EditorPanelTarget, EditorPanelWorkspace, EditorToolkitLayout};

        let mut workspace = EditorPanelWorkspace::default();
        let inspector = workspace
            .layout()
            .instances()
            .iter()
            .find(|panel| panel.kind() == "nara.inspector")
            .unwrap()
            .id();
        workspace
            .layout_mut()
            .set_toolkit_layout(Some(EditorToolkitLayout::new(
                "dear-imgui",
                1,
                "[Window][saved-layout-marker]\nPos=42,64\nSize=320,240\nCollapsed=0\n",
            )));
        let saved = workspace.layout().clone();
        workspace.layout_mut().close(inspector);
        let extra = workspace
            .open("nara.inspector", EditorPanelTarget::ActiveDocument)
            .unwrap();
        assert_ne!(extra, inspector);
        let mut context = Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        restore_workspace_layout(&mut context, &mut workspace, &saved, &[]);
        let mut ini = String::new();
        context.save_ini_settings(&mut ini);
        assert!(ini.contains("saved-layout-marker"));
        assert_eq!(
            workspace.layout(),
            &saved,
            "Restore must recover the same panel set and open state as its saved dock tree"
        );
    }

    #[test]
    fn generic_project_open_keeps_a_bounded_wake_without_native_input() {
        let missing = std::env::temp_dir().join(format!(
            "nara-imgui-missing-project-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut application = EditorApplication::new(EditorLaunchOptions::new(missing));
        assert!(application.native_shell_ready().actions_observed());
        assert_eq!(
            application.state(),
            crate::EditorApplicationState::OpeningInitial
        );
        let now = Instant::now();
        assert_eq!(
            imgui_next_deadline(&application, None, now),
            Some(now + Duration::from_millis(16))
        );

        assert!(application.advance_project_open().actions_observed());
        assert_eq!(application.state(), crate::EditorApplicationState::Recovery);
        assert_eq!(imgui_next_deadline(&application, None, now), None);
        let manager = EditorApplication::new(EditorLaunchOptions::project_manager());
        assert_eq!(imgui_next_deadline(&manager, None, now), None);
    }

    #[test]
    fn ordinary_browser_pending_work_keeps_its_host_deadline_until_completion() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generic-editor/known-unbound");
        let mut application =
            EditorApplication::new(EditorLaunchOptions::new(root).with_profile("desktop"));
        application.native_shell_ready();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if application.current_frame().is_some() {
                break;
            }
            application.advance_project_open();
            std::thread::sleep(Duration::from_millis(1));
        }
        let frame = application
            .current_frame()
            .expect("the ordinary fixture opens");
        application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::ProjectBrowser(
                    nara::tooling::EditorProjectBrowserAction::Refresh,
                )],
            ),
            Duration::ZERO,
        );
        let now = Instant::now();
        let frame = application.current_frame().unwrap();
        assert!(frame.project_browser().has_pending_work());
        let pending_deadline = imgui_next_deadline(&application, None, now);
        let pending_presentation_deadline =
            imgui_next_presentation_deadline(&application, None, now);
        let browser_deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < browser_deadline {
            let frame = application.current_frame().unwrap();
            if !frame.project_browser().has_pending_work() {
                break;
            }
            application
                .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            !application
                .current_frame()
                .unwrap()
                .project_browser()
                .has_pending_work()
        );
        let settled_deadline = imgui_next_deadline(&application, None, now);
        let retiring_frame = application
            .current_frame()
            .unwrap()
            .as_ref()
            .clone()
            .with_image_import(None, false)
            .with_source_asset_import(None, false)
            .with_authoring_retirement_pending(true);
        let retirement_deadline = imgui_frame_maintenance_deadline(&retiring_frame, now);
        let retirement_presentation = imgui_frame_presentation_deadline(&retiring_frame, now);
        let retired_frame = retiring_frame.with_authoring_retirement_pending(false);
        let retired_deadline = imgui_frame_maintenance_deadline(&retired_frame, now);
        let other_work = application
            .current_frame()
            .unwrap()
            .image_import_work_pending()
            || application.next_supervised_play_deadline().is_some()
            || application.game_step_requires_runtime_drive();
        let mut closed = application.request_exit().exit_ready();
        for _ in 0..128 {
            if closed {
                break;
            }
            closed = application
                .drive_editor_frame(Duration::ZERO, false, true)
                .exit_ready();
            if !closed {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        assert_eq!(pending_deadline, Some(now + Duration::from_millis(16)));
        assert_eq!(
            pending_presentation_deadline, None,
            "a maintenance wake observes work; unchanged work does not request presentation"
        );
        assert!(
            settled_deadline.is_none() || other_work,
            "the browser must release its wake after settling"
        );
        assert!(closed);
        assert_eq!(retirement_deadline, Some(now + Duration::from_millis(16)));
        assert_eq!(retirement_presentation, None);
        assert_eq!(retired_deadline, None);
    }
}

fn game_target(
    request: EditorGameStartRequest,
    current: &ImguiTarget,
) -> Result<(RenderFrameSourceGeneration, RenderImagePacket), EditorRunError> {
    let resolution = request.logical_window().resolution;
    let extent = Extent2d::new(resolution.physical_width, resolution.physical_height)
        .ok_or_else(|| imgui_error("Game logical window has zero extent"))?;
    let source = RenderFrameSourceGeneration::from(request.runtime_generation());
    let image = match current.target.filter(|_| current.source == Some(source)) {
        Some(target) => target.image(),
        None => nara::render::RenderImage2d::allocate_handle().map_err(imgui_error)?,
    };
    let target = editor_viewport_image_target(image, extent).map_err(imgui_error)?;
    Ok((source, target))
}

fn imgui_error(error: impl std::fmt::Display) -> EditorRunError {
    EditorRunError::Platform(error.to_string())
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
