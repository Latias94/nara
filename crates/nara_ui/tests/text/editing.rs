use nara_ui::text::{TextEdit, TextEditor, TextEngine, TextRect, bundled_font_face};

#[test]
fn preedit_paints_from_a_snapshot_and_commit_is_a_single_undo_step() {
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let mut editor = TextEditor::new(&mut engine, "office אבג", 18.0).unwrap();
    editor.edit(&mut engine, &TextEdit::SelectAll).unwrap();
    let initial_selection = editor.selection();
    editor
        .edit(
            &mut engine,
            &TextEdit::Preedit {
                text: "e\u{301}".into(),
                cursor: Some((3, 3)),
            },
        )
        .unwrap();
    assert_eq!(editor.text(), "office אבג");
    assert!(!editor.can_undo());
    let snapshot = editor.paint_snapshot();
    assert_eq!(snapshot.display_text(), "e\u{301}");
    let clipped = engine
        .prepare_window(
            &snapshot,
            TextRect {
                x0: 0.0,
                y0: 0.0,
                x1: snapshot.width() + 4.0,
                y1: snapshot.height() + 4.0,
            },
            1.5,
        )
        .unwrap();
    assert!(!clipped.glyphs().is_empty());
    assert!(
        clipped
            .glyphs()
            .iter()
            .all(|glyph| glyph.key().physical_size() == 27.0)
    );

    editor
        .edit(&mut engine, &TextEdit::Commit("e\u{301}".into()))
        .unwrap();
    assert!(!editor.is_composing());
    assert_eq!(editor.text(), "e\u{301}");
    editor.edit(&mut engine, &TextEdit::Undo).unwrap();
    assert_eq!(editor.text(), "office אבג");
    assert_eq!(editor.selection(), initial_selection);
    assert!(!editor.can_undo());
    assert_eq!(snapshot.display_text(), "e\u{301}");
}
