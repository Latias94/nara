use std::{fmt, io::Read};

use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, SafeSummary,
};
use nara_fs::{ContentDigest, FileCapability, FsError, FsOperation, PathValidationError};
use nara_project::{
    DEFAULT_MANIFEST_BYTE_LIMIT, EffectiveProjectSettings, ProductCapability, ProductCapabilitySet,
    ProjectManifest, ProjectProfileError, RuntimePreset,
};

use super::composition::{
    ProjectSettingsCandidate, ProjectSettingsLineage, compiled_product_capabilities,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectCandidateErrorKind {
    HostIo,
    HostAuthorityUnsupported,
    HostAuthorityUnproven,
    HostAuthorityRejected,
    ManifestTooLarge,
    Manifest,
    Profile,
    PresetConflict,
    UnavailableCapabilities,
}

#[derive(Clone, PartialEq)]
pub struct ProjectCandidateError {
    kind: ProjectCandidateErrorKind,
    diagnostics: DiagnosticReport,
}

impl ProjectCandidateError {
    #[must_use]
    pub const fn kind(&self) -> ProjectCandidateErrorKind {
        self.kind
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    /// Lowers an authority or host failure encountered before a manifest
    /// `FileCapability` exists into the project-manifest diagnostic contract.
    #[must_use]
    pub fn from_manifest_authority(error: FsError) -> Self {
        project_fs_error(error)
    }

    fn new(kind: ProjectCandidateErrorKind, diagnostics: DiagnosticReport) -> Self {
        Self { kind, diagnostics }
    }

    fn single(kind: ProjectCandidateErrorKind, diagnostic: Diagnostic) -> Self {
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(diagnostic);
        Self::new(kind, diagnostics)
    }
}

impl fmt::Debug for ProjectCandidateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectCandidateError")
            .field("kind", &self.kind)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

impl fmt::Display for ProjectCandidateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            ProjectCandidateErrorKind::HostIo => "project manifest host I/O failed",
            ProjectCandidateErrorKind::HostAuthorityUnsupported => {
                "project manifest authority is unsupported"
            }
            ProjectCandidateErrorKind::HostAuthorityUnproven => {
                "project manifest authority is unproven"
            }
            ProjectCandidateErrorKind::HostAuthorityRejected => {
                "project manifest authority was rejected"
            }
            ProjectCandidateErrorKind::ManifestTooLarge => {
                "project manifest exceeds its byte limit"
            }
            ProjectCandidateErrorKind::Manifest => "project manifest is invalid",
            ProjectCandidateErrorKind::Profile => "project profile is invalid",
            ProjectCandidateErrorKind::PresetConflict => {
                "runtime preset conflicts with requested product capabilities"
            }
            ProjectCandidateErrorKind::UnavailableCapabilities => {
                "project requests unavailable product capabilities"
            }
        })
    }
}

impl std::error::Error for ProjectCandidateError {}

/// Reads, parses, lowers, and product-validates one already authorized `nara.toml` handle.
pub fn ingest_project_manifest(
    manifest: &FileCapability,
    profile: Option<&str>,
) -> Result<ProjectSettingsCandidate, ProjectCandidateError> {
    let bytes = read_manifest_bytes(manifest)?;
    let load = ProjectManifest::parse_toml_bytes(&bytes);
    if load.has_errors() || load.manifest.is_none() {
        return Err(ProjectCandidateError::new(
            ProjectCandidateErrorKind::Manifest,
            load.diagnostics,
        ));
    }
    let manifest = load
        .manifest
        .expect("a successful project manifest load contains a manifest");

    let settings = manifest
        .resolve_profile(profile)
        .map_err(project_profile_error)?;
    let lineage = project_settings_lineage(&bytes, settings.profile_name.as_deref());
    validate_product_request(settings, lineage)
}

fn read_manifest_bytes(manifest: &FileCapability) -> Result<Vec<u8>, ProjectCandidateError> {
    let limit = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT)
        .expect("the project manifest byte limit fits usize on supported targets");
    let mut reader = manifest
        .reader()
        .map_err(ProjectCandidateError::from_manifest_authority)?;
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

fn validate_product_request(
    settings: EffectiveProjectSettings,
    lineage: ProjectSettingsLineage,
) -> Result<ProjectSettingsCandidate, ProjectCandidateError> {
    let explicit = settings.requested_capabilities;
    let required = ProductCapabilitySet::singleton(ProductCapability::RuntimeCore);
    let mut normalized = explicit.union(required);
    if explicit.contains(ProductCapability::ToolingEgui) {
        normalized.insert(ProductCapability::Tooling);
    }
    let implied = normalized.difference(explicit);

    if settings.runtime_preset == RuntimePreset::Server {
        let forbidden = ProductCapabilitySet::from_capabilities([
            ProductCapability::Runtime2d,
            ProductCapability::RuntimeUi,
            ProductCapability::Tooling,
            ProductCapability::DesktopWinit,
            ProductCapability::RenderWgpu,
            ProductCapability::ToolingEgui,
        ]);
        let conflicting = normalized.intersection(forbidden);
        if !conflicting.is_empty() {
            return Err(capability_error(
                ProjectCandidateErrorKind::PresetConflict,
                "project.product.preset-conflict",
                "Runtime preset conflicts with requested product capabilities",
                conflicting,
            ));
        }
    }

    let compiled = compiled_product_capabilities();
    let unavailable = normalized.difference(compiled.capabilities());
    if !unavailable.is_empty() {
        return Err(capability_error(
            ProjectCandidateErrorKind::UnavailableCapabilities,
            "project.product.unavailable-capability",
            "Project requests a product capability that is not compiled",
            unavailable,
        ));
    }

    Ok(ProjectSettingsCandidate {
        lineage,
        settings,
        explicit,
        implied,
        normalized,
        required,
        compiled,
    })
}

fn project_settings_lineage(bytes: &[u8], profile: Option<&str>) -> ProjectSettingsLineage {
    let manifest = ContentDigest::of_bytes(bytes);
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
    ProjectSettingsLineage(*ContentDigest::of_bytes(&framed).as_bytes())
}

fn project_profile_error(error: ProjectProfileError) -> ProjectCandidateError {
    match error {
        ProjectProfileError::InvalidManifest { diagnostics }
        | ProjectProfileError::UnknownProfile { diagnostics, .. } => {
            ProjectCandidateError::new(ProjectCandidateErrorKind::Profile, *diagnostics)
        }
        _ => ProjectCandidateError::single(
            ProjectCandidateErrorKind::Profile,
            diagnostic(
                "project.profile.lowering",
                "Project profile could not be lowered into runtime settings",
            ),
        ),
    }
}

fn project_fs_error(error: FsError) -> ProjectCandidateError {
    match error {
        FsError::Path(error) => authority_rejection(path_rejection_id(error)),
        FsError::Unsupported { operation, .. } => authority_error(
            ProjectCandidateErrorKind::HostAuthorityUnsupported,
            "project.manifest.authority-unsupported",
            "Project manifest authority is unsupported",
            operation,
        ),
        FsError::Unproven { operation, .. } => authority_error(
            ProjectCandidateErrorKind::HostAuthorityUnproven,
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
                ProjectCandidateErrorKind::HostAuthorityRejected,
                "project.manifest.authority-rejected",
                "Project manifest authority was rejected",
                operation,
            )
        }
        FsError::NotDirectory => authority_rejection("not-directory"),
        FsError::NotRegularFile => authority_rejection("not-regular-file"),
        FsError::ReparsePoint { .. } => authority_rejection("reparse-point"),
        FsError::CrossVolume => authority_rejection("cross-volume"),
        FsError::MultipleLinks { .. } => authority_rejection("multiple-links"),
        FsError::IdentityUnavailable => authority_rejection("identity-unavailable"),
        FsError::CapabilitySessionExhausted => authority_rejection("capability-session-exhausted"),
        FsError::IdentityMismatch { .. } => authority_rejection("identity-mismatch"),
        FsError::TemporaryParentMismatch => authority_rejection("temporary-parent-mismatch"),
        FsError::TargetStateMismatch => authority_rejection("target-state-mismatch"),
        FsError::LockContended => authority_rejection("lock-contended"),
        FsError::DigestMismatch { .. } => authority_rejection("digest-mismatch"),
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

fn project_io_error(source: std::io::Error) -> ProjectCandidateError {
    host_io_error(FsOperation::Read, source.kind())
}

fn host_io_error(operation: FsOperation, kind: std::io::ErrorKind) -> ProjectCandidateError {
    let diagnostic = diagnostic(
        "project.manifest.host-io",
        "Project manifest host I/O failed",
    );
    let diagnostic = with_public_identifier(diagnostic, "operation", fs_operation_id(operation));
    let diagnostic = with_public_identifier(diagnostic, "io_kind", io_error_kind_id(kind));
    ProjectCandidateError::single(
        ProjectCandidateErrorKind::HostIo,
        with_sensitive(diagnostic, "manifest_source"),
    )
}

fn authority_error(
    kind: ProjectCandidateErrorKind,
    code: &'static str,
    summary: &'static str,
    operation: FsOperation,
) -> ProjectCandidateError {
    let diagnostic = with_public_identifier(
        diagnostic(code, summary),
        "operation",
        fs_operation_id(operation),
    );
    ProjectCandidateError::single(kind, with_sensitive(diagnostic, "manifest_source"))
}

fn authority_rejection(reason: &'static str) -> ProjectCandidateError {
    let diagnostic = with_public_identifier(
        diagnostic(
            "project.manifest.authority-rejected",
            "Project manifest authority was rejected",
        ),
        "reason",
        reason,
    );
    ProjectCandidateError::single(
        ProjectCandidateErrorKind::HostAuthorityRejected,
        with_sensitive(diagnostic, "manifest_source"),
    )
}

fn manifest_too_large_error(actual: usize, limit: usize) -> ProjectCandidateError {
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
    ProjectCandidateError::single(ProjectCandidateErrorKind::ManifestTooLarge, diagnostic)
}

fn capability_error(
    kind: ProjectCandidateErrorKind,
    code: &'static str,
    summary: &'static str,
    capabilities: ProductCapabilitySet,
) -> ProjectCandidateError {
    let first = capabilities
        .iter()
        .next()
        .expect("capability errors require a non-empty set");
    let diagnostic =
        with_public_identifier(diagnostic(code, summary), "capability", first.as_str());
    let diagnostic = with_public_u64(
        diagnostic,
        "count",
        u64::try_from(capabilities.len()).unwrap_or(u64::MAX),
    );
    ProjectCandidateError::single(kind, diagnostic)
}

fn diagnostic(code: &'static str, summary: &'static str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new(code).expect("project host diagnostic codes are engine-owned"),
        SafeSummary::new(summary).expect("project host summaries are engine-owned"),
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
            .expect("project host identifiers are engine-owned literals"),
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
    DiagnosticFieldKey::new(key).expect("project host field keys are engine-owned")
}

fn attach_field(diagnostic: Diagnostic, field: DiagnosticField) -> Diagnostic {
    diagnostic
        .try_with_field(field)
        .expect("project host diagnostics use unique bounded fields")
}

fn fs_operation_id(operation: FsOperation) -> &'static str {
    match operation {
        FsOperation::InspectHandle => "inspect-handle",
        FsOperation::OpenDirectory => "open-directory",
        FsOperation::OpenFile => "open-file",
        FsOperation::ReadDirectory => "read-directory",
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
