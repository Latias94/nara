use nara_text::{
    TextAlignment, TextDiagnostic, TextDirection, TextEngine, TextEngineCreateError,
    TextLayoutError, TextLayoutRequest, TextLimits, bundled_font_face,
};

#[test]
fn empty_latin_unicode_and_bidi_text_produce_bounded_owned_results() {
    let mut engine = engine(TextLimits::default());
    let empty = engine
        .layout(&TextLayoutRequest::new("", 18.0).unwrap())
        .unwrap();
    assert_eq!(empty.width(), 0.0);
    assert!(empty.glyphs().is_empty());

    let latin = engine
        .layout(&TextLayoutRequest::new("Café Ω", 18.0).unwrap())
        .unwrap();
    assert!(latin.width().is_finite() && latin.width() > 0.0);
    assert!(latin.height().is_finite() && latin.height() > 0.0);
    assert!(!latin.glyphs().is_empty());

    let bidi = engine
        .layout(&TextLayoutRequest::new("abc אבג", 18.0).unwrap())
        .unwrap();
    assert!(
        bidi.runs()
            .iter()
            .any(|run| run.direction() == TextDirection::RightToLeft)
    );
    assert!(
        bidi.runs()
            .iter()
            .any(|run| run.direction() == TextDirection::LeftToRight)
    );
}

#[test]
fn wrapping_alignment_and_repeated_layout_are_deterministic() {
    let mut engine = engine(TextLimits::default());
    let request = TextLayoutRequest::new("one two three four five", 16.0)
        .unwrap()
        .with_wrap_width(72.0)
        .unwrap()
        .with_alignment(TextAlignment::Center)
        .with_scale_factor(1.5)
        .unwrap();
    let first = engine.layout(&request).unwrap();
    let second = engine.layout(&request).unwrap();

    assert_eq!(first, second);
    assert!(first.lines().len() > 1);
    assert!(first.width() <= 72.001);
    assert!(first.glyphs().iter().all(|glyph| glyph.x().is_finite()));
}

#[test]
fn missing_glyphs_are_diagnostic_not_silent_ascii_substitution() {
    let mut engine = engine(TextLimits::default());
    let layout = engine
        .layout(&TextLayoutRequest::new("status 🧪", 18.0).unwrap())
        .unwrap();
    assert!(
        layout
            .diagnostics()
            .iter()
            .any(|diagnostic| matches!(diagnostic, TextDiagnostic::MissingGlyph { scalar: '🧪' }))
    );
    assert!(!layout.glyphs().is_empty());
}

#[test]
fn text_line_and_glyph_limits_reject_without_poisoning_the_engine() {
    let limits = TextLimits {
        max_text_bytes: 8,
        max_lines: 1,
        max_glyphs: 3,
        ..TextLimits::default()
    };
    let mut engine = engine(limits);

    assert!(matches!(
        engine.layout(&TextLayoutRequest::new("too long!", 16.0).unwrap()),
        Err(TextLayoutError::TextBytesExceeded { .. })
    ));
    assert!(matches!(
        engine.layout(&TextLayoutRequest::new("abcd", 16.0).unwrap()),
        Err(TextLayoutError::GlyphLimitExceeded { .. })
    ));
    let wrapped = TextLayoutRequest::new("a b c", 16.0)
        .unwrap()
        .with_wrap_width(8.0)
        .unwrap();
    assert!(matches!(
        engine.layout(&wrapped),
        Err(TextLayoutError::LineLimitExceeded { .. })
    ));

    let recovered = engine
        .layout(&TextLayoutRequest::new("ok", 16.0).unwrap())
        .expect("a rejected candidate must not poison later layout");
    assert_eq!(recovered.glyphs().len(), 2);
}

#[test]
fn utf8_byte_limit_is_exact_and_recovery_remains_available() {
    let limits = TextLimits {
        max_text_bytes: 4,
        ..TextLimits::default()
    };
    let mut engine = engine(limits);

    let admitted = engine
        .layout(&TextLayoutRequest::new("éé", 16.0).unwrap())
        .unwrap();
    assert_eq!(admitted.glyphs().len(), 2);
    assert!(matches!(
        engine.layout(&TextLayoutRequest::new("ééx", 16.0).unwrap()),
        Err(TextLayoutError::TextBytesExceeded {
            actual: 5,
            maximum: 4
        })
    ));
    assert_eq!(
        engine
            .layout(&TextLayoutRequest::new("ok", 16.0).unwrap())
            .unwrap()
            .glyphs()
            .len(),
        2
    );
}

#[test]
fn fractional_scale_changes_raster_identity_not_logical_measurement() {
    let mut engine = engine(TextLimits::default());
    let mut measurements = Vec::new();
    let mut physical_sizes = Vec::new();
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let request = TextLayoutRequest::new("Scale", 20.0)
            .unwrap()
            .with_scale_factor(scale)
            .unwrap();
        let layout = engine.layout(&request).unwrap();
        measurements.push((layout.width(), layout.height()));
        physical_sizes.push(layout.glyphs()[0].key().physical_size());
    }

    assert!(measurements.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(physical_sizes, [20.0, 25.0, 30.0, 40.0]);
}

#[test]
fn request_construction_rejects_non_finite_or_out_of_range_values() {
    assert!(matches!(
        TextLayoutRequest::new("bad", f32::NAN),
        Err(TextLayoutError::InvalidLogicalSize { .. })
    ));
    assert!(matches!(
        TextLayoutRequest::new("bad", 0.0),
        Err(TextLayoutError::InvalidLogicalSize { .. })
    ));
    assert!(TextLayoutRequest::new("valid", 256.0).is_ok());
    assert!(matches!(
        TextLayoutRequest::new("bad", 256.01),
        Err(TextLayoutError::InvalidLogicalSize { .. })
    ));
    assert!(matches!(
        TextLayoutRequest::new("bad", 16.0)
            .unwrap()
            .with_scale_factor(f32::INFINITY),
        Err(TextLayoutError::InvalidScaleFactor { .. })
    ));
    assert!(matches!(
        TextLayoutRequest::new("bad", 16.0)
            .unwrap()
            .with_wrap_width(-1.0),
        Err(TextLayoutError::InvalidWrapWidth { .. })
    ));
}

#[test]
fn injected_limits_can_reduce_but_not_raise_hard_resource_ceilings() {
    let limits = TextLimits {
        max_glyphs: TextLimits::default().max_glyphs + 1,
        ..TextLimits::default()
    };
    assert!(matches!(
        TextEngine::new(bundled_font_face().unwrap(), limits),
        Err(TextEngineCreateError::InvalidLimit {
            field: "max_glyphs"
        })
    ));
}

fn engine(limits: TextLimits) -> TextEngine {
    TextEngine::new(bundled_font_face().unwrap(), limits).unwrap()
}
