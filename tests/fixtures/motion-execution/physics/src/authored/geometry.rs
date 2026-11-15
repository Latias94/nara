use nara_core::Vec2;
use nara_transform::Transform2d;
use rapier2d::{
    math::{Pose, Vector},
    prelude::{Collider, ColliderBuilder, RigidBody, RigidBodyBuilder},
};

use super::{CollisionDefinition, CollisionSpatialSource, MAX_CENTER_COORDINATE};

const ABSOLUTE_CORNER_BUDGET: f32 = 1e-4;
const RELATIVE_CORNER_BUDGET: f32 = 1e-5;

pub(super) struct PreparedPair {
    pub body: RigidBodyBuilder,
    pub collider: ColliderBuilder,
}

pub(super) struct GeometryError {
    pub code: &'static str,
    pub summary: &'static str,
    pub field: &'static str,
}

fn invalid(summary: &'static str, field: &'static str) -> GeometryError {
    GeometryError {
        code: "fixture.motion.collision.invalid-geometry",
        summary,
        field,
    }
}

pub(super) fn prepare(
    definition: CollisionDefinition,
    source: CollisionSpatialSource,
) -> Result<PreparedPair, GeometryError> {
    if !source.local.translation.is_finite()
        || !source.local.rotation.is_finite()
        || !source.local.scale.is_finite()
        || !source.global.is_finite()
    {
        return Err(invalid(
            "Collision input transforms must contain only finite affine values",
            "transform",
        ));
    }
    if definition.dynamic && (source.parent.is_some() || source.local.scale != Vec2::ONE) {
        return Err(GeometryError {
            code: "fixture.motion.collision.unsupported-dynamic-transform",
            summary: "Dynamic collisions require an actual root Transform with unit local scale",
            field: if source.parent.is_some() {
                "parent"
            } else {
                "scale"
            },
        });
    }
    let corners = half_corners(definition.half_width, definition.half_height);
    let expected_world =
        corners.map(|corner| source.global.transform_point2(definition.center + corner));
    if expected_world.iter().any(|point| !bounded(*point))
        || canonical_vertices(expected_world).is_none()
    {
        return Err(invalid(
            "Transformed box corners must be finite, nondegenerate and within the spatial bounds",
            "transform",
        ));
    }
    if definition.dynamic {
        let pose = Pose::new(
            Vector::new(source.local.translation.x, source.local.translation.y),
            source.local.rotation,
        );
        let collider_pose = pose * Pose::translation(definition.center.x, definition.center.y);
        let actual_world = corners.map(|corner| {
            let point = collider_pose.transform_point(Vector::new(corner.x, corner.y));
            Vec2::new(point.x, point.y)
        });
        if actual_world.iter().any(|point| !bounded(*point))
            || canonical_vertices(actual_world).is_none()
            || !corners_match(&expected_world, &actual_world)
        {
            return Err(invalid(
                "The native rigid pose cannot retain the transformed box within its corner budget",
                "transform",
            ));
        }
        return Ok(PreparedPair {
            body: RigidBodyBuilder::dynamic().pose(pose).lock_rotations(),
            collider: ColliderBuilder::cuboid(definition.half_width, definition.half_height)
                .translation(Vector::new(definition.center.x, definition.center.y))
                .restitution(0.0),
        });
    }
    let center = source.global.transform_point2(definition.center);
    let offsets = corners.map(|corner| source.global.transform_vector2(corner));
    let determinant = f64::from(source.global.x_axis.x) * f64::from(source.global.y_axis.y)
        - f64::from(source.global.x_axis.y) * f64::from(source.global.y_axis.x);
    if determinant == 0.0
        || !bounded(center)
        || offsets.iter().any(|point| !bounded(*point))
        || canonical_vertices(offsets).is_none()
        || (0..4).any(|index| {
            (offsets[(index + 1) % 4] - offsets[index])
                .try_normalize()
                .is_none()
        })
    {
        return Err(invalid(
            "Fixed box corners require bounded values and natively normalizable nondegenerate edges",
            "transform",
        ));
    }
    let points = offsets.map(|point| Vector::new(point.x, point.y));
    let collider = ColliderBuilder::convex_hull(&points).ok_or_else(|| {
        invalid(
            "Rapier cannot construct the transformed four-corner convex hull",
            "transform",
        )
    })?;
    let polygon = collider.shape.as_convex_polygon().ok_or_else(|| {
        invalid(
            "The transformed fixed box requires a native convex polygon",
            "transform",
        )
    })?;
    let native_points: [Vector; 4] = polygon.points().try_into().map_err(|_| {
        invalid(
            "The native convex hull did not retain all four box corners",
            "transform",
        )
    })?;
    let native_offsets = native_points.map(|point| Vec2::new(point.x, point.y));
    let actual_world = native_offsets.map(|point| center + point);
    if polygon.normals().iter().any(|normal| !normal.is_finite())
        || actual_world.iter().any(|point| !bounded(*point))
        || canonical_vertices(native_offsets).is_none()
        || canonical_vertices(actual_world).is_none()
        || !corners_match(&offsets, &native_offsets)
        || !corners_match(&expected_world, &actual_world)
    {
        return Err(invalid(
            "The native convex hull cannot retain all four corners within the declared budget",
            "transform",
        ));
    }
    Ok(PreparedPair {
        body: RigidBodyBuilder::fixed().pose(Pose::translation(center.x, center.y)),
        collider: collider.restitution(0.0),
    })
}

pub(super) fn body_transform(body: &RigidBody) -> Option<Transform2d> {
    let pose = body.position();
    let rotation = pose.rotation.angle();
    if !pose.translation.is_finite() || !pose.rotation.is_finite() || !rotation.is_finite() {
        return None;
    }
    Some(Transform2d {
        translation: Vec2::new(pose.translation.x, pose.translation.y),
        rotation,
        scale: Vec2::ONE,
    })
}

pub(super) fn native_vertices(collider: &Collider) -> Option<[Vec2; 4]> {
    let points = if let Some(polygon) = collider.shape().as_convex_polygon() {
        let points: [Vector; 4] = polygon.points().try_into().ok()?;
        points.map(|point| Vec2::new(point.x, point.y))
    } else {
        let extent = collider.shape().as_cuboid()?.half_extents;
        half_corners(extent.x, extent.y)
    };
    let pose = collider.position();
    if !pose.translation.is_finite() || !pose.rotation.is_finite() {
        return None;
    }
    canonical_vertices(points.map(|point| {
        let world = pose.transform_point(Vector::new(point.x, point.y));
        Vec2::new(world.x, world.y)
    }))
}

fn bounded(point: Vec2) -> bool {
    point.is_finite() && point.abs().max_element() <= MAX_CENTER_COORDINATE
}

fn half_corners(half_width: f32, half_height: f32) -> [Vec2; 4] {
    [
        Vec2::new(-half_width, -half_height),
        Vec2::new(half_width, -half_height),
        Vec2::new(half_width, half_height),
        Vec2::new(-half_width, half_height),
    ]
}

fn corners_match(expected: &[Vec2; 4], actual: &[Vec2; 4]) -> bool {
    let mut matched = [false; 4];
    expected.iter().all(|expected| {
        let Some(index) = actual.iter().enumerate().position(|(index, actual)| {
            !matched[index]
                && coordinate_matches(expected.x, actual.x)
                && coordinate_matches(expected.y, actual.y)
        }) else {
            return false;
        };
        matched[index] = true;
        true
    })
}

fn coordinate_matches(expected: f32, actual: f32) -> bool {
    (actual - expected).abs() <= ABSOLUTE_CORNER_BUDGET + RELATIVE_CORNER_BUDGET * expected.abs()
}

fn canonical_vertices(mut vertices: [Vec2; 4]) -> Option<[Vec2; 4]> {
    if vertices.iter().any(|vertex| !vertex.is_finite()) {
        return None;
    }
    if turn(vertices[0], vertices[1], vertices[2]) < 0.0 {
        vertices.reverse();
    }
    if (0..4).any(|index| {
        turn(
            vertices[index],
            vertices[(index + 1) % 4],
            vertices[(index + 2) % 4],
        ) <= 0.0
    }) {
        return None;
    }
    let first = (0..4).min_by(|first, second| {
        vertices[*first]
            .x
            .total_cmp(&vertices[*second].x)
            .then_with(|| vertices[*first].y.total_cmp(&vertices[*second].y))
    })?;
    vertices.rotate_left(first);
    Some(vertices)
}

fn turn(first: Vec2, second: Vec2, third: Vec2) -> f64 {
    let ax = f64::from(second.x) - f64::from(first.x);
    let ay = f64::from(second.y) - f64::from(first.y);
    let bx = f64::from(third.x) - f64::from(second.x);
    let by = f64::from(third.y) - f64::from(second.y);
    ax * by - ay * bx
}
