#![cfg(target_os = "windows")]

use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

use dear_imgui_rs::{ConfigFlags, Context, Key, MouseButton};
use nara::tooling::{EditorDocumentKind, EditorFrameModel, EditorWorkspaceIntentPhase};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::{Window, WindowId},
};

use super::{NativeImgui, canvas_tests::retire_application};
use crate::{
    EditorApplication, EditorApplicationState, EditorLaunchOptions,
    app::EditorProjectShellModel,
    frontend::imgui::{
        InteractionProject, clear_controls, control_clip, control_rect, control_visible,
    },
};

struct ManagerObservation {
    initial_cancelled: bool,
    recovery_diagnostic_complete: bool,
    replacement_cancel_preserved: bool,
    replacement_rejection_preserved: bool,
    close_cancel_preserved: bool,
    discarded_source_unchanged: bool,
    saved_entity_present: bool,
    saved_original_entities_preserved: bool,
    replacement_source_unchanged: bool,
    project_code_unchanged: bool,
    presents: u64,
    final_state: EditorApplicationState,
}

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_manager_widgets_recover_cancel_replacement_and_resolve_dirty_project_close() {
    let project = InteractionProject::new();
    let replacement = InteractionProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = ManagerTestApplication {
        project: &project,
        replacement: &replacement,
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    // Native graphics and the real ordinary Session retire before a behavior assertion can panic.
    let observed = application
        .result
        .expect("the native Manager workflow must run")
        .unwrap();
    assert!(observed.initial_cancelled);
    assert!(observed.recovery_diagnostic_complete);
    assert!(observed.replacement_cancel_preserved);
    assert!(observed.replacement_rejection_preserved);
    assert!(observed.close_cancel_preserved);
    assert!(observed.discarded_source_unchanged);
    assert!(observed.saved_entity_present);
    assert!(observed.saved_original_entities_preserved);
    assert!(observed.replacement_source_unchanged);
    assert!(observed.project_code_unchanged);
    assert!(observed.presents > 0);
    assert_eq!(observed.final_state, EditorApplicationState::ProjectManager);
}

struct ManagerTestApplication<'a> {
    project: &'a InteractionProject,
    replacement: &'a InteractionProject,
    result: Option<Result<ManagerObservation, String>>,
}

impl ApplicationHandler for ManagerTestApplication<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_manager(event_loop, self.project, self.replacement));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn run_manager(
    event_loop: &ActiveEventLoop,
    project: &InteractionProject,
    replacement: &InteractionProject,
) -> Result<ManagerObservation, String> {
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Nara native Manager widget workflow")
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
    let mut application = EditorApplication::new(EditorLaunchOptions::project_manager());
    let result = ManagerWorkflow {
        native: &mut native,
        application: &mut application,
        event_loop,
        pointer: [-1000.0, -1000.0],
        presents: 0,
        deadline: Instant::now() + Duration::from_secs(40),
    }
    .run(project, replacement);
    let graphics = native
        .shutdown_graphics()
        .map_err(|error| error.to_string());
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

struct ManagerWorkflow<'a> {
    native: &'a mut NativeImgui,
    application: &'a mut EditorApplication,
    event_loop: &'a ActiveEventLoop,
    pointer: [f32; 2],
    presents: u64,
    deadline: Instant,
}

impl ManagerWorkflow<'_> {
    fn paint(&mut self, input: impl FnOnce(&mut Context)) -> Result<(), String> {
        if Instant::now() >= self.deadline {
            return Err(format!(
                "the native Manager workflow timed out in {:?}",
                self.application.state()
            ));
        }
        // Opening is advanced explicitly between presented frames. In particular, the user can
        // operate Cancel on a real Opening frame before the next bounded filesystem phase runs.
        self.application
            .drive_editor_frame(Duration::ZERO, false, false);
        clear_controls();
        let pointer = self.pointer;
        let exited = self
            .native
            .redraw_with_input(self.event_loop, self.application, |context| {
                let viewport = context.main_viewport().id();
                let io = context.io_mut();
                // Synthetic input is local to this hidden Context after Winit polling. Neither the
                // system pointer, global keyboard input nor native foreground ownership is changed.
                io.clear_events_queue();
                io.add_focus_event(true);
                io.add_mouse_pos_event(pointer);
                io.add_mouse_viewport_event(viewport);
                input(context);
            })
            .map_err(|error| error.to_string())?;
        if exited {
            return Err(
                "a Manager workflow control unexpectedly exited the application".to_owned(),
            );
        }
        let stats = self.native.gpu.frame_transaction_stats();
        let primary_presents = stats.surface_acquires().min(stats.presents());
        if !self.native.presented || primary_presents == 0 {
            return Err(format!(
                "the {:?} widget frame was not presented by the primary GPU",
                self.application.state()
            ));
        }
        self.presents += primary_presents;
        Ok(())
    }

    fn frame(&self) -> Result<Arc<EditorFrameModel>, String> {
        self.application.current_frame().ok_or_else(|| {
            format!(
                "the Manager workflow lost its workspace in {:?}",
                self.application.state(),
            )
        })
    }

    fn click(&mut self, label: &str) -> Result<(), String> {
        let mut revealed = false;
        for attempt in 0..24 {
            let rect = control_rect(label)
                .ok_or_else(|| format!("the real Manager widget {label:?} was not painted"))?;
            let clip = control_clip(label)
                .ok_or_else(|| format!("the real Manager widget {label:?} has no draw clip"))?;
            let center = [
                (rect.min[0] + rect.max[0]) * 0.5,
                (rect.min[1] + rect.max[1]) * 0.5,
            ];
            if control_visible(label) && clip.contains(center) {
                self.pointer = center;
                revealed = true;
                break;
            }
            if attempt == 0 {
                eprintln!(
                    "revealing actual Manager widget {label:?}: rect={rect:?}; clip={clip:?}; visible={}; {}",
                    control_visible(label),
                    self.state_detail()
                );
            }
            if rect.max[0] <= clip.min[0] || rect.min[0] >= clip.max[0] {
                return Err(format!(
                    "the real Manager widget {label:?} is horizontally clipped; rect={rect:?}; clip={clip:?}; {}",
                    self.state_detail()
                ));
            }
            self.pointer = [
                (clip.min[0] + clip.max[0]) * 0.5,
                (clip.min[1] + clip.max[1]) * 0.5,
            ];
            self.paint(|_| {})?;
            let wheel = if center[1] >= clip.max[1] { -3.0 } else { 3.0 };
            self.paint(|context| context.io_mut().add_mouse_wheel_event([0.0, wheel]))?;
            for _ in 0..2 {
                self.paint(|_| {})?;
            }
        }
        if !revealed {
            return Err(format!(
                "the real Manager widget {label:?} remained clipped; rect={:?}; clip={:?}; {}",
                control_rect(label),
                control_clip(label),
                self.state_detail()
            ));
        }
        self.paint(|_| {})?;
        // Re-read after hover/layout so the press uses the current visible widget position.
        let rect = control_rect(label)
            .ok_or_else(|| format!("the real Manager widget {label:?} disappeared before press"))?;
        self.pointer = [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ];
        if !control_visible(label)
            || !control_clip(label).is_some_and(|clip| clip.contains(self.pointer))
        {
            return Err(format!(
                "the real Manager widget {label:?} moved outside its clip before press; {}",
                self.state_detail()
            ));
        }
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

    fn replace_text(&mut self, label: &str, text: &str) -> Result<(), String> {
        self.click(label)?;
        self.paint(|context| {
            context.io_mut().add_key_event(Key::ModCtrl, true);
            context.io_mut().add_key_event(Key::A, true);
        })?;
        self.paint(|context| {
            context.io_mut().add_key_event(Key::A, false);
            context.io_mut().add_key_event(Key::ModCtrl, false);
        })?;
        self.paint(|context| context.io_mut().add_input_characters_utf8(text))
    }

    fn wait_for(&mut self, state: EditorApplicationState) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            self.application.advance_project_open();
            self.paint(|_| {})?;
            if self.application.state() == state {
                for _ in 0..3 {
                    self.paint(|_| {})?;
                }
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(format!(
            "the widget workflow did not reach {state:?}; actual={:?}",
            self.application.state()
        ))
    }

    fn wait_for_workspace(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            self.application.advance_project_open();
            self.paint(|_| {})?;
            if matches!(
                self.application.state(),
                EditorApplicationState::AuthoringReady | EditorApplicationState::AuthoringDegraded
            ) {
                for _ in 0..3 {
                    self.paint(|_| {})?;
                }
                return Ok(());
            }
            if self.application.state() == EditorApplicationState::Recovery {
                return Err(format!(
                    "the widget-requested project open was rejected; {}",
                    self.state_detail()
                ));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err(format!(
            "the repaired source did not open; {}",
            self.state_detail()
        ))
    }

    fn state_detail(&self) -> String {
        match self.application.project_shell_model() {
            Some(EditorProjectShellModel::Recovery {
                attempt_generation,
                request,
                diagnostics,
                ..
            }) => format!(
                "state=Recovery; attempt={attempt_generation}; path={:?}; profile={:?}; diagnostics={diagnostics:?}",
                request.project_root(),
                request.owned_profile(),
            ),
            Some(EditorProjectShellModel::Opening {
                attempt_generation,
                phase,
                summary,
                replacement,
                ..
            }) => format!(
                "state={:?}; attempt={attempt_generation}; replacement={replacement}; phase={phase}; summary={summary}",
                self.application.state(),
            ),
            _ => format!("state={:?}", self.application.state()),
        }
    }

    fn require_state(&self, expected: EditorApplicationState) -> Result<(), String> {
        if self.application.state() == expected {
            Ok(())
        } else {
            Err(format!(
                "the actual widget did not reach {expected:?}; actual={:?}",
                self.application.state()
            ))
        }
    }

    fn require_decision(&self) -> Result<(), String> {
        if self.frame()?.workspace_intent().phase()
            == Some(EditorWorkspaceIntentPhase::AwaitingDecision)
        {
            Ok(())
        } else {
            Err("Close Project did not publish an actual unsaved-changes decision".to_owned())
        }
    }

    fn run(
        &mut self,
        project: &InteractionProject,
        replacement: &InteractionProject,
    ) -> Result<ManagerObservation, String> {
        let source = project.source(EditorDocumentKind::Scene);
        let original = fs::read(&source).map_err(|error| error.to_string())?;
        let replacement_source = replacement.source(EditorDocumentKind::Scene);
        let replacement_original =
            fs::read(&replacement_source).map_err(|error| error.to_string())?;
        let bundle_path = project.root.join("catalogs/authoring.bundle.json");
        let bundle = fs::read(&bundle_path).map_err(|error| error.to_string())?;
        fs::write(&bundle_path, b"invalid authoring bundle").map_err(|error| error.to_string())?;
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        self.replace_text(
            "Project path",
            project
                .root
                .to_str()
                .ok_or("the fixture path is not UTF-8")?,
        )?;
        self.replace_text("Profile", "desktop")?;
        self.click("shell:Open Project")?;
        self.require_state(EditorApplicationState::OpeningInitial)?;
        self.click("shell:Cancel")?;
        let initial_cancelled = self.application.state() == EditorApplicationState::ProjectManager;
        self.require_state(EditorApplicationState::ProjectManager)?;
        self.click("shell:Open Project")?;
        self.wait_for(EditorApplicationState::Recovery)?;
        let recovery_diagnostic_complete = match self.application.project_shell_model() {
            Some(EditorProjectShellModel::Recovery { diagnostics, .. }) => {
                !diagnostics.is_empty()
                    && diagnostics.iter().all(|diagnostic| {
                        !diagnostic.code.is_empty()
                            && !diagnostic.summary.is_empty()
                            && !diagnostic.impact.is_empty()
                            && !diagnostic.repair_hint.is_empty()
                    })
            }
            _ => false,
        };
        fs::write(&bundle_path, bundle).map_err(|error| error.to_string())?;
        self.click("shell:Retry")?;
        self.require_state(EditorApplicationState::OpeningInitial)
            .map_err(|error| {
                format!(
                    "the actual Retry click did not start a new attempt: {error}; {}",
                    self.state_detail()
                )
            })?;
        self.wait_for_workspace()?;
        self.click("+ Root")?;
        let dirty = self.frame()?.workspace().clone();
        if !dirty.documents.iter().any(|document| document.dirty) {
            return Err("the actual Create widget did not produce a dirty source".to_owned());
        }
        self.click("shell:Open Project...")?;
        self.replace_text(
            "Project path",
            replacement
                .root
                .to_str()
                .ok_or("the replacement path is not UTF-8")?,
        )?;
        self.click("shell:Open Project")?;
        self.require_state(EditorApplicationState::OpeningReplacement)?;
        self.click("shell:Cancel")?;
        let replacement_cancel_preserved = self.frame()?.workspace() == &dirty;

        // The rejected replacement must leave the accepted dirty source intact, just as Cancel.
        fs::write(
            replacement.root.join("catalogs/authoring.bundle.json"),
            b"invalid replacement bundle",
        )
        .map_err(|error| error.to_string())?;
        self.click("shell:Open Project...")?;
        self.click("shell:Open Project")?;
        self.require_state(EditorApplicationState::OpeningReplacement)?;
        self.wait_for_workspace()?;
        let replacement_rejection_preserved = self.frame()?.workspace() == &dirty;
        self.click("shell:Close Project")?;
        self.require_decision()?;
        self.click("close:Cancel")?;
        let close_cancel_preserved = self.frame()?.workspace() == &dirty;
        self.click("shell:Close Project")?;
        self.require_decision()?;
        self.click("close:Discard and continue")?;
        self.wait_for(EditorApplicationState::ProjectManager)?;
        let discarded_source_unchanged =
            fs::read(&source).map_err(|error| error.to_string())? == original;

        self.replace_text(
            "Project path",
            project
                .root
                .to_str()
                .ok_or("the fixture path is not UTF-8")?,
        )?;
        self.replace_text("Profile", "desktop")?;
        self.click("shell:Open Project")?;
        self.wait_for_workspace()?;
        self.click("+ Root")?;
        let saved_entity = self
            .frame()?
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.selection.top_entity())
            .cloned()
            .ok_or("the Save decision has no actual created entity")?;
        self.click("shell:Close Project")?;
        self.require_decision()?;
        self.click("close:Save and continue")?;
        self.wait_for(EditorApplicationState::ProjectManager)?;
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(&source).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        let saved_entities = saved["payload"]["entities"]
            .as_array()
            .ok_or("the saved Scene has no entities")?;
        let saved_entity_present = saved_entities
            .iter()
            .any(|entity| entity["id"].as_str() == Some(saved_entity.as_str()));
        let retained = saved_entities
            .iter()
            .filter(|entity| entity["id"].as_str() != Some(saved_entity.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let before: serde_json::Value =
            serde_json::from_slice(&original).map_err(|error| error.to_string())?;
        Ok(ManagerObservation {
            initial_cancelled,
            recovery_diagnostic_complete,
            replacement_cancel_preserved,
            replacement_rejection_preserved,
            close_cancel_preserved,
            discarded_source_unchanged,
            saved_entity_present,
            saved_original_entities_preserved: before["payload"]["entities"].as_array()
                == Some(&retained),
            replacement_source_unchanged: fs::read(replacement_source)
                .map_err(|error| error.to_string())?
                == replacement_original,
            project_code_unchanged: fs::read_to_string(project.root.join("src/lib.rs"))
                .map_err(|error| error.to_string())?
                .contains("compile_error!"),
            presents: self.presents,
            final_state: self.application.state(),
        })
    }
}
