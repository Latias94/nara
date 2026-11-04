use nara_ui_kernel::{
    UiActivationSource, UiButtonPhase, UiInputEvent, UiInteractionSession, UiPointerButton,
    UiPointerId, UiRouteStamp, UiSeatId, UiSurfaceId,
};

use super::*;

fn controller() -> OwnedSceneView {
    OwnedSceneView::new(
        UiRootKey::runtime("editor.scene-view", "test-panel").unwrap(),
        vec![nara_text::bundled_font_face().unwrap()],
    )
    .unwrap()
}

fn environment(width: f32, height: f32) -> UiEnvironment {
    UiEnvironment::new(
        UiSurfaceId::new(94).unwrap(),
        width,
        height,
        1.5,
        1,
        1,
        1,
        1,
    )
    .unwrap()
}

fn input() -> SceneViewInput {
    SceneViewInput {
        image: Some(SceneViewImage {
            extent: Extent2d::new(800, 400).unwrap(),
            occurrence: 7,
        }),
        presented: true,
        editing: true,
        unsaved_dependencies: 0,
    }
}

fn center(rect: DisplayRect) -> UiPoint {
    UiPoint::new(
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    )
}

fn pointer(point: UiPoint, phase: UiButtonPhase) -> UiInputEvent {
    UiInputEvent::PointerButton {
        seat: UiSeatId::PRIMARY,
        pointer: UiPointerId::PRIMARY,
        button: UiPointerButton::Primary,
        phase,
        position: Some(point),
    }
}

fn routed(
    frame: &Arc<UiFrame>,
    events: impl IntoIterator<Item = UiInputEvent>,
) -> Vec<UiDispatchBatch> {
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal
        .install_surface(frame.environment().surface_id(), 1)
        .unwrap();
    let receipt = authority
        .submit(&mut journal, Arc::clone(frame), 1)
        .unwrap();
    let mut session = UiInteractionSession::default();
    events
        .into_iter()
        .map(|event| {
            session
                .dispatch(&journal, UiRouteStamp::from_receipt(receipt), event)
                .unwrap()
                .consumed()
                .expect("the real frame consumes the test input")
                .clone()
        })
        .collect()
}

#[test]
fn identical_output_reuses_the_exact_frame_but_new_occurrences_do_not() {
    let mut view = controller();
    assert!(view.compiled_frame().is_none());
    let environment = environment(800.0, 600.0);
    let original = view.compile(environment, input()).unwrap();
    let reused = view.compile(environment, input()).unwrap();
    assert!(Arc::ptr_eq(&original, &reused));
    assert!(Arc::ptr_eq(view.compiled_frame().unwrap(), &original));

    let mut next_input = input();
    next_input.image.as_mut().unwrap().occurrence += 1;
    let next = view.compile(environment, next_input).unwrap();
    assert!(!Arc::ptr_eq(&original, &next));
    assert_eq!(view.placement(&original).unwrap().image, input().image);
    assert_eq!(view.placement(&next).unwrap().image, next_input.image);
    assert_eq!(
        view.invalidated_controls(&original, &next).unwrap(),
        vec![view.viewport.clone()]
    );
}

#[test]
fn keyed_layout_keeps_a_valid_local_clip_in_tiny_panels_and_while_waiting() {
    let mut view = controller();
    for (width, height) in [(0.5, 0.5), (8.0, 1.0), (120.0, 16.0), (800.0, 600.0)] {
        let frame = view.compile(environment(width, height), input()).unwrap();
        let placement = view.placement(&frame).unwrap();
        let layout = frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(&view.viewport))
            .unwrap();
        assert!(placement.clip.is_valid());
        assert_eq!(placement.clip, display_rect(layout.rect()));
        assert!(placement.clip.min[0] >= 0.0 && placement.clip.min[1] >= 0.0);
        assert!(placement.clip.max[0] <= width && placement.clip.max[1] <= height);
        assert_eq!(
            placement.mapping.unwrap().panel_rect(),
            display_rect(layout.rect())
        );
    }

    let frame = view
        .compile(
            environment(800.0, 600.0),
            SceneViewInput {
                image: None,
                presented: false,
                unsaved_dependencies: 2,
                ..input()
            },
        )
        .unwrap();
    let placement = view.placement(&frame).unwrap();
    assert!(placement.clip.is_valid());
    assert!(placement.image.is_none() && placement.mapping.is_none());
    for text in [
        "Reset View",
        "Preparing Scene View",
        "Unsaved dependencies: 2",
        "Waiting for a Nara Scene image",
    ] {
        assert!(frame.semantics().iter().any(|node| node.name() == text));
    }
}

#[test]
fn picks_use_the_original_frames_pointer_image_and_editing_state() {
    let mut view = controller();
    let environment = environment(800.0, 600.0);
    let original = view.compile(environment, input()).unwrap();
    let original_placement = view.placement(&original).unwrap();
    let point = center(original_placement.mapping.unwrap().content_rect());
    let mut next_input = input();
    next_input.editing = false;
    next_input.image.as_mut().unwrap().occurrence += 1;
    let next = view.compile(environment, next_input).unwrap();
    view.set_pointer(&next, point).unwrap();
    let original_press = routed(&original, [pointer(point, UiButtonPhase::Pressed)]);
    assert!(
        view.dispatch(&original, &original_press[0])
            .unwrap()
            .is_none()
    );
    view.set_pointer(&original, point).unwrap();
    let pick = view
        .dispatch(&original, &original_press[0])
        .unwrap()
        .unwrap();
    assert_eq!(pick.point, [point.x(), point.y()]);
    assert_eq!(Some(pick.image), original_placement.image);
    assert_eq!(Some(pick.mapping), original_placement.mapping);
    let next_press = routed(&next, [pointer(point, UiButtonPhase::Pressed)]);
    assert!(view.dispatch(&next, &next_press[0]).unwrap().is_none());

    let preparing = view
        .compile(
            environment,
            SceneViewInput {
                presented: false,
                ..input()
            },
        )
        .unwrap();
    view.set_pointer(&preparing, point).unwrap();
    let press = routed(&preparing, [pointer(point, UiButtonPhase::Pressed)]);
    assert!(view.dispatch(&preparing, &press[0]).unwrap().is_none());
}

#[test]
fn letterbox_bars_and_missing_pointer_cannot_pick_or_pan() {
    let mut view = controller();
    let frame = view.compile(environment(800.0, 600.0), input()).unwrap();
    let placement = view.placement(&frame).unwrap();
    let content = placement.mapping.unwrap().content_rect();
    let point = UiPoint::new(400.0, (placement.clip.min[1] + content.min[1]) * 0.5);
    assert!(placement.clip.contains([point.x(), point.y()]));
    assert!(!content.contains([point.x(), point.y()]));
    assert!(!view.pan(&frame, [20.0, 10.0]).unwrap());
    view.set_pointer(&frame, point).unwrap();
    let presses = routed(&frame, [pointer(point, UiButtonPhase::Pressed)]);
    assert!(view.dispatch(&frame, &presses[0]).unwrap().is_none());
    assert!(!view.pan(&frame, [20.0, 10.0]).unwrap());
    assert!(!view.zoom(&frame, point, 3.0).unwrap());
    assert_eq!(view.uv, DisplayRect::UNIT);
}

#[test]
fn placement_intersects_the_actual_ancestor_clip_without_refitting_the_image() {
    let mut view = controller();
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                view.root.clone(),
                UiLayer::Base,
                UiStyle::fill().with_overflow(UiOverflow::Clip),
            )
            .with_child(
                UiNode::new(
                    UiStyle::absolute(-100.0, 40.0, 1000.0, 600.0).with_overflow(UiOverflow::Clip),
                )
                .with_key(local_key("viewport").unwrap()),
            ),
        )
        .build()
        .unwrap();
    let frame = view
        .compiler
        .compile(&snapshot, environment(800.0, 600.0), UiTheme::default())
        .unwrap();
    let placement =
        frame_placement(&frame, &view.viewport, input().image, DisplayRect::UNIT).unwrap();
    assert_eq!(
        placement.clip,
        DisplayRect {
            min: [0.0, 40.0],
            max: [800.0, 600.0],
        }
    );
    let mapping = placement.mapping.unwrap();
    assert_eq!(mapping.panel_rect().min, [-100.0, 40.0]);
    assert!(mapping.content_rect().contains([-50.0, 200.0]));
    assert!(mapping_at(placement, UiPoint::new(-50.0, 200.0)).is_none());
    assert!(mapping_at(placement, UiPoint::new(800.0, 200.0)).is_none());
    assert!(mapping_at(placement, UiPoint::new(400.0, 200.0)).is_some());
}

#[test]
fn prepaint_zoom_and_pan_accumulate_and_keep_the_cursor_target() {
    let mut view = controller();
    let environment = environment(800.0, 600.0);
    let frame = view.compile(environment, input()).unwrap();
    let mapping = view.placement(&frame).unwrap().mapping.unwrap();
    let content = mapping.content_rect();
    let point = UiPoint::new(
        content.min[0] + (content.max[0] - content.min[0]) * 0.25,
        content.min[1] + (content.max[1] - content.min[1]) * 0.75,
    );
    let target = mapping.panel_to_target([point.x(), point.y()]).unwrap();
    assert!(view.zoom(&frame, point, 2.0).unwrap());
    assert!(view.zoom(&frame, point, 3.0).unwrap());
    assert!((view.uv.max[0] - view.uv.min[0] - (-0.5_f32).exp()).abs() < 0.00001);
    let zoomed = mapping.with_uv(view.uv).unwrap();
    let next_target = zoomed.panel_to_target([point.x(), point.y()]).unwrap();
    for axis in 0..2 {
        assert!((target[axis] - next_target[axis]).abs() < 0.001);
    }
    let before_pan = view.uv;
    view.set_pointer(&frame, point).unwrap();
    assert!(view.pan(&frame, [8.0, -4.0]).unwrap());
    assert!(view.pan(&frame, [8.0, -4.0]).unwrap());
    let span = before_pan.max[0] - before_pan.min[0];
    assert!((view.uv.min[0] - (before_pan.min[0] - 16.0 / 800.0 * span)).abs() < 0.00001);
    assert!((view.uv.min[1] - (before_pan.min[1] + 8.0 / 400.0 * span)).abs() < 0.00001);
    assert_eq!(view.placement(&frame).unwrap().mapping.unwrap(), mapping);
    let next = view.compile(environment, input()).unwrap();
    assert_eq!(
        view.placement(&next).unwrap().mapping.unwrap().uv_rect(),
        view.uv
    );
}

#[test]
fn camera_limits_and_nonfinite_inputs_leave_valid_uvs() {
    let mut view = controller();
    let frame = view.compile(environment(800.0, 600.0), input()).unwrap();
    let point = center(
        view.placement(&frame)
            .unwrap()
            .mapping
            .unwrap()
            .content_rect(),
    );
    view.set_pointer(&frame, point).unwrap();
    assert!(view.zoom(&frame, point, f32::MAX).unwrap());
    assert_eq!(view.uv.max[0] - view.uv.min[0], 0.125);
    assert!(view.pan(&frame, [100000.0, -100000.0]).unwrap());
    assert_eq!(view.uv.min[0], 0.0);
    assert_eq!(view.uv.max[1], 1.0);
    assert!(DisplayRect::UNIT.contains(view.uv.min));
    assert!(DisplayRect::UNIT.contains(view.uv.max));
    let before = view.uv;
    for wheel in [0.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(!view.zoom(&frame, point, wheel).unwrap());
    }
    for delta in [[0.0, 0.0], [f32::NAN, 1.0], [1.0, f32::INFINITY]] {
        assert!(!view.pan(&frame, delta).unwrap());
    }
    assert!(!view.zoom(&frame, UiPoint::new(f32::NAN, 0.0), 1.0).unwrap());
    assert!(
        view.set_pointer(&frame, UiPoint::new(0.0, f32::INFINITY))
            .is_err()
    );
    assert_eq!(view.uv, before);
    assert!(view.zoom(&frame, point, -f32::MAX).unwrap());
    assert_eq!(view.uv, DisplayRect::UNIT);
}

#[test]
fn reset_reduces_release_and_cancellation_clears_pointer_owned_state() {
    let mut view = controller();
    let environment = environment(800.0, 600.0);
    let frame = view.compile(environment, input()).unwrap();
    let point = center(
        view.placement(&frame)
            .unwrap()
            .mapping
            .unwrap()
            .content_rect(),
    );
    view.zoom(&frame, point, 3.0).unwrap();
    let frame = view.compile(environment, input()).unwrap();
    let cropped = view.uv;
    let reset_rect = frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(view.reset.key()))
        .unwrap()
        .rect();
    let reset_point = center(display_rect(reset_rect));
    let click = routed(
        &frame,
        [
            pointer(reset_point, UiButtonPhase::Pressed),
            pointer(reset_point, UiButtonPhase::Released),
        ],
    );
    view.dispatch(&frame, &click[0]).unwrap();
    assert!(view.reset_state.pressed());
    assert_eq!(view.uv, cropped);
    view.set_pointer(&frame, point).unwrap();
    view.cancel_input().unwrap();
    assert!(!view.reset_state.pressed() && !view.reset_state.hovered());
    assert!(view.reset_state.focused());
    assert!(!view.pan(&frame, [10.0, 10.0]).unwrap());
    view.dispatch(&frame, &click[1]).unwrap();
    assert_eq!(view.uv, cropped);

    for dispatch in &click {
        assert!(view.dispatch(&frame, dispatch).unwrap().is_none());
    }
    assert_eq!(view.uv, DisplayRect::UNIT);
    assert_eq!(
        view.placement(&frame).unwrap().mapping.unwrap().uv_rect(),
        cropped
    );
    let next = view.compile(environment, input()).unwrap();
    assert_eq!(
        view.placement(&next).unwrap().mapping.unwrap().uv_rect(),
        DisplayRect::UNIT
    );
}

#[test]
fn reconciliation_cannot_reset_the_view_or_leave_a_remembered_pointer() {
    let mut view = controller();
    let frame = view.compile(environment(800.0, 600.0), input()).unwrap();
    let point = center(
        view.placement(&frame)
            .unwrap()
            .mapping
            .unwrap()
            .content_rect(),
    );
    view.zoom(&frame, point, 3.0).unwrap();
    let cropped = view.uv;
    let reset_point = center(display_rect(
        frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(view.reset.key()))
            .unwrap()
            .rect(),
    ));
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal
        .install_surface(frame.environment().surface_id(), 1)
        .unwrap();
    let receipt = authority
        .submit(&mut journal, Arc::clone(&frame), 1)
        .unwrap();
    let mut session = UiInteractionSession::default();
    let press = session
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(receipt),
            pointer(reset_point, UiButtonPhase::Pressed),
        )
        .unwrap();
    view.dispatch(&frame, press.consumed().unwrap()).unwrap();
    assert!(view.reset_state.pressed());
    view.set_pointer(&frame, point).unwrap();
    let cleanup = session
        .stage_candidate_presentation(
            frame.environment().surface_id(),
            UiSeatId::PRIMARY,
            Some(&frame),
            &frame,
            &[view.reset.key().clone()],
        )
        .unwrap()
        .unwrap();
    assert!(cleanup.dispatch().is_reconciliation());
    assert!(view.dispatch(&frame, cleanup.dispatch()).unwrap().is_none());
    assert!(!view.reset_state.pressed());
    assert!(!view.pan(&frame, [10.0, 10.0]).unwrap());
    assert_eq!(view.uv, cropped);
}

#[test]
fn keyboard_viewport_actions_do_not_guess_a_pointer_coordinate() {
    let mut view = controller();
    let frame = view.compile(environment(800.0, 600.0), input()).unwrap();
    let point = center(
        view.placement(&frame)
            .unwrap()
            .mapping
            .unwrap()
            .content_rect(),
    );
    view.set_pointer(&frame, point).unwrap();
    let action = routed(
        &frame,
        [UiInputEvent::SemanticAction {
            seat: UiSeatId::PRIMARY,
            target: view.viewport.clone(),
            action: UiActionId::new("nara.scene-view.pick").unwrap(),
        }],
    );
    assert!(view.dispatch(&frame, &action[0]).unwrap().is_none());
    view.zoom(&frame, point, 3.0).unwrap();
    let reset = routed(
        &frame,
        [
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: view.reset.key().clone(),
            },
            UiInputEvent::ActivateFocused {
                seat: UiSeatId::PRIMARY,
                source: UiActivationSource::Keyboard,
            },
        ],
    );
    for dispatch in &reset {
        view.dispatch(&frame, dispatch).unwrap();
    }
    assert_eq!(view.uv, DisplayRect::UNIT);
}

#[test]
fn only_exact_retained_arcs_and_matching_dispatch_revisions_are_accepted() {
    let mut view = controller();
    let environment = environment(800.0, 600.0);
    let original = view.compile(environment, input()).unwrap();
    let point = center(
        view.placement(&original)
            .unwrap()
            .mapping
            .unwrap()
            .content_rect(),
    );
    let press = routed(&original, [pointer(point, UiButtonPhase::Pressed)]);
    let mut other = controller();
    let foreign = other.compile(environment, input()).unwrap();
    assert_eq!(original.revision(), foreign.revision());
    assert!(view.placement(&foreign).is_err());
    assert!(view.set_pointer(&foreign, point).is_err());
    assert!(view.dispatch(&foreign, &press[0]).is_err());

    for unsaved_dependencies in 1..=2 {
        let next = view
            .compile(
                environment,
                SceneViewInput {
                    unsaved_dependencies,
                    ..input()
                },
            )
            .unwrap();
        assert!(
            view.invalidated_controls(&original, &next)
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            view.dispatch(&next, &press[0]),
            Err(OwnedSceneViewError::ForeignDispatch)
        ));
    }
    assert!(view.placement(&original).is_ok());
    view.compile(
        environment,
        SceneViewInput {
            unsaved_dependencies: 3,
            ..input()
        },
    )
    .unwrap();
    assert!(matches!(
        view.placement(&original),
        Err(OwnedSceneViewError::RetiredFrame)
    ));
    assert!(view.set_pointer(&original, point).is_err());
    assert!(view.dispatch(&original, &press[0]).is_err());
    assert!(view.pan(&original, [1.0, 1.0]).is_err());
    assert!(view.zoom(&original, point, 1.0).is_err());
}

#[test]
fn viewport_behavior_and_geometry_changes_retire_only_the_viewport() {
    let mut view = controller();
    let initial_environment = environment(800.0, 600.0);
    let mut previous = view.compile(initial_environment, input()).unwrap();
    for next_input in [
        SceneViewInput {
            editing: false,
            ..input()
        },
        SceneViewInput {
            presented: false,
            ..input()
        },
        input(),
    ] {
        let next = view.compile(initial_environment, next_input).unwrap();
        assert_eq!(
            view.invalidated_controls(&previous, &next).unwrap(),
            vec![view.viewport.clone()]
        );
        previous = next;
    }
    let point = center(
        view.placement(&previous)
            .unwrap()
            .mapping
            .unwrap()
            .content_rect(),
    );
    view.zoom(&previous, point, 3.0).unwrap();
    let zoomed = view.compile(initial_environment, input()).unwrap();
    assert_eq!(
        view.invalidated_controls(&previous, &zoomed).unwrap(),
        vec![view.viewport.clone()]
    );
    let resized = view.compile(environment(600.0, 400.0), input()).unwrap();
    assert_eq!(
        view.invalidated_controls(&zoomed, &resized).unwrap(),
        vec![view.viewport.clone()]
    );
}
