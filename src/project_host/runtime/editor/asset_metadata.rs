use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_asset::{AssetMetaIdentityCandidate, AssetRecord, AssetSourcePath, StableAssetId};
use nara_fs::{ContentDigest, FileIdentity};

use crate::project_host::authoring::ProjectSourceIndexSnapshot;

pub(super) struct AdmittedAssetMetadata {
    pub(super) candidate: Arc<AssetMetaIdentityCandidate>,
    pub(super) identity: FileIdentity,
    pub(super) digest: ContentDigest,
}

pub(crate) struct EditorAssetMetadataSnapshot {
    source_index: Arc<ProjectSourceIndexSnapshot>,
    admitted: BTreeMap<AssetSourcePath, AdmittedAssetMetadata>,
    known_stable_ids: BTreeSet<StableAssetId>,
    conflicts: BTreeMap<AssetSourcePath, StableAssetId>,
}

impl EditorAssetMetadataSnapshot {
    pub(super) fn fresh_stable_id(&self) -> Option<StableAssetId> {
        (0..16)
            .filter_map(|_| StableAssetId::try_random().ok())
            .find(|id| !self.contains_stable_id(*id))
    }
    pub(super) fn new(
        source_index: Arc<ProjectSourceIndexSnapshot>,
        admitted: BTreeMap<AssetSourcePath, AdmittedAssetMetadata>,
        known_stable_ids: BTreeSet<StableAssetId>,
        conflicts: BTreeMap<AssetSourcePath, StableAssetId>,
    ) -> Self {
        Self {
            source_index,
            admitted,
            known_stable_ids,
            conflicts,
        }
    }

    #[must_use]
    pub(super) fn matches_source_index(
        &self,
        source_index: &Arc<ProjectSourceIndexSnapshot>,
    ) -> bool {
        Arc::ptr_eq(&self.source_index, source_index)
    }

    /// Returns an identity-admitted candidate, including conflicting identities.
    /// Callers must check `conflict` before admitting domain settings for publication.
    #[must_use]
    pub(super) fn candidate(&self, path: &AssetSourcePath) -> Option<&AssetMetaIdentityCandidate> {
        self.admitted
            .get(path)
            .map(|entry| entry.candidate.as_ref())
    }

    /// Returns a historical content observation, including conflicting identities.
    /// Callers must check `conflict` and freshly admit a live leaf before publication.
    #[must_use]
    pub(super) fn observed_digest(&self, path: &AssetSourcePath) -> Option<ContentDigest> {
        self.admitted.get(path).map(|entry| entry.digest)
    }

    #[must_use]
    pub(super) fn contains_stable_id(&self, stable_id: StableAssetId) -> bool {
        self.known_stable_ids.contains(&stable_id)
    }

    #[must_use]
    pub(super) fn conflict(&self, path: &AssetSourcePath) -> Option<StableAssetId> {
        self.conflicts.get(path).copied()
    }

    #[must_use]
    pub(super) fn asset_identity(
        &self,
        path: &AssetSourcePath,
    ) -> Option<(AssetRecord, FileIdentity, ContentDigest)> {
        if self.conflict(path).is_some() {
            return None;
        }
        let admitted = self.admitted.get(path)?;
        let identity = admitted.candidate.identity();
        Some((
            AssetRecord::new(
                identity.stable_id(),
                path.clone(),
                identity.source_kind().clone(),
            ),
            admitted.identity,
            admitted.digest,
        ))
    }
}
