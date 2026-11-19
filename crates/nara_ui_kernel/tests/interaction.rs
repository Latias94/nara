use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAccessibilityInput, UiActionId, UiActivationSource, UiAuthoringSnapshot,
    UiAuthoringSnapshotBuilder, UiButtonPhase, UiCompiler, UiDispatchError, UiEnvironment, UiFrame,
    UiFrameJournal, UiInputEvent, UiInteraction, UiInteractionSession, UiJournalError, UiLayer,
    UiLimits, UiLocalKey, UiNavigation, UiNode, UiNonActivation, UiOverflow, UiPoint,
    UiPointerButton, UiPointerId, UiPointerInput, UiRect, UiRoot, UiRootKey, UiRoutePhase,
    UiRouteStamp, UiRoutedEventKind, UiSeatId, UiSemanticRole, UiSemantics, UiSize, UiStyle,
    UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiTheme,
};

#[path = "interaction/pointer_input.rs"]
mod pointer_input;

struct HostedJournal {
    journal: UiFrameJournal,
    authority: nara_ui_kernel::__private::UiFrameJournalAuthority,
}

impl Deref for HostedJournal {
    type Target = UiFrameJournal;

    fn deref(&self) -> &Self::Target {
        &self.journal
    }
}

impl DerefMut for HostedJournal {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.journal
    }
}

fn hosted_journal() -> HostedJournal {
    hosted_journal_with_limits(Default::default())
}

fn hosted_journal_with_limits(limits: UiLimits) -> HostedJournal {
    let (journal, authority) = nara_ui_kernel::__private::new_hosted_frame_journal(limits).unwrap();
    HostedJournal { journal, authority }
}

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

fn clipping_parent(x: f32, y: f32, width: f32, height: f32, child: UiNode) -> UiNode {
    UiNode::new(UiStyle::absolute(x, y, width, height).with_overflow(UiOverflow::Clip))
        .with_child(child)
}

fn keyboard_activation() -> UiInputEvent {
    UiInputEvent::ActivateFocused {
        seat: SEAT,
        source: UiActivationSource::Keyboard,
    }
}

fn assert_keyboard_activation(
    session: &mut UiInteractionSession,
    journal: &UiFrameJournal,
    route: UiRouteStamp,
    root: &str,
    action_name: &str,
) {
    let outcome = session
        .dispatch(journal, route, keyboard_activation())
        .unwrap();
    let batch = outcome.consumed().unwrap();
    assert!(!batch.is_reconciliation());
    let target = exact_key(root, action_name);
    let activation = UiRoutedEventKind::ActivateRequested {
        action: UiActionId::new(action_name).unwrap(),
        source: UiActivationSource::Keyboard,
    };
    let targets = batch
        .events()
        .iter()
        .filter(|event| event.phase() == UiRoutePhase::Target)
        .map(|event| (event.target(), event.kind()))
        .collect::<Vec<_>>();
    assert_eq!(targets, vec![(&target, &activation)]);
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
    let mut journal = hosted_journal();
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
    let reconciled = session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .unwrap();
    assert!(reconciled.events().is_empty());
    let retained = session
        .semantic_snapshot_with_action_authority(&journal, disabled_route, SEAT, |_, _| true)
        .unwrap();
    let target = exact_key("base", "first");
    let first = retained
        .nodes()
        .iter()
        .find(|node| node.key() == &target)
        .unwrap();
    assert!(!first.state().enabled());
    assert!(!first.state().focusable());
    assert!(first.state().focused());
    assert!(first.actions().is_empty());
    for route in [disabled_route, initial_route] {
        let outcome = session
            .dispatch(&journal, route, keyboard_activation())
            .unwrap();
        assert_eq!(
            outcome.rejection(),
            Some(&UiNonActivation::TargetDisabled {
                target: target.clone(),
            })
        );
    }

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
        .find(|node| node.key() == &target)
        .unwrap();
    assert!(first.state().enabled());
    assert!(first.state().focusable());
    assert!(first.state().focused());
    assert_keyboard_activation(&mut session, &journal, reenabled_route, "base", "first");
}

#[test]
fn disabled_middle_focus_remains_the_origin_for_sequential_and_directional_navigation() {
    for (direction, expected) in [
        (UiNavigation::Next, "last"),
        (UiNavigation::Previous, "first"),
        (UiNavigation::Right, "last"),
    ] {
        let mut compiler = compiler();
        let mut journal = hosted_journal();
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

fn compile_for_slot(
    compiler: &mut UiCompiler,
    snapshot: &UiAuthoringSnapshot,
    slot_generation: u64,
    route_revision: u64,
    geometry_generation: u64,
    presentation_revision: u64,
) -> Arc<UiFrame> {
    let transform =
        UiSurfaceContentTransform::direct(UiSize::new(320.0, 180.0).unwrap(), 1.0).unwrap();
    let environment = UiEnvironment::for_surface(
        SURFACE,
        transform,
        UiSurfaceGenerations::new(
            slot_generation,
            route_revision,
            geometry_generation,
            presentation_revision,
        )
        .unwrap(),
        1,
        1,
    )
    .unwrap();
    compiler
        .compile(snapshot, environment, UiTheme::default())
        .unwrap()
}

fn compiler() -> UiCompiler {
    UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap()
}

fn submit(journal: &mut HostedJournal, frame: Arc<UiFrame>, epoch: u64) -> UiRouteStamp {
    let environment = frame.environment();
    match journal.install_surface(
        environment.surface_id(),
        environment.slot_generation().get(),
    ) {
        Ok(()) | Err(nara_ui_kernel::UiJournalError::SurfaceAlreadyInstalled { .. }) => {}
        Err(error) => panic!("test surface should install explicitly: {error}"),
    }
    UiRouteStamp::from_receipt(
        journal
            .authority
            .submit(&mut journal.journal, frame, epoch)
            .unwrap(),
    )
}

fn move_pointer(position: UiPoint) -> UiInputEvent {
    UiInputEvent::Pointer(UiPointerInput::moved(SEAT, POINTER, position))
}

fn pointer_button(phase: UiButtonPhase, position: UiPoint) -> UiInputEvent {
    UiInputEvent::Pointer(UiPointerInput::button(
        SEAT,
        POINTER,
        UiPointerButton::Primary,
        phase,
        Some(position),
    ))
}

#[test]
fn initial_modal_candidate_stages_owner_focus_without_input() {
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot(true), 1);
    let session = UiInteractionSession::default();
    let staged = session
        .stage_candidate_presentation(SURFACE, SEAT, None, &frame, &[])
        .unwrap()
        .expect("the endpoint seat and modal focus should change candidate state");
    let retry = exact_key("modal", "retry");
    assert!(staged.dispatch().events().iter().any(|event| {
        event.target() == &retry
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::FocusChanged { focused: true }
    }));
    assert!(
        staged
            .dispatch()
            .events()
            .iter()
            .all(|event| { !matches!(event.kind(), UiRoutedEventKind::ActivateRequested { .. }) })
    );

    let (staged_session, _) = staged.into_parts();
    let mut journal = hosted_journal();
    let route = submit(&mut journal, frame, 10);
    let semantics = staged_session
        .semantic_snapshot(&journal, route, SEAT)
        .unwrap();
    assert!(
        semantics
            .nodes()
            .iter()
            .find(|node| node.key() == &retry)
            .unwrap()
            .state()
            .focused()
    );
}

#[test]
fn base_to_modal_candidate_transition_focuses_without_prior_input() {
    let mut compiler = compiler();
    let base = compile(&mut compiler, &snapshot(false), 1);
    let initial = UiInteractionSession::default()
        .stage_candidate_presentation(SURFACE, SEAT, None, &base, &[])
        .unwrap()
        .expect("the endpoint seat should be admitted before initial presentation");
    assert!(initial.dispatch().events().is_empty());
    let (session, _) = initial.into_parts();

    let modal = compile(&mut compiler, &snapshot(true), 2);
    let staged = session
        .stage_candidate_presentation(SURFACE, SEAT, Some(&base), &modal, &[])
        .unwrap()
        .expect("entering a modal should reconcile focus before presentation");
    let retry = exact_key("modal", "retry");
    assert!(staged.dispatch().events().iter().any(|event| {
        event.target() == &retry
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::FocusChanged { focused: true }
    }));

    let (staged_session, _) = staged.into_parts();
    let mut journal = hosted_journal();
    let route = submit(&mut journal, modal, 10);
    let semantics = staged_session
        .semantic_snapshot(&journal, route, SEAT)
        .unwrap();
    assert_eq!(
        semantics
            .nodes()
            .iter()
            .filter(|node| node.state().focused())
            .map(|node| node.key())
            .collect::<Vec<_>>(),
        vec![&retry]
    );
}

#[test]
fn initial_non_modal_candidate_does_not_invent_focus() {
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot(false), 1);
    let staged = UiInteractionSession::default()
        .stage_candidate_presentation(SURFACE, SEAT, None, &frame, &[])
        .unwrap()
        .expect("the endpoint seat should be admitted even without a modal");
    assert!(staged.dispatch().events().is_empty());

    let (staged_session, _) = staged.into_parts();
    let mut journal = hosted_journal();
    let route = submit(&mut journal, frame, 10);
    let semantics = staged_session
        .semantic_snapshot(&journal, route, SEAT)
        .unwrap();
    assert!(semantics.nodes().iter().all(|node| !node.state().focused()));
}

#[test]
fn focus_visibility_modal_entry_and_navigation_can_activate_a_partly_visible_button() {
    let snapshot = UiAuthoringSnapshotBuilder::new(11)
        .with_root(
            UiRoot::new(root_key("modal"), UiLayer::Modal, UiStyle::fill()).with_children([
                button("outside", "Outside", "outside", 190.0, 200.0, true),
                button("disabled", "Disabled", "disabled", 110.0, 70.0, false),
                clipping_parent(
                    300.0,
                    120.0,
                    60.0,
                    30.0,
                    button("retry", "Retry", "retry", -10.0, -10.0, true),
                ),
                button("back", "Back", "back", 130.0, 120.0, true),
            ]),
        )
        .build()
        .unwrap();
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot, 1);
    let staged = UiInteractionSession::default()
        .stage_candidate_presentation(SURFACE, SEAT, None, &frame, &[])
        .unwrap()
        .expect("modal entry should select the first eligible focus target");
    let retry = exact_key("modal", "retry");
    let focused = staged
        .dispatch()
        .events()
        .iter()
        .filter(|event| {
            event.phase() == UiRoutePhase::Target
                && event.kind() == &UiRoutedEventKind::FocusChanged { focused: true }
        })
        .map(|event| event.target())
        .collect::<Vec<_>>();
    assert_eq!(focused, vec![&retry]);

    let (mut session, _) = staged.into_parts();
    let mut journal = hosted_journal();
    let route = submit(&mut journal, frame, 10);
    let semantics = session
        .semantic_snapshot_with_action_authority(&journal, route, SEAT, |_, _| true)
        .unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &retry)
        .unwrap();
    assert!(retry.state().focused());
    assert!(retry.state().focusable());
    assert_eq!(retry.actions(), &[UiActionId::new("retry").unwrap()]);
    assert_eq!(
        retry.rect(),
        UiRect::from_origin_size(300.0, 120.0, 20.0, 30.0).unwrap()
    );
    let physical = retry.physical_rect();
    assert_eq!(
        (
            physical.min_x(),
            physical.min_y(),
            physical.max_x(),
            physical.max_y()
        ),
        (300, 120, 320, 150)
    );
    assert!(
        semantics
            .nodes()
            .iter()
            .all(|node| node.key() != &exact_key("modal", "outside"))
    );
    assert_keyboard_activation(&mut session, &journal, route, "modal", "retry");
    for (direction, expected) in [
        (UiNavigation::Previous, "back"),
        (UiNavigation::Right, "retry"),
    ] {
        let navigation = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::Navigate {
                    seat: SEAT,
                    direction,
                },
            )
            .unwrap();
        assert!(navigation.consumed().is_some());
        assert_keyboard_activation(&mut session, &journal, route, "modal", expected);
    }
}

#[test]
fn pointer_capture_routes_capture_target_bubble_and_release_outside() {
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot(false), 1);
    let mut journal = hosted_journal();
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
            UiInputEvent::Pointer(UiPointerInput::cancelled(SEAT, POINTER)),
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
    let mut journal = hosted_journal();
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
    let mut journal = hosted_journal();
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
    let mut journal = hosted_journal();
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
    let mut journal = hosted_journal();
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
fn focus_visibility_drops_a_modal_restore_target_while_it_is_offscreen() {
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let initial = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 1),
        10,
    );
    let mut session = UiInteractionSession::default();
    let focused = session
        .dispatch(
            &journal,
            initial,
            UiInputEvent::FocusTarget {
                seat: SEAT,
                target: exact_key("base", "second"),
            },
        )
        .unwrap();
    assert!(focused.consumed().is_some());

    let modal = UiAuthoringSnapshotBuilder::new(2)
        .with_root(
            UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_children([
                button("first", "First", "first", 10.0, 10.0, true),
                button("second", "Second", "second", 400.0, 10.0, true),
            ]),
        )
        .with_root(modal_root())
        .build()
        .unwrap();
    let modal_route = submit(&mut journal, compile(&mut compiler, &modal, 2), 11);
    session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .unwrap();
    assert_keyboard_activation(&mut session, &journal, modal_route, "modal", "retry");

    let restored = submit(
        &mut journal,
        compile(&mut compiler, &snapshot(false), 3),
        12,
    );
    let reconciliation = session
        .dispatch(&journal, restored, keyboard_activation())
        .unwrap();
    let batch = reconciliation.consumed().unwrap();
    assert!(batch.is_reconciliation());
    assert!(
        batch
            .events()
            .iter()
            .all(|event| !matches!(event.kind(), UiRoutedEventKind::ActivateRequested { .. }))
    );
    assert_keyboard_activation(&mut session, &journal, restored, "base", "first");
}

#[test]
fn candidate_reconciliation_is_staged_without_mutating_the_presented_session() {
    let mut compiler = compiler();
    let initial_frame = compile(&mut compiler, &snapshot(false), 1);
    let mut journal = hosted_journal();
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
            SEAT,
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
    let mut journal = hosted_journal();
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
    let mut journal = hosted_journal();
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
        .with_limits(limits)
        .with_root(
            UiRoot::new(root_key("small"), UiLayer::Base, UiStyle::fill())
                .with_child(button("only", "Only", "only", 10.0, 10.0, true)),
        )
        .build()
        .unwrap();
    let mut journal = hosted_journal_with_limits(limits);
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
fn equivalent_pointer_routes_rebind_capture_across_paint_decoration_changes() {
    let mut compiler = compiler();
    let mut journal = hosted_journal();
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

    let decorated = UiAuthoringSnapshotBuilder::new(2)
        .with_root(base_root().with_child(
            UiNode::new(UiStyle::absolute(15.0, 15.0, 1.0, 20.0)).with_visual(
                nara_ui_kernel::UiVisual::role(nara_ui_kernel::UiColorRole::Focus),
            ),
        ))
        .build()
        .unwrap();
    let latest_route = submit(&mut journal, compile(&mut compiler, &decorated, 1), 11);
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
fn pointer_capture_survives_enabling_an_unrelated_target_but_not_disabling_its_owner() {
    let authored = |revision, first_enabled, second_enabled| {
        UiAuthoringSnapshotBuilder::new(revision)
            .with_root(
                UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_children([
                    button("first", "First", "first", 10.0, 10.0, first_enabled),
                    button("second", "Second", "second", 110.0, 10.0, second_enabled),
                ]),
            )
            .build()
            .unwrap()
    };
    for (first_enabled, second_enabled) in [(true, true), (false, false)] {
        let mut compiler = compiler();
        let initial = compile_for_slot(&mut compiler, &authored(1, false, true), 1, 1, 1, 1);
        let successor = compile_for_slot(
            &mut compiler,
            &authored(2, first_enabled, second_enabled),
            1,
            1,
            1,
            1,
        );
        assert_eq!(
            initial.environment().generations(),
            successor.environment().generations()
        );
        assert!(
            initial
                .hit_regions()
                .iter()
                .map(|region| (region.key(), region.rect()))
                .eq(successor
                    .hit_regions()
                    .iter()
                    .map(|region| (region.key(), region.rect())))
        );

        let mut journal = hosted_journal();
        let initial_route = submit(&mut journal, initial, 10);
        let mut session = UiInteractionSession::default();
        session
            .reconcile_latest_presentation(&journal, SURFACE, &[])
            .unwrap()
            .unwrap();
        let target = exact_key("base", "second");
        let position = UiPoint::new(120.0, 20.0);
        let pressed = session
            .dispatch(
                &journal,
                initial_route,
                pointer_button(UiButtonPhase::Pressed, position),
            )
            .unwrap();
        assert!(pressed.consumed().unwrap().events().iter().any(|event| {
            event.target() == &target
                && event.phase() == UiRoutePhase::Target
                && event.kind() == &UiRoutedEventKind::Pressed
        }));

        let successor_route = submit(&mut journal, successor, 11);
        let reconciled = session
            .reconcile_latest_presentation(&journal, SURFACE, &[])
            .unwrap()
            .unwrap();
        assert!(reconciled.is_reconciliation());
        let released = session
            .dispatch(
                &journal,
                successor_route,
                pointer_button(UiButtonPhase::Released, position),
            )
            .unwrap();

        if second_enabled {
            assert!(reconciled.events().is_empty());
            let released = released.consumed().unwrap();
            assert!(!released.is_reconciliation());
            let releases = released
                .events()
                .iter()
                .filter(|event| {
                    event.phase() == UiRoutePhase::Target
                        && event.kind() == &UiRoutedEventKind::Released { inside: true }
                })
                .collect::<Vec<_>>();
            assert_eq!(releases.len(), 1);
            assert_eq!(releases[0].target(), &target);
            assert!(
                released
                    .events()
                    .iter()
                    .all(|event| event.kind() != &UiRoutedEventKind::Cancelled)
            );
        } else {
            assert!(reconciled.events().iter().any(|event| {
                event.target() == &target
                    && event.phase() == UiRoutePhase::Target
                    && event.kind() == &UiRoutedEventKind::Cancelled
            }));
            assert!(reconciled.events().iter().all(|event| {
                !matches!(
                    event.kind(),
                    UiRoutedEventKind::Released { .. }
                        | UiRoutedEventKind::ActivateRequested { .. }
                )
            }));
            assert_eq!(
                released.rejection(),
                Some(&UiNonActivation::NoPointerCapture)
            );
            assert!(released.consumed().is_none());
        }
    }
}

#[test]
fn pointer_capture_cannot_cross_presentation_generations() {
    let mut compiler = compiler();
    let mut journal = hosted_journal();
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
    let stale_activation = session
        .dispatch(&journal, historical_route, keyboard_activation())
        .unwrap();
    assert_eq!(
        stale_activation.rejection(),
        Some(&UiNonActivation::StaleFocus)
    );
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
    let mut journal = hosted_journal();
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
    assert_eq!(rejected.rejection(), Some(&UiNonActivation::StaleFocus));
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
fn historical_activation_preserves_focus_and_same_presentation_navigation() {
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let frame = compile(&mut compiler, &snapshot(false), 1);
    let historical = submit(&mut journal, Arc::clone(&frame), 10);
    let mut session = UiInteractionSession::default();
    let tab = UiInputEvent::Navigate {
        seat: SEAT,
        direction: UiNavigation::Next,
    };
    session.dispatch(&journal, historical, tab.clone()).unwrap();
    assert_keyboard_activation(&mut session, &journal, historical, "base", "first");

    let latest = submit(&mut journal, frame, 11);
    session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap();
    assert_keyboard_activation(&mut session, &journal, historical, "base", "first");

    let navigation = session.dispatch(&journal, historical, tab.clone()).unwrap();
    assert!(navigation.consumed().is_some());
    let stale = session
        .dispatch(&journal, historical, keyboard_activation())
        .unwrap();
    assert_eq!(stale.rejection(), Some(&UiNonActivation::StaleFocus));
    assert_keyboard_activation(&mut session, &journal, latest, "base", "second");
    session.dispatch(&journal, latest, tab).unwrap();
    assert_keyboard_activation(&mut session, &journal, latest, "base", "first");
}

#[test]
fn historical_focus_writes_cannot_redirect_an_intermediate_presentation_activation() {
    let authored = |revision, clip_width| {
        UiAuthoringSnapshotBuilder::new(revision)
            .with_root(
                UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_children([
                    clipping_parent(
                        0.0,
                        0.0,
                        clip_width,
                        80.0,
                        button("first", "First", "first", 10.0, 10.0, true),
                    ),
                    button("second", "Second", "second", 110.0, 10.0, true),
                    button("third", "Third", "third", 210.0, 10.0, true),
                ]),
            )
            .build()
            .unwrap()
    };
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let original = submit(
        &mut journal,
        compile(&mut compiler, &authored(1, 100.0), 1),
        10,
    );
    let mut session = UiInteractionSession::default();
    let tab = UiInputEvent::Navigate {
        seat: SEAT,
        direction: UiNavigation::Next,
    };
    session.dispatch(&journal, original, tab.clone()).unwrap();
    assert_keyboard_activation(&mut session, &journal, original, "base", "first");

    let intermediate = submit(
        &mut journal,
        compile(&mut compiler, &authored(2, 5.0), 1),
        11,
    );
    session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap();
    let latest = submit(
        &mut journal,
        compile(&mut compiler, &authored(3, 5.0), 1),
        12,
    );
    session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap();

    let third = exact_key("base", "third");
    let stale_focus = UiAccessibilityInput::focus_target_from_route(original, SEAT, third.clone());
    let rejected = session
        .dispatch_accessibility(&journal, stale_focus)
        .unwrap();
    assert_eq!(rejected.rejection(), Some(&UiNonActivation::StaleFocus));
    assert_keyboard_activation(&mut session, &journal, intermediate, "base", "second");

    let rejected = session.dispatch(&journal, original, tab).unwrap();
    assert_eq!(rejected.rejection(), Some(&UiNonActivation::StaleFocus));
    assert_keyboard_activation(&mut session, &journal, intermediate, "base", "second");

    let position = UiPoint::new(220.0, 20.0);
    for (phase, kind) in [
        (UiButtonPhase::Pressed, UiRoutedEventKind::Pressed),
        (
            UiButtonPhase::Released,
            UiRoutedEventKind::Released { inside: true },
        ),
    ] {
        let outcome = session
            .dispatch(&journal, original, pointer_button(phase, position))
            .unwrap();
        assert!(outcome.consumed().unwrap().events().iter().any(|event| {
            event.phase() == UiRoutePhase::Target
                && event.target() == &third
                && event.kind() == &kind
        }));
        assert_keyboard_activation(&mut session, &journal, intermediate, "base", "second");
    }

    let compatible_focus = UiAccessibilityInput::focus_target_from_route(intermediate, SEAT, third);
    let focused = session
        .dispatch_accessibility(&journal, compatible_focus)
        .unwrap();
    assert!(focused.consumed().is_some());
    let stale = session
        .dispatch(&journal, intermediate, keyboard_activation())
        .unwrap();
    assert_eq!(stale.rejection(), Some(&UiNonActivation::StaleFocus));
    assert_keyboard_activation(&mut session, &journal, latest, "base", "third");
}

#[test]
fn historical_activation_stays_stale_after_focus_round_trip_and_seat_cancellation() {
    let mut journal = hosted_journal();
    let frame = compile(&mut compiler(), &snapshot(false), 1);
    let historical = submit(&mut journal, Arc::clone(&frame), 10);
    let mut session = UiInteractionSession::default();
    let tab = UiInputEvent::Navigate {
        seat: SEAT,
        direction: UiNavigation::Next,
    };
    session.dispatch(&journal, historical, tab.clone()).unwrap();

    let latest = submit(&mut journal, Arc::clone(&frame), 11);
    for _ in 0..2 {
        session.dispatch(&journal, latest, tab.clone()).unwrap();
        let stale = session
            .dispatch(&journal, historical, keyboard_activation())
            .unwrap();
        assert_eq!(stale.rejection(), Some(&UiNonActivation::StaleFocus));
    }
    assert_keyboard_activation(&mut session, &journal, latest, "base", "first");

    let rejected_focus = session
        .dispatch(
            &journal,
            historical,
            UiInputEvent::FocusTarget {
                seat: SEAT,
                target: exact_key("base", "second"),
            },
        )
        .unwrap();
    assert_eq!(
        rejected_focus.rejection(),
        Some(&UiNonActivation::StaleFocus)
    );
    let stale = session
        .dispatch(&journal, historical, keyboard_activation())
        .unwrap();
    assert_eq!(stale.rejection(), Some(&UiNonActivation::StaleFocus));
    assert_keyboard_activation(&mut session, &journal, latest, "base", "first");

    let successor = submit(&mut journal, frame, 12);
    for reset_route in [historical, successor] {
        let mut session = session.clone();
        let cancelled = session
            .dispatch(
                &journal,
                reset_route,
                UiInputEvent::SeatCancelled { seat: SEAT },
            )
            .unwrap();
        assert!(cancelled.consumed().is_some());
        let stale = session
            .dispatch(&journal, latest, keyboard_activation())
            .unwrap();
        assert_eq!(stale.rejection(), Some(&UiNonActivation::StaleFocus));
        let unfocused = session
            .dispatch(&journal, successor, keyboard_activation())
            .unwrap();
        assert_eq!(unfocused.rejection(), Some(&UiNonActivation::NoFocusTarget));

        session.dispatch(&journal, successor, tab.clone()).unwrap();
        assert_keyboard_activation(&mut session, &journal, successor, "base", "first");
    }
}

#[test]
fn empty_reconciliation_is_separate_when_the_presentation_generation_changed() {
    let mut compiler = compiler();
    let mut journal = hosted_journal();
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
fn nonfinite_input_validation_precedes_pending_presentation_reconciliation() {
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let route = submit(&mut journal, compile(&mut compiler, &snapshot(true), 1), 10);
    let mut session = UiInteractionSession::default();
    let invalid_move = move_pointer(UiPoint::new(f32::NAN, 20.0));

    assert_eq!(
        session
            .dispatch(&journal, route, invalid_move.clone())
            .unwrap_err(),
        UiDispatchError::NonFinitePointerPosition
    );
    assert_eq!(
        session.dispatch(&journal, route, invalid_move).unwrap_err(),
        UiDispatchError::NonFinitePointerPosition
    );
    let pending = session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .expect("non-finite input did not perform pending modal reconciliation");
    assert!(pending.is_reconciliation());
    assert!(!session.has_pointer_facts(SURFACE));
}

#[test]
fn retired_targets_and_invalidated_geometry_cannot_activate() {
    let mut compiler = compiler();
    let mut journal = hosted_journal();
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

    let mut fresh_journal = hosted_journal();
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
fn geometry_invalidation_is_scoped_to_the_surface_slot_lineage() {
    for (name, input) in [
        ("pointer", move_pointer(UiPoint::new(20.0, 20.0))),
        ("keyboard activation", keyboard_activation()),
        (
            "navigate",
            UiInputEvent::Navigate {
                seat: SEAT,
                direction: UiNavigation::Next,
            },
        ),
        (
            "focus",
            UiInputEvent::FocusTarget {
                seat: SEAT,
                target: exact_key("base", "first"),
            },
        ),
        (
            "semantic",
            UiInputEvent::SemanticAction {
                seat: SEAT,
                target: exact_key("base", "first"),
                action: UiActionId::new("first").unwrap(),
            },
        ),
    ] {
        let mut compiler = compiler();
        let mut journal = hosted_journal();
        let predecessor = submit(
            &mut journal,
            compile_for_slot(&mut compiler, &snapshot(false), 1, 1, 1, 1),
            100,
        );
        let mut session = UiInteractionSession::default();
        session
            .dispatch(
                &journal,
                predecessor,
                UiInputEvent::FocusTarget {
                    seat: SEAT,
                    target: exact_key("base", "first"),
                },
            )
            .unwrap();
        session
            .dispatch(&journal, predecessor, UiInputEvent::GeometryInvalidated)
            .unwrap();
        let stale = session
            .dispatch(&journal, predecessor, input.clone())
            .unwrap();
        assert_eq!(stale.rejection(), Some(&UiNonActivation::StaleGeometry));
        journal.advance_surface_slot(SURFACE, 1, 2).unwrap();
        let successor = submit(
            &mut journal,
            compile_for_slot(&mut compiler, &snapshot(false), 2, 2, 2, 1),
            1,
        );

        assert_eq!(
            session
                .dispatch(&journal, predecessor, input.clone())
                .unwrap_err(),
            UiDispatchError::Journal(UiJournalError::SurfaceRetired {
                surface: SURFACE,
                presentation_epoch: 100,
            })
        );

        let outcome = session.dispatch(&journal, successor, input).unwrap();
        assert!(
            outcome.rejection().is_none(),
            "{name} input must not inherit stale geometry from the predecessor slot: {outcome:?}"
        );
    }
}

#[test]
fn retained_predecessor_target_cannot_overwrite_successor_geometry_invalidation() {
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let predecessor = submit(
        &mut journal,
        compile_for_slot(&mut compiler, &snapshot(false), 1, 1, 1, 1),
        100,
    );
    let retained_predecessor = journal.resolve(predecessor).unwrap();
    journal.advance_surface_slot(SURFACE, 1, 2).unwrap();
    let successor = submit(
        &mut journal,
        compile_for_slot(&mut compiler, &snapshot(false), 2, 2, 2, 1),
        1,
    );
    let latest = journal.resolve(successor).unwrap();
    let mut session = UiInteractionSession::default();
    session
        .dispatch(&journal, successor, UiInputEvent::GeometryInvalidated)
        .unwrap();

    assert!(matches!(
        session.dispatch_resolved(
            &retained_predecessor,
            &latest,
            predecessor,
            UiInputEvent::GeometryInvalidated,
        ),
        Err(UiDispatchError::ResolvedRouteMismatch)
    ));

    let stale = session
        .dispatch(&journal, successor, move_pointer(UiPoint::new(20.0, 20.0)))
        .unwrap();
    assert_eq!(stale.rejection(), Some(&UiNonActivation::StaleGeometry));
}

#[test]
fn semantic_snapshot_uses_the_exact_frame_and_interaction_state() {
    let mut compiler = compiler();
    let mut journal = hosted_journal();
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
            (key == &exact_key("modal", "retry") && action == &UiActionId::new("retry").unwrap())
                || key == &exact_key("base", "first")
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
    assert!(base.actions().is_empty());
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
        .semantic_snapshot_with_action_authority(&journal, latest_route, SEAT, |_, _| true)
        .unwrap();
    let disabled_retry = latest_semantics
        .nodes()
        .iter()
        .find(|node| node.key() == &exact_key("modal", "retry"))
        .unwrap();
    assert!(!disabled_retry.state().enabled());
    assert!(!disabled_retry.state().focusable());
    assert!(disabled_retry.actions().is_empty());
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
fn focus_visibility_rejects_buttons_with_empty_surface_or_ancestor_intersection() {
    for layer in [UiLayer::Base, UiLayer::Modal] {
        let snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(root_key("visibility"), layer, UiStyle::fill()).with_children([
                    button("outside", "Outside", "outside", 400.0, 20.0, true),
                    clipping_parent(
                        10.0,
                        70.0,
                        40.0,
                        60.0,
                        button("clipped", "Clipped", "clipped", 50.0, 0.0, true),
                    ),
                    clipping_parent(
                        330.0,
                        100.0,
                        40.0,
                        60.0,
                        button("disjoint", "Disjoint", "disjoint", -20.0, 0.0, true),
                    ),
                ]),
            )
            .build()
            .unwrap();
        let frame = compile(&mut compiler(), &snapshot, 1);
        assert_eq!(frame.blocks_unhandled_input(), layer == UiLayer::Modal);
        assert!(frame.focus_order().is_empty());
        assert!(frame.semantics().is_empty());
        let staged = UiInteractionSession::default()
            .stage_candidate_presentation(SURFACE, SEAT, None, &frame, &[])
            .unwrap()
            .unwrap();
        assert!(staged.dispatch().events().is_empty());
        let (mut session, _) = staged.into_parts();
        let mut journal = hosted_journal();
        let route = submit(&mut journal, frame, 10);

        let semantics = session
            .semantic_snapshot_with_action_authority(&journal, route, SEAT, |_, _| true)
            .unwrap();
        assert!(semantics.nodes().is_empty());
        let navigation = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::Navigate {
                    seat: SEAT,
                    direction: UiNavigation::Next,
                },
            )
            .unwrap();
        let activation = session
            .dispatch(&journal, route, keyboard_activation())
            .unwrap();
        assert_eq!(
            navigation.rejection(),
            Some(&UiNonActivation::NoFocusTarget)
        );
        assert_eq!(
            activation.rejection(),
            Some(&UiNonActivation::NoFocusTarget)
        );

        for name in ["outside", "clipped", "disjoint"] {
            let target = exact_key("visibility", name);
            for request in [
                UiAccessibilityInput::focus_target_from_route(route, SEAT, target.clone()),
                UiAccessibilityInput::semantic_action_from_route(
                    route,
                    SEAT,
                    target.clone(),
                    UiActionId::new(name).unwrap(),
                ),
            ] {
                let outcome = session.dispatch_accessibility(&journal, request).unwrap();
                assert_eq!(
                    outcome.rejection(),
                    Some(&UiNonActivation::TargetHidden {
                        target: target.clone(),
                    })
                );
            }
        }
    }
}

#[test]
fn focus_visibility_directional_navigation_uses_clipped_origin_and_destination_centers() {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_children([
                button("reference", "Reference", "reference", 95.0, 10.0, true),
                clipping_parent(
                    140.0,
                    70.0,
                    20.0,
                    40.0,
                    button("cropped", "Cropped", "cropped", -60.0, 0.0, true),
                ),
                button("left", "Left", "left", 65.0, 120.0, true),
            ]),
        )
        .build()
        .unwrap();
    let mut journal = hosted_journal();
    let route = submit(&mut journal, compile(&mut compiler(), &snapshot, 1), 10);
    let mut session = UiInteractionSession::default();
    for (direction, expected) in [
        (UiNavigation::Next, "reference"),
        (UiNavigation::Right, "cropped"),
        (UiNavigation::Left, "reference"),
    ] {
        let focused = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::Navigate {
                    seat: SEAT,
                    direction,
                },
            )
            .unwrap();
        assert!(focused.consumed().is_some());
        assert_keyboard_activation(&mut session, &journal, route, "base", expected);
    }
}

#[test]
fn focus_visibility_requires_a_nonempty_snapped_physical_intersection() {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_children([
                button("edge", "Edge", "edge", 319.75, 20.0, true),
                clipping_parent(
                    80.0,
                    20.0,
                    0.25,
                    40.0,
                    button("sliver", "Sliver", "sliver", 0.0, 0.0, true),
                ),
                clipping_parent(
                    180.0,
                    20.0,
                    1.0,
                    40.0,
                    button("pixel", "Pixel", "pixel", 0.0, 0.0, true),
                ),
            ]),
        )
        .build()
        .unwrap();
    let frame = compile(&mut compiler(), &snapshot, 1);
    let sliver_key = exact_key("base", "sliver");
    let sliver = frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(&sliver_key))
        .unwrap();
    let clipped = sliver
        .rect()
        .intersect(sliver.clip_rect().unwrap())
        .unwrap();
    assert_eq!(clipped.size().width(), 0.25);
    assert!(!sliver.physical_rect().is_empty());

    let mut journal = hosted_journal();
    let route = submit(&mut journal, frame, 10);
    let mut session = UiInteractionSession::default();
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
    assert_keyboard_activation(&mut session, &journal, route, "base", "pixel");

    let semantics = session
        .semantic_snapshot_with_action_authority(&journal, route, SEAT, |_, _| true)
        .unwrap();
    assert_eq!(semantics.nodes().len(), 1);
    let pixel = &semantics.nodes()[0];
    assert_eq!(pixel.key(), &exact_key("base", "pixel"));
    assert!(pixel.state().focused());
    assert_eq!(pixel.physical_rect().width(), 1);
    let action = UiAccessibilityInput::semantic_action_from_route(
        route,
        SEAT,
        sliver_key.clone(),
        UiActionId::new("sliver").unwrap(),
    );
    let rejected = session.dispatch_accessibility(&journal, action).unwrap();
    assert_eq!(
        rejected.rejection(),
        Some(&UiNonActivation::TargetHidden { target: sliver_key })
    );
}

#[test]
fn focus_visibility_reconciles_focus_and_capture_when_a_control_leaves_view() {
    let authored = |revision, offset, clip_width| {
        UiAuthoringSnapshotBuilder::new(revision)
            .with_root(
                UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_children([
                    clipping_parent(
                        250.0,
                        20.0,
                        clip_width,
                        80.0,
                        button("moving", "Moving", "moving", offset, 0.0, true),
                    ),
                    button("backup", "Backup", "backup", 10.0, 20.0, true),
                ]),
            )
            .build()
            .unwrap()
    };
    for (offset, clip_width) in [(90.0, 120.0), (20.0, 16.0)] {
        let mut compiler = compiler();
        let mut journal = hosted_journal();
        let initial = compile(&mut compiler, &authored(1, 20.0, 120.0), 1);
        let initial_route = submit(&mut journal, Arc::clone(&initial), 10);
        let mut session = UiInteractionSession::default();
        let position = UiPoint::new(275.0, 25.0);
        let pressed = session
            .dispatch(
                &journal,
                initial_route,
                pointer_button(UiButtonPhase::Pressed, position),
            )
            .unwrap();
        assert!(pressed.consumed().is_some());

        let successor = compile(&mut compiler, &authored(2, offset, clip_width), 1);
        let staged = session
            .stage_candidate_presentation(SURFACE, SEAT, Some(&initial), &successor, &[])
            .unwrap()
            .unwrap();
        let moving = exact_key("base", "moving");
        let backup = exact_key("base", "backup");
        let targets = staged
            .dispatch()
            .events()
            .iter()
            .filter(|event| event.phase() == UiRoutePhase::Target)
            .map(|event| (event.target(), event.kind()))
            .collect::<Vec<_>>();
        assert_eq!(
            targets,
            vec![
                (&moving, &UiRoutedEventKind::Cancelled),
                (&moving, &UiRoutedEventKind::HoverChanged { hovered: false }),
                (&moving, &UiRoutedEventKind::FocusChanged { focused: false }),
                (&backup, &UiRoutedEventKind::FocusChanged { focused: true }),
            ]
        );
        let before = session
            .semantic_snapshot(&journal, initial_route, SEAT)
            .unwrap();
        let control = before
            .nodes()
            .iter()
            .find(|node| node.key() == &moving)
            .unwrap();
        assert!(control.state().focused());
        assert!(control.state().pressed());
        assert_keyboard_activation(&mut session, &journal, initial_route, "base", "moving");

        let (mut staged_session, staged_cleanup) = staged.into_parts();
        let route = submit(&mut journal, successor, 11);
        let staged_adoption = staged_session
            .reconcile_latest_presentation(&journal, SURFACE, &[])
            .unwrap()
            .unwrap();
        assert!(staged_adoption.events().is_empty());
        let cleanup = session
            .reconcile_latest_presentation(&journal, SURFACE, &[])
            .unwrap()
            .unwrap();
        assert_eq!(cleanup.events(), staged_cleanup.events());
        for session in [&mut session, &mut staged_session] {
            for _ in 0..2 {
                let stale = session
                    .dispatch(&journal, initial_route, keyboard_activation())
                    .unwrap();
                assert_eq!(stale.rejection(), Some(&UiNonActivation::StaleFocus));
            }
        }
        assert_keyboard_activation(&mut staged_session, &journal, route, "base", "backup");
        let after = session.semantic_snapshot(&journal, route, SEAT).unwrap();
        assert_eq!(after.nodes().len(), 1);
        assert_eq!(after.nodes()[0].key(), &backup);
        assert!(after.nodes()[0].state().focused());

        let released = session
            .dispatch(
                &journal,
                route,
                pointer_button(UiButtonPhase::Released, position),
            )
            .unwrap();
        assert_eq!(
            released.rejection(),
            Some(&UiNonActivation::NoPointerCapture)
        );
        let hidden_action = UiAccessibilityInput::semantic_action_from_route(
            initial_route,
            SEAT,
            moving.clone(),
            UiActionId::new("moving").unwrap(),
        );
        let hidden = session
            .dispatch_accessibility(&journal, hidden_action)
            .unwrap();
        assert_eq!(
            hidden.rejection(),
            Some(&UiNonActivation::TargetHidden { target: moving })
        );
        let navigated = session
            .dispatch(
                &journal,
                initial_route,
                UiInputEvent::Navigate {
                    seat: SEAT,
                    direction: UiNavigation::Previous,
                },
            )
            .unwrap();
        assert!(navigated.consumed().unwrap().events().is_empty());
        assert_keyboard_activation(&mut session, &journal, route, "base", "backup");
    }
}

#[test]
fn unsupported_pointer_buttons_do_not_consume_pointer_capacity() {
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot(false), 1);
    let mut journal = hosted_journal();
    let route = submit(&mut journal, frame, 10);
    let mut session = UiInteractionSession::default();

    for pointer in 0..64 {
        let rejected = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::Pointer(UiPointerInput::button(
                    SEAT,
                    UiPointerId::new(pointer),
                    UiPointerButton::Secondary,
                    UiButtonPhase::Pressed,
                    Some(UiPoint::new(20.0, 20.0)),
                )),
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
            UiInputEvent::Pointer(UiPointerInput::button(
                SEAT,
                UiPointerId::new(1_000),
                UiPointerButton::Primary,
                UiButtonPhase::Pressed,
                Some(UiPoint::new(20.0, 20.0)),
            )),
        )
        .unwrap();
    assert!(accepted.rejection().is_none());
}

#[test]
fn one_surface_has_one_seat_and_one_pointer_owner() {
    let mut compiler = compiler();
    let frame = compile(&mut compiler, &snapshot(false), 1);
    let mut journal = hosted_journal();
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
                UiInputEvent::Pointer(UiPointerInput::moved(
                    SEAT,
                    UiPointerId::new(1),
                    UiPoint::new(20.0, 20.0),
                )),
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
    let mut journal = hosted_journal();
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
    let unsupported = UiInputEvent::Pointer(UiPointerInput::button(
        SEAT,
        POINTER,
        UiPointerButton::Secondary,
        UiButtonPhase::Pressed,
        Some(UiPoint::new(20.0, 20.0)),
    ));

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
    let mut journal = hosted_journal();
    let route = submit(&mut journal, frame, 10);
    let mut session = UiInteractionSession::default();

    for pointer in 0..64 {
        let rejected = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::Pointer(UiPointerInput::button(
                    SEAT,
                    UiPointerId::new(pointer),
                    UiPointerButton::Primary,
                    UiButtonPhase::Pressed,
                    None,
                )),
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
            UiInputEvent::Pointer(UiPointerInput::button(
                SEAT,
                UiPointerId::new(1_000),
                UiPointerButton::Primary,
                UiButtonPhase::Pressed,
                Some(UiPoint::new(20.0, 20.0)),
            )),
        )
        .unwrap();
    assert!(accepted.consumed().is_some());
}

#[test]
fn geometry_invalidation_requires_a_fresh_pointer_position() {
    let mut compiler = compiler();
    let mut journal = hosted_journal();
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
            UiInputEvent::Pointer(UiPointerInput::button(
                SEAT,
                POINTER,
                UiPointerButton::Primary,
                UiButtonPhase::Pressed,
                None,
            )),
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
    let mut journal = hosted_journal();
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

fn assert_successor_cancels_pointer_capture(initial: Arc<UiFrame>, successor: Arc<UiFrame>) {
    assert_eq!(
        initial.environment().generations(),
        successor.environment().generations()
    );
    let position = UiPoint::new(20.0, 20.0);
    let initial_hit = initial.hit_test(position).unwrap();
    let target = initial_hit.key().clone();
    let successor_hit = successor.hit_test(position).unwrap();
    assert_eq!(successor_hit.key(), &target);
    assert_eq!(successor_hit.rect(), initial_hit.rect());

    let mut journal = hosted_journal();
    let initial_route = submit(&mut journal, initial, 10);
    let mut session = UiInteractionSession::default();
    session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .unwrap();
    let pressed = session
        .dispatch(
            &journal,
            initial_route,
            pointer_button(UiButtonPhase::Pressed, position),
        )
        .unwrap();
    assert!(pressed.consumed().unwrap().events().iter().any(|event| {
        event.target() == &target
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Pressed
    }));

    let successor_route = submit(&mut journal, successor, 11);
    let cancelled = session
        .reconcile_latest_presentation(&journal, SURFACE, &[])
        .unwrap()
        .unwrap();
    assert!(cancelled.is_reconciliation());
    assert!(cancelled.events().iter().any(|event| {
        event.target() == &target
            && event.phase() == UiRoutePhase::Target
            && event.kind() == &UiRoutedEventKind::Cancelled
    }));
    assert!(cancelled.events().iter().all(|event| {
        !matches!(
            event.kind(),
            UiRoutedEventKind::Released { .. } | UiRoutedEventKind::ActivateRequested { .. }
        )
    }));

    let released = session
        .dispatch(
            &journal,
            successor_route,
            pointer_button(UiButtonPhase::Released, position),
        )
        .unwrap();
    assert_eq!(
        released.rejection(),
        Some(&UiNonActivation::NoPointerCapture)
    );
    assert!(released.consumed().is_none());
}

#[test]
fn keyed_ancestor_reparenting_cancels_pointer_capture_with_fixed_generations() {
    let ancestor = UiNode::new(UiStyle::absolute(0.0, 0.0, 320.0, 180.0))
        .with_key(node_key("ancestor"))
        .with_child(button("first", "First", "first", 10.0, 10.0, true));
    let parent =
        UiNode::new(UiStyle::absolute(0.0, 0.0, 320.0, 180.0)).with_key(node_key("parent"));
    let initial = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill())
                .with_child(parent.clone().with_child(ancestor.clone())),
        )
        .build()
        .unwrap();
    let successor = UiAuthoringSnapshotBuilder::new(2)
        .with_root(
            UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill())
                .with_children([parent, ancestor]),
        )
        .build()
        .unwrap();
    let mut compiler = compiler();
    let initial = compile_for_slot(&mut compiler, &initial, 1, 1, 1, 1);
    let successor = compile_for_slot(&mut compiler, &successor, 1, 1, 1, 1);
    assert_eq!(initial.hit_regions(), successor.hit_regions());

    assert_successor_cancels_pointer_capture(initial, successor);
}

#[test]
fn overlapping_hit_region_reordering_cancels_pointer_capture_with_fixed_generations() {
    let authored = |revision, lower: [&str; 2]| {
        UiAuthoringSnapshotBuilder::new(revision)
            .with_root(
                UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill())
                    .with_children(lower.map(|name| button(name, name, name, 10.0, 10.0, true)))
                    .with_child(button("first", "First", "first", 10.0, 10.0, true)),
            )
            .build()
            .unwrap()
    };
    let mut compiler = compiler();
    let initial = compile_for_slot(&mut compiler, &authored(1, ["second", "third"]), 1, 1, 1, 1);
    let successor = compile_for_slot(&mut compiler, &authored(2, ["third", "second"]), 1, 1, 1, 1);
    assert_eq!(initial.hit_regions()[0], successor.hit_regions()[0]);
    assert_eq!(initial.hit_regions()[1], successor.hit_regions()[2]);
    assert_eq!(initial.hit_regions()[2], successor.hit_regions()[1]);

    assert_successor_cancels_pointer_capture(initial, successor);
}

#[test]
fn clip_rect_changes_cancel_pointer_capture_with_fixed_generations() {
    let authored = |revision, width| {
        UiAuthoringSnapshotBuilder::new(revision)
            .with_root(
                UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_child(
                    UiNode::new(
                        UiStyle::absolute(0.0, 0.0, width, 100.0)
                            .with_overflow(nara_ui_kernel::UiOverflow::Clip),
                    )
                    .with_child(button("first", "First", "first", 10.0, 10.0, true)),
                ),
            )
            .build()
            .unwrap()
    };
    let mut compiler = compiler();
    let initial = compile_for_slot(&mut compiler, &authored(1, 80.0), 1, 1, 1, 1);
    let successor = compile_for_slot(&mut compiler, &authored(2, 60.0), 1, 1, 1, 1);
    assert!(initial.hit_test(UiPoint::new(70.0, 20.0)).is_some());
    assert!(successor.hit_test(UiPoint::new(70.0, 20.0)).is_none());

    assert_successor_cancels_pointer_capture(initial, successor);
}

#[test]
fn content_transform_changes_cancel_pointer_capture_with_fixed_generations() {
    let mut compiler = compiler();
    let authored = snapshot(false);
    let initial = compile_for_slot(&mut compiler, &authored, 1, 1, 1, 1);
    let transform = UiSurfaceContentTransform::new(
        UiSize::new(320.0, 180.0).unwrap(),
        UiSize::new(320.0, 180.0).unwrap(),
        nara_ui_kernel::UiSafeAreaInsets::default(),
        2.0,
    )
    .unwrap();
    let environment = UiEnvironment::for_surface(
        SURFACE,
        transform,
        UiSurfaceGenerations::new(1, 1, 1, 1).unwrap(),
        1,
        1,
    )
    .unwrap();
    let successor = compiler
        .compile(&authored, environment, UiTheme::default())
        .unwrap();
    assert_eq!(initial.layouts(), successor.layouts());
    assert_eq!(initial.hit_regions(), successor.hit_regions());

    assert_successor_cancels_pointer_capture(initial, successor);
}
