use std::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
};

use crate::{AssetPath, AssetPathError, AssetSourceKind, StableAssetId};

pub const IMPORT_CACHE_ROOT: &str = ".nara/import-cache";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportLabelKind {
    ImporterId,
    ImportProfile,
    ImportedAssetType,
    ImportDependencyRole,
    ArtifactLabel,
    SourceExtension,
}

impl Display for ImportLabelKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImporterId => formatter.write_str("importer id"),
            Self::ImportProfile => formatter.write_str("import profile"),
            Self::ImportedAssetType => formatter.write_str("imported asset type"),
            Self::ImportDependencyRole => formatter.write_str("import dependency role"),
            Self::ArtifactLabel => formatter.write_str("artifact label"),
            Self::SourceExtension => formatter.write_str("source extension"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportLabelError {
    Empty {
        kind: ImportLabelKind,
    },
    InvalidSegment {
        kind: ImportLabelKind,
        value: String,
    },
    InvalidCharacter {
        kind: ImportLabelKind,
        value: String,
        character: char,
    },
}

impl Display for ImportLabelError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty { kind } => write!(formatter, "{kind} is empty"),
            Self::InvalidSegment { kind, value } => {
                write!(formatter, "{kind} '{value}' is not a valid path segment")
            }
            Self::InvalidCharacter {
                kind,
                value,
                character,
            } => write!(
                formatter,
                "{kind} '{value}' contains invalid character '{character}'"
            ),
        }
    }
}

impl Error for ImportLabelError {}

pub(crate) fn validate_import_label(
    kind: ImportLabelKind,
    value: impl AsRef<str>,
) -> Result<String, ImportLabelError> {
    let raw = value.as_ref();
    if raw.is_empty() {
        return Err(ImportLabelError::Empty { kind });
    }

    let normalized = raw.to_ascii_lowercase();
    if normalized == "." || normalized == ".." {
        return Err(ImportLabelError::InvalidSegment {
            kind,
            value: raw.to_string(),
        });
    }

    for character in normalized.chars() {
        let valid = character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.');
        if !valid {
            return Err(ImportLabelError::InvalidCharacter {
                kind,
                value: raw.to_string(),
                character,
            });
        }
    }

    Ok(normalized)
}

macro_rules! import_label_type {
    ($name:ident, $kind:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl AsRef<str>) -> Result<Self, ImportLabelError> {
                validate_import_label($kind, value).map(Self)
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        #[cfg(feature = "serde")]
        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                Self::new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

import_label_type!(ImporterId, ImportLabelKind::ImporterId);
import_label_type!(ImportProfile, ImportLabelKind::ImportProfile);
import_label_type!(ImportedAssetType, ImportLabelKind::ImportedAssetType);
import_label_type!(ImportDependencyRole, ImportLabelKind::ImportDependencyRole);
import_label_type!(ArtifactLabel, ImportLabelKind::ArtifactLabel);

impl Default for ImportProfile {
    fn default() -> Self {
        Self("default".to_string())
    }
}

impl Default for ArtifactLabel {
    fn default() -> Self {
        Self("primary".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImporterVersion(u32);

impl ImporterVersion {
    #[must_use]
    pub const fn new(version: u32) -> Self {
        Self(version)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl Display for ImporterVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ArtifactFormatVersion(u32);

impl ArtifactFormatVersion {
    #[must_use]
    pub const fn new(version: u32) -> Self {
        Self(version)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl Display for ArtifactFormatVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestParseError {
    InvalidLength { expected: usize, actual: usize },
    InvalidCharacter { index: usize, character: char },
}

impl Display for DigestParseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLength { expected, actual } => {
                write!(
                    formatter,
                    "digest hex length is {actual}, expected {expected}"
                )
            }
            Self::InvalidCharacter { index, character } => {
                write!(
                    formatter,
                    "digest hex contains invalid character '{character}' at byte {index}"
                )
            }
        }
    }
}

impl Error for DigestParseError {}

macro_rules! digest_type {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 32]);

        impl $name {
            #[must_use]
            pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
                Self(*blake3::hash(bytes.as_ref()).as_bytes())
            }

            #[must_use]
            pub const fn from_digest(digest: [u8; 32]) -> Self {
                Self(digest)
            }

            pub fn from_hex(hex: impl AsRef<str>) -> Result<Self, DigestParseError> {
                decode_hex_32(hex.as_ref()).map(Self)
            }

            #[must_use]
            pub const fn as_bytes(self) -> [u8; 32] {
                self.0
            }

            #[must_use]
            pub fn to_hex(self) -> String {
                encode_hex(&self.0)
            }
        }

        impl Debug for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter
                    .debug_tuple(stringify!($name))
                    .field(&self.to_hex())
                    .finish()
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.to_hex())
            }
        }

        #[cfg(feature = "serde")]
        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(&self.to_hex())
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                Self::from_hex(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

digest_type!(SourceHash);
digest_type!(ImportSettingsHash);
digest_type!(ImportDependencyDigest);
digest_type!(ImportArtifactDigest);

impl ImportDependencyDigest {
    #[must_use]
    pub fn empty() -> Self {
        Self::from_dependencies(std::iter::empty::<&ImportDependency>())
    }

    #[must_use]
    pub fn from_dependencies<'a>(
        dependencies: impl IntoIterator<Item = &'a ImportDependency>,
    ) -> Self {
        let mut canonical_dependencies = dependencies
            .into_iter()
            .map(canonical_dependency_key)
            .collect::<Vec<_>>();
        canonical_dependencies.sort();
        canonical_dependencies.dedup();

        let mut hasher = blake3::Hasher::new();
        feed_field(&mut hasher, "domain", b"nara.import_dependency_digest.v1");
        feed_field(
            &mut hasher,
            "count",
            &(canonical_dependencies.len() as u64).to_le_bytes(),
        );
        for dependency in canonical_dependencies {
            feed_field(&mut hasher, "dependency", &dependency);
        }
        Self(*hasher.finalize().as_bytes())
    }
}

impl Default for ImportSettingsHash {
    fn default() -> Self {
        Self::from_bytes([])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImportDependency {
    stable_id: StableAssetId,
    path: AssetPath,
    source_hash: SourceHash,
    source_kind: AssetSourceKind,
    role: ImportDependencyRole,
}

impl ImportDependency {
    #[must_use]
    pub const fn new(
        stable_id: StableAssetId,
        path: AssetPath,
        source_hash: SourceHash,
        source_kind: AssetSourceKind,
        role: ImportDependencyRole,
    ) -> Self {
        Self {
            stable_id,
            path,
            source_hash,
            source_kind,
            role,
        }
    }

    #[must_use]
    pub const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub const fn source_hash(&self) -> SourceHash {
        self.source_hash
    }

    #[must_use]
    pub const fn source_kind(&self) -> &AssetSourceKind {
        &self.source_kind
    }

    #[must_use]
    pub const fn role(&self) -> &ImportDependencyRole {
        &self.role
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImportArtifactKey {
    stable_id: StableAssetId,
    source_hash: SourceHash,
    dependency_digest: ImportDependencyDigest,
    importer_id: ImporterId,
    importer_version: ImporterVersion,
    settings_hash: ImportSettingsHash,
    profile: ImportProfile,
    output_asset_type: ImportedAssetType,
    artifact_label: ArtifactLabel,
    artifact_format_version: ArtifactFormatVersion,
}

impl ImportArtifactKey {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        stable_id: StableAssetId,
        source_hash: SourceHash,
        dependency_digest: ImportDependencyDigest,
        importer_id: ImporterId,
        importer_version: ImporterVersion,
        settings_hash: ImportSettingsHash,
        profile: ImportProfile,
        output_asset_type: ImportedAssetType,
        artifact_label: ArtifactLabel,
        artifact_format_version: ArtifactFormatVersion,
    ) -> Self {
        Self {
            stable_id,
            source_hash,
            dependency_digest,
            importer_id,
            importer_version,
            settings_hash,
            profile,
            output_asset_type,
            artifact_label,
            artifact_format_version,
        }
    }

    #[must_use]
    pub const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub const fn source_hash(&self) -> SourceHash {
        self.source_hash
    }

    #[must_use]
    pub const fn dependency_digest(&self) -> ImportDependencyDigest {
        self.dependency_digest
    }

    #[must_use]
    pub const fn importer_id(&self) -> &ImporterId {
        &self.importer_id
    }

    #[must_use]
    pub const fn importer_version(&self) -> ImporterVersion {
        self.importer_version
    }

    #[must_use]
    pub const fn settings_hash(&self) -> ImportSettingsHash {
        self.settings_hash
    }

    #[must_use]
    pub const fn profile(&self) -> &ImportProfile {
        &self.profile
    }

    #[must_use]
    pub const fn output_asset_type(&self) -> &ImportedAssetType {
        &self.output_asset_type
    }

    #[must_use]
    pub const fn artifact_label(&self) -> &ArtifactLabel {
        &self.artifact_label
    }

    #[must_use]
    pub const fn artifact_format_version(&self) -> ArtifactFormatVersion {
        self.artifact_format_version
    }

    #[must_use]
    pub fn digest(&self) -> ImportArtifactDigest {
        let mut hasher = blake3::Hasher::new();
        feed_field(&mut hasher, "domain", b"nara.import_artifact_key.v1");
        feed_field(
            &mut hasher,
            "stable_id",
            self.stable_id.to_string().as_bytes(),
        );
        feed_field(&mut hasher, "source_hash", &self.source_hash.as_bytes());
        feed_field(
            &mut hasher,
            "dependency_digest",
            &self.dependency_digest.as_bytes(),
        );
        feed_field(
            &mut hasher,
            "importer_id",
            self.importer_id.as_str().as_bytes(),
        );
        feed_field(
            &mut hasher,
            "importer_version",
            &self.importer_version.raw().to_le_bytes(),
        );
        feed_field(&mut hasher, "settings_hash", &self.settings_hash.as_bytes());
        feed_field(&mut hasher, "profile", self.profile.as_str().as_bytes());
        feed_field(
            &mut hasher,
            "output_asset_type",
            self.output_asset_type.as_str().as_bytes(),
        );
        feed_field(
            &mut hasher,
            "artifact_label",
            self.artifact_label.as_str().as_bytes(),
        );
        feed_field(
            &mut hasher,
            "artifact_format_version",
            &self.artifact_format_version.raw().to_le_bytes(),
        );
        ImportArtifactDigest::from_digest(*hasher.finalize().as_bytes())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImportArtifactRecord {
    key: ImportArtifactKey,
    artifact_path: ImportArtifactPath,
}

impl ImportArtifactRecord {
    pub fn new(key: ImportArtifactKey) -> Result<Self, ImportArtifactPathError> {
        let artifact_path = ImportArtifactPath::for_key(&key)?;
        Ok(Self { key, artifact_path })
    }

    #[must_use]
    pub const fn key(&self) -> &ImportArtifactKey {
        &self.key
    }

    #[must_use]
    pub const fn artifact_path(&self) -> &ImportArtifactPath {
        &self.artifact_path
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImportArtifactPath(String);

impl ImportArtifactPath {
    pub fn new(path: impl Into<String>) -> Result<Self, ImportArtifactPathError> {
        let path = path.into();
        AssetPath::new(path.clone()).map_err(ImportArtifactPathError::InvalidPath)?;
        if !path.starts_with(&format!("{IMPORT_CACHE_ROOT}/")) {
            return Err(ImportArtifactPathError::OutsideImportCache { path });
        }
        Ok(Self(path))
    }

    pub fn for_key(key: &ImportArtifactKey) -> Result<Self, ImportArtifactPathError> {
        Self::new(format!(
            "{IMPORT_CACHE_ROOT}/{}/{}/{}/{}/{}-{}.v{}.artifact",
            key.importer_id(),
            key.profile(),
            key.output_asset_type(),
            key.stable_id(),
            key.artifact_label(),
            key.digest(),
            key.artifact_format_version(),
        ))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ImportArtifactPath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ImportArtifactPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ImportArtifactPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let path = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(path).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportArtifactPathError {
    InvalidPath(AssetPathError),
    OutsideImportCache { path: String },
}

impl Display for ImportArtifactPathError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(error) => write!(formatter, "invalid import artifact path: {error}"),
            Self::OutsideImportCache { path } => write!(
                formatter,
                "import artifact path '{path}' must live under {IMPORT_CACHE_ROOT}/"
            ),
        }
    }
}

impl Error for ImportArtifactPathError {}

fn canonical_dependency_key(dependency: &ImportDependency) -> Vec<u8> {
    let mut key = Vec::new();
    push_canonical_field(&mut key, dependency.role().as_str().as_bytes());
    push_canonical_field(&mut key, dependency.stable_id().to_string().as_bytes());
    push_canonical_field(&mut key, dependency.path().as_str().as_bytes());
    push_canonical_source_kind(&mut key, dependency.source_kind());
    push_canonical_field(&mut key, &dependency.source_hash().as_bytes());
    key
}

fn push_canonical_source_kind(key: &mut Vec<u8>, source_kind: &AssetSourceKind) {
    match source_kind {
        AssetSourceKind::Unknown => push_canonical_field(key, b"unknown"),
        AssetSourceKind::Image => push_canonical_field(key, b"image"),
        AssetSourceKind::Scene => push_canonical_field(key, b"scene"),
        AssetSourceKind::Prefab => push_canonical_field(key, b"prefab"),
        AssetSourceKind::Other(kind) => {
            push_canonical_field(key, b"other");
            push_canonical_field(key, kind.as_bytes());
        }
    }
}

fn push_canonical_field(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    output.extend_from_slice(bytes);
}

fn feed_field(hasher: &mut blake3::Hasher, name: &str, bytes: &[u8]) {
    feed_len_prefixed(hasher, name.as_bytes());
    feed_len_prefixed(hasher, bytes);
}

fn feed_len_prefixed(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn encode_hex(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex_32(hex: &str) -> Result<[u8; 32], DigestParseError> {
    if hex.len() != 64 {
        return Err(DigestParseError::InvalidLength {
            expected: 64,
            actual: hex.len(),
        });
    }

    let mut digest = [0; 32];
    for (index, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        let high = decode_hex_nibble(chunk[0] as char, index * 2)?;
        let low = decode_hex_nibble(chunk[1] as char, index * 2 + 1)?;
        digest[index] = (high << 4) | low;
    }
    Ok(digest)
}

fn decode_hex_nibble(character: char, index: usize) -> Result<u8, DigestParseError> {
    character
        .to_digit(16)
        .map(|value| value as u8)
        .ok_or(DigestParseError::InvalidCharacter { index, character })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stable_id(value: &str) -> StableAssetId {
        StableAssetId::parse_str(value).unwrap()
    }

    fn key() -> ImportArtifactKey {
        ImportArtifactKey::new(
            stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f"),
            SourceHash::from_bytes(b"source"),
            ImportDependencyDigest::empty(),
            ImporterId::new("mock_importer").unwrap(),
            ImporterVersion::new(1),
            ImportSettingsHash::from_bytes(b"settings"),
            ImportProfile::default(),
            ImportedAssetType::new("mock_asset").unwrap(),
            ArtifactLabel::default(),
            ArtifactFormatVersion::new(1),
        )
    }

    #[test]
    fn artifact_key_changes_for_all_identity_inputs() {
        let base = key().digest();

        let mut changed = key();
        changed.source_hash = SourceHash::from_bytes(b"changed-source");
        assert_ne!(base, changed.digest());

        let mut changed = key();
        changed.dependency_digest = ImportDependencyDigest::from_digest([1; 32]);
        assert_ne!(base, changed.digest());

        let mut changed = key();
        changed.importer_version = ImporterVersion::new(2);
        assert_ne!(base, changed.digest());

        let mut changed = key();
        changed.settings_hash = ImportSettingsHash::from_bytes(b"changed-settings");
        assert_ne!(base, changed.digest());

        let mut changed = key();
        changed.profile = ImportProfile::new("mobile").unwrap();
        assert_ne!(base, changed.digest());

        let mut changed = key();
        changed.stable_id = stable_id("b73f0f16-09e8-4265-b090-b689b41c197e");
        assert_ne!(base, changed.digest());

        let mut changed = key();
        changed.output_asset_type = ImportedAssetType::new("other_asset").unwrap();
        assert_ne!(base, changed.digest());
    }

    #[test]
    fn artifact_paths_are_deterministic_and_under_import_cache() {
        let first = ImportArtifactRecord::new(key()).unwrap();
        let second = ImportArtifactRecord::new(key()).unwrap();

        assert_eq!(first, second);
        assert!(
            first
                .artifact_path()
                .as_str()
                .starts_with(".nara/import-cache/mock_importer/default/mock_asset/")
        );
    }

    #[test]
    fn dependency_digest_sorts_and_deduplicates_records() {
        let dependency_a = ImportDependency::new(
            stable_id("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f"),
            AssetPath::new("textures/a.png").unwrap(),
            SourceHash::from_bytes(b"a"),
            AssetSourceKind::Image,
            ImportDependencyRole::new("texture").unwrap(),
        );
        let dependency_b = ImportDependency::new(
            stable_id("b73f0f16-09e8-4265-b090-b689b41c197e"),
            AssetPath::new("textures/b.png").unwrap(),
            SourceHash::from_bytes(b"b"),
            AssetSourceKind::Image,
            ImportDependencyRole::new("texture").unwrap(),
        );

        let first = ImportDependencyDigest::from_dependencies([&dependency_a, &dependency_b]);
        let reversed = ImportDependencyDigest::from_dependencies([&dependency_b, &dependency_a]);
        let duplicated = ImportDependencyDigest::from_dependencies([
            &dependency_a,
            &dependency_b,
            &dependency_a,
        ]);

        assert_eq!(first, reversed);
        assert_eq!(first, duplicated);

        let different_role = ImportDependency::new(
            dependency_a.stable_id(),
            dependency_a.path().clone(),
            dependency_a.source_hash(),
            dependency_a.source_kind().clone(),
            ImportDependencyRole::new("normal_map").unwrap(),
        );
        assert_ne!(
            first,
            ImportDependencyDigest::from_dependencies([&different_role, &dependency_b])
        );

        let different_source_hash = ImportDependency::new(
            dependency_a.stable_id(),
            dependency_a.path().clone(),
            SourceHash::from_bytes(b"changed-a"),
            dependency_a.source_kind().clone(),
            dependency_a.role().clone(),
        );
        assert_ne!(
            first,
            ImportDependencyDigest::from_dependencies([&different_source_hash, &dependency_b])
        );
    }

    #[test]
    fn digest_hex_roundtrips_and_validates_length() {
        let digest = SourceHash::from_bytes(b"source");

        assert_eq!(SourceHash::from_hex(digest.to_hex()).unwrap(), digest);
        assert!(matches!(
            SourceHash::from_hex("abc").unwrap_err(),
            DigestParseError::InvalidLength { .. }
        ));
    }
}
