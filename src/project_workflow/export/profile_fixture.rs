use std::{fs, path::Path};

use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId,
};
use nara_reflect::{
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, encode_asset_reference_value,
};
use nara_scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord};

use crate::{
    ProductRecipe,
    fs::RelativePath,
    project::ProjectPath,
    project_host::{ProjectContentLoader, ingest_project_manifest, resolve_product_recipe},
    project_tooling::host_fs,
    project_workflow::ProjectExportBinaryTarget,
};

const LATER_SCENE: &str = "b.scene.json";
const FLAG: &str = "flag.png";
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
pub(super) const FLAG_ID: &str = "f12228c8-740f-4a2c-9ec1-bac91b72bf11";

pub(super) fn install_profiled_level(project: &Path) {
    let manifest_path = project.join("nara.toml");
    let mut manifest: toml::Value =
        toml::from_str(&fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest.as_table_mut().unwrap().insert(
        "content".to_owned(),
        toml::Value::Table(toml::Table::from_iter([(
            "scenes".to_owned(),
            toml::Value::Array(vec![LATER_SCENE.into()]),
        )])),
    );
    manifest["profiles"]["desktop"]
        .as_table_mut()
        .unwrap()
        .insert(
            "paths".to_owned(),
            toml::Value::Table(toml::Table::from_iter([
                ("assets".to_owned(), "prefabs".into()),
                ("prefabs".to_owned(), "assets".into()),
            ])),
        );
    fs::write(manifest_path, toml::to_string_pretty(&manifest).unwrap()).unwrap();

    write_flag(&project.join("assets"), FLAG_ID, RED);
    write_flag(
        &project.join("prefabs"),
        "f12228c8-740f-4a2c-9ec1-bac91b72bf12",
        BLUE,
    );
    write_flag_scene(&project.join("scenes"), AssetRef::path(FLAG).unwrap());
}

pub(super) fn write_flag(directory: &Path, stable_id: &str, pixels: [u8; 4]) {
    image::RgbaImage::from_pixel(1, 1, image::Rgba(pixels))
        .save(directory.join(FLAG))
        .unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(stable_id).unwrap(),
            AssetPath::new(FLAG).unwrap(),
            AssetSourceKind::Image,
        ),
        nara_render::image::ImageImportSettings::default(),
    );
    fs::write(
        directory.join(format!("{FLAG}.meta")),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
}

pub(super) fn write_flag_scene(scenes: &Path, reference: AssetRef) {
    let number = |value| ComponentValue::f64(value).unwrap();
    let vector = |value| ComponentValue::map([("x", number(value)), ("y", number(value))]);
    let transform = ComponentValue::map([
        ("translation", vector(0.0)),
        ("rotation", number(0.0)),
        ("scale", vector(1.0)),
    ]);
    let sprite = ComponentValue::map([
        ("size", vector(16.0)),
        (
            "material",
            ComponentValue::map([
                ("image", encode_asset_reference_value(&reference)),
                (
                    "tint",
                    ComponentValue::map(["r", "g", "b", "a"].map(|name| (name, number(1.0)))),
                ),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
    ]);
    let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        nara_ecs::identity::SceneEntityId::new("flag").unwrap(),
    )
    .with_component(
        ComponentTypeId::new("nara.transform.Transform2d"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, transform),
    )
    .with_component(
        ComponentTypeId::new("nara.sprite.Sprite"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite),
    )]);
    fs::write(scenes.join(LATER_SCENE), scene.to_json_string().unwrap()).unwrap();
    write_scene_sidecar(scenes, LATER_SCENE, "f12228c8-740f-4a2c-9ec1-bac91b72bf13");
}

pub(super) fn write_scene_sidecar(scenes: &Path, scene: &str, stable_id: &str) {
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(stable_id).unwrap(),
            AssetPath::new(scene).unwrap(),
            AssetSourceKind::Scene,
        ),
        (),
    );
    fs::write(
        scenes.join(format!("{scene}.meta")),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
}

pub(super) fn assert_profiled_level_images(project: &Path) {
    let root = host_fs::readable_directory(project).unwrap();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let loader = ProjectContentLoader::new(root).unwrap();
    let scene = ProjectPath::new(LATER_SCENE).unwrap();
    for (target, expected) in ProjectExportBinaryTarget::ALL.into_iter().zip([RED, BLUE]) {
        let candidate = ingest_project_manifest(&manifest, target.content_profile()).unwrap();
        let plan = resolve_product_recipe(&candidate, ProductRecipe::new()).unwrap();
        let snapshot = loader.load_scene(&candidate, &plan, &scene).unwrap();
        assert_eq!(snapshot.images().len(), 1, "{target:?}");
        assert_eq!(
            snapshot.images()[0].image().pixels(),
            &expected,
            "{target:?}"
        );
    }
}
