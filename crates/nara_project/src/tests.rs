use super::*;
use nara_app::{FixedCatchUpPolicy, FixedTime};
use nara_diagnostic::{
    Diagnostic, DiagnosticFieldClass, DiagnosticValueRef, MAX_RUNTIME_DIAGNOSTIC_ENTRIES,
};
use nara_tasks::{
    MAX_TASK_POOL_PENDING_PER_KIND, MAX_TASK_POOL_PENDING_TOTAL, MAX_TASK_POOL_THREADS_PER_KIND,
    MAX_TASK_POOL_THREADS_TOTAL, MAX_TASK_SHUTDOWN_PHASE_TIMEOUT, TaskPoolKind,
};
use std::time::Duration;

const MINIMAL_MANIFEST: &str = r#"
schema_version = 1

[project]
name = "Test Game"
"#;
const COMPLETE_MANIFEST: &str = include_str!("../tests/fixtures/complete_v1.toml");

fn diagnostic_codes(load: &ProjectManifestLoad) -> Vec<&str> {
    load.diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().as_str())
        .collect()
}

fn diagnostic_identifier<'a>(diagnostic: &'a Diagnostic, key: &str) -> Option<&'a str> {
    diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == key)
        .and_then(|field| match field.value() {
            DiagnosticValueRef::Identifier(value) => Some(value),
            _ => None,
        })
}

fn diagnostic_field_class(diagnostic: &Diagnostic, key: &str) -> Option<DiagnosticFieldClass> {
    diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == key)
        .map(|field| field.class())
}

#[test]
fn minimal_manifest_parses_and_resolves_validated_defaults() {
    let load = ProjectManifest::parse_toml_str(MINIMAL_MANIFEST);

    assert!(!load.has_errors());
    let manifest = load.manifest.unwrap();
    let settings = manifest.resolve_profile(None).unwrap();

    assert_eq!(settings.project.name, "Test Game");
    assert_eq!(settings.paths.assets.as_str(), "assets");
    assert_eq!(settings.paths.import_cache.as_str(), ".nara/import-cache");
    assert!(settings.authoring_schema_bundle.is_none());
    assert_eq!(settings.runtime_preset, RuntimePreset::Minimal);
    assert!(settings.requested_capabilities.is_empty());
    assert_eq!(
        settings.runtime.fixed_time().max_steps_per_frame(),
        FixedTime::DEFAULT_MAX_STEPS_PER_FRAME
    );
    assert_eq!(
        settings.runtime.fixed_time().max_debt_steps(),
        FixedTime::DEFAULT_MAX_DEBT_STEPS
    );
    assert_eq!(
        settings.runtime.fixed_time().catch_up_policy(),
        FixedCatchUpPolicy::DiscardExcess
    );
    assert!(settings.window.to_window().is_some());
}

#[test]
fn authoring_schema_bundle_is_project_wide_and_canonical() {
    let source = r#"
schema_version = 1

[project]
name = "Authoring Bundle"

[authoring]
schema_bundle = "schema/authoring.bundle.json"

[profiles.dev.startup]
default_scene = "scenes/dev.scene.json"
"#;
    let manifest = ProjectManifest::parse_toml_str(source).manifest.unwrap();
    let base = manifest.resolve_profile(None).unwrap();
    let dev = manifest.resolve_profile(Some("dev")).unwrap();

    assert_eq!(
        base.authoring_schema_bundle.as_ref().unwrap().as_str(),
        "schema/authoring.bundle.json"
    );
    assert_eq!(dev.authoring_schema_bundle, base.authoring_schema_bundle);

    let encoded = manifest.to_canonical_toml_bytes().unwrap();
    let loaded = ProjectManifest::parse_toml_bytes(&encoded);
    assert!(!loaded.has_errors(), "{:?}", loaded.diagnostics);
    assert_eq!(loaded.manifest.unwrap(), manifest);
}

#[test]
fn authoring_schema_bundle_rejects_an_invalid_project_path() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Invalid Authoring Bundle"

[authoring]
schema_bundle = "../outside.bundle.json"
"#,
    );

    assert!(load.has_errors());
    assert!(diagnostic_codes(&load).contains(&"project.path.invalid"));
    assert!(load.manifest.unwrap().resolve_profile(None).is_err());
}

#[test]
fn execution_contract_is_profile_local_exact_and_forward_compatible() {
    let source = r#"
schema_version = 1

[project]
name = "Execution Contract"

[profiles.desktop.execution]
contract = "desktop-supervised-play/v1"

[profiles.future.execution]
contract = "desktop-supervised-play/v2"
"#;
    let load = ProjectManifest::parse_toml_str(source);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    let manifest = load.manifest.unwrap();

    assert!(
        manifest
            .resolve_profile(None)
            .unwrap()
            .execution
            .contract
            .is_none()
    );
    assert_eq!(
        manifest
            .resolve_profile(Some("desktop"))
            .unwrap()
            .execution
            .contract,
        Some(ProjectExecutionContract::DesktopSupervisedPlayV1)
    );
    assert_eq!(
        manifest
            .resolve_profile(Some("future"))
            .unwrap()
            .execution
            .contract,
        Some(ProjectExecutionContract::Unsupported(
            "desktop-supervised-play/v2".to_owned()
        ))
    );

    let encoded = manifest.to_canonical_toml_bytes().unwrap();
    let reparsed = ProjectManifest::parse_toml_bytes(&encoded);
    assert!(!reparsed.has_errors(), "{:?}", reparsed.diagnostics);
    assert_eq!(reparsed.manifest.unwrap(), manifest);
}

#[test]
fn execution_contract_rejects_an_unbounded_or_non_identifier_value() {
    let oversized = "x".repeat(MAX_PROJECT_EXECUTION_CONTRACT_BYTES + 1);
    for contract in [
        String::new(),
        "desktop supervised play/v1".to_owned(),
        oversized,
    ] {
        let source = format!(
            r#"
schema_version = 1

[project]
name = "Invalid Execution Contract"

[profiles.desktop.execution]
contract = "{contract}"
"#
        );
        let load = ProjectManifest::parse_toml_str(&source);
        assert!(
            load.has_errors(),
            "contract should be rejected: {contract:?}"
        );
    }
}

#[test]
fn profile_cannot_override_the_project_wide_authoring_bundle() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Profile Authoring Override"

[profiles.dev.authoring]
schema_bundle = "schema/dev.bundle.json"
"#,
    );

    assert!(load.manifest.is_none());
    assert!(diagnostic_codes(&load).contains(&"project.manifest.parse"));
}

#[test]
fn primary_ui_surface_is_optional_bounded_and_profile_owned() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "UI Surface"

[ui.primary_surface]
logical_width = 960
logical_height = 540

[ui.primary_surface.safe_area]
left = 10
top = 12
right = 10
bottom = 12

[profiles.editor.ui.primary_surface]
logical_width = 1280
logical_height = 720
"#,
    );
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    let manifest = load.manifest.unwrap();
    let base = manifest.resolve_profile(None).unwrap();
    let editor = manifest.resolve_profile(Some("editor")).unwrap();
    assert_eq!(
        base.ui.primary_surface,
        Some(EffectivePrimaryUiSurface {
            logical_width: 960,
            logical_height: 540,
            safe_area: EffectiveUiSafeArea {
                left: 10,
                top: 12,
                right: 10,
                bottom: 12,
            },
        })
    );
    assert_eq!(editor.ui.primary_surface.unwrap().logical_width, 1_280);
    assert_eq!(editor.ui.primary_surface.unwrap().logical_height, 720);

    let absent = ProjectManifest::parse_toml_str(MINIMAL_MANIFEST)
        .manifest
        .unwrap()
        .resolve_profile(None)
        .unwrap();
    assert!(absent.ui.primary_surface.is_none());

    let invalid = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Invalid UI Surface"

[ui.primary_surface]
logical_width = 0
logical_height = 100

[ui.primary_surface.safe_area]
left = 60
right = 40
"#,
    );
    let codes = diagnostic_codes(&invalid);
    assert!(codes.contains(&"project.ui.invalid-logical-extent"));
    assert!(codes.contains(&"project.ui.invalid-horizontal-safe-area"));
}

#[test]
fn complete_v1_fixture_lowers_every_runtime_and_task_field() {
    let load = ProjectManifest::parse_toml_str(COMPLETE_MANIFEST);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    let settings = load.manifest.unwrap().resolve_profile(None).unwrap();

    assert_eq!(settings.runtime_preset, RuntimePreset::Minimal);
    assert_eq!(
        settings.requested_capabilities,
        ProductCapabilitySet::singleton(ProductCapability::Runtime2d)
    );
    let runtime = settings.runtime.runtime_time_settings();
    assert_eq!(runtime.time_scale(), 0.75);
    assert_eq!(runtime.max_delta(), Duration::from_millis(200));
    let fixed = settings.runtime.fixed_time();
    assert_eq!(fixed.timestep(), Duration::from_millis(10));
    assert_eq!(fixed.max_steps_per_frame(), 4);
    assert_eq!(fixed.max_debt_steps(), 20);
    assert_eq!(fixed.catch_up_policy(), FixedCatchUpPolicy::PreserveDebt);

    let tasks = settings.tasks.pool_config;
    assert_eq!(tasks.kind(TaskPoolKind::Io).workers().get(), 2);
    assert_eq!(tasks.kind(TaskPoolKind::Io).pending().get(), 64);
    assert_eq!(tasks.kind(TaskPoolKind::Compute).workers().get(), 3);
    assert_eq!(tasks.kind(TaskPoolKind::Compute).pending().get(), 65);
    assert_eq!(tasks.kind(TaskPoolKind::AsyncCompute).workers().get(), 4);
    assert_eq!(tasks.kind(TaskPoolKind::AsyncCompute).pending().get(), 66);
    assert_eq!(
        tasks.shutdown_policy().drain_timeout().get(),
        Duration::from_millis(400)
    );
    assert_eq!(
        tasks.shutdown_policy().cancel_timeout().get(),
        Duration::from_millis(300)
    );
    assert_eq!(
        tasks.shutdown_policy().join_timeout().get(),
        Duration::from_millis(200)
    );
}

#[test]
fn unknown_manifest_fields_are_rejected() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1
unexpected = true

[project]
name = "Bad"
"#,
    );

    assert!(load.manifest.is_none());
    assert!(load.has_errors());
    assert_eq!(
        load.diagnostics.iter().next().unwrap().code().as_str(),
        "project.manifest.parse"
    );
}

#[test]
fn parse_diagnostics_redact_manifest_content_and_keep_numeric_location() {
    let canary = "Bearer project-parse-canary";
    let source = format!(
        r#"
schema_version = 1
unexpected = "{canary}"

[project]
name = "Private Project"
"#
    );

    let load = ProjectManifest::parse_toml_str(&source);
    let diagnostic = load.diagnostics.iter().next().unwrap();
    let rendered = format!("{load:?}");

    assert_eq!(diagnostic.code().as_str(), "project.manifest.parse");
    assert_eq!(
        diagnostic_field_class(diagnostic, "manifest_content"),
        Some(DiagnosticFieldClass::Secret)
    );
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "line" && matches!(field.value(), DiagnosticValueRef::Unsigned(_))
    }));
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "column" && matches!(field.value(), DiagnosticValueRef::Unsigned(_))
    }));
    assert!(!rendered.contains(canary));
}

#[test]
fn invalid_paths_produce_structured_diagnostics() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Bad Paths"

[paths]
assets = "../assets"
"#,
    );

    assert!(load.has_errors());
    let diagnostics = &load.diagnostics;
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.path.invalid"
            && diagnostic_identifier(diagnostic, "field") == Some("paths.assets")
    }));
}

#[test]
fn invalid_project_path_diagnostic_never_retains_the_raw_path() {
    let raw_path = "C:/private/password-vault/assets";
    let load = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Private Paths"

[paths]
assets = "{raw_path}"
"#
    ));
    let diagnostic = load
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "project.path.invalid")
        .unwrap();

    assert_eq!(
        diagnostic_field_class(diagnostic, "path"),
        Some(DiagnosticFieldClass::Sensitive)
    );
    let rendered = format!("{load:?}");
    assert!(rendered.contains("manifest_present: true"));
    assert!(!rendered.contains(raw_path));
}

#[test]
fn invalid_profile_name_is_redacted_before_overlay_field_paths_are_built() {
    let profile_canary = "credential/profile-canary";
    let load = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Private Profile"

[profiles."{profile_canary}".window]
width = 0
"#
    ));
    let invalid_name = load
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "project.profile.invalid-name")
        .unwrap();
    let invalid_width = load
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "project.window.invalid-width")
        .unwrap();

    assert_eq!(
        diagnostic_field_class(invalid_name, "profile"),
        Some(DiagnosticFieldClass::Sensitive)
    );
    assert_eq!(
        diagnostic_identifier(invalid_width, "field"),
        Some("profiles.redacted.window.width")
    );
    assert!(!format!("{:?}", load.diagnostics).contains(profile_canary));
}

#[test]
fn diagnostic_identity_limits_do_not_reject_domain_valid_profile_names() {
    let profile_name = "a".repeat(128);
    let load = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Long Profile"

[profiles."{profile_name}".window]
width = 0
"#
    ));
    let codes = diagnostic_codes(&load);
    let invalid_width = load
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "project.window.invalid-width")
        .unwrap();

    assert!(!codes.contains(&"project.profile.invalid-name"));
    assert_eq!(
        diagnostic_field_class(invalid_width, "field"),
        Some(DiagnosticFieldClass::Sensitive)
    );
}

#[test]
fn runtime_rejects_non_finite_negative_overflowing_and_sub_nanosecond_values() {
    for (field, value) in [
        ("time_scale", "nan"),
        ("time_scale", "inf"),
        ("time_scale", "-0.1"),
        ("max_delta_seconds", "1e308"),
        ("max_delta_seconds", "1e-12"),
        ("fixed_timestep_seconds", "1e308"),
        ("fixed_timestep_seconds", "1e-12"),
    ] {
        let load = ProjectManifest::parse_toml_str(&format!(
            r#"
schema_version = 1

[project]
name = "Invalid Runtime"

[runtime]
{field} = {value}
"#
        ));

        assert!(load.has_errors(), "{field}={value} should be rejected");
    }
}

#[test]
fn runtime_rejects_zero_duration_and_fixed_caps() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Zero Runtime Limits"

[runtime]
max_delta_seconds = 0.0
fixed_timestep_seconds = 0.0
max_fixed_steps_per_frame = 0
max_fixed_debt_steps = 0
"#,
    );

    let codes = diagnostic_codes(&load);
    assert!(codes.contains(&"project.runtime.invalid-duration"));
    assert!(codes.contains(&"project.runtime.invalid-max-fixed-steps"));
    assert!(codes.contains(&"project.runtime.invalid-max-fixed-debt"));
}

#[test]
fn runtime_rejects_fixed_caps_above_project_hard_limits() {
    let too_many_steps = MAX_PROJECT_FIXED_STEPS_PER_FRAME + 1;
    let too_much_debt = MAX_PROJECT_FIXED_DEBT_STEPS + 1;
    let base = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Oversized Fixed Runtime"

[runtime]
max_fixed_steps_per_frame = {too_many_steps}
max_fixed_debt_steps = {too_much_debt}
"#
    ));
    assert!(base.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.runtime.max-fixed-steps-too-large"
            && diagnostic_identifier(diagnostic, "field")
                == Some("runtime.max_fixed_steps_per_frame")
    }));
    assert!(base.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.runtime.max-fixed-debt-too-large"
            && diagnostic_identifier(diagnostic, "field") == Some("runtime.max_fixed_debt_steps")
    }));

    let profile = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Oversized Fixed Profile"

[profiles.dev.runtime]
max_fixed_steps_per_frame = {too_many_steps}
max_fixed_debt_steps = {too_much_debt}
"#
    ));
    assert!(profile.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.runtime.max-fixed-steps-too-large"
            && diagnostic_identifier(diagnostic, "field")
                == Some("profiles.dev.runtime.max_fixed_steps_per_frame")
    }));
    assert!(profile.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.runtime.max-fixed-debt-too-large"
            && diagnostic_identifier(diagnostic, "field")
                == Some("profiles.dev.runtime.max_fixed_debt_steps")
    }));

    let boundary = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Fixed Runtime Boundary"

[runtime]
max_fixed_steps_per_frame = {MAX_PROJECT_FIXED_STEPS_PER_FRAME}
max_fixed_debt_steps = {MAX_PROJECT_FIXED_DEBT_STEPS}
"#
    ));
    assert!(!boundary.has_errors(), "{:?}", boundary.diagnostics);
    let settings = boundary.manifest.unwrap().resolve_profile(None).unwrap();
    assert_eq!(
        settings.runtime.fixed_time().max_steps_per_frame(),
        MAX_PROJECT_FIXED_STEPS_PER_FRAME
    );
    assert_eq!(
        settings.runtime.fixed_time().max_debt_steps(),
        MAX_PROJECT_FIXED_DEBT_STEPS
    );
}

#[test]
fn obsolete_plugin_plan_field_is_rejected() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Obsolete Plugin Plan"

[runtime]
plugin_plan = "runtime-2d"
"#,
    );

    assert!(load.manifest.is_none());
    assert!(diagnostic_codes(&load).contains(&"project.manifest.parse"));
}

#[test]
fn runtime_tooling_capabilities_are_rejected_by_manifest() {
    for capability in ["tooling", "editor"] {
        let load = ProjectManifest::parse_toml_str(&format!(
            r#"
schema_version = 1

[project]
name = "Runtime Tooling Capability"

[capabilities]
requested = ["{capability}"]
"#,
        ));

        assert!(load.manifest.is_none(), "accepted {capability}");
        assert!(
            diagnostic_codes(&load).contains(&"project.manifest.parse"),
            "missing parse diagnostic for {capability}"
        );
    }
}

#[test]
fn server_profile_enforces_headless_preserve_debt_with_threaded_tasks() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Server Game"

[profiles.server]
"#,
    );
    let settings = load
        .manifest
        .unwrap()
        .resolve_profile(Some("server"))
        .unwrap();

    assert_eq!(settings.runtime_preset, RuntimePreset::Server);
    assert!(!settings.window.enabled);
    assert_eq!(
        settings.runtime.fixed_time().catch_up_policy(),
        FixedCatchUpPolicy::PreserveDebt
    );
    assert!(
        settings
            .tasks
            .pool_config
            .kind(TaskPoolKind::Io)
            .workers()
            .get()
            > 0
    );
}

#[test]
fn editor_profile_does_not_request_runtime_tooling() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Host-Owned Editor"

[profiles.editor]
"#,
    );
    let settings = load
        .manifest
        .unwrap()
        .resolve_profile(Some("editor"))
        .unwrap();

    assert!(
        settings
            .requested_capabilities
            .iter()
            .all(|capability| !capability.as_str().starts_with("tooling"))
    );
}

#[test]
fn server_preset_enforces_runtime_invariants_without_named_profile() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Direct Server Plan"

[runtime]
preset = "server"
catch_up_policy = "discard-excess"
max_fixed_debt_steps = 17

[window]
enabled = true
"#,
    );
    let settings = load.manifest.unwrap().resolve_profile(None).unwrap();

    assert_eq!(settings.runtime_preset, RuntimePreset::Server);
    assert!(!settings.window.enabled);
    assert_eq!(
        settings.runtime.fixed_time().catch_up_policy(),
        FixedCatchUpPolicy::PreserveDebt
    );
    assert_eq!(settings.runtime.fixed_time().max_debt_steps(), 17);
}

#[test]
fn profile_overlay_lowers_complete_runtime_task_and_project_settings() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Overlay Game"

[runtime]
time_scale = 1.0
preset = "minimal"

[capabilities]
requested = ["runtime-2d"]

[startup]
default_scene = "scenes/main.scene.ron"

[input]
action_map = "input/default.actions.ron"

[profiles.dev.paths]
assets = "dev-assets"
import_cache = ".nara/dev-cache"

[profiles.dev.startup]
default_scene = "scenes/dev.scene.ron"

[profiles.dev.runtime]
paused = true
time_scale = 0.5
max_delta_seconds = 0.125
fixed_timestep_seconds = 0.02
max_fixed_steps_per_frame = 2
max_fixed_debt_steps = 9
catch_up_policy = "preserve-debt"
preset = "local-headless"

[profiles.dev.capabilities]
requested = ["asset-watch"]

[profiles.dev.tasks.io]
workers = 3
pending_capacity = 31

[profiles.dev.tasks.compute]
workers = 4
pending_capacity = 32

[profiles.dev.tasks.async_compute]
workers = 5
pending_capacity = 33

[profiles.dev.tasks.shutdown]
drain_timeout_ms = 300
cancel_timeout_ms = 200
join_timeout_ms = 100

[profiles.dev.window]
title = "Dev Window"
width = 640
height = 480

[profiles.dev.input]
action_map = "input/dev.actions.ron"

[profiles.dev.diagnostics]
runtime_capacity = 32
"#,
    );
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    let settings = load.manifest.unwrap().resolve_profile(Some("dev")).unwrap();

    assert_eq!(settings.runtime_preset, RuntimePreset::LocalHeadless);
    assert_eq!(
        settings.requested_capabilities,
        ProductCapabilitySet::singleton(ProductCapability::AssetWatch)
    );
    let runtime = settings.runtime.runtime_time_settings();
    assert!(runtime.paused());
    assert_eq!(runtime.time_scale(), 0.5);
    assert_eq!(runtime.max_delta(), Duration::from_millis(125));
    let fixed = settings.runtime.fixed_time();
    assert_eq!(fixed.timestep(), Duration::from_millis(20));
    assert_eq!(fixed.max_steps_per_frame(), 2);
    assert_eq!(fixed.max_debt_steps(), 9);
    assert_eq!(fixed.catch_up_policy(), FixedCatchUpPolicy::PreserveDebt);
    assert_eq!(settings.window.title, "Dev Window");
    assert_eq!(settings.window.width, 640);
    assert_eq!(settings.window.height, 480);
    assert_eq!(settings.paths.assets.as_str(), "dev-assets");
    assert_eq!(settings.paths.import_cache.as_str(), ".nara/dev-cache");
    assert_eq!(
        settings.startup.default_scene.as_ref().unwrap().as_str(),
        "scenes/dev.scene.ron"
    );
    assert_eq!(
        settings.input.action_map.as_ref().unwrap().as_str(),
        "input/dev.actions.ron"
    );
    assert_eq!(settings.diagnostics.runtime.entry_limit().get(), 32);
    assert_eq!(settings.diagnostics.runtime.byte_limit().get(), 1024 * 1024);

    let task_config = settings.tasks.pool_config;
    for (kind, workers, pending) in [
        (TaskPoolKind::Io, 3, 31),
        (TaskPoolKind::Compute, 4, 32),
        (TaskPoolKind::AsyncCompute, 5, 33),
    ] {
        let kind_config = task_config.kind(kind);
        assert_eq!(kind_config.workers().get(), workers);
        assert_eq!(kind_config.pending().get(), pending);
    }
    let shutdown = task_config.shutdown_policy();
    assert_eq!(shutdown.drain_timeout().get(), Duration::from_millis(300));
    assert_eq!(shutdown.cancel_timeout().get(), Duration::from_millis(200));
    assert_eq!(shutdown.join_timeout().get(), Duration::from_millis(100));
}

#[test]
fn profile_overlay_can_clear_optional_startup_and_input_paths() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Clear Overlay Game"

[startup]
default_scene = "scenes/main.scene.ron"

[input]
action_map = "input/default.actions.ron"

[profiles.dev.startup]
clear_default_scene = true

[profiles.dev.input]
clear_action_map = true
"#,
    );
    let settings = load.manifest.unwrap().resolve_profile(Some("dev")).unwrap();

    assert_eq!(settings.startup.default_scene, None);
    assert_eq!(settings.input.action_map, None);
}

#[test]
fn server_profile_invariants_survive_hostile_overlay_values_without_resetting_debt_cap() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Hostile Server Game"

[profiles.server.runtime]
preset = "minimal"
catch_up_policy = "discard-excess"
max_fixed_debt_steps = 7

[profiles.server.capabilities]
requested = ["desktop-winit", "render-wgpu"]

[profiles.server.tasks.io]
workers = 4
pending_capacity = 40

[profiles.server.window]
enabled = true
"#,
    );
    let settings = load
        .manifest
        .unwrap()
        .resolve_profile(Some("server"))
        .unwrap();

    assert_eq!(settings.runtime_preset, RuntimePreset::Server);
    assert!(
        settings
            .requested_capabilities
            .contains(ProductCapability::DesktopWinit)
    );
    assert!(
        settings
            .requested_capabilities
            .contains(ProductCapability::RenderWgpu)
    );
    assert!(!settings.window.enabled);
    assert_eq!(
        settings.runtime.fixed_time().catch_up_policy(),
        FixedCatchUpPolicy::PreserveDebt
    );
    assert_eq!(settings.runtime.fixed_time().max_debt_steps(), 7);
    assert_eq!(
        settings
            .tasks
            .pool_config
            .kind(TaskPoolKind::Io)
            .workers()
            .get(),
        4
    );
}

#[test]
fn obsolete_flat_task_schema_is_rejected() {
    for obsolete in [
        "mode = \"deterministic\"",
        "io_threads = 2",
        "compute_threads = 2",
        "async_compute_threads = 2",
    ] {
        let load = ProjectManifest::parse_toml_str(&format!(
            r#"
schema_version = 1

[project]
name = "Obsolete Tasks"

[tasks]
{obsolete}
"#
        ));
        assert!(load.manifest.is_none(), "{obsolete} should not parse");
    }
}

#[test]
fn task_settings_reject_zero_pool_and_shutdown_limits() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Zero Tasks"

[tasks.io]
workers = 0
pending_capacity = 0

[tasks.shutdown]
drain_timeout_ms = 0
cancel_timeout_ms = 0
join_timeout_ms = 0
"#,
    );

    let codes = diagnostic_codes(&load);
    assert!(codes.contains(&"project.tasks.invalid-workers"));
    assert!(codes.contains(&"project.tasks.invalid-pending-capacity"));
    assert!(codes.contains(&"project.tasks.invalid-shutdown-timeout"));
}

#[test]
fn task_settings_reject_per_pool_and_aggregate_limits() {
    let per_pool = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Per Pool Limits"

[tasks.io]
workers = {}
pending_capacity = {}
"#,
        MAX_TASK_POOL_THREADS_PER_KIND + 1,
        MAX_TASK_POOL_PENDING_PER_KIND + 1,
    ));
    let per_pool_codes = diagnostic_codes(&per_pool);
    assert!(per_pool_codes.contains(&"project.tasks.workers-too-large"));
    assert!(per_pool_codes.contains(&"project.tasks.pending-capacity-too-large"));

    let workers_each = MAX_TASK_POOL_THREADS_TOTAL / 3 + 1;
    let pending_each = MAX_TASK_POOL_PENDING_TOTAL / 3 + 1;
    assert!(workers_each <= MAX_TASK_POOL_THREADS_PER_KIND);
    assert!(pending_each <= MAX_TASK_POOL_PENDING_PER_KIND);
    let aggregate = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Aggregate Limits"

[tasks.io]
workers = {workers_each}
pending_capacity = {pending_each}

[tasks.compute]
workers = {workers_each}
pending_capacity = {pending_each}

[tasks.async_compute]
workers = {workers_each}
pending_capacity = {pending_each}
"#
    ));
    let aggregate_codes = diagnostic_codes(&aggregate);
    assert!(aggregate_codes.contains(&"project.tasks.total-workers-too-large"));
    assert!(aggregate_codes.contains(&"project.tasks.total-pending-too-large"));
}

#[test]
fn task_profile_patch_is_validated_after_merging_with_base_settings() {
    let workers_each = MAX_TASK_POOL_THREADS_TOTAL / 3;
    let load = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Patched Aggregate Limits"

[tasks.io]
workers = {workers_each}
pending_capacity = 1

[tasks.compute]
workers = {workers_each}
pending_capacity = 1

[tasks.async_compute]
workers = 1
pending_capacity = 1

[profiles.dev.tasks.async_compute]
workers = {}
"#,
        MAX_TASK_POOL_THREADS_PER_KIND
    ));

    assert!(
        diagnostic_codes(&load).contains(&"project.tasks.total-workers-too-large"),
        "profile patch must be validated against base pool values"
    );
}

#[test]
fn task_settings_reject_phase_and_aggregate_shutdown_timeouts() {
    let too_long_ms = u64::try_from(MAX_TASK_SHUTDOWN_PHASE_TIMEOUT.as_millis()).unwrap() + 1;
    let load = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Shutdown Limits"

[tasks.shutdown]
drain_timeout_ms = {too_long_ms}
cancel_timeout_ms = 30000
join_timeout_ms = 30000
"#
    ));

    let codes = diagnostic_codes(&load);
    assert!(codes.contains(&"project.tasks.shutdown-timeout-too-large"));
    assert!(codes.contains(&"project.tasks.shutdown-total-too-large"));
}

#[test]
fn manifest_rejects_diagnostic_capacity_above_engine_cap() {
    let load = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Oversized Diagnostics"

[diagnostics]
runtime_capacity = {}
"#,
        MAX_RUNTIME_DIAGNOSTIC_ENTRIES + 1
    ));

    assert!(diagnostic_codes(&load).contains(&"project.diagnostics.runtime-capacity-too-large"));
}

#[test]
fn disabled_invalid_base_window_is_rejected_before_profile_reenable() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Window Reenable Game"

[window]
enabled = false
width = 0

[profiles.dev.window]
enabled = true
"#,
    );

    assert!(load.has_errors());
    assert!(load.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.window.invalid-width"
            && diagnostic_identifier(diagnostic, "field") == Some("window.width")
    }));
}

#[test]
fn resolving_unknown_profile_returns_diagnostic_error() {
    let manifest = ProjectManifest::parse_toml_str(MINIMAL_MANIFEST)
        .manifest
        .unwrap();

    let error = manifest.resolve_profile(Some("missing")).unwrap_err();

    let ProjectProfileError::UnknownProfile {
        profile,
        diagnostics,
    } = error
    else {
        panic!("expected unknown profile error");
    };
    assert_eq!(profile, "missing");
    let diagnostic = diagnostics.iter().next().unwrap();
    assert_eq!(diagnostic.code().as_str(), "project.profile.unknown");
    assert_eq!(
        diagnostic_identifier(diagnostic, "profile"),
        Some("missing")
    );
    assert_eq!(
        diagnostic_identifier(diagnostic, "field"),
        Some("profiles.missing")
    );
}

#[test]
fn unknown_profile_error_debug_redacts_sensitive_profile_name() {
    let manifest = ProjectManifest::parse_toml_str(MINIMAL_MANIFEST)
        .manifest
        .unwrap();
    let profile_canary = "credential://private-profile-canary";
    let error = manifest.resolve_profile(Some(profile_canary)).unwrap_err();

    let ProjectProfileError::UnknownProfile { profile, .. } = &error else {
        panic!("expected unknown profile error");
    };
    assert_eq!(profile, profile_canary);
    assert_eq!(error.to_string(), "unknown project profile");
    let rendered = format!("{error:?}");
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains(profile_canary));
}

#[test]
fn byte_parser_accepts_the_exact_hard_limit_and_rejects_limit_plus_one() {
    let limit = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT).unwrap();
    let mut exact = MINIMAL_MANIFEST.as_bytes().to_vec();
    exact.resize(limit, b' ');
    let exact_load = ProjectManifest::parse_toml_bytes(&exact);
    assert!(!exact_load.has_errors(), "{:?}", exact_load.diagnostics);

    exact.push(b' ');
    let oversized = ProjectManifest::parse_toml_bytes(&exact);
    assert!(oversized.manifest.is_none());
    assert_eq!(
        oversized.diagnostics.iter().next().unwrap().code().as_str(),
        "project.manifest.too-large"
    );
}

#[test]
fn invalid_utf8_is_classified_without_retaining_input_bytes() {
    let canary = b"credential-canary\xff";
    let load = ProjectManifest::parse_toml_bytes(canary);
    let diagnostic = load.diagnostics.iter().next().unwrap();

    assert_eq!(diagnostic.code().as_str(), "project.manifest.utf8");
    assert_eq!(
        diagnostic_field_class(diagnostic, "manifest_content"),
        Some(DiagnosticFieldClass::Secret)
    );
    assert!(!format!("{load:?}").contains("credential-canary"));
}

#[test]
fn duplicate_and_unknown_product_capabilities_are_rejected() {
    let duplicate = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Duplicate Capabilities"

[capabilities]
requested = ["runtime-2d", "runtime-2d"]
"#,
    );
    assert!(diagnostic_codes(&duplicate).contains(&"project.capabilities.duplicate"));

    let unknown = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Unknown Capability"

[capabilities]
requested = ["future-renderer"]
"#,
    );
    assert!(unknown.manifest.is_none());
    assert!(diagnostic_codes(&unknown).contains(&"project.manifest.parse"));
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

#[test]
fn canonical_writer_changes_only_the_selected_profile_startup_scene_semantics() {
    let source = format!(
        "{COMPLETE_MANIFEST}\n[startup]\ndefault_scene = \"base.scene.json\"\n\n[profiles.dev.startup]\ndefault_scene = \"old.scene.json\"\n"
    );
    let mut expected = ProjectManifest::parse_toml_str(&source).manifest.unwrap();
    expected
        .replace_startup_scene(
            Some("dev"),
            ProjectPath::new("replacement.scene.json").unwrap(),
        )
        .unwrap();

    let encoded = expected.to_canonical_toml_bytes().unwrap();
    let loaded = ProjectManifest::parse_toml_bytes(&encoded);

    assert!(!loaded.has_errors(), "{:?}", loaded.diagnostics);
    assert_eq!(loaded.manifest.unwrap(), expected);
}
