use std::time::Duration;

use nara_core::Vec2;
use nara_physics2d::stateless::{
    CollisionError, StaticCollisionScene, TopDownCharacter,
    rapier::{
        math::{Pose, Vector},
        prelude::{
            Collider, ColliderBuilder, ColliderSet, QueryFilter, Ray, RigidBodyBuilder,
            RigidBodySet, SharedShape,
        },
    },
};

#[test]
fn empty_scene_is_immediately_queryable_and_preserves_desired_displacement() {
    let scene = StaticCollisionScene::new([]).unwrap();
    assert_eq!(scene.collider_count(), 0);
    let queries = scene.queries(QueryFilter::default());
    let ray = Ray::new(Vector::ZERO, Vector::X);
    assert!(queries.cast_ray(&ray, 100.0, true).is_none());
    let character = TopDownCharacter::new(SharedShape::ball(0.25), 0.01).unwrap();
    let desired = Vec2::new(1.5, -0.25);
    for delta in [
        Duration::ZERO,
        Duration::from_millis(16),
        Duration::from_secs(2),
    ] {
        let movement = character
            .move_shape(&scene, Pose::IDENTITY, desired, delta)
            .unwrap();
        assert_eq!(movement.translation, desired);
        assert_eq!(movement.collision_count, 0);
    }
    assert!(!character.overlaps(&scene, Pose::IDENTITY).unwrap());
}

#[test]
fn construction_refreshes_native_ray_queries_and_preserves_native_filters() {
    let scene = StaticCollisionScene::new([
        ColliderBuilder::ball(0.25)
            .translation(Vector::new(1.0, 0.0))
            .sensor(true)
            .user_data(7)
            .build(),
        ColliderBuilder::cuboid(0.5, 1.0)
            .translation(Vector::new(3.0, 0.0))
            .user_data(42)
            .build(),
        ColliderBuilder::ball(0.25)
            .translation(Vector::new(0.5, 0.0))
            .enabled(false)
            .build(),
    ])
    .unwrap();
    assert_eq!(scene.collider_count(), 3);
    let ray = Ray::new(Vector::ZERO, Vector::X);
    let queries = scene.queries(QueryFilter::default());
    let (sensor, distance) = queries.cast_ray(&ray, 10.0, true).unwrap();
    assert_eq!(queries.colliders[sensor].user_data, 7);
    assert!((distance - 0.75).abs() < 1e-5);
    assert!(queries.bodies.is_empty());

    let solid_queries = scene.queries(QueryFilter::default().exclude_sensors());
    let (solid, hit) = solid_queries
        .cast_ray_and_get_normal(&ray, 10.0, true)
        .unwrap();
    assert_eq!(solid_queries.colliders[solid].user_data, 42);
    assert!((hit.time_of_impact - 2.5).abs() < 1e-5);
    assert_eq!(hit.normal, -Vector::X);

    let predicate = |_, collider: &Collider| collider.user_data == 42;
    let filtered = scene.queries(QueryFilter::default().predicate(&predicate));
    assert_eq!(filtered.cast_ray(&ray, 10.0, true).unwrap().0, solid);
}

#[test]
fn attached_and_invalid_native_colliders_are_rejected() {
    let mut bodies = RigidBodySet::new();
    let mut colliders = ColliderSet::new();
    let body = bodies.insert(RigidBodyBuilder::fixed());
    let handle = colliders.insert_with_parent(ColliderBuilder::ball(1.0), body, &mut bodies);
    assert!(matches!(
        StaticCollisionScene::new([colliders[handle].clone()]),
        Err(CollisionError::AttachedCollider)
    ));
    assert!(
        StaticCollisionScene::new([ColliderBuilder::ball(1.0)
            .translation(Vector::new(f32::NAN, 0.0))
            .build()])
        .is_err()
    );
    assert!(StaticCollisionScene::new([ColliderBuilder::ball(f32::INFINITY).build()]).is_err());
}

#[test]
fn borrowed_native_scene_survives_movement_reuse_and_independent_scene_replacement() {
    let scene = StaticCollisionScene::new([ColliderBuilder::cuboid(0.05, 10.0)
        .translation(Vector::new(2.0, 0.0))
        .user_data(123)
        .build()])
    .unwrap();
    let character = TopDownCharacter::new(SharedShape::ball(0.25), 0.01).unwrap();
    let queries = scene.queries(QueryFilter::default());
    let ray = Ray::new(Vector::ZERO, Vector::X);
    let before = queries.cast_ray(&ray, 10.0, true).unwrap();
    let pose_before = *queries.colliders[before.0].position();
    for desired in [Vec2::new(4.0, 1.0), Vec2::new(-3.0, -1.0), Vec2::ZERO] {
        character
            .move_shape(&scene, Pose::IDENTITY, desired, Duration::from_millis(16))
            .unwrap();
    }
    let successor = StaticCollisionScene::new([]).unwrap();
    assert_eq!(successor.collider_count(), 0);
    assert_eq!(queries.cast_ray(&ray, 10.0, true), Some(before));
    assert_eq!(*queries.colliders[before.0].position(), pose_before);
    assert_eq!(queries.colliders[before.0].user_data, 123);
    let after = scene.queries(QueryFilter::default());
    assert!(std::ptr::eq(queries.bvh, after.bvh));
    assert!(std::ptr::eq(queries.colliders, after.colliders));
    assert!(std::ptr::eq(queries.bodies, after.bodies));
    assert_eq!(scene.collider_count(), 1);
}
