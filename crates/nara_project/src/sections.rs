use nara_core::SourceUniverseId;
use nara_core::diagnostic::DiagnosticReport;
use nara_scene::runtime_configuration::RuntimeTimeConfiguration;
use nara_scene::runtime_configuration::validation::{
    validate_fixed_step_limits, validate_time_scale,
};
use serde::{Deserialize, Serialize};

use crate::authoring::ProjectAuthoringState;
use crate::defaults::*;
use crate::path::ProjectPath;
use crate::profile::ProjectProfileError;
use crate::validation::{
    duration_from_positive_seconds, validate_duration_seconds, validate_path_field,
};
use crate::{
    EffectiveInputSettings, EffectiveRuntimeSettings, ProjectFixedCatchUpPolicy, RuntimePreset,
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfo {
    pub name: String,
    pub source_universe: SourceUniverseId,
    pub native_document_identity: NativeDocumentIdentity,
    /// Optional descriptive project coordinate, not source equality authority.
    #[serde(default)]
    pub stable_id: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

/// Project-wide source identity policy for native Scene and Prefab admission.
///
/// This is not a lookup fallback: missing stable metadata cannot select location identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeDocumentIdentity {
    ProjectLocation,
    StableMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPathsManifest {
    #[serde(default = "default_assets_path")]
    pub assets: String,
    #[serde(default = "default_scenes_path")]
    pub scenes: String,
    #[serde(default = "default_prefabs_path")]
    pub prefabs: String,
    #[serde(default = "default_scripts_path")]
    pub scripts: String,
    #[serde(default = "default_import_cache_path")]
    pub import_cache: String,
}

impl Default for ProjectPathsManifest {
    fn default() -> Self {
        Self {
            assets: default_assets_path(),
            scenes: default_scenes_path(),
            prefabs: default_prefabs_path(),
            scripts: default_scripts_path(),
            import_cache: default_import_cache_path(),
        }
    }
}

/// Project-wide source declaration for non-executable authoring data.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectAuthoringManifest {
    /// Required source policy. Parsing or resolving this value is not execution admission.
    pub state: ProjectAuthoringState,
    #[serde(default)]
    pub schema_bundle: Option<String>,
}

impl Default for ProjectAuthoringManifest {
    /// Selects Ready for trusted programmatic producers; deserialization requires explicit state.
    fn default() -> Self {
        Self {
            state: ProjectAuthoringState::Ready,
            schema_bundle: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ProjectStartupManifest {
    #[serde(default)]
    pub default_scene: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRuntimeManifest {
    #[serde(default)]
    pub paused: bool,
    #[serde(default = "default_time_scale")]
    pub time_scale: f32,
    #[serde(default = "default_max_delta_seconds")]
    pub max_delta_seconds: f64,
    #[serde(default = "default_fixed_timestep_seconds")]
    pub fixed_timestep_seconds: f64,
    #[serde(default = "default_max_fixed_steps_per_frame")]
    pub max_fixed_steps_per_frame: u32,
    #[serde(default = "default_max_fixed_debt_steps")]
    pub max_fixed_debt_steps: u32,
    #[serde(default)]
    pub catch_up_policy: ProjectFixedCatchUpPolicy,
    #[serde(default)]
    pub preset: RuntimePreset,
}

impl Default for ProjectRuntimeManifest {
    fn default() -> Self {
        Self {
            paused: false,
            time_scale: default_time_scale(),
            max_delta_seconds: default_max_delta_seconds(),
            fixed_timestep_seconds: default_fixed_timestep_seconds(),
            max_fixed_steps_per_frame: default_max_fixed_steps_per_frame(),
            max_fixed_debt_steps: default_max_fixed_debt_steps(),
            catch_up_policy: ProjectFixedCatchUpPolicy::default(),
            preset: RuntimePreset::default(),
        }
    }
}

impl ProjectRuntimeManifest {
    pub(crate) fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        validate_time_scale(
            diagnostics,
            &format!("{prefix}.time_scale"),
            self.time_scale,
        );
        validate_duration_seconds(
            diagnostics,
            &format!("{prefix}.max_delta_seconds"),
            self.max_delta_seconds,
        );
        validate_duration_seconds(
            diagnostics,
            &format!("{prefix}.fixed_timestep_seconds"),
            self.fixed_timestep_seconds,
        );
        validate_fixed_step_limits(
            diagnostics,
            prefix,
            Some(self.max_fixed_steps_per_frame),
            Some(self.max_fixed_debt_steps),
        );
    }

    pub(crate) fn lower(self) -> Result<EffectiveRuntimeSettings, ProjectProfileError> {
        let max_delta = duration_from_positive_seconds(self.max_delta_seconds).ok_or(
            ProjectProfileError::InvalidRuntimeDuration {
                field: "runtime.max_delta_seconds",
            },
        )?;
        let fixed_timestep = duration_from_positive_seconds(self.fixed_timestep_seconds).ok_or(
            ProjectProfileError::InvalidRuntimeDuration {
                field: "runtime.fixed_timestep_seconds",
            },
        )?;
        Ok(RuntimeTimeConfiguration {
            preset: self.preset,
            paused: self.paused,
            time_scale: self.time_scale,
            max_delta,
            fixed_timestep,
            max_fixed_steps_per_frame: self.max_fixed_steps_per_frame,
            max_fixed_debt_steps: self.max_fixed_debt_steps,
            catch_up_policy: self.catch_up_policy,
        }
        .try_into()?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ProjectInputManifest {
    #[serde(default)]
    pub action_map: Option<String>,
}

impl ProjectInputManifest {
    pub(crate) fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if let Some(action_map) = &self.action_map {
            validate_path_field(diagnostics, &format!("{prefix}.action_map"), action_map);
        }
    }

    pub(crate) fn lower(self) -> Result<EffectiveInputSettings, ProjectProfileError> {
        Ok(EffectiveInputSettings {
            action_map: self
                .action_map
                .map(ProjectPath::new)
                .transpose()
                .map_err(ProjectProfileError::InvalidProjectPath)?,
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ProjectProfileKind {
    Headless,
    Server,
    Editor,
    Dev,
    Release,
    #[default]
    Custom,
}

impl ProjectProfileKind {
    #[must_use]
    pub fn from_profile_name(name: &str) -> Self {
        match name {
            "headless" => Self::Headless,
            "server" => Self::Server,
            "editor" => Self::Editor,
            "dev" => Self::Dev,
            "release" => Self::Release,
            _ => Self::Custom,
        }
    }
}
