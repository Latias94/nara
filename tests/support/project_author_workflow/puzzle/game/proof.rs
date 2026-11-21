//! Opt-in normalized device input and fixed-tick observations for the process fixture.

use nara::{
    app::{App, CoreStage, FixedTime, FixedUpdateSet, PluginError, StartupStage},
    ecs::{Res, ResMut, Resource, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs},
    input::{
        ButtonDriverInput, ButtonTransitionPhase, GamepadButton, GamepadConnection, InputTimeline,
        InputWindowId, KeyCode,
    },
};

use super::{PLUGIN_ID, Puzzle, SEAT, simulate};

const WINDOW: InputWindowId = InputWindowId::new(7);
const RIGHT_KEY: KeyCode = KeyCode::Character('d');

#[derive(Resource)]
enum Device {
    Keyboard,
    Gamepad(Option<GamepadConnection>),
}

pub(super) fn install(app: &mut App) -> Result<(), PluginError> {
    let Some(mode) = std::env::var_os("NARA_PUSH_PROOF_INPUT") else {
        return Ok(());
    };
    let device = match mode.to_str() {
        Some("keyboard") => Device::Keyboard,
        Some("gamepad") => Device::Gamepad(None),
        _ => {
            return Err(PluginError::SetupFailed {
                plugin: PLUGIN_ID,
                message: "NARA_PUSH_PROOF_INPUT must be keyboard or gamepad".into(),
            });
        }
    };
    app.insert_resource(device)?
        .add_systems(StartupStage::Core, connect)?
        .add_systems(
            CoreStage::FixedUpdate,
            observe_and_feed
                .after(simulate)
                .in_set(FixedUpdateSet::Simulate),
        )?;
    Ok(())
}

fn connect(
    mut timeline: ResMut<InputTimeline>,
    mut device: ResMut<Device>,
) -> Result<(), BevyError> {
    timeline.connect_endpoint(WINDOW, SEAT)?;
    if let Device::Gamepad(connection) = &mut *device {
        *connection = Some(timeline.connect_gamepad(WINDOW, SEAT, 0)?);
    }
    Ok(())
}

fn observe_and_feed(
    time: Res<FixedTime>,
    puzzle: Res<Puzzle>,
    device: Res<Device>,
    mut timeline: ResMut<InputTimeline>,
) -> Result<(), BevyError> {
    let tick = time.tick();
    use ButtonTransitionPhase::{Pressed, Released};

    let phases: &[_] = match tick {
        1 | 5 => &[Pressed],
        4 | 8 => &[Released],
        9 => &[Pressed, Released, Pressed, Released],
        _ => &[],
    };
    for &phase in phases {
        let admission = match &*device {
            Device::Keyboard => {
                let input = match phase {
                    ButtonTransitionPhase::Pressed => ButtonDriverInput::Press(RIGHT_KEY),
                    ButtonTransitionPhase::Released => ButtonDriverInput::Release(RIGHT_KEY),
                };
                timeline.admit_keyboard(WINDOW, SEAT, input)?
            }
            Device::Gamepad(Some(connection)) => {
                timeline.admit_gamepad_button(connection, GamepadButton::DPadRight.code(), phase)?
            }
            Device::Gamepad(None) => return Err("proof gamepad is not connected".into()),
        };
        if admission.admitted() != 1 {
            return Err("proof input must admit exactly one physical edge".into());
        }
    }
    if matches!(tick, 1 | 2 | 3 | 4 | 6 | 8 | 10 | 11 | 12 | 14 | 16) {
        println!(
            "push_proof tick={tick} actor=({},{}) box=({},{}) door_open={} won={}",
            puzzle.actor.cell.column,
            puzzle.actor.cell.row,
            puzzle.box_piece.cell.column,
            puzzle.box_piece.cell.row,
            puzzle.door_open,
            puzzle.won,
        );
    }
    Ok(())
}
