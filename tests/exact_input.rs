#![cfg(feature = "runtime-core")]

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use nara::{
    app::{
        App, CoreStage, FixedTime, RealTime, RuntimeControl, RuntimeControlRequestResult,
        RuntimeControlStatus, RuntimeDriverScope, RuntimeFrameKind, RuntimeInstance, RuntimeState,
        RuntimeTimeSettings,
    },
    core::ByteLimit,
    ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs},
    gameplay::{
        ActionCommandBinding, ActionCommandMap, GameplayCommandBatch, GameplayCommandDraft,
        GameplayCommandEnvelope, GameplayCommandPlugin, GameplayCommandQueue, GameplayCommandSet,
        GameplayCommandSource, GameplayCommandTypeId, LocalGameplayCommands,
    },
    input::{
        __private::claim_input_endpoint, ActionBinding, ActionContext, ActionId, ActionMap,
        ActionMapBuilder, ActionMapLimits, ActionMaps, ActionOutcomes, ActionPhase,
        ButtonDriverInput, ButtonInput, InputLimits, InputPlugin, InputSeatId, InputSet,
        InputTimeline, InputWindowEvent, InputWindowId, KeyCode, KeyboardActionBinding,
        KeyboardContinuationError, KeyboardHold, SemanticActionCause, SemanticActionOutcome,
        SemanticActions, apply_keyboard_driver_input_at, apply_window_input_driver_event,
        capture_keyboard_holds, connect_input_endpoint, restore_keyboard_holds,
    },
};

const WINDOW: InputWindowId = InputWindowId::new(1);
const SEAT: InputSeatId = InputSeatId::PRIMARY;
const KEY: KeyCode = KeyCode::Character('d');

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
struct Motion {
    moving: bool,
    position: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct TickSample {
    tick: u64,
    commands: Vec<GameplayCommandEnvelope>,
    semantic: Vec<SemanticActionOutcome>,
    motion: Motion,
}

#[derive(Debug, Default, Resource)]
struct Trace {
    consumed: Vec<TickSample>,
    captured: Vec<TickSample>,
    pre_updates: usize,
}

#[derive(Debug, Default, Clone, Resource)]
struct ContextChanges(Arc<Mutex<Option<bool>>>);

fn binding() -> KeyboardActionBinding {
    KeyboardActionBinding {
        key: KEY,
        action: ActionId::new("move_right").unwrap(),
        context: ActionContext::gameplay(),
    }
}

fn draft(name: &str) -> GameplayCommandDraft {
    GameplayCommandDraft::new(GameplayCommandTypeId::new(name).unwrap())
}

fn sample_tick(
    time: &FixedTime,
    batch: &GameplayCommandBatch,
    semantic: Option<&SemanticActions>,
    motion: Motion,
) -> TickSample {
    assert_eq!(batch.active_tick().unwrap().get(), time.tick());
    TickSample {
        tick: time.tick(),
        commands: batch.commands().to_vec(),
        semantic: semantic.map_or_else(Vec::new, |actions| actions.as_slice().to_vec()),
        motion,
    }
}

fn consume_commands(
    time: Res<FixedTime>,
    batch: Res<GameplayCommandBatch>,
    semantic: Option<Res<SemanticActions>>,
    mut motion: ResMut<Motion>,
    mut trace: ResMut<Trace>,
) {
    for command in batch.commands() {
        match command.command_type().as_str() {
            "player.move" => motion.moving = true,
            "player.stop" => motion.moving = false,
            "player.marker" => {}
            other => panic!("unexpected command: {other}"),
        }
    }
    if motion.moving {
        motion.position += 1;
    }
    trace
        .consumed
        .push(sample_tick(&time, &batch, semantic.as_deref(), *motion));
}

fn capture_commands(
    time: Res<FixedTime>,
    batch: Res<GameplayCommandBatch>,
    queue: Res<GameplayCommandQueue>,
    semantic: Option<Res<SemanticActions>>,
    motion: Res<Motion>,
    mut trace: ResMut<Trace>,
) {
    assert_eq!(queue.stats().acknowledged_through_tick, time.tick() - 1);
    trace
        .captured
        .push(sample_tick(&time, &batch, semantic.as_deref(), *motion));
}

fn observe_pre_update(mut trace: ResMut<Trace>) {
    trace.pre_updates += 1;
}

fn apply_context_change(changes: Res<ContextChanges>, mut maps: ResMut<ActionMaps>) {
    let Some(enabled) = changes.0.lock().unwrap().take() else {
        return;
    };
    let map = maps.get_mut(SEAT).unwrap();
    if enabled {
        assert!(map.enable_context(&ActionContext::gameplay()).unwrap());
    } else {
        assert!(map.disable_context(&ActionContext::gameplay()).unwrap());
    }
}

fn install_gameplay(app: &mut App, moving: bool) {
    app.insert_resource(Motion {
        moving,
        position: 0,
    })
    .unwrap();
    app.init_resource::<Trace>().unwrap();
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    app.add_systems(CoreStage::PreUpdate, observe_pre_update)
        .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        consume_commands.in_set(GameplayCommandSet::Consume),
    )
    .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        capture_commands.in_set(GameplayCommandSet::Capture),
    )
    .unwrap();
}

fn driver<T>(
    runtime: &mut RuntimeInstance,
    operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> T,
) -> T {
    let outcome = runtime.with_driver_scope(operation).unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output
}

fn keyboard(runtime: &mut RuntimeInstance, input: ButtonDriverInput<KeyCode>) {
    driver(runtime, |scope| {
        apply_keyboard_driver_input_at(scope, WINDOW, SEAT, input)
    })
    .unwrap();
}

fn paused_frame(runtime: &mut RuntimeInstance) {
    let tick = runtime.world().resource::<FixedTime>().tick();
    let outcome = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(outcome.state(), RuntimeState::Paused);
    assert_eq!(outcome.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 0);
    assert_eq!(runtime.world().resource::<FixedTime>().tick(), tick);
    assert!(runtime.fault().is_none());
}

fn control(runtime: &mut RuntimeInstance, control: RuntimeControl, state: RuntimeState) {
    let RuntimeControlRequestResult::Accepted(ticket) = runtime.request_control(control) else {
        panic!("runtime control must be accepted");
    };
    let outcome = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(outcome.state(), state);
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 0);
    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Applied)
    );
    assert!(runtime.fault().is_none());
}

fn exact_tick(runtime: &mut RuntimeInstance) {
    let tick = runtime.world().resource::<FixedTime>().tick();
    let frame = runtime.world().resource::<RealTime>().frame;
    let pre_updates = runtime.world().resource::<Trace>().pre_updates;
    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("exact tick must be accepted");
    };
    let outcome = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(outcome.state(), RuntimeState::Paused);
    assert_eq!(outcome.frame_kind(), Some(RuntimeFrameKind::ExactFixedTick));
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 1);
    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(runtime.world().resource::<FixedTime>().tick(), tick + 1);
    assert_eq!(runtime.world().resource::<RealTime>().frame, frame + 1);
    assert_eq!(runtime.world().resource::<Trace>().pre_updates, pre_updates);
    assert!(runtime.fault().is_none());
}

fn input_runtime(moving: bool) -> RuntimeInstance {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    builder
        .bind(ActionBinding::key(binding().action, KEY))
        .unwrap();
    let map = ActionMap::new(
        builder.compile().unwrap().into_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap();
    let mut app = App::new();
    app.insert_resource(RuntimeTimeSettings::default().with_paused(true))
        .unwrap();
    app.insert_resource(
        InputTimeline::with_limits(InputLimits::new(64, 3, 1, 8).unwrap()).unwrap(),
    )
    .unwrap();
    app.insert_resource(ActionMaps::new(map)).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    install_gameplay(&mut app, moving);
    app.init_resource::<ContextChanges>().unwrap();
    app.add_systems(
        CoreStage::First,
        apply_context_change.before(InputSet::StageActions),
    )
    .unwrap();
    for (phase, command) in [
        (ActionPhase::Started, "player.move"),
        (ActionPhase::Released, "player.stop"),
    ] {
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                binding().action,
                phase,
                GameplayCommandTypeId::new(command).unwrap(),
            ))
            .unwrap();
    }
    let mut runtime = RuntimeInstance::start(app).unwrap();
    paused_frame(&mut runtime);
    driver(&mut runtime, |scope| {
        connect_input_endpoint(scope, WINDOW, SEAT)
    })
    .unwrap();
    paused_frame(&mut runtime);
    runtime
}

fn restore_movement(runtime: &mut RuntimeInstance) {
    let claim =
        claim_input_endpoint(runtime.world().resource::<InputTimeline>(), WINDOW, SEAT).unwrap();
    let admission = driver(runtime, |scope| {
        restore_keyboard_holds(
            scope,
            claim,
            vec![KeyboardHold {
                binding: binding(),
                held: true,
            }],
        )
    })
    .unwrap();
    assert_eq!(admission.admitted(), 1);
    paused_frame(runtime);
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEY)
    );
    assert!(runtime.world().resource::<SemanticActions>().pressed(
        SEAT,
        &binding().action,
        &binding().context,
    ));
    assert!(runtime.world().resource::<ActionOutcomes>().is_empty());
    assert!(
        runtime
            .world()
            .resource::<SemanticActions>()
            .as_slice()
            .is_empty()
    );
    assert_eq!(
        runtime
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        0
    );
    assert!(runtime.world().resource::<Trace>().consumed.is_empty());
    assert_eq!(
        capture_keyboard_holds(runtime.world(), WINDOW, SEAT, &[binding()]).unwrap(),
        [KeyboardHold {
            binding: binding(),
            held: true
        }],
    );
}

fn assert_acknowledged(runtime: &RuntimeInstance, commands: u64, ticks: u64) {
    let queue = runtime.world().resource::<GameplayCommandQueue>();
    let stats = queue.stats();
    assert_eq!(stats.accepted, commands);
    assert_eq!(stats.admitted, commands);
    assert_eq!(stats.acknowledged, commands);
    assert_eq!(stats.acknowledged_ticks, ticks);
    assert_eq!(stats.closed_through_tick, ticks);
    assert_eq!(stats.acknowledged_through_tick, ticks);
    assert_eq!(stats.rejected, 0);
    assert_eq!(stats.lifecycle_faults, 0);
    assert!(queue.is_idle());
    let batch = runtime.world().resource::<GameplayCommandBatch>();
    assert!(batch.is_empty());
    assert_eq!(batch.active_tick(), None);
    let trace = runtime.world().resource::<Trace>();
    assert_eq!(trace.consumed.len() as u64, ticks);
    assert_eq!(trace.consumed, trace.captured);
    assert!(runtime.fault().is_none());
}

fn assert_local_command(sample: &TickSample, command_type: &str, sequence: u64) {
    assert_eq!(sample.commands.len(), 1);
    let command = &sample.commands[0];
    assert_eq!(command.tick().get(), sample.tick);
    assert_eq!(command.command_type().as_str(), command_type);
    assert_eq!(command.source(), &GameplayCommandSource::LocalAction);
    assert_eq!(command.source_sequence().get(), sequence);
}

fn release_before_exact_tick(staged: bool, focus_loss: bool) {
    let mut runtime = input_runtime(true);
    restore_movement(&mut runtime);
    if focus_loss {
        let (_, released, _) = driver(&mut runtime, |scope| {
            apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::Focused(false))
        })
        .unwrap();
        assert_eq!(released, [KEY]);
    } else {
        keyboard(&mut runtime, ButtonDriverInput::Release(KEY));
    }
    if staged {
        paused_frame(&mut runtime);
        assert!(
            runtime
                .world()
                .resource::<InputTimeline>()
                .cursors()
                .oldest_deferred_action()
                .is_some()
        );
    }
    assert_eq!(
        capture_keyboard_holds(runtime.world(), WINDOW, SEAT, &[binding()]),
        Err(KeyboardContinuationError::PendingInput),
    );
    assert_eq!(
        runtime
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        0
    );
    exact_tick(&mut runtime);
    assert_acknowledged(&runtime, 1, 1);
    let sample = &runtime.world().resource::<Trace>().consumed[0];
    assert_local_command(sample, "player.stop", 1);
    assert_eq!(sample.motion, Motion::default());
    assert_eq!(sample.semantic.len(), 1);
    assert_eq!(sample.semantic[0].phase, ActionPhase::Released);
    assert_eq!(sample.semantic[0].cause, SemanticActionCause::PhysicalInput);
    assert!(sample.semantic[0].provenance.is_some());
    assert!(
        !runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEY)
    );
    assert!(
        runtime
            .world()
            .resource::<SemanticActions>()
            .as_slice()
            .is_empty()
    );
    paused_frame(&mut runtime);
    assert_eq!(
        capture_keyboard_holds(runtime.world(), WINDOW, SEAT, &[binding()]).unwrap(),
        [KeyboardHold {
            binding: binding(),
            held: false
        }],
    );
    exact_tick(&mut runtime);
    assert_acknowledged(&runtime, 1, 2);
    let sample = &runtime.world().resource::<Trace>().consumed[1];
    assert!(sample.commands.is_empty());
    assert!(sample.semantic.is_empty());
    assert_eq!(sample.motion, Motion::default());
    runtime.finish().unwrap();
}

#[test]
fn staged_release_and_focus_loss_stop_restored_motion_before_exact_tick() {
    for focus_loss in [false, true] {
        release_before_exact_tick(true, focus_loss);
    }
}

#[test]
fn unscanned_release_stops_restored_motion_before_exact_tick() {
    release_before_exact_tick(false, false);
}

#[test]
fn ordinary_press_lowers_once_and_keeps_outcomes_across_three_fixed_ticks() {
    let mut runtime = input_runtime(false);
    control(&mut runtime, RuntimeControl::Resume, RuntimeState::Running);
    keyboard(&mut runtime, ButtonDriverInput::Press(KEY));
    let pre_updates = runtime.world().resource::<Trace>().pre_updates;
    let outcome = runtime.drive(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();
    assert_eq!(outcome.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 3);
    assert_acknowledged(&runtime, 1, 3);
    let trace = runtime.world().resource::<Trace>();
    assert_eq!(trace.pre_updates, pre_updates + 1);
    assert_local_command(&trace.consumed[0], "player.move", 1);
    assert_eq!(trace.consumed[0].semantic.len(), 1);
    assert_eq!(trace.consumed[0].semantic[0].phase, ActionPhase::Started);
    for (index, sample) in trace.consumed.iter().enumerate() {
        assert_eq!(sample.tick, index as u64 + 1);
        assert_eq!(
            sample.motion,
            Motion {
                moving: true,
                position: sample.tick
            }
        );
        assert_eq!(sample.semantic, trace.consumed[0].semantic);
        if index > 0 {
            assert!(sample.commands.is_empty());
        }
    }
    assert!(
        runtime
            .world()
            .resource::<SemanticActions>()
            .as_slice()
            .is_empty()
    );
    runtime.finish().unwrap();
}

#[test]
fn late_fixed_press_uses_next_first_mapping_without_premature_staging() {
    fn admit_after_first_consume(time: Res<FixedTime>, mut timeline: ResMut<InputTimeline>) {
        if time.tick() == 1 {
            let admission = timeline
                .admit_keyboard(WINDOW, SEAT, ButtonDriverInput::Press(KEY))
                .unwrap();
            assert_eq!(admission.admitted(), 1);
        }
    }

    fn install_next_first_mapping(time: Res<FixedTime>, mut maps: ResMut<ActionMaps>) {
        if time.tick() != 2 {
            return;
        }
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .bind(ActionBinding::key(binding().action, KEY))
            .unwrap();
        maps.get_mut(SEAT)
            .unwrap()
            .replace_bindings(builder.compile().unwrap().into_bindings());
    }

    let mut app = App::new();
    app.insert_resource(ActionMaps::default()).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    install_gameplay(&mut app, false);
    for (phase, command) in [
        (ActionPhase::Started, "player.move"),
        (ActionPhase::Released, "player.stop"),
    ] {
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                binding().action,
                phase,
                GameplayCommandTypeId::new(command).unwrap(),
            ))
            .unwrap();
    }
    app.add_systems(
        CoreStage::FixedUpdate,
        admit_after_first_consume
            .in_set(GameplayCommandSet::Consume)
            .after(consume_commands),
    )
    .unwrap();
    app.add_systems(
        CoreStage::First,
        install_next_first_mapping.before(InputSet::StageActions),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    driver(&mut runtime, |scope| {
        connect_input_endpoint(scope, WINDOW, SEAT)
    })
    .unwrap();

    let outcome = runtime.drive(FixedTime::DEFAULT_TIMESTEP * 2).unwrap();
    assert_eq!(outcome.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 2);
    assert_acknowledged(&runtime, 0, 2);
    let trace = runtime.world().resource::<Trace>();
    assert_eq!(trace.pre_updates, 1);
    for sample in &trace.consumed {
        assert!(sample.commands.is_empty());
        assert!(sample.semantic.is_empty());
        assert_eq!(sample.motion, Motion::default());
    }
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEY)
    );
    let timeline = runtime.world().resource::<InputTimeline>();
    assert_eq!(timeline.retained_len(), 1);
    let press_sequence = timeline.entries().next().unwrap().sequence();
    assert!(
        timeline.cursors().action_scanned() < press_sequence,
        "catch-up tick 2 must leave the late press for the next First staging boundary"
    );
    assert_eq!(timeline.cursors().oldest_deferred_action(), None);

    let outcome = runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert_eq!(outcome.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 1);
    assert_acknowledged(&runtime, 1, 3);
    let trace = runtime.world().resource::<Trace>();
    assert_eq!(trace.pre_updates, 2);
    let sample = &trace.consumed[2];
    assert_local_command(sample, "player.move", 1);
    assert_eq!(sample.semantic.len(), 1);
    let action = &sample.semantic[0];
    assert_eq!(action.seat, SEAT);
    assert_eq!(action.action, binding().action);
    assert_eq!(action.context, binding().context);
    assert_eq!(action.phase, ActionPhase::Started);
    assert_eq!(action.cause, SemanticActionCause::PhysicalInput);
    assert_eq!(action.provenance.unwrap().sequence(), press_sequence);
    assert_eq!(
        sample.motion,
        Motion {
            moving: true,
            position: 1
        }
    );

    let outcome = runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 1);
    assert_acknowledged(&runtime, 1, 4);
    let sample = &runtime.world().resource::<Trace>().consumed[3];
    assert!(sample.commands.is_empty());
    assert!(sample.semantic.is_empty());
    assert_eq!(
        sample.motion,
        Motion {
            moving: true,
            position: 2
        }
    );
    runtime.finish().unwrap();
}

#[test]
fn context_cancellation_uses_the_original_release_command_before_exact_movement() {
    let mut runtime = input_runtime(true);
    restore_movement(&mut runtime);
    let changes = runtime.world().resource::<ContextChanges>().clone();
    for enabled in [false, true] {
        assert!(changes.0.lock().unwrap().replace(enabled).is_none());
        paused_frame(&mut runtime);
    }
    assert_eq!(
        runtime
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        0
    );
    exact_tick(&mut runtime);
    assert_acknowledged(&runtime, 1, 1);
    let sample = &runtime.world().resource::<Trace>().consumed[0];
    assert_local_command(sample, "player.stop", 1);
    assert_eq!(sample.motion, Motion::default());
    assert_eq!(sample.semantic.len(), 1);
    assert_eq!(sample.semantic[0].phase, ActionPhase::Released);
    assert_eq!(
        sample.semantic[0].cause,
        SemanticActionCause::ContextChanged
    );
    assert_eq!(sample.semantic[0].provenance, None);
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEY)
    );
    keyboard(&mut runtime, ButtonDriverInput::Release(KEY));
    exact_tick(&mut runtime);
    assert_acknowledged(&runtime, 1, 2);
    assert!(
        runtime.world().resource::<Trace>().consumed[1]
            .commands
            .is_empty()
    );
    keyboard(&mut runtime, ButtonDriverInput::Press(KEY));
    exact_tick(&mut runtime);
    assert_acknowledged(&runtime, 2, 3);
    let sample = &runtime.world().resource::<Trace>().consumed[2];
    assert_local_command(sample, "player.move", 2);
    assert_eq!(
        sample.motion,
        Motion {
            moving: true,
            position: 1
        }
    );
    runtime.finish().unwrap();
}

fn enqueue_after_first_consume(time: Res<FixedTime>, mut commands: ResMut<LocalGameplayCommands>) {
    if time.tick() == 1 {
        commands.push(draft("player.marker")).unwrap();
    }
}

#[test]
fn local_commands_drain_once_per_real_frame_and_exact_ticks_work_without_input() {
    let mut app = App::new();
    install_gameplay(&mut app, false);
    app.world_mut()
        .unwrap()
        .resource_mut::<LocalGameplayCommands>()
        .push(draft("player.marker"))
        .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        enqueue_after_first_consume
            .in_set(GameplayCommandSet::Consume)
            .after(consume_commands),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let outcome = runtime.drive(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 3);
    assert_acknowledged(&runtime, 1, 3);
    let trace = runtime.world().resource::<Trace>();
    assert_local_command(&trace.consumed[0], "player.marker", 1);
    assert!(trace.consumed[1].commands.is_empty());
    assert!(trace.consumed[2].commands.is_empty());
    assert_eq!(runtime.world().resource::<LocalGameplayCommands>().len(), 1);
    control(&mut runtime, RuntimeControl::Pause, RuntimeState::Paused);
    exact_tick(&mut runtime);
    assert_acknowledged(&runtime, 2, 4);
    assert_local_command(
        &runtime.world().resource::<Trace>().consumed[3],
        "player.marker",
        2,
    );
    assert!(
        runtime
            .world()
            .resource::<LocalGameplayCommands>()
            .is_empty()
    );
    runtime.finish().unwrap();
}
