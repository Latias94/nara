#![allow(dead_code)]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    image::ImageImportSettings,
    project_host::{
        ProjectSettingsCandidate, RuntimePlan, advanced::built_in_schema_providers,
        ingest_project_manifest, resolve_product_recipe,
    },
    reflect::{AuthoringSchemaBundle, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{
        PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityRecord,
    },
};

pub type ImageAssetMeta = AssetMeta<ImageImportSettings>;
pub type TileSetAssetMeta = AssetMeta<()>;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static BUILT_IN_AUTHORING_SCHEMA_BUNDLE: OnceLock<Box<[u8]>> = OnceLock::new();

pub struct TestProject {
    root: PathBuf,
}

impl TestProject {
    pub fn with_prefab_startup() -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_project_content_{}_{}",
            std::process::id(),
            sequence
        ));
        fs::create_dir_all(root.join("assets/textures")).unwrap();
        fs::create_dir_all(root.join("scenes")).unwrap();
        fs::create_dir_all(root.join("prefabs")).unwrap();
        fs::create_dir_all(root.join("schema")).unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            built_in_authoring_schema_bundle(),
        )
        .unwrap();
        fs::write(
            root.join("nara.toml"),
            r#"schema_version = 1

[project]
source_universe = '39f9d09d-63e3-5fc4-8e3f-71e84893cb8d'
native_document_identity = 'project_location'
name = "Content Test"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[authoring]
schema_bundle = "schema/authoring.bundle.json"

[capabilities]
requested = ["runtime-2d"]
"#,
        )
        .unwrap();

        let source = AssetRef::path("enemy.prefab.json").unwrap();
        let mut anchor = SceneEntityRecord::new(scene_id("enemy-anchor")).with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            identity_transform_record(),
        );
        anchor.prefab = Some(PrefabInstance {
            source,
            overrides: None,
        });
        let scene = SceneDocument::from_ordered_roots([anchor]);
        let image = AssetRef::path("textures/player.png").unwrap();
        let enemy = SceneEntityRecord::new(scene_id("enemy"))
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                identity_transform_record(),
            )
            .with_component(
                ComponentTypeId::new("nara.sprite.Sprite"),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&image)),
            );
        let prefab = PrefabDocument::from_ordered_roots([enemy]);
        fs::write(
            root.join("scenes/startup.scene.json"),
            scene.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("prefabs/enemy.prefab.json"),
            prefab.to_json_string().unwrap(),
        )
        .unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
            root.join("assets/textures/player.png"),
        )
        .unwrap();
        let meta = player_image_meta();
        fs::write(
            root.join("assets/textures/player.png.meta"),
            meta.to_json_string().unwrap(),
        )
        .unwrap();

        Self { root }
    }

    pub fn root_capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.root, CapabilityRights::ReadOnly),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
        )
        .unwrap()
    }

    pub fn writable_root_capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.root, CapabilityRights::ReadWrite),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }

    pub fn candidate_plan_and_root(
        &self,
    ) -> (ProjectSettingsCandidate, RuntimePlan, DirectoryCapability) {
        let root = self.root_capability();
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, nara::ProductRecipe::new()).unwrap();
        (candidate, plan, root)
    }

    pub fn write_prefab_source(&self, document: &PrefabDocument) {
        fs::write(
            self.root.join("prefabs/enemy.prefab.json"),
            document.to_json_string().unwrap(),
        )
        .unwrap();
    }

    pub fn prefab_projection(
        &self,
        anchor: &nara::scene::SceneEntityId,
        source: &nara::scene::SceneEntityId,
    ) -> nara::scene::SceneEntityId {
        let (candidate, plan, root) = self.candidate_plan_and_root();
        let snapshot = nara::project_host::ProjectContentLoader::new(root)
            .unwrap()
            .load(&candidate, &plan)
            .unwrap();
        let mut matches = snapshot
            .expanded_scene_provenance()
            .iter()
            .filter(|(_, provenance)| {
                provenance.local_entity_id() == source
                    && provenance
                        .projection_path()
                        .is_some_and(|path| path.root_anchor() == anchor)
            });
        let (projected, _) = matches
            .next()
            .expect("the authored instance must contain this source");
        assert!(
            matches.next().is_none(),
            "the fixture selects one concrete prefab projection"
        );
        projected.clone()
    }

    pub fn select_local_headless_profile(&self) {
        fs::write(
            self.root.join("nara.toml"),
            r#"schema_version = 1

[project]
source_universe = '39f9d09d-63e3-5fc4-8e3f-71e84893cb8d'
native_document_identity = 'project_location'
name = "Runtime Test"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[authoring]
schema_bundle = "schema/authoring.bundle.json"

[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d"]
"#,
        )
        .unwrap();
    }

    pub fn write_scene_source(&self, document: &SceneDocument) {
        fs::write(
            self.root.join("scenes/startup.scene.json"),
            document.to_json_string().unwrap(),
        )
        .unwrap();
    }

    pub fn use_stable_native_identity(&self) {
        let manifest = fs::read_to_string(self.root.join("nara.toml")).unwrap();
        fs::write(
            self.root.join("nara.toml"),
            manifest.replace(
                "native_document_identity = 'project_location'",
                "native_document_identity = 'stable_metadata'",
            ),
        )
        .unwrap();
    }

    pub fn write_scene_meta(&self, stable_id: StableAssetId) {
        self.write_scene_meta_path("startup.scene.json", stable_id);
    }

    pub fn write_scene_meta_path(&self, path: &str, stable_id: StableAssetId) {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                stable_id,
                AssetPath::new(path).unwrap(),
                AssetSourceKind::Scene,
            ),
            (),
        );
        fs::write(
            self.root.join("scenes/startup.scene.json.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }

    pub fn write_prefab_meta(&self, path: &str, stable_id: StableAssetId) {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                stable_id,
                AssetPath::new(path).unwrap(),
                AssetSourceKind::Prefab,
            ),
            (),
        );
        fs::write(
            self.root.join("prefabs").join(format!("{path}.meta")),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }

    pub fn write_scene_bytes(&self, bytes: &[u8]) {
        fs::write(self.root.join("scenes/startup.scene.json"), bytes).unwrap();
    }

    pub fn write_prefab(&self, path: &str, document: &PrefabDocument) {
        fs::write(
            self.root.join("prefabs").join(path),
            document.to_json_string().unwrap(),
        )
        .unwrap();
    }

    pub fn write_prefab_bytes(&self, path: &str, bytes: &[u8]) {
        fs::write(self.root.join("prefabs").join(path), bytes).unwrap();
    }

    pub fn write_player_image_meta(&self, meta: &ImageAssetMeta) {
        fs::write(
            self.root.join("assets/textures/player.png.meta"),
            meta.to_json_string().unwrap(),
        )
        .unwrap();
    }

    pub fn write_tileset_source(
        &self,
        path: &str,
        source: &nara::tilemap::TileSetSource,
        meta: &TileSetAssetMeta,
    ) {
        let source_path = self.root.join("assets").join(path);
        fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        fs::write(&source_path, source.encode_ron_bytes().unwrap()).unwrap();
        fs::write(
            self.root.join("assets").join(format!("{path}.meta")),
            meta.to_json_string().unwrap(),
        )
        .unwrap();
    }

    pub fn write_asset(&self, path: &str, bytes: &[u8], meta: &ImageAssetMeta) {
        let source = self.root.join("assets").join(path);
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, bytes).unwrap();
        fs::write(
            self.root.join("assets").join(format!("{path}.meta")),
            meta.to_json_string().unwrap(),
        )
        .unwrap();
    }

    pub fn path(&self) -> &Path {
        &self.root
    }
}

fn built_in_authoring_schema_bundle() -> &'static [u8] {
    BUILT_IN_AUTHORING_SCHEMA_BUNDLE.get_or_init(|| {
        AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
            .unwrap()
            .to_json_string()
            .unwrap()
            .into_bytes()
            .into_boxed_slice()
    })
}

impl Drop for TestProject {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

pub fn scene_id(value: &str) -> nara::identity::SceneEntityId {
    nara::identity::SceneEntityId::new(value).unwrap()
}

pub fn player_image_meta() -> ImageAssetMeta {
    image_meta(
        "3c7c5be4-fd4e-4b65-b8d4-c671f5982186",
        "textures/player.png",
    )
}

pub fn image_meta(stable_id: &str, path: &str) -> ImageAssetMeta {
    AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(stable_id).unwrap(),
            AssetPath::new(path).unwrap(),
            AssetSourceKind::Image,
        ),
        ImageImportSettings::default(),
    )
}

pub fn tileset_meta(stable_id: &str, path: &str) -> TileSetAssetMeta {
    AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(stable_id).unwrap(),
            AssetPath::new(path).unwrap(),
            AssetSourceKind::Other("tileset".to_string()),
        ),
        (),
    )
}

pub fn valid_png_bytes() -> &'static [u8] {
    include_bytes!("../fixtures/images/valid-rgba-1x1.png")
}

pub fn sprite_value(image: &AssetRef) -> ComponentValue {
    ComponentValue::map([
        (
            "size",
            ComponentValue::map([
                ("x", ComponentValue::f64(16.0).unwrap()),
                ("y", ComponentValue::f64(16.0).unwrap()),
            ]),
        ),
        (
            "material",
            ComponentValue::map([
                ("image", asset_ref_value(image)),
                (
                    "tint",
                    ComponentValue::map([
                        ("r", ComponentValue::f64(1.0).unwrap()),
                        ("g", ComponentValue::f64(1.0).unwrap()),
                        ("b", ComponentValue::f64(1.0).unwrap()),
                        ("a", ComponentValue::f64(1.0).unwrap()),
                    ]),
                ),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
    ])
}

pub fn identity_transform_record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            (
                "translation",
                ComponentValue::map([
                    ("x", ComponentValue::f64(0.0).unwrap()),
                    ("y", ComponentValue::f64(0.0).unwrap()),
                ]),
            ),
            ("rotation", ComponentValue::f64(0.0).unwrap()),
            (
                "scale",
                ComponentValue::map([
                    ("x", ComponentValue::f64(1.0).unwrap()),
                    ("y", ComponentValue::f64(1.0).unwrap()),
                ]),
            ),
        ]),
    )
}

fn asset_ref_value(asset_ref: &AssetRef) -> ComponentValue {
    match asset_ref {
        AssetRef::Path(path) => ComponentValue::map([
            ("kind", ComponentValue::String("path".to_owned())),
            ("value", ComponentValue::String(path.as_str().to_owned())),
        ]),
        AssetRef::StableId(id) => ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_owned())),
            ("value", ComponentValue::String(id.to_string())),
        ]),
    }
}

fn portable_trust() -> TrustMode {
    if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    }
}

fn host_directory(path: &Path, rights: CapabilityRights) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .write(rights == CapabilityRights::ReadWrite)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    {
        let _ = rights;
        File::open(path).unwrap()
    }
}
