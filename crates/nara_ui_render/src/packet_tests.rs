use std::sync::Arc;

use nara_render::RenderTarget;
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiLayer, UiLocalKey, UiNode,
    UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiText, UiTheme, UiVisual,
};
use nara_window::WindowId;

use crate::{
    UiPacketPrimitive, UiPacketUv, UiPaintPacketBuilder, UiPreparedImages, UiRenderLimits,
};

fn frame(text: &str) -> Arc<nara_ui_kernel::UiFrame> {
    let node = UiNode::new(UiStyle::absolute(8.0, 6.0, 120.0, 40.0))
        .with_key(UiLocalKey::runtime("packet-test", "label").unwrap())
        .with_visual(UiVisual::solid(UiColor::rgba(0.1, 0.2, 0.3, 1.0)))
        .with_text(UiText::new(text, 18.0).unwrap());
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("packet-test", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(node),
        )
        .build()
        .unwrap();
    let environment =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 160.0, 90.0, 1.0, 1, 1, 1, 1).unwrap();
    UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap()
}

#[test]
fn packet_preserves_solid_then_nonblank_glyph_order() {
    let frame = frame("Nara");
    let packet = UiPaintPacketBuilder::default()
        .build(
            frame,
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();

    assert!(matches!(
        packet.primitives().first(),
        Some(UiPacketPrimitive::Solid { .. })
    ));
    assert!(
        packet
            .primitives()
            .iter()
            .skip(1)
            .any(|primitive| matches!(primitive, UiPacketPrimitive::Glyph { .. }))
    );
    assert!(
        packet
            .glyph_pages()
            .iter()
            .flat_map(|page| page.pixels())
            .any(|byte| *byte != 0)
    );
}

#[test]
fn packet_uv_contract_uses_top_left_origin_and_positive_extent() {
    assert_eq!(UiPacketUv::FULL.top_left(), [0.0, 0.0]);
    assert_eq!(UiPacketUv::FULL.extent(), [1.0, 1.0]);

    let packet = UiPaintPacketBuilder::default()
        .build(
            frame("Nara"),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();
    let uv = packet
        .primitives()
        .iter()
        .find_map(|primitive| match primitive {
            UiPacketPrimitive::Glyph { uv, .. } => Some(*uv),
            UiPacketPrimitive::Solid { .. } | UiPacketPrimitive::Image { .. } => None,
        })
        .expect("text should produce one glyph primitive");
    let top_left = uv.top_left();
    let extent = uv.extent();
    assert!(top_left.into_iter().all(|coordinate| coordinate >= 0.0));
    assert!(extent.into_iter().all(|length| length > 0.0));
    assert!(top_left[0] + extent[0] <= 1.0);
    assert!(top_left[1] + extent[1] <= 1.0);
}

#[test]
fn unchanged_atlas_generation_reuses_the_exact_cpu_page() {
    let frame = frame("Stable");
    let mut builder = UiPaintPacketBuilder::default();
    let first = builder
        .build(
            Arc::clone(&frame),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();
    let second = builder
        .build(
            frame,
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();

    assert_eq!(first.glyph_pages()[0].key(), second.glyph_pages()[0].key());
    assert!(first.glyph_pages()[0].shares_storage_with(&second.glyph_pages()[0]));
}

#[test]
fn raster_budget_defers_only_new_glyphs_without_partial_page_publication() {
    let limits = UiRenderLimits {
        max_glyph_entries: 8,
        max_atlas_pages: 1,
        atlas_page_extent: 64,
        max_glyph_extent: 64,
        max_new_glyphs_per_packet: 1,
        max_upload_bytes_per_packet: 64 * 64,
        max_diagnostics_per_packet: 8,
    };
    let packet = UiPaintPacketBuilder::new(limits)
        .unwrap()
        .build(
            frame("AB"),
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap();

    assert!(
        packet
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code() == "glyph-raster-deferred")
    );
    assert_eq!(
        packet
            .primitives()
            .iter()
            .filter(|primitive| matches!(primitive, UiPacketPrimitive::Glyph { .. }))
            .count(),
        1
    );
    assert!(
        packet.glyph_pages()[0]
            .pixels()
            .iter()
            .any(|byte| *byte != 0)
    );
}
