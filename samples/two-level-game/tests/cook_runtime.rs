//! The source oracle uses public manifest ingestion, the selected recipe and strict delivery.
//! Preparation retains the shared production source selection. Raw paths are relative to
//! the project root; importer and delivery observations may repeat a path with the same digest.
//! Preparation precedes logical Player bootstrap generation. Source-free Session acceptance
//! consumes the prepared base after the copied project and import cache have been removed.

#![cfg(feature = "local-player")]
#![forbid(unsafe_code)]

#[cfg(any(feature = "development", feature = "cook"))]
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
use nara::{
    app::PluginId, input::ActionMapFileCandidate, project::ProjectManifest,
    project_host::resolve_runtime_configuration,
};
use two_level_game::delivery::{ControllerBackend, GameFlow, PlayerSelection, RuntimeKind};
#[cfg(any(feature = "development", feature = "cook"))]
use two_level_game::input::{INPUT_PATH, input_product};

fn keyboard(runtime: RuntimeKind) -> PlayerSelection {
    PlayerSelection::new(runtime, ControllerBackend::Disabled).unwrap()
}

#[cfg(any(feature = "development", feature = "cook"))]
fn plan(selection: PlayerSelection) -> nara::project_host::RuntimeConfigurationPlan {
    let manifest = ProjectManifest::parse_toml_bytes(include_bytes!("../nara.toml"));
    assert!(!manifest.has_errors(), "{:?}", manifest.diagnostics);
    let settings = manifest
        .manifest
        .unwrap()
        .resolve_profile(selection.manifest_profile())
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
    resolve_runtime_configuration(&configuration, selection.recipe().unwrap()).unwrap()
}

#[cfg(any(feature = "development", feature = "cook"))]
#[test]
fn headless_cooking_selects_authored_semantics_without_a_native_window() {
    let selected = keyboard(RuntimeKind::parse("headless").unwrap());
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
    assert_eq!(selected.flow(), GameFlow::Exploration);
    assert_eq!(selected.manifest_profile(), None);
}

#[cfg(feature = "desktop")]
#[cfg(any(feature = "development", feature = "cook"))]
#[test]
fn desktop_selection_binds_its_profile_and_authored_game_input() {
    let selected = keyboard(RuntimeKind::parse("desktop").unwrap());
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
        self::plan(keyboard(RuntimeKind::Headless))
            .plugin_plan()
            .fingerprint()
    );
    assert_eq!(selected.binary(), "player_desktop");
    assert_eq!(selected.features(), ["cooked-player", "desktop"]);
    assert_eq!(selected.content_profile(), "two-level-desktop-shipping");
    assert_eq!(selected.flow(), GameFlow::Exploration);
    assert_eq!(selected.manifest_profile(), Some("desktop"));
}

#[cfg(all(windows, feature = "desktop", feature = "audio-cpal"))]
#[cfg(any(feature = "development", feature = "cook"))]
#[test]
fn audible_selection_is_explicit_and_never_opens_a_device_during_composition() {
    let selected = keyboard(RuntimeKind::parse("audio-desktop").unwrap());
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
    assert_eq!(selected.flow(), GameFlow::Exploration);
    assert_eq!(selected.manifest_profile(), Some("desktop"));
    assert_eq!(
        plan.plugin_plan()
            .entries()
            .iter()
            .filter(|entry| entry.plugin_id() == two_level_game::audio::AUDIO_PLUGIN_ID)
            .count(),
        1
    );
    assert!(
        self::plan(keyboard(RuntimeKind::Desktop))
            .plugin_plan()
            .entries()
            .iter()
            .all(|entry| entry.plugin_id() != two_level_game::audio::AUDIO_PLUGIN_ID)
    );
    assert_ne!(
        plan.plugin_plan().fingerprint(),
        self::plan(keyboard(RuntimeKind::Desktop))
            .plugin_plan()
            .fingerprint()
    );
}

#[test]
fn an_unknown_runtime_cannot_select_a_player() {
    assert!(RuntimeKind::parse("Desktop").is_err());
    assert!(RuntimeKind::parse("anything").is_err());
    #[cfg(not(all(windows, feature = "desktop", feature = "audio-cpal")))]
    {
        assert!(RuntimeKind::parse("audio-desktop").is_err());
        assert!(
            PlayerSelection::new(RuntimeKind::AudioDesktop, ControllerBackend::Disabled).is_err()
        );
        assert!(PlayerSelection::new(RuntimeKind::AudioDesktop, ControllerBackend::Gilrs).is_err());
        for profile in [
            "two-level-audio-desktop-shipping",
            "two-level-audio-desktop-gamepad-shipping",
            "two-level-round-audio-desktop-shipping",
            "two-level-round-audio-desktop-gamepad-shipping",
        ] {
            assert!(PlayerSelection::from_content_profile(profile).is_err());
        }
    }
    #[cfg(not(feature = "desktop"))]
    {
        assert!(RuntimeKind::parse("desktop").is_err());
        assert!(PlayerSelection::new(RuntimeKind::Desktop, ControllerBackend::Disabled).is_err());
        assert!(PlayerSelection::from_content_profile("two-level-desktop-shipping").is_err());
        assert!(PlayerSelection::from_content_profile("two-level-round-desktop-shipping").is_err());
    }
}

#[test]
fn flow_names_are_explicit_and_headless_round_is_always_refused() {
    assert_eq!(GameFlow::default(), GameFlow::Exploration);
    for (name, flow) in [
        ("exploration", GameFlow::Exploration),
        ("round", GameFlow::Round),
    ] {
        assert_eq!(GameFlow::parse(name).unwrap(), flow);
        assert_eq!(flow.name(), name);
    }
    for name in ["", "Round", "play-flow", "anything"] {
        assert!(GameFlow::parse(name).is_err(), "{name}");
    }
    let headless = keyboard(RuntimeKind::Headless);
    assert_eq!(headless.with_flow(GameFlow::Exploration).unwrap(), headless);
    assert!(
        headless
            .with_flow(GameFlow::Round)
            .unwrap_err()
            .contains("desktop Player")
    );
    #[cfg(not(feature = "play-flow"))]
    {
        for profile in [
            "two-level-round-desktop-shipping",
            "two-level-round-desktop-gamepad-shipping",
            "two-level-round-audio-desktop-shipping",
            "two-level-round-audio-desktop-gamepad-shipping",
        ] {
            assert!(PlayerSelection::from_content_profile(profile).is_err());
        }
        #[cfg(feature = "desktop")]
        assert!(
            keyboard(RuntimeKind::Desktop)
                .with_flow(GameFlow::Round)
                .unwrap_err()
                .contains("play-flow Cargo feature")
        );
    }
}

#[cfg(any(feature = "development", feature = "cook"))]
fn cook_options(extra: &[&str]) -> Result<input::Options, String> {
    use std::{ffi::OsString, path::Path};

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut arguments = Vec::new();
    for flag in [
        "--project-root",
        "--output-parent",
        "--target-dir",
        "--cargo",
        "--rustc",
    ] {
        arguments.push(OsString::from(flag));
        arguments.push(root.as_os_str().to_owned());
    }
    arguments.extend(
        [
            "--name",
            "flow-control",
            "--target",
            "x86_64-pc-windows-msvc",
        ]
        .into_iter()
        .chain(extra.iter().copied())
        .map(OsString::from),
    );
    input::Options::from_args(arguments)
}

#[cfg(any(feature = "development", feature = "cook"))]
#[test]
fn cooker_defaults_to_exploration_and_refuses_invalid_flow_arguments() {
    let default = cook_options(&[]).unwrap().selection;
    assert_eq!(default, keyboard(RuntimeKind::Headless));
    assert_eq!(
        cook_options(&["--flow", "exploration"]).unwrap().selection,
        default
    );
    for (arguments, message) in [
        (vec!["--flow"], "missing value for --flow"),
        (
            vec!["--flow", "Round"],
            "--flow requires exploration or round",
        ),
        (
            vec!["--flow", "anything"],
            "--flow requires exploration or round",
        ),
        (
            vec!["--flow", "exploration", "--flow", "round"],
            "duplicate argument",
        ),
        (
            vec!["--game-flow", "round"],
            "unknown argument: --game-flow",
        ),
        (vec!["--flow", "round"], "desktop Player"),
    ] {
        let error = cook_options(&arguments).err().unwrap();
        assert!(error.contains(message), "{arguments:?}: {error}");
    }
    #[cfg(all(feature = "desktop", not(feature = "play-flow")))]
    assert!(
        cook_options(&["--runtime", "desktop", "--flow", "round"])
            .err()
            .unwrap()
            .contains("play-flow Cargo feature")
    );
    #[cfg(feature = "play-flow")]
    {
        let selected = cook_options(&["--flow", "round", "--runtime", "desktop"])
            .unwrap()
            .selection;
        assert_eq!(
            selected,
            keyboard(RuntimeKind::Desktop)
                .with_flow(GameFlow::Round)
                .unwrap()
        );
        assert_eq!(selected.manifest_profile(), Some("play-flow"));
    }
}

#[cfg(feature = "play-flow")]
#[cfg(any(feature = "development", feature = "cook"))]
#[test]
fn round_recipes_select_ui_owners_without_acquiring_native_devices() {
    for (runtime, controllers, expected_features) in [
        (
            RuntimeKind::Desktop,
            ControllerBackend::Disabled,
            vec!["cooked-player", "desktop", "play-flow"],
        ),
        #[cfg(all(windows, feature = "audio-cpal"))]
        (
            RuntimeKind::AudioDesktop,
            ControllerBackend::Disabled,
            vec!["cooked-player", "desktop", "audio-cpal", "play-flow"],
        ),
        #[cfg(all(windows, feature = "gamepad"))]
        (
            RuntimeKind::Desktop,
            ControllerBackend::Gilrs,
            vec!["cooked-player", "desktop", "gamepad", "play-flow"],
        ),
        #[cfg(all(windows, feature = "audio-cpal", feature = "gamepad"))]
        (
            RuntimeKind::AudioDesktop,
            ControllerBackend::Gilrs,
            vec![
                "cooked-player",
                "desktop",
                "audio-cpal",
                "gamepad",
                "play-flow",
            ],
        ),
    ] {
        let exploration = PlayerSelection::new(runtime, controllers).unwrap();
        let selected = exploration.with_flow(GameFlow::Round).unwrap();
        assert_eq!(selected.flow(), GameFlow::Round);
        assert_eq!(selected.runtime(), runtime);
        assert_eq!(selected.controllers(), controllers);
        assert_eq!(selected.binary(), exploration.binary());
        assert_eq!(selected.manifest_profile(), Some("play-flow"));
        assert_eq!(selected.features(), expected_features);
        let plan = plan(selected);
        let original = self::plan(exploration);
        assert!(plan.settings().window().enabled);
        for plugin in [
            "two-level-game.input",
            "two-level-game.play-flow",
            "two-level-game.play-flow.hud",
            "nara.ui",
            "nara.ui-render",
        ] {
            assert_eq!(
                plan.plugin_plan()
                    .entries()
                    .iter()
                    .filter(|entry| entry.plugin_id() == PluginId::new(plugin))
                    .count(),
                1,
                "{selected:?}: {plugin}"
            );
        }
        assert_eq!(
            plan.plugin_plan()
                .entries()
                .iter()
                .filter(|entry| entry.plugin_id() == PluginId::new("nara.gilrs"))
                .count(),
            usize::from(controllers == ControllerBackend::Gilrs)
        );
        #[cfg(all(windows, feature = "audio-cpal"))]
        assert_eq!(
            plan.plugin_plan()
                .entries()
                .iter()
                .filter(|entry| entry.plugin_id() == two_level_game::audio::AUDIO_PLUGIN_ID)
                .count(),
            usize::from(runtime == RuntimeKind::AudioDesktop)
        );
        assert!(original.plugin_plan().entries().iter().all(|entry| {
            ![
                PluginId::new("two-level-game.play-flow"),
                PluginId::new("two-level-game.play-flow.hud"),
                PluginId::new("nara.ui"),
                PluginId::new("nara.ui-render"),
            ]
            .contains(&entry.plugin_id())
        }));
        assert_ne!(
            plan.plugin_plan().fingerprint(),
            original.plugin_plan().fingerprint()
        );
        let restored = selected.with_flow(GameFlow::Exploration).unwrap();
        assert_eq!(restored, exploration);
        assert_eq!(restored.features(), exploration.features());
        assert_eq!(
            self::plan(restored).plugin_plan().fingerprint(),
            original.plugin_plan().fingerprint()
        );
    }
}

#[cfg(any(feature = "development", feature = "cook"))]
#[test]
fn keyboard_selections_never_add_native_controller_ingress() {
    for runtime in [
        RuntimeKind::Headless,
        #[cfg(feature = "desktop")]
        RuntimeKind::Desktop,
        #[cfg(all(windows, feature = "desktop", feature = "audio-cpal"))]
        RuntimeKind::AudioDesktop,
    ] {
        let selected = keyboard(runtime);
        let plan = plan(selected);
        assert_eq!(selected.controllers(), ControllerBackend::Disabled);
        assert!(
            !selected
                .features()
                .iter()
                .any(|feature| feature == "gamepad")
        );
        assert!(
            plan.plugin_plan()
                .entries()
                .iter()
                .all(|entry| entry.plugin_id() != PluginId::new("nara.gilrs"))
        );
        assert_eq!(
            plan.plugin_plan()
                .entries()
                .iter()
                .filter(|entry| entry.plugin_id() == PluginId::new("two-level-game.input"))
                .count(),
            1
        );
    }
}

#[cfg(all(windows, feature = "gamepad"))]
#[cfg(any(feature = "development", feature = "cook"))]
#[test]
fn explicit_controllers_select_one_native_ingress_without_changing_authored_input() {
    let selected = PlayerSelection::new(RuntimeKind::Desktop, ControllerBackend::Gilrs).unwrap();
    let plan = plan(selected);
    let keyboard = self::plan(keyboard(RuntimeKind::Desktop));
    assert_eq!(selected.runtime(), RuntimeKind::Desktop);
    assert_eq!(selected.controllers(), ControllerBackend::Gilrs);
    assert_eq!(selected.binary(), "player_desktop");
    assert_eq!(selected.features(), ["cooked-player", "desktop", "gamepad"]);
    assert_eq!(
        selected.content_profile(),
        "two-level-desktop-gamepad-shipping"
    );
    assert!(plan.settings().window().enabled);
    assert_eq!(plan.settings(), keyboard.settings());
    for plugin in ["two-level-game.input", "nara.gilrs"] {
        assert_eq!(
            plan.plugin_plan()
                .entries()
                .iter()
                .filter(|entry| entry.plugin_id() == PluginId::new(plugin))
                .count(),
            1
        );
    }
    assert_ne!(
        plan.plugin_plan().fingerprint(),
        keyboard.plugin_plan().fingerprint()
    );
    assert_eq!(
        plan.schema_validation().composition_fingerprint(),
        keyboard.schema_validation().composition_fingerprint()
    );
    assert_eq!(
        plan.runtime_codecs_fingerprint(),
        keyboard.runtime_codecs_fingerprint()
    );
}

#[cfg(all(windows, feature = "gamepad", feature = "audio-cpal"))]
#[cfg(any(feature = "development", feature = "cook"))]
#[test]
fn audio_and_controllers_compose_without_acquiring_either_native_device() {
    let selected =
        PlayerSelection::new(RuntimeKind::AudioDesktop, ControllerBackend::Gilrs).unwrap();
    let plan = plan(selected);
    let keyboard = self::plan(keyboard(RuntimeKind::AudioDesktop));
    let silent =
        self::plan(PlayerSelection::new(RuntimeKind::Desktop, ControllerBackend::Gilrs).unwrap());
    assert_eq!(selected.binary(), "player_audio");
    assert_eq!(
        selected.features(),
        ["cooked-player", "desktop", "audio-cpal", "gamepad"]
    );
    assert_eq!(
        selected.content_profile(),
        "two-level-audio-desktop-gamepad-shipping"
    );
    assert!(plan.settings().window().enabled);
    assert_eq!(plan.settings(), keyboard.settings());
    for plugin in [
        PluginId::new("two-level-game.input"),
        PluginId::new("nara.gilrs"),
        two_level_game::audio::AUDIO_PLUGIN_ID,
    ] {
        assert_eq!(
            plan.plugin_plan()
                .entries()
                .iter()
                .filter(|entry| entry.plugin_id() == plugin)
                .count(),
            1
        );
    }
    assert_ne!(
        plan.plugin_plan().fingerprint(),
        keyboard.plugin_plan().fingerprint()
    );
    assert_ne!(
        plan.plugin_plan().fingerprint(),
        silent.plugin_plan().fingerprint()
    );
    assert_eq!(
        plan.schema_validation().composition_fingerprint(),
        keyboard.schema_validation().composition_fingerprint()
    );
    assert_eq!(
        plan.runtime_codecs_fingerprint(),
        keyboard.runtime_codecs_fingerprint()
    );
}

#[test]
fn invalid_or_unavailable_controllers_cannot_select_a_player() {
    assert_eq!(
        ControllerBackend::parse("none").unwrap(),
        ControllerBackend::Disabled
    );
    assert_eq!(
        ControllerBackend::parse("gilrs").unwrap(),
        ControllerBackend::Gilrs
    );
    assert_eq!(ControllerBackend::Disabled.name(), "none");
    assert_eq!(ControllerBackend::Gilrs.name(), "gilrs");
    for value in ["", "Gilrs", "keyboard", "anything"] {
        assert!(ControllerBackend::parse(value).is_err(), "{value}");
    }
    assert!(PlayerSelection::new(RuntimeKind::Headless, ControllerBackend::Gilrs).is_err());
    #[cfg(not(all(windows, feature = "gamepad")))]
    {
        for runtime in [
            RuntimeKind::Headless,
            RuntimeKind::Desktop,
            RuntimeKind::AudioDesktop,
        ] {
            assert!(PlayerSelection::new(runtime, ControllerBackend::Gilrs).is_err());
        }
        for profile in [
            "two-level-desktop-gamepad-shipping",
            "two-level-audio-desktop-gamepad-shipping",
            "two-level-round-desktop-gamepad-shipping",
            "two-level-round-audio-desktop-gamepad-shipping",
        ] {
            assert!(PlayerSelection::from_content_profile(profile).is_err());
        }
    }
}

#[test]
fn content_profiles_roundtrip_the_complete_player_selection() {
    let mut profiles = std::collections::BTreeSet::new();
    for (runtime, controllers, flow, profile) in [
        (
            RuntimeKind::Headless,
            ControllerBackend::Disabled,
            GameFlow::Exploration,
            "two-level-shipping",
        ),
        #[cfg(feature = "desktop")]
        (
            RuntimeKind::Desktop,
            ControllerBackend::Disabled,
            GameFlow::Exploration,
            "two-level-desktop-shipping",
        ),
        #[cfg(all(windows, feature = "desktop", feature = "audio-cpal"))]
        (
            RuntimeKind::AudioDesktop,
            ControllerBackend::Disabled,
            GameFlow::Exploration,
            "two-level-audio-desktop-shipping",
        ),
        #[cfg(all(windows, feature = "gamepad"))]
        (
            RuntimeKind::Desktop,
            ControllerBackend::Gilrs,
            GameFlow::Exploration,
            "two-level-desktop-gamepad-shipping",
        ),
        #[cfg(all(windows, feature = "gamepad", feature = "audio-cpal"))]
        (
            RuntimeKind::AudioDesktop,
            ControllerBackend::Gilrs,
            GameFlow::Exploration,
            "two-level-audio-desktop-gamepad-shipping",
        ),
        #[cfg(feature = "play-flow")]
        (
            RuntimeKind::Desktop,
            ControllerBackend::Disabled,
            GameFlow::Round,
            "two-level-round-desktop-shipping",
        ),
        #[cfg(all(windows, feature = "play-flow", feature = "audio-cpal"))]
        (
            RuntimeKind::AudioDesktop,
            ControllerBackend::Disabled,
            GameFlow::Round,
            "two-level-round-audio-desktop-shipping",
        ),
        #[cfg(all(windows, feature = "play-flow", feature = "gamepad"))]
        (
            RuntimeKind::Desktop,
            ControllerBackend::Gilrs,
            GameFlow::Round,
            "two-level-round-desktop-gamepad-shipping",
        ),
        #[cfg(all(
            windows,
            feature = "play-flow",
            feature = "gamepad",
            feature = "audio-cpal"
        ))]
        (
            RuntimeKind::AudioDesktop,
            ControllerBackend::Gilrs,
            GameFlow::Round,
            "two-level-round-audio-desktop-gamepad-shipping",
        ),
    ] {
        let selected = PlayerSelection::new(runtime, controllers)
            .unwrap()
            .with_flow(flow)
            .unwrap();
        assert!(profiles.insert(profile), "{profile}");
        assert_eq!(selected.content_profile(), profile);
        let restored = PlayerSelection::from_content_profile(profile).unwrap();
        assert_eq!(restored, selected);
        assert_eq!(restored.runtime(), runtime);
        assert_eq!(restored.controllers(), controllers);
        assert_eq!(restored.flow(), flow);
        assert_eq!(restored.manifest_profile(), selected.manifest_profile());
        assert_eq!(restored.binary(), selected.binary());
        assert_eq!(restored.features(), selected.features());
        #[cfg(any(feature = "development", feature = "cook"))]
        assert_eq!(
            plan(restored).plugin_plan().fingerprint(),
            plan(selected).plugin_plan().fingerprint()
        );
    }
    for profile in [
        "",
        "desktop",
        "two-level-headless-gamepad-shipping",
        "two-level-desktop-shipping-gamepad",
        "two-level-round-shipping",
        "two-level-round-headless-shipping",
        "two-level-round-desktop-shipping-gamepad",
        "two-level-desktop-round-shipping",
        "two-level-round-desktop-shipping-extra",
    ] {
        assert!(
            PlayerSelection::from_content_profile(profile).is_err(),
            "{profile}"
        );
    }
}

#[test]
fn player_targets_preserve_portable_keyboard_and_restrict_native_backends() {
    for runtime in [
        RuntimeKind::Headless,
        #[cfg(feature = "desktop")]
        RuntimeKind::Desktop,
    ] {
        for flow in [
            GameFlow::Exploration,
            #[cfg(feature = "play-flow")]
            GameFlow::Round,
        ] {
            if runtime == RuntimeKind::Headless && flow == GameFlow::Round {
                continue;
            }
            let selected = keyboard(runtime).with_flow(flow).unwrap();
            for triple in [
                "x86_64-pc-windows-msvc",
                "x86_64-unknown-linux-gnu",
                "aarch64-apple-darwin",
            ] {
                selected.validate_target(triple).unwrap();
            }
        }
    }
    #[cfg(all(
        windows,
        feature = "desktop",
        any(feature = "gamepad", feature = "audio-cpal")
    ))]
    for (runtime, controllers) in [
        #[cfg(feature = "gamepad")]
        (RuntimeKind::Desktop, ControllerBackend::Gilrs),
        #[cfg(feature = "audio-cpal")]
        (RuntimeKind::AudioDesktop, ControllerBackend::Disabled),
        #[cfg(all(feature = "gamepad", feature = "audio-cpal"))]
        (RuntimeKind::AudioDesktop, ControllerBackend::Gilrs),
    ] {
        for flow in [
            GameFlow::Exploration,
            #[cfg(feature = "play-flow")]
            GameFlow::Round,
        ] {
            let selected = PlayerSelection::new(runtime, controllers)
                .unwrap()
                .with_flow(flow)
                .unwrap();
            selected.validate_target("x86_64-pc-windows-msvc").unwrap();
            for triple in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"] {
                assert!(
                    selected.validate_target(triple).is_err(),
                    "{selected:?}: {triple}"
                );
            }
        }
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
    use nara_render::animation::{SpriteAnimationCodec, SpriteAnimationDomain};
    use two_level_game::{
        GamePhase, GameSnapshot, Scenario, SideRoomPhase,
        animation::{atlas_product, clips_product},
        command::{
            CommandSequence, CommandSource, CommandTick, ScheduledCommand, SemanticCommand,
            ServerCommand,
        },
        levels::{LevelId, LevelKey},
        markers::{MarkerLink, SCENE_PATH, marker_image_product, marker_product},
        region_material, region_tileset, side_room,
        sound_bank::bank_product,
    };

    #[cfg(feature = "play-flow")]
    use super::GameFlow;
    #[cfg(all(
        windows,
        feature = "desktop",
        any(feature = "gamepad", feature = "audio-cpal")
    ))]
    use super::{ControllerBackend, PlayerSelection};
    use super::{RuntimeKind, content, evidence::Evidence, keyboard, two_level_pack};

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
        let recipe = keyboard(RuntimeKind::Headless).recipe().unwrap();
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

    #[test]
    fn processed_continuation_refuses_the_wrong_scene_and_an_unrelated_recipe() {
        use nara::{
            ProductRecipe,
            app::FixedTime,
            asset::{AssetPathScope, ProjectAssetDatabase},
            project_workflow::cooked_content::{
                CookedContentBuilder, CookedContentLimits, CookedContentRequest,
            },
            runtime_content::RuntimeContentTarget,
            scene::runtime_configuration::RuntimeConfiguration,
            scene::{SceneDocument, SourceDocumentKey},
        };
        use two_level_game::{GameResumeError, capture_processed_resume, restore_processed_resume};

        let cache = Cache::new();
        let project = cache.project();
        let store = cache.path.join("store");
        fs::create_dir(&store).unwrap();
        let triple = "x86_64-pc-windows-msvc";
        let prepare = || {
            content::prepare(
                &host_fs::readable_directory(&project).unwrap(),
                &host_fs::writable_directory(&store).unwrap(),
                triple,
                keyboard(RuntimeKind::Server),
            )
            .unwrap()
            .into_parts()
            .0
            .into_base()
        };
        let baseline = prepare();
        let state = with_paused_content(baseline, |session| {
            capture_processed_resume(session).unwrap()
        });

        let manifest_path = project.join("nara.toml");
        let mut manifest = ProjectManifest::parse_toml_bytes(&fs::read(&manifest_path).unwrap())
            .manifest
            .unwrap();
        manifest.startup.default_scene = Some(LevelId::Cavern.scene_path().as_str().into());
        manifest.content.scenes = vec![LevelId::Meadow.scene_path().as_str().into()];
        fs::write(manifest_path, manifest.to_canonical_toml_bytes().unwrap()).unwrap();
        let swapped_startup = prepare();

        let recipe = ProductRecipe::default();
        let configuration = RuntimeConfiguration::default();
        let plan =
            super::resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
                .unwrap();
        let product = LevelId::Meadow.scene_product();
        let database = ProjectAssetDatabase::default();
        let resolver = database.resolver(&AssetPathScope::ROOT);
        let scene = plan
            .schema_validation()
            .bindings()
            .try_compile_scene_with_loader(
                &SceneDocument::from_ordered_roots([]),
                Default::default(),
                Some(resolver),
                |_, _| Ok::<_, std::convert::Infallible>(None),
            )
            .unwrap()
            .candidate
            .unwrap()
            .bind_source(SourceDocumentKey::stable_asset(
                product.source(),
                SourceDocumentKind::Scene,
            ));
        let mut unrelated = CookedContentBuilder::new(
            CookedContentRequest {
                target: RuntimeContentTarget::new(triple, "continuation-other-product").unwrap(),
                configuration,
                startup_scene: product,
                scenes: vec![product],
                addressable_roots: Vec::new(),
            },
            recipe,
            CookedContentLimits::default(),
        )
        .unwrap();
        unrelated.add_scene(product, &scene, resolver).unwrap();

        for (base, product, reason) in [
            (
                swapped_startup,
                LevelId::Cavern.scene_product(),
                "Scene Product differs",
            ),
            (
                unrelated.finish().unwrap().into_base(),
                product,
                "requires the dedicated Server recipe",
            ),
        ] {
            let catalog = base.catalog_digest();
            with_paused_content(base, |session| {
                let before = session
                    .capture(|world| {
                        let current = CurrentScene::get(world).unwrap();
                        assert_eq!(
                            current.source_view().hosted_runtime_scene_identity(),
                            Some((product, catalog))
                        );
                        assert_eq!(world.resource::<FixedTime>().tick(), 0);
                        current.receipt().instance_id()
                    })
                    .unwrap();
                let mut requested = state.clone();
                requested.catalog = catalog.into();
                for result in [
                    capture_processed_resume(session).map(|_| ()),
                    restore_processed_resume(
                        session,
                        &requested,
                        None,
                        Instant::now() + Duration::from_secs(10),
                    ),
                ] {
                    assert!(
                        matches!(result, Err(GameResumeError::Ineligible(refusal)) if refusal.contains(reason))
                    );
                }
                session
                    .capture(|world| {
                        assert_eq!(world.resource::<FixedTime>().tick(), 0);
                        assert_eq!(
                            CurrentScene::get(world).unwrap().receipt().instance_id(),
                            before
                        );
                    })
                    .unwrap();
            });
        }
    }

    #[test]
    fn processed_continuation_refuses_lookout_after_restoring_side_room() {
        use nara::{
            app::FixedTime,
            identity::{PrefabEntityTarget, SceneEntityId},
            prelude::Transform2d,
            scene::SceneDocumentCandidate,
        };
        use two_level_game::{
            GameResumeError, capture_processed_resume,
            command::{CommandSchedule, SERVER_COMMAND_SOURCE},
            restore_processed_resume,
            resume::processed::{PendingServerCommand, ProcessedRoomState, ProcessedRooms},
        };

        let cache = Cache::new();
        let project = cache.project();
        let lookout_path = project.join("scenes").join(side_room::LOOKOUT_SCENE_PATH);
        let authored_lookout = fs::read(&lookout_path).unwrap();
        let mut lookout = SceneDocumentCandidate::decode_json_bytes(&authored_lookout)
            .unwrap()
            .into_composition_source()
            .unwrap();
        let relic = SceneEntityId::new(side_room::LOOKOUT_RELIC).unwrap();
        let renamed = SceneEntityId::new("lookout-decoration").unwrap();
        lookout
            .entities
            .iter_mut()
            .find(|entity| entity.id == relic)
            .unwrap()
            .id = renamed.clone();
        let target = PrefabEntityTarget::source(relic);
        lookout
            .placements
            .iter_mut()
            .find(|placement| placement.target == target)
            .unwrap()
            .target = PrefabEntityTarget::source(renamed);
        fs::write(lookout_path, lookout.to_json_string().unwrap()).unwrap();

        let store = cache.path.join("store");
        fs::create_dir(&store).unwrap();
        let (prepared, _) = content::prepare(
            &host_fs::readable_directory(&project).unwrap(),
            &host_fs::writable_directory(&store).unwrap(),
            "x86_64-pc-windows-msvc",
            keyboard(RuntimeKind::Server),
        )
        .unwrap()
        .into_parts();
        let base = prepared.into_base();
        let [room_source, lookout_source] = [
            side_room::scene_product(),
            side_room::lookout_scene_product(),
        ]
        .map(|product| {
            let member = base.member(product).unwrap();
            let RuntimeContentMemberKind::Scene { source } = member.kind() else {
                panic!("the cooked region Product must be a Scene");
            };
            source
        });
        let room_relic = SceneEntityId::new(side_room::RELIC).unwrap();
        let live_side_room = |world: &World| {
            let domain = world.resource::<WorldIdentityDomain>();
            assert_eq!(domain.stats().active_scene_instances, 2);
            let members = world
                .iter_entities()
                .filter_map(|entity| {
                    let source = entity.get::<SceneEntitySource>()?;
                    assert_ne!(source.containing_source(), lookout_source);
                    (source.containing_source() == room_source).then_some((source, entity.id()))
                })
                .collect::<Vec<_>>();
            let (source, relic) = members
                .iter()
                .find(|(source, _)| source.entity_id == room_relic)
                .expect("the active Side Room has its authored relic");
            let room = domain
                .active_scene_instance(world, source.instance_id)
                .unwrap();
            let primary = CurrentScene::get(world).unwrap();
            assert_ne!(room.instance_id(), primary.receipt().instance_id());
            assert_eq!(
                domain
                    .active_scene_instance(world, primary.receipt().instance_id())
                    .unwrap(),
                *primary.receipt()
            );
            assert_eq!(room.len(), members.len());
            assert!(world.get::<Transform2d>(*relic).is_some());
            for (source, entity) in members {
                assert_eq!(source.instance_id, room.instance_id());
                assert_eq!(
                    room.resolve(world, &source.entity_id),
                    EntityLookup::Resolved(entity)
                );
            }
            room
        };
        let submit_side_room = |session: &mut HeadlessSession, tick| {
            ScheduledCommand::new(
                CommandTick::new(tick).unwrap(),
                CommandSource::external(SERVER_COMMAND_SOURCE).unwrap(),
                CommandSequence::new(tick).unwrap(),
                ServerCommand::SideRoom.into_intent().unwrap(),
            )
            .submit_headless(session)
            .unwrap();
            assert_eq!(session.frame(Duration::ZERO).unwrap(), None);
        };

        with_paused_content(base.clone(), |incumbent| {
            submit_side_room(incumbent, 1);
            let captured = capture_processed_resume(incumbent).unwrap();
            assert_eq!(
                captured.rooms,
                ProcessedRooms {
                    side_room: ProcessedRoomState::Closed,
                    lookout: ProcessedRoomState::Closed,
                }
            );
            assert_eq!(
                captured.pending,
                [PendingServerCommand {
                    tick_offset: 1,
                    command: ServerCommand::SideRoom,
                }]
            );
            let mut requested = captured.clone();
            requested.rooms = ProcessedRooms {
                side_room: ProcessedRoomState::RelicFound,
                lookout: ProcessedRoomState::Open,
            };
            let observe_incumbent = |world: &World| {
                (
                    world.resource::<FixedTime>().tick(),
                    CurrentScene::get(world).unwrap().receipt().clone(),
                    world.resource::<CommandSchedule>().stats(),
                )
            };
            let generation = incumbent.runtime_generation().unwrap();
            let before = incumbent.capture(observe_incumbent).unwrap();
            assert_eq!(before.0, 0);

            with_paused_content(base.clone(), |candidate| {
                let result = restore_processed_resume(
                    candidate,
                    &requested,
                    None,
                    Instant::now() + Duration::from_secs(10),
                );
                assert!(
                    matches!(&result, Err(GameResumeError::Ineligible(reason))
                        if reason == "the lookout requires its authored relic"),
                    "{result:?}"
                );
                let observe_refusal = |world: &World| {
                    let snapshot = world.resource::<GameSnapshot>();
                    assert_eq!(snapshot.phase, GamePhase::Degraded);
                    assert_eq!(snapshot.side_room, SideRoomPhase::RelicFound);
                    assert_eq!(snapshot.lookout, SideRoomPhase::Closed);
                    assert!(!snapshot.restoring);
                    assert_eq!(world.resource::<FixedTime>().tick(), 1);
                    let queue = world.resource::<CommandSchedule>();
                    assert!(queue.is_idle());
                    let stats = queue.stats();
                    assert_eq!(stats.accepted, 1);
                    assert_eq!(stats.acknowledged_through_tick, 1);
                    live_side_room(world)
                };
                let room = candidate.capture(observe_refusal).unwrap();
                assert_ne!(room.domain_id(), before.1.domain_id());
                assert_eq!(candidate.frame(Duration::ZERO).unwrap(), None);
                assert_eq!(candidate.capture(observe_refusal).unwrap(), room);
            });

            assert_eq!(capture_processed_resume(incumbent).unwrap(), captured);
            assert_eq!(incumbent.runtime_generation(), Some(generation));
            assert_eq!(incumbent.capture(observe_incumbent).unwrap(), before);

            let release = hold_io_workers(incumbent);
            assert_eq!(incumbent.step().unwrap(), None);
            assert_eq!(
                incumbent
                    .capture(|world| world.resource::<GameSnapshot>().side_room)
                    .unwrap(),
                SideRoomPhase::Preparing
            );
            let result = capture_processed_resume(incumbent);
            assert!(
                matches!(&result, Err(GameResumeError::Ineligible(reason))
                    if reason == "the side room is still preparing"),
                "{result:?}"
            );
            drop(release);
            wait_for_snapshot(incumbent, "incumbent Side Room did not open", |snapshot| {
                snapshot.side_room == SideRoomPhase::Open
            })
            .unwrap();
            let room = incumbent.capture(live_side_room).unwrap();
            submit_side_room(incumbent, 2);
            assert_eq!(incumbent.step().unwrap(), None);
            incumbent
                .capture(|world| {
                    let snapshot = world.resource::<GameSnapshot>();
                    assert_eq!(snapshot.tick, 2);
                    assert_eq!(snapshot.phase, GamePhase::Running);
                    assert_eq!(snapshot.side_room, SideRoomPhase::Closed);
                    assert!(snapshot.error.is_none());
                    assert_eq!(CurrentScene::get(world).unwrap().receipt(), &before.1);
                    assert_retired(world, &room);
                })
                .unwrap();
        });
    }

    fn hold_io_workers(session: &HeadlessSession) -> Vec<std::sync::mpsc::Sender<()>> {
        use std::sync::mpsc;

        use nara::tasks::{TaskDomainKey, TaskPoolKind, TaskPools, TaskSpawnRequest};

        let (entered, started) = mpsc::channel();
        let releases: Vec<_> = session
            .observe(|world| {
                let pools = world.resource::<TaskPools>();
                let workers = pools.config().kind(TaskPoolKind::Io).workers().get();
                let request = TaskSpawnRequest::new(0, TaskDomainKey::new(0));
                (0..workers)
                    .map(|_| {
                        let (release, held) = mpsc::channel();
                        let entered = entered.clone();
                        pools
                            .spawn(TaskPoolKind::Io, request, move |_| {
                                let _ = entered.send(());
                                let _ = held.recv();
                            })
                            .into_handle()
                            .unwrap();
                        release
                    })
                    .collect()
            })
            .unwrap();
        drop(entered);
        for _ in &releases {
            started.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        releases
    }

    fn with_paused_content<R>(
        base: nara::runtime_content::RuntimeContentBase,
        action: impl FnOnce(&mut HeadlessSession) -> R,
    ) -> R {
        let mut session = HeadlessSession::from_runtime_content(base);
        session.start().unwrap();
        assert_eq!(session.frame(Duration::ZERO).unwrap(), None);
        assert_eq!(session.pause().unwrap(), None);
        let result = action(&mut session);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match session.close() {
                HeadlessSessionClose::Pending if Instant::now() < deadline => {
                    std::thread::yield_now()
                }
                outcome => {
                    assert_eq!(outcome, HeadlessSessionClose::Complete);
                    return result;
                }
            }
        }
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
            keyboard(RuntimeKind::Headless),
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
            let player = keyboard(runtime);
            let cache = Cache::new();
            let project = cache.project();
            let store = cache.path.join("store");
            fs::create_dir(&store).unwrap();
            let root = host_fs::readable_directory(&project).unwrap();
            let (prepared, selection) = content::prepare(
                &root,
                &host_fs::writable_directory(&store).unwrap(),
                "x86_64-pc-windows-msvc",
                player,
            )
            .unwrap_or_else(|error| panic!("{runtime:?}: {error}"))
            .into_parts();
            assert!(prepared.base().plan().settings().window().enabled);
            assert_eq!(prepared.base().catalog().members().len(), 15);
            let manifest = root
                .open_file(&RelativePath::new("nara.toml").unwrap())
                .unwrap();
            let candidate = ingest_project_manifest(&manifest, player.manifest_profile()).unwrap();
            let plan = resolve_product_recipe(&candidate, player.recipe().unwrap()).unwrap();
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
            #[cfg(any(feature = "play-flow", all(windows, feature = "gamepad")))]
            for (index, variant) in [
                #[cfg(all(windows, feature = "gamepad"))]
                PlayerSelection::new(runtime, ControllerBackend::Gilrs).unwrap(),
                #[cfg(feature = "play-flow")]
                player.with_flow(GameFlow::Round).unwrap(),
                #[cfg(all(windows, feature = "gamepad", feature = "play-flow"))]
                PlayerSelection::new(runtime, ControllerBackend::Gilrs)
                    .unwrap()
                    .with_flow(GameFlow::Round)
                    .unwrap(),
            ]
            .into_iter()
            .enumerate()
            {
                use nara::runtime_content::{
                    RuntimeContentBase, RuntimeContentExpectation, RuntimeContentLimits,
                    RuntimeContentTarget,
                };

                let (variant_content, variant_selection) = content::prepare(
                    &root,
                    &host_fs::writable_directory(&store).unwrap(),
                    "x86_64-pc-windows-msvc",
                    variant,
                )
                .unwrap_or_else(|error| panic!("{variant:?}: {error}"))
                .into_parts();
                variant_selection.verify(&project).unwrap();
                let candidate =
                    ingest_project_manifest(&manifest, variant.manifest_profile()).unwrap();
                let source_plan =
                    resolve_product_recipe(&candidate, variant.recipe().unwrap()).unwrap();
                assert_eq!(
                    variant_content.base().plan().settings().window(),
                    source_plan.settings().window()
                );
                assert_eq!(
                    variant_content.base().plan().settings().ui(),
                    source_plan.settings().ui()
                );
                assert_eq!(
                    variant_content
                        .base()
                        .plan()
                        .schema_validation()
                        .composition_fingerprint(),
                    source_plan.schema_validation().composition_fingerprint()
                );
                let original_members = selection.delivery_receipt().members();
                let variant_members = variant_selection.delivery_receipt().members();
                assert_eq!(variant_members.len(), original_members.len());
                for (member, original) in variant_members.iter().zip(original_members) {
                    assert_eq!(member.path(), original.path());
                    assert_eq!(member.digest(), original.digest());
                    assert_eq!(member.kind(), original.kind());
                    let expected = member.digest();
                    assert!(variant_selection.observations().iter().any(|observation| {
                        matches!(observation,
                                ArtifactBuildObservation::Raw { path, digest: Some(digest) }
                                    if path.as_str() == member.path().as_str()
                                        && digest.content() == expected)
                    }));
                }
                let baseline = prepared.base().catalog();
                let catalog = variant_content.base().catalog();
                assert_eq!(catalog.target().triple(), baseline.target().triple());
                assert_eq!(catalog.target().profile(), variant.content_profile());
                assert_ne!(catalog.target().profile(), baseline.target().profile());
                if variant.flow() == super::GameFlow::Exploration {
                    assert_eq!(catalog.configuration(), baseline.configuration());
                    assert_eq!(catalog.schema_fingerprint(), baseline.schema_fingerprint());
                } else {
                    assert_ne!(catalog.configuration(), baseline.configuration());
                }
                assert_eq!(catalog.members().len(), baseline.members().len());
                for (member, original) in catalog.members().iter().zip(baseline.members()) {
                    assert_eq!(member.path(), original.path());
                    assert_eq!(member.digest(), original.digest());
                    assert_eq!(member.kind(), original.kind());
                    assert_eq!(member.products(), original.products());
                    assert_eq!(member.dependencies(), original.dependencies());
                }
                assert_eq!(catalog.startup_scene(), baseline.startup_scene());
                assert_eq!(catalog.scenes(), baseline.scenes());
                assert_eq!(catalog.addressable_roots(), baseline.addressable_roots());
                assert_eq!(
                    catalog.runtime_codecs_fingerprint(),
                    baseline.runtime_codecs_fingerprint()
                );
                assert_ne!(
                    catalog.plugin_plan_fingerprint(),
                    baseline.plugin_plan_fingerprint()
                );
                let expected_plan = super::plan(variant);
                assert_eq!(
                    variant_content.base().plan().settings(),
                    expected_plan.settings()
                );
                assert_eq!(
                    variant_content.base().plan().plugin_plan().fingerprint(),
                    expected_plan.plugin_plan().fingerprint()
                );
                let authored = ActionMapFileCandidate::decode_json_bytes(
                    &fs::read(project.join(super::INPUT_PATH)).unwrap(),
                )
                .unwrap();
                let input = variant_content
                    .base()
                    .acquire_startup_input()
                    .unwrap()
                    .unwrap();
                assert_eq!(input.product(), authored.product());
                assert_eq!(
                    input.definition().contexts(),
                    authored.definition().contexts()
                );
                assert_eq!(
                    input.definition().bindings(),
                    authored.definition().bindings()
                );
                let variant_bootstrap = generate_bootstrap(&project, &variant_content);
                assert_ne!(variant_bootstrap.digest(), _bootstrap.digest());
                assert!(
                    std::str::from_utf8(variant_bootstrap.bytes())
                        .unwrap()
                        .contains(variant.content_profile())
                );

                for (cooked, expected, incompatible, name) in [
                    (&prepared, player, variant, "original"),
                    (&variant_content, variant, player, "selected"),
                ] {
                    let path = cache.path.join(format!("{index}-{name}.pack"));
                    cooked
                        .base()
                        .write_pack(&mut fs::File::create_new(&path).unwrap())
                        .unwrap();
                    let reopened = RuntimeContentBase::open_pack(
                        host_fs::readable_file(&path).unwrap(),
                        cooked.expectation(),
                        expected.recipe().unwrap(),
                        RuntimeContentLimits::default(),
                    )
                    .unwrap();
                    assert_eq!(
                        reopened.catalog().target().profile(),
                        expected.content_profile()
                    );
                    assert_eq!(
                        reopened.acquire_startup_input().unwrap().unwrap().product(),
                        authored.product()
                    );
                    let rejected = RuntimeContentBase::open_pack(
                        host_fs::readable_file(&path).unwrap(),
                        cooked.expectation(),
                        incompatible.recipe().unwrap(),
                        RuntimeContentLimits::default(),
                    );
                    if expected.flow() == super::GameFlow::Exploration
                        && incompatible.flow() == super::GameFlow::Round
                    {
                        assert!(matches!(rejected, Err(RuntimeContentError::Composition(_))));
                    } else {
                        assert!(matches!(rejected, Err(RuntimeContentError::RecipeMismatch)));
                    }
                    let wrong_profile = RuntimeContentExpectation::new(
                        cooked.expectation().catalog_digest(),
                        RuntimeContentTarget::new(
                            "x86_64-pc-windows-msvc",
                            incompatible.content_profile(),
                        )
                        .unwrap(),
                    );
                    assert!(matches!(
                        RuntimeContentBase::open_pack(
                            host_fs::readable_file(&path).unwrap(),
                            &wrong_profile,
                            expected.recipe().unwrap(),
                            RuntimeContentLimits::default(),
                        ),
                        Err(RuntimeContentError::TargetMismatch)
                    ));
                }
                let manifest_path = project.join("nara.toml");
                let bytes = fs::read(&manifest_path).unwrap();
                let mut changed = bytes.clone();
                changed.push(b'\n');
                fs::write(&manifest_path, changed).unwrap();
                assert!(selection.verify(&project).is_err());
                assert!(variant_selection.verify(&project).is_err());
                fs::write(&manifest_path, bytes).unwrap();
                selection.verify(&project).unwrap();
                variant_selection.verify(&project).unwrap();
            }
        }
    }

    #[cfg(all(
        windows,
        feature = "desktop",
        any(feature = "gamepad", feature = "audio-cpal")
    ))]
    #[test]
    fn unsupported_native_targets_refuse_before_reading_content_or_opening_the_store() {
        let cache = Cache::new();
        let project = cache.path.join("project");
        let store = cache.path.join("store");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&store).unwrap();
        let root = host_fs::readable_directory(&project).unwrap();
        let directory = host_fs::writable_directory(&store).unwrap();
        for (runtime, controllers) in [
            #[cfg(feature = "gamepad")]
            (RuntimeKind::Desktop, ControllerBackend::Gilrs),
            #[cfg(feature = "audio-cpal")]
            (RuntimeKind::AudioDesktop, ControllerBackend::Disabled),
            #[cfg(all(feature = "gamepad", feature = "audio-cpal"))]
            (RuntimeKind::AudioDesktop, ControllerBackend::Gilrs),
        ] {
            let selection = PlayerSelection::new(runtime, controllers).unwrap();
            for triple in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"] {
                let Err(error) = content::prepare_for_player(&root, &directory, triple, selection)
                else {
                    panic!("{selection:?} for {triple} unexpectedly prepared native content");
                };
                let ProjectCookError::Invalid(message) = error else {
                    panic!("{selection:?} for {triple} did not reject its target: {error}");
                };
                assert!(message.contains("Windows"), "{message}");
                assert!(!project.join("nara.toml").exists());
                assert_eq!(fs::read_dir(&store).unwrap().count(), 0);
            }
        }
    }

    #[cfg(all(windows, feature = "play-flow", feature = "gamepad"))]
    #[test]
    fn controller_round_cook_rejects_the_same_start_conflict_as_runtime_startup() {
        let cache = Cache::new();
        let project = cache.project();
        let store = cache.path.join("store");
        fs::create_dir(&store).unwrap();
        let root = host_fs::readable_directory(&project).unwrap();
        let directory = host_fs::writable_directory(&store).unwrap();
        let input_path = project.join(super::INPUT_PATH);
        let original = fs::read(&input_path).unwrap();
        let mut input: serde_json::Value = serde_json::from_slice(&original).unwrap();
        let start = input["payload"]["bindings"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|binding| {
                binding["input"]["GamepadButton"] == nara::input::GamepadButton::Start.code()
            })
            .unwrap();
        start["action"] = serde_json::json!("two-level-game.q");
        let edited = serde_json::to_vec(&input).unwrap();
        fs::write(&input_path, &edited).unwrap();

        let exploration =
            PlayerSelection::new(RuntimeKind::Desktop, ControllerBackend::Gilrs).unwrap();
        let round = exploration.with_flow(GameFlow::Round).unwrap();
        let triple = "x86_64-pc-windows-msvc";
        let Err(error) = content::prepare_for_player(&root, &directory, triple, round) else {
            panic!("a controller round cooked an incompatible Start binding");
        };
        assert!(
            matches!(error, ProjectCookError::Invalid(message) if message.contains("controller round reserves Start"))
        );
        assert!(!project.join(GENERATED).exists());
        let (_, accepted) = content::prepare_for_player(&root, &directory, triple, exploration)
            .unwrap()
            .into_parts();
        accepted.verify(&project).unwrap();
        assert_eq!(fs::read(&input_path).unwrap(), edited);
        drop(accepted);

        fs::write(&input_path, &original).unwrap();
        let (_, accepted) = content::prepare_for_player(&root, &directory, triple, round)
            .unwrap()
            .into_parts();
        accepted.verify(&project).unwrap();
        assert_eq!(fs::read(&input_path).unwrap(), original);
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
                keyboard(RuntimeKind::Headless),
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
                keyboard(RuntimeKind::Headless),
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
                keyboard(RuntimeKind::Headless),
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
                keyboard(RuntimeKind::Headless),
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
                keyboard(RuntimeKind::Headless),
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
                keyboard(RuntimeKind::Headless),
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
                keyboard(RuntimeKind::Headless),
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
                keyboard(RuntimeKind::Headless),
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
            if binding.action.as_str() == "two-level-game.move-w"
                && matches!(binding.input, InputBinding::Key(_))
            {
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
            keyboard(RuntimeKind::Headless),
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
            keyboard(RuntimeKind::Headless),
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
                keyboard(RuntimeKind::Headless),
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
            step_command(
                &mut session,
                1,
                ServerCommand::Gameplay(SemanticCommand::Marker),
            )?;
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

    fn step_command(
        session: &mut HeadlessSession,
        tick: u64,
        command: ServerCommand,
    ) -> Result<(), String> {
        ScheduledCommand::new(
            CommandTick::new(tick).unwrap(),
            CommandSource::external("test.cooked-content").unwrap(),
            CommandSequence::new(tick).unwrap(),
            command.into_intent()?,
        )
        .submit_headless(session)
        .map_err(|error| error.to_string())?;
        session.step().map_err(|error| error.to_string())?;
        Ok(())
    }

    fn check_cooked_regions(session: &mut HeadlessSession) -> Result<(), String> {
        const SIDE_ROOM: ServerCommand = ServerCommand::SideRoom;
        const LOOKOUT: ServerCommand = ServerCommand::Lookout;
        const TRAVEL: ServerCommand = ServerCommand::Gameplay(SemanticCommand::Travel);
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
