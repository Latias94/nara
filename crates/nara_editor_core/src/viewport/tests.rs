use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, FRAC_PI_4, PI, TAU};

use nara_core::{Affine2, EntityUuid};

use super::*;

fn camera(zoom: f32) -> ViewportCamera2D {
    ViewportCamera2D::new(
        PanelSize::new(800.0, 600.0).unwrap(),
        WorldPoint(Vec2::ZERO),
        zoom,
    )
    .unwrap()
}

fn entity(serial: u64) -> EntityUuid {
    format!("00000000-0000-0000-0000-{serial:012x}")
        .parse()
        .unwrap()
}

fn unit_bounds() -> LocalBounds2D {
    LocalBounds2D::new(-Vec2::ONE, Vec2::ONE).unwrap()
}

fn assert_near(actual: Vec2, expected: Vec2) {
    assert!(
        actual.abs_diff_eq(expected, 0.0001),
        "expected {expected:?}, got {actual:?}"
    );
}

#[test]
fn pane_projection_is_invertible_with_world_y_up() {
    let camera = ViewportCamera2D::new(
        PanelSize::new(801.5, 499.25).unwrap(),
        WorldPoint(Vec2::new(-20.0, 14.0)),
        2.5,
    )
    .unwrap();
    let center = camera.panel_size().as_vec2() * 0.5;
    let world = WorldPoint(camera.center().0 + Vec2::new(8.0, 4.0));
    let projected = camera.world_to_panel(world).unwrap();
    assert_near(projected.0, center + Vec2::new(20.0, -10.0));
    assert_near(camera.panel_to_world(projected).unwrap().0, world.0);

    let outside = PanelPoint(Vec2::new(-32.0, 802.0));
    assert!(!camera.contains(outside));
    let world = camera.panel_to_world(outside).unwrap();
    assert_near(camera.world_to_panel(world).unwrap().0, outside.0);
}

#[test]
fn drag_pan_anchors_content_and_resize_preserves_view_center() {
    let mut camera = camera(2.0);
    let press = PanelPoint(Vec2::new(510.0, 430.0));
    let anchored = camera.panel_to_world(press).unwrap();
    let delta = Vec2::new(24.0, -30.0);
    camera.pan(delta).unwrap();
    assert_eq!(camera.center(), WorldPoint(Vec2::new(-12.0, -15.0)));
    assert_eq!(
        camera.world_to_panel(anchored),
        Some(PanelPoint(press.0 + delta))
    );

    let center = camera.center();
    camera.resize(PanelSize::new(385.5, 298.0).unwrap());
    assert_eq!(camera.center(), center);
    assert_eq!(camera.zoom(), 2.0);
    assert_eq!(
        camera.world_to_panel(center),
        Some(PanelPoint(Vec2::new(192.75, 149.0)))
    );
}

#[test]
fn clamped_zoom_keeps_the_world_under_the_cursor() {
    let mut camera = camera(2.0);
    let anchor = PanelPoint(Vec2::new(650.0, 200.0));
    let world = camera.panel_to_world(anchor).unwrap();
    for (factor, expected) in [
        (0.5, 1.0),
        (f32::MAX, ViewportCamera2D::MAX_ZOOM),
        (2.0, ViewportCamera2D::MAX_ZOOM),
        (f32::MIN_POSITIVE, ViewportCamera2D::MIN_ZOOM),
    ] {
        camera.zoom_at(anchor, factor).unwrap();
        assert_eq!(camera.zoom(), expected);
        assert_near(camera.panel_to_world(anchor).unwrap().0, world.0);
        assert_near(camera.world_to_panel(world).unwrap().0, anchor.0);
    }
}

#[test]
fn invalid_camera_input_cannot_poison_its_state() {
    for (width, height) in [(0.0, 10.0), (10.0, -1.0), (f32::INFINITY, 5.0)] {
        assert_eq!(PanelSize::new(width, height), None);
    }
    let mut camera = camera(ViewportCamera2D::MIN_ZOOM);
    let snapshot = camera;
    assert_eq!(
        ViewportCamera2D::new(camera.panel_size(), camera.center(), 0.0),
        None
    );
    assert_eq!(camera.pan(Vec2::splat(f32::NAN)), None);
    assert_eq!(camera.pan(Vec2::splat(f32::MAX)), None);
    assert_eq!(camera.zoom_at(PanelPoint(Vec2::ZERO), f32::INFINITY), None);
    assert_eq!(
        camera.zoom_at(PanelPoint(Vec2::splat(f32::MAX)), 64.0),
        None
    );
    assert_eq!(camera.zoom_at(PanelPoint(Vec2::splat(f32::NAN)), 2.0), None);
    assert_eq!(camera, snapshot);
    assert_eq!(
        camera.world_to_panel(WorldPoint(Vec2::splat(f32::INFINITY))),
        None
    );
    assert_eq!(
        camera.panel_to_world(PanelPoint(Vec2::splat(f32::MAX))),
        None
    );

    camera.resize(PanelSize::new(f32::from_bits(1), 1.0).unwrap());
    assert_eq!(camera.world_to_clip_matrix(), None);
}

#[test]
fn clip_projection_agrees_with_logical_picking_at_rounded_raster_sizes() {
    let logical = Vec2::new(321.25, 241.75);
    let camera = ViewportCamera2D::new(
        PanelSize::new(logical.x, logical.y).unwrap(),
        WorldPoint(Vec2::new(20.0, -10.0)),
        2.25,
    )
    .unwrap();
    let projection = camera.world_to_clip_matrix().unwrap();
    let world = WorldPoint(camera.center().0 + Vec2::new(22.0, 35.0));
    let clip = projection.transform_point2(world.0);
    assert!(clip.y > 0.0);
    let panel = camera.world_to_panel(world).unwrap().0;
    assert!(panel.y < logical.y * 0.5);

    for dpi in [1.0, 1.25, 1.5, 2.0] {
        let raster = (logical * dpi).round();
        let physical = (clip * FLIP_Y + Vec2::ONE) * 0.5 * raster;
        assert_near(physical, panel * raster / logical);
    }
}

#[test]
fn picking_uses_paint_order_and_the_half_open_pane() {
    let camera = camera(1.0);
    let back = PickCandidate2D::new(entity(1), unit_bounds(), Affine2::IDENTITY).unwrap();
    let front =
        PickCandidate2D::new(entity(2), unit_bounds(), Affine2::from_angle(FRAC_PI_4)).unwrap();
    let distant = PickCandidate2D::new(
        entity(3),
        unit_bounds(),
        Affine2::from_translation(Vec2::splat(100.0)),
    )
    .unwrap();
    let center = camera.world_to_panel(WorldPoint(Vec2::ZERO)).unwrap();
    assert_eq!(
        camera.pick(center, &[back, front, distant]),
        Some(front.entity())
    );
    assert_eq!(
        camera.pick(center, &[front, back, distant]),
        Some(back.entity())
    );
    assert_eq!(camera.pick(center, &[distant]), None);

    let pane_bounds = LocalBounds2D::new(Vec2::splat(-500.0), Vec2::splat(500.0)).unwrap();
    let whole_pane = PickCandidate2D::new(entity(4), pane_bounds, Affine2::IDENTITY).unwrap();
    assert_eq!(
        camera.pick(PanelPoint(Vec2::ZERO), &[whole_pane]),
        Some(whole_pane.entity())
    );
    assert_eq!(
        camera.pick(PanelPoint(Vec2::new(800.0, 105.0)), &[whole_pane]),
        None
    );
    assert_eq!(
        camera.pick(PanelPoint(Vec2::new(117.0, 600.0)), &[whole_pane]),
        None
    );
}

#[test]
fn picking_and_selection_outline_share_the_exact_reflected_sheared_quad() {
    let transform = Affine2::from_cols(
        Vec2::new(-2.0, 1.0),
        Vec2::new(0.5, 2.0),
        Vec2::new(7.0, 4.0),
    );
    let candidate =
        PickCandidate2D::from_matrix(entity(1), unit_bounds(), transform.into()).unwrap();
    assert_eq!(
        candidate.world_corners(),
        [
            WorldPoint(Vec2::new(8.5, 1.0)),
            WorldPoint(Vec2::new(4.5, 3.0)),
            WorldPoint(Vec2::new(5.5, 7.0)),
            WorldPoint(Vec2::new(9.5, 5.0)),
        ]
    );
    assert!(candidate.contains(WorldPoint(Vec2::new(6.5, 2.0))));
    let camera = camera(2.0);
    let inside = camera
        .world_to_panel(WorldPoint(transform.translation))
        .unwrap();
    let only_in_aabb = camera
        .world_to_panel(WorldPoint(Vec2::new(9.4, 1.1)))
        .unwrap();
    assert_eq!(camera.pick(inside, &[candidate]), Some(candidate.entity()));
    assert_eq!(camera.pick(only_in_aabb, &[candidate]), None);
}

#[test]
fn picking_rejects_degenerate_or_projective_inputs() {
    assert_eq!(LocalBounds2D::new(Vec2::ZERO, Vec2::X), None);
    assert_eq!(LocalBounds2D::new(Vec2::ONE, Vec2::ZERO), None);
    assert_eq!(LocalBounds2D::new(Vec2::ZERO, Vec2::splat(f32::NAN)), None);
    for invalid in [
        Affine2::from_scale(Vec2::Y),
        Affine2::from_cols(Vec2::ONE, Vec2::splat(2.0), Vec2::ZERO),
        Affine2::from_translation(Vec2::splat(f32::INFINITY)),
        Affine2::from_translation(Vec2::splat(1e20)),
    ] {
        assert_eq!(
            PickCandidate2D::new(entity(1), unit_bounds(), invalid),
            None
        );
    }

    let mut projective = Mat3::IDENTITY;
    projective.x_axis.z = 0.2;
    assert_eq!(
        PickCandidate2D::from_matrix(entity(1), unit_bounds(), projective),
        None
    );
    let mut noncanonical = Mat3::IDENTITY;
    noncanonical.z_axis.z = 2.0;
    assert_eq!(
        PickCandidate2D::from_matrix(entity(1), unit_bounds(), noncanonical),
        None
    );

    let small = Affine2::from_scale(Vec2::splat(1e-25));
    let candidate = PickCandidate2D::new(entity(1), unit_bounds(), small).unwrap();
    assert!(candidate.contains(WorldPoint(Vec2::ZERO)));
    assert!(!candidate.contains(WorldPoint(Vec2::splat(1e-24))));
}

#[test]
fn world_gizmo_geometry_and_hits_keep_logical_dimensions_at_each_zoom() {
    let gizmo = Gizmo2D::new(WorldPoint(Vec2::new(15.0, -24.0)), 0.0).unwrap();
    for zoom in [0.5, 1.0, 8.0] {
        let camera = camera(zoom);
        let center = camera.world_to_panel(gizmo.pivot()).unwrap().0;
        let Some(GizmoGeometry2D::Move(axes)) = gizmo.geometry(&camera, ToolMode::Move) else {
            panic!("expected move geometry");
        };
        let x_end = camera.world_to_panel(axes.x_end).unwrap();
        let y_end = camera.world_to_panel(axes.y_end).unwrap();
        assert_eq!(x_end.0 - center, Vec2::new(48.0, 0.0));
        assert_eq!(y_end.0 - center, Vec2::new(0.0, -48.0));
        assert_eq!(
            gizmo.hit_test(&camera, ToolMode::Move, x_end),
            Some(GizmoHandle::X)
        );
        assert_eq!(
            gizmo.hit_test(&camera, ToolMode::Scale, y_end),
            Some(GizmoHandle::Y)
        );
        let hit = |offset| gizmo.hit_test(&camera, ToolMode::Move, PanelPoint(center + offset));
        assert_eq!(hit(Vec2::new(5.0, -5.0)), Some(GizmoHandle::Free));
        assert_eq!(hit(Vec2::new(5.75, -5.75)), Some(GizmoHandle::X));

        let Some(GizmoGeometry2D::Rotate(ring)) = gizmo.geometry(&camera, ToolMode::Rotate) else {
            panic!("expected ring geometry");
        };
        let top = camera
            .world_to_panel(WorldPoint(ring.pivot.0 + Vec2::Y * ring.radius))
            .unwrap();
        assert_eq!(top.0 - center, Vec2::new(0.0, -44.0));
        assert_eq!(
            gizmo.hit_test(&camera, ToolMode::Rotate, top),
            Some(GizmoHandle::Rotation)
        );
        assert_eq!(
            gizmo.hit_test(&camera, ToolMode::Rotate, PanelPoint(center)),
            None
        );
        assert_eq!(gizmo.geometry(&camera, ToolMode::Select), None);
        assert_eq!(gizmo.geometry(&camera, ToolMode::TileBrush), None);
    }
}

#[test]
fn free_and_axis_moves_are_cumulative_world_deltas_at_the_captured_zoom() {
    let mut camera = camera(2.0);
    let gizmo = Gizmo2D::new(WorldPoint(Vec2::ZERO), FRAC_PI_2).unwrap();
    let center = camera.world_to_panel(gizmo.pivot()).unwrap();
    let start = PanelPoint(center.0 - Vec2::Y * 48.0);
    let mut axis = gizmo.begin_drag(&camera, ToolMode::Move, start).unwrap();
    let mut free = gizmo.begin_drag(&camera, ToolMode::Move, center).unwrap();
    assert_eq!(axis.handle(), GizmoHandle::X);
    assert_eq!(free.gizmo(), gizmo);
    camera.zoom_at(center, 3.0).unwrap();

    let Some(GizmoDelta2D::Translation(delta)) =
        axis.update(PanelPoint(start.0 + Vec2::new(30.0, -18.0)))
    else {
        panic!("expected translation");
    };
    assert_near(delta, Vec2::new(0.0, 9.0));
    assert_eq!(
        axis.update(start),
        Some(GizmoDelta2D::Translation(Vec2::ZERO))
    );
    assert_eq!(
        free.update(PanelPoint(Vec2::new(-20.0, 420.0))),
        Some(GizmoDelta2D::Translation(Vec2::new(-210.0, -60.0)))
    );
}

#[test]
fn rotation_is_counterclockwise_and_continuous_across_the_angle_wrap() {
    let camera = camera(1.0);
    let gizmo = Gizmo2D::new(WorldPoint(Vec2::ZERO), 0.0).unwrap();
    let center = camera.world_to_panel(gizmo.pivot()).unwrap().0;
    let right = PanelPoint(center + Vec2::X * 44.0);
    let up = PanelPoint(center - Vec2::Y * 44.0);
    let left = PanelPoint(center - Vec2::X * 44.0);
    let down = PanelPoint(center + Vec2::Y * 44.0);
    let mut drag = gizmo.begin_drag(&camera, ToolMode::Rotate, right).unwrap();
    for (sample, expected) in [
        (up, FRAC_PI_2),
        (left, PI),
        (down, PI + FRAC_PI_2),
        (right, TAU),
    ] {
        let Some(GizmoDelta2D::Rotation(radians)) = drag.update(sample) else {
            panic!("expected rotation");
        };
        assert!((radians - expected).abs() < 0.00001);
    }
    assert_eq!(drag.update(PanelPoint(center)), None);
    assert_eq!(drag.update(left), Some(GizmoDelta2D::Rotation(TAU)));
    assert_eq!(drag.update(PanelPoint(Vec2::splat(f32::NAN))), None);
    assert_eq!(
        drag.update(up),
        Some(GizmoDelta2D::Rotation(PI + FRAC_PI_2))
    );
}

#[test]
fn scale_uses_multipliers_in_the_gizmo_basis() {
    let camera = camera(4.0);
    let gizmo = Gizmo2D::new(WorldPoint(Vec2::ZERO), FRAC_PI_2).unwrap();
    let center = camera.world_to_panel(gizmo.pivot()).unwrap().0;
    let x_end = PanelPoint(center - Vec2::Y * 48.0);
    let mut axis = gizmo.begin_drag(&camera, ToolMode::Scale, x_end).unwrap();
    let mut uniform = gizmo
        .begin_drag(&camera, ToolMode::Scale, PanelPoint(center))
        .unwrap();

    let Some(GizmoDelta2D::Scale(factors)) =
        axis.update(PanelPoint(x_end.0 + Vec2::new(14.0, -24.0)))
    else {
        panic!("expected axis scale");
    };
    assert_near(factors, Vec2::new(1.5, 1.0));
    assert_eq!(
        axis.update(PanelPoint(center + Vec2::Y * 100.0)),
        Some(GizmoDelta2D::Scale(Vec2::new(0.01, 1.0)))
    );
    assert_eq!(axis.update(x_end), Some(GizmoDelta2D::Scale(Vec2::ONE)));

    let Some(GizmoDelta2D::Scale(factors)) = uniform.update(PanelPoint(
        center + Vec2::new(1.0, -1.0) * (48.0 * FRAC_1_SQRT_2),
    )) else {
        panic!("expected uniform scale");
    };
    assert_near(factors, Vec2::splat(2.0));
}

#[test]
fn snapping_preserves_the_axis_and_uses_world_grid_units() {
    let grid = GridSnap2D::new(2.0).unwrap();
    assert_eq!(GridSnap2D::new(0.0), None);
    assert_eq!(
        grid.snap_point(WorldPoint(Vec2::new(-5.0, 7.0))),
        Some(WorldPoint(Vec2::new(-6.0, 8.0)))
    );

    let camera = camera(2.0);
    let gizmo = Gizmo2D::new(WorldPoint(Vec2::ZERO), FRAC_PI_4).unwrap();
    let Some(GizmoGeometry2D::Move(axes)) = gizmo.geometry(&camera, ToolMode::Move) else {
        panic!("expected move geometry");
    };
    let center = camera.world_to_panel(gizmo.pivot()).unwrap();
    let start = camera.world_to_panel(axes.x_end).unwrap();
    let mut axis = gizmo.begin_drag(&camera, ToolMode::Move, start).unwrap();
    let mut free = gizmo.begin_drag(&camera, ToolMode::Move, center).unwrap();
    let Some(GizmoDelta2D::Translation(delta)) =
        axis.update_snapped(PanelPoint(start.0 + Vec2::new(8.0, -8.0)), grid)
    else {
        panic!("expected snapped translation");
    };
    assert_near(delta, Vec2::splat(6.0 * FRAC_1_SQRT_2));
    assert_eq!(
        free.update_snapped(PanelPoint(center.0 + Vec2::new(6.8, 4.2)), grid),
        Some(GizmoDelta2D::Translation(Vec2::new(4.0, -2.0)))
    );
}

#[test]
fn gizmos_reject_invalid_or_unrepresentable_geometry() {
    assert_eq!(Gizmo2D::new(WorldPoint(Vec2::ZERO), f32::NAN), None);
    assert_eq!(
        Gizmo2D::new(WorldPoint(Vec2::splat(f32::INFINITY)), 0.0),
        None
    );
    let center = WorldPoint(Vec2::splat(1e20));
    let camera = ViewportCamera2D::new(PanelSize::new(100.0, 100.0).unwrap(), center, 1.0).unwrap();
    let gizmo = Gizmo2D::new(center, 0.0).unwrap();
    assert_eq!(gizmo.geometry(&camera, ToolMode::Move), None);
    assert_eq!(gizmo.geometry(&camera, ToolMode::Rotate), None);
    assert!(
        gizmo
            .begin_drag(&camera, ToolMode::Move, PanelPoint(Vec2::splat(50.0)))
            .is_none()
    );
}
