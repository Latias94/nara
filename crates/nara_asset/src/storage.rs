use std::{
    collections::HashMap,
    fmt::{self, Debug, Formatter},
    hash::{Hash, Hasher},
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use nara_ecs::Resource;

use crate::revision::{RevisionCounter, RevisionIdentity};
use crate::{
    AssetError, AssetEventKind, AssetEvents, AssetId, AssetServer, AssetStateError, AssetStates,
    AssetVersion, ImportArtifactDigest, SourceHash,
};

pub trait Asset: Send + Sync + 'static {}

impl<T> Asset for T where T: Send + Sync + 'static {}

/// A typed runtime identity. Source-backed handles come from `AssetServer`.
///
/// ```compile_fail,E0624
/// use nara_asset::{AssetServer, Handle};
/// let handle = AssetServer::new().reserve::<String>("image.png").unwrap();
/// let _ = Handle::<u32>::new(handle.id());
/// ```
pub struct Handle<T> {
    id: AssetId,
    marker: PhantomData<fn() -> T>,
}

/// Allocates a fresh typed runtime identity without a source catalog binding.
///
/// Render domains may retain it for a view or session. Use `AssetServer` for source-backed assets.
pub fn allocate_anonymous_handle<T>() -> Result<Handle<T>, AssetError> {
    AssetId::allocate().map(Handle::new)
}

impl<T> Handle<T> {
    #[must_use]
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
pub struct AssetSlotRevision {
    identity: RevisionIdentity,
    store: Arc<()>,
}

impl AssetSlotRevision {
    fn new(store: &Arc<()>, entry: Option<&RevisionCounter>) -> Self {
        Self {
            identity: RevisionIdentity::capture(store, entry),
            store: Arc::clone(store),
        }
    }

    pub(crate) fn same_store(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.store, &other.store)
    }
}

/// Owned admission for one managed mutation. All outcomes validate the same value/state cut.
pub struct AssetMutationBaseline<T: Asset> {
    handle: Handle<T>,
    slot: AssetSlotRevision,
    state: crate::AssetStateRevision,
}

impl<T: Asset> Clone for AssetMutationBaseline<T> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle,
            slot: self.slot.clone(),
            state: self.state.clone(),
        }
    }
}

impl<T: Asset> Debug for AssetMutationBaseline<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetMutationBaseline")
            .field("handle", &self.handle)
            .finish_non_exhaustive()
    }
}

impl<T: Asset> AssetMutationBaseline<T> {
    pub(crate) const fn handle(&self) -> Handle<T> {
        self.handle
    }

    pub fn validate(
        &self,
        assets: &Assets<T>,
        states: &AssetStates,
    ) -> Result<(), AssetStateError> {
        if states.state_revision(self.handle.id()) != self.state {
            return Err(AssetStateError::MutationSuperseded {
                id: self.handle.id(),
            });
        }
        if assets.slot_revision(self.handle) != self.slot {
            return Err(AssetStateError::SlotSuperseded {
                id: self.handle.id(),
            });
        }
        assets.validate_association(self.handle, states)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn commit_loaded(
        self,
        value: T,
        assets: &mut Assets<T>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
        source_hash: Option<SourceHash>,
        import_hash: Option<ImportArtifactDigest>,
    ) -> Result<AssetVersion, AssetStateError> {
        self.validate(assets, states)?;
        assets.commit_loaded(self.handle, value, states, events, source_hash, import_hash)
    }

    pub fn record_failure(
        self,
        assets: &mut Assets<T>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
        message: impl Into<String>,
    ) -> Result<AssetVersion, AssetStateError> {
        self.validate(assets, states)?;
        if assets.get(self.handle).is_some() {
            assets.record_reload_failure(self.handle, states, events, message)
        } else {
            assets.record_load_failure(self.handle, states, events, message)
        }
    }

    pub fn remove(
        self,
        assets: &mut Assets<T>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
    ) -> Result<Option<T>, AssetStateError> {
        self.validate(assets, states)?;
        assets.remove_with_state(self.handle, states, events)
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
    managed_state_stores: Mutex<HashMap<AssetId, Arc<()>>>,
}

impl<T: Asset> Default for Assets<T> {
    fn default() -> Self {
        Self {
            store_revision: Arc::new(()),
            values: HashMap::new(),
            slot_revisions: HashMap::new(),
            managed_state_stores: Mutex::new(HashMap::new()),
        }
    }
}

impl<T: Asset> Assets<T> {
    /// Captures only an actually associated managed publication or a fresh, empty initial slot.
    pub fn mutation_baseline(
        &self,
        handle: Handle<T>,
        states: &AssetStates,
    ) -> Result<AssetMutationBaseline<T>, AssetStateError> {
        self.validate_association(handle, states)?;
        Ok(AssetMutationBaseline {
            handle,
            slot: self.slot_revision(handle),
            state: states.state_revision(handle.id()),
        })
    }

    fn validate_association(
        &self,
        handle: Handle<T>,
        states: &AssetStates,
    ) -> Result<(), AssetStateError> {
        self.admit_association(handle, states, false)
    }

    fn admit_association(
        &self,
        handle: Handle<T>,
        states: &AssetStates,
        owner_replacement: bool,
    ) -> Result<(), AssetStateError> {
        let mut stores = self
            .managed_state_stores
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let state_store = states.store_identity();
        if stores
            .get(&handle.id())
            .is_some_and(|expected| !Arc::ptr_eq(expected, &state_store))
        {
            return Err(AssetStateError::StorageMismatch { id: handle.id() });
        }
        let slot = self.slot_revision(handle);
        let fresh = (owner_replacement || self.get(handle).is_none())
            && states.version(handle.id()).unwrap_or(AssetVersion::ZERO) == AssetVersion::ZERO
            && states
                .state(handle.id())
                .is_none_or(|state| state.usable_version().is_none());
        let valid = states.admit_managed_slot(handle.id(), slot, fresh, owner_replacement, || {
            stores.entry(handle.id()).or_insert(state_store);
        });
        if valid {
            Ok(())
        } else {
            Err(AssetStateError::StorageMismatch { id: handle.id() })
        }
    }

    fn bind_state_store(&mut self, handle: Handle<T>, states: &AssetStates) {
        self.managed_state_stores
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .insert(handle.id(), states.store_identity());
    }
    pub fn add(
        &mut self,
        asset_server: &mut AssetServer,
        value: T,
    ) -> Result<Handle<T>, AssetError> {
        let handle = asset_server.reserve_anonymous::<T>()?;
        self.insert(handle, value);
        Ok(handle)
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
        self.admit_association(handle, states, true)?;
        let event_kind = if self.insert(handle, value).is_some() {
            AssetEventKind::Modified
        } else {
            AssetEventKind::Added
        };
        states.set_loaded_at(id, version, source_hash, import_hash);
        states.bind_published_slot(id, self.slot_revision(handle));
        self.bind_state_store(handle, states);
        let _ = events.push(id, version, event_kind);
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
        if states.state(handle.id()).is_none() {
            return Err(AssetStateError::UnknownAsset { id: handle.id() });
        }
        self.validate_association(handle, states)?;
        let version = states.set_failed(handle.id(), message.into())?;
        let _ = events.push(handle.id(), version, AssetEventKind::ReloadFailed);
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
        if states.state(id).is_none() {
            return Err(AssetStateError::UnknownAsset { id });
        }
        self.validate_association(handle, states)?;
        let version = states.set_failed(id, message.into())?;
        states.invalidate_publication(id);
        self.remove(handle);
        states.bind_managed_slot(id, self.slot_revision(handle));
        self.bind_state_store(handle, states);
        let _ = events.push(id, version, AssetEventKind::LoadFailed);
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
        self.admit_association(handle, states, true)?;
        let removed = self.remove(handle);
        states.set_removed_at(id, version);
        states.bind_managed_slot(id, self.slot_revision(handle));
        self.bind_state_store(handle, states);
        let _ = events.push(id, version, AssetEventKind::Removed);
        Ok(removed)
    }

    #[must_use]
    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        self.values.get(&handle.id())
    }

    #[must_use]
    pub fn contains_id(&self, id: AssetId) -> bool {
        self.values.contains_key(&id)
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
    fn baseline_capture_rejects_mismatched_stores_before_observing_unchanged_slots() {
        let handle = Handle::<String>::new(AssetId::from_raw(50));
        let mut assets = Assets::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        assets
            .commit_loaded(
                handle,
                "last-good".into(),
                &mut states,
                &mut events,
                None,
                None,
            )
            .unwrap();
        events.drain();
        let other_assets = Assets::default();
        let other_states = AssetStates::default();
        assert!(matches!(
            other_assets.mutation_baseline(handle, &states),
            Err(AssetStateError::StorageMismatch { .. })
        ));
        assert!(matches!(
            assets.mutation_baseline(handle, &other_states),
            Err(AssetStateError::StorageMismatch { .. })
        ));
        assert_eq!(assets.get(handle).unwrap(), "last-good");
        assert!(events.drain().is_empty());
    }

    #[test]
    fn initial_association_survives_empty_value_and_state_resource_replacement() {
        let handle = Handle::<String>::new(AssetId::from_raw(51));
        let assets = Assets::default();
        let states = AssetStates::default();
        let _baseline = assets.mutation_baseline(handle, &states).unwrap();
        assert!(
            Assets::default()
                .mutation_baseline(handle, &states)
                .is_err()
        );
        assert!(
            assets
                .mutation_baseline(handle, &AssetStates::default())
                .is_err()
        );
    }

    #[test]
    fn rejected_association_never_claims_either_previously_unpaired_store() {
        let handle = Handle::<String>::new(AssetId::from_raw(54));
        let first = Assets::default();
        let first_states = AssetStates::default();
        first.mutation_baseline(handle, &first_states).unwrap();
        let second = Assets::default();
        let second_states = AssetStates::default();
        assert!(second.mutation_baseline(handle, &first_states).is_err());
        assert!(first.mutation_baseline(handle, &second_states).is_err());
        assert!(second.managed_state_stores.lock().unwrap().is_empty());
        assert!(second_states.managed_slots_empty_for_tests());
        second.mutation_baseline(handle, &second_states).unwrap();
        assert!(first.mutation_baseline(handle, &first_states).is_ok());
        assert!(second.mutation_baseline(handle, &second_states).is_ok());
    }

    #[test]
    fn common_baseline_rejects_late_success_failure_and_removal_without_effects() {
        for outcome in 0..3 {
            let handle = Handle::<String>::new(AssetId::from_raw(52));
            let mut assets = Assets::default();
            let mut states = AssetStates::default();
            let mut events = AssetEvents::default();
            assets
                .commit_loaded(handle, "first".into(), &mut states, &mut events, None, None)
                .unwrap();
            let baseline = assets.mutation_baseline(handle, &states).unwrap();
            assets
                .commit_loaded(
                    handle,
                    "successor".into(),
                    &mut states,
                    &mut events,
                    None,
                    None,
                )
                .unwrap();
            events.drain();
            let revision = states.state_revision(handle.id());
            let result = match outcome {
                0 => baseline
                    .commit_loaded(
                        "late".into(),
                        &mut assets,
                        &mut states,
                        &mut events,
                        None,
                        None,
                    )
                    .map(|_| ()),
                1 => baseline
                    .record_failure(&mut assets, &mut states, &mut events, "late failure")
                    .map(|_| ()),
                _ => baseline
                    .remove(&mut assets, &mut states, &mut events)
                    .map(|_| ()),
            };
            assert!(result.is_err());
            assert_eq!(assets.get(handle).unwrap(), "successor");
            assert_eq!(states.state_revision(handle.id()), revision);
            assert!(events.drain().is_empty());
        }
    }

    #[test]
    fn baseline_rejects_state_replacement_and_empty_slot_aba() {
        let handle = Handle::<String>::new(AssetId::from_raw(53));
        let mut assets = Assets::default();
        let states = AssetStates::default();
        let baseline = assets.mutation_baseline(handle, &states).unwrap();
        assert!(baseline.validate(&assets, &AssetStates::default()).is_err());
        assets.insert(handle, "transient".into());
        assets.remove(handle);
        assert!(baseline.validate(&assets, &states).is_err());
        assert!(assets.mutation_baseline(handle, &states).is_err());
    }

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
