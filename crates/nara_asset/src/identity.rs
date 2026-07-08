use std::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    str::FromStr,
};

use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AssetId(u64);

impl AssetId {
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", content = "value", rename_all = "snake_case")
)]
pub enum AssetRef {
    Path(AssetPath),
    StableId(StableAssetId),
}

impl AssetRef {
    pub fn path(path: impl Into<String>) -> Result<Self, AssetPathError> {
        Ok(Self::Path(AssetPath::new(path)?))
    }

    pub fn stable_id(id: impl AsRef<str>) -> Result<Self, StableAssetIdError> {
        Ok(Self::StableId(StableAssetId::parse_str(id)?))
    }

    #[must_use]
    pub fn as_path(&self) -> Option<&AssetPath> {
        match self {
            Self::Path(path) => Some(path),
            Self::StableId(_) => None,
        }
    }

    #[must_use]
    pub fn as_stable_id(&self) -> Option<StableAssetId> {
        match self {
            Self::Path(_) => None,
            Self::StableId(id) => Some(*id),
        }
    }
}

impl Display for AssetRef {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => formatter.write_str(path.as_str()),
            Self::StableId(id) => write!(formatter, "stable_id:{id}"),
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
