use std::{
    fmt::{self, Debug, Formatter},
    fs::File,
    io::{self, Read, Seek, SeekFrom},
};

use crate::{
    ContentDigest, DurabilityProgress, FileIdentity, FileKind, FileLock, FileSyncReceipt,
    FileSyncTier, FsError, FsOperation, LockMode, ResolutionTier, StageStatus, TrustMode,
    capability::new_host_session, platform,
};

pub struct FileCapability {
    pub(crate) file: File,
    identity: FileIdentity,
    parent_identity: Option<FileIdentity>,
    link_count: u64,
    trust: TrustMode,
    resolution: ResolutionTier,
}

impl FileCapability {
    /// Imports authority to one already-opened host file without granting
    /// sibling or parent access.
    pub fn from_host_handle(
        file: File,
        trust: TrustMode,
        generation: u64,
    ) -> Result<Self, FsError> {
        let facts = platform::facts(&file)?;
        if facts.kind != FileKind::Regular {
            return Err(FsError::NotRegularFile);
        }
        if let Some(tag) = facts.reparse_tag {
            return Err(FsError::ReparsePoint { tag });
        }
        if trust.is_strict() && !facts.identity_proven {
            return Err(FsError::Unproven {
                operation: FsOperation::InspectHandle,
                proof: "host file identity is unavailable or remote filesystem semantics are unproved",
            });
        }
        if trust.is_strict() && facts.link_count != 1 {
            return Err(FsError::MultipleLinks {
                links: facts.link_count,
            });
        }
        Ok(Self::from_opened(
            file,
            FileIdentity::new(
                new_host_session()?,
                crate::CapabilityGeneration(generation),
                facts.identity,
            ),
            None,
            facts.link_count,
            trust,
            ResolutionTier::OpenedHandleOnly,
        ))
    }

    pub(crate) fn from_opened(
        file: File,
        identity: FileIdentity,
        parent_identity: Option<FileIdentity>,
        link_count: u64,
        trust: TrustMode,
        resolution: ResolutionTier,
    ) -> Self {
        Self {
            file,
            identity,
            parent_identity,
            link_count,
            trust,
            resolution,
        }
    }

    #[must_use]
    pub const fn identity(&self) -> FileIdentity {
        self.identity
    }

    #[must_use]
    pub const fn parent_identity(&self) -> Option<FileIdentity> {
        self.parent_identity
    }

    #[must_use]
    pub const fn link_count(&self) -> u64 {
        self.link_count
    }

    #[must_use]
    pub const fn trust(&self) -> TrustMode {
        self.trust
    }

    /// Returns how this file handle was bound to filesystem authority.
    #[must_use]
    pub const fn resolution_tier(&self) -> ResolutionTier {
        self.resolution
    }

    pub fn reader(&self) -> Result<CapabilityReader, FsError> {
        Ok(CapabilityReader {
            file: platform::clone_file(&self.file)?,
            offset: 0,
        })
    }

    pub fn digest(&self, limit: u64) -> Result<ContentDigest, FsError> {
        let mut reader = self.reader()?;
        let mut hasher = blake3::Hasher::new();
        let mut total = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let remaining_with_sentinel = limit.saturating_sub(total).saturating_add(1);
            let capacity = usize::try_from(remaining_with_sentinel)
                .unwrap_or(usize::MAX)
                .min(buffer.len());
            let read = reader
                .read(&mut buffer[..capacity])
                .map_err(|source| FsError::io(FsOperation::Read, source))?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            if total > limit {
                return Err(FsError::ByteLimitExceeded { limit });
            }
            hasher.update(&buffer[..read]);
        }
        Ok(ContentDigest::from_parts(
            total,
            *hasher.finalize().as_bytes(),
        ))
    }

    pub fn verify_digest(&self, expected: ContentDigest, limit: u64) -> Result<(), FsError> {
        let actual = self.digest(limit)?;
        if actual == expected {
            Ok(())
        } else {
            Err(FsError::DigestMismatch { expected, actual })
        }
    }

    pub fn sync(&self) -> Result<FileSyncReceipt, FsError> {
        let tier = platform::sync_file(&self.file)?;
        let synced = match tier {
            FileSyncTier::DataAndMetadata => StageStatus::Achieved,
            FileSyncTier::Unsupported => StageStatus::Unsupported,
        };
        Ok(FileSyncReceipt::new(
            tier,
            DurabilityProgress::new(synced, synced, StageStatus::Unknown, StageStatus::Unknown),
        ))
    }

    pub fn try_lock(&self, mode: LockMode) -> Result<FileLock<'_>, FsError> {
        platform::try_lock(&self.file, mode)?;
        Ok(FileLock::new(&self.file, mode))
    }
}

impl Debug for FileCapability {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileCapability")
            .field("identity", &self.identity)
            .field("parent_identity", &self.parent_identity)
            .field("link_count", &self.link_count)
            .field("trust", &self.trust)
            .field("resolution", &self.resolution)
            .finish_non_exhaustive()
    }
}

pub struct CapabilityReader {
    file: File,
    offset: u64,
}

impl Read for CapabilityReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = positional_read(&self.file, buffer, self.offset)?;
        self.offset = self
            .offset
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "reader offset overflow"))?;
        Ok(read)
    }
}

impl Seek for CapabilityReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::Current(delta) => checked_offset(self.offset, delta),
            SeekFrom::End(delta) => checked_offset(self.file.metadata()?.len(), delta),
        }
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid reader offset"))?;
        self.offset = next;
        Ok(next)
    }
}

fn checked_offset(base: u64, delta: i64) -> Option<u64> {
    if delta >= 0 {
        base.checked_add(delta as u64)
    } else {
        base.checked_sub(delta.unsigned_abs())
    }
}

#[cfg(unix)]
fn positional_read(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buffer, offset)
}

#[cfg(windows)]
fn positional_read(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buffer, offset)
}

#[cfg(not(any(unix, windows)))]
fn positional_read(_file: &File, _buffer: &mut [u8], _offset: u64) -> io::Result<usize> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "positional file reads are unsupported on this target",
    ))
}

impl Debug for CapabilityReader {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapabilityReader")
            .finish_non_exhaustive()
    }
}
