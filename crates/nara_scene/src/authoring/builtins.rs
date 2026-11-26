use bevy_ecs::component::Component;
use nara_core::{AssetId, Color, Vec2};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueRef, Schema,
};

use crate::spatial::{Transform2d, Visibility};

/// A source quad with linear tint and an optional stable image ID.
#[derive(Component, Schema, Debug, Clone, PartialEq)]
#[schema(
    crate = "nara_reflect",
    id = "nara.SpriteSource",
    version = 1,
    default,
    alias = "Sprite",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct SpriteSource {
    #[schema(alias = "Size")]
    pub size: Vec2,
    #[schema(alias = "Tint", color_srgb)]
    pub tint: Color,
    #[schema(alias = "Layer")]
    pub layer: i32,
    #[schema(alias = "PNG Image")]
    pub image: Option<AssetId>,
}

impl SpriteSource {
    #[must_use]
    pub const fn tint(&self) -> Color {
        self.tint
    }
}

impl Default for SpriteSource {
    fn default() -> Self {
        Self {
            size: Vec2::ONE,
            tint: Color::WHITE,
            layer: 0,
            image: None,
        }
    }
}

/// A centered 2D box or circle in local coordinates.
#[derive(Component, Schema, Debug, Clone, Copy, PartialEq)]
#[schema(
    crate = "nara_reflect",
    id = "nara.ColliderSource",
    version = 1,
    default,
    alias = "Collider 2D",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct ColliderSource {
    /// Used for a box when `radius` is absent.
    #[schema(alias = "Box Half Extents")]
    pub half_extents: Vec2,
    /// Selects a circle when present.
    #[schema(alias = "Circle Radius")]
    pub radius: Option<f32>,
}

impl Default for ColliderSource {
    fn default() -> Self {
        Self {
            half_extents: Vec2::splat(0.5),
            radius: None,
        }
    }
}

impl Schema for Visibility {
    const COMPONENT_ID: &'static str = "nara.Visibility";

    fn type_schema() -> Result<ComponentSchema, ComponentCodecError> {
        Ok(ComponentSchema::new(
            ComponentTypeId::new(Self::COMPONENT_ID),
            "Visibility",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("mode"),
            "Mode",
            ComponentFieldPath::from_fields(["mode"]),
            ComponentValueKind::String,
            ComponentValue::String("inherited".into()),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_choices(["inherited", "visible", "hidden"])]))
    }

    fn decode_component(value: ComponentValueRef<'_>) -> Result<Self, ComponentCodecError> {
        match value.field("mode")?.as_str() {
            Some("inherited") => Ok(Self::Inherited),
            Some("visible") => Ok(Self::Visible),
            Some("hidden") => Ok(Self::Hidden),
            _ => Err(ComponentCodecError::invalid_field(
                "mode",
                "inherited, visible, or hidden",
            )),
        }
    }

    fn encode_component(&self) -> Result<ComponentValue, ComponentCodecError> {
        let mode = match self {
            Self::Inherited => "inherited",
            Self::Visible => "visible",
            Self::Hidden => "hidden",
        };
        Ok(ComponentValue::map([(
            "mode",
            ComponentValue::String(mode.into()),
        )]))
    }
}

/// Built-in metadata with complete creation values for a source document inspector.
pub fn builtin_component_schemas() -> Result<[ComponentSchema; 4], ComponentCodecError> {
    Ok([
        schema_with_initial::<Transform2d>()?,
        schema_with_initial::<Visibility>()?,
        schema_with_initial::<SpriteSource>()?,
        schema_with_initial::<ColliderSource>()?,
    ])
}

fn schema_with_initial<T: Schema + Default>() -> Result<ComponentSchema, ComponentCodecError> {
    Ok(T::type_schema()?.with_authoring_initial_value(T::default().encode_component()?))
}
