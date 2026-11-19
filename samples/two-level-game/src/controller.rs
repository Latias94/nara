//! Optional native controller configuration shared by source and cooked game recipes.

struct GamepadConfiguration(nara_gilrs::GilrsInputPlugin);

impl nara::ProductConfiguration for GamepadConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"two-level-game.gilrs/v1\0");
        match self.0.default_route {
            Some(route) => {
                output.push(1);
                output.extend_from_slice(&route.window.get().to_le_bytes());
                output.extend_from_slice(&route.seat.get().to_le_bytes());
            }
            None => output.push(0),
        }
        for limit in [self.0.max_devices, self.0.max_events_per_frame] {
            output.extend_from_slice(
                &u64::try_from(limit)
                    .expect("gamepad configuration limit fits u64")
                    .to_le_bytes(),
            );
        }
    }
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
    let configuration = GamepadConfiguration(nara_gilrs::GilrsInputPlugin::new(Some(
        nara_gilrs::GamepadRoute::new(
            nara::input::InputWindowId::new(1),
            nara::input::InputSeatId::PRIMARY,
        ),
    )));
    recipe.add_configured_plugin(configuration, |config| config.0)
}

#[cfg(test)]
mod tests;
