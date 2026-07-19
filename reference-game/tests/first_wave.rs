#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{num::NonZeroU32, time::Duration};

use nara::{
    app::{
        CoreStage, FixedUpdateSet, PluginCategory, PluginDeclaration, PluginDefinition, PluginId,
        RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter,
    },
    ecs::{With, schedule::IntoScheduleConfigs},
    gameplay::GameplayCommandSet,
    identity::WorldIdentityDomain,
    prelude::{App, EntityReference, FixedTime, Plugin, PluginError, Query, Res, Vec2, World},
    project_host::{HeadlessRun, HeadlessRunOutcome, HeadlessRunReport},
    scene::{SceneEntityId, SceneEntitySource},
};
use nara_reference_game::{
    Enemy, MovementCommandError, MovementDirection, Player, Projectile, ProjectileId,
    REFERENCE_WAVE_PLUGIN_ID, ReferenceWavePlugin, WaveOutcome, WaveSnapshot, WaveSpawn, Weapon,
    bundled_wave_commands, bundled_wave_run, movement_command, wave_headless_intent,
    wave_headless_run,
};
use project_content_fixture::project_root_capability;

const TIE_SETUP_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.tie-setup");
const INVALID_TOPOLOGY_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.invalid-topology");
const CONSUME_FAULT_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.consume-fault");
const CAPTURE_FAULT_PLUGIN_ID: PluginId = PluginId::new("reference-game.test.capture-fault");
const LATE_SIMULATION_FAULT_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.late-simulation-fault");
const TERMINAL_IDENTITY_AUDIT_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.terminal-identity-audit");
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
            corrupt_enemy_target_after_first_tick.in_set(GameplayCommandSet::Consume),
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

fn corrupt_enemy_target_after_first_tick(
    fixed_time: Res<FixedTime>,
    mut enemies: Query<&mut Enemy>,
) {
    if fixed_time.tick() < 2 {
        return;
    }
    let missing = SceneEntityId::new("missing-player").expect("test identity is valid");
    for mut enemy in &mut enemies {
        enemy.target = EntityReference::SceneLocal {
            entity: missing.clone(),
        };
    }
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
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.run.runtime-faulted"
    }));
    assert!(!report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.run.tick-limit"
    }));
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
    let intent = wave_headless_intent(maximum)
        .insert_after::<ReferenceWavePlugin>(terminal_identity_audit_plugin());
    let run = HeadlessRun::new(
        project_root_capability(),
        intent,
        bundled_wave_commands(),
    );

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
    let intent = wave_headless_intent(maximum)
        .insert_after::<ReferenceWavePlugin>(tie_setup_plugin());
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
    let intent = wave_headless_intent(NonZeroU32::new(4).unwrap())
        .insert_after::<ReferenceWavePlugin>(invalid_topology_plugin());
    let mut run = HeadlessRun::new(project_root_capability(), intent, Vec::new());

    let report = execute_to_terminal(&mut run);
    assert_runtime_fault_without_tick_limit(&report);
}

#[test]
fn consume_runtime_fault_skips_the_wave_simulation_tick() {
    let intent = wave_headless_intent(NonZeroU32::new(4).unwrap())
        .insert_after::<ReferenceWavePlugin>(consume_fault_plugin());
    let mut run = HeadlessRun::new(
        project_root_capability(),
        intent,
        bundled_wave_commands(),
    );

    let report = execute_to_terminal(&mut run);
    assert_runtime_fault_without_tick_limit(&report);
}

#[test]
fn capture_runtime_fault_prevents_wave_success() {
    let intent = wave_headless_intent(NonZeroU32::new(4).unwrap())
        .insert_after::<ReferenceWavePlugin>(capture_fault_plugin());
    let mut run = HeadlessRun::new(
        project_root_capability(),
        intent,
        bundled_wave_commands(),
    );

    let report = execute_to_terminal(&mut run);
    assert_runtime_fault_without_tick_limit(&report);
}

#[test]
fn late_simulation_fault_keeps_the_last_good_snapshot() {
    let intent = wave_headless_intent(NonZeroU32::new(4).unwrap())
        .insert_after::<ReferenceWavePlugin>(late_simulation_fault_plugin());
    let mut run = HeadlessRun::new(
        project_root_capability(),
        intent,
        bundled_wave_commands(),
    );

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
    assert!(codes.contains(&"project.run.command-rejected"), "{report:#?}");
    assert!(codes.contains(&"project.run.runtime-faulted"), "{report:#?}");
}
