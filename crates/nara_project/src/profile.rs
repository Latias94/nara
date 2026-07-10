use std::fmt;

use nara_app::TimeSettingsError;
use nara_diagnostic::{DiagnosticReport, DiagnosticSettingsError, MAX_RUNTIME_DIAGNOSTIC_ENTRIES};
use nara_tasks::TaskConfigError;
use serde::Deserialize;
use thiserror::Error;

use crate::effective::{
    EffectiveDiagnosticsSettings, EffectiveInputSettings, EffectiveProjectPaths,
    EffectiveProjectSettings, EffectiveRuntimeSettings, EffectiveStartupSettings,
    EffectiveTaskSettings, EffectiveWindowSettings,
};
use crate::path::{ProjectPath, ProjectPathError};
use crate::sections::{
    ProjectFixedCatchUpPolicy, ProjectPluginPlan, ProjectPresentMode, ProjectTaskPoolManifest,
    ProjectTaskShutdownManifest, ProjectTasksManifest, ProjectWindowMode,
    runtime_diagnostics_settings,
};
use crate::validation::{
    duration_from_positive_seconds, error, validate_duration_seconds, validate_fixed_step_limits,
    validate_optional_path_field, validate_path_field, with_field_path, with_public_bool,
    with_public_u64,
};

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ProjectProfileOverlay {
    #[serde(default)]
    pub paths: ProjectPathsPatch,
    #[serde(default)]
    pub startup: ProjectStartupPatch,
    #[serde(default)]
    pub runtime: ProjectRuntimePatch,
    #[serde(default)]
    pub tasks: ProjectTasksPatch,
    #[serde(default)]
    pub window: ProjectWindowPatch,
    #[serde(default)]
    pub input: ProjectInputPatch,
    #[serde(default)]
    pub diagnostics: ProjectDiagnosticsPatch,
}

impl ProjectProfileOverlay {
    pub(crate) fn validate_into(
        &self,
        diagnostics: &mut DiagnosticReport,
        prefix: &str,
        base_tasks: &ProjectTasksManifest,
    ) {
        self.paths
            .validate_into(diagnostics, &format!("{prefix}.paths"));
        self.startup
            .validate_into(diagnostics, &format!("{prefix}.startup"));
        self.runtime
            .validate_into(diagnostics, &format!("{prefix}.runtime"));
        self.tasks
            .validate_into(diagnostics, &format!("{prefix}.tasks"), base_tasks);
        self.window
            .validate_into(diagnostics, &format!("{prefix}.window"));
        self.input
            .validate_into(diagnostics, &format!("{prefix}.input"));
        self.diagnostics
            .validate_into(diagnostics, &format!("{prefix}.diagnostics"));
    }

    pub(crate) fn apply_to(
        &self,
        settings: &mut EffectiveProjectSettings,
    ) -> Result<(), ProjectProfileError> {
        self.paths.apply_to(&mut settings.paths)?;
        self.startup.apply_to(&mut settings.startup)?;
        self.runtime.apply_to(&mut settings.runtime)?;
        if let Some(plugin_plan) = self.runtime.plugin_plan {
            settings.plugin_plan = plugin_plan;
        }
        self.tasks.apply_to(&mut settings.tasks)?;
        self.window.apply_to(&mut settings.window);
        self.input.apply_to(&mut settings.input)?;
        self.diagnostics.apply_to(&mut settings.diagnostics)?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPathsPatch {
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub scenes: Option<String>,
    #[serde(default)]
    pub prefabs: Option<String>,
    #[serde(default)]
    pub scripts: Option<String>,
    #[serde(default)]
    pub import_cache: Option<String>,
}

impl ProjectPathsPatch {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        validate_optional_path_field(
            diagnostics,
            &format!("{prefix}.assets"),
            self.assets.as_ref(),
        );
        validate_optional_path_field(
            diagnostics,
            &format!("{prefix}.scenes"),
            self.scenes.as_ref(),
        );
        validate_optional_path_field(
            diagnostics,
            &format!("{prefix}.prefabs"),
            self.prefabs.as_ref(),
        );
        validate_optional_path_field(
            diagnostics,
            &format!("{prefix}.scripts"),
            self.scripts.as_ref(),
        );
        validate_optional_path_field(
            diagnostics,
            &format!("{prefix}.import_cache"),
            self.import_cache.as_ref(),
        );
    }

    fn apply_to(&self, paths: &mut EffectiveProjectPaths) -> Result<(), ProjectProfileError> {
        if let Some(path) = &self.assets {
            paths.assets =
                ProjectPath::new(path.clone()).map_err(ProjectProfileError::InvalidProjectPath)?;
        }
        if let Some(path) = &self.scenes {
            paths.scenes =
                ProjectPath::new(path.clone()).map_err(ProjectProfileError::InvalidProjectPath)?;
        }
        if let Some(path) = &self.prefabs {
            paths.prefabs =
                ProjectPath::new(path.clone()).map_err(ProjectProfileError::InvalidProjectPath)?;
        }
        if let Some(path) = &self.scripts {
            paths.scripts =
                ProjectPath::new(path.clone()).map_err(ProjectProfileError::InvalidProjectPath)?;
        }
        if let Some(path) = &self.import_cache {
            paths.import_cache =
                ProjectPath::new(path.clone()).map_err(ProjectProfileError::InvalidProjectPath)?;
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectStartupPatch {
    #[serde(default)]
    pub default_scene: Option<String>,
    #[serde(default)]
    pub clear_default_scene: bool,
}

impl ProjectStartupPatch {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if let Some(scene) = &self.default_scene {
            validate_path_field(diagnostics, &format!("{prefix}.default_scene"), scene);
        }
        if self.clear_default_scene && self.default_scene.is_some() {
            diagnostics.push(with_field_path(
                error(
                    "project.startup.conflicting-default-scene-patch",
                    "Startup patch cannot both clear and set the default scene",
                ),
                &format!("{prefix}.default_scene"),
            ));
        }
    }

    fn apply_to(&self, startup: &mut EffectiveStartupSettings) -> Result<(), ProjectProfileError> {
        if self.clear_default_scene {
            startup.default_scene = None;
        }
        if let Some(default_scene) = &self.default_scene {
            startup.default_scene = Some(
                ProjectPath::new(default_scene.clone())
                    .map_err(ProjectProfileError::InvalidProjectPath)?,
            );
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRuntimePatch {
    #[serde(default)]
    pub paused: Option<bool>,
    #[serde(default)]
    pub time_scale: Option<f32>,
    #[serde(default)]
    pub max_delta_seconds: Option<f64>,
    #[serde(default)]
    pub fixed_timestep_seconds: Option<f64>,
    #[serde(default)]
    pub max_fixed_steps_per_frame: Option<u32>,
    #[serde(default)]
    pub max_fixed_debt_steps: Option<u32>,
    #[serde(default)]
    pub catch_up_policy: Option<ProjectFixedCatchUpPolicy>,
    #[serde(default)]
    pub plugin_plan: Option<ProjectPluginPlan>,
}

impl ProjectRuntimePatch {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if let Some(time_scale) = self.time_scale
            && (!time_scale.is_finite() || time_scale < 0.0)
        {
            let diagnostic = error(
                "project.runtime.invalid-time-scale",
                "Runtime time scale must be finite and non-negative",
            );
            let diagnostic = with_field_path(diagnostic, &format!("{prefix}.time_scale"));
            let diagnostic = with_public_bool(diagnostic, "finite", time_scale.is_finite());
            diagnostics.push(with_public_bool(
                diagnostic,
                "non_negative",
                time_scale >= 0.0,
            ));
        }
        if let Some(seconds) = self.max_delta_seconds {
            validate_duration_seconds(diagnostics, &format!("{prefix}.max_delta_seconds"), seconds);
        }
        if let Some(seconds) = self.fixed_timestep_seconds {
            validate_duration_seconds(
                diagnostics,
                &format!("{prefix}.fixed_timestep_seconds"),
                seconds,
            );
        }
        validate_fixed_step_limits(
            diagnostics,
            prefix,
            self.max_fixed_steps_per_frame,
            self.max_fixed_debt_steps,
        );
    }

    fn apply_to(&self, runtime: &mut EffectiveRuntimeSettings) -> Result<(), ProjectProfileError> {
        let mut runtime_time = runtime.runtime_time_settings();
        let mut fixed_time = runtime.fixed_time();
        if let Some(paused) = self.paused {
            runtime_time.set_paused(paused);
        }
        if let Some(time_scale) = self.time_scale {
            runtime_time
                .set_time_scale(time_scale)
                .map_err(ProjectProfileError::InvalidRuntimeSettings)?;
        }
        if let Some(seconds) = self.max_delta_seconds {
            let max_delta = duration_from_positive_seconds(seconds).ok_or(
                ProjectProfileError::InvalidRuntimeDuration {
                    field: "profiles.runtime.max_delta_seconds",
                },
            )?;
            runtime_time
                .set_max_delta(max_delta)
                .map_err(ProjectProfileError::InvalidRuntimeSettings)?;
        }
        if let Some(seconds) = self.fixed_timestep_seconds {
            let timestep = duration_from_positive_seconds(seconds).ok_or(
                ProjectProfileError::InvalidRuntimeDuration {
                    field: "profiles.runtime.fixed_timestep_seconds",
                },
            )?;
            fixed_time
                .set_timestep(timestep)
                .map_err(ProjectProfileError::InvalidRuntimeSettings)?;
        }
        if let Some(max_steps) = self.max_fixed_steps_per_frame {
            fixed_time
                .set_max_steps_per_frame(max_steps)
                .map_err(ProjectProfileError::InvalidRuntimeSettings)?;
        }
        if let Some(max_debt_steps) = self.max_fixed_debt_steps {
            fixed_time
                .set_max_debt_steps(max_debt_steps)
                .map_err(ProjectProfileError::InvalidRuntimeSettings)?;
        }
        if let Some(catch_up_policy) = self.catch_up_policy {
            fixed_time
                .set_catch_up_policy(catch_up_policy.into())
                .map_err(ProjectProfileError::InvalidRuntimeSettings)?;
        }
        runtime.replace(runtime_time, fixed_time);
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTasksPatch {
    #[serde(default)]
    pub io: ProjectTaskPoolPatch,
    #[serde(default)]
    pub compute: ProjectTaskPoolPatch,
    #[serde(default)]
    pub async_compute: ProjectTaskPoolPatch,
    #[serde(default)]
    pub shutdown: ProjectTaskShutdownPatch,
}

impl ProjectTasksPatch {
    fn validate_into(
        &self,
        diagnostics: &mut DiagnosticReport,
        prefix: &str,
        base: &ProjectTasksManifest,
    ) {
        self.merged_with(*base).validate_into(diagnostics, prefix);
    }

    fn apply_to(&self, tasks: &mut EffectiveTaskSettings) -> Result<(), ProjectProfileError> {
        let base = ProjectTasksManifest::from_config(tasks.pool_config);
        *tasks = self.merged_with(base).lower()?;
        Ok(())
    }

    fn merged_with(&self, mut tasks: ProjectTasksManifest) -> ProjectTasksManifest {
        self.io.apply_to(&mut tasks.io);
        self.compute.apply_to(&mut tasks.compute);
        self.async_compute.apply_to(&mut tasks.async_compute);
        self.shutdown.apply_to(&mut tasks.shutdown);
        tasks
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTaskPoolPatch {
    #[serde(default)]
    pub workers: Option<u32>,
    #[serde(default)]
    pub pending_capacity: Option<u32>,
}

impl ProjectTaskPoolPatch {
    fn apply_to(&self, pool: &mut ProjectTaskPoolManifest) {
        if let Some(workers) = self.workers {
            pool.workers = workers;
        }
        if let Some(pending_capacity) = self.pending_capacity {
            pool.pending_capacity = pending_capacity;
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTaskShutdownPatch {
    #[serde(default)]
    pub drain_timeout_ms: Option<u64>,
    #[serde(default)]
    pub cancel_timeout_ms: Option<u64>,
    #[serde(default)]
    pub join_timeout_ms: Option<u64>,
}

impl ProjectTaskShutdownPatch {
    fn apply_to(&self, shutdown: &mut ProjectTaskShutdownManifest) {
        if let Some(drain_timeout_ms) = self.drain_timeout_ms {
            shutdown.drain_timeout_ms = drain_timeout_ms;
        }
        if let Some(cancel_timeout_ms) = self.cancel_timeout_ms {
            shutdown.cancel_timeout_ms = cancel_timeout_ms;
        }
        if let Some(join_timeout_ms) = self.join_timeout_ms {
            shutdown.join_timeout_ms = join_timeout_ms;
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectWindowPatch {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub scale_factor: Option<f64>,
    #[serde(default)]
    pub mode: Option<ProjectWindowMode>,
    #[serde(default)]
    pub present_mode: Option<ProjectPresentMode>,
    #[serde(default)]
    pub resizable: Option<bool>,
}

impl ProjectWindowPatch {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if self
            .title
            .as_ref()
            .is_some_and(|title| title.trim().is_empty())
        {
            diagnostics.push(with_field_path(
                error("project.window.empty-title", "Window title cannot be empty"),
                &format!("{prefix}.title"),
            ));
        }
        if matches!(self.width, Some(0)) {
            let diagnostic = error(
                "project.window.invalid-width",
                "Window width must be greater than zero",
            );
            let diagnostic = with_field_path(diagnostic, &format!("{prefix}.width"));
            diagnostics.push(with_public_u64(diagnostic, "actual", 0));
        }
        if matches!(self.height, Some(0)) {
            let diagnostic = error(
                "project.window.invalid-height",
                "Window height must be greater than zero",
            );
            let diagnostic = with_field_path(diagnostic, &format!("{prefix}.height"));
            diagnostics.push(with_public_u64(diagnostic, "actual", 0));
        }
        if let Some(scale_factor) = self.scale_factor
            && (!scale_factor.is_finite() || scale_factor <= 0.0)
        {
            let diagnostic = error(
                "project.window.invalid-scale-factor",
                "Window scale factor must be finite and greater than zero",
            );
            let diagnostic = with_field_path(diagnostic, &format!("{prefix}.scale_factor"));
            let diagnostic = with_public_bool(diagnostic, "finite", scale_factor.is_finite());
            diagnostics.push(with_public_bool(diagnostic, "positive", scale_factor > 0.0));
        }
    }

    fn apply_to(&self, window: &mut EffectiveWindowSettings) {
        if let Some(enabled) = self.enabled {
            window.enabled = enabled;
        }
        if let Some(title) = &self.title {
            window.title = title.clone();
        }
        if let Some(width) = self.width {
            window.width = width;
        }
        if let Some(height) = self.height {
            window.height = height;
        }
        if let Some(scale_factor) = self.scale_factor {
            window.scale_factor = scale_factor;
        }
        if let Some(mode) = self.mode {
            window.mode = mode.into();
        }
        if let Some(present_mode) = self.present_mode {
            window.present_mode = present_mode.into();
        }
        if let Some(resizable) = self.resizable {
            window.resizable = resizable;
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInputPatch {
    #[serde(default)]
    pub action_map: Option<String>,
    #[serde(default)]
    pub clear_action_map: bool,
}

impl ProjectInputPatch {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if let Some(action_map) = &self.action_map {
            validate_path_field(diagnostics, &format!("{prefix}.action_map"), action_map);
        }
        if self.clear_action_map && self.action_map.is_some() {
            diagnostics.push(with_field_path(
                error(
                    "project.input.conflicting-action-map-patch",
                    "Input patch cannot both clear and set the action map",
                ),
                &format!("{prefix}.action_map"),
            ));
        }
    }

    fn apply_to(&self, input: &mut EffectiveInputSettings) -> Result<(), ProjectProfileError> {
        if self.clear_action_map {
            input.action_map = None;
        }
        if let Some(action_map) = &self.action_map {
            input.action_map = Some(
                ProjectPath::new(action_map.clone())
                    .map_err(ProjectProfileError::InvalidProjectPath)?,
            );
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDiagnosticsPatch {
    #[serde(default)]
    pub runtime_capacity: Option<usize>,
}

impl ProjectDiagnosticsPatch {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if matches!(self.runtime_capacity, Some(0)) {
            let diagnostic = error(
                "project.diagnostics.invalid-runtime-capacity",
                "Runtime diagnostic capacity must be greater than zero",
            );
            let diagnostic = with_field_path(diagnostic, &format!("{prefix}.runtime_capacity"));
            diagnostics.push(with_public_u64(diagnostic, "actual", 0));
        }
        if let Some(capacity) = self.runtime_capacity
            && capacity > MAX_RUNTIME_DIAGNOSTIC_ENTRIES
        {
            let diagnostic = error(
                "project.diagnostics.runtime-capacity-too-large",
                "Runtime diagnostic capacity exceeds its hard limit",
            );
            let diagnostic = with_field_path(diagnostic, &format!("{prefix}.runtime_capacity"));
            let diagnostic = with_public_u64(
                diagnostic,
                "actual",
                u64::try_from(capacity).unwrap_or(u64::MAX),
            );
            diagnostics.push(with_public_u64(
                diagnostic,
                "limit",
                u64::try_from(MAX_RUNTIME_DIAGNOSTIC_ENTRIES).unwrap_or(u64::MAX),
            ));
        }
    }

    fn apply_to(
        &self,
        diagnostics: &mut EffectiveDiagnosticsSettings,
    ) -> Result<(), ProjectProfileError> {
        if let Some(capacity) = self.runtime_capacity {
            diagnostics.runtime = runtime_diagnostics_settings(capacity)?;
        }
        Ok(())
    }
}

#[derive(Error, Clone, PartialEq)]
pub enum ProjectProfileError {
    #[error("project manifest is invalid")]
    InvalidManifest { diagnostics: Box<DiagnosticReport> },
    #[error("unknown project profile")]
    UnknownProfile {
        profile: String,
        diagnostics: Box<DiagnosticReport>,
    },
    #[error("invalid project path: {0}")]
    InvalidProjectPath(ProjectPathError),
    #[error("runtime field '{field}' cannot be represented as a non-zero duration")]
    InvalidRuntimeDuration { field: &'static str },
    #[error("invalid runtime settings: {0}")]
    InvalidRuntimeSettings(TimeSettingsError),
    #[error("task field '{field}' must be non-zero and representable")]
    InvalidTaskLimit { field: &'static str },
    #[error("invalid task settings: {0}")]
    InvalidTaskSettings(TaskConfigError),
    #[error("diagnostic capacity must be non-zero and representable")]
    InvalidDiagnosticCapacity,
    #[error("invalid diagnostic settings: {0}")]
    InvalidDiagnosticSettings(DiagnosticSettingsError),
}

impl fmt::Debug for ProjectProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidManifest { diagnostics } => formatter
                .debug_struct("InvalidManifest")
                .field("diagnostics", diagnostics)
                .finish(),
            Self::UnknownProfile { diagnostics, .. } => formatter
                .debug_struct("UnknownProfile")
                .field("profile", &"[REDACTED]")
                .field("diagnostics", diagnostics)
                .finish(),
            Self::InvalidProjectPath(error) => formatter
                .debug_tuple("InvalidProjectPath")
                .field(error)
                .finish(),
            Self::InvalidRuntimeDuration { field } => formatter
                .debug_struct("InvalidRuntimeDuration")
                .field("field", field)
                .finish(),
            Self::InvalidRuntimeSettings(error) => formatter
                .debug_tuple("InvalidRuntimeSettings")
                .field(error)
                .finish(),
            Self::InvalidTaskLimit { field } => formatter
                .debug_struct("InvalidTaskLimit")
                .field("field", field)
                .finish(),
            Self::InvalidTaskSettings(error) => formatter
                .debug_tuple("InvalidTaskSettings")
                .field(error)
                .finish(),
            Self::InvalidDiagnosticCapacity => formatter.write_str("InvalidDiagnosticCapacity"),
            Self::InvalidDiagnosticSettings(error) => formatter
                .debug_tuple("InvalidDiagnosticSettings")
                .field(error)
                .finish(),
        }
    }
}
