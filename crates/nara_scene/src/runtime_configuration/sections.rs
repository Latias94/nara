use std::{collections::BTreeSet, time::Duration};

use nara_app::FixedCatchUpPolicy;
use nara_app::tasks::{
    MAX_TASK_POOL_PENDING_PER_KIND, MAX_TASK_POOL_PENDING_TOTAL, MAX_TASK_POOL_THREADS_PER_KIND,
    MAX_TASK_POOL_THREADS_TOTAL, MAX_TASK_SHUTDOWN_PHASE_TIMEOUT, MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT,
    TaskKindConfig, TaskPoolConfig, TaskPoolKind, TaskShutdownPolicy,
};
use nara_core::diagnostic::{
    DiagnosticReport, MAX_RUNTIME_DIAGNOSTIC_ENTRIES, RuntimeDiagnosticsSettings,
};
use nara_core::{ByteLimit, ItemLimit, TimeLimit};
use serde::{Deserialize, Serialize};

use super::defaults::*;
use super::validation::{
    error, validate_window_fields, with_field_path, with_public_bool, with_public_identifier,
    with_public_u64,
};
use super::{
    EffectiveDiagnosticsSettings, EffectiveTaskSettings, EffectiveWindowSettings,
    MAX_PROJECT_UI_LOGICAL_EXTENT, ProductCapability, ProductCapabilitySet, RuntimeSettingsError,
};

// The manifest keeps entry capacity configurable while reserving enough space for
// roughly 256 bytes per entry at the engine's maximum capacity.
const PROJECT_RUNTIME_DIAGNOSTIC_BYTE_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ProjectUiManifest {
    #[serde(default)]
    pub primary_surface: Option<ProjectUiSurfaceManifest>,
}

impl ProjectUiManifest {
    /// Appends validation diagnostics at the supplied section path.
    pub fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if let Some(surface) = self.primary_surface {
            surface.validate_into(diagnostics, &format!("{prefix}.primary_surface"));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectUiSurfaceManifest {
    pub logical_width: u32,
    pub logical_height: u32,
    #[serde(default)]
    pub safe_area: ProjectUiSafeAreaManifest,
}

impl ProjectUiSurfaceManifest {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        validate_ui_extent(
            diagnostics,
            &format!("{prefix}.logical_width"),
            self.logical_width,
        );
        validate_ui_extent(
            diagnostics,
            &format!("{prefix}.logical_height"),
            self.logical_height,
        );

        let horizontal = u64::from(self.safe_area.left) + u64::from(self.safe_area.right);
        if horizontal >= u64::from(self.logical_width) {
            let diagnostic = error(
                "project.ui.invalid-horizontal-safe-area",
                "Primary UI safe-area horizontal insets must leave positive content width",
            );
            let diagnostic = with_field_path(diagnostic, &format!("{prefix}.safe_area"));
            let diagnostic = with_public_u64(diagnostic, "inset_total", horizontal);
            diagnostics.push(with_public_u64(
                diagnostic,
                "logical_extent",
                u64::from(self.logical_width),
            ));
        }

        let vertical = u64::from(self.safe_area.top) + u64::from(self.safe_area.bottom);
        if vertical >= u64::from(self.logical_height) {
            let diagnostic = error(
                "project.ui.invalid-vertical-safe-area",
                "Primary UI safe-area vertical insets must leave positive content height",
            );
            let diagnostic = with_field_path(diagnostic, &format!("{prefix}.safe_area"));
            let diagnostic = with_public_u64(diagnostic, "inset_total", vertical);
            diagnostics.push(with_public_u64(
                diagnostic,
                "logical_extent",
                u64::from(self.logical_height),
            ));
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectUiSafeAreaManifest {
    #[serde(default)]
    pub left: u32,
    #[serde(default)]
    pub top: u32,
    #[serde(default)]
    pub right: u32,
    #[serde(default)]
    pub bottom: u32,
}

fn validate_ui_extent(diagnostics: &mut DiagnosticReport, field: &str, extent: u32) {
    if extent == 0 || extent > MAX_PROJECT_UI_LOGICAL_EXTENT {
        let diagnostic = error(
            "project.ui.invalid-logical-extent",
            "Primary UI logical extent must be positive and within its hard limit",
        );
        let diagnostic = with_field_path(diagnostic, field);
        let diagnostic = with_public_u64(diagnostic, "actual", u64::from(extent));
        diagnostics.push(with_public_u64(
            diagnostic,
            "limit",
            u64::from(MAX_PROJECT_UI_LOGICAL_EXTENT),
        ));
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ProjectCapabilitiesManifest {
    #[serde(default)]
    pub requested: Vec<ProductCapability>,
}

impl ProjectCapabilitiesManifest {
    /// Appends validation diagnostics at the supplied section path.
    pub fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        let mut observed = BTreeSet::new();
        for capability in &self.requested {
            if !observed.insert(*capability) {
                let diagnostic = error(
                    "project.capabilities.duplicate",
                    "A product capability is requested more than once",
                );
                let diagnostic = with_field_path(diagnostic, &format!("{prefix}.requested"));
                diagnostics.push(with_public_identifier(
                    diagnostic,
                    "capability",
                    capability.as_str(),
                ));
            }
        }
    }
}

impl From<&ProjectCapabilitiesManifest> for ProductCapabilitySet {
    fn from(capabilities: &ProjectCapabilitiesManifest) -> Self {
        Self::from_capabilities(capabilities.requested.iter().copied())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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
    pub(super) fn from_config(config: TaskPoolConfig) -> Self {
        Self {
            io: ProjectTaskPoolManifest::from_config(config.kind(TaskPoolKind::Io)),
            compute: ProjectTaskPoolManifest::from_config(config.kind(TaskPoolKind::Compute)),
            async_compute: ProjectTaskPoolManifest::from_config(
                config.kind(TaskPoolKind::AsyncCompute),
            ),
            shutdown: ProjectTaskShutdownManifest::from_policy(config.shutdown_policy()),
        }
    }

    /// Appends validation diagnostics at the supplied section path.
    pub fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
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
}

impl TryFrom<ProjectTasksManifest> for EffectiveTaskSettings {
    type Error = RuntimeSettingsError;

    fn try_from(tasks: ProjectTasksManifest) -> Result<Self, Self::Error> {
        let pool_config = TaskPoolConfig::new(
            lower_task_pool("tasks.io", tasks.io)?,
            lower_task_pool("tasks.compute", tasks.compute)?,
            lower_task_pool("tasks.async_compute", tasks.async_compute)?,
            lower_task_shutdown(tasks.shutdown)?,
        )?;
        Ok(Self { pool_config })
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
        let diagnostic = error(
            "project.tasks.invalid-workers",
            "Task pool worker count must be greater than zero",
        );
        let diagnostic = with_field_path(diagnostic, &format!("{prefix}.workers"));
        diagnostics.push(with_public_u64(
            diagnostic,
            "actual",
            u64::from(pool.workers),
        ));
    }
    if usize::try_from(pool.workers)
        .map_or(true, |workers| workers > MAX_TASK_POOL_THREADS_PER_KIND)
    {
        let diagnostic = error(
            "project.tasks.workers-too-large",
            "Task pool worker count exceeds its per-kind hard limit",
        );
        let diagnostic = with_field_path(diagnostic, &format!("{prefix}.workers"));
        let diagnostic = with_public_u64(diagnostic, "actual", u64::from(pool.workers));
        diagnostics.push(with_public_u64(
            diagnostic,
            "limit",
            u64::try_from(MAX_TASK_POOL_THREADS_PER_KIND).unwrap_or(u64::MAX),
        ));
    }
    if pool.pending_capacity == 0 {
        let diagnostic = error(
            "project.tasks.invalid-pending-capacity",
            "Task pool pending capacity must be greater than zero",
        );
        let diagnostic = with_field_path(diagnostic, &format!("{prefix}.pending_capacity"));
        diagnostics.push(with_public_u64(
            diagnostic,
            "actual",
            u64::from(pool.pending_capacity),
        ));
    }
    if usize::try_from(pool.pending_capacity)
        .map_or(true, |pending| pending > MAX_TASK_POOL_PENDING_PER_KIND)
    {
        let diagnostic = error(
            "project.tasks.pending-capacity-too-large",
            "Task pool pending capacity exceeds its per-kind hard limit",
        );
        let diagnostic = with_field_path(diagnostic, &format!("{prefix}.pending_capacity"));
        let diagnostic = with_public_u64(diagnostic, "actual", u64::from(pool.pending_capacity));
        diagnostics.push(with_public_u64(
            diagnostic,
            "limit",
            u64::try_from(MAX_TASK_POOL_PENDING_PER_KIND).unwrap_or(u64::MAX),
        ));
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
        let diagnostic = error(
            "project.tasks.total-workers-too-large",
            "Combined task pool worker count exceeds its hard limit",
        );
        let diagnostic = with_field_path(diagnostic, prefix);
        let diagnostic = with_public_u64(diagnostic, "actual", total_workers);
        diagnostics.push(with_public_u64(diagnostic, "limit", max_workers));
    }

    let total_pending = u64::from(tasks.io.pending_capacity)
        + u64::from(tasks.compute.pending_capacity)
        + u64::from(tasks.async_compute.pending_capacity);
    let max_pending = u64::try_from(MAX_TASK_POOL_PENDING_TOTAL).unwrap_or(u64::MAX);
    if total_pending > max_pending {
        let diagnostic = error(
            "project.tasks.total-pending-too-large",
            "Combined task pool pending capacity exceeds its hard limit",
        );
        let diagnostic = with_field_path(diagnostic, prefix);
        let diagnostic = with_public_u64(diagnostic, "actual", total_pending);
        diagnostics.push(with_public_u64(diagnostic, "limit", max_pending));
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
            let diagnostic = error(
                "project.tasks.invalid-shutdown-timeout",
                "Task shutdown timeout must be greater than zero",
            );
            let diagnostic = with_field_path(diagnostic, &field_path);
            diagnostics.push(with_public_u64(diagnostic, "actual", milliseconds));
        }
        if u128::from(milliseconds) > MAX_TASK_SHUTDOWN_PHASE_TIMEOUT.as_millis() {
            let diagnostic = error(
                "project.tasks.shutdown-timeout-too-large",
                "Task shutdown timeout exceeds its phase hard limit",
            );
            let diagnostic = with_field_path(diagnostic, &field_path);
            let diagnostic = with_public_u64(diagnostic, "actual", milliseconds);
            diagnostics.push(with_public_u64(
                diagnostic,
                "limit",
                u64::try_from(MAX_TASK_SHUTDOWN_PHASE_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
            ));
        }
    }

    let total_milliseconds = u128::from(shutdown.drain_timeout_ms)
        + u128::from(shutdown.cancel_timeout_ms)
        + u128::from(shutdown.join_timeout_ms);
    if total_milliseconds > MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT.as_millis() {
        let diagnostic = error(
            "project.tasks.shutdown-total-too-large",
            "Combined task shutdown timeout exceeds its hard limit",
        );
        let diagnostic = with_field_path(diagnostic, prefix);
        let actual = u64::try_from(total_milliseconds).unwrap_or(u64::MAX);
        let diagnostic = with_public_u64(diagnostic, "actual", actual);
        let diagnostic = with_public_bool(
            diagnostic,
            "actual_saturated",
            total_milliseconds > u128::from(u64::MAX),
        );
        diagnostics.push(with_public_u64(
            diagnostic,
            "limit",
            u64::try_from(MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
        ));
    }
}

fn lower_task_pool(
    field: &'static str,
    pool: ProjectTaskPoolManifest,
) -> Result<TaskKindConfig, RuntimeSettingsError> {
    let workers = usize::try_from(pool.workers)
        .ok()
        .and_then(ItemLimit::new)
        .ok_or(RuntimeSettingsError::InvalidTaskLimit { field })?;
    let pending = usize::try_from(pool.pending_capacity)
        .ok()
        .and_then(ItemLimit::new)
        .ok_or(RuntimeSettingsError::InvalidTaskLimit { field })?;
    Ok(TaskKindConfig::new(workers, pending))
}

fn lower_task_shutdown(
    shutdown: ProjectTaskShutdownManifest,
) -> Result<TaskShutdownPolicy, RuntimeSettingsError> {
    Ok(TaskShutdownPolicy::new(
        lower_timeout("tasks.shutdown.drain_timeout_ms", shutdown.drain_timeout_ms)?,
        lower_timeout(
            "tasks.shutdown.cancel_timeout_ms",
            shutdown.cancel_timeout_ms,
        )?,
        lower_timeout("tasks.shutdown.join_timeout_ms", shutdown.join_timeout_ms)?,
    ))
}

fn lower_timeout(
    field: &'static str,
    milliseconds: u64,
) -> Result<TimeLimit, RuntimeSettingsError> {
    TimeLimit::new(Duration::from_millis(milliseconds))
        .ok_or(RuntimeSettingsError::InvalidTaskLimit { field })
}

fn duration_millis_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
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
    /// Appends validation diagnostics at the supplied section path.
    pub fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        validate_window_fields(
            diagnostics,
            prefix,
            Some(&self.title),
            Some(self.width),
            Some(self.height),
            Some(self.scale_factor),
        );
    }
}

impl From<ProjectWindowManifest> for EffectiveWindowSettings {
    fn from(window: ProjectWindowManifest) -> Self {
        Self {
            enabled: window.enabled,
            title: window.title,
            width: window.width,
            height: window.height,
            scale_factor: window.scale_factor,
            mode: window.mode,
            present_mode: window.present_mode,
            resizable: window.resizable,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectWindowMode {
    #[default]
    Windowed,
    BorderlessFullscreen,
    Fullscreen,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectPresentMode {
    #[default]
    AutoVsync,
    AutoNoVsync,
    Fifo,
    Immediate,
    Mailbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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
    /// Appends validation diagnostics at the supplied section path.
    pub fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if self.runtime_capacity == 0 {
            let diagnostic = error(
                "project.diagnostics.invalid-runtime-capacity",
                "Runtime diagnostic capacity must be greater than zero",
            );
            let diagnostic = with_field_path(diagnostic, &format!("{prefix}.runtime_capacity"));
            diagnostics.push(with_public_u64(
                diagnostic,
                "actual",
                u64::try_from(self.runtime_capacity).unwrap_or(u64::MAX),
            ));
        }
        if self.runtime_capacity > MAX_RUNTIME_DIAGNOSTIC_ENTRIES {
            let diagnostic = error(
                "project.diagnostics.runtime-capacity-too-large",
                "Runtime diagnostic capacity exceeds its hard limit",
            );
            let diagnostic = with_field_path(diagnostic, &format!("{prefix}.runtime_capacity"));
            let diagnostic = with_public_u64(
                diagnostic,
                "actual",
                u64::try_from(self.runtime_capacity).unwrap_or(u64::MAX),
            );
            diagnostics.push(with_public_u64(
                diagnostic,
                "limit",
                u64::try_from(MAX_RUNTIME_DIAGNOSTIC_ENTRIES).unwrap_or(u64::MAX),
            ));
        }
    }
}

impl TryFrom<ProjectDiagnosticsManifest> for EffectiveDiagnosticsSettings {
    type Error = RuntimeSettingsError;

    fn try_from(diagnostics: ProjectDiagnosticsManifest) -> Result<Self, Self::Error> {
        let entry_limit = ItemLimit::new(diagnostics.runtime_capacity)
            .ok_or(RuntimeSettingsError::InvalidDiagnosticCapacity)?;
        let byte_limit = ByteLimit::new(PROJECT_RUNTIME_DIAGNOSTIC_BYTE_LIMIT)
            .ok_or(RuntimeSettingsError::InvalidDiagnosticCapacity)?;
        Ok(Self {
            runtime: RuntimeDiagnosticsSettings::new(entry_limit, byte_limit)?,
        })
    }
}
