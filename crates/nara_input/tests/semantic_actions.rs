use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use nara_app::{App, CoreStage, RuntimeControl, RuntimeControlRequestResult, RuntimeInstance};
use nara_core::ByteLimit;
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_input::__private::InputSet;
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBindings, ActionMapBuilder,
    ActionMapLimits, ActionOutcome, ActionOutcomes, ActionPhase, ButtonDriverInput, ButtonInput,
    InputBinding, InputSeatId, InputTimeline, InputWindowEvent, InputWindowId, KeyCode,
    MouseButton, SemanticActionCause, SemanticActionOutcome, SemanticActions,
    apply_keyboard_driver_input, apply_keyboard_driver_input_at, apply_window_input_driver_event,
};

#[derive(Debug, Default, Resource)]
struct CapturedActionOutcomes(Vec<ActionOutcome>);

fn capture_action_outcomes(
    outcomes: Res<ActionOutcomes>,
    mut captured: ResMut<CapturedActionOutcomes>,
) {
    captured.0.extend_from_slice(outcomes.as_slice());
}

#[derive(Debug, Default, Resource)]
struct CapturedSemanticActions(Vec<SemanticActionOutcome>);

fn capture_semantic_actions(
    actions: Res<SemanticActions>,
    mut captured: ResMut<CapturedSemanticActions>,
) {
    captured.0.extend_from_slice(actions.as_slice());
}

#[derive(Debug)]
enum MappingChange {
    Disable,
    Enable,
    Rebind(KeyCode),
    SyntheticPress(InputBinding),
    SyntheticRelease(InputBinding),
    SaveAndRebind(KeyCode),
    RestoreBindings,
}

#[derive(Debug, Default, Clone, Resource)]
struct MappingChanges(
    Arc<Mutex<Vec<MappingChange>>>,
    Arc<Mutex<Option<ActionMapBindings>>>,
);

fn movement_bindings(key: KeyCode) -> ActionMapBindings {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    builder
        .bind(ActionBinding::key(ActionId::new("move").unwrap(), key))
        .unwrap();
    builder.compile().unwrap().into_bindings()
}

fn apply_mapping_changes(
    changes: Res<MappingChanges>,
    mut map: ResMut<ActionMap>,
    mut keyboard: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    for change in changes.0.lock().unwrap().drain(..) {
        match change {
            MappingChange::Disable => {
                map.disable_context(ActionContext::gameplay()).unwrap();
            }
            MappingChange::Enable => {
                map.enable_context(&ActionContext::gameplay()).unwrap();
            }
            MappingChange::Rebind(key) => {
                map.replace_bindings(movement_bindings(key));
            }
            MappingChange::SyntheticPress(input) => match input {
                InputBinding::Key(key) => {
                    keyboard.press(key).unwrap();
                }
                InputBinding::Mouse(button) => {
                    mouse.press(button).unwrap();
                }
            },
            MappingChange::SyntheticRelease(input) => match input {
                InputBinding::Key(key) => {
                    keyboard.release(key).unwrap();
                }
                InputBinding::Mouse(button) => {
                    mouse.release(button).unwrap();
                }
            },
            MappingChange::SaveAndRebind(key) => {
                let saved = map.bindings_handle();
                map.replace_bindings(movement_bindings(key));
                *changes.1.lock().unwrap() = Some(saved);
            }
            MappingChange::RestoreBindings => {
                let bindings = changes.1.lock().unwrap().take().unwrap();
                map.replace_bindings(bindings);
            }
        }
    }
}

fn semantic_runtime() -> (RuntimeInstance, MappingChanges) {
    let mut app = App::new();
    let changes = MappingChanges::default();
    app.add_plugin(nara_input::InputPlugin).unwrap();
    app.insert_resource(changes.clone()).unwrap();
    app.insert_resource(CapturedSemanticActions::default())
        .unwrap();
    app.insert_resource(CapturedActionOutcomes::default())
        .unwrap();
    app.add_systems(
        CoreStage::First,
        apply_mapping_changes.before(InputSet::StageActions),
    )
    .unwrap();
    app.add_systems(
        CoreStage::Update,
        (capture_semantic_actions, capture_action_outcomes),
    )
    .unwrap();
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    let menu = ActionContext::new("menu").unwrap();
    builder.declare_context(menu.clone(), true).unwrap();
    let action = ActionId::new("move").unwrap();
    for binding in [
        ActionBinding::key(action.clone(), KeyCode::Character('w')),
        ActionBinding::key(action.clone(), KeyCode::ArrowUp),
        ActionBinding::mouse(action, MouseButton::Left),
        ActionBinding::key(ActionId::new("menu").unwrap(), KeyCode::Space).with_context(menu),
    ] {
        builder.bind(binding).unwrap();
    }
    app.insert_resource(
        ActionMap::new(
            builder.compile().unwrap().into_bindings(),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    (RuntimeInstance::start(app).unwrap(), changes)
}

fn key_edge(runtime: &mut RuntimeInstance, edge: ButtonDriverInput<KeyCode>) {
    runtime
        .with_driver_scope(|scope| apply_keyboard_driver_input(scope, edge))
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
}

fn movement_is_held(runtime: &RuntimeInstance) -> bool {
    runtime
        .world()
        .resource::<SemanticActions>()
        .pressed(&ActionId::new("move").unwrap(), &ActionContext::gameplay())
}

#[test]
fn semantic_synthetic_paused_presses_keep_their_staged_lifetime() {
    let key = InputBinding::Key(KeyCode::Character('w'));
    for rebind in [false, true] {
        let (mut runtime, changes) = semantic_runtime();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        changes.0.lock().unwrap().extend([
            MappingChange::SyntheticPress(key),
            MappingChange::SyntheticPress(InputBinding::Key(KeyCode::Space)),
        ]);
        runtime.drive(Duration::ZERO).unwrap();
        if rebind {
            changes
                .0
                .lock()
                .unwrap()
                .push(MappingChange::Rebind(KeyCode::Character('w')));
        } else {
            changes
                .0
                .lock()
                .unwrap()
                .extend([MappingChange::Disable, MappingChange::Enable]);
        }
        runtime.drive(Duration::ZERO).unwrap();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Resume),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert!(
            !movement_is_held(&runtime),
            "a synthetic press cannot borrow the resume configuration's lifetime"
        );
        let semantic = &runtime.world().resource::<CapturedSemanticActions>().0;
        assert!(
            semantic
                .iter()
                .all(|outcome| outcome.action.as_str() != "move")
        );
        if !rebind {
            assert_eq!(semantic.len(), 1);
            assert_eq!(
                semantic[0].action.as_str(),
                "menu",
                "a different context keeps its staged press"
            );
        }
        assert!(
            runtime
                .world()
                .resource::<CapturedActionOutcomes>()
                .0
                .iter()
                .any(|outcome| outcome.action.as_str() == "move"
                    && outcome.phase == ActionPhase::Started),
            "raw binding observations remain available"
        );
        changes
            .0
            .lock()
            .unwrap()
            .push(MappingChange::SyntheticRelease(key));
        runtime.drive(Duration::ZERO).unwrap();
        assert!(!movement_is_held(&runtime));
        changes
            .0
            .lock()
            .unwrap()
            .push(MappingChange::SyntheticPress(key));
        runtime.drive(Duration::ZERO).unwrap();
        assert!(movement_is_held(&runtime));
        runtime.finish().unwrap();
    }
}

#[test]
fn semantic_synthetic_paused_release_and_fresh_press_keep_only_the_new_lifetime() {
    for input in [
        InputBinding::Key(KeyCode::Character('w')),
        InputBinding::Mouse(MouseButton::Left),
    ] {
        let (mut runtime, changes) = semantic_runtime();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        changes
            .0
            .lock()
            .unwrap()
            .push(MappingChange::SyntheticPress(input));
        runtime.drive(Duration::ZERO).unwrap();
        changes
            .0
            .lock()
            .unwrap()
            .extend([MappingChange::Disable, MappingChange::Enable]);
        runtime.drive(Duration::ZERO).unwrap();
        changes.0.lock().unwrap().extend([
            MappingChange::SyntheticRelease(input),
            MappingChange::SyntheticPress(input),
        ]);
        runtime.drive(Duration::ZERO).unwrap();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Resume),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert!(movement_is_held(&runtime));
        let semantic = &runtime.world().resource::<CapturedSemanticActions>().0;
        assert_eq!(
            semantic.len(),
            1,
            "an expired press/release pair cannot start and end a new semantic lifetime"
        );
        assert_eq!(semantic[0].phase, ActionPhase::Started);
        assert_eq!(
            runtime.world().resource::<CapturedActionOutcomes>().0.len(),
            3
        );
        runtime.finish().unwrap();
    }
}

#[test]
fn semantic_observed_context_exit_survives_reenabling_the_context() {
    observed_configuration_restore(false);
}

#[test]
fn semantic_observed_rebinding_survives_restoring_the_original_bindings() {
    observed_configuration_restore(true);
}

fn observed_configuration_restore(rebind: bool) {
    let (mut runtime, changes) = semantic_runtime();
    let input = InputBinding::Key(KeyCode::Character('w'));
    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::SyntheticPress(input));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    changes.0.lock().unwrap().push(if rebind {
        MappingChange::SaveAndRebind(KeyCode::Character('w'))
    } else {
        MappingChange::Disable
    });
    runtime.drive(Duration::ZERO).unwrap();
    changes.0.lock().unwrap().push(if rebind {
        MappingChange::RestoreBindings
    } else {
        MappingChange::Enable
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        !movement_is_held(&runtime),
        "restoring old configuration cannot erase an observed lifetime exit"
    );
    let semantic = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(semantic.len(), 2);
    assert_eq!(semantic[1].phase, ActionPhase::Released);
    assert_eq!(
        semantic[1].cause,
        if rebind {
            SemanticActionCause::MappingChanged
        } else {
            SemanticActionCause::ContextChanged
        }
    );
    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::SyntheticRelease(input));
    runtime.drive(Duration::ZERO).unwrap();
    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::SyntheticPress(input));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        3
    );
    runtime.finish().unwrap();
}

#[test]
fn semantic_multiple_bindings_release_only_the_final_contribution() {
    let (mut runtime, _) = semantic_runtime();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    key_edge(&mut runtime, ButtonDriverInput::Press(KeyCode::ArrowUp));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        1
    );
    assert_eq!(
        runtime.world().resource::<CapturedActionOutcomes>().0.len(),
        2
    );

    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        1
    );

    key_edge(&mut runtime, ButtonDriverInput::Release(KeyCode::ArrowUp));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(
        captured
            .iter()
            .map(|outcome| outcome.phase)
            .collect::<Vec<_>>(),
        [ActionPhase::Started, ActionPhase::Released]
    );
    assert_eq!(captured[1].cause, SemanticActionCause::PhysicalInput);
    assert!(captured[1].provenance.unwrap().sequence().is_some());
    assert!(
        runtime
            .world()
            .resource::<SemanticActions>()
            .as_slice()
            .is_empty(),
        "cleanup clears outcomes while retaining held state"
    );
    runtime.finish().unwrap();
}

#[test]
fn semantic_context_exit_is_terminal_once_and_restoration_requires_fresh_press() {
    let (mut runtime, changes) = semantic_runtime();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    key_edge(&mut runtime, ButtonDriverInput::Press(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();

    changes.0.lock().unwrap().push(MappingChange::Disable);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    assert!(runtime.world().resource::<SemanticActions>().pressed(
        &ActionId::new("menu").unwrap(),
        &ActionContext::new("menu").unwrap()
    ));
    let ended = &runtime.world().resource::<CapturedSemanticActions>().0[2];
    assert_eq!(ended.phase, ActionPhase::Released);
    assert_eq!(ended.cause, SemanticActionCause::ContextChanged);
    assert_eq!(ended.provenance, None);

    changes.0.lock().unwrap().push(MappingChange::Disable);
    runtime.drive(Duration::ZERO).unwrap();
    changes.0.lock().unwrap().push(MappingChange::Enable);
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        3,
        "repeated disable, restore, and a cancelled physical release cannot duplicate the terminal edge"
    );

    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    changes
        .0
        .lock()
        .unwrap()
        .extend([MappingChange::Disable, MappingChange::Enable]);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        !movement_is_held(&runtime),
        "a context lifetime cannot survive disable/enable between resolution passes"
    );
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(captured.len(), 5);
    assert_eq!(captured[4].cause, SemanticActionCause::ContextChanged);
    runtime.finish().unwrap();
}

#[test]
fn semantic_paused_mapping_changes_do_not_resurrect_staged_presses() {
    let (mut runtime, changes) = semantic_runtime();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();

    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::Rebind(KeyCode::ArrowUp));
    key_edge(&mut runtime, ButtonDriverInput::Press(KeyCode::ArrowUp));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        runtime
            .world()
            .resource::<InputTimeline>()
            .cursors()
            .oldest_deferred_action()
            .is_some()
    );
    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::Rebind(KeyCode::Character('w')));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();

    assert!(!movement_is_held(&runtime));
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(
        captured.len(),
        2,
        "the old mapping ends once; an obsolete deferred press never restarts it"
    );
    assert_eq!(captured[1].cause, SemanticActionCause::MappingChanged);
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    key_edge(&mut runtime, ButtonDriverInput::Release(KeyCode::ArrowUp));
    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        2
    );
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        3
    );
    runtime.finish().unwrap();
}

#[test]
fn semantic_paused_context_restoration_rejects_its_old_press_and_preserves_other_contexts() {
    let (mut runtime, changes) = semantic_runtime();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    key_edge(&mut runtime, ButtonDriverInput::Press(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();
    changes
        .0
        .lock()
        .unwrap()
        .extend([MappingChange::Disable, MappingChange::Enable]);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();

    assert!(!movement_is_held(&runtime));
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].action.as_str(), "menu");
    assert!(runtime.world().resource::<SemanticActions>().pressed(
        &ActionId::new("menu").unwrap(),
        &ActionContext::new("menu").unwrap()
    ));
    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        2
    );
    runtime.finish().unwrap();
}

#[test]
fn semantic_same_binding_held_by_two_endpoints_survives_one_endpoint_focus_loss() {
    let (mut runtime, _) = semantic_runtime();
    for (window, seat) in [
        (InputWindowId::new(1), InputSeatId::PRIMARY),
        (InputWindowId::new(2), InputSeatId::new(1)),
    ] {
        runtime
            .with_driver_scope(|scope| {
                apply_window_input_driver_event(
                    scope,
                    window,
                    seat,
                    InputWindowEvent::Focused(true),
                )?;
                apply_keyboard_driver_input_at(
                    scope,
                    window,
                    seat,
                    ButtonDriverInput::Press(KeyCode::Character('w')),
                )?;
                Ok::<(), nara_input::ButtonInputDriverError>(())
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .unwrap();
    }
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        1
    );
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(false),
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                InputWindowId::new(2),
                InputSeatId::new(1),
                InputWindowEvent::Focused(false),
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        2
    );
    runtime.finish().unwrap();
}
