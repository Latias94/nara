use super::*;

use std::{mem::size_of, sync::atomic::AtomicUsize};

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiLayer, UiMesh, UiMeshVertex,
    UiNode, UiPoint, UiRoot, UiRootKey, UiSafeAreaInsets, UiSize, UiStyle,
    UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiTheme, UiVisual,
};

use crate::{GraphLimits, WgpuProgramTextureDescriptor, WgpuRenderProgramBuilder};

pub(crate) const POSITIONS: [[f32; 2]; 3] = [[-4.25, -2.75], [28.5, 3.125], [3.75, 25.5]];
pub(crate) const COLORS: [[f32; 4]; 3] = [
    [1.0, 0.0, 0.0, 0.25],
    [0.0, 1.0, 0.0, 0.5],
    [1.0, 1.0, 0.0, 0.75],
];
pub(crate) const NODE_ORIGIN: [f32; 2] = [0.375, 0.625];

#[repr(C)]
struct ArcAllocation<T> {
    _strong: AtomicUsize,
    _weak: AtomicUsize,
    _value: T,
}

pub(crate) fn viewport() -> ViewportRect {
    ViewportRect::new(3, 5, 64, 60).unwrap()
}

pub(crate) fn transform() -> UiSurfaceContentTransform {
    UiSurfaceContentTransform::new(
        UiSize::new(32.0, 28.0).unwrap(),
        UiSize::new(64.0, 60.0).unwrap(),
        UiSafeAreaInsets::new(1.25, 2.5, 2.75, 1.5).unwrap(),
        1.25,
    )
    .unwrap()
}

pub(crate) fn packet() -> Arc<UiPaintPacket> {
    packet_with_indices(vec![0, 1, 2])
}

fn packet_with_indices(indices: Vec<u32>) -> Arc<UiPaintPacket> {
    let mesh = UiMesh::new(
        POSITIONS
            .into_iter()
            .zip(COLORS)
            .map(|(position, color)| {
                UiMeshVertex::new(
                    UiPoint::new(position[0], position[1]),
                    UiColor::rgba(color[0], color[1], color[2], color[3]),
                )
            })
            .collect(),
        indices,
    )
    .unwrap();
    let root = UiRoot::new(
        UiRootKey::runtime("wgpu-ui-mesh", "root").unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
    .with_visual(UiVisual::solid(UiColor::rgba(0.0, 0.0, 1.0, 1.0)))
    .with_child(
        UiNode::new(UiStyle::absolute(
            NODE_ORIGIN[0],
            NODE_ORIGIN[1],
            22.25,
            20.25,
        ))
        .with_mesh(mesh),
    )
    .with_child(
        UiNode::new(UiStyle::absolute(0.0, 0.0, 32.0, 28.0))
            .with_visual(UiVisual::solid(UiColor::rgba(0.0, 1.0, 0.0, 0.25))),
    );
    plan(root)
}

fn plan(root: UiRoot) -> Arc<UiPaintPacket> {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root)
        .build()
        .unwrap();
    let environment = UiEnvironment::for_surface(
        UiSurfaceId::new(901).unwrap(),
        transform(),
        UiSurfaceGenerations::new(1, 1, 1, 1).unwrap(),
        1,
        1,
    )
    .unwrap();
    let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    nara_ui_render::UiPresentationPlanner::default()
        .plan(
            frame,
            RenderTarget::PrimaryWindow,
            &nara_ui_render::UiPreparedImages::default(),
        )
        .unwrap()
}

fn capture(packet: Arc<UiPaintPacket>) -> UiCapture<()> {
    let mut builder = WgpuRenderProgramBuilder::new(
        nara_render::RenderFrameSourceGeneration::try_acquire().unwrap(),
        1,
        GraphLimits::default(),
    )
    .unwrap();
    let target = builder.texture::<()>(descriptor()).unwrap();
    UiCapture::new(
        &[packet],
        RenderTarget::PrimaryWindow,
        None,
        target,
        viewport(),
    )
    .unwrap()
}

pub(crate) fn descriptor() -> WgpuProgramTextureDescriptor {
    WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("ui-mesh-native-oracle"),
            size: wgpu::Extent3d {
                width: 72,
                height: 72,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        },
        transfer: crate::TextureTransfer::Linear,
        alpha: crate::TextureAlpha::Premultiplied,
    }
}

pub(crate) fn native_metadata<S: 'static>(capture: &UiCapture<S>) -> usize {
    cost::UiPreparationCost::for_capture(capture)
        .unwrap()
        .metadata_bytes
        + crate::stock_quad::StockPreparationCost::for_batches(capture.batches.iter())
            .unwrap()
            .modeled_metadata_bytes
}

#[test]
fn same_material_quads_remain_separate_around_mesh_and_keep_exact_facts() {
    let packet = packet();
    let lowered = lower_ui_packets(
        &[Arc::clone(&packet)],
        RenderTarget::PrimaryWindow,
        None,
        viewport(),
    )
    .unwrap();
    assert_eq!(lowered.batches.len(), 2);
    assert_eq!(lowered.batches[0].material, lowered.batches[1].material);
    assert_eq!(lowered.batches[0].scissor, lowered.batches[1].scissor);
    assert_eq!(lowered.batches[0].instances.len(), 1);
    assert_eq!(lowered.batches[1].instances.len(), 1);
    let UiDraw::Mesh { indices, scissor } = &lowered.draws[1] else {
        panic!("the middle draw must be a mesh")
    };
    assert_eq!(indices, &(0..3));
    assert_eq!(
        *scissor,
        WgpuScissorRect {
            x: 5,
            y: 10,
            width: 42,
            height: 38
        }
    );
    assert_eq!(lowered.draws[0], UiDraw::Quads(0..1));
    assert_eq!(lowered.draws[2], UiDraw::Quads(1..2));
    assert_eq!(lowered.draws.len(), 3);
    let facts = &lowered.packet_facts[0];
    assert!(Arc::ptr_eq(&facts.original_packet, &packet));
    assert_eq!((facts.primitive_count, facts.mesh_primitive_count), (3, 1));
    assert_eq!(
        (facts.submitted_quad_count, facts.submitted_glyph_quad_count),
        (2, 0)
    );
    assert_eq!(
        (
            facts.submitted_mesh_count,
            facts.submitted_mesh_vertex_count,
            facts.submitted_mesh_triangle_count
        ),
        (1, 3, 1)
    );
    assert!(lowered.batches.iter().all(|batch| !batch.counts_as_sprites));
}

#[test]
fn repeated_packets_rebase_triangle_indices_and_count_every_occurrence() {
    let packet = packet();
    let lowered = lower_ui_packets(
        &[Arc::clone(&packet), Arc::clone(&packet)],
        RenderTarget::PrimaryWindow,
        None,
        viewport(),
    )
    .unwrap();
    assert_eq!(lowered.mesh_indices, [0, 1, 2, 3, 4, 5]);
    assert_eq!(lowered.mesh_vertices.len(), 6);
    assert_eq!(
        lowered
            .draws
            .iter()
            .filter(|draw| matches!(draw, UiDraw::Mesh { .. }))
            .count(),
        2
    );
    for facts in &lowered.packet_facts {
        assert!(Arc::ptr_eq(&facts.original_packet, &packet));
        assert_eq!(
            (
                facts.submitted_mesh_count,
                facts.submitted_mesh_vertex_count,
                facts.submitted_mesh_triangle_count
            ),
            (1, 3, 1)
        );
    }
    assert_eq!(lowered.packet_facts.len(), 2);
}

#[test]
fn fractional_geometry_adds_the_viewport_origin_exactly_once() {
    let packet = packet();
    let lowered =
        lower_ui_packets(&[packet], RenderTarget::PrimaryWindow, None, viewport()).unwrap();
    let transform = transform();
    assert_eq!(transform.content_scale_factor(), 1.875);
    assert_eq!(
        transform.physical_content_origin(),
        UiPoint::new(1.25, 4.25)
    );
    for ((vertex, local), color) in lowered.mesh_vertices.iter().zip(POSITIONS).zip(COLORS) {
        let physical_x = 1.25 + (NODE_ORIGIN[0] + local[0]) * 1.875;
        let physical_y = 4.25 + (NODE_ORIGIN[1] + local[1]) * 1.875;
        assert!((vertex.position[0] - (2.0 * physical_x / 64.0 - 1.0)).abs() < 0.000_001);
        assert!((vertex.position[1] - (1.0 - 2.0 * physical_y / 60.0)).abs() < 0.000_001);
        assert_eq!(vertex.color, color);
    }
}

#[test]
fn viewport_clipping_keeps_source_facts_and_both_quad_oracles_refuse_mesh() {
    let packet = packet();
    let clipped_mesh = lower_ui_packets(
        &[Arc::clone(&packet)],
        RenderTarget::PrimaryWindow,
        None,
        ViewportRect::new(3, 5, 2, 60).unwrap(),
    )
    .unwrap();
    assert_eq!(clipped_mesh.packet_facts[0].submitted_mesh_count, 0);
    assert_eq!(
        clipped_mesh.batches.len(),
        2,
        "a clipped source mesh still prevents cross-mesh batch merging"
    );
    assert!(
        clipped_mesh
            .batches
            .iter()
            .all(|batch| batch.instances.len() == 1)
    );
    let viewport = ViewportRect::new(3, 5, 1, 1).unwrap();
    let lowered = lower_ui_packets(
        &[Arc::clone(&packet)],
        RenderTarget::PrimaryWindow,
        None,
        viewport,
    )
    .unwrap();
    assert!(lowered.draws.is_empty());
    assert!(lowered.mesh_vertices.is_empty());
    assert_eq!(lowered.packet_facts[0].mesh_primitive_count, 1);
    assert_eq!(lowered.packet_facts[0].submitted_mesh_count, 0);
    assert!(Arc::ptr_eq(
        &lowered.packet_facts[0].original_packet,
        &packet
    ));
    assert_eq!(
        lowered.require_quad_only(),
        Err(WgpuUiPacketError::LegacyMeshUnsupported)
    );
    assert_eq!(
        collect_ui_quad_batches(
            &[Arc::clone(&packet)],
            RenderTarget::PrimaryWindow,
            None,
            viewport
        )
        .unwrap_err(),
        WgpuUiPacketError::LegacyMeshUnsupported
    );
    let payload = crate::frame::LegacyWgpuFramePayload {
        ui_packets: vec![packet],
        ..Default::default()
    };
    assert!(matches!(
        payload.lower_draw(RenderTarget::PrimaryWindow, viewport, None),
        Err(WgpuRenderError::UiPacketInvariant { .. })
    ));
}

#[test]
fn empty_native_cost_counts_the_capture_and_each_arc_allocation_once() {
    let mut capture = capture(packet());
    capture.batches = Arc::from([]);
    capture.draws = Arc::from([]);
    capture.mesh_vertices = Arc::from([]);
    capture.mesh_indices = Arc::from([]);
    capture.packet_facts = Arc::from([]);
    let expected = size_of::<ArcAllocation<UiCapture<()>>>()
        + size_of::<ArcAllocation<[WgpuLoweredUiPacketFacts; 0]>>()
        + size_of::<ArcAllocation<[WgpuQuadBatch; 0]>>()
        + size_of::<ArcAllocation<[UiDraw; 0]>>()
        + size_of::<ArcAllocation<[MeshVertex; 0]>>()
        + size_of::<ArcAllocation<[u32; 0]>>();
    let cost = cost::UiPreparationCost::for_capture(&capture).unwrap();
    assert_eq!(cost.metadata_bytes, expected);
    assert_eq!(cost.custody_entries, 0);
    assert_eq!((cost.vertex_bytes, cost.index_bytes), (0, 0));
}

#[test]
fn native_cost_matches_independent_allocation_sizes_for_three_and_six_indices() {
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
        (
            size_of::<ArcAllocation<[u32; 3]>>(),
            size_of::<ArcAllocation<[u32; 6]>>()
        ),
        (32, 40),
        "three indices need tail padding in their own Arc allocation"
    );
    let buffer_record = size_of::<ArcAllocation<crate::output::OutputReservation>>()
        + size_of::<crate::WgpuFeatureAllocation>()
        + size_of::<(wgpu::Buffer, crate::WgpuFeatureAllocation)>()
        + 2 * size_of::<Box<dyn crate::submission::SubmissionResources>>();
    let fixed_metadata = size_of::<ArcAllocation<UiCapture<()>>>()
        + size_of::<ArcAllocation<[WgpuLoweredUiPacketFacts; 1]>>()
        + size_of::<ArcAllocation<[WgpuQuadBatch; 2]>>()
        + size_of::<ArcAllocation<[UiDraw; 3]>>()
        + size_of::<ArcAllocation<[MeshVertex; 3]>>()
        + size_of::<ArcAllocation<mesh::MeshPipeline>>()
        + 2 * buffer_record;
    for (indices, index_allocation) in [
        (vec![0, 1, 2], size_of::<ArcAllocation<[u32; 3]>>()),
        (vec![0, 1, 2, 0, 2, 1], size_of::<ArcAllocation<[u32; 6]>>()),
    ] {
        let index_count = indices.len();
        let mut capture = capture(packet_with_indices(indices));
        Arc::make_mut(&mut capture.batches)[0].instances.reserve(3);
        assert!(capture.batches[0].instances.capacity() > capture.batches[0].instances.len());
        assert_eq!(capture.packet_facts.len(), 1);
        assert_eq!(capture.batches.len(), 2);
        assert_eq!(capture.draws.len(), 3);
        assert_eq!(capture.mesh_vertices.len(), 3);
        assert_eq!(capture.mesh_indices.len(), index_count);
        let instance_capacity: usize = capture
            .batches
            .iter()
            .map(|batch| batch.instances.capacity())
            .sum();
        let material_keys: usize = capture
            .packet_facts
            .iter()
            .map(|facts| facts.resources.len())
            .sum();
        let expected = fixed_metadata
            + index_allocation
            + instance_capacity * size_of::<WgpuQuadInstance>()
            + material_keys * size_of::<WgpuQuadMaterialKey>();
        let capture = Arc::new(capture);
        let _capture_alias = Arc::clone(&capture);
        let _facts_alias = Arc::clone(&capture.packet_facts);
        let _draws_alias = Arc::clone(&capture.draws);
        let cost = cost::UiPreparationCost::for_capture(capture.as_ref()).unwrap();
        assert_eq!(cost.metadata_bytes, expected, "{index_count} indices");
        assert_eq!(cost.custody_entries, 4);
        assert_eq!(cost.vertex_bytes, 3 * 24);
        assert_eq!(cost.index_bytes, (index_count * 4) as u64);
    }
}

#[test]
fn matching_packet_geometry_is_bounded_before_copy_even_when_completely_clipped() {
    for (vertices, indices) in [(65_536, 3), (3, 196_608)] {
        let mesh = UiMesh::new(
            (0..vertices)
                .map(|index| {
                    UiMeshVertex::new(
                        UiPoint::new(
                            if index % 2 == 0 { 0.0 } else { 1.0 },
                            if index % 3 == 0 { 1.0 } else { 0.0 },
                        ),
                        UiColor::rgba(1.0, 0.0, 0.0, 0.5),
                    )
                })
                .collect(),
            (0..indices).map(|index| (index % 3) as u32).collect(),
        )
        .unwrap();
        let packet = plan(
            UiRoot::new(
                UiRootKey::runtime("wgpu-ui-mesh-capacity", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(UiNode::new(UiStyle::absolute(10.0, 10.0, 10.0, 10.0)).with_mesh(mesh)),
        );
        let viewport = ViewportRect::new(3, 5, 1, 1).unwrap();
        let one = lower_ui_packets(
            &[Arc::clone(&packet)],
            RenderTarget::PrimaryWindow,
            None,
            viewport,
        )
        .unwrap();
        assert!(one.draws.is_empty());
        assert_eq!(one.packet_facts[0].mesh_primitive_count, 1);
        assert_eq!(
            lower_ui_packets(
                &[Arc::clone(&packet), Arc::clone(&packet)],
                RenderTarget::PrimaryWindow,
                None,
                viewport
            )
            .unwrap_err(),
            WgpuUiPacketError::MeshCapacity
        );
        assert!(
            lower_ui_packets(
                &[Arc::clone(&packet), packet],
                RenderTarget::Window(nara_window::WindowId::new(42)),
                None,
                viewport
            )
            .unwrap()
            .packet_facts
            .is_empty()
        );
    }
}
