use nara::{
    app::{PluginCategory, PluginDeclaration, PluginId},
    core::ByteLimit,
    gameplay::{ActionCommandBinding, ActionCommandMap},
    input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapError,
        ActionMapLimits, ActionMaps, ActionPhase, KeyCode,
    },
    prelude::{App, Plugin, PluginError},
};

use super::commands::*;

const ID: PluginId = PluginId::new("two-level-game.desktop-input");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime)
    .requires_plugins(&[
        super::PLUGIN_ID,
        nara::input::INPUT_PLUGIN_ID,
        nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
    ]);

/// Movement keys in command-payload direction order: left, right, up, down.
pub(crate) const MOVE_KEYS: [KeyCode; 4] = [
    KeyCode::Character('a'),
    KeyCode::Character('d'),
    KeyCode::Character('w'),
    KeyCode::Character('s'),
];

pub(crate) fn movement_action(direction: usize) -> Option<ActionId> {
    let KeyCode::Character(key) = MOVE_KEYS.get(direction)? else {
        return None;
    };
    Some(ActionId::new(format!("two-level-game.move-{key}")).expect("static movement action"))
}

pub(crate) fn movement_binding(direction: usize) -> Option<ActionBinding> {
    let key = *MOVE_KEYS.get(direction)?;
    Some(
        ActionBinding::key(movement_action(direction)?, key)
            .with_context(ActionContext::gameplay()),
    )
}

/// Exact keyboard Button declarations corresponding to the game's four held movement bits.
pub fn movement_keyboard_bindings() -> [nara::input::KeyboardActionBinding; 4] {
    std::array::from_fn(|direction| {
        let binding = movement_binding(direction).expect("static movement binding");
        nara::input::KeyboardActionBinding {
            key: MOVE_KEYS[direction],
            action: binding.action,
            context: binding.context,
        }
    })
}

#[derive(Default)]
pub(super) struct DesktopInputPlugin;
impl Plugin for DesktopInputPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .map_err(action_map_error)?;
        for direction in 0..MOVE_KEYS.len() {
            let binding = movement_binding(direction).expect("static movement binding");
            let id = binding.action.clone();
            builder.bind(binding).map_err(action_map_error)?;
            for (phase, pressed) in [(ActionPhase::Started, true), (ActionPhase::Released, false)] {
                bind(app, id.clone(), phase, movement(direction, pressed))?;
            }
        }
        for (key, command) in [
            ('e', TRAVEL),
            ('q', INSTANCE),
            ('m', MARKER),
            ('k', SAVE),
            ('l', RESTORE),
            ('r', RETRY),
        ] {
            let id = ActionId::new(format!("two-level-game.{key}")).unwrap();
            builder
                .bind(ActionBinding::key(id.clone(), KeyCode::Character(key)))
                .map_err(action_map_error)?;
            bind(app, id, ActionPhase::Started, action(command))?;
        }
        let definition = builder.compile().map_err(action_map_error)?;
        let action_map = ActionMap::new(
            definition.into_bindings(),
            ByteLimit::new(ActionMap::required_state_bytes())
                .expect("action-map state allowance is nonzero"),
        )
        .map_err(action_map_error)?;
        app.insert_resource(ActionMaps::new(action_map))?;
        Ok(())
    }
}

fn action_map_error(error: ActionMapError) -> PluginError {
    PluginError::SetupFailed {
        plugin: ID,
        message: error.to_string(),
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
