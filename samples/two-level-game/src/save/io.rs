//! One application-owned cooperative local slot. A successful encode is never a saved receipt.

use std::{
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use nara::fs::{
    CapabilityRights, ConflictProtection, ContentDigest, DirectoryCapability, ExpectedTarget,
    FileIdentity, FsError, LockMode, PublicationAtomicity, RelativeComponent, StageStatus,
    TemporaryFile, TemporaryFileCleanup, TemporaryFileFailure,
};

use super::{
    Checkpoint, CheckpointError, DecodedCheckpoint, MAX_CHECKPOINT_BYTES, decode_checkpoint,
    encode_checkpoint,
};

#[derive(Debug, thiserror::Error)]
pub enum SaveIoError {
    #[error("save path must be absolute and have an existing parent and a valid file name")]
    InvalidPath,
    #[error("checkpoint data was rejected: {0}")]
    Data(#[from] CheckpointError),
    #[error("checkpoint file operation failed: {0}")]
    File(#[from] FsError),
    #[error("the checkpoint changed since this slot was observed")]
    Conflict,
    #[error("temporary checkpoint cleanup remains pending")]
    CleanupPending,
    #[error("publication needs explicit readback reconciliation")]
    Uncertain,
    #[error("the platform could not establish the required file publication guarantee")]
    UnsupportedGuarantee,
    #[error("checkpoint publication was interrupted before replacement")]
    Interrupted,
}

#[derive(Debug, Clone, Copy)]
pub struct SaveReceipt {
    pub digest: ContentDigest,
    /// This fact may be unsupported/unknown on a platform; do not claim crash durability from it.
    pub parent_directory_sync: StageStatus,
}

#[derive(Clone, Copy)]
enum Binding {
    Missing,
    Existing {
        identity: FileIdentity,
        digest: ContentDigest,
    },
}
impl Binding {
    fn expected(self) -> ExpectedTarget {
        match self {
            Self::Missing => ExpectedTarget::Missing,
            Self::Existing { identity, .. } => ExpectedTarget::Identity(identity),
        }
    }
}

/// Retain this owner until cleanup succeeds. Runtime composition registers the same owner for
/// shutdown retries; a failed write never transfers cleanup to an unowned error string.
pub struct SaveSlot {
    parent: DirectoryCapability,
    target: RelativeComponent,
    binding: Binding,
    cleanup: Option<TemporaryFileCleanup>,
    uncertain: bool,
    #[cfg(test)]
    interrupt_after_write: bool,
    #[cfg(test)]
    after_replace: Option<Box<dyn FnOnce() + Send>>,
}

impl SaveSlot {
    pub fn open(path: &Path) -> Result<Self, SaveIoError> {
        if !path.is_absolute() {
            return Err(SaveIoError::InvalidPath);
        }
        let parent = path.parent().ok_or(SaveIoError::InvalidPath)?;
        let target = RelativeComponent::new(
            path.file_name()
                .and_then(|name| name.to_str())
                .ok_or(SaveIoError::InvalidPath)?,
        )
        .map_err(|_| SaveIoError::InvalidPath)?;
        Self::from_parent(
            crate::host::directory(parent, CapabilityRights::ReadWrite)?,
            target,
        )
    }

    pub fn from_parent(
        parent: DirectoryCapability,
        target: RelativeComponent,
    ) -> Result<Self, SaveIoError> {
        let binding = observe(&parent, &target)?;
        Ok(Self {
            parent,
            target,
            binding,
            cleanup: None,
            uncertain: false,
            #[cfg(test)]
            interrupt_after_write: false,
            #[cfg(test)]
            after_replace: None,
        })
    }

    /// Reads and validates before changing the conflict baseline, including after uncertain save.
    pub fn load(&mut self) -> Result<DecodedCheckpoint, SaveIoError> {
        if !self.retry_cleanup() {
            return Err(SaveIoError::CleanupPending);
        }
        let file = self.parent.open_child_file(&self.target)?;
        let bytes = file.read_to_end_bounded(MAX_CHECKPOINT_BYTES as u64)?;
        let decoded = decode_checkpoint(&bytes)?;
        let digest = ContentDigest::of_bytes(&bytes);
        file.verify_digest(digest, MAX_CHECKPOINT_BYTES as u64)?;
        self.binding = Binding::Existing {
            identity: file.identity(),
            digest,
        };
        self.uncertain = false;
        Ok(decoded)
    }

    pub fn save(&mut self, checkpoint: &Checkpoint) -> Result<SaveReceipt, SaveIoError> {
        let bytes = encode_checkpoint(checkpoint)?;
        if !self.retry_cleanup() {
            return Err(SaveIoError::CleanupPending);
        }
        if self.uncertain {
            return Err(SaveIoError::Uncertain);
        }
        self.parent.preflight_replace(self.binding.expected())?;
        let existing = match self.binding {
            Binding::Missing => None,
            Binding::Existing { identity, digest } => {
                let file = self
                    .parent
                    .open_child_file(&self.target)
                    .map_err(|_| SaveIoError::Conflict)?;
                if file.identity() != identity
                    || file
                        .verify_digest(digest, MAX_CHECKPOINT_BYTES as u64)
                        .is_err()
                {
                    return Err(SaveIoError::Conflict);
                }
                Some(file)
            }
        };
        let _lock = existing
            .as_ref()
            .map(|file| file.try_lock(LockMode::Exclusive))
            .transpose()?;
        if let (Some(file), Binding::Existing { digest, .. }) = (&existing, self.binding)
            && file
                .verify_digest(digest, MAX_CHECKPOINT_BYTES as u64)
                .is_err()
        {
            return Err(SaveIoError::Conflict);
        }
        let mut candidate = match self.create_candidate() {
            Ok(candidate) => candidate,
            Err(failure) => return Err(self.retain_failure(failure)),
        };
        if let Err(error) = candidate.write_all(&bytes).and_then(|()| candidate.flush()) {
            self.cleanup = Some(candidate.into_cleanup());
            return Err(SaveIoError::File(FsError::Io {
                operation: nara::fs::FsOperation::Write,
                source: error,
            }));
        }
        #[cfg(test)]
        if std::mem::take(&mut self.interrupt_after_write) {
            self.cleanup = Some(candidate.into_cleanup());
            return Err(SaveIoError::Interrupted);
        }
        let sync = match candidate.sync() {
            Ok(sync) => sync,
            Err(error) => {
                self.cleanup = Some(candidate.into_cleanup());
                return Err(error.into());
            }
        };
        if sync.progress().data_synced() != StageStatus::Achieved
            || sync.progress().file_metadata_synced() != StageStatus::Achieved
        {
            self.cleanup = Some(candidate.into_cleanup());
            return Err(SaveIoError::UnsupportedGuarantee);
        }
        let receipt =
            match self
                .parent
                .replace_temp(candidate, &self.target, self.binding.expected())
            {
                Ok(receipt) => receipt,
                Err(failure) => return Err(self.retain_failure(failure)),
            };
        self.uncertain = true;
        #[cfg(test)]
        if let Some(after_replace) = self.after_replace.take() {
            after_replace();
        }
        let expected_previous = match self.binding {
            Binding::Missing => None,
            Binding::Existing { identity, .. } => Some(identity),
        };
        if receipt.previous_identity() != expected_previous
            || receipt.publication_atomicity() != PublicationAtomicity::AtomicNameSwitch
            || (expected_previous.is_none()
                && receipt.conflict_protection() != ConflictProtection::StrongCompareAndSwap)
            || receipt.published_identity() != Some(receipt.candidate_identity())
            || receipt.verified_content_digest() != Some(ContentDigest::of_bytes(&bytes))
            || receipt.durability().data_synced() != StageStatus::Achieved
            || receipt.durability().file_metadata_synced() != StageStatus::Achieved
            || receipt.durability().name_published() != StageStatus::Achieved
        {
            self.uncertain = true;
            return Err(SaveIoError::Uncertain);
        }
        let file = self.parent.open_child_file(&self.target)?;
        if file.identity() != receipt.candidate_identity()
            || file
                .verify_digest(ContentDigest::of_bytes(&bytes), MAX_CHECKPOINT_BYTES as u64)
                .is_err()
        {
            self.uncertain = true;
            return Err(SaveIoError::Uncertain);
        }
        let digest = ContentDigest::of_bytes(&bytes);
        self.binding = Binding::Existing {
            identity: file.identity(),
            digest,
        };
        self.uncertain = false;
        let parent_directory_sync = self.parent.sync().map_or(StageStatus::Unknown, |sync| {
            sync.progress().parent_directory_synced()
        });
        Ok(SaveReceipt {
            digest,
            parent_directory_sync,
        })
    }

    pub fn retry_cleanup(&mut self) -> bool {
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

    #[must_use]
    pub fn cleanup_pending(&self) -> bool {
        self.cleanup.is_some()
    }

    fn retain_failure(&mut self, failure: TemporaryFileFailure) -> SaveIoError {
        let (error, cleanup) = failure.into_parts();
        self.cleanup = cleanup;
        if matches!(error, FsError::TargetStateMismatch) {
            SaveIoError::Conflict
        } else {
            SaveIoError::File(error)
        }
    }

    fn create_candidate(&self) -> Result<TemporaryFile, TemporaryFileFailure> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        for _ in 0..16 {
            let name = RelativeComponent::new(format!(
                ".two-level-{}-{}.tmp",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ))
            .expect("static temporary name");
            match self.parent.create_temp(&name) {
                Err(failure) if matches!(failure.error(), FsError::AlreadyExists { .. }) => {
                    continue;
                }
                result => return result,
            }
        }
        Err(FsError::AlreadyExists {
            operation: nara::fs::FsOperation::CreateTemporary,
        }
        .into())
    }
}

fn observe(
    parent: &DirectoryCapability,
    target: &RelativeComponent,
) -> Result<Binding, SaveIoError> {
    match parent.open_child_file(target) {
        Ok(file) => {
            let bytes = file.read_to_end_bounded(MAX_CHECKPOINT_BYTES as u64)?;
            Ok(Binding::Existing {
                identity: file.identity(),
                digest: ContentDigest::of_bytes(&bytes),
            })
        }
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            Ok(Binding::Missing)
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
