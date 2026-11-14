#![forbid(unsafe_code)]

use engine::{
    ProductRecipe,
    asset::{AssetProductRef, ImportedProductId, StableAssetId},
    image::ImageRuntimeCodec,
    material::{
        AlphaMode2d, Material2dDeclaration, Material2dDomain, Material2dRuntimeCodec,
        MaterialParameterValues, MaterialParameters, MaterialTextureDeclaration,
    },
    runtime_content::RuntimeCodecDefinitionId,
};

pub mod probe;

pub const MATERIAL_ID: &str = "consumer.palette";
pub const MATERIAL_PATH: &str = "palette.material2d.json";
pub const SHADER_PATH: &str = "palette.wgsl";
pub const SHADER: &str = include_str!("palette.wgsl");
pub const PIXELS: [u8; 4] = [24, 120, 220, 255];

#[derive(MaterialParameters)]
#[material(crate = "engine::material")]
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

pub fn parameters(offset: f32) -> MaterialParameterValues {
    PaletteParameters {
        x: offset,
        y: 1.0,
        z: 0.0,
        w: 0.0,
    }
    .parameter_values()
    .unwrap()
}

pub fn declaration() -> Material2dDeclaration {
    Material2dDeclaration::from_parameters::<PaletteParameters>(
        MATERIAL_ID,
        1,
        AlphaMode2d::Blend,
        vec![MaterialTextureDeclaration::new("palette", "material_texture", 1).unwrap()],
    )
    .unwrap()
}

pub fn material_product() -> AssetProductRef {
    primary("744cab2d-4c3c-43f0-bd13-8d72a8c698b1")
}

pub fn indices_product() -> AssetProductRef {
    primary("744cab2d-4c3c-43f0-bd13-8d72a8c698b2")
}

pub fn palette_product() -> AssetProductRef {
    primary("744cab2d-4c3c-43f0-bd13-8d72a8c698b3")
}

pub fn scene_product() -> AssetProductRef {
    primary("744cab2d-4c3c-43f0-bd13-8d72a8c698b4")
}

fn primary(id: &str) -> AssetProductRef {
    AssetProductRef::new(StableAssetId::parse_str(id).unwrap(), ImportedProductId::PRIMARY).unwrap()
}

pub fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_runtime_asset_codec::<Material2dDomain, Material2dRuntimeCodec>(
            RuntimeCodecDefinitionId::new("consumer.palette.cooked.material", 1),
        )
        .unwrap()
        .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
            "consumer.palette.cooked.image",
            1,
        ))
        .unwrap()
        .add_plugin::<probe::InspectPalette>()
        .unwrap()
}
