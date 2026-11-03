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
    match validate_contextual_defaults_with_admission(schema, record, limits, |_, _, _| {
        Ok::<_, std::convert::Infallible>(())
    }) {
        Ok(()) => Ok(()),
        Err(nara_reflect::__private::EffectiveValueAdmissionError::Value(error)) => Err(error),
        Err(nara_reflect::__private::EffectiveValueAdmissionError::Admission(error)) => {
            match error {}
        }
    }
}

/// The same source-default contract with work/ownership admission before planning and validation.
pub(crate) fn validate_contextual_defaults_with_admission<E>(
    schema: &nara_reflect::ComponentSchema,
    record: &SceneComponentRecord,
    limits: ComponentValueMaterializationLimits,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<(), nara_reflect::__private::EffectiveValueAdmissionError<E>> {
    use nara_reflect::__private::{
        ComponentPropertyAdmissionError, EffectiveValueAdmissionError,
        component_field_default_requires_context_with_admission,
        validate_component_property_value_with_admission,
    };
    use nara_reflect::{ComponentCapability, ComponentCodecError, ComponentValueKind};

    if !record.contextual_defaults.is_empty() && record.version != schema.version() {
        return Err(ComponentCodecError::invalid_field(
            "contextual defaults",
            "current record version",
        )
        .into());
    }
    for field in schema.fields() {
        admit(1, field.id().as_str().len(), 0).map_err(EffectiveValueAdmissionError::Admission)?;
        if component_field_default_requires_context_with_admission(field, &mut admit)?
            && !record.contextual_defaults.contains_key(field.id())
        {
            return Err(ComponentCodecError::invalid_field(
                "contextual defaults",
                "explicit source-dependent default",
            )
            .into());
        }
    }
    let mut nodes = 0usize;
    let mut bytes = 0usize;
    for (id, value) in &record.contextual_defaults {
        let mut selected = None;
        for field in schema.fields() {
            admit(
                1,
                field.id().as_str().len().saturating_add(id.as_str().len()),
                0,
            )
            .map_err(EffectiveValueAdmissionError::Admission)?;
            if field.id() == id {
                selected = Some(field);
                break;
            }
        }
        let field = selected
            .filter(|field| {
                field.default_value().is_some()
                    && field.has_capability(ComponentCapability::Scene)
                    && field.has_capability(ComponentCapability::EntityRef)
            })
            .ok_or_else(|| {
                ComponentCodecError::invalid_field(
                    "contextual defaults",
                    "declared reference default",
                )
            })?;
        admit(
            0,
            0,
            std::mem::size_of::<nara_reflect::ComponentPropertyPath>()
                .saturating_add(id.as_str().len()),
        )
        .map_err(EffectiveValueAdmissionError::Admission)?;
        let path = nara_reflect::ComponentPropertyPath::new(id.clone());
        let mut observed = (0usize, 0usize, 0usize);
        validate_component_property_value_with_admission(
            field.shape(),
            &path,
            value,
            limits,
            |nodes, bytes, allocated| {
                admit(nodes, bytes, allocated)?;
                observed.0 = observed.0.saturating_add(nodes);
                observed.1 = observed.1.saturating_add(bytes);
                observed.2 = observed.2.saturating_add(allocated);
                Ok(())
            },
        )
        .map_err(|error| match error {
            ComponentPropertyAdmissionError::Admission(error) => {
                EffectiveValueAdmissionError::Admission(error)
            }
            ComponentPropertyAdmissionError::Property(
                nara_reflect::ComponentPropertyError::InvalidValue(error),
            ) => EffectiveValueAdmissionError::Value(error),
            ComponentPropertyAdmissionError::Property(_) => EffectiveValueAdmissionError::Value(
                ComponentCodecError::invalid_field("contextual defaults", "declared field shape"),
            ),
        })?;
        // Aggregate source limits still use the ordinary raw cost. Its additional scan,
        // shape check and iterator scratch share the complete validator's upper bound.
        admit(observed.0, observed.1, observed.2)
            .map_err(EffectiveValueAdmissionError::Admission)?;
        if !field.shape().contains_kind(ComponentValueKind::EntityRef) {
            return Err(ComponentCodecError::invalid_field(
                "contextual defaults",
                "declared reference default",
            )
            .into());
        }
        let cost = value.cost();
        nodes = nodes.saturating_add(cost.nodes());
        bytes = bytes
            .saturating_add(cost.logical_bytes())
            .saturating_add(id.as_str().len());
        if nodes > limits.nodes().get() {
            return Err(ComponentCodecError::MaterializationNodeLimit {
                observed: nodes,
                maximum: limits.nodes().get(),
            }
            .into());
        }
        if bytes > limits.bytes().get() {
            return Err(ComponentCodecError::MaterializationByteLimit {
                observed: bytes,
                maximum: limits.bytes().get(),
            }
            .into());
        }
    }
    Ok(())
}

#[cfg(all(test, feature = "serde"))]
mod default_admission_tests {
    use super::*;
    use nara_reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
        ComponentValueKind,
    };

    #[test]
    fn no_default_fields_still_request_work_before_scanning_the_admitted_schema() {
        let component = ComponentTypeId::new("test.NoDefaultAdmission");
        let schema = ComponentSchema::new(
            component.clone(),
            "No defaults",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields((0..4_096).map(|index| {
            let name = format!("field{index}");
            ComponentFieldSchema::optional(
                ComponentFieldId::new(&name),
                &name,
                ComponentFieldPath::from_fields([name.clone()]),
                ComponentValueKind::U64,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        }));
        let owner = PersistentComponentSchemaOwnerId::try_new("test.no-defaults").unwrap();
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner.clone(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )])
        .unwrap();
        let adapter = crate::__private::AuthoringAdapterReceipt::from_schema_owner(
            &bundle.owners()[0],
            &Default::default(),
        )
        .unwrap();
        let basis = crate::AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
        let record = SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::Map(Default::default()),
        )
        .with_owner(owner);
        let marker = Box::new(std::io::Error::other("no field work remains"));
        let pointer = std::ptr::from_ref(marker.as_ref());
        let mut marker = Some(marker);
        let mut calls = 0;
        let result = validate_contextual_defaults_with_admission(
            basis.schema(&component).unwrap(),
            &record,
            Default::default(),
            |nodes, _, allocated| {
                calls += 1;
                assert_eq!(nodes, 1);
                assert_eq!(allocated, 0);
                Err(marker.take().unwrap())
            },
        );
        let Err(nara_reflect::__private::EffectiveValueAdmissionError::Admission(error)) = result
        else {
            panic!("the first field visit must consume the owner's allowance");
        };
        assert_eq!(calls, 1);
        assert!(std::ptr::eq(error.as_ref(), pointer));
    }
}
