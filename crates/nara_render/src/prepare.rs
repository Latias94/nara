use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_asset::{AssetId, AssetSlotRevision, AssetVersion, Handle, ImportArtifactDigest};
use nara_ecs::Resource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderResourceKind(&'static str);

impl RenderResourceKind {
    pub const IMAGE_2D: Self = Self("image_2d");

    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl Display for RenderResourceKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderResourceKey {
    asset_id: AssetId,
    kind: RenderResourceKind,
}

impl RenderResourceKey {
    #[must_use]
    pub const fn new(asset_id: AssetId, kind: RenderResourceKind) -> Self {
        Self { asset_id, kind }
    }

    #[must_use]
    pub const fn for_asset<T>(handle: Handle<T>, kind: RenderResourceKind) -> Self {
        Self::new(handle.id(), kind)
    }

    #[must_use]
    pub const fn asset_id(self) -> AssetId {
        self.asset_id
    }

    #[must_use]
    pub const fn kind(self) -> RenderResourceKind {
        self.kind
    }
}

/// Complete backend-neutral identity of one prepared asset value.
///
/// The slot revision binds the snapshot to the exact value stored in [`nara_asset::Assets`], even
/// when a direct replacement does not advance loader-owned [`AssetVersion`] metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderResourceSnapshot {
    key: RenderResourceKey,
    asset_version: AssetVersion,
    slot_revision: AssetSlotRevision,
    descriptor_hash: ImportArtifactDigest,
}

impl RenderResourceSnapshot {
    #[must_use]
    pub const fn new(
        key: RenderResourceKey,
        asset_version: AssetVersion,
        slot_revision: AssetSlotRevision,
        descriptor_hash: ImportArtifactDigest,
    ) -> Self {
        Self {
            key,
            asset_version,
            slot_revision,
            descriptor_hash,
        }
    }

    #[must_use]
    pub const fn key(&self) -> RenderResourceKey {
        self.key
    }

    #[must_use]
    pub const fn asset_version(&self) -> AssetVersion {
        self.asset_version
    }

    #[must_use]
    /// Returns the exact typed-asset slot mutation admitted by this snapshot.
    pub const fn slot_revision(&self) -> &AssetSlotRevision {
        &self.slot_revision
    }

    #[must_use]
    pub const fn descriptor_hash(&self) -> ImportArtifactDigest {
        self.descriptor_hash
    }
}

pub trait PreparedRenderResource: Send + Sync + 'static {}

impl<T> PreparedRenderResource for T where T: Send + Sync + 'static {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderPrepareStatus {
    Ready,
    Failed(RenderPrepareError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderPrepareError {
    message: String,
}

impl RenderPrepareError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for RenderPrepareError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for RenderPrepareError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRenderResourceRecord<T: PreparedRenderResource> {
    snapshot: RenderResourceSnapshot,
    status: RenderPrepareStatus,
    resource: Option<T>,
}

impl<T: PreparedRenderResource> PreparedRenderResourceRecord<T> {
    #[must_use]
    pub fn ready(snapshot: RenderResourceSnapshot, resource: T) -> Self {
        Self {
            snapshot,
            status: RenderPrepareStatus::Ready,
            resource: Some(resource),
        }
    }

    #[must_use]
    pub fn failed(snapshot: RenderResourceSnapshot, error: RenderPrepareError) -> Self {
        Self {
            snapshot,
            status: RenderPrepareStatus::Failed(error),
            resource: None,
        }
    }

    #[must_use]
    pub const fn snapshot(&self) -> &RenderResourceSnapshot {
        &self.snapshot
    }

    #[must_use]
    pub const fn status(&self) -> &RenderPrepareStatus {
        &self.status
    }

    #[must_use]
    pub fn resource(&self) -> Option<&T> {
        self.resource.as_ref()
    }
}

#[derive(Debug, Resource)]
pub struct PreparedRenderResources<T: PreparedRenderResource> {
    records: BTreeMap<RenderResourceKey, PreparedRenderResourceRecord<T>>,
}

impl<T: PreparedRenderResource> Default for PreparedRenderResources<T> {
    fn default() -> Self {
        Self {
            records: BTreeMap::new(),
        }
    }
}

impl<T: PreparedRenderResource> PreparedRenderResources<T> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_ready(&mut self, snapshot: RenderResourceSnapshot, resource: T) {
        self.records.insert(
            snapshot.key(),
            PreparedRenderResourceRecord::ready(snapshot, resource),
        );
    }

    pub fn record_failed(&mut self, snapshot: RenderResourceSnapshot, error: RenderPrepareError) {
        self.records.insert(
            snapshot.key(),
            PreparedRenderResourceRecord::failed(snapshot, error),
        );
    }

    #[must_use]
    pub fn get(&self, key: RenderResourceKey) -> Option<&PreparedRenderResourceRecord<T>> {
        self.records.get(&key)
    }

    #[must_use]
    pub fn get_ready(&self, key: RenderResourceKey) -> Option<&T> {
        let record = self.records.get(&key)?;
        if record.status() == &RenderPrepareStatus::Ready {
            record.resource()
        } else {
            None
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = RenderResourceKey> + '_ {
        self.records.keys().copied()
    }

    #[must_use]
    pub fn needs_prepare(&self, snapshot: &RenderResourceSnapshot) -> bool {
        let Some(record) = self.records.get(&snapshot.key()) else {
            return true;
        };

        record.snapshot() != snapshot || record.status() != &RenderPrepareStatus::Ready
    }

    pub fn remove(&mut self, key: RenderResourceKey) -> Option<PreparedRenderResourceRecord<T>> {
        self.records.remove(&key)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::Assets;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct MockPreparedResource(&'static str);

    fn handle() -> Handle<String> {
        Handle::new(AssetId::from_raw(7))
    }

    fn key() -> RenderResourceKey {
        RenderResourceKey::for_asset(handle(), RenderResourceKind::new("mock"))
    }

    fn snapshot(
        version: u64,
        slot_revision: AssetSlotRevision,
        hash: &[u8],
    ) -> RenderResourceSnapshot {
        RenderResourceSnapshot::new(
            key(),
            AssetVersion::from_raw(version),
            slot_revision,
            ImportArtifactDigest::from_bytes(hash),
        )
    }

    fn assets() -> Assets<String> {
        let mut assets = Assets::default();
        assets.insert(handle(), String::from("source"));
        assets
    }

    #[test]
    fn ready_resources_are_keyed_by_snapshot() {
        let mut resources = PreparedRenderResources::<MockPreparedResource>::default();
        let assets = assets();
        let snapshot = snapshot(1, assets.slot_revision(handle()), b"descriptor");

        resources.insert_ready(snapshot.clone(), MockPreparedResource("ready"));

        assert!(!resources.needs_prepare(&snapshot));
        assert_eq!(
            resources.get_ready(key()),
            Some(&MockPreparedResource("ready"))
        );
    }

    #[test]
    fn descriptor_or_version_changes_replace_the_snapshot_cache_record() {
        let mut resources = PreparedRenderResources::<MockPreparedResource>::default();
        let assets = assets();
        let revision = assets.slot_revision(handle());
        let old_snapshot = snapshot(1, revision.clone(), b"old");
        let new_snapshot = snapshot(2, revision, b"new");
        resources.insert_ready(old_snapshot, MockPreparedResource("old"));

        assert!(resources.needs_prepare(&new_snapshot));
        resources.insert_ready(new_snapshot.clone(), MockPreparedResource("new"));
        assert!(!resources.needs_prepare(&new_snapshot));
        assert_eq!(resources.len(), 1);
        assert_eq!(
            resources.get_ready(key()),
            Some(&MockPreparedResource("new"))
        );
    }

    #[test]
    fn slot_revision_changes_prepare_identity_without_version_or_descriptor_changes() {
        let mut resources = PreparedRenderResources::<MockPreparedResource>::default();
        let mut assets = assets();
        let old_snapshot = snapshot(1, assets.slot_revision(handle()), b"same-descriptor");
        resources.insert_ready(old_snapshot.clone(), MockPreparedResource("old"));

        assets.get_mut(handle()).unwrap().push_str("-changed");
        let changed_snapshot = snapshot(1, assets.slot_revision(handle()), b"same-descriptor");

        assert_ne!(old_snapshot, changed_snapshot);
        assert!(resources.needs_prepare(&changed_snapshot));
    }

    #[test]
    fn current_synchronous_prepare_replaces_a_prior_loader_version() {
        let mut resources = PreparedRenderResources::<MockPreparedResource>::default();
        let first_assets = assets();
        resources.insert_ready(
            snapshot(2, first_assets.slot_revision(handle()), b"old-store"),
            MockPreparedResource("old-store"),
        );
        let replacement_assets = assets();
        let replacement = snapshot(
            1,
            replacement_assets.slot_revision(handle()),
            b"replacement-store",
        );

        resources.insert_ready(
            replacement.clone(),
            MockPreparedResource("replacement-store"),
        );

        assert!(!resources.needs_prepare(&replacement));
        assert_eq!(
            resources.get_ready(key()),
            Some(&MockPreparedResource("replacement-store"))
        );
    }

    #[test]
    fn failed_prepare_records_status_without_panicking() {
        let mut resources = PreparedRenderResources::<MockPreparedResource>::default();
        let assets = assets();
        let snapshot = snapshot(1, assets.slot_revision(handle()), b"bad");

        resources.record_failed(snapshot.clone(), RenderPrepareError::new("out of memory"));

        let record = resources.get(key()).unwrap();
        assert!(matches!(
            record.status(),
            RenderPrepareStatus::Failed(error) if error.message() == "out of memory"
        ));
        assert!(resources.needs_prepare(&snapshot));
    }

    #[test]
    fn resources_can_be_removed_without_retaining_a_second_event_log() {
        let mut resources = PreparedRenderResources::<MockPreparedResource>::default();
        let assets = assets();
        resources.insert_ready(
            snapshot(1, assets.slot_revision(handle()), b"descriptor"),
            MockPreparedResource("ready"),
        );

        let removed = resources.remove(key());

        assert!(removed.is_some());
        assert!(resources.is_empty());
    }
}
