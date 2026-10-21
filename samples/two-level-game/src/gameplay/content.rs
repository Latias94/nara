use std::collections::BTreeMap;

use nara::{
    identity::SceneEntityId,
    prelude::{Component, PersistentComponent, PersistentComponentProvider, Vec2},
    reflect::ComponentTypeId,
    scene::{SceneDocument, SceneEntityRecord},
};

use crate::{levels::LevelId, save::Checkpoint};

pub const PLAYER: u32 = 1;
pub const CHECKPOINT: u32 = 2;
pub const PICKUP: u32 = 3;
pub const PORTAL: u32 = 4;

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

#[derive(Debug, Clone)]
pub(super) struct Content {
    pub player: SceneEntityId,
    pub checkpoint: SceneEntityId,
    pub checkpoint_position: Vec2,
    pub pickups: BTreeMap<SceneEntityId, Vec2>,
}

impl Content {
    pub fn read(scene: &SceneDocument) -> Result<Self, String> {
        if scene.entities.len() > 32 {
            return Err("level has too many authored entities".into());
        }
        let mut player = None;
        let mut checkpoint = None;
        let mut pickups = BTreeMap::new();
        let mut portals = 0;
        let mut cameras = 0;
        for entity in &scene.entities {
            if entity
                .components
                .keys()
                .any(|id| id.as_str() == "nara.render.Camera2d")
            {
                cameras += 1;
                if !scene.roots.contains(&entity.id) || authored_position(entity)?.is_none() {
                    return Err("camera must be a transformed level root".into());
                }
            }
            let Some(role) = decode::<ObjectRole>(entity)? else {
                continue;
            };
            if !scene.roots.contains(&entity.id) || !entity.children.is_empty() {
                return Err("game objects must be unparented level roots".into());
            }
            let position = authored_position(entity)?.ok_or("game object lacks Transform2d")?;
            if !valid_position(position) {
                return Err("game object position is outside the level bounds".into());
            }
            if !entity
                .components
                .keys()
                .any(|id| id.as_str() == "nara.sprite.Sprite")
            {
                return Err("game object lacks its authored Sprite".into());
            }
            match role.kind {
                PLAYER if player.is_none() => player = Some(entity.id.clone()),
                CHECKPOINT if checkpoint.is_none() => {
                    checkpoint = Some((entity.id.clone(), position))
                }
                PICKUP => {
                    pickups.insert(entity.id.clone(), position);
                }
                PORTAL => portals += 1,
                _ => return Err("duplicate or unsupported game object role".into()),
            }
        }
        let (checkpoint, checkpoint_position) =
            checkpoint.ok_or("level lacks a unique checkpoint")?;
        if portals != 1 || cameras != 1 || pickups.len() != 2 {
            return Err("level requires one camera, one portal and two authored pickups".into());
        }
        Ok(Self {
            player: player.ok_or("level lacks a unique explorer")?,
            checkpoint,
            checkpoint_position,
            pickups,
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

fn decode<T: PersistentComponentProvider>(entity: &SceneEntityRecord) -> Result<Option<T>, String> {
    let schema = T::persistent_component_schema();
    let Some(record) = entity.components.get(schema.id()) else {
        return Ok(None);
    };
    if record.version != schema.version() {
        return Err("gameplay component version is not current".into());
    }
    T::__decode_persistent_component(&record.value)
        .map(Some)
        .map_err(|error| error.to_string())
}

// ProjectScenes and CurrentScene already admitted these records through the frozen native codec.
// Inspect only the field needed by this game's bounded placement rules; this is not another
// Transform2d decoder or schema authority, and unsupported wire versions are rejected explicitly.
fn authored_position(entity: &SceneEntityRecord) -> Result<Option<Vec2>, String> {
    let Some(record) = entity
        .components
        .get(&ComponentTypeId::new("nara.transform.Transform2d"))
    else {
        return Ok(None);
    };
    if record.version.get() != 1 {
        return Err("game placement requires the current built-in Transform2d version".into());
    }
    let translation = record
        .value
        .field("translation")
        .map_err(|error| error.to_string())?;
    let x = translation
        .field_f64("x")
        .map_err(|error| error.to_string())?;
    let y = translation
        .field_f64("y")
        .map_err(|error| error.to_string())?;
    let position = Vec2::new(x as f32, y as f32);
    if !position.is_finite() {
        return Err(
            "authored translation cannot be represented by the game's finite coordinates".into(),
        );
    }
    Ok(Some(position))
}
