use std::collections::BTreeSet;

use nara_diagnostic::DiagnosticReport;
use serde::{Deserialize, Serialize};

use crate::validation::{
    error, validate_path_field, with_field_path, with_public_u64, with_sensitive,
};
use crate::{MAX_PROJECT_CONTENT_SCENES, ProjectPath, ProjectProfileError};

/// Additional project-wide Scene roots, relative to the effective `paths.scenes` directory.
///
/// Profiles may change that directory or the startup selection, but cannot replace this list.
/// The effective startup Scene is an implicit content root and may also occur once in this list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectContentManifest {
    #[serde(default)]
    pub scenes: Vec<String>,
}

impl ProjectContentManifest {
    pub(crate) fn is_empty(&self) -> bool {
        self.scenes.is_empty()
    }

    pub(crate) fn validate_into(&self, diagnostics: &mut DiagnosticReport) {
        if self.scenes.len() > MAX_PROJECT_CONTENT_SCENES {
            let diagnostic = with_field_path(
                error(
                    "project.content.scene-limit",
                    "Declared Scene roots exceed the project content limit",
                ),
                "content.scenes",
            );
            let diagnostic = with_public_u64(
                diagnostic,
                "actual",
                u64::try_from(self.scenes.len()).unwrap_or(u64::MAX),
            );
            diagnostics.push(with_public_u64(
                diagnostic,
                "limit",
                MAX_PROJECT_CONTENT_SCENES as u64,
            ));
            return;
        }
        let mut seen = BTreeSet::new();
        for (index, scene) in self.scenes.iter().enumerate() {
            let field = format!("content.scenes.{index}");
            validate_path_field(diagnostics, &field, scene);
            if !seen.insert(scene) {
                let diagnostic = with_field_path(
                    error(
                        "project.content.duplicate-scene",
                        "Project content cannot declare the same Scene root twice",
                    ),
                    &field,
                );
                diagnostics.push(with_sensitive(diagnostic, "path"));
            }
        }
    }
}

/// Validated explicit Scene roots, independent of the profile's startup Scene selection.
///
/// Paths remain relative to the effective `paths.scenes` directory. Content loading joins these
/// roots with the selected startup Scene and resolves each shared root only once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectiveProjectContentSettings {
    pub scenes: Vec<ProjectPath>,
}

impl EffectiveProjectContentSettings {
    pub(crate) fn from_manifest(
        manifest: &ProjectContentManifest,
    ) -> Result<Self, ProjectProfileError> {
        Ok(Self {
            scenes: manifest
                .scenes
                .iter()
                .cloned()
                .map(ProjectPath::new)
                .collect::<Result<_, _>>()
                .map_err(ProjectProfileError::InvalidProjectPath)?,
        })
    }
}

#[cfg(test)]
mod tests;
