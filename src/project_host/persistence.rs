use std::{
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_fs::{
    ContentDigest, DirectoryCapability, DurabilityProgress, ExpectedTarget, FileIdentity, FsError,
    LockMode, PublicationAtomicity, PublicationIdentityEvidence, RelativeComponent, RelativePath,
    ReplaceReceipt, ReplaceSourceBinding, StageStatus,
};
use nara_reflect::ComponentRegistry;
use nara_scene::{
    SceneAuthoringRevision, SceneAuthoringSession, SceneDocument, SceneDocumentCandidate,
    SceneFileLimits,
};
use nara_tooling::{
    EditorDocumentDigest, EditorDocumentId, EditorPersistenceCheckpoint,
    EditorPersistenceFailureStage, EditorPersistenceRejection,
};

static NEXT_TEMPORARY_NAME: AtomicU64 = AtomicU64::new(1);
const TEMPORARY_NAME_ATTEMPTS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneEncoding {
    Json,
    Ron,
}

pub(super) struct ScenePersistenceHost {
    parent: DirectoryCapability,
    target: RelativeComponent,
    encoding: SceneEncoding,
    limits: SceneFileLimits,
    observed_identity: FileIdentity,
    observed_digest: ContentDigest,
    uncertain: bool,
}

pub(super) struct OpenedScenePersistence {
    pub host: ScenePersistenceHost,
    pub session: SceneAuthoringSession,
    pub digest: EditorDocumentDigest,
}

pub(super) struct SceneSaveCandidate {
    pub document: EditorDocumentId,
    pub revision: SceneAuthoringRevision,
    pub scene: SceneDocument,
}

pub(super) enum SceneSaveOutcome {
    Saved(EditorPersistenceReceipt),
    Rejected(EditorPersistenceRejection),
    Failed(EditorPersistenceFailureStage),
    PersistenceUncertain {
        checkpoint: EditorPersistenceCheckpoint,
        evidence: EditorPersistenceReceipt,
    },
}

pub(super) enum SceneReopenOutcome {
    Opened {
        session: SceneAuthoringSession,
        digest: EditorDocumentDigest,
    },
    Rejected(EditorPersistenceRejection),
    Failed(EditorPersistenceFailureStage),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorPersistenceReceipt {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    digest: EditorDocumentDigest,
    previous_identity: Option<FileIdentity>,
    candidate_identity: FileIdentity,
    published_identity: Option<FileIdentity>,
    identity_evidence: PublicationIdentityEvidence,
    publication: PublicationAtomicity,
    durability: DurabilityProgress,
    parent_directory_sync: StageStatus,
}

impl EditorPersistenceReceipt {
    #[must_use]
    pub const fn document(self) -> EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn revision(self) -> SceneAuthoringRevision {
        self.revision
    }

    #[must_use]
    pub const fn digest(self) -> EditorDocumentDigest {
        self.digest
    }

    #[must_use]
    pub const fn previous_identity(self) -> Option<FileIdentity> {
        self.previous_identity
    }

    #[must_use]
    pub const fn candidate_identity(self) -> FileIdentity {
        self.candidate_identity
    }

    #[must_use]
    pub const fn published_identity(self) -> Option<FileIdentity> {
        self.published_identity
    }

    #[must_use]
    pub const fn publication_identity_evidence(self) -> PublicationIdentityEvidence {
        self.identity_evidence
    }

    #[must_use]
    pub const fn publication_atomicity(self) -> PublicationAtomicity {
        self.publication
    }

    #[must_use]
    pub const fn durability(self) -> DurabilityProgress {
        self.durability
    }

    #[must_use]
    pub const fn parent_directory_sync(self) -> StageStatus {
        self.parent_directory_sync
    }

    #[must_use]
    pub const fn checkpoint(self) -> EditorPersistenceCheckpoint {
        EditorPersistenceCheckpoint {
            document: self.document,
            revision: self.revision,
            digest: self.digest,
        }
    }
}

impl ScenePersistenceHost {
    pub(super) fn open(
        scenes: &DirectoryCapability,
        scene_path: &str,
        registry: &ComponentRegistry,
    ) -> Result<OpenedScenePersistence, EditorPersistenceFailureStage> {
        let path =
            RelativePath::new(scene_path).map_err(|_| EditorPersistenceFailureStage::OpenTarget)?;
        let (parent, target) = scenes
            .resolve_file_parent(&path)
            .map_err(|_| EditorPersistenceFailureStage::OpenTarget)?;
        let encoding = scene_encoding(scene_path).ok_or(EditorPersistenceFailureStage::Decode)?;
        let limits = SceneFileLimits::default();
        let file = parent
            .open_child_file(&target)
            .map_err(|_| EditorPersistenceFailureStage::OpenTarget)?;
        let identity = file.identity();
        let bytes = file
            .read_to_end_bounded(file_byte_limit(limits))
            .map_err(|_| EditorPersistenceFailureStage::ReadTarget)?;
        let digest = ContentDigest::of_bytes(&bytes);
        let session = decode_session(encoding, &bytes, limits, registry)?;
        Ok(OpenedScenePersistence {
            host: Self {
                parent,
                target,
                encoding,
                limits,
                observed_identity: identity,
                observed_digest: digest,
                uncertain: false,
            },
            session,
            digest: editor_digest(digest),
        })
    }

    pub(super) fn save(&mut self, candidate: SceneSaveCandidate) -> SceneSaveOutcome {
        if self.uncertain {
            return SceneSaveOutcome::Rejected(EditorPersistenceRejection::RequiresReconcile);
        }
        let bytes = match encode_scene(self.encoding, &candidate.scene) {
            Ok(bytes) => bytes,
            Err(stage) => return SceneSaveOutcome::Failed(stage),
        };
        if bytes.len() > self.limits.encoded_bytes().get() {
            return SceneSaveOutcome::Failed(EditorPersistenceFailureStage::Encode);
        }
        let digest = ContentDigest::of_bytes(&bytes);
        let checkpoint = EditorPersistenceCheckpoint {
            document: candidate.document,
            revision: candidate.revision,
            digest: editor_digest(digest),
        };

        let matrix = nara_fs::platform_capability_matrix();
        if matrix.publication_atomicity() != PublicationAtomicity::AtomicNameSwitch
            || matrix.replace_source_binding() == ReplaceSourceBinding::Unsupported
        {
            return SceneSaveOutcome::Rejected(EditorPersistenceRejection::UnsupportedGuarantee);
        }

        let target = match self.parent.open_child_file(&self.target) {
            Ok(target) => target,
            Err(error) => return open_target_failure(error),
        };
        if target.identity() != self.observed_identity {
            return SceneSaveOutcome::Rejected(EditorPersistenceRejection::TargetChanged);
        }
        let lock = match target.try_lock(LockMode::Exclusive) {
            Ok(lock) => lock,
            Err(FsError::LockContended) => {
                return SceneSaveOutcome::Rejected(EditorPersistenceRejection::LockUnavailable);
            }
            Err(_) => return SceneSaveOutcome::Failed(EditorPersistenceFailureStage::OpenTarget),
        };
        if target
            .verify_digest(self.observed_digest, file_byte_limit(self.limits))
            .is_err()
        {
            return SceneSaveOutcome::Rejected(EditorPersistenceRejection::TargetChanged);
        }

        let mut temporary = match create_temporary(&self.parent) {
            Ok(temporary) => temporary,
            Err(_) => {
                return SceneSaveOutcome::Failed(EditorPersistenceFailureStage::CreateTemporary);
            }
        };
        if temporary.write_all(&bytes).is_err() || temporary.flush().is_err() {
            return SceneSaveOutcome::Failed(EditorPersistenceFailureStage::WriteTemporary);
        }
        let sync = match temporary.sync() {
            Ok(sync) => sync,
            Err(_) => {
                return SceneSaveOutcome::Failed(EditorPersistenceFailureStage::SyncTemporary);
            }
        };
        if sync.progress().data_synced() != StageStatus::Achieved
            || sync.progress().file_metadata_synced() != StageStatus::Achieved
        {
            return SceneSaveOutcome::Rejected(EditorPersistenceRejection::UnsupportedGuarantee);
        }

        let receipt = match self.parent.replace_temp(
            temporary,
            &self.target,
            ExpectedTarget::Identity(self.observed_identity),
        ) {
            Ok(receipt) => receipt,
            Err(FsError::TargetStateMismatch) => {
                return SceneSaveOutcome::Rejected(EditorPersistenceRejection::TargetChanged);
            }
            Err(_) => {
                return SceneSaveOutcome::Failed(EditorPersistenceFailureStage::ReplaceTarget);
            }
        };
        drop(lock);

        let parent_directory_sync = self.parent.sync().map_or(StageStatus::Unknown, |receipt| {
            receipt.progress().parent_directory_synced()
        });
        let evidence = EditorPersistenceReceipt {
            document: candidate.document,
            revision: candidate.revision,
            digest: checkpoint.digest,
            previous_identity: receipt.previous_identity(),
            candidate_identity: receipt.candidate_identity(),
            published_identity: receipt.published_identity(),
            identity_evidence: receipt.publication_identity_evidence(),
            publication: receipt.publication_atomicity(),
            durability: receipt.durability(),
            parent_directory_sync,
        };

        if !receipt_matches_required_evidence(receipt, self.observed_identity) {
            self.uncertain = true;
            return SceneSaveOutcome::PersistenceUncertain {
                checkpoint,
                evidence,
            };
        }

        self.observed_identity = receipt.candidate_identity();
        self.observed_digest = digest;
        SceneSaveOutcome::Saved(evidence)
    }

    pub(super) fn reopen(&mut self, registry: &ComponentRegistry) -> SceneReopenOutcome {
        let file = match self.parent.open_child_file(&self.target) {
            Ok(file) => file,
            Err(error) => {
                return match open_target_failure(error) {
                    SceneSaveOutcome::Rejected(reason) => SceneReopenOutcome::Rejected(reason),
                    SceneSaveOutcome::Failed(stage) => SceneReopenOutcome::Failed(stage),
                    SceneSaveOutcome::Saved(_) | SceneSaveOutcome::PersistenceUncertain { .. } => {
                        unreachable!()
                    }
                };
            }
        };
        let bytes = match file.read_to_end_bounded(file_byte_limit(self.limits)) {
            Ok(bytes) => bytes,
            Err(_) => {
                return SceneReopenOutcome::Failed(EditorPersistenceFailureStage::ReadTarget);
            }
        };
        let session = match decode_session(self.encoding, &bytes, self.limits, registry) {
            Ok(session) => session,
            Err(stage) => return SceneReopenOutcome::Failed(stage),
        };
        let digest = ContentDigest::of_bytes(&bytes);
        self.observed_identity = file.identity();
        self.observed_digest = digest;
        self.uncertain = false;
        SceneReopenOutcome::Opened {
            session,
            digest: editor_digest(digest),
        }
    }
}

fn scene_encoding(path: &str) -> Option<SceneEncoding> {
    if path.ends_with(".json") {
        Some(SceneEncoding::Json)
    } else if path.ends_with(".ron") {
        Some(SceneEncoding::Ron)
    } else {
        None
    }
}

fn encode_scene(
    encoding: SceneEncoding,
    scene: &SceneDocument,
) -> Result<Vec<u8>, EditorPersistenceFailureStage> {
    let encoded = match encoding {
        SceneEncoding::Json => scene.to_json_string(),
        SceneEncoding::Ron => scene.to_ron_string(),
    }
    .map_err(|_| EditorPersistenceFailureStage::Encode)?;
    Ok(encoded.into_bytes())
}

fn decode_session(
    encoding: SceneEncoding,
    bytes: &[u8],
    limits: SceneFileLimits,
    registry: &ComponentRegistry,
) -> Result<SceneAuthoringSession, EditorPersistenceFailureStage> {
    let candidate = match encoding {
        SceneEncoding::Json => SceneDocumentCandidate::decode_json_bytes_with_limits(bytes, limits),
        SceneEncoding::Ron => SceneDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits),
    }
    .map_err(|_| EditorPersistenceFailureStage::Decode)?;
    SceneAuthoringSession::try_from_file_candidate(candidate, registry)
        .map_err(|_| EditorPersistenceFailureStage::Validate)
}

fn create_temporary(parent: &DirectoryCapability) -> Result<nara_fs::TemporaryFile, FsError> {
    for _ in 0..TEMPORARY_NAME_ATTEMPTS {
        let sequence = NEXT_TEMPORARY_NAME.fetch_add(1, Ordering::Relaxed);
        let name =
            RelativeComponent::new(format!(".nara-save-{}-{sequence}.tmp", std::process::id()))
                .expect("the engine-owned temporary filename is valid");
        match parent.create_temp(&name) {
            Ok(temporary) => return Ok(temporary),
            Err(FsError::AlreadyExists { .. }) => continue,
            Err(error) => return Err(error),
        }
    }
    Err(FsError::AlreadyExists {
        operation: nara_fs::FsOperation::CreateTemporary,
    })
}

fn receipt_matches_required_evidence(
    receipt: ReplaceReceipt,
    expected_previous: FileIdentity,
) -> bool {
    receipt.previous_identity() == Some(expected_previous)
        && receipt.naming_is_atomic()
        && receipt.published_identity() == Some(receipt.candidate_identity())
        && matches!(
            receipt.publication_identity_evidence(),
            PublicationIdentityEvidence::HandleBoundCandidate
                | PublicationIdentityEvidence::PostPublishObserved
        )
        && receipt.durability().data_synced() == StageStatus::Achieved
        && receipt.durability().file_metadata_synced() == StageStatus::Achieved
        && receipt.durability().name_published() == StageStatus::Achieved
}

fn open_target_failure(error: FsError) -> SceneSaveOutcome {
    match error {
        FsError::Io { ref source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            SceneSaveOutcome::Rejected(EditorPersistenceRejection::TargetDeleted)
        }
        FsError::Unsupported { .. } | FsError::Unproven { .. } => {
            SceneSaveOutcome::Rejected(EditorPersistenceRejection::UnsupportedGuarantee)
        }
        _ => SceneSaveOutcome::Failed(EditorPersistenceFailureStage::OpenTarget),
    }
}

fn editor_digest(digest: ContentDigest) -> EditorDocumentDigest {
    EditorDocumentDigest::new(digest.length(), *digest.as_bytes())
}

fn file_byte_limit(limits: SceneFileLimits) -> u64 {
    u64::try_from(limits.encoded_bytes().get()).unwrap_or(u64::MAX)
}
