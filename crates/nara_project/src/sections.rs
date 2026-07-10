use std::time::Duration;

use nara_app::{FixedCatchUpPolicy, FixedTime, RuntimeTimeSettings};
use nara_core::{ItemLimit, TimeLimit};
use nara_diagnostic::{
    Diagnostic, DiagnosticReport, MAX_RUNTIME_DIAGNOSTICS_CAPACITY, RuntimeDiagnosticsSettings,
};
use nara_tasks::{
    MAX_TASK_POOL_PENDING_PER_KIND, MAX_TASK_POOL_PENDING_TOTAL, MAX_TASK_POOL_THREADS_PER_KIND,
    MAX_TASK_POOL_THREADS_TOTAL, MAX_TASK_SHUTDOWN_PHASE_TIMEOUT, MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT,
    TaskKindConfig, TaskPoolConfig, TaskPoolKind, TaskShutdownPolicy,
};
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
    duration_from_positive_seconds, validate_duration_seconds, validate_fixed_step_limits,
    validate_path_field,
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
    #[serde(default = "default_max_fixed_debt_steps")]
    pub max_fixed_debt_steps: u32,
    #[serde(default)]
    pub catch_up_policy: ProjectFixedCatchUpPolicy,
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
            max_fixed_debt_steps: default_max_fixed_debt_steps(),
            catch_up_policy: ProjectFixedCatchUpPolicy::default(),
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
        let runtime_time = RuntimeTimeSettings::new(self.paused, self.time_scale, max_delta)
            .map_err(ProjectProfileError::InvalidRuntimeSettings)?;
        let fixed_time = FixedTime::new(fixed_timestep)
            .map_err(ProjectProfileError::InvalidRuntimeSettings)?
            .with_max_steps_per_frame(self.max_fixed_steps_per_frame)
            .map_err(ProjectProfileError::InvalidRuntimeSettings)?
            .with_max_debt_steps(self.max_fixed_debt_steps)
            .map_err(ProjectProfileError::InvalidRuntimeSettings)?
            .with_catch_up_policy(self.catch_up_policy.into());
        Ok(EffectiveRuntimeSettings::new(runtime_time, fixed_time))
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectFixedCatchUpPolicy {
    #[default]
    DiscardExcess,
    PreserveDebt,
}

impl From<ProjectFixedCatchUpPolicy> for FixedCatchUpPolicy {
    fn from(value: ProjectFixedCatchUpPolicy) -> Self {
        match value {
            ProjectFixedCatchUpPolicy::DiscardExcess => Self::DiscardExcess,
            ProjectFixedCatchUpPolicy::PreserveDebt => Self::PreserveDebt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTaskPoolManifest {
    pub workers: u32,
    pub pending_capacity: u32,
}

impl ProjectTaskPoolManifest {
    fn from_config(config: TaskKindConfig) -> Self {
        Self {
            workers: u32::try_from(config.workers().get()).unwrap_or(u32::MAX),
            pending_capacity: u32::try_from(config.pending().get()).unwrap_or(u32::MAX),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTaskShutdownManifest {
    pub drain_timeout_ms: u64,
    pub cancel_timeout_ms: u64,
    pub join_timeout_ms: u64,
}

impl ProjectTaskShutdownManifest {
    fn from_policy(policy: TaskShutdownPolicy) -> Self {
        Self {
            drain_timeout_ms: duration_millis_u64(policy.drain_timeout().get()),
            cancel_timeout_ms: duration_millis_u64(policy.cancel_timeout().get()),
            join_timeout_ms: duration_millis_u64(policy.join_timeout().get()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTasksManifest {
    #[serde(default = "default_io_task_pool")]
    pub io: ProjectTaskPoolManifest,
    #[serde(default = "default_compute_task_pool")]
    pub compute: ProjectTaskPoolManifest,
    #[serde(default = "default_async_compute_task_pool")]
    pub async_compute: ProjectTaskPoolManifest,
    #[serde(default = "default_task_shutdown")]
    pub shutdown: ProjectTaskShutdownManifest,
}

impl Default for ProjectTasksManifest {
    fn default() -> Self {
        Self::from_config(TaskPoolConfig::default())
    }
}

impl ProjectTasksManifest {
    pub(crate) fn from_config(config: TaskPoolConfig) -> Self {
        Self {
            io: ProjectTaskPoolManifest::from_config(config.kind(TaskPoolKind::Io)),
            compute: ProjectTaskPoolManifest::from_config(config.kind(TaskPoolKind::Compute)),
            async_compute: ProjectTaskPoolManifest::from_config(
                config.kind(TaskPoolKind::AsyncCompute),
            ),
            shutdown: ProjectTaskShutdownManifest::from_policy(config.shutdown_policy()),
        }
    }

    pub(crate) fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        validate_task_pool(diagnostics, &format!("{prefix}.io"), self.io);
        validate_task_pool(diagnostics, &format!("{prefix}.compute"), self.compute);
        validate_task_pool(
            diagnostics,
            &format!("{prefix}.async_compute"),
            self.async_compute,
        );
        validate_task_aggregates(diagnostics, prefix, self);
        validate_task_shutdown(diagnostics, &format!("{prefix}.shutdown"), self.shutdown);
    }

    pub(crate) fn lower(self) -> Result<EffectiveTaskSettings, ProjectProfileError> {
        let pool_config = TaskPoolConfig::new(
            lower_task_pool("tasks.io", self.io)?,
            lower_task_pool("tasks.compute", self.compute)?,
            lower_task_pool("tasks.async_compute", self.async_compute)?,
            lower_task_shutdown(self.shutdown)?,
        )
        .map_err(ProjectProfileError::InvalidTaskSettings)?;
        Ok(EffectiveTaskSettings { pool_config })
    }
}

fn default_io_task_pool() -> ProjectTaskPoolManifest {
    ProjectTaskPoolManifest::from_config(TaskPoolConfig::default().kind(TaskPoolKind::Io))
}

fn default_compute_task_pool() -> ProjectTaskPoolManifest {
    ProjectTaskPoolManifest::from_config(TaskPoolConfig::default().kind(TaskPoolKind::Compute))
}

fn default_async_compute_task_pool() -> ProjectTaskPoolManifest {
    ProjectTaskPoolManifest::from_config(TaskPoolConfig::default().kind(TaskPoolKind::AsyncCompute))
}

fn default_task_shutdown() -> ProjectTaskShutdownManifest {
    ProjectTaskShutdownManifest::from_policy(TaskPoolConfig::default().shutdown_policy())
}

fn validate_task_pool(
    diagnostics: &mut DiagnosticReport,
    prefix: &str,
    pool: ProjectTaskPoolManifest,
) {
    if pool.workers == 0 {
        diagnostics.push(
            Diagnostic::error(
                "project.tasks.invalid-workers",
                "task pool workers must be greater than zero",
            )
            .with_field_path(format!("{prefix}.workers")),
        );
    }
    if usize::try_from(pool.workers)
        .map_or(true, |workers| workers > MAX_TASK_POOL_THREADS_PER_KIND)
    {
        diagnostics.push(
            Diagnostic::error(
                "project.tasks.workers-too-large",
                format!("task pool workers must be <= {MAX_TASK_POOL_THREADS_PER_KIND}"),
            )
            .with_field_path(format!("{prefix}.workers")),
        );
    }
    if pool.pending_capacity == 0 {
        diagnostics.push(
            Diagnostic::error(
                "project.tasks.invalid-pending-capacity",
                "task pool pending_capacity must be greater than zero",
            )
            .with_field_path(format!("{prefix}.pending_capacity")),
        );
    }
    if usize::try_from(pool.pending_capacity)
        .map_or(true, |pending| pending > MAX_TASK_POOL_PENDING_PER_KIND)
    {
        diagnostics.push(
            Diagnostic::error(
                "project.tasks.pending-capacity-too-large",
                format!("task pool pending_capacity must be <= {MAX_TASK_POOL_PENDING_PER_KIND}"),
            )
            .with_field_path(format!("{prefix}.pending_capacity")),
        );
    }
}

fn validate_task_aggregates(
    diagnostics: &mut DiagnosticReport,
    prefix: &str,
    tasks: &ProjectTasksManifest,
) {
    let total_workers = u64::from(tasks.io.workers)
        + u64::from(tasks.compute.workers)
        + u64::from(tasks.async_compute.workers);
    let max_workers = u64::try_from(MAX_TASK_POOL_THREADS_TOTAL).unwrap_or(u64::MAX);
    if total_workers > max_workers {
        diagnostics.push(
            Diagnostic::error(
                "project.tasks.total-workers-too-large",
                format!("task pools request {total_workers} workers, maximum is {max_workers}"),
            )
            .with_field_path(prefix),
        );
    }

    let total_pending = u64::from(tasks.io.pending_capacity)
        + u64::from(tasks.compute.pending_capacity)
        + u64::from(tasks.async_compute.pending_capacity);
    let max_pending = u64::try_from(MAX_TASK_POOL_PENDING_TOTAL).unwrap_or(u64::MAX);
    if total_pending > max_pending {
        diagnostics.push(
            Diagnostic::error(
                "project.tasks.total-pending-too-large",
                format!(
                    "task pools request {total_pending} pending tasks, maximum is {max_pending}"
                ),
            )
            .with_field_path(prefix),
        );
    }
}

fn validate_task_shutdown(
    diagnostics: &mut DiagnosticReport,
    prefix: &str,
    shutdown: ProjectTaskShutdownManifest,
) {
    for (field, milliseconds) in [
        ("drain_timeout_ms", shutdown.drain_timeout_ms),
        ("cancel_timeout_ms", shutdown.cancel_timeout_ms),
        ("join_timeout_ms", shutdown.join_timeout_ms),
    ] {
        let field_path = format!("{prefix}.{field}");
        if milliseconds == 0 {
            diagnostics.push(
                Diagnostic::error(
                    "project.tasks.invalid-shutdown-timeout",
                    "task shutdown timeouts must be greater than zero",
                )
                .with_field_path(&field_path),
            );
        }
        if u128::from(milliseconds) > MAX_TASK_SHUTDOWN_PHASE_TIMEOUT.as_millis() {
            diagnostics.push(
                Diagnostic::error(
                    "project.tasks.shutdown-timeout-too-large",
                    format!(
                        "task shutdown timeout must be <= {} ms",
                        MAX_TASK_SHUTDOWN_PHASE_TIMEOUT.as_millis()
                    ),
                )
                .with_field_path(field_path),
            );
        }
    }

    let total_milliseconds = u128::from(shutdown.drain_timeout_ms)
        + u128::from(shutdown.cancel_timeout_ms)
        + u128::from(shutdown.join_timeout_ms);
    if total_milliseconds > MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT.as_millis() {
        diagnostics.push(
            Diagnostic::error(
                "project.tasks.shutdown-total-too-large",
                format!(
                    "task shutdown timeouts total {total_milliseconds} ms, maximum is {} ms",
                    MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT.as_millis()
                ),
            )
            .with_field_path(prefix),
        );
    }
}

fn lower_task_pool(
    field: &'static str,
    pool: ProjectTaskPoolManifest,
) -> Result<TaskKindConfig, ProjectProfileError> {
    let workers = usize::try_from(pool.workers)
        .ok()
        .and_then(ItemLimit::new)
        .ok_or(ProjectProfileError::InvalidTaskLimit { field })?;
    let pending = usize::try_from(pool.pending_capacity)
        .ok()
        .and_then(ItemLimit::new)
        .ok_or(ProjectProfileError::InvalidTaskLimit { field })?;
    Ok(TaskKindConfig::new(workers, pending))
}

fn lower_task_shutdown(
    shutdown: ProjectTaskShutdownManifest,
) -> Result<TaskShutdownPolicy, ProjectProfileError> {
    Ok(TaskShutdownPolicy::new(
        lower_timeout("tasks.shutdown.drain_timeout_ms", shutdown.drain_timeout_ms)?,
        lower_timeout(
            "tasks.shutdown.cancel_timeout_ms",
            shutdown.cancel_timeout_ms,
        )?,
        lower_timeout("tasks.shutdown.join_timeout_ms", shutdown.join_timeout_ms)?,
    ))
}

fn lower_timeout(field: &'static str, milliseconds: u64) -> Result<TimeLimit, ProjectProfileError> {
    TimeLimit::new(Duration::from_millis(milliseconds))
        .ok_or(ProjectProfileError::InvalidTaskLimit { field })
}

fn duration_millis_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
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
    #[serde(rename = "runtime-2d")]
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
