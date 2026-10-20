use super::*;
use nara::reflect::{ComponentSchemaVersion, ComponentTypeId, PersistentComponentSchemaOwnerId};
use nara::scene::{SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord};

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_ordinary_structure_widgets_arrange_delete_undo_save_and_reopen() {
    let project = InteractionProject::new();
    let parent = SceneEntityRecord::new(SceneEntityId::new("parent").unwrap()).with_component(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("value", ComponentValue::I64(10))]),
        )
        .with_owner(
            PersistentComponentSchemaOwnerId::try_new("studio.interaction.components").unwrap(),
        ),
    );
    fs::write(
        project.source(EditorDocumentKind::Scene),
        SceneDocument::from_ordered_roots([parent])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut test = StructureApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut test).unwrap();
    let presents = test
        .result
        .expect("the structural native workflow must run")
        .unwrap();
    assert!(presents > 0);
}

struct StructureApplication {
    project: PathBuf,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for StructureApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(
                event_loop,
                &self.project,
                |mut workflow| {
                    let deadline = Instant::now() + Duration::from_secs(10);
                    while workflow.application.current_frame().is_none()
                        && Instant::now() < deadline
                    {
                        workflow.paint(|_| {})?;
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    workflow.frame()?;
                    for _ in 0..4 {
                        workflow.paint(|_| {})?;
                    }
                    workflow.structure()
                },
            ));
        }
        event_loop.exit();
    }
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl AuthoringWorkflow<'_> {
    fn structural_tree(&self) -> Result<Vec<(String, Option<String>)>, String> {
        let frame = self.frame()?;
        Ok(frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("missing structural model")?
            .editor
            .inspector
            .entities
            .iter()
            .map(|row| {
                (
                    row.id.as_str().to_owned(),
                    row.parent.as_ref().map(|parent| parent.as_str().to_owned()),
                )
            })
            .collect())
    }

    fn selected_id(&self) -> Result<String, String> {
        self.frame()?
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.selection.top_entity())
            .map(|id| id.as_str().to_owned())
            .ok_or_else(|| "the real widget did not select its created entity".into())
    }

    fn structure(&mut self) -> Result<u64, String> {
        if self
            .application
            .active_authoring_unavailable_record_manifest()
            .is_none_or(|manifest| manifest.total_count() != 0)
        {
            return Err(
                "the deletion fixture must be Complete and contain no Prefab anchors".into(),
            );
        }
        self.click("+ Root")?;
        let parent = self.selected_id()?;
        self.click("+ Child")?;
        let child = self.selected_id()?;
        self.click("+ Root")?;
        let last = self.selected_id()?;
        let before_order = self.structural_tree()?;
        self.click("Up")?;
        if self.structural_tree()? == before_order {
            return Err("Up did not reorder the real root".into());
        }
        self.click("Down")?;
        if self.structural_tree()? != before_order {
            return Err("Down did not restore root order".into());
        }

        self.click(&format!("entity:{child}"))?;
        self.click("Parent")?;
        self.click("parent:root")?;
        if !self.structural_tree()?.contains(&(child.clone(), None)) {
            return Err("Parent Root did not detach the child".into());
        }
        self.click("Parent")?;
        self.click(&format!("parent:{parent}"))?;
        let arranged = self.structural_tree()?;
        if !arranged.contains(&(child.clone(), Some(parent.clone()))) {
            return Err("Parent did not reattach the child".into());
        }
        self.click("Remove Entity")?;
        self.click("Confirm removal")?;
        if self.structural_tree()?.iter().any(|(id, _)| id == &child) {
            return Err("Confirm removal did not delete the selected child".into());
        }
        if self.selected_id()? != parent {
            return Err("deletion did not select the surviving parent".into());
        }
        self.click("Undo")?;
        if self.structural_tree()? != arranged {
            return Err("Undo did not restore the exact structural tree".into());
        }
        self.click("Redo")?;
        if self.structural_tree()?.iter().any(|(id, _)| id == &child) {
            return Err("Redo did not repeat structural deletion".into());
        }
        self.click("Undo")?;
        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Saved { .. })
        ) {
            return Err("the structural Save did not complete".into());
        }
        if self.dirty()? {
            return Err("saved structure remained dirty".into());
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) {
            return Err("the structural Reopen did not complete".into());
        }
        if self.structural_tree()? != arranged {
            return Err("Reopen changed saved parent/order/identity".into());
        }
        if !arranged.iter().any(|(id, _)| id == &last) {
            return Err("reordering lost the second root".into());
        }
        if self.history()? != 0 || self.dirty()? {
            return Err("Reopen did not restore a clean history baseline".into());
        }
        self.click("Dismiss persistence result")?;
        Ok(self.presents)
    }
}
