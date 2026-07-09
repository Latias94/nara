use std::time::Duration;

use nara_diagnostic::{
    Diagnostic, DiagnosticReport, MAX_RUNTIME_DIAGNOSTICS_CAPACITY, RuntimeDiagnosticsSettings,
};
use nara_tasks::TaskPoolConfig;
use serde::Deserialize;
use thiserror::Error;

use crate::effective::{
    EffectiveDiagnosticsSettings, EffectiveInputSettings, EffectiveProjectPaths,
    EffectiveProjectSettings, EffectiveRuntimeSettings, EffectiveStartupSettings,
    EffectiveTaskSettings, EffectiveWindowSettings,
};
use crate::path::{ProjectPath, ProjectPathError};
use crate::sections::{
    ProjectPluginPlan, ProjectPresentMode, ProjectTaskExecutionMode, ProjectWindowMode,
};
use crate::validation::{
    validate_optional_max_thread_count, validate_optional_path_field, validate_path_field,
    validate_positive_seconds,
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
    pub(crate) fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        self.paths
            .validate_into(diagnostics, &format!("{prefix}.paths"));
        self.startup
            .validate_into(diagnostics, &format!("{prefix}.startup"));
        self.runtime
            .validate_into(diagnostics, &format!("{prefix}.runtime"));
        self.tasks
            .validate_into(diagnostics, &format!("{prefix}.tasks"));
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
        self.runtime.apply_to(&mut settings.runtime);
        if let Some(plugin_plan) = self.runtime.plugin_plan {
            settings.plugin_plan = plugin_plan;
        }
        self.tasks.apply_to(&mut settings.tasks);
        self.window.apply_to(&mut settings.window);
        self.input.apply_to(&mut settings.input)?;
        self.diagnostics.apply_to(&mut settings.diagnostics);
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
            diagnostics.push(
                Diagnostic::error(
                    "project.startup.conflicting-default-scene-patch",
                    "startup patch cannot both clear and set default_scene",
                )
                .with_field_path(format!("{prefix}.default_scene")),
            );
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
    pub plugin_plan: Option<ProjectPluginPlan>,
}

impl ProjectRuntimePatch {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if let Some(time_scale) = self.time_scale
            && (!time_scale.is_finite() || time_scale < 0.0)
        {
            diagnostics.push(
                Diagnostic::error(
                    "project.runtime.invalid-time-scale",
                    "runtime.time_scale must be finite and >= 0",
                )
                .with_field_path(format!("{prefix}.time_scale")),
            );
        }
        if let Some(seconds) = self.max_delta_seconds {
            validate_positive_seconds(diagnostics, &format!("{prefix}.max_delta_seconds"), seconds);
        }
        if let Some(seconds) = self.fixed_timestep_seconds {
            validate_positive_seconds(
                diagnostics,
                &format!("{prefix}.fixed_timestep_seconds"),
                seconds,
            );
        }
        if matches!(self.max_fixed_steps_per_frame, Some(0)) {
            diagnostics.push(
                Diagnostic::error(
                    "project.runtime.invalid-max-fixed-steps",
                    "runtime.max_fixed_steps_per_frame must be greater than zero",
                )
                .with_field_path(format!("{prefix}.max_fixed_steps_per_frame")),
            );
        }
    }

    fn apply_to(&self, runtime: &mut EffectiveRuntimeSettings) {
        if let Some(paused) = self.paused {
            runtime.paused = paused;
        }
        if let Some(time_scale) = self.time_scale {
            runtime.time_scale = time_scale;
        }
        if let Some(seconds) = self.max_delta_seconds {
            runtime.max_delta = Duration::from_secs_f64(seconds);
        }
        if let Some(seconds) = self.fixed_timestep_seconds {
            runtime.fixed_timestep = Duration::from_secs_f64(seconds);
        }
        if let Some(max_steps) = self.max_fixed_steps_per_frame {
            runtime.max_fixed_steps_per_frame = max_steps;
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTasksPatch {
    #[serde(default)]
    pub mode: Option<ProjectTaskExecutionMode>,
    #[serde(default)]
    pub io_threads: Option<usize>,
    #[serde(default)]
    pub compute_threads: Option<usize>,
    #[serde(default)]
    pub async_compute_threads: Option<usize>,
}

impl ProjectTasksPatch {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        validate_optional_max_thread_count(
            diagnostics,
            &format!("{prefix}.io_threads"),
            self.io_threads,
        );
        validate_optional_max_thread_count(
            diagnostics,
            &format!("{prefix}.compute_threads"),
            self.compute_threads,
        );
        validate_optional_max_thread_count(
            diagnostics,
            &format!("{prefix}.async_compute_threads"),
            self.async_compute_threads,
        );
    }

    fn apply_to(&self, tasks: &mut EffectiveTaskSettings) {
        if let Some(mode) = self.mode {
            tasks.mode = mode;
        }
        let mut io_threads = tasks.pool_config.threads_for(nara_tasks::TaskPoolKind::Io);
        let mut compute_threads = tasks
            .pool_config
            .threads_for(nara_tasks::TaskPoolKind::Compute);
        let mut async_compute_threads = tasks
            .pool_config
            .threads_for(nara_tasks::TaskPoolKind::AsyncCompute);

        if let Some(value) = self.io_threads {
            io_threads = value;
        }
        if let Some(value) = self.compute_threads {
            compute_threads = value;
        }
        if let Some(value) = self.async_compute_threads {
            async_compute_threads = value;
        }

        tasks.pool_config = match tasks.mode {
            ProjectTaskExecutionMode::Deterministic => TaskPoolConfig::deterministic(),
            ProjectTaskExecutionMode::Threaded => {
                TaskPoolConfig::threaded(io_threads, compute_threads, async_compute_threads)
            }
        };
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
            diagnostics.push(
                Diagnostic::error("project.window.empty-title", "window.title cannot be empty")
                    .with_field_path(format!("{prefix}.title")),
            );
        }
        if matches!(self.width, Some(0)) {
            diagnostics.push(
                Diagnostic::error(
                    "project.window.invalid-width",
                    "window.width must be greater than zero",
                )
                .with_field_path(format!("{prefix}.width")),
            );
        }
        if matches!(self.height, Some(0)) {
            diagnostics.push(
                Diagnostic::error(
                    "project.window.invalid-height",
                    "window.height must be greater than zero",
                )
                .with_field_path(format!("{prefix}.height")),
            );
        }
        if let Some(scale_factor) = self.scale_factor
            && (!scale_factor.is_finite() || scale_factor <= 0.0)
        {
            diagnostics.push(
                Diagnostic::error(
                    "project.window.invalid-scale-factor",
                    "window.scale_factor must be finite and greater than zero",
                )
                .with_field_path(format!("{prefix}.scale_factor")),
            );
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
            diagnostics.push(
                Diagnostic::error(
                    "project.input.conflicting-action-map-patch",
                    "input patch cannot both clear and set action_map",
                )
                .with_field_path(format!("{prefix}.action_map")),
            );
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
            diagnostics.push(
                Diagnostic::error(
                    "project.diagnostics.invalid-runtime-capacity",
                    "diagnostics.runtime_capacity must be greater than zero",
                )
                .with_field_path(format!("{prefix}.runtime_capacity")),
            );
        }
        if let Some(capacity) = self.runtime_capacity
            && capacity > MAX_RUNTIME_DIAGNOSTICS_CAPACITY
        {
            diagnostics.push(
                Diagnostic::error(
                    "project.diagnostics.runtime-capacity-too-large",
                    format!(
                        "diagnostics.runtime_capacity must be <= {MAX_RUNTIME_DIAGNOSTICS_CAPACITY}"
                    ),
                )
                .with_field_path(format!("{prefix}.runtime_capacity")),
            );
        }
    }

    fn apply_to(&self, diagnostics: &mut EffectiveDiagnosticsSettings) {
        if let Some(capacity) = self.runtime_capacity {
            diagnostics.runtime = RuntimeDiagnosticsSettings { capacity };
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProjectProfileError {
    #[error("project manifest is invalid")]
    InvalidManifest { diagnostics: DiagnosticReport },
    #[error("unknown project profile '{profile}'")]
    UnknownProfile {
        profile: String,
        diagnostics: DiagnosticReport,
    },
    #[error("invalid project path: {0}")]
    InvalidProjectPath(ProjectPathError),
}
