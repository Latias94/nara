#![cfg(feature = "sprite-render")]
use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, RuntimeControl, RuntimeControlRequestResult,
    RuntimeInstance,
};
use nara_core::{Color, Mat3, Vec2};
use nara_ecs::{Component, Entity, Query, Resource, With, World, schedule::IntoScheduleConfigs};
use nara_render::sprite::Sprite;
use nara_render::sprite_render::{SpriteRenderPlugin, batching::ExtractedSprites};
use nara_render::{Camera2d, RenderImage2d, RenderTarget, ViewportRect};
use nara_scene::spatial::{ChildOf, GlobalTransform2d, SpatialPlugin, Transform2d};
use std::time::Duration;

#[derive(Component)]
struct MovingSprite;

fn advance_sprite(mut sprites: Query<&mut Transform2d, With<MovingSprite>>) {
    for mut transform in &mut sprites {
        transform.translation.x += 8.0;
    }
}

#[test]
fn fixed_motion_is_extracted_from_the_latest_global_without_mutating_simulation() {
    let (app, sprite) = motion_scene();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let step = FixedTime::DEFAULT_TIMESTEP;
    runtime.drive(step).unwrap();
    runtime.drive(step / 2).unwrap();
    let rendered = rendered_center(&runtime, sprite);
    runtime.drive(Duration::ZERO).unwrap();
    let rendered_again = rendered_center(&runtime, sprite);
    let simulation = runtime
        .world()
        .get::<Transform2d>(sprite)
        .unwrap()
        .translation;
    let simulation_global = runtime
        .world()
        .get::<GlobalTransform2d>(sprite)
        .unwrap()
        .translation();
    let tick = runtime.world().resource::<FixedTime>().tick();
    // Physically retire before the behavioral assertion so a red result remains readable.
    runtime.finish().unwrap();

    assert_eq!(tick, 1);
    assert_eq!(simulation, Vec2::new(8.0, 0.0));
    assert_eq!(simulation_global, simulation);
    assert_eq!(rendered, simulation_global);
    assert_eq!(rendered_again, rendered);
}

fn presentation_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        SpatialPlugin,
        nara_render::RenderPlugin::default(),
        nara_render::image::ImagePreparePlugin,
        SpriteRenderPlugin,
    ))
    .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        advance_sprite.in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    app
}

fn camera() -> Camera2d {
    Camera2d {
        target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
        viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
        viewport_height: 100.0,
        ..Default::default()
    }
}

fn motion_scene() -> (App, Entity) {
    let mut app = presentation_app();
    let world = app.world_mut().unwrap();
    world.spawn((camera(), Transform2d::default()));
    let sprite = world
        .spawn((
            MovingSprite,
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    (app, sprite)
}

fn rendered_center(runtime: &RuntimeInstance, entity: Entity) -> Vec2 {
    runtime
        .world()
        .resource::<ExtractedSprites>()
        .as_slice()
        .iter()
        .find(|item| item.entity == entity)
        .unwrap()
        .world_center
}

#[derive(Component)]
struct MovingParent;

fn advance_parent(mut parents: Query<&mut Transform2d, With<MovingParent>>) {
    for mut transform in &mut parents {
        transform.translation = Vec2::new(10.0, 20.0);
        transform.rotation = std::f32::consts::FRAC_PI_2;
        transform.scale = Vec2::new(2.0, 1.0);
    }
}

#[test]
fn latest_parent_global_aligns_the_camera_sprite_and_tile_geometry() {
    let mut app = presentation_app();
    app.add_systems(
        CoreStage::FixedUpdate,
        advance_parent.in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    let world = app.world_mut().unwrap();
    let parent = world.spawn((MovingParent, Transform2d::default())).id();
    let camera_local = Transform2d {
        translation: Vec2::new(3.0, 4.0),
        rotation: 0.25,
        scale: Vec2::new(0.8, 1.2),
        ..Transform2d::IDENTITY
    };
    world.spawn((camera(), camera_local, ChildOf(parent)));
    let sprite = world
        .spawn((
            MovingSprite,
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::from_translation(Vec2::new(10.0, 1.0)),
            ChildOf(parent),
        ))
        .id();
    let mut tilemap = nara_render::tilemap::Tilemap::new(Vec2::ONE);
    tilemap.set_cell(
        nara_render::tilemap::TileCoord::new(0, 0),
        nara_render::tilemap::TileCell::new(nara_render::tilemap::TileIndex::new(0)),
    );
    let tile = world
        .spawn((tilemap, Transform2d::from_xy(-2.0, 0.0), ChildOf(parent)))
        .id();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime
        .drive(FixedTime::DEFAULT_TIMESTEP + FixedTime::DEFAULT_TIMESTEP / 2)
        .unwrap();
    let sprite_center = rendered_center(&runtime, sprite);
    let tile_center = rendered_center(&runtime, tile);
    let view = runtime
        .world()
        .resource::<nara_render::ExtractedViews>()
        .as_slice()[0];
    let camera_origin = view.mapping.clip_to_world(Vec2::ZERO);
    let camera_point = view.mapping.clip_to_world(Vec2::new(0.5, -0.5));
    let simulation = runtime
        .world()
        .get::<GlobalTransform2d>(sprite)
        .unwrap()
        .matrix();
    runtime.finish().unwrap();

    let parent_current = Mat3::from_scale_angle_translation(
        Vec2::new(2.0, 1.0),
        std::f32::consts::FRAC_PI_2,
        Vec2::new(10.0, 20.0),
    );
    let expected_camera = parent_current * camera_local.matrix();
    assert!(sprite_center.abs_diff_eq(
        parent_current.transform_point2(Vec2::new(18.0, 1.0)),
        1.0e-5
    ));
    assert!(tile_center.abs_diff_eq(
        parent_current.transform_point2(Vec2::new(-1.5, 0.5)),
        1.0e-5
    ));
    assert!(camera_origin.abs_diff_eq(expected_camera.transform_point2(Vec2::ZERO), 1.0e-5));
    assert!(camera_point.abs_diff_eq(
        expected_camera.transform_point2(Vec2::new(25.0, -25.0)),
        2.0e-5
    ));
    assert_eq!(simulation.transform_point2(Vec2::ZERO), sprite_center);
}

#[derive(Default, Resource)]
struct AuthoringEdit {
    pending: Option<Entity>,
    newborn: Option<Entity>,
}

fn apply_authoring_edit(world: &mut World) {
    let Some(entity) = world.resource_mut::<AuthoringEdit>().pending.take() else {
        return;
    };
    world.get_mut::<Transform2d>(entity).unwrap().translation = Vec2::new(40.0, 0.0);
    let newborn = world
        .spawn((
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::from_translation(Vec2::new(80.0, 0.0)),
        ))
        .id();
    world.resource_mut::<AuthoringEdit>().newborn = Some(newborn);
}

#[test]
fn resuming_after_a_paused_step_does_not_rewind_the_displayed_transform() {
    let (app, sprite) = motion_scene();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime
        .drive(FixedTime::DEFAULT_TIMESTEP + FixedTime::DEFAULT_TIMESTEP / 2)
        .unwrap();
    runtime.request_control(RuntimeControl::Pause);
    runtime.drive(Duration::ZERO).unwrap();
    runtime.request_control(RuntimeControl::StepFixedTick);
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let stepped = rendered_center(&runtime, sprite);
    runtime.request_control(RuntimeControl::Resume);
    runtime.drive(Duration::ZERO).unwrap();
    let resumed = rendered_center(&runtime, sprite);
    runtime.finish().unwrap();

    assert_eq!(stepped, Vec2::new(16.0, 0.0));
    assert_eq!(
        resumed, stepped,
        "resuming must not show an older fixed sample"
    );
}

#[test]
fn paused_step_and_authoring_changes_display_current_and_reset_new_entities() {
    let (mut app, sprite) = motion_scene();
    app.init_resource::<AuthoringEdit>().unwrap();
    app.add_systems(CoreStage::First, apply_authoring_edit)
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime
        .drive(FixedTime::DEFAULT_TIMESTEP + FixedTime::DEFAULT_TIMESTEP / 2)
        .unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    let paused = rendered_center(&runtime, sprite);
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let stepped = rendered_center(&runtime, sprite);
    let queued = runtime
        .apply_command(move |world: &mut World| {
            world.resource_mut::<AuthoringEdit>().pending = Some(sprite);
        })
        .unwrap();
    assert!(queued.post_operation_fault.is_none());
    runtime.drive(Duration::ZERO).unwrap();
    let edited = rendered_center(&runtime, sprite);
    let newborn = runtime.world().resource::<AuthoringEdit>().newborn.unwrap();
    let newborn_center = rendered_center(&runtime, newborn);
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    let resumed = rendered_center(&runtime, sprite);
    runtime.finish().unwrap();
    assert_eq!(paused, Vec2::new(8.0, 0.0));
    assert_eq!(stepped, Vec2::new(16.0, 0.0));
    assert_eq!(edited, Vec2::new(40.0, 0.0));
    assert_eq!(newborn_center, Vec2::new(80.0, 0.0));
    assert_eq!(resumed, edited);
}
