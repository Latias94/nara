use std::{
    collections::HashMap,
    fmt::{self, Display, Formatter},
};

use nara::{
    prelude::*,
    scene::{components::ColliderSource, cooked::PreparedSceneRow},
};

use crate::{
    RunRules,
    scene_roles::{ExtractionGate, PatrolDrone, Pilot, PowerCell},
};

#[cfg(all(feature = "source", any(feature = "desktop", test)))]
pub(crate) mod source;

pub(crate) const CELL_COUNT: usize = 3;
const DRONE_COUNT: usize = 3;
pub(crate) const WALLS: u32 = 1;
pub(crate) const COURIER: u32 = 1 << 1;
const DRONES: u32 = 1 << 2;
const CELLS: u32 = 1 << 3;
const GATE: u32 = 1 << 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Actor {
    Pilot,
    Cell(usize),
    Drone(usize),
    Gate,
}

impl Actor {
    const fn collision_groups(self) -> CollisionGroups2d {
        let (memberships, filters) = match self {
            Self::Pilot => (COURIER, WALLS | DRONES | CELLS | GATE),
            Self::Drone(_) => (DRONES, COURIER),
            Self::Cell(_) => (CELLS, COURIER),
            Self::Gate => (GATE, COURIER),
        };
        CollisionGroups2d {
            memberships,
            filters,
        }
    }
}

impl Display for Actor {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pilot => formatter.write_str("pilot"),
            Self::Cell(index) => write!(formatter, "power cell {}", index + 1),
            Self::Drone(index) => write!(formatter, "patrol drone {}", index + 1),
            Self::Gate => formatter.write_str("extraction gate"),
        }
    }
}

#[derive(Component)]
pub(crate) struct SceneActor {
    pub(crate) role: Actor,
    pub(crate) rotation: f32,
    pub(crate) visibility: Visibility,
}

#[derive(Clone, Copy)]
pub(crate) struct Marker {
    pub(crate) pose: Transform2d,
    pub(crate) radius: f32,
}

#[derive(Resource)]
pub(crate) struct Arena {
    pub(crate) pilot: Marker,
    pub(crate) cells: [Marker; CELL_COUNT],
    pub(crate) gate: Marker,
    pub(crate) patrols: [Patrol; DRONE_COUNT],
}

pub(crate) struct SceneRow<'a> {
    pub name: &'a str,
    pub is_child: bool,
    pub pose: Option<Transform2d>,
    pub visibility: Option<Visibility>,
    pub collider: Option<ColliderSource>,
    pub pilot: bool,
    pub cell_order: Option<u32>,
    pub drone_order: Option<u32>,
    pub gate: bool,
    pub rules: Option<RunRules>,
}

struct BoundRow {
    pose: Transform2d,
    visibility: Visibility,
    collider: Option<Collider2d>,
    actor: Option<Actor>,
}

impl<'a> SceneRow<'a> {
    pub(crate) fn from_entity(entity: nara::ecs::bevy_ecs::world::EntityRef<'a>) -> Self {
        Self {
            name: entity.get::<Name>().map_or("unnamed entity", Name::as_str),
            is_child: entity.contains::<ChildOf>(),
            pose: entity.get::<Transform2d>().copied(),
            visibility: entity.get::<Visibility>().copied(),
            collider: entity.get::<ColliderSource>().copied(),
            pilot: entity.contains::<Pilot>(),
            cell_order: entity.get::<PowerCell>().map(|cell| cell.order),
            drone_order: entity.get::<PatrolDrone>().map(|drone| drone.order),
            gate: entity.contains::<ExtractionGate>(),
            rules: entity.get::<RunRules>().copied(),
        }
    }

    pub(crate) fn from_prepared(row: PreparedSceneRow<'a>) -> Self {
        Self {
            name: row.get::<Name>().map_or("unnamed entity", Name::as_str),
            is_child: row.parent().is_some(),
            pose: row.get::<Transform2d>().copied(),
            visibility: row.get::<Visibility>().copied(),
            collider: row.get::<ColliderSource>().copied(),
            pilot: row.get::<Pilot>().is_some(),
            cell_order: row.get::<PowerCell>().map(|cell| cell.order),
            drone_order: row.get::<PatrolDrone>().map(|drone| drone.order),
            gate: row.get::<ExtractionGate>().is_some(),
            rules: row.get::<RunRules>().copied(),
        }
    }

    fn role(&self) -> Result<Option<Actor>, String> {
        let mut roles = [
            self.pilot.then_some(Actor::Pilot),
            self.cell_order
                .map(|order| role_index(self.name, order, CELL_COUNT).map(Actor::Cell))
                .transpose()?,
            self.drone_order
                .map(|order| role_index(self.name, order, DRONE_COUNT).map(Actor::Drone))
                .transpose()?,
            self.gate.then_some(Actor::Gate),
        ]
        .into_iter()
        .flatten();
        let role = roles.next();
        if roles.next().is_some() {
            return Err(format!("{} has multiple arena roles", self.name));
        }
        Ok(role)
    }

    fn bind(&self) -> Result<BoundRow, String> {
        let actor = self.role()?;
        if actor.is_some() && self.pose.is_none() {
            return Err(format!("{} requires a Transform 2D", self.name));
        }
        if self.is_child && (actor.is_some() || self.collider.is_some()) {
            return Err(format!(
                "{} must be a root to participate in gameplay",
                self.name
            ));
        }
        let pose = self.pose.unwrap_or_default();
        let collider = self
            .collider
            .map(|collider| {
                let scale = pose.scale.abs();
                if let Some(radius) = collider.radius {
                    let radius = radius * scale.max_element();
                    if radius.is_finite() && radius > 0.0 {
                        return Ok(Collider2d::Circle { radius });
                    }
                } else {
                    let half_extents = collider.half_extents * scale;
                    if half_extents.is_finite() && half_extents.min_element() > 0.0 {
                        return Ok(Collider2d::Box { half_extents });
                    }
                }
                Err(format!("{} has invalid collider dimensions", self.name))
            })
            .transpose()?;
        Ok(BoundRow {
            pose,
            visibility: self.visibility.unwrap_or_default(),
            collider,
            actor,
        })
    }
}

fn role_index(name: &str, order: u32, count: usize) -> Result<usize, String> {
    usize::try_from(order)
        .ok()
        .and_then(|order| order.checked_sub(1))
        .filter(|&index| index < count)
        .ok_or_else(|| format!("{name} needs an arena role order in 1–{count}"))
}

pub(crate) struct ArenaPlan {
    pub arena: Arena,
    pub rules: RunRules,
    rows: Vec<BoundRow>,
}

impl ArenaPlan {
    pub fn prepare<'a>(
        rows: impl IntoIterator<Item = Result<SceneRow<'a>, String>>,
    ) -> Result<Self, String> {
        let mut actors = HashMap::with_capacity(2 + CELL_COUNT + DRONE_COUNT);
        let mut bindings = Vec::new();
        let mut rules = None;
        for row in rows {
            let row = row?;
            if let Some(next) = row.rules
                && rules.replace(next).is_some()
            {
                return Err("Night Relay requires exactly one Run Rules component".into());
            }
            let binding = row.bind()?;
            if let Some(role) = binding.actor
                && let Some((previous, _, _)) =
                    actors.insert(role, (row.name, binding.pose, binding.collider))
            {
                return Err(format!(
                    "{previous} and {} both bind the {role} role",
                    row.name
                ));
            }
            bindings.push(binding);
        }
        let rules = rules
            .ok_or("Night Relay requires exactly one Run Rules component")?
            .checked()?;
        let marker = |actor| -> Result<Marker, String> {
            let &(name, pose, collider) = actors
                .get(&actor)
                .ok_or_else(|| format!("Night Relay is missing its {actor} entity"))?;
            let Some(Collider2d::Circle { radius }) = collider else {
                return Err(format!("{name} requires a circular collider"));
            };
            Ok(Marker { pose, radius })
        };
        let arena = Arena {
            pilot: marker(Actor::Pilot)?,
            cells: [
                marker(Actor::Cell(0))?,
                marker(Actor::Cell(1))?,
                marker(Actor::Cell(2))?,
            ],
            gate: marker(Actor::Gate)?,
            patrols: [
                Patrol::new(marker(Actor::Drone(0))?, Vec2::new(128.0, 0.0), 4.0, 0.0),
                Patrol::new(marker(Actor::Drone(1))?, Vec2::new(224.0, 0.0), 6.0, 0.35),
                Patrol::new(marker(Actor::Drone(2))?, Vec2::new(0.0, -96.0), 3.6, 0.85),
            ],
        };
        Ok(Self {
            arena,
            rules,
            rows: bindings,
        })
    }

    pub fn bind(self, world: &mut World, entities: &[Entity]) -> (Arena, RunRules) {
        for (&entity, row) in entities.iter().zip(self.rows) {
            let mut entity = world.entity_mut(entity);
            entity.insert((row.pose, row.visibility));
            if let Some(role) = row.actor {
                entity.insert((
                    role.collision_groups(),
                    SceneActor {
                        role,
                        rotation: row.pose.rotation,
                        visibility: row.visibility,
                    },
                ));
                if let Some(collider) = row.collider {
                    let body = match role {
                        Actor::Pilot | Actor::Drone(_) => RigidBody2d::Kinematic,
                        Actor::Cell(_) | Actor::Gate => RigidBody2d::Static,
                    };
                    entity.insert((collider, body, Sensor2d));
                }
            } else if let Some(collider) = row.collider {
                entity.insert((
                    collider,
                    CollisionGroups2d {
                        memberships: WALLS,
                        filters: COURIER,
                    },
                ));
            }
        }
        (self.arena, self.rules)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Patrol {
    marker: Marker,
    offset: Vec2,
    period: f32,
    phase: f32,
}

impl Patrol {
    const fn new(marker: Marker, offset: Vec2, period: f32, phase: f32) -> Self {
        Self {
            marker,
            offset,
            period,
            phase,
        }
    }

    pub(crate) fn radius(self) -> f32 {
        self.marker.radius
    }

    pub(crate) fn position(self, elapsed_seconds: f32) -> Vec2 {
        self.marker.pose.translation
            + self.offset * (self.fraction(elapsed_seconds) - self.fraction(0.0))
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn endpoints(self) -> (Vec2, Vec2) {
        let start = self.marker.pose.translation - self.offset * self.fraction(0.0);
        (start, start + self.offset)
    }

    fn fraction(self, elapsed_seconds: f32) -> f32 {
        let phase = (elapsed_seconds / self.period + self.phase).fract();
        1.0 - (2.0 * phase - 1.0).abs()
    }
}
