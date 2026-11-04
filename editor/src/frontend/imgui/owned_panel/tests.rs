use std::{collections::BTreeMap, sync::Arc, time::Duration};

use nara::{
    reflect::{ComponentTypeId, ComponentValue},
    scene::{ScenePlacement, ScenePlacementParent},
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorActionRejection,
        EditorDocumentMutation, SceneCompositionCommand, SceneCompositionEntityKind,
        SceneInspectorCommand,
    },
};
use nara_text::{FontFace, TextEdit, bundled_font_face};
use nara_ui_kernel::{UiFrame, UiKey, UiSemanticRole};
use nara_ui_widgets::{UiTextFieldError, UiTextInputToken};

use super::super::{
    ImguiEditorFrontend,
    field_drafts::FieldKey,
    interaction_tests::{CUSTOM_COMPONENT, InteractionProject},
    owned_inspector::{InspectorBinding, InspectorOperation, OwnedInspectorError, ReferenceAction},
    submission_tests::Harness,
};
use crate::workspace::EditorPanelInstanceId;

mod nested_references;
mod references;
mod structure;

fn fonts() -> Vec<FontFace> {
    let primary = bundled_font_face().unwrap();
    let fallback = FontFace::from_bytes(
        include_bytes!("../../../../../crates/nara_text/tests/fixtures/FiraMono-Medium.ttf")
            .to_vec(),
        0,
    )
    .unwrap();
    assert_ne!(primary.id(), fallback.id());
    vec![primary, fallback]
}

fn setup() -> (Harness, EditorPanelInstanceId) {
    setup_project(InteractionProject::new_field_drafts())
}

fn setup_project(project: InteractionProject) -> (Harness, EditorPanelInstanceId) {
    let mut harness = Harness::with_project(project, 0);
    let model = harness.application.current_frame().unwrap();
    let scene = model.workspace().active_scene.as_ref().unwrap();
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            model.revision(),
            [EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                document: scene.document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::CreateEntity {
                    kind: SceneCompositionEntityKind::Empty,
                    placement: ScenePlacement::last(ScenePlacementParent::Root),
                },
            })],
        ),
        Duration::ZERO,
    );
    let model = harness.application.current_frame().unwrap();
    let scene = model.workspace().active_scene.as_ref().unwrap();
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            model.revision(),
            [EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                document: scene.document,
                base_revision: scene.revision,
                command: SceneCompositionCommand::AddComponent {
                    entity: scene.selection.top_entity().unwrap().clone(),
                    component: ComponentTypeId::new(CUSTOM_COMPONENT),
                },
            })],
        ),
        Duration::ZERO,
    );
    let panel = harness
        .application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|instance| instance.kind() == "nara.inspector")
        .unwrap()
        .id();
    harness.frontend.set_owned_images(fonts(), BTreeMap::new());
    let candidate = harness.draw();
    harness.submit(candidate);
    (harness, panel)
}

fn input(
    frontend: &ImguiEditorFrontend,
    panel: EditorPanelInstanceId,
    name: &str,
) -> (Arc<UiFrame>, UiKey, FieldKey) {
    let inspector = frontend.presentation.widgets[&panel]
        .owned_inspector
        .as_ref()
        .unwrap();
    let frame = Arc::clone(inspector.compiled_frame().unwrap());
    let (field, key) = frame
        .semantics()
        .iter()
        .filter(|node| node.role() == UiSemanticRole::TextInput)
        .find_map(|node| match inspector.binding(&frame, node.key()) {
            Ok(InspectorBinding::Field(key))
                if key.component.as_str() == CUSTOM_COMPONENT && key.field.as_str() == name =>
            {
                Some((node.key().clone(), key))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing owned input: {name}"));
    (frame, field, key)
}

fn replace(
    frontend: &mut ImguiEditorFrontend,
    panel: EditorPanelInstanceId,
    name: &str,
    value: &str,
) -> (UiKey, FieldKey, UiTextInputToken) {
    let (frame, field, key) = input(frontend, panel, name);
    let token = frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .owned_inspector
        .as_mut()
        .unwrap()
        .focus(&frame, &field)
        .unwrap();
    frontend
        .owned_edit(panel, &frame, &field, token, &TextEdit::SelectAll)
        .unwrap();
    frontend
        .owned_edit(
            panel,
            &frame,
            &field,
            token,
            &TextEdit::Insert(value.into()),
        )
        .unwrap();
    (field, key, token)
}

#[test]
fn same_ordered_font_ids_retain_controls_and_dirty_input() {
    let (mut harness, panel) = setup();
    let (field, key, token) = replace(&mut harness.frontend, panel, "label", "retained");
    let candidate = harness.draw();
    harness.submit(candidate);
    let (frame, current, _) = input(&harness.frontend, panel, "label");
    assert_eq!(current, field);
    let widgets = &harness.frontend.presentation.widgets[&panel];
    let revision = widgets.owned_revision;
    let basis = widgets.authoring.fields.drafts[&key].basis.clone();

    harness.frontend.set_owned_images(fonts(), BTreeMap::new());

    let widgets = &harness.frontend.presentation.widgets[&panel];
    assert_eq!(widgets.owned_revision, revision);
    assert!(Arc::ptr_eq(
        widgets
            .owned_inspector
            .as_ref()
            .unwrap()
            .compiled_frame()
            .unwrap(),
        &frame,
    ));
    assert_eq!(
        harness.frontend.owned_field_token(panel, &frame, &field),
        Some(token),
    );
    assert_eq!(widgets.authoring.fields.drafts[&key].basis, basis);
    assert!(widgets.authoring.fields.drafts[&key].dirty);
    harness
        .frontend
        .owned_edit(panel, &frame, &field, token, &TextEdit::Insert("!".into()))
        .unwrap();
    assert_eq!(
        harness.frontend.presentation.widgets[&panel]
            .authoring
            .fields
            .drafts[&key]
            .candidate
            .copy_text(),
        "retained!",
    );
}

#[test]
fn reordered_fonts_preserve_queued_apply_and_drafts_after_admission_refusal() {
    let (mut harness, panel) = setup();
    let (value, value_key, _) = replace(&mut harness.frontend, panel, "value", "25");
    let (label, label_key, old_token) =
        replace(&mut harness.frontend, panel, "label", "unapplied label");
    let candidate = harness.draw();
    harness.submit(candidate);
    let (old_frame, _, _) = input(&harness.frontend, panel, "label");
    assert_eq!(
        harness
            .frontend
            .owned_field_token(panel, &old_frame, &label),
        Some(old_token),
    );
    let model = harness.application.current_frame().unwrap();
    let basis = harness.frontend.presentation.widgets[&panel]
        .authoring
        .fields
        .drafts[&value_key]
        .basis
        .clone();
    harness
        .frontend
        .owned_operation(panel, &old_frame, &value, InspectorOperation::Apply, &model)
        .unwrap();
    let mut reordered = harness.frontend.owned_fonts.clone();
    reordered.reverse();

    harness
        .frontend
        .set_owned_images(reordered, BTreeMap::new());

    let widgets = &harness.frontend.presentation.widgets[&panel];
    assert!(widgets.owned_inspector.is_none());
    assert_eq!(widgets.owned_revision, 0);
    let [super::OwnedSubmission::Field(queued)] = widgets.owned_pending.as_slice() else {
        panic!("font replacement must retain the queued Apply")
    };
    assert_eq!(queued.key, value_key);
    assert_eq!(queued.action.field_target(), Some(basis.target.clone()));
    assert!(matches!(
        &queued.action,
        EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
            command: SceneInspectorCommand::SetField {
                value: ComponentValue::U64(25),
                ..
            },
            ..
        })
    ));
    assert!(widgets.authoring.fields.drafts[&value_key].queued);
    assert!(widgets.authoring.fields.drafts[&value_key].dirty);
    assert_eq!(widgets.authoring.fields.drafts[&value_key].basis, basis);
    assert!(widgets.authoring.fields.drafts[&label_key].dirty);

    harness.frontend.next_submission = u64::MAX;
    let refused = harness.draw();
    assert!(refused.ledger.submissions.is_empty());
    assert!(refused.batch.first_field_target().is_none());
    let (frame, replacement, _) = input(&harness.frontend, panel, "label");
    assert_ne!(replacement, label);
    assert!(!Arc::ptr_eq(&frame, &old_frame));
    assert!(
        harness
            .frontend
            .owned_field_token(panel, &old_frame, &label)
            .is_none()
    );
    let widgets = harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap();
    assert!(widgets.owned_pending.is_empty());
    let draft = &widgets.authoring.fields.drafts[&value_key];
    assert!(draft.dirty && !draft.queued && draft.pending.is_none());
    assert_eq!(draft.basis, basis);
    assert_eq!(draft.candidate.copy_text(), "25");
    assert!(draft.error.as_deref().unwrap().contains("not queued"));
    assert!(matches!(
        widgets.owned_inspector.as_mut().unwrap().edit(
            &frame,
            &replacement,
            old_token,
            &TextEdit::Insert("late".into()),
            &mut widgets.authoring.fields,
        ),
        Err(OwnedInspectorError::TextField(UiTextFieldError::StaleInput))
    ));
    assert_eq!(
        widgets.authoring.fields.drafts[&label_key]
            .candidate
            .copy_text(),
        "unapplied label",
    );
    harness.submit(refused);
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        model.workspace().active_scene,
    );
}

#[test]
fn font_replacement_preserves_pending_ids_and_routes_exact_application_results() {
    let (mut harness, panel) = setup();
    let (value, value_key, _) = replace(&mut harness.frontend, panel, "value", "25");
    let (other, other_key, _) = replace(&mut harness.frontend, panel, "other", "7");
    let (_, label_key, _) = replace(&mut harness.frontend, panel, "label", "unapplied label");
    let candidate = harness.draw();
    harness.submit(candidate);
    let (frame, _, _) = input(&harness.frontend, panel, "value");
    let model = harness.application.current_frame().unwrap();
    for field in [&value, &other] {
        harness
            .frontend
            .owned_operation(panel, &frame, field, InspectorOperation::Apply, &model)
            .unwrap();
    }
    let pending = harness.draw();
    assert_eq!(pending.ledger.submissions.len(), 2);
    let widgets = &harness.frontend.presentation.widgets[&panel];
    let value_id = widgets.authoring.fields.drafts[&value_key].pending.unwrap();
    let other_id = widgets.authoring.fields.drafts[&other_key].pending.unwrap();
    assert_ne!(value_id, other_id);
    assert_eq!(pending.ledger.submissions[0].id, value_id);
    assert_eq!(pending.ledger.submissions[1].id, other_id);
    let incarnation = Arc::clone(&widgets.incarnation);
    let basis = widgets.authoring.fields.drafts[&other_key].basis.clone();
    let mut reordered = harness.frontend.owned_fonts.clone();
    reordered.reverse();

    harness
        .frontend
        .set_owned_images(reordered, BTreeMap::new());

    let widgets = &harness.frontend.presentation.widgets[&panel];
    assert!(widgets.owned_inspector.is_none());
    assert!(Arc::ptr_eq(&widgets.incarnation, &incarnation));
    assert!(Arc::ptr_eq(
        harness.frontend.pending.as_ref().unwrap(),
        &pending.ledger,
    ));
    for (key, id) in [(&value_key, value_id), (&other_key, other_id)] {
        let draft = &widgets.authoring.fields.drafts[key];
        assert!(draft.dirty && !draft.queued);
        assert_eq!(draft.pending, Some(id));
    }
    let dispatch = harness
        .frontend
        .dispatch_submission(&mut harness.application, pending)
        .unwrap();
    assert!(matches!(
        dispatch.original_outcome(0),
        Some(EditorActionOutcome::Workspace(report)) if report.applied
    ));
    assert!(matches!(
        dispatch.original_outcome(1),
        Some(EditorActionOutcome::Rejected(EditorActionRejection::StaleDocumentFact {
            document,
            expected,
            actual: Some(actual),
        })) if *document == other_key.document
            && *expected == basis.target.base_revision()
            && actual != expected
    ));
    harness
        .application
        .drive_editor_frame(Duration::ZERO, dispatch.actions_observed(), true);
    harness
        .frontend
        .finish_dispatch(&harness.application, &dispatch);

    let widgets = &harness.frontend.presentation.widgets[&panel];
    assert!(!widgets.authoring.fields.drafts.contains_key(&value_key));
    let retained = &widgets.authoring.fields.drafts[&other_key];
    assert!(retained.dirty && !retained.queued && retained.pending.is_none());
    assert_eq!(retained.basis, basis);
    assert_eq!(retained.candidate.copy_text(), "7");
    assert!(retained.error.is_some());
    let unapplied = &widgets.authoring.fields.drafts[&label_key];
    assert!(unapplied.dirty && !unapplied.in_flight());
    assert_eq!(unapplied.candidate.copy_text(), "unapplied label");
    assert!(unapplied.error.is_none());
    assert!(harness.frontend.pending.is_none());
}
