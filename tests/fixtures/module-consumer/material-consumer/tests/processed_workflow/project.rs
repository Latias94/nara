use std::{
    collections::BTreeMap,
    convert::Infallible,
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use nara::{
    ProductRecipe,
    asset::{
        AssetImportSource, AssetMeta, AssetMetaCandidate, AssetMetaIdentity,
        AssetMetaIdentityCandidate, AssetPath, AssetPathScope, AssetProductRef, AssetRef,
        AssetSourceKind, AssetSourcePath, ImportedProductId, ProjectAssetDatabase,
        RuntimeAssetCodec, SourceAssetImporter, StableAssetId,
    },
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildObservation, ArtifactBuildStore,
        ArtifactContentDigest, ArtifactSourceRequest, VerifiedArtifactGroup, build_image_import,
        cook_image_group,
    },
    core::{ByteLimit, FormatVersion},
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, FileCapability,
        HostCapabilityOptions, TrustMode,
    },
    image::{
        ImageAsset, ImageColorSpace, ImageCookLimits, ImageCookSettings, ImageCookTarget,
        ImageExtent, ImageFormat, ImageImportSettings, ImageRuntimeCodec,
    },
    material::{
        Material2dAsset, Material2dDeclaration, Material2dDomain, Material2dRecord,
        Material2dRuntimeCodec, Material2dSource, MaterialParameterValue, MaterialParameterValues,
    },
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::{RuntimeConfigurationPlan, resolve_runtime_configuration},
    project_workflow::cooked_content::{
        CookedContentBuilder, CookedContentError, CookedContentLimits, CookedContentRequest,
        PreparedRuntimeContent,
    },
    reflect::{
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, PersistentComponentSchemaOwnerId,
        encode_asset_reference_value,
    },
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentBase,
        RuntimeContentCatalog, RuntimeContentCatalogDefinition, RuntimeContentTarget,
    },
    scene::{
        AuthoringDocumentSource, CompiledSceneCandidate, SceneComponentRecord, SceneDocument,
        SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, SourceDocumentKey,
        SourceDocumentKind,
    },
};
use nara_material_module_consumer::startup_materials;

use super::runtime::{InspectPalette, Probe};

pub(super) const MATERIAL_ID: &str = "consumer.palette";
pub(super) const MATERIAL_PATH: &str = "palette.material2d.json";
pub(super) const SHADER_PATH: &str = "palette.wgsl";
pub(super) const SHADER: &str = include_str!("../../src/palette.wgsl");
pub(super) const PIXELS: [u8; 4] = [24, 120, 220, 255];
const PNG: &[u8] = include_bytes!("../../../../images/valid-rgba-1x1.png");
const ROOT_PREFIX: &str = "nara-processed-palette-";

pub(super) fn material_product() -> AssetProductRef {
    primary("744cab2d-4c3c-43f0-bd13-8d72a8c698b1")
}

pub(super) fn indices_product() -> AssetProductRef {
    primary("744cab2d-4c3c-43f0-bd13-8d72a8c698b2")
}

pub(super) fn palette_product() -> AssetProductRef {
    primary("744cab2d-4c3c-43f0-bd13-8d72a8c698b3")
}

pub(super) fn scene_product() -> AssetProductRef {
    primary("744cab2d-4c3c-43f0-bd13-8d72a8c698b4")
}

fn primary(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

pub(super) fn actor() -> SceneEntityId {
    SceneEntityId::new("palette-sprite").unwrap()
}

pub(super) fn parameters(offset: f32) -> MaterialParameterValues {
    BTreeMap::from([
        (
            "palette.offset".to_owned(),
            MaterialParameterValue::F32(offset),
        ),
        (
            "palette.strength".to_owned(),
            MaterialParameterValue::F32(1.0),
        ),
        (
            "palette.reserved-z".to_owned(),
            MaterialParameterValue::F32(0.0),
        ),
        (
            "palette.reserved-w".to_owned(),
            MaterialParameterValue::F32(0.0),
        ),
    ])
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

pub(super) struct Fixture {
    _root: Root,
    database: ProjectAssetDatabase,
    compiled: CompiledSceneCandidate,
    images: [VerifiedArtifactGroup; 2],
    pub(super) recipe: ProductRecipe,
    pub(super) material: Material2dAsset,
    pub(super) probe: Probe,
}

impl Fixture {
    pub(super) fn new() -> Self {
        let root = Root::new();
        for path in ["assets", "scenes", "cache"] {
            fs::create_dir(root.path().join(path)).unwrap();
        }
        let probe = Probe::new(root.path().to_path_buf());
        let recipe = ProductRecipe::new()
            .add_configured_plugin(probe.clone(), |probe| InspectPalette(probe.clone()))
            .unwrap()
            .add_runtime_asset_codec::<Material2dDomain, Material2dRuntimeCodec>(
                RuntimeCodecDefinitionId::new("consumer.processed-palette.material", 1),
            )
            .unwrap()
            .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
                "consumer.processed-palette.image",
                1,
            ))
            .unwrap();
        let plan =
            resolve_runtime_configuration(&configuration().resolve().unwrap(), recipe.clone())
                .unwrap();
        let declaration = startup_materials()
            .unwrap()
            .current(MATERIAL_ID)
            .unwrap()
            .declaration()
            .clone();
        let source = Material2dSource::new(
            declaration.clone(),
            vec![AssetPath::new(SHADER_PATH).unwrap()],
            parameters(0.0),
            BTreeMap::from([("palette".to_owned(), AssetRef::path("palette.png").unwrap())]),
        )
        .unwrap();
        fs::write(
            root.path().join("assets").join(MATERIAL_PATH),
            source.encode().unwrap(),
        )
        .unwrap();
        fs::write(root.path().join("assets").join(SHADER_PATH), SHADER).unwrap();
        write_unit_metadata(
            &root.path().join("assets"),
            MATERIAL_PATH,
            material_product(),
            AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
        );
        for (path, product) in [
            ("indices.png", indices_product()),
            ("palette.png", palette_product()),
        ] {
            fs::write(root.path().join("assets").join(path), PNG).unwrap();
            let metadata = AssetMeta::new(
                AssetMetaIdentity::new(
                    product.source(),
                    AssetPath::new(path).unwrap(),
                    AssetSourceKind::Image,
                ),
                ImageImportSettings::new(ImageColorSpace::Srgb),
            );
            fs::write(
                root.path().join("assets").join(format!("{path}.meta")),
                metadata.to_json_string().unwrap(),
            )
            .unwrap();
        }
        let scope = asset_scope();
        let mut database = ProjectAssetDatabase::default();
        for path in ["indices.png", "palette.png"] {
            let bytes = fs::read(root.path().join("assets").join(format!("{path}.meta"))).unwrap();
            let candidate =
                AssetMetaCandidate::<ImageImportSettings>::decode_json_bytes(&bytes).unwrap();
            database
                .insert_identity(
                    &scope,
                    AssetPath::new(path).unwrap(),
                    candidate.identity().clone(),
                )
                .unwrap();
        }
        let metadata = AssetMetaIdentityCandidate::decode_json_bytes(
            &fs::read(
                root.path()
                    .join("assets")
                    .join(format!("{MATERIAL_PATH}.meta")),
            )
            .unwrap(),
        )
        .unwrap();
        let admitted = metadata.admit::<()>().unwrap();
        database
            .insert_identity(
                &scope,
                AssetPath::new(MATERIAL_PATH).unwrap(),
                admitted.identity().clone(),
            )
            .unwrap();
        let mut reads = Vec::new();
        let mut reader = |path: &AssetPath, limit: ByteLimit| {
            let bytes = fs::read(root.path().join("assets").join(path.as_str()))?;
            assert!(bytes.len() <= limit.get());
            reads.push(path.clone());
            Ok::<Arc<[u8]>, std::io::Error>(Arc::from(bytes))
        };
        let imported = Material2dDomain::prepare(&mut AssetImportSource::new(
            &metadata,
            Arc::from(fs::read(root.path().join("assets").join(MATERIAL_PATH)).unwrap()),
            &mut reader,
        ))
        .unwrap();
        assert_eq!(reads, [AssetPath::new(SHADER_PATH).unwrap()]);
        assert_eq!(imported.product(), material_product());
        assert_eq!(imported.generation().inputs().len(), 1);
        assert_eq!(
            imported.generation().inputs()[0].path().as_str(),
            SHADER_PATH
        );
        assert_eq!(imported.generation().inputs()[0].source(), SHADER);
        assert!(Material2dRuntimeCodec::encode_runtime(&imported).is_err());
        let resolver = database.resolver(&scope);
        let material = imported
            .resolve_texture_references(|reference| {
                let record = resolver.resolve_ref(reference)?;
                assert_eq!(record.source_kind(), &AssetSourceKind::Image);
                let product =
                    AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY).unwrap();
                Ok::<_, nara::asset::AssetDatabaseError>(product)
            })
            .unwrap();
        assert_eq!(material.generation(), imported.generation());
        assert_eq!(
            material.textures()["palette"],
            AssetRef::Product(palette_product()),
        );
        let store = ArtifactBuildStore::open(
            authority(&root.path().join("cache")),
            ArtifactBuildLimits::default(),
        )
        .unwrap();
        let images = [
            ("indices.png", indices_product()),
            ("palette.png", palette_product()),
        ]
        .map(|(path, product)| {
            let request = ArtifactSourceRequest::new(
                authority(&root.path().join("assets")),
                scope.clone(),
                AssetPath::new(path).unwrap(),
                definition("consumer.processed-palette.png-import"),
            );
            let imported = build_image_import(&store, &request, |_| Ok(())).unwrap();
            assert_eq!(imported.group().seed().source, product);
            assert_eq!(
                imported.group().seed().primary,
                Some(ArtifactContentDigest::of_bytes(PNG)),
            );
            let cooked = cook_image_group(
                &store,
                imported.group(),
                product,
                definition("consumer.processed-palette.image-cook"),
                ImageCookSettings::default(),
                ImageCookTarget::new(NonZeroU32::new(1).unwrap()),
                ImageCookLimits::new(ByteLimit::new(4096).unwrap(), ByteLimit::new(8192).unwrap()),
                |_| Ok(()),
            )
            .unwrap();
            assert!(cooked.group().observations().iter().any(|observation| {
                matches!(
                    observation,
                    ArtifactBuildObservation::Processed { product: actual, group, .. }
                        if *actual == product && *group == imported.group().digest()
                )
            }));
            let group = cooked.into_group();
            assert_eq!(group.members().len(), 1);
            let member = &group.members()[0];
            assert_eq!(member.products, [product]);
            assert_eq!(member.format.as_str(), ImageRuntimeCodec::RUNTIME_KIND);
            assert_eq!(member.format_version, FormatVersion::ONE);
            assert!(member.dependencies.is_empty());
            let bytes = group.member_bytes(&member.name).unwrap();
            assert_eq!(member.digest, ArtifactContentDigest::of_bytes(bytes));
            assert_image(&ImageRuntimeCodec::decode_runtime(bytes).unwrap(), product);
            group
        });
        let document = scene_document(&plan, &declaration);
        let scene_path = "startup.scene.json";
        fs::write(
            root.path().join("scenes").join(scene_path),
            document.to_json_string().unwrap(),
        )
        .unwrap();
        write_unit_metadata(
            &root.path().join("scenes"),
            scene_path,
            scene_product(),
            AssetSourceKind::Scene,
        );
        let scene_metadata = AssetMetaCandidate::<()>::decode_json_bytes(
            &fs::read(
                root.path()
                    .join("scenes")
                    .join(format!("{scene_path}.meta")),
            )
            .unwrap(),
        )
        .unwrap();
        database
            .insert_identity(
                &AssetPathScope::under(AssetSourcePath::new("scenes").unwrap()),
                AssetPath::new(scene_path).unwrap(),
                scene_metadata.identity().clone(),
            )
            .unwrap();
        let admitted = SceneDocumentCandidate::decode_json_bytes(
            &fs::read(root.path().join("scenes").join(scene_path)).unwrap(),
        )
        .unwrap()
        .admit_authoring(
            AuthoringDocumentSource::new(AssetRef::Product(scene_product())),
            plan.schema_validation().authoring_schema_bundle(),
            &[],
        )
        .unwrap();
        assert_eq!(admitted.document(), &document);
        let compilation = plan
            .schema_validation()
            .bindings()
            .try_compile_scene_with_loader(
                admitted.document(),
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
                scene_metadata.identity().stable_id(),
                SourceDocumentKind::Scene,
            ));
        Self {
            _root: root,
            database,
            compiled,
            images,
            recipe,
            material,
            probe,
        }
    }

    pub(super) fn assemble(
        &self,
        include_palette: bool,
    ) -> Result<PreparedRuntimeContent, Box<CookedContentError>> {
        let mut builder = CookedContentBuilder::new(
            CookedContentRequest {
                target: RuntimeContentTarget::new("test-host", "processed-palette").unwrap(),
                configuration: configuration(),
                startup_scene: scene_product(),
                scenes: vec![scene_product()],
                addressable_roots: Vec::new(),
            },
            self.recipe.clone(),
            CookedContentLimits::default(),
        )?;
        builder.add_group(&self.images[0])?;
        if include_palette {
            builder.add_group(&self.images[1])?;
        }
        builder.add_material(&self.material)?;
        builder.add_scene(
            scene_product(),
            &self.compiled,
            self.database.resolver(&asset_scope()),
        )?;
        builder.finish().map_err(Box::new)
    }
}

fn scene_document(
    plan: &RuntimeConfigurationPlan,
    declaration: &Material2dDeclaration,
) -> SceneDocument {
    let bindings = plan.schema_validation().bindings();
    let owner = |component: &str| {
        PersistentComponentSchemaOwnerId::try_new(
            bindings
                .native_snapshot()
                .component_schema_owner(&ComponentTypeId::new(component))
                .unwrap()
                .as_str(),
        )
        .unwrap()
    };
    let sprite_schema = bindings
        .schema(&ComponentTypeId::new("nara.sprite.Sprite"))
        .unwrap();
    let mut sprite = sprite_schema.authoring_initial_value().unwrap().clone();
    let ComponentValue::Map(fields) = &mut sprite else {
        panic!("Sprite must have a map initial value");
    };
    let Some(ComponentValue::Map(stock)) = fields.get_mut("material") else {
        panic!("Sprite must have its ordinary stock material");
    };
    stock.insert(
        "image".to_owned(),
        encode_asset_reference_value(&AssetRef::path("indices.png").unwrap()),
    );
    let material = Material2dRecord::new(
        AssetRef::path(MATERIAL_PATH).unwrap(),
        declaration,
        parameters(2.0),
        BTreeMap::new(),
    )
    .unwrap();
    assert!(material.texture_overrides().is_empty());
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(actor())
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(sprite_schema.version(), sprite)
                .with_owner(owner("nara.sprite.Sprite")),
        )
        .with_component(
            ComponentTypeId::new("nara.material2d"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                material.to_component_value().unwrap(),
            )
            .with_owner(owner("nara.material2d")),
        )])
}

fn asset_scope() -> AssetPathScope {
    AssetPathScope::under(AssetSourcePath::new("assets").unwrap())
}

fn write_unit_metadata(root: &Path, path: &str, product: AssetProductRef, kind: AssetSourceKind) {
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

fn definition(id: &str) -> ArtifactBuildDefinition {
    static IMPLEMENTATION: OnceLock<ArtifactContentDigest> = OnceLock::new();
    ArtifactBuildDefinition {
        id: id.to_owned(),
        revision: FormatVersion::ONE,
        implementation: *IMPLEMENTATION.get_or_init(|| {
            let file = FileCapability::from_host_handle(
                fs::File::open(std::env::current_exe().unwrap()).unwrap(),
                TrustMode::TrustedLocal,
                0,
            )
            .unwrap();
            ArtifactContentDigest::from_content(file.digest(1024 * 1024 * 1024).unwrap())
        }),
    }
}

pub(super) fn assert_image(image: &ImageAsset, product: AssetProductRef) {
    assert_eq!(image.product(), Some(product));
    assert_eq!(image.extent(), ImageExtent::new(1, 1));
    assert_eq!(image.format(), ImageFormat::Rgba8);
    assert_eq!(image.color_space(), ImageColorSpace::Srgb);
    assert_eq!(image.pixels(), PIXELS);
    assert!(image.source().is_none());
}

pub(super) fn catalog_definition(base: &RuntimeContentBase) -> RuntimeContentCatalogDefinition {
    let catalog = base.catalog();
    RuntimeContentCatalogDefinition {
        target: catalog.target().clone(),
        configuration: catalog.configuration().clone(),
        plugin_plan_fingerprint: catalog.plugin_plan_fingerprint(),
        schema_fingerprint: catalog.schema_fingerprint(),
        runtime_codecs_fingerprint: catalog.runtime_codecs_fingerprint(),
        startup_scene: catalog.startup_scene(),
        scenes: catalog.scenes().to_vec(),
        addressable_roots: catalog.addressable_roots().to_vec(),
        members: catalog.members().to_vec(),
    }
}

pub(super) fn write_catalog(root: &Root, catalog: &RuntimeContentCatalog) {
    fs::write(
        root.path().join(RUNTIME_CONTENT_CATALOG_FILE),
        catalog.to_json_bytes().unwrap(),
    )
    .unwrap();
}

pub(super) fn publish_payloads(base: &RuntimeContentBase) -> Root {
    let root = Root::new();
    for member in base.catalog().members() {
        let product = member.products()[0];
        let bytes = if product == scene_product() {
            base.acquire_startup_scene()
                .unwrap()
                .candidate()
                .to_json_bytes()
                .unwrap()
        } else if product == material_product() {
            let acquired = base
                .acquire_asset::<Material2dDomain, Material2dRuntimeCodec>(product)
                .unwrap();
            Material2dRuntimeCodec::encode_runtime(acquired.product())
                .unwrap()
                .into_vec()
        } else {
            assert!([indices_product(), palette_product()].contains(&product));
            let acquired = base
                .acquire_asset::<nara::image::ImageDomain, ImageRuntimeCodec>(product)
                .unwrap();
            ImageRuntimeCodec::encode_runtime(acquired.product())
                .unwrap()
                .into_vec()
        };
        assert_eq!(ContentDigest::of_bytes(&bytes), member.digest());
        let path = root.path().join(member.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    write_catalog(&root, base.catalog());
    root
}

pub(super) fn authority(path: &Path) -> DirectoryCapability {
    #[cfg(windows)]
    let directory = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .unwrap()
    };
    #[cfg(not(windows))]
    let directory = fs::File::open(path).unwrap();
    DirectoryCapability::from_host_handle(
        directory,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap()
}

pub(super) struct Root(PathBuf);

impl Root {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "{ROOT_PREFIX}{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let path = self.0.canonicalize().unwrap();
        assert!(path.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(ROOT_PREFIX)
        );
        fs::remove_dir_all(path).unwrap();
    }
}
