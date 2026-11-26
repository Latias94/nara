use std::path::Path;

use bevy_ecs::component::Component;
use nara_core::{Color, Vec2, vfs::AssetPath};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueRef, PersistentComponent, PersistentComponentProvider,
};

use crate::spatial::{Transform2d, Visibility};

/// A source quad with linear tint and an optional PNG path relative to the project asset root.
#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    crate = "nara_reflect",
    id = "nara.SpriteSource",
    version = 1,
    alias = "Sprite",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct SpriteSource {
    #[nara(id = "size", alias = "Size")]
    pub size: Vec2,
    #[nara(id = "tint", alias = "Tint")]
    pub tint: Color,
    #[nara(id = "layer", alias = "Layer")]
    pub layer: i32,
    #[nara(id = "image", alias = "PNG Image")]
    pub image: Option<String>,
}

impl SpriteSource {
    #[must_use]
    pub const fn tint(&self) -> Color {
        self.tint
    }

    pub fn image_asset_path(&self) -> Result<Option<AssetPath>, ComponentCodecError> {
        self.image.as_deref().map(png_asset_path).transpose()
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

pub(super) fn png_asset_path(path: &str) -> Result<AssetPath, ComponentCodecError> {
    let invalid = || ComponentCodecError::invalid_field("image", "a portable PNG asset path");
    if !Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        return Err(invalid());
    }
    AssetPath::new(path).map_err(|_| invalid())
}

/// A centered 2D box or circle in local coordinates.
#[derive(Component, PersistentComponent, Debug, Clone, Copy, PartialEq)]
#[nara(
    crate = "nara_reflect",
    id = "nara.ColliderSource",
    version = 1,
    alias = "Collider 2D",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct ColliderSource {
    /// Used for a box when `radius` is absent.
    #[nara(id = "half_extents", alias = "Box Half Extents")]
    pub half_extents: Vec2,
    /// Selects a circle when present.
    #[nara(id = "radius", alias = "Circle Radius")]
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

impl PersistentComponentProvider for Visibility {
    const COMPONENT_ID: &'static str = "nara.Visibility";

    fn persistent_component_schema() -> ComponentSchema {
        ComponentSchema::new(
            ComponentTypeId::new(Self::COMPONENT_ID),
            "Visibility",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("mode"),
            "Mode",
            ComponentFieldPath::from_fields(["mode"]),
            ComponentValueKind::String,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_choices(["inherited", "visible", "hidden"])])
    }

    fn decode_persistent_component(
        value: ComponentValueRef<'_>,
    ) -> Result<Self, ComponentCodecError> {
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

    fn encode_persistent_component(&self) -> Result<ComponentValue, ComponentCodecError> {
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

fn schema_with_initial<T: PersistentComponentProvider + Default>()
-> Result<ComponentSchema, ComponentCodecError> {
    Ok(T::persistent_component_schema()
        .with_authoring_initial_value(T::default().encode_persistent_component()?))
}
