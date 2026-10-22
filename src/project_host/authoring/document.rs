use std::{error::Error, fmt, sync::Arc};

use nara_asset::{AssetPath, AssetPathError, AssetRef};
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, PublicDiagnosticIdentifier,
    SafeSummary,
};
use nara_fs::{
    ContentDigest, DirectoryCapability, FileIdentity, FsError, PathValidationError, RelativePath,
};
use nara_project::{ProjectPath, ProjectPathError};
use nara_reflect::AuthoringSchemaBundle;
use nara_scene::__private::AuthoringAdapterReceipt;
use nara_scene::{
    AuthoringDocumentPublicationError, AuthoringDocumentSource, PrefabDocumentCandidate,
    PublishedAuthoringPrefabDocument, PublishedAuthoringSceneDocument, SceneDocumentCandidate,
    SceneFileLimits, SceneFormatError,
};

use super::{
    AuthoringSchemaReceipt, ProjectSourceIndexSnapshot, source_index::ProjectSourceOpenCapability,
};

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthoringDocumentKind {
    Scene,
    Prefab,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthoringDocumentSourceFact {
    relative_path: Arc<str>,
    identity: FileIdentity,
    parent_identity: Option<FileIdentity>,
    digest: ContentDigest,
}

impl AuthoringDocumentSourceFact {
    #[must_use]
    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
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
}

pub(crate) enum AdmittedAuthoringDocument {
    Scene {
        source: AuthoringDocumentSourceFact,
        document: PublishedAuthoringSceneDocument,
    },
    Prefab {
        source: AuthoringDocumentSourceFact,
        document: PublishedAuthoringPrefabDocument,
    },
}

impl AdmittedAuthoringDocument {
    #[must_use]
    pub(crate) const fn kind(&self) -> AuthoringDocumentKind {
        match self {
            Self::Scene { .. } => AuthoringDocumentKind::Scene,
            Self::Prefab { .. } => AuthoringDocumentKind::Prefab,
        }
    }

    #[must_use]
    pub(crate) const fn source(&self) -> &AuthoringDocumentSourceFact {
        match self {
            Self::Scene { source, .. } | Self::Prefab { source, .. } => source,
        }
    }
}

pub(super) struct AuthoringDocumentAdmissionBasis<'basis> {
    schema: &'basis AuthoringSchemaReceipt,
    adapters: &'basis Arc<[AuthoringAdapterReceipt]>,
    limits: SceneFileLimits,
}

impl<'basis> AuthoringDocumentAdmissionBasis<'basis> {
    pub(super) const fn new(
        schema: &'basis AuthoringSchemaReceipt,
        adapters: &'basis Arc<[AuthoringAdapterReceipt]>,
        limits: SceneFileLimits,
    ) -> Self {
        Self {
            schema,
            adapters,
            limits,
        }
    }

    #[cfg(test)]
    pub(super) fn admit(
        &self,
        index: &ProjectSourceIndexSnapshot,
        relative_path: &str,
        expected_kind: AuthoringDocumentKind,
    ) -> Result<AdmittedAuthoringDocument, AuthoringDocumentAdmissionError> {
        DocumentIndexLineage::capture(self.schema).verify(index)?;
        admit_indexed_document(
            &self.schema.project_root(),
            index,
            relative_path,
            expected_kind,
            self.limits,
            self.schema.bundle(),
            self.adapters,
        )
    }

    /// Captures immutable admission authority without opening or decoding the source.
    pub(super) fn capture(
        &self,
        index: Arc<ProjectSourceIndexSnapshot>,
        relative_path: Arc<str>,
        expected_kind: AuthoringDocumentKind,
    ) -> Result<OwnedAuthoringDocumentRequest, AuthoringDocumentAdmissionError> {
        let lineage = DocumentIndexLineage::capture(self.schema);
        lineage.verify(&index)?;
        indexed_document_source(&index, &relative_path, expected_kind)?;
        Ok(OwnedAuthoringDocumentRequest {
            root: self.schema.project_root(),
            index,
            relative_path,
            expected_kind,
            bundle: self.schema.bundle.shared_bundle(),
            adapters: Arc::clone(self.adapters),
            limits: self.limits,
            lineage,
        })
    }
}

/// Finite, data-only preparation input. This grants no Workspace publication authority.
pub(crate) struct OwnedAuthoringDocumentRequest {
    root: DirectoryCapability,
    index: Arc<ProjectSourceIndexSnapshot>,
    relative_path: Arc<str>,
    expected_kind: AuthoringDocumentKind,
    bundle: Arc<AuthoringSchemaBundle>,
    adapters: Arc<[AuthoringAdapterReceipt]>,
    limits: SceneFileLimits,
    lineage: DocumentIndexLineage,
}

impl OwnedAuthoringDocumentRequest {
    pub(crate) fn project_root(&self) -> DirectoryCapability {
        self.root.share()
    }

    pub(crate) fn prepare(
        self,
    ) -> Result<AdmittedAuthoringDocument, AuthoringDocumentAdmissionError> {
        self.lineage.verify(&self.index)?;
        admit_indexed_document(
            &self.root,
            &self.index,
            &self.relative_path,
            self.expected_kind,
            self.limits,
            &self.bundle,
            &self.adapters,
        )
    }
}

struct DocumentIndexLineage {
    root_identity: FileIdentity,
    manifest_identity: FileIdentity,
    manifest_digest: ContentDigest,
    bundle_identity: FileIdentity,
    bundle_digest: ContentDigest,
}

impl DocumentIndexLineage {
    fn capture(schema: &AuthoringSchemaReceipt) -> Self {
        Self {
            root_identity: schema.root_identity(),
            manifest_identity: schema.manifest_identity(),
            manifest_digest: schema.manifest_digest(),
            bundle_identity: schema.bundle_identity(),
            bundle_digest: schema.bundle_digest(),
        }
    }

    fn verify(
        &self,
        index: &ProjectSourceIndexSnapshot,
    ) -> Result<(), AuthoringDocumentAdmissionError> {
        if index.root_identity() != self.root_identity
            || index.manifest_identity() != self.manifest_identity
            || index.manifest_digest() != self.manifest_digest
            || index.bundle_identity() != self.bundle_identity
            || index.bundle_digest() != self.bundle_digest
        {
            return Err(AuthoringDocumentAdmissionError::IndexLineageMismatch);
        }
        Ok(())
    }
}

fn indexed_document_source(
    index: &ProjectSourceIndexSnapshot,
    relative_path: &str,
    expected_kind: AuthoringDocumentKind,
) -> Result<(ProjectPath, RelativePath, FileIdentity), AuthoringDocumentAdmissionError> {
    let entry = index
        .entry(relative_path)
        .ok_or(AuthoringDocumentAdmissionError::MissingIndexMember)?;
    let expected_identity = entry
        .scoped_identity()
        .ok_or(AuthoringDocumentAdmissionError::IdentityUnavailable)?;
    let open_capability = match expected_kind {
        AuthoringDocumentKind::Scene => ProjectSourceOpenCapability::Scene,
        AuthoringDocumentKind::Prefab => ProjectSourceOpenCapability::Prefab,
    };
    if entry.open_capability() != open_capability {
        return Err(AuthoringDocumentAdmissionError::DocumentKindMismatch);
    }
    let path = ProjectPath::new(relative_path.to_owned())
        .map_err(AuthoringDocumentAdmissionError::ProjectPath)?;
    let relative =
        RelativePath::new(relative_path).map_err(AuthoringDocumentAdmissionError::RelativePath)?;
    Ok((path, relative, expected_identity))
}

fn admit_indexed_document(
    root: &DirectoryCapability,
    index: &ProjectSourceIndexSnapshot,
    relative_path: &str,
    expected_kind: AuthoringDocumentKind,
    limits: SceneFileLimits,
    bundle: &AuthoringSchemaBundle,
    adapters: &[AuthoringAdapterReceipt],
) -> Result<AdmittedAuthoringDocument, AuthoringDocumentAdmissionError> {
    let (path, relative, expected_identity) =
        indexed_document_source(index, relative_path, expected_kind)?;
    let capability = root
        .open_file(&relative)
        .map_err(AuthoringDocumentAdmissionError::OpenSource)?;
    if capability.identity() != expected_identity {
        return Err(AuthoringDocumentAdmissionError::SourceIdentityChanged);
    }
    let bytes = capability
        .read_to_end_bounded(u64::try_from(limits.encoded_bytes().get()).unwrap_or(u64::MAX))
        .map_err(|error| match error {
            FsError::ByteLimitExceeded { limit } => {
                AuthoringDocumentAdmissionError::SourceTooLarge { maximum: limit }
            }
            error => AuthoringDocumentAdmissionError::ReadSource(error),
        })?;
    let source = AuthoringDocumentSourceFact {
        relative_path: Arc::from(relative_path),
        identity: capability.identity(),
        parent_identity: capability.parent_identity(),
        digest: ContentDigest::of_bytes(&bytes),
    };
    match expected_kind {
        AuthoringDocumentKind::Scene => Ok(AdmittedAuthoringDocument::Scene {
            source,
            document: admit_scene_bytes(&path, &bytes, limits, bundle, adapters)
                .map_err(AuthoringDocumentAdmissionError::Document)?,
        }),
        AuthoringDocumentKind::Prefab => Ok(AdmittedAuthoringDocument::Prefab {
            source,
            document: admit_prefab_bytes(&path, &bytes, limits, bundle, adapters)
                .map_err(AuthoringDocumentAdmissionError::Document)?,
        }),
    }
}

pub(super) fn admit_scene_bytes(
    path: &ProjectPath,
    bytes: &[u8],
    limits: SceneFileLimits,
    bundle: &AuthoringSchemaBundle,
    adapters: &[AuthoringAdapterReceipt],
) -> Result<PublishedAuthoringSceneDocument, AuthoringDocumentDataError> {
    let candidate = if path.as_str().ends_with(".ron") {
        SceneDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
    } else if path.as_str().ends_with(".json") {
        SceneDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
    } else {
        return Err(AuthoringDocumentDataError::UnsupportedEncoding);
    }
    .map_err(AuthoringDocumentDataError::Format)?;
    Ok(candidate
        .admit_authoring(document_source(path)?, bundle, adapters)
        .map_err(AuthoringDocumentDataError::Publication)?
        .publish())
}

pub(super) fn admit_prefab_bytes(
    path: &ProjectPath,
    bytes: &[u8],
    limits: SceneFileLimits,
    bundle: &AuthoringSchemaBundle,
    adapters: &[AuthoringAdapterReceipt],
) -> Result<PublishedAuthoringPrefabDocument, AuthoringDocumentDataError> {
    let candidate = if path.as_str().ends_with(".ron") {
        PrefabDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
    } else if path.as_str().ends_with(".json") {
        PrefabDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
    } else {
        return Err(AuthoringDocumentDataError::UnsupportedEncoding);
    }
    .map_err(AuthoringDocumentDataError::Format)?;
    Ok(candidate
        .admit_authoring(document_source(path)?, bundle, adapters)
        .map_err(AuthoringDocumentDataError::Publication)?
        .publish())
}

fn document_source(
    path: &ProjectPath,
) -> Result<AuthoringDocumentSource, AuthoringDocumentDataError> {
    let path =
        AssetPath::new(path.as_str().to_owned()).map_err(AuthoringDocumentDataError::Source)?;
    Ok(AuthoringDocumentSource::new(AssetRef::Path(path)))
}

#[derive(Debug)]
pub(crate) enum AuthoringDocumentDataError {
    UnsupportedEncoding,
    Format(SceneFormatError),
    Source(AssetPathError),
    Publication(AuthoringDocumentPublicationError),
}

#[derive(Debug)]
pub(crate) enum AuthoringDocumentAdmissionError {
    IndexLineageMismatch,
    MissingIndexMember,
    IdentityUnavailable,
    DocumentKindMismatch,
    ProjectPath(ProjectPathError),
    RelativePath(PathValidationError),
    OpenSource(FsError),
    SourceIdentityChanged,
    SourceTooLarge { maximum: u64 },
    ReadSource(FsError),
    Document(AuthoringDocumentDataError),
}

impl AuthoringDocumentAdmissionError {
    pub(crate) fn diagnostic(&self) -> Diagnostic {
        if let Self::Document(AuthoringDocumentDataError::Format(error)) = self {
            let summary = match error {
                SceneFormatError::LegacyPlacementShape {
                    entity_index: Some(_),
                } => {
                    "Rewrite the legacy document with ordered roots and parent-owned children; remove record-owned parent fields because sibling order cannot be inferred"
                }
                SceneFormatError::LegacyPlacementShape { entity_index: None } => {
                    "Rewrite the legacy document with ordered roots and parent-owned children; add the missing ordered roots because sibling order cannot be inferred"
                }
                _ => "Editor document source format is invalid",
            };
            return crate::project_content::format_diagnostic::scene_format_diagnostic(
                self.code(),
                summary,
                error,
            );
        }
        let diagnostic = Diagnostic::error(
            DiagnosticCode::new(self.code()).expect("document error codes are engine-owned"),
            SafeSummary::new("Editor document source could not be admitted")
                .expect("document error summaries are engine-owned"),
        );
        if let Self::SourceTooLarge { maximum } = self {
            return diagnostic
                .try_with_field(DiagnosticField::public_identifier(
                    DiagnosticFieldKey::new("reason").expect("field key is engine-owned"),
                    PublicDiagnosticIdentifier::new("budget-exceeded")
                        .expect("reason is engine-owned"),
                ))
                .and_then(|diagnostic| {
                    diagnostic.try_with_field(DiagnosticField::public_identifier(
                        DiagnosticFieldKey::new("budget").expect("field key is engine-owned"),
                        PublicDiagnosticIdentifier::new("encoded-bytes")
                            .expect("budget is engine-owned"),
                    ))
                })
                .and_then(|diagnostic| {
                    diagnostic.try_with_field(DiagnosticField::public_u64(
                        DiagnosticFieldKey::new("maximum").expect("field key is engine-owned"),
                        *maximum,
                    ))
                })
                .expect("document diagnostics have unique bounded fields");
        }
        diagnostic
    }

    #[must_use]
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::IndexLineageMismatch => "project.editor.document-index-lineage-mismatch",
            Self::MissingIndexMember => "project.editor.document-index-member-missing",
            Self::IdentityUnavailable => "project.editor.document-identity-unavailable",
            Self::DocumentKindMismatch => "project.editor.document-kind-mismatch",
            Self::ProjectPath(_) | Self::RelativePath(_) => {
                "project.editor.document-source-path-invalid"
            }
            Self::OpenSource(_) => "project.editor.document-source-open-failed",
            Self::SourceIdentityChanged => "project.editor.document-source-identity-changed",
            Self::SourceTooLarge { .. } => "project.editor.document-source-too-large",
            Self::ReadSource(_) => "project.editor.document-source-read-failed",
            Self::Document(AuthoringDocumentDataError::UnsupportedEncoding) => {
                "project.editor.document-source-encoding-unsupported"
            }
            Self::Document(AuthoringDocumentDataError::Format(_)) => {
                "project.editor.document-source-format-invalid"
            }
            Self::Document(AuthoringDocumentDataError::Source(_)) => {
                "project.editor.document-source-identity-invalid"
            }
            Self::Document(AuthoringDocumentDataError::Publication(_)) => {
                "project.editor.document-authoring-publication-rejected"
            }
        }
    }
}

impl fmt::Display for AuthoringDocumentAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for AuthoringDocumentAdmissionError {}
