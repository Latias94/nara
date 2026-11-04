use nara_text::{
    FontFace, GlyphRasterError, TextDirection, TextEdit, TextEditor, TextEngine, TextLayoutError,
    TextLayoutRequest, TextLimits, TextRect, TextWindowError, bundled_font_face,
};

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

#[test]
fn paint_snapshots_retain_full_display_provenance_across_selection_and_composition() {
    let mut engine = engine(TextLimits {
        max_text_bytes: 32,
        ..TextLimits::default()
    });
    let mut editor = TextEditor::new(&mut engine, "office אבג q\u{301}", 20.0).unwrap();
    let original = editor.paint_snapshot();
    assert_eq!(original.display_text(), editor.display_text());
    assert_eq!(original.width(), editor.layout().width());
    assert_eq!(original.height(), editor.layout().height());
    assert_eq!(original.glyph_count(), editor.layout().glyphs().len());
    editor
        .edit(&mut engine, &TextEdit::End { extend: false })
        .unwrap();
    assert!(original.same_source(&editor.paint_snapshot()));
    editor.edit(&mut engine, &TextEdit::SelectAll).unwrap();
    let selection = editor.selection();
    let ime = editor.ime_area();
    engine
        .prepare_window(&original, clip(0.0, 30.0), 1.25)
        .unwrap();
    assert_eq!(editor.selection(), selection);
    assert_eq!(editor.ime_area(), ime);
    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "draft".into(),
                cursor: Some((5, 5)),
            },
        )
        .unwrap();
    let preedit = editor.paint_snapshot();
    assert!(!original.same_source(&preedit));
    assert_eq!(preedit.display_text(), "draft");
    assert_eq!(editor.text(), original.display_text());
    assert!(
        editor
            .edit(&mut engine, &TextEdit::Commit("x".repeat(33)))
            .is_err()
    );
    assert!(preedit.same_source(&editor.paint_snapshot()));
    assert!(editor.is_composing());
    editor
        .edit(&mut engine, &TextEdit::CancelComposition)
        .unwrap();
    assert_eq!(editor.display_text(), original.display_text());
    assert_eq!(editor.selection(), selection);
    assert_eq!(original.display_text(), "office אבג q\u{301}");
    assert_eq!(preedit.display_text(), "draft");
}

#[test]
fn windows_preserve_complete_visual_order_runs_and_scaled_glyph_positions() {
    let mut engine = engine(TextLimits::default());
    let source = "j office אבג q\u{301}\u{327} ffi W";
    let editor = TextEditor::new(&mut engine, source, 24.0).unwrap();
    assert!(
        editor
            .layout()
            .runs()
            .iter()
            .any(|run| run.direction() == TextDirection::RightToLeft)
    );
    let ligature = engine
        .layout(&TextLayoutRequest::new("ffi", 24.0).unwrap())
        .unwrap();
    assert!(
        ligature.glyphs().len() < 3,
        "the fixture must actually shape a ligature"
    );
    let snapshot = editor.paint_snapshot();
    let mut saw_zero_advance_ink = false;
    let mut saw_overhang = false;
    for scale in [1.0, 1.25, 1.5] {
        let full = engine
            .layout(
                &TextLayoutRequest::new(source, 24.0)
                    .unwrap()
                    .with_scale_factor(scale)
                    .unwrap(),
            )
            .unwrap();
        for glyph in full.glyphs() {
            let raster = engine.rasterize(glyph.key()).unwrap();
            if raster.mask().iter().any(|alpha| *alpha != 0) {
                saw_zero_advance_ink |= glyph.advance() == 0.0;
                saw_overhang |= raster.left() < 0
                    || (raster.left() as f32 + raster.width() as f32) > glyph.advance() * scale;
            }
        }
        for left in [0.0, 24.25, 60.75, snapshot.width() - 30.0] {
            let window = engine
                .prepare_window(&snapshot, clip(left, left + 25.0), scale)
                .unwrap();
            assert_eq!(window.width(), full.width());
            assert_eq!(window.height(), full.height());
            assert_eq!(window.lines(), full.lines());
            assert_eq!(window.diagnostics(), full.diagnostics());
            assert!(window.glyphs().len() < full.glyphs().len());
            let mut remaining = full.glyphs().iter();
            for (run_index, run) in window.runs().iter().enumerate() {
                for glyph in &window.glyphs()[run.glyph_range()] {
                    assert_eq!(glyph.run_index(), run_index);
                    let original = remaining
                        .find(|original| {
                            original.x() == glyph.x()
                                && original.y() == glyph.y()
                                && original.key() == glyph.key()
                                && original.advance() == glyph.advance()
                        })
                        .expect(
                            "window glyphs must form an ordered subsequence of the complete layout",
                        );
                    let original_run = &full.runs()[original.run_index()];
                    assert_eq!(run.text_range(), original_run.text_range());
                    assert_eq!(run.direction(), original_run.direction());
                    assert_eq!(run.line_index(), original_run.line_index());
                }
            }
        }
    }
    assert!(
        saw_zero_advance_ink,
        "the fixture must contain a positioned combining mark"
    );
    assert!(
        saw_overhang,
        "the fixture must contain ink beyond its glyph's advance"
    );
}

#[test]
fn exact_placement_rekeys_physical_size_and_subpixel_phase_without_moving_glyphs() {
    let mut engine = engine(TextLimits::default());
    let editor = TextEditor::new(&mut engine, "MWq\u{301}", 20.0).unwrap();
    let snapshot = editor.paint_snapshot();
    let full_clip = clip(-50.0, 500.0);
    for scale in [1.25, 1.5] {
        let local = engine.prepare_window(&snapshot, full_clip, scale).unwrap();
        let placed = engine
            .prepare_window_at(
                &snapshot,
                full_clip,
                scale,
                [0.25 / scale, 0.5 / scale],
                [8.0, 12.0],
            )
            .unwrap();
        assert_eq!(local.glyphs().len(), placed.glyphs().len());
        for (before, after) in local.glyphs().iter().zip(placed.glyphs()) {
            assert_eq!(before.x(), after.x());
            assert_eq!(before.y(), after.y());
            assert_eq!(before.advance(), after.advance());
            assert_eq!(before.key().glyph_id(), after.key().glyph_id());
            assert_eq!(before.key().face_id(), after.key().face_id());
            assert_eq!(after.key().physical_size(), 20.0 * scale);
            assert_ne!(
                before.key(),
                after.key(),
                "fractional placement must change raster identity"
            );
        }
    }
}

#[test]
fn window_receivers_require_the_complete_ordered_font_set_and_recover_after_refusal() {
    let primary = bundled_font_face().unwrap();
    let fallback =
        FontFace::from_bytes(include_bytes!("fixtures/FiraMono-Medium.ttf").to_vec(), 0).unwrap();
    let fonts = vec![primary.clone(), fallback.clone()];
    let mut owner = TextEngine::with_fonts(fonts.clone(), TextLimits::default()).unwrap();
    let editor = TextEditor::new(&mut owner, "A\u{2229}", 24.0).unwrap();
    let snapshot = editor.paint_snapshot();
    let full = owner
        .prepare_window(&snapshot, clip(-10.0, 100.0), 1.5)
        .unwrap();
    assert!(
        full.glyphs()
            .iter()
            .any(|glyph| glyph.key().face_id() == fallback.id())
    );
    let mut changed_bytes = include_bytes!("fixtures/FiraMono-Medium.ttf").to_vec();
    changed_bytes.push(1);
    let changed = FontFace::from_bytes(changed_bytes, 0).unwrap();
    for receiver_fonts in [
        vec![fallback.clone(), primary.clone()],
        vec![primary.clone()],
        vec![primary.clone(), changed],
    ] {
        let mut receiver = TextEngine::with_fonts(receiver_fonts, TextLimits::default()).unwrap();
        assert_eq!(
            receiver.prepare_window(&snapshot, clip(500.0, 600.0), 1.0),
            Err(TextWindowError::FontSetMismatch)
        );
        let own = TextEditor::new(&mut receiver, "A", 24.0)
            .unwrap()
            .paint_snapshot();
        assert!(
            !receiver
                .prepare_window(&own, clip(-10.0, 100.0), 1.0)
                .unwrap()
                .glyphs()
                .is_empty()
        );
        assert_eq!(
            receiver.prepare_window(&snapshot, TextRect::default(), 1.0),
            Err(TextWindowError::FontSetMismatch)
        );
    }
    let mut matching = TextEngine::with_fonts(fonts, TextLimits::default()).unwrap();
    assert_eq!(
        matching
            .prepare_window(&snapshot, clip(-10.0, 100.0), 1.5)
            .unwrap(),
        full
    );
}

#[test]
fn stricter_receiver_limits_apply_to_the_full_source_and_excluded_rasters() {
    let mut owner = engine(TextLimits::default());
    let snapshot = TextEditor::new(&mut owner, "iiiiWW", 24.0)
        .unwrap()
        .paint_snapshot();
    let invisible = clip(10_000.0, 10_010.0);
    for (limits, expected) in [
        (
            TextLimits {
                max_text_bytes: 5,
                ..TextLimits::default()
            },
            TextLayoutError::TextBytesExceeded {
                actual: 6,
                maximum: 5,
            },
        ),
        (
            TextLimits {
                max_glyphs: 5,
                ..TextLimits::default()
            },
            TextLayoutError::GlyphLimitExceeded {
                actual: 6,
                maximum: 5,
            },
        ),
        (
            TextLimits {
                max_scale_factor: 1.0,
                ..TextLimits::default()
            },
            TextLayoutError::ScaleFactorExceeded {
                actual: 1.25,
                maximum: 1.0,
            },
        ),
    ] {
        let mut receiver = engine(limits);
        assert_eq!(
            receiver.prepare_window(&snapshot, invisible, 1.25),
            Err(expected.into())
        );
        let own = TextEditor::new(&mut receiver, "A", 24.0)
            .unwrap()
            .paint_snapshot();
        assert!(
            !receiver
                .prepare_window(&own, clip(0.0, 100.0), 1.0)
                .unwrap()
                .glyphs()
                .is_empty()
        );
    }
    let mut receiver = engine(TextLimits {
        max_glyph_extent: 1,
        ..TextLimits::default()
    });
    assert!(matches!(
        receiver.prepare_window(&snapshot, invisible, 1.0),
        Err(TextWindowError::Raster(GlyphRasterError::ExtentExceeded {
            maximum: 1,
            ..
        }))
    ));
    let empty = TextEditor::new(&mut receiver, "", 24.0)
        .unwrap()
        .paint_snapshot();
    assert!(
        receiver
            .prepare_window(&empty, invisible, 1.0)
            .unwrap()
            .glyphs()
            .is_empty()
    );
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, 8.25] {
        assert!(owner.prepare_window(&snapshot, invisible, scale).is_err());
    }
    assert!(
        owner
            .prepare_window(&snapshot, TextRect::default(), 1.0)
            .unwrap()
            .glyphs()
            .is_empty()
    );
    assert!(
        !owner
            .prepare_window(&snapshot, clip(0.0, 100.0), 1.0)
            .unwrap()
            .glyphs()
            .is_empty()
    );
}
