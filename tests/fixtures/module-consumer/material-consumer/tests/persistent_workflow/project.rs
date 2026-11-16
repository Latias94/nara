use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara::{
    ProductRecipe,
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetSourceKind,
        ImportedProductId, StableAssetId,
    },
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    image::ImageImportSettings,
    material::{
        Material2dDeclaration, Material2dDomain, Material2dRecord, Material2dSource,
        MaterialParameterValue, MaterialParameterValues,
    },
    project_host::{ingest_project_manifest, resolve_product_recipe},
    reflect::{
        AuthoringSchemaBundle, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        PersistentComponentSchemaOwnerId, encode_asset_reference_value,
    },
    scene::{
        AuthoringDocumentSource, SceneComponentRecord, SceneDocument, SceneDocumentCandidate,
        SceneEntityId, SceneEntityRecord,
    },
};
use nara_material_module_consumer::startup_materials;

pub(super) const MATERIAL_ID: &str = "consumer.palette";
pub(super) const CARRIER: &str = "nara.material2d";
pub(super) const SPRITE: &str = "nara.sprite.Sprite";
pub(super) const STARTUP: &str = "scenes/startup.scene.json";
pub(super) const MATERIAL_PATH: &str = "palette.material2d.json";
pub(super) const SHADER_PATH: &str = "palette.wgsl";
pub(super) const SHADER: &str = include_str!("../../src/palette.wgsl");
const PNG: &[u8] = include_bytes!("../../../../images/valid-rgba-1x1.png");
const MATERIAL_SOURCE: &str = "744cab2d-4c3c-43f0-bd13-8d72a8c698b1";
const INDICES_SOURCE: &str = "744cab2d-4c3c-43f0-bd13-8d72a8c698b2";
const PALETTE_SOURCE: &str = "744cab2d-4c3c-43f0-bd13-8d72a8c698b3";

pub(super) fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_asset_domain::<Material2dDomain>()
        .unwrap()
}

fn primary(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

pub(super) fn material_product() -> AssetProductRef {
    primary(MATERIAL_SOURCE)
}

pub(super) fn indices_product() -> AssetProductRef {
    primary(INDICES_SOURCE)
}

pub(super) fn palette_product() -> AssetProductRef {
    primary(PALETTE_SOURCE)
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

pub(super) struct Project {
    root: PathBuf,
    pub(super) bundle: AuthoringSchemaBundle,
    pub(super) declaration: Material2dDeclaration,
    pub(super) initial: SceneDocument,
}

impl Project {
    pub(super) fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nara-persistent-palette-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        write_manifest(&root, &[]);
        let capability = capability(&root);
        let manifest = capability
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, recipe()).unwrap();
        let bundle = plan.schema_validation().authoring_schema_bundle().clone();
        let materials = startup_materials().unwrap();
        let declaration = materials
            .current(MATERIAL_ID)
            .unwrap()
            .declaration()
            .clone();
        let record = Material2dRecord::new(
            AssetRef::Product(material_product()),
            &declaration,
            parameters(0.0),
            BTreeMap::new(),
        )
        .unwrap();
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
        let sprite_schema = bindings.schema(&ComponentTypeId::new(SPRITE)).unwrap();
        let mut sprite = sprite_schema.authoring_initial_value().unwrap().clone();
        let ComponentValue::Map(fields) = &mut sprite else {
            panic!("the public Sprite authoring initial value must be a map");
        };
        let Some(ComponentValue::Map(material)) = fields.get_mut("material") else {
            panic!("the public Sprite initial value must contain its stock material");
        };
        material.insert(
            "image".to_owned(),
            encode_asset_reference_value(&AssetRef::Product(indices_product())),
        );
        let initial = SceneDocument::from_ordered_roots([SceneEntityRecord::new(actor())
            .with_component(
                ComponentTypeId::new(SPRITE),
                SceneComponentRecord::new(sprite_schema.version(), sprite)
                    .with_owner(owner(SPRITE)),
            )
            .with_component(
                ComponentTypeId::new(CARRIER),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    record.to_component_value().unwrap(),
                )
                .with_owner(owner(CARRIER)),
            )]);
        let project = Self {
            root,
            bundle,
            declaration,
            initial,
        };
        fs::write(
            project.root.join("schema/authoring.bundle.json"),
            project.bundle.to_json_string().unwrap(),
        )
        .unwrap();
        let source = Material2dSource::new(
            project.declaration.clone(),
            vec![AssetPath::new(SHADER_PATH).unwrap()],
            parameters(0.0),
            BTreeMap::from([("palette".to_owned(), AssetRef::Product(palette_product()))]),
        )
        .unwrap();
        fs::write(
            project.root.join("assets").join(MATERIAL_PATH),
            source.encode().unwrap(),
        )
        .unwrap();
        fs::write(project.root.join("assets").join(SHADER_PATH), SHADER).unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                material_product().source(),
                AssetPath::new(MATERIAL_PATH).unwrap(),
                AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
            ),
            (),
        );
        fs::write(
            project
                .root
                .join("assets")
                .join(format!("{MATERIAL_PATH}.meta")),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
        for (path, product) in [
            ("indices.png", indices_product()),
            ("palette.png", palette_product()),
        ] {
            fs::write(project.root.join("assets").join(path), PNG).unwrap();
            let metadata = AssetMeta::new(
                AssetMetaIdentity::new(
                    product.source(),
                    AssetPath::new(path).unwrap(),
                    AssetSourceKind::Image,
                ),
                ImageImportSettings::default(),
            );
            fs::write(
                project.root.join("assets").join(format!("{path}.meta")),
                metadata.to_json_string().unwrap(),
            )
            .unwrap();
        }
        project.write_scene(STARTUP, &project.initial);
        project
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn capability(&self) -> DirectoryCapability {
        capability(&self.root)
    }

    pub(super) fn record(
        &self,
        offset: f32,
        overrides: BTreeMap<String, AssetRef>,
    ) -> Material2dRecord {
        Material2dRecord::new(
            AssetRef::Product(material_product()),
            &self.declaration,
            parameters(offset),
            overrides,
        )
        .unwrap()
    }

    pub(super) fn document(&self, material: ComponentValue) -> SceneDocument {
        let mut document = self.initial.clone();
        document.entities[0]
            .components
            .get_mut(&ComponentTypeId::new(CARRIER))
            .unwrap()
            .set_value(material);
        document
    }

    pub(super) fn write_scene(&self, path: &str, document: &SceneDocument) {
        fs::write(self.root.join(path), document.to_json_string().unwrap()).unwrap();
    }

    pub(super) fn read_scene(&self) -> SceneDocument {
        SceneDocumentCandidate::decode_json_bytes(&fs::read(self.root.join(STARTUP)).unwrap())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path(STARTUP).unwrap()),
                &self.bundle,
                &[],
            )
            .unwrap()
            .document()
            .clone()
    }

    pub(super) fn declare_scenes(&self, scenes: &[&str]) {
        write_manifest(&self.root, scenes);
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_manifest(root: &Path, scenes: &[&str]) {
    let declared = scenes
        .iter()
        .map(|path| format!("'{path}'"))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        root.join("nara.toml"),
        format!(
            concat!(
                "schema_version = 1\n[project]\nname = 'Persistent Palette consumer'\n",
                "source_universe = '744cab2d-4c3c-43f0-bd13-8d72a8c698b0'\n",
                "native_document_identity = 'project_location'\n",
                "[paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n",
                "[startup]\ndefault_scene = 'startup.scene.json'\n[content]\nscenes = [{}]\n",
                "[authoring]\nstate = 'ready'\nschema_bundle = 'schema/authoring.bundle.json'\n",
                "[runtime]\npreset = 'local-headless'\n[capabilities]\nrequested = ['runtime-2d']\n",
                "[tasks.io]\nworkers = 1\npending_capacity = 16\n",
                "[tasks.compute]\nworkers = 1\npending_capacity = 16\n",
                "[tasks.async_compute]\nworkers = 1\npending_capacity = 16\n",
            ),
            declared,
        ),
    )
    .unwrap();
}

fn capability(path: &Path) -> DirectoryCapability {
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
