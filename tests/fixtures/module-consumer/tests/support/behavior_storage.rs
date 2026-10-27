//! Private storage algorithms for logical inspection and orphan-retention counterexamples.

use std::collections::BTreeMap;

use nara_ecs::{Component, Entity, Resource, World};
use nara_reflect::ComponentTypeId;
use nara_scene::SceneComponentRecord;

#[derive(Component)]
struct Attachments(BTreeMap<ComponentTypeId, SceneComponentRecord>);

#[derive(Default, Resource)]
struct SchemaRows(BTreeMap<ComponentTypeId, BTreeMap<Entity, SceneComponentRecord>>);

#[derive(Clone, Copy, Debug)]
pub(super) enum StorageKind {
    Envelope,
    Rows,
}

#[derive(Debug, Default)]
pub(super) struct QueryResult {
    pub entities: Vec<Entity>,
    /// Carriers checked for Envelope; target-schema rows checked for Rows, including orphans.
    /// Archetype metadata checks and map lookup costs are excluded.
    pub inspected: usize,
}

impl StorageKind {
    /// Installs one complete record group per live entity for the private workload.
    pub(super) fn install(
        self,
        world: &mut World,
        entity: Entity,
        records: BTreeMap<ComponentTypeId, SceneComponentRecord>,
    ) {
        assert!(world.get_entity(entity).is_ok());
        match self {
            Self::Envelope => {
                world.entity_mut(entity).insert(Attachments(records));
            }
            Self::Rows => {
                let mut rows = world.get_resource_or_init::<SchemaRows>();
                for (component, record) in records {
                    rows.0.entry(component).or_default().insert(entity, record);
                }
            }
        }
    }

    /// Returns live matches in Entity order without collecting or deleting orphan rows.
    pub(super) fn query(self, world: &World, component: &ComponentTypeId) -> QueryResult {
        let mut result = QueryResult::default();
        match self {
            Self::Envelope => {
                let Some(attachments_id) = world.components().component_id::<Attachments>() else {
                    return result;
                };
                for archetype in world
                    .archetypes()
                    .iter()
                    .filter(|archetype| archetype.contains(attachments_id))
                {
                    for entity in archetype.entities() {
                        result.inspected += 1;
                        if world
                            .get::<Attachments>(entity.id())
                            .is_some_and(|attachments| attachments.0.contains_key(component))
                        {
                            result.entities.push(entity.id());
                        }
                    }
                }
                result.entities.sort_unstable();
            }
            Self::Rows => {
                let Some(row) = world
                    .get_resource::<SchemaRows>()
                    .and_then(|rows| rows.0.get(component))
                else {
                    return result;
                };
                for &entity in row.keys() {
                    result.inspected += 1;
                    if world.get_entity(entity).is_ok() {
                        result.entities.push(entity);
                    }
                }
            }
        }
        result
    }

    /// Removes only the supplied entities' memberships; entity despawning belongs to the caller.
    pub(super) fn retire(self, world: &mut World, entities: &[Entity]) {
        match self {
            Self::Envelope => {
                for &entity in entities {
                    if let Ok(mut entity) = world.get_entity_mut(entity) {
                        entity.remove::<Attachments>();
                    }
                }
            }
            Self::Rows => {
                let empty = {
                    let Some(mut rows) = world.get_resource_mut::<SchemaRows>() else {
                        return;
                    };
                    rows.0.retain(|_, row| {
                        for entity in entities {
                            row.remove(entity);
                        }
                        !row.is_empty()
                    });
                    rows.0.is_empty()
                };
                if empty {
                    world.remove_resource::<SchemaRows>();
                }
            }
        }
    }

    /// Counts physically retained component records, including orphan rows in Rows.
    pub(super) fn retained_rows(self, world: &World) -> usize {
        match self {
            Self::Envelope => world
                .iter_entities()
                .filter_map(|entity| entity.get::<Attachments>())
                .map(|attachments| attachments.0.len())
                .sum(),
            Self::Rows => world
                .get_resource::<SchemaRows>()
                .map(|rows| rows.0.values().map(BTreeMap::len).sum())
                .unwrap_or(0),
        }
    }
}
