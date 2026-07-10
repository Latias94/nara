use super::*;
use nara_app::{FixedCatchUpPolicy, FixedTime};
use nara_diagnostic::MAX_RUNTIME_DIAGNOSTICS_CAPACITY;
use nara_tasks::{
    MAX_TASK_POOL_PENDING_PER_KIND, MAX_TASK_POOL_PENDING_TOTAL, MAX_TASK_POOL_THREADS_PER_KIND,
    MAX_TASK_POOL_THREADS_TOTAL, MAX_TASK_SHUTDOWN_PHASE_TIMEOUT, TaskPoolKind,
};
use std::{fs, time::Duration};

const MINIMAL_MANIFEST: &str = r#"
schema_version = 1

[project]
name = "Test Game"
"#;
const COMPLETE_MANIFEST: &str = include_str!("../tests/fixtures/complete_v1.toml");

fn diagnostic_codes(load: &ProjectManifestLoad) -> Vec<&str> {
    load.diagnostics
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect()
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
    assert_eq!(settings.plugin_plan, ProjectPluginPlan::Minimal);
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
fn complete_v1_fixture_lowers_every_runtime_and_task_field() {
    let load = ProjectManifest::parse_toml_str(COMPLETE_MANIFEST);
    assert!(!load.has_errors(), "{:?}", load.diagnostics.diagnostics());
    let settings = load.manifest.unwrap().resolve_profile(None).unwrap();

    assert_eq!(settings.plugin_plan, ProjectPluginPlan::Runtime2d);
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
        load.diagnostics.diagnostics()[0].code.as_str(),
        "project.manifest.parse"
    );
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
    let diagnostics = load.diagnostics.diagnostics();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code.as_str() == "project.path.invalid"
            && diagnostic.context.field_path.as_deref() == Some("paths.assets")
    }));
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
    assert!(base.diagnostics.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "project.runtime.max-fixed-steps-too-large"
            && diagnostic.context.field_path.as_deref() == Some("runtime.max_fixed_steps_per_frame")
    }));
    assert!(base.diagnostics.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "project.runtime.max-fixed-debt-too-large"
            && diagnostic.context.field_path.as_deref() == Some("runtime.max_fixed_debt_steps")
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
    assert!(profile.diagnostics.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "project.runtime.max-fixed-steps-too-large"
            && diagnostic.context.field_path.as_deref()
                == Some("profiles.dev.runtime.max_fixed_steps_per_frame")
    }));
    assert!(profile.diagnostics.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "project.runtime.max-fixed-debt-too-large"
            && diagnostic.context.field_path.as_deref()
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
    assert!(
        !boundary.has_errors(),
        "{:?}",
        boundary.diagnostics.diagnostics()
    );
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
fn obsolete_runtime_2d_plugin_plan_alias_is_rejected() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Obsolete Plugin Plan"

[runtime]
plugin_plan = "runtime2d"
"#,
    );

    assert!(load.manifest.is_none());
    assert!(diagnostic_codes(&load).contains(&"project.manifest.parse"));
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

    assert_eq!(settings.plugin_plan, ProjectPluginPlan::Server);
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
fn final_server_plugin_plan_enforces_server_runtime_invariants_without_named_profile() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Direct Server Plan"

[runtime]
plugin_plan = "server"
catch_up_policy = "discard-excess"
max_fixed_debt_steps = 17

[window]
enabled = true
"#,
    );
    let settings = load.manifest.unwrap().resolve_profile(None).unwrap();

    assert_eq!(settings.plugin_plan, ProjectPluginPlan::Server);
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
plugin_plan = "runtime-2d"

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
plugin_plan = "headless-runtime"

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
    assert!(!load.has_errors(), "{:?}", load.diagnostics.diagnostics());
    let settings = load.manifest.unwrap().resolve_profile(Some("dev")).unwrap();

    assert_eq!(settings.plugin_plan, ProjectPluginPlan::HeadlessRuntime);
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
    assert_eq!(settings.diagnostics.runtime.capacity, 32);

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
plugin_plan = "desktop-wgpu"
catch_up_policy = "discard-excess"
max_fixed_debt_steps = 7

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

    assert_eq!(settings.plugin_plan, ProjectPluginPlan::Server);
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
        MAX_RUNTIME_DIAGNOSTICS_CAPACITY + 1
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
    assert!(load.diagnostics.diagnostics().iter().any(|diagnostic| {
        diagnostic.code.as_str() == "project.window.invalid-width"
            && diagnostic.context.field_path.as_deref() == Some("window.width")
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
    assert_eq!(
        diagnostics.diagnostics()[0].code.as_str(),
        "project.profile.unknown"
    );
}

#[test]
fn file_loader_enforces_manifest_size_budget() {
    let temp_root = std::env::temp_dir().join(format!(
        "nara_project_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&temp_root).unwrap();
    let manifest_path = temp_root.join("nara.toml");
    fs::write(&manifest_path, MINIMAL_MANIFEST).unwrap();

    let load = ProjectManifest::parse_toml_file_with_limit(&manifest_path, 4);

    assert!(load.manifest.is_none());
    assert_eq!(
        load.diagnostics.diagnostics()[0].code.as_str(),
        "project.manifest.too-large"
    );

    fs::remove_dir_all(&temp_root).unwrap();
}
