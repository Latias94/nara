#![allow(dead_code)]

use std::{error::Error, fmt, path::Path, sync::mpsc, time::Duration};

use nara::{
    app::AppFrameOutcome,
    diagnostic::DiagnosticValueRef,
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    fs::ContentDigest,
    gameplay::{
        GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandQueue,
        GameplayCommandQueueStats, GameplayCommandSourceSequence, GameplayCommandSubmission,
        GameplayCommandTick, GameplayCommandTypeId,
    },
    identity::EntityLookup,
    image::ImageImportLimits,
    prelude::{App, FixedTime, Resource, Vec2, World},
    reflect::{COMPONENT_REGISTRY_PLUGIN_ID, ComponentRegistry},
    scene::{SceneSpawnReport, SpawnedSceneInstance, spawn_scene},
    sprite::Sprite,
    tasks::{
        TaskDomainKey, TaskPoolKind, TaskPools, TaskShutdownPhase, TaskShutdownReport,
        TaskSpawnRequest,
    },
};
use nara_reference_game::{Enemy, Player, Weapon};

use crate::project_content_fixture::{
    LoadedProjectContent, ProjectContentFixtureError, reference_runtime_plugins, scene_id,
    try_load_project_content, try_load_project_content_from_path,
};

const COMMAND_INPUT: &[u8] = include_bytes!("../data/manual-first-tick.command");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualRawAppBootError {
    ProjectContent(ProjectContentFixtureError),
    TaskPools,
    PluginCommit,
    PluginPlanDrift,
    Startup,
    UnexpectedStartupTick,
    SceneSpawn,
    SceneEntity,
    SceneComponent,
    CommandInput,
    CommandSubmit,
    FixedTick,
    UnexpectedFixedSteps,
    TaskSpawn,
    TaskStart,
    TaskShutdown,
    ExpectedIncompleteRetirement,
    TaskCleanup,
    PluginShutdown,
    MissingDiagnostic,
}

impl fmt::Display for ManualRawAppBootError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ProjectContent(error) => {
                return write!(formatter, "manual raw-App project content failed: {error}");
            }
            Self::TaskPools => "manual raw-App task pools could not be created",
            Self::PluginCommit => "manual raw-App plugin commit failed",
            Self::PluginPlanDrift => "manual raw-App plugin plan drifted from the resolved plan",
            Self::Startup => "manual raw-App startup failed",
            Self::UnexpectedStartupTick => "manual raw-App startup advanced a fixed tick",
            Self::SceneSpawn => "manual raw-App scene materialization failed",
            Self::SceneEntity => "manual raw-App scene entity could not be resolved",
            Self::SceneComponent => "manual raw-App scene component is missing",
            Self::CommandInput => "manual raw-App command fixture is invalid",
            Self::CommandSubmit => "manual raw-App command submission failed",
            Self::FixedTick => "manual raw-App fixed tick failed",
            Self::UnexpectedFixedSteps => "manual raw-App did not execute exactly one fixed tick",
            Self::TaskSpawn => "manual raw-App required task could not be spawned",
            Self::TaskStart => "manual raw-App required task did not start",
            Self::TaskShutdown => "manual raw-App task shutdown failed",
            Self::ExpectedIncompleteRetirement => {
                "manual raw-App task retirement unexpectedly completed"
            }
            Self::TaskCleanup => "manual raw-App task workers did not all join on retry",
            Self::PluginShutdown => "manual raw-App plugin shutdown failed",
            Self::MissingDiagnostic => "manual raw-App rejection had no diagnostic code",
        };
        formatter.write_str(message)
    }
}

impl Error for ManualRawAppBootError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualRawAppRetirementReport {
    pub task_shutdown: Result<TaskShutdownReport, ManualRawAppBootError>,
    pub plugin_shutdown: Result<(), ManualRawAppBootError>,
}

impl ManualRawAppRetirementReport {
    fn failure(&self) -> Option<ManualRawAppBootError> {
        match &self.task_shutdown {
            Err(error) => return Some(*error),
            Ok(report) if report.timed_out() => return Some(ManualRawAppBootError::TaskShutdown),
            Ok(_) => {}
        }
        self.plugin_shutdown.as_ref().err().copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualRawAppFailure {
    pub primary: ManualRawAppBootError,
    pub retirement: Option<ManualRawAppRetirementReport>,
}

impl ManualRawAppFailure {
    fn before_owner(primary: ManualRawAppBootError) -> Self {
        Self {
            primary,
            retirement: None,
        }
    }

    fn with_retirement(
        primary: ManualRawAppBootError,
        retirement: ManualRawAppRetirementReport,
    ) -> Self {
        Self {
            primary,
            retirement: Some(retirement),
        }
    }
}

impl fmt::Display for ManualRawAppFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.primary, formatter)
    }
}

impl Error for ManualRawAppFailure {}

#[derive(Debug, Clone, PartialEq)]
pub struct ManualFirstTickSnapshot {
    pub tick: u64,
    pub player_position: Vec2,
    pub player_hit_points: i64,
    pub enemy_position: Vec2,
    pub enemy_hit_points: i64,
    pub weapon_remaining_ticks: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManualRawAppBootReport {
    pub first_tick: ManualFirstTickSnapshot,
    pub command_stats: GameplayCommandQueueStats,
    pub command_queue_idle: bool,
    pub plugin_plan_fingerprint: String,
    pub schema_fingerprint: String,
    pub content_revision: String,
    pub content_digest: String,
    pub command_digest: String,
    pub task_shutdown: TaskShutdownReport,
    pub expected_task_workers: usize,
    pub joined_task_workers: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualRawAppFault {
    LatePersistentHook,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualRawAppFaultReport {
    pub diagnostic_code: String,
    pub persistent_apply_reason: String,
    pub lifecycle_event: String,
    pub hook_calls: u64,
    pub entities_before: usize,
    pub entities_after: usize,
    pub scene_published: bool,
    pub task_shutdown: TaskShutdownReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualRetirementCustody {
    AppWorld,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManualRawAppIncompleteRetirementReport {
    pub first_tick: ManualFirstTickSnapshot,
    pub diagnostic_class: ManualRawAppBootError,
    pub scene_published: bool,
    pub runtime_published: bool,
    pub incomplete_task_shutdown: TaskShutdownReport,
    pub completed_task_shutdown: TaskShutdownReport,
    pub incomplete_phase: TaskShutdownPhase,
    pub custody: ManualRetirementCustody,
    pub plugin_shutdown_complete: bool,
    pub expected_task_workers: usize,
    pub joined_task_workers: usize,
}

pub fn run_manual_raw_app_boot() -> Result<ManualRawAppBootReport, ManualRawAppFailure> {
    let (mut app, loaded) = prepare_manual_raw_app()?;
    let plugin_plan_fingerprint = app.configuration_fingerprint();
    let operation = execute_manual_first_tick(&mut app, &loaded);
    let retirement = shutdown_manual_raw_app(&mut app);
    drop(app);
    let (first_tick, command_stats, command_queue_idle) = operation
        .map_err(|primary| ManualRawAppFailure::with_retirement(primary, retirement.clone()))?;
    if let Some(primary) = retirement.failure() {
        return Err(ManualRawAppFailure::with_retirement(primary, retirement));
    }
    let task_shutdown = match retirement.task_shutdown {
        Ok(report) => report,
        Err(_) => unreachable!("the retirement failure was handled above"),
    };
    let joined_task_workers = joined_task_workers(&task_shutdown);
    let expected_task_workers = TaskPoolKind::ALL
        .into_iter()
        .map(|kind| {
            loaded
                .plan
                .settings()
                .tasks
                .pool_config
                .kind(kind)
                .workers()
                .get()
        })
        .sum();

    Ok(ManualRawAppBootReport {
        first_tick,
        command_stats,
        command_queue_idle,
        plugin_plan_fingerprint: digest_hex(plugin_plan_fingerprint.as_bytes()),
        schema_fingerprint: loaded.snapshot.schema_fingerprint().to_hex(),
        content_revision: loaded.snapshot.revision().to_hex(),
        content_digest: digest_hex(*loaded.snapshot.content_digest().as_bytes()),
        command_digest: digest_hex(*ContentDigest::of_bytes(COMMAND_INPUT).as_bytes()),
        task_shutdown,
        expected_task_workers,
        joined_task_workers,
    })
}

pub fn run_manual_raw_app_pre_owner_failure() -> Result<(), ManualRawAppFailure> {
    let invalid_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/missing-project-manifest");
    try_load_project_content_from_path(&invalid_root)
        .map(drop)
        .map_err(|error| {
            ManualRawAppFailure::before_owner(ManualRawAppBootError::ProjectContent(error))
        })
}

pub fn run_manual_raw_app_incomplete_retirement()
-> Result<ManualRawAppIncompleteRetirementReport, ManualRawAppFailure> {
    let (mut app, loaded) = prepare_manual_raw_app()?;
    let first_tick = match execute_manual_first_tick(&mut app, &loaded) {
        Ok((first_tick, _, _)) => first_tick,
        Err(primary) => {
            let retirement = shutdown_manual_raw_app(&mut app);
            drop(app);
            return Err(ManualRawAppFailure::with_retirement(primary, retirement));
        }
    };

    let (started_sender, started_receiver) = mpsc::sync_channel(1);
    let (release_sender, release_receiver) = mpsc::channel::<()>();
    let Some(pools) = app.world().get_resource::<TaskPools>() else {
        let retirement = shutdown_manual_raw_app(&mut app);
        drop(app);
        return Err(ManualRawAppFailure::with_retirement(
            ManualRawAppBootError::TaskSpawn,
            retirement,
        ));
    };
    let spawn = pools.spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(first_tick.tick, TaskDomainKey::new(26)),
        move |_| {
            let _ = started_sender.send(());
            let _ = release_receiver.recv();
        },
    );
    let task = match spawn.into_handle() {
        Ok(task) => task,
        Err(_) => {
            let retirement = shutdown_manual_raw_app(&mut app);
            drop(app);
            return Err(ManualRawAppFailure::with_retirement(
                ManualRawAppBootError::TaskSpawn,
                retirement,
            ));
        }
    };
    if started_receiver
        .recv_timeout(Duration::from_secs(10))
        .is_err()
    {
        drop(release_sender);
        let retirement = shutdown_manual_raw_app(&mut app);
        drop(app);
        drop(task);
        return Err(ManualRawAppFailure::with_retirement(
            ManualRawAppBootError::TaskStart,
            retirement,
        ));
    }

    let incomplete_task_shutdown = match shutdown_manual_tasks(&app) {
        Ok(report) => report,
        Err(_) => {
            drop(release_sender);
            let retirement = shutdown_manual_raw_app(&mut app);
            drop(app);
            drop(task);
            return Err(ManualRawAppFailure::with_retirement(
                ManualRawAppBootError::TaskShutdown,
                retirement,
            ));
        }
    };
    if !incomplete_task_shutdown.timed_out()
        || !incomplete_task_shutdown
            .for_kind(TaskPoolKind::Io)
            .timed_out()
    {
        drop(release_sender);
        let retirement = shutdown_manual_raw_app(&mut app);
        drop(app);
        drop(task);
        return Err(ManualRawAppFailure::with_retirement(
            ManualRawAppBootError::ExpectedIncompleteRetirement,
            retirement,
        ));
    }
    let incomplete_phase = incomplete_task_phase(&incomplete_task_shutdown, TaskPoolKind::Io);

    drop(release_sender);
    let completed_task_shutdown = shutdown_manual_tasks(&app).map_err(|_| {
        ManualRawAppFailure::with_retirement(
            ManualRawAppBootError::TaskShutdown,
            ManualRawAppRetirementReport {
                task_shutdown: Err(ManualRawAppBootError::TaskShutdown),
                plugin_shutdown: app
                    .shutdown_plugins()
                    .map_err(|_| ManualRawAppBootError::PluginShutdown),
            },
        )
    })?;
    let expected_task_workers = TaskPoolKind::ALL
        .into_iter()
        .map(|kind| {
            loaded
                .plan
                .settings()
                .tasks
                .pool_config
                .kind(kind)
                .workers()
                .get()
        })
        .sum();
    let joined_task_workers = joined_task_workers(&completed_task_shutdown);
    let plugin_shutdown = app
        .shutdown_plugins()
        .map_err(|_| ManualRawAppBootError::PluginShutdown);
    let retirement = ManualRawAppRetirementReport {
        task_shutdown: Ok(completed_task_shutdown.clone()),
        plugin_shutdown,
    };
    drop(app);
    drop(task);
    if retirement.plugin_shutdown.is_err() {
        return Err(ManualRawAppFailure::with_retirement(
            ManualRawAppBootError::PluginShutdown,
            retirement,
        ));
    }
    if joined_task_workers != expected_task_workers {
        return Err(ManualRawAppFailure::with_retirement(
            ManualRawAppBootError::TaskCleanup,
            retirement,
        ));
    }

    Ok(ManualRawAppIncompleteRetirementReport {
        first_tick,
        diagnostic_class: ManualRawAppBootError::TaskShutdown,
        scene_published: true,
        runtime_published: false,
        incomplete_task_shutdown,
        completed_task_shutdown,
        incomplete_phase,
        custody: ManualRetirementCustody::AppWorld,
        plugin_shutdown_complete: true,
        expected_task_workers,
        joined_task_workers,
    })
}

pub fn run_manual_raw_app_fault(
    fault: ManualRawAppFault,
) -> Result<ManualRawAppFaultReport, ManualRawAppFailure> {
    let (mut app, loaded) = prepare_manual_raw_app()?;
    let operation = (|| {
        match fault {
            ManualRawAppFault::LatePersistentHook => install_late_sprite_hook(
                app.world_mut()
                    .map_err(|_| ManualRawAppBootError::SceneSpawn)?,
            ),
        }
        let entities_before = app.world().iter_entities().count();
        let report = spawn_snapshot_scene(&mut app, &loaded)?;
        let entities_after = app.world().iter_entities().count();
        let diagnostic = report
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code().as_str() == "scene.persistent-apply-ineligible")
            .ok_or(ManualRawAppBootError::MissingDiagnostic)?;
        let diagnostic_code = diagnostic.code().as_str().to_owned();
        let persistent_apply_reason = diagnostic_identifier(diagnostic, "persistent-apply-reason")
            .ok_or(ManualRawAppBootError::MissingDiagnostic)?;
        let lifecycle_event = diagnostic_identifier(diagnostic, "lifecycle-event")
            .ok_or(ManualRawAppBootError::MissingDiagnostic)?;
        let hook_calls = app
            .world()
            .get_resource::<ManualPersistentApplyCanary>()
            .ok_or(ManualRawAppBootError::SceneComponent)?
            .0;
        Ok((
            diagnostic_code,
            persistent_apply_reason,
            lifecycle_event,
            hook_calls,
            entities_before,
            entities_after,
            report.instance.is_some(),
        ))
    })();
    let retirement = shutdown_manual_raw_app(&mut app);
    drop(app);
    let (
        diagnostic_code,
        persistent_apply_reason,
        lifecycle_event,
        hook_calls,
        entities_before,
        entities_after,
        scene_published,
    ) = operation
        .map_err(|primary| ManualRawAppFailure::with_retirement(primary, retirement.clone()))?;
    if let Some(primary) = retirement.failure() {
        return Err(ManualRawAppFailure::with_retirement(primary, retirement));
    }
    let task_shutdown = match retirement.task_shutdown {
        Ok(report) => report,
        Err(_) => unreachable!("the retirement failure was handled above"),
    };

    Ok(ManualRawAppFaultReport {
        diagnostic_code,
        persistent_apply_reason,
        lifecycle_event,
        hook_calls,
        entities_before,
        entities_after,
        scene_published,
        task_shutdown,
    })
}

fn prepare_manual_raw_app() -> Result<(App, LoadedProjectContent), ManualRawAppFailure> {
    let loaded = try_load_project_content().map_err(|error| {
        ManualRawAppFailure::before_owner(ManualRawAppBootError::ProjectContent(error))
    })?;
    let mut app = App::new();
    let pools = TaskPools::try_new(loaded.plan.settings().tasks.pool_config)
        .map_err(|_| ManualRawAppFailure::before_owner(ManualRawAppBootError::TaskPools))?;
    app.insert_resource(pools)
        .map_err(|_| ManualRawAppFailure::before_owner(ManualRawAppBootError::PluginCommit))?;
    let prepared = (|| {
        app.add_plugins(
            reference_runtime_plugins(&loaded.candidate, ImageImportLimits::default(), false)
                .into_app_plugins(),
        )
        .map_err(|_| ManualRawAppBootError::PluginCommit)?;
        let installed = app.installed_plugin_entries().collect::<Vec<_>>();
        let planned = loaded.plan.plugin_plan().entries();
        let same_recipe = installed.len() == planned.len()
            && installed.iter().zip(planned).all(|(installed, planned)| {
                installed == &planned
                    || (installed.plugin_id() == COMPONENT_REGISTRY_PLUGIN_ID
                        && planned.plugin_id() == COMPONENT_REGISTRY_PLUGIN_ID
                        && installed.slot() == planned.slot()
                        && installed.group_provenance() == planned.group_provenance())
            });
        if !same_recipe {
            return Err(ManualRawAppBootError::PluginPlanDrift);
        }
        app.insert_resource(loaded.plan.settings().runtime.runtime_time_settings())
            .map_err(|_| ManualRawAppBootError::PluginCommit)?;
        app.insert_resource(loaded.plan.settings().runtime.fixed_time())
            .map_err(|_| ManualRawAppBootError::PluginCommit)?;
        let startup = app
            .run_once(Duration::ZERO)
            .map_err(|_| ManualRawAppBootError::Startup)?;
        if startup.status.fixed_steps != 0 {
            return Err(ManualRawAppBootError::UnexpectedStartupTick);
        }
        let raw_snapshot = app
            .world()
            .get_resource::<ComponentRegistry>()
            .ok_or(ManualRawAppBootError::PluginPlanDrift)?
            .snapshot()
            .map_err(|_| ManualRawAppBootError::PluginPlanDrift)?;
        let raw_schema_fingerprint = raw_snapshot
            .schema_composition_fingerprint()
            .map_err(|_| ManualRawAppBootError::PluginPlanDrift)?;
        let raw_executable_fingerprint = raw_snapshot
            .executable_registry_fingerprint()
            .map_err(|_| ManualRawAppBootError::PluginPlanDrift)?;
        if raw_schema_fingerprint != loaded.plan.schema_validation().composition_fingerprint()
            || raw_executable_fingerprint
                != loaded.plan.schema_validation().executable_fingerprint()
            || raw_snapshot.contribution_receipts().collect::<Vec<_>>()
                != loaded
                    .plan
                    .schema_validation()
                    .contribution_receipts()
                    .collect::<Vec<_>>()
        {
            return Err(ManualRawAppBootError::PluginPlanDrift);
        }
        Ok(())
    })();
    if let Err(primary) = prepared {
        let retirement = shutdown_manual_raw_app(&mut app);
        drop(app);
        return Err(ManualRawAppFailure::with_retirement(primary, retirement));
    }
    Ok((app, loaded))
}

fn execute_manual_first_tick(
    app: &mut App,
    loaded: &LoadedProjectContent,
) -> Result<(ManualFirstTickSnapshot, GameplayCommandQueueStats, bool), ManualRawAppBootError> {
    let report = spawn_snapshot_scene(app, loaded)?;
    if report.diagnostics.has_errors() {
        return Err(ManualRawAppBootError::SceneSpawn);
    }
    let instance = report.instance.ok_or(ManualRawAppBootError::SceneSpawn)?;

    submit_frozen_command(
        app.world_mut()
            .map_err(|_| ManualRawAppBootError::CommandSubmit)?,
    )?;
    let fixed_timestep = loaded.plan.settings().runtime.fixed_time().timestep();
    let frame = app
        .run_once(fixed_timestep)
        .map_err(|_| ManualRawAppBootError::FixedTick)?;
    require_one_fixed_step(&frame)?;

    let first_tick = capture_first_tick(app.world(), &instance)?;
    let queue = app
        .world()
        .get_resource::<GameplayCommandQueue>()
        .ok_or(ManualRawAppBootError::SceneComponent)?;
    Ok((first_tick, queue.stats(), queue.is_idle()))
}

fn spawn_snapshot_scene(
    app: &mut App,
    loaded: &LoadedProjectContent,
) -> Result<SceneSpawnReport, ManualRawAppBootError> {
    let registry = app
        .world()
        .get_resource::<nara::reflect::ComponentRegistry>()
        .ok_or(ManualRawAppBootError::SceneSpawn)?
        .snapshot()
        .map(nara::reflect::ComponentRegistry::from_snapshot)
        .map_err(|_| ManualRawAppBootError::SceneSpawn)?;
    Ok(spawn_scene(
        app.world_mut()
            .map_err(|_| ManualRawAppBootError::SceneSpawn)?,
        &registry,
        loaded.snapshot.expanded_startup_scene(),
    ))
}

fn submit_frozen_command(world: &mut World) -> Result<(), ManualRawAppBootError> {
    let command = std::str::from_utf8(COMMAND_INPUT)
        .ok()
        .and_then(|value| value.strip_suffix('\n'))
        .filter(|value| !value.is_empty())
        .ok_or(ManualRawAppBootError::CommandInput)?;
    let submission = GameplayCommandSubmission::new(
        GameplayCommandTick::new(1).ok_or(ManualRawAppBootError::CommandInput)?,
        GameplayCommandIngressSource::test("u26-manual")
            .map_err(|_| ManualRawAppBootError::CommandInput)?,
        GameplayCommandSourceSequence::new(1).ok_or(ManualRawAppBootError::CommandInput)?,
        GameplayCommandDraft::new(
            GameplayCommandTypeId::new(command).map_err(|_| ManualRawAppBootError::CommandInput)?,
        ),
    );
    world
        .get_resource_mut::<GameplayCommandQueue>()
        .ok_or(ManualRawAppBootError::CommandSubmit)?
        .submit(submission)
        .map_err(|_| ManualRawAppBootError::CommandSubmit)?;
    Ok(())
}

fn require_one_fixed_step(frame: &AppFrameOutcome) -> Result<(), ManualRawAppBootError> {
    if frame.status.fixed_steps == 1 {
        Ok(())
    } else {
        Err(ManualRawAppBootError::UnexpectedFixedSteps)
    }
}

fn capture_first_tick(
    world: &World,
    instance: &SpawnedSceneInstance,
) -> Result<ManualFirstTickSnapshot, ManualRawAppBootError> {
    let player = resolve_scene_entity(world, instance, "player")?;
    let enemy = resolve_scene_entity(world, instance, "enemy-anchor/enemy")?;
    let player_component = world
        .get::<Player>(player)
        .ok_or(ManualRawAppBootError::SceneComponent)?;
    let enemy_component = world
        .get::<Enemy>(enemy)
        .ok_or(ManualRawAppBootError::SceneComponent)?;
    let weapon = world
        .get::<Weapon>(player)
        .ok_or(ManualRawAppBootError::SceneComponent)?;
    let fixed_time = world
        .get_resource::<FixedTime>()
        .ok_or(ManualRawAppBootError::SceneComponent)?;
    Ok(ManualFirstTickSnapshot {
        tick: fixed_time.tick(),
        player_position: player_component.position,
        player_hit_points: player_component.hit_points,
        enemy_position: enemy_component.position,
        enemy_hit_points: enemy_component.hit_points,
        weapon_remaining_ticks: weapon.remaining_ticks,
    })
}

fn resolve_scene_entity(
    world: &World,
    instance: &SpawnedSceneInstance,
    id: &str,
) -> Result<nara::prelude::Entity, ManualRawAppBootError> {
    match instance.resolve(world, &scene_id(id)) {
        EntityLookup::Resolved(entity) => Ok(entity),
        _ => Err(ManualRawAppBootError::SceneEntity),
    }
}

fn shutdown_manual_raw_app(app: &mut App) -> ManualRawAppRetirementReport {
    let task_shutdown = shutdown_manual_tasks(app);
    let plugin_shutdown = app
        .shutdown_plugins()
        .map_err(|_| ManualRawAppBootError::PluginShutdown);
    ManualRawAppRetirementReport {
        task_shutdown,
        plugin_shutdown,
    }
}

fn shutdown_manual_tasks(app: &App) -> Result<TaskShutdownReport, ManualRawAppBootError> {
    app.world()
        .get_resource::<TaskPools>()
        .ok_or(ManualRawAppBootError::TaskShutdown)
        .and_then(|pools| {
            pools
                .shutdown_blocking()
                .map_err(|_| ManualRawAppBootError::TaskShutdown)
        })
}

fn joined_task_workers(report: &TaskShutdownReport) -> usize {
    TaskPoolKind::ALL
        .into_iter()
        .map(|kind| report.for_kind(kind).joined_workers)
        .sum()
}

fn incomplete_task_phase(report: &TaskShutdownReport, kind: TaskPoolKind) -> TaskShutdownPhase {
    let report = report.for_kind(kind);
    if report.join_timed_out {
        TaskShutdownPhase::Join
    } else if report.cancel_timed_out {
        TaskShutdownPhase::Cancel
    } else {
        TaskShutdownPhase::Drain
    }
}

fn diagnostic_identifier(diagnostic: &nara::diagnostic::Diagnostic, key: &str) -> Option<String> {
    diagnostic.fields().iter().find_map(|field| {
        if field.key().as_str() != key {
            return None;
        }
        match field.value() {
            DiagnosticValueRef::Identifier(value) => Some(value.to_owned()),
            _ => None,
        }
    })
}

#[derive(Debug, Default, Resource)]
struct ManualPersistentApplyCanary(u64);

fn install_late_sprite_hook(world: &mut World) {
    world.init_resource::<ManualPersistentApplyCanary>();
    world.register_component::<Sprite>();
    world.commands().queue(|world: &mut World| {
        world
            .register_component_hooks::<Sprite>()
            .on_add(late_persistent_hook);
    });
}

fn late_persistent_hook(mut world: DeferredWorld<'_>, _context: HookContext) {
    world.resource_mut::<ManualPersistentApplyCanary>().0 += 1;
}

fn digest_hex(bytes: [u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        use fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}
