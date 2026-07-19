use std::{error::Error, fmt};

use nara::prelude::{Component, Resource};

use crate::snapshot::WaveOutcome;

pub(crate) const MOVE_COMMAND_TYPE: &str = "reference-game.move-v1";
pub(crate) const MOVE_X_FIELD: &str = "x";
pub(crate) const MOVE_Y_FIELD: &str = "y";
pub(crate) const PLAYER_SCENE_ID: &str = "player";
pub(crate) const COMMAND_SOURCE: &str = "reference-game.bundled";
pub(crate) const PROJECTILE_TTL_TICKS: u64 = 64;
pub(crate) const ENEMY_CONTACT_DAMAGE: i64 = 10;
pub(crate) const KILL_SCORE: u64 = 100;
pub(crate) const MAX_WAVE_ENEMIES: usize = 64;
pub(crate) const MAX_WAVE_PROJECTILES: usize = 128;

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

    pub(crate) fn allocate_projectile_ids(
        &mut self,
        count: usize,
    ) -> Option<Vec<ProjectileId>> {
        let count = u64::try_from(count).ok()?;
        let first = self.next_projectile_id;
        let next = first.checked_add(count)?;
        self.next_projectile_id = next;
        Some((first..next).map(ProjectileId).collect())
    }
}
