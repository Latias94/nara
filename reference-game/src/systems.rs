use std::{collections::BTreeMap, error::Error, fmt};

use nara::ecs as bevy_ecs;
use nara::{
    advanced_prelude::{
        SceneProductOverlayWriter, SceneProductTransactionLimits, StartupSceneActivation,
        replace_scene_with_product,
    },
    app::RuntimeFaultReporter,
    core::ItemLimit,
    ecs::{Entity, Mut, With, Without, error::BevyError, system::SystemParam},
    gameplay::{GameplayCommandBatch, GameplayCommandValue},
    hierarchy::Parent,
    identity::{EntityLookup, SpawnedSceneInstance, spawn_identity_entity},
    prelude::{
        Commands, ComponentRegistry, ComponentSchema, ComponentTypeId, FixedTime,
        PersistentComponentProvider, Query, Res, ResMut, SceneEntityId, Transform2d, Vec2, World,
    },
    reflect::component_registry,
    scene::{SceneDocument, SceneEntityRecord, SceneEntitySource, retire_and_despawn_scene_entity},
    transform::GlobalTransform2d,
};

use crate::{
    EnemyRole, Health, InitialHealth, InitialVelocity2d, PlayerRole, ProjectileDamage,
    ProjectileLifetime, ProjectileRole, ReferenceProjectSnapshot, RuntimeOnlyTag, Velocity2d,
    WaveSpawn, Weapon, WeaponCooldown, migrate_weapon_v1_to_v2,
    resources::{
        ENEMY_CONTACT_DAMAGE, KILL_SCORE, MAX_WAVE_ENEMIES, MAX_WAVE_PROJECTILES,
        MAX_WAVE_SCENE_ENTITIES, MOVE_COMMAND_TYPE, MOVE_PRESSED_FIELD, MOVE_X_FIELD, MOVE_Y_FIELD,
        MovementDirection, MovementIntent, PLAYER_SCENE_ID, PLAYER_WEAPON_SCENE_ID,
        PROJECTILE_TTL_TICKS, ProjectileId, RETRY_COMMAND_TYPE, WaveRetryRejection,
        WaveRetryStatus, WaveRunBaseline, WaveRunGeneration, WaveRunOwner, WaveState,
    },
    snapshot::{WaveOutcome, WaveSnapshot},
};

struct PreparedReferenceRun {
    actors: Vec<PreparedActorRuntime>,
    weapon: PreparedWeaponRuntime,
    baseline: WaveRunBaseline,
}

struct PreparedActorRuntime {
    id: SceneEntityId,
    health: Health,
    velocity: Velocity2d,
}

struct PreparedWeaponRuntime {
    id: SceneEntityId,
    cooldown: WeaponCooldown,
}

enum RunPublication {
    Startup,
    Retry {
        last_rejection: Option<WaveRetryRejection>,
    },
}

impl PreparedReferenceRun {
    fn write_overlay(&self, writer: &mut SceneProductOverlayWriter<'_>) {
        for actor in &self.actors {
            writer
                .insert_component(actor.id.clone(), actor.health)
                .insert_component(actor.id.clone(), actor.velocity);
        }
        writer.insert_component(self.weapon.id.clone(), self.weapon.cooldown);
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the Bevy system signature is the explicit startup access contract"
)]
pub(crate) fn initialize_reference_run(
    activation: StartupSceneActivation<'_>,
    mut commands: Commands,
    scene_entities: Query<(Entity, &SceneEntitySource)>,
    mut movement: ResMut<MovementIntent>,
    mut generation: ResMut<WaveRunGeneration>,
    mut retry: ResMut<WaveRetryStatus>,
    mut state: ResMut<WaveState>,
    mut snapshot: ResMut<WaveSnapshot>,
) -> Result<(), BevyError> {
    let prepared = prepare_reference_run(
        activation.source(),
        WaveRunGeneration::default(),
        RunPublication::Startup,
    )
    .map_err(BevyError::error)?;
    let entities = resolve_startup_entities(&activation, &scene_entities)?;

    for actor in &prepared.actors {
        let entity = entities
            .get(&actor.id)
            .copied()
            .ok_or_else(|| BevyError::error(ReferenceSimulationError::SceneMembershipMismatch))?;
        commands
            .entity(entity)
            .insert((actor.health, actor.velocity));
    }
    let weapon = entities
        .get(&prepared.weapon.id)
        .copied()
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::SceneMembershipMismatch))?;
    commands.entity(weapon).insert(prepared.weapon.cooldown);

    *movement = prepared.baseline.movement;
    *generation = prepared.baseline.generation;
    *retry = prepared.baseline.retry;
    *state = prepared.baseline.state.clone();
    *snapshot = WaveSnapshot::default();
    commands.insert_resource(WaveRunOwner::new(
        activation.source_view(),
        activation.receipt().clone(),
        entities,
        prepared.baseline,
    ));
    Ok(())
}

fn resolve_startup_entities(
    activation: &StartupSceneActivation<'_>,
    scene_entities: &Query<(Entity, &SceneEntitySource)>,
) -> Result<BTreeMap<SceneEntityId, Entity>, BevyError> {
    let mut entities = BTreeMap::new();
    for (entity, source) in scene_entities {
        if source.instance_id != activation.receipt().instance_id() {
            continue;
        }
        if entities.insert(source.entity_id.clone(), entity).is_some() {
            return Err(BevyError::error(
                ReferenceSimulationError::SceneMembershipMismatch,
            ));
        }
    }
    if !entities.keys().eq(activation.receipt().entity_ids().iter()) {
        return Err(BevyError::error(
            ReferenceSimulationError::SceneMembershipMismatch,
        ));
    }
    Ok(entities)
}

fn resolve_published_entities(
    world: &World,
    receipt: &SpawnedSceneInstance,
) -> Result<BTreeMap<SceneEntityId, Entity>, ReferenceSimulationError> {
    receipt
        .entity_ids()
        .iter()
        .map(|entity_id| {
            let EntityLookup::Resolved(entity) = receipt.resolve(world, entity_id) else {
                return Err(ReferenceSimulationError::SceneMembershipMismatch);
            };
            let source = world
                .get::<SceneEntitySource>(entity)
                .ok_or(ReferenceSimulationError::SceneMembershipMismatch)?;
            if source.instance_id != receipt.instance_id() || source.entity_id != *entity_id {
                return Err(ReferenceSimulationError::SceneMembershipMismatch);
            }
            Ok((entity_id.clone(), entity))
        })
        .collect()
}

fn prepare_reference_run(
    source: &SceneDocument,
    generation: WaveRunGeneration,
    publication: RunPublication,
) -> Result<PreparedReferenceRun, ReferenceSimulationError> {
    if source.entities.len() > MAX_WAVE_SCENE_ENTITIES {
        return Err(ReferenceSimulationError::TooManySceneEntities);
    }

    let mut actors = Vec::new();
    let mut player_count = 0_usize;
    let mut enemy_count = 0_usize;
    let mut weapon = None;
    let player_role_schema = PlayerRole::persistent_component_schema();
    let enemy_role_schema = EnemyRole::persistent_component_schema();
    let initial_health_schema = InitialHealth::persistent_component_schema();
    let initial_velocity_schema = InitialVelocity2d::persistent_component_schema();
    let wave_spawn_schema = WaveSpawn::persistent_component_schema();
    let weapon_schema = Weapon::persistent_component_schema();
    let transform_type = ComponentTypeId::new("nara.transform.Transform2d");
    let sprite_type = ComponentTypeId::new("nara.sprite.Sprite");
    for entity in &source.entities {
        let player = decode_current_component::<PlayerRole>(entity, &player_role_schema)?;
        let enemy = decode_current_component::<EnemyRole>(entity, &enemy_role_schema)?;
        let initial_health =
            decode_current_component::<InitialHealth>(entity, &initial_health_schema)?;
        let initial_velocity =
            decode_current_component::<InitialVelocity2d>(entity, &initial_velocity_schema)?;
        let wave_spawn = decode_current_component::<WaveSpawn>(entity, &wave_spawn_schema)?;
        let weapon_config = decode_weapon(entity, &weapon_schema)?;
        let has_transform = entity.components.contains_key(&transform_type);
        let has_sprite = entity.components.contains_key(&sprite_type);

        if player.is_some() && enemy.is_some() {
            return Err(ReferenceSimulationError::InvalidAuthoredConfiguration);
        }
        if player.is_some() || enemy.is_some() {
            let initial_health =
                initial_health.ok_or(ReferenceSimulationError::InvalidAuthoredConfiguration)?;
            let initial_velocity =
                initial_velocity.ok_or(ReferenceSimulationError::InvalidAuthoredConfiguration)?;
            if !has_transform {
                return Err(ReferenceSimulationError::InvalidSpatialHierarchy);
            }
            if !has_sprite {
                return Err(ReferenceSimulationError::InvalidAuthoredConfiguration);
            }
            if player.is_some() {
                player_count = player_count
                    .checked_add(1)
                    .ok_or(ReferenceSimulationError::TooManySceneEntities)?;
                if entity.id.as_str() != PLAYER_SCENE_ID || wave_spawn.is_some() {
                    return Err(ReferenceSimulationError::UnexpectedPlayerIdentity);
                }
            } else {
                enemy_count = enemy_count
                    .checked_add(1)
                    .ok_or(ReferenceSimulationError::TooManyEnemies)?;
                if enemy_count > MAX_WAVE_ENEMIES || wave_spawn.is_none() {
                    return Err(ReferenceSimulationError::MissingWaveSpawn);
                }
            }
            actors.push(PreparedActorRuntime {
                id: entity.id.clone(),
                health: Health {
                    current: initial_health.hit_points,
                },
                velocity: Velocity2d {
                    value: initial_velocity.velocity,
                },
            });
        } else if initial_health.is_some() || initial_velocity.is_some() || wave_spawn.is_some() {
            return Err(ReferenceSimulationError::InvalidAuthoredConfiguration);
        }

        if let Some(config) = weapon_config {
            if weapon.is_some()
                || entity.id.as_str() != PLAYER_WEAPON_SCENE_ID
                || entity
                    .parent
                    .as_ref()
                    .is_none_or(|parent| parent.as_str() != PLAYER_SCENE_ID)
                || !has_transform
                || !has_sprite
            {
                return Err(ReferenceSimulationError::MissingPlayerWeapon);
            }
            if config.cooldown_ticks == 0 || config.damage <= 0 {
                return Err(ReferenceSimulationError::InvalidAuthoredConfiguration);
            }
            weapon = Some(PreparedWeaponRuntime {
                id: entity.id.clone(),
                cooldown: WeaponCooldown {
                    remaining_ticks: config.cooldown_ticks,
                },
            });
        }
    }
    if player_count != 1 {
        return Err(if player_count == 0 {
            ReferenceSimulationError::MissingPlayer
        } else {
            ReferenceSimulationError::DuplicatePlayer
        });
    }
    if enemy_count == 0 {
        return Err(ReferenceSimulationError::MissingEnemy);
    }
    let weapon = weapon.ok_or(ReferenceSimulationError::MissingPlayerWeapon)?;
    actors.sort_by(|left, right| left.id.cmp(&right.id));

    let mut retry = WaveRetryStatus::default();
    if let RunPublication::Retry { last_rejection } = publication {
        retry.mark_applied(generation.get(), last_rejection);
    }
    Ok(PreparedReferenceRun {
        actors,
        weapon,
        baseline: WaveRunBaseline {
            movement: MovementIntent::default(),
            generation,
            retry,
            state: WaveState::default(),
        },
    })
}

fn decode_current_component<T>(
    entity: &SceneEntityRecord,
    schema: &ComponentSchema,
) -> Result<Option<T>, ReferenceSimulationError>
where
    T: PersistentComponentProvider,
{
    let Some(record) = entity.components.get(schema.id()) else {
        return Ok(None);
    };
    if record.version != schema.version() {
        return Err(ReferenceSimulationError::InvalidAuthoredConfiguration);
    }
    T::__decode_persistent_component(&record.value)
        .map(Some)
        .map_err(|_| ReferenceSimulationError::InvalidAuthoredConfiguration)
}

fn decode_weapon(
    entity: &SceneEntityRecord,
    schema: &ComponentSchema,
) -> Result<Option<Weapon>, ReferenceSimulationError> {
    let Some(record) = entity.components.get(schema.id()) else {
        return Ok(None);
    };
    let decoded = match record.version.get() {
        1 => migrate_weapon_v1_to_v2(record.value.clone())
            .map_err(|_| ReferenceSimulationError::InvalidAuthoredConfiguration)
            .and_then(|value| {
                Weapon::__decode_persistent_component(&value)
                    .map_err(|_| ReferenceSimulationError::InvalidAuthoredConfiguration)
            }),
        2 => Weapon::__decode_persistent_component(&record.value)
            .map_err(|_| ReferenceSimulationError::InvalidAuthoredConfiguration),
        _ => return Err(ReferenceSimulationError::InvalidAuthoredConfiguration),
    };
    decoded.map(Some)
}

#[allow(
    clippy::type_complexity,
    reason = "the query signature keeps the ECS read/write contract visible at the system boundary"
)]
pub(crate) fn move_project_players(
    owner: Res<WaveRunOwner>,
    mut players: Query<
        (Entity, &SceneEntitySource, &Velocity2d, &mut Transform2d),
        (With<PlayerRole>, With<SceneEntitySource>),
    >,
) {
    for (entity, source, velocity, mut transform) in &mut players {
        if !owner.owns_scene_entity(entity, source) {
            continue;
        }
        transform.translation += velocity.value;
    }
}

#[allow(
    clippy::type_complexity,
    reason = "the query signature keeps the ECS read/write contract visible at the system boundary"
)]
pub(crate) fn move_project_enemies(
    owner: Res<WaveRunOwner>,
    mut enemies: Query<
        (Entity, &SceneEntitySource, &Velocity2d, &mut Transform2d),
        (With<EnemyRole>, With<SceneEntitySource>),
    >,
) {
    for (entity, source, velocity, mut transform) in &mut enemies {
        if !owner.owns_scene_entity(entity, source) {
            continue;
        }
        transform.translation += velocity.value;
    }
}

#[allow(
    clippy::type_complexity,
    reason = "the query signature keeps the ECS read/write contract visible at the system boundary"
)]
pub(crate) fn tick_project_weapons(
    owner: Res<WaveRunOwner>,
    mut weapons: Query<
        (Entity, &SceneEntitySource, &mut WeaponCooldown),
        (With<Weapon>, With<SceneEntitySource>),
    >,
) {
    for (entity, source, mut cooldown) in &mut weapons {
        if !owner.owns_scene_entity(entity, source) {
            continue;
        }
        cooldown.remaining_ticks = cooldown.remaining_ticks.saturating_sub(1);
    }
}

pub(crate) fn begin_wave_tick(world: &mut World) -> Result<(), BevyError> {
    let generation = world.resource::<WaveRunGeneration>().get();
    let reset_applied =
        if world.resource::<WaveRetryStatus>().pending_generation() == Some(generation) {
            apply_wave_reset(world)?
        } else {
            false
        };
    if reset_applied {
        world
            .resource_mut::<WaveState>()
            .begin_projection_only_tick();
    } else {
        world.resource_mut::<WaveState>().begin_tick();
    }
    Ok(())
}

pub(crate) fn consume_retry_commands(world: &mut World) -> Result<(), BevyError> {
    let retry_count = world
        .resource::<GameplayCommandBatch>()
        .commands()
        .iter()
        .filter(|command| command.command_type().as_str() == RETRY_COMMAND_TYPE)
        .count();
    if retry_count == 0 {
        return Ok(());
    }

    let generation = world.resource::<WaveRunGeneration>().get();
    if world.resource::<WaveState>().is_running() {
        world
            .resource_mut::<WaveRetryStatus>()
            .reject_while_running();
        return Ok(());
    }
    let mut status = world.resource_mut::<WaveRetryStatus>();
    if status.mark_pending(generation) && retry_count > 1 {
        status.reject_duplicate();
    }

    Ok(())
}

fn apply_wave_reset(world: &mut World) -> Result<bool, BevyError> {
    let fixed_tick = world.resource::<FixedTime>().tick();
    let mut next_generation = *world.resource::<WaveRunGeneration>();
    if next_generation.advance_for_reset(fixed_tick).is_none() {
        world
            .resource_mut::<WaveRetryStatus>()
            .reject(WaveRetryRejection::GenerationExhausted);
        return Ok(false);
    }

    let registry = component_registry(world)
        .and_then(|registry| registry.snapshot().ok())
        .map(ComponentRegistry::from_snapshot)
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::ComponentRegistryUnavailable))?;

    world.resource_scope(|world, mut owner: Mut<WaveRunOwner>| {
        let last_rejection = world.resource::<WaveRetryStatus>().last_rejection();
        let replacement = owner.source().with_document(|source| {
            let prepared = prepare_reference_run(
                source,
                next_generation,
                RunPublication::Retry { last_rejection },
            )?;
            let limits = SceneProductTransactionLimits::new(
                ItemLimit::new(MAX_WAVE_SCENE_ENTITIES.saturating_mul(3))
                    .expect("the reference overlay limit is non-zero"),
                ItemLimit::new(MAX_WAVE_PROJECTILES)
                    .expect("the reference projectile limit is non-zero"),
            );
            let retirements = owner.projectile_tokens().to_vec();
            let baseline = prepared.baseline.clone();
            let report = replace_scene_with_product(
                world,
                &registry,
                source,
                owner.receipt(),
                limits,
                &retirements,
                |writer| {
                    prepared.write_overlay(writer);
                    writer
                        .replace_resource(baseline.movement)
                        .replace_resource(baseline.generation)
                        .replace_resource(baseline.retry)
                        .replace_resource(baseline.state.clone())
                        .replace_resource(WaveSnapshot::default());
                },
            );
            Ok::<_, ReferenceSimulationError>((prepared, report))
        });
        let Some(replacement) = replacement else {
            world
                .resource_mut::<WaveRetryStatus>()
                .reject(WaveRetryRejection::InvalidAuthoredConfiguration);
            return Ok(false);
        };
        let (prepared, mut report) = match replacement {
            Ok(replacement) => replacement,
            Err(_) => {
                world
                    .resource_mut::<WaveRetryStatus>()
                    .reject(WaveRetryRejection::InvalidAuthoredConfiguration);
                return Ok(false);
            }
        };
        if report.diagnostics.has_errors() {
            world
                .resource_mut::<WaveRetryStatus>()
                .reject(WaveRetryRejection::ReplacementRejected);
            return Ok(false);
        }
        let receipt = report
            .instance
            .take()
            .expect("successful product replacement publishes one scene receipt");
        let scene_entities = resolve_published_entities(world, &receipt)
            .expect("successful product replacement must publish exact scene membership");
        owner.publish_replacement(receipt, scene_entities, prepared.baseline);
        Ok(true)
    })
}

pub(crate) fn consume_movement_commands(
    batch: Res<GameplayCommandBatch>,
    owner: Res<WaveRunOwner>,
    mut state: ResMut<WaveState>,
    mut movement: ResMut<MovementIntent>,
    mut players: Query<(Entity, &SceneEntitySource, &mut Velocity2d), With<PlayerRole>>,
) -> Result<(), BevyError> {
    if !state.is_running() {
        return Ok(());
    }

    let result = (|| {
        let mut next_movement = *movement;
        let mut changed = false;
        for command in batch.commands() {
            if command.command_type().as_str() != MOVE_COMMAND_TYPE {
                continue;
            }
            let command = movement_intent(command.payload())?;
            next_movement.apply(command.direction, command.pressed);
            changed = true;
        }
        if !changed {
            return Ok(());
        }

        let mut player = None;
        for (entity, source, candidate) in &mut players {
            if !owner.owns_scene_entity(entity, source)
                || source.entity_id.as_str() != PLAYER_SCENE_ID
            {
                continue;
            }
            if player.replace(candidate).is_some() {
                return Err(BevyError::error(ReferenceSimulationError::DuplicatePlayer));
            }
        }
        let Some(mut velocity) = player else {
            return Err(BevyError::error(ReferenceSimulationError::MissingPlayer));
        };
        velocity.value = next_movement.velocity();
        *movement = next_movement;
        Ok(())
    })();
    state.reject_on_error(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MovementIntentCommand {
    direction: MovementDirection,
    pressed: bool,
}

fn movement_intent(
    payload: &nara::gameplay::GameplayCommandPayload,
) -> Result<MovementIntentCommand, BevyError> {
    if payload.len() != 3 {
        return Err(BevyError::error(
            ReferenceSimulationError::InvalidMovementCommand,
        ));
    }
    let (
        Some(GameplayCommandValue::I64(x)),
        Some(GameplayCommandValue::I64(y)),
        Some(GameplayCommandValue::Bool(pressed)),
    ) = (
        payload.get(MOVE_X_FIELD),
        payload.get(MOVE_Y_FIELD),
        payload.get(MOVE_PRESSED_FIELD),
    )
    else {
        return Err(BevyError::error(
            ReferenceSimulationError::InvalidMovementCommand,
        ));
    };
    let direction = MovementDirection::from_velocity(*x, *y)
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::InvalidMovementCommand))?;
    Ok(MovementIntentCommand {
        direction,
        pressed: *pressed,
    })
}

#[allow(
    clippy::type_complexity,
    reason = "the topology queries intentionally expose their complete validation contract"
)]
#[derive(SystemParam)]
pub(crate) struct WaveTopologyQueries<'w, 's> {
    enemies: Query<
        'w,
        's,
        (
            Entity,
            &'static SceneEntitySource,
            Option<&'static WaveSpawn>,
            Option<&'static Health>,
            Option<&'static Velocity2d>,
            Option<&'static Transform2d>,
            Option<&'static GlobalTransform2d>,
        ),
        With<EnemyRole>,
    >,
    players: Query<
        'w,
        's,
        (
            Entity,
            &'static SceneEntitySource,
            Option<&'static Health>,
            Option<&'static Velocity2d>,
            Option<&'static Transform2d>,
            Option<&'static GlobalTransform2d>,
        ),
        With<PlayerRole>,
    >,
    weapons: Query<
        'w,
        's,
        (
            Entity,
            &'static SceneEntitySource,
            &'static Parent,
            Option<&'static WeaponCooldown>,
            Option<&'static Transform2d>,
        ),
        With<Weapon>,
    >,
    projectiles: Query<
        'w,
        's,
        (
            &'static ProjectileRole,
            &'static ProjectileId,
            &'static Velocity2d,
            &'static ProjectileDamage,
            &'static ProjectileLifetime,
            &'static Transform2d,
        ),
        Without<SceneEntitySource>,
    >,
}

pub(crate) fn validate_wave_topology(
    faults: Res<RuntimeFaultReporter>,
    owner: Res<WaveRunOwner>,
    mut state: ResMut<WaveState>,
    queries: WaveTopologyQueries<'_, '_>,
) -> Result<(), BevyError> {
    if !state.is_running() {
        return Ok(());
    }
    if faults.fault().is_some() {
        state.reject_tick();
        return Ok(());
    }
    if !state.tick_is_pending() {
        return Ok(());
    }

    let result = (|| {
        let (enemy_count, player_entity) = {
            let mut enemy_count = 0_usize;
            for (entity, source, spawn, health, velocity, transform, global) in &queries.enemies {
                if !owner.owns_scene_entity(entity, source) {
                    continue;
                }
                if spawn.is_none()
                    || health.is_none()
                    || velocity.is_none()
                    || transform.is_none()
                    || global.is_none()
                {
                    return Err(BevyError::error(
                        ReferenceSimulationError::InvalidAuthoredConfiguration,
                    ));
                }
                enemy_count = enemy_count
                    .checked_add(1)
                    .ok_or_else(|| BevyError::error(ReferenceSimulationError::TooManyEnemies))?;
                if enemy_count > MAX_WAVE_ENEMIES {
                    return Err(BevyError::error(ReferenceSimulationError::TooManyEnemies));
                }
            }
            if enemy_count == 0 {
                return Err(BevyError::error(ReferenceSimulationError::MissingEnemy));
            }

            let mut player = None;
            for (entity, source, health, velocity, transform, global) in &queries.players {
                if !owner.owns_scene_entity(entity, source) {
                    continue;
                }
                if source.entity_id.as_str() != PLAYER_SCENE_ID {
                    return Err(BevyError::error(
                        ReferenceSimulationError::UnexpectedPlayerIdentity,
                    ));
                }
                if health.is_none() || velocity.is_none() || transform.is_none() || global.is_none()
                {
                    return Err(BevyError::error(
                        ReferenceSimulationError::InvalidAuthoredConfiguration,
                    ));
                }
                if player.replace(entity).is_some() {
                    return Err(BevyError::error(ReferenceSimulationError::DuplicatePlayer));
                }
            }
            let player =
                player.ok_or_else(|| BevyError::error(ReferenceSimulationError::MissingPlayer))?;
            (enemy_count, player)
        };

        let mut weapon_count = 0_usize;
        for (entity, source, parent, cooldown, transform) in &queries.weapons {
            if !owner.owns_scene_entity(entity, source) {
                continue;
            }
            if source.entity_id.as_str() != PLAYER_WEAPON_SCENE_ID
                || parent.parent() != player_entity
                || cooldown.is_none()
                || transform.is_none()
            {
                return Err(BevyError::error(
                    ReferenceSimulationError::MissingPlayerWeapon,
                ));
            }
            weapon_count = weapon_count.saturating_add(1);
        }
        if weapon_count != 1 {
            return Err(BevyError::error(
                ReferenceSimulationError::MissingPlayerWeapon,
            ));
        }

        if owner.projectile_tokens().len() > MAX_WAVE_PROJECTILES {
            return Err(BevyError::error(
                ReferenceSimulationError::TooManyProjectiles,
            ));
        }
        for token in owner.projectile_tokens() {
            if queries.projectiles.get(token.entity()).is_err() {
                return Err(BevyError::error(
                    ReferenceSimulationError::ProjectileOwnershipMismatch,
                ));
            }
        }

        let enemy_count = u64::try_from(enemy_count)
            .map_err(|_| BevyError::error(ReferenceSimulationError::TooManyEnemies))?;
        if let Some(planned) = state.planned_enemies {
            let observed = enemy_count
                .checked_add(state.defeated_enemies)
                .ok_or_else(|| BevyError::error(ReferenceSimulationError::ProgressOverflow))?;
            if observed != planned {
                return Err(BevyError::error(
                    ReferenceSimulationError::EnemyPopulationChanged,
                ));
            }
        } else {
            state.planned_enemies = Some(enemy_count);
        }
        Ok(())
    })();
    if result.is_ok() {
        state.admit_tick();
    } else {
        state.reject_tick();
    }
    result
}

pub(crate) fn move_scene_players(
    state: Res<WaveState>,
    owner: Res<WaveRunOwner>,
    mut players: Query<
        (Entity, &SceneEntitySource, &Velocity2d, &mut Transform2d),
        With<PlayerRole>,
    >,
) {
    if !state.can_simulate() {
        return;
    }
    for (entity, source, velocity, mut transform) in &mut players {
        if !owner.owns_scene_entity(entity, source) {
            continue;
        }
        transform.translation += velocity.value;
    }
}

#[allow(
    clippy::type_complexity,
    reason = "the query signatures keep the ECS read/write contract visible at the system boundary"
)]
pub(crate) fn pursue_scene_players(
    fixed_time: Res<FixedTime>,
    run: Res<WaveRunGeneration>,
    owner: Res<WaveRunOwner>,
    mut state: ResMut<WaveState>,
    players: Query<
        (Entity, &SceneEntitySource, &GlobalTransform2d),
        (With<PlayerRole>, Without<EnemyRole>),
    >,
    mut enemies: Query<
        (
            Entity,
            &SceneEntitySource,
            &WaveSpawn,
            &Health,
            Option<&Parent>,
            &GlobalTransform2d,
            &mut Velocity2d,
            &mut Transform2d,
        ),
        (With<EnemyRole>, Without<PlayerRole>),
    >,
    globals: Query<&GlobalTransform2d>,
) -> Result<(), BevyError> {
    if !state.can_simulate() {
        return Ok(());
    }
    let run_tick = run
        .run_tick(fixed_time.tick())
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::InvalidRunTick))?;

    let result = (|| {
        for (
            enemy_entity,
            enemy_source,
            spawn,
            health,
            parent,
            enemy_global,
            mut velocity,
            mut transform,
        ) in &mut enemies
        {
            if !owner.owns_scene_entity(enemy_entity, enemy_source)
                || health.current <= 0
                || spawn.tick > run_tick
            {
                continue;
            }
            let mut target_position = None;
            for (player_entity, source, player_global) in &players {
                if !owner.owns_scene_entity(player_entity, source) {
                    continue;
                }
                if target_position
                    .replace(player_global.translation())
                    .is_some()
                {
                    return Err(BevyError::error(
                        ReferenceSimulationError::DuplicateEnemyTarget,
                    ));
                }
            }
            let Some(target_position) = target_position else {
                return Err(BevyError::error(
                    ReferenceSimulationError::MissingEnemyTarget,
                ));
            };
            let offset = target_position - enemy_global.translation();
            let world_velocity =
                Vec2::new(axis_velocity(offset.x, 0.5), axis_velocity(offset.y, 0.5));
            let local_velocity = if let Some(parent) = parent {
                let parent_matrix = globals
                    .get(parent.parent())
                    .map_err(|_| {
                        BevyError::error(ReferenceSimulationError::InvalidSpatialHierarchy)
                    })?
                    .matrix();
                let determinant = parent_matrix.determinant();
                if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
                    return Err(BevyError::error(
                        ReferenceSimulationError::InvalidSpatialHierarchy,
                    ));
                }
                parent_matrix.inverse().transform_vector2(world_velocity)
            } else {
                world_velocity
            };
            if !local_velocity.is_finite() {
                return Err(BevyError::error(
                    ReferenceSimulationError::InvalidSpatialHierarchy,
                ));
            }
            velocity.value = local_velocity;
            transform.translation += velocity.value;
        }
        Ok(())
    })();
    state.reject_on_error(result)
}

pub(crate) fn fire_automatic_weapons(world: &mut World) -> Result<(), BevyError> {
    if !world.resource::<WaveState>().can_simulate() {
        return Ok(());
    }
    let result = fire_automatic_weapons_inner(world);
    world.resource_mut::<WaveState>().reject_on_error(result)
}

fn fire_automatic_weapons_inner(world: &mut World) -> Result<(), BevyError> {
    let run_tick = world
        .resource::<WaveRunGeneration>()
        .run_tick(world.resource::<FixedTime>().tick())
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::InvalidRunTick))?;

    world.resource_scope(|world, mut owner: Mut<WaveRunOwner>| {
        let player = {
            let mut players = world.query_filtered::<
            (Entity, &SceneEntitySource, &GlobalTransform2d),
            With<PlayerRole>,
        >();
            let mut player = None;
            for (entity, source, global) in players.iter(world) {
                if !owner.owns_scene_entity(entity, source)
                    || source.entity_id.as_str() != PLAYER_SCENE_ID
                {
                    continue;
                }
                if player.replace((entity, global.translation())).is_some() {
                    return Err(BevyError::error(ReferenceSimulationError::DuplicatePlayer));
                }
            }
            player.ok_or_else(|| BevyError::error(ReferenceSimulationError::MissingPlayer))?
        };

        let (weapon_entity, weapon, next_remaining) = {
            let mut weapons = world.query_filtered::<(
                Entity,
                &SceneEntitySource,
                &Parent,
                &Weapon,
                &WeaponCooldown,
            ), With<SceneEntitySource>>();
            let mut selected = None;
            for (entity, source, parent, config, cooldown) in weapons.iter(world) {
                if !owner.owns_scene_entity(entity, source) || parent.parent() != player.0 {
                    continue;
                }
                if selected
                    .replace((entity, *config, cooldown.remaining_ticks.saturating_sub(1)))
                    .is_some()
                {
                    return Err(BevyError::error(
                        ReferenceSimulationError::MissingPlayerWeapon,
                    ));
                }
            }
            selected
                .ok_or_else(|| BevyError::error(ReferenceSimulationError::MissingPlayerWeapon))?
        };

        if next_remaining != 0 {
            world
                .get_mut::<WeaponCooldown>(weapon_entity)
                .expect("validated weapon cooldown remains present")
                .remaining_ticks = next_remaining;
            return Ok(());
        }

        let target = {
            let mut enemies = world.query_filtered::<(
                Entity,
                &SceneEntitySource,
                &WaveSpawn,
                &Health,
                &GlobalTransform2d,
            ), With<EnemyRole>>();
            let mut target: Option<(f32, &str, Vec2)> = None;
            let mut target_count = 0_usize;
            for (entity, source, spawn, health, global) in enemies.iter(world) {
                if !owner.owns_scene_entity(entity, source)
                    || health.current <= 0
                    || spawn.tick > run_tick
                {
                    continue;
                }
                target_count = target_count
                    .checked_add(1)
                    .ok_or_else(|| BevyError::error(ReferenceSimulationError::TooManyEnemies))?;
                if target_count > MAX_WAVE_ENEMIES {
                    return Err(BevyError::error(ReferenceSimulationError::TooManyEnemies));
                }
                let candidate = (
                    distance_squared(player.1, global.translation()),
                    source.entity_id.as_str(),
                    global.translation(),
                );
                if target.as_ref().is_none_or(|best| {
                    candidate
                        .0
                        .total_cmp(&best.0)
                        .then_with(|| candidate.1.cmp(best.1))
                        .is_lt()
                }) {
                    target = Some(candidate);
                }
            }
            target.map(|(_, _, position)| position)
        };
        let Some(target_position) = target else {
            world
                .get_mut::<WeaponCooldown>(weapon_entity)
                .expect("validated weapon cooldown remains present")
                .remaining_ticks = next_remaining;
            return Ok(());
        };

        if owner.projectile_tokens().len() >= MAX_WAVE_PROJECTILES {
            return Err(BevyError::error(
                ReferenceSimulationError::TooManyProjectiles,
            ));
        }
        let next_projectile_id = world
            .resource::<WaveState>()
            .next_projectile_id
            .checked_add(1)
            .ok_or_else(|| {
                BevyError::error(ReferenceSimulationError::ProjectileIdentityOverflow)
            })?;
        let projectile_id = ProjectileId(world.resource::<WaveState>().next_projectile_id);
        let offset = target_position - player.1;
        let velocity = Vec2::new(axis_velocity(offset.x, 2.0), axis_velocity(offset.y, 2.0));
        let token = spawn_identity_entity(world)
            .map_err(|_| BevyError::error(ReferenceSimulationError::ProjectileIdentity))?;
        world.entity_mut(token.entity()).insert((
            ProjectileRole,
            projectile_id,
            Transform2d {
                translation: player.1,
                rotation: velocity.y.atan2(velocity.x),
                ..Transform2d::IDENTITY
            },
            Velocity2d { value: velocity },
            ProjectileDamage {
                amount: weapon.damage,
            },
            ProjectileLifetime {
                remaining_ticks: PROJECTILE_TTL_TICKS,
            },
        ));
        world.resource_mut::<WaveState>().next_projectile_id = next_projectile_id;
        world
            .get_mut::<WeaponCooldown>(weapon_entity)
            .expect("validated weapon cooldown remains present")
            .remaining_ticks = weapon.cooldown_ticks.max(1);
        let inserted = owner.record_projectile(token);
        debug_assert!(
            inserted,
            "a fresh bounded projectile token must be recordable"
        );
        Ok(())
    })
}

pub(crate) fn move_wave_projectiles(
    state: Res<WaveState>,
    owner: Res<WaveRunOwner>,
    mut projectiles: Query<(
        &Velocity2d,
        &mut Transform2d,
        Option<&mut ProjectileLifetime>,
    )>,
) -> Result<(), BevyError> {
    if !state.can_simulate() {
        return Ok(());
    }
    for token in owner.projectile_tokens() {
        let (velocity, mut transform, lifetime) = projectiles
            .get_mut(token.entity())
            .map_err(|_| BevyError::error(ReferenceSimulationError::ProjectileOwnershipMismatch))?;
        transform.translation += velocity.value;
        let mut lifetime = lifetime.ok_or_else(|| {
            BevyError::error(ReferenceSimulationError::ProjectileOwnershipMismatch)
        })?;
        lifetime.remaining_ticks = lifetime.remaining_ticks.saturating_sub(1);
    }
    Ok(())
}

pub(crate) fn resolve_wave_projectile_hits(world: &mut World) -> Result<(), BevyError> {
    if !world.resource::<WaveState>().can_simulate() {
        return Ok(());
    }
    let result = resolve_wave_projectile_hits_inner(world);
    world.resource_mut::<WaveState>().reject_on_error(result)
}

fn resolve_wave_projectile_hits_inner(world: &mut World) -> Result<(), BevyError> {
    let run_tick = world
        .resource::<WaveRunGeneration>()
        .run_tick(world.resource::<FixedTime>().tick())
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::InvalidRunTick))?;
    world.resource_scope(|world, owner: Mut<WaveRunOwner>| {
        let mut target_order = {
            let mut enemies = world.query_filtered::<(
                Entity,
                &SceneEntitySource,
                &WaveSpawn,
                &Health,
                &GlobalTransform2d,
            ), With<EnemyRole>>();
            enemies
                .iter(world)
                .filter(|(entity, source, spawn, health, _)| {
                    owner.owns_scene_entity(*entity, source)
                        && health.current > 0
                        && spawn.tick <= run_tick
                })
                .map(|(entity, source, _, _, global)| {
                    (
                        source.entity_id.as_str().to_owned(),
                        entity,
                        global.translation(),
                    )
                })
                .take(MAX_WAVE_ENEMIES + 1)
                .collect::<Vec<_>>()
        };
        if target_order.len() > MAX_WAVE_ENEMIES {
            return Err(BevyError::error(ReferenceSimulationError::TooManyEnemies));
        }
        target_order.sort_by(|left, right| left.0.cmp(&right.0));

        let mut projectiles = owner
            .projectile_tokens()
            .iter()
            .copied()
            .map(|token| {
                world
                    .get::<ProjectileId>(token.entity())
                    .copied()
                    .map(|id| (id, token))
                    .ok_or_else(|| {
                        BevyError::error(ReferenceSimulationError::ProjectileOwnershipMismatch)
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        projectiles.sort_by_key(|(id, _)| id.get());

        for (_, token) in projectiles {
            let damage = world
                .get::<ProjectileDamage>(token.entity())
                .copied()
                .ok_or_else(|| {
                    BevyError::error(ReferenceSimulationError::ProjectileOwnershipMismatch)
                })?;
            let lifetime = world
                .get::<ProjectileLifetime>(token.entity())
                .copied()
                .ok_or_else(|| {
                    BevyError::error(ReferenceSimulationError::ProjectileOwnershipMismatch)
                })?;
            let position = world
                .get::<Transform2d>(token.entity())
                .map(|transform| transform.translation)
                .ok_or_else(|| {
                    BevyError::error(ReferenceSimulationError::ProjectileOwnershipMismatch)
                })?;
            if damage.amount <= 0 || lifetime.remaining_ticks == 0 {
                continue;
            }
            let target = target_order
                .iter()
                .find_map(|(_, entity, target_position)| {
                    (distance_squared(position, *target_position) <= 1.0).then_some(*entity)
                });
            let Some(target) = target else {
                continue;
            };
            let Some(mut health) = world.get_mut::<Health>(target) else {
                return Err(BevyError::error(
                    ReferenceSimulationError::InvalidAuthoredConfiguration,
                ));
            };
            if health.current > 0 {
                health.current = health.current.saturating_sub(damage.amount);
                world
                    .get_mut::<ProjectileLifetime>(token.entity())
                    .expect("validated projectile lifetime remains present")
                    .remaining_ticks = 0;
            }
        }
        Ok(())
    })
}

#[allow(
    clippy::type_complexity,
    reason = "the query signatures keep the ECS read/write contract visible at the system boundary"
)]
pub(crate) fn resolve_enemy_contacts(
    fixed_time: Res<FixedTime>,
    run: Res<WaveRunGeneration>,
    owner: Res<WaveRunOwner>,
    mut state: ResMut<WaveState>,
    enemies: Query<
        (
            Entity,
            &SceneEntitySource,
            &Health,
            &GlobalTransform2d,
            &WaveSpawn,
        ),
        (With<EnemyRole>, Without<PlayerRole>),
    >,
    mut players: Query<
        (Entity, &SceneEntitySource, &GlobalTransform2d, &mut Health),
        (With<PlayerRole>, Without<EnemyRole>),
    >,
) -> Result<(), BevyError> {
    if !state.can_simulate() {
        return Ok(());
    }
    let run_tick = run
        .run_tick(fixed_time.tick())
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::InvalidRunTick))?;

    let result = (|| {
        let mut player = None;
        for (entity, source, global, health) in &mut players {
            if !owner.owns_scene_entity(entity, source)
                || source.entity_id.as_str() != PLAYER_SCENE_ID
            {
                continue;
            }
            if player.replace((global.translation(), health)).is_some() {
                return Err(BevyError::error(ReferenceSimulationError::DuplicatePlayer));
            }
        }
        let Some((player_position, mut player_health)) = player else {
            return Err(BevyError::error(ReferenceSimulationError::MissingPlayer));
        };
        for (entity, source, enemy_health, enemy_global, spawn) in &enemies {
            if owner.owns_scene_entity(entity, source)
                && enemy_health.current > 0
                && spawn.tick <= run_tick
                && distance_squared(player_position, enemy_global.translation()) <= 1.0
            {
                player_health.current = player_health.current.saturating_sub(ENEMY_CONTACT_DAMAGE);
            }
        }
        Ok(())
    })();
    state.reject_on_error(result)
}

pub(crate) fn retire_expired_entities(world: &mut World) -> Result<(), BevyError> {
    if !world.resource::<WaveState>().can_simulate() {
        return Ok(());
    }
    let result = retire_expired_entities_inner(world);
    world.resource_mut::<WaveState>().reject_on_error(result)
}

fn retire_expired_entities_inner(world: &mut World) -> Result<(), BevyError> {
    world.resource_scope(|world, mut owner: Mut<WaveRunOwner>| {
        let mut dead_enemies = world
            .iter_entities()
            .filter_map(|entity| {
                let source = entity.get::<SceneEntitySource>()?;
                if !owner.owns_scene_entity(entity.id(), source) {
                    return None;
                }
                entity
                    .get::<EnemyRole>()
                    .zip(entity.get::<Health>())
                    .is_some_and(|(_, health)| health.current <= 0)
                    .then(|| {
                        (
                            source.instance_id,
                            source.entity_id.as_str().to_owned(),
                            entity.id(),
                        )
                    })
            })
            .take(MAX_WAVE_ENEMIES + 1)
            .collect::<Vec<_>>();
        if dead_enemies.len() > MAX_WAVE_ENEMIES {
            return Err(BevyError::error(ReferenceSimulationError::TooManyEnemies));
        }
        dead_enemies.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        let mut expired_projectiles = owner
            .projectile_tokens()
            .iter()
            .copied()
            .filter_map(|token| {
                let lifetime = world.get::<ProjectileLifetime>(token.entity())?;
                let id = world.get::<ProjectileId>(token.entity())?;
                (lifetime.remaining_ticks == 0).then_some((id.get(), token))
            })
            .collect::<Vec<_>>();
        if expired_projectiles.len() > MAX_WAVE_PROJECTILES {
            return Err(BevyError::error(
                ReferenceSimulationError::TooManyProjectiles,
            ));
        }
        expired_projectiles.sort_by_key(|(id, _)| *id);

        for (_, _, entity) in dead_enemies.iter() {
            retire_and_despawn_scene_entity(world, *entity)
                .map_err(|_| BevyError::error(ReferenceSimulationError::IdentityRetirement))?;
        }
        for (_, token) in expired_projectiles {
            if !world.despawn(token.entity()) || !owner.forget_projectile(token) {
                return Err(BevyError::error(
                    ReferenceSimulationError::ProjectileDespawn,
                ));
            }
        }
        let kills = u64::try_from(dead_enemies.len())
            .map_err(|_| BevyError::error(ReferenceSimulationError::ScoreOverflow))?;
        let earned = kills
            .checked_mul(KILL_SCORE)
            .ok_or_else(|| BevyError::error(ReferenceSimulationError::ScoreOverflow))?;
        let mut state = world.resource_mut::<WaveState>();
        let score = state
            .score
            .checked_add(earned)
            .ok_or_else(|| BevyError::error(ReferenceSimulationError::ScoreOverflow))?;
        let defeated_enemies = state
            .defeated_enemies
            .checked_add(kills)
            .ok_or_else(|| BevyError::error(ReferenceSimulationError::ProgressOverflow))?;
        state.score = score;
        state.defeated_enemies = defeated_enemies;
        Ok(())
    })
}

pub(crate) fn evaluate_wave_outcome(
    owner: Res<WaveRunOwner>,
    mut state: ResMut<WaveState>,
    players: Query<(Entity, &SceneEntitySource, &Health), With<PlayerRole>>,
    enemies: Query<(Entity, &SceneEntitySource, &Health), With<EnemyRole>>,
) -> Result<(), BevyError> {
    if !state.can_simulate() {
        return Ok(());
    }

    let result = (|| {
        let mut player_hit_points = None;
        for (entity, source, health) in &players {
            if !owner.owns_scene_entity(entity, source)
                || source.entity_id.as_str() != PLAYER_SCENE_ID
            {
                continue;
            }
            if player_hit_points.replace(health.current).is_some() {
                return Err(BevyError::error(ReferenceSimulationError::DuplicatePlayer));
            }
        }
        let Some(player_hit_points) = player_hit_points else {
            return Err(BevyError::error(ReferenceSimulationError::MissingPlayer));
        };
        let enemies_alive = enemies.iter().any(|(entity, source, health)| {
            owner.owns_scene_entity(entity, source) && health.current > 0
        });
        let outcome = if player_hit_points <= 0 {
            WaveOutcome::Defeated
        } else if !enemies_alive {
            WaveOutcome::Completed
        } else {
            WaveOutcome::Running
        };
        if outcome.is_terminal() {
            state.outcome = outcome;
        }
        Ok(())
    })();
    state.reject_on_error(result)
}

fn distance_squared(left: Vec2, right: Vec2) -> f32 {
    let offset = left - right;
    offset.x.mul_add(offset.x, offset.y * offset.y)
}

fn axis_velocity(offset: f32, speed: f32) -> f32 {
    if offset > 0.0 {
        speed
    } else if offset < 0.0 {
        -speed
    } else {
        0.0
    }
}

#[derive(Debug)]
enum ReferenceSimulationError {
    TooManySceneEntities,
    SceneMembershipMismatch,
    ComponentRegistryUnavailable,
    InvalidAuthoredConfiguration,
    InvalidRunTick,
    MissingPlayer,
    DuplicatePlayer,
    UnexpectedPlayerIdentity,
    MissingPlayerWeapon,
    MissingEnemy,
    MissingWaveSpawn,
    EnemyPopulationChanged,
    InvalidMovementCommand,
    MissingEnemyTarget,
    DuplicateEnemyTarget,
    IdentityRetirement,
    ProjectileIdentity,
    ProjectileDespawn,
    ProjectileOwnershipMismatch,
    ScoreOverflow,
    ProgressOverflow,
    ProjectileIdentityOverflow,
    TooManyEnemies,
    TooManyProjectiles,
    InvalidSpatialHierarchy,
}

impl fmt::Display for ReferenceSimulationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManySceneEntities => "reference wave scene entity limit was exceeded",
            Self::SceneMembershipMismatch => {
                "reference wave scene identity membership is inconsistent"
            }
            Self::ComponentRegistryUnavailable => {
                "reference wave component registry is unavailable"
            }
            Self::InvalidAuthoredConfiguration => {
                "reference wave authored configuration is invalid"
            }
            Self::InvalidRunTick => "reference wave run tick is invalid",
            Self::MissingPlayer => "reference wave player is missing",
            Self::DuplicatePlayer => "reference wave player identity is duplicated",
            Self::UnexpectedPlayerIdentity => "reference wave player identity is unsupported",
            Self::MissingPlayerWeapon => "reference wave player weapon is missing",
            Self::MissingEnemy => "reference wave has no enemy roster",
            Self::MissingWaveSpawn => "reference wave enemy has no spawn schedule",
            Self::EnemyPopulationChanged => "reference wave enemy roster changed during execution",
            Self::InvalidMovementCommand => "reference wave movement command is invalid",
            Self::MissingEnemyTarget => "reference wave enemy target is missing",
            Self::DuplicateEnemyTarget => "reference wave enemy target identity is duplicated",
            Self::IdentityRetirement => "reference wave identity retirement failed",
            Self::ProjectileIdentity => "reference wave projectile identity allocation failed",
            Self::ProjectileDespawn => "reference wave projectile retirement failed",
            Self::ProjectileOwnershipMismatch => {
                "reference wave projectile ownership is inconsistent"
            }
            Self::ScoreOverflow => "reference wave score overflowed",
            Self::ProgressOverflow => "reference wave progress overflowed",
            Self::ProjectileIdentityOverflow => "reference wave projectile identity overflowed",
            Self::TooManyEnemies => "reference wave enemy limit was exceeded",
            Self::TooManyProjectiles => "reference wave projectile limit was exceeded",
            Self::InvalidSpatialHierarchy => {
                "reference game spatial hierarchy is incomplete or invalid"
            }
        })
    }
}

impl Error for ReferenceSimulationError {}

pub(crate) fn observe_project_commands(
    batch: Res<GameplayCommandBatch>,
    mut snapshot: ResMut<ReferenceProjectSnapshot>,
) {
    if snapshot.first_command_key.is_none()
        && let Some(command) = batch.commands().first()
    {
        snapshot.first_command_key = Some(command.key().clone());
        snapshot.first_command_type = Some(command.command_type().clone());
    }
    snapshot.commands_seen = snapshot
        .commands_seen
        .saturating_add(u64::try_from(batch.commands().len()).unwrap_or(u64::MAX));
}

#[derive(SystemParam)]
pub(crate) struct ProjectSnapshotQueries<'w, 's> {
    players: Query<
        'w,
        's,
        (
            Entity,
            &'static SceneEntitySource,
            &'static GlobalTransform2d,
            &'static Health,
        ),
        With<PlayerRole>,
    >,
    enemies: Query<
        'w,
        's,
        (
            Entity,
            &'static SceneEntitySource,
            &'static GlobalTransform2d,
            &'static Health,
        ),
        With<EnemyRole>,
    >,
    weapons:
        Query<'w, 's, (Entity, &'static SceneEntitySource, &'static WeaponCooldown), With<Weapon>>,
    runtime_only: Query<'w, 's, (), With<RuntimeOnlyTag>>,
    unbound_players: Query<'w, 's, (), (With<PlayerRole>, Without<SceneEntitySource>)>,
    unbound_enemies: Query<'w, 's, (), (With<EnemyRole>, Without<SceneEntitySource>)>,
    unbound_weapons: Query<'w, 's, (), (With<Weapon>, Without<SceneEntitySource>)>,
}

pub(crate) fn capture_project_snapshot(
    fixed_time: Res<FixedTime>,
    owner: Res<WaveRunOwner>,
    queries: ProjectSnapshotQueries,
    mut snapshot: ResMut<ReferenceProjectSnapshot>,
) -> Result<(), BevyError> {
    let mut player = None;
    for (entity, source, global, health) in &queries.players {
        if !owner.owns_scene_entity(entity, source) || source.entity_id.as_str() != PLAYER_SCENE_ID
        {
            continue;
        }
        if player.replace((global, health)).is_some() {
            return Err(BevyError::error(ProjectSnapshotError::DuplicatePlayer));
        }
    }
    let Some((player_global, player_health)) = player else {
        return Err(BevyError::error(ProjectSnapshotError::MissingPlayer));
    };

    let mut enemy = None;
    for (entity, source, global, health) in &queries.enemies {
        if !owner.owns_scene_entity(entity, source)
            || source.entity_id.as_str() != "enemy-anchor/enemy"
        {
            continue;
        }
        if enemy.replace((global, health)).is_some() {
            return Err(BevyError::error(ProjectSnapshotError::DuplicateEnemy));
        }
    }
    let Some((enemy_global, enemy_health)) = enemy else {
        return Err(BevyError::error(ProjectSnapshotError::MissingEnemy));
    };

    let mut weapon = None;
    for (entity, source, cooldown) in &queries.weapons {
        if !owner.owns_scene_entity(entity, source)
            || source.entity_id.as_str() != PLAYER_WEAPON_SCENE_ID
        {
            continue;
        }
        if weapon.replace(cooldown).is_some() {
            return Err(BevyError::error(ProjectSnapshotError::DuplicateWeapon));
        }
    }
    let Some(weapon) = weapon else {
        return Err(BevyError::error(ProjectSnapshotError::MissingWeapon));
    };

    snapshot.tick = fixed_time.tick();
    snapshot.player_position = player_global.translation();
    snapshot.player_hit_points = player_health.current;
    snapshot.enemy_position = enemy_global.translation();
    snapshot.enemy_hit_points = enemy_health.current;
    snapshot.weapon_remaining_ticks = weapon.remaining_ticks;
    snapshot.runtime_only_entities =
        u64::try_from(queries.runtime_only.iter().count()).unwrap_or(u64::MAX);
    snapshot.unbound_gameplay_components = [
        queries.unbound_players.iter().count(),
        queries.unbound_enemies.iter().count(),
        queries.unbound_weapons.iter().count(),
    ]
    .into_iter()
    .fold(0_u64, |total, count| {
        total.saturating_add(u64::try_from(count).unwrap_or(u64::MAX))
    });
    Ok(())
}

#[derive(Debug)]
enum ProjectSnapshotError {
    MissingPlayer,
    DuplicatePlayer,
    MissingEnemy,
    DuplicateEnemy,
    MissingWeapon,
    DuplicateWeapon,
}

impl fmt::Display for ProjectSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingPlayer => "project scene player is missing",
            Self::DuplicatePlayer => "project scene player identity is duplicated",
            Self::MissingEnemy => "project scene enemy is missing",
            Self::DuplicateEnemy => "project scene enemy identity is duplicated",
            Self::MissingWeapon => "project scene weapon is missing",
            Self::DuplicateWeapon => "project scene weapon identity is duplicated",
        })
    }
}

impl Error for ProjectSnapshotError {}
