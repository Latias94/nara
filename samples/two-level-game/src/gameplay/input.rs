use nara::{
    app::{PluginCategory, PluginDeclaration, PluginId},
    gameplay::{ActionCommandBinding, ActionCommandMap},
    input::{ActionBinding, ActionId, ActionMap, ActionPhase, KeyCode},
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

#[derive(Default)]
pub(super) struct DesktopInputPlugin;
impl Plugin for DesktopInputPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        for (direction, key) in ['a', 'd', 'w', 's'].into_iter().enumerate() {
            let id = ActionId::new(format!("two-level-game.move-{key}")).unwrap();
            app.world_mut()?
                .resource_mut::<ActionMap>()
                .bind(ActionBinding::key(id.clone(), KeyCode::Character(key)));
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
            app.world_mut()?
                .resource_mut::<ActionMap>()
                .bind(ActionBinding::key(id.clone(), KeyCode::Character(key)));
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
