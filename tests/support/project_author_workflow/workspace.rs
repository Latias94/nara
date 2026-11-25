use std::{
    fs,
    path::{Path, PathBuf},
};

use nara::{
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetSourceKind, StableAssetId},
    project::{NativeDocumentIdentity, ProjectManifest},
};

use crate::authored_project::{AuthoredFixture, create_authored_project};

const PROJECT_SLUG: &str = "collect-game";

pub struct Project {
    pub root: PathBuf,
    pub manifest: ProjectManifest,
    pub startup: Vec<u8>,
    pub prefab: StableAssetId,
    manifest_bytes: Vec<u8>,
    generated_cargo: toml::Value,
    generated_entries: Vec<(&'static str, Vec<u8>)>,
}

impl Project {
    pub fn create(checkout: &Path, parent: &Path) -> Self {
        let root = create_authored_project(checkout, parent, PROJECT_SLUG, AuthoredFixture::Basic);
        assert!(!root.starts_with(checkout));
        let generated_cargo = read_toml(&root.join("Cargo.toml"));
        let generated_entries = ["build.rs", "src/bin/cook.rs", "src/bin/player.rs"]
            .map(|name| (name, fs::read(root.join(name)).unwrap()))
            .to_vec();
        let manifest = manifest(&root);
        let manifest_bytes = fs::read(root.join("nara.toml")).unwrap();
        assert_eq!(
            manifest.project.native_document_identity,
            NativeDocumentIdentity::StableMetadata
        );
        let startup = fs::read(root.join("scenes/startup.scene.json.meta")).unwrap();
        Self {
            root,
            manifest,
            startup,
            manifest_bytes,
            generated_cargo,
            generated_entries,
            prefab: StableAssetId::try_random().unwrap(),
        }
    }

    pub fn cargo_package_name(&self) -> &str {
        self.generated_cargo["package"]["name"].as_str().unwrap()
    }

    pub fn add_first_system(&self) {
        fs::write(
            self.root.join("src/first_step.rs"),
            include_bytes!("../authored_project/first_step.rs"),
        )
        .unwrap();
        let library = self.root.join("src/lib.rs");
        let source = fs::read_to_string(&library).unwrap();
        let source = source.replacen(
            "nara::ProductRecipe::new()",
            "nara::ProductRecipe::new()\n        .add_plugin::<first_step::FirstStepPlugin>()?",
            1,
        );
        fs::write(library, format!("mod first_step;\n{source}")).unwrap();
        self.assert_identity();
    }

    pub fn add_collection(&self, checkout: &Path) {
        let sample = checkout.join("samples/collect-game");
        for name in [
            "src/bin/headless.rs",
            "src/bin/desktop.rs",
            "gameplay-plugin/Cargo.toml",
            "gameplay-plugin/src/lib.rs",
            "gameplay-plugin/src/input.rs",
            "gameplay-plugin/src/walking.rs",
            "gameplay-plugin/src/inventory_tests.rs",
            "gameplay-plugin/schema/component-schema-generation-1.json",
            "scenes/startup.scene.json",
            "prefabs/player.prefab.json",
            "assets/textures/token.png",
            "assets/textures/token.png.meta",
            "variants/fast-player/player.prefab.json",
            "tests/gameplay.rs",
        ] {
            let destination = self.root.join(name);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::copy(sample.join(name), destination).unwrap();
        }
        let cargo_path = self.root.join("Cargo.toml");
        let mut cargo = read_toml(&cargo_path);
        cargo["workspace"].as_table_mut().unwrap().insert(
            "members".into(),
            toml::Value::Array(vec!["gameplay-plugin".into()]),
        );
        cargo["dependencies"].as_table_mut().unwrap().insert(
            "collect-gameplay".into(),
            toml::Value::Table(toml::map::Map::from_iter([(
                "path".into(),
                "gameplay-plugin".into(),
            )])),
        );
        cargo["features"]["desktop"]
            .as_array_mut()
            .unwrap()
            .push("collect-gameplay/desktop".into());
        fs::write(&cargo_path, toml::to_string_pretty(&cargo).unwrap()).unwrap();
        let engine = self.generated_cargo["dependencies"]["nara"]["path"]
            .as_str()
            .unwrap();
        let plugin = self.root.join("gameplay-plugin/Cargo.toml");
        let mut plugin_cargo = read_toml(&plugin);
        plugin_cargo["dependencies"]["nara"]["path"] = engine.into();
        fs::write(plugin, toml::to_string_pretty(&plugin_cargo).unwrap()).unwrap();

        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                self.prefab,
                AssetPath::new("player.prefab.json").unwrap(),
                AssetSourceKind::Prefab,
            ),
            serde_json::Map::<String, serde_json::Value>::new(),
        );
        fs::write(
            self.root.join("prefabs/player.prefab.json.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
        let recipe = include_str!("../authored_project/collect_recipe.rs");
        let library = self.root.join("src/lib.rs");
        let source = fs::read_to_string(&library).unwrap().replace("\r\n", "\n");
        let start = source.find("pub fn product_recipe()").unwrap();
        let end = start + source[start..].find("\n}\n").unwrap() + 2;
        fs::write(
            library,
            format!(
                "{}{}{}",
                &source[..start],
                recipe.trim_end(),
                &source[end..]
            ),
        )
        .unwrap();

        let cooker = self.root.join("src/bin/cook.rs");
        let source = fs::read_to_string(&cooker).unwrap();
        let marker = "project_rust_input_inventory(&project)?,";
        assert_eq!(source.matches(marker).count(), 1);
        let inputs = include_str!("../authored_project/collect_cook_inputs.rs.in");
        fs::write(cooker, source.replacen(marker, inputs.trim_end(), 1)).unwrap();
        self.assert_identity();
        self.assert_generated_entries();
    }

    pub fn assert_identity(&self) {
        let current = manifest(&self.root);
        assert_eq!(
            current.project.source_universe,
            self.manifest.project.source_universe
        );
        assert_eq!(
            current.project.native_document_identity,
            NativeDocumentIdentity::StableMetadata
        );
        assert_eq!(current.project.name, self.manifest.project.name);
        assert_eq!(current.project.stable_id, self.manifest.project.stable_id);
        assert_eq!(
            fs::read(self.root.join("scenes/startup.scene.json.meta")).unwrap(),
            self.startup
        );
        assert_eq!(
            fs::read(self.root.join("nara.toml")).unwrap(),
            self.manifest_bytes
        );
    }

    pub fn authored_bytes(&self) -> Vec<(&'static str, Vec<u8>)> {
        [
            "nara.toml",
            "scenes/startup.scene.json",
            "scenes/startup.scene.json.meta",
            "prefabs/player.prefab.json",
            "prefabs/player.prefab.json.meta",
            "assets/textures/token.png",
            "assets/textures/token.png.meta",
        ]
        .map(|name| (name, fs::read(self.root.join(name)).unwrap()))
        .to_vec()
    }

    fn assert_generated_entries(&self) {
        let current = read_toml(&self.root.join("Cargo.toml"));
        assert_eq!(
            current["dependencies"]["nara"],
            self.generated_cargo["dependencies"]["nara"]
        );
        assert_eq!(current["bin"], self.generated_cargo["bin"]);
        for (name, bytes) in &self.generated_entries {
            if *name == "src/bin/cook.rs" {
                continue;
            }
            assert_eq!(fs::read(self.root.join(name)).unwrap(), *bytes);
        }
    }
}

pub(super) fn read_toml(path: &Path) -> toml::Value {
    read_toml_bytes(&fs::read(path).unwrap())
}

fn read_toml_bytes(bytes: &[u8]) -> toml::Value {
    toml::from_str(std::str::from_utf8(bytes).unwrap()).unwrap()
}

fn manifest(root: &Path) -> ProjectManifest {
    let load = ProjectManifest::parse_toml_bytes(&fs::read(root.join("nara.toml")).unwrap());
    assert!(!load.has_errors());
    load.manifest.unwrap()
}
