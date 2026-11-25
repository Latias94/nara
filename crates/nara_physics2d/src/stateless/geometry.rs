use nara_core::{Mat3, Vec2};
use rapier2d::{
    math::Vector,
    prelude::{Collider, ColliderBuilder},
};

use super::CollisionError;

const ABSOLUTE_CORNER_BUDGET: f64 = 1e-4;
const RELATIVE_CORNER_BUDGET: f64 = 1e-5;

/// Constructs an unattached native collider for a local box under a complete 2D affine map.
///
/// The world pose translates to `transform(center)`. The linear map, including shear and
/// reflection, is baked into the native convex polygon. All four native corners must retain
/// both local offsets and world coordinates within `1e-4 + 1e-5 * abs(expected_coordinate)`.
/// Expected coordinates are computed independently in `f64` from the original inputs.
///
/// # Errors
///
/// Rejects nonfinite inputs, nonpositive half extents, matrices whose last row is not exactly
/// `[0, 0, 1]`, singular maps, and geometry that native `f32` hull construction cannot retain.
/// No application-specific coordinate or size limits are imposed.
pub fn affine_box_collider(
    center: Vec2,
    half_extents: Vec2,
    transform: Mat3,
) -> Result<Collider, CollisionError> {
    if !half_extents.is_finite() || half_extents.min_element() <= 0.0 {
        return Err(CollisionError::InvalidGeometry(
            "Box half extents must be finite and strictly positive",
        ));
    }
    if !center.is_finite() || !transform.is_finite() {
        return Err(CollisionError::InvalidGeometry(
            "Box center and transform must contain only finite values",
        ));
    }
    if transform.x_axis.z != 0.0 || transform.y_axis.z != 0.0 || transform.z_axis.z != 1.0 {
        return Err(CollisionError::InvalidGeometry(
            "Box transform must be an exact 2D affine matrix",
        ));
    }
    let determinant = f64::from(transform.x_axis.x) * f64::from(transform.y_axis.y)
        - f64::from(transform.x_axis.y) * f64::from(transform.y_axis.x);
    if determinant == 0.0 {
        return Err(CollisionError::InvalidGeometry(
            "Box transform must have a nonsingular linear map",
        ));
    }

    let corners = [
        Vec2::new(-half_extents.x, -half_extents.y),
        Vec2::new(half_extents.x, -half_extents.y),
        Vec2::new(half_extents.x, half_extents.y),
        Vec2::new(-half_extents.x, half_extents.y),
    ];
    let world_center = transform.transform_point2(center);
    let expected_offsets = corners.map(|corner| {
        let x = f64::from(corner.x);
        let y = f64::from(corner.y);
        [
            f64::from(transform.x_axis.x) * x + f64::from(transform.y_axis.x) * y,
            f64::from(transform.x_axis.y) * x + f64::from(transform.y_axis.y) * y,
        ]
    });
    let expected_world = corners.map(|corner| {
        let x = f64::from(center.x) + f64::from(corner.x);
        let y = f64::from(center.y) + f64::from(corner.y);
        [
            f64::from(transform.x_axis.x) * x
                + f64::from(transform.y_axis.x) * y
                + f64::from(transform.z_axis.x),
            f64::from(transform.x_axis.y) * x
                + f64::from(transform.y_axis.y) * y
                + f64::from(transform.z_axis.y),
        ]
    });
    let offsets = corners.map(|corner| transform.transform_vector2(corner));
    if !world_center.is_finite()
        || !nondegenerate_quad(offsets)
        || (0..4).any(|index| {
            let edge = offsets[(index + 1) % 4] - offsets[index];
            Vector::new(edge.x, edge.y).try_normalize().is_none()
        })
    {
        return Err(CollisionError::InvalidGeometry(
            "Transformed box corners must be finite and nondegenerate with normalizable edges",
        ));
    }

    let points = offsets.map(|point| Vector::new(point.x, point.y));
    let builder = ColliderBuilder::convex_hull(&points).ok_or(CollisionError::InvalidGeometry(
        "Rapier cannot construct the transformed four-corner convex hull",
    ))?;
    let polygon = builder
        .shape
        .as_convex_polygon()
        .ok_or(CollisionError::InvalidGeometry(
            "The transformed box requires a native convex polygon",
        ))?;
    let native_points: [Vector; 4] = polygon.points().try_into().map_err(|_| {
        CollisionError::InvalidGeometry(
            "The native convex hull did not retain all four box corners",
        )
    })?;
    let native_offsets = native_points.map(|point| Vec2::new(point.x, point.y));
    let actual_world = native_offsets.map(|point| world_center + point);
    if polygon.normals().iter().any(|normal| !normal.is_finite())
        || !nondegenerate_quad(native_offsets)
        || !nondegenerate_quad(actual_world)
        || !corners_match(&expected_offsets, &native_offsets)
        || !corners_match(&expected_world, &actual_world)
    {
        return Err(CollisionError::InvalidGeometry(
            "The native convex hull cannot retain all four corners within the corner budget",
        ));
    }

    Ok(builder
        .translation(Vector::new(world_center.x, world_center.y))
        .restitution(0.0)
        .build())
}

fn nondegenerate_quad(mut vertices: [Vec2; 4]) -> bool {
    if vertices.iter().any(|vertex| !vertex.is_finite()) {
        return false;
    }
    if turn(vertices[0], vertices[1], vertices[2]) < 0.0 {
        vertices.reverse();
    }
    (0..4).all(|index| {
        turn(
            vertices[index],
            vertices[(index + 1) % 4],
            vertices[(index + 2) % 4],
        ) > 0.0
    })
}

fn turn(first: Vec2, second: Vec2, third: Vec2) -> f64 {
    let ax = f64::from(second.x) - f64::from(first.x);
    let ay = f64::from(second.y) - f64::from(first.y);
    let bx = f64::from(third.x) - f64::from(second.x);
    let by = f64::from(third.y) - f64::from(second.y);
    ax * by - ay * bx
}

fn corners_match(expected: &[[f64; 2]; 4], actual: &[Vec2; 4]) -> bool {
    let mut matched = [false; 4];
    expected.iter().all(|expected| {
        let Some(index) = actual.iter().enumerate().position(|(index, actual)| {
            !matched[index]
                && coordinate_matches(expected[0], actual.x)
                && coordinate_matches(expected[1], actual.y)
        }) else {
            return false;
        };
        matched[index] = true;
        true
    })
}

fn coordinate_matches(expected: f64, actual: f32) -> bool {
    (f64::from(actual) - expected).abs()
        <= ABSOLUTE_CORNER_BUDGET + RELATIVE_CORNER_BUDGET * expected.abs()
}
