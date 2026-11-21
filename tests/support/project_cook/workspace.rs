use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};

use nara::{
    asset::{
        AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath, AssetRef,
        AssetSourceKind, StableAssetId,
    },
    fs::RelativePath,
    project::{NativeDocumentIdentity, ProjectManifest},
    project_tooling::host_fs,
    project_workflow::{
        LocalDependencyRelation, NaraDependencySource, ProjectCreationOutcome,
        ProjectCreationRequest, ProjectSlug, ProjectStarter, prepare_desktop_2d_project,
    },
    reflect::{ComponentSchemaVersion, ComponentTypeId, PersistentComponentProvider},
    scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord},
};

use super::game::{self, AuthoredScore};
use crate::content_fixture::{identity_transform_record, image_meta, scene_id, sprite_value};

pub const PACKAGE: &str = "independent-cooked-game";
pub const SELECTED_SCHEMA: &str = "schema/selected.bundle.json";

pub struct Project {
    pub root: PathBuf,
    original_manifest: ProjectManifest,
    startup_metadata: Vec<u8>,
}

impl Project {
    pub fn create(checkout: &Path, owned: &Path) -> Self {
        let games = owned.join("games with spaces");
        fs::create_dir(&games).unwrap();
        let ancestor = host_fs::writable_directory(checkout).unwrap();
        let coordinate = games
            .strip_prefix(checkout)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let relative = RelativePath::new(coordinate).unwrap();
        let parent = ancestor.open_directory(&relative).unwrap();
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            ProjectStarter::Desktop2dV1,
            parent,
            ProjectSlug::new(PACKAGE).unwrap(),
            NaraDependencySource::local(LocalDependencyRelation::new(
                ancestor,
                None,
                Some(relative),
            )),
        ))
        .unwrap();
        let outcome = prepared.publish();
        let ProjectCreationOutcome::Created(receipt) = outcome else {
            panic!("the real starter must publish and admit the independent project: {outcome:?}");
        };
        let root = games.join(PACKAGE).canonicalize().unwrap();
        assert!(
            receipt
                .project_root()
                .same_object(&host_fs::readable_directory(&root).unwrap())
                .unwrap()
        );
        drop(receipt);
        let original_manifest = load_manifest(&root);
        assert_eq!(
            original_manifest.project.native_document_identity,
            NativeDocumentIdentity::StableMetadata
        );
        assert!(!original_manifest.project.source_universe.as_uuid().is_nil());
        let startup_metadata = fs::read(root.join("scenes/startup.scene.json.meta")).unwrap();
        let identity = AssetMetaIdentityCandidate::decode_json_bytes(&startup_metadata).unwrap();
        assert_eq!(identity.identity().source_kind(), &AssetSourceKind::Scene);
        assert!(!identity.identity().stable_id().as_uuid().is_nil());
        for path in [
            "build.rs",
            "src/bin/headless.rs",
            "src/bin/cook.rs",
            "src/bin/player.rs",
        ] {
            assert!(
                root.join(path).is_file(),
                "the published starter owns {path}"
            );
        }
        Self {
            root,
            original_manifest,
            startup_metadata,
        }
    }

    pub fn author(&self) {
        let library = self.root.join("src/lib.rs");
        let source = fs::read_to_string(&library).unwrap();
        let marker = "nara::ProductRecipe::new()";
        assert_eq!(source.matches(marker).count(), 1);
        let source = source.replacen(
            marker,
            "nara::ProductRecipe::new()\n        .add_contribution(cook_acceptance::contribution().expect(\"the independent game schema is valid\"))\n        .expect(\"the independent game contribution is unique\")",
            1,
        );
        fs::write(
            library,
            format!(
                "{source}\npub mod cook_acceptance {{\n{}\n}}\n",
                include_str!("game.rs")
            ),
        )
        .unwrap();
        fs::create_dir(self.root.join("assets/textures")).unwrap();
        for (index, (name, value, rgba)) in [
            ("startup", 17, [17, 39, 91, 255]),
            ("second", 43, [47, 151, 23, 255]),
            ("third", 97, [201, 73, 119, 255]),
        ]
        .into_iter()
        .enumerate()
        {
            let path = format!("textures/{name}.png");
            let width = index as u32 + 1;
            fs::write(self.root.join("assets").join(&path), png(width, rgba)).unwrap();
            fs::write(
                self.root.join("assets").join(format!("{path}.meta")),
                image_meta(game::IMAGE_IDS[index], &path)
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
            let record = SceneEntityRecord::new(scene_id("cook-probe"))
                .with_component(
                    ComponentTypeId::new("nara.transform.Transform2d"),
                    identity_transform_record(),
                )
                .with_component(
                    ComponentTypeId::new("nara.sprite.Sprite"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        sprite_value(&AssetRef::path(path).unwrap()),
                    ),
                )
                .with_component(
                    ComponentTypeId::new("independent.cook.AuthoredScore"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        AuthoredScore { value }
                            .__encode_persistent_component()
                            .unwrap(),
                    ),
                );
            let scene_path = format!("{name}.scene.json");
            fs::write(
                self.root.join("scenes").join(&scene_path),
                SceneDocument::from_ordered_roots([record])
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
            if index != 0 {
                let id = [game::SECOND_SCENE, game::THIRD_SCENE][index - 1];
                let metadata = AssetMeta::new(
                    AssetMetaIdentity::new(
                        StableAssetId::parse_str(id).unwrap(),
                        AssetPath::new(&scene_path).unwrap(),
                        AssetSourceKind::Scene,
                    ),
                    (),
                );
                fs::write(
                    self.root.join("scenes").join(format!("{scene_path}.meta")),
                    metadata.to_json_string().unwrap(),
                )
                .unwrap();
            }
        }
        let manifest_path = self.root.join("nara.toml");
        let mut manifest: toml::Value =
            toml::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
        manifest["authoring"]["schema_bundle"] = SELECTED_SCHEMA.into();
        manifest.as_table_mut().unwrap().insert(
            "content".to_owned(),
            toml::Value::Table(toml::map::Map::from_iter([(
                "scenes".to_owned(),
                toml::Value::Array(vec!["second.scene.json".into(), "third.scene.json".into()]),
            )])),
        );
        fs::write(manifest_path, toml::to_string_pretty(&manifest).unwrap()).unwrap();
        fs::rename(
            self.root.join("schema/authoring.bundle.json"),
            self.root.join(SELECTED_SCHEMA),
        )
        .unwrap();
        self.assert_identity();
    }

    pub fn assert_identity(&self) {
        let manifest = load_manifest(&self.root);
        assert_eq!(
            manifest.project.source_universe,
            self.original_manifest.project.source_universe
        );
        assert_eq!(
            manifest.project.native_document_identity,
            NativeDocumentIdentity::StableMetadata
        );
        assert_eq!(
            fs::read(self.root.join("scenes/startup.scene.json.meta")).unwrap(),
            self.startup_metadata
        );
    }
}

fn load_manifest(root: &Path) -> ProjectManifest {
    let load = ProjectManifest::parse_toml_bytes(&fs::read(root.join("nara.toml")).unwrap());
    assert!(!load.has_errors());
    load.manifest.unwrap()
}

fn png(width: u32, rgba: [u8; 4]) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, 1, image::Rgba(rgba));
    let mut encoded = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    encoded.into_inner()
}
