use nara::{
    scene::{
        AuthoringDocumentReadiness, PrefabEntityTarget, SceneEntityPlacement, SceneEntityRecord,
        ScenePlacementParent, ScenePlacementSlot,
    },
    tooling::SceneInspectorReadOnlyReason,
};
use serde_json::Value;

use super::{fixture::entity, *};

impl Workflow<'_> {
    pub(super) fn ordinary(&mut self, path: &str, kind: EditorDocumentKind) -> TestResult {
        let original = self.project.read(path)?;
        let baseline = self.state()?;
        let frame = self.frame()?;
        if baseline.kind != kind
            || baseline.undo != 0
            || baseline.redo != 0
            || baseline.dirty
            || frame
                .workspace()
                .document(baseline.document)
                .and_then(|tab| tab.authoring_readiness)
                != Some(AuthoringDocumentReadiness::Complete)
            || !frame
                .workspace()
                .active_scene
                .as_ref()
                .is_some_and(|scene| scene.editor.inspector.composition.generic_authoring)
        {
            return Err(format!(
                "the ordinary {kind:?} fixture is not clean Complete data-only authoring: {baseline:?}"
            ));
        }
        self.select(&entity("original"), false)?;
        let before_root = self.state()?;
        self.click(HIERARCHY, "+ Root")?;
        let root = self.selected()?;
        let after_root = self.expect_one_mutation(&before_root)?;
        let mut expected = before_root.clone();
        if expected
            .rows
            .insert(
                root.clone(),
                Some(ScenePlacement::last(ScenePlacementParent::Root)),
            )
            .is_some()
        {
            return Err("the actual + Root reused an original source identity".into());
        }
        self.expect_rows(&expected, 1, 0)?;
        self.history_roundtrip(&before_root, &after_root)?;
        self.select(&root, false)?;
        let before_child = self.state()?;
        self.click(HIERARCHY, "+ Child")?;
        let child = self.selected()?;
        self.expect_one_mutation(&before_child)?;
        if expected
            .rows
            .insert(child.clone(), Some(ScenePlacement::last(parent(&root))))
            .is_some()
        {
            return Err("the actual + Child reused an existing source identity".into());
        }
        self.expect_rows(&expected, 2, 0)?;
        self.select(&root, false)?;
        let before_move = self.state()?;
        self.click(HIERARCHY, "Parent")?;
        self.expect_rows(&before_move, before_move.undo, before_move.redo)?;
        self.click(HIERARCHY, "Parent entity: destination")?;
        let after_move = self.expect_one_mutation(&before_move)?;
        expected.rows.insert(
            root.clone(),
            Some(ScenePlacement::last(parent(&entity("destination")))),
        );
        self.expect_rows(&expected, 3, 0)?;
        self.history_roundtrip(&before_move, &after_move)?;
        self.select(&root, false)?;
        let before_unparent = self.state()?;
        self.click(HIERARCHY, "Parent")?;
        self.click(HIERARCHY, "Parent: Root")?;
        self.expect_one_mutation(&before_unparent)?;
        expected.rows.insert(
            root.clone(),
            Some(ScenePlacement::last(ScenePlacementParent::Root)),
        );
        self.expect_rows(&expected, 4, 0)?;
        let before_order = self.state()?;
        self.click(HIERARCHY, "Order")?;
        self.click(HIERARCHY, "Order: Before original")?;
        let after_order = self.expect_one_mutation(&before_order)?;
        expected.rows.insert(
            root.clone(),
            Some(ScenePlacement {
                parent: ScenePlacementParent::Root,
                slot: ScenePlacementSlot::Before(PrefabEntityTarget::source(entity("original"))),
            }),
        );
        self.expect_rows(&expected, 5, 0)?;
        self.history_roundtrip(&before_order, &after_order)?;
        let arranged = self.state()?;
        self.click(HIERARCHY, "Remove Entity")?;
        self.expect_rows(&arranged, 5, 0)?;
        self.click(HIERARCHY, "Cancel removal")?;
        self.expect_rows(&arranged, 5, 0)?;
        if self.selected()? != root {
            return Err("cancelling actual removal changed the original selection".into());
        }
        self.click(HIERARCHY, "Remove Entity")?;
        self.click(HIERARCHY, "Confirm removal")?;
        let removed = self.expect_one_mutation(&arranged)?;
        self.expect_rows(&baseline, 6, 0)?;
        if removed.rows.contains_key(&root) || removed.rows.contains_key(&child) {
            return Err("actual removal did not delete the complete source-local subtree".into());
        }
        self.history_roundtrip(&arranged, &removed)?;
        self.click(SHELL, "Undo")?;
        self.expect_rows(&arranged, 5, 1)?;
        if self.project.read(path)? != original {
            return Err("unsaved native structural edits rewrote the original source".into());
        }
        let expected = self.state()?;
        let saved = self.save_reopen(path, &expected)?;
        expect_saved_source(
            &original,
            &saved,
            &[
                (
                    root.clone(),
                    expected
                        .rows
                        .get(&root)
                        .cloned()
                        .flatten()
                        .ok_or("the arranged root lost placement")?,
                ),
                (
                    child.clone(),
                    expected
                        .rows
                        .get(&child)
                        .cloned()
                        .flatten()
                        .ok_or("the arranged child lost placement")?,
                ),
            ],
        )?;
        self.select(&child, false)?;
        Ok(())
    }

    pub(super) fn preserve_unknown(&mut self) -> TestResult {
        let original = self.project.read(fixture::PRESERVED)?;
        let baseline = self.state()?;
        self.select(&entity("opaque"), false)?;
        self.expect_unknown_record()?;
        if self.enabled(HIERARCHY, "Remove Entity")? {
            return Err(
                "unknown source data incorrectly admits destructive generic removal".into(),
            );
        }
        let before_create = self.state()?;
        self.click(HIERARCHY, "+ Root")?;
        let created = self.selected()?;
        let after_create = self.expect_one_mutation(&before_create)?;
        let mut expected = baseline.clone();
        expected.rows.insert(
            created.clone(),
            Some(ScenePlacement::last(ScenePlacementParent::Root)),
        );
        self.expect_rows(&expected, 1, 0)?;
        self.history_roundtrip(&before_create, &after_create)?;
        self.select(&created, false)?;
        let before_move = self.state()?;
        self.click(HIERARCHY, "Parent")?;
        self.click(HIERARCHY, "Parent entity: opaque")?;
        let after_move = self.expect_one_mutation(&before_move)?;
        let placement = ScenePlacement::last(parent(&entity("opaque")));
        expected
            .rows
            .insert(created.clone(), Some(placement.clone()));
        self.expect_rows(&expected, 2, 0)?;
        self.history_roundtrip(&before_move, &after_move)?;
        if self.project.read(fixture::PRESERVED)? != original {
            return Err("unsaved native edits changed the opaque source file".into());
        }
        let expected = self.state()?;
        let saved = self.save_reopen(fixture::PRESERVED, &expected)?;
        expect_saved_source(&original, &saved, &[(created, placement)])?;
        self.select(&entity("opaque"), false)?;
        self.expect_unknown_record()
    }

    fn expect_unknown_record(&self) -> TestResult {
        let frame = self.frame()?;
        let view = frame
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.editor.inspector.selected_entity_view.as_ref())
            .ok_or("the retained unknown record has no source Inspector view")?;
        if !view.components.iter().any(|component| {
            component.component.as_str() == "consumer.drawn.UnknownRecord"
                && component.access.read_only_reason()
                    == Some(SceneInspectorReadOnlyReason::UnknownSchema)
        }) {
            return Err("the original unknown component was lost or silently interpreted".into());
        }
        Ok(())
    }
}

fn parent(entity: &SceneEntityId) -> ScenePlacementParent {
    ScenePlacementParent::Entity(PrefabEntityTarget::source(entity.clone()))
}

fn expect_saved_source(
    original: &[u8],
    saved: &[u8],
    created: &[(SceneEntityId, ScenePlacement)],
) -> TestResult {
    let mut expected: Value =
        serde_json::from_slice(original).map_err(|error| error.to_string())?;
    for (id, placement) in created {
        expected["payload"]["entities"]
            .as_array_mut()
            .ok_or("the source has no entity records")?
            .push(
                serde_json::to_value(SceneEntityRecord::new(id.clone()))
                    .map_err(|error| error.to_string())?,
            );
        expected["payload"]["placements"]
            .as_array_mut()
            .ok_or("the source has no placement declarations")?
            .push(
                serde_json::to_value(SceneEntityPlacement::new(
                    PrefabEntityTarget::source(id.clone()),
                    placement.clone(),
                ))
                .map_err(|error| error.to_string())?,
            );
    }
    expected["payload"]["entities"]
        .as_array_mut()
        .ok_or("the source has no entity records")?
        .sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    let actual: Value = serde_json::from_slice(saved).map_err(|error| error.to_string())?;
    if actual != expected {
        return Err(format!(
            "native Save/Reopen changed source beyond the exact authored structure: expected {expected}; actual {actual}"
        ));
    }
    Ok(())
}
