use std::collections::BTreeMap;

use nara::{
    advanced_prelude::CurrentScene,
    asset::{
        AssetProductRef, AssetRef, AssetReloadRequests, AssetServer, AssetSourceChanges,
        AssetState, AssetStateRevision, AssetStates, Assets, Handle, LoadState,
    },
    ecs::{Entity, World},
    identity::{EntityLookup, SpawnedSceneInstance},
    image::{ImageAsset, ImageContentHash, ImageExtent},
    material::{Material2d, Material2dAsset, Material2dDomain, MaterialParameterValues},
    project_host::{
        ProjectDomainAssets, ProjectSceneContentStamp, ProjectSceneRequestId, ProjectScenes,
    },
    sprite::Sprite,
};

use super::{
    GameProgress,
    project::{MATERIAL_ID, actor, indices_product, material_product, parameters},
};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct MaterialFacts {
    pub inputs: Vec<(String, String)>,
    pub shader: [u8; 32],
    pub interface: [u8; 32],
    pub declaration: [u8; 32],
    pub texture: AssetProductRef,
    pub defaults: MaterialParameterValues,
}

impl MaterialFacts {
    fn capture(asset: &Material2dAsset) -> Self {
        assert_eq!(asset.product(), material_product());
        let generation = asset.generation();
        assert_eq!(generation.declaration().id(), MATERIAL_ID);
        assert_eq!(generation.declaration().version(), 1);
        assert_eq!(asset.default_parameters(), &parameters(0.0));
        assert_eq!(asset.textures().len(), 1);
        let AssetRef::Product(texture) = &asset.textures()["palette"] else {
            panic!("the Material default must select a complete Image Product");
        };
        Self {
            inputs: generation
                .inputs()
                .iter()
                .map(|input| (input.path().as_str().to_owned(), input.source().to_owned()))
                .collect(),
            shader: generation.shader().digest(),
            interface: generation.shader().interface_digest(),
            declaration: generation.declaration().semantic_digest(),
            texture: *texture,
            defaults: asset.default_parameters().clone(),
        }
    }

    pub fn assert_shader(&self, path: &str, source: &str) {
        assert_eq!(self.inputs, [(path.to_owned(), source.to_owned())]);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Candidate {
    pub stamp: ProjectSceneContentStamp,
    pub material: MaterialFacts,
}

pub(super) fn prepared(world: &mut World, request: ProjectSceneRequestId) -> Candidate {
    let expected = CurrentScene::get(world).unwrap().receipt().clone();
    ProjectScenes::scope(world, |scenes| {
        let view = scenes.prepared(request).unwrap();
        assert_eq!(view.expected_instance(), &expected);
        assert_eq!(
            view.selected_scene_path().unwrap().as_str(),
            "startup.scene.json",
        );
        assert!(view.selected_scene_product().is_none());
        assert_eq!(view.scene().roots(), [actor()]);
        let collection = view
            .domain_assets()
            .collection::<Material2dDomain>()
            .unwrap();
        assert_eq!(collection.iter().len(), 1);
        let asset = collection
            .get(&AssetRef::Product(material_product()))
            .unwrap();
        Candidate {
            stamp: view.source_content_stamp().unwrap(),
            material: MaterialFacts::capture(asset),
        }
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ResidentImage {
    pub handle: Handle<ImageAsset>,
    pub hash: ImageContentHash,
    pub pixels: Vec<u8>,
    pub state: AssetState,
    pub revision: AssetStateRevision,
    pub pixel_allocation: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Resident {
    pub entity: Entity,
    pub instance: SpawnedSceneInstance,
    pub stamp: ProjectSceneContentStamp,
    pub material: MaterialFacts,
    pub material_handle: Handle<Material2dAsset>,
    pub material_state: AssetState,
    pub material_revision: AssetStateRevision,
    pub images: BTreeMap<AssetProductRef, ResidentImage>,
    pub bound_texture: Handle<ImageAsset>,
    pub uniform: Vec<u8>,
    pub gameplay: GameProgress,
}

pub(super) fn assert_scene_owns_updates(world: &World) {
    assert!(
        world
            .get_resource::<AssetSourceChanges>()
            .is_none_or(AssetSourceChanges::is_empty),
        "Scene-owned source updates must not broadcast into the generic asset route",
    );
    assert!(
        world
            .get_resource::<AssetReloadRequests>()
            .is_none_or(AssetReloadRequests::is_empty),
    );
}

pub(super) fn resident(world: &mut World) -> Resident {
    assert_scene_owns_updates(world);
    assert_eq!(world.query::<&Sprite>().iter(world).count(), 1);
    assert_eq!(world.query::<&Material2d>().iter(world).count(), 1);
    let (entity, sprite, material, gameplay) = {
        let mut query = world.query::<(Entity, &Sprite, &Material2d, &GameProgress)>();
        let mut values = query.iter(world);
        let (entity, sprite, material, gameplay) = values.next().unwrap();
        let values_owned = (entity, sprite.clone(), material.clone(), gameplay.clone());
        assert!(values.next().is_none());
        values_owned
    };
    let current = CurrentScene::get(world).unwrap();
    assert_eq!(current.source().roots(), [actor()]);
    assert_eq!(
        current.receipt().resolve(world, &actor()),
        EntityLookup::Resolved(entity),
    );
    let instance = current.receipt().clone();
    let stamp = current.source_content_stamp().unwrap();
    let server = world.resource::<AssetServer>();
    let materials = world.resource::<Assets<Material2dAsset>>();
    let images = world.resource::<Assets<ImageAsset>>();
    let states = world.resource::<AssetStates>();
    assert_eq!(materials.len(), 1);
    assert_eq!(material.declaration_id(), MATERIAL_ID);
    assert_eq!(material.declaration_version(), 1);
    assert_eq!(material.parameters(), &parameters(0.0));
    assert!(material.texture_overrides().is_empty());
    let handle = material.handle();
    assert_eq!(server.product(handle.id()), Some(material_product()));
    assert_eq!(
        server.get_product_handle::<Material2dAsset>(material_product()),
        Some(handle),
    );
    let asset = materials.get(handle).unwrap();
    let facts = MaterialFacts::capture(asset);
    let domains = world.resource::<ProjectDomainAssets>();
    let collection = domains.collection::<Material2dDomain>().unwrap();
    assert_eq!(collection.iter().len(), 1);
    assert_eq!(
        collection
            .get(&AssetRef::Product(material_product()))
            .unwrap()
            .generation(),
        asset.generation(),
    );
    let mut resolved = Vec::new();
    let binding = material
        .bind(asset, |reference| {
            resolved.push(reference.clone());
            let AssetRef::Product(product) = reference else {
                panic!("resident defaults must retain their resolved Product identity");
            };
            server.get_product_handle::<ImageAsset>(*product)
        })
        .unwrap();
    assert_eq!(resolved, [AssetRef::Product(facts.texture)]);
    assert_eq!(binding.generation(), asset.generation());
    assert_eq!(binding.parameters(), material.parameters());
    assert_eq!(binding.textures().len(), 1);
    let expected_uniform = [0.0_f32, 1.0, 0.0, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    assert_eq!(binding.uniform_bytes(), expected_uniform.as_slice());
    let bound_texture = binding.textures()["palette"];
    let indices = sprite.material.image.unwrap();
    assert_eq!(
        server.get_product_handle::<ImageAsset>(indices_product()),
        Some(indices),
    );
    assert_eq!(
        server.get_product_handle::<ImageAsset>(facts.texture),
        Some(bound_texture),
    );
    let mut image_observations = BTreeMap::new();
    for (product, handle) in [(indices_product(), indices), (facts.texture, bound_texture)] {
        let image = images.get(handle).unwrap();
        assert_eq!(server.product(handle.id()), Some(product));
        assert_eq!(image.product(), Some(product));
        assert_eq!(image.extent(), ImageExtent::new(1, 1));
        assert_eq!(image.pixels().len(), 4);
        let state = states.state(handle.id()).unwrap();
        assert_eq!(state.load_state(), &LoadState::Loaded);
        assert_eq!(state.usable_version(), Some(state.version()));
        let source = image.source().unwrap();
        assert_eq!(state.source_hash(), Some(source.source_hash()));
        assert_eq!(
            state.import_recipe_digest(),
            Some(source.import_recipe().digest()),
        );
        image_observations.insert(
            product,
            ResidentImage {
                handle,
                hash: image.content_hash(),
                pixels: image.pixels().to_vec(),
                state: state.clone(),
                revision: states.state_revision(handle.id()),
                pixel_allocation: image.pixels().as_ptr() as usize,
            },
        );
    }
    assert_eq!(images.len(), image_observations.len());
    let material_state = states.state(handle.id()).unwrap();
    assert_eq!(material_state.load_state(), &LoadState::Loaded);
    assert_eq!(
        material_state.usable_version(),
        Some(material_state.version()),
    );
    Resident {
        entity,
        instance,
        stamp,
        material: facts,
        material_handle: handle,
        material_state: material_state.clone(),
        material_revision: states.state_revision(handle.id()),
        images: image_observations,
        bound_texture,
        uniform: binding.uniform_bytes().to_vec(),
        gameplay,
    }
}
