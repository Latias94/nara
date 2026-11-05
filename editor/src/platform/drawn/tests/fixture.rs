use std::time::{SystemTime, UNIX_EPOCH};

use nara::{
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetSourceKind, StableAssetId},
    image::ImageImportSettings,
    reflect::{AuthoringSchemaBundle, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
    scene::{PrefabDocument, SceneComponentRecord, SceneDocument, SceneEntityRecord},
};

use super::*;

pub(super) const SCENE: &str = "scenes/startup.scene.json";
pub(super) const PREFAB: &str = "prefabs/ordinary.prefab.json";
pub(super) const PRESERVED: &str = "scenes/preserved.scene.json";
pub(super) const UI: &str = "prefabs/ui.prefab.json";

pub(super) struct Project {
    root: PathBuf,
}

impl Project {
    pub(super) fn new() -> TestResult<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("nara-drawn-native-{}-{nonce}", std::process::id(),));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let project = Self { root };
        for directory in ["assets", "prefabs", "scenes", "schema"] {
            fs::create_dir(project.root.join(directory)).map_err(|error| error.to_string())?;
        }
        project.write("nara.toml", b"schema_version = 1\n[project]\nsource_universe = '24ed5e36-1b37-5fa3-9c42-232066b17916'\nnative_document_identity = 'project_location'\nname = 'Independent Drawn Native Acceptance'\n[paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n[startup]\ndefault_scene = 'startup.scene.json'\n[authoring]\nstate = 'ready'\nschema_bundle = 'schema/authoring.bundle.json'\n[runtime]\npreset = 'local-headless'\n[capabilities]\nrequested = ['runtime-2d', 'runtime-ui']\n[ui.primary_surface]\nlogical_width = 960\nlogical_height = 540\n")?;
        let bundle = AuthoringSchemaBundle::from_trusted_provider_definitions(
            nara::project_host::advanced::built_in_schema_providers(),
        )
        .map_err(|error| error.to_string())?;
        project.write(
            "schema/authoring.bundle.json",
            bundle
                .to_json_string()
                .map_err(|error| error.to_string())?
                .as_bytes(),
        )?;
        let records = ["original", "destination"].map(|id| SceneEntityRecord::new(entity(id)));
        project.write(
            SCENE,
            SceneDocument::from_ordered_roots(records.clone())
                .to_json_string()
                .map_err(|error| error.to_string())?
                .as_bytes(),
        )?;
        project.write(
            PREFAB,
            PrefabDocument::from_ordered_roots(records)
                .to_json_string()
                .map_err(|error| error.to_string())?
                .as_bytes(),
        )?;
        let opaque = SceneEntityRecord::new(entity("opaque")).with_component(
            ComponentTypeId::new("consumer.drawn.UnknownRecord"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "unrecognized",
                        ComponentValue::String("retain opaque payload café".into()),
                    ),
                    (
                        "nested",
                        ComponentValue::map([
                            ("large", ComponentValue::U64(u64::MAX - 7)),
                            (
                                "values",
                                ComponentValue::List(vec![
                                    ComponentValue::Null,
                                    ComponentValue::Bool(true),
                                    ComponentValue::I64(-103),
                                ]),
                            ),
                        ]),
                    ),
                ]),
            ),
        );
        project.write(
            PRESERVED,
            SceneDocument::from_ordered_roots([
                opaque,
                SceneEntityRecord::new(entity("untouched")),
            ])
            .to_json_string()
            .map_err(|error| error.to_string())?
            .as_bytes(),
        )?;
        let ui = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generic-editor/image-import/prefabs/ui.prefab.json");
        fs::copy(ui, project.root.join(UI)).map_err(|error| error.to_string())?;
        fs::create_dir(project.root.join("assets/textures")).map_err(|error| error.to_string())?;
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../tests/fixtures/images/valid-rgba-1x1.png"),
            project.root.join("assets/textures/player.png"),
        )
        .map_err(|error| error.to_string())?;
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("3c7c5be4-fd4e-4b65-b8d4-c671f5982186")
                    .map_err(|error| error.to_string())?,
                AssetPath::new("textures/player.png").map_err(|error| error.to_string())?,
                AssetSourceKind::Image,
            ),
            ImageImportSettings::default(),
        );
        project.write(
            "assets/textures/player.png.meta",
            metadata
                .to_json_string()
                .map_err(|error| error.to_string())?
                .as_bytes(),
        )?;
        Ok(project)
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn read(&self, path: &str) -> TestResult<Vec<u8>> {
        fs::read(self.root.join(path)).map_err(|error| format!("read {path}: {error}"))
    }

    fn write(&self, path: &str, bytes: &[u8]) -> TestResult {
        fs::write(self.root.join(path), bytes).map_err(|error| format!("write {path}: {error}"))
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let cleanup = (|| -> TestResult {
            let temporary = std::env::temp_dir()
                .canonicalize()
                .map_err(|error| error.to_string())?;
            let root = self
                .root
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if !root.starts_with(&temporary)
                || root == temporary
                || !root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("nara-drawn-native-"))
            {
                return Err("the owned temporary project escaped its parent".into());
            }
            fs::remove_dir_all(root).map_err(|error| error.to_string())
        })();
        if let Err(error) = cleanup {
            if std::thread::panicking() {
                eprintln!("drawn native project cleanup: {error}");
            } else {
                panic!("drawn native project cleanup: {error}");
            }
        }
    }
}

pub(super) fn entity(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).expect("the fixture entity ID is bounded")
}
