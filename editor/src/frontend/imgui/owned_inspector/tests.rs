use std::{
    fs,
    time::{Duration, Instant},
};

use nara::{
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, PersistentComponentSchemaOwnerId,
    },
    scene::{ScenePlacement, ScenePlacementParent},
    tooling::{
        EditorActionBatch, EditorFieldTarget, SceneCompositionCommand, SceneCompositionEntityKind,
    },
};
use nara_text::bundled_font_face;
use nara_ui_kernel::{UiPaintPrimitive, UiSurfaceId};

use super::super::{
    PanelActionDispatch, PanelActionNotDispatched, PanelSubmitError,
    interaction_tests::{CUSTOM_COMPONENT, InteractionProject},
    submission::{self, PanelOrigin, Recipient},
};
use super::*;
use crate::{EditorApplication, EditorApplicationActionDispatch, EditorLaunchOptions};

fn application() -> (InteractionProject, EditorApplication) {
    let project = InteractionProject::new();
    let mut reset = ComponentFieldSchema::required(
        ComponentFieldId::new("reset"),
        "Reset",
        ComponentFieldPath::from_fields(["reset"]),
        ComponentValueKind::U64,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    reset.default_value = Some(ComponentValue::U64(7));
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        "Owned Inspector",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::U64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("label"),
            "Label",
            ComponentFieldPath::from_fields(["label"]),
            ComponentValueKind::String,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("flag"),
            "Flag",
            ComponentFieldPath::from_fields(["flag"]),
            ComponentValueKind::Bool,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        reset,
    ])
    .with_authoring_initial_value(ComponentValue::map([
        ("value", ComponentValue::U64(10)),
        ("label", ComponentValue::String("before".into())),
        ("flag", ComponentValue::Bool(false)),
        ("reset", ComponentValue::U64(8)),
    ]));
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("studio.interaction.components").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    fs::write(
        project.root.join("catalogs/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(&project.root).with_profile("desktop"));
    application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_none() && Instant::now() < deadline {
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
    let model = application
        .current_frame()
        .expect("the ordinary fixture opens");
    let scene = model.workspace().active_scene.as_ref().unwrap();
    application.submit_and_drive(
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
    let model = application.current_frame().unwrap();
    let scene = model.workspace().active_scene.as_ref().unwrap();
    application.submit_and_drive(
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
    (project, application)
}

fn new_controller() -> OwnedInspector {
    OwnedInspector::new(
        UiRootKey::runtime("editor.inspector", "test-panel").unwrap(),
        vec![bundled_font_face().unwrap()],
    )
    .unwrap()
}

fn paint(
    controller: &mut OwnedInspector,
    fields: &mut FieldEditors,
    model: &EditorFrameModel,
) -> Arc<UiFrame> {
    fields.begin_paint(model);
    controller
        .compile(
            model,
            fields,
            UiEnvironment::new(
                UiSurfaceId::new(91).unwrap(),
                600.0,
                1600.0,
                1.0,
                1,
                1,
                1,
                1,
            )
            .unwrap(),
            UiTheme::default(),
            false,
        )
        .unwrap()
}

fn input(controller: &OwnedInspector, frame: &Arc<UiFrame>, field: &str) -> UiKey {
    frame.semantics().iter().find(|node| {
        node.role() == UiSemanticRole::TextInput
            && matches!(controller.binding(frame, node.key()), Ok(InspectorBinding::Field(key)) if key.field.as_str() == field)
    }).unwrap_or_else(|| panic!("missing owned input: {field}")).key().clone()
}

fn replace(
    controller: &mut OwnedInspector,
    fields: &mut FieldEditors,
    frame: &Arc<UiFrame>,
    field: &UiKey,
    value: &str,
) {
    let token = controller.focus(frame, field).unwrap();
    controller
        .edit(frame, field, token, &TextEdit::SelectAll, fields)
        .unwrap();
    controller
        .edit(frame, field, token, &TextEdit::Insert(value.into()), fields)
        .unwrap();
}

fn field_key(controller: &OwnedInspector, frame: &Arc<UiFrame>, field: &UiKey) -> FieldKey {
    let InspectorBinding::Field(key) = controller.binding(frame, field).unwrap() else {
        panic!("expected an actual field binding")
    };
    key
}

#[test]
fn compiled_controls_preserve_focus_across_redraw_and_reject_retired_input() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let original_revision = model.workspace().active_scene.as_ref().unwrap().revision;
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut controller, &mut fields, &model);
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    assert!(Arc::ptr_eq(controller.compiled_frame().unwrap(), &frame));
    assert_eq!(
        controller.authoring_snapshot().unwrap().roots()[0].key(),
        controller.root_key()
    );
    assert!(
        frame
            .paint()
            .iter()
            .any(|primitive| matches!(primitive, UiPaintPrimitive::Text { .. }))
    );
    assert_eq!(
        frame
            .semantics()
            .iter()
            .filter(|node| node.role() == UiSemanticRole::TextInput)
            .count(),
        3
    );
    assert!(!fields.drafts.keys().any(|key| key.field.as_str() == "flag"));
    let field = input(&controller, &frame, "label");
    let key = field_key(&controller, &frame, &field);
    let token = controller.focus(&frame, &field).unwrap();
    controller
        .edit(&frame, &field, token, &TextEdit::SelectAll, &mut fields)
        .unwrap();
    controller
        .edit(
            &frame,
            &field,
            token,
            &TextEdit::Preedit {
                text: "IME".into(),
                cursor: Some((0, 3)),
            },
            &mut fields,
        )
        .unwrap();
    assert!(controller.is_composing(&frame, &field, token).unwrap());
    assert!(!fields.drafts[&key].dirty);
    assert_eq!(fields.drafts[&key].candidate.copy_text(), "before");
    let frame = paint(&mut controller, &mut fields, &model);
    assert_eq!(input(&controller, &frame, "label"), field);
    assert_eq!(controller.input_token(&frame, &field).unwrap(), Some(token));
    assert_eq!(
        frame
            .semantics()
            .iter()
            .find(|node| node.key() == &field)
            .unwrap()
            .value(),
        Some("before")
    );
    controller.cancel_input().unwrap();
    assert!(controller.is_composing(&frame, &field, token).is_err());
    assert_eq!(fields.drafts[&key].candidate.copy_text(), "before");
    assert!(matches!(
        controller.edit(
            &frame,
            &field,
            token,
            &TextEdit::Commit("late".into()),
            &mut fields
        ),
        Err(OwnedInspectorError::TextField(UiTextFieldError::StaleInput))
    ));
    replace(&mut controller, &mut fields, &frame, &field, "retained");
    let token = controller.input_token(&frame, &field).unwrap().unwrap();
    controller.set_enabled(&frame, &field, false).unwrap();
    controller.set_enabled(&frame, &field, true).unwrap();
    assert!(matches!(
        controller.edit(
            &frame,
            &field,
            token,
            &TextEdit::Insert("late".into()),
            &mut fields
        ),
        Err(OwnedInspectorError::TextField(UiTextFieldError::StaleInput))
    ));
    let frame = paint(&mut controller, &mut fields, &model);
    assert!(
        controller
            .operate(
                &frame,
                &field,
                InspectorOperation::Discard,
                &model,
                &mut fields
            )
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        controller.focus(&frame, &field),
        Err(OwnedInspectorError::RetiredField)
    ));
    let replacement = paint(&mut controller, &mut fields, &model);
    let new_field = input(&controller, &replacement, "label");
    assert_ne!(new_field, field);
    controller.focus(&replacement, &new_field).unwrap();
    assert!(matches!(
        controller.edit(
            &replacement,
            &new_field,
            token,
            &TextEdit::Insert("late".into()),
            &mut fields
        ),
        Err(OwnedInspectorError::TextField(UiTextFieldError::StaleInput))
    ));
    assert_eq!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .revision,
        original_revision
    );
}

#[test]
fn unchanged_controls_reuse_the_exact_frame_until_their_presentation_changes() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut controller, &mut fields, &model);
    let unchanged = paint(&mut controller, &mut fields, &model);
    assert!(Arc::ptr_eq(&frame, &unchanged));
    let field = input(&controller, &frame, "label");
    replace(&mut controller, &mut fields, &frame, &field, "updated");
    let changed = paint(&mut controller, &mut fields, &model);
    assert!(!Arc::ptr_eq(&frame, &changed));
    let unchanged = paint(&mut controller, &mut fields, &model);
    assert!(Arc::ptr_eq(&changed, &unchanged));
}

#[test]
fn pointer_uses_presented_text_origin_and_cannot_apply_a_visible_preedit() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut controller, &mut fields, &model);
    let field = input(&controller, &frame, "label");
    let key = field_key(&controller, &frame, &field);
    let long_text = "Wide text ".repeat(40);
    replace(&mut controller, &mut fields, &frame, &field, &long_text);
    controller.blur(&frame, &field).unwrap();
    let frame = paint(&mut controller, &mut fields, &model);
    let token = controller.focus(&frame, &field).unwrap();
    controller
        .point(
            &frame,
            &field,
            token,
            UiPoint::new(0.0, 18.0),
            false,
            &mut fields,
        )
        .unwrap();
    assert_eq!(
        controller.controls[&key]
            .text
            .as_ref()
            .unwrap()
            .editor()
            .selection_bytes(),
        0..0
    );
    controller
        .edit(
            &frame,
            &field,
            token,
            &TextEdit::Insert("changed".into()),
            &mut fields,
        )
        .unwrap();
    assert!(matches!(
        controller.point(
            &frame,
            &field,
            token,
            UiPoint::new(0.0, 18.0),
            false,
            &mut fields
        ),
        Err(OwnedInspectorError::ChangedProjection)
    ));
    let frame = paint(&mut controller, &mut fields, &model);
    controller
        .edit(
            &frame,
            &field,
            token,
            &TextEdit::Preedit {
                text: "preedit".into(),
                cursor: Some((0, 7)),
            },
            &mut fields,
        )
        .unwrap();
    let composing = paint(&mut controller, &mut fields, &model);
    assert!(matches!(
        controller.operate(
            &composing,
            &field,
            InspectorOperation::Apply,
            &model,
            &mut fields
        ),
        Err(OwnedInspectorError::OperationDisabled(
            InspectorOperation::Apply
        ))
    ));
    assert!(matches!(
        controller.point(
            &composing,
            &field,
            token,
            UiPoint::new(0.0, 18.0),
            false,
            &mut fields
        ),
        Err(OwnedInspectorError::Composing)
    ));
    assert_eq!(
        fields.drafts[&key].candidate.copy_text(),
        format!("changed{long_text}")
    );
}

#[test]
fn invalid_number_and_u64_actions_keep_exact_drafts_and_submission_completion() {
    let (_project, mut application) = application();
    let model = application.current_frame().unwrap();
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut controller, &mut fields, &model);
    let field = input(&controller, &frame, "value");
    let key = field_key(&controller, &frame, &field);
    let basis = fields.drafts[&key].basis.clone();
    replace(&mut controller, &mut fields, &frame, &field, "-");
    let frame = paint(&mut controller, &mut fields, &model);
    assert!(matches!(
        controller.operate(
            &frame,
            &field,
            InspectorOperation::Apply,
            &model,
            &mut fields
        ),
        Err(OwnedInspectorError::InvalidCandidate(_))
    ));
    assert_eq!(fields.drafts[&key].candidate.copy_text(), "-");
    assert!(fields.drafts[&key].error.is_some());
    replace(
        &mut controller,
        &mut fields,
        &frame,
        &field,
        &u64::MAX.to_string(),
    );
    assert!(matches!(
        controller.operate(
            &frame,
            &field,
            InspectorOperation::Apply,
            &model,
            &mut fields
        ),
        Err(OwnedInspectorError::ChangedDraft)
    ));
    let frame = paint(&mut controller, &mut fields, &model);
    let button = frame.semantics().iter().find(|node| {
        matches!(controller.binding(&frame, node.key()), Ok(InspectorBinding::Operation { field: target, operation: InspectorOperation::Apply }) if target == field)
    }).unwrap().key().clone();
    let prepared = controller
        .activate(&frame, &button, &model, &mut fields)
        .unwrap()
        .unwrap();
    assert_eq!(prepared.key, key);
    assert_eq!(prepared.action.field_target(), Some(basis.target.clone()));
    assert!(
        matches!(&prepared.action, EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
        command: SceneInspectorCommand::SetField { value: ComponentValue::U64(value), .. }, ..
    }) if *value == u64::MAX)
    );
    let token = controller.input_token(&frame, &field).unwrap().unwrap();
    assert!(fields.drafts[&key].queued);
    assert!(matches!(
        controller.edit(
            &frame,
            &field,
            token,
            &TextEdit::Insert("0".into()),
            &mut fields
        ),
        Err(OwnedInspectorError::Pending)
    ));
    assert!(
        controller
            .operate(
                &frame,
                &field,
                InspectorOperation::Discard,
                &model,
                &mut fields
            )
            .is_err()
    );
    assert!(
        controller
            .operate(
                &frame,
                &field,
                InspectorOperation::Apply,
                &model,
                &mut fields
            )
            .is_err()
    );
    assert_eq!(
        fields.drafts[&key].candidate.copy_text(),
        u64::MAX.to_string()
    );
    let mut next = 0;
    let mut entries = Vec::new();
    let mut issue = || {
        submission::enqueue(
            &mut next,
            &mut entries,
            PanelOrigin {
                instance: crate::workspace::EditorPanelInstanceId(1),
                incarnation: Default::default(),
            },
            0,
            Recipient::Authoring,
        )
        .unwrap()
    };
    let own = issue();
    let other = issue();
    fields.admitted(&key, Ok(own));
    assert!(!fields.drafts[&key].queued);
    fields.action_finished(
        other,
        &PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded),
    );
    assert_eq!(fields.drafts[&key].pending, Some(own));
    let token = controller.input_token(&frame, &field).unwrap().unwrap();
    assert!(matches!(
        controller.edit(
            &frame,
            &field,
            token,
            &TextEdit::Insert("0".into()),
            &mut fields
        ),
        Err(OwnedInspectorError::Pending)
    ));
    let submission = application.submit_and_drive(
        EditorActionBatch::new(model.revision(), [prepared.action]),
        Duration::ZERO,
    );
    let EditorApplicationActionDispatch::Dispatched(outcome) = &submission.actions()[0] else {
        panic!("the actual Application must dispatch the exact prepared edit")
    };
    fields.action_finished(own, &PanelActionDispatch::Dispatched(outcome.clone()));
    assert!(!fields.drafts.contains_key(&key));
    let current = application.current_frame().unwrap();
    let scene = current.workspace().active_scene.as_ref().unwrap();
    let value = scene
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
        .find(|field| field.id.as_str() == "value")
        .unwrap();
    assert_eq!(value.value, Some(ComponentValue::U64(u64::MAX)));
}

#[test]
fn reset_reapply_and_incompatible_recovery_preserve_the_existing_draft_state() {
    let (_project, mut application) = application();
    let model = application.current_frame().unwrap();
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut controller, &mut fields, &model);
    let field = input(&controller, &frame, "reset");
    let key = field_key(&controller, &frame, &field);
    let original = fields.drafts[&key].basis.clone();
    let prepared = controller
        .operate(
            &frame,
            &field,
            InspectorOperation::Reset,
            &model,
            &mut fields,
        )
        .unwrap()
        .unwrap();
    assert!(matches!(
        prepared.action,
        EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
            command: SceneInspectorCommand::RemoveField { .. },
            ..
        })
    ));
    fields.admitted(&key, Err(PanelSubmitError::Capacity));
    assert!(fields.drafts[&key].dirty && fields.drafts[&key].remove_intent);
    let scene = model.workspace().active_scene.as_ref().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            model.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyInspectorCommand {
                    document: scene.document,
                    base_revision: scene.revision,
                    command: SceneInspectorCommand::SetField {
                        entity: key.entity.clone(),
                        component: key.component.clone(),
                        component_version: ComponentSchemaVersion::ONE,
                        field: ComponentFieldId::new("value"),
                        value: ComponentValue::U64(11),
                    },
                },
            )],
        ),
        Duration::ZERO,
    );
    let current = application.current_frame().unwrap();
    let frame = paint(&mut controller, &mut fields, &current);
    assert_eq!(fields.drafts[&key].basis, original);
    assert!(fields.drafts[&key].conflict && fields.drafts[&key].remove_intent);
    assert!(matches!(
        controller.operate(
            &frame,
            &field,
            InspectorOperation::Apply,
            &current,
            &mut fields
        ),
        Err(OwnedInspectorError::OperationDisabled(
            InspectorOperation::Apply
        ))
    ));
    let reapplied = controller
        .operate(
            &frame,
            &field,
            InspectorOperation::Reapply,
            &current,
            &mut fields,
        )
        .unwrap()
        .unwrap();
    assert!(
        matches!(reapplied.action, EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
        base_revision, command: SceneInspectorCommand::RemoveField { .. }, ..
    }) if base_revision == current.workspace().active_scene.as_ref().unwrap().revision)
    );
    assert!(!fields.drafts[&key].conflict);
    assert!(fields.drafts[&key].remove_intent);
    fields.admitted(&key, Err(PanelSubmitError::Capacity));
    fields.drafts.get_mut(&key).unwrap().incompatible = true;
    let frame = paint(&mut controller, &mut fields, &current);
    let field = input(&controller, &frame, "reset");
    assert!(controller.focus(&frame, &field).is_err());
    assert!(
        controller
            .operate(
                &frame,
                &field,
                InspectorOperation::Reapply,
                &current,
                &mut fields
            )
            .is_err()
    );
    controller
        .operate(
            &frame,
            &field,
            InspectorOperation::Discard,
            &current,
            &mut fields,
        )
        .unwrap();
    assert!(!fields.drafts.contains_key(&key));
}

#[test]
fn late_input_cannot_exceed_the_shared_64_draft_budget_or_use_a_foreign_arc() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let mut controller = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut controller, &mut fields, &model);
    let field = input(&controller, &frame, "value");
    let key = field_key(&controller, &frame, &field);
    let basis = fields.drafts[&key].basis.clone();
    for index in 0..64 {
        let mut retained = basis.clone();
        let id = ComponentFieldId::new(format!("retained{index}"));
        retained.field.id = id.clone();
        retained.target = EditorFieldTarget::new(
            key.document,
            retained.target.base_revision(),
            key.entity.clone(),
            key.component.clone(),
            retained.target.component_version(),
            id,
        );
        let retained_key = FieldKey::from_target(&retained.target);
        let mut draft = FieldDraft::new(retained);
        draft.dirty = true;
        fields.drafts.insert(retained_key, draft);
    }
    let token = controller.focus(&frame, &field).unwrap();
    assert!(matches!(
        controller.edit(
            &frame,
            &field,
            token,
            &TextEdit::Insert("0".into()),
            &mut fields
        ),
        Err(OwnedInspectorError::DraftCapacity)
    ));
    assert!(!fields.drafts[&key].dirty);
    assert_eq!(fields.drafts[&key].candidate.copy_text(), "10");
    let mut other = new_controller();
    let foreign = paint(&mut other, &mut FieldEditors::default(), &model);
    assert!(matches!(
        controller.focus(&foreign, &field),
        Err(OwnedInspectorError::RetiredFrame)
    ));
}
