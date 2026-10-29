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

impl fmt::Display for AssetProductRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.source, self.product)
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
    use serde::{Deserialize, Serialize, de, ser::SerializeStruct};

    impl Serialize for ImportedProductId {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut record = serializer
                .serialize_struct("ImportedProductId", if self.0.is_some() { 2 } else { 1 })?;
            match self.0 {
                None => record.serialize_field("kind", "primary")?,
                Some(id) => {
                    record.serialize_field("kind", "subproduct")?;
                    record.serialize_field("id", &id.to_string())?;
                }
            }
            record.end()
        }
    }

    impl<'de> Deserialize<'de> for ImportedProductId {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(field_identifier, rename_all = "snake_case")]
            enum Field {
                Kind,
                Id,
            }

            struct ProductVisitor;

            impl<'de> de::Visitor<'de> for ProductVisitor {
                type Value = ImportedProductId;

                fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    formatter.write_str("an imported product identity record")
                }

                fn visit_map<M: de::MapAccess<'de>>(
                    self,
                    mut map: M,
                ) -> Result<Self::Value, M::Error> {
                    let mut kind = None;
                    let mut id = None;
                    while let Some(field) = map.next_key()? {
                        match field {
                            Field::Kind => {
                                if kind.is_some() {
                                    return Err(de::Error::duplicate_field("kind"));
                                }
                                kind = Some(map.next_value::<String>()?);
                            }
                            Field::Id => {
                                if id.is_some() {
                                    return Err(de::Error::duplicate_field("id"));
                                }
                                id = Some(map.next_value::<String>()?);
                            }
                        }
                    }
                    let kind = kind.ok_or_else(|| de::Error::missing_field("kind"))?;
                    match kind.as_str() {
                        "primary" if id.is_none() => Ok(ImportedProductId::PRIMARY),
                        "primary" => Err(de::Error::unknown_field("id", &["kind"])),
                        "subproduct" => {
                            let id = id.ok_or_else(|| de::Error::missing_field("id"))?;
                            ImportedProductId::parse_subproduct(&id).map_err(de::Error::custom)
                        }
                        _ => Err(de::Error::unknown_variant(
                            &kind,
                            &["primary", "subproduct"],
                        )),
                    }
                }
            }

            // A record uses MapAccess in JSON and RON, but never accepts positional sequences.
            deserializer.deserialize_struct("ImportedProductId", &["kind", "id"], ProductVisitor)
        }
    }

    fn canonical_source(value: &str) -> Result<StableAssetId, &'static str> {
        const EXPECTED: &str = "source identity requires canonical UUID text";
        if value.len() != 36 {
            return Err(EXPECTED);
        }
        let id = StableAssetId::parse_str(value).map_err(|_| EXPECTED)?;
        if id.to_string() != value {
            return Err(EXPECTED);
        }
        Ok(id)
    }

    impl Serialize for AssetProductRef {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut record = serializer.serialize_struct("AssetProductRef", 2)?;
            record.serialize_field("source", &self.source)?;
            record.serialize_field("product", &self.product)?;
            record.end()
        }
    }

    impl<'de> Deserialize<'de> for AssetProductRef {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(field_identifier, rename_all = "snake_case")]
            enum Field {
                Source,
                Product,
            }

            struct ReferenceVisitor;

            impl<'de> de::Visitor<'de> for ReferenceVisitor {
                type Value = AssetProductRef;

                fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    formatter.write_str("a source-scoped asset product reference record")
                }

                fn visit_map<M: de::MapAccess<'de>>(
                    self,
                    mut map: M,
                ) -> Result<Self::Value, M::Error> {
                    let mut source = None;
                    let mut product = None;
                    while let Some(field) = map.next_key()? {
                        match field {
                            Field::Source => {
                                if source.is_some() {
                                    return Err(de::Error::duplicate_field("source"));
                                }
                                let value = map.next_value::<String>()?;
                                source = Some(canonical_source(&value).map_err(de::Error::custom)?);
                            }
                            Field::Product => {
                                if product.is_some() {
                                    return Err(de::Error::duplicate_field("product"));
                                }
                                product = Some(map.next_value::<ImportedProductId>()?);
                            }
                        }
                    }
                    let source = source.ok_or_else(|| de::Error::missing_field("source"))?;
                    let product = product.ok_or_else(|| de::Error::missing_field("product"))?;
                    AssetProductRef::new(source, product).map_err(de::Error::custom)
                }
            }

            deserializer.deserialize_struct(
                "AssetProductRef",
                &["source", "product"],
                ReferenceVisitor,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "serde")]
    const SOURCE: &str = "919617ea-c89c-4f83-9cce-b084de435a83";
    #[cfg(feature = "serde")]
    const PRODUCT: &str = "011a8cc0-343a-4b95-8adb-3aa6d81977ed";

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
    fn product_id_records_round_trip_json_and_ron() {
        for (product, json, ron) in [
            (
                ImportedProductId::PRIMARY,
                r#"{"kind":"primary"}"#.to_owned(),
                r#"(kind:"primary")"#.to_owned(),
            ),
            (
                ImportedProductId::parse_subproduct(PRODUCT).unwrap(),
                format!(r#"{{"kind":"subproduct","id":"{PRODUCT}"}}"#),
                format!(r#"(kind:"subproduct",id:"{PRODUCT}")"#),
            ),
        ] {
            assert_eq!(serde_json::to_string(&product).unwrap(), json);
            assert_eq!(
                serde_json::from_str::<ImportedProductId>(&json).unwrap(),
                product
            );
            assert_eq!(ron::to_string(&product).unwrap(), ron);
            assert_eq!(ron::from_str::<ImportedProductId>(&ron).unwrap(), product);
            let named = ron::ser::to_string_pretty(
                &product,
                ron::ser::PrettyConfig::default().struct_names(true),
            )
            .unwrap();
            assert_eq!(ron::from_str::<ImportedProductId>(&named).unwrap(), product);
        }
        let auxiliary = ImportedProductId::parse_subproduct(PRODUCT).unwrap();
        assert_eq!(
            serde_json::from_str::<ImportedProductId>(&format!(
                r#"{{"id":"{PRODUCT}","kind":"subproduct"}}"#
            ),)
            .unwrap(),
            auxiliary,
        );
        assert_eq!(
            ron::from_str::<ImportedProductId>(&format!(r#"(id:"{PRODUCT}",kind:"subproduct")"#),)
                .unwrap(),
            auxiliary,
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn product_id_records_reject_sequences_unknown_and_duplicate_fields() {
        for json in [
            r#"["primary"]"#.to_owned(),
            r#"{"kind":"primary","unknown":true}"#.to_owned(),
            r#"{"kind":"primary","id":null}"#.to_owned(),
            format!(r#"{{"kind":"primary","id":"{PRODUCT}"}}"#),
            r#"{"kind":"primary","kind":"primary"}"#.to_owned(),
            format!(r#"{{"kind":"subproduct","id":"{PRODUCT}","id":"{PRODUCT}"}}"#),
            r#"{"kind":"subproduct"}"#.to_owned(),
            r#"{"kind":"unknown"}"#.to_owned(),
        ] {
            assert!(
                serde_json::from_str::<ImportedProductId>(&json).is_err(),
                "{json}"
            );
        }
        for ron in [
            r#"["primary"]"#.to_owned(),
            r#"("primary")"#.to_owned(),
            r#"(kind:"primary",unknown:true)"#.to_owned(),
            r#"(kind:"primary",id:None)"#.to_owned(),
            format!(r#"(kind:"primary",id:"{PRODUCT}")"#),
            r#"(kind:"primary",kind:"primary")"#.to_owned(),
            format!(r#"(kind:"subproduct",id:"{PRODUCT}",id:"{PRODUCT}")"#),
            r#"(kind:"subproduct")"#.to_owned(),
            r#"(kind:"unknown")"#.to_owned(),
        ] {
            assert!(ron::from_str::<ImportedProductId>(&ron).is_err(), "{ron}");
        }
        for invalid in [
            "00000000-0000-0000-0000-000000000000",
            "011A8CC0-343A-4B95-8ADB-3AA6D81977ED",
            "011a8cc0343a4b958adb3aa6d81977ed",
        ] {
            let json = format!(r#"{{"kind":"subproduct","id":"{invalid}"}}"#);
            let ron = format!(r#"(kind:"subproduct",id:"{invalid}")"#);
            assert!(
                serde_json::from_str::<ImportedProductId>(&json).is_err(),
                "{json}"
            );
            assert!(ron::from_str::<ImportedProductId>(&ron).is_err(), "{ron}");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn product_reference_records_round_trip_json_and_ron() {
        let source = StableAssetId::parse_str(SOURCE).unwrap();
        for (product, json_product, ron_product) in [
            (
                ImportedProductId::PRIMARY,
                r#"{"kind":"primary"}"#.to_owned(),
                r#"(kind:"primary")"#.to_owned(),
            ),
            (
                ImportedProductId::parse_subproduct(PRODUCT).unwrap(),
                format!(r#"{{"kind":"subproduct","id":"{PRODUCT}"}}"#),
                format!(r#"(kind:"subproduct",id:"{PRODUCT}")"#),
            ),
        ] {
            let reference = AssetProductRef::new(source, product).unwrap();
            let json = format!(r#"{{"source":"{SOURCE}","product":{json_product}}}"#);
            let ron = format!(r#"(source:"{SOURCE}",product:{ron_product})"#);
            assert_eq!(serde_json::to_string(&reference).unwrap(), json);
            assert_eq!(
                serde_json::from_str::<AssetProductRef>(&json).unwrap(),
                reference
            );
            assert_eq!(ron::to_string(&reference).unwrap(), ron);
            assert_eq!(ron::from_str::<AssetProductRef>(&ron).unwrap(), reference);
            let reordered_json = format!(r#"{{"product":{json_product},"source":"{SOURCE}"}}"#);
            let reordered_ron = format!(r#"(product:{ron_product},source:"{SOURCE}")"#);
            assert_eq!(
                serde_json::from_str::<AssetProductRef>(&reordered_json).unwrap(),
                reference,
            );
            assert_eq!(
                ron::from_str::<AssetProductRef>(&reordered_ron).unwrap(),
                reference
            );
            let named = ron::ser::to_string_pretty(
                &reference,
                ron::ser::PrettyConfig::default().struct_names(true),
            )
            .unwrap();
            assert_eq!(ron::from_str::<AssetProductRef>(&named).unwrap(), reference);
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn product_reference_records_reject_sequences_unknown_and_duplicate_fields() {
        for json in [
            format!(r#"["{SOURCE}",{{"kind":"primary"}}]"#),
            format!(r#"{{"source":"{SOURCE}","product":["primary"]}}"#),
            format!(r#"{{"source":"{SOURCE}","product":{{"kind":"primary"}},"unknown":true}}"#),
            format!(
                r#"{{"source":"{SOURCE}","source":"{SOURCE}","product":{{"kind":"primary"}}}}"#
            ),
            format!(
                r#"{{"source":"{SOURCE}","product":{{"kind":"primary"}},"product":{{"kind":"primary"}}}}"#
            ),
            format!(r#"{{"source":"{SOURCE}"}}"#),
            r#"{"product":{"kind":"primary"}}"#.to_owned(),
        ] {
            assert!(
                serde_json::from_str::<AssetProductRef>(&json).is_err(),
                "{json}"
            );
        }
        for ron in [
            format!(r#"["{SOURCE}",(kind:"primary")]"#),
            format!(r#"("{SOURCE}",(kind:"primary"))"#),
            format!(r#"(source:"{SOURCE}",product:["primary"])"#),
            format!(r#"(source:"{SOURCE}",product:(kind:"primary"),unknown:true)"#),
            format!(r#"(source:"{SOURCE}",source:"{SOURCE}",product:(kind:"primary"))"#),
            format!(r#"(source:"{SOURCE}",product:(kind:"primary"),product:(kind:"primary"))"#),
            format!(r#"(source:"{SOURCE}")"#),
            r#"(product:(kind:"primary"))"#.to_owned(),
        ] {
            assert!(ron::from_str::<AssetProductRef>(&ron).is_err(), "{ron}");
        }
        for invalid in [
            "00000000-0000-0000-0000-000000000000",
            "919617EA-C89C-4F83-9CCE-B084DE435A83",
            "919617eac89c4f839cceb084de435a83",
        ] {
            let json = format!(r#"{{"source":"{invalid}","product":{{"kind":"primary"}}}}"#);
            let ron = format!(r#"(source:"{invalid}",product:(kind:"primary"))"#);
            assert!(
                serde_json::from_str::<AssetProductRef>(&json).is_err(),
                "{json}"
            );
            assert!(ron::from_str::<AssetProductRef>(&ron).is_err(), "{ron}");
        }
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
