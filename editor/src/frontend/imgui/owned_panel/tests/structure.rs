use super::*;

fn repaint(harness: &mut Harness) {
    let candidate = harness.draw();
    harness.submit(candidate);
}

fn control(
    harness: &Harness,
    panel: EditorPanelInstanceId,
    field: &str,
    role: UiSemanticRole,
    name: &str,
) -> (Arc<UiFrame>, UiKey) {
    let inspector = harness.frontend.presentation.widgets[&panel]
        .owned_inspector
        .as_ref()
        .unwrap();
    let frame = Arc::clone(inspector.compiled_frame().unwrap());
    let group = frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::Group && node.name() == field)
        .unwrap();
    let node = frame
        .semantics()
        .iter()
        .find(|node| {
            node.role() == role && node.name() == name && node.parent() == Some(group.key())
        })
        .unwrap_or_else(|| panic!("missing {field}/{name}"));
    (Arc::clone(&frame), node.key().clone())
}

fn action(harness: &mut Harness, panel: EditorPanelInstanceId, field: &str, name: &str) {
    let (frame, button) = control(harness, panel, field, UiSemanticRole::Button, name);
    let model = harness.application.current_frame().unwrap();
    let widgets = harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap();
    let edit = widgets
        .owned_inspector
        .as_mut()
        .unwrap()
        .activate(&frame, &button, &model, &mut widgets.authoring.fields)
        .unwrap();
    assert!(edit.is_none());
    assert!(widgets.owned_pending.is_empty());
    repaint(harness);
}

fn replace_leaf(
    harness: &mut Harness,
    panel: EditorPanelInstanceId,
    field: &str,
    name: &str,
    value: &str,
) -> (Arc<UiFrame>, UiKey, UiTextInputToken) {
    let (frame, input) = control(harness, panel, field, UiSemanticRole::TextInput, name);
    let token = harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .owned_inspector
        .as_mut()
        .unwrap()
        .focus(&frame, &input)
        .unwrap();
    harness
        .frontend
        .owned_edit(panel, &frame, &input, token, &TextEdit::SelectAll)
        .unwrap();
    harness
        .frontend
        .owned_edit(
            panel,
            &frame,
            &input,
            token,
            &TextEdit::Insert(value.into()),
        )
        .unwrap();
    repaint(harness);
    (frame, input, token)
}

fn operation(
    harness: &mut Harness,
    panel: EditorPanelInstanceId,
    field: &str,
    operation: InspectorOperation,
) -> Result<(), String> {
    let (frame, button) = control(
        harness,
        panel,
        field,
        UiSemanticRole::Button,
        operation.label(),
    );
    let InspectorBinding::Operation { field: owner, .. } = harness.frontend.presentation.widgets
        [&panel]
        .owned_inspector
        .as_ref()
        .unwrap()
        .binding(&frame, &button)
        .unwrap()
    else {
        panic!("the footer must retain its original field owner")
    };
    let model = harness.application.current_frame().unwrap();
    harness
        .frontend
        .owned_operation(panel, &frame, &owner, operation, &model)
}

fn selected_value(harness: &Harness, field: &str) -> ComponentValue {
    harness
        .application
        .current_frame()
        .unwrap()
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .selected_entity_view
        .as_ref()
        .unwrap()
        .components
        .iter()
        .find(|component| component.component.as_str() == CUSTOM_COMPONENT)
        .unwrap()
        .fields
        .iter()
        .find(|current| current.id.as_str() == field)
        .unwrap()
        .value
        .clone()
        .unwrap()
}

fn complete_exactly(
    harness: &mut Harness,
    panel: EditorPanelInstanceId,
    field: &str,
    expected: ComponentValue,
) {
    let candidate = harness.draw();
    let [submission] = candidate.ledger.submissions.as_slice() else {
        panic!("whole-field Apply must retain one exact submission")
    };
    let slot = submission.slot;
    let id = submission.id;
    let target = candidate.batch.first_field_target().unwrap();
    let key = FieldKey::from_target(&target);
    assert_eq!(key.field.as_str(), field);
    assert_eq!(
        harness.frontend.presentation.widgets[&panel]
            .authoring
            .fields
            .drafts[&key]
            .pending,
        Some(id)
    );
    let dispatch = harness
        .frontend
        .dispatch_submission(&mut harness.application, candidate)
        .unwrap();
    assert!(
        matches!(dispatch.original_outcome(slot), Some(EditorActionOutcome::Workspace(report)) if report.applied)
    );
    harness
        .application
        .drive_editor_frame(Duration::ZERO, dispatch.actions_observed(), true);
    harness
        .frontend
        .finish_dispatch(&harness.application, &dispatch);
    assert!(
        !harness.frontend.presentation.widgets[&panel]
            .authoring
            .fields
            .drafts
            .contains_key(&key)
    );
    assert_eq!(selected_value(harness, field), expected);
    repaint(harness);
}

#[test]
fn structured_optional_retires_cancelled_input_and_reapplies_the_exact_retained_candidate() {
    let (mut harness, panel) = setup_project(InteractionProject::new_recursive());
    let original = harness.application.current_frame().unwrap();
    action(&mut harness, panel, "optional", "root: Edit value");
    action(&mut harness, panel, "optional", "root: Some...");
    let (old_frame, old_input, old_token) =
        replace_leaf(&mut harness, panel, "optional", "root.new", "1e100");
    let (frame, confirm) = control(
        &harness,
        panel,
        "optional",
        UiSemanticRole::Button,
        "root: Use Some",
    );
    assert!(
        !frame
            .semantics()
            .iter()
            .find(|node| node.key() == &confirm)
            .unwrap()
            .enabled()
    );
    assert!(operation(&mut harness, panel, "optional", InspectorOperation::Apply).is_err());
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        original.workspace().active_scene
    );
    action(&mut harness, panel, "optional", "root: Cancel Some");
    assert!(
        harness
            .frontend
            .owned_edit(
                panel,
                &old_frame,
                &old_input,
                old_token,
                &TextEdit::Insert("late".into())
            )
            .is_err()
    );
    action(&mut harness, panel, "optional", "root: Some...");
    let (frame, replacement) = control(
        &harness,
        panel,
        "optional",
        UiSemanticRole::TextInput,
        "root.new",
    );
    assert_ne!(replacement, old_input);
    assert!(
        harness
            .frontend
            .owned_edit(
                panel,
                &frame,
                &replacement,
                old_token,
                &TextEdit::Insert("late".into())
            )
            .is_err()
    );
    let (pending_frame, pending_input, pending_token) =
        replace_leaf(&mut harness, panel, "optional", "root.new", "7");
    action(&mut harness, panel, "optional", "root: Use Some");
    assert!(
        harness
            .frontend
            .owned_edit(
                panel,
                &pending_frame,
                &pending_input,
                pending_token,
                &TextEdit::Insert("late".into())
            )
            .is_err()
    );
    assert_eq!(selected_value(&harness, "optional"), ComponentValue::Null);

    let model = harness.application.current_frame().unwrap();
    let scene = model.workspace().active_scene.as_ref().unwrap();
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            model.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyInspectorCommand {
                    document: scene.document,
                    base_revision: scene.revision,
                    command: SceneInspectorCommand::SetField {
                        entity: scene.selection.top_entity().unwrap().clone(),
                        component: ComponentTypeId::new(CUSTOM_COMPONENT),
                        component_version: nara::reflect::ComponentSchemaVersion::ONE,
                        field: nara::reflect::ComponentFieldId::new("optional"),
                        value: ComponentValue::f64(2.0).unwrap(),
                    },
                },
            )],
        ),
        Duration::ZERO,
    );
    repaint(&mut harness);
    assert_eq!(
        selected_value(&harness, "optional"),
        ComponentValue::f64(2.0).unwrap()
    );
    assert!(operation(&mut harness, panel, "optional", InspectorOperation::Apply).is_err());
    operation(&mut harness, panel, "optional", InspectorOperation::Reapply).unwrap();
    complete_exactly(
        &mut harness,
        panel,
        "optional",
        ComponentValue::f64(7.0).unwrap(),
    );
}

#[test]
fn structured_list_removal_preserves_the_surviving_leaf_identity_and_local_undo() {
    let (mut harness, panel) = setup_project(InteractionProject::new_recursive());
    action(&mut harness, panel, "list", "root: Edit value");
    let (removed_frame, removed_input, removed_token) =
        replace_leaf(&mut harness, panel, "list", "root[0]", "4");
    action(&mut harness, panel, "list", "root: Add...");
    let (pending_frame, pending_input, pending_token) =
        replace_leaf(&mut harness, panel, "list", "root.new", "7");
    action(&mut harness, panel, "list", "root: Add value");
    assert!(
        harness
            .frontend
            .owned_edit(
                panel,
                &pending_frame,
                &pending_input,
                pending_token,
                &TextEdit::Insert("late".into())
            )
            .is_err()
    );
    let (_, surviving_input, surviving_token) =
        replace_leaf(&mut harness, panel, "list", "root[1]", "8");
    let (frame, _) = control(
        &harness,
        panel,
        "list",
        UiSemanticRole::TextInput,
        "root[1]",
    );
    harness
        .frontend
        .owned_edit(
            panel,
            &frame,
            &surviving_input,
            surviving_token,
            &TextEdit::Preedit {
                text: "uncommitted".into(),
                cursor: Some((0, 11)),
            },
        )
        .unwrap();
    repaint(&mut harness);
    let (composing, remove) = control(
        &harness,
        panel,
        "list",
        UiSemanticRole::Button,
        "root[0]: Remove item",
    );
    let model = harness.application.current_frame().unwrap();
    let widgets = harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap();
    let candidate = widgets
        .authoring
        .fields
        .drafts
        .values()
        .find(|draft| draft.basis.target.field().as_str() == "list")
        .unwrap()
        .candidate
        .clone();
    assert!(
        widgets
            .owned_inspector
            .as_mut()
            .unwrap()
            .activate(&composing, &remove, &model, &mut widgets.authoring.fields)
            .is_err()
    );
    assert_eq!(
        widgets
            .authoring
            .fields
            .drafts
            .values()
            .find(|draft| draft.basis.target.field().as_str() == "list")
            .unwrap()
            .candidate,
        candidate
    );
    assert!(operation(&mut harness, panel, "list", InspectorOperation::Apply).is_err());
    harness
        .frontend
        .owned_edit(
            panel,
            &composing,
            &surviving_input,
            surviving_token,
            &TextEdit::CancelComposition,
        )
        .unwrap();
    repaint(&mut harness);
    action(&mut harness, panel, "list", "root[0]: Remove item");
    let (frame, renamed) = control(
        &harness,
        panel,
        "list",
        UiSemanticRole::TextInput,
        "root[0]",
    );
    assert_eq!(renamed, surviving_input);
    assert!(
        harness
            .frontend
            .owned_edit(
                panel,
                &removed_frame,
                &removed_input,
                removed_token,
                &TextEdit::Insert("late".into())
            )
            .is_err()
    );
    assert_eq!(
        harness.frontend.owned_field_token(panel, &frame, &renamed),
        Some(surviving_token)
    );
    harness
        .frontend
        .owned_edit(panel, &frame, &renamed, surviving_token, &TextEdit::Undo)
        .unwrap();
    repaint(&mut harness);
    assert_eq!(
        selected_value(&harness, "list"),
        ComponentValue::List(vec![ComponentValue::U64(3)])
    );
    let (frame, renamed) = control(
        &harness,
        panel,
        "list",
        UiSemanticRole::TextInput,
        "root[0]",
    );
    assert_eq!(
        frame
            .semantics()
            .iter()
            .find(|node| node.key() == &renamed)
            .unwrap()
            .value(),
        Some("7")
    );
    operation(&mut harness, panel, "list", InspectorOperation::Apply).unwrap();
    complete_exactly(
        &mut harness,
        panel,
        "list",
        ComponentValue::List(vec![ComponentValue::U64(7)]),
    );
}
