mod composition;
mod ingest;
mod product_launch;

#[cfg(windows)]
#[doc(hidden)]
pub mod process_image;

#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    any(feature = "tooling", feature = "project-tooling")
))]
mod file_publication;

#[cfg(feature = "project-tooling")]
pub(crate) mod schema;

#[cfg(feature = "serde")]
#[allow(
    dead_code,
    reason = "the root-private schema receipt is consumed by the generic authoring decoder"
)]
mod authoring;

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(crate) mod check;

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(crate) mod runtime_agreement;

pub mod advanced;

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
mod runtime;

#[cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]
mod persistence;

#[cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]
pub use persistence::EditorPersistenceReceipt;
#[cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]
pub use runtime::{
    EditorExtensionDocumentHostError, EditorSourceAssetDomain, EditorSourceAssetImporters,
};

#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub use crate::project_content::{
    ProjectAssetCollection, ProjectAssetDomain, ProjectAssetLimits, ProjectAssetSource,
    ProjectContentBudgetError, ProjectContentBudgetHost, ProjectContentBudgetKind,
    ProjectContentBudgetSnapshot, ProjectContentClosureReceipt, ProjectContentDeliveryReceipt,
    ProjectContentError, ProjectContentErrorKind, ProjectContentLimits, ProjectContentLoader,
    ProjectContentMemberKind, ProjectContentMemberReceipt, ProjectContentRevision,
    ProjectContentSnapshot, ProjectDomainAssets, ProjectImageContent, ProjectPrefabContent,
    ProjectTileSetContent, StartupContentClosureReceipt,
};
pub use composition::{
    CompiledProductCapabilities, CompositionError, ProjectSettingsCandidate,
    ProjectSettingsLineage, RuntimePlan, RuntimePlanError, SchemaValidationInput,
    compiled_product_capabilities, resolve_product_recipe,
};
pub use ingest::{ProjectCandidateError, ProjectCandidateErrorKind, ingest_project_manifest};
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub use product_launch::HeadlessProductRun;
#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
pub use product_launch::{DesktopProductLaunch, DesktopProductLaunchError};
#[cfg(feature = "runtime-core")]
#[doc(hidden)]
pub use product_launch::{
    MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES, MAX_SUPERVISED_DESKTOP_LAUNCH_CONTEXT_BYTES,
    MAX_SUPERVISED_DESKTOP_READY_ATTESTATION_BYTES, ProductSupervisedDesktopBootstrapError,
    ProductSupervisedDesktopLaunchCandidate, ProductSupervisedDesktopReadyAdvance,
    ProductSupervisedDesktopReadyPublication, ProductSupervisedDesktopSession,
    SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION, SUPERVISED_DESKTOP_READY_ATTESTATION_VERSION,
    SUPERVISED_DESKTOP_READY_WRITE_TIMEOUT, SupervisedDesktopGenerationId,
    SupervisedDesktopLaunchContext, SupervisedDesktopProductIdentity,
    SupervisedDesktopReadyAttestation, SupervisedDesktopRuntimeClosureFacts,
    SupervisedDesktopSessionNonce, SupervisedDesktopWireError,
    supervised_desktop_ready_frame_length,
};
#[doc(hidden)]
pub use product_launch::{
    ProductProjectRootError, ProductProjectRootSource, resolve_product_project_root,
};
pub use product_launch::{open_explicit_product_project_root, open_product_project_root};
#[cfg(feature = "runtime-ui")]
#[doc(hidden)]
pub const PROJECT_PRIMARY_UI_SURFACE_ID: u64 = crate::product::PROJECT_PRIMARY_UI_SURFACE_ID;
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(crate) use ingest::{
    ingest_project_manifest_bytes, ingest_project_manifest_with_expected_digest,
};
#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "tooling",
    feature = "presentation-ui"
))]
#[doc(hidden)]
pub use runtime::EditorUiPreviewCandidate;
#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
pub use runtime::{
    DesktopPresentationVerificationReceipt, DesktopRun, DesktopRunIntent, DesktopRunOutcome,
    DesktopRunReport,
};
#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "tooling",
    feature = "presentation-2d"
))]
pub use runtime::{
    EditorAcceptedPreview, EditorPresentationError, EditorPresentationImage, EditorPresentedToken,
    EditorPreviewPresentation,
};
#[cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]
#[doc(hidden)]
pub use runtime::{
    EditorEditPreparationError, EditorProjectCloseProgress, EditorProjectCloseReceipt,
    EditorProjectOpenAttempt, EditorProjectOpenError, EditorProjectOpenFailure,
    EditorProjectOpenFailureImpact, EditorProjectOpenFailureSource, EditorProjectOpenHost,
    EditorProjectOpenPhase, EditorProjectOpenProgress, EditorProjectOpenTasksCloseReport,
    EditorProjectSession, EditorSourceToolCommand, EditorSourceToolDefinition,
    EditorSourceToolInstallation, EditorSourceToolInstallationError,
    EditorSourceToolInstallationInspection, EditorSourceToolInstallationOrphan,
    EditorSourceToolInstallationOutcome, EditorSourceToolInstallationPlan,
    EditorSourceToolInstallationReceipt, EditorSourceToolInstalledMember,
    EditorSourceToolManagedError, EditorSourceToolManagedHost, EditorSourceToolManagedHostDigests,
    EditorSourceToolReleaseError, EditorSourceToolReleaseMember, EditorSourceToolReleaseMemberKind,
    EditorSourceToolReleaseRequirement, PreparedEditorSourceToolRelease,
    PreparedEditorSourceToolSelection, SupervisedDesktopSourceAdmissionError,
    SupervisedDesktopSourceAuthority, TrustedCompiledEditorProjectIntent,
    admit_supervised_desktop_source_receipt,
};
#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "tooling",
    any(feature = "presentation-2d", feature = "presentation-ui")
))]
pub use runtime::{
    EditorGameInputBatch, EditorGameInputEnvelope, EditorGameInputEvent, EditorGameStartActivation,
    EditorGameStartRequest, EditorGameViewBridgeFailure, EditorGameViewCandidate,
    EditorGameViewCandidateIdentity, EditorGameViewCandidateParts, EditorGameViewCompletion,
    EditorGameViewCompletionDisposition,
};
#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "tooling",
    feature = "presentation-2d"
))]
pub use runtime::{EditorPreviewCandidate, EditorPreviewContentStamp, EditorPreviewHit};
#[cfg(all(
    feature = "runtime-2d",
    feature = "serde",
    feature = "tooling",
    feature = "runtime-ui"
))]
#[doc(hidden)]
pub use runtime::{EditorUiAuthoringEditError, EditorUiAuthoringFrame};
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub use runtime::{HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport};
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub(crate) use runtime::{
    PROJECT_SCENE_REQUEST_CLOSE_OBLIGATION, poll_project_scene_requests,
    register_project_scene_close_participant,
};
#[cfg(all(feature = "runtime-2d", feature = "serde"))]
pub use runtime::{
    PreparedProjectSceneView, ProjectSceneRequestError, ProjectSceneRequestId,
    ProjectSceneRequestState, ProjectScenes,
};
