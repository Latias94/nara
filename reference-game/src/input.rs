use nara::{
    gameplay::{ActionCommandBinding, ActionCommandMap},
    input::{ActionBinding, ActionId, ActionMap, ActionPhase, KeyCode},
    prelude::{App, PluginError},
};

use crate::{MovementDirection, REFERENCE_DESKTOP_PLUGIN_ID, movement_draft, retry_draft};

const MOVE_LEFT_ACTION: &str = "reference-game.move-left";
const MOVE_RIGHT_ACTION: &str = "reference-game.move-right";
const MOVE_UP_ACTION: &str = "reference-game.move-up";
const MOVE_DOWN_ACTION: &str = "reference-game.move-down";
const RETRY_ACTION: &str = "reference-game.retry";

pub(crate) fn install_desktop_input(app: &mut App) -> Result<(), PluginError> {
    let bindings = [
        (
            MOVE_LEFT_ACTION,
            KeyCode::Character('a'),
            MovementDirection::Left,
        ),
        (
            MOVE_RIGHT_ACTION,
            KeyCode::Character('d'),
            MovementDirection::Right,
        ),
        (
            MOVE_UP_ACTION,
            KeyCode::Character('w'),
            MovementDirection::Up,
        ),
        (
            MOVE_DOWN_ACTION,
            KeyCode::Character('s'),
            MovementDirection::Down,
        ),
    ];

    {
        let world = app.world_mut()?;
        let mut action_map = world.resource_mut::<ActionMap>();
        for (id, key, _) in bindings {
            action_map.bind(ActionBinding::key(action_id(id), key));
        }
        action_map.bind(ActionBinding::key(action_id(RETRY_ACTION), KeyCode::Enter));
    }

    {
        let world = app.world_mut()?;
        let mut command_map = world.resource_mut::<ActionCommandMap>();
        for (id, _, direction) in bindings {
            bind_movement(&mut command_map, id, ActionPhase::Started, direction)?;
            bind_movement(
                &mut command_map,
                id,
                ActionPhase::Released,
                MovementDirection::Stop,
            )?;
        }
        command_map
            .bind(
                ActionCommandBinding::new(
                    action_id(RETRY_ACTION),
                    ActionPhase::Started,
                    retry_draft().command_type().clone(),
                )
                .with_command(retry_draft()),
            )
            .map_err(|_| PluginError::SetupFailed {
                plugin: REFERENCE_DESKTOP_PLUGIN_ID,
                message: "desktop retry action-command mapping was rejected".to_owned(),
            })?;
    }
    Ok(())
}

fn bind_movement(
    command_map: &mut ActionCommandMap,
    action: &'static str,
    phase: ActionPhase,
    direction: MovementDirection,
) -> Result<(), PluginError> {
    command_map
        .bind(
            ActionCommandBinding::new(
                action_id(action),
                phase,
                movement_draft(direction).command_type().clone(),
            )
            .with_command(movement_draft(direction)),
        )
        .map_err(|_| PluginError::SetupFailed {
            plugin: REFERENCE_DESKTOP_PLUGIN_ID,
            message: "desktop action-command mapping was rejected".to_owned(),
        })
}

fn action_id(id: &'static str) -> ActionId {
    ActionId::new(id).expect("reference-game action IDs are valid")
}
