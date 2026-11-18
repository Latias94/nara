use nara::{
    app::{PluginCategory, PluginDeclaration, PluginId},
    asset::{AssetProductRef, ImportedProductId, StableAssetId},
    ecs::World,
    gameplay::{ActionCommandBinding, ActionCommandMap},
    input::{
        ActionContext, ActionId, ActionMaps, ActionPhase, ActionTransform, InputBinding,
        InputSeatId, KeyboardActionBinding,
    },
    prelude::{App, Plugin, PluginError},
};

use super::commands::*;

const ID: PluginId = PluginId::new("two-level-game.input");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime)
    .requires_plugins(&[
        super::PLUGIN_ID,
        nara::input::INPUT_PLUGIN_ID,
        nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
    ]);

/// Project-relative path to the authored global action map.
pub const INPUT_PATH: &str = "input/explorer.actions.json";
/// Intrinsic source identity preserved across edits to the action map.
pub const INPUT_SOURCE_ID: &str = "b23d8f3a-9770-5c1a-bf4d-81fb93171257";

const MOVEMENT_ACTIONS: [&str; 4] = [
    "two-level-game.move-a",
    "two-level-game.move-d",
    "two-level-game.move-w",
    "two-level-game.move-s",
];

/// Complete primary Product selected by the source manifest and cooked Runtime configuration.
#[must_use]
pub fn input_product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(INPUT_SOURCE_ID).expect("fixed valid explorer input identity"),
        ImportedProductId::PRIMARY,
    )
    .expect("fixed non-nil primary explorer input Product")
}

/// Resolves left, right, up and down from the installed PRIMARY map for exact continuation.
///
/// Each action must have one keyboard Button source in the enabled gameplay context. Keys must
/// be distinct and have no other bindings. Native adapters decide their own platform support.
/// Other valid gameplay mappings remain usable but cannot provide this exact keyboard capture.
pub fn movement_bindings(world: &World) -> Result<[KeyboardActionBinding; 4], String> {
    let map = world
        .get_resource::<ActionMaps>()
        .and_then(|maps| maps.get(InputSeatId::PRIMARY))
        .ok_or_else(|| "PRIMARY action map is unavailable".to_owned())?;
    let context = ActionContext::gameplay();
    if !map.is_context_enabled(&context) {
        return Err("gameplay input context is disabled or missing".into());
    }
    let [left, right, up, down] = MOVEMENT_ACTIONS.map(|name| {
        let action = ActionId::new(name).expect("static movement action");
        let mut sources = map
            .bindings()
            .iter()
            .filter(|binding| binding.action == action && binding.context == context);
        let binding = sources
            .next()
            .ok_or_else(|| format!("movement action {action} is missing in gameplay context"))?;
        if sources.next().is_some() {
            return Err(format!(
                "movement action {action} has multiple sources in gameplay context"
            ));
        }
        let (InputBinding::Key(key), ActionTransform::Button) = (binding.input, binding.transform)
        else {
            return Err(format!(
                "movement action {action} requires one keyboard Button source"
            ));
        };
        Ok(KeyboardActionBinding {
            key,
            action,
            context: context.clone(),
        })
    });
    let bindings = [left?, right?, up?, down?];
    if bindings.iter().enumerate().any(|(index, binding)| {
        bindings[..index]
            .iter()
            .any(|other| other.key == binding.key)
    }) {
        return Err("movement keys must be distinct".into());
    }
    for binding in &bindings {
        if map
            .binding_indices_for_input(InputBinding::Key(binding.key))
            .count()
            != 1
        {
            return Err(format!(
                "movement key {:?} must not be shared with another binding",
                binding.key,
            ));
        }
    }
    Ok(bindings)
}

/// Reads exact movement bindings only at the original complete paused desktop boundary.
#[cfg(feature = "desktop")]
pub fn movement_keyboard_bindings(
    run: &nara::project_host::DesktopRun,
) -> Result<[KeyboardActionBinding; 4], crate::GameResumeError> {
    run.capture(movement_bindings)?
        .map_err(crate::GameResumeError::Ineligible)
}

#[derive(Default)]
pub(super) struct GameInputPlugin;
impl Plugin for GameInputPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        for (direction, name) in MOVEMENT_ACTIONS.into_iter().enumerate() {
            let id = ActionId::new(name).expect("static movement action");
            for (phase, pressed) in [(ActionPhase::Started, true), (ActionPhase::Released, false)] {
                bind(app, id.clone(), phase, movement(direction, pressed))?;
            }
        }
        for (name, command) in [
            ("two-level-game.e", TRAVEL),
            ("two-level-game.q", INSTANCE),
            ("two-level-game.m", MARKER),
            ("two-level-game.k", SAVE),
            ("two-level-game.l", RESTORE),
            ("two-level-game.r", RETRY),
            ("two-level-game.b", SIDE_ROOM),
            ("two-level-game.g", LOOKOUT),
        ] {
            let id = ActionId::new(name).expect("static game action");
            bind(app, id, ActionPhase::Started, action(command))?;
        }
        Ok(())
    }
}

fn bind(
    app: &mut App,
    id: ActionId,
    phase: ActionPhase,
    command: nara::gameplay::GameplayCommandDraft,
) -> Result<(), PluginError> {
    app.world_mut()?
        .resource_mut::<ActionCommandMap>()
        .bind(
            ActionCommandBinding::new(id, phase, command.command_type().clone())
                .with_command(command),
        )
        .map_err(|error| PluginError::SetupFailed {
            plugin: ID,
            message: error.to_string(),
        })
}

#[cfg(test)]
mod tests;
