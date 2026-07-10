//! Project manifest parsing, validation, and effective settings lowering.

pub const CURRENT_PROJECT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MANIFEST_BYTE_LIMIT: u64 = 256 * 1024;
/// Maximum fixed schedules a file-backed project may request in one app frame.
pub const MAX_PROJECT_FIXED_STEPS_PER_FRAME: u32 = 256;
/// Maximum retained fixed debt, equal to 64 frames at the project step ceiling.
pub const MAX_PROJECT_FIXED_DEBT_STEPS: u32 = MAX_PROJECT_FIXED_STEPS_PER_FRAME * 64;

mod defaults;
mod effective;
mod manifest;
mod path;
mod profile;
mod sections;
mod validation;

pub use effective::{
    EffectiveDiagnosticsSettings, EffectiveInputSettings, EffectiveProjectInfo,
    EffectiveProjectPaths, EffectiveProjectSettings, EffectiveRuntimeSettings,
    EffectiveStartupSettings, EffectiveTaskSettings, EffectiveWindowSettings,
};
pub use manifest::{ProjectManifest, ProjectManifestFileError, ProjectManifestLoad};
pub use path::{ProjectPath, ProjectPathError};
pub use profile::{
    ProjectDiagnosticsPatch, ProjectInputPatch, ProjectPathsPatch, ProjectProfileError,
    ProjectProfileOverlay, ProjectRuntimePatch, ProjectStartupPatch, ProjectTaskPoolPatch,
    ProjectTaskShutdownPatch, ProjectTasksPatch, ProjectWindowPatch,
};
pub use sections::{
    ProjectDiagnosticsManifest, ProjectFixedCatchUpPolicy, ProjectInfo, ProjectInputManifest,
    ProjectPathsManifest, ProjectPluginPlan, ProjectPresentMode, ProjectProfileKind,
    ProjectRuntimeManifest, ProjectStartupManifest, ProjectTaskPoolManifest,
    ProjectTaskShutdownManifest, ProjectTasksManifest, ProjectWindowManifest, ProjectWindowMode,
};

pub mod prelude {
    pub use crate::{
        CURRENT_PROJECT_SCHEMA_VERSION, DEFAULT_MANIFEST_BYTE_LIMIT, EffectiveDiagnosticsSettings,
        EffectiveInputSettings, EffectiveProjectInfo, EffectiveProjectPaths,
        EffectiveProjectSettings, EffectiveRuntimeSettings, EffectiveStartupSettings,
        EffectiveTaskSettings, EffectiveWindowSettings, MAX_PROJECT_FIXED_DEBT_STEPS,
        MAX_PROJECT_FIXED_STEPS_PER_FRAME, ProjectDiagnosticsManifest, ProjectFixedCatchUpPolicy,
        ProjectInfo, ProjectInputManifest, ProjectManifest, ProjectManifestFileError,
        ProjectManifestLoad, ProjectPath, ProjectPathError, ProjectPathsManifest,
        ProjectPluginPlan, ProjectPresentMode, ProjectProfileError, ProjectProfileKind,
        ProjectProfileOverlay, ProjectRuntimeManifest, ProjectStartupManifest,
        ProjectTaskPoolManifest, ProjectTaskPoolPatch, ProjectTaskShutdownManifest,
        ProjectTaskShutdownPatch, ProjectTasksManifest, ProjectTasksPatch, ProjectWindowManifest,
        ProjectWindowMode,
    };
}

#[cfg(test)]
mod tests;
