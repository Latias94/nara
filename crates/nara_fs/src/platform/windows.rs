use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io,
    mem::{offset_of, size_of, size_of_val},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle},
    },
    ptr,
};

use windows_sys::Wdk::{
    Foundation::OBJECT_ATTRIBUTES,
    Storage::FileSystem::{
        FILE_CREATE, FILE_DIRECTORY_FILE, FILE_ID_BOTH_DIR_INFORMATION, FILE_NAME_INFORMATION,
        FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT,
        FileFsDeviceInformation, FileIdBothDirectoryInformation, FileNormalizedNameInformation,
        FileRenameInformationEx, NtCreateFile, NtOpenFile, NtQueryDirectoryFileEx,
        NtQueryInformationFile, NtQueryVolumeInformationFile, NtSetInformationFile,
    },
    System::SystemServices::{RtlGetVersion, SL_RESTART_SCAN, SL_RETURN_SINGLE_ENTRY},
};
use windows_sys::Win32::{
    Foundation::{
        ERROR_ALREADY_EXISTS, ERROR_CALL_NOT_IMPLEMENTED, ERROR_FILE_EXISTS,
        ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SAME_DEVICE,
        ERROR_NOT_SUPPORTED, RtlNtStatusToDosError, STATUS_BUFFER_OVERFLOW, STATUS_NO_MORE_FILES,
        STATUS_NO_SUCH_FILE, UNICODE_STRING,
    },
    Storage::FileSystem::{
        DELETE, FILE_ATTRIBUTE_DEVICE, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_DISPOSITION_FLAG_DELETE,
        FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
        FILE_DISPOSITION_INFO_EX, FILE_EXECUTE, FILE_ID_INFO, FILE_LIST_DIRECTORY,
        FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_READ_EA, FILE_RENAME_INFO, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FILE_TRAVERSE,
        FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, FileAttributeTagInfo,
        FileDispositionInfoEx, FileIdInfo, FileStandardInfo, FlushFileBuffers,
        GetFileInformationByHandleEx, GetVolumeInformationByHandleW, LOCKFILE_EXCLUSIVE_LOCK,
        LOCKFILE_FAIL_IMMEDIATELY, LockFileEx, SYNCHRONIZE, SetFileInformationByHandle,
        UnlockFileEx,
    },
    System::IO::{IO_STATUS_BLOCK, OVERLAPPED},
    System::SystemInformation::OSVERSIONINFOW,
};

use crate::{
    ConflictProtection, DirectorySyncTier, FileFacts, FileKind, FileSyncTier, FsError, FsOperation,
    LockMode, NativeFileIdentity, ParentAuthorizationTier, PlatformCapabilityMatrix, ProofStatus,
    PublicationAtomicity, PublicationSourceBinding, RelativeComponent, ResolutionTier, TrustMode,
    platform::{DirectoryPublicationSupport, FileCreateMode, NativeDirectoryEntry},
};

const OBJ_CASE_INSENSITIVE: u32 = 0x40;
const EXACT_NAME_BUFFER_BYTES: usize = 65_536;
const FILE_REMOTE_DEVICE: u32 = 0x10;
const FILE_RENAME_REPLACE_IF_EXISTS: u32 = 0x1;
const FILE_RENAME_POSIX_SEMANTICS: u32 = 0x2;
const DIRECTORY_ENUMERATION_BUFFER_BYTES: usize = 4_096;
const DIRECTORY_ENUMERATION_BUFFER_WORDS: usize =
    (DIRECTORY_ENUMERATION_BUFFER_BYTES + size_of::<usize>() - 1) / size_of::<usize>();

#[repr(C)]
#[derive(Default)]
struct FileFsDeviceInformation {
    device_type: u32,
    characteristics: u32,
}

pub(crate) struct DirectoryEnumerationCursor {
    file: File,
    restart_scan: bool,
    buffer: Box<[usize; DIRECTORY_ENUMERATION_BUFFER_WORDS]>,
}

pub(crate) fn write_at(file: &File, buffer: &[u8], offset: u64) -> io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_write(buffer, offset)
}

pub(crate) fn restrictive_clone_directory(file: &File) -> Result<File, FsError> {
    restrictive_exact_self(
        file,
        FileKind::Directory,
        FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_DIRECTORY_FILE,
        "restrictive exact-self directory lease",
    )
}

pub(crate) fn restrictive_clone_file(file: &File) -> Result<File, FsError> {
    restrictive_exact_self(
        file,
        FileKind::Regular,
        FILE_READ_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_NON_DIRECTORY_FILE,
        "restrictive exact-self file lease",
    )
}

pub(crate) fn restrictive_clone_executable(file: &File) -> Result<File, FsError> {
    restrictive_exact_self(
        file,
        FileKind::Regular,
        FILE_READ_DATA | FILE_EXECUTE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_NON_DIRECTORY_FILE,
        "restrictive exact-self executable lease",
    )
}

fn restrictive_exact_self(
    source: &File,
    expected_kind: FileKind,
    desired_access: u32,
    type_options: u32,
    capability: &'static str,
) -> Result<File, FsError> {
    let original = facts(source)?;
    if original.kind != expected_kind {
        return match expected_kind {
            FileKind::Directory => Err(FsError::NotDirectory),
            FileKind::Regular => Err(FsError::NotRegularFile),
            FileKind::Other => Err(FsError::Unproven {
                operation: FsOperation::CloneHandle,
                proof: "exact launch leases cannot target device or other non-file objects",
            }),
        };
    }
    if let Some(tag) = original.reparse_tag {
        return Err(FsError::ReparsePoint { tag });
    }
    if !original.identity_proven {
        return Err(FsError::Unproven {
            operation: FsOperation::CloneHandle,
            proof: "the Windows handle cannot prove local live-object identity for an exact lease",
        });
    }

    let empty_name = UNICODE_STRING {
        Length: 0,
        MaximumLength: 0,
        Buffer: ptr::null_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: raw_handle(source),
        ObjectName: ptr::addr_of!(empty_name),
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: ptr::null(),
        SecurityQualityOfService: ptr::null(),
    };
    let mut status_block = IO_STATUS_BLOCK::default();
    let mut handle = ptr::null_mut();
    // SAFETY: the source handle is the capability owner’s live authority. The empty relative name
    // asks the NT object manager to reopen that exact object; no ambient path is consulted. The
    // restrictive share mode prevents later ordinary write/delete opens while the lease lives.
    let status = unsafe {
        NtOpenFile(
            &mut handle,
            desired_access,
            ptr::addr_of!(attributes),
            &mut status_block,
            FILE_SHARE_READ,
            FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT | type_options,
        )
    };
    if status != 0 {
        let code = unsafe { RtlNtStatusToDosError(status) };
        if matches!(
            code,
            ERROR_NOT_SUPPORTED | ERROR_CALL_NOT_IMPLEMENTED | ERROR_INVALID_FUNCTION
        ) {
            return Err(FsError::Unsupported {
                operation: FsOperation::CloneHandle,
                capability,
            });
        }
        if code == 0 {
            return Err(FsError::Unproven {
                operation: FsOperation::CloneHandle,
                proof: "the Windows exact-self lease returned a non-success NTSTATUS",
            });
        }
        return Err(FsError::io(
            FsOperation::CloneHandle,
            io::Error::from_raw_os_error(code as i32),
        ));
    }
    if handle.is_null() {
        return Err(FsError::IdentityUnavailable);
    }

    // SAFETY: strict `NtOpenFile` success transferred unique ownership of `handle`.
    let reopened = unsafe { File::from_raw_handle(handle.cast()) };
    let current = facts(&reopened)?;
    if current.kind != expected_kind {
        return Err(FsError::CapabilityIdentityDrift {
            operation: FsOperation::CloneHandle,
        });
    }
    if let Some(tag) = current.reparse_tag {
        return Err(FsError::ReparsePoint { tag });
    }
    if !current.identity_proven
        || current.identity != original.identity
        || current.link_count != original.link_count
    {
        return Err(FsError::CapabilityIdentityDrift {
            operation: FsOperation::CloneHandle,
        });
    }
    Ok(reopened)
}

pub(crate) const fn capability_matrix() -> PlatformCapabilityMatrix {
    PlatformCapabilityMatrix {
        resolution: ResolutionTier::HandleBound,
        single_link: ProofStatus::Proven,
        replace_parent: ParentAuthorizationTier::HandleBound,
        publication: PublicationAtomicity::AtomicNameSwitch,
        conflict: ConflictProtection::DetectOnly,
        replace_source: PublicationSourceBinding::HandleBound,
        file_sync: FileSyncTier::DataAndMetadata,
        directory_sync: DirectorySyncTier::Unsupported,
        advisory_lock: ProofStatus::Proven,
        read_directory: if cfg!(target_arch = "x86_64") {
            ProofStatus::Proven
        } else {
            ProofStatus::Unsupported
        },
        relative_unlink: ProofStatus::Unsupported,
        non_overwrite_rename: ProofStatus::Unsupported,
    }
}

pub(crate) const fn resolution_tier(_file: &File) -> ResolutionTier {
    ResolutionTier::HandleBound
}

pub(crate) fn begin_directory_enumeration(
    directory: &File,
) -> Result<DirectoryEnumerationCursor, FsError> {
    if !cfg!(target_arch = "x86_64") {
        return Err(FsError::Unsupported {
            operation: FsOperation::ReadDirectory,
            capability: "independent Windows directory enumeration is supported only on x86-64",
        });
    }
    match windows_1809_or_later() {
        Some(true) => {}
        Some(false) => {
            return Err(FsError::Unsupported {
                operation: FsOperation::ReadDirectory,
                capability: "NtQueryDirectoryFileEx requires the supported Windows baseline",
            });
        }
        None => {
            return Err(FsError::Unproven {
                operation: FsOperation::ReadDirectory,
                proof: "the Windows version required by the enumeration adapter is unavailable",
            });
        }
    }
    match volume_semantics(directory) {
        WindowsVolumeSemantics::LocalNtfs => {}
        WindowsVolumeSemantics::LocalRefs | WindowsVolumeSemantics::OtherLocal => {
            return Err(FsError::Unsupported {
                operation: FsOperation::ReadDirectory,
                capability: "strict directory enumeration is proven only for local NTFS",
            });
        }
        WindowsVolumeSemantics::Remote | WindowsVolumeSemantics::Unproven => {
            return Err(FsError::Unproven {
                operation: FsOperation::ReadDirectory,
                proof: "the live directory handle cannot prove local NTFS enumeration semantics",
            });
        }
    }

    let original = facts(directory)?;
    if original.kind != FileKind::Directory {
        return Err(FsError::NotDirectory);
    }
    if let Some(tag) = original.reparse_tag {
        return Err(FsError::ReparsePoint { tag });
    }
    let empty_name = UNICODE_STRING {
        Length: 0,
        MaximumLength: 0,
        Buffer: ptr::null_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: raw_handle(directory),
        ObjectName: ptr::addr_of!(empty_name),
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: ptr::null(),
        SecurityQualityOfService: ptr::null(),
    };
    let mut status_block = IO_STATUS_BLOCK::default();
    let mut handle = ptr::null_mut();
    // SAFETY: the retained directory handle is the only root authority; the empty relative name
    // reopens that exact directory without consulting an ambient path. The output handle is
    // synchronous and explicitly refuses reparse traversal.
    let status = unsafe {
        NtOpenFile(
            &mut handle,
            FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            ptr::addr_of!(attributes),
            &mut status_block,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
        )
    };
    if status < 0 {
        // SAFETY: conversion accepts the returned NTSTATUS value.
        let code = unsafe { RtlNtStatusToDosError(status) };
        if matches!(
            code,
            ERROR_NOT_SUPPORTED | ERROR_CALL_NOT_IMPLEMENTED | ERROR_INVALID_FUNCTION
        ) {
            return Err(FsError::Unsupported {
                operation: FsOperation::ReadDirectory,
                capability: "independent exact-self Windows directory handle reopen",
            });
        }
        return Err(FsError::io(
            FsOperation::ReadDirectory,
            io::Error::from_raw_os_error(code as i32),
        ));
    }
    if handle.is_null() {
        return Err(FsError::IdentityUnavailable);
    }
    // SAFETY: successful `NtOpenFile` returned unique ownership of `handle`.
    let file = unsafe { File::from_raw_handle(handle.cast()) };
    let reopened = facts(&file).map_err(|_| FsError::Unproven {
        operation: FsOperation::ReadDirectory,
        proof: "the independent enumeration handle lacks exact live-object facts",
    })?;
    if reopened.kind != FileKind::Directory
        || reopened.reparse_tag.is_some()
        || reopened.identity != original.identity
    {
        return Err(FsError::CapabilityIdentityDrift {
            operation: FsOperation::ReadDirectory,
        });
    }
    if !matches!(volume_semantics(&file), WindowsVolumeSemantics::LocalNtfs) {
        return Err(FsError::Unproven {
            operation: FsOperation::ReadDirectory,
            proof: "the independent enumeration handle changed filesystem semantics",
        });
    }

    Ok(DirectoryEnumerationCursor {
        file,
        restart_scan: true,
        buffer: Box::new([0; DIRECTORY_ENUMERATION_BUFFER_WORDS]),
    })
}

pub(crate) fn next_directory_entry(
    cursor: &mut DirectoryEnumerationCursor,
) -> Result<Option<NativeDirectoryEntry>, FsError> {
    loop {
        cursor.buffer.fill(0);
        let mut status_block = IO_STATUS_BLOCK::default();
        let query_flags = SL_RETURN_SINGLE_ENTRY
            | if cursor.restart_scan {
                SL_RESTART_SCAN
            } else {
                0
            };
        // SAFETY: the cursor owns a synchronous directory handle and an aligned writable fixed
        // buffer. No event, APC, filter name, or asynchronous completion state is requested.
        let status = unsafe {
            NtQueryDirectoryFileEx(
                raw_handle(&cursor.file),
                ptr::null_mut(),
                None,
                ptr::null(),
                &mut status_block,
                cursor.buffer.as_mut_ptr().cast(),
                DIRECTORY_ENUMERATION_BUFFER_BYTES as u32,
                FileIdBothDirectoryInformation,
                query_flags,
                ptr::null(),
            )
        };
        cursor.restart_scan = false;
        if status == STATUS_NO_MORE_FILES || status == STATUS_NO_SUCH_FILE {
            return Ok(None);
        }
        if status == STATUS_BUFFER_OVERFLOW {
            return Err(FsError::DirectoryEnumerationIncomplete);
        }
        if status < 0 {
            // SAFETY: conversion accepts the returned NTSTATUS value.
            let code = unsafe { RtlNtStatusToDosError(status) };
            return Err(FsError::io(
                FsOperation::ReadDirectory,
                io::Error::from_raw_os_error(code as i32),
            ));
        }

        let bytes = status_block.Information;
        let name_offset = offset_of!(FILE_ID_BOTH_DIR_INFORMATION, FileName);
        if bytes < name_offset || bytes > DIRECTORY_ENUMERATION_BUFFER_BYTES {
            return Err(FsError::DirectoryEnumerationIncomplete);
        }
        let info = cursor
            .buffer
            .as_ptr()
            .cast::<FILE_ID_BOTH_DIR_INFORMATION>();
        // SAFETY: the successful query wrote at least the fixed record prefix into the aligned
        // buffer; all variable-length bounds are checked before the name slice is constructed.
        let (next_offset, name_bytes, attributes, name_ptr) = unsafe {
            (
                (*info).NextEntryOffset,
                (*info).FileNameLength as usize,
                (*info).FileAttributes,
                ptr::addr_of!((*info).FileName).cast::<u16>(),
            )
        };
        if next_offset != 0
            || name_bytes == 0
            || !name_bytes.is_multiple_of(size_of::<u16>())
            || name_offset
                .checked_add(name_bytes)
                .is_none_or(|end| end > bytes)
        {
            return Err(FsError::DirectoryEnumerationIncomplete);
        }
        // SAFETY: `name_bytes` was checked against the returned record length and is an even number
        // of bytes, so the UTF-16 slice lies wholly inside the fixed buffer.
        let name_units = unsafe { std::slice::from_raw_parts(name_ptr, name_bytes / 2) };
        if matches!(name_units, [unit] if *unit == u16::from(b'.'))
            || matches!(name_units, [first, second] if *first == u16::from(b'.') && *second == u16::from(b'.'))
        {
            continue;
        }
        let kind = if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
            FileKind::Directory
        } else if attributes & FILE_ATTRIBUTE_DEVICE != 0 {
            FileKind::Other
        } else {
            FileKind::Regular
        };
        return Ok(Some(NativeDirectoryEntry {
            name: OsString::from_wide(name_units),
            name_bytes,
            kind,
            reparse: attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0,
        }));
    }
}

pub(crate) fn directory_publication_support(file: &File) -> DirectoryPublicationSupport {
    if !cfg!(target_arch = "x86_64") {
        return DirectoryPublicationSupport::Unsupported;
    }
    match windows_1809_or_later() {
        Some(true) => {}
        Some(false) => return DirectoryPublicationSupport::Unsupported,
        None => return DirectoryPublicationSupport::Unproven,
    }
    match volume_semantics(file) {
        WindowsVolumeSemantics::LocalNtfs => {
            DirectoryPublicationSupport::Proven(PublicationSourceBinding::HandleBound)
        }
        WindowsVolumeSemantics::LocalRefs | WindowsVolumeSemantics::OtherLocal => {
            DirectoryPublicationSupport::Unsupported
        }
        WindowsVolumeSemantics::Remote | WindowsVolumeSemantics::Unproven => {
            DirectoryPublicationSupport::Unproven
        }
    }
}

pub(crate) fn directory_publication_platform_identity(
    file: &File,
    operation: FsOperation,
) -> Result<u64, FsError> {
    let facts = facts(file).map_err(|_| FsError::Unproven {
        operation,
        proof: "the live directory handle volume identity is unavailable",
    })?;
    let NativeFileIdentity::Windows { volume_serial, .. } = facts.identity;
    Ok(volume_serial)
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
        identity_proven: matches!(
            volume_semantics(file),
            WindowsVolumeSemantics::LocalNtfs | WindowsVolumeSemantics::LocalRefs
        ),
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

pub(crate) fn verify_exact_spelling(
    file: &File,
    component: &RelativeComponent,
    operation: FsOperation,
) -> Result<(), FsError> {
    if !matches!(
        volume_semantics(file),
        WindowsVolumeSemantics::LocalNtfs | WindowsVolumeSemantics::LocalRefs
    ) {
        return Err(FsError::Unproven {
            operation,
            proof: "exact spelling requires local NTFS/ReFS normalized-name semantics",
        });
    }

    // Clearing OBJ_CASE_INSENSITIVE alone is insufficient under Windows' global compatibility
    // policy. Query the opened link's stored long name instead ([MS-FSA] FileNormalizedNameInformation).
    // The fixed allocation and single call deliberately reject overflow without a growing retry.
    let mut storage = vec![0_u32; EXACT_NAME_BUFFER_BYTES / size_of::<u32>()];
    let info = storage.as_mut_ptr().cast::<FILE_NAME_INFORMATION>();
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: `file` is live; `storage` is aligned for FILE_NAME_INFORMATION and remains writable
    // for exactly the supplied byte count throughout this synchronous query.
    let status = unsafe {
        NtQueryInformationFile(
            raw_handle(file),
            &mut status_block,
            info.cast(),
            EXACT_NAME_BUFFER_BYTES as u32,
            FileNormalizedNameInformation,
        )
    };
    if status != 0 {
        return Err(FsError::Unproven {
            operation,
            proof: "the opened handle did not provide a complete normalized name within 64 KiB",
        });
    }

    let offset = offset_of!(FILE_NAME_INFORMATION, FileName);
    // SAFETY: the fixed allocation contains the initialized header even if a faulty provider
    // returned too little data. Validate both reported lengths before accessing trailing units.
    let name_bytes = unsafe { (*info).FileNameLength as usize };
    let returned = status_block.Information;
    if returned > EXACT_NAME_BUFFER_BYTES
        || returned < offset
        || name_bytes == 0
        || name_bytes % size_of::<u16>() != 0
        || name_bytes > returned - offset
    {
        return Err(FsError::Unproven {
            operation,
            proof: "normalized-name query returned incomplete or invalid name evidence",
        });
    }
    // SAFETY: `info` is suitably aligned; the checked byte range lies entirely in `storage`.
    let name = unsafe {
        std::slice::from_raw_parts(
            ptr::addr_of!((*info).FileName).cast::<u16>(),
            name_bytes / size_of::<u16>(),
        )
    };
    verify_normalized_leaf(name, component, operation)
}

fn verify_normalized_leaf(
    name: &[u16],
    component: &RelativeComponent,
    operation: FsOperation,
) -> Result<(), FsError> {
    if name.first() != Some(&u16::from(b'\\')) || name.last() == Some(&u16::from(b'\\')) {
        return Err(FsError::Unproven {
            operation,
            proof: "normalized-name evidence is not an absolute file name",
        });
    }
    let leaf = name
        .rsplit(|unit| *unit == u16::from(b'\\'))
        .next()
        .unwrap_or_default();
    if leaf.iter().copied().eq(component.as_os_str().encode_wide()) {
        Ok(())
    } else {
        Err(FsError::ExactSpellingMismatch { operation })
    }
}

pub(crate) fn open_child(
    parent: &File,
    component: &RelativeComponent,
    operation: FsOperation,
) -> Result<File, FsError> {
    nt_open(
        parent,
        component,
        FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_OPEN,
        0,
        operation,
    )
}

pub(crate) fn create_directory_exclusive(
    parent: &File,
    component: &RelativeComponent,
    _trust: TrustMode,
) -> Result<super::DirectoryCreation, FsError> {
    nt_open(
        parent,
        component,
        FILE_LIST_DIRECTORY
            | FILE_TRAVERSE
            | FILE_READ_ATTRIBUTES
            | FILE_WRITE_ATTRIBUTES
            | DELETE
            | SYNCHRONIZE,
        FILE_CREATE,
        FILE_DIRECTORY_FILE,
        FsOperation::CreateDirectory,
    )
    .map(super::DirectoryCreation::Opened)
}

pub(crate) fn create_file_exclusive(
    parent: &File,
    component: &RelativeComponent,
    operation: FsOperation,
    _mode: FileCreateMode,
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
        operation,
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
    rename_open_handle(
        parent,
        temporary,
        target_name,
        FILE_RENAME_REPLACE_IF_EXISTS | FILE_RENAME_POSIX_SEMANTICS,
    )
    .map_err(|code| {
        FsError::io(
            FsOperation::Replace,
            io::Error::from_raw_os_error(code as i32),
        )
    })
}

pub(crate) fn publish_temporary_no_replace(
    parent: &File,
    temporary: &File,
    _source_name: &RelativeComponent,
    target_name: &RelativeComponent,
) -> Result<(), FsError> {
    rename_open_handle(parent, temporary, target_name, 0)
        .map_err(|code| classify_no_replace_publication_error(code, FsOperation::Replace))
}

pub(crate) fn publish_directory_no_replace(
    parent: &File,
    source: &File,
    _source_name: &RelativeComponent,
    destination: &RelativeComponent,
) -> Result<(), FsError> {
    rename_open_handle(
        parent,
        source,
        destination,
        DIRECTORY_PUBLICATION_RENAME_FLAGS,
    )
    .map_err(classify_directory_publication_error)
}

// Absence of `FILE_RENAME_REPLACE_IF_EXISTS` is the no-replace contract. POSIX semantics are
// intentionally not requested: that flag only changes replacement behavior and is irrelevant to
// an absent-to-present publication.
const DIRECTORY_PUBLICATION_RENAME_FLAGS: u32 = 0;

pub(crate) fn discard_directory_exact(directory: &File) -> Result<(), FsError> {
    let disposition = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_FLAG_DELETE
            | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS
            | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    };
    // SAFETY: `directory` owns a live handle with delete authority and `disposition` has the exact
    // layout required by `FileDispositionInfoEx` for the duration of this call.
    let succeeded = unsafe {
        SetFileInformationByHandle(
            raw_handle(directory),
            FileDispositionInfoEx,
            ptr::addr_of!(disposition).cast(),
            size_of_val(&disposition) as u32,
        )
    };
    if succeeded != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error().map(|code| code as u32) {
        Some(ERROR_NOT_SUPPORTED | ERROR_CALL_NOT_IMPLEMENTED | ERROR_INVALID_FUNCTION) => {
            Err(FsError::Unsupported {
                operation: FsOperation::RemoveDirectory,
                capability: "handle-bound empty-directory removal with POSIX disposition semantics",
            })
        }
        Some(ERROR_INVALID_PARAMETER) => Err(FsError::Unproven {
            operation: FsOperation::RemoveDirectory,
            proof: "the live filesystem rejected the required POSIX directory disposition flags",
        }),
        _ => Err(FsError::io(FsOperation::RemoveDirectory, error)),
    }
}

fn rename_open_handle(
    parent: &File,
    source: &File,
    destination: &RelativeComponent,
    flags: u32,
) -> Result<(), u32> {
    let target = wide_component(destination.as_os_str());
    let bytes = size_of::<FILE_RENAME_INFO>() + target.len() * size_of::<u16>();
    let words = bytes.div_ceil(size_of::<usize>());
    let mut storage = vec![0_usize; words];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();

    // SAFETY: `storage` is aligned for `FILE_RENAME_INFO` and sized for the
    // trailing UTF-16 name. Every field is initialized before the syscall.
    unsafe {
        (*info).Anonymous.Flags = flags;
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
            raw_handle(source),
            &mut status_block,
            info.cast(),
            bytes as u32,
            FileRenameInformationEx,
        )
    };
    if status < 0 {
        // SAFETY: conversion accepts the returned NTSTATUS value.
        let code = unsafe { RtlNtStatusToDosError(status) };
        Err(code)
    } else {
        Ok(())
    }
}

fn classify_directory_publication_error(code: u32) -> FsError {
    classify_no_replace_publication_error(code, FsOperation::PublishDirectory)
}

fn classify_no_replace_publication_error(code: u32, operation: FsOperation) -> FsError {
    match code {
        ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS => FsError::AlreadyExists { operation },
        ERROR_NOT_SAME_DEVICE => FsError::CrossVolume,
        ERROR_NOT_SUPPORTED | ERROR_CALL_NOT_IMPLEMENTED | ERROR_INVALID_FUNCTION => {
            FsError::Unsupported {
                operation,
                capability: "handle-bound no-replace publication",
            }
        }
        ERROR_INVALID_PARAMETER => FsError::Unproven {
            operation,
            proof: "the live filesystem rejected the required no-replace rename flags",
        },
        _ => FsError::io(operation, io::Error::from_raw_os_error(code as i32)),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WindowsVolumeSemantics {
    LocalNtfs,
    LocalRefs,
    OtherLocal,
    Remote,
    Unproven,
}

fn volume_semantics(file: &File) -> WindowsVolumeSemantics {
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
        return WindowsVolumeSemantics::Unproven;
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
        return WindowsVolumeSemantics::Unproven;
    }
    let length = filesystem_name
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(filesystem_name.len());
    classify_volume_semantics(info.characteristics, &filesystem_name[..length])
}

#[cfg(test)]
fn volume_identity_is_proven(characteristics: u32, filesystem_name: &[u16]) -> bool {
    matches!(
        classify_volume_semantics(characteristics, filesystem_name),
        WindowsVolumeSemantics::LocalNtfs | WindowsVolumeSemantics::LocalRefs
    )
}

fn classify_volume_semantics(
    characteristics: u32,
    filesystem_name: &[u16],
) -> WindowsVolumeSemantics {
    if characteristics & FILE_REMOTE_DEVICE != 0 {
        return WindowsVolumeSemantics::Remote;
    }
    if ascii_name_eq(filesystem_name, b"NTFS") {
        WindowsVolumeSemantics::LocalNtfs
    } else if ascii_name_eq(filesystem_name, b"REFS") {
        WindowsVolumeSemantics::LocalRefs
    } else {
        WindowsVolumeSemantics::OtherLocal
    }
}

fn windows_1809_or_later() -> Option<bool> {
    let mut version = OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
        ..OSVERSIONINFOW::default()
    };
    // SAFETY: `version` has the required size field and remains writable for the call.
    let status = unsafe { RtlGetVersion(&mut version) };
    if status < 0 {
        return None;
    }
    Some(
        version.dwMajorVersion > 10
            || (version.dwMajorVersion == 10 && version.dwBuildNumber >= 17_763),
    )
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
    fn exact_normalized_leaf_uses_raw_utf16_long_name_spelling() {
        let name = "\\Host\\Assets\\caf\u{00e9}.scene.json"
            .encode_utf16()
            .collect::<Vec<_>>();
        let operation = FsOperation::OpenFile;
        assert!(
            verify_normalized_leaf(
                &name,
                &RelativeComponent::new("caf\u{00e9}.scene.json").unwrap(),
                operation,
            )
            .is_ok()
        );
        for alias in [
            "CAF\u{00c9}.SCENE.JSON",
            "cafe\u{0301}.scene.json",
            "CAFE~1.JSO",
        ] {
            assert!(matches!(
                verify_normalized_leaf(&name, &RelativeComponent::new(alias).unwrap(), operation),
                Err(FsError::ExactSpellingMismatch {
                    operation: FsOperation::OpenFile
                })
            ));
        }
        // No lossy UTF-16 -> UTF-8 conversion: a native unpaired surrogate is preserved too.
        let component = OsString::from_wide(&[0xd800, u16::from(b'x')]);
        let native_name = [u16::from(b'\\'), 0xd800, u16::from(b'x')];
        assert!(
            verify_normalized_leaf(
                &native_name,
                &RelativeComponent::new(component).unwrap(),
                operation,
            )
            .is_ok()
        );
    }

    #[test]
    fn exact_normalized_leaf_rejects_missing_absolute_name_evidence() {
        let component = RelativeComponent::new("value.txt").unwrap();
        for name in ["", "value.txt", "\\", "\\Host\\"] {
            assert!(matches!(
                verify_normalized_leaf(
                    &name.encode_utf16().collect::<Vec<_>>(),
                    &component,
                    FsOperation::OpenFile,
                ),
                Err(FsError::Unproven { .. })
            ));
        }
    }

    #[test]
    fn directory_publication_never_requests_replacement() {
        assert_eq!(DIRECTORY_PUBLICATION_RENAME_FLAGS, 0);
        assert_eq!(
            DIRECTORY_PUBLICATION_RENAME_FLAGS & FILE_RENAME_REPLACE_IF_EXISTS,
            0
        );
    }

    #[test]
    fn directory_publication_native_failures_keep_collision_and_proof_distinct() {
        assert!(matches!(
            classify_directory_publication_error(ERROR_ALREADY_EXISTS),
            FsError::AlreadyExists {
                operation: FsOperation::PublishDirectory
            }
        ));
        assert!(matches!(
            classify_directory_publication_error(ERROR_NOT_SAME_DEVICE),
            FsError::CrossVolume
        ));
        assert!(matches!(
            classify_directory_publication_error(ERROR_NOT_SUPPORTED),
            FsError::Unsupported {
                operation: FsOperation::PublishDirectory,
                ..
            }
        ));
        assert!(matches!(
            classify_directory_publication_error(ERROR_INVALID_FUNCTION),
            FsError::Unsupported {
                operation: FsOperation::PublishDirectory,
                ..
            }
        ));
        assert!(matches!(
            classify_directory_publication_error(ERROR_INVALID_PARAMETER),
            FsError::Unproven {
                operation: FsOperation::PublishDirectory,
                ..
            }
        ));
    }

    #[test]
    fn strict_identity_accepts_only_known_local_filesystems() {
        let ntfs = "NTFS".encode_utf16().collect::<Vec<_>>();
        let refs = "ReFS".encode_utf16().collect::<Vec<_>>();
        let custom = "CUSTOMFS".encode_utf16().collect::<Vec<_>>();

        assert!(volume_identity_is_proven(0, &ntfs));
        assert!(volume_identity_is_proven(0, &refs));
        assert!(!volume_identity_is_proven(FILE_REMOTE_DEVICE, &ntfs));
        assert!(!volume_identity_is_proven(0, &custom));
        assert_eq!(
            classify_volume_semantics(0, &ntfs),
            WindowsVolumeSemantics::LocalNtfs
        );
        assert_eq!(
            classify_volume_semantics(0, &refs),
            WindowsVolumeSemantics::LocalRefs
        );
        assert_eq!(
            classify_volume_semantics(FILE_REMOTE_DEVICE, &ntfs),
            WindowsVolumeSemantics::Remote
        );
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
