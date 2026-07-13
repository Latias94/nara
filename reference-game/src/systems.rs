use nara::prelude::{Commands, Query};

use crate::{Enemy, Player, Projectile, RuntimeOnlyTag, Weapon};

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
