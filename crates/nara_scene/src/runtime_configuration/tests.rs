use nara_app::tasks::{
    MAX_TASK_POOL_PENDING_PER_KIND, MAX_TASK_POOL_THREADS_PER_KIND,
    MAX_TASK_SHUTDOWN_PHASE_TIMEOUT, TaskPoolKind,
};
use nara_asset::StableAssetId;
use nara_core::diagnostic::{DiagnosticValueRef, MAX_RUNTIME_DIAGNOSTIC_ENTRIES};

use super::*;

const COMPLETE_CONFIGURATION: &str = r#"
[capabilities]
requested = ["runtime-2d", "runtime-ui", "render-wgpu", "presentation-2d"]

[runtime]
paused = true
time_scale = 0.75
max_delta = { secs = 0, nanos = 123456789 }
fixed_timestep = { secs = 0, nanos = 16666667 }
max_fixed_steps_per_frame = 8
max_fixed_debt_steps = 64
catch_up_policy = "preserve-debt"
preset = "local-headless"

[tasks.io]
workers = 2
pending_capacity = 31
[tasks.compute]
workers = 3
pending_capacity = 32
[tasks.async_compute]
workers = 4
pending_capacity = 33
[tasks.shutdown]
drain_timeout_ms = 101
cancel_timeout_ms = 102
join_timeout_ms = 103

[render.world_canvas]
width = 320
height = 180

[ui.primary_surface]
logical_width = 960
logical_height = 540
[ui.primary_surface.safe_area]
left = 11
top = 12
right = 13
bottom = 14

[window]
enabled = false
title = "Independent runtime"
width = 1920
height = 1080
scale_factor = 1.2500000000000002
mode = "borderless-fullscreen"
present_mode = "mailbox"
resizable = false

[input.action_map]
source = "919617ea-c89c-4f83-9cce-b084de435a83"
[input.action_map.product]
kind = "primary"

[diagnostics]
runtime_capacity = 37
"#;

fn complete_configuration() -> RuntimeConfiguration {
    toml::from_str(COMPLETE_CONFIGURATION).unwrap()
}

fn primary_action_map() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str("919617ea-c89c-4f83-9cce-b084de435a83").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn assert_diagnostic(failure: &RuntimeConfigurationError, code: &str, field: &str) {
    assert!(failure.diagnostics().has_errors());
    assert!(
        failure.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().as_str() == code
                && diagnostic.fields().iter().any(|entry| {
                    entry.key().as_str() == "field"
                        && matches!(entry.value(), DiagnosticValueRef::Identifier(value) if value == field)
                })
        }),
        "missing {code} at {field}: {failure:?}"
    );
}

fn assert_json_round_trip(effective: &EffectiveRuntimeConfiguration) {
    let raw = effective.to_configuration();
    assert_eq!(&raw.resolve().unwrap(), effective);
    let json = serde_json::to_string(&raw).unwrap();
    let decoded: RuntimeConfiguration = serde_json::from_str(&json).unwrap();
    assert_eq!(&decoded.resolve().unwrap(), effective);
}

fn assert_round_trip(effective: &EffectiveRuntimeConfiguration) {
    assert_json_round_trip(effective);
    let raw = effective.to_configuration();
    let toml = toml::to_string(&raw).unwrap();
    let decoded: RuntimeConfiguration = toml::from_str(&toml).unwrap();
    assert_eq!(&decoded.resolve().unwrap(), effective);
}

#[test]
fn defaults_resolve_without_a_source_project() {
    let effective = RuntimeConfiguration::default().resolve().unwrap();
    assert_eq!(effective.runtime_preset(), RuntimePreset::Minimal);
    assert!(effective.requested_capabilities().is_empty());
    assert_eq!(effective.runtime().fixed_time(), FixedTime::default());
    assert_eq!(
        effective.runtime().runtime_time_settings(),
        RuntimeTimeSettings::default()
    );
    assert_eq!(
        effective.tasks().pool_config,
        nara_app::tasks::TaskPoolConfig::default()
    );
    assert!(effective.render().world_canvas.is_none());
    assert!(effective.ui().primary_surface.is_none());
    assert!(effective.window().enabled);
    assert_eq!(effective.input(), &RuntimeInputConfiguration::default());
    assert_round_trip(&effective);
    for raw in [
        serde_json::from_str::<RuntimeConfiguration>("{}").unwrap(),
        serde_json::from_str::<RuntimeConfiguration>(r#"{"input":{}}"#).unwrap(),
        serde_json::from_str::<RuntimeConfiguration>(r#"{"input":{"action_map":null}}"#).unwrap(),
        toml::from_str::<RuntimeConfiguration>("").unwrap(),
        toml::from_str::<RuntimeConfiguration>("[input]").unwrap(),
    ] {
        assert_eq!(raw.resolve().unwrap(), effective);
    }
}

#[test]
fn complete_configuration_lowers_every_runtime_section() {
    let configuration = complete_configuration();
    let effective = configuration.resolve().unwrap();
    assert_eq!(effective.runtime_preset(), RuntimePreset::LocalHeadless);
    assert_eq!(effective.requested_capabilities().len(), 4);
    assert!(
        effective
            .requested_capabilities()
            .contains(ProductCapability::RuntimeUi)
    );
    let time = effective.runtime().runtime_time_settings();
    assert!(time.paused());
    assert_eq!(time.time_scale(), 0.75);
    assert_eq!(time.max_delta(), Duration::from_nanos(123_456_789));
    let fixed = effective.runtime().fixed_time();
    assert_eq!(fixed.timestep(), Duration::from_nanos(16_666_667));
    assert_eq!(fixed.max_steps_per_frame(), 8);
    assert_eq!(fixed.max_debt_steps(), 64);
    assert_eq!(fixed.catch_up_policy(), FixedCatchUpPolicy::PreserveDebt);
    for (kind, workers, pending) in [
        (TaskPoolKind::Io, 2, 31),
        (TaskPoolKind::Compute, 3, 32),
        (TaskPoolKind::AsyncCompute, 4, 33),
    ] {
        let pool = effective.tasks().pool_config.kind(kind);
        assert_eq!(pool.workers().get(), workers);
        assert_eq!(pool.pending().get(), pending);
    }
    let shutdown = effective.tasks().pool_config.shutdown_policy();
    assert_eq!(shutdown.drain_timeout().get(), Duration::from_millis(101));
    assert_eq!(shutdown.cancel_timeout().get(), Duration::from_millis(102));
    assert_eq!(shutdown.join_timeout().get(), Duration::from_millis(103));
    assert_eq!(
        effective.render().world_canvas,
        Some(EffectiveWorldCanvas {
            width: 320,
            height: 180,
        })
    );
    assert_eq!(
        effective.ui().primary_surface,
        Some(EffectivePrimaryUiSurface {
            logical_width: 960,
            logical_height: 540,
            safe_area: EffectiveUiSafeArea {
                left: 11,
                top: 12,
                right: 13,
                bottom: 14,
            },
        })
    );
    assert_eq!(
        effective.window(),
        &EffectiveWindowSettings {
            enabled: false,
            title: "Independent runtime".to_owned(),
            width: 1920,
            height: 1080,
            scale_factor: 1.25_f64.next_up(),
            mode: ProjectWindowMode::BorderlessFullscreen,
            present_mode: ProjectPresentMode::Mailbox,
            resizable: false,
        }
    );
    let exported = effective.to_configuration();
    assert_eq!(exported.window, configuration.window);
    assert_eq!(
        serde_json::to_string(&exported.window).unwrap(),
        concat!(
            r#"{"enabled":false,"title":"Independent runtime","width":1920,"height":1080,"#,
            r#""scale_factor":1.2500000000000002,"mode":"borderless-fullscreen","#,
            r#""present_mode":"mailbox","resizable":false}"#,
        )
    );
    assert_eq!(effective.input().action_map, Some(primary_action_map()));
    assert_eq!(effective.diagnostics().runtime.entry_limit().get(), 37);
    assert_round_trip(&effective);
}

#[test]
fn primary_input_selection_exports_the_complete_reference_exactly() {
    let input = RuntimeInputConfiguration {
        action_map: Some(primary_action_map()),
    };
    let mut raw = RuntimeConfiguration {
        input: input.clone(),
        ..Default::default()
    };
    let effective = raw.resolve().unwrap();
    assert_eq!(effective.input(), &input);
    assert!(effective.requested_capabilities().is_empty());
    assert_eq!(effective.to_configuration(), raw);
    assert_round_trip(&effective);

    raw.input.action_map = None;
    assert_eq!(effective.input(), &input);
    assert_eq!(effective.to_configuration().input, input);
}

#[test]
fn auxiliary_input_products_are_rejected_by_admission() {
    let mut raw = complete_configuration();
    raw.input.action_map = Some(
        AssetProductRef::new(
            primary_action_map().source(),
            ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap(),
        )
        .unwrap(),
    );
    for configuration in [
        raw.clone(),
        serde_json::from_str(&serde_json::to_string(&raw).unwrap()).unwrap(),
        toml::from_str(&toml::to_string(&raw).unwrap()).unwrap(),
    ] {
        assert_diagnostic(
            &configuration.resolve().unwrap_err(),
            "runtime.configuration.input-primary-required",
            "input.action_map",
        );
    }
}

#[test]
fn runtime_input_rejects_source_paths_and_incomplete_or_nil_references() {
    let source = primary_action_map().source().to_string();
    for input in [
        serde_json::json!({"action_map": "source/input.actions.json"}),
        serde_json::json!({"action_map": {}}),
        serde_json::json!({"action_map": {"source": source}}),
        serde_json::json!({"action_map": {"product": {"kind": "primary"}}}),
        serde_json::json!({"action_map": {
            "source": "00000000-0000-0000-0000-000000000000",
            "product": {"kind": "primary"},
        }}),
    ] {
        let record = serde_json::json!({"input": input});
        let json = serde_json::to_string(&record).unwrap();
        assert!(
            serde_json::from_str::<RuntimeConfiguration>(&json).is_err(),
            "JSON accepted {json}"
        );
        let toml = toml::to_string(&record).unwrap();
        assert!(
            toml::from_str::<RuntimeConfiguration>(&toml).is_err(),
            "TOML accepted {toml}"
        );
    }
}

#[test]
fn json_input_rejects_duplicate_fields() {
    let reference = serde_json::to_string(&primary_action_map()).unwrap();
    for json in [
        r#"{"input":{},"input":{}}"#.to_owned(),
        r#"{"input":{"action_map":null,"action_map":null}}"#.to_owned(),
        format!(r#"{{"input":{{"action_map":{reference},"action_map":{reference}}}}}"#),
    ] {
        assert!(
            serde_json::from_str::<RuntimeConfiguration>(&json).is_err(),
            "JSON accepted {json}"
        );
    }

    let source = format!(r#""source":"{}""#, primary_action_map().source());
    let product = r#"{"kind":"primary"}"#;
    for fields in [
        format!(r#"{source},{source},"product":{product}"#),
        format!(r#"{source},"product":{product},"product":{product}"#),
        format!(r#"{source},"product":{{"kind":"primary","kind":"primary"}}"#),
    ] {
        let reference = format!("{{{fields}}}");
        let json = format!(r#"{{"input":{{"action_map":{reference}}}}}"#);
        assert!(
            serde_json::from_str::<RuntimeConfiguration>(&json).is_err(),
            "JSON accepted {json}"
        );
    }
}

#[test]
fn toml_input_rejects_duplicate_fields() {
    let source = format!(r#"source = "{}""#, primary_action_map().source());
    let product = r#"{ kind = "primary" }"#;
    let reference = format!("{{ {source}, product = {product} }}");
    for toml in [
        "[input]\n[input]\n".to_owned(),
        format!("[input]\naction_map = {reference}\naction_map = {reference}\n"),
        format!("[input.action_map]\n{source}\n{source}\nproduct = {product}\n"),
        format!("[input.action_map]\n{source}\nproduct = {product}\nproduct = {product}\n"),
        format!(
            "[input.action_map]\n{source}\n\
             [input.action_map.product]\nkind = 'primary'\nkind = 'primary'\n"
        ),
    ] {
        assert!(
            toml::from_str::<RuntimeConfiguration>(&toml).is_err(),
            "TOML accepted {toml}"
        );
    }
}

#[test]
fn invalid_timing_is_rejected_before_lowering() {
    for value in [f32::NAN, f32::INFINITY, -0.5] {
        let mut raw = RuntimeConfiguration::default();
        raw.runtime.time_scale = value;
        assert_diagnostic(
            &raw.resolve().unwrap_err(),
            "project.runtime.invalid-time-scale",
            "runtime.time_scale",
        );
    }
    let mut raw = RuntimeConfiguration::default();
    raw.runtime.max_delta = Duration::ZERO;
    raw.runtime.fixed_timestep = Duration::ZERO;
    let failure = raw.resolve().unwrap_err();
    for field in ["runtime.max_delta", "runtime.fixed_timestep"] {
        assert_diagnostic(&failure, "project.runtime.invalid-duration", field);
    }
    for (steps, debt, code, field) in [
        (
            0,
            120,
            "project.runtime.invalid-max-fixed-steps",
            "runtime.max_fixed_steps_per_frame",
        ),
        (
            5,
            0,
            "project.runtime.invalid-max-fixed-debt",
            "runtime.max_fixed_debt_steps",
        ),
        (
            MAX_PROJECT_FIXED_STEPS_PER_FRAME + 1,
            120,
            "project.runtime.max-fixed-steps-too-large",
            "runtime.max_fixed_steps_per_frame",
        ),
        (
            5,
            MAX_PROJECT_FIXED_DEBT_STEPS + 1,
            "project.runtime.max-fixed-debt-too-large",
            "runtime.max_fixed_debt_steps",
        ),
    ] {
        let mut raw = RuntimeConfiguration::default();
        raw.runtime.max_fixed_steps_per_frame = steps;
        raw.runtime.max_fixed_debt_steps = debt;
        assert_diagnostic(&raw.resolve().unwrap_err(), code, field);
    }
}

#[test]
fn task_limits_and_shutdown_are_validated_before_lowering() {
    for (pool, code, suffix) in [
        (
            ProjectTaskPoolManifest {
                workers: 0,
                pending_capacity: 1,
            },
            "project.tasks.invalid-workers",
            "workers",
        ),
        (
            ProjectTaskPoolManifest {
                workers: MAX_TASK_POOL_THREADS_PER_KIND as u32 + 1,
                pending_capacity: 1,
            },
            "project.tasks.workers-too-large",
            "workers",
        ),
        (
            ProjectTaskPoolManifest {
                workers: 1,
                pending_capacity: 0,
            },
            "project.tasks.invalid-pending-capacity",
            "pending_capacity",
        ),
        (
            ProjectTaskPoolManifest {
                workers: 1,
                pending_capacity: MAX_TASK_POOL_PENDING_PER_KIND as u32 + 1,
            },
            "project.tasks.pending-capacity-too-large",
            "pending_capacity",
        ),
    ] {
        for kind in TaskPoolKind::ALL {
            let mut raw = RuntimeConfiguration::default();
            let name = match kind {
                TaskPoolKind::Io => {
                    raw.tasks.io = pool;
                    "io"
                }
                TaskPoolKind::Compute => {
                    raw.tasks.compute = pool;
                    "compute"
                }
                TaskPoolKind::AsyncCompute => {
                    raw.tasks.async_compute = pool;
                    "async_compute"
                }
            };
            assert_diagnostic(
                &raw.resolve().unwrap_err(),
                code,
                &format!("tasks.{name}.{suffix}"),
            );
        }
    }
    for (timeout, code) in [
        (0, "project.tasks.invalid-shutdown-timeout"),
        (
            MAX_TASK_SHUTDOWN_PHASE_TIMEOUT.as_millis() as u64 + 1,
            "project.tasks.shutdown-timeout-too-large",
        ),
    ] {
        let mut raw = RuntimeConfiguration::default();
        raw.tasks.shutdown.drain_timeout_ms = timeout;
        raw.tasks.shutdown.cancel_timeout_ms = timeout;
        raw.tasks.shutdown.join_timeout_ms = timeout;
        let failure = raw.resolve().unwrap_err();
        for field in ["drain_timeout_ms", "cancel_timeout_ms", "join_timeout_ms"] {
            assert_diagnostic(&failure, code, &format!("tasks.shutdown.{field}"));
        }
    }
    let mut raw = RuntimeConfiguration::default();
    let pool = ProjectTaskPoolManifest {
        workers: MAX_TASK_POOL_THREADS_PER_KIND as u32,
        pending_capacity: MAX_TASK_POOL_PENDING_PER_KIND as u32,
    };
    raw.tasks.io = pool;
    raw.tasks.compute = pool;
    raw.tasks.async_compute = pool;
    let failure = raw.resolve().unwrap_err();
    assert_diagnostic(&failure, "project.tasks.total-workers-too-large", "tasks");
    assert_diagnostic(&failure, "project.tasks.total-pending-too-large", "tasks");
    let mut raw = RuntimeConfiguration::default();
    let timeout = MAX_TASK_SHUTDOWN_PHASE_TIMEOUT.as_millis() as u64;
    raw.tasks.shutdown.drain_timeout_ms = timeout;
    raw.tasks.shutdown.cancel_timeout_ms = timeout;
    raw.tasks.shutdown.join_timeout_ms = timeout;
    assert_diagnostic(
        &raw.resolve().unwrap_err(),
        "project.tasks.shutdown-total-too-large",
        "tasks.shutdown",
    );
}

#[test]
fn disabled_windows_still_require_valid_window_values() {
    let mut raw = RuntimeConfiguration::default();
    raw.window.enabled = false;
    raw.window.title = "  ".to_owned();
    raw.window.width = 0;
    raw.window.height = 0;
    for scale_factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        raw.window.scale_factor = scale_factor;
        let failure = raw.resolve().unwrap_err();
        for (code, field) in [
            ("project.window.empty-title", "window.title"),
            ("project.window.invalid-width", "window.width"),
            ("project.window.invalid-height", "window.height"),
            ("project.window.invalid-scale-factor", "window.scale_factor"),
        ] {
            assert_diagnostic(&failure, code, field);
        }
    }
}

#[test]
fn render_ui_diagnostics_and_duplicate_capabilities_are_validated() {
    let mut raw = complete_configuration();
    raw.capabilities
        .requested
        .push(ProductCapability::RuntimeUi);
    raw.render.world_canvas = Some(ProjectWorldCanvasManifest {
        width: 0,
        height: 0,
    });
    raw.ui.primary_surface = Some(ProjectUiSurfaceManifest {
        logical_width: 0,
        logical_height: MAX_PROJECT_UI_LOGICAL_EXTENT + 1,
        safe_area: ProjectUiSafeAreaManifest {
            top: u32::MAX,
            bottom: u32::MAX,
            ..Default::default()
        },
    });
    raw.diagnostics.runtime_capacity = 0;
    let failure = raw.resolve().unwrap_err();
    for (code, field) in [
        ("project.capabilities.duplicate", "capabilities.requested"),
        (
            "project.render.invalid-world-canvas-extent",
            "render.world_canvas.width",
        ),
        (
            "project.render.invalid-world-canvas-extent",
            "render.world_canvas.height",
        ),
        (
            "project.ui.invalid-logical-extent",
            "ui.primary_surface.logical_width",
        ),
        (
            "project.ui.invalid-logical-extent",
            "ui.primary_surface.logical_height",
        ),
        (
            "project.ui.invalid-horizontal-safe-area",
            "ui.primary_surface.safe_area",
        ),
        (
            "project.ui.invalid-vertical-safe-area",
            "ui.primary_surface.safe_area",
        ),
        (
            "project.diagnostics.invalid-runtime-capacity",
            "diagnostics.runtime_capacity",
        ),
    ] {
        assert_diagnostic(&failure, code, field);
    }
    raw.diagnostics.runtime_capacity = MAX_RUNTIME_DIAGNOSTIC_ENTRIES + 1;
    assert_diagnostic(
        &raw.resolve().unwrap_err(),
        "project.diagnostics.runtime-capacity-too-large",
        "diagnostics.runtime_capacity",
    );
}

#[test]
fn json_and_toml_reject_unknown_fields_at_every_runtime_boundary() {
    let raw = complete_configuration();
    for path in [
        &[][..],
        &["capabilities"][..],
        &["runtime"][..],
        &["runtime", "max_delta"][..],
        &["runtime", "fixed_timestep"][..],
        &["tasks"][..],
        &["tasks", "io"][..],
        &["tasks", "compute"][..],
        &["tasks", "async_compute"][..],
        &["tasks", "shutdown"][..],
        &["render"][..],
        &["render", "world_canvas"][..],
        &["ui"][..],
        &["ui", "primary_surface"][..],
        &["ui", "primary_surface", "safe_area"][..],
        &["window"][..],
        &["input"][..],
        &["input", "action_map"][..],
        &["input", "action_map", "product"][..],
        &["diagnostics"][..],
    ] {
        let mut json = serde_json::to_value(&raw).unwrap();
        let mut target = &mut json;
        for field in path {
            target = target.get_mut(*field).unwrap();
        }
        target
            .as_object_mut()
            .unwrap()
            .insert("unknown".to_owned(), true.into());
        assert!(
            serde_json::from_value::<RuntimeConfiguration>(json).is_err(),
            "JSON accepted {path:?}"
        );

        let mut toml: toml::Value = toml::from_str(&toml::to_string(&raw).unwrap()).unwrap();
        let mut target = &mut toml;
        for field in path {
            target = target.get_mut(*field).unwrap();
        }
        target
            .as_table_mut()
            .unwrap()
            .insert("unknown".to_owned(), toml::Value::Boolean(true));
        let encoded = toml::to_string(&toml).unwrap();
        assert!(
            toml::from_str::<RuntimeConfiguration>(&encoded).is_err(),
            "TOML accepted {path:?}"
        );
    }
}

#[test]
fn source_fields_are_not_part_of_the_runtime_record() {
    let raw = RuntimeConfiguration::default();
    for field in [
        "project",
        "paths",
        "profiles",
        "profile_name",
        "startup",
        "content",
        "authoring",
        "authoring_state",
        "execution",
        "schema_version",
    ] {
        let mut json = serde_json::to_value(&raw).unwrap();
        json.as_object_mut()
            .unwrap()
            .insert(field.to_owned(), serde_json::json!({}));
        assert!(
            serde_json::from_value::<RuntimeConfiguration>(json).is_err(),
            "JSON accepted {field}"
        );
        let encoded = format!("{}\n[{field}]\n", toml::to_string(&raw).unwrap());
        assert!(
            toml::from_str::<RuntimeConfiguration>(&encoded).is_err(),
            "TOML accepted {field}"
        );
    }
}

#[test]
fn server_preset_exports_normalized_values() {
    let mut raw = complete_configuration();
    raw.runtime.preset = RuntimePreset::Server;
    raw.runtime.catch_up_policy = ProjectFixedCatchUpPolicy::DiscardExcess;
    raw.window.enabled = true;
    let effective = raw.resolve().unwrap();
    assert!(!effective.window().enabled);
    assert_eq!(
        effective.runtime().fixed_time().catch_up_policy(),
        FixedCatchUpPolicy::PreserveDebt
    );
    assert_eq!(
        effective.requested_capabilities(),
        (&raw.capabilities).into()
    );
    let exported = effective.to_configuration();
    assert!(!exported.window.enabled);
    assert_eq!(
        exported.runtime.catch_up_policy,
        ProjectFixedCatchUpPolicy::PreserveDebt
    );
    assert_round_trip(&effective);
}

#[test]
fn exact_duration_records_round_trip_without_float_conversion() {
    for duration in [
        Duration::from_nanos(1),
        Duration::from_nanos(123_456_789),
        Duration::new(1, 999_999_999),
        Duration::new(8_388_608, 1),
        Duration::new(i64::MAX as u64, 999_999_999),
    ] {
        let mut raw = RuntimeConfiguration::default();
        raw.runtime.max_delta = duration;
        raw.runtime.fixed_timestep = duration;
        let effective = raw.resolve().unwrap();
        assert_eq!(
            effective.runtime().runtime_time_settings().max_delta(),
            duration
        );
        assert_eq!(effective.runtime().fixed_time().timestep(), duration);
        let exported = effective.to_configuration();
        assert_eq!(exported.runtime.max_delta, duration);
        assert_eq!(exported.runtime.fixed_timestep, duration);
        let json = serde_json::to_value(&exported).unwrap();
        for field in ["max_delta", "fixed_timestep"] {
            assert_eq!(
                json["runtime"][field],
                serde_json::json!({
                    "secs": duration.as_secs(),
                    "nanos": duration.subsec_nanos(),
                })
            );
        }
        assert_round_trip(&effective);
    }
}

#[test]
fn maximum_duration_is_admitted_and_round_trips_in_json_without_a_toml_range_gate() {
    let mut raw = RuntimeConfiguration::default();
    raw.runtime.max_delta = Duration::MAX;
    raw.runtime.fixed_timestep = Duration::MAX;
    let effective = raw.resolve().unwrap();
    assert_eq!(
        effective.runtime().runtime_time_settings().max_delta(),
        Duration::MAX
    );
    assert_eq!(effective.runtime().fixed_time().timestep(), Duration::MAX);
    assert_json_round_trip(&effective);
}

#[test]
fn duration_json_rejects_arrays_duplicate_missing_unknown_and_overflowing_fields() {
    for field in ["max_delta", "fixed_timestep"] {
        // Use JSON text to retain duplicate fields for the real deserializer.
        for invalid in [
            "[1, 2]",
            "[]",
            "null",
            "true",
            "0.25",
            "{}",
            r#"{"secs":1}"#,
            r#"{"nanos":2}"#,
            r#"{"secs":1,"secs":2,"nanos":3}"#,
            r#"{"secs":1,"nanos":2,"nanos":3}"#,
            r#"{"secs":1,"nanos":2,"unknown":3}"#,
            r#"{"secs":1,"nanos":1000000000}"#,
            r#"{"secs":18446744073709551615,"nanos":1000000000}"#,
            r#"{"secs":18446744073709551616,"nanos":0}"#,
            r#"{"secs":0,"nanos":4294967296}"#,
            r#"{"secs":-1,"nanos":0}"#,
            r#"{"secs":0,"nanos":-1}"#,
            r#"{"secs":1.0,"nanos":0}"#,
            r#"{"secs":1,"nanos":0.0}"#,
            r#"{"secs":"1","nanos":0}"#,
        ] {
            let source = format!(r#"{{"runtime":{{"{field}":{invalid}}}}}"#);
            assert!(
                serde_json::from_str::<RuntimeConfiguration>(&source).is_err(),
                "JSON accepted {field}={invalid}"
            );
        }
    }
}

#[test]
fn duration_toml_rejects_arrays_duplicate_missing_unknown_and_overflowing_fields() {
    for field in ["max_delta", "fixed_timestep"] {
        for invalid in [
            "[1, 2]",
            "[]",
            "true",
            "0.25",
            "{}",
            "{ secs = 1 }",
            "{ nanos = 2 }",
            "{ secs = 1, secs = 2, nanos = 3 }",
            "{ secs = 1, nanos = 2, nanos = 3 }",
            "{ secs = 1, nanos = 2, unknown = 3 }",
            "{ secs = 1, nanos = 1000000000 }",
            "{ secs = 18446744073709551615, nanos = 1000000000 }",
            "{ secs = 18446744073709551616, nanos = 0 }",
            "{ secs = 0, nanos = 4294967296 }",
            "{ secs = -1, nanos = 0 }",
            "{ secs = 0, nanos = -1 }",
            "{ secs = 1.0, nanos = 0 }",
            "{ secs = 1, nanos = 0.0 }",
            "{ secs = '1', nanos = 0 }",
        ] {
            let source = format!("[runtime]\n{field} = {invalid}\n");
            assert!(
                toml::from_str::<RuntimeConfiguration>(&source).is_err(),
                "TOML accepted {field}={invalid}"
            );
        }
    }
}

#[test]
fn decoded_zero_duration_fails_resolve_with_its_exact_field_path() {
    for field in ["max_delta", "fixed_timestep"] {
        let json = format!(r#"{{"runtime":{{"{field}":{{"secs":0,"nanos":0}}}}}}"#);
        let toml = format!("[runtime]\n{field} = {{ secs = 0, nanos = 0 }}\n");
        for raw in [
            serde_json::from_str::<RuntimeConfiguration>(&json).unwrap(),
            toml::from_str::<RuntimeConfiguration>(&toml).unwrap(),
        ] {
            assert_diagnostic(
                &raw.resolve().unwrap_err(),
                "project.runtime.invalid-duration",
                &format!("runtime.{field}"),
            );
        }
    }
}

#[test]
fn runtime_time_rejects_source_float_duration_field_names() {
    for field in ["max_delta_seconds", "fixed_timestep_seconds"] {
        let json = format!(r#"{{"runtime":{{"{field}":0.25}}}}"#);
        let toml = format!("[runtime]\n{field} = 0.25\n");
        assert!(serde_json::from_str::<RuntimeConfiguration>(&json).is_err());
        assert!(toml::from_str::<RuntimeConfiguration>(&toml).is_err());
    }
}

#[test]
fn product_capability_iteration_is_stable_and_sorted_by_engine_vocabulary() {
    let set = ProductCapabilitySet::from_capabilities([
        ProductCapability::Runtime2d,
        ProductCapability::RuntimeCore,
        ProductCapability::Presentation2d,
        ProductCapability::RenderWgpu,
    ]);

    assert_eq!(
        set.iter()
            .map(ProductCapability::as_str)
            .collect::<Vec<_>>(),
        vec![
            "runtime-core",
            "runtime-2d",
            "presentation-2d",
            "render-wgpu"
        ]
    );
}

#[test]
fn product_capability_discriminants_preserve_existing_bits() {
    assert_eq!(ProductCapability::RuntimeCore as u8, 0);
    assert_eq!(ProductCapability::Runtime2d as u8, 1);
    assert_eq!(ProductCapability::RuntimeUi as u8, 2);
    assert_eq!(ProductCapability::AssetWatch as u8, 3);
    assert_eq!(ProductCapability::DesktopWinit as u8, 4);
    assert_eq!(ProductCapability::RenderWgpu as u8, 5);
    assert_eq!(ProductCapability::Presentation2d as u8, 6);
}
