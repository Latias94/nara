use std::{path::Path, time::Duration};

use nara::{
    ProductRecipeError, SchemaContribution,
    advanced_prelude::{CurrentScene, add_startup_scene_activation_systems},
    app::{
        CoreStage, PluginCategory, PluginDeclaration, PluginError, PluginId, PluginSchemaProviderId,
    },
    asset::{
        AssetProductRef, AssetRef, AssetSourceRoot, Assets, ImportedProductId,
        ProjectAssetDatabase, StableAssetId,
    },
    core::ItemLimit,
    ecs::{Resource, World},
    identity::{EntityLookup, SceneEntityId},
    image::{ImageAsset, ImageDomain, ImageExtent},
    prelude::{App, Component, PersistentComponent, Plugin},
    project_host::{
        ProjectDomainAssets, ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
        advanced::commit_prepared_project_scene,
    },
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
    },
    scene::{SourceDocumentKey, SourceDocumentKind, advanced::SceneProductTransactionLimits},
    sprite::Sprite,
};

pub const SECOND_SCENE: &str = "c9a393f9-cb86-4bfb-9bd3-6ab63f2099a4";
pub const THIRD_SCENE: &str = "6717a11b-91e7-416e-a0df-83738933f4f0";
pub const IMAGE_IDS: [&str; 3] = [
    "818822b5-b34f-4ea0-b837-39f73827182b",
    "b771dc9c-fb09-4f39-b1a4-6b997e8df008",
    "ee0e8d7c-b034-4bde-9077-fc8d4b478746",
];

#[derive(Component, PersistentComponent)]
#[nara(
    id = "independent.cook.AuthoredScore",
    version = 1,
    alias = "Authored score",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct AuthoredScore {
    #[nara(id = "value", alias = "Value")]
    pub value: i64,
}

nara::persistent_components! { mod components = [AuthoredScore]; }

const PROVIDER_ID: PluginSchemaProviderId = PluginSchemaProviderId::new("independent.cook.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("independent.cook"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("independent.cook.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Default)]
pub struct CookGamePlugin;

impl Plugin for CookGamePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PluginId::new("independent.cook"), PluginCategory::Runtime)
                .requires_plugins(&[
                    nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID,
                    nara::advanced_prelude::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
                ])
                .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Progress>()?;
        add_startup_scene_activation_systems(app, observe)?;
        app.add_systems(CoreStage::Update, travel)?;
        Ok(())
    }
}

pub fn contribution() -> Result<SchemaContribution<CookGamePlugin>, ProductRecipeError> {
    SchemaContribution::for_default([PROVIDER])
}

#[derive(Default, Resource)]
struct Progress {
    visited: usize,
    score: i64,
    request: Option<ProjectSceneRequestId>,
}

pub fn product(id: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(id).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn observe(world: &mut World) {
    assert!(!Path::new(env!("CARGO_MANIFEST_DIR")).exists());
    let cwd = std::env::current_dir().unwrap();
    let executable = std::env::current_exe().unwrap();
    assert_ne!(cwd, executable.parent().unwrap());
    assert!(std::fs::read_dir(cwd).unwrap().next().is_none());
    for name in [
        "PATH",
        "CARGO",
        "RUSTC",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "HOME",
        "RUSTUP_TOOLCHAIN",
        "CARGO_TARGET_DIR",
        "NARA_CARGO_GENERATED_RUST_SOURCE",
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
    ] {
        assert!(
            std::env::var_os(name).is_none(),
            "ambient tool/build path: {name}"
        );
    }
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert!(!world.contains_resource::<AssetSourceRoot>());
    let index = world.resource::<Progress>().visited;
    assert!(index < 3, "each committed Scene is observed exactly once");
    let current = CurrentScene::get(world).unwrap();
    current
        .source_view()
        .with_runtime_scene(|candidate| {
            if index != 0 {
                let id = [SECOND_SCENE, THIRD_SCENE][index - 1];
                assert_eq!(
                    candidate.source_key(),
                    &SourceDocumentKey::stable_asset(
                        product(id).source(),
                        SourceDocumentKind::Scene
                    ),
                );
            }
        })
        .expect("gameplay must execute a processed Scene");
    let EntityLookup::Resolved(entity) = current
        .receipt()
        .resolve(world, &SceneEntityId::new("cook-probe").unwrap())
    else {
        panic!("the committed Scene must instantiate its authored entity");
    };
    let authored = world.get::<AuthoredScore>(entity).unwrap().value;
    assert_eq!(authored, [17, 43, 97][index]);
    let handle = world.get::<Sprite>(entity).unwrap().material.image.unwrap();
    let image = world.resource::<Assets<ImageAsset>>().get(handle).unwrap();
    let expected_product = product(IMAGE_IDS[index]);
    assert_eq!(image.product(), Some(expected_product));
    assert_eq!(image.extent(), ImageExtent::new(index as u32 + 1, 1));
    let rgba: [u8; 4] = [[17, 39, 91, 255], [47, 151, 23, 255], [201, 73, 119, 255]][index];
    assert_eq!(image.pixels(), rgba.repeat(index + 1).as_slice());
    let domains = world.resource::<ProjectDomainAssets>();
    let images = domains.collection::<ImageDomain>().unwrap();
    assert_eq!(images.source_records().len(), 0);
    let acquired = images.get(&AssetRef::Product(expected_product)).unwrap();
    assert_eq!(image.pixels().as_ptr(), acquired.pixels().as_ptr());
    assert!(acquired.source().is_none());
    let computed = authored * 7 + 3;
    println!(
        "PROJECT_COOK_GAMEPLAY scene={} authored={authored} computed={computed} image={expected_product} extent={}x{} pixels={:?}",
        ["startup", "second", "third"][index],
        image.extent().width,
        image.extent().height,
        acquired.pixels(),
    );
    let mut progress = world.resource_mut::<Progress>();
    progress.visited += 1;
    progress.score += computed;
    if progress.visited == 3 {
        println!(
            "PROJECT_COOK_COMPLETE scenes=3 travels=2 score={}",
            progress.score
        );
    }
}

fn travel(world: &mut World) {
    let (visited, request) = {
        let progress = world.resource::<Progress>();
        (progress.visited, progress.request)
    };
    if visited == 3 {
        return;
    }
    let Some(request) = request else {
        let destination = product([SECOND_SCENE, THIRD_SCENE][visited - 1]);
        let request = ProjectScenes::scope(world, |scenes| scenes.request(destination)).unwrap();
        world.resource_mut::<Progress>().request = Some(request);
        return;
    };
    let state = ProjectScenes::scope(world, |scenes| scenes.state(request)).unwrap();
    if state == ProjectSceneRequestState::Preparing {
        std::thread::sleep(Duration::from_millis(1));
        return;
    }
    assert_eq!(state, ProjectSceneRequestState::Ready);
    let expected = product([SECOND_SCENE, THIRD_SCENE][visited - 1]);
    ProjectScenes::scope(world, |scenes| {
        assert_eq!(
            scenes.prepared(request).unwrap().selected_scene_product(),
            Some(expected)
        );
    });
    let old = CurrentScene::get(world).unwrap().receipt().clone();
    let report = commit_prepared_project_scene(
        world,
        request,
        SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
        &[],
        |_| {},
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let committed = report.instance.unwrap();
    assert_ne!(committed.instance_id(), old.instance_id());
    assert_eq!(CurrentScene::get(world).unwrap().receipt(), &committed);
    ProjectScenes::scope(world, |scenes| {
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Committed)
        );
        assert!(scenes.acknowledge(request));
    });
    world.resource_mut::<Progress>().request = None;
    observe(world);
}
