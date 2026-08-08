use std::sync::Arc;

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiActionId, UiActivationSource, UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiButtonPhase,
    UiCompiler, UiDispatchError, UiEnvironment, UiFrame, UiFrameJournal, UiInputEvent,
    UiInteraction, UiInteractionSession, UiLayer, UiLocalKey, UiNavigation, UiNode,
    UiNonActivation, UiPoint, UiPointerButton, UiPointerId, UiRoot, UiRootKey, UiRoutePhase,
    UiRouteStamp, UiRoutedEventKind, UiSeatId, UiSemanticRole, UiSemantics, UiStyle, UiSurfaceId,
    UiTheme,
};

const SURFACE: UiSurfaceId = UiSurfaceId::new(17).unwrap();
const SEAT: UiSeatId = UiSeatId::PRIMARY;
const POINTER: UiPointerId = UiPointerId::PRIMARY;

fn root_key(name: &str) -> UiRootKey {
    UiRootKey::runtime("interaction-test", name).unwrap()
}

fn node_key(name: &str) -> UiLocalKey {
    UiLocalKey::runtime("interaction-test", name).unwrap()
}

fn exact_key(root: &str, node: &str) -> nara_ui_kernel::UiKey {
    nara_ui_kernel::UiKey::scoped(root_key(root), node_key(node)).unwrap()
}

fn button(name: &str, label: &str, action: &str, x: f32, y: f32, enabled: bool) -> UiNode {
    UiNode::new(UiStyle::absolute(x, y, 80.0, 40.0))
        .with_key(node_key(name))
        .with_interaction(UiInteraction::focus_target().with_enabled(enabled))
        .with_semantics(
            UiSemantics::new(UiSemanticRole::Button, label)
                .unwrap()
                .with_action(UiActionId::new(action).unwrap())
                .unwrap(),
        )
}

fn base_root() -> UiRoot {
    base_root_with_first_enabled(true)
}

fn base_root_with_first_enabled(first_enabled: bool) -> UiRoot {
    UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_children([
        button("first", "First", "first", 10.0, 10.0, first_enabled),
        button("second", "Second", "second", 110.0, 10.0, true),
        button("disabled", "Disabled", "disabled", 10.0, 70.0, false),
        hidden_button(),
    ])
}

#[test]
fn disabled_focus_identity_survives_reconciliation_and_reenable() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let initial_route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let mut session = UiInteractionSession::default();
    session
        .dispatch(
            &journal,
            initial_route,
            UiInputEvent::Navigate {
                seat: SEAT,
                direction: UiNavigation::Next,
            },
        )
        .unwrap();

    let disabled_snapshot = UiAuthoringSnapshotBuilder::new(4)
        .with_root(base_root_with_first_enabled(false))
        .build()
        .unwrap();
    let disabled_route = submit(
        &mut journal,
        compile(&mut compiler, &disabled_snapshot, 2),
        11,
    );
    let retained = session
        .semantic_snapshot(&journal, disabled_route, SEAT)
        .unwrap();
    let first = retained
        .nodes()
        .iter()
        .find(|node| node.key() == &exact_key("base", "first"))
        .unwrap();
    assert!(!first.state().enabled());
    assert!(!first.state().focusable());
    assert!(first.state().focused());

    let reenabled_snapshot = UiAuthoringSnapshotBuilder::new(5)
        .with_root(base_root_with_first_enabled(true))
        .build()
        .unwrap();
    let reenabled_route = submit(
        &mut journal,
        compile(&mut compiler, &reenabled_snapshot, 3),
        12,
    );
    let reenabled = session
        .semantic_snapshot(&journal, reenabled_route, SEAT)
        .unwrap();
    let first = reenabled
        .nodes()
        .iter()
        .find(|node| node.key() == &exact_key("base", "first"))
        .unwrap();
    assert!(first.state().enabled());
    assert!(first.state().focusable());
    assert!(first.state().focused());
}

#[test]
fn disabled_middle_focus_remains_the_origin_for_sequential_and_directional_navigation() {
    for (direction, expected) in [
        (UiNavigation::Next, "last"),
        (UiNavigation::Previous, "first"),
        (UiNavigation::Right, "last"),
    ] {
        let mut compiler = compiler();
        let mut journal = UiFrameJournal::default();
        let initial = navigation_snapshot(6, true);
        let initial_route = submit(&mut journal, compile(&mut compiler, &initial, 1), 10);
        let mut session = UiInteractionSession::default();
        session
            .dispatch(
                &journal,
                initial_route,
                UiInputEvent::FocusTarget {
                    seat: SEAT,
                    target: exact_key("navigation", "middle"),
                },
            )
            .unwrap();

        let disabled = navigation_snapshot(7, false);
        let disabled_route = submit(&mut journal, compile(&mut compiler, &disabled, 2), 11);
        let retained = session
            .semantic_snapshot(&journal, disabled_route, SEAT)
            .unwrap();
        let middle = retained
            .nodes()
            .iter()
            .find(|node| node.key() == &exact_key("navigation", "middle"))
            .unwrap();
        assert!(!middle.state().enabled());
        assert!(middle.state().focused());

        session
            .dispatch(
                &journal,
                disabled_route,
                UiInputEvent::Navigate {
                    seat: SEAT,
                    direction,
                },
            )
            .unwrap();
        let navigated = session
            .semantic_snapshot(&journal, disabled_route, SEAT)
            .unwrap();
        let destination = navigated
            .nodes()
            .iter()
            .find(|node| node.key() == &exact_key("navigation", expected))
            .unwrap();
        assert!(destination.state().focused(), "{direction:?}");
    }
}

fn navigation_snapshot(revision: u64, middle_enabled: bool) -> UiAuthoringSnapshot {
    UiAuthoringSnapshotBuilder::new(revision)
        .with_root(
            UiRoot::new(root_key("navigation"), UiLayer::Base, UiStyle::fill()).with_children([
                button("first", "First", "first", 10.0, 10.0, true),
                button("middle", "Middle", "middle", 110.0, 10.0, middle_enabled),
                button("last", "Last", "last", 210.0, 10.0, true),
            ]),
        )
        .build()
        .unwrap()
}

fn hidden_button() -> UiNode {
    UiNode::new(UiStyle::absolute(110.0, 70.0, 80.0, 40.0).with_visible(false))
        .with_key(node_key("hidden"))
        .with_interaction(UiInteraction::focus_target())
        .with_semantics(
            UiSemantics::new(UiSemanticRole::Button, "Hidden")
                .unwrap()
                .with_action(UiActionId::new("hidden").unwrap())
                .unwrap(),
        )
}

fn modal_root() -> UiRoot {
    UiRoot::new(root_key("modal"), UiLayer::Modal, UiStyle::fill())
        .with_child(button("retry", "Retry", "retry", 210.0, 120.0, true))
}

fn snapshot(include_modal: bool) -> UiAuthoringSnapshot {
    let builder =
        UiAuthoringSnapshotBuilder::new(if include_modal { 2 } else { 1 }).with_root(base_root());
    if include_modal {
        builder.with_root(modal_root()).build().unwrap()
    } else {
        builder.build().unwrap()
    }
}

fn empty_snapshot() -> UiAuthoringSnapshot {
    UiAuthoringSnapshotBuilder::new(3).build().unwrap()
}

fn compile(
    compiler: &mut UiCompiler,
    snapshot: &UiAuthoringSnapshot,
    route_revision: u64,
) -> Arc<UiFrame> {
    compiler
        .compile(
            snapshot,
            UiEnvironment::new(
                SURFACE,
                320.0,
                180.0,
                1.0,
                route_revision,
                route_revision,
                1,
                1,
            )
            .unwrap(),
            UiTheme::default(),
        )
        .unwrap()
}

fn compiler() -> UiCompiler {
    UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap()
}

fn submit(journal: &mut UiFrameJournal, frame: Arc<UiFrame>, epoch: u64) -> UiRouteStamp {
    journal.submit(frame, epoch).unwrap();
    UiRouteStamp::new(SURFACE, epoch)
}

fn move_pointer(position: UiPoint) -> UiInputEvent {
    UiInputEvent::PointerMoved {
        seat: SEAT,
        pointer: POINTER,
        position,
    }
}

fn pointer_button(phase: UiButtonPhase, position: UiPoint) -> UiInputEvent {
    UiInputEvent::PointerButton {
        seat: SEAT,
        pointer: POINTER,
        button: UiPointerButton::Primary,
        phase,
        position: Some(position),
    }
}

#[test]
fn pointer_capture_routes_capture_target_bubble_and_release_outside() {
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot(false), 1);
    let mut journal = UiFrameJournal::default();
    let route = submit(&mut journal, frame, 10);
    let mut session = UiInteractionSession::default();
    let first = exact_key("base", "first");

    session
        .dispatch(&journal, route, move_pointer(UiPoint::new(20.0, 20.0)))
        .unwrap();
    let pressed = session
        .dispatch(
            &journal,
            route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    let press_route = pressed
        .consumed()
        .unwrap()
        .events()
        .iter()
        .filter(|event| event.kind() == &UiRoutedEventKind::Pressed)
        .collect::<Vec<_>>();
    assert_eq!(press_route.len(), 3);
    assert_eq!(press_route[0].phase(), UiRoutePhase::Capture);
    assert_eq!(press_route[1].phase(), UiRoutePhase::Target);
    assert_eq!(press_route[2].phase(), UiRoutePhase::Bubble);
    assert_eq!(press_route[1].target(), &first);
    assert_eq!(press_route[1].current_target(), &first);

    let moved = session
        .dispatch(&journal, route, move_pointer(UiPoint::new(300.0, 170.0)))
        .unwrap();
    assert!(moved.consumed().unwrap().events().iter().any(|event| {
        event.target() == &first
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::HoverChanged { hovered: false }
    }));
    let released = session
        .dispatch(
            &journal,
            route,
            pointer_button(UiButtonPhase::Released, UiPoint::new(300.0, 170.0)),
        )
        .unwrap();
    assert!(released.consumed().unwrap().events().iter().any(|event| {
        event.target() == &first
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Released { inside: false }
    }));

    session
        .dispatch(
            &journal,
            route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    let cancelled = session
        .dispatch(
            &journal,
            route,
            UiInputEvent::PointerCancelled {
                seat: SEAT,
                pointer: POINTER,
            },
        )
        .unwrap();
    assert!(cancelled.consumed().unwrap().events().iter().any(|event| {
        event.target() == &first
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Cancelled
    }));
}

#[test]
fn disabled_hidden_and_modal_underlay_fail_closed() {
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot(false), 1);
    let mut journal = UiFrameJournal::default();
    let route = submit(&mut journal, frame, 10);
    let mut session = UiInteractionSession::default();

    let disabled = session
        .dispatch(
            &journal,
            route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 80.0)),
        )
        .unwrap();
    assert_eq!(
        disabled.rejection(),
        Some(&UiNonActivation::TargetDisabled {
            target: exact_key("base", "disabled")
        })
    );
    let hidden = session
        .dispatch(
            &journal,
            route,
            UiInputEvent::SemanticAction {
                seat: SEAT,
                target: exact_key("base", "hidden"),
                action: UiActionId::new("hidden").unwrap(),
            },
        )
        .unwrap();
    assert_eq!(
        hidden.rejection(),
        Some(&UiNonActivation::TargetHidden {
            target: exact_key("base", "hidden")
        })
    );

    let modal_route = submit(&mut journal, compile(&mut compiler, &snapshot(true), 2), 11);
    let inert = session
        .dispatch(
            &journal,
            modal_route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    assert_eq!(
        inert.rejection(),
        Some(&UiNonActivation::TargetOutsideModal {
            target: exact_key("base", "first")
        })
    );
}

#[test]
fn navigation_activation_and_modal_focus_restoration_share_one_session() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let base_route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let mut session = UiInteractionSession::default();

    session
        .dispatch(
            &journal,
            base_route,
            UiInputEvent::Navigate {
                seat: SEAT,
                direction: UiNavigation::Next,
            },
        )
        .unwrap();
    session
        .dispatch(
            &journal,
            base_route,
            UiInputEvent::Navigate {
                seat: SEAT,
                direction: UiNavigation::Right,
            },
        )
        .unwrap();
    let second = session
        .dispatch(
            &journal,
            base_route,
            UiInputEvent::ActivateFocused {
                seat: SEAT,
                source: UiActivationSource::Gamepad,
            },
        )
        .unwrap();
    assert!(second.consumed().unwrap().events().iter().any(|event| {
        event.target() == &exact_key("base", "second")
            && event.phase() == UiRoutePhase::Target
            && event.kind()
                == &UiRoutedEventKind::ActivateRequested {
                    action: UiActionId::new("second").unwrap(),
                    source: UiActivationSource::Gamepad,
                }
    }));

    let modal_route = submit(&mut journal, compile(&mut compiler, &snapshot(true), 2), 11);
    let retry = session
        .dispatch(
            &journal,
            modal_route,
            UiInputEvent::ActivateFocused {
                seat: SEAT,
                source: UiActivationSource::Keyboard,
            },
        )
        .unwrap();
    assert!(retry.consumed().unwrap().events().iter().any(|event| {
        event.target() == &exact_key("modal", "retry")
            && event.phase() == UiRoutePhase::Target
            && matches!(
                event.kind(),
                UiRoutedEventKind::ActivateRequested {
                    source: UiActivationSource::Keyboard,
                    ..
                }
            )
    }));

    let restored_route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 3),
        12,
    );
    let restored = session
        .dispatch(
            &journal,
            restored_route,
            UiInputEvent::ActivateFocused {
                seat: SEAT,
                source: UiActivationSource::Keyboard,
            },
        )
        .unwrap();
    assert!(restored.consumed().unwrap().events().iter().any(|event| {
        event.target() == &exact_key("base", "second")
            && event.phase() == UiRoutePhase::Target
            && matches!(event.kind(), UiRoutedEventKind::ActivateRequested { .. })
    }));
}

#[test]
fn retired_targets_and_invalidated_geometry_cannot_activate() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let old_route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let _retired_route = submit(
        &mut journal,
        compile(&mut compiler, &empty_snapshot(), 2),
        11,
    );
    let mut session = UiInteractionSession::default();
    let retired = session
        .dispatch(
            &journal,
            old_route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    assert_eq!(
        retired.rejection(),
        Some(&UiNonActivation::TargetRetired {
            target: exact_key("base", "first")
        })
    );

    let mut fresh_journal = UiFrameJournal::default();
    let route = submit(
        &mut fresh_journal,
        compile(&mut compiler, &snapshot(false), 3),
        20,
    );
    let mut session = UiInteractionSession::default();
    session
        .dispatch(&fresh_journal, route, UiInputEvent::GeometryInvalidated)
        .unwrap();
    let stale = session
        .dispatch(
            &fresh_journal,
            route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    assert_eq!(stale.rejection(), Some(&UiNonActivation::StaleGeometry));
    let newer = submit(
        &mut fresh_journal,
        compile(&mut compiler, &snapshot(false), 4),
        21,
    );
    let active = session
        .dispatch(
            &fresh_journal,
            newer,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    assert!(active.rejection().is_none());
}

#[test]
fn semantic_snapshot_uses_the_exact_frame_and_interaction_state() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let route = submit(&mut journal, compile(&mut compiler, &snapshot(true), 1), 10);
    let mut session = UiInteractionSession::default();
    session
        .dispatch(
            &journal,
            route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(220.0, 130.0)),
        )
        .unwrap();
    let semantics = session.semantic_snapshot(&journal, route, SEAT).unwrap();
    let base = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &exact_key("base", "first"))
        .unwrap();
    assert!(base.state().inert());
    assert!(base.state().enabled());
    assert!(!base.state().focusable());
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &exact_key("modal", "retry"))
        .unwrap();
    assert_eq!(retry.role(), UiSemanticRole::Button);
    assert_eq!(retry.name(), "Retry");
    assert_eq!(retry.actions(), &[UiActionId::new("retry").unwrap()]);
    assert!(retry.state().enabled());
    assert!(retry.state().focusable());
    assert!(retry.state().focused());
    assert!(retry.state().pressed());
    assert_eq!(retry.rect().origin(), UiPoint::new(210.0, 120.0));

    let disabled_retry = UiAuthoringSnapshotBuilder::new(3)
        .with_root(base_root())
        .with_root(
            UiRoot::new(root_key("modal"), UiLayer::Modal, UiStyle::fill())
                .with_child(button("retry", "Retry", "retry", 210.0, 120.0, false)),
        )
        .build()
        .unwrap();
    let latest_route = submit(&mut journal, compile(&mut compiler, &disabled_retry, 2), 11);
    let latest_semantics = session
        .semantic_snapshot(&journal, latest_route, SEAT)
        .unwrap();
    let disabled_retry = latest_semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &exact_key("modal", "retry"))
        .unwrap();
    assert!(!disabled_retry.state().enabled());
    assert!(!disabled_retry.state().focusable());
    assert!(disabled_retry.state().focused());
    assert!(!disabled_retry.state().pressed());
    let disabled_activation = session
        .dispatch(
            &journal,
            latest_route,
            UiInputEvent::ActivateFocused {
                seat: SEAT,
                source: UiActivationSource::Keyboard,
            },
        )
        .unwrap();
    assert!(disabled_activation.consumed().is_some());
    let rejected_activation = session
        .dispatch(
            &journal,
            latest_route,
            UiInputEvent::ActivateFocused {
                seat: SEAT,
                source: UiActivationSource::Keyboard,
            },
        )
        .unwrap();
    assert_eq!(
        rejected_activation.rejection(),
        Some(&UiNonActivation::TargetDisabled {
            target: exact_key("modal", "retry")
        })
    );
    assert_eq!(
        session
            .semantic_snapshot(&journal, route, SEAT)
            .unwrap_err(),
        UiDispatchError::SemanticSnapshotNotLatest {
            requested: 10,
            latest: 11,
        }
    );
}

#[test]
fn unsupported_pointer_buttons_do_not_consume_pointer_capacity() {
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot(false), 1);
    let mut journal = UiFrameJournal::default();
    let route = submit(&mut journal, frame, 10);
    let mut session = UiInteractionSession::default();

    for pointer in 0..64 {
        let rejected = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::PointerButton {
                    seat: SEAT,
                    pointer: UiPointerId::new(pointer),
                    button: UiPointerButton::Secondary,
                    phase: UiButtonPhase::Pressed,
                    position: Some(UiPoint::new(20.0, 20.0)),
                },
            )
            .unwrap();
        assert_eq!(
            rejected.rejection(),
            Some(&UiNonActivation::UnsupportedPointerButton)
        );
    }

    let accepted = session
        .dispatch(
            &journal,
            route,
            UiInputEvent::PointerButton {
                seat: SEAT,
                pointer: UiPointerId::new(1_000),
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Pressed,
                position: Some(UiPoint::new(20.0, 20.0)),
            },
        )
        .unwrap();
    assert!(accepted.rejection().is_none());
}

#[test]
fn rejected_missing_positions_do_not_consume_pointer_capacity() {
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot(false), 1);
    let mut journal = UiFrameJournal::default();
    let route = submit(&mut journal, frame, 10);
    let mut session = UiInteractionSession::default();

    for pointer in 0..64 {
        let rejected = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::PointerButton {
                    seat: SEAT,
                    pointer: UiPointerId::new(pointer),
                    button: UiPointerButton::Primary,
                    phase: UiButtonPhase::Pressed,
                    position: None,
                },
            )
            .unwrap();
        assert_eq!(
            rejected.rejection(),
            Some(&UiNonActivation::MissingPointerPosition)
        );
    }

    let accepted = session
        .dispatch(
            &journal,
            route,
            UiInputEvent::PointerButton {
                seat: SEAT,
                pointer: UiPointerId::new(1_000),
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Pressed,
                position: Some(UiPoint::new(20.0, 20.0)),
            },
        )
        .unwrap();
    assert!(accepted.consumed().is_some());
}

#[test]
fn geometry_invalidation_requires_a_fresh_pointer_position() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let mut session = UiInteractionSession::default();
    session
        .dispatch(&journal, route, move_pointer(UiPoint::new(20.0, 20.0)))
        .unwrap();
    session
        .dispatch(&journal, route, UiInputEvent::GeometryInvalidated)
        .unwrap();

    let newer = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 2),
        11,
    );
    let rejected = session
        .dispatch(
            &journal,
            newer,
            UiInputEvent::PointerButton {
                seat: SEAT,
                pointer: POINTER,
                button: UiPointerButton::Primary,
                phase: UiButtonPhase::Pressed,
                position: None,
            },
        )
        .unwrap();
    assert_eq!(
        rejected.rejection(),
        Some(&UiNonActivation::MissingPointerPosition)
    );
    assert!(rejected.consumed().is_none());
}

#[test]
fn semantic_snapshot_reports_only_committed_focus() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let mut session = UiInteractionSession::default();

    let initial = session.semantic_snapshot(&journal, route, SEAT).unwrap();
    assert!(initial.nodes().iter().all(|node| !node.state().focused()));

    let rejected = session
        .dispatch(
            &journal,
            route,
            UiInputEvent::ActivateFocused {
                seat: SEAT,
                source: UiActivationSource::Keyboard,
            },
        )
        .unwrap();
    assert_eq!(rejected.rejection(), Some(&UiNonActivation::NoFocusTarget));
    let after_rejection = session.semantic_snapshot(&journal, route, SEAT).unwrap();
    assert!(
        after_rejection
            .nodes()
            .iter()
            .all(|node| !node.state().focused())
    );

    let navigated = session
        .dispatch(
            &journal,
            route,
            UiInputEvent::Navigate {
                seat: SEAT,
                direction: UiNavigation::Next,
            },
        )
        .unwrap();
    assert!(navigated.consumed().is_some());
    let committed = session.semantic_snapshot(&journal, route, SEAT).unwrap();
    assert_eq!(
        committed
            .nodes()
            .iter()
            .filter(|node| node.state().focused())
            .count(),
        1
    );
}
