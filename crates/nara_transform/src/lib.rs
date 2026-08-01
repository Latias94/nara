//! Transform components and spatial primitives.

use nara_app::{App, Plugin, PluginError, PluginPreflightContext};
use nara_core::{Mat3, Vec2};
use nara_ecs::Component;
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Transform2d {
    pub translation: Vec2,
    pub rotation: f32,
    pub scale: Vec2,
}

impl Transform2d {
    pub const IDENTITY: Self = Self {
        translation: Vec2::ZERO,
        rotation: 0.0,
        scale: Vec2::ONE,
    };

    pub fn from_translation(translation: Vec2) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    pub fn matrix(&self) -> Mat3 {
        Mat3::from_scale_angle_translation(self.scale, self.rotation, self.translation)
    }
}

impl Default for Transform2d {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Component)]
pub struct GlobalTransform2d(pub Mat3);

impl Default for GlobalTransform2d {
    fn default() -> Self {
        Self(Mat3::IDENTITY)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TransformPlugin;

pub const TRANSFORM_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.transform");
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
    );
const TRANSFORM_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID,
    nara_hierarchy::HIERARCHY_PLUGIN_ID,
];
pub const TRANSFORM_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(TRANSFORM_PLUGIN_ID, nara_app::PluginCategory::Core)
        .requires_plugins(TRANSFORM_PLUGIN_REQUIREMENTS)
        .provides_schema(&[TRANSFORM_SCHEMA_PROVIDER_ID]);

impl Plugin for TransformPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &TRANSFORM_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        let component_id = ComponentTypeId::new("nara.transform.Transform2d");
        let registry = nara_reflect::registry_for_plugin_preflight(
            context,
            TRANSFORM_PLUGIN_ID,
            component_id.as_str(),
        )?;
        TRANSFORM_SCHEMA_PROVIDER
            .preflight(registry)
            .map_err(|error| {
                PluginError::component_registration(
                    TRANSFORM_PLUGIN_ID,
                    component_id.as_str(),
                    error,
                )
            })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let component_id = ComponentTypeId::new("nara.transform.Transform2d");
        nara_reflect::register_schema_provider_for_plugin(
            app,
            TRANSFORM_PLUGIN_ID,
            component_id.as_str(),
            &TRANSFORM_SCHEMA_PROVIDER,
        )
    }
}

pub fn register_transform_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    validate_transform_components(registry)?;
    registry.register_persistent_component_with_codec::<Transform2d, _, _>(
        transform_schema(),
        |value| {
            Ok(Transform2d {
                translation: read_vec2(value.field("translation")?, "translation")?,
                rotation: read_f32(value.field("rotation")?, "rotation")?,
                scale: read_vec2(value.field("scale")?, "scale")?,
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
    Ok(nara_reflect::ComponentSchemaCatalog {
        components: vec![transform_schema()],
        ..nara_reflect::ComponentSchemaCatalog::default()
    })
}

fn transform_schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.transform.Transform2d"),
        "Transform 2D",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields(transform_fields())
}

fn validate_transform_components(
    registry: &ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    registry.validate_component_registration::<Transform2d>(&ComponentTypeId::new(
        "nara.transform.Transform2d",
    ))
}

fn transform_fields() -> [ComponentFieldSchema; 5] {
    [
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
    ]
}

fn read_vec2(value: &ComponentValue, field: &str) -> Result<Vec2, ComponentCodecError> {
    Ok(Vec2::new(
        read_f32(value.field("x")?, &format!("{field}.x"))?,
        read_f32(value.field("y")?, &format!("{field}.y"))?,
    ))
}

fn read_f32(value: &ComponentValue, field: &str) -> Result<f32, ComponentCodecError> {
    let value = value
        .as_f64()
        .ok_or_else(|| ComponentCodecError::invalid_field(field, "finite f32"))?;
    if value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
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

pub mod prelude {
    pub use crate::{GlobalTransform2d, Transform2d, TransformPlugin};
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_app::{PluginCategory, PluginDeclaration, PluginId, PluginLifecycleState};
    use nara_reflect::ComponentRegistryPlugin;

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
        );
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
        app.add_plugins((
            ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            ConflictingTransformPlugin,
        ))
        .expect("the conflicting provider should install through the registry owner");

        let error = app
            .add_plugin(TransformPlugin)
            .err()
            .expect("duplicate component registration should fail preflight");

        assert!(matches!(
            error,
            nara_app::AddPluginsError::Plugin(PluginError::ComponentRegistrationFailed {
                plugin,
                component,
                ..
            }) if plugin == PluginId::new("nara.transform")
                && component == "nara.transform.Transform2d"
        ));
        assert_eq!(
            app.plugin_lifecycle_state(),
            PluginLifecycleState::Configuring
        );
        assert!(app.plugin_failure_report().is_none());
        assert!(!app.has_plugin(TRANSFORM_PLUGIN_ID));
        app.seal()
            .expect("the corrected App should remain sealable");
    }

    #[test]
    fn transform_schema_exposes_authoring_fields() {
        let mut registry = ComponentRegistry::new();
        register_transform_components(&mut registry)
            .expect("component registration should succeed");
        registry.freeze().expect("component registry should freeze");

        let schema = registry
            .schema(&ComponentTypeId::new("nara.transform.Transform2d"))
            .unwrap();
        let mut fields = schema
            .fields
            .iter()
            .map(|field| (field.path.to_string(), field.value_kind, field.required))
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
