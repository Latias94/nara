use std::{error::Error, fmt};

use nara::{
    ecs::{With, Without, error::BevyError},
    gameplay::GameplayCommandBatch,
    prelude::{Commands, FixedTime, Query, Res, ResMut},
    scene::SceneEntitySource,
};

use crate::{
    Enemy, Player, Projectile, ReferenceProjectSnapshot, RuntimeOnlyTag, Weapon,
};

pub(crate) fn seed_tracer(mut commands: Commands) {
    commands.spawn((Player::fixture(), RuntimeOnlyTag));
    commands.spawn(Enemy::fixture());
    commands.spawn(Weapon::fixture());
    commands.spawn(Projectile::fixture());
}

pub(crate) fn move_players(mut players: Query<&mut Player>) {
    for mut player in &mut players {
        let velocity = player.velocity;
        player.position += velocity;
    }
}

pub(crate) fn move_enemies(mut enemies: Query<&mut Enemy>) {
    for mut enemy in &mut enemies {
        let velocity = enemy.velocity;
        enemy.position += velocity;
    }
}

pub(crate) fn tick_weapons(mut weapons: Query<&mut Weapon>) {
    for mut weapon in &mut weapons {
        weapon.remaining_ticks = weapon.remaining_ticks.saturating_sub(1);
    }
}

pub(crate) fn move_projectiles(mut projectiles: Query<&mut Projectile>) {
    for mut projectile in &mut projectiles {
        let velocity = projectile.velocity;
        projectile.position += velocity;
        projectile.ttl_ticks = projectile.ttl_ticks.saturating_sub(1);
    }
}

pub(crate) fn resolve_projectile_hits(
    projectiles: Query<&Projectile>,
    mut enemies: Query<&mut Enemy>,
) {
    for projectile in &projectiles {
        if projectile.damage <= 0 {
            continue;
        }
        for mut enemy in &mut enemies {
            if projectile.position == enemy.position {
                enemy.hit_points = enemy.hit_points.saturating_sub(projectile.damage);
            }
        }
    }
}

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

pub(crate) fn capture_project_snapshot(
    fixed_time: Res<FixedTime>,
    players: Query<(&SceneEntitySource, &Player, Option<&Weapon>)>,
    enemies: Query<(&SceneEntitySource, &Enemy)>,
    runtime_only: Query<(), With<RuntimeOnlyTag>>,
    unbound_players: Query<(), (With<Player>, Without<SceneEntitySource>)>,
    unbound_enemies: Query<(), (With<Enemy>, Without<SceneEntitySource>)>,
    unbound_weapons: Query<(), (With<Weapon>, Without<SceneEntitySource>)>,
    unbound_projectiles: Query<(), (With<Projectile>, Without<SceneEntitySource>)>,
    mut snapshot: ResMut<ReferenceProjectSnapshot>,
) -> Result<(), BevyError> {
    let mut player = None;
    for (source, candidate, weapon) in &players {
        if source.entity_id.as_str() != "player" {
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
    for (source, candidate) in &enemies {
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
    snapshot.runtime_only_entities = u64::try_from(runtime_only.iter().count()).unwrap_or(u64::MAX);
    snapshot.unbound_gameplay_components = [
        unbound_players.iter().count(),
        unbound_enemies.iter().count(),
        unbound_weapons.iter().count(),
        unbound_projectiles.iter().count(),
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
