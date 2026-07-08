use std::{
    collections::HashMap,
    fmt::{self, Debug, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
};

use nara_ecs::Resource;

use crate::{
    AssetError, AssetEventKind, AssetEvents, AssetId, AssetServer, AssetStateError, AssetStates,
    AssetVersion, ImportArtifactDigest, SourceHash,
};

pub trait Asset: Send + Sync + 'static {}

impl<T> Asset for T where T: Send + Sync + 'static {}

pub struct Handle<T> {
    id: AssetId,
    marker: PhantomData<fn() -> T>,
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

#[derive(Debug, Resource)]
pub struct Assets<T: Asset> {
    values: HashMap<AssetId, T>,
}

impl<T: Asset> Default for Assets<T> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
}

impl<T: Asset> Assets<T> {
    pub fn add(
        &mut self,
        asset_server: &mut AssetServer,
        value: T,
    ) -> Result<Handle<T>, AssetError> {
        loop {
            let handle = asset_server.reserve_anonymous::<T>()?;
            if let std::collections::hash_map::Entry::Vacant(entry) = self.values.entry(handle.id())
            {
                entry.insert(value);
                return Ok(handle);
            }
        }
    }

    pub fn insert(&mut self, handle: Handle<T>, value: T) -> Option<T> {
        self.values.insert(handle.id(), value)
    }

    pub fn commit_loaded(
        &mut self,
        handle: Handle<T>,
        value: T,
        states: &mut AssetStates,
        events: &mut AssetEvents,
        source_hash: Option<SourceHash>,
        import_hash: Option<ImportArtifactDigest>,
    ) -> Result<AssetVersion, AssetStateError> {
        let id = handle.id();
        let version = states.next_version(id)?;
        let event_kind = if self.values.contains_key(&id) {
            AssetEventKind::Modified
        } else {
            AssetEventKind::Added
        };

        self.values.insert(id, value);
        states.set_loaded_at(id, version, source_hash, import_hash);
        events.push(id, version, event_kind);
        Ok(version)
    }

    pub fn commit_reload(
        &mut self,
        handle: Handle<T>,
        expected_version: AssetVersion,
        value: T,
        states: &mut AssetStates,
        events: &mut AssetEvents,
        source_hash: Option<SourceHash>,
        import_hash: Option<ImportArtifactDigest>,
    ) -> Result<AssetVersion, AssetStateError> {
        states.ensure_version(handle.id(), expected_version)?;
        self.commit_loaded(handle, value, states, events, source_hash, import_hash)
    }

    pub fn record_reload_failure(
        &mut self,
        handle: Handle<T>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
        message: impl Into<String>,
    ) -> Result<AssetVersion, AssetStateError> {
        let version = states.set_failed(handle.id(), message.into())?;
        events.push(handle.id(), version, AssetEventKind::ReloadFailed);
        Ok(version)
    }

    pub fn remove_with_state(
        &mut self,
        handle: Handle<T>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Result<Option<T>, AssetStateError> {
        let id = handle.id();
        let version = states.next_version(id)?;
        let removed = self.values.remove(&id);
        states.set_removed_at(id, version);
        events.push(id, version, AssetEventKind::Removed);
        Ok(removed)
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
