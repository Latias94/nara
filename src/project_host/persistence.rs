use std::{
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_fs::{
    ContentDigest, DirectoryCapability, DurabilityProgress, ExpectedTarget, FileIdentity, FsError,
    LockMode, PublicationAtomicity, PublicationIdentityEvidence, RelativeComponent, RelativePath,
    ReplaceSourceBinding, StageStatus,
};
use nara_reflect::ComponentRegistry;
use nara_scene::{
    SceneAuthoringRevision, SceneAuthoringSession, SceneDocument, SceneDocumentCandidate,
    SceneFileLimits,
};
use nara_tooling::{
    EditorDocumentDigest, EditorDocumentId, EditorPersistenceCheckpoint, EditorPersistenceCommit,
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
    #[cfg(test)]
    reject_next_valid_receipt: bool,
    #[cfg(test)]
    fail_next_save: Option<EditorPersistenceFailureStage>,
}

pub(super) struct OpenedScenePersistence {
    pub host: ScenePersistenceHost,
    pub session: SceneAuthoringSession,
    pub digest: EditorDocumentDigest,
}

pub(super) struct SceneSaveCandidate {
    pub checkpoint: EditorPersistenceCheckpoint,
    pub scene: SceneDocument,
}

pub(super) enum SceneSaveOutcome {
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

pub(super) enum SceneReopenOutcome {
    Opened(Box<OpenedSceneReopen>),
    Rejected(EditorPersistenceRejection),
    Failed(EditorPersistenceFailureStage),
}

pub(super) struct OpenedSceneReopen {
    pub(super) session: SceneAuthoringSession,
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
                #[cfg(test)]
                reject_next_valid_receipt: false,
                #[cfg(test)]
                fail_next_save: None,
            },
            session,
            digest: editor_digest(digest),
        })
    }

    pub(super) fn save(&mut self, candidate: SceneSaveCandidate) -> SceneSaveOutcome {
        if self.uncertain {
            return SceneSaveOutcome::Rejected(EditorPersistenceRejection::RequiresReconcile);
        }
        #[cfg(test)]
        if let Some(stage) = self.fail_next_save.take() {
            return SceneSaveOutcome::Failed(stage);
        }
        let SceneSaveCandidate { checkpoint, scene } = candidate;
        let bytes = match encode_scene(self.encoding, &scene) {
            Ok(bytes) => bytes,
            Err(stage) => return SceneSaveOutcome::Failed(stage),
        };
        if bytes.len() > self.limits.encoded_bytes().get() {
            return SceneSaveOutcome::Failed(EditorPersistenceFailureStage::Encode);
        }
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
        let expected_digest = ContentDigest::of_bytes(&bytes);
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
            return SceneSaveOutcome::PersistenceUncertain { evidence };
        }
        if written_digest != expected_digest {
            self.uncertain = true;
            return SceneSaveOutcome::PersistenceUncertain { evidence };
        }
        let candidate_identity = receipt.candidate_identity();
        let Some(commit) =
            EditorPersistenceCommit::from_publication(checkpoint, self.observed_identity, receipt)
        else {
            self.uncertain = true;
            return SceneSaveOutcome::PersistenceUncertain { evidence };
        };

        self.observed_identity = candidate_identity;
        self.observed_digest = published_digest
            .expect("an accepted persistence commit has published content evidence");
        SceneSaveOutcome::Saved { commit, evidence }
    }

    pub(super) fn reopen(&self, registry: &ComponentRegistry) -> SceneReopenOutcome {
        let file = match self.parent.open_child_file(&self.target) {
            Ok(file) => file,
            Err(error) => {
                return match open_target_failure(error) {
                    SceneSaveOutcome::Rejected(reason) => SceneReopenOutcome::Rejected(reason),
                    SceneSaveOutcome::Failed(stage) => SceneReopenOutcome::Failed(stage),
                    SceneSaveOutcome::Saved { .. }
                    | SceneSaveOutcome::PersistenceUncertain { .. } => {
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
        SceneReopenOutcome::Opened(Box::new(OpenedSceneReopen {
            session,
            digest: editor_digest(digest),
            identity: file.identity(),
            content_digest: digest,
        }))
    }

    pub(super) fn commit_reopen(&mut self, identity: FileIdentity, digest: ContentDigest) {
        self.observed_identity = identity;
        self.observed_digest = digest;
        self.uncertain = false;
    }

    pub(super) fn mark_uncertain(&mut self) {
        self.uncertain = true;
    }

    #[cfg(test)]
    pub(super) fn test_reject_next_valid_receipt(&mut self) {
        self.reject_next_valid_receipt = true;
    }

    #[cfg(test)]
    pub(super) fn test_fail_next_save(&mut self, stage: EditorPersistenceFailureStage) {
        self.fail_next_save = Some(stage);
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
    use nara_scene::{SceneAuthoringSession, SceneEntityId, SceneEntityRecord};
    use nara_tooling::EditorWorkspace;

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
                SceneAuthoringSession::new(SceneDocument::default()),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let revision = workspace.scene(document).unwrap().revision();

        host.reject_next_valid_receipt = true;
        let SceneSaveOutcome::PersistenceUncertain { evidence } = host.save(SceneSaveCandidate {
            checkpoint: authority.capture(&workspace, document).unwrap(),
            scene: first.clone(),
        }) else {
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
        assert!(host.uncertain);
        let first_bytes = fs::read(root.scene_path()).unwrap();

        assert!(matches!(
            host.save(SceneSaveCandidate {
                checkpoint: authority.capture(&workspace, document).unwrap(),
                scene: scene_with_entity("blind-retry"),
            }),
            SceneSaveOutcome::Rejected(EditorPersistenceRejection::RequiresReconcile)
        ));
        assert_eq!(fs::read(root.scene_path()).unwrap(), first_bytes);

        let SceneReopenOutcome::Opened(opened) = host.reopen(&registry) else {
            panic!("the published candidate must reopen");
        };
        let OpenedSceneReopen {
            session,
            identity,
            content_digest,
            ..
        } = *opened;
        assert_eq!(session.document(), &first);
        assert!(host.uncertain);

        host.commit_reopen(identity, content_digest);
        assert!(!host.uncertain);
        assert!(matches!(
            host.save(SceneSaveCandidate {
                checkpoint: authority.capture(&workspace, document).unwrap(),
                scene: scene_with_entity("after-reconcile"),
            }),
            SceneSaveOutcome::Saved { .. }
        ));
    }

    fn scene_with_entity(id: &str) -> SceneDocument {
        SceneDocument::new([SceneEntityRecord::new(SceneEntityId::new(id).unwrap())])
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
