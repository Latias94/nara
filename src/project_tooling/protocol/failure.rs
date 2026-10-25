use std::fmt;

use nara_diagnostic::{
    DiagnosticField, DiagnosticFieldKey, DiagnosticReport, DiagnosticSeverity,
    PublicDiagnosticIdentifier,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::project_workflow::ProjectCheckError;

pub(super) const MAX_FAILURE_BYTES: usize = 16 * 1024;
const MAX_DIAGNOSTICS: usize = 8;
const MAX_CODE_BYTES: usize = 96;
const MAX_SUMMARY_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectToolFailureStage {
    Request,
    Recipe,
    ProjectAccess,
    ProjectAdmission,
    ContentAdmission,
    Schema,
    ExportPreparation,
    Publication,
    Acceptance,
    Repair,
}

/// Additional transfer loss is separate from incompleteness already present at the producer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectToolDiagnosticLoss {
    pub source_incomplete: bool,
    pub omitted_entries: u64,
    pub omitted_fields: u64,
    pub truncated_summaries: u64,
    pub removed_summary_bytes: u64,
    pub withheld_repair_fields: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectToolDiagnostic {
    code: String,
    severity: DiagnosticSeverity,
    summary: String,
}

impl ProjectToolDiagnostic {
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub const fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectToolRepairHint {
    scene: Option<String>,
    scenes_root: Option<String>,
    profile: Option<String>,
    profile_selected: bool,
}

impl ProjectToolRepairHint {
    #[must_use]
    pub fn scene(&self) -> Option<&str> {
        self.scene.as_deref()
    }

    #[must_use]
    pub fn scenes_root(&self) -> Option<&str> {
        self.scenes_root.as_deref()
    }

    #[must_use]
    pub fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }

    /// True also when a selected profile was withheld by diagnostic admission.
    #[must_use]
    pub const fn profile_selected(&self) -> bool {
        self.profile_selected
    }
}

/// A bounded observation from the explicitly trusted compiled project, not a runtime diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectToolFailure {
    operation: String,
    stage: ProjectToolFailureStage,
    code: String,
    summary: String,
    diagnostics: Vec<ProjectToolDiagnostic>,
    loss: ProjectToolDiagnosticLoss,
    repair: Option<ProjectToolRepairHint>,
}

impl ProjectToolFailure {
    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }

    #[must_use]
    pub const fn stage(&self) -> ProjectToolFailureStage {
        self.stage
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[ProjectToolDiagnostic] {
        &self.diagnostics
    }

    #[must_use]
    pub const fn loss(&self) -> ProjectToolDiagnosticLoss {
        self.loss
    }

    #[must_use]
    pub const fn repair_hint(&self) -> Option<&ProjectToolRepairHint> {
        self.repair.as_ref()
    }

    pub(in crate::project_tooling) fn classified(
        operation: &str,
        stage: ProjectToolFailureStage,
        code: &'static str,
        summary: &'static str,
    ) -> Self {
        Self {
            operation: operation.to_owned(),
            stage,
            code: code.to_owned(),
            summary: summary.to_owned(),
            diagnostics: Vec::new(),
            loss: ProjectToolDiagnosticLoss::default(),
            repair: None,
        }
    }

    pub(in crate::project_tooling) fn from_check(
        operation: &str,
        error: &ProjectCheckError,
    ) -> Self {
        use crate::project_workflow::ProjectCheckErrorKind;
        let (stage, code, summary) = match error.kind() {
            ProjectCheckErrorKind::Manifest => (
                ProjectToolFailureStage::ProjectAdmission,
                "project.tool.manifest-rejected",
                "Project manifest admission failed",
            ),
            ProjectCheckErrorKind::ProductComposition => (
                ProjectToolFailureStage::ProjectAdmission,
                "project.tool.composition-rejected",
                "Project product composition was rejected",
            ),
            ProjectCheckErrorKind::Content => (
                ProjectToolFailureStage::ContentAdmission,
                "project.tool.content-rejected",
                "Project content admission failed; inspect the admitted diagnostics",
            ),
        };
        let mut failure = Self::classified(operation, stage, code, summary);
        if let Some(content) = error.content_error() {
            failure.project_report(content.diagnostics());
        }
        if let Some(issue) = error.startup_scene_issue() {
            failure.code = "project.tool.startup-scene-missing".to_owned();
            failure.summary = "The selected startup Scene is missing".to_owned();
            let scene = admitted_path(issue.selected_scene().as_str());
            let scenes_root = admitted_path(issue.scenes_path().as_str());
            let profile = issue
                .profile()
                .and_then(|value| PublicDiagnosticIdentifier::new(value).ok())
                .map(|value| value.as_str().to_owned());
            failure.loss.withheld_repair_fields = u64::from(scene.is_none())
                + u64::from(scenes_root.is_none())
                + u64::from(issue.profile().is_some() && profile.is_none());
            failure.repair = Some(ProjectToolRepairHint {
                scene,
                scenes_root,
                profile,
                profile_selected: issue.profile().is_some(),
            });
        }
        failure
    }

    pub(in crate::project_tooling) fn from_schema(
        operation: &str,
        error: crate::project_host::schema::SchemaRefreshError,
    ) -> Self {
        use crate::project_host::schema::{
            SchemaPublicationFailure as Outcome, SchemaRefreshError as Error,
        };
        let (code, summary) = match error {
            Error::ManifestAccess => (
                "project.tool.schema-manifest-unavailable",
                "Cannot open the project manifest for schema refresh",
            ),
            Error::ManifestAdmission => (
                "project.tool.schema-manifest-rejected",
                "Cannot admit the selected project profile for schema refresh",
            ),
            Error::DeclarationMissing => (
                "project.tool.schema-declaration-missing",
                "Declare authoring.schema_bundle before schema refresh",
            ),
            Error::InvalidPath => (
                "project.tool.schema-path-rejected",
                "The declared authoring schema bundle path is invalid",
            ),
            Error::Composition => (
                "project.tool.schema-composition-rejected",
                "Cannot compose the selected project schema",
            ),
            Error::Encode => (
                "project.tool.schema-encode-failed",
                "Cannot encode the selected project schema",
            ),
            Error::BundleRead | Error::BundleOpen => (
                "project.tool.schema-bundle-unavailable",
                "Cannot read the declared authoring schema bundle",
            ),
            Error::Stale => (
                "project.tool.schema-stale",
                "The declared authoring schema bundle is missing or stale; run nara schema <project-directory> --write with the same profile",
            ),
            Error::Binding => (
                "project.tool.schema-binding-rejected",
                "Cannot bind the declared authoring schema bundle for publication",
            ),
            Error::Publication {
                outcome: Outcome::Rejected,
                cleanup_pending: false,
            } => (
                "project.tool.schema-publication-rejected",
                "Schema bundle publication was rejected; prior bytes were retained",
            ),
            Error::Publication {
                outcome: Outcome::Rejected,
                cleanup_pending: true,
            } => (
                "project.tool.schema-publication-rejected-cleanup-pending",
                "Schema bundle publication was rejected; prior bytes were retained; temporary schema bundle cleanup is pending before retrying schema write",
            ),
            Error::Publication {
                outcome: Outcome::Failed,
                cleanup_pending: false,
            } => (
                "project.tool.schema-publication-failed",
                "Schema bundle publication failed; prior bytes were retained",
            ),
            Error::Publication {
                outcome: Outcome::Failed,
                cleanup_pending: true,
            } => (
                "project.tool.schema-publication-failed-cleanup-pending",
                "Schema bundle publication failed; prior bytes were retained; temporary schema bundle cleanup is pending before retrying schema write",
            ),
            Error::Publication {
                outcome: Outcome::Uncertain,
                cleanup_pending: false,
            } => (
                "project.tool.schema-publication-uncertain",
                "Schema bundle publication outcome is uncertain; reopen and check the declared file before retrying",
            ),
            Error::Publication {
                outcome: Outcome::Uncertain,
                cleanup_pending: true,
            } => (
                "project.tool.schema-publication-uncertain-cleanup-pending",
                "Schema bundle publication outcome is uncertain; reopen and check the declared file and resolve pending temporary schema bundle cleanup before retrying schema write",
            ),
        };
        Self::classified(operation, ProjectToolFailureStage::Schema, code, summary)
    }

    pub(in crate::project_tooling) fn from_repair(
        operation: &str,
        error: &crate::project_workflow::ProjectStartupSceneRepairError,
    ) -> Self {
        use crate::project_workflow::ProjectStartupSceneRepairError as Error;
        let (code, summary) = if matches!(error, Error::TemporaryFile(failure) if failure.cleanup().is_some())
        {
            (
                "project.tool.repair-cleanup-pending",
                "Repair failed and temporary manifest cleanup remains pending",
            )
        } else {
            ("project.tool.repair-failed", "Startup Scene repair failed")
        };
        let mut failure =
            Self::classified(operation, ProjectToolFailureStage::Repair, code, summary);
        match error {
            Error::CurrentProject(check) | Error::Replacement(check) => {
                if let Some(content) = check.content_error() {
                    failure.project_report(content.diagnostics());
                }
            }
            _ => {
                if let Some(report) = error.diagnostics() {
                    failure.project_report(report);
                }
            }
        }
        failure
    }

    pub(in crate::project_tooling) fn from_repair_postcheck(
        operation: &str,
        error: &crate::project_workflow::ProjectStartupScenePostcheckError,
    ) -> Self {
        use crate::project_workflow::ProjectStartupScenePostcheckError as Error;
        let (code, summary) = match error {
            Error::Check(_) => (
                "project.tool.repair-published-check-rejected",
                "The manifest was published but its fresh project check failed",
            ),
            Error::PreparedFactsChanged => (
                "project.tool.repair-published-facts-changed",
                "The manifest was published but the checked project facts changed",
            ),
            Error::PublicationEvidenceIncomplete => (
                "project.tool.repair-published-unproven",
                "The manifest was published but its exact byte evidence is incomplete; reopen and check before retrying",
            ),
        };
        let mut failure = Self::classified(
            operation,
            ProjectToolFailureStage::Acceptance,
            code,
            summary,
        );
        if let Error::Check(error) = error
            && let Some(content) = error.content_error()
        {
            failure.project_report(content.diagnostics());
        }
        failure
    }

    fn project_report(&mut self, report: &DiagnosticReport) {
        let stats = report.stats();
        self.loss.source_incomplete = stats.rejected_entries() != 0
            || stats.evicted_entries() != 0
            || stats.dropped_fields() != 0
            || stats.truncated_fields() != 0;
        for entry in report.iter() {
            self.loss.omitted_fields = self
                .loss
                .omitted_fields
                .saturating_add(entry.fields().len() as u64);
            if self.diagnostics.len() == MAX_DIAGNOSTICS {
                self.loss.omitted_entries = self.loss.omitted_entries.saturating_add(1);
                continue;
            }
            let summary = entry.summary().as_str();
            let end = summary.floor_char_boundary(MAX_SUMMARY_BYTES);
            if end != summary.len() {
                self.loss.truncated_summaries = self.loss.truncated_summaries.saturating_add(1);
                self.loss.removed_summary_bytes = self
                    .loss
                    .removed_summary_bytes
                    .saturating_add((summary.len() - end) as u64);
            }
            self.diagnostics.push(ProjectToolDiagnostic {
                code: entry.code().as_str().to_owned(),
                severity: entry.severity(),
                summary: summary[..end].to_owned(),
            });
        }
    }

    pub(super) fn to_value(&self) -> Value {
        let diagnostics: Vec<_> = self
            .diagnostics
            .iter()
            .map(|entry| {
                let severity = match entry.severity {
                    DiagnosticSeverity::Error => "error",
                    DiagnosticSeverity::Warning => "warning",
                    DiagnosticSeverity::Info => "info",
                };
                json!({"code":entry.code,"severity":severity,"summary":entry.summary})
            })
            .collect();
        let mut value = json!({"stage":self.stage,"code":self.code,"summary":self.summary,
            "diagnostics":null,"loss":self.loss,"repair":self.repair});
        value["diagnostics"] = Value::Array(diagnostics);
        value
    }
}

impl fmt::Display for ProjectToolFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.summary)?;
        for entry in &self.diagnostics {
            write!(formatter, "\n{}: {}", entry.code, entry.summary)?;
        }
        if let Some(hint) = &self.repair {
            if let Some(scene) = &hint.scene {
                write!(formatter, "\nstartup Scene `{scene}` is missing")?;
                if let Some(root) = &hint.scenes_root {
                    write!(formatter, " under the project Scene root `{root}`")?;
                }
            }
            write!(
                formatter,
                "\nrepair: nara repair startup-scene <project-directory>"
            )?;
            if hint.profile_selected {
                write!(
                    formatter,
                    " --profile {}",
                    hint.profile.as_deref().unwrap_or("<name>")
                )?;
            }
            write!(formatter, " --replacement <scenes-root-relative-scene>")?;
        }
        if self.loss != ProjectToolDiagnosticLoss::default() {
            write!(
                formatter,
                "\nDiagnostic projection is incomplete: {:?}",
                self.loss
            )?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawFailure {
    stage: ProjectToolFailureStage,
    code: String,
    summary: String,
    diagnostics: Vec<RawDiagnostic>,
    loss: ProjectToolDiagnosticLoss,
    repair: Option<RawRepairHint>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRepairHint {
    scene: Option<String>,
    scenes_root: Option<String>,
    profile: Option<String>,
    profile_selected: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDiagnostic {
    code: String,
    severity: String,
    summary: String,
}

impl RawFailure {
    pub(super) fn admit(self, operation: &str) -> Result<ProjectToolFailure, &'static str> {
        if !valid_code(&self.code)
            || !valid_summary(&self.summary)
            || self.diagnostics.len() > MAX_DIAGNOSTICS
        {
            return Err("invalid project tool failure projection");
        }
        let mut diagnostics = Vec::with_capacity(self.diagnostics.len());
        for entry in self.diagnostics {
            let severity = match entry.severity.as_str() {
                "error" => DiagnosticSeverity::Error,
                "warning" => DiagnosticSeverity::Warning,
                "info" => DiagnosticSeverity::Info,
                _ => return Err("invalid project tool diagnostic severity"),
            };
            if !valid_code(&entry.code) || !valid_summary(&entry.summary) {
                return Err("invalid project tool diagnostic projection");
            }
            diagnostics.push(ProjectToolDiagnostic {
                code: entry.code,
                severity,
                summary: entry.summary,
            });
        }
        if let Some(hint) = &self.repair
            && (hint
                .scene
                .as_deref()
                .is_some_and(|value| admitted_path(value).is_none())
                || hint
                    .scenes_root
                    .as_deref()
                    .is_some_and(|value| admitted_path(value).is_none())
                || hint.profile.as_deref().is_some_and(|value| {
                    !hint.profile_selected || PublicDiagnosticIdentifier::new(value).is_err()
                }))
        {
            return Err("invalid project tool repair hint");
        }
        Ok(ProjectToolFailure {
            operation: operation.to_owned(),
            stage: self.stage,
            code: self.code,
            summary: self.summary,
            diagnostics,
            loss: self.loss,
            repair: self.repair.map(|hint| ProjectToolRepairHint {
                scene: hint.scene,
                scenes_root: hint.scenes_root,
                profile: hint.profile,
                profile_selected: hint.profile_selected,
            }),
        })
    }
}

fn admitted_path(value: &str) -> Option<String> {
    DiagnosticField::project_relative(DiagnosticFieldKey::new("path").ok()?, value)
        .ok()
        .map(|_| value.to_owned())
}

fn valid_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CODE_BYTES
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
}

fn valid_summary(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SUMMARY_BYTES
        && !value.chars().any(|character| character.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};

    fn classified() -> ProjectToolFailure {
        ProjectToolFailure::classified(
            "check",
            ProjectToolFailureStage::ContentAdmission,
            "project.tool.check-failed",
            "Project check failed",
        )
    }

    #[test]
    fn published_repair_acceptance_failures_do_not_invent_a_failed_check() {
        use crate::project_workflow::ProjectStartupScenePostcheckError as Error;
        for (error, code, expected) in [
            (
                Error::PreparedFactsChanged,
                "project.tool.repair-published-facts-changed",
                "checked project facts changed",
            ),
            (
                Error::PublicationEvidenceIncomplete,
                "project.tool.repair-published-unproven",
                "exact byte evidence is incomplete",
            ),
        ] {
            let failure = ProjectToolFailure::from_repair_postcheck("repair-startup-scene", &error);
            let raw: RawFailure = serde_json::from_value(failure.to_value()).unwrap();
            let failure = raw.admit("repair-startup-scene").unwrap();
            assert_eq!(failure.stage(), ProjectToolFailureStage::Acceptance);
            assert_eq!(failure.code(), code);
            assert!(failure.summary().contains(expected));
            assert!(!failure.to_string().contains("fresh project check failed"));
        }
    }

    #[test]
    fn report_projection_preserves_codes_and_counts_only_additional_loss() {
        let mut report = DiagnosticReport::default();
        for _ in 0..10 {
            let entry = Diagnostic::error(
                DiagnosticCode::new("consumer.text.too-long").unwrap(),
                SafeSummary::new("Text exceeds max_characters").unwrap(),
            )
            .try_with_field(DiagnosticField::public_identifier(
                DiagnosticFieldKey::new("context").unwrap(),
                PublicDiagnosticIdentifier::new("NARA_PRIVATE_SOURCE_CANARY").unwrap(),
            ))
            .unwrap();
            report.push(entry);
        }
        let mut failure = classified();
        failure.project_report(&report);
        assert_eq!(failure.diagnostics().len(), 8);
        assert!(
            failure
                .diagnostics()
                .iter()
                .all(|entry| entry.code() == "consumer.text.too-long")
        );
        assert_eq!(failure.loss().omitted_entries, 2);
        assert_eq!(failure.loss().omitted_fields, 10);
        assert!(!failure.loss().source_incomplete);
        let value = failure.to_value();
        let encoded = serde_json::to_vec(&value).unwrap();
        assert!(encoded.len() + 1 <= MAX_FAILURE_BYTES);
        assert!(
            !String::from_utf8(encoded)
                .unwrap()
                .contains("NARA_PRIVATE_SOURCE_CANARY")
        );
        let decoded: RawFailure = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.admit("check"), Ok(failure));
    }

    #[test]
    fn summary_truncation_preserves_utf8_and_source_loss_is_not_counted_twice() {
        const LONG: &str = concat!(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "\u{1f980}tail",
        );
        let settings = nara_diagnostic::DiagnosticReportSettings::default()
            .with_summary_byte_limit(crate::core::ByteLimit::new(1024).unwrap())
            .unwrap();
        let mut report = DiagnosticReport::new(settings);
        report.push(Diagnostic::error(
            DiagnosticCode::new("consumer.long").unwrap(),
            SafeSummary::new(LONG).unwrap(),
        ));
        let mut failure = classified();
        failure.project_report(&report);
        assert_eq!(failure.diagnostics()[0].summary().len(), 255);
        assert_eq!(failure.loss().truncated_summaries, 1);
        assert_eq!(failure.loss().removed_summary_bytes, 8);
        assert!(!failure.loss().source_incomplete);

        let settings = nara_diagnostic::DiagnosticReportSettings::new(
            crate::core::ItemLimit::new(1).unwrap(),
            crate::core::ByteLimit::new(1024).unwrap(),
        )
        .unwrap();
        let mut evicted = DiagnosticReport::new(settings);
        for code in ["consumer.first", "consumer.second"] {
            evicted.push(Diagnostic::error(
                DiagnosticCode::new(code).unwrap(),
                SafeSummary::new("A bounded failure").unwrap(),
            ));
        }
        let mut failure = classified();
        failure.project_report(&evicted);
        assert!(failure.loss().source_incomplete);
        assert_eq!(failure.loss().omitted_entries, 0);
        assert_eq!(failure.diagnostics().len(), 1);
        assert_eq!(failure.diagnostics()[0].code(), "consumer.second");
    }

    #[test]
    fn malformed_remote_values_do_not_echo_private_input() {
        let base = classified().to_value();
        for (key, value) in [
            ("code", json!("NARA_PRIVATE_SOURCE_CANARY")),
            ("summary", json!("bad\nsummary")),
            ("stage", json!("NARA_PRIVATE_SOURCE_CANARY")),
            ("fields", json!({"private":"NARA_PRIVATE_SOURCE_CANARY"})),
        ] {
            let mut value_to_decode = base.clone();
            value_to_decode[key] = value;
            let result = serde_json::from_value::<RawFailure>(value_to_decode)
                .map_err(|_| "invalid project tool failure")
                .and_then(|raw| raw.admit("check"));
            let error = result.unwrap_err();
            assert!(!error.contains("NARA_PRIVATE_SOURCE_CANARY"));
        }
    }
}
