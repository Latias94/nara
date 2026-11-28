#![cfg(feature = "runtime")]

use std::time::Duration;

use nara_app::{App, CoreStage};
use nara_core::Vec2;
use nara_ecs::{Commands, Entity, With};
use nara_render::{
    Camera2d, ExtractedView, ExtractedViews, RenderImage2d, RenderPlugin, RenderTarget,
    ViewportRect, WorldCanvas,
    camera::{CameraBounds2d, CameraRig2d, CameraRig2dPlugin, CameraShake2d},
};
use nara_scene::spatial::{ChildOf, SpatialPlugin, Transform2d};
use nara_window::{Window, WindowPlugin, WindowResolution};

fn app(window: bool) -> App {
    let mut app = App::new();
    app.add_plugins((SpatialPlugin, RenderPlugin::default(), CameraRig2dPlugin))
        .unwrap();
    if window {
        app.add_plugin(WindowPlugin::default()).unwrap();
    }
    app
}

fn image_camera(height: f32) -> Camera2d {
    Camera2d {
        target: RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
        viewport: ViewportRect::new(0, 0, 100, 100),
        viewport_height: height,
        ..Camera2d::default()
    }
}

fn views(app: &App) -> &[ExtractedView] {
    app.world().resource::<ExtractedViews>().as_slice()
}

fn view(app: &App, entity: Entity) -> &ExtractedView {
    views(app)
        .iter()
        .find(|view| view.camera_entity == entity)
        .unwrap()
}

fn center(app: &App, entity: Entity) -> Vec2 {
    view(app, entity).mapping.clip_to_world(Vec2::ZERO)
}

fn advance(app: &mut App, millis: u64) {
    app.update_with_delta(Duration::from_millis(millis))
        .unwrap();
}

fn resize(app: &mut App, window: Entity, width: u32, height: u32) {
    app.world_mut()
        .unwrap()
        .get_mut::<Window>(window)
        .unwrap()
        .resolution = WindowResolution::new(width, height);
    advance(app, 0);
}

#[test]
fn damping_uses_the_completed_target_and_the_same_result_at_different_frame_rates() {
    fn follow(steps: &[u64]) -> Vec2 {
        let mut app = app(false);
        let world = app.world_mut().unwrap();
        let parent = world.spawn(Transform2d::from_xy(90.0, 20.0)).id();
        let target = world.spawn(Transform2d::from_xy(10.0, 30.0)).id();
        let camera = world
            .spawn((
                image_camera(100.0),
                Transform2d::IDENTITY,
                CameraRig2d {
                    deadzone: Vec2::new(20.0, 10.0),
                    smooth_time: 0.2,
                    ..CameraRig2d::default()
                }
                .with_target(target),
            ))
            .id();
        app.add_systems(CoreStage::PostUpdate, move |mut commands: Commands| {
            commands.entity(target).insert(ChildOf(parent));
        })
        .unwrap();
        for &step in steps {
            advance(&mut app, step);
        }
        center(&app, camera)
    }

    let expected = Vec2::new(90.0, 45.0) * (1.0 - (-1.0_f32).exp());
    assert!(follow(&[200]).abs_diff_eq(expected, 1e-4));
    assert!(follow(&[50; 4]).abs_diff_eq(expected, 1e-4));
}

#[test]
fn deadzone_and_world_bounds_follow_the_current_window_geometry() {
    let mut app = app(true);
    let world = app.world_mut().unwrap();
    let window = world
        .query_filtered::<Entity, With<Window>>()
        .single(world)
        .unwrap();
    let target = world.spawn(Transform2d::from_xy(8.0, -6.0)).id();
    let camera = world
        .spawn((
            Camera2d {
                viewport_height: 60.0,
                ..Camera2d::default()
            },
            Transform2d::IDENTITY,
            CameraRig2d {
                deadzone: Vec2::new(20.0, 16.0),
                bounds: Some(CameraBounds2d::new(
                    Vec2::new(-60.0, -90.0),
                    Vec2::new(140.0, 90.0),
                )),
                ..CameraRig2d::default()
            }
            .with_target(target),
        ))
        .id();

    resize(&mut app, window, 100, 100);
    assert_eq!(center(&app, camera), Vec2::ZERO);
    app.world_mut()
        .unwrap()
        .entity_mut(target)
        .insert(Transform2d::from_xy(30.0, -15.0));
    advance(&mut app, 0);
    assert_eq!(center(&app, camera), Vec2::new(20.0, -7.0));

    app.world_mut()
        .unwrap()
        .entity_mut(target)
        .insert(Transform2d::from_xy(500.0, 500.0));
    advance(&mut app, 0);
    assert_eq!(center(&app, camera), Vec2::new(110.0, 60.0));
    assert_eq!(
        view(&app, camera).mapping.clip_to_world(Vec2::ONE),
        Vec2::new(140.0, 90.0)
    );

    resize(&mut app, window, 200, 100);
    assert_eq!(center(&app, camera), Vec2::new(80.0, 60.0));
    assert_eq!(
        view(&app, camera).mapping.clip_to_world(Vec2::ONE),
        Vec2::new(140.0, 90.0)
    );

    resize(&mut app, window, 500, 100);
    assert_eq!(center(&app, camera), Vec2::new(40.0, 60.0));
    resize(&mut app, window, 0, 0);
    assert!(views(&app).is_empty());
}

#[test]
fn bounds_and_pixel_snap_use_the_clipped_world_canvas_viewport() {
    let mut app = app(true);
    app.insert_resource(WorldCanvas::new(100, 100).unwrap())
        .unwrap();
    let world = app.world_mut().unwrap();
    let target = world.spawn(Transform2d::from_xy(200.0, 0.3)).id();
    let camera = world
        .spawn((
            Camera2d {
                viewport: ViewportRect::new(50, 0, 100, 100),
                viewport_height: 100.0,
                ..Camera2d::default()
            },
            Transform2d::IDENTITY,
            CameraRig2d {
                bounds: Some(CameraBounds2d::new(Vec2::splat(-100.0), Vec2::splat(100.3))),
                pixel_snap: Some(2.0),
                ..CameraRig2d::default()
            }
            .with_target(target),
        ))
        .id();
    advance(&mut app, 0);
    assert_eq!(
        view(&app, camera).viewport,
        ViewportRect::new(50, 0, 50, 100).unwrap()
    );
    assert_eq!(center(&app, camera), Vec2::new(75.0, 0.5));
    assert_eq!(view(&app, camera).mapping.clip_to_world(Vec2::X).x, 100.0);

    app.world_mut().unwrap().entity_mut(target).despawn();
    advance(&mut app, 0);
    assert_eq!(center(&app, camera), Vec2::new(75.0, 0.5));
    app.world_mut()
        .unwrap()
        .get_mut::<CameraRig2d>(camera)
        .unwrap()
        .pixel_snap = None;
    advance(&mut app, 0);
    assert!(center(&app, camera).abs_diff_eq(Vec2::new(75.3, 0.3), 1e-5));
}

#[test]
fn seeded_shake_decays_without_moving_the_tracking_center() {
    let mut app = app(false);
    let world = app.world_mut().unwrap();
    let anchor = Vec2::new(8.0, -4.0);
    let transform = Transform2d::from_translation(anchor);
    let target = world.spawn(transform).id();
    let rig = CameraRig2d {
        deadzone: Vec2::splat(2.0),
        ..CameraRig2d::default()
    }
    .with_target(target);
    let cameras = [37, 37, 38].map(|seed| {
        let mut shake = CameraShake2d {
            decay: 2.0,
            seed,
            ..CameraShake2d::default()
        };
        shake.add_trauma(f32::NAN);
        shake.add_trauma(-1.0);
        assert_eq!(shake.trauma, 0.0);
        shake.add_trauma(100.0);
        assert_eq!(shake.trauma, 1.0);
        world
            .spawn((image_camera(100.0), transform, rig, shake))
            .id()
    });
    let [first, second, other_seed] = cameras;
    advance(&mut app, 0);
    let shaken = view(&app, first).mapping;
    assert_eq!(shaken, view(&app, second).mapping);
    assert_ne!(shaken, view(&app, other_seed).mapping);
    assert_ne!(center(&app, first), anchor);
    let shaken_horizontal = shaken.clip_to_world(Vec2::X) - center(&app, first);
    assert!(shaken_horizontal.y.abs() > 1e-4);

    advance(&mut app, 125);
    assert_eq!(view(&app, first).mapping, view(&app, second).mapping);
    assert_eq!(
        app.world().get::<CameraShake2d>(first).unwrap().trauma,
        0.75
    );
    let remaining_offset = (center(&app, first) - anchor).abs();
    assert!(
        remaining_offset
            .cmple(Vec2::splat(6.0 * 0.75_f32.powi(2)))
            .all()
    );

    advance(&mut app, 250);
    advance(&mut app, 125);
    for camera in cameras {
        assert_eq!(center(&app, camera), anchor);
        assert_eq!(app.world().get::<Transform2d>(camera), Some(&transform));
        assert_eq!(
            app.world().get::<CameraShake2d>(camera).unwrap().trauma,
            0.0
        );
        assert_eq!(
            view(&app, camera).mapping.clip_to_world(Vec2::X),
            anchor + Vec2::X * 50.0
        );
    }
}

#[test]
fn invalid_effects_and_non_finite_targets_keep_the_camera_renderable() {
    let mut app = app(false);
    let world = app.world_mut().unwrap();
    let target = world.spawn(Transform2d::from_xy(f32::NAN, 4.0)).id();
    let camera = world
        .spawn((
            image_camera(100.0),
            Transform2d::IDENTITY,
            CameraRig2d {
                target: Some(target),
                deadzone: Vec2::splat(f32::NAN),
                smooth_time: f32::NEG_INFINITY,
                bounds: Some(CameraBounds2d::new(Vec2::splat(20.0), Vec2::ZERO)),
                pixel_snap: Some(f32::INFINITY),
            },
            CameraShake2d {
                trauma: 1.0,
                max_translation: Vec2::new(f32::INFINITY, f32::NAN),
                max_rotation: f32::NAN,
                ..CameraShake2d::default()
            },
        ))
        .id();
    advance(&mut app, 0);
    assert_eq!(center(&app, camera), Vec2::ZERO);
    app.world_mut()
        .unwrap()
        .entity_mut(target)
        .insert(Transform2d::from_xy(18.5, -3.25));
    advance(&mut app, 0);
    assert_eq!(center(&app, camera), Vec2::new(18.5, -3.25));
}

#[cfg(feature = "camera-scene")]
#[test]
fn authored_activity_and_inherited_visibility_are_independent() {
    use nara_core::Color;
    use nara_render::ViewLoadOp;
    use nara_scene::{Name, components::Camera2dSource, spatial::Visibility};

    let mut app = app(true);
    let world = app.world_mut().unwrap();
    let parent = world.spawn(Transform2d::from_xy(20.0, -5.0)).id();
    let default = world.spawn(Camera2dSource::default()).id();
    let color = Color::rgb(0.25, 0.5, 0.75);
    let authored = world
        .spawn((
            Camera2dSource {
                viewport_height: 180.0,
                clear_color: Some(color),
                order: -2,
                active: false,
            },
            Name::new("Alternate Camera"),
            Visibility::Inherited,
            ChildOf(parent),
        ))
        .id();
    advance(&mut app, 0);
    assert_eq!(views(&app).len(), 1);
    assert_eq!(views(&app)[0].camera_entity, default);
    assert_eq!(
        app.world().get::<Camera2d>(default),
        Some(&Camera2d::default())
    );
    assert_eq!(app.world().get::<Camera2d>(authored).unwrap().order, -2);
    assert_eq!(
        app.world().get::<Visibility>(authored),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        app.world().get::<Name>(authored).unwrap().as_str(),
        "Alternate Camera"
    );

    app.world_mut()
        .unwrap()
        .get_mut::<Camera2dSource>(authored)
        .unwrap()
        .active = true;
    advance(&mut app, 0);
    assert_eq!(views(&app)[0].camera_entity, authored);
    assert_eq!(views(&app)[0].load, ViewLoadOp::Clear(color));
    assert_eq!(views(&app)[1].load, ViewLoadOp::Load);
    assert_eq!(center(&app, authored), Vec2::new(20.0, -5.0));
    let id = view(&app, authored).id;

    app.world_mut()
        .unwrap()
        .entity_mut(parent)
        .insert(Visibility::Hidden);
    advance(&mut app, 0);
    assert_eq!(views(&app).len(), 1);
    app.world_mut()
        .unwrap()
        .entity_mut(authored)
        .insert(Visibility::Visible);
    advance(&mut app, 0);
    assert_eq!(view(&app, authored).id, id);

    let world = app.world_mut().unwrap();
    world.get_mut::<Camera2dSource>(authored).unwrap().active = false;
    world.get_mut::<Camera2d>(authored).unwrap().viewport_height = 95.0;
    advance(&mut app, 0);
    assert_eq!(views(&app).len(), 1);
    app.world_mut()
        .unwrap()
        .get_mut::<Camera2dSource>(authored)
        .unwrap()
        .active = true;
    advance(&mut app, 0);
    assert_eq!(view(&app, authored).id, id);
    assert_eq!(
        app.world()
            .get::<Camera2d>(authored)
            .unwrap()
            .viewport_height,
        95.0
    );

    app.world_mut()
        .unwrap()
        .entity_mut(authored)
        .remove::<Camera2dSource>();
    advance(&mut app, 0);
    assert_eq!(views(&app).len(), 1);
    assert!(app.world().get::<Camera2d>(authored).is_none());
    assert!(app.world().get::<Name>(authored).is_some());
    assert!(app.world().get::<ChildOf>(authored).is_some());
    assert_eq!(
        app.world().get::<Transform2d>(authored),
        Some(&Transform2d::IDENTITY)
    );
}
