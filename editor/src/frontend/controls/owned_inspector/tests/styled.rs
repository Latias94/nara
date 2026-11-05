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
    let key = field_key(&controller, &previous, &field);
    let basis = fields.drafts[&key].basis.clone();
    let editor = controller.controls[&key].text().unwrap().editor();
    let selection = editor.selection();
    let composition = editor.composition_range();
    let preedit = editor.display_text().to_owned();
    for (density, scale, width) in [
        (UiDensity::Compact, 1.0, 220.0),
        (UiDensity::Compact, 2.0, 1200.0),
        (UiDensity::Normal, 2.0, 220.0),
        (UiDensity::Normal, 1.0, 1200.0),
    ] {
        let appearance = UiAppearance::default()
            .with_density(density)
            .with_text_scale(scale)
            .unwrap();
        controller.set_appearance(appearance);
        let current = paint_size(&mut controller, &mut fields, &model, width, 1600.0);
        assert!(!Arc::ptr_eq(&previous, &current));
        assert_eq!(input(&controller, &current, "label"), field);
        assert_eq!(
            controller.input_token(&current, &field).unwrap(),
            Some(token)
        );
        assert!(controller.is_composing(&current, &field, token).unwrap());
        let editor = controller.controls[&key].text().unwrap().editor();
        assert_eq!(editor.selection(), selection);
        assert_eq!(editor.composition_range(), composition);
        assert_eq!(editor.display_text(), preedit);
        let rect = controller.accepted_input_rect(&current, &field).unwrap();
        assert_eq!(
            controller.ime_area(&current, &field, token).unwrap(),
            controller.controls[&key]
                .text()
                .unwrap()
                .ime_area(rect.size().width())
                .unwrap(),
        );
        assert_eq!(fields.drafts[&key].basis, basis);
        assert_eq!(fields.drafts[&key].candidate.copy_text(), text);
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
