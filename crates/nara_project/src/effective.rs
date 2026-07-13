use nara_app::{FixedCatchUpPolicy, FixedTime, RuntimeTimeSettings};
use nara_diagnostic::RuntimeDiagnosticsSettings;
use nara_ecs::Resource;
use nara_tasks::TaskPoolConfig;
use nara_window::{PresentMode, Window, WindowMode, WindowResolution};

use crate::capability::{ProductCapability, ProductCapabilitySet, RuntimePreset};
use crate::manifest::ProjectManifest;
use crate::path::ProjectPath;
use crate::profile::ProjectProfileError;
use crate::sections::{ProjectPathsManifest, ProjectProfileKind, ProjectStartupManifest};

#[derive(Debug, Clone, PartialEq, Resource)]
pub struct EffectiveProjectSettings {
    pub schema_version: u32,
    pub project: EffectiveProjectInfo,
    pub profile_name: Option<String>,
    pub runtime_preset: RuntimePreset,
    pub requested_capabilities: ProductCapabilitySet,
    pub paths: EffectiveProjectPaths,
    pub startup: EffectiveStartupSettings,
    pub runtime: EffectiveRuntimeSettings,
    pub tasks: EffectiveTaskSettings,
    pub window: EffectiveWindowSettings,
    pub input: EffectiveInputSettings,
    pub diagnostics: EffectiveDiagnosticsSettings,
}

impl EffectiveProjectSettings {
    pub(crate) fn from_manifest(manifest: &ProjectManifest) -> Result<Self, ProjectProfileError> {
        let mut settings = Self {
            schema_version: manifest.schema_version,
            project: EffectiveProjectInfo {
                name: manifest.project.name.clone(),
                stable_id: manifest.project.stable_id.clone(),
                version: manifest.project.version.clone(),
            },
            profile_name: None,
            runtime_preset: manifest.runtime.preset,
            requested_capabilities: manifest.capabilities.lower(),
            paths: EffectiveProjectPaths::from_manifest(manifest.paths.clone())?,
            startup: EffectiveStartupSettings::from_manifest(manifest.startup.clone())?,
            runtime: manifest.runtime.lower()?,
            tasks: manifest.tasks.lower()?,
            window: manifest.window.clone().lower(),
            input: manifest.input.clone().lower()?,
            diagnostics: manifest.diagnostics.lower()?,
        };
        settings.enforce_product_invariants();
        Ok(settings)
    }

    pub(crate) fn apply_profile_kind_defaults(&mut self, kind: ProjectProfileKind) {
        match kind {
            ProjectProfileKind::Headless => {
                self.runtime_preset = RuntimePreset::LocalHeadless;
                self.window.enabled = false;
            }
            ProjectProfileKind::Server => {
                self.runtime_preset = RuntimePreset::Server;
                self.window.enabled = false;
                self.runtime
                    .force_catch_up_policy(FixedCatchUpPolicy::PreserveDebt);
            }
            ProjectProfileKind::Editor => {
                self.requested_capabilities
                    .insert(ProductCapability::Tooling);
            }
            ProjectProfileKind::Dev | ProjectProfileKind::Release | ProjectProfileKind::Custom => {}
        }
    }

    pub(crate) fn enforce_profile_kind_invariants(&mut self, kind: ProjectProfileKind) {
        match kind {
            ProjectProfileKind::Headless => {
                self.runtime_preset = RuntimePreset::LocalHeadless;
                self.window.enabled = false;
            }
            ProjectProfileKind::Server => {
                self.runtime_preset = RuntimePreset::Server;
                self.window.enabled = false;
                self.runtime
                    .force_catch_up_policy(FixedCatchUpPolicy::PreserveDebt);
            }
            ProjectProfileKind::Editor => {
                self.requested_capabilities
                    .insert(ProductCapability::Tooling);
            }
            ProjectProfileKind::Dev | ProjectProfileKind::Release | ProjectProfileKind::Custom => {}
        }
    }

    pub(crate) fn enforce_product_invariants(&mut self) {
        if self.runtime_preset == RuntimePreset::Server {
            self.window.enabled = false;
            self.runtime
                .force_catch_up_policy(FixedCatchUpPolicy::PreserveDebt);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveProjectInfo {
    pub name: String,
    pub stable_id: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveProjectPaths {
    pub assets: ProjectPath,
    pub scenes: ProjectPath,
    pub prefabs: ProjectPath,
    pub scripts: ProjectPath,
    pub import_cache: ProjectPath,
}

impl EffectiveProjectPaths {
    pub(crate) fn from_manifest(paths: ProjectPathsManifest) -> Result<Self, ProjectProfileError> {
        Ok(Self {
            assets: ProjectPath::new(paths.assets)
                .map_err(ProjectProfileError::InvalidProjectPath)?,
            scenes: ProjectPath::new(paths.scenes)
                .map_err(ProjectProfileError::InvalidProjectPath)?,
            prefabs: ProjectPath::new(paths.prefabs)
                .map_err(ProjectProfileError::InvalidProjectPath)?,
            scripts: ProjectPath::new(paths.scripts)
                .map_err(ProjectProfileError::InvalidProjectPath)?,
            import_cache: ProjectPath::new(paths.import_cache)
                .map_err(ProjectProfileError::InvalidProjectPath)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveStartupSettings {
    pub default_scene: Option<ProjectPath>,
}

impl EffectiveStartupSettings {
    pub(crate) fn from_manifest(
        startup: ProjectStartupManifest,
    ) -> Result<Self, ProjectProfileError> {
        Ok(Self {
            default_scene: startup
                .default_scene
                .map(ProjectPath::new)
                .transpose()
                .map_err(ProjectProfileError::InvalidProjectPath)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectiveRuntimeSettings {
    runtime_time: RuntimeTimeSettings,
    fixed_time: FixedTime,
}

impl EffectiveRuntimeSettings {
    pub(crate) const fn new(runtime_time: RuntimeTimeSettings, fixed_time: FixedTime) -> Self {
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

    pub(crate) fn replace(&mut self, runtime_time: RuntimeTimeSettings, fixed_time: FixedTime) {
        self.runtime_time = runtime_time;
        self.fixed_time = fixed_time;
    }

    pub(crate) fn force_catch_up_policy(&mut self, policy: FixedCatchUpPolicy) {
        self.fixed_time = self.fixed_time.with_catch_up_policy(policy);
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
    pub mode: WindowMode,
    pub present_mode: PresentMode,
    pub resizable: bool,
}

impl EffectiveWindowSettings {
    #[must_use]
    pub fn to_window(&self) -> Option<Window> {
        if !self.enabled {
            return None;
        }

        let resolution =
            WindowResolution::new(self.width, self.height).with_scale_factor(self.scale_factor);
        let mut window = Window::new(self.title.clone(), resolution);
        window.mode = self.mode;
        window.present_mode = self.present_mode;
        window.resizable = self.resizable;
        Some(window)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveInputSettings {
    pub action_map: Option<ProjectPath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveDiagnosticsSettings {
    pub runtime: RuntimeDiagnosticsSettings,
}
