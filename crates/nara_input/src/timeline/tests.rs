use nara_app::{__RuntimeDriverPort, RuntimeState};
use nara_core::Vec2;

use super::*;
use crate::{
    ActionMaps, ButtonDriverInput, ButtonTransitionPhase, InputBinding, KeyCode, MouseButton,
    PointerDriverInput, semantic::ActionStaging,
};

fn slot_generation(value: u64) -> InputSlotGeneration {
    InputSlotGeneration::new(value).unwrap()
}

fn route_generation(value: u64) -> InputRouteGeneration {
    InputRouteGeneration::new(value).unwrap()
}

fn geometry_generation(value: u64) -> InputGeometryGeneration {
    InputGeometryGeneration::new(value).unwrap()
}

fn route(
    window: InputWindowId,
    surface: InputSurfaceId,
    seat: InputSeatId,
    epoch: u64,
) -> InputPresentedRoute {
    InputPresentedRoute::new(
        window,
        surface,
        seat,
        InputPresentationEpoch::new(epoch).unwrap(),
        slot_generation(1),
        route_generation(epoch),
        geometry_generation(epoch),
    )
}

fn drain_unmapped(timeline: &mut InputTimeline) {
    timeline.finish_projection();
    let mut observations = InputFrameObservations::default();
    timeline.publish_observations(&mut observations);
    timeline.finalize_ui_routing();
    timeline.stage_actions(&ActionStaging::default());
    drop(timeline.take_deferred_action_inputs());
    observations.clear();
    timeline.finish_frame_observations();
    timeline.reclaim_completed();
    assert_eq!(timeline.retained_len(), 0);
}

fn primary_staging() -> ActionStaging {
    let mut staging = ActionStaging::default();
    staging.observe_maps(&ActionMaps::default());
    staging
}

fn published_timeline() -> InputTimeline {
    let mut timeline = InputTimeline::with_limits(InputLimits::new(32, 4, 4, 8).unwrap()).unwrap();
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    timeline.connect_endpoint(window, seat).unwrap();
    timeline
        .connect_pointer(window, seat, InputPointerId::PRIMARY)
        .unwrap();
    timeline
        .publish_presented_route(route(window, InputSurfaceId::new(7), seat, 11))
        .unwrap();
    timeline
        .admit_window_event(window, seat, InputWindowEvent::Focused(true))
        .unwrap();
    drain_unmapped(&mut timeline);
    timeline
}

#[test]
fn fault_retirement_admits_only_surface_retirement_input() {
    let retired = __InputDriverEvent::Window {
        window: InputWindowId::new(1),
        seat: InputSeatId::PRIMARY,
        event: InputWindowEvent::SurfaceRetired,
    };
    let focused = __InputDriverEvent::Window {
        window: InputWindowId::new(1),
        seat: InputSeatId::PRIMARY,
        event: InputWindowEvent::Focused(false),
    };

    for state in [
        RuntimeState::Faulted,
        RuntimeState::Stopping,
        RuntimeState::CloseIncomplete,
    ] {
        assert!(<InputTimeline as __RuntimeDriverPort>::accepts_driver_input(state, &retired));
        assert!(!<InputTimeline as __RuntimeDriverPort>::accepts_driver_input(state, &focused));
    }
    assert!(
        <InputTimeline as __RuntimeDriverPort>::accepts_driver_input(
            RuntimeState::Running,
            &focused,
        )
    );
}

#[test]
fn global_sequence_preserves_cross_device_order() {
    let mut timeline = published_timeline();
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    let pointer = InputPointerId::PRIMARY;

    timeline
        .admit_pointer(
            window,
            seat,
            pointer,
            PointerDriverInput::Moved(Vec2::new(4.0, 8.0)),
        )
        .unwrap();
    timeline
        .admit_mouse_button(
            window,
            seat,
            pointer,
            ButtonDriverInput::Press(MouseButton::Left),
        )
        .unwrap();
    timeline
        .admit_pointer(
            window,
            seat,
            pointer,
            PointerDriverInput::Moved(Vec2::new(5.0, 9.0)),
        )
        .unwrap();
    timeline
        .admit_mouse_button(
            window,
            seat,
            pointer,
            ButtonDriverInput::Release(MouseButton::Left),
        )
        .unwrap();

    let entries = timeline.entries().copied().collect::<Vec<_>>();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.sequence())
            .collect::<Vec<_>>(),
        [2, 3, 4, 5]
    );
    assert!(
        entries
            .iter()
            .all(|entry| entry.origin() == InputEventOrigin::Platform)
    );
    assert!(matches!(entries[0].kind(), InputEventKind::PointerMoved));
    assert!(matches!(
        entries[1].kind(),
        InputEventKind::PointerButton {
            phase: ButtonTransitionPhase::Pressed,
            ..
        }
    ));
    assert_eq!(entries[1].logical_position(), Some(Vec2::new(4.0, 8.0)));
    assert_eq!(entries[3].logical_position(), Some(Vec2::new(5.0, 9.0)));
    assert_eq!(entries[1].press_sequence(), Some(entries[1].sequence()));
    assert_eq!(entries[3].press_sequence(), entries[1].press_sequence());
}

#[test]
fn keyboard_routes_keep_the_connected_origin_when_another_window_is_focused() {
    let mut timeline = published_timeline();
    let fallback = InputWindowId::new(99);
    let unpublished = InputWindowId::new(2);
    let other_seat = InputSeatId::new(8);
    timeline
        .connect_endpoint(fallback, InputSeatId::PRIMARY)
        .unwrap();
    timeline.connect_endpoint(unpublished, other_seat).unwrap();
    timeline
        .admit_keyboard(
            InputWindowId::new(1),
            InputSeatId::PRIMARY,
            ButtonDriverInput::Press(KeyCode::Enter),
        )
        .unwrap();
    timeline
        .admit_keyboard(
            fallback,
            InputSeatId::PRIMARY,
            ButtonDriverInput::Press(KeyCode::Enter),
        )
        .unwrap();
    timeline
        .admit_keyboard(
            unpublished,
            other_seat,
            ButtonDriverInput::Press(KeyCode::Space),
        )
        .unwrap();

    let entries = timeline.entries().copied().collect::<Vec<_>>();
    assert_eq!(entries[0].route().window(), InputWindowId::new(1));
    assert_eq!(entries[0].route().surface(), Some(InputSurfaceId::new(7)));
    assert!(entries[0].route().is_ui_routable());
    assert_eq!(entries[1].route().window(), fallback);
    assert!(!entries[1].route().is_ui_routable());
    assert_eq!(entries[2].route().window(), unpublished);
    assert!(!entries[2].route().is_ui_routable());
    assert_ne!(entries[0].press_sequence(), entries[1].press_sequence());
}

#[test]
fn presented_route_ownership_is_idempotent_conflict_safe_and_releasable() {
    let mut timeline = InputTimeline::default();
    let window = InputWindowId::new(3);
    let seat = InputSeatId::new(4);
    let first = InputSurfaceId::new(10);
    let second = InputSurfaceId::new(20);
    timeline.connect_endpoint(window, seat).unwrap();
    let declaration = InputPresentedRoute::new(
        window,
        first,
        seat,
        InputPresentationEpoch::new(7).unwrap(),
        slot_generation(3),
        route_generation(4),
        geometry_generation(5),
    );

    let initial = timeline.publish_presented_route(declaration).unwrap();
    assert_eq!(
        timeline.publish_presented_route(declaration).unwrap(),
        initial
    );
    assert_eq!(initial.slot_generation(), Some(slot_generation(3)));
    assert_eq!(initial.geometry_generation(), Some(geometry_generation(5)));
    let advanced = timeline
        .publish_presented_route(InputPresentedRoute::new(
            window,
            first,
            seat,
            InputPresentationEpoch::new(8).unwrap(),
            slot_generation(3),
            route_generation(5),
            geometry_generation(6),
        ))
        .unwrap();
    assert!(advanced.binding_generation() > initial.binding_generation());
    assert_eq!(initial.route_generation(), Some(route_generation(4)));
    assert_eq!(advanced.route_generation(), Some(route_generation(5)));
    assert_eq!(advanced.geometry_generation(), Some(geometry_generation(6)));
    assert_eq!(
        timeline.publish_presented_route(route(window, second, seat, 9)),
        Err(InputIngressError::RouteConflict {
            window,
            seat,
            current_surface: first,
            requested_surface: second,
        })
    );
    assert_eq!(
        timeline.publish_presented_route(declaration),
        Err(InputIngressError::PresentedRouteRegression { window, seat })
    );
    assert_eq!(
        timeline.unbind_presented_endpoint(window, seat, Some(initial)),
        Err(InputIngressError::PresentedRouteMismatch { window, seat })
    );
    timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    assert_eq!(timeline.entries().last().unwrap().route(), advanced);
    timeline
        .unbind_presented_endpoint(window, seat, Some(advanced))
        .unwrap();
    assert_eq!(
        timeline.unbind_presented_endpoint(window, seat, Some(advanced)),
        Ok(())
    );
    let reclaimed = timeline
        .publish_presented_route(route(window, second, seat, 9))
        .unwrap();
    assert_eq!(reclaimed.surface(), Some(second));
}

#[test]
fn prepared_route_reserves_the_endpoint_without_changing_active_authority() {
    let mut timeline = InputTimeline::default();
    let window = InputWindowId::new(31);
    let seat = InputSeatId::PRIMARY;
    let surface = InputSurfaceId::new(41);
    timeline.connect_endpoint(window, seat).unwrap();
    let initial = timeline
        .publish_presented_route(route(window, surface, seat, 1))
        .unwrap();
    let next_route = route(window, surface, seat, 2);
    let prepared = prepare_presented_route(&mut timeline, next_route).unwrap();

    assert_eq!(
        timeline.publish_presented_route(next_route),
        Err(InputIngressError::RouteReservationConflict { window, seat })
    );
    assert_eq!(
        timeline.unbind_presented_endpoint(window, seat, Some(initial)),
        Err(InputIngressError::RouteReservationConflict { window, seat })
    );
    assert_eq!(
        timeline.retire_presented_endpoint(window, seat, Some(initial)),
        Err(InputIngressError::RouteReservationConflict { window, seat })
    );
    timeline
        .connect_pointer(window, seat, InputPointerId::PRIMARY)
        .unwrap();
    let moved = timeline
        .admit_pointer(
            window,
            seat,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
        )
        .unwrap();
    assert_eq!(
        timeline
            .entries()
            .last()
            .unwrap()
            .route()
            .presentation_epoch(),
        initial.presentation_epoch()
    );
    assert_eq!(moved.admitted(), 1);

    let InputRouteCommit::Published(committed) = commit_presented_route(&mut timeline, prepared)
    else {
        panic!("an uninterrupted reservation must publish");
    };
    assert_eq!(
        committed.presentation_epoch(),
        Some(next_route.presentation_epoch())
    );
    assert_eq!(
        committed.route_generation(),
        Some(next_route.route_generation())
    );
    assert_eq!(
        committed.geometry_generation(),
        Some(next_route.geometry_generation())
    );
    assert!(committed.binding_generation() > initial.binding_generation());
    timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    assert_eq!(timeline.entries().last().unwrap().route(), committed);
}

#[test]
fn prepared_platform_loss_is_reported_without_rearming_the_route() {
    for (event, reason) in [
        (
            InputWindowEvent::Focused(false),
            InputInvalidationReason::FocusLost,
        ),
        (
            InputWindowEvent::Resized {
                logical_size: Vec2::new(640.0, 480.0),
            },
            InputInvalidationReason::Resized,
        ),
        (
            InputWindowEvent::ScaleFactorChanged { scale_factor: 2.0 },
            InputInvalidationReason::ScaleFactorChanged,
        ),
        (
            InputWindowEvent::CloseRequested,
            InputInvalidationReason::CloseRequested,
        ),
        (
            InputWindowEvent::SurfaceRetired,
            InputInvalidationReason::SurfaceRetired,
        ),
    ] {
        let mut timeline = published_timeline();
        let window = InputWindowId::new(1);
        let seat = InputSeatId::PRIMARY;
        let prepared = prepare_presented_route(
            &mut timeline,
            route(window, InputSurfaceId::new(7), seat, 12),
        )
        .unwrap();
        timeline.admit_window_event(window, seat, event).unwrap();
        assert!(matches!(
            commit_presented_route(&mut timeline, prepared),
            InputRouteCommit::Invalidated { reason: actual } if actual == reason
        ));
        assert_eq!(
            timeline.entries().last().unwrap().invalidation(),
            Some(reason)
        );
        if reason != InputInvalidationReason::SurfaceRetired {
            timeline
                .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
                .unwrap();
            assert!(!timeline.entries().last().unwrap().route().is_ui_routable());
        } else {
            assert_eq!(
                timeline.admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter)),
                Err(InputIngressError::EndpointRetiring { window, seat })
            );
        }
    }
}

#[test]
fn aborted_and_superseded_route_reservations_release_the_endpoint() {
    let mut timeline = InputTimeline::default();
    let window = InputWindowId::new(32);
    let seat = InputSeatId::PRIMARY;
    timeline.connect_endpoint(window, seat).unwrap();
    let declaration = route(window, InputSurfaceId::new(42), seat, 1);
    let aborted = prepare_presented_route(&mut timeline, declaration).unwrap();
    timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    assert!(!timeline.entries().last().unwrap().route().is_ui_routable());
    abort_presented_route(&mut timeline, aborted);
    let superseded = prepare_presented_route(&mut timeline, declaration).unwrap();
    supersede_presented_route(&mut timeline, superseded);
    let published = timeline.publish_presented_route(declaration).unwrap();
    assert_eq!(published.surface(), Some(declaration.surface()));
}

#[test]
fn prepared_routes_use_connected_capacity_until_their_retirement_is_drained() {
    let limits = InputLimits::new(16, 2, 1, 2).unwrap();
    let mut timeline = InputTimeline::with_limits(limits).unwrap();
    let window = InputWindowId::new(100);
    let replacement = InputWindowId::new(101);
    let seat = InputSeatId::PRIMARY;
    timeline.connect_endpoint(window, seat).unwrap();
    let prepared = prepare_presented_route(
        &mut timeline,
        route(window, InputSurfaceId::new(100), seat, 1),
    )
    .unwrap();
    assert_eq!(
        timeline.connect_endpoint(replacement, seat),
        Err(InputIngressError::EndpointCapacityExceeded { limit: 2 })
    );
    timeline
        .admit_window_event(window, seat, InputWindowEvent::SurfaceRetired)
        .unwrap();
    assert_eq!(
        timeline.connect_endpoint(window, seat),
        Err(InputIngressError::EndpointRetiring { window, seat })
    );
    drain_unmapped(&mut timeline);
    assert_eq!(
        timeline.connect_endpoint(replacement, seat),
        Err(InputIngressError::EndpointCapacityExceeded { limit: 2 })
    );
    abort_presented_route(&mut timeline, prepared);
    drain_unmapped(&mut timeline);
    timeline.connect_endpoint(replacement, seat).unwrap();
    let reusable = prepare_presented_route(
        &mut timeline,
        route(replacement, InputSurfaceId::new(101), seat, 1),
    )
    .unwrap();
    abort_presented_route(&mut timeline, reusable);
}

#[test]
fn prepared_route_rejects_regression_and_exhaustion_without_reserving_the_endpoint() {
    let mut timeline = InputTimeline::default();
    let window = InputWindowId::new(33);
    let seat = InputSeatId::PRIMARY;
    let surface = InputSurfaceId::new(43);
    timeline.connect_endpoint(window, seat).unwrap();
    let active = timeline
        .publish_presented_route(route(window, surface, seat, 5))
        .unwrap();
    assert!(matches!(
        prepare_presented_route(&mut timeline, route(window, surface, seat, 4)),
        Err(InputIngressError::PresentedRouteRegression { window: actual_window, seat: actual_seat })
            if actual_window == window && actual_seat == seat
    ));
    timeline.next_binding_generation = u64::MAX;
    assert!(matches!(
        prepare_presented_route(&mut timeline, route(window, surface, seat, 6)),
        Err(InputIngressError::RouteBindingGenerationExhausted)
    ));
    timeline
        .connect_pointer(window, seat, InputPointerId::PRIMARY)
        .unwrap();
    timeline
        .admit_pointer(
            window,
            seat,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
        )
        .unwrap();
    let retained_route = timeline.entries().last().unwrap().route();
    assert_eq!(retained_route.surface(), active.surface());
    assert_eq!(
        retained_route.binding_generation(),
        active.binding_generation()
    );
    timeline
        .unbind_presented_endpoint(window, seat, Some(active))
        .unwrap();
    timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    assert!(!timeline.entries().last().unwrap().route().is_ui_routable());
}

#[test]
fn unbind_and_physical_loss_do_not_mint_generations_or_revoke_other_routes() {
    let mut timeline = InputTimeline::default();
    let seat = InputSeatId::PRIMARY;
    let first = InputWindowId::new(1);
    let second = InputWindowId::new(2);
    for window in [first, second] {
        timeline.connect_endpoint(window, seat).unwrap();
        timeline
            .connect_pointer(window, seat, InputPointerId::PRIMARY)
            .unwrap();
    }
    let initial = timeline
        .publish_presented_route(route(first, InputSurfaceId::new(7), seat, 1))
        .unwrap();
    let unaffected = timeline
        .publish_presented_route(route(second, InputSurfaceId::new(8), seat, 1))
        .unwrap();
    timeline
        .unbind_presented_endpoint(first, seat, Some(initial))
        .unwrap();
    let replacement = timeline
        .publish_presented_route(route(first, InputSurfaceId::new(9), seat, 1))
        .unwrap();
    assert_eq!(
        replacement.binding_generation(),
        unaffected.binding_generation() + 1
    );
    timeline
        .admit_window_event(first, seat, InputWindowEvent::Focused(false))
        .unwrap();
    timeline
        .unbind_presented_endpoint(first, seat, Some(replacement))
        .unwrap();
    let republished = timeline
        .publish_presented_route(route(first, InputSurfaceId::new(9), seat, 2))
        .unwrap();
    assert_eq!(
        republished.binding_generation(),
        replacement.binding_generation() + 1
    );
    timeline.next_binding_generation = u64::MAX;
    timeline
        .unbind_presented_endpoint(first, seat, Some(republished))
        .unwrap();
    timeline
        .admit_window_event(first, seat, InputWindowEvent::Focused(false))
        .unwrap();
    assert_eq!(timeline.next_binding_generation, u64::MAX);
    timeline
        .admit_pointer(
            second,
            seat,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(2.0, 3.0)),
        )
        .unwrap();
    assert_eq!(
        timeline
            .entries()
            .last()
            .unwrap()
            .route()
            .binding_generation(),
        unaffected.binding_generation()
    );
    assert_eq!(
        timeline.entries().last().unwrap().route().surface(),
        unaffected.surface()
    );
}

#[test]
fn unbinding_a_route_preserves_the_platform_keyboard_focus() {
    let mut timeline = InputTimeline::default();
    let seat = InputSeatId::PRIMARY;
    let first = InputWindowId::new(3);
    let second = InputWindowId::new(4);
    for window in [first, second] {
        timeline.connect_endpoint(window, seat).unwrap();
    }
    let initial = timeline
        .publish_presented_route(route(first, InputSurfaceId::new(10), seat, 1))
        .unwrap();
    timeline
        .publish_presented_route(route(second, InputSurfaceId::new(20), seat, 2))
        .unwrap();
    timeline
        .admit_window_event(first, seat, InputWindowEvent::Focused(true))
        .unwrap();
    timeline
        .unbind_presented_endpoint(first, seat, Some(initial))
        .unwrap();
    assert!(
        timeline
            .topology
            .endpoints
            .iter()
            .flatten()
            .any(|endpoint| endpoint.window == first && endpoint.seat == seat && endpoint.focused)
    );
    timeline
        .admit_keyboard(first, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    let stamp = timeline.entries().last().unwrap().route();
    assert_eq!(stamp.window(), first);
    assert_eq!(stamp.surface(), None);
    assert!(!stamp.is_ui_routable());
}

#[test]
fn keyboard_releases_follow_the_physical_origin_after_focus_migration() {
    let mut timeline = InputTimeline::default();
    let seat = InputSeatId::PRIMARY;
    let first = InputWindowId::new(3);
    let second = InputWindowId::new(4);
    for window in [first, second] {
        timeline.connect_endpoint(window, seat).unwrap();
    }
    let initial = timeline
        .publish_presented_route(route(first, InputSurfaceId::new(10), seat, 1))
        .unwrap();
    timeline
        .publish_presented_route(route(second, InputSurfaceId::new(20), seat, 2))
        .unwrap();
    timeline
        .admit_window_event(first, seat, InputWindowEvent::Focused(true))
        .unwrap();
    let enter = timeline
        .admit_keyboard(first, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    let space = timeline
        .admit_keyboard(first, seat, ButtonDriverInput::Press(KeyCode::Space))
        .unwrap();
    timeline
        .unbind_presented_endpoint(first, seat, Some(initial))
        .unwrap();
    timeline
        .admit_window_event(second, seat, InputWindowEvent::Focused(true))
        .unwrap();

    assert!(
        timeline
            .admit_keyboard(second, seat, ButtonDriverInput::Release(KeyCode::Enter))
            .unwrap()
            .is_ignored()
    );
    assert!(
        timeline
            .admit_keyboard(second, seat, ButtonDriverInput::ReleaseAll)
            .unwrap()
            .is_ignored()
    );
    let individual = timeline
        .admit_keyboard(first, seat, ButtonDriverInput::Release(KeyCode::Enter))
        .unwrap();
    assert_eq!(individual.admitted(), 1);
    let release = *timeline.entries().last().unwrap();
    assert_eq!(release.press_sequence(), enter.first_sequence());
    assert!(!release.route().is_ui_routable());
    let all = timeline
        .admit_keyboard(first, seat, ButtonDriverInput::ReleaseAll)
        .unwrap();
    assert_eq!(all.admitted(), 1);
    let release = *timeline.entries().last().unwrap();
    assert_eq!(release.press_sequence(), space.first_sequence());
    assert!(matches!(
        release.kind(),
        InputEventKind::Key {
            key: KeyCode::Space,
            phase: ButtonTransitionPhase::Released
        }
    ));
    assert!(!release.route().is_ui_routable());
    assert!(
        timeline
            .admit_keyboard(first, seat, ButtonDriverInput::ReleaseAll)
            .unwrap()
            .is_ignored()
    );
}

#[test]
fn unbinding_a_presented_route_preserves_balanced_physical_edges() {
    let mut timeline = published_timeline();
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    let pointer = InputPointerId::PRIMARY;
    timeline
        .admit_pointer(
            window,
            seat,
            pointer,
            PointerDriverInput::Moved(Vec2::new(20.0, 30.0)),
        )
        .unwrap();
    let key = timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    let mouse = timeline
        .admit_mouse_button(
            window,
            seat,
            pointer,
            ButtonDriverInput::Press(MouseButton::Left),
        )
        .unwrap();
    let presented = timeline
        .entries()
        .last()
        .unwrap()
        .action_provenance()
        .press_route()
        .unwrap();
    timeline
        .unbind_presented_endpoint(window, seat, Some(presented))
        .unwrap();
    timeline
        .publish_presented_route(route(window, InputSurfaceId::new(7), seat, 12))
        .unwrap();
    let mouse_release = timeline
        .admit_mouse_button(
            window,
            seat,
            pointer,
            ButtonDriverInput::Release(MouseButton::Left),
        )
        .unwrap();
    let key_release = timeline
        .admit_keyboard(window, seat, ButtonDriverInput::ReleaseAll)
        .unwrap();
    assert_eq!(mouse_release.admitted(), 1);
    assert_eq!(key_release.admitted(), 1);
    let entries = timeline.entries().copied().collect::<Vec<_>>();
    let releases = &entries[entries.len() - 2..];
    assert!(releases.iter().all(|entry| !entry.route().is_ui_routable()));
    assert!(releases.iter().all(|entry| entry.invalidation().is_none()));
    assert_eq!(releases[0].logical_position(), None);
    assert_eq!(releases[0].press_sequence(), mouse.first_sequence());
    assert_eq!(releases[1].press_sequence(), key.first_sequence());
    assert!(matches!(
        releases[0].kind(),
        InputEventKind::PointerButton {
            button: MouseButton::Left,
            phase: ButtonTransitionPhase::Released
        }
    ));
    assert!(matches!(
        releases[1].kind(),
        InputEventKind::Key {
            key: KeyCode::Enter,
            phase: ButtonTransitionPhase::Released
        }
    ));
}

#[test]
fn presented_endpoint_retirement_balances_edges_and_waits_for_consumers_before_reuse() {
    let mut timeline = published_timeline();
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    let pointer = InputPointerId::PRIMARY;
    let presented = timeline
        .publish_presented_route(route(window, InputSurfaceId::new(7), seat, 11))
        .unwrap();
    timeline
        .admit_pointer(
            window,
            seat,
            pointer,
            PointerDriverInput::Moved(Vec2::new(20.0, 30.0)),
        )
        .unwrap();
    timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    timeline
        .admit_mouse_button(
            window,
            seat,
            pointer,
            ButtonDriverInput::Press(MouseButton::Left),
        )
        .unwrap();
    let before = timeline.retained_len();
    let retirement = timeline
        .retire_presented_endpoint(window, seat, Some(presented))
        .unwrap();
    assert_eq!(retirement.released_keys(), [KeyCode::Enter]);
    assert_eq!(retirement.released_mouse_buttons(), [MouseButton::Left]);
    assert_eq!(retirement.admission().admitted(), 4);
    assert_eq!(
        timeline.connect_endpoint(window, seat),
        Err(InputIngressError::EndpointRetiring { window, seat })
    );
    let lifecycle = timeline.entries().skip(before).copied().collect::<Vec<_>>();
    assert_eq!(lifecycle.len(), 4);
    assert!(
        lifecycle
            .iter()
            .all(|entry| entry.invalidation() == Some(InputInvalidationReason::SurfaceRetired))
    );
    assert!(matches!(
        lifecycle[0].kind(),
        InputEventKind::Key {
            key: KeyCode::Enter,
            phase: ButtonTransitionPhase::Released
        }
    ));
    assert!(matches!(
        lifecycle[1].kind(),
        InputEventKind::PointerButton {
            button: MouseButton::Left,
            phase: ButtonTransitionPhase::Released
        }
    ));
    assert!(matches!(lifecycle[2].kind(), InputEventKind::PointerLeft));
    assert!(matches!(
        lifecycle[3].kind(),
        InputEventKind::SurfaceRetired
    ));
    drain_unmapped(&mut timeline);
    timeline.connect_endpoint(window, seat).unwrap();
    timeline.connect_pointer(window, seat, pointer).unwrap();
    assert_eq!(
        timeline.unbind_presented_endpoint(window, seat, Some(presented)),
        Err(InputIngressError::PresentedRouteMismatch { window, seat })
    );
    assert_eq!(
        timeline.retire_presented_endpoint(window, seat, Some(presented)),
        Err(InputIngressError::PresentedRouteMismatch { window, seat })
    );
    timeline
        .publish_presented_route(route(window, InputSurfaceId::new(8), seat, 1))
        .unwrap();
    timeline
        .admit_window_event(window, seat, InputWindowEvent::Focused(true))
        .unwrap();
    assert_eq!(
        timeline
            .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
            .unwrap()
            .admitted(),
        1
    );
    assert_eq!(
        timeline
            .admit_mouse_button(
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left)
            )
            .unwrap()
            .admitted(),
        1
    );
}

#[test]
fn synchronous_ui_router_preserves_completed_prefix_and_original_release_ownership_on_unwind() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let mut timeline = published_timeline();
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    let first = timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap()
        .first_sequence()
        .unwrap();
    let second = timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Space))
        .unwrap()
        .first_sequence()
        .unwrap();
    let mut routed = Vec::new();
    let unwind = catch_unwind(AssertUnwindSafe(|| {
        timeline.route_pending_ui_input(|entry| {
            routed.push(entry.sequence());
            if entry.sequence() == second {
                panic!("the custom UI router failed after completing its first decision");
            }
            Some(InputUiDisposition::Consumed)
        });
    }));
    assert!(unwind.is_err());
    assert_eq!(routed, [first, second]);
    assert_eq!(
        timeline
            .pending_ui_entries()
            .map(|entry| entry.sequence())
            .collect::<Vec<_>>(),
        [second],
    );
    assert_eq!(timeline.route_pending_ui_input(|_| None), 0);
    timeline.finalize_ui_routing();
    timeline.stage_actions(&primary_staging());
    let gameplay = timeline.take_deferred_action_inputs();
    assert_eq!(gameplay.len(), 1);
    assert_eq!(gameplay[0].0.sequence(), second);

    for key in [KeyCode::Enter, KeyCode::Space] {
        timeline
            .admit_keyboard(window, seat, ButtonDriverInput::Release(key))
            .unwrap();
    }
    timeline.finalize_ui_routing();
    timeline.stage_actions(&primary_staging());
    let releases = timeline.take_deferred_action_inputs();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].0.press_sequence(), Some(second));
    assert!(matches!(
        releases[0].0.kind(),
        InputEventKind::Key {
            key: KeyCode::Space,
            phase: ButtonTransitionPhase::Released
        }
    ));
}

#[test]
fn retirement_releases_follow_the_consumer_that_received_each_press() {
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    let pointer = InputPointerId::PRIMARY;
    for consumed in [false, true] {
        let mut timeline = published_timeline();
        let staging = primary_staging();
        let key = timeline
            .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
            .unwrap();
        let presented = timeline.entries().last().unwrap().route();
        let mouse = timeline
            .admit_mouse_button(
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
        if consumed {
            for admission in [key, mouse] {
                timeline
                    .__finalize_ui_entry(
                        admission.first_sequence().unwrap(),
                        InputUiDisposition::Consumed,
                    )
                    .unwrap();
            }
        }
        timeline.finalize_ui_routing();
        timeline.stage_actions(&staging);
        assert_eq!(
            timeline.take_deferred_action_inputs().len(),
            if consumed { 0 } else { 2 }
        );
        timeline
            .retire_presented_endpoint(window, seat, Some(presented))
            .unwrap();
        timeline.finalize_ui_routing();
        timeline.stage_actions(&staging);
        let releases = timeline.take_deferred_action_inputs();
        assert_eq!(releases.len(), if consumed { 0 } else { 2 });
        if !consumed {
            assert!(
                releases
                    .iter()
                    .all(|(entry, _)| !entry.route().is_ui_routable())
            );
            assert!(releases.iter().any(|(entry, _)| matches!(
                entry.kind(),
                InputEventKind::Key {
                    key: KeyCode::Enter,
                    phase: ButtonTransitionPhase::Released
                }
            )));
            assert!(releases.iter().any(|(entry, _)| matches!(
                entry.kind(),
                InputEventKind::PointerButton {
                    button: MouseButton::Left,
                    phase: ButtonTransitionPhase::Released
                }
            )));
        }
    }
}

#[test]
fn retained_entries_keep_their_exact_slot_and_geometry_generations() {
    let mut timeline = InputTimeline::default();
    let window = InputWindowId::new(3);
    let surface = InputSurfaceId::new(10);
    let seat = InputSeatId::PRIMARY;
    timeline.connect_endpoint(window, seat).unwrap();
    timeline
        .connect_pointer(window, seat, InputPointerId::PRIMARY)
        .unwrap();
    for (slot, geometry, epoch, generation, position) in [
        (7, 11, 13, 9, Vec2::new(1.0, 2.0)),
        (8, 12, 14, 10, Vec2::new(3.0, 4.0)),
    ] {
        timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                surface,
                seat,
                InputPresentationEpoch::new(epoch).unwrap(),
                slot_generation(slot),
                route_generation(generation),
                geometry_generation(geometry),
            ))
            .unwrap();
        timeline
            .admit_pointer(
                window,
                seat,
                InputPointerId::PRIMARY,
                PointerDriverInput::Moved(position),
            )
            .unwrap();
    }
    let routes = timeline
        .pending_ui_entries()
        .map(|entry| entry.route())
        .collect::<Vec<_>>();
    assert_eq!(routes.len(), 2);
    for (stamp, (slot, generation, geometry)) in routes.iter().zip([(7, 9, 11), (8, 10, 12)]) {
        assert_eq!(stamp.slot_generation(), Some(slot_generation(slot)));
        assert_eq!(stamp.route_generation(), Some(route_generation(generation)));
        assert_eq!(
            stamp.geometry_generation(),
            Some(geometry_generation(geometry))
        );
    }
    assert!(routes[1].binding_generation() > routes[0].binding_generation());
}

#[test]
fn windows_seats_and_pointers_keep_distinct_event_time_routes() {
    let mut timeline = InputTimeline::default();
    let first = InputWindowId::new(1);
    let second = InputWindowId::new(2);
    for (window, seat, pointer, surface, epoch) in [
        (first, InputSeatId::PRIMARY, InputPointerId::new(3), 10, 100),
        (second, InputSeatId::new(5), InputPointerId::new(4), 20, 200),
    ] {
        timeline.connect_endpoint(window, seat).unwrap();
        timeline.connect_pointer(window, seat, pointer).unwrap();
        timeline
            .publish_presented_route(route(window, InputSurfaceId::new(surface), seat, epoch))
            .unwrap();
        timeline
            .admit_window_event(window, seat, InputWindowEvent::Focused(true))
            .unwrap();
        timeline
            .admit_pointer(
                window,
                seat,
                pointer,
                PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
            )
            .unwrap();
    }
    let routes = timeline
        .entries()
        .filter(|entry| matches!(entry.kind(), InputEventKind::PointerMoved))
        .map(|entry| entry.route())
        .collect::<Vec<_>>();
    assert_eq!(routes.len(), 2);
    for (stamp, (window, seat, pointer, surface, epoch)) in routes.iter().zip([
        (first, InputSeatId::PRIMARY, InputPointerId::new(3), 10, 100),
        (second, InputSeatId::new(5), InputPointerId::new(4), 20, 200),
    ]) {
        assert_eq!(stamp.window(), window);
        assert_eq!(stamp.surface(), Some(InputSurfaceId::new(surface)));
        assert_eq!(stamp.seat(), seat);
        assert_eq!(stamp.pointer(), Some(pointer));
        assert_eq!(
            stamp.presentation_epoch(),
            InputPresentationEpoch::new(epoch)
        );
    }
}

#[test]
fn pointer_leave_releases_only_the_exact_pointer_state() {
    let mut timeline = published_timeline();
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    let first = InputPointerId::new(3);
    let second = InputPointerId::new(4);
    for (index, pointer) in [first, second].into_iter().enumerate() {
        timeline.connect_pointer(window, seat, pointer).unwrap();
        timeline
            .admit_pointer(
                window,
                seat,
                pointer,
                PointerDriverInput::Moved(Vec2::new(index as f32, 2.0)),
            )
            .unwrap();
        timeline
            .admit_mouse_button(
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
    }
    assert_eq!(
        timeline
            .admit_pointer(window, seat, first, PointerDriverInput::Left)
            .unwrap()
            .admitted(),
        2
    );
    let entries = timeline.entries().copied().collect::<Vec<_>>();
    let cancellation = &entries[entries.len() - 2..];
    assert!(matches!(
        cancellation[0].kind(),
        InputEventKind::PointerButton {
            button: MouseButton::Left,
            phase: ButtonTransitionPhase::Released
        }
    ));
    assert!(matches!(
        cancellation[1].kind(),
        InputEventKind::PointerLeft
    ));
    assert!(
        cancellation
            .iter()
            .all(|entry| entry.route().pointer() == Some(first))
    );
    assert!(
        timeline
            .admit_mouse_button(
                window,
                seat,
                first,
                ButtonDriverInput::Release(MouseButton::Left)
            )
            .unwrap()
            .is_ignored()
    );
    assert_eq!(
        timeline
            .admit_mouse_button(
                window,
                seat,
                second,
                ButtonDriverInput::Release(MouseButton::Left)
            )
            .unwrap()
            .admitted(),
        1
    );
}

#[test]
fn losing_an_unfocused_window_does_not_release_the_focused_window_keys() {
    let mut timeline = InputTimeline::default();
    let first = InputWindowId::new(1);
    let second = InputWindowId::new(2);
    let seat = InputSeatId::PRIMARY;
    for (window, surface, epoch) in [(first, 10, 100), (second, 20, 200)] {
        timeline.connect_endpoint(window, seat).unwrap();
        timeline
            .publish_presented_route(route(window, InputSurfaceId::new(surface), seat, epoch))
            .unwrap();
        timeline
            .admit_window_event(window, seat, InputWindowEvent::Focused(true))
            .unwrap();
    }
    let pressed = timeline
        .admit_keyboard(second, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    let before = timeline.retained_len();
    timeline
        .admit_window_event(first, seat, InputWindowEvent::Focused(false))
        .unwrap();
    assert!(
        timeline
            .entries()
            .skip(before)
            .all(|entry| !matches!(entry.kind(), InputEventKind::Key { .. }))
    );
    assert_eq!(
        timeline
            .admit_keyboard(second, seat, ButtonDriverInput::Release(KeyCode::Enter))
            .unwrap()
            .admitted(),
        1
    );
    assert_eq!(
        timeline.entries().last().unwrap().press_sequence(),
        pressed.first_sequence()
    );
}

#[test]
fn independent_cursors_and_ui_consumption_preserve_paused_action_edges() {
    let mut timeline = published_timeline();
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    timeline
        .admit_pointer(
            window,
            seat,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
        )
        .unwrap();
    let pressed = timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    let mut observations = InputFrameObservations::default();
    timeline.publish_observations(&mut observations);
    assert_eq!(observations.as_slice().len(), 2);
    assert_eq!(timeline.cursors().observation(), 3);
    assert_eq!(timeline.cursors().ui(), 1);
    timeline
        .__finalize_ui_entry(
            pressed.first_sequence().unwrap(),
            InputUiDisposition::Consumed,
        )
        .unwrap();
    timeline.finalize_ui_routing();
    timeline.stage_actions(&primary_staging());
    let cursors = timeline.cursors();
    assert_eq!(cursors.ui(), 3);
    assert_eq!(cursors.action_scanned(), 3);
    assert_eq!(cursors.oldest_deferred_action(), pressed.first_sequence());
    assert!(timeline.take_deferred_action_inputs().is_empty());
    assert_eq!(timeline.cursors().oldest_deferred_action(), None);
}

#[test]
fn ordinary_saturation_preserves_reserved_observed_and_coordinated_retirement() {
    for coordinated in [false, true] {
        let mut timeline = published_timeline();
        let window = InputWindowId::new(1);
        let seat = InputSeatId::PRIMARY;
        let pointer = InputPointerId::PRIMARY;
        let presented = timeline
            .publish_presented_route(route(window, InputSurfaceId::new(7), seat, 11))
            .unwrap();
        timeline
            .admit_pointer(
                window,
                seat,
                pointer,
                PointerDriverInput::Moved(Vec2::new(20.0, 30.0)),
            )
            .unwrap();
        timeline
            .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
            .unwrap();
        timeline
            .admit_mouse_button(
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .unwrap();
        let mut refused = false;
        for _ in 0..=timeline.limits().events() {
            let retained = timeline.entries().copied().collect::<Vec<_>>();
            let cursors = timeline.cursors();
            let sequence = timeline.capacity.next_sequence();
            match timeline.admit_wheel(window, seat, pointer, Vec2::ZERO) {
                Ok(_) => {}
                Err(error) => {
                    assert!(matches!(error, InputIngressError::CapacityExceeded { .. }));
                    assert_eq!(timeline.entries().copied().collect::<Vec<_>>(), retained);
                    assert_eq!(timeline.cursors(), cursors);
                    assert_eq!(timeline.capacity.next_sequence(), sequence);
                    refused = true;
                    break;
                }
            }
        }
        assert!(
            refused,
            "ordinary records must leave their terminal reservations intact"
        );
        assert_eq!(
            timeline.capacity.retained() + timeline.capacity.obligations(),
            timeline.limits().events()
        );
        let before = timeline.retained_len();
        let admission = if coordinated {
            timeline
                .retire_presented_endpoint(window, seat, Some(presented))
                .unwrap()
                .admission()
        } else {
            timeline
                .admit_window_event(window, seat, InputWindowEvent::SurfaceRetired)
                .unwrap()
        };
        assert_eq!(admission.admitted(), 4);
        let terminal = timeline.entries().skip(before).copied().collect::<Vec<_>>();
        assert_eq!(terminal.len(), 4);
        assert!(
            terminal
                .iter()
                .all(|entry| entry.invalidation() == Some(InputInvalidationReason::SurfaceRetired))
        );
        assert!(matches!(
            terminal[0].kind(),
            InputEventKind::Key {
                phase: ButtonTransitionPhase::Released,
                ..
            }
        ));
        assert!(matches!(
            terminal[1].kind(),
            InputEventKind::PointerButton {
                phase: ButtonTransitionPhase::Released,
                ..
            }
        ));
        assert!(matches!(terminal[2].kind(), InputEventKind::PointerLeft));
        assert!(matches!(terminal[3].kind(), InputEventKind::SurfaceRetired));
        assert!(timeline.retained_len() <= timeline.limits().events());
    }
}

#[test]
fn rejected_press_does_not_publish_change_cursors_or_fabricate_a_release() {
    let mut timeline = InputTimeline::with_limits(InputLimits::new(12, 2, 1, 1).unwrap()).unwrap();
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    timeline.connect_endpoint(window, seat).unwrap();
    let accepted = timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    let before = timeline.entries().copied().collect::<Vec<_>>();
    let cursors = timeline.cursors();
    let sequence = timeline.capacity.next_sequence();
    assert_eq!(
        timeline.admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Space)),
        Err(InputIngressError::MemberCapacityExceeded { limit: 1 })
    );
    assert_eq!(timeline.entries().copied().collect::<Vec<_>>(), before);
    assert_eq!(timeline.cursors(), cursors);
    assert_eq!(timeline.capacity.next_sequence(), sequence);
    assert!(
        timeline
            .admit_keyboard(window, seat, ButtonDriverInput::Release(KeyCode::Space))
            .unwrap()
            .is_ignored()
    );
    let release = timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Release(KeyCode::Enter))
        .unwrap();
    assert_eq!(
        release.first_sequence(),
        accepted.last_sequence().map(|sequence| sequence + 1)
    );
    assert_eq!(
        timeline.entries().last().unwrap().press_sequence(),
        accepted.first_sequence()
    );
}

#[test]
fn sequence_exhaustion_does_not_partially_publish_or_change_pressed_state() {
    for headroom in [1, 2] {
        let mut timeline =
            InputTimeline::with_limits(InputLimits::new(16, 2, 1, 2).unwrap()).unwrap();
        let window = InputWindowId::new(1);
        let seat = InputSeatId::PRIMARY;
        timeline.connect_endpoint(window, seat).unwrap();
        let held = timeline
            .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
            .unwrap();
        drain_unmapped(&mut timeline);

        let obligations = timeline.capacity.obligations();
        let last = u64::MAX - u64::try_from(obligations + headroom).unwrap();
        timeline.capacity.set_last_sequence(last).unwrap();
        let cursors = timeline.cursors();
        let press = timeline.admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Space));

        if headroom == 1 {
            assert!(matches!(
                press,
                Err(InputIngressError::SequenceExhausted { .. })
            ));
            assert!(timeline.entries().next().is_none());
            assert_eq!(timeline.capacity.retained(), 0);
            assert_eq!(timeline.capacity.next_sequence(), last);
            assert_eq!(timeline.capacity.obligations(), obligations);
            assert_eq!(timeline.cursors(), cursors);
            let members = timeline
                .topology
                .members
                .iter()
                .flatten()
                .collect::<Vec<_>>();
            assert_eq!(members.len(), 1);
            assert_eq!(members[0].input, InputBinding::Key(KeyCode::Enter));
            assert!(members[0].pressed);
            assert!(
                timeline
                    .admit_keyboard(window, seat, ButtonDriverInput::Release(KeyCode::Space))
                    .unwrap()
                    .is_ignored()
            );
            assert!(timeline.entries().next().is_none());
        } else {
            let press = press.unwrap();
            assert_eq!(press.first_sequence(), Some(last + 1));
            assert_eq!(
                timeline.capacity.next_sequence()
                    + u64::try_from(timeline.capacity.obligations()).unwrap(),
                u64::MAX
            );
            let release = timeline
                .admit_keyboard(window, seat, ButtonDriverInput::Release(KeyCode::Space))
                .unwrap();
            assert_eq!(release.admitted(), 1);
            assert_eq!(
                timeline.entries().last().unwrap().press_sequence(),
                press.first_sequence()
            );
        }

        let release = timeline
            .admit_keyboard(window, seat, ButtonDriverInput::Release(KeyCode::Enter))
            .unwrap();
        assert_eq!(release.admitted(), 1);
        assert_eq!(
            timeline.entries().last().unwrap().press_sequence(),
            held.first_sequence()
        );
        for endpoint in [window, InputWindowId::UNSPECIFIED] {
            for event in [
                InputWindowEvent::Focused(false),
                InputWindowEvent::CloseRequested,
                InputWindowEvent::SurfaceRetired,
            ] {
                assert_eq!(
                    timeline
                        .admit_window_event(endpoint, seat, event)
                        .unwrap()
                        .admitted(),
                    1
                );
            }
        }
        assert_eq!(timeline.capacity.obligations(), 0);
        assert_eq!(
            timeline.capacity.next_sequence(),
            u64::MAX - u64::from(headroom == 1)
        );
        drain_unmapped(&mut timeline);
        assert!(timeline.topology.members.iter().all(Option::is_none));
    }
}

#[test]
fn invalidated_prepared_route_cannot_be_overwritten_before_its_exact_token_finishes() {
    let mut timeline = published_timeline();
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    let exact = route(window, InputSurfaceId::new(7), seat, 11);
    timeline
        .admit_pointer(
            window,
            seat,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
        )
        .unwrap();
    drain_unmapped(&mut timeline);
    let first = prepare_presented_route(&mut timeline, exact).unwrap();
    timeline
        .admit_pointer(
            window,
            seat,
            InputPointerId::PRIMARY,
            PointerDriverInput::Left,
        )
        .unwrap();
    assert_eq!(
        prepared_route_invalidation(&timeline, &first),
        Some(InputInvalidationReason::PointerLeft)
    );
    assert!(matches!(
        prepare_presented_route(&mut timeline, exact),
        Err(InputIngressError::RouteReservationConflict { .. })
    ));
    assert_eq!(
        commit_presented_route(&mut timeline, first),
        InputRouteCommit::Invalidated {
            reason: InputInvalidationReason::PointerLeft
        }
    );
    let second = prepare_presented_route(&mut timeline, exact).unwrap();
    assert!(matches!(
        commit_presented_route(&mut timeline, second),
        InputRouteCommit::Published(_)
    ));
}

#[test]
fn invalidated_first_route_refunds_unused_geometry_before_reusing_exact_capacity() {
    let mut timeline = InputTimeline::with_limits(InputLimits::new(10, 2, 2, 2).unwrap()).unwrap();
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    timeline.connect_endpoint(window, seat).unwrap();
    timeline
        .connect_pointer(window, seat, InputPointerId::PRIMARY)
        .unwrap();
    let prepared = prepare_presented_route(
        &mut timeline,
        route(window, InputSurfaceId::new(7), seat, 1),
    )
    .unwrap();
    timeline
        .admit_pointer(
            window,
            seat,
            InputPointerId::PRIMARY,
            PointerDriverInput::Moved(Vec2::new(1.0, 2.0)),
        )
        .unwrap();
    timeline
        .admit_pointer(
            window,
            seat,
            InputPointerId::PRIMARY,
            PointerDriverInput::Left,
        )
        .unwrap();
    assert_eq!(
        commit_presented_route(&mut timeline, prepared),
        InputRouteCommit::Invalidated {
            reason: InputInvalidationReason::PointerLeft
        }
    );
    drain_unmapped(&mut timeline);
    assert_eq!(timeline.capacity.obligations(), 6);
    timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Space))
        .unwrap();
    assert_eq!(
        timeline.retained_len() + timeline.capacity.obligations(),
        10
    );
    assert_eq!(
        timeline
            .admit_window_event(window, seat, InputWindowEvent::Focused(false))
            .unwrap()
            .admitted(),
        3
    );
}

#[test]
fn coordinated_endpoint_claim_cannot_retire_a_reconnected_unrouted_replacement() {
    let mut timeline = InputTimeline::with_limits(InputLimits::new(20, 2, 1, 2).unwrap()).unwrap();
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    timeline.connect_endpoint(window, seat).unwrap();
    let original = claim_input_endpoint(&timeline, window, seat).unwrap();
    timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    drain_unmapped(&mut timeline);
    timeline
        .admit_window_event(window, seat, InputWindowEvent::SurfaceRetired)
        .unwrap();
    drain_unmapped(&mut timeline);
    timeline
        .connect_endpoint(window, seat)
        .expect("an old identity claim does not pin the mutable endpoint slot");
    let replacement = claim_input_endpoint(&timeline, window, seat).unwrap();
    let press = timeline
        .admit_keyboard(window, seat, ButtonDriverInput::Press(KeyCode::Enter))
        .unwrap();
    drain_unmapped(&mut timeline);
    let cursors = timeline.cursors();
    let credits = timeline.capacity.obligations();
    assert!(
        retire_claimed_endpoint(&mut timeline, &original, None)
            .unwrap()
            .admission()
            .is_ignored()
    );
    unbind_claimed_endpoint(&mut timeline, &original, None).unwrap();
    assert_eq!(timeline.cursors(), cursors);
    assert_eq!(timeline.capacity.obligations(), credits);
    assert_eq!(timeline.retained_len(), 0);
    assert!(matches!(
        prepare_claimed_route(
            &mut timeline,
            &original,
            route(window, InputSurfaceId::new(7), seat, 1)
        ),
        Err(InputIngressError::EndpointClaimExpired { .. })
    ));
    let retirement = retire_claimed_endpoint(&mut timeline, &replacement, None).unwrap();
    assert_eq!(retirement.released_keys(), [KeyCode::Enter]);
    assert_eq!(
        timeline.entries().next().unwrap().press_sequence(),
        press.first_sequence()
    );
}

#[test]
fn endpoint_claim_refuses_a_foreign_owner_and_a_conflicting_live_route() {
    let mut first = InputTimeline::default();
    let mut second = InputTimeline::default();
    let window = InputWindowId::UNSPECIFIED;
    let seat = InputSeatId::PRIMARY;
    let claim = claim_input_endpoint(&first, window, seat).unwrap();
    assert_eq!(
        unbind_claimed_endpoint(&mut second, &claim, None),
        Err(InputIngressError::ForeignEndpointClaim)
    );
    assert_eq!(
        retire_claimed_endpoint(&mut second, &claim, None),
        Err(InputIngressError::ForeignEndpointClaim)
    );
    let active = first
        .publish_presented_route(route(window, InputSurfaceId::new(7), seat, 1))
        .unwrap();
    assert_eq!(
        retire_claimed_endpoint(&mut first, &claim, None),
        Err(InputIngressError::PresentedRouteMismatch { window, seat })
    );
    assert!(retire_claimed_endpoint(&mut first, &claim, Some(active)).is_ok());
}
