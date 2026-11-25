use nara::{
    app::RuntimeInstance,
    ecs::schedule::IntoScheduleConfigs,
    prelude::{App, CoreStage, FixedTime, Res, ResMut, Resource},
};

use crate::{
    command::{
        CommandSchedule, CommandSequence, CommandSource, CommandTick, GameCommand,
        ScheduledCommand, SemanticCommand, acknowledge_commands, prepare_commands,
    },
    gameplay::{GamePhase, Session, commands},
};

#[derive(Clone, Default, Resource)]
struct Captured {
    commands: Vec<ScheduledCommand>,
    held: [bool; 4],
    phase: GamePhase,
    error: Option<String>,
}

fn observe(schedule: Res<CommandSchedule>, session: Res<Session>, mut captured: ResMut<Captured>) {
    captured.commands.extend_from_slice(schedule.current());
    captured.held = session.held;
    captured.phase = session.phase;
    captured.error.clone_from(&session.error);
}

fn consume(commands: Vec<GameCommand>, phase: GamePhase, held: [bool; 4]) -> Captured {
    let mut app = App::new();
    app.init_resource::<CommandSchedule>().unwrap();
    app.insert_resource(Session {
        phase,
        held,
        ..Session::default()
    })
    .unwrap();
    app.init_resource::<Captured>().unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        (
            prepare_commands,
            super::consume,
            observe,
            acknowledge_commands,
        )
            .chain(),
    )
    .unwrap();
    {
        let mut schedule = app.world_mut().unwrap().resource_mut::<CommandSchedule>();
        for (index, command) in commands.into_iter().enumerate() {
            schedule
                .schedule(ScheduledCommand::new(
                    CommandTick::new(1).unwrap(),
                    CommandSource::external("two-level-game.movement-admission-test").unwrap(),
                    CommandSequence::new(index as u64 + 1).unwrap(),
                    command,
                ))
                .unwrap();
        }
    }
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let captured = runtime.world().resource::<Captured>().clone();
    let stats = runtime.world().resource::<CommandSchedule>().stats();
    runtime.finish().unwrap();
    assert_eq!(stats.acknowledged_through_tick, 1);
    assert_eq!(stats.pending_commands, 0);
    for (index, command) in captured.commands.iter().enumerate() {
        assert_eq!(command.tick().get(), 1);
        assert_eq!(command.source_sequence().get(), index as u64 + 1);
    }
    captured
}

#[test]
fn live_movement_and_recording_reject_out_of_range_directions_before_held_changes() {
    for pressed in [true, false] {
        let held = [false, true, false, false];
        let captured = consume(
            vec![commands::movement(4, pressed)],
            GamePhase::Running,
            held,
        );
        assert_eq!(captured.commands.len(), 1);
        let error = SemanticCommand::from_scheduled(&captured.commands[0]).unwrap_err();
        assert_eq!(captured.held, held);
        assert_eq!(captured.phase, GamePhase::Running);
        assert_eq!(captured.error.as_deref(), Some(error.as_str()));
    }
}

#[test]
fn live_and_recorded_movement_keep_ordered_press_release_and_exact_consumption_keys() {
    let commands = (0..4)
        .flat_map(|direction| {
            [
                commands::movement(direction, true),
                commands::movement(direction, false),
            ]
        })
        .chain([commands::movement(2, true)])
        .collect();
    let captured = consume(commands, GamePhase::Running, [false; 4]);
    let actual = captured
        .commands
        .iter()
        .map(SemanticCommand::from_scheduled)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let expected = (0..4)
        .flat_map(|direction| {
            [
                SemanticCommand::Move {
                    direction,
                    pressed: true,
                },
                SemanticCommand::Move {
                    direction,
                    pressed: false,
                },
            ]
        })
        .chain([SemanticCommand::Move {
            direction: 2,
            pressed: true,
        }])
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert_eq!(captured.held, [false, false, true, false]);
    assert_eq!(captured.phase, GamePhase::Running);
    assert!(captured.error.is_none());
}

#[test]
fn valid_releases_still_clear_held_input_while_new_presses_wait_for_running() {
    for phase in [
        GamePhase::Preparing,
        GamePhase::Reconstructing,
        GamePhase::Degraded,
    ] {
        let captured = consume(
            vec![commands::movement(0, true), commands::movement(1, false)],
            phase,
            [false, true, false, false],
        );
        assert_eq!(captured.phase, phase);
        assert_eq!(captured.held, [false; 4]);
        assert!(captured.error.is_none());
        assert!(
            captured
                .commands
                .iter()
                .all(|command| SemanticCommand::from_scheduled(command).is_ok())
        );
    }
}
