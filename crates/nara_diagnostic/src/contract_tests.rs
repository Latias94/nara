use std::{
    fmt,
    num::NonZeroU64,
    sync::{Arc, Mutex},
};

use nara_app::{App, CoreStage, RealTime};
use nara_core::{ByteLimit, ItemLimit};
use nara_ecs::{
    Resource,
    schedule::IntoScheduleConfigs,
    system::{Res, ResMut},
};
use tracing::{Event, Metadata, Subscriber, field::Visit, span};

use crate::{
    Diagnostic, DiagnosticCleanupSet, DiagnosticCode, DiagnosticDomain, DiagnosticField,
    DiagnosticFieldClass, DiagnosticFieldKey, DiagnosticProducer, DiagnosticReport,
    DiagnosticReportSettings, DiagnosticSettingsError, DiagnosticSeverity, DiagnosticValueRef,
    DiagnosticsPlugin, PressureMeasurement, PressureMetricId, PressureMetricKind,
    PressurePublishRejection, PressureSourceId, PressureUnit, PublicDiagnosticIdentifier,
    RuntimeDiagnosticDraft, RuntimeDiagnosticFilter, RuntimeDiagnosticRetention,
    RuntimeDiagnostics, RuntimeDiagnosticsSettings, RuntimePressureSettings,
    RuntimePressureSnapshotDraft, RuntimePressureSnapshots, RuntimePublishOutcome, SafeDisplayText,
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

fn domain(value: &'static str) -> DiagnosticDomain {
    DiagnosticDomain::new(value).expect("test diagnostic domains are valid")
}

fn producer(value: &'static str) -> DiagnosticProducer {
    DiagnosticProducer::new(value).expect("test diagnostic producers are valid")
}

fn field_key(value: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(value).expect("test diagnostic field keys are valid")
}

fn metric(value: &'static str) -> PressureMetricId {
    PressureMetricId::new(value).expect("test pressure metric IDs are valid")
}

fn pressure_source(value: &'static str) -> PressureSourceId {
    PressureSourceId::new(value).expect("test pressure source IDs are valid")
}

fn summary(value: &'static str) -> SafeSummary {
    SafeSummary::new(value).expect("test summaries are safe static text")
}

fn runtime_settings(entry_limit: usize, byte_limit: usize) -> RuntimeDiagnosticsSettings {
    RuntimeDiagnosticsSettings::new(items(entry_limit), bytes(byte_limit))
        .expect("test runtime settings are within hard limits")
}

fn draft(
    producer_id: &'static str,
    domain_id: &'static str,
    code_id: &'static str,
    summary_text: &'static str,
) -> RuntimeDiagnosticDraft {
    RuntimeDiagnosticDraft::new(
        producer(producer_id),
        domain(domain_id),
        code(code_id),
        DiagnosticSeverity::Warning,
        summary(summary_text),
    )
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
fn sensitive_inputs_never_enter_debug_tracing_or_dedupe_visible_state() {
    let canaries = [
        "Bearer secret-token-canary",
        "https://user:password@example.invalid/private",
        "NARA_SECRET_CANARY_ENV=value",
        "/home/alice/private/key.pem",
        "C:\\Users\\alice\\private\\key.pem",
        "\\\\server\\private\\key.pem",
        "\\\\?\\C:\\private\\key.pem",
    ];
    let event = draft(
        "asset-watch",
        "asset",
        "asset.watch-failed",
        "Asset watch failed",
    )
    .try_with_field(DiagnosticField::sensitive(field_key("source-path")))
    .unwrap()
    .try_with_field(DiagnosticField::secret(field_key("credential")))
    .unwrap();
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(8, 4096));
    assert!(diagnostics.publish(event, 7).is_published());

    let debug = format!("{diagnostics:?}");
    let captured = Arc::new(Mutex::new(Vec::new()));
    let subscriber = RecordingSubscriber::new(Arc::clone(&captured));
    tracing::subscriber::with_default(subscriber, || {
        let mut cursor = diagnostics.tracing_cursor();
        assert_eq!(diagnostics.emit_new_to_tracing(&mut cursor), 1);
        assert_eq!(diagnostics.emit_new_to_tracing(&mut cursor), 0);
    });
    let tracing = captured.lock().unwrap().join("\n");

    for canary in canaries {
        assert!(!debug.contains(canary));
        assert!(!tracing.contains(canary));
    }
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn tracing_cursor_does_not_reemit_when_caught_up() {
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(4, 4096));
    diagnostics.publish(draft("runtime", "runtime", "runtime.first", "First"), 1);
    diagnostics.publish(draft("runtime", "runtime", "runtime.second", "Second"), 1);
    let captured = Arc::new(Mutex::new(Vec::new()));
    let subscriber = RecordingSubscriber::new(Arc::clone(&captured));

    tracing::subscriber::with_default(subscriber, || {
        let mut cursor = diagnostics.tracing_cursor();
        assert_eq!(diagnostics.emit_new_to_tracing(&mut cursor), 2);
        assert_eq!(diagnostics.emit_new_to_tracing(&mut cursor), 0);
        assert_eq!(diagnostics.emit_new_to_tracing(&mut cursor), 0);
    });

    assert_eq!(captured.lock().unwrap().len(), 2);
}

#[test]
fn tracing_cursor_resumes_after_a_middle_sequence_gap() {
    let settings = runtime_settings(4, 4096).with_retention(
        RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(1).unwrap()),
    );
    let mut diagnostics = RuntimeDiagnostics::new(settings);
    let repeated = draft("runtime", "runtime", "runtime.repeated", "Repeated")
        .with_dedupe_policy(crate::DiagnosticDedupePolicy::Code);
    diagnostics.publish(repeated.clone(), 1);
    let captured = Arc::new(Mutex::new(Vec::new()));
    let subscriber = RecordingSubscriber::new(Arc::clone(&captured));

    tracing::subscriber::with_default(subscriber, || {
        let mut cursor = diagnostics.tracing_cursor();
        assert_eq!(diagnostics.emit_new_to_tracing(&mut cursor), 1);

        diagnostics.publish(draft("runtime", "runtime", "runtime.expired", "Expired"), 1);
        diagnostics.publish(draft("runtime", "runtime", "runtime.latest", "Latest"), 3);
        diagnostics.publish(repeated, 3);
        diagnostics.maintain_for_test(4);

        assert_eq!(
            diagnostics
                .iter()
                .map(|entry| entry.sequence())
                .collect::<Vec<_>>(),
            [0, 2]
        );
        assert_eq!(diagnostics.emit_new_to_tracing(&mut cursor), 1);
        assert_eq!(diagnostics.emit_new_to_tracing(&mut cursor), 0);
    });

    assert_eq!(captured.lock().unwrap().len(), 2);
}

#[cfg(feature = "serde")]
#[test]
fn runtime_entries_and_snapshots_serialize_only_safe_bounded_state() {
    let event = draft("task-runtime", "task", "task.failed", "Task failed")
        .try_with_field(DiagnosticField::secret(field_key("panic-payload")))
        .unwrap();
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(4, 4096));
    diagnostics.publish(event, 1);

    let encoded = serde_json::to_string(&diagnostics.snapshot()).unwrap();

    assert!(encoded.contains("[REDACTED]"));
    assert!(!encoded.contains("dedupe"));
    assert!(!encoded.contains("panic payload canary"));
}

#[test]
fn utf8_summary_and_static_display_fields_truncate_on_scalar_boundaries_and_count_bytes() {
    let settings = runtime_settings(4, 4096)
        .with_summary_byte_limit(bytes(5))
        .unwrap()
        .with_field_text_byte_limit(bytes(7))
        .unwrap();
    let event = draft("render", "render", "render.failed", "ééé")
        .try_with_field(DiagnosticField::public_display(
            field_key("detail"),
            SafeDisplayText::new("界界界").unwrap(),
        ))
        .unwrap();
    let mut diagnostics = RuntimeDiagnostics::new(settings);

    let outcome = diagnostics.publish(event, 1);
    let entry = diagnostics.iter().next().unwrap();

    assert_eq!(entry.summary().as_str(), "éé");
    assert_eq!(entry.fields()[0].display_value(), "界界");
    assert_eq!(outcome.truncated_text_bytes(), 5);
    assert_eq!(diagnostics.stats().truncated_text_bytes(), 5);
    assert_eq!(diagnostics.stats().truncated_fields(), 2);
}

#[test]
fn utf8_text_uses_a_nonempty_safe_placeholder_when_the_first_scalar_does_not_fit() {
    let settings = runtime_settings(4, 4096)
        .with_summary_byte_limit(bytes(1))
        .unwrap()
        .with_field_text_byte_limit(bytes(1))
        .unwrap();
    let event = draft("render", "render", "render.failed", "界")
        .try_with_field(DiagnosticField::public_display(
            field_key("detail"),
            SafeDisplayText::new("界").unwrap(),
        ))
        .unwrap();
    let mut diagnostics = RuntimeDiagnostics::new(settings);

    let outcome = diagnostics.publish(event, 1);
    let entry = diagnostics.iter().next().unwrap();

    assert_eq!(entry.summary().as_str(), "?");
    assert_eq!(entry.fields()[0].display_value(), "?");
    assert_eq!(outcome.truncated_text_bytes(), 6);
}

#[test]
fn oversized_project_relative_fields_are_dropped_instead_of_forging_path_prefixes() {
    let settings = runtime_settings(4, 4096)
        .with_field_text_byte_limit(bytes(2))
        .unwrap();
    let event = draft("asset", "asset", "asset.failed", "Asset failed")
        .try_with_field(
            DiagnosticField::project_relative(field_key("first-path"), "..foo").unwrap(),
        )
        .unwrap()
        .try_with_field(
            DiagnosticField::project_relative(field_key("second-path"), "assets/..foo").unwrap(),
        )
        .unwrap();
    let mut diagnostics = RuntimeDiagnostics::new(settings);

    let outcome = diagnostics.publish(event, 1);
    let entry = diagnostics.iter().next().unwrap();

    assert!(entry.fields().is_empty());
    assert_eq!(outcome.dropped_fields(), 2);
    assert_eq!(outcome.truncated_text_bytes(), 17);
}

#[test]
fn runtime_field_count_limit_drops_tail_fields_deterministically() {
    let settings = runtime_settings(4, 4096)
        .with_field_limit(items(2))
        .unwrap();
    let event = draft(
        "project",
        "project",
        "project.invalid",
        "Project is invalid",
    )
    .try_with_field(DiagnosticField::public_u64(field_key("first"), 1))
    .unwrap()
    .try_with_field(DiagnosticField::public_u64(field_key("second"), 2))
    .unwrap()
    .try_with_field(DiagnosticField::public_u64(field_key("third"), 3))
    .unwrap();
    let mut diagnostics = RuntimeDiagnostics::new(settings);

    let outcome = diagnostics.publish(event, 2);
    let entry = diagnostics.iter().next().unwrap();

    assert_eq!(entry.fields().len(), 2);
    assert_eq!(entry.fields()[0].key().as_str(), "first");
    assert_eq!(entry.fields()[1].key().as_str(), "second");
    assert_eq!(outcome.dropped_fields(), 1);
}

#[test]
fn draft_hard_caps_fields_before_unbounded_allocation() {
    let mut event = draft(
        "project",
        "project",
        "project.invalid",
        "Project is invalid",
    );
    let mut rejection = None;
    const FIELD_KEYS: [&str; 33] = [
        "field-00", "field-01", "field-02", "field-03", "field-04", "field-05", "field-06",
        "field-07", "field-08", "field-09", "field-10", "field-11", "field-12", "field-13",
        "field-14", "field-15", "field-16", "field-17", "field-18", "field-19", "field-20",
        "field-21", "field-22", "field-23", "field-24", "field-25", "field-26", "field-27",
        "field-28", "field-29", "field-30", "field-31", "field-32",
    ];
    for (index, key) in FIELD_KEYS.into_iter().enumerate() {
        let key = field_key(key);
        match event
            .clone()
            .try_with_field(DiagnosticField::public_u64(key, index as u64))
        {
            Ok(next) => event = next,
            Err(error) => {
                rejection = Some(error);
                break;
            }
        }
    }

    assert!(rejection.is_some());
    assert_eq!(event.fields().len(), 32);
}

#[test]
fn runtime_count_limit_evicts_oldest_sequence() {
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(2, 4096));

    diagnostics.publish(draft("asset", "asset", "asset.first", "First"), 1);
    diagnostics.publish(draft("asset", "asset", "asset.second", "Second"), 2);
    let outcome = diagnostics.publish(draft("asset", "asset", "asset.third", "Third"), 3);

    assert_eq!(outcome.evicted_entries(), 1);
    assert_eq!(
        diagnostics
            .iter()
            .map(|entry| entry.code().as_str())
            .collect::<Vec<_>>(),
        ["asset.second", "asset.third"]
    );
}

#[test]
fn runtime_byte_limit_evicts_oldest_entry() {
    let first = draft("asset", "asset", "asset.same", "Same safe summary");
    let mut probe = RuntimeDiagnostics::new(runtime_settings(4, 4096));
    probe.publish(first.clone(), 1);
    let one_entry_bytes = probe.iter().next().unwrap().retained_bytes();
    let byte_limit = one_entry_bytes
        .checked_mul(2)
        .and_then(|value| value.checked_sub(1))
        .unwrap();
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(4, byte_limit));

    diagnostics.publish(first, 1);
    let outcome = diagnostics.publish(
        draft("asset", "asset", "asset.same-2", "Same safe summary"),
        2,
    );

    assert_eq!(outcome.evicted_entries(), 1);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics.retained_bytes() <= byte_limit);
    assert_eq!(diagnostics.iter().next().unwrap().last_frame(), 2);
}

#[test]
fn entry_larger_than_total_byte_budget_is_rejected_without_mutation() {
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(4, 1));

    let outcome = diagnostics.publish(
        draft("asset", "asset", "asset.failed", "Asset operation failed"),
        1,
    );

    assert!(outcome.is_rejected());
    assert_eq!(
        outcome.rejection().unwrap().to_string(),
        "diagnostic entry exceeds retained byte limit"
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn oversized_dedupe_hit_is_rejected_before_existing_state_changes() {
    const OVERSIZED_DISPLAY: &str = concat!(
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
    );
    let settings = runtime_settings(4, 128).with_retention(
        RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(2).unwrap()),
    );
    let mut diagnostics = RuntimeDiagnostics::new(settings);
    let original = draft("asset", "asset", "asset.failed", "Asset failed").dedupe_by_code();
    assert!(diagnostics.publish(original, 1).is_published());
    let before = diagnostics.stats();
    let before_expiry_index_len = diagnostics.expiry_index_len_for_test();

    let oversized = draft("asset", "asset", "asset.failed", "Asset failed")
        .dedupe_by_code()
        .try_with_field(DiagnosticField::public_display(
            field_key("detail"),
            SafeDisplayText::new(OVERSIZED_DISPLAY).unwrap(),
        ))
        .unwrap();
    let outcome = diagnostics.publish(oversized, 9);
    let after = diagnostics.stats();
    let retained = diagnostics.iter().next().unwrap();

    assert!(outcome.is_rejected());
    assert_eq!(
        outcome.rejection(),
        Some(crate::RuntimePublishRejection::EntryTooLarge)
    );
    assert_eq!(retained.repeat_count(), 1);
    assert_eq!(retained.last_frame(), 1);
    assert_eq!(after.published_entries(), before.published_entries());
    assert_eq!(after.deduplicated_entries(), before.deduplicated_entries());
    assert_eq!(after.rejected_entries(), before.rejected_entries() + 1);
    assert_eq!(after.truncated_text_bytes(), before.truncated_text_bytes());
    assert_eq!(
        diagnostics.expiry_index_len_for_test(),
        before_expiry_index_len
    );
}

#[test]
fn code_dedupe_includes_producer_domain_code_and_severity() {
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(8, 4096));

    diagnostics.publish(
        draft("watch", "asset", "io.failed", "Operation failed").dedupe_by_code(),
        1,
    );
    diagnostics.publish(
        draft("watch", "window", "io.failed", "Operation failed").dedupe_by_code(),
        2,
    );
    diagnostics.publish(
        draft("watch", "asset", "io.failed", "Operation failed").dedupe_by_code(),
        3,
    );

    assert_eq!(diagnostics.len(), 2);
    let asset = diagnostics
        .iter()
        .find(|entry| entry.domain().as_str() == "asset")
        .unwrap();
    assert_eq!(asset.repeat_count(), 2);
    assert_eq!(asset.first_frame(), 1);
    assert_eq!(asset.last_frame(), 3);
}

#[test]
fn runtime_filter_rejects_reversed_frame_ranges() {
    assert!(RuntimeDiagnosticFilter::new().frame_range(10, 5).is_err());
    assert!(RuntimeDiagnosticFilter::new().frame_range(5, 10).is_ok());
}

#[test]
fn dedupe_remains_stable_at_full_capacity_without_eviction() {
    const CODES: [&str; 8] = [
        "full.code-0",
        "full.code-1",
        "full.code-2",
        "full.code-3",
        "full.code-4",
        "full.code-5",
        "full.code-6",
        "full.code-7",
    ];
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(CODES.len(), 4096));
    for code_id in CODES {
        diagnostics.publish(
            draft("runtime", "runtime", code_id, "Runtime observation").dedupe_by_code(),
            1,
        );
    }
    let evictions_before = diagnostics.stats().evicted_entries();

    let outcome = diagnostics.publish(
        draft("runtime", "runtime", CODES[0], "Runtime observation").dedupe_by_code(),
        2,
    );

    assert_eq!(diagnostics.len(), CODES.len());
    assert_eq!(diagnostics.stats().evicted_entries(), evictions_before);
    assert_eq!(outcome.sequence(), Some(0));
    assert_eq!(diagnostics.iter().next().unwrap().repeat_count(), 2);
}

#[test]
fn runtime_retention_cleanup_reclaims_expired_capacity_before_admission() {
    let settings = runtime_settings(2, 4096).with_retention(
        RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(2).unwrap()),
    );
    let mut diagnostics = RuntimeDiagnostics::new(settings);
    diagnostics.publish(
        draft("runtime", "runtime", "runtime.active", "Active").dedupe_by_code(),
        1,
    );
    diagnostics.publish(draft("runtime", "runtime", "runtime.expired", "Expired"), 2);
    diagnostics.publish(
        draft("runtime", "runtime", "runtime.active", "Active").dedupe_by_code(),
        3,
    );

    diagnostics.maintain_for_test(5);
    let outcome = diagnostics.publish(draft("runtime", "runtime", "runtime.new", "New"), 5);

    assert!(outcome.is_published());
    assert_eq!(outcome.evicted_entries(), 0);
    assert_eq!(diagnostics.stats().expired_entries(), 1);
    assert_eq!(
        diagnostics
            .iter()
            .map(|entry| entry.code().as_str())
            .collect::<Vec<_>>(),
        ["runtime.active", "runtime.new"]
    );
}

#[test]
fn code_and_fields_dedupe_excludes_sensitive_and_secret_classes() {
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(8, 4096));
    for frame in [1, 2] {
        let event = draft(
            "project",
            "project",
            "project.denied",
            "Project access denied",
        )
        .dedupe_by_code_and_fields()
        .try_with_field(DiagnosticField::public_u64(field_key("attempt"), 7))
        .unwrap()
        .try_with_field(DiagnosticField::sensitive(field_key("root")))
        .unwrap()
        .try_with_field(DiagnosticField::secret(field_key("credential")))
        .unwrap();
        diagnostics.publish(event, frame);
    }

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics.iter().next().unwrap().repeat_count(), 2);
}

#[test]
fn code_and_fields_dedupe_distinguishes_field_class_and_value_variant() {
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(8, 4096));
    let identifier = draft("asset", "asset", "asset.failed", "Asset failed")
        .dedupe_by_code_and_fields()
        .try_with_field(DiagnosticField::public_identifier(
            field_key("subject"),
            PublicDiagnosticIdentifier::new("assets/item").unwrap(),
        ))
        .unwrap();
    let display = draft("asset", "asset", "asset.failed", "Asset failed")
        .dedupe_by_code_and_fields()
        .try_with_field(DiagnosticField::public_display(
            field_key("subject"),
            SafeDisplayText::new("assets/item").unwrap(),
        ))
        .unwrap();
    let project_relative = draft("asset", "asset", "asset.failed", "Asset failed")
        .dedupe_by_code_and_fields()
        .try_with_field(
            DiagnosticField::project_relative(field_key("subject"), "assets/item").unwrap(),
        )
        .unwrap();

    diagnostics.publish(identifier, 1);
    diagnostics.publish(display, 2);
    diagnostics.publish(project_relative, 3);

    assert_eq!(diagnostics.len(), 3);
    assert!(diagnostics.iter().all(|entry| entry.repeat_count() == 1));
}

#[test]
fn frame_window_retention_expires_entries_but_manual_retention_does_not() {
    let window = RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(2).unwrap());
    let mut expiring = RuntimeDiagnostics::new(runtime_settings(8, 4096).with_retention(window));
    expiring.publish(draft("asset", "asset", "asset.old", "Old"), 3);
    expiring.publish(draft("asset", "asset", "asset.new", "New"), 5);

    expiring.maintain_for_test(6);

    assert_eq!(expiring.len(), 1);
    assert_eq!(expiring.iter().next().unwrap().code().as_str(), "asset.new");
    assert_eq!(expiring.stats().expired_entries(), 1);

    let mut manual = RuntimeDiagnostics::new(
        runtime_settings(8, 4096).with_retention(RuntimeDiagnosticRetention::Manual),
    );
    manual.publish(draft("asset", "asset", "asset.old", "Old"), 3);
    manual.maintain_for_test(u64::MAX);
    assert_eq!(manual.len(), 1);
}

#[test]
fn runtime_expiry_index_tracks_dedupe_updates_without_growth() {
    let settings = runtime_settings(8, 4096).with_retention(
        RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(2).unwrap()),
    );
    let mut diagnostics = RuntimeDiagnostics::new(settings);
    let event = draft("asset", "asset", "asset.retry", "Asset retry").dedupe_by_code();

    diagnostics.publish(event.clone(), 1);
    assert_eq!(diagnostics.expiry_index_len_for_test(), 1);
    diagnostics.publish(event, 3);
    assert_eq!(diagnostics.expiry_index_len_for_test(), 1);
    assert_eq!(diagnostics.len(), 1);

    diagnostics.maintain_for_test(4);
    assert_eq!(
        diagnostics.len(),
        1,
        "the replaced expiry key must be removed"
    );
    assert_eq!(diagnostics.expiry_index_len_for_test(), 1);
    diagnostics.maintain_for_test(6);
    assert!(diagnostics.is_empty());
    assert_eq!(diagnostics.expiry_index_len_for_test(), 0);
}

#[test]
fn runtime_expiry_maintenance_only_removes_due_entries() {
    let settings = runtime_settings(8, 4096).with_retention(
        RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(2).unwrap()),
    );
    let mut diagnostics = RuntimeDiagnostics::new(settings);
    diagnostics.publish(draft("runtime", "runtime", "runtime.first", "First"), 1);
    diagnostics.publish(draft("runtime", "runtime", "runtime.second", "Second"), 3);
    diagnostics.publish(draft("runtime", "runtime", "runtime.third", "Third"), 5);

    let before_entries = diagnostics.snapshot().entries().to_vec();
    let before_index_len = diagnostics.expiry_index_len_for_test();
    let before_order_len = diagnostics.order_storage_len_for_test();
    diagnostics.maintain_for_test(3);
    assert_eq!(diagnostics.snapshot().entries(), before_entries);
    assert_eq!(diagnostics.expiry_index_len_for_test(), before_index_len);
    assert_eq!(diagnostics.order_storage_len_for_test(), before_order_len);

    diagnostics.maintain_for_test(4);
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics.expiry_index_len_for_test(), 2);
    assert_eq!(
        diagnostics
            .iter()
            .map(|entry| entry.code().as_str())
            .collect::<Vec<_>>(),
        vec!["runtime.second", "runtime.third"]
    );
}

#[test]
fn runtime_order_tombstones_stay_amortized_and_hard_bounded() {
    let settings = runtime_settings(8, 4096).with_retention(
        RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(1).unwrap()),
    );
    let mut diagnostics = RuntimeDiagnostics::new(settings);
    let event = draft("runtime", "runtime", "runtime.cycle", "Cycle");
    for frame in 100..108 {
        diagnostics.publish(event.clone(), frame);
    }

    let mut observed_compaction = false;
    let mut previous_tombstones = 0;
    for offset in 0..32 {
        diagnostics.maintain_for_test(102 + offset);
        let active = diagnostics.len();
        let tombstones = diagnostics.order_tombstones_for_test();
        let order_len = diagnostics.order_storage_len_for_test();
        assert_eq!(order_len, active + tombstones);
        assert!(tombstones <= active);
        assert!(order_len <= active.saturating_mul(2));
        assert_eq!(diagnostics.iter().count(), active);
        assert_eq!(diagnostics.expiry_index_len_for_test(), active);
        observed_compaction |= previous_tombstones > 0 && tombstones == 0;
        previous_tombstones = tombstones;

        diagnostics.publish(event.clone(), 108 + offset);
        assert_eq!(diagnostics.len(), 8);
    }
    assert!(observed_compaction);
}

#[test]
fn byte_eviction_rechecks_tombstone_bound_before_inserting() {
    const LARGE_DISPLAY: &str = concat!(
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
    );
    let large = draft("runtime", "runtime", "runtime.large", "Large")
        .try_with_field(DiagnosticField::public_display(
            field_key("detail"),
            SafeDisplayText::new(LARGE_DISPLAY).unwrap(),
        ))
        .unwrap();
    let mut probe = RuntimeDiagnostics::new(runtime_settings(1, 4096));
    assert!(probe.publish(large.clone(), 100).is_published());
    let large_entry_bytes = probe.iter().next().unwrap().retained_bytes();

    let settings = runtime_settings(8, large_entry_bytes).with_retention(
        RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(1).unwrap()),
    );
    let mut diagnostics = RuntimeDiagnostics::new(settings);
    let refreshed = draft("runtime", "runtime", "runtime.refreshed", "Refreshed").dedupe_by_code();
    assert!(diagnostics.publish(refreshed.clone(), 1).is_published());
    assert!(
        diagnostics
            .publish(draft("runtime", "runtime", "runtime.live", "Live"), 100)
            .is_published()
    );
    assert!(
        diagnostics
            .publish(draft("runtime", "runtime", "runtime.expired", "Expired"), 1)
            .is_published()
    );
    assert!(matches!(
        diagnostics.publish(refreshed, 100),
        RuntimePublishOutcome::Deduplicated { .. }
    ));

    diagnostics.maintain_for_test(3);
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics.order_tombstones_for_test(), 1);
    assert_eq!(diagnostics.order_storage_len_for_test(), 3);

    let outcome = diagnostics.publish(large, 100);
    assert_eq!(outcome.evicted_entries(), 2);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics.order_tombstones_for_test(), 0);
    assert_eq!(diagnostics.order_storage_len_for_test(), 1);
    assert_eq!(diagnostics.expiry_index_len_for_test(), 1);
    assert_eq!(
        diagnostics.iter().next().unwrap().code().as_str(),
        "runtime.large"
    );
}

#[test]
fn overflowing_runtime_expiry_is_never_reached_by_a_u64_watermark() {
    let settings = runtime_settings(4, 4096).with_retention(
        RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(1).unwrap()),
    );
    let mut diagnostics = RuntimeDiagnostics::new(settings);
    diagnostics.publish(
        draft("runtime", "runtime", "runtime.last-frame", "Last frame"),
        u64::MAX,
    );

    assert_eq!(diagnostics.expiry_index_len_for_test(), 0);
    diagnostics.maintain_for_test(u64::MAX);
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn cleanup_watermark_rejects_expired_diagnostics_without_mutating_retained_state() {
    let settings = runtime_settings(4, 4096).with_retention(
        RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(2).unwrap()),
    );
    let mut diagnostics = RuntimeDiagnostics::new(settings);
    diagnostics.publish(draft("runtime", "runtime", "runtime.old", "Old"), 5);
    diagnostics.maintain_for_test(10);
    let before = diagnostics.stats();

    let outcome = diagnostics.publish(draft("runtime", "runtime", "runtime.expired", "Expired"), 7);
    let after = diagnostics.stats();

    assert!(matches!(
        outcome.rejection(),
        Some(crate::RuntimePublishRejection::ExpiredFrame { .. })
    ));
    assert!(diagnostics.is_empty());
    assert_eq!(after.published_entries(), before.published_entries());
    assert_eq!(after.evicted_entries(), before.evicted_entries());
    assert_eq!(after.rejected_entries(), before.rejected_entries() + 1);
}

#[test]
fn sequence_exhaustion_is_typed_and_never_wraps() {
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(4, 4096));
    diagnostics.set_next_sequence_for_test(Some(u64::MAX));

    let final_outcome =
        diagnostics.publish(draft("asset", "asset", "asset.final", "Final sequence"), 1);
    let rejected = diagnostics.publish(
        draft("asset", "asset", "asset.overflow", "Sequence exhausted"),
        2,
    );

    assert_eq!(final_outcome.sequence(), Some(u64::MAX));
    assert!(rejected.is_sequence_exhausted());
    assert_eq!(diagnostics.len(), 1);
}

#[test]
fn repeat_and_statistics_counters_saturate() {
    let mut diagnostics = RuntimeDiagnostics::new(runtime_settings(4, 4096));
    let event = draft("asset", "asset", "asset.failed", "Asset failed").dedupe_by_code();
    let first = diagnostics.publish(event.clone(), 1);
    let sequence = first.sequence().unwrap();
    diagnostics.set_repeat_count_for_test(sequence, u64::MAX);
    diagnostics.set_stats_for_test(u64::MAX);

    diagnostics.publish(event, 2);

    assert_eq!(diagnostics.iter().next().unwrap().repeat_count(), u64::MAX);
    assert_eq!(diagnostics.stats().published_entries(), u64::MAX);
    assert_eq!(diagnostics.stats().deduplicated_entries(), u64::MAX);
}

#[test]
fn local_diagnostic_reports_use_the_same_safe_bounded_content_model() {
    let settings = DiagnosticReportSettings::new(items(1), bytes(4096))
        .unwrap()
        .with_summary_byte_limit(bytes(5))
        .unwrap();
    let first = Diagnostic::new(
        code("scene.invalid"),
        DiagnosticSeverity::Error,
        summary("界界"),
    )
    .try_with_field(DiagnosticField::secret(field_key("source")))
    .unwrap();
    let second = Diagnostic::new(
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
    evicting.push(Diagnostic::error(code("sticky.error"), summary("Error")));
    evicting.push(Diagnostic::warning(
        code("sticky.warning"),
        summary("Warning"),
    ));
    assert!(evicting.has_errors());
    assert!(evicting.has_warnings());
    assert_eq!(evicting.stats().observed_errors(), 1);

    let mut rejecting =
        DiagnosticReport::new(DiagnosticReportSettings::new(items(1), bytes(1)).unwrap());
    let outcome = rejecting.push(Diagnostic::error(
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
    source.push(Diagnostic::error(
        code("merge.lost-error"),
        summary("Lost error"),
    ));
    source.push(Diagnostic::warning(
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
    source.push(Diagnostic::error(code("source.error"), summary("界界")));
    source.push(Diagnostic::warning(
        code("source.warning"),
        summary("Warning"),
    ));
    source.push(
        Diagnostic::info(code("source.info"), summary("Info"))
            .try_with_field(DiagnosticField::public_display(
                field_key("detail"),
                SafeDisplayText::new(BIG_DISPLAY).unwrap(),
            ))
            .unwrap(),
    );
    let rejected = Diagnostic::error(code("source.huge"), summary("Huge"))
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
    source.push(Diagnostic::warning(code("merge.first"), summary("界界")));
    source.push(Diagnostic::warning(code("merge.second"), summary("Second")));

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

#[test]
fn pressure_snapshots_replace_each_source_atomically() {
    let settings = RuntimePressureSettings::new(items(4), items(1)).unwrap();
    let mut snapshots = RuntimePressureSnapshots::new(settings);
    let source = pressure_source("task-runtime");
    let first = RuntimePressureSnapshotDraft::new(source)
        .try_with_measurement(PressureMeasurement::gauge(
            metric("pending-items"),
            PressureUnit::Items,
            3,
        ))
        .unwrap();
    assert!(snapshots.publish(first, 1).is_inserted());

    let invalid_replacement = RuntimePressureSnapshotDraft::new(source)
        .try_with_measurement(PressureMeasurement::gauge(
            metric("pending-items"),
            PressureUnit::Items,
            5,
        ))
        .unwrap()
        .try_with_measurement(PressureMeasurement::counter(
            metric("rejected-items"),
            PressureUnit::Items,
            7,
        ))
        .unwrap();
    let rejected = snapshots.publish(invalid_replacement, 2);

    assert!(matches!(
        rejected.rejection(),
        Some(PressurePublishRejection::MeasurementLimitExceeded { .. })
    ));
    let retained = snapshots.get(&source).unwrap();
    assert_eq!(retained.frame(), 1);
    assert_eq!(retained.measurements()[0].value(), 3);

    let replacement = RuntimePressureSnapshotDraft::new(source)
        .try_with_measurement(PressureMeasurement::counter(
            metric("rejected-items"),
            PressureUnit::Items,
            7,
        ))
        .unwrap();
    assert!(snapshots.publish(replacement, 3).is_replaced());
    assert_eq!(snapshots.get(&source).unwrap().frame(), 3);
    assert_eq!(
        snapshots.get(&source).unwrap().measurements()[0].kind(),
        PressureMetricKind::Counter
    );
}

#[test]
fn pressure_expiry_index_replacement_is_atomic_and_does_not_grow() {
    let settings = RuntimePressureSettings::new(items(4), items(1))
        .unwrap()
        .with_retention_frame_window(NonZeroU64::new(2).unwrap());
    let mut snapshots = RuntimePressureSnapshots::new(settings);
    let source = pressure_source("task-runtime");
    let first = RuntimePressureSnapshotDraft::new(source).try_with_measurement(
        PressureMeasurement::gauge(metric("pending-items"), PressureUnit::Items, 3),
    );
    snapshots.publish(first.unwrap(), 1);
    assert_eq!(snapshots.expiry_index_len_for_test(), 1);

    let rejected = RuntimePressureSnapshotDraft::new(source)
        .try_with_measurement(PressureMeasurement::gauge(
            metric("pending-items"),
            PressureUnit::Items,
            5,
        ))
        .unwrap()
        .try_with_measurement(PressureMeasurement::counter(
            metric("rejected-items"),
            PressureUnit::Items,
            7,
        ))
        .unwrap();
    assert!(snapshots.publish(rejected, 2).is_rejected());
    assert_eq!(snapshots.expiry_index_len_for_test(), 1);
    assert_eq!(snapshots.get(&source).unwrap().frame(), 1);

    assert!(
        snapshots
            .publish(RuntimePressureSnapshotDraft::new(source), 3)
            .is_replaced()
    );
    assert_eq!(snapshots.expiry_index_len_for_test(), 1);
    snapshots.maintain_for_test(4);
    assert!(snapshots.get(&source).is_some());
    assert_eq!(snapshots.expiry_index_len_for_test(), 1);
    snapshots.maintain_for_test(6);
    assert!(snapshots.get(&source).is_none());
    assert_eq!(snapshots.expiry_index_len_for_test(), 0);
}

#[test]
fn pressure_expiry_maintenance_only_removes_due_sources() {
    let settings = RuntimePressureSettings::new(items(4), items(1))
        .unwrap()
        .with_retention_frame_window(NonZeroU64::new(2).unwrap());
    let mut snapshots = RuntimePressureSnapshots::new(settings);
    let first = pressure_source("first");
    let second = pressure_source("second");
    let third = pressure_source("third");
    snapshots.publish(RuntimePressureSnapshotDraft::new(first), 1);
    snapshots.publish(RuntimePressureSnapshotDraft::new(second), 3);
    snapshots.publish(RuntimePressureSnapshotDraft::new(third), 5);

    let before = snapshots.snapshot().snapshots().to_vec();
    let before_index_len = snapshots.expiry_index_len_for_test();
    snapshots.maintain_for_test(3);
    assert_eq!(snapshots.snapshot().snapshots(), before);
    assert_eq!(snapshots.expiry_index_len_for_test(), before_index_len);

    snapshots.maintain_for_test(4);
    assert!(snapshots.get(&first).is_none());
    assert!(snapshots.get(&second).is_some());
    assert!(snapshots.get(&third).is_some());
    assert_eq!(snapshots.expiry_index_len_for_test(), 2);
}

#[test]
fn overflowing_pressure_expiry_is_never_reached_by_a_u64_watermark() {
    let settings = RuntimePressureSettings::new(items(1), items(1))
        .unwrap()
        .with_retention_frame_window(NonZeroU64::new(1).unwrap());
    let mut snapshots = RuntimePressureSnapshots::new(settings);
    let source = pressure_source("last-frame");
    snapshots.publish(RuntimePressureSnapshotDraft::new(source), u64::MAX);

    assert_eq!(snapshots.expiry_index_len_for_test(), 0);
    snapshots.maintain_for_test(u64::MAX);
    assert!(snapshots.get(&source).is_some());
}

#[test]
fn pressure_stale_and_expired_frames_preserve_retained_state() {
    let settings = RuntimePressureSettings::new(items(2), items(2))
        .unwrap()
        .with_retention_frame_window(NonZeroU64::new(2).unwrap());
    let mut snapshots = RuntimePressureSnapshots::new(settings);
    let source = pressure_source("task-runtime");
    snapshots.publish(
        RuntimePressureSnapshotDraft::new(source)
            .try_with_measurement(PressureMeasurement::gauge(
                metric("pending-items"),
                PressureUnit::Items,
                5,
            ))
            .unwrap(),
        5,
    );

    let stale = snapshots.publish(
        RuntimePressureSnapshotDraft::new(source)
            .try_with_measurement(PressureMeasurement::gauge(
                metric("pending-items"),
                PressureUnit::Items,
                4,
            ))
            .unwrap(),
        4,
    );
    assert!(matches!(
        stale.rejection(),
        Some(PressurePublishRejection::StaleFrame { .. })
    ));
    assert_eq!(snapshots.get(&source).unwrap().frame(), 5);
    assert_eq!(snapshots.get(&source).unwrap().measurements()[0].value(), 5);

    snapshots.maintain_for_test(10);
    let before = snapshots.stats();
    let expired = snapshots.publish(RuntimePressureSnapshotDraft::new(source), 7);
    let after = snapshots.stats();
    assert!(matches!(
        expired.rejection(),
        Some(PressurePublishRejection::ExpiredFrame { .. })
    ));
    assert!(snapshots.get(&source).is_none());
    assert_eq!(after.inserted_snapshots(), before.inserted_snapshots());
    assert_eq!(after.replaced_snapshots(), before.replaced_snapshots());
    assert_eq!(after.rejected_snapshots(), before.rejected_snapshots() + 1);
}

#[test]
fn pressure_source_and_measurement_bounds_are_typed() {
    let settings = RuntimePressureSettings::new(items(1), items(1)).unwrap();
    let mut snapshots = RuntimePressureSnapshots::new(settings);
    let first = RuntimePressureSnapshotDraft::new(pressure_source("task"))
        .try_with_measurement(PressureMeasurement::gauge(
            metric("depth"),
            PressureUnit::Depth,
            1,
        ))
        .unwrap();
    snapshots.publish(first, 1);

    let second = RuntimePressureSnapshotDraft::new(pressure_source("render"))
        .try_with_measurement(PressureMeasurement::gauge(
            metric("bytes"),
            PressureUnit::Bytes,
            2,
        ))
        .unwrap();
    let rejected = snapshots.publish(second, 2);

    assert!(matches!(
        rejected.rejection(),
        Some(PressurePublishRejection::SourceLimitReached { .. })
    ));
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots.stats().rejected_snapshots(), 1);
}

#[test]
fn pressure_retention_cleanup_reclaims_expired_sources_before_capacity_check() {
    let settings = RuntimePressureSettings::new(items(1), items(1))
        .unwrap()
        .with_retention_frame_window(NonZeroU64::new(2).unwrap());
    let mut snapshots = RuntimePressureSnapshots::new(settings);
    assert!(
        snapshots
            .publish(
                RuntimePressureSnapshotDraft::new(pressure_source("old-source")),
                1,
            )
            .is_inserted()
    );

    snapshots.maintain_for_test(5);
    let outcome = snapshots.publish(
        RuntimePressureSnapshotDraft::new(pressure_source("new-source")),
        5,
    );

    assert!(outcome.is_inserted());
    assert!(snapshots.get(&pressure_source("old-source")).is_none());
    assert!(snapshots.get(&pressure_source("new-source")).is_some());
    assert_eq!(snapshots.stats().expired_snapshots(), 1);
}

#[test]
fn pressure_units_are_numeric_and_complete() {
    let units = [
        PressureUnit::Count,
        PressureUnit::Items,
        PressureUnit::Bytes,
        PressureUnit::Depth,
        PressureUnit::Nanoseconds,
        PressureUnit::Frames,
    ];

    assert_eq!(units.len(), 6);
    let metrics = [
        "metric-count",
        "metric-items",
        "metric-bytes",
        "metric-depth",
        "metric-nanos",
        "metric-frames",
    ];
    for (index, (unit, metric_id)) in units.into_iter().zip(metrics).enumerate() {
        let measurement = PressureMeasurement::gauge(metric(metric_id), unit, index as u64);
        assert_eq!(measurement.unit(), unit);
        assert_eq!(measurement.value(), index as u64);
    }
}

#[test]
fn pressure_retention_expires_by_frame_and_stats_saturate() {
    let settings = RuntimePressureSettings::new(items(2), items(2))
        .unwrap()
        .with_retention_frame_window(NonZeroU64::new(2).unwrap());
    let mut snapshots = RuntimePressureSnapshots::new(settings);
    let old = RuntimePressureSnapshotDraft::new(pressure_source("old"));
    let new = RuntimePressureSnapshotDraft::new(pressure_source("new"));
    snapshots.publish(old, 3);
    snapshots.publish(new, 5);
    snapshots.set_stats_for_test(u64::MAX);

    snapshots.maintain_for_test(6);

    assert!(snapshots.get(&pressure_source("old")).is_none());
    assert!(snapshots.get(&pressure_source("new")).is_some());
    assert_eq!(snapshots.stats().expired_snapshots(), u64::MAX);
}

#[test]
fn diagnostics_plugin_installs_headless_resources_and_frame_retention() {
    let runtime = runtime_settings(8, 4096).with_retention(
        RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(1).unwrap()),
    );
    let pressure = RuntimePressureSettings::new(items(4), items(4))
        .unwrap()
        .with_retention_frame_window(NonZeroU64::new(1).unwrap());
    let mut app = App::new();
    app.add_plugin(DiagnosticsPlugin::new(runtime, pressure))
        .unwrap();

    app.world_mut()
        .unwrap()
        .resource_mut::<RuntimeDiagnostics>()
        .publish(draft("task", "task", "task.old", "Old task failure"), 0);
    app.world_mut()
        .unwrap()
        .resource_mut::<RuntimePressureSnapshots>()
        .publish(
            RuntimePressureSnapshotDraft::new(pressure_source("task")),
            0,
        );

    app.update().unwrap();
    assert_eq!(app.world().resource::<RuntimeDiagnostics>().len(), 1);
    assert_eq!(app.world().resource::<RuntimePressureSnapshots>().len(), 1);

    app.update().unwrap();
    assert!(app.world().resource::<RuntimeDiagnostics>().is_empty());
    assert!(
        app.world()
            .resource::<RuntimePressureSnapshots>()
            .is_empty()
    );
}

#[derive(Default, Resource)]
struct FirstStagePublishProbe {
    outcome: Option<RuntimePublishOutcome>,
}

fn publish_after_diagnostic_retention(
    real_time: Res<RealTime>,
    mut diagnostics: ResMut<RuntimeDiagnostics>,
    mut probe: ResMut<FirstStagePublishProbe>,
) {
    if real_time.frame == 2 {
        probe.outcome = Some(diagnostics.publish(
            draft("runtime", "runtime", "runtime.new", "New observation"),
            real_time.frame,
        ));
    }
}

#[test]
fn first_stage_retention_runs_before_declared_producers() {
    let runtime = runtime_settings(1, 4096).with_retention(
        RuntimeDiagnosticRetention::frame_window(NonZeroU64::new(1).unwrap()),
    );
    let pressure = RuntimePressureSettings::new(items(1), items(1)).unwrap();
    let mut app = App::new();
    app.add_plugin(DiagnosticsPlugin::new(runtime, pressure))
        .unwrap();
    app.insert_resource(FirstStagePublishProbe::default())
        .unwrap();
    app.add_systems(
        CoreStage::First,
        publish_after_diagnostic_retention.after(DiagnosticCleanupSet::Retention),
    )
    .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<RuntimeDiagnostics>()
        .publish(
            draft("runtime", "runtime", "runtime.old", "Old observation"),
            0,
        );

    app.update().unwrap();
    app.update().unwrap();

    let probe = app.world().resource::<FirstStagePublishProbe>();
    let outcome = probe.outcome.expect("frame two producer should publish");
    assert!(outcome.is_published());
    assert_eq!(outcome.evicted_entries(), 0);
    let diagnostics = app.world().resource::<RuntimeDiagnostics>();
    assert_eq!(diagnostics.stats().expired_entries(), 1);
    assert_eq!(
        diagnostics.iter().next().unwrap().code().as_str(),
        "runtime.new"
    );
}

#[derive(Clone)]
struct RecordingSubscriber {
    events: Arc<Mutex<Vec<String>>>,
}

impl RecordingSubscriber {
    fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

#[derive(Default)]
struct FieldRecorder {
    output: String,
}

impl Visit for FieldRecorder {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        use fmt::Write as _;
        let _ = write!(self.output, "{}={value:?};", field.name());
    }
}

impl Subscriber for RecordingSubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut recorder = FieldRecorder::default();
        event.record(&mut recorder);
        self.events.lock().unwrap().push(recorder.output);
    }

    fn enter(&self, _span: &span::Id) {}

    fn exit(&self, _span: &span::Id) {}
}
