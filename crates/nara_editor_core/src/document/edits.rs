use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_core::{AssetId, EntityUuid};
use nara_reflect::validate_component_value;
use nara_scene::authoring::{SceneDocument, SceneEntity, prefab::ExpandedScene};

use super::{EditorCommand, EditorError, SceneProjection, prefabs};
use crate::EditorRegistry;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RowSlot {
    pub(super) authored: usize,
    pub(super) projected: Option<usize>,
}

pub(super) fn local_slots(
    scene: &SceneDocument,
    projection: &SceneProjection,
    command: &EditorCommand,
) -> Option<Vec<RowSlot>> {
    if matches!(projection, SceneProjection::AwaitingPrefabs) {
        return None;
    }
    let mut entities = BTreeSet::new();
    if !collect_row_targets(command, &mut entities) {
        return None;
    }
    let expanded = projection.expanded();
    entities
        .into_iter()
        .map(|entity| {
            let authored = row_index(scene, entity)?;
            if scene.entities[authored].prefab.is_some() {
                return None;
            }
            let projected = match expanded {
                Some(expanded) => Some(row_index(&expanded.document, entity)?),
                None => None,
            };
            Some(RowSlot {
                authored,
                projected,
            })
        })
        .collect()
}

fn collect_row_targets(command: &EditorCommand, targets: &mut BTreeSet<EntityUuid>) -> bool {
    match command {
        EditorCommand::Rename { entity, .. } => {
            targets.insert(*entity);
            true
        }
        EditorCommand::SetField { entities, .. }
        | EditorCommand::SetComponent { entities, .. }
        | EditorCommand::RemoveComponent { entities, .. } => {
            targets.extend(entities.iter().copied());
            true
        }
        EditorCommand::Batch(commands) => commands
            .iter()
            .all(|command| collect_row_targets(command, targets)),
        EditorCommand::DeclareAsset(_)
        | EditorCommand::Create(_)
        | EditorCommand::Delete(_)
        | EditorCommand::Reparent { .. }
        | EditorCommand::RevertPrefabOverride(_) => false,
    }
}

/// These commands preserve IDs, parents and aliases. Component writes admit payloads through
/// the registry's validated keys and versions, and removals only drop previously admitted data.
pub(super) fn prepare_rows(
    scene: &SceneDocument,
    registry: &EditorRegistry,
    slots: Vec<RowSlot>,
    command: EditorCommand,
) -> Result<Vec<(RowSlot, SceneEntity)>, EditorError> {
    let mut affected = SceneDocument::new(scene.id);
    affected.entities = slots
        .iter()
        .map(|slot| scene.entities[slot.authored].clone())
        .collect();
    apply_command(
        &mut affected,
        registry,
        &BTreeMap::new(),
        &mut EditProjection::default(),
        command,
    )?;
    Ok(slots
        .into_iter()
        .zip(affected.entities)
        .filter(|(slot, row)| scene.entities[slot.authored] != *row)
        .collect())
}

pub(super) fn prepare_document(
    scene: &SceneDocument,
    registry: &EditorRegistry,
    sources: &BTreeMap<AssetId, SceneDocument>,
    expanded: Option<&Arc<ExpandedScene>>,
    command: EditorCommand,
) -> Result<Option<(SceneDocument, SceneProjection)>, EditorError> {
    let mut after = scene.clone();
    let mut projection = EditProjection {
        initial: expanded,
        ..Default::default()
    };
    apply_command(&mut after, registry, sources, &mut projection, command)?;
    after.entities.sort_unstable_by_key(|row| row.id);
    if *scene == after {
        return Ok(None);
    }
    after.validate()?;
    let projection = SceneProjection::admitted(prefabs::expand(&after, sources, registry)?);
    Ok(Some((after, projection)))
}

#[derive(Default)]
struct EditProjection<'a> {
    initial: Option<&'a Arc<ExpandedScene>>,
    changed: BTreeSet<EntityUuid>,
}

impl<'a> EditProjection<'a> {
    fn before_edit(&mut self, entity: EntityUuid) -> Option<&'a Arc<ExpandedScene>> {
        self.initial.filter(|_| self.changed.insert(entity))
    }
}

fn apply_command(
    scene: &mut SceneDocument,
    registry: &EditorRegistry,
    prefabs: &BTreeMap<AssetId, SceneDocument>,
    projection: &mut EditProjection<'_>,
    command: EditorCommand,
) -> Result<(), EditorError> {
    match command {
        EditorCommand::DeclareAsset(asset) => prefabs::declare_asset(&mut scene.assets, asset),
        EditorCommand::Create(entity) => {
            projection.initial = None;
            if scene.entities.iter().any(|row| row.id == entity.id) {
                return Err(EditorError::DuplicateEntity(entity.id));
            }
            check_name(&entity.name)?;
            scene.entities.push(entity);
        }
        EditorCommand::Delete(entities) => {
            projection.initial = None;
            delete_subtrees(scene, entities)?;
        }
        EditorCommand::Rename { entity, name } => {
            check_name(&name)?;
            row_mut(scene, entity)?.name = name;
        }
        EditorCommand::Reparent { entities, parent } => {
            projection.initial = None;
            for entity in entities {
                row_mut(scene, entity)?.parent = parent;
            }
        }
        EditorCommand::SetField {
            entities,
            component,
            path,
            value,
        } => {
            let schema = registry.editable(&component)?;
            prefabs::check_editable_field(schema, &path)?;
            for entity in entities {
                let view = projection.before_edit(entity);
                prefabs::set_field(
                    scene, prefabs, registry, view, entity, &component, &path, &value,
                )?;
            }
        }
        EditorCommand::SetComponent {
            entities,
            component,
            value,
        } => {
            let schema = registry.editable(&component)?;
            let payload = prefabs::known(&component, &value, schema)?;
            validate_component_value(schema, payload)?;
            for entity in entities {
                let view = projection.before_edit(entity);
                prefabs::set_component(scene, prefabs, registry, view, entity, &component, &value)?;
            }
        }
        EditorCommand::RemoveComponent {
            entities,
            component,
        } => {
            projection.initial = None;
            for entity in entities {
                prefabs::remove_component(scene, prefabs, registry, entity, &component)?;
            }
        }
        EditorCommand::RevertPrefabOverride(edit) => {
            projection.initial = None;
            prefabs::revert(scene, &edit)?;
        }
        EditorCommand::Batch(commands) => {
            for command in commands {
                apply_command(scene, registry, prefabs, projection, command)?;
            }
        }
    }
    Ok(())
}

fn check_name(name: &str) -> Result<(), EditorError> {
    if name.trim().is_empty() {
        Err(EditorError::EmptyName)
    } else {
        Ok(())
    }
}

pub(super) fn row_index(scene: &SceneDocument, entity: EntityUuid) -> Option<usize> {
    let index = scene
        .entities
        .binary_search_by_key(&entity, |row| row.id)
        .ok()?;
    scene
        .entities
        .get(index)
        .is_some_and(|row| row.id == entity)
        .then_some(index)
}

pub(super) fn row_mut(
    scene: &mut SceneDocument,
    entity: EntityUuid,
) -> Result<&mut SceneEntity, EditorError> {
    let index = row_index(scene, entity)
        .or_else(|| scene.entities.iter().position(|row| row.id == entity))
        .ok_or(EditorError::MissingEntity(entity))?;
    Ok(&mut scene.entities[index])
}

fn delete_subtrees(
    scene: &mut SceneDocument,
    entities: Vec<EntityUuid>,
) -> Result<(), EditorError> {
    let mut children = BTreeMap::<_, Vec<_>>::new();
    let mut present = BTreeSet::new();
    for row in &scene.entities {
        present.insert(row.id);
        if let Some(parent) = row.parent {
            children.entry(parent).or_default().push(row.id);
        }
    }
    let mut removed = BTreeSet::new();
    let mut pending = entities;
    while let Some(entity) = pending.pop() {
        if !present.contains(&entity) {
            return Err(EditorError::MissingEntity(entity));
        }
        if removed.insert(entity) {
            pending.extend(children.remove(&entity).unwrap_or_default());
        }
    }
    scene.entities.retain(|row| !removed.contains(&row.id));
    Ok(())
}
