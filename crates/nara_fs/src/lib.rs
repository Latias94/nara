//! Capability-oriented filesystem primitives.
//!
//! Domain callers operate on host-issued handles and validated relative
//! components. This crate does not return an authorization-checked raw path.

mod capability;
mod digest;
mod error;
mod file;
mod identity;
mod lock;
mod path;
mod platform;
mod proof;
mod replace;
mod sync;

pub use capability::{
    CapabilityRights, DirectoryCapability, DirectoryEntryObservation, HostCapabilityOptions,
    TrustMode,
};
pub use digest::{ContentDigest, DigestLimit};
pub use error::{FsError, FsOperation, PathValidationError};
pub use file::{CapabilityReader, FileCapability};
pub use identity::{CapabilityGeneration, CapabilitySessionId, FileIdentity, FileKind};
pub use lock::{FileLock, LockGuarantee, LockMode, LockScope};
pub use path::{RelativeComponent, RelativePath, RelativePathPreflight};
pub use proof::{
    ConflictProtection, DirectorySyncTier, DurabilityProgress, FileSyncTier,
    ParentAuthorizationTier, PlatformCapabilityMatrix, ProofStatus, PublicationAtomicity,
    PublicationIdentityEvidence, ReplaceSourceBinding, ResolutionTier, StageStatus,
};
pub use replace::{ExpectedTarget, ReplaceReceipt, TemporaryFile};
pub use sync::{DirectorySyncReceipt, FileSyncReceipt};

pub(crate) use identity::{FileFacts, NativeFileIdentity};

/// Reports the guarantees implemented by the current platform adapter.
#[must_use]
pub fn platform_capability_matrix() -> PlatformCapabilityMatrix {
    platform::capability_matrix()
}
