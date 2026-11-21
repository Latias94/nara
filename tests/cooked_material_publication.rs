#![cfg(all(feature = "project-tooling", any(windows, target_os = "linux")))]

use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe, SchemaContribution,
    app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset::{
        AssetBuildSource, AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource,
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetReferenceDemand,
        AssetRuntimeError, AssetRuntimeLimits, AssetServer, AssetSourceKind, Assets,
        ImportedProductId, RuntimeAssetCodec, SourceAssetBuild, SourceAssetImporter, StableAssetId,
    },
    asset_build::{ArtifactBuildLimits, ArtifactBuildStore, ArtifactContentDigest},
    core::ByteLimit,
    diagnostic::{Diagnostic, DiagnosticCode, SafeSummary},
    ecs::World,
    fs::{DirectoryCapability, RelativePath},
    identity::SceneEntityId,
    image::{ImageAsset, ImageDomain, ImageExtent, ImageImportSettings, ImageRuntimeCodec},
    material::{
        AlphaMode2d, Material2dAsset, Material2dDeclaration, Material2dDomain, Material2dRecord,
        Material2dRuntimeCodec, Material2dSource, MaterialParameterKind, MaterialParameterLayout,
        MaterialParameterValue, MaterialTextureDeclaration, MaterialUniformField,
    },
    project::{ProductCapability, ProductCapabilitySet, RuntimePreset},
    project_host::{
        HeadlessSession, HeadlessSessionClose, ProjectDomainAssets, ProjectSettingsCandidate,
        ingest_project_manifest, resolve_product_recipe,
    },
    project_tooling::{
        host_fs::{readable_directory, readable_file, writable_directory},
        project_cook::{
            PreparedProjectCook, ProjectCookError, ProjectCookRequest, ProjectServerCookTarget,
            prepare_project_cooked_content, prepare_project_server_content,
        },
    },
    project_workflow::cooked_content::{CookedContentError, CookedContentLimits},
    reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
        ComponentSchemaVersion, ComponentTypeId, ComponentValueShape,
        PersistentComponentSchemaOwnerId, encode_asset_reference_value,
    },
    runtime_content::{
        RuntimeCodecDefinitionId, RuntimeContentBase, RuntimeContentDependency,
        RuntimeContentDependencyRole, RuntimeContentError, RuntimeContentExpectation,
        RuntimeContentLimits, RuntimeContentTarget,
    },
    scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord},
};
use serde::{Deserialize, Serialize};

const SCENE: &str = "3d7b2c3b-6b42-4534-bf5a-58ca4d4f4401";
const RULE: &str = "3d7b2c3b-6b42-4534-bf5a-58ca4d4f4402";
const MATERIAL: &str = "3d7b2c3b-6b42-4534-bf5a-58ca4d4f4403";
const IMAGE: &str = "3d7b2c3b-6b42-4534-bf5a-58ca4d4f4404";
const SCENE_PATH: &str = "startup.scene.json";
const RULE_PATH: &str = "gameplay.rule.json";
const MATERIAL_PATH: &str = "test.material2d.json";
const SHADER_PATH: &str = "test.wgsl";
const IMAGE_PATH: &str = "palette.png";
const TEXTURE_SLOT: &str = "palette";
const MATERIAL_DECLARATION: &str = "test.cooked-material";
const MATERIAL_COMPONENT: &str = "nara.material2d";
const RULE_COMPONENT: &str = "test.material.Rule";
const RULE_OWNER: &str = "test.material.rule";
const MATERIAL_CODEC: RuntimeCodecDefinitionId =
    RuntimeCodecDefinitionId::new("test.cooked-material", 1);

#[test]
fn server_cook_keeps_required_material_cpu_only_without_a_native_owner() {
    let scratch = Scratch::new();
    let project = CookProject::new(&scratch.0, None);
    let gameplay = gameplay_recipe();
    let recipes = [
        (
            "cpu",
            gameplay
                .clone()
                .add_runtime_asset_codec::<Material2dDomain, Material2dRuntimeCodec>(MATERIAL_CODEC)
                .unwrap(),
        ),
        (
            "native",
            gameplay
                .add_runtime_material_codec::<Material2dRuntimeCodec>(MATERIAL_CODEC)
                .unwrap(),
        ),
    ];
    let installed = scratch.0.join("installed");
    fs::create_dir(&installed).unwrap();
    let packs = recipes.map(|(name, recipe)| {
        let cook = |with_carrier| {
            project.write_scene(with_carrier);
            project.cook_server(recipe.clone())
        };
        let path = installed.join(format!("{name}-stripped.pack"));
        let control = installed.join(format!("{name}-absent.pack"));
        let expected = publish_pack(cook(true), &project.path, &path);
        let control_expected = publish_pack(cook(false), &project.path, &control);
        assert!(
            fs::read(&path).unwrap() == fs::read(&control).unwrap(),
            "{name} packs differed after carrier omission"
        );
        assert_eq!(expected.catalog_digest(), control_expected.catalog_digest());
        (name, recipe, path, expected)
    });

    project.retire_source();

    for (name, recipe, path, expected) in packs {
        let base = RuntimeContentBase::open_pack(
            readable_file(&path).unwrap(),
            &expected,
            recipe,
            RuntimeContentLimits::default(),
        )
        .unwrap();
        assert_eq!(
            base.plan().settings().runtime_preset(),
            RuntimePreset::Server
        );
        assert!(
            base.plan()
                .compiled_capabilities()
                .capabilities()
                .contains(ProductCapability::Runtime2d)
        );
        assert_eq!(
            base.plan().normalized_capabilities(),
            ProductCapabilitySet::singleton(ProductCapability::RuntimeCore)
        );
        assert_server_contents(&base);

        observe_server(base, name, |_, material| {
            assert!(material.textures().is_empty());
        });
    }
}

#[test]
fn server_cook_loads_material_texture_through_generic_cpu_image_codec() {
    let scratch = Scratch::new();
    let project = CookProject::new(&scratch.0, Some(AssetRef::path(IMAGE_PATH).unwrap()));
    let image = product(IMAGE);
    let assets = project.path.join("assets");
    fs::write(
        assets.join(IMAGE_PATH),
        include_bytes!("fixtures/images/valid-rgba-1x1.png"),
    )
    .unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            image.source(),
            AssetPath::new(IMAGE_PATH).unwrap(),
            AssetSourceKind::Image,
        ),
        ImageImportSettings::default(),
    );
    fs::write(
        assets.join(format!("{IMAGE_PATH}.meta")),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    project.write_scene(false);

    let recipe = gameplay_recipe()
        .add_runtime_asset_codec::<Material2dDomain, Material2dRuntimeCodec>(MATERIAL_CODEC)
        .unwrap()
        .add_runtime_asset_codec::<ImageDomain, ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
            "test.cooked-material.image",
            1,
        ))
        .unwrap();
    let path = scratch.0.join("textured.pack");
    let expected = publish_pack(project.cook_server(recipe.clone()), &project.path, &path);
    project.retire_source();
    let base = RuntimeContentBase::open_pack(
        readable_file(&path).unwrap(),
        &expected,
        recipe,
        RuntimeContentLimits::default(),
    )
    .unwrap();
    let validation = base.validate_deep().unwrap();
    assert_eq!((validation.members(), validation.products()), (4, 4));
    assert_eq!(base.catalog().members().len(), 4);
    assert_eq!(
        base.member(product(MATERIAL)).unwrap().dependencies(),
        [RuntimeContentDependency::new(
            image,
            RuntimeContentDependencyRole::Required,
        )]
    );

    observe_server(base, "generic CPU image", |world, material| {
        let texture = material.textures().get(TEXTURE_SLOT).unwrap();
        assert_eq!(texture, &AssetRef::Product(image));
        let images = world
            .resource::<ProjectDomainAssets>()
            .collection::<ImageDomain>()
            .unwrap();
        assert_eq!(images.iter().len(), 1);
        let texture_image = images.get(texture).unwrap();
        assert_eq!(texture_image.product(), Some(image));
        assert_eq!(texture_image.extent(), ImageExtent::new(1, 1));
        assert_eq!(texture_image.pixels(), [24, 120, 220, 255]);
        assert!(!world.contains_resource::<Assets<ImageAsset>>());
    });
}

#[test]
#[allow(
    clippy::result_large_err,
    reason = "The test matches the public Cook refusal."
)]
fn local_headless_material_carrier_requires_native_codec_registration() {
    let scratch = Scratch::new();
    let project = CookProject::new(&scratch.0, None);
    project.write_scene(true);
    let cpu = project
        .recipe
        .clone()
        .add_runtime_asset_codec::<Material2dDomain, Material2dRuntimeCodec>(MATERIAL_CODEC)
        .unwrap();
    let native = project
        .recipe
        .clone()
        .add_runtime_material_codec::<Material2dRuntimeCodec>(MATERIAL_CODEC)
        .unwrap();
    let cook = |recipe| {
        prepare_project_cooked_content(project.request(recipe, "local-headless"), |context| {
            context.add_source_assets::<GameplayDomain>()
        })
    };
    let error = cook(cpu.clone())
        .err()
        .expect("an actual native carrier requires Material publication");
    let ProjectCookError::Content(CookedContentError::Runtime(RuntimeContentError::Asset(error))) =
        error
    else {
        panic!("unexpected native carrier refusal: {error:?}");
    };
    assert_eq!(
        error.diagnostic().code().as_str(),
        "runtime.content.material-binding-mismatch"
    );
    cook(native).unwrap();
    project.write_scene(false);
    cook(cpu).unwrap();
}

fn observe_server(
    base: RuntimeContentBase,
    case: &str,
    observe: impl FnOnce(&World, &Material2dAsset),
) {
    let mut session = HeadlessSession::from_runtime_content(base);
    session
        .start()
        .unwrap_or_else(|error| panic!("{case} Server startup failed: {error:?}"));
    assert!(session.frame(Duration::ZERO).unwrap().is_none());
    session
        .observe(|world| {
            let domains = world.resource::<ProjectDomainAssets>();
            let rule = domains
                .collection::<GameplayDomain>()
                .unwrap()
                .get(&AssetRef::Product(product(RULE)))
                .unwrap();
            let selected = rule.material.expect("the built rule requires a Material");
            assert_eq!(selected, product(MATERIAL));
            let materials = domains.collection::<Material2dDomain>().unwrap();
            assert_eq!(materials.iter().len(), 1);
            let material = materials.get(&AssetRef::Product(selected)).unwrap();
            assert_eq!(material.product(), selected);
            assert_eq!(
                material.generation().declaration().id(),
                MATERIAL_DECLARATION
            );
            assert!(!world.contains_resource::<Assets<Material2dAsset>>());
            assert!(
                world
                    .resource::<AssetServer>()
                    .get_product_handle::<Material2dAsset>(selected)
                    .is_none()
            );
            observe(world, material);
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match session.close() {
            HeadlessSessionClose::Complete => return,
            HeadlessSessionClose::Failed => {
                panic!("{case} Server close failed: {:?}", session.diagnostics());
            }
            HeadlessSessionClose::Pending => {
                assert!(Instant::now() < deadline, "{case} Server close timed out");
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

fn assert_server_contents(base: &RuntimeContentBase) {
    let validation = base.validate_deep().unwrap();
    assert_eq!(validation.catalog_digest(), base.catalog_digest());
    assert_eq!((validation.members(), validation.products()), (3, 3));
    assert_eq!(base.catalog().members().len(), 3);
    assert_eq!(base.catalog().scenes(), [product(SCENE)]);
    for (parent, dependency) in [(SCENE, RULE), (RULE, MATERIAL)] {
        assert_eq!(
            base.member(product(parent)).unwrap().dependencies(),
            [RuntimeContentDependency::new(
                product(dependency),
                RuntimeContentDependencyRole::Required,
            )]
        );
    }
    assert!(
        base.member(product(MATERIAL))
            .unwrap()
            .dependencies()
            .is_empty()
    );
    let scene = base.acquire_scene(product(SCENE)).unwrap();
    let [entity] = scene.candidate().document().entities() else {
        panic!("expected the gameplay entity to survive Server Cook");
    };
    assert_eq!(entity.components().len(), 1);
    assert!(
        entity
            .components()
            .contains_key(&ComponentTypeId::new(RULE_COMPONENT))
    );
}

fn publish_pack(
    cooked: PreparedProjectCook,
    source: &Path,
    destination: &Path,
) -> RuntimeContentExpectation {
    let (content, selection) = cooked.into_parts();
    selection.verify(source).unwrap();
    let staged = destination.with_extension("pending");
    {
        let mut file = File::create_new(&staged).unwrap();
        content.base().write_pack(&mut file).unwrap();
        file.sync_all().unwrap();
    }
    fs::rename(staged, destination).unwrap();
    content.expectation().clone()
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nara-cooked-material-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

struct CookProject {
    path: PathBuf,
    root: DirectoryCapability,
    store: ArtifactBuildStore,
    source: ProjectSettingsCandidate,
    server: ProjectSettingsCandidate,
    recipe: ProductRecipe,
    carrier: SceneComponentRecord,
}

impl CookProject {
    fn new(scratch: &Path, texture: Option<AssetRef>) -> Self {
        let path = scratch.join("project");
        for directory in ["assets", "scenes", "prefabs"] {
            fs::create_dir_all(path.join(directory)).unwrap();
        }
        fs::write(
            path.join("nara.toml"),
            r#"schema_version = 1
[project]
source_universe = '0d5717d1-8d25-4e34-904b-03075db2389b'
native_document_identity = 'project_location'
name = 'Cooked Material Test'
[paths]
assets = 'assets'
scenes = 'scenes'
prefabs = 'prefabs'
[authoring]
state = 'ready'
schema_bundle = 'schema.bundle.json'
[startup]
default_scene = 'startup.scene.json'
[runtime]
preset = 'local-headless'
[capabilities]
requested = ['runtime-2d']
[profiles.server.capabilities]
requested = []
"#,
        )
        .unwrap();
        let assets = path.join("assets");
        fs::write(
            assets.join(RULE_PATH),
            serde_json::to_vec(&product(MATERIAL)).unwrap(),
        )
        .unwrap();
        write_metadata(
            &assets,
            RULE_PATH,
            product(RULE),
            AssetSourceKind::Other(GameplayDomain::SOURCE_KIND.to_owned()),
        );
        write_metadata(
            &path.join("scenes"),
            SCENE_PATH,
            product(SCENE),
            AssetSourceKind::Scene,
        );
        let carrier = write_material(&assets, texture);
        let recipe = gameplay_recipe()
            .add_asset_domain::<GameplayDomain>()
            .unwrap()
            .add_asset_domain::<Material2dDomain>()
            .unwrap();
        let root = readable_directory(&path).unwrap();
        let (source, server) = {
            let manifest = root
                .open_file_exact(&RelativePath::new("nara.toml").unwrap())
                .unwrap();
            (
                ingest_project_manifest(&manifest, None).unwrap(),
                ingest_project_manifest(&manifest, Some("server")).unwrap(),
            )
        };
        let plan = resolve_product_recipe(&source, recipe.clone()).unwrap();
        fs::write(
            path.join("schema.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let cache = scratch.join("cache");
        fs::create_dir(&cache).unwrap();
        let store = ArtifactBuildStore::open(
            writable_directory(&cache).unwrap(),
            ArtifactBuildLimits::default(),
        )
        .unwrap();
        Self {
            path,
            root,
            store,
            source,
            server,
            recipe,
            carrier,
        }
    }

    fn write_scene(&self, with_carrier: bool) {
        let mut entity = SceneEntityRecord::new(SceneEntityId::new("gameplay").unwrap())
            .with_component(
                ComponentTypeId::new(RULE_COMPONENT),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    encode_asset_reference_value(&AssetRef::Product(product(RULE))),
                )
                .with_owner(PersistentComponentSchemaOwnerId::try_new(RULE_OWNER).unwrap()),
            );
        if with_carrier {
            entity = entity.with_component(
                ComponentTypeId::new(MATERIAL_COMPONENT),
                self.carrier.clone(),
            );
        }
        let scene = SceneDocument::from_ordered_roots([entity]);
        fs::write(
            self.path.join("scenes").join(SCENE_PATH),
            scene.to_json_string().unwrap(),
        )
        .unwrap();
    }

    #[allow(
        clippy::result_large_err,
        reason = "The Cook callback returns the public ProjectCookError."
    )]
    fn cook_server(&self, recipe: ProductRecipe) -> PreparedProjectCook {
        prepare_project_server_content(
            self.request(self.recipe.clone(), "server"),
            ProjectServerCookTarget {
                candidate: &self.server,
                recipe,
                omit_components: &[ComponentTypeId::new(MATERIAL_COMPONENT)],
            },
            |context| context.add_source_assets::<GameplayDomain>(),
        )
        .unwrap()
    }

    fn retire_source(self) {
        let source = self.path.clone();
        drop(self);
        fs::rename(&source, source.with_file_name("retired-project")).unwrap();
        assert!(!source.exists());
    }

    fn request(&self, recipe: ProductRecipe, profile: &str) -> ProjectCookRequest<'_> {
        ProjectCookRequest {
            root: &self.root,
            candidate: &self.source,
            recipe,
            target: RuntimeContentTarget::new("test-headless", profile).unwrap(),
            store: &self.store,
            implementation: ArtifactContentDigest::of_bytes(b"cooked-material-publication"),
            limits: CookedContentLimits::default(),
        }
    }
}

fn write_metadata(root: &Path, path: &str, product: AssetProductRef, kind: AssetSourceKind) {
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(product.source(), AssetPath::new(path).unwrap(), kind),
        (),
    );
    fs::write(
        root.join(format!("{path}.meta")),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
}

fn write_material(assets: &Path, texture: Option<AssetRef>) -> SceneComponentRecord {
    let (slots, textures, sampled_color) = match texture {
        Some(reference) => (
            vec![MaterialTextureDeclaration::new(TEXTURE_SLOT, "material_texture", 1).unwrap()],
            BTreeMap::from([(TEXTURE_SLOT.to_owned(), reference)]),
            "sampled * textureLoad(material_texture, vec2<i32>(0, 0), 0)",
        ),
        None => (Vec::new(), BTreeMap::new(), "sampled"),
    };
    let declaration = Material2dDeclaration::new(
        MATERIAL_DECLARATION,
        1,
        AlphaMode2d::Blend,
        MaterialParameterLayout::new(
            vec![
                MaterialUniformField::new("color.tint", "tint", MaterialParameterKind::Vec4, 0)
                    .unwrap(),
            ],
            16,
        )
        .unwrap(),
        slots,
    )
    .unwrap();
    let parameters = BTreeMap::from([(
        "color.tint".to_owned(),
        MaterialParameterValue::Vec4([0.25, 0.5, 0.75, 1.0]),
    )]);
    let source = Material2dSource::new(
        declaration.clone(),
        vec![AssetPath::new(SHADER_PATH).unwrap()],
        parameters.clone(),
        textures,
    )
    .unwrap();
    fs::write(assets.join(MATERIAL_PATH), source.encode().unwrap()).unwrap();
    fs::write(
        assets.join(SHADER_PATH),
        format!(
            r#"fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {{
    return {sampled_color} * tint * material_parameters.tint;
}}
"#,
        ),
    )
    .unwrap();
    write_metadata(
        assets,
        MATERIAL_PATH,
        product(MATERIAL),
        AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
    );
    let record = Material2dRecord::new(
        AssetRef::Product(product(MATERIAL)),
        &declaration,
        parameters,
        BTreeMap::new(),
    )
    .unwrap();
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        record.to_component_value().unwrap(),
    )
}

fn gameplay_recipe() -> ProductRecipe {
    let schema = ComponentSchema::new(
        ComponentTypeId::new(RULE_COMPONENT),
        "Gameplay rule",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("rule"),
        "Rule",
        ComponentFieldPath::empty(),
        ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::AssetRef)]);
    let owner = ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new(RULE_OWNER).unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    ProductRecipe::new()
        .add_contribution(
            SchemaContribution::<GameplayDefinitions>::data_for_default([owner]).unwrap(),
        )
        .unwrap()
        .add_runtime_asset_codec::<GameplayDomain, GameplayDomain>(RuntimeCodecDefinitionId::new(
            RULE_OWNER, 1,
        ))
        .unwrap()
}

#[derive(Default)]
struct GameplayDefinitions;

impl Plugin for GameplayDefinitions {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PluginId::new(RULE_OWNER), PluginCategory::Runtime);
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
struct GameplayRule {
    product: AssetProductRef,
    material: Option<AssetProductRef>,
}

struct GameplayDomain;

impl AssetDomain for GameplayDomain {
    type Product = GameplayRule;
}

impl SourceAssetImporter for GameplayDomain {
    type Settings = ();

    const SOURCE_KIND: &'static str = RULE_OWNER;

    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(bytes(256), bytes(4096), bytes(4096))
    }

    fn prepare<E>(
        source: &mut AssetImportSource<'_, E>,
    ) -> Result<GameplayRule, AssetImportError<E>> {
        Ok(GameplayRule {
            product: primary(source.metadata().identity().stable_id()),
            material: None,
        })
    }
}

impl SourceAssetBuild for GameplayDomain {
    type Codec = Self;

    const BUILD_ID: &'static str = "test.material.rule-build";
    const BUILD_REVISION: u32 = 1;

    fn normalize_settings((): ()) -> Result<(), Diagnostic> {
        Ok(())
    }

    fn build<E>(source: &mut AssetBuildSource<'_, E>) -> Result<GameplayRule, AssetImportError<E>> {
        let mut rule = source.prepare_import::<Self>()?;
        rule.material = Some(
            serde_json::from_slice(source.bytes())
                .map_err(|_| AssetImportError::domain(invalid_rule()))?,
        );
        Ok(rule)
    }

    fn visit_products<E>(
        rule: &GameplayRule,
        mut visit: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        visit(rule.product)
    }
}

impl RuntimeAssetCodec<GameplayRule> for GameplayDomain {
    const RUNTIME_KIND: &'static str = "test_material_rule";
    const RUNTIME_VERSION: u32 = 1;

    fn runtime_limits() -> AssetRuntimeLimits {
        AssetRuntimeLimits::new(bytes(512), bytes(4096), bytes(4096))
    }

    fn encode_runtime(rule: &GameplayRule) -> Result<Box<[u8]>, AssetRuntimeError> {
        serde_json::to_vec(rule)
            .map(Vec::into_boxed_slice)
            .map_err(|_| AssetRuntimeError::new(invalid_rule()))
    }

    fn decode_runtime(bytes: &[u8]) -> Result<GameplayRule, AssetRuntimeError> {
        serde_json::from_slice(bytes).map_err(|_| AssetRuntimeError::new(invalid_rule()))
    }

    fn runtime_retained_bytes(_: &GameplayRule) -> Result<usize, AssetRuntimeError> {
        Ok(size_of::<GameplayRule>())
    }

    fn visit_runtime_dependencies<E>(
        rule: &GameplayRule,
        mut visit: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        if let Some(material) = rule.material {
            visit(material, AssetReferenceDemand::Immediate)?;
        }
        Ok(())
    }

    fn contains_runtime_product(rule: &GameplayRule, reference: AssetProductRef) -> bool {
        rule.product == reference
    }
}

fn invalid_rule() -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new("test.material.invalid-rule").unwrap(),
        SafeSummary::new("The gameplay rule fixture is invalid").unwrap(),
    )
}

fn product(id: &str) -> AssetProductRef {
    primary(StableAssetId::parse_str(id).unwrap())
}

fn primary(id: StableAssetId) -> AssetProductRef {
    AssetProductRef::new(id, ImportedProductId::PRIMARY).unwrap()
}

fn bytes(value: usize) -> ByteLimit {
    ByteLimit::new(value).unwrap()
}
