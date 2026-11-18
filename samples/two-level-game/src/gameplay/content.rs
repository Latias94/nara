use std::collections::BTreeMap;

use nara::{
    asset::AssetRef,
    ecs::World,
    identity::SceneEntityId,
    prelude::{Component, PersistentComponent, PersistentComponentProvider, Transform2d, Vec2},
    project_host::ProjectDomainAssets,
    reflect::ComponentTypeId,
    scene::{CompiledSceneDocument, CompiledSceneEntity},
};

use super::collision::{CollisionBox, RoomCollision, Wall};
use crate::animation::{ExplorerAnimation, PreparedAnimation};
use crate::markers::{MarkerDefinition, MarkerPalette, marker_product};
use crate::{levels::LevelId, save::Checkpoint};

pub const PLAYER: u32 = 1;
pub const CHECKPOINT: u32 = 2;
pub const PICKUP: u32 = 3;
pub const PORTAL: u32 = 4;
pub const WALL: u32 = 5;

/// Authored role only; score, collected state and dynamic identities are explicit game data.
#[derive(Debug, Clone, Component, PersistentComponent)]
#[nara(
    id = "two-level-game.ObjectRole",
    version = 1,
    alias = "Object role",
    authoring_initial,
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct ObjectRole {
    #[nara(id = "kind", alias = "Role kind", initial = 1)]
    pub kind: u32,
}

/// One immutable one-shot bank shared by this level's gameplay cues.
#[derive(Debug, Clone, Component, PersistentComponent)]
#[nara(
    id = "two-level-game.SoundPalette",
    version = 1,
    alias = "Sound palette",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct SoundPalette {
    #[nara(
        id = "bank",
        alias = "Bank",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    pub bank: AssetRef,
}

#[derive(Debug, Clone)]
pub(super) struct Content {
    pub player: SceneEntityId,
    pub checkpoint: SceneEntityId,
    pub checkpoint_position: Vec2,
    pub pickups: BTreeMap<SceneEntityId, Vec2>,
    pub bank: AssetRef,
    pub collision: RoomCollision,
    pub animation: PreparedAnimation,
    pub marker: MarkerDefinition,
}

impl Content {
    pub fn read_current(world: &World) -> Result<Self, String> {
        let current = nara::advanced_prelude::CurrentScene::get(world)
            .ok_or("current Scene is unavailable")?;
        let assets = world
            .get_resource::<ProjectDomainAssets>()
            .ok_or("current domain assets are unavailable")?;
        nara::project_host::inspect_current_scene_dependency(
            world,
            current.receipt(),
            marker_product(),
            |template| Self::read(current.source(), assets, template),
        )
        .map_err(|report| format!("marker dependency is unavailable: {:?}", report.diagnostics))?
    }

    pub fn read(
        scene: &CompiledSceneDocument,
        assets: &ProjectDomainAssets,
        marker: &CompiledSceneDocument,
    ) -> Result<Self, String> {
        if scene.entities().len() > 32 {
            return Err("level has too many authored entities".into());
        }
        let mut player = None;
        let mut checkpoint = None;
        let mut pickups = BTreeMap::new();
        let mut portals = 0;
        let mut cameras = 0;
        let mut bank = None;
        let mut walls = Vec::new();
        for entity in scene.entities() {
            let palette = decode::<SoundPalette>(entity)?;
            let marker = decode::<MarkerPalette>(entity)?;
            if entity
                .components()
                .keys()
                .any(|id| id.as_str() == "nara.render.Camera2d")
            {
                cameras += 1;
                if !scene.roots().contains(entity.id()) || transform(entity)?.is_none() {
                    return Err("camera must be a transformed level root".into());
                }
                bank = Some(palette.ok_or("camera lacks its SoundPalette")?.bank);
                let marker = marker.ok_or("camera lacks its MarkerPalette")?.template;
                let expected = marker_product();
                if marker.as_product() != Some(expected)
                    && marker.as_stable_id() != Some(expected.source())
                {
                    return Err("camera must select the admitted marker Scene Product".into());
                }
            } else if palette.is_some() || marker.is_some() {
                return Err(
                    "SoundPalette and MarkerPalette must belong to the level camera".into(),
                );
            }
            let Some(role) = decode::<ObjectRole>(entity)? else {
                continue;
            };
            if !scene.roots().contains(entity.id()) || !entity.children().is_empty() {
                return Err("game objects must be unparented level roots".into());
            }
            let transform = transform(entity)?.ok_or("game object lacks Transform2d")?;
            let position = transform.translation;
            if role.kind != WALL && !valid_position(position) {
                return Err("game object position is outside the level bounds".into());
            }
            if role.kind == WALL && (!position.is_finite() || position.abs().max_element() > 1024.0)
            {
                return Err("wall position is outside the game's finite geometry".into());
            }
            if !entity
                .components()
                .keys()
                .any(|id| id.as_str() == "nara.sprite.Sprite")
            {
                return Err("game object lacks its authored Sprite".into());
            }
            match role.kind {
                PLAYER if player.is_none() => {
                    let definition =
                        decode::<CollisionBox>(entity)?.ok_or("explorer lacks its CollisionBox")?;
                    let animation = decode::<ExplorerAnimation>(entity)?
                        .ok_or("explorer lacks its authored animation resource")?;
                    player = Some((entity.id().clone(), transform, definition, animation));
                }
                CHECKPOINT if checkpoint.is_none() => {
                    checkpoint = Some((entity.id().clone(), position))
                }
                PICKUP => {
                    pickups.insert(entity.id().clone(), position);
                }
                PORTAL => portals += 1,
                WALL => walls.push(Wall {
                    id: entity.id().clone(),
                    transform,
                    definition: decode::<CollisionBox>(entity)?
                        .ok_or("wall lacks its CollisionBox")?,
                }),
                _ => return Err("duplicate or unsupported game object role".into()),
            }
        }
        let (checkpoint, checkpoint_position) =
            checkpoint.ok_or("level lacks a unique checkpoint")?;
        if portals != 1 || cameras != 1 || pickups.len() != 2 {
            return Err("level requires one camera, one portal and two authored pickups".into());
        }
        let (player, mut player_transform, definition, animation) =
            player.ok_or("level lacks a unique explorer")?;
        player_transform.translation = checkpoint_position;
        walls.sort_by(|first, second| first.id.cmp(&second.id));
        let collision = RoomCollision::prepare(definition, walls, player_transform)?;
        let animation = PreparedAnimation::acquire(assets, animation.clips)?;
        let marker = MarkerDefinition::read(marker)?;
        Ok(Self {
            player,
            checkpoint,
            checkpoint_position,
            pickups,
            bank: bank.ok_or("level lacks its camera SoundPalette")?,
            collision,
            animation,
            marker,
        })
    }

    pub fn validate_progress(&self, level: LevelId, checkpoint: &Checkpoint) -> Result<(), String> {
        checkpoint.validate().map_err(|error| error.to_string())?;
        if checkpoint.score != checkpoint.collected.len() as u32 * 10 {
            return Err("saved score must equal ten points per collected pickup".into());
        }
        if checkpoint.location.level.level() == level
            && checkpoint.location.anchor != self.checkpoint
        {
            return Err("checkpoint anchor does not exist in prepared content".into());
        }
        for pickup in checkpoint
            .collected
            .iter()
            .filter(|pickup| pickup.level.level() == level)
        {
            if !self.pickups.contains_key(&pickup.entity) {
                return Err("saved pickup does not exist in prepared content".into());
            }
        }
        if checkpoint
            .markers
            .iter()
            .filter(|marker| marker.level.level() == level)
            .any(|marker| !valid_position(Vec2::from_array(marker.position)))
        {
            return Err("saved dynamic marker is outside the playable level".into());
        }
        Ok(())
    }
}

pub(super) fn valid_position(position: Vec2) -> bool {
    position.is_finite() && position.x.abs() <= 280.0 && position.y.abs() <= 150.0
}

pub(crate) fn decode<T: PersistentComponentProvider>(
    entity: &CompiledSceneEntity,
) -> Result<Option<T>, String> {
    let schema = T::persistent_component_schema();
    let Some(record) = entity.components().get(schema.id()) else {
        return Ok(None);
    };
    if record.version() != schema.version() {
        return Err("gameplay component version is not current".into());
    }
    T::__decode_persistent_component(record.value())
        .map(Some)
        .map_err(|error| error.to_string())
}

fn transform(entity: &CompiledSceneEntity) -> Result<Option<Transform2d>, String> {
    let Some(record) = entity
        .components()
        .get(&ComponentTypeId::new("nara.transform.Transform2d"))
    else {
        return Ok(None);
    };
    if record.version().get() != 1 {
        return Err(
            "game collision preparation requires the current built-in Transform2d version".into(),
        );
    }
    let value = record.value();
    let vector = |name| {
        let value = value.field(name).map_err(|error| error.to_string())?;
        Ok::<_, String>(Vec2::new(
            value.field_f64("x").map_err(|error| error.to_string())? as f32,
            value.field_f64("y").map_err(|error| error.to_string())? as f32,
        ))
    };
    let transform = Transform2d {
        translation: vector("translation")?,
        rotation: value
            .field_f64("rotation")
            .map_err(|error| error.to_string())? as f32,
        scale: vector("scale")?,
    };
    if !transform.translation.is_finite()
        || !transform.rotation.is_finite()
        || !transform.scale.is_finite()
    {
        return Err(
            "authored Transform2d cannot be represented by finite collision coordinates".into(),
        );
    }
    Ok(Some(transform))
}
