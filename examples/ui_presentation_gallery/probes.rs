use std::sync::Arc;

use nara::image::PreparedImageSnapshot;
use nara::ui_render::advanced::{UiPacketPrimitive, UiPaintPacket};
use nara::{advanced_prelude::*, backend_prelude::WindowId};

pub(super) fn assert_owned_packet(
    packet: &Arc<UiPaintPacket>,
    frame: &Arc<UiFrame>,
    image_snapshot: &PreparedImageSnapshot,
    descriptor: UiSurfaceDescriptor,
) {
    assert!(Arc::ptr_eq(&packet.frame(), frame));
    let identity = packet.identity();
    assert_eq!(identity.surface_id(), descriptor.surface_id());
    assert_eq!(identity.frame_revision(), frame.revision());
    assert_eq!(identity.source_revision(), frame.source_revision());
    assert_eq!(
        identity.route_revision(),
        frame.environment().route_revision()
    );
    assert_eq!(
        identity.presentation_revision(),
        frame.environment().presentation_revision()
    );
    assert_eq!(packet.scale_factor(), descriptor.scale_factor());
    assert_eq!(packet.target(), RenderTarget::Window(WindowId::PRIMARY));

    let solid = packet
        .primitives()
        .iter()
        .position(|primitive| matches!(primitive, UiPacketPrimitive::Solid { .. }))
        .expect("the packet should contain a solid primitive");
    let image = packet
        .primitives()
        .iter()
        .position(|primitive| matches!(primitive, UiPacketPrimitive::Image { .. }))
        .expect("the packet should contain an image primitive");
    let glyph = packet
        .primitives()
        .iter()
        .position(|primitive| matches!(primitive, UiPacketPrimitive::Glyph { .. }))
        .expect("the packet should contain a glyph primitive");
    assert!(solid < image && image < glyph);
    assert!(matches!(
        &packet.primitives()[image],
        UiPacketPrimitive::Image { clip: Some(_), .. }
    ));
    assert!(
        packet
            .primitives()
            .iter()
            .any(|primitive| matches!(primitive, UiPacketPrimitive::Glyph { clip: Some(_), .. }))
    );
    assert_eq!(packet.images().len(), 1);
    assert_eq!(packet.images()[0].snapshot(), image_snapshot);
    assert!(
        packet
            .glyph_pages()
            .iter()
            .flat_map(|page| page.pixels())
            .any(|byte| *byte != 0)
    );
}
