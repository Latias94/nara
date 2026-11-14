use std::{
    collections::BTreeMap,
    num::NonZeroU32,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    ProductConfiguration,
    advanced_prelude::{CurrentScene, SceneSourceView},
    app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset::{
        AssetDependencyGraph, AssetRef, AssetReloadRequests, AssetServer, AssetSourceChanges,
        AssetSourceRoot, AssetStates, AssetVersion, Assets, ProjectAssetDatabase,
    },
    ecs::{Entity, Resource, World},
    identity::{EntityLookup, SpawnedSceneInstance},
    image::{ImageAsset, ImageDomain, ImageImporter},
    material::{Material2d, Material2dAsset, Material2dBinding, Material2dDomain},
    project_host::{HeadlessRun, HeadlessRunOptions, HeadlessRunOutcome, ProjectDomainAssets},
    runtime_content::RuntimeContentBase,
    scene::{SourceDocumentKey, SourceDocumentKind},
    sprite::Sprite,
};

use super::project::{
    MATERIAL_ID, actor, assert_image, indices_product, material_product, palette_product,
    parameters, scene_product,
};

#[derive(Clone, Debug, Resource)]
pub(super) struct Probe {
    pub(super) removed_root: PathBuf,
    pub(super) builds: Arc<AtomicU64>,
    pub(super) frames: Arc<AtomicU64>,
}

impl Probe {
    pub(super) fn new(removed_root: PathBuf) -> Self {
        Self {
            removed_root,
            builds: Arc::new(AtomicU64::new(0)),
            frames: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(super) fn assert_not_started(&self) {
        assert_eq!(self.builds.load(Ordering::SeqCst), 0);
        assert_eq!(self.frames.load(Ordering::SeqCst), 0);
    }
}

impl ProductConfiguration for Probe {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"consumer.processed-palette.inspect.v1\0");
        let root = self.removed_root.to_string_lossy();
        output.extend_from_slice(&(root.len() as u64).to_le_bytes());
        output.extend_from_slice(root.as_bytes());
    }
}

#[derive(Clone, Debug)]
pub(super) struct Observation {
    pub(super) asset: Material2dAsset,
    pub(super) material: Material2d,
    pub(super) binding: Material2dBinding,
    pub(super) images: [Arc<ImageAsset>; 2],
    entity: Entity,
    instance: SpawnedSceneInstance,
    source: SceneSourceView,
    versions: [AssetVersion; 3],
}

#[derive(Clone, Debug, Default, Resource)]
struct Outcome(Option<Observation>);

pub(super) struct InspectPalette(pub(super) Probe);

impl Plugin for InspectPalette {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.processed-palette.inspect"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        self.0.builds.fetch_add(1, Ordering::SeqCst);
        app.insert_resource(self.0.clone())?
            .init_resource::<Outcome>()?
            .add_systems(CoreStage::Update, inspect)?;
        Ok(())
    }
}

fn inspect(world: &mut World) {
    assert!(!world.resource::<Probe>().removed_root.exists());
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert!(!world.contains_resource::<AssetDependencyGraph>());
    assert!(!world.contains_resource::<AssetSourceRoot>());
    assert!(!world.contains_resource::<AssetSourceChanges>());
    assert!(!world.contains_resource::<AssetReloadRequests>());
    assert!(!world.contains_resource::<ImageImporter>());
    assert_eq!(world.query::<&Sprite>().iter(world).count(), 1);
    assert_eq!(world.query::<&Material2d>().iter(world).count(), 1);
    let (entity, sprite, material) = {
        let mut query = world.query::<(Entity, &Sprite, &Material2d)>();
        let mut values = query.iter(world);
        let (entity, sprite, material) = values.next().unwrap();
        let result = (entity, sprite.clone(), material.clone());
        assert!(values.next().is_none());
        result
    };
    let current = CurrentScene::get(world).unwrap();
    assert_eq!(current.source().roots(), [actor()]);
    assert_eq!(
        current.receipt().resolve(world, &actor()),
        EntityLookup::Resolved(entity),
    );
    assert!(current.source_content_stamp().is_none());
    let source = current.source_view();
    assert!(source.with_compiled_scene(|_, _| ()).is_none());
    assert_eq!(
        source.with_runtime_scene(|candidate| candidate.source_key().clone()),
        Some(SourceDocumentKey::stable_asset(
            scene_product().source(),
            SourceDocumentKind::Scene,
        )),
    );
    let instance = current.receipt().clone();
    let server = world.resource::<AssetServer>();
    let materials = world.resource::<Assets<Material2dAsset>>();
    let images = world.resource::<Assets<ImageAsset>>();
    let states = world.resource::<AssetStates>();
    assert_eq!(materials.len(), 1);
    assert_eq!(images.len(), 2);
    assert_eq!(
        server.product(material.handle().id()),
        Some(material_product())
    );
    assert_eq!(
        server.get_product_handle::<Material2dAsset>(material_product()),
        Some(material.handle()),
    );
    assert!(server.path(material.handle().id()).is_none());
    assert_eq!(material.declaration_id(), MATERIAL_ID);
    assert_eq!(material.declaration_version(), 1);
    assert_eq!(material.parameters(), &parameters(2.0));
    assert!(material.texture_overrides().is_empty());
    let asset = materials.get(material.handle()).unwrap();
    assert_eq!(asset.product(), material_product());
    assert_eq!(asset.default_parameters(), &parameters(0.0));
    assert_eq!(
        asset.textures(),
        &BTreeMap::from([("palette".to_owned(), AssetRef::Product(palette_product()))]),
    );
    assert!(asset.generation().inputs().is_empty());
    assert_eq!(asset.generation().sequence(), 0);
    let collection = world
        .resource::<ProjectDomainAssets>()
        .collection::<Material2dDomain>()
        .unwrap();
    assert_eq!(collection.iter().len(), 1);
    assert_eq!(collection.source_records().len(), 0);
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
                panic!("processed defaults require complete Products");
            };
            server.get_product_handle::<ImageAsset>(*product)
        })
        .unwrap();
    assert_eq!(resolved, [AssetRef::Product(palette_product())]);
    assert_eq!(binding.generation(), asset.generation());
    assert_eq!(binding.parameters(), &parameters(2.0));
    let expected_uniform = [2.0_f32, 1.0, 0.0, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    assert_eq!(binding.uniform_bytes(), expected_uniform.as_slice());
    assert_eq!(binding.textures().len(), 1);
    let palette = binding.textures()["palette"];
    let indices = sprite.material.image.unwrap();
    assert_ne!(indices, palette);
    assert_eq!(
        server.get_product_handle::<ImageAsset>(indices_product()),
        Some(indices),
    );
    assert_eq!(
        server.get_product_handle::<ImageAsset>(palette_product()),
        Some(palette),
    );
    assert_eq!(server.product(indices.id()), Some(indices_product()));
    assert_eq!(server.product(palette.id()), Some(palette_product()));
    assert!(server.path(indices.id()).is_none());
    assert!(server.path(palette.id()).is_none());
    let cpu_images = world
        .resource::<ProjectDomainAssets>()
        .collection::<ImageDomain>()
        .unwrap();
    assert_eq!(cpu_images.iter().len(), 2);
    assert_eq!(cpu_images.source_records().len(), 0);
    for (product, handle) in [(indices_product(), indices), (palette_product(), palette)] {
        assert_eq!(
            cpu_images
                .get(&AssetRef::Product(product))
                .unwrap()
                .pixels()
                .as_ptr(),
            images.get(handle).unwrap().pixels().as_ptr(),
        );
    }
    let images = [indices, palette]
        .map(|handle| Arc::new(images.get(handle).unwrap().share_retained().unwrap()));
    assert_image(&images[0], indices_product());
    assert_image(&images[1], palette_product());
    let versions = [material.handle().id(), indices.id(), palette.id()].map(|id| {
        let state = states.state(id).unwrap();
        assert!(state.source_hash().is_none());
        assert!(state.import_recipe_digest().is_none());
        state.usable_version().unwrap()
    });
    let observation = Observation {
        asset: asset.clone(),
        material,
        binding,
        images,
        entity,
        instance,
        source,
        versions,
    };
    if let Some(previous) = &world.resource::<Outcome>().0 {
        assert_eq!(observation.entity, previous.entity);
        assert_eq!(observation.instance, previous.instance);
        assert_eq!(observation.material, previous.material);
        assert_eq!(observation.asset.generation(), previous.asset.generation());
        assert_eq!(observation.binding, previous.binding);
        assert_eq!(observation.versions, previous.versions);
        assert_eq!(observation.images, previous.images);
    }
    world
        .resource::<Probe>()
        .frames
        .fetch_add(1, Ordering::SeqCst);
    world.resource_mut::<Outcome>().0 = Some(observation);
}

pub(super) fn play(base: RuntimeContentBase) -> Observation {
    let options = HeadlessRunOptions::frames(NonZeroU32::new(2).unwrap(), Duration::ZERO);
    let mut run = HeadlessRun::<Outcome>::from_runtime_content(base, options, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = run.execute_bounded();
        assert!(report.startup_content_closure().is_none());
        match report.outcome() {
            HeadlessRunOutcome::Completed(outcome) => {
                assert!(
                    !report.diagnostics().has_errors(),
                    "{:?}",
                    report.diagnostics()
                );
                let observation = outcome
                    .0
                    .clone()
                    .expect("processed gameplay must inspect its materialized Palette Scene");
                assert!(observation.source.with_runtime_scene(|_| ()).is_none());
                return observation;
            }
            HeadlessRunOutcome::Failed => {
                panic!("processed Palette Host failed: {:?}", report.diagnostics());
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(
                    Instant::now() < deadline,
                    "processed Palette cleanup did not settle",
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
