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
mod atlas_content {
    use std::{
        collections::BTreeSet,
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };

    use nara::{
        asset::RuntimeAssetCodec,
        asset_build::ArtifactBuildObservation,
        input::{
            ActionMapBuilder, ActionMapFileCandidate, ActionMapLimits, ActionMapRuntimeCodec,
            InputBinding, KeyCode,
        },
        project_host::{HeadlessSession, HeadlessSessionClose},
        project_tooling::host_fs,
        project_workflow::cooked_content::PreparedRuntimeContent,
        runtime_content::{RuntimeContentDependencyRole, RuntimeContentMemberKind},
    };
    use nara_sprite_animation::{SpriteAnimationCodec, SpriteAnimationDomain};
    use two_level_game::{
        Scenario,
        animation::{atlas_product, clips_product},
        levels::LevelId,
        sound_bank::bank_product,
    };

    use super::{RuntimeKind, content, evidence::Evidence, source::ContentFence};

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
                "assets/textures/meadow.png",
                "assets/textures/meadow.png.meta",
                "assets/textures/cavern.png",
                "assets/textures/cavern.png.meta",
                "assets/textures/explorer.png",
                "assets/textures/explorer.png.meta",
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
        assert_eq!(catalog.members().len(), 8);
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
            assert_eq!(
                scene
                    .dependencies()
                    .iter()
                    .map(|dependency| (dependency.product(), dependency.role()))
                    .collect::<BTreeSet<_>>(),
                [
                    level.image_product(),
                    atlas_product(),
                    bank_product(),
                    clips_product()
                ]
                .into_iter()
                .map(|product| (product, RuntimeContentDependencyRole::Required))
                .collect::<BTreeSet<_>>()
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
        assert_eq!(proofs.len(), 2);
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
        assert_eq!(
            result["observed_image_products"]
                .as_array()
                .unwrap()
                .iter()
                .map(|product| product.as_str().unwrap().to_owned())
                .collect::<BTreeSet<_>>(),
            [LevelId::STARTUP.image_product(), atlas_product()]
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
            assert_eq!(
                prepared
                    .base()
                    .catalog()
                    .member_for_product(level.scene_product())
                    .unwrap()
                    .dependencies()
                    .iter()
                    .map(|dependency| dependency.product())
                    .collect::<BTreeSet<_>>(),
                [
                    level.image_product(),
                    atlas_product(),
                    bank_product(),
                    clips_product(),
                ]
                .into_iter()
                .collect()
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
            Ok(evidence.result(Scenario::Interactive, None, None))
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
