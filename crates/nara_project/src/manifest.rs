use std::{collections::BTreeMap, fmt};

use nara_core::{
    ByteLimit, DepthLimit, ItemLimit, SerdeShapeError, SerdeShapeLimits, SerdeShapePreflightError,
    preflight_serde_shape,
};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use serde::Deserialize;

use crate::effective::EffectiveProjectSettings;
use crate::profile::{ProjectProfileError, ProjectProfileOverlay};
use crate::sections::{
    ProjectCapabilitiesManifest, ProjectDiagnosticsManifest, ProjectInfo, ProjectInputManifest,
    ProjectPathsManifest, ProjectProfileKind, ProjectRuntimeManifest, ProjectStartupManifest,
    ProjectTasksManifest, ProjectWindowManifest,
};
use crate::validation::{
    error, validate_path_field, validate_profile_name, with_field_path, with_profile_identifier,
    with_public_u64, with_secret,
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
    pub capabilities: ProjectCapabilitiesManifest,
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
        Self::parse_toml_bytes(source.as_bytes())
    }

    #[must_use]
    pub fn parse_toml_bytes(source: &[u8]) -> ProjectManifestLoad {
        let limit = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT)
            .expect("the project manifest byte limit fits usize on supported targets");
        if source.len() > limit {
            return ProjectManifestLoad::failed(single_diagnostic(manifest_too_large_diagnostic(
                source.len(),
                limit,
            )));
        }

        let Ok(source) = std::str::from_utf8(source) else {
            let diagnostic = error(
                "project.manifest.utf8",
                "Project manifest content is not valid UTF-8",
            );
            return ProjectManifestLoad::failed(single_diagnostic(with_secret(
                diagnostic,
                "manifest_content",
            )));
        };

        parse_bounded_toml(source)
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

        self.capabilities
            .validate_into(&mut diagnostics, "capabilities");
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

fn parse_bounded_toml(source: &str) -> ProjectManifestLoad {
    let deserializer = match toml::de::Deserializer::parse(source) {
        Ok(deserializer) => deserializer,
        Err(parse_error) => {
            return ProjectManifestLoad::failed(single_diagnostic(manifest_parse_diagnostic(
                source,
                parse_error.span().map(|span| span.start),
            )));
        }
    };
    match preflight_serde_shape(deserializer, manifest_shape_limits()) {
        Ok(()) => {}
        Err(SerdeShapePreflightError::Shape(shape)) => {
            return ProjectManifestLoad::failed(single_diagnostic(manifest_shape_diagnostic(
                shape,
            )));
        }
        Err(SerdeShapePreflightError::Parse(parse_error)) => {
            return ProjectManifestLoad::failed(single_diagnostic(manifest_parse_diagnostic(
                source,
                parse_error.span().map(|span| span.start),
            )));
        }
    }

    match toml::from_str::<ProjectManifest>(source) {
        Ok(manifest) => ProjectManifestLoad::ok(manifest),
        Err(parse_error) => ProjectManifestLoad::failed(single_diagnostic(
            manifest_parse_diagnostic(source, parse_error.span().map(|span| span.start)),
        )),
    }
}

fn manifest_shape_limits() -> SerdeShapeLimits {
    SerdeShapeLimits::new(
        DepthLimit::new(32).expect("project manifest depth limit is non-zero"),
        ItemLimit::new(4_096).expect("project manifest node limit is non-zero"),
        ItemLimit::new(512).expect("project manifest container limit is non-zero"),
        ByteLimit::new(16 * 1024).expect("project manifest string limit is non-zero"),
        ByteLimit::new(192 * 1024).expect("project manifest total string limit is non-zero"),
    )
}

fn manifest_shape_diagnostic(shape: SerdeShapeError) -> Diagnostic {
    let (code, summary, maximum) = match shape {
        SerdeShapeError::DepthExceeded { maximum } => (
            "project.manifest.depth-limit",
            "Project manifest nesting exceeds its limit",
            Some(maximum),
        ),
        SerdeShapeError::NodeLimitExceeded { maximum } => (
            "project.manifest.node-limit",
            "Project manifest node count exceeds its limit",
            Some(maximum),
        ),
        SerdeShapeError::ContainerItemLimitExceeded { maximum } => (
            "project.manifest.container-limit",
            "A project manifest container exceeds its item limit",
            Some(maximum),
        ),
        SerdeShapeError::StringLimitExceeded { maximum } => (
            "project.manifest.string-limit",
            "A project manifest string exceeds its byte limit",
            Some(maximum),
        ),
        SerdeShapeError::TotalStringLimitExceeded { maximum } => (
            "project.manifest.total-string-limit",
            "Project manifest strings exceed their total byte limit",
            Some(maximum),
        ),
        SerdeShapeError::DuplicateMapKey => (
            "project.manifest.duplicate-key",
            "Project manifest contains a duplicate key",
            None,
        ),
    };
    let diagnostic = with_secret(error(code, summary), "manifest_content");
    maximum.map_or(diagnostic.clone(), |maximum| {
        with_public_u64(
            diagnostic,
            "limit",
            u64::try_from(maximum).unwrap_or(u64::MAX),
        )
    })
}

fn manifest_parse_diagnostic(source: &str, offset: Option<usize>) -> Diagnostic {
    let diagnostic = error(
        "project.manifest.parse",
        "Project manifest syntax or structure is invalid",
    );
    let diagnostic = with_secret(diagnostic, "manifest_content");
    let (line, column) = offset.map_or((0, 0), |offset| source_location(source, offset));
    let diagnostic = with_public_u64(diagnostic, "line", line);
    with_public_u64(diagnostic, "column", column)
}

fn manifest_too_large_diagnostic(actual: usize, limit: usize) -> Diagnostic {
    let diagnostic = error(
        "project.manifest.too-large",
        "Project manifest exceeds its byte limit",
    );
    let diagnostic = with_public_u64(
        diagnostic,
        "actual",
        u64::try_from(actual).unwrap_or(u64::MAX),
    );
    with_public_u64(
        diagnostic,
        "limit",
        u64::try_from(limit).unwrap_or(u64::MAX),
    )
}

fn single_diagnostic(diagnostic: Diagnostic) -> DiagnosticReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic);
    diagnostics
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
