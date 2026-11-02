#![allow(dead_code)]

use nara::scene::{
    PrefabEntityTarget, SceneEntityPlacement, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement,
};

pub fn add_entity_patch(
    placements: &[SceneEntityPlacement],
    entity: SceneEntityRecord,
    placement: ScenePlacement,
) -> ScenePatchDocument {
    let target = PrefabEntityTarget::source(entity.id.clone());
    let mut patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity { entity }]);
    patch
        .operations
        .extend(ScenePatchDocument::set_placement(placements, target, placement).operations);
    patch
}
