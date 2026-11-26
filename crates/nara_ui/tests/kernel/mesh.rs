use nara_ui::kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEdges, UiEnvironment, UiLayer, UiMesh,
    UiMeshError, UiMeshVertex, UiNode, UiOverflow, UiPaintPrimitive, UiPoint, UiRect, UiRoot,
    UiRootKey, UiStyle, UiSurfaceId, UiTheme, UiVisual,
};
use nara_ui::text::bundled_font_face;

const COLOR: UiColor = UiColor::rgba(0.25, 0.5, 0.75, 0.5);

fn vertices() -> Vec<UiMeshVertex> {
    [(0.0, 0.0), (30.0, 0.0), (0.0, 20.0)]
        .into_iter()
        .map(|(x, y)| UiMeshVertex::new(UiPoint::new(x, y), COLOR))
        .collect()
}

#[test]
fn mesh_requires_complete_triangles_and_valid_indices() {
    assert!(matches!(
        UiMesh::new(vertices(), vec![0, 1]),
        Err(UiMeshError::IncompleteTriangle { index_count: 2 })
    ));
    assert!(matches!(
        UiMesh::new(vertices(), vec![0, 3, 2]),
        Err(UiMeshError::IndexOutOfRange { offset: 1, .. })
    ));
}

#[test]
fn mesh_paints_in_the_content_box_and_clips_to_its_ancestor_at_fractional_scale() {
    let mesh = UiMesh::new(vertices(), vec![0, 1, 2]).unwrap();
    let child = UiNode::new(
        UiStyle::absolute(5.0, 4.0, 40.0, 26.0)
            .with_padding(UiEdges::new(2.0, 4.0, 1.0, 3.0).unwrap()),
    )
    .with_visual(UiVisual::solid(COLOR))
    .with_mesh(mesh.clone());
    let parent =
        UiNode::new(UiStyle::absolute(10.0, 12.0, 30.0, 22.0).with_overflow(UiOverflow::Clip))
            .with_child(child);
    let root = UiRoot::new(
        UiRootKey::runtime("mesh", "hud").unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
    .with_child(parent);
    let source = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root)
        .build()
        .unwrap();
    let environment =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 100.0, 80.0, 1.25, 1, 1, 1, 1).unwrap();
    let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&source, environment, UiTheme::default())
        .unwrap();
    let [
        UiPaintPrimitive::Rect { .. },
        UiPaintPrimitive::Mesh {
            rect,
            clip_rect,
            physical_rect,
            mesh: retained,
            ..
        },
    ] = frame.paint()
    else {
        panic!("expected background followed by its mesh")
    };
    assert_eq!(
        *rect,
        UiRect::from_origin_size(17.0, 17.0, 34.0, 22.0).unwrap()
    );
    assert_eq!(*clip_rect, UiRect::from_origin_size(17.0, 17.0, 23.0, 17.0));
    assert_eq!(Some(*physical_rect), environment.physical_rect(*rect));
    assert!(std::ptr::eq(mesh.vertices(), retained.vertices()));
    assert!(frame.hit_regions().is_empty());
}
