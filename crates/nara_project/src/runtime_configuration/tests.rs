use std::time::Duration;

use nara_app::tasks::TaskShutdownPolicy;
use nara_app::{FixedCatchUpPolicy, FixedTime};
use nara_core::diagnostic::DiagnosticValueRef;
use nara_core::{ByteLimit, TimeLimit};
use nara_scene::runtime_configuration::{EffectiveRuntimeConfiguration, RuntimeConfiguration};

use crate::{
    EffectiveProjectSettings, EffectiveRuntimeSettings, MAX_PROJECT_FIXED_DEBT_STEPS,
    MAX_PROJECT_FIXED_STEPS_PER_FRAME, ProjectAuthoringRecovery, ProjectAuthoringRecoveryDigest,
    ProjectAuthoringState, ProjectManifest, ProjectPath, ProjectPresentMode, ProjectWindowMode,
    RuntimeConfigurationError, RuntimePreset,
};

const PROJECT_SOURCE: &str = r#"
schema_version = 1
[project]
name = "Source project"
source_universe = "72011426-37e2-48ba-bc06-78841468889a"
native_document_identity = "stable_metadata"
stable_id = "source-only-coordinate"
[paths]
assets = "source-assets"
[authoring]
state = "ready"
schema_bundle = "source/schema.bundle.json"
[startup]
default_scene = "source/start.scene.json"
[content]
scenes = ["source/extra.scene.json"]
[input]
action_map = "source/input.actions.json"

[profiles.release.input]
clear_action_map = true
[profiles.release.execution]
contract = "desktop-supervised-play/v1"
[profiles.release.capabilities]
requested = ["runtime-2d", "runtime-ui"]
[profiles.release.runtime]
paused = true
time_scale = 0.5
max_delta_seconds = 0.123456789
fixed_timestep_seconds = 0.010000001
max_fixed_steps_per_frame = 7
max_fixed_debt_steps = 35
catch_up_policy = "preserve-debt"
preset = "local-headless"
[profiles.release.tasks.io]
workers = 2
pending_capacity = 31
[profiles.release.tasks.compute]
workers = 3
pending_capacity = 32
[profiles.release.tasks.async_compute]
workers = 4
pending_capacity = 33
[profiles.release.tasks.shutdown]
drain_timeout_ms = 101
cancel_timeout_ms = 102
join_timeout_ms = 103
[profiles.release.render.world_canvas]
width = 640
height = 360
[profiles.release.ui.primary_surface]
logical_width = 1280
logical_height = 720
[profiles.release.ui.primary_surface.safe_area]
left = 1
top = 2
right = 3
bottom = 4
[profiles.release.window]
title = "Resolved profile"
width = 1600
height = 900
scale_factor = 1.5
mode = "fullscreen"
present_mode = "immediate"
resizable = false
[profiles.release.diagnostics]
runtime_capacity = 41
"#;

fn project_manifest() -> ProjectManifest {
    let load = ProjectManifest::parse_toml_str(PROJECT_SOURCE);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    load.manifest.unwrap()
}

fn project_settings() -> EffectiveProjectSettings {
    project_manifest().resolve_profile(Some("release")).unwrap()
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
fn source_server_projection_uses_product_invariants() {
    let mut settings = project_settings();
    settings.runtime_preset = RuntimePreset::Server;
    settings.window.enabled = true;
    settings.runtime = EffectiveRuntimeSettings::new(
        settings.runtime.runtime_time_settings(),
        settings
            .runtime
            .fixed_time()
            .with_catch_up_policy(FixedCatchUpPolicy::DiscardExcess),
    );
    let projected = settings.runtime_configuration().unwrap();
    settings.enforce_product_invariants();
    assert_eq!(projected.window(), &settings.window);
    assert_eq!(projected.runtime(), &settings.runtime);
}

#[test]
fn base_server_preset_does_not_override_a_final_custom_or_dev_minimal_preset() {
    let source = r#"
schema_version = 1
[project]
name = "Final preset admission"
source_universe = "72011426-37e2-48ba-bc06-78841468889a"
native_document_identity = "stable_metadata"
[authoring]
state = "ready"
[runtime]
preset = "server"
[window]
enabled = true
[profiles.custom.runtime]
preset = "minimal"
[profiles.dev.runtime]
preset = "minimal"
"#;
    let manifest = ProjectManifest::parse_toml_str(source).manifest.unwrap();
    let base = manifest.resolve_profile(None).unwrap();
    assert!(!base.window.enabled);
    assert_eq!(
        base.runtime.fixed_time().catch_up_policy(),
        FixedCatchUpPolicy::PreserveDebt
    );
    for name in ["custom", "dev"] {
        let resolved = manifest.resolve_profile(Some(name)).unwrap();
        assert_eq!(resolved.runtime_preset, RuntimePreset::Minimal);
        assert!(resolved.window.enabled, "profile={name}");
        assert_eq!(
            resolved.runtime.fixed_time().catch_up_policy(),
            FixedCatchUpPolicy::DiscardExcess
        );
        assert_eq!(
            resolved.runtime_configuration().unwrap().runtime(),
            &resolved.runtime
        );
    }
}

#[test]
fn final_server_and_named_headless_profiles_apply_their_final_invariants() {
    let source = r#"
schema_version = 1
[project]
name = "Named preset invariants"
source_universe = "72011426-37e2-48ba-bc06-78841468889a"
native_document_identity = "stable_metadata"
[authoring]
state = "ready"
[runtime]
preset = "server"
[window]
enabled = true
[profiles.final_server.runtime]
preset = "server"
catch_up_policy = "discard-excess"
[profiles.final_server.window]
enabled = true
[profiles.server.runtime]
preset = "minimal"
catch_up_policy = "discard-excess"
[profiles.server.window]
enabled = true
[profiles.headless.runtime]
preset = "minimal"
[profiles.headless.window]
enabled = true
"#;
    let manifest = ProjectManifest::parse_toml_str(source).manifest.unwrap();
    for profile in [None, Some("final_server"), Some("server")] {
        let resolved = manifest.resolve_profile(profile).unwrap();
        assert_eq!(resolved.runtime_preset, RuntimePreset::Server);
        assert!(!resolved.window.enabled);
        assert_eq!(
            resolved.runtime.fixed_time().catch_up_policy(),
            FixedCatchUpPolicy::PreserveDebt
        );
        let projected = resolved.runtime_configuration().unwrap();
        assert_eq!(projected.runtime(), &resolved.runtime);
        assert_eq!(projected.window(), &resolved.window);
        assert_round_trip(&projected);
    }
    let headless = manifest.resolve_profile(Some("headless")).unwrap();
    assert_eq!(headless.runtime_preset, RuntimePreset::LocalHeadless);
    assert!(!headless.window.enabled);
    assert_eq!(
        headless.runtime.fixed_time().catch_up_policy(),
        FixedCatchUpPolicy::DiscardExcess
    );
}

#[test]
fn source_input_requires_an_explicit_profile_clear_and_does_not_leak_its_path() {
    let mut source = project_manifest().resolve_profile(None).unwrap();
    let failure = source.runtime_configuration().unwrap_err();
    assert_diagnostic(
        &failure,
        "runtime.configuration.source-input-required",
        "input.action_map",
    );
    assert!(!format!("{failure:?}").contains("source/input.actions.json"));
    source.input.action_map = Some(ProjectPath::new("private-canary/actions.json").unwrap());
    let failure = source.runtime_configuration().unwrap_err();
    assert!(!format!("{failure:?}").contains("private-canary"));
    let cleared = project_settings().runtime_configuration().unwrap();
    assert!(cleared.input().action_map.is_none());
}

#[test]
fn source_projection_retains_an_owned_unfulfilled_input_requirement() {
    let mut source = project_settings();
    let expected_runtime = source.runtime_configuration().unwrap();
    let input_path = ProjectPath::new("source/input.actions.json").unwrap();
    source.input.action_map = Some(input_path.clone());
    let original = source.clone();

    let projection: crate::SourceRuntimeProjection = source.source_runtime_projection().unwrap();
    assert_eq!(source, original);
    assert_eq!(projection.runtime(), &expected_runtime);
    assert!(projection.runtime().input().action_map.is_none());
    assert_eq!(projection.input_requirement(), Some(&input_path));
    assert_diagnostic(
        &source.runtime_configuration().unwrap_err(),
        "runtime.configuration.source-input-required",
        "input.action_map",
    );
    assert_round_trip(projection.runtime());
    assert!(projection.input_requirement().is_some());

    source.input.action_map = Some(ProjectPath::new("other/input.actions.json").unwrap());
    drop(source);
    assert_eq!(projection.input_requirement(), Some(&input_path));
    assert_eq!(projection.runtime(), &expected_runtime);
}

#[test]
fn source_projection_preserves_profile_input_selection_and_clear() {
    let source = format!(
        "{PROJECT_SOURCE}\n[profiles.custom.input]\naction_map = 'input/profile.actions.json'\n"
    );
    let load = ProjectManifest::parse_toml_str(&source);
    assert!(!load.has_errors(), "{:?}", load.diagnostics);
    let manifest = load.manifest.unwrap();

    for (profile, expected_path) in [
        (None, Some("source/input.actions.json")),
        (Some("custom"), Some("input/profile.actions.json")),
        (Some("release"), None),
    ] {
        let settings = manifest.resolve_profile(profile).unwrap();
        let projection = settings.source_runtime_projection().unwrap();
        assert_eq!(
            projection.input_requirement().map(ProjectPath::as_str),
            expected_path,
            "profile={profile:?}",
        );
        assert_eq!(
            projection.input_requirement(),
            settings.input.action_map.as_ref(),
        );
        assert!(projection.runtime().input().action_map.is_none());
        if expected_path.is_none() {
            assert_eq!(
                projection.runtime(),
                &settings.runtime_configuration().unwrap(),
            );
        } else {
            assert_diagnostic(
                &settings.runtime_configuration().unwrap_err(),
                "runtime.configuration.source-input-required",
                "input.action_map",
            );
        }
    }
}

#[test]
fn resolved_profile_exports_all_values_without_source_or_profile_dependencies() {
    let manifest = project_manifest();
    let encoded = manifest.to_canonical_toml_bytes().unwrap();
    let reparsed = ProjectManifest::parse_toml_bytes(&encoded)
        .manifest
        .unwrap();
    assert_eq!(reparsed, manifest);
    let settings = reparsed.resolve_profile(Some("release")).unwrap();
    let effective = settings.runtime_configuration().unwrap();
    assert_eq!(effective.runtime_preset(), settings.runtime_preset);
    assert_eq!(
        effective.requested_capabilities(),
        settings.requested_capabilities
    );
    assert_eq!(effective.runtime(), &settings.runtime);
    assert_eq!(effective.tasks(), &settings.tasks);
    assert_eq!(effective.render(), &settings.render);
    assert_eq!(effective.ui(), &settings.ui);
    assert_eq!(effective.window(), &settings.window);
    assert!(effective.input().action_map.is_none());
    assert_eq!(effective.diagnostics(), &settings.diagnostics);
    assert_eq!(effective.window().title, "Resolved profile");
    assert_eq!(effective.window().scale_factor, 1.5);
    assert_eq!(effective.window().mode, ProjectWindowMode::Fullscreen);
    assert_eq!(
        effective.window().present_mode,
        ProjectPresentMode::Immediate
    );
    assert_eq!(effective.diagnostics().runtime.entry_limit().get(), 41);
    assert_round_trip(&effective);

    let mut different_source = settings.clone();
    different_source.schema_version = 0;
    different_source.project.name.clear();
    different_source.profile_name = Some("different-profile".to_owned());
    different_source.paths.assets = ProjectPath::new("other-assets").unwrap();
    different_source.authoring_schema_bundle = None;
    different_source.content.scenes.clear();
    different_source.startup.default_scene = None;
    different_source.execution.contract = None;
    assert_eq!(different_source.runtime_configuration().unwrap(), effective);
    let json = serde_json::to_value(effective.to_configuration()).unwrap();
    let fields = json.as_object().unwrap();
    assert_eq!(fields.len(), 8);
    for field in [
        "capabilities",
        "runtime",
        "tasks",
        "render",
        "ui",
        "window",
        "input",
        "diagnostics",
    ] {
        assert!(fields.contains_key(field));
    }
}

#[test]
fn pending_refuses_source_projection_but_does_not_change_source_free_values() {
    let mut manifest = project_manifest();
    let ready = manifest.resolve_profile(Some("release")).unwrap();
    assert_eq!(ready.authoring_state, ProjectAuthoringState::Ready);
    let runtime = ready.runtime_configuration().unwrap();
    let source_projection = ready.source_runtime_projection().unwrap();
    let pending = ProjectAuthoringState::Pending(
        ProjectAuthoringRecovery::new(
            ProjectPath::new(".nara/upgrades/recovery.json").unwrap(),
            ProjectAuthoringRecoveryDigest::from_parts(123, [0xab; 32]),
        )
        .unwrap(),
    );
    manifest.authoring.state = pending.clone();
    let mut settings = manifest.resolve_profile(Some("release")).unwrap();
    assert_eq!(settings.runtime_configuration().unwrap(), runtime);
    assert_diagnostic(
        &settings.source_runtime_projection().unwrap_err(),
        "runtime.configuration.source-authoring-pending",
        "authoring.state",
    );
    // Previously projected immutable values do not acquire current-source authority.
    assert_eq!(source_projection.runtime(), &runtime);
    assert_eq!(settings.authoring_state, pending);
    assert_round_trip(&runtime);
    let encoded = serde_json::to_value(runtime.to_configuration()).unwrap();
    assert!(encoded.get("authoring").is_none());
    assert!(encoded.get("authoring_state").is_none());

    settings.input.action_map = Some(ProjectPath::new("source/input.actions.json").unwrap());
    assert_diagnostic(
        &settings.source_runtime_projection().unwrap_err(),
        "runtime.configuration.source-authoring-pending",
        "authoring.state",
    );
    assert_diagnostic(
        &settings.runtime_configuration().unwrap_err(),
        "runtime.configuration.source-input-required",
        "input.action_map",
    );
    assert_eq!(settings.authoring_state, pending);
    settings.authoring_state = ProjectAuthoringState::Ready;
    let projected = settings.source_runtime_projection().unwrap();
    assert_eq!(projected.runtime(), &runtime);
    assert_eq!(
        projected.input_requirement(),
        settings.input.action_map.as_ref()
    );
}

#[test]
fn projection_revalidates_public_effective_primitives() {
    let mut settings = project_settings();
    settings.window.width = 0;
    settings.window.scale_factor = f64::NAN;
    settings.render.world_canvas.as_mut().unwrap().height = 0;
    settings.ui.primary_surface.as_mut().unwrap().safe_area.left = u32::MAX;
    // These domain settings are valid at the App layer but exceed the project admission bounds.
    let runtime = settings.runtime.runtime_time_settings();
    let fixed = FixedTime::new(Duration::from_nanos(10_000_001))
        .unwrap()
        .with_max_steps_per_frame(MAX_PROJECT_FIXED_STEPS_PER_FRAME + 1)
        .unwrap()
        .with_max_debt_steps(MAX_PROJECT_FIXED_DEBT_STEPS + 1)
        .unwrap();
    settings.runtime = EffectiveRuntimeSettings::new(runtime, fixed);
    let runtime_failure = settings.runtime_configuration().unwrap_err();
    settings.input.action_map = Some(ProjectPath::new("source/input.actions.json").unwrap());
    let source_failure = settings.source_runtime_projection().unwrap_err();
    for failure in [&runtime_failure, &source_failure] {
        for (code, field) in [
            (
                "project.runtime.max-fixed-steps-too-large",
                "runtime.max_fixed_steps_per_frame",
            ),
            (
                "project.runtime.max-fixed-debt-too-large",
                "runtime.max_fixed_debt_steps",
            ),
            ("project.window.invalid-width", "window.width"),
            ("project.window.invalid-scale-factor", "window.scale_factor"),
            (
                "project.render.invalid-world-canvas-extent",
                "render.world_canvas.height",
            ),
            (
                "project.ui.invalid-horizontal-safe-area",
                "ui.primary_surface.safe_area",
            ),
        ] {
            assert_diagnostic(failure, code, field);
        }
    }
}

#[test]
fn projection_rejects_unrepresentable_task_and_diagnostic_policies() {
    let mut settings = project_settings();
    let policy = settings.tasks.pool_config.shutdown_policy();
    settings.tasks.pool_config = settings
        .tasks
        .pool_config
        .with_shutdown_policy(TaskShutdownPolicy::new(
            TimeLimit::new(Duration::from_micros(101_001)).unwrap(),
            policy.cancel_timeout(),
            policy.join_timeout(),
        ))
        .unwrap();
    settings.diagnostics.runtime = settings
        .diagnostics
        .runtime
        .with_summary_byte_limit(ByteLimit::new(42).unwrap())
        .unwrap();
    let runtime_failure = settings.runtime_configuration().unwrap_err();
    settings.input.action_map = Some(ProjectPath::new("source/input.actions.json").unwrap());
    let source_failure = settings.source_runtime_projection().unwrap_err();
    for failure in [&runtime_failure, &source_failure] {
        assert_diagnostic(
            failure,
            "runtime.configuration.unrepresentable-tasks",
            "tasks",
        );
        assert_diagnostic(
            failure,
            "runtime.configuration.unrepresentable-diagnostics",
            "diagnostics",
        );
    }
}

#[test]
fn source_lowered_durations_export_exactly_at_the_float_precision_boundary() {
    // Source remains authored in seconds. Once it lowers, exact integer records preserve the
    // selected nanoseconds without any inverse floating-point conversion or magnitude bound.
    let boundary = 8_388_608.0_f64;
    for seconds in [
        0.5e-9_f64.next_up(),
        1.5e-9,
        boundary.next_down(),
        boundary,
        boundary.next_up(),
        boundary.next_up().next_up(),
        18_446_744_073_709_549_568.0,
    ] {
        let source = format!(
            "schema_version = 1\n[project]\nname = 'Exact source duration'\n\
             source_universe = '72011426-37e2-48ba-bc06-78841468889a'\n\
             native_document_identity = 'stable_metadata'\n[authoring]\nstate = 'ready'\n[runtime]\n\
             max_delta_seconds = {seconds:?}\nfixed_timestep_seconds = {seconds:?}\n"
        );
        let load = ProjectManifest::parse_toml_str(&source);
        assert!(
            !load.has_errors(),
            "seconds={seconds:?}: {:?}",
            load.diagnostics
        );
        let settings = load.manifest.unwrap().resolve_profile(None).unwrap();
        let effective = settings.runtime_configuration().unwrap();
        assert_eq!(effective.runtime(), &settings.runtime);
        assert_json_round_trip(&effective);
    }
}

#[test]
fn source_projection_preserves_durations_that_float_seconds_cannot_encode() {
    let mut settings = project_settings();
    let exact = Duration::new(8_388_608, 1);
    let runtime = settings
        .runtime
        .runtime_time_settings()
        .with_max_delta(exact)
        .unwrap();
    settings.runtime = EffectiveRuntimeSettings::new(runtime, FixedTime::new(exact).unwrap());
    let effective = settings.runtime_configuration().unwrap();
    assert_eq!(effective.runtime(), &settings.runtime);
    assert_eq!(effective.to_configuration().runtime.max_delta, exact);
    assert_eq!(effective.to_configuration().runtime.fixed_timestep, exact);
    assert_round_trip(&effective);

    settings.input.action_map = Some(ProjectPath::new("source/input.actions.json").unwrap());
    let projection = settings.source_runtime_projection().unwrap();
    assert_eq!(projection.runtime(), &effective);
}

#[test]
fn source_projection_preserves_maximum_duration_in_json() {
    let mut settings = project_settings();
    let runtime = settings
        .runtime
        .runtime_time_settings()
        .with_max_delta(Duration::MAX)
        .unwrap();
    settings.runtime =
        EffectiveRuntimeSettings::new(runtime, FixedTime::new(Duration::MAX).unwrap());
    let projected = settings.runtime_configuration().unwrap();
    assert_eq!(projected.runtime(), &settings.runtime);
    assert_json_round_trip(&projected);
}
