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
use nara::{app::PluginId, project::ProjectManifest, project_host::resolve_runtime_configuration};

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
    let configuration = settings.runtime_configuration().unwrap();
    assert!(configuration.input().action_map.is_none());
    resolve_runtime_configuration(&configuration, runtime.recipe().unwrap()).unwrap()
}

#[test]
fn headless_cooking_excludes_native_input_even_when_desktop_is_compiled() {
    let selected = RuntimeKind::parse("headless").unwrap();
    let plan = plan(selected);
    assert!(!plan.settings().window().enabled);
    assert!(
        plan.plugin_plan()
            .entries()
            .iter()
            .all(|entry| { entry.plugin_id() != PluginId::new("two-level-game.desktop-input") })
    );
    assert_eq!(selected.binary(), "player");
    assert_eq!(selected.features(), ["cooked-player"]);
    assert_eq!(selected.content_profile(), "two-level-shipping");
}

#[cfg(feature = "desktop")]
#[test]
fn desktop_selection_binds_its_profile_and_compiled_game_input() {
    let selected = RuntimeKind::parse("desktop").unwrap();
    let plan = plan(selected);
    assert!(plan.settings().window().enabled);
    assert_eq!(
        plan.plugin_plan()
            .entries()
            .iter()
            .filter(|entry| entry.plugin_id() == PluginId::new("two-level-game.desktop-input"))
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
        asset_build::ArtifactBuildObservation,
        project_host::{HeadlessSession, HeadlessSessionClose},
        project_tooling::host_fs,
        runtime_content::{RuntimeContentDependencyRole, RuntimeContentMemberKind},
    };
    use two_level_game::{
        Scenario, animation::atlas_product, levels::LevelId, sound_bank::bank_product,
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
    fn cooking_shares_one_atlas_preserves_raw_inputs_and_decodes_source_free_player_evidence() {
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
        assert_eq!(catalog.members().len(), 6);
        assert_eq!(
            catalog
                .members()
                .iter()
                .flat_map(|member| member.products().iter().copied())
                .collect::<BTreeSet<_>>(),
            LevelId::ALL
                .into_iter()
                .flat_map(|level| [level.scene_product(), level.image_product()])
                .chain([atlas_product(), bank_product()])
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
                [level.image_product(), atlas_product(), bank_product()]
                    .into_iter()
                    .map(|product| (product, RuntimeContentDependencyRole::Required))
                    .collect::<BTreeSet<_>>()
            );
        }
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
                "assets/audio/game.soundbank.json",
                "assets/audio/pickup.wav",
                "assets/audio/travel.wav",
            ])
        );
        fence.bind_raw_inputs(&observations).unwrap();

        let mut session = HeadlessSession::from_runtime_content(prepared.into_base());
        let mut evidence = Evidence::default();
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
        let result = observed.unwrap();
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
}
