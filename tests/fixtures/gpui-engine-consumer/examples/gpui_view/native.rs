#[path = "content.rs"]
mod content;
#[path = "engine.rs"]
mod engine;
#[path = "panels.rs"]
mod panels;
#[path = "renderer.rs"]
mod renderer;
#[path = "smoke.rs"]
mod smoke;
#[path = "text_input.rs"]
mod text_input;

use anyhow::{Context as _, Result, ensure};
use engine::{Engine, Preview};
use nara_engine::{scene::SceneEntityId, tooling::EditorDocumentQueryBasis};
use nara_render::{DisplayMapping2d, DisplayRect, DisplayScale};
use nara_render_wgpu::WgpuImageDisplay;
use open_gpui::{
    Application, ApplicationHandle, Bounds, Context, Corners, DevicePixels, Entity, Focusable,
    HostedSurface, HostedSurfaceGeometry, HostedSurfaceOptions, ImageTexture, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, PlatformInput, PrepaintPublicationId, Render, RequestFrameOptions, StyleRefinement,
    WindowHandle, canvas, div, point, prelude::*, px, rgb, rgba, size,
};
use open_gpui_wgpu::CosmicTextSystem;
use open_gpui_windows::{
    WindowsPlatform, WindowsRenderer, WindowsRendererFactory, WindowsRendererWindow,
};
use panels::{EditorModel, Hierarchy, Inspector, PanelWork};
use renderer::{Publication, Renderer, Snapshot};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use text_input::DraftInput;
use windows::Win32::UI::WindowsAndMessaging::MSG;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, MouseButton as NativeButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key, NamedKey},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};

enum Command {
    Move,
    Undo,
    Redo,
    Pick([f32; 2]),
    Select {
        basis: EditorDocumentQueryBasis,
        entity: SceneEntityId,
    },
    SetX {
        basis: EditorDocumentQueryBasis,
        entity: SceneEntityId,
        value: f64,
    },
}

struct SceneView {
    preview: Rc<Preview>,
    image: ImageTexture,
    commands: Rc<RefCell<VecDeque<Command>>>,
    candidate: Rc<RefCell<Option<Rc<Snapshot>>>>,
    publications: Rc<RefCell<VecDeque<Publication>>>,
    publication: PrepaintPublicationId,
    clicks: Rc<Cell<u64>>,
    content_images: Vec<(WgpuImageDisplay, ImageTexture)>,
    candidate_content: Rc<RefCell<Vec<WgpuImageDisplay>>>,
}

impl Render for SceneView {
    fn render(&mut self, _: &mut open_gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        self.candidate.borrow_mut().take();
        self.candidate_content.borrow_mut().clear();
        let commands = self.commands.clone();
        let buttons = [
            ("Move sprite", Command::Move),
            ("Undo", Command::Undo),
            ("Redo", Command::Redo),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (label, command))| {
            let commands = commands.clone();
            let click = match command {
                Command::Move => 0,
                Command::Undo => 1,
                _ => 2,
            };
            div()
                .id(("scene-command", index))
                .w(px(160.))
                .h(px(38.))
                .px_3()
                .py_2()
                .rounded_md()
                .bg(rgb(0x315878))
                .cursor_pointer()
                .on_click(move |_, _, _| {
                    commands.borrow_mut().push_back(match click {
                        0 => Command::Move,
                        1 => Command::Undo,
                        _ => Command::Redo,
                    });
                })
                .child(label)
        });
        let candidate = self.candidate.clone();
        let publications = self.publications.clone();
        let publication = self.publication;
        let preview = self.preview.clone();
        let image = self.image.clone();
        let clicks = self.clicks.clone();
        let pick_commands = self.commands.clone();
        let content_images =
            self.content_images
                .iter()
                .enumerate()
                .map(|(index, (display, image))| {
                    let display = display.clone();
                    let image = image.clone();
                    let samples = self.candidate_content.clone();
                    let top = 150. + index as f32 * 140.;
                    div()
                .absolute()
                .left(px(700.))
                .top(px(top - 28.))
                .child(if index == 0 { "Source PNG" } else { "Cooked PNG" })
                .child(
                    div()
                        .absolute()
                        .top(px(28.))
                        .w(px(176.))
                        .h(px(64.))
                        .rounded_lg()
                        .overflow_hidden()
                        .bg(rgb(0x404040))
                        .child(canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                if bounds.intersects(&window.clip_bounds()) {
                                    window.paint_image_texture(bounds, Corners::all(px(0.)), &image)
                                        .expect("retained content stays in its original atlas");
                                    samples.borrow_mut().push(display.clone());
                                }
                            },
                        ).size_full())
                        .child(div().absolute().left(px(44.)).top(px(8.)).w(px(88.)).h(px(16.))
                            .bg(rgba(0x2080c080))),
                )
                })
                .collect::<Vec<_>>();
        div()
            .size_full()
            .relative()
            .bg(rgb(0x17212e))
            .font_family("Segoe UI")
            .text_size(px(16.))
            .text_color(rgb(0xf0f3f8))
            .child(
                div()
                    .absolute()
                    .left(px(24.))
                    .top(px(20.))
                    .flex()
                    .gap_3()
                    .children(buttons),
            )
            .child(
                div()
                    .absolute()
                    .left(px(24.))
                    .top(px(72.))
                    .child("Nara Scene View · original engine device and submission"),
            )
            .child(
                div()
                    .id("engine-scene-view")
                    .absolute()
                    .left(px(24.))
                    .top(px(106.))
                    .w(px(640.))
                    .h(px(360.))
                    .overflow_hidden()
                    .rounded_lg()
                    .on_mouse_down(MouseButton::Left, move |event, _, _| {
                        pick_commands.borrow_mut().push_back(Command::Pick([
                            f32::from(event.window_event().position.x),
                            f32::from(event.window_event().position.y),
                        ]));
                    })
                    .child(
                        canvas(
                            move |bounds, window, _| {
                                let panel = DisplayRect {
                                    min: [f32::from(bounds.origin.x), f32::from(bounds.origin.y)],
                                    max: [
                                        f32::from(bounds.origin.x + bounds.size.width),
                                        f32::from(bounds.origin.y + bounds.size.height),
                                    ],
                                };
                                let mapping = DisplayMapping2d::fit(
                                    preview.display.output().extent(),
                                    panel,
                                    window.scale_factor(),
                                    DisplayScale::Fit,
                                )
                                .expect("visible Scene View has finite geometry");
                                let snapshot = Rc::new(Snapshot { preview, mapping });
                                let accepted = snapshot.clone();
                                window.record_prepaint_window_transaction(
                                    publication,
                                    move |fence, _, _| {
                                        publications.borrow_mut().push_back(Publication {
                                            generation: fence.generation(),
                                            snapshot: accepted.clone(),
                                        });
                                    },
                                    |_, _, _| {},
                                );
                                snapshot
                            },
                            move |_, snapshot, window, _| {
                                let content = snapshot.mapping.content_rect();
                                let bounds = Bounds::new(
                                    point(px(content.min[0]), px(content.min[1])),
                                    size(
                                        px(content.max[0] - content.min[0]),
                                        px(content.max[1] - content.min[1]),
                                    ),
                                );
                                window
                                    .paint_image_texture(bounds, Corners::all(px(0.)), &image)
                                    .expect("engine output remains in its original atlas");
                                *candidate.borrow_mut() = Some(snapshot);
                            },
                        )
                        .size_full(),
                    )
                    .child(
                        div()
                            .id("scene-overlay")
                            .absolute()
                            .left(px(8.))
                            .top(px(8.))
                            .w(px(150.))
                            .h(px(34.))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x7d3650))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
                            .child("Overlay control"),
                    ),
            )
            .child(div().absolute().left(px(24.)).top(px(550.)).child(
                "Edit the original Scene, Undo/Redo, select its sprite, or type a local UI draft.",
            ))
            .children(content_images)
    }
}

struct Root {
    scene_view: Entity<SceneView>,
    draft: Entity<DraftInput>,
    hierarchy: Entity<Hierarchy>,
    inspectors: [Entity<Inspector>; 2],
}

impl Render for Root {
    fn render(&mut self, _: &mut open_gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .child(
                self.scene_view
                    .clone()
                    .cached(StyleRefinement::default().w(px(1280.)).h(px(640.))),
            )
            .child(
                div()
                    .absolute()
                    .left(px(24.))
                    .top(px(488.))
                    .w(px(640.))
                    .h(px(44.))
                    .child(self.draft.clone()),
            )
            .child(
                div().absolute().left(px(920.)).top(px(20.)).child(
                    self.hierarchy
                        .clone()
                        .cached(StyleRefinement::default().w(px(320.)).h(px(130.))),
                ),
            )
            .children(
                self.inspectors
                    .iter()
                    .enumerate()
                    .map(|(index, inspector)| {
                        div()
                            .absolute()
                            .left(px(920.))
                            .top(px(170. + index as f32 * 190.))
                            .child(
                                inspector
                                    .clone()
                                    .cached(StyleRefinement::default().w(px(320.)).h(px(176.))),
                            )
                    }),
            )
    }
}

#[derive(Default)]
struct NativeFactory {
    creates: Cell<u64>,
}

impl WindowsRendererFactory for NativeFactory {
    fn create(&self, _: WindowsRendererWindow) -> Result<Box<dyn WindowsRenderer>> {
        self.creates.set(self.creates.get() + 1);
        anyhow::bail!("this consumer uses only host-driven logical GPUI roots")
    }
}

#[derive(Clone, Copy, Debug)]
struct Wake;

struct Viewer {
    engine: Option<Rc<RefCell<Engine>>>,
    renderer: Option<Rc<Renderer>>,
    surface: Option<HostedSurface>,
    root: Option<WindowHandle<Root>>,
    draft: Option<Entity<DraftInput>>,
    model: Option<Entity<EditorModel>>,
    panel_work: Rc<PanelWork>,
    panel_refusals: u64,
    application: Option<ApplicationHandle>,
    platform: Rc<WindowsPlatform>,
    factory: Rc<NativeFactory>,
    proxy: EventLoopProxy<Wake>,
    commands: Rc<RefCell<VecDeque<Command>>>,
    clicks: Rc<Cell<u64>>,
    picks: u64,
    pointer: open_gpui::Point<Pixels>,
    modifiers: Modifiers,
    smoke: Option<smoke::Smoke>,
    closing: bool,
    finished: bool,
    failed_at: Option<Instant>,
    error: Option<anyhow::Error>,
    deadline: Instant,
}

impl Viewer {
    fn open(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Nara · GPUI Scene View")
                    .with_inner_size(LogicalSize::new(1280., 640.))
                    .with_visible(self.smoke.is_none()),
            )?,
        );
        let engine = Rc::new(RefCell::new(Engine::new(window.clone())?));
        let proxy = self.proxy.clone();
        let wake: Rc<dyn Fn()> = Rc::new(move || {
            let _ = proxy.send_event(Wake);
        });
        let renderer = Renderer::new(engine.clone(), wake.clone())?;
        renderer.diagnostic_next.set(self.smoke.is_some());
        let preview = engine.borrow_mut().capture()?;
        let panel_snapshot = engine.borrow().panels_snapshot()?;
        let image = renderer.import(&preview)?;
        let content_displays = engine.borrow().content_displays();
        let content_images = content_displays
            .into_iter()
            .map(|display| {
                renderer
                    .import_display(&display)
                    .map(|image| (display, image))
            })
            .collect::<Result<Vec<_>>>()?;
        let physical = window.inner_size();
        let scale = window.scale_factor() as f32;
        let geometry = HostedSurfaceGeometry::from_physical_size(
            size(
                DevicePixels(physical.width.try_into()?),
                DevicePixels(physical.height.try_into()?),
            ),
            scale,
        )?;
        let surface = HostedSurface::new(
            HostedSurfaceOptions {
                size: size(geometry.size().width, geometry.size().height),
                scale_factor: scale,
                ..Default::default()
            },
            renderer.clone(),
            move || wake(),
        )?;
        let initialized = Rc::new(RefCell::new(None));
        let output = initialized.clone();
        let commands = self.commands.clone();
        let clicks = self.clicks.clone();
        let hosted = surface.clone();
        let render = renderer.clone();
        let panel_work = self.panel_work.clone();
        let application =
            Application::with_platform(self.platform.clone()).run_embedded(move |cx| {
                DraftInput::bind_keys(cx);
                let mut draft = None;
                let mut model_output = None;
                let root = cx.open_surface(&hosted, |window, cx| {
                    let input = cx.new(|cx| DraftInput::new(window, cx));
                    input.read(cx).focus_handle(cx).focus(window, cx);
                    draft = Some(input.clone());
                    let model = cx.new(|_| EditorModel {
                        snapshot: panel_snapshot,
                    });
                    let hierarchy = cx.new(|cx| {
                        Hierarchy::new(model.clone(), commands.clone(), panel_work.clone(), cx)
                    });
                    let inspectors =
                        panels::create_inspectors(&model, &commands, &panel_work, window, cx);
                    let scene_view = cx.new(|_| SceneView {
                        preview,
                        image,
                        commands: commands.clone(),
                        candidate: render.candidate_snapshot.clone(),
                        publications: render.publications.clone(),
                        publication: PrepaintPublicationId::new(),
                        clicks: clicks.clone(),
                        content_images,
                        candidate_content: render.candidate_content.clone(),
                    });
                    model_output = Some(model);
                    cx.new(|_| Root {
                        scene_view,
                        draft: input,
                        hierarchy,
                        inspectors,
                    })
                });
                *output.borrow_mut() =
                    Some(root.map(|root| (root, draft.unwrap(), model_output.unwrap())));
            });
        let (root, draft, model) = initialized
            .borrow_mut()
            .take()
            .context("external GPUI launch did not run")??;
        ensure!(
            self.factory.creates.get() == 0,
            "a logical root requested a native GPUI renderer"
        );
        self.engine = Some(engine);
        self.renderer = Some(renderer);
        self.application = Some(application);
        self.surface = Some(surface);
        self.root = Some(root);
        self.draft = Some(draft);
        self.model = Some(model);
        window.set_ime_allowed(true);
        Ok(())
    }

    fn request(&self) -> Result<()> {
        self.request_frame(true)
    }

    fn request_frame(&self, force_render: bool) -> Result<()> {
        self.surface
            .as_ref()
            .unwrap()
            .request_frame(RequestFrameOptions {
                force_render,
                require_presentation: true,
                ..Default::default()
            })?;
        Ok(())
    }

    fn refresh_preview(&self) -> Result<()> {
        let engine = self.engine.as_ref().unwrap();
        let preview = engine.borrow_mut().capture()?;
        let image = self.renderer.as_ref().unwrap().import(&preview)?;
        self.application.as_ref().unwrap().update(|cx| {
            self.root.unwrap().update(cx, |root, _, cx| {
                root.scene_view.update(cx, |scene, cx| {
                    scene.preview = preview;
                    scene.image = image;
                    cx.notify();
                });
            })
        })?;
        self.request_frame(false)
    }

    fn publish_panels(&self) -> Result<()> {
        let snapshot = self.engine.as_ref().unwrap().borrow().panels_snapshot()?;
        self.application.as_ref().unwrap().update(|cx| {
            self.model
                .as_ref()
                .unwrap()
                .update(cx, |model, cx| model.publish(snapshot, cx));
        });
        Ok(())
    }

    fn drive(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if self.finished || self.engine.is_none() {
            return Ok(());
        }
        ensure!(
            Instant::now() < self.deadline || self.smoke.is_none(),
            "GPUI engine smoke exceeded its deadline"
        );
        let mut changed = false;
        let mut panels_changed = false;
        while let Some(command) = self.commands.borrow_mut().pop_front() {
            panels_changed = true;
            let mut engine = self.engine.as_ref().unwrap().borrow_mut();
            match command {
                Command::Move => {
                    engine.move_sprite()?;
                    changed = true;
                }
                Command::Undo => {
                    engine.undo()?;
                    changed = true;
                }
                Command::Redo => {
                    engine.redo()?;
                    changed = true;
                }
                Command::Pick(point) => {
                    if let Some(snapshot) =
                        self.renderer.as_ref().unwrap().presented.borrow().as_ref()
                    {
                        match engine.pick(&snapshot.preview, snapshot.mapping, point) {
                            Ok(Some(_)) => self.picks += 1,
                            Ok(None) => println!("NARA_GPUI_VIEW_PICK empty point={point:?}"),
                            Err(error) => println!(
                                "NARA_GPUI_VIEW_PICK refused point={point:?} cause={error:#}"
                            ),
                        }
                    }
                }
                Command::Select { basis, entity } => {
                    if let Err(error) = engine.select_entity(&basis, entity) {
                        self.panel_refusals += 1;
                        println!("NARA_GPUI_PANEL_REFUSED {error:#}");
                    }
                }
                Command::SetX {
                    basis,
                    entity,
                    value,
                } => match engine.set_sprite_x(&basis, &entity, value) {
                    Ok(()) => changed = true,
                    Err(error) => {
                        self.panel_refusals += 1;
                        println!("NARA_GPUI_PANEL_REFUSED {error:#}");
                    }
                },
            }
        }
        if changed {
            self.refresh_preview()?
        }
        if panels_changed {
            self.publish_panels()?;
        }
        self.application.as_ref().unwrap().update(|_| {});
        let renderer = self.renderer.as_ref().unwrap().clone();
        renderer.promote()?;
        renderer.submit_pending()?;
        self.application.as_ref().unwrap().update(|_| {});
        renderer.promote()?;
        let text_input = self.surface.as_ref().unwrap().text_input_state();
        if let Some(bounds) = text_input.caret_bounds {
            let engine = self.engine.as_ref().unwrap().borrow();
            let ratio = self.surface.as_ref().unwrap().geometry().scale_factor() as f64
                / engine.window.scale_factor();
            engine.window.set_ime_cursor_area(
                LogicalPosition::new(
                    f64::from(bounds.origin.x) * ratio,
                    f64::from(bounds.origin.y) * ratio,
                ),
                LogicalSize::new(
                    f64::from(bounds.size.width).max(1.) * ratio,
                    f64::from(bounds.size.height).max(1.) * ratio,
                ),
            );
        }
        if !self.closing && !renderer.pending() && self.surface.as_ref().unwrap().needs_frame() {
            self.surface.as_ref().unwrap().drive_frame()?;
        }
        if let Some(mut smoke) = self.smoke.take() {
            let done = smoke.advance(self)?;
            self.smoke = Some(smoke);
            if done {
                self.closing = true;
            }
        }
        if self.closing {
            self.surface.as_ref().unwrap().close()?;
            self.application.as_ref().unwrap().update(|cx| cx.quit());
            if renderer.quiesced() && self.platform.is_shutdown_complete() {
                let mut engine = self.engine.as_ref().unwrap().borrow_mut();
                let output = engine.pulse_other()?;
                ensure!(
                    output.is_current(),
                    "closing GPUI stopped another engine producer"
                );
                engine.close()?;
                self.finished = true;
                ensure!(
                    self.factory.creates.get() == 0,
                    "GPUI created a native renderer"
                );
                if self.smoke.is_some() {
                    println!(
                        "NARA_GPUI_VIEW_OK ui_submissions={} stale_discarded={} paint_refusals={} diagnostics={} native_gpui_renderers=0 original_host=true independent_producer_alive=true input_delivery=injected",
                        renderer.submissions.get(),
                        renderer.discarded.get(),
                        renderer.refusals.get(),
                        renderer.diagnostics.get()
                    );
                }
                event_loop.exit();
            }
        }
        Ok(())
    }

    fn click(&self, x: f32, y: f32) -> Result<()> {
        let surface = self.surface.as_ref().unwrap();
        let position = point(px(x), px(y));
        surface.dispatch_input(PlatformInput::MouseMove(MouseMoveEvent {
            position,
            ..Default::default()
        }))?;
        surface.dispatch_input(PlatformInput::MouseDown(MouseDownEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }))?;
        surface.dispatch_input(PlatformInput::MouseUp(MouseUpEvent {
            position,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        }))?;
        Ok(())
    }

    fn focus_inspector(&self, index: usize) -> Result<()> {
        self.click(1000., 252. + index as f32 * 190.)
    }

    fn input(&mut self, event: WindowEvent) -> Result<()> {
        if self.closing || self.finished {
            return Ok(());
        }
        let Some(surface) = &self.surface else {
            return Ok(());
        };
        match event {
            WindowEvent::CloseRequested => self.closing = true,
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                let (physical, scale) = {
                    let engine = self.engine.as_ref().unwrap().borrow();
                    (
                        engine.window.inner_size(),
                        engine.window.scale_factor() as f32,
                    )
                };
                let geometry = HostedSurfaceGeometry::from_physical_size(
                    size(
                        DevicePixels(physical.width.try_into()?),
                        DevicePixels(physical.height.try_into()?),
                    ),
                    scale,
                )?;
                if surface.geometry() == geometry {
                    return Ok(());
                }
                self.engine.as_ref().unwrap().borrow_mut().invalidate();
                if physical.width > 0 && physical.height > 0 {
                    self.refresh_preview()?;
                }
                surface.set_geometry(geometry.size(), geometry.scale_factor())?;
            }
            WindowEvent::Focused(active) if self.smoke.is_none() => surface.set_active(active)?,
            WindowEvent::CursorMoved { position, .. } => {
                let scale = surface.geometry().scale_factor();
                self.pointer = point(px(position.x as f32 / scale), px(position.y as f32 / scale));
                surface.dispatch_input(PlatformInput::MouseMove(MouseMoveEvent {
                    position: self.pointer,
                    modifiers: self.modifiers,
                    ..Default::default()
                }))?;
            }
            WindowEvent::MouseInput {
                state,
                button: NativeButton::Left,
                ..
            } => {
                let input = match state {
                    ElementState::Pressed => PlatformInput::MouseDown(MouseDownEvent {
                        position: self.pointer,
                        button: MouseButton::Left,
                        modifiers: self.modifiers,
                        click_count: 1,
                        ..Default::default()
                    }),
                    ElementState::Released => PlatformInput::MouseUp(MouseUpEvent {
                        position: self.pointer,
                        button: MouseButton::Left,
                        modifiers: self.modifiers,
                        click_count: 1,
                        ..Default::default()
                    }),
                };
                surface.dispatch_input(input)?;
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.modifiers = Modifiers {
                    control: state.control_key(),
                    alt: state.alt_key(),
                    shift: state.shift_key(),
                    platform: state.super_key(),
                    ..Default::default()
                };
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.keyboard_input(
                    event.logical_key,
                    event.text.map(|text| text.to_string()),
                    event.state,
                    event.repeat,
                )?;
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                surface.with_input_handler(|handler| handler.replace_text_in_range(None, &text))?;
            }
            WindowEvent::Ime(Ime::Disabled) => {
                surface.with_input_handler(|handler| {
                    if let Some(range) = handler.marked_text_range() {
                        handler.replace_text_in_range(Some(range), "");
                    }
                })?;
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                let range = cursor.and_then(|(start, end)| {
                    Some(
                        text.get(..start)?.encode_utf16().count()
                            ..text.get(..end)?.encode_utf16().count(),
                    )
                });
                surface.with_input_handler(|handler| {
                    handler.replace_and_mark_text_in_range(None, &text, range)
                })?;
            }
            _ => {}
        }
        Ok(())
    }

    fn keyboard_input(
        &self,
        logical_key: Key,
        text: Option<String>,
        state: ElementState,
        repeat: bool,
    ) -> Result<()> {
        let surface = self
            .surface
            .as_ref()
            .context("the logical root is not open")?;
        let key = match &logical_key {
            Key::Character(value) => value.to_string().to_lowercase(),
            Key::Named(value) => match value {
                NamedKey::ArrowLeft => "left",
                NamedKey::ArrowRight => "right",
                NamedKey::Backspace => "backspace",
                NamedKey::Delete => "delete",
                NamedKey::Home => "home",
                NamedKey::End => "end",
                NamedKey::Tab => "tab",
                NamedKey::Escape => "escape",
                NamedKey::Space => "space",
                NamedKey::Insert => "insert",
                _ => "",
            }
            .to_owned(),
            _ => String::new(),
        };
        if !key.is_empty() {
            let keystroke = Keystroke {
                modifiers: self.modifiers,
                key,
                key_char: None,
            };
            let result = surface.dispatch_input(match state {
                ElementState::Pressed => PlatformInput::KeyDown(KeyDownEvent {
                    keystroke,
                    is_held: repeat,
                    prefer_character_input: false,
                }),
                ElementState::Released => PlatformInput::KeyUp(KeyUpEvent { keystroke }),
            })?;
            if state == ElementState::Pressed
                && result.propagate
                && !result.default_prevented
                && !self.modifiers.control
                && !self.modifiers.alt
                && !self.modifiers.platform
                && let Some(text) =
                    text.filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
            {
                surface.with_input_handler(|handler| {
                    if handler.marked_text_range().is_none() {
                        handler.replace_text_in_range(None, &text);
                    }
                })?;
            }
        }
        Ok(())
    }

    fn fail(&mut self, error: anyhow::Error, event_loop: &ActiveEventLoop) {
        eprintln!("NARA_GPUI_VIEW_FAILURE {error:#}");
        if self.error.is_none() {
            self.error = Some(error);
        }
        self.closing = true;
        self.smoke = None;
        let failed_at = *self.failed_at.get_or_insert_with(Instant::now);
        if let Some(surface) = &self.surface {
            let _ = surface.close();
        }
        if let Some(application) = &self.application {
            application.update(|cx| cx.quit());
        }
        if self.engine.is_none() || failed_at.elapsed() > Duration::from_secs(10) {
            event_loop.exit();
        } else {
            let _ = self.proxy.send_event(Wake);
        }
    }
}

impl ApplicationHandler<Wake> for Viewer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.engine.is_none() {
            if let Err(error) = self.open(event_loop).and_then(|_| self.drive(event_loop)) {
                self.fail(error, event_loop);
            }
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, _: Wake) {
        if let Err(error) = self.drive(event_loop) {
            self.fail(error, event_loop);
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if let Err(error) = self.input(event).and_then(|_| self.drive(event_loop)) {
            self.fail(error, event_loop);
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.drive(event_loop) {
            self.fail(error, event_loop);
            return;
        }
        if self.smoke.is_some() || self.closing {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(10),
            ));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

pub(super) fn run() -> Result<()> {
    static LOGGER: DiagnosticsLogger = DiagnosticsLogger;
    log::set_logger(&LOGGER)
        .map_err(|error| anyhow::anyhow!("install consumer diagnostic logger: {error}"))?;
    log::set_max_level(log::LevelFilter::Warn);
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let backend =
        std::env::var("WGPU_BACKEND").context("select WGPU_BACKEND=dx12 or vulkan explicitly")?;
    ensure!(
        matches!(backend.as_str(), "dx12" | "vulkan"),
        "this native consumer requires an actual Windows GPU backend"
    );
    ensure!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() == Ok("0"),
        "set NARA_WGPU_FORCE_FALLBACK=0"
    );
    let factory = Rc::new(NativeFactory::default());
    let platform = Rc::new(WindowsPlatform::new_external_host(
        Arc::new(CosmicTextSystem::new("Segoe UI")),
        factory.clone(),
    )?);
    let native_platform = platform.clone();
    let mut builder = EventLoop::<Wake>::with_user_event();
    builder.with_msg_hook(move |message| {
        // SAFETY: Winit supplies the current UI-thread Win32 MSG for this hook's duration.
        let message = unsafe { &*message.cast::<MSG>() };
        native_platform.pretranslate_message(message)
    });
    let event_loop = builder.build()?;
    let proxy = event_loop.create_proxy();
    let mut viewer = Viewer {
        engine: None,
        renderer: None,
        surface: None,
        root: None,
        draft: None,
        model: None,
        panel_work: Rc::default(),
        panel_refusals: 0,
        application: None,
        platform,
        factory,
        proxy,
        commands: Rc::default(),
        clicks: Rc::default(),
        picks: 0,
        pointer: point(px(0.), px(0.)),
        modifiers: Modifiers::default(),
        smoke: smoke.then(smoke::Smoke::new),
        closing: false,
        finished: false,
        failed_at: None,
        error: None,
        deadline: Instant::now() + Duration::from_secs(90),
    };
    event_loop.run_app(&mut viewer)?;
    if let Some(error) = viewer.error.take() {
        return Err(error);
    }
    Ok(())
}

struct DiagnosticsLogger;
impl log::Log for DiagnosticsLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {} {}", record.level(), record.target(), record.args());
        }
    }
    fn flush(&self) {}
}
