use nara::{
    advanced_prelude::CurrentScene, ecs::World, identity::EntityLookup, prelude::Transform2d,
    sprite::Sprite,
};

use crate::gameplay::{ActiveLevel, Progress, hide_pickup};

pub(in crate::gameplay) fn reconstruct(world: &mut World) -> Result<(), String> {
    let level = world.resource::<ActiveLevel>();
    let key = level.key();
    let content = level
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

    for (id, entity) in entities {
        if content.pickups.contains_key(&id)
            && world
                .resource::<Progress>()
                .checkpoint()
                .collected
                .iter()
                .any(|pickup| pickup.level == key && pickup.entity == id)
        {
            hide_pickup(world, entity);
        }
    }
    super::markers::rebuild(world)
}
