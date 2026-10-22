use std::{
    fmt::{self, Debug, Formatter},
    fs::File,
};

use crate::{
    CapabilityReader, CapabilityRights, ContentDigest, FileCapability, FileIdentity, FsError,
    ResolutionTier, TrustMode, platform,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockMode {
    Shared,
    Exclusive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockGuarantee {
    AdvisoryCooperative,
    PlatformEnforced,
    Unsupported,
    Unproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockScope {
    OpenHandle,
    Process,
}

/// The unique owner of a native file lock and its authorized file handle.
///
/// Readers borrow this owner; neither native handle export nor another lock
/// acquisition is available. Explicit release returns the unlocked file or retains this owner
/// in the error. Drop attempts unlock and always closes the owned handle without a receipt.
///
/// ```compile_fail,E0599
/// use nara_fs::{LockMode, LockedFileCapability};
/// fn relock(locked: LockedFileCapability) {
///     let second = locked.try_lock(LockMode::Exclusive);
/// }
/// ```
///
/// ```compile_fail,E0599
/// use nara_fs::LockedFileCapability;
/// fn duplicate(locked: LockedFileCapability) {
///     let second = locked.clone();
/// }
/// ```
///
/// ```compile_fail,E0599
/// use nara_fs::LockedFileCapability;
/// fn export_lock_authority(locked: LockedFileCapability) {
///     let alias = locked.duplicate_restrictive_host_handle();
/// }
/// ```
#[must_use = "retain the lock owner until access is complete, or explicitly release it"]
pub struct LockedFileCapability {
    file: Option<FileCapability>,
    mode: LockMode,
}

impl LockedFileCapability {
    pub(crate) const fn new(file: FileCapability, mode: LockMode) -> Self {
        Self {
            file: Some(file),
            mode,
        }
    }

    fn file(&self) -> &FileCapability {
        self.file.as_ref().expect("a live lock owns its file")
    }

    #[must_use]
    pub const fn mode(&self) -> LockMode {
        self.mode
    }

    #[must_use]
    pub const fn guarantee(&self) -> LockGuarantee {
        if cfg!(windows) {
            LockGuarantee::PlatformEnforced
        } else if cfg!(unix) {
            LockGuarantee::AdvisoryCooperative
        } else {
            LockGuarantee::Unsupported
        }
    }

    #[must_use]
    pub const fn scope(&self) -> LockScope {
        LockScope::OpenHandle
    }

    #[must_use]
    pub fn identity(&self) -> FileIdentity {
        self.file().identity()
    }

    #[must_use]
    pub fn parent_identity(&self) -> Option<FileIdentity> {
        self.file().parent_identity()
    }

    #[must_use]
    pub fn link_count(&self) -> u64 {
        self.file().link_count()
    }

    #[must_use]
    pub fn rights(&self) -> CapabilityRights {
        self.file().rights()
    }

    #[must_use]
    pub fn trust(&self) -> TrustMode {
        self.file().trust()
    }

    #[must_use]
    pub fn resolution_tier(&self) -> ResolutionTier {
        self.file().resolution_tier()
    }

    pub fn reader(&self) -> Result<CapabilityReader<'_>, FsError> {
        self.file().reader()
    }

    pub fn read_to_end_bounded(&self, limit: u64) -> Result<Vec<u8>, FsError> {
        self.file().read_to_end_bounded(limit)
    }

    pub fn digest(&self, limit: u64) -> Result<ContentDigest, FsError> {
        self.file().digest(limit)
    }

    pub fn verify_digest(&self, expected: ContentDigest, limit: u64) -> Result<(), FsError> {
        self.file().verify_digest(expected, limit)
    }

    /// Releases this exact native lock. Failure retains its handle and release authority for retry.
    pub fn release(self) -> Result<FileCapability, FileUnlockError> {
        self.release_with(platform::unlock)
    }

    fn release_with(
        mut self,
        unlock: impl FnOnce(&File) -> Result<(), FsError>,
    ) -> Result<FileCapability, FileUnlockError> {
        if let Err(error) = unlock(&self.file().file) {
            return Err(FileUnlockError {
                error,
                locked: Box::new(self),
            });
        }
        Ok(self.file.take().expect("a released lock returns its file"))
    }
}

impl Debug for LockedFileCapability {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LockedFileCapability")
            .field("file", &self.file())
            .field("mode", &self.mode)
            .field("guarantee", &self.guarantee())
            .field("scope", &self.scope())
            .finish_non_exhaustive()
    }
}

impl Drop for LockedFileCapability {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            let _ = platform::unlock(&file.file);
        }
    }
}

/// A rejected acquisition retaining the same unlocked file capability.
#[derive(Debug, thiserror::Error)]
#[error("file lock acquisition failed: {error}")]
#[must_use = "recover the original file or deliberately close it"]
pub struct FileLockError {
    #[source]
    error: FsError,
    file: Box<FileCapability>,
}

impl FileLockError {
    pub(crate) fn new(error: FsError, file: FileCapability) -> Self {
        Self {
            error,
            file: Box::new(file),
        }
    }

    #[must_use]
    pub const fn error(&self) -> &FsError {
        &self.error
    }

    #[must_use]
    pub fn file(&self) -> &FileCapability {
        &self.file
    }

    #[must_use]
    pub fn into_parts(self) -> (FsError, FileCapability) {
        (self.error, *self.file)
    }

    #[must_use]
    pub fn into_file(self) -> FileCapability {
        *self.file
    }
}

/// A failed explicit release retaining the only locked file owner.
///
/// Recover the owner with `into_parts` or `into_locked` to retry. Dropping this error makes a
/// best-effort unlock and closes the file; it does not establish successful explicit release.
#[derive(Debug, thiserror::Error)]
#[error("file lock release failed: {error}")]
#[must_use = "retain the locked owner for retry or deliberately close it"]
pub struct FileUnlockError {
    #[source]
    error: FsError,
    locked: Box<LockedFileCapability>,
}

impl FileUnlockError {
    #[must_use]
    pub const fn error(&self) -> &FsError {
        &self.error
    }

    #[must_use]
    pub fn locked(&self) -> &LockedFileCapability {
        &self.locked
    }

    pub fn into_parts(self) -> (FsError, LockedFileCapability) {
        (self.error, *self.locked)
    }

    pub fn into_locked(self) -> LockedFileCapability {
        *self.locked
    }
}

#[cfg(all(test, any(windows, unix)))]
mod tests {
    use std::{
        fs, io,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::{DirectoryCapability, FsOperation, HostCapabilityOptions, RelativeComponent};

    static NEXT_TEST_FILE: AtomicU64 = AtomicU64::new(1);

    struct TestFile(PathBuf);

    impl TestFile {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let sequence = NEXT_TEST_FILE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nara-fs-unlock-{}-{stamp}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).unwrap();
            fs::write(path.join("workspace.lock"), b"lock").unwrap();
            Self(path)
        }

        fn open(&self) -> FileCapability {
            #[cfg(windows)]
            let handle = {
                use std::os::windows::fs::OpenOptionsExt;
                use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;
                fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                    .open(&self.0)
                    .unwrap()
            };
            #[cfg(unix)]
            let handle = File::open(&self.0).unwrap();
            DirectoryCapability::from_host_handle(
                handle,
                HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
            )
            .unwrap()
            .open_child_file(&RelativeComponent::new("workspace.lock").unwrap())
            .unwrap()
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            fs::remove_file(self.0.join("workspace.lock")).unwrap();
            fs::remove_dir(&self.0).unwrap();
        }
    }

    fn injected_unlock_failure(_file: &File) -> Result<(), FsError> {
        Err(FsError::io(
            FsOperation::Unlock,
            io::Error::other("injected unlock failure before native release"),
        ))
    }

    #[test]
    fn failed_explicit_release_retains_lock_and_can_retry() {
        let file = TestFile::new();
        let first = file.open();
        let identity = first.identity();
        let held = first.try_lock(LockMode::Exclusive).unwrap();
        let failure = held.release_with(injected_unlock_failure).unwrap_err();
        assert!(matches!(
            failure.error(),
            FsError::Io {
                operation: FsOperation::Unlock,
                ..
            }
        ));
        assert_eq!(failure.locked().identity(), identity);

        let contender = file.open().try_lock(LockMode::Exclusive).unwrap_err();
        assert!(matches!(contender.error(), FsError::LockContended));
        let contender = contender.into_file();
        let (error, held) = failure.into_parts();
        assert!(matches!(
            error,
            FsError::Io {
                operation: FsOperation::Unlock,
                ..
            }
        ));
        let released = held.release().unwrap();
        assert_eq!(released.identity(), identity);
        let next = contender.try_lock(LockMode::Exclusive).unwrap();
        drop(released);
        let blocked = file.open().try_lock(LockMode::Exclusive).unwrap_err();
        assert!(matches!(blocked.error(), FsError::LockContended));
        drop(blocked);
        drop(next);
    }

    #[test]
    fn dropping_failed_release_closes_the_only_lock_owner() {
        let file = TestFile::new();
        let held = file.open().try_lock(LockMode::Exclusive).unwrap();
        let failure = held.release_with(injected_unlock_failure).unwrap_err();
        let blocked = file.open().try_lock(LockMode::Exclusive).unwrap_err();
        assert!(matches!(blocked.error(), FsError::LockContended));
        let contender = blocked.into_file();
        drop(failure);
        let _next = contender.try_lock(LockMode::Exclusive).unwrap();
    }
}
