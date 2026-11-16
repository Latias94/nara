#![forbid(unsafe_code)]

#[allow(dead_code)]
#[path = "../src/bin/cook/input.rs"]
mod input;

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
