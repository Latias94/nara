use std::mem::{align_of, size_of};

use nara_ui_kernel::{UiMesh, UiMeshVertex};

use super::*;
use crate::{UiPacketMeshVertex, UiPacketRect, UiPresentationLimitsError};

fn triangle() -> UiMesh {
    mesh_at([[0.375, 0.625], [35.25, 0.625], [0.375, 25.5]])
}

fn mesh_at(positions: [[f32; 2]; 3]) -> UiMesh {
    let colors = [
        UiColor::rgba(1.0, 0.0, 0.0, 0.25),
        UiColor::rgba(0.0, 1.0, 0.0, 0.5),
        UiColor::rgba(0.0, 0.0, 1.0, 1.0),
    ];
    UiMesh::new(
        positions
            .into_iter()
            .zip(colors)
            .map(|([x, y], color)| UiMeshVertex::new(UiPoint::new(x, y), color))
            .collect(),
        vec![0, 1, 2],
    )
    .unwrap()
}

fn mesh_node(key: &str, origin_x: f32, mesh: UiMesh) -> UiNode {
    UiNode::new(UiStyle::absolute(origin_x, 6.0, 48.0, 32.0))
        .with_key(UiLocalKey::runtime("packet-mesh", key).unwrap())
        .with_mesh(mesh)
}

fn letterboxed_environment() -> UiEnvironment {
    let transform = UiSurfaceContentTransform::new(
        UiSize::new(100.0, 80.0).unwrap(),
        UiSize::new(301.0, 205.0).unwrap(),
        UiSafeAreaInsets::new(11.0, 7.0, 21.0, 13.0).unwrap(),
        1.25,
    )
    .unwrap();
    UiEnvironment::for_surface(
        UiSurfaceId::new(81).unwrap(),
        transform,
        UiSurfaceGenerations::new(2, 3, 4, 5).unwrap(),
        1,
        1,
    )
    .unwrap()
}

fn map_position(environment: UiEnvironment, content: [f32; 2], local: [f32; 2]) -> [f32; 2] {
    let transform = environment.content_transform();
    let origin = transform.physical_content_origin();
    let scale = transform.content_scale_factor();
    [
        origin.x() + (content[0] + local[0]) * scale,
        origin.y() + (content[1] + local[1]) * scale,
    ]
}

fn mesh_bytes(vertices: usize, indices: usize) -> usize {
    let header = 2 * size_of::<usize>();
    let vertex_alignment = align_of::<usize>().max(align_of::<UiPacketMeshVertex>());
    let index_alignment = align_of::<usize>().max(align_of::<u32>());
    size_of::<UiPacketPrimitive>()
        + (header + vertices * size_of::<UiPacketMeshVertex>()).next_multiple_of(vertex_alignment)
        + (header + indices * size_of::<u32>()).next_multiple_of(index_alignment)
}

#[test]
fn fractional_mesh_positions_use_original_content_origin_and_scale_with_safe_area() {
    let environment = letterboxed_environment();
    let source_mesh = triangle();
    let node = UiNode::new(
        UiStyle::absolute(10.25, 12.5, 40.5, 30.75)
            .with_padding(UiEdges::symmetric(2.25, 1.5).unwrap()),
    )
    .with_mesh(source_mesh.clone());
    let candidate = frame_with_environment(1, vec![node], environment);
    assert!(candidate.diagnostics().is_empty());
    let UiPaintPrimitive::Mesh { rect, .. } = &candidate.paint()[0] else {
        panic!("the node must retain its authored mesh")
    };
    assert_eq!(
        *rect,
        UiRect::from_origin_size(12.5, 14.0, 36.0, 27.75).unwrap()
    );
    let snapped_content = environment.physical_rect(*rect).unwrap();
    let packet = UiPresentationPlanner::default()
        .plan(
            candidate,
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();
    let [UiPacketPrimitive::Mesh { rect, clip, mesh }] = packet.primitives() else {
        panic!("one mesh must remain one ordered primitive")
    };
    let expected = source_mesh
        .vertices()
        .iter()
        .map(|vertex| {
            map_position(
                environment,
                [12.5, 14.0],
                [vertex.position().x(), vertex.position().y()],
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        mesh.vertices()
            .iter()
            .map(|vertex| vertex.position())
            .collect::<Vec<_>>(),
        expected,
    );
    assert_eq!(mesh.indices(), source_mesh.indices());
    assert_eq!(
        mesh.vertices()
            .iter()
            .map(|vertex| vertex.color())
            .collect::<Vec<_>>(),
        source_mesh
            .vertices()
            .iter()
            .map(|vertex| vertex.color())
            .collect::<Vec<_>>(),
    );
    let incorrect_snapped_origin = snapped_content.min_x() as f32
        + source_mesh.vertices()[0].position().x() * (snapped_content.width() as f32 / 36.0);
    assert_ne!(mesh.vertices()[0].position()[0], incorrect_snapped_origin);
    assert_ne!(environment.scale_factor(), 1.25);
    assert_eq!(
        *rect,
        UiPacketRect::new(
            expected[0][0].floor() as i32,
            expected[0][1].floor() as i32,
            expected[1][0].ceil() as i32,
            expected[2][1].ceil() as i32,
        )
        .unwrap(),
    );
    let authored_clip = UiPacketRect::new(
        snapped_content.min_x(),
        snapped_content.min_y(),
        snapped_content.max_x(),
        snapped_content.max_y(),
    )
    .unwrap();
    assert_eq!(*clip, authored_clip.intersect(*rect));
    assert!(packet.images().is_empty());
    assert!(packet.glyph_pages().is_empty());
}

#[test]
fn subpixel_mesh_envelope_preserves_a_covered_half_pixel_sample_at_fractional_dpi() {
    let environment =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 32.0, 32.0, 1.25, 1, 1, 1, 1).unwrap();
    let node = UiNode::new(UiStyle::absolute(0.0, 0.0, 8.0, 8.0)).with_mesh(mesh_at([
        [0.4, 0.4],
        [0.6, 0.4],
        [0.4, 0.6],
    ]));
    let packet = UiPresentationPlanner::default()
        .plan(
            frame_with_environment(1, vec![node], environment),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();
    let [UiPacketPrimitive::Mesh { rect, clip, mesh }] = packet.primitives() else {
        panic!("nearest-edge snapping must not erase a nondegenerate subpixel triangle")
    };
    let positions = mesh
        .vertices()
        .iter()
        .map(|vertex| vertex.position())
        .collect::<Vec<_>>();
    assert_eq!(positions, [[0.5, 0.5], [0.75, 0.5], [0.5, 0.75]]);
    assert_eq!(positions[0].map(f32::round), positions[1].map(f32::round));
    assert_eq!(positions[0].map(f32::round), positions[2].map(f32::round));
    assert!(positions[1][0] > positions[0][0] && positions[2][1] > positions[0][1]);
    assert_eq!(*rect, UiPacketRect::new(0, 0, 1, 1).unwrap());
    assert_eq!(*clip, Some(*rect));
}

#[test]
fn mesh_envelope_uses_actual_extrema_when_origin_extent_reconstruction_loses_precision() {
    let source = mesh_at([[-16_384.0, 0.0], [0.0003, 0.0], [0.0003, 16.0]]);
    let packet = UiPresentationPlanner::default()
        .plan(
            frame(1, 1, vec![mesh_node("precision", 8.0, source)]),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();
    let [UiPacketPrimitive::Mesh { rect, clip, mesh }] = packet.primitives() else {
        panic!("the tiny positive edge must survive cancellation in an origin/extent bound")
    };
    assert!(mesh.vertices()[1].position()[0] > 8.0);
    assert_eq!(rect.max_x(), 9);
    assert_eq!(clip.unwrap().min_x(), 8);
    assert_eq!(clip.unwrap().max_x(), 9);
}

#[test]
fn mesh_clip_intersects_content_parent_and_exact_target_without_clamping_vertices() {
    let environment = letterboxed_environment();
    let source = mesh_at([[-20.0, -20.0], [70.0, -20.0], [-20.0, 60.0]]);
    let child = UiNode::new(UiStyle::absolute(8.25, 4.5, 40.25, 35.5)).with_mesh(source.clone());
    let parent =
        UiNode::new(UiStyle::absolute(70.5, 60.25, 25.5, 30.0).with_overflow(UiOverflow::Clip))
            .with_child(child);
    let candidate = frame_with_environment(1, vec![parent], environment);
    assert!(candidate.diagnostics().is_empty());
    let packet = UiPresentationPlanner::default()
        .plan(
            candidate,
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();
    let [UiPacketPrimitive::Mesh { rect, clip, mesh }] = packet.primitives() else {
        panic!("a partly visible mesh must retain its complete transformed payload")
    };
    let expected_clip = environment
        .physical_rect(UiRect::from_origin_size(78.75, 64.75, 17.25, 15.25).unwrap())
        .unwrap();
    assert_eq!(
        *clip,
        UiPacketRect::new(
            expected_clip.min_x(),
            expected_clip.min_y(),
            expected_clip.max_x(),
            expected_clip.max_y(),
        ),
    );
    let first = map_position(environment, [78.75, 64.75], [-20.0, -20.0]);
    assert_eq!(mesh.vertices()[0].position(), first);
    assert!(first[0] < expected_clip.min_x() as f32);
    assert!(first[1] < expected_clip.min_y() as f32);
    assert!(rect.max_y() > expected_clip.max_y());
    assert_eq!(mesh.indices(), source.indices());
}

#[test]
fn rect_mesh_glyph_rect_preserves_the_original_paint_sequence() {
    let first = mesh_node("mixed", 8.0, triangle())
        .with_visual(UiVisual::solid(UiColor::rgba(0.2, 0.3, 0.4, 1.0)))
        .with_text(UiText::new("A", 12.0).unwrap());
    let packet = UiPresentationPlanner::default()
        .plan(
            frame(1, 1, vec![first, solid_node("after", 64.0)]),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();
    assert!(matches!(
        packet.primitives(),
        [
            UiPacketPrimitive::Rect { .. },
            UiPacketPrimitive::Mesh { .. },
            UiPacketPrimitive::Glyph { .. },
            UiPacketPrimitive::Rect { .. },
        ]
    ));
}

#[test]
fn mesh_limits_are_reduce_only_and_charge_shared_source_occurrences() {
    let defaults = UiPresentationLimits::default();
    assert_eq!(
        defaults.with_max_mesh_vertices(65_537),
        Err(UiPresentationLimitsError::CannotBroaden {
            field: "max_mesh_vertices"
        }),
    );
    assert_eq!(
        defaults.with_max_mesh_indices(196_609),
        Err(UiPresentationLimitsError::CannotBroaden {
            field: "max_mesh_indices"
        }),
    );
    assert!(
        defaults
            .with_max_mesh_vertices(0)
            .unwrap()
            .with_max_mesh_vertices(1)
            .is_err()
    );
    assert!(
        defaults
            .with_max_mesh_indices(0)
            .unwrap()
            .with_max_mesh_indices(1)
            .is_err()
    );
    let source = triangle();
    let candidate = frame(
        1,
        1,
        vec![
            mesh_node("one", 8.0, source.clone()),
            mesh_node("two", 64.0, source),
        ],
    );
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let images = UiPreparedImages::default();
    let exact = defaults
        .with_max_mesh_vertices(6)
        .unwrap()
        .with_max_mesh_indices(6)
        .unwrap();
    let packet = UiPresentationPlanner::new(exact)
        .unwrap()
        .plan(Arc::clone(&candidate), target, &images)
        .unwrap();
    assert_eq!(packet.primitives().len(), 2);
    let cases = [
        (
            exact.with_max_mesh_vertices(5).unwrap(),
            UiPresentationContentError::MeshVertexLimitExceeded { limit: 5 },
        ),
        (
            exact.with_max_mesh_indices(5).unwrap(),
            UiPresentationContentError::MeshIndexLimitExceeded { limit: 5 },
        ),
    ];
    for (limits, expected) in cases {
        let mut planner = UiPresentationPlanner::new(limits).unwrap();
        assert_eq!(
            planner
                .plan(Arc::clone(&candidate), target, &images)
                .unwrap_err(),
            expected
        );
        assert!(planner.planned(target).is_none());
    }
}

#[test]
fn fully_clipped_meshes_still_pay_the_source_occurrence_limits() {
    let candidate = frame(
        1,
        1,
        vec![mesh_node(
            "clipped",
            8.0,
            mesh_at([[100.0, 100.0], [120.0, 100.0], [100.0, 120.0]]),
        )],
    );
    assert!(matches!(candidate.paint(), [UiPaintPrimitive::Mesh { .. }]));
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let images = UiPreparedImages::default();
    let packet = UiPresentationPlanner::new(
        UiPresentationLimits::default()
            .with_max_packet_retained_bytes(0)
            .unwrap(),
    )
    .unwrap()
    .plan(Arc::clone(&candidate), target, &images)
    .unwrap();
    assert!(packet.primitives().is_empty());
    let mut planner = UiPresentationPlanner::new(
        UiPresentationLimits::default()
            .with_max_mesh_vertices(2)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        planner.plan(candidate, target, &images).unwrap_err(),
        UiPresentationContentError::MeshVertexLimitExceeded { limit: 2 },
    );
}

#[test]
fn mesh_count_refusal_precedes_atlas_staging_and_preserves_exact_recovery_packet() {
    let source = triangle();
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let images = UiPreparedImages::default();
    let cases = [
        (
            UiPresentationLimits::default()
                .with_max_mesh_vertices(5)
                .unwrap(),
            UiPresentationContentError::MeshVertexLimitExceeded { limit: 5 },
        ),
        (
            UiPresentationLimits::default()
                .with_max_mesh_indices(5)
                .unwrap(),
            UiPresentationContentError::MeshIndexLimitExceeded { limit: 5 },
        ),
    ];
    for (limits, expected) in cases {
        let mut planner = UiPresentationPlanner::new(limits).unwrap();
        let first_frame = frame(
            1,
            1,
            vec![
                text_node("label", "A", None),
                mesh_node("one", 8.0, source.clone()),
            ],
        );
        let first = planner
            .plan(Arc::clone(&first_frame), target, &images)
            .unwrap();
        let stats = planner.work_stats();
        let rejected = frame(
            1,
            2,
            vec![
                text_node("label", "B", None),
                mesh_node("one", 8.0, source.clone()),
                mesh_node("two", 64.0, source.clone()),
            ],
        );
        assert_eq!(
            planner.plan(rejected, target, &images).unwrap_err(),
            expected
        );
        let rejected_stats = planner.work_stats();
        assert_eq!(rejected_stats.text, stats.text);
        assert_eq!(rejected_stats.planned_packets, stats.planned_packets);
        assert_eq!(rejected_stats.lowered_primitives, stats.lowered_primitives);
        assert!(Arc::ptr_eq(&planner.planned(target).unwrap(), &first));
        let recovered = planner.plan(first_frame, target, &images).unwrap();
        assert_eq!(recovered.primitives(), first.primitives());
        assert_eq!(planner.work_stats().text, stats.text);
        assert!(first.glyph_pages()[0].shares_storage_with(&recovered.glyph_pages()[0]));
    }
}

#[test]
fn mesh_packet_bytes_charge_payload_arc_headers_and_every_output_occurrence() {
    let source = triangle();
    let one = frame(1, 1, vec![mesh_node("one", 8.0, source.clone())]);
    let two = frame(
        1,
        2,
        vec![
            mesh_node("one", 8.0, source.clone()),
            mesh_node("two", 64.0, source),
        ],
    );
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let images = UiPreparedImages::default();
    let single_bytes = mesh_bytes(3, 3);
    let mut exact = UiPresentationPlanner::new(
        UiPresentationLimits::default()
            .with_max_packet_retained_bytes(single_bytes)
            .unwrap(),
    )
    .unwrap();
    let first = exact.plan(Arc::clone(&one), target, &images).unwrap();
    for _ in 0..3 {
        let repeated = exact.plan(Arc::clone(&one), target, &images).unwrap();
        assert_eq!(repeated.primitives(), first.primitives());
    }
    assert_eq!(
        exact.plan(Arc::clone(&two), target, &images).unwrap_err(),
        UiPresentationContentError::PacketRetentionLimitExceeded {
            limit: single_bytes
        },
    );
    let before = exact.planned(target).unwrap();
    let rejected_stats = exact.work_stats();
    let recovered = exact.plan(one, target, &images).unwrap();
    assert_eq!(recovered.primitives(), first.primitives());
    assert_eq!(exact.work_stats().text, rejected_stats.text);
    assert!(!Arc::ptr_eq(&before, &recovered));

    let mut one_byte_short = UiPresentationPlanner::new(
        UiPresentationLimits::default()
            .with_max_packet_retained_bytes(single_bytes - 1)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        one_byte_short
            .plan(first.frame(), target, &images)
            .unwrap_err(),
        UiPresentationContentError::PacketRetentionLimitExceeded {
            limit: single_bytes - 1
        },
    );
    assert!(one_byte_short.planned(target).is_none());

    let mut doubled = UiPresentationPlanner::new(
        UiPresentationLimits::default()
            .with_max_packet_retained_bytes(2 * single_bytes)
            .unwrap(),
    )
    .unwrap();
    let doubled_packet = doubled.plan(Arc::clone(&two), target, &images).unwrap();
    let [
        UiPacketPrimitive::Mesh { mesh: first, .. },
        UiPacketPrimitive::Mesh { mesh: second, .. },
    ] = doubled_packet.primitives()
    else {
        panic!("both shared-source occurrences must retain independent transformed payloads")
    };
    assert!(!std::ptr::eq(
        first.vertices().as_ptr(),
        second.vertices().as_ptr()
    ));
    assert!(!std::ptr::eq(
        first.indices().as_ptr(),
        second.indices().as_ptr()
    ));
    let mut repeated_byte_short = UiPresentationPlanner::new(
        UiPresentationLimits::default()
            .with_max_packet_retained_bytes(2 * single_bytes - 1)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        repeated_byte_short.plan(two, target, &images).unwrap_err(),
        UiPresentationContentError::PacketRetentionLimitExceeded {
            limit: 2 * single_bytes - 1
        },
    );
}

#[test]
fn mesh_packet_byte_boundary_includes_each_arc_allocation_alignment() {
    let triangle = triangle();
    for indices in [vec![0, 1, 2], vec![0, 1, 2, 0, 1, 2]] {
        let source = UiMesh::new(triangle.vertices().to_vec(), indices.clone()).unwrap();
        let candidate = frame(1, 1, vec![mesh_node("aligned", 8.0, source)]);
        let target = RenderTarget::Window(WindowId::PRIMARY);
        let images = UiPreparedImages::default();
        let required = mesh_bytes(3, indices.len());
        if size_of::<usize>() == 8 {
            let payload = if indices.len() == 3 { 120 } else { 128 };
            assert_eq!(required, size_of::<UiPacketPrimitive>() + payload);
        }
        let mut short = UiPresentationPlanner::new(
            UiPresentationLimits::default()
                .with_max_packet_retained_bytes(required - 1)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            short
                .plan(Arc::clone(&candidate), target, &images)
                .unwrap_err(),
            UiPresentationContentError::PacketRetentionLimitExceeded {
                limit: required - 1
            }
        );
        assert!(short.planned(target).is_none());
        let mut exact = UiPresentationPlanner::new(
            UiPresentationLimits::default()
                .with_max_packet_retained_bytes(required)
                .unwrap(),
        )
        .unwrap();
        assert!(matches!(
            exact.plan(candidate, target, &images).unwrap().primitives(),
            [UiPacketPrimitive::Mesh { .. }]
        ));
    }
}

#[test]
fn combined_mesh_and_atlas_byte_refusal_preserves_last_good_atlas_and_recovers() {
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let images = UiPreparedImages::default();
    let limit = 64 * 64 + size_of::<UiPacketPrimitive>() + mesh_bytes(3, 3) - 1;
    let mut planner = UiPresentationPlanner::new(
        UiPresentationLimits::default()
            .with_atlas_page_extent(64)
            .unwrap()
            .with_max_packet_retained_bytes(limit)
            .unwrap(),
    )
    .unwrap();
    let first_frame = frame(1, 1, vec![text_node("label", "A", None)]);
    let first = planner
        .plan(Arc::clone(&first_frame), target, &images)
        .unwrap();
    assert_eq!(first.glyph_pages().len(), 1);
    assert_eq!(first.primitives().len(), 1);
    let pixels = first.glyph_pages()[0].pixels().to_vec();
    let stats = planner.work_stats();
    let rejected = frame(
        1,
        2,
        vec![
            text_node("label", "B", None),
            mesh_node("mesh", 8.0, triangle()),
        ],
    );
    assert_eq!(
        planner.plan(rejected, target, &images).unwrap_err(),
        UiPresentationContentError::PacketRetentionLimitExceeded { limit },
    );
    let rejected_stats = planner.work_stats();
    assert!(rejected_stats.text.raster_attempts > stats.text.raster_attempts);
    assert_eq!(rejected_stats.planned_packets, stats.planned_packets);
    assert_eq!(rejected_stats.lowered_primitives, stats.lowered_primitives);
    assert!(Arc::ptr_eq(&planner.planned(target).unwrap(), &first));
    assert_eq!(first.glyph_pages()[0].pixels(), pixels);
    let recovered = planner.plan(first_frame, target, &images).unwrap();
    assert_eq!(recovered.primitives(), first.primitives());
    assert_eq!(planner.work_stats().text, rejected_stats.text);
    assert!(first.glyph_pages()[0].shares_storage_with(&recovered.glyph_pages()[0]));
}

#[test]
fn out_of_range_physical_mesh_refuses_before_atlas_work_and_recovers() {
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let images = UiPreparedImages::default();
    let source = triangle();
    let first_frame = frame(1, 1, vec![text_node("label", "A", None)]);
    let mut planner = UiPresentationPlanner::default();
    let first = planner
        .plan(Arc::clone(&first_frame), target, &images)
        .unwrap();
    let stats = planner.work_stats();
    for (revision, offset) in [(2, 3_000_000_000.0), (3, -3_000_000_000.0)] {
        let candidate = frame(
            1,
            revision,
            vec![
                text_node("label", "B", None),
                UiNode::new(UiStyle::absolute(offset, 6.0, 512.0, 32.0)).with_mesh(source.clone()),
            ],
        );
        assert!(candidate.diagnostics().is_empty());
        assert!(
            candidate
                .paint()
                .iter()
                .any(|primitive| matches!(primitive, UiPaintPrimitive::Mesh { .. }))
        );
        assert_eq!(
            planner.plan(candidate, target, &images).unwrap_err(),
            UiPresentationContentError::InvalidMeshPhysicalBounds,
        );
        assert_eq!(planner.work_stats().text, stats.text);
        assert!(Arc::ptr_eq(&planner.planned(target).unwrap(), &first));
    }
    let recovered = planner.plan(first_frame, target, &images).unwrap();
    assert_eq!(recovered.primitives(), first.primitives());
    assert!(first.glyph_pages()[0].shares_storage_with(&recovered.glyph_pages()[0]));
}

#[test]
fn mesh_payloads_add_no_resource_references_and_outlive_source_and_planner() {
    let target = RenderTarget::Window(WindowId::PRIMARY);
    let image_id = UiImageId::new("project://ui/planner.png").unwrap();
    let captured = captured_image_snapshot();
    let expected_pixels = captured.resource().pixels().to_vec();
    let mut images = UiPreparedImages::default();
    images.insert(image_id.clone(), captured);
    let node = mesh_node("mixed", 8.0, triangle())
        .with_visual(UiVisual::image(
            image_id.clone(),
            UiColor::rgba(1.0, 1.0, 1.0, 1.0),
        ))
        .with_text(UiText::new("A", 12.0).unwrap());
    let candidate = frame(1, 1, vec![node]);
    let source_lifetime = Arc::downgrade(&candidate);
    assert_eq!(
        crate::planner::collect_referenced_image_ids(&[Arc::clone(&candidate)]),
        BTreeSet::from([image_id]),
    );
    let mut planner = UiPresentationPlanner::new(
        UiPresentationLimits::default()
            .with_max_referenced_images(1)
            .unwrap()
            .with_max_referenced_glyphs(1)
            .unwrap(),
    )
    .unwrap();
    let packet = planner.plan(candidate, target, &images).unwrap();
    assert_eq!(packet.images().len(), 1);
    assert_eq!(packet.glyph_pages().len(), 1);
    let retained_mesh = packet
        .primitives()
        .iter()
        .find_map(|primitive| match primitive {
            UiPacketPrimitive::Mesh { mesh, .. } => Some(mesh.clone()),
            _ => None,
        })
        .unwrap();
    let expected_vertices = retained_mesh.vertices().to_vec();
    let expected_indices = retained_mesh.indices().to_vec();
    drop(images);
    drop(planner);
    assert!(source_lifetime.upgrade().is_some());
    assert_eq!(
        packet.images()[0].snapshot().resource().pixels(),
        expected_pixels
    );
    assert!(
        packet.glyph_pages()[0]
            .pixels()
            .iter()
            .any(|coverage| *coverage > 0)
    );
    drop(packet);
    assert!(source_lifetime.upgrade().is_none());
    assert_eq!(retained_mesh.vertices(), expected_vertices);
    assert_eq!(retained_mesh.indices(), expected_indices);
}
