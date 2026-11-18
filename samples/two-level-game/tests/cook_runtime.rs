//! The source oracle uses public manifest ingestion, the selected recipe and strict delivery.
//! Preparation retains the shared production source selection. Raw paths are relative to
//! the project root; importer and delivery observations may repeat a path with the same digest.
//! Preparation precedes logical Player bootstrap generation. Source-free Session acceptance
//! consumes the prepared base after the copied project and import cache have been removed.

#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/bin/cook/input.rs"]
mod input;

#[cfg(any(feature = "development", feature = "cook"))]
#[path = "../src/bin/cook/content.rs"]
mod content;
#[cfg(any(feature = "development", feature = "cook"))]
#[allow(dead_code)]
#[path = "../src/bin/player/evidence.rs"]
mod evidence;
#[cfg(any(feature = "development", feature = "cook"))]
#[allow(dead_code)]
#[path = "../src/bin/player/output.rs"]
mod output;
use input::RuntimeKind;
use nara::{
    app::PluginId, input::ActionMapFileCandidate, project::ProjectManifest,
    project_host::resolve_runtime_configuration,
};
use two_level_game::input::{INPUT_PATH, input_product};

fn plan(runtime: RuntimeKind) -> nara::project_host::RuntimeConfigurationPlan {
    let manifest = ProjectManifest::parse_toml_bytes(include_bytes!("../nara.toml"));
    assert!(!manifest.has_errors(), "{:?}", manifest.diagnostics);
    let settings = manifest
        .manifest
        .unwrap()
        .resolve_profile(match runtime {
            RuntimeKind::Headless => None,
            RuntimeKind::Desktop | RuntimeKind::AudioDesktop => Some("desktop"),
        })
        .unwrap();
    let projection = settings.source_runtime_projection().unwrap();
    assert_eq!(projection.input_requirement().unwrap().as_str(), INPUT_PATH);
    let input = ActionMapFileCandidate::decode_json_bytes(
        &std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(INPUT_PATH)).unwrap(),
    )
    .unwrap();
    assert_eq!(input.product(), input_product());
    let mut configuration = projection.runtime().to_configuration();
    configuration.input.action_map = Some(input.product());
    let configuration = configuration.resolve().unwrap();
    assert_eq!(configuration.input().action_map, Some(input_product()));
    resolve_runtime_configuration(&configuration, runtime.recipe().unwrap()).unwrap()
}

#[test]
fn headless_cooking_selects_authored_semantics_without_a_native_window() {
    let selected = RuntimeKind::parse("headless").unwrap();
    let plan = plan(selected);
    assert!(!plan.settings().window().enabled);
    assert_eq!(
        plan.plugin_plan()
            .entries()
            .iter()
            .filter(|entry| entry.plugin_id() == PluginId::new("two-level-game.input"))
            .count(),
        1
    );
    assert_eq!(selected.binary(), "player");
    assert_eq!(selected.features(), ["cooked-player"]);
    assert_eq!(selected.content_profile(), "two-level-shipping");
}

#[cfg(feature = "desktop")]
#[test]
fn desktop_selection_binds_its_profile_and_authored_game_input() {
    let selected = RuntimeKind::parse("desktop").unwrap();
    let plan = plan(selected);
    assert!(plan.settings().window().enabled);
    assert_eq!(
        plan.plugin_plan()
            .entries()
            .iter()
            .filter(|entry| entry.plugin_id() == PluginId::new("two-level-game.input"))
            .count(),
        1
    );
    assert_ne!(
        plan.plugin_plan().fingerprint(),
        self::plan(RuntimeKind::Headless)
            .plugin_plan()
            .fingerprint()
    );
    assert_eq!(selected.binary(), "player_desktop");
    assert_eq!(selected.features(), ["cooked-player", "desktop"]);
    assert_eq!(selected.content_profile(), "two-level-desktop-shipping");
}

#[cfg(all(windows, feature = "desktop", feature = "audio-cpal"))]
#[test]
fn audible_selection_is_explicit_and_never_opens_a_device_during_composition() {
    let selected = RuntimeKind::parse("audio-desktop").unwrap();
    let plan = plan(selected);
    assert!(plan.settings().window().enabled);
    assert_eq!(selected.binary(), "player_audio");
    assert_eq!(
        selected.features(),
        ["cooked-player", "desktop", "audio-cpal"]
    );
    assert_eq!(
        selected.content_profile(),
        "two-level-audio-desktop-shipping"
    );
    assert_eq!(
        plan.plugin_plan()
            .entries()
            .iter()
            .filter(|entry| entry.plugin_id() == two_level_game::audio::AUDIO_PLUGIN_ID)
            .count(),
        1
    );
    assert!(
        self::plan(RuntimeKind::Desktop)
            .plugin_plan()
            .entries()
            .iter()
            .all(|entry| entry.plugin_id() != two_level_game::audio::AUDIO_PLUGIN_ID)
    );
    assert_ne!(
        plan.plugin_plan().fingerprint(),
        self::plan(RuntimeKind::Desktop).plugin_plan().fingerprint()
    );
}

#[test]
fn an_unknown_runtime_cannot_select_a_player() {
    assert!(RuntimeKind::parse("Desktop").is_err());
    assert!(RuntimeKind::parse("anything").is_err());
    #[cfg(not(all(windows, feature = "desktop", feature = "audio-cpal")))]
    {
        assert!(RuntimeKind::parse("audio-desktop").is_err());
        assert!(RuntimeKind::AudioDesktop.recipe().is_err());
    }
    #[cfg(not(feature = "desktop"))]
    {
        assert!(RuntimeKind::parse("desktop").is_err());
        assert!(RuntimeKind::Desktop.recipe().is_err());
    }
}

#[cfg(any(feature = "development", feature = "cook"))]
#[path = "../../../tests/support/two_level_pack.rs"]
mod two_level_pack;

#[cfg(any(feature = "development", feature = "cook"))]
mod atlas_content {
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };

    use nara::{
        advanced_prelude::CurrentScene,
        asset::{
            AssetProductRef, AssetRef, AssetServer, AssetStates, AssetVersion, Assets, Handle,
            RuntimeAssetCodec,
        },
        asset_build::ArtifactBuildObservation,
        ecs::World,
        fs::{ContentDigest, DirectoryCapability, FsError, RelativePath},
        gameplay::{
            GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandSourceSequence,
            GameplayCommandSubmission, GameplayCommandTick, GameplayCommandTypeId,
        },
        identity::{EntityLookup, IdentityDomainError, SpawnedSceneInstance, WorldIdentityDomain},
        image::{ImageAsset, ImageExtent, ImageRuntimeCodec},
        input::{
            ActionMapBuilder, ActionMapFileCandidate, ActionMapLimits, ActionMapRuntimeCodec,
            InputBinding, KeyCode,
        },
        material::{Material2d, Material2dAsset, Material2dDomain, Material2dRuntimeCodec},
        project::ProjectManifest,
        project_host::{
            HeadlessSession, HeadlessSessionClose, ProjectContentDeliveryReceipt,
            ProjectContentError, ProjectContentErrorKind, ProjectContentLoader,
            ProjectContentMemberKind, ingest_project_manifest, resolve_product_recipe,
        },
        project_tooling::{
            host_fs,
            player_package::{GeneratedPlayerBootstrap, PlayerPackageLimits},
            project_cook::ProjectCookError,
        },
        project_workflow::cooked_content::PreparedRuntimeContent,
        reflect::{
            AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentTypeId,
            PersistentComponentSchemaOwnerId, decode_asset_reference_value,
        },
        runtime_content::{
            RuntimeContentDependency, RuntimeContentDependencyRole, RuntimeContentError,
            RuntimeContentMemberKind,
        },
        scene::{SceneEntitySource, SourceDocumentKind, SourceDocumentOrigin},
        sprite::Sprite,
        tilemap::{
            TileSet, TileSetAssets, TileSetDomain, TileSetRuntimeCodec, TileSetSource,
            TileSetSourceFileLimits, Tilemap,
        },
    };
    use nara_sprite_animation::{SpriteAnimationCodec, SpriteAnimationDomain};
    use two_level_game::{
        GamePhase, GameSnapshot, Scenario, SideRoomPhase,
        animation::{atlas_product, clips_product},
        levels::{LevelId, LevelKey},
        markers::{MarkerLink, SCENE_PATH, marker_image_product, marker_product},
        region_material, region_tileset, side_room,
        sound_bank::bank_product,
    };

    use super::{RuntimeKind, content, evidence::Evidence, two_level_pack};

    const DEFAULT_SCHEMA: &str = "schema/authoring.bundle.json";
    const SELECTED_SCHEMA: &str = "authoring/selected.bundle.json";
    const GENERATED: &str = "player-bootstrap.rs";

    struct Cache {
        parent: PathBuf,
        path: PathBuf,
    }

    impl Cache {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let parent = std::env::temp_dir().canonicalize().unwrap();
            let path = parent.join(format!(
                "two-level-atlas-cook-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self { parent, path }
        }

        fn project(&self) -> PathBuf {
            let project = self.path.join("project");
            let sample = Path::new(env!("CARGO_MANIFEST_DIR"));
            for name in [
                "nara.toml",
                super::INPUT_PATH,
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
                let destination = project.join(name);
                fs::create_dir_all(destination.parent().unwrap()).unwrap();
                fs::copy(sample.join(name), destination).unwrap();
            }
            project
        }
    }

    impl Drop for Cache {
        fn drop(&mut self) {
            if let Ok(resolved) = self.path.canonicalize()
                && resolved == self.path
                && resolved.parent() == Some(self.parent.as_path())
            {
                let _ = fs::remove_dir_all(resolved);
            }
        }
    }

    fn select_alternate_schema(project: &Path) {
        let manifest_path = project.join("nara.toml");
        let loaded = ProjectManifest::parse_toml_bytes(&fs::read(&manifest_path).unwrap());
        assert!(!loaded.has_errors(), "{:?}", loaded.diagnostics);
        let mut manifest = loaded.manifest.unwrap();
        assert_eq!(
            manifest.authoring.schema_bundle.as_deref(),
            Some(DEFAULT_SCHEMA)
        );
        manifest.authoring.schema_bundle = Some(SELECTED_SCHEMA.to_owned());
        let selected = project.join(SELECTED_SCHEMA);
        fs::create_dir_all(selected.parent().unwrap()).unwrap();
        fs::rename(project.join(DEFAULT_SCHEMA), selected).unwrap();
        fs::write(manifest_path, manifest.to_canonical_toml_bytes().unwrap()).unwrap();
        assert!(!project.join(DEFAULT_SCHEMA).exists());
    }

    fn source_delivery(
        root: &DirectoryCapability,
    ) -> Result<ProjectContentDeliveryReceipt, ProjectContentError> {
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let recipe = RuntimeKind::Headless.recipe().unwrap();
        let plan = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
        ProjectContentLoader::with_limits(root.share(), recipe.source_content_limits())
            .unwrap()
            .load_delivery(&candidate, &plan)
    }

    fn divergent_bundle(bytes: &[u8]) -> Vec<u8> {
        let bundle = AuthoringSchemaBundle::from_json_bytes(bytes).unwrap();
        let mut owners = bundle
            .owners()
            .iter()
            .map(AuthoringSchemaBundleOwner::from)
            .collect::<Vec<_>>();
        let owner_id = PersistentComponentSchemaOwnerId::try_new(
            nara::scene::SCENE_COMPONENTS_SCHEMA_PROVIDER
                .owner()
                .as_str(),
        )
        .unwrap();
        let owner = owners
            .iter_mut()
            .find(|owner| owner.owner_id() == &owner_id)
            .unwrap();
        let mut catalog = owner.current().clone();
        catalog
            .type_tombstones
            .push(ComponentTypeId::new("test.cook.Divergent"));
        *owner = AuthoringSchemaBundleOwner::new(owner_id, catalog, owner.predecessor().cloned());
        AuthoringSchemaBundle::new(owners)
            .unwrap()
            .to_json_string()
            .unwrap()
            .into_bytes()
    }

    fn generate_bootstrap(
        project: &Path,
        content: &PreparedRuntimeContent,
    ) -> GeneratedPlayerBootstrap {
        let bootstrap =
            GeneratedPlayerBootstrap::new(content, GENERATED, PlayerPackageLimits::default())
                .unwrap();
        assert_eq!(bootstrap.name(), GENERATED);
        assert_eq!(
            bootstrap.digest(),
            ContentDigest::of_bytes(bootstrap.bytes())
        );
        assert!(!project.join(bootstrap.name()).exists());
        assert!(!project.join("src/bin/player/generated.rs").exists());
        bootstrap
    }

    #[test]
    fn cooking_shares_atlas_and_clips_preserves_raw_inputs_and_decodes_source_free_player_evidence()
    {
        assert_source_free_cooked_content(DEFAULT_SCHEMA);
    }

    #[test]
    fn selected_alternate_schema_cooks_and_runs_source_free_without_the_default_bundle() {
        assert_source_free_cooked_content(SELECTED_SCHEMA);
    }

    fn assert_source_free_cooked_content(schema: &str) {
        let cache = Cache::new();
        let project = cache.project();
        if schema == SELECTED_SCHEMA {
            select_alternate_schema(&project);
        }
        let store = cache.path.join("store");
        fs::create_dir(&store).unwrap();
        let root = host_fs::readable_directory(&project).unwrap();
        let (prepared, selection) = content::prepare(
            &root,
            &host_fs::writable_directory(&store).unwrap(),
            "x86_64-pc-windows-msvc",
            RuntimeKind::Headless,
        )
        .unwrap()
        .into_parts();
        let delivery = selection.delivery_receipt();
        let observations = selection.observations();
        assert_eq!(delivery.members().len(), 35);
        for (kind, count) in [
            (ProjectContentMemberKind::ProjectManifest, 1),
            (ProjectContentMemberKind::AuthoringSchemaBundle, 1),
            (ProjectContentMemberKind::Scene, 5),
            (ProjectContentMemberKind::Prefab, 1),
            (ProjectContentMemberKind::InputActionMap, 1),
            (ProjectContentMemberKind::AssetMetadata, 14),
            (ProjectContentMemberKind::AssetSource, 12),
        ] {
            assert_eq!(
                delivery
                    .members()
                    .iter()
                    .filter(|member| member.kind() == kind)
                    .count(),
                count,
                "{kind:?}"
            );
        }
        assert_eq!(
            observations
                .iter()
                .filter(|observation| matches!(
                    observation,
                    ArtifactBuildObservation::Raw { path, digest: None }
                        if path.as_str() == format!("{}.meta", super::INPUT_PATH)
                ))
                .count(),
            1
        );
        assert_eq!(
            delivery
                .scene_roots()
                .iter()
                .map(|path| path.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["cavern.scene.json", "meadow.scene.json"])
        );
        assert_eq!(
            delivery
                .members()
                .iter()
                .filter(|member| member.kind() == ProjectContentMemberKind::AuthoringSchemaBundle)
                .map(|member| member.path().as_str())
                .collect::<Vec<_>>(),
            [schema]
        );
        let catalog = prepared.base().catalog();
        assert_eq!(catalog.members().len(), 15);
        let mut scenes = LevelId::ALL.map(LevelId::scene_product).to_vec();
        scenes.sort();
        assert_eq!(catalog.scenes(), scenes);
        assert!(catalog.addressable_roots().is_empty());
        assert_eq!(
            catalog
                .members()
                .iter()
                .filter(|member| matches!(member.kind(), RuntimeContentMemberKind::Scene { .. }))
                .count(),
            5
        );
        assert_eq!(
            catalog
                .members()
                .iter()
                .filter(|member| {
                    matches!(member.kind(), RuntimeContentMemberKind::Asset { format, .. }
                        if format.as_str() == ImageRuntimeCodec::RUNTIME_KIND)
                })
                .count(),
            5
        );
        let marker = catalog.member_for_product(marker_product()).unwrap();
        assert_eq!(marker.products(), [marker_product()]);
        assert_eq!(
            marker.dependencies(),
            [RuntimeContentDependency::new(
                marker_image_product(),
                RuntimeContentDependencyRole::Required,
            )]
        );
        let RuntimeContentMemberKind::Scene { source } = marker.kind() else {
            panic!("marker template must be a real cooked Scene");
        };
        assert_eq!(source.kind(), SourceDocumentKind::Scene);
        let SourceDocumentOrigin::ProjectLocation { universe, path } = source.origin() else {
            panic!("marker wrapper must retain its ProjectLocation identity");
        };
        assert_eq!(universe.to_string(), "be3dea0c-93ef-5607-ab84-af715a1f98b3");
        assert_eq!(path.as_str(), format!("scenes/{SCENE_PATH}"));
        assert!(matches!(
            prepared.base().acquire_scene(marker_product()),
            Err(RuntimeContentError::UndeclaredScene)
        ));
        for product in [
            side_room::scene_product(),
            side_room::lookout_scene_product(),
        ] {
            two_level_pack::assert_room_member(catalog.member_for_product(product).unwrap());
            assert!(matches!(
                prepared.base().acquire_scene(product),
                Err(RuntimeContentError::UndeclaredScene)
            ));
        }
        assert_eq!(
            catalog
                .members()
                .iter()
                .flat_map(|member| member.products().iter().copied())
                .collect::<BTreeSet<_>>(),
            LevelId::ALL
                .into_iter()
                .flat_map(|level| [level.scene_product(), level.image_product()])
                .chain([
                    atlas_product(),
                    bank_product(),
                    clips_product(),
                    super::input_product(),
                    marker_product(),
                    marker_image_product(),
                    side_room::scene_product(),
                    side_room::lookout_scene_product(),
                    side_room::image_product(),
                    region_material::material_product(),
                    region_tileset::tileset_product(),
                ])
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(
            catalog
                .members()
                .iter()
                .filter(|member| member.products().contains(&atlas_product()))
                .count(),
            1
        );
        for level in LevelId::ALL {
            let scene = catalog.member_for_product(level.scene_product()).unwrap();
            assert!(matches!(
                scene.kind(),
                RuntimeContentMemberKind::Scene { .. }
            ));
            two_level_pack::assert_level_dependencies(scene, level);
            let cooked = prepared
                .base()
                .acquire_scene(level.scene_product())
                .unwrap();
            let palettes = cooked
                .candidate()
                .document()
                .entities()
                .iter()
                .filter_map(|entity| {
                    entity
                        .components()
                        .get(&ComponentTypeId::new("two-level-game.MarkerPalette"))
                })
                .collect::<Vec<_>>();
            assert_eq!(palettes.len(), 1);
            assert_eq!(
                decode_asset_reference_value(palettes[0].value().field("template").unwrap())
                    .unwrap(),
                AssetRef::Product(marker_product())
            );
        }
        let clips = catalog.member_for_product(clips_product()).unwrap();
        assert_eq!(clips.products(), [clips_product()]);
        assert!(clips.dependencies().is_empty());
        assert!(matches!(
            clips.kind(),
            RuntimeContentMemberKind::Asset { format, version }
                if format.as_str() == SpriteAnimationCodec::RUNTIME_KIND
                    && version.get() == SpriteAnimationCodec::RUNTIME_VERSION
        ));
        let acquired = prepared
            .base()
            .acquire_asset::<SpriteAnimationDomain, SpriteAnimationCodec>(clips_product())
            .unwrap();
        assert_eq!(acquired.product().product(), clips_product());
        let material = prepared
            .base()
            .acquire_asset::<Material2dDomain, Material2dRuntimeCodec>(
                region_material::material_product(),
            )
            .unwrap();
        two_level_pack::assert_room_material_member(
            catalog
                .member_for_product(region_material::material_product())
                .unwrap(),
            material.product(),
        );
        let tileset = prepared
            .base()
            .acquire_asset::<TileSetDomain, TileSetRuntimeCodec>(region_tileset::tileset_product())
            .unwrap();
        two_level_pack::assert_room_tileset_member(
            catalog
                .member_for_product(region_tileset::tileset_product())
                .unwrap(),
            tileset.product(),
        );
        let input = catalog.member_for_product(super::input_product()).unwrap();
        assert_eq!(input.products(), [super::input_product()]);
        assert!(input.dependencies().is_empty());
        assert!(matches!(
            input.kind(),
            RuntimeContentMemberKind::Asset { format, version }
                if format.as_str() == ActionMapRuntimeCodec::RUNTIME_KIND
                    && version.get() == ActionMapRuntimeCodec::RUNTIME_VERSION
        ));
        assert_eq!(
            catalog.configuration().input.action_map,
            Some(super::input_product())
        );
        let mut raw = BTreeMap::new();
        for observation in observations {
            let (path, digest) = match observation {
                ArtifactBuildObservation::Raw {
                    path,
                    digest: Some(digest),
                } => (path.as_str(), digest.content()),
                ArtifactBuildObservation::Raw { path, digest: None }
                    if path.as_str() == format!("{}.meta", super::INPUT_PATH) =>
                {
                    assert!(!project.join(path.as_str()).exists());
                    continue;
                }
                other => panic!("unexpected source import observation: {other:?}"),
            };
            if let Some(previous) = raw.insert(path, digest) {
                assert_eq!(
                    previous, digest,
                    "inconsistent source observations for {path}"
                );
            }
        }
        let mut expected = delivery
            .members()
            .iter()
            .map(|member| (member.path().as_str(), member.digest()))
            .collect::<BTreeMap<_, _>>();
        for path in [
            "assets/textures/meadow.png",
            "assets/textures/cavern.png",
            "assets/textures/explorer.png",
            "assets/textures/marker.png",
            "assets/textures/side-room.png",
            "assets/materials/region.material2d.json",
            "assets/materials/region.wgsl",
            "assets/tilesets/region.tileset.ron",
            "assets/animations/explorer.sprite-animations.json",
            "assets/audio/game.soundbank.json",
            "assets/audio/pickup.wav",
            "assets/audio/travel.wav",
            super::INPUT_PATH,
        ] {
            let digest = ContentDigest::of_bytes(&fs::read(project.join(path)).unwrap());
            if let Some(admitted) = expected.insert(path, digest) {
                assert_eq!(admitted, digest, "source delivery disagrees with {path}");
            }
        }
        assert_eq!(raw, expected);
        for path in [
            "nara.toml",
            schema,
            "scenes/meadow.scene.json",
            "scenes/meadow.scene.json.meta",
            "scenes/cavern.scene.json",
            "scenes/cavern.scene.json.meta",
            "scenes/marker.scene.json",
            "scenes/side-room.scene.json",
            "scenes/lookout.scene.json",
            "prefabs/marker.prefab.json",
            "assets/textures/explorer.png.meta",
            "assets/materials/region.material2d.json.meta",
            "assets/tilesets/region.tileset.ron.meta",
        ] {
            assert!(
                raw.contains_key(path),
                "missing admitted source member {path}"
            );
        }
        let bootstrap = generate_bootstrap(&project, &prepared);
        selection.verify(&project).unwrap();
        drop(selection);
        drop(root);
        drop(cache);
        assert!(!project.exists() && !store.exists());
        assert_eq!(bootstrap.name(), GENERATED);
        assert_eq!(
            bootstrap.digest(),
            ContentDigest::of_bytes(bootstrap.bytes())
        );
        two_level_pack::assert_room_material(material.product());
        two_level_pack::assert_room_tileset(tileset.product());
        let decoded_tileset = prepared
            .base()
            .acquire_asset::<TileSetDomain, TileSetRuntimeCodec>(region_tileset::tileset_product())
            .unwrap();
        two_level_pack::assert_room_tileset(decoded_tileset.product());
        let decoded = prepared
            .base()
            .acquire_asset::<Material2dDomain, Material2dRuntimeCodec>(
                region_material::material_product(),
            )
            .unwrap();
        two_level_pack::assert_room_material(decoded.product());
        let result = observe_startup(prepared, check_cooked_regions);
        let source: serde_json::Value =
            serde_json::from_slice(include_bytes!("../input/explorer.actions.json")).unwrap();
        assert_eq!(
            result["input_proof"],
            serde_json::json!({
                "product": super::input_product().to_string(),
                "source_free": true,
                "definition": source["payload"],
            })
        );
        let animation = &result["scene_receipts"][0]["animation_proof"];
        assert_eq!(animation, &default_animation_proof());
        let proofs = result["image_proofs"].as_array().unwrap();
        assert_eq!(proofs.len(), 3);
        let atlas = proofs
            .iter()
            .find(|proof| proof["product"] == atlas_product().to_string())
            .unwrap();
        assert_eq!(atlas["role"], "explorer_atlas");
        assert_eq!(
            atlas["extent"],
            serde_json::json!({"width": 64, "height": 16})
        );
        assert_eq!(
            atlas["cell_extent"],
            serde_json::json!({"width": 16, "height": 16})
        );
        assert_eq!(atlas["columns"], 4);
        assert_eq!(atlas["rows"], 1);
        assert_eq!(atlas["pixel_bytes"], 4096);
        assert_eq!(atlas["source_free"], true);
        let badge = proofs
            .iter()
            .find(|proof| proof["product"] == marker_image_product().to_string())
            .unwrap();
        assert_eq!(badge["role"], "marker_badge");
        assert_eq!(
            badge["extent"],
            serde_json::json!({"width": 2, "height": 2})
        );
        assert_eq!(badge["pixel_bytes"], 16);
        assert_eq!(badge["source_free"], true);
        assert_eq!(
            result["observed_image_products"]
                .as_array()
                .unwrap()
                .iter()
                .map(|product| product.as_str().unwrap().to_owned())
                .collect::<BTreeSet<_>>(),
            [
                LevelId::STARTUP.image_product(),
                atlas_product(),
                marker_image_product()
            ]
            .into_iter()
            .map(|product| product.to_string())
            .collect::<BTreeSet<_>>()
        );
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn desktop_cooker_profiles_prepare_against_the_admitted_source_schema() {
        let runtimes = &[
            RuntimeKind::Desktop,
            #[cfg(all(windows, feature = "audio-cpal"))]
            RuntimeKind::AudioDesktop,
        ];
        for &runtime in runtimes {
            let cache = Cache::new();
            let project = cache.project();
            let store = cache.path.join("store");
            fs::create_dir(&store).unwrap();
            let root = host_fs::readable_directory(&project).unwrap();
            let (prepared, selection) = content::prepare(
                &root,
                &host_fs::writable_directory(&store).unwrap(),
                "x86_64-pc-windows-msvc",
                runtime,
            )
            .unwrap_or_else(|error| panic!("{runtime:?}: {error}"))
            .into_parts();
            assert!(prepared.base().plan().settings().window().enabled);
            assert_eq!(prepared.base().catalog().members().len(), 15);
            let manifest = root
                .open_file(&RelativePath::new("nara.toml").unwrap())
                .unwrap();
            let candidate = ingest_project_manifest(&manifest, Some("desktop")).unwrap();
            let plan = resolve_product_recipe(&candidate, runtime.recipe().unwrap()).unwrap();
            assert_eq!(
                prepared
                    .base()
                    .plan()
                    .schema_validation()
                    .composition_fingerprint(),
                plan.schema_validation().composition_fingerprint(),
            );
            let _bootstrap = generate_bootstrap(&project, &prepared);
            selection.verify(&project).unwrap();
        }
    }

    #[test]
    fn invalid_selected_schema_matches_source_delivery_before_bootstrap_generation() {
        for (failure, kind, code) in [
            (
                "missing",
                ProjectContentErrorKind::HostIo,
                "project.content.host-io",
            ),
            (
                "corrupt",
                ProjectContentErrorKind::SchemaMismatch,
                "project.runtime-agreement.bundle-invalid",
            ),
            (
                "divergent",
                ProjectContentErrorKind::SchemaMismatch,
                "project.runtime-agreement.selected-owner-divergent",
            ),
        ] {
            let cache = Cache::new();
            let project = cache.project();
            select_alternate_schema(&project);
            let selected = project.join(SELECTED_SCHEMA);
            match failure {
                "missing" => fs::remove_file(&selected).unwrap(),
                "corrupt" => fs::write(&selected, b"invalid authoring schema bundle").unwrap(),
                "divergent" => {
                    fs::write(&selected, divergent_bundle(&fs::read(&selected).unwrap())).unwrap();
                }
                _ => unreachable!(),
            }
            let root = host_fs::readable_directory(&project).unwrap();
            let error = source_delivery(&root).unwrap_err();
            assert_eq!(error.kind(), kind, "{failure}: {error:?}");
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == code),
                "{failure}: {error:?}"
            );
            let store = cache.path.join("store");
            fs::create_dir(&store).unwrap();
            let store = host_fs::writable_directory(&store).unwrap();
            let mut generated = None;
            let error = content::prepare(
                &root,
                &store,
                "x86_64-pc-windows-msvc",
                RuntimeKind::Headless,
            )
            .map(|prepared| {
                let bootstrap = generate_bootstrap(&project, prepared.content());
                generated = Some((bootstrap.name().to_owned(), bootstrap.digest()));
            })
            .expect_err("invalid selected schema must reject before bootstrap generation");
            assert!(
                error.contains("source delivery agreement"),
                "{failure}: {error}"
            );
            assert!(error.contains(code), "{failure}: {error}");
            assert!(generated.is_none(), "{failure}: {generated:?}");
            assert!(!project.join(GENERATED).exists());
            assert!(!project.join("src/bin/player/generated.rs").exists());
            assert_eq!(
                fs::read_dir(cache.path.join("store"))
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect::<Vec<_>>(),
                [std::ffi::OsString::from("artifact-store.lock")]
            );
            assert!(!project.join(DEFAULT_SCHEMA).exists());
        }
    }

    #[test]
    fn selected_schema_changes_cannot_rebaseline_the_original_shared_selection() {
        for replace_identity in [false, true] {
            let cache = Cache::new();
            let project = cache.project();
            select_alternate_schema(&project);
            let store = cache.path.join("store");
            fs::create_dir(&store).unwrap();
            let root = host_fs::readable_directory(&project).unwrap();
            let (prepared, selection) = content::prepare(
                &root,
                &host_fs::writable_directory(&store).unwrap(),
                "x86_64-pc-windows-msvc",
                RuntimeKind::Headless,
            )
            .unwrap()
            .into_parts();
            let bootstrap = generate_bootstrap(&project, &prepared);
            selection.verify(&project).unwrap();
            let selected = project.join(SELECTED_SCHEMA);
            let relative = RelativePath::new(SELECTED_SCHEMA).unwrap();
            let original = root.open_file(&relative).unwrap();
            let bytes = fs::read(&selected).unwrap();
            if replace_identity {
                fs::rename(&selected, selected.with_extension("original")).unwrap();
                fs::write(&selected, &bytes).unwrap();
                let replacement = root.open_file(&relative).unwrap();
                assert_ne!(original.identity(), replacement.identity());
                assert_eq!(fs::read(&selected).unwrap(), bytes);
                source_delivery(&root).unwrap();
            } else {
                fs::write(&selected, divergent_bundle(&bytes)).unwrap();
                let changed = root.open_file(&relative).unwrap();
                assert_eq!(original.identity(), changed.identity());
                assert_ne!(fs::read(&selected).unwrap(), bytes);
                let error = source_delivery(&root).unwrap_err();
                assert!(
                    error.diagnostics().iter().any(|diagnostic| {
                        diagnostic.code().as_str()
                            == "project.runtime-agreement.selected-owner-divergent"
                    }),
                    "{error:?}"
                );
            }
            let error = selection.verify(&project).unwrap_err();
            if replace_identity {
                assert!(matches!(
                    error,
                    ProjectCookError::Filesystem(FsError::IdentityMismatch { .. })
                ));
                fs::remove_file(&selected).unwrap();
                fs::rename(selected.with_extension("original"), &selected).unwrap();
            } else {
                assert!(matches!(
                    error,
                    ProjectCookError::Filesystem(
                        FsError::DigestMismatch { .. } | FsError::ByteLimitExceeded { .. }
                    )
                ));
                fs::write(&selected, &bytes).unwrap();
            }
            selection.verify(&project).unwrap();
            assert_eq!(bootstrap.name(), GENERATED);
            assert_eq!(
                bootstrap.digest(),
                ContentDigest::of_bytes(bootstrap.bytes())
            );
            assert_eq!(prepared.base().catalog().members().len(), 15);
        }
    }

    #[test]
    fn material_preflight_rejects_corrupt_inputs_and_recovers_without_changing_products() {
        for (name, code) in [
            ("assets/materials/region.material2d.json", "material.format"),
            (
                "assets/materials/region.material2d.json.meta",
                "project.content.asset-meta-invalid",
            ),
            ("assets/materials/region.wgsl", "material.shader"),
        ] {
            let cache = Cache::new();
            let project = cache.project();
            let store = cache.path.join("store");
            fs::create_dir(&store).unwrap();
            let store = host_fs::writable_directory(&store).unwrap();
            let root = host_fs::readable_directory(&project).unwrap();
            let (original, selection) = content::prepare(
                &root,
                &store,
                "x86_64-pc-windows-msvc",
                RuntimeKind::Headless,
            )
            .unwrap()
            .into_parts();
            let bootstrap = generate_bootstrap(&project, &original);
            let path = project.join(name);
            let bytes = fs::read(&path).unwrap();
            fs::write(&path, "invalid region Material input").unwrap();
            assert!(
                matches!(
                    selection.verify(&project),
                    Err(ProjectCookError::Filesystem(
                        FsError::DigestMismatch { .. } | FsError::ByteLimitExceeded { .. }
                    ))
                ),
                "{name}"
            );
            let source_error = source_delivery(&root).unwrap_err();
            assert!(
                source_error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == code),
                "{name}: {source_error:?}"
            );
            let error = content::prepare(
                &root,
                &store,
                "x86_64-pc-windows-msvc",
                RuntimeKind::Headless,
            )
            .err()
            .expect("invalid source must not reuse the previous cooked Material");
            assert!(
                error.contains("source delivery agreement"),
                "{name}: {error}"
            );
            assert!(error.contains(code), "{name}: {error}");
            let retained = original
                .base()
                .acquire_asset::<Material2dDomain, Material2dRuntimeCodec>(
                    region_material::material_product(),
                )
                .unwrap();
            two_level_pack::assert_room_material(retained.product());
            fs::write(&path, bytes).unwrap();
            selection.verify(&project).unwrap();
            let (repaired, repaired_selection) = content::prepare(
                &root,
                &store,
                "x86_64-pc-windows-msvc",
                RuntimeKind::Headless,
            )
            .unwrap()
            .into_parts();
            repaired_selection.verify(&project).unwrap();
            assert_eq!(selection.observations(), repaired_selection.observations());
            assert_eq!(
                original.base().catalog().to_json_bytes().unwrap(),
                repaired.base().catalog().to_json_bytes().unwrap()
            );
            let repaired_bootstrap = generate_bootstrap(&project, &repaired);
            assert_eq!(bootstrap.bytes(), repaired_bootstrap.bytes());
            drop(selection);
            drop(repaired_selection);
            drop(root);
            drop(store);
            drop(cache);
            assert!(!project.exists());
            let material = repaired
                .base()
                .acquire_asset::<Material2dDomain, Material2dRuntimeCodec>(
                    region_material::material_product(),
                )
                .unwrap();
            two_level_pack::assert_room_material(material.product());
            two_level_pack::assert_room_material(retained.product());
        }
    }

    #[test]
    fn tileset_preflight_rejects_invalid_files_layout_and_room_cells_before_publication() {
        for (name, failure) in [
            ("assets/tilesets/region.tileset.ron", "source"),
            ("assets/tilesets/region.tileset.ron.meta", "metadata"),
            ("assets/tilesets/region.tileset.ron", "layout"),
            ("scenes/side-room.scene.json", "member"),
            ("scenes/side-room.scene.json", "cells"),
            ("scenes/lookout.scene.json", "cells"),
        ] {
            let cache = Cache::new();
            let project = cache.project();
            let store = cache.path.join("store");
            fs::create_dir(&store).unwrap();
            let store = host_fs::writable_directory(&store).unwrap();
            let root = host_fs::readable_directory(&project).unwrap();
            let (original, selection) = content::prepare(
                &root,
                &store,
                "x86_64-pc-windows-msvc",
                RuntimeKind::Headless,
            )
            .unwrap()
            .into_parts();
            let bootstrap = generate_bootstrap(&project, &original);
            let retained = original
                .base()
                .acquire_asset::<TileSetDomain, TileSetRuntimeCodec>(
                    region_tileset::tileset_product(),
                )
                .unwrap();
            let path = project.join(name);
            let bytes = fs::read(&path).unwrap();
            match failure {
                "source" | "metadata" => fs::write(&path, "invalid region TileSet input").unwrap(),
                "layout" => {
                    let mut tileset =
                        TileSetSource::decode_ron_bytes(&bytes, TileSetSourceFileLimits::default())
                            .unwrap();
                    tileset.atlas.columns = 3;
                    fs::write(&path, tileset.encode_ron_bytes().unwrap()).unwrap();
                }
                "member" | "cells" => {
                    let mut scene: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                    let floor = scene["payload"]["entities"]
                        .as_array_mut()
                        .unwrap()
                        .iter_mut()
                        .find(|entity| entity["components"].get("nara.tilemap.Tilemap").is_some())
                        .unwrap();
                    let tilemap =
                        &mut floor["components"]["nara.tilemap.Tilemap"]["value"]["value"];
                    if failure == "member" {
                        tilemap["tileset"]["value"]["value"]["value"]["source"]["value"] =
                            serde_json::json!(side_room::image_product().source().to_string());
                    } else {
                        tilemap["cells"]["value"][0]["value"]["cell"]["value"]["tile"]["value"] =
                            serde_json::json!(4);
                    }
                    fs::write(&path, serde_json::to_vec(&scene).unwrap()).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                matches!(
                    selection.verify(&project),
                    Err(ProjectCookError::Filesystem(
                        FsError::DigestMismatch { .. } | FsError::ByteLimitExceeded { .. }
                    ))
                ),
                "{name}, {failure}"
            );
            let source_error = source_delivery(&root).unwrap_err();
            let code = match failure {
                "source" => "project.content.tileset-source-invalid",
                "metadata" => "project.content.asset-meta-invalid",
                "layout" => "project.content.tileset-image-extent-mismatch",
                "member" => "project.content.tileset-unavailable",
                "cells" => "project.content.tile-index-out-of-range",
                _ => unreachable!(),
            };
            assert!(
                source_error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == code),
                "{name}, {failure}: {source_error:?}"
            );
            let error = content::prepare(
                &root,
                &store,
                "x86_64-pc-windows-msvc",
                RuntimeKind::Headless,
            )
            .err()
            .expect("invalid TileSet content must refuse the unpublished candidate");
            assert!(
                error.contains("source delivery agreement"),
                "{name}: {error}"
            );
            assert!(error.contains(code), "{name}, {failure}: {error}");
            two_level_pack::assert_room_tileset(retained.product());
            let original_tileset = original
                .base()
                .acquire_asset::<TileSetDomain, TileSetRuntimeCodec>(
                    region_tileset::tileset_product(),
                )
                .unwrap();
            two_level_pack::assert_room_tileset_member(
                original
                    .base()
                    .catalog()
                    .member_for_product(region_tileset::tileset_product())
                    .unwrap(),
                original_tileset.product(),
            );
            fs::write(&path, bytes).unwrap();
            selection.verify(&project).unwrap();
            let (repaired, repaired_selection) = content::prepare(
                &root,
                &store,
                "x86_64-pc-windows-msvc",
                RuntimeKind::Headless,
            )
            .unwrap()
            .into_parts();
            repaired_selection.verify(&project).unwrap();
            assert_eq!(selection.observations(), repaired_selection.observations());
            assert_eq!(
                original.base().catalog().to_json_bytes().unwrap(),
                repaired.base().catalog().to_json_bytes().unwrap()
            );
            let repaired_bootstrap = generate_bootstrap(&project, &repaired);
            assert_eq!(bootstrap.bytes(), repaired_bootstrap.bytes());
            drop(selection);
            drop(repaired_selection);
            drop(root);
            drop(store);
            drop(cache);
            assert!(!project.exists());
            let repaired_tileset = repaired
                .base()
                .acquire_asset::<TileSetDomain, TileSetRuntimeCodec>(
                    region_tileset::tileset_product(),
                )
                .unwrap();
            two_level_pack::assert_room_tileset(repaired_tileset.product());
            two_level_pack::assert_room_tileset(retained.product());
        }
    }

    #[test]
    fn cooking_preserves_changed_authored_mapping_without_source_in_the_installed_player() {
        let cache = Cache::new();
        let project = cache.project();
        let input_path = project.join(super::INPUT_PATH);
        let original =
            ActionMapFileCandidate::decode_json_bytes(&fs::read(&input_path).unwrap()).unwrap();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        for context in original.definition().contexts() {
            builder
                .declare_context(context.context.clone(), context.enabled)
                .unwrap();
        }
        let mut changed = false;
        for mut binding in original.definition().bindings().iter().cloned() {
            if binding.action.as_str() == "two-level-game.move-w" {
                binding.input = InputBinding::Key(KeyCode::Character('i'));
                changed = true;
            }
            builder.bind(binding).unwrap();
        }
        assert!(changed);
        let authored =
            ActionMapFileCandidate::new(original.product(), builder.compile().unwrap()).unwrap();
        let bytes = authored.to_json_string().unwrap().into_bytes();
        fs::write(&input_path, &bytes).unwrap();
        assert!(!project.join(format!("{}.meta", super::INPUT_PATH)).exists());
        let root = host_fs::readable_directory(&project).unwrap();
        let store = cache.path.join("store");
        fs::create_dir(&store).unwrap();
        let (prepared, selection) = content::prepare(
            &root,
            &host_fs::writable_directory(&store).unwrap(),
            "x86_64-pc-windows-msvc",
            RuntimeKind::Headless,
        )
        .unwrap()
        .into_parts();
        selection.verify(&project).unwrap();
        let input = prepared.base().acquire_startup_input().unwrap().unwrap();
        assert_eq!(
            prepared.base().plan().settings().input().action_map,
            Some(authored.product())
        );
        let member = prepared
            .base()
            .catalog()
            .member_for_product(authored.product())
            .unwrap();
        assert_eq!(member.products(), [authored.product()]);
        assert!(member.dependencies().is_empty());
        assert_eq!(input.product(), authored.product());
        assert_eq!(
            input.definition().contexts(),
            authored.definition().contexts()
        );
        assert_eq!(
            input.definition().bindings(),
            authored.definition().bindings()
        );
        assert_ne!(
            input.definition().bindings(),
            original.definition().bindings()
        );
        for level in LevelId::ALL {
            two_level_pack::assert_level_dependencies(
                prepared
                    .base()
                    .catalog()
                    .member_for_product(level.scene_product())
                    .unwrap(),
                level,
            );
        }
        fs::write(&input_path, original.to_json_string().unwrap()).unwrap();
        assert!(matches!(
            selection.verify(&project),
            Err(ProjectCookError::Filesystem(
                FsError::DigestMismatch { .. } | FsError::ByteLimitExceeded { .. }
            ))
        ));
        fs::remove_file(&input_path).unwrap();
        assert!(!input_path.exists());
        let result = observe_startup(prepared, |_| Ok(()));
        let expected: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            result["input_proof"],
            serde_json::json!({
                "product": authored.product().to_string(),
                "source_free": true,
                "definition": expected["payload"],
            })
        );
    }

    #[test]
    fn optional_input_sidecar_must_match_the_intrinsic_identity_before_cooking() {
        let cache = Cache::new();
        let project = cache.project();
        let path = project.join(format!("{}.meta", super::INPUT_PATH));
        let metadata = serde_json::json!({
            "kind": "asset_meta", "format_version": 1, "engine_min_version": "0.1.0",
            "generator": {"name": "nara", "version": "0.1.0"},
            "payload": {
                "asset": {
                    "stable_id": two_level_game::input::INPUT_SOURCE_ID,
                    "path": super::INPUT_PATH,
                    "source_kind": {"other": "input-action-map"}
                },
                "settings": {}
            }
        });
        fs::write(&path, serde_json::to_vec(&metadata).unwrap()).unwrap();
        let store = cache.path.join("store");
        fs::create_dir(&store).unwrap();
        let root = host_fs::readable_directory(&project).unwrap();
        let store = host_fs::writable_directory(&store).unwrap();
        let (prepared, selection) = content::prepare(
            &root,
            &store,
            "x86_64-pc-windows-msvc",
            RuntimeKind::Headless,
        )
        .unwrap()
        .into_parts();
        selection.verify(&project).unwrap();
        assert_eq!(
            prepared
                .base()
                .acquire_startup_input()
                .unwrap()
                .unwrap()
                .product(),
            super::input_product()
        );
        for (field, replacement) in [
            (
                "stable_id",
                serde_json::json!("c4fd21bb-9dd3-4f7f-8e6c-9934f8686938"),
            ),
            ("path", serde_json::json!("input/other.actions.json")),
            (
                "source_kind",
                serde_json::json!({"other": "different-kind"}),
            ),
        ] {
            let mut invalid = metadata.clone();
            invalid["payload"]["asset"][field] = replacement;
            fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
            let source_error = source_delivery(&root).unwrap_err();
            assert!(
                source_error.diagnostics().iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "project.content.asset-identity-conflict"
                }),
                "{field}: {source_error:?}"
            );
            let error = content::prepare(
                &root,
                &store,
                "x86_64-pc-windows-msvc",
                RuntimeKind::Headless,
            )
            .err()
            .unwrap();
            assert!(
                error.contains("project.content.asset-identity-conflict"),
                "{field}: {error}"
            );
        }
    }

    fn observe_startup(
        prepared: PreparedRuntimeContent,
        exercise: impl FnOnce(&mut HeadlessSession) -> Result<(), String>,
    ) -> serde_json::Value {
        let input = prepared.base().acquire_startup_input().unwrap().unwrap();
        let mut evidence = Evidence::default();
        evidence.expect_input(&input).unwrap();
        let mut session = HeadlessSession::from_runtime_content(prepared.into_base());
        let observed = (|| -> Result<_, String> {
            session.start().map_err(|error| error.to_string())?;
            session
                .frame(Duration::ZERO)
                .map_err(|error| error.to_string())?;
            evidence.observe(&session)?;
            let initial = evidence.result(Scenario::Interactive, None, None);
            session.pause().map_err(|error| error.to_string())?;
            step_command(&mut session, 1, "two-level-game.marker")?;
            session
                .observe(assert_cooked_marker_image)
                .map_err(|error| error.to_string())??;
            exercise(&mut session)?;
            Ok(initial)
        })();
        let deadline = Instant::now() + Duration::from_secs(10);
        let closed = loop {
            match session.close() {
                HeadlessSessionClose::Pending if Instant::now() < deadline => {
                    std::thread::yield_now();
                }
                outcome => break outcome,
            }
        };
        assert_eq!(closed, HeadlessSessionClose::Complete);
        observed.unwrap()
    }

    fn step_command(session: &mut HeadlessSession, tick: u64, command: &str) -> Result<(), String> {
        session
            .submit(GameplayCommandSubmission::new(
                GameplayCommandTick::new(tick).unwrap(),
                GameplayCommandIngressSource::external("test.cooked-content").unwrap(),
                GameplayCommandSourceSequence::new(tick).unwrap(),
                GameplayCommandDraft::new(GameplayCommandTypeId::new(command).unwrap()),
            ))
            .map_err(|error| error.to_string())?;
        session.step().map_err(|error| error.to_string())?;
        Ok(())
    }

    fn check_cooked_regions(session: &mut HeadlessSession) -> Result<(), String> {
        const SIDE_ROOM: &str = "two-level-game.side-room";
        const LOOKOUT: &str = "two-level-game.lookout";
        const TRAVEL: &str = "two-level-game.travel";
        let meadow = LevelKey::main(LevelId::Meadow);
        let cavern = LevelKey::main(LevelId::Cavern);
        let primary = session
            .observe(|world| {
                let snapshot = world.resource::<GameSnapshot>();
                assert_eq!(snapshot.level, Some(meadow));
                assert_eq!(snapshot.side_room, SideRoomPhase::Closed);
                assert_eq!(snapshot.lookout, SideRoomPhase::Closed);
                assert_resident_images(world, LevelId::Meadow, None);
                assert!(world.resource::<Assets<Material2dAsset>>().is_empty());
                assert!(world.resource::<Assets<TileSet>>().is_empty());
                CurrentScene::get(world).unwrap().receipt().clone()
            })
            .map_err(|error| error.to_string())?;
        let primary_id = Some(primary.instance_id());
        step_command(session, 2, SIDE_ROOM)?;
        wait_for_snapshot(session, "cooked side room did not open", |snapshot| {
            assert_eq!(snapshot.current_instance, primary_id);
            snapshot.side_room == SideRoomPhase::Open
        })?;
        step_command(session, 3, LOOKOUT)?;
        wait_for_snapshot(session, "cooked lookout did not open", |snapshot| {
            assert_eq!(snapshot.current_instance, primary_id);
            assert_eq!(snapshot.side_room, SideRoomPhase::Open);
            snapshot.lookout == SideRoomPhase::Open
        })?;
        let relic_product = Some(side_room::image_product());
        let (room, lookout, shared_image, shared_material, shared_version, retained_material) =
            session
                .observe(|world| {
                    assert_eq!(CurrentScene::get(world).unwrap().receipt(), &primary);
                    let (room, room_image, material, version) =
                        cooked_relic(world, side_room::SCENE_PATH, side_room::RELIC);
                    let (lookout, lookout_image, lookout_material, lookout_version) = cooked_relic(
                        world,
                        side_room::LOOKOUT_SCENE_PATH,
                        side_room::LOOKOUT_RELIC,
                    );
                    assert_ne!(room.instance_id(), lookout.instance_id());
                    assert_ne!(room.instance_id(), primary.instance_id());
                    assert_ne!(lookout.instance_id(), primary.instance_id());
                    assert_eq!(room_image, lookout_image);
                    assert_eq!(material, lookout_material);
                    assert_eq!(version, lookout_version);
                    assert_resident_images(world, LevelId::Meadow, relic_product);
                    let retained = world
                        .resource::<Assets<Material2dAsset>>()
                        .get(material)
                        .unwrap()
                        .clone();
                    (room, lookout, room_image, material, version, retained)
                })
                .map_err(|error| error.to_string())?;
        let (shared_tileset, tileset_version, tileset_slot, retained_tileset) = session
            .observe(|world| {
                let (handle, version) = cooked_floor(world, &room, region_tileset::SIDE_ROOM_FLOOR);
                assert_eq!(
                    cooked_floor(world, &lookout, region_tileset::LOOKOUT_FLOOR),
                    (handle, version)
                );
                let tilesets = world.resource::<Assets<TileSet>>();
                (
                    handle,
                    version,
                    tilesets.slot_revision(handle),
                    tilesets.get(handle).unwrap().clone(),
                )
            })
            .map_err(|error| error.to_string())?;
        let original_slot = session
            .observe(|world| {
                world
                    .resource::<Assets<Material2dAsset>>()
                    .slot_revision(shared_material)
            })
            .map_err(|error| error.to_string())?;
        step_command(session, 4, SIDE_ROOM)?;
        session
            .observe(|world| {
                let snapshot = world.resource::<GameSnapshot>();
                assert_eq!(snapshot.side_room, SideRoomPhase::Closed);
                assert_eq!(snapshot.lookout, SideRoomPhase::Open);
                assert_eq!(snapshot.current_instance, primary_id);
                assert_eq!(CurrentScene::get(world).unwrap().receipt(), &primary);
                assert_retired(world, &room);
                assert_eq!(
                    cooked_floor(world, &lookout, region_tileset::LOOKOUT_FLOOR),
                    (shared_tileset, tileset_version)
                );
                let tilesets = world.resource::<Assets<TileSet>>();
                assert_eq!(tilesets.slot_revision(shared_tileset), tileset_slot);
                assert!(
                    tilesets
                        .get(shared_tileset)
                        .unwrap()
                        .image_content()
                        .unwrap()
                        .same_allocation(retained_tileset.image_content().unwrap())
                );
                assert_eq!(
                    cooked_relic(
                        world,
                        side_room::LOOKOUT_SCENE_PATH,
                        side_room::LOOKOUT_RELIC
                    ),
                    (
                        lookout.clone(),
                        shared_image,
                        shared_material,
                        shared_version
                    )
                );
                assert_eq!(
                    world
                        .resource::<Assets<Material2dAsset>>()
                        .get(shared_material)
                        .unwrap()
                        .generation(),
                    retained_material.generation()
                );
                assert_resident_images(world, LevelId::Meadow, relic_product);
            })
            .map_err(|error| error.to_string())?;
        step_command(session, 5, SIDE_ROOM)?;
        wait_for_snapshot(session, "cooked side room did not reopen", |snapshot| {
            assert_eq!(snapshot.current_instance, primary_id);
            assert_eq!(snapshot.lookout, SideRoomPhase::Open);
            snapshot.side_room == SideRoomPhase::Open
        })?;
        let reopened = session
            .observe(|world| {
                let (reopened, image, material, version) =
                    cooked_relic(world, side_room::SCENE_PATH, side_room::RELIC);
                assert_ne!(reopened.instance_id(), room.instance_id());
                assert_eq!(image, shared_image);
                assert_eq!(material, shared_material);
                assert_eq!(version, shared_version);
                assert_eq!(
                    cooked_floor(world, &reopened, region_tileset::SIDE_ROOM_FLOOR),
                    (shared_tileset, tileset_version)
                );
                assert_eq!(
                    world
                        .resource::<Assets<TileSet>>()
                        .slot_revision(shared_tileset),
                    tileset_slot
                );
                assert_eq!(
                    world
                        .resource::<Assets<Material2dAsset>>()
                        .slot_revision(material),
                    original_slot
                );
                reopened
            })
            .map_err(|error| error.to_string())?;
        step_command(session, 6, LOOKOUT)?;
        session
            .observe(|world| {
                let snapshot = world.resource::<GameSnapshot>();
                assert_eq!(snapshot.side_room, SideRoomPhase::Open);
                assert_eq!(snapshot.lookout, SideRoomPhase::Closed);
                assert_eq!(snapshot.current_instance, primary_id);
                assert_retired(world, &lookout);
                assert_eq!(
                    cooked_floor(world, &reopened, region_tileset::SIDE_ROOM_FLOOR),
                    (shared_tileset, tileset_version)
                );
                let tilesets = world.resource::<Assets<TileSet>>();
                assert_eq!(tilesets.slot_revision(shared_tileset), tileset_slot);
                assert!(
                    tilesets
                        .get(shared_tileset)
                        .unwrap()
                        .image_content()
                        .unwrap()
                        .same_allocation(retained_tileset.image_content().unwrap())
                );
                assert_eq!(
                    cooked_relic(world, side_room::SCENE_PATH, side_room::RELIC),
                    (
                        reopened.clone(),
                        shared_image,
                        shared_material,
                        shared_version
                    )
                );
                assert_eq!(
                    world
                        .resource::<Assets<Material2dAsset>>()
                        .get(shared_material)
                        .unwrap()
                        .generation(),
                    retained_material.generation()
                );
            })
            .map_err(|error| error.to_string())?;
        step_command(session, 7, LOOKOUT)?;
        wait_for_snapshot(session, "cooked lookout did not reopen", |snapshot| {
            assert_eq!(snapshot.current_instance, primary_id);
            assert_eq!(snapshot.side_room, SideRoomPhase::Open);
            snapshot.lookout == SideRoomPhase::Open
        })?;
        let reopened_lookout = session
            .observe(|world| {
                let (reopened, image, material, version) = cooked_relic(
                    world,
                    side_room::LOOKOUT_SCENE_PATH,
                    side_room::LOOKOUT_RELIC,
                );
                assert_ne!(reopened.instance_id(), lookout.instance_id());
                assert_eq!(image, shared_image);
                assert_eq!(material, shared_material);
                assert_eq!(version, shared_version);
                assert_eq!(
                    cooked_floor(world, &reopened, region_tileset::LOOKOUT_FLOOR),
                    (shared_tileset, tileset_version)
                );
                assert_eq!(
                    world
                        .resource::<Assets<TileSet>>()
                        .slot_revision(shared_tileset),
                    tileset_slot
                );
                let materials = world.resource::<Assets<Material2dAsset>>();
                assert_eq!(materials.slot_revision(material), original_slot);
                assert_eq!(
                    materials.get(material).unwrap().generation(),
                    retained_material.generation()
                );
                reopened
            })
            .map_err(|error| error.to_string())?;
        step_command(session, 8, TRAVEL)?;
        wait_for_snapshot(session, "cooked travel did not reach Cavern", |snapshot| {
            if snapshot.travel_count == 0 {
                assert_eq!(snapshot.current_instance, primary_id);
                assert_eq!(snapshot.side_room, SideRoomPhase::Open);
                assert_eq!(snapshot.lookout, SideRoomPhase::Open);
            }
            snapshot.level == Some(cavern) && snapshot.phase == GamePhase::Running
        })?;
        session
            .observe(|world| {
                let snapshot = world.resource::<GameSnapshot>();
                let current = CurrentScene::get(world).unwrap();
                assert_eq!(snapshot.travel_count, 1);
                assert_eq!(
                    snapshot.current_instance,
                    Some(current.receipt().instance_id())
                );
                assert_eq!(current.receipt().domain_id(), primary.domain_id());
                let cavern_path = format!("scenes/{}", LevelId::Cavern.scene_path());
                let is_cooked_cavern = current.source_view().with_runtime_scene(|scene| {
                    matches!(
                        scene.source_key().origin(),
                        SourceDocumentOrigin::ProjectLocation { path, .. }
                            if path.as_str() == cavern_path
                    )
                });
                assert_eq!(is_cooked_cavern, Some(true));
                assert_eq!(snapshot.side_room, SideRoomPhase::Closed);
                assert_eq!(snapshot.lookout, SideRoomPhase::Closed);
                for retired in [&primary, &room, &reopened, &lookout, &reopened_lookout] {
                    assert_ne!(current.receipt().instance_id(), retired.instance_id());
                    assert_retired(world, retired);
                }
                assert!(
                    world
                        .resource::<Assets<ImageAsset>>()
                        .get(shared_image)
                        .is_none()
                );
                assert!(world.resource::<Assets<Material2dAsset>>().is_empty());
                assert!(world.resource::<Assets<TileSet>>().is_empty());
                assert!(
                    world
                        .resource::<TileSetAssets>()
                        .desired(shared_tileset)
                        .is_none()
                );
                assert!(world.resource::<TileSetAssets>().sources().next().is_none());
                assert!(
                    world
                        .resource::<AssetStates>()
                        .state(shared_tileset.id())
                        .is_none_or(|state| state.usable_version().is_none())
                );
                let retained = retained_tileset.image_content().unwrap();
                assert!(!retained.is_authorized(
                    world.resource::<AssetServer>(),
                    world.resource::<AssetStates>(),
                    world.resource::<Assets<ImageAsset>>()
                ));
                assert!(
                    world
                        .resource::<Assets<Material2dAsset>>()
                        .get(shared_material)
                        .is_none()
                );
                assert!(
                    world
                        .resource::<AssetStates>()
                        .state(shared_material.id())
                        .is_none_or(|state| state.usable_version().is_none())
                );
                assert_resident_images(world, LevelId::Cavern, None);
            })
            .map_err(|error| error.to_string())?;
        two_level_pack::assert_room_material(&retained_material);
        let retained_image = retained_tileset.image_content().unwrap();
        assert_eq!(retained_image.product(), side_room::image_product());
        assert!(retained_image.source().is_none());
        assert_eq!(retained_image.pixels(), [64, 224, 180, 255].repeat(4));
        Ok(())
    }

    fn wait_for_snapshot(
        session: &mut HeadlessSession,
        timeout: &str,
        ready: impl Fn(&GameSnapshot) -> bool,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = session
                .observe(|world| world.resource::<GameSnapshot>().clone())
                .map_err(|error| error.to_string())?;
            if let Some(error) = &snapshot.error {
                return Err(error.clone());
            }
            if ready(&snapshot) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(timeout.into());
            }
            session
                .frame(Duration::ZERO)
                .map_err(|error| error.to_string())?;
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn cooked_relic(
        world: &World,
        scene_path: &str,
        relic_id: &str,
    ) -> (
        SpawnedSceneInstance,
        Handle<ImageAsset>,
        Handle<Material2dAsset>,
        AssetVersion,
    ) {
        let scene_path = format!("scenes/{scene_path}");
        let mut relics = world.iter_entities().filter(|entity| {
            entity.get::<SceneEntitySource>().is_some_and(|source| {
                source.entity_id.as_str() == relic_id
                    && matches!(source.containing_source().origin(),
                        SourceDocumentOrigin::ProjectLocation { path, .. }
                            if path.as_str() == scene_path)
            })
        });
        let relic = relics
            .next()
            .expect("the cooked region publishes its relic");
        assert!(relics.next().is_none());
        let source = relic.get::<SceneEntitySource>().unwrap();
        let receipt = world
            .resource::<WorldIdentityDomain>()
            .active_scene_instance(world, source.instance_id)
            .unwrap();
        assert_eq!(
            receipt.resolve(world, &source.entity_id),
            EntityLookup::Resolved(relic.id())
        );
        let handle = relic.get::<Sprite>().unwrap().material.image.unwrap();
        let image = world.resource::<Assets<ImageAsset>>().get(handle).unwrap();
        two_level_pack::assert_room_image(image);
        let material = relic
            .get::<Material2d>()
            .expect("the cooked relic uses the authored Material");
        assert_eq!(material.declaration_id(), region_material::MATERIAL_ID);
        assert_eq!(
            material.declaration_version(),
            region_material::MATERIAL_VERSION
        );
        assert_eq!(material.parameters(), &region_material::parameters());
        assert!(material.texture_overrides().is_empty());
        let materials = world.resource::<Assets<Material2dAsset>>();
        assert_eq!(materials.len(), 1);
        let asset = materials.get(material.handle()).unwrap();
        two_level_pack::assert_room_material(asset);
        let server = world.resource::<AssetServer>();
        assert_eq!(
            server.product(material.handle().id()),
            Some(region_material::material_product())
        );
        assert_eq!(
            server.get_product_handle::<Material2dAsset>(region_material::material_product()),
            Some(material.handle())
        );
        assert!(server.path(material.handle().id()).is_none());
        let binding = material
            .bind(asset, |reference| {
                let AssetRef::Product(product) = reference else {
                    panic!("cooked Material textures must retain their original Image Products");
                };
                server.get_product_handle::<ImageAsset>(*product)
            })
            .unwrap();
        assert_eq!(binding.generation(), asset.generation());
        assert_eq!(binding.parameters(), &region_material::parameters());
        let palette = binding.textures()[region_material::PALETTE_ID];
        assert_eq!(
            server.product(palette.id()),
            Some(region_material::palette_product())
        );
        assert!(
            world
                .resource::<Assets<ImageAsset>>()
                .get(palette)
                .is_some()
        );
        let state = world
            .resource::<AssetStates>()
            .state(material.handle().id())
            .unwrap();
        assert!(state.source_hash().is_none());
        assert!(state.import_recipe_digest().is_none());
        let version = state
            .usable_version()
            .expect("the cooked shared Material is usable");
        (receipt, handle, material.handle(), version)
    }

    fn cooked_floor(
        world: &World,
        receipt: &SpawnedSceneInstance,
        floor_id: &str,
    ) -> (Handle<TileSet>, AssetVersion) {
        let mut floors = world.iter_entities().filter(|entity| {
            entity.get::<SceneEntitySource>().is_some_and(|source| {
                source.instance_id == receipt.instance_id() && source.entity_id.as_str() == floor_id
            })
        });
        let floor = floors
            .next()
            .expect("the cooked region publishes its Tilemap floor");
        assert!(floors.next().is_none());
        let source = floor.get::<SceneEntitySource>().unwrap();
        assert_eq!(
            receipt.resolve(world, &source.entity_id),
            EntityLookup::Resolved(floor.id())
        );
        let tilemap = floor
            .get::<Tilemap>()
            .expect("the region floor retains its authored Tilemap");
        assert!(floor.get::<Sprite>().is_none());
        assert_eq!(tilemap.tile_size, region_tileset::TILE_SIZE);
        assert_eq!(tilemap.layer.index, region_tileset::FLOOR_LAYER);
        assert_eq!(tilemap.sort_key, 0);
        assert!(tilemap.y_sort_anchor.is_none());
        let expected = if floor_id == region_tileset::SIDE_ROOM_FLOOR {
            region_tileset::side_room_cells()
        } else {
            assert_eq!(floor_id, region_tileset::LOOKOUT_FLOOR);
            region_tileset::lookout_cells()
        };
        assert_eq!(tilemap.cell_count(), expected.len());
        assert_eq!(
            tilemap
                .cells()
                .map(|(coord, cell)| (coord, *cell))
                .collect::<Vec<_>>(),
            expected
        );
        let handle = tilemap
            .tileset
            .expect("the cooked Tilemap binds its TileSet");
        let tilesets = world.resource::<Assets<TileSet>>();
        assert_eq!(tilesets.len(), 1);
        let tileset = tilesets.get(handle).unwrap();
        let descriptor = region_tileset::descriptor();
        assert_eq!(tileset.atlas(), Some(descriptor.atlas()));
        assert_eq!(tileset.tile_size(), descriptor.atlas().tile_size);
        assert_eq!(tileset.material().sampler, descriptor.sampler());
        assert_eq!(tileset.material().alpha_mode, descriptor.alpha_mode());
        assert_eq!(tileset.material().tint, descriptor.tint());
        let server = world.resource::<AssetServer>();
        assert_eq!(
            server.product(handle.id()),
            Some(region_tileset::tileset_product())
        );
        assert_eq!(
            server.get_product_handle::<TileSet>(region_tileset::tileset_product()),
            Some(handle)
        );
        assert!(server.path(handle.id()).is_none());
        let image_handle = tileset.material().image.unwrap();
        assert_eq!(
            server.get_product_handle::<ImageAsset>(side_room::image_product()),
            Some(image_handle)
        );
        assert!(server.path(image_handle.id()).is_none());
        let images = world.resource::<Assets<ImageAsset>>();
        let image = images.get(image_handle).unwrap();
        two_level_pack::assert_room_image(image);
        let content = tileset
            .image_content()
            .expect("the cooked TileSet captures its actual Image");
        assert_eq!(content.handle(), image_handle);
        assert_eq!(content.product(), side_room::image_product());
        assert_eq!(content.extent(), image.extent());
        assert_eq!(content.pixels(), image.pixels());
        assert!(content.source().is_none());
        let states = world.resource::<AssetStates>();
        assert!(content.is_current(server, states, images));
        let image_state = states.state(image_handle.id()).unwrap();
        assert!(image_state.source_hash().is_none());
        assert!(image_state.import_recipe_digest().is_none());
        let state = states.state(handle.id()).unwrap();
        assert!(state.source_hash().is_none());
        assert!(state.import_recipe_digest().is_none());
        let owner = world.resource::<TileSetAssets>();
        assert!(owner.desired(handle).is_none());
        assert!(owner.sources().next().is_none());
        assert!(owner.error(handle).is_none());
        (
            handle,
            state
                .usable_version()
                .expect("the cooked TileSet is usable"),
        )
    }

    fn assert_retired(world: &World, receipt: &SpawnedSceneInstance) {
        assert!(matches!(
            world
                .resource::<WorldIdentityDomain>()
                .active_scene_instance(world, receipt.instance_id()),
            Err(IdentityDomainError::SceneInstanceNotActive { instance })
                if instance == receipt.instance_id()
        ));
        for id in receipt.entity_ids() {
            assert!(matches!(
                receipt.resolve(world, id),
                EntityLookup::Tombstoned(_)
            ));
        }
        assert!(world.iter_entities().all(|entity| {
            entity
                .get::<SceneEntitySource>()
                .is_none_or(|source| source.instance_id != receipt.instance_id())
        }));
    }

    fn assert_resident_images(world: &World, level: LevelId, extra: Option<AssetProductRef>) {
        let images = world.resource::<Assets<ImageAsset>>();
        let actual = images
            .iter()
            .map(|(_, image)| {
                assert!(image.source().is_none());
                image.product().expect("a cooked Image retains its Product")
            })
            .collect::<BTreeSet<_>>();
        let expected = [
            level.image_product(),
            atlas_product(),
            marker_image_product(),
        ]
        .into_iter()
        .chain(extra)
        .collect::<BTreeSet<_>>();
        assert_eq!(images.len(), actual.len());
        assert_eq!(actual, expected);
    }

    fn assert_cooked_marker_image(world: &World) -> Result<(), String> {
        let root = world
            .iter_entities()
            .find(|entity| entity.contains::<MarkerLink>())
            .ok_or("processed gameplay did not place its marker")?;
        let link = root
            .get::<MarkerLink>()
            .ok_or("processed marker lost its child reference")?;
        let source = root
            .get::<SceneEntitySource>()
            .ok_or("processed marker has no Scene owner")?;
        let domain = world.resource::<WorldIdentityDomain>();
        let EntityLookup::Resolved(child) =
            domain.resolve_entity_reference(world, &link.target, Some(source.instance_id))
        else {
            return Err("processed marker link does not resolve its authored child".into());
        };
        let image = world
            .get::<Sprite>(child)
            .and_then(|sprite| sprite.material.image)
            .and_then(|handle| world.resource::<Assets<ImageAsset>>().get(handle))
            .ok_or("processed marker child did not bind a resident texture")?;
        if image.product() != Some(marker_image_product())
            || image.source().is_some()
            || image.extent() != ImageExtent::new(2, 2)
            || image.pixels() != [255, 192, 64, 255].repeat(4)
        {
            return Err("processed marker child binds the wrong retained Image".into());
        }
        Ok(())
    }

    fn default_animation_proof() -> serde_json::Value {
        serde_json::json!({
            "product": clips_product().to_string(),
            "source_free": true,
            "clips": [
                {"name": "idle", "mode": "once", "frames": [
                    {"min": [0.0_f32, 0.0_f32], "size": [0.25_f32, 1.0_f32], "duration_nanos": 100_000_000_u64},
                ]},
                {"name": "walk", "mode": "loop", "frames": [
                    {"min": [0.25_f32, 0.0_f32], "size": [0.25_f32, 1.0_f32], "duration_nanos": 100_000_000_u64},
                    {"min": [0.5_f32, 0.0_f32], "size": [0.25_f32, 1.0_f32], "duration_nanos": 100_000_000_u64},
                    {"min": [0.75_f32, 0.0_f32], "size": [0.25_f32, 1.0_f32], "duration_nanos": 100_000_000_u64},
                ]},
            ],
        })
    }
}
