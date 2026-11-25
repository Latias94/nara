use nara_app::tasks::TaskConfigError;
use nara_app::tasks::TaskPoolConfig;
use nara_app::{FixedCatchUpPolicy, FixedTime, RuntimeTimeSettings, TimeSettingsError};
use nara_core::diagnostic::{DiagnosticSettingsError, RuntimeDiagnosticsSettings};
use thiserror::Error;

use super::{
    ProjectPresentMode, ProjectUiManifest, ProjectUiSafeAreaManifest, ProjectUiSurfaceManifest,
    ProjectWindowMode, RuntimePreset,
};

/// Applies the runtime rules for the selected preset after profile resolution.
pub fn enforce_runtime_preset_invariants(
    preset: RuntimePreset,
    runtime: &mut EffectiveRuntimeSettings,
    window: &mut EffectiveWindowSettings,
) {
    if preset == RuntimePreset::Server {
        window.enabled = false;
        runtime.fixed_time = runtime
            .fixed_time
            .with_catch_up_policy(FixedCatchUpPolicy::PreserveDebt);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveUiSettings {
    pub primary_surface: Option<EffectivePrimaryUiSurface>,
}

impl From<ProjectUiManifest> for EffectiveUiSettings {
    fn from(ui: ProjectUiManifest) -> Self {
        Self {
            primary_surface: ui.primary_surface.map(EffectivePrimaryUiSurface::from),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectivePrimaryUiSurface {
    pub logical_width: u32,
    pub logical_height: u32,
    pub safe_area: EffectiveUiSafeArea,
}

impl From<ProjectUiSurfaceManifest> for EffectivePrimaryUiSurface {
    fn from(surface: ProjectUiSurfaceManifest) -> Self {
        Self {
            logical_width: surface.logical_width,
            logical_height: surface.logical_height,
            safe_area: surface.safe_area.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EffectiveUiSafeArea {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl From<ProjectUiSafeAreaManifest> for EffectiveUiSafeArea {
    fn from(safe_area: ProjectUiSafeAreaManifest) -> Self {
        Self {
            left: safe_area.left,
            top: safe_area.top,
            right: safe_area.right,
            bottom: safe_area.bottom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectiveRuntimeSettings {
    runtime_time: RuntimeTimeSettings,
    fixed_time: FixedTime,
}

impl EffectiveRuntimeSettings {
    #[must_use]
    pub const fn new(runtime_time: RuntimeTimeSettings, fixed_time: FixedTime) -> Self {
        Self {
            runtime_time,
            fixed_time,
        }
    }

    #[must_use]
    pub const fn runtime_time_settings(&self) -> RuntimeTimeSettings {
        self.runtime_time
    }

    #[must_use]
    pub const fn fixed_time(&self) -> FixedTime {
        self.fixed_time
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveTaskSettings {
    pub pool_config: TaskPoolConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectiveWindowSettings {
    pub enabled: bool,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub mode: ProjectWindowMode,
    pub present_mode: ProjectPresentMode,
    pub resizable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveDiagnosticsSettings {
    pub runtime: RuntimeDiagnosticsSettings,
}

/// Failure to lower a v0 configuration section into engine settings.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum RuntimeSettingsError {
    #[error("invalid runtime settings: {0}")]
    InvalidRuntimeSettings(#[from] TimeSettingsError),
    #[error("task field '{field}' must be non-zero and representable")]
    InvalidTaskLimit { field: &'static str },
    #[error("invalid task settings: {0}")]
    InvalidTaskSettings(#[from] TaskConfigError),
    #[error("diagnostic capacity must be non-zero and representable")]
    InvalidDiagnosticCapacity,
    #[error("invalid diagnostic settings: {0}")]
    InvalidDiagnosticSettings(#[from] DiagnosticSettingsError),
}
