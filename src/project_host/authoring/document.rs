use std::{error::Error, fmt, sync::Arc};

use nara_asset::{AssetPath, AssetPathError, AssetRef};
use nara_fs::{ContentDigest, FileIdentity, FsError, PathValidationError, RelativePath};
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
    adapters: &'basis [AuthoringAdapterReceipt],
    limits: SceneFileLimits,
}

impl<'basis> AuthoringDocumentAdmissionBasis<'basis> {
    pub(super) const fn new(
        schema: &'basis AuthoringSchemaReceipt,
        adapters: &'basis [AuthoringAdapterReceipt],
        limits: SceneFileLimits,
    ) -> Self {
        Self {
            schema,
            adapters,
            limits,
        }
    }

    pub(super) fn admit(
        &self,
        index: &ProjectSourceIndexSnapshot,
        relative_path: &str,
        expected_kind: AuthoringDocumentKind,
    ) -> Result<AdmittedAuthoringDocument, AuthoringDocumentAdmissionError> {
        self.verify_index_lineage(index)?;
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
        let relative = RelativePath::new(relative_path)
            .map_err(AuthoringDocumentAdmissionError::RelativePath)?;
        let capability = self
            .schema
            .project_root()
            .open_file(&relative)
            .map_err(AuthoringDocumentAdmissionError::OpenSource)?;
        if capability.identity() != expected_identity {
            return Err(AuthoringDocumentAdmissionError::SourceIdentityChanged);
        }
        let bytes = capability
            .read_to_end_bounded(
                u64::try_from(self.limits.encoded_bytes().get()).unwrap_or(u64::MAX),
            )
            .map_err(|error| match error {
                FsError::ByteLimitExceeded { .. } => {
                    AuthoringDocumentAdmissionError::SourceTooLarge
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
                document: admit_scene_bytes(
                    &path,
                    &bytes,
                    self.limits,
                    self.schema.bundle(),
                    self.adapters,
                )
                .map_err(AuthoringDocumentAdmissionError::Document)?,
            }),
            AuthoringDocumentKind::Prefab => Ok(AdmittedAuthoringDocument::Prefab {
                source,
                document: admit_prefab_bytes(
                    &path,
                    &bytes,
                    self.limits,
                    self.schema.bundle(),
                    self.adapters,
                )
                .map_err(AuthoringDocumentAdmissionError::Document)?,
            }),
        }
    }

    fn verify_index_lineage(
        &self,
        index: &ProjectSourceIndexSnapshot,
    ) -> Result<(), AuthoringDocumentAdmissionError> {
        if index.root_identity() != self.schema.root_identity()
            || index.manifest_identity() != self.schema.manifest_identity()
            || index.manifest_digest() != self.schema.manifest_digest()
            || index.bundle_identity() != self.schema.bundle_identity()
            || index.bundle_digest() != self.schema.bundle_digest()
        {
            return Err(AuthoringDocumentAdmissionError::IndexLineageMismatch);
        }
        Ok(())
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
    SourceTooLarge,
    ReadSource(FsError),
    Document(AuthoringDocumentDataError),
}

impl AuthoringDocumentAdmissionError {
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
            Self::SourceTooLarge => "project.editor.document-source-too-large",
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
