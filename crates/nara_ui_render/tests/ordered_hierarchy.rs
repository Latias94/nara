use nara_render::RenderTarget;
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiLayer, UiLocalKey, UiNode,
    UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme, UiVisual,
};
use nara_ui_render::{UiPacketPrimitive, UiPaintPacketBuilder, UiPreparedImages};
use nara_window::WindowId;

fn packet(colors: [UiColor; 2]) -> std::sync::Arc<nara_ui_render::UiPaintPacket> {
    let root = UiRoot::new(
        UiRootKey::runtime("ordered-packet", "root").unwrap(),
        UiLayer::Base,
        UiStyle::fill(),
    )
    .with_child(
        UiNode::new(UiStyle::absolute(0.0, 0.0, 80.0, 80.0))
            .with_key(UiLocalKey::runtime("ordered-packet", "first").unwrap())
            .with_visual(UiVisual::solid(colors[0])),
    )
    .with_child(
        UiNode::new(UiStyle::absolute(0.0, 0.0, 80.0, 80.0))
            .with_key(UiLocalKey::runtime("ordered-packet", "second").unwrap())
            .with_visual(UiVisual::solid(colors[1])),
    );
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root)
        .build()
        .unwrap();
    let environment =
        UiEnvironment::new(UiSurfaceId::new(7).unwrap(), 100.0, 100.0, 1.0, 1, 1, 1, 1).unwrap();
    let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    UiPaintPacketBuilder::default()
        .build(
            frame,
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap()
}

#[test]
fn paint_packet_keeps_the_kernel_structural_sequence_without_a_backend_sort_key() {
    let first = UiColor::rgba(1.0, 0.0, 0.0, 1.0);
    let second = UiColor::rgba(0.0, 0.0, 1.0, 1.0);
    let packet = packet([first, second]);
    let colors = packet
        .primitives()
        .iter()
        .filter_map(|primitive| match primitive {
            UiPacketPrimitive::Solid { color, .. } => Some(*color),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(colors, [first, second]);
}
