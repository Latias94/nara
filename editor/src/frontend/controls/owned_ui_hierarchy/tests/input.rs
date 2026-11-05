use nara_ui_kernel::{
    UiButtonPhase, UiNonActivation, UiPointerButton, UiPointerId, UiPointerInput,
};

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

impl RoutedInput {
    fn present_pointer_frame(&mut self, owner: &mut OwnedUiHierarchy, frame: &Arc<UiFrame>) {
        let invalidated = self
            .presented
            .as_ref()
            .filter(|(previous, _)| owner.retains_frame(previous))
            .map(|(previous, _)| owner.invalidated_controls(previous, frame).unwrap())
            .unwrap_or_default();
        self.epoch += 1;
        let receipt = self
            .authority
            .submit(&mut self.journal, Arc::clone(frame), self.epoch)
            .unwrap();
        self.presented = Some((Arc::clone(frame), receipt));
        assert!(Arc::ptr_eq(
            self.journal
                .latest(frame.environment().surface_id())
                .unwrap()
                .frame(),
            frame,
        ));
        if let Some(batch) = self
            .session
            .reconcile_latest_presentation(
                &self.journal,
                frame.environment().surface_id(),
                &invalidated,
            )
            .unwrap()
        {
            assert!(owner.dispatch(frame, &batch).unwrap().intents().is_empty());
        }
    }

    fn pointer_button(
        &mut self,
        owner: &mut OwnedUiHierarchy,
        frame: &Arc<UiFrame>,
        point: UiPoint,
        phase: UiButtonPhase,
    ) -> UiIntentBatch {
        let (presented, receipt) = self.presented.as_ref().unwrap();
        assert!(Arc::ptr_eq(presented, frame));
        let outcome = self
            .session
            .dispatch(
                &self.journal,
                UiRouteStamp::from_receipt(*receipt),
                UiInputEvent::Pointer(UiPointerInput::button(
                    UiSeatId::PRIMARY,
                    UiPointerId::PRIMARY,
                    UiPointerButton::Primary,
                    phase,
                    Some(point),
                )),
            )
            .unwrap();
        if let Some(batch) = outcome.consumed() {
            assert!(!batch.is_reconciliation());
            owner.dispatch(frame, batch).unwrap()
        } else {
            assert_eq!(phase, UiButtonPhase::Released);
            assert_eq!(
                outcome.rejection(),
                Some(&UiNonActivation::NoPointerCapture)
            );
            UiIntentBatch::default()
        }
    }

    fn pointer_pressed(&self, key: &UiKey) -> bool {
        let (_, receipt) = self.presented.as_ref().unwrap();
        self.session
            .semantic_snapshot(
                &self.journal,
                UiRouteStamp::from_receipt(*receipt),
                UiSeatId::PRIMARY,
            )
            .unwrap()
            .nodes()
            .iter()
            .any(|node| node.key() == key && node.state().pressed())
    }
}

fn pointer_button_point(frame: &UiFrame, key: &UiKey) -> UiPoint {
    let node = frame
        .semantics()
        .iter()
        .find(|node| node.key() == key)
        .unwrap();
    let rect = node
        .clip_rect()
        .map_or(Some(node.rect()), |clip| node.rect().intersect(clip))
        .unwrap();
    let point = UiPoint::new(
        rect.origin().x() + rect.size().width() / 2.0,
        rect.origin().y() + rect.size().height() / 2.0,
    );
    assert_eq!(frame.hit_test(point).unwrap().key(), key);
    point
}

#[test]
fn pointer_cancel_survives_composition_blur_and_presented_reconciliation_at_both_heights() {
    let fixture = Fixture::new();
    let original = fixture.model();
    let source = fixture.source();
    for height in [800.0, 208.0] {
        let mut owner = controller();
        let tree = paint(&mut owner, &fixture, height);
        let mut routing = RoutedInput::new(&tree);
        text_form(&mut owner, &mut routing, &fixture);
        let form = paint(&mut owner, &fixture, height);
        let text = field(&form, "Text");
        let token = set_field(&mut owner, &mut routing, &form, "Text", "Cancelled draft");
        owner
            .edit(
                &form,
                &text,
                token,
                &TextEdit::Preedit {
                    text: "uncommitted".to_owned(),
                    cursor: None,
                },
            )
            .unwrap();
        let composing = paint(&mut owner, &fixture, height);
        assert!(owner.is_composing(&composing, &text, token).unwrap());
        assert!(!enabled(&composing, "Apply structure change"));
        routing.present_pointer_frame(&mut owner, &composing);
        let cancel = button(&composing, "Cancel structure change");
        let point = pointer_button_point(&composing, &cancel);
        assert!(
            routing
                .pointer_button(&mut owner, &composing, point, UiButtonPhase::Pressed)
                .intents()
                .is_empty()
        );
        assert!(routing.pointer_pressed(&cancel));
        assert!(owner.controls[&Control::Cancel].state.pressed());
        assert!(!owner.field(Field::Text).unwrap().editor().is_composing());
        assert_eq!(owner.input_token(&composing, &text).unwrap(), None);

        let blurred = paint(&mut owner, &fixture, height);
        assert!(enabled(&blurred, "Apply structure change"));
        assert_eq!(button(&blurred, "Cancel structure change"), cancel);
        assert_eq!(pointer_button_point(&blurred, &cancel), point);
        assert!(
            !owner
                .invalidated_controls(&composing, &blurred)
                .unwrap()
                .contains(&cancel)
        );
        if height == 208.0 {
            assert!(owner.retained(&composing).unwrap().scroll_limit > 0.0);
            assert_eq!(owner.retained(&blurred).unwrap().scroll_limit, 0.0);
        }
        routing.present_pointer_frame(&mut owner, &blurred);
        assert!(routing.pointer_pressed(&cancel));
        assert!(owner.controls[&Control::Cancel].state.pressed());
        let intents = routing.pointer_button(&mut owner, &blurred, point, UiButtonPhase::Released);
        assert_eq!(intents.intents().len(), 1);
        assert_eq!(intents.intents()[0].target(), &cancel);
        assert!(
            owner
                .activate(&blurred, &cancel, &fixture.model(), &fixture.ui(), false)
                .unwrap()
                .is_none()
        );
        assert!(owner.form.is_none());
        let tree = paint(&mut owner, &fixture, height);
        routing.present_pointer_frame(&mut owner, &tree);
        assert!(enabled(&tree, "Add UI node"));
        assert!(
            tree.semantics()
                .iter()
                .all(|node| node.role() != UiSemanticRole::TextInput)
        );
        assert!(
            owner
                .edit(
                    &composing,
                    &text,
                    token,
                    &TextEdit::Commit("late".to_owned())
                )
                .is_err()
        );
        assert_eq!(fixture.model().workspace(), original.workspace());
        assert_eq!(fixture.source(), source);
        assert_eq!(fixture.history(), 0);
    }
}

#[test]
fn pointer_cancel_refuses_cancelled_input_changed_recipes_replaced_forms_and_reopened_sources() {
    enum Invalidation {
        InputCancelled,
        RecipeChanged,
        FormReplaced,
        SourceReopened,
    }

    let mut fixture = Fixture::new();
    let source = fixture.source();
    for height in [800.0, 208.0] {
        for invalidation in [
            Invalidation::InputCancelled,
            Invalidation::RecipeChanged,
            Invalidation::FormReplaced,
            Invalidation::SourceReopened,
        ] {
            let mut owner = controller();
            let tree = paint(&mut owner, &fixture, height);
            let mut routing = RoutedInput::new(&tree);
            text_form(&mut owner, &mut routing, &fixture);
            let form = paint(&mut owner, &fixture, height);
            routing.present_pointer_frame(&mut owner, &form);
            let cancel = button(&form, "Cancel structure change");
            let point = pointer_button_point(&form, &cancel);
            let identity = owner.form.as_ref().unwrap().identity;
            assert!(
                routing
                    .pointer_button(&mut owner, &form, point, UiButtonPhase::Pressed)
                    .intents()
                    .is_empty()
            );
            assert!(routing.pointer_pressed(&cancel));
            match invalidation {
                Invalidation::InputCancelled => {
                    owner.cancel_input().unwrap();
                    assert_eq!(owner.form.as_ref().unwrap().identity, identity);
                }
                Invalidation::RecipeChanged => {
                    assert!(
                        owner
                            .activate(
                                &form,
                                &button(&form, "Kind: Button"),
                                &fixture.model(),
                                &fixture.ui(),
                                false,
                            )
                            .unwrap()
                            .is_none()
                    );
                    assert_eq!(owner.form.as_ref().unwrap().identity, identity);
                }
                Invalidation::FormReplaced => {
                    let form = owner.form.as_ref().unwrap();
                    let StructureDialog::Create {
                        kind,
                        placement,
                        image,
                    } = &form.dialog
                    else {
                        panic!("the pointer fixture opens a creation form");
                    };
                    let basis = form.basis.clone();
                    let replacement = StructureDialog::Create {
                        kind: *kind,
                        placement: placement.clone(),
                        image: image.clone(),
                    };
                    owner.open_form(basis, replacement).unwrap();
                    assert_ne!(owner.form.as_ref().unwrap().identity, identity);
                }
                Invalidation::SourceReopened => {
                    let original = fixture.ui();
                    let outcome =
                        fixture.send(EditorAction::Persistence(EditorPersistenceAction::Reopen {
                            target: fixture.model().reopen_target(),
                        }));
                    assert!(matches!(
                        outcome,
                        EditorActionOutcome::Persistence(
                            nara::tooling::EditorPersistenceRequestResult::Accepted { .. }
                        )
                    ));
                    fixture.settle();
                    assert_ne!(
                        fixture.ui().revision().source_id(),
                        original.revision().source_id()
                    );
                    let nara::tooling::EditorPersistenceView::Terminal { attempt, .. } =
                        fixture.model().persistence()
                    else {
                        panic!("the source reopen has an exact terminal result");
                    };
                    let outcome = fixture.send(EditorAction::Persistence(
                        EditorPersistenceAction::AcknowledgeResult { attempt },
                    ));
                    assert!(matches!(outcome, EditorActionOutcome::Persistence(
                        nara::tooling::EditorPersistenceRequestResult::Acknowledged { attempt: actual }
                    ) if actual == attempt));
                }
            }
            let successor = paint(&mut owner, &fixture, height);
            let successor_form = owner.form.as_ref().map(|form| form.identity);
            if successor_form.is_some() {
                assert_eq!(button(&successor, "Cancel structure change"), cancel);
                assert_eq!(pointer_button_point(&successor, &cancel), point);
            } else {
                assert!(enabled(&successor, "Add UI node"));
            }
            assert!(
                owner
                    .invalidated_controls(&form, &successor)
                    .unwrap()
                    .contains(&cancel)
            );
            routing.present_pointer_frame(&mut owner, &successor);
            assert!(!routing.pointer_pressed(&cancel));
            let before_release = fixture.model();
            assert!(
                routing
                    .pointer_button(&mut owner, &successor, point, UiButtonPhase::Released)
                    .intents()
                    .is_empty()
            );
            assert_eq!(
                owner.form.as_ref().map(|form| form.identity),
                successor_form
            );
            assert!(
                owner
                    .activate(&form, &cancel, &fixture.model(), &fixture.ui(), false)
                    .is_err()
            );
            assert_eq!(fixture.model().workspace(), before_release.workspace());
            assert_eq!(fixture.source(), source);
            assert_eq!(fixture.history(), 0);
        }
    }
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
