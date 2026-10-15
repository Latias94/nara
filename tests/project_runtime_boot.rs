#![cfg(all(feature = "serde", feature = "runtime-2d"))]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe,
    app::{
        App, AppExitRequests, CoreStage, FixedTime, FixedUpdateSet, Plugin, PluginCategory,
        PluginDeclaration, PluginDefinition, PluginDefinitionId, PluginError, PluginId,
        PluginShutdownObligationId, RuntimeCloseContext, RuntimeCloseParticipant,
        RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
    },
    ecs::{Query, Res, ResMut, Resource, schedule::IntoScheduleConfigs},
    gameplay::{
        GAMEPLAY_COMMAND_PLUGIN_ID, GameplayCommandDraft, GameplayCommandIngressSource,
        GameplayCommandSet, GameplayCommandSourceSequence, GameplayCommandSubmission,
        GameplayCommandTick, GameplayCommandTypeId,
    },
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport,
        advanced::{RuntimeProbe, headless_run_with_probe},
    },
    scene::SceneEntitySource,
};
use project_content_fixture::{
    TestProject, image_meta, player_image_meta, scene_id, sprite_value, valid_png_bytes,
};

const BOOT_OUTCOME_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-boot-outcome");
const BOOT_OUTCOME_REQUIREMENTS: &[PluginId] = &[GAMEPLAY_COMMAND_PLUGIN_ID];
const BOOT_OUTCOME_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(BOOT_OUTCOME_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(BOOT_OUTCOME_REQUIREMENTS);
const EXIT_AFTER_TICK_PLUGIN_ID: PluginId = PluginId::new("nara.test.exit-after-tick");
const EXIT_AFTER_TICK_REQUIREMENTS: &[PluginId] = &[BOOT_OUTCOME_PLUGIN_ID];
const EXIT_AFTER_TICK_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(EXIT_AFTER_TICK_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(EXIT_AFTER_TICK_REQUIREMENTS);
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

#[derive(Debug, Default)]
struct ExitAfterTickPlugin;

impl Plugin for ExitAfterTickPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &EXIT_AFTER_TICK_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::FixedUpdate,
            request_exit_after_tick.in_set(FixedUpdateSet::Finalize),
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
        .map(|source| source.source_entity_id.as_str().to_owned())
        .collect();
    outcome.scene_entities.sort();
}

fn request_exit_after_tick(mut exit: ResMut<AppExitRequests>) {
    exit.request_exit();
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
    HeadlessRun::from_recipe(
        project.root_capability(),
        boot_recipe(),
        NonZeroU32::new(1).unwrap(),
        Vec::new(),
    )
}

fn boot_recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_plugin::<BootOutcomePlugin>()
        .expect("the boot recipe is statically valid")
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

#[test]
fn ordinary_recipe_headless_facade_owns_project_start_and_close() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let recipe = boot_recipe();
    let mut run = HeadlessRun::from_recipe(
        project.root_capability(),
        recipe,
        NonZeroU32::new(1).unwrap(),
        Vec::new(),
    );

    let report = execute_to_terminal(&mut run);

    assert_eq!(
        report.outcome(),
        &HeadlessRunOutcome::Completed(BootOutcome {
            tick: 1,
            scene_entities: vec!["enemy".to_owned(), "enemy-anchor".to_owned()],
        })
    );
}

#[test]
fn headless_run_materializes_a_stable_image_reference_after_source_rename() {
    use nara::{
        asset::AssetRef,
        reflect::{ComponentSchemaVersion, ComponentTypeId},
        scene::{PrefabDocument, SceneComponentRecord, SceneEntityRecord},
    };

    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let identity = player_image_meta().identity().stable_id();
    project.write_prefab_source(&PrefabDocument::from_ordered_roots([
        SceneEntityRecord::new(scene_id("enemy")).with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                sprite_value(&AssetRef::StableId(identity)),
            ),
        ),
    ]));
    let mut before_run = project_run(&project);
    let before = execute_to_terminal(&mut before_run);
    assert!(
        matches!(before.outcome(), HeadlessRunOutcome::Completed(_)),
        "{before:#?}"
    );
    drop(before_run);

    std::fs::rename(
        project.path().join("assets/textures/player.png"),
        project.path().join("assets/textures/renamed.png"),
    )
    .unwrap();
    std::fs::rename(
        project.path().join("assets/textures/player.png.meta"),
        project.path().join("assets/textures/renamed.png.meta"),
    )
    .unwrap();
    std::fs::write(
        project.path().join("assets/textures/renamed.png.meta"),
        image_meta(&identity.to_string(), "textures/renamed.png")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut after_run = project_run(&project);
    let after = execute_to_terminal(&mut after_run);

    assert_eq!(after.outcome(), before.outcome(), "{after:#?}");
    assert!(!after.diagnostics().has_errors());
    let closure = after.startup_content_closure().unwrap();
    assert!(
        closure
            .members()
            .iter()
            .any(|member| member.path().as_str() == "assets/textures/renamed.png")
    );
}

#[test]
fn product_action_boots_authorized_content_runs_one_tick_and_closes() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut run = project_run(&project);

    let first = execute_to_terminal(&mut run);
    assert_eq!(
        first.outcome(),
        &HeadlessRunOutcome::Completed(BootOutcome {
            tick: 1,
            scene_entities: vec!["enemy".to_owned(), "enemy-anchor".to_owned()],
        }),
        "{first:?}"
    );
    assert!(!first.diagnostics().has_errors());
    let first_source = first
        .startup_content_closure()
        .expect("successful project check publishes startup content evidence");
    assert!(
        first_source
            .members()
            .iter()
            .any(|member| member.path().as_str() == "nara.toml")
    );

    let terminal_repeat = run.execute_bounded();
    assert_eq!(terminal_repeat.outcome(), first.outcome());
    assert_eq!(terminal_repeat.diagnostics(), first.diagnostics());
    let repeated_source = terminal_repeat
        .startup_content_closure()
        .expect("terminal reports retain startup content evidence");
    assert_eq!(
        repeated_source.manifest_digest(),
        first_source.manifest_digest()
    );
    assert_eq!(
        repeated_source.content_digest(),
        first_source.content_digest()
    );
    assert_eq!(
        repeated_source.schema_fingerprint(),
        first_source.schema_fingerprint()
    );
    assert_eq!(repeated_source.members(), first_source.members());
}

#[test]
fn unreferenced_import_preserves_the_headless_startup_closure() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let mut baseline_run = project_run(&project);
    let baseline = execute_to_terminal(&mut baseline_run);
    let baseline_closure = baseline
        .startup_content_closure()
        .expect("the baseline Headless Run must publish its startup closure");
    let baseline_content = baseline_closure.content_digest();
    let baseline_plan = baseline_closure.closure_plan_digest();
    let baseline_members = baseline_closure.members().to_vec();

    project.write_asset(
        "textures/unreferenced.png",
        valid_png_bytes(),
        &image_meta(
            "92fca27e-d243-46bd-9cb4-6506f9f888de",
            "textures/unreferenced.png",
        ),
    );

    let mut successor_run = project_run(&project);
    let successor = execute_to_terminal(&mut successor_run);
    let successor_closure = successor
        .startup_content_closure()
        .expect("the successor Headless Run must publish its startup closure");

    assert_eq!(successor_closure.content_digest(), baseline_content);
    assert_eq!(successor_closure.closure_plan_digest(), baseline_plan);
    assert_eq!(successor_closure.members(), baseline_members);
    assert!(successor_closure.members().iter().all(|member| {
        !member
            .path()
            .as_str()
            .starts_with("assets/textures/unreferenced.png")
    }));
}

#[test]
fn project_content_failure_returns_without_a_runtime_outcome() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    project.write_scene_bytes(b"{");
    let mut run = project_run(&project);

    let report = run.execute_bounded();

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(report.diagnostics().has_errors());
    assert!(report.startup_content_closure().is_none());
}

#[test]
fn incomplete_cleanup_retries_the_same_owner_without_restarting_the_project() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let released = Arc::new(AtomicBool::new(false));
    let instances = Arc::new(AtomicUsize::new(0));
    let intent = HeadlessRunIntent::new(NonZeroU32::new(1).unwrap())
        .with_cleanup_timeout(Duration::ZERO)
        .with_recipe(boot_recipe());
    let mut run = headless_run_with_probe(
        project.root_capability(),
        intent,
        RuntimeProbe::new(stalled_close_plugin(&released, &instances)),
        Vec::new(),
    );

    let incomplete = run.execute_bounded();
    assert_eq!(incomplete.outcome(), &HeadlessRunOutcome::CleanupIncomplete);
    let incomplete_source = incomplete
        .startup_content_closure()
        .expect("cleanup retains the already-admitted startup closure");
    assert_eq!(instances.load(Ordering::SeqCst), 1);

    released.store(true, Ordering::SeqCst);
    let completed = execute_to_terminal(&mut run);
    assert_eq!(
        completed.outcome(),
        &HeadlessRunOutcome::Completed(BootOutcome {
            tick: 1,
            scene_entities: vec!["enemy".to_owned(), "enemy-anchor".to_owned()],
        })
    );
    assert_eq!(
        completed
            .startup_content_closure()
            .unwrap()
            .content_digest(),
        incomplete_source.content_digest()
    );
    assert_eq!(instances.load(Ordering::SeqCst), 1);
}

#[test]
fn product_action_stops_on_the_first_terminal_snapshot() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let intent = HeadlessRunIntent::new(NonZeroU32::new(4).unwrap())
        .stop_when(|outcome: &BootOutcome| outcome.tick >= 2)
        .with_recipe(boot_recipe());
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());

    let report = execute_to_terminal(&mut run);

    let HeadlessRunOutcome::Completed(outcome) = report.outcome() else {
        panic!("terminal project action failed: {report:#?}");
    };
    assert_eq!(outcome.tick, 2);
    assert!(!report.diagnostics().has_errors());
}

#[test]
fn product_action_propagates_an_app_exit_from_the_completed_fixed_tick() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let recipe = boot_recipe()
        .add_plugin::<ExitAfterTickPlugin>()
        .expect("the exit recipe is statically valid");
    let intent = HeadlessRunIntent::new(NonZeroU32::new(4).unwrap())
        .stop_when(|_: &BootOutcome| false)
        .with_recipe(recipe);
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());

    let report = execute_to_terminal(&mut run);

    assert_eq!(
        report.outcome(),
        &HeadlessRunOutcome::Completed(BootOutcome {
            tick: 1,
            scene_entities: vec!["enemy".to_owned(), "enemy-anchor".to_owned()],
        }),
        "{report:?}"
    );
    assert!(!report.diagnostics().has_errors());
}

#[test]
fn product_action_reports_a_tick_limit_before_publishing_success() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
    let intent = HeadlessRunIntent::new(NonZeroU32::new(2).unwrap())
        .stop_when(|_: &BootOutcome| false)
        .with_recipe(boot_recipe());
    let mut run = HeadlessRun::new(project.root_capability(), intent, Vec::new());

    let report = execute_to_terminal(&mut run);

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(report.startup_content_closure().is_some());
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.run.tick-limit"
            && diagnostic.summary().as_str() == "Headless project run reached its fixed-tick limit"
    }));
}

#[test]
fn source_label_cannot_downgrade_a_trusted_product_command_rejection() {
    let project = TestProject::with_prefab_startup();
    project.select_local_headless_profile();
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
    let mut run = HeadlessRun::from_recipe(
        project.root_capability(),
        boot_recipe(),
        NonZeroU32::new(1).unwrap(),
        commands,
    );

    let report = execute_to_terminal(&mut run);

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(report.startup_content_closure().is_some());
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
