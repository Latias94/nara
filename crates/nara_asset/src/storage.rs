use std::{
    collections::HashMap,
    fmt::{self, Debug, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
    sync::Arc,
};

use nara_ecs::Resource;

use crate::revision::{RevisionCounter, RevisionIdentity};
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

/// Opaque identity of one typed asset store and the latest mutation to one slot.
///
/// The identity changes for insertion, mutable access, replacement, and removal. Removal retains a
/// tombstone identity so an empty-slot ABA cannot revive an older publication admission. A never-
/// populated slot still carries the store identity, preventing publication into another newly
/// constructed store whose slot is also absent.
#[derive(Clone, PartialEq, Eq)]
pub struct AssetSlotRevision(RevisionIdentity);

impl AssetSlotRevision {
    fn new(store: &Arc<()>, entry: Option<&RevisionCounter>) -> Self {
        Self(RevisionIdentity::capture(store, entry))
    }
}

impl Debug for AssetSlotRevision {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetSlotRevision")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Resource)]
pub struct Assets<T: Asset> {
    store_revision: Arc<()>,
    values: HashMap<AssetId, T>,
    slot_revisions: HashMap<AssetId, RevisionCounter>,
}

impl<T: Asset> Default for Assets<T> {
    fn default() -> Self {
        Self {
            store_revision: Arc::new(()),
            values: HashMap::new(),
            slot_revisions: HashMap::new(),
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
                self.touch_slot(handle.id());
                return Ok(handle);
            }
        }
    }

    pub fn insert(&mut self, handle: Handle<T>, value: T) -> Option<T> {
        self.touch_slot(handle.id());
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
        let event_kind = if self.insert(handle, value).is_some() {
            AssetEventKind::Modified
        } else {
            AssetEventKind::Added
        };
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

    pub fn record_load_failure(
        &mut self,
        handle: Handle<T>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
        message: impl Into<String>,
    ) -> Result<AssetVersion, AssetStateError> {
        let id = handle.id();
        let version = states.set_failed(id, message.into())?;
        self.remove(handle);
        events.push(id, version, AssetEventKind::LoadFailed);
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
        let removed = self.remove(handle);
        states.set_removed_at(id, version);
        events.push(id, version, AssetEventKind::Removed);
        Ok(removed)
    }

    #[must_use]
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        self.values.get(&handle.id())
    }

    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        let value = self.values.get_mut(&handle.id())?;
        self.slot_revisions
            .entry(handle.id())
            .and_modify(RevisionCounter::advance)
            .or_default();
        Some(value)
    }

    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let removed = self.values.remove(&handle.id());
        if removed.is_some() {
            self.touch_slot(handle.id());
        }
        removed
    }

    fn touch_slot(&mut self, id: AssetId) {
        self.slot_revisions
            .entry(id)
            .and_modify(RevisionCounter::advance)
            .or_default();
    }

    /// Returns this store's identity plus the latest slot mutation identity.
    #[must_use]
    pub fn slot_revision(&self, handle: Handle<T>) -> AssetSlotRevision {
        AssetSlotRevision::new(&self.store_revision, self.slot_revisions.get(&handle.id()))
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
    fn slot_revision_changes_for_every_mutable_value_path_and_survives_removal() {
        let mut server = AssetServer::new();
        let mut assets = Assets::default();
        let handle = assets.add(&mut server, String::from("first")).unwrap();
        let added = assets.slot_revision(handle);

        assets.get_mut(handle).unwrap().push_str("-changed");
        let mutably_borrowed = assets.slot_revision(handle);
        assert_ne!(mutably_borrowed, added);

        assets.insert(handle, String::from("replacement"));
        let replaced = assets.slot_revision(handle);
        assert_ne!(replaced, mutably_borrowed);

        assert_eq!(assets.remove(handle).as_deref(), Some("replacement"));
        let removed = assets.slot_revision(handle);
        assert_ne!(removed, replaced);

        assets.insert(handle, String::from("reinserted"));
        assert_ne!(assets.slot_revision(handle), removed);
    }

    #[test]
    fn absent_slot_revisions_bind_candidates_to_one_store() {
        let handle = Handle::<String>::new(AssetId::from_raw(7));
        let first = Assets::<String>::default();
        let second = Assets::<String>::default();

        assert_ne!(first.slot_revision(handle), second.slot_revision(handle));
    }

    #[test]
    fn failed_load_failure_transition_preserves_the_existing_slot() {
        let mut server = AssetServer::new();
        let mut assets = Assets::default();
        let handle = assets.add(&mut server, String::from("last-good")).unwrap();
        let revision = assets.slot_revision(handle);
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();

        assert!(matches!(
            assets.record_load_failure(
                handle,
                &mut states,
                &mut events,
                "image.import-test-failure",
            ),
            Err(AssetStateError::UnknownAsset { .. })
        ));
        assert_eq!(assets.get(handle).map(String::as_str), Some("last-good"));
        assert_eq!(assets.slot_revision(handle), revision);
        assert!(events.drain().is_empty());
    }
}
