use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_asset::{AssetPathError, AssetRef, StableAssetIdError};

use crate::{
    ComponentCapability, ComponentFieldId, ComponentFieldPathError, ComponentRegistry,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredAssetReference {
    field_id: ComponentFieldId,
    asset_ref: AssetRef,
}

impl DeclaredAssetReference {
    #[must_use]
    pub const fn field_id(&self) -> &ComponentFieldId {
        &self.field_id
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
    },
    InvalidKind {
        component: ComponentTypeId,
        field: ComponentFieldId,
    },
    InvalidPath {
        component: ComponentTypeId,
        field: ComponentFieldId,
        source: AssetPathError,
    },
    InvalidStableId {
        component: ComponentTypeId,
        field: ComponentFieldId,
        source: StableAssetIdError,
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
        })
    }
}

impl Error for DeclaredAssetReferenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::FieldPath { source, .. } => Some(source),
            Self::InvalidPath { source, .. } => Some(source),
            Self::InvalidStableId { source, .. } => Some(source),
            Self::RegistryNotFrozen
            | Self::UnknownComponent { .. }
            | Self::SchemaVersionMismatch { .. }
            | Self::RequiredFieldIsNull { .. }
            | Self::InvalidValueShape { .. }
            | Self::InvalidKind { .. } => None,
        }
    }
}

/// Collects only values whose current schema fields explicitly opt into asset-reference semantics.
pub fn collect_declared_asset_references(
    registry: &ComponentRegistry,
    component: &ComponentTypeId,
    version: ComponentSchemaVersion,
    value: &ComponentValue,
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

    let mut references = Vec::new();
    for field in schema.fields().iter().filter(|field| {
        field.value_kind() == ComponentValueKind::AssetRef
            && field.has_capability(ComponentCapability::AssetRef)
    }) {
        let field_value = match value.get_path(field.path()) {
            Ok(value) => value,
            Err(ComponentFieldPathError::MissingField { .. }) if !field.is_required() => continue,
            Err(source) => {
                return Err(DeclaredAssetReferenceError::FieldPath {
                    component: component.clone(),
                    field: field.id().clone(),
                    source,
                });
            }
        };
        if matches!(field_value, ComponentValue::Null) {
            if field.is_required() {
                return Err(DeclaredAssetReferenceError::RequiredFieldIsNull {
                    component: component.clone(),
                    field: field.id().clone(),
                });
            }
            continue;
        }
        let asset_ref = parse_asset_reference(component, field.id(), field_value)?;
        references.push(DeclaredAssetReference {
            field_id: field.id().clone(),
            asset_ref,
        });
    }
    Ok(references)
}

fn parse_asset_reference(
    component: &ComponentTypeId,
    field: &ComponentFieldId,
    value: &ComponentValue,
) -> Result<AssetRef, DeclaredAssetReferenceError> {
    let Some((kind, value)) = asset_reference_parts(value) else {
        return Err(DeclaredAssetReferenceError::InvalidValueShape {
            component: component.clone(),
            field: field.clone(),
        });
    };
    match kind {
        "path" => {
            AssetRef::path(value).map_err(|source| DeclaredAssetReferenceError::InvalidPath {
                component: component.clone(),
                field: field.clone(),
                source,
            })
        }
        "stable_id" => AssetRef::stable_id(value).map_err(|source| {
            DeclaredAssetReferenceError::InvalidStableId {
                component: component.clone(),
                field: field.clone(),
                source,
            }
        }),
        _ => Err(DeclaredAssetReferenceError::InvalidKind {
            component: component.clone(),
            field: field.clone(),
        }),
    }
}

pub(crate) fn is_asset_reference_value(value: &ComponentValue) -> bool {
    matches!(
        asset_reference_parts(value),
        Some(("path" | "stable_id", _))
    )
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
