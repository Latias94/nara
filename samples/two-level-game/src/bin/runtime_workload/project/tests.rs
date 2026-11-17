use std::collections::BTreeSet;

use nara::{
    asset::AssetMetaCandidate,
    image::ImageExtent,
    scene::{PrefabInstanceBinding, SceneEntityPlacement},
    tilemap::TileSetSourceFileLimits,
};
use two_level_game::levels::WALL_ENTITY_IDS;

use super::*;

#[test]
fn populations_preserve_sources_and_fit_default_file_limits_at_one_world_footprint() {
    let original = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (population, side, tile_size) in [(64, 8, 32), (4096, 64, 4), (16384, 128, 2)] {
        let mut project = Project::new(population).unwrap();
        let report = project.source_report();
        assert_eq!(report["population"], population);
        assert_eq!(report["authored_scene_members"], AUTHORED_LEVEL_MEMBERS);
        assert_eq!(
            report["tile_size"],
            json!([tile_size as f64, tile_size as f64])
        );
        assert_eq!(
            report["world_bounds"],
            json!({"min": [-120, -128], "max": [136, 128]})
        );
        assert_eq!(report["bounds"]["scene_file_bytes"], 16 * 1024 * 1024);
        assert_eq!(report["bounds"]["shape_nodes"], 1_000_000);
        for relative in [
            "input/explorer.actions.json",
            "assets/animations/explorer.sprite-animations.json",
            "assets/animations/explorer.sprite-animations.json.meta",
        ] {
            let expected = file_report(relative, &fs::read(original.join(relative)).unwrap());
            for inventory in ["copied_source_files", "source_files"] {
                assert_eq!(
                    report[inventory]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|file| file["path"] == relative)
                        .collect::<Vec<_>>(),
                    [&expected],
                    "copied source inventory: {inventory} {relative}",
                );
            }
        }
        for file in report["copied_source_files"].as_array().unwrap() {
            let relative = file["path"].as_str().unwrap();
            let bytes = fs::read(original.join(relative)).unwrap();
            assert_eq!(
                *file,
                file_report(relative, &bytes),
                "original changed: {relative}"
            );
            if !relative.ends_with(".scene.json") {
                assert_eq!(fs::read(project.root().join(relative)).unwrap(), bytes);
            }
        }
        for file in report["source_files"].as_array().unwrap() {
            let relative = file["path"].as_str().unwrap();
            assert_eq!(
                *file,
                file_report(relative, &fs::read(project.root().join(relative)).unwrap()),
                "report must describe actual written source: {relative}",
            );
        }
        for (level, stable_id) in LEVELS {
            let relative = format!("scenes/{level}.scene.json");
            let bytes = fs::read(project.root().join(&relative)).unwrap();
            let document = SceneDocumentCandidate::decode_json_bytes_with_limits(
                &bytes,
                SceneFileLimits::default(),
            )
            .unwrap()
            .into_composition_source()
            .unwrap();
            assert_eq!(document.entities.len(), AUTHORED_LEVEL_MEMBERS);
            assert_eq!(document.placements.len(), AUTHORED_LEVEL_MEMBERS);
            let mut expected_ids = [
                "camera",
                "explorer",
                "checkpoint",
                "gem-near",
                "gem-far",
                "portal",
            ]
            .into_iter()
            .chain(WALL_ENTITY_IDS)
            .collect::<Vec<_>>();
            assert_eq!(
                document.placements,
                expected_ids
                    .iter()
                    .map(|id| SceneEntityPlacement::root(SceneEntityId::new(*id).unwrap()))
                    .collect::<Vec<_>>()
            );
            expected_ids.sort_unstable();
            assert_eq!(
                document
                    .entities
                    .iter()
                    .map(|entity| entity.id.as_str())
                    .collect::<Vec<_>>(),
                expected_ids
            );
            let checkpoint = document
                .entities
                .iter()
                .find(|entity| entity.id.as_str() == "checkpoint")
                .unwrap();
            let portal = document
                .entities
                .iter()
                .find(|entity| entity.id.as_str() == "portal")
                .unwrap();
            let tileset_path = format!("tilesets/{level}.tileset.ron");
            let mut covered = BTreeSet::new();
            inspect_map(
                &checkpoint.components[&ComponentTypeId::new(TILEMAP)],
                -120,
                tile_size,
                &tileset_path,
                &mut covered,
            );
            if population == 16384 {
                let alias = format!("workload/{level}-half.prefab.json");
                let instance = portal.prefab.as_ref().unwrap();
                assert_eq!(instance.source, AssetRef::path(&alias).unwrap());
                assert_eq!(
                    instance.binding,
                    PrefabInstanceBinding::Root {
                        base: SourceDocumentKey::project_location(
                            scene_universe(project.root()).unwrap(),
                            AssetSourcePath::new(format!("prefabs/{alias}")).unwrap(),
                            SourceDocumentKind::Prefab,
                        ),
                        root: PrefabEntityTarget::source(SceneEntityId::new(PREFAB_ROOT).unwrap()),
                    },
                );
                let prefab = PrefabDocumentCandidate::decode_json_bytes_with_limits(
                    &fs::read(project.root().join("prefabs").join(alias)).unwrap(),
                    SceneFileLimits::default(),
                )
                .unwrap()
                .into_composition_source()
                .unwrap();
                assert_eq!(prefab.entities.len(), 1);
                assert_eq!(
                    prefab.placements,
                    [SceneEntityPlacement::root(
                        SceneEntityId::new(PREFAB_ROOT).unwrap()
                    )],
                );
                inspect_map(
                    &prefab.entities[0].components[&ComponentTypeId::new(TILEMAP)],
                    200,
                    tile_size,
                    &tileset_path,
                    &mut covered,
                );
            } else {
                assert!(portal.prefab.is_none());
                assert!(!project.root().join("prefabs/workload").exists());
            }
            assert_eq!(covered.len(), population);
            assert_eq!(covered.iter().map(|point| point.0).min(), Some(-120));
            assert_eq!(covered.iter().map(|point| point.1).min(), Some(-128));
            assert_eq!(
                covered.iter().map(|point| point.0 + tile_size).max(),
                Some(136)
            );
            assert_eq!(
                covered.iter().map(|point| point.1 + tile_size).max(),
                Some(128)
            );
            assert_eq!((256 / tile_size) as usize, side);

            let original_scene: Value =
                serde_json::from_slice(&fs::read(original.join(&relative)).unwrap()).unwrap();
            let mut unchanged: Value = serde_json::from_slice(&bytes).unwrap();
            entity_components(&mut unchanged, "checkpoint")
                .unwrap()
                .remove(TILEMAP);
            if population == 16384 {
                let entities = unchanged["payload"]["entities"].as_array_mut().unwrap();
                entities
                    .iter_mut()
                    .find(|entity| entity["id"] == "portal")
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove("prefab");
            }
            assert_eq!(
                unchanged, original_scene,
                "all other authored data must survive"
            );

            let tileset = TileSetSource::decode_ron_bytes(
                &fs::read(project.root().join("assets").join(&tileset_path)).unwrap(),
                TileSetSourceFileLimits::default(),
            )
            .unwrap();
            assert_eq!(tileset.sampler, SamplerDescriptor::NEAREST_CLAMP);
            assert_eq!(
                tileset.image,
                AssetRef::path(format!("textures/{level}.png")).unwrap()
            );
            tileset
                .validate_image_extent(ImageExtent::new(16, 16))
                .unwrap();
            let metadata_bytes = fs::read(
                project
                    .root()
                    .join("assets")
                    .join(format!("{tileset_path}.meta")),
            )
            .unwrap();
            let metadata = AssetMetaCandidate::<()>::decode_json_bytes(&metadata_bytes)
                .unwrap()
                .into_meta();
            assert_eq!(
                metadata.identity().stable_id(),
                StableAssetId::parse_str(stable_id).unwrap(),
            );
            assert_eq!(
                metadata.identity().path(),
                &AssetPath::new(&tileset_path).unwrap()
            );
            assert_eq!(
                metadata.identity().source_kind(),
                &AssetSourceKind::Other("tileset".into()),
            );
            let wire: Value = serde_json::from_slice(&metadata_bytes).unwrap();
            assert!(wire["payload"]["settings"].is_null());
        }
        let root = project.root().to_path_buf();
        project.cleanup().unwrap();
        assert!(!root.exists());
        project.cleanup().unwrap();
    }
}

fn inspect_map(
    record: &SceneComponentRecord,
    host_x: i64,
    tile_size: i64,
    tileset: &str,
    covered: &mut BTreeSet<(i64, i64)>,
) {
    let value = record.value();
    assert_eq!(
        value.field("tile_size").unwrap().field_f64("x").unwrap(),
        tile_size as f64
    );
    assert_eq!(
        value.field("tile_size").unwrap().field_f64("y").unwrap(),
        tile_size as f64
    );
    assert_eq!(value.field_i64("layer").unwrap(), -10);
    assert_eq!(
        *value.field("tileset").unwrap(),
        encode_asset_reference_value(&AssetRef::path(tileset).unwrap()),
    );
    let ComponentValue::List(cells) = value.field("cells").unwrap() else {
        panic!("source Tilemap cells must be a list");
    };
    for cell in cells {
        let coord = cell.field("coord").unwrap();
        let x = host_x + coord.field_i64("x").unwrap() * tile_size;
        let y = coord.field_i64("y").unwrap() * tile_size;
        assert!(covered.insert((x, y)), "ground cells overlap at {x}, {y}");
        let cell = cell.field("cell").unwrap();
        assert_eq!(cell.field_u64("tile").unwrap(), 0);
        let color = cell.field("color").unwrap();
        for channel in ["r", "g", "b", "a"] {
            assert_eq!(color.field_f64(channel).unwrap(), 1.0);
        }
    }
}

#[test]
fn cleanup_refuses_a_changed_parent_or_replacement_directory() {
    let mut project = Project::new(64).unwrap();
    let root = project.root().to_path_buf();
    let parent = project.temp_parent.clone();
    project.temp_parent = root.clone();
    assert!(project.cleanup().is_err());
    assert!(root.exists());
    project.temp_parent = parent;

    let moved = root.with_file_name(format!("{}-moved", project.owner));
    assert!(!moved.exists());
    fs::rename(&root, &moved).unwrap();
    fs::create_dir(&root).unwrap();
    let sentinel = root.join("replacement.txt");
    fs::write(&sentinel, b"replacement must not be removed").unwrap();
    assert!(project.cleanup().is_err());
    assert!(sentinel.exists());
    assert!(moved.join(OWNER_FILE).exists());
    fs::remove_file(&sentinel).unwrap();
    fs::remove_dir(&root).unwrap();
    fs::rename(&moved, &root).unwrap();
    project.cleanup().unwrap();
    assert!(!root.exists());
}

#[test]
fn copy_refuses_an_outside_source_and_an_exhausted_byte_budget() {
    let mut project = Project::new(64).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let destination = project.root().join("must-not-be-copied");
    let mut files = BTreeMap::new();
    assert!(
        copy_source(
            &source,
            &project.root().join(OWNER_FILE),
            &destination,
            0,
            &mut CopyBudget::default(),
            &mut files,
        )
        .is_err()
    );
    assert!(!destination.exists());
    assert!(
        copy_source(
            &source,
            &source.join("nara.toml"),
            &destination,
            0,
            &mut CopyBudget {
                entries: 0,
                bytes: MAX_COPY_BYTES,
            },
            &mut files,
        )
        .is_err()
    );
    assert!(!destination.exists());
    assert!(files.is_empty());
    project.cleanup().unwrap();
    assert!(Project::new(65).is_err());
}

#[cfg(unix)]
#[test]
fn copy_refuses_a_symbolic_link_even_when_its_target_is_inside_the_source_root() {
    let mut project = Project::new(64).unwrap();
    let link = project.root().join("linked-manifest.toml");
    std::os::unix::fs::symlink(project.root().join("nara.toml"), &link).unwrap();
    let destination = project.root().join("must-not-be-copied");
    assert!(
        copy_source(
            project.root(),
            &link,
            &destination,
            0,
            &mut CopyBudget::default(),
            &mut BTreeMap::new(),
        )
        .is_err()
    );
    assert!(!destination.exists());
    project.cleanup().unwrap();
}
