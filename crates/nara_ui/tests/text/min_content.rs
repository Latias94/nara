use nara_ui::text::{TextEngine, TextLayoutRequest, bundled_font_face};

#[test]
fn min_content_shapes_a_narrower_layout_with_dpi_independent_logical_lines() {
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let request = TextLayoutRequest::new("one two three", 20.0).unwrap();
    let natural = engine.layout(&request).unwrap();
    let request = request.with_min_content();
    let minimum = engine.layout(&request).unwrap();
    let fractional = engine
        .layout(&request.with_scale_factor(1.5).unwrap())
        .unwrap();
    assert!(minimum.width() < natural.width());
    assert!(minimum.lines().len() > natural.lines().len());
    assert_eq!(minimum.lines(), fractional.lines());
    assert!(!fractional.glyphs().is_empty());
    assert!(
        fractional
            .glyphs()
            .iter()
            .all(|glyph| glyph.key().physical_size() == 30.0)
    );
}
