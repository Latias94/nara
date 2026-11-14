use std::collections::BTreeMap;

use engine::{
    advanced_prelude::CurrentScene,
    app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset::{
        AssetDependencyGraph, AssetRef, AssetReloadRequests, AssetServer, AssetSourceChanges,
        AssetSourceRoot, AssetStates, Assets, ProjectAssetDatabase,
    },
    ecs::{Entity, Resource, World},
    identity::EntityLookup,
    image::{ImageAsset, ImageColorSpace, ImageDomain, ImageExtent, ImageFormat, ImageImporter},
    material::{Material2d, Material2dAsset, Material2dDomain},
    project_host::ProjectDomainAssets,
    scene::{SceneEntityId, SourceDocumentKey, SourceDocumentKind},
    sprite::Sprite,
};

#[derive(Clone, Debug, Default, Resource)]
pub struct Outcome {
    pub frames: u32,
}

#[derive(Default)]
pub struct InspectPalette;

impl Plugin for InspectPalette {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.palette.cooked-probe"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Outcome>()?
            .add_systems(CoreStage::Update, inspect)?;
        Ok(())
    }
}

fn inspect(world: &mut World) {
    assert!(!std::path::Path::new(env!("CARGO_MANIFEST_DIR")).exists());
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert!(!world.contains_resource::<AssetDependencyGraph>());
    assert!(!world.contains_resource::<AssetSourceRoot>());
    assert!(!world.contains_resource::<AssetSourceChanges>());
    assert!(!world.contains_resource::<AssetReloadRequests>());
    assert!(!world.contains_resource::<ImageImporter>());
    assert_eq!(world.query::<&Sprite>().iter(world).count(), 1);
    assert_eq!(world.query::<&Material2d>().iter(world).count(), 1);
    let (entity, indices, material) = {
        let mut query = world.query::<(Entity, &Sprite, &Material2d)>();
        let mut values = query.iter(world);
        let (entity, sprite, material) = values.next().unwrap();
        let result = (entity, sprite.material.image.unwrap(), material.clone());
        assert!(values.next().is_none());
        result
    };
    let current = CurrentScene::get(world).unwrap();
    let actor = SceneEntityId::new("palette-sprite").unwrap();
    assert_eq!(current.source().roots(), [actor.clone()]);
    assert_eq!(
        current.receipt().resolve(world, &actor),
        EntityLookup::Resolved(entity)
    );
    assert!(current.source_content_stamp().is_none());
    let view = current.source_view();
    assert!(view.with_compiled_scene(|_, _| ()).is_none());
    assert_eq!(
        view.with_runtime_scene(|candidate| candidate.source_key().clone()),
        Some(SourceDocumentKey::stable_asset(
            crate::scene_product().source(),
            SourceDocumentKind::Scene,
        )),
    );

    let server = world.resource::<AssetServer>();
    let materials = world.resource::<Assets<Material2dAsset>>();
    let images = world.resource::<Assets<ImageAsset>>();
    assert_eq!(materials.len(), 1);
    assert_eq!(images.len(), 2);
    assert_eq!(
        server.product(material.handle().id()),
        Some(crate::material_product())
    );
    assert_eq!(
        server.get_product_handle::<Material2dAsset>(crate::material_product()),
        Some(material.handle())
    );
    assert!(server.path(material.handle().id()).is_none());
    assert_eq!(material.declaration_id(), crate::MATERIAL_ID);
    assert_eq!(material.declaration_version(), 1);
    assert_eq!(material.parameters(), &crate::parameters(2.0));
    assert!(material.texture_overrides().is_empty());
    let asset = materials.get(material.handle()).unwrap();
    assert_eq!(asset.product(), crate::material_product());
    assert_eq!(asset.generation().declaration(), &crate::declaration());
    assert!(asset.generation().inputs().is_empty());
    assert_eq!(asset.generation().sequence(), 0);
    assert_eq!(
        asset.generation().shader().user_source(),
        format!("{}\n", crate::SHADER)
    );
    assert_eq!(asset.default_parameters(), &crate::parameters(0.0));
    assert_eq!(
        asset.textures(),
        &BTreeMap::from([("palette".to_owned(), AssetRef::Product(crate::palette_product()))]),
    );
    let domains = world.resource::<ProjectDomainAssets>();
    let collection = domains.collection::<Material2dDomain>().unwrap();
    assert_eq!(collection.iter().len(), 1);
    assert_eq!(collection.source_records().len(), 0);
    assert_eq!(
        collection
            .get(&AssetRef::Product(crate::material_product()))
            .unwrap()
            .generation(),
        asset.generation(),
    );

    let mut resolved = Vec::new();
    let binding = material
        .bind(asset, |reference| {
            resolved.push(reference.clone());
            let AssetRef::Product(product) = reference else {
                panic!("cooked Material defaults must contain complete Products");
            };
            server.get_product_handle::<ImageAsset>(*product)
        })
        .unwrap();
    assert_eq!(resolved, [AssetRef::Product(crate::palette_product())]);
    assert_eq!(binding.generation(), asset.generation());
    assert_eq!(binding.parameters(), &crate::parameters(2.0));
    let expected_uniform = [2.0_f32, 1.0, 0.0, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    assert_eq!(binding.uniform_bytes(), expected_uniform.as_slice());
    assert_eq!(binding.textures().len(), 1);
    let palette = binding.textures()["palette"];
    assert_ne!(indices, palette);
    let image_collection = domains.collection::<ImageDomain>().unwrap();
    assert_eq!(image_collection.iter().len(), 2);
    assert_eq!(image_collection.source_records().len(), 0);
    for (product, handle) in [
        (crate::indices_product(), indices),
        (crate::palette_product(), palette),
    ] {
        assert_eq!(server.product(handle.id()), Some(product));
        assert_eq!(
            server.get_product_handle::<ImageAsset>(product),
            Some(handle)
        );
        assert!(server.path(handle.id()).is_none());
        let image = images.get(handle).unwrap();
        assert_eq!(image.product(), Some(product));
        assert_eq!(image.extent(), ImageExtent::new(1, 1));
        assert_eq!(image.format(), ImageFormat::Rgba8);
        assert_eq!(image.color_space(), ImageColorSpace::Srgb);
        assert!(image.source().is_none());
        assert_eq!(image.pixels(), crate::PIXELS);
        assert_eq!(
            image_collection
                .get(&AssetRef::Product(product))
                .unwrap()
                .pixels()
                .as_ptr(),
            image.pixels().as_ptr(),
        );
    }
    let states = world.resource::<AssetStates>();
    for id in [material.handle().id(), indices.id(), palette.id()] {
        let state = states.state(id).unwrap();
        assert!(state.source_hash().is_none());
        assert!(state.import_recipe_digest().is_none());
        assert!(state.usable_version().is_some());
    }
    world.resource_mut::<Outcome>().frames += 1;
}
