use std::{collections::BTreeMap, sync::Arc};

use nara_asset::{AssetPath, Handle};
use nara_core::Vec2;
use nara_image::ImageAsset;
use nara_material::{
    AlphaMode2d, Material2dBinding, Material2dDeclaration, Material2dLibrary, MaterialParameters,
    MaterialTextureDeclaration, MaterialUpdateError, SamplerDescriptor,
};
use nara_sprite::Sprite;
use nara_tilemap::Tilemap;

const MATERIAL_ID: &str = "consumer.palette";

#[derive(MaterialParameters)]
struct PaletteParameters {
    #[material(id = "palette.offset")]
    x: f32,
    #[material(id = "palette.strength")]
    y: f32,
    #[material(id = "palette.reserved-z")]
    z: f32,
    #[material(id = "palette.reserved-w")]
    w: f32,
}

pub fn startup_materials() -> Result<Material2dLibrary, MaterialUpdateError> {
    let mut materials = Material2dLibrary::default();
    materials.declare(Material2dDeclaration::from_parameters::<PaletteParameters>(
        MATERIAL_ID,
        1,
        AlphaMode2d::Blend,
        vec![MaterialTextureDeclaration::new(
            "palette",
            "material_texture",
            1,
        )?],
    )?)?;
    let shader_path = AssetPath::new("palette.wgsl").expect("the palette shader path is valid");
    let read_shader = |path: &AssetPath| match path.as_str() {
        "palette.wgsl" => Ok(Arc::<str>::from(include_str!("palette.wgsl"))),
        _ => Err(format!("unknown palette shader source {path}")),
    };
    let prepared =
        materials.prepare_update(MATERIAL_ID, std::slice::from_ref(&shader_path), read_shader)?;
    materials.publish(&prepared, read_shader)?;
    Ok(materials)
}

pub fn palette_sprite(
    materials: &Material2dLibrary,
    indices: Handle<ImageAsset>,
    palette: Handle<ImageAsset>,
    offset: f32,
) -> Result<(Sprite, Material2dBinding), MaterialUpdateError> {
    let generation = materials
        .current(MATERIAL_ID)
        .ok_or_else(|| MaterialUpdateError::UnknownDeclaration(MATERIAL_ID.to_owned()))?;
    Ok((
        Sprite::from_texture(indices, Vec2::splat(16.0))
            .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
            .with_layer(0)
            .with_sort_key(1)
            .with_y_sort_anchor(Vec2::new(0.0, -8.0)),
        generation.bind(
            &PaletteParameters {
                x: offset,
                y: 1.0,
                z: 0.0,
                w: 0.0,
            },
            BTreeMap::from([("palette".to_owned(), palette)]),
        )?,
    ))
}

pub fn foreground_tiles() -> Tilemap {
    Tilemap::new(Vec2::splat(16.0)).with_layer(1)
}
