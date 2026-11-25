use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, PluginPreflightContext,
};
use nara_core::Vec2;
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistryBuilder, ComponentRegistryError, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueRef,
};

use super::Transform2d;

pub const TRANSFORM_SCHEMA_PROVIDER_ID: nara_app::PluginSchemaProviderId =
    nara_app::PluginSchemaProviderId::new("nara.transform.components");
pub const TRANSFORM_SCHEMA_OWNER_ID: nara_reflect::ComponentSchemaOwnerId =
    nara_reflect::ComponentSchemaOwnerId::new("nara.transform.components");
pub const TRANSFORM_SCHEMA_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::with_validation(
        TRANSFORM_SCHEMA_OWNER_ID,
        TRANSFORM_SCHEMA_PROVIDER_ID,
        nara_reflect::ComponentSchemaProviderBindingId::new("nara.transform.components.native", 1),
        transform_schema_catalog,
        validate_transform_components,
        register_transform_components,
    )
    .with_predecessor(transform_schema_predecessor_catalog);

pub fn register_transform_components(
    registry: &mut ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError> {
    validate_transform_components(registry)?;
    registry.register_persistent_component_with_required_components::<Transform2d, _, _>(
        transform_schema(),
        |value| {
            Ok(Transform2d {
                translation: read_vec2(value.field("translation")?, "translation")?,
                rotation: read_f32(value.field("rotation")?, "rotation")?,
                scale: read_vec2(value.field("scale")?, "scale")?,
                z: 0.0,
            })
        },
        |transform| {
            Ok(ComponentValue::map([
                ("translation", vec2_value(transform.translation)?),
                (
                    "rotation",
                    ComponentValue::f64(f64::from(transform.rotation))?,
                ),
                ("scale", vec2_value(transform.scale)?),
            ]))
        },
    )?;
    Ok(())
}

fn transform_schema_catalog()
-> Result<nara_reflect::ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError> {
    let predecessor = transform_schema_predecessor_catalog()?;
    let mut current = nara_reflect::ComponentSchemaCatalog::successor_of(&predecessor)
        .expect("the fixed generation-one Transform catalog has a successor");
    current.components.push(transform_schema());
    Ok(current)
}

fn transform_schema_predecessor_catalog()
-> Result<nara_reflect::ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError> {
    Ok(nara_reflect::ComponentSchemaCatalog {
        components: vec![transform_schema_predecessor()],
        ..nara_reflect::ComponentSchemaCatalog::default()
    })
}

fn transform_schema() -> ComponentSchema {
    transform_schema_predecessor().with_authoring_initial_value(ComponentValue::map([
        (
            "translation",
            vec2_value(Vec2::ZERO).expect("zero translation is finite"),
        ),
        (
            "rotation",
            ComponentValue::f64(0.0).expect("zero rotation is finite"),
        ),
        (
            "scale",
            vec2_value(Vec2::ONE).expect("unit scale is finite"),
        ),
    ]))
}

fn transform_schema_predecessor() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.transform.Transform2d"),
        "Transform 2D",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("translation.x"),
            "Translation X",
            ComponentFieldPath::from_fields(["translation", "x"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("translation.y"),
            "Translation Y",
            ComponentFieldPath::from_fields(["translation", "y"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("rotation"),
            "Rotation",
            ComponentFieldPath::from_fields(["rotation"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("scale.x"),
            "Scale X",
            ComponentFieldPath::from_fields(["scale", "x"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("scale.y"),
            "Scale Y",
            ComponentFieldPath::from_fields(["scale", "y"]),
            ComponentValueKind::F64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn validate_transform_components(
    registry: &ComponentRegistryBuilder,
) -> Result<(), ComponentRegistryError> {
    registry.validate_component_registration::<Transform2d>(&ComponentTypeId::new(
        "nara.transform.Transform2d",
    ))
}

fn read_vec2(value: ComponentValueRef<'_>, field: &str) -> Result<Vec2, ComponentCodecError> {
    Ok(Vec2::new(
        read_f32(value.field("x")?, &format!("{field}.x"))?,
        read_f32(value.field("y")?, &format!("{field}.y"))?,
    ))
}

fn read_f32(value: ComponentValueRef<'_>, field: &str) -> Result<f32, ComponentCodecError> {
    let value = value
        .as_f64()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "finite f32"))?;
    if !(f64::from(f32::MIN)..=f64::from(f32::MAX)).contains(&value) {
        return Err(ComponentCodecError::invalid_field(field, "finite f32"));
    }
    Ok(value as f32)
}

fn vec2_value(value: Vec2) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("x", ComponentValue::f64(f64::from(value.x))?),
        ("y", ComponentValue::f64(f64::from(value.y))?),
    ]))
}

pub const TRANSFORM_COMPONENTS_PLUGIN_ID: PluginId = PluginId::new("nara.transform");
pub const TRANSFORM_COMPONENTS_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(TRANSFORM_COMPONENTS_PLUGIN_ID, PluginCategory::Core)
        .requires_plugins(nara_reflect::COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
        .provides_schema(&[TRANSFORM_SCHEMA_PROVIDER_ID]);

/// Registers the v0 Transform component codec and its schema lineage.
#[derive(Debug, Default, Clone, Copy)]
pub struct TransformComponentsPlugin;

impl Plugin for TransformComponentsPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &TRANSFORM_COMPONENTS_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        nara_reflect::validate_schema_provider_for_plugin(
            context,
            TRANSFORM_COMPONENTS_PLUGIN_ID,
            "nara.transform.Transform2d",
            &TRANSFORM_SCHEMA_PROVIDER,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        nara_reflect::register_schema_provider_for_plugin(
            app,
            TRANSFORM_COMPONENTS_PLUGIN_ID,
            "nara.transform.Transform2d",
            &TRANSFORM_SCHEMA_PROVIDER,
        )
    }
}

#[cfg(test)]
mod tests {
    use nara_app::PluginLifecycleState;
    use nara_reflect::ComponentRegistryPlugin;

    use super::*;
    use crate::spatial::{GlobalTransform2d, InheritedVisibility, Visibility, register_spatial};

    #[test]
    fn v0_transform_codec_keeps_wire_lineage_and_adds_only_derived_components() {
        let provider = TRANSFORM_SCHEMA_PROVIDER.resolve().unwrap();
        let predecessor = provider.predecessor().unwrap().catalog();
        let current = provider.current().catalog();
        assert_eq!(predecessor.generation(), 1);
        assert_eq!(current.generation(), 2);
        assert_eq!(current.predecessor(), Some(&predecessor.fingerprint()));
        assert!(
            predecessor.components[0]
                .authoring_initial_value()
                .is_none()
        );
        let [schema] = current.components() else {
            panic!("one Transform schema is expected");
        };
        assert_eq!(schema.version(), ComponentSchemaVersion::ONE);
        let mut builder = ComponentRegistryBuilder::new();
        register_transform_components(&mut builder).unwrap();
        let registry = builder.freeze().unwrap();
        assert_eq!(registry.schema(schema.id()), Some(schema));
        let mut world = nara_ecs::World::new();
        register_spatial(&mut world);
        let entity = world.spawn_empty().id();
        registry
            .preflight_component(schema.id(), schema.authoring_initial_value().unwrap())
            .unwrap()
            .apply(&mut world, entity)
            .unwrap();
        assert_eq!(
            world.get::<Transform2d>(entity),
            Some(&Transform2d::IDENTITY)
        );
        assert_eq!(
            world.get::<GlobalTransform2d>(entity),
            Some(&GlobalTransform2d::IDENTITY)
        );
        assert_eq!(
            world.get::<InheritedVisibility>(entity),
            Some(&InheritedVisibility::VISIBLE)
        );
        assert!(world.get::<Visibility>(entity).is_none());
    }

    const CONFLICTING_TRANSFORM_PLUGIN_ID: PluginId = PluginId::new("nara.test.transform-conflict");
    const CONFLICTING_TRANSFORM_PROVIDER_ID: nara_app::PluginSchemaProviderId =
        nara_app::PluginSchemaProviderId::new("nara.test.transform-conflict.components");
    const CONFLICTING_TRANSFORM_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
        nara_reflect::ComponentSchemaProviderDefinition::with_validation(
            nara_reflect::ComponentSchemaOwnerId::new("nara.test.transform-conflict.components"),
            CONFLICTING_TRANSFORM_PROVIDER_ID,
            nara_reflect::ComponentSchemaProviderBindingId::new(
                "nara.test.transform-conflict.native",
                1,
            ),
            transform_schema_catalog,
            validate_transform_components,
            register_transform_components,
        )
        .with_predecessor(transform_schema_predecessor_catalog);
    const CONFLICTING_TRANSFORM_DECLARATION: PluginDeclaration =
        PluginDeclaration::new(CONFLICTING_TRANSFORM_PLUGIN_ID, PluginCategory::Runtime)
            .requires_plugins(nara_reflect::COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
            .provides_schema(&[CONFLICTING_TRANSFORM_PROVIDER_ID]);

    struct ConflictingTransformPlugin;

    impl Plugin for ConflictingTransformPlugin {
        fn declaration() -> &'static PluginDeclaration {
            &CONFLICTING_TRANSFORM_DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            nara_reflect::register_schema_provider_for_plugin(
                app,
                CONFLICTING_TRANSFORM_PLUGIN_ID,
                CONFLICTING_TRANSFORM_PROVIDER_ID.as_str(),
                &CONFLICTING_TRANSFORM_PROVIDER,
            )
        }
    }

    #[test]
    fn plugin_preflight_reports_component_conflicts_without_poisoning_app() {
        let mut app = App::new();
        app.add_plugins((ComponentRegistryPlugin, ConflictingTransformPlugin))
            .expect("the conflicting provider should install through the registry owner");

        let error = app
            .add_plugin(TransformComponentsPlugin)
            .err()
            .expect("duplicate component registration should fail preflight");

        assert!(matches!(
            error,
            nara_app::AddPluginsError::Plugin(ref failure) if matches!(failure.error(), PluginError::ComponentRegistrationFailed {
                plugin,
                component,
                ..
            } if *plugin == PluginId::new("nara.transform")
                && component == "nara.transform.Transform2d"
            )
        ));
        assert_eq!(
            app.plugin_lifecycle_state(),
            PluginLifecycleState::Configuring
        );
        assert!(app.plugin_failure_report().is_none());
        assert!(!app.has_plugin(TRANSFORM_COMPONENTS_PLUGIN_ID));
        app.seal()
            .expect("the corrected App should remain sealable");
    }

    #[test]
    fn transform_schema_exposes_authoring_fields() {
        let mut registry = ComponentRegistryBuilder::new();
        register_transform_components(&mut registry)
            .expect("component registration should succeed");
        let registry = registry.freeze().expect("component registry should freeze");

        let schema = registry
            .schema(&ComponentTypeId::new("nara.transform.Transform2d"))
            .unwrap();
        let mut fields = schema
            .fields
            .iter()
            .map(|field| (field.path.to_string(), field.value_kind(), field.required))
            .collect::<Vec<_>>();
        fields.sort_by(|left, right| left.0.cmp(&right.0));

        assert_eq!(
            fields,
            vec![
                ("rotation".to_string(), ComponentValueKind::F64, true),
                ("scale.x".to_string(), ComponentValueKind::F64, true),
                ("scale.y".to_string(), ComponentValueKind::F64, true),
                ("translation.x".to_string(), ComponentValueKind::F64, true),
                ("translation.y".to_string(), ComponentValueKind::F64, true),
            ]
        );
    }
}
