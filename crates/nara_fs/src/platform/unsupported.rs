use std::fs::File;

use crate::{
    ConflictProtection, DirectorySyncTier, FileFacts, FileSyncTier, FsError, FsOperation, LockMode,
    ParentAuthorizationTier, PlatformCapabilityMatrix, ProofStatus, PublicationAtomicity,
    PublicationSourceBinding, RelativeComponent, ResolutionTier, TrustMode,
    platform::{DirectoryPublicationSupport, FileCreateMode, NativeDirectoryEntry},
};

pub(crate) struct DirectoryEnumerationCursor;

pub(crate) const fn capability_matrix() -> PlatformCapabilityMatrix {
    PlatformCapabilityMatrix {
        resolution: ResolutionTier::Unsupported,
        single_link: ProofStatus::Unsupported,
        replace_parent: ParentAuthorizationTier::Unsupported,
        publication: PublicationAtomicity::Unsupported,
        conflict: ConflictProtection::Unsupported,
        replace_source: PublicationSourceBinding::Unsupported,
        file_sync: FileSyncTier::Unsupported,
        directory_sync: DirectorySyncTier::Unsupported,
        advisory_lock: ProofStatus::Unsupported,
        read_directory: ProofStatus::Unsupported,
        relative_unlink: ProofStatus::Unsupported,
        non_overwrite_rename: ProofStatus::Unsupported,
    }
}

fn unsupported<T>(operation: FsOperation) -> Result<T, FsError> {
    Err(FsError::Unsupported {
        operation,
        capability: "no capability filesystem adapter exists for this target",
    })
}

pub(crate) fn begin_directory_enumeration(
    _directory: &File,
) -> Result<DirectoryEnumerationCursor, FsError> {
    unsupported(FsOperation::ReadDirectory)
}

pub(crate) fn next_directory_entry(
    _cursor: &mut DirectoryEnumerationCursor,
) -> Result<Option<NativeDirectoryEntry>, FsError> {
    unsupported(FsOperation::ReadDirectory)
}

pub(crate) fn facts(_file: &File) -> Result<FileFacts, FsError> {
    unsupported(FsOperation::InspectHandle)
}

pub(crate) const fn resolution_tier(_file: &File) -> ResolutionTier {
    ResolutionTier::Unsupported
}

pub(crate) const fn directory_publication_support(_file: &File) -> DirectoryPublicationSupport {
    DirectoryPublicationSupport::Unsupported
}

pub(crate) fn directory_publication_platform_identity(
    _file: &File,
    operation: FsOperation,
) -> Result<u64, FsError> {
    Err(FsError::Unsupported {
        operation,
        capability: "same-parent directory publication platform identity",
    })
}

pub(crate) fn open_directory(
    _parent: &File,
    _component: &RelativeComponent,
    _strict: bool,
) -> Result<File, FsError> {
    unsupported(FsOperation::OpenDirectory)
}

pub(crate) fn open_file(
    _parent: &File,
    _component: &RelativeComponent,
    _strict: bool,
) -> Result<File, FsError> {
    unsupported(FsOperation::OpenFile)
}

pub(crate) fn open_child(
    _parent: &File,
    _component: &RelativeComponent,
    operation: FsOperation,
) -> Result<File, FsError> {
    unsupported(operation)
}

pub(crate) fn create_directory_exclusive(
    _parent: &File,
    _component: &RelativeComponent,
    _trust: TrustMode,
) -> Result<super::DirectoryCreation, FsError> {
    unsupported(FsOperation::CreateDirectory)
}

pub(crate) fn create_file_exclusive(
    _parent: &File,
    _component: &RelativeComponent,
    operation: FsOperation,
    _mode: FileCreateMode,
) -> Result<File, FsError> {
    unsupported(operation)
}

pub(crate) fn publish_directory_no_replace(
    _parent: &File,
    _source: &File,
    _source_name: &RelativeComponent,
    _destination: &RelativeComponent,
) -> Result<(), FsError> {
    unsupported(FsOperation::PublishDirectory)
}

pub(crate) fn discard_directory_exact(_directory: &File) -> Result<(), FsError> {
    unsupported(FsOperation::RemoveDirectory)
}

pub(crate) fn discard_temporary(
    _parent: &File,
    _file: &File,
    _name: &RelativeComponent,
    _expected: crate::NativeFileIdentity,
) -> Result<(), FsError> {
    unsupported(FsOperation::RemoveTemporary)
}

pub(crate) fn replace_temporary(
    _parent: &File,
    _temporary: &File,
    _source_name: &RelativeComponent,
    _target_name: &RelativeComponent,
) -> Result<(), FsError> {
    unsupported(FsOperation::Replace)
}

pub(crate) fn publish_temporary_no_replace(
    _parent: &File,
    _temporary: &File,
    _source_name: &RelativeComponent,
    _target_name: &RelativeComponent,
) -> Result<(), FsError> {
    unsupported(FsOperation::Replace)
}

pub(crate) fn sync_file(_file: &File) -> Result<FileSyncTier, FsError> {
    unsupported(FsOperation::SyncFile)
}

pub(crate) fn sync_directory(_file: &File) -> Result<DirectorySyncTier, FsError> {
    unsupported(FsOperation::SyncDirectory)
}

pub(crate) fn try_lock(_file: &File, _mode: LockMode) -> Result<(), FsError> {
    unsupported(FsOperation::Lock)
}

pub(crate) fn unlock(_file: &File) -> Result<(), FsError> {
    unsupported(FsOperation::Unlock)
}
