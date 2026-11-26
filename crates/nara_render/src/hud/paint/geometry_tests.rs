use nara_ui::{
    UiColor, UiLayer, UiStyle,
    kernel::{
        UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiMeshVertex, UiNode, UiPoint,
        UiRoot, UiRootKey, UiSurfaceId, UiTheme, UiVisual,
    },
    text::bundled_font_face,
};

use super::*;

#[test]
fn mesh_keeps_its_unsnapped_vertices_own_clip_and_order_between_panels() {
    let panel = |color| {
        UiNode::new(UiStyle::absolute(0.0, 0.0, 64.0, 64.0)).with_visual(UiVisual::solid(color))
    };
    let mesh_color = UiColor::rgba(0.0, 1.0, 0.0, 0.5);
    let mesh = UiMesh::new(
        [[0.25, -2.0], [32.0, 4.0], [12.0, 28.0]]
            .map(|[x, y]| UiMeshVertex::new(UiPoint::new(x, y), mesh_color))
            .to_vec(),
        vec![0, 1, 2],
    )
    .unwrap();
    let root = UiRoot::new(
        UiRootKey::runtime("hud", "mesh-order").unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
    .with_child(panel(UiColor::rgba(1.0, 0.0, 0.0, 0.5)))
    .with_child(UiNode::new(UiStyle::absolute(12.25, 14.5, 16.0, 16.0)).with_mesh(mesh))
    .with_child(panel(UiColor::rgba(0.0, 0.0, 1.0, 0.5)));
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root)
        .build()
        .unwrap();
    let environment =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 64.0, 64.0, 1.25, 1, 1, 1, 1).unwrap();
    let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    let mut geometry = Geometry::default();
    geometry
        .prepare(
            &HudFrame::new(frame, []),
            Extent2d::new(80, 80).unwrap(),
            &mut GlyphAtlas::default(),
        )
        .unwrap();

    assert_eq!(geometry.draws.len(), 3);
    let [behind, mesh, ahead] = &geometry.draws[..] else {
        unreachable!()
    };
    assert_eq!(behind.indices, 0..6);
    assert_eq!(mesh.indices, 6..9);
    assert_eq!(ahead.indices, 9..15);
    assert_eq!(mesh.clip.0, [15, 18, 35, 38]);
    assert_eq!(&geometry.indices[6..9], &[4, 5, 6]);
    assert_eq!(geometry.vertices[4].color, mesh_color.components());
    let [x, y] = geometry.vertices[4].position;
    assert!(((x + 1.0) * 40.0 - 15.625).abs() < 0.001);
    assert!(((1.0 - y) * 40.0 - 15.625).abs() < 0.001);
}
