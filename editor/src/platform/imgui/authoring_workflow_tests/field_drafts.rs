use super::*;
use nara::{
    reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId},
    tooling::{EditorAction, EditorActionBatch, EditorDocumentMutation, SceneInspectorCommand},
};
use winit::event::Ime;

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_field_draft_focus_and_ime_survive_unrelated_document_revision() {
    let project = InteractionProject::new_field_drafts();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = DraftTestApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    assert!(
        application
            .result
            .expect("the native field draft workflow must run")
            .unwrap()
            > 0
    );
}

struct DraftTestApplication {
    project: PathBuf,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for DraftTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_field_drafts()
            }));
        }
        event_loop.exit();
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl AuthoringWorkflow<'_> {
    fn draft_text_owner(&self) -> Result<u32, String> {
        self.native.context.binding().with_bound_context(|| {
            // SAFETY: the live native Context is bound to this UI thread. Only identity values
            // are copied; no pointer or mutable toolkit authority escapes this check.
            unsafe {
                let active = dear_imgui_rs::sys::igGetActiveID();
                let state = dear_imgui_rs::sys::igGetInputTextState(active);
                if active == 0 || state.is_null() || (*state).ID != active {
                    return Err("the real text input lost its active editing state".to_owned());
                }
                if self.native.input.text_owner_item() != Some(active) {
                    return Err("native text routing lost the active field owner".to_owned());
                }
                Ok(active)
            }
        })
    }

    fn send_draft_ime(&mut self, ime: Ime) -> Result<(), String> {
        let event = WindowEvent::Ime(ime);
        let window = self.native.window.id();
        if !self
            .native
            .input
            .handle_event(window, &event, &self.native.context)
        {
            return Err("the native input adapter filtered an owned IME event".to_owned());
        }
        self.native
            .platform
            .handle_window_event(&mut self.native.context, window, &event)
            .map_err(|error| error.to_string())?;
        // Keep the commit queued by the actual Winit backend. The ordinary synthetic-input
        // helper intentionally clears its queue, which would erase this native event.
        self.application.advance_project_open();
        self.application
            .drive_editor_frame(Duration::ZERO, false, false);
        clear_controls();
        let pointer = self.pointer;
        let exited = self
            .native
            .redraw_with_input(self.event_loop, self.application, |context| {
                let viewport = context.main_viewport().id();
                context.io_mut().add_focus_event(true);
                context.io_mut().add_mouse_pos_event(pointer);
                context.io_mut().add_mouse_viewport_event(viewport);
            })
            .map_err(|error| error.to_string())?;
        if exited {
            return Err("native IME unexpectedly requested application exit".to_owned());
        }
        self.presents += self.native.gpu.frame_transaction_stats().presents();
        Ok(())
    }

    fn run_field_drafts(mut self) -> Result<u64, String> {
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
        self.click("+ Root")?;
        self.click("Add Component")?;
        self.click(CUSTOM_COMPONENT)?;
        let before_range_edit = self.history()?;
        self.replace_text(&format!("field:{CUSTOM_COMPONENT}:seconds"), "31")?;
        self.click(&format!("field-action:{CUSTOM_COMPONENT}:seconds:Apply"))?;
        if self.history()? != before_range_edit
            || self
                .native
                .frontend
                .retained_field_draft(CUSTOM_COMPONENT, "seconds")
                .is_none_or(|draft| draft.0 != "31")
            || self
                .native
                .frontend
                .retained_field_error(CUSTOM_COMPONENT, "seconds")
                .is_none()
        {
            return Err("native range rejection did not preserve the input and history".to_owned());
        }
        self.replace_text(&format!("field:{CUSTOM_COMPONENT}:seconds"), "30")?;
        self.click(&format!("field-action:{CUSTOM_COMPONENT}:seconds:Apply"))?;
        if self.history()? != before_range_edit + 1
            || self
                .native
                .frontend
                .retained_field_draft(CUSTOM_COMPONENT, "seconds")
                .is_some()
        {
            return Err(
                "native range correction did not receive its exact accepted result".to_owned(),
            );
        }
        self.replace_text(&format!("field:{CUSTOM_COMPONENT}:label"), "draft ")?;
        let owner = self.draft_text_owner()?;
        self.send_draft_ime(Ime::Enabled)?;
        self.send_draft_ime(Ime::Preedit("中文".into(), Some((0, 3))))?;
        if self
            .native
            .context
            .ime_preedit()
            .as_ref()
            .map(|preedit| preedit.text())
            != Some("中文")
        {
            return Err(
                "the native backend did not retain composition on the actual field".to_owned(),
            );
        }
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("no document")?;
        let history = scene.editor.inspector.history.undo_depth;
        self.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Mutate(
                    EditorDocumentMutation::ApplyInspectorCommand {
                        document: scene.document,
                        base_revision: scene.revision,
                        command: SceneInspectorCommand::SetField {
                            entity: scene.selection.top_entity().ok_or("no entity")?.clone(),
                            component: ComponentTypeId::new(CUSTOM_COMPONENT),
                            component_version: ComponentSchemaVersion::ONE,
                            field: ComponentFieldId::new("other"),
                            value: ComponentValue::I64(7),
                        },
                    },
                )],
            ),
            Duration::ZERO,
        );
        self.paint(|_| {})?;
        if self.draft_text_owner()? != owner {
            return Err(
                "an unrelated revision replaced the active text widget identity".to_owned(),
            );
        }
        if self
            .native
            .context
            .ime_preedit()
            .as_ref()
            .map(|preedit| preedit.text())
            != Some("中文")
        {
            return Err("the document revision discarded native IME composition".to_owned());
        }
        self.send_draft_ime(Ime::Commit("中文".into()))?;
        let retained = self
            .native
            .frontend
            .retained_field_draft(CUSTOM_COMPONENT, "label")
            .ok_or("the committed composition did not reach the retained draft")?;
        if retained.0 != "draft 中文" || !retained.1 {
            return Err(format!("composition or conflict was lost: {retained:?}"));
        }
        self.click(&format!(
            "field-action:{CUSTOM_COMPONENT}:label:Reapply draft"
        ))?;
        if self
            .native
            .frontend
            .retained_field_draft(CUSTOM_COMPONENT, "label")
            .is_some()
        {
            return Err("the exact accepted reapply did not clear its private draft".to_owned());
        }
        if self.history()? != history + 2 {
            return Err("IME editing bypassed ordinary document history".to_owned());
        }
        Ok(self.presents)
    }
}
