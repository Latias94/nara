use super::{
    ActiveEventLoop, ApplicationHandler, AuthoringWorkflow, CUSTOM_COMPONENT, Duration,
    EditorPersistenceResult, EventLoop, EventLoopBuilderExtWindows, Instant, InteractionProject,
    PathBuf, WindowEvent, WindowId, run_native_workflow,
};
use nara::{identity::EntityReference, reflect::ComponentValue, scene::SceneEntityId};

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_source_local_entity_reference_widgets_save_and_reopen() {
    let project = InteractionProject::new_entity_references();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = EntityReferenceTestApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    assert!(
        application
            .result
            .expect("the entity reference workflow must run")
            .unwrap()
            > 0
    );
}

struct EntityReferenceTestApplication {
    project: PathBuf,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for EntityReferenceTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_entity_reference()
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl AuthoringWorkflow<'_> {
    fn expect_entity_reference(&self, expected: &str) -> Result<(), String> {
        let frame = self.frame()?;
        let actual = frame
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
                    .find(|field| field.id.as_str() == "target")
            })
            .and_then(|field| field.value.as_ref());
        let expected = ComponentValue::EntityReference(EntityReference::SceneLocal {
            entity: SceneEntityId::new(expected).unwrap(),
        });
        if actual != Some(&expected) {
            return Err(format!("expected {expected:?}, observed {actual:?}"));
        }
        Ok(())
    }

    fn run_entity_reference(mut self) -> Result<u64, String> {
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
        self.click("entity:source")?;
        self.expect_entity_reference("target")?;
        let baseline = self.history()?;
        let apply = format!("field-action:{CUSTOM_COMPONENT}:target:Apply");
        self.click("entity-reference:picker")?;
        self.paint(|context| {
            context
                .io_mut()
                .add_key_event(dear_imgui_rs::Key::Escape, true)
        })?;
        self.paint(|context| {
            context
                .io_mut()
                .add_key_event(dear_imgui_rs::Key::Escape, false)
        })?;
        self.expect_entity_reference("target")?;
        if self.history()? != baseline {
            return Err("cancelling target selection changed authoring history".to_owned());
        }
        self.click("entity-reference:picker")?;
        self.click("entity-reference:choice:other")?;
        self.click(&apply)?;
        self.expect_entity_reference("other")?;
        if self.history()? != baseline + 1 {
            return Err("an accepted reference did not create one history entry".to_owned());
        }
        self.click("Undo")?;
        self.expect_entity_reference("target")?;
        self.click("Redo")?;
        self.expect_entity_reference("other")?;
        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Saved { .. })
        ) {
            return Err("Save did not publish the source-local reference".to_owned());
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        self.click("entity:source")?;
        self.expect_entity_reference("other")?;
        Ok(self.presents)
    }
}
