use std::{
    f32::consts::{FRAC_PI_2, FRAC_PI_6, PI},
    time::Duration,
};

use nara_app::{App, FixedTime};
use nara_core::Vec2;
use nara_physics2d::{
    Collider2d, CollisionGroups2d, CompoundBox2d, CompoundCollider2d, GridRect, Physics2dPlugin,
    PhysicsError, PhysicsPose2d, PhysicsQueryFilter2d, PhysicsWorld2d, PlatformerAutostep,
    PlatformerCharacter, PlatformerConfig, PlatformerJumpTimers, PlatformerMovement,
    merge_solid_grid_rects,
};
use nara_scene::spatial::{ChildOf, Transform2d};

const STEP: Duration = FixedTime::DEFAULT_TIMESTEP;
const CIRCLE: Collider2d = Collider2d::Circle { radius: 0.25 };
const ONE_WAY: u32 = 1 << 5;

fn physics_app() -> App {
    let mut app = App::new();
    app.add_plugin(Physics2dPlugin).unwrap();
    app
}

fn move_at(
    app: &App,
    character: &PlatformerCharacter,
    position: Vec2,
    displacement: Vec2,
) -> PlatformerMovement {
    app.world()
        .resource::<PhysicsWorld2d>()
        .move_platformer(
            character,
            PhysicsPose2d::new(position, 0.0),
            displacement,
            STEP,
            PhysicsQueryFilter2d::default(),
        )
        .unwrap()
}

#[test]
fn grid_merging_keeps_contiguous_runs_and_preserves_holes() {
    let rows = [b"####..", b"####.#", b"##...#", b"..#..."];
    let mut cells = Vec::new();
    let merged = merge_solid_grid_rects(6, 4, |x, y| {
        cells.push((x, y));
        rows[y as usize][x as usize] == b'#'
    });
    let row_major: Vec<_> = (0..4).flat_map(|y| (0..6).map(move |x| (x, y))).collect();
    assert_eq!(cells, row_major);
    assert_eq!(
        merged,
        [
            GridRect {
                x: 0,
                y: 0,
                width: 4,
                height: 2
            },
            GridRect {
                x: 5,
                y: 1,
                width: 1,
                height: 2
            },
            GridRect {
                x: 0,
                y: 2,
                width: 2,
                height: 1
            },
            GridRect {
                x: 2,
                y: 3,
                width: 1,
                height: 1
            },
        ]
    );
    assert!(merge_solid_grid_rects(0, 8, |_, _| unreachable!()).is_empty());
}

#[test]
fn compound_geometry_uses_parent_pose_and_updates_the_committed_world() {
    let mut app = physics_app();
    let parent = app
        .world_mut()
        .unwrap()
        .spawn(Transform2d {
            rotation: FRAC_PI_2,
            ..Transform2d::from_xy(3.0, 0.0)
        })
        .id();
    let boxes = [
        CompoundBox2d {
            center: Vec2::X * 2.0,
            half_extents: Vec2::splat(0.5),
        },
        CompoundBox2d {
            center: Vec2::NEG_X * 2.0,
            half_extents: Vec2::splat(0.5),
        },
    ];
    let compound = CompoundCollider2d::new(boxes).unwrap();
    let entity = app
        .world_mut()
        .unwrap()
        .spawn((compound.clone(), ChildOf(parent)))
        .id();
    let filter = PhysicsQueryFilter2d::default();
    let upper = Vec2::new(3.0, 2.0);
    app.step_fixed(STEP).unwrap();
    let physics = app.world().resource::<PhysicsWorld2d>();
    let geometry = physics.compound_colliders().next().unwrap();
    assert_eq!(geometry.entity, entity);
    assert_eq!(geometry.pose.translation, Vec2::new(3.0, 0.0));
    assert!((geometry.pose.rotation - FRAC_PI_2).abs() < 0.001);
    assert_eq!(geometry.boxes(), &boxes);
    assert_eq!(
        physics.overlap_circle(upper, 0.1, filter).unwrap(),
        [entity]
    );
    assert_eq!(
        physics
            .overlap_circle(geometry.pose.translation, 3.0, filter)
            .unwrap(),
        [entity]
    );

    let moved = CompoundCollider2d::new([CompoundBox2d {
        center: Vec2::ZERO,
        half_extents: Vec2::splat(0.5),
    }])
    .unwrap();
    app.world_mut().unwrap().entity_mut(entity).insert(moved);
    app.step_fixed(STEP).unwrap();
    let physics = app.world().resource::<PhysicsWorld2d>();
    assert!(
        physics
            .overlap_circle(upper, 0.1, filter)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        physics
            .overlap_circle(Vec2::new(3.0, 0.0), 0.1, filter)
            .unwrap(),
        [entity]
    );

    app.world_mut().unwrap().entity_mut(entity).insert(CIRCLE);
    app.step_fixed(STEP).unwrap();
    let physics = app.world().resource::<PhysicsWorld2d>();
    assert_eq!(
        physics.body_error(entity),
        Some(PhysicsError::ConflictingColliders)
    );
    assert_eq!(physics.compound_colliders().count(), 0);
    app.world_mut()
        .unwrap()
        .entity_mut(entity)
        .remove::<CompoundCollider2d>();
    app.step_fixed(STEP).unwrap();
    assert_eq!(
        app.world()
            .resource::<PhysicsWorld2d>()
            .colliders()
            .next()
            .unwrap()
            .entity,
        entity
    );
}

#[test]
fn platformer_distinguishes_climbable_and_sliding_slopes() {
    let mut app = physics_app();
    app.world_mut().unwrap().spawn((
        Collider2d::Box {
            half_extents: Vec2::new(5.0, 0.25),
        },
        Transform2d {
            rotation: FRAC_PI_6,
            ..Transform2d::IDENTITY
        },
    ));
    app.step_fixed(STEP).unwrap();
    let tangent = Vec2::new(FRAC_PI_6.cos(), FRAC_PI_6.sin());
    let normal = Vec2::new(-tangent.y, tangent.x);
    let start = normal * 0.51 - tangent;
    let climbable = PlatformerCharacter::new(CIRCLE, PlatformerConfig::default()).unwrap();
    let steep = PlatformerCharacter::new(
        CIRCLE,
        PlatformerConfig {
            max_slope_climb_angle: PI / 12.0,
            min_slope_slide_angle: PI / 12.0,
            ..Default::default()
        },
    )
    .unwrap();

    let uphill = Vec2::new(1.5, -0.1);
    let climbed = move_at(&app, &climbable, start, uphill);
    assert!(
        climbed.translation.x > 0.9 && climbed.translation.y > 0.5,
        "{climbed:?}"
    );
    assert!(climbed.grounded, "{climbed:?}");
    assert!(
        (climbed.slope_angle.unwrap() - FRAC_PI_6).abs() < 0.02,
        "{climbed:?}"
    );
    assert_eq!(climbed.wall_normal, None);
    let blocked = move_at(&app, &steep, start, uphill);
    assert!(blocked.translation.x < 0.02, "{blocked:?}");
    assert!(
        blocked.wall_normal.unwrap().dot(normal) > 0.99,
        "{blocked:?}"
    );

    let down = Vec2::NEG_Y * 0.5;
    let held = move_at(&app, &climbable, start, down);
    let slid = move_at(&app, &steep, start, down);
    assert!(held.translation.x.abs() < 0.02, "{held:?}");
    assert!(
        slid.translation.x < -0.1 && slid.translation.y < -0.05,
        "{slid:?}"
    );
}

#[test]
fn platformer_steps_up_a_small_solid_ledge() {
    let mut app = physics_app();
    let world = app.world_mut().unwrap();
    world.spawn((
        Collider2d::Box {
            half_extents: Vec2::new(5.0, 0.5),
        },
        Transform2d::from_xy(0.0, -0.5),
    ));
    world.spawn((
        Collider2d::Box {
            half_extents: Vec2::new(1.5, 0.075),
        },
        Transform2d::from_xy(1.5, 0.075),
    ));
    app.step_fixed(STEP).unwrap();
    let shape = Collider2d::Box {
        half_extents: Vec2::new(0.2, 0.25),
    };
    let blocked = PlatformerCharacter::new(shape, PlatformerConfig::default()).unwrap();
    let stepping = PlatformerCharacter::new(
        shape,
        PlatformerConfig {
            autostep: Some(PlatformerAutostep {
                max_height: 0.25,
                min_width: 0.35,
                include_dynamic_bodies: false,
            }),
            ..Default::default()
        },
    )
    .unwrap();
    let start = Vec2::new(-1.0, 0.26);
    let displacement = Vec2::X * 2.0;
    let wall = move_at(&app, &blocked, start, displacement);
    assert!(
        wall.translation.x < 0.85 && wall.wall_normal.is_some(),
        "{wall:?}"
    );
    let step = move_at(&app, &stepping, start, displacement);
    assert!(
        step.translation.x > 1.8 && step.translation.y > 0.14,
        "{step:?}"
    );
    assert!(step.grounded, "{step:?}");
}

#[test]
fn platformer_jumps_through_one_way_shapes_and_lands_from_above() {
    let mut app = physics_app();
    let world = app.world_mut().unwrap();
    world.spawn((
        Collider2d::Box {
            half_extents: Vec2::new(2.0, 0.125),
        },
        CollisionGroups2d {
            memberships: ONE_WAY,
            filters: u32::MAX,
        },
    ));
    world.spawn((
        Collider2d::Box {
            half_extents: Vec2::new(2.0, 0.125),
        },
        Transform2d::from_xy(0.0, 2.0),
    ));
    app.step_fixed(STEP).unwrap();
    let config = PlatformerConfig {
        one_way_layer: Some(ONE_WAY),
        ..Default::default()
    };
    let character = PlatformerCharacter::new(CIRCLE, config).unwrap();

    let jumping = move_at(&app, &character, Vec2::NEG_Y, Vec2::Y * 2.0);
    assert_eq!(jumping.translation, Vec2::Y * 2.0);
    assert!(!jumping.grounded && !jumping.hit_ceiling);
    let ceiling = move_at(&app, &character, Vec2::Y, Vec2::Y * 2.0);
    assert!(
        ceiling.hit_ceiling && ceiling.translation.y < 0.65,
        "{ceiling:?}"
    );

    let landing = move_at(&app, &character, Vec2::Y, Vec2::NEG_Y * 2.0);
    assert!(
        landing.grounded && !landing.hit_ceiling && landing.collision_count == 1,
        "{landing:?}"
    );
    assert!((landing.translation.y + 0.615).abs() < 0.005, "{landing:?}");
    let position = Vec2::Y + landing.translation;
    let standing = move_at(&app, &character, position, Vec2::ZERO);
    assert!(
        standing.grounded && standing.slope_angle.unwrap() < 0.01,
        "{standing:?}"
    );
    assert_eq!(standing.translation, Vec2::ZERO);
    let uphill = move_at(&app, &character, position, Vec2::Y * 0.001);
    assert!(!uphill.grounded, "{uphill:?}");

    let side = Vec2::new(2.0, -0.25);
    let passing = move_at(&app, &character, Vec2::new(-3.0, 0.1), side);
    assert!(
        (passing.translation - side).length() < 0.001 && !passing.grounded,
        "{passing:?}"
    );
    let physics = app.world().resource::<PhysicsWorld2d>();
    let ignored = physics
        .move_platformer(
            &character,
            PhysicsPose2d::new(Vec2::Y, 0.0),
            Vec2::NEG_Y * 2.0,
            STEP,
            PhysicsQueryFilter2d {
                groups: Some(CollisionGroups2d {
                    memberships: 1,
                    filters: 1,
                }),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(ignored.translation, Vec2::NEG_Y * 2.0);

    let reversed = PlatformerCharacter::new(
        CIRCLE,
        PlatformerConfig {
            up: Vec2::NEG_Y * 2.0,
            ..config
        },
    )
    .unwrap();
    let landing = move_at(&app, &reversed, Vec2::NEG_Y, Vec2::Y * 2.0);
    assert!(
        landing.grounded && (landing.translation.y - 0.615).abs() < 0.005,
        "{landing:?}"
    );
}

#[test]
fn one_way_compound_uses_each_box_top_for_landing_and_ground_probe() {
    let mut app = physics_app();
    let platforms = CompoundCollider2d::new([0.0, 2.0].map(|y| CompoundBox2d {
        center: Vec2::new(0.0, y),
        half_extents: Vec2::new(2.0, 0.125),
    }))
    .unwrap();
    app.world_mut().unwrap().spawn((
        platforms,
        CollisionGroups2d {
            memberships: ONE_WAY,
            filters: u32::MAX,
        },
    ));
    app.step_fixed(STEP).unwrap();
    let character = PlatformerCharacter::new(
        CIRCLE,
        PlatformerConfig {
            one_way_layer: Some(ONE_WAY),
            ..Default::default()
        },
    )
    .unwrap();
    for start in [1.0, 3.0] {
        let position = Vec2::new(0.0, start);
        let landed = move_at(&app, &character, position, Vec2::NEG_Y * 2.0);
        assert!((landed.translation.y + 0.615).abs() < 0.005, "{landed:?}");
        assert!(landed.grounded && landed.collision_count == 1, "{landed:?}");
        let standing = move_at(&app, &character, position + landed.translation, Vec2::X);
        assert_eq!(standing.translation, Vec2::X);
        assert!(standing.grounded, "{standing:?}");
    }
    let jump = move_at(&app, &character, Vec2::NEG_Y, Vec2::Y * 4.0);
    assert_eq!(jump.translation, Vec2::Y * 4.0);
    assert_eq!(jump.collision_count, 0);
}

#[test]
fn ground_snap_reaches_a_lower_one_way_platform() {
    let mut app = physics_app();
    let world = app.world_mut().unwrap();
    let shape = Collider2d::Box {
        half_extents: Vec2::new(2.0, 0.5),
    };
    world.spawn((shape, Transform2d::from_xy(-2.0, -0.5)));
    world.spawn((
        shape,
        Transform2d::from_xy(2.0, -0.7),
        CollisionGroups2d {
            memberships: ONE_WAY,
            filters: u32::MAX,
        },
    ));
    app.step_fixed(STEP).unwrap();
    let config = PlatformerConfig {
        one_way_layer: Some(ONE_WAY),
        ..Default::default()
    };
    let no_snap = PlatformerCharacter::new(CIRCLE, config).unwrap();
    let snap = PlatformerCharacter::new(
        CIRCLE,
        PlatformerConfig {
            snap_to_ground: Some(0.3),
            ..config
        },
    )
    .unwrap();
    let start = Vec2::new(-1.0, 0.26);
    let displacement = Vec2::X * 2.0;
    let floating = move_at(&app, &no_snap, start, displacement);
    assert!(!floating.grounded, "{floating:?}");
    let snapped = move_at(&app, &snap, start, displacement);
    assert!(
        snapped.grounded && (snapped.translation.y + 0.2).abs() < 0.005,
        "{snapped:?}"
    );
    let lifting = move_at(&app, &snap, start, displacement + Vec2::Y * 0.05);
    assert!(
        lifting.translation.y > 0.04 && !lifting.grounded,
        "{lifting:?}"
    );
}

#[test]
fn merged_floor_supports_motion_across_tile_boundaries() {
    let mut app = physics_app();
    let rectangles = merge_solid_grid_rects(8, 2, |x, y| y == 0 || x % 3 == 0);
    assert_eq!(rectangles.len(), 4);
    let boxes: Vec<_> = rectangles
        .into_iter()
        .map(|rect| {
            let half_extents = Vec2::new(rect.width as f32, rect.height as f32) * 0.5;
            CompoundBox2d {
                center: Vec2::new(
                    rect.x as f32 + half_extents.x,
                    -(rect.y as f32) - half_extents.y,
                ),
                half_extents,
            }
        })
        .collect();
    app.world_mut()
        .unwrap()
        .spawn(CompoundCollider2d::new(boxes).unwrap());
    app.step_fixed(STEP).unwrap();

    let character = PlatformerCharacter::new(CIRCLE, PlatformerConfig::default()).unwrap();
    let mut pose = Vec2::new(0.5, 0.26);
    for _ in 0..12 {
        let motion = move_at(&app, &character, pose, Vec2::new(0.55, -0.01));
        assert!(
            motion.grounded && motion.wall_normal.is_none(),
            "{motion:?}"
        );
        assert!((motion.translation.x - 0.55).abs() < 0.005, "{motion:?}");
        pose += motion.translation;
    }
    assert!((pose.y - 0.26).abs() < 0.005, "{pose:?}");
}

#[test]
fn jump_timers_expire_consume_all_windows_and_replay_deterministically() {
    let step = Duration::from_millis(10);
    let mut timers = PlatformerJumpTimers::new(step * 3, step * 2);
    let inputs = [
        (true, false),
        (false, true),
        (false, true),
        (true, false),
        (false, false),
        (false, false),
        (true, false),
        (false, false),
        (false, false),
        (false, true),
        (false, false),
        (true, false),
    ];
    let replay = |timers: &mut PlatformerJumpTimers| {
        inputs.map(|(grounded, pressed)| timers.tick(step, grounded, pressed))
    };
    let mut second = timers;
    let first_run = replay(&mut timers);
    assert_eq!(replay(&mut second), first_run);
    assert_eq!(
        first_run,
        [
            false, true, false, true, false, false, false, false, false, false, false, false
        ]
    );
    let mut disabled = PlatformerJumpTimers::default();
    assert!(!disabled.tick(step, false, true));
    assert!(!disabled.tick(step, true, false));
    assert!(disabled.tick(step, true, true));
}
