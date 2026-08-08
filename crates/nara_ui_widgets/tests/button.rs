use std::sync::Arc;

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiActionId, UiActivationSource, UiAuthoringSnapshotBuilder, UiBrush, UiButtonPhase,
    UiColorRole, UiCompiler, UiEnvironment, UiFrameJournal, UiInputEvent, UiInteractionSession,
    UiKey, UiLayer, UiLocalKey, UiNavigation, UiPoint, UiPointerButton, UiPointerId, UiRoot,
    UiRootKey, UiRouteStamp, UiSeatId, UiStyle, UiSurfaceId, UiTheme, UiVisual,
};
use nara_ui_widgets::{
    UiButton, UiButtonState, UiButtonVisualState, UiIntent, UiIntentBatch, UiIntentBatchError,
    UiWidgetError,
};

fn fixture() -> (UiButton, UiFrameJournal, UiRouteStamp, UiInteractionSession) {
    let root_key = UiRootKey::runtime("widget-test", "root").unwrap();
    let local_key = UiLocalKey::runtime("widget-test", "button").unwrap();
    let key = UiKey::scoped(root_key.clone(), local_key).unwrap();
    let button = UiButton::new(key, "Retry", UiActionId::new("retry").unwrap()).unwrap();
    let root = UiRoot::new(root_key.clone(), UiLayer::Base, UiStyle::fill()).with_child(
        button
            .authored_node(
                &root_key,
                UiStyle::absolute(20.0, 20.0, 100.0, 44.0),
                UiVisual::default(),
                UiBrush::Role(UiColorRole::PrimaryText),
                18.0,
            )
            .unwrap(),
    );
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root)
        .build()
        .unwrap();
    let surface = UiSurfaceId::new(5).unwrap();
    let environment = UiEnvironment::new(surface, 200.0, 120.0, 1.0, 1, 1, 1, 1).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    let semantic = frame.semantics().first().unwrap();
    assert_eq!(semantic.key(), button.key());
    assert_eq!(semantic.name(), "Retry");
    assert_eq!(semantic.actions(), &[UiActionId::new("retry").unwrap()]);
    let mut journal = UiFrameJournal::default();
    journal.submit(Arc::clone(&frame), 9).unwrap();
    (
        button,
        journal,
        UiRouteStamp::new(surface, 9),
        UiInteractionSession::default(),
    )
}

fn pointer_button(phase: UiButtonPhase, point: UiPoint) -> UiInputEvent {
    UiInputEvent::PointerButton {
        seat: UiSeatId::PRIMARY,
        pointer: UiPointerId::PRIMARY,
        button: UiPointerButton::Primary,
        phase,
        position: Some(point),
    }
}

#[test]
fn pointer_sequence_is_controlled_and_activates_only_on_release_inside() {
    let (button, journal, route, mut session) = fixture();
    let mut state = UiButtonState::default();
    let mut intents = UiIntentBatch::default();

    for input in [
        UiInputEvent::PointerMoved {
            seat: UiSeatId::PRIMARY,
            pointer: UiPointerId::PRIMARY,
            position: UiPoint::new(30.0, 30.0),
        },
        pointer_button(UiButtonPhase::Pressed, UiPoint::new(30.0, 30.0)),
    ] {
        let dispatch = session.dispatch(&journal, route, input).unwrap();
        state
            .reduce_batch(&button, dispatch.consumed().unwrap(), &mut intents)
            .unwrap();
    }
    assert!(state.hovered());
    assert!(state.pressed());
    assert!(state.focused());
    assert_eq!(state.visual_state(&button), UiButtonVisualState::Pressed);
    assert!(intents.intents().is_empty());

    let moved_outside = session
        .dispatch(
            &journal,
            route,
            UiInputEvent::PointerMoved {
                seat: UiSeatId::PRIMARY,
                pointer: UiPointerId::PRIMARY,
                position: UiPoint::new(180.0, 100.0),
            },
        )
        .unwrap();
    state
        .reduce_batch(&button, moved_outside.consumed().unwrap(), &mut intents)
        .unwrap();
    let released_outside = session
        .dispatch(
            &journal,
            route,
            pointer_button(UiButtonPhase::Released, UiPoint::new(180.0, 100.0)),
        )
        .unwrap();
    state
        .reduce_batch(&button, released_outside.consumed().unwrap(), &mut intents)
        .unwrap();
    assert!(!state.hovered());
    assert!(!state.pressed());
    assert!(intents.intents().is_empty());

    for input in [
        pointer_button(UiButtonPhase::Pressed, UiPoint::new(30.0, 30.0)),
        pointer_button(UiButtonPhase::Released, UiPoint::new(30.0, 30.0)),
    ] {
        let dispatch = session.dispatch(&journal, route, input).unwrap();
        state
            .reduce_batch(&button, dispatch.consumed().unwrap(), &mut intents)
            .unwrap();
    }
    assert_eq!(
        intents.intents(),
        &[UiIntent::Activate {
            target: button.key().clone(),
            action: UiActionId::new("retry").unwrap(),
            source: UiActivationSource::Pointer,
        }]
    );
}

#[test]
fn keyboard_gamepad_and_accessibility_share_the_typed_intent_path() {
    let (button, journal, route, mut session) = fixture();
    let mut state = UiButtonState::default();
    let mut intents = UiIntentBatch::default();
    let focus = session
        .dispatch(
            &journal,
            route,
            UiInputEvent::Navigate {
                seat: UiSeatId::PRIMARY,
                direction: UiNavigation::Next,
            },
        )
        .unwrap();
    state
        .reduce_batch(&button, focus.consumed().unwrap(), &mut intents)
        .unwrap();
    assert_eq!(state.visual_state(&button), UiButtonVisualState::Focused);

    for input in [
        UiInputEvent::ActivateFocused {
            seat: UiSeatId::PRIMARY,
            source: UiActivationSource::Keyboard,
        },
        UiInputEvent::ActivateFocused {
            seat: UiSeatId::PRIMARY,
            source: UiActivationSource::Gamepad,
        },
        UiInputEvent::SemanticAction {
            seat: UiSeatId::PRIMARY,
            target: button.key().clone(),
            action: button.action().clone(),
        },
    ] {
        let dispatch = session.dispatch(&journal, route, input).unwrap();
        state
            .reduce_batch(&button, dispatch.consumed().unwrap(), &mut intents)
            .unwrap();
    }
    let sources = intents
        .intents()
        .iter()
        .map(|intent| match intent {
            UiIntent::Activate { source, .. } => *source,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        sources,
        [
            UiActivationSource::Keyboard,
            UiActivationSource::Gamepad,
            UiActivationSource::Accessibility,
        ]
    );
}

#[test]
fn disabled_control_retains_focus_but_rejects_activation_from_an_older_frame() {
    let (button, journal, route, mut session) = fixture();
    let focus = session
        .dispatch(
            &journal,
            route,
            UiInputEvent::Navigate {
                seat: UiSeatId::PRIMARY,
                direction: UiNavigation::Next,
            },
        )
        .unwrap();
    let activation = session
        .dispatch(
            &journal,
            route,
            UiInputEvent::ActivateFocused {
                seat: UiSeatId::PRIMARY,
                source: UiActivationSource::Keyboard,
            },
        )
        .unwrap();
    let disabled = button.clone().with_enabled(false);
    let mut state = UiButtonState::default();
    let mut intents = UiIntentBatch::default();
    state
        .reduce_batch(&disabled, focus.consumed().unwrap(), &mut intents)
        .unwrap();
    state
        .reduce_batch(&disabled, activation.consumed().unwrap(), &mut intents)
        .unwrap();
    assert_eq!(state.visual_state(&disabled), UiButtonVisualState::Disabled);
    assert!(state.focused());
    assert_eq!(state.visual_state(&button), UiButtonVisualState::Focused);
    assert!(intents.intents().is_empty());
}

#[test]
fn intent_capacity_failure_does_not_partially_mutate_the_batch() {
    let (button, journal, route, mut session) = fixture();
    session
        .dispatch(
            &journal,
            route,
            UiInputEvent::Navigate {
                seat: UiSeatId::PRIMARY,
                direction: UiNavigation::Next,
            },
        )
        .unwrap();
    let activation = session
        .dispatch(
            &journal,
            route,
            UiInputEvent::ActivateFocused {
                seat: UiSeatId::PRIMARY,
                source: UiActivationSource::Keyboard,
            },
        )
        .unwrap();
    let mut state = UiButtonState::default();
    let mut intents = UiIntentBatch::with_maximum(1).unwrap();
    state
        .reduce_batch(&button, activation.consumed().unwrap(), &mut intents)
        .unwrap();
    assert_eq!(intents.intents().len(), 1);
    assert_eq!(
        state
            .reduce_batch(&button, activation.consumed().unwrap(), &mut intents)
            .unwrap_err(),
        UiIntentBatchError::LimitExceeded {
            actual: 2,
            maximum: 1,
        }
    );
    assert_eq!(intents.intents().len(), 1);
}

#[test]
fn authored_button_rejects_a_different_root_than_its_exact_key() {
    let expected = UiRootKey::runtime("widget-test", "expected").unwrap();
    let actual = UiRootKey::runtime("widget-test", "actual").unwrap();
    let key = UiKey::scoped(
        expected.clone(),
        UiLocalKey::runtime("widget-test", "button").unwrap(),
    )
    .unwrap();
    let button = UiButton::new(key, "Retry", UiActionId::new("retry").unwrap()).unwrap();
    assert_eq!(
        button
            .authored_node(
                &actual,
                UiStyle::absolute(0.0, 0.0, 100.0, 44.0),
                UiVisual::default(),
                UiBrush::Role(UiColorRole::PrimaryText),
                18.0,
            )
            .unwrap_err(),
        UiWidgetError::RootMismatch { expected, actual }
    );
}
