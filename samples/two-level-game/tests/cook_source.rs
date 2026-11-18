#![cfg(any(feature = "development", feature = "cook"))]

#[path = "../src/bin/cook/source.rs"]
mod source;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara::{
    asset::{AssetPath, AssetPathScope, AssetSourcePath, SourceAssetBuild},
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildObservation, ArtifactBuildStore,
        ArtifactContentDigest, ArtifactSourceRequest, VerifiedArtifactGroup, build_source,
    },
    core::FormatVersion,
    project_tooling::host_fs,
};
use two_level_game::sound_bank::{BANK_PATH, SoundBankDomain};

const RENAMED_PICKUP: &str = "assets/audio/pickup-v2.wav";

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
            ("prefabs/marker.prefab.json", "marker prefab"),
            ("assets/textures/meadow.png", "image A"),
            ("assets/textures/meadow.png.meta", "meta A"),
            ("assets/textures/cavern.png", "image B"),
            ("assets/textures/cavern.png.meta", "meta B"),
            ("assets/textures/explorer.png", "explorer atlas"),
            ("assets/textures/explorer.png.meta", "explorer metadata"),
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
            ("src/bin/player/generated.rs", "expected catalog"),
        ] {
            let target = path.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, bytes).unwrap();
        }
        Self(path)
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
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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
        "assets/animations/explorer.sprite-animations.json",
        "assets/animations/explorer.sprite-animations.json.meta",
    ] {
        let directory = Directory::new();
        let mut fence = source::ContentFence::capture(&directory.0).unwrap();
        let inputs = source::code_inventory(&directory.0).unwrap();
        fs::write(directory.0.join(name), "changed").unwrap();
        assert!(fence.bind_code(&inputs).unwrap_err().contains(name));
    }
}

#[test]
fn content_fence_requires_input_animation_and_marker_sources_and_metadata() {
    for name in [
        "input/explorer.actions.json",
        "scenes/marker.scene.json",
        "scenes/marker.scene.json.meta",
        "prefabs/marker.prefab.json",
        "assets/textures/explorer.png",
        "assets/textures/explorer.png.meta",
        "assets/animations/explorer.sprite-animations.json",
        "assets/animations/explorer.sprite-animations.json.meta",
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
fn code_and_generated_bootstrap_stay_fenced_through_later_publication_checks() {
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/lib.rs",
        "src/bin/player/generated.rs",
        "prefabs/marker.prefab.json",
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
    fence.bind_code(&inputs).unwrap();
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
