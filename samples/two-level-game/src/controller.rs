//! Optional native controller configuration shared by source and cooked game recipes.

fn controller_configuration_bytes(plugin: &nara::input::GilrsInputPlugin) -> Vec<u8> {
    let mut output = b"two-level-game.gilrs/v1\0".to_vec();
    match plugin.default_route {
        Some(route) => {
            output.push(1);
            output.extend_from_slice(&route.window.get().to_le_bytes());
            output.extend_from_slice(&route.seat.get().to_le_bytes());
        }
        None => output.push(0),
    }
    for limit in [plugin.max_devices, plugin.max_events_per_frame] {
        output.extend_from_slice(
            &u64::try_from(limit)
                .expect("gamepad configuration limit fits u64")
                .to_le_bytes(),
        );
    }
    output
}

/// Adds Gilrs input for window 1 and the primary seat with the backend's default limits.
///
/// Recipe construction acquires no native context. The selected Host installs the plugin when
/// building the Runtime; the route requires an existing input endpoint.
///
/// # Errors
///
/// Returns the original recipe error if the configured plugin cannot be added, including when
/// Gilrs input is already present.
pub fn with_gilrs_input(
    recipe: nara::ProductRecipe,
) -> Result<nara::ProductRecipe, nara::ProductRecipeError> {
    let plugin = nara::input::GilrsInputPlugin::new(Some(nara::input::GamepadRoute::new(
        nara::input::InputWindowId::new(1),
        nara::input::InputSeatId::PRIMARY,
    )));
    recipe.add_configured_plugin(controller_configuration_bytes(&plugin), move || plugin)
}

#[cfg(test)]
mod tests;
