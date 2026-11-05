use nara::{
    project_host::EditorUiAuthoringFrame,
    scene::{
        PrefabDocumentCandidate, PrefabEntityOrigin, PrefabEntityTarget, SceneEntityPlacement,
        SceneEntityRecord, ScenePlacementParent, ScenePlacementSlot,
    },
    ui::__private::{
        UiCatalogNodeKind, UiCreateParameters, UiPreparedStructure, UiStructureIntent,
    },
};
use nara_ui_kernel::UiActionId;
use serde_json::Value;

use super::{fixture::entity, *};

const CREATED_LABEL: &str = "Native 确认";
const CREATED_ACTION: &str = "ui.native.confirm";

impl Workflow<'_> {
    fn ui(&self) -> TestResult<Arc<EditorUiAuthoringFrame>> {
        self.native
            .application
            .current_ui_authoring_frame()
            .ok_or_else(|| {
                "the actual active UI Prefab has no original UI authoring projection".into()
            })
    }

    fn choose_ui_placement(
        &mut self,
        parent: &SceneEntityId,
        moving: Option<&SceneEntityId>,
    ) -> TestResult<ScenePlacement> {
        let parent_target =
            ScenePlacementParent::Entity(PrefabEntityTarget::source(parent.clone()));
        let placement = self
            .ui()?
            .projection()
            .map_err(|error| error.to_string())?
            .structure_placement_candidates(moving)
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|candidate| candidate.placement().parent == parent_target)
            .next()
            .ok_or("the original UI owner has no placement under the requested parent")?
            .placement()
            .clone();
        let anchor = |target: &PrefabEntityTarget| match &target.entity {
            PrefabEntityOrigin::SourceEntity(entity) if target.through.is_empty() => {
                Ok(entity.to_string())
            }
            _ => Err("a source-local placement unexpectedly names an expanded anchor"),
        };
        let slot = match &placement.slot {
            ScenePlacementSlot::First => "first".into(),
            ScenePlacementSlot::Last => "last".into(),
            ScenePlacementSlot::Before(target) => format!("before {}", anchor(target)?),
            ScenePlacementSlot::After(target) => format!("after {}", anchor(target)?),
        };
        self.click(HIERARCHY, "Choose placement")?;
        self.click(HIERARCHY, &format!("Placement: {parent} / {slot}"))?;
        Ok(placement)
    }

    pub(super) fn projection_replacement(
        &mut self,
        scene: EditorDocumentId,
        ui_document: EditorDocumentId,
    ) -> TestResult {
        self.switch_document(ui_document)?;
        let ui_before = self.state()?;
        self.switch_document(scene)?;
        let source_before = self.state()?;
        self.replace_text("Filter entities", "original")?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Preedit(
            "old preedit".into(),
            Some((0, 11)),
        )))?;
        self.paint()?;
        let previous = self.panel_frame(HIERARCHY)?;
        let old_surface = previous.environment().surface_id();
        self.switch_document(ui_document)?;
        let replacement = self.panel_frame(HIERARCHY)?;
        if replacement.environment().surface_id() == old_surface
            || Arc::ptr_eq(&previous, &replacement)
            || self
                .native
                .graphics
                .as_ref()
                .and_then(|gpu| gpu.journal(self.panel_id(HIERARCHY).ok()?))
                .is_some_and(|journal| journal.latest(old_surface).is_ok())
        {
            return Err(
                "switching to UI did not retire the original source projection and presented route"
                    .into(),
            );
        }
        self.event(EditorPanelPlatformEvent::Ime(Ime::Commit(
            "late old preedit".into(),
        )))?;
        if self.state()? != ui_before {
            return Err(format!(
                "late source-filter text or keyboard input changed the replacement UI document: before={ui_before:?}; after={:?}",
                self.state()?
            ));
        }
        self.select(&entity("ui-root"), true)?;
        self.key(Key::Named(NamedKey::Enter))?;
        if self.state()? != ui_before {
            return Err(
                "replacement UI keyboard activation published an old source-filter operation"
                    .into(),
            );
        }
        self.switch_document(scene)?;
        if !self.field_value("Filter entities")?.is_empty()
            || self.enabled(HIERARCHY, "Clear filter")?
            || self.state()? != source_before
        {
            return Err("returning to ordinary source retained retired filter/preedit or changed source/history".into());
        }
        let point = self.point(HIERARCHY, UiSemanticRole::Button, "+ Root")?;
        self.pointer(point)?;
        self.mouse(ElementState::Pressed)?;
        self.replace_source_while_pressed(ui_document)?;
        self.mouse(ElementState::Released)?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Commit(
            "late captured commit".into(),
        )))?;
        self.paint()?;
        if self.state()? != ui_before {
            return Err("a pointer release from the retired source projection mutated the replacement UI source".into());
        }
        self.switch_document(scene)?;
        if self.state()? != source_before {
            return Err(
                "an old captured + Root activation reached the replacement source projection"
                    .into(),
            );
        }
        self.switch_document(ui_document)
    }

    pub(super) fn specialized_ui(&mut self) -> TestResult {
        let created_text = format!("{}x", "文".repeat(1365));
        if created_text.len() != 4096 {
            return Err("the native text recipe must cover the full UTF-8 byte capacity".into());
        }
        let original = self.project.read(fixture::UI)?;
        let baseline = self.state()?;
        if baseline.kind != EditorDocumentKind::Prefab
            || baseline.undo != 0
            || baseline.redo != 0
            || baseline.dirty
            || !self.ui()?.structure_editing_enabled()
        {
            return Err("the independent data-only source workspace must admit complete original UI creation recipes; its UI structure authority is unavailable".into());
        }
        self.select(&entity("ui-root"), true)?;
        if self.enabled(HIERARCHY, "Delete subtree")? || self.enabled(HIERARCHY, "Move subtree")? {
            return Err(
                "the specialized controls admitted moving or deleting the protected UI root".into(),
            );
        }
        self.click(HIERARCHY, "Add UI node")?;
        self.click(HIERARCHY, "Kind: Text")?;
        self.local_field_enter_keeps_focus("Text", "Cancelled native draft")?;
        self.event(EditorPanelPlatformEvent::Ime(Ime::Preedit(
            "cancel preedit".into(),
            Some((0, 14)),
        )))?;
        self.paint()?;
        if self.enabled(HIERARCHY, "Apply structure change")? {
            return Err(
                "the original UI recipe admits Apply while its local text is composing".into(),
            );
        }
        self.click(HIERARCHY, "Cancel structure change")?;
        if !self.enabled(HIERARCHY, "Add UI node")? {
            return Err(
                "cancelling the composing UI form did not return to the structure tree".into(),
            );
        }
        if self.state()? != baseline || self.project.read(fixture::UI)? != original {
            return Err("cancelling the actual UI creation form changed source, revision, history or persistence".into());
        }
        self.click(HIERARCHY, "Add UI node")?;
        self.click(HIERARCHY, "Choose placement")?;
        self.key(Key::Named(NamedKey::Escape))?;
        if !self.enabled(HIERARCHY, "Apply structure change")? {
            return Err("native Escape did not close the placement chooser while retaining its valid recipe form".into());
        }
        self.key(Key::Named(NamedKey::Escape))?;
        if self.state()? != baseline || !self.enabled(HIERARCHY, "Add UI node")? {
            return Err(
                "native Escape did not cancel the original UI recipe without a history mutation"
                    .into(),
            );
        }
        let expected_source = PrefabDocumentCandidate::decode_json_bytes(&original)
            .map_err(|error| error.to_string())?
            .into_composition_source()
            .map_err(|error| error.to_string())?;
        let mut expected_json: Value = serde_json::from_str(
            &expected_source
                .to_json_string()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let schema = nara::reflect::AuthoringSchemaBundle::from_json_bytes(
            &self.project.read("schema/authoring.bundle.json")?,
        )
        .map_err(|error| error.to_string())?;
        let mut created = Vec::new();
        for kind in UiCatalogNodeKind::ALL {
            let before = self.state()?;
            self.click(HIERARCHY, "Add UI node")?;
            self.click(HIERARCHY, &format!("Kind: {}", kind.label()))?;
            let placement = self.choose_ui_placement(&entity("ui-root"), None)?;
            let parameters = match kind {
                UiCatalogNodeKind::Container => UiCreateParameters::Container,
                UiCatalogNodeKind::Text => {
                    self.replace_text("Text", &created_text)?;
                    UiCreateParameters::Text {
                        content: created_text.clone(),
                    }
                }
                UiCatalogNodeKind::Image => {
                    if self.enabled(HIERARCHY, "Apply structure change")? {
                        return Err(
                            "the actual Image recipe applied without an original image choice"
                                .into(),
                        );
                    }
                    self.click(HIERARCHY, "Choose image")?;
                    self.click(HIERARCHY, "Image: textures/player.png")?;
                    let ui = self.ui()?;
                    let choices = ui.image_choices().map_err(|error| error.to_string())?;
                    let selection = choices
                        .choices()
                        .iter()
                        .find(|choice| choice.project_path() == "textures/player.png")
                        .ok_or("the real project image catalog lost the chosen image")?
                        .selection(choices.generation());
                    UiCreateParameters::Image {
                        selection: Some(selection),
                    }
                }
                UiCatalogNodeKind::Button => {
                    if self.enabled(HIERARCHY, "Apply structure change")? {
                        return Err(
                            "the actual Button recipe applied without an action identity".into(),
                        );
                    }
                    self.replace_text("Label", CREATED_LABEL)?;
                    self.replace_text("Action", CREATED_ACTION)?;
                    UiCreateParameters::Button {
                        label: CREATED_LABEL.into(),
                        action: Some(
                            UiActionId::new(CREATED_ACTION).map_err(|error| error.to_string())?,
                        ),
                    }
                }
            };
            let ui = self.ui()?;
            let UiPreparedStructure::Create { recipe, .. } = ui
                .projection()
                .map_err(|error| error.to_string())?
                .prepare_structure(
                    &UiStructureIntent::Create {
                        parameters,
                        placement: placement.clone(),
                    },
                    Some(ui.image_choices().map_err(|error| error.to_string())?),
                )
                .map_err(|error| error.to_string())?
            else {
                return Err(
                    "the original UI owner did not prepare a complete entity recipe".into(),
                );
            };
            self.click(HIERARCHY, "Apply structure change")?;
            let after = self.expect_one_mutation(&before)?;
            let id = self.selected()?;
            let mut expected = before.clone();
            if expected
                .rows
                .insert(id.clone(), Some(placement.clone()))
                .is_some()
            {
                return Err("the actual UI recipe reused an original source identity".into());
            }
            self.expect_rows(&expected, before.undo + 1, 0)?;
            let ui = self.ui()?;
            let row = ui
                .projection()
                .map_err(|error| error.to_string())?
                .hierarchy()
                .iter()
                .find(|node| node.entity() == &id)
                .ok_or("the created UI recipe has no original hierarchy row")?;
            if row.kind().label() != kind.label() || row.parent() != Some(&entity("ui-root")) {
                return Err("the actual source model did not project the requested complete UI recipe and placement".into());
            }
            let mut record = SceneEntityRecord::new(id.clone());
            record.components = recipe
                .components()
                .iter()
                .map(|(component, value)| {
                    let owner = schema
                        .owners()
                        .iter()
                        .find(|owner| {
                            owner
                                .current()
                                .components
                                .iter()
                                .any(|entry| entry.id == *component)
                        })
                        .ok_or("the UI recipe component has no admitted source Schema owner")?;
                    Ok((
                        component.clone(),
                        value.clone().with_owner(owner.owner_id().clone()),
                    ))
                })
                .collect::<TestResult<_>>()?;
            expected_json["payload"]["entities"]
                .as_array_mut()
                .ok_or("the UI source lost its entity records")?
                .push(serde_json::to_value(record).map_err(|error| error.to_string())?);
            expected_json["payload"]["placements"]
                .as_array_mut()
                .ok_or("the UI source lost its placements")?
                .push(
                    serde_json::to_value(SceneEntityPlacement::new(
                        PrefabEntityTarget::source(id.clone()),
                        placement,
                    ))
                    .map_err(|error| error.to_string())?,
                );
            self.history_roundtrip(&before, &after)?;
            created.push(id);
            if self.project.read(fixture::UI)? != original {
                return Err("a UI recipe rewrote source before actual Save".into());
            }
        }
        let container = created
            .first()
            .cloned()
            .ok_or("no Container recipe was created")?;
        let text = created
            .get(1)
            .cloned()
            .ok_or("no Text recipe was created")?;
        self.select(&text, true)?;
        let before_move = self.state()?;
        self.click(HIERARCHY, "Move subtree")?;
        let text_placement = self.choose_ui_placement(&container, Some(&text))?;
        self.click(HIERARCHY, "Apply structure change")?;
        let after_move = self.expect_one_mutation(&before_move)?;
        let mut expected = before_move.clone();
        expected
            .rows
            .insert(text.clone(), Some(text_placement.clone()));
        self.expect_rows(&expected, 5, 0)?;
        self.history_roundtrip(&before_move, &after_move)?;
        let declaration = serde_json::to_value(SceneEntityPlacement::new(
            PrefabEntityTarget::source(text.clone()),
            text_placement,
        ))
        .map_err(|error| error.to_string())?;
        let placements = expected_json["payload"]["placements"]
            .as_array_mut()
            .ok_or("the UI source lost placement declarations")?;
        let old = placements
            .iter_mut()
            .find(|entry| entry["target"]["entity"]["entity"] == text.as_str())
            .ok_or("the expected Text declaration is absent")?;
        *old = declaration;
        self.select(&container, true)?;
        let arranged = self.state()?;
        self.click(HIERARCHY, "Delete subtree")?;
        self.click(HIERARCHY, "Cancel structure change")?;
        if self.state()? != arranged {
            return Err(
                "cancelled UI subtree deletion changed the original source or history".into(),
            );
        }
        self.click(HIERARCHY, "Delete subtree")?;
        self.click(HIERARCHY, "Apply structure change")?;
        let deleted = self.expect_one_mutation(&arranged)?;
        let mut expected = arranged.clone();
        expected.rows.remove(&container);
        expected.rows.remove(&text);
        self.expect_rows(&expected, 6, 0)?;
        self.history_roundtrip(&arranged, &deleted)?;
        self.click(SHELL, "Undo")?;
        self.expect_rows(&arranged, 5, 1)?;
        let expected = self.state()?;
        let saved = self.save_reopen(fixture::UI, &expected)?;
        expected_json["payload"]["entities"]
            .as_array_mut()
            .ok_or("the UI source lost entity records")?
            .sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
        let actual: Value = serde_json::from_slice(&saved).map_err(|error| error.to_string())?;
        if actual != expected_json {
            return Err(
                "native UI Save/Reopen changed original records or complete Schema-owned recipes"
                    .into(),
            );
        }
        self.select(&text, true)?;
        let ui = self.ui()?;
        if !ui
            .projection()
            .map_err(|error| error.to_string())?
            .hierarchy()
            .iter()
            .any(|node| node.entity() == &text && node.parent() == Some(&container))
        {
            return Err("Reopen lost the complete specialized UI subtree placement".into());
        }
        self.refuse_conflicting_source_order(&saved)
    }

    fn refuse_conflicting_source_order(&mut self, saved: &[u8]) -> TestResult {
        let original = self.state()?;
        self.click(HIERARCHY, "Add UI node")?;
        self.click(HIERARCHY, "Kind: Container")?;
        self.click(HIERARCHY, "Choose placement")?;
        self.click(HIERARCHY, "Placement: ui-root / after image-node")?;
        self.click(HIERARCHY, "Apply structure change")?;
        self.expect_one_mutation(&original)?;
        let container = self.selected()?;
        let before_text = self.state()?;
        self.click(HIERARCHY, "Add UI node")?;
        self.click(HIERARCHY, "Kind: Text")?;
        self.click(HIERARCHY, "Choose placement")?;
        self.click(
            HIERARCHY,
            &format!("Placement: ui-root / after {container}"),
        )?;
        self.click(HIERARCHY, "Apply structure change")?;
        self.expect_one_mutation(&before_text)?;
        self.select(&container, true)?;
        let before = self.state()?;
        self.click(HIERARCHY, "Move subtree")?;
        self.choose_ui_placement(&entity("image-node"), Some(&container))?;
        self.click(HIERARCHY, "Apply structure change")?;
        if self.state()? != before
            || self.project.read(fixture::UI)? != saved
            || !self
                .frame()?
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == "scene.patch-local-layout-invalid")
            || !self
                .panel_frame(HIERARCHY)?
                .semantics()
                .iter()
                .any(|node| node.name().starts_with("UI structure change was refused:"))
        {
            return Err("a conflicting source-order move did not report exact refusal without changing source or history".into());
        }
        self.click(HIERARCHY, "Cancel structure change")?;
        self.click(SHELL, "Undo")?;
        self.click(SHELL, "Undo")?;
        self.expect_rows(&original, 0, 2)?;
        if self.state()?.dirty || self.project.read(fixture::UI)? != saved {
            return Err(
                "undoing the source-order refusal fixture did not restore the saved source".into(),
            );
        }
        Ok(())
    }
}
