use std::{
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use egui::ViewportId;
use egui_wgpu::{RendererOptions, WgpuConfiguration, winit::Painter};
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
            Self::Usage(_) | Self::EventLoop(_) | Self::Platform(_) => None,
        }
    }
}

pub fn run(options: EditorLaunchOptions) -> Result<(), EditorRunError> {
    let application = EditorApplication::open(options)?;
    let event_loop = EventLoop::new().map_err(event_loop_error)?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut host = NativeEditorHost::new(application);
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
    painter: Option<Painter>,
    last_frame: Instant,
    repaint_deadline: Option<Instant>,
    terminal_error: Option<EditorRunError>,
}

impl NativeEditorHost {
    fn new(application: EditorApplication) -> Self {
        Self {
            application,
            context: egui::Context::default(),
            window: None,
            egui_state: None,
            painter: None,
            last_frame: Instant::now(),
            repaint_deadline: None,
            terminal_error: None,
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
        let mut painter = match self.painter.take() {
            Some(painter) => painter,
            None => pollster::block_on(Painter::new(
                self.context.clone(),
                WgpuConfiguration::default(),
                false,
                RendererOptions::default(),
            )),
        };
        pollster::block_on(painter.set_window(ViewportId::ROOT, Some(Arc::clone(&window))))
            .map_err(|error| EditorRunError::Platform(error.to_string()))?;
        let egui_state = egui_winit::State::new(
            self.context.clone(),
            ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            painter.max_texture_side(),
        );

        window.request_redraw();
        self.window = Some(window);
        self.egui_state = Some(egui_state);
        self.painter = Some(painter);
        self.last_frame = Instant::now();
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
                if let (Some(width), Some(height)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                {
                    self.painter
                        .as_mut()
                        .expect("an installed native window has an egui painter")
                        .on_window_resized(ViewportId::ROOT, width, height);
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
        let mut batch = None;
        let output = self.context.run_ui(raw_input, |ui| {
            batch = Some(self.application.show(ui));
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
        let mut textures = output.textures_delta;
        self.painter
            .as_mut()
            .expect("an installed native window has an egui painter")
            .paint_and_update_textures(
                ViewportId::ROOT,
                output.pixels_per_point,
                CLEAR_COLOR,
                &primitives,
                &mut textures,
                Vec::new(),
                window,
            );

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
        if let Some(painter) = self.painter.as_mut() {
            if let Err(error) = pollster::block_on(painter.set_window(ViewportId::ROOT, None)) {
                self.terminal_error
                    .get_or_insert_with(|| EditorRunError::Platform(error.to_string()));
                event_loop.exit();
            }
        }
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
            self.fail(event_loop, error);
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(painter) = self.painter.as_mut() {
            painter.destroy();
        }
    }
}
