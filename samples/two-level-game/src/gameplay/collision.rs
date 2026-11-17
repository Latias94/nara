use std::{fmt, sync::Arc, time::Duration};

use nara::{
    advanced_prelude::CurrentScene,
    ecs::World,
    hierarchy::Parent,
    identity::{EntityLookup, SceneEntityId},
    prelude::{Component, PersistentComponent, Resource, Transform2d, Vec2},
};
use nara_rapier2d::{
    StaticCollisionScene, TopDownCharacter, affine_box_collider,
    rapier::{math::Pose, prelude::SharedShape},
};

use super::{ActiveLevel, content::valid_position};

const MARGIN: f32 = 0.01;

/// Game-owned box configuration; native shapes and query indices remain derived data.
#[derive(Debug, Clone, Copy, PartialEq, Component, PersistentComponent)]
#[nara(
    id = "two-level-game.CollisionBox",
    version = 1,
    alias = "Collision box",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct CollisionBox {
    #[nara(id = "center", alias = "Center")]
    pub center: Vec2,
    #[nara(
        id = "half_width",
        alias = "Half width",
        range(min_inclusive = 0.001_f32, max_inclusive = 1024.0_f32)
    )]
    pub half_width: f32,
    #[nara(
        id = "half_height",
        alias = "Half height",
        range(min_inclusive = 0.001_f32, max_inclusive = 1024.0_f32)
    )]
    pub half_height: f32,
}

impl CollisionBox {
    fn validate(self) -> Result<(), String> {
        if !self.center.is_finite()
            || self.center.abs().max_element() > 1024.0
            || !self.half_width.is_finite()
            || !self.half_height.is_finite()
            || !(0.001..=1024.0).contains(&self.half_width)
            || !(0.001..=1024.0).contains(&self.half_height)
        {
            return Err("collision box is outside the game's admitted finite geometry".into());
        }
        Ok(())
    }

    fn collider(
        self,
        transform: Transform2d,
    ) -> Result<nara_rapier2d::rapier::prelude::Collider, String> {
        self.validate()?;
        affine_box_collider(
            self.center,
            Vec2::new(self.half_width, self.half_height),
            transform.matrix(),
        )
        .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone)]
pub(super) struct Wall {
    pub id: SceneEntityId,
    pub transform: Transform2d,
    pub definition: CollisionBox,
}

struct RoomData {
    scene: StaticCollisionScene,
    player: CollisionBox,
    walls: Vec<Wall>,
}

#[derive(Clone)]
pub(super) struct RoomCollision(Arc<RoomData>);

impl fmt::Debug for RoomCollision {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output
            .debug_struct("RoomCollision")
            .field("wall_count", &self.0.scene.collider_count())
            .field("player", &self.0.player)
            .finish()
    }
}

impl RoomCollision {
    pub fn prepare(
        player: CollisionBox,
        walls: Vec<Wall>,
        checkpoint: Transform2d,
    ) -> Result<Self, String> {
        player.validate()?;
        if walls.is_empty() {
            return Err("the level must declare its static collision walls".into());
        }
        let colliders = walls
            .iter()
            .map(|wall| wall.definition.collider(wall.transform))
            .collect::<Result<Vec<_>, _>>()?;
        let scene = StaticCollisionScene::new(colliders).map_err(|error| error.to_string())?;
        let room = Self(Arc::new(RoomData {
            scene,
            player,
            walls,
        }));
        room.validate_placement(checkpoint)?;
        Ok(room)
    }

    pub fn validate_placement(&self, transform: Transform2d) -> Result<(), String> {
        self.prepare_character(self.0.player, transform).map(|_| ())
    }

    fn prepare_character(
        &self,
        definition: CollisionBox,
        transform: Transform2d,
    ) -> Result<Character, String> {
        if !valid_position(transform.translation) {
            return Err("the player position is outside the game world".into());
        }
        let local = Transform2d {
            translation: Vec2::ZERO,
            ..transform
        };
        let collider = definition.collider(local)?;
        let local_pose = *collider.position();
        let shape: SharedShape = collider.shared_shape().clone();
        let controller = TopDownCharacter::new(shape, MARGIN).map_err(|error| error.to_string())?;
        let pose = Pose::translation(transform.translation.x, transform.translation.y) * local_pose;
        if controller
            .overlaps(&self.0.scene, pose)
            .map_err(|error| error.to_string())?
        {
            return Err("the player placement overlaps a static wall".into());
        }
        Ok(Character {
            room: self.clone(),
            definition,
            rotation: transform.rotation,
            scale: transform.scale,
            position: transform.translation,
            local_pose,
            controller,
        })
    }

    pub fn validate_representation(&self, world: &World) -> Result<(), String> {
        let current = CurrentScene::get(world).ok_or("current Scene is unavailable")?;
        for wall in &self.0.walls {
            let EntityLookup::Resolved(entity) = current.receipt().resolve(world, &wall.id) else {
                return Err("a collision wall no longer belongs to the current Scene".into());
            };
            if world.get::<Transform2d>(entity) != Some(&wall.transform)
                || world.get::<CollisionBox>(entity) != Some(&wall.definition)
                || world.get::<Parent>(entity).is_some()
            {
                return Err(
                    "live static wall changes are not represented by the admitted Scene".into(),
                );
            }
        }
        Ok(())
    }
}

struct Character {
    room: RoomCollision,
    definition: CollisionBox,
    rotation: f32,
    scale: Vec2,
    position: Vec2,
    local_pose: Pose,
    controller: TopDownCharacter,
}

#[derive(Default, Resource)]
pub(super) struct CharacterState {
    character: Option<Character>,
}

pub(super) fn place(
    world: &mut World,
    entity: nara::ecs::Entity,
    transform: Transform2d,
) -> Result<(), String> {
    let room = world
        .resource::<ActiveLevel>()
        .content()
        .ok_or("active collision content is unavailable")?
        .collision
        .clone();
    let definition = *world
        .get::<CollisionBox>(entity)
        .ok_or("the explorer has no CollisionBox")?;
    if definition != room.0.player || world.get::<Parent>(entity).is_some() {
        return Err("the explorer collision configuration differs from its admitted root".into());
    }
    let character = room.prepare_character(definition, transform)?;
    *world
        .get_mut::<Transform2d>(entity)
        .ok_or("the explorer has no Transform2d")? = transform;
    world.resource_mut::<CharacterState>().character = Some(character);
    Ok(())
}

pub(super) fn move_player(
    world: &mut World,
    entity: nara::ecs::Entity,
    desired: Vec2,
    delta: Duration,
) -> Result<Vec2, String> {
    let transform = *world
        .get::<Transform2d>(entity)
        .ok_or("the explorer has no Transform2d")?;
    let definition = *world
        .get::<CollisionBox>(entity)
        .ok_or("the explorer has no CollisionBox")?;
    if world.get::<Parent>(entity).is_some() {
        return Err("character movement requires the game's original root pose".into());
    }
    let room = world
        .resource::<ActiveLevel>()
        .content()
        .ok_or("active collision content is unavailable")?
        .collision
        .clone();
    world.resource_scope(|world, mut state: nara::ecs::Mut<CharacterState>| {
        let current = state.character.as_ref().is_some_and(|character| {
            Arc::ptr_eq(&character.room.0, &room.0)
                && character.definition == definition
                && character.rotation == transform.rotation
                && character.scale == transform.scale
                && character.position == transform.translation
        });
        if !current {
            let character = room.prepare_character(definition, transform)?;
            state.character = Some(character);
        }
        let character = state.character.as_mut().unwrap();
        let pose = Pose::translation(transform.translation.x, transform.translation.y)
            * character.local_pose;
        let bounds = Vec2::new(280.0, 150.0);
        let desired =
            (transform.translation + desired).clamp(-bounds, bounds) - transform.translation;
        let movement = character
            .controller
            .move_shape(&room.0.scene, pose, desired, delta)
            .map_err(|error| error.to_string())?;
        let mut position = transform.translation + movement.translation;
        if !position.is_finite() {
            return Err("collision movement returned a nonfinite player position".into());
        }
        if !valid_position(position) {
            position = transform.translation;
        }
        world
            .get_mut::<Transform2d>(entity)
            .ok_or("the explorer has no Transform2d")?
            .translation = position;
        character.position = position;
        Ok(position)
    })
}

pub(super) fn capture_eligible(world: &World, entity: nara::ecs::Entity) -> Result<(), String> {
    let room = &world
        .resource::<ActiveLevel>()
        .content()
        .ok_or("active collision content is unavailable")?
        .collision;
    if world.get::<CollisionBox>(entity) != Some(&room.0.player)
        || world.get::<Parent>(entity).is_some()
    {
        return Err("runtime character configuration is not represented by ResumeState".into());
    }
    room.validate_representation(world)?;
    room.validate_placement(
        *world
            .get::<Transform2d>(entity)
            .ok_or("the explorer has no Transform2d")?,
    )
}
