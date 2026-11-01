//! Owned file preparation against the captured source's authoring readmission.

use super::*;
use crate::project_host::file_publication::FilePublicationReadSource;

struct ReopenSource {
    source: FilePublicationReadSource,
    encoding: DocumentEncoding,
    limits: SceneFileLimits,
    authoring: AuthoringDocumentReadmission,
    observed_digest: ContentDigest,
    validated_authoring_digest: Option<ContentDigest>,
}

struct ReopenedFile {
    bytes: Vec<u8>,
    file: FileCapability,
    content_digest: ContentDigest,
}

#[derive(Clone, Copy)]
pub(in crate::project_host) enum ReopenFailure {
    Rejected(EditorPersistenceRejection),
    Failed(EditorPersistenceFailureStage),
}

impl ReopenSource {
    fn capture(file: &PersistenceFileState, authoring: AuthoringDocumentReadmission) -> Self {
        Self {
            source: file.publication.read_source(),
            encoding: file.encoding,
            limits: file.limits,
            authoring,
            observed_digest: file.observed_digest(),
            validated_authoring_digest: file.validated_authoring_digest,
        }
    }

    fn admits_source_bytes(&self, digest: ContentDigest) -> bool {
        digest == self.observed_digest || self.validated_authoring_digest == Some(digest)
    }

    fn read(&self) -> Result<ReopenedFile, ReopenFailure> {
        let file = self.source.open().map_err(|error| match error {
            FsError::Io { ref source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
                ReopenFailure::Rejected(EditorPersistenceRejection::TargetDeleted)
            }
            FsError::Unsupported { .. } | FsError::Unproven { .. } => {
                ReopenFailure::Rejected(EditorPersistenceRejection::UnsupportedGuarantee)
            }
            _ => ReopenFailure::Failed(EditorPersistenceFailureStage::OpenTarget),
        })?;
        let bytes = file
            .read_to_end_bounded(file_byte_limit(self.limits))
            .map_err(|_| ReopenFailure::Failed(EditorPersistenceFailureStage::ReadTarget))?;
        let content_digest = ContentDigest::of_bytes(&bytes);
        Ok(ReopenedFile {
            bytes,
            file,
            content_digest,
        })
    }
}

pub(in crate::project_host) struct SceneReopenRequest(ReopenSource);
pub(in crate::project_host) struct PrefabReopenRequest(ReopenSource);

pub(in crate::project_host) struct PreparedSceneReopen {
    result: Result<(SceneAuthoringSession, FileCapability, ContentDigest), ReopenFailure>,
}

pub(in crate::project_host) struct PreparedPrefabReopen {
    result: Result<(PrefabAuthoringSession, FileCapability, ContentDigest), ReopenFailure>,
}

impl SceneReopenRequest {
    pub(in crate::project_host) fn prepare(self) -> PreparedSceneReopen {
        PreparedSceneReopen {
            result: self.prepare_inner(),
        }
    }

    fn prepare_inner(
        self,
    ) -> Result<(SceneAuthoringSession, FileCapability, ContentDigest), ReopenFailure> {
        let source = self.0;
        let reopened = source.read()?;
        let admitted_source = source.admits_source_bytes(reopened.content_digest);
        let candidate = decode_scene_candidate(source.encoding, &reopened.bytes, source.limits)
            .map_err(ReopenFailure::Failed)?;
        let session = source
            .authoring
            .reopen_scene_candidate(candidate)
            .map_err(|_| ReopenFailure::Failed(EditorPersistenceFailureStage::Validate))?;
        // Explicit Reopen can discard unsaved topology in favor of already admitted
        // source bytes. Unobserved external changes still require exact preservation.
        if !admitted_source && !source.authoring.preserves_scene(&session) {
            return Err(ReopenFailure::Rejected(
                EditorPersistenceRejection::PreservationMismatch,
            ));
        }
        Ok((session, reopened.file, reopened.content_digest))
    }
}

impl PreparedSceneReopen {
    /// Observes pure preparation without invoking native codecs or migrations.
    pub(in crate::project_host) fn preparation_identity(
        &self,
    ) -> Result<FileIdentity, ReopenFailure> {
        match &self.result {
            Ok((_, file, _)) => Ok(file.identity()),
            Err(failure) => Err(*failure),
        }
    }

    pub(in crate::project_host) fn admit(self) -> SceneReopenOutcome {
        let (session, file, content_digest) = match self.result {
            Ok(result) => result,
            Err(ReopenFailure::Rejected(reason)) => return SceneReopenOutcome::Rejected(reason),
            Err(ReopenFailure::Failed(stage)) => return SceneReopenOutcome::Failed(stage),
        };
        SceneReopenOutcome::Opened(Box::new(OpenedSceneReopen {
            session,
            digest: editor_digest(content_digest),
            file,
            content_digest,
        }))
    }
}

impl PrefabReopenRequest {
    pub(in crate::project_host) fn prepare(self) -> PreparedPrefabReopen {
        PreparedPrefabReopen {
            result: self.prepare_inner(),
        }
    }

    fn prepare_inner(
        self,
    ) -> Result<(PrefabAuthoringSession, FileCapability, ContentDigest), ReopenFailure> {
        let source = self.0;
        let reopened = source.read()?;
        let admitted_source = source.admits_source_bytes(reopened.content_digest);
        let candidate = decode_prefab_candidate(source.encoding, &reopened.bytes, source.limits)
            .map_err(ReopenFailure::Failed)?;
        let session = source
            .authoring
            .reopen_prefab_candidate(candidate)
            .map_err(|_| ReopenFailure::Failed(EditorPersistenceFailureStage::Validate))?;
        if !admitted_source && !source.authoring.preserves_prefab(&session) {
            return Err(ReopenFailure::Rejected(
                EditorPersistenceRejection::PreservationMismatch,
            ));
        }
        Ok((session, reopened.file, reopened.content_digest))
    }
}

impl PreparedPrefabReopen {
    /// Observes pure preparation without invoking native codecs or migrations.
    pub(in crate::project_host) fn preparation_identity(
        &self,
    ) -> Result<FileIdentity, ReopenFailure> {
        match &self.result {
            Ok((_, file, _)) => Ok(file.identity()),
            Err(failure) => Err(*failure),
        }
    }

    pub(in crate::project_host) fn admit(self) -> PrefabReopenOutcome {
        let (session, file, content_digest) = match self.result {
            Ok(result) => result,
            Err(ReopenFailure::Rejected(reason)) => return PrefabReopenOutcome::Rejected(reason),
            Err(ReopenFailure::Failed(stage)) => return PrefabReopenOutcome::Failed(stage),
        };
        PrefabReopenOutcome::Opened(Box::new(OpenedPrefabReopen {
            session,
            digest: editor_digest(content_digest),
            file,
            content_digest,
        }))
    }
}

impl ScenePersistenceHost {
    pub(in crate::project_host) fn prepare_reopen(
        &self,
        authoring: Option<AuthoringDocumentReadmission>,
    ) -> Result<SceneReopenRequest, EditorPersistenceRejection> {
        let authoring = authoring.ok_or(EditorPersistenceRejection::PreservationMismatch)?;
        Ok(SceneReopenRequest(ReopenSource::capture(
            &self.file, authoring,
        )))
    }
}

impl PrefabPersistenceHost {
    pub(in crate::project_host) fn prepare_reopen(
        &self,
        authoring: Option<AuthoringDocumentReadmission>,
    ) -> Result<PrefabReopenRequest, EditorPersistenceRejection> {
        let authoring = authoring.ok_or(EditorPersistenceRejection::PreservationMismatch)?;
        Ok(PrefabReopenRequest(ReopenSource::capture(
            &self.file, authoring,
        )))
    }
}
