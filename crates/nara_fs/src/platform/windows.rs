use std::{
    ffi::OsStr,
    fs::File,
    io,
    mem::{size_of, size_of_val},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    ptr,
};

use windows_sys::Wdk::{
    Foundation::OBJECT_ATTRIBUTES,
    Storage::FileSystem::{
        FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
        FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT, FileFsDeviceInformation,
        FileRenameInformationEx, NtCreateFile, NtQueryVolumeInformationFile, NtSetInformationFile,
    },
};
use windows_sys::Win32::{
    Foundation::{RtlNtStatusToDosError, UNICODE_STRING},
    Storage::FileSystem::{
        DELETE, FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_ID_INFO, FILE_LIST_DIRECTORY,
        FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_READ_EA, FILE_RENAME_INFO, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FILE_TRAVERSE,
        FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, FileAttributeTagInfo, FileIdInfo,
        FileStandardInfo, FlushFileBuffers, GetFileInformationByHandleEx,
        GetVolumeInformationByHandleW, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
        LockFileEx, SYNCHRONIZE, SetFileInformationByHandle, UnlockFileEx,
    },
    System::IO::{IO_STATUS_BLOCK, OVERLAPPED},
};

use crate::{
    ConflictProtection, DirectorySyncTier, FileFacts, FileKind, FileSyncTier, FsError, FsOperation,
    LockMode, NativeFileIdentity, ParentAuthorizationTier, PlatformCapabilityMatrix, ProofStatus,
    PublicationAtomicity, RelativeComponent, ReplaceSourceBinding, ResolutionTier,
};

const OBJ_CASE_INSENSITIVE: u32 = 0x40;
const FILE_REMOTE_DEVICE: u32 = 0x10;
const FILE_RENAME_REPLACE_IF_EXISTS: u32 = 0x1;
const FILE_RENAME_POSIX_SEMANTICS: u32 = 0x2;

#[repr(C)]
#[derive(Default)]
struct FileFsDeviceInformation {
    device_type: u32,
    characteristics: u32,
}

pub(crate) const fn capability_matrix() -> PlatformCapabilityMatrix {
    PlatformCapabilityMatrix {
        resolution: ResolutionTier::HandleBound,
        single_link: ProofStatus::Proven,
        replace_parent: ParentAuthorizationTier::HandleBound,
        publication: PublicationAtomicity::AtomicNameSwitch,
        conflict: ConflictProtection::DetectOnly,
        replace_source: ReplaceSourceBinding::HandleBound,
        file_sync: FileSyncTier::DataAndMetadata,
        directory_sync: DirectorySyncTier::Unsupported,
        advisory_lock: ProofStatus::Proven,
        read_directory: ProofStatus::Unsupported,
        relative_unlink: ProofStatus::Unsupported,
        non_overwrite_rename: ProofStatus::Unsupported,
    }
}

pub(crate) const fn resolution_tier(_file: &File) -> ResolutionTier {
    ResolutionTier::HandleBound
}

pub(crate) fn facts(file: &File) -> Result<FileFacts, FsError> {
    let id: FILE_ID_INFO = query_info(file, FileIdInfo, IdentityQuery::FileId)?;
    let standard: FILE_STANDARD_INFO =
        query_info(file, FileStandardInfo, IdentityQuery::LinkCount)?;
    let attributes: FILE_ATTRIBUTE_TAG_INFO = query_info(
        file,
        FileAttributeTagInfo,
        IdentityQuery::AttributeAndReparseTag,
    )?;
    let kind = if attributes.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        FileKind::Directory
    } else if attributes.FileAttributes & FILE_ATTRIBUTE_DEVICE != 0 {
        FileKind::Other
    } else {
        FileKind::Regular
    };
    let reparse_tag = (attributes.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .then_some(attributes.ReparseTag);

    Ok(FileFacts {
        identity: NativeFileIdentity::Windows {
            volume_serial: id.VolumeSerialNumber,
            file_id: id.FileId.Identifier,
        },
        kind,
        link_count: u64::from(standard.NumberOfLinks),
        reparse_tag,
        identity_proven: local_volume_is_proven(file)?,
    })
}

pub(crate) fn open_directory(
    parent: &File,
    component: &RelativeComponent,
    _strict: bool,
) -> Result<File, FsError> {
    nt_open(
        parent,
        component,
        FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_OPEN,
        FILE_DIRECTORY_FILE,
        FsOperation::OpenDirectory,
    )
}

pub(crate) fn open_file(
    parent: &File,
    component: &RelativeComponent,
    _strict: bool,
) -> Result<File, FsError> {
    nt_open(
        parent,
        component,
        FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_OPEN,
        FILE_NON_DIRECTORY_FILE,
        FsOperation::OpenFile,
    )
}

pub(crate) fn create_file_exclusive(
    parent: &File,
    component: &RelativeComponent,
) -> Result<File, FsError> {
    nt_open(
        parent,
        component,
        FILE_READ_DATA
            | FILE_WRITE_DATA
            | FILE_READ_EA
            | FILE_WRITE_EA
            | FILE_READ_ATTRIBUTES
            | FILE_WRITE_ATTRIBUTES
            | DELETE
            | SYNCHRONIZE,
        FILE_CREATE,
        FILE_NON_DIRECTORY_FILE,
        FsOperation::CreateTemporary,
    )
}

pub(crate) fn discard_temporary(
    _parent: &File,
    file: &File,
    _name: &RelativeComponent,
    _expected: NativeFileIdentity,
) -> Result<(), FsError> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_DISPOSITION_INFO, FileDispositionInfo};

    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: `file` owns a valid handle and `disposition` has the layout required
    // by `FileDispositionInfo` for the duration of this call.
    let succeeded = unsafe {
        SetFileInformationByHandle(
            raw_handle(file),
            FileDispositionInfo,
            ptr::addr_of!(disposition).cast(),
            size_of_val(&disposition) as u32,
        )
    };
    if succeeded == 0 {
        Err(FsError::io(
            FsOperation::RemoveTemporary,
            io::Error::last_os_error(),
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn replace_temporary(
    parent: &File,
    temporary: &File,
    _source_name: &RelativeComponent,
    target_name: &RelativeComponent,
) -> Result<(), FsError> {
    let target = wide_component(target_name.as_os_str());
    let bytes = size_of::<FILE_RENAME_INFO>() + target.len() * size_of::<u16>();
    let words = bytes.div_ceil(size_of::<usize>());
    let mut storage = vec![0_usize; words];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();

    // SAFETY: `storage` is aligned for `FILE_RENAME_INFO` and sized for the
    // trailing UTF-16 name. Every field is initialized before the syscall.
    unsafe {
        (*info).Anonymous.Flags = FILE_RENAME_REPLACE_IF_EXISTS | FILE_RENAME_POSIX_SEMANTICS;
        (*info).RootDirectory = raw_handle(parent);
        (*info).FileNameLength = (target.len() * size_of::<u16>()) as u32;
        ptr::copy_nonoverlapping(
            target.as_ptr(),
            ptr::addr_of_mut!((*info).FileName).cast::<u16>(),
            target.len(),
        );
    }

    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: `temporary` and `parent` are live handles; `info` points to the
    // initialized buffer described above for the duration of this call.
    let status = unsafe {
        NtSetInformationFile(
            raw_handle(temporary),
            &mut status_block,
            info.cast(),
            bytes as u32,
            FileRenameInformationEx,
        )
    };
    if status < 0 {
        // SAFETY: conversion accepts the returned NTSTATUS value.
        let code = unsafe { RtlNtStatusToDosError(status) };
        Err(FsError::io(
            FsOperation::Replace,
            io::Error::from_raw_os_error(code as i32),
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn sync_file(file: &File) -> Result<FileSyncTier, FsError> {
    // SAFETY: `file` owns a live file handle.
    if unsafe { FlushFileBuffers(raw_handle(file)) } == 0 {
        Err(FsError::io(
            FsOperation::SyncFile,
            io::Error::last_os_error(),
        ))
    } else {
        Ok(FileSyncTier::DataAndMetadata)
    }
}

pub(crate) fn sync_directory(_file: &File) -> Result<DirectorySyncTier, FsError> {
    Err(FsError::Unsupported {
        operation: FsOperation::SyncDirectory,
        capability: "parent directory synchronization is not proven by the Windows adapter",
    })
}

pub(crate) fn try_lock(file: &File, mode: LockMode) -> Result<(), FsError> {
    let mut overlapped = OVERLAPPED::default();
    let mut flags = LOCKFILE_FAIL_IMMEDIATELY;
    if mode == LockMode::Exclusive {
        flags |= LOCKFILE_EXCLUSIVE_LOCK;
    }
    // SAFETY: `file` is live and `overlapped` remains valid for the synchronous
    // non-blocking call. The range covers the complete 64-bit file space.
    let succeeded = unsafe {
        LockFileEx(
            raw_handle(file),
            flags,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        )
    };
    if succeeded != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if matches!(error.raw_os_error(), Some(32 | 33)) {
        Err(FsError::LockContended)
    } else {
        Err(FsError::io(FsOperation::Lock, error))
    }
}

pub(crate) fn unlock(file: &File) -> Result<(), FsError> {
    let mut overlapped = OVERLAPPED::default();
    // SAFETY: this unlock uses the same handle and byte range as `try_lock`.
    if unsafe { UnlockFileEx(raw_handle(file), 0, u32::MAX, u32::MAX, &mut overlapped) } == 0 {
        Err(FsError::io(FsOperation::Unlock, io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

fn nt_open(
    parent: &File,
    component: &RelativeComponent,
    desired_access: u32,
    disposition: u32,
    type_option: u32,
    operation: FsOperation,
) -> Result<File, FsError> {
    let mut name = wide_component(component.as_os_str());
    let length = name
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|length| u16::try_from(length).ok())
        .ok_or(FsError::Unproven {
            operation,
            proof: "component does not fit a Windows UNICODE_STRING",
        })?;
    let unicode = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: name.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: raw_handle(parent),
        ObjectName: &unicode,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: ptr::null(),
        SecurityQualityOfService: ptr::null(),
    };
    let mut status_block = IO_STATUS_BLOCK::default();
    let mut handle = ptr::null_mut();
    // SAFETY: all pointers refer to initialized values that outlive this call;
    // the object name is a validated single relative component and authority is
    // supplied only by `parent` through `RootDirectory`.
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            desired_access,
            &attributes,
            &mut status_block,
            ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            disposition,
            FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT | type_option,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        // SAFETY: conversion accepts the returned NTSTATUS value.
        let code = unsafe { RtlNtStatusToDosError(status) };
        if disposition == FILE_CREATE && matches!(code, 80 | 183) {
            return Err(FsError::AlreadyExists { operation });
        }
        return Err(FsError::io(
            operation,
            io::Error::from_raw_os_error(code as i32),
        ));
    }
    if handle.is_null() {
        return Err(FsError::IdentityUnavailable);
    }

    // SAFETY: successful `NtCreateFile` returned unique ownership of `handle`.
    Ok(unsafe { File::from_raw_handle(handle.cast()) })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IdentityQuery {
    FileId,
    LinkCount,
    AttributeAndReparseTag,
}

fn query_info<T: Default>(file: &File, class: i32, query: IdentityQuery) -> Result<T, FsError> {
    let mut value = T::default();
    // SAFETY: the information class and `T` pairing is fixed at each callsite,
    // and the buffer remains valid and writable for the call.
    let succeeded = unsafe {
        GetFileInformationByHandleEx(
            raw_handle(file),
            class,
            ptr::addr_of_mut!(value).cast(),
            size_of::<T>() as u32,
        )
    };
    if succeeded == 0 {
        Err(classify_identity_query_failure(query))
    } else {
        Ok(value)
    }
}

fn classify_identity_query_failure(query: IdentityQuery) -> FsError {
    let proof = match query {
        IdentityQuery::FileId => {
            "Windows FILE_ID_INFO is unavailable for this handle or filesystem"
        }
        IdentityQuery::LinkCount => {
            "Windows FILE_STANDARD_INFO link-count evidence is unavailable for this handle or filesystem"
        }
        IdentityQuery::AttributeAndReparseTag => {
            "Windows FILE_ATTRIBUTE_TAG_INFO reparse evidence is unavailable for this handle or filesystem"
        }
    };
    FsError::Unproven {
        operation: FsOperation::InspectHandle,
        proof,
    }
}

fn local_volume_is_proven(file: &File) -> Result<bool, FsError> {
    let mut status_block = IO_STATUS_BLOCK::default();
    let mut info = FileFsDeviceInformation::default();
    // SAFETY: `file` is live and `info` is a writable buffer matching
    // `FileFsDeviceInformation` for the duration of the call.
    let status = unsafe {
        NtQueryVolumeInformationFile(
            raw_handle(file),
            &mut status_block,
            ptr::addr_of_mut!(info).cast(),
            size_of::<FileFsDeviceInformation>() as u32,
            FileFsDeviceInformation,
        )
    };
    if status < 0 {
        return Ok(false);
    }
    let mut filesystem_name = [0_u16; 32];
    // SAFETY: `file` is live, optional output pointers are null, and
    // `filesystem_name` is a writable buffer of the declared length.
    let succeeded = unsafe {
        GetVolumeInformationByHandleW(
            raw_handle(file),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            filesystem_name.as_mut_ptr(),
            filesystem_name.len() as u32,
        )
    };
    if succeeded == 0 {
        return Ok(false);
    }
    let length = filesystem_name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(filesystem_name.len());
    Ok(volume_identity_is_proven(
        info.characteristics,
        &filesystem_name[..length],
    ))
}

fn volume_identity_is_proven(characteristics: u32, filesystem_name: &[u16]) -> bool {
    characteristics & FILE_REMOTE_DEVICE == 0
        && (ascii_name_eq(filesystem_name, b"NTFS") || ascii_name_eq(filesystem_name, b"REFS"))
}

fn ascii_name_eq(actual: &[u16], expected: &[u8]) -> bool {
    actual.len() == expected.len()
        && actual.iter().zip(expected).all(|(actual, expected)| {
            let folded = if (u16::from(b'a')..=u16::from(b'z')).contains(actual) {
                *actual - u16::from(b'a' - b'A')
            } else {
                *actual
            };
            folded == u16::from(*expected)
        })
}

fn wide_component(value: &OsStr) -> Vec<u16> {
    value.encode_wide().collect()
}

fn raw_handle(file: &File) -> *mut core::ffi::c_void {
    file.as_raw_handle().cast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_identity_accepts_only_known_local_filesystems() {
        let ntfs = "NTFS".encode_utf16().collect::<Vec<_>>();
        let refs = "ReFS".encode_utf16().collect::<Vec<_>>();
        let custom = "CUSTOMFS".encode_utf16().collect::<Vec<_>>();

        assert!(volume_identity_is_proven(0, &ntfs));
        assert!(volume_identity_is_proven(0, &refs));
        assert!(!volume_identity_is_proven(FILE_REMOTE_DEVICE, &ntfs));
        assert!(!volume_identity_is_proven(0, &custom));
    }

    #[test]
    fn missing_identity_queries_are_classified_as_unproven() {
        for query in [
            IdentityQuery::FileId,
            IdentityQuery::LinkCount,
            IdentityQuery::AttributeAndReparseTag,
        ] {
            assert!(matches!(
                classify_identity_query_failure(query),
                FsError::Unproven {
                    operation: FsOperation::InspectHandle,
                    ..
                }
            ));
        }
    }
}
