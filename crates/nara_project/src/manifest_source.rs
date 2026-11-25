use std::{fmt, io::Read};

use nara_core::diagnostic::{
    DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, ReportDiagnostic as Diagnostic, SafeSummary,
};
use nara_core::vfs::capabilities::{
    ContentDigest, FileCapability, FileIdentity, FsError, FsOperation, PathValidationError,
};

use crate::{
    DEFAULT_MANIFEST_BYTE_LIMIT, EffectiveProjectSettings, ProjectManifest, ProjectProfileError,
    SourceRuntimeProjection,
};

/// The source facts resolved from one authorized project manifest snapshot.
///
/// The snapshot owns the parsed settings and the digest used to derive its lineage. It does not
/// grant filesystem authority or retain a mutable manifest handle. Callers that use a source
/// snapshot for content loading must still validate the root identity against their live root.
#[derive(Clone, PartialEq)]
pub struct ProjectManifestSource {
    lineage: ProjectSettingsLineage,
    manifest_digest: ContentDigest,
    settings: EffectiveProjectSettings,
    source_runtime_projection: SourceRuntimeProjection,
}

impl fmt::Debug for ProjectManifestSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectManifestSource")
            .field("lineage", &self.lineage)
            .field("manifest_digest", &self.manifest_digest)
            .field("profile_present", &self.settings.profile_name.is_some())
            .finish()
    }
}

impl ProjectManifestSource {
    /// Reads one already-authorized manifest handle with the bounded source-read contract.
    pub fn from_file(
        manifest: &FileCapability,
        profile: Option<&str>,
    ) -> Result<Self, ProjectManifestSourceError> {
        let project_root_identity = manifest.parent_identity();
        let bytes = read_manifest_bytes(manifest)?;
        let manifest_digest = ContentDigest::of_bytes(&bytes);
        Self::from_bytes_with_digest(&bytes, profile, project_root_identity, manifest_digest)
    }

    /// Reads one authorized handle and rejects bytes that differ from a prepared digest.
    pub fn from_file_with_expected_digest(
        manifest: &FileCapability,
        profile: Option<&str>,
        expected_digest: ContentDigest,
    ) -> Result<Self, ProjectManifestSourceError> {
        let project_root_identity = manifest.parent_identity();
        let bytes = read_manifest_bytes(manifest)?;
        let manifest_digest = ContentDigest::of_bytes(&bytes);
        if manifest_digest != expected_digest {
            return Err(ProjectManifestSourceError::single(
                ProjectManifestSourceErrorKind::HostAuthorityRejected,
                diagnostic(
                    "project.manifest.source-changed",
                    "Project manifest bytes do not match the prepared source digest",
                ),
            ));
        }
        Self::from_bytes_with_digest(&bytes, profile, project_root_identity, manifest_digest)
    }

    /// Resolves a bounded manifest byte snapshot for a project root identity.
    ///
    /// The identity must come from the currently authorized project root. The project Host
    /// performs the final live-root comparison before admitting source content.
    pub fn from_bytes(
        bytes: &[u8],
        profile: Option<&str>,
        project_root_identity: Option<FileIdentity>,
    ) -> Result<Self, ProjectManifestSourceError> {
        let manifest_digest = ContentDigest::of_bytes(bytes);
        Self::from_bytes_with_digest(bytes, profile, project_root_identity, manifest_digest)
    }

    /// Reads and resolves only the effective settings from an authorized manifest handle.
    pub fn read_profile(
        manifest: &FileCapability,
        profile: Option<&str>,
    ) -> Result<EffectiveProjectSettings, ProjectManifestSourceError> {
        resolve_manifest_profile(&read_manifest_bytes(manifest)?, profile)
    }

    #[must_use]
    pub const fn lineage(&self) -> ProjectSettingsLineage {
        self.lineage
    }

    #[must_use]
    pub const fn manifest_digest(&self) -> ContentDigest {
        self.manifest_digest
    }

    #[must_use]
    pub const fn settings(&self) -> &EffectiveProjectSettings {
        &self.settings
    }

    #[must_use]
    pub const fn source_runtime_projection(&self) -> &SourceRuntimeProjection {
        &self.source_runtime_projection
    }

    /// Separates the immutable source facts for root-level capability admission.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        ProjectSettingsLineage,
        ContentDigest,
        EffectiveProjectSettings,
        SourceRuntimeProjection,
    ) {
        (
            self.lineage,
            self.manifest_digest,
            self.settings,
            self.source_runtime_projection,
        )
    }

    fn from_bytes_with_digest(
        bytes: &[u8],
        profile: Option<&str>,
        project_root_identity: Option<FileIdentity>,
        manifest_digest: ContentDigest,
    ) -> Result<Self, ProjectManifestSourceError> {
        let settings = resolve_manifest_profile(bytes, profile)?;
        let lineage = ProjectSettingsLineage::from_manifest(
            manifest_digest,
            settings.profile_name.as_deref(),
            project_root_identity,
        );
        let source_runtime_projection = settings.source_runtime_projection().map_err(|error| {
            ProjectManifestSourceError::new(
                ProjectManifestSourceErrorKind::RuntimeConfiguration,
                error.diagnostics().clone(),
            )
        })?;
        Ok(Self {
            lineage,
            manifest_digest,
            settings,
            source_runtime_projection,
        })
    }
}

/// Opaque identity of one authorized project root, manifest byte sequence, and selected profile.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectSettingsLineage {
    settings_digest: [u8; 32],
    project_root_identity: Option<FileIdentity>,
}

impl fmt::Debug for ProjectSettingsLineage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProjectSettingsLineage(..)")
    }
}

impl ProjectSettingsLineage {
    #[must_use]
    pub const fn settings_digest(self) -> [u8; 32] {
        self.settings_digest
    }

    #[must_use]
    pub const fn project_root_identity(self) -> Option<FileIdentity> {
        self.project_root_identity
    }

    fn from_manifest(
        manifest: ContentDigest,
        profile: Option<&str>,
        project_root_identity: Option<FileIdentity>,
    ) -> Self {
        let profile = profile.map(|profile| ContentDigest::of_bytes(profile.as_bytes()));
        let mut framed = Vec::with_capacity(96);
        framed.extend_from_slice(b"nara.project-settings-lineage.v1\0");
        framed.extend_from_slice(&manifest.length().to_le_bytes());
        framed.extend_from_slice(manifest.as_bytes());
        match profile {
            Some(profile) => {
                framed.push(1);
                framed.extend_from_slice(&profile.length().to_le_bytes());
                framed.extend_from_slice(profile.as_bytes());
            }
            None => framed.push(0),
        }
        Self {
            settings_digest: *ContentDigest::of_bytes(&framed).as_bytes(),
            project_root_identity,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectManifestSourceErrorKind {
    HostIo,
    HostAuthorityUnsupported,
    HostAuthorityUnproven,
    HostAuthorityRejected,
    ManifestTooLarge,
    Manifest,
    Profile,
    RuntimeConfiguration,
}

#[derive(Clone, PartialEq)]
pub struct ProjectManifestSourceError {
    kind: ProjectManifestSourceErrorKind,
    diagnostics: DiagnosticReport,
}

impl ProjectManifestSourceError {
    #[must_use]
    pub const fn kind(&self) -> ProjectManifestSourceErrorKind {
        self.kind
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    #[must_use]
    pub fn into_diagnostics(self) -> DiagnosticReport {
        self.diagnostics
    }

    /// Lowers an authority or host failure encountered before a manifest source exists.
    #[must_use]
    pub fn from_manifest_authority(error: FsError) -> Self {
        project_fs_error(error)
    }

    fn new(kind: ProjectManifestSourceErrorKind, diagnostics: DiagnosticReport) -> Self {
        Self { kind, diagnostics }
    }

    fn single(kind: ProjectManifestSourceErrorKind, diagnostic: Diagnostic) -> Self {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(diagnostic);
        Self::new(kind, diagnostics)
    }
}

impl fmt::Debug for ProjectManifestSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectManifestSourceError")
            .field("kind", &self.kind)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

impl fmt::Display for ProjectManifestSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            ProjectManifestSourceErrorKind::HostIo => "project manifest host I/O failed",
            ProjectManifestSourceErrorKind::HostAuthorityUnsupported => {
                "project manifest authority is unsupported"
            }
            ProjectManifestSourceErrorKind::HostAuthorityUnproven => {
                "project manifest authority is unproven"
            }
            ProjectManifestSourceErrorKind::HostAuthorityRejected => {
                "project manifest authority was rejected"
            }
            ProjectManifestSourceErrorKind::ManifestTooLarge => {
                "project manifest exceeds its byte limit"
            }
            ProjectManifestSourceErrorKind::Manifest => "project manifest is invalid",
            ProjectManifestSourceErrorKind::Profile => "project profile is invalid",
            ProjectManifestSourceErrorKind::RuntimeConfiguration => {
                "project Runtime configuration is invalid"
            }
        })
    }
}

impl std::error::Error for ProjectManifestSourceError {}

fn resolve_manifest_profile(
    bytes: &[u8],
    profile: Option<&str>,
) -> Result<EffectiveProjectSettings, ProjectManifestSourceError> {
    let load = ProjectManifest::parse_toml_bytes(bytes);
    if load.has_errors() || load.manifest.is_none() {
        let kind = if load
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "project.manifest.too-large")
        {
            ProjectManifestSourceErrorKind::ManifestTooLarge
        } else {
            ProjectManifestSourceErrorKind::Manifest
        };
        return Err(ProjectManifestSourceError::new(kind, load.diagnostics));
    }
    let manifest = load
        .manifest
        .expect("a successful project manifest load contains a manifest");

    manifest
        .resolve_profile(profile)
        .map_err(project_profile_error)
}

fn read_manifest_bytes(manifest: &FileCapability) -> Result<Vec<u8>, ProjectManifestSourceError> {
    let limit = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT)
        .expect("the project manifest byte limit fits usize on supported targets");
    let mut reader = manifest
        .reader()
        .map_err(ProjectManifestSourceError::from_manifest_authority)?;
    let mut bytes = Vec::with_capacity(limit.min(8 * 1024));
    let mut buffer = [0_u8; 8 * 1024];

    loop {
        let remaining = limit.saturating_sub(bytes.len());
        let read_limit = buffer.len().min(remaining.saturating_add(1));
        let read = reader
            .read(&mut buffer[..read_limit])
            .map_err(project_io_error)?;
        if read == 0 {
            return Ok(bytes);
        }
        if read > remaining {
            return Err(manifest_too_large_error(limit.saturating_add(1), limit));
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
}

fn project_profile_error(error: ProjectProfileError) -> ProjectManifestSourceError {
    match error {
        ProjectProfileError::InvalidManifest { diagnostics }
        | ProjectProfileError::UnknownProfile { diagnostics, .. } => {
            ProjectManifestSourceError::new(ProjectManifestSourceErrorKind::Profile, *diagnostics)
        }
        _ => ProjectManifestSourceError::single(
            ProjectManifestSourceErrorKind::Profile,
            diagnostic(
                "project.profile.lowering",
                "Project profile could not be lowered into runtime settings",
            ),
        ),
    }
}

fn project_fs_error(error: FsError) -> ProjectManifestSourceError {
    match error {
        FsError::Path(error) => authority_rejection(path_rejection_id(error)),
        FsError::Unsupported { operation, .. } => authority_error(
            ProjectManifestSourceErrorKind::HostAuthorityUnsupported,
            "project.manifest.authority-unsupported",
            "Project manifest authority is unsupported",
            operation,
        ),
        FsError::Unproven { operation, .. } => authority_error(
            ProjectManifestSourceErrorKind::HostAuthorityUnproven,
            "project.manifest.authority-unproven",
            "Project manifest authority cannot prove the required invariant",
            operation,
        ),
        FsError::Io { operation, source } => host_io_error(operation, source.kind()),
        FsError::ByteLimitExceeded { limit } => manifest_too_large_error(
            usize::try_from(limit).unwrap_or(usize::MAX),
            usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT).unwrap_or(usize::MAX),
        ),
        FsError::ReadOnlyCapability { operation } | FsError::AlreadyExists { operation } => {
            authority_error(
                ProjectManifestSourceErrorKind::HostAuthorityRejected,
                "project.manifest.authority-rejected",
                "Project manifest authority was rejected",
                operation,
            )
        }
        FsError::NotDirectory => authority_rejection("not-directory"),
        FsError::NotRegularFile => authority_rejection("not-regular-file"),
        FsError::ReparsePoint { .. } => authority_rejection("reparse-point"),
        FsError::SymbolicLinkTraversal => authority_rejection("symbolic-link"),
        FsError::CrossVolume => authority_rejection("cross-volume"),
        FsError::MultipleLinks { .. } => authority_rejection("multiple-links"),
        FsError::IdentityUnavailable => authority_rejection("identity-unavailable"),
        FsError::CapabilitySessionExhausted => authority_rejection("capability-session-exhausted"),
        FsError::IdentityMismatch { .. } => authority_rejection("identity-mismatch"),
        FsError::TemporaryParentMismatch => authority_rejection("temporary-parent-mismatch"),
        FsError::TargetStateMismatch => authority_rejection("target-state-mismatch"),
        FsError::LockContended => authority_rejection("lock-contended"),
        FsError::DigestMismatch { .. } => authority_rejection("digest-mismatch"),
        _ => authority_rejection("unclassified-filesystem-rejection"),
    }
}

fn path_rejection_id(error: PathValidationError) -> &'static str {
    match error {
        PathValidationError::Empty => "empty-relative-path",
        PathValidationError::AbsoluteOrPrefixed => "absolute-or-prefixed-path",
        PathValidationError::CurrentDirectory => "current-directory-component",
        PathValidationError::ParentTraversal => "parent-traversal",
        PathValidationError::EmptyComponent => "empty-path-component",
        PathValidationError::ForbiddenCharacter => "forbidden-path-character",
        PathValidationError::ReservedDeviceName => "reserved-device-name",
        PathValidationError::ComponentTooLong => "path-component-too-long",
        PathValidationError::PathTooLong => "relative-path-too-long",
    }
}

fn project_io_error(source: std::io::Error) -> ProjectManifestSourceError {
    host_io_error(FsOperation::Read, source.kind())
}

fn host_io_error(operation: FsOperation, kind: std::io::ErrorKind) -> ProjectManifestSourceError {
    let diagnostic = diagnostic(
        "project.manifest.host-io",
        "Project manifest host I/O failed",
    );
    let diagnostic = with_public_identifier(diagnostic, "operation", fs_operation_id(operation));
    let diagnostic = with_public_identifier(diagnostic, "io_kind", io_error_kind_id(kind));
    ProjectManifestSourceError::single(
        ProjectManifestSourceErrorKind::HostIo,
        with_sensitive(diagnostic, "manifest_source"),
    )
}

fn authority_error(
    kind: ProjectManifestSourceErrorKind,
    code: &'static str,
    summary: &'static str,
    operation: FsOperation,
) -> ProjectManifestSourceError {
    let diagnostic = with_public_identifier(
        diagnostic(code, summary),
        "operation",
        fs_operation_id(operation),
    );
    ProjectManifestSourceError::single(kind, with_sensitive(diagnostic, "manifest_source"))
}

fn authority_rejection(reason: &'static str) -> ProjectManifestSourceError {
    let diagnostic = with_public_identifier(
        diagnostic(
            "project.manifest.authority-rejected",
            "Project manifest authority was rejected",
        ),
        "reason",
        reason,
    );
    ProjectManifestSourceError::single(
        ProjectManifestSourceErrorKind::HostAuthorityRejected,
        with_sensitive(diagnostic, "manifest_source"),
    )
}

fn manifest_too_large_error(actual: usize, limit: usize) -> ProjectManifestSourceError {
    let diagnostic = diagnostic(
        "project.manifest.too-large",
        "Project manifest exceeds its byte limit",
    );
    let diagnostic = with_public_u64(
        diagnostic,
        "actual",
        u64::try_from(actual).unwrap_or(u64::MAX),
    );
    let diagnostic = with_public_u64(
        diagnostic,
        "limit",
        u64::try_from(limit).unwrap_or(u64::MAX),
    );
    ProjectManifestSourceError::single(ProjectManifestSourceErrorKind::ManifestTooLarge, diagnostic)
}

fn diagnostic(code: &'static str, summary: &'static str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new(code).expect("project source diagnostic codes are engine-owned"),
        SafeSummary::new(summary).expect("project source summaries are engine-owned"),
    )
}

fn with_public_identifier(
    diagnostic: Diagnostic,
    key: &'static str,
    value: &'static str,
) -> Diagnostic {
    let field = DiagnosticField::public_identifier(
        field_key(key),
        PublicDiagnosticIdentifier::new(value)
            .expect("project source identifiers are engine-owned literals"),
    );
    attach_field(diagnostic, field)
}

fn with_public_u64(diagnostic: Diagnostic, key: &'static str, value: u64) -> Diagnostic {
    attach_field(
        diagnostic,
        DiagnosticField::public_u64(field_key(key), value),
    )
}

fn with_sensitive(diagnostic: Diagnostic, key: &'static str) -> Diagnostic {
    attach_field(diagnostic, DiagnosticField::sensitive(field_key(key)))
}

fn field_key(key: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(key).expect("project source field keys are engine-owned")
}

fn attach_field(diagnostic: Diagnostic, field: DiagnosticField) -> Diagnostic {
    diagnostic
        .try_with_field(field)
        .expect("project source diagnostics use unique bounded fields")
}

fn fs_operation_id(operation: FsOperation) -> &'static str {
    match operation {
        FsOperation::InspectHandle => "inspect-handle",
        FsOperation::OpenDirectory => "open-directory",
        FsOperation::OpenFile => "open-file",
        FsOperation::ReadDirectory => "read-directory",
        FsOperation::BeginDirectoryPublication => "begin-directory-publication",
        FsOperation::CreateDirectory => "create-directory",
        FsOperation::CreateFile => "create-file",
        FsOperation::PublishDirectory => "publish-directory",
        FsOperation::RemoveDirectory => "remove-directory",
        FsOperation::CreateTemporary => "create-temporary",
        FsOperation::RemoveTemporary => "remove-temporary",
        FsOperation::Rename => "rename",
        FsOperation::Unlink => "unlink",
        FsOperation::Replace => "replace",
        FsOperation::SyncFile => "sync-file",
        FsOperation::SyncDirectory => "sync-directory",
        FsOperation::Lock => "lock",
        FsOperation::Unlock => "unlock",
        FsOperation::CloneHandle => "clone-handle",
        FsOperation::Read => "read",
        FsOperation::Write => "write",
    }
}

fn io_error_kind_id(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "not-found",
        std::io::ErrorKind::PermissionDenied => "permission-denied",
        std::io::ErrorKind::AlreadyExists => "already-exists",
        std::io::ErrorKind::InvalidInput => "invalid-input",
        std::io::ErrorKind::InvalidData => "invalid-data",
        std::io::ErrorKind::TimedOut => "timed-out",
        std::io::ErrorKind::Interrupted => "interrupted",
        std::io::ErrorKind::UnexpectedEof => "unexpected-eof",
        std::io::ErrorKind::OutOfMemory => "out-of-memory",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_MANIFEST: &[u8] = include_bytes!("../tests/fixtures/complete_v1.toml");

    #[test]
    fn byte_snapshots_preserve_stable_manifest_lineage() {
        let first = ProjectManifestSource::from_bytes(MINIMAL_MANIFEST, None, None).unwrap();
        let repeated = ProjectManifestSource::from_bytes(MINIMAL_MANIFEST, None, None).unwrap();
        let changed = ProjectManifestSource::from_bytes(
            &MINIMAL_MANIFEST
                .iter()
                .copied()
                .chain([b'\n'])
                .collect::<Vec<_>>(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(first.manifest_digest(), repeated.manifest_digest());
        assert_eq!(first.lineage(), repeated.lineage());
        assert_ne!(first.manifest_digest(), changed.manifest_digest());
        assert_ne!(first.lineage(), changed.lineage());
    }

    #[test]
    fn byte_snapshots_reject_the_manifest_sentinel_byte() {
        let limit = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT).unwrap();
        let error =
            ProjectManifestSource::from_bytes(&vec![b' '; limit + 1], None, None).unwrap_err();

        assert_eq!(
            error.kind(),
            ProjectManifestSourceErrorKind::ManifestTooLarge
        );
        assert_eq!(
            error
                .diagnostics()
                .iter()
                .next()
                .expect("the size diagnostic is present")
                .code()
                .as_str(),
            "project.manifest.too-large"
        );
    }
}
