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

/// The role of one source file in an admitted Scene content closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectContentMemberKind {
    ProjectManifest,
    AuthoringSchemaBundle,
    Scene,
    Prefab,
    InputActionMap,
    AssetMetadata,
    AssetSource,
}

impl ProjectContentMemberKind {
    pub(super) const fn digest_role(self) -> &'static str {
        match self {
            Self::ProjectManifest => "manifest",
            Self::AuthoringSchemaBundle => "authoring-schema-bundle",
            Self::Scene => "scene",
            Self::Prefab => "prefab",
            Self::InputActionMap => "input-action-map",
            Self::AssetMetadata => "asset-meta",
            Self::AssetSource => "asset",
        }
    }
}

/// One exact source member admitted into a Scene content closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContentMemberReceipt {
    kind: ProjectContentMemberKind,
    path: ProjectPath,
    digest: ContentDigest,
}

impl ProjectContentMemberReceipt {
    #[must_use]
    pub const fn kind(&self) -> ProjectContentMemberKind {
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

/// Ordered, bounded source facts for one explicitly selected Scene content closure.
///
/// This receipt retains source facts and their budget, not decoded Scene or Image values. Its
/// member paths are relative to the exact project root retained privately by the receipt. The
/// selected Scene path is relative to the project's configured Scenes directory. Neither this
/// receipt nor its clonable source authority establishes runtime residency or delivery admission.
#[derive(Clone)]
pub struct ProjectContentClosureReceipt {
    inner: Arc<ProjectContentClosureReceiptInner>,
}

pub(super) struct ProjectContentClosureReceiptInner {
    project_root: DirectoryCapability,
    selected_scene_path: ProjectPath,
    is_startup: bool,
    settings_lineage: ProjectSettingsLineage,
    manifest_digest: ContentDigest,
    schema_fingerprint: SchemaCompositionFingerprint,
    content_revision: ProjectContentRevision,
    content_digest: ContentDigest,
    closure_plan_digest: ContentDigest,
    runtime_directories: Box<[ProjectPath]>,
    members: Box<[ProjectContentMemberReceipt]>,
    _lease: ProjectContentLease,
}

impl ProjectContentClosureReceipt {
    #[must_use]
    pub fn selected_scene_path(&self) -> &ProjectPath {
        &self.inner.selected_scene_path
    }

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

    /// Returns the exact ordered runtime-directory and source-member plan admitted by this load.
    ///
    /// Unlike [`Self::content_digest`], this digest includes the manifest, authoring schema bundle,
    /// runtime directory paths, and every member kind/path/digest tuple. Product and export
    /// adapters use it to prove that an executable started from the same source-carrying closure
    /// that the export transaction will publish.
    #[doc(hidden)]
    #[must_use]
    pub fn closure_plan_digest(&self) -> ContentDigest {
        self.inner.closure_plan_digest
    }

    #[must_use]
    pub fn members(&self) -> &[ProjectContentMemberReceipt] {
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

    pub(crate) fn has_runtime_schema_agreement(&self) -> bool {
        self.runtime_schema_bundle_digest().is_some()
    }

    pub(crate) fn runtime_schema_bundle_digest(&self) -> Option<ContentDigest> {
        self.inner
            .members
            .iter()
            .find(|member| member.kind() == ProjectContentMemberKind::AuthoringSchemaBundle)
            .map(ProjectContentMemberReceipt::digest)
    }

    pub(super) fn startup_receipt(&self) -> Option<StartupContentClosureReceipt> {
        self.inner.is_startup.then(|| StartupContentClosureReceipt {
            content: self.clone(),
        })
    }
}

impl std::fmt::Debug for ProjectContentClosureReceipt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectContentClosureReceipt")
            .field("project_root_identity", &self.project_root_identity())
            .field("selected_scene_path", self.selected_scene_path())
            .field("settings_lineage", &self.inner.settings_lineage)
            .field("manifest_digest", &self.inner.manifest_digest)
            .field("schema_fingerprint", &self.inner.schema_fingerprint)
            .field("content_revision", &self.inner.content_revision)
            .field("content_digest", &self.inner.content_digest)
            .field("closure_plan_digest", &self.inner.closure_plan_digest)
            .field(
                "runtime_directory_count",
                &self.inner.runtime_directories.len(),
            )
            .field("member_count", &self.inner.members.len())
            .field(
                "has_runtime_schema_agreement",
                &self.has_runtime_schema_agreement(),
            )
            .finish()
    }
}

/// The exact default Scene closure admitted by the project content owner.
///
/// Explicitly selected non-default Scenes cannot create this startup attestation. The wrapper
/// shares the generic receipt's source facts and budget without retaining decoded content.
#[derive(Clone)]
pub struct StartupContentClosureReceipt {
    content: ProjectContentClosureReceipt,
}

impl StartupContentClosureReceipt {
    #[must_use]
    pub fn content_closure(&self) -> &ProjectContentClosureReceipt {
        &self.content
    }

    #[must_use]
    pub fn settings_lineage(&self) -> ProjectSettingsLineage {
        self.content.settings_lineage()
    }

    #[must_use]
    pub fn manifest_digest(&self) -> ContentDigest {
        self.content.manifest_digest()
    }

    #[must_use]
    pub fn schema_fingerprint(&self) -> SchemaCompositionFingerprint {
        self.content.schema_fingerprint()
    }

    #[must_use]
    pub fn content_revision(&self) -> ProjectContentRevision {
        self.content.content_revision()
    }

    #[must_use]
    pub fn content_digest(&self) -> ContentDigest {
        self.content.content_digest()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn closure_plan_digest(&self) -> ContentDigest {
        self.content.closure_plan_digest()
    }

    #[must_use]
    pub fn members(&self) -> &[ProjectContentMemberReceipt] {
        self.content.members()
    }

    #[must_use]
    pub fn project_root_identity(&self) -> FileIdentity {
        self.content.project_root_identity()
    }

    pub(crate) fn runtime_directories(&self) -> &[ProjectPath] {
        self.content.runtime_directories()
    }

    pub(crate) fn project_root(&self) -> DirectoryCapability {
        self.content.project_root()
    }

    pub(crate) fn has_runtime_schema_agreement(&self) -> bool {
        self.content.has_runtime_schema_agreement()
    }

    pub(crate) fn runtime_schema_bundle_digest(&self) -> Option<ContentDigest> {
        self.content.runtime_schema_bundle_digest()
    }
}

impl std::fmt::Debug for StartupContentClosureReceipt {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StartupContentClosureReceipt")
            .field("content", &self.content)
            .finish()
    }
}

pub(super) struct ProjectContentClosureDraft {
    selected_scene_path: ProjectPath,
    is_startup: bool,
    settings_lineage: ProjectSettingsLineage,
    manifest_digest: ContentDigest,
    schema_fingerprint: SchemaCompositionFingerprint,
    content_revision: ProjectContentRevision,
    content_digest: ContentDigest,
    closure_plan_digest: ContentDigest,
    runtime_directories: Box<[ProjectPath]>,
    members: Box<[ProjectContentMemberReceipt]>,
}

impl ProjectContentClosureDraft {
    pub(super) fn selected_scene_path(&self) -> &ProjectPath {
        &self.selected_scene_path
    }

    pub(super) fn members(&self) -> &[ProjectContentMemberReceipt] {
        &self.members
    }

    pub(super) fn runtime_directories(&self) -> &[ProjectPath] {
        &self.runtime_directories
    }

    pub(super) fn publish(
        self,
        project_root: DirectoryCapability,
        lease: ProjectContentLease,
    ) -> ProjectContentClosureReceipt {
        ProjectContentClosureReceipt {
            inner: Arc::new(ProjectContentClosureReceiptInner {
                project_root,
                selected_scene_path: self.selected_scene_path,
                is_startup: self.is_startup,
                settings_lineage: self.settings_lineage,
                manifest_digest: self.manifest_digest,
                schema_fingerprint: self.schema_fingerprint,
                content_revision: self.content_revision,
                content_digest: self.content_digest,
                closure_plan_digest: self.closure_plan_digest,
                runtime_directories: self.runtime_directories,
                members: self.members,
                _lease: lease,
            }),
        }
    }
}

pub(super) struct ProjectContentClosureBuilder {
    manifest_digest: ContentDigest,
    runtime_directories: BTreeSet<String>,
    members: BTreeMap<String, ProjectContentMemberFacts>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ProjectContentMemberFacts {
    kind: ProjectContentMemberKind,
    digest: ContentDigest,
}

impl ProjectContentClosureBuilder {
    pub(super) fn new(manifest_digest: ContentDigest) -> Result<Self, ProjectContentError> {
        let mut builder = Self {
            manifest_digest,
            runtime_directories: BTreeSet::new(),
            members: BTreeMap::new(),
        };
        builder.insert_path(
            ProjectContentMemberKind::ProjectManifest,
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
        kind: ProjectContentMemberKind,
        directory: &ProjectPath,
        path: &str,
        digest: ContentDigest,
    ) -> Result<(), ProjectContentError> {
        let path = ProjectPath::new(format!("{}/{path}", directory.as_str())).map_err(|_| {
            ProjectContentError::single(
                ProjectContentErrorKind::HostAuthorityRejected,
                "project.content.closure-path-invalid",
                "Project Scene closure contains an invalid logical path",
            )
        })?;
        self.insert_path(kind, path, digest)
    }

    pub(super) fn content_digest_framed_bytes(&self) -> Result<usize, ProjectContentError> {
        let mut total = b"nara.project-startup-content-digest.v2\0".len();
        for (path, facts) in self
            .members
            .iter()
            .filter(|(_, facts)| facts.kind.contributes_to_runtime_content_digest())
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
            .filter(|(_, facts)| facts.kind.contributes_to_runtime_content_digest())
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

    pub(super) fn insert_project_path(
        &mut self,
        kind: ProjectContentMemberKind,
        path: ProjectPath,
        digest: ContentDigest,
    ) -> Result<(), ProjectContentError> {
        self.insert_path(kind, path, digest)
    }

    fn insert_path(
        &mut self,
        kind: ProjectContentMemberKind,
        path: ProjectPath,
        digest: ContentDigest,
    ) -> Result<(), ProjectContentError> {
        let key = path.as_str().to_owned();
        let facts = ProjectContentMemberFacts { kind, digest };
        match self.members.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(facts);
                Ok(())
            }
            Entry::Occupied(entry) if entry.get() == &facts => Ok(()),
            Entry::Occupied(_) => Err(ProjectContentError::single(
                ProjectContentErrorKind::ClosurePathCollision,
                "project.content.closure-path-collision",
                "Project Scene closure assigns conflicting facts to one logical path",
            )),
        }
    }

    pub(super) fn finish(
        self,
        selected_scene_path: ProjectPath,
        is_startup: bool,
        settings_lineage: ProjectSettingsLineage,
        schema_fingerprint: SchemaCompositionFingerprint,
        content_revision: ProjectContentRevision,
        content_digest: ContentDigest,
    ) -> ProjectContentClosureDraft {
        let runtime_directories = self
            .runtime_directories
            .into_iter()
            .map(|path| {
                ProjectPath::new(path)
                    .expect("runtime directory paths were validated before admission")
            })
            .collect::<Box<[_]>>();
        let members = self
            .members
            .into_iter()
            .map(|(path, facts)| ProjectContentMemberReceipt {
                kind: facts.kind,
                path: ProjectPath::new(path)
                    .expect("Scene closure paths were validated before admission"),
                digest: facts.digest,
            })
            .collect::<Box<[_]>>();
        let closure_plan_digest = closure_plan_digest(&runtime_directories, &members);
        ProjectContentClosureDraft {
            selected_scene_path,
            is_startup,
            settings_lineage,
            manifest_digest: self.manifest_digest,
            schema_fingerprint,
            content_revision,
            content_digest,
            closure_plan_digest,
            runtime_directories,
            members,
        }
    }
}

fn closure_plan_digest(
    runtime_directories: &[ProjectPath],
    members: &[ProjectContentMemberReceipt],
) -> ContentDigest {
    let mut framed = Vec::new();
    framed.extend_from_slice(b"nara.project-startup-closure-plan.v1\0");
    for directory in runtime_directories {
        append_plan_frame(&mut framed, b"runtime-directory");
        append_plan_frame(&mut framed, directory.as_str().as_bytes());
    }
    for member in members {
        append_plan_frame(&mut framed, member.kind.digest_role().as_bytes());
        append_plan_frame(&mut framed, member.path.as_str().as_bytes());
        append_plan_frame(&mut framed, &member.digest.length().to_le_bytes());
        append_plan_frame(&mut framed, member.digest.as_bytes());
    }
    ContentDigest::of_bytes(&framed)
}

fn append_plan_frame(target: &mut Vec<u8>, bytes: &[u8]) {
    target.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    target.extend_from_slice(bytes);
}

impl ProjectContentMemberKind {
    const fn contributes_to_runtime_content_digest(self) -> bool {
        !matches!(self, Self::ProjectManifest | Self::AuthoringSchemaBundle)
    }
}
