//! Logical imported product identity, separate from paths, artifacts and runtime handles.

use std::{error::Error, fmt};

use uuid::Uuid;

use crate::StableAssetId;

/// A logical product scoped to one source asset, independent of product kind and display name.
///
/// The primary product is reserved. Auxiliary products use non-nil opaque IDs whose continuity
/// belongs to intrinsic source identity or authored metadata, never generated cache state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImportedProductId(Option<Uuid>);

impl ImportedProductId {
    pub const PRIMARY: Self = Self(None);

    pub fn subproduct(id: Uuid) -> Result<Self, AssetProductIdentityError> {
        if id.is_nil() {
            return Err(AssetProductIdentityError::NilProduct);
        }
        Ok(Self(Some(id)))
    }

    pub fn parse_subproduct(value: &str) -> Result<Self, AssetProductIdentityError> {
        if value.len() != 36 {
            return Err(AssetProductIdentityError::InvalidProductText);
        }
        let id =
            Uuid::parse_str(value).map_err(|_| AssetProductIdentityError::InvalidProductText)?;
        if id.hyphenated().to_string() != value {
            return Err(AssetProductIdentityError::InvalidProductText);
        }
        Self::subproduct(id)
    }

    #[must_use]
    pub const fn as_subproduct(self) -> Option<Uuid> {
        self.0
    }
}

impl fmt::Display for ImportedProductId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            None => formatter.write_str("primary"),
            Some(id) => fmt::Display::fmt(&id, formatter),
        }
    }
}

/// A complete logical product reference in one composed source-identity universe.
///
/// Paths, package origin, encoded content and mount/residency generations do not participate in
/// equality. This reference neither loads a product nor supplies publication authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetProductRef {
    source: StableAssetId,
    product: ImportedProductId,
}

impl AssetProductRef {
    pub fn new(
        source: StableAssetId,
        product: ImportedProductId,
    ) -> Result<Self, AssetProductIdentityError> {
        if source.as_uuid().is_nil() {
            return Err(AssetProductIdentityError::NilSource);
        }
        Ok(Self { source, product })
    }

    #[must_use]
    pub const fn source(self) -> StableAssetId {
        self.source
    }

    #[must_use]
    pub const fn product(self) -> ImportedProductId {
        self.product
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetProductIdentityError {
    InvalidProductText,
    NilProduct,
    NilSource,
}

impl fmt::Display for AssetProductIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidProductText => "product identity requires canonical UUID text",
            Self::NilProduct => "an auxiliary product identity must be non-nil",
            Self::NilSource => "a product reference requires a non-nil source identity",
        })
    }
}

impl Error for AssetProductIdentityError {}

#[cfg(feature = "serde")]
mod wire {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    enum ProductWire {
        Primary {},
        Subproduct { id: String },
    }

    impl Serialize for ImportedProductId {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            match self.0 {
                None => ProductWire::Primary {},
                Some(id) => ProductWire::Subproduct { id: id.to_string() },
            }
            .serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for ImportedProductId {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            match ProductWire::deserialize(deserializer)? {
                ProductWire::Primary {} => Ok(Self::PRIMARY),
                ProductWire::Subproduct { id } => {
                    Self::parse_subproduct(&id).map_err(serde::de::Error::custom)
                }
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ReferenceWire {
        #[serde(deserialize_with = "canonical_source")]
        source: StableAssetId,
        product: ImportedProductId,
    }

    fn canonical_source<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<StableAssetId, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.len() != 36 {
            return Err(serde::de::Error::custom(
                "source identity requires canonical UUID text",
            ));
        }
        let id = StableAssetId::parse_str(&value).map_err(|_| {
            serde::de::Error::custom("source identity requires canonical UUID text")
        })?;
        if id.to_string() != value {
            return Err(serde::de::Error::custom(
                "source identity requires canonical UUID text",
            ));
        }
        Ok(id)
    }

    impl Serialize for AssetProductRef {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            ReferenceWire {
                source: self.source,
                product: self.product,
            }
            .serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for AssetProductRef {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let wire = ReferenceWire::deserialize(deserializer)?;
            Self::new(wire.source, wire.product).map_err(serde::de::Error::custom)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_auxiliary_ids_remain_scoped_to_their_source() {
        let product =
            ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap();
        let first = StableAssetId::parse_str("919617ea-c89c-4f83-9cce-b084de435a83").unwrap();
        let second = StableAssetId::parse_str("7214561b-ae28-454f-888b-fd049235d212").unwrap();
        assert_ne!(
            AssetProductRef::new(first, product).unwrap(),
            AssetProductRef::new(second, product).unwrap()
        );
        assert_ne!(product, ImportedProductId::PRIMARY);
        assert_eq!(
            AssetProductRef::new(first, product).unwrap().source(),
            first
        );
        assert_eq!(
            AssetProductRef::new(first, product).unwrap().product(),
            product
        );
    }

    #[test]
    fn nil_and_noncanonical_product_ids_never_alias_primary() {
        for invalid in [
            "00000000-0000-0000-0000-000000000000",
            "011A8CC0-343A-4B95-8ADB-3AA6D81977ED",
            "011a8cc0343a4b958adb3aa6d81977ed",
            "primary",
        ] {
            assert!(ImportedProductId::parse_subproduct(invalid).is_err());
        }
        assert_eq!(
            ImportedProductId::subproduct(Uuid::nil()),
            Err(AssetProductIdentityError::NilProduct)
        );
        assert_eq!(
            AssetProductRef::new(
                StableAssetId::from_uuid(Uuid::nil()),
                ImportedProductId::PRIMARY
            ),
            Err(AssetProductIdentityError::NilSource)
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn product_reference_wire_rejects_ambiguous_or_unknown_identity_fields() {
        let source = "919617ea-c89c-4f83-9cce-b084de435a83";
        let product = "011a8cc0-343a-4b95-8adb-3aa6d81977ed";
        let reference = AssetProductRef::new(
            StableAssetId::parse_str(source).unwrap(),
            ImportedProductId::parse_subproduct(product).unwrap(),
        )
        .unwrap();
        let encoded = serde_json::to_string(&reference).unwrap();
        assert_eq!(
            serde_json::from_str::<AssetProductRef>(&encoded).unwrap(),
            reference
        );
        for invalid in [
            serde_json::json!({"source": source, "product": {"kind": "primary", "id": product}}),
            serde_json::json!({"source": source, "product": {"kind": "subproduct"}}),
            serde_json::json!({"source": source, "product": {"kind": "label", "id": product}}),
            serde_json::json!({"source": source, "product": {"kind": "primary"}, "path": "hint"}),
            serde_json::json!({"source": source.to_uppercase(), "product": {"kind": "primary"}}),
            serde_json::json!({"source": "00000000-0000-0000-0000-000000000000", "product": {"kind": "primary"}}),
        ] {
            assert!(serde_json::from_value::<AssetProductRef>(invalid).is_err());
        }
    }
}
