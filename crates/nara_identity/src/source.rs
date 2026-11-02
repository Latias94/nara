use std::{
    borrow::Borrow,
    error::Error,
    fmt::{self, Display, Formatter},
    mem::size_of,
    str::FromStr,
    sync::Arc,
};

use nara_core::SourceUniverseId;
use uuid::{Builder, Uuid};

#[cfg(feature = "serde")]
mod wire;

/// A stable source identity used by asset metadata and source-document identities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StableAssetId(Uuid);

impl StableAssetId {
    /// Generates a stable identity from the platform's fallible random-byte source.
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

/// A canonical logical source location in one admitted catalogue.
///
/// This lexical value grants no filesystem access and cannot be used as an authored alias
/// without an explicit Asset-owned path scope.
///
/// ```
/// use nara_identity::AssetSourcePath;
///
/// let root = AssetSourcePath::new("Content/Images").unwrap();
/// let file = AssetSourcePath::new("textures/player.png").unwrap();
/// assert_eq!(
///     root.joined(&file).as_str(),
///     "Content/Images/textures/player.png",
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssetSourcePath(String);

impl AssetSourcePath {
    pub fn new(path: impl Into<String>) -> Result<Self, AssetPathError> {
        let path = path.into();
        validate_asset_path(&path)?;
        Ok(Self(path))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Joins two already validated, non-empty canonical path values.
    ///
    /// The result is valid because both operands have already passed the canonical lexical
    /// validator. This operation does not reparse or normalize either operand.
    #[must_use]
    pub fn joined(&self, suffix: &Self) -> Self {
        let mut path = String::with_capacity(self.0.len() + 1 + suffix.0.len());
        path.push_str(&self.0);
        path.push('/');
        path.push_str(&suffix.0);
        Self(path)
    }

    #[must_use]
    pub fn meta_path(&self) -> String {
        format!("{}.meta", self.0)
    }

    /// Returns the modeled retained capacity for cross-crate ownership accounting.
    #[doc(hidden)]
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.0.capacity()
    }
}

impl Display for AssetSourcePath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Borrow<str> for AssetSourcePath {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for AssetSourcePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for AssetSourcePath {
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
            Self::Absolute => formatter.write_str("asset path must be relative"),
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

/// Validates the shared canonical and authored relative path grammar.
///
/// Asset-owned aliases and canonical source locations use the same lexical rules. Scope
/// interpretation and catalogue policy remain outside this function.
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

/// The authored kind of one entity-bearing source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceDocumentKind {
    Scene,
    Prefab,
}

/// The semantic origin of one source document.
///
/// File identity and content digests are admission evidence, not members of this value. A path
/// origin is intentionally scoped by an authored universe and is not rename continuity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceDocumentOrigin {
    Named {
        universe: SourceUniverseId,
        name: Arc<str>,
    },
    ProjectLocation {
        universe: SourceUniverseId,
        path: AssetSourcePath,
    },
    StableAsset {
        stable_id: StableAssetId,
    },
}

/// Canonical semantic identity for one admitted Scene or Prefab document.
///
/// This identifies an entity-bearing source document, not an auxiliary imported product.
/// A primary product selector requires its resolver's admitted source-asset association;
/// it does not change this key's origin or establish arbitrary domain product membership.
///
/// With the `serde` feature, the representation preserves the document kind and explicit
/// origin. Deserialization validates an identity claim; it does not resolve a source or grant
/// Host admission. Standalone serde is not file admission: the containing Scene format owns
/// input preflight and aggregate bounds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceDocumentKey {
    origin: SourceDocumentOrigin,
    kind: SourceDocumentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceDocumentBindingError {
    DocumentKind,
    SourceKind,
    SourceIdentity,
    SourceKeyMismatch,
}

impl fmt::Display for SourceDocumentBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DocumentKind => "source binding has the wrong document kind",
            Self::SourceKind => "asset metadata has the wrong document source kind",
            Self::SourceIdentity => "document source metadata requires a non-nil stable asset ID",
            Self::SourceKeyMismatch => {
                "asset metadata conflicts with the canonical document source key"
            }
        })
    }
}

impl Error for SourceDocumentBindingError {}

impl SourceDocumentKey {
    /// Creates an explicit code-authored source identity.
    pub fn named(
        universe: SourceUniverseId,
        name: impl Into<String>,
        kind: SourceDocumentKind,
    ) -> Result<Self, SourceDocumentKeyError> {
        let name = name.into();
        if name.is_empty() {
            return Err(SourceDocumentKeyError::EmptyName);
        }
        if name.contains('\0') {
            return Err(SourceDocumentKeyError::NameContainsNull);
        }
        Ok(Self {
            origin: SourceDocumentOrigin::Named {
                universe,
                name: Arc::from(name),
            },
            kind,
        })
    }

    /// Creates an identity for an intentionally location-authored project document.
    #[must_use]
    pub const fn project_location(
        universe: SourceUniverseId,
        path: AssetSourcePath,
        kind: SourceDocumentKind,
    ) -> Self {
        Self {
            origin: SourceDocumentOrigin::ProjectLocation { universe, path },
            kind,
        }
    }

    /// Creates an identity backed by an admitted stable asset metadata record.
    #[must_use]
    pub const fn stable_asset(stable_id: StableAssetId, kind: SourceDocumentKind) -> Self {
        Self {
            origin: SourceDocumentOrigin::StableAsset { stable_id },
            kind,
        }
    }

    #[must_use]
    pub const fn origin(&self) -> &SourceDocumentOrigin {
        &self.origin
    }

    #[must_use]
    pub const fn kind(&self) -> SourceDocumentKind {
        self.kind
    }

    /// Returns the modeled owned bytes charged by Scene retention accounting.
    ///
    /// Shared names are conservatively charged for every retained key. This models logical
    /// ownership rather than allocator capacity or process resident memory.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.checked_retained_bytes().unwrap_or(usize::MAX)
    }

    /// Returns the modeled retained bytes or `None` when checked accounting overflows.
    #[doc(hidden)]
    #[must_use]
    pub fn checked_retained_bytes(&self) -> Option<usize> {
        let dynamic = match &self.origin {
            SourceDocumentOrigin::Named { name, .. } => retained_string_bytes(name.as_ref())?,
            SourceDocumentOrigin::ProjectLocation { path, .. } => {
                retained_string_bytes(path.as_str())?
            }
            SourceDocumentOrigin::StableAsset { .. } => 0,
        };
        size_of::<Self>().checked_add(dynamic)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceDocumentKeyError {
    EmptyName,
    NameContainsNull,
}

impl fmt::Display for SourceDocumentKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("source document name must not be empty"),
            Self::NameContainsNull => {
                formatter.write_str("source document name must not contain NUL")
            }
        }
    }
}

impl Error for SourceDocumentKeyError {}

fn retained_string_bytes(value: &str) -> Option<usize> {
    value.len().checked_add(2 * size_of::<usize>())
}

#[cfg(test)]
mod tests {
    use super::*;

    const STABLE_ID: &str = "00000000-0000-4000-8000-000000000041";

    fn universe() -> SourceUniverseId {
        "72011426-37e2-48ba-bc06-78841468889a"
            .parse()
            .expect("test source universe is valid")
    }

    fn source(path: &str) -> AssetSourcePath {
        AssetSourcePath::new(path).expect("test source path is valid")
    }

    #[test]
    fn stable_asset_ids_preserve_uuid_parsing_and_typed_errors() {
        let id = StableAssetId::parse_str(STABLE_ID).unwrap();
        assert_eq!(id.to_string(), STABLE_ID);
        assert_eq!(
            id.as_uuid(),
            uuid::Uuid::parse_str(STABLE_ID).expect("test stable id is valid")
        );
        assert_eq!(
            StableAssetId::parse_str("asset-123"),
            Err(StableAssetIdError::Invalid("asset-123".to_owned()))
        );
    }

    #[test]
    fn canonical_source_paths_reject_invalid_names_and_join_without_reparsing() {
        for (path, error) in [
            ("", AssetPathError::Empty),
            ("/absolute.png", AssetPathError::Absolute),
            ("textures\\player.png", AssetPathError::ContainsBackslash),
            (
                "C:/textures/player.png",
                AssetPathError::ContainsDrivePrefix,
            ),
            (
                "textures/./player.png",
                AssetPathError::ContainsCurrentDirectory,
            ),
            (
                "textures/../player.png",
                AssetPathError::ContainsParentDirectory,
            ),
            ("textures//player.png", AssetPathError::ContainsEmptySegment),
            ("textures/player.png/", AssetPathError::ContainsEmptySegment),
            ("textures/player\0.png", AssetPathError::ContainsNull),
        ] {
            assert_eq!(AssetSourcePath::new(path), Err(error));
        }

        let prefix = source("Content/Images");
        let suffix = source("textures/player.png");
        let joined = prefix.joined(&suffix);
        assert_eq!(joined.as_str(), "Content/Images/textures/player.png");
        assert_eq!(
            joined.meta_path(),
            "Content/Images/textures/player.png.meta"
        );
        assert_eq!(
            <AssetSourcePath as Borrow<str>>::borrow(&joined),
            joined.as_str()
        );
    }

    #[test]
    fn source_key_keeps_origin_and_document_kind_as_distinct_axes() {
        let path = source("scenes/startup.scene.json");
        let scene = SourceDocumentKey::project_location(
            universe(),
            path.clone(),
            SourceDocumentKind::Scene,
        );
        let prefab =
            SourceDocumentKey::project_location(universe(), path, SourceDocumentKind::Prefab);

        assert_ne!(scene, prefab);
        assert_eq!(scene.kind(), SourceDocumentKind::Scene);
        assert!(matches!(
            scene.origin(),
            SourceDocumentOrigin::ProjectLocation { universe: value, path }
                if *value == universe() && path.as_str() == "scenes/startup.scene.json"
        ));
        assert_ne!(
            SourceDocumentKey::named(universe(), "startup", SourceDocumentKind::Scene).unwrap(),
            SourceDocumentKey::stable_asset(
                StableAssetId::parse_str(STABLE_ID).unwrap(),
                SourceDocumentKind::Scene,
            )
        );
    }

    #[test]
    fn named_source_rejects_empty_and_nul_names_without_fallback() {
        assert_eq!(
            SourceDocumentKey::named(universe(), "", SourceDocumentKind::Scene),
            Err(SourceDocumentKeyError::EmptyName)
        );
        assert_eq!(
            SourceDocumentKey::named(universe(), "bad\0name", SourceDocumentKind::Scene),
            Err(SourceDocumentKeyError::NameContainsNull)
        );
    }

    #[test]
    fn checked_retained_bytes_preserves_the_scene_model() {
        let name = "startup";
        let key = SourceDocumentKey::named(universe(), name, SourceDocumentKind::Scene).unwrap();
        let expected = size_of::<SourceDocumentKey>() + name.len() + 2 * size_of::<usize>();
        assert_eq!(key.checked_retained_bytes(), Some(expected));
        assert_eq!(
            SourceDocumentKey::stable_asset(
                StableAssetId::parse_str(STABLE_ID).unwrap(),
                SourceDocumentKind::Scene,
            )
            .checked_retained_bytes(),
            Some(size_of::<SourceDocumentKey>())
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn pure_source_values_round_trip_their_json_and_ron_wire_shapes() {
        let path = source("Content/Images/player.png");
        let stable_id = StableAssetId::parse_str(STABLE_ID).unwrap();
        let key = SourceDocumentKey::stable_asset(stable_id, SourceDocumentKind::Prefab);

        assert_eq!(
            serde_json::to_string(&path).unwrap(),
            r#""Content/Images/player.png""#
        );
        assert_eq!(
            serde_json::from_str::<AssetSourcePath>(r#""Content/Images/player.png""#).unwrap(),
            path
        );
        assert_eq!(
            ron::to_string(&path).unwrap(),
            r#""Content/Images/player.png""#
        );
        assert_eq!(
            ron::from_str::<AssetSourcePath>(r#""Content/Images/player.png""#).unwrap(),
            path
        );

        for wire in [
            serde_json::to_string(&stable_id).unwrap(),
            ron::to_string(&stable_id).unwrap(),
        ] {
            assert_eq!(
                serde_json::from_str::<StableAssetId>(&wire).unwrap(),
                stable_id
            );
            assert_eq!(ron::from_str::<StableAssetId>(&wire).unwrap(), stable_id);
        }

        let json = serde_json::to_string(&key).unwrap();
        let ron = ron::to_string(&key).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap(),
            serde_json::json!({
                "kind": "prefab",
                "origin": {"kind": "stable_asset", "stable_id": STABLE_ID}
            })
        );
        assert_eq!(
            serde_json::from_str::<SourceDocumentKey>(&json).unwrap(),
            key
        );
        assert_eq!(ron::from_str::<SourceDocumentKey>(&ron).unwrap(), key);
        assert!(
            serde_json::from_value::<SourceDocumentKey>(serde_json::json!({
                "kind": "prefab",
                "origin": {"kind": "stable_asset", "stable_id": STABLE_ID}
            }))
            .is_ok()
        );
        assert!(
            serde_json::from_value::<SourceDocumentKey>(serde_json::json!({
                "kind": "prefab",
                "origin": {"product": {
                    "source": STABLE_ID,
                    "product": {"kind": "primary"}
                }}
            }))
            .is_err()
        );

        for invalid in [
            "",
            "/absolute.png",
            "textures\\player.png",
            "C:/player.png",
            "textures/./player.png",
            "textures/../player.png",
            "textures//player.png",
            "textures/player.png/",
            "textures/player\0.png",
        ] {
            let json = serde_json::to_string(invalid).unwrap();
            let ron = ron::to_string(invalid).unwrap();
            assert!(serde_json::from_str::<AssetSourcePath>(&json).is_err());
            assert!(ron::from_str::<AssetSourcePath>(&ron).is_err());
        }
    }
}
