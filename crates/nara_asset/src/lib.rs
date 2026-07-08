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
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
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

#[cfg(feature = "serde")]
impl<T> serde::Serialize for Handle<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.id.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de, T> serde::Deserialize<'de> for Handle<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::new(AssetId::deserialize(deserializer)?))
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
}

impl Display for AssetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdSpaceExhausted => formatter.write_str("asset id space exhausted"),
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
        let path = path.into();
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
    fn stores_typed_assets() {
        let mut assets = Assets::<String>::default();
        let handle = assets.add("texture".to_string());

        assert_eq!(assets.get(handle).map(String::as_str), Some("texture"));
    }
}
