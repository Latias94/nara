use std::{
    fmt::{self, Debug, Formatter},
    num::NonZeroU64,
    sync::Arc,
};

use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity};

use super::AuthoringSchemaReceipt;

mod attempt;
mod budget;

#[cfg(test)]
mod tests;

use attempt::{AuthoringSourceIndexAttempt, SourceIndexAttemptProgress};

const PROJECT_MANIFEST_PATH: &str = "nara.toml";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ProjectSourceIndexGeneration(NonZeroU64);

impl ProjectSourceIndexGeneration {
    #[must_use]
    pub(crate) const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ProjectSourceRootRole {
    Assets,
    Scenes,
    Prefabs,
    Scripts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSourceRootAvailability {
    Available,
    Missing,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectSourceRootFact {
    relative_path: Arc<str>,
    roles: Box<[ProjectSourceRootRole]>,
    availability: ProjectSourceRootAvailability,
    identity: Option<FileIdentity>,
}

impl ProjectSourceRootFact {
    #[must_use]
    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[must_use]
    pub(crate) fn roles(&self) -> &[ProjectSourceRootRole] {
        &self.roles
    }

    #[must_use]
    pub(crate) const fn availability(&self) -> ProjectSourceRootAvailability {
        self.availability
    }

    #[must_use]
    pub(crate) const fn identity(&self) -> Option<FileIdentity> {
        self.identity
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ProjectSourceKind {
    ProjectManifest,
    AuthoringSchemaBundle,
    Scene,
    Prefab,
    AssetSource,
    AssetMeta,
    Script,
    Directory,
    Unknown,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSourceInspectReason {
    ProjectAuthority,
    ReadOnlySource,
    Directory,
    UnknownSourceKind,
    MissingDeclaredRoot,
    UnavailableDeclaredRoot,
    UnsupportedEntry,
    IdentityUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSourceOpenCapability {
    Scene,
    Prefab,
    InspectOnly(ProjectSourceInspectReason),
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectSourceIndexEntry {
    relative_path: Arc<str>,
    kind: ProjectSourceKind,
    scoped_identity: Option<FileIdentity>,
    open_capability: ProjectSourceOpenCapability,
}

impl ProjectSourceIndexEntry {
    #[must_use]
    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[must_use]
    pub(crate) const fn kind(&self) -> ProjectSourceKind {
        self.kind
    }

    #[must_use]
    pub(crate) const fn scoped_identity(&self) -> Option<FileIdentity> {
        self.scoped_identity
    }

    #[must_use]
    pub(crate) const fn open_capability(&self) -> ProjectSourceOpenCapability {
        self.open_capability
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSourceIndexDiagnosticPhase {
    RootNormalization,
    Enumeration,
    LineageRevalidation,
    Publication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSourceIndexDiagnosticCode {
    DeclaredRootMissing,
    DeclaredRootUnavailable,
    EntryUnsupported,
    DirectoryUnavailable,
    CanonicalPathAmbiguous,
    IdentityAliasAmbiguous,
    DirectoryLineageChanged,
    EnumerationIncomplete,
    ManifestLineageChanged,
    SchemaBundleLineageChanged,
    EntryBudgetExceeded,
    DirectoryBudgetExceeded,
    DepthBudgetExceeded,
    PathBudgetExceeded,
    DiagnosticBudgetExceeded,
    RetainedSnapshotBudgetExceeded,
    GenerationExhausted,
}

impl ProjectSourceIndexDiagnosticCode {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::DeclaredRootMissing => "project.source-index.declared-root-missing",
            Self::DeclaredRootUnavailable => "project.source-index.declared-root-unavailable",
            Self::EntryUnsupported => "project.source-index.entry-unsupported",
            Self::DirectoryUnavailable => "project.source-index.directory-unavailable",
            Self::CanonicalPathAmbiguous => "project.source-index.canonical-path-ambiguous",
            Self::IdentityAliasAmbiguous => "project.source-index.identity-alias-ambiguous",
            Self::DirectoryLineageChanged => "project.source-index.directory-lineage-changed",
            Self::EnumerationIncomplete => "project.source-index.enumeration-incomplete",
            Self::ManifestLineageChanged => "project.source-index.manifest-lineage-changed",
            Self::SchemaBundleLineageChanged => {
                "project.source-index.schema-bundle-lineage-changed"
            }
            Self::EntryBudgetExceeded => "project.source-index.entry-budget-exceeded",
            Self::DirectoryBudgetExceeded => "project.source-index.directory-budget-exceeded",
            Self::DepthBudgetExceeded => "project.source-index.depth-budget-exceeded",
            Self::PathBudgetExceeded => "project.source-index.path-budget-exceeded",
            Self::DiagnosticBudgetExceeded => "project.source-index.diagnostic-budget-exceeded",
            Self::RetainedSnapshotBudgetExceeded => {
                "project.source-index.retained-snapshot-budget-exceeded"
            }
            Self::GenerationExhausted => "project.source-index.generation-exhausted",
        }
    }

    #[must_use]
    pub(crate) const fn summary(self) -> &'static str {
        match self {
            Self::DeclaredRootMissing => "A declared project source root is missing",
            Self::DeclaredRootUnavailable => {
                "A declared project source root cannot be safely enumerated"
            }
            Self::EntryUnsupported => "A project source entry was safely excluded",
            Self::DirectoryUnavailable => "A project source subtree was safely excluded",
            Self::CanonicalPathAmbiguous => {
                "Project source paths have ambiguous canonical spelling"
            }
            Self::IdentityAliasAmbiguous => "Project source roots alias the same live directory",
            Self::DirectoryLineageChanged => "A traversed project directory changed identity",
            Self::EnumerationIncomplete => "A project directory could not be completely enumerated",
            Self::ManifestLineageChanged => "The project manifest changed during source indexing",
            Self::SchemaBundleLineageChanged => {
                "The authoring schema bundle changed during source indexing"
            }
            Self::EntryBudgetExceeded => "The project source entry budget was exceeded",
            Self::DirectoryBudgetExceeded => "The project source directory budget was exceeded",
            Self::DepthBudgetExceeded => "The project source traversal depth was exceeded",
            Self::PathBudgetExceeded => "The project source path budget was exceeded",
            Self::DiagnosticBudgetExceeded => "The project source diagnostic budget was exceeded",
            Self::RetainedSnapshotBudgetExceeded => {
                "The retained project source snapshot budget was exceeded"
            }
            Self::GenerationExhausted => "The project source index generation space was exhausted",
        }
    }

    #[must_use]
    pub(crate) const fn impact(self) -> &'static str {
        match self {
            Self::DeclaredRootMissing
            | Self::DeclaredRootUnavailable
            | Self::EntryUnsupported
            | Self::DirectoryUnavailable => {
                "The affected source is unavailable; unrelated safe sources remain browsable."
            }
            _ => "The candidate index was not published; the last-good generation was retained.",
        }
    }

    #[must_use]
    pub(crate) const fn repair_hint(self) -> &'static str {
        match self {
            Self::DeclaredRootMissing => {
                "Create the declared directory or update nara.toml, then refresh."
            }
            Self::DeclaredRootUnavailable | Self::DirectoryUnavailable => {
                "Move the source onto a supported local filesystem and refresh."
            }
            Self::EntryUnsupported => {
                "Replace the unsupported entry with a portable regular file or directory."
            }
            Self::CanonicalPathAmbiguous | Self::IdentityAliasAmbiguous => {
                "Give each declared source root one unambiguous project-relative path."
            }
            Self::DirectoryLineageChanged | Self::EnumerationIncomplete => {
                "Finish external filesystem changes and refresh the Project Browser."
            }
            Self::ManifestLineageChanged | Self::SchemaBundleLineageChanged => {
                "Reopen the project so its manifest and authoring schema authority are read again."
            }
            Self::EntryBudgetExceeded
            | Self::DirectoryBudgetExceeded
            | Self::DepthBudgetExceeded
            | Self::PathBudgetExceeded
            | Self::DiagnosticBudgetExceeded
            | Self::RetainedSnapshotBudgetExceeded => {
                "Reduce the declared source tree and refresh the Project Browser."
            }
            Self::GenerationExhausted => "Reopen the project to create a fresh indexing session.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectSourceIndexDiagnostic {
    code: ProjectSourceIndexDiagnosticCode,
    phase: ProjectSourceIndexDiagnosticPhase,
    relative_source: Option<Arc<str>>,
}

impl ProjectSourceIndexDiagnostic {
    #[must_use]
    pub(crate) const fn code(&self) -> ProjectSourceIndexDiagnosticCode {
        self.code
    }

    #[must_use]
    pub(crate) const fn phase(&self) -> ProjectSourceIndexDiagnosticPhase {
        self.phase
    }

    #[must_use]
    pub(crate) fn relative_source(&self) -> Option<&str> {
        self.relative_source.as_deref()
    }

    #[must_use]
    pub(crate) const fn summary(&self) -> &'static str {
        self.code.summary()
    }

    #[must_use]
    pub(crate) const fn impact(&self) -> &'static str {
        self.code.impact()
    }

    #[must_use]
    pub(crate) const fn repair_hint(&self) -> &'static str {
        self.code.repair_hint()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectSourceIndexBudgetUsage {
    entries: usize,
    directories: usize,
    max_depth: usize,
    path_bytes: usize,
    diagnostics: usize,
    diagnostic_bytes: usize,
    retained_snapshot_bytes: usize,
}

impl ProjectSourceIndexBudgetUsage {
    #[must_use]
    pub(crate) const fn entries(self) -> usize {
        self.entries
    }

    #[must_use]
    pub(crate) const fn directories(self) -> usize {
        self.directories
    }

    #[must_use]
    pub(crate) const fn max_depth(self) -> usize {
        self.max_depth
    }

    #[must_use]
    pub(crate) const fn path_bytes(self) -> usize {
        self.path_bytes
    }

    #[must_use]
    pub(crate) const fn diagnostics(self) -> usize {
        self.diagnostics
    }

    #[must_use]
    pub(crate) const fn diagnostic_bytes(self) -> usize {
        self.diagnostic_bytes
    }

    #[must_use]
    pub(crate) const fn retained_snapshot_bytes(self) -> usize {
        self.retained_snapshot_bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSourceIndexCompletion {
    Ready,
    ReadyWithDiagnostics,
}

pub(crate) struct ProjectSourceIndexSnapshot {
    root_identity: FileIdentity,
    manifest_identity: FileIdentity,
    manifest_digest: ContentDigest,
    bundle_identity: FileIdentity,
    bundle_digest: ContentDigest,
    generation: ProjectSourceIndexGeneration,
    effective_roots: Box<[ProjectSourceRootFact]>,
    entries: Box<[ProjectSourceIndexEntry]>,
    diagnostics: Box<[ProjectSourceIndexDiagnostic]>,
    usage: ProjectSourceIndexBudgetUsage,
    completion: ProjectSourceIndexCompletion,
}

impl ProjectSourceIndexSnapshot {
    #[must_use]
    pub(crate) const fn root_identity(&self) -> FileIdentity {
        self.root_identity
    }

    #[must_use]
    pub(crate) const fn manifest_identity(&self) -> FileIdentity {
        self.manifest_identity
    }

    #[must_use]
    pub(crate) const fn manifest_digest(&self) -> ContentDigest {
        self.manifest_digest
    }

    #[must_use]
    pub(crate) const fn bundle_identity(&self) -> FileIdentity {
        self.bundle_identity
    }

    #[must_use]
    pub(crate) const fn bundle_digest(&self) -> ContentDigest {
        self.bundle_digest
    }

    #[must_use]
    pub(crate) const fn generation(&self) -> ProjectSourceIndexGeneration {
        self.generation
    }

    #[must_use]
    pub(crate) fn effective_roots(&self) -> &[ProjectSourceRootFact] {
        &self.effective_roots
    }

    #[must_use]
    pub(crate) fn entries(&self) -> &[ProjectSourceIndexEntry] {
        &self.entries
    }

    #[must_use]
    pub(crate) fn diagnostics(&self) -> &[ProjectSourceIndexDiagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub(crate) const fn usage(&self) -> ProjectSourceIndexBudgetUsage {
        self.usage
    }

    #[must_use]
    pub(crate) const fn completion(&self) -> ProjectSourceIndexCompletion {
        self.completion
    }

    pub(crate) fn entry(&self, relative_path: &str) -> Option<&ProjectSourceIndexEntry> {
        self.entries
            .binary_search_by(|entry| entry.relative_path().cmp(relative_path))
            .ok()
            .map(|index| &self.entries[index])
    }
}

impl Debug for ProjectSourceIndexSnapshot {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectSourceIndexSnapshot")
            .field("root_identity", &self.root_identity)
            .field("generation", &self.generation)
            .field("root_count", &self.effective_roots.len())
            .field("entry_count", &self.entries.len())
            .field("diagnostic_count", &self.diagnostics.len())
            .field("usage", &self.usage)
            .field("completion", &self.completion)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectSourceIndexFailure {
    diagnostic: ProjectSourceIndexDiagnostic,
    usage: ProjectSourceIndexBudgetUsage,
}

impl ProjectSourceIndexFailure {
    #[must_use]
    pub(crate) const fn diagnostic(&self) -> &ProjectSourceIndexDiagnostic {
        &self.diagnostic
    }

    #[must_use]
    pub(crate) const fn usage(&self) -> ProjectSourceIndexBudgetUsage {
        self.usage
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectSourceIndexStateKind {
    NotRequested,
    Scanning,
    Ready,
    ReadyWithDiagnostics,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProjectSourceIndexLimits {
    pub(super) max_entries: usize,
    pub(super) max_directories: usize,
    pub(super) max_depth: usize,
    pub(super) max_path_bytes: usize,
    pub(super) max_diagnostics: usize,
    pub(super) max_diagnostic_bytes: usize,
    pub(super) max_retained_snapshot_bytes: usize,
    pub(super) max_entries_per_update: usize,
    pub(super) max_directories_per_update: usize,
}

impl Default for ProjectSourceIndexLimits {
    fn default() -> Self {
        Self {
            max_entries: 16_384,
            max_directories: 2_048,
            max_depth: 32,
            max_path_bytes: 2 * 1024 * 1024,
            max_diagnostics: 256,
            max_diagnostic_bytes: 256 * 1024,
            max_retained_snapshot_bytes: 16 * 1024 * 1024,
            max_entries_per_update: 256,
            max_directories_per_update: 32,
        }
    }
}

pub(super) struct AuthoringSourceIndexBasis {
    pub(super) project_root: DirectoryCapability,
    pub(super) root_identity: FileIdentity,
    pub(super) manifest_identity: FileIdentity,
    pub(super) manifest_digest: ContentDigest,
    pub(super) bundle_path: Arc<str>,
    pub(super) bundle_identity: FileIdentity,
    pub(super) bundle_digest: ContentDigest,
    pub(super) declared_roots: Box<[(Arc<str>, ProjectSourceRootRole)]>,
}

impl AuthoringSourceIndexBasis {
    fn from_schema(schema: &AuthoringSchemaReceipt) -> Self {
        let settings = schema.settings();
        Self {
            project_root: schema.project_root(),
            root_identity: schema.root_identity(),
            manifest_identity: schema.manifest_identity(),
            manifest_digest: schema.manifest_digest(),
            bundle_path: Arc::from(schema.bundle_path().as_str()),
            bundle_identity: schema.bundle_identity(),
            bundle_digest: schema.bundle_digest(),
            declared_roots: Box::new([
                (
                    Arc::from(settings.paths.assets.as_str()),
                    ProjectSourceRootRole::Assets,
                ),
                (
                    Arc::from(settings.paths.scenes.as_str()),
                    ProjectSourceRootRole::Scenes,
                ),
                (
                    Arc::from(settings.paths.prefabs.as_str()),
                    ProjectSourceRootRole::Prefabs,
                ),
                (
                    Arc::from(settings.paths.scripts.as_str()),
                    ProjectSourceRootRole::Scripts,
                ),
            ]),
        }
    }

    pub(super) fn share(&self) -> Self {
        Self {
            project_root: self.project_root.share(),
            root_identity: self.root_identity,
            manifest_identity: self.manifest_identity,
            manifest_digest: self.manifest_digest,
            bundle_path: Arc::clone(&self.bundle_path),
            bundle_identity: self.bundle_identity,
            bundle_digest: self.bundle_digest,
            declared_roots: self.declared_roots.clone(),
        }
    }
}

pub(crate) struct ProjectSourceIndexState {
    basis: AuthoringSourceIndexBasis,
    limits: ProjectSourceIndexLimits,
    next_generation: Option<NonZeroU64>,
    attempt: Option<AuthoringSourceIndexAttempt>,
    snapshot: Option<Arc<ProjectSourceIndexSnapshot>>,
    failure: Option<ProjectSourceIndexFailure>,
}

impl ProjectSourceIndexState {
    pub(crate) fn new(schema: &AuthoringSchemaReceipt) -> Self {
        Self::with_limits(schema, ProjectSourceIndexLimits::default())
    }

    fn with_limits(schema: &AuthoringSchemaReceipt, limits: ProjectSourceIndexLimits) -> Self {
        Self {
            basis: AuthoringSourceIndexBasis::from_schema(schema),
            limits,
            next_generation: Some(NonZeroU64::MIN),
            attempt: None,
            snapshot: None,
            failure: None,
        }
    }

    pub(crate) fn request_refresh(&mut self) -> bool {
        if self.attempt.is_some() {
            return false;
        }
        let Some(generation) = self.next_generation else {
            self.failure = Some(ProjectSourceIndexFailure::new(
                ProjectSourceIndexDiagnosticCode::GenerationExhausted,
                ProjectSourceIndexDiagnosticPhase::Publication,
                None,
                ProjectSourceIndexBudgetUsage::default(),
            ));
            return false;
        };
        self.next_generation = generation.get().checked_add(1).and_then(NonZeroU64::new);
        let generation = ProjectSourceIndexGeneration(generation);
        match AuthoringSourceIndexAttempt::begin(self.basis.share(), generation, self.limits) {
            Ok(attempt) => {
                self.attempt = Some(attempt);
                self.failure = None;
                true
            }
            Err(failure) => {
                self.failure = Some(failure);
                false
            }
        }
    }

    pub(crate) fn advance(&mut self) {
        let Some(mut attempt) = self.attempt.take() else {
            return;
        };
        match attempt.advance() {
            SourceIndexAttemptProgress::Pending => self.attempt = Some(attempt),
            SourceIndexAttemptProgress::Published(snapshot) => {
                self.snapshot = Some(Arc::new(snapshot));
                self.failure = None;
            }
            SourceIndexAttemptProgress::Failed(failure) => {
                self.failure = Some(failure);
            }
        }
    }

    #[must_use]
    pub(crate) fn kind(&self) -> ProjectSourceIndexStateKind {
        if self.attempt.is_some() {
            ProjectSourceIndexStateKind::Scanning
        } else if self.failure.is_some() {
            ProjectSourceIndexStateKind::Unavailable
        } else {
            match self
                .snapshot
                .as_deref()
                .map(ProjectSourceIndexSnapshot::completion)
            {
                Some(ProjectSourceIndexCompletion::Ready) => ProjectSourceIndexStateKind::Ready,
                Some(ProjectSourceIndexCompletion::ReadyWithDiagnostics) => {
                    ProjectSourceIndexStateKind::ReadyWithDiagnostics
                }
                None => ProjectSourceIndexStateKind::NotRequested,
            }
        }
    }

    #[must_use]
    pub(crate) const fn has_pending_work(&self) -> bool {
        self.attempt.is_some()
    }

    #[must_use]
    pub(crate) fn snapshot(&self) -> Option<Arc<ProjectSourceIndexSnapshot>> {
        self.snapshot.as_ref().map(Arc::clone)
    }

    #[must_use]
    pub(crate) const fn failure(&self) -> Option<&ProjectSourceIndexFailure> {
        self.failure.as_ref()
    }

    #[cfg(test)]
    fn is_revalidating(&self) -> bool {
        self.attempt
            .as_ref()
            .is_some_and(AuthoringSourceIndexAttempt::is_revalidating)
    }
}

impl ProjectSourceIndexDiagnostic {
    pub(super) fn new(
        code: ProjectSourceIndexDiagnosticCode,
        phase: ProjectSourceIndexDiagnosticPhase,
        relative_source: Option<Arc<str>>,
    ) -> Self {
        Self {
            code,
            phase,
            relative_source,
        }
    }

    pub(super) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.relative_source.as_deref().map_or(0, str::len)
    }

    pub(super) fn text_bytes(&self) -> usize {
        self.code.as_str().len()
            + self.code.summary().len()
            + self.code.impact().len()
            + self.code.repair_hint().len()
            + self.relative_source.as_deref().map_or(0, str::len)
    }
}

impl ProjectSourceIndexFailure {
    pub(super) fn new(
        code: ProjectSourceIndexDiagnosticCode,
        phase: ProjectSourceIndexDiagnosticPhase,
        relative_source: Option<Arc<str>>,
        usage: ProjectSourceIndexBudgetUsage,
    ) -> Self {
        Self {
            diagnostic: ProjectSourceIndexDiagnostic::new(code, phase, relative_source),
            usage,
        }
    }
}

impl ProjectSourceIndexEntry {
    pub(super) fn new(
        relative_path: Arc<str>,
        kind: ProjectSourceKind,
        scoped_identity: Option<FileIdentity>,
        open_capability: ProjectSourceOpenCapability,
    ) -> Self {
        Self {
            relative_path,
            kind,
            scoped_identity,
            open_capability,
        }
    }

    pub(super) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.relative_path.len()
    }
}

impl ProjectSourceRootFact {
    pub(super) fn new(
        relative_path: Arc<str>,
        roles: Box<[ProjectSourceRootRole]>,
        availability: ProjectSourceRootAvailability,
        identity: Option<FileIdentity>,
    ) -> Self {
        Self {
            relative_path,
            roles,
            availability,
            identity,
        }
    }

    pub(super) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.relative_path.len()
            + std::mem::size_of_val(self.roles.as_ref())
    }
}

impl ProjectSourceIndexSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        basis: &AuthoringSourceIndexBasis,
        generation: ProjectSourceIndexGeneration,
        effective_roots: Box<[ProjectSourceRootFact]>,
        entries: Box<[ProjectSourceIndexEntry]>,
        diagnostics: Box<[ProjectSourceIndexDiagnostic]>,
        usage: ProjectSourceIndexBudgetUsage,
    ) -> Self {
        let completion = if diagnostics.is_empty() {
            ProjectSourceIndexCompletion::Ready
        } else {
            ProjectSourceIndexCompletion::ReadyWithDiagnostics
        };
        Self {
            root_identity: basis.root_identity,
            manifest_identity: basis.manifest_identity,
            manifest_digest: basis.manifest_digest,
            bundle_identity: basis.bundle_identity,
            bundle_digest: basis.bundle_digest,
            generation,
            effective_roots,
            entries,
            diagnostics,
            usage,
            completion,
        }
    }
}
