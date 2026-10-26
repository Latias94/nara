use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_asset::{AssetPathError, AssetRef, StableAssetIdError};

use crate::{
    ComponentCodecError, ComponentEntityReferenceRewriteError, ComponentFieldId,
    ComponentFieldPath, ComponentFieldPathError, ComponentFieldSchema, ComponentRegistry,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueMaterializationLimits, ComponentValueShape, EntityReferenceTraversalLimits,
    effective_component_value,
    entity_reference::{
        PresentReferenceLeaf, ReferenceTraversalError, collect_present_field_reference_leaves,
        collect_present_reference_leaves, collect_present_reference_leaves_with_admission,
        collect_present_value_reference_leaves,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredAssetReference {
    field_id: ComponentFieldId,
    path: ComponentFieldPath,
    asset_ref: AssetRef,
}

impl DeclaredAssetReference {
    #[must_use]
    pub const fn field_id(&self) -> &ComponentFieldId {
        &self.field_id
    }

    #[must_use]
    pub const fn path(&self) -> &ComponentFieldPath {
        &self.path
    }

    #[must_use]
    pub const fn asset_ref(&self) -> &AssetRef {
        &self.asset_ref
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredAssetReferenceError {
    RegistryNotFrozen,
    UnknownComponent {
        component: ComponentTypeId,
    },
    SchemaVersionMismatch {
        component: ComponentTypeId,
        expected: ComponentSchemaVersion,
        actual: ComponentSchemaVersion,
    },
    FieldPath {
        component: ComponentTypeId,
        field: ComponentFieldId,
        source: ComponentFieldPathError,
    },
    RequiredFieldIsNull {
        component: ComponentTypeId,
        field: ComponentFieldId,
    },
    InvalidValueShape {
        component: ComponentTypeId,
        field: ComponentFieldId,
        path: ComponentFieldPath,
    },
    InvalidKind {
        component: ComponentTypeId,
        field: ComponentFieldId,
        path: ComponentFieldPath,
    },
    InvalidPath {
        component: ComponentTypeId,
        field: ComponentFieldId,
        path: ComponentFieldPath,
        source: AssetPathError,
    },
    InvalidStableId {
        component: ComponentTypeId,
        field: ComponentFieldId,
        path: ComponentFieldPath,
        source: StableAssetIdError,
    },
    Materialization {
        component: ComponentTypeId,
        source: ComponentCodecError,
    },
    Traversal {
        component: ComponentTypeId,
        source: ComponentEntityReferenceRewriteError<()>,
    },
}

impl Display for DeclaredAssetReferenceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RegistryNotFrozen => {
                "component registry must be frozen before asset reference traversal"
            }
            Self::UnknownComponent { .. } => {
                "asset reference traversal targeted an unknown component"
            }
            Self::SchemaVersionMismatch { .. } => {
                "asset reference traversal requires the current component schema version"
            }
            Self::FieldPath { .. } => {
                "declared asset reference field could not be resolved in the component value"
            }
            Self::RequiredFieldIsNull { .. } => "required asset reference field contains null",
            Self::InvalidValueShape { .. } => {
                "declared asset reference field has an invalid value shape"
            }
            Self::InvalidKind { .. } => {
                "declared asset reference field has an unsupported reference kind"
            }
            Self::InvalidPath { .. } => "declared asset reference contains an invalid path",
            Self::InvalidStableId { .. } => {
                "declared asset reference contains an invalid stable ID"
            }
            Self::Materialization { .. } => {
                "declared asset reference default materialization rejected the effective value"
            }
            Self::Traversal { .. } => {
                "declared asset reference traversal rejected component structure"
            }
        })
    }
}

impl Error for DeclaredAssetReferenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::FieldPath { source, .. } => Some(source),
            Self::InvalidPath { source, .. } => Some(source),
            Self::InvalidStableId { source, .. } => Some(source),
            Self::Materialization { source, .. } => Some(source),
            Self::RegistryNotFrozen
            | Self::UnknownComponent { .. }
            | Self::SchemaVersionMismatch { .. }
            | Self::RequiredFieldIsNull { .. }
            | Self::InvalidValueShape { .. }
            | Self::InvalidKind { .. }
            | Self::Traversal { .. } => None,
        }
    }
}

/// Keeps the caller's allocation failure separate from schema/reference interpretation errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetReferenceCollectionError<E> {
    Reference(DeclaredAssetReferenceError),
    Admission(E),
}

/// Collects assets from a borrowed, already normalized value and an admitted current schema.
///
/// The Scene compiler owns schema/value admission and default materialization. This entry never
/// fills defaults or clones the input tree. The callback receives cumulative-work deltas:
/// visited nodes, logical bytes, and modeled allocation bytes. Scratch growth and each owned
/// output are admitted before allocation. Allocation includes the returned Vec's slots, leaf
/// scratch, paths, field IDs and parsed asset paths (or failed stable-ID text); a caller's
/// aggregate Vec is separate.
/// Whole new Vec requests are charged on growth, with no refunds for discarded scratch.
///
/// A denied callback returns its exact error without allocating an error path. No callback
/// reservation is made from the configured worst-case limits.
pub fn collect_declared_effective_asset_references_with_admission<E>(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
    limits: EntityReferenceTraversalLimits,
    mut admit: impl FnMut(usize, usize, usize) -> Result<(), E>,
) -> Result<Vec<DeclaredAssetReference>, AssetReferenceCollectionError<E>> {
    let component = schema.id();
    if version != schema.version() {
        admit(0, 0, component.as_str().len()).map_err(AssetReferenceCollectionError::Admission)?;
        return Err(AssetReferenceCollectionError::Reference(
            DeclaredAssetReferenceError::SchemaVersionMismatch {
                component: component.clone(),
                expected: schema.version(),
                actual: version,
            },
        ));
    }
    let leaves = match collect_present_reference_leaves_with_admission::<(), E>(
        schema,
        value,
        limits,
        ComponentValueKind::AssetRef,
        true,
        &mut admit,
    ) {
        Ok(leaves) => leaves,
        Err(ReferenceTraversalError::Admission(error)) => {
            return Err(AssetReferenceCollectionError::Admission(error));
        }
        Err(ReferenceTraversalError::Reference(source)) => {
            admit(0, 0, component.as_str().len())
                .map_err(AssetReferenceCollectionError::Admission)?;
            return Err(AssetReferenceCollectionError::Reference(
                DeclaredAssetReferenceError::Traversal {
                    component: component.clone(),
                    source,
                },
            ));
        }
    };
    // Each slot, field ID and parsed path was admitted at the borrowed record boundary.
    let mut references = Vec::with_capacity(leaves.len());
    for leaf in leaves {
        if matches!(leaf.value, ComponentValue::Null) {
            admit(0, 0, component.as_str().len())
                .map_err(AssetReferenceCollectionError::Admission)?;
            return Err(AssetReferenceCollectionError::Reference(
                DeclaredAssetReferenceError::RequiredFieldIsNull {
                    component: component.clone(),
                    field: leaf.field.id().clone(),
                },
            ));
        }
        let asset_ref = match decode_asset_reference(leaf.value) {
            Ok(reference) => reference,
            Err(error) => {
                admit(0, 0, component.as_str().len())
                    .map_err(AssetReferenceCollectionError::Admission)?;
                let component = component.clone();
                let field = leaf.field.id().clone();
                let path = leaf.path;
                return Err(AssetReferenceCollectionError::Reference(match error {
                    AssetReferenceValueError::Shape => {
                        DeclaredAssetReferenceError::InvalidValueShape {
                            component,
                            field,
                            path,
                        }
                    }
                    AssetReferenceValueError::Kind => DeclaredAssetReferenceError::InvalidKind {
                        component,
                        field,
                        path,
                    },
                    AssetReferenceValueError::Path(source) => {
                        DeclaredAssetReferenceError::InvalidPath {
                            component,
                            field,
                            path,
                            source,
                        }
                    }
                    AssetReferenceValueError::StableId(source) => {
                        DeclaredAssetReferenceError::InvalidStableId {
                            component,
                            field,
                            path,
                            source,
                        }
                    }
                }));
            }
        };
        references.push(DeclaredAssetReference {
            field_id: leaf.field.id().clone(),
            path: leaf.path,
            asset_ref,
        });
    }
    Ok(references)
}

/// Collects schema-declared effective asset values using the convenience interpretation limits.
pub fn collect_declared_asset_references(
    registry: &ComponentRegistry,
    component: &ComponentTypeId,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    collect_declared_asset_references_with_limits(
        registry,
        component,
        version,
        value,
        ComponentValueMaterializationLimits::default(),
        EntityReferenceTraversalLimits::default(),
    )
}

/// Collects effective asset values with explicit expansion and reference-work budgets.
pub fn collect_declared_asset_references_with_limits(
    registry: &ComponentRegistry,
    component: &ComponentTypeId,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
    materialization_limits: ComponentValueMaterializationLimits,
    traversal_limits: EntityReferenceTraversalLimits,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    if !registry.is_frozen() {
        return Err(DeclaredAssetReferenceError::RegistryNotFrozen);
    }
    let schema = registry.schema(component).ok_or_else(|| {
        DeclaredAssetReferenceError::UnknownComponent {
            component: component.clone(),
        }
    })?;
    if version != schema.version() {
        return Err(DeclaredAssetReferenceError::SchemaVersionMismatch {
            component: component.clone(),
            expected: schema.version(),
            actual: version,
        });
    }

    collect_declared_asset_references_from_schema_with_limits(
        schema,
        version,
        value,
        materialization_limits,
        traversal_limits,
    )
}

/// Collects schema-declared asset references without requiring executable registry authority.
///
/// The caller must already own an immutable, authenticated schema catalog. This function only
/// interprets the supplied data schema and generic component value; it does not grant native codec,
/// migration, provider, or Runtime authority.
pub fn collect_declared_asset_references_from_schema(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    collect_declared_asset_references_from_schema_with_limits(
        schema,
        version,
        value,
        ComponentValueMaterializationLimits::default(),
        EntityReferenceTraversalLimits::default(),
    )
}

/// Interprets an authenticated current schema using separate expansion and reference-work limits.
///
/// This fills defaults but does not validate unrelated required fields or grant native authority.
/// Returned references own their values; the caller's authored tree remains unchanged.
pub fn collect_declared_asset_references_from_schema_with_limits(
    schema: &ComponentSchema,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
    materialization_limits: ComponentValueMaterializationLimits,
    traversal_limits: EntityReferenceTraversalLimits,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    let component = schema.id();
    if version != schema.version() {
        return Err(DeclaredAssetReferenceError::SchemaVersionMismatch {
            component: component.clone(),
            expected: schema.version(),
            actual: version,
        });
    }

    let effective =
        effective_component_value(schema, value, materialization_limits).map_err(|source| {
            DeclaredAssetReferenceError::Materialization {
                component: component.clone(),
                source,
            }
        })?;
    let leaves = collect_present_reference_leaves::<()>(
        schema,
        effective.as_ref(),
        traversal_limits,
        ComponentValueKind::AssetRef,
    )
    .map_err(|source| DeclaredAssetReferenceError::Traversal {
        component: component.clone(),
        source,
    })?;
    materialize_asset_references(component, effective.as_ref(), leaves, 0)
}

/// Collects references from an already selected field value, retaining component-relative paths.
///
/// The caller supplies the actual field subtree, as a SetField operation does. Missing defaults
/// and reference target existence remain the owning transaction's responsibility.
pub fn collect_declared_field_asset_references(
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
    value: &ComponentValue,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    collect_declared_field_asset_references_with_limits(
        component,
        field,
        value,
        EntityReferenceTraversalLimits::default(),
    )
}

/// Collects references from a selected field subtree with explicit reference-work limits.
///
/// Paths remain component-relative. This does not fill defaults or grant native authority;
/// reference target existence remains the owning transaction's responsibility.
pub fn collect_declared_field_asset_references_with_limits(
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
    value: &ComponentValue,
    traversal_limits: EntityReferenceTraversalLimits,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    let leaves = collect_present_field_reference_leaves::<()>(
        field,
        value,
        traversal_limits,
        ComponentValueKind::AssetRef,
    )
    .map_err(|source| DeclaredAssetReferenceError::Traversal {
        component: component.clone(),
        source,
    })?;
    materialize_asset_references(component, value, leaves, field.path().segments().len())
}

/// Collects declared assets relative to a selected replacement payload, with explicit limits.
///
/// The caller supplies an admitted owning field and its selected shape, for example from
/// [`crate::resolve_component_property_schema_claim`]. Identity, metadata and capabilities remain
/// those of the original field. Returned paths start at the payload; a scalar asset occupies the
/// empty path. Input and returned reference work share the traversal limits.
///
/// This does not resolve a target, prove that a selected value exists, infer dependencies from
/// undeclared wire values, fill defaults or construct a complete component value. Shape affinity,
/// binding, edit and native admission remain the caller's responsibility.
pub fn collect_declared_value_asset_references_with_limits(
    component: &ComponentTypeId,
    field: &ComponentFieldSchema,
    shape: &ComponentValueShape,
    value: &ComponentValue,
    traversal_limits: EntityReferenceTraversalLimits,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    let leaves = collect_present_value_reference_leaves::<()>(
        field,
        shape,
        value,
        traversal_limits,
        ComponentValueKind::AssetRef,
    )
    .map_err(|source| DeclaredAssetReferenceError::Traversal {
        component: component.clone(),
        source,
    })?;
    materialize_asset_references(component, value, leaves, 0)
}

fn materialize_asset_references(
    component: &ComponentTypeId,
    value: &ComponentValue,
    leaves: Vec<PresentReferenceLeaf<'_, '_>>,
    root_segments: usize,
) -> Result<Vec<DeclaredAssetReference>, DeclaredAssetReferenceError> {
    let mut references = Vec::with_capacity(leaves.len());
    for leaf in leaves {
        let relative =
            ComponentFieldPath::new(leaf.path.segments()[root_segments..].iter().cloned());
        let field_value =
            value
                .get_path(&relative)
                .map_err(|source| DeclaredAssetReferenceError::FieldPath {
                    component: component.clone(),
                    field: leaf.field.id().clone(),
                    source,
                })?;
        if matches!(field_value, ComponentValue::Null) {
            return Err(DeclaredAssetReferenceError::RequiredFieldIsNull {
                component: component.clone(),
                field: leaf.field.id().clone(),
            });
        }
        let asset_ref = parse_asset_reference(component, leaf.field.id(), &leaf.path, field_value)?;
        references.push(DeclaredAssetReference {
            field_id: leaf.field.id().clone(),
            path: leaf.path,
            asset_ref,
        });
    }
    Ok(references)
}

fn parse_asset_reference(
    component: &ComponentTypeId,
    field: &ComponentFieldId,
    path: &ComponentFieldPath,
    value: &ComponentValue,
) -> Result<AssetRef, DeclaredAssetReferenceError> {
    decode_asset_reference(value).map_err(|error| match error {
        AssetReferenceValueError::Shape => DeclaredAssetReferenceError::InvalidValueShape {
            component: component.clone(),
            field: field.clone(),
            path: path.clone(),
        },
        AssetReferenceValueError::Kind => DeclaredAssetReferenceError::InvalidKind {
            component: component.clone(),
            field: field.clone(),
            path: path.clone(),
        },
        AssetReferenceValueError::Path(source) => DeclaredAssetReferenceError::InvalidPath {
            component: component.clone(),
            field: field.clone(),
            path: path.clone(),
            source,
        },
        AssetReferenceValueError::StableId(source) => {
            DeclaredAssetReferenceError::InvalidStableId {
                component: component.clone(),
                field: field.clone(),
                path: path.clone(),
                source,
            }
        }
    })
}

#[derive(Debug)]
enum AssetReferenceValueError {
    Shape,
    Kind,
    Path(AssetPathError),
    StableId(StableAssetIdError),
}

fn decode_asset_reference(value: &ComponentValue) -> Result<AssetRef, AssetReferenceValueError> {
    let (kind, payload) = asset_reference_parts(value).ok_or(AssetReferenceValueError::Shape)?;
    match kind {
        "path" => AssetRef::path(payload).map_err(AssetReferenceValueError::Path),
        "stable_id" => AssetRef::stable_id(payload).map_err(AssetReferenceValueError::StableId),
        _ => Err(AssetReferenceValueError::Kind),
    }
}

pub(crate) fn decode_asset_reference_value(
    value: &ComponentValue,
) -> Result<AssetRef, ComponentCodecError> {
    decode_asset_reference(value).map_err(|_| {
        ComponentCodecError::invalid_field("<asset-ref>", "a canonical admitted asset reference")
    })
}

pub(crate) fn is_asset_reference_value(value: &ComponentValue) -> bool {
    decode_asset_reference(value).is_ok()
}

fn asset_reference_parts(value: &ComponentValue) -> Option<(&str, &str)> {
    let ComponentValue::Map(fields) = value else {
        return None;
    };
    if fields.len() != 2 {
        return None;
    }
    let (Some(ComponentValue::String(kind)), Some(ComponentValue::String(value))) =
        (fields.get("kind"), fields.get("value"))
    else {
        return None;
    };
    Some((kind.as_str(), value.as_str()))
}
