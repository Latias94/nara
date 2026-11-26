use nara_app::{App, RuntimeInstance};
use nara_ecs::bevy_ecs::system::{RunSystemOnce, SystemState};
use nara_window::WindowResolution;

use super::*;
use crate::{
    Color, ExtractedViews, RenderTarget, ViewLoadOp, ViewMapping2d, ViewportRect,
    build_render_frame_packet_with_world_canvas,
    runtime_integration::build_window_overlay_frame_packet,
};

struct PointerFixture {
    world: World,
    window: Window,
    camera: Entity,
    id: RenderViewId,
    frame: RenderFramePacket,
    pointer: SystemState<WorldPointer<'static, 'static>>,
}

impl PointerFixture {
    fn new(canvas: Option<WorldCanvas>, viewports: &[ViewportRect]) -> Self {
        let runtime = RuntimeInstance::start(App::new()).unwrap();
        let generation = runtime.generation();
        runtime.finish().unwrap();
        let mut world = World::new();
        let window = Window {
            resolution: WindowResolution::new(1000, 650).with_scale_factor(2.0),
            ..Window::default()
        };
        world.spawn(window.clone());
        if let Some(canvas) = canvas {
            world.insert_resource(canvas);
        }
        let camera = world.spawn_empty().id();
        let id = RenderViewId::try_acquire().unwrap();
        let mut views = ExtractedViews::default();
        for (index, &viewport) in viewports.iter().enumerate() {
            views.push(ExtractedView {
                id: id.subview(index as u32),
                camera_entity: camera,
                target: RenderTarget::PrimaryWindow,
                viewport,
                mapping: ViewMapping2d::orthographic(
                    Mat3::from_translation(Vec2::new(30.0, -5.0)),
                    viewport.physical_height as f32,
                    viewport,
                )
                .unwrap(),
                order: index as i32,
                load: if index == 0 {
                    ViewLoadOp::Clear(Color::BLACK)
                } else {
                    ViewLoadOp::Load
                },
            });
        }
        let frame = build_render_frame_packet_with_world_canvas(
            generation,
            1,
            Some(window.id),
            [&window],
            &views,
            canvas,
        )
        .unwrap()
        .unwrap();
        let pointer = SystemState::new(&mut world);
        Self {
            world,
            window,
            camera,
            id,
            frame,
            pointer,
        }
    }

    fn present(&mut self, mapping: Option<WorldInputMapping2d>) {
        publish_presented_world_frame(&mut self.world, &self.frame, mapping);
    }

    fn pick(&mut self, point: Vec2) -> Option<Vec2> {
        self.pointer
            .get(&self.world)
            .unwrap()
            .window_to_world(self.camera, point)
    }
}

#[test]
fn newest_presented_views_control_picking_and_unmapped_output_clears_it() {
    let viewport = ViewportRect::new(0, 0, 1000, 650).unwrap();
    let mut fixture = PointerFixture::new(None, &[viewport]);
    let center = Vec2::new(250.0, 162.5);
    assert_eq!(fixture.pick(center), None);
    fixture.present(Some(WorldInputMapping2d::identity()));
    assert_eq!(fixture.pick(center), Some(Vec2::new(30.0, -5.0)));
    assert_eq!(fixture.pick(Vec2::new(500.0, 162.5)), None);

    let generation = fixture
        .frame
        .source_generation()
        .runtime_generation()
        .unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        mapping: ViewMapping2d::orthographic(
            Mat3::from_translation(Vec2::new(12.0, 25.0)),
            650.0,
            viewport,
        )
        .unwrap(),
        ..fixture.frame.views()[0]
    });
    let next = build_render_frame_packet_with_world_canvas(
        generation,
        2,
        None,
        [&fixture.window],
        &views,
        None,
    )
    .unwrap()
    .unwrap();
    assert_eq!(fixture.pick(center), Some(Vec2::new(30.0, -5.0)));
    publish_presented_world_frame(
        &mut fixture.world,
        &next,
        Some(WorldInputMapping2d::identity()),
    );
    assert!(
        fixture
            .pick(center)
            .unwrap()
            .abs_diff_eq(Vec2::new(12.0, 25.0), 1e-4)
    );

    fixture.present(None);
    assert_eq!(fixture.pick(center), None);
    fixture.present(Some(WorldInputMapping2d::identity()));
    let RenderFrameTarget::Window(window) = fixture.frame.target() else {
        unreachable!()
    };
    let overlay = build_window_overlay_frame_packet(generation, 3, window, None).unwrap();
    publish_presented_world_frame(
        &mut fixture.world,
        &overlay,
        Some(WorldInputMapping2d::identity()),
    );
    assert_eq!(fixture.pick(center), None);
    fixture.present(Some(WorldInputMapping2d::identity()));
    clear_presented_world_frame(&mut fixture.world);
    assert_eq!(fixture.pick(center), None);
}

#[test]
fn hidpi_canvas_picking_applies_policy_then_canvas_fit_and_viewport() {
    let canvas = WorldCanvas::new(320, 180).unwrap();
    let viewport = ViewportRect::new(40, 20, 160, 120).unwrap();
    let mut fixture = PointerFixture::new(Some(canvas), &[viewport]);
    let mapping =
        WorldInputMapping2d::affine(Mat3::from_translation(Vec2::new(80.0, 0.0))).unwrap();
    fixture.present(Some(mapping));

    let center = Vec2::new(150.0, 147.5);
    assert!(
        fixture
            .pick(center)
            .unwrap()
            .abs_diff_eq(Vec2::new(30.0, -5.0), 1e-4)
    );
    assert_eq!(fixture.pick(Vec2::new(150.0, 15.0)), None);
    assert_eq!(fixture.pick(Vec2::new(15.0, 147.5)), None);
    assert_eq!(fixture.pick(Vec2::new(-1.0, 147.5)), None);
    assert_eq!(fixture.pick(Vec2::new(479.0, 147.5)), None);
    assert_eq!(fixture.pick(Vec2::new(f32::NAN, 147.5)), None);
}

#[test]
fn letterboxed_canvas_downscales_to_keep_the_whole_raster_visible() {
    let canvas = WorldCanvas::new(1600, 900).unwrap();
    let viewport = ViewportRect::from_extent(canvas.extent()).unwrap();
    let mut fixture = PointerFixture::new(Some(canvas), &[viewport]);
    fixture.present(Some(WorldInputMapping2d::identity()));
    let displayed = fixture
        .frame
        .world_canvas()
        .unwrap()
        .mapping()
        .content_rect();
    assert_eq!(displayed.min, [0.0, 21.875]);
    assert_eq!(displayed.max, [500.0, 303.125]);
    assert!(fixture.pick(Vec2::new(499.0, 302.0)).is_some());
    assert_eq!(fixture.pick(Vec2::new(250.0, displayed.max[1])), None);
}

#[test]
fn picking_resolves_subviews_and_excludes_shared_maximum_edges() {
    let left = ViewportRect::new(0, 0, 500, 650).unwrap();
    let right = ViewportRect::new(500, 0, 500, 650).unwrap();
    for viewports in [[left, right], [right, left]] {
        let mut fixture = PointerFixture::new(None, &viewports);
        fixture.present(Some(WorldInputMapping2d::identity()));
        let right_center = Vec2::new(375.0, 162.5);
        let seam = Vec2::new(250.0, 162.5);
        assert_eq!(fixture.pick(right_center), Some(Vec2::new(30.0, -5.0)));
        let pointer = fixture.pointer.get(&fixture.world).unwrap();
        let left_index = u32::from(viewports[0] == right);
        let left_id = fixture.id.subview(left_index);
        let right_id = fixture.id.subview(1 - left_index);
        assert_eq!(pointer.window_to_world_in_view(left_id, seam), None);
        assert!(pointer.window_to_world_in_view(right_id, seam).is_some());
    }
}

#[test]
fn window_event_round_trip_retires_the_mapping_until_a_new_present() {
    let viewport = ViewportRect::new(0, 0, 1000, 650).unwrap();
    let mut fixture = PointerFixture::new(None, &[viewport]);
    let center = Vec2::new(250.0, 162.5);
    fixture.present(Some(WorldInputMapping2d::identity()));
    assert!(fixture.pick(center).is_some());

    let mut events = WindowEvents::default();
    for resolution in [WindowResolution::new(0, 0), fixture.window.resolution] {
        events.push(WindowEvent::Resized {
            window_id: fixture.window.id,
            resolution,
        });
    }
    fixture.world.insert_resource(events);
    fixture
        .world
        .run_system_once(invalidate_window_presentation)
        .unwrap();
    fixture.world.remove_resource::<WindowEvents>();
    assert_eq!(fixture.pick(center), None);
    fixture.present(Some(WorldInputMapping2d::identity()));
    assert!(fixture.pick(center).is_some());

    fixture
        .world
        .insert_resource(WorldCanvas::new(320, 180).unwrap());
    assert_eq!(fixture.pick(center), None);
    fixture
        .world
        .run_system_once(invalidate_window_presentation)
        .unwrap();
    fixture.world.remove_resource::<WorldCanvas>();
    assert_eq!(fixture.pick(center), None);
}

#[test]
fn affine_input_mapping_rejects_invalid_inverses() {
    let singular = Mat3::from_scale(Vec2::new(0.0, 1.0));
    let perspective = Mat3::from_cols_array(&[1.0, 0.0, 0.2, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    let overflow = Mat3::from_scale(Vec2::splat(f32::MAX));
    let nonfinite = Mat3::from_translation(Vec2::new(f32::NAN, 0.0));
    for matrix in [singular, perspective, overflow, nonfinite] {
        assert!(WorldInputMapping2d::affine(matrix).is_none());
    }
    let small = Mat3::from_scale(Vec2::splat(1e-8));
    assert_eq!(
        WorldInputMapping2d::affine(small)
            .unwrap()
            .final_to_source_physical(),
        small
    );
}
