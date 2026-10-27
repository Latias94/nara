//! Private storage alternatives for records already prepared by the behavior trial.

use std::collections::BTreeMap;

use nara_ecs::{Component, Entity, Resource, World};
use nara_reflect::ComponentTypeId;
use nara_scene::SceneComponentRecord;

#[derive(Component)]
struct Attachments(BTreeMap<ComponentTypeId, SceneComponentRecord>);

#[derive(Default, Resource)]
struct SchemaRows(BTreeMap<ComponentTypeId, BTreeMap<Entity, SceneComponentRecord>>);

#[derive(Component)]
#[component(on_remove = remove_membership)]
struct Membership;

fn remove_membership(
    mut world: nara_ecs::bevy_ecs::world::DeferredWorld<'_>,
    context: nara_ecs::bevy_ecs::lifecycle::HookContext,
) {
    if let Some(mut rows) = world.get_resource_mut::<SchemaRows>() {
        rows.0.retain(|_, members| {
            members.remove(&context.entity);
            !members.is_empty()
        });
    }
}

pub(super) fn install_membership_hook(world: &mut World, entity: Entity) {
    world.entity_mut(entity).insert(Membership);
}

pub(super) fn remove_membership_hook(world: &mut World, entity: Entity) {
    world.entity_mut(entity).remove::<Membership>();
}

#[derive(Clone, Copy, Debug)]
pub enum StorageKind {
    Envelope,
    Rows,
}

#[derive(Debug, Default)]
pub(super) struct QueryResult {
    pub entities: Vec<Entity>,
    /// Attachment carriers checked for Envelope; target-schema rows checked for Rows, including orphans.
    /// Archetype metadata checks are excluded.
    pub inspected: usize,
}

impl StorageKind {
    /// The caller installs the complete prepared record group once per live entity.
    pub(super) fn install(
        self,
        world: &mut World,
        entity: Entity,
        records: BTreeMap<ComponentTypeId, SceneComponentRecord>,
    ) {
        assert!(
            world.get_entity(entity).is_ok(),
            "behavior records require a live entity"
        );
        if records.is_empty() {
            return;
        }

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

    pub(super) fn read<'a>(
        self,
        world: &'a World,
        entity: Entity,
        component: &ComponentTypeId,
    ) -> Option<&'a SceneComponentRecord> {
        match self {
            Self::Envelope => world.get::<Attachments>(entity)?.0.get(component),
            Self::Rows => {
                world.get_entity(entity).ok()?;
                world
                    .get_resource::<SchemaRows>()?
                    .0
                    .get(component)?
                    .get(&entity)
            }
        }
    }

    pub(super) fn components(self, world: &World, entity: Entity) -> Vec<ComponentTypeId> {
        if world.get_entity(entity).is_err() {
            return Vec::new();
        }

        match self {
            Self::Envelope => world
                .get::<Attachments>(entity)
                .map(|attachments| attachments.0.keys().cloned().collect())
                .unwrap_or_default(),
            Self::Rows => world
                .get_resource::<SchemaRows>()
                .map(|rows| {
                    rows.0
                        .iter()
                        .filter(|(_, row)| row.contains_key(&entity))
                        .map(|(component, _)| component.clone())
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    pub(super) fn replace(
        self,
        world: &mut World,
        entity: Entity,
        component: &ComponentTypeId,
        value: SceneComponentRecord,
    ) -> bool {
        if world.get_entity(entity).is_err() {
            return false;
        }

        match self {
            Self::Envelope => {
                let Some(mut attachments) = world.get_mut::<Attachments>(entity) else {
                    return false;
                };
                let Some(record) = attachments.0.get_mut(component) else {
                    return false;
                };
                *record = value;
            }
            Self::Rows => {
                let Some(mut rows) = world.get_resource_mut::<SchemaRows>() else {
                    return false;
                };
                let Some(record) = rows
                    .0
                    .get_mut(component)
                    .and_then(|row| row.get_mut(&entity))
                else {
                    return false;
                };
                *record = value;
            }
        }
        true
    }

    pub(super) fn remove(
        self,
        world: &mut World,
        entity: Entity,
        component: &ComponentTypeId,
    ) -> Option<SceneComponentRecord> {
        world.get_entity(entity).ok()?;

        match self {
            Self::Envelope => {
                let (removed, empty) = {
                    let mut attachments = world.get_mut::<Attachments>(entity)?;
                    let removed = attachments.0.remove(component);
                    (removed, attachments.0.is_empty())
                };
                if empty {
                    world.entity_mut(entity).remove::<Attachments>();
                }
                removed
            }
            Self::Rows => {
                let (removed, empty) = {
                    let mut rows = world.get_resource_mut::<SchemaRows>()?;
                    let row = rows.0.get_mut(component)?;
                    let removed = row.remove(&entity);
                    if row.is_empty() {
                        rows.0.remove(component);
                    }
                    (removed, rows.0.is_empty())
                };
                if empty {
                    world.remove_resource::<SchemaRows>();
                }
                removed
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
