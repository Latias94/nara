//! Shared authored color grading for the Side Room and Lookout relics.

use nara::{
    asset::{AssetProductRef, ImportedProductId, StableAssetId},
    material::{
        AlphaMode2d, Material2dDeclaration, MaterialParameterValues, MaterialParameters,
        MaterialTextureDeclaration, Vec4,
    },
};

use crate::levels::LevelId;

pub const MATERIAL_ID: &str = "two-level-game.region";
pub const MATERIAL_VERSION: u32 = 1;
pub const MATERIAL_PATH: &str = "materials/region.material2d.json";
pub const SHADER_PATH: &str = "materials/region.wgsl";
pub const SHADER: &str = include_str!("../assets/materials/region.wgsl");
pub const GRADE_ID: &str = "region.grade";
pub const PALETTE_ID: &str = "palette";
pub const PALETTE_PATH: &str = "textures/meadow.png";
pub const DEFAULT_GRADE: [f32; 4] = [1.0, 1.0, 1.0, 0.0];
pub const GRADE: [f32; 4] = [1.0, 0.5, 0.25, 0.125];
pub const SOURCE_PIXEL: [u8; 4] = [64, 224, 180, 255];
pub const PALETTE_PIXEL: [u8; 4] = [255, 255, 255, 0];
pub const GRADED_PIXEL: [u8; 4] = [117, 187, 134, 255];

#[derive(MaterialParameters)]
#[material(crate = "nara::material")]
struct RegionParameters {
    #[material(id = "region.grade")]
    grade: Vec4,
}

#[must_use]
pub fn material_product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str("89cdbf37-e72e-4de4-a2cd-cfb9df526f6d")
            .expect("valid region Material Product identity"),
        ImportedProductId::PRIMARY,
    )
    .expect("non-nil region Material Product identity")
}

#[must_use]
pub fn palette_product() -> AssetProductRef {
    LevelId::Meadow.image_product()
}

#[must_use]
pub fn declaration() -> Material2dDeclaration {
    Material2dDeclaration::from_parameters::<RegionParameters>(
        MATERIAL_ID,
        MATERIAL_VERSION,
        AlphaMode2d::Blend,
        vec![
            MaterialTextureDeclaration::new(PALETTE_ID, "region_palette", 1)
                .expect("valid region palette texture declaration"),
        ],
    )
    .expect("valid region Material declaration")
}

#[must_use]
pub fn parameters() -> MaterialParameterValues {
    values(GRADE)
}

#[must_use]
pub fn default_parameters() -> MaterialParameterValues {
    values(DEFAULT_GRADE)
}

fn values(grade: [f32; 4]) -> MaterialParameterValues {
    RegionParameters {
        grade: Vec4::from_array(grade),
    }
    .parameter_values()
    .expect("finite region Material parameters")
}
