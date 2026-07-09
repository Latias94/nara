//! Stable component schema identifiers and authoring field metadata.

use crate::{ComponentFieldPath, ComponentValue};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct ComponentTypeId(String);

impl ComponentTypeId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct ComponentSchemaVersion(pub u32);

impl Default for ComponentSchemaVersion {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentSchema {
    pub id: ComponentTypeId,
    pub version: ComponentSchemaVersion,
    pub rust_type_path: String,
    pub serializable: bool,
    pub fields: Vec<ComponentFieldSchema>,
}

#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentSchemaCatalog {
    pub components: Vec<ComponentSchema>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentFieldSchema {
    pub path: ComponentFieldPath,
    pub value_kind: ComponentValueKind,
    pub required: bool,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub default_value: Option<ComponentValue>,
}

impl ComponentFieldSchema {
    #[must_use]
    pub fn required(path: ComponentFieldPath, value_kind: ComponentValueKind) -> Self {
        Self {
            path,
            value_kind,
            required: true,
            default_value: None,
        }
    }

    #[must_use]
    pub fn optional(path: ComponentFieldPath, value_kind: ComponentValueKind) -> Self {
        Self {
            path,
            value_kind,
            required: false,
            default_value: None,
        }
    }

    #[must_use]
    pub fn optional_with_default(
        path: ComponentFieldPath,
        value_kind: ComponentValueKind,
        default_value: ComponentValue,
    ) -> Self {
        Self {
            path,
            value_kind,
            required: false,
            default_value: Some(default_value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ComponentValueKind {
    Null,
    Bool,
    I64,
    U64,
    F64,
    String,
    List,
    Map,
    AssetRef,
}
