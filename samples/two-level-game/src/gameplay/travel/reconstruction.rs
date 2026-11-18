use std::collections::BTreeSet;

use nara::{
    advanced_prelude::CurrentScene, ecs::World, identity::EntityLookup, prelude::Transform2d,
    sprite::Sprite,
};

use crate::gameplay::{ActiveLevel, Progress};

pub(in crate::gameplay) fn reconstruct(world: &mut World) -> Result<(), String> {
    let content = world
        .resource::<ActiveLevel>()
        .content()
        .cloned()
        .ok_or("active level content is unavailable")?;
    let entities = {
        let current = CurrentScene::get(world).ok_or("current Scene is unavailable")?;
        let mut ids = vec![content.player.clone(), content.checkpoint.clone()];
        ids.extend(content.pickups.keys().cloned());
        ids.into_iter()
            .map(|id| match current.receipt().resolve(world, &id) {
                EntityLookup::Resolved(entity)
                    if world.get::<Transform2d>(entity).is_some()
                        && world.get::<Sprite>(entity).is_some() =>
                {
                    Ok((id, entity))
                }
                _ => Err("committed gameplay object is missing a required component".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let player = entities
        .iter()
        .find(|(id, _)| id == &content.player)
        .unwrap()
        .1;
    let mut pose = *world.get::<Transform2d>(player).unwrap();
    pose.translation = content.checkpoint_position;
    crate::gameplay::collision::place(world, player, pose)?;
    hide_collected(world);
    super::markers::rebuild(world)
}

pub(in crate::gameplay) fn hide_collected(world: &mut World) {
    let level = world.resource::<ActiveLevel>();
    let Some(content) = level.content() else {
        return;
    };
    let collected = world
        .resource::<Progress>()
        .checkpoint()
        .collected
        .iter()
        .filter(|pickup| pickup.level == level.key())
        .map(|pickup| pickup.entity.clone())
        .collect::<BTreeSet<_>>();
    let ids = content.pickups.keys().cloned().collect::<Vec<_>>();
    let entities = {
        let Some(current) = CurrentScene::get(world) else {
            return;
        };
        ids.into_iter()
            .filter_map(|id| match current.receipt().resolve(world, &id) {
                EntityLookup::Resolved(entity) => Some((id, entity)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for (id, entity) in entities {
        if let Some(mut sprite) = world.get_mut::<Sprite>(entity) {
            sprite.material.tint.a = if collected.contains(&id) { 0.0 } else { 1.0 };
        }
    }
}
