//! Authored 2D components with typed runtime codecs.

use bevy_ecs::component::Component;
use nara_core::{AssetId, Color, Vec2};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueRef, PackedU16, Schema, validate_component_value,
};
use thiserror::Error;

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

/// Starts property animation from a stable scene dependency.
#[derive(Component, Schema, Clone, Debug, PartialEq, Eq)]
#[schema(
    crate = "nara_reflect",
    id = "nara.AnimationSource",
    version = 1,
    alias = "Property Animation",
    component_capabilities(scene, inspect, edit)
)]
pub struct AnimationSource {
    #[schema(capabilities(scene, inspect, edit, asset_ref))]
    pub clip: AssetId,
}

/// Selects a named sprite clip from a stable scene dependency.
#[derive(Component, Schema, Clone, Debug, PartialEq, Eq)]
#[schema(
    crate = "nara_reflect",
    id = "nara.SpriteAnimatorSource",
    version = 1,
    alias = "Sprite Animation",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct SpriteAnimatorSource {
    #[schema(capabilities(scene, inspect, edit, asset_ref))]
    pub set: AssetId,
    pub clip: String,
}

/// An authored tile grid in row-major order, with rows increasing in local +Y.
/// Cell zero is empty. A nonzero cell selects tile `cell - 1` from the tileset.
#[derive(Component, Schema, Debug, Clone, PartialEq)]
#[require(Transform2d)]
#[schema(
    crate = "nara_reflect",
    id = "nara.TilemapSource",
    version = 1,
    alias = "Tilemap"
)]
pub struct TilemapSource {
    #[schema(capabilities(scene, inspect, edit, asset_ref))]
    pub tileset: AssetId,
    /// Size of one tile in scene units, independent of the atlas pixel dimensions.
    pub tile_size: Vec2,
    pub width: u32,
    pub height: u32,
    #[schema(default)]
    pub origin_x: i32,
    #[schema(default)]
    pub origin_y: i32,
    #[schema(default)]
    pub layer: i32,
    pub cells: PackedU16,
}

impl TilemapSource {
    pub fn validate(&self) -> Result<(), TilemapSourceError> {
        if !self.tile_size.is_finite() || self.tile_size.min_element() <= 0.0 {
            return Err(TilemapSourceError::TileSize);
        }
        let expected = u64::from(self.width) * u64::from(self.height);
        if self.cells.0.len() as u64 != expected {
            return Err(TilemapSourceError::CellCount {
                expected,
                actual: self.cells.0.len(),
            });
        }
        for (origin, size, step) in [
            (self.origin_x, self.width, self.tile_size.x),
            (self.origin_y, self.height, self.tile_size.y),
        ] {
            let last = i64::from(origin) + i64::from(size.saturating_sub(1));
            if last > i64::from(i32::MAX)
                || !(origin as f32 * step).is_finite()
                || !((last + 1) as f32 * step).is_finite()
            {
                return Err(TilemapSourceError::Bounds);
            }
        }
        Ok(())
    }

    pub fn validate_tile_indices(&self, tiles: usize) -> Result<(), TilemapSourceError> {
        if let Some((cell, &value)) = self
            .cells
            .0
            .iter()
            .enumerate()
            .find(|(_, value)| usize::from(**value) > tiles)
        {
            return Err(TilemapSourceError::TileIndex { cell, value, tiles });
        }
        Ok(())
    }

    /// Converts an occupied or empty cell's row-major index to its local tile coordinates.
    #[must_use]
    pub fn cell_coordinates(&self, index: usize) -> Option<(i32, i32)> {
        let width = self.width as usize;
        if width == 0 || index >= self.cells.0.len() || index / width >= self.height as usize {
            return None;
        }
        let x = i64::from(self.origin_x) + (index % width) as i64;
        let y = i64::from(self.origin_y) + (index / width) as i64;
        Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?))
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TilemapSourceError {
    #[error("tile size must be finite and positive")]
    TileSize,
    #[error("expected {expected} tile cells, found {actual}")]
    CellCount { expected: u64, actual: usize },
    #[error("the tile grid exceeds representable coordinates")]
    Bounds,
    #[error("cell {cell} selects tile {value} from a tileset with {tiles} tiles")]
    TileIndex {
        cell: usize,
        value: u16,
        tiles: usize,
    },
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
pub fn builtin_component_schemas() -> Result<Vec<ComponentSchema>, ComponentCodecError> {
    Ok(vec![
        schema_with_initial::<Transform2d>()?,
        schema_with_initial::<Visibility>()?,
        schema_with_initial::<SpriteSource>()?,
        schema_with_initial::<ColliderSource>()?,
        TilemapSource::type_schema()?,
        AnimationSource::type_schema()?,
        SpriteAnimatorSource::type_schema()?,
    ])
}

fn schema_with_initial<T: Schema + Default>() -> Result<ComponentSchema, ComponentCodecError> {
    Ok(T::type_schema()?.with_authoring_initial_value(T::default().encode_component()?))
}
