use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    mem::size_of,
    sync::Arc,
};

use nara_asset::{
    AssetDatabaseError, AssetMetaFileLimits, AssetMetaFormatError, AssetMetaIdentityCandidate,
    AssetPath, AssetPathError, AssetRecord, AssetRef, AssetSourceKind, ProjectAssetDatabase,
};
use nara_core::ByteLimit;
use nara_fs::{
    ContentDigest, DirectoryCapability, FileCapability, FileIdentity, FsError, PathValidationError,
    RelativePath,
};
use nara_project::{EffectiveProjectSettings, ProjectPath, ProjectPathError};
use nara_reflect::AuthoringSchemaBundle;
use nara_scene::__private::{
    AuthoringAdapterReceipt, fork_published_authoring_prefab_document,
    fork_published_authoring_scene_document,
};
use nara_scene::{
    AuthoringDocumentPublicationError, AuthoringDocumentReadiness, AuthoringRecordFact,
    PrefabDocument, PublishedAuthoringPrefabDocument, PublishedAuthoringSceneDocument,
    SceneDocument, SceneEntityRecord, SceneFileLimits, SceneFormatError, ScenePatchDocument,
    ScenePatchOperation, UnavailableRecordManifestEntry,
};

use super::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
    AuthoringProjectAdmissionUsage, AuthoringProjectAttemptGeneration, AuthoringSchemaReceipt,
    document::{AuthoringDocumentDataError, admit_prefab_bytes, admit_scene_bytes},
    source_index::ProjectSourceIndexState,
};

mod catalogue;
use catalogue::{ReferenceCatalogue, charge_database_record, insert_asset_record};

const SOURCE_OPEN_HANDLE_PEAK: usize = 7;
const VALUE_NODE_RETAINED_BYTES: usize = 32;
const AUTHORING_RECORD_RETAINED_OVERHEAD: usize = 64;
const AUTHORING_BASIS_ENCODED_MULTIPLIER: usize = 4;
const DEGRADED_ISSUE_RETAINED_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum AuthoringSourceKind {
    Scene,
    Prefab,
    AssetMetadata,
    AssetSource,
}

impl Display for AuthoringSourceKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Scene => "Scene",
            Self::Prefab => "Prefab",
            Self::AssetMetadata => "asset metadata",
            Self::AssetSource => "asset source",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthoringSourceFact {
    kind: AuthoringSourceKind,
    project_path: ProjectPath,
    identity: FileIdentity,
    parent_identity: Option<FileIdentity>,
    digest: ContentDigest,
    encoded_bytes: usize,
    dependency_depth: usize,
}

impl AuthoringSourceFact {
    #[must_use]
    pub(crate) const fn kind(&self) -> AuthoringSourceKind {
        self.kind
    }

    #[must_use]
    pub(crate) const fn project_path(&self) -> &ProjectPath {
        &self.project_path
    }

    #[must_use]
    pub(crate) const fn identity(&self) -> FileIdentity {
        self.identity
    }

    #[must_use]
    pub(crate) const fn parent_identity(&self) -> Option<FileIdentity> {
        self.parent_identity
    }

    #[must_use]
    pub(crate) const fn digest(&self) -> ContentDigest {
        self.digest
    }

    #[must_use]
    pub(crate) const fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }

    #[must_use]
    pub(crate) const fn dependency_depth(&self) -> usize {
        self.dependency_depth
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthoringProjectReadiness {
    Complete,
    Degraded,
}

impl AuthoringProjectReadiness {
    const fn from_document(readiness: AuthoringDocumentReadiness) -> Self {
        match readiness {
            AuthoringDocumentReadiness::Complete => Self::Complete,
            AuthoringDocumentReadiness::Degraded => Self::Degraded,
        }
    }

    const fn include(self, readiness: AuthoringDocumentReadiness) -> Self {
        match (self, readiness) {
            (Self::Degraded, _) | (_, AuthoringDocumentReadiness::Degraded) => Self::Degraded,
            (Self::Complete, AuthoringDocumentReadiness::Complete) => Self::Complete,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthoringUnresolvedDependencyKind {
    Prefab,
    Asset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthoringUnresolvedDependency {
    kind: AuthoringUnresolvedDependencyKind,
    reference: AssetRef,
    dependency_depth: usize,
}

impl AuthoringUnresolvedDependency {
    #[must_use]
    pub(crate) const fn kind(&self) -> AuthoringUnresolvedDependencyKind {
        self.kind
    }

    #[must_use]
    pub(crate) const fn reference(&self) -> &AssetRef {
        &self.reference
    }

    #[must_use]
    pub(crate) const fn dependency_depth(&self) -> usize {
        self.dependency_depth
    }
}

pub(crate) struct AuthoringPrefabSource {
    path: AssetPath,
    source: Arc<AuthoringSourceFact>,
    document: Arc<PublishedAuthoringPrefabDocument>,
}

pub(crate) struct AuthoringAssetSource {
    record: AssetRecord,
    metadata: AssetMetaIdentityCandidate,
    metadata_source: Arc<AuthoringSourceFact>,
    source: Arc<AuthoringSourceFact>,
    bytes: Arc<[u8]>,
}

impl AuthoringAssetSource {
    #[must_use]
    pub(crate) const fn path(&self) -> &AssetPath {
        self.record.path()
    }

    #[must_use]
    pub(crate) const fn record(&self) -> &AssetRecord {
        &self.record
    }

    #[must_use]
    pub(crate) const fn metadata(&self) -> &AssetMetaIdentityCandidate {
        &self.metadata
    }

    #[must_use]
    pub(crate) fn metadata_source(&self) -> &AuthoringSourceFact {
        &self.metadata_source
    }

    #[must_use]
    pub(crate) fn source(&self) -> &AuthoringSourceFact {
        &self.source
    }

    #[must_use]
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl Debug for AuthoringAssetSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringAssetSource")
            .field("path", self.path())
            .field("source_kind", self.record.source_kind())
            .field("metadata_source", &self.metadata_source)
            .field("source", &self.source)
            .field("encoded_bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

impl AuthoringPrefabSource {
    #[must_use]
    pub(crate) const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub(crate) fn source(&self) -> &AuthoringSourceFact {
        &self.source
    }

    #[must_use]
    pub(crate) fn document(&self) -> &PublishedAuthoringPrefabDocument {
        &self.document
    }
}

impl Debug for AuthoringPrefabSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringPrefabSource")
            .field("path", &self.path)
            .field("source", &self.source)
            .field("readiness", &self.document.readiness())
            .finish_non_exhaustive()
    }
}

struct AuthoringProjectSnapshotInner {
    schema: AuthoringSchemaReceipt,
    adapters: Box<[AuthoringAdapterReceipt]>,
    scene_limits: SceneFileLimits,
    readiness: AuthoringProjectReadiness,
    startup_source: Arc<AuthoringSourceFact>,
    startup_scene: Arc<PublishedAuthoringSceneDocument>,
    prefabs: Box<[AuthoringPrefabSource]>,
    assets: Box<[AuthoringAssetSource]>,
    asset_database: Arc<ProjectAssetDatabase>,
    unresolved_dependencies: Box<[AuthoringUnresolvedDependency]>,
    source_facts: Box<[Arc<AuthoringSourceFact>]>,
    usage: AuthoringProjectAdmissionUsage,
}

#[derive(Clone)]
pub(crate) struct AuthoringProjectSnapshot {
    inner: Arc<AuthoringProjectSnapshotInner>,
}

impl AuthoringProjectSnapshot {
    #[must_use]
    pub(crate) fn attempt_generation(&self) -> AuthoringProjectAttemptGeneration {
        self.inner.schema.attempt_generation()
    }

    #[must_use]
    pub(crate) fn settings(&self) -> &EffectiveProjectSettings {
        self.inner.schema.settings()
    }

    #[must_use]
    pub(crate) fn project_root(&self) -> DirectoryCapability {
        self.inner.schema.project_root()
    }

    #[must_use]
    pub(crate) fn manifest_digest(&self) -> ContentDigest {
        self.inner.schema.manifest_digest()
    }

    #[must_use]
    pub(crate) fn authoring_bundle_path(&self) -> &ProjectPath {
        self.inner.schema.bundle_path()
    }

    #[must_use]
    pub(crate) fn authoring_bundle_digest(&self) -> ContentDigest {
        self.inner.schema.bundle_digest()
    }

    #[must_use]
    pub(crate) fn schema_fingerprint_hex(&self) -> String {
        self.inner.schema.composition_fingerprint().to_hex()
    }

    #[must_use]
    pub(crate) fn root_identity(&self) -> FileIdentity {
        self.inner.schema.root_identity()
    }

    #[must_use]
    pub(crate) fn bundle(&self) -> &AuthoringSchemaBundle {
        self.inner.schema.bundle()
    }

    #[must_use]
    pub(crate) fn readiness(&self) -> AuthoringProjectReadiness {
        self.inner.readiness
    }

    #[must_use]
    pub(crate) fn startup_source(&self) -> &AuthoringSourceFact {
        &self.inner.startup_source
    }

    #[must_use]
    pub(crate) fn startup_scene(&self) -> &PublishedAuthoringSceneDocument {
        &self.inner.startup_scene
    }

    #[must_use]
    pub(crate) fn prefabs(&self) -> &[AuthoringPrefabSource] {
        &self.inner.prefabs
    }

    #[must_use]
    pub(crate) fn assets(&self) -> &[AuthoringAssetSource] {
        &self.inner.assets
    }

    #[must_use]
    pub(crate) fn unresolved_dependencies(&self) -> &[AuthoringUnresolvedDependency] {
        &self.inner.unresolved_dependencies
    }

    pub(crate) fn source_facts(&self) -> impl ExactSizeIterator<Item = &AuthoringSourceFact> {
        self.inner.source_facts.iter().map(Arc::as_ref)
    }

    #[must_use]
    pub(crate) fn usage(&self) -> AuthoringProjectAdmissionUsage {
        self.inner.usage
    }

    #[must_use]
    pub(crate) fn adapters(&self) -> &[AuthoringAdapterReceipt] {
        &self.inner.adapters
    }
}

impl Debug for AuthoringProjectSnapshot {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringProjectSnapshot")
            .field("attempt", &self.attempt_generation())
            .field("root", &self.root_identity())
            .field("readiness", &self.readiness())
            .field("startup_source", &self.startup_source())
            .field("prefab_count", &self.prefabs().len())
            .field("asset_count", &self.assets().len())
            .field(
                "unresolved_dependency_count",
                &self.unresolved_dependencies().len(),
            )
            .field("source_count", &self.inner.source_facts.len())
            .field("usage", &self.usage())
            .finish_non_exhaustive()
    }
}

pub(crate) struct AuthoringProjectCandidate {
    inner: AuthoringProjectSnapshotInner,
}

impl AuthoringProjectCandidate {
    #[must_use]
    pub(crate) const fn readiness(&self) -> AuthoringProjectReadiness {
        self.inner.readiness
    }

    #[must_use]
    pub(crate) const fn usage(&self) -> AuthoringProjectAdmissionUsage {
        self.inner.usage
    }

    #[must_use]
    pub(crate) fn publish(self) -> AuthoringProjectReceipt {
        AuthoringProjectReceipt {
            snapshot: AuthoringProjectSnapshot {
                inner: Arc::new(self.inner),
            },
        }
    }
}

pub(crate) struct AuthoringProjectReceipt {
    snapshot: AuthoringProjectSnapshot,
}

impl AuthoringProjectReceipt {
    #[must_use]
    pub(crate) fn snapshot(&self) -> AuthoringProjectSnapshot {
        self.snapshot.clone()
    }

    #[must_use]
    pub(crate) fn attempt_generation(&self) -> AuthoringProjectAttemptGeneration {
        self.snapshot.attempt_generation()
    }

    pub(crate) fn into_session_parts(
        self,
    ) -> Result<AuthoringProjectSessionParts, AuthoringProjectReceiptUseError> {
        let inner = Arc::try_unwrap(self.snapshot.inner)
            .map_err(|_| AuthoringProjectReceiptUseError::SnapshotAliased)?;
        let startup_scene = fork_published_authoring_scene_document(&inner.startup_scene);
        let mut source_index = ProjectSourceIndexState::new(&inner.schema);
        let _ = source_index.request_refresh();
        Ok(AuthoringProjectSessionParts {
            snapshot: AuthoringProjectSessionSnapshot {
                schema: inner.schema,
                adapters: Arc::from(inner.adapters),
                scene_limits: inner.scene_limits,
                readiness: inner.readiness,
                startup_source: inner.startup_source,
                startup_scene: inner.startup_scene,
                prefabs: inner.prefabs,
                assets: inner.assets,
                asset_database: inner.asset_database,
                unresolved_dependencies: inner.unresolved_dependencies,
                source_facts: inner.source_facts,
                usage: inner.usage,
                source_index,
            },
            startup_scene,
        })
    }
}

impl Debug for AuthoringProjectReceipt {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringProjectReceipt")
            .field("snapshot", &self.snapshot)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthoringProjectReceiptUseError {
    SnapshotAliased,
}

impl Display for AuthoringProjectReceiptUseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SnapshotAliased => {
                "authoring project receipt snapshot is still retained by another owner"
            }
        })
    }
}

impl Error for AuthoringProjectReceiptUseError {}

pub(crate) struct AuthoringProjectSessionParts {
    pub(crate) snapshot: AuthoringProjectSessionSnapshot,
    pub(crate) startup_scene: PublishedAuthoringSceneDocument,
}

pub(crate) struct AuthoringProjectSessionSnapshot {
    schema: AuthoringSchemaReceipt,
    adapters: Arc<[AuthoringAdapterReceipt]>,
    scene_limits: SceneFileLimits,
    readiness: AuthoringProjectReadiness,
    startup_source: Arc<AuthoringSourceFact>,
    startup_scene: Arc<PublishedAuthoringSceneDocument>,
    prefabs: Box<[AuthoringPrefabSource]>,
    assets: Box<[AuthoringAssetSource]>,
    asset_database: Arc<ProjectAssetDatabase>,
    unresolved_dependencies: Box<[AuthoringUnresolvedDependency]>,
    source_facts: Box<[Arc<AuthoringSourceFact>]>,
    usage: AuthoringProjectAdmissionUsage,
    source_index: ProjectSourceIndexState,
}

impl AuthoringProjectSessionSnapshot {
    #[must_use]
    pub(crate) fn asset_database(&self) -> Arc<ProjectAssetDatabase> {
        Arc::clone(&self.asset_database)
    }

    #[must_use]
    pub(crate) fn attempt_generation(&self) -> AuthoringProjectAttemptGeneration {
        self.schema.attempt_generation()
    }

    #[must_use]
    pub(crate) const fn settings(&self) -> &EffectiveProjectSettings {
        self.schema.settings()
    }

    #[must_use]
    pub(crate) fn project_root(&self) -> DirectoryCapability {
        self.schema.project_root()
    }

    #[must_use]
    pub(crate) const fn manifest_digest(&self) -> ContentDigest {
        self.schema.manifest_digest()
    }

    #[must_use]
    pub(crate) const fn authoring_bundle_digest(&self) -> ContentDigest {
        self.schema.bundle_digest()
    }

    #[must_use]
    pub(crate) fn schema_fingerprint_hex(&self) -> String {
        self.schema.composition_fingerprint().to_hex()
    }

    #[must_use]
    pub(crate) fn source_facts(&self) -> impl ExactSizeIterator<Item = &AuthoringSourceFact> {
        self.source_facts.iter().map(Arc::as_ref)
    }

    #[must_use]
    pub(crate) const fn readiness(&self) -> AuthoringProjectReadiness {
        self.readiness
    }

    #[must_use]
    pub(crate) fn startup_source(&self) -> &AuthoringSourceFact {
        &self.startup_source
    }

    #[must_use]
    pub(crate) fn fork_startup_scene(&self) -> PublishedAuthoringSceneDocument {
        fork_published_authoring_scene_document(&self.startup_scene)
    }

    #[must_use]
    pub(crate) fn prefab(&self, path: &AssetPath) -> Option<&AuthoringPrefabSource> {
        self.prefabs.iter().find(|prefab| prefab.path() == path)
    }

    #[must_use]
    pub(crate) fn fork_prefab(&self, path: &AssetPath) -> Option<PublishedAuthoringPrefabDocument> {
        self.prefab(path)
            .map(AuthoringPrefabSource::document)
            .map(fork_published_authoring_prefab_document)
    }

    pub(crate) fn asset_records(&self) -> impl Iterator<Item = &AssetRecord> {
        self.assets.iter().map(AuthoringAssetSource::record)
    }

    pub(crate) fn asset_sources(&self) -> impl Iterator<Item = &AuthoringAssetSource> {
        self.assets.iter()
    }

    pub(crate) fn image_asset_sources(&self) -> impl Iterator<Item = &AuthoringAssetSource> {
        self.assets
            .iter()
            .filter(|source| source.record().source_kind() == &AssetSourceKind::Image)
    }

    #[must_use]
    pub(crate) const fn usage(&self) -> AuthoringProjectAdmissionUsage {
        self.usage
    }

    #[must_use]
    pub(crate) fn source_index_kind(&self) -> super::ProjectSourceIndexStateKind {
        self.source_index.kind()
    }

    #[must_use]
    pub(crate) fn source_index_snapshot(&self) -> Option<Arc<super::ProjectSourceIndexSnapshot>> {
        self.source_index.snapshot()
    }

    #[must_use]
    pub(crate) const fn source_index_failure(&self) -> Option<&super::ProjectSourceIndexFailure> {
        self.source_index.failure()
    }

    #[must_use]
    pub(crate) const fn source_index_has_pending_work(&self) -> bool {
        self.source_index.has_pending_work()
    }

    #[must_use]
    pub(crate) fn source_index_progress(&self) -> Option<super::ProjectSourceIndexProgress> {
        self.source_index.progress()
    }

    pub(crate) fn advance_source_index(&mut self, pools: &nara_tasks::TaskPools) -> bool {
        self.source_index.advance(pools)
    }

    pub(crate) fn observe_source_index(&mut self) -> bool {
        self.source_index.poll_preparation()
    }

    pub(crate) fn cancel_source_index(&mut self) {
        self.source_index.cancel();
    }

    #[cfg(test)]
    pub(crate) fn hold_next_source_index_scan(
        &mut self,
        phase: super::source_index::ScanTestPhase,
    ) -> super::source_index::ScanTestControl {
        self.source_index.hold_next_scan(phase)
    }

    pub(crate) fn request_source_index_refresh(
        &mut self,
    ) -> Option<super::ProjectSourceIndexGeneration> {
        self.source_index.request_refresh()
    }

    pub(crate) fn capture_indexed_document(
        &self,
        index: Arc<super::ProjectSourceIndexSnapshot>,
        relative_path: Arc<str>,
        expected_kind: super::AuthoringDocumentKind,
    ) -> Result<
        super::document::OwnedAuthoringDocumentRequest,
        super::AuthoringDocumentAdmissionError,
    > {
        super::document::AuthoringDocumentAdmissionBasis::new(
            &self.schema,
            &self.adapters,
            self.scene_limits,
        )
        .capture(index, relative_path, expected_kind)
    }
}

impl Debug for AuthoringProjectSessionSnapshot {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringProjectSessionSnapshot")
            .field("attempt", &self.attempt_generation())
            .field("readiness", &self.readiness)
            .field("startup_source", &self.startup_source)
            .field("adapter_count", &self.adapters.len())
            .field("prefab_count", &self.prefabs.len())
            .field("asset_count", &self.assets.len())
            .field(
                "unresolved_dependency_count",
                &self.unresolved_dependencies.len(),
            )
            .field("source_count", &self.source_facts.len())
            .field("source_index", &self.source_index.kind())
            .field("usage", &self.usage)
            .finish_non_exhaustive()
    }
}

pub(crate) struct PreparedAuthoringProject {
    schema: AuthoringSchemaReceipt,
    adapters: Box<[AuthoringAdapterReceipt]>,
    scene_limits: SceneFileLimits,
}

impl PreparedAuthoringProject {
    #[must_use]
    pub(crate) fn new(
        schema: AuthoringSchemaReceipt,
        adapters: Box<[AuthoringAdapterReceipt]>,
    ) -> Self {
        Self::with_scene_limits(schema, adapters, SceneFileLimits::default())
    }

    #[must_use]
    pub(crate) const fn with_scene_limits(
        schema: AuthoringSchemaReceipt,
        adapters: Box<[AuthoringAdapterReceipt]>,
        scene_limits: SceneFileLimits,
    ) -> Self {
        Self {
            schema,
            adapters,
            scene_limits,
        }
    }

    pub(crate) fn admit_startup_closure(
        mut self,
    ) -> Result<AuthoringProjectCandidate, AuthoringProjectSourceAdmissionError> {
        let scene_root = self.schema.settings.paths.scenes.clone();
        let prefab_root = self.schema.settings.paths.prefabs.clone();
        let asset_root = self.schema.settings.paths.assets.clone();
        let bundle_encoded_bytes = self.schema.bundle_bytes().len();
        let startup_path = self
            .schema
            .settings
            .startup
            .default_scene
            .clone()
            .ok_or(AuthoringProjectSourceAdmissionError::MissingStartupScene)?;

        let adapter_retained = adapter_receipts_retained_bytes(&self.adapters)?;
        self.schema
            .budget
            .charge_source_bytes(0, 0, 0, adapter_retained)?;

        let scenes = open_source_directory(
            &self.schema.project_root,
            &scene_root,
            AuthoringSourceKind::Scene,
            &mut self.schema.budget,
        )?;
        self.schema
            .budget
            .charge_reference(startup_path.as_str().len(), 0, false)?;
        let startup_project_path = join_project_path(&scene_root, startup_path.as_str())?;
        let startup_opened = open_source_file(
            &scenes,
            startup_project_path,
            startup_path.as_str(),
            AuthoringSourceKind::Scene,
            0,
        )?;
        let mut identities = HashMap::new();
        identities.insert(self.schema.manifest_identity(), LoadedIdentity::SchemaBasis);
        identities.insert(self.schema.bundle_identity(), LoadedIdentity::SchemaBasis);
        if identities.contains_key(&startup_opened.identity) {
            return Err(
                AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                    path: startup_opened.project_path,
                },
            );
        }
        let startup_read = read_unique_source(
            startup_opened,
            self.scene_limits.encoded_bytes().get(),
            &mut self.schema.budget,
        )?;
        let startup_scene = {
            let bundle = self.schema.bundle.bundle();
            let budget = &mut self.schema.budget;
            admit_scene_document(
                &startup_read,
                self.scene_limits,
                bundle,
                &self.adapters,
                bundle_encoded_bytes,
                budget,
            )?
        };
        drop(scenes);

        let startup_source = Arc::clone(&startup_read.source);
        let mut readiness = AuthoringProjectReadiness::from_document(startup_scene.readiness());
        identities.insert(startup_source.identity(), LoadedIdentity::Scene);
        let mut source_facts = BTreeMap::new();
        source_facts.insert(
            startup_source.project_path().as_str().to_owned(),
            Arc::clone(&startup_source),
        );

        let mut pending = BTreeSet::new();
        let mut queued_depths = BTreeMap::new();
        let mut loaded_paths = BTreeSet::new();
        let mut pending_assets = BTreeMap::new();
        let mut unresolved_dependencies = Vec::new();
        let mut catalogue = ReferenceCatalogue::new(
            self.schema.project_root.share(),
            [asset_root.clone(), prefab_root.clone()],
        );
        enqueue_document_prefabs(
            &startup_scene.document().entities,
            0,
            &mut pending,
            &mut queued_depths,
            &loaded_paths,
            &mut unresolved_dependencies,
            &mut catalogue,
            &mut self.schema.budget,
        )?;
        enqueue_document_assets(
            startup_scene.record_facts(),
            0,
            &mut pending_assets,
            &mut unresolved_dependencies,
            &mut catalogue,
            &mut self.schema.budget,
        )?;

        let prefabs_directory = if pending.is_empty() {
            None
        } else {
            Some(open_source_directory(
                &self.schema.project_root,
                &prefab_root,
                AuthoringSourceKind::Prefab,
                &mut self.schema.budget,
            )?)
        };
        let mut prefabs = BTreeMap::new();
        while let Some((depth, path)) = pending.pop_first() {
            queued_depths.remove(&path);
            if !loaded_paths.insert(path.clone()) {
                continue;
            }
            let directory = prefabs_directory
                .as_ref()
                .ok_or(AuthoringProjectSourceAdmissionError::PrefabDirectoryAuthorityUnavailable)?;
            let project_path = join_project_path(&prefab_root, path.as_str())?;
            let opened = open_source_file(
                directory,
                project_path,
                path.as_str(),
                AuthoringSourceKind::Prefab,
                depth,
            )?;

            if let Some(loaded) = identities.get(&opened.identity) {
                let LoadedIdentity::Prefab { source, document } = loaded else {
                    return Err(
                        AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                            path: opened.project_path,
                        },
                    );
                };
                prefabs.insert(
                    path.clone(),
                    AuthoringPrefabSource {
                        path,
                        source: Arc::clone(source),
                        document: Arc::clone(document),
                    },
                );
                continue;
            }

            let read = read_unique_source(
                opened,
                self.scene_limits.encoded_bytes().get(),
                &mut self.schema.budget,
            )?;
            let document = {
                let bundle = self.schema.bundle.bundle();
                let budget = &mut self.schema.budget;
                admit_prefab_document(
                    &read,
                    self.scene_limits,
                    bundle,
                    &self.adapters,
                    bundle_encoded_bytes,
                    budget,
                )?
            };
            readiness = readiness.include(document.readiness());
            let source = Arc::clone(&read.source);
            identities.insert(
                source.identity(),
                LoadedIdentity::Prefab {
                    source: Arc::clone(&source),
                    document: Arc::clone(&document),
                },
            );
            source_facts.insert(
                source.project_path().as_str().to_owned(),
                Arc::clone(&source),
            );
            prefabs.insert(
                path.clone(),
                AuthoringPrefabSource {
                    path,
                    source,
                    document: Arc::clone(&document),
                },
            );
            enqueue_document_prefabs(
                &document.document().entities,
                depth,
                &mut pending,
                &mut queued_depths,
                &loaded_paths,
                &mut unresolved_dependencies,
                &mut catalogue,
                &mut self.schema.budget,
            )?;
            enqueue_document_assets(
                document.record_facts(),
                depth,
                &mut pending_assets,
                &mut unresolved_dependencies,
                &mut catalogue,
                &mut self.schema.budget,
            )?;
        }
        drop(prefabs_directory);

        let mut asset_database = ProjectAssetDatabase::default();
        if catalogue.is_indexed() && !prefabs.is_empty() {
            let directory = open_source_directory(
                &self.schema.project_root,
                &prefab_root,
                AuthoringSourceKind::Prefab,
                &mut self.schema.budget,
            )?;
            for path in prefabs.keys() {
                if let Some((record, source)) =
                    catalogue.admit_prefab_metadata(&directory, path, &mut self.schema.budget)?
                {
                    if identities.contains_key(&source.identity()) {
                        return Err(
                            AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                                path: source.project_path().clone(),
                            },
                        );
                    }
                    insert_asset_record(&mut asset_database, record)?;
                    identities.insert(source.identity(), LoadedIdentity::AssetMetadata);
                    source_facts.insert(source.project_path().as_str().to_owned(), source);
                }
            }
        }

        let assets_directory = if pending_assets.is_empty() {
            None
        } else {
            Some(open_source_directory(
                &self.schema.project_root,
                &asset_root,
                AuthoringSourceKind::AssetSource,
                &mut self.schema.budget,
            )?)
        };
        let mut assets = BTreeMap::new();
        for (path, depth) in pending_assets {
            let directory = assets_directory
                .as_ref()
                .ok_or(AuthoringProjectSourceAdmissionError::AssetDirectoryAuthorityUnavailable)?;
            let metadata_path = path.meta_path();
            self.schema
                .budget
                .charge_reference(metadata_path.len(), depth, false)?;
            let metadata_project_path = join_project_path(&asset_root, &metadata_path)?;
            let metadata_opened = open_source_file(
                directory,
                metadata_project_path,
                &metadata_path,
                AuthoringSourceKind::AssetMetadata,
                depth,
            )?;
            ensure_unclaimed_source_identity(&identities, &metadata_opened)?;
            let metadata_limits = AssetMetaFileLimits::default();
            let metadata_read = read_unique_source(
                metadata_opened,
                metadata_limits.encoded_bytes().get(),
                &mut self.schema.budget,
            )?;
            catalogue.verify_metadata(&path, false, &metadata_read.source)?;
            let exact_metadata_limit = ByteLimit::new(metadata_read.read_limit)
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
            let metadata_encoded_bytes = metadata_read.bytes.len();
            let metadata_source = metadata_read.source;
            let metadata = AssetMetaIdentityCandidate::decode_json_bytes_owned_with_limits(
                metadata_read.bytes.into_boxed_slice(),
                metadata_limits.with_encoded_bytes(exact_metadata_limit),
            )
            .map_err(|source| {
                AuthoringProjectSourceAdmissionError::AssetMetadataFormat {
                    path: path.clone(),
                    source,
                }
            })?;
            asset_database
                .insert_identity(path.clone(), metadata.identity().clone())
                .map_err(|source| match source {
                    AssetDatabaseError::MetaPathMismatch {
                        source_path,
                        meta_path,
                    } => AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch {
                        requested: source_path,
                        declared: meta_path,
                    },
                    source => AuthoringProjectSourceAdmissionError::AssetIdentity {
                        path: path.clone(),
                        source,
                    },
                })?;
            let record = AssetRecord::from(metadata.identity().clone());
            charge_database_record(&record, &mut self.schema.budget)?;
            charge_asset_metadata(
                &metadata_source,
                metadata_encoded_bytes,
                &metadata,
                &record,
                &mut self.schema.budget,
            )?;
            identities.insert(metadata_source.identity(), LoadedIdentity::AssetMetadata);
            source_facts.insert(
                metadata_source.project_path().as_str().to_owned(),
                Arc::clone(&metadata_source),
            );

            let source_project_path = join_project_path(&asset_root, record.path().as_str())?;
            let source_opened = open_source_file(
                directory,
                source_project_path,
                record.path().as_str(),
                AuthoringSourceKind::AssetSource,
                depth,
            )?;
            ensure_unclaimed_source_identity(&identities, &source_opened)?;
            let aggregate_source_limit = self.schema.budget.maximum();
            let source_read = read_unique_source(
                source_opened,
                aggregate_source_limit,
                &mut self.schema.budget,
            )?;
            charge_asset_source(&source_read, &mut self.schema.budget)?;
            let source = Arc::clone(&source_read.source);
            identities.insert(source.identity(), LoadedIdentity::AssetSource);
            source_facts.insert(
                source.project_path().as_str().to_owned(),
                Arc::clone(&source),
            );
            let bytes = Arc::<[u8]>::from(source_read.bytes);
            assets.insert(
                record.path().clone(),
                AuthoringAssetSource {
                    record,
                    metadata,
                    metadata_source,
                    source,
                    bytes,
                },
            );
        }
        drop(assets_directory);

        #[cfg(feature = "presentation-2d")]
        {
            // The preview shares this database until the first accepted image adds an identity.
            // Reserve its one private copy before publishing the authoring session.
            for asset in assets.values() {
                charge_database_record(asset.record(), &mut self.schema.budget)?;
            }
            for path in prefabs.keys() {
                if let Some(record) = asset_database.record_for_path(path) {
                    charge_database_record(record, &mut self.schema.budget)?;
                }
            }
            self.schema.budget.charge_source_bytes(
                0,
                0,
                0,
                size_of::<ProjectAssetDatabase>() + 2 * size_of::<usize>(),
            )?;
            let growth = checked_sum([
                checked_product(super::MAX_AUTHORING_IMAGE_IDENTITY_PATH_BYTES, 9)?,
                checked_product(
                    super::MAX_AUTHORING_IMAGE_IDENTITIES,
                    size_of::<AssetRecord>() + 3 * size_of::<AssetPath>() + 192,
                )?,
            ])?;
            self.schema.budget.charge_source_bytes(0, 0, 0, growth)?;
        }

        let prefab_path_bytes = prefabs.keys().try_fold(0_usize, |total, path| {
            total
                .checked_add(path.as_str().len())
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
        })?;
        if !unresolved_dependencies.is_empty() {
            readiness = AuthoringProjectReadiness::Degraded;
        }
        let unresolved_dependency_retained =
            unresolved_dependencies_retained_bytes(&unresolved_dependencies)?;
        self.schema
            .budget
            .charge_source_bytes(0, 0, 0, unresolved_dependency_retained)?;
        let source_facts = source_facts.into_values().collect::<Vec<_>>();
        self.schema.budget.charge_source_bytes(
            0,
            0,
            0,
            snapshot_index_retained_bytes(
                source_facts.len(),
                prefabs.len(),
                assets.len(),
                unresolved_dependencies.len(),
                prefab_path_bytes,
            )?,
        )?;
        let usage = self.schema.budget.usage();

        Ok(AuthoringProjectCandidate {
            inner: AuthoringProjectSnapshotInner {
                schema: self.schema,
                adapters: self.adapters,
                scene_limits: self.scene_limits,
                readiness,
                startup_source,
                startup_scene,
                prefabs: prefabs.into_values().collect::<Vec<_>>().into_boxed_slice(),
                assets: assets.into_values().collect::<Vec<_>>().into_boxed_slice(),
                asset_database: Arc::new(asset_database),
                unresolved_dependencies: unresolved_dependencies.into_boxed_slice(),
                source_facts: source_facts.into_boxed_slice(),
                usage,
            },
        })
    }
}

enum LoadedIdentity {
    SchemaBasis,
    Scene,
    AssetMetadata,
    AssetSource,
    Prefab {
        source: Arc<AuthoringSourceFact>,
        document: Arc<PublishedAuthoringPrefabDocument>,
    },
}

struct OpenedAuthoringSource {
    kind: AuthoringSourceKind,
    project_path: ProjectPath,
    capability: FileCapability,
    identity: FileIdentity,
    parent_identity: Option<FileIdentity>,
    dependency_depth: usize,
}

struct ReadAuthoringSource {
    source: Arc<AuthoringSourceFact>,
    bytes: Vec<u8>,
    read_limit: usize,
}

fn open_source_directory(
    root: &DirectoryCapability,
    path: &ProjectPath,
    kind: AuthoringSourceKind,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<DirectoryCapability, AuthoringProjectSourceAdmissionError> {
    budget.charge_reference(path.as_str().len(), 0, false)?;
    let relative = RelativePath::new(path.as_str()).map_err(|source| {
        AuthoringProjectSourceAdmissionError::SourceDirectoryPath { kind, source }
    })?;
    let directory = root.open_directory(&relative).map_err(|source| {
        AuthoringProjectSourceAdmissionError::SourceDirectoryHost { kind, source }
    })?;
    Ok(directory)
}

fn open_source_file(
    directory: &DirectoryCapability,
    project_path: ProjectPath,
    logical_path: &str,
    kind: AuthoringSourceKind,
    dependency_depth: usize,
) -> Result<OpenedAuthoringSource, AuthoringProjectSourceAdmissionError> {
    let relative = RelativePath::new(logical_path).map_err(|source| {
        AuthoringProjectSourceAdmissionError::SourcePath {
            kind,
            path: project_path.clone(),
            source,
        }
    })?;
    let capability = directory.open_file(&relative).map_err(|source| {
        AuthoringProjectSourceAdmissionError::SourceHost {
            kind,
            path: project_path.clone(),
            source,
        }
    })?;
    let identity = capability.identity();
    Ok(OpenedAuthoringSource {
        kind,
        project_path,
        identity,
        parent_identity: capability.parent_identity(),
        capability,
        dependency_depth,
    })
}

fn read_unique_source(
    opened: OpenedAuthoringSource,
    source_limit: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ReadAuthoringSource, AuthoringProjectSourceAdmissionError> {
    budget.charge_open_file(SOURCE_OPEN_HANDLE_PEAK)?;
    let aggregate_maximum = budget.maximum();
    let (read_limit, aggregate_limited) = budget.read_limit(source_limit)?;
    let bytes = opened
        .capability
        .read_to_end_bounded(u64::try_from(read_limit).unwrap_or(u64::MAX))
        .map_err(|source| {
            map_source_read_error(
                source,
                opened.kind,
                opened.project_path.clone(),
                aggregate_limited,
                read_limit,
                aggregate_maximum,
            )
        })?;
    let digest = ContentDigest::of_bytes(&bytes);
    budget.charge_source_bytes(bytes.len(), 0, bytes.len(), 0)?;
    Ok(ReadAuthoringSource {
        source: Arc::new(AuthoringSourceFact {
            kind: opened.kind,
            project_path: opened.project_path,
            identity: opened.identity,
            parent_identity: opened.parent_identity,
            digest,
            encoded_bytes: bytes.len(),
            dependency_depth: opened.dependency_depth,
        }),
        bytes,
        read_limit,
    })
}

fn ensure_unclaimed_source_identity(
    identities: &HashMap<FileIdentity, LoadedIdentity>,
    opened: &OpenedAuthoringSource,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    if identities.contains_key(&opened.identity) {
        return Err(
            AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                path: opened.project_path.clone(),
            },
        );
    }
    Ok(())
}

fn charge_asset_metadata(
    metadata_source: &AuthoringSourceFact,
    encoded_bytes: usize,
    candidate: &AssetMetaIdentityCandidate,
    record: &AssetRecord,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let decoded_bytes = asset_record_retained_bytes(record)?;
    let retained_bytes = decoded_bytes
        .checked_add(source_fact_retained_bytes(metadata_source)?)
        .and_then(|total| total.checked_add(candidate.retained_bytes()?))
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    budget.charge_source_bytes(0, decoded_bytes, encoded_bytes, retained_bytes)?;
    Ok(())
}

fn charge_asset_source(
    source: &ReadAuthoringSource,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let retained_bytes = source
        .bytes
        .len()
        .checked_add(source_fact_retained_bytes(&source.source)?)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    budget.charge_source_bytes(0, 0, 0, retained_bytes)?;
    Ok(())
}

fn asset_record_retained_bytes(
    record: &AssetRecord,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    let source_kind_bytes = match record.source_kind() {
        AssetSourceKind::Other(kind) => kind.len(),
        AssetSourceKind::Unknown
        | AssetSourceKind::Image
        | AssetSourceKind::Scene
        | AssetSourceKind::Prefab => 0,
    };
    checked_sum([
        size_of::<AssetRecord>(),
        record.path().as_str().len(),
        source_kind_bytes,
    ])
}

fn source_fact_retained_bytes(
    source: &AuthoringSourceFact,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    checked_sum([
        size_of::<AuthoringSourceFact>(),
        source.project_path().as_str().len(),
        size_of::<Arc<AuthoringSourceFact>>(),
    ])
}

fn admit_scene_document(
    source: &ReadAuthoringSource,
    scene_limits: SceneFileLimits,
    bundle: &AuthoringSchemaBundle,
    adapters: &[AuthoringAdapterReceipt],
    bundle_encoded_bytes: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Arc<PublishedAuthoringSceneDocument>, AuthoringProjectSourceAdmissionError> {
    let encoded_limit = ByteLimit::new(source.read_limit)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    let limits = scene_limits.with_encoded_bytes(encoded_limit);
    let published = Arc::new(
        admit_scene_bytes(
            source.source.project_path(),
            &source.bytes,
            limits,
            bundle,
            adapters,
        )
        .map_err(|error| map_document_data_error(source, error, budget))?,
    );
    charge_published_document(
        PublishedDocumentCharge {
            source: &source.source,
            encoded_bytes: source.bytes.len(),
            document: published.document(),
            record_facts: published.record_facts(),
            unavailable_records: published.unavailable_manifest().total_count(),
            diagnostics: published.diagnostics(),
            bundle_encoded_bytes,
        },
        budget,
    )?;
    Ok(published)
}

fn admit_prefab_document(
    source: &ReadAuthoringSource,
    scene_limits: SceneFileLimits,
    bundle: &AuthoringSchemaBundle,
    adapters: &[AuthoringAdapterReceipt],
    bundle_encoded_bytes: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Arc<PublishedAuthoringPrefabDocument>, AuthoringProjectSourceAdmissionError> {
    let encoded_limit = ByteLimit::new(source.read_limit)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    let limits = scene_limits.with_encoded_bytes(encoded_limit);
    let published = Arc::new(
        admit_prefab_bytes(
            source.source.project_path(),
            &source.bytes,
            limits,
            bundle,
            adapters,
        )
        .map_err(|error| map_document_data_error(source, error, budget))?,
    );
    charge_published_document(
        PublishedDocumentCharge {
            source: &source.source,
            encoded_bytes: source.bytes.len(),
            document: published.document(),
            record_facts: published.record_facts(),
            unavailable_records: published.unavailable_manifest().total_count(),
            diagnostics: published.diagnostics(),
            bundle_encoded_bytes,
        },
        budget,
    )?;
    Ok(published)
}

fn map_document_data_error(
    source: &ReadAuthoringSource,
    error: AuthoringDocumentDataError,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> AuthoringProjectSourceAdmissionError {
    match error {
        AuthoringDocumentDataError::UnsupportedEncoding => {
            AuthoringProjectSourceAdmissionError::UnsupportedSourceEncoding {
                kind: source.source.kind(),
                path: source.source.project_path().clone(),
            }
        }
        AuthoringDocumentDataError::Format(source_error) => {
            AuthoringProjectSourceAdmissionError::SourceFormat {
                kind: source.source.kind(),
                path: source.source.project_path().clone(),
                source: source_error,
            }
        }
        AuthoringDocumentDataError::Source(source_error) => {
            AuthoringProjectSourceAdmissionError::DocumentSourcePath {
                path: source.source.project_path().clone(),
                source: source_error,
            }
        }
        AuthoringDocumentDataError::Publication(source_error) => {
            if let Err(budget_error) = budget.charge_diagnostics(
                source_error.diagnostics().len(),
                source_error.diagnostics().retained_bytes(),
            ) {
                return budget_error.into();
            }
            AuthoringProjectSourceAdmissionError::SourcePublication {
                kind: source.source.kind(),
                path: source.source.project_path().clone(),
                source: source_error,
            }
        }
    }
}

struct PublishedDocumentCharge<'a, D> {
    source: &'a AuthoringSourceFact,
    encoded_bytes: usize,
    document: &'a D,
    record_facts: &'a [AuthoringRecordFact],
    unavailable_records: usize,
    diagnostics: &'a nara_diagnostic::DiagnosticReport,
    bundle_encoded_bytes: usize,
}

fn charge_published_document<D: AuthoringDocumentMetrics>(
    charge: PublishedDocumentCharge<'_, D>,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let document_retained = charge.document.retained_bytes()?;
    let record_retained = checked_product(
        charge.record_facts.len(),
        size_of::<AuthoringRecordFact>() + AUTHORING_RECORD_RETAINED_OVERHEAD,
    )?;
    let unavailable_retained = if charge.unavailable_records == 0 {
        0
    } else {
        checked_sum([
            checked_product(
                charge.unavailable_records,
                size_of::<UnavailableRecordManifestEntry>() + DEGRADED_ISSUE_RETAINED_BYTES,
            )?,
            // The manifest owns cloned values plus derived reference paths. Charging twice the
            // complete document payload is deliberately conservative and bounds both copies
            // without leaking the manifest's private representation into the Host.
            checked_product(document_retained, 2)?,
        ])?
    };
    let basis_retained = checked_product(
        charge.bundle_encoded_bytes,
        AUTHORING_BASIS_ENCODED_MULTIPLIER,
    )?;
    let source_retained = checked_sum([
        size_of::<AuthoringSourceFact>(),
        charge.source.project_path().as_str().len(),
        size_of::<Arc<AuthoringSourceFact>>(),
    ])?;
    let retained_bytes = checked_sum([
        document_retained,
        record_retained,
        unavailable_retained,
        basis_retained,
        source_retained,
        size_of::<Arc<D>>(),
    ])?;
    let decode_work = document_retained.saturating_add(charge.encoded_bytes);
    budget.charge_source_bytes(0, document_retained, decode_work, retained_bytes)?;
    budget.charge_diagnostics(
        charge.diagnostics.len(),
        charge.diagnostics.retained_bytes(),
    )?;
    Ok(())
}

trait AuthoringDocumentMetrics {
    fn retained_bytes(&self) -> Result<usize, AuthoringProjectSourceAdmissionError>;
}

impl AuthoringDocumentMetrics for SceneDocument {
    fn retained_bytes(&self) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        document_retained_bytes(size_of::<Self>(), &self.roots, &self.entities)
    }
}

impl AuthoringDocumentMetrics for PrefabDocument {
    fn retained_bytes(&self) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        document_retained_bytes(size_of::<Self>(), &self.roots, &self.entities)
    }
}

fn document_retained_bytes(
    base: usize,
    roots: &[nara_scene::SceneEntityId],
    entities: &[SceneEntityRecord],
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    let mut total = checked_sum([
        base,
        checked_product(roots.len(), size_of::<nara_scene::SceneEntityId>())?,
        checked_product(entities.len(), size_of::<SceneEntityRecord>())?,
    ])?;
    for root in roots {
        checked_add_to(&mut total, root.as_str().len())?;
    }
    for entity in entities {
        checked_add_to(&mut total, entity_retained_bytes(entity)?)?;
    }
    Ok(total)
}

fn entity_retained_bytes(
    entity: &SceneEntityRecord,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    let mut total = checked_sum([
        entity.id.as_str().len(),
        checked_product(
            entity.children.len(),
            size_of::<nara_scene::SceneEntityId>(),
        )?,
        checked_product(
            entity.components.len(),
            size_of::<nara_scene::SceneComponentRecord>(),
        )?,
    ])?;
    for child in &entity.children {
        checked_add_to(&mut total, child.as_str().len())?;
    }
    for (component, record) in &entity.components {
        let cost = record.value.cost();
        checked_add_to(
            &mut total,
            checked_sum([
                component.as_str().len(),
                record.owner().map_or(0, |owner| owner.as_str().len()),
                cost.logical_bytes(),
                checked_product(cost.nodes(), VALUE_NODE_RETAINED_BYTES)?,
            ])?,
        )?;
    }
    if let Some(prefab) = &entity.prefab {
        checked_add_to(&mut total, asset_ref_retained_bytes(&prefab.source)?)?;
        checked_add_to(&mut total, patch_retained_bytes(&prefab.overrides)?)?;
    }
    Ok(total)
}

fn patch_retained_bytes(
    patch: &ScenePatchDocument,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    let mut total = checked_product(patch.operations.len(), size_of::<ScenePatchOperation>())?;
    for operation in &patch.operations {
        let dynamic = match operation {
            ScenePatchOperation::AddEntity { entity, .. } => entity_retained_bytes(entity)?,
            ScenePatchOperation::RestoreEntity {
                entity,
                descendants,
                ..
            } => descendants
                .iter()
                .try_fold(entity_retained_bytes(entity)?, |total, entity| {
                    total
                        .checked_add(entity_retained_bytes(entity)?)
                        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
                })?,
            ScenePatchOperation::AddComponent {
                entity,
                component,
                value,
            }
            | ScenePatchOperation::ReplaceComponent {
                entity,
                component,
                value,
            } => {
                let cost = value.value.cost();
                checked_sum([
                    entity.as_str().len(),
                    component.as_str().len(),
                    cost.logical_bytes(),
                    checked_product(cost.nodes(), VALUE_NODE_RETAINED_BYTES)?,
                ])?
            }
            ScenePatchOperation::SetField {
                entity,
                component,
                field,
                value,
                ..
            } => {
                let cost = value.cost();
                checked_sum([
                    entity.as_str().len(),
                    component.as_str().len(),
                    field.as_str().len(),
                    cost.logical_bytes(),
                    checked_product(cost.nodes(), VALUE_NODE_RETAINED_BYTES)?,
                ])?
            }
            ScenePatchOperation::SetAssetRefField {
                entity,
                component,
                field,
                asset_ref,
                ..
            } => checked_sum([
                entity.as_str().len(),
                component.as_str().len(),
                field.as_str().len(),
                asset_ref_retained_bytes(asset_ref)?,
            ])?,
            ScenePatchOperation::RemoveEntity { entity, .. }
            | ScenePatchOperation::RemoveComponent { entity, .. }
            | ScenePatchOperation::RemoveField { entity, .. }
            | ScenePatchOperation::MoveEntity { entity, .. } => entity.as_str().len(),
        };
        checked_add_to(&mut total, dynamic)?;
    }
    Ok(total)
}

fn asset_ref_retained_bytes(
    reference: &AssetRef,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    Ok(match reference {
        AssetRef::Path(path) => path.as_str().len(),
        AssetRef::StableId(_) => 16,
    })
}

fn enqueue_document_assets(
    records: &[AuthoringRecordFact],
    current_depth: usize,
    pending: &mut BTreeMap<AssetPath, usize>,
    unresolved_dependencies: &mut Vec<AuthoringUnresolvedDependency>,
    catalogue: &mut ReferenceCatalogue,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let dependency_depth = current_depth
        .checked_add(1)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    for record in records {
        for declared in record.schema_declared_asset_references() {
            let reference = declared.asset_ref();
            match catalogue.resolve(reference, false, budget)? {
                Some(path) => {
                    budget.charge_reference(path.as_str().len(), dependency_depth, true)?;
                    pending
                        .entry(path)
                        .and_modify(|observed_depth| {
                            *observed_depth = (*observed_depth).min(dependency_depth);
                        })
                        .or_insert(dependency_depth);
                }
                None => {
                    budget.charge_reference(16, dependency_depth, true)?;
                    unresolved_dependencies.push(AuthoringUnresolvedDependency {
                        kind: AuthoringUnresolvedDependencyKind::Asset,
                        reference: reference.clone(),
                        dependency_depth,
                    });
                }
            }
        }
    }
    Ok(())
}

fn enqueue_document_prefabs(
    entities: &[SceneEntityRecord],
    current_depth: usize,
    pending: &mut BTreeSet<(usize, AssetPath)>,
    queued_depths: &mut BTreeMap<AssetPath, usize>,
    loaded_paths: &BTreeSet<AssetPath>,
    unresolved_dependencies: &mut Vec<AuthoringUnresolvedDependency>,
    catalogue: &mut ReferenceCatalogue,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let next_depth = current_depth.saturating_add(1);
    for entity in entities {
        enqueue_entity_prefabs(
            entity,
            next_depth,
            pending,
            queued_depths,
            loaded_paths,
            unresolved_dependencies,
            catalogue,
            budget,
        )?;
    }
    Ok(())
}

fn enqueue_entity_prefabs(
    entity: &SceneEntityRecord,
    dependency_depth: usize,
    pending: &mut BTreeSet<(usize, AssetPath)>,
    queued_depths: &mut BTreeMap<AssetPath, usize>,
    loaded_paths: &BTreeSet<AssetPath>,
    unresolved_dependencies: &mut Vec<AuthoringUnresolvedDependency>,
    catalogue: &mut ReferenceCatalogue,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let Some(prefab) = &entity.prefab else {
        return Ok(());
    };
    enqueue_prefab_reference(
        &prefab.source,
        dependency_depth,
        pending,
        queued_depths,
        loaded_paths,
        unresolved_dependencies,
        catalogue,
        budget,
    )?;
    enqueue_patch_prefabs(
        &prefab.overrides,
        dependency_depth,
        pending,
        queued_depths,
        loaded_paths,
        unresolved_dependencies,
        catalogue,
        budget,
    )
}

fn enqueue_patch_prefabs(
    patch: &ScenePatchDocument,
    dependency_depth: usize,
    pending: &mut BTreeSet<(usize, AssetPath)>,
    queued_depths: &mut BTreeMap<AssetPath, usize>,
    loaded_paths: &BTreeSet<AssetPath>,
    unresolved_dependencies: &mut Vec<AuthoringUnresolvedDependency>,
    catalogue: &mut ReferenceCatalogue,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    for operation in &patch.operations {
        match operation {
            ScenePatchOperation::AddEntity { entity, .. } => enqueue_entity_prefabs(
                entity,
                dependency_depth,
                pending,
                queued_depths,
                loaded_paths,
                unresolved_dependencies,
                catalogue,
                budget,
            )?,
            ScenePatchOperation::RestoreEntity {
                entity,
                descendants,
                ..
            } => {
                enqueue_entity_prefabs(
                    entity,
                    dependency_depth,
                    pending,
                    queued_depths,
                    loaded_paths,
                    unresolved_dependencies,
                    catalogue,
                    budget,
                )?;
                for descendant in descendants {
                    enqueue_entity_prefabs(
                        descendant,
                        dependency_depth,
                        pending,
                        queued_depths,
                        loaded_paths,
                        unresolved_dependencies,
                        catalogue,
                        budget,
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn enqueue_prefab_reference(
    reference: &AssetRef,
    dependency_depth: usize,
    pending: &mut BTreeSet<(usize, AssetPath)>,
    queued_depths: &mut BTreeMap<AssetPath, usize>,
    loaded_paths: &BTreeSet<AssetPath>,
    unresolved_dependencies: &mut Vec<AuthoringUnresolvedDependency>,
    catalogue: &mut ReferenceCatalogue,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let Some(path) = catalogue.resolve(reference, true, budget)? else {
        budget.charge_reference(16, dependency_depth, true)?;
        unresolved_dependencies.push(AuthoringUnresolvedDependency {
            kind: AuthoringUnresolvedDependencyKind::Prefab,
            reference: reference.clone(),
            dependency_depth,
        });
        return Ok(());
    };
    budget.charge_reference(path.as_str().len(), dependency_depth, true)?;
    if loaded_paths.contains(&path) {
        return Ok(());
    }
    if let Some(previous_depth) = queued_depths.get(&path).copied() {
        if previous_depth <= dependency_depth {
            return Ok(());
        }
        pending.remove(&(previous_depth, path.clone()));
    }
    queued_depths.insert(path.clone(), dependency_depth);
    pending.insert((dependency_depth, path.clone()));
    Ok(())
}

fn join_project_path(
    directory: &ProjectPath,
    leaf: &str,
) -> Result<ProjectPath, AuthoringProjectSourceAdmissionError> {
    let mut path = String::with_capacity(directory.as_str().len() + 1 + leaf.len());
    path.push_str(directory.as_str());
    path.push('/');
    path.push_str(leaf);
    ProjectPath::new(path).map_err(AuthoringProjectSourceAdmissionError::JoinedSourcePath)
}

fn map_source_read_error(
    source: FsError,
    kind: AuthoringSourceKind,
    path: ProjectPath,
    aggregate_limited: bool,
    read_limit: usize,
    aggregate_maximum: usize,
) -> AuthoringProjectSourceAdmissionError {
    match source {
        FsError::ByteLimitExceeded { .. } if aggregate_limited => {
            AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded {
                maximum: aggregate_maximum,
            }
            .into()
        }
        FsError::ByteLimitExceeded { .. } => AuthoringProjectSourceAdmissionError::SourceTooLarge {
            kind,
            path,
            maximum: read_limit,
        },
        source => AuthoringProjectSourceAdmissionError::SourceHost { kind, path, source },
    }
}

fn adapter_receipts_retained_bytes(
    adapters: &[AuthoringAdapterReceipt],
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    adapters.iter().try_fold(0_usize, |total, adapter| {
        let component_bytes = adapter.components().try_fold(0_usize, |total, component| {
            total
                .checked_add(component.as_str().len() + size_of::<nara_reflect::ComponentTypeId>())
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
        })?;
        total
            .checked_add(size_of::<AuthoringAdapterReceipt>())
            .and_then(|total| total.checked_add(component_bytes))
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
    })
}

fn snapshot_index_retained_bytes(
    source_count: usize,
    prefab_count: usize,
    asset_count: usize,
    unresolved_dependency_count: usize,
    prefab_path_bytes: usize,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    checked_sum([
        size_of::<AuthoringProjectSnapshotInner>(),
        size_of::<ProjectAssetDatabase>() + 2 * size_of::<usize>(),
        checked_product(source_count, size_of::<Arc<AuthoringSourceFact>>())?,
        checked_product(prefab_count, size_of::<AuthoringPrefabSource>())?,
        checked_product(asset_count, size_of::<AuthoringAssetSource>())?,
        checked_product(
            unresolved_dependency_count,
            size_of::<AuthoringUnresolvedDependency>(),
        )?,
        prefab_path_bytes,
    ])
}

fn unresolved_dependencies_retained_bytes(
    dependencies: &[AuthoringUnresolvedDependency],
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    dependencies.iter().try_fold(0, |total, dependency| {
        checked_sum([
            total,
            size_of::<AuthoringUnresolvedDependency>(),
            asset_ref_retained_bytes(dependency.reference())?,
        ])
    })
}

fn checked_product(
    count: usize,
    bytes: usize,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    count
        .checked_mul(bytes)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
}

fn checked_sum<const N: usize>(
    values: [usize; N],
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    values.into_iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(value)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
    })
}

fn checked_add_to(
    total: &mut usize,
    value: usize,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    *total = total
        .checked_add(value)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    Ok(())
}

#[derive(Debug)]
pub(crate) enum AuthoringProjectSourceAdmissionError {
    MissingStartupScene,
    SourceDirectoryPath {
        kind: AuthoringSourceKind,
        source: PathValidationError,
    },
    SourceDirectoryHost {
        kind: AuthoringSourceKind,
        source: FsError,
    },
    JoinedSourcePath(ProjectPathError),
    SourcePath {
        kind: AuthoringSourceKind,
        path: ProjectPath,
        source: PathValidationError,
    },
    SourceHost {
        kind: AuthoringSourceKind,
        path: ProjectPath,
        source: FsError,
    },
    SourceIdentityKindConflict {
        path: ProjectPath,
    },
    SourceTooLarge {
        kind: AuthoringSourceKind,
        path: ProjectPath,
        maximum: usize,
    },
    UnsupportedSourceEncoding {
        kind: AuthoringSourceKind,
        path: ProjectPath,
    },
    SourceFormat {
        kind: AuthoringSourceKind,
        path: ProjectPath,
        source: SceneFormatError,
    },
    DocumentSourcePath {
        path: ProjectPath,
        source: AssetPathError,
    },
    SourcePublication {
        kind: AuthoringSourceKind,
        path: ProjectPath,
        source: AuthoringDocumentPublicationError,
    },
    AssetMetadataFormat {
        path: AssetPath,
        source: AssetMetaFormatError,
    },
    AssetMetadataPathMismatch {
        requested: AssetPath,
        declared: AssetPath,
    },
    AssetIdentity {
        path: AssetPath,
        source: AssetDatabaseError,
    },
    CatalogueNameInvalid,
    AssetMetadataChanged {
        path: AssetPath,
    },
    PrefabDirectoryAuthorityUnavailable,
    AssetDirectoryAuthorityUnavailable,
    AccountingOverflow,
    Budget(AuthoringProjectAdmissionError),
}

impl From<AuthoringProjectAdmissionError> for AuthoringProjectSourceAdmissionError {
    fn from(source: AuthoringProjectAdmissionError) -> Self {
        Self::Budget(source)
    }
}

impl Display for AuthoringProjectSourceAdmissionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingStartupScene => {
                formatter.write_str("project manifest does not select a startup Scene")
            }
            Self::SourceDirectoryPath { kind, .. } => {
                write!(formatter, "{kind} source directory path is invalid")
            }
            Self::SourceDirectoryHost { kind, .. } => {
                write!(formatter, "{kind} source directory admission failed")
            }
            Self::JoinedSourcePath(_) => {
                formatter.write_str("project source path could not be composed")
            }
            Self::SourcePath { kind, path, .. } => {
                write!(
                    formatter,
                    "{kind} source path '{path}' is invalid for Host admission"
                )
            }
            Self::SourceHost { kind, path, .. } => {
                write!(formatter, "{kind} source '{path}' could not be admitted")
            }
            Self::SourceIdentityKindConflict { path } => write!(
                formatter,
                "project source '{path}' aliases another admitted source kind"
            ),
            Self::SourceTooLarge {
                kind,
                path,
                maximum,
            } => write!(
                formatter,
                "{kind} source '{path}' exceeds its {maximum}-byte admission limit"
            ),
            Self::UnsupportedSourceEncoding { kind, path } => {
                write!(
                    formatter,
                    "{kind} source '{path}' uses an unsupported encoding"
                )
            }
            Self::SourceFormat { kind, path, .. } => {
                write!(formatter, "{kind} source '{path}' is invalid")
            }
            Self::DocumentSourcePath { path, .. } => write!(
                formatter,
                "project source '{path}' cannot become an authoring document identity"
            ),
            Self::SourcePublication { kind, path, .. } => {
                write!(
                    formatter,
                    "{kind} source '{path}' could not be published for authoring"
                )
            }
            Self::AssetMetadataFormat { path, .. } => {
                write!(formatter, "asset metadata for '{path}' is invalid")
            }
            Self::AssetMetadataPathMismatch {
                requested,
                declared,
            } => write!(
                formatter,
                "asset metadata for '{requested}' declares source path '{declared}'"
            ),
            Self::AssetIdentity { path, .. } => write!(
                formatter,
                "asset identity for '{path}' conflicts within the authoring project closure"
            ),
            Self::CatalogueNameInvalid => formatter
                .write_str("asset catalogue entry cannot become a portable UTF-8 logical path"),
            Self::AssetMetadataChanged { path } => write!(
                formatter,
                "asset metadata for '{path}' changed after stable reference lookup; retry admission",
            ),
            Self::PrefabDirectoryAuthorityUnavailable => formatter
                .write_str("Prefab closure traversal lost its retained directory authority"),
            Self::AssetDirectoryAuthorityUnavailable => {
                formatter.write_str("asset closure traversal lost its retained directory authority")
            }
            Self::AccountingOverflow => {
                formatter.write_str("authoring project admission accounting overflowed")
            }
            Self::Budget(source) => Display::fmt(source, formatter),
        }
    }
}

impl Error for AuthoringProjectSourceAdmissionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SourceDirectoryPath { source, .. } | Self::SourcePath { source, .. } => {
                Some(source)
            }
            Self::SourceDirectoryHost { source, .. } | Self::SourceHost { source, .. } => {
                Some(source)
            }
            Self::JoinedSourcePath(source) => Some(source),
            Self::SourceFormat { source, .. } => Some(source),
            Self::DocumentSourcePath { source, .. } => Some(source),
            Self::SourcePublication { source, .. } => Some(source),
            Self::AssetMetadataFormat { source, .. } => Some(source),
            Self::AssetIdentity { source, .. } => Some(source),
            Self::Budget(source) => Some(source),
            Self::MissingStartupScene
            | Self::SourceIdentityKindConflict { .. }
            | Self::SourceTooLarge { .. }
            | Self::UnsupportedSourceEncoding { .. }
            | Self::AssetMetadataPathMismatch { .. }
            | Self::CatalogueNameInvalid
            | Self::AssetMetadataChanged { .. }
            | Self::PrefabDirectoryAuthorityUnavailable
            | Self::AssetDirectoryAuthorityUnavailable
            | Self::AccountingOverflow => None,
        }
    }
}
