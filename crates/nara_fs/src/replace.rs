use std::{
    fmt::{self, Debug, Formatter},
    fs::File,
    io::Write,
    sync::Arc,
};

use crate::{
    ConflictProtection, ContentDigest, DurabilityProgress, FileIdentity, FileSyncTier, FsError,
    ParentAuthorizationTier, PublicationAtomicity, PublicationIdentityEvidence,
    PublicationSourceBinding, RelativeComponent, StageStatus,
    capability::{DirectoryInner, validate_file_facts},
    platform,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedTarget {
    Any,
    Missing,
    Identity(FileIdentity),
}

/// A failed temporary-file operation and any cleanup authority it still owns.
#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct TemporaryFileFailure {
    #[source]
    error: FsError,
    cleanup: Option<Box<TemporaryFileCleanup>>,
}

impl TemporaryFileFailure {
    /// Associates an operation failure with cleanup authority the caller already owns.
    ///
    /// Use `From<FsError>` for failures before candidate creation; use this constructor when an
    /// existing candidate remains, including write or synchronization failures.
    #[must_use]
    pub fn with_cleanup(error: FsError, cleanup: TemporaryFileCleanup) -> Self {
        Self {
            error,
            cleanup: Some(Box::new(cleanup)),
        }
    }

    #[must_use]
    pub const fn error(&self) -> &FsError {
        &self.error
    }

    #[must_use]
    pub fn cleanup(&self) -> Option<&TemporaryFileCleanup> {
        self.cleanup.as_deref()
    }

    /// Attempts retained cleanup once, preserving the original failure and any remaining owner.
    #[must_use]
    pub fn retry_cleanup(&mut self) -> bool {
        let Some(cleanup) = self.cleanup.take() else {
            return true;
        };
        match (*cleanup).discard() {
            Ok(()) => true,
            Err(failure) => {
                self.cleanup = failure.cleanup;
                false
            }
        }
    }

    #[must_use]
    pub fn into_parts(self) -> (FsError, Option<TemporaryFileCleanup>) {
        (self.error, self.cleanup.map(|cleanup| *cleanup))
    }
}

impl From<FsError> for TemporaryFileFailure {
    fn from(error: FsError) -> Self {
        Self {
            error,
            cleanup: None,
        }
    }
}

/// Owns the opened candidate solely for an explicit, identity-bound cleanup retry.
///
/// A failed operation retains this owner without retrying cleanup during `Drop`. It never grants
/// read, write, or publication authority, including when creation validation did not succeed.
pub struct TemporaryFileCleanup {
    parent: Arc<DirectoryInner>,
    name: RelativeComponent,
    file: File,
    identity: Option<FileIdentity>,
    cleanup_on_drop: bool,
}

impl TemporaryFileCleanup {
    pub(crate) fn new(
        parent: Arc<DirectoryInner>,
        name: RelativeComponent,
        file: File,
        identity: Option<FileIdentity>,
    ) -> Self {
        Self {
            parent,
            name,
            file,
            identity,
            cleanup_on_drop: false,
        }
    }

    #[must_use]
    pub const fn identity(&self) -> Option<FileIdentity> {
        self.identity
    }

    /// Returns the original relative candidate name for recovery diagnostics, not authorization.
    #[must_use]
    pub const fn name(&self) -> &RelativeComponent {
        &self.name
    }

    /// Retries cleanup of this exact opened object and returns ownership again on failure.
    pub fn discard(mut self) -> Result<(), TemporaryFileFailure> {
        self.cleanup_on_drop = false;
        let result = self.discard_inner();
        result.map_err(|error| TemporaryFileFailure::with_cleanup(error, self))
    }

    fn discard_inner(&mut self) -> Result<(), FsError> {
        let identity = match self.identity {
            Some(identity) => identity,
            None => {
                let facts = platform::facts(&self.file)?;
                validate_file_facts(&self.parent, facts)?;
                if !facts.identity_proven {
                    return Err(FsError::IdentityUnavailable);
                }
                let identity = FileIdentity::new(
                    self.parent.identity.session(),
                    self.parent.options.generation(),
                    facts.identity,
                );
                self.identity = Some(identity);
                identity
            }
        };
        platform::discard_temporary(&self.parent.file, &self.file, &self.name, identity.native())
    }
}

impl Debug for TemporaryFileCleanup {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TemporaryFileCleanup")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

impl Drop for TemporaryFileCleanup {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            let _ = self.discard_inner();
        }
    }
}

pub struct TemporaryFile {
    cleanup: TemporaryFileCleanup,
    identity: FileIdentity,
    durability: DurabilityProgress,
    content_hasher: blake3::Hasher,
    content_length: u64,
}

impl TemporaryFile {
    pub(crate) fn new(
        parent: Arc<DirectoryInner>,
        name: RelativeComponent,
        file: File,
        identity: FileIdentity,
    ) -> Self {
        let mut cleanup = TemporaryFileCleanup::new(parent, name, file, Some(identity));
        cleanup.cleanup_on_drop = true;
        Self {
            cleanup,
            identity,
            durability: DurabilityProgress::NONE,
            content_hasher: blake3::Hasher::new(),
            content_length: 0,
        }
    }

    #[must_use]
    pub const fn identity(&self) -> FileIdentity {
        self.identity
    }

    pub fn sync(&mut self) -> Result<crate::FileSyncReceipt, FsError> {
        let tier = platform::sync_file(&self.cleanup.file)?;
        let synced = match tier {
            FileSyncTier::DataAndMetadata => StageStatus::Achieved,
            FileSyncTier::Unsupported => StageStatus::Unsupported,
        };
        let receipt = crate::FileSyncReceipt::new(
            tier,
            DurabilityProgress::new(synced, synced, StageStatus::Unknown, StageStatus::Unknown),
        );
        self.durability = receipt.progress();
        Ok(receipt)
    }

    pub fn discard(self) -> Result<(), TemporaryFileFailure> {
        self.into_cleanup().discard()
    }

    /// Relinquishes write and publication authority while retaining explicit cleanup ownership.
    #[must_use]
    pub fn into_cleanup(mut self) -> TemporaryFileCleanup {
        self.cleanup.cleanup_on_drop = false;
        self.cleanup
    }

    pub(crate) fn parent(&self) -> &Arc<DirectoryInner> {
        &self.cleanup.parent
    }

    pub(crate) fn name(&self) -> &RelativeComponent {
        &self.cleanup.name
    }

    pub(crate) fn file_handle(&self) -> &std::fs::File {
        &self.cleanup.file
    }

    pub(crate) const fn durability(&self) -> DurabilityProgress {
        self.durability
    }

    pub(crate) fn written_content_digest(&self) -> ContentDigest {
        ContentDigest::from_parts(
            self.content_length,
            *self.content_hasher.clone().finalize().as_bytes(),
        )
    }

    pub(crate) fn mark_published(&mut self) {
        self.cleanup.cleanup_on_drop = false;
    }
}

impl Write for TemporaryFile {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.durability = DurabilityProgress::NONE;
        let written = self.cleanup.file.write(buffer)?;
        self.content_hasher.update(&buffer[..written]);
        self.content_length = self
            .content_length
            .checked_add(written as u64)
            .expect("temporary file content length fits in u64");
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.cleanup.file.flush()
    }
}

impl Debug for TemporaryFile {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TemporaryFile")
            .field("identity", &self.identity)
            .field("cleanup_on_drop", &self.cleanup.cleanup_on_drop)
            .field("durability", &self.durability)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReplaceReceipt {
    previous: Option<FileIdentity>,
    candidate: FileIdentity,
    observed_published: Option<FileIdentity>,
    identity_evidence: PublicationIdentityEvidence,
    parent_authorization: ParentAuthorizationTier,
    publication: PublicationAtomicity,
    conflict: ConflictProtection,
    source_binding: PublicationSourceBinding,
    durability: DurabilityProgress,
    written_content_digest: ContentDigest,
    published_content_digest: Option<ContentDigest>,
}

impl ReplaceReceipt {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        previous: Option<FileIdentity>,
        candidate: FileIdentity,
        observed_published: Option<FileIdentity>,
        identity_evidence: PublicationIdentityEvidence,
        parent_authorization: ParentAuthorizationTier,
        publication: PublicationAtomicity,
        conflict: ConflictProtection,
        source_binding: PublicationSourceBinding,
        durability: DurabilityProgress,
        written_content_digest: ContentDigest,
        published_content_digest: Option<ContentDigest>,
    ) -> Self {
        Self {
            previous,
            candidate,
            observed_published,
            identity_evidence,
            parent_authorization,
            publication,
            conflict,
            source_binding,
            durability,
            written_content_digest,
            published_content_digest,
        }
    }

    #[must_use]
    pub const fn previous_identity(&self) -> Option<FileIdentity> {
        self.previous
    }

    #[must_use]
    pub const fn candidate_identity(&self) -> FileIdentity {
        self.candidate
    }

    #[must_use]
    pub const fn published_identity(&self) -> Option<FileIdentity> {
        self.observed_published
    }

    #[must_use]
    pub const fn publication_identity_evidence(&self) -> PublicationIdentityEvidence {
        self.identity_evidence
    }

    #[must_use]
    pub const fn parent_authorization(&self) -> ParentAuthorizationTier {
        self.parent_authorization
    }

    #[must_use]
    pub const fn publication_atomicity(&self) -> PublicationAtomicity {
        self.publication
    }

    #[must_use]
    pub const fn conflict_protection(&self) -> ConflictProtection {
        self.conflict
    }

    #[must_use]
    pub const fn source_binding(&self) -> PublicationSourceBinding {
        self.source_binding
    }

    #[must_use]
    pub const fn durability(&self) -> DurabilityProgress {
        self.durability
    }

    /// Digest of the exact byte stream accepted through this temporary-file capability.
    #[must_use]
    pub const fn written_content_digest(&self) -> ContentDigest {
        self.written_content_digest
    }

    /// Digest observed after publication through the target name and candidate identity.
    ///
    /// `None` means the published target could not be rebound, identified, or read within the
    /// written byte bound. A caller must not treat the written digest as published evidence.
    #[must_use]
    pub const fn published_content_digest(&self) -> Option<ContentDigest> {
        self.published_content_digest
    }

    /// Returns the digest only when post-publication observation matches the written byte stream.
    #[must_use]
    pub fn verified_content_digest(&self) -> Option<ContentDigest> {
        self.published_content_digest
            .filter(|published| *published == self.written_content_digest)
    }

    #[must_use]
    pub const fn naming_is_atomic(&self) -> bool {
        matches!(self.publication, PublicationAtomicity::AtomicNameSwitch)
    }
}
