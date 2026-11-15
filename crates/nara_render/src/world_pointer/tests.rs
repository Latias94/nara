use super::*;
use crate::{
    Color, ExtractedViews, RenderTarget, RenderViewId, ViewLoadOp, ViewMapping2d, ViewportRect,
    frame::build_render_frame_packet_for_source,
};
use nara_ecs::{World, bevy_ecs::system::SystemState};
use nara_window::WindowResolution;

const WORLD_TOLERANCE: f64 = 1e-5;
const PIXEL_TOLERANCE: f64 = 1e-3;
const VIEWPORT_HEIGHT: f32 = 8.0;

struct PointerFixture {
    world: World,
    window_entity: Entity,
    camera: Entity,
    packet: RenderFramePacket,
    pointer: SystemState<WorldPointer<'static, 'static>>,
}

impl PointerFixture {
    fn new(
        resolution: WindowResolution,
        viewport: ViewportRect,
        world_from_camera: Mat3,
        canvas: Option<WorldCanvas>,
    ) -> Self {
        let mut world = World::new();
        let window = Window {
            resolution,
            ..Default::default()
        };
        let window_entity = world.spawn(window.clone()).id();
        let camera = world.spawn_empty().id();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            id: RenderViewId::try_acquire().unwrap(),
            camera_entity: camera,
            target: RenderTarget::PrimaryWindow,
            viewport,
            mapping: ViewMapping2d::orthographic(world_from_camera, VIEWPORT_HEIGHT, viewport)
                .unwrap(),
            order: 0,
            load: ViewLoadOp::Clear(Color::BLACK),
        });
        let packet = build_render_frame_packet_for_source(
            RenderFrameSourceGeneration::try_acquire().unwrap(),
            1,
            Some(window.id),
            [&window],
            &views,
            None,
            canvas,
        )
        .unwrap()
        .unwrap();
        if let Some(canvas) = canvas {
            world.insert_resource(canvas);
        }
        let pointer = SystemState::new(&mut world);
        Self {
            world,
            window_entity,
            camera,
            packet,
            pointer,
        }
    }

    fn prepare(&mut self, mapping: WorldInputMapping2d) -> WorldPresentationGuard {
        let (presented, guard) = prepare_world_presentation(&self.packet, mapping).unwrap();
        self.world.insert_resource(presented);
        guard
    }

    fn pick(&mut self, logical_position: Vec2) -> Option<Vec2> {
        self.pointer
            .get(&self.world)
            .unwrap()
            .window_to_world(self.camera, logical_position)
    }
}

fn affine_matrix(rows: [[f64; 3]; 2]) -> Mat3 {
    Mat3::from_cols_array(&[
        rows[0][0] as f32,
        rows[1][0] as f32,
        0.0,
        rows[0][1] as f32,
        rows[1][1] as f32,
        0.0,
        rows[0][2] as f32,
        rows[1][2] as f32,
        1.0,
    ])
}

fn apply_affine(rows: [[f64; 3]; 2], point: [f64; 2]) -> [f64; 2] {
    rows.map(|row| row[0] * point[0] + row[1] * point[1] + row[2])
}

fn unapply_affine(rows: [[f64; 3]; 2], point: [f64; 2]) -> [f64; 2] {
    let x = point[0] - rows[0][2];
    let y = point[1] - rows[1][2];
    let determinant = rows[0][0] * rows[1][1] - rows[0][1] * rows[1][0];
    [
        (rows[1][1] * x - rows[0][1] * y) / determinant,
        (rows[0][0] * y - rows[1][0] * x) / determinant,
    ]
}

fn trs(scale: [f64; 2], angle: f64, translation: [f64; 2]) -> [[f64; 3]; 2] {
    let (sin, cos) = angle.sin_cos();
    [
        [cos * scale[0], -sin * scale[1], translation[0]],
        [sin * scale[0], cos * scale[1], translation[1]],
    ]
}

fn assert_near(actual: [f64; 2], expected: [f64; 2], tolerance: f64) {
    for axis in 0..2 {
        assert!(
            (actual[axis] - expected[axis]).abs() <= tolerance,
            "axis {axis}: {actual:?} != {expected:?}, tolerance {tolerance}"
        );
    }
}

#[test]
fn mapping_preserves_its_declared_matrix_and_accepts_small_invertible_determinants() {
    const IDENTITY: WorldInputMapping2d = WorldInputMapping2d::identity();
    assert_eq!(IDENTITY.final_to_source_physical(), Mat3::IDENTITY);
    assert_eq!(WorldInputMapping2d::affine(Mat3::IDENTITY), Some(IDENTITY));

    let mut matrix = affine_matrix([[-2.0, 0.25, 37.0], [0.5, 1.5, -19.0]]);
    let mapping = WorldInputMapping2d::affine(matrix).unwrap();
    let copied = mapping;
    matrix.z_axis.x = 91.0;
    assert_eq!(copied, mapping);
    assert_eq!(mapping.final_to_source_physical().z_axis.x, 37.0);
    assert_ne!(mapping.final_to_source_physical(), matrix);
    assert!(WorldInputMapping2d::affine(Mat3::from_scale(Vec2::new(1e-12, 1.0))).is_some());
}

#[test]
fn mapping_rejects_singular_projective_nonfinite_and_nonfinite_inverse_matrices() {
    for matrix in [
        Mat3::ZERO,
        affine_matrix([[1.0, 2.0, 3.0], [2.0, 4.0, 5.0]]),
        Mat3::from_scale(Vec2::new(f32::from_bits(1), 1.0)),
        affine_matrix([[1e-20, 0.0, 1e20], [0.0, 1.0, 0.0]]),
        Mat3::from_scale(Vec2::splat(f32::MAX)),
    ] {
        assert!(WorldInputMapping2d::affine(matrix).is_none(), "{matrix:?}");
    }
    for (index, value) in [
        (2, f32::MIN_POSITIVE),
        (5, -f32::MIN_POSITIVE),
        (8, 0.0),
        (8, 2.0),
        (8, f32::from_bits(1.0_f32.to_bits() + 1)),
    ] {
        let mut values = Mat3::IDENTITY.to_cols_array();
        values[index] = value;
        assert!(WorldInputMapping2d::affine(Mat3::from_cols_array(&values)).is_none());
    }
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for index in 0..9 {
            let mut values = Mat3::IDENTITY.to_cols_array();
            values[index] = value;
            assert!(WorldInputMapping2d::affine(Mat3::from_cols_array(&values)).is_none());
        }
    }
}

#[test]
fn identity_mapping_returns_original_camera_coordinates_at_all_dpi() {
    for scale_factor in [1.0, 1.5, 2.0] {
        let mut fixture = PointerFixture::new(
            WindowResolution::new(800, 600).with_scale_factor(scale_factor),
            ViewportRect::new(96, 72, 480, 360).unwrap(),
            Mat3::from_translation(Vec2::new(3.0, -2.0)),
            None,
        );
        let guard = fixture.prepare(WorldInputMapping2d::identity());
        guard.mark_presented();
        for (physical, expected) in [
            ([336.0, 252.0], [3.0, -2.0]),
            ([246.0, 342.0], [1.0, -4.0]),
            ([471.0, 162.0], [6.0, 0.0]),
        ] {
            let logical = Vec2::new(physical[0], physical[1]) / scale_factor as f32;
            let actual = fixture.pick(logical).unwrap().to_array().map(f64::from);
            assert_near(actual, expected, WORLD_TOLERANCE);
        }
    }
}

#[test]
fn affine_picking_matches_independent_world_and_pixel_oracles_at_all_dpi_and_mirror_axes() {
    let viewport = ViewportRect::new(96, 72, 480, 360).unwrap();
    for scale_factor in [1.0, 1.5, 2.0] {
        for camera_x in [-1.0, 1.0] {
            for camera_y in [-1.0, 1.0] {
                let parent = trs([camera_x * 1.25, camera_y * 0.75], 0.375, [3.0, -2.0]);
                let child = trs([0.5, 1.5], -0.625, [0.75, -0.5]);
                let world_from_camera = Mat3::from_scale_angle_translation(
                    Vec2::new(camera_x as f32 * 1.25, camera_y as f32 * 0.75),
                    0.375,
                    Vec2::new(3.0, -2.0),
                ) * Mat3::from_scale_angle_translation(
                    Vec2::new(0.5, 1.5),
                    -0.625,
                    Vec2::new(0.75, -0.5),
                );
                for output_x in [-1.0, 1.0] {
                    for output_y in [-1.0, 1.0] {
                        let rows = [
                            [output_x, 0.125, 408.0 - output_x * 400.0 - 37.5],
                            [-0.0625, output_y, 313.0 - output_y * 300.0],
                        ];
                        let mut fixture = PointerFixture::new(
                            WindowResolution::new(800, 600).with_scale_factor(scale_factor),
                            viewport,
                            world_from_camera,
                            None,
                        );
                        let guard = fixture
                            .prepare(WorldInputMapping2d::affine(affine_matrix(rows)).unwrap());
                        guard.mark_presented();
                        for final_pixel in [[320.5, 220.5], [400.5, 300.5], [460.5, 348.5]] {
                            let source = apply_affine(rows, final_pixel);
                            let camera_point =
                                [(source[0] - 336.0) / 45.0, (252.0 - source[1]) / 45.0];
                            let expected = apply_affine(parent, apply_affine(child, camera_point));
                            let logical = Vec2::new(
                                (final_pixel[0] / scale_factor) as f32,
                                (final_pixel[1] / scale_factor) as f32,
                            );
                            let world = fixture.pick(logical).unwrap().to_array().map(f64::from);
                            assert_near(world, expected, WORLD_TOLERANCE);
                            let camera_point = unapply_affine(child, unapply_affine(parent, world));
                            let source = [
                                336.0 + camera_point[0] * 45.0,
                                252.0 - camera_point[1] * 45.0,
                            ];
                            assert_near(unapply_affine(rows, source), final_pixel, PIXEL_TOLERANCE);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn final_target_bounds_are_checked_before_an_affine_can_move_points_inside() {
    let mut fixture = PointerFixture::new(
        WindowResolution::new(800, 600).with_scale_factor(2.0),
        ViewportRect::new(0, 0, 800, 600).unwrap(),
        Mat3::IDENTITY,
        None,
    );
    let guard = fixture.prepare(
        WorldInputMapping2d::affine(affine_matrix([[0.5, 0.0, 100.0], [0.0, 0.5, 100.0]])).unwrap(),
    );
    guard.mark_presented();
    for final_pixel in [
        Vec2::new(-0.5, 100.0),
        Vec2::new(100.0, -0.5),
        Vec2::new(800.0, 100.0),
        Vec2::new(100.0, 600.0),
        Vec2::new(f32::NAN, 100.0),
        Vec2::new(100.0, f32::NAN),
        Vec2::new(f32::INFINITY, 100.0),
        Vec2::new(100.0, f32::NEG_INFINITY),
    ] {
        assert!(fixture.pick(final_pixel / 2.0).is_none(), "{final_pixel:?}");
    }
    assert!(fixture.pick(Vec2::splat(f32::MAX)).is_none());
    assert!(fixture.pick(Vec2::ZERO).is_some());
    assert!(fixture.pick(Vec2::new(799.5, 599.5) / 2.0).is_some());
}

#[test]
fn source_target_bounds_are_half_open_after_mapping() {
    let mut fixture = PointerFixture::new(
        WindowResolution::new(800, 600).with_scale_factor(2.0),
        ViewportRect::new(0, 0, 800, 600).unwrap(),
        Mat3::IDENTITY,
        None,
    );
    let rows = [[2.0, 0.0, -100.0], [0.0, 2.0, -100.0]];
    let guard = fixture.prepare(WorldInputMapping2d::affine(affine_matrix(rows)).unwrap());
    guard.mark_presented();
    for source in [[-0.5, 100.0], [100.0, -0.5], [800.0, 100.0], [100.0, 600.0]] {
        let final_pixel = unapply_affine(rows, source);
        let logical = Vec2::new(final_pixel[0] as f32, final_pixel[1] as f32) / 2.0;
        assert!(fixture.pick(logical).is_none(), "source {source:?}");
    }
    for source in [[0.0, 0.0], [799.5, 599.5]] {
        let final_pixel = unapply_affine(rows, source);
        let logical = Vec2::new(final_pixel[0] as f32, final_pixel[1] as f32) / 2.0;
        assert!(fixture.pick(logical).is_some(), "source {source:?}");
    }
}

#[test]
fn source_viewport_bounds_are_checked_after_mapping() {
    let mut fixture = PointerFixture::new(
        WindowResolution::new(800, 600).with_scale_factor(1.5),
        ViewportRect::new(96, 72, 480, 360).unwrap(),
        Mat3::IDENTITY,
        None,
    );
    let rows = [[1.0, 0.0, 30.0], [0.0, 1.0, 15.0]];
    let guard = fixture.prepare(WorldInputMapping2d::affine(affine_matrix(rows)).unwrap());
    guard.mark_presented();
    for (source, inside) in [
        ([96.0, 72.0], true),
        ([575.5, 431.5], true),
        ([95.5, 100.0], false),
        ([100.0, 71.5], false),
        ([576.0, 100.0], false),
        ([100.0, 432.0], false),
    ] {
        let final_pixel = unapply_affine(rows, source);
        let logical = Vec2::new(final_pixel[0] as f32, final_pixel[1] as f32) / 1.5;
        assert_eq!(fixture.pick(logical).is_some(), inside, "source {source:?}");
    }
}

#[test]
fn finite_mapping_rejects_nonfinite_transformed_points() {
    let mut fixture = PointerFixture::new(
        WindowResolution::new(800, 600),
        ViewportRect::new(0, 0, 800, 600).unwrap(),
        Mat3::IDENTITY,
        None,
    );
    let guard = fixture
        .prepare(WorldInputMapping2d::affine(Mat3::from_scale(Vec2::new(f32::MAX, 1.0))).unwrap());
    guard.mark_presented();
    assert!(fixture.pick(Vec2::new(400.0, 300.0)).is_none());
}

#[test]
fn mapped_canvas_uses_original_physical_fit_before_offset_camera_conversion_at_all_dpi() {
    let canvas = WorldCanvas::new(320, 180).unwrap();
    let rows = [[1.0, 0.0, 90.0], [0.0, 1.0, -45.0]];
    for scale_factor in [1.0, 1.5, 2.0] {
        let mut fixture = PointerFixture::new(
            WindowResolution::new(1_000, 700).with_scale_factor(scale_factor),
            ViewportRect::new(32, 18, 240, 144).unwrap(),
            Mat3::IDENTITY,
            Some(canvas),
        );
        let guard = fixture.prepare(WorldInputMapping2d::affine(affine_matrix(rows)).unwrap());
        guard.mark_presented();
        for final_pixel in [[200.5, 300.5], [386.5, 395.5], [600.5, 500.5]] {
            let source = apply_affine(rows, final_pixel);
            let canvas_pixel = [(source[0] - 20.0) / 3.0, (source[1] - 80.0) / 3.0];
            let expected = [
                (canvas_pixel[0] - 152.0) / 18.0,
                (90.0 - canvas_pixel[1]) / 18.0,
            ];
            let logical = Vec2::new(
                (final_pixel[0] / scale_factor) as f32,
                (final_pixel[1] / scale_factor) as f32,
            );
            let world = fixture.pick(logical).unwrap().to_array().map(f64::from);
            assert_near(world, expected, WORLD_TOLERANCE);
            let source = [
                20.0 + (152.0 + world[0] * 18.0) * 3.0,
                80.0 + (90.0 - world[1] * 18.0) * 3.0,
            ];
            assert_near(unapply_affine(rows, source), final_pixel, PIXEL_TOLERANCE);
        }
    }
}

#[test]
fn canvas_bars_and_maximum_edges_are_rejected_in_source_coordinates() {
    let canvas = WorldCanvas::new(320, 180).unwrap();
    for scale_factor in [1.0, 1.5, 2.0] {
        let mut fixture = PointerFixture::new(
            WindowResolution::new(1_000, 700).with_scale_factor(scale_factor),
            ViewportRect::from_extent(canvas.extent()).unwrap(),
            Mat3::IDENTITY,
            Some(canvas),
        );
        let guard = fixture.prepare(
            WorldInputMapping2d::affine(affine_matrix([[1.0, 0.0, 90.0], [0.0, 1.0, -45.0]]))
                .unwrap(),
        );
        guard.mark_presented();
        for (final_pixel, inside) in [
            ([10.0, 350.0], true),
            ([500.0, 650.0], true),
            ([410.0, 125.0], true),
            ([410.0, 124.5], false),
            ([890.0, 395.0], false),
            ([410.0, 665.0], false),
        ] {
            let logical = Vec2::new(final_pixel[0], final_pixel[1]) / scale_factor as f32;
            assert_eq!(fixture.pick(logical).is_some(), inside, "{final_pixel:?}");
        }
    }
}

#[test]
fn captured_mapping_requires_actual_present_and_dies_with_its_guard() {
    let mut fixture = PointerFixture::new(
        WindowResolution::new(800, 600).with_scale_factor(1.5),
        ViewportRect::new(96, 72, 480, 360).unwrap(),
        Mat3::IDENTITY,
        None,
    );
    let rows = [[1.0, 0.0, 40.0], [0.0, 1.0, -30.0]];
    let mut matrix = affine_matrix(rows);
    let mapping = WorldInputMapping2d::affine(matrix).unwrap();
    let guard = fixture.prepare(mapping);
    matrix.z_axis.x = -100.0;
    let logical = Vec2::new(300.0, 250.0) / 1.5;
    assert!(fixture.pick(logical).is_none());
    guard.mark_presented();
    let expected = [4.0 / 45.0, 32.0 / 45.0];
    assert_near(
        fixture.pick(logical).unwrap().to_array().map(f64::from),
        expected,
        WORLD_TOLERANCE,
    );
    let (_, unpresented) = prepare_world_presentation(
        &fixture.packet,
        WorldInputMapping2d::affine(matrix).unwrap(),
    )
    .unwrap();
    drop(unpresented);
    assert_near(
        fixture.pick(logical).unwrap().to_array().map(f64::from),
        expected,
        WORLD_TOLERANCE,
    );
    drop(guard);
    assert!(fixture.pick(logical).is_none());
}

#[test]
fn mapped_frames_stay_revoked_after_native_geometry_returns_to_its_original_value() {
    let original = WindowResolution::new(800, 600).with_scale_factor(1.5);
    for changed in [
        WindowResolution::new(801, 600).with_scale_factor(1.5),
        WindowResolution::new(800, 601).with_scale_factor(1.5),
        WindowResolution::new(800, 600).with_scale_factor(2.0),
        WindowResolution::new(0, 0).with_scale_factor(1.5),
    ] {
        let mut fixture = PointerFixture::new(
            original,
            ViewportRect::new(96, 72, 480, 360).unwrap(),
            Mat3::IDENTITY,
            None,
        );
        let mapping =
            WorldInputMapping2d::affine(affine_matrix([[-1.0, 0.125, 770.0], [0.0, 1.0, -15.0]]))
                .unwrap();
        let guard = fixture.prepare(mapping);
        let revoker = guard.revoker();
        guard.mark_presented();
        let logical = Vec2::new(400.0, 300.0) / 1.5;
        assert!(fixture.pick(logical).is_some());
        fixture
            .world
            .get_mut::<Window>(fixture.window_entity)
            .unwrap()
            .resolution = changed;
        assert!(fixture.pick(logical).is_none());
        fixture
            .world
            .get_mut::<Window>(fixture.window_entity)
            .unwrap()
            .resolution = original;
        guard.mark_presented();
        assert!(fixture.pick(logical).is_none());
        let replacement = fixture.prepare(mapping);
        assert!(fixture.pick(logical).is_none());
        replacement.mark_presented();
        revoker.invalidate();
        drop(guard);
        assert!(fixture.pick(logical).is_some());
    }
}
