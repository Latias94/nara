use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use bevy_ecs::{change_detection::DetectChanges, prelude::*};
use nara_app::{App, CoreStage, FixedUpdateSet, StartupStage};
use nara_core::Vec2;

use super::{
    ChildOf, GlobalTransform2d, InheritedVisibility, SpatialPlugin, SpatialSet, Transform2d,
    Visibility, propagate_spatial, register_spatial,
};

fn schedule() -> Schedule {
    let mut schedule = Schedule::default();
    schedule.add_systems(propagate_spatial);
    schedule
}

#[test]
fn required_components_compose_affine_depth_and_visibility_through_a_bare_node() {
    let mut world = World::new();
    let root = world
        .spawn((
            Transform2d {
                translation: Vec2::new(10.0, 4.0),
                rotation: FRAC_PI_2,
                scale: Vec2::new(2.0, 3.0),
                z: 5.0,
            },
            Visibility::Hidden,
        ))
        .id();
    let group = world.spawn(ChildOf(root)).id();
    let child = world
        .spawn((
            Transform2d {
                scale: Vec2::new(0.5, 2.0),
                z: -2.0,
                ..Transform2d::from_xy(1.0, 2.0)
            },
            ChildOf(group),
        ))
        .id();
    let override_visibility = world.spawn((Visibility::Visible, ChildOf(child))).id();

    assert_eq!(
        world.get::<GlobalTransform2d>(child),
        Some(&GlobalTransform2d::IDENTITY)
    );
    assert!(world.get::<Visibility>(child).is_none());
    assert!(world.get::<InheritedVisibility>(child).is_some());

    register_spatial(&mut world);
    schedule().run(&mut world);

    let global = world.get::<GlobalTransform2d>(child).unwrap();
    assert!(global.translation().abs_diff_eq(Vec2::new(4.0, 6.0), 1e-5));
    assert!(
        global
            .matrix()
            .transform_point2(Vec2::X)
            .abs_diff_eq(Vec2::new(4.0, 7.0), 1e-5)
    );
    assert_eq!(global.z(), 3.0);
    assert_eq!(
        world.get::<InheritedVisibility>(child),
        Some(&InheritedVisibility::HIDDEN)
    );
    assert!(
        world
            .get::<InheritedVisibility>(override_visibility)
            .unwrap()
            .get()
    );
    assert!(world.get::<GlobalTransform2d>(group).is_none());
}

#[test]
fn same_frame_reparent_and_detach_update_a_bare_branch_and_respect_linked_despawn() {
    let mut world = World::new();
    register_spatial(&mut world);
    let mut schedule = schedule();
    let old_parent = world.spawn(Transform2d::from_xy(3.0, 0.0)).id();
    let new_parent = world
        .spawn((Transform2d::from_xy(20.0, 0.0), Visibility::Hidden))
        .id();
    let group = world.spawn(ChildOf(old_parent)).id();
    let child = world
        .spawn((Transform2d::from_xy(2.0, 0.0), ChildOf(group)))
        .id();
    schedule.run(&mut world);

    world.entity_mut(group).insert(ChildOf(new_parent));
    world.entity_mut(group).insert(ChildOf(old_parent));
    world.entity_mut(group).insert(ChildOf(new_parent));
    assert!(world.despawn(old_parent));
    world.clear_trackers();
    world.clear_trackers();
    schedule.run(&mut world);

    assert_eq!(
        world.get::<GlobalTransform2d>(child).unwrap().translation(),
        Vec2::new(22.0, 0.0)
    );
    assert!(!world.get::<InheritedVisibility>(child).unwrap().get());

    world.entity_mut(group).remove::<ChildOf>();
    world.clear_trackers();
    world.clear_trackers();
    schedule.run(&mut world);
    assert_eq!(
        world.get::<GlobalTransform2d>(child).unwrap().translation(),
        Vec2::new(2.0, 0.0)
    );
    assert!(world.get::<InheritedVisibility>(child).unwrap().get());

    assert!(world.despawn(group));
    schedule.run(&mut world);
    assert!(world.get_entity(child).is_err());
    assert!(world.get_entity(new_parent).is_ok());
}

#[test]
fn fixed_and_frame_passes_observe_edits_and_removal_without_rewriting_clean_outputs() {
    #[derive(Component)]
    struct OtherArchetype;

    let mut world = World::new();
    register_spatial(&mut world);
    let mut fixed = schedule();
    let mut post_update = schedule();
    let parent = world.spawn(Transform2d::from_xy(4.0, 0.0)).id();
    let child = world
        .spawn((Transform2d::from_xy(1.0, 0.0), ChildOf(parent)))
        .id();
    let unrelated = world.spawn(Transform2d::from_xy(99.0, 0.0)).id();
    fixed.run(&mut world);
    let global_tick = |world: &World, entity: Entity| {
        world
            .entity(entity)
            .get_ref::<GlobalTransform2d>()
            .unwrap()
            .last_changed()
    };
    let untouched_since = global_tick(&world, unrelated);

    world.get_mut::<Transform2d>(parent).unwrap().translation.x = 7.0;
    world.entity_mut(parent).insert(Visibility::Hidden);
    world.get_mut::<Transform2d>(child).unwrap().translation.x = 2.0;
    world.entity_mut(parent).insert(OtherArchetype);
    post_update.run(&mut world);
    assert_eq!(
        world.get::<GlobalTransform2d>(child).unwrap().translation(),
        Vec2::new(9.0, 0.0)
    );
    assert!(!world.get::<InheritedVisibility>(child).unwrap().get());

    let settled_since = global_tick(&world, child);
    fixed.run(&mut world);
    post_update.run(&mut world);
    assert_eq!(global_tick(&world, child), settled_since);
    assert_eq!(global_tick(&world, unrelated), untouched_since);

    world
        .entity_mut(parent)
        .remove::<(Transform2d, Visibility)>();
    fixed.run(&mut world);
    assert_eq!(
        world.get::<GlobalTransform2d>(child).unwrap().translation(),
        Vec2::new(2.0, 0.0)
    );
    assert!(world.get::<InheritedVisibility>(child).unwrap().get());
    let removed_since = global_tick(&world, child);
    post_update.run(&mut world);
    assert_eq!(global_tick(&world, child), removed_since);
}

#[test]
fn cyclic_branch_is_recomputed_after_detaching_the_cycle() {
    let mut world = World::new();
    register_spatial(&mut world);
    let mut schedule = schedule();
    let parent = world.spawn(Transform2d::from_xy(3.0, 0.0)).id();
    let child = world
        .spawn((Transform2d::from_xy(2.0, 0.0), ChildOf(parent)))
        .id();
    schedule.run(&mut world);

    world.entity_mut(parent).insert(ChildOf(child));
    world.get_mut::<Transform2d>(child).unwrap().translation.x = 8.0;
    schedule.run(&mut world);
    assert_eq!(
        world.get::<GlobalTransform2d>(child).unwrap().translation(),
        Vec2::new(5.0, 0.0)
    );

    world.entity_mut(parent).remove::<ChildOf>();
    schedule.run(&mut world);
    assert_eq!(
        world.get::<GlobalTransform2d>(child).unwrap().translation(),
        Vec2::new(11.0, 0.0)
    );
}

#[test]
fn plugin_propagates_startup_and_both_fixed_boundaries() {
    let mut app = App::new();
    app.add_plugin(SpatialPlugin).unwrap();
    let entity = app
        .world_mut()
        .unwrap()
        .spawn(Transform2d::from_xy(1.0, 0.0))
        .id();
    app.add_systems(
        StartupStage::Tooling,
        move |globals: Query<&GlobalTransform2d>| {
            assert_eq!(globals.get(entity).unwrap().translation(), Vec2::X);
        },
    )
    .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        (
            (move |mut commands: Commands| {
                commands
                    .entity(entity)
                    .insert(Transform2d::from_xy(2.0, 0.0));
            })
            .in_set(FixedUpdateSet::Prepare),
            (move |mut commands: Commands, globals: Query<&GlobalTransform2d>| {
                assert_eq!(globals.get(entity).unwrap().translation().x, 2.0);
                commands
                    .entity(entity)
                    .insert(Transform2d::from_xy(3.0, 0.0));
            })
            .in_set(FixedUpdateSet::Simulate),
            (move |globals: Query<&GlobalTransform2d>| {
                assert_eq!(globals.get(entity).unwrap().translation().x, 3.0);
            })
            .in_set(FixedUpdateSet::Finalize)
            .after(SpatialSet::AfterFixed),
        ),
    )
    .unwrap();
    app.step_fixed(Duration::from_millis(16)).unwrap();
}
