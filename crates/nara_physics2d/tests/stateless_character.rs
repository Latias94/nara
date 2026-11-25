use std::{f32::consts::FRAC_PI_4, time::Duration};

use nara_core::Vec2;
use nara_physics2d::stateless::{
    CollisionError, StaticCollisionScene, TopDownCharacter,
    rapier::{
        math::{Pose, Vector},
        prelude::{Collider, ColliderBuilder, SharedShape},
    },
};

const DELTA: Duration = Duration::from_millis(16);

fn character() -> TopDownCharacter {
    TopDownCharacter::new(SharedShape::ball(0.25), 0.01).unwrap()
}

fn wall(normal: Vec2, half_thickness: f32) -> Collider {
    ColliderBuilder::cuboid(half_thickness, 10.0)
        .rotation(normal.y.atan2(normal.x))
        .translation(Vector::new(normal.x * 2.0, normal.y * 2.0))
        .build()
}

fn displaced(pose: Pose, translation: Vec2) -> Pose {
    pose.append_translation(Vector::new(translation.x, translation.y))
}

#[test]
fn native_sweeps_stop_and_slide_against_walls_in_all_four_directions() {
    let character = character();
    for normal in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
        let scene = StaticCollisionScene::new([wall(normal, 0.05)]).unwrap();
        for tangent_distance in [0.0, -1.5, 1.5] {
            let tangent = Vec2::new(-normal.y, normal.x);
            let desired = normal * 4.0 + tangent * tangent_distance;
            let movement = character
                .move_shape(&scene, Pose::IDENTITY, desired, DELTA)
                .unwrap();
            assert!(movement.collision_count > 0, "{normal:?}, {desired:?}");
            assert!(
                (movement.translation.dot(normal) - 1.69).abs() < 0.005,
                "{movement:?}"
            );
            assert!(
                (movement.translation.dot(tangent) - tangent_distance).abs() < 0.005,
                "{normal:?}, {desired:?}, {movement:?}"
            );
            assert!(
                !character
                    .overlaps(&scene, displaced(Pose::IDENTITY, movement.translation))
                    .unwrap()
            );
        }
    }
}

#[test]
fn native_slide_treats_diagonal_surfaces_symmetrically() {
    let character = character();
    for angle in [FRAC_PI_4, -FRAC_PI_4, 3.0 * FRAC_PI_4, -3.0 * FRAC_PI_4] {
        let normal = Vec2::new(angle.cos(), angle.sin());
        let tangent = Vec2::new(-normal.y, normal.x);
        let scene = StaticCollisionScene::new([wall(normal, 0.05)]).unwrap();
        for sign in [-1.0, 1.0] {
            let desired = normal * 4.0 + tangent * (sign * 2.0);
            let movement = character
                .move_shape(&scene, Pose::IDENTITY, desired, DELTA)
                .unwrap();
            assert!(movement.collision_count > 0);
            assert!(
                (movement.translation.dot(normal) - 1.69).abs() < 0.005,
                "{movement:?}"
            );
            assert!(
                (movement.translation.dot(tangent) - sign * 2.0).abs() < 0.005,
                "{angle}, {sign}, {movement:?}"
            );
            assert!(
                !character
                    .overlaps(&scene, displaced(Pose::IDENTITY, movement.translation))
                    .unwrap()
            );
        }
    }
}

#[test]
fn native_corner_movement_stops_at_both_walls() {
    let scene = StaticCollisionScene::new([wall(Vec2::X, 0.05), wall(Vec2::Y, 0.05)]).unwrap();
    let character = character();
    let movement = character
        .move_shape(&scene, Pose::IDENTITY, Vec2::splat(4.0), DELTA)
        .unwrap();
    assert!(movement.collision_count >= 2, "{movement:?}");
    assert!(
        (movement.translation - Vec2::splat(1.69))
            .abs()
            .max_element()
            < 0.005,
        "{movement:?}"
    );
    assert!(
        !character
            .overlaps(&scene, displaced(Pose::IDENTITY, movement.translation))
            .unwrap()
    );
}

#[test]
fn fast_native_sweeps_cannot_cross_a_thin_wall_in_either_axis() {
    let character = character();
    for normal in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y] {
        let scene = StaticCollisionScene::new([wall(normal, 0.005)]).unwrap();
        let movement = character
            .move_shape(&scene, Pose::IDENTITY, normal * 1000.0, DELTA)
            .unwrap();
        assert!(movement.collision_count > 0);
        assert!(
            (movement.translation.dot(normal) - 1.735).abs() < 0.005,
            "{normal:?}, {movement:?}"
        );
        assert!(
            !character
                .overlaps(&scene, displaced(Pose::IDENTITY, movement.translation))
                .unwrap()
        );
    }
}

#[test]
fn opposing_movements_and_rotated_native_shapes_retain_no_controller_history() {
    let scene = StaticCollisionScene::new([wall(Vec2::X, 0.05), wall(-Vec2::X, 0.05)]).unwrap();
    let character = TopDownCharacter::new(SharedShape::cuboid(0.2, 0.35), 0.01).unwrap();
    let cloned = character.clone();
    let pose = Pose::rotation(FRAC_PI_4);
    let desired = Vec2::new(4.0, 1.0);
    let forward = character.move_shape(&scene, pose, desired, DELTA).unwrap();
    let reverse = character.move_shape(&scene, pose, -desired, DELTA).unwrap();
    assert!(forward.collision_count > 0);
    assert!(reverse.collision_count > 0);
    assert!((forward.translation + reverse.translation).length() < 0.005);
    assert_eq!(
        character.move_shape(&scene, pose, desired, DELTA).unwrap(),
        forward
    );
    assert_eq!(
        cloned.move_shape(&scene, pose, -desired, DELTA).unwrap(),
        reverse
    );
    assert!(
        !character
            .overlaps(&scene, displaced(pose, forward.translation))
            .unwrap()
    );
    assert!(std::ptr::eq(character.shape(), cloned.shape()));
}

#[test]
fn overlap_admission_uses_native_shape_intersection_and_ignores_sensors() {
    let scene = StaticCollisionScene::new([
        ColliderBuilder::ball(1.0).sensor(true).build(),
        ColliderBuilder::cuboid(0.5, 0.5)
            .translation(Vector::new(3.0, 0.0))
            .build(),
    ])
    .unwrap();
    let character = character();
    assert!(!character.overlaps(&scene, Pose::IDENTITY).unwrap());
    assert!(
        character
            .overlaps(&scene, Pose::translation(3.0, 0.0))
            .unwrap()
    );
    assert!(
        !character
            .overlaps(&scene, Pose::translation(2.245, 0.0))
            .unwrap()
    );

    let movement = character
        .move_shape(&scene, Pose::IDENTITY, Vec2::new(4.0, 0.0), DELTA)
        .unwrap();
    assert!(movement.collision_count > 0);
    assert!((movement.translation.x - 2.24).abs() < 0.005);
    assert!(
        !character
            .overlaps(&scene, displaced(Pose::IDENTITY, movement.translation))
            .unwrap()
    );
    assert_eq!(scene.collider_count(), 2);
}

#[test]
fn invalid_shape_margin_pose_and_displacement_are_rejected() {
    for margin in [0.0, -0.01, f32::NAN, f32::INFINITY] {
        assert!(matches!(
            TopDownCharacter::new(SharedShape::ball(0.25), margin),
            Err(CollisionError::InvalidMargin)
        ));
    }
    for shape in [
        SharedShape::ball(0.0),
        SharedShape::ball(f32::INFINITY),
        SharedShape::cuboid(0.0, 1.0),
    ] {
        assert!(TopDownCharacter::new(shape, 0.01).is_err());
    }
    let scene = StaticCollisionScene::new([]).unwrap();
    let character = character();
    for pose in [
        Pose::translation(f32::NAN, 0.0),
        Pose::rotation(f32::INFINITY),
    ] {
        assert_eq!(
            character.overlaps(&scene, pose),
            Err(CollisionError::InvalidPose)
        );
        assert_eq!(
            character.move_shape(&scene, pose, Vec2::ZERO, DELTA),
            Err(CollisionError::InvalidPose)
        );
    }
    for desired in [Vec2::new(f32::NAN, 0.0), Vec2::new(0.0, f32::INFINITY)] {
        assert_eq!(
            character.move_shape(&scene, Pose::IDENTITY, desired, DELTA),
            Err(CollisionError::InvalidMovement)
        );
    }
}
