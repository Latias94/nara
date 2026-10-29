use std::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};

use uuid::{Builder, Uuid};

use crate::{AssetProductRef, ImportedProductId};

static NEXT_RUNTIME_ASSET_ID: AtomicU64 = AtomicU64::new(1);

/// An opaque process-local identity; persist an `AssetRef` instead.
///
/// ```compile_fail,E0599
/// let _ = nara_asset::AssetId::from_raw(1);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetId(u64);

impl AssetId {
    #[must_use]
    #[cfg(test)]
    pub(crate) const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    pub(crate) fn allocate() -> Result<Self, crate::AssetError> {
        Self::allocate_from(&NEXT_RUNTIME_ASSET_ID)
    }

    fn allocate_from(allocator: &AtomicU64) -> Result<Self, crate::AssetError> {
        loop {
            let current = allocator.load(Ordering::Relaxed);
            let next = current
                .checked_add(1)
                .ok_or(crate::AssetError::IdSpaceExhausted)?;
            if allocator
                .compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return Ok(Self(current));
            }
        }
    }
}

#[cfg(test)]
mod allocation_tests {
    use super::*;

    #[test]
    fn runtime_asset_id_exhaustion_never_wraps_or_reuses_an_id() {
        let allocator = AtomicU64::new(u64::MAX - 1);

        assert_eq!(
            AssetId::allocate_from(&allocator).unwrap().raw(),
            u64::MAX - 1
        );
        assert_eq!(
            AssetId::allocate_from(&allocator),
            Err(crate::AssetError::IdSpaceExhausted)
        );
        assert_eq!(
            AssetId::allocate_from(&allocator),
            Err(crate::AssetError::IdSpaceExhausted)
        );
        assert_eq!(allocator.load(Ordering::Relaxed), u64::MAX);
    }
}

impl Debug for AssetId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("AssetId").field(&self.0).finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StableAssetId(Uuid);

impl StableAssetId {
    pub fn try_random() -> Result<Self, StableAssetIdGenerationError> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(StableAssetIdGenerationError::new)?;
        Ok(Self(Builder::from_random_bytes(bytes).into_uuid()))
    }

    pub fn parse_str(id: impl AsRef<str>) -> Result<Self, StableAssetIdError> {
        Uuid::parse_str(id.as_ref())
            .map(Self)
            .map_err(|_| StableAssetIdError::Invalid(id.as_ref().to_string()))
    }

    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

#[derive(Debug)]
pub struct StableAssetIdGenerationError {
    source: getrandom::Error,
}

impl StableAssetIdGenerationError {
    const fn new(source: getrandom::Error) -> Self {
        Self { source }
    }
}

impl Display for StableAssetIdGenerationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("stable asset identity generation failed")
    }
}

impl Error for StableAssetIdGenerationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

impl Display for StableAssetId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

impl FromStr for StableAssetId {
    type Err = StableAssetIdError;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        Self::parse_str(id)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for StableAssetId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for StableAssetId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::parse_str(&id).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StableAssetIdError {
    Invalid(String),
}

impl Display for StableAssetIdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(id) => write!(formatter, "invalid stable asset id '{id}'"),
        }
    }
}

impl Error for StableAssetIdError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetPath(String);

impl AssetPath {
    pub fn new(path: impl Into<String>) -> Result<Self, AssetPathError> {
        let path = path.into();
        validate_asset_path(&path)?;
        Ok(Self(path))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.0.capacity()
    }

    #[must_use]
    pub fn meta_path(&self) -> String {
        format!("{}.meta", self.0)
    }

    #[must_use]
    pub(crate) fn case_fold_key(&self) -> String {
        self.0.to_lowercase()
    }
}

impl Display for AssetPath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for AssetPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for AssetPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let path = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(path).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetPathError {
    Empty,
    Absolute,
    ContainsBackslash,
    ContainsDrivePrefix,
    ContainsCurrentDirectory,
    ContainsParentDirectory,
    ContainsEmptySegment,
    ContainsNull,
}

impl Display for AssetPathError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("asset path is empty"),
            Self::Absolute => formatter.write_str("asset path must be project-relative"),
            Self::ContainsBackslash => formatter.write_str("asset path must use '/' separators"),
            Self::ContainsDrivePrefix => {
                formatter.write_str("asset path must not contain a drive prefix")
            }
            Self::ContainsCurrentDirectory => {
                formatter.write_str("asset path must not contain '.' segments")
            }
            Self::ContainsParentDirectory => {
                formatter.write_str("asset path must not contain '..' segments")
            }
            Self::ContainsEmptySegment => {
                formatter.write_str("asset path must not contain empty segments")
            }
            Self::ContainsNull => formatter.write_str("asset path must not contain NUL"),
        }
    }
}

impl Error for AssetPathError {}

/// A complete authored asset selection.
///
/// Paths and stable source IDs select the primary product. A product reference preserves both
/// its source identity and its explicit primary or auxiliary product selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AssetRef {
    Path(AssetPath),
    StableId(StableAssetId),
    Product(AssetProductRef),
}

impl AssetRef {
    pub fn path(path: impl Into<String>) -> Result<Self, AssetPathError> {
        Ok(Self::Path(AssetPath::new(path)?))
    }

    pub fn stable_id(id: impl AsRef<str>) -> Result<Self, StableAssetIdError> {
        Ok(Self::StableId(StableAssetId::parse_str(id)?))
    }

    #[must_use]
    pub const fn product(reference: AssetProductRef) -> Self {
        Self::Product(reference)
    }

    /// Returns only a path selection; explicit product references are never projected to a path.
    #[must_use]
    pub fn as_path(&self) -> Option<&AssetPath> {
        match self {
            Self::Path(path) => Some(path),
            Self::StableId(_) | Self::Product(_) => None,
        }
    }

    /// Returns only a stable-ID selection, preserving explicit product selections as `None`.
    #[must_use]
    pub fn as_stable_id(&self) -> Option<StableAssetId> {
        match self {
            Self::StableId(id) => Some(*id),
            Self::Path(_) | Self::Product(_) => None,
        }
    }

    #[must_use]
    pub const fn as_product(&self) -> Option<AssetProductRef> {
        match self {
            Self::Product(reference) => Some(*reference),
            Self::Path(_) | Self::StableId(_) => None,
        }
    }

    /// Reports whether this reference selects the source's primary product.
    #[must_use]
    pub fn is_primary(&self) -> bool {
        match self {
            Self::Path(_) | Self::StableId(_) => true,
            Self::Product(reference) => reference.product() == ImportedProductId::PRIMARY,
        }
    }
}

impl From<AssetProductRef> for AssetRef {
    fn from(reference: AssetProductRef) -> Self {
        Self::product(reference)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for AssetRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("AssetRef", 2)?;
        match self {
            Self::Path(path) => {
                state.serialize_field("kind", "path")?;
                state.serialize_field("value", path.as_str())?;
            }
            Self::StableId(id) => {
                state.serialize_field("kind", "stable_id")?;
                state.serialize_field("value", &id.to_string())?;
            }
            Self::Product(reference) => {
                state.serialize_field("kind", "product")?;
                state.serialize_field("value", reference)?;
            }
        }
        state.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for AssetRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, value::MapAccessDeserializer};

        #[derive(serde::Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            Kind,
            Value,
        }

        // The two payload shapes can be decoded before kind without buffering an untyped tree.
        enum Payload {
            Text(String),
            Product(AssetProductRef),
        }

        impl<'de> serde::Deserialize<'de> for Payload {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct PayloadVisitor;

                impl<'de> de::Visitor<'de> for PayloadVisitor {
                    type Value = Payload;

                    fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                        formatter.write_str("asset reference text or a complete product record")
                    }

                    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        Ok(Payload::Text(value.to_owned()))
                    }

                    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                        Ok(Payload::Text(value))
                    }

                    fn visit_map<M: de::MapAccess<'de>>(
                        self,
                        map: M,
                    ) -> Result<Self::Value, M::Error> {
                        <AssetProductRef as serde::Deserialize>::deserialize(
                            MapAccessDeserializer::new(map),
                        )
                        .map(Payload::Product)
                    }
                }

                deserializer.deserialize_any(PayloadVisitor)
            }
        }

        struct ReferenceVisitor;

        impl<'de> de::Visitor<'de> for ReferenceVisitor {
            type Value = AssetRef;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str("an asset reference record")
            }

            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut kind = None;
                let mut value = None;
                while let Some(field) = map.next_key()? {
                    match field {
                        Field::Kind => {
                            if kind.is_some() {
                                return Err(de::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value::<String>()?);
                        }
                        Field::Value => {
                            if value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            value = Some(map.next_value::<Payload>()?);
                        }
                    }
                }
                let kind = kind.ok_or_else(|| de::Error::missing_field("kind"))?;
                let value = value.ok_or_else(|| de::Error::missing_field("value"))?;
                match (kind.as_str(), value) {
                    ("path", Payload::Text(value)) => {
                        AssetRef::path(value).map_err(de::Error::custom)
                    }
                    ("stable_id", Payload::Text(value)) => {
                        AssetRef::stable_id(value).map_err(de::Error::custom)
                    }
                    ("product", Payload::Product(reference)) => Ok(AssetRef::Product(reference)),
                    ("path" | "stable_id" | "product", _) => Err(de::Error::custom(
                        "asset reference kind does not match its value",
                    )),
                    _ => Err(de::Error::unknown_variant(
                        &kind,
                        &["path", "stable_id", "product"],
                    )),
                }
            }
        }

        deserializer.deserialize_struct("AssetRef", &["kind", "value"], ReferenceVisitor)
    }
}

impl Display for AssetRef {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => formatter.write_str(path.as_str()),
            Self::StableId(id) => write!(formatter, "stable_id:{id}"),
            Self::Product(reference) => write!(formatter, "product:{reference}"),
        }
    }
}

fn validate_asset_path(path: &str) -> Result<(), AssetPathError> {
    if path.is_empty() {
        return Err(AssetPathError::Empty);
    }
    if path.starts_with('/') {
        return Err(AssetPathError::Absolute);
    }
    if path.contains('\\') {
        return Err(AssetPathError::ContainsBackslash);
    }
    if path.contains(':') {
        return Err(AssetPathError::ContainsDrivePrefix);
    }
    if path.contains('\0') {
        return Err(AssetPathError::ContainsNull);
    }

    for segment in path.split('/') {
        match segment {
            "" => return Err(AssetPathError::ContainsEmptySegment),
            "." => return Err(AssetPathError::ContainsCurrentDirectory),
            ".." => return Err(AssetPathError::ContainsParentDirectory),
            _ => {}
        }
    }

    Ok(())
}

#[cfg(test)]
mod reference_tests {
    use super::*;

    const SOURCE: &str = "919617ea-c89c-4f83-9cce-b084de435a83";
    #[cfg(feature = "serde")]
    const OTHER_SOURCE: &str = "7214561b-ae28-454f-888b-fd049235d212";
    const PRODUCT: &str = "011a8cc0-343a-4b95-8adb-3aa6d81977ed";

    fn product_reference(source: &str, product: ImportedProductId) -> AssetProductRef {
        AssetProductRef::new(StableAssetId::parse_str(source).unwrap(), product).unwrap()
    }

    #[test]
    fn complete_product_selection_is_preserved_by_constructors_and_accessors() {
        let primary = product_reference(SOURCE, ImportedProductId::PRIMARY);
        let auxiliary = product_reference(
            SOURCE,
            ImportedProductId::parse_subproduct(PRODUCT).unwrap(),
        );
        let path = AssetRef::path("bundles/text.bundle").unwrap();
        let stable = AssetRef::stable_id(SOURCE).unwrap();

        assert!(path.is_primary());
        assert!(stable.is_primary());
        assert_eq!(path.as_path().unwrap().as_str(), "bundles/text.bundle");
        assert_eq!(path.as_stable_id(), None);
        assert_eq!(stable.as_stable_id(), Some(primary.source()));
        assert_eq!(stable.as_path(), None);
        assert_eq!(path.as_product(), None);
        assert_eq!(stable.as_product(), None);
        for reference in [primary, auxiliary] {
            let asset_ref = AssetRef::product(reference);
            assert_eq!(asset_ref, AssetRef::from(reference));
            assert_eq!(asset_ref.as_product(), Some(reference));
            assert_eq!(asset_ref.as_path(), None);
            assert_eq!(asset_ref.as_stable_id(), None);
            assert_eq!(asset_ref.is_primary(), reference == primary);
        }
        assert_ne!(AssetRef::product(primary), stable);
    }

    #[test]
    fn product_diagnostics_include_the_complete_bounded_identity() {
        let auxiliary = ImportedProductId::parse_subproduct(PRODUCT).unwrap();
        assert_eq!(
            AssetRef::product(product_reference(SOURCE, auxiliary)).to_string(),
            format!("product:{SOURCE}/{PRODUCT}")
        );
        assert_eq!(
            AssetRef::product(product_reference(SOURCE, ImportedProductId::PRIMARY)).to_string(),
            format!("product:{SOURCE}/primary")
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn asset_reference_records_round_trip_json_and_ron() {
        let auxiliary = ImportedProductId::parse_subproduct(PRODUCT).unwrap();
        for reference in [
            AssetRef::path("textures/terrain.png").unwrap(),
            AssetRef::stable_id(SOURCE).unwrap(),
            AssetRef::product(product_reference(SOURCE, ImportedProductId::PRIMARY)),
            AssetRef::product(product_reference(SOURCE, auxiliary)),
        ] {
            let json = serde_json::to_string(&reference).unwrap();
            assert_eq!(serde_json::from_str::<AssetRef>(&json).unwrap(), reference);
            let ron = ron::to_string(&reference).unwrap();
            assert_eq!(ron::from_str::<AssetRef>(&ron).unwrap(), reference);
            let named = ron::ser::to_string_pretty(
                &reference,
                ron::ser::PrettyConfig::default().struct_names(true),
            )
            .unwrap();
            assert_eq!(ron::from_str::<AssetRef>(&named).unwrap(), reference);
        }
        assert_eq!(
            ron::to_string(&AssetRef::path("textures/terrain.png").unwrap()).unwrap(),
            r#"(kind:"path",value:"textures/terrain.png")"#,
        );
        let reordered = format!(
            r#"(value:(product:(id:"{PRODUCT}",kind:"subproduct"),source:"{SOURCE}"),kind:"product")"#
        );
        assert_eq!(
            ron::from_str::<AssetRef>(&reordered).unwrap(),
            AssetRef::product(product_reference(SOURCE, auxiliary)),
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn asset_reference_ron_records_reject_sequences_unknown_and_duplicate_fields() {
        for wire in [
            r#"["path","textures/terrain.png"]"#.to_owned(),
            r#"("path","textures/terrain.png")"#.to_owned(),
            r#"(kind:"path",value:["textures/terrain.png"])"#.to_owned(),
            r#"(kind:"path",value:"textures/terrain.png",unknown:true)"#.to_owned(),
            r#"(kind:"path",kind:"path",value:"textures/terrain.png")"#.to_owned(),
            r#"(kind:"path",value:"textures/terrain.png",value:"textures/other.png")"#.to_owned(),
            r#"(kind:"path")"#.to_owned(),
            r#"(value:"textures/terrain.png")"#.to_owned(),
            r#"(kind:"unknown",value:"textures/terrain.png")"#.to_owned(),
            format!(r#"(kind:"product",value:"{SOURCE}")"#),
            format!(r#"(kind:"product",value:["{SOURCE}",(kind:"primary")])"#),
            format!(r#"(kind:"product",value:(source:"{SOURCE}",product:["primary"]))"#),
            format!(
                r#"(kind:"product",value:(source:"{SOURCE}",product:(kind:"primary"),unknown:true))"#
            ),
            format!(
                r#"(kind:"product",value:(source:"{SOURCE}",product:(kind:"primary",unknown:true)))"#
            ),
            format!(
                r#"(kind:"product",value:(source:"{SOURCE}",source:"{OTHER_SOURCE}",product:(kind:"primary")))"#
            ),
            format!(
                r#"(kind:"product",value:(source:"{SOURCE}",product:(kind:"subproduct",id:"{PRODUCT}",id:"{PRODUCT}")))"#
            ),
        ] {
            assert!(ron::from_str::<AssetRef>(&wire).is_err(), "{wire}");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn product_reference_wire_round_trips_canonical_source_scoped_selections() {
        let auxiliary = ImportedProductId::parse_subproduct(PRODUCT).unwrap();
        let mut references = std::collections::HashSet::new();
        for source in [SOURCE, OTHER_SOURCE] {
            for (product, product_wire) in [
                (
                    ImportedProductId::PRIMARY,
                    serde_json::json!({"kind": "primary"}),
                ),
                (
                    auxiliary,
                    serde_json::json!({"kind": "subproduct", "id": PRODUCT}),
                ),
            ] {
                let asset_ref = AssetRef::product(product_reference(source, product));
                let expected = serde_json::json!({
                    "kind": "product",
                    "value": {"source": source, "product": product_wire},
                });
                let encoded = serde_json::to_string(&asset_ref).unwrap();
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&encoded).unwrap(),
                    expected
                );
                assert_eq!(
                    serde_json::from_str::<AssetRef>(&encoded).unwrap(),
                    asset_ref
                );
                assert_eq!(
                    serde_json::from_value::<AssetRef>(expected).unwrap(),
                    asset_ref
                );
                assert!(references.insert(asset_ref));
            }
        }
        assert_eq!(references.len(), 4);
        let reordered = format!(
            r#"{{"value":{{"product":{{"id":"{PRODUCT}","kind":"subproduct"}},"source":"{SOURCE}"}},"kind":"product"}}"#
        );
        assert_eq!(
            serde_json::from_str::<AssetRef>(&reordered).unwrap(),
            AssetRef::product(product_reference(SOURCE, auxiliary))
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn product_reference_wire_rejects_unknown_fields_and_malformed_payloads() {
        use serde_json::json;

        for value in [
            json!(null),
            json!(true),
            json!(7),
            json!(SOURCE),
            json!([]),
            json!([SOURCE, {"kind": "primary"}]),
            json!({}),
            json!({"source": SOURCE}),
            json!({"product": {"kind": "primary"}}),
            json!({"source": SOURCE, "product": "primary"}),
            json!({"source": SOURCE, "product": ["primary"]}),
            json!({"source": SOURCE, "product": null}),
            json!({"source": SOURCE, "product": {"kind": "label", "id": PRODUCT}}),
            json!({"source": SOURCE, "product": {"kind": "subproduct"}}),
            json!({"source": SOURCE, "product": {"kind": "primary", "id": PRODUCT}}),
            json!({"source": SOURCE, "product": {"kind": "subproduct", "id": PRODUCT, "name": "hint"}}),
            json!({"source": SOURCE, "product": {"kind": "primary"}, "path": "hint"}),
            json!({"source": SOURCE.to_uppercase(), "product": {"kind": "primary"}}),
            json!({"source": "00000000-0000-0000-0000-000000000000", "product": {"kind": "primary"}}),
            json!({"source": SOURCE, "product": {"kind": "subproduct", "id": PRODUCT.to_uppercase()}}),
            json!({"source": SOURCE, "product": {"kind": "subproduct", "id": "00000000-0000-0000-0000-000000000000"}}),
        ] {
            let wire = json!({"kind": "product", "value": value});
            assert!(
                serde_json::from_value::<AssetRef>(wire.clone()).is_err(),
                "{wire}"
            );
        }
        let value = json!({"source": SOURCE, "product": {"kind": "primary"}});
        for wire in [
            json!({"kind": "unknown", "value": value}),
            json!({"kind": "product"}),
            json!({"kind": "product", "value": value, "path": "hint"}),
            json!({"kind": "product", "value": value, "version": 1}),
            json!(["product", value]),
        ] {
            assert!(
                serde_json::from_value::<AssetRef>(wire.clone()).is_err(),
                "{wire}"
            );
        }
        for wire in [
            format!(r#"{{"kind":"product","kind":"product","value":{value}}}"#),
            format!(r#"{{"kind":"product","value":{value},"value":{value}}}"#),
            format!(
                r#"{{"kind":"product","value":{{"source":"{SOURCE}","source":"{OTHER_SOURCE}","product":{{"kind":"primary"}}}}}}"#
            ),
        ] {
            assert!(serde_json::from_str::<AssetRef>(&wire).is_err(), "{wire}");
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn path_and_stable_reference_wire_and_validation_remain_primary() {
        for (asset_ref, wire) in [
            (
                AssetRef::path("bundles/text.bundle").unwrap(),
                serde_json::json!({"kind": "path", "value": "bundles/text.bundle"}),
            ),
            (
                AssetRef::stable_id(SOURCE).unwrap(),
                serde_json::json!({"kind": "stable_id", "value": SOURCE}),
            ),
        ] {
            assert!(asset_ref.is_primary());
            assert_eq!(serde_json::to_value(&asset_ref).unwrap(), wire);
            assert_eq!(serde_json::from_value::<AssetRef>(wire).unwrap(), asset_ref);
        }
        assert_eq!(
            serde_json::from_value::<AssetRef>(serde_json::json!({
                "kind": "stable_id", "value": SOURCE.to_uppercase(),
            }))
            .unwrap(),
            AssetRef::stable_id(SOURCE).unwrap()
        );
        for wire in [
            serde_json::json!({"kind": "path", "value": "../outside"}),
            serde_json::json!({"kind": "stable_id", "value": "invalid"}),
            serde_json::json!({"kind": "path", "value": {"source": SOURCE}}),
            serde_json::json!({"kind": "stable_id", "value": {"source": SOURCE}}),
            serde_json::json!({"kind": "path", "value": "text.bundle", "unknown": true}),
        ] {
            assert!(
                serde_json::from_value::<AssetRef>(wire.clone()).is_err(),
                "{wire}"
            );
        }
    }
}
