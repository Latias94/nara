use std::{f32::consts::FRAC_PI_2, time::Duration};

use bevy_ecs::{
    message::MessageCursor,
    prelude::{
        Commands, Entity, IntoScheduleConfigs, MessageReader, Messages, Query, Res, ResMut,
        Resource,
    },
};
use nara_app::{App, CoreStage, FixedTime};
use nara_core::Vec2;
use nara_physics2d::{
    Collider2d, CollisionEvent, CollisionGroups2d, Damping2d, ExternalForce2d, ExternalImpulse2d,
    GravityScale2d, LockedAxes2d, Physics2dPlugin, Physics2dSet, PhysicsError, PhysicsMaterial2d,
    PhysicsPose2d, PhysicsQueryFilter2d, PhysicsWorld2d, RigidBody2d, Sensor2d, Teleport2d,
    TopDownCharacter, Velocity2d,
};
use nara_reflect::{ComponentValue, Schema};
use nara_scene::spatial::{ChildOf, GlobalTransform2d, Transform2d};

const CIRCLE: Collider2d = Collider2d::Circle { radius: 0.25 };
const UNIT_BOX: Collider2d = Collider2d::Box {
    half_extents: Vec2::splat(0.5),
};
const STEP: Duration = FixedTime::DEFAULT_TIMESTEP;

#[derive(Resource, Default)]
struct CollisionLog(Vec<CollisionEvent>);

fn collect_collisions(mut events: MessageReader<CollisionEvent>, mut log: ResMut<CollisionLog>) {
    log.0.extend(events.read().copied());
}

fn physics_app() -> App {
    let mut app = App::new();
    app.add_plugin(Physics2dPlugin).unwrap();
    app.init_resource::<CollisionLog>().unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        collect_collisions.after(Physics2dSet::Step),
    )
    .unwrap();
    app
}

fn pair(first: Entity, second: Entity) -> (Entity, Entity) {
    (first.min(second), first.max(second))
}

#[test]
fn dynamics_use_the_fixed_delta_and_resolve_contacts_before_writing_back() {
    let mut app = physics_app();
    let floor = app
        .world_mut()
        .unwrap()
        .spawn((
            Collider2d::Box {
                half_extents: Vec2::new(10.0, 0.5),
            },
            Transform2d::from_xy(0.0, -0.5),
        ))
        .id();
    let ball = app
        .world_mut()
        .unwrap()
        .spawn((
            RigidBody2d::Dynamic,
            Collider2d::Circle { radius: 0.5 },
            Transform2d {
                scale: Vec2::new(2.0, 3.0),
                z: 17.0,
                ..Transform2d::from_xy(0.0, 3.0)
            },
        ))
        .id();

    app.step_fixed(Duration::from_millis(100)).unwrap();
    let initial_fall = app.world().get::<Transform2d>(ball).unwrap().translation.y;
    assert!((2.92..2.96).contains(&initial_fall), "{initial_fall}");
    {
        let mut transform = app
            .world_mut()
            .unwrap()
            .get_mut::<Transform2d>(ball)
            .unwrap();
        transform.translation = Vec2::new(100.0, 100.0);
        transform.rotation = 1.0;
    }
    for _ in 0..120 {
        app.step_fixed(STEP).unwrap();
    }
    let transform = app.world().get::<Transform2d>(ball).unwrap();
    assert!(transform.translation.x.abs() < 0.001);
    assert!(
        (transform.translation.y - 0.5).abs() < 0.02,
        "{transform:?}"
    );
    assert!(transform.rotation.abs() < 0.001);
    assert_eq!(transform.scale, Vec2::new(2.0, 3.0));
    assert_eq!(transform.z, 17.0);
    assert_eq!(
        app.world()
            .get::<GlobalTransform2d>(ball)
            .unwrap()
            .translation(),
        transform.translation
    );
    let (first, second) = pair(floor, ball);
    assert_eq!(
        app.world().resource::<CollisionLog>().0,
        [CollisionEvent::Started(first, second)]
    );

    app.world_mut()
        .unwrap()
        .entity_mut(ball)
        .insert((Transform2d::from_xy(0.0, 3.0), Teleport2d));
    app.step_fixed(STEP).unwrap();
    assert!(app.world().get::<Transform2d>(ball).unwrap().translation.y > 2.9);
    assert!(app.world().get::<Teleport2d>(ball).is_none());
    assert_eq!(
        app.world().resource::<CollisionLog>().0,
        [
            CollisionEvent::Started(first, second),
            CollisionEvent::Stopped(first, second)
        ],
    );
}

#[test]
fn kinematic_targets_follow_the_propagated_parent_and_receive_sensor_events() {
    let mut app = physics_app();
    let parent = app
        .world_mut()
        .unwrap()
        .spawn(Transform2d::from_xy(4.0, 0.0))
        .id();
    let actor = app
        .world_mut()
        .unwrap()
        .spawn((
            RigidBody2d::Kinematic,
            CIRCLE,
            Transform2d::from_xy(1.0, 0.0),
            ChildOf(parent),
        ))
        .id();
    let sensor = app
        .world_mut()
        .unwrap()
        .spawn((CIRCLE, Sensor2d, Transform2d::from_xy(4.0, 1.0)))
        .id();
    app.add_systems(
        CoreStage::FixedUpdate,
        (move |time: Res<FixedTime>, mut transforms: Query<&mut Transform2d>| {
            if time.tick() == 2 {
                transforms.get_mut(parent).unwrap().rotation = FRAC_PI_2;
            }
        })
        .before(Physics2dSet::Step),
    )
    .unwrap();
    app.step_fixed(STEP).unwrap();
    assert!(app.world().resource::<CollisionLog>().0.is_empty());

    app.step_fixed(STEP).unwrap();
    let world = app.world();
    let physics = world.resource::<PhysicsWorld2d>();
    let geometry = physics
        .colliders()
        .find(|collider| collider.entity == actor)
        .unwrap();
    assert!((geometry.pose.translation - Vec2::new(4.0, 1.0)).length() < 0.001);
    assert!((geometry.pose.rotation - FRAC_PI_2).abs() < 0.001);
    assert_eq!(
        world.get::<Transform2d>(actor).unwrap().translation,
        Vec2::X
    );
    assert_eq!(
        physics
            .overlap_circle(Vec2::new(4.0, 1.0), 0.1, PhysicsQueryFilter2d::default())
            .unwrap(),
        [actor],
    );

    app.step_fixed(STEP).unwrap();
    let (first, second) = pair(actor, sensor);
    assert_eq!(
        app.world().resource::<CollisionLog>().0,
        [CollisionEvent::Started(first, second)]
    );
    assert!(app.world_mut().unwrap().despawn(sensor));
    let replacement = app
        .world_mut()
        .unwrap()
        .spawn((CIRCLE, Sensor2d, Transform2d::from_xy(4.0, 1.0)))
        .id();
    app.step_fixed(STEP).unwrap();
    let events = &app.world().resource::<CollisionLog>().0;
    let (replaced_first, replaced_second) = pair(actor, replacement);
    assert_eq!(events.len(), 3);
    assert!(events[1..].contains(&CollisionEvent::Stopped(first, second)));
    assert!(events[1..].contains(&CollisionEvent::Started(replaced_first, replaced_second)));
}

#[test]
fn collision_messages_survive_every_fixed_tick_of_a_catch_up_frame() {
    let mut app = physics_app();
    let timestep = Duration::from_millis(10);
    app.insert_resource(
        FixedTime::new(timestep)
            .unwrap()
            .with_max_steps_per_frame(6)
            .unwrap(),
    )
    .unwrap();
    let sensor = app.world_mut().unwrap().spawn((CIRCLE, Sensor2d)).id();
    let actor = app
        .world_mut()
        .unwrap()
        .spawn((RigidBody2d::Kinematic, CIRCLE))
        .id();
    app.add_systems(
        CoreStage::FixedUpdate,
        (move |time: Res<FixedTime>, mut commands: Commands| {
            let at_sensor = time.tick() % 2 == 1;
            let position = if at_sensor {
                Vec2::ZERO
            } else {
                Vec2::X * 10.0
            };
            commands
                .entity(actor)
                .insert((Transform2d::from_translation(position), Teleport2d));
        })
        .before(Physics2dSet::Step),
    )
    .unwrap();

    app.update_with_delta(timestep * 4).unwrap();
    let (first, second) = pair(actor, sensor);
    let expected = [
        CollisionEvent::Started(first, second),
        CollisionEvent::Stopped(first, second),
        CollisionEvent::Started(first, second),
        CollisionEvent::Stopped(first, second),
    ];
    let messages = app.world().resource::<Messages<CollisionEvent>>();
    assert_eq!(
        MessageCursor::default()
            .read(messages)
            .copied()
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(app.world().resource::<CollisionLog>().0, expected);
}

#[test]
fn native_queries_use_the_committed_world_and_apply_entity_and_sensor_filters() {
    let mut app = physics_app();
    let wall = app
        .world_mut()
        .unwrap()
        .spawn((
            Collider2d::Box {
                half_extents: Vec2::new(0.05, 4.0),
            },
            Transform2d::from_xy(2.0, 0.0),
        ))
        .id();
    let sensor = app
        .world_mut()
        .unwrap()
        .spawn((CIRCLE, Sensor2d, Transform2d::from_xy(1.0, 0.0)))
        .id();
    let actor = app
        .world_mut()
        .unwrap()
        .spawn((CIRCLE, RigidBody2d::Kinematic))
        .id();
    let solids = PhysicsQueryFilter2d {
        exclude: Some(actor),
        ..Default::default()
    };
    assert!(
        app.world()
            .resource::<PhysicsWorld2d>()
            .cast_ray(Vec2::ZERO, Vec2::X, 4.0, solids)
            .unwrap()
            .is_none()
    );

    app.step_fixed(STEP).unwrap();
    let physics = app.world().resource::<PhysicsWorld2d>();
    let hit = physics
        .cast_ray(Vec2::ZERO, Vec2::X * 3.0, 4.0, solids)
        .unwrap()
        .unwrap();
    assert_eq!(hit.entity, wall);
    assert!((hit.distance - 1.95).abs() < 0.0001);
    assert!((hit.point - Vec2::new(1.95, 0.0)).length() < 0.0001);
    assert_eq!(hit.normal, Vec2::NEG_X);
    let with_sensors = PhysicsQueryFilter2d {
        include_sensors: true,
        ..solids
    };
    assert_eq!(
        physics.overlap_circle(Vec2::X, 0.1, with_sensors).unwrap(),
        [sensor]
    );
    assert!(
        physics
            .overlap_circle(Vec2::X, 0.1, solids)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        physics
            .cast_ray(Vec2::ZERO, Vec2::X, 4.0, with_sensors)
            .unwrap()
            .unwrap()
            .entity,
        sensor,
    );

    let hit = physics
        .cast_shape(CIRCLE, PhysicsPose2d::default(), Vec2::X * 4.0, solids)
        .unwrap()
        .unwrap();
    assert_eq!(hit.entity, wall);
    assert!((hit.fraction - 0.425).abs() < 0.001);
    let character = TopDownCharacter::new(CIRCLE, 0.01).unwrap();
    let start = PhysicsPose2d::default();
    let movement = physics
        .move_top_down_character(&character, start, Vec2::new(4.0, 1.0), STEP, Some(actor))
        .unwrap();
    assert!((movement.translation.x - 1.69).abs() < 0.02, "{movement:?}");
    assert!(movement.translation.y > 0.95);
    assert!(movement.collision_count > 0);
    assert_eq!(
        physics
            .colliders()
            .find(|collider| collider.entity == actor)
            .unwrap()
            .pose,
        start
    );
    assert_eq!(
        physics.cast_ray(Vec2::ZERO, Vec2::ZERO, 4.0, solids),
        Err(PhysicsError::InvalidRay)
    );
}

#[test]
fn invalid_inputs_are_excluded_until_ecs_repairs_the_entity() {
    let mut app = physics_app();
    let parent = app.world_mut().unwrap().spawn(Transform2d::IDENTITY).id();
    let child = app
        .world_mut()
        .unwrap()
        .spawn((RigidBody2d::Dynamic, CIRCLE, ChildOf(parent)))
        .id();
    let malformed = app
        .world_mut()
        .unwrap()
        .spawn((
            Collider2d::Circle {
                radius: f32::INFINITY,
            },
            Transform2d::from_xy(5.0, 0.0),
        ))
        .id();
    app.add_systems(
        CoreStage::FixedUpdate,
        (move |time: Res<FixedTime>, mut commands: Commands| {
            if time.tick() == 2 {
                commands.entity(child).remove::<ChildOf>();
                commands.entity(malformed).insert(Collider2d::CapsuleY {
                    half_height: 0.3,
                    radius: 0.2,
                });
            }
        })
        .before(Physics2dSet::Step),
    )
    .unwrap();
    app.step_fixed(STEP).unwrap();
    let physics = app.world().resource::<PhysicsWorld2d>();
    assert_eq!(
        physics.body_error(child),
        Some(PhysicsError::ParentedDynamic)
    );
    assert_eq!(
        physics.body_error(malformed),
        Some(PhysicsError::InvalidCollider)
    );
    assert_eq!(physics.colliders().count(), 0);

    app.step_fixed(STEP).unwrap();
    let physics = app.world().resource::<PhysicsWorld2d>();
    assert_eq!(physics.body_error(child), None);
    assert_eq!(physics.body_error(malformed), None);
    assert_eq!(physics.colliders().count(), 2);
    assert_eq!(
        physics
            .overlap_circle(Vec2::new(5.0, 0.45), 0.1, PhysicsQueryFilter2d::default())
            .unwrap(),
        [malformed]
    );

    app.world_mut()
        .unwrap()
        .entity_mut(child)
        .remove::<Collider2d>();
    app.step_fixed(STEP).unwrap();
    assert!(app.world().get::<RigidBody2d>(child).is_some());
    let physics = app.world().resource::<PhysicsWorld2d>();
    assert_eq!(
        physics
            .colliders()
            .map(|collider| collider.entity)
            .collect::<Vec<_>>(),
        [malformed]
    );
}

#[test]
fn collision_groups_schema_defaults_to_all_bits() {
    let schema = CollisionGroups2d::type_schema().unwrap();
    assert_eq!(schema.id().as_str(), "nara.CollisionGroups2d");
    let defaults = schema
        .with_defaults(ComponentValue::Map(Default::default()))
        .unwrap();
    assert_eq!(
        CollisionGroups2d::decode_component((&defaults).into()).unwrap(),
        CollisionGroups2d::ALL,
    );
    assert_eq!(CollisionGroups2d::default(), CollisionGroups2d::ALL);
}

#[test]
fn bilateral_masks_update_contacts_and_committed_queries() {
    const PLAYER: u32 = 1;
    const WALL: u32 = 1 << 31;
    const ENEMY: u32 = 1 << 2;

    let mut app = physics_app();
    let wall = app
        .world_mut()
        .unwrap()
        .spawn((
            UNIT_BOX,
            Transform2d::from_xy(2.0, 0.0),
            CollisionGroups2d {
                memberships: WALL,
                filters: ENEMY,
            },
        ))
        .id();
    let actor_groups = CollisionGroups2d {
        memberships: PLAYER,
        filters: WALL,
    };
    let actor = app
        .world_mut()
        .unwrap()
        .spawn((
            RigidBody2d::Kinematic,
            CIRCLE,
            Transform2d::from_xy(2.0, 0.0),
            actor_groups,
        ))
        .id();
    let filter = PhysicsQueryFilter2d {
        exclude: Some(actor),
        groups: Some(actor_groups),
        ..Default::default()
    };
    let character = TopDownCharacter::new(CIRCLE, 0.01).unwrap();
    let start = PhysicsPose2d::default();
    let displacement = Vec2::X * 4.0;
    let ray = |app: &App, filter| {
        app.world()
            .resource::<PhysicsWorld2d>()
            .cast_ray(Vec2::ZERO, Vec2::X, 4.0, filter)
            .unwrap()
            .map(|hit| hit.entity)
    };

    app.step_fixed(STEP).unwrap();
    assert_eq!(ray(&app, filter), None);
    assert!(app.world().resource::<CollisionLog>().0.is_empty());
    let physics = app.world().resource::<PhysicsWorld2d>();
    assert_eq!(
        physics
            .move_top_down_character_with_filter(&character, start, displacement, STEP, filter)
            .unwrap()
            .translation,
        displacement,
    );

    app.world_mut()
        .unwrap()
        .get_mut::<CollisionGroups2d>(wall)
        .unwrap()
        .filters = PLAYER;
    assert_eq!(ray(&app, filter), None);
    app.step_fixed(STEP).unwrap();
    assert_eq!(ray(&app, filter), Some(wall));
    let physics = app.world().resource::<PhysicsWorld2d>();
    assert_eq!(
        physics
            .cast_shape(CIRCLE, start, displacement, filter)
            .unwrap()
            .unwrap()
            .entity,
        wall
    );
    assert_eq!(
        physics.overlap_circle(Vec2::X * 2.0, 0.1, filter).unwrap(),
        [wall]
    );
    let blocked = physics
        .move_top_down_character_with_filter(&character, start, displacement, STEP, filter)
        .unwrap();
    assert!(blocked.translation.x < 1.3, "{blocked:?}");
    let (first, second) = pair(actor, wall);
    assert_eq!(
        app.world().resource::<CollisionLog>().0,
        [CollisionEvent::Started(first, second)]
    );

    app.world_mut()
        .unwrap()
        .get_mut::<CollisionGroups2d>(wall)
        .unwrap()
        .memberships = ENEMY;
    app.step_fixed(STEP).unwrap();
    let physics = app.world().resource::<PhysicsWorld2d>();
    assert!(
        physics
            .cast_shape(CIRCLE, start, displacement, filter)
            .unwrap()
            .is_none()
    );
    assert!(
        physics
            .overlap_circle(Vec2::X * 2.0, 0.1, filter)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        app.world().resource::<CollisionLog>().0,
        [
            CollisionEvent::Started(first, second),
            CollisionEvent::Stopped(first, second),
        ]
    );

    app.world_mut()
        .unwrap()
        .entity_mut(wall)
        .remove::<CollisionGroups2d>();
    app.step_fixed(STEP).unwrap();
    assert_eq!(ray(&app, filter), Some(wall));
    assert_eq!(
        app.world().resource::<CollisionLog>().0.last(),
        Some(&CollisionEvent::Started(first, second))
    );
}

#[test]
fn dynamic_velocity_is_written_back_force_persists_and_impulse_is_consumed_once() {
    let mut app = physics_app();
    app.world_mut()
        .unwrap()
        .resource_mut::<PhysicsWorld2d>()
        .set_gravity(Vec2::ZERO)
        .unwrap();
    let force = ExternalForce2d {
        force: Vec2::X * 3.0,
        torque: 1.0 / 6.0,
    };
    let body = app
        .world_mut()
        .unwrap()
        .spawn((
            RigidBody2d::Dynamic,
            UNIT_BOX,
            Velocity2d {
                linear: Vec2::X * 2.0,
                angular: 1.0,
            },
            force,
            ExternalImpulse2d {
                impulse: Vec2::X * 4.0,
                torque_impulse: 1.0 / 6.0,
            },
        ))
        .id();
    let dt = Duration::from_millis(100);
    for (x, angular) in [(6.3, 2.1), (6.6, 2.2)] {
        app.step_fixed(dt).unwrap();
        let world = app.world();
        let velocity = world.get::<Velocity2d>(body).unwrap();
        assert!((velocity.linear.x - x).abs() < 0.001, "{velocity:?}");
        assert!((velocity.angular - angular).abs() < 0.001, "{velocity:?}");
        assert_eq!(world.get::<ExternalForce2d>(body), Some(&force));
        assert_eq!(
            world.get::<ExternalImpulse2d>(body),
            Some(&ExternalImpulse2d::default())
        );
    }

    let overwritten = Velocity2d {
        linear: Vec2::NEG_X,
        angular: -1.0,
    };
    let mut entity = app.world_mut().unwrap().entity_mut(body);
    entity.remove::<ExternalForce2d>();
    entity.insert(overwritten);
    app.step_fixed(dt).unwrap();
    assert_eq!(app.world().get::<Velocity2d>(body), Some(&overwritten));
}

#[test]
fn impulse_waits_for_a_valid_dynamic_collider_and_uses_replaced_mass() {
    let mut app = physics_app();
    let pending = ExternalImpulse2d {
        impulse: Vec2::X * 2.0,
        ..Default::default()
    };
    let body = app
        .world_mut()
        .unwrap()
        .spawn((RigidBody2d::Static, Velocity2d::default(), pending))
        .id();
    app.step_fixed(STEP).unwrap();
    assert_eq!(app.world().get::<ExternalImpulse2d>(body), Some(&pending));

    app.world_mut()
        .unwrap()
        .entity_mut(body)
        .insert(RigidBody2d::Dynamic);
    app.step_fixed(STEP).unwrap();
    assert_eq!(app.world().get::<ExternalImpulse2d>(body), Some(&pending));

    app.world_mut()
        .unwrap()
        .entity_mut(body)
        .insert(Collider2d::Circle { radius: -1.0 });
    app.step_fixed(STEP).unwrap();
    assert_eq!(
        app.world().resource::<PhysicsWorld2d>().body_error(body),
        Some(PhysicsError::InvalidCollider)
    );
    assert_eq!(app.world().get::<ExternalImpulse2d>(body), Some(&pending));

    app.world_mut().unwrap().entity_mut(body).insert(UNIT_BOX);
    app.step_fixed(STEP).unwrap();
    assert!((app.world().get::<Velocity2d>(body).unwrap().linear.x - 2.0).abs() < 0.001);
    assert_eq!(
        app.world().get::<ExternalImpulse2d>(body),
        Some(&ExternalImpulse2d::default())
    );

    app.world_mut().unwrap().entity_mut(body).insert((
        Collider2d::Box {
            half_extents: Vec2::new(1.0, 0.5),
        },
        Velocity2d::default(),
        pending,
    ));
    app.step_fixed(STEP).unwrap();
    assert!((app.world().get::<Velocity2d>(body).unwrap().linear.x - 1.0).abs() < 0.001);
    assert_eq!(
        app.world().get::<ExternalImpulse2d>(body),
        Some(&ExternalImpulse2d::default())
    );
}

#[test]
fn damping_gravity_scale_and_locks_apply_and_reset_from_ecs() {
    let mut app = physics_app();
    app.world_mut()
        .unwrap()
        .resource_mut::<PhysicsWorld2d>()
        .set_gravity(Vec2::NEG_Y * 10.0)
        .unwrap();
    let body = app
        .world_mut()
        .unwrap()
        .spawn((
            RigidBody2d::Dynamic,
            UNIT_BOX,
            Velocity2d {
                linear: Vec2::X * 4.0,
                angular: 2.0,
            },
            Damping2d {
                linear: 5.0,
                angular: 5.0,
            },
            GravityScale2d(-1.0),
        ))
        .id();
    let dt = Duration::from_millis(100);
    app.step_fixed(dt).unwrap();
    let velocity = app.world().get::<Velocity2d>(body).unwrap();
    assert!((0.0..4.0).contains(&velocity.linear.x), "{velocity:?}");
    assert!(velocity.linear.y > 0.0, "{velocity:?}");
    assert!((0.0..2.0).contains(&velocity.angular), "{velocity:?}");

    let pose = *app.world().get::<Transform2d>(body).unwrap();
    app.world_mut().unwrap().entity_mut(body).insert((
        LockedAxes2d {
            lock_translation_x: true,
            lock_translation_y: true,
            lock_rotation: true,
        },
        Velocity2d {
            linear: Vec2::splat(5.0),
            angular: 5.0,
        },
        ExternalImpulse2d {
            impulse: Vec2::splat(5.0),
            torque_impulse: 5.0,
        },
    ));
    app.step_fixed(dt).unwrap();
    let locked = app.world().get::<Transform2d>(body).unwrap();
    assert!((locked.translation - pose.translation).length() < 0.001);
    assert!((locked.rotation - pose.rotation).abs() < 0.001);
    assert_eq!(
        app.world().get::<Velocity2d>(body),
        Some(&Velocity2d::default())
    );

    app.world_mut()
        .unwrap()
        .entity_mut(body)
        .remove::<(LockedAxes2d, Damping2d, GravityScale2d)>()
        .insert(Velocity2d {
            linear: Vec2::X,
            angular: 2.0,
        });
    app.step_fixed(dt).unwrap();
    let velocity = app.world().get::<Velocity2d>(body).unwrap();
    assert_eq!(velocity.linear.x, 1.0);
    assert!((velocity.linear.y + 1.0).abs() < 0.001, "{velocity:?}");
    assert_eq!(velocity.angular, 2.0);
}

#[test]
fn restitution_changes_take_effect_on_existing_colliders() {
    let mut app = physics_app();
    app.world_mut()
        .unwrap()
        .resource_mut::<PhysicsWorld2d>()
        .set_gravity(Vec2::ZERO)
        .unwrap();
    let wall = app
        .world_mut()
        .unwrap()
        .spawn(Collider2d::Box {
            half_extents: Vec2::new(0.25, 2.0),
        })
        .id();
    let initial_pose = Transform2d::from_xy(-1.0, 0.0);
    let initial_velocity = Velocity2d {
        linear: Vec2::X * 5.0,
        angular: 0.0,
    };
    let body = app
        .world_mut()
        .unwrap()
        .spawn((RigidBody2d::Dynamic, CIRCLE, initial_pose, initial_velocity))
        .id();
    for _ in 0..12 {
        app.step_fixed(STEP).unwrap();
    }
    assert!(app.world().get::<Velocity2d>(body).unwrap().linear.x.abs() < 0.1);

    let elastic = PhysicsMaterial2d {
        restitution: 1.0,
        ..Default::default()
    };
    let world = app.world_mut().unwrap();
    world.entity_mut(wall).insert(elastic);
    world
        .entity_mut(body)
        .insert((initial_pose, Teleport2d, initial_velocity, elastic));
    for _ in 0..12 {
        app.step_fixed(STEP).unwrap();
    }
    assert!(app.world().get::<Velocity2d>(body).unwrap().linear.x < -4.5);
}

#[test]
fn invalid_dynamic_and_material_inputs_are_reported_before_stepping() {
    let mut app = physics_app();
    let world = app.world_mut().unwrap();
    let cases = [
        (
            world
                .spawn((
                    UNIT_BOX,
                    Velocity2d {
                        linear: Vec2::NAN,
                        angular: 0.0,
                    },
                ))
                .id(),
            PhysicsError::InvalidVelocity,
        ),
        (
            world
                .spawn((
                    UNIT_BOX,
                    ExternalForce2d {
                        torque: f32::NAN,
                        ..Default::default()
                    },
                ))
                .id(),
            PhysicsError::InvalidForce,
        ),
        (
            world
                .spawn((
                    UNIT_BOX,
                    ExternalImpulse2d {
                        impulse: Vec2::NAN,
                        ..Default::default()
                    },
                ))
                .id(),
            PhysicsError::InvalidImpulse,
        ),
        (
            world
                .spawn((
                    UNIT_BOX,
                    PhysicsMaterial2d {
                        friction: -0.5,
                        restitution: 0.0,
                    },
                ))
                .id(),
            PhysicsError::InvalidMaterial,
        ),
        (
            world
                .spawn((
                    UNIT_BOX,
                    Damping2d {
                        linear: 0.0,
                        angular: -1.0,
                    },
                ))
                .id(),
            PhysicsError::InvalidDamping,
        ),
        (
            world.spawn((UNIT_BOX, GravityScale2d(f32::INFINITY))).id(),
            PhysicsError::InvalidGravityScale,
        ),
    ];
    app.step_fixed(STEP).unwrap();
    let physics = app.world().resource::<PhysicsWorld2d>();
    for (entity, error) in cases {
        assert_eq!(physics.body_error(entity), Some(error));
    }
    assert_eq!(physics.colliders().count(), 0);
}
