use nara_text::{FontFace, TextEdit, TextEditError, TextEngine, TextLimits, bundled_font_face};
use nara_ui_kernel::UiRootKey;
use nara_ui_widgets::{UiTextField, UiTextFieldError};

fn fallback() -> FontFace {
    FontFace::from_bytes(
        include_bytes!("../../nara_text/tests/fixtures/FiraMono-Medium.ttf").to_vec(),
        0,
    )
    .unwrap()
}

#[test]
fn explicit_font_refresh_keeps_disabled_state_and_never_revives_a_retired_focus() {
    let limits = TextLimits::default();
    let mut original = TextEngine::new(bundled_font_face().unwrap(), limits).unwrap();
    let mut replacement = TextEngine::new(fallback(), limits).unwrap();
    let root = UiRootKey::runtime("font-refresh", "disabled").unwrap();
    let mut field = UiTextField::new(root, "Value", "original", 18.0, &mut original).unwrap();
    let key = field.key().clone();
    let retired = field.focus().unwrap();
    field
        .edit(retired, &TextEdit::SelectAll, &mut original)
        .unwrap();
    field
        .edit(retired, &TextEdit::Insert("draft".into()), &mut original)
        .unwrap();
    field
        .edit(
            retired,
            &TextEdit::Preedit {
                text: "pending".into(),
                cursor: None,
            },
            &mut original,
        )
        .unwrap();
    field.set_enabled(false, &mut original).unwrap();
    assert_eq!(field.editor().display_text(), "draft");
    let selection = field.editor().selection();
    let outcome = field.reconfigure_fonts(&mut replacement).unwrap();
    assert!(outcome.display_changed);
    assert!(!outcome.text_changed);
    assert!(!outcome.composition_changed);
    assert_eq!(field.key(), &key);
    assert_eq!(field.editor().text(), "draft");
    assert_eq!(field.editor().selection(), selection);
    assert!(!field.enabled());
    assert_eq!(field.input_token(), None);
    assert!(matches!(field.focus(), Err(UiTextFieldError::Disabled)));
    assert!(matches!(
        field.edit(retired, &TextEdit::Commit("stale".into()), &mut replacement),
        Err(UiTextFieldError::StaleInput)
    ));

    field.set_enabled(true, &mut replacement).unwrap();
    let current = field.input_token().unwrap();
    assert_ne!(retired, current);
    field
        .edit(current, &TextEdit::Undo, &mut replacement)
        .unwrap();
    assert_eq!(field.editor().text(), "original");
    assert!(!field.editor().can_undo());
    field
        .edit(current, &TextEdit::Redo, &mut replacement)
        .unwrap();
    assert_eq!(field.editor().text(), "draft");
}

#[test]
fn refused_font_refresh_keeps_live_composition_and_accepts_retry_on_the_same_focus() {
    let limits = TextLimits::default();
    let mut original = TextEngine::new(bundled_font_face().unwrap(), limits).unwrap();
    let mut refused = TextEngine::new(
        fallback(),
        TextLimits {
            max_text_bytes: 4,
            ..limits
        },
    )
    .unwrap();
    let mut replacement = TextEngine::new(fallback(), limits).unwrap();
    let mut field = UiTextField::new(
        UiRootKey::runtime("font-refresh", "retry").unwrap(),
        "Value",
        "buffer",
        18.0,
        &mut original,
    )
    .unwrap();
    let key = field.key().clone();
    let token = field.focus().unwrap();
    field
        .edit(token, &TextEdit::SelectAll, &mut original)
        .unwrap();
    field
        .edit(
            token,
            &TextEdit::Preedit {
                text: "candidate".into(),
                cursor: Some((0, 4)),
            },
            &mut original,
        )
        .unwrap();
    let selection = field.editor().selection();
    let caret = field.editor().caret();
    let paint = field.editor().paint_snapshot();
    assert!(matches!(
        field.reconfigure_fonts(&mut refused),
        Err(UiTextFieldError::Text(TextEditError::EngineMismatch))
    ));
    assert_eq!(field.key(), &key);
    assert_eq!(field.input_token(), Some(token));
    assert!(field.enabled());
    assert_eq!(field.editor().text(), "buffer");
    assert_eq!(field.editor().display_text(), "candidate");
    assert_eq!(field.editor().selection(), selection);
    assert_eq!(field.editor().caret(), caret);
    assert!(field.editor().paint_snapshot().same_source(&paint));

    field.reconfigure_fonts(&mut replacement).unwrap();
    assert_eq!(field.input_token(), Some(token));
    assert!(field.editor().is_composing());
    assert_eq!(field.editor().text(), "buffer");
    assert_eq!(field.editor().display_text(), "candidate");
    field
        .edit(token, &TextEdit::CancelComposition, &mut replacement)
        .unwrap();
    assert_eq!(field.editor().text(), "buffer");
    assert!(!field.editor().can_undo());
    field
        .edit(
            token,
            &TextEdit::Commit("accepted".into()),
            &mut replacement,
        )
        .unwrap();
    field
        .edit(token, &TextEdit::Undo, &mut replacement)
        .unwrap();
    assert_eq!(field.editor().text(), "buffer");
    assert!(!field.editor().can_undo());
}
