use nara_asset::Handle;
use nara_core::Vec2;
use nara_image::ImageAsset;
use nara_material::{
    AlphaMode2d, Material2dDefinition, Material2dDefinitions, Material2dError, Material2dInput,
    Material2dInstance, SamplerDescriptor,
};
use nara_sprite::Sprite;
use nara_tilemap::Tilemap;

static DEFINITIONS: [Material2dDefinition; 1] = [Material2dDefinition {
    id: "consumer.palette",
    fragment_shader: include_str!("palette.wgsl"),
    input: Material2dInput::SPRITE_FRAGMENT_V1,
    alpha_mode: AlphaMode2d::Blend,
}];

pub fn startup_materials() -> Result<Material2dDefinitions, Material2dError> {
    Material2dDefinitions::new(&DEFINITIONS)
}

pub fn palette_sprite(
    materials: &Material2dDefinitions,
    indices: Handle<ImageAsset>,
    palette: Handle<ImageAsset>,
    offset: f32,
) -> Result<(Sprite, Material2dInstance), Material2dError> {
    Ok((
        Sprite::from_texture(indices, Vec2::splat(16.0))
            .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
            .with_layer(0)
            .with_sort_key(1)
            .with_y_sort_anchor(Vec2::new(0.0, -8.0)),
        materials.instantiate("consumer.palette", [offset, 1.0, 0.0, 0.0], palette)?,
    ))
}

pub fn foreground_tiles() -> Tilemap {
    Tilemap::new(Vec2::splat(16.0)).with_layer(1)
}
