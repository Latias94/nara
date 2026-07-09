use super::*;
use nara_diagnostic::MAX_RUNTIME_DIAGNOSTICS_CAPACITY;
use nara_tasks::{MAX_TASK_POOL_THREADS_PER_KIND, TaskExecutionMode, TaskPoolKind};
use std::fs;

const MINIMAL_MANIFEST: &str = r#"
schema_version = 1

[project]
name = "Test Game"
"#;

#[test]
fn minimal_manifest_parses_and_resolves_defaults() {
    let load = ProjectManifest::parse_toml_str(MINIMAL_MANIFEST);

    assert!(!load.has_errors());
    let manifest = load.manifest.unwrap();
    let settings = manifest.resolve_profile(None).unwrap();

    assert_eq!(settings.project.name, "Test Game");
    assert_eq!(settings.paths.assets.as_str(), "assets");
    assert_eq!(settings.paths.import_cache.as_str(), ".nara/import-cache");
    assert_eq!(settings.plugin_plan, ProjectPluginPlan::Minimal);
    assert_eq!(settings.runtime.fixed_time().max_steps_per_frame(), 5);
    assert!(settings.window.to_window().is_some());
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
fn server_profile_infers_headless_plugin_and_deterministic_tasks() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Server Game"

[profiles.server]
"#,
    );
    let manifest = load.manifest.unwrap();

    let settings = manifest.resolve_profile(Some("server")).unwrap();

    assert_eq!(settings.plugin_plan, ProjectPluginPlan::Server);
    assert!(!settings.window.enabled);
    assert_eq!(settings.tasks.mode, ProjectTaskExecutionMode::Deterministic);
    assert_eq!(
        settings.tasks.pool_config.execution_mode(),
        TaskExecutionMode::Deterministic
    );
    assert_eq!(settings.tasks.pool_config.threads_for(TaskPoolKind::Io), 0);
}

#[test]
fn profile_overlay_overrides_effective_values() {
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
time_scale = 0.5
max_fixed_steps_per_frame = 2
plugin_plan = "headless-runtime"

[profiles.dev.tasks]
mode = "threaded"
io_threads = 3
compute_threads = 4
async_compute_threads = 5

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
    let manifest = load.manifest.unwrap();

    let settings = manifest.resolve_profile(Some("dev")).unwrap();

    assert_eq!(settings.plugin_plan, ProjectPluginPlan::HeadlessRuntime);
    assert_eq!(settings.runtime.time_scale, 0.5);
    assert_eq!(settings.runtime.fixed_time().max_steps_per_frame(), 2);
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
    assert_eq!(settings.tasks.pool_config.threads_for(TaskPoolKind::Io), 3);
    assert_eq!(
        settings
            .tasks
            .pool_config
            .threads_for(TaskPoolKind::Compute),
        4
    );
    assert_eq!(
        settings
            .tasks
            .pool_config
            .threads_for(TaskPoolKind::AsyncCompute),
        5
    );
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
    let manifest = load.manifest.unwrap();

    let settings = manifest.resolve_profile(Some("dev")).unwrap();

    assert_eq!(settings.startup.default_scene, None);
    assert_eq!(settings.input.action_map, None);
}

#[test]
fn server_profile_invariants_survive_unsafe_overlay_values() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Hostile Server Game"

[profiles.server.runtime]
plugin_plan = "desktop-wgpu"

[profiles.server.tasks]
mode = "threaded"
io_threads = 4
compute_threads = 4
async_compute_threads = 4

[profiles.server.window]
enabled = true
"#,
    );
    let manifest = load.manifest.unwrap();

    let settings = manifest.resolve_profile(Some("server")).unwrap();

    assert_eq!(settings.plugin_plan, ProjectPluginPlan::Server);
    assert!(!settings.window.enabled);
    assert_eq!(settings.tasks.mode, ProjectTaskExecutionMode::Deterministic);
    assert_eq!(
        settings.tasks.pool_config.execution_mode(),
        TaskExecutionMode::Deterministic
    );
}

#[test]
fn deterministic_profile_patch_can_explicitly_use_zero_threads() {
    let load = ProjectManifest::parse_toml_str(
        r#"
schema_version = 1

[project]
name = "Deterministic Patch Game"

[profiles.dev.tasks]
mode = "deterministic"
io_threads = 0
compute_threads = 0
async_compute_threads = 0
"#,
    );

    assert!(!load.has_errors());
    let settings = load.manifest.unwrap().resolve_profile(Some("dev")).unwrap();
    assert_eq!(
        settings.tasks.pool_config.execution_mode(),
        TaskExecutionMode::Deterministic
    );
}

#[test]
fn manifest_rejects_resource_counts_above_engine_caps() {
    let load = ProjectManifest::parse_toml_str(&format!(
        r#"
schema_version = 1

[project]
name = "Oversized Game"

[tasks]
mode = "threaded"
io_threads = {}

[diagnostics]
runtime_capacity = {}
"#,
        MAX_TASK_POOL_THREADS_PER_KIND + 1,
        MAX_RUNTIME_DIAGNOSTICS_CAPACITY + 1
    ));

    assert!(load.has_errors());
    let codes = load
        .diagnostics
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"project.tasks.thread-count-too-large"));
    assert!(codes.contains(&"project.diagnostics.runtime-capacity-too-large"));
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
