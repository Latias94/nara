use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_core::{AssetId, DocumentId, EntityUuid};
use nara_reflect::{
    ComponentCapability, ComponentFieldPath, ComponentSchema, ComponentTypeId, ComponentValue,
    validate_component_value,
};
use nara_scene::authoring::{
    PrefabInstance, RawRon, SceneAsset, SceneComponent, SceneDocument, SceneEntity,
    prefab::{
        ExpandedScene, PrefabOverridePath, decode_override_value, encode_override_value,
        expand_prefabs_for_editor,
    },
};

use super::{EditorError, EditorSession, ScenePatch, SceneProjection, row_index, row_mut};
use crate::EditorRegistry;

type Library = BTreeMap<AssetId, SceneDocument>;

/// An Inspector action bound to the exact authored override it displayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabFieldOverride {
    document: DocumentId,
    asset: AssetId,
    pub instance: EntityUuid,
    pub address: String,
    pub raw: RawRon,
}

impl EditorSession {
    pub fn prefab_sources(&self) -> &BTreeMap<AssetId, SceneDocument> {
        &self.prefabs
    }

    pub fn is_inherited_entity(&self, entity: EntityUuid) -> bool {
        self.projection
            .expanded()
            .and_then(|scene| scene.provenance.get(&entity))
            .is_some_and(|origin| !origin.child_path.is_empty())
    }

    pub fn prefab_overrides(&self, entity: EntityUuid) -> Vec<PrefabFieldOverride> {
        let Some(expanded) = self.projection.expanded() else {
            return Vec::new();
        };
        let Some(origin) = expanded.provenance.get(&entity) else {
            return Vec::new();
        };
        let Some(instance) =
            row(&self.document.scene, origin.instance).and_then(|row| row.prefab.as_ref())
        else {
            return Vec::new();
        };
        instance
            .overrides
            .iter()
            .filter(|(address, _)| {
                expanded
                    .parse_override(origin.instance, address, self.registry.catalog())
                    .is_ok_and(|path| path.child_path() == origin.child_path)
            })
            .map(|(address, raw)| PrefabFieldOverride {
                document: self.document.scene.id,
                asset: instance.asset,
                instance: origin.instance,
                address: address.clone(),
                raw: raw.clone(),
            })
            .collect()
    }

    pub fn can_apply_to_prefab(&self, edit: &PrefabFieldOverride) -> bool {
        !self.gesture_active() && self.editable_override(edit).is_ok()
    }

    fn editable_override(
        &self,
        edit: &PrefabFieldOverride,
    ) -> Result<PrefabOverridePath, EditorError> {
        matching_override(&self.document.scene, edit)?;
        let expanded = self
            .projection
            .expanded()
            .ok_or(EditorError::StalePrefabOverride)?;
        let path =
            expanded.parse_override(edit.instance, &edit.address, self.registry.catalog())?;
        check_editable_field(self.registry.editable(path.component())?, path.field())?;
        expanded.resolve_override(edit.instance, &path)?;
        Ok(path)
    }

    /// Applies an override to the immediate upstream prefab and clears it from this scene.
    ///
    /// Call after explicit user confirmation. The publisher runs only after both edited
    /// documents and the resulting scene expansion have been validated. Local Undo restores
    /// the override in this scene. The upstream file is a separate saved edit.
    pub fn apply_to_prefab<E: std::fmt::Display>(
        &mut self,
        edit: &PrefabFieldOverride,
        publish: impl FnOnce(AssetId, &SceneDocument, &SceneDocument) -> Result<(), E>,
    ) -> Result<bool, EditorError> {
        self.require_idle()?;
        let path = self.editable_override(edit)?;
        let schema = self.registry.editable(path.component())?;
        let instance = matching_override(&self.document.scene, edit)?;
        let id = instance.asset;
        let original = self
            .prefabs
            .get(&id)
            .ok_or(EditorError::MissingPrefab(id))?;
        let expanded = self
            .projection
            .expanded()
            .ok_or(EditorError::StalePrefabOverride)?;
        let effective = expanded.resolve_override(edit.instance, &path)?;
        let mut value = effective.clone();

        let mut upstream = original.clone();
        let root = upstream
            .entities
            .iter()
            .find(|row| row.parent.is_none())
            .map(|row| row.id)
            .ok_or(EditorError::MissingPrefab(id))?;
        let children = path.child_path();
        let (upstream_entity, nested, nested_path) = if let Some(first) = children.first()
            && row(&upstream, *first).is_some()
        {
            (
                *first,
                expanded.resolve_target(edit.instance, &children[..1]),
                &children[1..],
            )
        } else {
            (root, expanded.resolve_target(edit.instance, &[]), children)
        };
        let target =
            row(&upstream, upstream_entity).ok_or(EditorError::MissingEntity(upstream_entity))?;
        let nested_instance = target
            .prefab
            .as_ref()
            .zip(nested)
            .map(|(instance, row)| (instance.asset, row.id));
        let use_nested_override = target.prefab.as_ref().is_some_and(|instance| {
            !nested_path.is_empty()
                || (!target.components.contains_key(path.component())
                    && !instance.added_components.contains_key(path.component()))
        });

        // Known references can be re-authored in their original scope. An unrepresentable
        // generated descendant must not turn into a reference to one specific scene occurrence.
        remap_references(&mut value, &mut |reference| {
            if let Some(local) = expanded.local_entity_reference_in(edit.instance, id, reference) {
                return Ok(local);
            }
            if let Some(local) = nested_instance.and_then(|(asset, instance)| {
                expanded.local_entity_reference_in(instance, asset, reference)
            }) {
                return Ok(local);
            }
            if expanded
                .provenance
                .get(&reference)
                .is_some_and(|origin| origin.instance == edit.instance)
                || row(&upstream, reference).is_some()
            {
                return Err(EditorError::PrefabReferenceScope(reference));
            }
            Ok(reference)
        })?;
        copy_asset_aliases(&mut upstream, &self.document.scene, &value);
        if use_nested_override {
            let projected = expand(&upstream, &self.prefabs, &self.registry)?
                .ok_or(EditorError::NotPrefabInstance(upstream_entity))?;
            let nested = PrefabOverridePath::new(
                nested_path.iter().copied(),
                path.component().clone(),
                path.field().clone(),
            )?;
            write_override(
                &mut upstream,
                upstream_entity,
                &nested,
                &projected,
                &self.registry,
                &value,
            )?;
        } else {
            if !nested_path.is_empty() {
                return Err(EditorError::InheritedPrefabChild);
            }
            let target = row_mut(&mut upstream, upstream_entity)?;
            let records = match &mut target.prefab {
                Some(instance) if instance.added_components.contains_key(path.component()) => {
                    &mut instance.added_components
                }
                _ => &mut target.components,
            };
            set_record_field(records, schema, upstream_entity, path.field(), &value)?;
        }

        let mut after = self.document.scene.clone();
        instance_mut(&mut after, edit.instance)?
            .overrides
            .remove(&edit.address);
        let mut library = self.prefabs.clone();
        library.insert(id, upstream.clone());
        upstream.validate()?;
        let preview = expand(&after, &library, &self.registry)?;
        if preview
            .as_ref()
            .ok_or(EditorError::StalePrefabOverride)?
            .resolve_override(edit.instance, &path)?
            != effective
        {
            return Err(EditorError::PrefabApplyChangedValue);
        }
        let mut previous = self.document.scene.clone();
        for entry in self.history.entries[..self.history.cursor].iter().rev() {
            entry.patch.apply(&mut previous, false);
            expand(&previous, &library, &self.registry)
                .map_err(|error| EditorError::PrefabHistoryWouldBreak(Box::new(error)))?;
        }
        publish(id, original, &upstream).map_err(|error| EditorError::PublishPrefab {
            asset: id,
            message: error.to_string(),
        })?;
        self.history
            .push(ScenePatch::between(&self.document.scene, &after));
        self.document.scene = after;
        self.prefabs = library;
        self.projection = SceneProjection::admitted(preview);
        self.prune_selection();
        Ok(true)
    }
}

pub(super) fn expand(
    scene: &SceneDocument,
    prefabs: &Library,
    registry: &EditorRegistry,
) -> Result<Option<Arc<ExpandedScene>>, EditorError> {
    if !scene.entities.iter().any(|row| row.prefab.is_some()) {
        return Ok(None);
    }
    let scene = expand_prefabs_for_editor(scene, registry.catalog(), |asset| {
        prefabs
            .get(&asset)
            .cloned()
            .ok_or(EditorError::MissingPrefab(asset))
    })?;
    Ok(Some(Arc::new(scene)))
}

pub(super) fn declare_asset(assets: &mut BTreeMap<String, SceneAsset>, asset: SceneAsset) {
    if assets.values().any(|entry| entry.id == asset.id) {
        return;
    }
    let filename = asset.hint.as_str().rsplit('/').next().unwrap_or("asset");
    let base = filename
        .split_once('.')
        .map_or(filename, |(stem, _)| stem)
        .replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_");
    let base = if base.is_empty() { "asset" } else { &base };
    let mut key = base.to_owned();
    let mut suffix = 2;
    while assets.contains_key(&key) {
        key = format!("{base}_{suffix}");
        suffix += 1;
    }
    assets.insert(key, asset);
}

pub(super) fn known<'a>(
    component: &ComponentTypeId,
    record: &'a SceneComponent,
    schema: &ComponentSchema,
) -> Result<&'a ComponentValue, EditorError> {
    record
        .value()
        .filter(|_| record.version() == schema.version())
        .ok_or_else(|| EditorError::ReadOnlyComponent(component.clone()))
}

pub(super) fn check_editable_field(
    schema: &ComponentSchema,
    path: &ComponentFieldPath,
) -> Result<(), EditorError> {
    let permitted = schema
        .fields()
        .iter()
        .filter(|field| path.segments().starts_with(field.path().segments()))
        .max_by_key(|field| field.path().segments().len())
        .is_some_and(|field| field.has_capability(ComponentCapability::Edit));
    if !permitted {
        return Err(EditorError::ReadOnlyField(path.clone()));
    }
    Ok(())
}

pub(super) fn set_field(
    scene: &mut SceneDocument,
    prefabs: &Library,
    registry: &EditorRegistry,
    previous: Option<&Arc<ExpandedScene>>,
    entity: EntityUuid,
    component: &ComponentTypeId,
    path: &ComponentFieldPath,
    value: &ComponentValue,
) -> Result<(), EditorError> {
    let schema = registry.editable(component)?;
    let inherited = projection(scene, prefabs, registry, previous, entity)?;
    match inherited {
        None => {
            let target = row_mut(scene, entity)?;
            set_record_field(&mut target.components, schema, entity, path, value)
        }
        Some(expanded) => {
            let origin = &expanded.provenance[&entity];
            let target = expanded
                .resolve_target(origin.instance, &origin.child_path)
                .ok_or(EditorError::MissingEntity(entity))?;
            let current =
                target
                    .components
                    .get(component)
                    .ok_or_else(|| EditorError::MissingComponent {
                        entity,
                        component: component.clone(),
                    })?;
            let current = known(component, current, schema)?;
            if current
                .read_path(path)
                .is_ok_and(|current| current == *value)
            {
                return Ok(());
            }
            let mut changed = current.clone();
            changed.set_path(path, value.clone())?;
            check_read_only_fields(schema, current, &changed)?;
            validate_component_value(schema, &changed)?;
            store_inherited_field(scene, registry, &expanded, entity, schema, path, value)
        }
    }
}

pub(super) fn set_component(
    scene: &mut SceneDocument,
    prefabs: &Library,
    registry: &EditorRegistry,
    previous: Option<&Arc<ExpandedScene>>,
    entity: EntityUuid,
    component: &ComponentTypeId,
    record: &SceneComponent,
) -> Result<(), EditorError> {
    let schema = registry.editable(component)?;
    let replacement = known(component, record, schema)?;
    let expanded = projection(scene, prefabs, registry, previous, entity)?;
    if let Some(expanded) = expanded {
        let origin = &expanded.provenance[&entity];
        let target = expanded
            .resolve_target(origin.instance, &origin.child_path)
            .ok_or(EditorError::MissingEntity(entity))?;
        if let Some(current) = target.components.get(component) {
            let from = known(component, current, schema)?;
            check_read_only_fields(schema, from, replacement)?;
            let mut changed = Vec::new();
            for field in schema.fields() {
                if let Ok(value) = replacement.get_path(field.path())
                    && from.get_path(field.path()).ok() != Some(value)
                {
                    changed.push((field.path(), value));
                }
            }
            for (path, value) in changed {
                store_inherited_field(scene, registry, &expanded, entity, schema, path, value)?;
            }
        } else {
            if !origin.child_path.is_empty() {
                return Err(EditorError::InheritedPrefabChild);
            }
            let mut value = replacement.clone();
            remap_references(&mut value, &mut |reference| {
                expanded
                    .override_entity_reference(origin.instance, reference)
                    .ok_or(EditorError::PrefabReferenceScope(reference))
            })?;
            let instance = instance_mut(scene, origin.instance)?;
            if instance.removed_components.remove(component) {
                // Restore the inherited component, then apply any values the edit changed.
                return set_component(scene, prefabs, registry, None, entity, component, record);
            }
            instance.added_components.insert(
                component.clone(),
                SceneComponent::known(record.version(), value),
            );
        }
    } else {
        let records = &mut row_mut(scene, entity)?.components;
        if let Some(current) = records.get(component) {
            check_read_only_fields(schema, known(component, current, schema)?, replacement)?;
        }
        records.insert(component.clone(), record.clone());
    }
    Ok(())
}

fn store_inherited_field(
    scene: &mut SceneDocument,
    registry: &EditorRegistry,
    expanded: &ExpandedScene,
    entity: EntityUuid,
    schema: &ComponentSchema,
    path: &ComponentFieldPath,
    value: &ComponentValue,
) -> Result<(), EditorError> {
    let origin = &expanded.provenance[&entity];
    let component = schema.id();
    let mut value = value.clone();
    remap_references(&mut value, &mut |reference| {
        expanded
            .override_entity_reference(origin.instance, reference)
            .ok_or(EditorError::PrefabReferenceScope(reference))
    })?;
    if origin.child_path.is_empty() {
        let target = row_mut(scene, origin.instance)?;
        if target.components.contains_key(component) {
            return set_record_field(&mut target.components, schema, entity, path, &value);
        }
        let instance = target
            .prefab
            .as_mut()
            .ok_or(EditorError::NotPrefabInstance(entity))?;
        if instance.added_components.contains_key(component) {
            return set_record_field(&mut instance.added_components, schema, entity, path, &value);
        }
    }
    let path = PrefabOverridePath::new(origin.child_path.clone(), component.clone(), path.clone())?;
    write_override(scene, origin.instance, &path, expanded, registry, &value)
}

pub(super) fn remove_component(
    scene: &mut SceneDocument,
    prefabs: &Library,
    registry: &EditorRegistry,
    entity: EntityUuid,
    component: &ComponentTypeId,
) -> Result<(), EditorError> {
    if let Some(expanded) = projection(scene, prefabs, registry, None, entity)? {
        let origin = &expanded.provenance[&entity];
        if !origin.child_path.is_empty() {
            return Err(EditorError::InheritedPrefabChild);
        }
        let target = row_mut(scene, origin.instance)?;
        let instance = target
            .prefab
            .as_mut()
            .ok_or(EditorError::NotPrefabInstance(entity))?;
        let present = expanded
            .resolve_target(origin.instance, &[])
            .is_some_and(|row| row.components.contains_key(component));
        if !present {
            return Ok(());
        }
        if target.components.remove(component).is_none()
            && instance.added_components.remove(component).is_none()
        {
            instance.removed_components.insert(component.clone());
        }
        instance.overrides.retain(|address, _| {
            expanded
                .parse_override(origin.instance, address, registry.catalog())
                .map_or(true, |path| {
                    !path.child_path().is_empty() || path.component() != component
                })
        });
    } else {
        row_mut(scene, entity)?.components.remove(component);
    }
    Ok(())
}

pub(super) fn revert(
    scene: &mut SceneDocument,
    edit: &PrefabFieldOverride,
) -> Result<(), EditorError> {
    matching_override(scene, edit)?;
    instance_mut(scene, edit.instance)?
        .overrides
        .remove(&edit.address);
    Ok(())
}

fn matching_override<'a>(
    scene: &'a SceneDocument,
    edit: &PrefabFieldOverride,
) -> Result<&'a PrefabInstance, EditorError> {
    if scene.id != edit.document {
        return Err(EditorError::StalePrefabOverride);
    }
    row(scene, edit.instance)
        .and_then(|row| row.prefab.as_ref())
        .filter(|instance| {
            instance.asset == edit.asset && instance.overrides.get(&edit.address) == Some(&edit.raw)
        })
        .ok_or(EditorError::StalePrefabOverride)
}

fn projection(
    scene: &SceneDocument,
    prefabs: &Library,
    registry: &EditorRegistry,
    previous: Option<&Arc<ExpandedScene>>,
    entity: EntityUuid,
) -> Result<Option<Arc<ExpandedScene>>, EditorError> {
    if row(scene, entity).is_some_and(|row| row.prefab.is_none()) {
        return Ok(None);
    }
    let expanded = if let Some(previous) = previous {
        Some(Arc::clone(previous))
    } else {
        expand(scene, prefabs, registry)?
    };
    expanded
        .filter(|expanded| expanded.provenance.contains_key(&entity))
        .map(Some)
        .ok_or(EditorError::MissingEntity(entity))
}

fn row(scene: &SceneDocument, id: EntityUuid) -> Option<&SceneEntity> {
    row_index(scene, id)
        .map(|index| &scene.entities[index])
        .or_else(|| scene.entities.iter().find(|row| row.id == id))
}

fn instance_mut(
    scene: &mut SceneDocument,
    id: EntityUuid,
) -> Result<&mut PrefabInstance, EditorError> {
    row_mut(scene, id)?
        .prefab
        .as_mut()
        .ok_or(EditorError::NotPrefabInstance(id))
}

fn set_record_field(
    records: &mut BTreeMap<ComponentTypeId, SceneComponent>,
    schema: &ComponentSchema,
    entity: EntityUuid,
    path: &ComponentFieldPath,
    value: &ComponentValue,
) -> Result<(), EditorError> {
    let component = schema.id();
    let record = records
        .get_mut(component)
        .ok_or_else(|| EditorError::MissingComponent {
            entity,
            component: component.clone(),
        })?;
    let current = known(component, record, schema)?;
    let mut replacement = current.clone();
    replacement.set_path(path, value.clone())?;
    check_read_only_fields(schema, current, &replacement)?;
    validate_component_value(schema, &replacement)?;
    *record = SceneComponent::known(record.version(), replacement);
    Ok(())
}

fn check_read_only_fields(
    schema: &ComponentSchema,
    original: &ComponentValue,
    next: &ComponentValue,
) -> Result<(), EditorError> {
    for field in schema.fields() {
        if !field.has_capability(ComponentCapability::Edit)
            && original.get_path(field.path()).ok() != next.get_path(field.path()).ok()
        {
            return Err(EditorError::ReadOnlyField(field.path().clone()));
        }
    }
    Ok(())
}

fn write_override(
    scene: &mut SceneDocument,
    instance: EntityUuid,
    path: &PrefabOverridePath,
    expanded: &ExpandedScene,
    registry: &EditorRegistry,
    value: &ComponentValue,
) -> Result<(), EditorError> {
    let catalog = registry.catalog();
    let existing = row(scene, instance)
        .and_then(|row| row.prefab.as_ref())
        .ok_or(EditorError::NotPrefabInstance(instance))?;
    // If a parent field already has an override, update the nested value within that
    // override so a change to one channel cannot discard other explicitly set channels.
    for (address, raw) in &existing.overrides {
        let Ok(parent) = expanded.parse_override(instance, address, catalog) else {
            continue;
        };
        if parent.child_path() == path.child_path()
            && parent.component() == path.component()
            && let Some(rest) = path
                .field()
                .segments()
                .strip_prefix(parent.field().segments())
            && !rest.is_empty()
        {
            let key = address.clone();
            let mut replacement = decode_override_value(&parent, raw, catalog, &scene.assets)?;
            replacement.set_path(
                &ComponentFieldPath::new(rest.iter().cloned()),
                value.clone(),
            )?;
            let raw = encode_override_value(&parent, &replacement, catalog, &scene.assets)?;
            instance_mut(scene, instance)?.overrides.insert(key, raw);
            return Ok(());
        }
    }
    let value = encode_override_value(path, value, catalog, &scene.assets)?;
    let overrides = &mut instance_mut(scene, instance)?.overrides;
    overrides.retain(|address, _| {
        expanded
            .parse_override(instance, address, catalog)
            .map_or(true, |existing| {
                existing.child_path() != path.child_path()
                    || existing.component() != path.component()
                    || !existing
                        .field()
                        .segments()
                        .starts_with(path.field().segments())
            })
    });
    overrides.insert(path.canonical_key(catalog), value);
    Ok(())
}

fn remap_references(
    value: &mut ComponentValue,
    map: &mut impl FnMut(EntityUuid) -> Result<EntityUuid, EditorError>,
) -> Result<(), EditorError> {
    match value {
        ComponentValue::EntityReference(reference) => *reference = map(*reference)?,
        ComponentValue::List(items) => {
            for item in items {
                remap_references(item, map)?;
            }
        }
        ComponentValue::Map(items) => {
            for item in items.values_mut() {
                remap_references(item, map)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn copy_asset_aliases(upstream: &mut SceneDocument, scene: &SceneDocument, value: &ComponentValue) {
    visit_assets(value, &mut |id| {
        if let Some(asset) = scene.assets.values().find(|asset| asset.id == id) {
            declare_asset(&mut upstream.assets, asset.clone());
        }
    });
}

pub(super) fn collect_authored_assets(
    scene: &SceneDocument,
    registry: &EditorRegistry,
    referenced: &mut BTreeSet<AssetId>,
) {
    let catalog = registry.catalog();
    let mut collect = |id| {
        referenced.insert(id);
    };
    for row in &scene.entities {
        let added = row
            .prefab
            .iter()
            .flat_map(|prefab| prefab.added_components.values());
        for record in row.components.values().chain(added) {
            if let Some(value) = record.value() {
                visit_assets(value, &mut collect);
            }
        }
        if let Some(prefab) = &row.prefab {
            collect(prefab.asset);
            for (address, raw) in &prefab.overrides {
                if let Ok(path) = PrefabOverridePath::parse(address, catalog)
                    && let Ok(value) = decode_override_value(&path, raw, catalog, &scene.assets)
                {
                    visit_assets(&value, &mut collect);
                }
            }
        }
    }
}

fn visit_assets(value: &ComponentValue, visit: &mut impl FnMut(AssetId)) {
    match value {
        ComponentValue::AssetReference(id) => visit(*id),
        ComponentValue::Map(items) => {
            for item in items.values() {
                visit_assets(item, visit);
            }
        }
        ComponentValue::List(items) => {
            for item in items {
                visit_assets(item, visit);
            }
        }
        _ => {}
    }
}
