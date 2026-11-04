use super::*;
use nara_ui_kernel::{
    UiCaptureContinuity, UiDispatchBatch, UiDispatchOutcome, UiKey, UiModifiers, UiOverflow,
    UiPointerButtons, UiPointerInputKind, UiRootStatus, UiRoutedEvent, UiScrollDelta,
};

fn viewport_input(continuity: UiCaptureContinuity) -> UiInteraction {
    UiInteraction::focus_target()
        .with_pointer_motion(true)
        .with_scroll(true)
        .with_pointer_buttons(UiPointerButtons::new(true, true, true))
        .with_capture_continuity(continuity)
}

fn viewport(input: UiInteraction, pan: f32, child_visible: bool) -> UiNode {
    UiNode::new(UiStyle::absolute(10.0, 10.0, 160.0, 100.0).with_overflow(UiOverflow::Clip))
        .with_key(node_key("viewport"))
        .with_interaction(input)
        .with_semantics(UiSemantics::new(UiSemanticRole::Group, "Canvas").unwrap())
        .with_child(
            UiNode::new(
                UiStyle::absolute(30.0 + pan, 10.0, 80.0, 40.0).with_visible(child_visible),
            )
            .with_key(node_key("moving"))
            .with_interaction(UiInteraction::focus_target()),
        )
}

fn canvas_root(viewport: UiNode) -> UiRoot {
    UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_child(
        UiNode::new(UiStyle::absolute(0.0, 0.0, 320.0, 180.0))
            .with_key(node_key("ancestor"))
            .with_child(viewport),
    )
}

fn canvas_snapshot(
    revision: u64,
    input: UiInteraction,
    pan: f32,
    child_visible: bool,
) -> UiAuthoringSnapshot {
    UiAuthoringSnapshotBuilder::new(revision)
        .with_root(canvas_root(viewport(input, pan, child_visible)))
        .build()
        .unwrap()
}

fn target_events<'a>(
    batch: &'a UiDispatchBatch,
    kind: &UiRoutedEventKind,
) -> Vec<&'a UiRoutedEvent> {
    batch
        .events()
        .iter()
        .filter(|event| event.phase() == UiRoutePhase::Target && event.kind() == kind)
        .collect()
}

fn sample_button(
    button: UiPointerButton,
    phase: UiButtonPhase,
    position: Option<UiPoint>,
) -> UiPointerInput {
    UiPointerInput::button(SEAT, POINTER, button, phase, position)
}

fn capture_viewport(
    session: &mut UiInteractionSession,
    journal: &HostedJournal,
    route: UiRouteStamp,
) {
    let outcome = session
        .dispatch(
            journal,
            route,
            UiInputEvent::Pointer(sample_button(
                UiPointerButton::Middle,
                UiButtonPhase::Pressed,
                Some(UiPoint::new(20.0, 90.0)),
            )),
        )
        .unwrap();
    let events = target_events(outcome.consumed().unwrap(), &UiRoutedEventKind::Pressed);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].target(), &exact_key("base", "viewport"));
}

#[test]
fn pointer_samples_preserve_modifiers_and_resolve_only_actual_button_positions() {
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let frame = compile_for_slot(
        &mut compiler,
        &canvas_snapshot(
            1,
            viewport_input(UiCaptureContinuity::TargetGeometry),
            0.0,
            true,
        ),
        1,
        1,
        1,
        1,
    );
    let route = submit(&mut journal, frame, 10);
    let mut session = UiInteractionSession::default();
    let modifiers = UiModifiers::new(true, false, true, false);
    assert!(modifiers.shift());
    assert!(!modifiers.control());
    assert!(modifiers.alt());
    assert!(!modifiers.meta());
    assert_eq!(
        UiModifiers::default(),
        UiModifiers::new(false, false, false, false)
    );

    let moved =
        UiPointerInput::moved(SEAT, POINTER, UiPoint::new(20.25, 90.5)).with_modifiers(modifiers);
    let moved_outcome = session
        .dispatch(&journal, route, UiInputEvent::Pointer(moved))
        .unwrap();
    let motion = target_events(
        moved_outcome.consumed().unwrap(),
        &UiRoutedEventKind::PointerMoved,
    );
    assert_eq!(motion.len(), 1);
    assert_eq!(motion[0].pointer_input(), Some(&moved));

    for (phase, kind) in [
        (UiButtonPhase::Pressed, UiRoutedEventKind::Pressed),
        (
            UiButtonPhase::Released,
            UiRoutedEventKind::Released { inside: true },
        ),
    ] {
        let original =
            sample_button(UiPointerButton::Primary, phase, None).with_modifiers(modifiers);
        let outcome = session
            .dispatch(&journal, route, UiInputEvent::Pointer(original))
            .unwrap();
        let batch = outcome.consumed().unwrap();
        let event = target_events(batch, &kind)[0];
        let sample = event.pointer_input().unwrap();
        assert_eq!(sample.seat(), SEAT);
        assert_eq!(sample.pointer(), POINTER);
        assert_eq!(sample.position(), moved.position());
        assert_eq!(sample.modifiers(), modifiers);
        assert_eq!(sample.kind(), original.kind());
        assert!(
            batch
                .events()
                .iter()
                .filter(|event| event.kind() != &kind)
                .all(|event| event.pointer_input().is_none())
        );
    }
}

#[test]
fn captured_motion_outside_bounds_and_matching_release_work_for_each_supported_button() {
    for button in [
        UiPointerButton::Primary,
        UiPointerButton::Secondary,
        UiPointerButton::Middle,
    ] {
        let mut compiler = compiler();
        let mut journal = hosted_journal();
        let route = submit(
            &mut journal,
            compile_for_slot(
                &mut compiler,
                &canvas_snapshot(
                    1,
                    viewport_input(UiCaptureContinuity::TargetGeometry),
                    0.0,
                    true,
                ),
                1,
                1,
                1,
                1,
            ),
            10,
        );
        let mut session = UiInteractionSession::default();
        let pressed = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::Pointer(sample_button(
                    button,
                    UiButtonPhase::Pressed,
                    Some(UiPoint::new(20.0, 90.0)),
                )),
            )
            .unwrap();
        assert_eq!(
            target_events(pressed.consumed().unwrap(), &UiRoutedEventKind::Pressed).len(),
            1
        );
        let outside = UiPoint::new(280.5, 150.25);
        let moved = UiPointerInput::moved(SEAT, POINTER, outside)
            .with_modifiers(UiModifiers::new(false, true, false, true));
        let outcome = session
            .dispatch(&journal, route, UiInputEvent::Pointer(moved))
            .unwrap();
        let batch = outcome.consumed().unwrap();
        let motion = target_events(batch, &UiRoutedEventKind::PointerMoved);
        assert_eq!(motion.len(), 1);
        assert_eq!(motion[0].target(), &exact_key("base", "viewport"));
        assert_eq!(motion[0].pointer_input(), Some(&moved));
        assert!(
            target_events(batch, &UiRoutedEventKind::HoverChanged { hovered: false })
                .iter()
                .all(|event| event.pointer_input().is_none())
        );

        let wrong = if button == UiPointerButton::Primary {
            UiPointerButton::Secondary
        } else {
            UiPointerButton::Primary
        };
        let rejected = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::Pointer(sample_button(
                    wrong,
                    UiButtonPhase::Released,
                    Some(UiPoint::new(20.0, 90.0)),
                )),
            )
            .unwrap();
        assert_eq!(
            rejected.rejection(),
            Some(&UiNonActivation::PointerButtonMismatch {
                captured: button,
                received: wrong
            })
        );
        let released = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::Pointer(sample_button(button, UiButtonPhase::Released, None)),
            )
            .unwrap();
        let release = target_events(
            released.consumed().unwrap(),
            &UiRoutedEventKind::Released { inside: false },
        );
        assert_eq!(release.len(), 1);
        assert_eq!(
            release[0].pointer_input().unwrap().position(),
            Some(outside)
        );
        let again = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::Pointer(sample_button(button, UiButtonPhase::Released, None)),
            )
            .unwrap();
        assert_eq!(again.rejection(), Some(&UiNonActivation::NoPointerCapture));
    }
}

#[test]
fn declared_buttons_do_not_tunnel_through_an_unaccepting_top_hit() {
    let mut compiler = compiler();
    let authored = UiAuthoringSnapshotBuilder::new(1)
        .with_root(canvas_root(viewport(
            viewport_input(UiCaptureContinuity::TargetGeometry),
            0.0,
            true,
        )))
        .build()
        .unwrap();
    let mut journal = hosted_journal();
    let route = submit(
        &mut journal,
        compile_for_slot(&mut compiler, &authored, 1, 1, 1, 1),
        10,
    );
    let mut session = UiInteractionSession::default();
    for button in [
        UiPointerButton::Secondary,
        UiPointerButton::Middle,
        UiPointerButton::Other(4),
    ] {
        let rejected = session
            .dispatch(
                &journal,
                route,
                UiInputEvent::Pointer(sample_button(
                    button,
                    UiButtonPhase::Pressed,
                    Some(UiPoint::new(50.0, 30.0)),
                )),
            )
            .unwrap();
        assert_eq!(
            rejected.rejection(),
            Some(&UiNonActivation::UnsupportedPointerButton)
        );
        assert!(!session.has_pointer_facts(SURFACE));
    }
    let accepted = session
        .dispatch(
            &journal,
            route,
            pointer_button(UiButtonPhase::Pressed, UiPoint::new(50.0, 30.0)),
        )
        .unwrap();
    let events = target_events(accepted.consumed().unwrap(), &UiRoutedEventKind::Pressed);
    assert_eq!(events[0].target(), &exact_key("base", "moving"));
    let motion = session
        .dispatch(&journal, route, move_pointer(UiPoint::new(20.0, 90.0)))
        .unwrap();
    assert!(target_events(motion.consumed().unwrap(), &UiRoutedEventKind::PointerMoved).is_empty());
}

#[test]
fn nested_scroll_keeps_units_and_selects_one_nearest_listener_over_a_button() {
    let outer = UiNode::new(UiStyle::absolute(10.0, 10.0, 280.0, 150.0))
        .with_key(node_key("outer"))
        .with_interaction(
            UiInteraction::default()
                .with_enabled(true)
                .with_scroll(true),
        )
        .with_child(
            UiNode::new(UiStyle::absolute(20.0, 20.0, 180.0, 100.0))
                .with_key(node_key("inner"))
                .with_interaction(
                    UiInteraction::default()
                        .with_enabled(true)
                        .with_scroll(true),
                )
                .with_child(button("child", "Child", "child", 10.0, 10.0, true)),
        );
    let authored = UiAuthoringSnapshotBuilder::new(1)
        .with_root(UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_child(outer))
        .build()
        .unwrap();
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let frame = compile_for_slot(&mut compiler, &authored, 1, 1, 1, 1);
    assert_eq!(
        frame.hit_test(UiPoint::new(50.0, 50.0)).unwrap().key(),
        &exact_key("base", "child")
    );
    let route = submit(&mut journal, frame, 10);
    let mut session = UiInteractionSession::default();
    for delta in [
        UiScrollDelta::Lines(UiPoint::new(1.25, -2.5)),
        UiScrollDelta::Pixels(UiPoint::new(1.25, -2.5)),
    ] {
        let input = UiPointerInput::scroll(SEAT, POINTER, UiPoint::new(50.0, 50.0), delta)
            .with_modifiers(UiModifiers::new(true, true, false, true));
        let outcome = session
            .dispatch(&journal, route, UiInputEvent::Pointer(input))
            .unwrap();
        let batch = outcome.consumed().unwrap();
        assert_eq!(batch.events().len(), 5);
        assert_eq!(target_events(batch, &UiRoutedEventKind::Scrolled).len(), 1);
        assert!(
            batch
                .events()
                .iter()
                .all(|event| event.target() == &exact_key("base", "inner")
                    && event.pointer_input() == Some(&input))
        );
        assert_eq!(
            batch
                .events()
                .iter()
                .map(|event| (event.current_target().clone(), event.phase()))
                .collect::<Vec<_>>(),
            vec![
                (
                    UiKey::root(root_key("base")).unwrap(),
                    UiRoutePhase::Capture
                ),
                (exact_key("base", "outer"), UiRoutePhase::Capture),
                (exact_key("base", "inner"), UiRoutePhase::Target),
                (exact_key("base", "outer"), UiRoutePhase::Bubble),
                (UiKey::root(root_key("base")).unwrap(), UiRoutePhase::Bubble),
            ]
        );
    }
}

#[test]
fn uncaptured_motion_uses_nearest_interest_without_duplicating_parent_targets() {
    let authored = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            canvas_root(viewport(
                viewport_input(UiCaptureContinuity::TargetGeometry),
                0.0,
                true,
            ))
            .with_interaction(
                UiInteraction::default()
                    .with_enabled(true)
                    .with_pointer_motion(true),
            ),
        )
        .build()
        .unwrap();
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let route = submit(
        &mut journal,
        compile_for_slot(&mut compiler, &authored, 1, 1, 1, 1),
        10,
    );
    let mut session = UiInteractionSession::default();
    let input = UiPointerInput::moved(SEAT, POINTER, UiPoint::new(50.0, 30.0));
    let outcome = session
        .dispatch(&journal, route, UiInputEvent::Pointer(input))
        .unwrap();
    let batch = outcome.consumed().unwrap();
    let motion = target_events(batch, &UiRoutedEventKind::PointerMoved);
    assert_eq!(motion.len(), 1);
    assert_eq!(motion[0].target(), &exact_key("base", "viewport"));
    assert_eq!(motion[0].pointer_input(), Some(&input));
    assert!(
        batch
            .events()
            .iter()
            .filter(|event| event.kind() == &UiRoutedEventKind::PointerMoved)
            .all(|event| event.target() == &exact_key("base", "viewport"))
    );
}

#[test]
fn scroll_without_interest_and_disabled_listener_refuse_without_partial_pointer_state() {
    for disabled_listener in [false, true] {
        let mut compiler = compiler();
        let mut journal = hosted_journal();
        let declaration = UiInteraction::default().with_scroll(disabled_listener);
        let authored = canvas_snapshot(1, declaration, 0.0, true);
        let route = submit(
            &mut journal,
            compile_for_slot(&mut compiler, &authored, 1, 1, 1, 1),
            10,
        );
        let mut session = UiInteractionSession::default();
        let input = UiPointerInput::scroll(
            SEAT,
            POINTER,
            UiPoint::new(50.0, 30.0),
            UiScrollDelta::Lines(UiPoint::new(0.0, 1.0)),
        );
        let rejected = session
            .dispatch(&journal, route, UiInputEvent::Pointer(input))
            .unwrap();
        let expected = if disabled_listener {
            UiNonActivation::TargetDisabled {
                target: exact_key("base", "viewport"),
            }
        } else {
            UiNonActivation::NoScrollTarget
        };
        assert_eq!(rejected.rejection(), Some(&expected));
        assert!(!session.has_pointer_facts(SURFACE));
    }
}

#[test]
fn target_geometry_renews_capture_through_two_pans_and_default_geometry_cancels() {
    for continuity in [
        UiCaptureContinuity::TargetGeometry,
        UiCaptureContinuity::SurfaceGeometry,
    ] {
        let declaration = viewport_input(continuity);
        let mut compiler = compiler();
        let mut journal = hosted_journal();
        let route = submit(
            &mut journal,
            compile_for_slot(
                &mut compiler,
                &canvas_snapshot(1, declaration, 0.0, true),
                1,
                1,
                1,
                1,
            ),
            10,
        );
        let mut session = UiInteractionSession::default();
        capture_viewport(&mut session, &journal, route);
        session
            .dispatch(&journal, route, move_pointer(UiPoint::new(50.0, 30.0)))
            .unwrap();

        let mut latest_route = route;
        for (revision, pan, visible) in [(2, 75.0, true), (3, 150.0, false)] {
            latest_route = submit(
                &mut journal,
                compile_for_slot(
                    &mut compiler,
                    &canvas_snapshot(revision, declaration, pan, visible),
                    1,
                    1,
                    1,
                    1,
                ),
                9 + revision,
            );
            let cleanup = session
                .reconcile_latest_presentation(&journal, SURFACE, &[])
                .unwrap()
                .unwrap();
            assert_eq!(cleanup.presentation_epoch(), 9 + revision);
            assert!(
                cleanup
                    .events()
                    .iter()
                    .all(|event| event.pointer_input().is_none())
            );
            if continuity == UiCaptureContinuity::TargetGeometry {
                assert!(target_events(&cleanup, &UiRoutedEventKind::Cancelled).is_empty());
                let input = UiPointerInput::moved(
                    SEAT,
                    POINTER,
                    UiPoint::new(260.0 + revision as f32, 150.0),
                );
                let outcome = session
                    .dispatch(&journal, latest_route, UiInputEvent::Pointer(input))
                    .unwrap();
                let motion = target_events(
                    outcome.consumed().unwrap(),
                    &UiRoutedEventKind::PointerMoved,
                );
                assert_eq!(motion.len(), 1);
                assert_eq!(motion[0].target(), &exact_key("base", "viewport"));
                assert_eq!(motion[0].pointer_input(), Some(&input));
            } else if revision == 2 {
                assert_eq!(
                    target_events(&cleanup, &UiRoutedEventKind::Cancelled).len(),
                    1
                );
                assert!(!session.has_pointer_facts(SURFACE));
            }
        }
        let outcome = session
            .dispatch(
                &journal,
                latest_route,
                UiInputEvent::Pointer(sample_button(
                    UiPointerButton::Middle,
                    UiButtonPhase::Released,
                    Some(UiPoint::new(270.0, 150.0)),
                )),
            )
            .unwrap();
        if continuity == UiCaptureContinuity::TargetGeometry {
            assert_eq!(
                target_events(
                    outcome.consumed().unwrap(),
                    &UiRoutedEventKind::Released { inside: false }
                )
                .len(),
                1
            );
        } else {
            assert_eq!(
                outcome.rejection(),
                Some(&UiNonActivation::NoPointerCapture)
            );
        }
    }
}

#[test]
fn staged_pan_cannot_renew_epoch_or_mutate_the_live_presented_capture() {
    let declaration = viewport_input(UiCaptureContinuity::TargetGeometry);
    let mut compiler = compiler();
    let initial = compile_for_slot(
        &mut compiler,
        &canvas_snapshot(1, declaration, 0.0, true),
        1,
        1,
        1,
        1,
    );
    let mut journal = hosted_journal();
    let route = submit(&mut journal, Arc::clone(&initial), 10);
    let mut session = UiInteractionSession::default();
    capture_viewport(&mut session, &journal, route);
    session
        .dispatch(&journal, route, move_pointer(UiPoint::new(50.0, 30.0)))
        .unwrap();
    let successor = compile_for_slot(
        &mut compiler,
        &canvas_snapshot(2, declaration, 90.0, false),
        1,
        1,
        1,
        1,
    );
    let staged = session
        .stage_candidate_presentation(SURFACE, SEAT, Some(&initial), &successor, &[])
        .unwrap()
        .unwrap();
    assert_eq!(staged.dispatch().presentation_epoch(), 10);
    assert!(target_events(staged.dispatch(), &UiRoutedEventKind::Cancelled).is_empty());
    let (mut staged_session, _) = staged.into_parts();
    for current in [&mut session, &mut staged_session] {
        let outcome = current
            .dispatch(
                &journal,
                route,
                UiInputEvent::Pointer(sample_button(
                    UiPointerButton::Middle,
                    UiButtonPhase::Released,
                    Some(UiPoint::new(20.0, 90.0)),
                )),
            )
            .unwrap();
        assert_eq!(
            target_events(
                outcome.consumed().unwrap(),
                &UiRoutedEventKind::Released { inside: true }
            )
            .len(),
            1
        );
    }
    assert_eq!(
        journal
            .latest(SURFACE)
            .unwrap()
            .receipt()
            .presentation_epoch(),
        10
    );
}

#[test]
fn late_historical_pointer_samples_cannot_cancel_or_redirect_a_renewed_capture() {
    let declaration = viewport_input(UiCaptureContinuity::TargetGeometry);
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let original = submit(
        &mut journal,
        compile_for_slot(
            &mut compiler,
            &canvas_snapshot(1, declaration, 0.0, true),
            1,
            1,
            1,
            1,
        ),
        10,
    );
    let mut session = UiInteractionSession::default();
    capture_viewport(&mut session, &journal, original);
    let mut latest = original;
    for revision in 2..=3 {
        latest = submit(
            &mut journal,
            compile_for_slot(
                &mut compiler,
                &canvas_snapshot(revision, declaration, 40.0 * revision as f32, true),
                1,
                1,
                1,
                1,
            ),
            9 + revision,
        );
        session
            .reconcile_latest_presentation(&journal, SURFACE, &[])
            .unwrap();
    }
    let outside = UiPoint::new(285.0, 160.0);
    session
        .dispatch(&journal, latest, move_pointer(outside))
        .unwrap();
    let position = UiPoint::new(20.0, 90.0);
    for input in [
        UiPointerInput::moved(SEAT, POINTER, position),
        sample_button(
            UiPointerButton::Middle,
            UiButtonPhase::Pressed,
            Some(position),
        ),
        sample_button(
            UiPointerButton::Middle,
            UiButtonPhase::Released,
            Some(position),
        ),
        UiPointerInput::scroll(
            SEAT,
            POINTER,
            position,
            UiScrollDelta::Pixels(UiPoint::new(1.0, 2.0)),
        ),
        UiPointerInput::cancelled(SEAT, POINTER),
    ] {
        let rejected = session
            .dispatch(&journal, original, UiInputEvent::Pointer(input))
            .unwrap();
        assert_eq!(
            rejected,
            UiDispatchOutcome::Rejected(UiNonActivation::StaleGeometry)
        );
    }
    let released = session
        .dispatch(
            &journal,
            latest,
            UiInputEvent::Pointer(sample_button(
                UiPointerButton::Middle,
                UiButtonPhase::Released,
                None,
            )),
        )
        .unwrap();
    let events = target_events(
        released.consumed().unwrap(),
        &UiRoutedEventKind::Released { inside: false },
    );
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].pointer_input().unwrap().position(), Some(outside));
}

#[test]
fn target_geometry_opt_in_cannot_renew_capture_by_dispatching_an_unreconciled_epoch() {
    let declaration = viewport_input(UiCaptureContinuity::TargetGeometry);
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let original = submit(
        &mut journal,
        compile_for_slot(
            &mut compiler,
            &canvas_snapshot(1, declaration, 0.0, true),
            1,
            1,
            1,
            1,
        ),
        10,
    );
    let mut session = UiInteractionSession::default();
    capture_viewport(&mut session, &journal, original);
    let latest = submit(
        &mut journal,
        compile_for_slot(
            &mut compiler,
            &canvas_snapshot(2, declaration, 90.0, false),
            1,
            1,
            1,
            1,
        ),
        11,
    );
    let cleanup = session
        .dispatch(&journal, latest, move_pointer(UiPoint::new(280.0, 160.0)))
        .unwrap();
    let batch = cleanup.consumed().unwrap();
    assert!(batch.is_reconciliation());
    assert_eq!(target_events(batch, &UiRoutedEventKind::Cancelled).len(), 1);
    assert!(
        batch
            .events()
            .iter()
            .all(|event| event.pointer_input().is_none())
    );
    let rejected = session
        .dispatch(
            &journal,
            latest,
            UiInputEvent::Pointer(sample_button(
                UiPointerButton::Middle,
                UiButtonPhase::Released,
                Some(UiPoint::new(20.0, 90.0)),
            )),
        )
        .unwrap();
    assert_eq!(
        rejected.rejection(),
        Some(&UiNonActivation::NoPointerCapture)
    );
}

#[test]
fn successor_slot_epoch_restart_cleans_predecessor_capture_instead_of_rejecting_as_late() {
    let declaration = viewport_input(UiCaptureContinuity::TargetGeometry);
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let authored = canvas_snapshot(1, declaration, 0.0, true);
    let old = submit(
        &mut journal,
        compile_for_slot(&mut compiler, &authored, 1, 1, 1, 1),
        40,
    );
    let mut session = UiInteractionSession::default();
    capture_viewport(&mut session, &journal, old);
    journal.advance_surface_slot(SURFACE, 1, 2).unwrap();
    let successor = submit(
        &mut journal,
        compile_for_slot(&mut compiler, &authored, 2, 1, 1, 1),
        1,
    );
    let cleanup = session
        .dispatch(&journal, successor, move_pointer(UiPoint::new(20.0, 90.0)))
        .unwrap();
    let batch = cleanup.consumed().unwrap();
    assert!(batch.is_reconciliation());
    assert_eq!(target_events(batch, &UiRoutedEventKind::Cancelled).len(), 1);
    let pressed = session
        .dispatch(
            &journal,
            successor,
            UiInputEvent::Pointer(sample_button(
                UiPointerButton::Middle,
                UiButtonPhase::Pressed,
                Some(UiPoint::new(20.0, 90.0)),
            )),
        )
        .unwrap();
    assert_eq!(
        target_events(pressed.consumed().unwrap(), &UiRoutedEventKind::Pressed).len(),
        1
    );
}

#[test]
fn nonfinite_samples_leave_pending_capture_cleanup_and_session_unchanged() {
    let declaration = viewport_input(UiCaptureContinuity::TargetGeometry);
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let original = submit(
        &mut journal,
        compile_for_slot(
            &mut compiler,
            &canvas_snapshot(1, declaration, 0.0, true),
            1,
            1,
            1,
            1,
        ),
        10,
    );
    let mut session = UiInteractionSession::default();
    capture_viewport(&mut session, &journal, original);
    let latest = submit(
        &mut journal,
        compile_for_slot(
            &mut compiler,
            &canvas_snapshot(2, declaration.with_enabled(false), 0.0, true),
            1,
            1,
            1,
            1,
        ),
        11,
    );
    for (input, error) in [
        (
            UiPointerInput::moved(SEAT, POINTER, UiPoint::new(f32::NAN, 0.0)),
            UiDispatchError::NonFinitePointerPosition,
        ),
        (
            sample_button(
                UiPointerButton::Other(7),
                UiButtonPhase::Released,
                Some(UiPoint::new(1.0, f32::INFINITY)),
            ),
            UiDispatchError::NonFinitePointerPosition,
        ),
        (
            UiPointerInput::scroll(
                SEAT,
                POINTER,
                UiPoint::new(f32::NEG_INFINITY, 0.0),
                UiScrollDelta::Lines(UiPoint::new(0.0, 1.0)),
            ),
            UiDispatchError::NonFinitePointerPosition,
        ),
        (
            UiPointerInput::scroll(
                SEAT,
                POINTER,
                UiPoint::new(20.0, 90.0),
                UiScrollDelta::Lines(UiPoint::new(f32::NAN, 1.0)),
            ),
            UiDispatchError::NonFiniteScrollDelta,
        ),
        (
            UiPointerInput::scroll(
                SEAT,
                POINTER,
                UiPoint::new(20.0, 90.0),
                UiScrollDelta::Pixels(UiPoint::new(0.0, f32::INFINITY)),
            ),
            UiDispatchError::NonFiniteScrollDelta,
        ),
    ] {
        let mut candidate = session.clone();
        let mut control = session.clone();
        assert_eq!(
            candidate
                .dispatch(&journal, latest, UiInputEvent::Pointer(input))
                .unwrap_err(),
            error
        );
        assert!(candidate.has_pointer_facts(SURFACE));
        let next = move_pointer(UiPoint::new(280.0, 160.0));
        let expected = control.dispatch(&journal, latest, next.clone()).unwrap();
        let actual = candidate.dispatch(&journal, latest, next).unwrap();
        assert_eq!(actual, expected);
        let cleanup = actual.consumed().unwrap();
        assert!(cleanup.is_reconciliation());
        assert_eq!(
            target_events(cleanup, &UiRoutedEventKind::Cancelled).len(),
            1
        );
        assert!(
            cleanup
                .events()
                .iter()
                .all(|event| event.pointer_input().is_none())
        );
    }
}

#[test]
fn nonfinite_validation_also_precedes_stale_geometry_and_resolved_route_checks() {
    let declaration = viewport_input(UiCaptureContinuity::TargetGeometry);
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let route = submit(
        &mut journal,
        compile_for_slot(
            &mut compiler,
            &canvas_snapshot(1, declaration, 0.0, true),
            1,
            1,
            1,
            1,
        ),
        10,
    );
    let mut session = UiInteractionSession::default();
    capture_viewport(&mut session, &journal, route);
    session
        .dispatch(&journal, route, UiInputEvent::GeometryInvalidated)
        .unwrap();
    let invalid = UiInputEvent::Pointer(UiPointerInput::scroll(
        SEAT,
        POINTER,
        UiPoint::new(20.0, 90.0),
        UiScrollDelta::Pixels(UiPoint::new(f32::NAN, 0.0)),
    ));
    assert_eq!(
        session
            .dispatch(&journal, route, invalid.clone())
            .unwrap_err(),
        UiDispatchError::NonFiniteScrollDelta
    );
    let resolved = journal.resolve(route).unwrap();
    let malformed_route = UiRouteStamp::new(
        SURFACE,
        route.slot_generation(),
        route.route_revision(),
        route.geometry_generation(),
        999,
    );
    assert_eq!(
        session
            .dispatch_resolved(&resolved, &resolved, malformed_route, invalid.clone())
            .unwrap_err(),
        UiDispatchError::NonFiniteScrollDelta
    );
    assert_eq!(
        session
            .dispatch(&journal, malformed_route, invalid)
            .unwrap_err(),
        UiDispatchError::NonFiniteScrollDelta
    );
    let rejected = session
        .dispatch(&journal, route, move_pointer(UiPoint::new(20.0, 90.0)))
        .unwrap();
    assert_eq!(rejected.rejection(), Some(&UiNonActivation::StaleGeometry));
    assert!(!session.has_pointer_facts(SURFACE));
}

#[test]
fn target_geometry_still_cancels_for_owner_ancestor_policy_modal_and_source_changes() {
    for change in [
        "root-size",
        "ancestor-size",
        "ancestor-clip",
        "ancestor-policy",
        "owner-size",
        "owner-policy",
        "owner-buttons",
        "owner-continuity",
        "owner-disabled",
        "owner-hidden",
        "owner-removed",
        "reparent",
        "modal",
        "fallback",
    ] {
        let declaration = viewport_input(UiCaptureContinuity::TargetGeometry);
        let mut compiler = compiler();
        let mut journal = hosted_journal();
        let initial = compile_for_slot(
            &mut compiler,
            &canvas_snapshot(1, declaration, 0.0, true),
            1,
            1,
            1,
            1,
        );
        let original = submit(&mut journal, initial, 10);
        let mut session = UiInteractionSession::default();
        capture_viewport(&mut session, &journal, original);

        let input = match change {
            "owner-policy" => declaration.with_scroll(false),
            "owner-buttons" => declaration.with_pointer_buttons(UiPointerButtons::PRIMARY),
            "owner-continuity" => {
                declaration.with_capture_continuity(UiCaptureContinuity::SurfaceGeometry)
            }
            "owner-disabled" => declaration.with_enabled(false),
            _ => declaration,
        };
        let owner = if change == "owner-size" || change == "owner-hidden" {
            UiNode::new(
                UiStyle::absolute(
                    10.0,
                    10.0,
                    if change == "owner-size" { 159.0 } else { 160.0 },
                    100.0,
                )
                .with_overflow(UiOverflow::Clip)
                .with_visible(change != "owner-hidden"),
            )
            .with_key(node_key("viewport"))
            .with_interaction(input)
            .with_semantics(UiSemantics::new(UiSemanticRole::Group, "Canvas").unwrap())
            .with_child(
                UiNode::new(UiStyle::absolute(30.0, 10.0, 80.0, 40.0))
                    .with_key(node_key("moving"))
                    .with_interaction(UiInteraction::focus_target()),
            )
        } else {
            viewport(input, 0.0, true)
        };
        let ancestor_style = UiStyle::absolute(
            0.0,
            0.0,
            if change == "ancestor-size" {
                319.0
            } else {
                320.0
            },
            180.0,
        )
        .with_overflow(if change == "ancestor-clip" {
            UiOverflow::Clip
        } else {
            UiOverflow::Visible
        });
        let mut ancestor = UiNode::new(ancestor_style)
            .with_key(node_key("ancestor"))
            .with_interaction(UiInteraction::default().with_enabled(change == "ancestor-policy"));
        if change != "owner-removed" {
            ancestor = ancestor.with_child(owner);
        }
        if change == "reparent" {
            ancestor = UiNode::new(UiStyle::absolute(0.0, 0.0, 320.0, 180.0))
                .with_key(node_key("new-parent"))
                .with_child(ancestor);
        }
        let root_style = match change {
            "root-size" => UiStyle::absolute(0.0, 0.0, 319.0, 180.0),
            "fallback" => UiStyle::absolute(f32::NAN, 0.0, 320.0, 180.0),
            _ => UiStyle::fill(),
        };
        let builder = UiAuthoringSnapshotBuilder::new(2).with_root(
            UiRoot::new(root_key("base"), UiLayer::Base, root_style).with_child(ancestor),
        );
        let authored = if change == "modal" {
            builder.with_root(modal_root())
        } else {
            builder
        }
        .build()
        .unwrap();
        let successor = compile_for_slot(&mut compiler, &authored, 1, 1, 1, 1);
        if change == "fallback" {
            assert_eq!(
                successor.root_outcomes()[0].status(),
                UiRootStatus::RetainedVisual
            );
        }
        let latest = submit(&mut journal, successor, 11);
        let cleanup = session
            .reconcile_latest_presentation(&journal, SURFACE, &[])
            .unwrap()
            .unwrap();
        let cancelled = target_events(&cleanup, &UiRoutedEventKind::Cancelled);
        assert_eq!(cancelled.len(), 1, "{change}: {cleanup:?}");
        assert_eq!(cancelled[0].target(), &exact_key("base", "viewport"));
        assert!(
            cleanup
                .events()
                .iter()
                .all(|event| event.pointer_input().is_none())
        );
        let released = session
            .dispatch(
                &journal,
                latest,
                UiInputEvent::Pointer(sample_button(
                    UiPointerButton::Middle,
                    UiButtonPhase::Released,
                    Some(UiPoint::new(20.0, 90.0)),
                )),
            )
            .unwrap();
        assert_eq!(
            released.rejection(),
            Some(&UiNonActivation::NoPointerCapture),
            "{change}"
        );
    }
}

#[test]
fn explicit_owner_or_ancestor_invalidation_cancels_even_identical_target_geometry() {
    for invalidated in ["viewport", "ancestor"] {
        let declaration = viewport_input(UiCaptureContinuity::TargetGeometry);
        let mut compiler = compiler();
        let mut journal = hosted_journal();
        let original = submit(
            &mut journal,
            compile_for_slot(
                &mut compiler,
                &canvas_snapshot(1, declaration, 0.0, true),
                1,
                1,
                1,
                1,
            ),
            10,
        );
        let mut session = UiInteractionSession::default();
        capture_viewport(&mut session, &journal, original);
        let latest = submit(
            &mut journal,
            compile_for_slot(
                &mut compiler,
                &canvas_snapshot(2, declaration, 0.0, true),
                1,
                1,
                1,
                1,
            ),
            11,
        );
        let cleanup = session
            .reconcile_latest_presentation(&journal, SURFACE, &[exact_key("base", invalidated)])
            .unwrap()
            .unwrap();
        assert_eq!(
            target_events(&cleanup, &UiRoutedEventKind::Cancelled).len(),
            1
        );
        let released = session
            .dispatch(
                &journal,
                latest,
                UiInputEvent::Pointer(sample_button(
                    UiPointerButton::Middle,
                    UiButtonPhase::Released,
                    Some(UiPoint::new(20.0, 90.0)),
                )),
            )
            .unwrap();
        assert_eq!(
            released.rejection(),
            Some(&UiNonActivation::NoPointerCapture)
        );
    }
}

#[test]
fn independent_hover_invalidation_preserves_only_opted_in_capture() {
    for continuity in [
        UiCaptureContinuity::SurfaceGeometry,
        UiCaptureContinuity::TargetGeometry,
    ] {
        let declaration = viewport_input(continuity);
        let mut compiler = compiler();
        let mut journal = hosted_journal();
        let original = submit(
            &mut journal,
            compile_for_slot(
                &mut compiler,
                &canvas_snapshot(1, declaration, 0.0, true),
                1,
                1,
                1,
                1,
            ),
            10,
        );
        let mut session = UiInteractionSession::default();
        capture_viewport(&mut session, &journal, original);
        session
            .dispatch(&journal, original, move_pointer(UiPoint::new(50.0, 30.0)))
            .unwrap();
        let latest = submit(
            &mut journal,
            compile_for_slot(
                &mut compiler,
                &canvas_snapshot(2, declaration, 0.0, true),
                1,
                1,
                1,
                1,
            ),
            11,
        );
        let cleanup = session
            .reconcile_latest_presentation(&journal, SURFACE, &[exact_key("base", "moving")])
            .unwrap()
            .unwrap();
        assert_eq!(
            target_events(
                &cleanup,
                &UiRoutedEventKind::HoverChanged { hovered: false }
            )
            .len(),
            1
        );
        let cancelled = target_events(&cleanup, &UiRoutedEventKind::Cancelled);
        let released = session
            .dispatch(
                &journal,
                latest,
                UiInputEvent::Pointer(sample_button(
                    UiPointerButton::Middle,
                    UiButtonPhase::Released,
                    None,
                )),
            )
            .unwrap();
        if continuity == UiCaptureContinuity::TargetGeometry {
            assert!(cancelled.is_empty());
            assert_eq!(
                target_events(
                    released.consumed().unwrap(),
                    &UiRoutedEventKind::Released { inside: false }
                )
                .len(),
                1
            );
        } else {
            assert_eq!(cancelled.len(), 1);
            assert_eq!(
                released.rejection(),
                Some(&UiNonActivation::MissingPointerPosition)
            );
        }
    }
}

#[test]
fn target_geometry_cancels_for_surface_environment_changes_with_unchanged_owner_rects() {
    for change in ["route", "geometry", "content", "theme", "font"] {
        let declaration = viewport_input(UiCaptureContinuity::TargetGeometry);
        let mut compiler = compiler();
        let mut journal = hosted_journal();
        let authored = canvas_snapshot(1, declaration, 0.0, true);
        let original = submit(
            &mut journal,
            compile_for_slot(&mut compiler, &authored, 1, 1, 1, 1),
            10,
        );
        let mut session = UiInteractionSession::default();
        capture_viewport(&mut session, &journal, original);
        let transform = UiSurfaceContentTransform::direct(
            UiSize::new(320.0, 180.0).unwrap(),
            if change == "content" { 1.5 } else { 1.0 },
        )
        .unwrap();
        let environment = UiEnvironment::for_surface(
            SURFACE,
            transform,
            UiSurfaceGenerations::new(
                1,
                if change == "route" { 2 } else { 1 },
                if change == "geometry" { 2 } else { 1 },
                1,
            )
            .unwrap(),
            if change == "theme" { 2 } else { 1 },
            if change == "font" { 2 } else { 1 },
        )
        .unwrap();
        let theme = UiTheme::new(
            environment.theme_revision().get(),
            UiTheme::default().palette(),
        )
        .unwrap();
        let successor = compiler.compile(&authored, environment, theme).unwrap();
        submit(&mut journal, successor, 11);
        let cleanup = session
            .reconcile_latest_presentation(&journal, SURFACE, &[])
            .unwrap()
            .unwrap();
        assert_eq!(
            target_events(&cleanup, &UiRoutedEventKind::Cancelled).len(),
            1,
            "{change}"
        );
    }
}

#[test]
fn surface_close_clears_capture_hover_and_focus_without_fabricating_pointer_samples() {
    let declaration = viewport_input(UiCaptureContinuity::TargetGeometry);
    let mut compiler = compiler();
    let mut journal = hosted_journal();
    let route = submit(
        &mut journal,
        compile_for_slot(
            &mut compiler,
            &canvas_snapshot(1, declaration, 0.0, true),
            1,
            1,
            1,
            1,
        ),
        10,
    );
    let mut session = UiInteractionSession::default();
    capture_viewport(&mut session, &journal, route);
    let cleanup = session
        .dispatch(&journal, route, UiInputEvent::SurfaceCancelled)
        .unwrap();
    let batch = cleanup.consumed().unwrap();
    assert_eq!(target_events(batch, &UiRoutedEventKind::Cancelled).len(), 1);
    assert_eq!(
        target_events(batch, &UiRoutedEventKind::HoverChanged { hovered: false }).len(),
        1
    );
    assert_eq!(
        target_events(batch, &UiRoutedEventKind::FocusChanged { focused: false }).len(),
        1
    );
    assert!(
        batch
            .events()
            .iter()
            .all(|event| event.pointer_input().is_none()
                && !matches!(
                    event.kind(),
                    UiRoutedEventKind::Released { .. }
                        | UiRoutedEventKind::ActivateRequested { .. }
                ))
    );
    assert!(!session.has_pointer_facts(SURFACE));
    journal.retire_surface(SURFACE, 2).unwrap();
    assert!(matches!(
        session.dispatch(&journal, route, move_pointer(UiPoint::new(20.0, 90.0))),
        Err(UiDispatchError::Journal(_))
    ));
}

#[test]
fn motion_and_scroll_interest_require_keys_and_are_retained_in_layout_facts() {
    for interest in [
        UiInteraction::default()
            .with_enabled(true)
            .with_pointer_motion(true),
        UiInteraction::default()
            .with_enabled(true)
            .with_scroll(true),
    ] {
        let mut compiler = compiler();
        let authored = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_child(
                    UiNode::new(UiStyle::absolute(10.0, 10.0, 40.0, 40.0))
                        .with_interaction(interest),
                ),
            )
            .build()
            .unwrap();
        let rejected = compile_for_slot(&mut compiler, &authored, 1, 1, 1, 1);
        assert_eq!(rejected.root_outcomes()[0].status(), UiRootStatus::Absent);
        let authored = UiAuthoringSnapshotBuilder::new(2)
            .with_root(
                UiRoot::new(root_key("base"), UiLayer::Base, UiStyle::fill()).with_child(
                    UiNode::new(UiStyle::absolute(10.0, 10.0, 40.0, 40.0))
                        .with_key(node_key("listener"))
                        .with_interaction(interest),
                ),
            )
            .build()
            .unwrap();
        let frame = compile_for_slot(&mut compiler, &authored, 1, 1, 1, 1);
        assert_eq!(
            frame
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(&exact_key("base", "listener")))
                .unwrap()
                .interaction(),
            interest
        );
        assert!(frame.hit_regions().is_empty());
    }
    assert_eq!(
        UiInteraction::hit_target().pointer_buttons(),
        UiPointerButtons::PRIMARY
    );
    assert_eq!(
        UiInteraction::focus_target().capture_continuity(),
        UiCaptureContinuity::SurfaceGeometry
    );
    assert!(!UiPointerButtons::new(true, true, true).contains(UiPointerButton::Other(0)));
    let cancel = UiPointerInput::cancelled(SEAT, POINTER);
    assert_eq!(cancel.kind(), UiPointerInputKind::Cancelled);
    assert_eq!(cancel.position(), None);
}
