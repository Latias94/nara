use crate::{ByteLimit, ItemLimit};

use super::{
    DiagnosticCode, DiagnosticDomain, DiagnosticField, DiagnosticFieldClass, DiagnosticFieldKey,
    DiagnosticProducer, DiagnosticReport, DiagnosticReportSettings, DiagnosticSettingsError,
    DiagnosticSeverity, DiagnosticValueRef, PressureMetricId, PressureSourceId,
    PublicDiagnosticIdentifier, ReportDiagnostic, RuntimeDiagnosticsSettings, SafeDisplayText,
    SafeSummary,
};

fn items(value: usize) -> ItemLimit {
    ItemLimit::new(value).expect("test limits are non-zero")
}

fn bytes(value: usize) -> ByteLimit {
    ByteLimit::new(value).expect("test limits are non-zero")
}

fn code(value: &'static str) -> DiagnosticCode {
    DiagnosticCode::new(value).expect("test diagnostic codes are valid")
}

fn field_key(value: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(value).expect("test diagnostic field keys are valid")
}

fn summary(value: &'static str) -> SafeSummary {
    SafeSummary::new(value).expect("test summaries are safe static text")
}

#[test]
fn safe_summary_constructor_requires_static_text_and_rejects_secret_shaped_literals() {
    let _: fn(&'static str) -> Result<SafeSummary, _> = SafeSummary::new;

    assert!(SafeSummary::new("Bearer test-token").is_err());
    assert!(SafeSummary::new("https://user:password@example.invalid/path").is_err());
    assert!(SafeSummary::new("value from SECRET_CANARY_ENV").is_err());
    assert!(SafeSummary::new("safe static summary").is_ok());
}

#[test]
fn safe_text_and_project_relative_fields_reject_unicode_format_spoofing() {
    for value in [
        "safe\u{061c}text",
        "safe\u{200b}text",
        "safe\u{2028}text",
        "safe\u{202e}text",
        "safe\u{2066}text",
        "safe\u{feff}text",
    ] {
        assert!(SafeSummary::new(value).is_err());
        assert!(SafeDisplayText::new(value).is_err());
        assert!(DiagnosticField::project_relative(field_key("path"), value).is_err());
    }
}

#[test]
fn stable_identities_reject_invalid_or_oversized_input_without_truncation() {
    const OVERSIZED: &str = concat!(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    let _: fn(&'static str) -> Result<DiagnosticCode, _> = DiagnosticCode::new;
    let _: fn(&'static str) -> Result<DiagnosticDomain, _> = DiagnosticDomain::new;
    let _: fn(&'static str) -> Result<DiagnosticProducer, _> = DiagnosticProducer::new;
    let _: fn(&'static str) -> Result<DiagnosticFieldKey, _> = DiagnosticFieldKey::new;
    let _: fn(&'static str) -> Result<PressureSourceId, _> = PressureSourceId::new;
    let _: fn(&'static str) -> Result<PressureMetricId, _> = PressureMetricId::new;

    for result in [
        DiagnosticCode::new("Bad Code").map(|_| ()),
        DiagnosticDomain::new("render/").map(|_| ()),
        DiagnosticProducer::new("window adapter").map(|_| ()),
        DiagnosticFieldKey::new("field=value").map(|_| ()),
        PressureSourceId::new("task pool").map(|_| ()),
        PressureMetricId::new("queue/depth").map(|_| ()),
        PublicDiagnosticIdentifier::new("Bearer token").map(|_| ()),
    ] {
        assert!(result.is_err());
    }

    assert!(DiagnosticCode::new(OVERSIZED).is_err());
    assert!(DiagnosticDomain::new(OVERSIZED).is_err());
    assert!(DiagnosticProducer::new(OVERSIZED).is_err());
    assert!(DiagnosticFieldKey::new(OVERSIZED).is_err());
    assert!(PressureSourceId::new(OVERSIZED).is_err());
    assert!(PressureMetricId::new(OVERSIZED).is_err());
}

#[test]
fn public_identifiers_accept_domain_locators_but_reject_paths_urls_and_secrets() {
    assert!(PublicDiagnosticIdentifier::new("Root/Player-1").is_ok());
    assert!(PublicDiagnosticIdentifier::new("nara.scene.Transform2D").is_ok());

    for value in [
        "/Root/Player",
        "Root//Player",
        "Root/../Player",
        "C:\\Users\\Alice",
        "\\\\server\\share",
        "https://example.invalid/id",
        "user@example.invalid",
        "BearerToken",
        "SecretCredential",
    ] {
        assert!(
            PublicDiagnosticIdentifier::new(value).is_err(),
            "unexpectedly accepted {value:?}"
        );
    }
}

#[test]
fn field_privacy_classes_only_retain_explicitly_safe_values() {
    let public = DiagnosticField::public_identifier(
        field_key("backend"),
        PublicDiagnosticIdentifier::new("vulkan-1").unwrap(),
    );
    let project =
        DiagnosticField::project_relative(field_key("asset-path"), "textures/ui/button.png")
            .unwrap();
    let sensitive = DiagnosticField::sensitive(field_key("user-path"));
    let secret = DiagnosticField::secret(field_key("access-token"));

    assert_eq!(public.class(), DiagnosticFieldClass::Public);
    assert_eq!(project.class(), DiagnosticFieldClass::ProjectRelative);
    assert_eq!(sensitive.class(), DiagnosticFieldClass::Sensitive);
    assert_eq!(secret.class(), DiagnosticFieldClass::Secret);
    assert!(matches!(sensitive.value(), DiagnosticValueRef::Redacted));
    assert!(matches!(secret.value(), DiagnosticValueRef::Redacted));
    assert_eq!(sensitive.display_value(), "[REDACTED]");
    assert_eq!(secret.display_value(), "[REDACTED]");
}

#[test]
fn project_relative_fields_reject_absolute_parent_and_platform_escape_forms() {
    for path in [
        "/home/alice/.config/nara/token",
        "../outside",
        "assets/../../outside",
        "C:\\Users\\alice\\secret.txt",
        "\\\\server\\share\\secret.txt",
        "\\\\?\\C:\\device-path",
        "\\\\.\\PhysicalDrive0",
        "https://user:password@example.invalid/file",
        "Bearer secret-token-canary",
        "NARA_SECRET_CANARY_ENV=value",
        "credential-value",
    ] {
        assert!(
            DiagnosticField::project_relative(field_key("path"), path).is_err(),
            "unexpectedly accepted {path:?}"
        );
    }
}

#[test]
fn local_diagnostic_reports_use_the_same_safe_bounded_content_model() {
    let settings = DiagnosticReportSettings::new(items(1), bytes(4096))
        .unwrap()
        .with_summary_byte_limit(bytes(5))
        .unwrap();
    let first = ReportDiagnostic::new(
        code("scene.invalid"),
        DiagnosticSeverity::Error,
        summary("界界"),
    )
    .try_with_field(DiagnosticField::secret(field_key("source")))
    .unwrap();
    let second = ReportDiagnostic::new(
        code("scene.second"),
        DiagnosticSeverity::Warning,
        summary("Second"),
    );
    let mut report = DiagnosticReport::new(settings);

    let first_outcome = report.push(first);
    let second_outcome = report.push(second);

    assert_eq!(first_outcome.truncated_text_bytes(), 3);
    assert_eq!(second_outcome.evicted_entries(), 1);
    assert_eq!(report.len(), 2);
    assert_eq!(report.retained_len(), 1);
    assert!(report.has_warnings());
    assert!(report.has_errors());
    assert_eq!(report.stats().evicted_entries(), 1);
}

#[test]
fn report_severity_is_sticky_across_eviction_and_rejection() {
    let mut evicting =
        DiagnosticReport::new(DiagnosticReportSettings::new(items(1), bytes(4096)).unwrap());
    evicting.push(ReportDiagnostic::error(
        code("sticky.error"),
        summary("Error"),
    ));
    evicting.push(ReportDiagnostic::warning(
        code("sticky.warning"),
        summary("Warning"),
    ));
    assert!(evicting.has_errors());
    assert!(evicting.has_warnings());
    assert_eq!(evicting.stats().observed_errors(), 1);

    let mut rejecting =
        DiagnosticReport::new(DiagnosticReportSettings::new(items(1), bytes(1)).unwrap());
    let outcome = rejecting.push(ReportDiagnostic::error(
        code("sticky.oversized"),
        summary("Oversized error"),
    ));
    assert!(outcome.rejection().is_some());
    assert!(!rejecting.is_empty());
    assert!(rejecting.is_retained_empty());
    assert_eq!(rejecting.len(), 1);
    assert!(rejecting.has_errors());
    assert_eq!(rejecting.stats().observed_errors(), 1);
    assert_eq!(rejecting.stats().rejected_entries(), 1);
}

#[test]
fn report_merge_propagates_unretained_source_severity() {
    let mut source =
        DiagnosticReport::new(DiagnosticReportSettings::new(items(1), bytes(4096)).unwrap());
    source.push(ReportDiagnostic::error(
        code("merge.lost-error"),
        summary("Lost error"),
    ));
    source.push(ReportDiagnostic::warning(
        code("merge.retained-warning"),
        summary("Retained warning"),
    ));
    assert!(source.has_errors());
    assert_eq!(source.len(), 2);
    assert_eq!(source.retained_len(), 1);

    let mut target =
        DiagnosticReport::new(DiagnosticReportSettings::new(items(4), bytes(4096)).unwrap());
    let outcome = target.extend(source);

    assert!(target.has_errors());
    assert!(target.has_warnings());
    assert_eq!(outcome.propagated_unretained_errors(), 1);
    assert_eq!(target.stats().observed_errors(), 1);
    assert_eq!(target.stats().observed_warnings(), 1);
}

#[test]
fn report_merge_combines_source_accounting_once_and_adds_only_target_budget_effects() {
    const BIG_DISPLAY: &str = concat!(
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
    );
    const HUGE_DISPLAY: &str = concat!(
        "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
        "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
        "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
        "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
        "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
        "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
        "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
        "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
        "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy",
        "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"
    );
    let source_settings = DiagnosticReportSettings::new(items(2), bytes(1024))
        .unwrap()
        .with_summary_byte_limit(bytes(5))
        .unwrap();
    let mut source = DiagnosticReport::new(source_settings);
    source.push(ReportDiagnostic::error(
        code("source.error"),
        summary("界界"),
    ));
    source.push(ReportDiagnostic::warning(
        code("source.warning"),
        summary("Warning"),
    ));
    source.push(
        ReportDiagnostic::info(code("source.info"), summary("Info"))
            .try_with_field(DiagnosticField::public_display(
                field_key("detail"),
                SafeDisplayText::new(BIG_DISPLAY).unwrap(),
            ))
            .unwrap(),
    );
    let rejected = ReportDiagnostic::error(code("source.huge"), summary("Huge"))
        .try_with_field(DiagnosticField::public_display(
            field_key("first"),
            SafeDisplayText::new(HUGE_DISPLAY).unwrap(),
        ))
        .unwrap()
        .try_with_field(DiagnosticField::public_display(
            field_key("second"),
            SafeDisplayText::new(HUGE_DISPLAY).unwrap(),
        ))
        .unwrap();
    assert!(source.push(rejected).rejection().is_some());
    let source_stats = source.stats();
    assert_eq!(source_stats.published_entries(), 3);
    assert_eq!(source_stats.rejected_entries(), 1);
    assert_eq!(source_stats.evicted_entries(), 1);
    assert_eq!(source_stats.truncated_text_bytes(), 5);

    let target_settings = DiagnosticReportSettings::new(items(4), bytes(200))
        .unwrap()
        .with_summary_byte_limit(bytes(1))
        .unwrap()
        .with_field_text_byte_limit(bytes(200))
        .unwrap();
    let mut target = DiagnosticReport::new(target_settings);
    let outcome = target.extend(source);
    let stats = target.stats();

    assert_eq!(stats.observed_errors(), 2);
    assert_eq!(stats.observed_warnings(), 1);
    assert_eq!(stats.observed_info(), 1);
    assert_eq!(stats.published_entries(), 3);
    assert_eq!(stats.rejected_entries(), 2);
    assert_eq!(stats.evicted_entries(), 1);
    assert_eq!(stats.truncated_fields(), 3);
    assert_eq!(stats.truncated_text_bytes(), 9);
    assert_eq!(outcome.rejected_entries(), 1);
    assert_eq!(outcome.truncated_text_bytes(), 4);
    assert_eq!(outcome.propagated_unretained_errors(), 2);
    assert_eq!(target.retained_len(), 1);
    assert!(target.has_errors());
    assert!(target.has_warnings());
    assert!(target.has_info());
}

#[test]
fn report_merge_republishes_entries_and_retained_consumption_is_explicit() {
    let source_settings = DiagnosticReportSettings::new(items(4), bytes(4096)).unwrap();
    let mut source = DiagnosticReport::new(source_settings);
    source.push(ReportDiagnostic::warning(
        code("merge.first"),
        summary("界界"),
    ));
    source.push(ReportDiagnostic::warning(
        code("merge.second"),
        summary("Second"),
    ));

    let target_settings = DiagnosticReportSettings::new(items(1), bytes(4096))
        .unwrap()
        .with_summary_byte_limit(bytes(5))
        .unwrap();
    let mut target = DiagnosticReport::new(target_settings);
    let outcome = target.extend(source);

    assert_eq!(outcome.attempted_entries(), 2);
    assert_eq!(outcome.published_entries(), 2);
    assert_eq!(outcome.rejected_entries(), 0);
    assert_eq!(outcome.evicted_entries(), 1);
    assert_eq!(outcome.truncated_fields(), 2);
    assert_eq!(outcome.truncated_text_bytes(), 4);
    assert_eq!(target.len(), 2);
    assert_eq!(target.retained_len(), 1);
    assert_eq!(target.iter().next().unwrap().summary().as_str(), "Secon");

    let equal = target.clone();
    assert_eq!(target, equal);
    assert_eq!((&equal).into_iter().count(), 1);
    let owned = equal.into_retained_diagnostics().collect::<Vec<_>>();
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].code().as_str(), "merge.second");
}

#[test]
fn runtime_settings_reject_hard_limit_escapes() {
    assert!(matches!(
        RuntimeDiagnosticsSettings::new(items(usize::MAX), bytes(4096)),
        Err(DiagnosticSettingsError::EntryLimitTooLarge { .. })
    ));
    assert!(matches!(
        RuntimeDiagnosticsSettings::new(items(4), bytes(usize::MAX)),
        Err(DiagnosticSettingsError::ByteLimitTooLarge { .. })
    ));
}

#[cfg(feature = "serde")]
#[test]
fn report_retains_its_serialized_entry_shape() {
    let mut report = DiagnosticReport::default();
    report.push(
        ReportDiagnostic::error(code("report.failed"), summary("Report failed"))
            .try_with_field(DiagnosticField::secret(field_key("detail")))
            .unwrap(),
    );

    let encoded = serde_json::to_value(&report).unwrap();
    assert_eq!(
        encoded["diagnostics"],
        serde_json::json!([{
            "code": "report.failed",
            "severity": "Error",
            "summary": "Report failed",
            "fields": [{"key": "detail", "class": "Secret", "value": {"Redacted": "[REDACTED]"}}],
        }])
    );
}
