//! Authored 2D components with typed runtime codecs.

use bevy_ecs::component::Component;
use nara_core::{AssetId, Color, Vec2};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueRef, Schema, validate_component_value,
};

pub use crate::spatial::{Transform2d, Visibility};

/// A source quad with linear tint and optional stable image and material IDs.
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
    #[schema(alias = "Material")]
    pub material: Option<AssetId>,
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
            material: None,
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
        .with_postcard_codec()
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
        validate_component_value(&Self::type_schema()?, value)?;
        match value.get("mode") {
            None => Ok(Self::Inherited),
            Some(mode) if mode.kind() == ComponentValueKind::Null => Ok(Self::Inherited),
            Some(mode) => {
                Self::from_schema_mode(mode.as_str().ok_or_else(Self::invalid_schema_mode)?)
            }
        }
    }

    fn encode_component(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::map([(
            "mode",
            ComponentValue::String(self.schema_mode().into()),
        )]))
    }

    nara_reflect::__schema_postcard_methods! {
        fn encode_postcard(&self) -> Result<Vec<u8>, ComponentCodecError> {
            let mut bytes = Vec::new();
            nara_reflect::__macro_support::write_postcard_field(&mut bytes, &Some(self.schema_mode().to_owned()), "mode")?;
            Ok(bytes)
        }

        fn decode_postcard(mut bytes: &[u8]) -> Result<Self, ComponentCodecError> {
            let mode: Option<String> = nara_reflect::__macro_support::read_postcard_field(&mut bytes, "mode")?;
            nara_reflect::__macro_support::finish_postcard(bytes)?;
            match mode.as_deref() {
                Some(mode) => Self::from_schema_mode(mode),
                None => Ok(Self::Inherited),
            }
        }
    }
}

impl Visibility {
    const fn schema_mode(self) -> &'static str {
        match self {
            Self::Inherited => "inherited",
            Self::Visible => "visible",
            Self::Hidden => "hidden",
        }
    }

    fn from_schema_mode(mode: &str) -> Result<Self, ComponentCodecError> {
        match mode {
            "inherited" => Ok(Self::Inherited),
            "visible" => Ok(Self::Visible),
            "hidden" => Ok(Self::Hidden),
            _ => Err(Self::invalid_schema_mode()),
        }
    }

    fn invalid_schema_mode() -> ComponentCodecError {
        ComponentCodecError::invalid_field("mode", "inherited, visible, or hidden")
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
