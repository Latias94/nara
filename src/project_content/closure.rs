use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    sync::Arc,
};

use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity};
use nara_project::ProjectPath;
use nara_reflect::SchemaCompositionFingerprint;

use super::{
    ProjectContentBudgetKind, ProjectContentError, ProjectContentErrorKind, ProjectContentLease,
    ProjectContentRevision, append_frame, checked_plan_sum, overflow_budget,
};
use crate::project_host::ProjectSettingsLineage;

/// The role of one source file in the admitted startup runtime closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupContentMemberKind {
    ProjectManifest,
    Scene,
    Prefab,
    AssetMetadata,
    AssetSource,
}

impl StartupContentMemberKind {
    pub(super) const fn digest_role(self) -> &'static str {
        match self {
            Self::ProjectManifest => "manifest",
            Self::Scene => "scene",
            Self::Prefab => "prefab",
            Self::AssetMetadata => "asset-meta",
            Self::AssetSource => "asset",
        }
    }
}

/// One exact source member admitted into the startup runtime closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupContentMemberReceipt {
    kind: StartupContentMemberKind,
    path: ProjectPath,
    digest: ContentDigest,
}

impl StartupContentMemberReceipt {
    #[must_use]
    pub const fn kind(&self) -> StartupContentMemberKind {
        self.kind
    }

    #[must_use]
    pub const fn path(&self) -> &ProjectPath {
        &self.path
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }

    #[must_use]
    pub const fn byte_len(&self) -> u64 {
        self.digest.length()
    }
}

/// Ordered, bounded source facts for one admitted startup runtime closure.
///
/// The receipt deliberately excludes dynamically requested or unreferenced project content. Its
/// member paths are relative to the exact authorized project root retained privately by the
/// receipt, so source-carrying export can reopen the admitted authority without accepting a second
/// caller-supplied source root. The capability is process-local and is never serialized as evidence.
#[derive(Clone)]
pub struct StartupContentClosureReceipt {
    inner: Arc<StartupContentClosureReceiptInner>,
}

pub(super) struct StartupContentClosureReceiptInner {
    project_root: DirectoryCapability,
    settings_lineage: ProjectSettingsLineage,
    manifest_digest: ContentDigest,
    schema_fingerprint: SchemaCompositionFingerprint,
    content_revision: ProjectContentRevision,
    content_digest: ContentDigest,
    runtime_directories: Box<[ProjectPath]>,
    members: Box<[StartupContentMemberReceipt]>,
    _lease: ProjectContentLease,
}

impl StartupContentClosureReceipt {
    #[must_use]
    pub fn settings_lineage(&self) -> ProjectSettingsLineage {
        self.inner.settings_lineage
    }

    #[must_use]
    pub fn manifest_digest(&self) -> ContentDigest {
        self.inner.manifest_digest
    }

    #[must_use]
    pub fn schema_fingerprint(&self) -> SchemaCompositionFingerprint {
        self.inner.schema_fingerprint
    }

    #[must_use]
    pub fn content_revision(&self) -> ProjectContentRevision {
        self.inner.content_revision
    }

    #[must_use]
    pub fn content_digest(&self) -> ContentDigest {
        self.inner.content_digest
    }

    #[must_use]
    pub fn members(&self) -> &[StartupContentMemberReceipt] {
        &self.inner.members
    }

    pub(crate) fn runtime_directories(&self) -> &[ProjectPath] {
        &self.inner.runtime_directories
    }

    #[must_use]
    pub fn project_root_identity(&self) -> FileIdentity {
        self.inner.project_root.identity()
    }

    pub(crate) fn project_root(&self) -> DirectoryCapability {
        self.inner.project_root.share()
    }
}

impl std::fmt::Debug for StartupContentClosureReceipt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StartupContentClosureReceipt")
            .field("project_root_identity", &self.project_root_identity())
            .field("settings_lineage", &self.inner.settings_lineage)
            .field("manifest_digest", &self.inner.manifest_digest)
            .field("schema_fingerprint", &self.inner.schema_fingerprint)
            .field("content_revision", &self.inner.content_revision)
            .field("content_digest", &self.inner.content_digest)
            .field(
                "runtime_directory_count",
                &self.inner.runtime_directories.len(),
            )
            .field("member_count", &self.inner.members.len())
            .finish()
    }
}

pub(super) struct StartupContentClosureDraft {
    settings_lineage: ProjectSettingsLineage,
    manifest_digest: ContentDigest,
    schema_fingerprint: SchemaCompositionFingerprint,
    content_revision: ProjectContentRevision,
    content_digest: ContentDigest,
    runtime_directories: Box<[ProjectPath]>,
    members: Box<[StartupContentMemberReceipt]>,
}

impl StartupContentClosureDraft {
    pub(super) fn members(&self) -> &[StartupContentMemberReceipt] {
        &self.members
    }

    pub(super) fn runtime_directories(&self) -> &[ProjectPath] {
        &self.runtime_directories
    }

    pub(super) fn publish(
        self,
        project_root: DirectoryCapability,
        lease: ProjectContentLease,
    ) -> StartupContentClosureReceipt {
        StartupContentClosureReceipt {
            inner: Arc::new(StartupContentClosureReceiptInner {
                project_root,
                settings_lineage: self.settings_lineage,
                manifest_digest: self.manifest_digest,
                schema_fingerprint: self.schema_fingerprint,
                content_revision: self.content_revision,
                content_digest: self.content_digest,
                runtime_directories: self.runtime_directories,
                members: self.members,
                _lease: lease,
            }),
        }
    }
}

pub(super) struct StartupContentClosureBuilder {
    manifest_digest: ContentDigest,
    runtime_directories: BTreeSet<String>,
    members: BTreeMap<String, StartupContentMemberFacts>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct StartupContentMemberFacts {
    kind: StartupContentMemberKind,
    digest: ContentDigest,
}

impl StartupContentClosureBuilder {
    pub(super) fn new(manifest_digest: ContentDigest) -> Result<Self, ProjectContentError> {
        let mut builder = Self {
            manifest_digest,
            runtime_directories: BTreeSet::new(),
            members: BTreeMap::new(),
        };
        builder.insert_path(
            StartupContentMemberKind::ProjectManifest,
            ProjectPath::new("nara.toml").expect("the engine-owned manifest path is valid"),
            manifest_digest,
        )?;
        Ok(builder)
    }

    pub(super) fn set_runtime_directories(&mut self, paths: [&ProjectPath; 3]) {
        self.runtime_directories
            .extend(paths.into_iter().map(|path| path.as_str().to_owned()));
    }

    pub(super) fn insert(
        &mut self,
        kind: StartupContentMemberKind,
        directory: &ProjectPath,
        path: &str,
        digest: ContentDigest,
    ) -> Result<(), ProjectContentError> {
        let path = ProjectPath::new(format!("{}/{path}", directory.as_str())).map_err(|_| {
            ProjectContentError::single(
                ProjectContentErrorKind::HostAuthorityRejected,
                "project.content.closure-path-invalid",
                "Project startup closure contains an invalid logical path",
            )
        })?;
        self.insert_path(kind, path, digest)
    }

    pub(super) fn content_digest_framed_bytes(&self) -> Result<usize, ProjectContentError> {
        let mut total = b"nara.project-startup-content-digest.v2\0".len();
        for (path, facts) in self
            .members
            .iter()
            .filter(|(_, facts)| facts.kind != StartupContentMemberKind::ProjectManifest)
        {
            total = checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [
                    total,
                    8,
                    facts.kind.digest_role().len(),
                    8,
                    path.len(),
                    8,
                    32,
                ],
            )?;
        }
        Ok(total)
    }

    pub(super) fn content_digest(
        &self,
        framed_bytes: usize,
    ) -> Result<ContentDigest, ProjectContentError> {
        let mut framed = Vec::new();
        framed.try_reserve_exact(framed_bytes).map_err(|_| {
            ProjectContentError::single(
                ProjectContentErrorKind::AllocationFailed,
                "project.content.digest-allocation-failed",
                "Project content digest allocation failed",
            )
        })?;
        let mut encoded_length = 0_u64;
        framed.extend_from_slice(b"nara.project-startup-content-digest.v2\0");
        for (path, facts) in self
            .members
            .iter()
            .filter(|(_, facts)| facts.kind != StartupContentMemberKind::ProjectManifest)
        {
            append_frame(&mut framed, facts.kind.digest_role().as_bytes())?;
            append_frame(&mut framed, path.as_bytes())?;
            framed.extend_from_slice(&facts.digest.length().to_le_bytes());
            framed.extend_from_slice(facts.digest.as_bytes());
            encoded_length = encoded_length
                .checked_add(facts.digest.length())
                .ok_or_else(|| overflow_budget(ProjectContentBudgetKind::EncodedBytes))?;
        }
        debug_assert_eq!(framed.len(), framed_bytes);
        let hash = ContentDigest::of_bytes(&framed);
        Ok(ContentDigest::from_parts(encoded_length, *hash.as_bytes()))
    }

    fn insert_path(
        &mut self,
        kind: StartupContentMemberKind,
        path: ProjectPath,
        digest: ContentDigest,
    ) -> Result<(), ProjectContentError> {
        let key = path.as_str().to_owned();
        let facts = StartupContentMemberFacts { kind, digest };
        match self.members.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(facts);
                Ok(())
            }
            Entry::Occupied(entry) if entry.get() == &facts => Ok(()),
            Entry::Occupied(_) => Err(ProjectContentError::single(
                ProjectContentErrorKind::ClosurePathCollision,
                "project.content.closure-path-collision",
                "Project startup closure assigns conflicting facts to one logical path",
            )),
        }
    }

    pub(super) fn finish(
        self,
        settings_lineage: ProjectSettingsLineage,
        schema_fingerprint: SchemaCompositionFingerprint,
        content_revision: ProjectContentRevision,
        content_digest: ContentDigest,
    ) -> StartupContentClosureDraft {
        StartupContentClosureDraft {
            settings_lineage,
            manifest_digest: self.manifest_digest,
            schema_fingerprint,
            content_revision,
            content_digest,
            runtime_directories: self
                .runtime_directories
                .into_iter()
                .map(|path| {
                    ProjectPath::new(path)
                        .expect("runtime directory paths were validated before admission")
                })
                .collect(),
            members: self
                .members
                .into_iter()
                .map(|(path, facts)| StartupContentMemberReceipt {
                    kind: facts.kind,
                    path: ProjectPath::new(path)
                        .expect("startup closure paths were validated before admission"),
                    digest: facts.digest,
                })
                .collect(),
        }
    }
}
