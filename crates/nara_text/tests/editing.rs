use std::ops::Range;

use nara_text::{
    FontFace, TextDirection, TextEdit, TextEditError, TextEditor, TextEngine, TextLayout,
    TextLayoutError, TextLayoutRequest, TextLimits, TextRect, TextSelection, bundled_font_face,
};

const RIGHT: TextEdit = TextEdit::MoveRight {
    extend: false,
    word: false,
};
const LEFT: TextEdit = TextEdit::MoveLeft {
    extend: false,
    word: false,
};

#[test]
fn empty_buffers_have_owned_geometry_and_no_op_edits_do_not_create_history() {
    let mut engine = engine(TextLimits::default());
    let mut editor = TextEditor::new(&mut engine, "", 18.0).unwrap();

    assert_eq!(editor.logical_size(), 18.0);
    assert_eq!(editor.text(), "");
    assert_eq!(editor.display_text(), "");
    assert_eq!(editor.layout(), editor.snapshot());
    assert!(editor.snapshot().glyphs().is_empty());
    assert_eq!(editor.selection_bytes(), 0..0);
    assert!(editor.selection().is_collapsed());
    assert!(editor.caret().is_some());
    assert!(editor.selection_rects().is_empty());
    assert_rect(editor.ime_area());

    for action in [
        TextEdit::Insert(String::new()),
        TextEdit::Backspace,
        TextEdit::Delete,
        TextEdit::Undo,
        TextEdit::Redo,
        TextEdit::CancelComposition,
    ] {
        assert!(!editor.edit(&mut engine, &action).unwrap().changed());
        assert!(!editor.can_undo());
        assert!(!editor.can_redo());
    }
}

#[test]
fn cursor_movement_and_deletion_follow_parley_clusters_instead_of_utf8_bytes() {
    let mut engine = engine(TextLimits::default());
    let original = "e\u{301}👩\u{200d}💻Z";
    let combining_end = "e\u{301}".len();
    let emoji_end = "e\u{301}👩\u{200d}💻".len();
    let mut editor = TextEditor::new(&mut engine, original, 18.0).unwrap();

    editor.edit(&mut engine, &RIGHT).unwrap();
    assert_eq!(editor.selection_bytes(), combining_end..combining_end);
    editor.edit(&mut engine, &RIGHT).unwrap();
    assert_eq!(editor.selection_bytes(), emoji_end..emoji_end);
    editor.edit(&mut engine, &LEFT).unwrap();
    assert_eq!(editor.selection_bytes(), combining_end..combining_end);

    editor.edit(&mut engine, &TextEdit::Delete).unwrap();
    assert_eq!(editor.text(), "e\u{301}Z");
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), original);
    assert_eq!(editor.selection_bytes(), combining_end..combining_end);

    editor
        .edit(&mut engine, &TextEdit::End { extend: false })
        .unwrap();
    editor.edit(&mut engine, &TextEdit::Backspace).unwrap();
    assert_eq!(editor.text(), "e\u{301}👩\u{200d}💻");
    editor.edit(&mut engine, &TextEdit::Backspace).unwrap();
    assert_eq!(editor.text(), "e\u{301}");
    editor.edit(&mut engine, &TextEdit::Backspace).unwrap();
    assert_eq!(editor.text(), "");
}

#[test]
fn grapheme_pointer_selection_and_insertion_never_adopt_a_shaped_ligature_component() {
    let mut engine = engine(TextLimits::default());
    let text = "e\u{301}👩\u{200d}💻Z";
    let boundaries = [
        0,
        "e\u{301}".len(),
        "e\u{301}👩\u{200d}💻".len(),
        text.len(),
    ];
    let mut editor = TextEditor::new(&mut engine, text, 18.0).unwrap();
    let line = editor.layout().lines()[0].clone();
    let height = (line.block_min() + line.block_max()) * 0.5;
    for extend in [false, true] {
        for position in 0..=100 {
            editor
                .edit(&mut engine, &TextEdit::Home { extend: false })
                .unwrap();
            editor
                .edit(
                    &mut engine,
                    &TextEdit::Point {
                        x: line.advance() * position as f32 / 100.0,
                        y: height,
                        extend,
                    },
                )
                .unwrap();
            assert!(boundaries.contains(&editor.selection().anchor));
            assert!(boundaries.contains(&editor.selection().focus));
        }
    }
    editor
        .edit(&mut engine, &TextEdit::Home { extend: false })
        .unwrap();
    editor
        .edit(
            &mut engine,
            &TextEdit::MoveRight {
                extend: true,
                word: false,
            },
        )
        .unwrap();
    assert_eq!(editor.selection_bytes(), 0..3);
    editor.edit(&mut engine, &TextEdit::Delete).unwrap();
    assert_eq!(editor.text(), "👩\u{200d}💻Z");
    editor.edit(&mut engine, &TextEdit::Delete).unwrap();
    assert_eq!(editor.text(), "Z");

    let mut joined = TextEditor::new(&mut engine, "👩💻", 18.0).unwrap();
    joined.edit(&mut engine, &RIGHT).unwrap();
    joined
        .edit(&mut engine, &TextEdit::Insert("\u{200d}".into()))
        .unwrap();
    assert_eq!(joined.text(), "👩\u{200d}💻");
    assert_eq!(
        joined.selection_bytes(),
        joined.text().len()..joined.text().len()
    );
    joined.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(joined.text(), "👩💻");
    assert_eq!(joined.selection_bytes(), "👩".len().."👩".len());
}

#[test]
fn deletion_that_joins_regional_indicators_moves_out_of_the_new_grapheme_and_undo_restores_selection()
 {
    let mut engine = engine(TextLimits::default());
    for backward in [false, true] {
        let mut editor = TextEditor::new(&mut engine, "🇦x🇧", 18.0).unwrap();
        editor.edit(&mut engine, &RIGHT).unwrap();
        if backward {
            editor.edit(&mut engine, &RIGHT).unwrap();
        }
        let mut before = observe(&editor);
        editor
            .edit(
                &mut engine,
                if backward {
                    &TextEdit::Backspace
                } else {
                    &TextEdit::Delete
                },
            )
            .unwrap();
        assert_eq!(editor.text(), "🇦🇧");
        assert_eq!(editor.selection_bytes(), 8..8);
        editor
            .edit(
                &mut engine,
                &TextEdit::Preedit {
                    text: "x".into(),
                    cursor: Some((1, 1)),
                },
            )
            .unwrap();
        editor
            .edit(&mut engine, &TextEdit::CancelComposition)
            .unwrap();
        assert_eq!(editor.text(), "🇦🇧");
        assert_eq!(editor.selection_bytes(), 8..8);
        editor.edit(&mut engine, &TextEdit::Undo).unwrap();
        before.can_redo = true;
        assert_eq!(observe(&editor), before);
    }
}

#[test]
fn excessive_complete_replacement_graphemes_refuse_before_parley_without_consuming_history() {
    let mut engine = engine(TextLimits::default());
    let original = format!("a{}", "\u{301}".repeat(31));
    let mut editor = TextEditor::new(&mut engine, &original, 18.0).unwrap();
    editor
        .edit(&mut engine, &TextEdit::End { extend: false })
        .unwrap();
    for action in [
        TextEdit::Insert("\u{301}".into()),
        TextEdit::Commit("\u{301}".into()),
        TextEdit::Preedit {
            text: "\u{301}".into(),
            cursor: Some((2, 2)),
        },
    ] {
        let before = observe(&editor);
        assert!(matches!(
            editor.edit(&mut engine, &action),
            Err(TextEditError::Layout(
                TextLayoutError::GraphemeLimitExceeded {
                    actual: 33,
                    maximum: 32,
                    ..
                }
            ))
        ));
        assert_eq!(observe(&editor), before);
    }
    editor
        .edit(&mut engine, &TextEdit::Insert("x".into()))
        .unwrap();
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), original);
    let separated = format!("a{}x{}", "\u{301}".repeat(31), "\u{301}".repeat(31));
    let mut editor = TextEditor::new(&mut engine, &separated, 18.0).unwrap();
    editor.edit(&mut engine, &RIGHT).unwrap();
    let mut before = observe(&editor);
    editor.edit(&mut engine, &TextEdit::Delete).unwrap();
    assert_eq!(editor.text(), original);
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    before.can_redo = true;
    assert_eq!(observe(&editor), before);
}

#[test]
fn word_movement_selection_and_replacement_remain_local_edits() {
    let mut engine = engine(TextLimits::default());
    let mut editor = TextEditor::new(&mut engine, "one two", 18.0).unwrap();
    editor
        .edit(
            &mut engine,
            &TextEdit::MoveRight {
                extend: false,
                word: true,
            },
        )
        .unwrap();
    assert_eq!(editor.selection_bytes(), 3..3);
    editor
        .edit(
            &mut engine,
            &TextEdit::MoveRight {
                extend: true,
                word: true,
            },
        )
        .unwrap();
    assert_eq!(editor.selection_bytes(), 3..7);
    editor
        .edit(&mut engine, &TextEdit::Insert(" three".into()))
        .unwrap();
    assert_eq!(editor.text(), "one three");
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), "one two");
    assert_eq!(editor.selection_bytes(), 3..7);
    editor
        .edit(&mut engine, &TextEdit::Home { extend: true })
        .unwrap();
    assert_eq!(editor.selection_bytes(), 0..3);
    assert_eq!(editor.selection().anchor, 3);
    editor
        .edit(&mut engine, &TextEdit::End { extend: true })
        .unwrap();
    assert_eq!(editor.selection_bytes(), 3..7);
}

#[test]
fn bidi_pointer_selection_and_caret_use_the_actual_owned_layout() {
    let mut engine = engine(TextLimits::default());
    let text = "abc אבג xyz";
    let mut editor = TextEditor::new(&mut engine, text, 20.0).unwrap();
    let expected = engine
        .layout(&TextLayoutRequest::new(text, 20.0).unwrap())
        .unwrap();
    assert_eq!(editor.snapshot(), &expected);

    let rtl_run = editor
        .snapshot()
        .runs()
        .iter()
        .find(|run| run.direction() == TextDirection::RightToLeft)
        .unwrap();
    let rightmost = editor.snapshot().glyphs()[rtl_run.glyph_range()]
        .iter()
        .filter(|glyph| glyph.advance() > 0.0)
        .max_by(|first, second| first.x().total_cmp(&second.x()))
        .unwrap()
        .clone();
    let line = editor.snapshot().lines()[rtl_run.line_index()].clone();
    let pointer_y = (line.block_min() + line.block_max()) * 0.5;
    let left_x = rightmost.x() + rightmost.advance() * 0.2;
    let right_x = rightmost.x() + rightmost.advance() * 0.8;

    editor
        .edit(
            &mut engine,
            &TextEdit::Point {
                x: left_x,
                y: pointer_y,
                extend: false,
            },
        )
        .unwrap();
    let left_selection = editor.selection();
    let left_caret = editor.caret().unwrap();
    editor
        .edit(
            &mut engine,
            &TextEdit::Point {
                x: right_x,
                y: pointer_y,
                extend: false,
            },
        )
        .unwrap();
    let right_selection = editor.selection();
    let right_caret = editor.caret().unwrap();
    assert!(left_selection.focus > right_selection.focus);
    assert!(left_caret.x0 < right_caret.x0);
    assert_eq!(left_caret.y0, line.block_min());
    assert_eq!(left_caret.y1, line.block_max());
    assert!((left_caret.x0 - rightmost.x()).abs() < 0.01);
    assert!((right_caret.x0 - rightmost.x() - rightmost.advance()).abs() < 0.01);

    editor
        .edit(&mut engine, &TextEdit::Home { extend: false })
        .unwrap();
    editor.edit(&mut engine, &RIGHT).unwrap();
    editor
        .edit(
            &mut engine,
            &TextEdit::Point {
                x: left_x,
                y: pointer_y,
                extend: true,
            },
        )
        .unwrap();
    assert_eq!(editor.selection_bytes(), 1.."abc א".len());
    assert_eq!(editor.selection_rects().len(), 2);
    for rect in editor.selection_rects() {
        assert_rect(*rect);
        assert!(rect.x1 > rect.x0);
        assert_eq!(rect.y0, line.block_min());
        assert_eq!(rect.y1, line.block_max());
    }
    assert_rect(editor.ime_area());
    assert!(editor.ime_area().x0 <= editor.caret().unwrap().x0);
    assert!(editor.ime_area().x1 >= editor.caret().unwrap().x0);
    assert_eq!(editor.text(), text);
    assert!(!editor.can_undo());
}

#[test]
fn ordinary_bidi_insertion_keeps_the_inserted_runs_trailing_caret_and_empty_insert_is_inert() {
    let mut engine = engine(TextLimits::default());
    let mut editor = TextEditor::new(&mut engine, "abc אבג", 20.0).unwrap();
    for _ in 0..4 {
        editor.edit(&mut engine, &RIGHT).unwrap();
    }
    let before = observe(&editor);
    assert!(
        !editor
            .edit(&mut engine, &TextEdit::Insert(String::new()))
            .unwrap()
            .changed()
    );
    assert_eq!(observe(&editor), before);
    editor
        .edit(&mut engine, &TextEdit::Insert("x".into()))
        .unwrap();
    assert_eq!(editor.text(), "abc xאבג");
    assert_eq!(editor.selection_bytes(), 5..5);
    let inserted = editor
        .layout()
        .runs()
        .iter()
        .find(|run| run.text_range().contains(&4))
        .unwrap();
    let glyph = editor.layout().glyphs()[inserted.glyph_range()]
        .last()
        .unwrap();
    assert!((editor.caret().unwrap().x0 - glyph.x() - glyph.advance()).abs() < 0.01);
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), before.text);
    assert_eq!(editor.caret(), before.caret);
    assert_eq!(editor.selection(), before.selection);
}

#[test]
fn preedit_replaces_selection_only_visually_and_cancel_restores_exact_selection() {
    let mut engine = engine(TextLimits::default());
    let mut editor = TextEditor::new(&mut engine, "AéZ", 18.0).unwrap();
    select_middle_backwards(&mut engine, &mut editor);
    assert_eq!(
        editor.selection(),
        TextSelection {
            anchor: 3,
            focus: 1
        }
    );
    let original = observe(&editor);

    let outcome = editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "拼音".into(),
                cursor: Some((3, 6)),
            },
        )
        .unwrap();
    assert!(!outcome.text_changed);
    assert!(outcome.display_changed && outcome.composition_changed);
    assert_eq!(editor.text(), "AéZ");
    assert_eq!(editor.display_text(), "A拼音Z");
    assert_eq!(editor.composition_range(), Some(1..7));
    assert_eq!(
        editor.selection(),
        TextSelection {
            anchor: 4,
            focus: 7
        }
    );
    assert!(!editor.can_undo());

    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "に".into(),
                cursor: None,
            },
        )
        .unwrap();
    assert_eq!(editor.text(), "AéZ");
    assert_eq!(editor.display_text(), "AにZ");
    assert!(editor.caret().is_none());
    assert!(editor.selection_rects().is_empty());
    assert_rect(editor.ime_area());
    editor
        .edit(&mut engine, &TextEdit::CancelComposition)
        .unwrap();
    assert_eq!(observe(&editor), original);

    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "x".into(),
                cursor: Some((1, 1)),
            },
        )
        .unwrap();
    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: String::new(),
                cursor: Some((0, 0)),
            },
        )
        .unwrap();
    assert_eq!(observe(&editor), original);
}

#[test]
fn ime_commit_replaces_original_selection_once_and_is_one_undo_step() {
    let mut engine = engine(TextLimits::default());
    let mut editor = TextEditor::new(&mut engine, "LéR", 18.0).unwrap();
    select_middle_backwards(&mut engine, &mut editor);
    let original = observe(&editor);
    let retained_layout = editor.snapshot().clone();

    for text in ["中", "候选"] {
        editor
            .edit(
                &mut engine,
                &TextEdit::Preedit {
                    text: text.into(),
                    cursor: Some((text.len(), text.len())),
                },
            )
            .unwrap();
        assert_eq!(editor.text(), "LéR");
        assert!(!editor.can_undo());
    }
    let outcome = editor
        .edit(&mut engine, &TextEdit::Commit("😀!".into()))
        .unwrap();
    assert!(outcome.text_changed && outcome.composition_changed);
    assert_eq!(editor.text(), "L😀!R");
    assert_eq!(editor.display_text(), "L😀!R");
    assert_eq!(editor.selection_bytes(), "L😀!".len().."L😀!".len());
    assert!(!editor.is_composing());
    assert_eq!(retained_layout, original.layout);

    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), original.text);
    assert_eq!(editor.selection(), original.selection);
    assert_eq!(editor.caret(), original.caret);
    assert_eq!(
        editor.selection_rects(),
        original.selection_rects.as_slice()
    );
    assert!(!editor.can_undo());
    assert!(editor.can_redo());
    let restored = observe(&editor);
    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "draft".into(),
                cursor: None,
            },
        )
        .unwrap();
    assert!(editor.can_redo());
    editor
        .edit(&mut engine, &TextEdit::CancelComposition)
        .unwrap();
    assert_eq!(observe(&editor), restored);
    editor.edit(&mut engine, &TextEdit::Redo).unwrap();
    assert_eq!(editor.text(), "L😀!R");
    assert!(!editor.can_redo());
}

#[test]
fn cancel_restores_bidi_cursor_affinity_and_clones_own_independent_compositions() {
    let mut engine = engine(TextLimits::default());
    let mut editor = TextEditor::new(&mut engine, "abc אבג", 18.0).unwrap();
    let rtl_run = editor
        .snapshot()
        .runs()
        .iter()
        .find(|run| run.direction() == TextDirection::RightToLeft)
        .unwrap();
    let glyph = editor.snapshot().glyphs()[rtl_run.glyph_range()]
        .iter()
        .filter(|glyph| glyph.advance() > 0.0)
        .min_by(|first, second| first.x().total_cmp(&second.x()))
        .unwrap()
        .clone();
    let line = editor.snapshot().lines()[rtl_run.line_index()].clone();
    editor
        .edit(
            &mut engine,
            &TextEdit::Point {
                x: glyph.x() + glyph.advance() * 0.2,
                y: (line.block_min() + line.block_max()) * 0.5,
                extend: false,
            },
        )
        .unwrap();
    let original = observe(&editor);
    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "x".into(),
                cursor: Some((1, 1)),
            },
        )
        .unwrap();
    let composing = observe(&editor);
    let mut independent = editor.clone();
    independent
        .edit(&mut engine, &TextEdit::Commit("y".into()))
        .unwrap();
    assert_ne!(independent.text(), original.text);
    assert_eq!(observe(&editor), composing);
    editor
        .edit(&mut engine, &TextEdit::CancelComposition)
        .unwrap();
    assert_eq!(observe(&editor), original);
}

#[test]
fn normal_actions_cancel_preedit_before_editing_or_navigating_the_committed_buffer() {
    let mut engine = engine(TextLimits::default());
    let mut editor = TextEditor::new(&mut engine, "AéZ", 18.0).unwrap();
    select_middle_backwards(&mut engine, &mut editor);
    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "draft".into(),
                cursor: None,
            },
        )
        .unwrap();
    editor
        .edit(&mut engine, &TextEdit::Insert("!".into()))
        .unwrap();
    assert_eq!(editor.text(), "A!Z");
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), "AéZ");
    assert_eq!(
        editor.selection(),
        TextSelection {
            anchor: 3,
            focus: 1
        }
    );
    assert!(!editor.can_undo());

    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "temporary".into(),
                cursor: None,
            },
        )
        .unwrap();
    editor
        .edit(&mut engine, &TextEdit::End { extend: false })
        .unwrap();
    assert_eq!(editor.text(), "AéZ");
    assert_eq!(editor.display_text(), "AéZ");
    assert_eq!(editor.selection_bytes(), 4..4);
    assert!(!editor.is_composing());
    assert!(editor.can_redo());
}

#[test]
fn controls_non_finite_points_and_invalid_preedit_offsets_leave_every_observation_unchanged() {
    let mut engine = engine(TextLimits {
        max_lines: 1,
        ..TextLimits::default()
    });
    let mut editor = TextEditor::new(&mut engine, "AéZ", 18.0).unwrap();
    select_middle_backwards(&mut engine, &mut editor);
    assert_eq!(editor.snapshot().lines().len(), 1);
    for scalar in ['\r', '\n', '\t', '\0', '\u{85}', '\u{2028}', '\u{2029}'] {
        assert!(matches!(
            TextEditor::new(&mut engine, &format!("a{scalar}b"), 18.0),
            Err(TextEditError::InvalidSingleLineText { byte_index: 1, scalar: found })
                if found == scalar
        ));
        for action in [
            TextEdit::Insert(scalar.to_string()),
            TextEdit::Commit(scalar.to_string()),
            TextEdit::Preedit {
                text: scalar.to_string(),
                cursor: None,
            },
        ] {
            let before = observe(&editor);
            assert!(matches!(
                editor.edit(&mut engine, &action),
                Err(TextEditError::InvalidSingleLineText { scalar: found, .. }) if found == scalar
            ));
            assert_eq!(observe(&editor), before);
        }
    }

    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "é".into(),
                cursor: Some((2, 2)),
            },
        )
        .unwrap();
    let composing = observe(&editor);
    for cursor in [(1, 2), (0, 1), (3, 0), (0, usize::MAX)] {
        assert!(matches!(
            editor.edit(
                &mut engine,
                &TextEdit::Preedit {
                    text: "é".into(),
                    cursor: Some(cursor)
                },
            ),
            Err(TextEditError::InvalidCompositionCursor { .. })
        ));
        assert_eq!(observe(&editor), composing);
    }
    for (pointer_x, pointer_y) in [
        (f32::NAN, 0.0),
        (0.0, f32::INFINITY),
        (f32::NEG_INFINITY, 1.0),
    ] {
        assert!(matches!(
            editor.edit(
                &mut engine,
                &TextEdit::Point {
                    x: pointer_x,
                    y: pointer_y,
                    extend: true
                },
            ),
            Err(TextEditError::InvalidPoint { .. })
        ));
        assert_eq!(observe(&editor), composing);
    }
    editor
        .edit(&mut engine, &TextEdit::Commit("ok".into()))
        .unwrap();
    assert_eq!(editor.text(), "AokZ");
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), "AéZ");
    assert_eq!(
        editor.selection(),
        TextSelection {
            anchor: 3,
            focus: 1
        }
    );
}

#[test]
fn utf8_byte_quota_counts_the_replacement_result_and_retry_keeps_original_history() {
    let limits = TextLimits {
        max_text_bytes: 4,
        ..TextLimits::default()
    };
    let mut engine = engine(limits);
    assert!(matches!(
        TextEditor::new(&mut engine, "ééx", 18.0),
        Err(TextEditError::Layout(TextLayoutError::TextBytesExceeded {
            actual: 5,
            maximum: 4
        }))
    ));
    let mut editor = TextEditor::new(&mut engine, "éé", 18.0).unwrap();
    editor
        .edit(&mut engine, &TextEdit::End { extend: false })
        .unwrap();
    let before = observe(&editor);
    for action in [
        TextEdit::Insert("x".into()),
        TextEdit::Commit("x".into()),
        TextEdit::Preedit {
            text: "x".into(),
            cursor: Some((1, 1)),
        },
    ] {
        assert!(matches!(
            editor.edit(&mut engine, &action),
            Err(TextEditError::Layout(TextLayoutError::TextBytesExceeded {
                actual: 5,
                maximum: 4
            }))
        ));
        assert_eq!(observe(&editor), before);
    }
    editor.edit(&mut engine, &TextEdit::SelectAll).unwrap();
    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "🧪".into(),
                cursor: Some((4, 4)),
            },
        )
        .unwrap();
    assert_eq!(editor.text(), "éé");
    assert_eq!(editor.display_text(), "🧪");
    let composing = observe(&editor);
    assert!(matches!(
        editor.edit(&mut engine, &TextEdit::Commit("🧪x".into())),
        Err(TextEditError::Layout(TextLayoutError::TextBytesExceeded {
            actual: 5,
            maximum: 4
        }))
    ));
    assert_eq!(observe(&editor), composing);
    editor
        .edit(&mut engine, &TextEdit::Commit("äx".into()))
        .unwrap();
    assert_eq!(editor.text(), "äx");
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), "éé");
    assert_eq!(editor.selection_bytes(), 0..4);
    assert!(!editor.can_undo());
}

#[test]
fn rejected_glyph_layouts_preserve_preedit_original_selection_and_redo_for_retry() {
    let limits = TextLimits {
        max_glyphs: 2,
        ..TextLimits::default()
    };
    let mut engine = engine(limits);
    let mut editor = TextEditor::new(&mut engine, "a", 18.0).unwrap();
    editor
        .edit(&mut engine, &TextEdit::End { extend: false })
        .unwrap();
    editor
        .edit(&mut engine, &TextEdit::Insert("b".into()))
        .unwrap();
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    editor.edit(&mut engine, &TextEdit::SelectAll).unwrap();
    let before = observe(&editor);
    assert!(editor.can_redo());
    for action in [
        TextEdit::Insert("xyz".into()),
        TextEdit::Preedit {
            text: "xyz".into(),
            cursor: Some((3, 3)),
        },
    ] {
        assert!(matches!(
            editor.edit(&mut engine, &action),
            Err(TextEditError::Layout(TextLayoutError::GlyphLimitExceeded {
                maximum: 2,
                ..
            }))
        ));
        assert_eq!(observe(&editor), before);
    }

    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "xy".into(),
                cursor: Some((2, 2)),
            },
        )
        .unwrap();
    let composing = observe(&editor);
    for action in [
        TextEdit::Preedit {
            text: "xyz".into(),
            cursor: Some((3, 3)),
        },
        TextEdit::Commit("xyz".into()),
    ] {
        assert!(matches!(
            editor.edit(&mut engine, &action),
            Err(TextEditError::Layout(TextLayoutError::GlyphLimitExceeded {
                maximum: 2,
                ..
            }))
        ));
        assert_eq!(observe(&editor), composing);
    }
    editor
        .edit(&mut engine, &TextEdit::CancelComposition)
        .unwrap();
    assert_eq!(observe(&editor), before);
    editor.edit(&mut engine, &TextEdit::Redo).unwrap();
    assert_eq!(editor.text(), "ab");
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), "a");
    assert!(!editor.can_undo());
}

#[test]
fn undo_and_redo_reject_changed_engine_limits_without_consuming_history() {
    let mut engine = engine(TextLimits::default());
    let mut editor = TextEditor::new(&mut engine, "abcd", 18.0).unwrap();
    editor
        .edit(&mut engine, &TextEdit::End { extend: false })
        .unwrap();
    editor.edit(&mut engine, &TextEdit::Backspace).unwrap();
    let before = observe(&editor);
    let mut narrow = TextEngine::new(
        bundled_font_face().unwrap(),
        TextLimits {
            max_glyphs: 3,
            ..TextLimits::default()
        },
    )
    .unwrap();
    assert!(matches!(
        editor.edit(&mut narrow, &TextEdit::Undo),
        Err(TextEditError::EngineMismatch)
    ));
    assert_eq!(observe(&editor), before);
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), "abcd");

    let before = observe(&editor);
    let mut narrower = TextEngine::new(
        bundled_font_face().unwrap(),
        TextLimits {
            max_glyphs: 2,
            ..TextLimits::default()
        },
    )
    .unwrap();
    assert!(matches!(
        editor.edit(&mut narrower, &TextEdit::Redo),
        Err(TextEditError::EngineMismatch)
    ));
    assert_eq!(observe(&editor), before);
    editor.edit(&mut engine, &TextEdit::Redo).unwrap();
    assert_eq!(editor.text(), "abc");
}

#[test]
fn an_equal_primary_does_not_authorize_different_fallback_faces_or_fallback_order() {
    let primary = bundled_font_face().unwrap();
    let fallback_bytes = include_bytes!("fixtures/FiraMono-Medium.ttf");
    let fallback = FontFace::from_bytes(fallback_bytes.to_vec(), 0).unwrap();
    let mut second_bytes = fallback_bytes.to_vec();
    second_bytes.push(1);
    let second_fallback = FontFace::from_bytes(second_bytes, 0).unwrap();
    let mut third_bytes = fallback_bytes.to_vec();
    third_bytes.push(2);
    let third_fallback = FontFace::from_bytes(third_bytes, 0).unwrap();
    let fonts = vec![primary.clone(), fallback.clone(), second_fallback.clone()];
    let limits = TextLimits::default();
    let mut engine = TextEngine::with_fonts(fonts.clone(), limits).unwrap();
    let mut editor = TextEditor::new(&mut engine, "A∩", 18.0).unwrap();
    assert!(
        editor
            .snapshot()
            .glyphs()
            .iter()
            .any(|glyph| glyph.key().face_id() == fallback.id())
    );
    editor
        .edit(&mut engine, &TextEdit::End { extend: false })
        .unwrap();
    editor
        .edit(&mut engine, &TextEdit::Insert("x".into()))
        .unwrap();
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    editor.edit(&mut engine, &TextEdit::SelectAll).unwrap();
    let original = observe(&editor);
    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "draft".into(),
                cursor: None,
            },
        )
        .unwrap();
    let composing = observe(&editor);

    let mut different_fallback = TextEngine::with_fonts(
        vec![primary.clone(), fallback.clone(), third_fallback],
        limits,
    )
    .unwrap();
    let mut different_order =
        TextEngine::with_fonts(vec![primary.clone(), second_fallback, fallback], limits).unwrap();
    assert_eq!(different_fallback.font_face_id(), engine.font_face_id());
    assert_eq!(different_order.font_face_id(), engine.font_face_id());
    for mismatched in [&mut different_fallback, &mut different_order] {
        for action in [
            TextEdit::Commit("replacement".into()),
            TextEdit::CancelComposition,
            TextEdit::Undo,
            TextEdit::Redo,
            TextEdit::Point {
                x: f32::NAN,
                y: 0.0,
                extend: false,
            },
        ] {
            assert_eq!(
                editor.edit(mismatched, &action),
                Err(TextEditError::EngineMismatch)
            );
            assert_eq!(observe(&editor), composing);
        }
    }

    let mut equivalent = TextEngine::with_fonts(fonts, limits).unwrap();
    editor
        .edit(&mut equivalent, &TextEdit::CancelComposition)
        .unwrap();
    assert_eq!(observe(&editor), original);
    editor.edit(&mut equivalent, &TextEdit::Redo).unwrap();
    assert_eq!(editor.text(), "A∩x");
}

#[test]
fn local_history_retains_only_the_last_32_edits_and_an_accepted_branch_discards_redo() {
    let limits = TextLimits {
        max_text_bytes: 40,
        ..TextLimits::default()
    };
    let mut engine = engine(limits);
    let mut editor = TextEditor::new(&mut engine, "", 18.0).unwrap();
    for _ in 0..35 {
        editor
            .edit(&mut engine, &TextEdit::Insert("x".into()))
            .unwrap();
    }
    for _ in 0..32 {
        assert!(editor.can_undo());
        editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    }
    assert_eq!(editor.text(), "xxx");
    assert!(!editor.can_undo());
    assert!(!editor.edit(&mut engine, &TextEdit::Undo).unwrap().changed());
    for _ in 0..32 {
        assert!(editor.can_redo());
        editor.edit(&mut engine, &TextEdit::Redo).unwrap();
    }
    assert_eq!(editor.text(), "x".repeat(35));
    assert!(!editor.can_redo());
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    let before = observe(&editor);
    assert!(matches!(
        editor.edit(&mut engine, &TextEdit::Insert("1234567".into())),
        Err(TextEditError::Layout(TextLayoutError::TextBytesExceeded {
            maximum: 40,
            ..
        }))
    ));
    assert_eq!(observe(&editor), before);
    editor
        .edit(&mut engine, &TextEdit::Insert("!".into()))
        .unwrap();
    assert!(!editor.can_redo());
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), "x".repeat(34));
}

#[test]
fn constructor_refuses_invalid_size_without_poisoning_later_editors() {
    let mut engine = engine(TextLimits::default());
    for size in [0.0, -1.0, f32::NAN, f32::INFINITY, 256.01] {
        assert!(matches!(
            TextEditor::new(&mut engine, "valid", size),
            Err(TextEditError::Layout(
                TextLayoutError::InvalidLogicalSize { .. }
            ))
        ));
    }
    let editor = TextEditor::new(&mut engine, "valid", 18.0).unwrap();
    assert!(!editor.snapshot().glyphs().is_empty());
}

fn engine(limits: TextLimits) -> TextEngine {
    TextEngine::new(bundled_font_face().unwrap(), limits).unwrap()
}

fn select_middle_backwards(engine: &mut TextEngine, editor: &mut TextEditor) {
    editor
        .edit(engine, &TextEdit::Home { extend: false })
        .unwrap();
    editor.edit(engine, &RIGHT).unwrap();
    editor.edit(engine, &RIGHT).unwrap();
    editor
        .edit(
            engine,
            &TextEdit::MoveLeft {
                extend: true,
                word: false,
            },
        )
        .unwrap();
}

fn assert_rect(rect: TextRect) {
    assert!(
        [rect.x0, rect.y0, rect.x1, rect.y1]
            .into_iter()
            .all(f32::is_finite)
    );
    assert!(rect.x0 <= rect.x1);
    assert!(rect.y0 <= rect.y1);
}

#[derive(Debug, PartialEq)]
struct ObservedEditor {
    text: String,
    display_text: String,
    layout: TextLayout,
    selection: TextSelection,
    caret: Option<TextRect>,
    selection_rects: Vec<TextRect>,
    ime_area: TextRect,
    composition_range: Option<Range<usize>>,
    can_undo: bool,
    can_redo: bool,
}

fn observe(editor: &TextEditor) -> ObservedEditor {
    ObservedEditor {
        text: editor.text().into(),
        display_text: editor.display_text().into(),
        layout: editor.snapshot().clone(),
        selection: editor.selection(),
        caret: editor.caret(),
        selection_rects: editor.selection_rects().into(),
        ime_area: editor.ime_area(),
        composition_range: editor.composition_range(),
        can_undo: editor.can_undo(),
        can_redo: editor.can_redo(),
    }
}
