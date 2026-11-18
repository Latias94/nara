use std::collections::BTreeMap;

use nara::{
    ecs::World,
    hierarchy::{Children, HierarchyRoots, Parent},
    identity::{
        EntityLookup, EntityReference, SceneEntityId, SpawnedSceneInstance, WorldIdentityDomain,
    },
    prelude::{Transform2d, Vec2},
    reflect::{ComponentRegistrySnapshot, ComponentTypeId, ComponentValueKind},
    scene::CompiledSceneDocument,
    sprite::Sprite,
};

use super::MarkerLink;
use crate::gameplay::content::decode;

/// Owned expectations derived from the original compiled marker Scene.
#[derive(Clone, Debug)]
pub(crate) struct MarkerDefinition {
    pub(crate) root: SceneEntityId,
    pub(crate) parts: Vec<MarkerPart>,
    pub(crate) link_target: SceneEntityId,
}

#[derive(Clone, Debug)]
pub(crate) struct MarkerPart {
    pub(crate) id: SceneEntityId,
    pub(crate) transform: Transform2d,
    pub(crate) sprite: Sprite,
    pub(crate) children: Vec<SceneEntityId>,
}

impl MarkerDefinition {
    pub(crate) fn read(
        scene: &CompiledSceneDocument,
        registry: &ComponentRegistrySnapshot,
    ) -> Result<Self, String> {
        let [root] = scene.roots() else {
            return Err("marker template requires exactly one spatial root".into());
        };
        if !(2..=16).contains(&scene.entities().len()) {
            return Err("marker template requires a bounded tree of 2 to 16 entities".into());
        }

        let transform_id = ComponentTypeId::new("nara.transform.Transform2d");
        let sprite_id = ComponentTypeId::new("nara.sprite.Sprite");
        let mut scratch = World::new();
        let mut parts = Vec::with_capacity(scene.entities().len());
        let mut link_target = None;

        for entity in scene.entities() {
            if entity.components().keys().any(|id| {
                !matches!(
                    id.as_str(),
                    "nara.transform.Transform2d"
                        | "nara.sprite.Sprite"
                        | "two-level-game.MarkerLink"
                )
            }) {
                return Err("marker template contains an unexpected component".into());
            }
            let transform = entity
                .components()
                .get(&transform_id)
                .ok_or("every marker part requires Transform2d")?;
            let sprite = entity
                .components()
                .get(&sprite_id)
                .ok_or("every marker part requires Sprite")?;
            if sprite
                .value()
                .field("material")
                .map_err(|error| error.to_string())?
                .get("image")
                .is_some_and(|image| image.kind() != ComponentValueKind::Null)
            {
                return Err("marker template Sprites must be untextured".into());
            }

            let target = scratch.spawn_empty().id();
            for (id, record) in [(&transform_id, transform), (&sprite_id, sprite)] {
                let schema = registry.schema(id).ok_or("marker codec is unavailable")?;
                if record.version() != schema.version() {
                    return Err(
                        "marker template requires current built-in component versions".into(),
                    );
                }
                registry
                    .preflight_component(id, record.value())
                    .and_then(|prepared| prepared.apply(&mut scratch, target))
                    .map_err(|error| error.to_string())?;
            }
            parts.push(MarkerPart {
                id: entity.id().clone(),
                transform: *scratch
                    .get::<Transform2d>(target)
                    .ok_or("marker Transform2d codec produced no component")?,
                sprite: scratch
                    .get::<Sprite>(target)
                    .ok_or("marker Sprite codec produced no component")?
                    .clone(),
                children: entity.children().to_vec(),
            });

            match (entity.id() == root, decode::<MarkerLink>(entity)?) {
                (
                    true,
                    Some(MarkerLink {
                        target: EntityReference::SceneLocal { entity },
                    }),
                ) => {
                    link_target = Some(entity);
                }
                (true, _) => {
                    return Err("marker root requires one resolved local MarkerLink".into());
                }
                (false, Some(_)) => {
                    return Err("only the marker root may contain MarkerLink".into());
                }
                (false, None) => {}
            }
        }

        let link_target = link_target.ok_or("marker template lacks its root MarkerLink")?;
        if &link_target == root || !parts.iter().any(|part| part.id == link_target) {
            return Err("marker root must link to a distinct child in its own tree".into());
        }
        Ok(Self {
            root: root.clone(),
            parts,
            link_target,
        })
    }

    pub(crate) fn validate_world(
        &self,
        world: &World,
        receipt: &SpawnedSceneInstance,
        position: Vec2,
    ) -> Result<(), String> {
        if !position.is_finite()
            || receipt.len() != self.parts.len()
            || self.parts.iter().any(|part| !receipt.contains(&part.id))
        {
            return Err("marker occurrence disagrees with its complete authored definition".into());
        }
        let entities = self
            .parts
            .iter()
            .map(|part| match receipt.resolve(world, &part.id) {
                EntityLookup::Resolved(entity) => Ok((&part.id, entity)),
                _ => Err(
                    "an authored marker member no longer resolves in its original receipt"
                        .to_owned(),
                ),
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let roots = world
            .get_resource::<HierarchyRoots>()
            .ok_or("marker hierarchy roots are unavailable")?;

        for part in &self.parts {
            let entity = entities[&part.id];
            let is_root = part.id == self.root;
            let mut transform = part.transform;
            if is_root {
                transform.translation += position;
            }
            if !transform.translation.is_finite()
                || world.get::<Transform2d>(entity) != Some(&transform)
                || world.get::<Sprite>(entity) != Some(&part.sprite)
            {
                return Err("marker occurrence has missing or changed authored geometry".into());
            }

            let parent = self
                .parts
                .iter()
                .find(|candidate| candidate.children.contains(&part.id))
                .map(|parent| entities[&parent.id]);
            let children = part
                .children
                .iter()
                .map(|id| entities[id])
                .collect::<Vec<_>>();
            if world.get::<Parent>(entity).map(Parent::parent) != parent
                || roots.contains(entity) != is_root
                || world
                    .get::<Children>(entity)
                    .map_or(&[][..], Children::as_slice)
                    != children.as_slice()
            {
                return Err(
                    "marker occurrence disagrees with its complete authored hierarchy".into(),
                );
            }
            if !is_root && world.get::<MarkerLink>(entity).is_some() {
                return Err("only the marker root may contain a live MarkerLink".into());
            }
        }

        let root = entities[&self.root];
        let link = world
            .get::<MarkerLink>(root)
            .ok_or("marker root lost its MarkerLink")?;
        let expected = EntityReference::SceneLocal {
            entity: self.link_target.clone(),
        };
        let domain = world
            .get_resource::<WorldIdentityDomain>()
            .ok_or("marker identity domain is unavailable")?;
        if link.target != expected
            || domain.resolve_entity_reference(world, &link.target, Some(receipt.instance_id()))
                != EntityLookup::Resolved(entities[&self.link_target])
        {
            return Err("marker link does not resolve within its original occurrence".into());
        }
        Ok(())
    }
}
