//! Transform components and spatial primitives.

use nara_app::{App, Plugin};
use nara_core::{Mat3, Vec2};
use nara_ecs::Component;
use nara_reflect::{
    ComponentCodecError, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
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
    fn build(&self, app: &mut App) {
        app.init_resource::<ComponentRegistry>();
        register_transform_components(&mut app.world_mut().resource_mut::<ComponentRegistry>());
    }
}

pub fn register_transform_components(registry: &mut ComponentRegistry) {
    registry
        .register_serializable_component::<Transform2d, _, _>(
            ComponentTypeId::new("nara.transform.Transform2d"),
            ComponentSchemaVersion(1),
            |value| {
                Ok(Transform2d {
                    translation: read_vec2(value.field("translation")?, "translation")?,
                    rotation: value.field_f64("rotation")? as f32,
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
        )
        .expect("nara.transform.Transform2d component registration should be unique");
}

fn read_vec2(value: &ComponentValue, field: &str) -> Result<Vec2, ComponentCodecError> {
    Ok(Vec2::new(
        value.field("x").and_then(|value| {
            value.as_f64().ok_or_else(|| {
                ComponentCodecError::invalid_field(format!("{field}.x"), "finite float")
            })
        })? as f32,
        value.field("y").and_then(|value| {
            value.as_f64().ok_or_else(|| {
                ComponentCodecError::invalid_field(format!("{field}.y"), "finite float")
            })
        })? as f32,
    ))
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
