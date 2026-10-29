//! Project manifest parsing, validation, and effective settings lowering.

// Bevy derives resolve this path; Nara's ECS facade remains the actual dependency.
pub const CURRENT_PROJECT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MANIFEST_BYTE_LIMIT: u64 = 256 * 1024;
/// Maximum fixed schedules a file-backed project may request in one app frame.
pub const MAX_PROJECT_FIXED_STEPS_PER_FRAME: u32 = 256;
/// Maximum retained fixed debt, equal to 64 frames at the project step ceiling.
pub const MAX_PROJECT_FIXED_DEBT_STEPS: u32 = MAX_PROJECT_FIXED_STEPS_PER_FRAME * 64;
/// Maximum authored logical width or height for the primary UI surface.
pub const MAX_PROJECT_UI_LOGICAL_EXTENT: u32 = 65_536;
/// Maximum bytes retained for one profile execution contract identifier.
pub const MAX_PROJECT_EXECUTION_CONTRACT_BYTES: usize = 128;
/// Maximum explicit Scene roots in the project content declaration.
///
/// The selected startup Scene is implicit and does not consume this declaration limit. This is
/// a manifest admission bound, not a promise about supported scene size or runtime performance.
pub const MAX_PROJECT_CONTENT_SCENES: usize = 64;

mod capability;
mod content;
mod defaults;
mod effective;
mod manifest;
mod path;
mod profile;
mod render;
mod runtime_configuration;
mod sections;
mod validation;

pub use capability::{ProductCapability, ProductCapabilitySet, RuntimePreset};
pub use content::{EffectiveProjectContentSettings, ProjectContentManifest};
pub use effective::{
    DESKTOP_SUPERVISED_PLAY_V1_CONTRACT, EffectiveDiagnosticsSettings, EffectiveInputSettings,
    EffectivePrimaryUiSurface, EffectiveProjectExecutionSettings, EffectiveProjectInfo,
    EffectiveProjectPaths, EffectiveProjectSettings, EffectiveRuntimeSettings,
    EffectiveStartupSettings, EffectiveTaskSettings, EffectiveUiSafeArea, EffectiveUiSettings,
    EffectiveWindowSettings, ProjectExecutionContract,
};
pub use manifest::{
    ProjectManifest, ProjectManifestLoad, ProjectManifestMutationError, ProjectManifestWriteError,
};
pub use nara_core::{SourceUniverseId, SourceUniverseIdError};
pub use path::{ProjectPath, ProjectPathError};
pub use profile::{
    ProjectCapabilitiesPatch, ProjectDiagnosticsPatch, ProjectExecutionPatch, ProjectInputPatch,
    ProjectPathsPatch, ProjectProfileError, ProjectProfileOverlay, ProjectRuntimePatch,
    ProjectStartupPatch, ProjectTaskPoolPatch, ProjectTaskShutdownPatch, ProjectTasksPatch,
    ProjectUiPatch, ProjectWindowPatch,
};
pub use render::{
    EffectiveRenderSettings, EffectiveWorldCanvas, ProjectRenderManifest, ProjectRenderPatch,
    ProjectWorldCanvasManifest,
};
pub use runtime_configuration::{
    EffectiveRuntimeConfiguration, RuntimeConfiguration, RuntimeConfigurationError,
    RuntimeTimeConfiguration,
};
pub use sections::{
    NativeDocumentIdentity, ProjectAuthoringManifest, ProjectCapabilitiesManifest,
    ProjectDiagnosticsManifest, ProjectFixedCatchUpPolicy, ProjectInfo, ProjectInputManifest,
    ProjectPathsManifest, ProjectPresentMode, ProjectProfileKind, ProjectRuntimeManifest,
    ProjectStartupManifest, ProjectTaskPoolManifest, ProjectTaskShutdownManifest,
    ProjectTasksManifest, ProjectUiManifest, ProjectUiSafeAreaManifest, ProjectUiSurfaceManifest,
    ProjectWindowManifest, ProjectWindowMode,
};

pub mod prelude {
    pub use crate::{
        CURRENT_PROJECT_SCHEMA_VERSION, DEFAULT_MANIFEST_BYTE_LIMIT,
        DESKTOP_SUPERVISED_PLAY_V1_CONTRACT, EffectiveDiagnosticsSettings, EffectiveInputSettings,
        EffectivePrimaryUiSurface, EffectiveProjectContentSettings,
        EffectiveProjectExecutionSettings, EffectiveProjectInfo, EffectiveProjectPaths,
        EffectiveProjectSettings, EffectiveRenderSettings, EffectiveRuntimeConfiguration,
        EffectiveRuntimeSettings, EffectiveStartupSettings, EffectiveTaskSettings,
        EffectiveUiSafeArea, EffectiveUiSettings, EffectiveWindowSettings, EffectiveWorldCanvas,
        MAX_PROJECT_CONTENT_SCENES, MAX_PROJECT_EXECUTION_CONTRACT_BYTES,
        MAX_PROJECT_FIXED_DEBT_STEPS, MAX_PROJECT_FIXED_STEPS_PER_FRAME,
        MAX_PROJECT_UI_LOGICAL_EXTENT, NativeDocumentIdentity, ProductCapability,
        ProductCapabilitySet, ProjectAuthoringManifest, ProjectCapabilitiesManifest,
        ProjectCapabilitiesPatch, ProjectContentManifest, ProjectDiagnosticsManifest,
        ProjectExecutionContract, ProjectExecutionPatch, ProjectFixedCatchUpPolicy, ProjectInfo,
        ProjectInputManifest, ProjectManifest, ProjectManifestLoad, ProjectManifestMutationError,
        ProjectManifestWriteError, ProjectPath, ProjectPathError, ProjectPathsManifest,
        ProjectPresentMode, ProjectProfileError, ProjectProfileKind, ProjectProfileOverlay,
        ProjectRenderManifest, ProjectRenderPatch, ProjectRuntimeManifest, ProjectStartupManifest,
        ProjectTaskPoolManifest, ProjectTaskPoolPatch, ProjectTaskShutdownManifest,
        ProjectTaskShutdownPatch, ProjectTasksManifest, ProjectTasksPatch, ProjectUiManifest,
        ProjectUiPatch, ProjectUiSafeAreaManifest, ProjectUiSurfaceManifest, ProjectWindowManifest,
        ProjectWindowMode, ProjectWorldCanvasManifest, RuntimeConfiguration,
        RuntimeConfigurationError, RuntimePreset, RuntimeTimeConfiguration, SourceUniverseId,
        SourceUniverseIdError,
    };
}

#[cfg(test)]
mod tests;
