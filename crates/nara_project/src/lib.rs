//! Project manifest parsing, validation, and effective settings lowering.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
    fs,
    path::Path,
    time::Duration,
};

use nara_app::{FixedTime, RuntimeTimeSettings};
use nara_diagnostic::{Diagnostic, DiagnosticReport, RuntimeDiagnosticsSettings};
use nara_ecs::Resource;
use nara_tasks::TaskPoolConfig;
use nara_window::{PresentMode, Window, WindowMode, WindowResolution};
use serde::Deserialize;
use thiserror::Error;

pub const CURRENT_PROJECT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MANIFEST_BYTE_LIMIT: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPath(String);

impl ProjectPath {
    pub fn new(path: impl Into<String>) -> Result<Self, ProjectPathError> {
        let path = path.into();
        validate_project_path(&path)?;
        Ok(Self(path))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ProjectPath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectPathError {
    Empty,
    Absolute,
    ContainsBackslash,
    ContainsDrivePrefix,
    ContainsNull,
    ContainsEmptySegment,
    ContainsCurrentDirectory,
    ContainsParentDirectory,
}

impl Display for ProjectPathError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("project path cannot be empty"),
            Self::Absolute => formatter.write_str("project path must be relative"),
            Self::ContainsBackslash => formatter.write_str("project path must use '/' separators"),
            Self::ContainsDrivePrefix => {
                formatter.write_str("project path must not contain a drive prefix")
            }
            Self::ContainsNull => formatter.write_str("project path must not contain null bytes"),
            Self::ContainsEmptySegment => {
                formatter.write_str("project path must not contain empty segments")
            }
            Self::ContainsCurrentDirectory => {
                formatter.write_str("project path must not contain '.' segments")
            }
            Self::ContainsParentDirectory => {
                formatter.write_str("project path must not contain '..' segments")
            }
        }
    }
}

impl Error for ProjectPathError {}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectManifestLoad {
    pub manifest: Option<ProjectManifest>,
    pub diagnostics: DiagnosticReport,
}

impl ProjectManifestLoad {
    #[must_use]
    pub fn ok(manifest: ProjectManifest) -> Self {
        let diagnostics = manifest.validate();
        Self {
            manifest: Some(manifest),
            diagnostics,
        }
    }

    #[must_use]
    pub const fn failed(diagnostics: DiagnosticReport) -> Self {
        Self {
            manifest: None,
            diagnostics,
        }
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }
}

#[derive(Debug, Error)]
pub enum ProjectManifestFileError {
    #[error("failed to read project manifest metadata: {0}")]
    Metadata(std::io::Error),
    #[error("project manifest is too large: {actual_bytes} bytes > {limit_bytes} bytes")]
    TooLarge { actual_bytes: u64, limit_bytes: u64 },
    #[error("failed to read project manifest: {0}")]
    Read(std::io::Error),
}

impl ProjectManifestFileError {
    #[must_use]
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            Self::Metadata(error) => Diagnostic::error(
                "project.manifest.metadata",
                format!("failed to read project manifest metadata: {error}"),
            ),
            Self::TooLarge {
                actual_bytes,
                limit_bytes,
            } => Diagnostic::error(
                "project.manifest.too-large",
                format!(
                    "project manifest is too large: {actual_bytes} bytes > {limit_bytes} bytes"
                ),
            ),
            Self::Read(error) => Diagnostic::error(
                "project.manifest.read",
                format!("failed to read project manifest: {error}"),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    pub schema_version: u32,
    pub project: ProjectInfo,
    #[serde(default)]
    pub paths: ProjectPathsManifest,
    #[serde(default)]
    pub startup: ProjectStartupManifest,
    #[serde(default)]
    pub runtime: ProjectRuntimeManifest,
    #[serde(default)]
    pub tasks: ProjectTasksManifest,
    #[serde(default)]
    pub window: ProjectWindowManifest,
    #[serde(default)]
    pub input: ProjectInputManifest,
    #[serde(default)]
    pub diagnostics: ProjectDiagnosticsManifest,
    #[serde(default)]
    pub profiles: BTreeMap<String, ProjectProfileOverlay>,
}

impl ProjectManifest {
    #[must_use]
    pub fn parse_toml_str(source: &str) -> ProjectManifestLoad {
        match toml::from_str::<Self>(source) {
            Ok(manifest) => ProjectManifestLoad::ok(manifest),
            Err(error) => {
                let mut diagnostics = DiagnosticReport::default();
                diagnostics.push(Diagnostic::error(
                    "project.manifest.parse",
                    format!("failed to parse nara.toml: {error}"),
                ));
                ProjectManifestLoad::failed(diagnostics)
            }
        }
    }

    #[must_use]
    pub fn parse_toml_file(path: impl AsRef<Path>) -> ProjectManifestLoad {
        Self::parse_toml_file_with_limit(path, DEFAULT_MANIFEST_BYTE_LIMIT)
    }

    #[must_use]
    pub fn parse_toml_file_with_limit(
        path: impl AsRef<Path>,
        limit_bytes: u64,
    ) -> ProjectManifestLoad {
        match read_manifest_to_string(path.as_ref(), limit_bytes) {
            Ok(source) => Self::parse_toml_str(&source),
            Err(error) => {
                let mut diagnostics = DiagnosticReport::default();
                diagnostics.push(error.to_diagnostic());
                ProjectManifestLoad::failed(diagnostics)
            }
        }
    }

    #[must_use]
    pub fn validate(&self) -> DiagnosticReport {
        let mut diagnostics = DiagnosticReport::default();

        if self.schema_version != CURRENT_PROJECT_SCHEMA_VERSION {
            diagnostics.push(
                Diagnostic::error(
                    "project.manifest.unsupported-schema",
                    format!(
                        "unsupported project schema version {}, expected {}",
                        self.schema_version, CURRENT_PROJECT_SCHEMA_VERSION
                    ),
                )
                .with_field_path("schema_version"),
            );
        }

        if self.project.name.trim().is_empty() {
            diagnostics.push(
                Diagnostic::error("project.name.empty", "project.name cannot be empty")
                    .with_field_path("project.name"),
            );
        }

        if self
            .project
            .name
            .chars()
            .any(|character| character.is_control())
        {
            diagnostics.push(
                Diagnostic::error(
                    "project.name.control-character",
                    "project.name cannot contain control characters",
                )
                .with_field_path("project.name"),
            );
        }

        validate_path_field(&mut diagnostics, "paths.assets", &self.paths.assets);
        validate_path_field(&mut diagnostics, "paths.scenes", &self.paths.scenes);
        validate_path_field(&mut diagnostics, "paths.prefabs", &self.paths.prefabs);
        validate_path_field(&mut diagnostics, "paths.scripts", &self.paths.scripts);
        validate_path_field(
            &mut diagnostics,
            "paths.import_cache",
            &self.paths.import_cache,
        );

        if let Some(scene) = &self.startup.default_scene {
            validate_path_field(&mut diagnostics, "startup.default_scene", scene);
        }

        self.runtime.validate_into(&mut diagnostics, "runtime");
        self.tasks.validate_into(&mut diagnostics, "tasks");
        self.window.validate_into(&mut diagnostics, "window");
        self.input.validate_into(&mut diagnostics, "input");
        self.diagnostics
            .validate_into(&mut diagnostics, "diagnostics");

        for (name, profile) in &self.profiles {
            validate_profile_name(&mut diagnostics, name);
            profile.validate_into(&mut diagnostics, &format!("profiles.{name}"));
        }

        diagnostics
    }

    pub fn resolve_profile(
        &self,
        profile: Option<&str>,
    ) -> Result<EffectiveProjectSettings, ProjectProfileError> {
        let diagnostics = self.validate();
        if diagnostics.has_errors() {
            return Err(ProjectProfileError::InvalidManifest { diagnostics });
        }

        let mut settings = EffectiveProjectSettings::from_manifest(self)?;
        let Some(profile_name) = profile else {
            return Ok(settings);
        };
        let Some(overlay) = self.profiles.get(profile_name) else {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(
                Diagnostic::error(
                    "project.profile.unknown",
                    format!("unknown project profile '{profile_name}'"),
                )
                .with_field_path(format!("profiles.{profile_name}")),
            );
            return Err(ProjectProfileError::UnknownProfile {
                profile: profile_name.to_owned(),
                diagnostics,
            });
        };

        settings.profile_name = Some(profile_name.to_owned());
        settings.apply_profile_kind_defaults(ProjectProfileKind::from_profile_name(profile_name));
        overlay.apply_to(&mut settings)?;
        Ok(settings)
    }
}

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
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
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

    fn lower(self) -> EffectiveRuntimeSettings {
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
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
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
    }

    fn lower(self) -> EffectiveTaskSettings {
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
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if self.enabled {
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
    }

    fn lower(self) -> EffectiveWindowSettings {
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
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if let Some(action_map) = &self.action_map {
            validate_path_field(diagnostics, &format!("{prefix}.action_map"), action_map);
        }
    }

    fn lower(self) -> Result<EffectiveInputSettings, ProjectProfileError> {
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
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if self.runtime_capacity == 0 {
            diagnostics.push(
                Diagnostic::error(
                    "project.diagnostics.invalid-runtime-capacity",
                    "diagnostics.runtime_capacity must be greater than zero",
                )
                .with_field_path(format!("{prefix}.runtime_capacity")),
            );
        }
    }

    const fn lower(self) -> EffectiveDiagnosticsSettings {
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

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ProjectProfileOverlay {
    #[serde(default)]
    pub plugin_plan: Option<ProjectPluginPlan>,
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
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
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

    fn apply_to(&self, settings: &mut EffectiveProjectSettings) -> Result<(), ProjectProfileError> {
        if let Some(plugin_plan) = self.plugin_plan {
            settings.plugin_plan = plugin_plan;
        }
        self.paths.apply_to(&mut settings.paths)?;
        self.startup.apply_to(&mut settings.startup)?;
        self.runtime.apply_to(&mut settings.runtime);
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
    pub default_scene: Option<Option<String>>,
}

impl ProjectStartupPatch {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if let Some(Some(scene)) = &self.default_scene {
            validate_path_field(diagnostics, &format!("{prefix}.default_scene"), scene);
        }
    }

    fn apply_to(&self, startup: &mut EffectiveStartupSettings) -> Result<(), ProjectProfileError> {
        if let Some(default_scene) = &self.default_scene {
            startup.default_scene = default_scene
                .clone()
                .map(ProjectPath::new)
                .transpose()
                .map_err(ProjectProfileError::InvalidProjectPath)?;
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
        validate_optional_thread_count(
            diagnostics,
            &format!("{prefix}.io_threads"),
            self.io_threads,
        );
        validate_optional_thread_count(
            diagnostics,
            &format!("{prefix}.compute_threads"),
            self.compute_threads,
        );
        validate_optional_thread_count(
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
    pub action_map: Option<Option<String>>,
}

impl ProjectInputPatch {
    fn validate_into(&self, diagnostics: &mut DiagnosticReport, prefix: &str) {
        if let Some(Some(action_map)) = &self.action_map {
            validate_path_field(diagnostics, &format!("{prefix}.action_map"), action_map);
        }
    }

    fn apply_to(&self, input: &mut EffectiveInputSettings) -> Result<(), ProjectProfileError> {
        if let Some(action_map) = &self.action_map {
            input.action_map = action_map
                .clone()
                .map(ProjectPath::new)
                .transpose()
                .map_err(ProjectProfileError::InvalidProjectPath)?;
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
    }

    fn apply_to(&self, diagnostics: &mut EffectiveDiagnosticsSettings) {
        if let Some(capacity) = self.runtime_capacity {
            diagnostics.runtime = RuntimeDiagnosticsSettings { capacity };
        }
    }
}

#[derive(Debug, Clone, PartialEq, Resource)]
pub struct EffectiveProjectSettings {
    pub schema_version: u32,
    pub project: EffectiveProjectInfo,
    pub profile_name: Option<String>,
    pub plugin_plan: ProjectPluginPlan,
    pub paths: EffectiveProjectPaths,
    pub startup: EffectiveStartupSettings,
    pub runtime: EffectiveRuntimeSettings,
    pub tasks: EffectiveTaskSettings,
    pub window: EffectiveWindowSettings,
    pub input: EffectiveInputSettings,
    pub diagnostics: EffectiveDiagnosticsSettings,
}

impl EffectiveProjectSettings {
    fn from_manifest(manifest: &ProjectManifest) -> Result<Self, ProjectProfileError> {
        Ok(Self {
            schema_version: manifest.schema_version,
            project: EffectiveProjectInfo {
                name: manifest.project.name.clone(),
                stable_id: manifest.project.stable_id.clone(),
                version: manifest.project.version.clone(),
            },
            profile_name: None,
            plugin_plan: manifest.runtime.plugin_plan,
            paths: EffectiveProjectPaths::from_manifest(manifest.paths.clone())?,
            startup: EffectiveStartupSettings::from_manifest(manifest.startup.clone())?,
            runtime: manifest.runtime.lower(),
            tasks: manifest.tasks.lower(),
            window: manifest.window.clone().lower(),
            input: manifest.input.clone().lower()?,
            diagnostics: manifest.diagnostics.lower(),
        })
    }

    fn apply_profile_kind_defaults(&mut self, kind: ProjectProfileKind) {
        match kind {
            ProjectProfileKind::Headless => {
                self.plugin_plan = ProjectPluginPlan::HeadlessRuntime;
                self.window.enabled = false;
            }
            ProjectProfileKind::Server => {
                self.plugin_plan = ProjectPluginPlan::Server;
                self.window.enabled = false;
                self.tasks.mode = ProjectTaskExecutionMode::Deterministic;
                self.tasks.pool_config = TaskPoolConfig::deterministic();
            }
            ProjectProfileKind::Editor => {
                self.plugin_plan = ProjectPluginPlan::Tooling;
            }
            ProjectProfileKind::Dev | ProjectProfileKind::Release | ProjectProfileKind::Custom => {}
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
    fn from_manifest(paths: ProjectPathsManifest) -> Result<Self, ProjectProfileError> {
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
    fn from_manifest(startup: ProjectStartupManifest) -> Result<Self, ProjectProfileError> {
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
    pub paused: bool,
    pub time_scale: f32,
    pub max_delta: Duration,
    pub fixed_timestep: Duration,
    pub max_fixed_steps_per_frame: u32,
}

impl EffectiveRuntimeSettings {
    #[must_use]
    pub fn runtime_time_settings(&self) -> RuntimeTimeSettings {
        RuntimeTimeSettings::default()
            .with_paused(self.paused)
            .with_time_scale(self.time_scale)
            .with_max_delta(self.max_delta)
    }

    #[must_use]
    pub fn fixed_time(&self) -> FixedTime {
        FixedTime::new(self.fixed_timestep).with_max_steps_per_frame(self.max_fixed_steps_per_frame)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveTaskSettings {
    pub mode: ProjectTaskExecutionMode,
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

fn read_manifest_to_string(
    path: &Path,
    limit_bytes: u64,
) -> Result<String, ProjectManifestFileError> {
    let metadata = fs::metadata(path).map_err(ProjectManifestFileError::Metadata)?;
    let actual_bytes = metadata.len();
    if actual_bytes > limit_bytes {
        return Err(ProjectManifestFileError::TooLarge {
            actual_bytes,
            limit_bytes,
        });
    }
    fs::read_to_string(path).map_err(ProjectManifestFileError::Read)
}

fn validate_profile_name(diagnostics: &mut DiagnosticReport, name: &str) {
    if name.is_empty() {
        diagnostics.push(Diagnostic::error(
            "project.profile.empty-name",
            "profile names cannot be empty",
        ));
        return;
    }

    if name.chars().any(|character| {
        !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    }) {
        diagnostics.push(
            Diagnostic::error(
                "project.profile.invalid-name",
                format!(
                    "profile name '{name}' can only contain ASCII letters, numbers, '-', '_', or '.'"
                ),
            )
            .with_field_path(format!("profiles.{name}")),
        );
    }
}

fn validate_path_field(diagnostics: &mut DiagnosticReport, field_path: &str, path: &str) {
    if let Err(error) = ProjectPath::new(path.to_owned()) {
        diagnostics.push(
            Diagnostic::error(
                "project.path.invalid",
                format!("invalid project path '{path}': {error}"),
            )
            .with_field_path(field_path),
        );
    }
}

fn validate_optional_path_field(
    diagnostics: &mut DiagnosticReport,
    field_path: &str,
    path: Option<&String>,
) {
    if let Some(path) = path {
        validate_path_field(diagnostics, field_path, path);
    }
}

fn validate_positive_seconds(diagnostics: &mut DiagnosticReport, field_path: &str, value: f64) {
    if !value.is_finite() || value <= 0.0 {
        diagnostics.push(
            Diagnostic::error(
                "project.runtime.invalid-duration",
                format!("{field_path} must be finite and greater than zero"),
            )
            .with_field_path(field_path),
        );
    }
}

fn validate_thread_count(diagnostics: &mut DiagnosticReport, field_path: &str, value: usize) {
    if value == 0 {
        diagnostics.push(
            Diagnostic::error(
                "project.tasks.invalid-thread-count",
                format!("{field_path} must be greater than zero in threaded mode"),
            )
            .with_field_path(field_path),
        );
    }
}

fn validate_optional_thread_count(
    diagnostics: &mut DiagnosticReport,
    field_path: &str,
    value: Option<usize>,
) {
    if matches!(value, Some(0)) {
        validate_thread_count(diagnostics, field_path, 0);
    }
}

fn validate_project_path(path: &str) -> Result<(), ProjectPathError> {
    if path.is_empty() {
        return Err(ProjectPathError::Empty);
    }
    if path.starts_with('/') {
        return Err(ProjectPathError::Absolute);
    }
    if path.contains('\\') {
        return Err(ProjectPathError::ContainsBackslash);
    }
    if path.as_bytes().get(1) == Some(&b':') {
        return Err(ProjectPathError::ContainsDrivePrefix);
    }
    if path.contains('\0') {
        return Err(ProjectPathError::ContainsNull);
    }

    for segment in path.split('/') {
        match segment {
            "" => return Err(ProjectPathError::ContainsEmptySegment),
            "." => return Err(ProjectPathError::ContainsCurrentDirectory),
            ".." => return Err(ProjectPathError::ContainsParentDirectory),
            _ => {}
        }
    }

    Ok(())
}

fn default_assets_path() -> String {
    "assets".to_owned()
}

fn default_scenes_path() -> String {
    "scenes".to_owned()
}

fn default_prefabs_path() -> String {
    "prefabs".to_owned()
}

fn default_scripts_path() -> String {
    "scripts".to_owned()
}

fn default_import_cache_path() -> String {
    ".nara/import-cache".to_owned()
}

const fn default_time_scale() -> f32 {
    1.0
}

const fn default_max_delta_seconds() -> f64 {
    0.25
}

const fn default_fixed_timestep_seconds() -> f64 {
    1.0 / 60.0
}

const fn default_max_fixed_steps_per_frame() -> u32 {
    5
}

const fn default_io_threads() -> usize {
    2
}

fn default_compute_threads() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

fn default_async_compute_threads() -> usize {
    (default_compute_threads() / 2).max(1)
}

const fn default_window_enabled() -> bool {
    true
}

fn default_window_title() -> String {
    "nara".to_owned()
}

const fn default_window_width() -> u32 {
    1280
}

const fn default_window_height() -> u32 {
    720
}

const fn default_window_scale_factor() -> f64 {
    1.0
}

const fn default_window_resizable() -> bool {
    true
}

const fn default_diagnostics_capacity() -> usize {
    256
}

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
mod tests {
    use super::*;
    use nara_tasks::{TaskExecutionMode, TaskPoolKind};

    const MINIMAL_MANIFEST: &str = r#"
schema_version = 1

[project]
name = "Test Game"
"#;

    #[test]
    fn minimal_manifest_parses_and_resolves_defaults() {
        let load = ProjectManifest::parse_toml_str(MINIMAL_MANIFEST);

        assert!(!load.has_errors());
        let manifest = load.manifest.unwrap();
        let settings = manifest.resolve_profile(None).unwrap();

        assert_eq!(settings.project.name, "Test Game");
        assert_eq!(settings.paths.assets.as_str(), "assets");
        assert_eq!(settings.paths.import_cache.as_str(), ".nara/import-cache");
        assert_eq!(settings.plugin_plan, ProjectPluginPlan::Minimal);
        assert_eq!(settings.runtime.fixed_time().max_steps_per_frame(), 5);
        assert!(settings.window.to_window().is_some());
    }

    #[test]
    fn unknown_manifest_fields_are_rejected() {
        let load = ProjectManifest::parse_toml_str(
            r#"
schema_version = 1
unexpected = true

[project]
name = "Bad"
"#,
        );

        assert!(load.manifest.is_none());
        assert!(load.has_errors());
        assert_eq!(
            load.diagnostics.diagnostics()[0].code.as_str(),
            "project.manifest.parse"
        );
    }

    #[test]
    fn invalid_paths_produce_structured_diagnostics() {
        let load = ProjectManifest::parse_toml_str(
            r#"
schema_version = 1

[project]
name = "Bad Paths"

[paths]
assets = "../assets"
"#,
        );

        assert!(load.has_errors());
        let diagnostics = load.diagnostics.diagnostics();
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_str() == "project.path.invalid"
                && diagnostic.context.field_path.as_deref() == Some("paths.assets")
        }));
    }

    #[test]
    fn server_profile_infers_headless_plugin_and_deterministic_tasks() {
        let load = ProjectManifest::parse_toml_str(
            r#"
schema_version = 1

[project]
name = "Server Game"

[profiles.server]
"#,
        );
        let manifest = load.manifest.unwrap();

        let settings = manifest.resolve_profile(Some("server")).unwrap();

        assert_eq!(settings.plugin_plan, ProjectPluginPlan::Server);
        assert!(!settings.window.enabled);
        assert_eq!(settings.tasks.mode, ProjectTaskExecutionMode::Deterministic);
        assert_eq!(
            settings.tasks.pool_config.execution_mode(),
            TaskExecutionMode::Deterministic
        );
        assert_eq!(settings.tasks.pool_config.threads_for(TaskPoolKind::Io), 0);
    }

    #[test]
    fn profile_overlay_overrides_effective_values() {
        let load = ProjectManifest::parse_toml_str(
            r#"
schema_version = 1

[project]
name = "Overlay Game"

[runtime]
time_scale = 1.0
plugin_plan = "runtime-2d"

[profiles.dev.runtime]
time_scale = 0.5
max_fixed_steps_per_frame = 2

[profiles.dev.tasks]
mode = "threaded"
io_threads = 3
compute_threads = 4
async_compute_threads = 5

[profiles.dev.window]
title = "Dev Window"
width = 640
height = 480

[profiles.dev.diagnostics]
runtime_capacity = 32
"#,
        );
        let manifest = load.manifest.unwrap();

        let settings = manifest.resolve_profile(Some("dev")).unwrap();

        assert_eq!(settings.plugin_plan, ProjectPluginPlan::Runtime2d);
        assert_eq!(settings.runtime.time_scale, 0.5);
        assert_eq!(settings.runtime.fixed_time().max_steps_per_frame(), 2);
        assert_eq!(settings.window.title, "Dev Window");
        assert_eq!(settings.window.width, 640);
        assert_eq!(settings.window.height, 480);
        assert_eq!(settings.diagnostics.runtime.capacity, 32);
        assert_eq!(settings.tasks.pool_config.threads_for(TaskPoolKind::Io), 3);
        assert_eq!(
            settings
                .tasks
                .pool_config
                .threads_for(TaskPoolKind::Compute),
            4
        );
        assert_eq!(
            settings
                .tasks
                .pool_config
                .threads_for(TaskPoolKind::AsyncCompute),
            5
        );
    }

    #[test]
    fn resolving_unknown_profile_returns_diagnostic_error() {
        let manifest = ProjectManifest::parse_toml_str(MINIMAL_MANIFEST)
            .manifest
            .unwrap();

        let error = manifest.resolve_profile(Some("missing")).unwrap_err();

        let ProjectProfileError::UnknownProfile {
            profile,
            diagnostics,
        } = error
        else {
            panic!("expected unknown profile error");
        };
        assert_eq!(profile, "missing");
        assert_eq!(
            diagnostics.diagnostics()[0].code.as_str(),
            "project.profile.unknown"
        );
    }

    #[test]
    fn file_loader_enforces_manifest_size_budget() {
        let temp_root = std::env::temp_dir().join(format!(
            "nara_project_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_root).unwrap();
        let manifest_path = temp_root.join("nara.toml");
        fs::write(&manifest_path, MINIMAL_MANIFEST).unwrap();

        let load = ProjectManifest::parse_toml_file_with_limit(&manifest_path, 4);

        assert!(load.manifest.is_none());
        assert_eq!(
            load.diagnostics.diagnostics()[0].code.as_str(),
            "project.manifest.too-large"
        );

        fs::remove_dir_all(&temp_root).unwrap();
    }
}
