use std::sync::Arc;

use nara::render::RenderTarget;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiLayer, UiNode, UiRoot,
    UiRootKey, UiStyle, UiSurfaceId, UiTheme, UiVisual,
};
use nara_ui_render::{UiPaintPacket, UiPreparedImages, UiPresentationPlanner};

/// Plans real kernel UI drawing. This creates no native presentation permission.
pub fn packet(target: RenderTarget, width: u32, height: u32) -> Arc<UiPaintPacket> {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("consumer.stock-composition", "hud").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(
                UiNode::new(UiStyle::absolute(1.0, 1.0, 1.0, (height - 2) as f32))
                    .with_visual(UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
            ),
        )
        .build()
        .unwrap();
    let frame = UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(
            &snapshot,
            UiEnvironment::new(
                UiSurfaceId::new(1).unwrap(),
                width as f32,
                height as f32,
                1.0,
                1,
                1,
                1,
                1,
            )
            .unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    UiPresentationPlanner::default()
        .plan(frame, target, &UiPreparedImages::new())
        .unwrap()
}
