#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    fs,
    num::NonZeroU32,
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe,
    app::{
        App, FixedCatchUpPolicy, FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginError,
        PluginId, RuntimeTimeSettings, StartupStage,
    },
    diagnostic::RuntimeDiagnostics,
    ecs::{Res, ResMut, Resource},
    fs::RelativePath,
    project::{ProjectPath, RuntimePreset},
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, ingest_project_manifest,
        resolve_product_recipe,
    },
    tasks::{TaskPoolKind, TaskPools},
};

use project_content_fixture::TestProject;

#[derive(Debug, Clone, Copy, Default, PartialEq, Resource)]
struct ConfigurationObservation {
    build_timestep: Duration,
    build_time_scale: f32,
    finish_timestep: Duration,
    paused: bool,
    time_scale: f32,
    max_delta: Duration,
    timestep: Duration,
    max_fixed_steps_per_frame: u32,
    max_fixed_debt_steps: u32,
    catch_up_policy: FixedCatchUpPolicy,
    diagnostic_capacity: usize,
    io_workers: usize,
    io_pending: usize,
}

#[derive(Default)]
struct ObserveConfiguration;

impl Plugin for ObserveConfiguration {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.runtime-configuration"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[
            nara::diagnostic::DIAGNOSTICS_PLUGIN_ID,
            nara::tasks::TASK_PLUGIN_ID,
        ]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let initial = ConfigurationObservation {
            build_timestep: app.world().resource::<FixedTime>().timestep(),
            build_time_scale: app.world().resource::<RuntimeTimeSettings>().time_scale(),
            ..ConfigurationObservation::default()
        };
        app.insert_resource(initial)?
            .add_systems(StartupStage::Runtime, observe)?;
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        let mut observation = *app.world().resource::<ConfigurationObservation>();
        observation.finish_timestep = app.world().resource::<FixedTime>().timestep();
        app.insert_resource(observation)?;
        Ok(())
    }
}

fn observe(
    time: Res<RuntimeTimeSettings>,
    fixed: Res<FixedTime>,
    diagnostics: Res<RuntimeDiagnostics>,
    pools: Res<TaskPools>,
    mut observation: ResMut<ConfigurationObservation>,
) {
    let io = pools.config().kind(TaskPoolKind::Io);
    *observation = ConfigurationObservation {
        build_timestep: observation.build_timestep,
        build_time_scale: observation.build_time_scale,
        finish_timestep: observation.finish_timestep,
        paused: time.paused(),
        time_scale: time.time_scale(),
        max_delta: time.max_delta(),
        timestep: fixed.timestep(),
        max_fixed_steps_per_frame: fixed.max_steps_per_frame(),
        max_fixed_debt_steps: fixed.max_debt_steps(),
        catch_up_policy: fixed.catch_up_policy(),
        diagnostic_capacity: diagnostics.settings().entry_limit().get(),
        io_workers: io.workers().get(),
        io_pending: io.pending().get(),
    };
}

#[test]
fn selected_source_profile_reaches_runtime_configuration_and_actual_startup() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let path = project.path().join("nara.toml");
    let base = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        format!(
            "{base}\n{}",
            r#"
[profiles.delivery.runtime]
paused = true
time_scale = 0.5
max_delta_seconds = 0.125
fixed_timestep_seconds = 0.0125
max_fixed_steps_per_frame = 6
max_fixed_debt_steps = 12
catch_up_policy = "preserve-debt"

[profiles.delivery.diagnostics]
runtime_capacity = 7

[profiles.delivery.tasks.io]
workers = 1
pending_capacity = 8
"#
        ),
    )
    .unwrap();
    let root = project.root_capability();
    let file = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&file, Some("delivery")).unwrap();
    let recipe = ProductRecipe::new()
        .add_plugin::<ObserveConfiguration>()
        .unwrap();
    let plan = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
    assert_eq!(
        candidate.settings().profile_name.as_deref(),
        Some("delivery")
    );
    assert_eq!(plan.lineage(), candidate.lineage());
    assert_eq!(plan.settings(), candidate.runtime_configuration());
    let delivered = plan.settings().to_configuration();
    assert_eq!(delivered.resolve().unwrap(), *plan.settings());
    assert_eq!(
        plan.settings().runtime_preset(),
        RuntimePreset::LocalHeadless
    );
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(1).unwrap(), Duration::ZERO)
        .with_profile("delivery")
        .with_recipe(recipe);
    let mut run = HeadlessRun::<ConfigurationObservation>::new(root, intent, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(observed) => {
                assert_eq!(
                    observed,
                    &ConfigurationObservation {
                        build_timestep: Duration::from_micros(12_500),
                        build_time_scale: 0.5,
                        finish_timestep: Duration::from_micros(12_500),
                        paused: true,
                        time_scale: 0.5,
                        max_delta: Duration::from_millis(125),
                        timestep: Duration::from_micros(12_500),
                        max_fixed_steps_per_frame: 6,
                        max_fixed_debt_steps: 12,
                        catch_up_policy: FixedCatchUpPolicy::PreserveDebt,
                        diagnostic_capacity: 7,
                        io_workers: 1,
                        io_pending: 8,
                    }
                );
                break;
            }
            HeadlessRunOutcome::Failed => panic!("{:?}", report.diagnostics()),
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

#[test]
fn source_relative_input_is_retained_as_an_obligation_until_explicit_profile_clear() {
    let project = TestProject::with_prefab_startup();
    let path = project.path().join("nara.toml");
    let base = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        format!(
            "{base}\n[input]\naction_map = 'input/game.actions.json'\n\
             [profiles.delivery.input]\nclear_action_map = true\n"
        ),
    )
    .unwrap();
    let root = project.root_capability();
    let file = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let input_path = ProjectPath::new("input/game.actions.json").unwrap();
    assert!(!project.path().join(input_path.as_str()).exists());
    let candidate = ingest_project_manifest(&file, None).unwrap();
    assert_eq!(candidate.input_requirement(), Some(&input_path));
    assert_eq!(
        candidate.settings().input.action_map.as_ref(),
        Some(&input_path)
    );
    assert!(candidate.settings().runtime_configuration().is_err());
    let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
    assert_eq!(plan.settings(), candidate.runtime_configuration());
    assert_eq!(plan.input_requirement(), Some(&input_path));
    let cloned_plan = plan.clone();
    drop(plan);
    drop(candidate);
    assert_eq!(cloned_plan.input_requirement(), Some(&input_path));

    let candidate = ingest_project_manifest(&file, Some("delivery")).unwrap();
    assert!(candidate.settings().input.action_map.is_none());
    assert!(candidate.input_requirement().is_none());
    let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
    assert!(plan.input_requirement().is_none());
}

#[test]
fn initial_clock_values_participate_in_the_resolved_plugin_identity() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let path = project.path().join("nara.toml");
    let base = fs::read_to_string(&path).unwrap();
    let fingerprint = |patch: &str| {
        fs::write(
            &path,
            format!("{base}\n[profiles.delivery.runtime]\n{patch}\n"),
        )
        .unwrap();
        let root = project.root_capability();
        let file = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&file, Some("delivery")).unwrap();
        resolve_product_recipe(&candidate, ProductRecipe::new())
            .unwrap()
            .plugin_plan()
            .fingerprint()
    };
    let baseline = fingerprint("");
    assert_eq!(fingerprint(""), baseline);
    for patch in [
        "paused = true",
        "time_scale = 0.5",
        "max_delta_seconds = 0.125",
        "fixed_timestep_seconds = 0.02",
        "max_fixed_steps_per_frame = 9",
        "max_fixed_debt_steps = 64",
        "catch_up_policy = 'preserve-debt'",
    ] {
        assert_ne!(fingerprint(patch), baseline, "{patch}");
    }
}
