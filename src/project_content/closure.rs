use std::{
    collections::{BTreeMap, BTreeSet, btree_map::Entry},
    sync::Arc,
};

use crate::fs::{ContentDigest, DirectoryCapability, FileIdentity};
use nara_project::ProjectPath;
use nara_reflect::SchemaCompositionFingerprint;
pub use nara_scene::current_scene::SceneSourceStamp as ProjectSceneContentStamp;

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
    /// Source bytes with no more specific role in this closure.
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

    fn merge(self, other: Self) -> Option<Self> {
        match (self, other) {
            (Self::AssetSource, typed) | (typed, Self::AssetSource) => Some(typed),
            (left, right) if left == right => Some(left),
            _ => None,
        }
    }
}

/// One exact source member admitted into a Scene content closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectContentMemberReceipt {
    facts: ProjectContentMemberFacts,
    path: ProjectPath,
}

impl ProjectContentMemberReceipt {
    pub(super) const fn new(
        kind: ProjectContentMemberKind,
        path: ProjectPath,
        digest: ContentDigest,
    ) -> Self {
        Self {
            facts: ProjectContentMemberFacts::new(kind, digest),
            path,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> ProjectContentMemberKind {
        self.facts.kind
    }

    #[must_use]
    pub const fn path(&self) -> &ProjectPath {
        &self.path
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.facts.digest
    }

    #[must_use]
    pub const fn byte_len(&self) -> u64 {
        self.digest().length()
    }

    /// Combines uses of one project-relative file under a shared project authority.
    pub(crate) fn merge_usage(&mut self, member: &Self) -> bool {
        self.path == member.path && self.facts.merge(member.facts)
    }
}

/// Ordered, bounded source facts for one explicitly selected Scene content closure.
///
/// This receipt retains source facts and their budget, not decoded Scene or Image values. Its
/// member paths are relative to the exact project root retained privately by the receipt. The
/// selected Scene path is relative to its bound source directory. Neither this
/// receipt nor its clonable source authority establishes runtime residency or delivery admission.
/// These are frozen observations, not a live filesystem validity claim. Bodies needed only by
/// Deferred references are excluded; a later target request resolves its own current inputs. Strict
/// saved-source delivery independently validates the complete mandatory content closure.
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
    /// Copies the original admitted content observations without retaining this receipt's budget.
    #[must_use]
    pub fn stamp(&self) -> ProjectSceneContentStamp {
        ProjectSceneContentStamp::new(
            self.inner.schema_fingerprint,
            self.inner.manifest_digest,
            self.inner.content_digest,
            self.inner.closure_plan_digest,
        )
    }

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
    /// runtime directory paths, and every member kind/path/digest tuple. It covers the admitted
    /// startup closure. Deferred bodies and physical file incarnations require their own checks.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProjectContentMemberFacts {
    kind: ProjectContentMemberKind,
    digest: ContentDigest,
    raw_source: bool,
}

impl ProjectContentMemberFacts {
    const fn new(kind: ProjectContentMemberKind, digest: ContentDigest) -> Self {
        Self {
            kind,
            digest,
            raw_source: matches!(kind, ProjectContentMemberKind::AssetSource),
        }
    }

    fn merge(&mut self, other: Self) -> bool {
        if self.digest != other.digest {
            return false;
        }
        let Some(kind) = self.kind.merge(other.kind) else {
            return false;
        };
        self.kind = kind;
        self.raw_source |= other.raw_source;
        true
    }

    const fn runtime_kind(self) -> Option<ProjectContentMemberKind> {
        if self.kind.contributes_to_runtime_content_digest() {
            Some(self.kind)
        } else if self.raw_source {
            Some(ProjectContentMemberKind::AssetSource)
        } else {
            None
        }
    }
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

    #[cfg(feature = "project-tooling")]
    pub(super) fn supplemental_files(&self) -> impl Iterator<Item = (&str, ContentDigest)> {
        self.members.iter().filter_map(|(path, facts)| {
            matches!(
                facts.kind,
                ProjectContentMemberKind::AssetMetadata | ProjectContentMemberKind::AssetSource
            )
            .then_some((path.as_str(), facts.digest))
        })
    }

    #[cfg(feature = "project-tooling")]
    pub(super) fn member_digest(&self, path: &str) -> Option<ContentDigest> {
        self.members.get(path).map(|facts| facts.digest)
    }

    pub(super) fn insert(
        &mut self,
        kind: ProjectContentMemberKind,
        directory: &ProjectPath,
        path: &str,
        digest: ContentDigest,
    ) -> Result<bool, ProjectContentError> {
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
        for (path, facts) in &self.members {
            let Some(kind) = facts.runtime_kind() else {
                continue;
            };
            total = checked_plan_sum(
                ProjectContentBudgetKind::WorkBytes,
                [total, 8, kind.digest_role().len(), 8, path.len(), 8, 32],
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
        for (path, facts) in &self.members {
            let Some(kind) = facts.runtime_kind() else {
                continue;
            };
            append_frame(&mut framed, kind.digest_role().as_bytes())?;
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
    ) -> Result<bool, ProjectContentError> {
        self.insert_path(kind, path, digest)
    }

    fn insert_path(
        &mut self,
        kind: ProjectContentMemberKind,
        path: ProjectPath,
        digest: ContentDigest,
    ) -> Result<bool, ProjectContentError> {
        let key = path.as_str().to_owned();
        let facts = ProjectContentMemberFacts::new(kind, digest);
        match self.members.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(facts);
                Ok(true)
            }
            Entry::Occupied(mut entry) => {
                if entry.get_mut().merge(facts) {
                    return Ok(false);
                }
                Err(ProjectContentError::single(
                    ProjectContentErrorKind::ClosurePathCollision,
                    "project.content.closure-path-collision",
                    "Project Scene closure assigns conflicting facts to one logical path",
                ))
            }
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
                facts,
                path: ProjectPath::new(path)
                    .expect("Scene closure paths were validated before admission"),
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
        append_plan_frame(&mut framed, member.kind().digest_role().as_bytes());
        append_plan_frame(&mut framed, member.path.as_str().as_bytes());
        append_plan_frame(&mut framed, &member.digest().length().to_le_bytes());
        append_plan_frame(&mut framed, member.digest().as_bytes());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_usage_merge_rejects_different_paths_bytes_and_typed_roles() {
        use ProjectContentMemberKind::{AssetSource, InputActionMap, Scene};

        let path = ProjectPath::new("assets/input.json").unwrap();
        let digest = ContentDigest::of_bytes(b"original");
        let input = ProjectContentMemberReceipt::new(InputActionMap, path.clone(), digest);
        let mut merged = input.clone();
        assert!(merged.merge_usage(&ProjectContentMemberReceipt::new(
            AssetSource,
            path.clone(),
            digest,
        )));
        let expected = merged.clone();

        for rejected in [
            ProjectContentMemberReceipt::new(
                AssetSource,
                ProjectPath::new("assets/other.json").unwrap(),
                digest,
            ),
            ProjectContentMemberReceipt::new(
                AssetSource,
                path.clone(),
                ContentDigest::of_bytes(b"modified"),
            ),
            ProjectContentMemberReceipt::new(Scene, path, digest),
        ] {
            assert!(!merged.merge_usage(&rejected));
            assert_eq!(merged, expected);
        }
    }

    #[test]
    fn an_authoring_member_used_as_raw_data_still_contributes_to_runtime_content() {
        use ProjectContentMemberKind::{AssetSource, AuthoringSchemaBundle, InputActionMap};

        let manifest = ContentDigest::of_bytes(b"manifest");
        let bytes = ContentDigest::of_bytes(b"schema bundle read by an importer");
        let path = ProjectPath::new("assets/schema.bundle.json").unwrap();
        let mut builder = ProjectContentClosureBuilder::new(manifest).unwrap();
        builder
            .insert_project_path(AuthoringSchemaBundle, path.clone(), bytes)
            .unwrap();
        let digest = |builder: &ProjectContentClosureBuilder| {
            builder
                .content_digest(builder.content_digest_framed_bytes().unwrap())
                .unwrap()
        };
        let authoring_only = digest(&builder);
        builder
            .insert_project_path(AssetSource, path.clone(), bytes)
            .unwrap();
        let shared = digest(&builder);
        assert_ne!(shared, authoring_only);

        let mut reversed = ProjectContentClosureBuilder::new(manifest).unwrap();
        reversed
            .insert_project_path(AssetSource, path.clone(), bytes)
            .unwrap();
        assert_eq!(shared, digest(&reversed));
        reversed
            .insert_project_path(AuthoringSchemaBundle, path.clone(), bytes)
            .unwrap();
        assert_eq!(shared, digest(&reversed));
        for (kind, bytes) in [(InputActionMap, bytes), (AssetSource, manifest)] {
            let error = reversed
                .insert_project_path(kind, path.clone(), bytes)
                .unwrap_err();
            assert_eq!(error.kind(), ProjectContentErrorKind::ClosurePathCollision);
        }
        assert_eq!(shared, digest(&reversed));
    }
}
