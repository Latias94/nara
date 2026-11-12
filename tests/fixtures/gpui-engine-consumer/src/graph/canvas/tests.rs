use super::*;
use nara_core::DocumentRevision;
use nara_engine::tooling::EditorDocumentId;

fn basis() -> GraphPreviewBasis {
    GraphPreviewBasis {
        document: EditorDocumentId::from_raw(7),
        revision: DocumentRevision::initial(),
    }
}

fn state(canvas: &GraphCanvas) -> (CanvasTransform, Option<Gesture>) {
    (canvas.transform(), canvas.gesture)
}

fn rendered(transform: CanvasTransform, position: [f32; 2]) -> [f32; 2] {
    std::array::from_fn(|axis| transform.pan[axis] + position[axis] * transform.zoom)
}

fn world_at(transform: CanvasTransform, pointer: [f32; 2]) -> [f64; 2] {
    std::array::from_fn(|axis| {
        (f64::from(pointer[axis]) - f64::from(transform.pan[axis])) / f64::from(transform.zoom)
    })
}

fn assert_world_near(actual: [f64; 2], expected: [f64; 2]) {
    for axis in 0..2 {
        assert!(
            (actual[axis] - expected[axis]).abs() < 0.0001,
            "world point changed: {actual:?} != {expected:?}"
        );
    }
}

#[test]
fn default_and_idle_operations_preserve_the_view_and_fallback() {
    let mut canvas = GraphCanvas::default();
    assert_eq!(canvas.transform(), CanvasTransform::default());
    assert_eq!(canvas.transform().pan, [0.0; 2]);
    assert_eq!(canvas.transform().zoom, 1.0);
    assert!(!canvas.is_active());
    assert_eq!(canvas.active_node(), None);
    assert_eq!(canvas.node_position(NodeId(1), [12.0, 34.0]), [12.0, 34.0]);
    assert!(!canvas.motion([10.0, 20.0]));
    assert_eq!(canvas.finish(CanvasButton::Primary, [10.0, 20.0]), None);
    assert_eq!(canvas.finish(CanvasButton::Middle, [10.0, 20.0]), None);
    assert!(!canvas.cancel());
    assert!(!canvas.pan_by([0.0; 2]));
    assert!(!canvas.zoom_by(1.0, [300.0, 200.0]));
    assert_eq!(canvas.transform(), CanvasTransform::default());
}

#[test]
fn clicks_and_wrong_releases_do_not_start_or_replace_a_node_preview() {
    let mut canvas = GraphCanvas::default();
    let basis = basis();
    let node = NodeId(1);
    let original = [20.0, 40.0];
    assert!(canvas.begin_node(basis, node, original, [100.0, 200.0]));
    assert!(canvas.is_active());
    assert_eq!(canvas.active_node(), Some(node));
    let before = state(&canvas);
    assert!(!canvas.begin_pan([0.0; 2]));
    assert!(!canvas.begin_node(basis, NodeId(2), [0.0; 2], [0.0; 2]));
    assert_eq!(state(&canvas), before);
    assert!(!canvas.motion([102.0, 202.0]));
    assert_eq!(canvas.node_position(node, [-1.0; 2]), original);
    assert_eq!(canvas.node_position(NodeId(2), [5.0; 2]), [5.0; 2]);
    assert_eq!(canvas.finish(CanvasButton::Middle, [400.0, 500.0]), None);
    assert_eq!(state(&canvas), before);
    assert_eq!(canvas.finish(CanvasButton::Primary, [102.0, 202.0]), None);
    assert!(!canvas.is_active());
    assert_eq!(canvas.active_node(), None);
    assert_eq!(canvas.transform(), CanvasTransform::default());
}

#[test]
fn node_threshold_is_logical_and_remains_crossed_when_the_pointer_returns() {
    for zoom in [0.5, 2.0] {
        let mut canvas = GraphCanvas::default();
        canvas.zoom_by(zoom, [0.0; 2]);
        let node = NodeId(1);
        let original = [20.0, 40.0];
        assert!(canvas.begin_node(basis(), node, original, [100.0, 200.0]));
        assert!(!canvas.motion([102.0, 202.0]));
        assert_eq!(canvas.node_position(node, original), original);
        assert!(canvas.motion([103.0, 200.0]));
        assert_eq!(
            canvas.node_position(node, original),
            [20.0 + 3.0 / zoom, 40.0]
        );
        assert!(canvas.motion([100.25, 200.5]));
        assert_eq!(
            canvas.node_position(node, original),
            [20.0 + 0.25 / zoom, 40.0 + 0.5 / zoom]
        );
        assert_eq!(canvas.finish(CanvasButton::Primary, [100.0, 200.0]), None);
        assert!(!canvas.is_active());
    }
}

#[test]
fn fractional_zoom_node_moves_round_trip_without_accumulation_drift() {
    for zoom in [0.75, 1.25] {
        let mut canvas = GraphCanvas::default();
        assert!(canvas.zoom_by(zoom, [0.0; 2]));
        assert!(canvas.pan_by([19.25, -31.5]));
        let transform = canvas.transform();
        let basis = basis();
        let node = NodeId(3);
        let original = [12.5, -3.75];
        let target = [18.5, -13.75];
        let start = rendered(transform, original);
        let end = rendered(transform, target);
        assert!(canvas.begin_node(basis, node, original, start));
        for step in 1..=200 {
            let fraction = step as f32 / 200.0;
            canvas.motion(std::array::from_fn(|axis| {
                start[axis] + (end[axis] - start[axis]) * fraction
            }));
        }
        assert_eq!(canvas.node_position(node, original), target);
        assert!(!canvas.motion(end));
        assert_eq!(
            canvas.finish(CanvasButton::Primary, end),
            Some(NodeMove {
                basis,
                node,
                position: target,
            })
        );
        assert_eq!(canvas.transform(), transform);
        assert!(canvas.begin_node(basis, node, target, end));
        assert_eq!(
            canvas.finish(CanvasButton::Primary, start),
            Some(NodeMove {
                basis,
                node,
                position: original,
            })
        );
    }
}

#[test]
fn matching_node_release_applies_its_final_sample_and_wrong_release_preserves_preview() {
    let mut canvas = GraphCanvas::default();
    let basis = basis();
    let node = NodeId(1);
    assert!(canvas.begin_node(basis, node, [20.0, 40.0], [100.0, 200.0]));
    assert!(canvas.motion([110.0, 205.0]));
    let before = state(&canvas);
    assert_eq!(canvas.finish(CanvasButton::Middle, [900.0, -300.0]), None);
    assert_eq!(state(&canvas), before);
    assert_eq!(
        canvas.finish(CanvasButton::Primary, [112.5, 208.0]),
        Some(NodeMove {
            basis,
            node,
            position: [32.5, 48.0],
        })
    );
    assert!(!canvas.is_active());
    assert_eq!(canvas.finish(CanvasButton::Primary, [500.0; 2]), None);
}

#[test]
fn node_drag_continues_outside_the_fixed_viewport_and_cancels_without_a_move() {
    let mut canvas = GraphCanvas::default();
    let node = NodeId(1);
    let original = [20.0, 40.0];
    assert!(canvas.begin_node(basis(), node, original, [50.0, 50.0]));
    assert!(canvas.motion([-200.0, 700.0]));
    assert_eq!(canvas.node_position(node, original), [-230.0, 690.0]);
    assert!(canvas.cancel());
    assert_eq!(canvas.node_position(node, original), original);
    assert_eq!(canvas.finish(CanvasButton::Primary, [-200.0, 700.0]), None);
    assert!(!canvas.cancel());
    assert_eq!(canvas.transform(), CanvasTransform::default());
}

#[test]
fn pan_threshold_wrong_release_and_cancellation_preserve_the_original_view() {
    let mut canvas = GraphCanvas::default();
    assert!(canvas.pan_by([17.5, -21.25]));
    let original = canvas.transform();
    assert!(canvas.begin_pan([100.0, 200.0]));
    assert_eq!(canvas.active_node(), None);
    assert!(!canvas.motion([102.0, 202.0]));
    assert_eq!(canvas.transform(), original);
    assert_eq!(canvas.finish(CanvasButton::Middle, [102.0, 202.0]), None);
    assert_eq!(canvas.transform(), original);
    assert!(canvas.begin_pan([100.0, 200.0]));
    assert!(canvas.motion([-200.0, 700.0]));
    assert_eq!(canvas.transform().pan, [-282.5, 478.75]);
    let before = state(&canvas);
    assert_eq!(canvas.finish(CanvasButton::Primary, [1000.0; 2]), None);
    assert_eq!(state(&canvas), before);
    assert!(canvas.cancel());
    assert_eq!(canvas.transform(), original);
    assert!(canvas.begin_pan([100.0, 200.0]));
    assert_eq!(canvas.finish(CanvasButton::Middle, [104.0, 208.0]), None);
    assert_eq!(canvas.transform().pan, [21.5, -13.25]);
    assert!(!canvas.is_active());
}

#[test]
fn pan_at_limit_then_wheel_then_stationary_release_preserves_the_exact_result() {
    for (delta, unit) in [
        ([-16.0, 0.0], ScrollUnit::Pixels),
        ([-0.5, 0.0], ScrollUnit::Lines),
    ] {
        let mut canvas = GraphCanvas::default();
        assert!(canvas.pan_by([14_990.0, 0.0]));
        assert!(canvas.begin_pan([200.0, 300.0]));
        assert!(canvas.motion([260.0, 300.0]));
        assert_eq!(canvas.transform().pan, [15_000.0, 0.0]);
        assert!(canvas.scroll([270.0, 300.0], delta, unit, false));
        assert_eq!(canvas.transform().pan, [14_984.0, 0.0]);
        assert!(canvas.is_active());
        assert!(!canvas.motion([270.0, 300.0]));
        assert_eq!(canvas.finish(CanvasButton::Middle, [270.0, 300.0]), None);
        assert_eq!(canvas.transform().pan, [14_984.0, 0.0]);
    }
}

#[test]
fn wheel_rebase_keeps_pan_continuous_and_sets_its_cancellation_baseline() {
    for zoom in [false, true] {
        let mut canvas = GraphCanvas::default();
        assert!(canvas.pan_by([14_990.0, 0.0]));
        assert!(canvas.begin_pan([200.0, 300.0]));
        assert!(canvas.motion([260.0, 300.0]));
        assert!(canvas.scroll([270.0, 300.0], [-16.0, -32.0], ScrollUnit::Pixels, zoom));
        let rebased = canvas.transform();
        assert!(!canvas.motion([270.0, 300.0]));
        assert!(canvas.motion([269.75, 300.5]));
        assert_eq!(
            canvas.transform().pan,
            [rebased.pan[0] - 0.25, rebased.pan[1] + 0.5]
        );
        assert!(canvas.cancel());
        assert_eq!(canvas.transform(), rebased);
    }
}

#[test]
fn scroll_normalizes_lines_and_pixels_for_both_pan_and_multiplicative_zoom() {
    for zoom in [false, true] {
        let mut lines = GraphCanvas::default();
        let mut pixels = GraphCanvas::default();
        let pointer = [96.0, 64.0];
        assert!(lines.scroll(pointer, [0.5, -0.75], ScrollUnit::Lines, zoom));
        assert!(pixels.scroll(pointer, [16.0, -24.0], ScrollUnit::Pixels, zoom));
        assert_eq!(lines.transform(), pixels.transform());
        if zoom {
            assert_eq!(lines.transform().zoom, (-24.0_f64 * 0.0015).exp() as f32);
            assert_world_near(world_at(lines.transform(), pointer), pointer.map(f64::from));
        } else {
            assert_eq!(lines.transform().pan, [16.0, -24.0]);
            assert_eq!(lines.transform().zoom, 1.0);
        }
    }
}

#[test]
fn wheel_cancels_node_previews_before_applying_pan_or_zoom() {
    for zoom in [false, true] {
        let mut canvas = GraphCanvas::default();
        let node = NodeId(1);
        let original = [20.0, 40.0];
        assert!(canvas.begin_node(basis(), node, original, [100.0, 200.0]));
        assert!(canvas.motion([110.0, 210.0]));
        assert!(canvas.scroll([110.0, 210.0], [0.5, 1.0], ScrollUnit::Lines, zoom));
        assert!(!canvas.is_active());
        assert_eq!(canvas.node_position(node, original), original);
        assert_eq!(canvas.finish(CanvasButton::Primary, [110.0, 210.0]), None);
        if zoom {
            assert!(canvas.transform().zoom > 1.0);
        } else {
            assert_eq!(canvas.transform().pan, [16.0, 32.0]);
        }
    }
}

#[test]
fn zero_relevant_scroll_preserves_both_node_and_pan_gestures() {
    for pan in [false, true] {
        let mut canvas = GraphCanvas::default();
        if pan {
            assert!(canvas.begin_pan([10.0; 2]));
        } else {
            assert!(canvas.begin_node(basis(), NodeId(1), [0.0; 2], [10.0; 2]));
        }
        assert!(canvas.motion([20.0; 2]));
        let before = state(&canvas);
        assert!(!canvas.scroll([50.0; 2], [0.0; 2], ScrollUnit::Lines, false));
        assert!(!canvas.scroll([50.0; 2], [10.0, 0.0], ScrollUnit::Pixels, true));
        assert_eq!(state(&canvas), before);
    }
}

#[test]
fn anchored_zoom_preserves_the_world_point_through_both_zoom_bounds() {
    let mut canvas = GraphCanvas::default();
    assert!(canvas.pan_by([100.0, -40.0]));
    assert!(canvas.zoom_by(0.75, [0.0; 2]));
    let anchor = [180.0, 92.0];
    let world = world_at(canvas.transform(), anchor);
    assert!(canvas.zoom_by(f32::MAX, anchor));
    assert_eq!(canvas.transform().zoom, 2.0);
    assert_world_near(world_at(canvas.transform(), anchor), world);
    let upper = canvas.transform();
    assert!(!canvas.zoom_by(f32::MAX, anchor));
    assert_eq!(canvas.transform(), upper);
    assert!(canvas.zoom_by(f32::from_bits(1), anchor));
    assert_eq!(canvas.transform().zoom, 0.5);
    assert_world_near(world_at(canvas.transform(), anchor), world);
    let lower = canvas.transform();
    assert!(!canvas.zoom_by(0.25, anchor));
    assert_eq!(canvas.transform(), lower);
    assert!(canvas.zoom_by(2.0, anchor));
    assert_eq!(canvas.transform().zoom, 1.0);
    assert_world_near(world_at(canvas.transform(), anchor), world);
}

#[test]
fn explicit_view_changes_cancel_gestures_before_using_the_restored_pan() {
    for zoom in [false, true] {
        let mut canvas = GraphCanvas::default();
        assert!(canvas.pan_by([40.0, -20.0]));
        assert!(canvas.begin_pan([100.0; 2]));
        assert!(canvas.motion([140.0, 150.0]));
        if zoom {
            assert!(canvas.zoom_by(2.0, [0.0; 2]));
            assert_eq!(canvas.transform().pan, [80.0, -40.0]);
            assert_eq!(canvas.transform().zoom, 2.0);
        } else {
            assert!(canvas.pan_by([3.0, 4.0]));
            assert_eq!(canvas.transform().pan, [43.0, -16.0]);
        }
        assert!(!canvas.is_active());
        assert_eq!(canvas.finish(CanvasButton::Middle, [140.0, 150.0]), None);
        let original = [20.0, 40.0];
        assert!(canvas.begin_node(basis(), NodeId(1), original, [100.0; 2]));
        assert!(canvas.motion([110.0; 2]));
        if zoom {
            assert!(canvas.zoom_by(1.0, [0.0; 2]));
        } else {
            assert!(canvas.pan_by([0.0; 2]));
        }
        assert!(!canvas.is_active());
        assert_eq!(canvas.node_position(NodeId(1), original), original);
    }
}

#[test]
fn rejected_numeric_inputs_preserve_idle_node_and_pan_state_exactly() {
    for mode in 0..3 {
        let mut canvas = GraphCanvas::default();
        let basis = basis();
        assert!(canvas.pan_by([15.0, -21.0]));
        assert!(canvas.zoom_by(0.75, [64.0, 32.0]));
        match mode {
            1 => assert!(canvas.begin_node(basis, NodeId(1), [20.0; 2], [10.0; 2])),
            2 => assert!(canvas.begin_pan([10.0; 2])),
            _ => {}
        }
        canvas.motion([20.0, 25.0]);
        let before = state(&canvas);
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for axis in 0..2 {
                let mut point = [10.0, 20.0];
                point[axis] = invalid;
                assert!(!canvas.begin_node(basis, NodeId(2), point, [0.0; 2]));
                assert!(!canvas.begin_node(basis, NodeId(2), [0.0; 2], point));
                assert!(!canvas.begin_pan(point));
                assert!(!canvas.motion(point));
                assert_eq!(canvas.finish(CanvasButton::Primary, point), None);
                assert_eq!(canvas.finish(CanvasButton::Middle, point), None);
                assert!(!canvas.pan_by(point));
                assert!(!canvas.zoom_by(1.25, point));
                for zoom in [false, true] {
                    for unit in [ScrollUnit::Lines, ScrollUnit::Pixels] {
                        assert!(!canvas.scroll(point, [1.0; 2], unit, zoom));
                        assert!(!canvas.scroll([0.0; 2], point, unit, zoom));
                    }
                }
                assert_eq!(state(&canvas), before);
            }
        }
        for factor in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!canvas.zoom_by(factor, [0.0; 2]));
            assert_eq!(state(&canvas), before);
        }
    }
}

#[test]
fn finite_extremes_cannot_overflow_node_motion_pan_or_wheel_zoom() {
    let mut canvas = GraphCanvas::default();
    let basis = basis();
    let node = NodeId(1);
    let negative = [-f32::MAX, f32::MAX];
    let positive = [f32::MAX, -f32::MAX];
    assert!(canvas.begin_node(basis, node, [1.0, -1.0], negative));
    assert_eq!(
        canvas.finish(CanvasButton::Primary, positive),
        Some(NodeMove {
            basis,
            node,
            position: [10_000.0, -10_000.0],
        })
    );
    assert!(canvas.begin_pan(negative));
    assert!(canvas.motion(positive));
    assert_eq!(canvas.transform().pan, [15_000.0, -15_000.0]);
    assert!(canvas.scroll(positive, negative, ScrollUnit::Lines, false));
    let rebased = canvas.transform();
    assert_eq!(rebased.pan, [-15_000.0, 15_000.0]);
    assert_eq!(canvas.finish(CanvasButton::Middle, positive), None);
    assert_eq!(canvas.transform(), rebased);
    assert!(canvas.scroll(positive, [0.0, f32::MAX], ScrollUnit::Lines, true));
    assert_eq!(canvas.transform().zoom, 2.0);
    assert!(canvas.scroll(positive, [0.0, -f32::MAX], ScrollUnit::Lines, true));
    assert_eq!(canvas.transform().zoom, 0.5);
    assert!(canvas.zoom_by(f32::MAX, positive));
    assert_eq!(canvas.transform().zoom, 2.0);
    assert!(canvas.pan_by(positive));
    assert_eq!(canvas.transform().pan, [15_000.0, -15_000.0]);
    assert!(canvas.pan_by(negative));
    assert_eq!(canvas.transform().pan, [-15_000.0, 15_000.0]);
}

#[test]
fn node_clamping_keeps_the_original_drag_basis_and_avoids_a_no_change_move() {
    let mut canvas = GraphCanvas::default();
    assert!(canvas.zoom_by(0.5, [0.0; 2]));
    let node = NodeId(1);
    let original = [9_999.0, -9_999.0];
    assert!(canvas.begin_node(basis(), node, original, [0.0; 2]));
    assert!(canvas.motion([20.0, -20.0]));
    assert_eq!(canvas.node_position(node, original), [10_000.0, -10_000.0]);
    assert!(!canvas.motion([20.0, -20.0]));
    assert!(canvas.motion([-1.0, 1.0]));
    assert_eq!(canvas.node_position(node, original), [9_997.0, -9_997.0]);
    assert_eq!(canvas.finish(CanvasButton::Primary, [0.0; 2]), None);
    let bound = [10_000.0, -10_000.0];
    assert!(canvas.begin_node(basis(), node, bound, [0.0; 2]));
    assert_eq!(canvas.finish(CanvasButton::Primary, [20.0, -20.0]), None);
    assert!(!canvas.is_active());
}

#[test]
fn independent_instances_keep_same_basis_gestures_and_transforms_separate() {
    let mut first = GraphCanvas::default();
    let mut second = GraphCanvas::default();
    let basis = basis();
    let node = NodeId(1);
    let original = [20.0, 40.0];
    assert!(first.begin_node(basis, node, original, [10.0; 2]));
    assert!(second.begin_node(basis, node, original, [10.0; 2]));
    assert!(first.motion([16.0, 10.0]));
    assert!(second.motion([0.0, 22.0]));
    assert_eq!(first.node_position(node, original), [26.0, 40.0]);
    let second_before = state(&second);
    assert!(first.cancel());
    assert!(first.pan_by([10.0, 20.0]));
    assert!(first.zoom_by(1.25, [40.0, 80.0]));
    assert_eq!(state(&second), second_before);
    assert_eq!(
        second.finish(CanvasButton::Primary, [0.0, 22.0]),
        Some(NodeMove {
            basis,
            node,
            position: [10.0, 52.0],
        })
    );
    assert_eq!(second.transform(), CanvasTransform::default());
    assert_eq!(first.node_position(node, original), original);
}
