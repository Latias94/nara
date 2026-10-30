use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    mem::size_of,
    sync::Arc,
};

use nara_asset::{
    AssetDatabaseError, AssetMetaFileLimits, AssetMetaFormatError, AssetMetaIdentityCandidate,
    AssetPath, AssetPathError, AssetPathScope, AssetRecord, AssetRef, AssetReferenceResolver,
    AssetSourceKind, AssetSourcePath, ProjectAssetDatabase,
};
use nara_core::ByteLimit;
use nara_diagnostic::DiagnosticReport;
use nara_fs::{
    ContentDigest, DirectoryCapability, FileCapability, FileIdentity, FsError, PathValidationError,
    RelativePath,
};
use nara_project::{EffectiveProjectSettings, ProjectPath, ProjectPathError};
use nara_reflect::{AuthoringSchemaBundle, ComponentFieldPathSegment, DeclaredAssetReference};
use nara_scene::__private::{AuthoringAdapterReceipt, fork_published_authoring_scene_document};
use nara_scene::{
    AuthoringDocumentPublicationError, AuthoringDocumentReadiness, AuthoringDocumentSource,
    AuthoringRecordFact, AuthoringSchemaBasis, PrefabDocument, PrefabDocumentCandidate,
    PrefabInstance, PublishedAuthoringPrefabDocument, PublishedAuthoringSceneDocument,
    ResolvedPrefabSource, SceneDocument, SceneEntityRecord, SceneFileLimits,
    SceneFilePublicationError, SceneFormatError, UnavailableRecordManifest,
    UnavailableRecordManifestEntry,
};

use super::{
    AUTHORING_BASIS_ENCODED_MULTIPLIER, AuthoringProjectAdmissionBudget,
    AuthoringProjectAdmissionError, AuthoringProjectAdmissionUsage,
    AuthoringProjectAttemptGeneration, AuthoringSchemaReceipt, checked_retained_replacement,
    document::{AuthoringDocumentDataError, admit_scene_bytes_with_basis},
    source_index::ProjectSourceIndexState,
};

mod capture;
mod catalogue;
#[cfg(any(feature = "tooling", test))]
mod schema_owner;
pub(crate) use capture::{
    AuthoringDocumentCapture, AuthoringPrefabSessionSnapshot, AuthoringPrefabSourceCapture,
    AuthoringSceneSessionSnapshot, AuthoringSceneSource, AuthoringSourceCaptureInputs,
    AuthoringSourceValidation, CapturedAuthoringPreview,
};
#[cfg(any(feature = "tooling", test))]
pub(crate) use schema_owner::{
    AuthoringSchemaOwnerProjectInput, PreparedSchemaOwnerProject, PreparedSchemaOwnerSceneCapture,
    SchemaOwnerObservedProjectAttachmentFailure, SchemaOwnerProjectAttachmentFailure,
};
#[cfg(test)]
mod tests;
use super::native_source::{AdmittedNativeDocumentSource, NativeDocumentSourcePolicy};
use catalogue::{ReferenceCatalogue, charge_database_record, insert_asset_record};

const SOURCE_OPEN_HANDLE_PEAK: usize = 7;
const VALUE_NODE_RETAINED_BYTES: usize = 32;
const AUTHORING_RECORD_RETAINED_OVERHEAD: usize = 64;
const DEGRADED_ISSUE_RETAINED_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum AuthoringSourceKind {
    ProjectManifest,
    SchemaBundle,
    Scene,
    Prefab,
    InputActionMap,
    AssetMetadata,
    AssetSource,
}

impl Display for AuthoringSourceKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ProjectManifest => "project manifest",
            Self::SchemaBundle => "authoring Schema bundle",
            Self::Scene => "Scene",
            Self::Prefab => "Prefab",
            Self::InputActionMap => "Input action map",
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
    /// Retains the same opened object whose historical source observation was validated.
    fn collect_reclamation_source(
        &self,
        root: &DirectoryCapability,
        output: &mut Vec<FileCapability>,
    ) -> Result<(), FsError> {
        let path = RelativePath::new(self.project_path.as_str())?;
        let file = root.open_file_exact(&path)?;
        if file.identity() != self.identity {
            return Err(FsError::IdentityMismatch {
                expected: self.identity,
                actual: file.identity(),
            });
        }
        if file.parent_identity() != self.parent_identity {
            return Err(FsError::Unproven {
                operation: nara_fs::FsOperation::InspectHandle,
                proof: "the historical source parent no longer matches",
            });
        }
        file.verify_digest(self.digest, self.digest.length())?;
        output.push(file);
        Ok(())
    }

    pub(super) fn native_metadata(
        project_path: ProjectPath,
        identity: FileIdentity,
        parent_identity: Option<FileIdentity>,
        digest: ContentDigest,
        encoded_bytes: usize,
        dependency_depth: usize,
    ) -> Self {
        Self {
            kind: AuthoringSourceKind::AssetMetadata,
            project_path,
            identity,
            parent_identity,
            digest,
            encoded_bytes,
            dependency_depth,
        }
    }

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

    /// Traversal depth describes a capture edge, not the observed file.
    fn same_file_observation(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.project_path == other.project_path
            && self.identity == other.identity
            && self.parent_identity == other.parent_identity
            && self.digest == other.digest
            && self.encoded_bytes == other.encoded_bytes
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
    binding: catalogue::SourceDirectoryBinding,
    native_source: AdmittedNativeDocumentSource,
    source: Arc<AuthoringSourceFact>,
    content: AuthoringPrefabContent,
    basis: AuthoringSchemaBasis,
    limits: SceneFileLimits,
}

#[derive(Clone)]
enum AuthoringPrefabContent {
    File(Arc<PrefabDocument>),
    Session {
        document: Arc<PrefabDocument>,
        revision: nara_scene::SceneAuthoringRevision,
    },
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
    pub(crate) const fn path(&self) -> &AssetSourcePath {
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
    pub(crate) fn native_source(&self) -> &AdmittedNativeDocumentSource {
        &self.native_source
    }

    #[must_use]
    pub(crate) const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub(crate) fn source(&self) -> &AuthoringSourceFact {
        &self.source
    }

    #[must_use]
    pub(crate) fn document(&self) -> &PrefabDocument {
        self.shared_source_document()
    }

    fn shared_source_document(&self) -> &Arc<PrefabDocument> {
        match &self.content {
            AuthoringPrefabContent::File(document)
            | AuthoringPrefabContent::Session { document, .. } => document,
        }
    }

    /// The file receipt and this revision describe separate authorities.
    pub(crate) fn session_revision(&self) -> Option<nara_scene::SceneAuthoringRevision> {
        match &self.content {
            AuthoringPrefabContent::File(_) => None,
            AuthoringPrefabContent::Session { revision, .. } => Some(*revision),
        }
    }

    /// Retains raw declarations and their exact authority for deferred composition.
    pub(crate) fn resolved_source(&self) -> ResolvedPrefabSource {
        ResolvedPrefabSource::bound_authoring_source(
            Arc::clone(self.shared_source_document()),
            self.basis.clone(),
            self.native_source.key().clone(),
        )
    }

    /// Strictly classifies a cloned raw source against its exact captured Schema basis.
    ///
    /// Composition capture alone cannot establish standalone publishability. This may reject
    /// a source whose discarded values are harmless in a repaired outer composition.
    pub(crate) fn publish_document(
        &self,
    ) -> Result<PublishedAuthoringPrefabDocument, AuthoringDocumentDataError> {
        if self.session_revision().is_some() {
            return Err(AuthoringDocumentDataError::SessionSnapshot);
        }
        let source_path = AssetPath::new(self.source.project_path().as_str().to_owned())
            .map_err(AuthoringDocumentDataError::Source)?;
        let candidate =
            PrefabDocumentCandidate::from_source_document(self.document().clone(), self.limits)
                .map_err(AuthoringDocumentDataError::Format)?;
        Ok(candidate
            .admit_authoring_with_basis(
                AuthoringDocumentSource::new(AssetRef::Path(source_path)),
                &self.basis,
            )
            .map_err(AuthoringDocumentDataError::Publication)?
            .publish())
    }
}

impl Debug for AuthoringPrefabSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringPrefabSource")
            .field("path", &self.path)
            .field("source", &self.source)
            .field("entity_count", &self.document().entities.len())
            .field("session_revision", &self.session_revision())
            .finish_non_exhaustive()
    }
}

struct AuthoringProjectSnapshotInner {
    schema: Arc<AuthoringSchemaReceipt>,
    adapters: Box<[AuthoringAdapterReceipt]>,
    scene_limits: SceneFileLimits,
    readiness: AuthoringProjectReadiness,
    startup_compilation_diagnostics: DiagnosticReport,
    startup_composition_failure: Option<nara_scene::PrefabExpansionFailure>,
    startup_preview: Option<CapturedAuthoringPreview>,
    startup_source: Arc<AuthoringSourceFact>,
    startup_native_source: AdmittedNativeDocumentSource,
    startup_scene: Arc<PublishedAuthoringSceneDocument>,
    scenes: Box<[AuthoringSceneSource]>,
    prefabs: Box<[AuthoringPrefabSource]>,
    assets: Box<[AuthoringAssetSource]>,
    asset_database: Arc<ProjectAssetDatabase>,
    asset_scope: Arc<AssetPathScope>,
    prefab_scope: Arc<AssetPathScope>,
    unresolved_dependencies: Box<[AuthoringUnresolvedDependency]>,
    source_facts: Arc<[Arc<AuthoringSourceFact>]>,
    stable_lookups: Arc<[catalogue::StableLookupObservation]>,
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

    /// Readiness of captured source inventory; effective activation is owned by Scene.
    #[must_use]
    pub(crate) fn readiness(&self) -> AuthoringProjectReadiness {
        self.inner.readiness
    }

    pub(crate) fn startup_compilation_diagnostics(&self) -> &DiagnosticReport {
        &self.inner.startup_compilation_diagnostics
    }

    pub(crate) fn startup_composition_failure(
        &self,
    ) -> Option<&nara_scene::PrefabExpansionFailure> {
        self.inner.startup_composition_failure.as_ref()
    }

    #[must_use]
    pub(crate) fn startup_source(&self) -> &AuthoringSourceFact {
        &self.inner.startup_source
    }

    #[must_use]
    pub(crate) fn startup_scene(&self) -> &PublishedAuthoringSceneDocument {
        &self.inner.startup_scene
    }

    /// Immutable sources actually requested by the selected declaration compiler.
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

    pub(crate) fn into_session_snapshot(
        self,
    ) -> Result<AuthoringProjectSessionSnapshot, AuthoringProjectReceiptUseError> {
        let inner = Arc::try_unwrap(self.snapshot.inner)
            .map_err(|_| AuthoringProjectReceiptUseError::SnapshotAliased)?;
        let mut source_index = ProjectSourceIndexState::new(&inner.schema);
        let _ = source_index.request_refresh();
        Ok(AuthoringProjectSessionSnapshot {
            schema: inner.schema,
            adapters: Arc::from(inner.adapters),
            scene_limits: inner.scene_limits,
            readiness: inner.readiness,
            startup_compilation_diagnostics: inner.startup_compilation_diagnostics,
            startup_composition_failure: inner.startup_composition_failure,
            startup_preview: inner.startup_preview,
            startup_source: inner.startup_source,
            startup_native_source: inner.startup_native_source,
            startup_scene: inner.startup_scene,
            scenes: inner.scenes,
            prefabs: inner.prefabs,
            assets: inner.assets,
            asset_database: inner.asset_database,
            asset_scope: inner.asset_scope,
            prefab_scope: inner.prefab_scope,
            unresolved_dependencies: inner.unresolved_dependencies,
            source_facts: inner.source_facts,
            stable_lookups: inner.stable_lookups,
            usage: inner.usage,
            source_index,
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

pub(crate) struct AuthoringProjectSessionSnapshot {
    schema: Arc<AuthoringSchemaReceipt>,
    adapters: Arc<[AuthoringAdapterReceipt]>,
    scene_limits: SceneFileLimits,
    readiness: AuthoringProjectReadiness,
    startup_compilation_diagnostics: DiagnosticReport,
    startup_composition_failure: Option<nara_scene::PrefabExpansionFailure>,
    startup_preview: Option<CapturedAuthoringPreview>,
    startup_source: Arc<AuthoringSourceFact>,
    startup_native_source: AdmittedNativeDocumentSource,
    startup_scene: Arc<PublishedAuthoringSceneDocument>,
    scenes: Box<[AuthoringSceneSource]>,
    prefabs: Box<[AuthoringPrefabSource]>,
    assets: Box<[AuthoringAssetSource]>,
    asset_database: Arc<ProjectAssetDatabase>,
    asset_scope: Arc<AssetPathScope>,
    prefab_scope: Arc<AssetPathScope>,
    unresolved_dependencies: Box<[AuthoringUnresolvedDependency]>,
    source_facts: Arc<[Arc<AuthoringSourceFact>]>,
    stable_lookups: Arc<[catalogue::StableLookupObservation]>,
    usage: AuthoringProjectAdmissionUsage,
    source_index: ProjectSourceIndexState,
}

impl AuthoringProjectSessionSnapshot {
    /// Checks retained schema handles; historical sources are collected separately.
    pub(crate) fn source_object_in_use(&self, candidate: &FileCapability) -> Result<bool, FsError> {
        self.schema.source_object_in_use(candidate)
    }

    /// Collects the bounded historical source set once for all reclamation candidates.
    /// An error invalidates the entire collection, including any files already appended.
    pub(crate) fn collect_reclamation_sources(
        &self,
        output: &mut Vec<FileCapability>,
    ) -> Result<(), FsError> {
        self.schema.collect_reclamation_sources(output)?;
        for source in self.source_facts.iter() {
            source.collect_reclamation_source(&self.schema.project_root, output)?;
        }
        Ok(())
    }

    #[cfg(feature = "tooling")]
    pub(crate) fn source_index_scan_pending(&self) -> bool {
        self.source_index.schema_adoption_pending()
    }
    pub(crate) fn admit_startup_document(
        &self,
    ) -> Result<super::AdmittedAuthoringDocument, super::AuthoringDocumentAdmissionError> {
        let source = super::document::AuthoringDocumentSourceFact::from_captured_source(
            &self.startup_source,
            &self.startup_native_source,
        )?;
        super::document::admit_captured_startup_scene(
            &self.schema.project_root(),
            &source,
            &self.startup_scene.schema_basis(),
            self.scene_limits,
        )
    }

    pub(crate) fn take_startup_preview(&mut self) -> Option<CapturedAuthoringPreview> {
        self.startup_preview.take()
    }

    pub(crate) fn startup_source_validation(&self) -> AuthoringSourceValidation {
        AuthoringSourceValidation::new(
            Arc::clone(&self.schema),
            Arc::clone(&self.source_facts),
            Arc::clone(&self.stable_lookups),
            self.usage.retained_candidate_bytes(),
        )
    }

    pub(crate) fn capture_scene_document(
        &self,
        source: &super::document::AuthoringDocumentSourceFact,
        session: &nara_scene::SceneAuthoringSession,
        open_prefabs: &[AuthoringPrefabSessionSnapshot],
    ) -> Result<AuthoringDocumentCapture, AuthoringProjectSourceAdmissionError> {
        let inputs = self.admit_source_capture_inputs(
            0,
            checked_sum([
                capture::prefabs::open_prefab_inputs_retained_bytes(open_prefabs)?,
                source
                    .checked_retained_bytes()
                    .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
            ])?,
        )?;
        self.capture_scene_request(
            source,
            session,
            inputs,
            open_prefabs.iter().cloned().collect(),
        )?
        .prepare()
    }

    /// Admits Host and worker custody before source selections or working snapshots are copied.
    /// Host retention transfers into the capture; worker retention ends with the request.
    pub(crate) fn admit_source_capture_inputs(
        &self,
        host_retained_bytes: usize,
        worker_retained_bytes: usize,
    ) -> Result<AuthoringSourceCaptureInputs, AuthoringProjectSourceAdmissionError> {
        let retained_bytes = checked_sum([host_retained_bytes, worker_retained_bytes])?;
        let mut budget = self.schema.budget;
        checked_retained_replacement(
            budget.retained_candidate_bytes,
            0,
            retained_bytes,
            budget.limits.retained_candidate_bytes().get(),
        )?;
        budget.charge_source_bytes(0, 0, 0, retained_bytes)?;
        Ok(AuthoringSourceCaptureInputs {
            basis: self.startup_scene.schema_basis(),
            host_retained_bytes,
            worker_retained_bytes,
        })
    }

    pub(crate) fn capture_scene_request(
        &self,
        source: &super::document::AuthoringDocumentSourceFact,
        session: &nara_scene::SceneAuthoringSession,
        inputs: AuthoringSourceCaptureInputs,
        open_prefabs: Box<[AuthoringPrefabSessionSnapshot]>,
    ) -> Result<capture::OwnedAuthoringSceneCapture, AuthoringProjectSourceAdmissionError> {
        if inputs.basis != self.startup_scene.schema_basis() {
            return Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch);
        }
        let basis = session
            .authoring_schema_basis()
            .filter(|basis| basis == &self.startup_scene.schema_basis())
            .ok_or(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch)?;
        if source.source_key().kind() != nara_scene::SourceDocumentKind::Scene {
            return Err(AuthoringProjectSourceAdmissionError::NativeSource(
                super::AuthoringDocumentAdmissionError::DocumentKindMismatch,
            ));
        }
        if !capture::source_locator_matches(session.authoring_source(), source.relative_path()) {
            return Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch);
        }
        Ok(capture::OwnedAuthoringSceneCapture {
            schema: Arc::clone(&self.schema),
            scene_limits: self.scene_limits,
            source: source.clone(),
            document: session.shared_document(),
            revision: session.revision(),
            basis,
            readiness: AuthoringProjectReadiness::from_document(
                session
                    .authoring_readiness()
                    .ok_or(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch)?,
            ),
            adapter_retained: adapter_receipts_retained_bytes(&self.adapters)?,
            open_prefabs,
            open_scenes: Box::new([]),
            inputs,
        })
    }

    pub(crate) fn startup_native_source(&self) -> &AdmittedNativeDocumentSource {
        &self.startup_native_source
    }

    pub(crate) fn scene_sources(&self) -> &[AuthoringSceneSource] {
        &self.scenes
    }

    #[must_use]
    pub(crate) fn asset_database(&self) -> Arc<ProjectAssetDatabase> {
        Arc::clone(&self.asset_database)
    }

    pub(crate) fn asset_scope(&self) -> Arc<AssetPathScope> {
        Arc::clone(&self.asset_scope)
    }

    pub(crate) fn prefab_scope(&self) -> Arc<AssetPathScope> {
        Arc::clone(&self.prefab_scope)
    }

    pub(crate) fn asset_resolution(&self) -> AssetReferenceResolver<'_> {
        self.asset_database.resolver(&self.asset_scope)
    }

    pub(crate) fn prefab_resolution(&self) -> AssetReferenceResolver<'_> {
        self.asset_database.resolver(&self.prefab_scope)
    }

    #[must_use]
    pub(crate) fn attempt_generation(&self) -> AuthoringProjectAttemptGeneration {
        self.schema.attempt_generation()
    }

    #[must_use]
    pub(crate) fn settings(&self) -> &EffectiveProjectSettings {
        self.schema.settings()
    }

    #[must_use]
    pub(crate) fn project_root(&self) -> DirectoryCapability {
        self.schema.project_root()
    }

    pub(crate) fn authoring_bundle(&self) -> &AuthoringSchemaBundle {
        self.schema.bundle()
    }

    #[must_use]
    pub(crate) fn manifest_digest(&self) -> ContentDigest {
        self.schema.manifest_digest()
    }

    #[must_use]
    pub(crate) fn authoring_bundle_digest(&self) -> ContentDigest {
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

    pub(crate) fn startup_compilation_diagnostics(&self) -> &DiagnosticReport {
        &self.startup_compilation_diagnostics
    }

    pub(crate) fn startup_composition_failure(
        &self,
    ) -> Option<&nara_scene::PrefabExpansionFailure> {
        self.startup_composition_failure.as_ref()
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
        let canonical = self.prefab_scope.resolve(path);
        self.prefabs
            .iter()
            .find(|prefab| prefab.source().project_path().as_str() == canonical.as_str())
    }

    pub(crate) fn prefab_sources(&self) -> impl Iterator<Item = &AuthoringPrefabSource> {
        self.prefabs.iter()
    }

    /// Independently classifies a captured source before creating an editable document.
    pub(crate) fn fork_prefab(
        &self,
        path: &AssetPath,
    ) -> Result<Option<PublishedAuthoringPrefabDocument>, AuthoringDocumentDataError> {
        self.prefab(path)
            .map(AuthoringPrefabSource::publish_document)
            .transpose()
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
            &self.startup_scene.schema_basis(),
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
    schema: Arc<AuthoringSchemaReceipt>,
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
    pub(crate) fn with_scene_limits(
        schema: AuthoringSchemaReceipt,
        adapters: Box<[AuthoringAdapterReceipt]>,
        scene_limits: SceneFileLimits,
    ) -> Self {
        Self {
            schema: Arc::new(schema),
            adapters,
            scene_limits,
        }
    }

    pub(crate) fn admit_startup_closure(
        self,
    ) -> Result<AuthoringProjectCandidate, AuthoringProjectSourceAdmissionError> {
        let mut budget = self.schema.budget;
        let adapter_retained = adapter_receipts_retained_bytes(&self.adapters)?;
        budget.charge_source_bytes(0, 0, 0, adapter_retained)?;
        let startup_read = read_saved_startup(&self.schema, self.scene_limits, &mut budget)?;
        let basis = AuthoringSchemaBasis::admit(self.schema.bundle.bundle(), &self.adapters)
            .map_err(|error| {
                map_document_data_error(
                    &startup_read,
                    AuthoringDocumentDataError::Publication(error),
                    &mut budget,
                )
            })?;
        let mut startup = capture_saved_startup(
            &self.schema,
            self.scene_limits,
            startup_read,
            &basis,
            self.schema.bundle_bytes().len(),
            &mut budget,
            &[],
            &[],
            capture::AuthoringCapturePreview::Prepare,
        )?;
        #[cfg(feature = "presentation-2d")]
        reserve_preview_identity_store(&startup.capture, &mut budget)?;
        startup.capture.usage = budget.usage();
        Ok(startup.into_candidate(
            self.schema,
            self.adapters,
            self.scene_limits,
            budget.usage(),
        ))
    }
}

/// The saved root and its observed file remain paired, separately from working Scene captures.
struct CapturedSavedStartup {
    source: Arc<AuthoringSourceFact>,
    native_source: AdmittedNativeDocumentSource,
    document: Arc<PublishedAuthoringSceneDocument>,
    capture: AuthoringDocumentCapture,
}

impl CapturedSavedStartup {
    fn into_candidate(
        self,
        schema: Arc<AuthoringSchemaReceipt>,
        adapters: Box<[AuthoringAdapterReceipt]>,
        scene_limits: SceneFileLimits,
        usage: AuthoringProjectAdmissionUsage,
    ) -> AuthoringProjectCandidate {
        let AuthoringDocumentCapture {
            readiness,
            diagnostics: startup_compilation_diagnostics,
            prefabs,
            scenes,
            assets,
            asset_database,
            asset_scope,
            prefab_scope,
            unresolved_dependencies,
            source_facts,
            stable_lookups,
            usage: _,
            host_retained_bytes: _,
            preview: startup_preview,
            composition_failure: startup_composition_failure,
            root_revision: _,
            authority: _,
        } = self.capture;

        AuthoringProjectCandidate {
            inner: AuthoringProjectSnapshotInner {
                schema,
                adapters,
                scene_limits,
                readiness,
                startup_compilation_diagnostics,
                startup_composition_failure,
                startup_preview,
                startup_source: self.source,
                startup_native_source: self.native_source,
                startup_scene: self.document,
                scenes,
                prefabs,
                assets,
                asset_database,
                asset_scope,
                prefab_scope,
                unresolved_dependencies,
                source_facts,
                stable_lookups,
                usage,
            },
        }
    }
}

fn read_saved_startup(
    schema: &AuthoringSchemaReceipt,
    scene_limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<ReadAuthoringSource, AuthoringProjectSourceAdmissionError> {
    let scene_root = &schema.settings.paths.scenes;
    let startup_path = schema
        .settings
        .startup
        .default_scene
        .as_ref()
        .ok_or(AuthoringProjectSourceAdmissionError::MissingStartupScene)?;
    let scenes = open_source_directory(
        &schema.project_root,
        scene_root,
        AuthoringSourceKind::Scene,
        budget,
    )?;
    budget.charge_reference(startup_path.as_str().len(), 0, false)?;
    let startup_project_path = join_project_path(scene_root, startup_path.as_str())?;
    let opened = open_source_file(
        &scenes,
        startup_project_path,
        startup_path.as_str(),
        AuthoringSourceKind::Scene,
        0,
        budget,
    )?;
    if opened.identity == schema.manifest_identity() || opened.identity == schema.bundle_identity()
    {
        return Err(
            AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict {
                path: opened.project_path,
            },
        );
    }
    read_unique_source(opened, scene_limits.encoded_bytes().get(), budget)
}

fn capture_saved_startup(
    schema: &Arc<AuthoringSchemaReceipt>,
    scene_limits: SceneFileLimits,
    read: ReadAuthoringSource,
    basis: &AuthoringSchemaBasis,
    bundle_encoded_bytes: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
    open_prefabs: &[AuthoringPrefabSessionSnapshot],
    open_scenes: &[AuthoringSceneSessionSnapshot],
    preview: capture::AuthoringCapturePreview,
) -> Result<CapturedSavedStartup, AuthoringProjectSourceAdmissionError> {
    let document = admit_scene_document(&read, scene_limits, basis, bundle_encoded_bytes, budget)?;
    let source = Arc::clone(&read.source);
    let native_source = NativeDocumentSourcePolicy::new(schema.settings()).admit_budgeted(
        &schema.project_root,
        source.project_path().as_str(),
        nara_scene::SourceDocumentKind::Scene,
        source.identity(),
        0,
        3,
        budget,
    )?;
    capture::charge_open_prefab_inputs(open_prefabs, budget)?;
    budget.charge_source_bytes(
        0,
        0,
        0,
        capture::scenes::open_scene_inputs_retained_bytes(open_scenes)?,
    )?;
    let capture = capture::capture_scene(
        schema,
        scene_limits,
        document.document(),
        basis,
        &source,
        &native_source,
        AuthoringProjectReadiness::from_document(document.readiness()),
        budget,
        open_prefabs,
        open_scenes,
        preview,
        schema.settings.input.action_map.as_ref(),
    )?;
    Ok(CapturedSavedStartup {
        source,
        native_source,
        document,
        capture,
    })
}

#[cfg(feature = "presentation-2d")]
fn reserve_preview_identity_store(
    capture: &AuthoringDocumentCapture,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    for asset in &capture.assets {
        charge_database_record(asset.record(), budget)?;
    }
    for prefab in &capture.prefabs {
        if let Some(record) = capture.prefab_resolution().record_for_path(prefab.path()) {
            charge_database_record(record, budget)?;
        }
    }
    budget.charge_source_bytes(
        0,
        0,
        0,
        size_of::<ProjectAssetDatabase>() + 2 * size_of::<usize>(),
    )?;
    let growth = checked_sum([
        checked_product(super::MAX_AUTHORING_IMAGE_IDENTITY_PATH_BYTES, 9)?,
        checked_product(
            super::MAX_AUTHORING_IMAGE_IDENTITIES,
            size_of::<AssetRecord>() + 3 * size_of::<AssetSourcePath>() + 192,
        )?,
    ])?;
    budget.charge_source_bytes(0, 0, 0, growth)?;
    Ok(())
}

enum LoadedIdentity {
    SchemaBasis,
    ContainingDocument,
    AssetMetadata,
    AssetSource,
    Prefab,
    InputActionMap,
}

fn include_native_metadata_source(
    native_source: &AdmittedNativeDocumentSource,
    identities: &mut HashMap<FileIdentity, LoadedIdentity>,
    source_facts: &mut BTreeMap<String, Arc<AuthoringSourceFact>>,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let Some(source) = native_source.metadata_source() else {
        return Ok(());
    };
    let path = source.project_path();
    if source_facts
        .get(path.as_str())
        .is_some_and(|existing| existing.same_file_observation(source))
    {
        return Ok(());
    }
    if identities.contains_key(&source.identity()) || source_facts.contains_key(path.as_str()) {
        return Err(
            AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict { path: path.clone() },
        );
    }
    identities.insert(source.identity(), LoadedIdentity::AssetMetadata);
    source_facts.insert(path.as_str().to_owned(), Arc::clone(source));
    Ok(())
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
    let preflight = RelativePath::preflight(path.as_str()).map_err(|source| {
        AuthoringProjectSourceAdmissionError::SourceDirectoryPath { kind, source }
    })?;
    budget.charge_source_bytes(
        0,
        0,
        exact_open_work(preflight.components(), path.as_str().len()),
        0,
    )?;
    let relative = RelativePath::new(path.as_str()).map_err(|source| {
        AuthoringProjectSourceAdmissionError::SourceDirectoryPath { kind, source }
    })?;
    let directory = root.open_directory_exact(&relative).map_err(|source| {
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
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<OpenedAuthoringSource, AuthoringProjectSourceAdmissionError> {
    let preflight = RelativePath::preflight(logical_path).map_err(|source| {
        AuthoringProjectSourceAdmissionError::SourcePath {
            kind,
            path: project_path.clone(),
            source,
        }
    })?;
    budget.charge_source_bytes(
        0,
        0,
        exact_open_work(preflight.components(), logical_path.len()),
        0,
    )?;
    let relative = RelativePath::new(logical_path).map_err(|source| {
        AuthoringProjectSourceAdmissionError::SourcePath {
            kind,
            path: project_path.clone(),
            source,
        }
    })?;
    let capability = directory.open_file_exact(&relative).map_err(|source| {
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

fn exact_open_work(components: usize, path_bytes: usize) -> usize {
    components
        .saturating_mul(65_536)
        .saturating_add(path_bytes.saturating_mul(2))
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

pub(super) fn source_fact_retained_bytes(
    source: &AuthoringSourceFact,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    source_fact_path_retained_bytes(source.project_path().as_str())
}

fn source_fact_path_retained_bytes(
    path: &str,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    checked_sum([
        size_of::<AuthoringSourceFact>(),
        path.len(),
        size_of::<Arc<AuthoringSourceFact>>(),
    ])
}

fn admit_scene_document(
    source: &ReadAuthoringSource,
    scene_limits: SceneFileLimits,
    basis: &AuthoringSchemaBasis,
    bundle_encoded_bytes: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Arc<PublishedAuthoringSceneDocument>, AuthoringProjectSourceAdmissionError> {
    let encoded_limit = ByteLimit::new(source.read_limit)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    let limits = scene_limits.with_encoded_bytes(encoded_limit);
    let published = Arc::new(
        admit_scene_bytes_with_basis(source.source.project_path(), &source.bytes, limits, basis)
            .map_err(|error| map_document_data_error(source, error, budget))?,
    );
    charge_published_document(
        PublishedDocumentCharge {
            source: &source.source,
            encoded_bytes: source.bytes.len(),
            document: published.document(),
            record_facts: published.record_facts(),
            asset_references_retained_bytes: declared_asset_references_retained_bytes(
                published.schema_declared_asset_references(),
            )?,
            unavailable_records: published.unavailable_manifest().total_count(),
            overlay_declarations_retained_bytes: manifest_overlay_declarations_retained_bytes(
                published.unavailable_manifest(),
            )?,
            diagnostics: published.diagnostics(),
            bundle_encoded_bytes,
        },
        budget,
    )?;
    Ok(published)
}

fn admit_prefab_composition_source(
    source: &ReadAuthoringSource,
    scene_limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(Arc<PrefabDocument>, SceneFileLimits), AuthoringProjectSourceAdmissionError> {
    let reservation = reserve_composition_decode(source, scene_limits, budget)?;
    let limits = reservation.limits;
    let path = source.source.project_path().as_str();
    let candidate = if path.ends_with(".ron") {
        PrefabDocumentCandidate::decode_ron_bytes_with_limits(&source.bytes, limits)
    } else if path.ends_with(".json") {
        PrefabDocumentCandidate::decode_json_bytes_with_limits(&source.bytes, limits)
    } else {
        return Err(map_document_data_error(
            source,
            AuthoringDocumentDataError::UnsupportedEncoding,
            budget,
        ));
    }
    .map_err(|error| reservation.format_error(source, error))?;
    let document = candidate.into_composition_source().map_err(|error| {
        if let Err(budget_error) = budget.charge_diagnostics(
            error.diagnostics().len(),
            error.diagnostics().retained_bytes(),
        ) {
            return budget_error.into();
        }
        AuthoringProjectSourceAdmissionError::SourceStructure {
            kind: source.source.kind(),
            path: source.source.project_path().clone(),
            source: error,
        }
    })?;
    let retained = checked_sum([
        document.retained_bytes()?,
        source_fact_retained_bytes(&source.source)?,
        size_of::<Arc<PrefabDocument>>(),
    ])?;
    budget.reconcile_retained_reservation(reservation.bytes, retained)?;
    budget.charge_source_bytes(
        0,
        document.retained_bytes()?,
        checked_sum([document.retained_bytes()?, source.bytes.len()])?,
        0,
    )?;
    Ok((Arc::new(document), limits))
}

/// The production decoder enforces this shape ceiling before allocating domain data.
/// Parser/domain memory is reserved separately from the finite wire scan work.
struct CompositionDecodeReservation {
    bytes: usize,
    limits: SceneFileLimits,
    constrained_nodes: bool,
    maximum_retained: usize,
}

impl CompositionDecodeReservation {
    fn format_error(
        &self,
        source: &ReadAuthoringSource,
        error: SceneFormatError,
    ) -> AuthoringProjectSourceAdmissionError {
        if self.constrained_nodes
            && matches!(
                error.shape_error(),
                Some(
                    nara_core::SerdeShapeError::NodeLimitExceeded { .. }
                        | nara_core::SerdeShapeError::ContainerItemLimitExceeded { .. }
                )
            )
        {
            return AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                maximum: self.maximum_retained,
            }
            .into();
        }
        AuthoringProjectSourceAdmissionError::SourceFormat {
            kind: source.source.kind(),
            path: source.source.project_path().clone(),
            source: error,
        }
    }
}

fn reserve_composition_decode(
    source: &ReadAuthoringSource,
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<CompositionDecodeReservation, AuthoringProjectSourceAdmissionError> {
    let encoded = source.bytes.len().max(1);
    let maximum_retained = budget.limits.retained_candidate_bytes().get();
    let available = maximum_retained.saturating_sub(budget.retained_candidate_bytes);
    let bound = |nodes| {
        checked_sum([
            checked_product(encoded, 8)?,
            checked_product(nodes, 32)?,
            checked_product(limits.shape().container_items().get().min(nodes), 64)?,
            checked_product(
                limits.entities().get().min(nodes),
                checked_sum([
                    size_of::<SceneEntityRecord>(),
                    3 * size_of::<nara_scene::SceneEntityId>(),
                    128,
                ])?,
            )?,
            checked_product(
                limits.components().get().min(nodes),
                checked_sum([
                    size_of::<nara_scene::SceneComponentRecord>(),
                    size_of::<nara_reflect::ComponentTypeId>(),
                    128,
                ])?,
            )?,
            checked_product(
                limits.prefab_instances().get().min(nodes),
                size_of::<PrefabInstance>(),
            )?,
            checked_product(limits.overlay_declarations().get().min(nodes), 512)?,
            checked_product(limits.component_value_nodes().get().min(nodes), 96)?,
            checked_product(limits.diagnostic_sources().get().min(nodes), 128)?,
            source_fact_retained_bytes(&source.source)?,
            4 * size_of::<usize>(),
        ])
    };
    let mut lower = 0;
    let mut upper = limits.shape().nodes().get().min(encoded);
    while lower < upper {
        let midpoint = lower + (upper - lower) / 2 + 1;
        if bound(midpoint)? <= available {
            lower = midpoint;
        } else {
            upper = midpoint - 1;
        }
    }
    let nodes = nara_core::ItemLimit::new(lower).ok_or(
        AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
            maximum: maximum_retained,
        },
    )?;
    let shape = limits.shape();
    let limits = limits
        .with_encoded_bytes(
            ByteLimit::new(source.read_limit)
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        )
        .with_shape(nara_core::SerdeShapeLimits::new(
            shape.depth(),
            nodes,
            nara_core::ItemLimit::new(shape.container_items().get().min(nodes.get()))
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
            shape.string_bytes(),
            shape.total_string_bytes(),
        ));
    let bytes = bound(nodes.get())?;
    // Shape, header and payload parsing each scan the actual encoded input. Memory capacity
    // is not work performed; whitespace must not masquerade as millions of domain records.
    budget.charge_source_bytes(0, 0, checked_product(source.bytes.len(), 3)?, bytes)?;
    Ok(CompositionDecodeReservation {
        bytes,
        limits,
        constrained_nodes: nodes.get() < shape.nodes().get().min(encoded),
        maximum_retained,
    })
}

fn charge_composition_source(
    source: &AuthoringSourceFact,
    encoded_bytes: usize,
    document: &PrefabDocument,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let document_retained = document.retained_bytes()?;
    let retained_bytes = checked_sum([
        document_retained,
        source_fact_retained_bytes(source)?,
        size_of::<Arc<PrefabDocument>>(),
    ])?;
    // The startup publication owns the shared Schema basis. Raw dependencies retain no
    // standalone classification, reference projections or duplicate unavailable manifests.
    let decode_work = document_retained.saturating_add(encoded_bytes);
    budget.charge_source_bytes(0, document_retained, decode_work, retained_bytes)?;
    Ok(())
}

fn map_document_data_error(
    source: &ReadAuthoringSource,
    error: AuthoringDocumentDataError,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> AuthoringProjectSourceAdmissionError {
    match error {
        AuthoringDocumentDataError::SessionSnapshot => {
            AuthoringProjectSourceAdmissionError::SessionSnapshotPublication
        }
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
    asset_references_retained_bytes: usize,
    unavailable_records: usize,
    overlay_declarations_retained_bytes: usize,
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
            // Unavailable entries own cloned values and derived location/reference paths.
            // This conservative allowance is independent of the complete instance declarations
            // charged below, which also exist without unavailable component facts.
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
    // Unavailable complete records retain another reference projection in their manifest.
    // Schema defaults can introduce those references without any authored value bytes, so the
    // document-payload allowance above cannot cover them. Conservatively reserve a second copy
    // of all projections whenever unavailable entries exist.
    let asset_references_retained = checked_product(
        charge.asset_references_retained_bytes,
        if charge.unavailable_records == 0 {
            1
        } else {
            2
        },
    )?;
    let retained_bytes = checked_sum([
        document_retained,
        record_retained,
        asset_references_retained,
        unavailable_retained,
        charge.overlay_declarations_retained_bytes,
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
        checked_add_to(&mut total, prefab_instance_retained_bytes(prefab)?)?;
    }
    Ok(total)
}

fn prefab_instance_retained_bytes(
    prefab: &PrefabInstance,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    checked_sum([
        asset_ref_retained_bytes(&prefab.source)?,
        if matches!(&prefab.source, AssetRef::Path(_)) {
            crate::scene_retention::STRING_ALLOCATION_OVERHEAD
        } else {
            0
        },
        prefab.overrides.as_ref().map_or(Ok(0), |overlay| {
            crate::scene_retention::overlay_retained_bytes(overlay)
                .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)
        })?,
    ])
}

fn manifest_overlay_declarations_retained_bytes(
    manifest: &UnavailableRecordManifest,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    let mut total = 0;
    // Every retained anchor owns a complete instance copy, even when all component facts are
    // understood or the overlay contains only structural intent and nested detach claims.
    for (anchor, instance) in manifest.overlay_declarations() {
        checked_add_to(
            &mut total,
            checked_sum([
                size_of::<nara_scene::SceneEntityId>(),
                anchor.as_str().len(),
                crate::scene_retention::STRING_ALLOCATION_OVERHEAD,
                size_of::<PrefabInstance>(),
                crate::scene_retention::BTREE_ENTRY_OVERHEAD,
                prefab_instance_retained_bytes(instance)?,
            ])?,
        )?;
    }
    Ok(total)
}

fn asset_ref_retained_bytes(
    reference: &AssetRef,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    Ok(match reference {
        AssetRef::Path(path) => path.as_str().len(),
        AssetRef::StableId(_) => 16,
        AssetRef::Product(_) => size_of::<nara_asset::AssetProductRef>(),
    })
}

fn declared_asset_references_retained_bytes<'a>(
    references: impl IntoIterator<Item = &'a DeclaredAssetReference>,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    // These owned projections include effective defaults absent from the authored payload.
    let mut total = 0;
    for reference in references {
        checked_add_to(
            &mut total,
            checked_sum([
                size_of::<DeclaredAssetReference>(),
                reference.field_id().as_str().len(),
                checked_product(
                    reference.path().segments().len(),
                    size_of::<ComponentFieldPathSegment>(),
                )?,
                asset_ref_retained_bytes(reference.asset_ref())?,
            ])?,
        )?;
        for segment in reference.path().segments() {
            if let ComponentFieldPathSegment::Field(field) = segment {
                checked_add_to(&mut total, field.len())?;
            }
        }
    }
    Ok(total)
}

fn remaining_compiler_work(
    budget: &AuthoringProjectAdmissionBudget,
    reserved: usize,
    previous_limit: ByteLimit,
) -> Result<ByteLimit, AuthoringProjectSourceAdmissionError> {
    let host_retained = budget
        .retained_candidate_bytes
        .checked_sub(reserved)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    let available = budget
        .limits
        .retained_candidate_bytes()
        .get()
        .saturating_sub(host_retained);
    ByteLimit::new(available.min(previous_limit.get())).ok_or_else(|| {
        AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
            maximum: budget.limits.retained_candidate_bytes().get(),
        }
        .into()
    })
}

fn resize_compiler_work(
    budget: &mut AuthoringProjectAdmissionBudget,
    reserved: &mut usize,
    bytes: usize,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    if bytes > *reserved {
        budget.charge_source_bytes(0, 0, 0, bytes - *reserved)?;
    } else {
        budget.reconcile_retained_reservation(*reserved, bytes)?;
    }
    *reserved = bytes;
    Ok(())
}

fn is_missing_source(error: &AuthoringProjectSourceAdmissionError) -> bool {
    matches!(error,
        AuthoringProjectSourceAdmissionError::SourceHost { source: FsError::Io { source, .. }, .. }
        | AuthoringProjectSourceAdmissionError::SourceDirectoryHost { source: FsError::Io { source, .. }, .. }
        if source.kind() == std::io::ErrorKind::NotFound
    )
}

pub(super) fn revalidate_source_fact(
    root: &DirectoryCapability,
    source: &AuthoringSourceFact,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), AuthoringProjectSourceAdmissionError> {
    let changed = || AuthoringProjectSourceAdmissionError::SourceChanged {
        kind: source.kind(),
        path: source.project_path().clone(),
    };
    budget.admit_handle_peak(SOURCE_OPEN_HANDLE_PEAK)?;
    let preflight =
        RelativePath::preflight(source.project_path().as_str()).map_err(|_| changed())?;
    budget.charge_source_bytes(
        0,
        0,
        exact_open_work(preflight.components(), source.project_path().as_str().len()),
        0,
    )?;
    let relative = RelativePath::new(source.project_path().as_str()).map_err(|_| changed())?;
    let file = root.open_file_exact(&relative).map_err(|error| {
        AuthoringProjectSourceAdmissionError::SourceHost {
            kind: source.kind(),
            path: source.project_path().clone(),
            source: error,
        }
    })?;
    if file.identity() != source.identity() || file.parent_identity() != source.parent_identity() {
        return Err(changed());
    }
    let read_limit = source
        .encoded_bytes()
        .checked_add(1)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    budget.charge_source_bytes(0, 0, read_limit, read_limit)?;
    let bytes = file
        .read_to_end_bounded(
            u64::try_from(read_limit)
                .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        )
        .map_err(|error| match error {
            FsError::ByteLimitExceeded { .. } => changed(),
            error => AuthoringProjectSourceAdmissionError::SourceHost {
                kind: source.kind(),
                path: source.project_path().clone(),
                source: error,
            },
        })?;
    if bytes.len() != source.encoded_bytes() || ContentDigest::of_bytes(&bytes) != source.digest() {
        return Err(changed());
    }
    drop(bytes);
    budget.reconcile_retained_reservation(read_limit, 0)?;
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

pub(super) fn map_source_read_error(
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
    NativeSource(super::AuthoringDocumentAdmissionError),
    MissingStartupScene,
    SchemaBasisMismatch,
    DocumentSourceMismatch,
    AmbiguousPrefabSession {
        path: ProjectPath,
    },
    SessionSnapshotPublication,
    CompilationBudget(DiagnosticReport),
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
    SourceChanged {
        kind: AuthoringSourceKind,
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
    InputFormat {
        path: ProjectPath,
        source: nara_input::ActionMapFileError,
    },
    IntrinsicSourceMetadataMismatch {
        path: AssetSourcePath,
    },
    SourceStructure {
        kind: AuthoringSourceKind,
        path: ProjectPath,
        source: SceneFilePublicationError,
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
        path: AssetSourcePath,
        source: AssetMetaFormatError,
    },
    AssetMetadataPathMismatch {
        requested: AssetPath,
        declared: AssetPath,
    },
    AssetIdentity {
        path: AssetSourcePath,
        source: AssetDatabaseError,
    },
    CatalogueNameInvalid,
    AssetMetadataChanged {
        path: AssetSourcePath,
    },
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
            Self::NativeSource(source) => {
                write!(formatter, "native source admission failed: {source}")
            }
            Self::MissingStartupScene => {
                formatter.write_str("project manifest does not select a startup Scene")
            }
            Self::SchemaBasisMismatch => formatter
                .write_str("document capture requires the exact admitted authoring Schema basis"),
            Self::DocumentSourceMismatch => {
                formatter.write_str("document capture receipt belongs to another authoring source")
            }
            Self::AmbiguousPrefabSession { path } => write!(
                formatter,
                "multiple open Prefab sessions claim source '{path}'"
            ),
            Self::SessionSnapshotPublication => formatter.write_str(
                "an unsaved session snapshot cannot establish a file-backed document publication",
            ),
            Self::CompilationBudget(_) => formatter
                .write_str("selected authoring Prefab compilation exceeded its work budget"),
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
            Self::SourceChanged { kind, path } => write!(
                formatter,
                "{kind} source '{path}' changed during authoring capture; retry admission",
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
            Self::InputFormat { path, .. } => {
                write!(formatter, "Input action-map source '{path}' is invalid")
            }
            Self::IntrinsicSourceMetadataMismatch { path } => write!(
                formatter,
                "asset metadata for '{path}' disagrees with its intrinsic source identity"
            ),
            Self::SourceStructure { kind, path, .. } => {
                write!(
                    formatter,
                    "{kind} source '{path}' has invalid local structure"
                )
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
            Self::NativeSource(source) => Some(source),
            Self::SourceDirectoryPath { source, .. } | Self::SourcePath { source, .. } => {
                Some(source)
            }
            Self::SourceDirectoryHost { source, .. } | Self::SourceHost { source, .. } => {
                Some(source)
            }
            Self::JoinedSourcePath(source) => Some(source),
            Self::SourceFormat { source, .. } => Some(source),
            Self::InputFormat { source, .. } => Some(source),
            Self::SourceStructure { source, .. } => Some(source),
            Self::DocumentSourcePath { source, .. } => Some(source),
            Self::SourcePublication { source, .. } => Some(source),
            Self::AssetMetadataFormat { source, .. } => Some(source),
            Self::AssetIdentity { source, .. } => Some(source),
            Self::Budget(source) => Some(source),
            Self::MissingStartupScene
            | Self::SchemaBasisMismatch
            | Self::DocumentSourceMismatch
            | Self::AmbiguousPrefabSession { .. }
            | Self::SessionSnapshotPublication
            | Self::CompilationBudget(_)
            | Self::SourceIdentityKindConflict { .. }
            | Self::SourceChanged { .. }
            | Self::SourceTooLarge { .. }
            | Self::UnsupportedSourceEncoding { .. }
            | Self::AssetMetadataPathMismatch { .. }
            | Self::IntrinsicSourceMetadataMismatch { .. }
            | Self::CatalogueNameInvalid
            | Self::AssetMetadataChanged { .. }
            | Self::AssetDirectoryAuthorityUnavailable
            | Self::AccountingOverflow => None,
        }
    }
}
