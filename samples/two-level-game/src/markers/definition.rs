use std::collections::BTreeMap;

use nara::{
    ecs::World,
    hierarchy::{Children, HierarchyRoots, Parent},
    identity::{
        EntityLookup, EntityReference, SceneEntityId, SpawnedSceneInstance, WorldIdentityDomain,
    },
    prelude::{Transform2d, Vec2},
    reflect::ComponentTypeId,
    scene::CompiledSceneDocument,
    sprite::Sprite,
};

use super::MarkerLink;
use crate::gameplay::content::decode;

/// Topology and local reference selected from the compiled marker Scene.
#[derive(Clone, Debug)]
pub(crate) struct MarkerDefinition {
    pub(crate) root: SceneEntityId,
    pub(crate) parts: Vec<MarkerPart>,
    pub(crate) link_target: SceneEntityId,
}

#[derive(Clone, Debug)]
pub(crate) struct MarkerPart {
    id: SceneEntityId,
    children: Vec<SceneEntityId>,
}

/// The geometry installed by the original Scene codecs for one placed marker.
#[derive(Clone, Debug)]
pub(crate) struct MarkerGeometry {
    position: Vec2,
    parts: Vec<(Transform2d, Sprite)>,
}

impl MarkerDefinition {
    pub(crate) fn read(scene: &CompiledSceneDocument) -> Result<Self, String> {
        let [root] = scene.roots() else {
            return Err("marker template requires exactly one spatial root".into());
        };
        if !(2..=16).contains(&scene.entities().len()) {
            return Err("marker template requires a bounded tree of 2 to 16 entities".into());
        }

        let transform_id = ComponentTypeId::new("nara.transform.Transform2d");
        let sprite_id = ComponentTypeId::new("nara.sprite.Sprite");
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
            if !entity.components().contains_key(&transform_id)
                || !entity.components().contains_key(&sprite_id)
            {
                return Err("every marker part requires Transform2d and Sprite".into());
            }
            parts.push(MarkerPart {
                id: entity.id().clone(),
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

    pub(crate) fn capture_geometry(
        &self,
        world: &World,
        receipt: &SpawnedSceneInstance,
        position: Vec2,
    ) -> Result<MarkerGeometry, String> {
        if !position.is_finite() {
            return Err("marker placement must be finite".into());
        }
        let mut parts = Vec::with_capacity(self.parts.len());
        self.visit_world(world, receipt, |_, transform, sprite| {
            parts.push((*transform, sprite.clone()));
            Ok(())
        })?;
        Ok(MarkerGeometry { position, parts })
    }

    pub(crate) fn validate_world(
        &self,
        world: &World,
        receipt: &SpawnedSceneInstance,
        position: Vec2,
        geometry: &MarkerGeometry,
    ) -> Result<(), String> {
        if position != geometry.position || self.parts.len() != geometry.parts.len() {
            return Err("marker occurrence differs from its saved placement".into());
        }
        self.visit_world(world, receipt, |index, transform, sprite| {
            let (expected_transform, expected_sprite) = &geometry.parts[index];
            if transform != expected_transform || sprite != expected_sprite {
                return Err("marker occurrence has changed authored geometry".into());
            }
            Ok(())
        })
    }

    fn visit_world(
        &self,
        world: &World,
        receipt: &SpawnedSceneInstance,
        mut visit_geometry: impl FnMut(usize, &Transform2d, &Sprite) -> Result<(), String>,
    ) -> Result<(), String> {
        if receipt.len() != self.parts.len()
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

        for (index, part) in self.parts.iter().enumerate() {
            let entity = entities[&part.id];
            let is_root = part.id == self.root;
            let transform = world
                .get::<Transform2d>(entity)
                .filter(|transform| transform.translation.is_finite())
                .ok_or("marker member lacks a finite Transform2d")?;
            let sprite = world
                .get::<Sprite>(entity)
                .ok_or("marker member lacks its Sprite")?;
            visit_geometry(index, transform, sprite)?;

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
