//! Sprite authoring data for 2D scenes.

use nara_app::{App, Plugin};
use nara_asset::{AssetRef, AssetServer, Handle};
use nara_core::{Color, Vec2};
use nara_ecs::{Component, World};
use nara_image::ImageAsset;
use nara_reflect::{
    ComponentCodecError, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, PreparedComponent,
};

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TextureRegion {
    pub min: Vec2,
    pub size: Vec2,
}

impl TextureRegion {
    pub const FULL: Self = Self {
        min: Vec2::ZERO,
        size: Vec2::ONE,
    };

    #[must_use]
    pub const fn new(min: Vec2, size: Vec2) -> Self {
        Self { min, size }
    }

    #[must_use]
    pub fn from_pixels(min: Vec2, size: Vec2, image_size: Vec2) -> Option<Self> {
        if image_size.x <= 0.0
            || image_size.y <= 0.0
            || !image_size.is_finite()
            || !min.is_finite()
            || !size.is_finite()
            || size.x <= 0.0
            || size.y <= 0.0
        {
            return None;
        }

        let region = Self::new(min / image_size, size / image_size);
        region.is_valid_uv().then_some(region)
    }

    #[must_use]
    pub fn max(self) -> Vec2 {
        self.min + self.size
    }

    #[must_use]
    pub fn is_valid_uv(self) -> bool {
        self.min.is_finite()
            && self.size.is_finite()
            && self.size.x > 0.0
            && self.size.y > 0.0
            && self.min.x >= 0.0
            && self.min.y >= 0.0
            && self.max().x <= 1.0
            && self.max().y <= 1.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SpriteAnchor {
    pub normalized: Vec2,
}

impl SpriteAnchor {
    pub const CENTER: Self = Self {
        normalized: Vec2::ZERO,
    };
}

impl Default for SpriteAnchor {
    fn default() -> Self {
        Self::CENTER
    }
}

#[derive(Debug, Clone, PartialEq, Component)]
pub struct Sprite {
    pub texture: Option<Handle<ImageAsset>>,
    pub texture_region: Option<TextureRegion>,
    pub color: Color,
    pub size: Vec2,
    pub anchor: SpriteAnchor,
    pub layer: i32,
    pub sort_key: i32,
}

impl Sprite {
    #[must_use]
    pub fn from_color(size: Vec2, color: Color) -> Self {
        Self {
            texture: None,
            texture_region: None,
            color,
            size,
            anchor: SpriteAnchor::CENTER,
            layer: 0,
            sort_key: 0,
        }
    }

    #[must_use]
    pub fn from_texture(texture: Handle<ImageAsset>, size: Vec2) -> Self {
        Self {
            texture: Some(texture),
            texture_region: None,
            color: Color::WHITE,
            size,
            anchor: SpriteAnchor::CENTER,
            layer: 0,
            sort_key: 0,
        }
    }

    #[must_use]
    pub fn with_texture_region(mut self, region: TextureRegion) -> Self {
        self.texture_region = Some(region);
        self
    }

    #[must_use]
    pub fn with_layer(mut self, layer: i32) -> Self {
        self.layer = layer;
        self
    }

    #[must_use]
    pub fn with_sort_key(mut self, sort_key: i32) -> Self {
        self.sort_key = sort_key;
        self
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ComponentRegistry>();
        register_sprite_components(&mut app.world_mut().resource_mut::<ComponentRegistry>());
    }
}

pub fn register_sprite_components(registry: &mut ComponentRegistry) {
    registry
        .register_component_codec::<Sprite, _, _>(
            ComponentTypeId::new("nara.sprite.Sprite"),
            ComponentSchemaVersion(1),
            |value| {
                let size = read_vec2(value.field("size")?, "size")?;
                let color = read_color(value.field("color")?, "color")?;
                let texture_ref = read_optional_asset_ref(value.get("texture"), "texture")?;
                let texture_region =
                    read_optional_texture_region(value.get("texture_region"), "texture_region")?;
                let layer = optional_i32(value, "layer")?.unwrap_or(0);
                let sort_key = optional_i32(value, "sort_key")?.unwrap_or(0);

                Ok(PreparedComponent::new(move |world, entity| {
                    let texture = resolve_optional_texture(world, texture_ref.as_ref())?;
                    let sprite = Sprite {
                        texture,
                        texture_region,
                        color,
                        size,
                        anchor: SpriteAnchor::CENTER,
                        layer,
                        sort_key,
                    };
                    let mut entity_mut = world
                        .get_entity_mut(entity)
                        .map_err(|_| ComponentCodecError::EntityMissing)?;
                    entity_mut.insert(sprite);
                    Ok(())
                }))
            },
            |world, entity| {
                let Some(sprite) = world.get::<Sprite>(entity) else {
                    return Ok(None);
                };
                let texture = match sprite.texture {
                    Some(handle) => Some(asset_ref_value(
                        &AssetRef::from_handle(
                            world.get_resource::<AssetServer>().ok_or_else(|| {
                                ComponentCodecError::Message(
                                    "AssetServer resource is missing".to_string(),
                                )
                            })?,
                            handle,
                        )
                        .map_err(|error| ComponentCodecError::Message(error.to_string()))?,
                    )?),
                    None => None,
                };

                let mut fields = vec![
                    ("size", vec2_value(sprite.size)?),
                    ("color", color_value(sprite.color)?),
                    ("layer", ComponentValue::I64(i64::from(sprite.layer))),
                    ("sort_key", ComponentValue::I64(i64::from(sprite.sort_key))),
                ];
                fields.push(("texture", texture.unwrap_or(ComponentValue::Null)));
                fields.push((
                    "texture_region",
                    sprite
                        .texture_region
                        .map(texture_region_value)
                        .transpose()?
                        .unwrap_or(ComponentValue::Null),
                ));
                Ok(Some(ComponentValue::map(fields)))
            },
        )
        .expect("nara.sprite.Sprite component registration should be unique");
}

fn resolve_optional_texture(
    world: &mut World,
    texture_ref: Option<&AssetRef>,
) -> Result<Option<Handle<ImageAsset>>, ComponentCodecError> {
    let Some(texture_ref) = texture_ref else {
        return Ok(None);
    };
    if world.get_resource::<AssetServer>().is_none() {
        world.insert_resource(AssetServer::new());
    }
    texture_ref
        .resolve::<ImageAsset>(&mut world.resource_mut::<AssetServer>())
        .map(Some)
        .map_err(|error| ComponentCodecError::Message(error.to_string()))
}

fn optional_i32(value: &ComponentValue, field: &str) -> Result<Option<i32>, ComponentCodecError> {
    value
        .get(field)
        .map(|value| {
            let value = value
                .as_i64()
                .ok_or_else(|| ComponentCodecError::invalid_field(field, "i32"))?;
            i32::try_from(value).map_err(|_| ComponentCodecError::invalid_field(field, "i32"))
        })
        .transpose()
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

fn read_optional_texture_region(
    value: Option<&ComponentValue>,
    field: &str,
) -> Result<Option<TextureRegion>, ComponentCodecError> {
    match value {
        None | Some(ComponentValue::Null) => Ok(None),
        Some(value) => {
            let region = TextureRegion::new(
                read_vec2(value.field("min")?, &format!("{field}.min"))?,
                read_vec2(value.field("size")?, &format!("{field}.size"))?,
            );
            if !region.is_valid_uv() {
                return Err(ComponentCodecError::invalid_field(
                    field,
                    "valid normalized uv region",
                ));
            }
            Ok(Some(region))
        }
    }
}

fn texture_region_value(region: TextureRegion) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("min", vec2_value(region.min)?),
        ("size", vec2_value(region.size)?),
    ]))
}

fn read_color(value: &ComponentValue, field: &str) -> Result<Color, ComponentCodecError> {
    Ok(Color::rgba(
        read_f32(value.field("r")?, &format!("{field}.r"))?,
        read_f32(value.field("g")?, &format!("{field}.g"))?,
        read_f32(value.field("b")?, &format!("{field}.b"))?,
        read_f32(value.field("a")?, &format!("{field}.a"))?,
    ))
}

fn color_value(value: Color) -> Result<ComponentValue, ComponentCodecError> {
    Ok(ComponentValue::map([
        ("r", ComponentValue::f64(f64::from(value.r))?),
        ("g", ComponentValue::f64(f64::from(value.g))?),
        ("b", ComponentValue::f64(f64::from(value.b))?),
        ("a", ComponentValue::f64(f64::from(value.a))?),
    ]))
}

fn read_optional_asset_ref(
    value: Option<&ComponentValue>,
    field: &str,
) -> Result<Option<AssetRef>, ComponentCodecError> {
    match value {
        None | Some(ComponentValue::Null) => Ok(None),
        Some(value) => read_asset_ref(value, field).map(Some),
    }
}

fn read_asset_ref(value: &ComponentValue, field: &str) -> Result<AssetRef, ComponentCodecError> {
    match value.field_str("kind")? {
        "path" => AssetRef::path(value.field_str("value")?).map_err(|error| {
            ComponentCodecError::invalid_asset_ref(
                format!("{field}.value"),
                value.field_str("value").unwrap_or_default(),
                error.to_string(),
            )
        }),
        "stable_id" => Err(ComponentCodecError::invalid_asset_ref(
            format!("{field}.value"),
            value.field_str("value").unwrap_or_default(),
            "stable asset ids are reserved for the asset meta database slice",
        )),
        _ => Err(ComponentCodecError::invalid_field(
            format!("{field}.kind"),
            "'path' or 'stable_id'",
        )),
    }
}

fn asset_ref_value(asset_ref: &AssetRef) -> Result<ComponentValue, ComponentCodecError> {
    match asset_ref {
        AssetRef::Path(path) => Ok(ComponentValue::map([
            ("kind", ComponentValue::String("path".to_string())),
            ("value", ComponentValue::String(path.as_str().to_string())),
        ])),
        AssetRef::StableId(id) => Ok(ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_string())),
            ("value", ComponentValue::String(id.to_string())),
        ])),
    }
}

pub mod prelude {
    pub use crate::{Sprite, SpriteAnchor, SpritePlugin, TextureRegion};
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::AssetId;

    #[test]
    fn creates_color_sprite_with_default_authoring_state() {
        let sprite = Sprite::from_color(Vec2::new(16.0, 16.0), Color::WHITE);

        assert_eq!(sprite.texture, None);
        assert_eq!(sprite.texture_region, None);
        assert_eq!(sprite.size, Vec2::new(16.0, 16.0));
        assert_eq!(sprite.anchor, SpriteAnchor::CENTER);
        assert_eq!(sprite.layer, 0);
        assert_eq!(sprite.sort_key, 0);
    }

    #[test]
    fn creates_texture_sprite_without_backend_handles() {
        let texture = Handle::new(AssetId::from_raw(7));
        let sprite = Sprite::from_texture(texture, Vec2::new(32.0, 32.0))
            .with_texture_region(TextureRegion::new(Vec2::ZERO, Vec2::splat(0.5)));

        assert_eq!(sprite.texture, Some(texture));
        assert_eq!(
            sprite.texture_region,
            Some(TextureRegion::new(Vec2::ZERO, Vec2::splat(0.5)))
        );
        assert_eq!(sprite.color, Color::WHITE);
    }

    #[test]
    fn converts_pixel_regions_to_normalized_uv_regions() {
        let region = TextureRegion::from_pixels(
            Vec2::new(16.0, 32.0),
            Vec2::new(16.0, 16.0),
            Vec2::new(64.0, 128.0),
        )
        .unwrap();

        assert_eq!(
            region,
            TextureRegion::new(Vec2::new(0.25, 0.25), Vec2::new(0.25, 0.125))
        );
        assert!(region.is_valid_uv());
    }

    #[test]
    fn records_layer_and_sort_key() {
        let sprite = Sprite::from_color(Vec2::new(8.0, 8.0), Color::WHITE)
            .with_layer(3)
            .with_sort_key(-4);

        assert_eq!(sprite.layer, 3);
        assert_eq!(sprite.sort_key, -4);
    }
}
