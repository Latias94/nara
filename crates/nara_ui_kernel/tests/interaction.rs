use std::sync::Arc;

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAccessibilityInput, UiActionId, UiActivationSource, UiAuthoringSnapshot,
    UiAuthoringSnapshotBuilder, UiButtonPhase, UiCompiler, UiDispatchError, UiEnvironment, UiFrame,
    UiFrameJournal, UiInputEvent, UiInteraction, UiInteractionSession, UiLayer, UiLimits,
    UiLocalKey, UiNavigation, UiNode, UiNonActivation, UiPoint, UiPointerButton, UiPointerId,
    UiRoot, UiRootKey, UiRoutePhase, UiRouteStamp, UiRoutedEventKind, UiSeatId, UiSemanticRole,
    UiSemantics, UiStyle, UiSurfaceId, UiTheme,
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
    let environment = frame.environment();
    match journal.install_surface(
        environment.surface_id(),
        environment.slot_generation().get(),
    ) {
        Ok(()) | Err(nara_ui_kernel::UiJournalError::SurfaceAlreadyInstalled { .. }) => {}
        Err(error) => panic!("test surface should install explicitly: {error}"),
    }
    UiRouteStamp::from_receipt(journal.submit(frame, epoch).unwrap())
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
        .dispatch_accessibility(
            &journal,
            UiAccessibilityInput::semantic_action(
                journal.resolve(route).unwrap().receipt(),
                SEAT,
                exact_key("base", "hidden"),
                UiActionId::new("hidden").unwrap(),
            ),
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
    assert!(inert.consumed().unwrap().is_reconciliation());
    let rejected_inert = session
        .dispatch(
            &journal,
            modal_route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    assert_eq!(
        rejected_inert.rejection(),
        Some(&UiNonActivation::TargetOutsideModal {
            target: exact_key("base", "first")
        })
    );
}

#[test]
fn receipt_stamped_accessibility_focus_and_action_share_the_presented_transaction() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let receipt = journal.resolve(route).unwrap().receipt();
    let target = exact_key("base", "first");
    let mut session = UiInteractionSession::default();

    let focused = session
        .dispatch_accessibility(
            &journal,
            UiAccessibilityInput::focus_target(receipt, SEAT, target.clone()),
        )
        .unwrap();
    assert!(focused.consumed().unwrap().events().iter().any(|event| {
        event.target() == &target
            && event.kind() == &UiRoutedEventKind::FocusChanged { focused: true }
    }));

    let activated = session
        .dispatch_accessibility(
            &journal,
            UiAccessibilityInput::semantic_action(
                receipt,
                SEAT,
                target.clone(),
                UiActionId::new("first").unwrap(),
            ),
        )
        .unwrap();
    assert!(activated.consumed().unwrap().events().iter().any(|event| {
        event.target() == &target
            && event.kind()
                == &UiRoutedEventKind::ActivateRequested {
                    action: UiActionId::new("first").unwrap(),
                    source: UiActivationSource::Accessibility,
                }
    }));
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
    let modal_reconciliation = session
        .dispatch(
            &journal,
            modal_route,
            UiInputEvent::ActivateFocused {
                seat: SEAT,
                source: UiActivationSource::Keyboard,
            },
        )
        .unwrap();
    assert!(modal_reconciliation.consumed().unwrap().is_reconciliation());
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
    let restore_reconciliation = session
        .dispatch(
            &journal,
            restored_route,
            UiInputEvent::ActivateFocused {
                seat: SEAT,
                source: UiActivationSource::Keyboard,
            },
        )
        .unwrap();
    assert!(
        restore_reconciliation
            .consumed()
            .unwrap()
            .is_reconciliation()
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
fn removed_modal_restore_target_cannot_revive_after_same_key_reinsertion() {
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
            UiInputEvent::FocusTarget {
                seat: SEAT,
                target: exact_key("base", "second"),
            },
        )
        .unwrap();

    submit(&mut journal, compile(&mut compiler, &snapshot(true), 2), 11);
    session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .unwrap();

    let without_second = UiAuthoringSnapshotBuilder::new(8)
        .with_root(
            UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_children([
                button("first", "First", "first", 10.0, 10.0, true),
                button("disabled", "Disabled", "disabled", 10.0, 70.0, false),
                hidden_button(),
            ]),
        )
        .with_root(modal_root())
        .build()
        .unwrap();
    submit(&mut journal, compile(&mut compiler, &without_second, 3), 12);
    let retired_restore = session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .unwrap();
    assert!(retired_restore.is_reconciliation());
    assert!(retired_restore.events().is_empty());

    let reinserted = UiAuthoringSnapshotBuilder::new(9)
        .with_root(base_root())
        .with_root(modal_root())
        .build()
        .unwrap();
    submit(&mut journal, compile(&mut compiler, &reinserted, 4), 13);
    let rebound_generation = session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .unwrap();
    assert!(rebound_generation.is_reconciliation());
    assert!(rebound_generation.events().is_empty());

    let restored = UiAuthoringSnapshotBuilder::new(10)
        .with_root(base_root())
        .build()
        .unwrap();
    let restored_route = submit(&mut journal, compile(&mut compiler, &restored, 5), 14);
    session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .unwrap();
    let semantics = session
        .semantic_snapshot(&journal, restored_route, SEAT)
        .unwrap();
    assert!(
        semantics
            .nodes()
            .iter()
            .find(|node| node.key() == &exact_key("base", "first"))
            .unwrap()
            .state()
            .focused()
    );
    assert!(
        !semantics
            .nodes()
            .iter()
            .find(|node| node.key() == &exact_key("base", "second"))
            .unwrap()
            .state()
            .focused()
    );
}

#[test]
fn candidate_reconciliation_is_staged_without_mutating_the_presented_session() {
    let mut compiler = compiler();
    let initial_frame = compile(&mut compiler, &snapshot(false), 1);
    let mut journal = UiFrameJournal::default();
    let initial_route = submit(&mut journal, Arc::clone(&initial_frame), 10);
    let mut session = UiInteractionSession::default();
    session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .unwrap();
    session
        .dispatch(
            &journal,
            initial_route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();

    let target = exact_key("base", "first");
    let candidate_frame = compile(&mut compiler, &snapshot(false), 2);
    let staged = session
        .stage_candidate_presentation(
            SURFACE,
            Some(&initial_frame),
            &candidate_frame,
            std::slice::from_ref(&target),
        )
        .unwrap()
        .unwrap();
    assert!(staged.dispatch().events().iter().any(|event| {
        event.target() == &target
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Cancelled
    }));

    let released = session
        .dispatch(
            &journal,
            initial_route,
            pointer_button(UiButtonPhase::Released, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    assert!(released.consumed().unwrap().events().iter().any(|event| {
        event.target() == &target
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Released { inside: true }
    }));

    let (mut staged_session, _) = staged.into_parts();
    let staged_release = staged_session
        .dispatch(
            &journal,
            initial_route,
            pointer_button(UiButtonPhase::Released, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    assert_eq!(
        staged_release.rejection(),
        Some(&UiNonActivation::NoPointerCapture)
    );
}

#[test]
fn explicit_behavior_identity_retirement_cancels_inflight_pointer_state() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let initial_route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let mut session = UiInteractionSession::default();
    let target = exact_key("base", "first");
    session
        .dispatch(
            &journal,
            initial_route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    let changed_snapshot = UiAuthoringSnapshotBuilder::new(11)
        .with_root(base_root())
        .build()
        .unwrap();
    let changed_route = submit(
        &mut journal,
        compile(&mut compiler, &changed_snapshot, 2),
        11,
    );

    let retired = session
        .reconcile_latest_presentation(&journal, SURFACE, std::slice::from_ref(&target))
        .unwrap()
        .unwrap();
    assert!(retired.is_reconciliation());
    assert!(retired.events().iter().any(|event| {
        event.target() == &target
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Cancelled
    }));
    let semantics = session
        .semantic_snapshot(&journal, changed_route, SEAT)
        .unwrap();
    assert!(
        semantics
            .nodes()
            .iter()
            .find(|node| node.key() == &target)
            .unwrap()
            .state()
            .focused()
    );

    let release = session
        .dispatch(
            &journal,
            changed_route,
            pointer_button(UiButtonPhase::Released, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    assert_eq!(
        release.rejection(),
        Some(&UiNonActivation::NoPointerCapture)
    );
}

#[test]
fn invalidating_capture_retires_the_same_pointers_independent_hover_state() {
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
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    session
        .dispatch(
            &journal,
            initial_route,
            move_pointer(UiPoint::new(120.0, 20.0)),
        )
        .unwrap();
    let captured = exact_key("base", "first");
    let hovered = exact_key("base", "second");
    submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 2),
        11,
    );

    let reconciled = session
        .reconcile_latest_presentation(&journal, SURFACE, std::slice::from_ref(&captured))
        .unwrap()
        .unwrap();
    assert!(reconciled.events().iter().any(|event| {
        event.target() == &captured
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Cancelled
    }));
    assert!(reconciled.events().iter().any(|event| {
        event.target() == &hovered
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::HoverChanged { hovered: false }
    }));
}

#[test]
fn low_node_limit_still_admits_the_complete_pointer_cleanup_batch() {
    let limits = UiLimits::default().with_max_nodes(2).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), limits).unwrap();
    let authored = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root_key("small"), UiLayer::Base, UiStyle::fill())
                .with_child(button("only", "Only", "only", 10.0, 10.0, true)),
        )
        .build()
        .unwrap();
    let mut journal = UiFrameJournal::with_limits(limits).unwrap();
    let initial_route = submit(&mut journal, compile(&mut compiler, &authored, 1), 10);
    let mut session = UiInteractionSession::with_limits(limits).unwrap();
    session
        .dispatch(
            &journal,
            initial_route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    submit(&mut journal, compile(&mut compiler, &authored, 2), 11);

    let cleanup = session
        .reconcile_latest_presentation(&journal, SURFACE, &[exact_key("small", "only")])
        .unwrap()
        .unwrap();
    assert!(cleanup.events().iter().any(|event| {
        event.target() == &exact_key("small", "only")
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Cancelled
    }));
    assert!(cleanup.events().iter().any(|event| {
        event.target() == &exact_key("small", "only")
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::HoverChanged { hovered: false }
    }));
}

#[test]
fn equivalent_presentation_rebinds_pointer_capture_to_the_latest_epoch() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let initial_route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let mut session = UiInteractionSession::default();
    let initial = session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .unwrap();
    assert!(initial.is_reconciliation());
    let target = exact_key("base", "first");
    session
        .dispatch(
            &journal,
            initial_route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();

    let latest_route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        11,
    );
    let rebound = session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .unwrap();
    assert!(rebound.is_reconciliation());
    assert!(rebound.events().is_empty());
    let semantics = session
        .semantic_snapshot(&journal, latest_route, SEAT)
        .unwrap();
    assert!(
        semantics
            .nodes()
            .iter()
            .find(|node| node.key() == &target)
            .unwrap()
            .state()
            .pressed()
    );

    let released = session
        .dispatch(
            &journal,
            latest_route,
            pointer_button(UiButtonPhase::Released, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    let released = released.consumed().unwrap();
    assert!(!released.is_reconciliation());
    assert!(released.events().iter().any(|event| {
        event.target() == &target
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Released { inside: true }
    }));
    assert!(
        released
            .events()
            .iter()
            .all(|event| event.kind() != &UiRoutedEventKind::Cancelled)
    );
}

#[test]
fn pointer_capture_cannot_cross_presentation_generations() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let historical_route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let latest_route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 2),
        11,
    );
    let mut session = UiInteractionSession::default();
    let target = exact_key("base", "first");

    let pressed = session
        .dispatch(
            &journal,
            historical_route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    assert!(pressed.consumed().is_some());
    let semantics = session
        .semantic_snapshot(&journal, latest_route, SEAT)
        .unwrap();
    assert!(
        !semantics
            .nodes()
            .iter()
            .find(|node| node.key() == &target)
            .unwrap()
            .state()
            .pressed()
    );

    let cleanup = session
        .dispatch(
            &journal,
            latest_route,
            pointer_button(UiButtonPhase::Released, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    let cleanup = cleanup.consumed().unwrap();
    assert!(cleanup.is_reconciliation());
    assert_eq!(cleanup.presentation_epoch(), 11);
    assert!(cleanup.events().iter().any(|event| {
        event.target() == &target
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Cancelled
    }));
    assert!(cleanup.events().iter().all(|event| {
        !matches!(
            event.kind(),
            UiRoutedEventKind::Released { .. } | UiRoutedEventKind::ActivateRequested { .. }
        )
    }));

    let rejected = session
        .dispatch(
            &journal,
            latest_route,
            pointer_button(UiButtonPhase::Released, UiPoint::new(20.0, 20.0)),
        )
        .unwrap();
    assert_eq!(
        rejected.rejection(),
        Some(&UiNonActivation::NoPointerCapture)
    );
}

#[test]
fn historical_input_reconciles_against_the_latest_presentation_before_routing() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let historical_route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let mut session = UiInteractionSession::default();
    session
        .dispatch(
            &journal,
            historical_route,
            UiInputEvent::Navigate {
                seat: SEAT,
                direction: UiNavigation::Next,
            },
        )
        .unwrap();
    let latest_route = submit(&mut journal, compile(&mut compiler, &snapshot(true), 2), 11);

    let reconciled = session
        .dispatch(
            &journal,
            historical_route,
            UiInputEvent::ActivateFocused {
                seat: SEAT,
                source: UiActivationSource::Keyboard,
            },
        )
        .unwrap();
    let reconciliation = reconciled.consumed().unwrap();
    assert!(reconciliation.is_reconciliation());
    assert_eq!(reconciliation.presentation_epoch(), 11);
    assert!(
        reconciliation
            .events()
            .iter()
            .all(|event| { !matches!(event.kind(), UiRoutedEventKind::ActivateRequested { .. }) })
    );
    assert!(reconciliation.events().iter().any(|event| {
        event.target() == &exact_key("modal", "retry")
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::FocusChanged { focused: true }
    }));

    let rejected = session
        .dispatch(
            &journal,
            historical_route,
            UiInputEvent::ActivateFocused {
                seat: SEAT,
                source: UiActivationSource::Keyboard,
            },
        )
        .unwrap();
    assert_eq!(
        rejected.rejection(),
        Some(&UiNonActivation::TargetRetired {
            target: exact_key("modal", "retry")
        })
    );
    let semantics = session
        .semantic_snapshot(&journal, latest_route, SEAT)
        .unwrap();
    assert!(
        semantics
            .nodes()
            .iter()
            .find(|node| node.key() == &exact_key("modal", "retry"))
            .unwrap()
            .state()
            .focused()
    );
}

#[test]
fn empty_reconciliation_is_separate_when_the_presentation_generation_changed() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let historical_route = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let _latest_route = submit(&mut journal, compile(&mut compiler, &snapshot(true), 2), 11);
    let mut session = UiInteractionSession::default();
    let historical_move = move_pointer(UiPoint::new(20.0, 20.0));

    let reconciled = session
        .dispatch(&journal, historical_route, historical_move.clone())
        .unwrap();
    let reconciliation = reconciled.consumed().unwrap();
    assert!(reconciliation.is_reconciliation());
    assert_eq!(reconciliation.presentation_epoch(), 11);
    assert!(reconciliation.events().is_empty());

    let rejected = session
        .dispatch(&journal, historical_route, historical_move)
        .unwrap();
    assert_eq!(
        rejected.rejection(),
        Some(&UiNonActivation::TargetOutsideModal {
            target: exact_key("base", "first")
        })
    );
}

#[test]
fn presentation_reconciliation_precedes_input_validation_errors() {
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let route = submit(&mut journal, compile(&mut compiler, &snapshot(true), 1), 10);
    let mut session = UiInteractionSession::default();
    let invalid_move = UiInputEvent::PointerMoved {
        seat: SEAT,
        pointer: POINTER,
        position: UiPoint::new(f32::NAN, 20.0),
    };

    let reconciled = session
        .dispatch(&journal, route, invalid_move.clone())
        .unwrap();
    let reconciliation = reconciled.consumed().unwrap();
    assert!(reconciliation.is_reconciliation());
    assert!(reconciliation.events().is_empty());

    assert_eq!(
        session.dispatch(&journal, route, invalid_move).unwrap_err(),
        UiDispatchError::NonFinitePointerPosition
    );
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
    let descriptive = session.semantic_snapshot(&journal, route, SEAT).unwrap();
    assert!(
        descriptive
            .nodes()
            .iter()
            .all(|node| node.actions().is_empty())
    );
    let semantics = session
        .semantic_snapshot_with_action_authority(&journal, route, SEAT, |key, action| {
            key == &exact_key("modal", "retry") && action == &UiActionId::new("retry").unwrap()
        })
        .unwrap();
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
    assert_eq!(retry.physical_rect().min_x(), 210);
    assert_eq!(retry.physical_rect().min_y(), 120);

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
fn semantic_snapshot_omits_nodes_outside_the_surface_content_rect() {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill())
                .with_child(button("outside", "Outside", "outside", 400.0, 20.0, true)),
        )
        .build()
        .unwrap();
    let mut compiler = compiler();
    let mut journal = UiFrameJournal::default();
    let route = submit(&mut journal, compile(&mut compiler, &snapshot, 1), 10);

    let semantics = UiInteractionSession::default()
        .semantic_snapshot_with_action_authority(&journal, route, SEAT, |_, _| true)
        .unwrap();
    assert!(semantics.nodes().is_empty());
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
fn one_surface_has_one_seat_and_one_pointer_owner() {
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot(false), 1);
    let mut journal = UiFrameJournal::default();
    let route = submit(&mut journal, frame, 10);
    let mut session = UiInteractionSession::default();
    session
        .dispatch(&journal, route, move_pointer(UiPoint::new(20.0, 20.0)))
        .unwrap();

    assert_eq!(
        session
            .dispatch(
                &journal,
                route,
                UiInputEvent::PointerMoved {
                    seat: SEAT,
                    pointer: UiPointerId::new(1),
                    position: UiPoint::new(20.0, 20.0),
                },
            )
            .unwrap_err(),
        UiDispatchError::PointerLimitExceeded { maximum: 1 }
    );
    assert_eq!(
        session
            .dispatch(
                &journal,
                route,
                UiInputEvent::Navigate {
                    seat: UiSeatId::new(1),
                    direction: UiNavigation::Next,
                },
            )
            .unwrap_err(),
        UiDispatchError::SeatLimitExceeded { maximum: 1 }
    );
}

#[test]
fn pending_reconciliation_is_consumed_before_unsupported_pointer_rejection() {
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
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(20.0, 20.0)),
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
    let unsupported = UiInputEvent::PointerButton {
        seat: SEAT,
        pointer: POINTER,
        button: UiPointerButton::Secondary,
        phase: UiButtonPhase::Pressed,
        position: Some(UiPoint::new(20.0, 20.0)),
    };

    let reconciled = session
        .dispatch(&journal, disabled_route, unsupported.clone())
        .unwrap();
    assert!(reconciled.consumed().unwrap().events().iter().any(|event| {
        event.target() == &exact_key("base", "first")
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Cancelled
    }));

    let rejected = session
        .dispatch(&journal, disabled_route, unsupported)
        .unwrap();
    assert_eq!(
        rejected.rejection(),
        Some(&UiNonActivation::UnsupportedPointerButton)
    );
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
    assert!(session.has_pointer_facts(SURFACE));
    session
        .dispatch(&journal, route, UiInputEvent::GeometryInvalidated)
        .unwrap();
    assert!(!session.has_pointer_facts(SURFACE));

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
