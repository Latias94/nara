use nara_text::{
    FontFace, TextDirection, TextEdit, TextEditor, TextEngine, TextLayoutError, TextLayoutRequest,
    TextLimits, TextRect, TextTypography, bundled_font_face,
};
use sha2::{Digest, Sha256};

const CJK: &[u8] = include_bytes!("fixtures/NotoSansSC-subset.otf");

#[test]
fn regular_interface_face_has_pinned_content_real_weight_and_distinct_raster_identity() {
    let bytes = include_bytes!("../assets/FiraSans-Regular.ttf");
    assert_eq!(bytes.len(), 456_996);
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "c29556a2719bf613ef3d5e070e40d903a8965d9c081beca1375dc1e6e0f93c23"
    );
    assert_eq!(
        ttf_parser::Face::parse(bytes, 0)
            .unwrap()
            .weight()
            .to_number(),
        400
    );
    let face = nara_text::bundled_regular_font_face().unwrap();
    assert_ne!(face.id(), bundled_font_face().unwrap().id());
    assert_eq!(face, nara_text::bundled_regular_font_face().unwrap());
    let mut engine = TextEngine::new(face.clone(), TextLimits::default()).unwrap();
    let layout = engine
        .layout(&TextLayoutRequest::from_typography(
            "Source café",
            TextTypography::new(14.0)
                .unwrap()
                .with_line_height(20.0)
                .unwrap(),
        ))
        .unwrap();
    assert!(layout.diagnostics().is_empty());
    assert!(
        layout
            .glyphs()
            .iter()
            .all(|glyph| glyph.key().face_id() == face.id())
    );
    assert!(
        layout
            .glyphs()
            .iter()
            .any(|glyph| engine.rasterize(glyph.key()).unwrap().width() > 0)
    );
}

#[test]
fn typography_rejects_invalid_values_and_distinguishes_automatic_from_absolute_height() {
    for value in [0.0, -1.0, f32::NAN, f32::INFINITY, 256.01] {
        assert!(matches!(
            TextTypography::new(value),
            Err(TextLayoutError::InvalidLogicalSize { .. })
        ));
    }
    let automatic = TextTypography::new(14.0).unwrap();
    assert_eq!(automatic.logical_size(), 14.0);
    assert_eq!(automatic.line_height(), None);
    let explicit = automatic.with_line_height(20.0).unwrap();
    assert_eq!(explicit.logical_size(), 14.0);
    assert_eq!(explicit.line_height(), Some(20.0));
    assert_ne!(explicit, automatic);
    assert_eq!(explicit.with_auto_line_height(), automatic);
    for value in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(matches!(
            automatic.with_line_height(value),
            Err(TextLayoutError::InvalidLineHeight { .. })
        ));
    }
    assert_eq!(automatic.line_height(), None);
}

#[test]
fn mixed_fonts_cjk_combining_and_bidi_keep_real_baselines_and_explicit_line_boxes() {
    assert_eq!(
        format!("{:x}", Sha256::digest(CJK)),
        "2059aee549faf4281b8e63cae6392fea2a358ac26da2875ba63bcbbc53ce6196"
    );
    let primary = bundled_font_face().unwrap();
    let mono =
        FontFace::from_bytes(include_bytes!("fixtures/FiraMono-Medium.ttf").to_vec(), 0).unwrap();
    let cjk = FontFace::from_bytes(CJK.to_vec(), 0).unwrap();
    let parsed_cjk = ttf_parser::Face::parse(CJK, 0).unwrap();
    let mut engine = TextEngine::with_fonts(
        vec![primary.clone(), mono.clone(), cjk.clone()],
        TextLimits::default(),
    )
    .unwrap();
    let text = "A○中文 q\u{301}\u{327} abc אבג";
    let automatic = TextTypography::new(20.0).unwrap();
    let natural = engine
        .layout(&TextLayoutRequest::from_typography(text, automatic))
        .unwrap();
    let font_units = f32::from(parsed_cjk.units_per_em());
    let ascent = f32::from(parsed_cjk.ascender()) * 20.0 / font_units;
    let descent = -f32::from(parsed_cjk.descender()) * 20.0 / font_units;
    assert!((natural.height() - ascent - descent).abs() < 0.001);
    assert!((natural.lines()[0].baseline() - ascent).abs() < 0.001);
    assert!(natural.height() > automatic.logical_size());
    assert_eq!(natural.glyphs()[0].key().face_id(), primary.id());
    assert_eq!(natural.glyphs()[1].key().face_id(), mono.id());
    for (glyph, scalar) in natural.glyphs()[2..4].iter().zip(['中', '文']) {
        assert_eq!(glyph.key().face_id(), cjk.id());
        assert_eq!(
            glyph.key().glyph_id(),
            parsed_cjk.glyph_index(scalar).unwrap().0
        );
    }
    assert!(natural.glyphs().iter().any(|glyph| glyph.advance() == 0.0));
    for direction in [TextDirection::LeftToRight, TextDirection::RightToLeft] {
        assert!(
            natural
                .runs()
                .iter()
                .any(|run| run.direction() == direction)
        );
    }

    for height in [6.0, 40.0] {
        let typography = automatic.with_line_height(height).unwrap();
        let request = TextLayoutRequest::from_typography(text, typography);
        let explicit = engine.layout(&request).unwrap();
        let mut editor = TextEditor::with_typography(&mut engine, text, typography).unwrap();
        assert_eq!(editor.layout(), &explicit);
        assert_eq!(explicit.typography(), typography);
        assert!((explicit.height() - height).abs() < 0.001);
        let expected_baseline = (height + ascent - descent) * 0.5;
        assert!((explicit.lines()[0].baseline() - expected_baseline).abs() < 0.001);
        let baseline_shift = expected_baseline - natural.lines()[0].baseline();
        assert_eq!(explicit.glyphs().len(), natural.glyphs().len());
        for (glyph, original) in explicit.glyphs().iter().zip(natural.glyphs()) {
            assert_eq!(glyph.x(), original.x());
            assert_eq!(glyph.advance(), original.advance());
            assert_eq!(glyph.key().face_id(), original.key().face_id());
            assert_eq!(glyph.key().glyph_id(), original.key().glyph_id());
            assert_eq!(glyph.key().physical_size(), original.key().physical_size());
            assert!((glyph.y() - original.y() - baseline_shift).abs() < 0.001);
        }
        editor.edit(&mut engine, &TextEdit::SelectAll).unwrap();
        let selection = editor.selection_rects();
        assert!(!selection.is_empty());
        for rect in selection {
            assert!((rect.y0 - explicit.lines()[0].block_min()).abs() < 0.001);
            assert!((rect.y1 - explicit.lines()[0].block_max()).abs() < 0.001);
        }
        let caret = editor.caret().unwrap();
        let ime = editor.ime_area();
        assert_eq!((caret.y0, caret.y1), (ime.y0, ime.y1));
        if height < natural.height() {
            assert!(explicit.lines()[0].block_min() < 0.0);
            assert!(explicit.lines()[0].block_max() > explicit.height());
        }
        let multiline = engine
            .layout(&TextLayoutRequest::from_typography(
                format!("{text}\n{text}"),
                typography,
            ))
            .unwrap();
        assert_eq!(multiline.lines().len(), 2);
        assert!((multiline.height() - height * 2.0).abs() < 0.001);
        assert!((multiline.lines()[1].baseline() - expected_baseline - height).abs() < 0.001);
    }
}

#[test]
fn reconfigured_empty_fields_use_adopted_size_for_caret_and_ime_without_reusing_old_paint() {
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), TextLimits::default()).unwrap();
    let original = TextTypography::new(14.0)
        .unwrap()
        .with_line_height(20.0)
        .unwrap();
    let mut editor = TextEditor::with_typography(&mut engine, "", original).unwrap();
    let old = editor.paint_snapshot();
    for typography in [
        original.with_line_height(22.0).unwrap(),
        TextTypography::new(28.0)
            .unwrap()
            .with_line_height(40.0)
            .unwrap(),
        TextTypography::new(12.0).unwrap(),
    ] {
        assert!(
            editor
                .reconfigure(&mut engine, typography)
                .unwrap()
                .display_changed
        );
        let reference = TextEditor::with_typography(&mut engine, "", typography).unwrap();
        assert_eq!(editor.typography(), typography);
        assert_eq!(editor.layout(), reference.layout());
        assert_eq!(editor.caret(), reference.caret());
        assert_eq!(editor.ime_area(), reference.ime_area());
        assert!(!editor.can_undo() && !editor.can_redo());
        let current = editor.paint_snapshot();
        assert!(!old.same_source(&current));
        assert_eq!(current.typography(), typography);
        assert_eq!(old.typography(), original);
        assert_eq!(old.height(), 20.0);
        assert!(
            !editor
                .reconfigure(&mut engine, typography)
                .unwrap()
                .changed()
        );
        assert!(current.same_source(&editor.paint_snapshot()));
    }
}

#[test]
fn retained_snapshots_keep_typography_real_faces_and_target_scale_ink_after_reconfiguration() {
    let cjk = FontFace::from_bytes(CJK.to_vec(), 0).unwrap();
    let primary = bundled_font_face().unwrap();
    let mut engine =
        TextEngine::with_fonts(vec![primary, cjk.clone()], TextLimits::default()).unwrap();
    let text = "j中 q\u{301}\u{327} office אבג";
    let original = TextTypography::new(14.0)
        .unwrap()
        .with_line_height(20.0)
        .unwrap();
    let mut editor = TextEditor::with_typography(&mut engine, text, original).unwrap();
    let old = editor.paint_snapshot();
    let clip = TextRect {
        x0: -100.0,
        y0: -100.0,
        x1: 2000.0,
        y1: 1000.0,
    };
    let old_window = engine.prepare_window(&old, clip, 1.5).unwrap();
    editor.edit(&mut engine, &TextEdit::SelectAll).unwrap();
    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "中文".into(),
                cursor: Some((6, 6)),
            },
        )
        .unwrap();
    let preedit = editor.paint_snapshot();
    let preedit_window = engine.prepare_window(&preedit, clip, 1.5).unwrap();
    let typography = TextTypography::new(28.0)
        .unwrap()
        .with_line_height(40.0)
        .unwrap();
    editor.reconfigure(&mut engine, typography).unwrap();
    assert_eq!(editor.text(), text);
    assert_eq!(editor.display_text(), "中文");
    let adopted = editor.paint_snapshot();
    assert_eq!(adopted.typography(), typography);
    assert!(!preedit.same_source(&adopted));
    assert_eq!(engine.prepare_window(&old, clip, 1.5).unwrap(), old_window);
    assert_eq!(
        engine.prepare_window(&preedit, clip, 1.5).unwrap(),
        preedit_window
    );
    assert_eq!(
        old.first_baseline(),
        old_window.lines().first().map(|line| line.baseline())
    );
    assert_eq!(old_window.typography(), original);
    assert!(
        old_window
            .glyphs()
            .iter()
            .any(|glyph| glyph.key().face_id() == cjk.id())
    );
    for (snapshot, expected) in [
        (&old, original),
        (&preedit, original),
        (&adopted, typography),
    ] {
        for scale in [1.0, 1.5, 2.0] {
            let window = engine.prepare_window(snapshot, clip, scale).unwrap();
            assert_eq!(window.typography(), expected);
            assert_eq!(window.height(), snapshot.height());
            for glyph in window.glyphs() {
                assert_eq!(glyph.key().physical_size(), expected.logical_size() * scale);
                let raster = engine.rasterize(glyph.key()).unwrap();
                if raster.width() == 0 || raster.height() == 0 {
                    continue;
                }
                // Pick an actual covered pixel; a line-box or advance-only clip test loses
                // overhangs and zero-advance marks. Coordinates match renderer baseline snapping.
                let Some(pixel) = raster.mask().iter().position(|coverage| *coverage != 0) else {
                    continue;
                };
                let x = (glyph.x() * scale).round()
                    + raster.left() as f32
                    + (pixel % raster.width() as usize) as f32;
                let y = (glyph.y() * scale).round() - raster.top() as f32
                    + (pixel / raster.width() as usize) as f32;
                let pixel_clip = TextRect {
                    x0: x / scale,
                    y0: y / scale,
                    x1: (x + 1.0) / scale,
                    y1: (y + 1.0) / scale,
                };
                let visible = engine.prepare_window(snapshot, pixel_clip, scale).unwrap();
                assert!(visible.glyphs().iter().any(|candidate| {
                    candidate.key() == glyph.key()
                        && candidate.x() == glyph.x()
                        && candidate.y() == glyph.y()
                }));
            }
        }
    }
}
