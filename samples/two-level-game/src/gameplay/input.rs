use nara::{
    app::{PluginCategory, PluginDeclaration, PluginId},
    asset::{AssetProductRef, ImportedProductId, StableAssetId},
    ecs::{Res, ResMut, World, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs},
    input::{
        ActionContext, ActionId, ActionMaps, ActionPhase, ActionTransform, InputBinding,
        InputSeatId, InputSet, KeyboardActionBinding, SemanticActionOutcome, SemanticActions,
    },
    prelude::{App, CoreStage, Plugin, PluginError},
};

use super::commands::*;
use crate::command::{CommandSchedule, CommandSet, GameCommand};

const ID: PluginId = PluginId::new("two-level-game.input");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime)
    .requires_plugins(&[super::PLUGIN_ID, nara::input::INPUT_PLUGIN_ID]);

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
pub(super) const SAVE_ACTION: &str = "two-level-game.k";

pub(super) fn held_movement(world: &World) -> [bool; 4] {
    let Some(actions) = world.get_resource::<SemanticActions>() else {
        return [false; 4];
    };
    let gameplay = ActionContext::gameplay();
    MOVEMENT_ACTIONS.map(|name| {
        let action = ActionId::new(name).expect("static movement action");
        actions.pressed(InputSeatId::PRIMARY, &action, &gameplay)
    })
}

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
/// Each action must have one keyboard Button source in the enabled gameplay context; gamepad
/// Button alternatives are allowed. Keys must be distinct and have no other bindings. Native
/// adapters decide their own platform support.
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
            .filter(|binding| binding.action == action && binding.context == context)
            .peekable();
        if sources.peek().is_none() {
            return Err(format!(
                "movement action {action} is missing in gameplay context"
            ));
        }
        let mut key = None;
        for binding in sources {
            match (binding.input, binding.transform) {
                (InputBinding::Key(source), ActionTransform::Button) => {
                    if key.replace(source).is_some() {
                        return Err(format!(
                            "movement action {action} has multiple keyboard Button sources in gameplay context"
                        ));
                    }
                }
                (InputBinding::GamepadButton(_), ActionTransform::Button) => {}
                _ => {
                    return Err(format!(
                        "movement action {action} requires one keyboard Button source and only gamepad Button alternatives"
                    ));
                }
            }
        }
        let key = key.ok_or_else(|| {
            format!("movement action {action} requires one keyboard Button source")
        })?;
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
        app.add_systems(
            CoreStage::PreUpdate,
            collect_gameplay_input
                .in_set(CommandSet::FrameInput)
                .after(InputSet::ResolveActions)
                .before(super::verification::drive),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            collect_gameplay_input
                .in_set(CommandSet::FrameInput)
                .before(super::verification::drive),
        )?;
        Ok(())
    }
}

fn collect_gameplay_input(
    actions: Res<SemanticActions>,
    round: Option<Res<super::flow::FlowSnapshot>>,
    mut commands: ResMut<CommandSchedule>,
) -> Result<(), BevyError> {
    for outcome in actions.as_slice() {
        if let Some(command) = gameplay_intent(outcome, round.is_some()) {
            commands.push_local(command).map_err(BevyError::error)?;
        }
    }
    Ok(())
}

fn gameplay_intent(outcome: &SemanticActionOutcome, round: bool) -> Option<GameCommand> {
    if outcome.seat != InputSeatId::PRIMARY || outcome.context != ActionContext::gameplay() {
        return None;
    }
    let pressed = match outcome.phase {
        ActionPhase::Started => true,
        ActionPhase::Released => false,
        _ => return None,
    };
    let name = outcome.action.as_str();
    if let Some(direction) = MOVEMENT_ACTIONS.iter().position(|action| *action == name) {
        return Some(movement(direction, pressed));
    }
    if !pressed {
        return None;
    }
    Some(action(match name {
        "two-level-game.e" => TRAVEL,
        "two-level-game.q" => INSTANCE,
        "two-level-game.m" => MARKER,
        SAVE_ACTION if round => return Some(GameCommand::RoundSave),
        SAVE_ACTION => SAVE,
        "two-level-game.l" => RESTORE,
        "two-level-game.r" => RETRY,
        "two-level-game.b" => SIDE_ROOM,
        "two-level-game.g" => LOOKOUT,
        _ => return None,
    }))
}

#[cfg(test)]
mod tests;
