use nara::prelude::{Component, PersistentComponent, Vec2};

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "reference_game.PlayerRole",
    version = 1,
    alias = "Player role",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
/// Authored marker identifying the player scene entity.
pub struct PlayerRole {}

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "reference_game.EnemyRole",
    version = 1,
    alias = "Enemy role",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
/// Authored marker identifying an enemy scene entity.
pub struct EnemyRole {}

#[derive(Component, PersistentComponent, Debug, Clone, Copy, PartialEq, Eq)]
#[nara(
    id = "reference_game.InitialHealth",
    version = 1,
    alias = "Initial health",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
/// Authored health used to initialize one run generation.
pub struct InitialHealth {
    /// Hit points assigned when the run generation is initialized.
    #[nara(id = "hit-points", alias = "Hit points")]
    pub hit_points: i64,
}

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "reference_game.InitialVelocity2d",
    version = 1,
    alias = "Initial velocity 2D",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
/// Authored velocity used to initialize one run generation.
pub struct InitialVelocity2d {
    /// Local-space velocity assigned when the run generation is initialized.
    #[nara(id = "velocity", alias = "Velocity")]
    pub velocity: Vec2,
}

#[derive(Component, PersistentComponent, Debug, Clone, Copy, PartialEq, Eq)]
#[nara(
    id = "reference_game.WaveSpawn",
    version = 1,
    alias = "Wave spawn",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
/// Authored fixed tick at which an enemy joins the active wave.
pub struct WaveSpawn {
    /// Run-relative fixed tick at which the entity becomes active.
    #[nara(id = "tick", alias = "Tick")]
    pub tick: u64,
}

impl WaveSpawn {
    #[must_use]
    pub const fn fixture() -> Self {
        Self { tick: 1 }
    }
}

#[derive(Component, PersistentComponent, Debug, Clone, Copy, PartialEq, Eq)]
#[nara(
    id = "reference_game.Weapon",
    version = 2,
    alias = "Weapon",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit),
    tombstone = "remaining-ticks"
)]
/// Authored weapon configuration; mutable cooldown state is runtime-only.
pub struct Weapon {
    /// Positive number of fixed ticks between automatic shots.
    #[nara(id = "cooldown-ticks", alias = "Cooldown ticks")]
    pub cooldown_ticks: u64,
    /// Positive damage applied by each projectile.
    #[nara(id = "damage", alias = "Damage")]
    pub damage: i64,
}

impl Weapon {
    #[must_use]
    pub const fn fixture() -> Self {
        Self {
            cooldown_ticks: 3,
            damage: 3,
        }
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
/// Runtime health for the current run generation.
pub struct Health {
    /// Current hit points, which may change during simulation.
    pub current: i64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
/// Runtime velocity for the current run generation.
pub struct Velocity2d {
    /// Translation delta applied by one fixed simulation tick.
    pub value: Vec2,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
/// Mutable runtime cooldown for an authored [`Weapon`].
pub struct WeaponCooldown {
    /// Fixed ticks remaining before the weapon may fire again.
    pub remaining_ticks: u64,
}

#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
/// Runtime-only marker identifying a projectile owned by the current run.
pub struct ProjectileRole;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
/// Runtime-only projectile damage derived from authored weapon configuration.
pub struct ProjectileDamage {
    /// Damage applied on a successful hit.
    pub amount: i64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
/// Runtime-only projectile lifetime.
pub struct ProjectileLifetime {
    /// Fixed ticks remaining before retirement.
    pub remaining_ticks: u64,
}

#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
/// Test-facing marker proving runtime-only components never enter persistent schema.
pub struct RuntimeOnlyTag;
