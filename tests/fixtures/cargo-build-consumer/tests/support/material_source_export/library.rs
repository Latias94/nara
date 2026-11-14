use std::collections::BTreeMap;

use engine::{
    ProductRecipe,
    app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset::{AssetProductRef, AssetRef, AssetServer, Assets, ImportedProductId, StableAssetId},
    ecs::World,
    image::ImageAsset,
    material::{
        AlphaMode2d, Material2d, Material2dAsset, Material2dDeclaration, Material2dDomain,
        MaterialParameterValues, MaterialParameters, MaterialTextureDeclaration,
    },
    sprite::Sprite,
};

pub use engine::project_tooling::{ProductObservationTarget, record_product_observation};

pub const MATERIAL_ID: &str = "consumer.palette";
pub const MATERIAL_SOURCE: &str = "744cab2d-4c3c-43f0-bd13-8d72a8c698b1";
pub const INDICES_SOURCE: &str = "744cab2d-4c3c-43f0-bd13-8d72a8c698b2";
pub const PALETTE_SOURCE: &str = "744cab2d-4c3c-43f0-bd13-8d72a8c698b3";
pub const RESIDENCY_MARKER: &str =
    "palette_source_product: ok material_default_texture_resident=true";

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

pub fn product_recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_asset_domain::<Material2dDomain>()
        .unwrap()
        .add_plugin::<VerifyPalette>()
        .unwrap()
}

pub fn primary(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
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

#[derive(Default)]
struct VerifyPalette;

impl Plugin for VerifyPalette {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.palette.source-export-verification"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(CoreStage::FixedUpdate, verify_palette)?;
        Ok(())
    }
}

fn verify_palette(world: &mut World) {
    let (material, indices) = {
        let mut query = world.query::<(&Sprite, &Material2d)>();
        let mut sprites = query.iter(world);
        let (sprite, material) = sprites.next().expect("the Palette Sprite must be spawned");
        let result = (material.clone(), sprite.material.image.unwrap());
        assert!(sprites.next().is_none());
        result
    };
    let server = world.resource::<AssetServer>();
    let materials = world.resource::<Assets<Material2dAsset>>();
    let images = world.resource::<Assets<ImageAsset>>();
    assert_eq!(materials.len(), 1);
    assert_eq!(images.len(), 2);
    let asset = materials.get(material.handle()).unwrap();
    assert_eq!(asset.product(), primary(MATERIAL_SOURCE));
    assert_eq!(asset.generation().declaration(), &declaration());
    assert_eq!(asset.generation().inputs().len(), 1);
    assert_eq!(
        asset.generation().inputs()[0].path().as_str(),
        "palette.wgsl"
    );
    assert_eq!(asset.default_parameters(), &parameters(0.0));
    assert_eq!(material.parameters(), &parameters(1.0));
    assert!(material.texture_overrides().is_empty());
    assert_eq!(
        asset.textures(),
        &BTreeMap::from([(
            "palette".to_owned(),
            AssetRef::Product(primary(PALETTE_SOURCE)),
        )]),
    );
    let mut resolved = Vec::new();
    let binding = material
        .bind(asset, |reference| {
            resolved.push(reference.clone());
            let AssetRef::Product(product) = reference else {
                panic!("resident Material defaults must resolve to a Product");
            };
            server.get_product_handle::<ImageAsset>(*product)
        })
        .unwrap();
    assert_eq!(resolved, [AssetRef::Product(primary(PALETTE_SOURCE))]);
    assert_eq!(binding.parameters(), &parameters(1.0));
    assert_eq!(binding.generation(), asset.generation());
    let palette = binding.textures()["palette"];
    assert_ne!(indices, palette);
    assert_eq!(
        images.get(indices).unwrap().product(),
        Some(primary(INDICES_SOURCE)),
    );
    assert_eq!(
        images.get(palette).unwrap().product(),
        Some(primary(PALETTE_SOURCE)),
    );
    println!("{RESIDENCY_MARKER}");
}
