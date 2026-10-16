use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use egui::ViewportId;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor, wgpu};
use nara::{
    project_host::TrustedCompiledEditorProjectIntent,
    render::{Extent2d, RenderWindowPacket},
    render_wgpu::__private::{
        WgpuEditorDeviceScope, WgpuEditorExecution, WgpuEditorExecutionError, WgpuEditorViewId,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorNavigationAction, EditorPlayAction, EditorPlayState,
    },
    tooling_egui::{
        EguiAuthoringViewportOutput, EguiCentralViewportInput, EguiGameViewInput,
        EguiGameViewOutput, EguiSceneViewInput, EguiUiCanvasInput,
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
    event::{ElementState, KeyEvent, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::app::{EditorApplicationDispatch, EditorApplicationSurface};
use crate::frontend::EguiEditorFrontend;
use crate::supervised_play::SupervisedPlayWake;
use crate::{
    EditorApplication, EditorApplicationOpenError, EditorApplicationUpdate, EditorLaunchOptions,
};

mod canary;
mod game_input;
mod generic_open_canary;
mod image_import_canary;
mod supervised_play_canary;
mod ui_canary;
mod viewport;
mod views;

use views::EditorViewInstances;

pub use canary::{
    EditorGameViewCanaryEvidence, EditorGameViewCanaryFrame, EditorGameViewCanaryRequest,
};
pub use supervised_play_canary::EditorSupervisedPlayCanaryEvidence;
pub use ui_canary::{
    EditorUiAuthoringCanaryEvidence, EditorUiAuthoringCanaryFrame, EditorUiAuthoringCanaryRequest,
};

use canary::{EditorGameViewCanaryFrameInput, EditorGameViewCanaryHost};
use game_input::EditorGameInputRouter;
use generic_open_canary::EditorGenericOpenCanaryHost;
use image_import_canary::EditorImageImportCanaryHost;
use viewport::{
    EditorGamePrepareOutcome, EditorGamePresenter, EditorScenePickRequest, EditorScenePresenter,
    EditorUiCanvasIdentity, EditorUiCanvasPickRequest, EditorUiCanvasPresenter, EditorViewportSlot,
    PreparedEditorViewportFrame, append_editor_action, physical_viewport_extent,
    primitives_sample_texture, set_requested_scene_extent,
};

const EDITOR_TITLE: &str = "Nara Editor";
const INITIAL_WIDTH: f64 = 1200.0;
const INITIAL_HEIGHT: f64 = 800.0;
const MIN_WIDTH: f64 = 720.0;
const MIN_HEIGHT: f64 = 520.0;
const CLEAR_COLOR: [f32; 4] = [0.055, 0.06, 0.075, 1.0];
const EDITOR_GPU_UNAVAILABLE_TITLE: &str = "Nara Editor - GPU unavailable";
const GAME_VIEW_PENDING_REPAINT_DELAY: Duration = Duration::from_millis(16);
const PLAY_DRIVE_CADENCE: Duration = Duration::from_millis(16);
const PLAY_LIFECYCLE_POLL_DELAY: Duration = Duration::from_millis(16);
const IMAGE_IMPORT_POLL_DELAY: Duration = Duration::from_millis(16);

#[derive(Debug, Error)]
pub enum EditorRunError {
    #[error("{0}")]
    Usage(String),
    #[error(transparent)]
    TrustedOpen(#[from] EditorApplicationOpenError),
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
            Self::TrustedOpen(error) => error.diagnostics(),
            Self::Usage(_) | Self::EventLoop(_) | Self::Platform(_) | Self::Gpu(_) => None,
        }
    }

    fn requires_gpu_replacement(&self) -> bool {
        matches!(self, Self::Gpu(error) if error.requires_owner_replacement())
    }
}

pub fn run(options: EditorLaunchOptions) -> Result<(), EditorRunError> {
    let canary_count = [
        image_import_canary::requested(),
        generic_open_canary::requested(),
        supervised_play_canary::requested(),
    ]
    .into_iter()
    .filter(|requested| *requested)
    .count();
    if canary_count > 1 {
        return Err(EditorRunError::usage(
            "only one native Editor canary may be requested per launch",
        ));
    }
    if image_import_canary::requested() {
        return run_image_import_canary(options);
    }
    if supervised_play_canary::requested() {
        return run_supervised_play_canary(options);
    }
    if generic_open_canary::requested() {
        return run_generic_open_canary(options);
    }
    run_application(EditorApplication::new(options))
}

/// Runs one private first-party Editor with an explicitly trusted compiled product intent.
#[doc(hidden)]
pub fn run_trusted_compiled(
    options: EditorLaunchOptions,
    intent: TrustedCompiledEditorProjectIntent,
) -> Result<(), EditorRunError> {
    let application = EditorApplication::open_trusted_compiled(options, intent)?;
    run_application(application)
}

fn run_application(mut application: EditorApplication) -> Result<(), EditorRunError> {
    if let Some(preferences) = crate::workspace::EditorWorkspacePreferences::user_default() {
        application
            .panel_workspace_mut()
            .set_preferences(preferences);
    }
    let _host = drive_native_editor(application, None)?;
    Ok(())
}

fn run_generic_open_canary(options: EditorLaunchOptions) -> Result<(), EditorRunError> {
    let (application, canary) = EditorGenericOpenCanaryHost::new(options)?;
    let mut host = drive_native_editor(
        application,
        Some(NativeEditorCanary::GenericOpen(Box::new(canary))),
    )?;
    let canary = match host.canary.take() {
        Some(NativeEditorCanary::GenericOpen(canary)) => canary,
        Some(
            NativeEditorCanary::GameView(_)
            | NativeEditorCanary::ImageImport(_)
            | NativeEditorCanary::SupervisedPlay(_),
        )
        | None => {
            unreachable!("the guarded launch installs one generic Open/Recovery canary")
        }
    };
    drop(host);
    canary.finish()
}

fn run_image_import_canary(options: EditorLaunchOptions) -> Result<(), EditorRunError> {
    let (application, canary) = EditorImageImportCanaryHost::new(options)?;
    let mut host = drive_native_editor(
        application,
        Some(NativeEditorCanary::ImageImport(Box::new(canary))),
    )?;
    let canary = match host.canary.take() {
        Some(NativeEditorCanary::ImageImport(canary)) => canary,
        Some(
            NativeEditorCanary::GameView(_)
            | NativeEditorCanary::GenericOpen(_)
            | NativeEditorCanary::SupervisedPlay(_),
        )
        | None => {
            unreachable!("the guarded launch installs one image import canary")
        }
    };
    drop(host);
    canary.finish()
}

fn run_supervised_play_canary(options: EditorLaunchOptions) -> Result<(), EditorRunError> {
    let (application, canary) = supervised_play_canary::SupervisedPlayCanaryHost::new(options)?;
    let mut host = drive_native_editor(
        application,
        Some(NativeEditorCanary::SupervisedPlay(Box::new(canary))),
    )?;
    match host.canary.take() {
        Some(NativeEditorCanary::SupervisedPlay(canary)) => {
            let evidence = canary.finish()?;
            if evidence.first_generation() == evidence.successor_generation() {
                return Err(EditorRunError::Platform(
                    "the supervised Play canary reused its immutable generation".to_owned(),
                ));
            }
            println!(
                "{{\"kind\":\"nara_editor_supervised_play_canary\",\"version\":1,\"guarded_project\":true,\"first\":{{\"saved_revision\":{},\"generation\":{},\"output_sequence\":{},\"output_truncated\":{}}},\"failure\":{{\"false_running\":false,\"last_good_generation\":{}}},\"successor\":{{\"saved_revision\":{},\"generation\":{},\"output_sequence\":{},\"output_truncated\":{}}},\"cleanup\":\"complete\",\"result\":\"ok\"}}",
                evidence.first_saved_revision(),
                evidence.first_generation(),
                evidence.first_output_sequence(),
                evidence.first_output_truncated(),
                evidence.last_good_generation(),
                evidence.successor_saved_revision(),
                evidence.successor_generation(),
                evidence.successor_output_sequence(),
                evidence.successor_output_truncated(),
            );
            Ok(())
        }
        Some(
            NativeEditorCanary::GameView(_)
            | NativeEditorCanary::GenericOpen(_)
            | NativeEditorCanary::ImageImport(_),
        )
        | None => unreachable!("the supervised Play canary installs one matching host"),
    }
}

/// Runs one bounded native Game View canary over an already compiled first-party product intent.
#[doc(hidden)]
pub fn run_trusted_compiled_game_view_canary(
    options: EditorLaunchOptions,
    intent: TrustedCompiledEditorProjectIntent,
    request: EditorGameViewCanaryRequest,
) -> Result<EditorGameViewCanaryEvidence, EditorRunError> {
    let application = EditorApplication::open_trusted_compiled(options, intent)?;
    let mut host = drive_native_editor(
        application,
        Some(NativeEditorCanary::GameView(Box::new(
            EditorGameViewCanaryHost::new(request),
        ))),
    )?;
    match host.canary.take() {
        Some(NativeEditorCanary::GameView(canary)) => canary.finish(),
        Some(
            NativeEditorCanary::GenericOpen(_)
            | NativeEditorCanary::ImageImport(_)
            | NativeEditorCanary::SupervisedPlay(_),
        )
        | None => {
            unreachable!("the canary launch installs one Game View canary host")
        }
    }
}

fn drive_native_editor(
    mut application: EditorApplication,
    canary: Option<NativeEditorCanary>,
) -> Result<NativeEditorHost, EditorRunError> {
    let event_loop = EventLoop::<EditorUserEvent>::with_user_event()
        .build()
        .map_err(event_loop_error)?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();
    let supervised_play_wake = SupervisedPlayWake::new(move || {
        proxy
            .send_event(EditorUserEvent::SupervisedPlayWake)
            .is_ok()
    });
    application.install_supervised_play_wake(supervised_play_wake.clone());
    let mut host = NativeEditorHost::new(application, canary, supervised_play_wake)?;
    if let Err(error) = event_loop.run_app(&mut host) {
        host.terminal_error
            .get_or_insert_with(|| event_loop_error(error));
    }
    if let Some(error) = host.terminal_error.take() {
        return Err(error);
    }
    Ok(host)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorUserEvent {
    SupervisedPlayWake,
}

fn event_loop_error(error: EventLoopError) -> EditorRunError {
    EditorRunError::EventLoop(error.to_string())
}

struct NativeEditorHost {
    application: EditorApplication,
    frontend: EguiEditorFrontend,
    supervised_play_wake: SupervisedPlayWake,
    context: egui::Context,
    window: Option<Arc<Window>>,
    egui_state: Option<egui_winit::State>,
    gpu: Option<WgpuEditorExecution>,
    renderer: Option<EditorEguiRenderer>,
    pending_textures: egui::TexturesDelta,
    authoring_slot: EditorViewportSlot,
    game_slot: EditorViewportSlot,
    views: EditorViewInstances,
    scene_presenter: EditorScenePresenter,
    ui_canvas_presenter: EditorUiCanvasPresenter,
    game_presenter: EditorGamePresenter,
    game_input: EditorGameInputRouter,
    game_pointer_target: Option<EguiGameViewOutput>,
    requested_scene_extent: Option<Extent2d>,
    authoring_view_notice: Option<&'static str>,
    game_view_notice: Option<&'static str>,
    gpu_recovery: EditorGpuRecovery,
    play_cadence: EditorPlayCadence,
    editor_frame_index: u64,
    repaint_deadline: Option<Instant>,
    terminal_error: Option<EditorRunError>,
    canary: Option<NativeEditorCanary>,
}

enum NativeEditorCanary {
    GameView(Box<EditorGameViewCanaryHost>),
    GenericOpen(Box<EditorGenericOpenCanaryHost>),
    ImageImport(Box<EditorImageImportCanaryHost>),
    SupervisedPlay(Box<supervised_play_canary::SupervisedPlayCanaryHost>),
}

struct NativeEditorCanaryAdvance {
    action: Option<EditorAction>,
    redraw: bool,
    request_exit: bool,
    exit: bool,
}

impl NativeEditorCanaryAdvance {
    const fn idle() -> Self {
        Self {
            action: None,
            redraw: false,
            request_exit: false,
            exit: false,
        }
    }

    const fn redraw() -> Self {
        Self {
            action: None,
            redraw: true,
            request_exit: false,
            exit: false,
        }
    }

    fn action(action: EditorAction) -> Self {
        Self {
            action: Some(action),
            redraw: true,
            request_exit: false,
            exit: false,
        }
    }

    const fn request_exit() -> Self {
        Self {
            action: None,
            redraw: true,
            request_exit: true,
            exit: false,
        }
    }

    const fn exit() -> Self {
        Self {
            action: None,
            redraw: false,
            request_exit: false,
            exit: true,
        }
    }
}

impl NativeEditorCanary {
    const fn deadline(&self) -> Instant {
        match self {
            Self::GameView(canary) => canary.deadline(),
            Self::GenericOpen(canary) => canary.deadline(),
            Self::ImageImport(canary) => canary.deadline(),
            Self::SupervisedPlay(canary) => canary.deadline(),
        }
    }

    fn tracked_sprite_sort_key(&self) -> Option<i32> {
        match self {
            Self::GameView(canary) => Some(canary.tracked_sprite_sort_key()),
            Self::GenericOpen(_) | Self::ImageImport(_) | Self::SupervisedPlay(_) => None,
        }
    }

    const fn viewport_extent_override(&self) -> Option<Extent2d> {
        match self {
            Self::GameView(canary) => canary.viewport_extent_override(),
            Self::GenericOpen(_) | Self::ImageImport(_) | Self::SupervisedPlay(_) => None,
        }
    }
}

impl NativeEditorHost {
    fn new(
        application: EditorApplication,
        canary: Option<NativeEditorCanary>,
        supervised_play_wake: SupervisedPlayWake,
    ) -> Result<Self, EditorRunError> {
        let capture_ui_image_facts =
            matches!(canary.as_ref(), Some(NativeEditorCanary::ImageImport(_)));
        Ok(Self {
            application,
            frontend: EguiEditorFrontend::new(),
            supervised_play_wake,
            context: egui::Context::default(),
            window: None,
            egui_state: None,
            gpu: None,
            renderer: None,
            pending_textures: egui::TexturesDelta::default(),
            authoring_slot: EditorViewportSlot::default(),
            game_slot: EditorViewportSlot::default(),
            views: EditorViewInstances::default(),
            scene_presenter: EditorScenePresenter::new(),
            ui_canvas_presenter: EditorUiCanvasPresenter::new(capture_ui_image_facts),
            game_presenter: EditorGamePresenter::new(),
            game_input: EditorGameInputRouter::default(),
            game_pointer_target: None,
            requested_scene_extent: None,
            authoring_view_notice: None,
            game_view_notice: None,
            gpu_recovery: EditorGpuRecovery::Ready,
            play_cadence: EditorPlayCadence::default(),
            editor_frame_index: 0,
            repaint_deadline: None,
            terminal_error: None,
            canary,
        })
    }

    fn handle_supervised_play_wake(&mut self, event_loop: &ActiveEventLoop) {
        self.supervised_play_wake.acknowledge();
        let update = self
            .application
            .drive_supervised_play_effects(Instant::now());
        if update.exit_ready() {
            event_loop.exit();
            return;
        }
        if update.actions_observed()
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        self.schedule_next_wake(event_loop);
    }

    fn schedule_next_wake(&self, event_loop: &ActiveEventLoop) {
        if let Some(deadline) = earliest_deadline(
            self.repaint_deadline,
            self.application.next_supervised_play_deadline(),
        ) {
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
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
        if !matches!(
            self.canary.as_ref(),
            Some(NativeEditorCanary::GenericOpen(_) | NativeEditorCanary::SupervisedPlay(_),)
        ) {
            let _ = self.application.native_shell_ready();
        }
        self.gpu_recovery.owner_recreated_for_platform_resume();
        self.play_cadence.retire();
        Ok(())
    }

    fn replace_gpu_execution(&mut self) -> Result<(), EditorRunError> {
        let window =
            self.window.as_ref().cloned().ok_or_else(|| {
                EditorRunError::Platform("Editor window is unavailable".to_owned())
            })?;
        self.retire_gpu_execution();
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
        self.play_cadence.retire();
        window.request_redraw();
        Ok(())
    }

    fn await_explicit_gpu_retry(&mut self, error: &EditorRunError) {
        let _ = self.suspend_game_for_gpu_recovery();
        self.retire_gpu_execution();
        set_requested_scene_extent(
            &mut self.application,
            &mut self.requested_scene_extent,
            None,
        );
        self.gpu_recovery.require_explicit_retry();
        self.authoring_view_notice = Some("Authoring GPU execution is unavailable");
        self.game_view_notice = Some("Game View GPU execution is unavailable");
        if let Some(window) = self.window.as_ref() {
            window.set_title(EDITOR_GPU_UNAVAILABLE_TITLE);
        }
        eprintln!("Nara Editor GPU recovery paused: {error}");
    }

    fn retire_gpu_execution(&mut self) {
        self.renderer = None;
        self.gpu = None;
        self.authoring_slot.retire_gpu_binding();
        self.game_slot.retire_gpu_binding();
        self.scene_presenter.retire_gpu_binding();
        self.ui_canvas_presenter.retire_gpu_binding();
        self.game_presenter.retire_gpu_binding();
        self.pending_textures.clear();
    }

    fn suspend_game_for_gpu_recovery(&mut self) -> Result<(), EditorRunError> {
        if self.application.active_game_start_request().is_none() {
            return Ok(());
        }
        self.application
            .set_game_view_source_active(false, "the Editor GPU owner was retired")
            .map_err(|error| {
                EditorRunError::Platform(format!(
                    "the Game View source could not suspend for GPU recovery: {error}"
                ))
            })
    }

    fn retry_gpu_execution(&mut self) {
        if !self.gpu_recovery.begin_explicit_retry() {
            return;
        }
        if let Err(error) = self.replace_gpu_execution() {
            self.await_explicit_gpu_retry(&error);
        }
    }

    fn handle_gpu_failure(&mut self, event_loop: &ActiveEventLoop, error: EditorRunError) {
        self.game_input.invalidate(true);
        if let Err(input_error) = self.flush_game_input() {
            self.fail(event_loop, input_error);
            return;
        }
        self.play_cadence.retire();
        if !error.requires_gpu_replacement() {
            self.fail(event_loop, error);
            return;
        }
        let mut play_state = match self.application.surface() {
            EditorApplicationSurface::Workspace { frame, .. } => frame.play().state(),
            EditorApplicationSurface::ProjectShell => {
                if self.gpu_recovery.begin_automatic_replacement() {
                    if let Err(replacement_error) = self.replace_gpu_execution() {
                        self.await_explicit_gpu_retry(&replacement_error);
                    }
                } else {
                    self.await_explicit_gpu_retry(&error);
                }
                return;
            }
        };
        if play_state == EditorPlayState::Stepping {
            self.application.cancel_game_step_presentation();
            let frame = self
                .application
                .current_frame()
                .expect("the matched workspace remains published during GPU recovery");
            let batch = EditorActionBatch::new(frame.revision(), []);
            let _ = self.application.submit_and_drive(batch, Duration::ZERO);
            play_state = self
                .application
                .current_frame()
                .expect("the matched workspace remains published during GPU recovery")
                .play()
                .state();
        }
        if matches!(
            play_state,
            EditorPlayState::Running | EditorPlayState::Stepping
        ) {
            if !self.gpu_recovery.begin_runtime_pause() {
                self.await_explicit_gpu_retry(&error);
                return;
            }
            if let Err(suspend_error) = self.suspend_game_for_gpu_recovery() {
                self.fail(event_loop, suspend_error);
                return;
            }
            self.retire_gpu_execution();
            if play_state == EditorPlayState::Running {
                let frame = self
                    .application
                    .current_frame()
                    .expect("the matched workspace remains published while pausing Play");
                let batch = EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::Play(EditorPlayAction::Pause)],
                );
                let _ = self.application.submit_and_drive(batch, Duration::ZERO);
            }
            if let Err(recovery_error) = self.advance_gpu_recovery_pause(event_loop) {
                self.fail(event_loop, recovery_error);
            }
        } else if self.gpu_recovery.begin_automatic_replacement() {
            if let Err(suspend_error) = self.suspend_game_for_gpu_recovery() {
                self.fail(event_loop, suspend_error);
                return;
            }
            if let Err(replacement_error) = self.replace_gpu_execution() {
                self.await_explicit_gpu_retry(&replacement_error);
            }
        } else {
            self.await_explicit_gpu_retry(&error);
        }
    }

    fn advance_gpu_recovery_pause(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), EditorRunError> {
        if self.gpu_recovery != EditorGpuRecovery::AwaitingRuntimePause {
            return Ok(());
        }
        let play_state = match self.application.surface() {
            EditorApplicationSurface::Workspace { frame, .. } => frame.play().state(),
            EditorApplicationSurface::ProjectShell => {
                self.gpu_recovery.finish_runtime_pause();
                if let Err(error) = self.replace_gpu_execution() {
                    self.await_explicit_gpu_retry(&error);
                }
                return Ok(());
            }
        };
        if matches!(
            play_state,
            EditorPlayState::Running | EditorPlayState::Stepping
        ) {
            let frame = self
                .application
                .current_frame()
                .expect("the matched workspace remains published while GPU recovery advances");
            let batch = EditorActionBatch::new(frame.revision(), []);
            let _ = self.application.submit_and_drive(batch, Duration::ZERO);
        }
        let play_state = self
            .application
            .current_frame()
            .expect("the matched workspace remains published while GPU recovery advances")
            .play()
            .state();
        if matches!(
            play_state,
            EditorPlayState::Paused
                | EditorPlayState::Faulted
                | EditorPlayState::CloseIncomplete
                | EditorPlayState::Empty
                | EditorPlayState::RetiringPlay
                | EditorPlayState::RetirementIncomplete
        ) {
            self.gpu_recovery.finish_runtime_pause();
            if let Err(error) = self.replace_gpu_execution() {
                self.await_explicit_gpu_retry(&error);
            }
            return Ok(());
        }
        let deadline = Instant::now() + GAME_VIEW_PENDING_REPAINT_DELAY;
        self.repaint_deadline = Some(deadline);
        self.schedule_next_wake(event_loop);
        Ok(())
    }

    fn handle_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) -> Result<(), EditorRunError> {
        let Some(window) = self.window.as_ref().cloned() else {
            return Ok(());
        };
        if window.id() != window_id {
            return Ok(());
        }

        if self.game_input.is_captured()
            && matches!(
                event,
                WindowEvent::CursorMoved { .. }
                    | WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        ..
                    }
            )
        {
            let position = match &event {
                WindowEvent::CursorMoved { position, .. } => Some(*position),
                _ => self.game_input.cursor_position(),
            };
            let owns_pointer =
                self.game_pointer_target
                    .zip(position)
                    .is_some_and(|(target, position)| {
                        let scale = f64::from(self.context.pixels_per_point());
                        target.owns_pointer(
                            &self.context,
                            egui::pos2((position.x / scale) as f32, (position.y / scale) as f32),
                        )
                    });
            if !owns_pointer {
                self.game_input.release_to_editor();
            }
        }
        let forward_to_egui = if self.application.ordinary_input_locked() {
            self.game_input.invalidate_after_control(true);
            true
        } else {
            self.game_input.handle_window_event(&event)
        };
        let repaint_requested = !forward_to_egui
            || self
                .egui_state
                .as_mut()
                .expect("an installed native window has egui platform state")
                .on_window_event(&window, &event)
                .repaint;
        if repaint_requested {
            window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => {
                self.flush_game_input()?;
                let update = self.application.request_exit();
                if update.exit_ready() {
                    event_loop.exit();
                } else {
                    window.request_redraw();
                }
            }
            WindowEvent::Focused(false) | WindowEvent::ScaleFactorChanged { .. } => {
                self.flush_game_input()?;
                window.request_redraw();
            }
            WindowEvent::Resized(_) => {
                self.flush_game_input()?;
                window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } if explicit_gpu_retry_requested(&event) => {
                self.retry_gpu_execution();
            }
            WindowEvent::RedrawRequested
                if self.gpu_recovery == EditorGpuRecovery::AwaitingRuntimePause =>
            {
                self.advance_gpu_recovery_pause(event_loop)?;
            }
            WindowEvent::RedrawRequested
                if self.gpu_recovery == EditorGpuRecovery::AwaitingExplicitRetry => {}
            WindowEvent::RedrawRequested => self.redraw(event_loop)?,
            WindowEvent::Destroyed => {
                self.flush_game_input()?;
                self.authoring_slot.retire_gpu_binding();
                self.game_slot.retire_gpu_binding();
                self.scene_presenter.retire_gpu_binding();
                self.ui_canvas_presenter.retire_gpu_binding();
                self.game_presenter.retire_gpu_binding();
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

    fn flush_game_input(&mut self) -> Result<(), EditorRunError> {
        if self.application.ordinary_input_locked() {
            self.game_input.invalidate_after_control(true);
            return Ok(());
        }
        let Some(batch) = self.game_input.take_batch() else {
            return Ok(());
        };
        if let Err(error) = self.application.apply_game_view_input_batch(batch) {
            self.game_input.injection_failed();
            return Err(EditorRunError::Platform(format!(
                "the Game View input transaction was rejected: {error}"
            )));
        }
        Ok(())
    }

    fn commit_pending_game_start(
        &mut self,
        window: RenderWindowPacket,
    ) -> Result<bool, EditorRunError> {
        let Some(request) = self.application.pending_game_start_request() else {
            return Ok(false);
        };
        // Starting the Runtime does not depend on whether its panel is currently visible.
        if self.game_slot.view_id().is_none() {
            let prepared = self
                .application
                .prepare_game_start_activation(request)
                .map_err(|error| {
                    EditorRunError::Platform(format!(
                        "the Game Runtime start reservation was rejected: {error}"
                    ))
                })?;
            self.application.commit_game_start_activation(prepared);
            return Ok(true);
        }
        let gpu = self
            .gpu
            .as_mut()
            .expect("an installed native window has one Editor GPU owner");
        let renderer = self
            .renderer
            .as_mut()
            .expect("an installed native window has one egui renderer");
        let prepared = self.game_presenter.prepare_start(
            &mut self.application,
            &self.game_slot,
            gpu,
            renderer,
            window,
            request,
        );
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = self.application.abort_pending_game_start(request);
                return Err(error);
            }
        };
        self.game_presenter.commit_start(
            &mut self.application,
            &mut self.game_slot,
            gpu,
            renderer,
            prepared,
        );
        self.game_view_notice = Some("Game View is starting");
        Ok(true)
    }

    fn reconcile_viewport_source(
        &mut self,
        window: RenderWindowPacket,
        authoring_ui: Option<(
            nara::tooling::EditorDocumentId,
            nara::scene::SceneAuthoringRevision,
        )>,
        has_authoring_scene: bool,
    ) -> Result<bool, EditorRunError> {
        let generation = self.application.workspace_generation();
        self.views
            .synchronize(self.application.panel_workspace_mut(), generation)
            .map_err(|message| EditorRunError::Platform(message.to_owned()))?;
        let authoring_id = self.views.authoring().map(|id| WgpuEditorViewId::new(id.0));
        let game_id = self.views.game().map(|id| WgpuEditorViewId::new(id.0));
        let authoring_replaced = self.authoring_slot.view_id() != authoring_id;
        let game_replaced = self.game_slot.view_id() != game_id;
        let source_matches = authoring_ui.is_some_and(|(document, revision)| {
            self.authoring_slot.is_ui_prefab(document, revision)
        });
        let authoring_source_changed = match authoring_ui {
            Some(_) => self.authoring_slot.has_binding() && !source_matches,
            None => {
                self.authoring_slot.is_ui_prefab_binding()
                    || (!has_authoring_scene && self.authoring_slot.is_scene())
            }
        };
        let mut changed = authoring_replaced || game_replaced || authoring_source_changed;
        if authoring_replaced || authoring_source_changed {
            let gpu = self.gpu.as_mut().expect("live Editor GPU");
            let renderer = self.renderer.as_mut().expect("live egui renderer");
            if authoring_replaced {
                self.authoring_slot.retire_view(gpu, renderer);
            } else {
                self.authoring_slot.retire_target(gpu, renderer);
            }
            self.scene_presenter.retire_presentation();
            self.ui_canvas_presenter.retire_presentation();
            if authoring_replaced {
                self.authoring_slot.set_view_id(authoring_id);
            }
            if authoring_id.is_none() {
                set_requested_scene_extent(
                    &mut self.application,
                    &mut self.requested_scene_extent,
                    None,
                );
            }
        }
        let request = self.application.active_game_start_request();
        if game_replaced || (request.is_none() && self.game_slot.has_binding()) {
            self.game_input.invalidate(false);
            self.flush_game_input()?;
            let gpu = self.gpu.as_mut().expect("live Editor GPU");
            let renderer = self.renderer.as_mut().expect("live egui renderer");
            if game_replaced {
                self.game_slot.retire_view(gpu, renderer);
            } else {
                self.game_slot.retire_target(gpu, renderer);
            }
            self.game_presenter.retire_presentation();
            if game_replaced {
                self.game_slot.set_view_id(game_id);
            }
            changed = true;
        }
        if request.is_some() && game_id.is_none() {
            self.application
                .set_game_view_source_active(false, "the Game view was hidden")
                .map_err(|error| {
                    EditorRunError::Platform(format!(
                        "the hidden Game View source could not retire: {error}"
                    ))
                })?;
        }
        if let Some(request) = request
            && game_id.is_some()
            && (!self.game_slot.is_game() || self.game_slot.game_display_scale_pending())
        {
            self.application
                .set_game_view_source_active(true, "the Game view is visible")
                .map_err(|error| {
                    EditorRunError::Platform(format!(
                        "the Game View source could not activate: {error}"
                    ))
                })?;
            self.game_presenter.ensure_target(
                &mut self.game_slot,
                self.gpu.as_mut().expect("live Editor GPU"),
                self.renderer.as_mut().expect("live egui renderer"),
                window,
                request,
            )?;
            changed = true;
        }
        Ok(changed)
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<(), EditorRunError> {
        let window = Arc::clone(
            self.window
                .as_ref()
                .expect("redraw is emitted only for the installed window"),
        );
        let window_packet = editor_window_packet(&window);
        if window_packet.resolution.physical_width == 0
            || window_packet.resolution.physical_height == 0
        {
            // Keep queued egui input bound to the current product surface until that surface can
            // render and consume it. Advancing an open attempt here could otherwise route a queued
            // Cancel to the successor Workspace or Recovery surface.
            return self.drive_without_presentation(event_loop);
        }
        let raw_input = self
            .egui_state
            .as_mut()
            .expect("an installed native window has egui platform state")
            .take_egui_input(&window);
        match self.application.surface() {
            EditorApplicationSurface::ProjectShell => {
                self.redraw_project_shell(event_loop, window, raw_input, window_packet)
            }
            EditorApplicationSurface::Workspace {
                frame,
                ui_authoring,
                read_only,
            } => self.redraw_workspace(
                event_loop,
                window,
                raw_input,
                window_packet,
                frame,
                ui_authoring,
                read_only,
            ),
        }
    }

    fn redraw_project_shell(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: Arc<Window>,
        raw_input: egui::RawInput,
        window_packet: RenderWindowPacket,
    ) -> Result<(), EditorRunError> {
        self.game_input.invalidate_after_control(true);
        self.game_pointer_target = None;
        self.play_cadence.retire();
        self.views
            .synchronize(self.application.panel_workspace_mut(), None)
            .map_err(|message| EditorRunError::Platform(message.to_owned()))?;
        let gpu = self
            .gpu
            .as_mut()
            .expect("an installed native window has one Editor GPU owner");
        let renderer = self
            .renderer
            .as_mut()
            .expect("an installed native window has one egui renderer");
        self.authoring_slot.retire_view(gpu, renderer);
        self.game_slot.retire_view(gpu, renderer);
        self.scene_presenter.retire_presentation();
        self.ui_canvas_presenter.retire_presentation();
        self.game_presenter.retire_presentation();
        set_requested_scene_extent(
            &mut self.application,
            &mut self.requested_scene_extent,
            None,
        );

        let rendered_state = self.application.state();
        let mut shell_update = EditorApplicationUpdate::idle();
        let output = self.context.run_ui(raw_input, |ui| {
            shell_update = self.frontend.show_project_shell(ui, &mut self.application);
        });
        if shell_update.exit_ready() {
            event_loop.exit();
            return Ok(());
        }
        let open_update = self.application.advance_project_open();

        let repaint_delay = output
            .viewport_output
            .get(&ViewportId::ROOT)
            .map_or(Duration::MAX, |viewport| viewport.repaint_delay);
        self.egui_state
            .as_mut()
            .expect("an installed native window has egui platform state")
            .handle_platform_output_with_event_loop(&window, event_loop, output.platform_output);
        let pixels_per_point = output.pixels_per_point;
        let primitives = self.context.tessellate(output.shapes, pixels_per_point);
        self.pending_textures.append(output.textures_delta);
        let editor_frame_index = self.next_editor_frame_index()?;

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
        gpu.begin_editor_frame(window_packet, editor_frame_index)
            .map_err(platform_gpu_error)?;
        let pending_textures = &mut self.pending_textures;
        let mut freed_textures = Vec::new();
        let presentation = gpu
            .present_window(
                window_packet,
                editor_frame_index,
                &[],
                |frame| {
                    if renderer.surface_format != frame.surface_format() {
                        return Err(WgpuEditorExecutionError::toolkit_paint(
                            "the Editor surface format changed outside the renderer namespace",
                        ));
                    }
                    let extent = frame.surface_extent();
                    let screen = ScreenDescriptor {
                        size_in_pixels: [extent.width, extent.height],
                        pixels_per_point,
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
                            label: Some("nara_editor_project_shell_egui_pass"),
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
                    freed_textures.extend(pending_textures.free.drain());
                    Ok(())
                },
                || window.pre_present_notify(),
            )
            .map_err(platform_gpu_error)?;
        for id in freed_textures {
            renderer.renderer.free_texture(&id);
        }
        if !presentation.composites().is_empty() {
            return Err(EditorRunError::Platform(
                "the Project Shell published a viewport receipt without a candidate".to_owned(),
            ));
        }
        let presentation_deferred = presentation.window().is_none();
        if !presentation_deferred {
            window.set_title(EDITOR_TITLE);
            self.gpu_recovery.mark_presentation_ready();
        }
        let generic_canary_advanced = if presentation_deferred {
            false
        } else if let Some(NativeEditorCanary::GenericOpen(canary)) = self.canary.as_mut() {
            canary.advance_project_shell(rendered_state, &mut self.application, window.id())?
        } else {
            false
        };
        let supervised_play_canary_advanced = if presentation_deferred {
            false
        } else if let Some(NativeEditorCanary::SupervisedPlay(canary)) = self.canary.as_mut() {
            canary.advance_project_shell(rendered_state, &mut self.application, window.id())?
        } else {
            false
        };

        if open_update.actions_observed()
            || shell_update.actions_observed()
            || generic_canary_advanced
            || supervised_play_canary_advanced
            || repaint_delay.is_zero()
        {
            window.request_redraw();
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if let Some(deadline) = earliest_deadline(
            if presentation_deferred {
                Some(Instant::now() + repaint_delay.min(GAME_VIEW_PENDING_REPAINT_DELAY))
            } else {
                (repaint_delay < Duration::MAX).then(|| Instant::now() + repaint_delay)
            },
            self.canary.as_ref().map(NativeEditorCanary::deadline),
        ) {
            self.repaint_deadline = Some(deadline);
            self.schedule_next_wake(event_loop);
        } else {
            self.repaint_deadline = None;
            self.schedule_next_wake(event_loop);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn redraw_workspace(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: Arc<Window>,
        raw_input: egui::RawInput,
        window_packet: RenderWindowPacket,
        editor_frame: Arc<nara::tooling::EditorFrameModel>,
        ui_authoring_frame: Option<Arc<nara::project_host::EditorUiAuthoringFrame>>,
        read_only: bool,
    ) -> Result<(), EditorRunError> {
        let rendered_state = self.application.state();
        if read_only {
            self.game_input.invalidate_after_control(true);
        }
        let authoring_ui = ui_authoring_frame
            .as_ref()
            .map(|frame| (frame.document(), frame.revision()));
        let has_authoring_scene = editor_frame
            .workspace()
            .active_scene
            .as_ref()
            .is_some_and(|document| document.kind == nara::tooling::EditorDocumentKind::Scene);
        if !has_authoring_scene && self.requested_scene_extent.is_some() {
            set_requested_scene_extent(
                &mut self.application,
                &mut self.requested_scene_extent,
                None,
            );
        }
        let ui_authoring_identity = ui_authoring_frame.as_ref().and_then(|frame| {
            frame.canvas_generation().map(|canvas_generation| {
                EditorUiCanvasIdentity::new(
                    frame.document(),
                    frame.revision(),
                    frame.gesture().map(nara::tooling::EditorGestureView::stamp),
                    canvas_generation,
                )
            })
        });
        let editor_frame_index = self.next_editor_frame_index()?;
        self.gpu
            .as_mut()
            .expect("live Editor GPU")
            .begin_editor_frame(window_packet, editor_frame_index)
            .map_err(platform_gpu_error)?;
        let source_changed =
            self.reconcile_viewport_source(window_packet, authoring_ui, has_authoring_scene)?;
        let authoring_visible = self.authoring_slot.view_id().is_some();
        let game_visible = self.game_slot.view_id().is_some();
        let mut pick_stamp = (authoring_visible && has_authoring_scene && authoring_ui.is_none())
            .then(|| {
                self.scene_presenter.pick_stamp(
                    &self.authoring_slot,
                    &editor_frame,
                    self.requested_scene_extent,
                )
            })
            .flatten();
        let mut ui_pick_stamp = authoring_visible
            .then(|| {
                self.ui_canvas_presenter
                    .pick_stamp(&self.authoring_slot, ui_authoring_identity)
            })
            .flatten();
        let mut viewport_retry_pending = false;
        let mut awaiting_game_frame = false;
        let mut game_input_invalidated = false;
        let canary_sprite_sort_key = self
            .canary
            .as_ref()
            .and_then(NativeEditorCanary::tracked_sprite_sort_key);
        let prepared_authoring = match (
            authoring_visible,
            authoring_ui.is_some(),
            has_authoring_scene,
        ) {
            (true, true, _) => {
                if ui_pick_stamp.is_some() {
                    None
                } else {
                    match self.application.take_editor_ui_preview_candidate() {
                        Some(candidate) => match self.ui_canvas_presenter.prepare(
                            &mut self.authoring_slot,
                            self.gpu
                                .as_mut()
                                .expect("an installed native window has one Editor GPU owner"),
                            self.renderer
                                .as_mut()
                                .expect("an installed native window has one egui renderer"),
                            window_packet,
                            candidate,
                        ) {
                            Ok(prepared) => Some(PreparedEditorViewportFrame::UiCanvas(prepared)),
                            Err(EditorRunError::Gpu(error)) if error.is_deferred() => {
                                viewport_retry_pending = true;
                                ui_pick_stamp = None;
                                None
                            }
                            Err(error) => return Err(error),
                        },
                        None => None,
                    }
                }
            }
            (true, false, true) => match self.application.take_scene_preview_candidate() {
                Some(candidate) => match self.scene_presenter.prepare(
                    &mut self.authoring_slot,
                    self.gpu
                        .as_mut()
                        .expect("an installed native window has one Editor GPU owner"),
                    self.renderer
                        .as_mut()
                        .expect("an installed native window has one egui renderer"),
                    window_packet,
                    candidate,
                ) {
                    Ok(prepared) => Some(PreparedEditorViewportFrame::Scene(prepared)),
                    Err(EditorRunError::Gpu(error)) if error.is_deferred() => {
                        viewport_retry_pending = true;
                        pick_stamp = None;
                        None
                    }
                    Err(error) => return Err(error),
                },
                None => None,
            },
            _ => None,
        };
        let prepared_game_frame = if self.game_slot.is_game()
            && matches!(
                editor_frame.play().state(),
                EditorPlayState::Running | EditorPlayState::Paused | EditorPlayState::Stepping
            ) {
            match self.application.take_game_view_candidate() {
                Ok(Some(candidate)) => match self.game_presenter.prepare_candidate(
                    &mut self.application,
                    &self.game_slot,
                    self.gpu
                        .as_mut()
                        .expect("an installed native window has one Editor GPU owner"),
                    window_packet,
                    candidate,
                    canary_sprite_sort_key,
                )? {
                    EditorGamePrepareOutcome::Prepared(prepared) => {
                        Some(PreparedEditorViewportFrame::Game(prepared))
                    }
                    EditorGamePrepareOutcome::Deferred => {
                        viewport_retry_pending = true;
                        game_input_invalidated = true;
                        self.game_view_notice = Some("Game View presentation deferred");
                        None
                    }
                    EditorGamePrepareOutcome::ContentRejected => {
                        awaiting_game_frame = true;
                        game_input_invalidated = true;
                        self.game_view_notice = Some("Game View content was rejected");
                        None
                    }
                },
                Ok(None) => {
                    awaiting_game_frame = true;
                    None
                }
                Err(error) => {
                    return Err(EditorRunError::Platform(format!(
                        "the Game View bridge rejected candidate transfer: {error}"
                    )));
                }
            }
        } else {
            awaiting_game_frame = true;
            None
        };
        let prepared_viewports = [prepared_authoring, prepared_game_frame];
        let prepared_scene = prepared_viewports
            .iter()
            .find_map(|prepared| match prepared {
                Some(PreparedEditorViewportFrame::Scene(scene)) => Some(scene),
                _ => None,
            });
        let prepared_ui = prepared_viewports
            .iter()
            .find_map(|prepared| match prepared {
                Some(PreparedEditorViewportFrame::UiCanvas(ui)) => Some(ui),
                _ => None,
            });
        let prepared_game = prepared_viewports
            .iter()
            .find_map(|prepared| match prepared {
                Some(PreparedEditorViewportFrame::Game(game)) => Some(game),
                _ => None,
            });
        let mut scene_view_input = if authoring_visible && has_authoring_scene {
            self.scene_presenter.scene_view_input(
                &self.authoring_slot,
                &editor_frame,
                self.requested_scene_extent,
                prepared_scene,
                pick_stamp,
                viewport_retry_pending,
            )
        } else {
            EguiSceneViewInput::unavailable()
        };
        let mut ui_canvas_input = if authoring_visible && authoring_ui.is_some() {
            self.ui_canvas_presenter.view_input(
                &self.authoring_slot,
                prepared_ui,
                ui_authoring_identity,
            )
        } else {
            EguiUiCanvasInput::unavailable()
        };
        if authoring_visible && let Some(notice) = self.authoring_view_notice.take() {
            if authoring_ui.is_some() {
                ui_canvas_input = ui_canvas_input.with_notice(notice);
            } else {
                scene_view_input = scene_view_input.with_notice(notice);
            }
        }
        let workspace_modal_active = editor_frame.workspace_intent().phase().is_some();
        let restore_game_focus = self
            .game_input
            .take_focus_restore_request(game_visible && !workspace_modal_active);
        let mut game_view_input = if game_visible {
            self.game_presenter
                .view_input(&self.game_slot, &editor_frame, prepared_game)
                .with_capture_active(self.game_input.is_captured())
                .with_restore_focus(restore_game_focus)
                .with_integer_scaling(
                    self.game_slot.game_display_scale() == nara::render::DisplayScale::IntegerFit,
                )
        } else {
            EguiGameViewInput::unavailable()
        };
        if game_visible && let Some(notice) = self.game_view_notice {
            game_view_input = game_view_input.with_notice(notice);
        }
        let mut panel_output = None;
        let output = self.context.run_ui(raw_input, |ui| {
            panel_output = self.frontend.show_with_viewport(
                ui,
                &mut self.application,
                EguiCentralViewportInput::new(scene_view_input, game_view_input)
                    .with_ui_canvas(ui_canvas_input),
            );
        });
        let panel_output = panel_output
            .expect("a matched workspace surface always returns one Editor panel output");
        let (mut batch, project_action, viewport_output, project_browser_local_changed) =
            panel_output.into_parts();
        let mut desired_scene_extent = None;
        let mut camera_changed = false;
        match viewport_output.authoring {
            Some(EguiAuthoringViewportOutput::Scene(scene_view_output)) => {
                if let Some((point, mapping)) = scene_view_output
                    .pick_point()
                    .zip(scene_view_output.display_mapping())
                {
                    let selection = pick_stamp
                        .ok_or("Scene View input is unavailable")
                        .and_then(|stamp| {
                            self.scene_presenter
                                .resolve_pick(EditorScenePickRequest {
                                    stamp,
                                    point,
                                    mapping,
                                })
                                .map_err(|()| "Scene View input became stale")
                        });
                    match selection {
                        Ok((document, entity)) => {
                            batch = append_editor_action(
                                batch,
                                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                                    document,
                                    entity,
                                }),
                            )
                        }
                        Err(notice) => self.authoring_view_notice = Some(notice),
                    }
                }
                desired_scene_extent = physical_viewport_extent(
                    scene_view_output.desired_size_points(),
                    output.pixels_per_point,
                )
                .filter(|_| has_authoring_scene);
                camera_changed = scene_view_output.camera_changed();
            }
            Some(EguiAuthoringViewportOutput::UiCanvas(canvas_output)) => {
                if let Some(uv) = canvas_output.pick_uv() {
                    let selection = ui_pick_stamp
                        .ok_or("UI Canvas input is unavailable")
                        .and_then(|stamp| {
                            self.ui_canvas_presenter
                                .resolve_pick(EditorUiCanvasPickRequest { stamp, uv })
                                .map(|entity| (stamp.document(), entity))
                                .map_err(|()| "UI Canvas input became stale")
                        });
                    match selection {
                        Ok((document, entity)) => {
                            batch = append_editor_action(
                                batch,
                                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                                    document,
                                    entity,
                                }),
                            )
                        }
                        Err(notice) => self.authoring_view_notice = Some(notice),
                    }
                }
            }
            None => {}
        }
        let mut desired_game_extent = viewport_output.game.and_then(|game| {
            physical_viewport_extent(game.desired_size_points(), output.pixels_per_point)
        });
        let game_content_rect = viewport_output
            .game
            .and_then(|game| game.content_rect_points());
        let game_capture_requested = viewport_output
            .game
            .is_some_and(|game| game.capture_requested());
        let game_focus_released = viewport_output.game.is_some_and(|game| {
            game.primary_pointer_pressed_outside_image()
                || (!game.image_has_focus() && self.context.egui_wants_keyboard_input())
        });
        if let Some(canary_extent) = self
            .canary
            .as_ref()
            .and_then(NativeEditorCanary::viewport_extent_override)
        {
            desired_game_extent = Some(canary_extent);
        }
        let egui_repaint_delay = output
            .viewport_output
            .get(&ViewportId::ROOT)
            .map_or(Duration::MAX, |viewport| viewport.repaint_delay);
        self.egui_state
            .as_mut()
            .expect("an installed native window has egui platform state")
            .handle_platform_output_with_event_loop(&window, event_loop, output.platform_output);
        let primitives = self
            .context
            .tessellate(output.shapes, output.pixels_per_point);
        self.pending_textures.append(output.textures_delta);
        let game_resize_extent = (project_action.is_none()
            && self.game_slot.game_display_scale() == nara::render::DisplayScale::Fit
            && viewport_output.game.is_some())
        .then_some(desired_game_extent)
        .flatten()
        .filter(|extent| {
            self.application
                .active_game_start_request()
                .is_some_and(|request| {
                    let resolution = request.logical_window().resolution;
                    request.resizable()
                        && (resolution.physical_width != extent.width
                            || resolution.physical_height != extent.height)
                })
        });
        if game_resize_extent.is_some() {
            game_input_invalidated = true;
            self.game_input.invalidate(false);
            self.flush_game_input()?;
        }
        let gpu = self
            .gpu
            .as_mut()
            .expect("an installed native window has one Editor GPU owner");
        let renderer = self
            .renderer
            .as_mut()
            .expect("an installed native window has one egui renderer");
        let mut sampled_images = Vec::new();
        for prepared in prepared_viewports.iter().flatten() {
            if primitives_sample_texture(&primitives, prepared.texture_id()) {
                let slot = if matches!(prepared, PreparedEditorViewportFrame::Game(_)) {
                    &self.game_slot
                } else {
                    &self.authoring_slot
                };
                if !slot.matches_prepared(prepared) {
                    return Err(EditorRunError::Platform(
                        "egui sampled an unregistered Editor view texture".to_owned(),
                    ));
                }
                sampled_images.push(prepared.image());
            }
        }
        for slot in [&self.authoring_slot, &self.game_slot] {
            if let Some((texture, image)) = slot.retained_image()
                && !sampled_images
                    .iter()
                    .any(|sample| sample.view_id() == image.view_id())
                && primitives_sample_texture(&primitives, texture)
            {
                sampled_images.push(image);
            }
        }
        let pending_textures = &mut self.pending_textures;
        let mut freed_textures = Vec::new();
        let presentation = gpu.present_window(
            window_packet,
            editor_frame_index,
            &sampled_images,
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
                for image in &sampled_images {
                    frame.record_viewport_sample(*image)?;
                }
                freed_textures.extend(pending_textures.free.drain());
                Ok(())
            },
            || window.pre_present_notify(),
        );
        for id in freed_textures {
            renderer.renderer.free_texture(&id);
        }
        let presentation = match presentation {
            Ok(presentation) => presentation,
            Err(error) => {
                for prepared in prepared_viewports.into_iter().flatten() {
                    if let PreparedEditorViewportFrame::Game(prepared) = prepared {
                        self.game_presenter.fail_prepared(
                            &mut self.application,
                            prepared,
                            &error,
                        )?;
                    }
                }
                return Err(platform_gpu_error(error));
            }
        };
        if presentation.window().is_some() {
            window.set_title(EDITOR_TITLE);
        }
        if !presentation.composites().is_empty() {
            self.gpu_recovery.mark_presentation_ready();
        }
        let mut game_presentation_advanced = false;
        for prepared in prepared_viewports.into_iter().flatten() {
            let image = prepared.image();
            let receipt = presentation
                .composites()
                .iter()
                .find(|receipt| receipt.image() == image)
                .copied();
            match (prepared, receipt) {
                (PreparedEditorViewportFrame::Scene(prepared), Some(receipt)) => {
                    let Some(EguiAuthoringViewportOutput::Scene(scene_output)) =
                        viewport_output.authoring
                    else {
                        return Err(EditorRunError::Platform(
                            "the Scene candidate has no displayed panel".to_owned(),
                        ));
                    };
                    let mapping = scene_output.display_mapping().ok_or_else(|| {
                        EditorRunError::Platform(
                            "the Scene image has no displayed mapping".to_owned(),
                        )
                    })?;
                    self.scene_presenter.promote(prepared, receipt, mapping)?;
                    self.authoring_slot.remember_presented(receipt);
                }
                (PreparedEditorViewportFrame::UiCanvas(prepared), Some(receipt)) => {
                    self.ui_canvas_presenter.promote(prepared, receipt)?;
                    self.authoring_slot.remember_presented(receipt);
                }
                (PreparedEditorViewportFrame::Game(prepared), Some(receipt)) => {
                    self.game_presenter.promote(
                        &mut self.application,
                        &self.game_slot,
                        prepared,
                        receipt,
                    )?;
                    self.game_slot.remember_presented(receipt);
                    game_presentation_advanced = true;
                    self.game_view_notice = None;
                }
                (PreparedEditorViewportFrame::Scene(_), None) => {
                    viewport_retry_pending |= authoring_visible;
                    self.scene_presenter.retire_presentation();
                }
                (PreparedEditorViewportFrame::UiCanvas(_), None) => {
                    viewport_retry_pending |= authoring_visible;
                    self.ui_canvas_presenter.retire_presentation();
                }
                (PreparedEditorViewportFrame::Game(prepared), None) => {
                    viewport_retry_pending |= game_visible;
                    game_input_invalidated = true;
                    self.game_presenter.defer(
                        &mut self.application,
                        prepared,
                        "the Editor window did not sample the submitted Game image".to_owned(),
                    )?;
                    self.game_view_notice = Some("Game View presentation deferred");
                }
            }
        }

        let mut native_canary_advanced = false;
        if presentation.window().is_some() {
            match self.canary.as_mut() {
                Some(NativeEditorCanary::GenericOpen(canary)) => {
                    let advance = canary.advance_workspace(
                        rendered_state,
                        &mut self.application,
                        &mut self.frontend,
                        window.id(),
                    )?;
                    native_canary_advanced = advance.redraw;
                    if let Some(action) = advance.action {
                        batch = append_editor_action(batch, action);
                    }
                    if advance.exit {
                        event_loop.exit();
                        return Ok(());
                    }
                }
                Some(NativeEditorCanary::ImageImport(canary)) => {
                    let observation = self
                        .ui_canvas_presenter
                        .presented_observation(&self.authoring_slot, ui_authoring_identity);
                    let advance = canary.advance_workspace(
                        rendered_state,
                        &mut self.application,
                        &editor_frame,
                        ui_authoring_frame.as_deref(),
                        observation.as_ref(),
                        window.id(),
                        read_only,
                    )?;
                    native_canary_advanced = advance.redraw;
                    if let Some(action) = advance.action {
                        batch = append_editor_action(batch, action);
                    }
                    if advance.exit {
                        event_loop.exit();
                        return Ok(());
                    }
                }
                Some(NativeEditorCanary::SupervisedPlay(canary)) => {
                    let advance = canary.advance_workspace(&mut self.application, window.id())?;
                    native_canary_advanced = advance.redraw;
                    if let Some(action) = advance.action {
                        batch = append_editor_action(batch, action);
                    }
                    if advance.request_exit {
                        let update = self.application.request_exit();
                        if update.exit_ready() {
                            event_loop.exit();
                        } else {
                            window.request_redraw();
                            self.repaint_deadline = None;
                            event_loop.set_control_flow(ControlFlow::Wait);
                        }
                        return Ok(());
                    }
                    if advance.exit {
                        event_loop.exit();
                        return Ok(());
                    }
                }
                Some(NativeEditorCanary::GameView(_)) | None => {}
            }
        }

        self.game_pointer_target = viewport_output.game;
        if viewport_output.game.is_some() {
            if game_input_invalidated {
                self.game_input.invalidate(false);
            } else {
                self.game_input.update_presented(
                    self.game_presenter.input_stamp(),
                    game_content_rect,
                    output.pixels_per_point,
                    game_presentation_advanced,
                );
            }
            if game_focus_released {
                self.game_input.release_to_editor();
            }
            if editor_frame.workspace_intent().phase()
                == Some(nara::tooling::EditorWorkspaceIntentPhase::AwaitingDecision)
            {
                self.game_input.invalidate(false);
            } else if game_capture_requested && !game_focus_released {
                self.game_input.capture_requested();
            }
        } else {
            self.game_input.invalidate(false);
        }

        if project_action.is_none()
            && !read_only
            && matches!(self.canary.as_ref(), Some(NativeEditorCanary::GameView(_)))
        {
            let canary_observation = self.game_presenter.presented_observation();
            let ui_canary_observation = self
                .ui_canvas_presenter
                .presented_observation(&self.authoring_slot, ui_authoring_identity);
            let active_start = self.application.active_game_start_request();
            let Some(NativeEditorCanary::GameView(canary)) = self.canary.as_mut() else {
                unreachable!("the guarded Game View canary remains installed")
            };
            if Instant::now() >= canary.deadline() {
                return Err(canary.timeout_error(canary_observation.as_ref(), active_start));
            }
            let advance = canary.advance(
                EditorGameViewCanaryFrameInput {
                    frame: &editor_frame,
                    observation: canary_observation.as_ref(),
                    active_start,
                    ui_authoring_frame: ui_authoring_frame.as_deref(),
                    ui_observation: ui_canary_observation.as_ref(),
                    ui_presenter: &self.ui_canvas_presenter,
                    scene_receipt: presentation
                        .composites()
                        .iter()
                        .find(|receipt| {
                            Some(receipt.image().view_id()) == self.authoring_slot.view_id()
                        })
                        .copied(),
                    workspace: self.application.panel_workspace_mut(),
                    gpu,
                    game_texture_registered: self.game_slot.has_binding(),
                },
                &mut self.game_input,
                &window,
            )?;
            if Instant::now() >= canary.deadline() {
                return Err(canary.timeout_error(canary_observation.as_ref(), active_start));
            }
            for action in advance.actions {
                batch = append_editor_action(batch, action);
                window.request_redraw();
            }
            if advance.exit {
                event_loop.exit();
                return Ok(());
            }
        }

        if project_action.is_none()
            && !read_only
            && viewport_output.authoring.is_some()
            && desired_scene_extent != self.requested_scene_extent
        {
            if self.authoring_slot.is_scene() {
                self.authoring_slot.retire_target(gpu, renderer);
                self.scene_presenter.retire_presentation();
            }
            set_requested_scene_extent(
                &mut self.application,
                &mut self.requested_scene_extent,
                desired_scene_extent,
            );
            viewport_retry_pending = true;
        }
        let mut game_resized = false;
        let game_scale_changed = if !read_only
            && project_action.is_none()
            && let Some(output) = viewport_output.game
        {
            self.game_slot
                .request_game_display_scale(output.requested_scale())
        } else {
            false
        };
        if game_scale_changed {
            self.game_input.invalidate(false);
            self.game_presenter.retire_presentation();
            viewport_retry_pending = true;
            awaiting_game_frame = true;
            window.request_redraw();
        }
        if !read_only && let Some(extent) = game_resize_extent {
            game_resized = self.application.resize_game_view(extent).map_err(|error| {
                EditorRunError::Platform(format!(
                    "the Game View logical window could not resize: {error}"
                ))
            })?;
            if game_resized {
                self.game_slot.retire_target(gpu, renderer);
                self.game_presenter.retire_presentation();
                self.game_view_notice = Some("Game View is resizing");
                viewport_retry_pending = true;
                awaiting_game_frame = true;
            }
        }

        if let Some(action) = project_action {
            let update = self.application.submit_project_action(action);
            self.game_input.invalidate_after_control(true);
            self.flush_game_input()?;
            self.play_cadence.retire();
            if update.exit_ready() {
                event_loop.exit();
                return Ok(());
            }
            window.request_redraw();
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
            return Ok(());
        }

        // The rendered Workspace (including the replacement-progress overlay) owns this redraw's
        // input. Only after its actions have been applied may the product open attempt publish the
        // next state.
        let open_update = self.application.advance_project_open();
        let now = Instant::now();
        let play_state_before = editor_frame.play().state();
        let image_import_was_pending = editor_frame.image_import_work_pending();
        let dispatch = self.application.dispatch_editor_actions(batch);
        self.frontend.observe_dispatch(&dispatch);
        if dispatch.invalidates_game_input() {
            self.game_input.invalidate_after_control(matches!(
                play_state_before,
                EditorPlayState::Faulted | EditorPlayState::CloseIncomplete
            ));
        }
        self.flush_game_input()?;
        let drive_runtime = dispatch.requires_runtime_drive()
            || game_scale_changed
            || game_resized
            || source_changed
            || self.application.game_step_requires_runtime_drive()
            || play_lifecycle_requires_runtime_drive(play_state_before);
        let delta = self
            .play_cadence
            .real_delta(now, play_state_before, &dispatch, drive_runtime);
        let update = self.application.drive_editor_frame(
            delta,
            dispatch.actions_observed() || open_update.actions_observed(),
            drive_runtime,
        );
        if update.exit_ready() {
            event_loop.exit();
            return Ok(());
        }
        let Some(after_frame) = self.application.current_frame() else {
            self.game_input.invalidate_after_control(true);
            self.play_cadence.retire();
            window.request_redraw();
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
            return Ok(());
        };
        let game_started = self.commit_pending_game_start(window_packet)?;
        let play_state_after_update = after_frame.play().state();
        let image_import_is_pending = after_frame.image_import_work_pending();
        let image_import_completed = image_import_terminal_successor_required(
            image_import_was_pending,
            image_import_is_pending,
        );
        if play_state_after_update == EditorPlayState::Faulted {
            self.application.cancel_game_step_presentation();
            self.game_input.invalidate(true);
            self.flush_game_input()?;
        } else if drive_runtime
            && game_visible
            && matches!(
                play_state_after_update,
                EditorPlayState::Running | EditorPlayState::Paused | EditorPlayState::Stepping
            )
        {
            self.game_input.mark_presented_stale();
        }
        self.play_cadence
            .observe_after(now, play_state_after_update);
        let game_wait_pending = game_view_wait_pending(
            game_visible,
            play_state_after_update,
            viewport_retry_pending,
            awaiting_game_frame,
        );
        let repaint_delay = if game_wait_pending {
            egui_repaint_delay.min(GAME_VIEW_PENDING_REPAINT_DELAY)
        } else {
            egui_repaint_delay
        };
        let cadence_deadline = self.play_cadence.deadline(now, play_state_after_update);
        let wake_deadline = earliest_deadline(
            earliest_deadline(
                earliest_deadline(
                    cadence_deadline,
                    play_lifecycle_poll_deadline(now, play_state_after_update),
                ),
                image_import_poll_deadline(now, image_import_is_pending),
            ),
            earliest_deadline(
                self.canary.as_ref().map(NativeEditorCanary::deadline),
                self.application.session_close_retry_deadline(),
            ),
        );
        let scene_retry_pending = authoring_visible && viewport_retry_pending;
        if open_update.actions_observed()
            || update.actions_observed()
            || scene_retry_pending
            || game_started
            || source_changed
            || camera_changed
            || project_browser_local_changed
            || native_canary_advanced
            || image_import_completed
            || after_frame.project_browser().work_observed()
            || after_frame.project_browser().has_pending_work()
            || repaint_delay.is_zero()
        {
            window.request_redraw();
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if repaint_delay < Duration::MAX || wake_deadline.is_some() {
            let deadline = match (repaint_delay < Duration::MAX, wake_deadline) {
                (true, Some(cadence)) => cadence.min(now + repaint_delay),
                (true, None) => now + repaint_delay,
                (false, Some(cadence)) => cadence,
                (false, None) => unreachable!("the deadline branch requires one wake source"),
            };
            self.repaint_deadline = Some(deadline);
            self.schedule_next_wake(event_loop);
        } else {
            self.repaint_deadline = None;
            self.schedule_next_wake(event_loop);
        }
        Ok(())
    }

    fn next_editor_frame_index(&mut self) -> Result<u64, EditorRunError> {
        self.editor_frame_index = self
            .editor_frame_index
            .checked_add(1)
            .ok_or_else(|| EditorRunError::Platform("Editor frame index exhausted".to_owned()))?;
        Ok(self.editor_frame_index)
    }

    fn drive_without_presentation(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), EditorRunError> {
        let session_close_pending = self.application.session_close_pending();
        let frame = match self.application.surface() {
            EditorApplicationSurface::ProjectShell => {
                self.game_input.invalidate_after_control(true);
                self.play_cadence.retire();
                self.repaint_deadline = None;
                event_loop.set_control_flow(ControlFlow::Wait);
                return Ok(());
            }
            EditorApplicationSurface::Workspace {
                frame,
                read_only: true,
                ..
            } if !session_close_pending
                && !frame.image_import_work_pending()
                && !play_lifecycle_requires_runtime_drive(frame.play().state()) =>
            {
                self.game_input.invalidate_after_control(true);
                self.flush_game_input()?;
                self.play_cadence.retire();
                self.repaint_deadline = None;
                event_loop.set_control_flow(ControlFlow::Wait);
                return Ok(());
            }
            EditorApplicationSurface::Workspace { frame, .. } => frame,
        };

        self.game_input.invalidate(false);
        self.flush_game_input()?;

        let now = Instant::now();
        let state = frame.play().state();
        let image_import_was_pending = frame.image_import_work_pending();
        let drive_runtime = self.application.game_step_requires_runtime_drive()
            || play_lifecycle_requires_runtime_drive(state);
        let dispatch = EditorApplicationDispatch::default();
        let delta = self
            .play_cadence
            .real_delta(now, state, &dispatch, drive_runtime);
        let update = self
            .application
            .drive_editor_frame(delta, false, drive_runtime);
        if update.exit_ready() {
            event_loop.exit();
            return Ok(());
        }

        let Some(after_frame) = self.application.current_frame() else {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            self.play_cadence.retire();
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
            return Ok(());
        };
        let state = after_frame.play().state();
        let image_import_is_pending = after_frame.image_import_work_pending();
        let image_import_completed = image_import_terminal_successor_required(
            image_import_was_pending,
            image_import_is_pending,
        );
        if state == EditorPlayState::Faulted {
            self.game_input.invalidate(true);
            self.flush_game_input()?;
        }
        self.play_cadence.observe_after(now, state);
        let deadline = earliest_deadline(
            earliest_deadline(
                earliest_deadline(
                    self.play_cadence.deadline(now, state),
                    play_lifecycle_poll_deadline(now, state),
                ),
                image_import_poll_deadline(now, image_import_is_pending),
            ),
            earliest_deadline(
                self.canary.as_ref().map(NativeEditorCanary::deadline),
                self.application.session_close_retry_deadline(),
            ),
        );
        if update.actions_observed()
            || image_import_completed
            || after_frame.project_browser().work_observed()
            || after_frame.project_browser().has_pending_work()
        {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if let Some(deadline) = deadline {
            self.repaint_deadline = Some(deadline);
            self.schedule_next_wake(event_loop);
        } else {
            self.repaint_deadline = None;
            self.schedule_next_wake(event_loop);
        }
        Ok(())
    }

    fn expired_canary_error(&self, now: Instant) -> Option<EditorRunError> {
        let canary = self.canary.as_ref()?;
        if now < canary.deadline() {
            return None;
        }
        Some(match canary {
            NativeEditorCanary::GameView(canary) => {
                let observation = self.game_presenter.presented_observation();
                let active_start = self.application.active_game_start_request();
                canary.timeout_error(observation.as_ref(), active_start)
            }
            NativeEditorCanary::GenericOpen(canary) => {
                canary.timeout_error(self.application.state())
            }
            NativeEditorCanary::ImageImport(canary) => {
                canary.timeout_error(self.application.state())
            }
            NativeEditorCanary::SupervisedPlay(canary) => canary.timeout_error(&self.application),
        })
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: EditorRunError) {
        self.terminal_error.get_or_insert(error);
        event_loop.exit();
    }
}

fn earliest_deadline(left: Option<Instant>, right: Option<Instant>) -> Option<Instant> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(deadline), None) | (None, Some(deadline)) => Some(deadline),
        (None, None) => None,
    }
}

fn play_lifecycle_requires_runtime_drive(state: EditorPlayState) -> bool {
    matches!(
        state,
        EditorPlayState::PreparingPlay
            | EditorPlayState::Starting
            | EditorPlayState::RetiringPlay
            | EditorPlayState::Running
            | EditorPlayState::Stopping
    )
}

fn play_lifecycle_poll_deadline(now: Instant, state: EditorPlayState) -> Option<Instant> {
    matches!(
        state,
        EditorPlayState::PreparingPlay
            | EditorPlayState::Starting
            | EditorPlayState::RetiringPlay
            | EditorPlayState::Stopping
    )
    .then_some(now + PLAY_LIFECYCLE_POLL_DELAY)
}

fn image_import_poll_deadline(now: Instant, pending: bool) -> Option<Instant> {
    pending.then_some(now + IMAGE_IMPORT_POLL_DELAY)
}

fn image_import_terminal_successor_required(was_pending: bool, is_pending: bool) -> bool {
    was_pending && !is_pending
}

impl ApplicationHandler<EditorUserEvent> for NativeEditorHost {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        let now = Instant::now();
        if let Some(error) = self.expired_canary_error(now) {
            self.fail(event_loop, error);
            return;
        }
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            let paint_due = self
                .repaint_deadline
                .is_some_and(|deadline| now >= deadline);
            if paint_due {
                self.repaint_deadline = None;
            }
            let mut observed = false;
            if self
                .application
                .next_supervised_play_deadline()
                .is_some_and(|deadline| now >= deadline)
            {
                let update = self.application.drive_supervised_play_effects(now);
                if update.exit_ready() {
                    event_loop.exit();
                    return;
                }
                observed = update.actions_observed();
            }
            if (paint_due || observed)
                && let Some(window) = self.window.as_ref()
            {
                window.request_redraw();
                event_loop.set_control_flow(ControlFlow::Wait);
            } else {
                self.schedule_next_wake(event_loop);
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: EditorUserEvent) {
        match event {
            EditorUserEvent::SupervisedPlayWake => {
                self.handle_supervised_play_wake(event_loop);
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.initialize(event_loop) {
            self.fail(event_loop, error);
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.play_cadence.retire();
        self.game_input.invalidate(true);
        if let Err(error) = self.flush_game_input() {
            self.fail(event_loop, error);
            return;
        }
        self.renderer = None;
        self.gpu = None;
        self.authoring_slot.retire_gpu_binding();
        self.game_slot.retire_gpu_binding();
        self.scene_presenter.retire_gpu_binding();
        self.ui_canvas_presenter.retire_gpu_binding();
        self.game_presenter.retire_gpu_binding();
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
            self.handle_gpu_failure(event_loop, error);
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {}
}

#[derive(Debug, Default)]
struct EditorPlayCadence {
    last_drive: Option<Instant>,
}

impl EditorPlayCadence {
    fn retire(&mut self) {
        self.last_drive = None;
    }

    fn real_delta(
        &mut self,
        now: Instant,
        play_state: EditorPlayState,
        dispatch: &EditorApplicationDispatch,
        drive_runtime: bool,
    ) -> Duration {
        if !drive_runtime {
            return Duration::ZERO;
        }
        if dispatch.retires_cadence()
            || dispatch.starts_cadence()
            || dispatch.requests_step()
            || play_state != EditorPlayState::Running
        {
            self.last_drive = dispatch.starts_cadence().then_some(now);
            return Duration::ZERO;
        }
        let delta = self
            .last_drive
            .map_or(Duration::ZERO, |last| now.saturating_duration_since(last));
        self.last_drive = Some(now);
        delta
    }

    fn observe_after(&mut self, now: Instant, play_state: EditorPlayState) {
        if play_state == EditorPlayState::Running {
            self.last_drive.get_or_insert(now);
        } else {
            self.retire();
        }
    }

    fn deadline(&self, now: Instant, play_state: EditorPlayState) -> Option<Instant> {
        (play_state == EditorPlayState::Running).then_some(now + PLAY_DRIVE_CADENCE)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorGpuRecovery {
    Ready,
    AwaitingRuntimePause,
    AutomaticReplacementAttempted,
    AwaitingExplicitRetry,
}

impl EditorGpuRecovery {
    fn begin_runtime_pause(&mut self) -> bool {
        if *self != Self::Ready {
            return false;
        }
        *self = Self::AwaitingRuntimePause;
        true
    }

    fn finish_runtime_pause(&mut self) {
        debug_assert_eq!(*self, Self::AwaitingRuntimePause);
        *self = Self::AutomaticReplacementAttempted;
    }

    fn begin_automatic_replacement(&mut self) -> bool {
        if *self != Self::Ready {
            return false;
        }
        *self = Self::AutomaticReplacementAttempted;
        true
    }

    fn begin_explicit_retry(&mut self) -> bool {
        if *self != Self::AwaitingExplicitRetry {
            return false;
        }
        *self = Self::AutomaticReplacementAttempted;
        true
    }

    fn require_explicit_retry(&mut self) {
        *self = Self::AwaitingExplicitRetry;
    }

    fn mark_presentation_ready(&mut self) {
        *self = Self::Ready;
    }

    fn owner_recreated_for_platform_resume(&mut self) {
        if *self == Self::AwaitingExplicitRetry {
            *self = Self::AutomaticReplacementAttempted;
        }
    }
}

fn explicit_gpu_retry_requested(event: &KeyEvent) -> bool {
    event.state == ElementState::Pressed
        && !event.repeat
        && event.physical_key == PhysicalKey::Code(KeyCode::F5)
}

fn game_view_wait_pending(
    visible: bool,
    play_state: EditorPlayState,
    retry_pending: bool,
    awaiting_successor: bool,
) -> bool {
    visible
        && matches!(
            play_state,
            EditorPlayState::PreparingPlay
                | EditorPlayState::Starting
                | EditorPlayState::Running
                | EditorPlayState::Paused
                | EditorPlayState::Stepping
        )
        && (retry_pending || awaiting_successor)
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use nara::tooling::EditorPlayState;

    use super::{
        EditorApplicationDispatch, EditorGpuRecovery, EditorPlayCadence, earliest_deadline,
        game_view_wait_pending, image_import_poll_deadline,
        image_import_terminal_successor_required, play_lifecycle_poll_deadline,
        play_lifecycle_requires_runtime_drive,
    };

    #[test]
    fn runtime_lifecycle_drive_is_independent_of_presentation() {
        for state in [
            EditorPlayState::PreparingPlay,
            EditorPlayState::Starting,
            EditorPlayState::RetiringPlay,
            EditorPlayState::Running,
            EditorPlayState::Stopping,
        ] {
            assert!(play_lifecycle_requires_runtime_drive(state));
        }

        assert!(!play_lifecycle_requires_runtime_drive(
            EditorPlayState::Paused
        ));
        assert!(!play_lifecycle_requires_runtime_drive(
            EditorPlayState::RetirementIncomplete
        ));
    }

    #[test]
    fn transitional_runtime_lifecycle_schedules_bounded_follow_up_drive() {
        let now = Instant::now();

        for state in [
            EditorPlayState::PreparingPlay,
            EditorPlayState::Starting,
            EditorPlayState::RetiringPlay,
            EditorPlayState::Stopping,
        ] {
            assert_eq!(
                play_lifecycle_poll_deadline(now, state),
                Some(now + Duration::from_millis(16))
            );
        }

        assert_eq!(
            play_lifecycle_poll_deadline(now, EditorPlayState::Running),
            None
        );
        assert_eq!(
            play_lifecycle_poll_deadline(now, EditorPlayState::CloseIncomplete),
            None
        );
    }

    #[test]
    fn image_import_pending_work_uses_a_bounded_poll_deadline() {
        let now = Instant::now();

        assert_eq!(
            image_import_poll_deadline(now, true),
            Some(now + Duration::from_millis(16))
        );
        assert_eq!(image_import_poll_deadline(now, false), None);
    }

    #[test]
    fn image_import_terminal_transition_requests_one_successor_frame() {
        assert!(image_import_terminal_successor_required(true, false));
        assert!(!image_import_terminal_successor_required(true, true));
        assert!(!image_import_terminal_successor_required(false, false));
        assert!(!image_import_terminal_successor_required(false, true));
    }

    #[test]
    fn earlier_wake_source_wins_over_image_import_polling() {
        let now = Instant::now();
        let earlier = now + Duration::from_millis(4);

        assert_eq!(
            earliest_deadline(Some(earlier), image_import_poll_deadline(now, true)),
            Some(earlier)
        );
    }

    #[test]
    fn gpu_recovery_requires_explicit_retry_after_one_automatic_replacement() {
        let mut recovery = EditorGpuRecovery::Ready;

        assert!(recovery.begin_automatic_replacement());
        assert!(!recovery.begin_automatic_replacement());

        recovery.require_explicit_retry();
        assert!(!recovery.begin_automatic_replacement());
        recovery.owner_recreated_for_platform_resume();
        assert!(!recovery.begin_automatic_replacement());

        recovery.require_explicit_retry();
        assert!(recovery.begin_explicit_retry());
        assert!(!recovery.begin_explicit_retry());

        recovery.mark_presentation_ready();
        assert!(recovery.begin_automatic_replacement());

        recovery.mark_presentation_ready();
        assert!(recovery.begin_runtime_pause());
        assert!(!recovery.begin_automatic_replacement());
        recovery.finish_runtime_pause();
        assert_eq!(recovery, EditorGpuRecovery::AutomaticReplacementAttempted);
    }

    #[test]
    fn paused_content_rejection_keeps_one_bounded_successor_wait() {
        assert!(game_view_wait_pending(
            true,
            EditorPlayState::Paused,
            false,
            true,
        ));
        assert!(!game_view_wait_pending(
            true,
            EditorPlayState::Running,
            false,
            false,
        ));
        assert!(!game_view_wait_pending(
            false,
            EditorPlayState::Paused,
            false,
            true,
        ));
    }

    #[test]
    fn running_cadence_starts_a_fresh_epoch_after_pause() {
        let start = Instant::now();
        let mut cadence = EditorPlayCadence::default();
        cadence.observe_after(start, EditorPlayState::Running);

        assert_eq!(
            cadence.real_delta(
                start + Duration::from_millis(16),
                EditorPlayState::Running,
                &EditorApplicationDispatch::default(),
                true,
            ),
            Duration::from_millis(16)
        );

        cadence.observe_after(start + Duration::from_secs(1), EditorPlayState::Paused);
        assert_eq!(
            cadence.real_delta(
                start + Duration::from_secs(30),
                EditorPlayState::Running,
                &EditorApplicationDispatch::default(),
                true,
            ),
            Duration::ZERO
        );
    }
}
