use nara_asset::{AssetProductRef, AssetRef, ImportedProductId, StableAssetId};
use nara_core::AssetId;

use crate::{ComponentCodecError, ComponentValue, ComponentValueRef};

/// Decodes an asset reference used in component field values.
pub fn decode_asset_reference_value<'a>(
    value: impl Into<ComponentValueRef<'a>>,
) -> Result<AssetRef, ComponentCodecError> {
    let invalid =
        || ComponentCodecError::invalid_field("<asset-ref>", "a canonical asset reference");
    let value = value.into();
    if let Some(stable) = value.as_asset_reference() {
        return Ok(AssetRef::StableId((*stable).into()));
    }
    let fields = value
        .as_map()
        .filter(|map| map.len() == 2)
        .ok_or_else(invalid)?;
    let kind = fields
        .get("kind")
        .and_then(ComponentValueRef::as_str)
        .ok_or_else(invalid)?;
    let value = fields.get("value").ok_or_else(invalid)?;
    match kind {
        "path" => AssetRef::path(value.as_str().ok_or_else(invalid)?).map_err(|_| invalid()),
        "product" => decode_product(value).map_err(|_| invalid()),
        _ => Err(invalid()),
    }
}

fn decode_product(value: ComponentValueRef<'_>) -> Result<AssetRef, ()> {
    let value = value.as_map().filter(|map| map.len() == 2).ok_or(())?;
    let source = value
        .get("source")
        .and_then(ComponentValueRef::as_str)
        .ok_or(())?;
    let product = value
        .get("product")
        .and_then(ComponentValueRef::as_map)
        .ok_or(())?;
    let source_id = StableAssetId::parse_str(source).map_err(|_| ())?;
    if source_id.to_string() != source {
        return Err(());
    }
    let product_id = match product.get("kind").and_then(ComponentValueRef::as_str) {
        Some("primary") if product.len() == 1 => ImportedProductId::PRIMARY,
        Some("subproduct") if product.len() == 2 => {
            let id = product
                .get("id")
                .and_then(ComponentValueRef::as_str)
                .ok_or(())?;
            ImportedProductId::parse_subproduct(id).map_err(|_| ())?
        }
        _ => return Err(()),
    };
    AssetProductRef::new(source_id, product_id)
        .map(AssetRef::product)
        .map_err(|_| ())
}

/// Encodes a reference without resolving its target. Exact stable IDs use the same value as
/// `AssetId` fields. Path and product selections remain available to native locator callers.
pub fn encode_asset_reference_value(
    reference: &AssetRef,
) -> Result<ComponentValue, ComponentCodecError> {
    let (kind, value) = match reference {
        AssetRef::Path(path) => ("path", ComponentValue::String(path.as_str().to_owned())),
        AssetRef::StableId(id) => {
            return AssetId::try_from(*id)
                .map(ComponentValue::AssetReference)
                .map_err(|_| {
                    ComponentCodecError::invalid_field("<asset-ref>", "non-nil asset UUID")
                });
        }
        AssetRef::Product(reference) => {
            let product = match reference.product().as_subproduct() {
                None => {
                    ComponentValue::map([("kind", ComponentValue::String("primary".to_owned()))])
                }
                Some(id) => ComponentValue::map([
                    ("kind", ComponentValue::String("subproduct".to_owned())),
                    ("id", ComponentValue::String(id.to_string())),
                ]),
            };
            (
                "product",
                ComponentValue::map([
                    (
                        "source",
                        ComponentValue::String(reference.source().to_string()),
                    ),
                    ("product", product),
                ]),
            )
        }
    };
    Ok(ComponentValue::map([
        ("kind", ComponentValue::String(kind.to_owned())),
        ("value", value),
    ]))
}
