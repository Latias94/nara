use super::structure::{complete_exactly, repaint, selected_value};
use super::*;
use nara::{
    identity::EntityReference,
    scene::{SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord},
};

const FIELD: &str = "references";

fn reference(entity: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: SceneEntityId::new(entity).unwrap(),
    })
}

fn control(
    harness: &mut Harness,
    panel: EditorPanelInstanceId,
    _: &str,
    role: UiSemanticRole,
    name: &str,
) -> (Arc<UiFrame>, UiKey) {
    for delta in [-48.0, 48.0] {
        for _ in 0..128 {
            let frame = Arc::clone(
                harness.frontend.presentation.widgets[&panel]
                    .owned_inspector
                    .as_ref()
                    .unwrap()
                    .compiled_frame()
                    .unwrap(),
            );
            if let Some(group) = frame
                .semantics()
                .iter()
                .find(|node| node.role() == UiSemanticRole::Group && node.name() == FIELD)
                && let Some(node) = frame.semantics().iter().find(|node| {
                    node.role() == role && node.name() == name && node.parent() == Some(group.key())
                })
            {
                return (Arc::clone(&frame), node.key().clone());
            }
            if !harness
                .frontend
                .scroll_owned_panel(panel, &frame, delta)
                .unwrap()
            {
                break;
            }
            repaint(harness);
        }
    }
    panic!("missing presented {FIELD}/{name}");
}

fn button(
    harness: &mut Harness,
    panel: EditorPanelInstanceId,
    name: &str,
) -> (Arc<UiFrame>, UiKey) {
    control(harness, panel, FIELD, UiSemanticRole::Button, name)
}

fn action(harness: &mut Harness, panel: EditorPanelInstanceId, _: &str, name: &str) {
    let (frame, button) = button(harness, panel, name);
    activate(harness, panel, &frame, &button).unwrap();
    repaint(harness);
}

fn replace_leaf(
    harness: &mut Harness,
    panel: EditorPanelInstanceId,
    _: &str,
    name: &str,
    value: &str,
) -> (Arc<UiFrame>, UiKey, UiTextInputToken) {
    let (frame, input) = control(harness, panel, FIELD, UiSemanticRole::TextInput, name);
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
    _: &str,
    operation: InspectorOperation,
) -> Result<(), String> {
    button(harness, panel, operation.label());
    super::structure::operation(harness, panel, FIELD, operation)
}

fn activate(
    harness: &mut Harness,
    panel: EditorPanelInstanceId,
    frame: &Arc<UiFrame>,
    button: &UiKey,
) -> Result<(), OwnedInspectorError> {
    let model = harness.application.current_frame().unwrap();
    let widgets = harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap();
    let edit = widgets.owned_inspector.as_mut().unwrap().activate(
        frame,
        button,
        &model,
        &mut widgets.authoring.fields,
    )?;
    assert!(edit.is_none());
    assert!(widgets.owned_pending.is_empty());
    Ok(())
}

#[test]
fn nested_choice_retires_only_its_leaf_and_rejects_reopened_menu_events() {
    let (mut harness, panel) = setup_project(InteractionProject::new_nested_entity_references());
    let original = harness.application.current_frame().unwrap();
    let initial = selected_value(&harness, FIELD);
    action(&mut harness, panel, FIELD, "root: Edit value");
    let (_, sibling, _) = replace_leaf(&mut harness, panel, FIELD, "root.other", "after");
    let (initial_frame, direct) = control(
        &mut harness,
        panel,
        FIELD,
        UiSemanticRole::TextInput,
        "root.direct",
    );
    let token = harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .owned_inspector
        .as_mut()
        .unwrap()
        .focus(&initial_frame, &direct)
        .unwrap();
    action(&mut harness, panel, FIELD, "root.direct: Choose target");
    let (open, old_choice) = button(&mut harness, panel, "root.direct: Door [other]");
    let (_, toggle) = button(&mut harness, panel, "root.direct: Hide targets");
    action(&mut harness, panel, FIELD, "root.direct: Hide targets");
    action(&mut harness, panel, FIELD, "root.direct: Choose target");
    let (reopened, choice) = button(&mut harness, panel, "root.direct: Door [other]");
    assert_ne!(choice, old_choice);
    assert_eq!(
        button(&mut harness, panel, "root.direct: Hide targets").1,
        toggle
    );
    assert!(matches!(
        activate(&mut harness, panel, &open, &toggle),
        Err(OwnedInspectorError::ChangedProjection)
    ));
    assert!(activate(&mut harness, panel, &open, &old_choice).is_err());
    activate(&mut harness, panel, &reopened, &choice).unwrap();
    assert!(
        harness
            .frontend
            .owned_edit(
                panel,
                &initial_frame,
                &direct,
                token,
                &TextEdit::Insert("late".into())
            )
            .is_err()
    );
    repaint(&mut harness);
    assert_eq!(selected_value(&harness, FIELD), initial);
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        original.workspace().active_scene
    );
    let (frame, unchanged) = control(
        &mut harness,
        panel,
        FIELD,
        UiSemanticRole::TextInput,
        "root.other",
    );
    assert_eq!(unchanged, sibling);
    let sibling_token = harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .owned_inspector
        .as_mut()
        .unwrap()
        .focus(&frame, &unchanged)
        .unwrap();
    harness
        .frontend
        .owned_edit(panel, &frame, &unchanged, sibling_token, &TextEdit::Undo)
        .unwrap();
    repaint(&mut harness);
    let ComponentValue::Map(mut expected) = initial else {
        panic!("reference Record")
    };
    expected.insert("direct".into(), reference("other"));
    operation(&mut harness, panel, FIELD, InspectorOperation::Apply).unwrap();
    complete_exactly(&mut harness, panel, FIELD, ComponentValue::Map(expected));
}

#[test]
fn pending_reference_picker_cannot_target_cancelled_or_confirmed_children() {
    let (mut harness, panel) = setup_project(InteractionProject::new_nested_entity_references());
    let initial = selected_value(&harness, FIELD);
    action(&mut harness, panel, FIELD, "root: Edit value");
    action(&mut harness, panel, FIELD, "root.optional: Some...");
    action(
        &mut harness,
        panel,
        FIELD,
        "root.optional.new: Choose target",
    );
    let (old_frame, old_choice) = button(&mut harness, panel, "root.optional.new: Door [other]");
    action(&mut harness, panel, FIELD, "root.optional: Cancel Some");
    assert!(activate(&mut harness, panel, &old_frame, &old_choice).is_err());
    action(&mut harness, panel, FIELD, "root.optional: Some...");
    action(
        &mut harness,
        panel,
        FIELD,
        "root.optional.new: Choose target",
    );
    let (replacement, choice) = button(&mut harness, panel, "root.optional.new: Door [other]");
    assert_ne!(choice, old_choice);
    assert!(activate(&mut harness, panel, &old_frame, &old_choice).is_err());
    activate(&mut harness, panel, &replacement, &choice).unwrap();
    repaint(&mut harness);
    assert!(operation(&mut harness, panel, FIELD, InspectorOperation::Apply).is_err());
    assert_eq!(selected_value(&harness, FIELD), initial);
    action(&mut harness, panel, FIELD, "root.optional: Use Some");
    assert!(activate(&mut harness, panel, &replacement, &choice).is_err());
    let ComponentValue::Map(mut expected) = initial else {
        panic!("reference Record")
    };
    expected.insert("optional".into(), reference("other"));
    operation(&mut harness, panel, FIELD, InspectorOperation::Apply).unwrap();
    complete_exactly(&mut harness, panel, FIELD, ComponentValue::Map(expected));
}

#[test]
fn nested_reference_pages_keep_choice_authority_when_the_input_is_clipped() {
    let project = InteractionProject::new_nested_entity_references();
    let source = project.root.join("worlds/entry.scene.json");
    let document = SceneDocumentCandidate::decode_json_bytes(&std::fs::read(&source).unwrap())
        .unwrap()
        .into_composition_source()
        .unwrap();
    let entities = document.entities.into_iter().chain((0..64).map(|index| {
        SceneEntityRecord::new(SceneEntityId::new(format!("target-{index:02}")).unwrap())
    }));
    std::fs::write(
        &source,
        SceneDocument::from_ordered_roots(entities)
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let (mut harness, panel) = setup_project(project);
    let initial = selected_value(&harness, FIELD);
    action(&mut harness, panel, FIELD, "root: Edit value");
    let (_, direct) = control(
        &mut harness,
        panel,
        FIELD,
        UiSemanticRole::TextInput,
        "root.direct",
    );
    action(&mut harness, panel, FIELD, "root.direct: Choose target");
    let (first, first_choice) = button(&mut harness, panel, "root.direct: Door [other]");
    action(&mut harness, panel, FIELD, "root.direct: Next targets");
    assert!(activate(&mut harness, panel, &first, &first_choice).is_err());
    let (frame, _) = button(&mut harness, panel, "root.direct: target-39 [target-39]");
    let direct_rect = frame
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(&direct))
        .unwrap()
        .rect();
    harness
        .frontend
        .scroll_owned_panel(
            panel,
            &frame,
            direct_rect.origin().y() + direct_rect.size().height() + 1.0,
        )
        .unwrap();
    repaint(&mut harness);
    let (frame, choice) = button(&mut harness, panel, "root.direct: target-39 [target-39]");
    assert!(!frame.semantics().iter().any(|node| node.key() == &direct));
    assert!(
        harness
            .frontend
            .owned_field_token(panel, &frame, &direct)
            .is_none()
    );
    let inspector = harness.frontend.presentation.widgets[&panel]
        .owned_inspector
        .as_ref()
        .unwrap();
    let choices = frame
        .semantics()
        .iter()
        .filter(|node| {
            matches!(
                inspector.binding(&frame, node.key()),
                Ok(InspectorBinding::Reference {
                    input: Some(_),
                    action: ReferenceAction::Choose(_),
                    ..
                })
            )
        })
        .count();
    assert!(choices > 0 && choices <= 32);
    activate(&mut harness, panel, &frame, &choice).unwrap();
    harness
        .frontend
        .scroll_owned_panel(panel, &frame, -f32::MAX)
        .unwrap();
    repaint(&mut harness);
    let ComponentValue::Map(mut expected) = initial else {
        panic!("reference Record")
    };
    expected.insert("direct".into(), reference("target-39"));
    operation(&mut harness, panel, FIELD, InspectorOperation::Apply).unwrap();
    complete_exactly(&mut harness, panel, FIELD, ComponentValue::Map(expected));
}
