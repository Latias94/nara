//! Capability-oriented filesystem primitives.
//!
//! Domain callers operate on host-issued handles and validated relative
//! components. This crate does not return an authorization-checked raw path.

mod capability;
mod digest;
mod directory_create;
mod directory_enumeration;
mod directory_publication;
mod error;
mod file;
mod host_handle;
mod identity;
mod lock;
mod path;
mod platform;
mod proof;
mod replace;
mod sync;
mod unlink;

pub use capability::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
pub use digest::{ContentDigest, DigestLimit};
pub use directory_create::{CreatedDirectoryCustody, DirectoryCreateFailure};
pub use directory_enumeration::{
    DirectoryEntryObservation, DirectoryEntryRejectedFact, DirectoryEntryRejectionReason,
    DirectoryEnumerationBatch, DirectoryEnumerationLimits, DirectoryEnumerationSession,
};
pub use directory_publication::{
    DirectoryCleanupEvidence, DirectoryPublicationBeginError, DirectoryPublicationCause,
    DirectoryPublicationOrphan, DirectoryPublicationOutcome, DirectoryPublicationReceipt,
    DirectoryPublicationRejection, DirectoryPublicationTransaction,
};
pub use error::{FsError, FsOperation, PathValidationError};
pub use file::{CapabilityReader, CapabilityWriter, FileCapability};
pub use host_handle::DirectoryNamespaceLease;
#[doc(hidden)]
pub use host_handle::RestrictiveCapabilityHandleLease;
pub use identity::{CapabilityGeneration, CapabilitySessionId, FileIdentity, FileKind};
pub use lock::{
    FileLockError, FileUnlockError, LockGuarantee, LockMode, LockScope, LockedFileCapability,
};
pub use path::{RelativeComponent, RelativePath, RelativePathPreflight};
pub use proof::{
    ConflictProtection, DirectorySyncTier, DurabilityProgress, FileSyncTier,
    ParentAuthorizationTier, PlatformCapabilityMatrix, ProofStatus, PublicationAtomicity,
    PublicationIdentityEvidence, PublicationSourceBinding, PublicationTopologyEvidence,
    ResolutionTier, StageStatus,
};
pub use replace::{
    ExpectedTarget, ReplaceReceipt, TemporaryFile, TemporaryFileCleanup, TemporaryFileFailure,
};
pub use sync::{DirectorySyncReceipt, FileSyncReceipt};
pub use unlink::FileUnlinkReceipt;

pub(crate) use identity::{FileFacts, NativeFileIdentity};

/// Reports the guarantees implemented by the current platform adapter.
#[must_use]
pub fn platform_capability_matrix() -> PlatformCapabilityMatrix {
    platform::capability_matrix()
}
