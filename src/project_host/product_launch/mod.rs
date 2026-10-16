#[cfg(feature = "runtime-core")]
mod child;
#[cfg(feature = "runtime-core")]
mod platform;
mod root;
#[cfg(feature = "runtime-core")]
mod source;
#[cfg(feature = "runtime-core")]
mod wire;

#[cfg(feature = "runtime-core")]
#[doc(hidden)]
pub use child::{
    ProductSupervisedDesktopBootstrapError, ProductSupervisedDesktopLaunchCandidate,
    ProductSupervisedDesktopReadyAdvance, ProductSupervisedDesktopReadyPublication,
    ProductSupervisedDesktopSession, SUPERVISED_DESKTOP_READY_WRITE_TIMEOUT,
};
#[doc(hidden)]
pub use root::{ProductProjectRootError, ProductProjectRootSource, resolve_product_project_root};
#[cfg(feature = "runtime-core")]
#[allow(unused_imports)]
pub(crate) use source::{
    EditorExternalPlaySourceFact, EditorExternalPlaySourceKind, EditorExternalPlaySourceReceipt,
    EditorExternalPlayTarget,
};
#[cfg(feature = "runtime-core")]
#[doc(hidden)]
pub use wire::{
    MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES, MAX_SUPERVISED_DESKTOP_LAUNCH_CONTEXT_BYTES,
    MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES, SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION,
    SUPERVISED_DESKTOP_READY_ATTESTATION_VERSION, SupervisedDesktopGenerationId,
    SupervisedDesktopLaunchContext, SupervisedDesktopProductIdentity,
    SupervisedDesktopReadyAttestation, SupervisedDesktopRuntimeClosureFacts,
    SupervisedDesktopSessionNonce, SupervisedDesktopWireError,
    supervised_desktop_ready_frame_length,
};
