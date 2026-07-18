#[path = "support/manual_raw_app_boot.rs"]
mod manual_raw_app_boot;
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    fs,
    num::NonZeroU32,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use manual_raw_app_boot::{
    ManualRawAppBootError, ManualRawAppFault, run_manual_raw_app_boot, run_manual_raw_app_fault,
    run_manual_raw_app_incomplete_retirement, run_manual_raw_app_pre_owner_failure,
};
use nara::{
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
        PluginError, PluginId,
    },
    ecs::{
        Resource,
        lifecycle::{Add, HookContext},
        observer::On,
        world::DeferredWorld,
    },
    diagnostic::DiagnosticValueRef,
    project_host::{HeadlessRun, HeadlessRunOutcome},
    sprite::Sprite,
    tasks::{
        TASK_PLUGIN_ID, TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskSpawnRequest,
    },
};
use nara_reference_game::{
    REFERENCE_FIRST_TICK_COMMAND_SOURCE, REFERENCE_FIRST_TICK_COMMAND_TYPE,
    REFERENCE_GAME_PLUGIN_ID, ReferenceGamePlugin, ReferenceProjectSnapshot,
    project_first_tick_command, project_headless_intent, project_headless_run,
};
use project_content_fixture::{
    ProjectContentFixtureError, project_root_capability, try_project_root_capability_at,
};

const LATE_HOOK_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.late-hook");
const LATE_HOOK_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(LATE_HOOK_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[REFERENCE_GAME_PLUGIN_ID]);
const LATE_EVENT_OBSERVER_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.late-event-observer");
const LATE_EVENT_OBSERVER_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(LATE_EVENT_OBSERVER_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[REFERENCE_GAME_PLUGIN_ID]);
const LATE_COMPONENT_OBSERVER_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.late-component-observer");
const LATE_COMPONENT_OBSERVER_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(LATE_COMPONENT_OBSERVER_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[REFERENCE_GAME_PLUGIN_ID]);
const BLOCKING_TASK_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.blocking-task");
const BLOCKING_TASK_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("reference-game.test.blocking-task", 1);
const BLOCKING_TASK_REQUIREMENTS: &[PluginId] = &[REFERENCE_GAME_PLUGIN_ID, TASK_PLUGIN_ID];
const BLOCKING_TASK_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(BLOCKING_TASK_PLUGIN_ID, PluginCategory::Service)
        .requires_plugins(BLOCKING_TASK_REQUIREMENTS);

static LATE_HOOK_CALLS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Default)]
struct LateHookPlugin;

impl Plugin for LateHookPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &LATE_HOOK_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let world = app.world_mut()?;
        world.register_component::<Sprite>();
        world.commands().queue(|world: &mut nara::prelude::World| {
            world
                .register_component_hooks::<Sprite>()
                .on_add(late_sprite_hook);
        });
        Ok(())
    }
}

fn late_sprite_hook(_world: DeferredWorld<'_>, _context: HookContext) {
    LATE_HOOK_CALLS.fetch_add(1, Ordering::SeqCst);
}

#[derive(Debug, Default)]
struct LateEventObserverPlugin;

impl Plugin for LateEventObserverPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &LATE_EVENT_OBSERVER_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.commands().queue(|world: &mut nara::prelude::World| {
            world.add_observer(|_: On<Add>| {});
        });
        Ok(())
    }
}

#[derive(Debug, Default)]
struct LateComponentObserverPlugin;

impl Plugin for LateComponentObserverPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &LATE_COMPONENT_OBSERVER_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.commands().queue(|world: &mut nara::prelude::World| {
            world.add_observer(|_: On<Add, Sprite>| {});
        });
        Ok(())
    }
}

#[derive(Debug)]
struct BlockingTaskPlugin {
    control: Arc<BlockingTaskControl>,
}

impl Plugin for BlockingTaskPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &BLOCKING_TASK_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        self.control.builds.fetch_add(1, Ordering::SeqCst);
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let (release_sender, release_receiver) = mpsc::channel();
        *self
            .control
            .release
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(release_sender);
        let handle = app
            .world()
            .get_resource::<TaskPools>()
            .ok_or_else(blocking_task_setup_failed)?
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(1, TaskDomainKey::new(26)),
                {
                    let control = Arc::clone(&self.control);
                    move |_| {
                    let _ = started_sender.send(());
                    let _ = release_receiver.recv();
                        control.finished.fetch_add(1, Ordering::SeqCst);
                    }
                },
            )
            .into_handle()
            .map_err(|_| blocking_task_setup_failed())?;
        started_receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| blocking_task_setup_failed())?;
        app.insert_resource(BlockingTaskHandle(handle))?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct BlockingTaskControl {
    builds: AtomicUsize,
    finished: AtomicUsize,
    release: Mutex<Option<mpsc::Sender<()>>>,
}

impl BlockingTaskControl {
    fn release(&self) {
        let sender = self
            .release
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .expect("the blocking task retains one release sender");
        sender.send(()).unwrap();
    }
}

#[derive(Resource)]
struct BlockingTaskHandle(#[allow(dead_code)] TaskHandle<()>);

fn blocking_task_setup_failed() -> PluginError {
    PluginError::SetupFailed {
        plugin: BLOCKING_TASK_PLUGIN_ID,
        message: "the test blocking task could not start".to_owned(),
    }
}

#[test]
fn product_action_matches_the_manual_first_tick_without_rust_seed_content() {
    let manual = run_manual_raw_app_boot().unwrap();
    let canonical_command = project_first_tick_command();
    assert_eq!(
        canonical_command.command().command_type().as_str(),
        include_str!("data/manual-first-tick.command").trim()
    );
    assert_eq!(
        canonical_command.command().command_type().as_str(),
        REFERENCE_FIRST_TICK_COMMAND_TYPE
    );
    assert_eq!(
        canonical_command.source().id().as_str(),
        REFERENCE_FIRST_TICK_COMMAND_SOURCE
    );
    assert_eq!(canonical_command.tick().get(), 1);
    assert_eq!(canonical_command.source_sequence().get(), 1);
    let expected_command_key = canonical_command.key();
    let expected_command_type = canonical_command.command().command_type().clone();
    let mut product = project_headless_run(
        project_root_capability(),
        NonZeroU32::new(1).unwrap(),
    );

    let deadline = Instant::now() + Duration::from_secs(5);
    let report = loop {
        let report = product.execute_bounded();
        if report.outcome() != &HeadlessRunOutcome::CleanupIncomplete {
            break report;
        }
        assert!(Instant::now() < deadline, "product cleanup did not finish");
        std::thread::yield_now();
    };
    let HeadlessRunOutcome::Completed(snapshot) = report.outcome() else {
        panic!("product run failed: {report:?}");
    };

    assert_eq!(
        snapshot,
        &ReferenceProjectSnapshot {
            tick: manual.first_tick.tick,
            player_position: manual.first_tick.player_position,
            player_hit_points: manual.first_tick.player_hit_points,
            enemy_position: manual.first_tick.enemy_position,
            enemy_hit_points: manual.first_tick.enemy_hit_points,
            weapon_remaining_ticks: manual.first_tick.weapon_remaining_ticks,
            commands_seen: manual.command_stats.admitted,
            first_command_key: Some(expected_command_key),
            first_command_type: Some(expected_command_type),
            runtime_only_entities: 0,
            unbound_gameplay_components: 0,
        }
    );
    assert_eq!(manual.command_stats.accepted, 1);
    assert_eq!(manual.command_stats.acknowledged, 1);
    assert!(manual.command_queue_idle);
    assert_eq!(
        selected_plugin_plan_fingerprint(report.diagnostics()),
        manual.plugin_plan_fingerprint
    );
    assert!(!report.diagnostics().has_errors());
}

#[test]
fn missing_manifest_fails_before_product_runtime_ownership() {
    let manual = run_manual_raw_app_pre_owner_failure().unwrap_err();
    assert_eq!(
        manual.primary,
        ManualRawAppBootError::ProjectContent(ProjectContentFixtureError::OpenManifest)
    );
    assert!(manual.retirement.is_none());

    let root = EmptyProjectRoot::new();
    let authority = try_project_root_capability_at(&root.path).unwrap();
    let mut product = project_headless_run(authority, NonZeroU32::new(1).unwrap());
    let report = product.execute_bounded();

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.manifest.host-io"
    }));
}

#[test]
fn late_persistent_hook_matches_the_manual_rejection_before_publication() {
    let manual = run_manual_raw_app_fault(ManualRawAppFault::LatePersistentHook).unwrap();
    LATE_HOOK_CALLS.store(0, Ordering::SeqCst);
    let intent = project_headless_intent(NonZeroU32::new(1).unwrap())
        .insert_after::<ReferenceGamePlugin>(PluginDefinition::for_default::<LateHookPlugin>());
    let mut product = HeadlessRun::new(project_root_capability(), intent, []);

    let deadline = Instant::now() + Duration::from_secs(5);
    let report = loop {
        let report = product.execute_bounded();
        if report.outcome() != &HeadlessRunOutcome::CleanupIncomplete {
            break report;
        }
        assert!(Instant::now() < deadline, "product cleanup did not finish");
        std::thread::yield_now();
    };

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed, "{report:?}");
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == manual.diagnostic_code
    }));
    assert!(!manual.scene_published);
    assert_eq!(manual.hook_calls, 0);
    assert_eq!(LATE_HOOK_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn deferred_global_observers_reject_project_scene_before_publication() {
    assert_global_observer_rejected(PluginDefinition::for_default::<
        LateEventObserverPlugin,
    >());
    assert_global_observer_rejected(PluginDefinition::for_default::<
        LateComponentObserverPlugin,
    >());
}

fn assert_global_observer_rejected(definition: PluginDefinition) {
    let intent = project_headless_intent(NonZeroU32::new(1).unwrap())
        .insert_after::<ReferenceGamePlugin>(definition);
    let mut product = HeadlessRun::new(project_root_capability(), intent, []);
    let deadline = Instant::now() + Duration::from_secs(5);
    let report = loop {
        let report = product.execute_bounded();
        if report.outcome() != &HeadlessRunOutcome::CleanupIncomplete {
            break report;
        }
        assert!(Instant::now() < deadline, "observer cleanup did not finish");
        std::thread::yield_now();
    };

    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed, "{report:?}");
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.persistent-apply-ineligible"
    }));
}

#[test]
fn incomplete_retirement_stays_with_the_product_action_until_retry() {
    let manual = run_manual_raw_app_incomplete_retirement().unwrap();
    let control = Arc::new(BlockingTaskControl::default());
    let definition_control = Arc::clone(&control);
    let blocking_task = PluginDefinition::infallible::<BlockingTaskPlugin, _>(
        BLOCKING_TASK_DEFINITION_ID,
        b"reference-blocking-task-v1",
        move || BlockingTaskPlugin {
            control: Arc::clone(&definition_control),
        },
    );
    let intent = project_headless_intent(NonZeroU32::new(1).unwrap())
        .with_cleanup_timeout(Duration::ZERO)
        .insert_after::<ReferenceGamePlugin>(blocking_task);
    let mut product = HeadlessRun::new(
        project_root_capability(),
        intent,
        [project_first_tick_command()],
    );

    let first_drive_started = Instant::now();
    let incomplete = product.execute_bounded();
    assert!(first_drive_started.elapsed() < Duration::from_secs(1));
    assert_eq!(
        incomplete.outcome(),
        &HeadlessRunOutcome::CleanupIncomplete
    );
    assert!(incomplete.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.run.cleanup-incomplete"
    }));
    assert!(incomplete.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.run.cleanup-deadline-exceeded"
    }));
    assert_eq!(control.builds.load(Ordering::SeqCst), 1);
    assert_eq!(control.finished.load(Ordering::SeqCst), 0);
    assert!(manual.scene_published);
    assert!(!manual.runtime_published);
    assert_eq!(manual.diagnostic_class, ManualRawAppBootError::TaskShutdown);

    control.release();
    let cleanup_deadline = Instant::now() + Duration::from_secs(10);
    let completed = loop {
        let report = product.execute_bounded();
        if report.outcome() != &HeadlessRunOutcome::CleanupIncomplete {
            break report;
        }
        assert!(
            Instant::now() < cleanup_deadline,
            "product retained the released task owner indefinitely"
        );
        std::thread::yield_now();
    };
    let HeadlessRunOutcome::Completed(snapshot) = completed.outcome() else {
        panic!("product cleanup retry failed: {completed:?}");
    };
    assert_eq!(snapshot.tick, manual.first_tick.tick);
    assert_eq!(snapshot.player_position, manual.first_tick.player_position);
    assert_eq!(snapshot.enemy_hit_points, manual.first_tick.enemy_hit_points);
    assert_eq!(snapshot.commands_seen, 1);
    assert_eq!(control.builds.load(Ordering::SeqCst), 1);
    assert_eq!(control.finished.load(Ordering::SeqCst), 1);
}

fn selected_plugin_plan_fingerprint(diagnostics: &nara::diagnostic::DiagnosticReport) -> &str {
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "project.run.plan-selected")
        .expect("the product report records its selected plan");
    let field = diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == "plugin-plan-fingerprint")
        .expect("the selected-plan diagnostic records its fingerprint");
    let DiagnosticValueRef::Identifier(fingerprint) = field.value() else {
        panic!("the selected-plan fingerprint is not a public identifier");
    };
    fingerprint
}

struct EmptyProjectRoot {
    path: PathBuf,
}

impl EmptyProjectRoot {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nara_reference_empty_project_{}_{}",
            std::process::id(),
            stamp
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for EmptyProjectRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap();
    }
}
