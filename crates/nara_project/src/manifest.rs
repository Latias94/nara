use std::{collections::BTreeMap, fs::File, io::Read, path::Path};

use nara_diagnostic::{Diagnostic, DiagnosticReport};
use serde::Deserialize;
use thiserror::Error;

use crate::effective::EffectiveProjectSettings;
use crate::profile::{ProjectProfileError, ProjectProfileOverlay};
use crate::sections::{
    ProjectDiagnosticsManifest, ProjectInfo, ProjectInputManifest, ProjectPathsManifest,
    ProjectProfileKind, ProjectRuntimeManifest, ProjectStartupManifest, ProjectTasksManifest,
    ProjectWindowManifest,
};
use crate::validation::{validate_path_field, validate_profile_name};
use crate::{CURRENT_PROJECT_SCHEMA_VERSION, DEFAULT_MANIFEST_BYTE_LIMIT};

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

        let profile_kind = ProjectProfileKind::from_profile_name(profile_name);
        settings.profile_name = Some(profile_name.to_owned());
        settings.apply_profile_kind_defaults(profile_kind);
        overlay.apply_to(&mut settings)?;
        settings.enforce_profile_kind_invariants(profile_kind);
        Ok(settings)
    }
}

fn read_manifest_to_string(
    path: &Path,
    limit_bytes: u64,
) -> Result<String, ProjectManifestFileError> {
    let mut file = File::open(path).map_err(ProjectManifestFileError::Read)?;
    let metadata = file
        .metadata()
        .map_err(ProjectManifestFileError::Metadata)?;
    let actual_bytes = metadata.len();
    if actual_bytes > limit_bytes {
        return Err(ProjectManifestFileError::TooLarge {
            actual_bytes,
            limit_bytes,
        });
    }

    let mut buffer = Vec::new();
    file.by_ref()
        .take(limit_bytes.saturating_add(1))
        .read_to_end(&mut buffer)
        .map_err(ProjectManifestFileError::Read)?;
    if buffer.len() as u64 > limit_bytes {
        return Err(ProjectManifestFileError::TooLarge {
            actual_bytes: buffer.len() as u64,
            limit_bytes,
        });
    }

    String::from_utf8(buffer).map_err(|error| {
        ProjectManifestFileError::Read(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    })
}
