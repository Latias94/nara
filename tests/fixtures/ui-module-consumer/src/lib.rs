use nara_text::{
    GlyphRaster, TextEngine, TextLayout, TextLayoutRequest, TextLimits, bundled_font_face,
};
use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiKey, UiLayer, UiLocalKey,
    UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiText, UiTheme, UiVisual,
};
use nara_ui_widgets::UiButton;
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

/// Compiles one controlled Button through the ordinary public authoring interface.
pub fn compile_retry_button() -> Result<(UiButton, Arc<nara_ui_kernel::UiFrame>), StatusLabelError>
{
    let root_key = UiRootKey::runtime("consumer", "terminal")?;
    let local_key = UiLocalKey::runtime("consumer", "retry")?;
    let key = UiKey::scoped(root_key.clone(), local_key)?;
    let button = UiButton::new(key, "Retry", UiActionId::new("retry")?)?;
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root_key.clone(), UiLayer::Modal, UiStyle::fill()).with_child(
                button.authored_node(
                    &root_key,
                    UiStyle::absolute(100.0, 60.0, 120.0, 44.0),
                    UiVisual::role(nara_ui_kernel::UiColorRole::Accent),
                    nara_ui_kernel::UiBrush::Role(nara_ui_kernel::UiColorRole::PrimaryText),
                    18.0,
                )?,
            ),
        )
        .build()?;
    let surface = UiSurfaceId::new(2).expect("the consumer surface identifier is non-zero");
    let environment = UiEnvironment::new(surface, 320.0, 180.0, 1.0, 1, 1, 1, 1)?;
    let mut compiler = UiCompiler::new(bundled_font_face()?, Default::default())?;
    let frame = compiler.compile(&snapshot, environment, UiTheme::default())?;
    Ok((button, frame))
}
