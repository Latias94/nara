use nara::{
    ProductRecipe, ProductRecipeError, SchemaContribution,
    app::{PluginCategory, PluginDeclaration, PluginError, PluginId, PluginSchemaProviderId},
    ecs::Query,
    prelude::{App, Component, PersistentComponent, PersistentComponentProvider, Plugin},
    reflect::{
        ComponentCodecError, ComponentRegistry, ComponentRegistryError, ComponentSchemaCatalog,
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaProviderSourceError,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    },
};

const PULSE_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("studio.pulse.schema");
const PULSE_PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("studio.pulse"),
    PULSE_PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("studio.pulse.native", 1),
    pulse_catalog,
    register_pulse,
);
const PULSE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PluginId::new("studio.pulse"), PluginCategory::Runtime)
        .requires_plugins(&[
            nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID,
            nara::advanced_prelude::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
        ])
        .provides_schema(&[PULSE_PROVIDER_ID]);

#[derive(Component, PersistentComponent)]
#[nara(
    id = "studio.pulse.Pulse",
    version = 2,
    alias = "Pulse",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct RenamedPulse {
    #[nara(id = "value", alias = "Value")]
    pub value: i64,
}

#[derive(Default)]
pub struct PulsePlugin;

impl Plugin for PulsePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PULSE_DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        nara::advanced_prelude::add_startup_scene_activation_systems(app, observe_pulse)?;
        Ok(())
    }
}

fn pulse_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![RenamedPulse::persistent_component_schema()],
        ..Default::default()
    })
}

fn register_pulse(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    registry.register_persistent_component::<RenamedPulse>()?;
    registry.register_component_migration(
        &ComponentTypeId::new("studio.pulse.Pulse"),
        ComponentSchemaVersion::ONE,
        ComponentSchemaVersion::new(2).unwrap(),
        migrate_pulse,
    )?;
    Ok(())
}

fn migrate_pulse(mut value: ComponentValue) -> Result<ComponentValue, ComponentCodecError> {
    value
        .set_path(
            &nara::reflect::ComponentFieldPath::from_fields(["value"]),
            ComponentValue::I64(42),
        )
        .map_err(|error| ComponentCodecError::invalid_field("value", error.to_string()))?;
    Ok(value)
}

fn observe_pulse(pulses: Query<&RenamedPulse>) {
    assert_eq!(pulses.single().unwrap().value, 42);
    if let Some(path) = std::env::var_os("NARA_TEST_PULSE_MARKER") {
        std::fs::write(path, "migrated-pulse=42\n").expect("the fixture marker must be writable");
    }
}

pub fn custom_contribution() -> Result<SchemaContribution<PulsePlugin>, ProductRecipeError> {
    SchemaContribution::for_default([PULSE_PROVIDER])
}

pub fn custom_recipe() -> Result<ProductRecipe, ProductRecipeError> {
    ProductRecipe::new().add_contribution(custom_contribution()?)
}
