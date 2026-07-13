use nara::prelude::{Component, EntityReference, PersistentComponent, SceneEntityId, Vec2};

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "reference_game.Player",
    version = 1,
    alias = "Player",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct Player {
    #[nara(id = "position", alias = "Position")]
    pub position: Vec2,
    #[nara(id = "velocity", alias = "Velocity")]
    pub velocity: Vec2,
    #[nara(id = "hit-points", alias = "Hit points")]
    pub hit_points: i64,
}

impl Player {
    #[must_use]
    pub fn fixture() -> Self {
        Self {
            position: Vec2::ZERO,
            velocity: Vec2::X,
            hit_points: 20,
        }
    }
}

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "reference_game.Enemy",
    version = 1,
    alias = "Enemy",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct Enemy {
    #[nara(id = "position", alias = "Position")]
    pub position: Vec2,
    #[nara(id = "velocity", alias = "Velocity")]
    pub velocity: Vec2,
    #[nara(id = "hit-points", alias = "Hit points")]
    pub hit_points: i64,
    #[nara(
        id = "target",
        alias = "Target",
        capabilities(scene, inspect, edit, entity_ref)
    )]
    pub target: EntityReference,
}

impl Enemy {
    #[must_use]
    pub fn fixture() -> Self {
        Self {
            position: Vec2::new(5.0, 0.0),
            velocity: Vec2::new(-0.5, 0.0),
            hit_points: 10,
            target: EntityReference::SceneLocal {
                entity: SceneEntityId::new("player")
                    .expect("the reference-game player fixture ID is valid"),
            },
        }
    }
}

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "reference_game.Weapon",
    version = 1,
    alias = "Weapon",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct Weapon {
    #[nara(id = "cooldown-ticks", alias = "Cooldown ticks")]
    pub cooldown_ticks: u64,
    #[nara(id = "remaining-ticks", alias = "Remaining ticks")]
    pub remaining_ticks: u64,
    #[nara(id = "damage", alias = "Damage")]
    pub damage: i64,
}

impl Weapon {
    #[must_use]
    pub fn fixture() -> Self {
        Self {
            cooldown_ticks: 3,
            remaining_ticks: 3,
            damage: 3,
        }
    }
}

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "reference_game.Projectile",
    version = 1,
    alias = "Projectile",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct Projectile {
    #[nara(id = "position", alias = "Position")]
    pub position: Vec2,
    #[nara(id = "velocity", alias = "Velocity")]
    pub velocity: Vec2,
    #[nara(id = "damage", alias = "Damage")]
    pub damage: i64,
    #[nara(id = "ttl-ticks", alias = "TTL ticks")]
    pub ttl_ticks: u64,
}

impl Projectile {
    #[must_use]
    pub fn fixture() -> Self {
        Self {
            position: Vec2::ZERO,
            velocity: Vec2::new(2.0, 0.0),
            damage: 3,
            ttl_ticks: 4,
        }
    }
}

#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeOnlyTag;
