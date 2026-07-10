use std::{collections::BTreeMap, fmt, fs::File, io::Read, path::Path};

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
use crate::validation::{
    error, validate_path_field, validate_profile_name, with_field_path, with_profile_identifier,
    with_public_i64, with_public_identifier, with_public_u64, with_secret, with_sensitive,
};
use crate::{CURRENT_PROJECT_SCHEMA_VERSION, DEFAULT_MANIFEST_BYTE_LIMIT};

#[derive(Clone, PartialEq)]
pub struct ProjectManifestLoad {
    pub manifest: Option<ProjectManifest>,
    pub diagnostics: DiagnosticReport,
}

impl fmt::Debug for ProjectManifestLoad {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectManifestLoad")
            .field("manifest_present", &self.manifest.is_some())
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
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
    #[error("failed to read project manifest metadata")]
    Metadata(#[source] std::io::Error),
    #[error("project manifest is too large: {actual_bytes} bytes > {limit_bytes} bytes")]
    TooLarge { actual_bytes: u64, limit_bytes: u64 },
    #[error("failed to read project manifest")]
    Read(#[source] std::io::Error),
}

impl ProjectManifestFileError {
    #[must_use]
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            Self::Metadata(io_error) => io_error_diagnostic(
                "project.manifest.metadata",
                "Project manifest metadata could not be read",
                io_error,
            ),
            Self::TooLarge {
                actual_bytes,
                limit_bytes,
            } => {
                let diagnostic = error(
                    "project.manifest.too-large",
                    "Project manifest exceeds its byte limit",
                );
                let diagnostic = with_public_u64(diagnostic, "actual", *actual_bytes);
                with_public_u64(diagnostic, "limit", *limit_bytes)
            }
            Self::Read(io_error) => io_error_diagnostic(
                "project.manifest.read",
                "Project manifest content could not be read",
                io_error,
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
            Err(parse_error) => {
                let mut diagnostics = DiagnosticReport::default();
                let diagnostic = error(
                    "project.manifest.parse",
                    "Project manifest syntax or structure is invalid",
                );
                let diagnostic = with_secret(diagnostic, "manifest_content");
                let (line, column) = parse_error
                    .span()
                    .map_or((0, 0), |span| source_location(source, span.start));
                let diagnostic = with_public_u64(diagnostic, "line", line);
                let diagnostic = with_public_u64(diagnostic, "column", column);
                diagnostics.push(diagnostic);
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
            let diagnostic = error(
                "project.manifest.unsupported-schema",
                "Project manifest schema version is unsupported",
            );
            let diagnostic = with_field_path(diagnostic, "schema_version");
            let diagnostic = with_public_u64(diagnostic, "actual", u64::from(self.schema_version));
            diagnostics.push(with_public_u64(
                diagnostic,
                "expected",
                u64::from(CURRENT_PROJECT_SCHEMA_VERSION),
            ));
        }

        if self.project.name.trim().is_empty() {
            diagnostics.push(with_field_path(
                error("project.name.empty", "Project name cannot be empty"),
                "project.name",
            ));
        }

        if self
            .project
            .name
            .chars()
            .any(|character| character.is_control())
        {
            diagnostics.push(with_field_path(
                error(
                    "project.name.control-character",
                    "Project name cannot contain control characters",
                ),
                "project.name",
            ));
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
            let valid_name = validate_profile_name(&mut diagnostics, name);
            let prefix = if valid_name {
                format!("profiles.{name}")
            } else {
                "profiles.redacted".to_owned()
            };
            profile.validate_into(&mut diagnostics, &prefix, &self.tasks);
        }

        diagnostics
    }

    pub fn resolve_profile(
        &self,
        profile: Option<&str>,
    ) -> Result<EffectiveProjectSettings, ProjectProfileError> {
        let diagnostics = self.validate();
        if diagnostics.has_errors() {
            return Err(ProjectProfileError::InvalidManifest {
                diagnostics: Box::new(diagnostics),
            });
        }

        let mut settings = EffectiveProjectSettings::from_manifest(self)?;
        let Some(profile_name) = profile else {
            return Ok(settings);
        };
        let Some(overlay) = self.profiles.get(profile_name) else {
            let mut diagnostics = DiagnosticReport::default();
            let diagnostic = error(
                "project.profile.unknown",
                "Requested project profile does not exist",
            );
            let diagnostic = with_profile_identifier(diagnostic, "profile", profile_name);
            diagnostics.push(with_field_path(
                diagnostic,
                &format!("profiles.{profile_name}"),
            ));
            return Err(ProjectProfileError::UnknownProfile {
                profile: profile_name.to_owned(),
                diagnostics: Box::new(diagnostics),
            });
        };

        let profile_kind = ProjectProfileKind::from_profile_name(profile_name);
        settings.profile_name = Some(profile_name.to_owned());
        settings.apply_profile_kind_defaults(profile_kind);
        overlay.apply_to(&mut settings)?;
        settings.enforce_profile_kind_invariants(profile_kind);
        settings.enforce_product_invariants();
        Ok(settings)
    }
}

fn io_error_diagnostic(
    code: &'static str,
    summary: &'static str,
    io_error: &std::io::Error,
) -> Diagnostic {
    let diagnostic = error(code, summary);
    let diagnostic = with_public_identifier(
        diagnostic,
        "io_kind",
        io_error_kind_identifier(io_error.kind()),
    );
    let diagnostic = if let Some(os_code) = io_error.raw_os_error() {
        with_public_i64(diagnostic, "os_code", i64::from(os_code))
    } else {
        diagnostic
    };
    with_sensitive(diagnostic, "manifest_path")
}

fn io_error_kind_identifier(kind: std::io::ErrorKind) -> &'static str {
    match kind {
        std::io::ErrorKind::NotFound => "not-found",
        std::io::ErrorKind::PermissionDenied => "permission-denied",
        std::io::ErrorKind::AlreadyExists => "already-exists",
        std::io::ErrorKind::InvalidInput => "invalid-input",
        std::io::ErrorKind::InvalidData => "invalid-data",
        std::io::ErrorKind::TimedOut => "timed-out",
        std::io::ErrorKind::Interrupted => "interrupted",
        std::io::ErrorKind::UnexpectedEof => "unexpected-eof",
        std::io::ErrorKind::OutOfMemory => "out-of-memory",
        _ => "other",
    }
}

fn source_location(source: &str, byte_offset: usize) -> (u64, u64) {
    let mut bounded_offset = byte_offset.min(source.len());
    while bounded_offset > 0 && !source.is_char_boundary(bounded_offset) {
        bounded_offset -= 1;
    }
    let prefix = &source[..bounded_offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let column = prefix[line_start..].chars().count() + 1;
    (
        u64::try_from(line).unwrap_or(u64::MAX),
        u64::try_from(column).unwrap_or(u64::MAX),
    )
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
