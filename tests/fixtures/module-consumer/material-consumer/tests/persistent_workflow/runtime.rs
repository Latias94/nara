use std::{
    num::NonZeroU32,
    time::{Duration, Instant},
};

use nara::{
    advanced_prelude::CurrentScene,
    app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset::{AssetRef, AssetServer, AssetStates, AssetVersion, Assets},
    ecs::{Entity, Resource, World},
    identity::{EntityLookup, SpawnedSceneInstance},
    image::{ImageAsset, ImageContentHash, ImageExtent},
    material::{
        Material2d, Material2dAsset, Material2dBinding, Material2dDomain, MaterialParameterValues,
    },
    project::ProjectPath,
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, ProjectDomainAssets,
        ProjectSceneContentStamp, ProjectSceneRequestError, ProjectSceneRequestId,
        ProjectSceneRequestState, ProjectScenes,
    },
    sprite::Sprite,
};

use super::{
    REJECTED_SCENES,
    project::{
        MATERIAL_ID, Project, SHADER, SHADER_PATH, actor, indices_product, material_product,
        palette_product, parameters, recipe,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Observation {
    entity: Entity,
    instance: SpawnedSceneInstance,
    source: ProjectSceneContentStamp,
    pub(super) material: Material2d,
    pub(super) binding: Material2dBinding,
    pub(super) defaults: MaterialParameterValues,
    versions: [AssetVersion; 3],
    image_hashes: [ImageContentHash; 2],
}

fn observe(world: &mut World) -> Observation {
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
    assert_eq!(
        current.receipt().resolve(world, &actor()),
        EntityLookup::Resolved(entity),
    );
    let instance = current.receipt().clone();
    let source = current.source_content_stamp().unwrap();
    let server = world.resource::<AssetServer>();
    let materials = world.resource::<Assets<Material2dAsset>>();
    let images = world.resource::<Assets<ImageAsset>>();
    let states = world.resource::<AssetStates>();
    assert_eq!(materials.len(), 1);
    assert_eq!(images.len(), 2);
    assert_eq!(
        server.product(material.handle().id()),
        Some(material_product()),
    );
    assert_eq!(
        server.get_product_handle::<Material2dAsset>(material_product()),
        Some(material.handle()),
    );
    assert_eq!(material.declaration_id(), MATERIAL_ID);
    assert_eq!(material.declaration_version(), 1);
    let asset = materials.get(material.handle()).unwrap();
    assert_eq!(asset.product(), material_product());
    assert_eq!(asset.default_parameters(), &parameters(0.0));
    let domain = world
        .resource::<ProjectDomainAssets>()
        .collection::<Material2dDomain>()
        .unwrap();
    assert_eq!(domain.iter().len(), 1);
    assert_eq!(
        domain
            .get(&AssetRef::Product(material_product()))
            .unwrap()
            .generation(),
        asset.generation(),
    );
    assert_eq!(asset.generation().inputs().len(), 1);
    assert_eq!(asset.generation().inputs()[0].path().as_str(), SHADER_PATH);
    assert_eq!(asset.generation().inputs()[0].source(), SHADER);
    assert_eq!(
        asset.textures()["palette"],
        AssetRef::Product(palette_product()),
    );
    let mut resolved_defaults = Vec::new();
    let binding = material
        .bind(asset, |reference| {
            resolved_defaults.push(reference.clone());
            let AssetRef::Product(product) = reference else {
                panic!(
                    "resident Material defaults must retain complete resolved Product identities"
                );
            };
            server.get_product_handle::<ImageAsset>(*product)
        })
        .unwrap();
    assert_eq!(binding.generation(), asset.generation());
    assert_eq!(binding.parameters(), material.parameters());
    assert!(material.texture_overrides().is_empty());
    assert_eq!(resolved_defaults, [AssetRef::Product(palette_product())]);
    let palette = binding.textures()["palette"];
    let indices = sprite.material.image.unwrap();
    assert_ne!(palette, indices);
    assert_eq!(
        server.get_product_handle::<ImageAsset>(palette_product()),
        Some(palette),
    );
    assert_eq!(
        server.get_product_handle::<ImageAsset>(indices_product()),
        Some(indices),
    );
    let palette_image = images.get(palette).unwrap();
    let indices_image = images.get(indices).unwrap();
    assert_eq!(palette_image.product(), Some(palette_product()));
    assert_eq!(indices_image.product(), Some(indices_product()));
    for image in [palette_image, indices_image] {
        assert_eq!(image.extent(), ImageExtent::new(1, 1));
        assert_eq!(image.pixels().len(), 4);
    }
    Observation {
        entity,
        instance,
        source,
        versions: [material.handle().id(), indices.id(), palette.id()]
            .map(|id| states.state(id).unwrap().usable_version().unwrap()),
        image_hashes: [indices_image.content_hash(), palette_image.content_hash()],
        defaults: asset.default_parameters().clone(),
        material,
        binding,
    }
}

#[derive(Clone, Debug, Default, Resource)]
struct PaletteOutcome(Option<Observation>);

#[derive(Default)]
struct InspectPalette;

impl Plugin for InspectPalette {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.palette.inspect"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<PaletteOutcome>()?.add_systems(
            CoreStage::Update,
            |world: &mut World| {
                let observation = observe(world);
                world.resource_mut::<PaletteOutcome>().0 = Some(observation);
            },
        )?;
        Ok(())
    }
}

pub(super) fn play(project: &Project) -> Observation {
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(1).unwrap(), Duration::ZERO)
        .with_recipe(recipe().add_plugin::<InspectPalette>().unwrap());
    run::<PaletteOutcome>(project, intent).0.unwrap()
}

#[derive(Clone, Debug, Default, Resource)]
struct RejectionOutcome {
    incumbent: Option<Observation>,
    request: Option<ProjectSceneRequestId>,
    rejected: usize,
    finished: bool,
}

#[derive(Default)]
struct RejectPaletteCandidates;

impl Plugin for RejectPaletteCandidates {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.palette.reject-candidates"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<RejectionOutcome>()?
            .add_systems(CoreStage::Update, reject_candidates)?;
        Ok(())
    }
}

fn reject_candidates(world: &mut World) {
    let observation = observe(world);
    {
        let mut outcome = world.resource_mut::<RejectionOutcome>();
        if let Some(incumbent) = &outcome.incumbent {
            assert_eq!(incumbent, &observation);
        } else {
            outcome.incumbent = Some(observation);
        }
        if outcome.finished {
            return;
        }
    }
    let request = world.resource::<RejectionOutcome>().request;
    if let Some(request) = request {
        let state = ProjectScenes::scope(world, |scenes| scenes.state(request)).unwrap();
        if state == ProjectSceneRequestState::Preparing {
            std::thread::sleep(Duration::from_millis(1));
            return;
        }
        assert!(
            matches!(
                state,
                ProjectSceneRequestState::Rejected(ProjectSceneRequestError::Content(_)),
            ),
            "candidate must reject before replacing the incumbent: {state:?}",
        );
        ProjectScenes::scope(world, |scenes| {
            assert!(scenes.diagnostics(request).unwrap().has_errors());
            assert!(scenes.prepared(request).is_none());
            assert!(scenes.acknowledge(request));
        });
        let mut outcome = world.resource_mut::<RejectionOutcome>();
        outcome.request = None;
        outcome.rejected += 1;
        outcome.finished = outcome.rejected == REJECTED_SCENES.len();
    } else {
        let next = world.resource::<RejectionOutcome>().rejected;
        let request = ProjectScenes::scope(world, |scenes| {
            scenes.request(ProjectPath::new(REJECTED_SCENES[next]).unwrap())
        })
        .unwrap();
        world.resource_mut::<RejectionOutcome>().request = Some(request);
    }
}

pub(super) fn reject_replacements(project: &Project) {
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(2000).unwrap(), Duration::ZERO)
        .with_recipe(recipe().add_plugin::<RejectPaletteCandidates>().unwrap())
        .stop_when(|outcome: &RejectionOutcome| outcome.finished);
    let outcome = run::<RejectionOutcome>(project, intent);
    assert!(outcome.finished);
    assert_eq!(outcome.rejected, REJECTED_SCENES.len());
    assert!(outcome.incumbent.is_some());
}

fn run<O: Resource + Clone>(project: &Project, intent: HeadlessRunIntent<O>) -> O {
    let mut run = HeadlessRun::<O>::new(project.capability(), intent, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(outcome) => return outcome.clone(),
            HeadlessRunOutcome::Failed => {
                panic!("source HeadlessRun failed: {:?}", report.diagnostics());
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(
                    Instant::now() < deadline,
                    "source HeadlessRun cleanup did not settle",
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
