#![cfg(feature = "runtime-input")]

use std::time::Duration;

use nara::{
    app::{
        App, CoreStage, FixedTime, FixedUpdateSet, RealTime, RuntimeControl,
        RuntimeControlRequestResult, RuntimeControlStatus, RuntimeDriverScope, RuntimeFrameKind,
        RuntimeInstance, RuntimeState, RuntimeTimeSettings,
    },
    ecs::{Res, ResMut, Resource, World, schedule::IntoScheduleConfigs},
    input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapLimits,
        ActionMaps, ActionOutcomes, ActionPhase, ActionState, ButtonDriverInput, ButtonInput,
        InputLimits, InputPlugin, InputSeatId, InputSet, InputTimeline, InputWindowEvent,
        InputWindowId, KeyCode, KeyboardActionBinding, KeyboardContinuationError, KeyboardHold,
        SemanticActionCause, SemanticActionOutcome, SemanticActions,
        apply_keyboard_driver_input_at, apply_window_input_driver_event, capture_keyboard_holds,
        connect_input_endpoint, restore_keyboard_holds,
    },
};

const WINDOW: InputWindowId = InputWindowId::new(1);
const SEAT: InputSeatId = InputSeatId::PRIMARY;
const KEY: KeyCode = KeyCode::Character('d');
const MOVE_RIGHT: &str = "move_right";

#[derive(Debug, Default, Resource)]
struct Position(u64);

#[derive(Debug)]
struct TickSample {
    tick: u64,
    moving: bool,
    position: u64,
    semantic: Vec<SemanticActionOutcome>,
}

#[derive(Debug, Default, Resource)]
struct Trace {
    ticks: Vec<TickSample>,
    pre_updates: usize,
}

fn binding() -> KeyboardActionBinding {
    KeyboardActionBinding {
        key: KEY,
        action: ActionId::new(MOVE_RIGHT).unwrap(),
        context: ActionContext::gameplay(),
    }
}

fn simulate_movement(
    time: Res<FixedTime>,
    actions: Res<ActionState>,
    mut position: ResMut<Position>,
    mut trace: ResMut<Trace>,
) {
    let moving = actions.button(MOVE_RIGHT);
    position.0 += u64::from(moving);
    trace.ticks.push(TickSample {
        tick: time.tick(),
        moving,
        position: position.0,
        semantic: actions.as_slice().to_vec(),
    });
}

fn observe_pre_update(mut trace: ResMut<Trace>) {
    trace.pre_updates += 1;
}

fn install_movement(app: &mut App) {
    app.init_resource::<Position>().unwrap();
    app.init_resource::<Trace>().unwrap();
    app.add_systems(CoreStage::PreUpdate, observe_pre_update)
        .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        simulate_movement.in_set(FixedUpdateSet::Simulate),
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
    let trace = runtime.world().resource::<Trace>();
    let (pre_updates, samples) = (trace.pre_updates, trace.ticks.len());
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
    let trace = runtime.world().resource::<Trace>();
    assert_eq!(trace.pre_updates, pre_updates);
    assert_eq!(trace.ticks.len(), samples + 1);
    assert!(runtime.fault().is_none());
}

fn input_runtime() -> RuntimeInstance {
    let map = ActionMap::from_bindings([ActionBinding::key(binding().action, KEY)]).unwrap();
    let mut app = App::new();
    app.insert_resource(RuntimeTimeSettings::default().with_paused(true))
        .unwrap();
    app.insert_resource(
        InputTimeline::with_limits(InputLimits::new(64, 3, 1, 8).unwrap()).unwrap(),
    )
    .unwrap();
    app.insert_resource(ActionMaps::new(map)).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    install_movement(&mut app);
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
    let claim = runtime
        .world()
        .resource::<InputTimeline>()
        .claim_endpoint(WINDOW, SEAT)
        .unwrap();
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
    assert!(runtime.world().resource::<ActionState>().button(MOVE_RIGHT));
    assert!(runtime.world().resource::<ActionOutcomes>().is_empty());
    assert!(
        runtime
            .world()
            .resource::<SemanticActions>()
            .as_slice()
            .is_empty()
    );
    assert!(runtime.world().resource::<Trace>().ticks.is_empty());
    assert_eq!(
        capture_keyboard_holds(runtime.world(), WINDOW, SEAT, &[binding()]).unwrap(),
        [KeyboardHold {
            binding: binding(),
            held: true
        }],
    );
}

fn assert_action_edge(
    sample: &TickSample,
    phase: ActionPhase,
    cause: SemanticActionCause,
) -> &SemanticActionOutcome {
    let [edge] = sample.semantic.as_slice() else {
        panic!("expected one semantic edge, got {:?}", sample.semantic);
    };
    assert_eq!(edge.seat, SEAT);
    assert_eq!(edge.action.as_str(), MOVE_RIGHT);
    assert_eq!(edge.context, ActionContext::gameplay());
    assert_eq!(edge.phase, phase);
    assert_eq!(edge.cause, cause);
    edge
}

fn release_before_exact_tick(staged: bool, focus_loss: bool) {
    let mut runtime = input_runtime();
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
    assert!(runtime.world().resource::<ActionState>().button(MOVE_RIGHT));
    assert!(runtime.world().resource::<Trace>().ticks.is_empty());
    exact_tick(&mut runtime);
    let sample = &runtime.world().resource::<Trace>().ticks[0];
    let edge = assert_action_edge(
        sample,
        ActionPhase::Released,
        SemanticActionCause::PhysicalInput,
    );
    assert!(edge.provenance.is_some());
    assert!(!sample.moving);
    assert_eq!(sample.position, 0);
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
    let trace = runtime.world().resource::<Trace>();
    assert_eq!(trace.ticks.len(), 2);
    let sample = &trace.ticks[1];
    assert!(sample.semantic.is_empty());
    assert!(!sample.moving);
    assert_eq!(sample.position, 0);
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
fn ordinary_press_keeps_one_semantic_edge_across_three_fixed_ticks() {
    let mut runtime = input_runtime();
    control(&mut runtime, RuntimeControl::Resume, RuntimeState::Running);
    keyboard(&mut runtime, ButtonDriverInput::Press(KEY));
    let pre_updates = runtime.world().resource::<Trace>().pre_updates;
    let outcome = runtime.drive(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();
    assert_eq!(outcome.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 3);
    let trace = runtime.world().resource::<Trace>();
    assert_eq!(trace.pre_updates, pre_updates + 1);
    assert_eq!(trace.ticks.len(), 3);
    assert_action_edge(
        &trace.ticks[0],
        ActionPhase::Started,
        SemanticActionCause::PhysicalInput,
    );
    for (index, sample) in trace.ticks.iter().enumerate() {
        assert_eq!(sample.tick, index as u64 + 1);
        assert!(sample.moving);
        assert_eq!(sample.position, sample.tick);
        assert_eq!(sample.semantic, trace.ticks[0].semantic);
    }
    assert!(
        runtime
            .world()
            .resource::<SemanticActions>()
            .as_slice()
            .is_empty()
    );
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let sample = &runtime.world().resource::<Trace>().ticks[3];
    assert!(sample.semantic.is_empty());
    assert!(sample.moving);
    assert_eq!(sample.position, 4);
    runtime.finish().unwrap();
}

#[test]
fn late_fixed_press_uses_next_first_mapping_without_premature_staging() {
    fn admit_after_first_tick(time: Res<FixedTime>, mut timeline: ResMut<InputTimeline>) {
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
    install_movement(&mut app);
    app.add_systems(
        CoreStage::FixedUpdate,
        admit_after_first_tick
            .in_set(FixedUpdateSet::Simulate)
            .after(simulate_movement),
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
    let trace = runtime.world().resource::<Trace>();
    assert_eq!(trace.pre_updates, 1);
    assert_eq!(trace.ticks.len(), 2);
    for sample in &trace.ticks {
        assert!(sample.semantic.is_empty());
        assert!(!sample.moving);
        assert_eq!(sample.position, 0);
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
    let trace = runtime.world().resource::<Trace>();
    assert_eq!(trace.pre_updates, 2);
    let sample = &trace.ticks[2];
    let action = assert_action_edge(
        sample,
        ActionPhase::Started,
        SemanticActionCause::PhysicalInput,
    );
    assert_eq!(action.provenance.unwrap().sequence(), press_sequence);
    assert!(sample.moving);
    assert_eq!(sample.position, 1);

    let outcome = runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 1);
    let sample = &runtime.world().resource::<Trace>().ticks[3];
    assert!(sample.semantic.is_empty());
    assert!(sample.moving);
    assert_eq!(sample.position, 2);
    runtime.finish().unwrap();
}

#[test]
fn context_cancellation_releases_restored_motion_before_exact_tick() {
    let mut runtime = input_runtime();
    restore_movement(&mut runtime);
    for enabled in [false, true] {
        let outcome = runtime
            .apply_command(move |world: &mut World| {
                let mut maps = world.resource_mut::<ActionMaps>();
                let map = maps.get_mut(SEAT).unwrap();
                if enabled {
                    map.enable_context(&ActionContext::gameplay())
                } else {
                    map.disable_context(&ActionContext::gameplay())
                }
            })
            .unwrap();
        assert!(outcome.output.unwrap());
        assert!(outcome.post_operation_fault.is_none());
        paused_frame(&mut runtime);
    }
    assert!(runtime.world().resource::<Trace>().ticks.is_empty());
    exact_tick(&mut runtime);
    let sample = &runtime.world().resource::<Trace>().ticks[0];
    let edge = assert_action_edge(
        sample,
        ActionPhase::Released,
        SemanticActionCause::ContextChanged,
    );
    assert_eq!(edge.provenance, None);
    assert!(!sample.moving);
    assert_eq!(sample.position, 0);
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEY)
    );
    keyboard(&mut runtime, ButtonDriverInput::Release(KEY));
    exact_tick(&mut runtime);
    let sample = &runtime.world().resource::<Trace>().ticks[1];
    assert!(sample.semantic.is_empty());
    assert!(!sample.moving);
    assert_eq!(sample.position, 0);
    keyboard(&mut runtime, ButtonDriverInput::Press(KEY));
    exact_tick(&mut runtime);
    let sample = &runtime.world().resource::<Trace>().ticks[2];
    assert_action_edge(
        sample,
        ActionPhase::Started,
        SemanticActionCause::PhysicalInput,
    );
    assert!(sample.moving);
    assert_eq!(sample.position, 1);
    runtime.finish().unwrap();
}
