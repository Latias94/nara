#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    num::NonZeroU32,
    time::{Duration, Instant},
};

use nara::{
    app::{
        CoreStage, FixedUpdateSet, PluginCategory, PluginDeclaration, PluginDefinition, PluginId,
        RuntimeAdmissionReservation, RuntimeClosePolicy, RuntimeControl,
        RuntimeControlRequestResult, RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter,
        RuntimeInstance, RuntimeObligationLedger, RuntimeState,
    },
    ecs::{
        Entity, Resource, With,
        component::ComponentId,
        lifecycle::{Add, Despawn},
        observer::{Observer, On},
        schedule::IntoScheduleConfigs,
    },
    gameplay::{GameplayCommandQueue, GameplayCommandSet, submit_gameplay_driver_command},
    identity::{EntityLookup, RuntimeEntityReference, TombstoneCause, WorldIdentityDomain},
    prelude::{App, Commands, FixedTime, Plugin, PluginError, Query, Res, ResMut, Vec2, World},
    project_host::{HeadlessRun, HeadlessRunOutcome, HeadlessRunReport, ProjectContentLoader},
    scene::{SceneEntitySource, spawn_scene},
};
use nara_reference_game::{
    Enemy, MovementCommandError, MovementDirection, Player, Projectile, ProjectileId,
    REFERENCE_WAVE_PLUGIN_ID, ReferenceWavePlugin, WaveOutcome, WaveRetryPhase, WaveRetryStatus,
    WaveRunGeneration, WaveSnapshot, WaveSpawn, Weapon, bundled_wave_commands, bundled_wave_run,
    advanced_wave_headless_intent_after, movement_command, retry_command, wave_headless_run,
};
use project_content_fixture::{
    headless_wave_candidate_plan_and_root, headless_wave_candidate_plan_and_root_with,
    project_root_capability,
};

const TIE_SETUP_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.tie-setup");
const INVALID_TOPOLOGY_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.invalid-topology");
const CONSUME_FAULT_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.consume-fault");
const CAPTURE_FAULT_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.capture-fault");
const LATE_SIMULATION_FAULT_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.late-simulation-fault");
const TERMINAL_IDENTITY_AUDIT_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.terminal-identity-audit");
const RETRY_IDENTITY_CORRUPTION_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.retry-identity-corruption");
const RETRY_CANDIDATE_DESPAWN_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.retry-candidate-despawn");
const RETRY_RETIREMENT_OBSERVER_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.retry-retirement-observer");
const TEST_PLUGIN_REQUIREMENTS: &[PluginId] = &[REFERENCE_WAVE_PLUGIN_ID];
const TIE_SETUP_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TIE_SETUP_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);
const INVALID_TOPOLOGY_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(INVALID_TOPOLOGY_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);
const CONSUME_FAULT_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CONSUME_FAULT_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);
const CAPTURE_FAULT_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CAPTURE_FAULT_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);
const LATE_SIMULATION_FAULT_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(LATE_SIMULATION_FAULT_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);
const TERMINAL_IDENTITY_AUDIT_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TERMINAL_IDENTITY_AUDIT_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);
const RETRY_IDENTITY_CORRUPTION_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(RETRY_IDENTITY_CORRUPTION_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);
const RETRY_CANDIDATE_DESPAWN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(RETRY_CANDIDATE_DESPAWN_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);
const RETRY_RETIREMENT_OBSERVER_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(RETRY_RETIREMENT_OBSERVER_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);

#[derive(Debug, Default)]
struct TieSetupPlugin;

impl Plugin for TieSetupPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TIE_SETUP_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::FixedUpdate,
            prepare_same_tick_tie.in_set(GameplayCommandSet::Consume),
        )?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct InvalidTopologyPlugin;

impl Plugin for InvalidTopologyPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &INVALID_TOPOLOGY_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::FixedUpdate,
            remove_wave_spawn.in_set(GameplayCommandSet::Consume),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            assert_invalid_topology_did_not_advance.after(FixedUpdateSet::Simulate),
        )?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ConsumeFaultPlugin;

impl Plugin for ConsumeFaultPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CONSUME_FAULT_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::FixedUpdate,
            report_consume_fault.in_set(GameplayCommandSet::Consume),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            assert_consume_fault_did_not_mutate.after(FixedUpdateSet::Simulate),
        )?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct CaptureFaultPlugin;

impl Plugin for CaptureFaultPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CAPTURE_FAULT_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::FixedUpdate,
            report_capture_fault.in_set(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct LateSimulationFaultPlugin;

impl Plugin for LateSimulationFaultPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &LATE_SIMULATION_FAULT_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::FixedUpdate,
            remove_player_role_after_first_tick.in_set(GameplayCommandSet::Consume),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            assert_failed_tick_kept_last_good_snapshot.after(GameplayCommandSet::Capture),
        )?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct TerminalIdentityAuditPlugin;

impl Plugin for TerminalIdentityAuditPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TERMINAL_IDENTITY_AUDIT_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::FixedUpdate,
            assert_completed_wave_retired_scene_identities.after(FixedUpdateSet::Simulate),
        )?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct RetryIdentityCorruptionPlugin;

impl Plugin for RetryIdentityCorruptionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RETRY_IDENTITY_CORRUPTION_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(RetryIdentityCorruption::default())?
            .add_systems(
                CoreStage::FixedUpdate,
                corrupt_retry_identity_after_terminal.after(GameplayCommandSet::Capture),
            )?;
        Ok(())
    }
}

#[derive(Debug, Default, Resource)]
struct RetryIdentityCorruption {
    applied: bool,
}

#[derive(Debug, Default)]
struct RetryCandidateDespawnPlugin;

impl Plugin for RetryCandidateDespawnPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RETRY_CANDIDATE_DESPAWN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(RetryCandidateObserver::default())?
            .add_systems(
                CoreStage::FixedUpdate,
                install_retry_candidate_observer_after_terminal.after(GameplayCommandSet::Capture),
            )?;
        Ok(())
    }
}

#[derive(Debug, Default, Resource)]
struct RetryCandidateObserver {
    installed: bool,
    triggered: u32,
}

#[derive(Debug, Default)]
struct RetryRetirementObserverPlugin;

impl Plugin for RetryRetirementObserverPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RETRY_RETIREMENT_OBSERVER_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(RetryRetirementObserver::default())?
            .add_systems(
                CoreStage::FixedUpdate,
                install_retry_retirement_observer_after_terminal.after(GameplayCommandSet::Capture),
            )?;
        Ok(())
    }
}

#[derive(Debug, Default, Resource)]
struct RetryRetirementObserver {
    installed: bool,
    triggered: u32,
}

fn prepare_same_tick_tie(
    mut players: Query<(&SceneEntitySource, &mut Player, &mut Weapon)>,
    mut enemies: Query<(&SceneEntitySource, &mut Enemy, &mut WaveSpawn)>,
) {
    for (source, mut player, mut weapon) in &mut players {
        if source.entity_id.as_str() != "player" {
            continue;
        }
        player.position = Vec2::ZERO;
        player.velocity = Vec2::ZERO;
        player.hit_points = 10;
        weapon.cooldown_ticks = 100;
        weapon.remaining_ticks = 1;
        weapon.damage = 10;
    }
    for (source, mut enemy, mut spawn) in &mut enemies {
        spawn.tick = 1;
        enemy.velocity = Vec2::ZERO;
        if source.entity_id.as_str() == "enemy-anchor/enemy" {
            enemy.position = Vec2::new(1.5, 0.0);
            enemy.hit_points = 10;
        } else {
            enemy.hit_points = 0;
        }
    }
}

fn remove_wave_spawn(world: &mut World) {
    let target = world.iter_entities().find_map(|entity| {
        (entity
            .get::<SceneEntitySource>()
            .is_some_and(|source| source.entity_id.as_str() == "enemy-anchor/enemy"))
        .then_some(entity.id())
    });
    if let Some(target) = target {
        world.entity_mut(target).remove::<WaveSpawn>();
    }
}

fn assert_invalid_topology_did_not_advance(players: Query<(&SceneEntitySource, &Player)>) {
    for (source, player) in &players {
        if source.entity_id.as_str() == "player" {
            assert_eq!(player.position, Vec2::ZERO);
        }
    }
}

fn assert_consume_fault_did_not_mutate(
    players: Query<(&SceneEntitySource, &Player)>,
    projectiles: Query<(&SceneEntitySource, Option<&ProjectileId>), With<Projectile>>,
) {
    assert_invalid_topology_did_not_advance(players);
    for (source, projectile_id) in &projectiles {
        if source.entity_id.as_str() == "projectile-fixture" {
            assert!(projectile_id.is_none());
        }
    }
}

fn report_consume_fault(faults: Res<RuntimeFaultReporter>) {
    faults.report(RuntimeFault::engine(
        RuntimeFaultKind::GameplayLifecycle,
        "reference-game.test.consume",
    ));
}

fn report_capture_fault(faults: Res<RuntimeFaultReporter>) {
    faults.report(RuntimeFault::engine(
        RuntimeFaultKind::GameplayLifecycle,
        "reference-game.test.capture",
    ));
}

fn remove_player_role_after_first_tick(world: &mut World) {
    if world.resource::<FixedTime>().tick() < 2 {
        return;
    }
    let players = world
        .iter_entities()
        .filter(|entity| entity.contains::<Player>())
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    for player in players {
        world.entity_mut(player).remove::<Player>();
    }
}

fn corrupt_retry_identity_after_terminal(world: &mut World) {
    if world.resource::<RetryIdentityCorruption>().applied
        || !world.resource::<WaveSnapshot>().is_terminal()
    {
        return;
    }
    let player = world
        .iter_entities()
        .find(|entity| {
            entity.contains::<Player>()
                && entity
                    .get::<SceneEntitySource>()
                    .is_some_and(|source| source.entity_id.as_str() == "player")
        })
        .map(|entity| entity.id())
        .expect("the terminal wave should retain its player identity");
    world
        .entity_mut(player)
        .remove::<SceneEntitySource>()
        .insert(Projectile::fixture());
    world.resource_mut::<RetryIdentityCorruption>().applied = true;
}

fn install_retry_candidate_observer_after_terminal(world: &mut World) {
    if world.resource::<RetryCandidateObserver>().installed
        || !world.resource::<WaveSnapshot>().is_terminal()
    {
        return;
    }
    world.add_observer(
        |event: On<Add, SceneEntitySource>,
         snapshot: Res<WaveSnapshot>,
         mut observer: ResMut<RetryCandidateObserver>,
         players: Query<(Entity, &SceneEntitySource), With<Player>>,
         mut commands: Commands| {
            if snapshot.is_terminal() && observer.triggered == 0 {
                observer.triggered = 1;
                if let Some((old_player, _)) = players.iter().find(|(entity, source)| {
                    *entity != event.entity && source.entity_id.as_str() == "player"
                }) {
                    commands.entity(old_player).despawn();
                }
            }
        },
    );
    world.flush();
    world.resource_mut::<RetryCandidateObserver>().installed = true;
}

fn install_retry_retirement_observer_after_terminal(world: &mut World) {
    if world.resource::<RetryRetirementObserver>().installed
        || !world.resource::<WaveSnapshot>().is_terminal()
    {
        return;
    }
    let old_player = world
        .iter_entities()
        .find(|entity| {
            entity.contains::<Player>()
                && entity
                    .get::<SceneEntitySource>()
                    .is_some_and(|source| source.entity_id.as_str() == "player")
        })
        .map(|entity| entity.id())
        .expect("the terminal wave should retain its player entity");
    world.spawn(
        Observer::new(
            |event: On<Despawn>,
             mut observer: ResMut<RetryRetirementObserver>,
             players: Query<(Entity, &SceneEntitySource), With<Player>>,
             mut commands: Commands| {
                observer.triggered += 1;
                if let Some((candidate, _)) = players.iter().find(|(entity, source)| {
                    *entity != event.entity && source.entity_id.as_str() == "player"
                }) {
                    commands.entity(candidate).despawn();
                }
            },
        )
        .with_entity(old_player),
    );
    world.flush();
    world.resource_mut::<RetryRetirementObserver>().installed = true;
}

fn assert_failed_tick_kept_last_good_snapshot(
    fixed_time: Res<FixedTime>,
    snapshot: Res<WaveSnapshot>,
) {
    if fixed_time.tick() >= 2 {
        assert_eq!(snapshot.tick, 1);
        assert_eq!(snapshot.outcome, WaveOutcome::Running);
    }
}

fn assert_completed_wave_retired_scene_identities(world: &mut World) {
    if world.query::<&Enemy>().iter(world).next().is_some() {
        return;
    }
    let stats = world.resource::<WorldIdentityDomain>().stats();
    assert_eq!(stats.active_scene_entities, 4);
    assert!(stats.recent_tombstones >= 4);
}

fn tie_setup_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<TieSetupPlugin>()
}

fn invalid_topology_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<InvalidTopologyPlugin>()
}

fn consume_fault_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ConsumeFaultPlugin>()
}

fn capture_fault_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<CaptureFaultPlugin>()
}

fn late_simulation_fault_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<LateSimulationFaultPlugin>()
}

fn terminal_identity_audit_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<TerminalIdentityAuditPlugin>()
}

fn retry_identity_corruption_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<RetryIdentityCorruptionPlugin>()
}

fn retry_candidate_despawn_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<RetryCandidateDespawnPlugin>()
}

fn retry_retirement_observer_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<RetryRetirementObserverPlugin>()
}

fn execute_to_terminal(run: &mut HeadlessRun<WaveSnapshot>) -> HeadlessRunReport<WaveSnapshot> {
    for _ in 0..64 {
        let report = run.execute_bounded();
        if report.outcome() != &HeadlessRunOutcome::CleanupIncomplete {
            return report;
        }
        std::thread::park_timeout(Duration::from_millis(1));
    }
    panic!("reference wave cleanup did not finish");
}

fn assert_runtime_fault_without_tick_limit(report: &HeadlessRunReport<WaveSnapshot>) {
    assert_eq!(report.outcome(), &HeadlessRunOutcome::Failed);
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "project.run.runtime-faulted" })
    );
    assert!(
        !report
            .diagnostics()
            .iter()
            .any(|diagnostic| { diagnostic.code().as_str() == "project.run.tick-limit" })
    );
}

fn complete(mut run: HeadlessRun<WaveSnapshot>) -> WaveSnapshot {
    let report = execute_to_terminal(&mut run);
    let HeadlessRunOutcome::Completed(snapshot) = report.outcome() else {
        panic!("reference wave failed: {report:#?}");
    };
    assert!(!report.diagnostics().has_errors(), "{report:#?}");
    snapshot.clone()
}

#[test]
fn bundled_wave_completes_at_a_stable_tick() {
    let maximum = NonZeroU32::new(96).unwrap();
    let first = complete(bundled_wave_run(project_root_capability(), maximum));
    let second = complete(bundled_wave_run(project_root_capability(), maximum));

    assert_eq!(first, second);
    assert_eq!(first.outcome, WaveOutcome::Completed);
    assert_eq!(first.tick, 49);
    assert_eq!(first.score, 300);
    assert_eq!(first.planned_enemies, 3);
    assert_eq!(first.defeated_enemies, 3);
    assert_eq!(first.player.hit_points, 20);
    assert!(first.enemies.is_empty());
    assert!(first.tick > 1 && first.tick < u64::from(maximum.get()));
}

#[test]
fn completed_wave_retires_scene_identities_before_runtime_despawn() {
    let maximum = NonZeroU32::new(96).unwrap();
    let intent = advanced_wave_headless_intent_after::<ReferenceWavePlugin>(
        maximum,
        terminal_identity_audit_plugin(),
    );
    let run = HeadlessRun::new(project_root_capability(), intent, bundled_wave_commands());

    let snapshot = complete(run);

    assert_eq!(snapshot.outcome, WaveOutcome::Completed);
}

#[test]
fn moving_toward_the_wave_reaches_defeat_at_a_stable_tick() {
    let maximum = NonZeroU32::new(32).unwrap();
    let commands = vec![movement_command(1, 1, MovementDirection::Right).unwrap()];
    let first = complete(wave_headless_run(
        project_root_capability(),
        maximum,
        commands.clone(),
    ));
    let second = complete(wave_headless_run(
        project_root_capability(),
        maximum,
        commands,
    ));

    assert_eq!(first, second);
    assert_eq!(first.outcome, WaveOutcome::Defeated);
    assert_eq!(first.player.hit_points, 0);
    assert_eq!(first.tick, 4);
    assert_eq!(first.planned_enemies, 3);
    assert!(!first.enemies.is_empty());
}

#[test]
fn movement_command_rejects_zero_ordering_values_without_panicking() {
    assert_eq!(
        movement_command(0, 1, MovementDirection::Stop),
        Err(MovementCommandError::ZeroTick)
    );
    assert_eq!(
        movement_command(1, 0, MovementDirection::Stop),
        Err(MovementCommandError::ZeroSequence)
    );
}

#[test]
fn same_tick_player_and_final_enemy_death_resolves_to_defeat() {
    let maximum = NonZeroU32::new(4).unwrap();
    let intent = advanced_wave_headless_intent_after::<ReferenceWavePlugin>(
        maximum,
        tie_setup_plugin(),
    );
    let run = HeadlessRun::new(project_root_capability(), intent, Vec::new());

    let snapshot = complete(run);

    assert_eq!(snapshot.outcome, WaveOutcome::Defeated);
    assert_eq!(snapshot.tick, 1);
    assert_eq!(snapshot.player.hit_points, 0);
    assert_eq!(snapshot.score, 300);
    assert_eq!(snapshot.planned_enemies, 3);
    assert_eq!(snapshot.defeated_enemies, 3);
    assert!(snapshot.enemies.is_empty());
}

#[test]
fn missing_wave_spawn_faults_on_the_first_tick_instead_of_timing_out() {
    let intent = advanced_wave_headless_intent_after::<ReferenceWavePlugin>(
        NonZeroU32::new(4).unwrap(),
        invalid_topology_plugin(),
    );
    let mut run = HeadlessRun::new(project_root_capability(), intent, Vec::new());

    let report = execute_to_terminal(&mut run);
    assert_runtime_fault_without_tick_limit(&report);
}

#[test]
fn consume_runtime_fault_skips_the_wave_simulation_tick() {
    let intent = advanced_wave_headless_intent_after::<ReferenceWavePlugin>(
        NonZeroU32::new(4).unwrap(),
        consume_fault_plugin(),
    );
    let mut run = HeadlessRun::new(project_root_capability(), intent, bundled_wave_commands());

    let report = execute_to_terminal(&mut run);
    assert_runtime_fault_without_tick_limit(&report);
}

#[test]
fn capture_runtime_fault_prevents_wave_success() {
    let intent = advanced_wave_headless_intent_after::<ReferenceWavePlugin>(
        NonZeroU32::new(4).unwrap(),
        capture_fault_plugin(),
    );
    let mut run = HeadlessRun::new(project_root_capability(), intent, bundled_wave_commands());

    let report = execute_to_terminal(&mut run);
    assert_runtime_fault_without_tick_limit(&report);
}

#[test]
fn late_simulation_fault_keeps_the_last_good_snapshot() {
    let intent = advanced_wave_headless_intent_after::<ReferenceWavePlugin>(
        NonZeroU32::new(4).unwrap(),
        late_simulation_fault_plugin(),
    );
    let mut run = HeadlessRun::new(project_root_capability(), intent, bundled_wave_commands());

    let report = execute_to_terminal(&mut run);
    assert_runtime_fault_without_tick_limit(&report);
}

#[test]
fn rejected_trusted_command_faults_the_candidate_before_success() {
    let commands = (1..=4_097)
        .map(|sequence| movement_command(1, sequence, MovementDirection::Left).unwrap())
        .collect::<Vec<_>>();
    let mut run = wave_headless_run(
        project_root_capability(),
        NonZeroU32::new(1).unwrap(),
        commands,
    );
    let report = execute_to_terminal(&mut run);
    assert_runtime_fault_without_tick_limit(&report);
    let codes = report
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().as_str())
        .collect::<Vec<_>>();
    assert!(
        codes.contains(&"project.run.command-rejected"),
        "{report:#?}"
    );
    assert!(
        codes.contains(&"project.run.runtime-faulted"),
        "{report:#?}"
    );
}

#[test]
fn semantic_retry_resets_the_wave_without_a_platform_or_runtime_replacement() {
    let mut runtime = adapter_retry_runtime();
    let runtime_generation = runtime.generation();

    drive_adapter_to_terminal(&mut runtime);
    assert_eq!(
        runtime.world().resource::<WaveSnapshot>().outcome,
        WaveOutcome::Completed
    );
    let fixed_tick = runtime.world().resource::<FixedTime>().tick();
    runtime
        .with_driver_scope(|scope| {
            submit_gameplay_driver_command(scope, retry_command(fixed_tick + 1, 2).unwrap())
        })
        .unwrap()
        .unwrap()
        .unwrap();

    drive_adapter_fixed(&mut runtime);
    assert_eq!(runtime.world().resource::<WaveRunGeneration>().get(), 1);
    let retry = runtime.world().resource::<WaveRetryStatus>();
    assert_eq!(retry.phase(), WaveRetryPhase::Pending);
    assert_eq!(retry.pending_generation(), Some(1));

    drive_adapter_fixed(&mut runtime);

    let snapshot = runtime.world().resource::<WaveSnapshot>();
    assert_eq!(runtime.generation(), runtime_generation);
    assert_eq!(runtime.world().resource::<WaveRunGeneration>().get(), 2);
    assert_eq!(snapshot.run_generation, 2);
    assert_eq!(snapshot.tick, 1);
    assert_eq!(snapshot.outcome, WaveOutcome::Running);
    assert_eq!(snapshot.player.hit_points, 20);
    assert_eq!(snapshot.enemies.len(), 3);
    let retry = runtime.world().resource::<WaveRetryStatus>();
    assert_eq!(retry.phase(), WaveRetryPhase::Applied);
    assert_eq!(retry.pending_generation(), None);
    assert_eq!(retry.applied_generation(), Some(2));
    stop_adapter_retry_runtime(runtime);
}

#[test]
fn successful_retry_tombstones_old_identity_and_resolves_replacement() {
    let mut runtime = adapter_retry_runtime();
    drive_adapter_to_terminal(&mut runtime);
    let (old_entity, old_reference) = player_scene_identity(runtime.world());

    submit_retry_for_next_tick(&mut runtime, 2);
    drive_adapter_to_run_generation(&mut runtime, 2);

    let (new_entity, new_reference) = player_scene_identity(runtime.world());
    assert_ne!(new_entity, old_entity);
    assert_ne!(new_reference, old_reference);
    let domain = runtime.world().resource::<WorldIdentityDomain>();
    assert!(matches!(
        domain.lookup(runtime.world(), &old_reference),
        EntityLookup::Tombstoned(Some(tombstone))
            if tombstone.cause() == TombstoneCause::Replaced
    ));
    assert_eq!(
        domain.lookup(runtime.world(), &new_reference),
        EntityLookup::Resolved(new_entity)
    );
    stop_adapter_retry_runtime(runtime);
}

#[test]
fn retry_topology_failure_preserves_existing_identity_generation_and_stats() {
    let mut runtime = adapter_retry_runtime_with_test_plugin(retry_identity_corruption_plugin());
    drive_adapter_fixed(&mut runtime);
    let (old_entity, old_reference) = player_scene_identity(runtime.world());
    drive_adapter_to_terminal(&mut runtime);
    let identity_stats = runtime.world().resource::<WorldIdentityDomain>().stats();
    assert_eq!(
        runtime
            .world()
            .resource::<WorldIdentityDomain>()
            .lookup(runtime.world(), &old_reference),
        EntityLookup::Resolved(old_entity)
    );

    submit_retry_for_next_tick(&mut runtime, 2);
    let failure = (0..4)
        .find_map(|_| {
            let timestep = runtime.world().resource::<FixedTime>().timestep();
            runtime.drive(timestep).err()
        })
        .expect("the corrupt retry topology should fault before identity publication");

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::System);
    assert_eq!(runtime.world().resource::<WaveRunGeneration>().get(), 1);
    let domain = runtime.world().resource::<WorldIdentityDomain>();
    assert_eq!(domain.stats(), identity_stats);
    assert_eq!(
        domain.lookup(runtime.world(), &old_reference),
        EntityLookup::Resolved(old_entity)
    );
    stop_adapter_retry_runtime(runtime);
}

#[test]
fn retry_candidate_lifecycle_work_rejects_before_old_authority_can_change() {
    let mut runtime = adapter_retry_runtime_with_test_plugin(retry_candidate_despawn_plugin());
    drive_adapter_to_terminal(&mut runtime);
    assert!(
        runtime
            .world()
            .resource::<RetryCandidateObserver>()
            .installed
    );
    let (old_entity, old_reference) = player_scene_identity(runtime.world());

    submit_retry_for_next_tick(&mut runtime, 2);
    drive_adapter_fixed(&mut runtime);
    assert_eq!(
        runtime.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Pending
    );
    let topology = entity_component_topology(runtime.world());
    let identity_stats = runtime.world().resource::<WorldIdentityDomain>().stats();
    let snapshot = runtime.world().resource::<WaveSnapshot>().clone();

    let timestep = runtime.world().resource::<FixedTime>().timestep();
    let failure = runtime
        .drive(timestep)
        .expect_err("candidate lifecycle work must reject before component insertion");

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::System);
    assert_eq!(runtime.world().resource::<WaveRunGeneration>().get(), 1);
    assert_eq!(
        runtime
            .world()
            .resource::<RetryCandidateObserver>()
            .triggered,
        0,
        "candidate validation must not execute an observer that could mutate the old scene"
    );
    assert_eq!(entity_component_topology(runtime.world()), topology);
    let domain = runtime.world().resource::<WorldIdentityDomain>();
    assert_eq!(domain.stats(), identity_stats);
    assert_eq!(
        domain.lookup(runtime.world(), &old_reference),
        EntityLookup::Resolved(old_entity)
    );
    assert_eq!(runtime.world().resource::<WaveSnapshot>(), &snapshot);
    let retry = runtime.world().resource::<WaveRetryStatus>();
    assert_eq!(retry.phase(), WaveRetryPhase::Pending);
    assert_eq!(retry.pending_generation(), Some(1));
    assert_eq!(retry.applied_generation(), None);
    stop_adapter_retry_runtime(runtime);
}

#[test]
fn retry_retirement_lifecycle_work_rejects_before_candidate_publication() {
    let mut runtime = adapter_retry_runtime_with_test_plugin(retry_retirement_observer_plugin());
    drive_adapter_to_terminal(&mut runtime);
    assert!(
        runtime
            .world()
            .resource::<RetryRetirementObserver>()
            .installed
    );
    let (old_entity, old_reference) = player_scene_identity(runtime.world());

    submit_retry_for_next_tick(&mut runtime, 2);
    drive_adapter_fixed(&mut runtime);
    assert_eq!(
        runtime.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Pending
    );
    let topology = entity_component_topology(runtime.world());
    let identity_stats = runtime.world().resource::<WorldIdentityDomain>().stats();
    let snapshot = runtime.world().resource::<WaveSnapshot>().clone();

    let timestep = runtime.world().resource::<FixedTime>().timestep();
    let failure = runtime
        .drive(timestep)
        .expect_err("old-entity lifecycle work must reject before replacement publication");

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::System);
    assert_eq!(runtime.world().resource::<WaveRunGeneration>().get(), 1);
    assert_eq!(
        runtime
            .world()
            .resource::<RetryRetirementObserver>()
            .triggered,
        0,
        "retirement validation must not execute the rejected observer"
    );
    assert_eq!(entity_component_topology(runtime.world()), topology);
    let domain = runtime.world().resource::<WorldIdentityDomain>();
    assert_eq!(domain.stats(), identity_stats);
    assert_eq!(
        domain.lookup(runtime.world(), &old_reference),
        EntityLookup::Resolved(old_entity)
    );
    assert_eq!(runtime.world().resource::<WaveSnapshot>(), &snapshot);
    let retry = runtime.world().resource::<WaveRetryStatus>();
    assert_eq!(retry.phase(), WaveRetryPhase::Pending);
    assert_eq!(retry.pending_generation(), Some(1));
    assert_eq!(retry.applied_generation(), None);
    stop_adapter_retry_runtime(runtime);
}

fn adapter_retry_runtime() -> RuntimeInstance {
    build_adapter_retry_runtime(headless_wave_candidate_plan_and_root())
}

fn adapter_retry_runtime_with_test_plugin(test_plugin: PluginDefinition) -> RuntimeInstance {
    build_adapter_retry_runtime(headless_wave_candidate_plan_and_root_with(test_plugin))
}

fn build_adapter_retry_runtime(
    (project, plan, root): (
        nara::project_host::ProjectSettingsCandidate,
        nara::project_host::RuntimePlan,
        nara::fs::DirectoryCapability,
    ),
) -> RuntimeInstance {
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&project, &plan).unwrap();
    let scene = snapshot.expanded_startup_scene().clone();
    let sealed = plan.plugin_plan().instantiate().unwrap();
    let mut candidate = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            sealed,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut World| {
                let report = spawn_scene(world, plan.schema_validation().registry(), &scene);
                assert!(
                    !report.diagnostics.has_errors(),
                    "{:#?}",
                    report.diagnostics
                );
                assert!(report.instance.is_some());
                world
                    .resource_mut::<GameplayCommandQueue>()
                    .submit(movement_command(1, 1, MovementDirection::Left).unwrap())
                    .unwrap();
            });
        })
        .unwrap();
    candidate.complete_startup().unwrap().promote()
}

fn drive_adapter_to_terminal(runtime: &mut RuntimeInstance) {
    for _ in 0..96 {
        drive_adapter_fixed(runtime);
        if runtime.world().resource::<WaveSnapshot>().is_terminal() {
            return;
        }
    }
    panic!("adapter retry runtime did not reach a terminal wave");
}

fn submit_retry_for_next_tick(runtime: &mut RuntimeInstance, sequence: u64) {
    let fixed_tick = runtime.world().resource::<FixedTime>().tick();
    runtime
        .with_driver_scope(|scope| {
            submit_gameplay_driver_command(scope, retry_command(fixed_tick + 1, sequence).unwrap())
        })
        .unwrap()
        .unwrap()
        .unwrap();
}

fn drive_adapter_to_run_generation(runtime: &mut RuntimeInstance, generation: u64) {
    for _ in 0..4 {
        drive_adapter_fixed(runtime);
        if runtime.world().resource::<WaveRunGeneration>().get() == generation {
            return;
        }
    }
    panic!("adapter retry runtime did not publish run generation {generation}");
}

fn drive_adapter_fixed(runtime: &mut RuntimeInstance) {
    let timestep = runtime.world().resource::<FixedTime>().timestep();
    runtime.drive(timestep).unwrap();
}

fn player_scene_identity(world: &World) -> (Entity, RuntimeEntityReference) {
    world
        .iter_entities()
        .find_map(|entity| {
            let source = entity.get::<SceneEntitySource>()?;
            (entity.contains::<Player>() && source.entity_id.as_str() == "player").then(|| {
                (
                    entity.id(),
                    RuntimeEntityReference::scene(source.instance_id, source.entity_id.clone()),
                )
            })
        })
        .expect("the wave should expose one scene-managed player")
}

fn entity_component_topology(world: &World) -> Vec<(Entity, Vec<ComponentId>)> {
    let mut topology = world
        .iter_entities()
        .map(|entity| (entity.id(), entity.archetype().components().to_vec()))
        .collect::<Vec<_>>();
    topology.sort_by_key(|(entity, _)| *entity);
    topology
}

fn stop_adapter_retry_runtime(mut runtime: RuntimeInstance) {
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .expect("the adapter runtime cleanup deadline is bounded");
    loop {
        if runtime.state() == RuntimeState::Stopped {
            return;
        }
        if runtime.state() == RuntimeState::CloseIncomplete {
            assert!(matches!(
                runtime.request_control(RuntimeControl::RetryClose),
                RuntimeControlRequestResult::Accepted(_)
            ));
        }
        let _ = runtime.drive(Duration::ZERO);
        if Instant::now() >= deadline {
            panic!(
                "adapter retry runtime did not stop before its deadline; state={:?}",
                runtime.state()
            );
        }
        std::thread::park_timeout(Duration::from_millis(1));
    }
}
