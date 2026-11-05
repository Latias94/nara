use std::ops::Range;

use nara_text::{
    FontFace, GlyphRasterError, TextDirection, TextEdit, TextEditError, TextEditOutcome,
    TextEditor, TextEngine, TextLayout, TextLayoutError, TextLimits, TextPaintSnapshot, TextRect,
    TextSelection, TextTypography, TextWindowError, bundled_font_face,
};

const SIZE: f32 = 21.25;
const RIGHT: TextEdit = TextEdit::MoveRight {
    extend: false,
    word: false,
};

#[test]
fn real_primary_fallback_and_order_changes_replace_layout_and_paint_provenance() {
    let sans = bundled_font_face().unwrap();
    let mono = fira_mono();
    let mona = mona_sans();
    let limits = TextLimits::default();
    let mut active = engine(vec![sans.clone()], limits);
    let mut editor = TextEditor::new(&mut active, "A○", SIZE).unwrap();
    editor.edit(&mut active, &TextEdit::SelectAll).unwrap();
    // The circle is absent from bundled Fira Sans and present in both real fallbacks.
    assert_eq!(editor.layout().glyphs()[1].key().glyph_id(), 0);
    for (fonts, circle_face) in [
        (vec![mono.clone()], mono.id()),
        (vec![sans.clone(), mono.clone()], mono.id()),
        (vec![sans.clone(), mona.clone()], mona.id()),
        (vec![sans.clone(), mono.clone(), mona.clone()], mono.id()),
        (vec![sans.clone(), mona.clone(), mono.clone()], mona.id()),
        (vec![sans.clone()], sans.id()),
    ] {
        let mut receiving = engine(fonts, limits);
        let mut reference = TextEditor::new(&mut receiving, "A○", SIZE).unwrap();
        reference
            .edit(&mut receiving, &TextEdit::SelectAll)
            .unwrap();
        let before = observe(&editor);
        let old_paint = editor.paint_snapshot();
        let old_window = active
            .prepare_window(&old_paint, full_clip(&old_paint), 1.25)
            .unwrap();
        assert_eq!(
            editor.edit(&mut receiving, &TextEdit::Insert("x".into())),
            Err(TextEditError::EngineMismatch)
        );
        assert_retained(&editor, &before, &old_paint);
        let outcome = editor.reconfigure_fonts(&mut receiving).unwrap();
        assert!(outcome.display_changed && !outcome.text_changed && !outcome.composition_changed);
        assert_eq!(observe(&editor), observe(&reference));
        assert_ne!(editor.layout(), &before.layout);
        assert_eq!(
            outcome.selection_changed,
            before.caret != editor.caret() || before.selection_rects != editor.selection_rects()
        );
        let circle = editor.layout().glyphs()[1].key();
        assert_eq!(circle.face_id(), circle_face);
        assert_eq!(circle.glyph_id() == 0, circle_face == sans.id());
        assert_eq!(
            editor.layout().diagnostics().is_empty(),
            circle_face != sans.id()
        );

        let new_paint = editor.paint_snapshot();
        assert!(!old_paint.same_source(&new_paint));
        assert_eq!(old_paint.display_text(), "A○");
        assert_eq!(
            active
                .prepare_window(&old_paint, full_clip(&old_paint), 1.25)
                .unwrap(),
            old_window
        );
        assert_eq!(
            receiving.prepare_window(&old_paint, full_clip(&old_paint), 1.25),
            Err(TextWindowError::FontSetMismatch)
        );
        assert_eq!(
            active.prepare_window(&new_paint, full_clip(&new_paint), 1.25),
            Err(TextWindowError::FontSetMismatch)
        );
        receiving
            .prepare_window(&new_paint, full_clip(&new_paint), 1.25)
            .unwrap();
        active = receiving;
    }
    assert!(!editor.edit(&mut active, &TextEdit::Undo).unwrap().changed());
}

#[test]
fn unused_fallback_changes_still_invalidate_paint_and_adopt_the_complete_ordered_set() {
    let sans = bundled_font_face().unwrap();
    let mono = fira_mono();
    let mona = mona_sans();
    let limits = TextLimits::default();
    let mut active = engine(vec![sans.clone(), mono.clone()], limits);
    let mut editor = TextEditor::new(&mut active, "Wi café", SIZE).unwrap();
    editor.edit(&mut active, &TextEdit::SelectAll).unwrap();

    for fonts in [
        vec![sans.clone(), mona.clone()],
        vec![sans.clone(), mona.clone(), mono.clone()],
        vec![sans.clone(), mono.clone(), mona.clone()],
        vec![sans.clone()],
    ] {
        let mut receiving = engine(fonts, limits);
        let before = observe(&editor);
        let old_paint = editor.paint_snapshot();
        assert_eq!(active.validate_paint_snapshot(&old_paint, 1.0), Ok(()));
        assert!(editor.layout().diagnostics().is_empty());
        assert!(
            editor
                .layout()
                .glyphs()
                .iter()
                .all(|glyph| glyph.key().face_id() == sans.id())
        );
        assert_eq!(
            editor.reconfigure_fonts(&mut receiving).unwrap(),
            TextEditOutcome {
                display_changed: true,
                ..TextEditOutcome::default()
            }
        );
        // No glyph, geometry, buffer, or history change can explain this provenance change.
        assert_eq!(observe(&editor), before);
        let new_paint = editor.paint_snapshot();
        assert!(!old_paint.same_source(&new_paint));
        assert_eq!(
            receiving.validate_paint_snapshot(&old_paint, 1.0),
            Err(TextWindowError::FontSetMismatch)
        );
        active = receiving;
    }
}

#[test]
fn identical_content_identities_keep_preedit_geometry_history_and_cached_paint() {
    let limits = TextLimits::default();
    let mut original = engine(vec![bundled_font_face().unwrap(), fira_mono()], limits);
    let mut editor = composing_editor(&mut original, None);
    let mut reference = editor.clone();
    let before = observe(&editor);
    let paint = editor.paint_snapshot();
    // Independent containers with the same ordered identities are the same configuration.
    let same_sans =
        FontFace::from_bytes(include_bytes!("../assets/FiraSans-Bold.ttf").to_vec(), 0).unwrap();
    let mut equivalent = engine(vec![same_sans, fira_mono()], limits);
    assert_eq!(
        editor.reconfigure_fonts(&mut equivalent).unwrap(),
        TextEditOutcome::default()
    );
    assert_retained(&editor, &before, &paint);
    assert_eq!(
        assert_history_matches(&mut equivalent, &mut editor, &mut reference),
        (1, 2)
    );
}

#[test]
fn reconfiguration_preserves_all_32_history_entries_and_preedit_cancel_or_one_commit() {
    for typography in [
        TextTypography::new(SIZE).unwrap(),
        TextTypography::new(14.0)
            .unwrap()
            .with_line_height(20.0)
            .unwrap(),
        TextTypography::new(28.0)
            .unwrap()
            .with_line_height(40.0)
            .unwrap(),
    ] {
        check_complete_history_reconfiguration(typography);
    }
}

fn check_complete_history_reconfiguration(typography: TextTypography) {
    let limits = TextLimits {
        max_text_bytes: 256,
        max_glyphs: 128,
        ..TextLimits::default()
    };
    let sans = bundled_font_face().unwrap();
    let mono = fira_mono();
    let mona = mona_sans();
    let mut original = engine(vec![mono.clone()], limits);
    let mut receiving = engine(vec![sans.clone(), mono.clone()], limits);
    let mut editor = TextEditor::new(&mut original, "Aé○Z", SIZE).unwrap();
    let mut reference = TextEditor::with_typography(&mut receiving, "Aé○Z", typography).unwrap();
    for (draft, engine) in [
        (&mut editor, &mut original),
        (&mut reference, &mut receiving),
    ] {
        for action in std::iter::once(TextEdit::End { extend: false })
            .chain(
                (0..32)
                    .map(|index| TextEdit::Insert(if index % 2 == 0 { "W" } else { "x" }.into())),
            )
            .chain((0..16).map(|_| TextEdit::Undo))
        {
            draft.edit(engine, &action).unwrap();
        }
        select_middle_backwards(engine, draft);
        draft
            .edit(
                engine,
                &TextEdit::Preedit {
                    text: "Ωé".into(),
                    cursor: Some((4, 2)),
                },
            )
            .unwrap();
    }
    let committed = editor.text().to_owned();
    let display = editor.display_text().to_owned();
    let old_paint = editor.paint_snapshot();
    // One full history crosses primary replacement, fallback order, then fallback replacement.
    for (fonts, circle_face) in [
        (vec![sans.clone(), mono.clone(), mona.clone()], mono.id()),
        (vec![sans.clone(), mona.clone(), mono.clone()], mona.id()),
    ] {
        let mut step = engine(fonts, limits);
        let outcome = editor.reconfigure(&mut step, typography).unwrap();
        assert!(outcome.display_changed && !outcome.text_changed && !outcome.composition_changed);
        assert_eq!(editor.layout().glyphs()[3].key().face_id(), circle_face);
        assert_eq!(editor.text(), committed);
        assert_eq!(editor.display_text(), display);
    }
    editor.reconfigure(&mut receiving, typography).unwrap();
    assert_eq!(observe(&editor), observe(&reference));
    assert!(!old_paint.same_source(&editor.paint_snapshot()));
    assert_eq!(editor.paint_snapshot().display_text(), display);
    let mut cancelled = editor.clone();
    let mut cancelled_reference = reference.clone();
    assert_eq!(
        assert_history_matches(&mut receiving, &mut cancelled, &mut cancelled_reference),
        (16, 32)
    );
    assert_same_edit(
        &mut receiving,
        &mut editor,
        &mut reference,
        &TextEdit::Commit("ñ".into()),
    );
    assert_eq!(editor.text(), committed.replacen('é', "ñ", 1));
    assert!(!editor.can_redo());
    assert_same_edit(&mut receiving, &mut editor, &mut reference, &TextEdit::Undo);
    assert_eq!(editor.text(), committed);
    assert_eq!(
        (editor.selection().anchor, editor.selection().focus),
        (3, 1)
    );
    assert_same_edit(&mut receiving, &mut editor, &mut reference, &TextEdit::Redo);
    // Commit adds exactly one local edit and discards the previous Redo branch.
    assert_eq!(
        assert_history_matches(&mut receiving, &mut editor, &mut reference),
        (17, 17)
    );
}

#[test]
fn bidi_affinities_and_hidden_or_visible_preedit_keep_the_first_origin_across_font_changes() {
    for typography in [
        TextTypography::new(SIZE).unwrap(),
        TextTypography::new(12.0)
            .unwrap()
            .with_line_height(18.0)
            .unwrap(),
        TextTypography::new(28.0)
            .unwrap()
            .with_line_height(40.0)
            .unwrap(),
    ] {
        check_bidi_affinity_reconfiguration(typography);
    }
}

fn check_bidi_affinity_reconfiguration(typography: TextTypography) {
    let mut carets = Vec::new();
    for downstream in [false, true] {
        let mut original = engine(vec![fira_mono()], TextLimits::default());
        let mut receiving = engine(vec![mona_sans()], TextLimits::default());
        let mut editor = TextEditor::new(&mut original, "abc אבג xyz", SIZE).unwrap();
        let mut reference =
            TextEditor::with_typography(&mut receiving, "abc אבג xyz", typography).unwrap();
        point_at_bidi_boundary(&mut original, &mut editor, downstream);
        point_at_bidi_boundary(&mut receiving, &mut reference, downstream);
        // Both visual sides share a byte index; comparing offsets alone would miss affinity loss.
        assert_eq!(editor.selection_bytes(), 4..4);
        assert_eq!(reference.selection(), editor.selection());
        carets.push((editor.caret().unwrap().x0, reference.caret().unwrap().x0));
        let mut origin = observe(&reference);
        let preedit = TextEdit::Preedit {
            text: "é".into(),
            cursor: downstream.then_some((2, 0)),
        };
        editor.edit(&mut original, &preedit).unwrap();
        reference.edit(&mut receiving, &preedit).unwrap();

        let outcome = editor.reconfigure(&mut receiving, typography).unwrap();
        assert!(outcome.display_changed && !outcome.text_changed && !outcome.composition_changed);
        assert_eq!(observe(&editor), observe(&reference));
        assert_eq!(editor.caret().is_none(), !downstream);
        assert_same_edit(
            &mut receiving,
            &mut editor,
            &mut reference,
            &TextEdit::Preedit {
                text: "Ωé".into(),
                cursor: downstream.then_some((4, 2)),
            },
        );
        assert_eq!(editor.caret().is_none(), !downstream);
        let mut cancelled = editor.clone();
        cancelled
            .edit(&mut receiving, &TextEdit::CancelComposition)
            .unwrap();
        assert_eq!(observe(&cancelled), origin);
        assert_same_edit(
            &mut receiving,
            &mut editor,
            &mut reference,
            &TextEdit::Commit("Q".into()),
        );
        assert_eq!(editor.text(), "abc Qאבג xyz");
        assert_same_edit(&mut receiving, &mut editor, &mut reference, &TextEdit::Undo);
        origin.can_redo = true;
        assert_eq!(observe(&editor), origin);
        for action in [
            RIGHT,
            TextEdit::MoveLeft {
                extend: true,
                word: false,
            },
            TextEdit::MoveRight {
                extend: true,
                word: false,
            },
        ] {
            assert_same_edit(&mut receiving, &mut editor, &mut reference, &action);
        }
    }
    assert_ne!(carets[0].0, carets[1].0);
    assert_ne!(carets[0].1, carets[1].1);
}

#[test]
fn stricter_and_wider_limits_refuse_even_when_text_fits_and_fonts_match() {
    let limits = TextLimits {
        max_glyphs: 64,
        ..TextLimits::default()
    };
    let fonts = vec![bundled_font_face().unwrap(), fira_mono()];
    let mut original = engine(fonts.clone(), limits);
    let mut editor = composing_editor(&mut original, None);
    let mut reference = editor.clone();
    let before = observe(&editor);
    let paint = editor.paint_snapshot();
    // A stricter replacement and a wider same-font fast path must both refuse, although text fits.
    for (fonts, max_glyphs) in [(vec![mona_sans()], 63), (fonts, 65)] {
        let mut receiving = engine(
            fonts,
            TextLimits {
                max_glyphs,
                ..limits
            },
        );
        TextEditor::new(&mut receiving, editor.display_text(), SIZE).unwrap();
        assert_eq!(
            editor.reconfigure_fonts(&mut receiving),
            Err(TextEditError::EngineMismatch)
        );
        assert_eq!(
            editor.edit(&mut receiving, &TextEdit::Undo),
            Err(TextEditError::EngineMismatch)
        );
        assert_retained(&editor, &before, &paint);
    }
    assert_eq!(
        assert_history_matches(&mut original, &mut editor, &mut reference),
        (1, 2)
    );
}

#[test]
fn receiving_glyph_quota_checks_the_oldest_undo_and_farthest_redo_before_adoption() {
    let limits = ligature_limits();
    for redo_only in [false, true] {
        let mut original = engine(vec![mona_sans()], limits);
        let mut receiving = engine(vec![fira_mono()], limits);
        let mut editor =
            TextEditor::new(&mut original, if redo_only { "a" } else { "ffi" }, SIZE).unwrap();
        for index in 1..=32 {
            let text = if redo_only && index == 32 {
                "ffi"
            } else if index % 2 == 0 {
                "a"
            } else {
                "b"
            };
            editor.edit(&mut original, &TextEdit::SelectAll).unwrap();
            assert!(
                editor
                    .edit(&mut original, &TextEdit::Insert(text.into()))
                    .unwrap()
                    .text_changed
            );
        }
        if redo_only {
            for _ in 0..32 {
                editor.edit(&mut original, &TextEdit::Undo).unwrap();
            }
        }
        assert_eq!(editor.can_undo(), !redo_only);
        assert_eq!(editor.can_redo(), redo_only);
        editor.edit(&mut original, &TextEdit::SelectAll).unwrap();
        editor
            .edit(
                &mut original,
                &TextEdit::Preedit {
                    text: "z".into(),
                    cursor: None,
                },
            )
            .unwrap();
        assert_ne!(editor.text(), "ffi");
        let typography = TextTypography::new(28.0)
            .unwrap()
            .with_line_height(40.0)
            .unwrap();
        TextEditor::with_typography(&mut receiving, editor.text(), typography).unwrap();
        TextEditor::with_typography(&mut receiving, editor.display_text(), typography).unwrap();
        let before = observe(&editor);
        let paint = editor.paint_snapshot();
        let mut reference = editor.clone();

        assert_eq!(
            editor.reconfigure(&mut receiving, typography),
            Err(TextEditError::Layout(TextLayoutError::GlyphLimitExceeded {
                actual: 3,
                maximum: 2
            }))
        );
        assert_retained(&editor, &before, &paint);
        assert_eq!(
            editor.edit(&mut receiving, &TextEdit::CancelComposition),
            Err(TextEditError::EngineMismatch)
        );
        assert_retained(&editor, &before, &paint);
        // Compare every retained buffer, selection and geometry against a pre-refusal clone.
        let expected_undo = if redo_only { 0 } else { 32 };
        assert_eq!(
            assert_history_matches(&mut original, &mut editor, &mut reference),
            (expected_undo, 32)
        );
    }
}

#[test]
fn layout_refusal_in_display_or_composition_origin_keeps_the_entire_original_draft() {
    let limits = ligature_limits();
    for origin_overflows in [false, true] {
        let mut original = engine(vec![mona_sans()], limits);
        let mut receiving = engine(vec![fira_mono()], limits);
        let origin = if origin_overflows { "ffi" } else { "x" };
        let display = if origin_overflows { "x" } else { "ffi" };
        let mut editor = TextEditor::new(&mut original, origin, SIZE).unwrap();
        editor.edit(&mut original, &TextEdit::SelectAll).unwrap();
        let origin_state = observe(&editor);
        editor
            .edit(
                &mut original,
                &TextEdit::Preedit {
                    text: display.into(),
                    cursor: Some((display.len(), 0)),
                },
            )
            .unwrap();
        let typography = TextTypography::new(14.0)
            .unwrap()
            .with_line_height(20.0)
            .unwrap();
        if origin_overflows {
            // The current display fits; the hidden composition origin alone must refuse adoption.
            TextEditor::with_typography(&mut receiving, editor.display_text(), typography).unwrap();
        } else {
            TextEditor::with_typography(&mut receiving, editor.text(), typography).unwrap();
        }
        let before = observe(&editor);
        let paint = editor.paint_snapshot();
        assert_eq!(
            editor.reconfigure(&mut receiving, typography),
            Err(TextEditError::Layout(TextLayoutError::GlyphLimitExceeded {
                actual: 3,
                maximum: 2
            }))
        );
        assert_retained(&editor, &before, &paint);
        assert_eq!(
            editor.edit(&mut receiving, &TextEdit::Commit("y".into())),
            Err(TextEditError::EngineMismatch)
        );
        assert_retained(&editor, &before, &paint);
        editor
            .edit(&mut original, &TextEdit::CancelComposition)
            .unwrap();
        assert_eq!(observe(&editor), origin_state);

        // A refusal must not poison a later viable replacement of the same retained state.
        let mut viable = engine(vec![mona_sans(), bundled_font_face().unwrap()], limits);
        assert!(
            editor
                .reconfigure_fonts(&mut viable)
                .unwrap()
                .display_changed
        );
        assert_eq!(observe(&editor), origin_state);
    }
}

#[test]
fn cross_font_forward_and_reverse_selections_keep_icu_graphemes_and_ffi_component_boundaries() {
    let text = "e\u{301}👩\u{200d}💻ffiZ";
    let combining_end = "e\u{301}".len();
    let emoji_end = "e\u{301}👩\u{200d}💻".len();
    let selected_end = emoji_end + 1;
    for (original_face, receiving_face) in [(mona_sans(), fira_mono()), (fira_mono(), mona_sans())]
    {
        for reverse in [false, true] {
            let mut original = engine(vec![original_face.clone()], TextLimits::default());
            let mut receiving = engine(vec![receiving_face.clone()], TextLimits::default());
            let mut editor = TextEditor::new(&mut original, text, SIZE).unwrap();
            if reverse {
                for _ in 0..3 {
                    editor.edit(&mut original, &RIGHT).unwrap();
                }
            }
            let extend = if reverse {
                TextEdit::MoveLeft {
                    extend: true,
                    word: false,
                }
            } else {
                TextEdit::MoveRight {
                    extend: true,
                    word: false,
                }
            };
            for _ in 0..3 {
                editor.edit(&mut original, &extend).unwrap();
            }
            let selection = editor.selection();
            let endpoints = if reverse {
                (selected_end, 0)
            } else {
                (0, selected_end)
            };
            assert_eq!((selection.anchor, selection.focus), endpoints);
            editor.reconfigure_fonts(&mut receiving).unwrap();
            assert_eq!(editor.selection(), selection);
            let mut selected = observe(&editor);
            editor.edit(&mut receiving, &TextEdit::Delete).unwrap();
            assert_eq!(editor.text(), "fiZ");
            editor.edit(&mut receiving, &TextEdit::Undo).unwrap();
            selected.can_redo = true;
            assert_eq!(observe(&editor), selected);

            for (steps, byte, action, remaining) in [
                (0, 0, TextEdit::Delete, "👩\u{200d}💻ffiZ"),
                (1, combining_end, TextEdit::Delete, "e\u{301}ffiZ"),
                (
                    3,
                    selected_end,
                    TextEdit::Backspace,
                    "e\u{301}👩\u{200d}💻fiZ",
                ),
            ] {
                editor
                    .edit(&mut receiving, &TextEdit::Home { extend: false })
                    .unwrap();
                for _ in 0..steps {
                    editor.edit(&mut receiving, &RIGHT).unwrap();
                }
                assert_eq!(editor.selection_bytes(), byte..byte);
                let before = observe(&editor);
                editor.edit(&mut receiving, &action).unwrap();
                assert_eq!(editor.text(), remaining);
                editor.edit(&mut receiving, &TextEdit::Undo).unwrap();
                assert_eq!(observe(&editor), before);
            }
        }
    }
}

#[test]
fn layout_reconfiguration_succeeds_before_separate_raster_extent_admission_refuses_drawing() {
    let limits = TextLimits {
        max_glyph_extent: 4,
        ..TextLimits::default()
    };
    let mut original = engine(vec![bundled_font_face().unwrap()], limits);
    let mut receiving = engine(vec![fira_mono()], limits);
    let mut editor = TextEditor::new(&mut original, "MW", SIZE).unwrap();
    let old_paint = editor.paint_snapshot();
    assert!(
        editor
            .reconfigure_fonts(&mut receiving)
            .unwrap()
            .display_changed
    );
    let adopted = observe(&editor);
    let paint = editor.paint_snapshot();
    assert!(!old_paint.same_source(&paint));
    assert_eq!(receiving.validate_paint_snapshot(&paint, 1.0), Ok(()));
    // Layout admission does not rasterize; drawing still enforces the original extent limit.
    assert!(matches!(
        receiving.rasterize(editor.layout().glyphs()[0].key()),
        Err(GlyphRasterError::ExtentExceeded { maximum: 4, .. })
    ));
    assert!(matches!(
        receiving.prepare_window(&paint, full_clip(&paint), 1.0),
        Err(TextWindowError::Raster(GlyphRasterError::ExtentExceeded {
            maximum: 4,
            ..
        }))
    ));
    assert_retained(&editor, &adopted, &paint);
    assert_eq!(
        editor.edit(&mut original, &TextEdit::Undo),
        Err(TextEditError::EngineMismatch)
    );
    assert!(
        !editor
            .edit(&mut receiving, &TextEdit::Undo)
            .unwrap()
            .changed()
    );
    assert_retained(&editor, &adopted, &paint);
}

fn fira_mono() -> FontFace {
    FontFace::from_bytes(include_bytes!("fixtures/FiraMono-Medium.ttf").to_vec(), 0).unwrap()
}

fn mona_sans() -> FontFace {
    FontFace::from_bytes(
        include_bytes!("fixtures/MonaSans-VariableFont.ttf").to_vec(),
        0,
    )
    .unwrap()
}

fn engine(fonts: Vec<FontFace>, limits: TextLimits) -> TextEngine {
    TextEngine::with_fonts(fonts, limits).unwrap()
}

fn ligature_limits() -> TextLimits {
    // Establish the quota counterexample using actual editor shaping, not character counts.
    for (face, glyphs) in [(mona_sans(), 1), (fira_mono(), 3)] {
        let mut engine = engine(vec![face], TextLimits::default());
        let editor = TextEditor::new(&mut engine, "ffi", SIZE).unwrap();
        assert!(editor.layout().diagnostics().is_empty());
        assert_eq!(
            editor.layout().glyphs().len(),
            glyphs,
            "pinned Parley ffi shaping"
        );
    }
    TextLimits {
        max_glyphs: 2,
        ..TextLimits::default()
    }
}

fn full_clip(snapshot: &TextPaintSnapshot) -> TextRect {
    TextRect {
        x0: -256.0,
        y0: -256.0,
        x1: snapshot.width() + 256.0,
        y1: snapshot.height() + 256.0,
    }
}

fn select_middle_backwards(engine: &mut TextEngine, editor: &mut TextEditor) {
    for action in [
        TextEdit::Home { extend: false },
        RIGHT,
        RIGHT,
        TextEdit::MoveLeft {
            extend: true,
            word: false,
        },
    ] {
        editor.edit(engine, &action).unwrap();
    }
}

fn composing_editor(engine: &mut TextEngine, cursor: Option<(usize, usize)>) -> TextEditor {
    let mut editor = TextEditor::new(engine, "AéZ", SIZE).unwrap();
    for action in [
        TextEdit::End { extend: false },
        TextEdit::Insert("x".into()),
        TextEdit::Insert("y".into()),
        TextEdit::Undo,
    ] {
        editor.edit(engine, &action).unwrap();
    }
    select_middle_backwards(engine, &mut editor);
    editor
        .edit(
            engine,
            &TextEdit::Preedit {
                text: "Ωé".into(),
                cursor,
            },
        )
        .unwrap();
    assert!(editor.is_composing() && editor.can_undo() && editor.can_redo());
    editor
}

fn point_at_bidi_boundary(engine: &mut TextEngine, editor: &mut TextEditor, downstream: bool) {
    let direction = if downstream {
        TextDirection::RightToLeft
    } else {
        TextDirection::LeftToRight
    };
    let run = editor
        .layout()
        .runs()
        .iter()
        .find(|run| run.direction() == direction)
        .unwrap();
    let glyphs = &editor.layout().glyphs()[run.glyph_range()];
    let glyph = if downstream {
        glyphs
            .iter()
            .max_by(|left, right| left.x().total_cmp(&right.x()))
            .unwrap()
    } else {
        glyphs.last().unwrap()
    };
    let line = &editor.layout().lines()[run.line_index()];
    let point = TextEdit::Point {
        x: glyph.x() + glyph.advance() * 0.8,
        y: (line.block_min() + line.block_max()) * 0.5,
        extend: false,
    };
    editor.edit(engine, &point).unwrap();
}

fn assert_same_edit(
    engine: &mut TextEngine,
    editor: &mut TextEditor,
    reference: &mut TextEditor,
    action: &TextEdit,
) {
    assert_eq!(
        editor.edit(engine, action).unwrap(),
        reference.edit(engine, action).unwrap()
    );
    assert_eq!(observe(editor), observe(reference));
}

fn assert_history_matches(
    engine: &mut TextEngine,
    editor: &mut TextEditor,
    reference: &mut TextEditor,
) -> (usize, usize) {
    assert_same_edit(engine, editor, reference, &TextEdit::CancelComposition);
    let mut undo = 0;
    while reference.can_undo() {
        assert!(
            undo < 32,
            "local history exceeded the shared 32-entry bound"
        );
        assert_same_edit(engine, editor, reference, &TextEdit::Undo);
        undo += 1;
    }
    assert_same_edit(engine, editor, reference, &TextEdit::Undo);
    let mut redo = 0;
    while reference.can_redo() {
        assert!(
            redo < 32,
            "local history exceeded the shared 32-entry bound"
        );
        assert_same_edit(engine, editor, reference, &TextEdit::Redo);
        redo += 1;
    }
    assert_same_edit(engine, editor, reference, &TextEdit::Redo);
    (undo, redo)
}

fn assert_retained(editor: &TextEditor, before: &ObservedEditor, paint: &TextPaintSnapshot) {
    assert_eq!(&observe(editor), before);
    assert_eq!(&editor.paint_snapshot(), paint);
    assert!(paint.same_source(&editor.paint_snapshot()));
}

#[derive(Debug, PartialEq)]
struct ObservedEditor {
    text: String,
    display: String,
    size: f32,
    layout: TextLayout,
    selection: TextSelection,
    caret: Option<TextRect>,
    selection_rects: Vec<TextRect>,
    ime_area: TextRect,
    composition: Option<Range<usize>>,
    composing: bool,
    can_undo: bool,
    can_redo: bool,
}

fn observe(editor: &TextEditor) -> ObservedEditor {
    ObservedEditor {
        text: editor.text().into(),
        display: editor.display_text().into(),
        size: editor.logical_size(),
        layout: editor.layout().clone(),
        selection: editor.selection(),
        caret: editor.caret(),
        selection_rects: editor.selection_rects().into(),
        ime_area: editor.ime_area(),
        composition: editor.composition_range(),
        composing: editor.is_composing(),
        can_undo: editor.can_undo(),
        can_redo: editor.can_redo(),
    }
}
