use std::collections::BTreeMap;

use nara_asset::AssetReferenceResolver;
use nara_diagnostic::DiagnosticReport;
use nara_identity::SceneEntityId;
use nara_reflect::{
    ComponentDecodeContext, ComponentRegistrySnapshot, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueMaterializationLimits, PersistentComponentSchemaOwnerId,
};

use crate::{
    PrefabInstance, SceneComponentBindings, SceneEntityPlacement,
    validation::{
        preflight_authoring_scene, preflight_authoring_scene_for_patch,
        preflight_authoring_scene_with_context, preflight_authoring_scene_with_context_for_patch,
        preflight_scene, preflight_scene_with_context, preflight_scene_with_context_and_bindings,
    },
};

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SceneDocument {
    pub placements: Vec<SceneEntityPlacement>,
    pub entities: Vec<SceneEntityRecord>,
}

impl SceneDocument {
    #[must_use]
    pub fn new(
        placements: impl IntoIterator<Item = SceneEntityPlacement>,
        entities: impl IntoIterator<Item = SceneEntityRecord>,
    ) -> Self {
        let mut entities = entities.into_iter().collect::<Vec<_>>();
        entities.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            placements: placements.into_iter().collect(),
            entities,
        }
    }

    /// Declares every input entity as a root, preserving the input order in explicit placements.
    #[must_use]
    pub fn from_ordered_roots(entities: impl IntoIterator<Item = SceneEntityRecord>) -> Self {
        let entities = entities.into_iter().collect::<Vec<_>>();
        let placements = entities
            .iter()
            .map(|entity| SceneEntityPlacement::root(entity.id.clone()))
            .collect::<Vec<_>>();
        Self::new(placements, entities)
    }

    pub fn canonicalize(&mut self) {
        self.entities.sort_by(|left, right| left.id.cmp(&right.id));
    }

    #[must_use]
    pub fn validate(&self, registry: &ComponentRegistrySnapshot) -> DiagnosticReport {
        preflight_scene(self, registry).diagnostics
    }

    #[must_use]
    pub fn validate_with_asset_resolution(
        &self,
        registry: &ComponentRegistrySnapshot,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> DiagnosticReport {
        let mut context = ComponentDecodeContext::new().with_asset_resolution(asset_resolution);
        preflight_scene_with_context(self, registry, &mut context).diagnostics
    }

    pub(crate) fn validate_authoring(
        &self,
        registry: &ComponentRegistrySnapshot,
    ) -> DiagnosticReport {
        preflight_authoring_scene(self, registry).diagnostics
    }

    pub(crate) fn validate_authoring_for_patch(
        &self,
        registry: &ComponentRegistrySnapshot,
        operation_index: usize,
    ) -> DiagnosticReport {
        preflight_authoring_scene_for_patch(self, registry, operation_index).diagnostics
    }

    pub(crate) fn validate_authoring_with_asset_resolution(
        &self,
        registry: &ComponentRegistrySnapshot,
        asset_resolution: AssetReferenceResolver<'_>,
    ) -> DiagnosticReport {
        let mut context = ComponentDecodeContext::new().with_asset_resolution(asset_resolution);
        preflight_authoring_scene_with_context(self, registry, &mut context).diagnostics
    }

    pub(crate) fn validate_authoring_for_patch_with_asset_resolution(
        &self,
        registry: &ComponentRegistrySnapshot,
        asset_resolution: AssetReferenceResolver<'_>,
        operation_index: usize,
    ) -> DiagnosticReport {
        let mut context = ComponentDecodeContext::new().with_asset_resolution(asset_resolution);
        preflight_authoring_scene_with_context_for_patch(
            self,
            registry,
            &mut context,
            operation_index,
        )
        .diagnostics
    }
}

impl SceneComponentBindings {
    /// Checks a complete compiled batch against these receiving definitions before any codec.
    /// This neither migrates source values nor supplies defaults from a different definition.
    #[must_use]
    pub fn validate_compiled_scene(
        &self,
        document: &crate::CompiledSceneDocument,
        asset_resolution: Option<AssetReferenceResolver<'_>>,
        limits: ComponentValueMaterializationLimits,
    ) -> DiagnosticReport {
        let mut context = ComponentDecodeContext::new().with_materialization_limits(limits);
        if let Some(resolver) = asset_resolution {
            context = context.with_asset_resolution(resolver);
        }
        crate::validation::preflight_compiled_scene_with_context(
            document,
            self.native_snapshot(),
            Some(self),
            &mut context,
            false,
        )
        .diagnostics
    }

    /// Compiles and validates a complete Scene through the shared native/data preflight.
    ///
    /// Native records use the selected real codecs and migrations; data records require their
    /// admitted owner, current version, Schema and declared references. `limits` bounds aggregate
    /// retained effective component values across both families. When supplied, `asset_resolution`
    /// checks declared asset references against project metadata without loading assets.
    /// This does not access a `World`, mutate the source or publish executable state. Sources
    /// that require a Prefab resolver must use the complete compilation operation with that resolver.
    #[must_use]
    pub fn validate_scene(
        &self,
        document: &SceneDocument,
        asset_resolution: Option<AssetReferenceResolver<'_>>,
        limits: ComponentValueMaterializationLimits,
    ) -> DiagnosticReport {
        let mut context = ComponentDecodeContext::new().with_materialization_limits(limits);
        if let Some(asset_resolution) = asset_resolution {
            context = context.with_asset_resolution(asset_resolution);
        }
        preflight_scene_with_context_and_bindings(
            document,
            self.native_snapshot(),
            self,
            &mut context,
        )
        .diagnostics
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SceneEntityRecord {
    pub id: SceneEntityId,
    #[cfg_attr(feature = "serde", serde(default))]
    pub components: BTreeMap<ComponentTypeId, SceneComponentRecord>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub prefab: Option<PrefabInstance>,
}

impl SceneEntityRecord {
    #[must_use]
    pub fn new(id: SceneEntityId) -> Self {
        Self {
            id,
            components: BTreeMap::new(),
            prefab: None,
        }
    }

    #[must_use]
    pub fn with_component(
        mut self,
        component_type: ComponentTypeId,
        component: SceneComponentRecord,
    ) -> Self {
        self.components.insert(component_type, component);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SceneComponentRecord {
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub owner: Option<PersistentComponentSchemaOwnerId>,
    pub version: ComponentSchemaVersion,
    pub value: ComponentValue,
    /// Source-dependent defaults bound to this record's version and stable root-field IDs.
    /// Explicit values may hide them without removing their declaration.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "BTreeMap::is_empty")
    )]
    pub contextual_defaults: BTreeMap<nara_reflect::ComponentFieldId, ComponentValue>,
}

impl SceneComponentRecord {
    #[must_use]
    pub fn new(version: ComponentSchemaVersion, value: ComponentValue) -> Self {
        Self {
            owner: None,
            version,
            value,
            contextual_defaults: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_owner(mut self, owner: PersistentComponentSchemaOwnerId) -> Self {
        self.owner = Some(owner);
        self
    }

    #[must_use]
    pub const fn owner(&self) -> Option<&PersistentComponentSchemaOwnerId> {
        self.owner.as_ref()
    }

    #[must_use]
    pub fn with_contextual_default(
        mut self,
        field: nara_reflect::ComponentFieldId,
        value: ComponentValue,
    ) -> Self {
        self.contextual_defaults.insert(field, value);
        self
    }
}

/// Validates source metadata without replacing the canonical Schema or interpreting targets.
pub(crate) fn validate_contextual_defaults(
    schema: &nara_reflect::ComponentSchema,
    record: &SceneComponentRecord,
    limits: ComponentValueMaterializationLimits,
) -> Result<(), nara_reflect::ComponentCodecError> {
    use nara_reflect::{ComponentCapability, ComponentCodecError, ComponentValueKind};

    if !record.contextual_defaults.is_empty() && record.version != schema.version() {
        return Err(ComponentCodecError::invalid_field(
            "contextual defaults",
            "current record version",
        ));
    }
    for field in schema.fields() {
        if nara_reflect::__private::component_field_default_requires_context(field)?
            && !record.contextual_defaults.contains_key(field.id())
        {
            return Err(ComponentCodecError::invalid_field(
                "contextual defaults",
                "explicit source-dependent default",
            ));
        }
    }
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    for (id, value) in &record.contextual_defaults {
        let field = schema
            .fields()
            .iter()
            .find(|field| field.id() == id)
            .filter(|field| {
                field.default_value().is_some()
                    && field.has_capability(ComponentCapability::Scene)
                    && field.has_capability(ComponentCapability::EntityRef)
                    && field.shape().contains_kind(ComponentValueKind::EntityRef)
            })
            .ok_or_else(|| {
                ComponentCodecError::invalid_field(
                    "contextual defaults",
                    "declared reference default",
                )
            })?;
        nara_reflect::__private::validate_value_materialization_budget(value, limits)?;
        let cost = value.cost();
        nodes = nodes.saturating_add(cost.nodes());
        bytes = bytes
            .saturating_add(cost.logical_bytes())
            .saturating_add(id.as_str().len());
        if nodes > limits.nodes().get() {
            return Err(ComponentCodecError::MaterializationNodeLimit {
                observed: nodes,
                maximum: limits.nodes().get(),
            });
        }
        if bytes > limits.bytes().get() {
            return Err(ComponentCodecError::MaterializationByteLimit {
                observed: bytes,
                maximum: limits.bytes().get(),
            });
        }
        field.shape().validate_value(value).map_err(|_| {
            ComponentCodecError::invalid_field("contextual defaults", "declared field shape")
        })?;
    }
    Ok(())
}
