use nara::{
    app::{FixedUpdateSet, PluginCategory, PluginDeclaration, PluginId, RealTime},
    core::ByteLimit,
    ecs::{Res, ResMut, Resource, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs},
    input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapError,
        ActionMapLimits, ActionMaps, ActionPhase, InputSeatId, InputSet, KeyCode, SemanticActions,
    },
    prelude::{App, CoreStage, Plugin, PluginError},
};

use crate::{CollectionInput, Direction, consume_controls, movement, reset};

const ID: PluginId = PluginId::new("collect-game.desktop-input");
const DECLARATION: PluginDeclaration = PluginDeclaration::new(ID, PluginCategory::Runtime)
    .requires_plugins(&[crate::PLUGIN_ID, nara::input::INPUT_PLUGIN_ID]);

#[derive(Default)]
pub struct DesktopInputPlugin;

impl Plugin for DesktopInputPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let directions = ['a', 'd', 'w', 's'].map(|key| {
            (
                ActionId::new(format!("collect-game.move-{key}")).expect("static action ID"),
                KeyCode::Character(key),
            )
        });
        let reset_action = ActionId::new("collect-game.reset").expect("static action ID");
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .map_err(action_map_error)?;
        for (id, key) in directions {
            builder
                .bind(ActionBinding::key(id, key))
                .map_err(action_map_error)?;
        }
        builder
            .bind(ActionBinding::key(reset_action, KeyCode::Character('r')))
            .map_err(action_map_error)?;
        let definition = builder.compile().map_err(action_map_error)?;
        let action_map = ActionMap::new(
            definition.into_bindings(),
            ByteLimit::new(ActionMap::required_state_bytes())
                .expect("action-map state allowance is nonzero"),
        )
        .map_err(action_map_error)?;
        app.insert_resource(ActionMaps::new(action_map))?
            .init_resource::<InputFrame>()?
            .add_systems(
                CoreStage::PreUpdate,
                collect_controls.after(InputSet::ResolveActions),
            )?
            .add_systems(
                CoreStage::FixedUpdate,
                collect_controls
                    .in_set(FixedUpdateSet::Prepare)
                    .before(consume_controls),
            )?;
        Ok(())
    }
}

fn action_map_error(error: ActionMapError) -> PluginError {
    PluginError::SetupFailed {
        plugin: ID,
        message: error.to_string(),
    }
}

#[derive(Default, Resource)]
struct InputFrame(Option<u64>);

fn collect_controls(
    time: Res<RealTime>,
    mut frame: ResMut<InputFrame>,
    actions: Res<SemanticActions>,
    mut input: ResMut<CollectionInput>,
) -> Result<(), BevyError> {
    if frame.0 == Some(time.frame) {
        return Ok(());
    }
    frame.0 = Some(time.frame);
    for action in actions.as_slice() {
        if action.seat != InputSeatId::PRIMARY || action.context != ActionContext::gameplay() {
            continue;
        }
        let pressed = match action.phase {
            ActionPhase::Started => true,
            ActionPhase::Released => false,
            _ => continue,
        };
        let direction = match action.action.as_str() {
            "collect-game.move-a" => Direction::Left,
            "collect-game.move-d" => Direction::Right,
            "collect-game.move-w" => Direction::Up,
            "collect-game.move-s" => Direction::Down,
            "collect-game.reset" if pressed => {
                input.push(reset())?;
                continue;
            }
            _ => continue,
        };
        input.push(movement(direction, pressed))?;
    }
    Ok(())
}
