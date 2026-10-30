use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
    sync::{Arc, Mutex},
};

use nara_core::ItemLimit;
use nara_ecs::Resource;

use crate::revision::{RevisionCounter, RevisionIdentity};
use crate::{
    Asset, AssetId, AssetMutationBaseline, AssetSlotRevision, Assets, ImportArtifactDigest,
    SourceHash, StableAssetId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AssetVersion(u64);

impl AssetVersion {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl Display for AssetVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

/// Opaque identity of one asset-state store and the latest write to one entry.
///
/// Unlike `AssetVersion`, this changes for same-version transitions such as Loading to Failed. An
/// absent entry still carries the store identity, so a candidate captured from one runtime cannot
/// be published into a newly constructed state store whose entry is also absent.
#[derive(Clone, PartialEq, Eq)]
pub struct AssetStateRevision(RevisionIdentity);

impl AssetStateRevision {
    fn new(store: &Arc<()>, entry: Option<&RevisionCounter>) -> Self {
        Self(RevisionIdentity::capture(store, entry))
    }
}

impl fmt::Debug for AssetStateRevision {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetStateRevision")
            .finish_non_exhaustive()
    }
}

/// Opaque publication authority retained across replacement and revoked by removal or invalidation.
#[derive(Clone, PartialEq, Eq)]
pub struct AssetPublicationRevision(RevisionIdentity);

impl fmt::Debug for AssetPublicationRevision {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AssetPublicationRevision")
            .finish_non_exhaustive()
    }
}

/// Opaque state-owner continuity for Image's pixel-free removal recovery.
///
/// Only the registered Image reload consumer issues these tokens. Resolver-admitted Loading,
/// resolver capacity rejection, and an exactly validated Image failure preserve continuity.
/// Ordinary state writes revoke it independently of the asset's version and state revision.
/// Image must also validate its catalog, value store, and empty-slot recovery association.
#[derive(Clone)]
pub struct ImageRecoveryContinuity {
    id: AssetId,
    revision: RevisionIdentity,
    authority: Arc<()>,
}

impl PartialEq for ImageRecoveryContinuity {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.revision == other.revision
            && Arc::ptr_eq(&self.authority, &other.authority)
    }
}

impl Eq for ImageRecoveryContinuity {}

impl fmt::Debug for ImageRecoveryContinuity {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageRecoveryContinuity")
            .finish_non_exhaustive()
    }
}

impl ImageRecoveryContinuity {
    #[must_use]
    pub fn is_current(&self, states: &AssetStates, id: AssetId) -> bool {
        self.id == id
            && states
                .image_reload_authority
                .as_ref()
                .is_some_and(|authority| Arc::ptr_eq(authority, &self.authority))
            && self.revision == states.image_recovery_revision(id)
    }

    /// Records the existing failure transition without revoking current recovery continuity.
    ///
    /// Both this token and the exact per-attempt mutation baseline must still be current.
    /// Successful failure recording advances the ordinary state revision and retains its events.
    pub fn record_failure<T: Asset>(
        &self,
        baseline: AssetMutationBaseline<T>,
        assets: &mut Assets<T>,
        states: &mut AssetStates,
        events: &mut AssetEvents,
        message: impl Into<String>,
    ) -> Result<AssetVersion, AssetStateError> {
        let id = baseline.handle().id();
        if !self.is_current(states, id) {
            return Err(AssetStateError::MutationSuperseded { id });
        }
        baseline.validate(assets, states)?;
        let current = states.take_image_recovery_revision(id);
        let result = baseline.record_failure(assets, states, events, message);
        if result.is_ok() {
            states.restore_image_recovery_revision(id, current);
        }
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LoadState {
    NotLoaded,
    Loading,
    Loaded,
    Failed { message: String },
    Removed,
}

impl Default for LoadState {
    fn default() -> Self {
        Self::NotLoaded
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AssetState {
    version: AssetVersion,
    load_state: LoadState,
    source_hash: Option<SourceHash>,
    import_hash: Option<ImportArtifactDigest>,
    published_available: bool,
}

impl AssetState {
    #[must_use]
    pub const fn new(
        version: AssetVersion,
        load_state: LoadState,
        source_hash: Option<SourceHash>,
        import_hash: Option<ImportArtifactDigest>,
    ) -> Self {
        let published_available = matches!(load_state, LoadState::Loaded);
        Self {
            version,
            load_state,
            source_hash,
            import_hash,
            published_available,
        }
    }

    #[must_use]
    pub const fn version(&self) -> AssetVersion {
        self.version
    }

    /// Returns the usable published version independently of the latest load attempt.
    #[must_use]
    pub const fn usable_version(&self) -> Option<AssetVersion> {
        if self.published_available {
            Some(self.version)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn load_state(&self) -> &LoadState {
        &self.load_state
    }

    #[must_use]
    pub const fn source_hash(&self) -> Option<SourceHash> {
        self.source_hash
    }

    #[must_use]
    pub const fn import_hash(&self) -> Option<ImportArtifactDigest> {
        self.import_hash
    }
}

impl Default for AssetState {
    fn default() -> Self {
        Self::new(AssetVersion::ZERO, LoadState::NotLoaded, None, None)
    }
}

#[derive(Debug, Resource)]
pub struct AssetStates {
    store_revision: Arc<()>,
    image_reload_authority: Option<Arc<()>>,
    states: BTreeMap<AssetId, AssetState>,
    state_revisions: BTreeMap<AssetId, RevisionCounter>,
    publication_revisions: BTreeMap<AssetId, RevisionCounter>,
    image_recovery_revisions: BTreeMap<AssetId, RevisionCounter>,
    published_slots: BTreeMap<AssetId, AssetSlotRevision>,
    managed_slots: Mutex<BTreeMap<AssetId, AssetSlotRevision>>,
    metadata_dirty: BTreeMap<AssetId, AssetStateRevision>,
    removed_sources: BTreeMap<AssetId, Option<AssetStateRevision>>,
}

impl Default for AssetStates {
    fn default() -> Self {
        Self {
            store_revision: Arc::new(()),
            image_reload_authority: None,
            states: BTreeMap::new(),
            state_revisions: BTreeMap::new(),
            publication_revisions: BTreeMap::new(),
            image_recovery_revisions: BTreeMap::new(),
            published_slots: BTreeMap::new(),
            managed_slots: Mutex::new(BTreeMap::new()),
            metadata_dirty: BTreeMap::new(),
            removed_sources: BTreeMap::new(),
        }
    }
}

impl AssetStates {
    #[cfg(test)]
    pub(crate) fn managed_slots_empty_for_tests(&self) -> bool {
        self.managed_slots.lock().unwrap().is_empty()
    }
    pub(crate) fn store_identity(&self) -> Arc<()> {
        Arc::clone(&self.store_revision)
    }
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn state(&self, id: AssetId) -> Option<&AssetState> {
        self.states.get(&id)
    }

    /// Retains an unresolved metadata observation independently of queue or task admission.
    pub fn observe_metadata_change(&mut self, id: AssetId) {
        self.advance_state_revision(id);
        self.metadata_dirty.insert(id, self.state_revision(id));
    }

    #[must_use]
    pub fn metadata_change_pending(&self, id: AssetId) -> bool {
        self.metadata_dirty.contains_key(&id)
    }

    /// Returns the state revision captured at the latest unresolved metadata observation.
    ///
    /// Loading and failure transitions preserve this observation. A new observation invalidates
    /// previously prepared mutations even when no reload request can be admitted.
    #[must_use]
    pub fn metadata_change_revision(&self, id: AssetId) -> Option<&AssetStateRevision> {
        self.metadata_dirty.get(&id)
    }

    /// Whether an observed source removal still requires explicit source re-admission.
    #[must_use]
    pub fn source_removal_pending(&self, id: AssetId) -> bool {
        self.removed_sources.contains_key(&id)
    }

    /// Whether unresolved metadata was observed after the latest source removal.
    ///
    /// This permits a concrete owner to revalidate recreated source metadata; the observation
    /// alone does not establish matching identity or authorize publication.
    #[must_use]
    pub fn metadata_change_after_source_removal(&self, id: AssetId) -> bool {
        self.removed_sources.get(&id).is_some_and(|at_removal| {
            self.metadata_change_revision(id)
                .is_some_and(|current| Some(current) != at_removal.as_ref())
        })
    }

    pub(crate) fn observe_source_removal(&mut self, id: AssetId) {
        self.removed_sources
            .insert(id, self.metadata_change_revision(id).cloned());
        self.published_slots.remove(&id);
        if let Some(current) = self.states.get_mut(&id) {
            current.published_available = false;
        }
        // Source retirement is an observation even if no usable publication exists yet.
        self.advance_state_revision(id);
        self.advance_publication_revision(id);
    }

    pub(crate) fn observe_image_source_removal(&mut self, id: AssetId) {
        let current = self.take_image_recovery_revision(id);
        self.observe_source_removal(id);
        self.restore_image_recovery_revision(id, current);
    }

    /// A concrete owner calls this only after publishing the current desired recipe.
    pub fn acknowledge_metadata_publication(&mut self, id: AssetId) {
        if self.metadata_dirty.remove(&id).is_some() {
            self.advance_state_revision(id);
        }
    }

    /// Iterates every authoritative runtime asset state, including removed and failed assets.
    ///
    /// Observers use this view to rebuild after [`AssetEvents`] reports an observation gap.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (AssetId, &AssetState)> {
        self.states.iter().map(|(&id, state)| (id, state))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.states.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Returns this store's identity plus the latest runtime-state write identity for this asset.
    #[must_use]
    pub fn state_revision(&self, id: AssetId) -> AssetStateRevision {
        AssetStateRevision::new(&self.store_revision, self.state_revisions.get(&id))
    }

    pub(crate) fn bind_image_reload_authority(&mut self, authority: &Arc<()>) {
        if self
            .image_reload_authority
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, authority))
        {
            return;
        }
        for revision in self.image_recovery_revisions.values_mut() {
            revision.advance();
        }
        self.image_reload_authority = Some(Arc::clone(authority));
    }

    pub(crate) fn image_recovery_continuity(
        &self,
        id: AssetId,
        authority: Arc<()>,
    ) -> ImageRecoveryContinuity {
        ImageRecoveryContinuity {
            id,
            revision: self.image_recovery_revision(id),
            authority,
        }
    }

    fn image_recovery_revision(&self, id: AssetId) -> RevisionIdentity {
        RevisionIdentity::capture(&self.store_revision, self.image_recovery_revisions.get(&id))
    }

    /// Returns authority that an immutable last-good value may retain until explicitly revoked.
    #[must_use]
    pub fn publication_revision(&self, id: AssetId) -> AssetPublicationRevision {
        AssetPublicationRevision(RevisionIdentity::capture(
            &self.store_revision,
            self.publication_revisions.get(&id),
        ))
    }

    /// Returns the exact storage slot written by the latest managed publication.
    #[must_use]
    pub fn published_slot_revision(&self, id: AssetId) -> Option<&AssetSlotRevision> {
        self.published_slots.get(&id)
    }

    pub(crate) fn bind_published_slot(&mut self, id: AssetId, slot: AssetSlotRevision) {
        self.bind_managed_slot(id, slot.clone());
        self.published_slots.insert(id, slot);
    }

    pub(crate) fn admit_managed_slot(
        &self,
        id: AssetId,
        slot: AssetSlotRevision,
        fresh: bool,
        owner_replacement: bool,
        publish_reverse: impl FnOnce(),
    ) -> bool {
        let mut slots = self
            .managed_slots
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let valid = match slots.get(&id) {
            Some(expected) => {
                expected == &slot || (owner_replacement && expected.same_store(&slot))
            }
            None if fresh => true,
            None => false,
        };
        if valid {
            // Both association locks stay held until both sides publish. Rejection writes neither.
            publish_reverse();
            slots.entry(id).or_insert(slot);
        }
        valid
    }

    pub(crate) fn validates_managed_slot(
        &self,
        id: AssetId,
        slot: &AssetSlotRevision,
        fresh: bool,
    ) -> bool {
        let slots = self
            .managed_slots
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        match slots.get(&id) {
            Some(expected) => expected == slot,
            None => fresh,
        }
    }

    pub(crate) fn bind_managed_slot(&mut self, id: AssetId, slot: AssetSlotRevision) {
        self.managed_slots
            .get_mut()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id, slot);
    }

    #[must_use]
    pub fn version(&self, id: AssetId) -> Option<AssetVersion> {
        self.state(id).map(AssetState::version)
    }

    pub fn next_version(&self, id: AssetId) -> Result<AssetVersion, AssetStateError> {
        let current = self.version(id).unwrap_or(AssetVersion::ZERO);
        current
            .raw()
            .checked_add(1)
            .map(AssetVersion::from_raw)
            .ok_or(AssetStateError::VersionExhausted { id, current })
    }

    pub fn ensure_version(
        &self,
        id: AssetId,
        expected: AssetVersion,
    ) -> Result<(), AssetStateError> {
        let current = self
            .version(id)
            .ok_or(AssetStateError::UnknownAsset { id })?;
        if current != expected {
            return Err(AssetStateError::StaleReload {
                id,
                expected,
                current,
            });
        }
        Ok(())
    }

    pub(crate) fn set_loaded_at(
        &mut self,
        id: AssetId,
        version: AssetVersion,
        source_hash: Option<SourceHash>,
        import_hash: Option<ImportArtifactDigest>,
    ) {
        self.removed_sources.remove(&id);
        self.published_slots.remove(&id);
        self.insert_state(
            id,
            AssetState::new(version, LoadState::Loaded, source_hash, import_hash),
        );
    }

    pub(crate) fn set_removed_at(&mut self, id: AssetId, version: AssetVersion) {
        self.published_slots.remove(&id);
        self.advance_publication_revision(id);
        self.insert_state(id, AssetState::new(version, LoadState::Removed, None, None));
    }

    pub fn set_loading(&mut self, id: AssetId) -> AssetVersion {
        let mut current = self.state(id).cloned().unwrap_or_default();
        let version = current.version();
        current.load_state = LoadState::Loading;
        self.insert_state(id, current);
        version
    }

    pub(crate) fn set_loading_for_image_reload(&mut self, id: AssetId) -> AssetVersion {
        let current = self.take_image_recovery_revision(id);
        let version = self.set_loading(id);
        self.restore_image_recovery_revision(id, current);
        version
    }

    pub(crate) fn set_failed(
        &mut self,
        id: AssetId,
        message: String,
    ) -> Result<AssetVersion, AssetStateError> {
        let mut current = self
            .state(id)
            .cloned()
            .ok_or(AssetStateError::UnknownAsset { id })?;
        let version = current.version();
        current.load_state = LoadState::Failed { message };
        self.insert_state(id, current);
        Ok(version)
    }

    pub(crate) fn reject_reload_request(
        &mut self,
        id: AssetId,
        message: &'static str,
    ) -> AssetVersion {
        let current = self.states.entry(id).or_default();
        let version = current.version();
        current.load_state = LoadState::Failed {
            message: message.to_owned(),
        };
        self.advance_state_revision(id);
        version
    }

    pub(crate) fn reject_image_reload_capacity(
        &mut self,
        id: AssetId,
        message: &'static str,
    ) -> AssetVersion {
        let current = self.take_image_recovery_revision(id);
        let version = self.reject_reload_request(id, message);
        self.restore_image_recovery_revision(id, current);
        version
    }

    /// Revokes a publication whose identity or authority is no longer valid.
    ///
    /// Retained CPU data and the latest diagnostic remain available for inspection, but cannot
    /// authorize derived resources. A subsequent successful publication restores availability.
    pub fn invalidate_publication(&mut self, id: AssetId) {
        self.advance_state_revision(id);
        self.published_slots.remove(&id);
        let Some(current) = self.states.get_mut(&id) else {
            return;
        };
        if current.published_available {
            current.published_available = false;
            self.advance_publication_revision(id);
        }
    }

    fn insert_state(&mut self, id: AssetId, state: AssetState) {
        self.states.insert(id, state);
        self.advance_state_revision(id);
    }

    fn advance_state_revision(&mut self, id: AssetId) {
        self.state_revisions
            .entry(id)
            .and_modify(RevisionCounter::advance)
            .or_default();
        self.advance_image_recovery_revision(id);
    }

    fn advance_image_recovery_revision(&mut self, id: AssetId) {
        self.image_recovery_revisions
            .entry(id)
            .and_modify(RevisionCounter::advance)
            .or_default();
    }

    fn take_image_recovery_revision(&mut self, id: AssetId) -> Option<RevisionCounter> {
        // Keep a distinct current identity until the known synchronous transition succeeds.
        self.image_recovery_revisions
            .insert(id, RevisionCounter::default())
    }

    fn restore_image_recovery_revision(&mut self, id: AssetId, current: Option<RevisionCounter>) {
        match current {
            Some(current) => {
                self.image_recovery_revisions.insert(id, current);
            }
            None => {
                self.image_recovery_revisions.remove(&id);
            }
        }
    }

    fn advance_publication_revision(&mut self, id: AssetId) {
        self.publication_revisions
            .entry(id)
            .and_modify(RevisionCounter::advance)
            .or_default();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetStateError {
    StorageMismatch {
        id: AssetId,
    },
    SlotSuperseded {
        id: AssetId,
    },
    MutationSuperseded {
        id: AssetId,
    },
    UnknownAsset {
        id: AssetId,
    },
    VersionExhausted {
        id: AssetId,
        current: AssetVersion,
    },
    StaleReload {
        id: AssetId,
        expected: AssetVersion,
        current: AssetVersion,
    },
}

impl Display for AssetStateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::StorageMismatch { id } => write!(
                formatter,
                "asset {id:?} value/state stores are not associated"
            ),
            Self::SlotSuperseded { id } => {
                write!(formatter, "asset {id:?} value slot was superseded")
            }
            Self::MutationSuperseded { id } => {
                write!(formatter, "asset {id:?} managed mutation was superseded")
            }
            Self::UnknownAsset { id } => write!(formatter, "asset {:?} has no state", id),
            Self::VersionExhausted { id, current } => write!(
                formatter,
                "asset {:?} version cannot advance past {current}",
                id
            ),
            Self::StaleReload {
                id,
                expected,
                current,
            } => write!(
                formatter,
                "asset {:?} reload expected version {expected}, but current version is {current}",
                id
            ),
        }
    }
}

impl Error for AssetStateError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetEventKind {
    Added,
    Modified,
    Removed,
    LoadFailed,
    ReloadFailed,
    ReloadRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetEvent {
    id: AssetId,
    version: AssetVersion,
    kind: AssetEventKind,
}

impl AssetEvent {
    #[must_use]
    pub const fn new(id: AssetId, version: AssetVersion, kind: AssetEventKind) -> Self {
        Self { id, version, kind }
    }

    #[must_use]
    pub const fn id(self) -> AssetId {
        self.id
    }

    #[must_use]
    pub const fn version(self) -> AssetVersion {
        self.version
    }

    #[must_use]
    pub const fn kind(self) -> AssetEventKind {
        self.kind
    }
}

const DEFAULT_MAX_ASSET_EVENTS: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetEventLimits {
    items: ItemLimit,
}

impl AssetEventLimits {
    #[must_use]
    pub const fn new(items: ItemLimit) -> Self {
        Self { items }
    }

    #[must_use]
    pub const fn items(self) -> ItemLimit {
        self.items
    }
}

impl Default for AssetEventLimits {
    fn default() -> Self {
        Self::new(
            ItemLimit::new(DEFAULT_MAX_ASSET_EVENTS)
                .expect("default asset event limit is non-zero"),
        )
    }
}

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetEventPushOutcome {
    Accepted,
    RescanRequired,
}

/// Bounded retained asset observations.
///
/// Asset publication/removal/reload paths are producers and tooling or domain observers explicitly
/// drain admitted events. If state transitions exceed the item ceiling, later events are
/// suppressed and `requires_rescan` remains sticky until the owner completes a full `AssetStates`
/// rescan and calls `complete_rescan`. The queue is observational runtime state, not replay input.
#[derive(Debug, Resource)]
pub struct AssetEvents {
    limits: AssetEventLimits,
    discarded_events: u64,
    events: Vec<AssetEvent>,
}

impl Default for AssetEvents {
    fn default() -> Self {
        Self::with_limits(AssetEventLimits::default())
    }
}

impl AssetEvents {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn with_limits(limits: AssetEventLimits) -> Self {
        Self {
            limits,
            discarded_events: 0,
            events: Vec::new(),
        }
    }

    pub fn push(
        &mut self,
        id: AssetId,
        version: AssetVersion,
        kind: AssetEventKind,
    ) -> AssetEventPushOutcome {
        if self.requires_rescan() || self.events.len() >= self.limits.items().get() {
            self.discarded_events = self.discarded_events.saturating_add(1);
            return AssetEventPushOutcome::RescanRequired;
        }
        self.events.push(AssetEvent::new(id, version, kind));
        AssetEventPushOutcome::Accepted
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &AssetEvent> {
        self.events.iter()
    }

    pub fn drain(&mut self) -> Vec<AssetEvent> {
        std::mem::take(&mut self.events)
    }

    #[must_use]
    pub const fn limits(&self) -> AssetEventLimits {
        self.limits
    }

    #[must_use]
    pub const fn discarded_events(&self) -> u64 {
        self.discarded_events
    }

    #[must_use]
    pub const fn requires_rescan(&self) -> bool {
        self.discarded_events != 0
    }

    /// Clears retained observations after the caller has rebuilt its view from `AssetStates`.
    pub fn complete_rescan(&mut self) {
        self.events.clear();
        self.discarded_events = 0;
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[derive(Debug, Default, Resource)]
pub struct AssetDependencyGraph {
    source_to_artifacts: BTreeMap<StableAssetId, BTreeSet<ImportArtifactDigest>>,
    artifact_to_sources: BTreeMap<ImportArtifactDigest, BTreeSet<StableAssetId>>,
    source_to_dependents: BTreeMap<StableAssetId, BTreeSet<StableAssetId>>,
}

impl AssetDependencyGraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_artifact_dependency(
        &mut self,
        source: StableAssetId,
        artifact: ImportArtifactDigest,
    ) {
        self.source_to_artifacts
            .entry(source)
            .or_default()
            .insert(artifact);
        self.artifact_to_sources
            .entry(artifact)
            .or_default()
            .insert(source);
    }

    pub fn remove_artifact_dependency(
        &mut self,
        source: StableAssetId,
        artifact: ImportArtifactDigest,
    ) {
        if let Some(artifacts) = self.source_to_artifacts.get_mut(&source) {
            artifacts.remove(&artifact);
            if artifacts.is_empty() {
                self.source_to_artifacts.remove(&source);
            }
        }
        if let Some(sources) = self.artifact_to_sources.get_mut(&artifact) {
            sources.remove(&source);
            if sources.is_empty() {
                self.artifact_to_sources.remove(&artifact);
            }
        }
    }

    pub fn artifacts_for_source(
        &self,
        source: StableAssetId,
    ) -> impl Iterator<Item = ImportArtifactDigest> + '_ {
        self.source_to_artifacts
            .get(&source)
            .into_iter()
            .flat_map(|artifacts| artifacts.iter().copied())
    }

    pub fn sources_for_artifact(
        &self,
        artifact: ImportArtifactDigest,
    ) -> impl Iterator<Item = StableAssetId> + '_ {
        self.artifact_to_sources
            .get(&artifact)
            .into_iter()
            .flat_map(|sources| sources.iter().copied())
    }

    #[must_use]
    pub fn affected_artifacts(&self, source: StableAssetId) -> Vec<ImportArtifactDigest> {
        self.artifacts_for_source(source).collect()
    }

    pub fn add_source_dependency(&mut self, source: StableAssetId, dependent: StableAssetId) {
        self.source_to_dependents
            .entry(source)
            .or_default()
            .insert(dependent);
    }

    pub fn remove_source_dependency(&mut self, source: StableAssetId, dependent: StableAssetId) {
        if let Some(dependents) = self.source_to_dependents.get_mut(&source) {
            dependents.remove(&dependent);
            if dependents.is_empty() {
                self.source_to_dependents.remove(&source);
            }
        }
    }

    pub fn dependents_for_source(
        &self,
        source: StableAssetId,
    ) -> impl Iterator<Item = StableAssetId> + '_ {
        self.source_to_dependents
            .get(&source)
            .into_iter()
            .flat_map(|dependents| dependents.iter().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AssetRecord, AssetServer, AssetSourceKind, AssetSourcePath, Assets, StableAssetId,
    };

    fn stable_id() -> StableAssetId {
        StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap()
    }

    fn other_stable_id() -> StableAssetId {
        StableAssetId::parse_str("b73f0f16-09e8-4265-b090-b689b41c197e").unwrap()
    }

    fn handle() -> crate::Handle<String> {
        let mut server = AssetServer::new();
        let record = AssetRecord::new(
            stable_id(),
            AssetSourcePath::new("textures/player.png").unwrap(),
            AssetSourceKind::Image,
        );
        server.reserve_record::<String>(&record).unwrap()
    }

    #[test]
    fn state_revision_changes_across_same_version_loading_aba() {
        let handle = handle();
        let mut states = AssetStates::default();
        let version = states.set_loading(handle.id());
        let loading = states.state_revision(handle.id());

        states
            .set_failed(handle.id(), String::from("test-failure"))
            .unwrap();
        let failed = states.state_revision(handle.id());
        assert_ne!(failed, loading);

        assert_eq!(states.set_loading(handle.id()), version);
        assert_eq!(states.version(handle.id()), Some(version));
        assert_eq!(
            states.state(handle.id()).unwrap().load_state(),
            &LoadState::Loading
        );
        assert_ne!(states.state_revision(handle.id()), failed);
        assert_ne!(states.state_revision(handle.id()), loading);
    }

    #[test]
    fn absent_state_revisions_bind_candidates_to_one_store() {
        let id = handle().id();
        let first = AssetStates::default();
        let second = AssetStates::default();

        assert_ne!(first.state_revision(id), second.state_revision(id));
    }

    #[test]
    fn metadata_observation_outlives_attempts_and_repeated_observation_revokes_old_mutation() {
        let handle = handle();
        let mut assets = Assets::<String>::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        states.observe_metadata_change(handle.id());
        let first = states
            .metadata_change_revision(handle.id())
            .unwrap()
            .clone();
        states.set_loading(handle.id());
        assets
            .record_load_failure(handle, &mut states, &mut events, "decode failed")
            .unwrap();
        assert_eq!(states.metadata_change_revision(handle.id()), Some(&first));
        states.set_loading(handle.id());
        let baseline = assets.mutation_baseline(handle, &states).unwrap();
        states.observe_metadata_change(handle.id());
        assert_ne!(states.metadata_change_revision(handle.id()), Some(&first));
        assert!(baseline.validate(&assets, &states).is_err());
        assert!(states.metadata_change_pending(handle.id()));
    }

    #[test]
    fn source_removal_advances_revocation_even_before_a_usable_publication() {
        let handle = handle();
        let mut states = AssetStates::default();
        let initial = states.publication_revision(handle.id());
        states.observe_source_removal(handle.id());
        let first = states.publication_revision(handle.id());
        assert_ne!(first, initial);
        assert!(states.source_removal_pending(handle.id()));
        states.set_loading(handle.id());
        states
            .set_failed(handle.id(), "decode failed".to_owned())
            .unwrap();
        assert!(states.source_removal_pending(handle.id()));
        states.observe_source_removal(handle.id());
        assert_ne!(states.publication_revision(handle.id()), first);
        states.set_loaded_at(handle.id(), AssetVersion::from_raw(1), None, None);
        assert!(!states.source_removal_pending(handle.id()));
    }

    #[test]
    fn metadata_readmission_requires_an_observation_after_the_latest_removal() {
        let handle = handle();
        let mut states = AssetStates::default();
        states.observe_metadata_change(handle.id());
        states.observe_source_removal(handle.id());
        assert!(states.metadata_change_pending(handle.id()));
        assert!(!states.metadata_change_after_source_removal(handle.id()));
        states.observe_metadata_change(handle.id());
        assert!(states.metadata_change_after_source_removal(handle.id()));
        states.set_loading(handle.id());
        states
            .set_failed(handle.id(), "queue rejected".to_owned())
            .unwrap();
        assert!(states.metadata_change_after_source_removal(handle.id()));
        states.observe_source_removal(handle.id());
        assert!(!states.metadata_change_after_source_removal(handle.id()));
        states.observe_metadata_change(handle.id());
        assert!(states.metadata_change_after_source_removal(handle.id()));
    }

    #[test]
    fn publication_revision_survives_replacement_and_rotates_on_removal() {
        let handle = handle();
        let mut states = AssetStates::default();
        let initial = states.publication_revision(handle.id());
        states.set_loading(handle.id());
        assert_eq!(states.publication_revision(handle.id()), initial);
        states.set_loaded_at(handle.id(), AssetVersion::from_raw(1), None, None);
        let published = states.publication_revision(handle.id());
        assert_eq!(published, initial);
        states.invalidate_publication(handle.id());
        assert_ne!(states.publication_revision(handle.id()), published);
        let revoked = states.publication_revision(handle.id());
        states.set_loaded_at(handle.id(), AssetVersion::from_raw(2), None, None);
        assert_eq!(states.publication_revision(handle.id()), revoked);
    }

    #[test]
    fn reload_commits_value_hash_version_and_event_together() {
        let handle = handle();
        let mut assets = Assets::<String>::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        let source_hash = SourceHash::from_bytes(b"source-v1");
        let import_hash = ImportArtifactDigest::from_bytes(b"import-v1");

        let version = assets
            .commit_loaded(
                handle,
                "first".to_string(),
                &mut states,
                &mut events,
                Some(source_hash),
                Some(import_hash),
            )
            .unwrap();

        assert_eq!(version, AssetVersion::from_raw(1));
        assert_eq!(assets.get(handle).map(String::as_str), Some("first"));
        assert_eq!(states.state(handle.id()).unwrap().version(), version);
        assert_eq!(
            states.state(handle.id()).unwrap().source_hash(),
            Some(source_hash)
        );

        let version = assets
            .commit_reload(
                handle,
                version,
                "second".to_string(),
                &mut states,
                &mut events,
                Some(SourceHash::from_bytes(b"source-v2")),
                Some(ImportArtifactDigest::from_bytes(b"import-v2")),
            )
            .unwrap();

        assert_eq!(version, AssetVersion::from_raw(2));
        assert_eq!(assets.get(handle).map(String::as_str), Some("second"));
        assert_eq!(
            events.drain(),
            vec![
                AssetEvent::new(
                    handle.id(),
                    AssetVersion::from_raw(1),
                    AssetEventKind::Added
                ),
                AssetEvent::new(
                    handle.id(),
                    AssetVersion::from_raw(2),
                    AssetEventKind::Modified
                ),
            ]
        );
    }

    #[test]
    fn failed_reload_keeps_last_good_value_and_version() {
        let handle = handle();
        let mut assets = Assets::<String>::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        let version = assets
            .commit_loaded(
                handle,
                "first".to_string(),
                &mut states,
                &mut events,
                None,
                None,
            )
            .unwrap();
        events.drain();

        let failed_version = assets
            .record_reload_failure(handle, &mut states, &mut events, "decode failed")
            .unwrap();

        assert_eq!(failed_version, version);
        assert_eq!(
            states.state(handle.id()).unwrap().usable_version(),
            Some(version)
        );
        assert_eq!(assets.get(handle).map(String::as_str), Some("first"));
        assert_eq!(
            states.state(handle.id()).unwrap().load_state(),
            &LoadState::Failed {
                message: "decode failed".to_string()
            }
        );
        assert_eq!(
            events.drain(),
            vec![AssetEvent::new(
                handle.id(),
                version,
                AssetEventKind::ReloadFailed
            )]
        );
    }

    #[test]
    fn removed_asset_updates_state_and_emits_event() {
        let handle = handle();
        let mut assets = Assets::<String>::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        let version = assets
            .commit_loaded(
                handle,
                "first".to_string(),
                &mut states,
                &mut events,
                None,
                None,
            )
            .unwrap();
        events.drain();

        let removed = assets
            .remove_with_state(handle, &mut states, &mut events)
            .unwrap();

        assert_eq!(removed.as_deref(), Some("first"));
        assert_eq!(states.state(handle.id()).unwrap().usable_version(), None);
        assert_eq!(assets.get(handle), None);
        assert_eq!(
            states.state(handle.id()).unwrap().load_state(),
            &LoadState::Removed
        );
        assert_eq!(
            states.state(handle.id()).unwrap().version(),
            AssetVersion::from_raw(version.raw() + 1)
        );
        assert_eq!(
            events.drain(),
            vec![AssetEvent::new(
                handle.id(),
                AssetVersion::from_raw(version.raw() + 1),
                AssetEventKind::Removed
            )]
        );
    }

    #[test]
    fn publication_invalidation_changes_revision_and_stays_revoked_until_success() {
        let handle = handle();
        let mut states = AssetStates::default();
        states.set_loading(handle.id());
        assert_eq!(states.state(handle.id()).unwrap().usable_version(), None);
        states
            .set_failed(handle.id(), "initial failure".to_owned())
            .unwrap();
        assert_eq!(states.state(handle.id()).unwrap().usable_version(), None);

        let version = AssetVersion::from_raw(1);
        states.set_loaded_at(handle.id(), version, None, None);
        states.set_loading(handle.id());
        let before = states.state_revision(handle.id());
        states.invalidate_publication(handle.id());
        assert_ne!(states.state_revision(handle.id()), before);
        assert_eq!(states.state(handle.id()).unwrap().version(), version);
        assert_eq!(states.state(handle.id()).unwrap().usable_version(), None);
        states.reject_reload_request(handle.id(), "identity changed");
        states.set_loading(handle.id());
        assert_eq!(states.state(handle.id()).unwrap().usable_version(), None);

        let next = states.next_version(handle.id()).unwrap();
        states.set_loaded_at(handle.id(), next, None, None);
        assert_eq!(
            states.state(handle.id()).unwrap().usable_version(),
            Some(next)
        );
    }

    #[test]
    fn stale_reload_cannot_overwrite_newer_asset_state() {
        let handle = handle();
        let mut assets = Assets::<String>::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        let version = assets
            .commit_loaded(
                handle,
                "first".to_string(),
                &mut states,
                &mut events,
                None,
                None,
            )
            .unwrap();
        let newer = assets
            .commit_reload(
                handle,
                version,
                "second".to_string(),
                &mut states,
                &mut events,
                None,
                None,
            )
            .unwrap();

        let error = assets
            .commit_reload(
                handle,
                version,
                "stale".to_string(),
                &mut states,
                &mut events,
                None,
                None,
            )
            .unwrap_err();

        assert!(matches!(error, AssetStateError::StaleReload { current, .. } if current == newer));
        assert_eq!(assets.get(handle).map(String::as_str), Some("second"));
    }

    #[test]
    fn dependency_graph_queries_source_to_artifacts() {
        let mut graph = AssetDependencyGraph::default();
        let first = ImportArtifactDigest::from_bytes(b"first");
        let second = ImportArtifactDigest::from_bytes(b"second");

        graph.add_artifact_dependency(stable_id(), first);
        graph.add_artifact_dependency(stable_id(), second);
        graph.add_artifact_dependency(other_stable_id(), second);

        assert_eq!(graph.affected_artifacts(stable_id()), vec![first, second]);
        assert_eq!(
            graph.sources_for_artifact(second).collect::<Vec<_>>(),
            vec![stable_id(), other_stable_id()]
        );

        graph.remove_artifact_dependency(stable_id(), first);
        assert_eq!(graph.affected_artifacts(stable_id()), vec![second]);
    }
}
