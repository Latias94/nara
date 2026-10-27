//! Code-free capture of one local finite tool release, before installation or execution approval.

use std::fmt;

use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity, FsError, RelativePath, TrustMode};
use nara_reflect::{
    ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentTypeId,
    PersistentComponentSchemaOwnerId,
};

mod manifest;

use manifest::{MANIFEST_NAME, MAX_MANIFEST_BYTES, Manifest, MemberKind};

/// The role of a captured file. Content has no installation destination or execution authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolReleaseMemberKind {
    Tool,
    Content,
}

/// An exact required local release. This is neither a version range nor a source resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolReleaseRequirement {
    package: String,
    manifest: ContentDigest,
}

impl EditorSourceToolReleaseRequirement {
    #[must_use]
    pub fn package(&self) -> &str {
        &self.package
    }

    #[must_use]
    pub const fn manifest_digest(&self) -> ContentDigest {
        self.manifest
    }
}

/// Owned bytes verified against the admitted release manifest, independent of later source edits.
pub struct EditorSourceToolReleaseMember {
    path: String,
    kind: EditorSourceToolReleaseMemberKind,
    digest: ContentDigest,
    bytes: Box<[u8]>,
}

impl EditorSourceToolReleaseMember {
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub const fn kind(&self) -> EditorSourceToolReleaseMemberKind {
        self.kind
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl fmt::Debug for EditorSourceToolReleaseMember {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditorSourceToolReleaseMember")
            .field("path", &self.path)
            .field("kind", &self.kind)
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}

/// A complete data-only local release candidate, not an installed or enabled executable.
///
/// Capture performs no writes, launches no child and does not admit metadata into a project.
/// The caller supplies the expected manifest digest from its selected source observation.
/// All declared payloads are copied through exact-spelling strict filesystem capabilities and
/// checked before this candidate is returned. Subsequent changes to the original directory do
/// not alter the candidate. The host runtime itself is outside this release's artifact closure.
pub struct PreparedEditorSourceToolRelease {
    package: String,
    release: String,
    host_target: String,
    source_root: FileIdentity,
    manifest_digest: ContentDigest,
    manifest_bytes: Box<[u8]>,
    owner: ComponentSchemaOwnerSnapshot,
    component: ComponentTypeId,
    entry: String,
    requires: Vec<EditorSourceToolReleaseRequirement>,
    members: Vec<EditorSourceToolReleaseMember>,
}

impl fmt::Debug for PreparedEditorSourceToolRelease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedEditorSourceToolRelease")
            .field("package", &self.package)
            .field("release", &self.release)
            .field("host_target", &self.host_target)
            .field("manifest_digest", &self.manifest_digest)
            .field("schema_owner", self.owner.owner_id())
            .field("component", &self.component)
            .field("entry", &self.entry)
            .field("requires", &self.requires)
            .field("member_count", &self.members.len())
            .finish_non_exhaustive()
    }
}

/// Code-free release admission failures. File names are bounded release-relative observations.
#[derive(Debug, thiserror::Error)]
pub enum EditorSourceToolReleaseError {
    #[error("local tool release capture requires a strict source capability")]
    SourceTrust,
    #[error("the selected manifest exceeds the local tool release allowance")]
    ManifestLimit,
    #[error("the local tool release manifest is invalid: {0}")]
    Manifest(&'static str),
    #[error("the release targets another Editor host")]
    HostTarget,
    #[error("the local tool release member does not match its declared digest: {member}")]
    Digest { member: String },
    #[error("the local tool release Schema does not admit its selected component")]
    Schema,
    #[error("the local tool release member could not be captured: {member}")]
    Filesystem {
        member: String,
        #[source]
        source: FsError,
    },
}

impl PreparedEditorSourceToolRelease {
    /// Reads `nara-tool.json` and every declared payload without executing package-controlled code.
    ///
    /// Fixed ceilings are 64 KiB manifest, 64 files, 16 MiB per payload and 64 MiB aggregate
    /// payload. Paths have at most eight ASCII components and 512 bytes. The catalog additionally
    /// passes the existing Reflect format and Schema admission limits. This first form accepts
    /// a generation-one catalog; it carries no predecessor lineage. This is a bounded native
    /// filesystem read, not a time limit, archive reader or RSS sandbox. Undeclared siblings are
    /// not read or captured and may never become implicit runtime dependencies of this candidate.
    pub fn capture(
        root: &DirectoryCapability,
        expected_manifest: ContentDigest,
    ) -> Result<Self, EditorSourceToolReleaseError> {
        if !matches!(
            root.options().trust(),
            TrustMode::Untrusted | TrustMode::Recovery
        ) {
            return Err(EditorSourceToolReleaseError::SourceTrust);
        }
        if expected_manifest.length() == 0 || expected_manifest.length() > MAX_MANIFEST_BYTES {
            return Err(EditorSourceToolReleaseError::ManifestLimit);
        }
        let manifest_bytes = capture_member(root, MANIFEST_NAME, expected_manifest)?;
        let manifest =
            Manifest::decode(&manifest_bytes).map_err(EditorSourceToolReleaseError::Manifest)?;
        if current_host_target() != Some(manifest.host_target.as_str()) {
            return Err(EditorSourceToolReleaseError::HostTarget);
        }

        let mut members = Vec::with_capacity(manifest.files.len());
        for member in manifest.files {
            let bytes = capture_member(root, &member.path, member.digest)?;
            members.push(EditorSourceToolReleaseMember {
                path: member.path,
                kind: match member.kind {
                    MemberKind::Tool => EditorSourceToolReleaseMemberKind::Tool,
                    MemberKind::Content => EditorSourceToolReleaseMemberKind::Content,
                },
                digest: member.digest,
                bytes,
            });
        }
        let catalog_bytes = members
            .iter()
            .find(|member| member.path == manifest.catalog)
            .ok_or(EditorSourceToolReleaseError::Schema)?
            .bytes();
        let catalog = ComponentSchemaCatalog::from_json_bytes(catalog_bytes)
            .map_err(|_| EditorSourceToolReleaseError::Schema)?;
        let owner = ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new(manifest.owner)
                .map_err(|_| EditorSourceToolReleaseError::Schema)?,
            catalog,
            None,
        )
        .map_err(|_| EditorSourceToolReleaseError::Schema)?;
        let component = ComponentTypeId::try_new(manifest.component)
            .map_err(|_| EditorSourceToolReleaseError::Schema)?;
        // Share the actual finite tool definition's admission contract, without a command or probe.
        super::validate_tool_component(&owner, &component)
            .map_err(|_| EditorSourceToolReleaseError::Schema)?;

        Ok(Self {
            package: manifest.package,
            release: manifest.release,
            host_target: manifest.host_target,
            source_root: root.identity(),
            manifest_digest: expected_manifest,
            manifest_bytes,
            owner,
            component,
            entry: manifest.entry,
            requires: manifest
                .requires
                .into_iter()
                .map(|requirement| EditorSourceToolReleaseRequirement {
                    package: requirement.package,
                    manifest: requirement.manifest,
                })
                .collect(),
            members,
        })
    }

    #[must_use]
    pub fn package(&self) -> &str {
        &self.package
    }

    #[must_use]
    pub fn release(&self) -> &str {
        &self.release
    }

    #[must_use]
    pub fn host_target(&self) -> &str {
        &self.host_target
    }

    /// Source observation only; this is not a durable trust ID or authority over installed files.
    #[must_use]
    pub const fn source_root_identity(&self) -> FileIdentity {
        self.source_root
    }

    #[must_use]
    pub const fn manifest_digest(&self) -> ContentDigest {
        self.manifest_digest
    }

    #[must_use]
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.manifest_bytes
    }

    #[must_use]
    pub fn schema_owner(&self) -> &ComponentSchemaOwnerSnapshot {
        &self.owner
    }

    #[must_use]
    pub fn component(&self) -> &ComponentTypeId {
        &self.component
    }

    #[must_use]
    pub fn entry(&self) -> &str {
        &self.entry
    }

    #[must_use]
    pub fn requires(&self) -> &[EditorSourceToolReleaseRequirement] {
        &self.requires
    }

    #[must_use]
    pub fn members(&self) -> &[EditorSourceToolReleaseMember] {
        &self.members
    }
}

fn capture_member(
    root: &DirectoryCapability,
    member: &str,
    expected: ContentDigest,
) -> Result<Box<[u8]>, EditorSourceToolReleaseError> {
    let read = || {
        let path = RelativePath::new(member)?;
        let file = root.open_file_exact(&path)?;
        file.read_to_end_bounded(expected.length())
    };
    let bytes = read().map_err(|source| EditorSourceToolReleaseError::Filesystem {
        member: member.to_owned(),
        source,
    })?;
    if ContentDigest::of_bytes(&bytes) != expected {
        return Err(EditorSourceToolReleaseError::Digest {
            member: member.to_owned(),
        });
    }
    Ok(bytes.into_boxed_slice())
}

fn current_host_target() -> Option<&'static str> {
    if cfg!(all(
        target_os = "windows",
        target_arch = "x86_64",
        target_env = "msvc"
    )) {
        Some("x86_64-pc-windows-msvc")
    } else if cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        Some("x86_64-unknown-linux-gnu")
    } else {
        None
    }
}

#[cfg(all(
    test,
    any(
        all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
        all(target_os = "linux", target_arch = "x86_64", target_env = "gnu")
    )
))]
mod tests;
