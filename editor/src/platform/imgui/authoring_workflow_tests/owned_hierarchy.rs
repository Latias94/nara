use super::owned_fields::OwnedFieldProbe;
use super::*;
use nara::{
    scene::{
        AuthoringDocumentReadiness, PrefabDocument, PrefabEntityTarget, SceneDocument,
        SceneEntityId, SceneEntityPlacement, SceneEntityRecord, ScenePlacement,
        ScenePlacementParent, ScenePlacementSlot,
    },
    tooling::SceneInspectorReadOnlyReason,
};
use nara_ui_kernel::{UiPoint, UiSemanticRole};
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, Ime, MouseScrollDelta, TouchPhase},
};

const PANEL: &str = "nara.hierarchy";
const DECLARATIONS: usize = 1024;
const LAST_DECLARATION: &str = "declaration-1023";
const RETAINED_LABEL: &str = "retained hierarchy draft";

#[test]
#[ignore = "requires an interactive Windows desktop and a working DX12 WGPU adapter"]
fn native_owned_hierarchy_scene_and_prefab_arrange_remove_history_save_and_reopen() {
    let scene = InteractionProject::new();
    fs::write(
        scene.source(EditorDocumentKind::Scene),
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("original").unwrap(),
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let prefab = InteractionProject::new();
    fs::write(
        prefab.source(EditorDocumentKind::Prefab),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("original").unwrap(),
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = HierarchyStructureApplication {
        sources: [
            (scene.root.clone(), EditorDocumentKind::Scene),
            (prefab.root.clone(), EditorDocumentKind::Prefab),
        ],
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    let presents = application
        .result
        .expect("both complete native Hierarchy trajectories must execute")
        .unwrap();
    assert!(presents.into_iter().all(|count| count > 0));
}

#[test]
#[ignore = "requires an interactive Windows desktop and a working DX12 WGPU adapter"]
fn native_owned_hierarchy_thousand_declarations_filter_wheel_and_document_text_lifetime() {
    let project = thousand_declaration_project();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = HierarchyFilterApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    assert!(
        application
            .result
            .expect("the native thousand-declaration Hierarchy trajectory must execute")
            .unwrap()
            > 0
    );
}

fn thousand_declaration_project() -> InteractionProject {
    let project = InteractionProject::new_field_drafts();
    let records = (0..DECLARATIONS)
        .map(|index| {
            SceneEntityRecord::new(SceneEntityId::new(format!("declaration-{index:04}")).unwrap())
        })
        .collect::<Vec<_>>();
    let placements = records
        .iter()
        .map(|record| SceneEntityPlacement::root(record.id.clone()))
        .collect::<Vec<_>>();
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let source = project.source(kind);
        let mut original = read_source(&source).unwrap();
        let entities = original["payload"]["entities"].as_array_mut().unwrap();
        entities.extend(
            records
                .iter()
                .map(|record| serde_json::to_value(record).unwrap()),
        );
        entities.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
        original["payload"]["placements"]
            .as_array_mut()
            .unwrap()
            .extend(
                placements
                    .iter()
                    .map(|placement| serde_json::to_value(placement).unwrap()),
            );
        fs::write(source, serde_json::to_vec_pretty(&original).unwrap()).unwrap();
    }
    project
}

struct HierarchyStructureApplication {
    sources: [(PathBuf, EditorDocumentKind); 2],
    result: Option<Result<[u64; 2], String>>,
}

impl ApplicationHandler for HierarchyStructureApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some((|| {
                let mut presents = [0; 2];
                for (index, (project, kind)) in self.sources.iter().enumerate() {
                    presents[index] = run_native_workflow(event_loop, project, |workflow| {
                        workflow.run_hierarchy_structure(project, *kind)
                    })?;
                }
                Ok(presents)
            })());
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

struct HierarchyFilterApplication {
    project: PathBuf,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for HierarchyFilterApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_hierarchy_filter(&self.project)
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn source_path(project: &Path, kind: EditorDocumentKind) -> Result<PathBuf, String> {
    match kind {
        EditorDocumentKind::Scene => Ok(project.join("worlds/entry.scene.json")),
        EditorDocumentKind::Prefab => Ok(project.join("blueprints/ordinary.prefab.json")),
        EditorDocumentKind::Extension(_) => {
            Err("the Hierarchy fixture must be a Scene or Prefab".into())
        }
    }
}

impl AuthoringWorkflow<'_> {
    pub(super) fn hierarchy_click(&mut self, name: &str) -> Result<(), String> {
        self.owned_test_panel_reveal(PANEL, None, UiSemanticRole::Button, name)?;
        self.owned_test_panel_pointer(PANEL, None, UiSemanticRole::Button, name, 0.5)?;
        for state in [ElementState::Pressed, ElementState::Released] {
            self.owned_test_mouse(state)?;
            self.owned_test_paint()?;
        }
        Ok(())
    }

    pub(super) fn hierarchy_select(&mut self, entity: &str) -> Result<(), String> {
        self.hierarchy_click(&format!("Select {entity}"))?;
        if self.selected_id()? != entity {
            return Err(format!(
                "the confirmed Hierarchy row did not select {entity:?}"
            ));
        }
        Ok(())
    }

    fn hierarchy_probe(&self, role: UiSemanticRole, name: &str) -> Result<OwnedFieldProbe, String> {
        self.owned_test_panel_probe(PANEL, None, role, name)
    }

    fn hierarchy_enabled(&self, name: &str) -> Result<bool, String> {
        let probe = self.hierarchy_probe(UiSemanticRole::Button, name)?;
        probe
            .frame
            .semantics()
            .iter()
            .find(|node| node.key() == &probe.key)
            .map(|node| node.enabled())
            .ok_or_else(|| format!("the Hierarchy lost button {name:?}"))
    }

    fn hierarchy_ready(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.application.current_frame().is_none() && Instant::now() < deadline {
            self.paint(|_| {})?;
            std::thread::sleep(Duration::from_millis(1));
        }
        self.frame()?;
        self.owned_test_paint()?;
        self.owned_test_panel_image(PANEL)?;
        Ok(())
    }

    fn hierarchy_open_prefab(&mut self) -> Result<(), String> {
        self.hierarchy_select("prefab-anchor")?;
        self.owned_component_click("Open Prefab Source")?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.owned_test_paint()?;
            if self
                .frame()?
                .workspace()
                .active_scene
                .as_ref()
                .is_some_and(|scene| scene.kind == EditorDocumentKind::Prefab)
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("the actual Open Prefab Source widget did not switch documents".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn hierarchy_expect_placement(
        &self,
        entity: &str,
        parent: ScenePlacementParent,
        slot: ScenePlacementSlot,
        history: usize,
    ) -> Result<(), String> {
        let expected = Some(ScenePlacement { parent, slot });
        let declarations = self.structural_intents()?;
        let actual = declarations.iter().find(|(id, _)| id == entity);
        if actual.map(|(_, placement)| placement) != Some(&expected) || self.history()? != history {
            return Err(format!(
                "Hierarchy placement for {entity:?} was {actual:?} at Undo depth {}; expected {expected:?} at {history}",
                self.history()?
            ));
        }
        Ok(())
    }

    fn run_hierarchy_structure(
        mut self,
        project: &Path,
        kind: EditorDocumentKind,
    ) -> Result<u64, String> {
        self.hierarchy_ready()?;
        if kind == EditorDocumentKind::Prefab {
            self.hierarchy_open_prefab()?;
        }
        let source = source_path(project, kind)?;
        let original = read_source(&source)?;
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("missing Hierarchy source")?;
        let document = scene.document;
        if scene.kind != kind
            || frame
                .workspace()
                .document(document)
                .and_then(|tab| tab.authoring_readiness)
                != Some(AuthoringDocumentReadiness::Complete)
            || self.history()? != 0
            || self.dirty()?
        {
            return Err(format!(
                "the native {kind:?} deletion fixture is not Complete and clean"
            ));
        }
        self.hierarchy_click("+ Root")?;
        let parent = self.selected_id()?;
        self.hierarchy_click("+ Child")?;
        let child = self.selected_id()?;
        self.hierarchy_expect_placement(
            &child,
            ScenePlacementParent::Entity(PrefabEntityTarget::source(
                SceneEntityId::new(&parent).unwrap(),
            )),
            ScenePlacementSlot::Last,
            2,
        )?;
        self.hierarchy_click("+ Root")?;
        let peer = self.selected_id()?;
        let created = self.structural_intents()?;
        let parent_target = PrefabEntityTarget::source(SceneEntityId::new(&parent).unwrap());
        for (name, slot, history) in [
            ("Order: First".to_owned(), ScenePlacementSlot::First, 4),
            ("Order: Last".to_owned(), ScenePlacementSlot::Last, 5),
            (
                format!("Order: Before {parent}"),
                ScenePlacementSlot::Before(parent_target.clone()),
                6,
            ),
            (
                format!("Order: After {parent}"),
                ScenePlacementSlot::After(parent_target.clone()),
                7,
            ),
            ("Order: Last".to_owned(), ScenePlacementSlot::Last, 8),
        ] {
            self.hierarchy_click("Order")?;
            self.hierarchy_click(&name)?;
            self.hierarchy_expect_placement(&peer, ScenePlacementParent::Root, slot, history)?;
        }
        if self.structural_intents()? != created {
            return Err("native ordering did not restore the exact source declarations".into());
        }
        for menu in ["Order", "Parent"] {
            self.hierarchy_click(menu)?;
            self.hierarchy_click("Close choices")?;
            if self.structural_intents()? != created || self.history()? != 8 {
                return Err("closing native placement choices changed source or history".into());
            }
        }
        self.hierarchy_select(&child)?;
        self.hierarchy_click("Parent")?;
        self.hierarchy_click("Parent: Root")?;
        self.hierarchy_expect_placement(
            &child,
            ScenePlacementParent::Root,
            ScenePlacementSlot::Last,
            9,
        )?;
        self.hierarchy_click("Parent")?;
        self.hierarchy_click(&format!("Parent entity: {parent}"))?;
        self.hierarchy_expect_placement(
            &child,
            ScenePlacementParent::Entity(parent_target),
            ScenePlacementSlot::Last,
            10,
        )?;
        let arranged = self.structural_intents()?;
        self.hierarchy_click("Remove Entity")?;
        if self.structural_intents()? != arranged || self.history()? != 10 {
            return Err("requesting native removal changed the source before confirmation".into());
        }
        self.hierarchy_click("Cancel removal")?;
        if self.structural_intents()? != arranged
            || self.history()? != 10
            || self.selected_id()? != child
        {
            return Err(
                "canceling native removal changed declarations, selection or history".into(),
            );
        }
        self.hierarchy_click("Remove Entity")?;
        self.hierarchy_click("Confirm removal")?;
        let removed = self.structural_intents()?;
        if removed.iter().any(|(id, _)| id == &child)
            || self.selected_id()? != parent
            || self.history()? != 11
        {
            return Err(
                "native confirmation did not delete the child and select its surviving parent"
                    .into(),
            );
        }
        self.click("Undo")?;
        if self.structural_intents()? != arranged || self.history()? != 10 {
            return Err("toolbar Undo did not restore the exact Nara-arranged declarations".into());
        }
        self.click("Redo")?;
        if self.structural_intents()? != removed || self.history()? != 11 {
            return Err("toolbar Redo did not repeat the exact Nara-confirmed removal".into());
        }
        self.click("Undo")?;
        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(self.frame()?.persistence().result(), Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
            || self.dirty()?
        {
            return Err(format!("the native {kind:?} hierarchy was not saved"));
        }
        let stored = read_source(&source)?;
        let entities = stored["payload"]["entities"]
            .as_array()
            .ok_or("saved Hierarchy has no records")?;
        let originals = original["payload"]["entities"]
            .as_array()
            .ok_or("original Hierarchy has no records")?;
        if entities.len() != originals.len() + 3
            || !originals.iter().all(|record| entities.contains(record))
        {
            return Err(
                "saving native structure changed an original entity or lost a created entity"
                    .into(),
            );
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) || self.structural_intents()? != arranged
            || self.history()? != 0
            || self.dirty()?
            || read_source(&source)? != stored
        {
            return Err("native structure Reopen did not retain exact saved parent/order/identity and clean history".into());
        }
        self.click("Dismiss persistence result")?;
        self.hierarchy_select(&child)?;
        Ok(self.presents)
    }

    fn hierarchy_focus_filter(&mut self) -> Result<OwnedFieldProbe, String> {
        self.owned_test_panel_reveal(PANEL, None, UiSemanticRole::TextInput, "Filter entities")?;
        self.owned_test_panel_pointer(
            PANEL,
            None,
            UiSemanticRole::TextInput,
            "Filter entities",
            1.0,
        )?;
        self.owned_test_mouse(ElementState::Pressed)?;
        let probe = self.hierarchy_probe(UiSemanticRole::TextInput, "Filter entities")?;
        if self.native.owned_focused_control_for_test() != Some((probe.panel, probe.key.clone()))
            || !self.native.owned_text_focus_for_test()
        {
            return Err(
                "the confirmed Hierarchy filter did not acquire exact native text focus".into(),
            );
        }
        self.owned_test_panel_pointer(
            PANEL,
            None,
            UiSemanticRole::TextInput,
            "Filter entities",
            0.0,
        )?;
        self.owned_test_mouse(ElementState::Released)?;
        self.owned_test_paint()?;
        self.hierarchy_probe(UiSemanticRole::TextInput, "Filter entities")
    }

    fn hierarchy_replace_filter(&mut self, text: &str) -> Result<(), String> {
        self.hierarchy_focus_filter()?;
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Commit(text.into()))?;
        self.hierarchy_filter_text(text, false)
    }

    fn hierarchy_filter_text(&self, expected: &str, composing: bool) -> Result<(), String> {
        let probe = self.hierarchy_probe(UiSemanticRole::TextInput, "Filter entities")?;
        let value = probe
            .frame
            .semantics()
            .iter()
            .find(|node| node.key() == &probe.key)
            .and_then(|node| node.value());
        let token = self
            .native
            .frontend
            .owned_field_token(probe.panel, &probe.frame, &probe.key);
        let actual_composing = token.is_some_and(|token| {
            self.native
                .frontend
                .owned_composing(probe.panel, &probe.frame, &probe.key, token)
        });
        if value != Some(expected) || actual_composing != composing {
            return Err(format!(
                "Hierarchy filter retained {value:?}, composing {actual_composing}; expected {expected:?}, {composing}"
            ));
        }
        Ok(())
    }

    fn hierarchy_matches(&self, matching: usize, total: usize) -> Result<(), String> {
        self.hierarchy_probe(
            UiSemanticRole::Text,
            &format!("{matching} of {total} source entities"),
        )?;
        Ok(())
    }

    fn hierarchy_visible_rows(&self) -> Result<Vec<String>, String> {
        let (frame, _) = self.owned_test_panel_image(PANEL)?;
        Ok(frame
            .semantics()
            .iter()
            .filter_map(|node| {
                let id = (node.role() == UiSemanticRole::Button)
                    .then(|| node.name().strip_prefix("Select "))
                    .flatten()?;
                let layout = frame
                    .layouts()
                    .iter()
                    .find(|layout| layout.key() == Some(node.key()))?;
                if !layout.visible() {
                    return None;
                }
                let rect = match layout.clip_rect() {
                    Some(clip) => layout.rect().intersect(clip)?,
                    None => layout.rect(),
                };
                let point = UiPoint::new(
                    rect.origin().x() + rect.size().width() * 0.5,
                    rect.origin().y() + rect.size().height() * 0.5,
                );
                frame
                    .hit_test(point)
                    .is_some_and(|hit| hit.key() == node.key())
                    .then(|| id.to_owned())
            })
            .collect())
    }

    fn hierarchy_wheel(&mut self, delta: MouseScrollDelta) -> Result<(), String> {
        self.hierarchy_wheel_burst(&[delta])
    }

    fn hierarchy_wheel_burst(&mut self, deltas: &[MouseScrollDelta]) -> Result<(), String> {
        let (_, image) = self.owned_test_panel_image(PANEL)?;
        self.pointer = [
            image.viewport_origin[0] + (image.rect.min[0] + image.rect.max[0]) * 0.5,
            image.viewport_origin[1] + (image.rect.min[1] + image.rect.max[1]) * 0.5,
        ];
        self.paint(|_| {})?;
        let (_, image) = self.owned_test_panel_image(PANEL)?;
        let local = [
            (image.rect.min[0] + image.rect.max[0]) * 0.5,
            (image.rect.min[1] + image.rect.max[1]) * 0.5,
        ];
        self.pointer = [
            image.viewport_origin[0] + local[0],
            image.viewport_origin[1] + local[1],
        ];
        if !image.owns_native_pointer(&self.native.context, self.pointer) {
            return Err("the actual Hierarchy image is occluded at its native wheel target".into());
        }
        self.owned_test_event(WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(
                f64::from(local[0]) * f64::from(image.pixels_per_point),
                f64::from(local[1]) * f64::from(image.pixels_per_point),
            ),
        })?;
        for &delta in deltas {
            if !self.owned_test_event(WindowEvent::MouseWheel {
                device_id: DeviceId::dummy(),
                delta,
                phase: TouchPhase::Moved,
            })? {
                return Err(
                    "the confirmed Hierarchy image did not consume its native wheel".into(),
                );
            }
        }
        self.owned_test_paint()
    }

    fn hierarchy_preserved_document(
        &mut self,
        project: &Path,
        kind: EditorDocumentKind,
    ) -> Result<(), String> {
        let source = source_path(project, kind)?;
        let original = read_source(&source)?;
        let original_manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("the Hierarchy fixture has no unavailable-record manifest")?;
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the large Hierarchy has no source")?;
        if scene.kind != kind
            || scene.editor.inspector.entities.len() < DECLARATIONS
            || original_manifest.total_count() == 0
        {
            return Err("the large native source must retain at least 1,024 declarations and actual unknown records".into());
        }
        let document = scene.document;
        self.hierarchy_replace_filter("preserved")?;
        self.hierarchy_select("preserved")?;
        let frame = self.frame()?;
        let unknown = frame
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.editor.inspector.selected_entity_view.as_ref())
            .and_then(|entity| {
                entity
                    .components
                    .iter()
                    .find(|component| component.component.as_str() == "studio.unavailable.Payload")
            })
            .ok_or("the native unknown entity has no preserved component observation")?;
        if unknown.access.read_only_reason() != Some(SceneInspectorReadOnlyReason::UnknownSchema)
            || self.hierarchy_enabled("Remove Entity")?
        {
            return Err(
                "the unknown source lost read-only diagnostics or admitted unsafe entity removal"
                    .into(),
            );
        }
        self.owned_test_panel_control(
            "nara.inspector",
            None,
            UiSemanticRole::Text,
            "Read-only component: UnknownSchema",
        )?;
        let (hierarchy, _) = self.owned_test_panel_image(PANEL)?;
        if !hierarchy
            .semantics()
            .iter()
            .any(|node| node.role() == UiSemanticRole::Text && node.name().contains("Complete"))
        {
            return Err(
                "the native Hierarchy omitted its Complete-document deletion diagnostic".into(),
            );
        }
        self.hierarchy_click("Clear filter")?;
        self.hierarchy_click("+ Root")?;
        let root = self.selected_id()?;
        self.hierarchy_click("+ Child")?;
        let child = self.selected_id()?;
        self.owned_component_click("Add Component")?;
        self.owned_component_click(&format!("Add {CUSTOM_COMPONENT}"))?;
        self.owned_test_named_replace("label", "label", RETAINED_LABEL)?;
        self.owned_test_document("before", 10, 3, "large Hierarchy unrelated Inspector draft")?;
        let baseline = self.frame()?;
        let total = baseline
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the draft lost its source")?
            .editor
            .inspector
            .entities
            .len();
        let manifest = self
            .application
            .active_authoring_unavailable_record_manifest()
            .ok_or("creating native rows lost the preservation manifest")?;
        let mut placements = manifest.source_placements().iter();
        if manifest.entries() != original_manifest.entries()
            || !manifest
                .overlay_declarations()
                .eq(original_manifest.overlay_declarations())
            || !original_manifest
                .source_placements()
                .iter()
                .all(|original| placements.any(|placement| placement == original))
        {
            return Err("creating native Hierarchy rows changed unknown source records".into());
        }
        let unchanged = |workflow: &Self, phase: &str| -> Result<(), String> {
            if workflow.frame()?.workspace().active_scene != baseline.workspace().active_scene
                || read_source(&source)? != original
                || workflow
                    .application
                    .active_authoring_unavailable_record_manifest()
                    != Some(manifest.clone())
            {
                return Err(format!(
                    "{phase}: local native Hierarchy input changed source, exact selection or history"
                ));
            }
            workflow.owned_test_draft("label", Some(RETAINED_LABEL))
        };
        self.hierarchy_wheel(MouseScrollDelta::LineDelta(0.0, 10_000.0))?;
        self.hierarchy_matches(total, total)?;
        let first = self.hierarchy_visible_rows()?;
        let middle = first
            .get(first.len() / 2)
            .ok_or("the large Hierarchy has no visible source row")?;
        let name = format!("Select {middle}");
        let before = self.hierarchy_probe(UiSemanticRole::Button, &name)?;
        let physical_scale = f64::from(before.image.pixels_per_point)
            * f64::from(before.image.rect.max[1] - before.image.rect.min[1])
            / f64::from(before.frame.environment().logical_size().height());
        self.hierarchy_wheel_burst(&[
            MouseScrollDelta::LineDelta(0.0, -0.5),
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -24.0 * physical_scale)),
        ])?;
        let after = self.hierarchy_probe(UiSemanticRole::Button, &name)?;
        let displacement = before.rect.origin().y() - after.rect.origin().y();
        if (displacement - 48.0).abs() > 0.1 {
            return Err(format!(
                "native line/pixel wheel events before repaint moved {displacement} logical points; expected their sum, 48"
            ));
        }
        unchanged(&self, "native wheel burst before repaint")?;
        self.hierarchy_wheel(MouseScrollDelta::LineDelta(0.0, 10_000.0))?;
        let wide_filter = "wide filter ".repeat(25);
        self.hierarchy_replace_filter(&wide_filter)?;
        self.owned_test_select("label")?;
        self.owned_test_paint()?;
        self.hierarchy_filter_text(&wide_filter, false)?;
        self.owned_test_panel_pointer(
            PANEL,
            None,
            UiSemanticRole::TextInput,
            "Filter entities",
            0.0,
        )?;
        self.owned_test_mouse(ElementState::Pressed)?;
        self.owned_test_mouse(ElementState::Released)?;
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Commit("prefix ".into()))?;
        self.hierarchy_filter_text(&format!("prefix {wide_filter}"), false)?;
        unchanged(&self, "refocus long native filter at its displayed prefix")?;
        self.hierarchy_click("Clear filter")?;
        self.hierarchy_focus_filter()?;
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Preedit(
            LAST_DECLARATION.into(),
            Some((0, LAST_DECLARATION.len())),
        ))?;
        self.hierarchy_filter_text("", true)?;
        self.hierarchy_matches(total, total)?;
        if self.hierarchy_visible_rows()? != first {
            return Err(
                "Hierarchy filter preedit changed the presented matches before commit".into(),
            );
        }
        unchanged(&self, "Hierarchy filter preedit")?;
        self.owned_test_ime(Ime::Disabled)?;
        self.hierarchy_filter_text("", false)?;
        self.hierarchy_matches(total, total)?;
        if self.hierarchy_visible_rows()? != first {
            return Err("canceling Hierarchy composition changed the presented matches".into());
        }
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Commit(LAST_DECLARATION.into()))?;
        self.hierarchy_filter_text(LAST_DECLARATION, false)?;
        self.hierarchy_matches(1, total)?;
        if self.hierarchy_visible_rows()? != [LAST_DECLARATION.to_owned()] {
            return Err(
                "the committed native filter did not expose its one exact source declaration"
                    .into(),
            );
        }
        unchanged(&self, "filter hides the exact Session selection")?;
        self.hierarchy_click("Clear filter")?;
        self.hierarchy_filter_text("", false)?;
        self.hierarchy_matches(total, total)?;
        unchanged(&self, "clear native filter")?;
        self.hierarchy_wheel(MouseScrollDelta::LineDelta(0.0, -10_000.0))?;
        let last = self.hierarchy_visible_rows()?;
        if last == first || !last.iter().any(|id| id == LAST_DECLARATION) {
            return Err(
                "native Hierarchy wheel did not advance the visible declaration range".into(),
            );
        }
        // Created IDs use fresh entropy. Either endpoint must hide the selected declaration.
        if last.contains(&child) {
            self.hierarchy_wheel(MouseScrollDelta::LineDelta(0.0, 10_000.0))?;
            if self.hierarchy_visible_rows()?.contains(&child) {
                return Err(
                    "native wheel did not hide the selection across the full declaration range"
                        .into(),
                );
            }
        }
        unchanged(&self, "native wheel hides the exact Session selection")?;
        self.hierarchy_wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
            0.0,
            1_000_000.0,
        )))?;
        if self.hierarchy_visible_rows()? != first {
            return Err(
                "native pixel wheel did not restore the first visible declaration range".into(),
            );
        }
        unchanged(&self, "native pixel wheel returns to the first range")?;
        self.hierarchy_replace_filter(LAST_DECLARATION)?;
        self.hierarchy_click("Show selection")?;
        self.hierarchy_filter_text("", false)?;
        self.hierarchy_matches(total, total)?;
        if !self.hierarchy_visible_rows()?.contains(&child) {
            return Err("native Show selection did not reveal the actual Session selection".into());
        }
        unchanged(&self, "Show selection after filter and wheel")?;
        self.owned_test_apply("label")?;
        self.owned_test_document(
            RETAINED_LABEL,
            10,
            4,
            "explicit Apply after local Hierarchy input",
        )?;
        let saved_declarations = self.structural_intents()?;
        self.click("Save")?;
        self.settle_persistence()?;
        // The large projection can finish persistence after the preceding toolbar was painted.
        // Present its terminal result before requiring the real acknowledgement widget.
        self.owned_test_paint()?;
        if !matches!(self.frame()?.persistence().result(), Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
            || self.dirty()?
        {
            return Err(
                "Save did not persist the large native Hierarchy and explicit Inspector Apply"
                    .into(),
            );
        }
        let stored = read_source(&source)?;
        let entities = stored["payload"]["entities"]
            .as_array()
            .ok_or("the large saved source has no records")?;
        let originals = original["payload"]["entities"]
            .as_array()
            .ok_or("the large original source has no records")?;
        if entities.len() != originals.len() + 2
            || !originals.iter().all(|record| entities.contains(record))
            || !entities
                .iter()
                .any(|record| record["id"].as_str() == Some(root.as_str()))
        {
            return Err(
                "large native Save lost a source declaration or changed an original unknown record"
                    .into(),
            );
        }
        self.click("Dismiss persistence result")?;
        self.click("Reopen")?;
        self.settle_persistence()?;
        self.owned_test_paint()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) || self.structural_intents()? != saved_declarations
            || self.history()? != 0
            || self.dirty()?
            || read_source(&source)? != stored
            || self
                .application
                .active_authoring_unavailable_record_manifest()
                != Some(manifest)
        {
            return Err(
                "large native Reopen changed exact saved source, unavailable records or history"
                    .into(),
            );
        }
        self.click("Dismiss persistence result")?;
        self.hierarchy_replace_filter(&child)?;
        self.hierarchy_select(&child)?;
        self.hierarchy_click("Clear filter")?;
        self.owned_test_document(RETAINED_LABEL, 10, 0, "large native saved source Reopen")?;
        self.owned_test_draft("label", None)
    }

    fn run_hierarchy_filter(mut self, project: &Path) -> Result<u64, String> {
        self.hierarchy_ready()?;
        self.hierarchy_preserved_document(project, EditorDocumentKind::Scene)
            .map_err(|error| format!("large Scene Hierarchy: {error}"))?;
        self.hierarchy_replace_filter("prefab-anchor")?;
        self.hierarchy_open_prefab()?;
        self.hierarchy_filter_text("", false)?;
        self.hierarchy_preserved_document(project, EditorDocumentKind::Prefab)
            .map_err(|error| format!("large Prefab Hierarchy: {error}"))?;
        self.hierarchy_focus_filter()?;
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Preedit("retired filter".into(), Some((0, 14))))?;
        self.hierarchy_filter_text("", true)?;
        let previous = self.hierarchy_probe(UiSemanticRole::TextInput, "Filter entities")?;
        let token = self
            .native
            .frontend
            .owned_field_token(previous.panel, &previous.frame, &previous.key)
            .ok_or("the native Prefab filter has no live text lifetime before document switch")?;
        if !self.native.owned_text_focus_for_test() {
            return Err("the composing native filter lost focus before document switch".into());
        }
        self.click("Back to Scene")?;
        self.owned_test_paint()?;
        if self
            .native
            .frontend
            .owned_field_token(previous.panel, &previous.frame, &previous.key)
            .is_some()
            || self.native.owned_text_focus_for_test()
        {
            return Err("switching documents retained the old native Hierarchy text token".into());
        }
        let baseline = self.frame()?;
        let scene_source = source_path(project, EditorDocumentKind::Scene)?;
        let prefab_source = source_path(project, EditorDocumentKind::Prefab)?;
        let sources = [read_source(&scene_source)?, read_source(&prefab_source)?];
        let matches = self.hierarchy_visible_rows()?;
        self.owned_test_event(WindowEvent::Ime(Ime::Commit("late retired filter".into())))?;
        self.owned_test_paint()?;
        self.hierarchy_filter_text("", false)?;
        let replacement = self.hierarchy_focus_filter()?;
        let next_token = self
            .native
            .frontend
            .owned_field_token(replacement.panel, &replacement.frame, &replacement.key)
            .ok_or("the successor document filter has no fresh native text token")?;
        if token == next_token {
            return Err("the successor document reused the retired Hierarchy text lifetime".into());
        }
        self.owned_test_ime(Ime::Commit("late before successor IME handshake".into()))?;
        self.hierarchy_filter_text("", false)?;
        if self.frame()?.workspace().active_scene != baseline.workspace().active_scene
            || self.frame()?.workspace().documents != baseline.workspace().documents
            || self.hierarchy_visible_rows()? != matches
            || [read_source(&scene_source)?, read_source(&prefab_source)?] != sources
            || self.history()? != 0
            || self.dirty()?
        {
            return Err("a late retired filter commit changed source, matches, selection or history after document switch".into());
        }
        self.owned_test_ime(Ime::Enabled)?;
        self.owned_test_ime(Ime::Commit(LAST_DECLARATION.into()))?;
        self.hierarchy_filter_text(LAST_DECLARATION, false)?;
        if self.hierarchy_visible_rows()? != [LAST_DECLARATION.to_owned()] {
            return Err(
                "the successor Hierarchy filter did not accept its fresh native input lifetime"
                    .into(),
            );
        }
        Ok(self.presents)
    }
}
