use super::*;
use nara_ui_widgets::UiDensity;

#[test]
fn complete_inspector_adopts_scoped_appearance_without_replacing_live_unicode_draft() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut controller, &mut fields, &model);
    let field = input(&controller, &frame, "label");
    let text = format!("{}x", "café".repeat(819));
    assert_eq!(text.len(), 4096);
    replace(&mut controller, &mut fields, &frame, &field, &text);
    let frame = paint(&mut controller, &mut fields, &model);
    let token = controller.input_token(&frame, &field).unwrap().unwrap();
    controller
        .edit(&frame, &field, token, &TextEdit::SelectAll, &mut fields)
        .unwrap();
    controller
        .edit(
            &frame,
            &field,
            token,
            &TextEdit::Preedit {
                text: "préedit".into(),
                cursor: Some((0, 8)),
            },
            &mut fields,
        )
        .unwrap();
    let mut previous = paint(&mut controller, &mut fields, &model);
    for (density, scale) in [
        (UiDensity::Compact, 1.0),
        (UiDensity::Compact, 2.0),
        (UiDensity::Normal, 2.0),
        (UiDensity::Normal, 1.0),
    ] {
        let appearance = UiAppearance::default()
            .with_density(density)
            .with_text_scale(scale)
            .unwrap();
        controller.set_appearance(appearance);
        let current = paint(&mut controller, &mut fields, &model);
        assert!(!Arc::ptr_eq(&previous, &current));
        assert_eq!(input(&controller, &current, "label"), field);
        assert_eq!(
            controller.input_token(&current, &field).unwrap(),
            Some(token)
        );
        assert!(controller.is_composing(&current, &field, token).unwrap());
        assert_eq!(
            current
                .semantics()
                .iter()
                .find(|node| node.key() == &field)
                .unwrap()
                .value(),
            Some(text.as_str())
        );
        assert_eq!(controller.appearance, appearance);
        previous = current;
    }
    controller
        .edit(
            &previous,
            &field,
            token,
            &TextEdit::CancelComposition,
            &mut fields,
        )
        .unwrap();
    controller
        .edit(&previous, &field, token, &TextEdit::Undo, &mut fields)
        .unwrap();
    let undo = paint(&mut controller, &mut fields, &model);
    assert_eq!(
        undo.semantics()
            .iter()
            .find(|node| node.key() == &field)
            .unwrap()
            .value(),
        Some("before")
    );
    controller
        .edit(&undo, &field, token, &TextEdit::Redo, &mut fields)
        .unwrap();
    let redo = paint(&mut controller, &mut fields, &model);
    assert_eq!(
        redo.semantics()
            .iter()
            .find(|node| node.key() == &field)
            .unwrap()
            .value(),
        Some(text.as_str())
    );
    assert_eq!(
        application.current_frame().unwrap().revision(),
        model.revision()
    );
}
