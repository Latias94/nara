use nara_app::{FixedCatchUpPolicy, FixedTime, RuntimeTimeSettings};
use nara_core::SourceUniverseId;
use nara_diagnostic::RuntimeDiagnosticsSettings;
use nara_ecs::Resource;
use nara_tasks::TaskPoolConfig;
use nara_window::{PresentMode, Window, WindowMode, WindowResolution};

use crate::capability::{ProductCapabilitySet, RuntimePreset};
use crate::content::EffectiveProjectContentSettings;
use crate::manifest::ProjectManifest;
use crate::path::ProjectPath;
use crate::profile::ProjectProfileError;
use crate::render::EffectiveRenderSettings;
use crate::sections::{
    NativeDocumentIdentity, ProjectPathsManifest, ProjectProfileKind, ProjectStartupManifest,
    ProjectUiManifest, ProjectUiSafeAreaManifest, ProjectUiSurfaceManifest,
};

#[derive(Debug, Clone, PartialEq, Resource)]
pub struct EffectiveProjectSettings {
    pub schema_version: u32,
    pub project: EffectiveProjectInfo,
    pub profile_name: Option<String>,
    pub runtime_preset: RuntimePreset,
    pub requested_capabilities: ProductCapabilitySet,
    pub paths: EffectiveProjectPaths,
    /// Project-wide authoring schema bundle source. Profiles cannot replace this path.
    pub authoring_schema_bundle: Option<ProjectPath>,
    pub content: EffectiveProjectContentSettings,
    pub startup: EffectiveStartupSettings,
    pub runtime: EffectiveRuntimeSettings,
    pub tasks: EffectiveTaskSettings,
    pub render: EffectiveRenderSettings,
    pub ui: EffectiveUiSettings,
    pub window: EffectiveWindowSettings,
    pub input: EffectiveInputSettings,
    pub diagnostics: EffectiveDiagnosticsSettings,
    pub execution: EffectiveProjectExecutionSettings,
}

impl EffectiveProjectSettings {
    // Resolve profile overlays before enforcing rules derived from the final runtime preset.
    pub(crate) fn from_manifest(manifest: &ProjectManifest) -> Result<Self, ProjectProfileError> {
        Ok(Self {
            schema_version: manifest.schema_version,
            project: EffectiveProjectInfo {
                name: manifest.project.name.clone(),
                source_universe: manifest.project.source_universe,
                native_document_identity: manifest.project.native_document_identity,
                stable_id: manifest.project.stable_id.clone(),
                version: manifest.project.version.clone(),
            },
            profile_name: None,
            runtime_preset: manifest.runtime.preset,
            requested_capabilities: manifest.capabilities.lower(),
            paths: EffectiveProjectPaths::from_manifest(manifest.paths.clone())?,
            authoring_schema_bundle: manifest
                .authoring
                .schema_bundle
                .clone()
                .map(ProjectPath::new)
                .transpose()
                .map_err(ProjectProfileError::InvalidProjectPath)?,
            content: EffectiveProjectContentSettings::from_manifest(&manifest.content)?,
            startup: EffectiveStartupSettings::from_manifest(manifest.startup.clone())?,
            runtime: manifest.runtime.lower()?,
            tasks: manifest.tasks.lower()?,
            render: manifest.render.into(),
            ui: EffectiveUiSettings::from_manifest(manifest.ui.clone()),
            window: manifest.window.clone().lower(),
            input: manifest.input.clone().lower()?,
            diagnostics: manifest.diagnostics.lower()?,
            execution: EffectiveProjectExecutionSettings::default(),
        })
    }

    pub(crate) fn apply_profile_kind_defaults(&mut self, kind: ProjectProfileKind) {
        match kind {
            ProjectProfileKind::Headless => {
                self.runtime_preset = RuntimePreset::LocalHeadless;
            }
            ProjectProfileKind::Server => {
                self.runtime_preset = RuntimePreset::Server;
            }
            ProjectProfileKind::Editor
            | ProjectProfileKind::Dev
            | ProjectProfileKind::Release
            | ProjectProfileKind::Custom => {}
        }
    }

    pub(crate) fn enforce_profile_kind_invariants(&mut self, kind: ProjectProfileKind) {
        self.apply_profile_kind_defaults(kind);
        if kind == ProjectProfileKind::Headless {
            self.window.enabled = false;
        }
    }

    pub(crate) fn enforce_product_invariants(&mut self) {
        enforce_runtime_preset_invariants(self.runtime_preset, &mut self.runtime, &mut self.window);
    }
}

pub(crate) fn enforce_runtime_preset_invariants(
    preset: RuntimePreset,
    runtime: &mut EffectiveRuntimeSettings,
    window: &mut EffectiveWindowSettings,
) {
    if preset == RuntimePreset::Server {
        window.enabled = false;
        runtime.force_catch_up_policy(FixedCatchUpPolicy::PreserveDebt);
    }
}

pub const DESKTOP_SUPERVISED_PLAY_V1_CONTRACT: &str = "desktop-supervised-play/v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectExecutionContract {
    DesktopSupervisedPlayV1,
    Unsupported(String),
}

impl ProjectExecutionContract {
    pub fn as_str(&self) -> &str {
        match self {
            Self::DesktopSupervisedPlayV1 => DESKTOP_SUPERVISED_PLAY_V1_CONTRACT,
            Self::Unsupported(identifier) => identifier,
        }
    }

    pub fn is_desktop_supervised_play_v1(&self) -> bool {
        matches!(self, Self::DesktopSupervisedPlayV1)
    }

    pub(crate) fn from_identifier(identifier: &str) -> Self {
        if identifier == DESKTOP_SUPERVISED_PLAY_V1_CONTRACT {
            Self::DesktopSupervisedPlayV1
        } else {
            Self::Unsupported(identifier.to_owned())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectiveProjectExecutionSettings {
    pub contract: Option<ProjectExecutionContract>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveUiSettings {
    pub primary_surface: Option<EffectivePrimaryUiSurface>,
}

impl EffectiveUiSettings {
    pub(crate) fn from_manifest(ui: ProjectUiManifest) -> Self {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveProjectInfo {
    pub name: String,
    pub source_universe: SourceUniverseId,
    pub native_document_identity: NativeDocumentIdentity,
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
