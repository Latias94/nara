use std::{
    collections::{HashMap, hash_map::Entry},
    fmt::{self, Debug, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
    sync::Arc,
};

use crate::revision::{RevisionCounter, RevisionIdentity};
use crate::{
    AssetError, AssetEventKind, AssetEvents, AssetId, AssetServer, AssetStateError, AssetStates,
    AssetVersion, LoadState,
};

pub trait Asset: Send + Sync + 'static {}

impl<T: Send + Sync + 'static> Asset for T {}

/// A typed runtime identity. The asset store owns the value.
pub struct Handle<T> {
    id: AssetId,
    marker: PhantomData<fn() -> T>,
}

/// Allocates a typed identity without registering it with an asset server.
pub fn allocate_anonymous_handle<T>() -> Result<Handle<T>, AssetError> {
    AssetId::allocate().map(Handle::new)
}

impl<T> Handle<T> {
    pub(crate) const fn new(id: AssetId) -> Self {
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
        formatter.debug_tuple("Handle").field(&self.id).finish()
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

/// Identity of a stored value revision, used by renderer caches.
#[derive(Clone, PartialEq, Eq)]
pub struct AssetSlotRevision(RevisionIdentity);

impl Debug for AssetSlotRevision {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetSlotRevision")
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Slot<T> {
    value: T,
    revision: RevisionCounter,
}

/// In-memory values for a single asset type.
#[derive(Debug)]
#[cfg_attr(feature = "runtime", derive(nara_ecs::Resource))]
pub struct Assets<T: Asset> {
    values: HashMap<AssetId, Slot<T>>,
    identity: Arc<()>,
}

impl<T: Asset> Default for Assets<T> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            identity: Arc::new(()),
        }
    }
}

impl<T: Asset> Assets<T> {
    pub fn add(&mut self, server: &mut AssetServer, value: T) -> Result<Handle<T>, AssetError> {
        let handle = server.reserve_anonymous()?;
        self.insert(handle, value);
        Ok(handle)
    }

    pub fn insert(&mut self, handle: Handle<T>, value: T) -> Option<T> {
        match self.values.entry(handle.id()) {
            Entry::Occupied(mut entry) => {
                let slot = entry.get_mut();
                slot.revision.advance();
                Some(std::mem::replace(&mut slot.value, value))
            }
            Entry::Vacant(entry) => {
                entry.insert(Slot {
                    value,
                    revision: RevisionCounter::default(),
                });
                None
            }
        }
    }

    /// Stores a loaded value and advances its public version.
    pub fn commit_loaded(
        &mut self,
        handle: Handle<T>,
        value: T,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Result<AssetVersion, AssetStateError> {
        let id = handle.id();
        let version = states.next_version(id)?;
        let kind = if self.insert(handle, value).is_some() {
            AssetEventKind::Modified
        } else {
            AssetEventKind::Added
        };
        states.set_loaded(id, version, self.slot_revision(handle));
        events.push(id, version, kind);
        Ok(version)
    }

    /// Records a load error while keeping the last loaded value available.
    pub fn record_reload_failure(
        &mut self,
        handle: Handle<T>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
        message: impl Into<String>,
    ) -> Result<AssetVersion, AssetStateError> {
        let id = handle.id();
        if self.get(handle).is_none() {
            return Err(AssetStateError::UnknownAsset { id });
        }
        let message = message.into();
        let reported = LoadState::Failed { message };
        let state = states
            .state_mut(id)
            .ok_or(AssetStateError::UnknownAsset { id })?;
        if state.load_state() != &reported {
            state.set_load_state(reported);
            events.push(id, state.version(), AssetEventKind::ReloadFailed);
        }
        Ok(state.version())
    }

    pub fn remove_with_state(
        &mut self,
        handle: Handle<T>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Result<Option<T>, AssetStateError> {
        let id = handle.id();
        let version = states.next_version(id)?;
        let removed = self.remove(handle);
        states.set_removed(id, version);
        events.push(id, version, AssetEventKind::Removed);
        Ok(removed)
    }

    #[must_use]
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        self.values.get(&handle.id()).map(|slot| &slot.value)
    }

    #[must_use]
    pub fn contains_id(&self, id: AssetId) -> bool {
        self.values.contains_key(&id)
    }

    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        let slot = self.values.get_mut(&handle.id())?;
        slot.revision.advance();
        Some(&mut slot.value)
    }

    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        self.values.remove(&handle.id()).map(|slot| slot.value)
    }

    /// Returns the latest revision at a slot. Re-insertion creates a fresh revision.
    #[must_use]
    pub fn slot_revision(&self, handle: Handle<T>) -> AssetSlotRevision {
        AssetSlotRevision(RevisionIdentity::capture(
            &self.identity,
            self.values.get(&handle.id()).map(|slot| &slot.revision),
        ))
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (Handle<T>, &T)> {
        self.values
            .iter()
            .map(|(id, slot)| (Handle::new(*id), &slot.value))
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
