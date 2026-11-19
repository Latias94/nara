use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{
    ProductRecipe, SchemaContribution,
    app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset::{
        AssetBuildSource, AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource,
        AssetMeta, AssetMetaIdentity, AssetPath, AssetPathScope, AssetReferenceDemand,
        AssetRuntimeError, AssetRuntimeLimits, RuntimeAssetCodec, SourceAssetImporter,
    },
    asset_build::ArtifactBuildLimits,
    core::{ByteLimit, ItemLimit},
    diagnostic::{Diagnostic, DiagnosticCode, SafeSummary},
    identity::SceneEntityId,
    project_host::{ProjectContentLimits, ProjectSettingsCandidate, ingest_project_manifest},
    project_tooling::host_fs,
    project_workflow::cooked_content::{CookedContentError, CookedContentLimits},
    reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
        ComponentSchemaVersion, ComponentTypeId, ComponentValueShape,
        PersistentComponentSchemaOwnerId, encode_asset_reference_value,
    },
    runtime_content::{RuntimeCodecDefinitionId, RuntimeContentTarget},
    scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord},
};

use super::super::*;

pub(super) const SOURCE: &str = "assets/shared.compound";
pub(super) const OPTIONAL: &str = "assets/build-only.bin";
const COMPONENT: &str = "test.cook.SharedCompound";
const PREFIX: &str = "nara-shared-cook-content-";
static NEXT: AtomicU64 = AtomicU64::new(0);
static BUILDS: [AtomicUsize; 7] = [const { AtomicUsize::new(0) }; 7];

pub(super) fn product(tag: u8) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("deb8f407-c402-4f08-b215-{tag:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

pub(super) fn auxiliary() -> AssetProductRef {
    AssetProductRef::new(
        product(3).source(),
        ImportedProductId::parse_subproduct("94d34240-cdb7-49d6-bdcc-ddf0cacdb542").unwrap(),
    )
    .unwrap()
}

fn diagnostic() -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new("test.cook.invalid-compound").unwrap(),
        SafeSummary::new("The compound fixture is invalid").unwrap(),
    )
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    rows: [u8; 2],
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Mapping {
    assignments: [(ImportedProductId, usize); 2],
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Compound {
    pub source: StableAssetId,
    pub auxiliary: ImportedProductId,
    pub source_value: u8,
    pub auxiliary_value: u8,
    pub build_value: Option<u8>,
}

pub(super) struct Domain<const CASE: usize>;
pub(super) struct Codec;

impl<const CASE: usize> AssetDomain for Domain<CASE> {
    type Product = Compound;

    fn contains_product(product: &Compound, id: ImportedProductId) -> bool {
        id == ImportedProductId::PRIMARY || id == product.auxiliary
    }
}

impl<const CASE: usize> SourceAssetImporter for Domain<CASE> {
    type Settings = Mapping;

    const SOURCE_KIND: &'static str = "test.cook.shared-compound";

    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(
            ByteLimit::new(256).unwrap(),
            ByteLimit::new(4096).unwrap(),
            ByteLimit::new(4096).unwrap(),
        )
    }

    fn prepare<E>(source: &mut AssetImportSource<'_, E>) -> Result<Compound, AssetImportError<E>> {
        let metadata = source
            .metadata()
            .admit::<Mapping>()
            .map_err(|_| AssetImportError::domain(diagnostic()))?;
        let body: Source = serde_json::from_slice(source.bytes())
            .map_err(|_| AssetImportError::domain(diagnostic()))?;
        let row = |product| {
            metadata
                .settings()
                .assignments
                .iter()
                .find(|(id, _)| *id == product)
                .and_then(|(_, index)| body.rows.get(*index))
                .copied()
                .ok_or_else(|| AssetImportError::domain(diagnostic()))
        };
        let auxiliary = auxiliary().product();
        Ok(Compound {
            source: source.metadata().identity().stable_id(),
            auxiliary,
            source_value: row(ImportedProductId::PRIMARY)?,
            auxiliary_value: row(auxiliary)?,
            build_value: None,
        })
    }
}

impl<const CASE: usize> SourceAssetBuild for Domain<CASE> {
    type Codec = Codec;

    const BUILD_ID: &'static str = "test.cook.shared-compound-build";
    const BUILD_REVISION: u32 = 1;

    fn normalize_settings(mut settings: Mapping) -> Result<Mapping, Diagnostic> {
        settings.assignments.sort_unstable_by_key(|(_, row)| *row);
        Ok(settings)
    }

    fn build<E>(source: &mut AssetBuildSource<'_, E>) -> Result<Compound, AssetImportError<E>> {
        BUILDS[CASE].fetch_add(1, Ordering::SeqCst);
        let mut product = source.prepare_import::<Self>()?;
        let path = AssetPath::new("build-only.bin").unwrap();
        product.build_value = source
            .read_optional_dependency(&path, ByteLimit::new(1).unwrap())?
            .map(|bytes| {
                let [value] = bytes.as_ref() else {
                    return Err(AssetImportError::domain(diagnostic()));
                };
                Ok(*value)
            })
            .transpose()?;
        Ok(product)
    }

    fn visit_products<E>(
        product: &Compound,
        mut visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        visitor(AssetProductRef::new(product.source, ImportedProductId::PRIMARY).unwrap())?;
        visitor(AssetProductRef::new(product.source, product.auxiliary).unwrap())
    }
}

impl RuntimeAssetCodec<Compound> for Codec {
    const RUNTIME_KIND: &'static str = "test_cook_shared_compound";
    const RUNTIME_VERSION: u32 = 1;

    fn runtime_limits() -> AssetRuntimeLimits {
        AssetRuntimeLimits::new(
            ByteLimit::new(512).unwrap(),
            ByteLimit::new(4096).unwrap(),
            ByteLimit::new(4096).unwrap(),
        )
    }

    fn encode_runtime(product: &Compound) -> Result<Box<[u8]>, AssetRuntimeError> {
        serde_json::to_vec(product)
            .map(Vec::into_boxed_slice)
            .map_err(|_| AssetRuntimeError::new(diagnostic()))
    }

    fn decode_runtime(bytes: &[u8]) -> Result<Compound, AssetRuntimeError> {
        serde_json::from_slice(bytes).map_err(|_| AssetRuntimeError::new(diagnostic()))
    }

    fn runtime_retained_bytes(_: &Compound) -> Result<usize, AssetRuntimeError> {
        Ok(size_of::<Compound>())
    }

    fn visit_runtime_dependencies<E>(
        _: &Compound,
        _: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        Ok(())
    }

    fn contains_runtime_product(product: &Compound, reference: AssetProductRef) -> bool {
        reference.source() == product.source
            && Domain::<0>::contains_product(product, reference.product())
    }
}

#[derive(Default)]
struct Definitions;

impl Plugin for Definitions {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("test.cook.shared-compound"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

pub(super) struct Fixture<const CASE: usize> {
    parent: PathBuf,
    pub path: PathBuf,
    pub root: DirectoryCapability,
    pub candidate: ProjectSettingsCandidate,
    pub recipe: ProductRecipe,
    pub store: ArtifactBuildStore,
}

impl<const CASE: usize> Fixture<CASE> {
    pub fn empty_scenes() -> Self {
        let fixture = Self::new(None);
        let empty = SceneDocument::default().to_json_string().unwrap();
        for path in ["scenes/first.scene.json", "scenes/second.scene.json"] {
            fixture.write(path, empty.as_bytes());
        }
        fixture
    }

    pub fn new(optional: Option<u8>) -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "{PREFIX}{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&parent).unwrap();
        let path = parent.join("project");
        for directory in ["assets", "scenes", "prefabs"] {
            fs::create_dir_all(path.join(directory)).unwrap();
        }
        let owner = PersistentComponentSchemaOwnerId::try_new("test.cook.shared-compound").unwrap();
        let schema = ComponentSchema::new(
            ComponentTypeId::new(COMPONENT),
            "Shared compound",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::empty(),
            ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::AssetRef)]);
        let snapshot = ComponentSchemaOwnerSnapshot::admit(
            owner.clone(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let recipe = ProductRecipe::new()
            .with_source_content_limits(
                ProjectContentLimits::default()
                    .with_retained_generations(ItemLimit::new(1).unwrap()),
            )
            .add_contribution(
                SchemaContribution::<Definitions>::data_for_default([snapshot]).unwrap(),
            )
            .unwrap()
            .add_asset_domain::<Domain<CASE>>()
            .unwrap()
            .add_runtime_asset_codec::<Domain<CASE>, Codec>(RuntimeCodecDefinitionId::new(
                "test.cook.shared-compound",
                1,
            ))
            .unwrap();
        fs::write(
            path.join("nara.toml"),
            r#"schema_version = 1
[project]
source_universe = 'a75fac7e-f4c9-4c3b-870b-576576bf3560'
native_document_identity = 'project_location'
name = "Shared cook content test"
[authoring]
state = "ready"
schema_bundle = "schema.bundle.json"
[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"
[startup]
default_scene = "first.scene.json"
[content]
scenes = ["second.scene.json"]
[runtime]
preset = "local-headless"
[capabilities]
requested = ["runtime-2d"]
"#,
        )
        .unwrap();
        for (name, scene, reference) in [
            ("first.scene.json", product(1), product(3)),
            ("second.scene.json", product(2), auxiliary()),
        ] {
            let entity = SceneEntityRecord::new(SceneEntityId::new("shared").unwrap())
                .with_component(
                    ComponentTypeId::new(COMPONENT),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        encode_asset_reference_value(&AssetRef::Product(reference)),
                    )
                    .with_owner(owner.clone()),
                );
            fs::write(
                path.join("scenes").join(name),
                SceneDocument::from_ordered_roots([entity])
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
            write_metadata(
                &path,
                &format!("scenes/{name}"),
                name,
                scene.source(),
                AssetSourceKind::Scene,
            );
        }
        let root = host_fs::readable_directory(&path).unwrap();
        let manifest = root
            .open_file_exact(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
        fs::write(
            path.join("schema.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let store_path = parent.join("store");
        fs::create_dir(&store_path).unwrap();
        let store = ArtifactBuildStore::open(
            host_fs::writable_directory(&store_path).unwrap(),
            ArtifactBuildLimits::default(),
        )
        .unwrap();
        let fixture = Self {
            parent,
            path,
            root,
            candidate,
            recipe,
            store,
        };
        fixture.write_source(b'S');
        fixture.write_mapping([(ImportedProductId::PRIMARY, 0), (auxiliary().product(), 1)]);
        if let Some(value) = optional {
            fixture.write(OPTIONAL, &[value]);
        }
        fixture
    }

    pub fn write(&self, path: &str, bytes: &[u8]) {
        fs::write(self.path.join(path), bytes).unwrap();
    }

    pub fn write_source(&self, value: u8) {
        self.write(
            SOURCE,
            &serde_json::to_vec(&Source {
                rows: [value, b'Q'],
            })
            .unwrap(),
        );
    }

    pub fn write_mapping(&self, assignments: [(ImportedProductId, usize); 2]) {
        self.write(
            "assets/shared.compound.meta",
            AssetMeta::new(
                AssetMetaIdentity::new(
                    product(3).source(),
                    AssetPath::new("shared.compound").unwrap(),
                    AssetSourceKind::Other(Domain::<CASE>::SOURCE_KIND.to_owned()),
                ),
                Mapping { assignments },
            )
            .to_json_string()
            .unwrap()
            .as_bytes(),
        );
    }

    pub fn build_group(&self) -> VerifiedArtifactGroup {
        let request = ArtifactSourceRequest::new(
            self.root
                .open_directory_exact(&RelativePath::new("assets").unwrap())
                .unwrap(),
            AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
            AssetPath::new("shared.compound").unwrap(),
            ArtifactBuildDefinition {
                id: Domain::<CASE>::BUILD_ID.to_owned(),
                revision: FormatVersion::new(Domain::<CASE>::BUILD_REVISION).unwrap(),
                implementation: self.request().implementation,
            },
        );
        build_source::<Domain<CASE>>(&self.store, &request, |_| Ok(()))
            .unwrap()
            .into_group()
    }

    pub fn builds(&self) -> usize {
        BUILDS[CASE].load(Ordering::SeqCst)
    }

    pub fn request(&self) -> ProjectCookRequest<'_> {
        ProjectCookRequest {
            root: &self.root,
            candidate: &self.candidate,
            recipe: self.recipe.clone(),
            target: target(),
            store: &self.store,
            implementation: ArtifactContentDigest::of_bytes(b"shared cook fixture build v1"),
            limits: CookedContentLimits::default(),
        }
    }

    pub fn builder(
        &self,
        limits: CookedContentLimits,
    ) -> Result<CookedContentBuilder, CookedContentError> {
        CookedContentBuilder::new(
            CookedContentRequest {
                target: target(),
                configuration: self.candidate.runtime_configuration().to_configuration(),
                startup_scene: product(1),
                scenes: vec![product(1)],
                addressable_roots: Vec::new(),
            },
            self.recipe.clone(),
            limits,
        )
    }
}

impl<const CASE: usize> Drop for Fixture<CASE> {
    fn drop(&mut self) {
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        let parent = self.parent.canonicalize().unwrap();
        let relative = parent.strip_prefix(&temporary).unwrap();
        assert_eq!(relative.components().count(), 1);
        assert!(relative.to_str().unwrap().starts_with(PREFIX));
        fs::remove_dir_all(parent).unwrap();
    }
}

fn target() -> RuntimeContentTarget {
    RuntimeContentTarget::new("x86_64-pc-windows-msvc", "shared-cook-test").unwrap()
}

pub(super) fn write_metadata(
    path: &std::path::Path,
    source: &str,
    alias: &str,
    stable: StableAssetId,
    kind: AssetSourceKind,
) {
    fs::write(
        path.join(format!("{source}.meta")),
        AssetMeta::new(
            AssetMetaIdentity::new(stable, AssetPath::new(alias).unwrap(), kind),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
}
