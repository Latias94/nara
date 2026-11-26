mod decode;
mod encode;
mod layout;

use std::collections::BTreeMap;

use nara_core::AssetId;
use nara_reflect::{
    ComponentCodecError, ComponentSchema, ComponentSchemaVersion, ComponentShapeError,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    validate_component_value,
};
use ron::value::RawValue;
use thiserror::Error;

use super::{RawRon, SceneAsset, SceneDocumentError, builtin_component_schemas};
use layout::{Layout, Node};

type Assets = BTreeMap<String, SceneAsset>;

pub(super) struct Schemas<'a> {
    builtin: [ComponentSchema; 4],
    project: BTreeMap<&'a ComponentTypeId, &'a ComponentSchema>,
}

impl<'a> Schemas<'a> {
    pub(super) fn new(schemas: &'a [ComponentSchema]) -> Result<Self, SceneDocumentError> {
        let builtin = builtin_component_schemas().map_err(SceneDocumentError::BuiltinSchemas)?;
        let mut project = BTreeMap::new();
        for schema in schemas {
            if builtin.iter().any(|builtin| builtin.id() == schema.id()) {
                continue;
            }
            if project.insert(schema.id(), schema).is_some() {
                return Err(SceneDocumentError::DuplicateSchema {
                    component: schema.id().clone(),
                });
            }
        }
        Ok(Self { builtin, project })
    }

    pub(super) fn matching(
        &self,
        id: &ComponentTypeId,
        version: ComponentSchemaVersion,
    ) -> Option<&ComponentSchema> {
        self.builtin
            .iter()
            .find(|schema| schema.id() == id)
            .or_else(|| self.project.get(id).copied())
            .filter(|schema| schema.version() == version)
    }
}

pub(super) fn decode_component(
    raw: &RawRon,
    schema: &ComponentSchema,
    assets: &Assets,
) -> Result<ComponentValue, CompactValueError> {
    let layout = Layout::new(schema)?;
    let value = decode::decode(raw, Node::Layout(&layout), assets)?;
    Ok(schema.with_defaults(value)?)
}

pub(super) fn encode_component(
    value: &ComponentValue,
    schema: &ComponentSchema,
    assets: &Assets,
) -> Result<RawRon, CompactValueError> {
    validate_component_value(schema, value)?;
    let layout = Layout::new(schema)?;
    encode::encode(value, Node::Layout(&layout), assets)
}

pub(crate) fn decode_value(
    raw: &RawRon,
    shape: &ComponentValueShape,
    assets: &Assets,
) -> Result<ComponentValue, CompactValueError> {
    let value = decode::decode(raw, Node::Shape(shape), assets)?;
    shape.validate_value(&value)?;
    Ok(value)
}

pub(crate) fn encode_value(
    value: &ComponentValue,
    shape: &ComponentValueShape,
    assets: &Assets,
) -> Result<RawRon, CompactValueError> {
    shape.validate_value(value)?;
    encode::encode(value, Node::Shape(shape), assets)
}

pub(super) fn decode_asset(reference: &str, assets: &Assets) -> Result<AssetId, CompactValueError> {
    if let Some(alias) = reference.strip_prefix('@') {
        return assets
            .get(alias)
            .map(|asset| asset.id)
            .ok_or_else(|| CompactValueError::UnknownAssetAlias(alias.to_owned()));
    }
    reference
        .parse()
        .map_err(|_| CompactValueError::InvalidAssetId(reference.to_owned()))
}

pub(super) fn asset_text(id: AssetId, assets: &Assets) -> String {
    assets
        .iter()
        .find(|(_, asset)| asset.id == id)
        .map_or_else(|| id.to_string(), |(alias, _)| format!("@{alias}"))
}

fn make_raw(source: String) -> Result<RawRon, CompactValueError> {
    Ok(RawRon(RawValue::from_boxed_ron(source.into_boxed_str())?))
}

/// A compact value could not be represented by its declared schema or asset aliases.
#[derive(Debug, Error)]
pub enum CompactValueError {
    #[error("{0}")]
    Parse(#[from] ron::error::SpannedError),
    #[error("{0}")]
    Serialize(#[from] ron::Error),
    #[error(transparent)]
    Codec(#[from] ComponentCodecError),
    #[error(transparent)]
    Shape(#[from] ComponentShapeError),
    #[error("conflicting scene field declaration at '{0}'")]
    Layout(String),
    #[error("unknown asset alias '@{0}'")]
    UnknownAssetAlias(String),
    #[error("expected a non-nil asset UUID or @alias, got {0:?}")]
    InvalidAssetId(String),
    #[error("no compact wire representation for opaque {0:?}; declare the element shape")]
    OpaqueShape(ComponentValueKind),
    #[error("schema cannot encode value of kind {0:?} in compact RON")]
    InvalidValue(ComponentValueKind),
}
