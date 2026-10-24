use nara_ecs::{Component, Entity, Resource, World};
use nara_identity::{EntityLookup, SpawnedSceneInstance, WorldIdentityDomain};
use nara_reflect::ComponentRegistry;
use nara_scene::{
    SceneDocument, SceneEntityId, SceneEntityRecord, retire_and_despawn_scene_entity,
    retire_and_despawn_scene_instance,
};

fn id() -> SceneEntityId {
    SceneEntityId::new("root").unwrap()
}

fn spawn(
    world: &mut World,
    empty: bool,
    source: nara_scene::SourceDocumentKey,
    occurrence: &nara_scene::SceneOccurrenceKey,
) -> SpawnedSceneInstance {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    let document = if empty {
        SceneDocument::default()
    } else {
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id())])
    };
    let report = nara_scene::spawn_scene(
        world,
        &registry,
        &nara_scene::ExpandedSceneCandidate::plain(source, std::sync::Arc::new((document).clone())),
        occurrence,
        None,
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report.instance.unwrap()
}

fn resolve(world: &World, instance: &SpawnedSceneInstance) -> Entity {
    match instance.resolve(world, &id()) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected a live Scene entity, got {other:?}"),
    }
}

#[test]
fn empty_instances_end_by_receipt_without_scanning_entities() {
    let mut world = World::new();
    let instance = spawn(
        &mut world,
        true,
        scene_source_key("empty_instances_end_by_receipt_without_scanning_entities/source"),
        &scene_occurrence("empty_instances_end_by_receipt_without_scanning_entities/instance"),
    );
    assert!(instance.is_empty());
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .stats()
            .active_scene_instances,
        1
    );

    assert_eq!(
        retire_and_despawn_scene_instance(&mut world, &instance).unwrap(),
        0
    );
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .stats()
            .active_scene_instances,
        0
    );
    assert!(retire_and_despawn_scene_instance(&mut world, &instance).is_err());
}

#[test]
fn ending_one_instance_preserves_other_instances_and_runtime_history() {
    let mut world = World::new();
    let first = spawn(
        &mut world,
        false,
        scene_source_key(
            "ending_one_instance_preserves_other_instances_and_runtime_history/source",
        ),
        &scene_occurrence(
            "ending_one_instance_preserves_other_instances_and_runtime_history/first",
        ),
    );
    let second = spawn(
        &mut world,
        false,
        scene_source_key(
            "ending_one_instance_preserves_other_instances_and_runtime_history/source",
        ),
        &scene_occurrence(
            "ending_one_instance_preserves_other_instances_and_runtime_history/second",
        ),
    );
    let first_entity = resolve(&world, &first);
    let second_entity = resolve(&world, &second);

    assert_eq!(
        retire_and_despawn_scene_instance(&mut world, &first).unwrap(),
        1
    );
    assert!(world.get_entity(first_entity).is_err());
    assert_eq!(resolve(&world, &second), second_entity);
    assert!(matches!(
        first.resolve(&world, &id()),
        EntityLookup::Tombstoned(_)
    ));
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .stats()
            .claimed_scene_instances,
        2
    );

    let successor = spawn(
        &mut world,
        false,
        scene_source_key(
            "ending_one_instance_preserves_other_instances_and_runtime_history/source",
        ),
        &scene_occurrence(
            "ending_one_instance_preserves_other_instances_and_runtime_history/first",
        ),
    );
    let successor_entity = resolve(&world, &successor);
    assert!(retire_and_despawn_scene_instance(&mut world, &first).is_err());
    assert_eq!(resolve(&world, &successor), successor_entity);
    assert_eq!(resolve(&world, &second), second_entity);
}

#[test]
fn foreign_receipt_cannot_end_an_instance_with_equal_local_ids() {
    let mut first_world = World::new();
    let foreign = spawn(
        &mut first_world,
        false,
        scene_source_key("foreign_receipt_cannot_end_an_instance_with_equal_local_ids/source"),
        &scene_occurrence("foreign_receipt_cannot_end_an_instance_with_equal_local_ids/foreign"),
    );
    let mut world = World::new();
    let local = spawn(
        &mut world,
        false,
        scene_source_key("foreign_receipt_cannot_end_an_instance_with_equal_local_ids/source"),
        &scene_occurrence("foreign_receipt_cannot_end_an_instance_with_equal_local_ids/local"),
    );
    let entity = resolve(&world, &local);
    let baseline = world.resource::<WorldIdentityDomain>().stats();

    assert!(retire_and_despawn_scene_instance(&mut world, &foreign).is_err());
    assert_eq!(resolve(&world, &local), entity);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), baseline);
    assert!(matches!(
        foreign.resolve(&first_world, &id()),
        EntityLookup::Resolved(_)
    ));
}

#[test]
fn deleting_the_final_entity_does_not_end_the_instance() {
    let mut world = World::new();
    let instance = spawn(
        &mut world,
        false,
        scene_source_key("deleting_the_final_entity_does_not_end_the_instance/source"),
        &scene_occurrence("deleting_the_final_entity_does_not_end_the_instance/instance"),
    );
    let entity = resolve(&world, &instance);
    retire_and_despawn_scene_entity(&mut world, entity).unwrap();
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .stats()
            .active_scene_instances,
        1
    );

    assert_eq!(
        retire_and_despawn_scene_instance(&mut world, &instance).unwrap(),
        0
    );
    assert_eq!(
        world
            .resource::<WorldIdentityDomain>()
            .stats()
            .active_scene_instances,
        0
    );
}

#[derive(Component)]
#[component(on_remove = record_removal)]
struct NativeState;

#[derive(Resource, Default)]
struct Removals(usize);

fn record_removal(
    mut world: nara_ecs::bevy_ecs::world::DeferredWorld<'_>,
    _: nara_ecs::bevy_ecs::lifecycle::HookContext,
) {
    world.resource_mut::<Removals>().0 += 1;
}

#[test]
fn unsupported_native_lifecycle_rejects_before_identity_or_entity_retirement() {
    let mut world = World::new();
    world.init_resource::<Removals>();
    let instance = spawn(
        &mut world,
        false,
        scene_source_key(
            "unsupported_native_lifecycle_rejects_before_identity_or_entity_retirement/source",
        ),
        &scene_occurrence(
            "unsupported_native_lifecycle_rejects_before_identity_or_entity_retirement/instance",
        ),
    );
    let entity = resolve(&world, &instance);
    world.entity_mut(entity).insert(NativeState);
    let baseline = world.resource::<WorldIdentityDomain>().stats();

    let report = retire_and_despawn_scene_instance(&mut world, &instance).unwrap_err();
    assert!(report.has_errors());
    assert_eq!(resolve(&world, &instance), entity);
    assert!(world.get::<NativeState>(entity).is_some());
    assert_eq!(world.resource::<Removals>().0, 0);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), baseline);
}

fn scene_source_key(name: &str) -> nara_scene::SourceDocumentKey {
    nara_scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara_scene::SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn scene_occurrence(slot: &str) -> nara_scene::SceneOccurrenceKey {
    nara_scene::SceneOccurrenceKey::new("nara.scene.tests", slot).unwrap()
}
