use nara::{
    core::ByteLimit,
    gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
    image::{IMAGE_PLUGIN_ID, ImageImportLimits},
    project::ProductCapability,
    reflect::ComponentRegistry,
    render::RENDER_SCHEMA_PROVIDER_ID,
    sprite::SPRITE_SCHEMA_PROVIDER_ID,
    tilemap::{TILEMAP_PLUGIN_ID, TILEMAP_SCHEMA_PROVIDER_ID},
};
use nara_reference_game::{REFERENCE_GAME_PLUGIN_ID, REFERENCE_GAME_SCHEMA_PROVIDER_ID};

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use project_content_fixture::{candidate_plan_and_root, resolve_reference_plan};

#[cfg(feature = "desktop")]
use nara::{
    project::ProductCapabilitySet, project_host::ProjectContentLoader, window::WINDOW_PLUGIN_ID,
};
#[cfg(feature = "desktop")]
use nara_reference_game::REFERENCE_DESKTOP_PLUGIN_ID;
#[cfg(feature = "desktop")]
use project_content_fixture::{
    desktop_candidate_plan_and_root, headless_wave_candidate_plan_and_root,
};

#[test]
fn reference_game_configures_a_repeatable_headless_product_plan() {
    let limits = ImageImportLimits::default().with_max_encoded_bytes(
        ByteLimit::new(8 * 1024 * 1024).expect("test image limit is non-zero"),
    );
    let (candidate, _baseline, _root) = candidate_plan_and_root(limits, false);

    let first = resolve_reference_plan(&candidate, limits, false);
    let repeated = resolve_reference_plan(&candidate, limits, false);
    let with_tilemap = resolve_reference_plan(&candidate, limits, true);

    assert_eq!(first.lineage(), candidate.lineage());
    assert_eq!(
        first.plugin_plan().fingerprint(),
        repeated.plugin_plan().fingerprint()
    );
    assert_eq!(
        first.schema_validation().fingerprint(),
        repeated.schema_validation().fingerprint()
    );
    assert!(
        first
            .required_capabilities()
            .contains(ProductCapability::Runtime2d)
    );

    let entries = first.plugin_plan().entries();
    assert!(
        !entries
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
    assert_eq!(
        configured_image.definition_key(),
        nara::image::plugin(limits).key()
    );

    let providers = first.schema_validation().provider_ids();
    assert!(providers.contains(&SPRITE_SCHEMA_PROVIDER_ID));
    assert!(providers.contains(&RENDER_SCHEMA_PROVIDER_ID));
    assert!(providers.contains(&REFERENCE_GAME_SCHEMA_PROVIDER_ID));
    assert!(!providers.contains(&TILEMAP_SCHEMA_PROVIDER_ID));
    assert!(
        with_tilemap
            .schema_validation()
            .provider_ids()
            .contains(&TILEMAP_SCHEMA_PROVIDER_ID)
    );
    assert_ne!(
        first.plugin_plan().fingerprint(),
        with_tilemap.plugin_plan().fingerprint()
    );
    assert_ne!(
        first.schema_validation().fingerprint(),
        with_tilemap.schema_validation().fingerprint()
    );

    let first_app = first.plugin_plan().instantiate().unwrap();
    let second_app = first.plugin_plan().instantiate().unwrap();
    let first_registry = first_app
        .world()
        .resource::<ComponentRegistry>()
        .snapshot()
        .unwrap();
    let second_registry = second_app
        .world()
        .resource::<ComponentRegistry>()
        .snapshot()
        .unwrap();
    assert!(first_registry.ptr_eq(&second_registry));
    assert!(first_registry.ptr_eq(first.schema_validation().snapshot()));
    assert_eq!(
        first_registry.catalog().fingerprint(),
        first.schema_validation().fingerprint()
    );
    assert_eq!(
        second_registry.catalog().fingerprint(),
        first.schema_validation().fingerprint()
    );
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
            ProductCapability::DesktopWinit,
            ProductCapability::RenderWgpu,
        ]
        .into_iter()
        .all(|capability| first.required_capabilities().contains(capability))
    );
    assert_eq!(
        candidate.normalized_capabilities(),
        ProductCapabilitySet::from_capabilities([
            ProductCapability::RuntimeCore,
            ProductCapability::Runtime2d,
            ProductCapability::RuntimeUi,
            ProductCapability::DesktopWinit,
            ProductCapability::RenderWgpu,
        ])
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
