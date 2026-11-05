use nara_text::{
    TextAlignment, TextEngine, TextLayout, TextLayoutError, TextLayoutRequest, TextLimits,
    TextWorkStats, bundled_font_face,
};
use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamilyName, Layout, LayoutContext, StyleProperty,
    TextWrapMode,
    fontique::{Blob, Collection, CollectionOptions, FontInfoOverride, SourceCache},
};

#[test]
fn intrinsic_requests_are_distinct_and_explicit_width_switches_the_mode() {
    let natural = TextLayoutRequest::new("one two", 20.0).unwrap();
    let minimum = natural.clone().with_min_content();
    assert_eq!(natural.wrap_width(), None);
    assert!(!natural.is_min_content());
    assert_eq!(minimum.wrap_width(), None);
    assert!(minimum.is_min_content());
    assert_ne!(minimum, natural);
    assert_eq!(minimum.clone().with_min_content(), minimum);

    let wrapped = minimum.clone().with_wrap_width(72.0).unwrap();
    assert_eq!(wrapped.wrap_width(), Some(72.0));
    assert!(!wrapped.is_min_content());
    assert_eq!(wrapped, natural.with_wrap_width(72.0).unwrap());
    assert_eq!(wrapped.with_min_content(), minimum);
    for width in [0.0, -1.0, f32::NAN, f32::INFINITY, 1_000_001.0] {
        assert!(matches!(
            minimum.clone().with_wrap_width(width),
            Err(TextLayoutError::InvalidWrapWidth { .. })
        ));
    }
}

#[test]
fn multiword_min_content_uses_exact_shaped_line_metrics_face_and_scale() {
    let mut engine = engine(TextLimits::default());
    let request = TextLayoutRequest::new("one two three", 20.0)
        .unwrap()
        .with_alignment(TextAlignment::Center);
    let natural = engine.layout(&request).unwrap();
    let mut reference = shaped_layout(request.text(), request.logical_size());
    let widths = reference.calculate_content_widths();
    assert!(widths.min > 0.0 && widths.min < widths.max);
    reference.break_all_lines(Some(widths.min));
    reference.align(Alignment::Center, AlignmentOptions::default());

    let mut logical_lines: Option<Vec<_>> = None;
    for scale in [1.0, 1.5] {
        let minimum = engine
            .layout(
                &request
                    .clone()
                    .with_min_content()
                    .with_scale_factor(scale)
                    .unwrap(),
            )
            .unwrap();
        assert!(minimum.width() < natural.width());
        assert!(minimum.lines().len() > natural.lines().len());
        assert_layout_matches_parley(&minimum, &reference);
        assert!(minimum.glyphs().iter().all(|glyph| {
            glyph.key().face_id() == engine.font_face_id()
                && glyph.key().physical_size() == request.logical_size() * scale
        }));
        if let Some(lines) = &logical_lines {
            assert_eq!(minimum.lines(), lines.as_slice());
        } else {
            logical_lines = Some(minimum.lines().to_vec());
        }
    }
}

#[test]
fn an_unbreakable_word_has_the_same_minimum_and_natural_layout() {
    let mut engine = engine(TextLimits::default());
    let request = TextLayoutRequest::new("office", 20.0).unwrap();
    let natural = engine.layout(&request).unwrap();
    let minimum = engine.layout(&request.with_min_content()).unwrap();
    assert_eq!(minimum, natural);
    assert_eq!(minimum.lines().len(), 1);
    assert!(minimum.glyphs().len() < "office".len());
}

#[test]
fn empty_spaces_and_combining_sequences_follow_shaped_intrinsic_widths() {
    let mut engine = engine(TextLimits::default());
    for text in ["", " ", "   ", "e\u{301} e\u{301}"] {
        let mut reference = shaped_layout(text, 20.0);
        let min_width = reference.calculate_content_widths().min;
        if text.is_empty() || text == " " {
            assert_eq!(min_width, 0.0);
        }
        reference.break_all_lines(Some(min_width));
        reference.align(Alignment::Start, AlignmentOptions::default());
        let minimum = engine
            .layout(
                &TextLayoutRequest::new(text, 20.0)
                    .unwrap()
                    .with_min_content(),
            )
            .unwrap();
        assert_layout_matches_parley(&minimum, &reference);
        assert!(minimum.width().is_finite() && minimum.width() >= 0.0);
        assert!(minimum.diagnostics().is_empty());
        if text.contains('\u{301}') {
            assert_eq!(minimum.lines().len(), 2);
            for line in minimum.lines() {
                assert_eq!(text[line.text_range()].trim(), "e\u{301}");
            }
        }
    }
}

#[test]
fn min_content_limits_charge_submitted_source_and_completed_shaping_on_refusal() {
    let mut engine = engine(TextLimits {
        max_text_bytes: 4,
        max_lines: 1,
        max_glyphs: 2,
        max_scale_factor: 1.0,
        ..TextLimits::default()
    });
    let request = |text| {
        TextLayoutRequest::new(text, 20.0)
            .unwrap()
            .with_min_content()
    };
    assert_eq!(
        engine.layout(&request("ééx")),
        Err(TextLayoutError::TextBytesExceeded {
            actual: 5,
            maximum: 4,
        })
    );
    assert_eq!(
        engine.layout(&request("A").with_scale_factor(1.5).unwrap()),
        Err(TextLayoutError::ScaleFactorExceeded {
            actual: 1.5,
            maximum: 1.0,
        })
    );
    assert_eq!(engine.work_stats().shaped_glyphs, 0);
    assert_eq!(
        engine.layout(&request("iii")),
        Err(TextLayoutError::GlyphLimitExceeded {
            actual: 3,
            maximum: 2,
        })
    );
    assert_eq!(engine.work_stats().shaped_glyphs, 3);
    assert_eq!(
        engine.layout(&request("A A")),
        Err(TextLayoutError::LineLimitExceeded {
            actual: 2,
            maximum: 1,
        })
    );
    assert_eq!(engine.work_stats().shaped_glyphs, 6);
    assert_eq!(
        engine.layout_with_glyph_limit(&request("A"), 0),
        Err(TextLayoutError::GlyphLimitExceeded {
            actual: 1,
            maximum: 0,
        })
    );
    let recovered = engine
        .layout_with_glyph_limit(&request("AA"), usize::MAX)
        .unwrap();
    assert_eq!(recovered.glyphs().len(), 2);
    assert_eq!(
        engine.work_stats(),
        TextWorkStats {
            layout_attempts: 6,
            layout_source_bytes: 15,
            shaped_glyphs: 9,
            ..TextWorkStats::default()
        }
    );
}

#[test]
fn width_probe_preserves_exact_min_content_geometry_without_admitting_drawable_lines() {
    let request = TextLayoutRequest::new("A A", 20.0)
        .unwrap()
        .with_min_content();
    let expected = engine(TextLimits::default()).layout(&request).unwrap();
    let mut restricted = engine(TextLimits {
        max_lines: 1,
        ..TextLimits::default()
    });
    let measured = restricted
        .measure_for_width_with_glyph_limit(&request, usize::MAX)
        .unwrap();
    assert_eq!(measured.width(), expected.width());
    assert_eq!(measured.height(), expected.height());
    assert_eq!(
        measured.first_baseline(),
        Some(expected.lines()[0].baseline())
    );
    assert_eq!(measured.glyph_count(), expected.glyphs().len());
    assert_eq!(
        restricted.layout(&request),
        Err(TextLayoutError::LineLimitExceeded {
            actual: 2,
            maximum: 1,
        })
    );
    assert_eq!(restricted.work_stats().layout_attempts, 2);
    assert_eq!(restricted.work_stats().layout_source_bytes, 6);
    assert_eq!(restricted.work_stats().shaped_glyphs, 6);
    assert_eq!(restricted.work_stats().raster_attempts, 0);
}

#[test]
fn width_probe_keeps_source_grapheme_scale_and_glyph_work_bounds() {
    let mut engine = engine(TextLimits {
        max_text_bytes: 4,
        max_lines: 1,
        max_glyphs: 2,
        max_scale_factor: 1.0,
        ..TextLimits::default()
    });
    let request = |text| {
        TextLayoutRequest::new(text, 20.0)
            .unwrap()
            .with_min_content()
    };
    assert_eq!(
        engine.measure_for_width_with_glyph_limit(&request("ééx"), usize::MAX),
        Err(TextLayoutError::TextBytesExceeded {
            actual: 5,
            maximum: 4,
        })
    );
    assert_eq!(
        engine.measure_for_width_with_glyph_limit(
            &request("A").with_scale_factor(1.5).unwrap(),
            usize::MAX,
        ),
        Err(TextLayoutError::ScaleFactorExceeded {
            actual: 1.5,
            maximum: 1.0,
        })
    );
    assert_eq!(engine.work_stats().shaped_glyphs, 0);
    assert_eq!(
        engine.measure_for_width_with_glyph_limit(&request("iii"), usize::MAX),
        Err(TextLayoutError::GlyphLimitExceeded {
            actual: 3,
            maximum: 2,
        })
    );
    assert_eq!(
        engine.measure_for_width_with_glyph_limit(&request("A"), 0),
        Err(TextLayoutError::GlyphLimitExceeded {
            actual: 1,
            maximum: 0,
        })
    );
    assert_eq!(
        engine
            .measure_for_width_with_glyph_limit(&request("AA"), usize::MAX)
            .unwrap()
            .glyph_count(),
        2
    );
    assert_eq!(
        engine.work_stats(),
        TextWorkStats {
            layout_attempts: 5,
            layout_source_bytes: 12,
            shaped_glyphs: 6,
            ..TextWorkStats::default()
        }
    );
    let mut grapheme_engine = self::engine(TextLimits::default());
    let oversized = format!("a{}", "\u{301}".repeat(nara_text::MAX_GRAPHEME_SCALARS));
    assert!(matches!(
        grapheme_engine.measure_for_width_with_glyph_limit(&request(&oversized), usize::MAX),
        Err(TextLayoutError::GraphemeLimitExceeded { .. })
    ));
    assert_eq!(grapheme_engine.work_stats().shaped_glyphs, 0);
}

#[test]
fn width_probe_uses_the_same_fallback_typography_and_scale_as_direct_min_content() {
    let fonts = vec![
        bundled_font_face().unwrap(),
        nara_text::FontFace::from_bytes(
            include_bytes!("fixtures/NotoSansSC-subset.otf").to_vec(),
            0,
        )
        .unwrap(),
    ];
    let mut reference = TextEngine::with_fonts(fonts.clone(), TextLimits::default()).unwrap();
    let mut probe = TextEngine::with_fonts(
        fonts,
        TextLimits {
            max_lines: 1,
            max_glyph_extent: 1,
            ..TextLimits::default()
        },
    )
    .unwrap();
    for typography in [
        nara_text::TextTypography::new(20.0).unwrap(),
        nara_text::TextTypography::new(28.0)
            .unwrap()
            .with_line_height(3.0)
            .unwrap(),
    ] {
        for scale in [1.0, 1.5] {
            let request = TextLayoutRequest::from_typography("office e\u{301} 中文", typography)
                .with_min_content()
                .with_scale_factor(scale)
                .unwrap();
            let expected = reference.layout(&request).unwrap();
            assert!(expected.lines().len() > 1);
            assert!(
                expected
                    .glyphs()
                    .iter()
                    .any(|glyph| glyph.key().face_id() != reference.font_face_id())
            );
            let measured = probe
                .measure_for_width_with_glyph_limit(&request, usize::MAX)
                .unwrap();
            assert_eq!(measured.width(), expected.width());
            assert_eq!(measured.height(), expected.height());
            assert_eq!(
                measured.first_baseline(),
                Some(expected.lines()[0].baseline())
            );
            assert_eq!(measured.glyph_count(), expected.glyphs().len());
        }
    }
    assert_eq!(probe.work_stats().raster_attempts, 0);
    assert_eq!(probe.work_stats().raster_output_bytes, 0);
}

fn engine(limits: TextLimits) -> TextEngine {
    TextEngine::new(bundled_font_face().unwrap(), limits).unwrap()
}

fn shaped_layout(text: &str, size: f32) -> Layout<()> {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    let registered = collection.register_fonts(
        Blob::from(include_bytes!("../assets/FiraSans-Bold.ttf").to_vec()),
        Some(FontInfoOverride {
            family_name: Some("min-content-fixture"),
            width: Some(Default::default()),
            style: Some(Default::default()),
            weight: Some(Default::default()),
            ..FontInfoOverride::default()
        }),
    );
    assert_eq!(registered.len(), 1);
    let mut fonts = FontContext {
        collection,
        source_cache: SourceCache::default(),
    };
    let mut context = LayoutContext::new();
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, false);
    builder.push_default(StyleProperty::FontFamily(
        FontFamilyName::Named("min-content-fixture".into()).into(),
    ));
    builder.push_default(StyleProperty::FontSize(size));
    builder.push_default(StyleProperty::TextWrapMode(TextWrapMode::Wrap));
    builder.build(text)
}

fn assert_layout_matches_parley(actual: &TextLayout, expected: &Layout<()>) {
    assert_eq!(actual.width(), expected.width());
    assert_eq!(actual.height(), expected.height());
    assert_eq!(actual.lines().len(), expected.len());
    for (line, reference) in actual.lines().iter().zip(expected.lines()) {
        let metrics = reference.metrics();
        assert_eq!(line.text_range(), reference.text_range());
        assert_eq!(line.baseline(), metrics.baseline);
        assert_eq!(line.block_min(), metrics.block_min_coord);
        assert_eq!(line.block_max(), metrics.block_max_coord);
        assert_eq!(line.advance(), metrics.advance);
    }
}
