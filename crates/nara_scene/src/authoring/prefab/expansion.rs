use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

use nara_core::{AssetId, DocumentId, EntityUuid, vfs::AssetPath};
use nara_reflect::{
    ComponentCodecError, ComponentFieldPath, ComponentFieldPathSegment, ComponentSchema,
    ComponentSchemaCatalog, ComponentShapeErrorKind, ComponentTypeId, ComponentValue,
    ComponentValueShape, validate_component_value,
};

use super::{
    super::{
        PrefabInstance, RawRon, SceneAsset, SceneComponent, SceneDocument, SceneDocumentError,
        SceneEntity,
    },
    DeferredPrefabOverride, EntityIds, ExpandedScene, InstanceEntityIds, MAX_EXPANDED_ENTITIES,
    MAX_PREFAB_DEPTH, PrefabError, PrefabOverrideError, PrefabOverridePath, PrefabProvenance,
    address::{find_schema, nested_shape, split_target},
    child_id, decode_override_value, derive_path,
};

pub(super) fn expand(
    source: &SceneDocument,
    schemas: &ComponentSchemaCatalog,
    mode: Mode,
    load: impl FnMut(AssetId) -> Result<SceneDocument, PrefabError>,
) -> Result<ExpandedScene, PrefabError> {
    validate_source(source)?;
    let mut expansion = Expansion {
        schemas,
        load,
        mode,
        sources: BTreeMap::new(),
        stack: Vec::new(),
        entities: Vec::new(),
        indices: BTreeMap::new(),
        provenance: BTreeMap::new(),
        instance_ids: InstanceEntityIds::default(),
        asset_hints: BTreeMap::new(),
        deferred_overrides: Vec::new(),
    };
    expansion.collect_hints(source);
    let outer = Scope {
        document: source.id,
        ids: source
            .entities
            .iter()
            .map(|entity| (entity.id, entity.id))
            .collect(),
        parent: None,
        assets: &source.assets,
    };
    let mut ordered: Vec<_> = source.entities.iter().collect();
    ordered.sort_unstable_by_key(|entity| entity.id);
    for entity in ordered {
        expansion.entity(entity, entity.id, entity.parent, &outer, None)?;
    }
    expansion.entities.sort_unstable_by_key(|entity| entity.id);
    Ok(ExpandedScene {
        document: SceneDocument {
            id: source.id,
            assets: source.assets.clone(),
            entities: expansion.entities,
        },
        provenance: expansion.provenance,
        asset_hints: expansion.asset_hints,
        deferred_overrides: expansion.deferred_overrides,
        instance_ids: expansion.instance_ids,
        authored_ids: outer.ids.into_keys().collect::<BTreeSet<_>>(),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Strict,
    Editor,
}

struct Scope<'a> {
    document: DocumentId,
    ids: EntityIds,
    parent: Option<&'a Scope<'a>>,
    assets: &'a BTreeMap<String, SceneAsset>,
}

impl Scope<'_> {
    fn resolve(&self, id: EntityUuid) -> Option<EntityUuid> {
        let mut scope = Some(self);
        while let Some(current) = scope {
            if let Some(final_id) = current.ids.get(&id) {
                return Some(*final_id);
            }
            scope = current.parent;
        }
        None
    }

    fn remap(&self, value: &mut ComponentValue, fallback: Option<&Scope<'_>>) {
        let mut pending = vec![value];
        while let Some(value) = pending.pop() {
            match value {
                ComponentValue::EntityReference(id) => {
                    if let Some(final_id) = self
                        .resolve(*id)
                        .or_else(|| fallback.and_then(|scope| scope.ids.get(id).copied()))
                    {
                        *id = final_id;
                    }
                }
                ComponentValue::List(items) => pending.extend(items),
                ComponentValue::Map(fields) => pending.extend(fields.values_mut()),
                _ => {}
            }
        }
    }

    fn component(&self, source: &SceneComponent, fallback: Option<&Scope<'_>>) -> SceneComponent {
        let mut component = source.clone();
        if let SceneComponent::Known { value, .. } = &mut component {
            self.remap(value, fallback);
        }
        component
    }
}

struct Mount {
    id: EntityUuid,
    parent: Option<EntityUuid>,
    provenance: PrefabProvenance,
}

struct PrefabSource {
    document: SceneDocument,
    root: EntityUuid,
}

struct Expansion<'a, F> {
    schemas: &'a ComponentSchemaCatalog,
    load: F,
    mode: Mode,
    sources: BTreeMap<AssetId, Rc<PrefabSource>>,
    stack: Vec<AssetId>,
    entities: Vec<SceneEntity>,
    indices: BTreeMap<EntityUuid, usize>,
    provenance: BTreeMap<EntityUuid, PrefabProvenance>,
    instance_ids: InstanceEntityIds,
    asset_hints: BTreeMap<AssetId, AssetPath>,
    deferred_overrides: Vec<DeferredPrefabOverride>,
}

impl<F> Expansion<'_, F>
where
    F: FnMut(AssetId) -> Result<SceneDocument, PrefabError>,
{
    fn collect_hints(&mut self, document: &SceneDocument) {
        for asset in document.assets.values() {
            self.asset_hints
                .entry(asset.id)
                .or_insert_with(|| asset.hint.clone());
        }
    }

    fn load(&mut self, asset: AssetId) -> Result<Rc<PrefabSource>, PrefabError> {
        if let Some(source) = self.sources.get(&asset) {
            return Ok(Rc::clone(source));
        }
        let mut document = (self.load)(asset)?;
        document.entities.sort_unstable_by_key(|entity| entity.id);
        validate_source(&document)?;
        let mut roots = document
            .entities
            .iter()
            .filter(|entity| entity.parent.is_none());
        let root = roots.next().map(|entity| entity.id);
        let remaining = roots.count();
        let root = root
            .filter(|_| remaining == 0)
            .ok_or(PrefabError::InvalidRootCount {
                asset,
                roots: remaining + usize::from(root.is_some()),
            })?;
        self.collect_hints(&document);
        let source = Rc::new(PrefabSource { document, root });
        self.sources.insert(asset, Rc::clone(&source));
        Ok(source)
    }

    fn entity(
        &mut self,
        entity: &SceneEntity,
        id: EntityUuid,
        parent: Option<EntityUuid>,
        scope: &Scope<'_>,
        provenance: Option<PrefabProvenance>,
    ) -> Result<(), PrefabError> {
        if let Some(instance) = &entity.prefab {
            let mount = Mount {
                id,
                parent,
                provenance: provenance.unwrap_or_else(|| PrefabProvenance {
                    instance: id,
                    child_path: Vec::new(),
                }),
            };
            if self.stack.contains(&instance.asset) {
                let mut chain = self.stack.clone();
                chain.push(instance.asset);
                return Err(PrefabError::Cycle { chain });
            }
            if self.stack.len() == MAX_PREFAB_DEPTH {
                return Err(PrefabError::DepthLimit {
                    asset: instance.asset,
                    limit: MAX_PREFAB_DEPTH,
                });
            }
            let source = self.load(instance.asset)?;
            self.stack.push(instance.asset);
            let result = self.instance(entity, instance, mount, scope, &source);
            self.stack.pop();
            return result;
        }
        if self.entities.len() == MAX_EXPANDED_ENTITIES {
            return Err(PrefabError::EntityLimit {
                limit: MAX_EXPANDED_ENTITIES,
            });
        }
        if self.indices.insert(id, self.entities.len()).is_some() {
            return Err(PrefabError::EntityCollision { entity: id });
        }
        let components = entity
            .components
            .iter()
            .map(|(component, record)| (component.clone(), scope.component(record, None)))
            .collect();
        self.entities.push(SceneEntity {
            id,
            name: entity.name.clone(),
            parent,
            components,
            prefab: None,
        });
        if let Some(origin) = provenance {
            self.provenance.insert(id, origin);
        }
        Ok(())
    }

    fn instance(
        &mut self,
        entity: &SceneEntity,
        instance: &PrefabInstance,
        mount: Mount,
        enclosing: &Scope<'_>,
        source: &PrefabSource,
    ) -> Result<(), PrefabError> {
        let mut ids = BTreeMap::new();
        for entity in &source.document.entities {
            let instance_id = if entity.id == source.root {
                mount.id
            } else {
                child_id(mount.id, entity.id).ok_or(PrefabError::InvalidDerivedId {
                    instance: mount.id,
                    child: entity.id,
                })?
            };
            ids.insert(entity.id, instance_id);
        }
        let scope = Scope {
            document: source.document.id,
            ids,
            parent: Some(enclosing),
            assets: &source.document.assets,
        };
        let start = self.entities.len();
        for entity in &source.document.entities {
            let mut provenance = mount.provenance.clone();
            if entity.id != source.root {
                provenance.child_path.push(entity.id);
            }
            let parent = entity
                .parent
                .map_or(mount.parent, |parent| scope.ids.get(&parent).copied());
            self.entity(
                entity,
                scope.ids[&entity.id],
                parent,
                &scope,
                Some(provenance),
            )?;
        }
        let root_index = self.indices[&mount.id];
        let root = &mut self.entities[root_index];
        if !entity.name.is_empty() {
            root.name.clone_from(&entity.name);
        }
        for component in &instance.removed_components {
            root.components.remove(component);
        }
        for (component, record) in entity.components.iter().chain(&instance.added_components) {
            if instance.removed_components.contains(component) {
                return Err(PrefabError::AddRemoveConflict {
                    instance: mount.id,
                    component: component.clone(),
                });
            }
            let std::collections::btree_map::Entry::Vacant(entry) =
                root.components.entry(component.clone())
            else {
                return Err(PrefabError::DuplicateComponent {
                    instance: mount.id,
                    component: component.clone(),
                });
            };
            entry.insert(enclosing.component(record, Some(&scope)));
        }
        self.overrides(instance, mount.id, start, enclosing, &scope)?;
        self.instance_ids
            .record_completed(mount.id, instance.asset, scope.ids);
        Ok(())
    }

    fn overrides(
        &mut self,
        instance: &PrefabInstance,
        instance_id: EntityUuid,
        start: usize,
        enclosing: &Scope<'_>,
        prefab: &Scope<'_>,
    ) -> Result<(), PrefabError> {
        let mut groups: BTreeMap<(usize, ComponentTypeId), Vec<Override<'_>>> = BTreeMap::new();
        for (address, raw) in &instance.overrides {
            let invalid = |source| override_error(instance_id, address, source);
            let (child_path, tail) = split_target(address).map_err(invalid)?;
            let target = derive_path(instance_id, &child_path)
                .and_then(|target| self.indices.get(&target).copied())
                .filter(|index| *index >= start)
                .ok_or_else(|| invalid(PrefabOverrideError::MissingTarget))?;
            let path = PrefabOverridePath::parse_target(
                child_path,
                tail,
                self.schemas,
                &self.entities[target],
            )
            .map_err(invalid)?;
            groups
                .entry((target, path.component().clone()))
                .or_default()
                .push(Override {
                    address,
                    raw,
                    path,
                    previous: None,
                });
        }

        for ((index, component), mut overrides) in groups {
            overrides.sort_unstable_by(|left, right| {
                left.path
                    .field()
                    .cmp(right.path.field())
                    .then(left.address.cmp(right.address))
            });
            for pair in overrides.windows(2) {
                let [first, second] = pair else { continue };
                if second
                    .path
                    .field()
                    .segments()
                    .starts_with(first.path.field().segments())
                {
                    return Err(override_error(
                        instance_id,
                        second.address,
                        PrefabOverrideError::OverlappingField(first.address.to_owned()),
                    ));
                }
            }
            let Some(first_address) = overrides.first().map(|edit| edit.address) else {
                continue;
            };
            let invalid_component = |source| override_error(instance_id, first_address, source);
            let target = &mut self.entities[index];
            let target_id = target.id;
            let record = target.components.get_mut(&component).ok_or_else(|| {
                invalid_component(PrefabOverrideError::MissingComponent(component.clone()))
            })?;
            let SceneComponent::Known { version, value } = record else {
                if self.mode == Mode::Strict {
                    return Err(invalid_component(PrefabOverrideError::OpaqueComponent(
                        component,
                    )));
                }
                self.deferred_overrides.extend(overrides.iter().map(|edit| {
                    DeferredPrefabOverride {
                        instance: instance_id,
                        target: target_id,
                        address: edit.address.to_owned(),
                        component: component.clone(),
                    }
                }));
                continue;
            };
            let schema = find_schema(self.schemas, &component).ok_or_else(|| {
                invalid_component(PrefabOverrideError::MissingSchema(component.clone()))
            })?;
            if *version != schema.version() {
                return Err(invalid_component(PrefabOverrideError::VersionMismatch {
                    stored: *version,
                    current: schema.version(),
                }));
            }
            for edit in &mut overrides {
                let invalid = |source| override_error(instance_id, edit.address, source);
                let mut replacement =
                    decode_override_value(&edit.path, edit.raw, self.schemas, enclosing.assets)
                        .map_err(invalid)?;
                enclosing.remap(&mut replacement, Some(prefab));
                edit.previous = value
                    .set_path(edit.path.field(), replacement)
                    .map_err(|error| invalid(error.into()))?;
            }
            if let Err(source) = validate_component_value(schema, &*value) {
                if let Some(keys) = conflicting_list_keys(schema, value, &source)
                    && let Some(edit) = overrides
                        .iter()
                        .rev()
                        .find(|edit| keys.iter().any(|key| edit.changed_key(value, key)))
                {
                    return Err(override_error(instance_id, edit.address, source.into()));
                }
                return Err(PrefabError::InvalidDocument {
                    document: enclosing.document,
                    source: SceneDocumentError::InvalidComponentValue {
                        entity: target_id,
                        component,
                        source: source.into(),
                    },
                });
            }
        }
        Ok(())
    }
}

struct Override<'a> {
    address: &'a str,
    raw: &'a RawRon,
    path: PrefabOverridePath,
    previous: Option<ComponentValue>,
}

impl Override<'_> {
    fn changed_key(&self, value: &ComponentValue, key: &ComponentFieldPath) -> bool {
        let Some(relative) = key.segments().strip_prefix(self.path.field().segments()) else {
            return false;
        };
        let relative = ComponentFieldPath::new(relative.iter().cloned());
        self.previous
            .as_ref()
            .and_then(|previous| previous.get_path(&relative).ok())
            != value.get_path(key).ok()
    }
}

fn conflicting_list_keys(
    schema: &ComponentSchema,
    value: &ComponentValue,
    source: &ComponentCodecError,
) -> Option<[ComponentFieldPath; 2]> {
    if !matches!(source, ComponentCodecError::InvalidField { .. }) {
        return None;
    }
    let (field, error) = schema.fields().iter().find_map(|field| {
        let error = field
            .validate_value(value.get_path(field.path()).ok()?)
            .err()?;
        Some((field, error))
    })?;
    if !matches!(error.kind, ComponentShapeErrorKind::DuplicateListKey(_)) {
        return None;
    }
    let (ComponentFieldPathSegment::Index(duplicate), relative) =
        error.path.segments().split_last()?
    else {
        return None;
    };
    let shape = nested_shape(field.shape(), relative)?;
    let mut list_shape = shape.as_ref();
    while let ComponentValueShape::Optional { value } = list_shape {
        list_shape = value;
    }
    let ComponentValueShape::KeyedList { item, key } = list_shape else {
        return None;
    };
    let ComponentValueShape::Record { members } = item.as_ref() else {
        return None;
    };
    let member = members
        .iter()
        .find(|member| member.id.as_ref() == Some(key))?;
    let list_path =
        ComponentFieldPath::new(field.path().segments().iter().chain(relative).cloned());
    let ComponentValue::List(items) = value.get_path(&list_path).ok()? else {
        return None;
    };
    let duplicate_index = usize::try_from(*duplicate).ok()?;
    let repeated_key = items.get(duplicate_index)?.get(&member.name)?;
    let first = items
        .iter()
        .take(duplicate_index)
        .position(|item| item.get(&member.name) == Some(repeated_key))?;
    Some([u32::try_from(first).ok()?, *duplicate].map(|index| {
        ComponentFieldPath::new(list_path.segments().iter().cloned().chain([
            ComponentFieldPathSegment::index(index),
            ComponentFieldPathSegment::field(&member.name),
        ]))
    }))
}

fn override_error(instance: EntityUuid, address: &str, source: PrefabOverrideError) -> PrefabError {
    PrefabError::Override {
        instance,
        address: address.to_owned(),
        source,
    }
}

fn validate_source(document: &SceneDocument) -> Result<(), PrefabError> {
    if document.entities.len() > MAX_EXPANDED_ENTITIES {
        return Err(PrefabError::EntityLimit {
            limit: MAX_EXPANDED_ENTITIES,
        });
    }
    document
        .validate()
        .map_err(|source| PrefabError::InvalidDocument {
            document: document.id,
            source,
        })
}
