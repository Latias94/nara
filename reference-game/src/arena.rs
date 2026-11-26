use std::collections::HashMap;

use nara::{core::EntityUuid, prelude::*, reflect::PersistentComponentProvider};
use nara_scene::authoring::{ColliderSource, SceneDocument, SceneEntity, SpriteSource};

use crate::RunRules;

const PILOT_ID: u128 = 0x4f99_b200_2a5e_4872_8d3d_0000_0000_0001;
const CELL_IDS: [u128; 3] = [PILOT_ID + 1, PILOT_ID + 2, PILOT_ID + 3];
const DRONE_IDS: [u128; 3] = [PILOT_ID + 4, PILOT_ID + 5, PILOT_ID + 6];
const GATE_ID: u128 = PILOT_ID + 7;
pub(crate) const CELL_COUNT: usize = CELL_IDS.len();
#[cfg(feature = "desktop")]
pub(crate) const WIDTH: f32 = 672.0;

pub(crate) fn bundled_scene() -> Result<SceneDocument, nara_scene::authoring::SceneDocumentError> {
    SceneDocument::parse(include_str!("../assets/night-relay.nara.scene.ron"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Actor {
    Pilot,
    Cell(usize),
    Drone(usize),
    Gate,
}

impl Actor {
    fn from_id(id: EntityUuid) -> Option<Self> {
        let id = id.as_uuid().as_u128();
        match id {
            PILOT_ID => Some(Self::Pilot),
            GATE_ID => Some(Self::Gate),
            _ => CELL_IDS
                .iter()
                .position(|&known| known == id)
                .map(Self::Cell)
                .or_else(|| {
                    DRONE_IDS
                        .iter()
                        .position(|&known| known == id)
                        .map(Self::Drone)
                }),
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
    pub(crate) patrols: [Patrol; DRONE_IDS.len()],
}

struct Spawn<'a> {
    source: &'a SceneEntity,
    pose: Transform2d,
    visibility: Visibility,
    sprite: Option<SpriteSource>,
    collider: Option<Collider2d>,
    actor: Option<Actor>,
    rules: Option<RunRules>,
}

impl Spawn<'_> {
    fn component<T: PersistentComponentProvider>(
        source: &SceneEntity,
    ) -> Result<Option<T>, String> {
        source.typed::<T>().map_err(|error| {
            format!(
                "{} ({}) {}: {error}",
                source.name,
                source.id,
                T::COMPONENT_ID
            )
        })
    }

    fn read(source: &SceneEntity) -> Result<Spawn<'_>, String> {
        let actor = Actor::from_id(source.id);
        let pose = Self::component::<Transform2d>(source)?;
        let collider = Self::component::<ColliderSource>(source)?;
        if actor.is_some() && pose.is_none() {
            return Err(format!("{} requires a Transform 2D", source.name));
        }
        if source.parent.is_some() && (actor.is_some() || collider.is_some()) {
            return Err(format!(
                "{} must be a root to participate in gameplay",
                source.name
            ));
        }
        let pose = pose.unwrap_or_default();
        let collider = collider
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
                Err(format!("{} has invalid collider dimensions", source.name))
            })
            .transpose()?;
        Ok(Spawn {
            source,
            pose,
            visibility: Self::component::<Visibility>(source)?.unwrap_or_default(),
            sprite: Self::component::<SpriteSource>(source)?,
            collider,
            actor,
            rules: Self::component::<RunRules>(source)?,
        })
    }
}

pub(crate) fn spawn_scene(
    world: &mut World,
    scene: &SceneDocument,
    rules_override: Option<RunRules>,
) -> Result<(Arena, RunRules), String> {
    scene.validate().map_err(|error| error.to_string())?;
    let spawns = scene
        .entities
        .iter()
        .map(Spawn::read)
        .collect::<Result<Vec<_>, _>>()?;
    let mut rules = spawns.iter().filter_map(|spawn| spawn.rules);
    let rules = match (rules.next(), rules.next()) {
        (Some(rules), None) => rules_override.unwrap_or(rules).checked()?,
        _ => return Err("Night Relay requires exactly one Run Rules component".into()),
    };
    let marker = |actor| -> Result<Marker, String> {
        let spawn = spawns
            .iter()
            .find(|spawn| spawn.actor == Some(actor))
            .ok_or_else(|| format!("Night Relay is missing its {actor:?} entity"))?;
        let Some(Collider2d::Circle { radius }) = spawn.collider else {
            return Err(format!(
                "{} requires a circular collider",
                spawn.source.name
            ));
        };
        Ok(Marker {
            pose: spawn.pose,
            radius,
        })
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

    let mut entities = HashMap::with_capacity(spawns.len());
    let mut children = Vec::new();
    for spawn in spawns {
        let mut entity = world.spawn((
            Name::new(spawn.source.name.as_str()),
            spawn.pose,
            spawn.visibility,
        ));
        entities.insert(spawn.source.id, entity.id());
        if let Some(parent) = spawn.source.parent {
            children.push((entity.id(), parent));
        }
        if let Some(role) = spawn.actor {
            entity.insert(SceneActor {
                role,
                rotation: spawn.pose.rotation,
                visibility: spawn.visibility,
            });
        } else if let Some(collider) = spawn.collider {
            entity.insert(collider);
        }
        if let Some(sprite) = spawn.sprite {
            entity.insert(sprite);
        }
        if spawn.rules.is_some() {
            entity.insert(rules);
        }
    }
    for (child, parent) in children {
        world.entity_mut(child).insert(ChildOf(entities[&parent]));
    }
    Ok((arena, rules))
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
