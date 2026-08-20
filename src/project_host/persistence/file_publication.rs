use std::{
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_fs::{
    ContentDigest, DirectoryCapability, ExpectedTarget, FileCapability, FileIdentity, FsError,
    LockMode, RelativeComponent, RelativePath, ReplaceReceipt, StageStatus,
};

static NEXT_TEMPORARY_NAME: AtomicU64 = AtomicU64::new(1);
const TEMPORARY_NAME_ATTEMPTS: usize = 16;

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
    ReplaceTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::project_host) enum FilePublicationRejection {
    TargetChanged,
    TargetDeleted,
    LockUnavailable,
    UnsupportedGuarantee,
    RequiresReconcile,
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
    binding: FilePublicationBinding,
}

impl AcceptedFilePublication {
    pub(in crate::project_host) const fn evidence(&self) -> &FilePublicationEvidence {
        &self.evidence
    }

    pub(in crate::project_host) fn into_parts(
        self,
    ) -> (FilePublicationEvidence, FilePublicationBinding) {
        (self.evidence, self.binding)
    }
}

#[derive(Debug)]
pub(in crate::project_host) struct FilePublicationHost {
    parent: DirectoryCapability,
    target: RelativeComponent,
    binding: FilePublicationBinding,
    byte_limit: u64,
    uncertain: bool,
}

impl FilePublicationHost {
    pub(in crate::project_host) fn from_existing(
        parent: DirectoryCapability,
        target: RelativeComponent,
        identity: FileIdentity,
        digest: ContentDigest,
        byte_limit: u64,
    ) -> Self {
        Self {
            parent,
            target,
            binding: FilePublicationBinding::Existing { identity, digest },
            byte_limit,
            uncertain: false,
        }
    }

    pub(in crate::project_host) fn bind_existing(
        root: &DirectoryCapability,
        path: &str,
        expected_identity: FileIdentity,
        expected_digest: ContentDigest,
        byte_limit: u64,
    ) -> Result<Self, FilePublicationFailureStage> {
        let path = RelativePath::new(path).map_err(|_| FilePublicationFailureStage::OpenTarget)?;
        let (parent, target) = root
            .resolve_file_parent(&path)
            .map_err(|_| FilePublicationFailureStage::OpenTarget)?;
        Ok(Self {
            parent,
            target,
            binding: FilePublicationBinding::Existing {
                identity: expected_identity,
                digest: expected_digest,
            },
            byte_limit,
            uncertain: false,
        })
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
            binding: FilePublicationBinding::Missing,
            byte_limit,
            uncertain: false,
        })
    }

    pub(in crate::project_host) const fn binding(&self) -> FilePublicationBinding {
        self.binding
    }

    pub(in crate::project_host) const fn uncertain(&self) -> bool {
        self.uncertain
    }

    pub(in crate::project_host) fn open_target(&self) -> Result<FileCapability, FsError> {
        self.parent.open_child_file(&self.target)
    }

    pub(in crate::project_host) fn mark_uncertain(&mut self) {
        self.uncertain = true;
    }

    pub(in crate::project_host) fn commit_reconciled(
        &mut self,
        identity: FileIdentity,
        digest: ContentDigest,
    ) {
        self.binding = FilePublicationBinding::Existing { identity, digest };
        self.uncertain = false;
    }

    pub(in crate::project_host) fn commit_published(&mut self, binding: FilePublicationBinding) {
        debug_assert!(matches!(binding, FilePublicationBinding::Existing { .. }));
        self.binding = binding;
        self.uncertain = false;
    }

    pub(in crate::project_host) fn publish(&mut self, bytes: &[u8]) -> FilePublicationOutcome {
        if self.uncertain {
            return FilePublicationOutcome::Rejected(FilePublicationRejection::RequiresReconcile);
        }
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > self.byte_limit {
            return FilePublicationOutcome::Failed(FilePublicationFailureStage::PayloadTooLarge);
        }
        let matrix = nara_fs::platform_capability_matrix();
        if matrix.publication_atomicity() != nara_fs::PublicationAtomicity::AtomicNameSwitch
            || matrix.replace_source_binding() == nara_fs::PublicationSourceBinding::Unsupported
        {
            return FilePublicationOutcome::Rejected(
                FilePublicationRejection::UnsupportedGuarantee,
            );
        }

        let target = match self.binding {
            FilePublicationBinding::Missing => None,
            FilePublicationBinding::Existing { .. } => {
                let target = match self.parent.open_child_file(&self.target) {
                    Ok(target) => target,
                    Err(error) => return open_target_failure(error),
                };
                Some(target)
            }
        };
        let lock = match target
            .as_ref()
            .map(|target| target.try_lock(LockMode::Exclusive))
        {
            Some(Ok(lock)) => Some(lock),
            Some(Err(FsError::LockContended)) => {
                return FilePublicationOutcome::Rejected(FilePublicationRejection::LockUnavailable);
            }
            Some(Err(FsError::Unsupported { .. } | FsError::Unproven { .. })) => {
                return FilePublicationOutcome::Rejected(
                    FilePublicationRejection::UnsupportedGuarantee,
                );
            }
            Some(Err(_)) => {
                return FilePublicationOutcome::Failed(FilePublicationFailureStage::OpenTarget);
            }
            None => None,
        };
        if let FilePublicationBinding::Existing { identity, digest } = self.binding {
            let target = target
                .as_ref()
                .expect("an existing publication binding opened one target before locking");
            if target.identity() != identity
                || target.verify_digest(digest, self.byte_limit).is_err()
            {
                return FilePublicationOutcome::Rejected(FilePublicationRejection::TargetChanged);
            }
        }

        let mut temporary = match create_temporary(&self.parent) {
            Ok(temporary) => temporary,
            Err(_) => {
                return FilePublicationOutcome::Failed(
                    FilePublicationFailureStage::CreateTemporary,
                );
            }
        };
        if temporary.write_all(bytes).is_err() || temporary.flush().is_err() {
            return FilePublicationOutcome::Failed(FilePublicationFailureStage::WriteTemporary);
        }
        let sync = match temporary.sync() {
            Ok(sync) => sync,
            Err(_) => {
                return FilePublicationOutcome::Failed(FilePublicationFailureStage::SyncTemporary);
            }
        };
        if sync.progress().data_synced() != StageStatus::Achieved
            || sync.progress().file_metadata_synced() != StageStatus::Achieved
        {
            return FilePublicationOutcome::Rejected(
                FilePublicationRejection::UnsupportedGuarantee,
            );
        }

        let receipt =
            match self
                .parent
                .replace_temp(temporary, &self.target, self.binding.expected_target())
            {
                Ok(receipt) => receipt,
                Err(FsError::TargetStateMismatch) => {
                    return FilePublicationOutcome::Rejected(
                        FilePublicationRejection::TargetChanged,
                    );
                }
                Err(FsError::Unsupported { .. } | FsError::Unproven { .. }) => {
                    return FilePublicationOutcome::Rejected(
                        FilePublicationRejection::UnsupportedGuarantee,
                    );
                }
                Err(_) => {
                    return FilePublicationOutcome::Failed(
                        FilePublicationFailureStage::ReplaceTarget,
                    );
                }
            };
        drop(lock);

        let parent_directory_sync = self.parent.sync().map_or(StageStatus::Unknown, |receipt| {
            receipt.progress().parent_directory_synced()
        });
        let evidence = FilePublicationEvidence {
            receipt,
            parent_directory_sync,
        };
        if !publication_is_accepted(self.binding, &evidence.receipt) {
            self.uncertain = true;
            return FilePublicationOutcome::Uncertain(evidence);
        }

        let binding = FilePublicationBinding::Existing {
            identity: evidence.receipt.candidate_identity(),
            digest: evidence.receipt.written_content_digest(),
        };
        FilePublicationOutcome::Published(AcceptedFilePublication { evidence, binding })
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

fn create_temporary(parent: &DirectoryCapability) -> Result<nara_fs::TemporaryFile, FsError> {
    for _ in 0..TEMPORARY_NAME_ATTEMPTS {
        let sequence = NEXT_TEMPORARY_NAME.fetch_add(1, Ordering::Relaxed);
        let name = RelativeComponent::new(format!(
            ".nara-publication-{}-{sequence}.tmp",
            std::process::id()
        ))
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
