//! One application-owned cooperative local slot. A successful encode is never a saved receipt.

use std::{
    io::Write,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use nara::fs::{
    CapabilityRights, ConflictProtection, ContentDigest, DirectoryCapability, ExpectedTarget,
    FileCapability, FsError, LockMode, PublicationAtomicity, RelativeComponent, StageStatus,
    TemporaryFile, TemporaryFileCleanup, TemporaryFileFailure,
};

use super::{
    Checkpoint, CheckpointError, DecodedCheckpoint, MAX_CHECKPOINT_BYTES, decode_checkpoint,
    encode_checkpoint,
};

mod server;
use server::{ServerSlot, server_file_error};

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
    #[error("the Server checkpoint operation is unavailable in the current handoff state")]
    HandoffUnavailable,
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

enum Binding {
    Missing,
    Existing {
        file: FileCapability,
        digest: ContentDigest,
    },
}

pub(crate) struct CheckpointBinding(Binding);

impl Binding {
    fn observe(file: Option<FileCapability>) -> Result<Self, SaveIoError> {
        let Some(file) = file else {
            return Ok(Self::Missing);
        };
        let bytes = file.read_to_end_bounded(MAX_CHECKPOINT_BYTES as u64)?;
        Ok(Self::Existing {
            file,
            digest: ContentDigest::of_bytes(&bytes),
        })
    }

    fn expected(&self) -> ExpectedTarget {
        match self {
            Self::Missing => ExpectedTarget::Missing,
            Self::Existing { file, .. } => ExpectedTarget::Identity(file.identity()),
        }
    }

    fn verify(
        &self,
        observed: Option<FileCapability>,
    ) -> Result<Option<FileCapability>, SaveIoError> {
        match (self, observed) {
            (Self::Missing, None) => Ok(None),
            (Self::Existing { file, digest }, Some(observed))
                if file.identity() == observed.identity() =>
            {
                observed
                    .verify_digest(*digest, MAX_CHECKPOINT_BYTES as u64)
                    .map_err(|_| SaveIoError::Conflict)?;
                Ok(Some(observed))
            }
            _ => Err(SaveIoError::Conflict),
        }
    }
}

/// Retain this owner until cleanup succeeds. Runtime composition registers the same owner for
/// shutdown retries; a failed write never transfers cleanup to an unowned error string.
pub struct SaveSlot {
    parent: DirectoryCapability,
    target: RelativeComponent,
    binding: Binding,
    server: Option<ServerSlot>,
    cleanup: Option<TemporaryFileCleanup>,
    uncertain: bool,
    #[cfg(test)]
    interrupt_after_write: bool,
    #[cfg(test)]
    after_replace: Option<Box<dyn FnOnce() + Send>>,
}

impl SaveSlot {
    pub fn open(path: &Path) -> Result<Self, SaveIoError> {
        let (parent, target) = open_path(path)?;
        Self::from_parent(parent, target)
    }

    pub fn from_parent(
        parent: DirectoryCapability,
        target: RelativeComponent,
    ) -> Result<Self, SaveIoError> {
        let binding = Binding::observe(optional_file(parent.open_child_file(&target))?)?;
        Ok(Self::with_binding(parent, target, binding, None))
    }

    fn with_binding(
        parent: DirectoryCapability,
        target: RelativeComponent,
        binding: Binding,
        server: Option<ServerSlot>,
    ) -> Self {
        Self {
            parent,
            target,
            binding,
            server,
            cleanup: None,
            uncertain: false,
            #[cfg(test)]
            interrupt_after_write: false,
            #[cfg(test)]
            after_replace: None,
        }
    }

    /// Reads and validates before changing the conflict baseline, including after uncertain save.
    pub fn load(&mut self) -> Result<DecodedCheckpoint, SaveIoError> {
        let (decoded, binding) = self.prepare_load()?;
        self.accept_load(binding)?;
        Ok(decoded)
    }

    pub(crate) fn prepare_load(
        &mut self,
    ) -> Result<(DecodedCheckpoint, CheckpointBinding), SaveIoError> {
        self.verify_server_access()?;
        if !self.retry_cleanup() {
            return Err(SaveIoError::CleanupPending);
        }
        let file = self.open_checkpoint()?;
        let bytes = file.read_to_end_bounded(MAX_CHECKPOINT_BYTES as u64)?;
        let decoded = decode_checkpoint(&bytes)?;
        let digest = ContentDigest::of_bytes(&bytes);
        file.verify_digest(digest, MAX_CHECKPOINT_BYTES as u64)?;
        let binding = CheckpointBinding(Binding::Existing { file, digest });
        Ok((decoded, binding))
    }

    pub(crate) fn accept_load(&mut self, binding: CheckpointBinding) -> Result<(), SaveIoError> {
        self.verify_server_access()?;
        let observed = self.open_checkpoint()?;
        let CheckpointBinding(binding) = binding;
        binding.verify(Some(observed))?;
        self.binding = binding;
        self.uncertain = false;
        Ok(())
    }

    pub fn save(&mut self, checkpoint: &Checkpoint) -> Result<SaveReceipt, SaveIoError> {
        self.verify_server_access()?;
        let bytes = encode_checkpoint(checkpoint)?;
        if !self.retry_cleanup() {
            return Err(SaveIoError::CleanupPending);
        }
        if self.uncertain {
            return Err(SaveIoError::Uncertain);
        }
        let expected = self.binding.expected();
        self.parent.preflight_replace(expected)?;
        let observed = match (&self.server, &self.binding) {
            (Some(server), _) => server.observe_target()?,
            (None, Binding::Missing) => None,
            (None, Binding::Existing { .. }) => Some(
                self.parent
                    .open_child_file(&self.target)
                    .map_err(|_| SaveIoError::Conflict)?,
            ),
        };
        let existing = self
            .binding
            .verify(observed)?
            .map(|file| file.try_lock(LockMode::Exclusive))
            .transpose()
            .map_err(|failure| {
                let (error, _file) = failure.into_parts();
                SaveIoError::File(error)
            })?;
        if let (Some(file), Binding::Existing { digest, .. }) = (&existing, &self.binding)
            && file
                .verify_digest(*digest, MAX_CHECKPOINT_BYTES as u64)
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
        if let Err(error) = self.verify_server_access() {
            self.cleanup = Some(candidate.into_cleanup());
            return Err(error);
        }
        let receipt = match self.parent.replace_temp(candidate, &self.target, expected) {
            Ok(receipt) => receipt,
            Err(failure) => return Err(self.retain_failure(failure)),
        };
        self.uncertain = true;
        #[cfg(test)]
        if let Some(after_replace) = self.after_replace.take() {
            after_replace();
        }
        let expected_previous = match &self.binding {
            Binding::Missing => None,
            Binding::Existing { file, .. } => Some(file.identity()),
        };
        let digest = ContentDigest::of_bytes(&bytes);
        if receipt.previous_identity() != expected_previous
            || receipt.publication_atomicity() != PublicationAtomicity::AtomicNameSwitch
            || (expected_previous.is_none()
                && receipt.conflict_protection() != ConflictProtection::StrongCompareAndSwap)
            || receipt.published_identity() != Some(receipt.candidate_identity())
            || receipt.verified_content_digest() != Some(digest)
            || receipt.durability().data_synced() != StageStatus::Achieved
            || receipt.durability().file_metadata_synced() != StageStatus::Achieved
            || receipt.durability().name_published() != StageStatus::Achieved
        {
            return Err(SaveIoError::Uncertain);
        }
        self.verify_server_access()
            .map_err(|_| SaveIoError::Uncertain)?;
        let file = match self.open_checkpoint() {
            Ok(file) => file,
            Err(error) if self.server.is_none() => return Err(error),
            Err(_) => return Err(SaveIoError::Uncertain),
        };
        if file.identity() != receipt.candidate_identity()
            || file
                .verify_digest(digest, MAX_CHECKPOINT_BYTES as u64)
                .is_err()
        {
            return Err(SaveIoError::Uncertain);
        }
        self.binding = Binding::Existing { file, digest };
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

    fn verify_server_access(&self) -> Result<(), SaveIoError> {
        self.server
            .as_ref()
            .map_or(Ok(()), ServerSlot::verify_access)
    }

    fn open_checkpoint(&self) -> Result<FileCapability, SaveIoError> {
        match &self.server {
            Some(server) => server.open_target().map_err(server_file_error),
            None => Ok(self.parent.open_child_file(&self.target)?),
        }
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

fn open_path(path: &Path) -> Result<(DirectoryCapability, RelativeComponent), SaveIoError> {
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
    Ok((
        crate::host::directory(parent, CapabilityRights::ReadWrite)?,
        target,
    ))
}

fn optional_file(file: Result<FileCapability, FsError>) -> Result<Option<FileCapability>, FsError> {
    match file {
        Ok(file) => Ok(Some(file)),
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
