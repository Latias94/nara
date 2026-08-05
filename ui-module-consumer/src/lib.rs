use nara_text::{
    GlyphRaster, TextEngine, TextLayout, TextLayoutRequest, TextLimits, bundled_font_face,
};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiLayer, UiLocalKey, UiNode, UiRoot,
    UiRootKey, UiStyle, UiSurfaceId, UiText, UiTheme, UiVisual,
};
use std::sync::Arc;

pub type StatusLabelError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Shapes and rasterizes one status label through the direct text-module boundary.
pub fn prepare_status_label(
    text: &str,
) -> Result<(TextLayout, Vec<GlyphRaster>), StatusLabelError> {
    let mut engine = TextEngine::new(bundled_font_face()?, TextLimits::default())?;
    let layout = engine.layout(&TextLayoutRequest::new(text, 18.0)?)?;
    let rasters = layout
        .glyphs()
        .iter()
        .map(|glyph| engine.rasterize(glyph.key()))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((layout, rasters))
}

/// Compiles one immutable status panel without importing an ECS, Scene, window, or renderer.
pub fn compile_status_panel(text: &str) -> Result<Arc<nara_ui_kernel::UiFrame>, StatusLabelError> {
    let status = UiNode::new(UiStyle::absolute(8.0, 8.0, 180.0, 28.0))
        .with_key(UiLocalKey::runtime("consumer", "status")?)
        .with_visual(UiVisual::role(nara_ui_kernel::UiColorRole::Panel))
        .with_text(UiText::new(text, 16.0)?);
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("consumer", "root")?,
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(status),
        )
        .build()?;
    let environment = UiEnvironment::new(
        UiSurfaceId::new(1).expect("the consumer surface identifier is non-zero"),
        320.0,
        180.0,
        1.25,
        1,
        1,
        1,
        1,
    )?;
    let mut compiler = UiCompiler::new(bundled_font_face()?, Default::default())?;
    Ok(compiler.compile(&snapshot, environment, UiTheme::default())?)
}
