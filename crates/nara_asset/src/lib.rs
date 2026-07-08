//! Asset identity, typed handles, and in-memory asset tables.

use std::{
    collections::HashMap,
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use nara_ecs::Resource;

pub trait Asset: Send + Sync + 'static {}

impl<T> Asset for T where T: Send + Sync + 'static {}

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

pub struct Handle<T> {
    id: AssetId,
    marker: PhantomData<fn() -> T>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    StableId(String),
}

impl AssetRef {
    pub fn path(path: impl Into<String>) -> Result<Self, AssetPathError> {
        Ok(Self::Path(AssetPath::new(path)?))
    }

    #[must_use]
    pub fn stable_id(id: impl Into<String>) -> Self {
        Self::StableId(id.into())
    }

    #[must_use]
    pub fn as_path(&self) -> Option<&AssetPath> {
        match self {
            Self::Path(path) => Some(path),
            Self::StableId(_) => None,
        }
    }

    pub fn resolve<T>(&self, asset_server: &mut AssetServer) -> Result<Handle<T>, AssetRefError> {
        match self {
            Self::Path(path) => asset_server.reserve::<T>(path.as_str()).map_err(Into::into),
            Self::StableId(id) => Err(AssetRefError::UnsupportedStableId(id.clone())),
        }
    }

    pub fn from_handle<T>(
        asset_server: &AssetServer,
        handle: Handle<T>,
    ) -> Result<Self, AssetRefError> {
        let path = asset_server
            .path(handle.id())
            .ok_or_else(|| AssetRefError::UnknownHandle(handle.id()))?;
        Self::path(path).map_err(Into::into)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetRefError {
    InvalidPath(AssetPathError),
    UnsupportedStableId(String),
    UnknownHandle(AssetId),
    Asset(AssetError),
}

impl Display for AssetRefError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(error) => write!(formatter, "invalid asset path: {error}"),
            Self::UnsupportedStableId(id) => {
                write!(formatter, "stable asset id '{id}' is not supported yet")
            }
            Self::UnknownHandle(id) => write!(formatter, "asset handle {:?} has no known path", id),
            Self::Asset(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for AssetRefError {}

impl From<AssetPathError> for AssetRefError {
    fn from(error: AssetPathError) -> Self {
        Self::InvalidPath(error)
    }
}

impl From<AssetError> for AssetRefError {
    fn from(error: AssetError) -> Self {
        Self::Asset(error)
    }
}

impl<T> Handle<T> {
    #[must_use]
    pub const fn new(id: AssetId) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }

    #[must_use]
    pub const fn id(self) -> AssetId {
        self.id
    }
}

impl<T> Copy for Handle<T> {}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Debug for Handle<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Handle")
            .field("id", &self.id)
            .finish()
    }
}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Handle<T> {}

impl<T> Hash for Handle<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    IdSpaceExhausted,
    InvalidPath(AssetPathError),
}

impl Display for AssetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdSpaceExhausted => formatter.write_str("asset id space exhausted"),
            Self::InvalidPath(error) => write!(formatter, "invalid asset path: {error}"),
        }
    }
}

impl Error for AssetError {}

#[derive(Debug, Resource)]
pub struct AssetServer {
    next_id: u64,
    paths: HashMap<String, AssetId>,
    reverse_paths: HashMap<AssetId, String>,
}

impl Default for AssetServer {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetServer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: 1,
            paths: HashMap::new(),
            reverse_paths: HashMap::new(),
        }
    }

    pub fn reserve<T>(&mut self, path: impl Into<String>) -> Result<Handle<T>, AssetError> {
        let path = AssetPath::new(path.into()).map_err(AssetError::InvalidPath)?;
        let path = path.as_str().to_string();
        if let Some(id) = self.paths.get(&path).copied() {
            return Ok(Handle::new(id));
        }

        let id = AssetId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(AssetError::IdSpaceExhausted)?;

        self.paths.insert(path.clone(), id);
        self.reverse_paths.insert(id, path);
        Ok(Handle::new(id))
    }

    #[must_use]
    pub fn path(&self, id: AssetId) -> Option<&str> {
        self.reverse_paths.get(&id).map(String::as_str)
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

#[derive(Debug, Resource)]
pub struct Assets<T: Asset> {
    next_id: u64,
    values: HashMap<AssetId, T>,
}

impl<T: Asset> Default for Assets<T> {
    fn default() -> Self {
        Self {
            next_id: 1,
            values: HashMap::new(),
        }
    }
}

impl<T: Asset> Assets<T> {
    pub fn add(&mut self, value: T) -> Handle<T> {
        let id = AssetId(self.next_id);
        self.next_id += 1;
        self.values.insert(id, value);
        Handle::new(id)
    }

    pub fn insert(&mut self, handle: Handle<T>, value: T) -> Option<T> {
        self.values.insert(handle.id(), value)
    }

    #[must_use]
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        self.values.get(&handle.id())
    }

    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        self.values.get_mut(&handle.id())
    }

    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        self.values.remove(&handle.id())
    }

    pub fn iter(&self) -> impl Iterator<Item = (Handle<T>, &T)> + '_ {
        self.values
            .iter()
            .map(|(id, value)| (Handle::new(*id), value))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserves_stable_path_handles() {
        let mut server = AssetServer::new();
        let first = server.reserve::<String>("player.png").unwrap();
        let second = server.reserve::<String>("player.png").unwrap();

        assert_eq!(first, second);
        assert_eq!(server.path(first.id()), Some("player.png"));
    }

    #[test]
    fn rejects_non_logical_asset_paths() {
        assert_eq!(AssetPath::new(""), Err(AssetPathError::Empty));
        assert_eq!(
            AssetPath::new("/absolute.png"),
            Err(AssetPathError::Absolute)
        );
        assert_eq!(
            AssetPath::new("textures\\player.png"),
            Err(AssetPathError::ContainsBackslash)
        );
        assert_eq!(
            AssetPath::new("textures/../player.png"),
            Err(AssetPathError::ContainsParentDirectory)
        );
        assert_eq!(
            AssetPath::new("textures//player.png"),
            Err(AssetPathError::ContainsEmptySegment)
        );
    }

    #[test]
    fn resolves_and_exports_asset_refs_by_path() {
        let mut server = AssetServer::new();
        let asset_ref = AssetRef::path("textures/player.png").unwrap();

        let handle = asset_ref.resolve::<String>(&mut server).unwrap();

        assert_eq!(server.path(handle.id()), Some("textures/player.png"));
        assert_eq!(
            AssetRef::from_handle(&server, handle).unwrap(),
            AssetRef::path("textures/player.png").unwrap()
        );
    }

    #[test]
    fn stable_asset_ids_are_reserved_for_later() {
        let mut server = AssetServer::new();
        let asset_ref = AssetRef::stable_id("asset-123");

        assert_eq!(
            asset_ref.resolve::<String>(&mut server),
            Err(AssetRefError::UnsupportedStableId("asset-123".to_string()))
        );
    }

    #[test]
    fn stores_typed_assets() {
        let mut assets = Assets::<String>::default();
        let handle = assets.add("texture".to_string());

        assert_eq!(assets.get(handle).map(String::as_str), Some("texture"));
    }
}
