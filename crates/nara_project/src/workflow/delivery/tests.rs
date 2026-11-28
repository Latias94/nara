use std::{
    process::ExitStatus,
    time::{Duration, UNIX_EPOCH},
};

use nara_asset::cooked::{NpakArchive, NpakEntryKind};
use nara_reflect::{
    ComponentCapability, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
};
use nara_scene::{
    authoring::{SceneComponent, SceneDocument},
    cooked::CookedScene,
};

use crate::workflow::{SCHEMA_CACHE_PATH, schema::finish_refresh};

use super::*;

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt as _;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt as _;

const ENTRY: &str = "0193a100-1111-7000-8000-000000000001";
const PREFAB: &str = "0193a100-1111-7000-8000-000000000002";
const PNG: &str = "0193a100-1111-7000-8000-000000000003";
const MATERIAL: &str = "0193a100-1111-7000-8000-000000000004";
const SHADER: &str = "0193a100-1111-7000-8000-000000000005";
const UNUSED: &str = "0193a100-1111-7000-8000-000000000006";

const VALID_SHADER: &str = "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return sampled * tint * material_parameters.color; }";

#[test]
fn packages_only_flattened_dependencies_and_caches_each_input_cut() {
    let temp = tempfile::tempdir().unwrap();
    fixture(temp.path());
    let builtins = nara_scene::authoring::builtin_component_schemas().unwrap();
    let catalog = ComponentSchemaCatalog::from_components(builtins).unwrap();
    let state = temp.path().join(PROJECT_STATE);
    fs::create_dir(&state).unwrap();
    fs::write(
        temp.path().join(crate::workflow::SCHEMA_CACHE_PATH),
        catalog.to_json_string().unwrap(),
    )
    .unwrap();
    let cook = || {
        cook_project(
            temp.path(),
            ProjectTrust::Untrusted,
            || false,
            |_| {
                panic!("cached data cooking must not run a build");
            },
        )
        .unwrap()
    };
    let first = cook();
    assert_eq!(
        (first.assets, first.cache_hits, first.cache_misses),
        (4, 0, 4)
    );
    let first_bytes = fs::read(&first.archive).unwrap();
    let pack = NpakArchive::from_bytes(first_bytes.clone()).unwrap();
    assert_eq!(pack.entries().len(), 4);
    assert!(pack.entry(PREFAB.parse().unwrap()).is_none());
    assert!(pack.entry(UNUSED.parse().unwrap()).is_none());
    let bytes = pack
        .read_kind(ENTRY.parse().unwrap(), NpakEntryKind::Scene)
        .unwrap();
    let scene = CookedScene::parse(&bytes).unwrap();
    assert_eq!(scene.entity_count(), 1);
    assert_eq!(
        scene.asset_dependencies(),
        &[PNG.parse().unwrap(), MATERIAL.parse().unwrap()]
    );
    let image = pack
        .read_kind(PNG.parse().unwrap(), NpakEntryKind::Image)
        .unwrap();
    assert_eq!(&image[..4], b"NIMG");
    assert_eq!(
        pack.read_kind(SHADER.parse().unwrap(), NpakEntryKind::Shader)
            .unwrap(),
        VALID_SHADER.as_bytes()
    );

    let again = cook();
    assert_eq!((again.cache_hits, again.cache_misses), (4, 0));
    assert_eq!(first_bytes, fs::read(&again.archive).unwrap());

    let root = temp.path().join("assets");
    fs::rename(root.join("pixel.png"), root.join("renamed.png")).unwrap();
    fs::rename(root.join("pixel.png.meta"), root.join("renamed.png.meta")).unwrap();
    let moved = cook();
    assert_eq!((moved.cache_hits, moved.cache_misses), (4, 0));
    assert_eq!(first_bytes, fs::read(&moved.archive).unwrap());

    let prefab = root.join("node.prefab.ron");
    let edited = fs::read_to_string(&prefab)
        .unwrap()
        .replace("(12.0, 8.0)", "(14.0, 8.0)");
    fs::write(prefab, edited).unwrap();
    let edited = cook();
    assert_eq!((edited.cache_hits, edited.cache_misses), (3, 1));
    let edited_bytes = fs::read(&edited.archive).unwrap();
    assert_ne!(first_bytes, edited_bytes);

    fs::write(root.join("effect.wgsl"), "invalid WGSL").unwrap();
    let error = cook_project(temp.path(), ProjectTrust::Untrusted, || false, |_| {}).unwrap_err();
    assert!(matches!(error, DeliveryError::Asset { id, .. } if id == SHADER.parse().unwrap()));
    assert_eq!(edited_bytes, fs::read(&edited.archive).unwrap());
}

#[test]
fn untrusted_cook_leaves_missing_sidecars_for_explicit_discovery() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    fixture(project);
    fs::write(project.join("Cargo.toml"), "[package]\nname = 'fixture'\n").unwrap();
    let source = project.join("assets/unused.txt");
    let sidecar = source.with_file_name("unused.txt.meta");
    fs::remove_file(&sidecar).unwrap();
    let cook = |trust| cook_project(project, trust, || false, |_| panic!("unexpected build"));

    assert!(matches!(
        cook(ProjectTrust::Untrusted),
        Err(DeliveryError::AssetScan(AssetScanError::MissingSidecar { path }))
            if path.as_str() == "assets/unused.txt"
    ));
    assert!(!sidecar.exists());
    assert_eq!(fs::read(&source).unwrap(), b"unused");
    assert!(!project.join(PROJECT_STATE).exists());

    let trusted = cook(ProjectTrust::Trusted).unwrap();
    let discovered = fs::read(&sidecar).unwrap();
    assert_eq!(
        trusted.archive,
        project.canonicalize().unwrap().join(".nara/game.npak")
    );
    let offline = cook(ProjectTrust::Untrusted).unwrap();
    assert_eq!((offline.assets, offline.cache_hits), (4, 4));
    assert_eq!(fs::read(sidecar).unwrap(), discovered);
    assert_eq!(fs::read(source).unwrap(), b"unused");
}

#[test]
fn offline_cook_uses_the_newest_matching_catalog() {
    let component_id = ComponentTypeId::new("fixture.Marker");
    let version = ComponentSchemaVersion(2);
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    fixture(project);
    fs::write(project.join("Cargo.toml"), "[package]\nname = 'fixture'\n").unwrap();
    let release_cache = schema_cache_path("fixture", "game", &[]);
    let builtins = nara_scene::authoring::builtin_component_schemas().unwrap();
    let stale = ComponentSchemaCatalog::from_components(builtins).unwrap();
    let stale_json = stale.to_json_string().unwrap();
    fs::create_dir(project.join(PROJECT_STATE)).unwrap();
    fs::write(project.join(&release_cache), &stale_json).unwrap();
    let profile = read_project_manifest(project).unwrap().delivery.unwrap();
    assert_eq!(
        load_schemas(
            project,
            &profile,
            ProjectTrust::Untrusted,
            &mut || false,
            &mut |_| {}
        )
        .unwrap(),
        stale
    );

    let fresh = ComponentSchemaCatalog::from_components([ComponentSchema::new(
        component_id.clone(),
        "Marker",
        version,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_postcard_codec()])
    .unwrap();
    let scene_path = project.join("assets/entry.scene.ron");
    let mut scene = SceneDocument::read(&scene_path).unwrap();
    scene.entities[0].components.insert(
        component_id.clone(),
        SceneComponent::known(version, ComponentValue::Map(Default::default())),
    );
    scene.write_with(&scene_path, fresh.components()).unwrap();
    let cook = || {
        cook_project(
            project,
            ProjectTrust::Untrusted,
            || false,
            |_| {
                panic!("offline Cook must not run a build");
            },
        )
    };
    assert!(matches!(
        cook(),
        Err(DeliveryError::MissingComponentSchema { component, version: 2, .. })
            if component == component_id.as_str()
    ));

    let fresh_json = fresh.to_json_string().unwrap();
    finish_refresh(
        project,
        ExitStatus::from_raw(0),
        fresh_json.as_bytes(),
        || false,
    )
    .unwrap();
    let report = cook().unwrap();
    assert_eq!(report.assets, 4);
    assert_eq!(
        fs::read_to_string(project.join(SCHEMA_CACHE_PATH)).unwrap(),
        fresh_json
    );
    assert_eq!(
        fs::read_to_string(project.join(&release_cache)).unwrap(),
        stale_json
    );

    fs::write(project.join(&release_cache), &fresh_json).unwrap();
    fs::write(project.join(SCHEMA_CACHE_PATH), stale_json).unwrap();
    for (path, time) in [(release_cache.as_str(), 2), (SCHEMA_CACHE_PATH, 1)] {
        fs::OpenOptions::new()
            .write(true)
            .open(project.join(path))
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(time))
            .unwrap();
    }
    assert_eq!(cook().unwrap().cache_hits, 4);
}

#[test]
fn export_guards_destination_and_trust_before_running_cargo() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("existing");
    fs::create_dir(&output).unwrap();
    let present = output.join("present.txt");
    fs::write(&present, "keep me").unwrap();
    let export = |trust| {
        export_project(
            temp.path(),
            &output,
            trust,
            || false,
            |_| panic!("unexpected build"),
        )
    };
    assert!(matches!(
        export(ProjectTrust::Untrusted).unwrap_err(),
        DeliveryError::UntrustedProject
    ));
    assert!(matches!(
        export(ProjectTrust::Trusted).unwrap_err(),
        DeliveryError::DestinationExists(_)
    ));
    assert_eq!(fs::read_to_string(present).unwrap(), "keep me");
    assert!(
        cook_project(temp.path(), ProjectTrust::Untrusted, || true, |_| {})
            .unwrap_err()
            .is_interrupted()
    );
}

#[test]
fn publication_moves_the_complete_directory_and_excludes_a_concurrent_destination() {
    let root = tempfile::tempdir().unwrap();
    let staging = root.path().join("staging");
    let destination = root.path().join("output");
    fs::create_dir(&staging).unwrap();
    fs::create_dir(&destination).unwrap();
    fs::write(staging.join("game"), "exe").unwrap();
    fs::write(staging.join(GAME_ARCHIVE), "assets").unwrap();
    assert!(publish::directory(&staging, &destination).is_err());
    assert!(fs::read_dir(&destination).unwrap().next().is_none());
    fs::remove_dir(&destination).unwrap();
    publish::directory(&staging, &destination).unwrap();
    assert!(!staging.exists());
    assert_eq!(fs::read_to_string(destination.join("game")).unwrap(), "exe");
    assert_eq!(
        fs::read_to_string(destination.join(GAME_ARCHIVE)).unwrap(),
        "assets"
    );
}

#[test]
fn schema_selection_respects_cargo_auto_targets_and_caches_the_release_profile() {
    let root = tempfile::tempdir().unwrap();
    let cargo = root.path().join("Cargo.toml");
    let source = root.path().join("src/bin/nara_schema");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("main.rs"), "fn main() {}\n").unwrap();
    let manifest = "[package]\nname = 'fixture'\nversion = '0.1.0'\n";
    fs::write(&cargo, manifest).unwrap();
    let first = CargoPackage::read(root.path()).unwrap();
    assert!(first.has_binary(root.path(), SCHEMA_BINARY));
    fs::write(&cargo, format!("{manifest}autobins = false\n")).unwrap();
    let second = CargoPackage::read(root.path()).unwrap();
    assert!(!second.has_binary(root.path(), SCHEMA_BINARY));
    assert_ne!(
        schema_cache_path("fixture", "game", &["desktop"]),
        schema_cache_path("fixture", "game", &["arena", "desktop"])
    );
}

fn fixture(project: &Path) {
    fs::write(project.join("nara.toml"), format!("[project]\nname = \"Fixture\"\nversion = \"0.1.0\"\n[delivery]\nscenes = [\"{ENTRY}\"]\n")).unwrap();
    let root = project.join("assets");
    fs::create_dir(&root).unwrap();
    let put = |name: &str, id: &str, importer: &str, bytes: &[u8]| {
        fs::write(root.join(name), bytes).unwrap();
        fs::write(
            root.join(format!("{name}.meta")),
            format!(r#"AssetMeta(id: "{id}", importer: "{importer}", settings: ())"#),
        )
        .unwrap();
    };
    put(
        "entry.scene.ron",
        ENTRY,
        "nara.scene",
        format!(
            r#"
Scene(version: 1, id: "0513a100-1111-7000-8000-000000000001",
    assets: {{"node": (id: "{PREFAB}", hint: "node.prefab.ron")}},
    entities: [(id: "0513a100-1111-7000-8000-000000000002", name: "Node", prefab: "@node")])"#
        )
        .as_bytes(),
    );
    put("node.prefab.ron", PREFAB, "nara.prefab", format!(r#"
Prefab(version: 1, id: "0513a100-1111-7000-8000-000000000003",
    assets: {{"pixel": (id: "{PNG}", hint: "pixel.png"), "paint": (id: "{MATERIAL}", hint: "effect.material.ron")}},
    entities: [(id: "0513a100-1111-7000-8000-000000000004", name: "Sprite",
        components: {{"nara.SpriteSource": (size: (12.0, 8.0), image: "@pixel", material: "@paint")}})])"#).as_bytes());
    put(
        "pixel.png",
        PNG,
        "nara.image",
        include_bytes!("../../../../../reference-game/assets/power-cell.png"),
    );
    put(
        "effect.material.ron",
        MATERIAL,
        "nara.material",
        format!(r#"SpriteMaterial(shader: "{SHADER}")"#).as_bytes(),
    );
    put(
        "effect.wgsl",
        SHADER,
        "nara.shader",
        VALID_SHADER.as_bytes(),
    );
    put("unused.txt", UNUSED, "nara.raw", b"unused");
}
