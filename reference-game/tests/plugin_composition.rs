use std::{fs::File, path::PathBuf};

use nara::{
    core::ByteLimit,
    fs::{FileCapability, TrustMode},
    gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
    image::{IMAGE_PLUGIN_ID, ImageImportLimits},
    project::ProductCapability,
    project_host::{built_in_schema_providers, ingest_project_manifest, resolve_runtime_plan},
    reflect::ComponentRegistry,
    render::RENDER_SCHEMA_PROVIDER_ID,
    sprite::SPRITE_SCHEMA_PROVIDER_ID,
    tilemap::{TILEMAP_PLUGIN_ID, TILEMAP_SCHEMA_PROVIDER_ID, TilemapPlugin},
};
use nara_reference_game::{
    REFERENCE_GAME_PLUGIN_ID, REFERENCE_GAME_SCHEMA_PROVIDER, REFERENCE_GAME_SCHEMA_PROVIDER_ID,
    runtime_plugins,
};

#[test]
fn reference_game_configures_a_repeatable_headless_product_plan() {
    let candidate = project_candidate();
    let limits = ImageImportLimits::default().with_max_encoded_bytes(
        ByteLimit::new(8 * 1024 * 1024).expect("test image limit is non-zero"),
    );

    let first = resolve_reference_plan(&candidate, limits);
    let repeated = resolve_reference_plan(&candidate, limits);
    let with_tilemap = resolve_reference_plan_with_tilemap(&candidate, limits);

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
    assert!(!first_registry.ptr_eq(&second_registry));
    assert_eq!(
        first_registry.catalog().fingerprint(),
        first.schema_validation().fingerprint()
    );
    assert_eq!(
        second_registry.catalog().fingerprint(),
        first.schema_validation().fingerprint()
    );
}

fn resolve_reference_plan(
    candidate: &nara::project_host::ProjectSettingsCandidate,
    limits: ImageImportLimits,
) -> nara::project_host::RuntimePlan {
    let request = runtime_plugins(candidate)
        .disable::<TilemapPlugin>()
        .configure(nara::image::plugin(limits));
    resolve_reference_request(candidate, request)
}

fn resolve_reference_plan_with_tilemap(
    candidate: &nara::project_host::ProjectSettingsCandidate,
    limits: ImageImportLimits,
) -> nara::project_host::RuntimePlan {
    resolve_reference_request(
        candidate,
        runtime_plugins(candidate).configure(nara::image::plugin(limits)),
    )
}

fn resolve_reference_request(
    candidate: &nara::project_host::ProjectSettingsCandidate,
    request: nara::project_host::ProjectRuntimePlugins,
) -> nara::project_host::RuntimePlan {
    let mut providers = built_in_schema_providers();
    providers.push(REFERENCE_GAME_SCHEMA_PROVIDER);
    resolve_runtime_plan(candidate, request, providers).unwrap()
}

fn project_candidate() -> nara::project_host::ProjectSettingsCandidate {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("nara.toml");
    let capability =
        FileCapability::from_host_handle(File::open(manifest).unwrap(), TrustMode::TrustedLocal, 1)
            .unwrap();
    ingest_project_manifest(&capability, None).unwrap()
}
