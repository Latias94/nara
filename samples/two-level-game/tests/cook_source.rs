//! Source selections and typed dependency controls use the public production project cooker.

#![cfg(any(feature = "development", feature = "cook"))]

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use nara::{
    asset::{
        AssetPath, AssetPathScope, AssetRef, AssetReferenceDemand, AssetSourcePath,
        RuntimeAssetCodec, SourceAssetBuild,
    },
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildLimits, ArtifactBuildObservation, ArtifactBuildStore,
        ArtifactContentDigest, ArtifactSourceRequest, VerifiedArtifactGroup, build_source,
    },
    core::FormatVersion,
    fs::{ContentDigest, FsError, RelativePath},
    material::{Material2dDomain, Material2dRuntimeCodec, Material2dSource},
    project::ProjectManifest,
    project_host::{ProjectContentErrorKind, ProjectContentMemberKind, ingest_project_manifest},
    project_tooling::{
        cargo_build::{CargoBuildFailure, CargoBuildLimits, CargoBuildRequest, CargoBuildTools},
        host_fs,
        player_package::{GeneratedPlayerBootstrap, PlayerPackageLimits},
        project_cook::{
            PreparedProjectCook, ProjectCookContext, ProjectCookError, ProjectCookRequest,
            prepare_project_cooked_content, project_rust_input_inventory,
        },
    },
    project_workflow::{cargo::CargoBuildSelection, cooked_content::CookedContentLimits},
    runtime_content::{
        RuntimeContentDependency, RuntimeContentDependencyRole, RuntimeContentTarget,
    },
    tilemap::{TileSetDomain, TileSetRuntimeCodec, TileSetSource, TileSetSourceFileLimits},
};
use nara_sprite_animation::SpriteAnimationDomain;
use two_level_game::{
    delivery::{ControllerBackend, PlayerSelection, RuntimeKind},
    input::INPUT_PATH,
    levels::LevelId,
    region_material, region_tileset, side_room,
    sound_bank::{BANK_PATH, SoundBankCodec, SoundBankDomain, SoundCue, bank_product},
};

const RENAMED_PICKUP: &str = "assets/audio/pickup-v2.wav";
const SELECTED_SCHEMA: &str = "authoring/selected.bundle.json";
const TRIPLE: &str = "x86_64-pc-windows-msvc";

struct Directory {
    parent: PathBuf,
    project: PathBuf,
}

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        let parent = temporary.join(format!(
            "nara-cook-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&parent).unwrap();
        let directory = Self {
            project: parent.join("project"),
            parent,
        };
        let sample = Path::new(env!("CARGO_MANIFEST_DIR"));
        for name in [
            "nara.toml",
            INPUT_PATH,
            "schema/authoring.bundle.json",
            "scenes/meadow.scene.json",
            "scenes/meadow.scene.json.meta",
            "scenes/cavern.scene.json",
            "scenes/cavern.scene.json.meta",
            "scenes/marker.scene.json",
            "scenes/marker.scene.json.meta",
            "scenes/side-room.scene.json",
            "scenes/side-room.scene.json.meta",
            "scenes/lookout.scene.json",
            "scenes/lookout.scene.json.meta",
            "prefabs/marker.prefab.json",
            "assets/textures/meadow.png",
            "assets/textures/meadow.png.meta",
            "assets/textures/cavern.png",
            "assets/textures/cavern.png.meta",
            "assets/textures/explorer.png",
            "assets/textures/explorer.png.meta",
            "assets/textures/marker.png",
            "assets/textures/marker.png.meta",
            "assets/textures/side-room.png",
            "assets/textures/side-room.png.meta",
            "assets/materials/region.material2d.json",
            "assets/materials/region.material2d.json.meta",
            "assets/materials/region.wgsl",
            "assets/tilesets/region.tileset.ron",
            "assets/tilesets/region.tileset.ron.meta",
            "assets/animations/explorer.sprite-animations.json",
            "assets/animations/explorer.sprite-animations.json.meta",
            "assets/audio/game.soundbank.json",
            "assets/audio/game.soundbank.json.meta",
            "assets/audio/pickup.wav",
            "assets/audio/travel.wav",
        ] {
            let target = directory.project.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(sample.join(name), target).unwrap();
        }
        for (name, bytes) in [
            ("Cargo.toml", "package"),
            ("Cargo.lock", "lock"),
            ("build.rs", "fn main() {}"),
            ("src/lib.rs", "pub fn game() {}"),
            ("src/bin/player.rs", "fn main() {}"),
        ] {
            let target = directory.project.join(name);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, bytes).unwrap();
        }
        fs::create_dir(directory.parent.join("cache")).unwrap();
        directory
    }

    fn selected_schema() -> Self {
        let directory = Self::new();
        let path = directory.project.join("nara.toml");
        let loaded = ProjectManifest::parse_toml_bytes(&fs::read(&path).unwrap());
        assert!(!loaded.has_errors(), "{:?}", loaded.diagnostics);
        let mut manifest = loaded.manifest.unwrap();
        manifest.authoring.schema_bundle = Some(SELECTED_SCHEMA.to_owned());
        let selected = directory.project.join(SELECTED_SCHEMA);
        fs::create_dir_all(selected.parent().unwrap()).unwrap();
        fs::rename(
            directory.project.join("schema/authoring.bundle.json"),
            selected,
        )
        .unwrap();
        fs::write(path, manifest.to_canonical_toml_bytes().unwrap()).unwrap();
        directory
    }

    fn renamed_sound_bank() -> Self {
        let directory = Self::new();
        fs::rename(
            directory.project.join("assets/audio/pickup.wav"),
            directory.project.join(RENAMED_PICKUP),
        )
        .unwrap();
        fs::write(
            directory.project.join("assets/audio/game.soundbank.json"),
            br#"{"pickup":"audio/pickup-v2.wav","travel":"audio/travel.wav"}"#,
        )
        .unwrap();
        directory
    }

    fn store(&self) -> ArtifactBuildStore {
        ArtifactBuildStore::open(
            host_fs::writable_directory(&self.parent.join("cache")).unwrap(),
            ArtifactBuildLimits::default(),
        )
        .unwrap()
    }

    fn prepare(&self) -> Result<PreparedProjectCook, ProjectCookError> {
        self.prepare_with(|cx| {
            cx.add_source_assets::<SoundBankDomain>()?;
            cx.add_source_assets::<SpriteAnimationDomain>()?;
            Ok(())
        })
    }

    fn prepare_with(
        &self,
        contribute: impl FnMut(&mut ProjectCookContext<'_>) -> Result<(), ProjectCookError>,
    ) -> Result<PreparedProjectCook, ProjectCookError> {
        let root = host_fs::readable_directory(&self.project).unwrap();
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        prepare_project_cooked_content(
            ProjectCookRequest {
                root: &root,
                candidate: &candidate,
                recipe: PlayerSelection::new(RuntimeKind::Headless, ControllerBackend::Disabled)
                    .unwrap()
                    .recipe()
                    .unwrap(),
                target: RuntimeContentTarget::new(TRIPLE, "two-level-shipping").unwrap(),
                store: &self.store(),
                implementation: ArtifactContentDigest::of_bytes(b"cook-source-selection-test/v1"),
                limits: CookedContentLimits::default(),
            },
            contribute,
        )
    }

    fn build_sound_bank(&self) -> VerifiedArtifactGroup {
        let request = ArtifactSourceRequest::new(
            host_fs::readable_directory(&self.project.join("assets")).unwrap(),
            AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
            AssetPath::new(BANK_PATH).unwrap(),
            ArtifactBuildDefinition {
                id: SoundBankDomain::BUILD_ID.to_owned(),
                revision: FormatVersion::new(SoundBankDomain::BUILD_REVISION).unwrap(),
                implementation: ArtifactContentDigest::of_bytes(b"cook-source-selection-test/v1"),
            },
        );
        build_source::<SoundBankDomain>(&self.store(), &request, |_| Ok(()))
            .unwrap()
            .into_group()
    }

    fn request(&self, inputs: Vec<RelativePath>) -> CargoBuildRequest {
        let cargo = self
            .parent
            .join(format!("cargo{}", std::env::consts::EXE_SUFFIX));
        let rustc = self
            .parent
            .join(format!("rustc{}", std::env::consts::EXE_SUFFIX));
        for path in [&cargo, &rustc] {
            fs::write(path, b"unexecuted tool identity").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        CargoBuildRequest::new(
            &self.project,
            RelativePath::new("Cargo.toml").unwrap(),
            CargoBuildSelection::new(
                "two-level-game",
                "player",
                "release",
                TRIPLE,
                ["cooked-player"],
                false,
            )
            .unwrap(),
            CargoBuildTools::new(cargo, rustc).unwrap(),
            inputs,
            CargoBuildLimits::default(),
        )
        .unwrap()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        let resolved = self.parent.canonicalize().unwrap();
        assert_eq!(resolved, self.parent);
        assert_eq!(resolved.parent(), Some(temporary.as_path()));
        assert!(
            resolved
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("nara-cook-source-")
        );
        fs::remove_dir_all(resolved).unwrap();
    }
}

fn assert_raw(prepared: &PreparedProjectCook, name: &str, expected: Option<&[u8]>) {
    let observations = prepared
        .observations()
        .iter()
        .filter_map(|observation| match observation {
            ArtifactBuildObservation::Raw { path, digest } if path.as_str() == name => {
                Some(*digest)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!observations.is_empty(), "missing observed input {name}");
    assert!(
        observations
            .iter()
            .all(|digest| *digest == expected.map(ArtifactContentDigest::of_bytes)),
        "{name}: {observations:?}"
    );
}

#[test]
fn selected_schema_is_retained_without_the_default_bundle() {
    let directory = Directory::selected_schema();
    let default = directory.project.join("schema/authoring.bundle.json");
    assert!(!default.exists());
    let selected = directory.project.join(SELECTED_SCHEMA);
    let bytes = fs::read(&selected).unwrap();
    let prepared = directory.prepare().unwrap();
    assert_raw(&prepared, SELECTED_SCHEMA, Some(&bytes));
    assert_eq!(
        prepared
            .source()
            .members()
            .iter()
            .filter(|member| { member.kind() == ProjectContentMemberKind::AuthoringSchemaBundle })
            .map(|member| member.path().as_str())
            .collect::<Vec<_>>(),
        [SELECTED_SCHEMA]
    );
    let (_, selection) = prepared.into_parts();
    fs::write(&default, b"unselected default bundle").unwrap();
    selection.verify(&directory.project).unwrap();
    fs::write(&selected, b"changed selected bundle").unwrap();
    assert!(matches!(
        selection.verify(&directory.project),
        Err(ProjectCookError::Filesystem(FsError::DigestMismatch { .. }))
    ));
    fs::write(&selected, &bytes).unwrap();
    selection.verify(&directory.project).unwrap();
    fs::remove_file(&selected).unwrap();
    assert!(matches!(
        selection.verify(&directory.project),
        Err(ProjectCookError::Filesystem(FsError::Io { source, .. }))
            if source.kind() == std::io::ErrorKind::NotFound
    ));
    assert!(default.is_file());
}

#[test]
fn source_mutation_during_a_typed_contribution_cannot_rebaseline_selected_schema() {
    for missing in [false, true] {
        let directory = Directory::selected_schema();
        let selected = directory.project.join(SELECTED_SCHEMA);
        let mut changed = false;
        let error = directory
            .prepare_with(|cx| {
                cx.add_source_assets::<SoundBankDomain>()?;
                cx.add_source_assets::<SpriteAnimationDomain>()?;
                if !changed {
                    changed = true;
                    if missing {
                        fs::remove_file(&selected).unwrap();
                    } else {
                        fs::write(&selected, b"changed selected schema during preparation")
                            .unwrap();
                    }
                }
                Ok(())
            })
            .err()
            .expect("stale delivery must not produce a prepared source selection");
        assert!(changed);
        assert!(
            matches!(
                error,
                ProjectCookError::Source(_) | ProjectCookError::Filesystem(_)
            ),
            "{error:?}"
        );
        assert!(!directory.project.join("player-bootstrap.rs").exists());
    }
}

#[test]
fn optional_input_metadata_presence_and_bytes_remain_in_the_original_selection() {
    let directory = Directory::new();
    let path = directory.project.join(format!("{INPUT_PATH}.meta"));
    let absent = directory.prepare().unwrap();
    assert_raw(&absent, &format!("{INPUT_PATH}.meta"), None);
    assert_eq!(
        absent
            .observations()
            .iter()
            .filter(|observation| matches!(
                observation,
                ArtifactBuildObservation::Raw { path, digest: None }
                    if path.as_str() == format!("{INPUT_PATH}.meta")
            ))
            .count(),
        1
    );
    let (_, absent) = absent.into_parts();
    absent.verify(&directory.project).unwrap();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "kind": "asset_meta", "format_version": 1, "engine_min_version": "0.1.0",
        "generator": {"name": "nara", "version": "0.1.0"},
        "payload": {
            "asset": {
                "stable_id": two_level_game::input::INPUT_SOURCE_ID,
                "path": INPUT_PATH,
                "source_kind": {"other": "input-action-map"}
            },
            "settings": {}
        }
    }))
    .unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        absent.verify(&directory.project),
        Err(ProjectCookError::Filesystem(FsError::TargetStateMismatch))
    ));
    let present = directory.prepare().unwrap();
    assert_raw(&present, &format!("{INPUT_PATH}.meta"), Some(&bytes));
    let (_, present) = present.into_parts();
    present.verify(&directory.project).unwrap();
    fs::write(&path, b"modified input metadata").unwrap();
    assert!(matches!(
        present.verify(&directory.project),
        Err(ProjectCookError::Filesystem(FsError::DigestMismatch { .. }))
    ));
    fs::write(&path, &bytes).unwrap();
    present.verify(&directory.project).unwrap();
    fs::rename(&path, path.with_extension("original")).unwrap();
    assert!(matches!(
        present.verify(&directory.project),
        Err(ProjectCookError::Filesystem(FsError::Io { .. }))
    ));
    absent.verify(&directory.project).unwrap();
}

#[test]
fn original_delivery_and_build_inputs_stay_selected_after_bootstrap_generation() {
    let directory = Directory::new();
    let prepared = directory.prepare().unwrap();
    let bootstrap = GeneratedPlayerBootstrap::new(
        prepared.content(),
        "player-bootstrap.rs",
        PlayerPackageLimits::default(),
    )
    .unwrap();
    assert_eq!(prepared.content().base().catalog().members().len(), 15);
    let (_, selection) = prepared.into_parts();
    for name in [
        "nara.toml",
        "schema/authoring.bundle.json",
        INPUT_PATH,
        "scenes/meadow.scene.json",
        "scenes/meadow.scene.json.meta",
        "scenes/cavern.scene.json",
        "scenes/cavern.scene.json.meta",
        "scenes/marker.scene.json",
        "scenes/marker.scene.json.meta",
        "prefabs/marker.prefab.json",
        "scenes/side-room.scene.json",
        "scenes/side-room.scene.json.meta",
        "scenes/lookout.scene.json",
        "scenes/lookout.scene.json.meta",
        "assets/textures/meadow.png",
        "assets/textures/meadow.png.meta",
        "assets/textures/cavern.png",
        "assets/textures/cavern.png.meta",
        "assets/textures/explorer.png",
        "assets/textures/explorer.png.meta",
        "assets/textures/marker.png",
        "assets/textures/marker.png.meta",
        "assets/textures/side-room.png",
        "assets/textures/side-room.png.meta",
        "assets/animations/explorer.sprite-animations.json",
        "assets/animations/explorer.sprite-animations.json.meta",
        "assets/materials/region.material2d.json",
        "assets/materials/region.material2d.json.meta",
        "assets/materials/region.wgsl",
        "assets/tilesets/region.tileset.ron",
        "assets/tilesets/region.tileset.ron.meta",
        "assets/audio/game.soundbank.json",
        "assets/audio/game.soundbank.json.meta",
        "assets/audio/pickup.wav",
        "assets/audio/travel.wav",
    ] {
        let path = directory.project.join(name);
        let bytes = fs::read(&path).unwrap();
        let mut changed = bytes.clone();
        changed[0] ^= 1;
        fs::write(&path, changed).unwrap();
        assert!(
            matches!(
                selection.verify(&directory.project),
                Err(ProjectCookError::Filesystem(FsError::DigestMismatch { .. }))
            ),
            "{name}"
        );
        fs::write(&path, bytes).unwrap();
        selection.verify(&directory.project).unwrap();
    }
    assert_eq!(
        bootstrap.digest(),
        ContentDigest::of_bytes(bootstrap.bytes())
    );
    assert!(!directory.project.join(bootstrap.name()).exists());
    assert!(
        !directory
            .project
            .join("src/bin/player/generated.rs")
            .exists()
    );
}

#[test]
fn missing_authored_sources_and_metadata_refuse_a_new_cook() {
    let directory = Directory::new();
    for name in [
        INPUT_PATH,
        "schema/authoring.bundle.json",
        "scenes/meadow.scene.json.meta",
        "scenes/cavern.scene.json.meta",
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
        let path = directory.project.join(name);
        let original = path.with_extension("original");
        fs::rename(&path, &original).unwrap();
        let error = directory.prepare().err();
        fs::rename(original, path).unwrap();
        match error.expect("a missing required source must reject cooking") {
            ProjectCookError::Invalid(message) if name == "scenes/meadow.scene.json.meta" => {
                assert_eq!(message, "cooked startup requires authored stable metadata");
            }
            ProjectCookError::Invalid(message) if name == "scenes/cavern.scene.json.meta" => {
                assert_eq!(message, "cooked Scenes require authored stable metadata");
            }
            ProjectCookError::Source(error) => {
                assert!(!error.diagnostics().is_empty(), "{name}: {error:?}");
            }
            error => panic!("unexpected missing-source refusal for {name}: {error:?}"),
        }
    }
    let prepared = directory.prepare().unwrap();
    assert_eq!(prepared.content().base().catalog().members().len(), 15);
}

#[test]
fn rust_inventory_keeps_compiler_inputs_explicit_and_never_selects_bootstrap_bytes() {
    let directory = Directory::new();
    fs::write(directory.project.join("src/notes.txt"), "not compiled Rust").unwrap();
    let inventory = project_rust_input_inventory(&directory.project).unwrap();
    assert_eq!(
        inventory,
        [
            "Cargo.lock",
            "Cargo.toml",
            "build.rs",
            "src/bin/player.rs",
            "src/lib.rs"
        ]
        .map(|name| RelativePath::new(name).unwrap())
    );
    let shader = RelativePath::new("assets/materials/region.wgsl").unwrap();
    assert!(!inventory.contains(&shader));
    let mut inputs = inventory;
    inputs.push(shader.clone());
    let request = directory.request(inputs);
    assert!(request.declared_input_paths().contains(&shader));
    assert!(request.generated_rust_input().is_none());
    let prepared = directory.prepare().unwrap();
    let generated = GeneratedPlayerBootstrap::new(
        prepared.content(),
        "player-bootstrap.rs",
        PlayerPackageLimits::default(),
    )
    .unwrap();
    let request = request
        .with_generated_rust_input(generated.name(), generated.bytes())
        .unwrap();
    assert_eq!(
        request.generated_rust_input().unwrap().path(),
        generated.name()
    );
    assert!(
        !request
            .declared_input_paths()
            .contains(&RelativePath::new(generated.name()).unwrap())
    );
    assert!(!directory.project.join(generated.name()).exists());
}

#[test]
fn changed_rust_input_sets_reject_the_original_declared_request_before_cargo() {
    for name in [
        "src/lib.rs",
        "src/bin/player.rs",
        "build.rs",
        "src/later.rs",
    ] {
        let directory = Directory::new();
        let mut inputs = project_rust_input_inventory(&directory.project).unwrap();
        if name == "src/later.rs" {
            fs::write(directory.project.join(name), "pub struct Later;").unwrap();
        } else {
            inputs.retain(|path| path != &RelativePath::new(name).unwrap());
        }
        let request = directory.request(inputs);
        let error = directory
            .prepare()
            .unwrap()
            .build_player(request, PlayerPackageLimits::default())
            .err()
            .expect("missing declared Rust input must fail before any tool runs");
        assert!(
            matches!(
                error,
                ProjectCookError::Cargo(CargoBuildFailure::MissingDeclaredInput)
            ),
            "{name}: {error:?}"
        );
    }
}

#[test]
fn changed_source_refuses_build_player_before_tool_execution() {
    for name in [
        "prefabs/marker.prefab.json",
        "assets/materials/region.wgsl",
        "assets/tilesets/region.tileset.ron",
    ] {
        let directory = Directory::new();
        let prepared = directory.prepare().unwrap();
        let request = directory.request(project_rust_input_inventory(&directory.project).unwrap());
        fs::write(directory.project.join(name), b"changed source").unwrap();
        let error = prepared
            .build_player(request, PlayerPackageLimits::default())
            .err()
            .expect("the original preparation source must reject before Cargo");
        assert!(
            matches!(
                error,
                ProjectCookError::Filesystem(FsError::DigestMismatch { .. })
            ),
            "{name}: {error:?}"
        );
    }
}

#[test]
fn renamed_sound_bank_dependency_cooks_without_its_old_path_and_decodes_source_free() {
    let directory = Directory::renamed_sound_bank();
    assert!(!directory.project.join("assets/audio/pickup.wav").exists());
    let group = directory.build_sound_bank();
    let prepared = directory
        .prepare_with(|cx| {
            cx.add_group(&group)?;
            cx.add_source_assets::<SpriteAnimationDomain>()?;
            Ok(())
        })
        .unwrap();
    assert_raw(
        &prepared,
        RENAMED_PICKUP,
        Some(&fs::read(directory.project.join(RENAMED_PICKUP)).unwrap()),
    );
    assert!(prepared.observations().iter().all(|observation| !matches!(
        observation,
        ArtifactBuildObservation::Raw { path, .. } if path.as_str() == "assets/audio/pickup.wav"
    )));
    let (content, selection) = prepared.into_parts();
    selection.verify(&directory.project).unwrap();
    let project = directory.project.clone();
    drop(selection);
    drop(group);
    drop(directory);
    assert!(!project.exists());
    let bank = content
        .base()
        .acquire_asset::<SoundBankDomain, SoundBankCodec>(bank_product())
        .unwrap();
    assert_eq!(bank.product().product(), bank_product());
    assert_eq!(
        [
            bank.product().samples(SoundCue::Pickup).len(),
            bank.product().samples(SoundCue::Travel).len(),
        ],
        [5760, 11520]
    );
}

#[test]
fn edited_raw_dependency_is_rejected_after_the_public_selection_is_returned() {
    let directory = Directory::renamed_sound_bank();
    let prepared = directory.prepare().unwrap();
    let bytes = fs::read(directory.project.join(RENAMED_PICKUP)).unwrap();
    assert_raw(&prepared, RENAMED_PICKUP, Some(&bytes));
    let (_, selection) = prepared.into_parts();
    let mut changed = bytes.clone();
    *changed.last_mut().unwrap() ^= 1;
    fs::write(directory.project.join(RENAMED_PICKUP), changed).unwrap();
    assert!(matches!(
        selection.verify(&directory.project),
        Err(ProjectCookError::Filesystem(FsError::DigestMismatch { .. }))
    ));
    fs::write(directory.project.join(RENAMED_PICKUP), bytes).unwrap();
    selection.verify(&directory.project).unwrap();
}

#[test]
fn stale_build_group_cannot_rebaseline_new_source_delivery() {
    let directory = Directory::renamed_sound_bank();
    let group = directory.build_sound_bank();
    let path = directory.project.join(RENAMED_PICKUP);
    let mut bytes = fs::read(&path).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(path, bytes).unwrap();
    let error = directory
        .prepare_with(|cx| {
            cx.add_group(&group)?;
            cx.add_source_assets::<SpriteAnimationDomain>()?;
            Ok(())
        })
        .err()
        .expect("the actual old build observations must disagree with current delivery");
    assert!(
        matches!(
            error,
            ProjectCookError::Invalid(ref message) if message == &format!("source selection changed: {RENAMED_PICKUP}")
        ),
        "{error:?}"
    );
    let prepared = directory.prepare().unwrap();
    let (_, selection) = prepared.into_parts();
    selection.verify(&directory.project).unwrap();
}

#[test]
fn identical_source_replacement_keeps_the_original_selected_file_identity() {
    let directory = Directory::renamed_sound_bank();
    let (_, selection) = directory.prepare().unwrap().into_parts();
    let path = directory.project.join("assets/audio/game.soundbank.json");
    let bytes = fs::read(&path).unwrap();
    let root = host_fs::readable_directory(&directory.project).unwrap();
    let relative = RelativePath::new("assets/audio/game.soundbank.json").unwrap();
    let original = root.open_file(&relative).unwrap();
    fs::rename(&path, path.with_extension("original")).unwrap();
    fs::write(&path, &bytes).unwrap();
    let replacement = root.open_file(&relative).unwrap();
    assert_ne!(original.identity(), replacement.identity());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(matches!(
        selection.verify(&directory.project),
        Err(ProjectCookError::Filesystem(FsError::IdentityMismatch { expected, actual }))
            if expected != actual
    ));
}

#[test]
fn material_texture_path_requires_an_admitted_image_identity() {
    for missing in [false, true] {
        let directory = Directory::new();
        let path = directory
            .project
            .join("assets/materials/region.material2d.json");
        let bytes = fs::read(&path).unwrap();
        let authored = Material2dSource::decode(&bytes).unwrap();
        let mut textures = authored.textures().clone();
        textures.insert(
            region_material::PALETTE_ID.to_owned(),
            if missing {
                AssetRef::path("textures/missing.png").unwrap()
            } else {
                AssetRef::path(region_tileset::TILESET_PATH).unwrap()
            },
        );
        let invalid = Material2dSource::new(
            authored.declaration().clone(),
            authored.shader_paths().to_vec(),
            authored.default_parameters().clone(),
            textures,
        )
        .unwrap();
        fs::write(path, invalid.encode().unwrap()).unwrap();
        let error = directory
            .prepare()
            .err()
            .expect("a Material texture must resolve to an admitted Image");
        let ProjectCookError::Source(error) = error else {
            panic!("expected the original Material dependency refusal: {error:?}");
        };
        assert_eq!(
            error.kind(),
            if missing {
                ProjectContentErrorKind::HostIo
            } else {
                ProjectContentErrorKind::AssetReference
            }
        );
        let code = if missing {
            "project.content.host-io"
        } else {
            "project.content.dependency-kind-mismatch"
        };
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == code),
            "{error:?}"
        );
    }
}

#[test]
fn material_shader_uses_original_admitted_inputs_and_decodes_without_source() {
    let directory = Directory::new();
    let path = directory
        .project
        .join("assets/materials/region.material2d.json");
    let authored = Material2dSource::decode(&fs::read(path).unwrap()).unwrap();
    assert_eq!(
        authored.textures()[region_material::PALETTE_ID],
        AssetRef::path(region_material::PALETTE_PATH).unwrap()
    );
    let mut imported = false;
    let prepared = directory
        .prepare_with(|cx| {
            if let Some(materials) = cx
                .snapshot()
                .domain_assets()
                .collection::<Material2dDomain>()
            {
                assert_eq!(materials.iter().len(), 1);
                let material = materials.iter().next().unwrap();
                assert_eq!(material.generation().inputs().len(), 1);
                assert_eq!(
                    material.generation().inputs()[0].path().as_str(),
                    region_material::SHADER_PATH
                );
                assert_eq!(
                    material.generation().inputs()[0].source(),
                    region_material::SHADER
                );
                imported = true;
            }
            cx.add_source_assets::<SoundBankDomain>()?;
            cx.add_source_assets::<SpriteAnimationDomain>()?;
            Ok(())
        })
        .unwrap();
    assert!(imported);
    let expected = [
        "assets/materials/region.material2d.json",
        "assets/materials/region.wgsl",
    ]
    .map(|name| {
        (
            name,
            ContentDigest::of_bytes(&fs::read(directory.project.join(name)).unwrap()),
        )
    })
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    assert_eq!(
        prepared
            .source()
            .members()
            .iter()
            .filter(|member| expected.contains_key(member.path().as_str()))
            .map(|member| (member.path().as_str(), member.digest()))
            .collect::<BTreeMap<_, _>>(),
        expected
    );
    for name in expected.keys() {
        assert_raw(
            &prepared,
            name,
            Some(&fs::read(directory.project.join(name)).unwrap()),
        );
    }
    let member = prepared
        .content()
        .base()
        .catalog()
        .member_for_product(region_material::material_product())
        .unwrap();
    assert_eq!(
        member.dependencies(),
        [RuntimeContentDependency::new(
            region_material::palette_product(),
            RuntimeContentDependencyRole::Required,
        )]
    );
    let (content, selection) = prepared.into_parts();
    let shader = directory.project.join("assets/materials/region.wgsl");
    fs::write(shader, "changed shader after capture").unwrap();
    assert!(matches!(
        selection.verify(&directory.project),
        Err(ProjectCookError::Filesystem(FsError::DigestMismatch { .. }))
    ));
    drop(selection);
    drop(directory);
    let material = content
        .base()
        .acquire_asset::<Material2dDomain, Material2dRuntimeCodec>(
            region_material::material_product(),
        )
        .unwrap();
    let material = material.product();
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
}

#[test]
fn shader_changed_during_preparation_cannot_replace_original_delivery_bytes() {
    let directory = Directory::new();
    let shader = directory.project.join("assets/materials/region.wgsl");
    let mut changed = false;
    let error = directory
        .prepare_with(|cx| {
            cx.add_source_assets::<SoundBankDomain>()?;
            cx.add_source_assets::<SpriteAnimationDomain>()?;
            if !changed
                && cx
                    .snapshot()
                    .domain_assets()
                    .collection::<Material2dDomain>()
                    .is_some()
            {
                fs::write(&shader, format!("{}\n", region_material::SHADER)).unwrap();
                changed = true;
            }
            Ok(())
        })
        .err()
        .expect("the prepared shader and its original delivery bytes must agree");
    assert!(changed);
    assert!(
        matches!(
            error,
            ProjectCookError::Source(_) | ProjectCookError::Filesystem(_)
        ),
        "{error:?}"
    );
}

#[test]
fn identical_shader_replacement_cannot_replace_the_original_selected_file_identity() {
    let directory = Directory::new();
    let (_, selection) = directory.prepare().unwrap().into_parts();
    let shader = directory.project.join("assets/materials/region.wgsl");
    let bytes = fs::read(&shader).unwrap();
    fs::rename(&shader, shader.with_extension("original")).unwrap();
    fs::write(&shader, bytes).unwrap();
    assert!(matches!(
        selection.verify(&directory.project),
        Err(ProjectCookError::Filesystem(
            FsError::IdentityMismatch { .. }
        ))
    ));
}

#[test]
fn tileset_image_reference_requires_an_admitted_image_identity() {
    for missing in [false, true] {
        let directory = Directory::new();
        let path = directory.project.join("assets/tilesets/region.tileset.ron");
        let mut authored = TileSetSource::decode_ron_bytes(
            &fs::read(&path).unwrap(),
            TileSetSourceFileLimits::default(),
        )
        .unwrap();
        authored.image = if missing {
            AssetRef::path("textures/missing.png").unwrap()
        } else {
            AssetRef::Product(region_material::material_product())
        };
        fs::write(path, authored.encode_ron_bytes().unwrap()).unwrap();
        let error = directory
            .prepare()
            .err()
            .expect("a TileSet must resolve its actual Image");
        let ProjectCookError::Source(error) = error else {
            panic!("expected the original TileSet dependency refusal: {error:?}");
        };
        assert_eq!(
            error.kind(),
            if missing {
                ProjectContentErrorKind::HostIo
            } else {
                ProjectContentErrorKind::AssetMetaMismatch
            }
        );
        let code = if missing {
            "project.content.host-io"
        } else {
            "project.content.stable-asset-reference-unresolved"
        };
        assert!(
            error
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == code),
            "{error:?}"
        );
    }
}

#[test]
fn tileset_preserves_the_actual_required_image_and_decodes_without_source() {
    let directory = Directory::new();
    let path = directory.project.join("assets/tilesets/region.tileset.ron");
    let bytes = fs::read(&path).unwrap();
    let authored =
        TileSetSource::decode_ron_bytes(&bytes, TileSetSourceFileLimits::default()).unwrap();
    assert_eq!(
        authored.image,
        AssetRef::path(side_room::IMAGE_PATH).unwrap()
    );
    let prepared = directory.prepare().unwrap();
    assert_raw(
        &prepared,
        "assets/tilesets/region.tileset.ron",
        Some(&bytes),
    );
    let member = prepared
        .content()
        .base()
        .catalog()
        .member_for_product(region_tileset::tileset_product())
        .unwrap();
    assert_eq!(
        member.dependencies(),
        [RuntimeContentDependency::new(
            side_room::image_product(),
            RuntimeContentDependencyRole::Required,
        )]
    );
    let (content, selection) = prepared.into_parts();
    fs::write(&path, "changed TileSet after capture").unwrap();
    assert!(matches!(
        selection.verify(&directory.project),
        Err(ProjectCookError::Filesystem(FsError::DigestMismatch { .. }))
    ));
    fs::write(&path, bytes).unwrap();
    selection.verify(&directory.project).unwrap();
    drop(selection);
    drop(directory);
    let cooked = content
        .base()
        .acquire_asset::<TileSetDomain, TileSetRuntimeCodec>(region_tileset::tileset_product())
        .unwrap();
    assert_eq!(cooked.product(), &region_tileset::descriptor());
    let mut dependencies = Vec::new();
    TileSetRuntimeCodec::visit_runtime_dependencies(cooked.product(), |product, demand| {
        dependencies.push((product, demand));
        Ok::<_, std::convert::Infallible>(())
    })
    .unwrap();
    assert_eq!(
        dependencies,
        [(side_room::image_product(), AssetReferenceDemand::Immediate)]
    );
}

#[test]
fn tileset_changed_during_preparation_cannot_rebaseline_original_delivery() {
    let directory = Directory::new();
    let path = directory.project.join("assets/tilesets/region.tileset.ron");
    let mut authored = TileSetSource::decode_ron_bytes(
        &fs::read(&path).unwrap(),
        TileSetSourceFileLimits::default(),
    )
    .unwrap();
    authored.tint = nara::core::Color::rgba(0.5, 1.0, 1.0, 1.0);
    let bytes = authored.encode_ron_bytes().unwrap();
    let mut changed = false;
    let error = directory
        .prepare_with(|cx| {
            cx.add_source_assets::<SoundBankDomain>()?;
            cx.add_source_assets::<SpriteAnimationDomain>()?;
            if !changed && !cx.snapshot().tilesets().is_empty() {
                fs::write(&path, &bytes).unwrap();
                changed = true;
            }
            Ok(())
        })
        .err()
        .expect("the compiled TileSet and its original source delivery must agree");
    assert!(changed);
    assert!(
        matches!(
            error,
            ProjectCookError::Source(_) | ProjectCookError::Filesystem(_)
        ),
        "{error:?}"
    );
}

#[test]
fn identical_tileset_source_and_metadata_replacements_preserve_selected_file_identity() {
    let directory = Directory::new();
    let (_, selection) = directory.prepare().unwrap().into_parts();
    for name in [
        "assets/tilesets/region.tileset.ron",
        "assets/tilesets/region.tileset.ron.meta",
    ] {
        let path = directory.project.join(name);
        let bytes = fs::read(&path).unwrap();
        let original = path.with_extension("original");
        fs::rename(&path, &original).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(
            matches!(
                selection.verify(&directory.project),
                Err(ProjectCookError::Filesystem(
                    FsError::IdentityMismatch { .. }
                ))
            ),
            "{name}"
        );
        fs::remove_file(&path).unwrap();
        fs::rename(original, path).unwrap();
        selection.verify(&directory.project).unwrap();
    }
}

#[test]
fn declared_scene_metadata_retains_exact_original_level_products() {
    let directory = Directory::new();
    let prepared = directory.prepare().unwrap();
    let mut scenes = LevelId::ALL.map(LevelId::scene_product).to_vec();
    scenes.sort();
    assert_eq!(prepared.content().base().catalog().scenes(), scenes);
    for name in [
        "scenes/meadow.scene.json.meta",
        "scenes/cavern.scene.json.meta",
    ] {
        let bytes = fs::read(directory.project.join(name)).unwrap();
        let metadata: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let level = if name.contains("meadow") {
            LevelId::Meadow
        } else {
            LevelId::Cavern
        };
        assert_eq!(
            metadata["payload"]["asset"]["stable_id"],
            level.scene_product().source().to_string()
        );
        assert_eq!(
            prepared
                .source()
                .members()
                .iter()
                .filter(|member| {
                    member.kind() == ProjectContentMemberKind::AssetMetadata
                        && member.path().as_str() == name
                })
                .count(),
            1
        );
        assert_raw(&prepared, name, Some(&bytes));
    }
}
