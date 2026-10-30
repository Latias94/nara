//! Root-private admission for non-executable project authoring schema data.

mod adapter;
mod bundle;
pub(crate) mod document;
pub(crate) mod native_source;
mod project;
mod source_index;

#[cfg(any(feature = "tooling", test))]
pub(crate) use adapter::{first_party_authoring_preview_registry, project_authoring_adapters};
pub(crate) use document::{
    AdmittedAuthoringDocument, AuthoringDocumentAdmissionError, AuthoringDocumentKind,
    OwnedAuthoringDocumentRequest,
};
#[cfg(any(feature = "tooling", test))]
pub(crate) use project::{
    AuthoringAssetSource, AuthoringDocumentCapture, AuthoringPrefabSessionSnapshot,
    AuthoringPrefabSourceCapture, AuthoringProjectCandidate, AuthoringProjectReadiness,
    AuthoringProjectReceipt, AuthoringProjectSessionSnapshot, AuthoringProjectSnapshot,
    AuthoringProjectSourceAdmissionError, AuthoringSceneSessionSnapshot,
    AuthoringSchemaOwnerProjectInput, AuthoringSourceCaptureInputs, AuthoringSourceKind,
    AuthoringSourceValidation, CapturedAuthoringPreview, PreparedAuthoringProject,
    PreparedSchemaOwnerProject, PreparedSchemaOwnerSceneCapture,
    SchemaOwnerObservedProjectAttachmentFailure, SchemaOwnerProjectAttachmentFailure,
};

pub(crate) const MAX_AUTHORING_IMAGE_IDENTITIES: usize = 4_096;
pub(crate) const MAX_AUTHORING_IMAGE_IDENTITY_PATH_BYTES: usize = 2 * 1024 * 1024;
const AUTHORING_BASIS_ENCODED_MULTIPLIER: usize = 4;
pub(crate) use source_index::{
    ProjectSourceIndexBudgetUsage, ProjectSourceIndexDiagnostic, ProjectSourceIndexDiagnosticPhase,
    ProjectSourceIndexEntry, ProjectSourceIndexFailure, ProjectSourceIndexGeneration,
    ProjectSourceIndexProgress, ProjectSourceIndexSnapshot, ProjectSourceIndexStateKind,
    ProjectSourceInspectReason, ProjectSourceKind, ProjectSourceOpenCapability,
};
#[cfg(test)]
pub(crate) use source_index::{ScanTestControl, ScanTestPhase};

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    num::NonZeroU64,
    sync::Arc,
};

use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_diagnostic::DiagnosticReport;
use nara_fs::{
    ContentDigest, DirectoryCapability, FileCapability, FileIdentity, FsError, PathValidationError,
    RelativePath,
};
use nara_project::{
    DEFAULT_MANIFEST_BYTE_LIMIT, EffectiveProjectSettings, ProjectManifest, ProjectPath,
    ProjectProfileError,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleError, SchemaCompositionFingerprint,
};

use self::bundle::{AdmittedAuthoringSchemaBundle, admit_bundle};
#[cfg(any(feature = "tooling", test))]
pub(crate) use bundle::{
    AdmittedAuthoringSchemaOwner, AuthoringSchemaOwnerAttachmentFailure, AuthoringSchemaOwnerError,
    AuthoringSchemaOwnerPreparation, AuthoringSchemaOwnerReconciliation,
    ObservedAuthoringSchemaOwner, PreparedAuthoringSchemaOwner,
};

const PROJECT_MANIFEST_PATH: &str = "nara.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthoringProjectAdmissionLimits {
    unique_files: ItemLimit,
    dependency_edges: ItemLimit,
    traversal_depth: DepthLimit,
    aggregate_path_bytes: ByteLimit,
    peak_open_handles: ItemLimit,
    aggregate_encoded_bytes: ByteLimit,
    aggregate_decoded_bytes: ByteLimit,
    decode_work: ItemLimit,
    diagnostic_items: ItemLimit,
    diagnostic_bytes: ByteLimit,
    retained_candidate_bytes: ByteLimit,
}

impl AuthoringProjectAdmissionLimits {
    #[must_use]
    pub(crate) const fn new(aggregate_encoded_bytes: ByteLimit) -> Self {
        Self {
            unique_files: ItemLimit::new(4_096)
                .expect("the authoring project file limit is non-zero"),
            dependency_edges: ItemLimit::new(16_384)
                .expect("the authoring project dependency-edge limit is non-zero"),
            traversal_depth: DepthLimit::new(64)
                .expect("the authoring project traversal depth is non-zero"),
            aggregate_path_bytes: ByteLimit::new(4 * 1024 * 1024)
                .expect("the authoring project path budget is non-zero"),
            peak_open_handles: ItemLimit::new(64)
                .expect("the authoring project handle limit is non-zero"),
            aggregate_encoded_bytes,
            aggregate_decoded_bytes: ByteLimit::new(256 * 1024 * 1024)
                .expect("the authoring project decoded-byte limit is non-zero"),
            decode_work: ItemLimit::new(20_000_000)
                .expect("the authoring project decode-work limit is non-zero"),
            diagnostic_items: ItemLimit::new(4_096)
                .expect("the authoring project diagnostic limit is non-zero"),
            diagnostic_bytes: ByteLimit::new(1024 * 1024)
                .expect("the authoring project diagnostic-byte limit is non-zero"),
            retained_candidate_bytes: ByteLimit::new(256 * 1024 * 1024)
                .expect("the authoring project retained-byte limit is non-zero"),
        }
    }

    /// Sets the finite retained candidate ceiling for project admission and source capture.
    #[must_use]
    pub(crate) const fn with_retained_candidate_bytes(mut self, maximum: ByteLimit) -> Self {
        self.retained_candidate_bytes = maximum;
        self
    }

    #[must_use]
    pub(crate) const fn aggregate_encoded_bytes(self) -> ByteLimit {
        self.aggregate_encoded_bytes
    }

    #[must_use]
    pub(crate) const fn unique_files(self) -> ItemLimit {
        self.unique_files
    }

    #[must_use]
    pub(crate) const fn dependency_edges(self) -> ItemLimit {
        self.dependency_edges
    }

    #[must_use]
    pub(crate) const fn traversal_depth(self) -> DepthLimit {
        self.traversal_depth
    }

    #[must_use]
    pub(crate) const fn aggregate_path_bytes(self) -> ByteLimit {
        self.aggregate_path_bytes
    }

    #[must_use]
    pub(crate) const fn peak_open_handles(self) -> ItemLimit {
        self.peak_open_handles
    }

    #[must_use]
    pub(crate) const fn aggregate_decoded_bytes(self) -> ByteLimit {
        self.aggregate_decoded_bytes
    }

    #[must_use]
    pub(crate) const fn decode_work(self) -> ItemLimit {
        self.decode_work
    }

    #[must_use]
    pub(crate) const fn diagnostic_items(self) -> ItemLimit {
        self.diagnostic_items
    }

    #[must_use]
    pub(crate) const fn diagnostic_bytes(self) -> ByteLimit {
        self.diagnostic_bytes
    }

    #[must_use]
    pub(crate) const fn retained_candidate_bytes(self) -> ByteLimit {
        self.retained_candidate_bytes
    }
}

impl Default for AuthoringProjectAdmissionLimits {
    fn default() -> Self {
        let manifest_bytes = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT)
            .expect("the manifest byte limit fits usize on supported targets");
        let bundle_bytes = nara_reflect::AuthoringSchemaBundleLimits::default()
            .encoded_bytes()
            .get();
        let basis_bytes = manifest_bytes
            .checked_add(bundle_bytes)
            .expect("the default authoring basis byte limits fit usize");
        let aggregate_encoded_bytes = basis_bytes
            .checked_add(128 * 1024 * 1024)
            .expect("the default authoring project byte limits fit usize");
        Self::new(
            ByteLimit::new(aggregate_encoded_bytes)
                .expect("the default authoring project byte limit is non-zero"),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthoringProjectAdmissionUsage {
    manifest_encoded_bytes: usize,
    bundle_encoded_bytes: usize,
    source_encoded_bytes: usize,
    decoded_bytes: usize,
    decode_work: usize,
    unique_files: usize,
    dependency_edges: usize,
    max_traversal_depth: usize,
    path_bytes: usize,
    peak_open_handles: usize,
    diagnostic_items: usize,
    diagnostic_bytes: usize,
    retained_candidate_bytes: usize,
}

impl AuthoringProjectAdmissionUsage {
    #[must_use]
    pub(crate) const fn manifest_encoded_bytes(self) -> usize {
        self.manifest_encoded_bytes
    }

    #[must_use]
    pub(crate) const fn bundle_encoded_bytes(self) -> usize {
        self.bundle_encoded_bytes
    }

    #[must_use]
    pub(crate) const fn aggregate_encoded_bytes(self) -> usize {
        self.manifest_encoded_bytes + self.bundle_encoded_bytes + self.source_encoded_bytes
    }

    #[must_use]
    pub(crate) const fn unique_files(self) -> usize {
        self.unique_files
    }

    #[must_use]
    pub(crate) const fn dependency_edges(self) -> usize {
        self.dependency_edges
    }

    #[must_use]
    pub(crate) const fn max_traversal_depth(self) -> usize {
        self.max_traversal_depth
    }

    #[must_use]
    pub(crate) const fn path_bytes(self) -> usize {
        self.path_bytes
    }

    #[must_use]
    pub(crate) const fn peak_open_handles(self) -> usize {
        self.peak_open_handles
    }

    #[must_use]
    pub(crate) const fn decoded_bytes(self) -> usize {
        self.decoded_bytes
    }

    #[must_use]
    pub(crate) const fn decode_work(self) -> usize {
        self.decode_work
    }

    #[must_use]
    pub(crate) const fn diagnostic_items(self) -> usize {
        self.diagnostic_items
    }

    #[must_use]
    pub(crate) const fn diagnostic_bytes(self) -> usize {
        self.diagnostic_bytes
    }

    #[must_use]
    pub(crate) const fn retained_candidate_bytes(self) -> usize {
        self.retained_candidate_bytes
    }
}

fn checked_retained_replacement(
    current: usize,
    previous: usize,
    next: usize,
    maximum: usize,
) -> Result<usize, project::AuthoringProjectSourceAdmissionError> {
    let retained = current
        .checked_sub(previous)
        .and_then(|bytes| bytes.checked_add(next))
        .ok_or(project::AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
    if retained > maximum {
        return Err(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { maximum }.into(),
        );
    }
    Ok(retained)
}

#[derive(Clone, Copy)]
struct AuthoringProjectAdmissionBudget {
    limits: AuthoringProjectAdmissionLimits,
    manifest_encoded_bytes: usize,
    bundle_encoded_bytes: usize,
    source_encoded_bytes: usize,
    decoded_bytes: usize,
    decode_work: usize,
    unique_files: usize,
    dependency_edges: usize,
    max_traversal_depth: usize,
    path_bytes: usize,
    peak_open_handles: usize,
    diagnostic_items: usize,
    diagnostic_bytes: usize,
    retained_candidate_bytes: usize,
}

impl AuthoringProjectAdmissionBudget {
    fn new(limits: AuthoringProjectAdmissionLimits) -> Self {
        Self {
            limits,
            manifest_encoded_bytes: 0,
            bundle_encoded_bytes: 0,
            source_encoded_bytes: 0,
            decoded_bytes: 0,
            decode_work: 0,
            unique_files: 0,
            dependency_edges: 0,
            max_traversal_depth: 0,
            path_bytes: 0,
            peak_open_handles: 0,
            diagnostic_items: 0,
            diagnostic_bytes: 0,
            retained_candidate_bytes: 0,
        }
    }

    fn read_limit(
        &self,
        source_limit: usize,
    ) -> Result<(usize, bool), AuthoringProjectAdmissionError> {
        let remaining = self
            .limits
            .aggregate_encoded_bytes()
            .get()
            .saturating_sub(self.usage().aggregate_encoded_bytes());
        if remaining == 0 {
            return Err(
                AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded {
                    maximum: self.limits.aggregate_encoded_bytes().get(),
                },
            );
        }
        Ok((remaining.min(source_limit), remaining < source_limit))
    }

    const fn maximum(&self) -> usize {
        self.limits.aggregate_encoded_bytes().get()
    }

    fn charge_reference(
        &mut self,
        path_bytes: usize,
        traversal_depth: usize,
        dependency_edge: bool,
    ) -> Result<(), AuthoringProjectAdmissionError> {
        self.path_bytes = self.path_bytes.saturating_add(path_bytes);
        if dependency_edge {
            self.dependency_edges = self.dependency_edges.saturating_add(1);
        }
        self.max_traversal_depth = self.max_traversal_depth.max(traversal_depth);
        if traversal_depth > self.limits.traversal_depth().get() {
            return Err(AuthoringProjectAdmissionError::TraversalDepthExceeded {
                maximum: self.limits.traversal_depth().get(),
            });
        }
        self.ensure_within_limits()
    }

    fn charge_open_file(
        &mut self,
        live_handles: usize,
    ) -> Result<(), AuthoringProjectAdmissionError> {
        self.unique_files = self.unique_files.saturating_add(1);
        self.admit_handle_peak(live_handles)
    }

    fn admit_handle_peak(
        &mut self,
        live_handles: usize,
    ) -> Result<(), AuthoringProjectAdmissionError> {
        self.peak_open_handles = self.peak_open_handles.max(live_handles);
        self.ensure_within_limits()
    }

    fn charge_manifest_bytes(
        &mut self,
        bytes: usize,
    ) -> Result<(), AuthoringProjectAdmissionError> {
        self.manifest_encoded_bytes = bytes;
        self.decoded_bytes = self.decoded_bytes.saturating_add(bytes);
        self.decode_work = self.decode_work.saturating_add(bytes);
        self.retained_candidate_bytes = self.retained_candidate_bytes.saturating_add(bytes);
        self.ensure_within_limits()
    }

    fn charge_bundle_bytes(&mut self, bytes: usize) -> Result<(), AuthoringProjectAdmissionError> {
        self.bundle_encoded_bytes = bytes;
        self.decoded_bytes = self.decoded_bytes.saturating_add(bytes);
        self.decode_work = self.decode_work.saturating_add(bytes);
        self.retained_candidate_bytes = self.retained_candidate_bytes.saturating_add(bytes);
        self.ensure_within_limits()
    }

    fn charge_source_bytes(
        &mut self,
        encoded_bytes: usize,
        decoded_bytes: usize,
        decode_work: usize,
        retained_bytes: usize,
    ) -> Result<(), AuthoringProjectAdmissionError> {
        self.source_encoded_bytes = self.source_encoded_bytes.saturating_add(encoded_bytes);
        self.decoded_bytes = self.decoded_bytes.saturating_add(decoded_bytes);
        self.decode_work = self.decode_work.saturating_add(decode_work);
        self.retained_candidate_bytes =
            self.retained_candidate_bytes.saturating_add(retained_bytes);
        self.ensure_within_limits()
    }

    fn charge_diagnostics(
        &mut self,
        items: usize,
        bytes: usize,
    ) -> Result<(), AuthoringProjectAdmissionError> {
        self.diagnostic_items = self.diagnostic_items.saturating_add(items);
        self.diagnostic_bytes = self.diagnostic_bytes.saturating_add(bytes);
        self.ensure_within_limits()
    }

    fn reconcile_retained_reservation(
        &mut self,
        reserved: usize,
        actual: usize,
    ) -> Result<(), project::AuthoringProjectSourceAdmissionError> {
        let refund = reserved
            .checked_sub(actual)
            .ok_or(project::AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        self.retained_candidate_bytes = self
            .retained_candidate_bytes
            .checked_sub(refund)
            .ok_or(project::AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        Ok(())
    }

    fn ensure_within_limits(&self) -> Result<(), AuthoringProjectAdmissionError> {
        if self.usage().aggregate_encoded_bytes() > self.limits.aggregate_encoded_bytes().get() {
            return Err(
                AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded {
                    maximum: self.limits.aggregate_encoded_bytes().get(),
                },
            );
        }
        if self.unique_files > self.limits.unique_files().get() {
            return Err(AuthoringProjectAdmissionError::UniqueFileLimitExceeded {
                maximum: self.limits.unique_files().get(),
            });
        }
        if self.dependency_edges > self.limits.dependency_edges().get() {
            return Err(
                AuthoringProjectAdmissionError::DependencyEdgeLimitExceeded {
                    maximum: self.limits.dependency_edges().get(),
                },
            );
        }
        if self.path_bytes > self.limits.aggregate_path_bytes().get() {
            return Err(AuthoringProjectAdmissionError::AggregatePathBytesExceeded {
                maximum: self.limits.aggregate_path_bytes().get(),
            });
        }
        if self.peak_open_handles > self.limits.peak_open_handles().get() {
            return Err(AuthoringProjectAdmissionError::PeakOpenHandlesExceeded {
                maximum: self.limits.peak_open_handles().get(),
            });
        }
        if self.decoded_bytes > self.limits.aggregate_decoded_bytes().get() {
            return Err(
                AuthoringProjectAdmissionError::AggregateDecodedBytesExceeded {
                    maximum: self.limits.aggregate_decoded_bytes().get(),
                },
            );
        }
        if self.decode_work > self.limits.decode_work().get() {
            return Err(AuthoringProjectAdmissionError::DecodeWorkExceeded {
                maximum: self.limits.decode_work().get(),
            });
        }
        if self.diagnostic_items > self.limits.diagnostic_items().get() {
            return Err(
                AuthoringProjectAdmissionError::DiagnosticItemLimitExceeded {
                    maximum: self.limits.diagnostic_items().get(),
                },
            );
        }
        if self.diagnostic_bytes > self.limits.diagnostic_bytes().get() {
            return Err(AuthoringProjectAdmissionError::DiagnosticBytesExceeded {
                maximum: self.limits.diagnostic_bytes().get(),
            });
        }
        if self.retained_candidate_bytes > self.limits.retained_candidate_bytes().get() {
            return Err(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                    maximum: self.limits.retained_candidate_bytes().get(),
                },
            );
        }
        Ok(())
    }

    const fn usage(&self) -> AuthoringProjectAdmissionUsage {
        AuthoringProjectAdmissionUsage {
            manifest_encoded_bytes: self.manifest_encoded_bytes,
            bundle_encoded_bytes: self.bundle_encoded_bytes,
            source_encoded_bytes: self.source_encoded_bytes,
            decoded_bytes: self.decoded_bytes,
            decode_work: self.decode_work,
            unique_files: self.unique_files,
            dependency_edges: self.dependency_edges,
            max_traversal_depth: self.max_traversal_depth,
            path_bytes: self.path_bytes,
            peak_open_handles: self.peak_open_handles,
            diagnostic_items: self.diagnostic_items,
            diagnostic_bytes: self.diagnostic_bytes,
            retained_candidate_bytes: self.retained_candidate_bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct AuthoringProjectAttemptGeneration(NonZeroU64);

impl AuthoringProjectAttemptGeneration {
    #[must_use]
    pub(crate) const fn get(self) -> u64 {
        self.0.get()
    }
}

pub(crate) struct AuthoringProjectAdmissionHost {
    limits: AuthoringProjectAdmissionLimits,
    next_attempt: Option<NonZeroU64>,
}

impl Default for AuthoringProjectAdmissionHost {
    fn default() -> Self {
        Self::new(AuthoringProjectAdmissionLimits::default())
    }
}

impl AuthoringProjectAdmissionHost {
    #[must_use]
    pub(crate) const fn new(limits: AuthoringProjectAdmissionLimits) -> Self {
        Self {
            limits,
            next_attempt: Some(NonZeroU64::MIN),
        }
    }

    pub(crate) fn prepare(
        &mut self,
        project_root: DirectoryCapability,
        profile: Option<&str>,
    ) -> Result<PreparedAuthoringSchemaBasis, AuthoringProjectAdmissionError> {
        let attempt = self.allocate_attempt()?;
        let root_identity = project_root.identity();
        let mut budget = AuthoringProjectAdmissionBudget::new(self.limits);
        budget.charge_reference(PROJECT_MANIFEST_PATH.len(), 0, false)?;
        let manifest_path = RelativePath::new(PROJECT_MANIFEST_PATH)
            .map_err(AuthoringProjectAdmissionError::ManifestPath)?;
        let manifest_capability = project_root
            .open_file(&manifest_path)
            .map_err(AuthoringProjectAdmissionError::ManifestHost)?;
        budget.charge_open_file(1)?;
        if manifest_capability.parent_identity() != Some(root_identity) {
            return Err(AuthoringProjectAdmissionError::ManifestRootMismatch);
        }

        let manifest_limit = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT)
            .expect("the manifest byte limit fits usize on supported targets");
        let aggregate_maximum = budget.maximum();
        let (read_limit, aggregate_limited) = budget.read_limit(manifest_limit)?;
        let manifest_bytes = manifest_capability
            .read_to_end_bounded(u64::try_from(read_limit).unwrap_or(u64::MAX))
            .map_err(|error| {
                map_manifest_read_error(error, aggregate_limited, read_limit, aggregate_maximum)
            })?;
        budget.charge_manifest_bytes(manifest_bytes.len())?;
        let manifest_digest = ContentDigest::of_bytes(&manifest_bytes);

        let load = ProjectManifest::parse_toml_bytes(&manifest_bytes);
        if load.has_errors() || load.manifest.is_none() {
            return Err(AuthoringProjectAdmissionError::ManifestInvalid {
                diagnostics: load.diagnostics,
            });
        }
        let manifest = load
            .manifest
            .expect("a successful manifest load contains a manifest");
        let settings = manifest
            .resolve_profile(profile)
            .map_err(AuthoringProjectAdmissionError::Profile)?;
        let bundle_path = settings
            .authoring_schema_bundle
            .clone()
            .ok_or(AuthoringProjectAdmissionError::MissingSchemaBundle)?;

        Ok(PreparedAuthoringSchemaBasis {
            attempt,
            project_root,
            manifest: AdmittedAuthoringManifest {
                capability: manifest_capability,
                bytes: Arc::from(manifest_bytes),
                digest: manifest_digest,
            }
            .into(),
            settings,
            bundle_path,
            budget,
        })
    }

    fn allocate_attempt(
        &mut self,
    ) -> Result<AuthoringProjectAttemptGeneration, AuthoringProjectAdmissionError> {
        let attempt = self
            .next_attempt
            .ok_or(AuthoringProjectAdmissionError::AttemptGenerationExhausted)?;
        self.next_attempt = attempt.get().checked_add(1).and_then(NonZeroU64::new);
        Ok(AuthoringProjectAttemptGeneration(attempt))
    }
}

struct AdmittedAuthoringManifest {
    capability: FileCapability,
    bytes: Arc<[u8]>,
    digest: ContentDigest,
}

pub(crate) struct PreparedAuthoringSchemaBasis {
    attempt: AuthoringProjectAttemptGeneration,
    project_root: DirectoryCapability,
    manifest: Arc<AdmittedAuthoringManifest>,
    settings: EffectiveProjectSettings,
    bundle_path: ProjectPath,
    budget: AuthoringProjectAdmissionBudget,
}

impl PreparedAuthoringSchemaBasis {
    #[must_use]
    pub(crate) const fn bundle_path(&self) -> &ProjectPath {
        &self.bundle_path
    }

    pub(crate) fn admit_bundle(
        mut self,
    ) -> Result<AuthoringSchemaCandidate, AuthoringProjectAdmissionError> {
        let bundle = admit_bundle(&self.project_root, self.bundle_path, &mut self.budget)?;
        Ok(AuthoringSchemaCandidate {
            attempt: self.attempt,
            project_root: self.project_root,
            manifest: self.manifest,
            settings: self.settings,
            bundle,
            budget: self.budget,
        })
    }
}

pub(crate) struct AuthoringSchemaCandidate {
    attempt: AuthoringProjectAttemptGeneration,
    project_root: DirectoryCapability,
    manifest: Arc<AdmittedAuthoringManifest>,
    settings: EffectiveProjectSettings,
    bundle: AdmittedAuthoringSchemaBundle,
    budget: AuthoringProjectAdmissionBudget,
}

impl AuthoringSchemaCandidate {
    #[must_use]
    pub(crate) fn publish(self) -> AuthoringSchemaReceipt {
        AuthoringSchemaReceipt {
            attempt: self.attempt,
            project_root: self.project_root,
            manifest: self.manifest,
            settings: self.settings,
            bundle: self.bundle,
            budget: self.budget,
        }
    }
}

pub(crate) struct AuthoringSchemaReceipt {
    attempt: AuthoringProjectAttemptGeneration,
    project_root: DirectoryCapability,
    manifest: Arc<AdmittedAuthoringManifest>,
    settings: EffectiveProjectSettings,
    bundle: AdmittedAuthoringSchemaBundle,
    budget: AuthoringProjectAdmissionBudget,
}

impl AuthoringSchemaReceipt {
    /// Compares retained schema files without reopening names or reading their contents.
    pub(crate) fn source_object_in_use(&self, candidate: &FileCapability) -> Result<bool, FsError> {
        Ok(self.manifest.capability.same_object(candidate)?
            || self.bundle.capability().same_object(candidate)?)
    }

    /// Validates current schema names against the retained objects before retaining those opens.
    fn collect_reclamation_sources(&self, output: &mut Vec<FileCapability>) -> Result<(), FsError> {
        for (path, retained, digest) in [
            (
                PROJECT_MANIFEST_PATH,
                &self.manifest.capability,
                self.manifest.digest,
            ),
            (
                self.bundle.path().as_str(),
                self.bundle.capability(),
                self.bundle.digest(),
            ),
        ] {
            let file = self
                .project_root
                .open_file_exact(&RelativePath::new(path)?)?;
            if !file.same_object(retained)? || file.parent_identity() != retained.parent_identity()
            {
                return Err(FsError::Unproven {
                    operation: nara_fs::FsOperation::InspectHandle,
                    proof: "the retained schema source no longer matches its current name and parent",
                });
            }
            file.verify_digest(digest, digest.length())?;
            output.push(file);
        }
        Ok(())
    }

    #[must_use]
    pub(crate) const fn attempt_generation(&self) -> AuthoringProjectAttemptGeneration {
        self.attempt
    }

    #[must_use]
    pub(crate) fn project_root(&self) -> DirectoryCapability {
        self.project_root.share()
    }

    #[must_use]
    pub(crate) fn root_identity(&self) -> FileIdentity {
        self.project_root.identity()
    }

    #[must_use]
    pub(crate) fn manifest_identity(&self) -> FileIdentity {
        self.manifest.capability.identity()
    }

    #[must_use]
    pub(crate) fn manifest_parent_identity(&self) -> Option<FileIdentity> {
        self.manifest.capability.parent_identity()
    }

    #[must_use]
    pub(crate) fn manifest_bytes(&self) -> &[u8] {
        &self.manifest.bytes
    }

    #[must_use]
    pub(crate) fn manifest_digest(&self) -> ContentDigest {
        self.manifest.digest
    }

    #[must_use]
    pub(crate) const fn settings(&self) -> &EffectiveProjectSettings {
        &self.settings
    }

    #[must_use]
    pub(crate) const fn bundle_path(&self) -> &ProjectPath {
        self.bundle.path()
    }

    #[must_use]
    pub(crate) const fn bundle_identity(&self) -> FileIdentity {
        self.bundle.capability().identity()
    }

    #[must_use]
    pub(crate) const fn bundle_parent_identity(&self) -> Option<FileIdentity> {
        self.bundle.capability().parent_identity()
    }

    #[must_use]
    pub(crate) fn bundle_bytes(&self) -> &[u8] {
        self.bundle.bytes()
    }

    #[must_use]
    pub(crate) const fn bundle_digest(&self) -> ContentDigest {
        self.bundle.digest()
    }

    #[must_use]
    pub(crate) fn bundle(&self) -> &AuthoringSchemaBundle {
        self.bundle.bundle()
    }

    #[must_use]
    pub(crate) fn composition_fingerprint(&self) -> SchemaCompositionFingerprint {
        self.bundle.fingerprint()
    }

    #[must_use]
    pub(crate) const fn limits(&self) -> AuthoringProjectAdmissionLimits {
        self.budget.limits
    }

    #[must_use]
    pub(crate) const fn usage(&self) -> AuthoringProjectAdmissionUsage {
        self.budget.usage()
    }
}

#[derive(Debug)]
pub(crate) enum AuthoringProjectAdmissionError {
    AttemptGenerationExhausted,
    ManifestPath(PathValidationError),
    ManifestHost(FsError),
    ManifestRootMismatch,
    ManifestTooLarge { maximum: usize },
    ManifestInvalid { diagnostics: DiagnosticReport },
    Profile(ProjectProfileError),
    MissingSchemaBundle,
    BundlePath(PathValidationError),
    BundleHost(FsError),
    BundleTooLarge { maximum: usize },
    BundleInvalid(AuthoringSchemaBundleError),
    AggregateEncodedBytesExceeded { maximum: usize },
    UniqueFileLimitExceeded { maximum: usize },
    DependencyEdgeLimitExceeded { maximum: usize },
    TraversalDepthExceeded { maximum: usize },
    AggregatePathBytesExceeded { maximum: usize },
    PeakOpenHandlesExceeded { maximum: usize },
    AggregateDecodedBytesExceeded { maximum: usize },
    DecodeWorkExceeded { maximum: usize },
    DiagnosticItemLimitExceeded { maximum: usize },
    DiagnosticBytesExceeded { maximum: usize },
    RetainedCandidateBytesExceeded { maximum: usize },
}

impl Display for AuthoringProjectAdmissionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::AttemptGenerationExhausted => {
                formatter.write_str("authoring schema admission generation is exhausted")
            }
            Self::ManifestPath(_) => {
                formatter.write_str("project manifest path is invalid for host admission")
            }
            Self::ManifestHost(_) => formatter.write_str("project manifest host admission failed"),
            Self::ManifestRootMismatch => {
                formatter.write_str("project manifest handle is not bound to the project root")
            }
            Self::ManifestTooLarge { maximum } => write!(
                formatter,
                "project manifest exceeds its {maximum}-byte admission limit"
            ),
            Self::ManifestInvalid { .. } => formatter.write_str("project manifest is invalid"),
            Self::Profile(_) => formatter.write_str("project profile could not be resolved"),
            Self::MissingSchemaBundle => {
                formatter.write_str("project manifest does not declare an authoring schema bundle")
            }
            Self::BundlePath(_) => {
                formatter.write_str("authoring schema bundle path is invalid for host admission")
            }
            Self::BundleHost(_) => {
                formatter.write_str("authoring schema bundle host admission failed")
            }
            Self::BundleTooLarge { maximum } => write!(
                formatter,
                "authoring schema bundle exceeds its {maximum}-byte admission limit"
            ),
            Self::BundleInvalid(_) => formatter.write_str("authoring schema bundle is invalid"),
            Self::AggregateEncodedBytesExceeded { maximum } => write!(
                formatter,
                "authoring schema admission exceeds its aggregate {maximum}-byte limit"
            ),
            Self::UniqueFileLimitExceeded { maximum } => write!(
                formatter,
                "authoring project admission exceeds its {maximum}-file limit"
            ),
            Self::DependencyEdgeLimitExceeded { maximum } => write!(
                formatter,
                "authoring project admission exceeds its {maximum}-dependency-edge limit"
            ),
            Self::TraversalDepthExceeded { maximum } => write!(
                formatter,
                "authoring project admission exceeds its {maximum}-level traversal depth"
            ),
            Self::AggregatePathBytesExceeded { maximum } => write!(
                formatter,
                "authoring project admission exceeds its aggregate {maximum}-byte path limit"
            ),
            Self::PeakOpenHandlesExceeded { maximum } => write!(
                formatter,
                "authoring project admission exceeds its {maximum}-handle limit"
            ),
            Self::AggregateDecodedBytesExceeded { maximum } => write!(
                formatter,
                "authoring project admission exceeds its aggregate {maximum}-byte decoded limit"
            ),
            Self::DecodeWorkExceeded { maximum } => write!(
                formatter,
                "authoring project admission exceeds its {maximum}-unit decode-work limit"
            ),
            Self::DiagnosticItemLimitExceeded { maximum } => write!(
                formatter,
                "authoring project admission exceeds its {maximum}-diagnostic limit"
            ),
            Self::DiagnosticBytesExceeded { maximum } => write!(
                formatter,
                "authoring project admission exceeds its aggregate {maximum}-byte diagnostic limit"
            ),
            Self::RetainedCandidateBytesExceeded { maximum } => write!(
                formatter,
                "authoring project admission exceeds its {maximum}-byte retained-candidate limit"
            ),
        }
    }
}

impl Error for AuthoringProjectAdmissionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ManifestPath(error) | Self::BundlePath(error) => Some(error),
            Self::ManifestHost(error) | Self::BundleHost(error) => Some(error),
            Self::Profile(error) => Some(error),
            Self::BundleInvalid(error) => Some(error),
            Self::AttemptGenerationExhausted
            | Self::ManifestRootMismatch
            | Self::ManifestTooLarge { .. }
            | Self::ManifestInvalid { .. }
            | Self::MissingSchemaBundle
            | Self::BundleTooLarge { .. }
            | Self::AggregateEncodedBytesExceeded { .. }
            | Self::UniqueFileLimitExceeded { .. }
            | Self::DependencyEdgeLimitExceeded { .. }
            | Self::TraversalDepthExceeded { .. }
            | Self::AggregatePathBytesExceeded { .. }
            | Self::PeakOpenHandlesExceeded { .. }
            | Self::AggregateDecodedBytesExceeded { .. }
            | Self::DecodeWorkExceeded { .. }
            | Self::DiagnosticItemLimitExceeded { .. }
            | Self::DiagnosticBytesExceeded { .. }
            | Self::RetainedCandidateBytesExceeded { .. } => None,
        }
    }
}

fn map_manifest_read_error(
    error: FsError,
    aggregate_limited: bool,
    read_limit: usize,
    aggregate_maximum: usize,
) -> AuthoringProjectAdmissionError {
    match error {
        FsError::ByteLimitExceeded { .. } if aggregate_limited => {
            AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded {
                maximum: aggregate_maximum,
            }
        }
        FsError::ByteLimitExceeded { .. } => AuthoringProjectAdmissionError::ManifestTooLarge {
            maximum: read_limit,
        },
        error => AuthoringProjectAdmissionError::ManifestHost(error),
    }
}

#[cfg(test)]
mod tests {
    mod actual_demand;
    pub(in crate::project_host::authoring) mod independent_capture;

    use std::{
        fs::{self, File},
        io,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    #[cfg(windows)]
    use std::fs::OpenOptions;

    use nara_asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, AssetSourcePath,
        StableAssetId,
    };
    use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
    use nara_reflect::{
        AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId, ComponentFieldPath,
        ComponentFieldSchema, ComponentPropertyPath, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaOwnerId, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
    };
    use nara_scene::{
        AuthoringOverlayPathSegment, AuthoringOverlayRecordSlot, AuthoringRecordLocation,
        PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode, PrefabDocument,
        PrefabEntityTarget, PrefabInstance, PrefabInstanceClaim, PrefabOverlay,
        PrefabPropertyClaim, PrefabPropertyIntent, SceneComponentRecord, SceneDocument,
        SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
        SourceDocumentKey, SourceDocumentKind,
    };

    use super::*;

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    const BUNDLE_PATH: &str = "schemas/authoring.bundle.json";
    const BUILTIN_OWNER: ComponentSchemaOwnerId =
        ComponentSchemaOwnerId::new("nara.ui.components.native");

    fn inventory_overlay(name: &str) -> PrefabOverlay {
        PrefabOverlay::new(SourceDocumentKey::project_location(
            "bb46c078-85b1-5823-8b1d-647b3b9e1f22".parse().unwrap(),
            AssetSourcePath::new(format!("prefabs/{name}.prefab.json")).unwrap(),
            SourceDocumentKind::Prefab,
        ))
    }

    #[test]
    fn reclamation_schema_query_tracks_retained_objects_after_equal_byte_replacement() {
        let project = TestAuthoringProject::new(
            "reclamation-schema",
            schema_bundle("nara.test.authoring", "nara.test.Transform"),
        );
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let original = fs::read(project.root.join(BUNDLE_PATH)).unwrap();
        let moved_path = "schemas/retained.bundle.json";
        fs::rename(
            project.root.join(BUNDLE_PATH),
            project.root.join(moved_path),
        )
        .unwrap();
        fs::write(project.root.join(BUNDLE_PATH), original).unwrap();

        // Independent Host sessions must compare actual objects, not scoped FileIdentity values.
        let root = project.capability();
        let moved = root
            .open_file_exact(&RelativePath::new(moved_path).unwrap())
            .unwrap();
        let replacement = root
            .open_file_exact(&RelativePath::new(BUNDLE_PATH).unwrap())
            .unwrap();
        assert!(schema.source_object_in_use(&moved).unwrap());
        assert!(!schema.source_object_in_use(&replacement).unwrap());
        assert!(schema.collect_reclamation_sources(&mut Vec::new()).is_err());
    }

    #[test]
    fn reclamation_source_collection_retains_validated_objects_and_refuses_drift() {
        let project = TestAuthoringProject::new(
            "reclamation-sources",
            schema_bundle("nara.test.authoring", "nara.test.Transform"),
        );
        project.write_startup_manifest();
        for directory in ["assets", "scenes", "prefabs"] {
            fs::create_dir_all(project.root.join(directory)).unwrap();
        }
        let path = "scenes/startup.scene.json";
        let bytes = SceneDocument::default().to_json_string().unwrap();
        fs::write(project.root.join(path), &bytes).unwrap();
        let schema = AuthoringProjectAdmissionHost::default()
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let snapshot = PreparedAuthoringProject::new(schema, Box::new([]))
            .admit_startup_closure()
            .unwrap()
            .publish()
            .into_session_snapshot()
            .unwrap();
        let validation = snapshot.startup_source_validation();
        fs::write(project.root.join(path), format!("{bytes} ")).unwrap();
        assert!(
            snapshot
                .collect_reclamation_sources(&mut Vec::new())
                .is_err()
        );
        assert!(
            validation
                .collect_reclamation_sources(&mut Vec::new())
                .is_err()
        );
        fs::write(project.root.join(path), &bytes).unwrap();
        let mut sources = Vec::new();
        snapshot.collect_reclamation_sources(&mut sources).unwrap();
        validation
            .collect_reclamation_sources(&mut Vec::new())
            .unwrap();

        let moved_path = "scenes/retained.scene.json";
        fs::rename(project.root.join(path), project.root.join(moved_path)).unwrap();
        let moved = project
            .capability()
            .open_file_exact(&RelativePath::new(moved_path).unwrap())
            .unwrap();
        assert!(
            sources
                .iter()
                .any(|source| source.same_object(&moved).unwrap())
        );
        assert!(
            snapshot
                .collect_reclamation_sources(&mut Vec::new())
                .is_err()
        );
        assert!(
            validation
                .collect_reclamation_sources(&mut Vec::new())
                .is_err()
        );

        fs::write(project.root.join(path), &bytes).unwrap();
        assert!(
            snapshot
                .collect_reclamation_sources(&mut Vec::new())
                .is_err()
        );
        assert!(
            validation
                .collect_reclamation_sources(&mut Vec::new())
                .is_err()
        );
    }

    #[test]
    fn exact_manifest_and_bundle_handles_publish_one_schema_receipt() {
        let project = TestAuthoringProject::new(
            "exact-receipt",
            schema_bundle("nara.test.authoring", "nara.test.Transform"),
        );
        let project_root = project.capability();
        let expected_root_identity = project_root.identity();
        let expected_manifest = project_root
            .open_file(&RelativePath::new(PROJECT_MANIFEST_PATH).unwrap())
            .unwrap();
        let expected_bundle = project_root
            .open_file(&RelativePath::new(BUNDLE_PATH).unwrap())
            .unwrap();
        let manifest_bytes = fs::read(project.root.join(PROJECT_MANIFEST_PATH)).unwrap();
        let bundle_bytes = fs::read(project.root.join(BUNDLE_PATH)).unwrap();

        let mut host = AuthoringProjectAdmissionHost::default();
        let receipt = host
            .prepare(project_root, None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();

        assert_eq!(receipt.attempt_generation().get(), 1);
        assert_eq!(receipt.root_identity(), expected_root_identity);
        assert_eq!(receipt.project_root().identity(), expected_root_identity);
        assert_eq!(receipt.manifest_identity(), expected_manifest.identity());
        assert_eq!(
            receipt.manifest_parent_identity(),
            expected_manifest.parent_identity()
        );
        assert_eq!(receipt.bundle_identity(), expected_bundle.identity());
        assert_eq!(
            receipt.bundle_parent_identity(),
            expected_bundle.parent_identity()
        );
        assert_eq!(receipt.manifest_bytes(), manifest_bytes);
        assert_eq!(receipt.bundle_bytes(), bundle_bytes);
        assert_eq!(
            receipt.manifest_digest(),
            ContentDigest::of_bytes(&manifest_bytes)
        );
        assert_eq!(
            receipt.bundle_digest(),
            ContentDigest::of_bytes(&bundle_bytes)
        );
        assert_eq!(receipt.bundle_path().as_str(), BUNDLE_PATH);
        assert_eq!(
            receipt.composition_fingerprint(),
            receipt.bundle().fingerprint()
        );
        assert_eq!(
            receipt.usage().aggregate_encoded_bytes(),
            manifest_bytes.len() + bundle_bytes.len()
        );
        assert_eq!(
            receipt.usage().manifest_encoded_bytes(),
            manifest_bytes.len()
        );
        assert_eq!(receipt.usage().bundle_encoded_bytes(), bundle_bytes.len());
        assert_eq!(receipt.settings().project.name, "exact-receipt");

        let second = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        assert_eq!(second.attempt_generation().get(), 2);
    }

    #[test]
    fn aggregate_manifest_and_bundle_budget_rejects_before_publication() {
        let project = TestAuthoringProject::new(
            "aggregate-budget",
            schema_bundle("nara.test.authoring", "nara.test.Transform"),
        );
        let manifest_bytes = fs::read(project.root.join(PROJECT_MANIFEST_PATH)).unwrap();
        let bundle_bytes = fs::read(project.root.join(BUNDLE_PATH)).unwrap();
        let aggregate_limit = manifest_bytes.len() + bundle_bytes.len() - 1;
        let limits = AuthoringProjectAdmissionLimits::new(
            ByteLimit::new(aggregate_limit).expect("the test aggregate limit is non-zero"),
        );
        let mut host = AuthoringProjectAdmissionHost::new(limits);

        let prepared = host.prepare(project.capability(), None).unwrap();
        let error = match prepared.admit_bundle() {
            Ok(_) => panic!("aggregate budget rejection must not publish a candidate"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded { maximum }
                if maximum == aggregate_limit
        ));
    }

    #[test]
    fn spoofed_builtin_owner_does_not_match_the_trusted_adapter_basis() {
        let spoofed = schema_bundle(BUILTIN_OWNER.as_str(), "nara.test.SpoofedUiNode");
        let project = TestAuthoringProject::new("spoofed-owner", spoofed);
        let trusted_catalog = component_catalog("nara.ui.UiNode");
        let mut host = AuthoringProjectAdmissionHost::default();
        let receipt = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();

        let admitted_owner = &receipt.bundle().owners()[0];
        assert_eq!(
            admitted_owner.owner_id().matches_executable(BUILTIN_OWNER),
            Ok(true)
        );
        assert_eq!(admitted_owner.current().generation(), 1);
        assert_ne!(
            admitted_owner.current().fingerprint(),
            trusted_catalog.fingerprint()
        );
    }

    #[test]
    fn project_receipt_retains_declared_asset_metadata_and_source_bytes() {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ImageSettingsFixture {
            color_space: ImageColorSpaceFixture,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum ImageColorSpaceFixture {
            Linear,
        }

        let owner = PersistentComponentSchemaOwnerId::try_new("nara.test.asset-owner").unwrap();
        let component = ComponentTypeId::new("nara.test.ImageLink");
        let bundle = asset_schema_bundle(owner.clone(), component.clone());
        let project = TestAuthoringProject::new("asset-closure", bundle);
        project.write_startup_manifest();

        let asset_path = AssetPath::new("textures/icon.png").unwrap();
        let source_bytes = b"authoring asset source";
        fs::create_dir_all(project.root.join("scenes")).unwrap();
        fs::create_dir_all(project.root.join("assets/textures")).unwrap();
        let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("root").unwrap(),
        )
        .with_component(
            component,
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "image",
                    ComponentValue::map([
                        ("kind", ComponentValue::String("path".to_owned())),
                        (
                            "value",
                            ComponentValue::String(asset_path.as_str().to_owned()),
                        ),
                    ]),
                )]),
            )
            .with_owner(owner),
        )]);
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            scene.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(project.root.join("assets/textures/icon.png"), source_bytes).unwrap();
        let meta = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap(),
                asset_path.clone(),
                AssetSourceKind::Image,
            ),
            ImageSettingsFixture {
                color_space: ImageColorSpaceFixture::Linear,
            },
        );
        fs::write(
            project.root.join("assets/textures/icon.png.meta"),
            meta.to_json_string().unwrap(),
        )
        .unwrap();

        let mut host = AuthoringProjectAdmissionHost::default();
        let schema = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let receipt = PreparedAuthoringProject::new(schema, Box::new([]))
            .admit_startup_closure()
            .unwrap()
            .publish();
        let snapshot = receipt.snapshot();

        assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Degraded);
        assert!(snapshot.assets().is_empty());
        assert_eq!(
            snapshot.startup_scene().record_facts()[0]
                .schema_declared_asset_references()
                .len(),
            1
        );
        assert_eq!(snapshot.source_facts().len(), 1);
        drop(snapshot);
        drop(receipt);
        // Inventory alone does not demand assets. Admit a data Adapter for effective capture.
        let schema = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_schema_owner(
            &schema.bundle().owners()[0],
            &Default::default(),
        )
        .unwrap();
        let receipt = PreparedAuthoringProject::new(schema, vec![adapter].into_boxed_slice())
            .admit_startup_closure()
            .unwrap()
            .publish();
        let snapshot = receipt.snapshot();
        assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Complete);
        assert!(!snapshot.startup_compilation_diagnostics().has_errors());
        assert_eq!(snapshot.assets().len(), 1);
        let asset = &snapshot.assets()[0];
        assert_eq!(asset.path().as_str(), format!("assets/{asset_path}"));
        assert_eq!(asset.record().source_kind(), &AssetSourceKind::Image);
        assert_eq!(asset.bytes(), source_bytes);
        assert_eq!(
            asset
                .metadata()
                .admit::<ImageSettingsFixture>()
                .unwrap()
                .settings(),
            meta.settings()
        );
        #[cfg(feature = "runtime-2d")]
        assert_eq!(
            asset
                .metadata()
                .admit::<nara_image::ImageImportSettings>()
                .unwrap()
                .settings()
                .color_space(),
            nara_image::ImageColorSpace::Linear
        );
        assert_eq!(
            asset.metadata_source().kind(),
            AuthoringSourceKind::AssetMetadata
        );
        assert_eq!(asset.source().kind(), AuthoringSourceKind::AssetSource);
        assert_eq!(
            snapshot.startup_scene().record_facts()[0].schema_declared_asset_references()[0]
                .asset_ref(),
            &AssetRef::Path(asset_path)
        );
        assert_eq!(
            snapshot
                .source_facts()
                .map(|source| source.kind())
                .collect::<Vec<_>>(),
            vec![
                AuthoringSourceKind::AssetSource,
                AuthoringSourceKind::AssetMetadata,
                AuthoringSourceKind::Scene,
            ]
        );
    }

    #[test]
    fn stable_asset_reference_is_preserved_as_a_degraded_dependency_fact() {
        let owner = PersistentComponentSchemaOwnerId::try_new("nara.test.stable-owner").unwrap();
        let component = ComponentTypeId::new("nara.test.StableImageLink");
        let bundle = asset_schema_bundle(owner.clone(), component.clone());
        let project = TestAuthoringProject::new("stable-asset-closure", bundle);
        project.write_startup_manifest();
        fs::create_dir_all(project.root.join("scenes")).unwrap();
        let stable_id = StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap();
        let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("root").unwrap(),
        )
        .with_component(
            component,
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "image",
                    ComponentValue::map([
                        ("kind", ComponentValue::String("stable_id".to_owned())),
                        ("value", ComponentValue::String(stable_id.to_string())),
                    ]),
                )]),
            )
            .with_owner(owner),
        )]);
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            scene.to_json_string().unwrap(),
        )
        .unwrap();

        let mut host = AuthoringProjectAdmissionHost::default();
        let schema = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_schema_owner(
            &schema.bundle().owners()[0],
            &Default::default(),
        )
        .unwrap();
        let receipt = PreparedAuthoringProject::new(schema, vec![adapter].into_boxed_slice())
            .admit_startup_closure()
            .unwrap()
            .publish();
        let snapshot = receipt.snapshot();

        assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Degraded);
        assert_eq!(snapshot.unresolved_dependencies().len(), 1);
        let dependency = &snapshot.unresolved_dependencies()[0];
        assert_eq!(
            dependency.kind(),
            project::AuthoringUnresolvedDependencyKind::Asset
        );
        assert_eq!(dependency.reference(), &AssetRef::StableId(stable_id));

        drop(snapshot);
        drop(receipt);
        fs::create_dir_all(project.root.join("assets/textures")).unwrap();
        let source = b"stable authoring image bytes";
        let admit = || {
            let mut host = AuthoringProjectAdmissionHost::default();
            let schema = host
                .prepare(project.capability(), None)
                .unwrap()
                .admit_bundle()
                .unwrap()
                .publish();
            let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_schema_owner(
                &schema.bundle().owners()[0],
                &Default::default(),
            )
            .unwrap();
            PreparedAuthoringProject::new(schema, vec![adapter].into_boxed_slice())
                .admit_startup_closure()
                .unwrap()
                .publish()
        };
        for (previous, path) in [
            (None, "textures/original.png"),
            (Some("textures/original.png"), "textures/renamed.png"),
        ] {
            if let Some(previous) = previous {
                fs::rename(
                    project.root.join("assets").join(previous),
                    project.root.join("assets").join(path),
                )
                .unwrap();
                fs::remove_file(project.root.join("assets").join(format!("{previous}.meta")))
                    .unwrap();
            } else {
                fs::write(project.root.join("assets").join(path), source).unwrap();
            }
            fs::write(
                project.root.join("assets").join(format!("{path}.meta")),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        stable_id,
                        AssetPath::new(path).unwrap(),
                        AssetSourceKind::Image,
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
            let loaded = admit();
            let snapshot = loaded.snapshot();
            assert!(snapshot.unresolved_dependencies().is_empty());
            assert_eq!(
                snapshot.assets()[0].path().as_str(),
                format!("assets/{path}")
            );
            assert_eq!(snapshot.assets()[0].bytes(), source);
            assert_eq!(snapshot.startup_scene().document(), &scene);
            assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Complete);
            let preview = nara_scene::__private::project_published_authoring_scene_preview(
                snapshot.startup_scene(),
            );
            assert_eq!(
                preview.omissions()[0].reason(),
                nara_scene::__private::AuthoringPreviewOmissionReason::NativeBindingUnavailable
            );
        }
    }

    #[test]
    fn path_prefab_native_admission_enforces_the_simultaneous_handle_peak() {
        for stable in [false, true] {
            let project = TestAuthoringProject::new(
                "native-handle-peak",
                AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>())
                    .unwrap(),
            );
            project.write_startup_manifest();
            if stable {
                let path = project.root.join(PROJECT_MANIFEST_PATH);
                let manifest = fs::read_to_string(&path).unwrap();
                fs::write(
                    path,
                    manifest.replace("'project_location'", "'stable_metadata'"),
                )
                .unwrap();
            }
            for directory in ["scenes", "prefabs"] {
                fs::create_dir_all(project.root.join(directory)).unwrap();
            }
            let mut anchor = SceneEntityRecord::new(SceneEntityId::new("instance").unwrap());
            anchor.prefab = Some(PrefabInstance {
                source: AssetRef::Path(AssetPath::new("p.prefab.json").unwrap()),
                overrides: None,
            });
            fs::write(
                project.root.join("scenes/startup.scene.json"),
                SceneDocument::from_ordered_roots([anchor])
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
            fs::write(
                project.root.join("prefabs/p.prefab.json"),
                PrefabDocument::default().to_json_string().unwrap(),
            )
            .unwrap();
            if stable {
                for (root, path, kind, id) in [
                    (
                        "scenes",
                        "startup.scene.json",
                        AssetSourceKind::Scene,
                        "9899ce12-26f0-4427-9c2d-ef42bb558f1b",
                    ),
                    (
                        "prefabs",
                        "p.prefab.json",
                        AssetSourceKind::Prefab,
                        "9899ce12-26f0-4427-9c2d-ef42bb558f1c",
                    ),
                ] {
                    fs::write(
                        project.root.join(root).join(format!("{path}.meta")),
                        AssetMeta::new(
                            AssetMetaIdentity::new(
                                StableAssetId::parse_str(id).unwrap(),
                                AssetPath::new(path).unwrap(),
                                kind,
                            ),
                            (),
                        )
                        .to_json_string()
                        .unwrap(),
                    )
                    .unwrap();
                }
            }
            // Three schema handles, the Prefab directory and original source, plus exact-open
            // traversal. Stable metadata keeps the second source handle alive during its open.
            let expected_peak = 7 + usize::from(cfg!(target_os = "linux")) + usize::from(stable);
            for maximum in [7, expected_peak] {
                let limits = AuthoringProjectAdmissionLimits {
                    peak_open_handles: ItemLimit::new(maximum).unwrap(),
                    ..AuthoringProjectAdmissionLimits::default()
                };
                let schema = AuthoringProjectAdmissionHost::new(limits)
                    .prepare(project.capability(), None)
                    .unwrap()
                    .admit_bundle()
                    .unwrap()
                    .publish();
                let result =
                    PreparedAuthoringProject::new(schema, Box::new([])).admit_startup_closure();
                if maximum < expected_peak {
                    assert!(matches!(
                        result,
                        Err(AuthoringProjectSourceAdmissionError::Budget(
                            AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { maximum: 7 }
                        ))
                    ));
                } else {
                    let receipt = result.unwrap().publish();
                    assert_eq!(
                        receipt.snapshot().usage().peak_open_handles(),
                        expected_peak
                    );
                    assert_eq!(receipt.snapshot().prefabs().len(), 1);
                    let snapshot = receipt.snapshot();
                    assert_eq!(snapshot.source_facts().len(), if stable { 4 } else { 2 });
                    if stable {
                        let metadata = snapshot.prefabs()[0]
                            .native_source()
                            .metadata_source()
                            .unwrap();
                        let fact = snapshot
                            .source_facts()
                            .find(|fact| fact.project_path() == metadata.project_path())
                            .unwrap();
                        assert!(std::ptr::eq(metadata.as_ref(), fact));
                    }
                    assert_eq!(
                        snapshot
                            .source_facts()
                            .filter(|fact| fact.kind() == AuthoringSourceKind::AssetMetadata)
                            .map(|fact| fact.project_path().as_str())
                            .collect::<Vec<_>>(),
                        if stable {
                            vec![
                                "prefabs/p.prefab.json.meta",
                                "scenes/startup.scene.json.meta",
                            ]
                        } else {
                            Vec::new()
                        }
                    );
                    let usage = snapshot.usage();
                    assert_eq!(
                        usage.aggregate_encoded_bytes(),
                        usage.manifest_encoded_bytes()
                            + usage.bundle_encoded_bytes()
                            + snapshot
                                .source_facts()
                                .map(|fact| fact.encoded_bytes())
                                .sum::<usize>()
                    );
                }
            }
        }
    }

    #[test]
    fn declarative_instance_inventory_survives_rejected_activation_without_capturing_raw_pins() {
        use nara_scene::{AuthoringDocumentSource, SceneAuthoringSession, SceneDocumentCandidate};

        let owner = PersistentComponentSchemaOwnerId::try_new("nara.test.snapshot-owner").unwrap();
        let component = ComponentTypeId::new("nara.test.SnapshotImageLink");
        let project = TestAuthoringProject::new(
            "instance-snapshot-closure",
            asset_schema_bundle(owner.clone(), component.clone()),
        );
        project.write_startup_manifest();
        for directory in ["scenes", "prefabs", "assets"] {
            fs::create_dir_all(project.root.join(directory)).unwrap();
        }
        let asset_record = |name: &str| {
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "image",
                    ComponentValue::map([
                        ("kind", ComponentValue::String("path".to_owned())),
                        ("value", ComponentValue::String(format!("{name}.png"))),
                    ]),
                )]),
            )
            .with_owner(owner.clone())
        };
        let asset_overlay = |name: &str| {
            let mut overlay = inventory_overlay(name);
            overlay.components.push(PrefabComponentIntent::Set {
                mode: PrefabComponentSetMode::Add,
                target: PrefabEntityTarget::source(SceneEntityId::new("same-target").unwrap()),
                component: component.clone(),
                record: asset_record(name),
                bindings: Vec::new(),
            });
            overlay
        };
        let declaration = |name: &str, overrides| PrefabInstance {
            source: AssetRef::path(format!("{name}.prefab.json")).unwrap(),
            overrides,
        };
        let mut pinned = asset_overlay("pinned");
        pinned.instances.push(PrefabInstanceClaim {
            target: PrefabEntityTarget::source(SceneEntityId::new("nested-target").unwrap()),
            value: Some(declaration("nested", Some(asset_overlay("nested")))),
        });
        let mut added = PrefabAddition::new(SceneEntityId::new("added").unwrap());
        added
            .components
            .insert(component.clone(), asset_record("added"));
        added.prefab = Some(declaration("added", None));
        let mut outer = inventory_overlay("outer");
        outer.additions.push(added);
        outer.instances.push(PrefabInstanceClaim {
            target: PrefabEntityTarget::source(SceneEntityId::new("same-target").unwrap()),
            value: Some(declaration("pinned", Some(pinned))),
        });
        let original = declaration("history-only", Some(asset_overlay("history-only")));
        let selected = declaration("outer", Some(outer));
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        anchor.prefab = Some(original.clone());
        let original_scene = SceneDocument::from_ordered_roots([anchor]);
        let published = SceneDocumentCandidate::decode_json_bytes(
            original_scene.to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::path("startup.scene.json").unwrap()),
            &asset_schema_bundle(owner.clone(), component.clone()),
            &[],
        )
        .unwrap()
        .publish();
        let mut session = SceneAuthoringSession::from_published_authoring_document(published);
        let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
            entity: SceneEntityId::new("anchor").unwrap(),
            expected: Some(original.clone()),
            value: Some(selected.clone()),
        }]);
        let prepared = session
            .prepare_authoring_edit(session.revision(), &patch)
            .unwrap();
        assert_eq!(
            prepared.inverse(),
            &ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
                entity: SceneEntityId::new("anchor").unwrap(),
                expected: Some(selected),
                value: Some(original),
            }]),
        );
        assert!(session.commit_authoring_edit(prepared).applied);
        let scene = session.document().clone();
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            scene.to_json_string().unwrap(),
        )
        .unwrap();
        for name in ["outer", "added", "pinned", "nested"] {
            fs::write(
                project.root.join(format!("prefabs/{name}.prefab.json")),
                PrefabDocument::default().to_json_string().unwrap(),
            )
            .unwrap();
        }
        for (index, name) in ["added", "pinned", "nested"].iter().enumerate() {
            fs::write(
                project.root.join(format!("assets/{name}.png")),
                name.as_bytes(),
            )
            .unwrap();
            fs::write(
                project.root.join(format!("assets/{name}.png.meta")),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        StableAssetId::parse_str(&format!("2f0d71c7-14fc-4ed4-b48b-{index:012x}"))
                            .unwrap(),
                        AssetPath::new(format!("{name}.png")).unwrap(),
                        AssetSourceKind::Image,
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
        }
        let mut host = AuthoringProjectAdmissionHost::default();
        let schema = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let receipt = PreparedAuthoringProject::new(schema, Box::new([]))
            .admit_startup_closure()
            .unwrap()
            .publish();
        let snapshot = receipt.snapshot();
        assert_eq!(snapshot.startup_scene().document(), &scene);
        assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Degraded);
        // Structural target closure rejects the nonexistent claim before deferred additions.
        // Rejected compilation has no effective asset closure, but keeps every source claim.
        assert_eq!(
            snapshot
                .prefabs()
                .iter()
                .map(|prefab| prefab.path().as_str())
                .collect::<Vec<_>>(),
            ["outer.prefab.json"]
        );
        assert!(snapshot.assets().is_empty());
        assert!(snapshot.unresolved_dependencies().is_empty());
        assert!(
            snapshot
                .startup_compilation_diagnostics()
                .iter()
                .any(|diagnostic| {
                    diagnostic.code().as_str() == "scene.prefab-compilation-failed"
                })
        );
        assert_eq!(
            snapshot
                .source_facts()
                .map(|fact| fact.project_path().as_str())
                .collect::<Vec<_>>(),
            ["prefabs/outer.prefab.json", "scenes/startup.scene.json",]
        );
        let facts = snapshot.startup_scene().record_facts();
        assert_eq!(facts.len(), 3);
        for (name, depth) in [("added", 1), ("pinned", 2), ("nested", 3)] {
            let reference = AssetRef::path(format!("{name}.png")).unwrap();
            let fact = facts
                .iter()
                .find(|fact| fact.schema_declared_asset_references()[0].asset_ref() == &reference)
                .unwrap();
            let AuthoringRecordLocation::OverlayComponent {
                anchor,
                overlays,
                slot,
                target,
            } = fact.location()
            else {
                panic!("inventory must retain the actual declarative record location");
            };
            assert_eq!(anchor.as_str(), "anchor");
            assert_eq!(overlays.len(), depth);
            assert!(matches!(
                overlays[0],
                AuthoringOverlayPathSegment::Root { .. }
            ));
            assert!(overlays[1..].iter().all(|segment| matches!(
                segment,
                AuthoringOverlayPathSegment::InstanceClaim { index: 0, .. }
            )));
            if name == "added" {
                assert_eq!(slot, &AuthoringOverlayRecordSlot::Addition { index: 0 });
                assert_eq!(
                    target,
                    &PrefabEntityTarget::addition(SceneEntityId::new("added").unwrap()),
                );
            } else {
                assert_eq!(slot, &AuthoringOverlayRecordSlot::Component { index: 0 });
                assert_eq!(
                    target,
                    &PrefabEntityTarget::source(SceneEntityId::new("same-target").unwrap()),
                );
            }
        }
    }

    #[test]
    fn property_overlay_assets_capture_actual_sources_without_complete_record_facts() {
        let owner = PersistentComponentSchemaOwnerId::try_new("nara.test.field-assets").unwrap();
        let component = ComponentTypeId::new("nara.test.FieldAssets");
        let reference_value =
            |reference: AssetRef| nara_reflect::encode_asset_reference_value(&reference);
        let path_value =
            |name: &str| reference_value(AssetRef::path(format!("{name}.png")).unwrap());
        let schema =
            ComponentSchema::new(component.clone(), "Field assets", ComponentSchemaVersion(2))
                .with_capabilities([ComponentCapability::Scene])
                .with_fields([
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("image"),
                        "Image",
                        ComponentFieldPath::from_fields(["settings", "image"]),
                        ComponentValueShape::AssetRef,
                    )
                    .with_capability(ComponentCapability::AssetRef),
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("gallery"),
                        "Gallery",
                        ComponentFieldPath::from_fields(["gallery"]),
                        ComponentValueShape::list(ComponentValueShape::AssetRef),
                    )
                    .with_capability(ComponentCapability::AssetRef),
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("palette"),
                        "Palette",
                        ComponentFieldPath::from_fields(["palette"]),
                        ComponentValueShape::map(ComponentValueShape::AssetRef),
                    )
                    .with_capability(ComponentCapability::AssetRef),
                    ComponentFieldSchema::optional_with_default(
                        ComponentFieldId::new("fallback"),
                        "Fallback",
                        ComponentFieldPath::from_fields(["defaults", "image"]),
                        ComponentValueShape::AssetRef,
                        path_value("default"),
                    )
                    .with_capability(ComponentCapability::AssetRef),
                ]);
        let project = TestAuthoringProject::new(
            "field-asset-capture",
            AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
                owner,
                ComponentSchemaCatalog {
                    components: vec![schema],
                    ..ComponentSchemaCatalog::default()
                },
                None,
            )])
            .unwrap(),
        );
        project.write_startup_manifest();
        for directory in ["scenes", "prefabs", "assets"] {
            fs::create_dir_all(project.root.join(directory)).unwrap();
        }
        let target = SceneEntityId::new("not-a-complete-local-record").unwrap();
        let set = |field: &str, value| PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(target.clone()),
            component: component.clone(),
            version: ComponentSchemaVersion(2),
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new(field)),
                intent: PrefabPropertyIntent::Set(value),
                bindings: Vec::new(),
            }],
        };
        let remove_default = || PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(target.clone()),
            component: component.clone(),
            version: ComponentSchemaVersion(2),
            claims: vec![PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("fallback")),
                intent: PrefabPropertyIntent::Absent,
                bindings: Vec::new(),
            }],
        };
        let instance = |name: &str, components, instances| {
            let mut overlay = inventory_overlay(name);
            overlay.components = components;
            overlay.instances = instances;
            PrefabInstance {
                source: AssetRef::path(format!("{name}.prefab.json")).unwrap(),
                overrides: Some(overlay),
            }
        };
        let pin = |name: &str, value| PrefabInstanceClaim {
            target: PrefabEntityTarget::source(SceneEntityId::new(name).unwrap()),
            value: Some(value),
        };
        let missing = StableAssetId::parse_str("cb39569e-e9c8-4a10-b527-6c21f3d271f0").unwrap();
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
        anchor.prefab = Some(instance(
            "outer",
            vec![
                set("image", path_value("direct")),
                set(
                    "gallery",
                    ComponentValue::List(vec![path_value("list-a"), path_value("list-b")]),
                ),
            ],
            vec![
                pin(
                    "prior-target",
                    instance("prior", vec![set("image", path_value("prior"))], Vec::new()),
                ),
                pin(
                    "next-target",
                    instance(
                        "next",
                        Vec::new(),
                        vec![
                            pin(
                                "nested-prior-target",
                                instance(
                                    "nested-prior",
                                    vec![set(
                                        "palette",
                                        ComponentValue::map([
                                            ("a", path_value("map-a")),
                                            ("b", path_value("map-b")),
                                            (
                                                "missing",
                                                reference_value(AssetRef::StableId(missing)),
                                            ),
                                        ]),
                                    )],
                                    Vec::new(),
                                ),
                            ),
                            pin(
                                "nested-next-target",
                                instance("nested-next", vec![remove_default()], Vec::new()),
                            ),
                        ],
                    ),
                ),
            ],
        ));
        let overlay = anchor.prefab.as_mut().unwrap().overrides.as_mut().unwrap();
        // Dormant, obsolete and conflicting payloads are source data. Inventory must neither
        // guess their dependencies nor require native semantic activation to preserve them.
        for version in [ComponentSchemaVersion::ONE, ComponentSchemaVersion(3)] {
            let mut obsolete = set("image", path_value("unavailable-version"));
            let PrefabComponentIntent::Properties {
                version: declared, ..
            } = &mut obsolete
            else {
                unreachable!();
            };
            *declared = version;
            overlay.components.push(obsolete);
        }
        let mut unknown = set("image", path_value("unknown-schema"));
        let PrefabComponentIntent::Properties { component, .. } = &mut unknown else {
            unreachable!();
        };
        *component = ComponentTypeId::new("nara.test.UnavailableOverlayAssets");
        overlay.components.push(unknown);
        overlay.components.push(set(
            "image",
            ComponentValue::String("preserve incompatible asset payload".to_owned()),
        ));
        let scene = SceneDocument::from_ordered_roots([anchor]);
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            scene.to_json_string().unwrap(),
        )
        .unwrap();
        let mut prefab_anchor =
            SceneEntityRecord::new(SceneEntityId::new("prefab-anchor").unwrap());
        prefab_anchor.prefab = Some(instance(
            "next",
            vec![remove_default()],
            vec![
                pin(
                    "prior-target",
                    instance(
                        "prior",
                        vec![set("image", path_value("prefab-prior"))],
                        Vec::new(),
                    ),
                ),
                pin(
                    "next-target",
                    instance(
                        "next",
                        vec![set("image", path_value("prefab-next"))],
                        Vec::new(),
                    ),
                ),
            ],
        ));
        let outer = PrefabDocument::from_ordered_roots([prefab_anchor]);
        for name in ["outer", "prior", "next", "nested-prior", "nested-next"] {
            let encoded = if name == "outer" {
                outer.to_json_string().unwrap()
            } else {
                PrefabDocument::default().to_json_string().unwrap()
            };
            fs::write(
                project.root.join(format!("prefabs/{name}.prefab.json")),
                encoded,
            )
            .unwrap();
        }
        let names = [
            "direct",
            "list-a",
            "list-b",
            "prior",
            "map-a",
            "map-b",
            "default",
            "prefab-prior",
            "prefab-next",
        ];
        for (index, name) in names.iter().chain(["unreferenced"].iter()).enumerate() {
            fs::write(
                project.root.join(format!("assets/{name}.png")),
                name.as_bytes(),
            )
            .unwrap();
            fs::write(
                project.root.join(format!("assets/{name}.png.meta")),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        StableAssetId::parse_str(&format!("2f0d71c7-14fc-4ed4-b48b-{index:012x}"))
                            .unwrap(),
                        AssetPath::new(format!("{name}.png")).unwrap(),
                        AssetSourceKind::Image,
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
        }
        let mut host = AuthoringProjectAdmissionHost::default();
        let schema = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let receipt = PreparedAuthoringProject::new(schema, Box::new([]))
            .admit_startup_closure()
            .unwrap()
            .publish();
        let snapshot = receipt.snapshot();
        assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Degraded);
        assert_eq!(snapshot.startup_scene().document(), &scene);
        assert!(snapshot.startup_scene().record_facts().is_empty());
        assert_eq!(
            snapshot
                .startup_scene()
                .schema_declared_asset_references()
                .count(),
            8
        );
        // Raw nested pins and field payloads remain inventory. Structural target closure
        // rejects the outer declaration before entering its ordinary deferred source slots.
        assert_eq!(
            snapshot
                .prefabs()
                .iter()
                .map(|prefab| prefab.path().as_str())
                .collect::<Vec<_>>(),
            ["outer.prefab.json"]
        );
        let captured_outer = snapshot
            .prefabs()
            .iter()
            .find(|source| source.path().as_str() == "outer.prefab.json")
            .unwrap();
        assert_eq!(captured_outer.document(), &outer);
        let independent_outer = captured_outer.publish_document().unwrap();
        assert_eq!(independent_outer.document(), captured_outer.document());
        assert_eq!(
            independent_outer.schema_basis(),
            snapshot.startup_scene().schema_basis(),
        );
        assert!(independent_outer.record_facts().is_empty());
        assert_eq!(
            independent_outer.schema_declared_asset_references().count(),
            3
        );
        assert!(snapshot.assets().is_empty());
        assert!(snapshot.unresolved_dependencies().is_empty());
        assert!(
            snapshot
                .startup_compilation_diagnostics()
                .iter()
                .any(|diagnostic| {
                    diagnostic.code().as_str() == "scene.prefab-compilation-failed"
                })
        );
        let expected_sources = std::collections::BTreeSet::from([
            "scenes/startup.scene.json".to_owned(),
            "prefabs/outer.prefab.json".to_owned(),
        ]);
        assert_eq!(
            snapshot
                .source_facts()
                .map(|fact| fact.project_path().as_str().to_owned())
                .collect::<std::collections::BTreeSet<_>>(),
            expected_sources
        );

        let scene_manifest = snapshot.startup_scene().unavailable_manifest().clone();
        for readiness in [
            nara_scene::AuthoringRecordReadiness::KnownUnbound,
            nara_scene::AuthoringRecordReadiness::UnknownSchema,
            nara_scene::AuthoringRecordReadiness::Unmigratable,
            nara_scene::AuthoringRecordReadiness::FutureSchema,
            nara_scene::AuthoringRecordReadiness::UnresolvedOverlay,
        ] {
            assert!(scene_manifest.entries().iter().any(|entry| matches!(
                entry, nara_scene::UnavailableRecordManifestEntry::OverlayTarget(target)
                    if target.readiness() == readiness
            )));
        }
        assert_eq!(scene_manifest.overlay_declarations().count(), 1);
        assert_eq!(
            scene_manifest.overlay_declarations().next().unwrap().1,
            scene.entities[0].prefab.as_ref().unwrap(),
        );
        let outer_manifest = independent_outer.unavailable_manifest().clone();
        // Persist captured declarations, then recapture and classify independently again.
        // Rejected effective activation does not prevent lossless source Save/Reopen.
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            snapshot
                .startup_scene()
                .document()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.join("prefabs/outer.prefab.json"),
            captured_outer.document().to_json_string().unwrap(),
        )
        .unwrap();
        drop(snapshot);
        drop(receipt);
        let schema = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let reopened = PreparedAuthoringProject::new(schema, Box::new([]))
            .admit_startup_closure()
            .unwrap()
            .publish();
        let reopened_snapshot = reopened.snapshot();
        assert_eq!(reopened_snapshot.startup_scene().document(), &scene);
        assert_eq!(
            reopened_snapshot.startup_scene().unavailable_manifest(),
            &scene_manifest,
        );
        let reopened_outer = reopened_snapshot
            .prefabs()
            .iter()
            .find(|source| source.path().as_str() == "outer.prefab.json")
            .unwrap();
        assert_eq!(reopened_outer.document(), &outer);
        let independent_reopened_outer = reopened_outer.publish_document().unwrap();
        assert_eq!(
            independent_reopened_outer.schema_basis(),
            reopened_snapshot.startup_scene().schema_basis(),
        );
        assert_eq!(
            independent_reopened_outer.unavailable_manifest(),
            &outer_manifest,
        );
        assert!(reopened_snapshot.assets().is_empty());
        drop(reopened_snapshot);
        drop(reopened);
        fs::remove_file(project.root.join("assets/map-a.png")).unwrap();
        let schema = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let inactive_asset_removed = PreparedAuthoringProject::new(schema, Box::new([]))
            .admit_startup_closure()
            .unwrap()
            .publish();
        let snapshot = inactive_asset_removed.snapshot();
        assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Degraded);
        assert_eq!(
            snapshot.startup_scene().unavailable_manifest(),
            &scene_manifest
        );
        assert!(snapshot.assets().is_empty());
    }

    #[test]
    fn stable_prefab_closure_survives_rename_without_enabling_generic_nested_preview() {
        let project = TestAuthoringProject::new(
            "stable-prefab-closure",
            AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>()).unwrap(),
        );
        project.write_startup_manifest();
        fs::create_dir_all(project.root.join("scenes")).unwrap();
        fs::create_dir_all(project.root.join("prefabs")).unwrap();
        let stable_id = StableAssetId::parse_str("4e438770-1757-44f0-bbeb-6237cd6768ab").unwrap();
        let mut anchor = SceneEntityRecord::new(SceneEntityId::new("instance").unwrap());
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::StableId(stable_id),
            overrides: None,
        });
        let scene = SceneDocument::from_ordered_roots([anchor]);
        let prefab = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("leaf").unwrap(),
        )]);
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            scene.to_json_string().unwrap(),
        )
        .unwrap();
        for (previous, path) in [
            (None, "original.prefab.json"),
            (Some("original.prefab.json"), "renamed.prefab.json"),
        ] {
            if let Some(previous) = previous {
                fs::rename(
                    project.root.join("prefabs").join(previous),
                    project.root.join("prefabs").join(path),
                )
                .unwrap();
                fs::remove_file(
                    project
                        .root
                        .join("prefabs")
                        .join(format!("{previous}.meta")),
                )
                .unwrap();
            } else {
                fs::write(
                    project.root.join("prefabs").join(path),
                    prefab.to_json_string().unwrap(),
                )
                .unwrap();
            }
            fs::write(
                project.root.join("prefabs").join(format!("{path}.meta")),
                AssetMeta::new(
                    AssetMetaIdentity::new(
                        stable_id,
                        AssetPath::new(path).unwrap(),
                        AssetSourceKind::Prefab,
                    ),
                    (),
                )
                .to_json_string()
                .unwrap(),
            )
            .unwrap();
            let mut host = AuthoringProjectAdmissionHost::default();
            let schema = host
                .prepare(project.capability(), None)
                .unwrap()
                .admit_bundle()
                .unwrap()
                .publish();
            let loaded = PreparedAuthoringProject::new(schema, Box::new([]))
                .admit_startup_closure()
                .unwrap()
                .publish();
            let snapshot = loaded.snapshot();
            assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Complete);
            assert!(snapshot.unresolved_dependencies().is_empty());
            assert_eq!(snapshot.prefabs()[0].path().as_str(), path);
            assert_eq!(snapshot.startup_scene().document(), &scene);
            assert!(
                snapshot
                    .source_facts()
                    .any(|fact| fact.project_path().as_str() == format!("prefabs/{path}.meta"))
            );
            let projection = nara_scene::__private::project_published_authoring_scene_preview(
                snapshot.startup_scene(),
            );
            assert_eq!(projection.omissions().len(), 1);
            assert_eq!(
                projection.omissions()[0].reason(),
                nara_scene::__private::AuthoringPreviewOmissionReason::NestedPrefabUnsupported
            );
            assert!(projection.document().entities[0].prefab.is_none());
        }

        fs::write(
            project.root.join("prefabs/duplicate.prefab.json.meta"),
            AssetMeta::new(
                AssetMetaIdentity::new(
                    stable_id,
                    AssetPath::new("duplicate.prefab.json").unwrap(),
                    AssetSourceKind::Prefab,
                ),
                (),
            )
            .to_json_string()
            .unwrap(),
        )
        .unwrap();
        let mut host = AuthoringProjectAdmissionHost::default();
        let schema = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        let error = match PreparedAuthoringProject::new(schema, Box::new([]))
            .admit_startup_closure()
        {
            Ok(_) => panic!("duplicate stable identities must not publish an authoring closure"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            AuthoringProjectSourceAdmissionError::AssetIdentity {
                source: nara_asset::AssetDatabaseError::DuplicateStableId { .. },
                ..
            }
        ));
    }

    #[test]
    fn repeated_prefab_edges_exhaust_the_shared_closure_budget_without_a_receipt() {
        let project = TestAuthoringProject::new(
            "closure-edge-budget",
            AuthoringSchemaBundle::new(std::iter::empty::<AuthoringSchemaBundleOwner>()).unwrap(),
        );
        project.write_startup_manifest();
        fs::create_dir_all(project.root.join("scenes")).unwrap();
        fs::create_dir_all(project.root.join("prefabs")).unwrap();
        let shared_prefab = AssetRef::path("shared.prefab.json").unwrap();
        let startup = SceneDocument::from_ordered_roots([
            SceneEntityRecord {
                id: SceneEntityId::new("first").unwrap(),
                children: Vec::new(),
                components: Default::default(),
                prefab: Some(PrefabInstance {
                    source: shared_prefab.clone(),
                    overrides: None,
                }),
            },
            SceneEntityRecord {
                id: SceneEntityId::new("second").unwrap(),
                children: Vec::new(),
                components: Default::default(),
                prefab: Some(PrefabInstance {
                    source: shared_prefab,
                    overrides: None,
                }),
            },
        ]);
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            startup.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            project.root.join("prefabs/shared.prefab.json"),
            PrefabDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        let limits = AuthoringProjectAdmissionLimits {
            dependency_edges: ItemLimit::new(1).unwrap(),
            ..AuthoringProjectAdmissionLimits::default()
        };
        let mut host = AuthoringProjectAdmissionHost::new(limits);
        let schema = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();

        let error =
            match PreparedAuthoringProject::new(schema, Box::new([])).admit_startup_closure() {
                Ok(_) => panic!("the over-budget closure must not publish a project candidate"),
                Err(error) => error,
            };

        assert!(matches!(
            error,
            AuthoringProjectSourceAdmissionError::Budget(
                AuthoringProjectAdmissionError::DependencyEdgeLimitExceeded { maximum: 1 }
            )
        ));
    }

    fn schema_bundle(owner: &str, component: &str) -> AuthoringSchemaBundle {
        AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new(owner).unwrap(),
            component_catalog(component),
            None,
        )])
        .unwrap()
    }

    fn component_catalog(component: &str) -> ComponentSchemaCatalog {
        ComponentSchemaCatalog {
            components: vec![ComponentSchema::new(
                ComponentTypeId::new(component),
                component.rsplit('.').next().unwrap(),
                ComponentSchemaVersion::ONE,
            )],
            ..ComponentSchemaCatalog::default()
        }
    }

    fn asset_schema_bundle(
        owner: PersistentComponentSchemaOwnerId,
        component: ComponentTypeId,
    ) -> AuthoringSchemaBundle {
        let schema = ComponentSchema::new(component, "ImageLink", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("image"),
                "Image",
                ComponentFieldPath::from_fields(["image"]),
                ComponentValueKind::AssetRef,
            )
            .with_capability(ComponentCapability::AssetRef)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
        AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner,
            ComponentSchemaCatalog {
                components: vec![schema],
                ..ComponentSchemaCatalog::default()
            },
            None,
        )])
        .unwrap()
    }

    pub(in crate::project_host::authoring) struct TestAuthoringProject {
        pub(in crate::project_host::authoring) root: PathBuf,
    }

    impl TestAuthoringProject {
        fn new(name: &str, bundle: AuthoringSchemaBundle) -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "nara_authoring_admission_{}_{}",
                std::process::id(),
                sequence
            ));
            fs::create_dir_all(root.join("schemas")).unwrap();
            fs::write(
                root.join(PROJECT_MANIFEST_PATH),
                format!(
                    r#"schema_version = 1

[project]
source_universe = 'bb46c078-85b1-5823-8b1d-647b3b9e1f22'
native_document_identity = 'project_location'
name = "{name}"

[authoring]
schema_bundle = "{BUNDLE_PATH}"
"#
                ),
            )
            .unwrap();
            fs::write(root.join(BUNDLE_PATH), bundle.to_json_string().unwrap()).unwrap();
            Self { root }
        }

        fn capability(&self) -> DirectoryCapability {
            DirectoryCapability::from_host_handle(
                host_directory(&self.root).unwrap(),
                HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
            )
            .unwrap()
        }

        fn write_startup_manifest(&self) {
            fs::write(
                self.root.join(PROJECT_MANIFEST_PATH),
                format!(
                    r#"schema_version = 1

[project]
source_universe = 'bb46c078-85b1-5823-8b1d-647b3b9e1f22'
native_document_identity = 'project_location'
name = "asset-closure"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[authoring]
schema_bundle = "{BUNDLE_PATH}"
"#
                ),
            )
            .unwrap();
        }
    }

    impl Drop for TestAuthoringProject {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[cfg(windows)]
    fn host_directory(path: &Path) -> io::Result<File> {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }

    #[cfg(not(windows))]
    fn host_directory(path: &Path) -> io::Result<File> {
        File::open(path)
    }
}
