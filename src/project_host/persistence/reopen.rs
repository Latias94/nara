//! Owned file preparation; native registry behavior is admitted only by the document owner.

use super::*;
use crate::project_host::file_publication::FilePublicationReadSource;

struct ReopenSource {
    source: FilePublicationReadSource,
    encoding: DocumentEncoding,
    limits: SceneFileLimits,
    authoring: Option<AuthoringDocumentReadmission>,
}

struct ReopenedFile {
    bytes: Vec<u8>,
    identity: FileIdentity,
    content_digest: ContentDigest,
}

enum ReopenFailure {
    Rejected(EditorPersistenceRejection),
    Failed(EditorPersistenceFailureStage),
}

impl ReopenSource {
    fn capture(
        file: &PersistenceFileState,
        lane: PersistenceDocumentLane,
        authoring: Option<AuthoringDocumentReadmission>,
    ) -> Result<Self, EditorPersistenceRejection> {
        if (lane == PersistenceDocumentLane::Authoring) != authoring.is_some() {
            return Err(EditorPersistenceRejection::PreservationMismatch);
        }
        Ok(Self {
            source: file.publication.read_source(),
            encoding: file.encoding,
            limits: file.limits,
            authoring,
        })
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
            identity: file.identity(),
            content_digest,
        })
    }
}

pub(in crate::project_host) struct SceneReopenRequest(ReopenSource);
pub(in crate::project_host) struct PrefabReopenRequest(ReopenSource);

enum SceneAdmission {
    Native(SceneDocumentCandidate),
    Authoring(SceneAuthoringSession),
}

enum PrefabAdmission {
    Native(PrefabDocumentCandidate),
    Authoring(PrefabAuthoringSession),
}

pub(in crate::project_host) struct PreparedSceneReopen {
    result: Result<(SceneAdmission, FileIdentity, ContentDigest), ReopenFailure>,
}

pub(in crate::project_host) struct PreparedPrefabReopen {
    result: Result<(PrefabAdmission, FileIdentity, ContentDigest), ReopenFailure>,
}

impl SceneReopenRequest {
    pub(in crate::project_host) fn prepare(self) -> PreparedSceneReopen {
        PreparedSceneReopen {
            result: self.prepare_inner(),
        }
    }

    fn prepare_inner(self) -> Result<(SceneAdmission, FileIdentity, ContentDigest), ReopenFailure> {
        let reopened = self.0.read()?;
        let candidate = match self.0.encoding {
            DocumentEncoding::Json => SceneDocumentCandidate::decode_json_bytes_with_limits(
                &reopened.bytes,
                self.0.limits,
            ),
            DocumentEncoding::Ron => {
                SceneDocumentCandidate::decode_ron_bytes_with_limits(&reopened.bytes, self.0.limits)
            }
        }
        .map_err(|_| ReopenFailure::Failed(EditorPersistenceFailureStage::Decode))?;
        let admission = match self.0.authoring {
            Some(authoring) => {
                let session = authoring
                    .reopen_scene_candidate(candidate)
                    .map_err(|_| ReopenFailure::Failed(EditorPersistenceFailureStage::Validate))?;
                if !authoring.preserves_scene(&session) {
                    return Err(ReopenFailure::Rejected(
                        EditorPersistenceRejection::PreservationMismatch,
                    ));
                }
                SceneAdmission::Authoring(session)
            }
            None => SceneAdmission::Native(candidate),
        };
        Ok((admission, reopened.identity, reopened.content_digest))
    }
}

impl PreparedSceneReopen {
    pub(in crate::project_host) fn admit(
        self,
        registry: Option<&ComponentRegistry>,
    ) -> SceneReopenOutcome {
        let (admission, identity, content_digest) = match self.result {
            Ok(result) => result,
            Err(ReopenFailure::Rejected(reason)) => return SceneReopenOutcome::Rejected(reason),
            Err(ReopenFailure::Failed(stage)) => return SceneReopenOutcome::Failed(stage),
        };
        let session = match admission {
            SceneAdmission::Authoring(session) => session,
            SceneAdmission::Native(candidate) => {
                let Some(registry) = registry else {
                    return SceneReopenOutcome::Rejected(
                        EditorPersistenceRejection::PreservationMismatch,
                    );
                };
                match SceneAuthoringSession::try_from_file_candidate(candidate, registry) {
                    Ok(session) => session,
                    Err(_) => {
                        return SceneReopenOutcome::Failed(EditorPersistenceFailureStage::Validate);
                    }
                }
            }
        };
        SceneReopenOutcome::Opened(Box::new(OpenedSceneReopen {
            session,
            digest: editor_digest(content_digest),
            identity,
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
    ) -> Result<(PrefabAdmission, FileIdentity, ContentDigest), ReopenFailure> {
        let reopened = self.0.read()?;
        let candidate = match self.0.encoding {
            DocumentEncoding::Json => PrefabDocumentCandidate::decode_json_bytes_with_limits(
                &reopened.bytes,
                self.0.limits,
            ),
            DocumentEncoding::Ron => PrefabDocumentCandidate::decode_ron_bytes_with_limits(
                &reopened.bytes,
                self.0.limits,
            ),
        }
        .map_err(|_| ReopenFailure::Failed(EditorPersistenceFailureStage::Decode))?;
        let admission = match self.0.authoring {
            Some(authoring) => {
                let session = authoring
                    .reopen_prefab_candidate(candidate)
                    .map_err(|_| ReopenFailure::Failed(EditorPersistenceFailureStage::Validate))?;
                if !authoring.preserves_prefab(&session) {
                    return Err(ReopenFailure::Rejected(
                        EditorPersistenceRejection::PreservationMismatch,
                    ));
                }
                PrefabAdmission::Authoring(session)
            }
            None => PrefabAdmission::Native(candidate),
        };
        Ok((admission, reopened.identity, reopened.content_digest))
    }
}

impl PreparedPrefabReopen {
    pub(in crate::project_host) fn admit(
        self,
        registry: Option<&ComponentRegistry>,
    ) -> PrefabReopenOutcome {
        let (admission, identity, content_digest) = match self.result {
            Ok(result) => result,
            Err(ReopenFailure::Rejected(reason)) => return PrefabReopenOutcome::Rejected(reason),
            Err(ReopenFailure::Failed(stage)) => return PrefabReopenOutcome::Failed(stage),
        };
        let session = match admission {
            PrefabAdmission::Authoring(session) => session,
            PrefabAdmission::Native(candidate) => {
                let Some(registry) = registry else {
                    return PrefabReopenOutcome::Rejected(
                        EditorPersistenceRejection::PreservationMismatch,
                    );
                };
                match PrefabAuthoringSession::try_from_file_candidate(candidate, registry) {
                    Ok(session) => session,
                    Err(_) => {
                        return PrefabReopenOutcome::Failed(
                            EditorPersistenceFailureStage::Validate,
                        );
                    }
                }
            }
        };
        PrefabReopenOutcome::Opened(Box::new(OpenedPrefabReopen {
            session,
            digest: editor_digest(content_digest),
            identity,
            content_digest,
        }))
    }
}

impl ScenePersistenceHost {
    pub(in crate::project_host) fn prepare_reopen(
        &self,
        authoring: Option<AuthoringDocumentReadmission>,
    ) -> Result<SceneReopenRequest, EditorPersistenceRejection> {
        ReopenSource::capture(&self.file, self.lane, authoring).map(SceneReopenRequest)
    }
}

impl PrefabPersistenceHost {
    pub(in crate::project_host) fn prepare_reopen(
        &self,
        authoring: Option<AuthoringDocumentReadmission>,
    ) -> Result<PrefabReopenRequest, EditorPersistenceRejection> {
        ReopenSource::capture(&self.file, self.lane, authoring).map(PrefabReopenRequest)
    }
}
