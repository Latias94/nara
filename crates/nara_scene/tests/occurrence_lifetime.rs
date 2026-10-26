use std::sync::Arc;

use nara_asset::ProjectAssetDatabase;
use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::World;
use nara_identity::{EntityLookup, WorldIdentityDomain};
use nara_reflect::ComponentRegistry;
use nara_scene::{
    ExpandedSceneCandidate, SceneDocument, SceneEntityId, SceneEntityRecord, SceneEntitySource,
    SceneOccurrenceKey, SceneOccurrenceLimits, SceneProjection, SourceDocumentKey,
    SourceDocumentKind, retire_and_despawn_scene_entity, retire_and_despawn_scene_instance,
    spawn_scene,
};

fn source(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "683075b0-93d9-44b6-894d-87ab9230c77c".parse().unwrap(),
        name,
        SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn occurrence(slot: &str) -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("test.occurrence", slot).unwrap()
}

fn candidate(name: &str, empty: bool) -> ExpandedSceneCandidate {
    let document = if empty {
        SceneDocument::default()
    } else {
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("root").unwrap(),
        )])
    };
    ExpandedSceneCandidate::plain(source(name), Arc::new(document))
}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.freeze().unwrap();
    registry
}

#[test]
fn explicit_slots_separate_sources_and_duplicate_slot_refusal_precedes_allocation() {
    let registry = registry();
    let mut world = World::new();
    let first = candidate("first", false);
    let second = candidate("second", false);
    let first_receipt = spawn_scene(&mut world, &registry, &first, &occurrence("left"), None)
        .instance
        .unwrap();
    let second_receipt = spawn_scene(&mut world, &registry, &second, &occurrence("right"), None)
        .instance
        .unwrap();
    let root = SceneEntityId::new("root").unwrap();
    for (receipt, key, slot) in [
        (&first_receipt, first.source_key(), occurrence("left")),
        (&second_receipt, second.source_key(), occurrence("right")),
    ] {
        let EntityLookup::Resolved(entity) = receipt.resolve(&world, &root) else {
            panic!("active root");
        };
        let runtime_source = world.get::<SceneEntitySource>(entity).unwrap();
        assert_eq!(runtime_source.containing_source(), key);
        assert_eq!(runtime_source.occurrence(), &slot);
        assert_eq!(runtime_source.provenance().local_entity_id(), &root);
    }
    let before = world
        .iter_entities()
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    let stats = world.resource::<WorldIdentityDomain>().stats();
    let rejected = spawn_scene(&mut world, &registry, &second, &occurrence("left"), None);
    assert!(rejected.instance.is_none());
    assert!(
        rejected
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.occurrence-occupied")
    );
    assert_eq!(
        world
            .iter_entities()
            .map(|entity| entity.id())
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
}

#[test]
fn empty_and_individually_emptied_instances_hold_slots_until_exact_end() {
    let registry = registry();
    for empty in [true, false] {
        let mut world = World::new();
        let original = spawn_scene(
            &mut world,
            &registry,
            &candidate("old", empty),
            &occurrence("slot"),
            None,
        )
        .instance
        .unwrap();
        if !empty {
            let root = SceneEntityId::new("root").unwrap();
            let EntityLookup::Resolved(entity) = original.resolve(&world, &root) else {
                panic!("root");
            };
            retire_and_despawn_scene_entity(&mut world, entity).unwrap();
        }
        assert!(
            spawn_scene(
                &mut world,
                &registry,
                &candidate("other", true),
                &occurrence("slot"),
                None
            )
            .instance
            .is_none()
        );
        assert_eq!(
            retire_and_despawn_scene_instance(&mut world, &original).unwrap(),
            0
        );
        let successor = spawn_scene(
            &mut world,
            &registry,
            &candidate("new", false),
            &occurrence("slot"),
            None,
        )
        .instance
        .unwrap();
        assert_ne!(original.instance_id(), successor.instance_id());
        assert!(retire_and_despawn_scene_instance(&mut world, &original).is_err());
        let root = SceneEntityId::new("root").unwrap();
        assert!(matches!(
            successor.resolve(&world, &root),
            EntityLookup::Resolved(_)
        ));
        retire_and_despawn_scene_instance(&mut world, &successor).unwrap();
    }
}

#[test]
fn active_item_and_byte_budgets_reject_without_evicting_or_installing_identity() {
    let registry = registry();
    let mut world = World::new();
    world.insert_resource(SceneOccurrenceLimits::default().with_retained_bytes(ByteLimit::ONE));
    let rejected = spawn_scene(
        &mut world,
        &registry,
        &candidate("first", true),
        &occurrence("one"),
        None,
    );
    assert!(rejected.instance.is_none());
    assert!(!world.contains_resource::<WorldIdentityDomain>());
    world.insert_resource(SceneOccurrenceLimits::default().with_active_occurrences(ItemLimit::ONE));
    let first = spawn_scene(
        &mut world,
        &registry,
        &candidate("first", true),
        &occurrence("one"),
        None,
    )
    .instance
    .unwrap();
    let stats = world.resource::<WorldIdentityDomain>().stats();
    let rejected = spawn_scene(
        &mut world,
        &registry,
        &candidate("second", true),
        &occurrence("two"),
        None,
    );
    assert!(rejected.instance.is_none());
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
    retire_and_despawn_scene_instance(&mut world, &first).unwrap();
    assert!(
        spawn_scene(
            &mut world,
            &registry,
            &candidate("second", true),
            &occurrence("two"),
            None
        )
        .instance
        .is_some()
    );
}

#[test]
fn projection_epochs_replace_exact_occupancy_and_stale_end_cannot_release_successor() {
    let registry = registry();
    let mut world = World::new();
    let database = ProjectAssetDatabase::default();
    let mut projection = SceneProjection::new(occurrence("preview"));
    let old = projection
        .publish(&mut world, &registry, &candidate("old", true), &database)
        .instance
        .unwrap();
    let new = projection
        .publish(&mut world, &registry, &candidate("new", true), &database)
        .instance
        .unwrap();
    assert_eq!(old.instance_id(), new.instance_id());
    assert_ne!(old.domain_id(), new.domain_id());
    assert!(retire_and_despawn_scene_instance(&mut world, &old).is_err());
    assert!(
        spawn_scene(
            &mut world,
            &registry,
            &candidate("intruder", true),
            &occurrence("preview"),
            None
        )
        .instance
        .is_none()
    );
    assert_eq!(projection.clear(&mut world).unwrap(), 0);
    assert!(
        spawn_scene(
            &mut world,
            &registry,
            &candidate("new", true),
            &occurrence("preview"),
            None
        )
        .instance
        .is_some()
    );
}

#[test]
fn same_slot_in_two_worlds_is_independent_and_foreign_end_preserves_both() {
    let registry = registry();
    let mut first = World::new();
    let mut second = World::new();
    let candidate = candidate("source", true);
    let slot = occurrence("same");
    let a = spawn_scene(&mut first, &registry, &candidate, &slot, None)
        .instance
        .unwrap();
    let b = spawn_scene(&mut second, &registry, &candidate, &slot, None)
        .instance
        .unwrap();
    assert!(retire_and_despawn_scene_instance(&mut second, &a).is_err());
    assert!(
        spawn_scene(&mut second, &registry, &candidate, &slot, None)
            .instance
            .is_none()
    );
    retire_and_despawn_scene_instance(&mut first, &a).unwrap();
    retire_and_despawn_scene_instance(&mut second, &b).unwrap();
}

#[test]
fn exact_replacement_switches_source_in_the_same_slot_and_fences_old_receipt() {
    use nara_scene::advanced::{
        SceneProductTransactionLimits, replace_expanded_scene_with_product,
    };
    let registry = registry();
    let mut world = World::new();
    let slot = occurrence("replace");
    let old = spawn_scene(
        &mut world,
        &registry,
        &candidate("first", false),
        &slot,
        None,
    )
    .instance
    .unwrap();
    let replacement = candidate("second", false);
    let report = replace_expanded_scene_with_product(
        &mut world,
        &registry,
        &replacement,
        None,
        &old,
        SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
        &[],
        |_| {},
    );
    let new = report.instance.unwrap();
    let root = SceneEntityId::new("root").unwrap();
    let EntityLookup::Resolved(entity) = new.resolve(&world, &root) else {
        panic!("root");
    };
    let runtime_source = world.get::<SceneEntitySource>(entity).unwrap();
    assert_eq!(runtime_source.containing_source(), replacement.source_key());
    assert_eq!(runtime_source.occurrence(), &slot);
    assert!(retire_and_despawn_scene_instance(&mut world, &old).is_err());
    assert!(
        spawn_scene(
            &mut world,
            &registry,
            &candidate("third", true),
            &slot,
            None
        )
        .instance
        .is_none()
    );
    retire_and_despawn_scene_instance(&mut world, &new).unwrap();
    assert!(
        spawn_scene(
            &mut world,
            &registry,
            &candidate("third", true),
            &slot,
            None
        )
        .instance
        .is_some()
    );
}

#[test]
fn shrinking_limits_rejects_replacement_without_evicting_the_predecessor() {
    use nara_scene::advanced::{
        SceneProductTransactionLimits, replace_expanded_scene_with_product,
    };
    let registry = registry();
    let mut world = World::new();
    let old_source = candidate("old", false);
    let slot = occurrence("shrink");
    let old = spawn_scene(&mut world, &registry, &old_source, &slot, None)
        .instance
        .unwrap();
    world.insert_resource(SceneOccurrenceLimits::default().with_retained_bytes(ByteLimit::ONE));
    let stats = world.resource::<WorldIdentityDomain>().stats();
    let replacement = candidate("new", false);
    let rejected = replace_expanded_scene_with_product(
        &mut world,
        &registry,
        &replacement,
        None,
        &old,
        SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
        &[],
        |_| {},
    );
    assert!(rejected.instance.is_none());
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
    let root = SceneEntityId::new("root").unwrap();
    let EntityLookup::Resolved(entity) = old.resolve(&world, &root) else {
        panic!("old root");
    };
    let source = world.get::<SceneEntitySource>(entity).unwrap();
    assert_eq!(source.containing_source(), old_source.source_key());
    assert_eq!(source.occurrence(), &slot);
    retire_and_despawn_scene_instance(&mut world, &old).unwrap();
    world.insert_resource(SceneOccurrenceLimits::default());
    assert!(
        spawn_scene(&mut world, &registry, &replacement, &slot, None)
            .instance
            .is_some()
    );
}
