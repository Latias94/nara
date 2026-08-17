use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use egui::ViewportId;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor, wgpu};
use nara::{
    project_host::EditorProjectIntent,
    render::{Extent2d, RenderWindowPacket},
    render_wgpu::__private::{
        WgpuEditorDeviceScope, WgpuEditorExecution, WgpuEditorExecutionError,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorNavigationAction, EditorPlayAction, EditorPlayState,
    },
    tooling_egui::{
        EguiCentralViewportInput, EguiCentralViewportOutput, EguiEditorViewport,
        EguiEditorViewportIntent, EguiGameViewInput, EguiSceneViewInput,
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

use crate::app::EditorApplicationDispatch;
use crate::{EditorApplication, EditorApplicationOpenError, EditorLaunchOptions};

mod game_input;
mod viewport;

use game_input::EditorGameInputRouter;
use viewport::{
    EditorGamePrepareOutcome, EditorGamePresenter, EditorScenePickRequest, EditorScenePresenter,
    EditorViewportSlot, PreparedEditorViewportFrame, append_editor_action,
    physical_viewport_extent, primitives_sample_texture, set_requested_scene_extent,
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
    run_application(application)
}

/// Runs one private first-party Editor with an already compiled product intent.
#[doc(hidden)]
pub fn run_with_intent(
    options: EditorLaunchOptions,
    intent: EditorProjectIntent,
) -> Result<(), EditorRunError> {
    let application = EditorApplication::open_with_intent(options, intent)?;
    run_application(application)
}

fn run_application(application: EditorApplication) -> Result<(), EditorRunError> {
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
    viewport_slot: EditorViewportSlot,
    scene_presenter: EditorScenePresenter,
    game_presenter: EditorGamePresenter,
    game_input: EditorGameInputRouter,
    viewport_selection: EditorViewportSelection,
    requested_scene_extent: Option<Extent2d>,
    scene_view_notice: Option<&'static str>,
    game_view_notice: Option<&'static str>,
    gpu_recovery: EditorGpuRecovery,
    play_cadence: EditorPlayCadence,
    editor_frame_index: u64,
    repaint_deadline: Option<Instant>,
    terminal_error: Option<EditorRunError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorViewportSelection {
    Scene,
    Game,
    AwaitingPauseForScene,
}

impl EditorViewportSelection {
    const fn selected(self) -> EguiEditorViewport {
        match self {
            Self::Scene => EguiEditorViewport::Scene,
            Self::Game | Self::AwaitingPauseForScene => EguiEditorViewport::Game,
        }
    }

    fn reconcile(&mut self, play_state: EditorPlayState) -> Option<EditorAction> {
        match play_state {
            EditorPlayState::Empty => *self = Self::Scene,
            EditorPlayState::PreparingPlay | EditorPlayState::Starting => {
                if *self == Self::Scene {
                    *self = Self::Game;
                }
            }
            EditorPlayState::Running => {
                if *self == Self::Scene {
                    *self = Self::Game;
                }
                return (*self == Self::AwaitingPauseForScene)
                    .then_some(EditorAction::Play(EditorPlayAction::Pause));
            }
            EditorPlayState::Stepping => {
                if *self == Self::Scene {
                    *self = Self::Game;
                }
            }
            EditorPlayState::Paused
            | EditorPlayState::Faulted
            | EditorPlayState::CloseIncomplete
                if *self == Self::AwaitingPauseForScene =>
            {
                *self = Self::Scene;
            }
            EditorPlayState::Paused
            | EditorPlayState::Faulted
            | EditorPlayState::RetiringPlay
            | EditorPlayState::Stopping
            | EditorPlayState::RetirementIncomplete
            | EditorPlayState::CloseIncomplete => {}
        }
        None
    }

    fn apply_intent(
        &mut self,
        intent: EguiEditorViewportIntent,
        play_state: EditorPlayState,
    ) -> Option<EditorAction> {
        match intent {
            EguiEditorViewportIntent::SelectGame if play_state != EditorPlayState::Empty => {
                *self = Self::Game;
                None
            }
            EguiEditorViewportIntent::SelectScene => match play_state {
                EditorPlayState::Empty
                | EditorPlayState::Paused
                | EditorPlayState::Faulted
                | EditorPlayState::CloseIncomplete => {
                    *self = Self::Scene;
                    None
                }
                EditorPlayState::PreparingPlay | EditorPlayState::Starting => {
                    *self = Self::AwaitingPauseForScene;
                    None
                }
                EditorPlayState::Stepping => {
                    *self = Self::Scene;
                    None
                }
                EditorPlayState::Running => {
                    *self = Self::AwaitingPauseForScene;
                    Some(EditorAction::Play(EditorPlayAction::Pause))
                }
                EditorPlayState::RetiringPlay
                | EditorPlayState::Stopping
                | EditorPlayState::RetirementIncomplete => None,
            },
            EguiEditorViewportIntent::SelectGame => None,
        }
    }
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
            viewport_slot: EditorViewportSlot::new(),
            scene_presenter: EditorScenePresenter::new(),
            game_presenter: EditorGamePresenter::new(),
            game_input: EditorGameInputRouter::default(),
            viewport_selection: EditorViewportSelection::Scene,
            requested_scene_extent: None,
            scene_view_notice: None,
            game_view_notice: None,
            gpu_recovery: EditorGpuRecovery::Ready,
            play_cadence: EditorPlayCadence::default(),
            editor_frame_index: 0,
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
        self.scene_view_notice = Some("Scene View GPU execution is unavailable");
        self.game_view_notice = Some("Game View GPU execution is unavailable");
        if let Some(window) = self.window.as_ref() {
            window.set_title(EDITOR_GPU_UNAVAILABLE_TITLE);
        }
        eprintln!("Nara Editor GPU recovery paused: {error}");
    }

    fn retire_gpu_execution(&mut self) {
        self.renderer = None;
        self.gpu = None;
        self.viewport_slot.retire_gpu_binding();
        self.scene_presenter.retire_gpu_binding();
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
        let mut play_state = self.application.current_frame().play().state();
        if play_state == EditorPlayState::Stepping {
            self.application.cancel_game_step_presentation();
            let frame = self.application.current_frame();
            let batch = EditorActionBatch::new(frame.revision(), []);
            let _ = self.application.submit_and_drive(batch, Duration::ZERO);
            play_state = self.application.current_frame().play().state();
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
                let frame = self.application.current_frame();
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
        let play_state = self.application.current_frame().play().state();
        if matches!(
            play_state,
            EditorPlayState::Running | EditorPlayState::Stepping
        ) {
            let frame = self.application.current_frame();
            let batch = EditorActionBatch::new(frame.revision(), []);
            let _ = self.application.submit_and_drive(batch, Duration::ZERO);
        }
        let play_state = self.application.current_frame().play().state();
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
        event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
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

        let forward_to_egui = self.game_input.handle_window_event(&event);
        if forward_to_egui
            && self
                .egui_state
                .as_mut()
                .expect("an installed native window has egui platform state")
                .on_window_event(&window, &event)
                .repaint
        {
            window.request_redraw();
        } else if !forward_to_egui {
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
                if let (Some(gpu), Some(renderer)) = (self.gpu.as_mut(), self.renderer.as_mut()) {
                    self.viewport_slot.retire_target(gpu, renderer);
                    self.scene_presenter.retire_presentation();
                    self.game_presenter.retire_presentation();
                }
                set_requested_scene_extent(
                    &mut self.application,
                    &mut self.requested_scene_extent,
                    None,
                );
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
                self.viewport_slot.retire_gpu_binding();
                self.scene_presenter.retire_gpu_binding();
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
        let prepared = {
            let gpu = self
                .gpu
                .as_mut()
                .expect("an installed native window has one Editor GPU owner");
            let renderer = self
                .renderer
                .as_mut()
                .expect("an installed native window has one egui renderer");
            self.game_presenter.prepare_start(
                &mut self.application,
                &self.viewport_slot,
                gpu,
                renderer,
                window,
                request,
            )
        };
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = self.application.abort_pending_game_start(request);
                return Err(error);
            }
        };
        let gpu = self
            .gpu
            .as_mut()
            .expect("an installed native window has one Editor GPU owner");
        let renderer = self
            .renderer
            .as_mut()
            .expect("an installed native window has one egui renderer");
        self.game_presenter.commit_start(
            &mut self.application,
            &mut self.viewport_slot,
            gpu,
            renderer,
            &mut self.scene_presenter,
            prepared,
        );
        set_requested_scene_extent(
            &mut self.application,
            &mut self.requested_scene_extent,
            None,
        );
        self.game_view_notice = Some("Game View is starting");
        Ok(true)
    }

    fn reconcile_viewport_source(
        &mut self,
        selected: EguiEditorViewport,
        play_state: EditorPlayState,
        window: RenderWindowPacket,
    ) -> Result<bool, EditorRunError> {
        match selected {
            EguiEditorViewport::Scene if self.viewport_slot.is_game() => {
                if !matches!(
                    play_state,
                    EditorPlayState::Empty
                        | EditorPlayState::Paused
                        | EditorPlayState::Faulted
                        | EditorPlayState::CloseIncomplete
                ) {
                    return Err(EditorRunError::Platform(
                        "Game View cannot hand off to Scene before Play reaches a safe state"
                            .to_owned(),
                    ));
                }
                self.game_input.invalidate(false);
                self.flush_game_input()?;
                self.application.cancel_game_step_presentation();
                self.application
                    .set_game_view_source_active(false, "the Scene viewport became active")
                    .map_err(|error| {
                        EditorRunError::Platform(format!(
                            "the Game View source could not retire before Scene handoff: {error}"
                        ))
                    })?;
                let gpu = self
                    .gpu
                    .as_mut()
                    .expect("an installed native window has one Editor GPU owner");
                let renderer = self
                    .renderer
                    .as_mut()
                    .expect("an installed native window has one egui renderer");
                self.viewport_slot.retire_target(gpu, renderer);
                self.scene_presenter.retire_presentation();
                self.game_presenter.retire_presentation();
                Ok(true)
            }
            EguiEditorViewport::Game if !self.viewport_slot.is_game() => {
                let Some(request) = self.application.active_game_start_request() else {
                    return Ok(false);
                };
                let gpu = self
                    .gpu
                    .as_mut()
                    .expect("an installed native window has one Editor GPU owner");
                let renderer = self
                    .renderer
                    .as_mut()
                    .expect("an installed native window has one egui renderer");
                if self.viewport_slot.has_binding() {
                    self.viewport_slot.retire_target(gpu, renderer);
                    self.scene_presenter.retire_presentation();
                    self.game_presenter.retire_presentation();
                }
                self.application
                    .set_game_view_source_active(true, "the Game viewport became active")
                    .map_err(|error| {
                        EditorRunError::Platform(format!(
                            "the Game View source could not activate: {error}"
                        ))
                    })?;
                self.game_presenter.ensure_target(
                    &mut self.viewport_slot,
                    gpu,
                    renderer,
                    window,
                    request,
                )?;
                Ok(true)
            }
            EguiEditorViewport::Scene | EguiEditorViewport::Game => Ok(false),
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) -> Result<(), EditorRunError> {
        let window = Arc::clone(
            self.window
                .as_ref()
                .expect("redraw is emitted only for the installed window"),
        );
        let raw_input = self
            .egui_state
            .as_mut()
            .expect("an installed native window has egui platform state")
            .take_egui_input(&window);
        let window_packet = editor_window_packet(&window);
        if window_packet.resolution.physical_width == 0
            || window_packet.resolution.physical_height == 0
        {
            return self.drive_without_presentation(event_loop);
        }
        let editor_frame = self.application.current_frame();
        let mut shell_action = self
            .viewport_selection
            .reconcile(editor_frame.play().state());
        let selected_viewport = self.viewport_selection.selected();
        let gpu = self
            .gpu
            .as_ref()
            .expect("an installed native window has one Editor GPU owner");
        let renderer = self
            .renderer
            .as_ref()
            .expect("an installed native window has one egui renderer");
        if !renderer.matches(gpu) {
            return Err(EditorRunError::Platform(
                "the egui renderer does not belong to the live Editor GPU namespace".to_owned(),
            ));
        }
        let source_changed = self.reconcile_viewport_source(
            selected_viewport,
            editor_frame.play().state(),
            window_packet,
        )?;
        let mut pick_stamp = (selected_viewport == EguiEditorViewport::Scene)
            .then(|| {
                self.scene_presenter.pick_stamp(
                    &self.viewport_slot,
                    &editor_frame,
                    self.requested_scene_extent,
                )
            })
            .flatten();
        let mut viewport_retry_pending = false;
        let mut awaiting_game_frame = false;
        let mut game_input_invalidated = false;
        let mut game_notice = None;
        let prepared_viewport = match selected_viewport {
            EguiEditorViewport::Scene => match self.application.take_scene_preview_candidate() {
                Some(candidate) => match self.scene_presenter.prepare(
                    &mut self.viewport_slot,
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
            EguiEditorViewport::Game
                if self.viewport_slot.is_game()
                    && matches!(
                        editor_frame.play().state(),
                        EditorPlayState::Running
                            | EditorPlayState::Paused
                            | EditorPlayState::Stepping
                    ) =>
            {
                match self.application.take_game_view_candidate() {
                    Ok(Some(candidate)) => match self.game_presenter.prepare_candidate(
                        &mut self.application,
                        &self.viewport_slot,
                        self.gpu
                            .as_mut()
                            .expect("an installed native window has one Editor GPU owner"),
                        window_packet,
                        candidate,
                    )? {
                        EditorGamePrepareOutcome::Prepared(prepared) => {
                            Some(PreparedEditorViewportFrame::Game(prepared))
                        }
                        EditorGamePrepareOutcome::Deferred => {
                            viewport_retry_pending = true;
                            game_input_invalidated = true;
                            self.game_view_notice = Some("Game View presentation deferred");
                            game_notice = Some("Game View presentation deferred");
                            None
                        }
                        EditorGamePrepareOutcome::ContentRejected => {
                            awaiting_game_frame = true;
                            game_input_invalidated = true;
                            self.game_view_notice = Some("Game View content was rejected");
                            game_notice = Some("Game View content was rejected");
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
            }
            EguiEditorViewport::Game => {
                awaiting_game_frame = true;
                None
            }
        };
        let prepared_scene = match prepared_viewport.as_ref() {
            Some(PreparedEditorViewportFrame::Scene(scene)) => Some(scene),
            Some(PreparedEditorViewportFrame::Game(_)) | None => None,
        };
        let prepared_game = match prepared_viewport.as_ref() {
            Some(PreparedEditorViewportFrame::Game(game)) => Some(game),
            Some(PreparedEditorViewportFrame::Scene(_)) | None => None,
        };
        let mut scene_view_input = if selected_viewport == EguiEditorViewport::Scene {
            self.scene_presenter.scene_view_input(
                &self.viewport_slot,
                &editor_frame,
                self.requested_scene_extent,
                prepared_scene,
                pick_stamp,
                viewport_retry_pending,
            )
        } else {
            EguiSceneViewInput::unavailable()
        };
        if selected_viewport == EguiEditorViewport::Scene
            && let Some(notice) = self.scene_view_notice.take()
        {
            scene_view_input = scene_view_input.with_notice(notice);
        }
        let workspace_modal_active = editor_frame.workspace_intent().phase()
            == Some(nara::tooling::EditorWorkspaceIntentPhase::AwaitingDecision);
        let restore_game_focus = self.game_input.take_focus_restore_request(
            selected_viewport == EguiEditorViewport::Game && !workspace_modal_active,
        );
        let mut game_view_input = if selected_viewport == EguiEditorViewport::Game {
            self.game_presenter
                .view_input(&self.viewport_slot, &editor_frame, prepared_game)
                .with_capture_active(self.game_input.is_captured())
                .with_restore_focus(restore_game_focus)
        } else {
            EguiGameViewInput::unavailable()
        };
        if selected_viewport == EguiEditorViewport::Game
            && let Some(notice) = game_notice.or(self.game_view_notice)
        {
            game_view_input = game_view_input.with_notice(notice);
        }
        let mut panel_output = None;
        let output = self.context.run_ui(raw_input, |ui| {
            panel_output = Some(self.application.show_with_viewport(
                ui,
                EguiCentralViewportInput::new(scene_view_input, game_view_input),
                selected_viewport,
            ));
        });
        let (mut batch, viewport_output, viewport_intent) = panel_output
            .expect("the Editor panel always returns one output")
            .into_parts();
        if let Some(intent) = viewport_intent {
            if intent == EguiEditorViewportIntent::SelectScene
                && editor_frame.play().state() == EditorPlayState::Stepping
            {
                self.application.cancel_game_step_presentation();
            }
            shell_action = self
                .viewport_selection
                .apply_intent(intent, editor_frame.play().state());
        }
        if let Some(action) = shell_action {
            batch = append_editor_action(batch, action);
        }
        let (
            displayed_viewport,
            desired_scene_extent,
            desired_game_extent,
            game_content_rect,
            game_capture_requested,
            camera_changed,
        ) = match viewport_output {
            EguiCentralViewportOutput::Scene(scene_view_output) => {
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
                            Err(()) => {
                                self.scene_view_notice = Some("Scene View input became stale");
                            }
                        },
                        None => self.scene_view_notice = Some("Scene View input is unavailable"),
                    }
                }
                (
                    EguiEditorViewport::Scene,
                    physical_viewport_extent(
                        scene_view_output.desired_size_points(),
                        output.pixels_per_point,
                    ),
                    None,
                    None,
                    false,
                    scene_view_output.camera_changed(),
                )
            }
            EguiCentralViewportOutput::Game(game_view_output) => (
                EguiEditorViewport::Game,
                None,
                physical_viewport_extent(
                    game_view_output.desired_size_points(),
                    output.pixels_per_point,
                ),
                game_view_output.content_rect_points(),
                game_view_output.capture_requested(),
                false,
            ),
        };
        let selected_after_ui = self.viewport_selection.selected();
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
        self.editor_frame_index = self
            .editor_frame_index
            .checked_add(1)
            .ok_or_else(|| EditorRunError::Platform("Editor frame index exhausted".to_owned()))?;
        let editor_frame_index = self.editor_frame_index;
        let game_resize_extent = (displayed_viewport == EguiEditorViewport::Game
            && selected_after_ui == EguiEditorViewport::Game)
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
        let sampled_viewport = prepared_viewport
            .as_ref()
            .is_some_and(|prepared| primitives_sample_texture(&primitives, prepared.texture_id()));
        let pending_textures = &mut self.pending_textures;
        let mut freed_textures = Vec::new();
        let presentation = gpu.present_window(
            window_packet,
            editor_frame_index,
            prepared_viewport
                .as_ref()
                .map(PreparedEditorViewportFrame::image),
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
                if let Some(prepared) = prepared_viewport.as_ref() {
                    if !sampled_viewport {
                        return Err(WgpuEditorExecutionError::toolkit_paint(
                            "egui did not sample the exact Editor viewport texture",
                        ));
                    }
                    frame.record_viewport_sample(prepared.texture_registration_generation())?;
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
                if let Some(PreparedEditorViewportFrame::Game(prepared)) = prepared_viewport {
                    self.game_presenter
                        .fail_prepared(&mut self.application, prepared, &error)?;
                }
                return Err(platform_gpu_error(error));
            }
        };
        if presentation.window().is_some() {
            window.set_title(EDITOR_TITLE);
        }
        if presentation.composite().is_some() {
            self.gpu_recovery.mark_viewport_presented();
        }
        let mut game_presentation_advanced = false;
        match (prepared_viewport, presentation.composite()) {
            (Some(PreparedEditorViewportFrame::Scene(prepared)), Some(receipt)) => {
                self.scene_presenter.promote(prepared, receipt)?;
            }
            (Some(PreparedEditorViewportFrame::Game(prepared)), Some(receipt)) => {
                self.game_presenter.promote(
                    &mut self.application,
                    &self.viewport_slot,
                    prepared,
                    receipt,
                )?;
                game_presentation_advanced = true;
                self.game_view_notice = None;
            }
            (Some(PreparedEditorViewportFrame::Scene(_)), None)
                if presentation.window().is_none() =>
            {
                viewport_retry_pending = true;
                self.scene_presenter.retire_presentation();
            }
            (Some(PreparedEditorViewportFrame::Game(prepared)), None)
                if presentation.window().is_none() =>
            {
                viewport_retry_pending = true;
                game_input_invalidated = true;
                self.game_presenter.defer(
                    &mut self.application,
                    prepared,
                    "the Editor window did not present the submitted Game image".to_owned(),
                )?;
                self.game_view_notice = Some("Game View presentation deferred");
            }
            (Some(PreparedEditorViewportFrame::Scene(_)), None) => {
                return Err(EditorRunError::Platform(
                    "the Editor window presented without the exact Scene sample".to_owned(),
                ));
            }
            (Some(PreparedEditorViewportFrame::Game(prepared)), None) => {
                let error = WgpuEditorExecutionError::StaleImageSubmission;
                self.game_presenter
                    .fail_prepared(&mut self.application, prepared, &error)?;
                return Err(EditorRunError::Platform(
                    "the Editor window presented without the exact Game sample".to_owned(),
                ));
            }
            (None, Some(_)) => {
                return Err(EditorRunError::Platform(
                    "the Editor GPU published a viewport receipt without a candidate".to_owned(),
                ));
            }
            (None, None)
                if displayed_viewport == EguiEditorViewport::Scene
                    && (viewport_retry_pending || pick_stamp.is_none()) =>
            {
                self.scene_presenter.retire_presentation();
            }
            (None, None) => {}
        }

        if displayed_viewport == EguiEditorViewport::Game
            && selected_after_ui == EguiEditorViewport::Game
        {
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
            if editor_frame.workspace_intent().phase()
                == Some(nara::tooling::EditorWorkspaceIntentPhase::AwaitingDecision)
            {
                self.game_input.invalidate(false);
            } else if game_capture_requested {
                self.game_input.capture_requested();
            }
        } else {
            self.game_input.invalidate(false);
        }

        if displayed_viewport == EguiEditorViewport::Scene
            && desired_scene_extent != self.requested_scene_extent
        {
            if self.viewport_slot.is_scene() {
                self.viewport_slot.retire_target(gpu, renderer);
                self.scene_presenter.retire_presentation();
            }
            set_requested_scene_extent(
                &mut self.application,
                &mut self.requested_scene_extent,
                desired_scene_extent,
            );
            viewport_retry_pending = true;
        } else if displayed_viewport == EguiEditorViewport::Game
            && self.requested_scene_extent.is_some()
        {
            set_requested_scene_extent(
                &mut self.application,
                &mut self.requested_scene_extent,
                None,
            );
        }
        let mut game_resized = false;
        if let Some(extent) = game_resize_extent {
            game_resized = self.application.resize_game_view(extent).map_err(|error| {
                EditorRunError::Platform(format!(
                    "the Game View logical window could not resize: {error}"
                ))
            })?;
            if game_resized {
                self.viewport_slot.retire_target(gpu, renderer);
                self.game_presenter.retire_presentation();
                self.game_view_notice = Some("Game View is resizing");
                viewport_retry_pending = true;
                awaiting_game_frame = true;
            }
        }

        let now = Instant::now();
        let play_state_before = editor_frame.play().state();
        let dispatch = self.application.dispatch_editor_actions(batch);
        if dispatch.invalidates_game_input() {
            self.game_input.invalidate_after_control(matches!(
                play_state_before,
                EditorPlayState::Faulted | EditorPlayState::CloseIncomplete
            ));
        }
        self.flush_game_input()?;
        let drive_runtime = dispatch.requires_runtime_drive()
            || game_resized
            || self.application.game_step_requires_runtime_drive()
            || matches!(
                play_state_before,
                EditorPlayState::PreparingPlay
                    | EditorPlayState::Starting
                    | EditorPlayState::RetiringPlay
                    | EditorPlayState::Running
                    | EditorPlayState::Stopping
            );
        let delta = self
            .play_cadence
            .real_delta(now, play_state_before, dispatch, drive_runtime);
        let update =
            self.application
                .drive_editor_frame(delta, dispatch.actions_observed(), drive_runtime);
        if update.exit_ready() {
            event_loop.exit();
            return Ok(());
        }
        let game_started = self.commit_pending_game_start(window_packet)?;
        let play_state_after_update = self.application.current_frame().play().state();
        if play_state_after_update == EditorPlayState::Faulted {
            self.application.cancel_game_step_presentation();
            self.game_input.invalidate(true);
            self.flush_game_input()?;
        } else if drive_runtime
            && selected_after_ui == EguiEditorViewport::Game
            && matches!(
                play_state_after_update,
                EditorPlayState::Running | EditorPlayState::Paused | EditorPlayState::Stepping
            )
        {
            self.game_input.mark_presented_stale();
        }
        self.play_cadence
            .observe_after(now, play_state_after_update, selected_after_ui);
        let game_wait_pending = game_view_wait_pending(
            selected_after_ui,
            play_state_after_update,
            viewport_retry_pending,
            awaiting_game_frame,
        );
        let repaint_delay = if game_wait_pending {
            egui_repaint_delay.min(GAME_VIEW_PENDING_REPAINT_DELAY)
        } else {
            egui_repaint_delay
        };
        let cadence_deadline =
            self.play_cadence
                .deadline(now, play_state_after_update, selected_after_ui);
        let scene_retry_pending =
            selected_after_ui == EguiEditorViewport::Scene && viewport_retry_pending;
        if update.actions_observed()
            || scene_retry_pending
            || game_started
            || source_changed
            || selected_after_ui != displayed_viewport
            || camera_changed
            || repaint_delay.is_zero()
        {
            window.request_redraw();
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
        } else if repaint_delay < Duration::MAX || cadence_deadline.is_some() {
            let deadline = match (repaint_delay < Duration::MAX, cadence_deadline) {
                (true, Some(cadence)) => cadence.min(now + repaint_delay),
                (true, None) => now + repaint_delay,
                (false, Some(cadence)) => cadence,
                (false, None) => unreachable!("the deadline branch requires one wake source"),
            };
            self.repaint_deadline = Some(deadline);
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        }
        Ok(())
    }

    fn drive_without_presentation(
        &mut self,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), EditorRunError> {
        self.game_input.invalidate(false);
        self.flush_game_input()?;

        let now = Instant::now();
        let selected = self.viewport_selection.selected();
        let state = self.application.current_frame().play().state();
        let drive_runtime =
            state == EditorPlayState::Running && selected == EguiEditorViewport::Game;
        let dispatch = EditorApplicationDispatch::default();
        let delta = self
            .play_cadence
            .real_delta(now, state, dispatch, drive_runtime);
        let update = self
            .application
            .drive_editor_frame(delta, false, drive_runtime);
        if update.exit_ready() {
            event_loop.exit();
            return Ok(());
        }

        let state = self.application.current_frame().play().state();
        if state == EditorPlayState::Faulted {
            self.game_input.invalidate(true);
            self.flush_game_input()?;
        }
        self.play_cadence.observe_after(now, state, selected);
        if let Some(deadline) = self.play_cadence.deadline(now, state, selected) {
            self.repaint_deadline = Some(deadline);
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            self.repaint_deadline = None;
            event_loop.set_control_flow(ControlFlow::Wait);
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
        self.play_cadence.retire();
        self.game_input.invalidate(true);
        if let Err(error) = self.flush_game_input() {
            self.fail(event_loop, error);
            return;
        }
        self.renderer = None;
        self.gpu = None;
        self.viewport_slot.retire_gpu_binding();
        self.scene_presenter.retire_gpu_binding();
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
        dispatch: EditorApplicationDispatch,
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

    fn observe_after(
        &mut self,
        now: Instant,
        play_state: EditorPlayState,
        selected: EguiEditorViewport,
    ) {
        if play_state == EditorPlayState::Running && selected == EguiEditorViewport::Game {
            self.last_drive.get_or_insert(now);
        } else {
            self.retire();
        }
    }

    fn deadline(
        &self,
        now: Instant,
        play_state: EditorPlayState,
        selected: EguiEditorViewport,
    ) -> Option<Instant> {
        (play_state == EditorPlayState::Running && selected == EguiEditorViewport::Game)
            .then_some(now + PLAY_DRIVE_CADENCE)
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

    fn mark_viewport_presented(&mut self) {
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
    selected: EguiEditorViewport,
    play_state: EditorPlayState,
    retry_pending: bool,
    awaiting_successor: bool,
) -> bool {
    selected == EguiEditorViewport::Game
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use nara::{
        tooling::{EditorAction, EditorPlayAction, EditorPlayState},
        tooling_egui::{EguiEditorViewport, EguiEditorViewportIntent},
    };

    use super::{
        EditorApplicationDispatch, EditorGpuRecovery, EditorPlayCadence, EditorViewportSelection,
        game_view_wait_pending,
    };

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

        recovery.mark_viewport_presented();
        assert!(recovery.begin_automatic_replacement());

        recovery.mark_viewport_presented();
        assert!(recovery.begin_runtime_pause());
        assert!(!recovery.begin_automatic_replacement());
        recovery.finish_runtime_pause();
        assert_eq!(recovery, EditorGpuRecovery::AutomaticReplacementAttempted);
    }

    #[test]
    fn scene_tab_handoff_keeps_game_selected_until_pause_is_visible() {
        let mut selection = EditorViewportSelection::Scene;

        assert_eq!(selection.reconcile(EditorPlayState::Starting), None);
        assert_eq!(selection.selected(), EguiEditorViewport::Game);
        assert_eq!(
            selection.apply_intent(
                EguiEditorViewportIntent::SelectScene,
                EditorPlayState::Running,
            ),
            Some(EditorAction::Play(EditorPlayAction::Pause))
        );
        assert_eq!(selection.selected(), EguiEditorViewport::Game);

        assert_eq!(selection.reconcile(EditorPlayState::Paused), None);
        assert_eq!(selection.selected(), EguiEditorViewport::Scene);
    }

    #[test]
    fn selecting_game_cancels_a_synthesized_scene_pause_before_dispatch() {
        let mut selection = EditorViewportSelection::Game;
        assert_eq!(
            selection.apply_intent(
                EguiEditorViewportIntent::SelectScene,
                EditorPlayState::Running,
            ),
            Some(EditorAction::Play(EditorPlayAction::Pause))
        );
        let mut pending = selection.reconcile(EditorPlayState::Running);
        assert_eq!(pending, Some(EditorAction::Play(EditorPlayAction::Pause)));
        pending = selection.apply_intent(
            EguiEditorViewportIntent::SelectGame,
            EditorPlayState::Running,
        );

        assert_eq!(pending, None);
        assert_eq!(selection.selected(), EguiEditorViewport::Game);
    }

    #[test]
    fn paused_content_rejection_keeps_one_bounded_successor_wait() {
        assert!(game_view_wait_pending(
            EguiEditorViewport::Game,
            EditorPlayState::Paused,
            false,
            true,
        ));
        assert!(!game_view_wait_pending(
            EguiEditorViewport::Game,
            EditorPlayState::Running,
            false,
            false,
        ));
        assert!(!game_view_wait_pending(
            EguiEditorViewport::Scene,
            EditorPlayState::Paused,
            false,
            true,
        ));
    }

    #[test]
    fn running_cadence_starts_a_fresh_epoch_after_pause() {
        let start = Instant::now();
        let mut cadence = EditorPlayCadence::default();
        cadence.observe_after(start, EditorPlayState::Running, EguiEditorViewport::Game);

        assert_eq!(
            cadence.real_delta(
                start + Duration::from_millis(16),
                EditorPlayState::Running,
                EditorApplicationDispatch::default(),
                true,
            ),
            Duration::from_millis(16)
        );

        cadence.observe_after(
            start + Duration::from_secs(1),
            EditorPlayState::Paused,
            EguiEditorViewport::Game,
        );
        assert_eq!(
            cadence.real_delta(
                start + Duration::from_secs(30),
                EditorPlayState::Running,
                EditorApplicationDispatch::default(),
                true,
            ),
            Duration::ZERO
        );
    }
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
