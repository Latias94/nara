use super::*;

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_recursive_inspector_drafts_round_trip_scene_and_prefab() {
    let project = InteractionProject::new_recursive();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = RecursiveAuthoringTestApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    let presents = application
        .result
        .expect("the recursive authoring workflow must run")
        .unwrap();
    assert!(presents > 0);
}

struct RecursiveAuthoringTestApplication {
    project: PathBuf,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for RecursiveAuthoringTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_recursive()
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn control(field: &str, path: &str, label: &str) -> String {
    format!("recursive:{CUSTOM_COMPONENT}:{field}:{path}:{label}")
}

impl AuthoringWorkflow<'_> {
    fn recursive_click(&mut self, field: &str, path: &str, label: &str) -> Result<(), String> {
        self.click(&control(field, path, label))
    }

    fn recursive_replace(&mut self, field: &str, path: &str, value: &str) -> Result<(), String> {
        self.replace_text(&control(field, path, "value"), value)
    }

    fn expect_recursive_value(&self, field: &str, expected: &ComponentValue) -> Result<(), String> {
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
                    .find(|candidate| candidate.id.as_str() == field)
            })
            .and_then(|field| field.value.as_ref());
        if actual != Some(expected) {
            return Err(format!(
                "{field}: expected {expected:?}, observed {actual:?}"
            ));
        }
        Ok(())
    }

    fn recursive_document(&mut self) -> Result<(), String> {
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the source has no unavailable-record manifest")?;
        self.click("+ Root")?;
        let frame = self.frame()?;
        let entity = frame
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.selection.top_entity())
            .cloned()
            .ok_or("the create widget did not select its entity")?;
        self.click("Add Component")?;
        self.click(CUSTOM_COMPONENT)?;
        let baseline = self.history()?;
        self.recursive_click("optional", "root", "Edit value")?;
        self.recursive_click("optional", "root", "Some...")?;
        self.recursive_replace("optional", "root.new", "1e100")?;
        self.recursive_click("optional", "root", "Use Some")?;
        self.recursive_click("optional", "root", "Cancel Some")?;
        self.recursive_click("optional", "root", "Cancel value")?;
        if self.history()? != baseline {
            return Err("invalid/cancelled child created a transaction".to_owned());
        }
        self.recursive_click("optional", "root", "Edit value")?;
        self.recursive_click("optional", "root", "Some...")?;
        self.recursive_replace("optional", "root.new", "7")?;
        self.recursive_click("optional", "root", "Use Some")?;
        self.expect_recursive_value("optional", &ComponentValue::Null)?;
        self.recursive_click("optional", "root", "Apply value")?;
        self.expect_recursive_value("optional", &ComponentValue::f64(7.0).unwrap())?;
        self.recursive_click("optional", "root", "Edit value")?;
        self.recursive_click("optional", "root", "Set Null")?;
        self.recursive_click("optional", "root", "Apply value")?;
        self.click("Undo")?;
        self.expect_recursive_value("optional", &ComponentValue::f64(7.0).unwrap())?;
        self.click("Redo")?;
        self.expect_recursive_value("optional", &ComponentValue::Null)?;
        self.recursive_click("vector", "root", "Edit value")?;
        self.recursive_replace("vector", "root.x", "9")?;
        self.recursive_click("vector", "root", "Apply value")?;
        let vector = ComponentValue::map([
            ("x", ComponentValue::f64(9.0).unwrap()),
            ("y", ComponentValue::f64(2.0).unwrap()),
        ]);
        self.expect_recursive_value("vector", &vector)?;
        self.recursive_click("list", "root", "Edit value")?;
        self.recursive_click("list", "root", "Add...")?;
        self.recursive_replace("list", "root.new", "8")?;
        self.recursive_click("list", "root", "Add value")?;
        self.recursive_click("list", "root[0]", "Remove item")?;
        self.recursive_click("list", "root", "Apply value")?;
        let list = ComponentValue::List(vec![ComponentValue::U64(8)]);
        self.expect_recursive_value("list", &list)?;
        self.recursive_click("map", "root", "Edit value")?;
        self.recursive_click("map", "root", "Insert...")?;
        self.replace_text(&control("map", "root", "key"), "second")?;
        self.recursive_replace("map", "root.new", "after")?;
        self.recursive_click("map", "root", "Insert value")?;
        self.recursive_click("map", "root[\"first\"]", "Remove entry")?;
        self.recursive_click("map", "root", "Apply value")?;
        let map = ComponentValue::map([("second", ComponentValue::String("after".into()))]);
        self.expect_recursive_value("map", &map)?;
        if self.history()? != baseline + 5 {
            return Err(
                "recursive controls did not create exactly five root transactions".to_owned(),
            );
        }
        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Saved { .. })
        ) {
            return Err("Save did not publish the recursive document".to_owned());
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        self.click(&format!("entity:{}", entity.as_str()))?;
        self.expect_recursive_value("optional", &ComponentValue::Null)?;
        self.expect_recursive_value("vector", &vector)?;
        self.expect_recursive_value("list", &list)?;
        self.expect_recursive_value("map", &map)?;
        if self
            .application
            .active_authoring_unavailable_record_manifest()
            != Some(manifest)
        {
            return Err("recursive edits changed the unavailable-record manifest".to_owned());
        }
        self.click("Dismiss persistence result")?;
        Ok(())
    }

    fn run_recursive(mut self) -> Result<u64, String> {
        for _ in 0..120 {
            self.paint(|_| {})?;
            if self.application.current_frame().is_some() {
                break;
            }
        }
        self.frame()?;
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        self.recursive_document()?;
        self.click("entity:prefab-anchor")?;
        self.click("Open Prefab Source")?;
        self.recursive_document()?;
        Ok(self.presents)
    }
}
