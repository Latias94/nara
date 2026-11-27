use std::collections::{BTreeMap, btree_map::Entry};

use nara_scene::authoring::{SceneDocument, SceneEntity};

use super::{EditorGesture, RowSlot, ScenePatch, SceneProjection};

pub(super) struct ActiveGesture {
    pub(super) owner: EditorGesture,
    baseline: GestureBaseline,
}

enum GestureBaseline {
    Rows(BTreeMap<RowSlot, SceneEntity>),
    Document {
        scene: SceneDocument,
        projection: SceneProjection,
    },
}

impl ActiveGesture {
    pub(super) fn new(owner: EditorGesture) -> Self {
        Self {
            owner,
            baseline: GestureBaseline::Rows(BTreeMap::new()),
        }
    }

    pub(super) fn changed(&self, current: &SceneDocument) -> bool {
        match &self.baseline {
            GestureBaseline::Rows(rows) => !rows.is_empty(),
            GestureBaseline::Document { scene, .. } => scene != current,
        }
    }

    pub(super) fn record_rows(
        &mut self,
        scene: &SceneDocument,
        changes: &[(RowSlot, SceneEntity)],
    ) {
        let GestureBaseline::Rows(rows) = &mut self.baseline else {
            return;
        };
        for (slot, row) in changes {
            match rows.entry(*slot) {
                Entry::Vacant(entry) => {
                    entry.insert(scene.entities[slot.authored].clone());
                }
                Entry::Occupied(entry) if entry.get() == row => {
                    entry.remove();
                }
                Entry::Occupied(_) => {}
            }
        }
    }

    pub(super) fn record_document(
        &mut self,
        mut scene: SceneDocument,
        mut projection: SceneProjection,
    ) {
        if let GestureBaseline::Rows(rows) = &mut self.baseline {
            projection.replace_rows(&mut scene, std::mem::take(rows));
            self.baseline = GestureBaseline::Document { scene, projection };
        }
    }

    pub(super) fn record_assets(&mut self, scene: &SceneDocument, projection: &SceneProjection) {
        if matches!(self.baseline, GestureBaseline::Rows(_)) {
            self.record_document(scene.clone(), projection.clone());
        }
    }

    pub(super) fn finish(self, current: &SceneDocument) -> ScenePatch {
        match self.baseline {
            GestureBaseline::Rows(rows) => ScenePatch::from_rows(
                rows.into_iter()
                    .map(|(slot, before)| (before, current.entities[slot.authored].clone())),
            ),
            GestureBaseline::Document { scene, .. } => ScenePatch::between(&scene, current),
        }
    }

    pub(super) fn restore(self, current: &mut SceneDocument, preview: &mut SceneProjection) {
        match self.baseline {
            GestureBaseline::Rows(rows) => preview.replace_rows(current, rows),
            GestureBaseline::Document { scene, projection } => {
                *current = scene;
                *preview = projection;
            }
        }
    }
}
