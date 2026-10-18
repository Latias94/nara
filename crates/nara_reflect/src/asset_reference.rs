use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_asset::{AssetPathError, AssetRef, StableAssetIdError};

use crate::{
    ComponentCodecError, ComponentEntityReferenceRewriteError, ComponentFieldId,
    ComponentFieldPath, ComponentFieldPathError, ComponentFieldSchema, ComponentRegistry,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueMaterializationLimits, EntityReferenceTraversalLimits, effective_component_value,
    entity_reference::{
        PresentReferenceLeaf, collect_present_field_reference_leaves,
        collect_present_reference_leaves,
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
    let leaves = collect_present_field_reference_leaves::<()>(
        field,
        value,
        EntityReferenceTraversalLimits::default(),
        ComponentValueKind::AssetRef,
    )
    .map_err(|source| DeclaredAssetReferenceError::Traversal {
        component: component.clone(),
        source,
    })?;
    materialize_asset_references(component, value, leaves, field.path().segments().len())
}

fn materialize_asset_references(
    component: &ComponentTypeId,
    value: &ComponentValue,
    leaves: Vec<PresentReferenceLeaf<'_>>,
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
