use std::{error::Error, fmt};

use nara::{
    prelude::{Component, Resource},
    scene::{SceneEntityId, SpawnedSceneInstance},
};

use crate::{Enemy, Player, Projectile, WaveSpawn, Weapon, snapshot::WaveOutcome};

pub(crate) const MOVE_COMMAND_TYPE: &str = "reference-game.move-v1";
pub(crate) const MOVE_X_FIELD: &str = "x";
pub(crate) const MOVE_Y_FIELD: &str = "y";
pub(crate) const RETRY_COMMAND_TYPE: &str = "reference-game.retry-v1";
pub(crate) const PLAYER_SCENE_ID: &str = "player";
pub(crate) const COMMAND_SOURCE: &str = "reference-game.bundled";
pub(crate) const PROJECTILE_TTL_TICKS: u64 = 64;
pub(crate) const ENEMY_CONTACT_DAMAGE: i64 = 10;
pub(crate) const KILL_SCORE: u64 = 100;
pub(crate) const MAX_WAVE_ENEMIES: usize = 64;
pub(crate) const MAX_WAVE_PROJECTILES: usize = 128;
pub(crate) const MAX_WAVE_SCENE_ENTITIES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementDirection {
    Left,
    Right,
    Up,
    Down,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementCommandError {
    ZeroTick,
    ZeroSequence,
}

impl fmt::Display for MovementCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroTick => "movement command tick must be non-zero",
            Self::ZeroSequence => "movement command sequence must be non-zero",
        })
    }
}

impl Error for MovementCommandError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryCommandError {
    ZeroTick,
    ZeroSequence,
}

impl fmt::Display for RetryCommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ZeroTick => "retry command tick must be non-zero",
            Self::ZeroSequence => "retry command sequence must be non-zero",
        })
    }
}

impl Error for RetryCommandError {}

impl MovementDirection {
    pub(crate) const fn velocity(self) -> (i64, i64) {
        match self {
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
            Self::Up => (0, 1),
            Self::Down => (0, -1),
            Self::Stop => (0, 0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Component)]
pub struct ProjectileId(pub(crate) u64);

impl ProjectileId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Monotonic game-owned run generation within one engine runtime generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub struct WaveRunGeneration {
    generation: u64,
    start_fixed_tick: u64,
}

impl Default for WaveRunGeneration {
    fn default() -> Self {
        Self {
            generation: 1,
            start_fixed_tick: 0,
        }
    }
}

impl WaveRunGeneration {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn run_tick(self, fixed_tick: u64) -> Option<u64> {
        fixed_tick.checked_sub(self.start_fixed_tick)
    }

    pub(crate) fn advance_for_reset(&mut self, fixed_tick: u64) -> Option<()> {
        let generation = self.generation.checked_add(1)?;
        let start_fixed_tick = fixed_tick.checked_sub(1)?;
        self.generation = generation;
        self.start_fixed_tick = start_fixed_tick;
        Some(())
    }

}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WaveRetryPhase {
    #[default]
    Idle,
    Pending,
    Applied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveRetryRejection {
    WhileRunning,
    AlreadyPending,
}

/// Inspectable game-owned retry state; it never requests platform or runtime replacement.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub struct WaveRetryStatus {
    phase: WaveRetryPhase,
    pending_generation: Option<u64>,
    applied_generation: Option<u64>,
    last_rejection: Option<WaveRetryRejection>,
}

impl WaveRetryStatus {
    #[must_use]
    pub const fn phase(self) -> WaveRetryPhase {
        self.phase
    }

    #[must_use]
    pub const fn pending_generation(self) -> Option<u64> {
        self.pending_generation
    }

    #[must_use]
    pub const fn applied_generation(self) -> Option<u64> {
        self.applied_generation
    }

    #[must_use]
    pub const fn last_rejection(self) -> Option<WaveRetryRejection> {
        self.last_rejection
    }

    pub(crate) fn mark_pending(&mut self, generation: u64) -> bool {
        if self.pending_generation == Some(generation) {
            self.last_rejection = Some(WaveRetryRejection::AlreadyPending);
            return false;
        }
        self.phase = WaveRetryPhase::Pending;
        self.pending_generation = Some(generation);
        self.last_rejection = None;
        true
    }

    pub(crate) fn reject_while_running(&mut self) {
        self.last_rejection = Some(WaveRetryRejection::WhileRunning);
    }

    pub(crate) fn reject_duplicate(&mut self) {
        self.last_rejection = Some(WaveRetryRejection::AlreadyPending);
    }

    pub(crate) fn mark_applied(&mut self, generation: u64) {
        self.phase = WaveRetryPhase::Applied;
        self.pending_generation = None;
        self.applied_generation = Some(generation);
    }
}

#[derive(Debug, Default, Resource)]
pub(crate) struct WaveResetTemplate {
    pub(crate) scene: Option<WaveSceneTemplate>,
}

#[derive(Debug)]
pub(crate) struct WaveSceneTemplate {
    pub(crate) instance: SpawnedSceneInstance,
    pub(crate) entities: Vec<WaveEntityTemplate>,
}

#[derive(Debug, Clone)]
pub(crate) struct WaveEntityTemplate {
    pub(crate) id: SceneEntityId,
    pub(crate) player: Option<Player>,
    pub(crate) enemy: Option<Enemy>,
    pub(crate) spawn: Option<WaveSpawn>,
    pub(crate) weapon: Option<Weapon>,
    pub(crate) projectile: Option<Projectile>,
}

#[derive(Debug, Resource)]
pub(crate) struct WaveState {
    pub(crate) outcome: WaveOutcome,
    pub(crate) score: u64,
    pub(crate) planned_enemies: Option<u64>,
    pub(crate) defeated_enemies: u64,
    pub(crate) next_projectile_id: u64,
    tick_gate: WaveTickGate,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum WaveTickGate {
    #[default]
    Pending,
    Admitted,
    Rejected,
}

impl Default for WaveState {
    fn default() -> Self {
        Self {
            outcome: WaveOutcome::Running,
            score: 0,
            planned_enemies: None,
            defeated_enemies: 0,
            next_projectile_id: 1,
            tick_gate: WaveTickGate::Pending,
        }
    }
}

impl WaveState {
    pub(crate) const fn is_running(&self) -> bool {
        matches!(self.outcome, WaveOutcome::Running)
    }

    pub(crate) fn begin_tick(&mut self) {
        self.tick_gate = if self.is_running() {
            WaveTickGate::Pending
        } else {
            WaveTickGate::Rejected
        };
    }

    pub(crate) const fn tick_is_pending(&self) -> bool {
        self.is_running() && matches!(self.tick_gate, WaveTickGate::Pending)
    }

    pub(crate) const fn can_simulate(&self) -> bool {
        self.is_running() && matches!(self.tick_gate, WaveTickGate::Admitted)
    }

    pub(crate) const fn can_publish_snapshot(&self) -> bool {
        matches!(self.tick_gate, WaveTickGate::Admitted)
    }

    pub(crate) fn admit_tick(&mut self) {
        if self.tick_is_pending() {
            self.tick_gate = WaveTickGate::Admitted;
        }
    }

    pub(crate) fn reject_tick(&mut self) {
        self.tick_gate = WaveTickGate::Rejected;
    }

    pub(crate) fn reject_on_error<T, E>(&mut self, result: Result<T, E>) -> Result<T, E> {
        if result.is_err() {
            self.reject_tick();
        }
        result
    }

    pub(crate) fn allocate_projectile_id(&mut self) -> Option<ProjectileId> {
        let id = ProjectileId(self.next_projectile_id);
        self.next_projectile_id = self.next_projectile_id.checked_add(1)?;
        Some(id)
    }

    pub(crate) fn allocate_projectile_ids(&mut self, count: usize) -> Option<Vec<ProjectileId>> {
        let count = u64::try_from(count).ok()?;
        let first = self.next_projectile_id;
        let next = first.checked_add(count)?;
        self.next_projectile_id = next;
        Some((first..next).map(ProjectileId).collect())
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}
