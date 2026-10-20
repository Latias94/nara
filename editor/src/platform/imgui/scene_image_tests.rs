#![cfg(target_os = "windows")]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use dear_imgui_rs::ConfigFlags;
use nara::{
    image::PreparedImageSnapshot,
    render_wgpu::__private::WgpuSubmittedImageReceipt,
    tooling::{
        EditorAction, EditorActionBatch, EditorImageImportAttemptView, EditorImageImportSettings,
        EditorProjectBrowserLocalAction,
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

use super::{NativeImgui, canvas_tests::retire_application};
use crate::{EditorApplication, EditorLaunchOptions, frontend::imgui::UiProject};

const IMAGE: &str = "assets/textures/player.png";
// A valid 1x1 RGBA PNG with a visibly different green pixel [17, 220, 33, 255].
const REPLACEMENT_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 16, 188, 163, 248, 31, 0, 4,
    30, 2, 14, 70, 154, 30, 225, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_scene_reimport_replaces_presented_pixels_without_document_or_extent_changes() {
    let project = UiProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = SceneImageApplication {
        project: project.0.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    // Surface, renderer, Session and project capabilities are retired before a failure can panic.
    application
        .result
        .expect("the Scene image publication workflow must run")
        .unwrap();
}

struct SceneImageApplication {
    project: PathBuf,
    result: Option<Result<(), String>>,
}

impl ApplicationHandler for SceneImageApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_scene_image_workflow(event_loop, &self.project));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn run_scene_image_workflow(event_loop: &ActiveEventLoop, project: &Path) -> Result<(), String> {
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Nara Scene image publication regression")
                    .with_active(false)
                    .with_visible(false)
                    .with_inner_size(LogicalSize::new(1600.0, 1100.0)),
            )
            .map_err(|error| error.to_string())?,
    );
    let mut native =
        NativeImgui::new(event_loop, Arc::clone(&window)).map_err(|error| error.to_string())?;
    native.layout_loaded = true;
    let mut flags = native.context.io().config_flags() | ConfigFlags::NO_MOUSE_CURSOR_CHANGE;
    flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
    native.context.io_mut().set_config_flags(flags);
    // This ordinary lane admits data and built-in preview types without compiling project Rust.
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(project).with_profile("desktop"));
    application.native_shell_ready();
    let result = SceneImageWorkflow {
        native: &mut native,
        application: &mut application,
        event_loop,
    }
    .run(project);
    let graphics = native
        .shutdown_graphics()
        .map_err(|error| error.to_string());
    drop(native);
    drop(window);
    let session = retire_application(&mut application);
    drop(application);
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
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

struct SceneImageWorkflow<'a> {
    native: &'a mut NativeImgui,
    application: &'a mut EditorApplication,
    event_loop: &'a ActiveEventLoop,
}

impl SceneImageWorkflow<'_> {
    fn paint(&mut self) -> Result<(), String> {
        self.application.advance_project_open();
        self.application
            .drive_editor_frame(Duration::ZERO, false, false);
        let exited = self
            .native
            .redraw_with_input(self.event_loop, self.application, |context| {
                let viewport = context.main_viewport().id();
                let io = context.io_mut();
                // Keep all synthetic input in this inactive Context, after native input polling.
                io.clear_events_queue();
                io.add_focus_event(true);
                io.add_mouse_pos_event([-1000.0, -1000.0]);
                io.add_mouse_viewport_event(viewport);
            })
            .map_err(|error| error.to_string())?;
        if exited {
            return Err("the Scene image workflow unexpectedly exited".to_owned());
        }
        let stats = self.native.gpu.frame_transaction_stats();
        if !self.native.presented || stats.surface_acquires().min(stats.presents()) == 0 {
            return Err("the hidden primary Scene frame was not actually presented".to_owned());
        }
        Ok(())
    }

    fn displayed(&mut self) -> Result<WgpuSubmittedImageReceipt, String> {
        let frame = self
            .application
            .current_frame()
            .ok_or("the ordinary project has no Editor frame")?;
        if !self.native.scene_presenter.is_presented(
            &self.native.scene,
            &frame,
            self.native.scene_extent,
        ) {
            return Err(
                "the Scene image has no exact successful composite presentation".to_owned(),
            );
        }
        self.native
            .scene
            .submitted
            .ok_or_else(|| "the displayed Scene has no submitted image receipt".to_owned())
    }

    fn import(&mut self) -> Result<PreparedImageSnapshot, String> {
        let frame = self
            .application
            .current_frame()
            .ok_or("the ordinary project has no Editor frame")?;
        let detail = frame
            .image_import()
            .ok_or("the selected image has no Import detail")?;
        if !detail.can_submit() {
            return Err(format!(
                "the real image transaction is unavailable: {detail:?}"
            ));
        }
        self.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::ImageImport(
                    detail.submit_action(EditorImageImportSettings::default()),
                )],
            ),
            Duration::ZERO,
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.paint()?;
            let frame = self
                .application
                .current_frame()
                .ok_or("the image import lost its Session")?;
            let detail = frame
                .image_import()
                .ok_or("the image import lost its detail")?;
            match detail.attempt() {
                EditorImageImportAttemptView::Succeeded { .. }
                    if !frame.image_import_work_pending() =>
                {
                    break;
                }
                EditorImageImportAttemptView::Failed { .. }
                | EditorImageImportAttemptView::ReconcileRequired { .. } => {
                    return Err(format!("the real image transaction failed: {detail:?}"));
                }
                _ if Instant::now() >= deadline => {
                    return Err(format!(
                        "the real image transaction did not finish: {detail:?}"
                    ));
                }
                _ => std::thread::sleep(Duration::from_millis(1)),
            }
        }
        // Inspect the exact immutable prepared image that the ordinary preview owner produces.
        // The next drive supplies its successor candidate to the real NativeHost, not a fake GPU.
        self.application
            .drive_editor_frame(Duration::ZERO, false, false);
        let candidate = self
            .application
            .take_scene_preview_candidate()
            .ok_or("the owner published no Scene candidate")?;
        let (_, _, _, _, batches, _) = candidate.into_parts();
        let images = batches
            .as_slice()
            .iter()
            .filter_map(|batch| batch.image.as_ref())
            .collect::<Vec<_>>();
        match images.as_slice() {
            [image] => Ok((*image).clone()),
            _ => Err(format!(
                "the ordinary Sprite must retain one prepared image, got {}",
                images.len()
            )),
        }
    }

    fn run(&mut self, project: &Path) -> Result<(), String> {
        let original_scene = fs::read(project.join("scenes/startup.scene.json"))
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.paint()?;
            if self.application.current_frame().is_some() && self.native.scene.submitted.is_some() {
                break;
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "ordinary data-only Open did not produce a Scene image: state={:?}, extent={:?}, target={:?}, diagnostics={:?}",
                    self.application.state(),
                    self.native.scene_extent,
                    self.native.scene.target,
                    self.application
                        .current_frame()
                        .map(|frame| frame.diagnostics().clone()),
                ));
            }
            // Keep this image-publication proof independent from the embedded Game pane's focus
            // and docking. The real Authoring pane must request its own physical preview extent.
            if self.application.current_frame().is_some() {
                let game = self
                    .application
                    .panel_workspace()
                    .layout()
                    .instances()
                    .iter()
                    .find(|panel| panel.kind() == "nara.game-view" && panel.is_visible())
                    .map(|panel| panel.id());
                if let Some(game) = game {
                    self.application
                        .panel_workspace_mut()
                        .layout_mut()
                        .close(game);
                    self.native.frontend.reset_layout();
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if !self.application.apply_project_browser_local_action(
            EditorProjectBrowserLocalAction::Select {
                relative_path: Some(Arc::from(IMAGE)),
            },
        ) {
            return Err(
                "the ordinary Project Browser did not select the referenced PNG".to_owned(),
            );
        }
        loop {
            self.paint()?;
            let frame = self
                .application
                .current_frame()
                .ok_or("the image selection lost its Session")?;
            if !frame.image_import_work_pending()
                && frame
                    .image_import()
                    .is_some_and(|detail| detail.can_submit())
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err("the referenced PNG did not become importable".to_owned());
            }
        }
        let frame = self.application.current_frame().unwrap();
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the startup Scene is unavailable")?;
        let document = (scene.document, scene.revision);
        let extent = self.native.scene_extent;
        let before_import = self.displayed()?;
        let first = self.import()?;
        if first.resource().pixels() != [24, 120, 220, 255] {
            return Err("the first real import did not prepare the original blue pixel".to_owned());
        }
        self.paint()?;
        let first_display = self.displayed()?;
        if first_display == before_import {
            return Err("successful image publication retained the old Scene submission despite a new prepared image".to_owned());
        }
        fs::write(project.join(IMAGE), REPLACEMENT_PNG).map_err(|error| error.to_string())?;
        let replacement = self.import()?;
        if replacement.resource().pixels() != [17, 220, 33, 255]
            || replacement.identity() == first.identity()
        {
            return Err(
                "Reimport did not publish distinct prepared green pixels and resource identity"
                    .to_owned(),
            );
        }
        self.paint()?;
        let replacement_display = self.displayed()?;
        if replacement_display == first_display {
            return Err("green-pixel Reimport retained the old blue Scene submission".to_owned());
        }
        let target = self.native.scene.identity;
        let next_view = self.native.next_view;
        for _ in 0..4 {
            self.paint()?;
            if self.displayed()? != replacement_display
                || self.native.scene.identity != target
                || self.native.next_view != next_view
                || self
                    .native
                    .gpu
                    .frame_transaction_stats()
                    .queue_submissions()
                    != 1
            {
                return Err(
                    "unchanged Scene repaint resubmitted pixels or replaced its target".to_owned(),
                );
            }
        }
        let frame = self.application.current_frame().unwrap();
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the workflow lost its startup Scene")?;
        if (scene.document, scene.revision) != document
            || self.native.scene_extent != extent
            || fs::read(project.join("scenes/startup.scene.json"))
                .map_err(|error| error.to_string())?
                != original_scene
        {
            return Err(
                "image-only publication changed the Scene source, revision or panel extent"
                    .to_owned(),
            );
        }
        Ok(())
    }
}
