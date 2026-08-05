use nara_text::{GlyphRasterError, TextEngine, TextLayoutRequest, TextLimits, bundled_font_face};

#[test]
fn shaped_glyph_key_rasterizes_to_a_deterministic_bounded_mask() {
    let mut engine = engine(TextLimits::default());
    let request = TextLayoutRequest::new("A", 24.0)
        .unwrap()
        .with_scale_factor(2.0)
        .unwrap();
    let layout = engine.layout(&request).unwrap();
    let key = layout.glyphs()[0].key();
    assert_eq!(key.physical_size(), 48.0);

    let first = engine.rasterize(key).unwrap();
    let second = engine.rasterize(key).unwrap();
    assert_eq!(first, second);
    assert!(first.width() > 0 && first.height() > 0);
    assert_eq!(
        first.mask().len(),
        first.width() as usize * first.height() as usize
    );
}

#[test]
fn scale_factor_is_part_of_the_exact_raster_key() {
    let mut engine = engine(TextLimits::default());
    let one = engine
        .layout(&TextLayoutRequest::new("A", 20.0).unwrap())
        .unwrap();
    let two = engine
        .layout(
            &TextLayoutRequest::new("A", 20.0)
                .unwrap()
                .with_scale_factor(2.0)
                .unwrap(),
        )
        .unwrap();
    assert_ne!(one.glyphs()[0].key(), two.glyphs()[0].key());
}

#[test]
fn glyph_extent_limit_rejects_without_partial_mask_publication() {
    let limits = TextLimits {
        max_glyph_extent: 4,
        ..TextLimits::default()
    };
    let mut engine = engine(limits);
    let layout = engine
        .layout(&TextLayoutRequest::new("W", 64.0).unwrap())
        .unwrap();
    assert!(matches!(
        engine.rasterize(layout.glyphs()[0].key()),
        Err(GlyphRasterError::ExtentExceeded { maximum: 4, .. })
    ));
}

fn engine(limits: TextLimits) -> TextEngine {
    TextEngine::new(bundled_font_face().unwrap(), limits).unwrap()
}
