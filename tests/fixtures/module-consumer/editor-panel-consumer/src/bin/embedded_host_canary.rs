//! Caller-driven public native Host startup, painting and normal-close smoke.

use std::{cell::Cell, path::PathBuf, rc::Rc, sync::Arc};

use nara_editor::{
    EditorApplication, EditorLaunchOptions, EditorNativeComposition, EditorNativeEvent,
    EditorNativeHost, ImguiEditorFrontend, ImguiEditorPanel, ImguiEditorPanelContext,
    workspace::{EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

const MAX_REDRAWS: usize = 240;
const REQUIRED_PAINTED_FRAMES: usize = 3;

struct PaintCanary(Rc<Cell<usize>>);

impl ImguiEditorPanel for PaintCanary {
    fn show(&mut self, ui: &dear_imgui_rs::Ui, _: &mut ImguiEditorPanelContext<'_, '_>) {
        ui.text("Embedded native Host startup smoke");
        self.0.set(self.0.get() + 1);
    }
}

struct EmbeddedHostCanary {
    pending: Option<(EditorApplication, ImguiEditorFrontend)>,
    proxy: EventLoopProxy<EditorNativeEvent>,
    host: Option<EditorNativeHost>,
    window: Option<Arc<Window>>,
    paints: Rc<Cell<usize>>,
    redraws: usize,
    painted_frames: usize,
    close_requested: bool,
}

impl EmbeddedHostCanary {
    fn verify_primary_window(&self) {
        let supplied = self.window.as_ref().expect("the caller must own a window");
        let primary = self
            .host
            .as_ref()
            .expect("the public native Host must exist")
            .primary_window()
            .expect("the public native Host must retain its primary window");
        assert_eq!(
            primary.id(),
            supplied.id(),
            "the caller's window was replaced",
        );
        assert_eq!(
            primary.is_visible(),
            Some(false),
            "the window became visible"
        );
    }
}

impl ApplicationHandler<EditorNativeEvent> for EmbeddedHostCanary {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.host.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("Nara embedded native Host canary")
                            .with_active(false)
                            .with_visible(false)
                            .with_inner_size(LogicalSize::new(1600.0, 1100.0)),
                    )
                    .expect("the caller must create its hidden primary window"),
            );
            let (application, frontend) = self
                .pending
                .take()
                .expect("the complete public composition must be consumed once");
            let proxy = self.proxy.clone();
            self.host = Some(EditorNativeHost::from_application(
                application,
                frontend,
                Some(Arc::clone(&window)),
                move || {
                    proxy
                        .send_event(EditorNativeEvent::SupervisedPlayWake)
                        .is_ok()
                },
            ));
            self.window = Some(window);
        }
        self.host.as_mut().unwrap().resumed(event_loop);
        self.verify_primary_window();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let redraw = matches!(event, WindowEvent::RedrawRequested)
            && self
                .window
                .as_ref()
                .is_some_and(|window| window.id() == window_id);
        let paints_before = self.paints.get();
        let Some(host) = self.host.as_mut() else {
            return;
        };
        host.window_event(event_loop, window_id, event);
        self.verify_primary_window();
        if redraw && !self.close_requested {
            self.redraws += 1;
            if self
                .host
                .as_ref()
                .unwrap()
                .application()
                .current_frame()
                .is_some()
                && self.paints.get() > paints_before
            {
                self.painted_frames += 1;
            }
            if self.painted_frames >= REQUIRED_PAINTED_FRAMES || self.redraws >= MAX_REDRAWS {
                self.close_requested = true;
                eprintln!(
                    "embedded native Host requesting normal close: redraws={}, painted_frames={}, paints={}",
                    self.redraws,
                    self.painted_frames,
                    self.paints.get(),
                );
                self.host.as_mut().unwrap().window_event(
                    event_loop,
                    window_id,
                    WindowEvent::CloseRequested,
                );
                self.verify_primary_window();
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: EditorNativeEvent) {
        if let Some(host) = &mut self.host {
            host.user_event(event_loop, event);
            self.verify_primary_window();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(host) = self.host.as_mut() else {
            return;
        };
        host.about_to_wait(event_loop);
        self.verify_primary_window();
        if !self.close_requested && !event_loop.exiting() {
            let window = self.window.as_ref().unwrap();
            window.request_redraw();
            let window_id = window.id();
            self.window_event(event_loop, window_id, WindowEvent::RedrawRequested);
            if !self.close_requested && !event_loop.exiting() {
                self.proxy
                    .send_event(EditorNativeEvent::SupervisedPlayWake)
                    .expect("the caller's loop must accept its next hidden-window redraw wake");
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let project = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("the embedded native Host canary requires a project root argument")?,
    );
    let paints = Rc::new(Cell::new(0));
    let shared = Rc::clone(&paints);
    let mut composition = EditorNativeComposition::new();
    composition.register_panel(
        EditorPanelDescriptor::new(
            "consumer.embedded-host-canary",
            "Embedded Host smoke",
            EditorPanelMultiplicity::Single,
            EditorPanelPlacement::Floating,
            true,
        ),
        move || PaintCanary(Rc::clone(&shared)),
    )?;
    let application =
        composition.build(EditorLaunchOptions::new(project).with_profile("desktop"))?;
    let event_loop = EventLoop::<EditorNativeEvent>::with_user_event().build()?;
    let mut canary = EmbeddedHostCanary {
        pending: Some(application),
        proxy: event_loop.create_proxy(),
        host: None,
        window: None,
        paints,
        redraws: 0,
        painted_frames: 0,
        close_requested: false,
    };
    let result = event_loop.run_app(&mut canary);
    let Some(host) = canary.host.take() else {
        result?;
        return Err("the event loop returned without constructing the embedded native Host".into());
    };
    let workspace_retired = host.application().current_frame().is_none();
    host.finish(result)?;
    assert!(
        canary.close_requested,
        "the caller must request normal native close",
    );
    assert!(
        workspace_retired,
        "normal close must retire the published workspace",
    );
    assert!(
        canary.painted_frames >= REQUIRED_PAINTED_FRAMES,
        "the fixture must open and paint within {MAX_REDRAWS} redraws: redraws={}, painted_frames={}, paints={}",
        canary.redraws,
        canary.painted_frames,
        canary.paints.get(),
    );
    assert!(canary.redraws <= MAX_REDRAWS);
    assert!(
        canary.paints.get() > 0,
        "the real contributed panel must paint"
    );
    assert_eq!(canary.window.as_ref().unwrap().is_visible(), Some(false));
    println!(
        "NARA_EMBEDDED_NATIVE_HOST_OK redraws={} painted_frames={} paints={}",
        canary.redraws,
        canary.painted_frames,
        canary.paints.get(),
    );
    Ok(())
}
