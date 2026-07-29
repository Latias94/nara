use std::fs::File;

use rustix::fs::{FileType, FlockOperation, Mode, OFlags, fstat, fsync, openat, renameat};
#[cfg(target_os = "linux")]
use rustix::fs::{ResolveFlags, openat2};

use crate::{
    ConflictProtection, DirectorySyncTier, FileFacts, FileKind, FileSyncTier, FsError, FsOperation,
    LockMode, NativeFileIdentity, ParentAuthorizationTier, PlatformCapabilityMatrix, ProofStatus,
    PublicationAtomicity, RelativeComponent, ReplaceSourceBinding, ResolutionTier,
};

pub(crate) const fn capability_matrix() -> PlatformCapabilityMatrix {
    #[cfg(target_os = "linux")]
    let directory_sync = DirectorySyncTier::Naming;
    #[cfg(not(target_os = "linux"))]
    let directory_sync = DirectorySyncTier::Unproven;

    PlatformCapabilityMatrix {
        // Strict resolution is capability- and runtime-specific on Linux:
        // kernels or sandboxes may reject `openat2` or individual resolve flags.
        resolution: ResolutionTier::Unproven,
        single_link: ProofStatus::Proven,
        replace_parent: ParentAuthorizationTier::HandleBound,
        publication: PublicationAtomicity::AtomicNameSwitch,
        conflict: ConflictProtection::DetectOnly,
        replace_source: ReplaceSourceBinding::NameBound,
        file_sync: FileSyncTier::DataAndMetadata,
        directory_sync,
        advisory_lock: ProofStatus::Proven,
        read_directory: ProofStatus::Unsupported,
        relative_unlink: ProofStatus::Unsupported,
        non_overwrite_rename: ProofStatus::Unsupported,
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn resolution_tier(file: &File) -> ResolutionTier {
    match openat2(
        file,
        ".",
        OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
        strict_resolve_flags(),
    ) {
        Ok(_) => ResolutionTier::HandleBound,
        Err(error) => resolution_error_tier(error),
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) const fn resolution_tier(_file: &File) -> ResolutionTier {
    ResolutionTier::Unproven
}

pub(crate) fn facts(file: &File) -> Result<FileFacts, FsError> {
    let stat = fstat(file).map_err(|error| super::rustix_io(FsOperation::InspectHandle, error))?;
    let kind = match FileType::from_raw_mode(stat.st_mode) {
        FileType::Directory => FileKind::Directory,
        FileType::RegularFile => FileKind::Regular,
        _ => FileKind::Other,
    };
    Ok(FileFacts {
        identity: NativeFileIdentity::Unix {
            device: stat.st_dev as u64,
            inode: stat.st_ino as u64,
        },
        kind,
        link_count: stat.st_nlink as u64,
        reparse_tag: None,
        identity_proven: local_identity_semantics_proven(file),
    })
}

#[cfg(target_os = "linux")]
fn local_identity_semantics_proven(file: &File) -> bool {
    const EXT_MAGIC: u64 = 0x0000_EF53;
    const XFS_MAGIC: u64 = 0x5846_5342;
    const BTRFS_MAGIC: u64 = 0x9123_683E;
    const TMPFS_MAGIC: u64 = 0x0102_1994;
    const OVERLAYFS_MAGIC: u64 = 0x794C_7630;
    const NTFS3_MAGIC: u64 = 0x5346_544E;

    rustix::fs::fstatfs(file).is_ok_and(|stat| {
        matches!(
            stat.f_type as u64,
            EXT_MAGIC | XFS_MAGIC | BTRFS_MAGIC | TMPFS_MAGIC | OVERLAYFS_MAGIC | NTFS3_MAGIC
        )
    })
}

#[cfg(not(target_os = "linux"))]
fn local_identity_semantics_proven(_file: &File) -> bool {
    false
}

pub(crate) fn open_directory(
    parent: &File,
    component: &RelativeComponent,
    strict: bool,
) -> Result<File, FsError> {
    open_component(
        parent,
        component,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        strict,
        FsOperation::OpenDirectory,
    )
}

pub(crate) fn open_file(
    parent: &File,
    component: &RelativeComponent,
    strict: bool,
) -> Result<File, FsError> {
    open_component(
        parent,
        component,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        strict,
        FsOperation::OpenFile,
    )
}

pub(crate) fn create_file_exclusive(
    parent: &File,
    component: &RelativeComponent,
) -> Result<File, FsError> {
    let result = openat(
        parent,
        component.as_os_str(),
        OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    );
    match result {
        Ok(file) => Ok(File::from(file)),
        Err(error) if error == rustix::io::Errno::EXIST => Err(FsError::AlreadyExists {
            operation: FsOperation::CreateTemporary,
        }),
        Err(error) => Err(super::rustix_io(FsOperation::CreateTemporary, error)),
    }
}

pub(crate) fn discard_temporary(
    _parent: &File,
    _file: &File,
    _name: &RelativeComponent,
    _expected: NativeFileIdentity,
) -> Result<(), FsError> {
    Err(FsError::Unsupported {
        operation: FsOperation::RemoveTemporary,
        capability: "Unix cannot bind unlink of a named temporary to its opened handle",
    })
}

pub(crate) fn replace_temporary(
    parent: &File,
    _temporary: &File,
    source_name: &RelativeComponent,
    target_name: &RelativeComponent,
) -> Result<(), FsError> {
    renameat(
        parent,
        source_name.as_os_str(),
        parent,
        target_name.as_os_str(),
    )
    .map_err(|error| super::rustix_io(FsOperation::Replace, error))
}

pub(crate) fn sync_file(file: &File) -> Result<FileSyncTier, FsError> {
    fsync(file)
        .map(|()| FileSyncTier::DataAndMetadata)
        .map_err(|error| super::rustix_io(FsOperation::SyncFile, error))
}

pub(crate) fn sync_directory(file: &File) -> Result<DirectorySyncTier, FsError> {
    fsync(file)
        .map(|()| {
            #[cfg(target_os = "linux")]
            {
                DirectorySyncTier::Naming
            }
            #[cfg(not(target_os = "linux"))]
            {
                DirectorySyncTier::Unproven
            }
        })
        .map_err(|error| super::rustix_io(FsOperation::SyncDirectory, error))
}

pub(crate) fn try_lock(file: &File, mode: LockMode) -> Result<(), FsError> {
    let operation = match mode {
        LockMode::Shared => FlockOperation::NonBlockingLockShared,
        LockMode::Exclusive => FlockOperation::NonBlockingLockExclusive,
    };
    match rustix::fs::flock(file, operation) {
        Ok(()) => Ok(()),
        Err(error)
            if error == rustix::io::Errno::WOULDBLOCK || error == rustix::io::Errno::AGAIN =>
        {
            Err(FsError::LockContended)
        }
        Err(error) => Err(super::rustix_io(FsOperation::Lock, error)),
    }
}

pub(crate) fn unlock(file: &File) -> Result<(), FsError> {
    rustix::fs::flock(file, FlockOperation::Unlock)
        .map_err(|error| super::rustix_io(FsOperation::Unlock, error))
}

fn open_component(
    parent: &File,
    component: &RelativeComponent,
    flags: OFlags,
    strict: bool,
    operation: FsOperation,
) -> Result<File, FsError> {
    #[cfg(target_os = "linux")]
    if strict {
        return openat2(
            parent,
            component.as_os_str(),
            flags,
            Mode::empty(),
            strict_resolve_flags(),
        )
        .map(File::from)
        .map_err(|error| strict_open_error(operation, error));
    }

    #[cfg(not(target_os = "linux"))]
    if strict {
        return Err(FsError::Unproven {
            operation,
            proof: "this Unix adapter cannot prove absence of same-device mount traversal",
        });
    }

    openat(parent, component.as_os_str(), flags, Mode::empty())
        .map(File::from)
        .map_err(|error| super::rustix_io(operation, error))
}

#[cfg(target_os = "linux")]
const fn strict_resolve_flags() -> ResolveFlags {
    ResolveFlags::BENEATH
        .union(ResolveFlags::NO_MAGICLINKS)
        .union(ResolveFlags::NO_SYMLINKS)
        .union(ResolveFlags::NO_XDEV)
}

#[cfg(target_os = "linux")]
fn resolution_error_tier(error: rustix::io::Errno) -> ResolutionTier {
    if matches!(
        error,
        rustix::io::Errno::NOSYS | rustix::io::Errno::INVAL | rustix::io::Errno::OPNOTSUPP
    ) {
        ResolutionTier::Unsupported
    } else {
        ResolutionTier::Unproven
    }
}

#[cfg(target_os = "linux")]
fn strict_open_error(operation: FsOperation, error: rustix::io::Errno) -> FsError {
    if error == rustix::io::Errno::LOOP {
        return FsError::SymbolicLinkTraversal;
    }
    if error == rustix::io::Errno::XDEV {
        return FsError::CrossVolume;
    }
    match resolution_error_tier(error) {
        ResolutionTier::Unsupported => FsError::Unsupported {
            operation,
            capability: "Linux openat2 strict resolve flags",
        },
        ResolutionTier::Unproven if error == rustix::io::Errno::PERM => FsError::Unproven {
            operation,
            proof: "the runtime policy prevented strict openat2 resolution",
        },
        _ => super::rustix_io(operation, error),
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn unavailable_openat2_evidence_never_reports_handle_bound() {
        for error in [
            rustix::io::Errno::NOSYS,
            rustix::io::Errno::INVAL,
            rustix::io::Errno::OPNOTSUPP,
        ] {
            assert_eq!(resolution_error_tier(error), ResolutionTier::Unsupported);
        }
        assert_eq!(
            resolution_error_tier(rustix::io::Errno::PERM),
            ResolutionTier::Unproven
        );
    }

    #[test]
    fn strict_open_classifies_link_and_mount_policy_rejections() {
        assert!(matches!(
            strict_open_error(FsOperation::OpenFile, rustix::io::Errno::LOOP),
            FsError::SymbolicLinkTraversal
        ));
        assert!(matches!(
            strict_open_error(FsOperation::OpenFile, rustix::io::Errno::XDEV),
            FsError::CrossVolume
        ));
    }
}
