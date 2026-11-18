//! Fence-only fixtures use finite opaque bytes; cook_runtime exercises real source admission.
//! A selected schema joins the original fence through project-relative Raw delivery observations.
//! Binding validates the observed digest, then publication checks preserve that file identity.

#![cfg(any(feature = "development", feature = "cook"))]

#[path = "../src/bin/cook/source.rs"]
mod source;

#[path = "../src/bin/cook/material.rs"]
mod material;
#[path = "../src/bin/cook/tileset.rs"]
mod tileset;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara::{
    asset::{
        AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath, AssetPathScope, AssetRef,
        AssetSourceKind, AssetSourcePath, ProjectAssetDatabase, RuntimeAssetCodec,
        SourceAssetBuild,
    },
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildObservation, ArtifactBuildStore,
        ArtifactContentDigest, ArtifactSourceRequest, VerifiedArtifactGroup, build_source,
    },
    core::FormatVersion,
    material::{Material2dAsset, Material2dRuntimeCodec, Material2dSource},
    project_tooling::host_fs,
    tilemap::{TileSetDescriptor, TileSetRuntimeCodec, TileSetSource, TileSetSourceFileLimits},
};
use two_level_game::{
    region_material, region_tileset, side_room,
    sound_bank::{BANK_PATH, SoundBankDomain},
};

const RENAMED_PICKUP: &str = "assets/audio/pickup-v2.wav";
const SELECTED_SCHEMA: &str = "authoring/selected.bundle.json";

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nara-cook-fence-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        for (name, bytes) in [
            ("nara.toml", "manifest"),
            ("input/explorer.actions.json", "input mapping"),
            ("schema/authoring.bundle.json", "schema"),
            ("scenes/meadow.scene.json", "meadow"),
            ("scenes/cavern.scene.json", "cavern"),
            ("scenes/marker.scene.json", "marker wrapper"),
            ("scenes/marker.scene.json.meta", "marker metadata"),
            ("scenes/side-room.scene.json", "side room"),
            ("scenes/side-room.scene.json.meta", "side room metadata"),
            ("scenes/lookout.scene.json", "lookout"),
            ("scenes/lookout.scene.json.meta", "lookout metadata"),
            ("prefabs/marker.prefab.json", "marker prefab"),
            ("assets/textures/meadow.png", "image A"),
            ("assets/textures/meadow.png.meta", "meta A"),
            ("assets/textures/cavern.png", "image B"),
            ("assets/textures/cavern.png.meta", "meta B"),
            ("assets/textures/explorer.png", "explorer atlas"),
            ("assets/textures/explorer.png.meta", "explorer metadata"),
            ("assets/textures/marker.png", "marker badge"),
            ("assets/textures/marker.png.meta", "marker metadata"),
            ("assets/textures/side-room.png", "side room relic"),
            (
                "assets/textures/side-room.png.meta",
                "side room relic metadata",
            ),
            ("assets/materials/region.material2d.json", "material"),
            (
                "assets/materials/region.material2d.json.meta",
                "material metadata",
            ),
            ("assets/materials/region.wgsl", "shader"),
            ("assets/tilesets/region.tileset.ron", "tileset"),
            (
                "assets/tilesets/region.tileset.ron.meta",
                "tileset metadata",
            ),
            ("assets/animations/explorer.sprite-animations.json", "clips"),
            (
                "assets/animations/explorer.sprite-animations.json.meta",
                "clips meta",
            ),
            ("assets/audio/game.soundbank.json", "bank"),
            ("assets/audio/game.soundbank.json.meta", "bank meta"),
            ("assets/audio/pickup.wav", "pickup"),
            ("assets/audio/travel.wav", "travel"),
            ("Cargo.toml", "package"),
            ("Cargo.lock", "lock"),
            ("build.rs", "fn main() {}"),
            ("src/lib.rs", "pub fn game() {}"),
            ("src/bin/player.rs", "fn main() {}"),
        ] {
            let target = path.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, bytes).unwrap();
        }
        Self(path)
    }

    fn selected_schema() -> Self {
        let directory = Self::new();
        let selected = directory.0.join(SELECTED_SCHEMA);
        fs::create_dir_all(selected.parent().unwrap()).unwrap();
        fs::rename(directory.0.join("schema/authoring.bundle.json"), selected).unwrap();
        fs::write(
            directory.0.join("nara.toml"),
            format!("[authoring]\nstate = \"ready\"\nschema_bundle = \"{SELECTED_SCHEMA}\"\n"),
        )
        .unwrap();
        directory
    }

    fn renamed_sound_bank() -> Self {
        let directory = Self::new();
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"));
        for name in [
            "assets/audio/game.soundbank.json.meta",
            "assets/audio/pickup.wav",
            "assets/audio/travel.wav",
        ] {
            fs::copy(sample.join(name), directory.0.join(name)).unwrap();
        }
        fs::rename(
            directory.0.join("assets/audio/pickup.wav"),
            directory.0.join(RENAMED_PICKUP),
        )
        .unwrap();
        fs::write(
            directory.0.join("assets/audio/game.soundbank.json"),
            br#"{"pickup":"audio/pickup-v2.wav","travel":"audio/travel.wav"}"#,
        )
        .unwrap();
        directory
    }

    fn build_sound_bank(&self) -> VerifiedArtifactGroup {
        let cache = self.0.join("cache");
        fs::create_dir(&cache).unwrap();
        let store = ArtifactBuildStore::open(
            host_fs::writable_directory(&cache).unwrap(),
            ArtifactBuildLimits::default(),
        )
        .unwrap();
        let request = ArtifactSourceRequest::new(
            host_fs::readable_directory(&self.0.join("assets")).unwrap(),
            AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
            AssetPath::new(BANK_PATH).unwrap(),
            ArtifactBuildDefinition {
                id: SoundBankDomain::BUILD_ID.to_owned(),
                revision: FormatVersion::new(SoundBankDomain::BUILD_REVISION).unwrap(),
                implementation: ArtifactContentDigest::of_bytes(b"cook-source-fence-test/v1"),
            },
        );
        build_source::<SoundBankDomain>(&store, &request, |_| Ok(()))
            .unwrap()
            .into_group()
    }

    fn material() -> Self {
        let directory = Self::new();
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"));
        for name in [
            "assets/materials/region.material2d.json",
            "assets/materials/region.material2d.json.meta",
            "assets/materials/region.wgsl",
            "assets/textures/meadow.png.meta",
        ] {
            fs::copy(sample.join(name), directory.0.join(name)).unwrap();
        }
        directory
    }

    fn import_material(&self) -> (Material2dAsset, Vec<ArtifactBuildObservation>) {
        let root = host_fs::readable_directory(&self.0).unwrap();
        let scope = AssetPathScope::under(AssetSourcePath::new("assets").unwrap());
        let metadata = AssetMetaIdentityCandidate::decode_json_bytes(
            &fs::read(self.0.join("assets/textures/meadow.png.meta")).unwrap(),
        )
        .unwrap();
        let mut database = ProjectAssetDatabase::default();
        database
            .insert_identity(
                &scope,
                AssetPath::new(region_material::PALETTE_PATH).unwrap(),
                metadata.into_identity(),
            )
            .unwrap();
        let mut observations = Vec::new();
        let material = material::prepare(&root, &scope, &mut database, &mut observations).unwrap();
        (material, observations)
    }

    fn tileset() -> Self {
        let directory = Self::new();
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"));
        for name in [
            "assets/tilesets/region.tileset.ron",
            "assets/tilesets/region.tileset.ron.meta",
            "assets/textures/side-room.png.meta",
        ] {
            fs::copy(sample.join(name), directory.0.join(name)).unwrap();
        }
        directory
    }

    fn resolve_tileset(&self) -> (TileSetDescriptor, Vec<ArtifactBuildObservation>) {
        let root = host_fs::readable_directory(&self.0).unwrap();
        let scope = AssetPathScope::under(AssetSourcePath::new("assets").unwrap());
        let metadata = AssetMetaIdentityCandidate::decode_json_bytes(
            &fs::read(self.0.join("assets/textures/side-room.png.meta")).unwrap(),
        )
        .unwrap();
        let mut database = ProjectAssetDatabase::default();
        database
            .insert_identity(
                &scope,
                AssetPath::new(side_room::IMAGE_PATH).unwrap(),
                metadata.into_identity(),
            )
            .unwrap();
        let mut observations = Vec::new();
        let tileset = tileset::prepare(&root, &scope, &mut database, &mut observations).unwrap();
        (tileset, observations)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn selected_schema_is_bound_by_delivery_observations_without_the_default_bundle() {
    let directory = Directory::selected_schema();
    let default = directory.0.join("schema/authoring.bundle.json");
    assert!(!default.exists());
    let selected = directory.0.join(SELECTED_SCHEMA);
    let bytes = fs::read(&selected).unwrap();
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    fence.verify().unwrap();
    let observations = [ArtifactBuildObservation::Raw {
        path: AssetSourcePath::new(SELECTED_SCHEMA).unwrap(),
        digest: Some(ArtifactContentDigest::of_bytes(&bytes)),
    }];
    fence.bind_raw_inputs(&observations).unwrap();
    fs::write(&default, b"unselected default bundle").unwrap();
    let inputs = source::code_inventory(&directory.0).unwrap();
    fence.bind_code(&inputs).unwrap();
    fence.verify().unwrap();
    fs::write(&selected, b"changed selected bundle").unwrap();
    assert!(fence.verify().unwrap_err().contains(SELECTED_SCHEMA));
    fs::write(&selected, &bytes).unwrap();
    fence.verify().unwrap();
    fs::remove_file(&selected).unwrap();
    assert!(fence.verify().is_err());
    assert!(default.is_file());
}

#[test]
fn selected_schema_absence_or_stale_digest_cannot_be_admitted_at_raw_binding() {
    for missing in [false, true] {
        let directory = Directory::selected_schema();
        let selected = directory.0.join(SELECTED_SCHEMA);
        let bytes = fs::read(&selected).unwrap();
        let mut fence = source::ContentFence::capture(&directory.0).unwrap();
        let observations = [ArtifactBuildObservation::Raw {
            path: AssetSourcePath::new(SELECTED_SCHEMA).unwrap(),
            digest: Some(ArtifactContentDigest::of_bytes(&bytes)),
        }];
        if missing {
            fs::remove_file(&selected).unwrap();
        } else {
            fs::write(&selected, b"different selected bundle before binding").unwrap();
        }
        let error = fence.bind_raw_inputs(&observations).unwrap_err();
        if !missing {
            assert!(error.contains(SELECTED_SCHEMA), "{error}");
        }
        fs::write(&selected, &bytes).unwrap();
        fence.bind_raw_inputs(&observations).unwrap();
        fence.verify().unwrap();
    }
}

#[test]
fn optional_input_metadata_presence_and_content_stay_in_the_original_fence() {
    let directory = Directory::new();
    let path = directory.0.join("input/explorer.actions.json.meta");
    let absent = source::ContentFence::capture(&directory.0).unwrap();
    absent.verify().unwrap();
    fs::write(&path, b"original input metadata").unwrap();
    assert!(
        absent
            .verify()
            .unwrap_err()
            .contains("input/explorer.actions.json.meta")
    );
    let present = source::ContentFence::capture(&directory.0).unwrap();
    present.verify().unwrap();
    fs::write(&path, b"modified input metadata").unwrap();
    assert!(present.verify().is_err());
    fs::write(&path, b"original input metadata").unwrap();
    present.verify().unwrap();
    fs::remove_file(&path).unwrap();
    assert!(present.verify().is_err());
    absent.verify().unwrap();
}

#[test]
fn content_fence_remains_original_when_code_is_bound_after_generation() {
    for name in [
        "scenes/cavern.scene.json",
        "scenes/marker.scene.json",
        "scenes/marker.scene.json.meta",
        "prefabs/marker.prefab.json",
        "input/explorer.actions.json",
        "assets/textures/explorer.png",
        "assets/textures/explorer.png.meta",
        "assets/textures/marker.png",
        "assets/textures/marker.png.meta",
        "scenes/side-room.scene.json",
        "scenes/lookout.scene.json",
        "scenes/lookout.scene.json.meta",
        "assets/textures/side-room.png.meta",
        "assets/animations/explorer.sprite-animations.json",
        "assets/animations/explorer.sprite-animations.json.meta",
        "assets/materials/region.material2d.json",
        "assets/materials/region.material2d.json.meta",
        "assets/materials/region.wgsl",
        "assets/tilesets/region.tileset.ron",
        "assets/tilesets/region.tileset.ron.meta",
    ] {
        let directory = Directory::new();
        let mut fence = source::ContentFence::capture(&directory.0).unwrap();
        let inputs = source::code_inventory(&directory.0).unwrap();
        fs::write(directory.0.join(name), "changed").unwrap();
        assert!(fence.bind_code(&inputs).unwrap_err().contains(name));
    }
}

#[test]
fn content_fence_requires_authored_sources_and_metadata() {
    for name in [
        "input/explorer.actions.json",
        "scenes/marker.scene.json",
        "scenes/marker.scene.json.meta",
        "prefabs/marker.prefab.json",
        "assets/textures/explorer.png",
        "assets/textures/explorer.png.meta",
        "assets/textures/marker.png",
        "assets/textures/marker.png.meta",
        "scenes/side-room.scene.json",
        "scenes/side-room.scene.json.meta",
        "scenes/lookout.scene.json",
        "scenes/lookout.scene.json.meta",
        "assets/textures/side-room.png",
        "assets/textures/side-room.png.meta",
        "assets/animations/explorer.sprite-animations.json",
        "assets/animations/explorer.sprite-animations.json.meta",
        "assets/materials/region.material2d.json",
        "assets/materials/region.material2d.json.meta",
        "assets/materials/region.wgsl",
        "assets/tilesets/region.tileset.ron",
        "assets/tilesets/region.tileset.ron.meta",
    ] {
        let directory = Directory::new();
        fs::remove_file(directory.0.join(name)).unwrap();
        assert!(
            source::ContentFence::capture(&directory.0).is_err(),
            "{name}"
        );
    }
}

#[test]
fn original_code_stays_fenced_through_later_publication_checks() {
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/lib.rs",
        "src/bin/player.rs",
        "prefabs/marker.prefab.json",
        "assets/materials/region.material2d.json",
        "assets/materials/region.material2d.json.meta",
        "assets/materials/region.wgsl",
        "assets/tilesets/region.tileset.ron",
        "assets/tilesets/region.tileset.ron.meta",
    ] {
        let directory = Directory::new();
        let mut fence = source::ContentFence::capture(&directory.0).unwrap();
        let inputs = source::code_inventory(&directory.0).unwrap();
        fence.bind_code(&inputs).unwrap();
        assert_eq!(
            fence
                .root()
                .open_file(&nara::fs::RelativePath::new("nara.toml").unwrap())
                .unwrap()
                .read_to_end_bounded(64)
                .unwrap(),
            b"manifest"
        );
        fence.verify().unwrap();
        fs::write(directory.0.join(name), "changed code selection").unwrap();
        assert!(fence.verify().is_err(), "{name}");
    }
}

#[test]
fn code_set_changes_and_rebinding_are_refused() {
    let directory = Directory::new();
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    let inputs = source::code_inventory(&directory.0).unwrap();
    assert_eq!(
        inputs,
        [
            "Cargo.toml",
            "Cargo.lock",
            "build.rs",
            "assets/materials/region.wgsl",
            "src/bin/player.rs",
            "src/lib.rs",
        ]
        .map(|path| nara::fs::RelativePath::new(path).unwrap())
    );
    assert!(!directory.0.join("player-bootstrap.rs").exists());
    assert!(!directory.0.join("src/bin/player/generated.rs").exists());
    fence.bind_code(&inputs).unwrap();
    fence.verify().unwrap();
    assert!(fence.bind_code(&inputs).is_err());
    fs::write(directory.0.join("src/later.rs"), "new code").unwrap();
    assert!(fence.verify().is_err());
}

#[test]
fn renamed_sound_bank_dependency_is_fenced_without_its_old_path() {
    let directory = Directory::renamed_sound_bank();
    assert!(!directory.0.join("assets/audio/pickup.wav").exists());
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    let group = directory.build_sound_bank();
    assert!(group.observations().iter().any(|observation| matches!(
        observation,
        ArtifactBuildObservation::Raw { path, digest: Some(_) }
            if path.as_str() == RENAMED_PICKUP
    )));
    fence.bind_raw_inputs(group.observations()).unwrap();
    let inputs = source::code_inventory(&directory.0).unwrap();
    fence.bind_code(&inputs).unwrap();
    fence.verify().unwrap();
}

#[test]
fn edited_raw_dependency_is_rejected_after_observation_binding() {
    let directory = Directory::renamed_sound_bank();
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    let group = directory.build_sound_bank();
    fence.bind_raw_inputs(group.observations()).unwrap();
    let inputs = source::code_inventory(&directory.0).unwrap();
    fence.bind_code(&inputs).unwrap();
    let path = directory.0.join(RENAMED_PICKUP);
    let mut bytes = fs::read(&path).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(path, bytes).unwrap();
    assert!(fence.verify().unwrap_err().contains(RENAMED_PICKUP));
}

#[test]
fn stale_raw_observation_is_rejected_instead_of_rebaselining_at_binding() {
    let directory = Directory::renamed_sound_bank();
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    let group = directory.build_sound_bank();
    let path = directory.0.join(RENAMED_PICKUP);
    let mut bytes = fs::read(&path).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(path, bytes).unwrap();
    assert!(
        fence
            .bind_raw_inputs(group.observations())
            .unwrap_err()
            .contains(RENAMED_PICKUP)
    );
}

#[test]
fn binding_raw_observations_preserves_prebuild_source_file_identity() {
    let directory = Directory::renamed_sound_bank();
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    let group = directory.build_sound_bank();
    let path = directory.0.join("assets/audio/game.soundbank.json");
    let bytes = fs::read(&path).unwrap();
    fs::rename(&path, path.with_extension("original")).unwrap();
    fs::write(&path, bytes).unwrap();
    assert!(
        fence
            .bind_raw_inputs(group.observations())
            .unwrap_err()
            .contains("assets/audio/game.soundbank.json")
    );
    assert!(fence.verify().is_err());
}

#[test]
fn material_texture_path_requires_an_admitted_image_identity() {
    let directory = Directory::material();
    let root = host_fs::readable_directory(&directory.0).unwrap();
    let scope = AssetPathScope::under(AssetSourcePath::new("assets").unwrap());
    for kind in [None, Some(AssetSourceKind::Scene)] {
        let mut database = ProjectAssetDatabase::default();
        if let Some(kind) = kind {
            let path = AssetPath::new(region_material::PALETTE_PATH).unwrap();
            database
                .insert_identity(
                    &scope,
                    path.clone(),
                    AssetMetaIdentity::new(region_material::palette_product().source(), path, kind),
                )
                .unwrap();
        }
        let error = material::prepare(&root, &scope, &mut database, &mut Vec::new()).unwrap_err();
        assert!(error.contains("Material texture"), "{error}");
    }
}

#[test]
fn material_shader_observations_stay_fenced_through_bootstrap_and_publication() {
    let directory = Directory::material();
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    let authored = Material2dSource::decode(
        &fs::read(directory.0.join("assets/materials/region.material2d.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        authored.textures()[region_material::PALETTE_ID],
        AssetRef::path(region_material::PALETTE_PATH).unwrap()
    );
    let (imported, observations) = directory.import_material();
    assert_eq!(imported.generation().inputs().len(), 1);
    assert_eq!(
        imported.generation().inputs()[0].path().as_str(),
        region_material::SHADER_PATH
    );
    assert_eq!(
        imported.generation().inputs()[0].source(),
        region_material::SHADER
    );
    let material = Material2dRuntimeCodec::decode_runtime(
        &Material2dRuntimeCodec::encode_runtime(&imported).unwrap(),
    )
    .unwrap();
    assert_eq!(material.product(), region_material::material_product());
    assert_eq!(
        material.textures()[region_material::PALETTE_ID],
        AssetRef::Product(region_material::palette_product())
    );
    assert!(material.generation().inputs().is_empty());
    assert_eq!(
        material.generation().shader().user_source(),
        format!("{}\n", region_material::SHADER)
    );
    let expected = [
        "assets/materials/region.material2d.json",
        "assets/materials/region.wgsl",
    ]
    .into_iter()
    .map(|path| {
        (
            path.to_owned(),
            ArtifactContentDigest::of_bytes(&fs::read(directory.0.join(path)).unwrap()),
        )
    })
    .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(observations.len(), expected.len());
    assert_eq!(
        observations
            .iter()
            .map(|observation| match observation {
                ArtifactBuildObservation::Raw {
                    path,
                    digest: Some(digest),
                } => (path.as_str().to_owned(), *digest),
                other => panic!("expected the original Material and WGSL observations: {other:?}"),
            })
            .collect::<std::collections::BTreeMap<_, _>>(),
        expected
    );
    fence.bind_raw_inputs(&observations).unwrap();
    let inputs = source::code_inventory(&directory.0).unwrap();
    assert!(inputs.iter().any(|path| {
        path == &nara::fs::RelativePath::new("assets/materials/region.wgsl").unwrap()
    }));
    fence.bind_code(&inputs).unwrap();
    fence.verify().unwrap();
    let shader_path = directory.0.join("assets/materials/region.wgsl");
    fs::write(&shader_path, "changed shader after capture").unwrap();
    assert!(
        fence
            .verify()
            .unwrap_err()
            .contains("assets/materials/region.wgsl")
    );
    assert_eq!(
        material.generation().shader().user_source(),
        format!("{}\n", region_material::SHADER)
    );
}

#[test]
fn shader_changed_before_import_cannot_rebaseline_the_original_capture() {
    let directory = Directory::material();
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    fs::write(
        directory.0.join("assets/materials/region.wgsl"),
        format!("{}\n", region_material::SHADER),
    )
    .unwrap();
    let (_, observations) = directory.import_material();
    assert!(
        fence
            .bind_raw_inputs(&observations)
            .unwrap_err()
            .contains("assets/materials/region.wgsl")
    );
    let inputs = source::code_inventory(&directory.0).unwrap();
    assert!(
        fence
            .bind_code(&inputs)
            .unwrap_err()
            .contains("assets/materials/region.wgsl")
    );
}

#[test]
fn shader_replacement_with_identical_bytes_cannot_replace_the_original_file_identity() {
    let directory = Directory::material();
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    let (_, observations) = directory.import_material();
    let shader = directory.0.join("assets/materials/region.wgsl");
    let bytes = fs::read(&shader).unwrap();
    fs::rename(&shader, shader.with_extension("original")).unwrap();
    fs::write(&shader, bytes).unwrap();
    assert!(
        fence
            .bind_raw_inputs(&observations)
            .unwrap_err()
            .contains("assets/materials/region.wgsl")
    );
    let inputs = source::code_inventory(&directory.0).unwrap();
    assert!(
        fence
            .bind_code(&inputs)
            .unwrap_err()
            .contains("assets/materials/region.wgsl")
    );
}

#[test]
fn tileset_image_reference_requires_an_admitted_image_identity() {
    let directory = Directory::tileset();
    let root = host_fs::readable_directory(&directory.0).unwrap();
    let scope = AssetPathScope::under(AssetSourcePath::new("assets").unwrap());
    for kind in [None, Some(AssetSourceKind::Scene)] {
        let mut database = ProjectAssetDatabase::default();
        if let Some(kind) = kind {
            let path = AssetPath::new(side_room::IMAGE_PATH).unwrap();
            database
                .insert_identity(
                    &scope,
                    path.clone(),
                    AssetMetaIdentity::new(side_room::image_product().source(), path, kind),
                )
                .unwrap();
        }
        let error = tileset::prepare(&root, &scope, &mut database, &mut Vec::new()).unwrap_err();
        assert!(error.contains("TileSet"), "{error}");
        assert!(error.contains("Image"), "{error}");
    }
}

#[test]
fn tileset_source_resolves_the_actual_required_image_and_stays_fenced_through_publication() {
    let directory = Directory::tileset();
    let path = directory.0.join("assets/tilesets/region.tileset.ron");
    let bytes = fs::read(&path).unwrap();
    let authored =
        TileSetSource::decode_ron_bytes(&bytes, TileSetSourceFileLimits::default()).unwrap();
    assert_eq!(
        authored.image,
        AssetRef::path(side_room::IMAGE_PATH).unwrap()
    );
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    let (descriptor, observations) = directory.resolve_tileset();
    assert_eq!(descriptor, region_tileset::descriptor());
    assert_eq!(observations.len(), 1);
    assert!(matches!(
        &observations[0],
        ArtifactBuildObservation::Raw { path, digest: Some(digest) }
            if path.as_str() == "assets/tilesets/region.tileset.ron"
                && *digest == ArtifactContentDigest::of_bytes(&bytes)
    ));
    let cooked = TileSetRuntimeCodec::decode_runtime(
        &TileSetRuntimeCodec::encode_runtime(&descriptor).unwrap(),
    )
    .unwrap();
    assert_eq!(cooked, descriptor);
    let mut dependencies = Vec::new();
    TileSetRuntimeCodec::visit_runtime_dependencies(&cooked, |product, demand| {
        dependencies.push((product, demand));
        Ok::<_, std::convert::Infallible>(())
    })
    .unwrap();
    assert_eq!(
        dependencies,
        [(
            side_room::image_product(),
            nara::asset::AssetReferenceDemand::Immediate,
        )]
    );
    fence.bind_raw_inputs(&observations).unwrap();
    let inputs = source::code_inventory(&directory.0).unwrap();
    fence.bind_code(&inputs).unwrap();
    fence.verify().unwrap();
    fs::write(&path, "changed TileSet after capture").unwrap();
    assert!(
        fence
            .verify()
            .unwrap_err()
            .contains("assets/tilesets/region.tileset.ron")
    );
    fs::write(&path, bytes).unwrap();
    fence.verify().unwrap();
    assert_eq!(cooked, region_tileset::descriptor());
}

#[test]
fn tileset_changes_before_resolution_cannot_rebaseline_the_original_capture() {
    let directory = Directory::tileset();
    let mut fence = source::ContentFence::capture(&directory.0).unwrap();
    let path = directory.0.join("assets/tilesets/region.tileset.ron");
    let mut authored = TileSetSource::decode_ron_bytes(
        &fs::read(&path).unwrap(),
        TileSetSourceFileLimits::default(),
    )
    .unwrap();
    authored.tint = nara::core::Color::rgba(0.5, 1.0, 1.0, 1.0);
    fs::write(&path, authored.encode_ron_bytes().unwrap()).unwrap();
    let (_, observations) = directory.resolve_tileset();
    assert!(
        fence
            .bind_raw_inputs(&observations)
            .unwrap_err()
            .contains("assets/tilesets/region.tileset.ron")
    );
    let inputs = source::code_inventory(&directory.0).unwrap();
    assert!(
        fence
            .bind_code(&inputs)
            .unwrap_err()
            .contains("assets/tilesets/region.tileset.ron")
    );
}

#[test]
fn identical_tileset_source_and_metadata_replacements_preserve_original_file_identity() {
    for name in [
        "assets/tilesets/region.tileset.ron",
        "assets/tilesets/region.tileset.ron.meta",
    ] {
        let directory = Directory::tileset();
        let mut fence = source::ContentFence::capture(&directory.0).unwrap();
        let (_, observations) = directory.resolve_tileset();
        let path = directory.0.join(name);
        let bytes = fs::read(&path).unwrap();
        fs::rename(&path, path.with_extension("original")).unwrap();
        fs::write(&path, bytes).unwrap();
        assert!(
            fence
                .bind_raw_inputs(&observations)
                .unwrap_err()
                .contains(name)
        );
        assert!(fence.verify().unwrap_err().contains(name));
    }
}
