use super::*;
use nara_core::diagnostic::{
    DiagnosticCode, DiagnosticReportSettings, ReportDiagnostic as Diagnostic, SafeDisplayText,
    SafeSummary,
};
use std::{fs, path::PathBuf};

fn failure() -> ProjectToolFailure {
    ProjectToolFailure::classified(
        "check",
        ProjectToolFailureStage::ContentAdmission,
        "project.tool.content-rejected",
        "Project content admission failed",
    )
}

fn diagnostic() -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new("project.content.host-io").unwrap(),
        SafeSummary::new("Project content host I/O failed").unwrap(),
    )
}

fn field_key(value: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(value).unwrap()
}

#[test]
fn explicit_project_location_survives_projection_roundtrip_and_terminal_formatting() {
    let mut report = DiagnosticReport::default();
    report.push(
        diagnostic()
            .try_with_field(
                DiagnosticField::project_relative(
                    field_key("project-path"),
                    "content/Prefabs/玩家.prefab.json.meta",
                )
                .unwrap(),
            )
            .unwrap()
            .try_with_field(
                DiagnosticField::project_relative(field_key("path"), "local-alias.meta").unwrap(),
            )
            .unwrap()
            .try_with_field(DiagnosticField::sensitive(field_key("source")))
            .unwrap(),
    );
    let mut failure = failure();
    failure.project_report(&report);
    assert_eq!(
        failure.diagnostics()[0].project_path(),
        Some("content/Prefabs/玩家.prefab.json.meta")
    );
    assert_eq!(failure.loss().omitted_fields, 2);
    assert!(!failure.loss().source_incomplete);
    assert!(
        failure
            .to_string()
            .contains("\n  at content/Prefabs/玩家.prefab.json.meta")
    );
    assert!(!failure.to_string().contains("local-alias.meta"));
    let value = failure.to_value();
    assert!(!value.to_string().contains("local-alias.meta"));
    let raw: RawFailure = serde_json::from_value(value).unwrap();
    assert_eq!(raw.admit("check"), Ok(failure));
}

#[test]
fn unrelated_wrong_class_and_oversize_locations_remain_omitted_without_truncation() {
    let settings = DiagnosticReportSettings::default()
        .with_field_text_byte_limit(crate::core::ByteLimit::new(1024).unwrap())
        .unwrap();
    let mut report = DiagnosticReport::new(settings);
    for field in [
        DiagnosticField::project_relative(field_key("path"), "assets/local-alias.meta").unwrap(),
        DiagnosticField::public_display(
            field_key("project-path"),
            SafeDisplayText::new("local-alias.meta").unwrap(),
        ),
        DiagnosticField::sensitive(field_key("project-path")),
        DiagnosticField::project_relative(
            field_key("project-path"),
            &"a".repeat(MAX_PROJECT_PATH_BYTES + 1),
        )
        .unwrap(),
    ] {
        report.push(diagnostic().try_with_field(field).unwrap());
    }
    let mut failure = failure();
    failure.project_report(&report);
    assert_eq!(failure.diagnostics().len(), 4);
    assert!(
        failure
            .diagnostics()
            .iter()
            .all(|entry| entry.project_path().is_none())
    );
    assert_eq!(failure.loss().omitted_fields, 4);
    assert!(!failure.loss().source_incomplete);
    assert!(!failure.to_value().to_string().contains("local-alias.meta"));
    assert!(
        !failure
            .to_value()
            .to_string()
            .contains(&"a".repeat(MAX_PROJECT_PATH_BYTES))
    );
}

#[test]
fn omitted_diagnostics_keep_their_location_in_the_omitted_field_count() {
    let mut report = DiagnosticReport::default();
    for _ in 0..10 {
        report.push(
            diagnostic()
                .try_with_field(
                    DiagnosticField::project_relative(field_key("project-path"), "scene.meta")
                        .unwrap(),
                )
                .unwrap(),
        );
    }
    let mut failure = failure();
    failure.project_report(&report);
    assert_eq!(failure.diagnostics().len(), 8);
    assert_eq!(failure.loss().omitted_entries, 2);
    assert_eq!(failure.loss().omitted_fields, 2);
    assert!(
        failure
            .diagnostics()
            .iter()
            .all(|entry| entry.project_path() == Some("scene.meta"))
    );
}

#[test]
fn remote_locations_require_the_existing_bounded_project_relative_policy() {
    let mut report = DiagnosticReport::default();
    report.push(diagnostic());
    let mut failure = failure();
    failure.project_report(&report);
    let base = failure.to_value();
    for path in [
        "",
        "/private/source.meta",
        "C:/private/source.meta",
        "../source.meta",
        "assets/../source.meta",
        "assets\\source.meta",
        "assets/\u{202e}source.meta",
        "assets/source\n.meta",
        &"a".repeat(MAX_PROJECT_PATH_BYTES + 1),
    ] {
        let mut value = base.clone();
        value["diagnostics"][0]["project_path"] = json!(path);
        let raw: RawFailure = serde_json::from_value(value).unwrap();
        assert_eq!(
            raw.admit("check").unwrap_err(),
            "invalid project tool diagnostic projection"
        );
    }
    let mut value = base.clone();
    value["diagnostics"][0]["line"] = json!(1);
    assert!(serde_json::from_value::<RawFailure>(value).is_err());
    let encoded = serde_json::to_string(&base).unwrap().replace(
        "\"project_path\":null",
        "\"project_path\":null,\"project_path\":\"scene.meta\"",
    );
    assert!(serde_json::from_str::<RawFailure>(&encoded).is_err());
}

#[test]
fn maximum_locations_and_summaries_still_fit_the_complete_failure_envelope() {
    let path = "\"".repeat(MAX_PROJECT_PATH_BYTES);
    const SUMMARY: &str = concat!(
        r#""""""""""""""""""""""""""""""""""#,
        r#""""""""""""""""""""""""""""""""""#,
        r#""""""""""""""""""""""""""""""""""#,
        r#""""""""""""""""""""""""""""""""""#,
        r#""""""""""""""""""""""""""""""""""#,
        r#""""""""""""""""""""""""""""""""""#,
        r#""""""""""""""""""""""""""""""""""#,
        r#""""""""""""""""""""""""""""""""""#,
    );
    assert_eq!(SUMMARY.len(), MAX_SUMMARY_BYTES);
    let mut report = DiagnosticReport::default();
    for _ in 0..MAX_DIAGNOSTICS {
        report.push(
            Diagnostic::error(
                DiagnosticCode::new("project.content.host-io").unwrap(),
                SafeSummary::new(SUMMARY).unwrap(),
            )
            .try_with_field(
                DiagnosticField::project_relative(field_key("project-path"), &path).unwrap(),
            )
            .unwrap(),
        );
    }
    let mut failure = failure();
    failure.project_report(&report);
    failure.code = "a".repeat(MAX_CODE_BYTES);
    failure.summary = SUMMARY.to_owned();
    for entry in &mut failure.diagnostics {
        entry.code = "a".repeat(MAX_CODE_BYTES);
    }
    failure.repair = Some(ProjectToolRepairHint {
        scene: admitted_path(&"\"".repeat(nara_core::diagnostic::MAX_DIAGNOSTIC_DRAFT_TEXT_BYTES)),
        scenes_root: admitted_path(
            &"\"".repeat(nara_core::diagnostic::MAX_DIAGNOSTIC_DRAFT_TEXT_BYTES),
        ),
        profile: Some("a".repeat(MAX_CODE_BYTES)),
        profile_selected: true,
    });
    assert!(failure.repair_hint().unwrap().scene().is_some());
    let response =
        json!({"version":1,"operation":"check","success":false,"failure":failure.to_value()});
    let mut encoded = serde_json::to_vec(&response).unwrap();
    encoded.push(b'\n');
    assert!(encoded.len() <= MAX_FAILURE_BYTES);
    let decoded = super::super::decode_response(&encoded, "check")
        .unwrap()
        .unwrap_err();
    assert_eq!(decoded, failure);
}

#[test]
fn missing_startup_scene_diagnostic_preserves_the_selected_profile() {
    use crate::{
        ProductRecipe,
        fs::RelativePath,
        project_host::{ingest_project_manifest, resolve_product_recipe},
        project_tooling::{ProjectToolRequest, execute, host_fs, protocol},
    };

    struct Project(PathBuf);
    impl Drop for Project {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    let mut nonce = [0; 8];
    getrandom::fill(&mut nonce).unwrap();
    let path = std::env::temp_dir().join(format!(
        "nara-missing-startup-{:016x}",
        u64::from_ne_bytes(nonce)
    ));
    fs::create_dir(&path).unwrap();
    let project = Project(path);
    for directory in ["assets", "scenes", "prefabs", "schema"] {
        fs::create_dir(project.0.join(directory)).unwrap();
    }
    let base = r#"schema_version = 1
[project]
source_universe = "796b54db-7d94-51a0-9ca8-a19d46ce7b50"
native_document_identity = "project_location"
name = "Startup Scene Repair"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"
[startup]
default_scene = "missing.scene.json"
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d"]
"#;
    for profile in [None, Some("tools".to_owned()), Some("a".repeat(128))] {
        let manifest = match &profile {
            Some(profile) => format!(
                "{base}\n[profiles.{profile}.startup]\ndefault_scene = \"missing.scene.json\"\n"
            ),
            None => base.to_owned(),
        };
        fs::write(project.0.join("nara.toml"), manifest).unwrap();
        let root = host_fs::readable_directory(&project.0).unwrap();
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, profile.as_deref()).unwrap();
        let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
        let schema = plan
            .schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap();
        fs::write(project.0.join("schema/authoring.bundle.json"), schema).unwrap();

        let error = execute(
            ProjectToolRequest::Check {
                project: project.0.clone(),
                profile: profile.clone(),
            },
            ProductRecipe::new(),
        )
        .unwrap_err();
        let response = protocol::encode_response("check", Err(error)).unwrap();
        let bytes = serde_json::to_vec(&response).unwrap();
        let failure = protocol::decode_response(&bytes, "check")
            .unwrap()
            .unwrap_err();
        let hint = failure.repair_hint().unwrap();
        assert_eq!(hint.scene(), Some("missing.scene.json"));
        assert_eq!(hint.scenes_root(), Some("scenes"));
        assert_eq!(hint.profile_selected(), profile.is_some());
        let display = failure.to_string();
        assert!(
            display.contains("startup Scene `missing.scene.json` is missing"),
            "{display}"
        );
        match profile.as_deref() {
            Some("tools") => {
                assert_eq!(hint.profile(), Some("tools"));
                assert_eq!(failure.loss().withheld_repair_fields, 0);
                assert!(display.contains("for profile `tools`"));
            }
            Some(withheld) => {
                assert_eq!(hint.profile(), None);
                assert_eq!(failure.loss().withheld_repair_fields, 1);
                assert!(display.contains("for the selected profile"));
                assert!(!display.contains(withheld));
                assert!(!String::from_utf8_lossy(&bytes).contains(withheld));
            }
            None => {
                assert_eq!(hint.profile(), None);
                assert_eq!(failure.loss().withheld_repair_fields, 0);
                assert!(!display.contains("for profile"));
                assert!(!display.contains("for the selected profile"));
            }
        }
    }
}
