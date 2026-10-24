use std::fs::File;
#[cfg(target_os = "linux")]
use std::{
    ffi::OsString,
    io::Read,
    os::{fd::AsRawFd, unix::ffi::OsStringExt},
};

use rustix::fs::{FileType, FlockOperation, Mode, OFlags, fstat, fsync, mkdirat, openat, renameat};
#[cfg(target_os = "linux")]
use rustix::fs::{RenameFlags, ResolveFlags, openat2, renameat_with};

use crate::{
    ConflictProtection, DirectorySyncTier, FileFacts, FileKind, FileSyncTier, FsError, FsOperation,
    LockMode, NativeFileIdentity, ParentAuthorizationTier, PlatformCapabilityMatrix, ProofStatus,
    PublicationAtomicity, PublicationSourceBinding, RelativeComponent, ResolutionTier, TrustMode,
    platform::{DirectoryPublicationSupport, FileCreateMode, NativeDirectoryEntry},
};

pub(crate) struct DirectoryEnumerationCursor {
    #[cfg(target_os = "linux")]
    entries: rustix::fs::Dir,
}

pub(crate) fn write_at(file: &File, buffer: &[u8], offset: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.write_at(buffer, offset)
}

pub(crate) fn restrictive_clone_directory(file: &File) -> Result<File, FsError> {
    file.try_clone()
        .map_err(|source| FsError::io(FsOperation::CloneHandle, source))
}

pub(crate) fn restrictive_clone_file(file: &File) -> Result<File, FsError> {
    file.try_clone()
        .map_err(|source| FsError::io(FsOperation::CloneHandle, source))
}

pub(crate) fn restrictive_clone_executable(file: &File) -> Result<File, FsError> {
    file.try_clone()
        .map_err(|source| FsError::io(FsOperation::CloneHandle, source))
}

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
        replace_source: PublicationSourceBinding::NameBound,
        file_sync: FileSyncTier::DataAndMetadata,
        directory_sync,
        advisory_lock: ProofStatus::Proven,
        read_directory: if cfg!(target_os = "linux") {
            ProofStatus::Proven
        } else {
            ProofStatus::Unsupported
        },
        relative_unlink: ProofStatus::Unsupported,
        non_overwrite_rename: ProofStatus::Unsupported,
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn begin_directory_enumeration(
    directory: &File,
) -> Result<DirectoryEnumerationCursor, FsError> {
    let original = facts(directory)?;
    if original.kind != FileKind::Directory {
        return Err(FsError::NotDirectory);
    }
    // A dup would share the directory offset. Opening "." relative to the live authority
    // creates an independent cursor on that same object, even after its external name changes.
    let opened = openat(
        directory,
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map(File::from)
    .map_err(|error| super::rustix_io(FsOperation::ReadDirectory, error))?;
    let current = facts(&opened)?;
    if current.kind != FileKind::Directory || current.identity != original.identity {
        return Err(FsError::CapabilityIdentityDrift {
            operation: FsOperation::ReadDirectory,
        });
    }
    let entries = rustix::fs::Dir::new(opened)
        .map_err(|error| super::rustix_io(FsOperation::ReadDirectory, error))?;
    Ok(DirectoryEnumerationCursor { entries })
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn begin_directory_enumeration(
    _directory: &File,
) -> Result<DirectoryEnumerationCursor, FsError> {
    Err(FsError::Unsupported {
        operation: FsOperation::ReadDirectory,
        capability: "handle-bound directory enumeration is unsupported on this Unix target",
    })
}

#[cfg(target_os = "linux")]
pub(crate) fn next_directory_entry(
    cursor: &mut DirectoryEnumerationCursor,
) -> Result<Option<NativeDirectoryEntry>, FsError> {
    loop {
        let Some(entry) = cursor.entries.next() else {
            return Ok(None);
        };
        let entry = entry.map_err(|error| super::rustix_io(FsOperation::ReadDirectory, error))?;
        let name = entry.file_name().to_bytes();
        if matches!(name, b"." | b"..") {
            continue;
        }
        let kind = match entry.file_type() {
            FileType::Directory => FileKind::Directory,
            FileType::RegularFile => FileKind::Regular,
            _ => FileKind::Other,
        };
        return Ok(Some(NativeDirectoryEntry {
            name: OsString::from_vec(name.to_vec()),
            name_bytes: name.len(),
            kind,
            reparse: entry.file_type() == FileType::Symlink,
        }));
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn next_directory_entry(
    _cursor: &mut DirectoryEnumerationCursor,
) -> Result<Option<NativeDirectoryEntry>, FsError> {
    Err(FsError::Unsupported {
        operation: FsOperation::ReadDirectory,
        capability: "handle-bound directory enumeration is unsupported on this Unix target",
    })
}

#[cfg(target_os = "linux")]
pub(crate) fn verify_exact_spelling(
    parent: &File,
    file: &File,
    component: &RelativeComponent,
    operation: FsOperation,
) -> Result<(), FsError> {
    verify_exact_spelling_with_observer(parent, file, component, operation, 65_536, || {})
}

#[cfg(target_os = "linux")]
fn verify_exact_spelling_with_observer(
    parent: &File,
    file: &File,
    component: &RelativeComponent,
    operation: FsOperation,
    max_entries: usize,
    mut before_join: impl FnMut(),
) -> Result<(), FsError> {
    use rustix::fs::{AtFlags, statat};

    // Lookup can be case-folded even on Linux. Observe the exact native directory entry, then
    // join it to the already opened object without reopening a path or retaining sibling names.
    let mut cursor = begin_directory_enumeration(parent)?;
    let mut observed = 0;
    for entry in &mut cursor.entries {
        let entry = entry.map_err(|error| super::rustix_io(operation, error))?;
        let name = entry.file_name().to_bytes();
        if matches!(name, b"." | b"..") {
            continue;
        }
        if observed == max_entries {
            return Err(FsError::DirectoryEnumerationEntryLimitExceeded { limit: max_entries });
        }
        observed += 1;
        if name != component.as_os_str().as_encoded_bytes() {
            continue;
        }
        let entry_inode = entry.ino();
        if entry_inode == 0 {
            return Err(FsError::Unproven {
                operation,
                proof: "the exact native entry did not provide inode association evidence",
            });
        }
        before_join();
        let named = statat(parent, component.as_os_str(), AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|error| super::rustix_io(operation, error))?;
        let opened = fstat(file).map_err(|error| super::rustix_io(operation, error))?;
        if named.st_dev != opened.st_dev
            || named.st_ino != opened.st_ino
            || entry_inode != opened.st_ino as u64
        {
            return Err(FsError::CapabilityIdentityDrift { operation });
        }
        return Ok(());
    }
    Err(FsError::ExactSpellingMismatch { operation })
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

#[cfg(target_os = "linux")]
pub(crate) fn directory_publication_support(file: &File) -> DirectoryPublicationSupport {
    const EXT_MAGIC: u64 = 0x0000_EF53;

    if !cfg!(target_arch = "x86_64") {
        return DirectoryPublicationSupport::Unsupported;
    }
    match resolution_tier(file) {
        ResolutionTier::HandleBound => {}
        ResolutionTier::Unsupported => return DirectoryPublicationSupport::Unsupported,
        _ => return DirectoryPublicationSupport::Unproven,
    }
    let Ok(stat) = rustix::fs::fstatfs(file) else {
        return DirectoryPublicationSupport::Unproven;
    };
    if stat.f_type as u64 != EXT_MAGIC {
        return DirectoryPublicationSupport::Unsupported;
    }
    match exact_mount_filesystem(file) {
        LinuxMountFilesystem::Ext4 { .. } => {
            DirectoryPublicationSupport::Proven(PublicationSourceBinding::NameBound)
        }
        LinuxMountFilesystem::Other { .. } => DirectoryPublicationSupport::Unsupported,
        LinuxMountFilesystem::Unproven => DirectoryPublicationSupport::Unproven,
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) const fn directory_publication_support(_file: &File) -> DirectoryPublicationSupport {
    DirectoryPublicationSupport::Unsupported
}

#[cfg(target_os = "linux")]
pub(crate) fn directory_publication_platform_identity(
    file: &File,
    operation: FsOperation,
) -> Result<u64, FsError> {
    match exact_mount_filesystem(file) {
        LinuxMountFilesystem::Ext4 { mount_id } | LinuxMountFilesystem::Other { mount_id } => {
            Ok(mount_id)
        }
        LinuxMountFilesystem::Unproven => Err(FsError::Unproven {
            operation,
            proof: "the live directory handle mount identity is unavailable",
        }),
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn directory_publication_platform_identity(
    file: &File,
    _operation: FsOperation,
) -> Result<u64, FsError> {
    let NativeFileIdentity::Unix { device, .. } = facts(file)?.identity;
    Ok(device)
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
        // Open a FIFO/device without waiting for a peer, then let the capability owner validate
        // regular-file facts on this same handle. Regular files ignore NONBLOCK.
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        strict,
        FsOperation::OpenFile,
    )
}

pub(crate) fn open_child(
    parent: &File,
    component: &RelativeComponent,
    operation: FsOperation,
) -> Result<File, FsError> {
    #[cfg(target_os = "linux")]
    let flags = OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    #[cfg(not(target_os = "linux"))]
    let flags = OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC;

    openat(parent, component.as_os_str(), flags, Mode::empty())
        .map(File::from)
        .map_err(|error| super::rustix_io(operation, error))
}

pub(crate) fn create_directory_exclusive(
    parent: &File,
    component: &RelativeComponent,
    trust: TrustMode,
) -> Result<super::DirectoryCreation, FsError> {
    if trust.is_strict() {
        return Err(FsError::Unproven {
            operation: FsOperation::CreateDirectory,
            proof: "Unix directory creation cannot bind mkdirat to the subsequently opened child handle under strict trust",
        });
    }
    match mkdirat(
        parent,
        component.as_os_str(),
        Mode::RUSR | Mode::WUSR | Mode::XUSR,
    ) {
        Ok(()) => {}
        Err(error) if error == rustix::io::Errno::EXIST => {
            return Err(FsError::AlreadyExists {
                operation: FsOperation::CreateDirectory,
            });
        }
        Err(error) => return Err(super::rustix_io(FsOperation::CreateDirectory, error)),
    }
    Ok(match open_directory(parent, component, false) {
        Ok(file) => super::DirectoryCreation::Opened(file),
        Err(error) => super::DirectoryCreation::CreatedButUnopened(error),
    })
}

pub(crate) fn create_file_exclusive(
    parent: &File,
    component: &RelativeComponent,
    operation: FsOperation,
    mode: FileCreateMode,
) -> Result<File, FsError> {
    let mode = match mode {
        FileCreateMode::Regular => Mode::RUSR | Mode::WUSR,
        FileCreateMode::Executable => Mode::RUSR | Mode::WUSR | Mode::XUSR,
    };
    let result = openat(
        parent,
        component.as_os_str(),
        OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        mode,
    );
    match result {
        Ok(file) => Ok(File::from(file)),
        Err(error) if error == rustix::io::Errno::EXIST => {
            Err(FsError::AlreadyExists { operation })
        }
        Err(error) => Err(super::rustix_io(operation, error)),
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn publish_directory_no_replace(
    parent: &File,
    _source: &File,
    source_name: &RelativeComponent,
    destination: &RelativeComponent,
) -> Result<(), FsError> {
    renameat_with(
        parent,
        source_name.as_os_str(),
        parent,
        destination.as_os_str(),
        RenameFlags::NOREPLACE,
    )
    .map_err(classify_directory_publication_error)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn publish_directory_no_replace(
    _parent: &File,
    _source: &File,
    _source_name: &RelativeComponent,
    _destination: &RelativeComponent,
) -> Result<(), FsError> {
    Err(FsError::Unsupported {
        operation: FsOperation::PublishDirectory,
        capability: "same-parent no-replace directory publication",
    })
}

pub(crate) fn discard_directory_exact(_directory: &File) -> Result<(), FsError> {
    Err(FsError::Unsupported {
        operation: FsOperation::RemoveDirectory,
        capability: "identity-bound empty-directory removal",
    })
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

#[cfg(target_os = "linux")]
pub(crate) fn publish_temporary_no_replace(
    parent: &File,
    _temporary: &File,
    source_name: &RelativeComponent,
    target_name: &RelativeComponent,
) -> Result<(), FsError> {
    renameat_with(
        parent,
        source_name.as_os_str(),
        parent,
        target_name.as_os_str(),
        RenameFlags::NOREPLACE,
    )
    .map_err(|error| classify_no_replace_publication_error(error, FsOperation::Replace))
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn publish_temporary_no_replace(
    _parent: &File,
    _temporary: &File,
    _source_name: &RelativeComponent,
    _target_name: &RelativeComponent,
) -> Result<(), FsError> {
    Err(FsError::Unsupported {
        operation: FsOperation::Replace,
        capability: "same-parent no-replace temporary-file publication",
    })
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

#[cfg(target_os = "linux")]
fn classify_directory_publication_error(error: rustix::io::Errno) -> FsError {
    classify_no_replace_publication_error(error, FsOperation::PublishDirectory)
}

#[cfg(target_os = "linux")]
fn classify_no_replace_publication_error(
    error: rustix::io::Errno,
    operation: FsOperation,
) -> FsError {
    match error {
        rustix::io::Errno::EXIST | rustix::io::Errno::NOTEMPTY => {
            FsError::AlreadyExists { operation }
        }
        rustix::io::Errno::XDEV => FsError::CrossVolume,
        rustix::io::Errno::NOSYS | rustix::io::Errno::OPNOTSUPP | rustix::io::Errno::INVAL => {
            FsError::Unsupported {
                operation,
                capability: "renameat2 RENAME_NOREPLACE publication",
            }
        }
        rustix::io::Errno::PERM => FsError::Unproven {
            operation,
            proof: "the runtime policy rejected RENAME_NOREPLACE",
        },
        rustix::io::Errno::NOENT => FsError::CapabilityIdentityDrift { operation },
        rustix::io::Errno::BUSY => FsError::Unproven {
            operation,
            proof: "the live source or mount topology became busy during publication",
        },
        error => super::rustix_io(operation, error),
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinuxMountFilesystem {
    Ext4 { mount_id: u64 },
    Other { mount_id: u64 },
    Unproven,
}

#[cfg(target_os = "linux")]
fn exact_mount_filesystem(file: &File) -> LinuxMountFilesystem {
    let fdinfo_path = format!("/proc/self/fdinfo/{}", file.as_raw_fd());
    let Some(fdinfo) = read_proc_file_bounded(&fdinfo_path, 16 * 1024) else {
        return LinuxMountFilesystem::Unproven;
    };
    let Some(mount_id) = parse_mount_id(&fdinfo) else {
        return LinuxMountFilesystem::Unproven;
    };
    let Some(mountinfo) = read_proc_file_bounded("/proc/self/mountinfo", 4 * 1024 * 1024) else {
        return LinuxMountFilesystem::Unproven;
    };
    match parse_mount_filesystem(&mountinfo, mount_id) {
        Some("ext4") => LinuxMountFilesystem::Ext4 { mount_id },
        Some(_) => LinuxMountFilesystem::Other { mount_id },
        None => LinuxMountFilesystem::Unproven,
    }
}

#[cfg(target_os = "linux")]
fn read_proc_file_bounded(path: &str, limit: u64) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

#[cfg(target_os = "linux")]
fn parse_mount_id(fdinfo: &[u8]) -> Option<u64> {
    std::str::from_utf8(fdinfo).ok()?.lines().find_map(|line| {
        line.strip_prefix("mnt_id:")
            .and_then(|value| value.trim().parse().ok())
    })
}

#[cfg(target_os = "linux")]
fn parse_mount_filesystem(mountinfo: &[u8], expected_mount_id: u64) -> Option<&str> {
    std::str::from_utf8(mountinfo)
        .ok()?
        .lines()
        .find_map(|line| {
            let mut fields = line.split_whitespace();
            let mount_id = fields.next()?.parse::<u64>().ok()?;
            if mount_id != expected_mount_id {
                return None;
            }
            let (_, filesystem_and_source) = line.split_once(" - ")?;
            filesystem_and_source.split_whitespace().next()
        })
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    struct ExactFixture(std::path::PathBuf);

    impl ExactFixture {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "nara_fs_exact_{}_{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed),
            ));
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
    }

    impl Drop for ExactFixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn exact_entry_join_rejects_a_replacement_even_if_named_and_opened_inodes_match() {
        let fixture = ExactFixture::new();
        let requested = fixture.0.join("VALUE");
        let original = fixture.0.join("original");
        std::fs::write(&requested, b"placeholder").unwrap();
        std::fs::write(&original, b"original").unwrap();
        let parent = File::open(&fixture.0).unwrap();
        let opened = File::open(&original).unwrap();
        let error = verify_exact_spelling_with_observer(
            &parent,
            &opened,
            &RelativeComponent::new("VALUE").unwrap(),
            FsOperation::OpenFile,
            8,
            || {
                std::fs::remove_file(&requested).unwrap();
                std::fs::rename(&original, &requested).unwrap();
            },
        )
        .unwrap_err();
        assert!(matches!(error, FsError::CapabilityIdentityDrift { .. }));
    }

    #[test]
    fn exact_entry_scan_is_bounded_and_distinguishes_exhaustion_from_absence() {
        let fixture = ExactFixture::new();
        for name in ["a", "b", "c"] {
            std::fs::write(fixture.0.join(name), b"value").unwrap();
        }
        let parent = File::open(&fixture.0).unwrap();
        let opened = File::open(fixture.0.join("a")).unwrap();
        let component = RelativeComponent::new("missing").unwrap();
        let check = |limit| {
            verify_exact_spelling_with_observer(
                &parent,
                &opened,
                &component,
                FsOperation::OpenFile,
                limit,
                || {},
            )
        };
        assert!(matches!(
            check(2),
            Err(FsError::DirectoryEnumerationEntryLimitExceeded { limit: 2 })
        ));
        assert!(matches!(
            check(3),
            Err(FsError::ExactSpellingMismatch { .. })
        ));
    }

    #[test]
    fn nested_exact_name_observation_holds_three_additional_descriptors() {
        let fixture = ExactFixture::new();
        std::fs::create_dir(fixture.0.join("nested")).unwrap();
        std::fs::write(fixture.0.join("nested/value"), b"value").unwrap();
        let descriptor_count = || {
            std::fs::read_dir("/proc/self/fd")
                .unwrap()
                .filter_map(Result::ok)
                .filter_map(|entry| std::fs::read_link(entry.path()).ok())
                .filter(|target| target.starts_with(&fixture.0))
                .count()
        };
        assert_eq!(descriptor_count(), 0);
        let parent = File::open(fixture.0.join("nested")).unwrap();
        let opened = File::open(fixture.0.join("nested/value")).unwrap();
        verify_exact_spelling_with_observer(
            &parent,
            &opened,
            &RelativeComponent::new("value").unwrap(),
            FsOperation::OpenFile,
            8,
            || assert_eq!(descriptor_count(), 3),
        )
        .unwrap();
        drop((parent, opened));
        assert_eq!(descriptor_count(), 0);
    }

    #[test]
    fn directory_publication_native_failures_keep_collision_and_proof_distinct() {
        assert!(matches!(
            classify_directory_publication_error(rustix::io::Errno::EXIST),
            FsError::AlreadyExists {
                operation: FsOperation::PublishDirectory
            }
        ));
        assert!(matches!(
            classify_directory_publication_error(rustix::io::Errno::NOTEMPTY),
            FsError::AlreadyExists {
                operation: FsOperation::PublishDirectory
            }
        ));
        assert!(matches!(
            classify_directory_publication_error(rustix::io::Errno::XDEV),
            FsError::CrossVolume
        ));
        assert!(matches!(
            classify_directory_publication_error(rustix::io::Errno::OPNOTSUPP),
            FsError::Unsupported {
                operation: FsOperation::PublishDirectory,
                ..
            }
        ));
        assert!(matches!(
            classify_directory_publication_error(rustix::io::Errno::INVAL),
            FsError::Unsupported {
                operation: FsOperation::PublishDirectory,
                ..
            }
        ));
        assert!(matches!(
            classify_directory_publication_error(rustix::io::Errno::PERM),
            FsError::Unproven {
                operation: FsOperation::PublishDirectory,
                ..
            }
        ));
        assert!(matches!(
            classify_directory_publication_error(rustix::io::Errno::NOENT),
            FsError::CapabilityIdentityDrift {
                operation: FsOperation::PublishDirectory
            }
        ));
    }

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

    #[test]
    fn mountinfo_parser_binds_the_exact_mount_and_filesystem() {
        let fdinfo = b"pos:\t0\nflags:\t0100000\nmnt_id:\t42\n";
        let mountinfo =
            b"41 1 8:1 / / rw - xfs /dev/sda1 rw\n42 1 8:2 / /work rw - ext4 /dev/sda2 rw\n";

        assert_eq!(parse_mount_id(fdinfo), Some(42));
        assert_eq!(parse_mount_filesystem(mountinfo, 42), Some("ext4"));
        assert_eq!(parse_mount_filesystem(mountinfo, 41), Some("xfs"));
        assert_eq!(parse_mount_filesystem(mountinfo, 99), None);
    }
}
