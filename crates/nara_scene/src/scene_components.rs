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
    );
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
