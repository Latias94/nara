use nara::{
    app::PluginGroupId,
    gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
    image::IMAGE_PLUGIN_ID,
    project::ProductCapability,
    project_host::resolve_product_recipe,
    sprite::SPRITE_SCHEMA_PROVIDER_ID,
    tilemap::{TILEMAP_PLUGIN_ID, TILEMAP_SCHEMA_PROVIDER_ID},
};
use nara_reference_game::{
    REFERENCE_GAME_PLUGIN_ID, REFERENCE_GAME_SCHEMA_PROVIDER_ID,
    REFERENCE_PROJECT_OUTCOME_PLUGIN_ID, REFERENCE_WAVE_PLUGIN_ID, project_recipe, wave_recipe,
};

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use project_content_fixture::{candidate_plan_and_root, resolve_reference_plan};

const CHECKED_IN_AUTHORING_BUNDLE: &[u8] = include_bytes!("../schema/authoring.bundle.json");

#[cfg(feature = "desktop")]
use nara::{
    image::IMAGE_PREPARE_PLUGIN_ID, project_host::ProjectContentLoader, render::RENDER_PLUGIN_ID,
    render_wgpu::WGPU_RENDER_PLUGIN_ID, sprite_render::SPRITE_RENDER_PLUGIN_ID,
    ui_render::UI_RENDER_PLUGIN_ID, window::WINDOW_PLUGIN_ID,
};
#[cfg(feature = "desktop")]
use nara_reference_game::REFERENCE_DESKTOP_PLUGIN_ID;
#[cfg(feature = "desktop")]
use project_content_fixture::{
    desktop_candidate_plan_and_root, headless_wave_candidate_plan_and_root,
};

#[test]
fn checked_in_authoring_bundle_matches_the_normal_reference_product_plan() {
    let (_candidate, plan, _root) = candidate_plan_and_root();
    let expected = plan.schema_validation().authoring_schema_bundle();
    let mut expected_bytes = expected.to_json_string().unwrap().into_bytes();
    expected_bytes.push(b'\n');

    assert_eq!(CHECKED_IN_AUTHORING_BUNDLE, expected_bytes.as_slice());
}

#[test]
fn ordinary_recipes_bind_reference_schema_once_without_parallel_provider_lists() {
    let (candidate, _baseline, _root) = candidate_plan_and_root();

    let wave = resolve_product_recipe(&candidate, wave_recipe().unwrap()).unwrap();
    let project = resolve_product_recipe(&candidate, project_recipe().unwrap()).unwrap();

    for (plan, expected_plugin) in [
        (&wave, REFERENCE_WAVE_PLUGIN_ID),
        (&project, REFERENCE_PROJECT_OUTCOME_PLUGIN_ID),
    ] {
        assert!(
            plan.plugin_plan()
                .entries()
                .iter()
                .any(|entry| entry.plugin_id() == REFERENCE_GAME_PLUGIN_ID)
        );
        assert!(
            plan.plugin_plan()
                .entries()
                .iter()
                .any(|entry| entry.plugin_id() == expected_plugin)
        );
        assert!(
            plan.schema_validation()
                .provider_ids()
                .contains(&REFERENCE_GAME_SCHEMA_PROVIDER_ID)
        );
    }
}

#[test]
fn reference_game_configures_a_repeatable_headless_product_plan() {
    let (candidate, _baseline, _root) = candidate_plan_and_root();

    let first = resolve_reference_plan(&candidate);
    let repeated = resolve_reference_plan(&candidate);

    assert_eq!(first.lineage(), candidate.lineage());
    assert_eq!(
        first.plugin_plan().fingerprint(),
        repeated.plugin_plan().fingerprint()
    );
    assert_eq!(
        first.schema_validation().composition_fingerprint(),
        repeated.schema_validation().composition_fingerprint()
    );
    assert!(
        first
            .required_capabilities()
            .contains(ProductCapability::Runtime2d)
    );
    assert!(
        !first
            .required_capabilities()
            .contains(ProductCapability::PresentationUi)
    );

    let entries = first.plugin_plan().entries();
    for absent in [
        "nara.render",
        "nara.image.prepare",
        "nara.sprite-render",
        "nara.ui-render",
        "nara.render-wgpu",
        "nara.window",
        "nara.tooling",
    ] {
        assert!(
            entries
                .iter()
                .all(|entry| entry.plugin_id().as_str() != absent),
            "headless plan unexpectedly installed {absent}"
        );
    }
    for absent in [
        "nara.plugins.presentation-2d",
        "nara.plugins.desktop-winit",
        "nara.plugins.render-wgpu",
    ] {
        assert!(
            first
                .plugin_plan()
                .groups()
                .iter()
                .all(|group| group.id() != PluginGroupId::new(absent)),
            "headless plan unexpectedly installed group {absent}"
        );
    }
    assert!(
        entries
            .iter()
            .any(|entry| entry.plugin_id() == TILEMAP_PLUGIN_ID)
    );
    let command_index = entries
        .iter()
        .position(|entry| entry.plugin_id() == GAMEPLAY_COMMAND_PLUGIN_ID)
        .unwrap();
    let game_index = entries
        .iter()
        .position(|entry| entry.plugin_id() == REFERENCE_GAME_PLUGIN_ID)
        .unwrap();
    assert!(command_index < game_index);

    let configured_image = entries
        .iter()
        .find(|entry| entry.plugin_id() == IMAGE_PLUGIN_ID)
        .unwrap();
    let image_definition = configured_image
        .definition_key()
        .expect("profile-owned image plugin has a repeatable definition")
        .definition();
    assert_eq!(image_definition.as_str(), IMAGE_PLUGIN_ID.as_str());
    assert_eq!(image_definition.version(), 1);

    let providers = first.schema_validation().provider_ids();
    assert!(providers.contains(&SPRITE_SCHEMA_PROVIDER_ID));
    assert!(providers.contains(&REFERENCE_GAME_SCHEMA_PROVIDER_ID));
    assert!(providers.contains(&TILEMAP_SCHEMA_PROVIDER_ID));

    let first_app = first.plugin_plan().instantiate().unwrap();
    let second_app = first.plugin_plan().instantiate().unwrap();
    let first_registry = nara::reflect::component_registry(first_app.world())
        .unwrap()
        .snapshot()
        .unwrap();
    let second_registry = nara::reflect::component_registry(second_app.world())
        .unwrap()
        .snapshot()
        .unwrap();
    assert!(first_registry.ptr_eq(&second_registry));
    assert!(first_registry.ptr_eq(first.schema_validation().snapshot()));
    assert_eq!(
        first_registry.schema_composition_fingerprint().unwrap(),
        first.schema_validation().composition_fingerprint()
    );
    assert_eq!(
        second_registry.schema_composition_fingerprint().unwrap(),
        first.schema_validation().composition_fingerprint()
    );
}

#[test]
fn profile_owned_image_and_tilemap_are_stable_across_recipe_resolution() {
    let (candidate, _baseline, _root) = candidate_plan_and_root();
    let first = resolve_reference_plan(&candidate);
    let repeated = resolve_reference_plan(&candidate);

    for plugin in [IMAGE_PLUGIN_ID, TILEMAP_PLUGIN_ID] {
        let first_entry = first
            .plugin_plan()
            .entries()
            .iter()
            .find(|entry| entry.plugin_id() == plugin)
            .unwrap();
        let repeated_entry = repeated
            .plugin_plan()
            .entries()
            .iter()
            .find(|entry| entry.plugin_id() == plugin)
            .unwrap();
        assert_eq!(
            first_entry.definition_key(),
            repeated_entry.definition_key()
        );
    }
}

#[cfg(feature = "desktop")]
#[test]
fn desktop_profile_configures_one_window_slot_and_preserves_source_content() {
    let (candidate, first, root) = desktop_candidate_plan_and_root();
    let (repeated_candidate, repeated, repeated_root) = desktop_candidate_plan_and_root();

    assert_eq!(
        first.plugin_plan().fingerprint(),
        repeated.plugin_plan().fingerprint()
    );
    assert!(
        [
            ProductCapability::Runtime2d,
            ProductCapability::RuntimeUi,
            ProductCapability::PresentationUi,
            ProductCapability::Presentation2d,
            ProductCapability::DesktopWinit,
            ProductCapability::RenderWgpu,
        ]
        .into_iter()
        .all(|capability| first.required_capabilities().contains(capability))
    );
    let window_entries = first
        .plugin_plan()
        .entries()
        .iter()
        .filter(|entry| entry.plugin_id() == WINDOW_PLUGIN_ID)
        .collect::<Vec<_>>();
    assert_eq!(window_entries.len(), 1);
    assert_eq!(
        window_entries[0].definition_key(),
        nara::window::plugin(candidate.settings().window.to_window()).key()
    );
    for plugin in [
        RENDER_PLUGIN_ID,
        IMAGE_PREPARE_PLUGIN_ID,
        SPRITE_RENDER_PLUGIN_ID,
        UI_RENDER_PLUGIN_ID,
        WGPU_RENDER_PLUGIN_ID,
    ] {
        assert_eq!(
            first
                .plugin_plan()
                .entries()
                .iter()
                .filter(|entry| entry.plugin_id() == plugin)
                .count(),
            1,
            "desktop plan must install {plugin} exactly once"
        );
    }
    for group in [
        "nara.plugins.runtime-2d",
        "nara.plugins.presentation-2d",
        "nara.plugins.runtime-ui",
        "nara.plugins.presentation-ui",
        "nara.plugins.desktop-winit",
        "nara.plugins.render-wgpu",
    ] {
        assert_eq!(
            first
                .plugin_plan()
                .groups()
                .iter()
                .filter(|resolved| resolved.id() == PluginGroupId::new(group))
                .count(),
            1,
            "desktop plan must install group {group} exactly once"
        );
    }
    assert!(
        first
            .plugin_plan()
            .entries()
            .iter()
            .any(|entry| entry.plugin_id() == REFERENCE_DESKTOP_PLUGIN_ID)
    );

    let desktop_loader = ProjectContentLoader::new(root).unwrap();
    let desktop_snapshot = desktop_loader.load(&candidate, &first).unwrap();
    let repeated_loader = ProjectContentLoader::new(repeated_root).unwrap();
    let repeated_snapshot = repeated_loader
        .load(&repeated_candidate, &repeated)
        .unwrap();
    assert_eq!(
        desktop_snapshot.expanded_startup_scene(),
        repeated_snapshot.expanded_startup_scene()
    );
    assert_eq!(
        desktop_snapshot.images()[0].image().source().source_hash(),
        repeated_snapshot.images()[0].image().source().source_hash()
    );

    let (headless_candidate, headless_plan, headless_root) =
        headless_wave_candidate_plan_and_root();
    let headless_loader = ProjectContentLoader::new(headless_root).unwrap();
    let headless_snapshot = headless_loader
        .load(&headless_candidate, &headless_plan)
        .unwrap();
    assert_ne!(
        headless_snapshot.lineage(),
        desktop_snapshot.lineage(),
        "profile-specific plans retain their own settings lineage",
    );
    assert_eq!(
        headless_snapshot.content_digest(),
        desktop_snapshot.content_digest(),
        "the profiles authorize the same committed content closure",
    );
    assert_eq!(
        headless_snapshot.images()[0].image().source().source_hash(),
        desktop_snapshot.images()[0].image().source().source_hash(),
    );
    assert_eq!(headless_loader.budget_snapshot().active_reservations(), 1);
    assert_eq!(desktop_loader.budget_snapshot().active_reservations(), 1);

    drop(headless_snapshot);
    drop(desktop_snapshot);
    drop(repeated_snapshot);
    assert_eq!(headless_loader.budget_snapshot().active_reservations(), 0);
    assert_eq!(desktop_loader.budget_snapshot().active_reservations(), 0);
    assert_eq!(repeated_loader.budget_snapshot().active_reservations(), 0);
}
