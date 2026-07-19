use std::{error::Error, fmt};

use nara::ecs as bevy_ecs;
use nara::{
    app::RuntimeFaultReporter,
    ecs::{error::BevyError, system::SystemParam},
    prelude::{FixedTime, Query, Res, ResMut, Resource, Vec2},
    scene::SceneEntitySource,
};

use crate::{
    Enemy, Player, Projectile, WaveSpawn,
    resources::{
        MAX_WAVE_ENEMIES, MAX_WAVE_PROJECTILES, PLAYER_SCENE_ID, ProjectileId, WaveRunGeneration,
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WaveOutcome {
    #[default]
    Running,
    Completed,
    Defeated,
}

impl WaveOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Defeated => "defeated",
        }
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PlayerSnapshot {
    pub id: String,
    pub position: Vec2,
    pub hit_points: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnemySnapshot {
    pub id: String,
    pub position: Vec2,
    pub hit_points: i64,
    pub spawn_tick: u64,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectileSnapshot {
    pub id: u64,
    pub position: Vec2,
    pub velocity: Vec2,
    pub ttl_ticks: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Resource)]
pub struct WaveSnapshot {
    pub run_generation: u64,
    pub tick: u64,
    pub outcome: WaveOutcome,
    pub score: u64,
    pub planned_enemies: u64,
    pub defeated_enemies: u64,
    pub player: PlayerSnapshot,
    pub enemies: Vec<EnemySnapshot>,
    pub projectiles: Vec<ProjectileSnapshot>,
}

impl WaveSnapshot {
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        self.outcome.is_terminal()
    }
}

#[derive(SystemParam)]
pub(crate) struct WaveSnapshotQueries<'w, 's> {
    players: Query<'w, 's, (&'static SceneEntitySource, &'static Player)>,
    enemies: Query<
        'w,
        's,
        (
            &'static SceneEntitySource,
            &'static Enemy,
            &'static WaveSpawn,
        ),
    >,
    projectiles: Query<'w, 's, (&'static ProjectileId, &'static Projectile)>,
}

pub(crate) fn capture_wave_snapshot(
    fixed_time: Res<FixedTime>,
    run: Res<WaveRunGeneration>,
    state: Res<crate::resources::WaveState>,
    faults: Res<RuntimeFaultReporter>,
    queries: WaveSnapshotQueries<'_, '_>,
    mut snapshot: ResMut<WaveSnapshot>,
) -> Result<(), BevyError> {
    if !state.can_publish_snapshot() || faults.fault().is_some() {
        return Ok(());
    }

    let mut player = None;
    for (source, candidate) in &queries.players {
        if source.entity_id.as_str() != PLAYER_SCENE_ID {
            continue;
        }
        if player.replace((source, candidate)).is_some() {
            return Err(BevyError::error(WaveSnapshotError::DuplicatePlayer));
        }
    }
    let Some((source, player)) = player else {
        return Err(BevyError::error(WaveSnapshotError::MissingPlayer));
    };
    let run_tick = run
        .run_tick(fixed_time.tick())
        .ok_or_else(|| BevyError::error(WaveSnapshotError::InvalidRunTick))?;

    let mut enemy_values = queries
        .enemies
        .iter()
        .map(|(source, enemy, spawn)| EnemySnapshot {
            id: source.entity_id.as_str().to_owned(),
            position: enemy.position,
            hit_points: enemy.hit_points,
            spawn_tick: spawn.tick,
            active: spawn.tick <= run_tick,
        })
        .take(MAX_WAVE_ENEMIES + 1)
        .collect::<Vec<_>>();
    if enemy_values.len() > MAX_WAVE_ENEMIES {
        return Err(BevyError::error(WaveSnapshotError::TooManyEnemies));
    }
    enemy_values.sort_by(|left, right| left.id.cmp(&right.id));

    let mut projectile_values = queries
        .projectiles
        .iter()
        .map(|(id, projectile)| ProjectileSnapshot {
            id: id.get(),
            position: projectile.position,
            velocity: projectile.velocity,
            ttl_ticks: projectile.ttl_ticks,
        })
        .take(MAX_WAVE_PROJECTILES + 1)
        .collect::<Vec<_>>();
    if projectile_values.len() > MAX_WAVE_PROJECTILES {
        return Err(BevyError::error(WaveSnapshotError::TooManyProjectiles));
    }
    projectile_values.sort_by_key(|projectile| projectile.id);

    snapshot.run_generation = run.get();
    snapshot.tick = run_tick;
    snapshot.outcome = state.outcome;
    snapshot.score = state.score;
    snapshot.planned_enemies = state.planned_enemies.unwrap_or_default();
    snapshot.defeated_enemies = state.defeated_enemies;
    snapshot.player = PlayerSnapshot {
        id: source.entity_id.as_str().to_owned(),
        position: player.position,
        hit_points: player.hit_points,
    };
    snapshot.enemies = enemy_values;
    snapshot.projectiles = projectile_values;
    Ok(())
}

#[derive(Debug)]
enum WaveSnapshotError {
    InvalidRunTick,
    MissingPlayer,
    DuplicatePlayer,
    TooManyEnemies,
    TooManyProjectiles,
}

impl fmt::Display for WaveSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRunTick => "reference wave run tick is invalid",
            Self::MissingPlayer => "reference wave player is missing",
            Self::DuplicatePlayer => "reference wave player identity is duplicated",
            Self::TooManyEnemies => "reference wave enemy snapshot limit was exceeded",
            Self::TooManyProjectiles => "reference wave projectile snapshot limit was exceeded",
        })
    }
}

impl Error for WaveSnapshotError {}
