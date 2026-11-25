use std::fs;

use nara::{
    ProductRecipe, SchemaContribution,
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::{
        AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource, AssetMeta,
        AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetSourceKind,
        ImportedProductId, SourceAssetImporter, StableAssetId,
    },
    core::ByteLimit,
    fs::{ContentDigest, DirectoryCapability, RelativePath},
    input::{
        ActionBinding, ActionContext, ActionId, ActionMapBuilder, ActionMapFileCandidate,
        ActionMapLimits, KeyCode, MouseButton,
    },
    project_host::{
        ProjectContentLoader, ProjectContentMemberKind, ProjectSettingsCandidate, RuntimePlan,
        ingest_project_manifest, resolve_product_recipe,
    },
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        PersistentComponentProvider,
    },
    scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord},
};

use super::project_content_fixture::{TestProject, scene_id};

#[derive(nara::ecs::Component, nara::PersistentComponent)]
#[nara(
    id = "test.delivery.SourceText",
    version = 1,
    alias = "Source text",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct SourceText {
    #[nara(id = "source", alias = "Source", capabilities(scene, asset_ref))]
    pub source: AssetRef,
}

#[derive(nara::ecs::Component, nara::PersistentComponent)]
#[nara(
    id = "test.delivery.FutureTarget",
    version = 1,
    alias = "Future target",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct FutureTarget {
    #[nara(id = "target", alias = "Target", capabilities(scene, asset_ref), asset_demand = deferred)]
    pub target: AssetRef,
}

nara::persistent_components! { pub mod source_components = [SourceText, FutureTarget]; }

#[derive(Default)]
struct SourcePlugin;

const SOURCE_PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("test.delivery.source-text"),
    PluginSchemaProviderId::new("test.delivery.source-text.schema"),
    ComponentSchemaProviderBindingId::new("test.delivery.source-text.native", 1),
    source_components::initial_catalog,
    source_components::register,
);

impl Plugin for SourcePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("test.delivery.source-text"),
            PluginCategory::Runtime,
        )
        .provides_schema(&[SOURCE_PROVIDER.id()]);
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

struct SourceTextDomain;

impl AssetDomain for SourceTextDomain {
    type Product = Vec<u8>;
}

impl SourceAssetImporter for SourceTextDomain {
    type Settings = ();
    const SOURCE_KIND: &'static str = "test.delivery.text";

    fn limits() -> AssetImportLimits {
        let bytes = ByteLimit::new(1024).unwrap();
        AssetImportLimits::new(bytes, bytes, bytes)
    }

    fn prepare<E>(
        source: &mut AssetImportSource<'_, E>,
    ) -> Result<Self::Product, AssetImportError<E>> {
        source.metadata().admit::<Self::Settings>().unwrap();
        Ok(source
            .read_dependency(
                &AssetPath::new("suffix.txt").unwrap(),
                ByteLimit::new(1024).unwrap(),
            )?
            .to_vec())
    }
}

#[test]
fn strict_check_follows_deferred_scenes_and_reads_the_final_importers_raw_input() {
    let project = TestProject::with_prefab_startup();
    let scene_link = |target| {
        component_scene(
            "test.delivery.FutureTarget",
            FutureTarget {
                target: AssetRef::Product(target),
            }
            .encode_persistent_component()
            .unwrap(),
        )
    };
    write_scene(
        &project,
        "startup.scene.json",
        product(1),
        &scene_link(product(2)),
    );
    write_scene(
        &project,
        "b.scene.json",
        product(2),
        &scene_link(product(3)),
    );
    let text = source_scene(AssetRef::path("text.txt").unwrap());
    write_scene(&project, "c.scene.json", product(3), &text);
    fs::write(project.path().join("assets/text.txt"), b"future source").unwrap();
    fs::write(
        project.path().join("assets/suffix.txt"),
        b"original dependency",
    )
    .unwrap();
    let meta = AssetMeta::new(
        AssetMetaIdentity::new(
            product(4).source(),
            AssetPath::new("text.txt").unwrap(),
            AssetSourceKind::Other(SourceTextDomain::SOURCE_KIND.into()),
        ),
        (),
    );
    fs::write(
        project.path().join("assets/text.txt.meta"),
        meta.to_json_string().unwrap(),
    )
    .unwrap();
    let (recipe, candidate, plan, root) = configure(&project);
    let loader = ProjectContentLoader::new(root).unwrap();
    let startup = loader.load(&candidate, &plan).unwrap();
    assert!(startup.scene_dependencies().is_empty());
    assert!(
        startup
            .domain_assets()
            .collection::<SourceTextDomain>()
            .is_none()
    );

    let check = || {
        check_project(ProjectCheckRequest::new(
            project.root_capability(),
            recipe.clone(),
        ))
    };
    let delivery = check().unwrap();
    assert_eq!(delivery.scene_roots().len(), 1);
    assert_eq!(delivery.scene_roots()[0].as_str(), "startup.scene.json");
    for path in [
        "scenes/b.scene.json",
        "scenes/c.scene.json",
        "assets/text.txt",
        "assets/suffix.txt",
    ] {
        assert!(
            !delivery
                .startup_content()
                .members()
                .iter()
                .any(|member| member.path().as_str() == path)
        );
        assert!(
            delivery
                .members()
                .iter()
                .any(|member| member.path().as_str() == path),
            "{path}"
        );
    }
    assert_eq!(
        delivery
            .members()
            .iter()
            .filter(|member| member.kind() == ProjectContentMemberKind::Scene)
            .count(),
        3
    );

    let suffix = project.path().join("assets/suffix.txt");
    fs::write(&suffix, b"changed dependency").unwrap();
    let changed = check().unwrap();
    assert_ne!(changed.delivery_digest(), delivery.delivery_digest());
    assert_eq!(
        changed.startup_content().content_digest(),
        delivery.startup_content().content_digest()
    );
    fs::remove_file(suffix).unwrap();
    check().unwrap_err();
}

#[test]
fn cyclic_authored_scenes_keep_the_selected_input_and_exclude_unreachable_scenes() {
    let project = TestProject::with_prefab_startup();
    let input_path = "input/player.actions.json";
    let input_product = product(4);
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    let menu = ActionContext::new("menu").unwrap();
    builder.declare_context(menu.clone(), false).unwrap();
    for binding in [
        ActionBinding::key(ActionId::new("jump").unwrap(), KeyCode::Space),
        ActionBinding::mouse(ActionId::new("fire").unwrap(), MouseButton::Left),
        ActionBinding::key(ActionId::new("confirm").unwrap(), KeyCode::Enter).with_context(menu),
    ] {
        builder.bind(binding).unwrap();
    }
    let input_file =
        ActionMapFileCandidate::new(input_product, builder.compile().unwrap()).unwrap();
    let input_bytes = input_file.to_json_string().unwrap().into_bytes();
    fs::create_dir(project.path().join("input")).unwrap();
    fs::write(project.path().join(input_path), &input_bytes).unwrap();
    let manifest = project.path().join("nara.toml");
    let source = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        format!("{source}\n[input]\naction_map = '{input_path}'\n"),
    )
    .unwrap();
    let scene_a = source_scene(AssetRef::Product(product(1)));
    let scene_b = source_scene(AssetRef::Product(product(2)));
    write_scene(&project, "startup.scene.json", product(2), &scene_a);
    write_scene(&project, "b.scene.json", product(1), &scene_b);
    write_scene(
        &project,
        "c.scene.json",
        product(3),
        &SceneDocument::default(),
    );
    fs::write(
        project.path().join("scenes/c.scene.json"),
        b"{ unreachable source",
    )
    .unwrap();

    let (recipe, candidate, plan, root) = configure(&project);
    let delivery =
        check_project(ProjectCheckRequest::new(project.root_capability(), recipe)).unwrap();
    assert_eq!(delivery.scene_roots().len(), 1);
    assert_eq!(delivery.scene_roots()[0].as_str(), "startup.scene.json");
    assert_eq!(delivery.startup_content().members(), delivery.members());
    let paths: Vec<_> = delivery
        .members()
        .iter()
        .map(|member| member.path().as_str())
        .collect();
    assert_eq!(
        paths,
        [
            input_path,
            "nara.toml",
            "scenes/b.scene.json",
            "scenes/b.scene.json.meta",
            "scenes/startup.scene.json",
            "scenes/startup.scene.json.meta",
            "schema/authoring.bundle.json"
        ]
    );

    let loader = ProjectContentLoader::new(root).unwrap();
    let startup = loader.load(&candidate, &plan).unwrap();
    assert_eq!(startup.scene(), &scene_a);
    assert_eq!(startup.selected_scene_product(), Some(product(2)));
    assert_eq!(startup.scene_dependencies().len(), 1);
    assert_eq!(
        startup.scene_dependencies()[0].path().as_str(),
        "scenes/b.scene.json"
    );
    assert_eq!(startup.scene_dependencies()[0].document(), &scene_b);
    assert!(
        startup
            .asset_resolution()
            .resolve_ref(&AssetRef::Product(product(3)))
            .is_err()
    );
    let input = startup.input_action_map().unwrap();
    assert_eq!(input.product(), input_product);
    assert_eq!(input.path().as_str(), input_path);
    assert_eq!(input.source_digest(), ContentDigest::of_bytes(&input_bytes));
    assert_eq!(
        input.definition().contexts(),
        input_file.definition().contexts()
    );
    assert_eq!(
        input.definition().bindings(),
        input_file.definition().bindings()
    );
}

fn configure(
    project: &TestProject,
) -> (
    ProductRecipe,
    ProjectSettingsCandidate,
    RuntimePlan,
    DirectoryCapability,
) {
    let recipe = ProductRecipe::new()
        .add_contribution(
            SchemaContribution::<SourcePlugin>::for_default([SOURCE_PROVIDER]).unwrap(),
        )
        .unwrap()
        .add_asset_domain::<SourceTextDomain>()
        .unwrap();
    let root = project.root_capability();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
    let bundle = plan
        .schema_validation()
        .authoring_schema_bundle()
        .to_json_string()
        .unwrap();
    fs::write(project.path().join("schema/authoring.bundle.json"), bundle).unwrap();
    (recipe, candidate, plan, root)
}

fn product(suffix: u64) -> AssetProductRef {
    let id = StableAssetId::parse_str(format!("6c420000-0000-4000-8000-{suffix:012x}")).unwrap();
    AssetProductRef::new(id, ImportedProductId::PRIMARY).unwrap()
}

fn source_scene(source: AssetRef) -> SceneDocument {
    component_scene(
        "test.delivery.SourceText",
        SourceText { source }.encode_persistent_component().unwrap(),
    )
}

fn component_scene(component: &str, value: ComponentValue) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("door")).with_component(
        ComponentTypeId::new(component),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
    )])
}

fn write_scene(project: &TestProject, name: &str, product: AssetProductRef, scene: &SceneDocument) {
    let scenes = project.path().join("scenes");
    fs::write(scenes.join(name), scene.to_json_string().unwrap()).unwrap();
    let meta = AssetMeta::new(
        AssetMetaIdentity::new(
            product.source(),
            AssetPath::new(name).unwrap(),
            AssetSourceKind::Scene,
        ),
        (),
    );
    fs::write(
        scenes.join(format!("{name}.meta")),
        meta.to_json_string().unwrap(),
    )
    .unwrap();
}
