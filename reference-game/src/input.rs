use nara::{
    core::ByteLimit,
    gameplay::{ActionCommandBinding, ActionCommandMap},
    input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapError,
        ActionMapLimits, ActionPhase, KeyCode,
    },
    prelude::{App, PluginError},
};

use crate::{
    MovementDirection, REFERENCE_DESKTOP_PLUGIN_ID, movement_draft, movement_release_draft,
};

const MOVE_LEFT_ACTION: &str = "reference-game.move-left";
const MOVE_RIGHT_ACTION: &str = "reference-game.move-right";
const MOVE_UP_ACTION: &str = "reference-game.move-up";
const MOVE_DOWN_ACTION: &str = "reference-game.move-down";

pub(crate) fn install_desktop_input(app: &mut App) -> Result<(), PluginError> {
    let bindings = [
        (
            MOVE_LEFT_ACTION,
            KeyCode::Character('a'),
            KeyCode::ArrowLeft,
            MovementDirection::Left,
        ),
        (
            MOVE_RIGHT_ACTION,
            KeyCode::Character('d'),
            KeyCode::ArrowRight,
            MovementDirection::Right,
        ),
        (
            MOVE_UP_ACTION,
            KeyCode::Character('w'),
            KeyCode::ArrowUp,
            MovementDirection::Up,
        ),
        (
            MOVE_DOWN_ACTION,
            KeyCode::Character('s'),
            KeyCode::ArrowDown,
            MovementDirection::Down,
        ),
    ];

    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .map_err(action_map_error)?;
    for (id, key, alternate, _) in bindings {
        builder
            .bind(ActionBinding::key(action_id(id), key))
            .map_err(action_map_error)?;
        builder
            .bind(ActionBinding::key(action_id(id), alternate))
            .map_err(action_map_error)?;
    }
    let definition = builder.compile().map_err(action_map_error)?;
    let action_map = ActionMap::new(
        definition.into_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes())
            .expect("action-map state allowance is nonzero"),
    )
    .map_err(action_map_error)?;
    app.insert_resource(action_map)?;

    {
        let world = app.world_mut()?;
        let mut command_map = world.resource_mut::<ActionCommandMap>();
        for (id, _, _, direction) in bindings {
            bind_movement(
                &mut command_map,
                id,
                ActionPhase::Started,
                movement_draft(direction),
            )?;
            bind_movement(
                &mut command_map,
                id,
                ActionPhase::Released,
                movement_release_draft(direction),
            )?;
        }
    }
    Ok(())
}

fn action_map_error(error: ActionMapError) -> PluginError {
    PluginError::SetupFailed {
        plugin: REFERENCE_DESKTOP_PLUGIN_ID,
        message: error.to_string(),
    }
}

fn bind_movement(
    command_map: &mut ActionCommandMap,
    action: &'static str,
    phase: ActionPhase,
    command: nara::gameplay::GameplayCommandDraft,
) -> Result<(), PluginError> {
    let command_type = command.command_type().clone();
    command_map
        .bind(
            ActionCommandBinding::new(action_id(action), phase, command_type).with_command(command),
        )
        .map_err(|_| PluginError::SetupFailed {
            plugin: REFERENCE_DESKTOP_PLUGIN_ID,
            message: "desktop action-command mapping was rejected".to_owned(),
        })
}

fn action_id(id: &'static str) -> ActionId {
    ActionId::new(id).expect("reference-game action IDs are valid")
}
