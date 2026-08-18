use std::{
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_fs::{
    ContentDigest, DirectoryCapability, DurabilityProgress, ExpectedTarget, FileIdentity, FsError,
    LockMode, PublicationAtomicity, PublicationIdentityEvidence, PublicationSourceBinding,
    RelativeComponent, RelativePath, StageStatus,
};
use nara_reflect::ComponentRegistry;
use nara_scene::{
    PrefabAuthoringSession, PrefabDocumentCandidate, SceneAuthoringRevision, SceneAuthoringSession,
    SceneDocument, SceneDocumentCandidate, SceneFileLimits,
};
use nara_tooling::{
    EditorDocumentDigest, EditorDocumentId, EditorPersistenceCheckpoint, EditorPersistenceCommit,
    EditorPersistenceFailureStage, EditorPersistenceRejection,
};

static NEXT_TEMPORARY_NAME: AtomicU64 = AtomicU64::new(1);
const TEMPORARY_NAME_ATTEMPTS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DocumentEncoding {
    Json,
    Ron,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PersistenceDocumentLane {
    Native,
    Authoring,
}

#[derive(Debug)]
struct PersistenceFileState {
    parent: DirectoryCapability,
    target: RelativeComponent,
    encoding: DocumentEncoding,
    limits: SceneFileLimits,
    observed_identity: FileIdentity,
    observed_digest: ContentDigest,
    uncertain: bool,
    #[cfg(test)]
    reject_next_valid_receipt: bool,
    #[cfg(test)]
    fail_next_save: Option<EditorPersistenceFailureStage>,
}

pub(super) struct ScenePersistenceHost {
    file: PersistenceFileState,
    lane: PersistenceDocumentLane,
}

pub(super) struct PrefabPersistenceHost {
    file: PersistenceFileState,
    source_binding: nara_tooling::EditorDocumentSourceBinding,
    lane: PersistenceDocumentLane,
}

pub(super) struct OpenedScenePersistence {
    pub host: ScenePersistenceHost,
    pub session: SceneAuthoringSession,
    pub digest: EditorDocumentDigest,
}

pub(super) struct OpenedPrefabPersistence {
    pub host: PrefabPersistenceHost,
    pub session: PrefabAuthoringSession,
    pub digest: EditorDocumentDigest,
}

pub(super) struct SceneSaveCandidate {
    pub checkpoint: EditorPersistenceCheckpoint,
}

pub(super) struct PrefabSaveCandidate {
    pub checkpoint: EditorPersistenceCheckpoint,
}

pub(super) enum PersistenceSaveOutcome {
    Saved {
        commit: EditorPersistenceCommit,
        evidence: EditorPersistenceReceipt,
    },
    Rejected(EditorPersistenceRejection),
    Failed(EditorPersistenceFailureStage),
    PersistenceUncertain {
        evidence: EditorPersistenceReceipt,
    },
}

pub(super) type SceneSaveOutcome = PersistenceSaveOutcome;
type PrefabSaveOutcome = PersistenceSaveOutcome;

pub(super) enum SceneReopenOutcome {
    Opened(Box<OpenedSceneReopen>),
    Rejected(EditorPersistenceRejection),
    Failed(EditorPersistenceFailureStage),
}

pub(super) enum PrefabReopenOutcome {
    Opened(Box<OpenedPrefabReopen>),
    Rejected(EditorPersistenceRejection),
    Failed(EditorPersistenceFailureStage),
}

enum FileReopenOutcome {
    Opened {
        bytes: Vec<u8>,
        identity: FileIdentity,
        content_digest: ContentDigest,
    },
    Rejected(EditorPersistenceRejection),
    Failed(EditorPersistenceFailureStage),
}

pub(super) struct OpenedSceneReopen {
    pub(super) session: SceneAuthoringSession,
    pub(super) digest: EditorDocumentDigest,
    pub(super) identity: FileIdentity,
    pub(super) content_digest: ContentDigest,
}

pub(super) struct OpenedPrefabReopen {
    pub(super) session: PrefabAuthoringSession,
    pub(super) digest: EditorDocumentDigest,
    pub(super) identity: FileIdentity,
    pub(super) content_digest: ContentDigest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorPersistenceReceipt {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    digest: EditorDocumentDigest,
    published_digest: Option<EditorDocumentDigest>,
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
    pub const fn published_digest(self) -> Option<EditorDocumentDigest> {
        self.published_digest
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
}

impl PersistenceFileState {
    fn open(
        root: &DirectoryCapability,
        path: &str,
        limits: SceneFileLimits,
    ) -> Result<(Self, Vec<u8>), EditorPersistenceFailureStage> {
        let encoding = document_encoding(path).ok_or(EditorPersistenceFailureStage::Decode)?;
        let path =
            RelativePath::new(path).map_err(|_| EditorPersistenceFailureStage::OpenTarget)?;
        let (parent, target) = root
            .resolve_file_parent(&path)
            .map_err(|_| EditorPersistenceFailureStage::OpenTarget)?;
        let file = parent
            .open_child_file(&target)
            .map_err(|_| EditorPersistenceFailureStage::OpenTarget)?;
        let identity = file.identity();
        let bytes = file
            .read_to_end_bounded(file_byte_limit(limits))
            .map_err(|_| EditorPersistenceFailureStage::ReadTarget)?;
        let digest = ContentDigest::of_bytes(&bytes);
        Ok((
            Self {
                parent,
                target,
                encoding,
                limits,
                observed_identity: identity,
                observed_digest: digest,
                uncertain: false,
                #[cfg(test)]
                reject_next_valid_receipt: false,
                #[cfg(test)]
                fail_next_save: None,
            },
            bytes,
        ))
    }

    fn bind_existing(
        root: &DirectoryCapability,
        path: &str,
        limits: SceneFileLimits,
        expected_identity: FileIdentity,
        expected_digest: ContentDigest,
    ) -> Result<Self, EditorPersistenceFailureStage> {
        let encoding = document_encoding(path).ok_or(EditorPersistenceFailureStage::Decode)?;
        let path =
            RelativePath::new(path).map_err(|_| EditorPersistenceFailureStage::OpenTarget)?;
        let (parent, target) = root
            .resolve_file_parent(&path)
            .map_err(|_| EditorPersistenceFailureStage::OpenTarget)?;
        let file = parent
            .open_child_file(&target)
            .map_err(|_| EditorPersistenceFailureStage::OpenTarget)?;
        if file.identity() != expected_identity {
            return Err(EditorPersistenceFailureStage::Validate);
        }
        file.verify_digest(expected_digest, file_byte_limit(limits))
            .map_err(|error| match error {
                FsError::DigestMismatch { .. } => EditorPersistenceFailureStage::Validate,
                _ => EditorPersistenceFailureStage::ReadTarget,
            })?;
        Ok(Self {
            parent,
            target,
            encoding,
            limits,
            observed_identity: expected_identity,
            observed_digest: expected_digest,
            uncertain: false,
            #[cfg(test)]
            reject_next_valid_receipt: false,
            #[cfg(test)]
            fail_next_save: None,
        })
    }

    fn preflight_save(&mut self) -> Result<(), PersistenceSaveOutcome> {
        if self.uncertain {
            return Err(PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::RequiresReconcile,
            ));
        }
        #[cfg(test)]
        if let Some(stage) = self.fail_next_save.take() {
            return Err(PersistenceSaveOutcome::Failed(stage));
        }
        let matrix = nara_fs::platform_capability_matrix();
        if matrix.publication_atomicity() != PublicationAtomicity::AtomicNameSwitch
            || matrix.replace_source_binding() == PublicationSourceBinding::Unsupported
        {
            return Err(PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::UnsupportedGuarantee,
            ));
        }
        Ok(())
    }

    fn save_preflighted(
        &mut self,
        checkpoint: EditorPersistenceCheckpoint,
        bytes: &[u8],
    ) -> PersistenceSaveOutcome {
        if bytes.len() > self.limits.encoded_bytes().get() {
            return PersistenceSaveOutcome::Failed(EditorPersistenceFailureStage::Encode);
        }

        let target = match self.parent.open_child_file(&self.target) {
            Ok(target) => target,
            Err(error) => return open_target_failure(error),
        };
        if target.identity() != self.observed_identity {
            return PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::TargetChanged);
        }
        let lock = match target.try_lock(LockMode::Exclusive) {
            Ok(lock) => lock,
            Err(FsError::LockContended) => {
                return PersistenceSaveOutcome::Rejected(
                    EditorPersistenceRejection::LockUnavailable,
                );
            }
            Err(_) => {
                return PersistenceSaveOutcome::Failed(EditorPersistenceFailureStage::OpenTarget);
            }
        };
        if target
            .verify_digest(self.observed_digest, file_byte_limit(self.limits))
            .is_err()
        {
            return PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::TargetChanged);
        }

        let mut temporary = match create_temporary(&self.parent) {
            Ok(temporary) => temporary,
            Err(_) => {
                return PersistenceSaveOutcome::Failed(
                    EditorPersistenceFailureStage::CreateTemporary,
                );
            }
        };
        if temporary.write_all(bytes).is_err() || temporary.flush().is_err() {
            return PersistenceSaveOutcome::Failed(EditorPersistenceFailureStage::WriteTemporary);
        }
        let sync = match temporary.sync() {
            Ok(sync) => sync,
            Err(_) => {
                return PersistenceSaveOutcome::Failed(
                    EditorPersistenceFailureStage::SyncTemporary,
                );
            }
        };
        if sync.progress().data_synced() != StageStatus::Achieved
            || sync.progress().file_metadata_synced() != StageStatus::Achieved
        {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::UnsupportedGuarantee,
            );
        }

        let receipt = match self.parent.replace_temp(
            temporary,
            &self.target,
            ExpectedTarget::Identity(self.observed_identity),
        ) {
            Ok(receipt) => receipt,
            Err(FsError::TargetStateMismatch) => {
                return PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::TargetChanged);
            }
            Err(_) => {
                return PersistenceSaveOutcome::Failed(
                    EditorPersistenceFailureStage::ReplaceTarget,
                );
            }
        };
        drop(lock);

        let parent_directory_sync = self.parent.sync().map_or(StageStatus::Unknown, |receipt| {
            receipt.progress().parent_directory_synced()
        });
        let expected_digest = ContentDigest::of_bytes(bytes);
        let written_digest = receipt.written_content_digest();
        let published_digest = receipt.published_content_digest();
        let evidence = EditorPersistenceReceipt {
            document: checkpoint.document(),
            revision: checkpoint.revision(),
            digest: editor_digest(expected_digest),
            published_digest: published_digest.map(editor_digest),
            previous_identity: receipt.previous_identity(),
            candidate_identity: receipt.candidate_identity(),
            published_identity: receipt.published_identity(),
            identity_evidence: receipt.publication_identity_evidence(),
            publication: receipt.publication_atomicity(),
            durability: receipt.durability(),
            parent_directory_sync,
        };

        #[cfg(test)]
        if std::mem::take(&mut self.reject_next_valid_receipt) {
            self.uncertain = true;
            return PersistenceSaveOutcome::PersistenceUncertain { evidence };
        }
        if written_digest != expected_digest {
            self.uncertain = true;
            return PersistenceSaveOutcome::PersistenceUncertain { evidence };
        }
        let candidate_identity = receipt.candidate_identity();
        let Some(commit) =
            EditorPersistenceCommit::from_publication(checkpoint, self.observed_identity, receipt)
        else {
            self.uncertain = true;
            return PersistenceSaveOutcome::PersistenceUncertain { evidence };
        };

        self.observed_identity = candidate_identity;
        self.observed_digest = published_digest
            .expect("an accepted persistence commit has published content evidence");
        PersistenceSaveOutcome::Saved { commit, evidence }
    }

    fn reopen(&self) -> FileReopenOutcome {
        let file = match self.parent.open_child_file(&self.target) {
            Ok(file) => file,
            Err(error) => return open_target_reopen_failure(error),
        };
        let bytes = match file.read_to_end_bounded(file_byte_limit(self.limits)) {
            Ok(bytes) => bytes,
            Err(_) => return FileReopenOutcome::Failed(EditorPersistenceFailureStage::ReadTarget),
        };
        let content_digest = ContentDigest::of_bytes(&bytes);
        FileReopenOutcome::Opened {
            bytes,
            identity: file.identity(),
            content_digest,
        }
    }

    fn commit_reopen(&mut self, identity: FileIdentity, digest: ContentDigest) {
        self.observed_identity = identity;
        self.observed_digest = digest;
        self.uncertain = false;
    }

    fn mark_uncertain(&mut self) {
        self.uncertain = true;
    }

    #[cfg(test)]
    fn test_reject_next_valid_receipt(&mut self) {
        self.reject_next_valid_receipt = true;
    }

    #[cfg(test)]
    fn test_fail_next_save(&mut self, stage: EditorPersistenceFailureStage) {
        self.fail_next_save = Some(stage);
    }
}

impl ScenePersistenceHost {
    pub(super) fn open(
        scenes: &DirectoryCapability,
        scene_path: &str,
        registry: &ComponentRegistry,
    ) -> Result<OpenedScenePersistence, EditorPersistenceFailureStage> {
        let limits = SceneFileLimits::default();
        let (file, bytes) = PersistenceFileState::open(scenes, scene_path, limits)?;
        let session = decode_scene_session(file.encoding, &bytes, limits, registry)?;
        Ok(OpenedScenePersistence {
            digest: editor_digest(file.observed_digest),
            host: Self {
                file,
                lane: PersistenceDocumentLane::Native,
            },
            session,
        })
    }

    pub(super) fn bind_authoring_source(
        scenes: &DirectoryCapability,
        scene_path: &str,
        identity: FileIdentity,
        digest: ContentDigest,
    ) -> Result<(Self, EditorDocumentDigest), EditorPersistenceFailureStage> {
        let file = PersistenceFileState::bind_existing(
            scenes,
            scene_path,
            SceneFileLimits::default(),
            identity,
            digest,
        )?;
        Ok((
            Self {
                file,
                lane: PersistenceDocumentLane::Authoring,
            },
            editor_digest(digest),
        ))
    }

    pub(super) fn save_native(&mut self, candidate: SceneSaveCandidate) -> SceneSaveOutcome {
        if self.lane != PersistenceDocumentLane::Native {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::PreservationMismatch,
            );
        }
        let SceneSaveCandidate { checkpoint } = candidate;
        if checkpoint.kind() != nara_tooling::EditorDocumentKind::Scene {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::DocumentKindMismatch,
            );
        }
        let Some(scene) = checkpoint.scene() else {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::DocumentKindMismatch,
            );
        };
        if let Err(outcome) = self.file.preflight_save() {
            return outcome;
        }
        let bytes = match encode_scene(self.file.encoding, scene) {
            Ok(bytes) => bytes,
            Err(stage) => return PersistenceSaveOutcome::Failed(stage),
        };
        self.file.save_preflighted(checkpoint, &bytes)
    }

    pub(super) fn save_authoring(
        &mut self,
        candidate: SceneSaveCandidate,
        current: &SceneAuthoringSession,
    ) -> SceneSaveOutcome {
        if self.lane != PersistenceDocumentLane::Authoring
            || current.authoring_readiness().is_none()
        {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::PreservationMismatch,
            );
        }
        let SceneSaveCandidate { checkpoint } = candidate;
        if checkpoint.kind() != nara_tooling::EditorDocumentKind::Scene {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::DocumentKindMismatch,
            );
        }
        let Some(scene) = checkpoint.scene() else {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::DocumentKindMismatch,
            );
        };
        if let Err(outcome) = self.file.preflight_save() {
            return outcome;
        }
        let bytes = match encode_scene(self.file.encoding, scene) {
            Ok(bytes) => bytes,
            Err(stage) => return PersistenceSaveOutcome::Failed(stage),
        };
        match validate_authoring_scene_save(self.file.encoding, &bytes, self.file.limits, current) {
            Ok(()) => {}
            Err(AuthoringSaveValidationError::Decode(stage)) => {
                return PersistenceSaveOutcome::Failed(stage);
            }
            Err(AuthoringSaveValidationError::PreservationMismatch) => {
                return PersistenceSaveOutcome::Rejected(
                    EditorPersistenceRejection::PreservationMismatch,
                );
            }
        }
        self.file.save_preflighted(checkpoint, &bytes)
    }

    pub(super) fn reopen_native(&self, registry: &ComponentRegistry) -> SceneReopenOutcome {
        if self.lane != PersistenceDocumentLane::Native {
            return SceneReopenOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch);
        }
        let reopened = self.file.reopen();
        let FileReopenOutcome::Opened {
            bytes,
            identity,
            content_digest,
        } = reopened
        else {
            return match reopened {
                FileReopenOutcome::Rejected(reason) => SceneReopenOutcome::Rejected(reason),
                FileReopenOutcome::Failed(stage) => SceneReopenOutcome::Failed(stage),
                FileReopenOutcome::Opened { .. } => unreachable!(),
            };
        };
        let session =
            match decode_scene_session(self.file.encoding, &bytes, self.file.limits, registry) {
                Ok(session) => session,
                Err(stage) => return SceneReopenOutcome::Failed(stage),
            };
        SceneReopenOutcome::Opened(Box::new(OpenedSceneReopen {
            session,
            digest: editor_digest(content_digest),
            identity,
            content_digest,
        }))
    }

    pub(super) fn reopen_authoring(&self, current: &SceneAuthoringSession) -> SceneReopenOutcome {
        if self.lane != PersistenceDocumentLane::Authoring
            || current.authoring_readiness().is_none()
        {
            return SceneReopenOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch);
        }
        let reopened = self.file.reopen();
        let FileReopenOutcome::Opened {
            bytes,
            identity,
            content_digest,
        } = reopened
        else {
            return match reopened {
                FileReopenOutcome::Rejected(reason) => SceneReopenOutcome::Rejected(reason),
                FileReopenOutcome::Failed(stage) => SceneReopenOutcome::Failed(stage),
                FileReopenOutcome::Opened { .. } => unreachable!(),
            };
        };
        let session = match decode_authoring_scene_session(
            self.file.encoding,
            &bytes,
            self.file.limits,
            current,
        ) {
            Ok(session) => session,
            Err(stage) => return SceneReopenOutcome::Failed(stage),
        };
        SceneReopenOutcome::Opened(Box::new(OpenedSceneReopen {
            session,
            digest: editor_digest(content_digest),
            identity,
            content_digest,
        }))
    }

    pub(super) fn commit_reopen(&mut self, identity: FileIdentity, digest: ContentDigest) {
        self.file.commit_reopen(identity, digest);
    }

    pub(super) fn mark_uncertain(&mut self) {
        self.file.mark_uncertain();
    }

    #[cfg(test)]
    pub(super) fn test_reject_next_valid_receipt(&mut self) {
        self.file.test_reject_next_valid_receipt();
    }

    #[cfg(test)]
    pub(super) fn test_fail_next_save(&mut self, stage: EditorPersistenceFailureStage) {
        self.file.test_fail_next_save(stage);
    }
}

impl PrefabPersistenceHost {
    pub(super) fn open(
        prefabs: &DirectoryCapability,
        source_binding: nara_tooling::EditorDocumentSourceBinding,
        registry: &ComponentRegistry,
    ) -> Result<OpenedPrefabPersistence, EditorPersistenceFailureStage> {
        let limits = SceneFileLimits::default();
        let (file, bytes) = PersistenceFileState::open(prefabs, source_binding.locator(), limits)?;
        let session = decode_prefab_session(file.encoding, &bytes, limits, registry)?;
        Ok(OpenedPrefabPersistence {
            digest: editor_digest(file.observed_digest),
            host: Self {
                file,
                source_binding,
                lane: PersistenceDocumentLane::Native,
            },
            session,
        })
    }

    pub(super) fn bind_authoring_source(
        prefabs: &DirectoryCapability,
        source_binding: nara_tooling::EditorDocumentSourceBinding,
        identity: FileIdentity,
        digest: ContentDigest,
    ) -> Result<(Self, EditorDocumentDigest), EditorPersistenceFailureStage> {
        let file = PersistenceFileState::bind_existing(
            prefabs,
            source_binding.locator(),
            SceneFileLimits::default(),
            identity,
            digest,
        )?;
        Ok((
            Self {
                file,
                source_binding,
                lane: PersistenceDocumentLane::Authoring,
            },
            editor_digest(digest),
        ))
    }

    pub(super) fn save_native(&mut self, candidate: PrefabSaveCandidate) -> PrefabSaveOutcome {
        if self.lane != PersistenceDocumentLane::Native {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::PreservationMismatch,
            );
        }
        let PrefabSaveCandidate { checkpoint } = candidate;
        if checkpoint.kind() != nara_tooling::EditorDocumentKind::Prefab {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::DocumentKindMismatch,
            );
        }
        if checkpoint.source_binding() != Some(&self.source_binding) {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::SourceBindingMismatch,
            );
        }
        let Some(prefab) = checkpoint.prefab() else {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::DocumentKindMismatch,
            );
        };
        if let Err(outcome) = self.file.preflight_save() {
            return outcome;
        }
        let bytes = match encode_prefab(self.file.encoding, prefab) {
            Ok(bytes) => bytes,
            Err(stage) => return PersistenceSaveOutcome::Failed(stage),
        };
        self.file.save_preflighted(checkpoint, &bytes)
    }

    pub(super) fn save_authoring(
        &mut self,
        candidate: PrefabSaveCandidate,
        current: &PrefabAuthoringSession,
    ) -> PrefabSaveOutcome {
        if self.lane != PersistenceDocumentLane::Authoring
            || current.authoring_readiness().is_none()
        {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::PreservationMismatch,
            );
        }
        let PrefabSaveCandidate { checkpoint } = candidate;
        if checkpoint.kind() != nara_tooling::EditorDocumentKind::Prefab {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::DocumentKindMismatch,
            );
        }
        if checkpoint.source_binding() != Some(&self.source_binding) {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::SourceBindingMismatch,
            );
        }
        let Some(prefab) = checkpoint.prefab() else {
            return PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::DocumentKindMismatch,
            );
        };
        if let Err(outcome) = self.file.preflight_save() {
            return outcome;
        }
        let bytes = match encode_prefab(self.file.encoding, prefab) {
            Ok(bytes) => bytes,
            Err(stage) => return PersistenceSaveOutcome::Failed(stage),
        };
        match validate_authoring_prefab_save(self.file.encoding, &bytes, self.file.limits, current)
        {
            Ok(()) => {}
            Err(AuthoringSaveValidationError::Decode(stage)) => {
                return PersistenceSaveOutcome::Failed(stage);
            }
            Err(AuthoringSaveValidationError::PreservationMismatch) => {
                return PersistenceSaveOutcome::Rejected(
                    EditorPersistenceRejection::PreservationMismatch,
                );
            }
        }
        self.file.save_preflighted(checkpoint, &bytes)
    }

    pub(super) fn reopen_native(&self, registry: &ComponentRegistry) -> PrefabReopenOutcome {
        if self.lane != PersistenceDocumentLane::Native {
            return PrefabReopenOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch);
        }
        let reopened = self.file.reopen();
        let FileReopenOutcome::Opened {
            bytes,
            identity,
            content_digest,
        } = reopened
        else {
            return match reopened {
                FileReopenOutcome::Rejected(reason) => PrefabReopenOutcome::Rejected(reason),
                FileReopenOutcome::Failed(stage) => PrefabReopenOutcome::Failed(stage),
                FileReopenOutcome::Opened { .. } => unreachable!(),
            };
        };
        let session =
            match decode_prefab_session(self.file.encoding, &bytes, self.file.limits, registry) {
                Ok(session) => session,
                Err(stage) => return PrefabReopenOutcome::Failed(stage),
            };
        PrefabReopenOutcome::Opened(Box::new(OpenedPrefabReopen {
            session,
            digest: editor_digest(content_digest),
            identity,
            content_digest,
        }))
    }

    pub(super) fn reopen_authoring(&self, current: &PrefabAuthoringSession) -> PrefabReopenOutcome {
        if self.lane != PersistenceDocumentLane::Authoring
            || current.authoring_readiness().is_none()
        {
            return PrefabReopenOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch);
        }
        let reopened = self.file.reopen();
        let FileReopenOutcome::Opened {
            bytes,
            identity,
            content_digest,
        } = reopened
        else {
            return match reopened {
                FileReopenOutcome::Rejected(reason) => PrefabReopenOutcome::Rejected(reason),
                FileReopenOutcome::Failed(stage) => PrefabReopenOutcome::Failed(stage),
                FileReopenOutcome::Opened { .. } => unreachable!(),
            };
        };
        let session = match decode_authoring_prefab_session(
            self.file.encoding,
            &bytes,
            self.file.limits,
            current,
        ) {
            Ok(session) => session,
            Err(stage) => return PrefabReopenOutcome::Failed(stage),
        };
        PrefabReopenOutcome::Opened(Box::new(OpenedPrefabReopen {
            session,
            digest: editor_digest(content_digest),
            identity,
            content_digest,
        }))
    }

    pub(super) const fn publishes_runtime_content(&self) -> bool {
        matches!(self.lane, PersistenceDocumentLane::Native)
    }

    pub(super) fn commit_reopen(&mut self, identity: FileIdentity, digest: ContentDigest) {
        self.file.commit_reopen(identity, digest);
    }

    pub(super) fn mark_uncertain(&mut self) {
        self.file.mark_uncertain();
    }

    #[cfg(test)]
    pub(super) fn test_reject_next_valid_receipt(&mut self) {
        self.file.test_reject_next_valid_receipt();
    }

    #[cfg(test)]
    pub(super) fn test_fail_next_save(&mut self, stage: EditorPersistenceFailureStage) {
        self.file.test_fail_next_save(stage);
    }
}

fn document_encoding(path: &str) -> Option<DocumentEncoding> {
    if path.ends_with(".json") {
        Some(DocumentEncoding::Json)
    } else if path.ends_with(".ron") {
        Some(DocumentEncoding::Ron)
    } else {
        None
    }
}

fn encode_scene(
    encoding: DocumentEncoding,
    scene: &SceneDocument,
) -> Result<Vec<u8>, EditorPersistenceFailureStage> {
    let encoded = match encoding {
        DocumentEncoding::Json => scene.to_json_string(),
        DocumentEncoding::Ron => scene.to_ron_string(),
    }
    .map_err(|_| EditorPersistenceFailureStage::Encode)?;
    Ok(encoded.into_bytes())
}

fn decode_scene_session(
    encoding: DocumentEncoding,
    bytes: &[u8],
    limits: SceneFileLimits,
    registry: &ComponentRegistry,
) -> Result<SceneAuthoringSession, EditorPersistenceFailureStage> {
    let candidate = match encoding {
        DocumentEncoding::Json => {
            SceneDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
        }
        DocumentEncoding::Ron => {
            SceneDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
        }
    }
    .map_err(|_| EditorPersistenceFailureStage::Decode)?;
    SceneAuthoringSession::try_from_file_candidate(candidate, registry)
        .map_err(|_| EditorPersistenceFailureStage::Validate)
}

fn decode_authoring_scene_session(
    encoding: DocumentEncoding,
    bytes: &[u8],
    limits: SceneFileLimits,
    current: &SceneAuthoringSession,
) -> Result<SceneAuthoringSession, EditorPersistenceFailureStage> {
    let candidate = match encoding {
        DocumentEncoding::Json => {
            SceneDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
        }
        DocumentEncoding::Ron => {
            SceneDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
        }
    }
    .map_err(|_| EditorPersistenceFailureStage::Decode)?;
    current
        .try_reopen_authoring_candidate(candidate)
        .map_err(|_| EditorPersistenceFailureStage::Validate)
}

enum AuthoringSaveValidationError {
    Decode(EditorPersistenceFailureStage),
    PreservationMismatch,
}

fn validate_authoring_scene_save(
    encoding: DocumentEncoding,
    bytes: &[u8],
    limits: SceneFileLimits,
    current: &SceneAuthoringSession,
) -> Result<(), AuthoringSaveValidationError> {
    let reopened = decode_authoring_scene_session(encoding, bytes, limits, current)
        .map_err(AuthoringSaveValidationError::Decode)?;
    if reopened.authoring_readiness() != current.authoring_readiness()
        || reopened.unavailable_record_manifest() != current.unavailable_record_manifest()
    {
        return Err(AuthoringSaveValidationError::PreservationMismatch);
    }
    Ok(())
}

fn encode_prefab(
    encoding: DocumentEncoding,
    prefab: &nara_scene::PrefabDocument,
) -> Result<Vec<u8>, EditorPersistenceFailureStage> {
    let encoded = match encoding {
        DocumentEncoding::Json => prefab.to_json_string(),
        DocumentEncoding::Ron => prefab.to_ron_string(),
    }
    .map_err(|_| EditorPersistenceFailureStage::Encode)?;
    Ok(encoded.into_bytes())
}

fn decode_prefab_session(
    encoding: DocumentEncoding,
    bytes: &[u8],
    limits: SceneFileLimits,
    registry: &ComponentRegistry,
) -> Result<PrefabAuthoringSession, EditorPersistenceFailureStage> {
    let candidate = match encoding {
        DocumentEncoding::Json => {
            PrefabDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
        }
        DocumentEncoding::Ron => {
            PrefabDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
        }
    }
    .map_err(|_| EditorPersistenceFailureStage::Decode)?;
    PrefabAuthoringSession::try_from_file_candidate(candidate, registry)
        .map_err(|_| EditorPersistenceFailureStage::Validate)
}

fn decode_authoring_prefab_session(
    encoding: DocumentEncoding,
    bytes: &[u8],
    limits: SceneFileLimits,
    current: &PrefabAuthoringSession,
) -> Result<PrefabAuthoringSession, EditorPersistenceFailureStage> {
    let candidate = match encoding {
        DocumentEncoding::Json => {
            PrefabDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
        }
        DocumentEncoding::Ron => {
            PrefabDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
        }
    }
    .map_err(|_| EditorPersistenceFailureStage::Decode)?;
    current
        .try_reopen_authoring_candidate(candidate)
        .map_err(|_| EditorPersistenceFailureStage::Validate)
}

fn validate_authoring_prefab_save(
    encoding: DocumentEncoding,
    bytes: &[u8],
    limits: SceneFileLimits,
    current: &PrefabAuthoringSession,
) -> Result<(), AuthoringSaveValidationError> {
    let reopened = decode_authoring_prefab_session(encoding, bytes, limits, current)
        .map_err(AuthoringSaveValidationError::Decode)?;
    if reopened.authoring_readiness() != current.authoring_readiness()
        || reopened.unavailable_record_manifest() != current.unavailable_record_manifest()
    {
        return Err(AuthoringSaveValidationError::PreservationMismatch);
    }
    Ok(())
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

fn open_target_failure(error: FsError) -> PersistenceSaveOutcome {
    match error {
        FsError::Io { ref source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::TargetDeleted)
        }
        FsError::Unsupported { .. } | FsError::Unproven { .. } => {
            PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::UnsupportedGuarantee)
        }
        _ => PersistenceSaveOutcome::Failed(EditorPersistenceFailureStage::OpenTarget),
    }
}

fn open_target_reopen_failure(error: FsError) -> FileReopenOutcome {
    match error {
        FsError::Io { ref source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            FileReopenOutcome::Rejected(EditorPersistenceRejection::TargetDeleted)
        }
        FsError::Unsupported { .. } | FsError::Unproven { .. } => {
            FileReopenOutcome::Rejected(EditorPersistenceRejection::UnsupportedGuarantee)
        }
        _ => FileReopenOutcome::Failed(EditorPersistenceFailureStage::OpenTarget),
    }
}

fn editor_digest(digest: ContentDigest) -> EditorDocumentDigest {
    EditorDocumentDigest::new(digest.length(), *digest.as_bytes())
}

fn file_byte_limit(limits: SceneFileLimits) -> u64 {
    u64::try_from(limits.encoded_bytes().get()).unwrap_or(u64::MAX)
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    use super::*;

    use std::{
        fs::{self, File},
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    #[cfg(windows)]
    use std::fs::OpenOptions;

    use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
    use nara_reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldId, ComponentValue,
        PersistentComponentSchemaOwnerId,
    };
    use nara_scene::{
        AuthoringDocumentSource, AuthoringScalarEdit, PrefabDocument,
        SCENE_COMPONENTS_SCHEMA_OWNER_ID, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneAuthoringSession,
        SceneComponentRecord, SceneEntityId, SceneEntityRecord,
    };
    use nara_tooling::{EditorDocumentSourceBinding, EditorWorkspace, EditorWorkspaceCommand};

    static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn uncertain_receipt_blocks_save_until_reopen_is_committed() {
        let root = TestRoot::new();
        let scenes = root.capability();
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let opened = ScenePersistenceHost::open(&scenes, "startup.scene.json", &registry).unwrap();
        let mut host = opened.host;
        let first = scene_with_entity("first");
        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let document = workspace
            .open_scene_session(
                "startup.scene.json",
                SceneAuthoringSession::new(first.clone()),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let revision = workspace.scene(document).unwrap().revision();

        host.file.reject_next_valid_receipt = true;
        let SceneSaveOutcome::PersistenceUncertain { evidence } =
            host.save_native(SceneSaveCandidate {
                checkpoint: authority.capture(&workspace, document).unwrap(),
            })
        else {
            panic!("a rejected post-publication receipt must be uncertain");
        };
        assert_eq!(evidence.document(), document);
        assert_eq!(evidence.revision(), revision);
        assert_eq!(
            evidence.publication_atomicity(),
            PublicationAtomicity::AtomicNameSwitch
        );
        assert_eq!(
            evidence.published_identity(),
            Some(evidence.candidate_identity())
        );
        assert!(host.file.uncertain);
        let first_bytes = fs::read(root.scene_path()).unwrap();

        assert!(matches!(
            host.save_native(SceneSaveCandidate {
                checkpoint: authority.capture(&workspace, document).unwrap(),
            }),
            SceneSaveOutcome::Rejected(EditorPersistenceRejection::RequiresReconcile)
        ));
        assert_eq!(fs::read(root.scene_path()).unwrap(), first_bytes);

        let SceneReopenOutcome::Opened(opened) = host.reopen_native(&registry) else {
            panic!("the published candidate must reopen");
        };
        let OpenedSceneReopen {
            session,
            identity,
            content_digest,
            ..
        } = *opened;
        assert_eq!(session.document(), &first);
        assert!(host.file.uncertain);

        host.commit_reopen(identity, content_digest);
        assert!(!host.file.uncertain);
        assert!(matches!(
            host.save_native(SceneSaveCandidate {
                checkpoint: authority.capture(&workspace, document).unwrap(),
            }),
            SceneSaveOutcome::Saved { .. }
        ));
    }

    #[test]
    fn prefab_lane_saves_and_reopens_the_prefab_source_without_scene_projection() {
        let root = TestRoot::new();
        let prefabs = root.capability();
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let source_binding = EditorDocumentSourceBinding::new("reference-ui.prefab.json");
        let opened =
            PrefabPersistenceHost::open(&prefabs, source_binding.clone(), &registry).unwrap();
        let expected = prefab_with_entity("retry");
        assert_eq!(opened.session.document(), &expected);

        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let document = workspace
            .open_prefab_session("reference-ui.prefab.json", source_binding, opened.session)
            .unwrap()
            .opened_document
            .unwrap();
        let mut host = opened.host;
        assert!(host.publishes_runtime_content());

        let authoring_target = prefabs
            .open_file(&RelativePath::new("reference-ui.prefab.json").unwrap())
            .unwrap();
        let authoring_identity = authoring_target.identity();
        let authoring_digest = authoring_target
            .digest(file_byte_limit(SceneFileLimits::default()))
            .unwrap();
        drop(authoring_target);
        let (authoring_host, _) = PrefabPersistenceHost::bind_authoring_source(
            &prefabs,
            EditorDocumentSourceBinding::new("reference-ui.prefab.json"),
            authoring_identity,
            authoring_digest,
        )
        .unwrap();
        assert!(!authoring_host.publishes_runtime_content());

        let opened_scene =
            ScenePersistenceHost::open(&prefabs, "startup.scene.json", &registry).unwrap();
        let mut scene_host = opened_scene.host;
        assert!(matches!(
            scene_host.save_native(SceneSaveCandidate {
                checkpoint: authority.capture(&workspace, document).unwrap(),
            }),
            SceneSaveOutcome::Rejected(EditorPersistenceRejection::DocumentKindMismatch)
        ));
        let scene_document = workspace
            .open_scene_session("startup.scene.json", opened_scene.session)
            .unwrap()
            .opened_document
            .unwrap();
        assert!(matches!(
            host.save_native(PrefabSaveCandidate {
                checkpoint: authority.capture(&workspace, scene_document).unwrap(),
            }),
            PrefabSaveOutcome::Rejected(EditorPersistenceRejection::DocumentKindMismatch)
        ));

        let mut other_host = PrefabPersistenceHost::open(
            &prefabs,
            EditorDocumentSourceBinding::new("other.prefab.json"),
            &registry,
        )
        .unwrap()
        .host;
        assert!(matches!(
            other_host.save_native(PrefabSaveCandidate {
                checkpoint: authority.capture(&workspace, document).unwrap(),
            }),
            PrefabSaveOutcome::Rejected(EditorPersistenceRejection::SourceBindingMismatch)
        ));

        let checkpoint = authority.capture(&workspace, document).unwrap();
        let PrefabSaveOutcome::Saved { commit, evidence } =
            host.save_native(PrefabSaveCandidate { checkpoint })
        else {
            panic!("the canonical Prefab source should publish through the shared receipt lane");
        };
        assert_eq!(evidence.document(), document);
        assert!(authority.commit(&mut workspace, commit).applied);

        let PrefabReopenOutcome::Opened(reopened) = host.reopen_native(&registry) else {
            panic!("the saved Prefab source should reopen through its own lane");
        };
        assert_eq!(reopened.session.document(), &expected);
        host.commit_reopen(reopened.identity, reopened.content_digest);
    }

    #[test]
    fn degraded_scene_save_and_reopen_preserve_the_exact_unavailable_manifest() {
        let root = TestRoot::new();
        let entity = SceneEntityId::new("player").unwrap();
        let owner =
            PersistentComponentSchemaOwnerId::try_new(SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str())
                .unwrap();
        let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity.clone())
            .with_component(
                nara_reflect::ComponentTypeId::new("nara.scene.Name"),
                SceneComponentRecord::new(
                    nara_reflect::ComponentSchemaVersion::ONE,
                    ComponentValue::String("Player".to_owned()),
                )
                .with_owner(owner.clone()),
            )
            .with_component(
                nara_reflect::ComponentTypeId::new("nara.scene.Visibility"),
                SceneComponentRecord::new(
                    nara_reflect::ComponentSchemaVersion::ONE,
                    ComponentValue::String("visible".to_owned()),
                )
                .with_owner(owner),
            )]);
        let encoded = scene.to_json_string().unwrap();
        fs::write(root.scene_path(), &encoded).unwrap();

        let scenes = root.capability();
        let target = scenes
            .open_file(&RelativePath::new("startup.scene.json").unwrap())
            .unwrap();
        let identity = target.identity();
        let content_digest = ContentDigest::of_bytes(encoded.as_bytes());
        drop(target);
        let (bundle, adapter) = scene_bundle_and_name_adapter();
        let published = SceneDocumentCandidate::decode_json_bytes(encoded.as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    nara_asset::AssetRef::path("scenes/startup.scene.json").unwrap(),
                ),
                &bundle,
                &[adapter],
            )
            .unwrap()
            .publish();
        let expected_manifest = published.unavailable_manifest().clone();
        assert_eq!(expected_manifest.record_count(), 1);

        let (mut host, opened_digest) = ScenePersistenceHost::bind_authoring_source(
            &scenes,
            "startup.scene.json",
            identity,
            content_digest,
        )
        .unwrap();
        let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
        let document = workspace
            .open_authoring_scene_document(
                "startup.scene.json",
                EditorDocumentSourceBinding::new("startup.scene.json"),
                published,
            )
            .unwrap()
            .opened_document
            .unwrap();
        assert!(
            authority
                .bind_opened_source_digest(&mut workspace, document, opened_digest)
                .applied
        );
        let base_revision = workspace.scene(document).unwrap().revision();
        let report = workspace.apply_command(
            &ComponentRegistry::new(),
            EditorWorkspaceCommand::ApplyAuthoringScalarEdit {
                document,
                base_revision,
                edit: AuthoringScalarEdit::new(
                    entity,
                    nara_reflect::ComponentTypeId::new("nara.scene.Name"),
                    ComponentFieldId::new("value"),
                    ComponentValue::String("Renamed".to_owned()),
                ),
            },
        );
        assert!(report.applied, "{:#?}", report.diagnostics);
        assert_eq!(
            workspace
                .scene(document)
                .unwrap()
                .session()
                .unavailable_record_manifest(),
            Some(&expected_manifest)
        );

        let bypass_checkpoint = authority.capture(&workspace, document).unwrap();
        assert!(matches!(
            host.save_native(SceneSaveCandidate {
                checkpoint: bypass_checkpoint,
            }),
            SceneSaveOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch)
        ));
        assert_eq!(fs::read(root.scene_path()).unwrap(), encoded.as_bytes());

        let checkpoint = authority.capture(&workspace, document).unwrap();
        let SceneSaveOutcome::Saved { commit, .. } = host.save_authoring(
            SceneSaveCandidate { checkpoint },
            workspace.scene(document).unwrap().session(),
        ) else {
            panic!("the preservation-safe encoded Scene should publish");
        };
        assert!(authority.commit(&mut workspace, commit).applied);
        assert!(matches!(
            host.reopen_native(&ComponentRegistry::new()),
            SceneReopenOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch)
        ));

        let SceneReopenOutcome::Opened(reopened) =
            host.reopen_authoring(workspace.scene(document).unwrap().session())
        else {
            panic!("the saved degraded Scene should reopen through its exact authoring basis");
        };
        assert_eq!(
            reopened.session.unavailable_record_manifest(),
            Some(&expected_manifest)
        );
        assert_eq!(
            reopened.session.document().entities[0]
                .components
                .get(&nara_reflect::ComponentTypeId::new("nara.scene.Name"))
                .unwrap()
                .value,
            ComponentValue::String("Renamed".to_owned())
        );
        host.commit_reopen(reopened.identity, reopened.content_digest);
    }

    fn scene_with_entity(id: &str) -> SceneDocument {
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(SceneEntityId::new(id).unwrap())])
    }

    fn prefab_with_entity(id: &str) -> PrefabDocument {
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new(id).unwrap(),
        )])
    }

    fn scene_bundle_and_name_adapter() -> (
        AuthoringSchemaBundle,
        nara_scene::__private::AuthoringAdapterReceipt,
    ) {
        let resolved =
            nara_reflect::__private::resolve_schema_provider(SCENE_COMPONENTS_SCHEMA_PROVIDER)
                .unwrap();
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new(SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str())
                .unwrap(),
            resolved.current().catalog().clone(),
            resolved
                .predecessor()
                .map(|record| record.catalog().clone()),
        )])
        .unwrap();
        let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_trusted_provider(
            resolved.contribution_receipt(),
            "nara.scene.first-party-authoring-adapter",
            1,
            1,
            [nara_reflect::ComponentTypeId::new("nara.scene.Name")],
        )
        .unwrap();
        (bundle, adapter)
    }

    struct TestRoot {
        path: PathBuf,
    }

    impl TestRoot {
        fn new() -> Self {
            let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nara_persistence_uncertain_{}_{}",
                std::process::id(),
                sequence
            ));
            fs::create_dir_all(&path).unwrap();
            fs::write(
                path.join("startup.scene.json"),
                SceneDocument::default().to_json_string().unwrap(),
            )
            .unwrap();
            fs::write(
                path.join("reference-ui.prefab.json"),
                prefab_with_entity("retry").to_json_string().unwrap(),
            )
            .unwrap();
            fs::write(
                path.join("other.prefab.json"),
                prefab_with_entity("other").to_json_string().unwrap(),
            )
            .unwrap();
            Self { path }
        }

        fn capability(&self) -> DirectoryCapability {
            DirectoryCapability::from_host_handle(
                host_directory(&self.path),
                HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
            )
            .unwrap()
        }

        fn scene_path(&self) -> PathBuf {
            self.path.join("startup.scene.json")
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let temporary_root = std::env::temp_dir().canonicalize().unwrap();
            let test_root = self.path.canonicalize().unwrap();
            assert!(test_root.starts_with(&temporary_root));
            assert!(
                test_root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("nara_persistence_uncertain_"))
            );
            fs::remove_dir_all(test_root).unwrap();
        }
    }

    fn host_directory(path: &Path) -> File {
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;

            const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

            OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ_WRITE_DELETE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(path)
                .unwrap()
        }

        #[cfg(unix)]
        {
            File::open(path).unwrap()
        }
    }
}
