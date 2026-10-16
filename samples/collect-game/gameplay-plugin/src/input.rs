use nara::{
    app::{PluginCategory, PluginDeclaration, PluginId},
    gameplay::{ActionCommandBinding, ActionCommandMap},
    input::{ActionBinding, ActionId, ActionMap, ActionPhase, KeyCode},
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
        ];
        for (key, direction) in directions {
            let id = ActionId::new(format!("collect-game.move-{key}")).expect("static action ID");
            app.world_mut()?
                .resource_mut::<ActionMap>()
                .bind(ActionBinding::key(id.clone(), KeyCode::Character(key)));
            for (phase, pressed) in [(ActionPhase::Started, true), (ActionPhase::Released, false)] {
                bind(app, id.clone(), phase, movement(direction, pressed))?;
            }
        }
        let id = ActionId::new("collect-game.reset").expect("static action ID");
        app.world_mut()?
            .resource_mut::<ActionMap>()
            .bind(ActionBinding::key(id.clone(), KeyCode::Character('r')));
        bind(app, id, ActionPhase::Started, reset())
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
