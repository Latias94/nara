//! Shared projection of source-owner failures; callers retain their operation and phase.

use nara_diagnostic::DiagnosticReport;

use crate::project_host::authoring::{AuthoringProjectSourceAdmissionError, AuthoringSourceKind};

use super::super::single_error;

pub(in crate::project_host::runtime::editor) fn authoring_source_failure(
    error: AuthoringProjectSourceAdmissionError,
) -> (Option<String>, DiagnosticReport) {
    use AuthoringProjectSourceAdmissionError as E;
    match error {
        E::NativeSource(error) => {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(error.diagnostic());
            (None, diagnostics)
        }
        E::SourcePublication { path, source, .. } => {
            (Some(path.as_str().to_owned()), source.into_diagnostics())
        }
        E::SourceStructure { path, source, .. } => {
            (Some(path.as_str().to_owned()), source.into_diagnostics())
        }
        E::CompilationBudget(diagnostics) => (None, diagnostics),
        E::SourceChanged { path, .. } => (
            Some(path.as_str().to_owned()),
            single_error(
                "project.editor.authoring-source-changed",
                "A required source changed during authoring capture; prepare it again",
            ),
        ),
        E::SourcePath { path, .. }
        | E::SourceHost { path, .. }
        | E::SourceIdentityKindConflict { path }
        | E::SourceTooLarge { path, .. }
        | E::UnsupportedSourceEncoding { path, .. }
        | E::SourceFormat { path, .. }
        | E::DocumentSourcePath { path, .. } => (
            Some(path.as_str().to_owned()),
            single_error(
                "project.editor.authoring-source-rejected",
                "A required project source could not be admitted for authoring",
            ),
        ),
        E::AssetMetadataFormat { path, .. } | E::AssetIdentity { path, .. } => (
            Some(path.as_str().to_owned()),
            single_error(
                "project.editor.authoring-asset-rejected",
                "A required project asset could not be admitted for authoring",
            ),
        ),
        E::AssetMetadataPathMismatch { requested, .. } => (
            Some(requested.as_str().to_owned()),
            single_error(
                "project.editor.authoring-asset-metadata-mismatch",
                "Asset metadata does not describe its requested project source",
            ),
        ),
        E::AssetMetadataChanged { path } => (
            Some(path.as_str().to_owned()),
            single_error(
                "project.editor.authoring-asset-metadata-source-changed",
                "Asset metadata changed during stable reference lookup; prepare it again",
            ),
        ),
        E::MissingStartupScene => (
            Some("nara.toml".to_owned()),
            single_error(
                "project.editor.startup-scene-missing",
                "The selected project profile does not declare a startup Scene",
            ),
        ),
        E::SourceDirectoryPath { kind, .. } | E::SourceDirectoryHost { kind, .. } => (
            None,
            single_error(
                "project.editor.authoring-source-directory-rejected",
                match kind {
                    AuthoringSourceKind::ProjectManifest => {
                        "The configured project manifest directory could not be admitted"
                    }
                    AuthoringSourceKind::SchemaBundle => {
                        "The configured authoring Schema bundle directory could not be admitted"
                    }
                    AuthoringSourceKind::Scene => {
                        "The configured Scene source directory could not be admitted"
                    }
                    AuthoringSourceKind::Prefab => {
                        "The configured Prefab source directory could not be admitted"
                    }
                    AuthoringSourceKind::AssetMetadata | AuthoringSourceKind::AssetSource => {
                        "The configured asset source directory could not be admitted"
                    }
                },
            ),
        ),
        E::Budget(_) | E::AccountingOverflow => (
            None,
            single_error(
                "project.editor.authoring-source-budget",
                "Authoring source preparation exhausted its finite allowance",
            ),
        ),
        _ => (
            None,
            single_error(
                "project.editor.startup-closure-rejected",
                "The authoring source closure could not be admitted",
            ),
        ),
    }
}
