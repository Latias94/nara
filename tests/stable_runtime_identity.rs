use nara::ecs::World;
use nara::{
    core::ItemLimit,
    gameplay::GameplayCommandTarget,
    identity::{EntityLookup, SceneEntityId, resolve_in_world},
    reflect::ComponentRegistry,
    scene::{SceneDocument, SceneEntityRecord, spawn_scene},
    tooling::WorldIdentitySnapshot,
};

#[test]
fn scene_commands_and_tooling_share_one_world_scoped_identity_contract() {
    let mut registry = ComponentRegistry::new();
    registry
        .freeze()
        .expect("component registry should freeze before scene spawn");
    let entity_id = SceneEntityId::new("player").unwrap();
    let document = SceneDocument::new([SceneEntityRecord::new(entity_id.clone())]);
    let mut world = World::new();
    world.spawn_empty();

    let first_report = spawn_scene(&mut world, &registry, &document);
    assert!(!first_report.diagnostics.has_errors());
    let first = first_report
        .instance
        .expect("the first scene instance should spawn");
    let second_report = spawn_scene(&mut world, &registry, &document);
    assert!(!second_report.diagnostics.has_errors());
    let second = second_report
        .instance
        .expect("the second scene instance should spawn");

    let first_reference = first.runtime_reference(&entity_id).unwrap();
    let second_reference = second.runtime_reference(&entity_id).unwrap();
    assert_ne!(first_reference, second_reference);

    let first_target = GameplayCommandTarget::Entity(first_reference);
    let first_entity = match first_target.resolve_entity(&world) {
        Some(EntityLookup::Resolved(entity)) => entity,
        lookup => panic!("expected the command target to resolve, got {lookup:?}"),
    };
    let second_entity = match second.resolve(&world, &entity_id) {
        EntityLookup::Resolved(entity) => entity,
        lookup => panic!("expected the second scene entity to resolve, got {lookup:?}"),
    };
    assert_ne!(first_entity, second_entity);

    let snapshot = WorldIdentitySnapshot::capture(&world, ItemLimit::ONE).unwrap();
    assert_eq!(snapshot.domain_id, Some(first.domain_id()));
    assert_eq!(snapshot.total_entity_count, 3);
    assert_eq!(snapshot.identified_entity_count, 2);
    assert_eq!(snapshot.runtime_only_entity_count, 1);
    assert_eq!(snapshot.returned_locator_count, 1);
    assert_eq!(snapshot.omitted_locator_count, 1);
    assert_eq!(snapshot.locators.len(), 1);
    assert!(matches!(
        resolve_in_world(&world, &snapshot.locators[0]),
        EntityLookup::Resolved(_)
    ));
}

#[cfg(feature = "serde")]
#[test]
fn command_target_serialization_contains_stable_timeline_identity_only() {
    use nara::identity::{RuntimeEntityReference, SceneInstanceId};

    let target = GameplayCommandTarget::Entity(RuntimeEntityReference::scene(
        SceneInstanceId::new(7).unwrap(),
        SceneEntityId::new("player").unwrap(),
    ));

    assert_eq!(
        serde_json::to_value(&target).unwrap(),
        serde_json::json!({
            "Entity": {
                "kind": "scene",
                "instance": 7,
                "entity": "player"
            }
        })
    );
}
