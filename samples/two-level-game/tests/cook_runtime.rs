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
#[cfg(any(feature = "development", feature = "cook"))]
#[allow(dead_code)]
#[path = "../src/bin/cook/source.rs"]
mod source;

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
        collections::BTreeSet,
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };

    use nara::{
        asset::{AssetRef, Assets, RuntimeAssetCodec},
        asset_build::ArtifactBuildObservation,
        ecs::World,
        gameplay::{
            GameplayCommandDraft, GameplayCommandIngressSource, GameplayCommandSourceSequence,
            GameplayCommandSubmission, GameplayCommandTick, GameplayCommandTypeId,
        },
        identity::{EntityLookup, WorldIdentityDomain},
        image::{ImageAsset, ImageExtent},
        input::{
            ActionMapBuilder, ActionMapFileCandidate, ActionMapLimits, ActionMapRuntimeCodec,
            InputBinding, KeyCode,
        },
        project_host::{HeadlessSession, HeadlessSessionClose},
        project_tooling::host_fs,
        project_workflow::cooked_content::PreparedRuntimeContent,
        reflect::{ComponentTypeId, decode_asset_reference_value},
        runtime_content::{
            RuntimeContentDependency, RuntimeContentDependencyRole, RuntimeContentError,
            RuntimeContentMemberKind,
        },
        scene::{SceneEntitySource, SourceDocumentKind, SourceDocumentOrigin},
        sprite::Sprite,
    };
    use nara_sprite_animation::{SpriteAnimationCodec, SpriteAnimationDomain};
    use two_level_game::{
        GameSnapshot, Scenario, SideRoomPhase,
        animation::{atlas_product, clips_product},
        levels::LevelId,
        markers::{MarkerLink, SCENE_PATH, marker_image_product, marker_product},
        side_room,
        sound_bank::bank_product,
    };

    use super::{RuntimeKind, content, evidence::Evidence, source::ContentFence, two_level_pack};

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
                "scenes/cavern.scene.json",
                "scenes/marker.scene.json",
                "scenes/marker.scene.json.meta",
                "scenes/side-room.scene.json",
                "scenes/side-room.scene.json.meta",
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

    #[test]
    fn cooking_shares_atlas_and_clips_preserves_raw_inputs_and_decodes_source_free_player_evidence()
    {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"));
        let cache = Cache::new();
        let mut fence = ContentFence::capture(project).unwrap();
        let (prepared, observations) = content::prepare(
            fence.root(),
            &host_fs::writable_directory(&cache.path).unwrap(),
            "x86_64-pc-windows-msvc",
            RuntimeKind::Headless,
        )
        .unwrap();
        let catalog = prepared.base().catalog();
        assert_eq!(catalog.members().len(), 12);
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
            4
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
        two_level_pack::assert_room_member(
            catalog
                .member_for_product(side_room::scene_product())
                .unwrap(),
        );
        assert!(matches!(
            prepared.base().acquire_scene(side_room::scene_product()),
            Err(RuntimeContentError::UndeclaredScene)
        ));
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
                    side_room::image_product(),
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
        let raw = observations
            .iter()
            .map(|observation| match observation {
                ArtifactBuildObservation::Raw {
                    path,
                    digest: Some(_),
                } => path.as_str(),
                other => panic!("unexpected source import observation: {other:?}"),
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(observations.len(), raw.len());
        assert_eq!(
            raw,
            BTreeSet::from([
                "assets/textures/meadow.png",
                "assets/textures/cavern.png",
                "assets/textures/explorer.png",
                "assets/textures/marker.png",
                "assets/textures/side-room.png",
                "assets/animations/explorer.sprite-animations.json",
                "assets/audio/game.soundbank.json",
                "assets/audio/pickup.wav",
                "assets/audio/travel.wav",
                super::INPUT_PATH,
            ])
        );
        fence.bind_raw_inputs(&observations).unwrap();
        let result = observe_startup(prepared);
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
        let mut fence = ContentFence::capture(&project).unwrap();
        let store = cache.path.join("store");
        fs::create_dir(&store).unwrap();
        let (prepared, observations) = content::prepare(
            fence.root(),
            &host_fs::writable_directory(&store).unwrap(),
            "x86_64-pc-windows-msvc",
            RuntimeKind::Headless,
        )
        .unwrap();
        fence.bind_raw_inputs(&observations).unwrap();
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
        assert!(fence.verify().unwrap_err().contains(super::INPUT_PATH));
        fs::remove_file(&input_path).unwrap();
        assert!(!input_path.exists());
        let result = observe_startup(prepared);
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
        let (prepared, _) = content::prepare(
            &root,
            &store,
            "x86_64-pc-windows-msvc",
            RuntimeKind::Headless,
        )
        .unwrap();
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
            let error = content::prepare(
                &root,
                &store,
                "x86_64-pc-windows-msvc",
                RuntimeKind::Headless,
            )
            .err()
            .unwrap();
            assert!(
                error.contains("conflicts with the intrinsic Input identity"),
                "{field}: {error}"
            );
        }
    }

    fn observe_startup(prepared: PreparedRuntimeContent) -> serde_json::Value {
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
            check_cooked_side_room(&mut session)?;
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

    fn check_cooked_side_room(session: &mut HeadlessSession) -> Result<(), String> {
        const COMMAND: &str = "two-level-game.side-room";
        let initial = session
            .observe(|world| world.resource::<GameSnapshot>().current_instance)
            .map_err(|error| error.to_string())?;
        step_command(session, 2, COMMAND)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = session
                .observe(|world| world.resource::<GameSnapshot>().clone())
                .map_err(|error| error.to_string())?;
            assert_eq!(snapshot.current_instance, initial);
            if let Some(error) = snapshot.error {
                return Err(error);
            }
            if snapshot.side_room == SideRoomPhase::Open {
                break;
            }
            if Instant::now() >= deadline {
                return Err("cooked side room did not open".into());
            }
            session
                .frame(Duration::ZERO)
                .map_err(|error| error.to_string())?;
            std::thread::sleep(Duration::from_millis(1));
        }
        session
            .observe(|world| {
                let relic = world
                    .iter_entities()
                    .find(|entity| {
                        entity
                            .get::<SceneEntitySource>()
                            .is_some_and(|source| source.entity_id.as_str() == side_room::RELIC)
                    })
                    .expect("the cooked room publishes its actual relic");
                let sprite = relic.get::<Sprite>().unwrap();
                let image = world
                    .resource::<Assets<ImageAsset>>()
                    .get(sprite.material.image.unwrap())
                    .unwrap();
                two_level_pack::assert_room_image(image);
            })
            .map_err(|error| error.to_string())?;
        step_command(session, 3, COMMAND)?;
        session
            .observe(|world| {
                let snapshot = world.resource::<GameSnapshot>();
                assert_eq!(snapshot.side_room, SideRoomPhase::Closed);
                assert_eq!(snapshot.current_instance, initial);
                assert!(
                    world
                        .resource::<Assets<ImageAsset>>()
                        .iter()
                        .all(|(_, image)| image.product() != Some(side_room::image_product()))
                );
            })
            .map_err(|error| error.to_string())?;
        Ok(())
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
