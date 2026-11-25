use nara::{
    ProductRecipe, ProductRecipeEntryKind, ProductRecipeError,
    input::{GILRS_INPUT_PLUGIN_ID, GamepadRoute, GilrsInputPlugin},
};

use super::with_gilrs_input;

fn controller_fingerprint(recipe: &ProductRecipe) -> [u8; 32] {
    recipe
        .entries()
        .find(|entry| entry.plugin_id() == GILRS_INPUT_PLUGIN_ID)
        .unwrap()
        .configuration_fingerprint()
}

#[test]
fn controller_fingerprint_preserves_original_desktop_configuration() {
    let mut original_configuration = b"two-level-game.gilrs/v1\0".to_vec();
    original_configuration.push(1);
    original_configuration.extend_from_slice(&1_u64.to_le_bytes());
    original_configuration.extend_from_slice(&0_u32.to_le_bytes());
    original_configuration.extend_from_slice(&16_u64.to_le_bytes());
    original_configuration.extend_from_slice(&256_u64.to_le_bytes());
    let original = ProductRecipe::new()
        .add_configured_plugin(original_configuration, || {
            GilrsInputPlugin::new(Some(GamepadRoute::new(
                nara::input::InputWindowId::new(1),
                nara::input::InputSeatId::PRIMARY,
            )))
        })
        .unwrap();
    let shared = with_gilrs_input(ProductRecipe::new()).unwrap();

    assert_eq!(shared.len(), 1);
    let entry = shared.entries().next().unwrap();
    assert_eq!(entry.plugin_id(), GILRS_INPUT_PLUGIN_ID);
    assert_eq!(entry.kind(), ProductRecipeEntryKind::RuntimePlugin);
    assert!(entry.schema_provider_ids().is_empty());
    assert!(entry.data_schema_owners().is_empty());
    assert_eq!(
        controller_fingerprint(&shared),
        controller_fingerprint(&original),
    );
}

#[test]
fn source_and_cooked_recipes_share_controller_configuration_and_preserve_existing_entries() {
    let expected = with_gilrs_input(ProductRecipe::new()).unwrap();
    for recipe in [
        crate::product_recipe(),
        crate::cooked_recipe(),
        crate::cooked_desktop_recipe(),
    ] {
        assert!(
            recipe
                .entries()
                .all(|entry| entry.plugin_id() != GILRS_INPUT_PLUGIN_ID)
        );
        let configured = with_gilrs_input(recipe.clone()).unwrap();

        assert_eq!(configured.len(), recipe.len() + 1);
        assert_eq!(
            controller_fingerprint(&configured),
            controller_fingerprint(&expected),
        );
        for original in recipe.entries() {
            let retained = configured
                .entries()
                .find(|entry| entry.plugin_id() == original.plugin_id())
                .unwrap();
            assert_eq!(retained.kind(), original.kind());
            assert_eq!(
                retained.configuration_fingerprint(),
                original.configuration_fingerprint(),
            );
        }
    }
}

#[test]
fn duplicate_controller_configuration_returns_original_recipe_error() {
    let recipe = with_gilrs_input(ProductRecipe::new()).unwrap();

    assert_eq!(
        with_gilrs_input(recipe).unwrap_err(),
        ProductRecipeError::DuplicatePlugin {
            plugin: GILRS_INPUT_PLUGIN_ID,
        },
    );
}
