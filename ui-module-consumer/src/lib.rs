use nara_text::{
    GlyphRaster, TextEngine, TextLayout, TextLayoutRequest, TextLimits, bundled_font_face,
};

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
