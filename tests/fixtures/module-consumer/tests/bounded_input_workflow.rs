//! Bounded code-first Input through ordinary semantic commands and fixed gameplay effects.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use nara_engine::{
    app::{
        App, CoreStage, FixedTime, RuntimeControl, RuntimeControlRequestResult, RuntimeDriverScope,
        RuntimeInstance, RuntimeState,
    },
    core::{ByteLimit, Vec2},
    ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs},
    gameplay::{
        ActionCommandBinding, ActionCommandMap, GameplayCommandBatch, GameplayCommandPlugin,
        GameplayCommandSet, GameplayCommandTypeId,
    },
    input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBindings, ActionMapBuilder,
        ActionMapLimits, ActionPhase, ButtonDriverInput, ButtonInput, ButtonInputDriverError,
        InputEventOrigin, InputIngressError, InputLimits, InputPlugin, InputPointerId, InputSeatId,
        InputTimeline, InputWindowEvent, InputWindowId, KeyCode, MouseButton, PointerDriverInput,
        SemanticActionCause, SemanticActions, apply_keyboard_driver_input,
        apply_keyboard_driver_input_at, apply_mouse_driver_input_at, apply_pointer_driver_input_at,
        apply_window_input_driver_event, connect_input_endpoint, connect_input_pointer,
    },
};

const MOVE: &str = "consumer.bounded.move";
const MENU: &str = "consumer.bounded.menu";
const MOVE_START: &str = "consumer.bounded.move-start";
const MOVE_STOP: &str = "consumer.bounded.move-stop";
const MENU_START: &str = "consumer.bounded.menu-start";
const MENU_STOP: &str = "consumer.bounded.menu-stop";
const WINDOW: InputWindowId = InputWindowId::new(1);
const OTHER_WINDOW: InputWindowId = InputWindowId::new(2);
const SEAT: InputSeatId = InputSeatId::PRIMARY;
const POINTER: InputPointerId = InputPointerId::PRIMARY;
const KEY: KeyCode = KeyCode::Character('w');

fn menu() -> ActionContext {
    ActionContext::new("menu").unwrap()
}

fn bindings(key: KeyCode) -> ActionMapBindings {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    builder.declare_context(menu(), true).unwrap();
    builder
        .bind(ActionBinding::key(ActionId::new(MOVE).unwrap(), key))
        .unwrap();
    builder
        .bind(ActionBinding::mouse(
            ActionId::new(MOVE).unwrap(),
            MouseButton::Left,
        ))
        .unwrap();
    builder
        .bind(ActionBinding::key(ActionId::new(MENU).unwrap(), KeyCode::Space).with_context(menu()))
        .unwrap();
    builder.compile().unwrap().into_bindings()
}

#[derive(Clone, Copy, Debug)]
enum ConfigurationChange {
    DisableGameplay,
    EnableGameplay,
    Rebind,
    Restore,
}

#[derive(Resource)]
struct Configuration {
    pending: Arc<Mutex<Option<ConfigurationChange>>>,
    original: ActionMapBindings,
    alternate: ActionMapBindings,
}

fn apply_configuration(configuration: Res<Configuration>, mut map: ResMut<ActionMap>) {
    match configuration.pending.lock().unwrap().take() {
        Some(ConfigurationChange::DisableGameplay) => {
            assert!(map.disable_context(&ActionContext::gameplay()).unwrap());
        }
        Some(ConfigurationChange::EnableGameplay) => {
            assert!(map.enable_context(&ActionContext::gameplay()).unwrap());
        }
        Some(ConfigurationChange::Rebind) => map.replace_bindings(configuration.alternate.clone()),
        Some(ConfigurationChange::Restore) => map.replace_bindings(configuration.original.clone()),
        None => {}
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Resource)]
struct Effects {
    moving: bool,
    menu_open: bool,
    starts: u32,
    stops: u32,
    menu_starts: u32,
    menu_stops: u32,
    ticks: u32,
    distance: u32,
}

fn simulate(batch: Res<GameplayCommandBatch>, mut effects: ResMut<Effects>) {
    for command in batch.commands() {
        match command.command_type().as_str() {
            MOVE_START => {
                assert!(
                    !effects.moving,
                    "the command bridge must aggregate contributions"
                );
                effects.moving = true;
                effects.starts += 1;
            }
            MOVE_STOP => {
                assert!(effects.moving, "each semantic lifetime stops once");
                effects.moving = false;
                effects.stops += 1;
            }
            MENU_START => {
                assert!(!effects.menu_open);
                effects.menu_open = true;
                effects.menu_starts += 1;
            }
            MENU_STOP => {
                assert!(effects.menu_open);
                effects.menu_open = false;
                effects.menu_stops += 1;
            }
            unexpected => panic!("unexpected command {unexpected}"),
        }
    }
    effects.ticks += 1;
    effects.distance += u32::from(effects.moving);
}

#[derive(Debug, Default, Resource)]
struct ActionEvidence {
    platform_starts: u32,
    synthetic_starts: u32,
    platform_stops: u32,
    synthetic_stops: u32,
    context_cancellations: u32,
    mapping_cancellations: u32,
}

fn observe_actions(actions: Res<SemanticActions>, mut evidence: ResMut<ActionEvidence>) {
    for edge in actions
        .as_slice()
        .iter()
        .filter(|edge| edge.action.as_str() == MOVE)
    {
        match edge.cause {
            SemanticActionCause::PhysicalInput => {
                let provenance = edge.provenance.expect("admitted input retains its origin");
                assert!(provenance.sequence() > 0);
                assert!(provenance.press_sequence().is_some());
                match (edge.phase, provenance.origin()) {
                    (ActionPhase::Started, InputEventOrigin::Platform) => {
                        evidence.platform_starts += 1
                    }
                    (ActionPhase::Started, InputEventOrigin::Synthetic) => {
                        evidence.synthetic_starts += 1
                    }
                    (ActionPhase::Released, InputEventOrigin::Platform) => {
                        evidence.platform_stops += 1
                    }
                    (ActionPhase::Released, InputEventOrigin::Synthetic) => {
                        evidence.synthetic_stops += 1
                    }
                }
            }
            SemanticActionCause::ContextChanged => {
                assert_eq!(edge.phase, ActionPhase::Released);
                assert!(edge.provenance.is_none());
                evidence.context_cancellations += 1;
            }
            SemanticActionCause::MappingChanged => {
                assert_eq!(edge.phase, ActionPhase::Released);
                assert!(edge.provenance.is_none());
                evidence.mapping_cancellations += 1;
            }
        }
    }
}

fn start(
    limits: InputLimits,
) -> (
    RuntimeInstance,
    Arc<Mutex<Option<ConfigurationChange>>>,
    ActionMapBindings,
) {
    let original = bindings(KEY);
    let pending = Arc::new(Mutex::new(None));
    let mut app = App::new();
    app.insert_resource(InputTimeline::with_limits(limits).unwrap())
        .unwrap();
    app.add_plugin(InputPlugin).unwrap();
    app.insert_resource(
        ActionMap::new(
            original.clone(),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    app.insert_resource(Configuration {
        pending: Arc::clone(&pending),
        original: original.clone(),
        alternate: bindings(KeyCode::ArrowUp),
    })
    .unwrap();
    app.init_resource::<Effects>().unwrap();
    app.init_resource::<ActionEvidence>().unwrap();
    for (action, context, phase, command) in [
        (
            MOVE,
            ActionContext::gameplay(),
            ActionPhase::Started,
            MOVE_START,
        ),
        (
            MOVE,
            ActionContext::gameplay(),
            ActionPhase::Released,
            MOVE_STOP,
        ),
        (MENU, menu(), ActionPhase::Started, MENU_START),
        (MENU, menu(), ActionPhase::Released, MENU_STOP),
    ] {
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(
                ActionCommandBinding::new(
                    ActionId::new(action).unwrap(),
                    phase,
                    GameplayCommandTypeId::new(command).unwrap(),
                )
                .with_context(context),
            )
            .unwrap();
    }
    app.add_systems(
        CoreStage::FixedUpdate,
        simulate.in_set(GameplayCommandSet::Consume),
    )
    .unwrap();
    app.add_systems(CoreStage::Update, observe_actions).unwrap();
    app.add_systems(CoreStage::Last, apply_configuration)
        .unwrap();
    (RuntimeInstance::start(app).unwrap(), pending, original)
}

fn input<T>(
    runtime: &mut RuntimeInstance,
    operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> Result<T, ButtonInputDriverError>,
) -> Result<T, ButtonInputDriverError> {
    let outcome = runtime.with_driver_scope(operation).unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output
}

fn effects(runtime: &RuntimeInstance) -> Effects {
    *runtime.world().resource::<Effects>()
}

fn movement_is_held(runtime: &RuntimeInstance) -> bool {
    runtime
        .world()
        .resource::<SemanticActions>()
        .pressed(&ActionId::new(MOVE).unwrap(), &ActionContext::gameplay())
}

fn control(runtime: &mut RuntimeInstance, command: RuntimeControl) {
    assert!(matches!(
        runtime.request_control(command),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let outcome = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        outcome.frame().unwrap().status.fixed_steps,
        u32::from(command == RuntimeControl::StepFixedTick)
    );
}

fn configure(
    runtime: &mut RuntimeInstance,
    pending: &Mutex<Option<ConfigurationChange>>,
    change: ConfigurationChange,
) {
    assert!(pending.lock().unwrap().replace(change).is_none());
    // Last applies the change; the next First observes its revision even while simulation is paused.
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
}

#[test]
fn bounded_input_aggregates_rejects_and_reuses_retired_capacity() {
    let (mut runtime, _, original) = start(InputLimits::new(32, 3, 3, 3).unwrap());
    input(&mut runtime, |scope| {
        for window in [WINDOW, OTHER_WINDOW] {
            connect_input_endpoint(scope, window, SEAT)?;
            connect_input_pointer(scope, window, SEAT, POINTER)?;
            apply_pointer_driver_input_at(
                scope,
                window,
                SEAT,
                POINTER,
                PointerDriverInput::Moved(Vec2::new(2.0, 3.0)),
            )?;
        }
        apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::Focused(true))?;
        apply_keyboard_driver_input_at(scope, WINDOW, SEAT, ButtonDriverInput::Press(KEY))
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    input(&mut runtime, |scope| {
        apply_mouse_driver_input_at(
            scope,
            OTHER_WINDOW,
            SEAT,
            POINTER,
            ButtonDriverInput::Press(MouseButton::Left),
        )
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    input(&mut runtime, |scope| {
        apply_keyboard_driver_input(scope, ButtonDriverInput::Press(KEY))
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    assert_eq!(effects(&runtime), Effects::default());
    let frame = runtime.drive(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();
    assert_eq!(frame.frame().unwrap().status.fixed_steps, 3);
    assert_eq!(
        (effects(&runtime).starts, effects(&runtime).distance),
        (1, 3)
    );

    let cursors = runtime.world().resource::<InputTimeline>().cursors();
    assert_eq!(
        input(&mut runtime, |scope| connect_input_endpoint(
            scope,
            InputWindowId::new(3),
            SEAT
        )),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::EndpointCapacityExceeded { limit: 3 }
        ))
    );
    assert_eq!(
        input(&mut runtime, |scope| connect_input_pointer(
            scope,
            WINDOW,
            SEAT,
            InputPointerId::new(1)
        )),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::PointerCapacityExceeded { limit: 3 }
        ))
    );
    assert_eq!(
        input(&mut runtime, |scope| {
            apply_keyboard_driver_input_at(
                scope,
                WINDOW,
                SEAT,
                ButtonDriverInput::Press(KeyCode::Space),
            )
        }),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::MemberCapacityExceeded { limit: 3 }
        ))
    );
    assert_eq!(
        input(&mut runtime, |scope| apply_keyboard_driver_input(
            scope,
            ButtonDriverInput::Press(KeyCode::Space)
        )),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::MemberCapacityExceeded { limit: 3 }
        ))
    );
    assert_eq!(
        runtime.world().resource::<InputTimeline>().cursors(),
        cursors
    );
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEY)
    );
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left)
    );
    input(&mut runtime, |scope| {
        apply_keyboard_driver_input_at(
            scope,
            WINDOW,
            SEAT,
            ButtonDriverInput::Release(KeyCode::Space),
        )
    })
    .unwrap();
    input(&mut runtime, |scope| {
        apply_keyboard_driver_input(scope, ButtonDriverInput::Release(KeyCode::Space))
    })
    .unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert_eq!(
        (
            effects(&runtime).starts,
            effects(&runtime).stops,
            effects(&runtime).menu_starts
        ),
        (1, 0, 0)
    );

    input(&mut runtime, |scope| {
        apply_window_input_driver_event(
            scope,
            OTHER_WINDOW,
            SEAT,
            InputWindowEvent::Focused(true),
        )?;
        apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::Focused(false))
    })
    .unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!((effects(&runtime).starts, effects(&runtime).stops), (1, 0));

    control(&mut runtime, RuntimeControl::Pause);
    input(&mut runtime, |scope| {
        apply_window_input_driver_event(scope, OTHER_WINDOW, SEAT, InputWindowEvent::SurfaceRetired)
    })
    .unwrap();
    assert_eq!(
        input(&mut runtime, |scope| connect_input_endpoint(
            scope,
            OTHER_WINDOW,
            SEAT
        )),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::EndpointRetiring {
                window: OTHER_WINDOW,
                seat: SEAT
            }
        ))
    );
    let repeated = input(&mut runtime, |scope| {
        apply_window_input_driver_event(scope, OTHER_WINDOW, SEAT, InputWindowEvent::SurfaceRetired)
    })
    .unwrap();
    assert!(repeated.0.is_ignored());
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<InputTimeline>().retained_len() > 0);
    assert_eq!(
        input(&mut runtime, |scope| connect_input_endpoint(
            scope,
            OTHER_WINDOW,
            SEAT
        )),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::EndpointRetiring {
                window: OTHER_WINDOW,
                seat: SEAT
            }
        ))
    );
    control(&mut runtime, RuntimeControl::Resume);
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    input(&mut runtime, |scope| {
        connect_input_endpoint(scope, OTHER_WINDOW, SEAT)?;
        connect_input_pointer(scope, OTHER_WINDOW, SEAT, POINTER)?;
        apply_mouse_driver_input_at(
            scope,
            OTHER_WINDOW,
            SEAT,
            POINTER,
            ButtonDriverInput::Press(MouseButton::Left),
        )
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    input(&mut runtime, |scope| {
        apply_keyboard_driver_input(scope, ButtonDriverInput::Release(KEY))
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert!(
        !runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEY)
    );
    input(&mut runtime, |scope| {
        apply_pointer_driver_input_at(scope, OTHER_WINDOW, SEAT, POINTER, PointerDriverInput::Left)
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    assert!(
        effects(&runtime).moving,
        "zero-tick frames queue the stop without simulating it"
    );
    runtime.drive(FixedTime::DEFAULT_TIMESTEP * 2).unwrap();
    assert_eq!(
        (
            effects(&runtime).starts,
            effects(&runtime).stops,
            effects(&runtime).distance
        ),
        (1, 1, 5)
    );
    assert!(!effects(&runtime).moving);
    assert_eq!(
        runtime.world().resource::<ActionEvidence>().platform_starts,
        1
    );
    assert_eq!(
        runtime.world().resource::<ActionEvidence>().platform_stops,
        1
    );
    assert!(
        runtime
            .world()
            .resource::<ActionMap>()
            .bindings_handle()
            .shares_owner_with(&original)
    );
    runtime.finish().unwrap();
}

#[test]
fn bounded_input_configuration_lifetimes_and_exact_steps_apply_commands_once() {
    let (mut runtime, pending, original) = start(InputLimits::new(48, 2, 2, 3).unwrap());
    input(&mut runtime, |scope| {
        connect_input_endpoint(scope, WINDOW, SEAT)?;
        connect_input_pointer(scope, WINDOW, SEAT, POINTER)?;
        apply_keyboard_driver_input_at(scope, WINDOW, SEAT, ButtonDriverInput::Press(KEY))?;
        apply_mouse_driver_input_at(
            scope,
            WINDOW,
            SEAT,
            POINTER,
            ButtonDriverInput::Press(MouseButton::Left),
        )?;
        apply_keyboard_driver_input(scope, ButtonDriverInput::Press(KeyCode::Space))?;
        Ok(())
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(effects(&runtime), Effects::default());
    control(&mut runtime, RuntimeControl::Pause);
    control(&mut runtime, RuntimeControl::StepFixedTick);
    control(&mut runtime, RuntimeControl::StepFixedTick);
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(
        (
            effects(&runtime).starts,
            effects(&runtime).menu_starts,
            effects(&runtime).distance
        ),
        (1, 1, 2)
    );
    let paused = effects(&runtime);
    runtime.drive(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();
    assert_eq!(effects(&runtime), paused);

    configure(&mut runtime, &pending, ConfigurationChange::DisableGameplay);
    configure(&mut runtime, &pending, ConfigurationChange::EnableGameplay);
    assert_eq!(
        input(&mut runtime, |scope| apply_keyboard_driver_input_at(
            scope,
            WINDOW,
            SEAT,
            ButtonDriverInput::Press(KeyCode::ArrowUp)
        )),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::MemberCapacityExceeded { limit: 3 }
        ))
    );
    control(&mut runtime, RuntimeControl::Resume);
    assert!(!movement_is_held(&runtime));
    assert!(
        runtime
            .world()
            .resource::<SemanticActions>()
            .pressed(&ActionId::new(MENU).unwrap(), &menu())
    );
    assert_eq!(
        runtime
            .world()
            .resource::<ActionEvidence>()
            .context_cancellations,
        1
    );
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEY)
    );
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left)
    );
    control(&mut runtime, RuntimeControl::Pause);
    control(&mut runtime, RuntimeControl::StepFixedTick);
    control(&mut runtime, RuntimeControl::StepFixedTick);
    assert_eq!(
        (
            effects(&runtime).stops,
            effects(&runtime).menu_stops,
            effects(&runtime).distance
        ),
        (1, 0, 2)
    );

    input(&mut runtime, |scope| {
        apply_keyboard_driver_input_at(scope, WINDOW, SEAT, ButtonDriverInput::Release(KEY))?;
        apply_mouse_driver_input_at(
            scope,
            WINDOW,
            SEAT,
            POINTER,
            ButtonDriverInput::Release(MouseButton::Left),
        )
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    control(&mut runtime, RuntimeControl::Resume);
    assert!(!movement_is_held(&runtime));
    input(&mut runtime, |scope| {
        apply_keyboard_driver_input_at(scope, WINDOW, SEAT, ButtonDriverInput::Press(KEY))
    })
    .unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();
    assert_eq!(
        (
            effects(&runtime).starts,
            effects(&runtime).stops,
            effects(&runtime).distance
        ),
        (2, 1, 5)
    );
    input(&mut runtime, |scope| {
        apply_mouse_driver_input_at(
            scope,
            WINDOW,
            SEAT,
            POINTER,
            ButtonDriverInput::Press(MouseButton::Left),
        )
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    control(&mut runtime, RuntimeControl::Pause);
    configure(&mut runtime, &pending, ConfigurationChange::Rebind);
    assert!(
        !runtime
            .world()
            .resource::<ActionMap>()
            .bindings_handle()
            .shares_owner_with(&original)
    );
    configure(&mut runtime, &pending, ConfigurationChange::Restore);
    assert!(
        runtime
            .world()
            .resource::<ActionMap>()
            .bindings_handle()
            .shares_owner_with(&original)
    );
    control(&mut runtime, RuntimeControl::Resume);
    assert!(!movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<ActionEvidence>()
            .mapping_cancellations,
        1
    );
    control(&mut runtime, RuntimeControl::Pause);
    control(&mut runtime, RuntimeControl::StepFixedTick);
    control(&mut runtime, RuntimeControl::StepFixedTick);
    assert_eq!(
        (
            effects(&runtime).stops,
            effects(&runtime).menu_stops,
            effects(&runtime).distance
        ),
        (2, 1, 5)
    );
    input(&mut runtime, |scope| {
        apply_keyboard_driver_input_at(scope, WINDOW, SEAT, ButtonDriverInput::Release(KEY))?;
        apply_mouse_driver_input_at(
            scope,
            WINDOW,
            SEAT,
            POINTER,
            ButtonDriverInput::Release(MouseButton::Left),
        )?;
        apply_keyboard_driver_input(scope, ButtonDriverInput::Release(KeyCode::Space))
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    control(&mut runtime, RuntimeControl::Resume);
    assert!(!movement_is_held(&runtime));
    control(&mut runtime, RuntimeControl::Pause);
    input(&mut runtime, |scope| {
        apply_keyboard_driver_input(scope, ButtonDriverInput::Press(KEY))
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let before_step = effects(&runtime);
    control(&mut runtime, RuntimeControl::StepFixedTick);
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEY)
    );
    assert!(
        !movement_is_held(&runtime),
        "exact stepping cannot resolve paused semantic input early"
    );
    assert_eq!(
        (
            effects(&runtime).starts,
            effects(&runtime).stops,
            effects(&runtime).distance
        ),
        (before_step.starts, before_step.stops, before_step.distance)
    );
    control(&mut runtime, RuntimeControl::Resume);
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    input(&mut runtime, |scope| {
        apply_keyboard_driver_input(scope, ButtonDriverInput::Release(KEY))
    })
    .unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP * 2).unwrap();
    let final_effects = effects(&runtime);
    assert_eq!(
        (
            final_effects.starts,
            final_effects.stops,
            final_effects.distance
        ),
        (3, 3, 6)
    );
    assert_eq!(
        (final_effects.menu_starts, final_effects.menu_stops),
        (1, 1)
    );
    assert!(!final_effects.moving);
    let evidence = runtime.world().resource::<ActionEvidence>();
    assert_eq!(
        (
            evidence.platform_starts,
            evidence.synthetic_starts,
            evidence.synthetic_stops
        ),
        (2, 1, 1)
    );
    assert_eq!(
        (
            evidence.context_cancellations,
            evidence.mapping_cancellations
        ),
        (1, 1)
    );
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    assert!(
        runtime
            .world()
            .resource::<ActionMap>()
            .bindings_handle()
            .shares_owner_with(&original)
    );
    runtime.finish().unwrap();
}

#[test]
fn bounded_input_reuses_keyboard_and_pointer_capacity_across_historical_identities() {
    const IDENTITIES: u32 = 4_097;
    let (mut runtime, _, original) = start(InputLimits::new(16, 2, 2, 2).unwrap());
    for identity in 1..=IDENTITIES {
        let window = InputWindowId::new(u64::from(identity) + 10);
        let pointer = InputPointerId::new(identity);
        input(&mut runtime, |scope| {
            connect_input_endpoint(scope, window, SEAT)?;
            connect_input_pointer(scope, window, SEAT, pointer)?;
            apply_pointer_driver_input_at(
                scope,
                window,
                SEAT,
                pointer,
                PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
            )?;
            apply_keyboard_driver_input_at(scope, window, SEAT, ButtonDriverInput::Press(KEY))?;
            apply_mouse_driver_input_at(
                scope,
                window,
                SEAT,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
        })
        .unwrap();
        runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
        assert!(movement_is_held(&runtime));
        assert_eq!(
            runtime.world().resource::<InputTimeline>().retained_len(),
            0
        );
        input(&mut runtime, |scope| {
            apply_window_input_driver_event(scope, window, SEAT, InputWindowEvent::SurfaceRetired)
        })
        .unwrap();
        runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
        assert!(!movement_is_held(&runtime));
        assert!(
            !runtime
                .world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KEY)
        );
        assert!(
            !runtime
                .world()
                .resource::<ButtonInput<MouseButton>>()
                .pressed(MouseButton::Left)
        );
        assert_eq!(
            runtime.world().resource::<InputTimeline>().retained_len(),
            0
        );
    }
    let final_effects = effects(&runtime);
    assert_eq!(
        (
            final_effects.starts,
            final_effects.stops,
            final_effects.distance
        ),
        (IDENTITIES, IDENTITIES, IDENTITIES)
    );
    assert_eq!(final_effects.ticks, IDENTITIES * 2);
    let evidence = runtime.world().resource::<ActionEvidence>();
    assert_eq!(
        (evidence.platform_starts, evidence.platform_stops),
        (IDENTITIES, IDENTITIES)
    );
    assert!(
        runtime
            .world()
            .resource::<ActionMap>()
            .bindings_handle()
            .shares_owner_with(&original)
    );
    runtime.finish().unwrap();
}
