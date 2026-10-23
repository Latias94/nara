use nara_app::{
    __RuntimeDriverPort, App, CoreStage, FixedTime, FixedUpdateSet, RuntimeControl,
    RuntimeControlRequestResult, RuntimeInstance,
};
use nara_core::{Color, Mat3, Vec2};
use nara_ecs::{Component, Entity, Query, Resource, With, World, schedule::IntoScheduleConfigs};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_render::{Camera2d, RenderImage2d, RenderTarget, ViewportRect};
use nara_sprite::Sprite;
use nara_sprite_render::{__private::ExtractedSprites, SpriteRenderPlugin};
use nara_transform::{Transform2d, TransformPlugin};
use std::time::Duration;

#[derive(Component)]
struct MovingSprite;

fn advance_sprite(mut sprites: Query<&mut Transform2d, With<MovingSprite>>) {
    for mut transform in &mut sprites {
        transform.translation.x += 8.0;
    }
}

#[test]
fn fixed_motion_uses_half_tick_presentation_without_mutating_simulation() {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin::default(),
        nara_image::ImagePreparePlugin,
        SpriteRenderPlugin,
    ))
    .unwrap();
    app.insert_resource(nara_transform::FixedTransformInterpolation)
        .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        advance_sprite.in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    let world = app.world_mut().unwrap();
    let camera = world
        .spawn((
            Camera2d {
                target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
                viewport: Some(ViewportRect::new(0, 0, 100, 100).unwrap()),
                viewport_height: 100.0,
                ..Default::default()
            },
            Transform2d::default(),
        ))
        .id();
    let sprite = world
        .spawn((
            MovingSprite,
            Sprite::from_color(Vec2::new(2.0, 2.0), Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    HierarchyWriter::new(world)
        .apply_batch(&[camera, sprite].map(|entity| HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        }))
        .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    let step = FixedTime::DEFAULT_TIMESTEP;
    runtime.drive(step).unwrap();
    runtime.drive(step / 2).unwrap();
    let rendered = runtime
        .world()
        .resource::<ExtractedSprites>()
        .as_slice()
        .iter()
        .find(|item| item.entity == sprite)
        .unwrap()
        .world_center;
    runtime.drive(Duration::ZERO).unwrap();
    let rendered_again = rendered_center(&runtime, sprite);
    let simulation = runtime
        .world()
        .get::<Transform2d>(sprite)
        .unwrap()
        .translation;
    let simulation_global = nara_transform::historical_spatial_snapshot(runtime.world())
        .unwrap()
        .global(sprite)
        .unwrap()
        .translation();
    let tick = runtime.world().resource::<FixedTime>().tick();
    // Physically retire before the behavioral assertion so a red result remains readable.
    runtime.finish().unwrap();

    assert_eq!(tick, 1);
    assert_eq!(simulation, Vec2::new(8.0, 0.0));
    assert_eq!(simulation_global, simulation);
    assert!(
        rendered.abs_diff_eq(Vec2::new(4.0, 0.0), 0.000_01),
        "the actual extracted sprite should show the half-tick midpoint, observed {rendered:?}"
    );
    assert_eq!(
        rendered_again, rendered,
        "Extract must not rotate fixed history"
    );
}

fn presentation_app(interpolate: bool) -> App {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin::default(),
        nara_image::ImagePreparePlugin,
        SpriteRenderPlugin,
    ))
    .unwrap();
    if interpolate {
        app.insert_resource(nara_transform::FixedTransformInterpolation)
            .unwrap();
    }
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

fn motion_scene(interpolate: bool) -> (App, Entity) {
    let mut app = presentation_app(interpolate);
    let world = app.world_mut().unwrap();
    let camera = world.spawn((camera(), Transform2d::default())).id();
    let sprite = world
        .spawn((
            MovingSprite,
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::default(),
        ))
        .id();
    HierarchyWriter::new(world)
        .apply_batch(&[camera, sprite].map(|entity| HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        }))
        .unwrap();
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

#[test]
fn presentation_keeps_current_tick_by_default() {
    let (app, sprite) = motion_scene(false);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime
        .drive(FixedTime::DEFAULT_TIMESTEP + FixedTime::DEFAULT_TIMESTEP / 2)
        .unwrap();
    let rendered = rendered_center(&runtime, sprite);
    runtime.finish().unwrap();
    assert_eq!(rendered, Vec2::new(8.0, 0.0));
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
fn parent_local_interpolation_composes_one_affine_projection_for_camera_sprite_and_tile() {
    let mut app = presentation_app(true);
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
    };
    let camera = world.spawn((camera(), camera_local)).id();
    let sprite = world
        .spawn((
            MovingSprite,
            Sprite::from_color(Vec2::ONE, Color::WHITE),
            Transform2d::from_translation(Vec2::new(10.0, 1.0)),
        ))
        .id();
    let mut tilemap = nara_tilemap::Tilemap::new(Vec2::ONE);
    tilemap.set_cell(
        nara_tilemap::TileCoord::new(0, 0),
        nara_tilemap::TileCell::new(nara_tilemap::TileIndex::new(0)),
    );
    let tile = world
        .spawn((tilemap, Transform2d::from_translation(Vec2::new(-2.0, 0.0))))
        .id();
    let mut placements = vec![HierarchyOperation::Place {
        entity: parent,
        placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
    }];
    placements.extend(
        [camera, sprite, tile].map(|entity| HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
        }),
    );
    HierarchyWriter::new(world)
        .apply_batch(&placements)
        .unwrap();
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
    let simulation = nara_transform::historical_spatial_snapshot(runtime.world())
        .unwrap()
        .global(sprite)
        .unwrap()
        .matrix();
    runtime.finish().unwrap();

    let parent_half = Mat3::from_scale_angle_translation(
        Vec2::new(1.5, 1.0),
        std::f32::consts::FRAC_PI_4,
        Vec2::new(5.0, 10.0),
    );
    let expected_camera = parent_half * camera_local.matrix();
    assert!(
        sprite_center.abs_diff_eq(parent_half.transform_point2(Vec2::new(14.0, 1.0)), 0.000_01)
    );
    assert!(tile_center.abs_diff_eq(parent_half.transform_point2(Vec2::new(-1.5, 0.5)), 0.000_01));
    assert!(camera_origin.abs_diff_eq(expected_camera.transform_point2(Vec2::ZERO), 0.000_01));
    assert!(camera_point.abs_diff_eq(
        expected_camera.transform_point2(Vec2::new(25.0, -25.0)),
        0.000_02
    ));
    let parent_current = Mat3::from_scale_angle_translation(
        Vec2::new(2.0, 1.0),
        std::f32::consts::FRAC_PI_2,
        Vec2::new(10.0, 20.0),
    );
    assert!(simulation.transform_point2(Vec2::ZERO).abs_diff_eq(
        parent_current.transform_point2(Vec2::new(18.0, 1.0)),
        0.000_01
    ));
}

#[derive(Default, Resource)]
struct AuthoringEdit {
    pending: Option<Entity>,
    newborn: Option<Entity>,
}

impl __RuntimeDriverPort for AuthoringEdit {
    type Input = Entity;
    type Output = ();
    fn apply_driver_input(&mut self, entity: Entity) {
        self.pending = Some(entity);
    }
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
    HierarchyWriter::new(world)
        .apply_batch(&[HierarchyOperation::Place {
            entity: newborn,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        }])
        .unwrap();
    world.resource_mut::<AuthoringEdit>().newborn = Some(newborn);
}

#[test]
fn resuming_after_a_paused_step_does_not_rewind_the_displayed_transform() {
    let (app, sprite) = motion_scene(true);
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
    let (mut app, sprite) = motion_scene(true);
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
    runtime
        .with_driver_scope(|scope| scope.__apply_port::<AuthoringEdit>(sprite))
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
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
