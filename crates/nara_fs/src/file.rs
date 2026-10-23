use std::{
    fmt::{self, Debug, Formatter},
    fs::File,
    io::{self, Read, Seek, SeekFrom, Write},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    CapabilityRights, ContentDigest, DurabilityProgress, FileIdentity, FileKind, FileLockError,
    FileSyncReceipt, FileSyncTier, FsError, FsOperation, LockMode, LockedFileCapability,
    ResolutionTier, RestrictiveCapabilityHandleLease, StageStatus, TrustMode,
    capability::new_host_session, platform,
};

pub struct FileCapability {
    pub(crate) file: File,
    identity: FileIdentity,
    parent_identity: Option<FileIdentity>,
    link_count: u64,
    rights: CapabilityRights,
    trust: TrustMode,
    resolution: ResolutionTier,
    write_offset: u64,
    native_aliases_possible: AtomicBool,
}

impl FileCapability {
    /// Imports authority to one already-opened host file without granting
    /// sibling or parent access.
    ///
    /// Import cannot revoke or exclude existing native aliases, so this capability cannot
    /// acquire locks. Lock ownership requires a file independently opened or created through
    /// a directory capability and never exported as a native handle lease.
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
        let capability = Self::from_opened(
            file,
            FileIdentity::new(
                new_host_session()?,
                crate::CapabilityGeneration(generation),
                facts.identity,
            ),
            None,
            facts.link_count,
            CapabilityRights::ReadOnly,
            trust,
            ResolutionTier::OpenedHandleOnly,
        );
        capability
            .native_aliases_possible
            .store(true, Ordering::Relaxed);
        Ok(capability)
    }

    pub(crate) fn from_opened(
        file: File,
        identity: FileIdentity,
        parent_identity: Option<FileIdentity>,
        link_count: u64,
        rights: CapabilityRights,
        trust: TrustMode,
        resolution: ResolutionTier,
    ) -> Self {
        Self {
            file,
            identity,
            parent_identity,
            link_count,
            rights,
            trust,
            resolution,
            write_offset: 0,
            native_aliases_possible: AtomicBool::new(false),
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
    pub const fn rights(&self) -> CapabilityRights {
        self.rights
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

    /// Reopens this exact regular file through the platform's restrictive launch lease contract.
    ///
    /// Exporting a native handle permanently prevents lock acquisition on this capability:
    /// a Unix duplicate can change the same open-file description's lock state.
    #[doc(hidden)]
    pub fn duplicate_restrictive_host_handle(
        &self,
    ) -> Result<RestrictiveCapabilityHandleLease, FsError> {
        let file = platform::restrictive_clone_file(&self.file)?;
        self.native_aliases_possible.store(true, Ordering::Relaxed);
        Ok(RestrictiveCapabilityHandleLease::new(file))
    }

    /// Reopens this exact regular file with the execute access required by image attestation.
    /// Exporting this native lease also permanently prevents lock acquisition.
    #[doc(hidden)]
    pub fn duplicate_restrictive_executable_host_handle(
        &self,
    ) -> Result<RestrictiveCapabilityHandleLease, FsError> {
        let file = platform::restrictive_clone_executable(&self.file)?;
        self.native_aliases_possible.store(true, Ordering::Relaxed);
        Ok(RestrictiveCapabilityHandleLease::new(file))
    }

    /// Borrows this handle with an independent positional cursor, without duplicating it.
    pub fn reader(&self) -> Result<CapabilityReader<'_>, FsError> {
        Ok(CapabilityReader {
            file: &self.file,
            offset: 0,
        })
    }

    /// Borrows the already-authorized writable handle at this capability's logical write offset.
    ///
    /// Opening an existing file through a directory capability remains read-only. Only files
    /// created by a read-write capability currently carry write authority.
    /// The offset starts at zero and advances by each successful write's byte count. It survives
    /// writer reacquisition and lock/release; reads and reader seeks cannot reposition it.
    /// This is not append mode: external changes to the file length do not move the write offset.
    /// Requests exceeding the signed native file-offset range are rejected before I/O.
    pub fn writer(&mut self) -> Result<CapabilityWriter<'_>, FsError> {
        if self.rights == CapabilityRights::ReadOnly {
            return Err(FsError::ReadOnlyCapability {
                operation: FsOperation::Write,
            });
        }
        Ok(CapabilityWriter {
            file: &mut self.file,
            offset: &mut self.write_offset,
        })
    }

    /// Reads at most `limit` bytes from this already-authorized file.
    ///
    /// The opened handle length selects one exact allocation. Early EOF detects shrinkage and a
    /// stack sentinel detects growth. If the observed length changes while reading, the previous
    /// allocation is dropped before a bounded geometric retry so reallocations never overlap in
    /// memory. Same-length concurrent content replacement is outside this byte-limit contract.
    pub fn read_to_end_bounded(&self, limit: u64) -> Result<Vec<u8>, FsError> {
        const MAX_SNAPSHOT_ATTEMPTS: usize = 8;

        let allocation_limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let mut target = self.bounded_handle_len(limit)?;
        for _ in 0..MAX_SNAPSHOT_ATTEMPTS {
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(target)
                .map_err(|_| bounded_read_allocation_error())?;
            bytes.resize(target, 0);

            let mut reader = self.reader()?;
            let mut filled = 0;
            while filled < target {
                let read = reader
                    .read(&mut bytes[filled..])
                    .map_err(|source| FsError::io(FsOperation::Read, source))?;
                if read == 0 {
                    break;
                }
                filled = filled
                    .checked_add(read)
                    .ok_or_else(bounded_read_allocation_error)?;
            }

            if filled < target {
                drop(bytes);
                target = filled;
                continue;
            }

            let mut sentinel = [0_u8; 1];
            let grew = reader
                .read(&mut sentinel)
                .map_err(|source| FsError::io(FsOperation::Read, source))?
                != 0;
            if !grew {
                return Ok(bytes);
            }
            if target == allocation_limit {
                return Err(FsError::ByteLimitExceeded { limit });
            }

            drop(bytes);
            let observed = self.bounded_handle_len(limit)?;
            let geometric = target.max(1).saturating_mul(2).min(allocation_limit);
            target = observed.max(geometric).min(allocation_limit);
        }

        Err(FsError::io(
            FsOperation::Read,
            io::Error::new(
                io::ErrorKind::Interrupted,
                "opened file length changed repeatedly during bounded read",
            ),
        ))
    }

    fn bounded_handle_len(&self, limit: u64) -> Result<usize, FsError> {
        let len = self
            .file
            .metadata()
            .map_err(|source| FsError::io(FsOperation::Read, source))?
            .len();
        if len > limit {
            return Err(FsError::ByteLimitExceeded { limit });
        }
        usize::try_from(len).map_err(|_| bounded_read_allocation_error())
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

    /// Acquires one owned lock, returning this same file on rejection.
    ///
    /// Existing readers must finish before ownership can move into the lock.
    /// Host import and native handle export make release ownership unprovable and are rejected.
    ///
    /// A file cannot issue two guards for the same native lock:
    ///
    /// ```compile_fail,E0382
    /// use nara_fs::{FileCapability, LockMode};
    /// fn duplicate_owner(file: FileCapability) {
    ///     let first = file.try_lock(LockMode::Exclusive).unwrap();
    ///     let second = file.try_lock(LockMode::Exclusive).unwrap();
    ///     drop((first, second));
    /// }
    /// ```
    pub fn try_lock(self, mode: LockMode) -> Result<LockedFileCapability, FileLockError> {
        if self.native_aliases_possible.load(Ordering::Relaxed) {
            return Err(FileLockError::new(
                FsError::Unproven {
                    operation: FsOperation::Lock,
                    proof: "host import or native handle export cannot prove unique lock authority",
                },
                self,
            ));
        }
        match platform::try_lock(&self.file, mode) {
            Ok(()) => Ok(LockedFileCapability::new(self, mode)),
            Err(error) => Err(FileLockError::new(error, self)),
        }
    }
}

fn bounded_read_allocation_error() -> FsError {
    FsError::io(
        FsOperation::Read,
        io::Error::new(
            io::ErrorKind::OutOfMemory,
            "bounded file read allocation failed",
        ),
    )
}

impl Debug for FileCapability {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileCapability")
            .field("identity", &self.identity)
            .field("parent_identity", &self.parent_identity)
            .field("link_count", &self.link_count)
            .field("rights", &self.rights)
            .field("trust", &self.trust)
            .field("resolution", &self.resolution)
            .finish_non_exhaustive()
    }
}

/// An independent positional cursor borrowing an authorized file handle.
///
/// This reader cannot keep a lock alive after its owner closes or release the lock itself.
///
/// ```compile_fail,E0505
/// use std::io::Read;
/// use nara_fs::LockedFileCapability;
/// fn release_while_reading(locked: LockedFileCapability) {
///     let mut reader = locked.reader().unwrap();
///     let file = locked.release().unwrap();
///     let mut bytes = [0; 1];
///     reader.read_exact(&mut bytes).unwrap();
/// }
/// ```
pub struct CapabilityReader<'file> {
    file: &'file File,
    offset: u64,
}

/// A borrowed writer continuing its capability's logical position on the exact authorized handle.
pub struct CapabilityWriter<'file> {
    file: &'file mut File,
    offset: &'file mut u64,
}

impl Write for CapabilityWriter<'_> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let file = &*self.file;
        write_at_offset(self.offset, buffer, |buffer, offset| {
            platform::write_at(file, buffer, offset)
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn write_at_offset(
    offset: &mut u64,
    buffer: &[u8],
    write: impl FnOnce(&[u8], u64) -> io::Result<usize>,
) -> io::Result<usize> {
    if buffer.is_empty() {
        return Ok(0);
    }
    // Validate the entire request before I/O, excluding negative Windows position sentinels.
    offset
        .checked_add(buffer.len() as u64)
        .filter(|end| *end <= i64::MAX as u64)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "writer offset overflow"))?;
    let written = write(buffer, *offset)?;
    *offset += written as u64;
    Ok(written)
}

impl Debug for CapabilityWriter<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapabilityWriter")
            .finish_non_exhaustive()
    }
}

impl Read for CapabilityReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = positional_read(self.file, buffer, self.offset)?;
        self.offset = self
            .offset
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "reader offset overflow"))?;
        Ok(read)
    }
}

impl Seek for CapabilityReader<'_> {
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

impl Debug for CapabilityReader<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapabilityReader")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::write_at_offset;

    #[test]
    fn write_offset_advances_by_partial_count_and_continues_at_that_position() {
        let mut offset = 5;
        let written = write_at_offset(&mut offset, b"abcd", |buffer, position| {
            assert_eq!(buffer, b"abcd");
            assert_eq!(position, 5);
            Ok(2)
        })
        .unwrap();
        assert_eq!(written, 2);
        assert_eq!(offset, 7);
        write_at_offset(&mut offset, b"cd", |buffer, position| {
            assert_eq!(buffer, b"cd");
            assert_eq!(position, 7);
            Ok(buffer.len())
        })
        .unwrap();
        assert_eq!(offset, 9);
    }

    #[test]
    fn write_offset_is_unchanged_on_error_or_zero_progress() {
        let mut offset = 7;
        let error = write_at_offset(&mut offset, b"ab", |_, position| {
            assert_eq!(position, 7);
            Err(io::Error::from(io::ErrorKind::Interrupted))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
        assert_eq!(offset, 7);
        assert_eq!(
            write_at_offset(&mut offset, b"ab", |_, position| {
                assert_eq!(position, 7);
                Ok(0)
            })
            .unwrap(),
            0
        );
        assert_eq!(offset, 7);
    }

    #[test]
    fn write_offset_checks_native_range_before_io_and_accepts_empty_writes() {
        for initial in [u64::MAX - 1, i64::MAX as u64 - 1, u64::MAX] {
            let mut offset = initial;
            let error = write_at_offset(&mut offset, b"ab", |_, _| {
                panic!("an overflowing write must not reach the native handle")
            })
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert_eq!(offset, initial);
        }
        let mut offset = i64::MAX as u64 - 1;
        assert_eq!(write_at_offset(&mut offset, b"a", |_, _| Ok(1)).unwrap(), 1);
        assert_eq!(offset, i64::MAX as u64);
        assert_eq!(
            write_at_offset(&mut offset, b"", |_, _| {
                panic!("an empty write must not reach the native handle")
            })
            .unwrap(),
            0
        );
        assert_eq!(offset, i64::MAX as u64);
    }
}
