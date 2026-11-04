use super::*;

fn text_form(
    owner: &mut OwnedUiHierarchy,
    routing: &mut RoutedInput,
    fixture: &Fixture,
) -> Arc<UiFrame> {
    click_current(owner, routing, fixture, "Add UI node");
    click_current(owner, routing, fixture, "Kind: Text");
    paint(owner, fixture, 800.0)
}

#[test]
fn long_refocus_uses_presented_text_origin_and_stale_tokens_refuse() {
    let fixture = Fixture::new();
    let source = fixture.source();
    let mut owner = controller();
    let frame = paint(&mut owner, &fixture, 800.0);
    let mut routing = RoutedInput::new(&frame);
    let form = text_form(&mut owner, &mut routing, &fixture);
    let key = field(&form, "Text");
    let long = "wide value ".repeat(70);
    let original_token = set_field(&mut owner, &mut routing, &form, "Text", &long);
    let focused = paint(&mut owner, &fixture, 800.0);
    let input = owner
        .retained(&focused)
        .unwrap()
        .inputs
        .iter()
        .find(|input| input.key == key)
        .unwrap();
    assert!(owner.field(Field::Text).unwrap().editor().layout().width() > input.width);
    routing.focus(&mut owner, &focused, &button(&focused, "Choose placement"));
    assert_eq!(owner.input_token(&focused, &key).unwrap(), None);
    let unfocused = paint(&mut owner, &fixture, 800.0);
    routing.focus(&mut owner, &unfocused, &key);
    let token = owner.input_token(&unfocused, &key).unwrap().unwrap();
    assert_ne!(token, original_token);
    assert!(
        owner
            .point(
                &unfocused,
                &key,
                original_token,
                UiPoint::new(6.0, 12.0),
                false
            )
            .is_err()
    );
    owner
        .point(&unfocused, &key, token, UiPoint::new(6.0, 12.0), false)
        .unwrap();
    owner
        .edit(
            &unfocused,
            &key,
            token,
            &TextEdit::Insert("prefix-".to_owned()),
        )
        .unwrap();
    assert_eq!(
        owner.form.as_ref().unwrap().value(Field::Text),
        format!("prefix-{long}")
    );
    assert!(matches!(
        owner.point(&unfocused, &key, token, UiPoint::new(6.0, 12.0), false),
        Err(OwnedUiHierarchyError::ChangedProjection)
    ));
    assert_eq!(fixture.source(), source);
    assert_eq!(fixture.history(), 0);
}

#[test]
fn hidden_text_blur_cancels_ime_chooser_escape_preserves_form_and_finish_discard_retires_it() {
    let fixture = Fixture::new();
    let mut owner = controller();
    let frame = paint(&mut owner, &fixture, 800.0);
    let mut routing = RoutedInput::new(&frame);
    let form = text_form(&mut owner, &mut routing, &fixture);
    let key = field(&form, "Text");
    let token = set_field(&mut owner, &mut routing, &form, "Text", "Committed draft");
    owner
        .edit(
            &form,
            &key,
            token,
            &TextEdit::Preedit {
                text: "uncommitted".to_owned(),
                cursor: None,
            },
        )
        .unwrap();
    let composing = paint(&mut owner, &fixture, 800.0);
    assert!(owner.is_composing(&composing, &key, token).unwrap());
    assert!(!enabled(&composing, "Apply structure change"));
    assert!(owner.selected_text(&composing, &key, token).is_err());
    assert!(owner.ime_area(&composing, &key, token).is_ok());
    let tiny = paint(&mut owner, &fixture, 80.0);
    assert!(!owner.field(Field::Text).unwrap().editor().is_composing());
    assert!(
        owner
            .edit(
                &composing,
                &key,
                token,
                &TextEdit::Commit("late".to_owned())
            )
            .is_err()
    );
    assert!(
        tiny.semantics()
            .iter()
            .all(|node| node.role() != UiSemanticRole::TextInput)
    );
    assert_eq!(
        owner.form.as_ref().unwrap().value(Field::Text),
        "Committed draft"
    );
    let current = paint(&mut owner, &fixture, 800.0);
    let form_id = owner.form.as_ref().unwrap().identity;
    click(
        &mut owner,
        &mut routing,
        &current,
        "Choose placement",
        &fixture.model(),
        &fixture.ui(),
    );
    let choices = paint(&mut owner, &fixture, 800.0);
    assert!(owner.escape(&choices).unwrap());
    assert!(owner.chooser.is_none());
    assert_eq!(owner.form.as_ref().unwrap().identity, form_id);
    assert_eq!(
        owner.form.as_ref().unwrap().value(Field::Text),
        "Committed draft"
    );
    let intact = paint(&mut owner, &fixture, 800.0);
    routing.focus(&mut owner, &intact, &key);
    let token = owner.input_token(&intact, &key).unwrap().unwrap();
    owner.finish_input(&intact, &key, true).unwrap();
    assert!(owner.form.is_none());
    assert!(
        owner
            .edit(&intact, &key, token, &TextEdit::Insert("late".to_owned()))
            .is_err()
    );
    let tree = paint(&mut owner, &fixture, 800.0);
    assert!(!owner.escape(&tree).unwrap());
    assert_eq!(fixture.history(), 0);
}

#[test]
fn full_owner_text_capacity_survives_paint_copy_history_and_exact_submission() {
    let mut fixture = Fixture::new();
    let mut owner = controller();
    let frame = paint(&mut owner, &fixture, 800.0);
    let mut routing = RoutedInput::new(&frame);
    let form = text_form(&mut owner, &mut routing, &fixture);
    let key = field(&form, "Text");
    routing.focus(&mut owner, &form, &key);
    let token = owner.input_token(&form, &key).unwrap().unwrap();
    owner
        .edit(&form, &key, token, &TextEdit::SelectAll)
        .unwrap();
    let oversized = "x".repeat(FIELD_TEXT_BYTES + 1);
    assert!(
        owner
            .edit(&form, &key, token, &TextEdit::Insert(oversized))
            .is_err()
    );
    assert_eq!(owner.form.as_ref().unwrap().value(Field::Text), "Text");
    assert!(owner.input_error.as_ref().unwrap().contains("4096"));
    let accepted = "y".repeat(FIELD_TEXT_BYTES);
    owner
        .edit(&form, &key, token, &TextEdit::Insert(accepted.clone()))
        .unwrap();
    let repaired = paint(&mut owner, &fixture, 800.0);
    assert!(enabled(&repaired, "Apply structure change"));
    assert!(owner.input_error.is_none());
    assert!(
        matches!(owner.form.as_ref().unwrap().intent().unwrap(), UiStructureIntent::Create { parameters: UiCreateParameters::Text { content }, .. } if content == accepted)
    );
    owner
        .edit(&repaired, &key, token, &TextEdit::SelectAll)
        .unwrap();
    assert_eq!(
        owner
            .selected_text(&repaired, &key, token)
            .unwrap()
            .as_deref(),
        Some(accepted.as_str())
    );
    owner.edit(&repaired, &key, token, &TextEdit::Undo).unwrap();
    owner.edit(&repaired, &key, token, &TextEdit::Redo).unwrap();
    assert_eq!(owner.form.as_ref().unwrap().value(Field::Text), accepted);
    assert_eq!(fixture.history(), 0);
    let prepared =
        click_current(&mut owner, &mut routing, &fixture, "Apply structure change").unwrap();
    apply(&mut owner, &mut fixture, prepared, 1);
    let ui = fixture.ui();
    let selected = ui.selection().top_entity().unwrap();
    assert!(ui.projection().unwrap().fields().iter().any(|descriptor| {
        descriptor.key().entity() == selected && descriptor.label() == "Content"
            && matches!(descriptor.value(), nara::ui::__private::UiAuthoringFieldValue::Text(value) if value == &accepted)
    }));
    assert_eq!(fixture.history(), 1);
}
