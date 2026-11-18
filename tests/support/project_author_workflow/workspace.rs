use std::{
    fs,
    path::{Path, PathBuf},
};

use nara::{
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetSourceKind, StableAssetId},
    fs::RelativePath,
    project::{NativeDocumentIdentity, ProjectManifest},
    project_tooling::host_fs,
    project_workflow::{
        LocalDependencyRelation, NaraDependencySource, ProjectCreationOutcome,
        ProjectCreationRequest, ProjectSlug, prepare_desktop_2d_project,
    },
};

pub const PACKAGE: &str = "collect-game";
const TEMPLATE_README: &str = include_str!("../../../starters/desktop-2d/v1/README.md");
const GUIDE: &str = include_str!("../../../docs/guides/first-2d-game.md");

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
        let project_root = parent.join(PACKAGE);
        let ancestor = checkout.parent().unwrap();
        let relative = RelativePath::new(
            parent
                .strip_prefix(ancestor)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/"),
        )
        .unwrap();
        let shared = host_fs::writable_directory(ancestor).unwrap();
        let parent = shared.open_directory(&relative).unwrap();
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            parent,
            ProjectSlug::new(PACKAGE).unwrap(),
            NaraDependencySource::local(LocalDependencyRelation::new(
                shared,
                Some(RelativePath::new(checkout.file_name().unwrap()).unwrap()),
                Some(relative),
            )),
        ))
        .unwrap();
        let ProjectCreationOutcome::Created(receipt) = prepared.publish() else {
            panic!("the original creation owner must publish each new game");
        };
        let root = project_root.canonicalize().unwrap();
        assert!(
            receipt
                .project_root()
                .same_object(&host_fs::readable_directory(&root).unwrap())
                .unwrap()
        );
        drop(receipt);
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

    pub fn add_first_system(&self) {
        let readme = TEMPLATE_README.replace("\r\n", "\n");
        let snippet = readme
            .split_once("### Your first fixed-step system")
            .unwrap()
            .1
            .split_once("```rust\n")
            .unwrap()
            .1
            .split_once("\n```")
            .unwrap()
            .0;
        fs::write(self.root.join("src/first_step.rs"), snippet).unwrap();
        let library = self.root.join("src/lib.rs");
        let source = fs::read_to_string(&library).unwrap().replace("\r\n", "\n");
        let marker = ".expect(\"the starter Image runtime codec definition is valid\")";
        assert_eq!(source.matches(marker).count(), 1);
        let source = source.replacen(marker, &format!("{marker}\n        .add_plugin::<first_step::FirstStepPlugin>()\n        .expect(\"the first-step plugin is unique\")"), 1);
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
        plugin_cargo["dependencies"]["nara"]["path"] = format!("../{engine}").into();
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
        let guide = GUIDE.replace("\r\n", "\n");
        let guide_recipe = guide
            .split_once("### Keep one product recipe")
            .unwrap()
            .1
            .split_once("```rust\n")
            .unwrap()
            .1
            .split_once("\n```")
            .unwrap()
            .0;
        let library = self.root.join("src/lib.rs");
        let source = fs::read_to_string(&library).unwrap().replace("\r\n", "\n");
        let start = source.find("pub fn product_recipe()").unwrap();
        let end = start + source[start..].find("\n}\n").unwrap() + 2;
        fs::write(
            library,
            format!("{}{}{}", &source[..start], guide_recipe, &source[end..]),
        )
        .unwrap();

        let cooker = self.root.join("src/bin/cook.rs");
        let source = fs::read_to_string(&cooker).unwrap();
        let marker = "project_rust_input_inventory(&project)?,";
        assert_eq!(source.matches(marker).count(), 1);
        let replacement = guide
            .split_once("### Declare the local plugin's Cook inputs")
            .unwrap()
            .1
            .split_once("```rust,ignore\n")
            .unwrap()
            .1
            .split_once("\n```")
            .unwrap()
            .0;
        fs::write(cooker, source.replacen(marker, replacement, 1)).unwrap();
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

fn read_toml(path: &Path) -> toml::Value {
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
