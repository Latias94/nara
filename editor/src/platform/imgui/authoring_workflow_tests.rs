#![cfg(target_os = "windows")]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use dear_imgui_rs::{ConfigFlags, Context, Key, MouseButton};
use nara::{
    reflect::ComponentValue,
    tooling::{
        EditorDocumentKind, EditorFrameModel, EditorPersistenceOperation, EditorPersistenceResult,
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
use crate::{
    EditorApplication, EditorLaunchOptions,
    frontend::imgui::{InteractionProject, clear_controls, control_rect},
};

const CUSTOM_COMPONENT: &str = "studio.interaction.Health";

mod field_drafts;
mod field_overrides;
mod ordinary_structure;
mod recursive_shapes;

struct DocumentObservation {
    kind: EditorDocumentKind,
    values: [Option<i64>; 9],
    history: [usize; 10],
    dirty: [bool; 4],
    saved_value: ComponentValue,
    unavailable_preserved: bool,
    original_entities_preserved: bool,
}

struct AuthoringObservation {
    documents: Vec<DocumentObservation>,
    presents: u64,
    project_code_unchanged: bool,
}

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_ordinary_scene_and_prefab_widgets_save_and_reopen_preserved_source() {
    let project = InteractionProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = AuthoringTestApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    // Every behavior assertion runs after graphics and the ordinary Session have retired.
    let observed = application
        .result
        .expect("the ordinary authoring workflow must run")
        .unwrap();
    assert_eq!(observed.documents.len(), 2);
    for (document, kind) in observed
        .documents
        .into_iter()
        .zip([EditorDocumentKind::Scene, EditorDocumentKind::Prefab])
    {
        assert_eq!(document.kind, kind);
        assert_eq!(
            document.values,
            [
                Some(10),
                Some(42),
                Some(10),
                Some(42),
                None,
                Some(42),
                None,
                Some(42),
                Some(42)
            ]
        );
        assert_eq!(document.history, [1, 2, 3, 2, 3, 4, 3, 4, 3, 0]);
        assert_eq!(document.dirty, [false, true, false, false]);
        assert_eq!(
            document.saved_value,
            ComponentValue::map([("value", ComponentValue::I64(42))])
        );
        assert!(
            document.unavailable_preserved,
            "{kind:?} must retain its unavailable-record manifest"
        );
        assert!(
            document.original_entities_preserved,
            "{kind:?} must retain every original entity record"
        );
    }
    assert!(observed.presents > 0);
    assert!(observed.project_code_unchanged);
}

struct AuthoringTestApplication {
    project: PathBuf,
    result: Option<Result<AuthoringObservation, String>>,
}

impl ApplicationHandler for AuthoringTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_authoring(event_loop, &self.project));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn run_authoring(
    event_loop: &ActiveEventLoop,
    project: &Path,
) -> Result<AuthoringObservation, String> {
    run_native_workflow(event_loop, project, |workflow| workflow.run(project))
}

fn run_native_workflow<T>(
    event_loop: &ActiveEventLoop,
    project: &Path,
    run: impl FnOnce(AuthoringWorkflow<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Nara ordinary authoring widget workflow")
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
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(project).with_profile("desktop"));
    application.native_shell_ready();
    let result = run(AuthoringWorkflow {
        native: &mut native,
        application: &mut application,
        event_loop,
        pointer: [-1000.0, -1000.0],
        presents: 0,
    });
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

struct AuthoringWorkflow<'a> {
    native: &'a mut NativeImgui,
    application: &'a mut EditorApplication,
    event_loop: &'a ActiveEventLoop,
    pointer: [f32; 2],
    presents: u64,
}

impl AuthoringWorkflow<'_> {
    fn paint(&mut self, input: impl FnOnce(&mut Context)) -> Result<(), String> {
        self.application.advance_project_open();
        self.application
            .drive_editor_frame(Duration::ZERO, false, false);
        clear_controls();
        let pointer = self.pointer;
        let exited = self
            .native
            .redraw_with_input(self.event_loop, self.application, |context| {
                let viewport = context.main_viewport().id();
                let io = context.io_mut();
                // Replace only queued test-window input after native preparation. Keep held state
                // inside Dear ImGui and never activate a native window or move the system pointer.
                io.clear_events_queue();
                io.add_focus_event(true);
                io.add_mouse_pos_event(pointer);
                io.add_mouse_viewport_event(viewport);
                input(context);
            })
            .map_err(|error| error.to_string())?;
        if exited {
            return Err("an authoring widget unexpectedly requested application exit".to_owned());
        }
        self.presents += self.native.gpu.frame_transaction_stats().presents();
        Ok(())
    }

    fn frame(&self) -> Result<Arc<EditorFrameModel>, String> {
        self.application
            .current_frame()
            .ok_or_else(|| "ordinary project Open did not publish an Editor frame".to_owned())
    }

    fn click(&mut self, label: &str) -> Result<(), String> {
        let rect = control_rect(label)
            .ok_or_else(|| format!("the real widget {label:?} was not painted"))?;
        self.pointer = [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ];
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

    fn replace_integer(&mut self) -> Result<(), String> {
        self.replace_text(&format!("field:{CUSTOM_COMPONENT}:value"), "42")?;
        self.paint(|context| context.io_mut().add_key_event(Key::Enter, true))?;
        self.paint(|context| context.io_mut().add_key_event(Key::Enter, false))?;
        for _ in 0..3 {
            self.paint(|_| {})?;
        }
        Ok(())
    }

    fn replace_text(&mut self, label: &str, value: &str) -> Result<(), String> {
        self.click(label)?;
        self.paint(|context| {
            context.io_mut().add_key_event(Key::ModCtrl, true);
            context.io_mut().add_key_event(Key::A, true);
        })?;
        self.paint(|context| {
            context.io_mut().add_key_event(Key::A, false);
            context.io_mut().add_key_event(Key::ModCtrl, false);
        })?;
        self.paint(|context| context.io_mut().add_input_characters_utf8(value))?;
        Ok(())
    }

    fn selected_value(&self) -> Result<Option<i64>, String> {
        let frame = self.frame()?;
        let value = frame
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.editor.inspector.selected_entity_view.as_ref())
            .and_then(|entity| {
                entity
                    .components
                    .iter()
                    .find(|component| component.component.as_str() == CUSTOM_COMPONENT)
            })
            .and_then(|component| {
                component
                    .fields
                    .iter()
                    .find(|field| field.id.as_str() == "value")
            })
            .and_then(|field| field.value.as_ref());
        match value {
            Some(ComponentValue::I64(value)) => Ok(Some(*value)),
            Some(value) => Err(format!(
                "the custom field has an unexpected typed value: {value:?}"
            )),
            None => Ok(None),
        }
    }

    fn history(&self) -> Result<usize, String> {
        self.frame()?
            .workspace()
            .active_scene
            .as_ref()
            .map(|scene| scene.editor.inspector.history.undo_depth)
            .ok_or_else(|| "the active source has no history projection".to_owned())
    }

    fn dirty(&self) -> Result<bool, String> {
        let frame = self.frame()?;
        let document = frame
            .workspace()
            .active_document
            .ok_or("there is no active document")?;
        frame
            .workspace()
            .document(document)
            .map(|tab| tab.dirty)
            .ok_or_else(|| "the active document has no dirty projection".to_owned())
    }

    fn settle_persistence(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            self.paint(|_| {})?;
            if self.frame()?.persistence().operation() == EditorPersistenceOperation::Idle {
                return Ok(());
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        Err("the ordinary widget-requested persistence operation did not complete".to_owned())
    }

    fn document(
        &mut self,
        project: &Path,
        kind: EditorDocumentKind,
    ) -> Result<DocumentObservation, String> {
        let source = project.join(match kind {
            EditorDocumentKind::Scene => "worlds/entry.scene.json",
            EditorDocumentKind::Prefab => "blueprints/ordinary.prefab.json",
        });
        let original = read_source(&source)?;
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the source has no authoring model")?;
        if scene.kind != kind || !scene.editor.inspector.composition.generic_authoring {
            return Err(format!(
                "the ordinary {kind:?} generic authoring model is not active"
            ));
        }
        let document = scene.document;
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the ordinary source has no unavailable-record manifest")?;
        let clean = self.dirty()?;
        self.click("+ Root")?;
        let frame = self.frame()?;
        let entity = frame
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.selection.top_entity())
            .cloned()
            .ok_or("the real create widget did not select its entity")?;
        let created_history = self.history()?;
        self.click("Add Component")?;
        self.click(CUSTOM_COMPONENT)?;
        let initial = self.selected_value()?;
        let added_history = self.history()?;
        self.replace_integer()?;
        let edited = self.selected_value()?;
        let edited_history = self.history()?;
        self.click("Undo")?;
        let undo_edit = self.selected_value()?;
        let undo_edit_history = self.history()?;
        self.click("Redo")?;
        let redo_edit = self.selected_value()?;
        let redo_edit_history = self.history()?;
        self.click("Remove Component")?;
        let removed = self.selected_value()?;
        let removed_history = self.history()?;
        self.click("Undo")?;
        let undo_remove = self.selected_value()?;
        let undo_remove_history = self.history()?;
        self.click("Redo")?;
        let redo_remove = self.selected_value()?;
        let redo_remove_history = self.history()?;
        self.click("Undo")?;
        let retained = self.selected_value()?;
        let retained_history = self.history()?;
        let dirty = self.dirty()?;
        let manifest_after_edit = self
            .application
            .active_authoring_unavailable_record_manifest()
            == Some(manifest.clone());

        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(self.frame()?.persistence().result(), Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
        {
            return Err(format!(
                "the real Save widget did not save the {kind:?} source"
            ));
        }
        let saved_dirty = self.dirty()?;
        let stored = read_source(&source)?;
        let entities = stored["payload"]["entities"]
            .as_array()
            .ok_or("the saved source has no entity records")?;
        let created = entities
            .iter()
            .find(|record| record["id"].as_str() == Some(entity.as_str()))
            .ok_or("the created entity was not saved")?;
        let saved_value =
            serde_json::from_value(created["components"][CUSTOM_COMPONENT]["value"].clone())
                .map_err(|error| error.to_string())?;
        let retained_entities = entities
            .iter()
            .filter(|record| record["id"].as_str() != Some(entity.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let original_entities_preserved =
            original["payload"]["entities"].as_array() == Some(&retained_entities);
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) {
            return Err(format!(
                "the real Reopen widget did not reopen the {kind:?} source"
            ));
        }
        self.click(&format!("entity:{}", entity.as_str()))?;
        let reopened = self.selected_value()?;
        let reopen_history = self.history()?;
        let reopened_dirty = self.dirty()?;
        let unavailable_preserved = manifest_after_edit
            && self
                .application
                .active_authoring_unavailable_record_manifest()
                == Some(manifest);
        self.click("Dismiss persistence result")?;
        Ok(DocumentObservation {
            kind,
            values: [
                initial,
                edited,
                undo_edit,
                redo_edit,
                removed,
                undo_remove,
                redo_remove,
                retained,
                reopened,
            ],
            history: [
                created_history,
                added_history,
                edited_history,
                undo_edit_history,
                redo_edit_history,
                removed_history,
                undo_remove_history,
                redo_remove_history,
                retained_history,
                reopen_history,
            ],
            dirty: [clean, dirty, saved_dirty, reopened_dirty],
            saved_value,
            unavailable_preserved,
            original_entities_preserved,
        })
    }

    fn run(mut self, project: &Path) -> Result<AuthoringObservation, String> {
        let project_code =
            fs::read(project.join("src/lib.rs")).map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            self.paint(|_| {})?;
            if self.application.current_frame().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        self.frame()?;
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        let scene = self.document(project, EditorDocumentKind::Scene)?;
        self.click("entity:prefab-anchor")?;
        self.click("Open Prefab Source")?;
        let prefab = self.document(project, EditorDocumentKind::Prefab)?;
        Ok(AuthoringObservation {
            documents: vec![scene, prefab],
            presents: self.presents,
            project_code_unchanged: project_code
                == fs::read(project.join("src/lib.rs")).map_err(|error| error.to_string())?
                && String::from_utf8_lossy(&project_code).contains("compile_error!"),
        })
    }
}

fn read_source(path: &Path) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}
