pub(super) use super::file_publication::{
    FilePublicationBinding, FilePublicationEvidence, FilePublicationFailureStage,
    FilePublicationHost, FilePublicationOutcome, FilePublicationRejection,
};

#[cfg(test)]
pub(super) use super::file_publication::CleanupTestControl;

use nara_fs::{
    ContentDigest, DirectoryCapability, DurabilityProgress, FileIdentity, FsError,
    PublicationAtomicity, PublicationIdentityEvidence, RelativePath, StageStatus,
};
use nara_reflect::ComponentRegistry;
use nara_scene::{
    AuthoringDocumentReadmission, PrefabAuthoringSession, PrefabDocumentCandidate,
    SceneAuthoringRevision, SceneAuthoringSession, SceneDocument, SceneDocumentCandidate,
    SceneFileLimits,
};
use nara_tooling::{
    EditorDocumentDigest, EditorDocumentId, EditorPersistenceCheckpoint, EditorPersistenceCommit,
    EditorPersistenceFailureStage, EditorPersistenceRejection,
};

mod reopen;
pub(super) use reopen::{
    PrefabReopenRequest, PreparedPrefabReopen, PreparedSceneReopen, SceneReopenRequest,
};

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
    publication: FilePublicationHost,
    encoding: DocumentEncoding,
    limits: SceneFileLimits,
    // One fully authoring-validated candidate may have reached disk without a confirmed receipt.
    validated_authoring_digest: Option<ContentDigest>,
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
                publication: FilePublicationHost::from_existing(
                    parent,
                    target,
                    identity,
                    digest,
                    file_byte_limit(limits),
                ),
                encoding,
                limits,
                validated_authoring_digest: None,
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
            publication: FilePublicationHost::from_existing(
                parent,
                target,
                expected_identity,
                expected_digest,
                file_byte_limit(limits),
            ),
            encoding,
            limits,
            validated_authoring_digest: None,
            #[cfg(test)]
            reject_next_valid_receipt: false,
            #[cfg(test)]
            fail_next_save: None,
        })
    }

    fn preflight_save(&mut self) -> Result<(), PersistenceSaveOutcome> {
        if self.publication.uncertain() {
            return Err(PersistenceSaveOutcome::Rejected(
                EditorPersistenceRejection::RequiresReconcile,
            ));
        }
        #[cfg(test)]
        if let Some(stage) = self.fail_next_save.take() {
            return Err(PersistenceSaveOutcome::Failed(stage));
        }
        self.publication.preflight().map_err(|reason| {
            PersistenceSaveOutcome::Rejected(editor_publication_rejection(reason))
        })
    }

    fn save_preflighted(
        &mut self,
        checkpoint: EditorPersistenceCheckpoint,
        bytes: &[u8],
    ) -> PersistenceSaveOutcome {
        if bytes.len() > self.limits.encoded_bytes().get() {
            return PersistenceSaveOutcome::Failed(EditorPersistenceFailureStage::Encode);
        }

        let previous_identity = match self.publication.binding() {
            FilePublicationBinding::Existing { identity, .. } => identity,
            FilePublicationBinding::Missing => {
                return PersistenceSaveOutcome::Rejected(
                    EditorPersistenceRejection::PreservationMismatch,
                );
            }
        };
        let accepted = match self.publication.publish(bytes) {
            FilePublicationOutcome::Published(accepted) => accepted,
            FilePublicationOutcome::Rejected(reason) => {
                return PersistenceSaveOutcome::Rejected(editor_publication_rejection(reason));
            }
            FilePublicationOutcome::Failed(stage) => {
                return PersistenceSaveOutcome::Failed(editor_publication_failure(stage));
            }
            FilePublicationOutcome::Uncertain(evidence) => {
                return PersistenceSaveOutcome::PersistenceUncertain {
                    evidence: editor_persistence_receipt(&checkpoint, &evidence),
                };
            }
        };
        let evidence = editor_persistence_receipt(&checkpoint, accepted.evidence());

        #[cfg(test)]
        if std::mem::take(&mut self.reject_next_valid_receipt) {
            self.publication.mark_uncertain();
            return PersistenceSaveOutcome::PersistenceUncertain { evidence };
        }

        let (publication_evidence, binding) = accepted.into_parts();
        let (receipt, _) = publication_evidence.into_parts();
        let Some(commit) =
            EditorPersistenceCommit::from_publication(checkpoint, previous_identity, receipt)
        else {
            self.publication.mark_uncertain();
            return PersistenceSaveOutcome::PersistenceUncertain { evidence };
        };

        self.publication.commit_published(binding);
        PersistenceSaveOutcome::Saved { commit, evidence }
    }

    fn commit_reopen(&mut self, identity: FileIdentity, digest: ContentDigest) {
        self.publication.commit_reconciled(identity, digest);
        self.validated_authoring_digest = None;
    }

    fn mark_uncertain(&mut self) {
        self.publication.mark_uncertain();
    }

    #[cfg(test)]
    fn uncertain(&self) -> bool {
        self.publication.uncertain()
    }

    #[cfg(test)]
    fn observed_identity(&self) -> FileIdentity {
        match self.publication.binding() {
            FilePublicationBinding::Existing { identity, .. } => identity,
            FilePublicationBinding::Missing => {
                unreachable!("document persistence is always bound to an existing source")
            }
        }
    }

    fn observed_digest(&self) -> ContentDigest {
        match self.publication.binding() {
            FilePublicationBinding::Existing { digest, .. } => digest,
            FilePublicationBinding::Missing => {
                unreachable!("document persistence is always bound to an existing source")
            }
        }
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
    #[cfg(test)]
    pub(super) fn test_fail_after_write(&mut self) -> CleanupTestControl {
        self.file.publication.test_fail_after_write()
    }

    #[cfg(test)]
    pub(super) fn test_allow_cleanup(&mut self) {
        self.file.publication.test_allow_cleanup();
    }

    pub(super) fn retry_cleanup(&mut self) -> bool {
        self.file.publication.retry_cleanup()
    }

    pub(super) fn open(
        scenes: &DirectoryCapability,
        scene_path: &str,
        registry: &ComponentRegistry,
    ) -> Result<OpenedScenePersistence, EditorPersistenceFailureStage> {
        let limits = SceneFileLimits::default();
        let (file, bytes) = PersistenceFileState::open(scenes, scene_path, limits)?;
        let session = decode_scene_session(file.encoding, &bytes, limits, registry)?;
        Ok(OpenedScenePersistence {
            digest: editor_digest(file.observed_digest()),
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

    pub(super) fn save_owned(
        &mut self,
        candidate: SceneSaveCandidate,
        authoring: Option<AuthoringDocumentReadmission>,
    ) -> SceneSaveOutcome {
        if (self.lane == PersistenceDocumentLane::Authoring) != authoring.is_some() {
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
        if let Some(authoring) = authoring {
            match validate_authoring_scene_save(
                self.file.encoding,
                &bytes,
                self.file.limits,
                &authoring,
            ) {
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
            self.file.validated_authoring_digest = Some(ContentDigest::of_bytes(&bytes));
        }
        let outcome = self.file.save_preflighted(checkpoint, &bytes);
        if !matches!(outcome, PersistenceSaveOutcome::PersistenceUncertain { .. }) {
            self.file.validated_authoring_digest = None;
        }
        outcome
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
    #[cfg(test)]
    pub(super) fn test_fail_after_write(&mut self) -> CleanupTestControl {
        self.file.publication.test_fail_after_write()
    }

    #[cfg(test)]
    pub(super) fn test_allow_cleanup(&mut self) {
        self.file.publication.test_allow_cleanup();
    }

    pub(super) fn retry_cleanup(&mut self) -> bool {
        self.file.publication.retry_cleanup()
    }

    pub(super) fn open(
        prefabs: &DirectoryCapability,
        source_binding: nara_tooling::EditorDocumentSourceBinding,
        registry: &ComponentRegistry,
    ) -> Result<OpenedPrefabPersistence, EditorPersistenceFailureStage> {
        let limits = SceneFileLimits::default();
        let (file, bytes) = PersistenceFileState::open(prefabs, source_binding.locator(), limits)?;
        let session = decode_prefab_session(file.encoding, &bytes, limits, registry)?;
        Ok(OpenedPrefabPersistence {
            digest: editor_digest(file.observed_digest()),
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

    pub(super) fn save_owned(
        &mut self,
        candidate: PrefabSaveCandidate,
        authoring: Option<AuthoringDocumentReadmission>,
    ) -> PrefabSaveOutcome {
        if (self.lane == PersistenceDocumentLane::Authoring) != authoring.is_some() {
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
        if let Some(authoring) = authoring {
            match validate_authoring_prefab_save(
                self.file.encoding,
                &bytes,
                self.file.limits,
                &authoring,
            ) {
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
            self.file.validated_authoring_digest = Some(ContentDigest::of_bytes(&bytes));
        }
        let outcome = self.file.save_preflighted(checkpoint, &bytes);
        if !matches!(outcome, PersistenceSaveOutcome::PersistenceUncertain { .. }) {
            self.file.validated_authoring_digest = None;
        }
        outcome
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

enum AuthoringSaveValidationError {
    Decode(EditorPersistenceFailureStage),
    PreservationMismatch,
}

fn validate_authoring_scene_save(
    encoding: DocumentEncoding,
    bytes: &[u8],
    limits: SceneFileLimits,
    authoring: &AuthoringDocumentReadmission,
) -> Result<(), AuthoringSaveValidationError> {
    let candidate = match encoding {
        DocumentEncoding::Json => {
            SceneDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
        }
        DocumentEncoding::Ron => {
            SceneDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
        }
    }
    .map_err(|_| AuthoringSaveValidationError::Decode(EditorPersistenceFailureStage::Decode))?;
    let reopened = authoring.reopen_scene_candidate(candidate).map_err(|_| {
        AuthoringSaveValidationError::Decode(EditorPersistenceFailureStage::Validate)
    })?;
    if !authoring.preserves_scene(&reopened) {
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

fn validate_authoring_prefab_save(
    encoding: DocumentEncoding,
    bytes: &[u8],
    limits: SceneFileLimits,
    authoring: &AuthoringDocumentReadmission,
) -> Result<(), AuthoringSaveValidationError> {
    let candidate = match encoding {
        DocumentEncoding::Json => {
            PrefabDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
        }
        DocumentEncoding::Ron => {
            PrefabDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
        }
    }
    .map_err(|_| AuthoringSaveValidationError::Decode(EditorPersistenceFailureStage::Decode))?;
    let reopened = authoring.reopen_prefab_candidate(candidate).map_err(|_| {
        AuthoringSaveValidationError::Decode(EditorPersistenceFailureStage::Validate)
    })?;
    if !authoring.preserves_prefab(&reopened) {
        return Err(AuthoringSaveValidationError::PreservationMismatch);
    }
    Ok(())
}

fn editor_persistence_receipt(
    checkpoint: &EditorPersistenceCheckpoint,
    evidence: &FilePublicationEvidence,
) -> EditorPersistenceReceipt {
    let receipt = evidence.receipt();
    EditorPersistenceReceipt {
        document: checkpoint.document(),
        revision: checkpoint.revision(),
        digest: editor_digest(receipt.written_content_digest()),
        published_digest: receipt.published_content_digest().map(editor_digest),
        previous_identity: receipt.previous_identity(),
        candidate_identity: receipt.candidate_identity(),
        published_identity: receipt.published_identity(),
        identity_evidence: receipt.publication_identity_evidence(),
        publication: receipt.publication_atomicity(),
        durability: receipt.durability(),
        parent_directory_sync: evidence.parent_directory_sync(),
    }
}

const fn editor_publication_rejection(
    rejection: FilePublicationRejection,
) -> EditorPersistenceRejection {
    match rejection {
        FilePublicationRejection::TargetChanged => EditorPersistenceRejection::TargetChanged,
        FilePublicationRejection::TargetDeleted => EditorPersistenceRejection::TargetDeleted,
        FilePublicationRejection::LockUnavailable => EditorPersistenceRejection::LockUnavailable,
        FilePublicationRejection::UnsupportedGuarantee => {
            EditorPersistenceRejection::UnsupportedGuarantee
        }
        FilePublicationRejection::RequiresReconcile => {
            EditorPersistenceRejection::RequiresReconcile
        }
        FilePublicationRejection::CleanupPending => EditorPersistenceRejection::CleanupPending,
    }
}

const fn editor_publication_failure(
    failure: FilePublicationFailureStage,
) -> EditorPersistenceFailureStage {
    match failure {
        FilePublicationFailureStage::PayloadTooLarge => EditorPersistenceFailureStage::Encode,
        FilePublicationFailureStage::OpenTarget => EditorPersistenceFailureStage::OpenTarget,
        FilePublicationFailureStage::CreateTemporary => {
            EditorPersistenceFailureStage::CreateTemporary
        }
        FilePublicationFailureStage::WriteTemporary => {
            EditorPersistenceFailureStage::WriteTemporary
        }
        FilePublicationFailureStage::SyncTemporary => EditorPersistenceFailureStage::SyncTemporary,
        FilePublicationFailureStage::ReplaceTarget => EditorPersistenceFailureStage::ReplaceTarget,
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
        sync::{Arc, Mutex},
    };

    #[cfg(windows)]
    use std::fs::OpenOptions;

    use nara_fs::{CapabilityRights, HostCapabilityOptions, LockMode, TrustMode};
    use nara_reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentFieldId, ComponentValue,
        PersistentComponentSchemaOwnerId,
    };
    use nara_scene::{
        AuthoringDocumentSource, AuthoringFieldEdit, PrefabDocument,
        SCENE_COMPONENTS_SCHEMA_OWNER_ID, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneAuthoringSession,
        SceneComponentRecord, SceneEntityId, SceneEntityRecord,
    };
    use nara_tooling::{EditorDocumentSourceBinding, EditorWorkspace, EditorWorkspaceCommand};

    static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

    #[derive(nara_ecs::Component)]
    struct ReopenThreadProbe(i64);

    #[test]
    fn native_reopen_prepares_on_a_worker_but_runs_codecs_and_migrations_only_on_owner_admission() {
        use nara_reflect::{
            ComponentCapability, ComponentFieldPath, ComponentFieldSchema, ComponentSchema,
            ComponentSchemaVersion, ComponentTypeId, ComponentValueKind,
        };
        let root = TestRoot::new();
        let directory = root.capability();
        let callbacks = Arc::new(Mutex::new(Vec::new()));
        let mut registry = ComponentRegistry::new();
        let component = ComponentTypeId::new("nara.test.ReopenThreadProbe");
        let decode_calls = Arc::clone(&callbacks);
        let encode_calls = Arc::clone(&callbacks);
        registry
            .register_persistent_component_with_codec::<ReopenThreadProbe, _, _>(
                ComponentSchema::new(
                    component.clone(),
                    "Reopen thread probe",
                    ComponentSchemaVersion(2),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("value"),
                    "Value",
                    ComponentFieldPath::from_fields(["value"]),
                    ComponentValueKind::I64,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
                move |value| {
                    decode_calls
                        .lock()
                        .unwrap()
                        .push(("decode", std::thread::current().id()));
                    Ok(ReopenThreadProbe(value.field_i64("value")?))
                },
                move |value| {
                    encode_calls
                        .lock()
                        .unwrap()
                        .push(("encode", std::thread::current().id()));
                    Ok(ComponentValue::map([(
                        "value",
                        ComponentValue::I64(value.0),
                    )]))
                },
            )
            .unwrap();
        let migrate_calls = Arc::clone(&callbacks);
        registry
            .register_component_migration(
                &component,
                ComponentSchemaVersion::ONE,
                ComponentSchemaVersion(2),
                move |value| {
                    migrate_calls
                        .lock()
                        .unwrap()
                        .push(("migration", std::thread::current().id()));
                    Ok(value)
                },
            )
            .unwrap();
        registry.freeze().unwrap();
        let scene_host = ScenePersistenceHost::open(&directory, "startup.scene.json", &registry)
            .unwrap()
            .host;
        let prefab_host = PrefabPersistenceHost::open(
            &directory,
            EditorDocumentSourceBinding::new("reference-ui.prefab.json"),
            &registry,
        )
        .unwrap()
        .host;
        let scene_identity = scene_host.file.observed_identity();
        let scene_digest = scene_host.file.observed_digest();
        let entity = SceneEntityRecord::new(SceneEntityId::new("native-probe").unwrap())
            .with_component(
                component.clone(),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("value", ComponentValue::I64(7))]),
                ),
            );
        fs::write(
            root.scene_path(),
            SceneDocument::from_ordered_roots([entity.clone()])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            root.path.join("reference-ui.prefab.json"),
            PrefabDocument::from_ordered_roots([entity])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let scene_request = scene_host.prepare_reopen(None).unwrap();
        let prefab_request = prefab_host.prepare_reopen(None).unwrap();
        let (scene, prefab) =
            std::thread::spawn(move || (scene_request.prepare(), prefab_request.prepare()))
                .join()
                .unwrap();
        assert!(
            callbacks.lock().unwrap().is_empty(),
            "worker preparation cannot execute native behavior"
        );
        assert_eq!(scene_host.file.observed_identity(), scene_identity);
        assert_eq!(scene_host.file.observed_digest(), scene_digest);
        let SceneReopenOutcome::Opened(scene) = scene.admit(Some(&registry)) else {
            panic!("owner must admit the valid Scene")
        };
        let PrefabReopenOutcome::Opened(prefab) = prefab.admit(Some(&registry)) else {
            panic!("owner must admit the valid Prefab")
        };
        assert_eq!(
            scene.session.document().entities[0].components[&component].version,
            ComponentSchemaVersion(2)
        );
        assert_eq!(
            prefab.session.document().entities[0].components[&component].version,
            ComponentSchemaVersion(2)
        );
        let callbacks = callbacks.lock().unwrap();
        assert!(callbacks.iter().any(|(kind, _)| *kind == "migration"));
        assert!(callbacks.iter().any(|(kind, _)| *kind == "decode"));
        assert!(
            callbacks
                .iter()
                .all(|(_, thread)| *thread == std::thread::current().id())
        );
    }

    #[test]
    fn authoring_scene_and_prefab_reopen_prepare_independently_without_mutating_the_original_hosts()
    {
        let root = TestRoot::new();
        let directory = root.capability();
        let scene_bytes = fs::read(root.scene_path()).unwrap();
        let prefab_bytes = fs::read(root.path.join("reference-ui.prefab.json")).unwrap();
        let bundle = AuthoringSchemaBundle::new([]).unwrap();
        let scene = SceneDocumentCandidate::decode_json_bytes(&scene_bytes)
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    nara_asset::AssetRef::path("scenes/startup.scene.json").unwrap(),
                ),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
        let prefab = PrefabDocumentCandidate::decode_json_bytes(&prefab_bytes)
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    nara_asset::AssetRef::path("prefabs/reference-ui.prefab.json").unwrap(),
                ),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
        let scene_basis =
            SceneAuthoringSession::from_published_authoring_document(scene).authoring_readmission();
        let prefab_basis = PrefabAuthoringSession::from_published_authoring_document(prefab)
            .authoring_readmission();
        let scene_file = directory
            .open_file(&RelativePath::new("startup.scene.json").unwrap())
            .unwrap();
        let prefab_file = directory
            .open_file(&RelativePath::new("reference-ui.prefab.json").unwrap())
            .unwrap();
        let (mut scene_host, _) = ScenePersistenceHost::bind_authoring_source(
            &directory,
            "startup.scene.json",
            scene_file.identity(),
            ContentDigest::of_bytes(&scene_bytes),
        )
        .unwrap();
        let (mut prefab_host, _) = PrefabPersistenceHost::bind_authoring_source(
            &directory,
            EditorDocumentSourceBinding::new("reference-ui.prefab.json"),
            prefab_file.identity(),
            ContentDigest::of_bytes(&prefab_bytes),
        )
        .unwrap();
        scene_host.mark_uncertain();
        prefab_host.mark_uncertain();
        let scene_request = scene_host.prepare_reopen(scene_basis).unwrap();
        let prefab_request = prefab_host.prepare_reopen(prefab_basis).unwrap();
        let (scene, prefab) =
            std::thread::spawn(move || (scene_request.prepare(), prefab_request.prepare()))
                .join()
                .unwrap();
        assert!(scene_host.file.uncertain());
        assert!(prefab_host.file.uncertain());
        let SceneReopenOutcome::Opened(scene) = scene.admit(None) else {
            panic!("pure Scene classification must prepare")
        };
        let PrefabReopenOutcome::Opened(prefab) = prefab.admit(None) else {
            panic!("pure Prefab classification must prepare")
        };
        assert!(scene_host.file.uncertain());
        assert!(prefab_host.file.uncertain());
        scene_host.commit_reopen(scene.identity, scene.content_digest);
        prefab_host.commit_reopen(prefab.identity, prefab.content_digest);
        assert!(!scene_host.file.uncertain());
        assert!(!prefab_host.file.uncertain());
        assert_eq!(fs::read(root.scene_path()).unwrap(), scene_bytes);
        assert_eq!(
            fs::read(root.path.join("reference-ui.prefab.json")).unwrap(),
            prefab_bytes
        );
    }

    #[cfg(windows)]
    #[test]
    fn failed_write_retains_one_candidate_until_cleanup_retry() {
        let root = TestRoot::new();
        let capability = root.capability();
        let mut publication =
            FilePublicationHost::bind_missing(&capability, "import.png.meta", 1024).unwrap();
        let initial_entries = fs::read_dir(&root.path).unwrap().count();
        publication.test_fail_after_write();

        assert!(matches!(
            publication.publish(b"metadata"),
            FilePublicationOutcome::Failed(FilePublicationFailureStage::WriteTemporary)
        ));
        assert!(publication.has_cleanup());
        assert!(!root.path.join("import.png.meta").exists());
        assert_eq!(
            fs::read_dir(&root.path).unwrap().count(),
            initial_entries + 1
        );
        assert!(matches!(
            publication.publish(b"retry"),
            FilePublicationOutcome::Rejected(FilePublicationRejection::CleanupPending)
        ));
        assert_eq!(
            fs::read_dir(&root.path).unwrap().count(),
            initial_entries + 1
        );

        publication.test_allow_cleanup();
        assert!(publication.retry_cleanup());
        assert!(!publication.has_cleanup());
        assert_eq!(fs::read_dir(&root.path).unwrap().count(), initial_entries);
        assert!(matches!(
            publication.publish(b"retry"),
            FilePublicationOutcome::Published(_)
        ));
        assert_eq!(
            fs::read(root.path.join("import.png.meta")).unwrap(),
            b"retry"
        );
    }

    #[test]
    fn file_publication_preserves_missing_and_existing_target_expectations() {
        let root = TestRoot::new();
        let capability = root.capability();
        let target = root.path.join("new-image.png.meta");
        let first = b"first canonical metadata";
        let second = b"second canonical metadata";
        let mut publication =
            FilePublicationHost::bind_missing(&capability, "new-image.png.meta", 1024).unwrap();

        let FilePublicationOutcome::Published(accepted) = publication.publish(first) else {
            panic!("an absent target must accept one expected-missing publication");
        };
        assert_eq!(accepted.evidence().receipt().previous_identity(), None);
        let (_, binding) = accepted.into_parts();
        publication.commit_published(binding);
        assert_eq!(fs::read(&target).unwrap(), first);

        let FilePublicationOutcome::Published(accepted) = publication.publish(second) else {
            panic!("the accepted successor binding must support exact replacement");
        };
        assert_eq!(
            accepted.evidence().receipt().previous_identity(),
            publication.binding().identity()
        );
        let (_, binding) = accepted.into_parts();
        publication.commit_published(binding);
        assert_eq!(fs::read(&target).unwrap(), second);

        let raced_target = root.path.join("raced-image.png.meta");
        let mut raced =
            FilePublicationHost::bind_missing(&capability, "raced-image.png.meta", 1024).unwrap();
        fs::write(&raced_target, b"external metadata").unwrap();
        assert!(matches!(
            raced.publish(b"editor metadata"),
            FilePublicationOutcome::Rejected(FilePublicationRejection::TargetChanged)
        ));
        assert_eq!(fs::read(raced_target).unwrap(), b"external metadata");
    }

    #[test]
    fn existing_file_publication_locks_before_revalidating_the_binding() {
        let root = TestRoot::new();
        let capability = root.capability();
        let target_name = "new-image.png.meta";
        let target_path = RelativePath::new(target_name).unwrap();
        let target = root.path.join(target_name);
        fs::write(&target, b"catalog metadata").unwrap();
        let opened = capability.open_file(&target_path).unwrap();
        let identity = opened.identity();
        let digest = opened.digest(1024).unwrap();
        drop(opened);
        let mut publication =
            FilePublicationHost::bind_existing(&capability, target_name, identity, digest, 1024)
                .unwrap();

        fs::write(&target, b"externally changed metadata").unwrap();
        let locked = capability.open_file(&target_path).unwrap();
        let _lock = locked.try_lock(LockMode::Exclusive).unwrap();

        assert!(matches!(
            publication.publish(b"editor metadata"),
            FilePublicationOutcome::Rejected(FilePublicationRejection::LockUnavailable)
        ));
        drop(_lock);
        drop(locked);
        assert_eq!(fs::read(target).unwrap(), b"externally changed metadata");
    }

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
        let SceneSaveOutcome::PersistenceUncertain { evidence } = host.save_owned(
            SceneSaveCandidate {
                checkpoint: authority.capture(&workspace, document).unwrap(),
            },
            None,
        ) else {
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
        assert!(host.file.uncertain());
        let first_bytes = fs::read(root.scene_path()).unwrap();

        assert!(matches!(
            host.save_owned(
                SceneSaveCandidate {
                    checkpoint: authority.capture(&workspace, document).unwrap(),
                },
                None
            ),
            SceneSaveOutcome::Rejected(EditorPersistenceRejection::RequiresReconcile)
        ));
        assert_eq!(fs::read(root.scene_path()).unwrap(), first_bytes);

        let SceneReopenOutcome::Opened(opened) = host
            .prepare_reopen(None)
            .unwrap()
            .prepare()
            .admit(Some(&registry))
        else {
            panic!("the published candidate must reopen");
        };
        let OpenedSceneReopen {
            session,
            identity,
            content_digest,
            ..
        } = *opened;
        assert_eq!(session.document(), &first);
        assert!(host.file.uncertain());

        host.commit_reopen(identity, content_digest);
        assert!(!host.file.uncertain());
        assert!(matches!(
            host.save_owned(
                SceneSaveCandidate {
                    checkpoint: authority.capture(&workspace, document).unwrap(),
                },
                None
            ),
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
            scene_host.save_owned(
                SceneSaveCandidate {
                    checkpoint: authority.capture(&workspace, document).unwrap(),
                },
                None
            ),
            SceneSaveOutcome::Rejected(EditorPersistenceRejection::DocumentKindMismatch)
        ));
        let scene_document = workspace
            .open_scene_session("startup.scene.json", opened_scene.session)
            .unwrap()
            .opened_document
            .unwrap();
        assert!(matches!(
            host.save_owned(
                PrefabSaveCandidate {
                    checkpoint: authority.capture(&workspace, scene_document).unwrap(),
                },
                None
            ),
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
            other_host.save_owned(
                PrefabSaveCandidate {
                    checkpoint: authority.capture(&workspace, document).unwrap(),
                },
                None
            ),
            PrefabSaveOutcome::Rejected(EditorPersistenceRejection::SourceBindingMismatch)
        ));

        let checkpoint = authority.capture(&workspace, document).unwrap();
        let PrefabSaveOutcome::Saved { commit, evidence } =
            host.save_owned(PrefabSaveCandidate { checkpoint }, None)
        else {
            panic!("the canonical Prefab source should publish through the shared receipt lane");
        };
        assert_eq!(evidence.document(), document);
        assert!(authority.commit(&mut workspace, commit).applied);

        let PrefabReopenOutcome::Opened(reopened) = host
            .prepare_reopen(None)
            .unwrap()
            .prepare()
            .admit(Some(&registry))
        else {
            panic!("the saved Prefab source should reopen through its own lane");
        };
        assert_eq!(reopened.session.document(), &expected);
        host.commit_reopen(reopened.identity, reopened.content_digest);
    }

    #[test]
    fn degraded_scene_save_and_reopen_require_the_exact_unavailable_manifest() {
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
            EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
                document,
                base_revision,
                edit: AuthoringFieldEdit::single(
                    entity,
                    nara_reflect::ComponentTypeId::new("nara.scene.Name"),
                    nara_reflect::ComponentSchemaVersion::ONE,
                    ComponentFieldId::new("value"),
                    ComponentValue::String("Renamed".to_owned()),
                )
                .unwrap(),
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
            host.save_owned(
                SceneSaveCandidate {
                    checkpoint: bypass_checkpoint,
                },
                None
            ),
            SceneSaveOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch)
        ));
        assert_eq!(fs::read(root.scene_path()).unwrap(), encoded.as_bytes());

        let checkpoint = authority.capture(&workspace, document).unwrap();
        let SceneSaveOutcome::Saved { commit, .. } = host.save_owned(
            SceneSaveCandidate { checkpoint },
            workspace
                .scene(document)
                .unwrap()
                .session()
                .authoring_readmission(),
        ) else {
            panic!("the preservation-safe encoded Scene should publish");
        };
        assert!(authority.commit(&mut workspace, commit).applied);
        assert!(matches!(
            host.prepare_reopen(None),
            Err(EditorPersistenceRejection::PreservationMismatch)
        ));

        let SceneReopenOutcome::Opened(reopened) = host
            .prepare_reopen(
                workspace
                    .scene(document)
                    .unwrap()
                    .session()
                    .authoring_readmission(),
            )
            .unwrap()
            .prepare()
            .admit(None)
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

        let observed_identity = host.file.observed_identity();
        let observed_digest = host.file.observed_digest();
        let mut manifest_drift = scene.clone();
        assert!(
            manifest_drift.entities[0]
                .components
                .remove(&nara_reflect::ComponentTypeId::new("nara.scene.Visibility"))
                .is_some()
        );
        fs::write(root.scene_path(), manifest_drift.to_json_string().unwrap()).unwrap();

        assert!(matches!(
            host.prepare_reopen(
                workspace
                    .scene(document)
                    .unwrap()
                    .session()
                    .authoring_readmission()
            )
            .unwrap()
            .prepare()
            .admit(None),
            SceneReopenOutcome::Rejected(EditorPersistenceRejection::PreservationMismatch)
        ));
        assert_eq!(host.file.observed_identity(), observed_identity);
        assert_eq!(host.file.observed_digest(), observed_digest);
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
