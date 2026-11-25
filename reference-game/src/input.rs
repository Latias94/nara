use nara::{
    app::{FixedUpdateSet, RealTime},
    core::ByteLimit,
    ecs::{Res, ResMut, Resource, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs},
    input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapError,
        ActionMapLimits, ActionMaps, ActionPhase, InputSeatId, InputSet, KeyCode, SemanticActions,
    },
    prelude::{App, CoreStage, PluginError},
};

use crate::{
    MovementDirection, REFERENCE_DESKTOP_PLUGIN_ID,
    wave_input::{WaveInput, WaveIntent, prepare_wave_input},
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
    app.insert_resource(ActionMaps::new(action_map))?
        .init_resource::<MovementInputFrame>()?
        .add_systems(
            CoreStage::PreUpdate,
            collect_movement.after(InputSet::ResolveActions),
        )?
        .add_systems(
            CoreStage::FixedUpdate,
            collect_movement
                .in_set(FixedUpdateSet::Prepare)
                .before(prepare_wave_input),
        )?;
    Ok(())
}

#[derive(Default, Resource)]
struct MovementInputFrame(Option<u64>);

fn collect_movement(
    time: Res<RealTime>,
    mut frame: ResMut<MovementInputFrame>,
    actions: Res<SemanticActions>,
    mut input: ResMut<WaveInput>,
) -> Result<(), BevyError> {
    if frame.0 == Some(time.frame) {
        return Ok(());
    }
    frame.0 = Some(time.frame);
    for action in actions.as_slice() {
        if action.seat != InputSeatId::PRIMARY || action.context != ActionContext::gameplay() {
            continue;
        }
        let direction = match action.action.as_str() {
            MOVE_LEFT_ACTION => MovementDirection::Left,
            MOVE_RIGHT_ACTION => MovementDirection::Right,
            MOVE_UP_ACTION => MovementDirection::Up,
            MOVE_DOWN_ACTION => MovementDirection::Down,
            _ => continue,
        };
        let pressed = match action.phase {
            ActionPhase::Started => true,
            ActionPhase::Released => false,
            _ => continue,
        };
        input.push(WaveIntent::Move { direction, pressed })?;
    }
    Ok(())
}

fn action_map_error(error: ActionMapError) -> PluginError {
    PluginError::SetupFailed {
        plugin: REFERENCE_DESKTOP_PLUGIN_ID,
        message: error.to_string(),
    }
}

fn action_id(id: &'static str) -> ActionId {
    ActionId::new(id).expect("reference-game action IDs are valid")
}
