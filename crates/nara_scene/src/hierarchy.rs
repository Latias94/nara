use nara_app::{App, CoreStage, Plugin, PluginError, PluginPreflightContext};
use nara_ecs::{Bundle, Component, Entity, World};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
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

pub const HIERARCHY_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.scene.hierarchy");
pub const HIERARCHY_SCHEMA_PROVIDER_ID: nara_app::PluginSchemaProviderId =
    nara_app::PluginSchemaProviderId::new("nara.scene.hierarchy.components");
pub const HIERARCHY_SCHEMA_OWNER_ID: nara_reflect::ComponentSchemaOwnerId =
    nara_reflect::ComponentSchemaOwnerId::new("nara.scene.hierarchy.components");
pub const HIERARCHY_SCHEMA_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::with_validation(
        HIERARCHY_SCHEMA_OWNER_ID,
        HIERARCHY_SCHEMA_PROVIDER_ID,
        nara_reflect::ComponentSchemaProviderBindingId::new(
            "nara.scene.hierarchy.components.native",
            1,
        ),
        hierarchy_schema_catalog,
        validate_scene_components,
        register_scene_components,
    );
pub const HIERARCHY_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(HIERARCHY_PLUGIN_ID, nara_app::PluginCategory::Core)
        .requires_plugins(nara_reflect::COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
        .provides_schema(&[HIERARCHY_SCHEMA_PROVIDER_ID]);

impl Plugin for HierarchyPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &HIERARCHY_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        let registry = nara_reflect::registry_for_plugin_preflight(
            context,
            HIERARCHY_PLUGIN_ID,
            HIERARCHY_SCHEMA_PROVIDER_ID.as_str(),
        )?;
        HIERARCHY_SCHEMA_PROVIDER
            .preflight(registry)
            .map_err(|error| {
                PluginError::component_registration(
                    HIERARCHY_PLUGIN_ID,
                    HIERARCHY_SCHEMA_PROVIDER_ID.as_str(),
                    error,
                )
            })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        HIERARCHY_SCHEMA_PROVIDER
            .register_or_validate_into(&mut app.world_mut()?.resource_mut::<ComponentRegistry>())
            .map_err(|error| {
                PluginError::component_registration(
                    HIERARCHY_PLUGIN_ID,
                    HIERARCHY_SCHEMA_PROVIDER_ID.as_str(),
                    error,
                )
            })?;
        app.add_systems(CoreStage::PostUpdate, sync_children)?;
        Ok(())
    }
}

pub fn register_scene_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    validate_scene_components(registry)?;
    register_name_component(registry)?;
    register_visibility_component(registry)
}

fn validate_scene_components(registry: &ComponentRegistry) -> Result<(), ComponentRegistryError> {
    registry.validate_component_registration::<Name>(&ComponentTypeId::new("nara.scene.Name"))?;
    registry.validate_component_registration::<Visibility>(&ComponentTypeId::new(
        "nara.scene.Visibility",
    ))?;
    Ok(())
}

fn register_name_component(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    registry.register_persistent_component_with_codec::<Name, _, _>(
        name_schema(),
        |value| {
            Ok(Name::new(value.as_str().ok_or_else(|| {
                ComponentCodecError::invalid_field("Name", "string")
            })?))
        },
        |name| Ok(ComponentValue::String(name.as_str().to_string())),
    )?;
    Ok(())
}

fn register_visibility_component(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    registry.register_persistent_component_with_codec::<Visibility, _, _>(
        visibility_schema(),
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
    )?;
    Ok(())
}

fn hierarchy_schema_catalog()
-> Result<nara_reflect::ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError> {
    Ok(nara_reflect::ComponentSchemaCatalog {
        components: vec![name_schema(), visibility_schema()],
        ..nara_reflect::ComponentSchemaCatalog::default()
    })
}

fn name_schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.scene.Name"),
        "Name",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Name",
        ComponentFieldPath::empty(),
        ComponentValueKind::String,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}

fn visibility_schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.scene.Visibility"),
        "Visibility",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Visibility",
        ComponentFieldPath::empty(),
        ComponentValueKind::String,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
}
