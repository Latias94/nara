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
        owned: false,
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

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_owned_inspector_entity_reference_picker_apply_undo_redo_save_and_reopen() {
    let project = InteractionProject::new_entity_references();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = EntityReferenceTestApplication {
        project: project.root.clone(),
        owned: true,
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    assert!(
        application
            .result
            .expect("the Nara-owned entity reference workflow must run")
            .unwrap()
            > 0
    );
}

struct EntityReferenceTestApplication {
    project: PathBuf,
    owned: bool,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for EntityReferenceTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                if self.owned {
                    workflow.run_owned_entity_reference()
                } else {
                    workflow.run_entity_reference()
                }
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl AuthoringWorkflow<'_> {
    fn click_owned_reference(&mut self, action: &str) -> Result<(), String> {
        self.owned_test_named_click("target", action)
    }

    fn run_owned_entity_reference(mut self) -> Result<u64, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            self.paint(|_| {})?;
            if self.application.current_frame().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        self.frame()?;
        self.owned_test_paint()?;
        self.click("entity:source")?;
        self.expect_entity_reference("target")?;
        let original = self.frame()?;
        let scene = original
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the reference fixture has no active Scene")?;
        let document = scene.document;
        let baseline = self.history()?;
        self.owned_test_paint()?;
        self.owned_test_control("target", None)?;
        self.owned_test_control("target", Some("Choose target"))?;
        self.click_owned_reference("Choose target")?;
        let first_choice = self.owned_test_control("target", Some("other [other]"))?;
        self.click_owned_reference("Hide targets")?;
        self.owned_test_control("target", Some("Choose target"))?;
        self.owned_test_draft("target", None)?;
        if self.frame()?.workspace().active_scene != original.workspace().active_scene {
            return Err("cancelling the owned picker changed source or authoring history".into());
        }
        self.click_owned_reference("Choose target")?;
        let replacement = self.owned_test_control("target", Some("other [other]"))?;
        if first_choice.key == replacement.key {
            return Err("reopening the owned picker reused a retired choice key".into());
        }
        self.click_owned_reference("other [other]")?;
        self.owned_test_draft("target", Some("scene-local:other"))?;
        self.owned_test_control("target", Some("Choose target"))?;
        if self.frame()?.workspace().active_scene != original.workspace().active_scene {
            return Err("choosing an owned reference published before explicit Apply".into());
        }
        self.owned_test_apply("target")?;
        self.expect_entity_reference("other")?;
        if self.history()? != baseline + 1 || !self.dirty()? {
            return Err("owned reference Apply did not create one dirty history entry".into());
        }
        self.click("Undo")?;
        self.expect_entity_reference("target")?;
        if self.history()? != baseline {
            return Err("ordinary Undo did not restore the original reference history".into());
        }
        self.click("Redo")?;
        self.expect_entity_reference("other")?;
        if self.history()? != baseline + 1 {
            return Err("ordinary Redo did not restore the owned reference edit".into());
        }
        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Saved { document: saved, .. }) if saved == document
        ) || self.dirty()?
        {
            return Err("Save did not persist the owned reference in its original document".into());
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        self.click("entity:source")?;
        self.expect_entity_reference("other")?;
        if self.history()? != 0 || self.dirty()? {
            return Err("Reopen did not restore a clean source with fresh history".into());
        }
        Ok(self.presents)
    }

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
        self.click_owned_reference("Choose target")?;
        self.click_owned_reference("Hide targets")?;
        self.expect_entity_reference("target")?;
        if self.history()? != baseline {
            return Err("cancelling target selection changed authoring history".to_owned());
        }
        self.click_owned_reference("Choose target")?;
        self.click_owned_reference("other [other]")?;
        self.owned_test_apply("target")?;
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
