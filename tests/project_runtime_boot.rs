#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    fs,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    app::{
        App, CoreStage, FixedTime, FixedUpdateSet, Plugin, PluginCategory, PluginDeclaration,
        PluginDefinition, PluginDefinitionId, PluginError, PluginId, PluginShutdownObligationId,
        RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
        RuntimeCloseParticipantId, RuntimeCloseProgress,
    },
    ecs::{Query, Res, ResMut, Resource, schedule::IntoScheduleConfigs},
    gameplay::{
        GAMEPLAY_COMMAND_PLUGIN_ID, GameplayCommandDraft, GameplayCommandIngressSource,
        GameplayCommandPlugin, GameplayCommandSet, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTick, GameplayCommandTypeId,
    },
    project_host::{HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport},
    scene::SceneEntitySource,
};
use project_content_fixture::TestProject;

const BOOT_OUTCOME_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-boot-outcome");
const BOOT_OUTCOME_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-boot-outcome", 1);
const BOOT_OUTCOME_REQUIREMENTS: &[PluginId] = &[GAMEPLAY_COMMAND_PLUGIN_ID];
const BOOT_OUTCOME_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(BOOT_OUTCOME_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(BOOT_OUTCOME_REQUIREMENTS);
const STALLED_CLOSE_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-stalled-close");
const STALLED_CLOSE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-stalled-close", 1);
const STALLED_CLOSE_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.project-stalled-close");
const STALLED_CLOSE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.project-stalled-close");
const STALLED_CLOSE_REQUIREMENTS: &[PluginId] = &[BOOT_OUTCOME_PLUGIN_ID];
const STALLED_CLOSE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(STALLED_CLOSE_PLUGIN_ID, PluginCategory::Service)
        .requires_plugins(STALLED_CLOSE_REQUIREMENTS)
        .shutdown_obligations(&[STALLED_CLOSE_OBLIGATION]);

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
struct BootOutcome {
    tick: u64,
    scene_entities: Vec<String>,
}

#[derive(Debug, Default)]
struct BootOutcomePlugin;

impl Plugin for BootOutcomePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &BOOT_OUTCOME_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(BootOutcome::default())?.add_systems(
            CoreStage::FixedUpdate,
            capture_boot_outcome
                .in_set(FixedUpdateSet::Finalize)
                .after(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}

#[derive(Debug)]
struct StalledClosePlugin {
    released: Arc<AtomicBool>,
}

impl Plugin for StalledClosePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &STALLED_CLOSE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_plugin_runtime_close_participant(
            STALLED_CLOSE_OBLIGATION,
            STALLED_CLOSE_PARTICIPANT,
            StalledCloseParticipant {
                released: Arc::clone(&self.released),
            },
        )?;
        Ok(())
    }
}

struct StalledCloseParticipant {
    released: Arc<AtomicBool>,
}

impl RuntimeCloseParticipant for StalledCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.progress())
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(self.progress())
    }
}

impl StalledCloseParticipant {
    fn progress(&self) -> RuntimeCloseProgress {
        if self.released.load(Ordering::SeqCst) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        }
    }
}

fn capture_boot_outcome(
    fixed_time: Res<FixedTime>,
    scene_entities: Query<&SceneEntitySource>,
    mut outcome: ResMut<BootOutcome>,
) {
    outcome.tick = fixed_time.tick();
    outcome.scene_entities = scene_entities
        .iter()
        .map(|source| source.entity_id.as_str().to_owned())
        .collect();
    outcome.scene_entities.sort();
}

fn boot_outcome_plugin() -> PluginDefinition {
    PluginDefinition::infallible::<BootOutcomePlugin, _>(
        BOOT_OUTCOME_DEFINITION_ID,
        b"project-boot-outcome-v1",
        BootOutcomePlugin::default,
    )
}

fn stalled_close_plugin(
    released: &Arc<AtomicBool>,
    instances: &Arc<AtomicUsize>,
) -> PluginDefinition {
    let released = Arc::clone(released);
    let instances = Arc::clone(instances);
    PluginDefinition::infallible::<StalledClosePlugin, _>(
        STALLED_CLOSE_DEFINITION_ID,
        b"project-stalled-close-v1",
        move || {
            instances.fetch_add(1, Ordering::SeqCst);
            StalledClosePlugin {
                released: Arc::clone(&released),
            }
        },
    )
}

fn project_run(project: &TestProject) -> HeadlessRun<BootOutcome> {
    let intent = HeadlessRunIntent::new(NonZeroU32::new(1).unwrap())
        .insert_after::<GameplayCommandPlugin>(boot_outcome_plugin());
    HeadlessRun::new(project.root_capability(), intent, Vec::new())
}

fn execute_to_terminal(run: &mut HeadlessRun<BootOutcome>) -> HeadlessRunReport<BootOutcome> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let report = run.execute_bounded();
        if report.outcome() != &HeadlessRunOutcome::CleanupIncomplete {
            return report;
        }
        assert!(Instant::now() < deadline, "product cleanup did not finish");
        std::thread::yield_now();
    }
}

fn select_local_headless_profile(project: &TestProject) {
    fs::write(
        project.path().join("nara.toml"),
        r#"schema_version = 1

[project]
name = "Runtime Boot Test"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d"]
"#,
    )
    .unwrap();
}

#[test]
fn product_action_boots_authorized_content_runs_one_tick_and_closes() {
    let project = TestProject::with_prefab_startup();
    select_local_headless_profile(&project);
    let mut run = project_run(&project);

    let first = execute_to_terminal(&mut run);
    assert_eq!(
        first.outcome(),
        &HeadlessRunOutcome::Completed(BootOutcome {
            tick: 1,
            scene_entities: vec!["enemy-anchor".to_owned(), "enemy-anchor/enemy".to_owned()],
        }),
        "{first:?}"
    );
    assert!(!first.diagnostics().has_errors());

    let terminal_repeat = run.execute_bounded();
    assert_eq!(terminal_repeat, first);
}

#[test]
fn project_content_failure_returns_without_a_runtime_outcome() {
    let project = TestProject::with_prefab_startup();
    select_local_headless_profile(&project);
    project.write_scene_bytes(b"{");
    let mut run = project_run(&project);

    let report = run.execute_bounded();

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(report.diagnostics().has_errors());
}

#[test]
fn incomplete_cleanup_retries_the_same_owner_without_restarting_the_project() {
    let project = TestProject::with_prefab_startup();
    select_local_headless_profile(&project);
    let released = Arc::new(AtomicBool::new(false));
    let instances = Arc::new(AtomicUsize::new(0));
    let intent = HeadlessRunIntent::new(NonZeroU32::new(1).unwrap())
        .with_cleanup_timeout(Duration::ZERO)
        .insert_after::<GameplayCommandPlugin>(boot_outcome_plugin())
        .insert_after::<BootOutcomePlugin>(stalled_close_plugin(&released, &instances));
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());

    let incomplete = run.execute_bounded();
    assert_eq!(incomplete.outcome(), &HeadlessRunOutcome::CleanupIncomplete);
    assert_eq!(instances.load(Ordering::SeqCst), 1);

    released.store(true, Ordering::SeqCst);
    let completed = execute_to_terminal(&mut run);
    assert_eq!(
        completed.outcome(),
        &HeadlessRunOutcome::Completed(BootOutcome {
            tick: 1,
            scene_entities: vec!["enemy-anchor".to_owned(), "enemy-anchor/enemy".to_owned()],
        })
    );
    assert_eq!(instances.load(Ordering::SeqCst), 1);
}

#[test]
fn product_action_stops_on_the_first_terminal_snapshot() {
    let project = TestProject::with_prefab_startup();
    select_local_headless_profile(&project);
    let intent = HeadlessRunIntent::new(NonZeroU32::new(4).unwrap())
        .stop_when(|outcome: &BootOutcome| outcome.tick >= 2)
        .insert_after::<GameplayCommandPlugin>(boot_outcome_plugin());
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());

    let report = execute_to_terminal(&mut run);

    let HeadlessRunOutcome::Completed(outcome) = report.outcome() else {
        panic!("terminal project action failed: {report:#?}");
    };
    assert_eq!(outcome.tick, 2);
    assert!(!report.diagnostics().has_errors());
}

#[test]
fn product_action_reports_a_tick_limit_before_publishing_success() {
    let project = TestProject::with_prefab_startup();
    select_local_headless_profile(&project);
    let intent = HeadlessRunIntent::new(NonZeroU32::new(2).unwrap())
        .stop_when(|_: &BootOutcome| false)
        .insert_after::<GameplayCommandPlugin>(boot_outcome_plugin());
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());

    let report = execute_to_terminal(&mut run);

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.run.tick-limit"
            && diagnostic.summary().as_str() == "Headless project run reached its fixed-tick limit"
    }));
}

#[test]
fn source_label_cannot_downgrade_a_trusted_product_command_rejection() {
    let project = TestProject::with_prefab_startup();
    select_local_headless_profile(&project);
    let intent = HeadlessRunIntent::new(NonZeroU32::new(1).unwrap())
        .insert_after::<GameplayCommandPlugin>(boot_outcome_plugin());
    let commands = (1..=4_097)
        .map(|sequence| {
            GameplayCommandSubmission::new(
                GameplayCommandTick::new(1).unwrap(),
                GameplayCommandIngressSource::external("nara.test.external").unwrap(),
                GameplayCommandSourceSequence::new(sequence).unwrap(),
                GameplayCommandDraft::new(
                    GameplayCommandTypeId::new("nara.test.external-command").unwrap(),
                ),
            )
        })
        .collect::<Vec<_>>();
    let mut run = HeadlessRun::new(project.root_capability(), intent, commands);

    let report = execute_to_terminal(&mut run);

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "project.run.command-rejected" })
    );
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "project.run.runtime-faulted" })
    );
}
