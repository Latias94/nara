use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_asset::{AssetId, AssetVersion, Handle, ImportArtifactDigest};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderResourceSnapshot {
    key: RenderResourceKey,
    asset_version: AssetVersion,
    descriptor_hash: ImportArtifactDigest,
}

impl RenderResourceSnapshot {
    #[must_use]
    pub const fn new(
        key: RenderResourceKey,
        asset_version: AssetVersion,
        descriptor_hash: ImportArtifactDigest,
    ) -> Self {
        Self {
            key,
            asset_version,
            descriptor_hash,
        }
    }

    #[must_use]
    pub const fn key(self) -> RenderResourceKey {
        self.key
    }

    #[must_use]
    pub const fn asset_version(self) -> AssetVersion {
        self.asset_version
    }

    #[must_use]
    pub const fn descriptor_hash(self) -> ImportArtifactDigest {
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
    pub const fn snapshot(&self) -> RenderResourceSnapshot {
        self.snapshot
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderPrepareApplyResult {
    Applied,
    DiscardedStale {
        current: AssetVersion,
        attempted: AssetVersion,
    },
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

    pub fn insert_ready(
        &mut self,
        snapshot: RenderResourceSnapshot,
        resource: T,
    ) -> RenderPrepareApplyResult {
        if let Some(result) = self.stale_result(snapshot) {
            return result;
        }

        self.records.insert(
            snapshot.key(),
            PreparedRenderResourceRecord::ready(snapshot, resource),
        );
        RenderPrepareApplyResult::Applied
    }

    pub fn record_failed(
        &mut self,
        snapshot: RenderResourceSnapshot,
        error: RenderPrepareError,
    ) -> RenderPrepareApplyResult {
        if let Some(result) = self.stale_result(snapshot) {
            return result;
        }

        self.records.insert(
            snapshot.key(),
            PreparedRenderResourceRecord::failed(snapshot, error),
        );
        RenderPrepareApplyResult::Applied
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
    pub fn needs_prepare(&self, snapshot: RenderResourceSnapshot) -> bool {
        let Some(record) = self.records.get(&snapshot.key()) else {
            return true;
        };

        record.snapshot() != snapshot || record.status() != &RenderPrepareStatus::Ready
    }

    pub fn invalidate_if_snapshot_changed(
        &mut self,
        snapshot: RenderResourceSnapshot,
        invalidations: &mut RenderPrepareInvalidations,
        reason: RenderPrepareInvalidationReason,
    ) -> bool {
        let changed = self
            .records
            .get(&snapshot.key())
            .is_some_and(|record| record.snapshot() != snapshot);
        if changed {
            self.records.remove(&snapshot.key());
            invalidations.push(snapshot.key(), reason);
        }
        changed
    }

    pub fn remove(
        &mut self,
        key: RenderResourceKey,
        invalidations: &mut RenderPrepareInvalidations,
        reason: RenderPrepareInvalidationReason,
    ) -> Option<PreparedRenderResourceRecord<T>> {
        let removed = self.records.remove(&key);
        if removed.is_some() {
            invalidations.push(key, reason);
        }
        removed
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    fn stale_result(&self, snapshot: RenderResourceSnapshot) -> Option<RenderPrepareApplyResult> {
        let current = self.records.get(&snapshot.key())?.snapshot();
        if current.asset_version() > snapshot.asset_version() {
            return Some(RenderPrepareApplyResult::DiscardedStale {
                current: current.asset_version(),
                attempted: snapshot.asset_version(),
            });
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderPrepareInvalidationReason {
    AssetModified,
    AssetRemoved,
    DescriptorChanged,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderPrepareInvalidation {
    key: RenderResourceKey,
    reason: RenderPrepareInvalidationReason,
}

impl RenderPrepareInvalidation {
    #[must_use]
    pub const fn new(key: RenderResourceKey, reason: RenderPrepareInvalidationReason) -> Self {
        Self { key, reason }
    }

    #[must_use]
    pub const fn key(self) -> RenderResourceKey {
        self.key
    }

    #[must_use]
    pub const fn reason(self) -> RenderPrepareInvalidationReason {
        self.reason
    }
}

#[derive(Debug, Default, Resource)]
pub struct RenderPrepareInvalidations {
    invalidations: Vec<RenderPrepareInvalidation>,
}

impl RenderPrepareInvalidations {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, key: RenderResourceKey, reason: RenderPrepareInvalidationReason) {
        self.invalidations
            .push(RenderPrepareInvalidation::new(key, reason));
    }

    #[must_use]
    pub fn iter(&self) -> impl Iterator<Item = &RenderPrepareInvalidation> {
        self.invalidations.iter()
    }

    pub fn drain(&mut self) -> Vec<RenderPrepareInvalidation> {
        std::mem::take(&mut self.invalidations)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.invalidations.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.invalidations.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct MockPreparedResource(&'static str);

    fn key() -> RenderResourceKey {
        RenderResourceKey::new(AssetId::from_raw(7), RenderResourceKind::new("mock"))
    }

    fn snapshot(version: u64, hash: &[u8]) -> RenderResourceSnapshot {
        RenderResourceSnapshot::new(
            key(),
            AssetVersion::from_raw(version),
            ImportArtifactDigest::from_bytes(hash),
        )
    }

    #[test]
    fn ready_resources_are_keyed_by_snapshot() {
        let mut resources = PreparedRenderResources::<MockPreparedResource>::default();
        let snapshot = snapshot(1, b"descriptor");

        assert_eq!(
            resources.insert_ready(snapshot, MockPreparedResource("ready")),
            RenderPrepareApplyResult::Applied
        );

        assert!(!resources.needs_prepare(snapshot));
        assert_eq!(
            resources.get_ready(key()),
            Some(&MockPreparedResource("ready"))
        );
    }

    #[test]
    fn descriptor_or_version_changes_invalidate_existing_resource() {
        let mut resources = PreparedRenderResources::<MockPreparedResource>::default();
        let mut invalidations = RenderPrepareInvalidations::default();
        let old_snapshot = snapshot(1, b"old");
        let new_snapshot = snapshot(2, b"new");
        resources.insert_ready(old_snapshot, MockPreparedResource("old"));

        assert!(resources.invalidate_if_snapshot_changed(
            new_snapshot,
            &mut invalidations,
            RenderPrepareInvalidationReason::DescriptorChanged
        ));

        assert!(resources.get_ready(key()).is_none());
        assert_eq!(
            invalidations.drain(),
            vec![RenderPrepareInvalidation::new(
                key(),
                RenderPrepareInvalidationReason::DescriptorChanged
            )]
        );
    }

    #[test]
    fn stale_prepare_results_do_not_overwrite_newer_resources() {
        let mut resources = PreparedRenderResources::<MockPreparedResource>::default();
        resources.insert_ready(snapshot(2, b"new"), MockPreparedResource("new"));

        assert_eq!(
            resources.insert_ready(snapshot(1, b"old"), MockPreparedResource("old")),
            RenderPrepareApplyResult::DiscardedStale {
                current: AssetVersion::from_raw(2),
                attempted: AssetVersion::from_raw(1),
            }
        );
        assert_eq!(
            resources.get_ready(key()),
            Some(&MockPreparedResource("new"))
        );
    }

    #[test]
    fn failed_prepare_records_status_without_panicking() {
        let mut resources = PreparedRenderResources::<MockPreparedResource>::default();
        let snapshot = snapshot(1, b"bad");

        assert_eq!(
            resources.record_failed(snapshot, RenderPrepareError::new("out of memory")),
            RenderPrepareApplyResult::Applied
        );

        let record = resources.get(key()).unwrap();
        assert!(matches!(
            record.status(),
            RenderPrepareStatus::Failed(error) if error.message() == "out of memory"
        ));
        assert!(resources.needs_prepare(snapshot));
    }

    #[test]
    fn resources_can_be_removed_with_invalidation_event() {
        let mut resources = PreparedRenderResources::<MockPreparedResource>::default();
        let mut invalidations = RenderPrepareInvalidations::default();
        resources.insert_ready(snapshot(1, b"descriptor"), MockPreparedResource("ready"));

        let removed = resources.remove(
            key(),
            &mut invalidations,
            RenderPrepareInvalidationReason::AssetRemoved,
        );

        assert!(removed.is_some());
        assert!(resources.is_empty());
        assert_eq!(
            invalidations.drain(),
            vec![RenderPrepareInvalidation::new(
                key(),
                RenderPrepareInvalidationReason::AssetRemoved
            )]
        );
    }
}
