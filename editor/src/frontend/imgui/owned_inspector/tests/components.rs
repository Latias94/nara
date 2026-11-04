use super::*;

fn component_button(frame: &Arc<UiFrame>, label: &str) -> UiKey {
    frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::Button && node.name() == label)
        .unwrap_or_else(|| panic!("missing component control: {label}"))
        .key()
        .clone()
}

fn send(application: &mut EditorApplication, action: EditorAction) {
    let frame = application.current_frame().unwrap();
    let result = application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [action]),
        Duration::ZERO,
    );
    assert!(
        matches!(&result.actions()[0], EditorApplicationActionDispatch::Dispatched(nara::tooling::EditorActionOutcome::Workspace(outcome)) if outcome.applied)
    );
}

#[test]
fn component_remove_and_add_retain_exact_basis_and_refuse_stale_or_read_only_controls() {
    let (_project, mut application) = application();
    let model = application.current_frame().unwrap();
    let scene = model.workspace().active_scene.as_ref().unwrap();
    let mut inspector = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut inspector, &mut fields, &model);
    let field = input(&inspector, &frame, "label");
    replace(
        &mut inspector,
        &mut fields,
        &frame,
        &field,
        "keep after removal",
    );
    let key = field_key(&inspector, &frame, &field);
    let frame = paint(&mut inspector, &mut fields, &model);
    let remove = component_button(&frame, &format!("Remove {CUSTOM_COMPONENT}"));
    let action = inspector
        .activate_component(&frame, &remove, &model)
        .unwrap()
        .unwrap();
    assert!(matches!(
        inspector.activate_component(&frame, &remove, &model),
        Err(OwnedInspectorError::ChangedProjection)
    ));
    let action = action.into_action(&model, false).unwrap();
    assert!(
        matches!(&action, EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
        document, base_revision, command: SceneCompositionCommand::RemoveComponent { entity, component }
    }) if *document == scene.document && *base_revision == scene.revision
        && Some(entity) == scene.selection.top_entity() && component.as_str() == CUSTOM_COMPONENT)
    );
    assert_eq!(
        application.current_frame().unwrap().revision(),
        model.revision()
    );
    send(&mut application, action);
    let current = application.current_frame().unwrap();
    assert!(matches!(
        inspector.activate_component(&frame, &remove, &current),
        Err(OwnedInspectorError::ChangedProjection)
    ));
    let removed = paint(&mut inspector, &mut fields, &current);
    assert!(fields.drafts[&key].incompatible);
    assert_eq!(
        fields.drafts[&key].candidate,
        Candidate::Text("keep after removal".into())
    );
    let menu = component_button(&removed, "Add Component");
    assert!(
        inspector
            .activate_component(&removed, &menu, &current)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        inspector.activate_component(&removed, &menu, &current),
        Err(OwnedInspectorError::ChangedProjection)
    ));
    let options = paint(&mut inspector, &mut fields, &current);
    let add = component_button(&options, &format!("Add {CUSTOM_COMPONENT}"));
    let action = inspector
        .activate_component(&options, &add, &current)
        .unwrap()
        .unwrap()
        .into_action(&current, false)
        .unwrap();
    send(&mut application, action);
    let current = application.current_frame().unwrap();
    assert!(
        inspector
            .activate_component(&options, &add, &current)
            .is_err()
    );
    let added = paint(&mut inspector, &mut fields, &current);
    let menu = component_button(&added, "Add Component");
    inspector
        .activate_component(&added, &menu, &current)
        .unwrap();
    let added = paint(&mut inspector, &mut fields, &current);
    let duplicate = component_button(&added, &format!("Add {CUSTOM_COMPONENT}"));
    assert!(matches!(
        inspector.activate_component(&added, &duplicate, &current),
        Err(OwnedInspectorError::CompositionDisabled)
    ));
    let new_remove = component_button(&added, &format!("Remove {CUSTOM_COMPONENT}"));
    assert_ne!(new_remove, remove);
    fields.begin_paint(&current);
    let read_only = inspector
        .compile(
            &current,
            &mut fields,
            added.environment(),
            UiTheme::default(),
            true,
            None,
        )
        .unwrap();
    assert!(matches!(
        inspector.activate_component(&read_only, &new_remove, &current),
        Err(OwnedInspectorError::CompositionDisabled)
    ));
    assert!(matches!(
        inspector.activate_component(&added, &new_remove, &current),
        Err(OwnedInspectorError::ChangedProjection)
    ));
    assert_eq!(
        application.current_frame().unwrap().revision(),
        current.revision()
    );
}

#[test]
fn folding_preserves_local_undo_and_drafts_but_retires_hidden_composition() {
    let (_project, application) = application();
    let model = application.current_frame().unwrap();
    let mut inspector = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut inspector, &mut fields, &model);
    let field = input(&inspector, &frame, "label");
    replace(&mut inspector, &mut fields, &frame, &field, "local draft");
    let key = field_key(&inspector, &frame, &field);
    let frame = paint(&mut inspector, &mut fields, &model);
    let token = inspector.input_token(&frame, &field).unwrap().unwrap();
    inspector
        .edit(
            &frame,
            &field,
            token,
            &TextEdit::Preedit {
                text: "uncommitted".into(),
                cursor: None,
            },
            &mut fields,
        )
        .unwrap();
    let frame = paint(&mut inspector, &mut fields, &model);
    let fold = component_button(&frame, &format!("Collapse {CUSTOM_COMPONENT}"));
    assert!(
        inspector
            .activate_component(&frame, &fold, &model)
            .unwrap()
            .is_none()
    );
    let folded = paint(&mut inspector, &mut fields, &model);
    assert!(
        !folded
            .semantics()
            .iter()
            .any(|node| node.role() == UiSemanticRole::TextInput)
    );
    assert_eq!(
        fields.drafts[&key].candidate,
        Candidate::Text("local draft".into())
    );
    assert!(
        inspector
            .edit(
                &frame,
                &field,
                token,
                &TextEdit::Commit("late".into()),
                &mut fields
            )
            .is_err()
    );
    let expand = component_button(&folded, &format!("Expand {CUSTOM_COMPONENT}"));
    inspector
        .activate_component(&folded, &expand, &model)
        .unwrap();
    let expanded = paint(&mut inspector, &mut fields, &model);
    let restored = input(&inspector, &expanded, "label");
    assert_eq!(restored, field);
    let new_token = inspector.focus(&expanded, &restored).unwrap();
    assert_ne!(new_token, token);
    inspector
        .edit(
            &expanded,
            &restored,
            new_token,
            &TextEdit::Undo,
            &mut fields,
        )
        .unwrap();
    assert_eq!(
        fields.drafts[&key].candidate,
        Candidate::Text("before".into())
    );
    assert_eq!(
        application.current_frame().unwrap().revision(),
        model.revision()
    );
}

#[test]
fn unavailable_component_controls_cannot_remove_unknown_source_records() {
    let (_project, mut application) = application();
    let model = application.current_frame().unwrap();
    let scene = model.workspace().active_scene.as_ref().unwrap();
    let manifest = application
        .active_authoring_unavailable_record_manifest()
        .unwrap();
    send(
        &mut application,
        EditorAction::Navigate(nara::tooling::EditorNavigationAction::SelectEntity {
            document: scene.document,
            entity: Some(nara::scene::SceneEntityId::new("preserved").unwrap()),
        }),
    );
    let model = application.current_frame().unwrap();
    let mut inspector = new_controller();
    let frame = paint(&mut inspector, &mut FieldEditors::default(), &model);
    let remove = component_button(&frame, "Remove studio.unavailable.Payload");
    assert!(matches!(
        inspector.activate_component(&frame, &remove, &model),
        Err(OwnedInspectorError::CompositionDisabled)
    ));
    assert_eq!(
        application.active_authoring_unavailable_record_manifest(),
        Some(manifest)
    );
    assert_eq!(
        application.current_frame().unwrap().revision(),
        model.revision()
    );
}

#[test]
fn queued_component_refuses_same_revision_schema_replacement_at_submission() {
    use nara::scene::{__private::AuthoringAdapterReceipt, AuthoringSchemaBasis};
    use nara::tooling::EditorFrameClock;

    let (_project, mut application) = application();
    let original = application.current_frame().unwrap();
    let scene = original.workspace().active_scene.as_ref().unwrap();
    send(
        &mut application,
        authoring::compose_action(
            scene,
            SceneCompositionCommand::RemoveComponent {
                entity: scene.selection.top_entity().unwrap().clone(),
                component: ComponentTypeId::new(CUSTOM_COMPONENT),
            },
        ),
    );
    let model = application.current_frame().unwrap();
    let mut inspector = new_controller();
    let mut fields = FieldEditors::default();
    let frame = paint(&mut inspector, &mut fields, &model);
    let menu = component_button(&frame, "Add Component");
    inspector.activate_component(&frame, &menu, &model).unwrap();
    let frame = paint(&mut inspector, &mut fields, &model);
    let add = component_button(&frame, &format!("Add {CUSTOM_COMPONENT}"));
    let pending = inspector
        .activate_component(&frame, &add, &model)
        .unwrap()
        .unwrap();
    let mut workspace = model.workspace().clone();
    let document = workspace.active_document.unwrap();
    let tab = workspace
        .documents
        .iter_mut()
        .find(|tab| tab.document == document)
        .unwrap();
    let previous = tab.authoring_schema_basis.as_ref().unwrap();
    let schema = previous
        .schema(&ComponentTypeId::new(CUSTOM_COMPONENT))
        .unwrap()
        .clone()
        .with_authoring_initial_value(ComponentValue::map([
            ("value", ComponentValue::U64(99)),
            ("label", ComponentValue::String("changed template".into())),
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
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let successor = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    assert_ne!(previous, &successor);
    tab.authoring_schema_basis = Some(successor);
    let changed = EditorFrameClock::new().publish(
        workspace,
        model.project().clone(),
        Default::default(),
        nara::tooling::EditorReopenTarget::Unavailable,
        None,
    );
    assert_eq!(
        changed.workspace().active_scene,
        model.workspace().active_scene
    );
    assert!(matches!(
        pending.into_action(&changed, false),
        Err(OwnedInspectorError::ChangedProjection)
    ));
    assert_eq!(
        application.current_frame().unwrap().revision(),
        model.revision()
    );
}
