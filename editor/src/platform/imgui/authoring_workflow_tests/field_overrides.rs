use super::*;
use nara::{scene::SceneEntityId, tooling::SceneInspectorFieldState};

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_field_reset_and_remove_round_trip_authored_scene_and_prefab_overrides() {
    let project = InteractionProject::new_field_overrides();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = OverrideTestApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    let presents = application
        .result
        .expect("the field override workflow must run")
        .unwrap();
    assert!(presents > 0);
}

struct OverrideTestApplication {
    project: PathBuf,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for OverrideTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_field_overrides(&self.project)
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn control(field: &str, label: &str) -> String {
    format!("field-action:{CUSTOM_COMPONENT}:{field}:{label}")
}

fn raw_value(path: &Path, entity: &SceneEntityId) -> Result<ComponentValue, String> {
    let source = read_source(path)?;
    let record = source["payload"]["entities"]
        .as_array()
        .and_then(|entities| {
            entities
                .iter()
                .find(|record| record["id"].as_str() == Some(entity.as_str()))
        })
        .ok_or("the authored entity was not saved")?;
    serde_json::from_value(record["components"][CUSTOM_COMPONENT]["value"].clone())
        .map_err(|error| error.to_string())
}

impl AuthoringWorkflow<'_> {
    fn expect_override(
        &self,
        name: &str,
        state: SceneInspectorFieldState,
        value: Option<ComponentValue>,
    ) -> Result<(), String> {
        let frame = self.frame()?;
        let field = frame
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
                    .find(|field| field.id.as_str() == name)
            })
            .ok_or_else(|| format!("the Inspector has no {name} field"))?;
        if field.state != state || field.value != value {
            return Err(format!(
                "{name}: expected {state:?}/{value:?}, observed {:?}/{:?}",
                field.state, field.value
            ));
        }
        Ok(())
    }

    fn field_overrides_document(
        &mut self,
        project: &Path,
        kind: EditorDocumentKind,
    ) -> Result<(), String> {
        self.click("+ Root")?;
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the source has no authoring model")?;
        if scene.kind != kind {
            return Err(format!("the {kind:?} source is not active"));
        }
        let entity = scene
            .selection
            .top_entity()
            .cloned()
            .ok_or("create did not select its entity")?;
        let source = project.join(match kind {
            EditorDocumentKind::Scene => "worlds/entry.scene.json",
            EditorDocumentKind::Prefab => "blueprints/ordinary.prefab.json",
            EditorDocumentKind::Extension(_) => {
                unreachable!("extension documents are not fixture sources")
            }
        });
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest();
        self.click("Add Component")?;
        self.click(CUSTOM_COMPONENT)?;
        self.click("Save")?;
        self.settle_persistence()?;
        let authored = ComponentValue::map([
            ("required_default", ComponentValue::I64(1)),
            ("optional_default", ComponentValue::I64(2)),
            ("optional", ComponentValue::I64(3)),
            ("nullable", ComponentValue::Null),
        ]);
        if raw_value(&source, &entity)? != authored || self.dirty()? {
            return Err(
                "Save did not retain authored overrides, including explicit Null".to_owned(),
            );
        }
        self.click("Dismiss persistence result")?;
        let mut depth = self.history()?;
        for (name, authored, effective) in [
            (
                "required_default",
                ComponentValue::I64(1),
                Some(ComponentValue::I64(10)),
            ),
            (
                "optional_default",
                ComponentValue::I64(2),
                Some(ComponentValue::I64(20)),
            ),
            ("optional", ComponentValue::I64(3), None),
            (
                "nullable",
                ComponentValue::Null,
                Some(ComponentValue::I64(40)),
            ),
        ] {
            let label = if effective.is_some() {
                "Reset to default"
            } else {
                "Remove field"
            };
            let state = if effective.is_some() {
                SceneInspectorFieldState::Defaulted
            } else {
                SceneInspectorFieldState::Missing
            };
            self.expect_override(
                name,
                SceneInspectorFieldState::Present,
                Some(authored.clone()),
            )?;
            self.click(&control(name, label))?;
            depth += 1;
            if self.history()? != depth {
                return Err(format!(
                    "{name}: the real {label} control did not create one history entry"
                ));
            }
            self.expect_override(name, state.clone(), effective.clone())?;
            self.click("Undo")?;
            self.expect_override(name, SceneInspectorFieldState::Present, Some(authored))?;
            if self.history()? != depth - 1 {
                return Err(format!("{name}: Undo did not restore authored history"));
            }
            self.click("Redo")?;
            self.expect_override(name, state.clone(), effective.clone())?;
            self.click(&control(name, label))?;
            self.expect_override(name, state, effective)?;
            if self.history()? != depth {
                return Err(format!(
                    "{name}: an omitted override created a removal history entry"
                ));
            }
        }
        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Saved { .. })
        ) || raw_value(&source, &entity)?
            != ComponentValue::map([] as [(&str, ComponentValue); 0])
            || self.dirty()?
        {
            return Err(
                "Save materialized removed overrides instead of preserving raw omission".to_owned(),
            );
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) {
            return Err("Reopen did not publish the saved source".to_owned());
        }
        self.click(&format!("entity:{}", entity.as_str()))?;
        for (name, value) in [
            ("required_default", 10),
            ("optional_default", 20),
            ("nullable", 40),
        ] {
            self.expect_override(
                name,
                SceneInspectorFieldState::Defaulted,
                Some(ComponentValue::I64(value)),
            )?;
            self.click(&control(name, "Reset to default"))?;
        }
        self.expect_override("optional", SceneInspectorFieldState::Missing, None)?;
        self.click(&control("optional", "Remove field"))?;
        if self.history()? != 0
            || self.dirty()?
            || self
                .application
                .active_authoring_unavailable_record_manifest()
                != manifest
        {
            return Err(
                "Reopen/disabled removal changed history, source dirtiness or unavailable records"
                    .to_owned(),
            );
        }
        self.click("Dismiss persistence result")?;
        Ok(())
    }

    fn run_field_overrides(mut self, project: &Path) -> Result<u64, String> {
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
        self.field_overrides_document(project, EditorDocumentKind::Scene)?;
        self.click("entity:prefab-anchor")?;
        self.click("Open Prefab Source")?;
        self.field_overrides_document(project, EditorDocumentKind::Prefab)?;
        Ok(self.presents)
    }
}
