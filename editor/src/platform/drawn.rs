use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::render::{DisplayMapping2d, DisplayRect, DisplayScale};
use nara_text::FontFace;
use nara_ui_kernel::{
    UiEnvironment, UiFrame, UiRootKey, UiSize, UiSurfaceContentTransform, UiSurfaceGenerations,
    UiSurfaceId,
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use crate::{
    EditorApplication, EditorApplicationState, EditorLaunchOptions, EditorPanelHost,
    EditorPanelInstanceToken,
    frontend::drawn::{DrawnPanel, input::PanelInput},
    native::{
        EditorClipboard, EditorPanelCapability, EditorPanelImage, EditorPanelImageHost,
        EditorPanelImageStatus, EditorPanelPlatformEvent,
    },
    workspace::{
        EditorPanelDescriptor, EditorPanelInstanceId, EditorPanelMultiplicity,
        EditorPanelPlacement, EditorPanelTarget,
    },
};

use super::EditorRunError;

mod graphics;
mod recovery;
#[cfg(all(test, target_os = "windows", not(feature = "imgui-reference")))]
mod tests;

const CAPABILITIES: [EditorPanelCapability; 8] = [
    EditorPanelCapability::Pointer,
    EditorPanelCapability::Wheel,
    EditorPanelCapability::Keyboard,
    EditorPanelCapability::TextInput,
    EditorPanelCapability::Clipboard,
    EditorPanelCapability::Cursor,
    EditorPanelCapability::Refresh,
    EditorPanelCapability::WindowModal,
];
const SHELL_KIND: &str = "nara.drawn.workspace";
const SHELL_HEIGHT: f32 = 272.0;

struct PanelSurface {
    token: EditorPanelInstanceToken,
    surface: UiSurfaceId,
    bounds: DisplayRect,
    scale: f32,
    specialized: bool,
    input: PanelInput,
}

pub(super) struct DrawnNative {
    window: Arc<Window>,
    application: EditorApplication,
    panels: EditorPanelHost<DrawnPanel>,
    native: EditorPanelImageHost<UiFrame>,
    graphics: Option<graphics::Graphics>,
    surfaces: BTreeMap<EditorPanelInstanceId, PanelSurface>,
    next_surface: u64,
    display_frame: u64,
    fonts: Vec<FontFace>,
    presented: BTreeMap<EditorPanelInstanceId, Arc<UiFrame>>,
    started: Instant,
    closing: bool,
    recovery: Option<recovery::Recovery>,
    cleanup_errors: Vec<String>,
}

impl DrawnNative {
    fn maintenance_deadline(&self, now: Instant) -> Option<Instant> {
        let frame = self.application.current_frame();
        let awaiting_decision = frame.as_ref().is_some_and(|frame| {
            frame.workspace_intent().decision_prompt().is_some()
                || frame.workspace_intent().result().is_some()
        });
        let pending = matches!(
            self.application.state(),
            EditorApplicationState::OpeningInitial | EditorApplicationState::OpeningReplacement
        ) || self.application.project_open_retirement_pending()
            || !awaiting_decision
                && (self.application.has_pending_exit()
                    || frame
                        .as_ref()
                        .is_some_and(|frame| frame.workspace_intent().intent().is_some()));
        super::earliest_deadline(
            super::earliest_deadline(
                pending.then_some(now + Duration::from_millis(16)),
                frame
                    .as_ref()
                    .and_then(|frame| super::editor_frame_maintenance_deadline(frame, now)),
            ),
            self.application.session_close_retry_deadline(),
        )
    }

    fn new(window: Arc<Window>, options: EditorLaunchOptions) -> Result<Self, String> {
        let fonts = font_policy()?;
        let mut application = EditorApplication::new(options);
        let mut panels = EditorPanelHost::new();
        let shell_fonts = fonts.clone();
        panels
            .register_panel(
                &mut application,
                EditorPanelDescriptor::new(
                    SHELL_KIND,
                    "Workspace",
                    EditorPanelMultiplicity::Single,
                    EditorPanelPlacement::Bottom,
                    true,
                ),
                move || {
                    DrawnPanel::shell(
                        UiRootKey::runtime("nara.drawn", "workspace")
                            .expect("the shell root is bounded"),
                        shell_fonts.clone(),
                    )
                    .expect("the admitted font policy constructs shell controls")
                },
            )
            .map_err(|error| error.to_string())?;
        let hierarchy_fonts = fonts.clone();
        panels
            .install_registered_panel(&application, "nara.hierarchy", move || {
                DrawnPanel::hierarchy(
                    UiRootKey::runtime("nara.drawn", "hierarchy")
                        .expect("the hierarchy root is bounded"),
                    hierarchy_fonts.clone(),
                )
            })
            .map_err(|error| error.to_string())?;
        let instances = application.panel_workspace().layout().instances().to_vec();
        for instance in instances {
            if instance.kind() == SHELL_KIND {
                application
                    .panel_workspace_mut()
                    .layout_mut()
                    .set_target(instance.id(), EditorPanelTarget::Workspace);
            } else if instance.kind() != "nara.hierarchy" {
                application
                    .panel_workspace_mut()
                    .layout_mut()
                    .set_visible(instance.id(), false);
            }
        }
        let graphics = graphics::Graphics::new(&window, fonts.clone())?;
        let native = EditorPanelImageHost::new(
            Arc::clone(&window),
            nara::window::WindowId::PRIMARY,
            EditorClipboard::new(),
        );
        application.native_shell_ready();
        Ok(Self {
            window,
            application,
            panels,
            native,
            graphics: Some(graphics),
            surfaces: BTreeMap::new(),
            next_surface: 1,
            display_frame: 0,
            fonts,
            presented: BTreeMap::new(),
            started: Instant::now(),
            closing: false,
            recovery: None,
            cleanup_errors: Vec::new(),
        })
    }

    fn dispatch(&mut self, event: Option<EditorPanelPlatformEvent>) -> Result<bool, String> {
        let ui = self.application.current_ui_authoring_frame();
        let surfaces = &mut self.surfaces;
        let graphics = self.graphics.as_ref();
        let mut error = None;
        let outcome = self
            .native
            .dispatch(
                &self.application,
                &mut self.panels,
                event,
                |panel, context, event, services| {
                    if error.is_some() {
                        return;
                    }
                    let instance = event.endpoint.instance();
                    let Some(surface) = surfaces.get_mut(&instance) else {
                        return;
                    };
                    let Some(journal) = graphics.and_then(|graphics| graphics.journal(instance))
                    else {
                        return;
                    };
                    if let Err(failure) = surface.input.deliver(
                        panel,
                        context,
                        event,
                        services,
                        journal,
                        ui.as_deref(),
                    ) {
                        error = Some(failure);
                    }
                },
            )
            .map_err(|error| error.to_string())?;
        if let Some(error) = error {
            return Err(error);
        }
        if let Some(candidate) = outcome.submission {
            self.panels
                .submit_and_drive(&mut self.application, candidate, Duration::ZERO);
        }
        Ok(outcome.consumed)
    }

    fn begin_graphics_frame(&mut self) -> Result<(), String> {
        self.display_frame = self
            .display_frame
            .checked_add(1)
            .ok_or("Editor display frame identity exhausted")?;
        self.graphics
            .as_mut()
            .ok_or("The graphics Host is closed")?
            .begin(&self.window, self.display_frame)
    }

    fn paint(&mut self) -> Result<bool, String> {
        if self.window.inner_size().width == 0 || self.window.inner_size().height == 0 {
            return Ok(false);
        }
        if self.application.current_frame().is_none() {
            if self.application.has_pending_exit() {
                return Ok(false);
            }
            if matches!(
                self.application.state(),
                EditorApplicationState::Recovery | EditorApplicationState::ProjectManager
            ) {
                let diagnostics = match self.application.project_shell_model() {
                    Some(crate::app::EditorProjectShellModel::Recovery { diagnostics, .. }) => {
                        diagnostics
                            .iter()
                            .map(|diagnostic| {
                                format!("{}: {}", diagnostic.code, diagnostic.summary)
                            })
                            .collect::<Vec<_>>()
                            .join("; ")
                    }
                    _ => "No authoring Session is available".to_owned(),
                };
                return Err(format!(
                    "Could not open the selected source workspace: {diagnostics}. Use the reference Editor for project-manager recovery"
                ));
            }
            if self.started.elapsed() > Duration::from_secs(60) {
                return Err("Opening the source workspace timed out".into());
            }
            return Ok(false);
        }
        let logical = self
            .window
            .inner_size()
            .to_logical::<f32>(self.window.scale_factor());
        if logical.width < 960.0 || logical.height < SHELL_HEIGHT + 96.0 {
            self.native.invalidate();
            self.dispatch(None)?;
            return Ok(false);
        }
        let scale = self.window.scale_factor() as f32;
        let ui = self.application.current_ui_authoring_frame();
        self.dispatch(None)?;
        let shell_instance = self
            .application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|instance| instance.kind() == SHELL_KIND)
            .map(|instance| instance.id());
        let mut withdraw = self.surfaces.values().any(|surface| {
            let expected = if Some(surface.token.instance()) == shell_instance {
                DisplayRect {
                    min: [0.0, 0.0],
                    max: [logical.width, SHELL_HEIGHT],
                }
            } else {
                DisplayRect {
                    min: [0.0, SHELL_HEIGHT],
                    max: [logical.width, logical.height],
                }
            };
            surface.bounds != expected || surface.scale != scale
        });
        drop(self.panels.prepare(&self.application, |panel, _| {
            withdraw |= panel.projection_changes(ui.is_some());
        }));
        if withdraw {
            self.native.invalidate();
            self.dispatch(None)?;
        }
        let mut compiled = Vec::new();
        let mut error = None;
        let surfaces = &mut self.surfaces;
        let next_surface = &mut self.next_surface;
        let graphics = self
            .graphics
            .as_ref()
            .ok_or("The graphics Host is closed")?;
        drop(self.panels.prepare(&self.application, |panel, context| {
            if error.is_some() {
                return;
            }
            let instance = context.model().instance().id();
            let token = context.instance_token();
            let shell = context.model().instance().kind() == SHELL_KIND;
            let bounds = if shell {
                DisplayRect {
                    min: [0.0, 0.0],
                    max: [logical.width, SHELL_HEIGHT],
                }
            } else {
                DisplayRect {
                    min: [0.0, SHELL_HEIGHT],
                    max: [logical.width, logical.height],
                }
            };
            let compatible = surfaces.get(&instance).is_some_and(|surface| {
                surface.token == token
                    && surface.bounds == bounds
                    && surface.scale == scale
                    && surface.specialized == (!shell && ui.is_some())
            });
            if !compatible {
                let Some(surface) = UiSurfaceId::new(*next_surface) else {
                    error = Some("The native Surface identity was exhausted".into());
                    return;
                };
                let Some(next) = next_surface.checked_add(1) else {
                    error = Some("The native Surface identity was exhausted".into());
                    return;
                };
                *next_surface = next;
                surfaces.insert(
                    instance,
                    PanelSurface {
                        token: token.clone(),
                        surface,
                        bounds,
                        scale,
                        specialized: !shell && ui.is_some(),
                        input: PanelInput::default(),
                    },
                );
            }
            let surface = surfaces
                .get(&instance)
                .expect("the current Surface was admitted");
            if let Some(frame) = graphics.pending_frame(instance).filter(|frame| {
                frame.environment().surface_id() == surface.surface && panel.retains_frame(frame)
            }) {
                compiled.push((instance, token, bounds, Arc::clone(frame)));
                return;
            }
            let environment =
                UiSize::new(bounds.max[0] - bounds.min[0], bounds.max[1] - bounds.min[1])
                    .ok_or_else(|| "The panel has no positive logical extent".to_owned())
                    .and_then(|size| {
                        UiSurfaceContentTransform::direct(size, scale)
                            .map_err(|error| error.to_string())
                    })
                    .and_then(|transform| {
                        UiEnvironment::for_surface(
                            surface.surface,
                            transform,
                            UiSurfaceGenerations::new(1, 1, 1, 1)
                                .expect("fixed generations are nonzero"),
                            1,
                            1,
                        )
                        .map_err(|error| error.to_string())
                    });
            match environment.and_then(|environment| {
                panel.compile(context.model().frame(), ui.as_deref(), environment)
            }) {
                Ok(frame) => compiled.push((instance, token, bounds, frame)),
                Err(failure) => error = Some(failure),
            }
        }));
        if let Some(error) = error {
            return Err(error);
        }
        let live = compiled.iter().map(|(id, _, _, _)| *id).collect::<Vec<_>>();
        self.graphics
            .as_mut()
            .ok_or("The graphics Host is closed")?
            .retain(&live)?;
        self.surfaces.retain(|id, _| live.contains(id));
        self.begin_graphics_frame()?;
        let gpu = self
            .graphics
            .as_mut()
            .ok_or("The graphics Host is closed")?;
        let mut images = Vec::new();
        for (instance, token, bounds, frame) in compiled {
            self.native
                .attach(&self.application, token.clone(), CAPABILITIES)
                .map_err(|error| error.to_string())?;
            let (display, frame) = gpu.render(instance, Arc::clone(&frame))?;
            let target = display.output().extent();
            let mapping = DisplayMapping2d::fit(target, bounds, scale, DisplayScale::Fit)
                .ok_or("The panel has no valid native display mapping")?;
            let mut image = EditorPanelImage::new(token, display, frame.clone(), mapping, bounds)
                .with_window_modal(frame.blocks_unhandled_input());
            if Some(instance) == shell_instance {
                if let Some(model) = self.application.current_frame() {
                    if model.workspace_intent().decision_prompt().is_some()
                        || model.workspace_intent().result().is_some()
                    {
                        image = image.with_workspace_decision(&model.workspace_intent());
                    }
                }
            }
            images.push(image);
        }
        let candidate = self
            .native
            .stage(&self.application, gpu.frame, images)
            .map_err(|error| error.to_string())?;
        let outcome = gpu.present(&self.window, &candidate)?;
        let accepted = candidate
            .images()
            .iter()
            .map(|image| (image.token().instance(), Arc::clone(image.snapshot())))
            .collect::<BTreeMap<_, _>>();
        let completions = gpu.complete(&outcome)?;
        if outcome.window().is_some() {
            for (instance, frame) in &accepted {
                let journal = gpu
                    .journal(*instance)
                    .ok_or("The original UI presenter is absent")?;
                let actual = journal
                    .latest(frame.environment().surface_id())
                    .map_err(|error| error.to_string())?;
                if !Arc::ptr_eq(actual.frame(), frame) {
                    self.native.invalidate();
                    return Err(
                        "The original UI presenter did not accept the sampled panel snapshot"
                            .into(),
                    );
                }
            }
        }
        let status = self
            .native
            .presented(&self.application, candidate, &outcome)
            .map_err(|error| error.to_string())?;
        if status == EditorPanelImageStatus::Presented {
            let previous = std::mem::replace(&mut self.presented, accepted);
            let surfaces = &mut self.surfaces;
            let graphics = self
                .graphics
                .as_ref()
                .ok_or("The graphics Host is closed")?;
            let presented = &self.presented;
            let mut error = None;
            drop(self.panels.prepare(&self.application, |panel, context| {
                let id = context.model().instance().id();
                if let (Some(surface), Some(journal), Some(frame)) = (
                    surfaces.get_mut(&id),
                    graphics.journal(id),
                    presented.get(&id),
                ) {
                    if let Err(failure) = surface.input.reconcile(
                        panel,
                        journal,
                        frame,
                        previous.get(&id),
                        context,
                        ui.as_deref(),
                    ) {
                        error.get_or_insert(failure);
                    }
                }
            }));
            if let Some(error) = error {
                return Err(error);
            }
        }
        if !completions.is_empty() {
            self.window.request_redraw();
        }
        self.dispatch(None)?;
        self.refresh_keyboard()?;
        Ok(status == EditorPanelImageStatus::Presented)
    }

    fn refresh_keyboard(&mut self) -> Result<(), String> {
        let ui = self.application.current_ui_authoring_frame();
        let surfaces = &mut self.surfaces;
        let graphics = self
            .graphics
            .as_ref()
            .ok_or("The graphics Host is closed")?;
        let mut error = None;
        let outcome = self
            .native
            .refresh_keyboard_services(
                &self.application,
                &mut self.panels,
                |panel, context, event, services| {
                    let instance = event.endpoint.instance();
                    if let (Some(surface), Some(journal)) =
                        (surfaces.get_mut(&instance), graphics.journal(instance))
                    {
                        if let Err(failure) = surface.input.deliver(
                            panel,
                            context,
                            event,
                            services,
                            journal,
                            ui.as_deref(),
                        ) {
                            error.get_or_insert(failure);
                        }
                    }
                },
            )
            .map_err(|error| error.to_string())?;
        if let Some(error) = error {
            return Err(error);
        }
        if let Some(candidate) = outcome.submission {
            self.panels
                .submit_and_drive(&mut self.application, candidate, Duration::ZERO);
        }
        Ok(())
    }

    fn poll_close(&mut self) -> bool {
        if !self.closing {
            self.closing = true;
            self.native.close();
            if self.graphics.is_some() {
                if let Err(error) = self.dispatch(None) {
                    self.cleanup_errors.push(error);
                }
            }
            self.presented.clear();
            self.surfaces.clear();
        }
        if let Some(graphics) = self.graphics.as_mut() {
            let report = graphics.poll_close();
            self.cleanup_errors.extend(report.errors);
            if !report.completed {
                return false;
            }
        }
        drop(self.graphics.take());
        true
    }
}

fn font_policy() -> Result<Vec<FontFace>, String> {
    let mut fonts = vec![nara_text::bundled_font_face().map_err(|error| error.to_string())?];
    if let Some(path) = std::env::var_os("NARA_EDITOR_CJK_FONT") {
        let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
        if metadata.len() > nara_text::MAX_FONT_CONTAINER_BYTES as u64 {
            return Err("Editor fallback font exceeds the container limit".into());
        }
        fonts.push(
            FontFace::from_bytes(std::fs::read(path).map_err(|error| error.to_string())?, 0)
                .map_err(|error| error.to_string())?,
        );
    }
    Ok(fonts)
}

struct DrawnApplication {
    options: Option<EditorLaunchOptions>,
    native: Option<DrawnNative>,
    failure: Option<String>,
    last_drive: Instant,
}

impl DrawnApplication {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: String) {
        eprintln!("Nara drawn Editor paused: {error}");
        self.failure.get_or_insert(error.clone());
        if let Some(native) = self.native.as_mut() {
            native.pause(error);
        } else {
            event_loop.exit();
        }
    }
}

impl ApplicationHandler for DrawnApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.native.is_some() {
            return;
        }
        let Some(options) = self.options.take() else {
            return;
        };
        let window = event_loop.create_window(
            Window::default_attributes()
                .with_title("Nara Editor — Source workspace")
                .with_inner_size(LogicalSize::new(1200.0, 960.0))
                .with_min_inner_size(LogicalSize::new(960.0, 480.0)),
        );
        match window
            .map(Arc::new)
            .map_err(|error| error.to_string())
            .and_then(|window| DrawnNative::new(window, options))
        {
            Ok(native) => {
                native.window.request_redraw();
                self.native = Some(native);
            }
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(native) = self
            .native
            .as_mut()
            .filter(|native| native.window.id() == window_id)
        else {
            return;
        };
        if native.closing {
            return;
        }
        let result = if matches!(event, WindowEvent::CloseRequested) {
            native.application.request_exit();
            native.window.request_redraw();
            Ok(())
        } else if native.recovery.is_some() {
            native.recovery_event(&event)
        } else if matches!(event, WindowEvent::RedrawRequested) {
            native.paint().map(|_| ())
        } else if let Some(event) = EditorPanelPlatformEvent::from_window_event(&event) {
            native.dispatch(Some(event)).map(|_| {
                native.window.request_redraw();
            })
        } else {
            Ok(())
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(native) = self.native.as_mut() else {
            return;
        };
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last_drive);
        self.last_drive = now;
        let maintenance_before = native.maintenance_deadline(now);
        let update = native
            .application
            .advance(delta.min(Duration::from_millis(50)));
        if update.exit_ready() {
            if native.poll_close() {
                event_loop.exit();
            } else {
                event_loop
                    .set_control_flow(ControlFlow::WaitUntil(now + Duration::from_millis(16)));
            }
            return;
        }
        if native.recovery.is_some() {
            if let Err(error) = native.poll_graphics_retry() {
                eprintln!("Nara drawn Editor graphics retry paused: {error}");
                self.failure.get_or_insert(error);
            }
        }
        if native.recovery.is_some() {
            native.refresh_recovery();
            let deadline = super::earliest_deadline(
                native.maintenance_deadline(now),
                native
                    .graphics_retry_pending()
                    .then_some(now + Duration::from_millis(16)),
            );
            event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
            return;
        }
        if update.actions_observed()
            || maintenance_before.is_some()
            || native.application.current_frame().is_none()
        {
            native.window.request_redraw();
        }
        let presentation_pending = native
            .graphics
            .as_ref()
            .is_some_and(|graphics| graphics.has_pending());
        let deadline = super::earliest_deadline(
            native.maintenance_deadline(now),
            native.native.next_deadline(&native.application),
        );
        if presentation_pending {
            native.window.request_redraw();
        }
        if presentation_pending {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                deadline.map_or(now + Duration::from_millis(16), |deadline| {
                    deadline.min(now + Duration::from_millis(16))
                }),
            ));
        } else if let Some(deadline) = deadline {
            native.native.poll_deadlines(&native.application, now);
            event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

/// Runs the independent drawn source workspace for an explicit project root.
///
/// Select `default-features = false` to exclude the optional Dear reference. This composition
/// provides source opening, document tabs, Hierarchy editing, history and persistence; the
/// remaining Editor panels retain their separate reference composition.
pub fn run_drawn(options: EditorLaunchOptions) -> Result<(), EditorRunError> {
    if options.project_root().is_none() {
        return Err(EditorRunError::usage(
            "nara-editor-drawn requires a project root",
        ));
    }
    let event_loop =
        EventLoop::new().map_err(|error| EditorRunError::EventLoop(error.to_string()))?;
    let mut application = DrawnApplication {
        options: Some(options),
        native: None,
        failure: None,
        last_drive: Instant::now(),
    };
    let event_error = event_loop
        .run_app(&mut application)
        .err()
        .map(|error| EditorRunError::EventLoop(error.to_string()));
    let mut primary = application
        .failure
        .take()
        .map(EditorRunError::Platform)
        .or(event_error);
    if let Some(native) = application.native.as_mut() {
        while !native.poll_close() {
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut exit_ready = native.application.request_exit().exit_ready();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !exit_ready && Instant::now() < deadline {
            if native
                .application
                .current_frame()
                .is_some_and(|frame| frame.workspace_intent().decision_prompt().is_some())
            {
                break;
            }
            exit_ready = native.application.advance(Duration::ZERO).exit_ready();
            if !exit_ready {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        if !exit_ready {
            primary.get_or_insert_with(|| EditorRunError::Platform(
                "The native event loop ended before ordinary document decisions and Session retirement completed".into()));
        }
        let tasks = super::termination::finish_open_host_shutdown(
            &mut native.application,
            Instant::now() + Duration::from_secs(5),
        );
        let cleanup = (!native.cleanup_errors.is_empty()).then(|| native.cleanup_errors.join("; "));
        return super::termination::editor_run_result(
            primary,
            cleanup.as_deref(),
            native.application.failed_open_close_evidence(),
            native.application.failed_session_close_receipt(),
            Some(&tasks),
        );
    }
    primary.map_or(Ok(()), Err)
}
