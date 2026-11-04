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
        self.owned_component_click("Add Component")?;
        self.owned_component_click(&format!("Add {CUSTOM_COMPONENT}"))?;
        let before_range_edit = self.history()?;
        self.owned_test_named_replace("seconds", "seconds", "31")?;
        self.owned_test_named_click("seconds", "Apply")?;
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
        self.owned_test_named_replace("seconds", "seconds", "30")?;
        self.owned_test_apply("seconds")?;
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
        self.owned_test_named_replace("label", "label", "draft ")?;
        let owner = self.owned_test_control("label", None)?;
        let token = self
            .native
            .frontend
            .owned_field_token(owner.panel, &owner.frame, &owner.key)
            .ok_or("the actual field has no native text lifetime")?;
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Preedit("中文".into(), Some((0, 3))))?;
        self.owned_test_text("label", "draft ", true)?;
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
        self.owned_test_paint()?;
        let current = self.owned_test_control("label", None)?;
        if current.key != owner.key
            || self.native.owned_focused_control_for_test()
                != Some((owner.panel, owner.key.clone()))
            || self
                .native
                .frontend
                .owned_field_token(current.panel, &current.frame, &current.key)
                != Some(token)
        {
            return Err(
                "an unrelated revision replaced the active text widget identity".to_owned(),
            );
        }
        self.owned_test_text("label", "draft ", true)?;
        self.owned_test_ime(Ime::Commit("中文".into()))?;
        let retained = self
            .native
            .frontend
            .retained_field_draft(CUSTOM_COMPONENT, "label")
            .ok_or("the committed composition did not reach the retained draft")?;
        if retained.0 != "draft 中文" || !retained.1 {
            return Err(format!("composition or conflict was lost: {retained:?}"));
        }
        self.owned_test_named_click("label", "Reapply")?;
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
