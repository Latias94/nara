use std::collections::BTreeMap;

use nara_asset::{AssetDatabaseError, AssetReferenceResolver};
use nara_ecs::{
    __private::{
        PersistentComponentMetadataError, PersistentLifecycleEvent, PersistentObserverScope,
        validate_registered_persistent_component_apply,
    },
    Component, Entity, World,
    bevy_ecs::{lifecycle, world::WorldId},
};
use nara_identity::{
    EntityLookup, EntityReference, IdentityDomainError, RuntimeEntityReference, SceneInstanceId,
    WorldIdentityDomain,
};
use nara_reflect::{
    __private::effective_component_value_with_budget, ComponentCapability, ComponentCodecError,
    ComponentRegistryAuthorityError, ComponentSchema, ComponentSchemaOwnerSnapshot,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueCost,
    ComponentValueMaterializationLimits, DeclaredAssetReferenceError,
    collect_declared_asset_references_from_schema_with_limits, component_registry,
    validate_component_registry_authority, validate_component_value_with_limits,
};

use crate::{
    SceneComponentBindings, SceneComponentBindingsAuthorityError, SceneComponentRecord,
    SceneEntitySource, scene_component_bindings, validate_scene_component_bindings_authority,
    validation::{reference_traversal_limits, validate_effective_local_references},
};

/// The complete data-only component group attached to one live Scene entity.
///
/// The carrier deliberately stores only admitted Schema ownership and the persistent record. It
/// does not retain the native registry, executable codecs, a language runtime, or a mutable
/// binding selection. The World/entity witness is transient runtime custody: it makes reads and
/// export reject a carrier that was copied through an unguarded ECS operation.
#[derive(Component)]
pub struct SceneDataComponents {
    world: WorldId,
    entity: Entity,
    entries: BTreeMap<ComponentTypeId, SceneDataComponent>,
}

pub(crate) struct SceneDataComponent {
    owner: ComponentSchemaOwnerSnapshot,
    record: SceneComponentRecord,
}

impl SceneDataComponents {
    pub(crate) fn new(
        world: &World,
        entity: Entity,
        entries: impl IntoIterator<
            Item = (
                ComponentTypeId,
                ComponentSchemaOwnerSnapshot,
                SceneComponentRecord,
            ),
        >,
    ) -> Self {
        let entries = entries
            .into_iter()
            .map(|(component, owner, record)| (component, SceneDataComponent { owner, record }))
            .collect();
        Self {
            world: world.id(),
            entity,
            entries,
        }
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (&ComponentTypeId, &SceneDataComponent)> {
        self.entries.iter()
    }

    pub(crate) fn matches_location(&self, world: &World, entity: Entity) -> bool {
        self.world == world.id() && self.entity == entity
    }
}

impl SceneDataComponent {
    pub(crate) fn owner(&self) -> &ComponentSchemaOwnerSnapshot {
        &self.owner
    }

    pub(crate) fn record(&self) -> &SceneComponentRecord {
        &self.record
    }
}

/// A guarded data operation was refused before changing any Scene records.
#[derive(Debug)]
pub enum SceneDataComponentError {
    NativeAuthority(ComponentRegistryAuthorityError),
    BindingsAuthority(SceneComponentBindingsAuthorityError),
    BindingsMismatch,
    NotSceneEntity,
    Identity(IdentityDomainError),
    CarrierLocationMismatch,
    BindingUnavailable {
        component: ComponentTypeId,
    },
    OwnerMismatch {
        component: ComponentTypeId,
    },
    VersionMismatch {
        component: ComponentTypeId,
        expected: ComponentSchemaVersion,
        actual: ComponentSchemaVersion,
    },
    SchemaInvalid {
        component: ComponentTypeId,
    },
    IncompleteValue {
        component: ComponentTypeId,
    },
    InvalidValue {
        component: ComponentTypeId,
        source: ComponentCodecError,
    },
    AssetReferences {
        component: ComponentTypeId,
        source: Box<DeclaredAssetReferenceError>,
    },
    AssetUnavailable {
        component: ComponentTypeId,
        source: AssetDatabaseError,
    },
    LocalReferenceInvalid {
        component: ComponentTypeId,
    },
    Lifecycle(PersistentComponentMetadataError),
}

impl std::fmt::Display for SceneDataComponentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NativeAuthority(error) => write!(formatter, "Scene native authority: {error}"),
            Self::BindingsAuthority(error) => write!(formatter, "Scene binding authority: {error}"),
            Self::BindingsMismatch => {
                formatter.write_str("Scene bindings differ from the installed admission")
            }
            Self::NotSceneEntity => {
                formatter.write_str("target is not a current live Scene entity")
            }
            Self::Identity(error) => write!(formatter, "Scene target identity: {error}"),
            Self::CarrierLocationMismatch => formatter
                .write_str("Scene data carrier belongs to another World or entity incarnation"),
            Self::BindingUnavailable { component } => {
                write!(formatter, "data binding is unavailable for '{component}'")
            }
            Self::OwnerMismatch { component } => write!(
                formatter,
                "data component '{component}' has a different owner admission"
            ),
            Self::VersionMismatch {
                component,
                expected,
                actual,
            } => write!(
                formatter,
                "data component '{component}' has version {actual:?}, expected {expected:?}"
            ),
            Self::SchemaInvalid { component } => write!(
                formatter,
                "data Schema '{component}' does not support complete Scene values"
            ),
            Self::IncompleteValue { component } => write!(
                formatter,
                "stored data component '{component}' is missing materialized defaults"
            ),
            Self::InvalidValue { component, source } => write!(
                formatter,
                "data component '{component}' is invalid: {source}"
            ),
            Self::AssetReferences { component, source } => write!(
                formatter,
                "data component '{component}' has invalid asset references: {source}"
            ),
            Self::AssetUnavailable { component, source } => write!(
                formatter,
                "data component '{component}' requires an unavailable asset: {source}"
            ),
            Self::LocalReferenceInvalid { component } => write!(
                formatter,
                "data component '{component}' has an unresolved live Scene-local reference"
            ),
            Self::Lifecycle(error) => write!(
                formatter,
                "Scene data carrier has unsupported lifecycle work: {error:?}"
            ),
        }
    }
}

impl std::error::Error for SceneDataComponentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NativeAuthority(error) => Some(error),
            Self::BindingsAuthority(error) => Some(error),
            Self::Identity(error) => Some(error),
            Self::InvalidValue { source, .. } => Some(source),
            Self::AssetReferences { source, .. } => Some(source.as_ref()),
            Self::AssetUnavailable { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl SceneComponentBindings {
    /// Enumerates complete current membership without copying records or exposing mutable storage.
    pub fn data_components<'world>(
        &self,
        world: &'world World,
        entity: Entity,
    ) -> Result<
        impl Iterator<Item = (&'world ComponentTypeId, &'world SceneComponentRecord)> + use<'world>,
        SceneDataComponentError,
    > {
        self.validate_data_target(world, entity)?;
        self.validate_data_group(
            world,
            entity,
            None,
            ComponentValueMaterializationLimits::default(),
            None,
        )?;
        Ok(world
            .get::<SceneDataComponents>(entity)
            .into_iter()
            .flat_map(SceneDataComponents::entries)
            .map(|(component, entry)| (component, entry.record())))
    }

    /// Borrows a current member after checking the complete group under the default value limits.
    ///
    /// Absence is distinct from an unavailable binding, invalid sibling or stale entity. Asset
    /// syntax is checked without loading assets; this read does not require project metadata.
    pub fn data_component<'world>(
        &self,
        world: &'world World,
        entity: Entity,
        component: &ComponentTypeId,
    ) -> Result<Option<&'world SceneComponentRecord>, SceneDataComponentError> {
        self.validate_data_target(world, entity)?;
        self.data_schema(component)?;
        self.validate_data_group(
            world,
            entity,
            None,
            ComponentValueMaterializationLimits::default(),
            None,
        )?;
        Ok(world
            .get::<SceneDataComponents>(entity)
            .and_then(|group| group.entries.get(component))
            .map(SceneDataComponent::record))
    }

    /// Replaces one complete data member, preserving every sibling.
    ///
    /// `limits` bounds the complete resulting group's effective logical value. The replaced
    /// member is independently validated under the default read limits. Definitions, identities,
    /// every existing payload, references and lifecycle metadata are checked before any write.
    /// Project metadata, when supplied, must resolve all declared assets in the group. This never
    /// invokes a native codec or creates asset handles.
    pub fn set_data_component(
        &self,
        world: &mut World,
        entity: Entity,
        component: &ComponentTypeId,
        value: ComponentValue,
        asset_resolution: Option<AssetReferenceResolver<'_>>,
        limits: ComponentValueMaterializationLimits,
    ) -> Result<(), SceneDataComponentError> {
        let instance = self.validate_data_target(world, entity)?;
        let schema = self.data_schema(component)?;
        let retained =
            self.validate_data_group(world, entity, asset_resolution, limits, Some(component))?;
        let (effective, _) = effective_component_value_with_budget(
            schema,
            &value,
            limits.nodes().get() - retained.nodes(),
            limits.bytes().get() - retained.logical_bytes(),
            limits.depth(),
        )
        .map_err(|source| SceneDataComponentError::InvalidValue {
            component: component.clone(),
            source,
        })?;
        validate_data_value(
            world,
            instance,
            schema,
            &effective,
            asset_resolution,
            limits,
        )?;
        // A no-default candidate can be moved directly; only Reflect's admitted missing defaults
        // require an owning copy. No sibling record or source document is copied.
        let value = match effective {
            std::borrow::Cow::Borrowed(_) => value,
            std::borrow::Cow::Owned(value) => value,
        };
        let owner = self.data_owner(component).ok_or_else(|| {
            SceneDataComponentError::BindingUnavailable {
                component: component.clone(),
            }
        })?;
        let entry = SceneDataComponent {
            owner: owner.clone(),
            record: SceneComponentRecord::new(schema.version(), value)
                .with_owner(owner.owner_id().clone()),
        };
        if let Some(mut group) = world.get_mut::<SceneDataComponents>(entity) {
            group.entries.insert(component.clone(), entry);
        } else {
            let group = SceneDataComponents::new(
                world,
                entity,
                [(component.clone(), entry.owner, entry.record)],
            );
            world.entity_mut(entity).insert(group);
        }
        Ok(())
    }

    /// Removes a member under the default value limits, returning whether it was present.
    pub fn remove_data_component(
        &self,
        world: &mut World,
        entity: Entity,
        component: &ComponentTypeId,
    ) -> Result<bool, SceneDataComponentError> {
        self.remove_data_component_with_limits(
            world,
            entity,
            component,
            None,
            ComponentValueMaterializationLimits::default(),
        )
    }

    /// Removes a member after checking all current payloads and the retained group's limits.
    /// The removed member uses the default read limits, as in full-value replacement.
    pub fn remove_data_component_with_limits(
        &self,
        world: &mut World,
        entity: Entity,
        component: &ComponentTypeId,
        asset_resolution: Option<AssetReferenceResolver<'_>>,
        limits: ComponentValueMaterializationLimits,
    ) -> Result<bool, SceneDataComponentError> {
        self.validate_data_target(world, entity)?;
        self.data_schema(component)?;
        self.validate_data_group(world, entity, asset_resolution, limits, Some(component))?;
        if !world
            .get::<SceneDataComponents>(entity)
            .is_some_and(|group| group.entries.contains_key(component))
        {
            return Ok(false);
        }
        let empty = {
            let mut group = world
                .get_mut::<SceneDataComponents>(entity)
                .ok_or(SceneDataComponentError::NotSceneEntity)?;
            group.entries.remove(component);
            group.entries.is_empty()
        };
        if empty {
            world.entity_mut(entity).remove::<SceneDataComponents>();
        }
        Ok(true)
    }

    fn data_schema(
        &self,
        component: &ComponentTypeId,
    ) -> Result<&ComponentSchema, SceneDataComponentError> {
        let schema = self
            .data_owner(component)
            .and_then(|owner| owner.schema(component))
            .ok_or_else(|| SceneDataComponentError::BindingUnavailable {
                component: component.clone(),
            })?;
        nara_reflect::__private::validate_schema_capabilities(schema, [ComponentCapability::Scene])
            .map_err(|_| SceneDataComponentError::SchemaInvalid {
                component: component.clone(),
            })?;
        Ok(schema)
    }

    fn validate_data_target(
        &self,
        world: &World,
        entity: Entity,
    ) -> Result<SceneInstanceId, SceneDataComponentError> {
        let standalone = match (
            component_registry(world),
            validate_component_registry_authority(world),
        ) {
            (None, Err(ComponentRegistryAuthorityError::MissingAuthority)) => true,
            (Some(registry), Ok(())) if registry.shares_snapshot(self.native_snapshot()) => false,
            (_, Err(error)) => return Err(SceneDataComponentError::NativeAuthority(error)),
            _ => return Err(SceneDataComponentError::BindingsMismatch),
        };
        match (
            scene_component_bindings(world),
            validate_scene_component_bindings_authority(world),
        ) {
            (None, Err(SceneComponentBindingsAuthorityError::MissingAuthority)) if standalone => {}
            (Some(installed), Ok(())) if !standalone && self.same_definitions(installed) => {}
            (_, Err(error)) => return Err(SceneDataComponentError::BindingsAuthority(error)),
            _ => return Err(SceneDataComponentError::BindingsMismatch),
        }
        let source = world
            .get::<SceneEntitySource>(entity)
            .ok_or(SceneDataComponentError::NotSceneEntity)?;
        let domain = world
            .get_resource::<WorldIdentityDomain>()
            .ok_or(SceneDataComponentError::NotSceneEntity)?;
        let locators = domain
            .locators_for_entity(world, entity)
            .map_err(SceneDataComponentError::Identity)?
            .ok_or(SceneDataComponentError::NotSceneEntity)?;
        let locator = locators
            .scene()
            .ok_or(SceneDataComponentError::NotSceneEntity)?;
        if !matches!(locator.entity(), RuntimeEntityReference::Scene { instance, entity: id, .. }
            if *instance == source.instance_id && *id == source.entity_id)
            || domain.lookup(world, locator.entity()) != EntityLookup::Resolved(entity)
        {
            return Err(SceneDataComponentError::NotSceneEntity);
        }
        if world
            .get::<SceneDataComponents>(entity)
            .is_some_and(|group| !group.matches_location(world, entity))
        {
            return Err(SceneDataComponentError::CarrierLocationMismatch);
        }
        validate_carrier_lifecycle(world, entity)?;
        Ok(source.instance_id)
    }

    fn validate_data_group(
        &self,
        world: &World,
        entity: Entity,
        asset_resolution: Option<AssetReferenceResolver<'_>>,
        limits: ComponentValueMaterializationLimits,
        replaced: Option<&ComponentTypeId>,
    ) -> Result<ComponentValueCost, SceneDataComponentError> {
        let mut retained = ComponentValueCost::ZERO;
        let Some(group) = world.get::<SceneDataComponents>(entity) else {
            return Ok(retained);
        };
        let instance = world
            .get::<SceneEntitySource>(entity)
            .ok_or(SceneDataComponentError::NotSceneEntity)?
            .instance_id;
        for (component, entry) in group.entries() {
            let schema = self.data_schema(component)?;
            let owner = self.data_owner(component).ok_or_else(|| {
                SceneDataComponentError::BindingUnavailable {
                    component: component.clone(),
                }
            })?;
            let record = entry.record();
            if !owner.ptr_eq(entry.owner()) || record.owner() != Some(owner.owner_id()) {
                return Err(SceneDataComponentError::OwnerMismatch {
                    component: component.clone(),
                });
            }
            if record.version != schema.version() {
                return Err(SceneDataComponentError::VersionMismatch {
                    component: component.clone(),
                    expected: schema.version(),
                    actual: record.version,
                });
            }
            // Stored members were fully materialized at publication. Reject corrupted sparse
            // storage before a default processor could copy it during group validation.
            if schema.fields().iter().any(|field| {
                field.default_value().is_some() && record.value.get_path(field.path()).is_err()
            }) {
                return Err(SceneDataComponentError::IncompleteValue {
                    component: component.clone(),
                });
            }
            let removed = replaced == Some(component);
            let admission = if removed {
                ComponentValueMaterializationLimits::default()
            } else {
                limits
            };
            let (nodes, bytes) = if removed {
                (admission.nodes().get(), admission.bytes().get())
            } else {
                (
                    limits.nodes().get() - retained.nodes(),
                    limits.bytes().get() - retained.logical_bytes(),
                )
            };
            let (_, cost) = effective_component_value_with_budget(
                schema,
                &record.value,
                nodes,
                bytes,
                admission.depth(),
            )
            .map_err(|source| SceneDataComponentError::InvalidValue {
                component: component.clone(),
                source,
            })?;
            if !removed {
                retained = retained.saturating_add(cost);
            }
        }
        // Admit the whole group before reference walkers allocate their bounded scratch.
        for (component, entry) in group.entries() {
            let admission = if replaced == Some(component) {
                ComponentValueMaterializationLimits::default()
            } else {
                limits
            };
            validate_data_value(
                world,
                instance,
                self.data_schema(component)?,
                &entry.record().value,
                asset_resolution,
                admission,
            )?;
        }
        Ok(retained)
    }
}

fn validate_data_value(
    world: &World,
    instance: SceneInstanceId,
    schema: &ComponentSchema,
    value: &ComponentValue,
    asset_resolution: Option<AssetReferenceResolver<'_>>,
    limits: ComponentValueMaterializationLimits,
) -> Result<(), SceneDataComponentError> {
    let component = schema.id();
    validate_component_value_with_limits(schema, value, limits).map_err(|source| {
        SceneDataComponentError::InvalidValue {
            component: component.clone(),
            source,
        }
    })?;
    let traversal = reference_traversal_limits(limits);
    let assets = collect_declared_asset_references_from_schema_with_limits(
        schema,
        schema.version(),
        value,
        limits,
        traversal,
    )
    .map_err(|source| SceneDataComponentError::AssetReferences {
        component: component.clone(),
        source: Box::new(source),
    })?;
    if let Some(asset_resolution) = asset_resolution {
        for reference in assets {
            let observed = asset_resolution
                .resolution_work_bytes(reference.asset_ref())
                .unwrap_or(usize::MAX);
            let maximum = limits.bytes().get();
            if observed > maximum {
                return Err(SceneDataComponentError::InvalidValue {
                    component: component.clone(),
                    source: ComponentCodecError::MaterializationByteLimit { observed, maximum },
                });
            }
            asset_resolution
                .resolve_ref(reference.asset_ref())
                .map_err(|source| SceneDataComponentError::AssetUnavailable {
                    component: component.clone(),
                    source,
                })?;
        }
    }
    let domain = world
        .get_resource::<WorldIdentityDomain>()
        .ok_or(SceneDataComponentError::NotSceneEntity)?;
    validate_effective_local_references(
        schema,
        value,
        |id| {
            let reference = EntityReference::SceneLocal { entity: id.clone() };
            match domain.resolve_entity_reference(world, &reference, Some(instance)) {
                EntityLookup::Resolved(target) => world
                    .get::<SceneEntitySource>(target)
                    .is_some_and(|source| {
                        source.instance_id == instance && source.entity_id == *id
                    }),
                _ => false,
            }
        },
        traversal,
    )
    .map_err(|_| SceneDataComponentError::LocalReferenceInvalid {
        component: component.clone(),
    })
}

fn validate_carrier_lifecycle(
    world: &World,
    entity: Entity,
) -> Result<(), SceneDataComponentError> {
    if world.component_id::<SceneDataComponents>().is_some() {
        return validate_registered_persistent_component_apply::<SceneDataComponents>(
            world,
            Some(entity),
        )
        .map_err(SceneDataComponentError::Lifecycle);
    }
    // Before the first carrier registration, component-scoped observers and custom metadata
    // cannot exist. Global and entity-scoped observers still can; reject them without registering
    // anything in a failing operation. The concrete carrier has no intrinsic hooks/requirements.
    for (event, key) in [
        (PersistentLifecycleEvent::Add, lifecycle::ADD),
        (PersistentLifecycleEvent::Insert, lifecycle::INSERT),
        (PersistentLifecycleEvent::Discard, lifecycle::DISCARD),
        (PersistentLifecycleEvent::Remove, lifecycle::REMOVE),
        (PersistentLifecycleEvent::Despawn, lifecycle::DESPAWN),
    ] {
        let Some(observers) = world.observers().try_get_observers(key) else {
            continue;
        };
        let scope = if !observers.global_observers().is_empty() {
            Some(PersistentObserverScope::EventGlobal)
        } else if observers
            .entity_observers()
            .get(&entity)
            .is_some_and(|observers| !observers.is_empty())
        {
            Some(PersistentObserverScope::Entity)
        } else {
            None
        };
        if let Some(scope) = scope {
            return Err(SceneDataComponentError::Lifecycle(
                PersistentComponentMetadataError::Observer { event, scope },
            ));
        }
    }
    Ok(())
}
