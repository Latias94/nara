use std::fmt::{self, Display, Formatter};

use nara_asset::{AssetId, AssetSlotRevision, AssetVersion, Handle};

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

/// BLAKE3 content hash of a complete backend-neutral prepared resource.
///
/// The resource domain owns the canonical encoding and its domain separation policy. The hashed
/// content must cover both the prepared descriptor and its data. A domain can transfer an existing
/// digest with [`Self::from_digest`] when it covers that same complete prepared content.
///
/// This hash does not identify a logical resource, an asset slot mutation, a GPU epoch, or an import
/// artifact. Those identities remain the responsibility of their respective owners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderResourceContentHash([u8; 32]);

impl RenderResourceContentHash {
    /// Hashes the domain's complete canonical prepared descriptor and data encoding.
    #[must_use]
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
        Self(*blake3::hash(bytes.as_ref()).as_bytes())
    }

    /// Wraps an existing BLAKE3 digest of the complete prepared content without hashing it again.
    #[must_use]
    pub const fn from_digest(digest: [u8; 32]) -> Self {
        Self(digest)
    }

    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Complete backend-neutral identity of one prepared asset value.
///
/// The slot revision binds the snapshot to the exact value stored in [`nara_asset::Assets`], even
/// when a direct replacement does not advance loader-owned [`AssetVersion`] metadata.
///
/// The content hash has a distinct type from an import-artifact digest:
///
/// ```compile_fail
/// use nara_asset::{AssetSlotRevision, AssetVersion, ImportArtifactDigest};
/// use nara_render::{RenderResourceKey, RenderResourceSnapshot};
///
/// fn imported_snapshot(
///     key: RenderResourceKey,
///     version: AssetVersion,
///     slot: AssetSlotRevision,
///     artifact: ImportArtifactDigest,
/// ) -> RenderResourceSnapshot {
///     RenderResourceSnapshot::new(key, version, slot, artifact)
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderResourceSnapshot {
    key: RenderResourceKey,
    asset_version: AssetVersion,
    slot_revision: AssetSlotRevision,
    content_hash: RenderResourceContentHash,
}

impl RenderResourceSnapshot {
    #[must_use]
    pub const fn new(
        key: RenderResourceKey,
        asset_version: AssetVersion,
        slot_revision: AssetSlotRevision,
        content_hash: RenderResourceContentHash,
    ) -> Self {
        Self {
            key,
            asset_version,
            slot_revision,
            content_hash,
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
    pub const fn content_hash(&self) -> RenderResourceContentHash {
        self.content_hash
    }
}
