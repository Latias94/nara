mod composition;
#[cfg(feature = "project-source")]
mod ingest;
mod product_launch;
#[cfg(any(
    feature = "serde",
    all(feature = "desktop-winit", feature = "render-wgpu")
))]
mod runtime_diagnostics;

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
pub(crate) mod desktop_run;
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
pub use desktop_run::{
    DesktopControlError, DesktopPresentationControl, DesktopPresentationObservation, DesktopRun,
    DesktopRunOptions, DesktopRunOutcome, DesktopRunProgress, DesktopRunReport,
    DesktopStartupPhase, DesktopStartupTimings, DesktopTelemetrySealOutcome,
    DesktopTelemetrySealReport,
};
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
pub use nara_window::winit::{
    WinitEventLoop, WinitInputActivation, WinitKeyboardContinuationError,
};

#[cfg(windows)]
#[doc(hidden)]
pub mod process_image;

#[cfg(feature = "project-tooling")]
mod file_publication;

#[cfg(feature = "project-tooling")]
pub(crate) mod schema;

#[cfg(feature = "project-source")]
pub(crate) mod check;

#[cfg(feature = "project-source")]
pub(crate) mod runtime_agreement;

pub mod advanced;

#[cfg(feature = "serde")]
mod runtime;

#[cfg(feature = "serde")]
pub(crate) use runtime::has_active_project_scene_addition;

#[cfg(feature = "serde")]
pub use crate::project_assets::{ProjectAssetCollection, ProjectDomainAssets};
#[cfg(feature = "project-source")]
pub use crate::project_content::{
    ProjectContentBudgetError, ProjectContentBudgetHost, ProjectContentBudgetKind,
    ProjectContentBudgetSnapshot, ProjectContentClosureReceipt, ProjectContentDeliveryReceipt,
    ProjectContentError, ProjectContentErrorKind, ProjectContentLimits, ProjectContentLoader,
    ProjectContentMemberKind, ProjectContentMemberReceipt, ProjectContentRevision,
    ProjectContentSnapshot, ProjectImageContent, ProjectInputContent, ProjectPrefabContent,
    ProjectSceneContent, ProjectSceneContentStamp, ProjectTileSetContent,
    StartupContentClosureReceipt,
};
pub use composition::{
    CompiledProductCapabilities, CompositionError, RuntimeCapabilityError,
    RuntimeCapabilityErrorKind, RuntimeConfigurationPlan, RuntimeConfigurationPlanError,
    RuntimePlanError, SchemaValidationInput, compiled_product_capabilities,
    resolve_runtime_configuration,
};
#[cfg(feature = "project-source")]
pub use composition::{
    ProjectSettingsCandidate, ProjectSettingsLineage, RuntimePlan, resolve_product_recipe,
};
#[cfg(feature = "project-tooling")]
pub(crate) use ingest::read_manifest_profile;
#[cfg(feature = "project-source")]
pub use ingest::{ProjectCandidateError, ProjectCandidateErrorKind, ingest_project_manifest};
#[cfg(all(
    feature = "project-source",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
pub use product_launch::DesktopProductLaunch;
#[cfg(feature = "project-source")]
pub use product_launch::HeadlessProductRun;
#[doc(hidden)]
pub use product_launch::{open_explicit_product_project_root, open_product_project_root};
#[cfg(feature = "serde")]
mod runtime_pack;
#[cfg(feature = "serde")]
pub use runtime_pack::open_executable_relative_runtime_pack;
#[cfg(feature = "runtime-ui")]
#[doc(hidden)]
pub const PROJECT_PRIMARY_UI_SURFACE_ID: u64 = crate::product::PROJECT_PRIMARY_UI_SURFACE_ID;
#[cfg(all(
    feature = "project-source",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
pub use desktop_run::DesktopPresentationVerificationReceipt;
#[cfg(feature = "project-source")]
pub(crate) use ingest::{
    ingest_project_manifest_bytes, ingest_project_manifest_with_expected_digest,
};
#[cfg(all(
    feature = "project-source",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
pub use runtime::DesktopRunIntent;
#[cfg(feature = "serde")]
pub use runtime::{
    HeadlessRun, HeadlessRunOptions, HeadlessRunOutcome, HeadlessRunReport, HeadlessSession,
    HeadlessSessionClose, HeadlessSessionError,
};
#[cfg(feature = "project-source")]
pub use runtime::{
    HeadlessRunIntent, ProjectAppLoadError, ProjectSceneSourceUpdateStatus, load_project_app,
};
#[cfg(feature = "serde")]
pub(crate) use runtime::{
    PROJECT_SCENE_REQUEST_CLOSE_OBLIGATION, poll_project_scene_requests,
    register_project_scene_close_participant,
};
#[cfg(all(feature = "project-source", feature = "asset-watch"))]
pub use runtime::{
    PROJECT_SCENE_WATCH_PLUGIN_ID, ProjectSceneWatchConfigurationError, ProjectSceneWatchPlugin,
    ProjectSceneWatchState, ProjectSceneWatchStatus,
};
#[cfg(feature = "serde")]
pub use runtime::{
    PreparedProjectSceneView, ProjectSceneRequestError, ProjectSceneRequestId,
    ProjectSceneRequestState, ProjectSceneSelection, ProjectScenes,
    activate_prepared_project_scene, inspect_current_scene_dependency,
    instantiate_current_scene_dependency, unload_project_scene_addition,
};
