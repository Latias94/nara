use nara_app::{App, Plugin, PluginError, PluginPreflightContext};
use nara_ecs::Component;
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind, Reflect,
    bevy_reflect,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Component, Reflect)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SceneComponentsPlugin;

pub const SCENE_COMPONENTS_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.scene.components");
pub const SCENE_COMPONENTS_SCHEMA_PROVIDER_ID: nara_app::PluginSchemaProviderId =
    nara_app::PluginSchemaProviderId::new("nara.scene.hierarchy.components");
pub const SCENE_COMPONENTS_SCHEMA_OWNER_ID: nara_reflect::ComponentSchemaOwnerId =
    nara_reflect::ComponentSchemaOwnerId::new("nara.scene.hierarchy.components");
pub const SCENE_COMPONENTS_SCHEMA_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::with_validation(
        SCENE_COMPONENTS_SCHEMA_OWNER_ID,
        SCENE_COMPONENTS_SCHEMA_PROVIDER_ID,
        nara_reflect::ComponentSchemaProviderBindingId::new(
            "nara.scene.hierarchy.components.native",
            1,
        ),
        scene_components_schema_catalog,
        validate_scene_components,
        register_scene_components,
    )
    .with_predecessor(scene_components_schema_predecessor_catalog);
pub const SCENE_COMPONENTS_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(SCENE_COMPONENTS_PLUGIN_ID, nara_app::PluginCategory::Core)
        .requires_plugins(nara_reflect::COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
        .provides_schema(&[SCENE_COMPONENTS_SCHEMA_PROVIDER_ID]);

impl Plugin for SceneComponentsPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &SCENE_COMPONENTS_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        let registry = nara_reflect::registry_for_plugin_preflight(
            context,
            SCENE_COMPONENTS_PLUGIN_ID,
            SCENE_COMPONENTS_SCHEMA_PROVIDER_ID.as_str(),
        )?;
        SCENE_COMPONENTS_SCHEMA_PROVIDER
            .preflight(registry)
            .map_err(|error| {
                PluginError::component_registration(
                    SCENE_COMPONENTS_PLUGIN_ID,
                    SCENE_COMPONENTS_SCHEMA_PROVIDER_ID.as_str(),
                    error,
                )
            })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        nara_reflect::register_schema_provider_for_plugin(
            app,
            SCENE_COMPONENTS_PLUGIN_ID,
            SCENE_COMPONENTS_SCHEMA_PROVIDER_ID.as_str(),
            &SCENE_COMPONENTS_SCHEMA_PROVIDER,
        )
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

fn scene_components_schema_catalog()
-> Result<nara_reflect::ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError> {
    let predecessor = scene_components_schema_predecessor_catalog()?;
    let mut current = nara_reflect::ComponentSchemaCatalog::successor_of(&predecessor)
        .expect("the fixed generation-one Scene components catalog has a successor");
    current.components = vec![name_schema(), visibility_schema()];
    Ok(current)
}

fn scene_components_schema_predecessor_catalog()
-> Result<nara_reflect::ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError> {
    Ok(nara_reflect::ComponentSchemaCatalog {
        components: vec![name_schema_predecessor(), visibility_schema_predecessor()],
        ..nara_reflect::ComponentSchemaCatalog::default()
    })
}

fn name_schema() -> ComponentSchema {
    name_schema_predecessor()
        .with_authoring_initial_value(ComponentValue::String("Entity".to_owned()))
}

// Generation-one wire contract retained as owner-local catalog lineage data.
fn name_schema_predecessor() -> ComponentSchema {
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
    visibility_schema_predecessor()
        .with_authoring_initial_value(ComponentValue::String("visible".to_owned()))
}

fn visibility_schema_predecessor() -> ComponentSchema {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation_templates_preserve_scalar_wire_contracts_and_owner_lineage() {
        let predecessor = scene_components_schema_predecessor_catalog().unwrap();
        let current = scene_components_schema_catalog().unwrap();
        assert_eq!(predecessor.generation(), 1);
        assert_eq!(current.generation(), 2);
        assert_eq!(current.predecessor(), Some(&predecessor.fingerprint()));
        assert!(
            predecessor
                .components
                .iter()
                .all(|schema| schema.authoring_initial_value().is_none())
        );
        let mut registry = ComponentRegistry::new();
        register_scene_components(&mut registry).unwrap();
        registry.freeze().unwrap();
        let snapshot = registry.snapshot().unwrap();
        let mut world = nara_ecs::World::new();
        let entity = world.spawn_empty().id();
        for schema in &current.components {
            assert_eq!(schema.version(), ComponentSchemaVersion::ONE);
            assert_eq!(registry.schema(schema.id()), Some(schema));
            snapshot
                .preflight_component(schema.id(), schema.authoring_initial_value().unwrap())
                .unwrap()
                .apply(&mut world, entity)
                .unwrap();
        }
        assert_eq!(world.get::<Name>(entity), Some(&Name::new("Entity")));
        assert_eq!(world.get::<Visibility>(entity), Some(&Visibility::Visible));
        nara_reflect::__private::resolve_schema_provider(SCENE_COMPONENTS_SCHEMA_PROVIDER).unwrap();
    }
}
