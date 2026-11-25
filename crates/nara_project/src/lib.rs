//! Project manifests, source runtime projection, and native project workflows.

// Bevy derives resolve this path; Nara's ECS facade remains the actual dependency.
pub const CURRENT_PROJECT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MANIFEST_BYTE_LIMIT: u64 = 256 * 1024;
/// Maximum bytes retained for one profile execution contract identifier.
pub const MAX_PROJECT_EXECUTION_CONTRACT_BYTES: usize = 128;
/// Maximum explicit Scene roots in the project content declaration.
///
/// The selected startup Scene is implicit and does not consume this declaration limit. This is
/// a manifest admission bound, not a promise about supported scene size or runtime performance.
pub const MAX_PROJECT_CONTENT_SCENES: usize = 64;

mod authoring;
#[cfg(feature = "cargo-facts")]
pub mod cargo;
mod content;
mod defaults;
pub mod delivery;
mod effective;
#[cfg(feature = "cargo-build")]
pub mod host_fs;
mod manifest;
mod path;
#[cfg(all(
    feature = "workflow",
    any(target_os = "linux", target_os = "macos", windows)
))]
pub mod play;
#[cfg(feature = "product-process")]
pub mod process;
mod profile;
mod render;
mod runtime_configuration;
mod sections;
mod validation;
#[cfg(all(windows, any(feature = "product-process", feature = "workflow")))]
mod windows_command_line;
#[cfg(all(
    feature = "workflow",
    any(target_os = "linux", target_os = "macos", windows)
))]
pub mod workflow;

pub use authoring::{
    MAX_PROJECT_AUTHORING_RECOVERY_PATH_BYTES, ProjectAuthoringRecovery,
    ProjectAuthoringRecoveryDigest, ProjectAuthoringRecoveryDigestError,
    ProjectAuthoringRecoveryPathError, ProjectAuthoringState,
};
pub use content::{EffectiveProjectContentSettings, ProjectContentManifest};
pub use effective::{
    DESKTOP_SUPERVISED_PLAY_V1_CONTRACT, EffectiveInputSettings, EffectiveProjectExecutionSettings,
    EffectiveProjectInfo, EffectiveProjectPaths, EffectiveProjectSettings,
    EffectiveStartupSettings, ProjectExecutionContract,
};
pub use manifest::{
    ProjectManifest, ProjectManifestLoad, ProjectManifestMutationError, ProjectManifestWriteError,
};
pub use nara_core::{SourceUniverseId, SourceUniverseIdError};
pub use nara_scene::runtime_configuration::{
    EffectiveDiagnosticsSettings, EffectivePrimaryUiSurface, EffectiveRenderSettings,
    EffectiveRuntimeSettings, EffectiveTaskSettings, EffectiveUiSafeArea, EffectiveUiSettings,
    EffectiveWindowSettings, EffectiveWorldCanvas, MAX_PROJECT_FIXED_DEBT_STEPS,
    MAX_PROJECT_FIXED_STEPS_PER_FRAME, MAX_PROJECT_UI_LOGICAL_EXTENT, ProductCapability,
    ProductCapabilitySet, ProjectCapabilitiesManifest, ProjectDiagnosticsManifest,
    ProjectFixedCatchUpPolicy, ProjectPresentMode, ProjectRenderManifest, ProjectTaskPoolManifest,
    ProjectTaskShutdownManifest, ProjectTasksManifest, ProjectUiManifest,
    ProjectUiSafeAreaManifest, ProjectUiSurfaceManifest, ProjectWindowManifest, ProjectWindowMode,
    ProjectWorldCanvasManifest, RuntimeConfigurationError, RuntimePreset,
};
pub use path::{ProjectPath, ProjectPathError};
pub use profile::{
    ProjectCapabilitiesPatch, ProjectDiagnosticsPatch, ProjectExecutionPatch, ProjectInputPatch,
    ProjectPathsPatch, ProjectProfileError, ProjectProfileOverlay, ProjectRuntimePatch,
    ProjectStartupPatch, ProjectTaskPoolPatch, ProjectTaskShutdownPatch, ProjectTasksPatch,
    ProjectUiPatch, ProjectWindowPatch,
};
pub use render::ProjectRenderPatch;
pub use runtime_configuration::SourceRuntimeProjection;
pub use sections::{
    NativeDocumentIdentity, ProjectAuthoringManifest, ProjectInfo, ProjectInputManifest,
    ProjectPathsManifest, ProjectProfileKind, ProjectRuntimeManifest, ProjectStartupManifest,
};

pub mod prelude {
    pub use crate::{
        CURRENT_PROJECT_SCHEMA_VERSION, DEFAULT_MANIFEST_BYTE_LIMIT,
        DESKTOP_SUPERVISED_PLAY_V1_CONTRACT, EffectiveDiagnosticsSettings, EffectiveInputSettings,
        EffectivePrimaryUiSurface, EffectiveProjectContentSettings,
        EffectiveProjectExecutionSettings, EffectiveProjectInfo, EffectiveProjectPaths,
        EffectiveProjectSettings, EffectiveRenderSettings, EffectiveRuntimeSettings,
        EffectiveStartupSettings, EffectiveTaskSettings, EffectiveUiSafeArea, EffectiveUiSettings,
        EffectiveWindowSettings, EffectiveWorldCanvas, MAX_PROJECT_AUTHORING_RECOVERY_PATH_BYTES,
        MAX_PROJECT_CONTENT_SCENES, MAX_PROJECT_EXECUTION_CONTRACT_BYTES,
        MAX_PROJECT_FIXED_DEBT_STEPS, MAX_PROJECT_FIXED_STEPS_PER_FRAME,
        MAX_PROJECT_UI_LOGICAL_EXTENT, NativeDocumentIdentity, ProductCapability,
        ProductCapabilitySet, ProjectAuthoringManifest, ProjectAuthoringRecovery,
        ProjectAuthoringRecoveryDigest, ProjectAuthoringRecoveryDigestError,
        ProjectAuthoringRecoveryPathError, ProjectAuthoringState, ProjectCapabilitiesManifest,
        ProjectCapabilitiesPatch, ProjectContentManifest, ProjectDiagnosticsManifest,
        ProjectExecutionContract, ProjectExecutionPatch, ProjectFixedCatchUpPolicy, ProjectInfo,
        ProjectInputManifest, ProjectManifest, ProjectManifestLoad, ProjectManifestMutationError,
        ProjectManifestWriteError, ProjectPath, ProjectPathError, ProjectPathsManifest,
        ProjectPresentMode, ProjectProfileError, ProjectProfileKind, ProjectProfileOverlay,
        ProjectRenderManifest, ProjectRenderPatch, ProjectRuntimeManifest, ProjectStartupManifest,
        ProjectTaskPoolManifest, ProjectTaskPoolPatch, ProjectTaskShutdownManifest,
        ProjectTaskShutdownPatch, ProjectTasksManifest, ProjectTasksPatch, ProjectUiManifest,
        ProjectUiPatch, ProjectUiSafeAreaManifest, ProjectUiSurfaceManifest, ProjectWindowManifest,
        ProjectWindowMode, ProjectWorldCanvasManifest, RuntimeConfigurationError, RuntimePreset,
        SourceRuntimeProjection, SourceUniverseId, SourceUniverseIdError,
    };
}

#[cfg(test)]
mod tests;
