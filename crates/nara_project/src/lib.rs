//! Project manifest parsing, validation, and effective settings lowering.

pub const CURRENT_PROJECT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MANIFEST_BYTE_LIMIT: u64 = 256 * 1024;

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
    ProjectProfileOverlay, ProjectRuntimePatch, ProjectStartupPatch, ProjectTasksPatch,
    ProjectWindowPatch,
};
pub use sections::{
    ProjectDiagnosticsManifest, ProjectInfo, ProjectInputManifest, ProjectPathsManifest,
    ProjectPluginPlan, ProjectPresentMode, ProjectProfileKind, ProjectRuntimeManifest,
    ProjectStartupManifest, ProjectTaskExecutionMode, ProjectTasksManifest, ProjectWindowManifest,
    ProjectWindowMode,
};

pub mod prelude {
    pub use crate::{
        CURRENT_PROJECT_SCHEMA_VERSION, DEFAULT_MANIFEST_BYTE_LIMIT, EffectiveDiagnosticsSettings,
        EffectiveInputSettings, EffectiveProjectInfo, EffectiveProjectPaths,
        EffectiveProjectSettings, EffectiveRuntimeSettings, EffectiveStartupSettings,
        EffectiveTaskSettings, EffectiveWindowSettings, ProjectDiagnosticsManifest, ProjectInfo,
        ProjectInputManifest, ProjectManifest, ProjectManifestFileError, ProjectManifestLoad,
        ProjectPath, ProjectPathError, ProjectPathsManifest, ProjectPluginPlan, ProjectPresentMode,
        ProjectProfileError, ProjectProfileKind, ProjectProfileOverlay, ProjectRuntimeManifest,
        ProjectStartupManifest, ProjectTaskExecutionMode, ProjectTasksManifest,
        ProjectWindowManifest, ProjectWindowMode,
    };
}

#[cfg(test)]
mod tests;
