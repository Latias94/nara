use nara_text::{
    FontFace, GlyphRasterError, TextEngine, TextLayoutRequest, TextLimits, bundled_font_face,
};

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

#[test]
fn fallback_glyphs_rasterize_with_the_exact_admitted_face_and_cache_key() {
    let primary = bundled_font_face().unwrap();
    let fallback =
        FontFace::from_bytes(include_bytes!("fixtures/FiraMono-Medium.ttf").to_vec(), 0).unwrap();
    let mut engine = TextEngine::with_fonts(
        vec![primary.clone(), fallback.clone()],
        TextLimits::default(),
    )
    .unwrap();
    let mut primary_engine = TextEngine::new(primary.clone(), TextLimits::default()).unwrap();
    let mut fallback_engine = TextEngine::new(fallback.clone(), TextLimits::default()).unwrap();
    let layout = engine
        .layout(&TextLayoutRequest::new("A\u{2229}", 24.0).unwrap())
        .unwrap();
    assert_eq!(layout.glyphs().len(), 2);
    let primary_key = layout.glyphs()[0].key();
    let fallback_key = layout.glyphs()[1].key();
    assert_eq!(primary_key.face_id(), primary.id());
    assert_eq!(fallback_key.face_id(), fallback.id());
    let expected_primary = primary_engine.rasterize(primary_key).unwrap();
    let expected_fallback = fallback_engine.rasterize(fallback_key).unwrap();
    assert!(expected_fallback.width() > 0 && expected_fallback.height() > 0);
    assert!(expected_fallback.mask().iter().any(|alpha| *alpha != 0));
    for _ in 0..3 {
        assert_eq!(engine.rasterize(primary_key).unwrap(), expected_primary);
        assert_eq!(engine.rasterize(fallback_key).unwrap(), expected_fallback);
    }
    assert!(matches!(
        primary_engine.rasterize(fallback_key),
        Err(GlyphRasterError::FaceMismatch)
    ));
    assert!(matches!(
        fallback_engine.rasterize(primary_key),
        Err(GlyphRasterError::FaceMismatch)
    ));
}

fn engine(limits: TextLimits) -> TextEngine {
    TextEngine::new(bundled_font_face().unwrap(), limits).unwrap()
}
