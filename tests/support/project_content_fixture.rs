#![allow(dead_code)]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    asset::{AssetMeta, AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    project_host::{
        ProjectSettingsCandidate, RuntimePlan, built_in_schema_providers, ingest_project_manifest,
        project_runtime_plugins, resolve_runtime_plan,
    },
    reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{
        PrefabDocument, PrefabInstance, SceneComponentRecord, SceneDocument, SceneEntityRecord,
    },
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
        fs::write(
            root.join("nara.toml"),
            r#"schema_version = 1

[project]
name = "Content Test"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[capabilities]
requested = ["runtime-2d"]
"#,
        )
        .unwrap();

        let source = AssetRef::path("enemy.prefab.json").unwrap();
        let mut anchor = SceneEntityRecord::new(scene_id("enemy-anchor"));
        anchor.prefab = Some(PrefabInstance {
            source,
            overrides: nara::scene::ScenePatchDocument::default(),
        });
        let scene = SceneDocument::new([anchor]);
        let image = AssetRef::path("textures/player.png").unwrap();
        let enemy = SceneEntityRecord::new(scene_id("enemy")).with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&image)),
        );
        let prefab = PrefabDocument::new([enemy]);
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
            host_directory(&self.root),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
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
        let plan = resolve_runtime_plan(
            &candidate,
            project_runtime_plugins(&candidate),
            built_in_schema_providers(),
        )
        .unwrap();
        (candidate, plan, root)
    }

    pub fn write_prefab_source(&self, document: &PrefabDocument) {
        fs::write(
            self.root.join("prefabs/enemy.prefab.json"),
            document.to_json_string().unwrap(),
        )
        .unwrap();
    }

    pub fn select_local_headless_profile(&self) {
        fs::write(
            self.root.join("nara.toml"),
            r#"schema_version = 1

[project]
name = "Runtime Test"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

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

    pub fn write_player_image_meta(&self, meta: &AssetMeta) {
        fs::write(
            self.root.join("assets/textures/player.png.meta"),
            meta.to_json_string().unwrap(),
        )
        .unwrap();
    }

    pub fn write_asset(&self, path: &str, bytes: &[u8], meta: &AssetMeta) {
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

impl Drop for TestProject {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

pub fn scene_id(value: &str) -> nara::identity::SceneEntityId {
    nara::identity::SceneEntityId::new(value).unwrap()
}

pub fn player_image_meta() -> AssetMeta {
    image_meta(
        "3c7c5be4-fd4e-4b65-b8d4-c671f5982186",
        "textures/player.png",
    )
}

pub fn image_meta(stable_id: &str, path: &str) -> AssetMeta {
    AssetMeta::new(
        StableAssetId::parse_str(stable_id).unwrap(),
        AssetPath::new(path).unwrap(),
        AssetSourceKind::Image,
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

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    {
        File::open(path).unwrap()
    }
}
