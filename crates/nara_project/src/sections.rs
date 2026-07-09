use std::time::Duration;

use nara_diagnostic::{
    Diagnostic, DiagnosticReport, MAX_RUNTIME_DIAGNOSTICS_CAPACITY, RuntimeDiagnosticsSettings,
};
use nara_tasks::TaskPoolConfig;
use nara_window::{PresentMode, WindowMode};
use serde::Deserialize;

use crate::defaults::*;
use crate::effective::{
    EffectiveDiagnosticsSettings, EffectiveInputSettings, EffectiveRuntimeSettings,
    EffectiveTaskSettings, EffectiveWindowSettings,
};
use crate::path::ProjectPath;
use crate::profile::ProjectProfileError;
use crate::validation::{
    validate_max_thread_count, validate_path_field, validate_positive_seconds,
    validate_thread_count,
};

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectInfo {
    pub name: String,
    #[serde(default)]
    pub stable_id: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ProjectStartupManifest {
    #[serde(default)]
    pub default_scene: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
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
    #[serde(default)]
    pub plugin_plan: ProjectPluginPlan,
}

impl Default for ProjectRuntimeManifest {
    fn default() -> Self {
        Self {
            paused: false,
            time_scale: default_time_scale(),
            max_delta_seconds: default_max_delta_seconds(),
            fixed_timestep_seconds: default_fixed_timestep_seconds(),
            max_fixed_steps_per_frame: default_max_fixed_steps_per_frame(),
            plugin_plan: ProjectPluginPlan::default(),
        }
    }
}

impl ProjectRuntimeManifest {
    pub(crate) fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if !self.time_scale.is_finite() || self.time_scale < 0.0 {
            diagnostics.push(
                Diagnostic::error(
                    "project.runtime.invalid-time-scale",
                    "runtime.time_scale must be finite and >= 0",
                )
                .with_field_path(format!("{prefix}.time_scale")),
            );
        }
        validate_positive_seconds(
            diagnostics,
            &format!("{prefix}.max_delta_seconds"),
            self.max_delta_seconds,
        );
        validate_positive_seconds(
            diagnostics,
            &format!("{prefix}.fixed_timestep_seconds"),
            self.fixed_timestep_seconds,
        );
        if self.max_fixed_steps_per_frame == 0 {
            diagnostics.push(
                Diagnostic::error(
                    "project.runtime.invalid-max-fixed-steps",
                    "runtime.max_fixed_steps_per_frame must be greater than zero",
                )
                .with_field_path(format!("{prefix}.max_fixed_steps_per_frame")),
            );
        }
    }

    pub(crate) fn lower(self) -> EffectiveRuntimeSettings {
        EffectiveRuntimeSettings {
            paused: self.paused,
            time_scale: self.time_scale,
            max_delta: Duration::from_secs_f64(self.max_delta_seconds),
            fixed_timestep: Duration::from_secs_f64(self.fixed_timestep_seconds),
            max_fixed_steps_per_frame: self.max_fixed_steps_per_frame,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectTaskExecutionMode {
    Deterministic,
    #[default]
    Threaded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTasksManifest {
    #[serde(default)]
    pub mode: ProjectTaskExecutionMode,
    #[serde(default = "default_io_threads")]
    pub io_threads: usize,
    #[serde(default = "default_compute_threads")]
    pub compute_threads: usize,
    #[serde(default = "default_async_compute_threads")]
    pub async_compute_threads: usize,
}

impl Default for ProjectTasksManifest {
    fn default() -> Self {
        Self {
            mode: ProjectTaskExecutionMode::default(),
            io_threads: default_io_threads(),
            compute_threads: default_compute_threads(),
            async_compute_threads: default_async_compute_threads(),
        }
    }
}

impl ProjectTasksManifest {
    pub(crate) fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if matches!(self.mode, ProjectTaskExecutionMode::Threaded) {
            validate_thread_count(
                diagnostics,
                &format!("{prefix}.io_threads"),
                self.io_threads,
            );
            validate_thread_count(
                diagnostics,
                &format!("{prefix}.compute_threads"),
                self.compute_threads,
            );
            validate_thread_count(
                diagnostics,
                &format!("{prefix}.async_compute_threads"),
                self.async_compute_threads,
            );
        }
        validate_max_thread_count(
            diagnostics,
            &format!("{prefix}.io_threads"),
            self.io_threads,
        );
        validate_max_thread_count(
            diagnostics,
            &format!("{prefix}.compute_threads"),
            self.compute_threads,
        );
        validate_max_thread_count(
            diagnostics,
            &format!("{prefix}.async_compute_threads"),
            self.async_compute_threads,
        );
    }

    pub(crate) fn lower(self) -> EffectiveTaskSettings {
        let pool_config = match self.mode {
            ProjectTaskExecutionMode::Deterministic => TaskPoolConfig::deterministic(),
            ProjectTaskExecutionMode::Threaded => TaskPoolConfig::threaded(
                self.io_threads,
                self.compute_threads,
                self.async_compute_threads,
            ),
        };
        EffectiveTaskSettings {
            mode: self.mode,
            pool_config,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectWindowManifest {
    #[serde(default = "default_window_enabled")]
    pub enabled: bool,
    #[serde(default = "default_window_title")]
    pub title: String,
    #[serde(default = "default_window_width")]
    pub width: u32,
    #[serde(default = "default_window_height")]
    pub height: u32,
    #[serde(default = "default_window_scale_factor")]
    pub scale_factor: f64,
    #[serde(default)]
    pub mode: ProjectWindowMode,
    #[serde(default)]
    pub present_mode: ProjectPresentMode,
    #[serde(default = "default_window_resizable")]
    pub resizable: bool,
}

impl Default for ProjectWindowManifest {
    fn default() -> Self {
        Self {
            enabled: default_window_enabled(),
            title: default_window_title(),
            width: default_window_width(),
            height: default_window_height(),
            scale_factor: default_window_scale_factor(),
            mode: ProjectWindowMode::default(),
            present_mode: ProjectPresentMode::default(),
            resizable: default_window_resizable(),
        }
    }
}

impl ProjectWindowManifest {
    pub(crate) fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if self.title.trim().is_empty() {
            diagnostics.push(
                Diagnostic::error("project.window.empty-title", "window.title cannot be empty")
                    .with_field_path(format!("{prefix}.title")),
            );
        }
        if self.width == 0 {
            diagnostics.push(
                Diagnostic::error(
                    "project.window.invalid-width",
                    "window.width must be greater than zero",
                )
                .with_field_path(format!("{prefix}.width")),
            );
        }
        if self.height == 0 {
            diagnostics.push(
                Diagnostic::error(
                    "project.window.invalid-height",
                    "window.height must be greater than zero",
                )
                .with_field_path(format!("{prefix}.height")),
            );
        }
        if !self.scale_factor.is_finite() || self.scale_factor <= 0.0 {
            diagnostics.push(
                Diagnostic::error(
                    "project.window.invalid-scale-factor",
                    "window.scale_factor must be finite and greater than zero",
                )
                .with_field_path(format!("{prefix}.scale_factor")),
            );
        }
    }

    pub(crate) fn lower(self) -> EffectiveWindowSettings {
        EffectiveWindowSettings {
            enabled: self.enabled,
            title: self.title,
            width: self.width,
            height: self.height,
            scale_factor: self.scale_factor,
            mode: self.mode.into(),
            present_mode: self.present_mode.into(),
            resizable: self.resizable,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectWindowMode {
    #[default]
    Windowed,
    BorderlessFullscreen,
    Fullscreen,
}

impl From<ProjectWindowMode> for WindowMode {
    fn from(value: ProjectWindowMode) -> Self {
        match value {
            ProjectWindowMode::Windowed => Self::Windowed,
            ProjectWindowMode::BorderlessFullscreen => Self::BorderlessFullscreen,
            ProjectWindowMode::Fullscreen => Self::Fullscreen,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectPresentMode {
    #[default]
    AutoVsync,
    AutoNoVsync,
    Fifo,
    Immediate,
    Mailbox,
}

impl From<ProjectPresentMode> for PresentMode {
    fn from(value: ProjectPresentMode) -> Self {
        match value {
            ProjectPresentMode::AutoVsync => Self::AutoVsync,
            ProjectPresentMode::AutoNoVsync => Self::AutoNoVsync,
            ProjectPresentMode::Fifo => Self::Fifo,
            ProjectPresentMode::Immediate => Self::Immediate,
            ProjectPresentMode::Mailbox => Self::Mailbox,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDiagnosticsManifest {
    #[serde(default = "default_diagnostics_capacity")]
    pub runtime_capacity: usize,
}

impl Default for ProjectDiagnosticsManifest {
    fn default() -> Self {
        Self {
            runtime_capacity: default_diagnostics_capacity(),
        }
    }
}

impl ProjectDiagnosticsManifest {
    pub(crate) fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if self.runtime_capacity == 0 {
            diagnostics.push(
                Diagnostic::error(
                    "project.diagnostics.invalid-runtime-capacity",
                    "diagnostics.runtime_capacity must be greater than zero",
                )
                .with_field_path(format!("{prefix}.runtime_capacity")),
            );
        }
        if self.runtime_capacity > MAX_RUNTIME_DIAGNOSTICS_CAPACITY {
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

    pub(crate) const fn lower(self) -> EffectiveDiagnosticsSettings {
        EffectiveDiagnosticsSettings {
            runtime: RuntimeDiagnosticsSettings {
                capacity: self.runtime_capacity,
            },
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ProjectPluginPlan {
    #[default]
    #[serde(rename = "minimal")]
    Minimal,
    #[serde(rename = "headless-runtime")]
    HeadlessRuntime,
    #[serde(rename = "server")]
    Server,
    #[serde(rename = "runtime-2d", alias = "runtime2d")]
    Runtime2d,
    #[serde(rename = "desktop-window")]
    DesktopWindow,
    #[serde(rename = "desktop-wgpu")]
    DesktopWgpu,
    #[serde(rename = "tooling")]
    Tooling,
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
