#![cfg(target_os = "windows")]

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use dear_imgui_rs::{ConfigFlags, MouseButton};
use nara::{
    project_host::TrustedCompiledEditorProjectIntent,
    scene::SceneEntityId,
    tooling::{EditorAction, EditorActionBatch, EditorNavigationAction},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};

use super::{NativeImgui, canvas_tests::retire_application, imgui_next_deadline};
use crate::{
    EditorApplication, EditorLaunchOptions,
    frontend::imgui::{UiProject, clear_controls, control_clip, control_rect, control_visible},
};

struct Observation {
    initially_presented: bool,
    retired_when_closed: bool,
    reopened_presented: bool,
    same_geometry_generation: bool,
    document_unchanged: bool,
    unconfirmed_pick_granted: bool,
    redisplayed_with_same_image: bool,
    frames: [usize; 3],
}

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn an_idle_canvas_reopens_without_unrelated_input_or_forced_frames() {
    let observed = native_observation();
    assert!(observed.initially_presented);
    assert!(observed.retired_when_closed);
    assert!(
        observed.reopened_presented,
        "the Host reached idle before presenting the reopened Canvas: frames {:?}",
        observed.frames
    );
    assert!(observed.same_geometry_generation);
    assert!(observed.document_unchanged);
}

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn an_unconfirmed_canvas_redisplay_cannot_grant_picking() {
    let observed = native_observation();
    assert!(observed.initially_presented);
    assert!(
        !observed.unconfirmed_pick_granted,
        "a prior Canvas receipt cannot certify a later unconfirmed redisplay"
    );
    assert!(
        observed.redisplayed_with_same_image,
        "a deferred redisplay must retain its already submitted image"
    );
    assert!(observed.document_unchanged);
}

struct MenuObservation {
    initially_presented: bool,
    retired_when_closed: bool,
    reopened_presented: bool,
    document_unchanged: bool,
    menu_open_when_idle: bool,
    input_frames: usize,
    deadline_frames: usize,
}

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn the_workspace_checkbox_reopens_an_idle_canvas_using_only_host_deadlines() {
    let project = UiProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut app = MenuCanvasTest {
        project,
        result: None,
    };
    event_loop.run_app(&mut app).unwrap();
    // Do not turn the menu's possible follow-up frames into an unconditional proof frame.
    // Closing it is left to normal Context teardown after the observed idle state.
    let observed = app
        .result
        .expect("the native Workspace menu test must run")
        .unwrap();
    println!(
        "Workspace checkbox Canvas: input_frames={}, deadline_frames={}, menu_open_at_idle={}",
        observed.input_frames, observed.deadline_frames, observed.menu_open_when_idle,
    );
    assert!(observed.initially_presented);
    assert!(observed.retired_when_closed);
    assert!(
        observed.reopened_presented,
        "the Host reached idle without presenting the Canvas reopened by the actual Workspace checkbox"
    );
    assert!(observed.document_unchanged);
}

struct MenuCanvasTest {
    project: UiProject,
    result: Option<Result<MenuObservation, String>>,
}

impl ApplicationHandler for MenuCanvasTest {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_with_observer(event_loop, &self.project.0, observe_menu));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn native_observation() -> Observation {
    let project = UiProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut app = IdleCanvasTest {
        project,
        result: None,
    };
    event_loop.run_app(&mut app).unwrap();
    // Graphics and the Session retire before any behavior assertion.
    let observed = app
        .result
        .expect("the native idle Canvas test must run")
        .unwrap();
    println!("idle Canvas frames: {:?}", observed.frames);
    observed
}

struct IdleCanvasTest {
    project: UiProject,
    result: Option<Result<Observation, String>>,
}

impl ApplicationHandler for IdleCanvasTest {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run(event_loop, &self.project.0));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn run(event_loop: &ActiveEventLoop, project: &Path) -> Result<Observation, String> {
    run_with_observer(event_loop, project, observe)
}

fn run_with_observer<T>(
    event_loop: &ActiveEventLoop,
    project: &Path,
    observer: impl FnOnce(
        &ActiveEventLoop,
        &mut NativeImgui,
        &mut EditorApplication,
    ) -> Result<T, String>,
) -> Result<T, String> {
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Nara idle Canvas reopening regression")
                    .with_visible(false)
                    .with_active(false)
                    .with_inner_size(LogicalSize::new(1200.0, 800.0)),
            )
            .map_err(|error| error.to_string())?,
    );
    let mut native =
        NativeImgui::new(event_loop, Arc::clone(&window)).map_err(|error| error.to_string())?;
    let mut flags = native.context.io().config_flags();
    flags.remove(ConfigFlags::VIEWPORTS_ENABLE);
    flags.insert(ConfigFlags::NO_MOUSE_CURSOR_CHANGE);
    native.context.io_mut().set_config_flags(flags);
    native.layout_loaded = true;
    let opened = EditorApplication::open_trusted_compiled(
        EditorLaunchOptions::new(project).with_profile("desktop"),
        TrustedCompiledEditorProjectIntent::new(),
    );
    let mut application = match opened {
        Ok(application) => application,
        Err(error) => {
            let cleanup = native.shutdown_graphics();
            return Err(match cleanup {
                Ok(()) => error.to_string(),
                Err(cleanup) => format!("{error}; graphics cleanup also failed: {cleanup}"),
            });
        }
    };
    application.native_shell_ready();
    let result = observer(event_loop, &mut native, &mut application);
    let graphics = native.shutdown_graphics();
    drop(native);
    drop(window);
    let retirement = retire_application(&mut application);
    drop(application);
    match (result, graphics, retirement) {
        (Ok(observed), Ok(()), Ok(())) => Ok(observed),
        (result, graphics, retirement) => {
            let mut errors = Vec::new();
            if let Err(error) = result {
                errors.push(error);
            }
            if let Err(error) = graphics {
                errors.push(format!("graphics cleanup failed: {error}"));
            }
            if let Err(error) = retirement {
                errors.push(format!("Session cleanup failed: {error}"));
            }
            Err(errors.join("; "))
        }
    }
}

fn observe_menu(
    event_loop: &ActiveEventLoop,
    native: &mut NativeImgui,
    application: &mut EditorApplication,
) -> Result<MenuObservation, String> {
    let frame = application
        .current_frame()
        .ok_or("the startup Scene is unavailable")?;
    let scene = frame
        .workspace()
        .active_scene
        .as_ref()
        .ok_or("the startup Scene is unavailable")?;
    let anchor = SceneEntityId::new("ui-anchor").map_err(|error| error.to_string())?;
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene.document,
                    entity: Some(anchor.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene.document,
                    base_revision: scene.revision,
                    entity: anchor,
                }),
            ],
        ),
        Duration::ZERO,
    );
    pump_until_idle(event_loop, native, application)?;
    let authoring = application
        .current_ui_authoring_frame()
        .ok_or("the UI Prefab is unavailable")?;
    let document = authoring.document();
    let revision = authoring.revision();
    let initially_presented = native.canvas.is_current(&native.scene, &authoring);
    let panel = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|instance| instance.kind() == "nara.authoring-view")
        .ok_or("the Authoring panel is unavailable")?
        .id();
    application.panel_workspace_mut().layout_mut().close(panel);
    pump_until_idle(event_loop, native, application)?;
    let retired_when_closed = native.scene.submitted.is_none() && native.scene.view.is_none();

    let mut input_frames = 0;
    click_menu_control(
        event_loop,
        native,
        application,
        "Workspace menu",
        &mut input_frames,
    )?;
    let checkbox = format!("panel-instance:{}", panel.0);
    click_menu_control(
        event_loop,
        native,
        application,
        &checkbox,
        &mut input_frames,
    )?;
    if !application
        .panel_workspace()
        .layout()
        .instance(panel)
        .is_some_and(|panel| panel.is_visible())
    {
        return Err("the actual Workspace checkbox did not reopen its existing panel".to_owned());
    }
    // This is the last input frame. In particular, no Escape or unrelated pointer event may
    // accidentally provide the extra redraw that this regression is trying to detect.
    let timeout = Instant::now() + Duration::from_secs(5);
    let mut deadline_frames = 0;
    loop {
        let now = Instant::now();
        let Some(deadline) = imgui_next_deadline(application, Some(native), now) else {
            break;
        };
        if now >= timeout {
            return Err(
                "the Workspace menu never released its actual Host redraw deadline".to_owned(),
            );
        }
        std::thread::sleep(deadline.saturating_duration_since(now));
        clear_controls();
        native
            .redraw(event_loop, application)
            .map_err(|error| error.to_string())?;
        application.drive_editor_frame(Duration::ZERO, false, false);
        deadline_frames += 1;
    }
    let authoring = application
        .current_ui_authoring_frame()
        .ok_or("the UI Prefab was lost")?;
    Ok(MenuObservation {
        initially_presented,
        retired_when_closed,
        reopened_presented: native.canvas.is_current(&native.scene, &authoring),
        document_unchanged: authoring.document() == document && authoring.revision() == revision,
        menu_open_when_idle: control_rect(&checkbox).is_some(),
        input_frames,
        deadline_frames,
    })
}

fn click_menu_control(
    event_loop: &ActiveEventLoop,
    native: &mut NativeImgui,
    application: &mut EditorApplication,
    label: &str,
    frames: &mut usize,
) -> Result<(), String> {
    let rect = control_rect(label)
        .ok_or_else(|| format!("the actual menu control {label:?} was not painted"))?;
    let point = [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ];
    let clip = control_clip(label)
        .ok_or_else(|| format!("the actual menu control {label:?} has no draw clip"))?;
    if !control_visible(label) || !clip.contains(point) {
        return Err(format!(
            "the actual menu control {label:?} is clipped or hidden"
        ));
    }
    for button in [None, Some(true), Some(false)] {
        clear_controls();
        native
            .redraw_with_input(event_loop, application, |context| {
                let viewport = context.main_viewport().id();
                let io = context.io_mut();
                // All mouse input stays in this hidden test Context after native platform polling.
                io.clear_events_queue();
                io.add_focus_event(true);
                io.add_mouse_pos_event(point);
                io.add_mouse_viewport_event(viewport);
                if let Some(button) = button {
                    io.add_mouse_button_event(MouseButton::Left, button);
                }
            })
            .map_err(|error| error.to_string())?;
        application.drive_editor_frame(Duration::ZERO, false, false);
        *frames += 1;
    }
    Ok(())
}

fn observe(
    event_loop: &ActiveEventLoop,
    native: &mut NativeImgui,
    application: &mut EditorApplication,
) -> Result<Observation, String> {
    let frame = application
        .current_frame()
        .ok_or("the startup Scene is unavailable")?;
    let scene = frame
        .workspace()
        .active_scene
        .as_ref()
        .ok_or("the startup Scene is unavailable")?;
    let anchor = SceneEntityId::new("ui-anchor").map_err(|error| error.to_string())?;
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene.document,
                    entity: Some(anchor.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene.document,
                    base_revision: scene.revision,
                    entity: anchor,
                }),
            ],
        ),
        Duration::ZERO,
    );
    let authoring = application
        .current_ui_authoring_frame()
        .ok_or("the UI Prefab is unavailable")?;
    let document = authoring.document();
    let revision = authoring.revision();
    let panel = application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|instance| instance.kind() == "nara.authoring-view")
        .ok_or("the Authoring panel is unavailable")?
        .id();
    let initial = pump_until_idle(event_loop, native, application)?;
    let authoring = application
        .current_ui_authoring_frame()
        .ok_or("the UI Prefab was lost")?;
    let generation = authoring.canvas_generation();
    let initially_presented = native.canvas.is_current(&native.scene, &authoring);
    let displayed_image = native.scene.submitted;
    native
        .canvas
        .promote(&native.scene, std::iter::empty())
        .map_err(|error| error.to_string())?;
    let unconfirmed_pick_granted = native
        .canvas
        .resolve_pick(&native.scene, &authoring, [0.5, 0.5])
        .is_some();
    pump_until_idle(event_loop, native, application)?;
    let redisplayed_with_same_image = native.canvas.is_current(&native.scene, &authoring)
        && native.scene.submitted == displayed_image;
    application.panel_workspace_mut().layout_mut().close(panel);
    let closed = pump_until_idle(event_loop, native, application)?;
    let retired_when_closed = native.scene.submitted.is_none() && native.scene.view.is_none();
    application.panel_workspace_mut().layout_mut().reopen(panel);
    let reopened = pump_until_idle(event_loop, native, application)?;
    let authoring = application
        .current_ui_authoring_frame()
        .ok_or("reopening lost the UI Prefab")?;
    Ok(Observation {
        initially_presented,
        retired_when_closed,
        reopened_presented: native.canvas.is_current(&native.scene, &authoring),
        same_geometry_generation: generation.is_some()
            && authoring.canvas_generation() == generation,
        document_unchanged: authoring.document() == document && authoring.revision() == revision,
        unconfirmed_pick_granted,
        redisplayed_with_same_image,
        frames: [initial, closed, reopened],
    })
}

fn pump_until_idle(
    event_loop: &ActiveEventLoop,
    native: &mut NativeImgui,
    application: &mut EditorApplication,
) -> Result<usize, String> {
    let timeout = Instant::now() + Duration::from_secs(5);
    let mut frames = 0;
    loop {
        native
            .redraw(event_loop, application)
            .map_err(|error| error.to_string())?;
        frames += 1;
        // The ordinary Host drives the owner, then chooses whether another frame has a deadline.
        application.drive_editor_frame(Duration::ZERO, false, false);
        let now = Instant::now();
        let Some(deadline) = imgui_next_deadline(application, Some(native), now) else {
            return Ok(frames);
        };
        if now >= timeout {
            return Err("the Host never returned to idle".to_owned());
        }
        std::thread::sleep(deadline.saturating_duration_since(now));
    }
}
