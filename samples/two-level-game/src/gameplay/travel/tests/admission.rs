use nara::{
    app::RuntimeInstance,
    ecs::schedule::IntoScheduleConfigs,
    gameplay::{
        GameplayCommandBatch, GameplayCommandDraft, GameplayCommandEnvelope,
        GameplayCommandIngressSource, GameplayCommandPayload, GameplayCommandPlugin,
        GameplayCommandQueue, GameplayCommandSet, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTarget, GameplayCommandTick,
        GameplayCommandValue,
    },
    prelude::{App, CoreStage, FixedTime, Res, ResMut, Resource},
};

use crate::{
    command::SemanticCommand,
    gameplay::{GamePhase, Session, commands},
};

#[derive(Clone, Default, Resource)]
struct Captured {
    commands: Vec<GameplayCommandEnvelope>,
    held: [bool; 4],
    phase: GamePhase,
    error: Option<String>,
}

fn observe(
    batch: Res<GameplayCommandBatch>,
    session: Res<Session>,
    mut captured: ResMut<Captured>,
) {
    captured.commands.extend_from_slice(batch.commands());
    captured.held = session.held;
    captured.phase = session.phase;
    captured.error.clone_from(&session.error);
}

fn consume(drafts: Vec<GameplayCommandDraft>, phase: GamePhase, held: [bool; 4]) -> Captured {
    let mut app = App::new();
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    app.insert_resource(Session {
        phase,
        held,
        ..Session::default()
    })
    .unwrap();
    app.init_resource::<Captured>().unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        super::super::consume.in_set(GameplayCommandSet::Consume),
    )
    .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        observe.in_set(GameplayCommandSet::Capture),
    )
    .unwrap();
    for (index, draft) in drafts.into_iter().enumerate() {
        app.world_mut()
            .unwrap()
            .resource_mut::<GameplayCommandQueue>()
            .submit(GameplayCommandSubmission::new(
                GameplayCommandTick::new(1).unwrap(),
                GameplayCommandIngressSource::external("two-level-game.movement-admission-test")
                    .unwrap(),
                GameplayCommandSourceSequence::new(index as u64 + 1).unwrap(),
                draft,
            ))
            .unwrap();
    }
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let captured = runtime.world().resource::<Captured>().clone();
    let stats = runtime.world().resource::<GameplayCommandQueue>().stats();
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
fn live_movement_and_recording_reject_the_same_malformed_envelopes_before_held_changes() {
    let mut extra = commands::movement(1, true).payload().clone();
    extra
        .insert("extra", GameplayCommandValue::Bool(false))
        .unwrap();
    let mut wrong_pressed = commands::movement(1, true).payload().clone();
    wrong_pressed
        .insert("pressed", GameplayCommandValue::I64(1))
        .unwrap();
    let mut missing_direction = GameplayCommandPayload::new();
    missing_direction
        .insert("pressed", GameplayCommandValue::Bool(true))
        .unwrap();
    let mut negative = commands::movement(1, true).payload().clone();
    negative
        .insert("direction", GameplayCommandValue::I64(-1))
        .unwrap();
    for draft in [
        commands::movement(1, true).with_payload(extra),
        commands::movement(1, true).with_target(GameplayCommandTarget::named("player").unwrap()),
        commands::movement(1, true).with_payload(wrong_pressed),
        commands::movement(1, true).with_payload(missing_direction),
        commands::movement(1, true).with_payload(negative),
        commands::movement(4, true),
        commands::action(commands::MOVE),
    ] {
        let captured = consume(vec![draft], GamePhase::Running, [false; 4]);
        assert_eq!(captured.commands.len(), 1);
        let error = SemanticCommand::from_envelope(&captured.commands[0]).unwrap_err();
        assert_eq!(captured.held, [false; 4], "{:?}", captured.commands[0]);
        assert_eq!(captured.phase, GamePhase::Running);
        assert_eq!(captured.error.as_deref(), Some(error.as_str()));
    }
}

#[test]
fn live_and_recorded_movement_keep_ordered_press_release_and_exact_consumption_keys() {
    let drafts = (0..4)
        .flat_map(|direction| {
            [
                commands::movement(direction, true),
                commands::movement(direction, false),
            ]
        })
        .chain([commands::movement(2, true)])
        .collect();
    let captured = consume(drafts, GamePhase::Running, [false; 4]);
    let actual = captured
        .commands
        .iter()
        .map(SemanticCommand::from_envelope)
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
                .all(|command| SemanticCommand::from_envelope(command).is_ok())
        );
    }
}
