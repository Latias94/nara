use super::*;
use nara::{
    identity::EntityReference,
    scene::SceneEntityId,
    tooling::{EditorAction, EditorActionBatch, EditorNavigationAction},
};

const OTHER_TEXT: &str = "retained nested draft";

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_owned_inspector_nested_reference_choices_apply_undo_redo_save_and_reopen() {
    let project = InteractionProject::new_nested_entity_references();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = OwnedNestedReferencesTestApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    assert!(
        application
            .result
            .expect("the native nested reference workflow must run")
            .unwrap()
            > 0
    );
}

struct OwnedNestedReferencesTestApplication {
    project: PathBuf,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for OwnedNestedReferencesTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_owned_nested_references(&self.project)
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn reference(entity: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: SceneEntityId::new(entity).unwrap(),
    })
}

fn initial_references() -> ComponentValue {
    ComponentValue::map([
        ("direct", reference("target")),
        ("optional", ComponentValue::Null),
        ("list", ComponentValue::List(vec![reference("target")])),
        ("map", ComponentValue::map([("first", reference("target"))])),
        ("other", ComponentValue::String("before".into())),
    ])
}

fn changed_references() -> ComponentValue {
    ComponentValue::map([
        ("direct", reference("other")),
        ("optional", reference("other")),
        (
            "list",
            ComponentValue::List(vec![reference("other"), reference("target")]),
        ),
        (
            "map",
            ComponentValue::map([
                ("first", reference("other")),
                ("second", reference("target")),
            ]),
        ),
        ("other", ComponentValue::String(OTHER_TEXT.into())),
    ])
}

impl AuthoringWorkflow<'_> {
    fn owned_nested_select_source(&mut self) -> Result<(), String> {
        let frame = self.frame()?;
        let document = frame
            .workspace()
            .active_document
            .ok_or("the nested reference fixture has no active document")?;
        self.application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document,
                        entity: Some(SceneEntityId::new("source").unwrap()),
                    },
                )],
            ),
            Duration::ZERO,
        );
        self.owned_test_paint()
    }

    fn expect_owned_nested_value(
        &self,
        expected: &ComponentValue,
        history: usize,
        dirty: bool,
        phase: &str,
    ) -> Result<(), String> {
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the nested reference fixture has no authoring model")?;
        if scene.selection.top_entity().map(SceneEntityId::as_str) != Some("source") {
            return Err(format!("{phase}: the source entity is not selected"));
        }
        let actual = scene
            .editor
            .inspector
            .selected_entity_view
            .as_ref()
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
                    .find(|field| field.id.as_str() == "references")
            })
            .and_then(|field| field.value.as_ref());
        if actual != Some(expected) || self.history()? != history || self.dirty()? != dirty {
            return Err(format!(
                "{phase}: expected value {expected:?}, Undo depth {history}, dirty {dirty}; observed {actual:?}, {}, {}",
                self.history()?,
                self.dirty()?,
            ));
        }
        Ok(())
    }

    fn expect_owned_nested_unchanged(
        &self,
        original: &EditorFrameModel,
        phase: &str,
    ) -> Result<(), String> {
        if self.frame()?.workspace().active_scene != original.workspace().active_scene
            || self.dirty()?
        {
            return Err(format!(
                "{phase}: a nested draft changed the document or authoring history before Apply"
            ));
        }
        Ok(())
    }

    fn owned_nested_choose(&mut self, path: &str, entity: &str) -> Result<(), String> {
        self.owned_test_named_click("references", &format!("{path}: Choose target"))?;
        self.owned_test_named_click("references", &format!("{path}: Door [{entity}]"))
    }

    fn owned_nested_document(
        &mut self,
        project: &Path,
        kind: EditorDocumentKind,
    ) -> Result<(), String> {
        let source = project.join(match kind {
            EditorDocumentKind::Scene => "worlds/entry.scene.json",
            EditorDocumentKind::Prefab => "blueprints/ordinary.prefab.json",
            EditorDocumentKind::Extension(_) => {
                return Err("the nested reference fixture must be a Scene or Prefab".into());
            }
        });
        let original_source = read_source(&source)?;
        self.owned_nested_select_source()?;
        let original = self.frame()?;
        let scene = original
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the nested reference source has no authoring model")?;
        if scene.kind != kind || !scene.editor.inspector.composition.generic_authoring {
            return Err(format!("the ordinary {kind:?} source is not active"));
        }
        let document = scene.document;
        for entity in ["target", "other"] {
            if !scene
                .editor
                .inspector
                .entities
                .iter()
                .any(|row| row.id.as_str() == entity && row.name.as_deref() == Some("Door"))
            {
                return Err(format!(
                    "the {kind:?} fixture lost duplicate Door {entity:?}"
                ));
            }
        }
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the nested reference source has no unavailable-record manifest")?;
        if manifest.total_count() == 0 {
            return Err("the nested reference fixture must preserve actual unknown records".into());
        }
        let initial = initial_references();
        let changed = changed_references();
        self.expect_owned_nested_value(&initial, 0, false, "initial source")?;
        self.owned_test_paint()?;

        self.owned_test_named_click("references", "root: Edit value")?;
        self.owned_test_named_click("references", "root.direct: Choose target")?;
        self.expect_owned_nested_unchanged(&original, "opening the direct picker")?;
        self.owned_test_named_click("references", "root.direct: Hide targets")?;
        self.expect_owned_nested_unchanged(&original, "cancelling the direct picker")?;
        self.owned_nested_choose("root.direct", "other")?;
        self.expect_owned_nested_unchanged(&original, "reopened direct picker selection")?;
        self.owned_test_named_click("references", "Discard")?;
        self.expect_owned_nested_unchanged(&original, "whole-field cancellation")?;

        self.owned_test_named_click("references", "root: Edit value")?;
        self.owned_test_named_replace("references", "root.other", OTHER_TEXT)?;
        self.expect_owned_nested_unchanged(&original, "unrelated string draft")?;
        self.owned_nested_choose("root.direct", "other")?;
        self.owned_test_named_click("references", "root.direct: Choose target")?;
        self.owned_test_named_click("references", "root.direct: Hide targets")?;
        self.expect_owned_nested_unchanged(&original, "direct choice and automatic picker close")?;

        self.owned_test_named_click("references", "root.optional: Some...")?;
        self.owned_nested_choose("root.optional.new", "target")?;
        self.owned_test_named_click("references", "Apply")?;
        self.expect_owned_nested_unchanged(&original, "unconfirmed Optional Apply")?;
        self.owned_test_named_click("references", "root.optional: Use Some")?;
        self.owned_nested_choose("root.optional.some", "other")?;
        self.expect_owned_nested_unchanged(&original, "confirmed Optional reference draft")?;

        self.owned_nested_choose("root.list[0]", "other")?;
        self.owned_test_named_click("references", "root.list: Add...")?;
        self.owned_nested_choose("root.list.new", "target")?;
        self.owned_test_named_click("references", "Apply")?;
        self.expect_owned_nested_unchanged(&original, "unconfirmed List Apply")?;
        self.owned_test_named_click("references", "root.list: Add value")?;
        self.expect_owned_nested_unchanged(&original, "confirmed List reference draft")?;

        self.owned_nested_choose("root.map[\"first\"]", "other")?;
        self.owned_test_named_click("references", "root.map: Insert...")?;
        self.owned_test_named_replace("references", "root.map.key", "second")?;
        self.owned_nested_choose("root.map.new", "target")?;
        self.owned_test_named_click("references", "Apply")?;
        self.expect_owned_nested_unchanged(&original, "unconfirmed Map Apply")?;
        self.owned_test_named_click("references", "root.map: Insert value")?;
        self.expect_owned_nested_unchanged(&original, "confirmed Map reference draft")?;
        if read_source(&source)? != original_source {
            return Err("nested drafts changed source data on disk before Apply".into());
        }

        self.owned_test_named_click("references", "Apply")?;
        self.expect_owned_nested_value(&changed, 1, true, "whole-field Apply")?;
        self.owned_test_draft("references", None)?;
        self.click("Undo")?;
        self.expect_owned_nested_value(&initial, 0, false, "whole-field Undo")?;
        self.click("Redo")?;
        self.expect_owned_nested_value(&changed, 1, true, "whole-field Redo")?;
        if read_source(&source)? != original_source
            || self
                .application
                .active_authoring_unavailable_record_manifest()
                != Some(manifest.clone())
        {
            return Err("nested Apply/history changed disk data or unavailable records".into());
        }

        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(self.frame()?.persistence().result(), Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
        {
            return Err(format!(
                "Save did not persist the nested {kind:?} references"
            ));
        }
        self.expect_owned_nested_value(&changed, 1, false, "saved references")?;
        let mut stored = read_source(&source)?;
        let saved_record = stored["payload"]["entities"]
            .as_array_mut()
            .and_then(|entities| {
                entities
                    .iter_mut()
                    .find(|record| record["id"].as_str() == Some("source"))
            })
            .ok_or("the saved nested source has no source entity")?;
        let saved_component: ComponentValue =
            serde_json::from_value(saved_record["components"][CUSTOM_COMPONENT]["value"].clone())
                .map_err(|error| error.to_string())?;
        if saved_component != ComponentValue::map([("references", changed.clone())]) {
            return Err(
                "Save did not preserve the typed nested references and unrelated text".into(),
            );
        }
        let original_record = original_source["payload"]["entities"]
            .as_array()
            .and_then(|entities| {
                entities
                    .iter()
                    .find(|record| record["id"].as_str() == Some("source"))
            })
            .ok_or("the original nested fixture has no source entity")?;
        saved_record["components"][CUSTOM_COMPONENT]["value"] =
            original_record["components"][CUSTOM_COMPONENT]["value"].clone();
        if stored != original_source {
            return Err(format!(
                "saving the nested {kind:?} changed an original record or source declaration"
            ));
        }

        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) {
            return Err(format!("Reopen did not load the saved nested {kind:?}"));
        }
        self.owned_nested_select_source()?;
        self.expect_owned_nested_value(&changed, 0, false, "reopened typed references")?;
        self.owned_test_draft("references", None)?;
        if self
            .application
            .active_authoring_unavailable_record_manifest()
            != Some(manifest)
        {
            return Err("Reopen changed the original unavailable source records".into());
        }
        self.click("Dismiss persistence result")?;
        self.owned_test_named_click("references", "root: Edit value")?;
        self.owned_test_named_click("references", "Discard")?;
        self.expect_owned_nested_value(&changed, 0, false, "reopened owned Inspector")
    }

    fn run_owned_nested_references(mut self, project: &Path) -> Result<u64, String> {
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
        self.owned_test_paint()?;
        self.owned_nested_document(project, EditorDocumentKind::Scene)?;
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the saved nested fixture lost its Scene")?;
        let anchor = SceneEntityId::new("prefab-anchor").unwrap();
        self.application.submit_and_drive(
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
        self.owned_test_paint()?;
        self.owned_nested_document(project, EditorDocumentKind::Prefab)?;
        if fs::read(project.join("src/lib.rs")).map_err(|error| error.to_string())? != project_code
        {
            return Err("nested reference authoring changed project code".into());
        }
        Ok(self.presents)
    }
}
