use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_ecs::Resource;

use crate::{AssetId, ImportArtifactDigest, SourceHash, StableAssetId};

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
}

impl AssetState {
    #[must_use]
    pub const fn new(
        version: AssetVersion,
        load_state: LoadState,
        source_hash: Option<SourceHash>,
        import_hash: Option<ImportArtifactDigest>,
    ) -> Self {
        Self {
            version,
            load_state,
            source_hash,
            import_hash,
        }
    }

    #[must_use]
    pub const fn version(&self) -> AssetVersion {
        self.version
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

#[derive(Debug, Default, Resource)]
pub struct AssetStates {
    states: BTreeMap<AssetId, AssetState>,
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
        self.states.insert(
            id,
            AssetState::new(version, LoadState::Loaded, source_hash, import_hash),
        );
    }

    pub(crate) fn set_removed_at(&mut self, id: AssetId, version: AssetVersion) {
        self.states
            .insert(id, AssetState::new(version, LoadState::Removed, None, None));
    }

    pub fn set_loading(&mut self, id: AssetId) -> AssetVersion {
        let current = self.state(id).cloned().unwrap_or_default();
        let version = current.version();
        self.states.insert(
            id,
            AssetState::new(
                version,
                LoadState::Loading,
                current.source_hash(),
                current.import_hash(),
            ),
        );
        version
    }

    pub(crate) fn set_failed(
        &mut self,
        id: AssetId,
        message: String,
    ) -> Result<AssetVersion, AssetStateError> {
        let current = self
            .state(id)
            .cloned()
            .ok_or(AssetStateError::UnknownAsset { id })?;
        self.states.insert(
            id,
            AssetState::new(
                current.version(),
                LoadState::Failed { message },
                current.source_hash(),
                current.import_hash(),
            ),
        );
        Ok(current.version())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.states.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetStateError {
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

#[derive(Debug, Default, Resource)]
pub struct AssetEvents {
    events: Vec<AssetEvent>,
}

impl AssetEvents {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, id: AssetId, version: AssetVersion, kind: AssetEventKind) {
        self.events.push(AssetEvent::new(id, version, kind));
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &AssetEvent> {
        self.events.iter()
    }

    pub fn drain(&mut self) -> Vec<AssetEvent> {
        std::mem::take(&mut self.events)
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
    use crate::{AssetPath, AssetRecord, AssetServer, AssetSourceKind, Assets, StableAssetId};

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
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceKind::Image,
        );
        server.reserve_record::<String>(&record).unwrap()
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
