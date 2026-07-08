//! Scene data components and serializable scene asset shells.

use nara_app::{App, CoreStage, Plugin};
use nara_ecs::{Bundle, Component, Entity, World};
pub use nara_transform::Transform2d;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Name(pub String);

impl Name {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
pub struct Parent(pub Entity);

#[derive(Debug, Clone, PartialEq, Eq, Default, Component)]
pub struct Children(pub Vec<Entity>);

impl Children {
    pub fn push(&mut self, child: Entity) {
        self.0.push(child);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.iter().copied()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Entity] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneEntity {
    pub stable_id: u64,
    pub component_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneAsset {
    pub entities: Vec<SceneEntity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Scene {
    pub roots: Vec<SceneNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SceneNode {
    pub name: Option<String>,
    pub children: Vec<SceneNode>,
}

pub fn spawn_child<B: Bundle>(world: &mut World, parent: Entity, bundle: B) -> Entity {
    let child = world.spawn(bundle).id();
    world.entity_mut(child).insert(Parent(parent));
    child
}

pub fn sync_children(world: &mut World) {
    {
        let mut query = world.query::<&mut Children>();
        for mut children in query.iter_mut(world) {
            children.clear();
        }
    }

    let links = {
        let mut query = world.query::<(Entity, &Parent)>();
        query
            .iter(world)
            .map(|(child, parent)| (child, parent.0))
            .collect::<Vec<_>>()
    };

    for (child, parent) in links {
        if world.get_entity(parent).is_err() {
            continue;
        }

        let mut parent_entity = world.entity_mut(parent);
        if let Some(mut children) = parent_entity.get_mut::<Children>() {
            children.push(child);
        } else {
            parent_entity.insert(Children(vec![child]));
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HierarchyPlugin;

impl Plugin for HierarchyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(CoreStage::PostUpdate, sync_children);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syncs_parent_child_links() {
        let mut world = World::new();
        let parent = world.spawn((Name::new("parent"),)).id();
        let child = spawn_child(&mut world, parent, (Name::new("child"),));

        sync_children(&mut world);

        let parent_ref = world.get_entity(parent).unwrap();
        let children = parent_ref.get::<Children>().unwrap();
        assert_eq!(children.as_slice(), &[child]);
    }
}
