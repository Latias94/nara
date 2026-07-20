use std::{collections::BTreeSet, error::Error, fmt};

use nara::ecs as bevy_ecs;
use nara::{
    app::RuntimeFaultReporter,
    ecs::{
        Component, Entity, LifecycleFreeInsertionPlan, With, Without, error::BevyError,
        prepare_lifecycle_free_despawn, system::SystemParam,
    },
    gameplay::{GameplayCommandBatch, GameplayCommandValue},
    identity::{EntityLookup, TombstoneCause, WorldIdentityDomain, spawn_identity_entity},
    prelude::{Commands, EntityReference, FixedTime, Query, Res, ResMut, Vec2, World},
    scene::{SceneEntitySource, retire_and_despawn_scene_entity},
};

use crate::{
    Enemy, Player, Projectile, ReferenceProjectSnapshot, RuntimeOnlyTag, WaveSpawn, Weapon,
    resources::{
        DesktopInputGate, ENEMY_CONTACT_DAMAGE, KILL_SCORE, MAX_WAVE_ENEMIES, MAX_WAVE_PROJECTILES,
        MAX_WAVE_SCENE_ENTITIES, MOVE_COMMAND_TYPE, MOVE_X_FIELD, MOVE_Y_FIELD, PLAYER_SCENE_ID,
        PROJECTILE_TTL_TICKS, ProjectileId, RETRY_COMMAND_TYPE, WaveEntityTemplate,
        WaveResetTemplate, WaveRetryStatus, WaveRunGeneration, WaveSceneTemplate, WaveState,
    },
    snapshot::WaveOutcome,
};

pub(crate) fn move_project_players(mut players: Query<&mut Player, With<SceneEntitySource>>) {
    for mut player in &mut players {
        let velocity = player.velocity;
        player.position += velocity;
    }
}

pub(crate) fn move_project_enemies(mut enemies: Query<&mut Enemy, With<SceneEntitySource>>) {
    for mut enemy in &mut enemies {
        let velocity = enemy.velocity;
        enemy.position += velocity;
    }
}

pub(crate) fn tick_project_weapons(mut weapons: Query<&mut Weapon, With<SceneEntitySource>>) {
    for mut weapon in &mut weapons {
        weapon.remaining_ticks = weapon.remaining_ticks.saturating_sub(1);
    }
}

pub(crate) fn begin_wave_tick(world: &mut World) -> Result<(), BevyError> {
    if world.resource::<WaveResetTemplate>().scene.is_none() {
        let template = capture_wave_reset_template(world)?;
        world.resource_mut::<WaveResetTemplate>().scene = Some(template);
    }
    let generation = world.resource::<WaveRunGeneration>().get();
    if world.resource::<WaveRetryStatus>().pending_generation() == Some(generation) {
        apply_wave_reset(world)?;
        let applied_generation = world.resource::<WaveRunGeneration>().get();
        world
            .resource_mut::<WaveRetryStatus>()
            .mark_applied(applied_generation);
    }
    let wait_for_input = world.contains_resource::<DesktopInputGate>();
    world.resource_mut::<WaveState>().begin_tick(wait_for_input);
    if world.resource::<WaveState>().is_waiting_for_input() {
        let fixed_tick = world.resource::<FixedTime>().tick();
        world
            .resource_mut::<WaveRunGeneration>()
            .hold_before_input(fixed_tick);
    }
    Ok(())
}

fn capture_wave_reset_template(world: &World) -> Result<WaveSceneTemplate, BevyError> {
    let mut instance_id = None;
    let mut source_count = 0_usize;
    for entity in world.iter_entities() {
        let Some(source) = entity.get::<SceneEntitySource>() else {
            continue;
        };
        source_count = source_count
            .checked_add(1)
            .ok_or_else(|| BevyError::error(ReferenceSimulationError::TooManySceneEntities))?;
        if source_count > MAX_WAVE_SCENE_ENTITIES {
            return Err(BevyError::error(
                ReferenceSimulationError::TooManySceneEntities,
            ));
        }
        if instance_id
            .replace(source.instance_id)
            .is_some_and(|current| current != source.instance_id)
        {
            return Err(BevyError::error(
                ReferenceSimulationError::MultipleSceneInstances,
            ));
        }
    }
    let instance_id = instance_id
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::MissingSceneInstance))?;
    let instance = world
        .resource::<WorldIdentityDomain>()
        .active_scene_instance(world, instance_id)
        .map_err(|_| BevyError::error(ReferenceSimulationError::IdentitySnapshot))?;
    if instance.len() != source_count {
        return Err(BevyError::error(
            ReferenceSimulationError::SceneMembershipMismatch,
        ));
    }

    let mut entities = Vec::with_capacity(instance.len());
    for id in instance.entity_ids() {
        let EntityLookup::Resolved(entity) = instance.resolve(world, id) else {
            return Err(BevyError::error(ReferenceSimulationError::IdentitySnapshot));
        };
        let entity = world
            .get_entity(entity)
            .map_err(|_| BevyError::error(ReferenceSimulationError::IdentitySnapshot))?;
        entities.push(WaveEntityTemplate {
            id: id.clone(),
            player: entity.get::<Player>().cloned(),
            enemy: entity.get::<Enemy>().cloned(),
            spawn: entity.get::<WaveSpawn>().copied(),
            weapon: entity.get::<Weapon>().cloned(),
            projectile: entity.get::<Projectile>().cloned(),
        });
    }
    Ok(WaveSceneTemplate { instance, entities })
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
    {
        let mut status = world.resource_mut::<WaveRetryStatus>();
        if status.pending_generation() == Some(generation) {
            status.reject_duplicate();
        } else {
            status.mark_pending(generation);
        }
        if retry_count > 1 {
            status.reject_duplicate();
        }
    }

    Ok(())
}

fn apply_wave_reset(world: &mut World) -> Result<(), BevyError> {
    let fixed_tick = world.resource::<FixedTime>().tick();
    let mut next_generation = *world.resource::<WaveRunGeneration>();
    next_generation
        .advance_for_reset(fixed_tick)
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::RunGenerationOverflow))?;

    let mut reset = world
        .remove_resource::<WaveResetTemplate>()
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::MissingResetTemplate))?;
    let result = apply_wave_reset_with_template(world, &mut reset, next_generation);
    world.insert_resource(reset);
    result
}

fn apply_wave_reset_with_template(
    world: &mut World,
    reset: &mut WaveResetTemplate,
    next_generation: WaveRunGeneration,
) -> Result<(), BevyError> {
    let scene = reset
        .scene
        .as_mut()
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::MissingResetTemplate))?;
    let candidate_instance = world
        .resource::<WorldIdentityDomain>()
        .preflight_scene_instance_replacement(
            world,
            &scene.instance,
            scene.entities.len(),
            TombstoneCause::Replaced,
        )
        .map_err(|_| BevyError::error(ReferenceSimulationError::IdentityReplacement))?;
    let retirements = preflight_wave_reset_retirements(world, scene)?;
    let retirement_entities = retirements.all.iter().copied().collect::<Vec<_>>();
    let retirement_preflight = prepare_lifecycle_free_despawn(world, &retirement_entities)
        .map_err(|_| BevyError::error(ReferenceSimulationError::IdentityReplacement))?;
    let _ = retirement_preflight.cancel();

    let mut spawned = Vec::with_capacity(scene.entities.len());
    for template in &scene.entities {
        let token = match spawn_identity_entity(world) {
            Ok(token) => token,
            Err(_) => {
                rollback_reset_entities(world, &spawned);
                return Err(BevyError::error(
                    ReferenceSimulationError::IdentityReplacement,
                ));
            }
        };
        spawned.push((template.id.clone(), token));
    }

    let mut insertion_plan = LifecycleFreeInsertionPlan::new();
    for (template, (_, token)) in scene.entities.iter().zip(&spawned) {
        let entity = token.entity();
        if let Some(component) = template.player.clone() {
            insertion_plan.push(entity, component);
        }
        if let Some(component) = template.enemy.clone() {
            insertion_plan.push(entity, component);
        }
        if let Some(component) = template.spawn {
            insertion_plan.push(entity, component);
        }
        if let Some(component) = template.weapon.clone() {
            insertion_plan.push(entity, component);
        }
        if let Some(component) = template.projectile.clone() {
            insertion_plan.push(entity, component);
        }
        insertion_plan.push(
            entity,
            SceneEntitySource {
                instance_id: candidate_instance,
                entity_id: template.id.clone(),
            },
        );
    }
    if insertion_plan.commit(world).is_err() {
        rollback_reset_entities(world, &spawned);
        return Err(BevyError::error(
            ReferenceSimulationError::IdentityReplacement,
        ));
    }
    world.flush();

    let confirmed_instance = match world
        .resource::<WorldIdentityDomain>()
        .preflight_scene_instance_replacement(
            world,
            &scene.instance,
            scene.entities.len(),
            TombstoneCause::Replaced,
        ) {
        Ok(instance) => instance,
        Err(_) => {
            rollback_reset_entities(world, &spawned);
            return Err(BevyError::error(
                ReferenceSimulationError::IdentityReplacement,
            ));
        }
    };
    if confirmed_instance != candidate_instance {
        rollback_reset_entities(world, &spawned);
        return Err(BevyError::error(
            ReferenceSimulationError::IdentityReplacement,
        ));
    }
    if !reset_candidates_match_template(world, scene, &spawned, candidate_instance) {
        rollback_reset_entities(world, &spawned);
        return Err(BevyError::error(
            ReferenceSimulationError::IdentityReplacement,
        ));
    }
    let replacement_entries = spawned
        .iter()
        .map(|(id, token)| (id.clone(), *token))
        .collect::<Vec<_>>();
    let replacement = WorldIdentityDomain::replace_scene_instance_and_despawn(
        world,
        &scene.instance,
        &replacement_entries,
        &retirement_entities,
        TombstoneCause::Replaced,
    );
    let (replacement, retired) = match replacement {
        Ok(replacement) => replacement,
        Err(_) => {
            rollback_reset_entities(world, &spawned);
            return Err(BevyError::error(
                ReferenceSimulationError::IdentityReplacement,
            ));
        }
    };
    debug_assert_eq!(
        retired.into_iter().collect::<BTreeSet<_>>(),
        retirements.scene
    );
    debug_assert_eq!(replacement.instance_id(), candidate_instance);
    scene.instance = replacement;
    *world.resource_mut::<WaveRunGeneration>() = next_generation;
    let wait_for_input = world.contains_resource::<DesktopInputGate>();
    world.resource_mut::<WaveState>().reset(wait_for_input);
    Ok(())
}

fn reset_candidates_match_template(
    world: &World,
    scene: &WaveSceneTemplate,
    spawned: &[(
        nara::prelude::SceneEntityId,
        nara::identity::WorldEntityToken,
    )],
    candidate_instance: nara::identity::SceneInstanceId,
) -> bool {
    scene
        .entities
        .iter()
        .zip(spawned)
        .all(|(template, (id, token))| {
            &template.id == id
                && world.get::<SceneEntitySource>(token.entity())
                    == Some(&SceneEntitySource {
                        instance_id: candidate_instance,
                        entity_id: id.clone(),
                    })
                && reset_component_matches(world, token.entity(), &template.player)
                && reset_component_matches(world, token.entity(), &template.enemy)
                && reset_component_matches(world, token.entity(), &template.spawn)
                && reset_component_matches(world, token.entity(), &template.weapon)
                && reset_component_matches(world, token.entity(), &template.projectile)
        })
}

fn reset_component_matches<T>(world: &World, entity: Entity, expected: &Option<T>) -> bool
where
    T: Component + PartialEq,
{
    world.get::<T>(entity) == expected.as_ref()
}

struct WaveResetRetirements {
    scene: BTreeSet<Entity>,
    all: BTreeSet<Entity>,
}

fn preflight_wave_reset_retirements(
    world: &World,
    scene: &WaveSceneTemplate,
) -> Result<WaveResetRetirements, BevyError> {
    let active = world
        .resource::<WorldIdentityDomain>()
        .active_scene_instance(world, scene.instance.instance_id())
        .map_err(|_| BevyError::error(ReferenceSimulationError::IdentityReplacement))?;
    let mut scene_entities = BTreeSet::new();
    for id in active.entity_ids() {
        let EntityLookup::Resolved(entity) = active.resolve(world, id) else {
            return Err(BevyError::error(
                ReferenceSimulationError::SceneMembershipMismatch,
            ));
        };
        let Some(source) = world.get::<SceneEntitySource>(entity) else {
            return Err(BevyError::error(
                ReferenceSimulationError::SceneMembershipMismatch,
            ));
        };
        if source.instance_id != active.instance_id() || &source.entity_id != id {
            return Err(BevyError::error(
                ReferenceSimulationError::SceneMembershipMismatch,
            ));
        }
        if !scene_entities.insert(entity) {
            return Err(BevyError::error(
                ReferenceSimulationError::SceneMembershipMismatch,
            ));
        }
    }

    let mut source_count = 0_usize;
    for entity in world.iter_entities() {
        let Some(source) = entity.get::<SceneEntitySource>() else {
            continue;
        };
        source_count = source_count
            .checked_add(1)
            .ok_or_else(|| BevyError::error(ReferenceSimulationError::TooManySceneEntities))?;
        if source_count > MAX_WAVE_SCENE_ENTITIES {
            return Err(BevyError::error(
                ReferenceSimulationError::TooManySceneEntities,
            ));
        }
        if source.instance_id != active.instance_id()
            || !active.contains(&source.entity_id)
            || active.resolve(world, &source.entity_id) != EntityLookup::Resolved(entity.id())
        {
            return Err(BevyError::error(
                ReferenceSimulationError::SceneMembershipMismatch,
            ));
        }
    }
    if source_count != active.len() {
        return Err(BevyError::error(
            ReferenceSimulationError::SceneMembershipMismatch,
        ));
    }

    let runtime_projectiles = world
        .iter_entities()
        .filter(|entity| entity.contains::<Projectile>() && !entity.contains::<SceneEntitySource>())
        .map(|entity| entity.id())
        .take(MAX_WAVE_PROJECTILES + 1)
        .collect::<Vec<_>>();
    if runtime_projectiles.len() > MAX_WAVE_PROJECTILES {
        return Err(BevyError::error(
            ReferenceSimulationError::TooManyProjectiles,
        ));
    }
    let mut all = scene_entities.clone();
    if runtime_projectiles
        .into_iter()
        .any(|entity| !all.insert(entity))
    {
        return Err(BevyError::error(
            ReferenceSimulationError::SceneMembershipMismatch,
        ));
    }
    Ok(WaveResetRetirements {
        scene: scene_entities,
        all,
    })
}

fn rollback_reset_entities(
    world: &mut World,
    spawned: &[(nara::scene::SceneEntityId, nara::identity::WorldEntityToken)],
) {
    let entities = spawned
        .iter()
        .map(|(_, token)| token.entity())
        .filter(|entity| world.get_entity(*entity).is_ok())
        .collect::<Vec<_>>();
    if let Ok(retirement) = prepare_lifecycle_free_despawn(world, &entities) {
        let _ = retirement.commit();
    }
}

pub(crate) fn consume_movement_commands(
    batch: Res<GameplayCommandBatch>,
    fixed_time: Res<FixedTime>,
    mut run: ResMut<WaveRunGeneration>,
    mut state: ResMut<WaveState>,
    mut players: Query<(&SceneEntitySource, &mut Player)>,
) -> Result<(), BevyError> {
    if !state.is_running() {
        return Ok(());
    }

    let result = (|| {
        for command in batch.commands() {
            if command.command_type().as_str() != MOVE_COMMAND_TYPE {
                continue;
            }
            let velocity = movement_velocity(command.payload())?;
            let mut player = None;
            for (source, candidate) in &mut players {
                if source.entity_id.as_str() != PLAYER_SCENE_ID {
                    continue;
                }
                if player.replace(candidate).is_some() {
                    return Err(BevyError::error(ReferenceSimulationError::DuplicatePlayer));
                }
            }
            let Some(mut player) = player else {
                return Err(BevyError::error(ReferenceSimulationError::MissingPlayer));
            };
            if state.is_waiting_for_input() && velocity != Vec2::ZERO {
                run.begin_from_input(fixed_time.tick())
                    .ok_or_else(|| BevyError::error(ReferenceSimulationError::InvalidRunTick))?;
                state.start_from_input();
            }
            player.velocity = velocity;
        }
        Ok(())
    })();
    state.reject_on_error(result)
}

fn movement_velocity(payload: &nara::gameplay::GameplayCommandPayload) -> Result<Vec2, BevyError> {
    if payload.len() != 2 {
        return Err(BevyError::error(
            ReferenceSimulationError::InvalidMovementCommand,
        ));
    }
    let (Some(GameplayCommandValue::I64(x)), Some(GameplayCommandValue::I64(y))) =
        (payload.get(MOVE_X_FIELD), payload.get(MOVE_Y_FIELD))
    else {
        return Err(BevyError::error(
            ReferenceSimulationError::InvalidMovementCommand,
        ));
    };
    if !(-1..=1).contains(x) || !(-1..=1).contains(y) || (x != &0 && y != &0) {
        return Err(BevyError::error(
            ReferenceSimulationError::InvalidMovementCommand,
        ));
    }
    Ok(Vec2::new(*x as f32, *y as f32))
}

pub(crate) fn assign_scene_projectile_ids(world: &mut World) -> Result<(), BevyError> {
    if !world.resource::<WaveState>().tick_is_pending() {
        return Ok(());
    }
    if world.resource::<RuntimeFaultReporter>().fault().is_some() {
        world.resource_mut::<WaveState>().reject_tick();
        return Ok(());
    }

    let pending = (|| {
        let mut pending = Vec::new();
        let mut projectile_count = 0_usize;
        let mut orphan_projectile_ids =
            world.query_filtered::<(), (With<ProjectileId>, Without<Projectile>)>();
        if orphan_projectile_ids.iter(world).next().is_some() {
            return Err(ReferenceSimulationError::OrphanProjectileIdentity);
        }
        let mut projectiles = world.query_filtered::<
            (Entity, Option<&SceneEntitySource>, Option<&ProjectileId>),
            With<Projectile>,
        >();
        for (entity, source, projectile_id) in projectiles.iter(world) {
            projectile_count = projectile_count
                .checked_add(1)
                .ok_or(ReferenceSimulationError::TooManyProjectiles)?;
            if projectile_count > MAX_WAVE_PROJECTILES {
                return Err(ReferenceSimulationError::TooManyProjectiles);
            }
            if projectile_id.is_some() {
                continue;
            }
            pending.push((
                source
                    .cloned()
                    .ok_or(ReferenceSimulationError::MissingProjectileIdentity)?,
                entity,
            ));
        }
        pending.sort_by(|(left, _), (right, _)| {
            left.instance_id
                .cmp(&right.instance_id)
                .then_with(|| left.entity_id.as_str().cmp(right.entity_id.as_str()))
        });
        Ok(pending)
    })();
    let pending = match pending {
        Ok(pending) => pending,
        Err(error) => {
            world.resource_mut::<WaveState>().reject_tick();
            return Err(BevyError::error(error));
        }
    };
    let projectile_ids = world
        .resource_mut::<WaveState>()
        .allocate_projectile_ids(pending.len());
    let Some(projectile_ids) = projectile_ids else {
        world.resource_mut::<WaveState>().reject_tick();
        return Err(BevyError::error(
            ReferenceSimulationError::ProjectileIdentityOverflow,
        ));
    };
    for ((_, entity), projectile_id) in pending.into_iter().zip(projectile_ids) {
        world.entity_mut(entity).insert(projectile_id);
    }
    Ok(())
}

pub(crate) fn validate_wave_topology(
    mut state: ResMut<WaveState>,
    faults: Res<RuntimeFaultReporter>,
    enemies: Query<(&SceneEntitySource, Option<&WaveSpawn>), With<Enemy>>,
    spawns: Query<(&SceneEntitySource, Option<&Enemy>), With<WaveSpawn>>,
    players: Query<(&SceneEntitySource, Option<&Weapon>), With<Player>>,
    projectiles: Query<Option<&ProjectileId>, With<Projectile>>,
    orphan_projectile_ids: Query<(), (With<ProjectileId>, Without<Projectile>)>,
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
        let mut enemy_count = 0_usize;
        for (_, spawn) in &enemies {
            if spawn.is_none() {
                return Err(BevyError::error(ReferenceSimulationError::MissingWaveSpawn));
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
        if spawns.iter().any(|(_, enemy)| enemy.is_none()) {
            return Err(BevyError::error(ReferenceSimulationError::OrphanWaveSpawn));
        }
        let mut projectile_count = 0_usize;
        for projectile_id in &projectiles {
            projectile_count = projectile_count
                .checked_add(1)
                .ok_or_else(|| BevyError::error(ReferenceSimulationError::TooManyProjectiles))?;
            if projectile_count > MAX_WAVE_PROJECTILES {
                return Err(BevyError::error(
                    ReferenceSimulationError::TooManyProjectiles,
                ));
            }
            if projectile_id.is_none() {
                return Err(BevyError::error(
                    ReferenceSimulationError::MissingProjectileIdentity,
                ));
            }
        }
        if !orphan_projectile_ids.is_empty() {
            return Err(BevyError::error(
                ReferenceSimulationError::OrphanProjectileIdentity,
            ));
        }

        let mut player = None;
        for (source, weapon) in &players {
            if source.entity_id.as_str() != PLAYER_SCENE_ID {
                return Err(BevyError::error(
                    ReferenceSimulationError::UnexpectedPlayerIdentity,
                ));
            }
            if weapon.is_none() {
                return Err(BevyError::error(
                    ReferenceSimulationError::MissingPlayerWeapon,
                ));
            }
            if player.replace(()).is_some() {
                return Err(BevyError::error(ReferenceSimulationError::DuplicatePlayer));
            }
        }
        if player.is_none() {
            return Err(BevyError::error(ReferenceSimulationError::MissingPlayer));
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
    mut players: Query<&mut Player, With<SceneEntitySource>>,
) {
    if !state.can_simulate() {
        return;
    }
    for mut player in &mut players {
        let velocity = player.velocity;
        player.position += velocity;
    }
}

pub(crate) fn pursue_scene_players(
    fixed_time: Res<FixedTime>,
    run: Res<WaveRunGeneration>,
    mut state: ResMut<WaveState>,
    players: Query<(&SceneEntitySource, &Player)>,
    mut enemies: Query<(&SceneEntitySource, &WaveSpawn, &mut Enemy)>,
) -> Result<(), BevyError> {
    if !state.can_simulate() {
        return Ok(());
    }
    let run_tick = run
        .run_tick(fixed_time.tick())
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::InvalidRunTick))?;

    let result = (|| {
        for (enemy_source, spawn, mut enemy) in &mut enemies {
            if enemy.hit_points <= 0 || spawn.tick > run_tick {
                continue;
            }
            let EntityReference::SceneLocal { entity: target } = &enemy.target else {
                return Err(BevyError::error(
                    ReferenceSimulationError::UnsupportedEnemyTarget,
                ));
            };
            let mut target_position = None;
            for (source, player) in &players {
                if source.instance_id != enemy_source.instance_id || &source.entity_id != target {
                    continue;
                }
                if target_position.replace(player.position).is_some() {
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
            let offset = target_position - enemy.position;
            enemy.velocity = Vec2::new(axis_velocity(offset.x, 0.5), axis_velocity(offset.y, 0.5));
            let velocity = enemy.velocity;
            enemy.position += velocity;
        }
        Ok(())
    })();
    state.reject_on_error(result)
}

pub(crate) fn fire_automatic_weapons(
    mut commands: Commands,
    fixed_time: Res<FixedTime>,
    run: Res<WaveRunGeneration>,
    mut state: ResMut<WaveState>,
    enemies: Query<(&SceneEntitySource, &WaveSpawn, &Enemy)>,
    mut players: Query<(&SceneEntitySource, &Player, &mut Weapon)>,
) -> Result<(), BevyError> {
    if !state.can_simulate() {
        return Ok(());
    }
    let run_tick = run
        .run_tick(fixed_time.tick())
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::InvalidRunTick))?;

    let result = (|| {
        let mut player = None;
        for (source, candidate, weapon) in &mut players {
            if source.entity_id.as_str() != PLAYER_SCENE_ID {
                continue;
            }
            if player.replace((candidate.position, weapon)).is_some() {
                return Err(BevyError::error(ReferenceSimulationError::DuplicatePlayer));
            }
        }
        let Some((player_position, mut weapon)) = player else {
            return Err(BevyError::error(
                ReferenceSimulationError::MissingPlayerWeapon,
            ));
        };

        weapon.remaining_ticks = weapon.remaining_ticks.saturating_sub(1);
        if weapon.remaining_ticks != 0 {
            return Ok(());
        }

        let mut target: Option<(f32, &str, Vec2)> = None;
        let mut target_count = 0_usize;
        for (source, spawn, enemy) in &enemies {
            if enemy.hit_points <= 0 || spawn.tick > run_tick {
                continue;
            }
            target_count = target_count
                .checked_add(1)
                .ok_or_else(|| BevyError::error(ReferenceSimulationError::TooManyEnemies))?;
            if target_count > MAX_WAVE_ENEMIES {
                return Err(BevyError::error(ReferenceSimulationError::TooManyEnemies));
            }
            let candidate = (
                distance_squared(player_position, enemy.position),
                source.entity_id.as_str(),
                enemy.position,
            );
            let replace = target.as_ref().is_none_or(|best| {
                candidate
                    .0
                    .total_cmp(&best.0)
                    .then_with(|| candidate.1.cmp(best.1))
                    .is_lt()
            });
            if replace {
                target = Some(candidate);
            }
        }
        let Some((_, _, target_position)) = target else {
            return Ok(());
        };
        let offset = target_position - player_position;
        let velocity = Vec2::new(axis_velocity(offset.x, 2.0), axis_velocity(offset.y, 2.0));
        let projectile_id = state.allocate_projectile_id().ok_or_else(|| {
            BevyError::error(ReferenceSimulationError::ProjectileIdentityOverflow)
        })?;
        commands.spawn((
            Projectile {
                position: player_position,
                velocity,
                damage: weapon.damage,
                ttl_ticks: PROJECTILE_TTL_TICKS,
            },
            projectile_id,
        ));
        weapon.remaining_ticks = weapon.cooldown_ticks.max(1);
        Ok(())
    })();
    state.reject_on_error(result)
}

pub(crate) fn move_wave_projectiles(
    state: Res<WaveState>,
    mut projectiles: Query<&mut Projectile, With<ProjectileId>>,
) {
    if !state.can_simulate() {
        return;
    }
    for mut projectile in &mut projectiles {
        let velocity = projectile.velocity;
        projectile.position += velocity;
        projectile.ttl_ticks = projectile.ttl_ticks.saturating_sub(1);
    }
}

pub(crate) fn resolve_wave_projectile_hits(
    fixed_time: Res<FixedTime>,
    run: Res<WaveRunGeneration>,
    mut state: ResMut<WaveState>,
    mut projectiles: Query<(Entity, &ProjectileId, &mut Projectile)>,
    mut enemies: Query<(Entity, &SceneEntitySource, &WaveSpawn, &mut Enemy)>,
) -> Result<(), BevyError> {
    if !state.can_simulate() {
        return Ok(());
    }
    let run_tick = run
        .run_tick(fixed_time.tick())
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::InvalidRunTick))?;

    let result = (|| {
        let mut target_order = enemies
            .iter()
            .filter(|(_, _, spawn, enemy)| enemy.hit_points > 0 && spawn.tick <= run_tick)
            .map(|(entity, source, _, enemy)| {
                (source.entity_id.as_str().to_owned(), entity, enemy.position)
            })
            .take(MAX_WAVE_ENEMIES + 1)
            .collect::<Vec<_>>();
        if target_order.len() > MAX_WAVE_ENEMIES {
            return Err(BevyError::error(ReferenceSimulationError::TooManyEnemies));
        }
        target_order.sort_by(|left, right| left.0.cmp(&right.0));
        let mut projectile_order = projectiles
            .iter_mut()
            .take(MAX_WAVE_PROJECTILES + 1)
            .collect::<Vec<_>>();
        if projectile_order.len() > MAX_WAVE_PROJECTILES {
            return Err(BevyError::error(
                ReferenceSimulationError::TooManyProjectiles,
            ));
        }
        projectile_order.sort_by_key(|(_, id, _)| id.get());

        for (_, _, mut projectile) in projectile_order {
            if projectile.damage <= 0 || projectile.ttl_ticks == 0 {
                continue;
            }
            let target = target_order.iter().find_map(|(_, entity, position)| {
                (distance_squared(projectile.position, *position) <= 1.0).then_some(*entity)
            });
            let Some(target) = target else {
                continue;
            };
            if let Ok((_, _, _, mut enemy)) = enemies.get_mut(target)
                && enemy.hit_points > 0
            {
                enemy.hit_points = enemy.hit_points.saturating_sub(projectile.damage);
                projectile.ttl_ticks = 0;
            }
        }
        Ok(())
    })();
    state.reject_on_error(result)
}

pub(crate) fn resolve_enemy_contacts(
    fixed_time: Res<FixedTime>,
    run: Res<WaveRunGeneration>,
    mut state: ResMut<WaveState>,
    enemies: Query<(&Enemy, &WaveSpawn), With<SceneEntitySource>>,
    mut players: Query<(&SceneEntitySource, &mut Player)>,
) -> Result<(), BevyError> {
    if !state.can_simulate() {
        return Ok(());
    }
    let run_tick = run
        .run_tick(fixed_time.tick())
        .ok_or_else(|| BevyError::error(ReferenceSimulationError::InvalidRunTick))?;

    let result = (|| {
        let mut player = None;
        for (source, candidate) in &mut players {
            if source.entity_id.as_str() != PLAYER_SCENE_ID {
                continue;
            }
            if player.replace(candidate).is_some() {
                return Err(BevyError::error(ReferenceSimulationError::DuplicatePlayer));
            }
        }
        let Some(mut player) = player else {
            return Err(BevyError::error(ReferenceSimulationError::MissingPlayer));
        };
        for (enemy, spawn) in &enemies {
            if enemy.hit_points > 0
                && spawn.tick <= run_tick
                && distance_squared(player.position, enemy.position) <= 1.0
            {
                player.hit_points = player.hit_points.saturating_sub(ENEMY_CONTACT_DAMAGE);
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
    let mut dead_enemies = world
        .iter_entities()
        .filter_map(|entity| {
            let source = entity.get::<SceneEntitySource>()?;
            entity
                .get::<Enemy>()
                .is_some_and(|enemy| enemy.hit_points <= 0)
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
    let mut expired_projectiles = world
        .iter_entities()
        .filter_map(|entity| {
            let id = entity.get::<ProjectileId>()?;
            entity
                .get::<Projectile>()
                .is_some_and(|projectile| projectile.ttl_ticks == 0)
                .then_some((
                    id.get(),
                    entity.id(),
                    entity.contains::<SceneEntitySource>(),
                ))
        })
        .take(MAX_WAVE_PROJECTILES + 1)
        .collect::<Vec<_>>();
    if expired_projectiles.len() > MAX_WAVE_PROJECTILES {
        return Err(BevyError::error(
            ReferenceSimulationError::TooManyProjectiles,
        ));
    }
    expired_projectiles.sort_by_key(|(id, _, _)| *id);

    for (_, _, entity) in dead_enemies.iter() {
        retire_and_despawn_scene_entity(world, *entity)
            .map_err(|_| BevyError::error(ReferenceSimulationError::IdentityRetirement))?;
    }
    for (_, entity, scene_managed) in expired_projectiles {
        if scene_managed {
            retire_and_despawn_scene_entity(world, entity)
                .map_err(|_| BevyError::error(ReferenceSimulationError::IdentityRetirement))?;
        } else if !world.despawn(entity) {
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
}

pub(crate) fn evaluate_wave_outcome(
    mut state: ResMut<WaveState>,
    players: Query<(&SceneEntitySource, &Player)>,
    enemies: Query<&Enemy, With<SceneEntitySource>>,
) -> Result<(), BevyError> {
    if !state.can_simulate() {
        return Ok(());
    }

    let result = (|| {
        let mut player_hit_points = None;
        for (source, player) in &players {
            if source.entity_id.as_str() != PLAYER_SCENE_ID {
                continue;
            }
            if player_hit_points.replace(player.hit_points).is_some() {
                return Err(BevyError::error(ReferenceSimulationError::DuplicatePlayer));
            }
        }
        let Some(player_hit_points) = player_hit_points else {
            return Err(BevyError::error(ReferenceSimulationError::MissingPlayer));
        };
        let enemies_alive = enemies.iter().any(|enemy| enemy.hit_points > 0);
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
    MissingSceneInstance,
    MultipleSceneInstances,
    TooManySceneEntities,
    IdentitySnapshot,
    SceneMembershipMismatch,
    MissingResetTemplate,
    IdentityReplacement,
    RunGenerationOverflow,
    InvalidRunTick,
    MissingPlayer,
    DuplicatePlayer,
    UnexpectedPlayerIdentity,
    MissingPlayerWeapon,
    MissingEnemy,
    MissingWaveSpawn,
    OrphanWaveSpawn,
    EnemyPopulationChanged,
    InvalidMovementCommand,
    MissingEnemyTarget,
    DuplicateEnemyTarget,
    UnsupportedEnemyTarget,
    IdentityRetirement,
    ProjectileDespawn,
    ScoreOverflow,
    ProgressOverflow,
    ProjectileIdentityOverflow,
    MissingProjectileIdentity,
    OrphanProjectileIdentity,
    TooManyEnemies,
    TooManyProjectiles,
}

impl fmt::Display for ReferenceSimulationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingSceneInstance => "reference wave scene instance is missing",
            Self::MultipleSceneInstances => "reference wave spans multiple scene instances",
            Self::TooManySceneEntities => "reference wave scene entity limit was exceeded",
            Self::IdentitySnapshot => "reference wave scene identity snapshot failed",
            Self::SceneMembershipMismatch => {
                "reference wave scene identity membership is inconsistent"
            }
            Self::MissingResetTemplate => "reference wave reset template is missing",
            Self::IdentityReplacement => "reference wave scene identity replacement failed",
            Self::RunGenerationOverflow => "reference wave run generation overflowed",
            Self::InvalidRunTick => "reference wave run tick is invalid",
            Self::MissingPlayer => "reference wave player is missing",
            Self::DuplicatePlayer => "reference wave player identity is duplicated",
            Self::UnexpectedPlayerIdentity => "reference wave player identity is unsupported",
            Self::MissingPlayerWeapon => "reference wave player weapon is missing",
            Self::MissingEnemy => "reference wave has no enemy roster",
            Self::MissingWaveSpawn => "reference wave enemy has no spawn schedule",
            Self::OrphanWaveSpawn => "reference wave spawn schedule has no enemy",
            Self::EnemyPopulationChanged => "reference wave enemy roster changed during execution",
            Self::InvalidMovementCommand => "reference wave movement command is invalid",
            Self::MissingEnemyTarget => "reference wave enemy target is missing",
            Self::DuplicateEnemyTarget => "reference wave enemy target identity is duplicated",
            Self::UnsupportedEnemyTarget => "reference wave enemy target kind is unsupported",
            Self::IdentityRetirement => "reference wave identity retirement failed",
            Self::ProjectileDespawn => "reference wave projectile retirement failed",
            Self::ScoreOverflow => "reference wave score overflowed",
            Self::ProgressOverflow => "reference wave progress overflowed",
            Self::ProjectileIdentityOverflow => "reference wave projectile identity overflowed",
            Self::MissingProjectileIdentity => {
                "reference wave projectile has no stable runtime identity"
            }
            Self::OrphanProjectileIdentity => {
                "reference wave projectile identity has no projectile"
            }
            Self::TooManyEnemies => "reference wave enemy limit was exceeded",
            Self::TooManyProjectiles => "reference wave projectile limit was exceeded",
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
            &'static SceneEntitySource,
            &'static Player,
            Option<&'static Weapon>,
        ),
    >,
    enemies: Query<'w, 's, (&'static SceneEntitySource, &'static Enemy)>,
    runtime_only: Query<'w, 's, (), With<RuntimeOnlyTag>>,
    unbound_players: Query<'w, 's, (), (With<Player>, Without<SceneEntitySource>)>,
    unbound_enemies: Query<'w, 's, (), (With<Enemy>, Without<SceneEntitySource>)>,
    unbound_weapons: Query<'w, 's, (), (With<Weapon>, Without<SceneEntitySource>)>,
    unbound_projectiles: Query<'w, 's, (), (With<Projectile>, Without<SceneEntitySource>)>,
}

pub(crate) fn capture_project_snapshot(
    fixed_time: Res<FixedTime>,
    queries: ProjectSnapshotQueries,
    mut snapshot: ResMut<ReferenceProjectSnapshot>,
) -> Result<(), BevyError> {
    let mut player = None;
    for (source, candidate, weapon) in &queries.players {
        if source.entity_id.as_str() != PLAYER_SCENE_ID {
            continue;
        }
        if player.replace((candidate, weapon)).is_some() {
            return Err(BevyError::error(ProjectSnapshotError::DuplicatePlayer));
        }
    }
    let Some((player, Some(weapon))) = player else {
        return Err(BevyError::error(ProjectSnapshotError::MissingPlayer));
    };

    let mut enemy = None;
    for (source, candidate) in &queries.enemies {
        if source.entity_id.as_str() != "enemy-anchor/enemy" {
            continue;
        }
        if enemy.replace(candidate).is_some() {
            return Err(BevyError::error(ProjectSnapshotError::DuplicateEnemy));
        }
    }
    let Some(enemy) = enemy else {
        return Err(BevyError::error(ProjectSnapshotError::MissingEnemy));
    };

    snapshot.tick = fixed_time.tick();
    snapshot.player_position = player.position;
    snapshot.player_hit_points = player.hit_points;
    snapshot.enemy_position = enemy.position;
    snapshot.enemy_hit_points = enemy.hit_points;
    snapshot.weapon_remaining_ticks = weapon.remaining_ticks;
    snapshot.runtime_only_entities =
        u64::try_from(queries.runtime_only.iter().count()).unwrap_or(u64::MAX);
    snapshot.unbound_gameplay_components = [
        queries.unbound_players.iter().count(),
        queries.unbound_enemies.iter().count(),
        queries.unbound_weapons.iter().count(),
        queries.unbound_projectiles.iter().count(),
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
}

impl fmt::Display for ProjectSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingPlayer => "project scene player is missing",
            Self::DuplicatePlayer => "project scene player identity is duplicated",
            Self::MissingEnemy => "project scene enemy is missing",
            Self::DuplicateEnemy => "project scene enemy identity is duplicated",
        })
    }
}

impl Error for ProjectSnapshotError {}
