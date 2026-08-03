#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    collections::BTreeMap,
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::{StartupSceneSource, materialize_startup_scene},
    app::{
        CoreStage, FixedUpdateSet, PluginCategory, PluginDeclaration, PluginDefinition, PluginId,
        RuntimeAdmissionReservation, RuntimeCandidateRetirementState, RuntimeClosePolicy,
        RuntimeControl, RuntimeControlRequestResult, RuntimeFault, RuntimeFaultKind,
        RuntimeFaultReporter, RuntimeInstance, RuntimeObligationLedger, RuntimeState,
    },
    core::ByteLimit,
    ecs::{
        Entity, Resource, With, Without,
        component::ComponentId,
        lifecycle::{Add, Despawn},
        observer::{Observer, On},
        schedule::IntoScheduleConfigs,
    },
    gameplay::{GameplayCommandSet, submit_gameplay_driver_command},
    identity::{
        EntityLookup, RuntimeEntityReference, TombstoneCause, WorldIdentityDomain,
        spawn_identity_entity,
    },
    prelude::{
        App, Commands, ComponentTypeId, ComponentValue, FixedTime, Parent, Plugin, PluginError,
        Query, Res, ResMut, Sprite, Transform2d, Vec2, World,
    },
    project_host::{HeadlessRun, HeadlessRunOutcome, HeadlessRunReport, ProjectContentLoader},
    scene::SceneEntitySource,
};
use nara_reference_game::{
    EnemyRole, Health, InitialHealth, InitialVelocity2d, MovementCommandError, MovementDirection,
    PlayerRole, ProjectileDamage, ProjectileId, ProjectileLifetime, ProjectileRole,
    REFERENCE_WAVE_PLUGIN_ID, ReferenceWavePlugin, Velocity2d, WaveOutcome, WaveRetryPhase,
    WaveRetryRejection, WaveRetryStatus, WaveRunGeneration, WaveSnapshot, WaveSpawn, Weapon,
    WeaponCooldown, advanced_wave_headless_intent_after, bundled_wave_commands, bundled_wave_run,
    movement_command, retry_command, wave_headless_run,
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
const RUN_INITIALIZATION_AUDIT_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.run-initialization-audit");
const PROJECTILE_SENTINEL_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.projectile-sentinel");
const FOREIGN_GAMEPLAY_SENTINEL_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.test.foreign-gameplay-sentinel");
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
const RUN_INITIALIZATION_AUDIT_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(RUN_INITIALIZATION_AUDIT_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);
const PROJECTILE_SENTINEL_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PROJECTILE_SENTINEL_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(TEST_PLUGIN_REQUIREMENTS);
const FOREIGN_GAMEPLAY_SENTINEL_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(FOREIGN_GAMEPLAY_SENTINEL_PLUGIN_ID, PluginCategory::Runtime)
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
                (
                    install_retry_candidate_observer_after_terminal,
                    repair_retry_candidate_observer_after_rejection,
                )
                    .chain()
                    .after(GameplayCommandSet::Capture),
            )?;
        Ok(())
    }
}

#[derive(Debug, Default, Resource)]
struct RetryCandidateObserver {
    installed: bool,
    triggered: u32,
    observer_entity: Option<Entity>,
    repair_at_tick: Option<u64>,
    repaired: bool,
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

#[derive(Debug, Default)]
struct RunInitializationAuditPlugin;

impl Plugin for RunInitializationAuditPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RUN_INITIALIZATION_AUDIT_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(RunInitializationAudit::default())?
            .add_systems(
                CoreStage::FixedUpdate,
                capture_run_initialization.in_set(GameplayCommandSet::Consume),
            )?;
        Ok(())
    }
}

#[derive(Debug, Default, Resource)]
struct RunInitializationAudit {
    generations: BTreeMap<u64, NormalizedRunState>,
}

#[derive(Debug, Default)]
struct ProjectileSentinelPlugin;

impl Plugin for ProjectileSentinelPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PROJECTILE_SENTINEL_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ProjectileSentinel::default())?
            .add_systems(
                CoreStage::FixedUpdate,
                spawn_same_shaped_projectile_sentinel.after(GameplayCommandSet::Capture),
            )?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ForeignGameplaySentinelPlugin;

impl Plugin for ForeignGameplaySentinelPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &FOREIGN_GAMEPLAY_SENTINEL_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ForeignGameplaySentinels::default())?
            .add_systems(
                CoreStage::FixedUpdate,
                spawn_foreign_gameplay_sentinels.before(GameplayCommandSet::Consume),
            )?;
        Ok(())
    }
}

#[derive(Debug, Default, Resource)]
struct ProjectileSentinel {
    entity: Option<Entity>,
    owned_projectile: Option<Entity>,
}

#[derive(Debug, Default, Resource)]
struct ForeignGameplaySentinels {
    player: Option<Entity>,
    enemy: Option<Entity>,
}

#[derive(Debug, Clone, PartialEq)]
struct NormalizedRunState {
    actors: Vec<NormalizedActorState>,
    weapon: NormalizedWeaponState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ActorRole {
    Player,
    Enemy,
}

#[derive(Debug, Clone, PartialEq)]
struct NormalizedActorState {
    role: ActorRole,
    scene_id: String,
    transform: Transform2d,
    sprite: Sprite,
    initial_health: InitialHealth,
    initial_velocity: InitialVelocity2d,
    health: Health,
    velocity: Velocity2d,
    wave_spawn: Option<WaveSpawn>,
}

#[derive(Debug, Clone, PartialEq)]
struct NormalizedWeaponState {
    scene_id: String,
    parent_scene_id: String,
    transform: Transform2d,
    sprite: Sprite,
    weapon: Weapon,
    cooldown: WeaponCooldown,
}

#[allow(
    clippy::type_complexity,
    reason = "the test system signature is the complete same-tick state injection contract"
)]
fn prepare_same_tick_tie(
    mut players: Query<
        (
            &SceneEntitySource,
            &mut Transform2d,
            &mut Velocity2d,
            &mut Health,
        ),
        (With<PlayerRole>, Without<EnemyRole>),
    >,
    mut enemies: Query<
        (
            &SceneEntitySource,
            &mut Transform2d,
            &mut Velocity2d,
            &mut Health,
            &mut WaveSpawn,
        ),
        (With<EnemyRole>, Without<PlayerRole>),
    >,
    mut weapons: Query<(&mut Weapon, &mut WeaponCooldown)>,
) {
    for (source, mut transform, mut velocity, mut health) in &mut players {
        if source.entity_id.as_str() != "player" {
            continue;
        }
        transform.translation = Vec2::ZERO;
        velocity.value = Vec2::ZERO;
        health.current = 10;
    }
    for (mut weapon, mut cooldown) in &mut weapons {
        weapon.cooldown_ticks = 100;
        weapon.damage = 10;
        cooldown.remaining_ticks = 1;
    }
    for (source, mut transform, mut velocity, mut health, mut spawn) in &mut enemies {
        spawn.tick = 1;
        velocity.value = Vec2::ZERO;
        if source.entity_id.as_str() == "enemy-anchor/enemy" {
            transform.translation = Vec2::new(1.5, 0.0);
            health.current = 10;
        } else {
            health.current = 0;
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

fn assert_invalid_topology_did_not_advance(
    players: Query<(&SceneEntitySource, &Transform2d), With<PlayerRole>>,
) {
    for (source, transform) in &players {
        if source.entity_id.as_str() == "player" {
            assert_eq!(transform.translation, Vec2::ZERO);
        }
    }
}

fn assert_consume_fault_did_not_mutate(
    players: Query<(&SceneEntitySource, &Transform2d), With<PlayerRole>>,
    projectiles: Query<(), With<ProjectileRole>>,
) {
    assert_invalid_topology_did_not_advance(players);
    assert!(projectiles.is_empty());
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
        .filter(|entity| entity.contains::<PlayerRole>())
        .map(|entity| entity.id())
        .collect::<Vec<_>>();
    for player in players {
        world.entity_mut(player).remove::<PlayerRole>();
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
            entity.contains::<PlayerRole>()
                && entity
                    .get::<SceneEntitySource>()
                    .is_some_and(|source| source.entity_id.as_str() == "player")
        })
        .map(|entity| entity.id())
        .expect("the terminal wave should retain its player identity");
    world
        .entity_mut(player)
        .remove::<SceneEntitySource>()
        .insert((
            ProjectileRole,
            Velocity2d {
                value: Vec2::new(2.0, 0.0),
            },
            ProjectileDamage { amount: 3 },
            ProjectileLifetime { remaining_ticks: 4 },
        ));
    world.resource_mut::<RetryIdentityCorruption>().applied = true;
}

fn install_retry_candidate_observer_after_terminal(world: &mut World) {
    if world.resource::<RetryCandidateObserver>().installed
        || !world.resource::<WaveSnapshot>().is_terminal()
    {
        return;
    }
    let observer_entity = world
        .add_observer(
            |event: On<Add, SceneEntitySource>,
             snapshot: Res<WaveSnapshot>,
             mut observer: ResMut<RetryCandidateObserver>,
             players: Query<(Entity, &SceneEntitySource), With<PlayerRole>>,
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
        )
        .id();
    world.flush();
    let mut observer = world.resource_mut::<RetryCandidateObserver>();
    observer.installed = true;
    observer.observer_entity = Some(observer_entity);
}

fn repair_retry_candidate_observer_after_rejection(world: &mut World) {
    let fixed_tick = world.resource::<FixedTime>().tick();
    let rejected = world.resource::<WaveRetryStatus>().last_rejection()
        == Some(WaveRetryRejection::ReplacementRejected);
    let repair_at_tick = world.resource::<RetryCandidateObserver>().repair_at_tick;
    if rejected && repair_at_tick.is_none() {
        world
            .resource_mut::<RetryCandidateObserver>()
            .repair_at_tick = fixed_tick.checked_add(1);
        return;
    }
    if repair_at_tick.is_none_or(|repair_at_tick| fixed_tick < repair_at_tick) {
        return;
    }
    let observer_entity = world
        .resource_mut::<RetryCandidateObserver>()
        .observer_entity
        .take()
        .expect("the installed retry observer should remain repairable");
    assert!(world.despawn(observer_entity));
    let mut observer = world.resource_mut::<RetryCandidateObserver>();
    observer.repair_at_tick = None;
    observer.repaired = true;
}

fn capture_run_initialization(world: &mut World) {
    let generation = world.resource::<WaveRunGeneration>().get();
    if world
        .resource::<RunInitializationAudit>()
        .generations
        .contains_key(&generation)
    {
        return;
    }
    let state = normalized_run_state(world);
    world
        .resource_mut::<RunInitializationAudit>()
        .generations
        .insert(generation, state);
}

fn normalized_run_state(world: &World) -> NormalizedRunState {
    let mut actors = world
        .iter_entities()
        .filter_map(|entity| {
            let role = if entity.contains::<PlayerRole>() {
                ActorRole::Player
            } else if entity.contains::<EnemyRole>() {
                ActorRole::Enemy
            } else {
                return None;
            };
            let source = entity
                .get::<SceneEntitySource>()
                .expect("every initialized actor should retain its scene identity");
            Some(NormalizedActorState {
                role,
                scene_id: source.entity_id.as_str().to_owned(),
                transform: *entity
                    .get::<Transform2d>()
                    .expect("every initialized actor should have an authored transform"),
                sprite: entity
                    .get::<Sprite>()
                    .expect("every initialized actor should retain authored presentation")
                    .clone(),
                initial_health: *entity
                    .get::<InitialHealth>()
                    .expect("every initialized actor should retain authored health"),
                initial_velocity: entity
                    .get::<InitialVelocity2d>()
                    .expect("every initialized actor should retain authored velocity")
                    .clone(),
                health: *entity
                    .get::<Health>()
                    .expect("every initialized actor should have runtime health"),
                velocity: *entity
                    .get::<Velocity2d>()
                    .expect("every initialized actor should have runtime velocity"),
                wave_spawn: entity.get::<WaveSpawn>().copied(),
            })
        })
        .collect::<Vec<_>>();
    actors.sort_by(|left, right| {
        left.role
            .cmp(&right.role)
            .then_with(|| left.scene_id.cmp(&right.scene_id))
    });

    let weapon_entity = world
        .iter_entities()
        .find(|entity| {
            entity.contains::<Weapon>()
                && entity
                    .get::<SceneEntitySource>()
                    .is_some_and(|source| source.entity_id.as_str() == "player-weapon")
        })
        .expect("the initialized run should expose one authored player weapon");
    let source = weapon_entity
        .get::<SceneEntitySource>()
        .expect("the weapon should retain its scene identity");
    let parent = weapon_entity
        .get::<Parent>()
        .expect("the authored weapon should retain its parent");
    let parent_source = world
        .get::<SceneEntitySource>(parent.parent())
        .expect("the weapon parent should retain its scene identity");
    let weapon = NormalizedWeaponState {
        scene_id: source.entity_id.as_str().to_owned(),
        parent_scene_id: parent_source.entity_id.as_str().to_owned(),
        transform: *weapon_entity
            .get::<Transform2d>()
            .expect("the weapon should retain its authored transform"),
        sprite: weapon_entity
            .get::<Sprite>()
            .expect("the weapon should retain its authored presentation")
            .clone(),
        weapon: *weapon_entity
            .get::<Weapon>()
            .expect("the weapon should retain its authored configuration"),
        cooldown: *weapon_entity
            .get::<WeaponCooldown>()
            .expect("the weapon should have a runtime cooldown"),
    };
    NormalizedRunState { actors, weapon }
}

fn spawn_same_shaped_projectile_sentinel(world: &mut World) {
    let sentinel = world.resource::<ProjectileSentinel>();
    if let Some(owned_projectile) = sentinel.owned_projectile {
        if world.get_entity(owned_projectile).is_ok() {
            preserve_owned_projectile(world, owned_projectile);
        }
        return;
    }
    if sentinel.entity.is_some() {
        return;
    }
    let Some((owned_projectile, projectile_id, transform, velocity, damage, lifetime)) =
        world.iter_entities().find_map(|entity| {
            if !entity.contains::<ProjectileRole>() {
                return None;
            }
            Some((
                entity.id(),
                *entity.get::<ProjectileId>()?,
                *entity.get::<Transform2d>()?,
                *entity.get::<Velocity2d>()?,
                *entity.get::<ProjectileDamage>()?,
                *entity.get::<ProjectileLifetime>()?,
            ))
        })
    else {
        return;
    };
    let token =
        spawn_identity_entity(world).expect("the sentinel should use the public identity path");
    world.entity_mut(token.entity()).insert((
        ProjectileRole,
        projectile_id,
        transform,
        velocity,
        damage,
        lifetime,
    ));
    preserve_owned_projectile(world, owned_projectile);
    let mut sentinel = world.resource_mut::<ProjectileSentinel>();
    sentinel.entity = Some(token.entity());
    sentinel.owned_projectile = Some(owned_projectile);
}

fn preserve_owned_projectile(world: &mut World, entity: Entity) {
    world
        .get_mut::<Transform2d>(entity)
        .expect("the retained owned projectile should preserve its transform")
        .translation = Vec2::new(10_000.0, 10_000.0);
    world
        .get_mut::<ProjectileLifetime>(entity)
        .expect("the retained owned projectile should preserve its lifetime")
        .remaining_ticks = u64::MAX;
}

fn spawn_foreign_gameplay_sentinels(world: &mut World) {
    if world
        .resource::<ForeignGameplaySentinels>()
        .player
        .is_some()
    {
        return;
    }
    let player_source = world
        .iter_entities()
        .find_map(|entity| {
            let source = entity.get::<SceneEntitySource>()?;
            (source.entity_id.as_str() == "player").then(|| source.clone())
        })
        .expect("the authored player source must be available");
    let enemy_source = world
        .iter_entities()
        .find_map(|entity| {
            let source = entity.get::<SceneEntitySource>()?;
            (source.entity_id.as_str() == "enemy-anchor/enemy").then(|| source.clone())
        })
        .expect("the authored enemy source must be available");
    let player = world
        .spawn((
            player_source,
            PlayerRole {},
            Health { current: 999 },
            Velocity2d {
                value: Vec2::new(17.0, -19.0),
            },
            Transform2d::from_translation(Vec2::new(400.0, 500.0)),
        ))
        .id();
    let enemy = world
        .spawn((
            enemy_source,
            EnemyRole {},
            Health { current: 0 },
            Velocity2d {
                value: Vec2::new(-23.0, 29.0),
            },
            Transform2d::from_translation(Vec2::new(-400.0, -500.0)),
            WaveSpawn { tick: 0 },
        ))
        .id();
    let mut sentinels = world.resource_mut::<ForeignGameplaySentinels>();
    sentinels.player = Some(player);
    sentinels.enemy = Some(enemy);
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
            entity.contains::<PlayerRole>()
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
             players: Query<(Entity, &SceneEntitySource), With<PlayerRole>>,
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
    if world.query::<&EnemyRole>().iter(world).next().is_some() {
        return;
    }
    let stats = world.resource::<WorldIdentityDomain>().stats();
    assert_eq!(stats.active_scene_entities, 5);
    assert!(stats.recent_tombstones >= 3);
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

fn run_initialization_audit_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<RunInitializationAuditPlugin>()
}

fn projectile_sentinel_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ProjectileSentinelPlugin>()
}

fn foreign_gameplay_sentinel_plugin() -> PluginDefinition {
    PluginDefinition::for_default::<ForeignGameplaySentinelPlugin>()
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
    assert_eq!(first.tick, 50);
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
    let intent =
        advanced_wave_headless_intent_after::<ReferenceWavePlugin>(maximum, tie_setup_plugin());
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
fn startup_and_retry_publish_the_same_normalized_authored_and_runtime_state() {
    let mut runtime = adapter_retry_runtime_with_test_plugin(run_initialization_audit_plugin());
    drive_adapter_fixed(&mut runtime);
    let startup = runtime
        .world()
        .resource::<RunInitializationAudit>()
        .generations
        .get(&1)
        .expect("Startup should publish the first normalized run")
        .clone();

    let player = startup
        .actors
        .iter()
        .find(|actor| actor.role == ActorRole::Player)
        .expect("the normalized run should contain its player");
    assert_eq!(player.scene_id, "player");
    assert_eq!(player.transform, Transform2d::IDENTITY);
    assert_eq!(player.initial_health.hit_points, 20);
    assert_eq!(player.health.current, 20);
    assert_eq!(player.initial_velocity.velocity, Vec2::ZERO);
    assert_eq!(player.velocity.value, Vec2::ZERO);
    assert_eq!(player.wave_spawn, None);
    assert_eq!(startup.weapon.scene_id, "player-weapon");
    assert_eq!(startup.weapon.parent_scene_id, "player");
    assert_eq!(
        startup.weapon.transform,
        Transform2d::from_translation(Vec2::new(1.2, 0.0))
    );
    assert_eq!(startup.weapon.weapon, Weapon::fixture());
    assert_eq!(startup.weapon.cooldown.remaining_ticks, 3);
    assert_eq!(
        startup
            .actors
            .iter()
            .filter(|actor| actor.role == ActorRole::Enemy)
            .map(|actor| (actor.scene_id.as_str(), actor.transform.translation.x))
            .collect::<Vec<_>>(),
        [
            ("enemy-anchor-2/enemy", 9.0),
            ("enemy-anchor-3/enemy", 13.0),
            ("enemy-anchor/enemy", 5.0),
        ]
    );

    drive_adapter_to_terminal(&mut runtime);
    let (old_player, old_reference) = player_scene_identity(runtime.world());
    submit_retry_for_next_tick(&mut runtime, 2);
    drive_adapter_to_run_generation(&mut runtime, 2);

    let retry = runtime
        .world()
        .resource::<RunInitializationAudit>()
        .generations
        .get(&2)
        .expect("Retry should publish the next normalized run")
        .clone();
    assert_eq!(retry, startup);
    let (new_player, new_reference) = player_scene_identity(runtime.world());
    assert_ne!(new_player, old_player);
    assert_ne!(new_reference, old_reference);
    let domain = runtime.world().resource::<WorldIdentityDomain>();
    assert!(matches!(
        domain.lookup(runtime.world(), &old_reference),
        EntityLookup::Tombstoned(Some(tombstone))
            if tombstone.cause() == TombstoneCause::Replaced
    ));
    assert_eq!(
        domain.lookup(runtime.world(), &new_reference),
        EntityLookup::Resolved(new_player)
    );
    stop_adapter_retry_runtime(runtime);
}

#[test]
fn invalid_authored_weapon_configuration_rejects_startup_before_runtime_publication() {
    let (project, plan, root) = headless_wave_candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&project, &plan).unwrap();
    let mut scene = snapshot.expanded_startup_scene().clone();
    let weapon_id = ComponentTypeId::new("reference_game.Weapon");
    let weapon = scene
        .entities
        .iter_mut()
        .find(|entity| entity.id.as_str() == "player-weapon")
        .and_then(|entity| entity.components.get_mut(&weapon_id))
        .expect("the startup fixture should expose its authored weapon");
    let ComponentValue::Map(fields) = &mut weapon.value else {
        panic!("the authored Weapon value should remain a map");
    };
    fields.insert("damage".to_owned(), ComponentValue::I64(0));

    assert_startup_rejected(plan, scene);
}

#[test]
fn missing_authored_weapon_sprite_rejects_startup_before_runtime_publication() {
    let (project, plan, root) = headless_wave_candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&project, &plan).unwrap();
    let mut scene = snapshot.expanded_startup_scene().clone();
    let weapon = scene
        .entities
        .iter_mut()
        .find(|entity| entity.id.as_str() == "player-weapon")
        .expect("the startup fixture should expose its authored weapon");
    assert!(
        weapon
            .components
            .remove(&ComponentTypeId::new("nara.sprite.Sprite"))
            .is_some(),
        "the authored weapon fixture should carry its Sprite"
    );

    assert_startup_rejected(plan, scene);
}

fn assert_startup_rejected(
    plan: nara::project_host::RuntimePlan,
    scene: nara::scene::SceneDocument,
) {
    let candidate = admit_adapter_candidate(plan, scene);
    let mut failure = match candidate.complete_startup() {
        Err(failure) => failure,
        Ok(candidate) => {
            let mut retirement = candidate.begin_retirement();
            for _ in 0..64 {
                if retirement.drive_retirement() == RuntimeCandidateRetirementState::Retired {
                    break;
                }
            }
            assert_eq!(
                retirement.retirement_state(),
                RuntimeCandidateRetirementState::Retired,
                "the unexpectedly ready candidate should retire before the assertion fails"
            );
            panic!("invalid authored startup content must reject Startup")
        }
    };
    assert_eq!(failure.fault().kind(), RuntimeFaultKind::System);
    for _ in 0..64 {
        if failure.retirement_state() == RuntimeCandidateRetirementState::Retired {
            return;
        }
        let _ = failure.drive_retirement();
    }
    panic!("rejected invalid-authored Startup did not retire its unpublished candidate");
}

#[test]
fn retry_retires_only_current_generation_projectiles_and_preserves_same_shaped_sentinel() {
    let mut runtime = adapter_retry_runtime_with_test_plugin(projectile_sentinel_plugin());
    let sentinel = (0..32)
        .find_map(|_| {
            drive_adapter_fixed(&mut runtime);
            runtime.world().resource::<ProjectileSentinel>().entity
        })
        .expect("the running wave should create a projectile-shaped sentinel");
    let owned_projectile = runtime
        .world()
        .resource::<ProjectileSentinel>()
        .owned_projectile
        .expect("the sentinel fixture should retain one exact owner-tracked projectile");
    drive_adapter_to_terminal(&mut runtime);
    assert!(
        runtime.world().get_entity(owned_projectile).is_ok(),
        "the fixture must retain its exact owner-tracked projectile until Retry"
    );

    submit_retry_for_next_tick(&mut runtime, 2);
    drive_adapter_to_run_generation(&mut runtime, 2);

    let sentinel_ref = runtime
        .world()
        .get_entity(sentinel)
        .expect("an unrelated same-shaped sentinel must survive Retry");
    assert!(sentinel_ref.contains::<ProjectileRole>());
    assert!(sentinel_ref.contains::<ProjectileId>());
    assert!(sentinel_ref.contains::<Transform2d>());
    assert!(sentinel_ref.contains::<Velocity2d>());
    assert!(sentinel_ref.contains::<ProjectileDamage>());
    assert!(sentinel_ref.contains::<ProjectileLifetime>());
    assert!(
        runtime.world().get_entity(owned_projectile).is_err(),
        "Retry should retire the exact current-generation projectile token"
    );
    stop_adapter_retry_runtime(runtime);
}

#[test]
fn current_receipt_excludes_foreign_same_shaped_actors_from_simulation_and_snapshots() {
    let mut runtime = adapter_retry_runtime_with_test_plugin(foreign_gameplay_sentinel_plugin());
    runtime
        .with_driver_scope(|scope| {
            submit_gameplay_driver_command(
                scope,
                movement_command(1, 1, MovementDirection::Left).unwrap(),
            )
        })
        .unwrap()
        .unwrap()
        .unwrap();
    drive_adapter_fixed(&mut runtime);

    let (foreign_player, foreign_enemy) = {
        let sentinels = runtime.world().resource::<ForeignGameplaySentinels>();
        (
            sentinels
                .player
                .expect("the foreign player sentinel should be installed"),
            sentinels
                .enemy
                .expect("the foreign enemy sentinel should be installed"),
        )
    };
    assert_eq!(
        runtime
            .world()
            .get::<Transform2d>(foreign_player)
            .expect("the foreign player sentinel should survive")
            .translation,
        Vec2::new(400.0, 500.0),
    );
    assert_eq!(
        runtime
            .world()
            .get::<Transform2d>(foreign_enemy)
            .expect("the foreign enemy sentinel should survive")
            .translation,
        Vec2::new(-400.0, -500.0),
    );
    let first_snapshot = runtime.world().resource::<WaveSnapshot>();
    assert_eq!(first_snapshot.player.id, "player");
    assert_eq!(first_snapshot.planned_enemies, 3);
    assert_eq!(first_snapshot.enemies.len(), 3);
    assert_eq!(first_snapshot.score, 0);

    drive_adapter_to_terminal(&mut runtime);

    let terminal = runtime.world().resource::<WaveSnapshot>();
    assert_eq!(terminal.outcome, WaveOutcome::Completed);
    assert_eq!(terminal.planned_enemies, 3);
    assert_eq!(terminal.defeated_enemies, 3);
    assert_eq!(terminal.score, 300);
    assert!(runtime.world().get_entity(foreign_player).is_ok());
    assert!(runtime.world().get_entity(foreign_enemy).is_ok());
    stop_adapter_retry_runtime(runtime);
}

#[test]
fn semantic_retry_resets_the_wave_without_a_platform_or_runtime_replacement() {
    let mut runtime = adapter_retry_runtime();
    let runtime_generation = runtime.generation();

    drive_adapter_to_terminal(&mut runtime);
    assert!(runtime.world().resource::<WaveSnapshot>().is_terminal());
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
fn retry_replaces_corrupted_runtime_component_topology_from_retained_source() {
    let mut runtime = adapter_retry_runtime_with_test_plugin(retry_identity_corruption_plugin());
    drive_adapter_fixed(&mut runtime);
    let (old_entity, old_reference) = player_scene_identity(runtime.world());
    drive_adapter_to_terminal(&mut runtime);
    let corrupted_player = runtime
        .world()
        .get_entity(old_entity)
        .expect("the corrupted live player should remain allocated before Retry");
    assert!(!corrupted_player.contains::<SceneEntitySource>());
    assert!(corrupted_player.contains::<ProjectileRole>());
    assert_eq!(
        runtime
            .world()
            .resource::<WorldIdentityDomain>()
            .lookup(runtime.world(), &old_reference),
        EntityLookup::Resolved(old_entity)
    );

    submit_retry_for_next_tick(&mut runtime, 2);
    drive_adapter_to_run_generation(&mut runtime, 2);

    let (new_entity, new_reference) = player_scene_identity(runtime.world());
    assert_ne!(new_entity, old_entity);
    assert_ne!(new_reference, old_reference);
    let replacement_player = runtime
        .world()
        .get_entity(new_entity)
        .expect("Retry should publish a replacement player");
    assert!(replacement_player.contains::<PlayerRole>());
    assert!(replacement_player.contains::<SceneEntitySource>());
    assert!(!replacement_player.contains::<ProjectileRole>());
    assert_eq!(runtime.world().resource::<WaveRunGeneration>().get(), 2);
    let retry = runtime.world().resource::<WaveRetryStatus>();
    assert_eq!(retry.phase(), WaveRetryPhase::Applied);
    assert_eq!(retry.pending_generation(), None);
    assert_eq!(retry.applied_generation(), Some(2));
    assert_eq!(retry.last_rejection(), None);
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

    drive_adapter_fixed(&mut runtime);

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
    assert_eq!(retry.phase(), WaveRetryPhase::Idle);
    assert_eq!(retry.pending_generation(), None);
    assert_eq!(retry.applied_generation(), None);
    assert_eq!(
        retry.last_rejection(),
        Some(WaveRetryRejection::ReplacementRejected)
    );

    drive_adapter_fixed(&mut runtime);
    assert!(
        runtime
            .world()
            .resource::<RetryCandidateObserver>()
            .repaired
    );
    assert_eq!(runtime.world().resource::<WaveRunGeneration>().get(), 1);
    assert_eq!(
        runtime
            .world()
            .resource::<WorldIdentityDomain>()
            .lookup(runtime.world(), &old_reference),
        EntityLookup::Resolved(old_entity)
    );
    submit_retry_for_next_tick(&mut runtime, 3);
    drive_adapter_to_run_generation(&mut runtime, 2);
    let retry = runtime.world().resource::<WaveRetryStatus>();
    assert_eq!(retry.phase(), WaveRetryPhase::Applied);
    assert_eq!(retry.applied_generation(), Some(2));
    assert_eq!(retry.last_rejection(), None);
    assert!(matches!(
        runtime
            .world()
            .resource::<WorldIdentityDomain>()
            .lookup(runtime.world(), &old_reference),
        EntityLookup::Tombstoned(Some(tombstone))
            if tombstone.cause() == TombstoneCause::Replaced
    ));
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

    drive_adapter_fixed(&mut runtime);

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
    assert_eq!(retry.phase(), WaveRetryPhase::Idle);
    assert_eq!(retry.pending_generation(), None);
    assert_eq!(retry.applied_generation(), None);
    assert_eq!(
        retry.last_rejection(),
        Some(WaveRetryRejection::ReplacementRejected)
    );
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
    admit_adapter_candidate(plan, scene)
        .complete_startup()
        .unwrap()
        .promote()
}

fn admit_adapter_candidate(
    plan: nara::project_host::RuntimePlan,
    scene: nara::scene::SceneDocument,
) -> nara::app::RuntimeCandidate {
    let runtime_time = plan.settings().runtime.runtime_time_settings();
    let fixed_time = plan.settings().runtime.fixed_time();
    let source = StartupSceneSource::direct(
        Arc::new(scene),
        ByteLimit::new(16 * 1024 * 1024).expect("the direct retained-scene limit is non-zero"),
    )
    .expect("the reference scene should fit the bounded direct retention limit");
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
                world.insert_resource(runtime_time);
                world.insert_resource(fixed_time);
                let report = materialize_startup_scene(world, source)
                    .expect("the retained startup scene should materialize");
                assert!(!report.has_errors(), "{report:#?}");
            });
        })
        .unwrap();
    candidate
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
            (entity.contains::<PlayerRole>() && source.entity_id.as_str() == "player").then(|| {
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
