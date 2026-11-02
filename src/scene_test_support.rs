use nara_scene::{
    PrefabEntityTarget, SceneEntityPlacement, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementParent,
};

/// Adds one fixture record and its explicit root placement in the same source transaction.
pub(crate) fn add_root_entity_patch(
    placements: &[SceneEntityPlacement],
    entity: SceneEntityRecord,
) -> ScenePatchDocument {
    let mut patch = ScenePatchDocument::set_placement(
        placements,
        PrefabEntityTarget::source(entity.id.clone()),
        ScenePlacement::last(ScenePlacementParent::Root),
    );
    patch
        .operations
        .insert(0, ScenePatchOperation::AddEntity { entity });
    patch
}
