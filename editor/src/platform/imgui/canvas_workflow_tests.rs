#![cfg(target_os = "windows")]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use dear_imgui_rs::{ConfigFlags, Context, Key, MouseButton};
use nara::{
    project_host::{EditorUiAuthoringFrame, TrustedCompiledEditorProjectIntent},
    reflect::ComponentValue,
    render::DisplayRect,
    tooling::EditorPersistenceResult,
    ui::__private::{
        UiAuthoringCanvasProjection, UiAuthoringDimensionKind, UiAuthoringFieldValue,
        UiAuthoringHierarchyKind,
    },
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};

use super::{
    NativeImgui,
    canvas_tests::retire_application,
    retirement::{finish_graphics_for_test, initialize_native_for_test},
};
use crate::{
    EditorApplication, EditorLaunchOptions,
    frontend::imgui::{UiProject, clear_controls, control_clip, control_rect, control_visible},
};

const IMAGE: &str = "textures/player.png";

struct CanvasWorkflowObservation {
    picked: Option<String>,
    layouts: [[f64; 4]; 6],
    history: [usize; 5],
    saved: [f64; 4],
    unrelated_source_unchanged: bool,
    presents: u64,
    content_generations: [u64; 2],
}

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_canvas_widgets_pick_move_resize_and_save_reopen_real_source() {
    let project = UiProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = CanvasWorkflowApplication {
        project: project.0.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    // All native and Session cleanup completes before behavior assertions can panic.
    let observed = application
        .result
        .expect("the Canvas workflow must run")
        .unwrap();
    let [baseline, moved, resized, undone, redone, reopened] = observed.layouts;
    assert_eq!(observed.picked.as_deref(), Some("image-node"));
    assert!(moved[0] > baseline[0] && moved[1] > baseline[1]);
    assert_eq!(&moved[2..], &baseline[2..]);
    assert_eq!(&resized[..2], &moved[..2]);
    assert!(resized[2] > moved[2] && resized[3] > moved[3]);
    assert_eq!(undone, moved);
    assert_eq!(redone, resized);
    assert_eq!(reopened, resized);
    assert_eq!(observed.saved, resized);
    assert_eq!(observed.history, [0, 1, 2, 1, 2]);
    assert!(observed.unrelated_source_unchanged);
    assert!(observed.presents > 0);
    assert!(observed.content_generations[1] > observed.content_generations[0]);
}

struct CanvasWorkflowApplication {
    project: PathBuf,
    result: Option<Result<CanvasWorkflowObservation, String>>,
}

impl ApplicationHandler for CanvasWorkflowApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_workflow(event_loop, &self.project));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn run_workflow(
    event_loop: &ActiveEventLoop,
    project: &Path,
) -> Result<CanvasWorkflowObservation, String> {
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Nara synthetic Canvas widget workflow")
                    .with_active(false)
                    .with_visible(false)
                    .with_inner_size(LogicalSize::new(1600.0, 1100.0)),
            )
            .map_err(|error| error.to_string())?,
    );
    let mut native = initialize_native_for_test(event_loop, Arc::clone(&window))?;
    native.layout_loaded = true;
    let mut flags = native.context.io().config_flags() | ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
    flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
    native.context.io_mut().set_config_flags(flags);
    let mut application = match EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project).with_profile("desktop"),
        TrustedCompiledEditorProjectIntent::new(),
    ) {
        Ok(application) => application,
        Err(error) => {
            let cleanup = finish_graphics_for_test(&mut native);
            return Err(match cleanup {
                Ok(()) => error.to_string(),
                Err(cleanup) => format!("{error}; native cleanup failed: {cleanup}"),
            });
        }
    };
    application.native_shell_ready();
    let result = CanvasWorkflow {
        native: &mut native,
        application: &mut application,
        event_loop,
        pointer: [-1000.0, -1000.0],
        presents: 0,
    }
    .run(project);
    let graphics = finish_graphics_for_test(&mut native);
    drop(native);
    drop(window);
    let session = retire_application(&mut application);
    drop(application);
    match (result, graphics, session) {
        (Ok(observed), Ok(()), Ok(())) => Ok(observed),
        (result, graphics, session) => {
            let mut failures = Vec::new();
            if let Err(error) = result {
                failures.push(error);
            }
            if let Err(error) = graphics {
                failures.push(format!("native cleanup failed: {error}"));
            }
            if let Err(error) = session {
                failures.push(format!("Session cleanup failed: {error}"));
            }
            Err(failures.join("; "))
        }
    }
}

struct CanvasWorkflow<'a> {
    native: &'a mut NativeImgui,
    application: &'a mut EditorApplication,
    event_loop: &'a ActiveEventLoop,
    pointer: [f32; 2],
    presents: u64,
}

impl CanvasWorkflow<'_> {
    fn paint(&mut self, input: impl FnOnce(&mut Context)) -> Result<(), String> {
        self.application
            .drive_editor_frame(Duration::ZERO, false, true);
        clear_controls();
        let pointer = self.pointer;
        let exited = self
            .native
            .redraw_with_input(self.event_loop, self.application, |context| {
                let viewport = context.main_viewport().id();
                let io = context.io_mut();
                // Native polling still runs. This test replaces its queued events locally without
                // moving the OS pointer, activating a native window or resetting held-button state.
                io.clear_events_queue();
                io.add_focus_event(true);
                io.add_mouse_pos_event(pointer);
                io.add_mouse_viewport_event(viewport);
                input(context);
            })
            .map_err(|error| error.to_string())?;
        if exited {
            return Err("a Canvas widget unexpectedly requested application exit".to_owned());
        }
        self.presents += self.native.gpu.frame_transaction_stats().presents();
        Ok(())
    }

    fn click_point(&mut self, point: [f32; 2]) -> Result<(), String> {
        self.pointer = point;
        self.paint(|_| {})?;
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true)
        })?;
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false)
        })?;
        for _ in 0..3 {
            self.paint(|_| {})?;
        }
        Ok(())
    }

    fn click(&mut self, label: &str) -> Result<(), String> {
        let point = self.reveal(label)?;
        self.click_point(point)
    }

    fn reveal(&mut self, label: &str) -> Result<[f32; 2], String> {
        for _ in 0..32 {
            let rect =
                control_rect(label).ok_or_else(|| format!("widget {label:?} was not painted"))?;
            let clip = control_clip(label)
                .ok_or_else(|| format!("widget {label:?} has no actual draw clip"))?;
            let point = center(rect);
            if contains(clip, point) && control_visible(label) {
                return Ok(point);
            }
            if point[0] < clip.min[0] || point[0] >= clip.max[0] {
                return Err(format!("widget {label:?} is horizontally clipped"));
            }
            if point[1] >= clip.min[1] && point[1] < clip.max[1] {
                // A newly opened popup can still be in its hidden sizing frame.
                self.paint(|_| {})?;
                continue;
            }
            let direction = if point[1] < clip.min[1] { 3.0 } else { -3.0 };
            self.pointer = center(clip);
            // Establish the hovered window first. Input trickling otherwise defers a wheel
            // queued after a mouse move, and paint's next queue replacement would discard it.
            self.paint(|_| {})?;
            self.paint(|context| context.io_mut().add_mouse_wheel_event([0.0, direction]))?;
            self.paint(|_| {})?;
            self.paint(|_| {})?;
            let after = control_rect(label)
                .ok_or_else(|| format!("scrolling stopped painting widget {label:?}"))?;
            if after.min[1] == rect.min[1] {
                return Err(format!(
                    "scrolling reached an edge without revealing widget {label:?}"
                ));
            }
        }
        Err(format!(
            "widget {label:?} stayed clipped after 32 scroll steps"
        ))
    }

    fn press(&mut self, key: Key) -> Result<(), String> {
        self.paint(|context| context.io_mut().add_key_event(key, true))?;
        self.paint(|context| context.io_mut().add_key_event(key, false))
    }

    fn field(&self, entity: &str, field: &str) -> Result<UiAuthoringFieldValue, String> {
        self.authoring()?
            .projection()
            .map_err(|error| error.to_string())?
            .fields()
            .iter()
            .find(|descriptor| {
                descriptor.key().entity().as_str() == entity
                    && descriptor.key().primary_field().as_str() == field
            })
            .map(|descriptor| descriptor.value().clone())
            .ok_or_else(|| format!("the typed UI field {entity}.{field} is unavailable"))
    }

    fn check_history(&self, expected: usize, operation: &str) -> Result<(), String> {
        let actual = self.history()?;
        if actual != expected {
            return Err(format!(
                "{operation} produced history depth {actual}, expected {expected}"
            ));
        }
        Ok(())
    }

    fn cancel_scalar_drag(&mut self) -> Result<(), String> {
        let label = "ui-field:image-node:style.flex_grow";
        let baseline = self.field("image-node", "style.flex_grow")?;
        let history = self.history()?;
        let point = self.reveal(label)?;
        let clip = control_clip(label).ok_or("the scalar field has no actual draw clip")?;
        let dragged = [point[0] + 32.0, point[1]];
        if !contains(clip, dragged) {
            return Err("the real scalar drag would leave the Inspector clip".to_owned());
        }
        self.pointer = point;
        self.paint(|_| {})?;
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true)
        })?;
        self.pointer = dragged;
        self.paint(|_| {})?;
        self.settle_canvas()?;
        if self.authoring()?.gesture().is_none()
            || self.field("image-node", "style.flex_grow")? == baseline
        {
            return Err(
                "the visible scalar widget did not begin and preview its gesture".to_owned(),
            );
        }
        self.press(Key::Escape)?;
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false)
        })?;
        self.settle_canvas()?;
        if self.authoring()?.gesture().is_some()
            || self.field("image-node", "style.flex_grow")? != baseline
        {
            return Err("Escape did not cancel and restore the visible scalar gesture".to_owned());
        }
        self.check_history(history, "cancelling the scalar gesture")
    }

    fn has_image(&self, entity: &str) -> Result<bool, String> {
        Ok(self
            .authoring()?
            .projection()
            .map_err(|error| error.to_string())?
            .hierarchy()
            .iter()
            .any(|node| {
                node.entity().as_str() == entity && node.kind() == UiAuthoringHierarchyKind::Image
            }))
    }

    fn edit_specialized_fields_and_structure(&mut self) -> Result<String, String> {
        let history = self.history()?;
        self.click("ui-field:image-node:style.visible")?;
        if self.field("image-node", "style.visible")? != UiAuthoringFieldValue::Bool(false) {
            return Err("the visible bool widget did not hide image-node".to_owned());
        }
        self.check_history(history + 1, "editing the bool field")?;
        self.click("Undo")?;
        if self.field("image-node", "style.visible")? != UiAuthoringFieldValue::Bool(true) {
            return Err("Undo did not restore the bool field".to_owned());
        }
        self.check_history(history, "undoing the bool field")?;
        self.click("Redo")?;
        if self.field("image-node", "style.visible")? != UiAuthoringFieldValue::Bool(false) {
            return Err("Redo did not restore the edited bool field".to_owned());
        }
        self.check_history(history + 1, "redoing the bool field")?;
        self.click("Add UI node")?;
        self.click("Node kind")?;
        self.press(Key::DownArrow)?;
        self.press(Key::DownArrow)?;
        self.press(Key::Enter)?;
        self.click("Create image")?;
        self.click(&format!("create-image:{IMAGE}"))?;
        self.click("Apply structure change")?;
        let created = self
            .authoring()?
            .selection()
            .top_entity()
            .map(|entity| entity.as_str().to_owned())
            .ok_or("the real create widget did not select its new Image")?;
        if created == "image-node" || !self.has_image(&created)? {
            return Err("the real create widget did not create a distinct Image".to_owned());
        }
        self.check_history(history + 2, "creating the chosen Image")?;
        self.click("Delete subtree")?;
        self.click("Apply structure change")?;
        if self.has_image(&created)? {
            return Err("the real delete widget left the new Image present".to_owned());
        }
        self.check_history(history + 3, "deleting the new Image")?;
        self.click("Undo")?;
        if !self.has_image(&created)? {
            return Err("Undo did not restore the deleted Image".to_owned());
        }
        self.check_history(history + 2, "undoing Image deletion")?;
        self.click("Redo")?;
        if self.has_image(&created)? {
            return Err("Redo did not delete the Image again".to_owned());
        }
        self.check_history(history + 3, "redoing Image deletion")?;
        self.click("Undo")?;
        if !self.has_image(&created)? {
            return Err("the final Undo did not restore the Image for saving".to_owned());
        }
        self.check_history(history + 2, "restoring the Image for saving")?;
        Ok(created)
    }

    fn authoring(&self) -> Result<Arc<EditorUiAuthoringFrame>, String> {
        self.application
            .current_ui_authoring_frame()
            .ok_or_else(|| "the UI Prefab has no authoring frame".to_owned())
    }

    fn settle_canvas(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        for _ in 0..120 {
            self.paint(|_| {})?;
            if let Some(authoring) = self.application.current_ui_authoring_frame()
                && self
                    .native
                    .canvas
                    .is_current(&self.native.scene, &authoring)
                && self.native.presented
                && self.native.gpu.frame_transaction_stats().presents() > 0
                && control_rect("canvas-input").is_some()
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(
            "the actual sampled UI Canvas did not become current within the frame budget"
                .to_owned(),
        )
    }

    fn layout(&self) -> Result<[f64; 4], String> {
        let frame = self.authoring()?;
        let projection = frame.projection().map_err(|error| error.to_string())?;
        let mut values = [0.0; 4];
        for (index, field) in [
            "style.left.kind",
            "style.top.kind",
            "style.width.kind",
            "style.height.kind",
        ]
        .iter()
        .enumerate()
        {
            let descriptor = projection
                .fields()
                .iter()
                .find(|descriptor| {
                    descriptor.key().entity().as_str() == "image-node"
                        && descriptor.key().primary_field().as_str() == *field
                })
                .ok_or_else(|| format!("the typed UI field {field} is unavailable"))?;
            let UiAuthoringFieldValue::Dimension {
                kind: UiAuthoringDimensionKind::Pixels,
                value,
            } = descriptor.value()
            else {
                return Err(format!(
                    "the typed UI field {field} is not an absolute pixel dimension"
                ));
            };
            values[index] = *value;
        }
        Ok(values)
    }

    fn history(&self) -> Result<usize, String> {
        let authoring = self.authoring()?;
        self.application
            .current_frame()
            .and_then(|frame| {
                frame
                    .workspace()
                    .document(authoring.document())
                    .map(|tab| tab.history.undo_depth)
            })
            .ok_or_else(|| "the UI Prefab has no history projection".to_owned())
    }

    fn source_point(&self) -> Result<[f32; 2], String> {
        let frame = self.authoring()?;
        if !self.native.canvas.is_current(&self.native.scene, &frame) {
            return Err("Canvas picking requires the exact presented UI generation".to_owned());
        }
        let projection = frame.projection().map_err(|error| error.to_string())?;
        let UiAuthoringCanvasProjection::Ready(canvas) = projection.canvas() else {
            return Err("the presented UI Canvas has no geometry".to_owned());
        };
        let fact = canvas
            .facts()
            .iter()
            .find(|fact| fact.entity().as_str() == "image-node" && fact.visible())
            .ok_or("the presented image-node has no visible Canvas fact")?;
        let image =
            control_rect("canvas-input").ok_or("the Canvas image widget was not painted")?;
        let transform = canvas.content_transform();
        let physical = transform.physical_surface_size();
        let origin = transform.physical_content_origin();
        let scale = transform.content_scale_factor();
        let min = fact.rect().origin();
        let max = fact.rect().max();
        Ok([
            image.min[0]
                + (origin.x() + (min.x() + max.x()) * 0.5 * scale) / physical.width()
                    * (image.max[0] - image.min[0]),
            image.min[1]
                + (origin.y() + (min.y() + max.y()) * 0.5 * scale) / physical.height()
                    * (image.max[1] - image.min[1]),
        ])
    }

    fn drag(&mut self, control: &str, delta: [f32; 2]) -> Result<(), String> {
        let rect = control_rect(control)
            .ok_or_else(|| format!("Canvas control {control} was not painted"))?;
        self.pointer = center(rect);
        self.paint(|_| {})?;
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true)
        })?;
        self.pointer = [self.pointer[0] + delta[0], self.pointer[1] + delta[1]];
        self.paint(|_| {})?;
        self.settle_canvas()?;
        if self.authoring()?.gesture().is_none() {
            return Err(format!(
                "dragging the actual {control} did not begin a document gesture"
            ));
        }
        self.paint(|context| {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false)
        })?;
        self.settle_canvas()?;
        if self.authoring()?.gesture().is_some() {
            return Err(format!(
                "releasing the actual {control} did not commit its gesture"
            ));
        }
        Ok(())
    }

    fn settle_persistence(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            self.paint(|_| {})?;
            if self
                .application
                .current_frame()
                .is_some_and(|frame| frame.persistence().operation().is_none())
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err("the widget-requested persistence operation did not finish".to_owned())
    }

    fn run(mut self, project: &Path) -> Result<CanvasWorkflowObservation, String> {
        let source = project.join("prefabs/ui.prefab.json");
        let scene = project.join("scenes/startup.scene.json");
        let original_bytes = fs::read(&source).map_err(|error| error.to_string())?;
        let original: serde_json::Value =
            serde_json::from_slice(&original_bytes).map_err(|error| error.to_string())?;
        let original_scene = fs::read(&scene).map_err(|error| error.to_string())?;
        for _ in 0..6 {
            self.paint(|_| {})?;
        }
        self.click("entity:ui-anchor")?;
        self.click("Open Prefab Source")?;
        self.settle_canvas()?;
        let baseline = self.layout()?;
        let initial_history = self.history()?;
        self.click_point(self.source_point()?)?;
        self.settle_canvas()?;
        let picked = self
            .authoring()?
            .selection()
            .top_entity()
            .map(|entity| entity.as_str().to_owned());
        if picked.as_deref() != Some("image-node") {
            return Err(format!(
                "the actual Canvas click selected {picked:?} instead of image-node"
            ));
        }
        self.drag("canvas-selection", [24.0, 16.0])?;
        let moved = self.layout()?;
        let moved_history = self.history()?;
        self.drag("canvas-resize", [20.0, 12.0])?;
        let resized = self.layout()?;
        let resized_history = self.history()?;
        self.click("Undo")?;
        self.settle_canvas()?;
        let undone = self.layout()?;
        let undo_history = self.history()?;
        self.click("Redo")?;
        self.settle_canvas()?;
        let redone = self.layout()?;
        let redo_history = self.history()?;
        self.cancel_scalar_drag()?;
        if fs::read(&source).map_err(|error| error.to_string())? != original_bytes {
            return Err("the cancelled scalar preview wrote the source".to_owned());
        }
        let created = self.edit_specialized_fields_and_structure()?;
        if fs::read(&source).map_err(|error| error.to_string())? != original_bytes {
            return Err("the accepted widget edits wrote the source before Save".to_owned());
        }
        let document = self.authoring()?.document();
        if !self.application.current_frame().is_some_and(|frame| {
            frame
                .workspace()
                .document(document)
                .is_some_and(|tab| tab.dirty)
        }) {
            return Err("the widget edits did not leave the UI Prefab dirty".to_owned());
        }
        let before_content = self
            .application
            .current_frame()
            .and_then(|frame| frame.project_content().current_generation())
            .ok_or("trusted Canvas has no project-content generation")?;
        self.click("Save")?;
        self.settle_persistence()?;
        if !self.application.current_frame().is_some_and(|frame| {
            matches!(
                frame.persistence().result(),
                Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document
            ) && frame
                .workspace()
                .document(document)
                .is_some_and(|tab| !tab.dirty)
        }) {
            return Err("the real Save widget did not save the UI Prefab".to_owned());
        }
        let refresh_deadline = Instant::now() + Duration::from_secs(5);
        let after_content = loop {
            let frame = self
                .application
                .current_frame()
                .ok_or("the Canvas frame disappeared")?;
            if frame.project_content().is_current()
                && !frame.authoring_retirement_pending()
                && let Some(generation) = frame.project_content().current_generation()
                && generation > before_content
            {
                break generation;
            }
            if Instant::now() >= refresh_deadline {
                return Err(
                    "Save did not publish refreshed content and retire the authoring predecessor"
                        .to_owned(),
                );
            }
            self.paint(|_| {})?;
            std::thread::sleep(Duration::from_millis(1));
        };
        let stored: serde_json::Value =
            serde_json::from_slice(&fs::read(&source).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        let image = source_entity(&stored, "image-node")?;
        let component: ComponentValue =
            serde_json::from_value(image["components"]["nara.ui.UiNode"]["value"].clone())
                .map_err(|error| error.to_string())?;
        let mut saved = [0.0; 4];
        for (index, field) in ["left", "top", "width", "height"].iter().enumerate() {
            saved[index] = component
                .field("style")
                .and_then(|style| style.field(field))
                .and_then(|value| value.field_f64("value"))
                .map_err(|error| error.to_string())?;
        }
        check_preserved_source(&original, &stored, &created, resized)?;
        if fs::read(&scene).map_err(|error| error.to_string())? != original_scene {
            return Err("editing the UI Prefab changed the unrelated startup Scene".to_owned());
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        self.settle_canvas()?;
        if !self.application.current_frame().is_some_and(|frame| {
            matches!(
                frame.persistence().result(),
                Some(EditorPersistenceResult::Opened { document: actual, .. }) if actual == document
            ) && frame
                .workspace()
                .document(document)
                .is_some_and(|tab| !tab.dirty)
        }) {
            return Err("the real Reopen widget did not reopen the UI Prefab".to_owned());
        }
        self.check_history(0, "reopening the saved UI Prefab")?;
        if self.field("image-node", "style.visible")? != UiAuthoringFieldValue::Bool(false)
            || !self.has_image(&created)?
        {
            return Err("Reopen lost the saved bool or the created Image".to_owned());
        }
        let reopened_source: serde_json::Value =
            serde_json::from_slice(&fs::read(&source).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        if reopened_source != stored {
            return Err("Reopen modified the saved UI source".to_owned());
        }
        Ok(CanvasWorkflowObservation {
            picked,
            layouts: [baseline, moved, resized, undone, redone, self.layout()?],
            history: [
                initial_history,
                moved_history,
                resized_history,
                undo_history,
                redo_history,
            ],
            saved,
            unrelated_source_unchanged: true,
            presents: self.presents,
            content_generations: [before_content.get(), after_content.get()],
        })
    }
}

fn center(rect: DisplayRect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}

fn contains(rect: DisplayRect, point: [f32; 2]) -> bool {
    point[0] >= rect.min[0]
        && point[0] < rect.max[0]
        && point[1] >= rect.min[1]
        && point[1] < rect.max[1]
}

fn check_preserved_source(
    original: &serde_json::Value,
    stored: &serde_json::Value,
    created: &str,
    resized: [f64; 4],
) -> Result<(), String> {
    let original_entities = original["payload"]["entities"]
        .as_array()
        .ok_or("the original UI source has no entity records")?;
    let stored_entities = stored["payload"]["entities"]
        .as_array()
        .ok_or("the saved UI source has no entity records")?;
    if stored_entities.len() != original_entities.len() + 1
        || stored_entities
            .iter()
            .filter(|record| record["id"].as_str() == Some(created))
            .count()
            != 1
    {
        return Err(
            "Save did not preserve exactly the original records plus one new Image".to_owned(),
        );
    }
    let mut expected = original.clone();
    let expected_entities = expected["payload"]["entities"]
        .as_array_mut()
        .ok_or("the expected UI source has no entity records")?;
    for record in expected_entities {
        let id = record["id"]
            .as_str()
            .ok_or("an original source record has no identity")?
            .to_owned();
        match id.as_str() {
            "ui-root" => {
                let children = record["children"]
                    .as_array_mut()
                    .ok_or("the original UI root has no children")?;
                // The dialog's first admitted placement inserts before the root's first child.
                children.insert(0, serde_json::Value::String(created.to_owned()));
            }
            "image-node" => {
                let mut node: ComponentValue =
                    serde_json::from_value(record["components"]["nara.ui.UiNode"]["value"].clone())
                        .map_err(|error| error.to_string())?;
                let ComponentValue::Map(fields) = &mut node else {
                    return Err("the original UiNode is not a field map".to_owned());
                };
                let Some(ComponentValue::Map(style)) = fields.get_mut("style") else {
                    return Err("the original UiNode has no style map".to_owned());
                };
                for (field, value) in ["left", "top", "width", "height"].into_iter().zip(resized) {
                    let Some(ComponentValue::Map(dimension)) = style.get_mut(field) else {
                        return Err(format!("the original {field} is not a dimension map"));
                    };
                    *dimension
                        .get_mut("value")
                        .ok_or_else(|| format!("{field} has no value"))? =
                        ComponentValue::f64(value).map_err(|error| error.to_string())?;
                }
                *style
                    .get_mut("visible")
                    .ok_or("the original style has no visible field")? =
                    ComponentValue::Bool(false);
                record["components"]["nara.ui.UiNode"]["value"] =
                    serde_json::to_value(node).map_err(|error| error.to_string())?;
            }
            _ => {}
        }
        if source_entity(stored, &id)? != &*record {
            return Err(format!(
                "Save changed unrelated fields on original entity {id}"
            ));
        }
    }
    let created_record = source_entity(stored, created)?;
    let components = created_record["components"]
        .as_object()
        .ok_or("the created Image has no component records")?;
    if components.len() != 2
        || !components.contains_key("nara.ui.UiNode")
        || !components.contains_key("nara.ui.UiVisual")
        || created_record["children"]
            .as_array()
            .is_none_or(|children| !children.is_empty())
    {
        return Err("the created Image has an unexpected component or child recipe".to_owned());
    }
    let visual: ComponentValue =
        serde_json::from_value(created_record["components"]["nara.ui.UiVisual"]["value"].clone())
            .map_err(|error| error.to_string())?;
    let brush = visual
        .field("background")
        .map_err(|error| error.to_string())?;
    let image = brush.field("image").map_err(|error| error.to_string())?;
    if brush
        .field("kind")
        .map_err(|error| error.to_string())?
        .as_str()
        != Some("image")
        || image
            .field("kind")
            .map_err(|error| error.to_string())?
            .as_str()
            != Some("path")
        || image
            .field("value")
            .map_err(|error| error.to_string())?
            .as_str()
            != Some(IMAGE)
    {
        return Err("Save lost the exact catalog image chosen in the real popup".to_owned());
    }
    // Records were checked above; retaining their saved order isolates all remaining source fields.
    expected["payload"]["entities"] = stored["payload"]["entities"].clone();
    if expected != *stored {
        return Err("Save changed unrelated UI source metadata or roots".to_owned());
    }
    Ok(())
}

fn source_entity<'a>(
    document: &'a serde_json::Value,
    entity: &str,
) -> Result<&'a serde_json::Value, String> {
    document["payload"]["entities"]
        .as_array()
        .and_then(|entities| {
            entities
                .iter()
                .find(|record| record["id"].as_str() == Some(entity))
        })
        .ok_or_else(|| format!("the stored UI Prefab has no {entity} source record"))
}
