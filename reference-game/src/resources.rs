use std::{collections::BTreeMap, error::Error, fmt};

use nara::{
    advanced_prelude::StartupSceneSourceView,
    ecs::Entity,
    identity::{SceneEntityId, SpawnedSceneInstance, WorldEntityToken},
    prelude::{Component, Resource, Vec2},
    scene::{SceneEntitySource, advanced::SceneProductResource},
};

use crate::snapshot::WaveOutcome;

pub(crate) const MOVE_COMMAND_TYPE: &str = "reference-game.move-v2";
pub(crate) const MOVE_X_FIELD: &str = "x";
pub(crate) const MOVE_Y_FIELD: &str = "y";
pub(crate) const MOVE_PRESSED_FIELD: &str = "pressed";
pub(crate) const RETRY_COMMAND_TYPE: &str = "reference-game.retry-v1";
pub(crate) const PLAYER_SCENE_ID: &str = "player";
pub(crate) const PLAYER_WEAPON_SCENE_ID: &str = "player-weapon";
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

    pub(crate) const fn from_velocity(x: i64, y: i64) -> Option<Self> {
        match (x, y) {
            (-1, 0) => Some(Self::Left),
            (1, 0) => Some(Self::Right),
            (0, 1) => Some(Self::Up),
            (0, -1) => Some(Self::Down),
            (0, 0) => Some(Self::Stop),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub(crate) struct MovementIntent {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

impl MovementIntent {
    pub(crate) fn apply(&mut self, direction: MovementDirection, pressed: bool) {
        match direction {
            MovementDirection::Left => self.left = pressed,
            MovementDirection::Right => self.right = pressed,
            MovementDirection::Up => self.up = pressed,
            MovementDirection::Down => self.down = pressed,
            MovementDirection::Stop => *self = Self::default(),
        }
    }

    pub(crate) fn velocity(self) -> Vec2 {
        let x = self.right as i8 - self.left as i8;
        let y = self.up as i8 - self.down as i8;
        let scale = if x != 0 && y != 0 {
            std::f32::consts::FRAC_1_SQRT_2
        } else {
            1.0
        };
        Vec2::new(f32::from(x) * scale, f32::from(y) * scale)
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
/// Bounded reason for rejecting a requested in-runtime wave retry.
pub enum WaveRetryRejection {
    /// The current wave has not reached a terminal state.
    WhileRunning,
    /// The same run generation already has a pending retry request.
    AlreadyPending,
    /// The run generation counter cannot advance without overflow.
    GenerationExhausted,
    /// The retained authored source cannot initialize a valid run.
    InvalidAuthoredConfiguration,
    /// The prepared scene and product-state replacement was rejected atomically.
    ReplacementRejected,
}

/// Inspectable game-owned retry state; it never requests platform or runtime replacement.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
pub struct WaveRetryStatus {
    pending_generation: Option<u64>,
    applied_generation: Option<u64>,
    last_rejection: Option<WaveRetryRejection>,
}

impl WaveRetryStatus {
    #[must_use]
    pub const fn phase(self) -> WaveRetryPhase {
        if self.pending_generation.is_some() {
            WaveRetryPhase::Pending
        } else if self.applied_generation.is_some() {
            WaveRetryPhase::Applied
        } else {
            WaveRetryPhase::Idle
        }
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
        self.pending_generation = Some(generation);
        self.last_rejection = None;
        true
    }

    pub(crate) fn reject_while_running(&mut self) {
        self.reject(WaveRetryRejection::WhileRunning);
    }

    pub(crate) fn reject_duplicate(&mut self) {
        self.last_rejection = Some(WaveRetryRejection::AlreadyPending);
    }

    pub(crate) fn reject(&mut self, rejection: WaveRetryRejection) {
        self.pending_generation = None;
        self.last_rejection = Some(rejection);
    }

    pub(crate) fn mark_applied(
        &mut self,
        generation: u64,
        last_rejection: Option<WaveRetryRejection>,
    ) {
        self.pending_generation = None;
        self.applied_generation = Some(generation);
        self.last_rejection = last_rejection;
    }
}

impl SceneProductResource for WaveRetryStatus {}

#[derive(Debug, Resource)]
pub(crate) struct WaveRunOwner {
    source: StartupSceneSourceView,
    receipt: SpawnedSceneInstance,
    scene_entities: BTreeMap<SceneEntityId, Entity>,
    projectiles: Vec<WorldEntityToken>,
    baseline: WaveRunBaseline,
}

impl WaveRunOwner {
    pub(crate) fn new(
        source: StartupSceneSourceView,
        receipt: SpawnedSceneInstance,
        scene_entities: BTreeMap<SceneEntityId, Entity>,
        baseline: WaveRunBaseline,
    ) -> Self {
        Self {
            source,
            receipt,
            scene_entities,
            projectiles: Vec::new(),
            baseline,
        }
    }

    pub(crate) fn source(&self) -> &StartupSceneSourceView {
        &self.source
    }

    pub(crate) fn receipt(&self) -> &SpawnedSceneInstance {
        &self.receipt
    }

    pub(crate) fn projectile_tokens(&self) -> &[WorldEntityToken] {
        &self.projectiles
    }

    pub(crate) fn owns_scene_entity(&self, entity: Entity, source: &SceneEntitySource) -> bool {
        source.instance_id == self.receipt.instance_id()
            && self.scene_entities.get(&source.entity_id) == Some(&entity)
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn owns_projectile_entity(&self, entity: nara::ecs::Entity) -> bool {
        self.projectiles
            .iter()
            .any(|token| token.entity() == entity)
    }

    pub(crate) fn record_projectile(&mut self, token: WorldEntityToken) -> bool {
        if self.projectiles.len() >= MAX_WAVE_PROJECTILES || self.projectiles.contains(&token) {
            return false;
        }
        self.projectiles.push(token);
        true
    }

    pub(crate) fn forget_projectile(&mut self, token: WorldEntityToken) -> bool {
        let Some(index) = self
            .projectiles
            .iter()
            .position(|candidate| *candidate == token)
        else {
            return false;
        };
        self.projectiles.swap_remove(index);
        true
    }

    pub(crate) fn publish_replacement(
        &mut self,
        receipt: SpawnedSceneInstance,
        scene_entities: BTreeMap<SceneEntityId, Entity>,
        baseline: WaveRunBaseline,
    ) {
        self.receipt = receipt;
        self.scene_entities = scene_entities;
        self.projectiles.clear();
        self.baseline = baseline;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WaveRunBaseline {
    pub(crate) movement: MovementIntent,
    pub(crate) generation: WaveRunGeneration,
    pub(crate) retry: WaveRetryStatus,
    pub(crate) state: WaveState,
}

#[derive(Debug, Clone, Resource)]
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
    ProjectionOnly,
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

    pub(crate) fn begin_projection_only_tick(&mut self) {
        self.tick_gate = if self.is_running() {
            WaveTickGate::ProjectionOnly
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
        matches!(
            self.tick_gate,
            WaveTickGate::ProjectionOnly | WaveTickGate::Admitted
        )
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
}

impl SceneProductResource for MovementIntent {}
impl SceneProductResource for WaveRunGeneration {}
impl SceneProductResource for WaveState {}
