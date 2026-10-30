use nara::{
    app::{PluginCategory, PluginDeclaration, PluginId},
    core::ByteLimit,
    gameplay::{ActionCommandBinding, ActionCommandMap},
    input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapError,
        ActionMapLimits, ActionPhase, KeyCode,
    },
    prelude::{App, Plugin, PluginError},
};

use crate::{Direction, movement, reset};

const ID: PluginId = PluginId::new("collect-game.desktop-input");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime)
    .requires_plugins(&[
        crate::PLUGIN_ID,
        nara::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
        nara::input::INPUT_PLUGIN_ID,
    ]);

#[derive(Default)]
pub struct DesktopInputPlugin;

impl Plugin for DesktopInputPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let directions = [
            ('a', Direction::Left),
            ('d', Direction::Right),
            ('w', Direction::Up),
            ('s', Direction::Down),
        ]
        .map(|(key, direction)| {
            (
                ActionId::new(format!("collect-game.move-{key}")).expect("static action ID"),
                KeyCode::Character(key),
                direction,
            )
        });
        let reset_action = ActionId::new("collect-game.reset").expect("static action ID");
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .map_err(action_map_error)?;
        for (id, key, _) in &directions {
            builder
                .bind(ActionBinding::key(id.clone(), *key))
                .map_err(action_map_error)?;
        }
        builder
            .bind(ActionBinding::key(
                reset_action.clone(),
                KeyCode::Character('r'),
            ))
            .map_err(action_map_error)?;
        let definition = builder.compile().map_err(action_map_error)?;
        let action_map = ActionMap::new(
            definition.into_bindings(),
            ByteLimit::new(ActionMap::required_state_bytes())
                .expect("action-map state allowance is nonzero"),
        )
        .map_err(action_map_error)?;
        app.insert_resource(action_map)?;

        for (id, _, direction) in directions {
            for (phase, pressed) in [(ActionPhase::Started, true), (ActionPhase::Released, false)] {
                bind(app, id.clone(), phase, movement(direction, pressed))?;
            }
        }
        bind(app, reset_action, ActionPhase::Started, reset())
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
