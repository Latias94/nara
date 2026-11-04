use super::*;
use winit::event::{DeviceId, ElementState, MouseButton as NativeMouseButton};

#[test]
#[ignore = "requires an interactive Windows desktop and a working WGPU adapter"]
fn native_owned_inspector_components_scene_and_prefab_fold_apply_undo_redo_save_and_reopen() {
    let project = InteractionProject::new();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = OwnedComponentsTestApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    let presents = application
        .result
        .expect("the native owned component workflow must run")
        .unwrap();
    assert!(presents > 0);
}

struct OwnedComponentsTestApplication {
    project: PathBuf,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for OwnedComponentsTestApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_owned_components(&self.project)
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl AuthoringWorkflow<'_> {
    pub(super) fn owned_component_click(&mut self, label: &str) -> Result<(), String> {
        self.owned_test_root_pointer(label)?;
        for state in [ElementState::Pressed, ElementState::Released] {
            if !self.owned_test_event(WindowEvent::MouseInput {
                device_id: DeviceId::dummy(),
                state,
                button: NativeMouseButton::Left,
            })? {
                return Err(format!(
                    "component Button {label:?} did not consume native {state:?}"
                ));
            }
            self.owned_test_paint()?;
        }
        Ok(())
    }

    fn expect_owned_component(
        &self,
        expected: Option<i64>,
        history: usize,
        phase: &str,
    ) -> Result<(), String> {
        let frame = self.frame()?;
        let entity = frame
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.editor.inspector.selected_entity_view.as_ref())
            .ok_or("the component fixture lost its selected entity")?;
        let present = entity
            .components
            .iter()
            .any(|component| component.component.as_str() == CUSTOM_COMPONENT);
        let actual = self.selected_value()?;
        let depth = self.history()?;
        if present != expected.is_some() || actual != expected || depth != history {
            return Err(format!(
                "{phase}: component present={present}, value={actual:?}, Undo depth={depth}; expected value={expected:?}, Undo depth={history}"
            ));
        }
        Ok(())
    }

    fn owned_components_document(
        &mut self,
        project: &Path,
        kind: EditorDocumentKind,
    ) -> Result<(), String> {
        let source = project.join(match kind {
            EditorDocumentKind::Scene => "worlds/entry.scene.json",
            EditorDocumentKind::Prefab => "blueprints/ordinary.prefab.json",
            EditorDocumentKind::Extension(_) => {
                return Err("the component fixture must be a Scene or Prefab".into());
            }
        });
        let original_bytes = fs::read(&source).map_err(|error| error.to_string())?;
        let original: serde_json::Value =
            serde_json::from_slice(&original_bytes).map_err(|error| error.to_string())?;
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the component fixture has no authoring model")?;
        if scene.kind != kind
            || !scene.editor.inspector.composition.generic_authoring
            || self.history()? != 0
            || self.dirty()?
        {
            return Err(format!(
                "the ordinary {kind:?} component fixture is not active and clean"
            ));
        }
        let document = scene.document;
        let initial_manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the component fixture has no unavailable-record manifest")?;
        if initial_manifest.total_count() == 0 {
            return Err("component authoring must preserve actual unknown source records".into());
        }
        self.click("+ Root")?;
        let entity = self
            .frame()?
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.selection.top_entity())
            .cloned()
            .ok_or("the real create widget did not select its entity")?;
        self.expect_owned_component(None, 1, "entity creation")?;
        self.owned_test_paint()?;
        self.owned_component_click("Add Component")?;
        self.expect_owned_component(None, 1, "component menu without admission")?;
        self.owned_component_click(&format!("Add {CUSTOM_COMPONENT}"))?;
        self.expect_owned_component(Some(10), 2, "native component Add")?;
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("native component Add lost the unavailable-record manifest")?;
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
                "native entity/component creation changed original unavailable records".into(),
            );
        }

        self.owned_test_named_replace("value", "value", "42")?;
        self.owned_test_draft("value", Some("42"))?;
        self.expect_owned_component(Some(10), 2, "field commit without Apply")?;
        let before_fold = self.frame()?.workspace().active_scene.clone();
        for (action, replacement) in [("Collapse", "Expand"), ("Expand", "Collapse")] {
            self.owned_component_click(&format!("{action} {CUSTOM_COMPONENT}"))?;
            self.owned_test_root_control(&format!("{replacement} {CUSTOM_COMPONENT}"))?;
            self.owned_test_draft("value", Some("42"))?;
            if self.frame()?.workspace().active_scene != before_fold {
                return Err(format!(
                    "component {action} changed source, selection or document history"
                ));
            }
            if action == "Collapse" {
                if self.owned_test_control("value", None).is_ok()
                    || self.native.owned_text_focus_for_test()
                {
                    return Err(
                        "a collapsed component retained an interactive or focused field".into(),
                    );
                }
            } else {
                self.owned_test_named_text("value", "value", "42")?;
            }
        }
        self.owned_test_apply("value")?;
        self.expect_owned_component(Some(42), 3, "explicit native field Apply")?;
        for (action, expected, history) in [("Undo", Some(10), 2), ("Redo", Some(42), 3)] {
            self.click(action)?;
            self.expect_owned_component(expected, history, action)?;
        }
        self.owned_component_click(&format!("Remove {CUSTOM_COMPONENT}"))?;
        self.expect_owned_component(None, 4, "native component Remove")?;
        for (action, expected, history) in [
            ("Undo", Some(42), 3),
            ("Redo", None, 4),
            ("Undo", Some(42), 3),
        ] {
            self.click(action)?;
            self.expect_owned_component(expected, history, action)?;
        }
        if !self.dirty()?
            || fs::read(&source).map_err(|error| error.to_string())? != original_bytes
            || self
                .application
                .active_authoring_unavailable_record_manifest()
                != Some(manifest.clone())
        {
            return Err(
                "native component edits changed disk/unknown records or lost dirty state".into(),
            );
        }

        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(self.frame()?.persistence().result(), Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
            || self.dirty()?
        {
            return Err("the real Save widget did not persist native component authoring".into());
        }
        let stored = read_source(&source)?;
        let entities = stored["payload"]["entities"]
            .as_array()
            .ok_or("the saved component source has no entity records")?;
        let created = entities
            .iter()
            .find(|record| record["id"].as_str() == Some(entity.as_str()))
            .ok_or("the native component entity was not saved")?;
        let saved_value: ComponentValue =
            serde_json::from_value(created["components"][CUSTOM_COMPONENT]["value"].clone())
                .map_err(|error| error.to_string())?;
        if saved_value != ComponentValue::map([("value", ComponentValue::I64(42))]) {
            return Err(format!(
                "native component Save stored the wrong typed value: {saved_value:?}"
            ));
        }
        let retained_entities = entities
            .iter()
            .filter(|record| record["id"].as_str() != Some(entity.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if original["payload"]["entities"].as_array() != Some(&retained_entities) {
            return Err(
                "native component Save changed an original entity or unknown component".into(),
            );
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) {
            return Err("the real Reopen widget did not reopen native component authoring".into());
        }
        self.click(&format!("entity:{}", entity.as_str()))?;
        self.expect_owned_component(Some(42), 0, "saved component Reopen")?;
        self.owned_test_named_text("value", "value", "42")?;
        self.owned_test_draft("value", None)?;
        if self.dirty()?
            || self
                .application
                .active_authoring_unavailable_record_manifest()
                != Some(manifest)
        {
            return Err("component Reopen changed unknown records or retained dirty state".into());
        }
        self.click("Dismiss persistence result")?;
        Ok(())
    }

    fn run_owned_components(mut self, project: &Path) -> Result<u64, String> {
        let project_code =
            fs::read(project.join("src/lib.rs")).map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.application.current_frame().is_none() && Instant::now() < deadline {
            self.paint(|_| {})?;
            std::thread::sleep(Duration::from_millis(1));
        }
        self.frame()?;
        self.owned_test_paint()?;
        let first = self
            .application
            .panel_workspace()
            .layout()
            .instances()
            .iter()
            .find(|panel| panel.kind() == "nara.inspector")
            .ok_or("the default Inspector is missing")?
            .id();
        let second = self
            .application
            .panel_workspace_mut()
            .open(
                "nara.inspector",
                crate::workspace::EditorPanelTarget::Workspace,
            )
            .map_err(|error| error.to_string())?;
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .place(second, crate::workspace::EditorPanelPlacement::Floating);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.owned_test_paint()?;
            let frames = self.native.owned_frames_for_test();
            if frames.iter().any(|(panel, ..)| *panel == first)
                && frames.iter().any(|(panel, ..)| *panel == second)
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err(
                    "both independent hosted Inspectors did not reach actual native presentation"
                        .into(),
                );
            }
        }
        let frames = self.native.owned_frames_for_test();
        let (_, first_frame, first_image) = frames
            .iter()
            .find(|(panel, ..)| *panel == first)
            .ok_or("the default Inspector has no actual image")?;
        let (_, second_frame, second_image) = frames
            .iter()
            .find(|(panel, ..)| *panel == second)
            .ok_or("the second Inspector has no actual image")?;
        if Arc::ptr_eq(first_frame, second_frame)
            || first_frame.environment().surface_id() == second_frame.environment().surface_id()
            || first_image.occurrence.is_none()
            || second_image.occurrence.is_none()
            || first_image.occurrence == second_image.occurrence
        {
            return Err(
                "independent Inspectors shared a frame, surface or native presentation occurrence"
                    .into(),
            );
        }
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .close(second);
        self.owned_test_paint()?;
        let frames = self.native.owned_frames_for_test();
        if frames.iter().any(|(panel, ..)| *panel == second)
            || !frames.iter().any(|(panel, ..)| *panel == first)
        {
            return Err(
                "closing one hosted Inspector retired another or preserved the old route".into(),
            );
        }
        self.owned_components_document(project, EditorDocumentKind::Scene)
            .map_err(|error| format!("Scene component workflow: {error}"))?;
        self.click("entity:prefab-anchor")?;
        self.click("Inspector tab: Document")?;
        self.owned_component_click("Open Prefab Source")?;
        self.owned_components_document(project, EditorDocumentKind::Prefab)
            .map_err(|error| format!("Prefab component workflow: {error}"))?;
        if fs::read(project.join("src/lib.rs")).map_err(|error| error.to_string())? != project_code
            || !String::from_utf8_lossy(&project_code).contains("compile_error!")
        {
            return Err("native component authoring changed or compiled project code".into());
        }
        Ok(self.presents)
    }
}
