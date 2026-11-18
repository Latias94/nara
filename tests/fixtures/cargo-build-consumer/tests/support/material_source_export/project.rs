use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use engine::{
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    fs::RelativePath,
    image::ImageImportSettings,
    material::{Material2dDomain, Material2dRecord, Material2dSource},
    project_host::{ingest_project_manifest, resolve_product_recipe},
    project_tooling::host_fs,
    reflect::{
        ComponentTypeId, ComponentValue, PersistentComponentSchemaOwnerId,
        encode_asset_reference_value,
    },
    scene::{SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord},
};

use super::{library, workspace::Tools};

const PROJECT_MANIFEST: &str = r#"schema_version = 1

[project]
name = "Palette source export"
source_universe = "744cab2d-4c3c-43f0-bd13-8d72a8c698b0"
native_document_identity = "stable_metadata"
stable_id = "palette-source-export-game"
version = "0.1.0"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[authoring]
state = "ready"
schema_bundle = "schema/authoring.bundle.json"

[startup]
default_scene = "startup.scene.json"

[runtime]
preset = "local-headless"
fixed_timestep_seconds = 0.02

[window]
enabled = false

[capabilities]
requested = ["runtime-2d"]

[tasks.io]
workers = 1
pending_capacity = 16

[tasks.compute]
workers = 1
pending_capacity = 16

[tasks.async_compute]
workers = 1
pending_capacity = 16

[profiles.desktop.capabilities]
requested = ["runtime-2d", "presentation-2d", "desktop-winit", "render-wgpu"]

[profiles.desktop.execution]
contract = "desktop-supervised-play/v1"

[profiles.desktop.window]
enabled = true
title = "Palette source export acceptance"
width = 320
height = 240
present_mode = "fifo"
resizable = false
"#;

pub const SOURCE_FILES: &[&str] = &[
    "nara.toml",
    "schema/authoring.bundle.json",
    "scenes/startup.scene.json",
    "scenes/startup.scene.json.meta",
    "assets/palette.material2d.json",
    "assets/palette.material2d.json.meta",
    "assets/palette.wgsl",
    "assets/indices.png",
    "assets/indices.png.meta",
    "assets/palette.png",
    "assets/palette.png.meta",
];

pub struct Project {
    pub root: PathBuf,
    sources: Vec<(&'static str, Vec<u8>)>,
}

impl Project {
    pub fn new(parent: &Path, tools: &Tools) -> Self {
        let root = parent.join("ordinary-palette-project");
        for directory in ["assets", "scenes", "prefabs", "schema", "src/bin"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        let repository = serde_json::to_string(tools.repository.to_str().unwrap()).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            format!(
                r#"[package]
name = "palette-source-export-game"
version = "0.1.0"
edition = "2024"
rust-version = "1.95"
publish = false

[package.metadata.nara]
project-tool-protocol = 1

[workspace]
resolver = "3"

[dependencies]
engine = {{ package = "nara", path = {repository}, default-features = false, features = ["runtime-2d", "serde", "project-tooling"] }}

[features]
default = []
desktop = ["engine/presentation-2d", "engine/desktop-winit", "engine/render-wgpu"]

[lib]
name = "nara_game"
path = "src/lib.rs"

[[bin]]
name = "headless"
path = "src/bin/headless.rs"

[[bin]]
name = "desktop"
path = "src/bin/desktop.rs"
required-features = ["desktop"]
"#,
            ),
        )
        .unwrap();
        fs::write(root.join("src/lib.rs"), include_str!("library.rs")).unwrap();
        for (name, source) in [
            (
                "headless",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../starters/desktop-2d/v1/src/bin/headless.rs"
                )),
            ),
            (
                "desktop",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../../starters/desktop-2d/v1/src/bin/desktop.rs"
                )),
            ),
        ] {
            fs::write(
                root.join(format!("src/bin/{name}.rs")),
                source.replace("nara::", "engine::"),
            )
            .unwrap();
        }
        fs::write(root.join("nara.toml"), PROJECT_MANIFEST).unwrap();
        let capability = host_fs::readable_directory(&root).unwrap();
        let manifest = capability
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, library::product_recipe().unwrap()).unwrap();
        let bundle = plan.schema_validation().authoring_schema_bundle();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        let bindings = plan.schema_validation().bindings();
        let component = |id: &str, value: Option<ComponentValue>| {
            let id = ComponentTypeId::new(id);
            let schema = bindings.schema(&id).unwrap();
            let owner = PersistentComponentSchemaOwnerId::try_new(
                bindings
                    .native_snapshot()
                    .component_schema_owner(&id)
                    .unwrap()
                    .as_str(),
            )
            .unwrap();
            SceneComponentRecord::new(
                schema.version(),
                value.unwrap_or_else(|| schema.authoring_initial_value().unwrap().clone()),
            )
            .with_owner(owner)
        };
        let mut sprite = bindings
            .schema(&ComponentTypeId::new("nara.sprite.Sprite"))
            .unwrap()
            .authoring_initial_value()
            .unwrap()
            .clone();
        let ComponentValue::Map(fields) = &mut sprite else {
            panic!("Sprite value must be a map");
        };
        let Some(ComponentValue::Map(material)) = fields.get_mut("material") else {
            panic!("Sprite must have its stock material");
        };
        material.insert(
            "image".to_owned(),
            encode_asset_reference_value(&AssetRef::Product(library::primary(
                library::INDICES_SOURCE,
            ))),
        );
        let record = Material2dRecord::new(
            AssetRef::Product(library::primary(library::MATERIAL_SOURCE)),
            &library::declaration(),
            library::parameters(1.0),
            BTreeMap::new(),
        )
        .unwrap();
        let material = component(
            "nara.material2d",
            Some(record.to_component_value().unwrap()),
        );
        let camera = ComponentValue::map([
            ("viewport_height", ComponentValue::f64(720.0).unwrap()),
            (
                "clear_color",
                ComponentValue::map([
                    ("r", ComponentValue::f64(0.06).unwrap()),
                    ("g", ComponentValue::f64(0.1).unwrap()),
                    ("b", ComponentValue::f64(0.18).unwrap()),
                    ("a", ComponentValue::f64(1.0).unwrap()),
                ]),
            ),
        ]);
        let scene = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(SceneEntityId::new("main-camera").unwrap())
                .with_component(
                    ComponentTypeId::new("nara.render.Camera2d"),
                    component("nara.render.Camera2d", Some(camera)),
                )
                .with_component(
                    ComponentTypeId::new("nara.transform.Transform2d"),
                    component("nara.transform.Transform2d", None),
                ),
            SceneEntityRecord::new(SceneEntityId::new("palette-sprite").unwrap())
                .with_component(
                    ComponentTypeId::new("nara.sprite.Sprite"),
                    component("nara.sprite.Sprite", Some(sprite)),
                )
                .with_component(
                    ComponentTypeId::new("nara.transform.Transform2d"),
                    component("nara.transform.Transform2d", None),
                )
                .with_component(ComponentTypeId::new("nara.material2d"), material),
        ]);
        let scene = scene.to_json_string().unwrap();
        assert!(!scene.contains(library::PALETTE_SOURCE));
        assert!(!scene.contains("palette.png"));
        fs::write(root.join("scenes/startup.scene.json"), scene).unwrap();
        let scene_meta = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("744cab2d-4c3c-43f0-bd13-8d72a8c698b4").unwrap(),
                AssetPath::new("startup.scene.json").unwrap(),
                AssetSourceKind::Scene,
            ),
            (),
        );
        fs::write(
            root.join("scenes/startup.scene.json.meta"),
            scene_meta.to_json_string().unwrap(),
        )
        .unwrap();
        let source = Material2dSource::new(
            library::declaration(),
            vec![AssetPath::new("palette.wgsl").unwrap()],
            library::parameters(0.0),
            BTreeMap::from([(
                "palette".to_owned(),
                AssetRef::Product(library::primary(library::PALETTE_SOURCE)),
            )]),
        )
        .unwrap();
        fs::write(
            root.join("assets/palette.material2d.json"),
            source.encode().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("assets/palette.wgsl"),
            include_str!("palette.wgsl"),
        )
        .unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                library::primary(library::MATERIAL_SOURCE).source(),
                AssetPath::new("palette.material2d.json").unwrap(),
                AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
            ),
            (),
        );
        fs::write(
            root.join("assets/palette.material2d.json.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
        let png = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../images/valid-rgba-1x1.png"
        ));
        for (name, identity) in [
            ("indices.png", library::INDICES_SOURCE),
            ("palette.png", library::PALETTE_SOURCE),
        ] {
            fs::write(root.join("assets").join(name), png).unwrap();
            let metadata = AssetMeta::new(
                AssetMetaIdentity::new(
                    library::primary(identity).source(),
                    AssetPath::new(name).unwrap(),
                    AssetSourceKind::Image,
                ),
                ImageImportSettings::default(),
            );
            fs::write(
                root.join("assets").join(format!("{name}.meta")),
                metadata.to_json_string().unwrap(),
            )
            .unwrap();
        }
        let sources = SOURCE_FILES
            .iter()
            .map(|path| (*path, fs::read(root.join(path)).unwrap()))
            .collect();
        Self { root, sources }
    }

    pub fn assert_published(&self, published: &Path) {
        for (path, bytes) in &self.sources {
            assert_eq!(
                &fs::read(published.join("project").join(path)).unwrap(),
                bytes,
                "{path}",
            );
        }
        for target in ["headless", "desktop"] {
            assert!(
                published
                    .join("bin")
                    .join(format!("{target}{}", std::env::consts::EXE_SUFFIX))
                    .is_file(),
            );
        }
        for path in ["Cargo.toml", "Cargo.lock", "src", "target", ".cargo"] {
            assert!(!published.join("project").join(path).exists(), "{path}");
        }
    }
}
