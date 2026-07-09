use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_ecs::{Bundle, Component, Entity, World};
use nara_reflect::{
    ComponentCodecError, ComponentFieldPath, ComponentFieldSchema, ComponentRegistry,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind, Reflect,
    bevy_reflect,
};

pub use nara_transform::Transform2d;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Component, Reflect)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Component, Reflect)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
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
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<ComponentRegistry>();
        register_scene_components(&mut app.world_mut().resource_mut::<ComponentRegistry>());
        app.add_systems(CoreStage::PostUpdate, sync_children);
        Ok(())
    }
}

pub fn register_scene_components(registry: &mut ComponentRegistry) {
    let name_id = ComponentTypeId::new("nara.scene.Name");
    registry
        .register_serializable_component_with_fields::<Name, _, _>(
            name_id.clone(),
            ComponentSchemaVersion(1),
            [ComponentFieldSchema::required(
                ComponentFieldPath::empty(),
                ComponentValueKind::String,
            )],
            |value| {
                Ok(Name::new(value.as_str().ok_or_else(|| {
                    ComponentCodecError::invalid_field("Name", "string")
                })?))
            },
            |name| Ok(ComponentValue::String(name.as_str().to_string())),
        )
        .expect("nara.scene.Name component registration should be unique");

    let visibility_id = ComponentTypeId::new("nara.scene.Visibility");
    registry
        .register_serializable_component_with_fields::<Visibility, _, _>(
            visibility_id.clone(),
            ComponentSchemaVersion(1),
            [ComponentFieldSchema::required(
                ComponentFieldPath::empty(),
                ComponentValueKind::String,
            )],
            |value| match value.as_str() {
                Some("visible") => Ok(Visibility::Visible),
                Some("hidden") => Ok(Visibility::Hidden),
                _ => Err(ComponentCodecError::invalid_field(
                    "Visibility",
                    "'visible' or 'hidden'",
                )),
            },
            |visibility| {
                Ok(ComponentValue::String(match visibility {
                    Visibility::Visible => "visible".to_string(),
                    Visibility::Hidden => "hidden".to_string(),
                }))
            },
        )
        .expect("nara.scene.Visibility component registration should be unique");
}
