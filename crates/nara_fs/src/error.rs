use std::io;

use thiserror::Error;

use crate::{ContentDigest, FileIdentity};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsOperation {
    InspectHandle,
    OpenDirectory,
    OpenFile,
    ReadDirectory,
    CreateTemporary,
    RemoveTemporary,
    Rename,
    Unlink,
    Replace,
    SyncFile,
    SyncDirectory,
    Lock,
    Unlock,
    CloneHandle,
    Read,
}

impl FsOperation {
    const fn label(self) -> &'static str {
        match self {
            Self::InspectHandle => "inspect handle",
            Self::OpenDirectory => "open directory",
            Self::OpenFile => "open file",
            Self::ReadDirectory => "read directory",
            Self::CreateTemporary => "create temporary file",
            Self::RemoveTemporary => "remove temporary file",
            Self::Rename => "rename file",
            Self::Unlink => "unlink file",
            Self::Replace => "replace file",
            Self::SyncFile => "synchronize file",
            Self::SyncDirectory => "synchronize directory",
            Self::Lock => "lock file",
            Self::Unlock => "unlock file",
            Self::CloneHandle => "clone file handle",
            Self::Read => "read file",
        }
    }
}

impl std::fmt::Display for FsOperation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum PathValidationError {
    #[error("relative path must contain at least one component")]
    Empty,
    #[error("absolute paths and platform prefixes are not allowed")]
    AbsoluteOrPrefixed,
    #[error("current-directory components are not allowed")]
    CurrentDirectory,
    #[error("parent traversal is not allowed")]
    ParentTraversal,
    #[error("empty or trailing path components are not allowed")]
    EmptyComponent,
    #[error("path component contains a forbidden character")]
    ForbiddenCharacter,
    #[error("platform device names are not allowed")]
    ReservedDeviceName,
    #[error("path component exceeds the supported length")]
    ComponentTooLong,
    #[error("relative path exceeds the supported component or byte limit")]
    PathTooLong,
}

/// Failures reported by filesystem capabilities and operations.
///
/// New variants may be added as platform adapters prove additional rejection causes.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FsError {
    #[error(transparent)]
    Path(#[from] PathValidationError),
    #[error("{operation} failed")]
    Io {
        operation: FsOperation,
        #[source]
        source: io::Error,
    },
    #[error("{operation} is unsupported: {capability}")]
    Unsupported {
        operation: FsOperation,
        capability: &'static str,
    },
    #[error("{operation} cannot prove the required invariant: {proof}")]
    Unproven {
        operation: FsOperation,
        proof: &'static str,
    },
    #[error("a read-only directory capability cannot perform {operation}")]
    ReadOnlyCapability { operation: FsOperation },
    #[error("host-issued handle is not a directory")]
    NotDirectory,
    #[error("opened object is not a regular file")]
    NotRegularFile,
    #[error("reparse traversal is forbidden (tag {tag:#x})")]
    ReparsePoint { tag: u32 },
    #[error("symbolic-link traversal is forbidden")]
    SymbolicLinkTraversal,
    #[error("opened object crossed the capability volume or device boundary")]
    CrossVolume,
    #[error("strict mode requires a single-link file, found {links} links")]
    MultipleLinks { links: u64 },
    #[error("platform did not provide stable file identity")]
    IdentityUnavailable,
    #[error("capability session identifier space is exhausted")]
    CapabilitySessionExhausted,
    #[error("file identity did not match the expected object")]
    IdentityMismatch {
        expected: FileIdentity,
        actual: FileIdentity,
    },
    #[error("temporary file belongs to a different directory capability")]
    TemporaryParentMismatch,
    #[error("exclusive temporary name already exists")]
    AlreadyExists { operation: FsOperation },
    #[error("replacement target does not match the requested state")]
    TargetStateMismatch,
    #[error("file lock is already held")]
    LockContended,
    #[error("read exceeded the byte limit of {limit}")]
    ByteLimitExceeded { limit: u64 },
    #[error("content digest does not match")]
    DigestMismatch {
        expected: ContentDigest,
        actual: ContentDigest,
    },
}

impl FsError {
    pub(crate) fn io(operation: FsOperation, source: io::Error) -> Self {
        Self::Io { operation, source }
    }
}
