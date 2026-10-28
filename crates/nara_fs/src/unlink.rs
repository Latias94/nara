use crate::{FileIdentity, PublicationSourceBinding, StageStatus};

/// Successful removal of the link bound to one continuously held regular-file object.
///
/// The caller explicitly authorized unlinking that object, including any alternate data streams.
/// This is not evidence that its bytes were unchanged or that an installation owned those bytes.
/// Other readers may retain the unlinked object, a new object may occupy the same name, and
/// physical storage release and parent-directory crash durability remain separate.
#[derive(Debug)]
#[must_use = "observe the namespace result separately from domain protection and durability"]
pub struct FileUnlinkReceipt {
    identity: FileIdentity,
    parent: FileIdentity,
}

impl FileUnlinkReceipt {
    pub(crate) const fn new(identity: FileIdentity, parent: FileIdentity) -> Self {
        Self { identity, parent }
    }

    #[must_use]
    pub const fn identity(&self) -> FileIdentity {
        self.identity
    }

    #[must_use]
    pub const fn parent_identity(&self) -> FileIdentity {
        self.parent
    }

    #[must_use]
    pub const fn source_binding(&self) -> PublicationSourceBinding {
        PublicationSourceBinding::HandleBound
    }

    /// The native deleting handle closed after successful POSIX disposition.
    #[must_use]
    pub const fn name_removed(&self) -> StageStatus {
        StageStatus::Achieved
    }

    #[must_use]
    pub const fn parent_directory_synced(&self) -> StageStatus {
        StageStatus::Unsupported
    }
}
