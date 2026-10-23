use std::fmt::{self, Display, Formatter};

use nara_asset::{AssetId, AssetSlotRevision, AssetVersion, Handle, ImportArtifactDigest};

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
