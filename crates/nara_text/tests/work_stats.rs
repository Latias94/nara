use std::collections::HashSet;

use nara_text::{
    FontFace, GlyphRasterError, TextEdit, TextEditError, TextEditor, TextEngine, TextLayoutError,
    TextLayoutRequest, TextLimits, TextRect, TextWindowError, TextWorkStats, bundled_font_face,
};

#[test]
fn direct_layout_work_counts_repetition_and_shaping_before_caller_refusal() {
    let mut engine = engine(TextLimits::default());
    assert_eq!(engine.work_stats(), TextWorkStats::default());
    let request = TextLayoutRequest::new("ffi é", 24.0).unwrap();
    let first = engine.layout(&request).unwrap();
    let first_stats = engine.work_stats();
    assert_eq!(
        first_stats,
        TextWorkStats {
            layout_attempts: 1,
            layout_source_bytes: request.text().len() as u64,
            shaped_glyphs: first.glyphs().len() as u64,
            ..TextWorkStats::default()
        }
    );
    assert_eq!(
        engine
            .layout_with_glyph_limit(&request, usize::MAX)
            .unwrap(),
        first
    );
    assert_eq!(
        engine.layout_with_glyph_limit(&request, 0),
        Err(TextLayoutError::GlyphLimitExceeded {
            actual: 1,
            maximum: 0,
        })
    );
    assert!(
        engine
            .layout(&TextLayoutRequest::new("", 24.0).unwrap())
            .unwrap()
            .glyphs()
            .is_empty()
    );
    assert_eq!(
        engine.work_stats(),
        TextWorkStats {
            layout_attempts: 4,
            layout_source_bytes: 3 * first_stats.layout_source_bytes,
            shaped_glyphs: 3 * first_stats.shaped_glyphs,
            ..TextWorkStats::default()
        }
    );
    assert_eq!(first_stats.layout_attempts, 1);
    assert_eq!(
        TextEngine::new(bundled_font_face().unwrap(), TextLimits::default())
            .unwrap()
            .work_stats(),
        TextWorkStats::default()
    );
}

#[test]
fn direct_layout_failures_distinguish_submitted_bytes_from_completed_shaping() {
    let mut reference = engine(TextLimits::default());
    let multiline = TextLayoutRequest::new("A\nB", 24.0).unwrap();
    let multiline_glyphs = reference.layout(&multiline).unwrap().glyphs().len() as u64;
    let mut engine = engine(TextLimits {
        max_text_bytes: 4,
        max_lines: 1,
        max_glyphs: 2,
        ..TextLimits::default()
    });
    assert_eq!(
        engine.layout(&TextLayoutRequest::new("ééx", 24.0).unwrap()),
        Err(TextLayoutError::TextBytesExceeded {
            actual: 5,
            maximum: 4,
        })
    );
    assert_eq!(
        engine.work_stats(),
        TextWorkStats {
            layout_attempts: 1,
            layout_source_bytes: 5,
            ..TextWorkStats::default()
        }
    );
    assert_eq!(
        engine.layout(&TextLayoutRequest::new("iii", 24.0).unwrap()),
        Err(TextLayoutError::GlyphLimitExceeded {
            actual: 3,
            maximum: 2,
        })
    );
    assert_eq!(engine.work_stats().shaped_glyphs, 3);
    assert_eq!(
        engine.layout(&multiline),
        Err(TextLayoutError::LineLimitExceeded {
            actual: 2,
            maximum: 1,
        })
    );
    assert_eq!(engine.work_stats().shaped_glyphs, 3 + multiline_glyphs);

    let recovery = TextLayoutRequest::new("é", 24.0).unwrap();
    let expected = reference.layout(&recovery).unwrap();
    assert_eq!(engine.layout(&recovery).unwrap(), expected);
    assert_eq!(
        engine.work_stats(),
        TextWorkStats {
            layout_attempts: 4,
            layout_source_bytes: 13,
            shaped_glyphs: 3 + multiline_glyphs + expected.glyphs().len() as u64,
            ..TextWorkStats::default()
        }
    );
}

#[test]
fn raster_work_counts_repeated_masks_and_refusals_before_and_after_rendering() {
    let mut engine = engine(TextLimits::default());
    let layout = engine
        .layout(&TextLayoutRequest::new("W ", 64.0).unwrap())
        .unwrap();
    let ink_key = layout.glyphs()[0].key();
    let space_key = layout.glyphs()[1].key();
    let layout_stats = engine.work_stats();
    let first = engine.rasterize(ink_key).unwrap();
    assert!(!first.mask().is_empty());
    assert_eq!(engine.rasterize(ink_key).unwrap(), first);
    assert!(engine.rasterize(space_key).unwrap().mask().is_empty());

    let foreign_face =
        FontFace::from_bytes(include_bytes!("fixtures/FiraMono-Medium.ttf").to_vec(), 0).unwrap();
    let mut foreign = TextEngine::new(foreign_face, TextLimits::default()).unwrap();
    let foreign_layout = foreign
        .layout(&TextLayoutRequest::new("W", 64.0).unwrap())
        .unwrap();
    assert_eq!(
        engine.rasterize(foreign_layout.glyphs()[0].key()),
        Err(GlyphRasterError::FaceMismatch)
    );
    assert_eq!(
        engine.work_stats(),
        TextWorkStats {
            raster_attempts: 4,
            raster_output_bytes: 2 * first.mask().len() as u64,
            ..layout_stats
        }
    );

    let mut restricted = TextEngine::new(
        bundled_font_face().unwrap(),
        TextLimits {
            max_glyph_extent: 1,
            ..TextLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        restricted.rasterize(ink_key),
        Err(GlyphRasterError::ExtentExceeded {
            width: first.width(),
            height: first.height(),
            maximum: 1,
        })
    );
    assert!(restricted.rasterize(space_key).unwrap().mask().is_empty());
    assert_eq!(
        restricted.work_stats(),
        TextWorkStats {
            raster_attempts: 2,
            raster_output_bytes: first.mask().len() as u64,
            ..TextWorkStats::default()
        }
    );
}

#[test]
fn paint_window_work_counts_full_scans_and_only_actual_raster_calls() {
    let mut engine = engine(TextLimits::default());
    let editor = TextEditor::new(&mut engine, "iiiiiiiiiiiiiiii", 24.0).unwrap();
    let snapshot = editor.paint_snapshot();
    let keys = editor
        .layout()
        .glyphs()
        .iter()
        .map(|glyph| glyph.key())
        .collect::<HashSet<_>>();
    assert!(keys.len() < snapshot.glyph_count());
    let mut reference =
        TextEngine::new(bundled_font_face().unwrap(), TextLimits::default()).unwrap();
    let bytes_per_scan = keys
        .iter()
        .map(|key| reference.rasterize(*key).unwrap().mask().len() as u64)
        .sum::<u64>();
    let before = engine.work_stats();
    engine.validate_paint_snapshot(&snapshot, 1.0).unwrap();
    assert_eq!(engine.work_stats(), before);
    assert_eq!(before, TextWorkStats::default());

    let full_clip = clip(-100.0, 1_000.0);
    let first = engine.prepare_window(&snapshot, full_clip, 1.0).unwrap();
    assert_eq!(first.as_ref(), editor.layout());
    assert_eq!(
        engine
            .prepare_window_at(&snapshot, full_clip, 1.0, [0.0; 2], [0.0; 2])
            .unwrap(),
        first
    );
    for invisible in [TextRect::default(), clip(10_000.0, 10_010.0)] {
        let empty = engine.prepare_window(&snapshot, invisible, 1.0).unwrap();
        assert!(empty.glyphs().is_empty());
        assert_eq!(empty.width(), snapshot.width());
        assert_eq!(empty.height(), snapshot.height());
    }
    assert_eq!(
        engine.work_stats(),
        TextWorkStats {
            paint_window_attempts: 4,
            paint_window_glyphs_scanned: 4 * snapshot.glyph_count() as u64,
            raster_attempts: 4 * keys.len() as u64,
            raster_output_bytes: 4 * bytes_per_scan,
            ..TextWorkStats::default()
        }
    );
    assert_eq!(before, TextWorkStats::default());
}

#[test]
fn paint_window_refusals_count_only_work_entered_before_failure() {
    let mut owner = engine(TextLimits::default());
    let editor = TextEditor::new(&mut owner, "WWWW", 64.0).unwrap();
    let snapshot = editor.paint_snapshot();
    let produced_bytes = owner
        .rasterize(editor.layout().glyphs()[0].key())
        .unwrap()
        .mask()
        .len() as u64;
    assert!(produced_bytes > 0);
    let mut receiver = engine(TextLimits {
        max_glyphs: 1,
        ..TextLimits::default()
    });
    assert_eq!(
        receiver.prepare_window(&snapshot, TextRect::default(), 1.0),
        Err(TextLayoutError::GlyphLimitExceeded {
            actual: snapshot.glyph_count(),
            maximum: 1,
        }
        .into())
    );
    assert_eq!(
        receiver.work_stats(),
        TextWorkStats {
            paint_window_attempts: 1,
            ..TextWorkStats::default()
        }
    );

    let mut receiver = engine(TextLimits {
        max_glyph_extent: 1,
        ..TextLimits::default()
    });
    assert_eq!(
        receiver.prepare_window(&snapshot, clip(1.0, 0.0), 1.0),
        Err(TextWindowError::InvalidWindow)
    );
    assert_eq!(
        receiver.work_stats(),
        TextWorkStats {
            paint_window_attempts: 1,
            ..TextWorkStats::default()
        }
    );
    for _ in 0..2 {
        assert!(matches!(
            receiver.prepare_window(&snapshot, clip(10_000.0, 10_010.0), 1.0),
            Err(TextWindowError::Raster(GlyphRasterError::ExtentExceeded {
                maximum: 1,
                ..
            }))
        ));
    }
    let empty = TextEditor::new(&mut receiver, "", 64.0)
        .unwrap()
        .paint_snapshot();
    assert!(
        receiver
            .prepare_window(&empty, TextRect::default(), 1.0)
            .unwrap()
            .glyphs()
            .is_empty()
    );
    assert_eq!(
        receiver.work_stats(),
        TextWorkStats {
            paint_window_attempts: 4,
            paint_window_glyphs_scanned: 2,
            raster_attempts: 2,
            raster_output_bytes: 2 * produced_bytes,
            ..TextWorkStats::default()
        }
    );
    assert!(snapshot.same_source(&editor.paint_snapshot()));
}

#[test]
fn editor_parley_work_is_excluded_and_rejected_edits_keep_published_state() {
    let mut engine = engine(TextLimits {
        max_glyphs: 2,
        ..TextLimits::default()
    });
    let mut editor = TextEditor::new(&mut engine, "A", 24.0).unwrap();
    editor.edit(&mut engine, &TextEdit::SelectAll).unwrap();
    editor
        .edit(&mut engine, &TextEdit::Insert("AB".into()))
        .unwrap();
    let snapshot = editor.paint_snapshot();
    let selection = editor.selection();
    let caret = editor.caret();
    assert_eq!(editor.text(), "AB");
    assert_eq!(
        editor.edit(&mut engine, &TextEdit::Insert("C".into())),
        Err(TextEditError::Layout(TextLayoutError::GlyphLimitExceeded {
            actual: 3,
            maximum: 2,
        }))
    );
    assert_eq!(editor.text(), "AB");
    assert_eq!(editor.display_text(), "AB");
    assert_eq!(editor.selection(), selection);
    assert_eq!(editor.caret(), caret);
    assert!(snapshot.same_source(&editor.paint_snapshot()));
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), "A");
    editor.edit(&mut engine, &TextEdit::Redo).unwrap();
    assert_eq!(editor.text(), "AB");
    assert_eq!(engine.work_stats(), TextWorkStats::default());
}

fn engine(limits: TextLimits) -> TextEngine {
    TextEngine::new(bundled_font_face().unwrap(), limits).unwrap()
}

fn clip(x0: f32, x1: f32) -> TextRect {
    TextRect {
        x0,
        y0: -100.0,
        x1,
        y1: 200.0,
    }
}
