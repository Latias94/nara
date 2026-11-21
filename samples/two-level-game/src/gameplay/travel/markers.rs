use std::collections::BTreeSet;

use nara::{
    advanced_prelude::CurrentScene,
    ecs::{Component, Entity, World},
    identity::{EntityLookup, SceneEntityId, SpawnedSceneInstance},
    prelude::{Transform2d, Vec2},
    project_host::instantiate_current_scene_dependency,
    scene::{SceneOccurrenceKey, retire_and_despawn_scene_instance},
};

use crate::{
    gameplay::{ActiveLevel, Progress, Session, fail, player},
    levels::{LevelId, LevelKey},
    markers::{MarkerDefinition, MarkerGeometry, marker_product},
    save::{MAX_MARKERS, MarkerId},
};

#[derive(Component)]
pub(in crate::gameplay) struct DynamicMarker {
    pub id: MarkerId,
}

#[derive(Clone, Debug)]
pub(in crate::gameplay) struct MarkerInstance {
    pub id: MarkerId,
    pub level: LevelKey,
    pub occurrence: SceneOccurrenceKey,
    pub receipt: SpawnedSceneInstance,
    pub root: SceneEntityId,
    geometry: Option<MarkerGeometry>,
}

impl MarkerInstance {
    pub fn root_entity(&self, world: &World) -> Result<Entity, String> {
        match self.receipt.resolve(world, &self.root) {
            EntityLookup::Resolved(entity) => Ok(entity),
            _ => Err("authored marker root no longer resolves in its original activation".into()),
        }
    }

    pub(super) fn validate_geometry(
        &self,
        world: &World,
        definition: &MarkerDefinition,
        position: Vec2,
    ) -> Result<(), String> {
        let geometry = self
            .geometry
            .as_ref()
            .ok_or("marker placement did not complete")?;
        definition.validate_world(world, &self.receipt, position, geometry)
    }
}

fn occurrence(level: LevelKey, id: MarkerId) -> SceneOccurrenceKey {
    let level_name = match level.level() {
        LevelId::Meadow => "meadow",
        LevelId::Cavern => "cavern",
    };
    SceneOccurrenceKey::new(
        "two-level-game.markers",
        format!("{level_name}/{}/{}", level.instance(), id.get()),
    )
    .expect("finite game marker occurrence is valid")
}

pub(in crate::gameplay) fn observe_markers(world: &World) -> Result<Vec<MarkerId>, String> {
    let mut query = world.try_query::<(Entity, &DynamicMarker)>();
    let mut actual = query
        .iter_mut()
        .flat_map(|query| query.iter(world))
        .take(MAX_MARKERS + 1)
        .map(|(entity, marker)| (entity, marker.id))
        .collect::<Vec<_>>();
    actual.sort_by_key(|(_, id)| *id);
    let level = world.resource::<ActiveLevel>();
    let instances = &world.resource::<Session>().markers;
    let records = world
        .resource::<Progress>()
        .checkpoint()
        .markers
        .iter()
        .filter(|record| record.level == level.key())
        .collect::<Vec<_>>();
    if actual.is_empty() && instances.is_empty() && records.is_empty() {
        return Ok(Vec::new());
    }
    let definition = &level
        .content()
        .ok_or("active marker content is unavailable")?
        .marker;
    let actual_entities = actual
        .iter()
        .map(|(entity, _)| *entity)
        .collect::<BTreeSet<_>>();
    let owned_entities = instances
        .iter()
        .map(|instance| instance.root_entity(world))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let ids = actual.iter().map(|(_, id)| *id).collect::<BTreeSet<_>>();
    if actual.len() > MAX_MARKERS
        || instances.len() != actual.len()
        || records.len() != actual.len()
        || actual_entities != owned_entities
        || ids.len() != actual.len()
    {
        return Err(
            "all-World marker roots disagree with the complete game-owned activation set".into(),
        );
    }
    for instance in instances {
        let record = records
            .iter()
            .find(|record| record.id == instance.id)
            .ok_or("authored marker activation has no saved current-level identity")?;
        if instance.level != level.key()
            || instance.occurrence != occurrence(level.key(), instance.id)
            || instance.root != definition.root
            || world
                .get::<DynamicMarker>(instance.root_entity(world)?)
                .is_none_or(|marker| marker.id != instance.id)
        {
            return Err(
                "authored marker activation differs from its saved logical placement".into(),
            );
        }
        instance.validate_geometry(world, definition, Vec2::from_array(record.position))?;
    }
    Ok(actual.into_iter().map(|(_, id)| id).collect())
}

pub(super) fn add_marker(world: &mut World) -> Result<(), String> {
    let entity = player(world).ok_or("explorer identity is unavailable")?;
    let position = world
        .get::<Transform2d>(entity)
        .ok_or("explorer has no transform")?
        .translation;
    let key = world.resource::<ActiveLevel>().key();
    let id = world.resource_mut::<Progress>().add_marker(key, position)?;
    if world.resource::<Session>().local_player {
        println!("two_level_game: marker={} level={key:?}", id.get());
    }
    if let Err(error) = rebuild(world) {
        fail(world, error);
    }
    Ok(())
}

pub(super) fn rebuild(world: &mut World) -> Result<(), String> {
    let level = world.resource::<ActiveLevel>().key();
    let definition = world
        .resource::<ActiveLevel>()
        .content()
        .ok_or("active marker content is unavailable")?
        .marker
        .clone();
    let records = world
        .resource::<Progress>()
        .checkpoint()
        .markers
        .iter()
        .filter(|record| record.level == level)
        .cloned()
        .collect::<Vec<_>>();
    let mut ids = BTreeSet::new();
    if world.resource::<Session>().markers.iter().any(|instance| {
        instance.level != level
            || !ids.insert(instance.id)
            || !records.iter().any(|record| record.id == instance.id)
    }) {
        return Err("retained marker activation has duplicate or foreign saved identity".into());
    }
    for record in records {
        let position = Vec2::from_array(record.position);
        let existing = world
            .resource::<Session>()
            .markers
            .iter()
            .position(|instance| instance.id == record.id);
        if let Some(index) = existing {
            let instance = world.resource::<Session>().markers[index].clone();
            if instance
                .validate_geometry(world, &definition, position)
                .is_ok()
                && instance.root_entity(world).is_ok_and(|root| {
                    world
                        .get::<DynamicMarker>(root)
                        .is_some_and(|marker| marker.id == record.id)
                })
            {
                continue;
            }
            retire_and_despawn_scene_instance(world, &instance.receipt).map_err(|diagnostics| {
                format!("authored marker retirement failed: {diagnostics:?}")
            })?;
            world.resource_mut::<Session>().markers.remove(index);
        }
        let expected = CurrentScene::get(world)
            .ok_or("current Scene is unavailable")?
            .receipt()
            .clone();
        let occurrence = occurrence(level, record.id);
        let report =
            instantiate_current_scene_dependency(world, &expected, marker_product(), &occurrence);
        if report.diagnostics.has_errors() {
            return Err(format!(
                "authored marker instantiation failed: {:?}",
                report.diagnostics
            ));
        }
        let instance = MarkerInstance {
            id: record.id,
            level,
            occurrence,
            receipt: report
                .instance
                .ok_or("authored marker instantiation produced no activation")?,
            root: definition.root.clone(),
            geometry: None,
        };
        world
            .resource_mut::<Session>()
            .markers
            .push(instance.clone());
        let root = instance.root_entity(world)?;
        let mut transform = world
            .get_mut::<Transform2d>(root)
            .ok_or("authored marker root lacks Transform2d")?;
        transform.translation += position;
        world
            .entity_mut(root)
            .insert(DynamicMarker { id: record.id });
        let geometry = definition.capture_geometry(world, &instance.receipt, position)?;
        world
            .resource_mut::<Session>()
            .markers
            .last_mut()
            .expect("the placed marker receipt is retained")
            .geometry = Some(geometry);
    }
    observe_markers(world)?;
    Ok(())
}
