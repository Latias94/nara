//! Private storage counterpressure, without Scene materialization or language execution.
//!
//! Counts describe logical inspections and retained records in these two private stores. They
//! exclude archetype metadata checks and map lookup costs, and do not measure throughput,
//! allocation, memory usage, or the production Scene data carrier.

#[path = "support/behavior_storage.rs"]
mod storage;

use std::collections::BTreeMap;

use nara_ecs::{Component, World};
use nara_reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use nara_scene::SceneComponentRecord;
use storage::StorageKind;

const KINDS: [StorageKind; 2] = [StorageKind::Envelope, StorageKind::Rows];

#[derive(Component)]
struct NativeMarker;

fn records(components: &[ComponentTypeId]) -> BTreeMap<ComponentTypeId, SceneComponentRecord> {
    components
        .iter()
        .map(|component| {
            (
                component.clone(),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(1)),
            )
        })
        .collect()
}

#[test]
fn selective_queries_count_carriers_or_target_schema_rows_without_native_only_entities() {
    let mover = ComponentTypeId::new("trial.behavior.Mover");
    let sensor = ComponentTypeId::new("trial.behavior.Sensor");
    for kind in KINDS {
        let mut world = World::new();
        let actor = world.spawn(NativeMarker).id();
        let friend = world.spawn(NativeMarker).id();
        let mut members = vec![actor, friend];
        kind.install(&mut world, actor, records(&[mover.clone(), sensor.clone()]));
        for _ in 0..32 {
            let entity = world.spawn_empty().id();
            kind.install(&mut world, entity, records(std::slice::from_ref(&sensor)));
            members.push(entity);
        }
        for _ in 0..256 {
            world.spawn(NativeMarker);
        }

        let query = kind.query(&world, &mover);
        assert_eq!(query.entities, vec![actor]);
        assert_eq!(
            query.inspected,
            match kind {
                StorageKind::Envelope => 33,
                StorageKind::Rows => 1,
            }
        );
        let query = kind.query(&world, &sensor);
        assert_eq!((query.entities.len(), query.inspected), (33, 33));
        assert_eq!(kind.retained_rows(&world), 34);

        for &entity in &members {
            assert!(world.despawn(entity));
        }
        kind.retire(&mut world, &members);
        assert_eq!(kind.retained_rows(&world), 0);
        assert_eq!(world.query::<&NativeMarker>().iter(&world).count(), 256);
    }
}

#[test]
fn direct_despawn_filters_orphan_rows_until_exact_member_cleanup() {
    let mover = ComponentTypeId::new("trial.behavior.Mover");
    let sensor = ComponentTypeId::new("trial.behavior.Sensor");
    for kind in KINDS {
        let mut world = World::new();
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();
        for entity in [first, second] {
            kind.install(
                &mut world,
                entity,
                records(&[mover.clone(), sensor.clone()]),
            );
        }
        assert!(world.despawn(first));

        let query = kind.query(&world, &mover);
        assert_eq!(query.entities, vec![second]);
        assert_eq!(
            (query.inspected, kind.retained_rows(&world)),
            match kind {
                StorageKind::Envelope => (1, 2),
                StorageKind::Rows => (2, 4),
            }
        );

        kind.retire(&mut world, &[first]);
        assert_eq!(kind.retained_rows(&world), 2);
        for component in [&mover, &sensor] {
            let query = kind.query(&world, component);
            assert_eq!(query.entities, vec![second]);
            assert_eq!(query.inspected, 1);
        }
    }
}
