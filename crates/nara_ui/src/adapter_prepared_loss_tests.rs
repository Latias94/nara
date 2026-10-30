use super::*;

use crate::__private::{
    UiDeferredPresentationRetry, UiPreparedPresentationAuthority, UiPresentationInvalidationReason,
};
use nara_input::InputInvalidationReason;

fn prepare_pressed_presentation() -> (
    UiTestApp,
    UiPresentationReceipt,
    UiPreparedPresentationAuthority,
    UiActionCursor,
) {
    let (app, _button) = build_button_app();
    prepare_pressed_presentation_in(app)
}

fn prepare_pressed_presentation_in(
    mut app: UiTestApp,
) -> (
    UiTestApp,
    UiPresentationReceipt,
    UiPreparedPresentationAuthority,
    UiActionCursor,
) {
    let cursor = app
        .register_consumer(UiActionConsumerId::new("prepared-loss-test").unwrap())
        .unwrap();
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    {
        let mut timeline = app.timeline();
        timeline
            .admit_window_event(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(true),
            )
            .unwrap();
        timeline
            .admit_pointer(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(10.0, 10.0)),
            )
            .unwrap();
        timeline
            .admit_mouse_button(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
    }
    app.update().unwrap();
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let button = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap();
    assert!(button.state().focused());
    assert!(
        button.state().pressed(),
        "the live session must own pointer capture",
    );
    let runtime = app.world().resource::<crate::runtime::UiRuntimeState>();
    let surface = runtime.surface(surface_id()).unwrap();
    let reducer = surface.reducer_states().get(button.key()).unwrap();
    assert!(reducer.hovered() && reducer.pressed() && reducer.focused());

    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let prepared = app.prepare_presentation(frame, 2).unwrap();
    (app, receipt, prepared, cursor)
}

fn cancel_focus(app: &mut UiTestApp, receipt: UiPresentationReceipt) {
    app.timeline()
        .admit_window_event(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputWindowEvent::Focused(false),
        )
        .expect("platform loss must revoke even while a prepared route pins the endpoint");
    app.update()
        .expect("the actual UI schedule must consume physical cancellation before late commit");
    assert_cancelled_presentation(app, receipt);
}

fn assert_cancelled_presentation(app: &UiTestApp, receipt: UiPresentationReceipt) {
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let button = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap();
    assert!(!button.state().focused());
    assert!(!button.state().pressed());
    let runtime = app.world().resource::<crate::runtime::UiRuntimeState>();
    let surface = runtime.surface(surface_id()).unwrap();
    assert!(!surface.interaction().has_pointer_facts(surface_id()));
    assert_eq!(
        surface.reducer_states().get(button.key()).copied(),
        Some(nara_ui_widgets::UiButtonState::default()),
    );
}

#[test]
fn focus_loss_late_commit_cannot_resurrect_prepared_focus_capture_or_reducers() {
    let (mut app, receipt, prepared, cursor) = prepare_pressed_presentation();
    let invalidated_receipt = prepared.receipt();
    cancel_focus(&mut app, receipt);
    let live_frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let live_reducers = app
        .world()
        .resource::<crate::runtime::UiRuntimeState>()
        .surface(surface_id())
        .unwrap()
        .reducer_states()
        .clone();

    assert_eq!(
        app.commit_presentation(prepared),
        UiPresentationCommit::Invalidated {
            reason: UiPresentationInvalidationReason::Input(InputInvalidationReason::FocusLost),
        },
    );

    assert_eq!(test_receipt(app.world(), 1), receipt);
    assert!(crate::runtime::presented_receipt(app.world(), surface_id(), 2).is_none());
    assert!(ui_semantic_snapshot(app.world(), invalidated_receipt, UiSeatId::PRIMARY).is_err());
    assert!(Arc::ptr_eq(
        &live_frame,
        &ui_candidate_frame(app.world(), surface_id())
            .unwrap()
            .unwrap(),
    ));
    assert_eq!(
        app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .surface(surface_id())
            .unwrap()
            .reducer_states(),
        &live_reducers,
    );
    assert_cancelled_presentation(&app, receipt);

    app.timeline()
        .admit_pointer(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(10.0, 10.0)),
        )
        .unwrap();
    assert!(
        !app.world()
            .resource::<InputTimeline>()
            .entries()
            .last()
            .unwrap()
            .route()
            .is_ui_routable(),
        "late commit must not rearm the revoked input route",
    );
    assert!(
        app.timeline()
            .admit_mouse_button(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                ButtonDriverInput::Release(MouseButton::Left),
            )
            .unwrap()
            .is_ignored(),
    );
    app.update().unwrap();
    assert_cancelled_presentation(&app, receipt);
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty(),
    );
    assert!(
        app.world()
            .resource::<UiRouteRejectionLog>()
            .as_slice()
            .is_empty(),
    );
}

#[test]
fn invalidated_late_commit_preserves_a_successor_on_a_replacement_endpoint() {
    let (mut app, receipt, prepared, cursor) = prepare_pressed_presentation();
    let invalidated_receipt = prepared.receipt();
    cancel_focus(&mut app, receipt);
    app.update_surface(
        surface_id(),
        test_endpoint(12),
        test_transform(200.0, 100.0, 200.0, 100.0, 1.0),
    )
    .expect("a revoked predecessor must not prevent a replacement endpoint");
    app.update().unwrap();
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let successor = app.present_frame(frame, 3).unwrap();
    let key = expected_runtime_node("root", "retry");
    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::focus_target(
            successor,
            UiSeatId::PRIMARY,
            key,
        )),
        UiRouteDisposition::Consumed(_),
    ));
    let successor_semantics =
        ui_semantic_snapshot(app.world(), successor, UiSeatId::PRIMARY).unwrap();

    assert_eq!(
        app.commit_presentation(prepared),
        UiPresentationCommit::Invalidated {
            reason: UiPresentationInvalidationReason::Input(InputInvalidationReason::FocusLost),
        },
    );

    assert_eq!(test_receipt(app.world(), 3), successor);
    assert!(crate::runtime::presented_receipt(app.world(), surface_id(), 2).is_none());
    assert!(ui_semantic_snapshot(app.world(), invalidated_receipt, UiSeatId::PRIMARY).is_err());
    assert_eq!(
        ui_semantic_snapshot(app.world(), successor, UiSeatId::PRIMARY).unwrap(),
        successor_semantics,
    );
    assert_eq!(
        active_ui_surface_slots(app.world()).unwrap()[0].endpoint(),
        test_endpoint(12),
    );
    app.timeline()
        .admit_pointer(
            InputWindowId::new(12),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(10.0, 10.0)),
        )
        .unwrap();
    let route = app
        .world()
        .resource::<InputTimeline>()
        .entries()
        .last()
        .unwrap()
        .route();
    assert!(route.is_ui_routable());
    assert_eq!(
        route.presentation_epoch().unwrap().get(),
        successor.presentation_epoch(),
    );
    assert_eq!(
        route.slot_generation().unwrap().get(),
        successor.slot_generation().get(),
    );
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty(),
    );
}

#[test]
fn aborting_an_invalidated_presentation_allows_a_fresh_presentation_and_press() {
    let (mut app, receipt, prepared, cursor) = prepare_pressed_presentation();
    cancel_focus(&mut app, receipt);
    app.abort_presentation(prepared);
    assert_cancelled_presentation(&app, receipt);
    assert!(crate::runtime::presented_receipt(app.world(), surface_id(), 2).is_none());

    app.timeline()
        .admit_window_event(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputWindowEvent::Focused(true),
        )
        .unwrap();
    app.update().unwrap();
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let retry = app.prepare_presentation(frame, 2).unwrap();
    let UiPresentationCommit::Committed(retried_receipt) = app.commit_presentation(retry) else {
        panic!("aborting an invalidated token must release its exact reservation for retry");
    };
    assert_cancelled_presentation(&app, retried_receipt);
    {
        let mut timeline = app.timeline();
        timeline
            .admit_pointer(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(Vec2::new(10.0, 10.0)),
            )
            .unwrap();
        for input in [
            ButtonDriverInput::Press(MouseButton::Left),
            ButtonDriverInput::Release(MouseButton::Left),
        ] {
            timeline
                .admit_mouse_button(
                    InputWindowId::new(11),
                    InputSeatId::PRIMARY,
                    InputPointerId::PRIMARY,
                    input,
                )
                .unwrap();
        }
    }
    app.update().unwrap();
    let intents = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap();
    assert_eq!(
        intents.len(),
        1,
        "only a fresh press may activate after loss"
    );
    assert_eq!(intents[0].action().as_str(), "retry-draft");
}

#[test]
fn invalidated_deferred_presentation_cannot_retry_cancelled_pressed_visuals() {
    let (mut app, receipt, prepared, cursor) = prepare_pressed_presentation();
    let pressed_frame = Arc::clone(prepared.frame());
    cancel_focus(&mut app, receipt);
    let cancelled_frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    assert_ne!(pressed_frame.paint(), cancelled_frame.paint());
    let deferred = app.defer_presentation(prepared);

    assert!(matches!(
        app.retry_presentation(deferred).unwrap(),
        UiDeferredPresentationRetry::Superseded,
    ));
    assert_cancelled_presentation(&app, receipt);
    let retry = app.prepare_presentation(cancelled_frame, 2).unwrap();
    let UiPresentationCommit::Committed(receipt) = app.commit_presentation(retry) else {
        panic!("superseding invalidated deferred work must leave a fresh candidate publishable");
    };
    assert_cancelled_presentation(&app, receipt);
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty(),
    );
}

fn observe_input_route(app: &mut UiTestApp) -> nara_input::InputRouteStamp {
    app.timeline()
        .admit_wheel(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            Vec2::new(0.0, 1.0),
        )
        .unwrap();
    app.world()
        .resource::<InputTimeline>()
        .entries()
        .last()
        .unwrap()
        .route()
}

fn assert_interaction_change_rejects_prepared(
    app: &mut UiTestApp,
    receipt: UiPresentationReceipt,
    prepared: UiPreparedPresentationAuthority,
) {
    let invalidated_receipt = prepared.receipt();
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let reducers = app
        .world()
        .resource::<crate::runtime::UiRuntimeState>()
        .surface(surface_id())
        .unwrap()
        .reducer_states()
        .clone();
    let route = observe_input_route(app);
    assert!(route.is_ui_routable());
    assert_eq!(
        route.presentation_epoch().unwrap().get(),
        receipt.presentation_epoch(),
    );

    assert_eq!(
        app.commit_presentation(prepared),
        UiPresentationCommit::Invalidated {
            reason: UiPresentationInvalidationReason::InteractionChanged,
        },
    );

    assert_eq!(
        observe_input_route(app),
        route,
        "UI invalidation must not publish or unbind a route",
    );
    assert_eq!(
        test_receipt(app.world(), receipt.presentation_epoch()),
        receipt,
    );
    assert!(
        crate::runtime::presented_receipt(
            app.world(),
            surface_id(),
            invalidated_receipt.presentation_epoch(),
        )
        .is_none(),
    );
    assert!(ui_semantic_snapshot(app.world(), invalidated_receipt, UiSeatId::PRIMARY).is_err());
    assert_eq!(
        ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap(),
        semantics,
    );
    assert_eq!(
        app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .surface(surface_id())
            .unwrap()
            .reducer_states(),
        &reducers,
    );
}

#[test]
fn ordinary_release_activation_survives_late_commit_without_restoring_capture() {
    let (mut app, receipt, prepared, cursor) = prepare_pressed_presentation();
    app.timeline()
        .admit_mouse_button(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            ButtonDriverInput::Release(MouseButton::Left),
        )
        .unwrap();
    assert!(
        app.world()
            .resource::<InputTimeline>()
            .entries()
            .last()
            .unwrap()
            .invalidation()
            .is_none(),
    );
    app.update().unwrap();
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let button = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap();
    assert!(button.state().focused());
    assert!(
        !button.state().pressed(),
        "ordinary release must finish the live pointer capture",
    );
    let activation = app
        .world()
        .resource::<UiActionJournal>()
        .pending(&cursor)
        .unwrap()
        .to_vec();
    assert_eq!(activation.len(), 1);
    assert_eq!(activation[0].action().as_str(), "retry-draft");
    assert_eq!(activation[0].source(), UiActivationSource::Pointer);

    assert_interaction_change_rejects_prepared(&mut app, receipt, prepared);

    assert!(
        app.timeline()
            .admit_mouse_button(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                ButtonDriverInput::Release(MouseButton::Left),
            )
            .unwrap()
            .is_ignored(),
    );
    app.update().unwrap();
    assert_eq!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap(),
        activation.as_slice(),
    );
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    assert!(
        !semantics
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .pressed(),
    );
}

#[test]
fn ordinary_move_outside_content_cannot_restore_cancelled_capture_on_late_commit() {
    let (mut app, receipt, prepared, cursor) = prepare_pressed_presentation();
    app.timeline()
        .admit_pointer(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(220.0, 10.0)),
        )
        .unwrap();
    assert!(
        app.world()
            .resource::<InputTimeline>()
            .entries()
            .last()
            .unwrap()
            .invalidation()
            .is_none(),
    );
    app.update().unwrap();
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let button = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap();
    assert!(button.state().focused());
    assert!(!button.state().pressed());
    let runtime = app.world().resource::<crate::runtime::UiRuntimeState>();
    let surface = runtime.surface(surface_id()).unwrap();
    assert!(!surface.interaction().has_pointer_facts(surface_id()));
    let reducer = surface.reducer_states().get(button.key()).unwrap();
    assert!(!reducer.hovered() && !reducer.pressed() && reducer.focused());

    assert_interaction_change_rejects_prepared(&mut app, receipt, prepared);

    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty(),
    );
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let retry = app
        .prepare_presentation(frame, 2)
        .expect("UI invalidation must release the route reservation for a fresh candidate");
    app.abort_presentation(retry);
}

#[test]
fn accessibility_focus_change_invalidates_prepared_state_without_an_input_sequence() {
    let (mut app, _button) = build_unstarted_button_app();
    let world = app.configuration_world();
    let root = world
        .iter_entities()
        .find_map(|entity| entity.contains::<UiRoot>().then_some(entity.id()))
        .unwrap();
    let other = world
        .spawn(button_bundle(
            "other",
            "other-action",
            UiColor::rgba(0.3, 0.5, 0.2, 1.0),
        ))
        .id();
    world
        .entity_mut(other)
        .insert(UiNode::new(UiStyle::absolute(110.0, 0.0, 80.0, 50.0)));
    HierarchyWriter::new(world)
        .place(other, child_last(root))
        .unwrap();
    app.update().unwrap();
    let (mut app, receipt, prepared, cursor) = prepare_pressed_presentation_in(app);
    let input_cursors = app.world().resource::<InputTimeline>().cursors();

    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::focus_target(
            receipt,
            UiSeatId::PRIMARY,
            expected_runtime_node("root", "other"),
        )),
        UiRouteDisposition::Consumed(_),
    ));

    assert_eq!(
        app.world().resource::<InputTimeline>().cursors(),
        input_cursors,
    );
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let original = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap();
    let other = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "other")
        .unwrap();
    assert!(!original.state().focused());
    assert!(original.state().pressed());
    assert!(other.state().focused());

    assert_interaction_change_rejects_prepared(&mut app, receipt, prepared);

    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty(),
    );
}

#[test]
fn pointer_left_invalidates_prepared_state_while_preserving_the_live_window_route() {
    let (mut app, receipt, prepared, cursor) = prepare_pressed_presentation();
    app.timeline()
        .admit_pointer(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            PointerDriverInput::Left,
        )
        .unwrap();
    app.update().unwrap();
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let button = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap();
    assert!(!button.state().pressed());
    assert!(button.state().focused());
    let route = observe_input_route(&mut app);
    assert!(route.is_ui_routable());

    assert_eq!(
        app.commit_presentation(prepared),
        UiPresentationCommit::Invalidated {
            reason: UiPresentationInvalidationReason::Input(InputInvalidationReason::PointerLeft),
        },
    );

    assert_eq!(observe_input_route(&mut app), route);
    assert_eq!(test_receipt(app.world(), 1), receipt);
    assert!(crate::runtime::presented_receipt(app.world(), surface_id(), 2).is_none());
    assert_eq!(
        ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap(),
        semantics,
    );
    assert!(
        !app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .surface(surface_id())
            .unwrap()
            .interaction()
            .has_pointer_facts(surface_id()),
    );
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty(),
    );
}

#[test]
fn newer_authored_candidate_does_not_invalidate_an_unchanged_live_interaction_basis() {
    let (mut app, receipt, prepared, _cursor) = prepare_pressed_presentation();
    let expected_receipt = prepared.receipt();
    let prepared_frame = Arc::clone(prepared.frame());
    let button = app
        .world()
        .iter_entities()
        .find_map(|entity| entity.contains::<UiButton>().then_some(entity.id()))
        .unwrap();
    app.set_button(
        button,
        UiButton::new(
            "Unpresented successor",
            UiActionId::new("unpresented-action").unwrap(),
            16.0,
        )
        .unwrap()
        .with_enabled(false),
    );
    app.update().unwrap();
    let latest = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    assert!(!Arc::ptr_eq(&latest, &prepared_frame));
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    assert!(
        semantics
            .nodes()
            .iter()
            .find(|node| node.name() == "retry")
            .unwrap()
            .state()
            .pressed(),
    );

    assert_eq!(
        app.commit_presentation(prepared),
        UiPresentationCommit::Committed(expected_receipt),
    );

    let runtime = app.world().resource::<crate::runtime::UiRuntimeState>();
    let presented = runtime
        .surface(surface_id())
        .unwrap()
        .resolve_presented(UiRouteStamp::from_receipt(expected_receipt))
        .unwrap();
    assert!(Arc::ptr_eq(presented.target().frame(), &prepared_frame));
    let semantics = ui_semantic_snapshot(app.world(), expected_receipt, UiSeatId::PRIMARY).unwrap();
    let button = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap();
    assert!(button.state().focused() && button.state().pressed());
    assert_eq!(button.actions()[0].as_str(), "retry-draft");
}

#[test]
fn committing_reconciled_candidate_cannot_rebase_an_older_pressed_candidate() {
    let (mut app, _receipt, pressed, cursor) = prepare_pressed_presentation();
    app.abort_presentation(pressed);
    let button = app
        .world()
        .iter_entities()
        .find_map(|entity| entity.contains::<UiButton>().then_some(entity.id()))
        .unwrap();
    let action_a = app.world().get::<UiButton>(button).unwrap().clone();

    app.set_button(
        button,
        UiButton::new("retry", UiActionId::new("action-b").unwrap(), 16.0).unwrap(),
    );
    app.update().unwrap();
    let c1 = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let prepared_c1 = app.prepare_presentation(Arc::clone(&c1), 2).unwrap();
    let receipt_c1 = prepared_c1.receipt();

    app.set_button(button, action_a);
    app.update().unwrap();
    let c2 = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    assert_ne!(
        c1.paint(),
        c2.paint(),
        "C2 still contains the live pressed A visual"
    );

    assert_eq!(
        app.commit_presentation(prepared_c1),
        UiPresentationCommit::Committed(receipt_c1),
    );
    assert!(
        ui_candidate_frame(app.world(), surface_id())
            .unwrap()
            .is_none(),
        "committing C1 must retire C2, which was reconciled against the previous live basis",
    );
    let semantics = ui_semantic_snapshot(app.world(), receipt_c1, UiSeatId::PRIMARY).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap();
    assert!(!retry.state().pressed());
    assert_eq!(retry.actions()[0].as_str(), "action-b");

    let release = app
        .timeline()
        .admit_mouse_button(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            ButtonDriverInput::Release(MouseButton::Left),
        )
        .unwrap();
    assert_eq!(release.admitted(), 1);
    match app.prepare_presentation(c2, 3) {
        Err(UiAdapterError::FrameNotLatest { surface }) => assert_eq!(surface, surface_id().get()),
        Err(error) => panic!("retired C2 must fail as an obsolete candidate: {error:?}"),
        Ok(stale) => {
            app.abort_presentation(stale);
            panic!("C2 cannot acquire C1's new live basis and restore the old A capture");
        }
    }
    app.update().unwrap();
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );

    let fresh = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    assert!(!Arc::ptr_eq(&fresh, &c1));
    let receipt = app.present_frame(fresh, 3).unwrap();
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let retry = semantics
        .nodes()
        .iter()
        .find(|node| node.name() == "retry")
        .unwrap();
    assert_eq!(retry.actions()[0].as_str(), "retry-draft");
    assert!(!retry.state().pressed());
    let runtime = app.world().resource::<crate::runtime::UiRuntimeState>();
    assert!(
        runtime
            .surface(surface_id())
            .unwrap()
            .reducer_states()
            .values()
            .all(|state| !state.pressed())
    );
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn drained_physical_endpoint_retirement_allows_later_ui_surface_retirement() {
    let (mut app, receipt, prepared, cursor) = prepare_pressed_presentation();
    app.abort_presentation(prepared);
    let other_surface = UiSurfaceId::new(8).unwrap();
    app.install_surface(test_slot(other_surface, 12, 200.0, 100.0))
        .unwrap();
    app.update().unwrap();
    let other_frame = ui_candidate_frame(app.world(), other_surface)
        .unwrap()
        .unwrap();
    let other_receipt = app.present_frame(other_frame, 1).unwrap();
    assert!(matches!(
        app.dispatch_accessibility(UiAccessibilityInput::focus_target(
            other_receipt,
            UiSeatId::PRIMARY,
            expected_runtime_node("root", "retry"),
        )),
        UiRouteDisposition::Consumed(_),
    ));

    app.timeline()
        .admit_window_event(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            InputWindowEvent::SurfaceRetired,
        )
        .unwrap();
    app.update().unwrap();
    assert_cancelled_presentation(&app, receipt);
    assert!(matches!(
        app.timeline().admit_keyboard(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            ButtonDriverInput::Press(KeyCode::Escape),
        ),
        Err(nara_input::InputIngressError::UnknownEndpoint { window, seat })
            if window == InputWindowId::new(11) && seat == InputSeatId::PRIMARY,
    ));
    let survivor = ui_semantic_snapshot(app.world(), other_receipt, UiSeatId::PRIMARY).unwrap();
    app.timeline()
        .admit_wheel(
            InputWindowId::new(12),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            Vec2::new(0.0, 1.0),
        )
        .unwrap();
    let retained = app
        .world()
        .resource::<InputTimeline>()
        .entries()
        .copied()
        .collect::<Vec<_>>();
    let survivor_route = retained.last().unwrap().route();
    let cursors = app.world().resource::<InputTimeline>().cursors();

    let retirement = app
        .retire_surface(surface_id())
        .expect("UI retirement must settle an already-drained physical endpoint")
        .unwrap();

    assert_eq!(retirement.surface_id(), surface_id());
    assert!(
        app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .is_retired(surface_id())
    );
    assert!(ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).is_err());
    assert_eq!(
        app.world()
            .resource::<InputTimeline>()
            .entries()
            .copied()
            .collect::<Vec<_>>(),
        retained
    );
    assert_eq!(app.world().resource::<InputTimeline>().cursors(), cursors);
    assert_eq!(
        ui_semantic_snapshot(app.world(), other_receipt, UiSeatId::PRIMARY).unwrap(),
        survivor
    );
    assert!(app.retire_surface(surface_id()).unwrap().is_none());
    app.timeline()
        .admit_wheel(
            InputWindowId::new(12),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
            Vec2::new(0.0, 1.0),
        )
        .unwrap();
    assert_eq!(
        app.world()
            .resource::<InputTimeline>()
            .entries()
            .last()
            .unwrap()
            .route(),
        survivor_route
    );
    assert!(
        app.world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap()
            .is_empty()
    );
}

fn retire_and_reconnect_with_gameplay_key(app: &mut UiTestApp) {
    let window = InputWindowId::new(11);
    app.timeline()
        .admit_window_event(
            window,
            InputSeatId::PRIMARY,
            InputWindowEvent::SurfaceRetired,
        )
        .unwrap();
    app.update().unwrap();
    assert!(matches!(
        app.timeline().admit_keyboard(
            window, InputSeatId::PRIMARY, ButtonDriverInput::Press(KeyCode::Escape),
        ),
        Err(nara_input::InputIngressError::UnknownEndpoint { window: source, seat })
            if source == window && seat == InputSeatId::PRIMARY,
    ));
    app.connect_endpoint(window, InputSeatId::PRIMARY)
        .expect("a retained UI claim must not pin the retired Input pool slot");
    app.timeline()
        .admit_keyboard(
            window,
            InputSeatId::PRIMARY,
            ButtonDriverInput::Press(KeyCode::Space),
        )
        .unwrap();
    assert!(
        !app.world()
            .resource::<InputTimeline>()
            .entries()
            .last()
            .unwrap()
            .route()
            .is_ui_routable(),
        "the replacement endpoint must own this press without a UI route",
    );
    app.update().unwrap();
    assert!(
        app.world()
            .resource::<nara_input::ButtonInput<KeyCode>>()
            .pressed(KeyCode::Space)
    );
    assert_eq!(
        app.world().resource::<CapturedGameplayPhases>().0,
        [ActionPhase::Started]
    );
}

fn assert_old_surface_retirement_preserves_replacement_key(app: &mut UiTestApp) {
    let retained = app
        .world()
        .resource::<InputTimeline>()
        .entries()
        .copied()
        .collect::<Vec<_>>();
    let cursors = app.world().resource::<InputTimeline>().cursors();
    app.retire_surface(surface_id()).unwrap().unwrap();
    assert!(
        app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .is_retired(surface_id())
    );
    assert_eq!(
        app.world()
            .resource::<InputTimeline>()
            .entries()
            .copied()
            .collect::<Vec<_>>(),
        retained
    );
    assert_eq!(app.world().resource::<InputTimeline>().cursors(), cursors);
    app.update().unwrap();
    assert!(
        app.world()
            .resource::<nara_input::ButtonInput<KeyCode>>()
            .pressed(KeyCode::Space)
    );
    assert_eq!(
        app.world().resource::<CapturedGameplayPhases>().0,
        [ActionPhase::Started]
    );
    assert!(
        app.world()
            .resource::<ActionOutcomes>()
            .as_slice()
            .is_empty()
    );

    let release = app
        .timeline()
        .admit_keyboard(
            InputWindowId::new(11),
            InputSeatId::PRIMARY,
            ButtonDriverInput::Release(KeyCode::Space),
        )
        .unwrap();
    assert_eq!(
        release.admitted(),
        1,
        "only the replacement's real release may end its hold"
    );
    app.update().unwrap();
    assert!(
        !app.world()
            .resource::<nara_input::ButtonInput<KeyCode>>()
            .pressed(KeyCode::Space)
    );
    assert_eq!(
        app.world().resource::<CapturedGameplayPhases>().0,
        [ActionPhase::Started, ActionPhase::Released],
    );
}

#[test]
fn suspended_surface_claim_cannot_retire_reconnected_endpoint_with_the_same_labels() {
    let (mut app, _button) = build_unstarted_button_app();
    install_gameplay_capture(
        &mut app,
        &[ActionBinding::key(
            ActionId::new("replacement-key").unwrap(),
            KeyCode::Space,
        )],
    );
    app.update().unwrap();
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    app.suspend_surface(surface_id(), UiSurfaceSuspensionReason::EndpointUnavailable)
        .unwrap()
        .unwrap();
    assert!(ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).is_err());

    retire_and_reconnect_with_gameplay_key(&mut app);
    assert_old_surface_retirement_preserves_replacement_key(&mut app);
}

#[test]
fn active_surface_claim_refuses_prepare_and_deferred_retry_after_same_labels_reconnect() {
    let (mut app, _button) = build_unstarted_button_app();
    install_gameplay_capture(
        &mut app,
        &[ActionBinding::key(
            ActionId::new("replacement-key").unwrap(),
            KeyCode::Space,
        )],
    );
    app.update().unwrap();
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let receipt = app.present_frame(frame, 1).unwrap();
    app.update().unwrap();
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();
    let prepared = app.prepare_presentation(frame, 2).unwrap();
    let deferred = app.defer_presentation(prepared);

    retire_and_reconnect_with_gameplay_key(&mut app);
    assert!(
        app.world()
            .resource::<crate::runtime::UiRuntimeState>()
            .surface(surface_id())
            .unwrap()
            .is_active()
    );
    let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
    let retained = app
        .world()
        .resource::<InputTimeline>()
        .entries()
        .copied()
        .collect::<Vec<_>>();
    let cursors = app.world().resource::<InputTimeline>().cursors();
    let frame = ui_candidate_frame(app.world(), surface_id())
        .unwrap()
        .unwrap();

    assert!(matches!(
        app.prepare_presentation(frame, 2),
        Err(UiAdapterError::Input(nara_input::InputIngressError::EndpointClaimExpired { window, seat }))
            if window == InputWindowId::new(11) && seat == InputSeatId::PRIMARY,
    ));
    assert!(matches!(
        app.retry_presentation(deferred),
        Err(UiAdapterError::Input(nara_input::InputIngressError::EndpointClaimExpired { window, seat }))
            if window == InputWindowId::new(11) && seat == InputSeatId::PRIMARY,
    ));
    assert_eq!(
        app.world()
            .resource::<InputTimeline>()
            .entries()
            .copied()
            .collect::<Vec<_>>(),
        retained
    );
    assert_eq!(app.world().resource::<InputTimeline>().cursors(), cursors);
    assert_eq!(
        ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap(),
        semantics
    );
    assert!(
        app.world()
            .resource::<nara_input::ButtonInput<KeyCode>>()
            .pressed(KeyCode::Space)
    );
    assert_eq!(
        app.world().resource::<CapturedGameplayPhases>().0,
        [ActionPhase::Started]
    );

    assert_old_surface_retirement_preserves_replacement_key(&mut app);
}

#[test]
fn unpresented_surface_claim_survives_geometry_update_without_adopting_replacement_endpoint() {
    let (mut app, _button) = build_unstarted_button_app();
    install_gameplay_capture(
        &mut app,
        &[ActionBinding::key(
            ActionId::new("replacement-key").unwrap(),
            KeyCode::Space,
        )],
    );
    app.update().unwrap();
    assert!(crate::runtime::presented_receipt(app.world(), surface_id(), 1).is_none());

    retire_and_reconnect_with_gameplay_key(&mut app);
    app.update_surface(
        surface_id(),
        test_endpoint(11),
        test_transform(200.0, 100.0, 400.0, 200.0, 1.0),
    )
    .expect("a geometry update must preserve the unpresented surface's original endpoint claim");
    assert_old_surface_retirement_preserves_replacement_key(&mut app);
}

#[test]
fn same_label_restoration_retains_accessibility_focus_only_for_the_original_endpoint_claim() {
    for replace_endpoint in [false, true] {
        let (mut app, _button) = build_button_app();
        let cursor = app
            .register_consumer(UiActionConsumerId::new("restored-claim-focus-test").unwrap())
            .unwrap();
        let frame = ui_candidate_frame(app.world(), surface_id())
            .unwrap()
            .unwrap();
        let receipt = app.present_frame(frame, 1).unwrap();
        let key = expected_runtime_node("root", "retry");
        assert!(matches!(
            app.dispatch_accessibility(UiAccessibilityInput::focus_target(
                receipt,
                UiSeatId::PRIMARY,
                key.clone(),
            )),
            UiRouteDisposition::Consumed(_),
        ));
        let semantics = ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).unwrap();
        assert!(
            semantics
                .nodes()
                .iter()
                .find(|node| node.key() == &key)
                .unwrap()
                .state()
                .focused()
        );

        let suspension = app
            .suspend_surface(surface_id(), UiSurfaceSuspensionReason::EndpointUnavailable)
            .unwrap()
            .unwrap();
        assert!(ui_semantic_snapshot(app.world(), receipt, UiSeatId::PRIMARY).is_err());
        assert!(
            app.world()
                .resource::<crate::runtime::UiRuntimeState>()
                .surface(surface_id())
                .unwrap()
                .reducer_states()
                .get(&key)
                .unwrap()
                .focused()
        );

        if replace_endpoint {
            app.timeline()
                .admit_window_event(
                    InputWindowId::new(11),
                    InputSeatId::PRIMARY,
                    InputWindowEvent::SurfaceRetired,
                )
                .unwrap();
            app.update().unwrap();
            assert!(matches!(
                app.timeline().admit_keyboard(
                    InputWindowId::new(11),
                    InputSeatId::PRIMARY,
                    ButtonDriverInput::Press(KeyCode::Escape),
                ),
                Err(nara_input::InputIngressError::UnknownEndpoint { window, seat })
                    if window == InputWindowId::new(11) && seat == InputSeatId::PRIMARY,
            ));
            assert!(
                app.world()
                    .resource::<crate::runtime::UiRuntimeState>()
                    .surface(surface_id())
                    .unwrap()
                    .reducer_states()
                    .get(&key)
                    .unwrap()
                    .focused(),
                "the suspended surface still retains A's focus before explicit restoration",
            );
            app.connect_endpoint(InputWindowId::new(11), InputSeatId::PRIMARY)
                .unwrap();
            app.connect_pointer(
                InputWindowId::new(11),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            )
            .unwrap();
        }

        app.restore_surface(
            surface_id(),
            test_endpoint(11),
            suspension.snapshot().slot().transform(),
        )
        .unwrap();
        {
            let runtime = app.world().resource::<crate::runtime::UiRuntimeState>();
            let reducers = runtime.surface(surface_id()).unwrap().reducer_states();
            if replace_endpoint {
                assert!(
                    reducers.is_empty(),
                    "restoring onto B must discard A's reducers"
                );
            } else {
                assert!(
                    reducers.get(&key).unwrap().focused(),
                    "restoring the same endpoint lifetime must preserve its intended focus",
                );
            }
        }
        app.update().unwrap();
        let frame = ui_candidate_frame(app.world(), surface_id())
            .unwrap()
            .unwrap();
        let restored = app.present_frame(frame, 2).unwrap();
        let semantics = ui_semantic_snapshot(app.world(), restored, UiSeatId::PRIMARY).unwrap();
        let retry = semantics
            .nodes()
            .iter()
            .find(|node| node.key() == &key)
            .unwrap();
        assert_eq!(
            retry.state().focused(),
            !replace_endpoint,
            "semantic focus must follow the nominal endpoint lifetime",
        );
        assert!(!retry.state().pressed());

        for input in [
            ButtonDriverInput::Press(KeyCode::Enter),
            ButtonDriverInput::Release(KeyCode::Enter),
        ] {
            let admitted = app
                .timeline()
                .admit_keyboard(InputWindowId::new(11), InputSeatId::PRIMARY, input)
                .unwrap();
            assert_eq!(admitted.admitted(), 1);
            app.update().unwrap();
        }
        let actions = app
            .world()
            .resource::<UiActionJournal>()
            .pending(&cursor)
            .unwrap();
        if replace_endpoint {
            assert!(
                actions.is_empty(),
                "B's Enter must not activate A's old focus"
            );
            assert!(
                app.world()
                    .resource::<crate::runtime::UiRuntimeState>()
                    .surface(surface_id())
                    .unwrap()
                    .reducer_states()
                    .values()
                    .all(|state| !state.focused() && !state.hovered() && !state.pressed())
            );
        } else {
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].action().as_str(), "retry-draft");
            assert_eq!(actions[0].source(), UiActivationSource::Keyboard);
        }
    }
}
