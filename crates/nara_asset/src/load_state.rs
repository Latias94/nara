use std::{
    collections::{HashMap, VecDeque},
    fmt::{self, Display, Formatter},
};

use crate::{AssetId, AssetSlotRevision};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadState {
    Loading,
    Loaded,
    Failed { message: String },
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetState {
    version: AssetVersion,
    load_state: LoadState,
    published_slot: Option<AssetSlotRevision>,
    dependencies: Vec<nara_core::AssetId>,
}

impl AssetState {
    #[must_use]
    pub const fn version(&self) -> AssetVersion {
        self.version
    }

    /// Returns the last usable version, including after a failed refresh.
    #[must_use]
    pub const fn usable_version(&self) -> Option<AssetVersion> {
        if self.published_slot.is_some() {
            Some(self.version)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn load_state(&self) -> &LoadState {
        &self.load_state
    }

    /// Persistent IDs of the byte dependencies read by the loader of the usable version.
    #[must_use]
    pub fn dependencies(&self) -> &[nara_core::AssetId] {
        &self.dependencies
    }

    pub(crate) fn set_load_state(&mut self, load_state: LoadState) {
        self.load_state = load_state;
    }
}

#[derive(Debug, Default)]
#[cfg_attr(feature = "runtime", derive(nara_ecs::Resource))]
pub struct AssetStates {
    states: HashMap<AssetId, AssetState>,
}

impl AssetStates {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn state(&self, id: AssetId) -> Option<&AssetState> {
        self.states.get(&id)
    }

    #[must_use]
    pub fn version(&self, id: AssetId) -> Option<AssetVersion> {
        self.state(id).map(AssetState::version)
    }

    #[must_use]
    pub fn published_slot_revision(&self, id: AssetId) -> Option<&AssetSlotRevision> {
        self.state(id)?.published_slot.as_ref()
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (AssetId, &AssetState)> {
        self.states.iter().map(|(id, state)| (*id, state))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.states.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub(crate) fn next_version(&self, id: AssetId) -> Result<AssetVersion, AssetStateError> {
        let current = self.version(id).unwrap_or(AssetVersion::ZERO);
        current
            .0
            .checked_add(1)
            .map(AssetVersion)
            .ok_or(AssetStateError::VersionExhausted { id, current })
    }

    pub(crate) fn state_mut(&mut self, id: AssetId) -> Option<&mut AssetState> {
        self.states.get_mut(&id)
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn set_loading(&mut self, id: AssetId) {
        self.states
            .entry(id)
            .or_insert_with(|| AssetState {
                version: AssetVersion::ZERO,
                load_state: LoadState::Loading,
                published_slot: None,
                dependencies: Vec::new(),
            })
            .set_load_state(LoadState::Loading);
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn set_dependencies(&mut self, id: AssetId, dependencies: Vec<nara_core::AssetId>) {
        if let Some(state) = self.state_mut(id) {
            state.dependencies = dependencies;
        }
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn reset_unpublished_failures(&mut self) {
        for state in self.states.values_mut() {
            if state.published_slot.is_none()
                && matches!(state.load_state, LoadState::Failed { .. })
            {
                state.set_load_state(LoadState::Removed);
            }
        }
    }

    #[cfg(feature = "runtime")]
    pub(crate) fn fail_loading(&mut self, id: AssetId, message: String, events: &mut AssetEvents) {
        if let Some(state) = self.state_mut(id) {
            state.set_load_state(LoadState::Failed { message });
            let kind = if state.published_slot.is_some() {
                AssetEventKind::ReloadFailed
            } else {
                AssetEventKind::LoadFailed
            };
            events.push(id, state.version(), kind);
        }
    }

    pub(crate) fn set_loaded(
        &mut self,
        id: AssetId,
        version: AssetVersion,
        slot: AssetSlotRevision,
    ) {
        self.states.insert(
            id,
            AssetState {
                version,
                load_state: LoadState::Loaded,
                published_slot: Some(slot),
                dependencies: Vec::new(),
            },
        );
    }

    pub(crate) fn set_removed(&mut self, id: AssetId, version: AssetVersion) {
        self.states.insert(
            id,
            AssetState {
                version,
                load_state: LoadState::Removed,
                published_slot: None,
                dependencies: Vec::new(),
            },
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssetStateError {
    #[error("asset {id:?} has no loaded state")]
    UnknownAsset { id: AssetId },
    #[error("asset {id:?} version cannot advance past {current}")]
    VersionExhausted { id: AssetId, current: AssetVersion },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetEventKind {
    Added,
    Modified,
    Removed,
    LoadFailed,
    ReloadFailed,
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

const MAX_ASSET_EVENTS: usize = 4_096;

/// Recent asset changes in publication order. The oldest event expires when the queue fills.
#[derive(Debug, Default)]
#[cfg_attr(feature = "runtime", derive(nara_ecs::Resource))]
pub struct AssetEvents {
    events: VecDeque<AssetEvent>,
}

impl AssetEvents {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, id: AssetId, version: AssetVersion, kind: AssetEventKind) {
        if self.events.len() == MAX_ASSET_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(AssetEvent::new(id, version, kind));
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &AssetEvent> {
        self.events.iter()
    }

    pub fn drain(&mut self) -> Vec<AssetEvent> {
        self.events.drain(..).collect()
    }
}
