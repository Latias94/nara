use std::{
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(test)]
use std::sync::{Arc, atomic::AtomicBool};

use nara_fs::{
    ContentDigest, DirectoryCapability, ExpectedTarget, FileCapability, FileIdentity, FsError,
    LockMode, RelativeComponent, RelativePath, ReplaceReceipt, StageStatus, TemporaryFileCleanup,
    TemporaryFileFailure,
};

static NEXT_TEMPORARY_NAME: AtomicU64 = AtomicU64::new(1);
const TEMPORARY_NAME_ATTEMPTS: usize = 16;

/// Keeps the exact accepted leaf alive; its digest records an observation, not frozen content.
#[derive(Debug)]
pub(in crate::project_host) struct FilePublicationSource {
    file: FileCapability,
    digest: ContentDigest,
}

impl FilePublicationSource {
    pub(in crate::project_host) const fn identity(&self) -> FileIdentity {
        self.file.identity()
    }

    pub(in crate::project_host) const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::project_host) enum FilePublicationBinding {
    Missing,
    Existing {
        identity: FileIdentity,
        digest: ContentDigest,
    },
}

impl FilePublicationBinding {
    const fn expected_target(self) -> ExpectedTarget {
        match self {
            Self::Missing => ExpectedTarget::Missing,
            Self::Existing { identity, .. } => ExpectedTarget::Identity(identity),
        }
    }

    pub(in crate::project_host) const fn identity(self) -> Option<FileIdentity> {
        match self {
            Self::Missing => None,
            Self::Existing { identity, .. } => Some(identity),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::project_host) enum FilePublicationFailureStage {
    PayloadTooLarge,
    OpenTarget,
    CreateTemporary,
    WriteTemporary,
    SyncTemporary,
    ValidateTemporary,
    ReplaceTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::project_host) enum FilePublicationRejection {
    TargetChanged,
    TargetDeleted,
    LockUnavailable,
    UnsupportedGuarantee,
    RequiresReconcile,
    CleanupPending,
}

#[must_use = "file publication outcomes must be accepted or reconciled explicitly"]
pub(in crate::project_host) enum FilePublicationOutcome {
    Published(AcceptedFilePublication),
    Rejected(FilePublicationRejection),
    Failed(FilePublicationFailureStage),
    Uncertain(FilePublicationEvidence),
}

#[derive(Debug)]
pub(in crate::project_host) struct FilePublicationEvidence {
    receipt: ReplaceReceipt,
    parent_directory_sync: StageStatus,
}

impl FilePublicationEvidence {
    pub(in crate::project_host) const fn receipt(&self) -> &ReplaceReceipt {
        &self.receipt
    }

    pub(in crate::project_host) const fn parent_directory_sync(&self) -> StageStatus {
        self.parent_directory_sync
    }

    pub(in crate::project_host) fn into_parts(self) -> (ReplaceReceipt, StageStatus) {
        (self.receipt, self.parent_directory_sync)
    }
}

#[derive(Debug)]
#[must_use = "a published candidate must be committed or the host marked uncertain"]
pub(in crate::project_host) struct AcceptedFilePublication {
    evidence: FilePublicationEvidence,
    source: FilePublicationSource,
}

impl AcceptedFilePublication {
    pub(in crate::project_host) const fn evidence(&self) -> &FilePublicationEvidence {
        &self.evidence
    }

    pub(in crate::project_host) fn into_parts(
        self,
    ) -> (FilePublicationEvidence, FilePublicationSource) {
        (self.evidence, self.source)
    }
}

#[derive(Debug)]
pub(in crate::project_host) struct FilePublicationHost {
    parent: DirectoryCapability,
    target: RelativeComponent,
    source: Option<FilePublicationSource>,
    byte_limit: u64,
    uncertain: bool,
    cleanup: Option<TemporaryFileCleanup>,
    #[cfg(test)]
    fail_after_write: bool,
    #[cfg(test)]
    block_cleanup: Option<Arc<AtomicBool>>,
}

/// Releases an injected cleanup failure without retaining the publication Host.
///
/// Dropping this control does not release the failure: existing fault-injection callers may
/// deliberately ignore it and later release through the Host's `test_allow_cleanup` method.
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct CleanupTestControl {
    blocked: Arc<AtomicBool>,
}

#[cfg(test)]
impl CleanupTestControl {
    pub(crate) fn blocked() -> Self {
        Self {
            blocked: Arc::new(AtomicBool::new(true)),
        }
    }

    pub(crate) fn release(&self) {
        self.blocked.store(false, Ordering::Release);
    }
}

/// Read-only access to the exact parent capability and target name retained by a publication Host.
pub(in crate::project_host) struct FilePublicationReadSource {
    parent: DirectoryCapability,
    target: RelativeComponent,
}

impl FilePublicationReadSource {
    pub(in crate::project_host) fn open(&self) -> Result<FileCapability, FsError> {
        self.parent.open_child_file(&self.target)
    }
}

impl FilePublicationHost {
    pub(in crate::project_host) fn from_existing(
        parent: DirectoryCapability,
        target: RelativeComponent,
        file: FileCapability,
        digest: ContentDigest,
        byte_limit: u64,
    ) -> Self {
        Self {
            parent,
            target,
            source: Some(FilePublicationSource { file, digest }),
            byte_limit,
            uncertain: false,
            cleanup: None,
            #[cfg(test)]
            fail_after_write: false,
            #[cfg(test)]
            block_cleanup: None,
        }
    }

    /// Rebinds a live observed leaf by name without leaving its object lifetime unprotected.
    pub(in crate::project_host) fn bind_existing(
        root: &DirectoryCapability,
        path: &str,
        observed: &FileCapability,
        expected_digest: ContentDigest,
        byte_limit: u64,
    ) -> Result<Self, FilePublicationFailureStage> {
        let path = RelativePath::new(path).map_err(|_| FilePublicationFailureStage::OpenTarget)?;
        let (parent, target) = root
            .resolve_file_parent(&path)
            .map_err(|_| FilePublicationFailureStage::OpenTarget)?;
        let file = parent
            .open_child_file(&target)
            .map_err(|_| FilePublicationFailureStage::OpenTarget)?;
        if file.identity() != observed.identity() {
            return Err(FilePublicationFailureStage::OpenTarget);
        }
        file.verify_digest(expected_digest, byte_limit)
            .map_err(|_| FilePublicationFailureStage::OpenTarget)?;
        Ok(Self::from_existing(
            parent,
            target,
            file,
            expected_digest,
            byte_limit,
        ))
    }

    pub(in crate::project_host) fn bind_missing(
        root: &DirectoryCapability,
        path: &str,
        byte_limit: u64,
    ) -> Result<Self, FilePublicationRejection> {
        let path =
            RelativePath::new(path).map_err(|_| FilePublicationRejection::UnsupportedGuarantee)?;
        let (parent, target) = root
            .resolve_file_parent(&path)
            .map_err(|_| FilePublicationRejection::UnsupportedGuarantee)?;
        Ok(Self {
            parent,
            target,
            source: None,
            byte_limit,
            uncertain: false,
            cleanup: None,
            #[cfg(test)]
            fail_after_write: false,
            #[cfg(test)]
            block_cleanup: None,
        })
    }

    pub(in crate::project_host) const fn binding(&self) -> FilePublicationBinding {
        match &self.source {
            Some(source) => FilePublicationBinding::Existing {
                identity: source.identity(),
                digest: source.digest(),
            },
            None => FilePublicationBinding::Missing,
        }
    }

    pub(in crate::project_host) const fn uncertain(&self) -> bool {
        self.uncertain
    }

    pub(in crate::project_host) fn parent_identity(&self) -> FileIdentity {
        self.parent.identity()
    }

    /// Returns the last accepted observation without inspecting the filesystem.
    pub(in crate::project_host) fn accepted_source_observation(
        &self,
    ) -> Option<(FileIdentity, Option<FileIdentity>, ContentDigest)> {
        if self.uncertain {
            return None;
        }
        self.source.as_ref().map(|source| {
            (
                source.identity(),
                Some(self.parent.identity()),
                source.digest(),
            )
        })
    }

    pub(in crate::project_host) fn has_cleanup(&self) -> bool {
        self.cleanup.is_some()
    }

    #[cfg(test)]
    pub(in crate::project_host) fn test_accepted_source_bytes(&self) -> Option<Vec<u8>> {
        self.source
            .as_ref()?
            .file
            .read_to_end_bounded(self.byte_limit)
            .ok()
    }

    pub(in crate::project_host) fn retry_cleanup(&mut self) -> bool {
        #[cfg(test)]
        if self
            .block_cleanup
            .as_ref()
            .is_some_and(|blocked| blocked.load(Ordering::Acquire))
            && self.cleanup.is_some()
        {
            return false;
        }
        let Some(cleanup) = self.cleanup.take() else {
            return true;
        };
        match cleanup.discard() {
            Ok(()) => true,
            Err(failure) => {
                self.cleanup = failure.into_parts().1;
                false
            }
        }
    }

    #[cfg(test)]
    pub(in crate::project_host) fn test_fail_after_write(&mut self) -> CleanupTestControl {
        let control = CleanupTestControl::blocked();
        self.test_fail_after_write_with_control(&control);
        control
    }

    #[cfg(test)]
    pub(in crate::project_host) fn test_fail_after_write_with_control(
        &mut self,
        control: &CleanupTestControl,
    ) {
        self.fail_after_write = true;
        self.block_cleanup = Some(Arc::clone(&control.blocked));
    }

    #[cfg(test)]
    pub(in crate::project_host) fn test_allow_cleanup(&mut self) {
        if let Some(blocked) = &self.block_cleanup {
            blocked.store(false, Ordering::Release);
        }
    }

    pub(in crate::project_host) fn preflight(&mut self) -> Result<(), FilePublicationRejection> {
        if !self.retry_cleanup() {
            return Err(FilePublicationRejection::CleanupPending);
        }
        if self.uncertain {
            return Err(FilePublicationRejection::RequiresReconcile);
        }
        self.parent
            .preflight_replace(self.binding().expected_target())
            .map_err(|_| FilePublicationRejection::UnsupportedGuarantee)
    }

    fn retain_failed_cleanup(&mut self, failure: TemporaryFileFailure) -> FsError {
        let (error, cleanup) = failure.into_parts();
        debug_assert!(self.cleanup.is_none());
        self.cleanup = cleanup;
        self.retry_cleanup();
        error
    }

    fn discard_candidate(&mut self, candidate: nara_fs::TemporaryFile) {
        if let Err(failure) = candidate.discard() {
            debug_assert!(self.cleanup.is_none());
            self.cleanup = failure.into_parts().1;
        }
    }

    pub(in crate::project_host) fn read_source(&self) -> FilePublicationReadSource {
        FilePublicationReadSource {
            parent: self.parent.share(),
            target: self.target.clone(),
        }
    }

    pub(in crate::project_host) fn mark_uncertain(&mut self) {
        self.uncertain = true;
    }

    pub(in crate::project_host) fn commit_reconciled(
        &mut self,
        file: FileCapability,
        digest: ContentDigest,
    ) {
        self.source = Some(FilePublicationSource { file, digest });
        self.uncertain = false;
    }

    pub(in crate::project_host) fn commit_published(&mut self, source: FilePublicationSource) {
        self.source = Some(source);
        self.uncertain = false;
    }

    pub(in crate::project_host) fn publish(&mut self, bytes: &[u8]) -> FilePublicationOutcome {
        if let Err(reason) = self.preflight() {
            return FilePublicationOutcome::Rejected(reason);
        }
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > self.byte_limit {
            return FilePublicationOutcome::Failed(FilePublicationFailureStage::PayloadTooLarge);
        }
        let binding = self.binding();
        let target = match binding {
            FilePublicationBinding::Missing => None,
            FilePublicationBinding::Existing { .. } => {
                let target = match self.parent.open_child_file(&self.target) {
                    Ok(target) => target,
                    Err(error) => return open_target_failure(error),
                };
                Some(target)
            }
        };
        let target = match target.map(|target| target.try_lock(LockMode::Exclusive)) {
            Some(Ok(lock)) => Some(lock),
            Some(Err(failure)) => match failure.error() {
                FsError::LockContended => {
                    return FilePublicationOutcome::Rejected(
                        FilePublicationRejection::LockUnavailable,
                    );
                }
                FsError::Unsupported { .. } | FsError::Unproven { .. } => {
                    return FilePublicationOutcome::Rejected(
                        FilePublicationRejection::UnsupportedGuarantee,
                    );
                }
                _ => {
                    return FilePublicationOutcome::Failed(FilePublicationFailureStage::OpenTarget);
                }
            },
            None => None,
        };
        if let FilePublicationBinding::Existing { identity, digest } = binding {
            let target = target
                .as_ref()
                .expect("an existing publication binding opened one target before locking");
            if target.identity() != identity
                || target.verify_digest(digest, self.byte_limit).is_err()
            {
                return FilePublicationOutcome::Rejected(FilePublicationRejection::TargetChanged);
            }
        }

        let (temporary_name, mut temporary) = match create_temporary(&self.parent) {
            Ok(temporary) => temporary,
            Err(failure) => {
                self.retain_failed_cleanup(failure);
                return FilePublicationOutcome::Failed(
                    FilePublicationFailureStage::CreateTemporary,
                );
            }
        };
        if temporary.write_all(bytes).is_err() || temporary.flush().is_err() {
            self.discard_candidate(temporary);
            return FilePublicationOutcome::Failed(FilePublicationFailureStage::WriteTemporary);
        }
        #[cfg(test)]
        if std::mem::take(&mut self.fail_after_write) {
            self.cleanup = Some(temporary.into_cleanup());
            return FilePublicationOutcome::Failed(FilePublicationFailureStage::WriteTemporary);
        }
        let sync = match temporary.sync() {
            Ok(sync) => sync,
            Err(_) => {
                self.discard_candidate(temporary);
                return FilePublicationOutcome::Failed(FilePublicationFailureStage::SyncTemporary);
            }
        };
        if sync.progress().data_synced() != StageStatus::Achieved
            || sync.progress().file_metadata_synced() != StageStatus::Achieved
        {
            self.discard_candidate(temporary);
            return FilePublicationOutcome::Rejected(
                FilePublicationRejection::UnsupportedGuarantee,
            );
        }

        // Pin the exact candidate before replace_temp consumes and closes its writer.
        let digest = ContentDigest::of_bytes(bytes);
        let file = match self.parent.open_child_file(&temporary_name) {
            Ok(file)
                if file.identity() == temporary.identity()
                    && file.verify_digest(digest, self.byte_limit).is_ok() =>
            {
                file
            }
            _ => {
                self.discard_candidate(temporary);
                return FilePublicationOutcome::Failed(
                    FilePublicationFailureStage::ValidateTemporary,
                );
            }
        };
        let source = FilePublicationSource { file, digest };

        let receipt =
            match self
                .parent
                .replace_temp(temporary, &self.target, binding.expected_target())
            {
                Ok(receipt) => receipt,
                Err(failure) => match self.retain_failed_cleanup(failure) {
                    FsError::TargetStateMismatch => {
                        return FilePublicationOutcome::Rejected(
                            FilePublicationRejection::TargetChanged,
                        );
                    }
                    FsError::Unsupported { .. } | FsError::Unproven { .. } => {
                        return FilePublicationOutcome::Rejected(
                            FilePublicationRejection::UnsupportedGuarantee,
                        );
                    }
                    _ => {
                        return FilePublicationOutcome::Failed(
                            FilePublicationFailureStage::ReplaceTarget,
                        );
                    }
                },
            };
        drop(target);

        let parent_directory_sync = self.parent.sync().map_or(StageStatus::Unknown, |receipt| {
            receipt.progress().parent_directory_synced()
        });
        let evidence = FilePublicationEvidence {
            receipt,
            parent_directory_sync,
        };
        if !publication_is_accepted(binding, &evidence.receipt)
            || evidence.receipt.candidate_identity() != source.identity()
            || evidence.receipt.written_content_digest() != source.digest()
            || source
                .file
                .verify_digest(source.digest(), self.byte_limit)
                .is_err()
            || !self
                .parent
                .open_child_file(&self.target)
                .is_ok_and(|current| {
                    current.identity() == source.identity()
                        && current
                            .verify_digest(source.digest(), self.byte_limit)
                            .is_ok()
                })
        {
            self.uncertain = true;
            return FilePublicationOutcome::Uncertain(evidence);
        }

        FilePublicationOutcome::Published(AcceptedFilePublication { evidence, source })
    }
}

fn publication_is_accepted(binding: FilePublicationBinding, receipt: &ReplaceReceipt) -> bool {
    receipt.previous_identity() == binding.identity()
        && receipt.publication_atomicity() == nara_fs::PublicationAtomicity::AtomicNameSwitch
        && (!matches!(binding, FilePublicationBinding::Missing)
            || receipt.conflict_protection() == nara_fs::ConflictProtection::StrongCompareAndSwap)
        && receipt.published_identity() == Some(receipt.candidate_identity())
        && matches!(
            receipt.publication_identity_evidence(),
            nara_fs::PublicationIdentityEvidence::HandleBoundCandidate
                | nara_fs::PublicationIdentityEvidence::PostPublishObserved
        )
        && receipt.durability().data_synced() == StageStatus::Achieved
        && receipt.durability().file_metadata_synced() == StageStatus::Achieved
        && receipt.durability().name_published() == StageStatus::Achieved
        && receipt.verified_content_digest() == Some(receipt.written_content_digest())
}

fn create_temporary(
    parent: &DirectoryCapability,
) -> Result<(RelativeComponent, nara_fs::TemporaryFile), TemporaryFileFailure> {
    for _ in 0..TEMPORARY_NAME_ATTEMPTS {
        let sequence = NEXT_TEMPORARY_NAME.fetch_add(1, Ordering::Relaxed);
        let name = RelativeComponent::new(format!(
            ".nara-publication-{}-{sequence}.tmp",
            std::process::id()
        ))
        .expect("the engine-owned temporary filename is valid");
        match parent.create_temp(&name) {
            Ok(temporary) => return Ok((name, temporary)),
            Err(error) if matches!(error.error(), FsError::AlreadyExists { .. }) => continue,
            Err(error) => return Err(error),
        }
    }
    Err(FsError::AlreadyExists {
        operation: nara_fs::FsOperation::CreateTemporary,
    }
    .into())
}

fn open_target_failure(error: FsError) -> FilePublicationOutcome {
    match error {
        FsError::Io { ref source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            FilePublicationOutcome::Rejected(FilePublicationRejection::TargetDeleted)
        }
        FsError::Unsupported { .. } | FsError::Unproven { .. } => {
            FilePublicationOutcome::Rejected(FilePublicationRejection::UnsupportedGuarantee)
        }
        _ => FilePublicationOutcome::Failed(FilePublicationFailureStage::OpenTarget),
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests {
    #[cfg(target_os = "linux")]
    use std::fs::File;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};

    use super::*;

    const TARGET: &str = "source.json";
    const MOVED: &str = "moved.json";
    const BYTE_LIMIT: u64 = 1024;

    struct Fixture {
        directory: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let sequence = NEXT_TEMPORARY_NAME.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir().join(format!(
                "nara-file-publication-custody-{}-{stamp}-{sequence}",
                std::process::id()
            ));
            // Exclusive creation makes every fixture path below owned by this test.
            fs::create_dir(&directory).unwrap();
            Self { directory }
        }

        fn capability(&self) -> DirectoryCapability {
            #[cfg(windows)]
            let directory = {
                use std::os::windows::fs::OpenOptionsExt;
                use windows_sys::Win32::Storage::FileSystem::{
                    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE,
                    FILE_SHARE_READ, FILE_SHARE_WRITE,
                };

                fs::OpenOptions::new()
                    .read(true)
                    .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                    .open(&self.directory)
                    .unwrap()
            };
            #[cfg(target_os = "linux")]
            let directory = File::open(&self.directory).unwrap();

            DirectoryCapability::from_host_handle(
                directory,
                HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
            )
            .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // Remove only the known fixture leaves, never recursively traverse a path.
            for name in [TARGET, MOVED] {
                match fs::remove_file(self.directory.join(name)) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => panic!("failed to remove fixture leaf {name}: {error}"),
                }
            }
            fs::remove_dir(&self.directory).unwrap();
        }
    }

    #[test]
    fn existing_source_remains_readable_after_rename_and_unlink() {
        let fixture = Fixture::new();
        let bytes = b"accepted original";
        fs::write(fixture.directory.join(TARGET), bytes).unwrap();
        let root = fixture.capability();
        let target = RelativeComponent::new(TARGET).unwrap();
        let file = root.open_child_file(&target).unwrap();
        let identity = file.identity();
        let digest = ContentDigest::of_bytes(&file.read_to_end_bounded(BYTE_LIMIT).unwrap());
        let mut host = FilePublicationHost::from_existing(root, target, file, digest, BYTE_LIMIT);

        fs::rename(
            fixture.directory.join(TARGET),
            fixture.directory.join(MOVED),
        )
        .unwrap();
        fs::write(fixture.directory.join(TARGET), b"different current name").unwrap();
        let source = host.source.as_ref().unwrap();
        assert_eq!(source.identity(), identity);
        assert_eq!(source.digest(), digest);
        assert_eq!(source.file.read_to_end_bounded(BYTE_LIMIT).unwrap(), bytes);
        fs::remove_file(fixture.directory.join(MOVED)).unwrap();
        assert_eq!(source.file.read_to_end_bounded(BYTE_LIMIT).unwrap(), bytes);
        assert_eq!(
            host.binding(),
            FilePublicationBinding::Existing { identity, digest }
        );
        assert_eq!(
            host.accepted_source_observation(),
            Some((identity, Some(host.parent.identity()), digest))
        );

        host.mark_uncertain();
        assert_eq!(host.accepted_source_observation(), None);
        assert_eq!(
            host.binding(),
            FilePublicationBinding::Existing { identity, digest }
        );
        assert_eq!(
            host.source
                .as_ref()
                .unwrap()
                .file
                .read_to_end_bounded(BYTE_LIMIT)
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn bind_existing_validates_and_retains_the_observed_leaf() {
        let fixture = Fixture::new();
        let bytes = b"observed source";
        fs::write(fixture.directory.join(TARGET), bytes).unwrap();
        fs::write(fixture.directory.join(MOVED), bytes).unwrap();
        let root = fixture.capability();
        let file = root
            .open_child_file(&RelativeComponent::new(TARGET).unwrap())
            .unwrap();
        let identity = file.identity();
        let other = root
            .open_child_file(&RelativeComponent::new(MOVED).unwrap())
            .unwrap();
        let digest = ContentDigest::of_bytes(bytes);
        for (observed, expected_digest, limit) in [
            (&other, digest, BYTE_LIMIT),
            (&file, ContentDigest::of_bytes(b"wrong digest"), BYTE_LIMIT),
            (&file, digest, 1),
        ] {
            assert!(matches!(
                FilePublicationHost::bind_existing(&root, TARGET, observed, expected_digest, limit,),
                Err(FilePublicationFailureStage::OpenTarget)
            ));
        }
        let host =
            FilePublicationHost::bind_existing(&root, TARGET, &file, digest, BYTE_LIMIT).unwrap();
        drop((file, other));
        fs::remove_file(fixture.directory.join(TARGET)).unwrap();
        let source = host.source.as_ref().unwrap();
        assert_eq!(source.identity(), identity);
        assert_eq!(source.file.read_to_end_bounded(BYTE_LIMIT).unwrap(), bytes);
    }

    #[test]
    fn reconciled_source_retains_the_fresh_read_handle() {
        let fixture = Fixture::new();
        let root = fixture.capability();
        let mut host = FilePublicationHost::bind_missing(&root, TARGET, BYTE_LIMIT).unwrap();
        assert_eq!(host.binding(), FilePublicationBinding::Missing);
        assert_eq!(host.accepted_source_observation(), None);
        host.mark_uncertain();
        let bytes = b"freshly decoded source";
        fs::write(fixture.directory.join(TARGET), bytes).unwrap();
        let file = host.read_source().open().unwrap();
        let identity = file.identity();
        let digest = ContentDigest::of_bytes(&file.read_to_end_bounded(BYTE_LIMIT).unwrap());
        host.commit_reconciled(file, digest);
        fs::remove_file(fixture.directory.join(TARGET)).unwrap();

        assert!(!host.uncertain());
        assert_eq!(
            host.accepted_source_observation(),
            Some((identity, Some(root.identity()), digest))
        );
        assert_eq!(
            host.source
                .as_ref()
                .unwrap()
                .file
                .read_to_end_bounded(BYTE_LIMIT)
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn published_source_retains_candidate_after_writer_consumption_before_host_commit() {
        let fixture = Fixture::new();
        let root = fixture.capability();
        let mut host = FilePublicationHost::bind_missing(&root, TARGET, BYTE_LIMIT).unwrap();
        let bytes = b"published candidate";
        let accepted = match host.publish(bytes) {
            FilePublicationOutcome::Published(accepted) => accepted,
            FilePublicationOutcome::Rejected(reason) => panic!("publication rejected: {reason:?}"),
            FilePublicationOutcome::Failed(stage) => panic!("publication failed: {stage:?}"),
            FilePublicationOutcome::Uncertain(evidence) => {
                panic!("publication uncertain: {evidence:?}")
            }
        };
        // The receipt identifies the original TemporaryFile, whose writer publish consumed.
        let identity = accepted.evidence().receipt().candidate_identity();
        let (evidence, source) = accepted.into_parts();
        assert_eq!(source.identity(), identity);
        assert_eq!(source.digest(), ContentDigest::of_bytes(bytes));
        assert_eq!(evidence.receipt().published_identity(), Some(identity));
        assert_eq!(host.binding(), FilePublicationBinding::Missing);
        assert!(host.source.is_none());

        fs::rename(
            fixture.directory.join(TARGET),
            fixture.directory.join(MOVED),
        )
        .unwrap();
        fs::write(fixture.directory.join(TARGET), b"unrelated replacement").unwrap();
        assert_eq!(source.file.read_to_end_bounded(BYTE_LIMIT).unwrap(), bytes);
        fs::remove_file(fixture.directory.join(MOVED)).unwrap();
        assert_eq!(source.file.read_to_end_bounded(BYTE_LIMIT).unwrap(), bytes);

        host.commit_published(source);
        let source = host.source.as_ref().unwrap();
        assert_eq!(source.identity(), identity);
        assert_eq!(source.file.read_to_end_bounded(BYTE_LIMIT).unwrap(), bytes);
        assert_eq!(
            host.accepted_source_observation(),
            Some((
                identity,
                Some(root.identity()),
                ContentDigest::of_bytes(bytes)
            ))
        );
    }
}
