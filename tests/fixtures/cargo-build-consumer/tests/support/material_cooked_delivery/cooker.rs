#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    convert::Infallible,
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use engine::{
    asset::{
        AssetImportSource, AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath,
        AssetPathScope, AssetProductRef, AssetRef, AssetReferenceDemand, AssetSourceKind,
        AssetSourcePath, ImportedProductId, ProjectAssetDatabase, RuntimeAssetCodec,
        SourceAssetImporter,
    },
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildError, ArtifactBuildLimits, ArtifactBuildObservation,
        ArtifactBuildStore, ArtifactContentDigest, ArtifactSourceRequest, VerifiedArtifactGroup,
        build_image_import, cook_image_group,
    },
    core::{ByteLimit, FormatVersion},
    fs::{ContentDigest, DirectoryCapability, RelativeComponent, RelativePath},
    image::{
        ImageColorSpace, ImageCookLimits, ImageCookSettings, ImageCookTarget, ImageExtent,
        ImageImportSettings, ImageRuntimeCodec,
    },
    material::{Material2dDomain, Material2dRecord, Material2dRuntimeCodec, Material2dSource},
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::{RuntimeConfigurationPlan, resolve_runtime_configuration},
    project_tooling::{
        cargo_build::{
            CargoBuildFailure, CargoBuildLimits, CargoBuildRequest, CargoBuildTools, build_executable,
        },
        host_fs,
        player_package::{
            GeneratedPlayerBootstrap, PlayerPackageControlFailure, PlayerPackageLimits,
            PlayerPackageOutcome, PlayerPackagePoint, PreparedPlayerPackage,
        },
    },
    project_workflow::{
        cargo::CargoBuildSelection,
        cooked_content::{CookedContentBuilder, CookedContentLimits, CookedContentRequest},
    },
    reflect::{
        ComponentTypeId, ComponentValue, PersistentComponentSchemaOwnerId,
        encode_asset_reference_value,
    },
    runtime_content::{RuntimeContentDependencyRole, RuntimeContentTarget},
    scene::{
        AuthoringDocumentSource, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
        SceneEntityId, SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
    },
};
use palette_cooked_game as game;

const GENERATED: &str = "src/player/generated.rs";
const PLAYER_INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    ".cargo/config.toml",
    "src/lib.rs",
    "src/probe.rs",
    "src/palette.wgsl",
    "src/cooker.rs",
    "src/player/main.rs",
    GENERATED,
];
const SOURCE_INPUTS: &[&str] = &[
    "indices.png",
    "indices.png.meta",
    "palette.png",
    "palette.png.meta",
    game::MATERIAL_PATH,
    "palette.material2d.json.meta",
    game::SHADER_PATH,
    "startup.scene.json",
    "startup.scene.json.meta",
];

fn main() {
    assert!(cfg!(feature = "cook"));
    assert!(!cfg!(feature = "player"));
    let mut arguments = std::env::args_os().skip(1);
    let workspace = PathBuf::from(arguments.next().unwrap())
        .canonicalize()
        .unwrap();
    let output = PathBuf::from(arguments.next().unwrap()).canonicalize().unwrap();
    let cargo = PathBuf::from(arguments.next().unwrap());
    let rustc = PathBuf::from(arguments.next().unwrap());
    let triple = arguments.next().unwrap().into_string().unwrap();
    let target = PathBuf::from(arguments.next().unwrap()).canonicalize().unwrap();
    assert!(arguments.next().is_none());
    assert_eq!(std::env::var("CARGO_NET_OFFLINE").unwrap(), "true");
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").unwrap(), "1");

    let configuration = configuration();
    let recipe = game::recipe();
    let plan =
        resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone()).unwrap();
    let source = workspace.join("source");
    prepare_source(&source, &plan);
    let inputs = SourceInputs::capture(&source);
    let scope = AssetPathScope::under(AssetSourcePath::new("assets").unwrap());
    let mut database = ProjectAssetDatabase::default();
    for path in ["indices.png", "palette.png"] {
        let metadata =
            AssetMetaIdentityCandidate::decode_json_bytes(&inputs.bytes(&format!("{path}.meta")))
                .unwrap()
                .admit::<ImageImportSettings>()
                .unwrap();
        database
            .insert_identity(&scope, AssetPath::new(path).unwrap(), metadata.identity().clone())
            .unwrap();
    }
    for path in [game::MATERIAL_PATH, "startup.scene.json"] {
        let metadata =
            AssetMetaIdentityCandidate::decode_json_bytes(&inputs.bytes(&format!("{path}.meta")))
                .unwrap()
                .admit::<()>()
                .unwrap();
        database
            .insert_identity(&scope, AssetPath::new(path).unwrap(), metadata.identity().clone())
            .unwrap();
    }
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(
        &inputs.bytes("palette.material2d.json.meta"),
    )
    .unwrap();
    assert_eq!(Material2dDomain::SOURCE_KIND, "material2d");
    assert_eq!(Material2dRuntimeCodec::RUNTIME_KIND, "runtime_material2d");
    let mut reads = Vec::new();
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        let bytes = inputs.bytes(path.as_str());
        assert!(bytes.len() <= limit.get());
        reads.push(path.clone());
        Ok::<_, Infallible>(bytes)
    };
    let imported = Material2dDomain::prepare(&mut AssetImportSource::new(
        &metadata,
        inputs.bytes(game::MATERIAL_PATH),
        &mut reader,
    ))
    .unwrap();
    assert_eq!(reads, [AssetPath::new(game::SHADER_PATH).unwrap()]);
    assert_eq!(imported.product(), game::material_product());
    assert_eq!(imported.generation().declaration(), &game::declaration());
    assert_eq!(imported.generation().inputs().len(), 1);
    assert_eq!(
        imported.generation().inputs()[0].path().as_str(),
        game::SHADER_PATH
    );
    assert_eq!(imported.generation().inputs()[0].source(), game::SHADER);
    assert_eq!(
        imported.generation().shader().user_source(),
        format!("{}\n", game::SHADER)
    );
    let mut dependencies = Vec::new();
    Material2dDomain::visit_source_dependencies(&imported, |reference, kind, demand| {
        dependencies.push((reference.clone(), kind.clone(), demand));
        Ok::<_, Infallible>(())
    })
    .unwrap();
    assert_eq!(
        dependencies,
        [(
            AssetRef::path("palette.png").unwrap(),
            AssetSourceKind::Image,
            AssetReferenceDemand::Immediate,
        )],
    );
    let resolver = database.resolver(&scope);
    let material = imported
        .resolve_texture_references(|reference| {
            let record = resolver.resolve_ref(reference)?;
            assert_eq!(record.source_kind(), &AssetSourceKind::Image);
            Ok::<_, engine::asset::AssetDatabaseError>(
                AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY).unwrap(),
            )
        })
        .unwrap();
    assert_eq!(material.generation(), imported.generation());
    assert_eq!(
        material.textures(),
        &BTreeMap::from([("palette".to_owned(), AssetRef::Product(game::palette_product()))]),
    );
    let encoded_material = Material2dRuntimeCodec::encode_runtime(&material).unwrap();
    let decoded = Material2dRuntimeCodec::decode_runtime(&encoded_material).unwrap();
    assert_eq!(decoded.generation().shader(), material.generation().shader());
    assert!(decoded.generation().inputs().is_empty());

    let cache = workspace.join("source-cache");
    fs::create_dir(&cache).unwrap();
    let store = ArtifactBuildStore::open(
        host_fs::writable_directory(&cache).unwrap(),
        ArtifactBuildLimits::default(),
    )
    .unwrap();
    let images = [
        ("indices.png", game::indices_product()),
        ("palette.png", game::palette_product()),
    ]
    .map(|(path, product)| cook_image(&store, &inputs, &scope, path, product));
    let document = SceneDocumentCandidate::decode_json_bytes(&inputs.bytes("startup.scene.json"))
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::Product(game::scene_product())),
            plan.schema_validation().authoring_schema_bundle(),
            &[],
        )
        .unwrap();
    let compilation = plan
        .schema_validation()
        .bindings()
        .try_compile_scene_with_loader(
            document.document(),
            Default::default(),
            Some(database.resolver(&scope)),
            |_, _| Ok::<_, Infallible>(None),
        )
        .unwrap();
    assert!(
        !compilation.diagnostics.has_errors(),
        "{:?}",
        compilation.diagnostics,
    );
    let compiled = compilation
        .candidate
        .unwrap()
        .bind_source(SourceDocumentKey::stable_asset(
            game::scene_product().source(),
            SourceDocumentKind::Scene,
        ));
    let mut builder = CookedContentBuilder::new(
        CookedContentRequest {
            target: RuntimeContentTarget::new(&triple, "palette-shipping").unwrap(),
            configuration,
            startup_scene: game::scene_product(),
            scenes: vec![game::scene_product()],
            addressable_roots: Vec::new(),
        },
        recipe,
        CookedContentLimits::default(),
    )
    .unwrap();
    builder.add_material(&material).unwrap();
    for group in &images {
        builder.add_group(group).unwrap();
    }
    builder
        .add_scene(game::scene_product(), &compiled, database.resolver(&scope))
        .unwrap();
    let content = builder.finish().unwrap();
    let catalog = content.base().catalog();
    assert_eq!(catalog.members().len(), 4);
    let scene_edges = catalog
        .member_for_product(game::scene_product())
        .unwrap()
        .dependencies();
    assert_eq!(scene_edges.len(), 2);
    for product in [game::material_product(), game::indices_product()] {
        assert!(scene_edges.iter().any(|edge| edge.product() == product
            && edge.role() == RuntimeContentDependencyRole::Required));
    }
    let material_edges = catalog
        .member_for_product(game::material_product())
        .unwrap()
        .dependencies();
    assert_eq!(material_edges.len(), 1);
    assert_eq!(material_edges[0].product(), game::palette_product());
    assert_eq!(
        material_edges[0].role(),
        RuntimeContentDependencyRole::Required
    );

    inputs.package_fence().unwrap();
    let limits = PlayerPackageLimits::default();
    let bootstrap = GeneratedPlayerBootstrap::new(&content, GENERATED, limits).unwrap();
    fs::write(workspace.join(GENERATED), bootstrap.bytes()).unwrap();
    let selection = CargoBuildSelection::new(
        "palette-cooked-game",
        "palette-cooked-player",
        "consumer-checked",
        &triple,
        ["player"],
        false,
    )
    .unwrap();
    let request = CargoBuildRequest::new(
        &workspace,
        RelativePath::new("Cargo.toml").unwrap(),
        selection.clone(),
        CargoBuildTools::new(&cargo, &rustc).unwrap(),
        PLAYER_INPUTS.iter().map(|path| RelativePath::new(path).unwrap()),
        CargoBuildLimits::default(),
    )
    .unwrap()
    .with_target_directory(&target)
    .unwrap();
    let executable = build_executable(&request, |_| {
        inputs.package_fence().map_err(|_| CargoBuildFailure::Stale)
    })
    .unwrap();
    assert_eq!(executable.provenance().selection(), &selection);
    assert_eq!(executable.provenance().resolved_features(), ["player"]);
    assert_eq!(
        executable.provenance().declared_inputs().len(),
        PLAYER_INPUTS.len()
    );
    assert!(
        executable
            .provenance()
            .declared_inputs()
            .iter()
            .any(|input| input.path() == GENERATED && input.digest() == bootstrap.digest())
    );
    let prepared =
        PreparedPlayerPackage::bind(content, bootstrap, &selection, executable, limits).unwrap();
    let mut points = Vec::new();
    let published = prepared.publish(
        &host_fs::writable_directory(&output).unwrap(),
        &RelativeComponent::new("published").unwrap(),
        |point| {
            points.push(point);
            inputs.package_fence()
        },
    );
    let PlayerPackageOutcome::Published {
        receipt,
        error: None,
    } = published
    else {
        panic!("{published:?}");
    };
    assert!(points.contains(&PlayerPackagePoint::AfterPackWrite));
    assert!(points.contains(&PlayerPackagePoint::BeforePublication));
    assert_eq!(points.last(), Some(&PlayerPackagePoint::AfterPublication));
    assert_eq!(
        receipt.files.executable_name,
        format!("palette-cooked-player{}", std::env::consts::EXE_SUFFIX)
    );
    println!("palette-cooker ok png-import material-import canonical-products published");
}

fn configuration() -> RuntimeConfiguration {
    let mut configuration = RuntimeConfiguration::default();
    configuration.runtime.preset = RuntimePreset::LocalHeadless;
    configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
    configuration.tasks.io.workers = 1;
    configuration.tasks.compute.workers = 1;
    configuration.tasks.async_compute.workers = 1;
    configuration
}

struct SourceInputs {
    directory: DirectoryCapability,
    files: Vec<(&'static str, Arc<[u8]>, ContentDigest)>,
}

impl SourceInputs {
    fn capture(path: &Path) -> Self {
        let directory = host_fs::readable_directory(path).unwrap();
        let files = SOURCE_INPUTS
            .iter()
            .map(|path| {
                let bytes: Arc<[u8]> = directory
                    .open_file_exact(&RelativePath::new(path).unwrap())
                    .unwrap()
                    .read_to_end_bounded(1024 * 1024)
                    .unwrap()
                    .into();
                let digest = ContentDigest::of_bytes(&bytes);
                (*path, bytes, digest)
            })
            .collect();
        Self { directory, files }
    }

    fn bytes(&self, path: &str) -> Arc<[u8]> {
        let (_, bytes, _) = self.files.iter().find(|(name, _, _)| *name == path).unwrap();
        Arc::clone(bytes)
    }

    fn current(&self) -> bool {
        self.files.iter().all(|(path, _, digest)| {
            self.directory
                .open_file_exact(&RelativePath::new(path).unwrap())
                .is_ok_and(|file| file.verify_digest(*digest, digest.length()).is_ok())
        })
    }

    fn artifact_fence(&self) -> Result<(), ArtifactBuildError> {
        self.current().then_some(()).ok_or(ArtifactBuildError::Stale)
    }

    fn package_fence(&self) -> Result<(), PlayerPackageControlFailure> {
        self.current()
            .then_some(())
            .ok_or(PlayerPackageControlFailure::SelectionChanged)
    }
}

fn definition(id: &str) -> ArtifactBuildDefinition {
    static IMPLEMENTATION: OnceLock<ArtifactContentDigest> = OnceLock::new();
    ArtifactBuildDefinition {
        id: id.to_owned(),
        revision: FormatVersion::ONE,
        implementation: *IMPLEMENTATION.get_or_init(|| {
            let path = std::env::current_exe().unwrap();
            let file = host_fs::readable_file(&path).unwrap();
            ArtifactContentDigest::from_content(
                file.digest(fs::metadata(path).unwrap().len()).unwrap(),
            )
        }),
    }
}

fn cook_image(
    store: &ArtifactBuildStore,
    inputs: &SourceInputs,
    scope: &AssetPathScope,
    path: &str,
    product: AssetProductRef,
) -> VerifiedArtifactGroup {
    let request = ArtifactSourceRequest::new(
        inputs.directory.share(),
        scope.clone(),
        AssetPath::new(path).unwrap(),
        definition("consumer.palette.png-import"),
    );
    let imported = build_image_import(store, &request, |_| inputs.artifact_fence()).unwrap();
    assert_eq!(imported.group().seed().source, product);
    assert_eq!(
        imported.group().seed().primary,
        Some(ArtifactContentDigest::of_bytes(&inputs.bytes(path)))
    );
    let cooked = cook_image_group(
        store,
        imported.group(),
        product,
        definition("consumer.palette.image-cook"),
        ImageCookSettings::default(),
        ImageCookTarget::new(NonZeroU32::new(1).unwrap()),
        ImageCookLimits::new(
            ByteLimit::new(4096).unwrap(),
            ByteLimit::new(8192).unwrap(),
        ),
        |_| inputs.artifact_fence(),
    )
    .unwrap();
    let group = cooked.into_group();
    assert!(matches!(
        group.observations(),
        [ArtifactBuildObservation::Processed { product: actual, group, .. }]
            if *actual == product && *group == imported.group().digest()
    ));
    assert_eq!(group.members().len(), 1);
    let member = &group.members()[0];
    assert_eq!(member.products, [product]);
    assert_eq!(member.format.as_str(), ImageRuntimeCodec::RUNTIME_KIND);
    assert_eq!(member.format_version, FormatVersion::ONE);
    assert!(member.dependencies.is_empty());
    let bytes = group.member_bytes(&member.name).unwrap();
    assert_eq!(member.digest, ArtifactContentDigest::of_bytes(bytes));
    let decoded = ImageRuntimeCodec::decode_runtime(bytes).unwrap();
    assert_eq!(decoded.product(), Some(product));
    assert_eq!(decoded.extent(), ImageExtent::new(1, 1));
    assert_eq!(decoded.pixels(), game::PIXELS);
    group
}

fn prepare_source(root: &Path, plan: &RuntimeConfigurationPlan) {
    fs::write(root.join(game::SHADER_PATH), game::SHADER).unwrap();
    let material = Material2dSource::new(
        game::declaration(),
        vec![AssetPath::new(game::SHADER_PATH).unwrap()],
        game::parameters(0.0),
        BTreeMap::from([("palette".to_owned(), AssetRef::path("palette.png").unwrap())]),
    )
    .unwrap();
    fs::write(root.join(game::MATERIAL_PATH), material.encode().unwrap()).unwrap();
    for (path, product) in [
        ("indices.png", game::indices_product()),
        ("palette.png", game::palette_product()),
    ] {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                product.source(),
                AssetPath::new(path).unwrap(),
                AssetSourceKind::Image,
            ),
            ImageImportSettings::new(ImageColorSpace::Srgb),
        );
        fs::write(
            root.join(format!("{path}.meta")),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }
    for (path, product, kind) in [
        (
            game::MATERIAL_PATH,
            game::material_product(),
            AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
        ),
        (
            "startup.scene.json",
            game::scene_product(),
            AssetSourceKind::Scene,
        ),
    ] {
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
    let bindings = plan.schema_validation().bindings();
    let component = |id: &str, value: ComponentValue| {
        let id = ComponentTypeId::new(id);
        let owner = PersistentComponentSchemaOwnerId::try_new(
            bindings
                .native_snapshot()
                .component_schema_owner(&id)
                .unwrap()
                .as_str(),
        )
        .unwrap();
        SceneComponentRecord::new(bindings.schema(&id).unwrap().version(), value).with_owner(owner)
    };
    let mut sprite = bindings
        .schema(&ComponentTypeId::new("nara.sprite.Sprite"))
        .unwrap()
        .authoring_initial_value()
        .unwrap()
        .clone();
    let ComponentValue::Map(fields) = &mut sprite else {
        panic!("Sprite initial value must be a map");
    };
    let Some(ComponentValue::Map(stock)) = fields.get_mut("material") else {
        panic!("Sprite stock material must be present");
    };
    stock.insert(
        "image".to_owned(),
        encode_asset_reference_value(&AssetRef::path("indices.png").unwrap()),
    );
    let material = Material2dRecord::new(
        AssetRef::path(game::MATERIAL_PATH).unwrap(),
        &game::declaration(),
        game::parameters(2.0),
        BTreeMap::new(),
    )
    .unwrap();
    let document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(SceneEntityId::new("palette-sprite").unwrap())
            .with_component(
                ComponentTypeId::new("nara.sprite.Sprite"),
                component("nara.sprite.Sprite", sprite),
            )
            .with_component(
                ComponentTypeId::new("nara.material2d"),
                component("nara.material2d", material.to_component_value().unwrap()),
            ),
    ]);
    fs::write(
        root.join("startup.scene.json"),
        document.to_json_string().unwrap(),
    )
    .unwrap();
}
