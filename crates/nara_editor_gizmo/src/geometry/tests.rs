use nara_core::{Color, Mat3};
use nara_ecs::Entity;
use nara_render::{RenderTarget, RenderViewId, ViewLoadOp, ViewMapping2d, ViewportRect};

use super::*;

fn view(extent: Extent2d, camera: Mat3, height: f32) -> ExtractedView {
    let viewport = ViewportRect::from_extent(extent).unwrap();
    ExtractedView {
        id: RenderViewId::try_acquire().unwrap(),
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::PrimaryWindow,
        viewport,
        mapping: ViewMapping2d::orthographic(camera, height, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    }
}

#[test]
fn adaptive_grid_bounds_work_at_very_low_zoom() {
    let extent = Extent2d::new(4096, 3072).unwrap();
    let view = view(extent, Mat3::from_angle(0.4), 40_000.0);
    let projection = ProjectedView::new(&view, None, extent).unwrap();
    let frame = OverlayFrame {
        grid: Some(OverlayGrid {
            spacing: 0.00001,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut instances = Vec::new();
    projection.collect(&frame, &mut instances);
    assert!(instances.len() > 2);
    assert!(instances.len() <= 2 * MAX_GRID_LINES_PER_AXIS + 2);
    assert!(instances.iter().all(|instance| instance.kind == LINE));
}

#[test]
fn projection_matches_the_extracted_view_at_large_world_coordinates() {
    let extent = Extent2d::new(101, 63).unwrap();
    let center = Vec2::new(987_654.25, -2_345_678.0);
    let camera = Mat3::from_translation(center) * Mat3::from_angle(-0.37);
    let view = view(extent, camera, 7.0);
    let projection = ProjectedView::new(&view, None, extent).unwrap();
    let point = center + Vec2::new(1.25, -0.5);
    assert_eq!(
        projection.project_point(point),
        view.world_to_viewport(point),
    );
}

#[test]
fn valid_shapes_preserve_order_and_premultiplied_color() {
    let extent = Extent2d::new(64, 64).unwrap();
    let view = view(extent, Mat3::IDENTITY, 64.0);
    let projection = ProjectedView::new(&view, None, extent).unwrap();
    let stroke = OverlayStroke::new(Color::rgba(2.0, 0.5, -1.0, 0.5), 2.0);
    let frame = OverlayFrame {
        primitives: vec![
            OverlayPrimitive::rectangle(Vec2::splat(90.0), Vec2::splat(100.0), stroke),
            OverlayPrimitive::rectangle(Vec2::splat(-10.0), Vec2::splat(10.0), stroke),
            OverlayPrimitive::Circle {
                center: Vec2::ZERO,
                radius: f32::NAN,
                stroke,
            },
            OverlayPrimitive::Circle {
                center: Vec2::ZERO,
                radius: 4.0,
                stroke,
            },
            OverlayPrimitive::Line {
                from: Vec2::ZERO,
                to: Vec2::ZERO,
                stroke,
            },
        ],
        ..Default::default()
    };
    let mut instances = Vec::new();
    projection.collect(&frame, &mut instances);
    assert_eq!(
        instances
            .iter()
            .map(|instance| instance.kind)
            .collect::<Vec<_>>(),
        [RECT, CIRCLE, LINE]
    );
    assert!(
        instances
            .iter()
            .all(|instance| instance.premultiplied == [0.5, 0.25, 0.0, 0.5])
    );
}
