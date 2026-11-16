use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nara_app::{
    App, CoreStage, FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    RealTime, RuntimeControl, RuntimeControlRequestResult, RuntimeFaultKind, RuntimeFaultReporter,
    RuntimeFrameKind, RuntimeInstance, RuntimeState, RuntimeTimeSettings, ScheduleGraphError,
};
use nara_core::ByteLimit;
use nara_ecs::{
    Commands, FromWorld, Local, Res, ResMut, Resource, World,
    bevy_ecs::error::BevyError,
    schedule::{IntoScheduleConfigs, ScheduleLabel},
};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapLimits,
    ActionMaps, ActionValue, AxisDirection, ButtonDriverInput, InputPlugin, KeyCode,
    SemanticActions, apply_keyboard_driver_input,
};

use crate::{
    GameplayCommandBatch, GameplayCommandDraft, GameplayCommandEnvelope,
    GameplayCommandIngressSource, GameplayCommandPayload, GameplayCommandPlugin,
    GameplayCommandQueue, GameplayCommandSet, GameplayCommandSource, GameplayCommandSourceSequence,
    GameplayCommandSubmission, GameplayCommandTick, GameplayCommandTypeId, GameplayCommandValue,
    LocalGameplayCommands, add_local_gameplay_command_systems,
};

#[derive(Default, Resource)]
struct Trace {
    collections: Vec<(u64, u64)>,
    batches: Vec<(u64, Vec<GameplayCommandEnvelope>)>,
    order: Vec<&'static str>,
    later_effects: usize,
    captures: usize,
}

fn draft(name: &str, value: u64) -> GameplayCommandDraft {
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert(
            "value",
            GameplayCommandValue::I64(i64::try_from(value).unwrap()),
        )
        .unwrap();
    GameplayCommandDraft::new(GameplayCommandTypeId::new(name).unwrap()).with_payload(payload)
}

fn capture_batch(
    fixed: Res<FixedTime>,
    batch: Res<GameplayCommandBatch>,
    mut trace: ResMut<Trace>,
) {
    trace
        .batches
        .push((fixed.tick(), batch.commands().to_vec()));
    trace.captures += 1;
}

fn configure_capture(app: &mut App, paused: bool) {
    app.insert_resource(RuntimeTimeSettings::default().with_paused(paused))
        .unwrap();
    app.init_resource::<Trace>().unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        capture_batch.in_set(GameplayCommandSet::Capture),
    )
    .unwrap();
}

fn control(runtime: &mut RuntimeInstance, request: RuntimeControl) {
    assert!(matches!(
        runtime.request_control(request),
        RuntimeControlRequestResult::Accepted(_)
    ));
}

fn exact_tick(runtime: &mut RuntimeInstance) {
    control(runtime, RuntimeControl::StepFixedTick);
    let outcome = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(outcome.state(), RuntimeState::Paused);
    assert_eq!(outcome.frame_kind(), Some(RuntimeFrameKind::ExactFixedTick));
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 1);
}

fn keyboard(runtime: &mut RuntimeInstance, input: ButtonDriverInput<KeyCode>) {
    let outcome = runtime
        .with_driver_scope(|scope| apply_keyboard_driver_input(scope, input))
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output.unwrap();
}

fn collect_axis(actions: Res<SemanticActions>, mut commands: ResMut<LocalGameplayCommands>) {
    for outcome in actions.as_slice() {
        let ActionValue::Axis(value) = outcome.value else {
            panic!("the movement fixture only declares an axis");
        };
        let mut payload = GameplayCommandPayload::new();
        payload
            .insert("value", GameplayCommandValue::F64(f64::from(value.get())))
            .unwrap();
        commands
            .push(
                GameplayCommandDraft::new(GameplayCommandTypeId::new("move.axis").unwrap())
                    .with_payload(payload),
            )
            .unwrap();
    }
}

#[test]
fn exact_step_collects_staged_axis_changes_before_admission_and_does_not_replay_on_resume() {
    for register_before_plugin in [false, true] {
        let mut app = App::new();
        app.add_plugin(InputPlugin).unwrap();
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        builder
            .bind(ActionBinding::key_axis(
                ActionId::new("move").unwrap(),
                KeyCode::Character('d'),
                AxisDirection::Positive,
            ))
            .unwrap();
        app.insert_resource(ActionMaps::new(
            ActionMap::new(
                builder.compile().unwrap().into_bindings(),
                ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
            )
            .unwrap(),
        ))
        .unwrap();
        if register_before_plugin {
            add_local_gameplay_command_systems(&mut app, collect_axis).unwrap();
        }
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        if !register_before_plugin {
            add_local_gameplay_command_systems(&mut app, collect_axis).unwrap();
        }
        configure_capture(&mut app, true);
        let mut runtime = RuntimeInstance::start(app).unwrap();

        keyboard(
            &mut runtime,
            ButtonDriverInput::Press(KeyCode::Character('d')),
        );
        runtime.drive(Duration::ZERO).unwrap();
        exact_tick(&mut runtime);
        keyboard(
            &mut runtime,
            ButtonDriverInput::Release(KeyCode::Character('d')),
        );
        runtime.drive(Duration::ZERO).unwrap();
        exact_tick(&mut runtime);
        control(&mut runtime, RuntimeControl::Resume);
        runtime.drive(Duration::ZERO).unwrap();
        runtime.drive(FixedTime::DEFAULT_TIMESTEP * 2).unwrap();

        let trace = runtime.world().resource::<Trace>();
        assert_eq!(trace.batches.len(), 4);
        for (index, value) in [1.0, 0.0].into_iter().enumerate() {
            let (tick, commands) = &trace.batches[index];
            assert_eq!(*tick, index as u64 + 1);
            assert_eq!(commands.len(), 1);
            assert_eq!(commands[0].source(), &GameplayCommandSource::LocalAction);
            assert_eq!(commands[0].source_sequence().get(), index as u64 + 1);
            assert_eq!(
                commands[0].payload().get("value"),
                Some(&GameplayCommandValue::F64(value))
            );
        }
        assert!(trace.batches[2..].iter().all(|(_, batch)| batch.is_empty()));
        assert_eq!(
            runtime
                .world()
                .resource::<GameplayCommandQueue>()
                .stats()
                .pending_commands,
            0
        );
        runtime.finish().unwrap();
    }
}

#[derive(Default)]
struct NonCloneCounter(u64);

fn counted_producer(
    time: Res<RealTime>,
    mut counter: Local<NonCloneCounter>,
    mut trace: ResMut<Trace>,
    mut commands: ResMut<LocalGameplayCommands>,
) {
    counter.0 += 1;
    trace.collections.push((time.frame, counter.0));
    commands.push(draft("collection.count", counter.0)).unwrap();
}

#[test]
fn no_input_zero_and_multi_tick_frames_share_local_state_with_exact_steps_once_per_frame() {
    for register_before_plugin in [false, true] {
        let mut app = App::new();
        if register_before_plugin {
            add_local_gameplay_command_systems(&mut app, counted_producer).unwrap();
        }
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        if !register_before_plugin {
            add_local_gameplay_command_systems(&mut app, counted_producer).unwrap();
        }
        configure_capture(&mut app, false);
        let mut runtime = RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        runtime.drive(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();
        control(&mut runtime, RuntimeControl::Pause);
        runtime.drive(Duration::ZERO).unwrap();
        exact_tick(&mut runtime);
        exact_tick(&mut runtime);
        control(&mut runtime, RuntimeControl::Resume);
        runtime.drive(Duration::ZERO).unwrap();
        runtime.drive(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();

        let trace = runtime.world().resource::<Trace>();
        assert_eq!(
            trace
                .collections
                .iter()
                .map(|(_, count)| *count)
                .collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6]
        );
        assert!(
            trace
                .collections
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0)
        );
        let actual = trace
            .batches
            .iter()
            .map(|(tick, batch)| {
                let values = batch
                    .iter()
                    .map(|command| match command.payload().get("value") {
                        Some(GameplayCommandValue::I64(value)) => *value,
                        other => panic!("unexpected collection payload: {other:?}"),
                    })
                    .collect::<Vec<_>>();
                (*tick, values)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            [
                (1, vec![1, 2]),
                (2, vec![]),
                (3, vec![]),
                (4, vec![3]),
                (5, vec![4]),
                (6, vec![5, 6]),
                (7, vec![]),
                (8, vec![]),
            ]
        );
        runtime.finish().unwrap();
    }
}

fn first_deferred(mut commands: Commands) {
    commands.queue(|world: &mut World| {
        world.resource_mut::<Trace>().order.push("first");
        world
            .resource_mut::<LocalGameplayCommands>()
            .push(draft("ordered.first", 1))
            .unwrap();
    });
}

fn second_deferred(trace: Res<Trace>, mut commands: Commands) {
    assert_eq!(trace.order, ["first"]);
    commands.queue(|world: &mut World| {
        world.resource_mut::<Trace>().order.push("second");
        world
            .resource_mut::<LocalGameplayCommands>()
            .push(draft("ordered.second", 2))
            .unwrap();
    });
}

#[test]
fn separate_registrations_preserve_ordered_deferred_effects_before_command_lowering() {
    for exact in [false, true] {
        let mut app = App::new();
        add_local_gameplay_command_systems(&mut app, first_deferred.before(second_deferred))
            .unwrap();
        app.add_plugin(GameplayCommandPlugin::default()).unwrap();
        add_local_gameplay_command_systems(&mut app, second_deferred).unwrap();
        configure_capture(&mut app, exact);
        let mut runtime = RuntimeInstance::start(app).unwrap();
        if exact {
            exact_tick(&mut runtime);
        } else {
            runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
        }
        let trace = runtime.world().resource::<Trace>();
        assert_eq!(trace.order, ["first", "second"]);
        assert_eq!(
            trace.batches[0]
                .1
                .iter()
                .map(|command| command.command_type().as_str())
                .collect::<Vec<_>>(),
            ["ordered.first", "ordered.second"]
        );
        assert!(
            runtime
                .world()
                .resource::<LocalGameplayCommands>()
                .is_empty()
        );
        runtime.finish().unwrap();
    }
}

#[derive(Resource)]
struct FailureMode {
    deferred: bool,
    replace_reporter: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("injected local collection failure")]
struct CollectionFailure;

fn fail_collection(world: &mut World) -> Result<(), BevyError> {
    let mode = world.resource::<FailureMode>();
    let replace_reporter = mode.replace_reporter;
    if mode.deferred {
        world
            .commands()
            .queue(move |world: &mut World| -> Result<(), BevyError> {
                if replace_reporter {
                    world.insert_resource(RuntimeFaultReporter::default());
                }
                Err(BevyError::error(CollectionFailure))
            });
        Ok(())
    } else {
        if replace_reporter {
            world.insert_resource(RuntimeFaultReporter::default());
        }
        Err(BevyError::error(CollectionFailure))
    }
}

fn later_collection(mut commands: Commands) {
    commands.queue(|world: &mut World| {
        world.resource_mut::<Trace>().later_effects += 1;
    });
}

#[test]
fn collection_system_and_deferred_errors_retain_original_fault_custody_and_pending_commands() {
    for exact in [false, true] {
        for deferred in [false, true] {
            for replace_reporter in [false, true] {
                let mut app = App::new();
                app.add_plugin(GameplayCommandPlugin::default()).unwrap();
                configure_capture(&mut app, exact);
                app.insert_resource(FailureMode {
                    deferred,
                    replace_reporter,
                })
                .unwrap();
                add_local_gameplay_command_systems(
                    &mut app,
                    (fail_collection, later_collection).chain(),
                )
                .unwrap();
                app.world_mut()
                    .unwrap()
                    .resource_mut::<GameplayCommandQueue>()
                    .submit(GameplayCommandSubmission::new(
                        GameplayCommandTick::new(1).unwrap(),
                        GameplayCommandIngressSource::test("retained").unwrap(),
                        GameplayCommandSourceSequence::new(1).unwrap(),
                        draft("retained.command", 1),
                    ))
                    .unwrap();
                let mut runtime = RuntimeInstance::start(app).unwrap();
                let original = runtime.fault_reporter();
                if exact {
                    control(&mut runtime, RuntimeControl::StepFixedTick);
                }
                let failure = runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap_err();
                assert_eq!(
                    failure.fault().kind(),
                    if deferred {
                        RuntimeFaultKind::Command
                    } else {
                        RuntimeFaultKind::System
                    }
                );
                assert_eq!(original.fault().as_ref(), Some(failure.fault()));
                assert_eq!(runtime.state(), RuntimeState::Faulted);
                let trace = runtime.world().resource::<Trace>();
                assert_eq!(trace.later_effects, 1);
                assert_eq!(trace.captures, 0);
                let stats = runtime.world().resource::<GameplayCommandQueue>().stats();
                assert_eq!(stats.retained_commands, 1);
                assert_eq!(stats.acknowledged_through_tick, 0);
                assert_eq!(runtime.drive(Duration::ZERO).unwrap_err(), failure);
                assert_eq!(runtime.world().resource::<Trace>().later_effects, 1);
                runtime.finish().unwrap();
            }
        }
    }
}

#[test]
fn collection_graph_errors_are_rejected_during_app_sealing() {
    fn first() {}
    fn second() {}

    let mut app = App::new();
    add_local_gameplay_command_systems(&mut app, first.before(second)).unwrap();
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    add_local_gameplay_command_systems(&mut app, second.before(first)).unwrap();
    let error = app.seal().unwrap_err();
    assert!(matches!(
        error.error(),
        PluginError::ScheduleGraph(ScheduleGraphError::BuildFailed { schedule, .. })
            if *schedule == nara_app::__private::FrameIntent.intern()
    ));
}

#[test]
fn empty_collection_without_input_completes_an_exact_tick() {
    let mut app = App::new();
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    configure_capture(&mut app, true);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    exact_tick(&mut runtime);
    let trace = runtime.world().resource::<Trace>();
    assert_eq!(trace.batches.len(), 1);
    assert!(trace.batches[0].1.is_empty());
    runtime.finish().unwrap();
}

fn late_cycle_first() {}

fn late_cycle_second() {}

struct LateCollectionCycle;

impl Plugin for LateCollectionCycle {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.gameplay.test-late-collection-cycle"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        add_local_gameplay_command_systems(app, late_cycle_first.before(late_cycle_second))?;
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        add_local_gameplay_command_systems(app, late_cycle_second.before(late_cycle_first))?;
        Ok(())
    }
}

#[test]
fn later_plugin_finish_collection_cycle_is_rejected_by_final_app_sealing() {
    let startup_ran = Arc::new(AtomicBool::new(false));
    let startup_probe = Arc::clone(&startup_ran);
    let mut app = App::new();
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    app.add_plugin(LateCollectionCycle).unwrap();
    app.add_systems(nara_app::StartupStage::Runtime, move || {
        startup_probe.store(true, Ordering::SeqCst);
    })
    .unwrap();
    let error = app.seal().unwrap_err();
    assert!(matches!(
        error.error(),
        PluginError::ScheduleGraph(ScheduleGraphError::BuildFailed { schedule, .. })
            if *schedule == nara_app::__private::FrameIntent.intern()
    ));
    assert!(
        !startup_ran.load(Ordering::SeqCst),
        "a late Finish cycle must reject admission before any Startup system runs"
    );
}

#[derive(Default, Resource)]
struct LateConfiguration(u64);

struct LateConfiguredLocal(u64);

impl FromWorld for LateConfiguredLocal {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource::<LateConfiguration>().0)
    }
}

fn collect_late_configuration(
    configuration: Local<LateConfiguredLocal>,
    mut commands: ResMut<LocalGameplayCommands>,
) {
    commands
        .push(draft("late.configuration", configuration.0))
        .unwrap();
}

struct ConfigureCollectionOnFinish;

impl Plugin for ConfigureCollectionOnFinish {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.gameplay.test-late-local-configuration"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<LateConfiguration>()?;
        add_local_gameplay_command_systems(app, collect_late_configuration)?;
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(LateConfiguration(41))?;
        Ok(())
    }
}

#[test]
fn collection_local_initialization_observes_later_plugin_finish_configuration() {
    let mut app = App::new();
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    app.add_plugin(ConfigureCollectionOnFinish).unwrap();
    configure_capture(&mut app, true);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    exact_tick(&mut runtime);
    let trace = runtime.world().resource::<Trace>();
    assert_eq!(trace.batches.len(), 1);
    assert_eq!(trace.batches[0].1.len(), 1);
    assert_eq!(
        trace.batches[0].1[0].payload().get("value"),
        Some(&GameplayCommandValue::I64(41))
    );
    runtime.finish().unwrap();
}
