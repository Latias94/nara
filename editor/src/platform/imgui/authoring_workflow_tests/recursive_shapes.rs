use super::*;
use nara_ui_kernel::UiSemanticRole;

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_recursive_inspector_drafts_round_trip_scene_and_prefab() {
    run_recursive_test(false);
}

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_owned_inspector_structured_fields_apply_undo_redo_save_and_reopen() {
    run_recursive_test(true);
}

fn run_recursive_test(owned: bool) {
    let project = InteractionProject::new_recursive();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = RecursiveAuthoringTestApplication {
        project: project.root.clone(),
        owned,
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
    owned: bool,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for RecursiveAuthoringTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                if self.owned {
                    workflow.run_owned_recursive(&self.project)
                } else {
                    workflow.run_recursive(&self.project)
                }
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl AuthoringWorkflow<'_> {
    fn recursive_click(&mut self, field: &str, path: &str, label: &str) -> Result<(), String> {
        let name = match label {
            "Apply value" => "Apply".to_owned(),
            "Cancel value" => "Discard".to_owned(),
            _ => format!("{path}: {label}"),
        };
        self.owned_test_named_click(field, &name)
    }

    fn recursive_replace(&mut self, field: &str, path: &str, value: &str) -> Result<(), String> {
        self.owned_test_named_replace(field, path, value)
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

    fn recursive_document(&mut self, source: &Path) -> Result<(), String> {
        let original = read_source(source)?;
        let initial_manifest = self
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
        self.owned_component_click("Add Component")?;
        self.owned_component_click(&format!("Add {CUSTOM_COMPONENT}"))?;
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("component creation lost the unavailable-record manifest")?;
        let mut placements = manifest.source_placements().iter();
        if manifest.entries() != initial_manifest.entries()
            || !manifest
                .overlay_declarations()
                .eq(initial_manifest.overlay_declarations())
            || !initial_manifest
                .source_placements()
                .iter()
                .all(|original| placements.any(|placement| placement == original))
        {
            return Err(
                "entity/component creation changed existing unavailable source data".into(),
            );
        }
        let baseline = self.history()?;
        self.recursive_click("optional", "root", "Edit value")?;
        self.recursive_click("optional", "root", "Some...")?;
        self.recursive_replace("optional", "root.new", "1e100")?;
        if self.owned_test_named_enabled("optional", UiSemanticRole::Button, "root: Use Some")? {
            return Err("an invalid Some exposed an enabled confirmation".into());
        }
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
        self.owned_test_named_replace("map", "root.key", "second")?;
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
        let stored = read_source(source)?;
        let entities = stored["payload"]["entities"]
            .as_array()
            .ok_or("the saved recursive source has no entity records")?;
        let retained = entities
            .iter()
            .filter(|record| record["id"].as_str() != Some(entity.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if original["payload"]["entities"].as_array() != Some(&retained) {
            return Err("recursive Save changed original source entity records".into());
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

    fn run_recursive(mut self, project: &Path) -> Result<u64, String> {
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
        self.recursive_document(&project.join("worlds/entry.scene.json"))?;
        self.click("entity:prefab-anchor")?;
        self.owned_component_click("Open Prefab Source")?;
        self.recursive_document(&project.join("blueprints/ordinary.prefab.json"))?;
        Ok(self.presents)
    }

    fn expect_owned_recursive_values(
        &self,
        fields: &[(&str, ComponentValue)],
        history: usize,
        phase: &str,
    ) -> Result<(), String> {
        for (field, value) in fields {
            self.expect_recursive_value(field, value)
                .map_err(|error| format!("{phase}: {error}"))?;
        }
        let actual = self.history()?;
        if actual != history {
            return Err(format!(
                "{phase}: structured document Undo depth was {actual}; expected {history}"
            ));
        }
        Ok(())
    }

    fn owned_recursive_document(
        &mut self,
        project: &Path,
        kind: EditorDocumentKind,
    ) -> Result<(), String> {
        let source = project.join(match kind {
            EditorDocumentKind::Scene => "worlds/entry.scene.json",
            EditorDocumentKind::Prefab => "blueprints/ordinary.prefab.json",
            EditorDocumentKind::Extension(_) => {
                return Err("the structured fixture must be a Scene or Prefab".into());
            }
        });
        let original = read_source(&source)?;
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the structured source has no authoring model")?;
        if scene.kind != kind || !scene.editor.inspector.composition.generic_authoring {
            return Err(format!(
                "the ordinary {kind:?} generic authoring model is not active"
            ));
        }
        let document = scene.document;
        let initial_manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the structured source has no unavailable-record manifest")?;
        if initial_manifest.total_count() == 0 || self.dirty()? {
            return Err(
                "the structured fixture must start clean with actual unknown records".into(),
            );
        }
        self.click("Inspector tab: Document")?;
        self.click("+ Root")?;
        let entity = self
            .frame()?
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.selection.top_entity())
            .cloned()
            .ok_or("the Root widget did not select the structured fixture entity")?;
        self.owned_component_click("Add Component")?;
        self.owned_component_click(&format!("Add {CUSTOM_COMPONENT}"))?;
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("component creation lost the structured unavailable-record manifest")?;
        let mut placements = manifest.source_placements().iter();
        if manifest.entries() != initial_manifest.entries()
            || !manifest
                .overlay_declarations()
                .eq(initial_manifest.overlay_declarations())
            || !initial_manifest
                .source_placements()
                .iter()
                .all(|original| placements.any(|placement| placement == original))
        {
            return Err(
                "structured fixture creation changed existing unavailable source data".into(),
            );
        }
        let baseline = self.history()?;
        let mut expected = [
            ("optional", ComponentValue::Null),
            (
                "vector",
                ComponentValue::map([
                    ("x", ComponentValue::f64(1.0).unwrap()),
                    ("y", ComponentValue::f64(2.0).unwrap()),
                ]),
            ),
            ("list", ComponentValue::List(vec![ComponentValue::U64(3)])),
            (
                "map",
                ComponentValue::map([("first", ComponentValue::String("before".into()))]),
            ),
        ];
        self.expect_owned_recursive_values(&expected, baseline, "component creation")?;
        self.owned_test_paint()?;

        self.owned_test_named_click("optional", "root: Edit value")?;
        self.owned_test_named_click("optional", "root: Some...")?;
        self.owned_test_named_replace("optional", "root.new", "1e100")?;
        if self.owned_test_named_enabled("optional", UiSemanticRole::Button, "root: Use Some")? {
            return Err("an invalid pending Some exposed an enabled confirmation".into());
        }
        self.owned_test_named_click("optional", "Apply")?;
        self.owned_test_named_text("optional", "root.new", "1e100")?;
        self.expect_owned_recursive_values(&expected, baseline, "invalid pending Some Apply")?;
        self.owned_test_named_click("optional", "root: Cancel Some")?;
        self.owned_test_named_click("optional", "Discard")?;
        self.expect_owned_recursive_values(&expected, baseline, "Some and root cancellation")?;
        self.owned_test_named_click("optional", "root: Edit value")?;
        self.owned_test_named_click("optional", "root: Some...")?;
        self.owned_test_named_replace("optional", "root.new", "7")?;
        self.owned_test_named_click("optional", "root: Use Some")?;
        self.expect_owned_recursive_values(&expected, baseline, "Some confirmation before Apply")?;
        self.owned_test_named_click("optional", "Apply")?;
        expected[0].1 = ComponentValue::f64(7.0).unwrap();
        self.expect_owned_recursive_values(&expected, baseline + 1, "whole Optional Apply")?;

        self.owned_test_named_click("vector", "root: Edit value")?;
        self.owned_test_named_replace("vector", "root.x", "1e100")?;
        self.owned_test_named_click("vector", "Apply")?;
        self.owned_test_named_text("vector", "root.x", "1e100")?;
        self.owned_test_named_text("vector", "root.y", "2")?;
        self.expect_owned_recursive_values(&expected, baseline + 1, "invalid Record Apply")?;
        self.owned_test_named_replace("vector", "root.x", "9")?;
        self.expect_owned_recursive_values(
            &expected,
            baseline + 1,
            "Record correction before Apply",
        )?;
        self.owned_test_named_click("vector", "Apply")?;
        expected[1].1 = ComponentValue::map([
            ("x", ComponentValue::f64(9.0).unwrap()),
            ("y", ComponentValue::f64(2.0).unwrap()),
        ]);
        self.expect_owned_recursive_values(&expected, baseline + 2, "whole Record Apply")?;

        self.owned_test_named_click("list", "root: Edit value")?;
        self.owned_test_named_click("list", "root: Add...")?;
        self.owned_test_named_replace("list", "root.new", "8")?;
        self.owned_test_named_click("list", "Apply")?;
        self.owned_test_named_text("list", "root.new", "8")?;
        self.expect_owned_recursive_values(&expected, baseline + 2, "pending List Apply")?;
        self.owned_test_named_click("list", "root: Cancel item")?;
        self.owned_test_named_text("list", "root[0]", "3")?;
        self.expect_owned_recursive_values(&expected, baseline + 2, "List child cancellation")?;
        self.owned_test_named_click("list", "root: Add...")?;
        self.owned_test_named_replace("list", "root.new", "8")?;
        self.owned_test_named_click("list", "root: Add value")?;
        self.owned_test_named_text("list", "root[1]", "8")?;
        self.owned_test_named_click("list", "root[0]: Remove item")?;
        self.owned_test_named_text("list", "root[0]", "8")?;
        self.expect_owned_recursive_values(&expected, baseline + 2, "List changes before Apply")?;
        self.owned_test_named_click("list", "Apply")?;
        expected[2].1 = ComponentValue::List(vec![ComponentValue::U64(8)]);
        self.expect_owned_recursive_values(&expected, baseline + 3, "whole List Apply")?;

        self.owned_test_named_click("map", "root: Edit value")?;
        self.owned_test_named_click("map", "root: Insert...")?;
        self.owned_test_named_replace("map", "root.key", "second")?;
        self.owned_test_named_replace("map", "root.new", "after")?;
        self.owned_test_named_click("map", "Apply")?;
        self.owned_test_named_text("map", "root.key", "second")?;
        self.owned_test_named_text("map", "root.new", "after")?;
        self.expect_owned_recursive_values(&expected, baseline + 3, "pending Map Apply")?;
        self.owned_test_named_click("map", "root: Cancel entry")?;
        self.owned_test_named_text("map", "root[\"first\"]", "before")?;
        self.expect_owned_recursive_values(&expected, baseline + 3, "Map child cancellation")?;
        self.owned_test_named_click("map", "root: Insert...")?;
        self.owned_test_named_replace("map", "root.key", "second")?;
        self.owned_test_named_replace("map", "root.new", "after")?;
        self.owned_test_named_click("map", "root: Insert value")?;
        self.owned_test_named_text("map", "root[\"second\"]", "after")?;
        self.owned_test_named_click("map", "root[\"first\"]: Remove entry")?;
        self.expect_owned_recursive_values(&expected, baseline + 3, "Map changes before Apply")?;
        self.owned_test_named_click("map", "Apply")?;
        let original_map = expected[3].1.clone();
        expected[3].1 = ComponentValue::map([("second", ComponentValue::String("after".into()))]);
        self.expect_owned_recursive_values(&expected, baseline + 4, "whole Map Apply")?;
        self.click("Undo")?;
        let applied_map = std::mem::replace(&mut expected[3].1, original_map);
        self.expect_owned_recursive_values(&expected, baseline + 3, "structured document Undo")?;
        self.click("Redo")?;
        expected[3].1 = applied_map;
        self.expect_owned_recursive_values(&expected, baseline + 4, "structured document Redo")?;
        if !self.dirty()?
            || read_source(&source)? != original
            || self
                .application
                .active_authoring_unavailable_record_manifest()
                != Some(manifest.clone())
        {
            return Err("structured drafts/history changed disk data or unknown records".into());
        }

        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(self.frame()?.persistence().result(), Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
            || self.dirty()?
        {
            return Err(format!(
                "Save did not persist the structured {kind:?} Apply results"
            ));
        }
        let stored = read_source(&source)?;
        let entities = stored["payload"]["entities"]
            .as_array()
            .ok_or("the saved structured source has no entity records")?;
        let created = entities
            .iter()
            .find(|record| record["id"].as_str() == Some(entity.as_str()))
            .ok_or("the structured fixture entity was not saved")?;
        let saved_component: ComponentValue =
            serde_json::from_value(created["components"][CUSTOM_COMPONENT]["value"].clone())
                .map_err(|error| error.to_string())?;
        let expected_component = ComponentValue::map(
            expected
                .iter()
                .map(|(field, value)| (*field, value.clone())),
        );
        let retained = entities
            .iter()
            .filter(|record| record["id"].as_str() != Some(entity.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if saved_component != expected_component
            || original["payload"]["entities"].as_array() != Some(&retained)
        {
            return Err(format!(
                "saving the structured {kind:?} changed its typed values or original entity records"
            ));
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) {
            return Err(format!("Reopen did not load the saved structured {kind:?}"));
        }
        self.click(&format!("entity:{}", entity.as_str()))?;
        self.expect_owned_recursive_values(&expected, 0, "structured source Reopen")?;
        if self.dirty()?
            || self
                .application
                .active_authoring_unavailable_record_manifest()
                != Some(manifest)
        {
            return Err(
                "structured Reopen changed unknown records or retained unsaved edits".into(),
            );
        }
        self.click("Dismiss persistence result")?;
        self.owned_test_paint()?;
        for (field, _) in &expected {
            if !self.owned_test_named_enabled(field, UiSemanticRole::Button, "root: Edit value")? {
                return Err(format!(
                    "the reopened structured field {field:?} retained a draft"
                ));
            }
        }
        self.expect_owned_recursive_values(&expected, 0, "reopened owned Inspector")
    }

    fn run_owned_recursive(mut self, project: &Path) -> Result<u64, String> {
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
        self.owned_recursive_document(project, EditorDocumentKind::Scene)?;
        self.click("entity:prefab-anchor")?;
        self.click("Inspector tab: Document")?;
        self.owned_component_click("Open Prefab Source")?;
        self.owned_recursive_document(project, EditorDocumentKind::Prefab)?;
        Ok(self.presents)
    }
}
