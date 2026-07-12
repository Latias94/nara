//! Transform components and spatial primitives.

use nara_app::{App, Plugin, PluginError};
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

impl Plugin for TransformPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.transform"),
            nara_app::PluginCategory::Core,
        )
        .requires_plugins(nara_reflect::COMPONENT_REGISTRY_PLUGIN_REQUIREMENT)
    }

    fn preflight(&self, app: &App) -> Result<(), PluginError> {
        let Some(registry) = app.world().get_resource::<ComponentRegistry>() else {
            return Ok(());
        };
        let component_id = ComponentTypeId::new("nara.transform.Transform2d");
        registry
            .validate_component_registration::<Transform2d>(&component_id)
            .map_err(|error| {
                PluginError::component_registration(self.plugin_id(), component_id.as_str(), error)
            })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let component_id = ComponentTypeId::new("nara.transform.Transform2d");
        register_transform_components(&mut app.world_mut()?.resource_mut::<ComponentRegistry>())
            .map_err(|error| {
                PluginError::component_registration(self.plugin_id(), component_id.as_str(), error)
            })?;
        Ok(())
    }
}

pub fn register_transform_components(
    registry: &mut ComponentRegistry,
) -> Result<(), ComponentRegistryError> {
    let component_id = ComponentTypeId::new("nara.transform.Transform2d");
    let schema = ComponentSchema::new(component_id, "Transform 2D", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(transform_fields());
    registry.register_persistent_component_with_codec::<Transform2d, _, _>(
        schema,
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
    use nara_app::{PluginId, PluginLifecycleState};
    use nara_reflect::ComponentRegistryPlugin;

    #[test]
    fn plugin_preflight_reports_component_conflicts_without_poisoning_app() {
        let mut registry = ComponentRegistry::new();
        register_transform_components(&mut registry)
            .expect("component registration should succeed");
        let mut app = App::new();
        app.insert_resource(registry)
            .expect("app should accept the component registry");
        app.add_plugin(ComponentRegistryPlugin)
            .expect("registry owner should install");

        let error = app
            .add_plugin(TransformPlugin)
            .err()
            .expect("duplicate component registration should fail preflight");

        assert!(matches!(
            error,
            PluginError::ComponentRegistrationFailed {
                plugin,
                component,
                ..
            } if plugin == PluginId::new("nara.transform")
                && component == "nara.transform.Transform2d"
        ));
        assert_eq!(
            app.plugin_lifecycle_state(),
            PluginLifecycleState::Configuring
        );
        assert!(app.plugin_failure_report().is_none());
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
