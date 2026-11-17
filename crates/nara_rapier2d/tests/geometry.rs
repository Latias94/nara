use nara_core::{Mat3, Vec2, Vec3};
use nara_rapier2d::{
    affine_box_collider,
    rapier::{math::Pose, prelude::Collider},
};

fn assert_corners(collider: &Collider, center: Vec2, half_extents: Vec2, transform: Mat3) {
    assert!(collider.parent().is_none());
    let world_center = transform.transform_point2(center);
    assert_eq!(
        *collider.position(),
        Pose::translation(world_center.x, world_center.y)
    );
    let polygon = collider.shape().as_convex_polygon().unwrap();
    assert_eq!(polygon.points().len(), 4);
    assert_eq!(polygon.normals().len(), 4);
    for normal in polygon.normals() {
        assert!(normal.is_finite());
        assert!((normal.length() - 1.0).abs() < 1e-5);
    }
    let actual: Vec<_> = polygon
        .points()
        .iter()
        .map(|point| collider.position().transform_point(*point))
        .collect();
    let mut matched = [false; 4];
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let local_x = f64::from(center.x) + sx * f64::from(half_extents.x);
        let local_y = f64::from(center.y) + sy * f64::from(half_extents.y);
        let expected = [
            f64::from(transform.x_axis.x) * local_x
                + f64::from(transform.y_axis.x) * local_y
                + f64::from(transform.z_axis.x),
            f64::from(transform.x_axis.y) * local_x
                + f64::from(transform.y_axis.y) * local_y
                + f64::from(transform.z_axis.y),
        ];
        let index = actual
            .iter()
            .enumerate()
            .position(|(index, point)| {
                !matched[index]
                    && (f64::from(point.x) - expected[0]).abs() <= 1e-4 + 1e-5 * expected[0].abs()
                    && (f64::from(point.y) - expected[1]).abs() <= 1e-4 + 1e-5 * expected[1].abs()
            })
            .unwrap_or_else(|| panic!("Native corners {actual:?} omit {expected:?}"));
        matched[index] = true;
    }
}

#[test]
fn full_affine_boxes_retain_four_corners_through_shear_and_all_mirrors() {
    let center = Vec2::new(0.35, -0.4);
    let half_extents = Vec2::new(1.1, 0.45);
    for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        let transform =
            Mat3::from_scale_angle_translation(
                Vec2::new(sx * 1.75, sy * 0.65),
                0.38,
                Vec2::new(8.0, -3.0),
            ) * Mat3::from_scale_angle_translation(
                Vec2::new(1.3, 0.8),
                -0.62,
                Vec2::new(-1.25, 2.0),
            ) * Mat3::from_scale_angle_translation(Vec2::new(1.1, 0.9), 0.21, Vec2::new(0.4, -0.7));
        assert!(
            transform
                .x_axis
                .truncate()
                .dot(transform.y_axis.truncate())
                .abs()
                > 0.05
        );
        let collider = affine_box_collider(center, half_extents, transform).unwrap();
        assert_corners(&collider, center, half_extents, transform);
    }
}

#[test]
fn boxes_do_not_inherit_fixture_size_or_world_coordinate_limits() {
    let center = Vec2::new(200.0, -150.0);
    let half_extents = Vec2::new(128.0, 96.0);
    let transform = Mat3::from_cols(
        Vec3::new(-1.25, 0.3, 0.0),
        Vec3::new(0.5, 1.5, 0.0),
        Vec3::new(16_384.0, -8192.0, 1.0),
    );
    let collider = affine_box_collider(center, half_extents, transform).unwrap();
    assert_corners(&collider, center, half_extents, transform);
}

#[test]
fn invalid_extent_center_and_transform_inputs_are_rejected() {
    for half_extents in [
        Vec2::ZERO,
        Vec2::new(-1.0, 1.0),
        Vec2::new(1.0, 0.0),
        Vec2::new(f32::NAN, 1.0),
        Vec2::new(1.0, f32::INFINITY),
    ] {
        assert!(affine_box_collider(Vec2::ZERO, half_extents, Mat3::IDENTITY).is_err());
    }
    for center in [Vec2::new(f32::NAN, 0.0), Vec2::new(0.0, f32::INFINITY)] {
        assert!(affine_box_collider(center, Vec2::ONE, Mat3::IDENTITY).is_err());
    }
    let mut nonfinite = Mat3::IDENTITY;
    nonfinite.z_axis.x = f32::INFINITY;
    assert!(affine_box_collider(Vec2::ZERO, Vec2::ONE, nonfinite).is_err());

    let mut projective_x = Mat3::IDENTITY;
    projective_x.x_axis.z = f32::EPSILON;
    let mut projective_y = Mat3::IDENTITY;
    projective_y.y_axis.z = -f32::EPSILON;
    let mut homogeneous_scale = Mat3::IDENTITY;
    homogeneous_scale.z_axis.z += f32::EPSILON;
    for transform in [projective_x, projective_y, homogeneous_scale] {
        assert!(affine_box_collider(Vec2::ZERO, Vec2::ONE, transform).is_err());
    }
}

#[test]
fn singular_collapsed_and_native_corner_losing_boxes_are_rejected() {
    for transform in [
        Mat3::from_scale(Vec2::new(0.0, 1.0)),
        Mat3::from_cols(Vec3::new(1.0, 2.0, 0.0), Vec3::new(2.0, 4.0, 0.0), Vec3::Z),
        Mat3::from_cols(Vec3::X, Vec3::new(1.0, 1e-8, 0.0), Vec3::Z),
        Mat3::from_translation(Vec2::new(1e12, 0.0)),
    ] {
        assert!(affine_box_collider(Vec2::ZERO, Vec2::ONE, transform).is_err());
    }
    assert!(affine_box_collider(Vec2::ZERO, Vec2::splat(f32::MAX), Mat3::IDENTITY).is_err());
    assert!(
        affine_box_collider(Vec2::ZERO, Vec2::splat(f32::MIN_POSITIVE), Mat3::IDENTITY).is_err()
    );
}

#[test]
fn affine_cancellation_cannot_bypass_world_corner_budget() {
    let center = Vec2::new(1_000_000.0, 0.0);
    let half_extents = Vec2::splat(1.25);
    let transform = Mat3::from_cols(
        Vec3::new(1.1, 0.0, 0.0),
        Vec3::Y,
        Vec3::new(-1_100_000.0, 0.0, 1.0),
    );
    assert_eq!(transform.transform_point2(center), Vec2::ZERO);
    assert!(affine_box_collider(center, half_extents, transform).is_err());
}
