use std::{
    fmt::{self, Debug, Formatter},
    io::Write,
    sync::Arc,
};

use crate::{
    ConflictProtection, ContentDigest, DurabilityProgress, FileCapability, FileIdentity, FsError,
    ParentAuthorizationTier, PublicationAtomicity, PublicationIdentityEvidence, RelativeComponent,
    ReplaceSourceBinding, capability::DirectoryInner, platform,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedTarget {
    Any,
    Missing,
    Identity(FileIdentity),
}

pub struct TemporaryFile {
    parent: Arc<DirectoryInner>,
    name: RelativeComponent,
    file: FileCapability,
    durability: DurabilityProgress,
    content_hasher: blake3::Hasher,
    content_length: u64,
    cleanup_on_drop: bool,
}

impl TemporaryFile {
    pub(crate) fn new(
        parent: Arc<DirectoryInner>,
        name: RelativeComponent,
        file: FileCapability,
    ) -> Self {
        Self {
            parent,
            name,
            file,
            durability: DurabilityProgress::NONE,
            content_hasher: blake3::Hasher::new(),
            content_length: 0,
            cleanup_on_drop: true,
        }
    }

    #[must_use]
    pub const fn identity(&self) -> FileIdentity {
        self.file.identity()
    }

    pub fn sync(&mut self) -> Result<crate::FileSyncReceipt, FsError> {
        let receipt = self.file.sync()?;
        self.durability = receipt.progress();
        Ok(receipt)
    }

    pub fn discard(mut self) -> Result<(), FsError> {
        // An explicit terminal request must not trigger a hidden second
        // name-based cleanup attempt from `Drop` after it reports failure.
        self.cleanup_on_drop = false;
        platform::discard_temporary(
            &self.parent.file,
            &self.file.file,
            &self.name,
            self.file.identity().native(),
        )
    }

    pub(crate) fn parent(&self) -> &Arc<DirectoryInner> {
        &self.parent
    }

    pub(crate) fn name(&self) -> &RelativeComponent {
        &self.name
    }

    pub(crate) fn file_handle(&self) -> &std::fs::File {
        &self.file.file
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
        self.cleanup_on_drop = false;
    }
}

impl Write for TemporaryFile {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.durability = DurabilityProgress::NONE;
        let written = self.file.file.write(buffer)?;
        self.content_hasher.update(&buffer[..written]);
        self.content_length = self
            .content_length
            .checked_add(written as u64)
            .expect("temporary file content length fits in u64");
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.file.flush()
    }
}

impl Debug for TemporaryFile {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TemporaryFile")
            .field("identity", &self.file.identity())
            .field("cleanup_on_drop", &self.cleanup_on_drop)
            .field("durability", &self.durability)
            .finish_non_exhaustive()
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            let _ = platform::discard_temporary(
                &self.parent.file,
                &self.file.file,
                &self.name,
                self.file.identity().native(),
            );
        }
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
    source_binding: ReplaceSourceBinding,
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
        source_binding: ReplaceSourceBinding,
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
    pub const fn source_binding(&self) -> ReplaceSourceBinding {
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
