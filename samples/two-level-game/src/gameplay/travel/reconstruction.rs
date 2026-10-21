use std::collections::BTreeSet;

use nara::{
    advanced_prelude::CurrentScene,
    ecs::{Component, World, system::SystemState},
    hierarchy::{
        HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyRoots,
        HierarchyWriter, Parent,
    },
    identity::{EntityLookup, spawn_identity_entity},
    prelude::{Color, Transform2d, Vec2},
    sprite::Sprite,
};

use crate::{
    gameplay::{ActiveLevel, Progress, Session, fail, player},
    save::{MAX_MARKERS, MarkerId, MarkerRecord},
};

#[derive(Component)]
pub(in crate::gameplay) struct DynamicMarker {
    pub id: MarkerId,
}

pub(in crate::gameplay) fn observe_markers(world: &mut World) -> Result<Vec<MarkerId>, String> {
    let mut query = world.query::<(nara::ecs::Entity, &DynamicMarker)>();
    let mut actual = query
        .iter(world)
        .take(MAX_MARKERS + 1)
        .map(|(entity, marker)| (entity, marker.id))
        .collect::<Vec<_>>();
    actual.sort_by_key(|(_, id)| *id);
    let tokens = &world.resource::<Session>().markers;
    let actual_entities = actual
        .iter()
        .map(|(entity, _)| *entity)
        .collect::<BTreeSet<_>>();
    let cached_entities = tokens
        .iter()
        .map(|token| token.entity())
        .collect::<BTreeSet<_>>();
    let ids = actual.iter().map(|(_, id)| *id).collect::<BTreeSet<_>>();
    if actual.len() > MAX_MARKERS
        || actual_entities != cached_entities
        || cached_entities.len() != tokens.len()
        || ids.len() != actual.len()
    {
        return Err(
            "all-World dynamic markers disagree with the exact game-owned token set".into(),
        );
    }
    let domain = world
        .get_resource::<nara::identity::WorldIdentityDomain>()
        .ok_or("marker identity domain is unavailable")?;
    if tokens
        .iter()
        .any(|token| domain.locators_for_token(world, *token).is_err())
    {
        return Err("a dynamic marker token no longer names its actual live identity".into());
    }
    let key = world.resource::<ActiveLevel>().key;
    let records = world
        .resource::<Progress>()
        .0
        .markers
        .iter()
        .filter(|marker| marker.level == key)
        .collect::<Vec<_>>();
    if records.len() != actual.len()
        || actual.iter().any(|(entity, id)| {
            !records.iter().any(|record| {
                record.id == *id
                    && world.get::<Transform2d>(*entity).is_some_and(|transform| {
                        transform.translation.to_array() == record.position
                    })
            }) || world.get::<Sprite>(*entity).is_none()
        })
    {
        return Err("live dynamic marker identities or positions disagree with current-level checkpoint facts".into());
    }
    Ok(actual.into_iter().map(|(_, id)| id).collect())
}

pub(in crate::gameplay) fn reconstruct(world: &mut World) -> Result<(), String> {
    let content = world
        .resource::<ActiveLevel>()
        .content
        .clone()
        .ok_or("active level content is unavailable")?;
    let entities = {
        let mut access = SystemState::<CurrentScene>::new(world);
        let current = access.get(world).map_err(|error| error.to_string())?;
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
    world.get_mut::<Transform2d>(player).unwrap().translation = content.checkpoint_position;
    hide_collected(world);
    rebuild_markers(world)
}

pub(in crate::gameplay) fn hide_collected(world: &mut World) {
    let level = world.resource::<ActiveLevel>();
    let Some(content) = &level.content else {
        return;
    };
    let collected = world
        .resource::<Progress>()
        .0
        .collected
        .iter()
        .filter(|pickup| pickup.level == level.key)
        .map(|pickup| pickup.entity.clone())
        .collect::<BTreeSet<_>>();
    let ids = content.pickups.keys().cloned().collect::<Vec<_>>();
    let entities = {
        let mut access = SystemState::<CurrentScene>::new(world);
        let Ok(current) = access.get(world) else {
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

pub(super) fn add_marker(world: &mut World) -> Result<(), String> {
    let entity = player(world).ok_or("explorer identity is unavailable")?;
    let position = world
        .get::<Transform2d>(entity)
        .ok_or("explorer has no transform")?
        .translation;
    let key = world.resource::<ActiveLevel>().key;
    let mut progress = world.resource_mut::<Progress>();
    if progress.0.markers.len() >= MAX_MARKERS {
        return Err("dynamic marker limit reached".into());
    }
    let id = progress.0.next_marker_id;
    let next = id
        .get()
        .checked_add(1)
        .and_then(MarkerId::new)
        .filter(|id| id.get() != u64::MAX)
        .ok_or("marker identities are exhausted")?;
    progress.0.markers.push(MarkerRecord {
        id,
        level: key,
        position: position.to_array(),
    });
    progress.0.next_marker_id = next;
    println!("two_level_game: marker={} level={key:?}", id.get());
    if let Err(error) = rebuild_markers(world) {
        fail(world, error);
    }
    Ok(())
}

fn rebuild_markers(world: &mut World) -> Result<(), String> {
    let key = world.resource::<ActiveLevel>().key;
    let records = world
        .resource::<Progress>()
        .0
        .markers
        .iter()
        .filter(|marker| marker.level == key)
        .cloned()
        .collect::<Vec<_>>();
    for marker in records {
        let existing = world
            .resource::<Session>()
            .markers
            .iter()
            .find(|token| {
                world
                    .get::<DynamicMarker>(token.entity())
                    .is_some_and(|value| value.id == marker.id)
            })
            .copied();
        let token = match existing {
            Some(token) => token,
            None => {
                let token = spawn_identity_entity(world).map_err(|error| error.to_string())?;
                world.entity_mut(token.entity()).insert((
                    DynamicMarker { id: marker.id },
                    Transform2d {
                        translation: Vec2::from_array(marker.position),
                        ..Transform2d::IDENTITY
                    },
                    Sprite::from_color(Vec2::splat(10.0), Color::rgb(1.0, 0.3, 0.8)),
                ));
                world.resource_mut::<Session>().markers.push(token);
                token
            }
        };
        if world
            .resource::<HierarchyRoots>()
            .as_slice()
            .contains(&token.entity())
        {
            continue;
        }
        if world.get::<Parent>(token.entity()).is_some() {
            return Err("dynamic marker is unexpectedly parented".into());
        }
        HierarchyWriter::new(world)
            .apply(HierarchyOperation::Place {
                entity: token.entity(),
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            })
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
