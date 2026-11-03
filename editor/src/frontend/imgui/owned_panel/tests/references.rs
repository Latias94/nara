use std::{fs, time::Instant};

use nara::{
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    identity::EntityReference,
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentValueKind, PersistentComponentSchemaOwnerId,
        encode_asset_reference_value,
    },
    scene::{
        SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId,
        SceneEntityRecord,
    },
    tooling::{EditorNavigationAction, EditorProjectBrowserAction},
};

use super::*;

fn select(harness: &mut Harness, entity: &str) {
    let model = harness.application.current_frame().unwrap();
    let scene = model.workspace().active_scene.as_ref().unwrap();
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            model.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SelectEntity {
                    document: scene.document,
                    entity: Some(SceneEntityId::new(entity).unwrap()),
                },
            )],
        ),
        Duration::ZERO,
    );
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .selection
            .top_entity()
            .unwrap()
            .as_str(),
        entity,
    );
}

fn repaint(harness: &mut Harness) {
    let candidate = harness.draw();
    assert!(candidate.ledger.submissions.is_empty());
    assert!(candidate.batch.first_field_target().is_none());
    harness.submit(candidate);
}

fn setup_references(project: InteractionProject) -> (Harness, EditorPanelInstanceId) {
    let mut harness = Harness::with_project(project, 0);
    select(&mut harness, "source");
    let panel = harness
        .application
        .panel_workspace()
        .layout()
        .instances()
        .iter()
        .find(|instance| instance.kind() == "nara.inspector")
        .unwrap()
        .id();
    harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .inspector_tab = InspectorTab::Owned;
    harness.frontend.set_owned_images(fonts(), BTreeMap::new());
    repaint(&mut harness);
    (harness, panel)
}

fn reference(entity: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal {
        entity: SceneEntityId::new(entity).unwrap(),
    })
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

fn target_button(
    frontend: &ImguiEditorFrontend,
    panel: EditorPanelInstanceId,
    label: &str,
) -> (Arc<UiFrame>, UiKey) {
    let inspector = frontend.presentation.widgets[&panel]
        .owned_inspector
        .as_ref()
        .unwrap();
    let frame = Arc::clone(inspector.compiled_frame().unwrap());
    let group = frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::Group && node.name() == "target")
        .unwrap();
    let key = frame
        .semantics()
        .iter()
        .find(|node| {
            node.role() == UiSemanticRole::Button
                && node.name() == label
                && node.parent() == Some(group.key())
        })
        .unwrap_or_else(|| panic!("missing target button: {label}"))
        .key()
        .clone();
    (frame, key)
}

fn live_choices(
    frontend: &ImguiEditorFrontend,
    panel: EditorPanelInstanceId,
) -> Vec<(UiKey, String)> {
    let inspector = frontend.presentation.widgets[&panel]
        .owned_inspector
        .as_ref()
        .unwrap();
    let frame = inspector.compiled_frame().unwrap();
    frame
        .semantics()
        .iter()
        .filter_map(|node| match inspector.binding(frame, node.key()) {
            Ok(InspectorBinding::Reference {
                action: ReferenceAction::Choose(entity),
                ..
            }) => Some((node.key().clone(), entity)),
            _ => None,
        })
        .collect()
}

fn operation(harness: &mut Harness, panel: EditorPanelInstanceId, operation: InspectorOperation) {
    let (frame, field, _) = input(&harness.frontend, panel, "target");
    let model = harness.application.current_frame().unwrap();
    harness
        .frontend
        .owned_operation(panel, &frame, &field, operation, &model)
        .unwrap();
}

fn reference_operation(
    harness: &mut Harness,
    panel: EditorPanelInstanceId,
    action: ReferenceAction,
) {
    let inspector = harness.frontend.presentation.widgets[&panel]
        .owned_inspector
        .as_ref()
        .unwrap();
    let frame = Arc::clone(inspector.compiled_frame().unwrap());
    let button = frame.semantics().iter().find(|node| matches!(
        inspector.binding(&frame, node.key()),
        Ok(InspectorBinding::Reference { input: None, action: current, .. }) if current == action
    )).unwrap().key().clone();
    choose(harness, panel, &frame, &button).unwrap();
}

fn choose(
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
    assert!(
        edit.is_none(),
        "choosing a reference must remain a local edit"
    );
    assert!(widgets.owned_pending.is_empty());
    Ok(())
}

fn apply_exactly(
    harness: &mut Harness,
    panel: EditorPanelInstanceId,
    field: &str,
    operation: InspectorOperation,
    expected: ComponentValue,
) {
    let model = harness.application.current_frame().unwrap();
    let scene = model.workspace().active_scene.as_ref().unwrap();
    let (frame, control, key) = input(&harness.frontend, panel, field);
    harness
        .frontend
        .owned_operation(panel, &frame, &control, operation, &model)
        .unwrap();
    let widgets = &harness.frontend.presentation.widgets[&panel];
    let target = widgets.authoring.fields.drafts[&key].basis.target.clone();
    assert_eq!(target.base_revision(), scene.revision);
    let [queued] = widgets.owned_pending.as_slice() else {
        panic!("one explicit field operation must queue one edit")
    };
    assert_eq!(queued.key, key);
    assert_eq!(queued.action.field_target(), Some(target.clone()));
    assert!(matches!(
        &queued.action,
        EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
            command: SceneInspectorCommand::SetField { value, .. },
            ..
        }) if value == &expected
    ));

    let candidate = harness.draw();
    let [submission] = candidate.ledger.submissions.as_slice() else {
        panic!("the edit must receive one exact submission identity")
    };
    let slot = submission.slot;
    assert_eq!(
        harness.frontend.presentation.widgets[&panel]
            .authoring
            .fields
            .drafts[&key]
            .pending,
        Some(submission.id),
    );
    assert_eq!(candidate.batch.first_field_target(), Some(target));
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        model.workspace().active_scene,
    );
    let dispatch = harness
        .frontend
        .dispatch_submission(&mut harness.application, candidate)
        .unwrap();
    assert!(matches!(
        dispatch.original_outcome(slot),
        Some(EditorActionOutcome::Workspace(report)) if report.applied
    ));
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
    assert!(harness.frontend.pending.is_none());
    assert_eq!(selected_value(harness, field), expected);
}

#[test]
fn owned_entity_reference_picker_retires_menu_input_and_recovers_conflicting_drafts() {
    let project = InteractionProject::new_entity_references();
    fs::write(
        project.root.join("worlds/alternate.scene.json"),
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("isolated").unwrap(),
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let (mut harness, panel) = setup_references(project);
    let original = harness.application.current_frame().unwrap();
    let (initial_frame, initial_field, key) = input(&harness.frontend, panel, "target");
    let original_basis = harness.frontend.presentation.widgets[&panel]
        .authoring
        .fields
        .drafts[&key]
        .basis
        .clone();
    let token = harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .owned_inspector
        .as_mut()
        .unwrap()
        .focus(&initial_frame, &initial_field)
        .unwrap();
    let (_, initial_toggle) = target_button(&harness.frontend, panel, "Choose target");
    reference_operation(&mut harness, panel, ReferenceAction::Toggle);
    assert!(choose(&mut harness, panel, &initial_frame, &initial_toggle).is_err());
    repaint(&mut harness);
    let (open_frame, first_choice) = target_button(&harness.frontend, panel, "other [other]");
    let (_, toggle) = target_button(&harness.frontend, panel, "Hide targets");
    let inspector = harness.frontend.presentation.widgets[&panel]
        .owned_inspector
        .as_ref()
        .unwrap();
    assert_eq!(
        inspector.binding(&open_frame, &first_choice).unwrap(),
        InspectorBinding::Reference {
            field: initial_field.clone(),
            input: None,
            action: ReferenceAction::Choose("other".into()),
        },
    );
    assert_eq!(
        inspector.binding(&open_frame, &toggle).unwrap(),
        InspectorBinding::Reference {
            field: initial_field.clone(),
            input: None,
            action: ReferenceAction::Toggle,
        },
    );
    repaint(&mut harness);
    assert_eq!(
        target_button(&harness.frontend, panel, "other [other]").1,
        first_choice,
    );
    reference_operation(&mut harness, panel, ReferenceAction::Toggle);
    assert!(choose(&mut harness, panel, &open_frame, &first_choice).is_err());
    repaint(&mut harness);
    target_button(&harness.frontend, panel, "Choose target");
    assert!(live_choices(&harness.frontend, panel).is_empty());
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        original.workspace().active_scene,
    );

    reference_operation(&mut harness, panel, ReferenceAction::Toggle);
    repaint(&mut harness);
    let (reopened, replacement) = target_button(&harness.frontend, panel, "other [other]");
    assert_ne!(replacement, first_choice);
    assert!(choose(&mut harness, panel, &open_frame, &first_choice).is_err());
    assert_eq!(
        target_button(&harness.frontend, panel, "Hide targets").1,
        toggle
    );
    assert!(matches!(
        choose(&mut harness, panel, &open_frame, &toggle),
        Err(OwnedInspectorError::ChangedProjection)
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness
        .application
        .current_frame()
        .unwrap()
        .project_browser()
        .has_pending_work()
        && Instant::now() < deadline
    {
        harness
            .application
            .drive_editor_frame(Duration::ZERO, false, false);
        std::thread::sleep(Duration::from_millis(1));
    }
    let current = harness.application.current_frame().unwrap();
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            current.revision(),
            [EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::Open {
                    relative_path: "worlds/alternate.scene.json".into(),
                },
            )],
        ),
        Duration::ZERO,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness
        .application
        .current_frame()
        .unwrap()
        .workspace()
        .active_document
        == Some(key.document)
        && Instant::now() < deadline
    {
        harness
            .application
            .drive_editor_frame(Duration::ZERO, false, false);
        std::thread::sleep(Duration::from_millis(1));
    }
    let current = harness.application.current_frame().unwrap();
    assert_ne!(current.workspace().active_document, Some(key.document));
    assert!(matches!(
        choose(&mut harness, panel, &reopened, &replacement),
        Err(OwnedInspectorError::ChangedProjection)
    ));
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            current.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SetActiveDocument {
                    document: key.document,
                },
            )],
        ),
        Duration::ZERO,
    );
    select(&mut harness, "other");
    assert!(matches!(
        choose(&mut harness, panel, &reopened, &replacement),
        Err(OwnedInspectorError::ChangedProjection)
    ));
    repaint(&mut harness);
    assert!(
        harness
            .frontend
            .owned_edit(
                panel,
                &initial_frame,
                &initial_field,
                token,
                &TextEdit::Insert("late".into()),
            )
            .is_err()
    );
    select(&mut harness, "source");
    repaint(&mut harness);
    let (frame, field, _) = input(&harness.frontend, panel, "target");
    assert_ne!(field, initial_field);
    assert!(
        harness
            .frontend
            .owned_edit(
                panel,
                &frame,
                &field,
                token,
                &TextEdit::Insert("late".into())
            )
            .is_err()
    );
    let replacement_token = harness
        .frontend
        .presentation
        .widgets
        .get_mut(&panel)
        .unwrap()
        .owned_inspector
        .as_mut()
        .unwrap()
        .focus(&frame, &field)
        .unwrap();
    reference_operation(&mut harness, panel, ReferenceAction::Toggle);
    repaint(&mut harness);
    let (frame, choice) = target_button(&harness.frontend, panel, "other [other]");
    choose(&mut harness, panel, &frame, &choice).unwrap();
    assert!(choose(&mut harness, panel, &frame, &choice).is_err());
    assert!(
        harness
            .frontend
            .owned_field_token(panel, &frame, &field)
            .is_none()
    );
    assert!(
        harness
            .frontend
            .owned_edit(
                panel,
                &frame,
                &field,
                replacement_token,
                &TextEdit::Insert("late".into()),
            )
            .is_err()
    );
    repaint(&mut harness);
    let draft = &harness.frontend.presentation.widgets[&panel]
        .authoring
        .fields
        .drafts[&key];
    assert_eq!(draft.candidate.copy_text(), "scene-local:other");
    assert_eq!(draft.basis, original_basis);
    assert!(draft.dirty && !draft.in_flight() && draft.error.is_none());
    assert!(live_choices(&harness.frontend, panel).is_empty());
    target_button(&harness.frontend, panel, "Choose target");
    operation(&mut harness, panel, InspectorOperation::Discard);
    repaint(&mut harness);
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        original.workspace().active_scene,
    );
    assert_eq!(
        harness.frontend.presentation.widgets[&panel]
            .authoring
            .fields
            .drafts[&key]
            .candidate
            .copy_text(),
        "scene-local:target",
    );

    reference_operation(&mut harness, panel, ReferenceAction::Toggle);
    repaint(&mut harness);
    let (frame, choice) = target_button(&harness.frontend, panel, "other [other]");
    choose(&mut harness, panel, &frame, &choice).unwrap();
    repaint(&mut harness);
    reference_operation(&mut harness, panel, ReferenceAction::Toggle);
    repaint(&mut harness);
    let (stale_frame, stale_choice) = target_button(&harness.frontend, panel, "target [target]");
    let current = harness.application.current_frame().unwrap();
    harness.application.submit_and_drive(
        EditorActionBatch::new(
            current.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyInspectorCommand {
                    document: key.document,
                    base_revision: original_basis.target.base_revision(),
                    command: SceneInspectorCommand::SetField {
                        entity: key.entity.clone(),
                        component: key.component.clone(),
                        component_version: original_basis.target.component_version(),
                        field: key.field.clone(),
                        value: reference("source"),
                    },
                },
            )],
        ),
        Duration::ZERO,
    );
    assert_eq!(selected_value(&harness, "target"), reference("source"));
    assert!(matches!(
        choose(&mut harness, panel, &stale_frame, &stale_choice),
        Err(OwnedInspectorError::ChangedProjection)
    ));
    repaint(&mut harness);
    let draft = &harness.frontend.presentation.widgets[&panel]
        .authoring
        .fields
        .drafts[&key];
    assert_eq!(draft.candidate.copy_text(), "scene-local:other");
    assert_eq!(draft.basis, original_basis);
    assert!(draft.conflict && !draft.in_flight());
    let (frame, field, _) = input(&harness.frontend, panel, "target");
    let current = harness.application.current_frame().unwrap();
    assert!(
        harness
            .frontend
            .owned_operation(panel, &frame, &field, InspectorOperation::Apply, &current)
            .is_err()
    );
    apply_exactly(
        &mut harness,
        panel,
        "target",
        InspectorOperation::Reapply,
        reference("other"),
    );
    assert_eq!(
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
            .history
            .undo_depth,
        2,
    );
}

#[test]
fn owned_entity_reference_pages_bound_live_choices_and_apply_a_later_target_exactly() {
    let project = InteractionProject::new_entity_references();
    let source = project.root.join("worlds/entry.scene.json");
    let document = SceneDocumentCandidate::decode_json_bytes(&fs::read(&source).unwrap())
        .unwrap()
        .into_composition_source()
        .unwrap();
    let entities = document.entities.into_iter().chain((0..64).map(|index| {
        SceneEntityRecord::new(SceneEntityId::new(format!("target-{index:02}")).unwrap())
    }));
    fs::write(
        &source,
        SceneDocument::from_ordered_roots(entities)
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let (mut harness, panel) = setup_references(project);
    let original = harness.application.current_frame().unwrap();
    assert_eq!(
        original
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .entities
            .len(),
        67,
    );
    reference_operation(&mut harness, panel, ReferenceAction::Toggle);
    repaint(&mut harness);
    let first_page = live_choices(&harness.frontend, panel);
    assert!(!first_page.is_empty() && first_page.len() <= 32);
    assert!(!first_page.iter().any(|(_, entity)| entity == "target-39"));
    let (first_frame, first_choice) = target_button(&harness.frontend, panel, "other [other]");
    let (_, next) = target_button(&harness.frontend, panel, "Next targets");
    let (_, field, _) = input(&harness.frontend, panel, "target");
    assert_eq!(
        harness.frontend.presentation.widgets[&panel]
            .owned_inspector
            .as_ref()
            .unwrap()
            .binding(&first_frame, &next)
            .unwrap(),
        InspectorBinding::Reference {
            field: field.clone(),
            input: None,
            action: ReferenceAction::Next,
        },
    );
    let (_, previous) = target_button(&harness.frontend, panel, "Previous targets");
    assert!(choose(&mut harness, panel, &first_frame, &previous).is_err());
    reference_operation(&mut harness, panel, ReferenceAction::Next);
    assert!(choose(&mut harness, panel, &first_frame, &first_choice).is_err());
    assert!(choose(&mut harness, panel, &first_frame, &next).is_err());
    repaint(&mut harness);
    let second_page = live_choices(&harness.frontend, panel);
    assert!(!second_page.is_empty() && second_page.len() <= 32);
    let (second_frame, later_choice) =
        target_button(&harness.frontend, panel, "target-39 [target-39]");
    assert!(choose(&mut harness, panel, &first_frame, &first_choice).is_err());
    reference_operation(&mut harness, panel, ReferenceAction::Previous);
    assert!(choose(&mut harness, panel, &second_frame, &later_choice).is_err());
    repaint(&mut harness);
    assert_ne!(
        target_button(&harness.frontend, panel, "other [other]").1,
        first_choice,
    );
    reference_operation(&mut harness, panel, ReferenceAction::Next);
    repaint(&mut harness);
    let (frame, replacement) = target_button(&harness.frontend, panel, "target-39 [target-39]");
    assert_ne!(replacement, later_choice);
    assert!(choose(&mut harness, panel, &second_frame, &later_choice).is_err());
    assert!(
        harness
            .frontend
            .scroll_owned_panel(panel, &frame, f32::MAX)
            .unwrap()
    );
    repaint(&mut harness);
    let (scrolled, visible_choice) =
        target_button(&harness.frontend, panel, "target-39 [target-39]");
    assert_eq!(visible_choice, replacement);
    let hidden_input = scrolled
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(&field))
        .unwrap();
    assert!(
        hidden_input
            .clip_rect()
            .is_some_and(|clip| hidden_input.rect().intersect(clip).is_none())
    );
    assert!(!scrolled.semantics().iter().any(|node| node.key() == &field));
    let choice_layout = scrolled
        .layouts()
        .iter()
        .find(|layout| layout.key() == Some(&visible_choice))
        .unwrap();
    assert!(choice_layout.visible());
    assert!(
        scrolled
            .semantics()
            .iter()
            .any(|node| node.key() == &visible_choice && node.enabled())
    );
    let choices = live_choices(&harness.frontend, panel);
    assert!(!choices.is_empty() && choices.len() <= 32);
    choose(&mut harness, panel, &scrolled, &visible_choice).unwrap();
    harness
        .frontend
        .scroll_owned_panel(panel, &scrolled, -f32::MAX)
        .unwrap();
    repaint(&mut harness);
    let (_, _, key) = input(&harness.frontend, panel, "target");
    let draft = &harness.frontend.presentation.widgets[&panel]
        .authoring
        .fields
        .drafts[&key];
    assert_eq!(draft.candidate.copy_text(), "scene-local:target-39");
    assert_eq!(
        draft.basis.target.base_revision(),
        original.workspace().active_scene.as_ref().unwrap().revision,
    );
    assert!(draft.dirty && !draft.in_flight());
    assert!(live_choices(&harness.frontend, panel).is_empty());
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        original.workspace().active_scene,
    );
    apply_exactly(
        &mut harness,
        panel,
        "target",
        InspectorOperation::Apply,
        reference("target-39"),
    );
    repaint(&mut harness);
    harness.click_control("Undo");
    assert_eq!(selected_value(&harness, "target"), reference("target"));
    let undone = harness.application.current_frame().unwrap();
    let history = &undone
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .history;
    assert_eq!((history.undo_depth, history.redo_depth), (0, 1));
    harness.click_control("Redo");
    assert_eq!(selected_value(&harness, "target"), reference("target-39"));
    let redone = harness.application.current_frame().unwrap();
    let history = &redone
        .workspace()
        .active_scene
        .as_ref()
        .unwrap()
        .editor
        .inspector
        .history;
    assert_eq!((history.undo_depth, history.redo_depth), (1, 0));
}

#[test]
fn owned_asset_reference_text_input_applies_a_typed_source_value_exactly() {
    let project = InteractionProject::new();
    for (name, stable) in [
        ("first.png", "0b035fcc-a832-4287-90a0-02d978b97303"),
        ("second.png", "3c7c5be4-fd4e-4b65-b8d4-c671f5982186"),
    ] {
        fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../tests/fixtures/images/valid-rgba-1x1.png"),
            project.root.join("media").join(name),
        )
        .unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(stable).unwrap(),
                AssetPath::new(name).unwrap(),
                AssetSourceKind::Image,
            ),
            nara::image::ImageImportSettings::default(),
        );
        fs::write(
            project.root.join("media").join(format!("{name}.meta")),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }
    let initial = encode_asset_reference_value(&AssetRef::path("first.png").unwrap());
    let owner = PersistentComponentSchemaOwnerId::try_new("studio.interaction.components").unwrap();
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        "Asset references",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("asset"),
        "Asset",
        ComponentFieldPath::from_fields(["asset"]),
        ComponentValueKind::AssetRef,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::AssetRef)])
    .with_authoring_initial_value(ComponentValue::map([("asset", initial.clone())]));
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
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
    let source = SceneEntityRecord::new(SceneEntityId::new("source").unwrap()).with_component(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("asset", initial.clone())]),
        )
        .with_owner(owner),
    );
    fs::write(
        project.root.join("worlds/entry.scene.json"),
        SceneDocument::from_ordered_roots([source])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let (mut harness, panel) = setup_references(project);
    let original = harness.application.current_frame().unwrap();
    assert_eq!(selected_value(&harness, "asset"), initial);
    let (_, key, _) = replace(&mut harness.frontend, panel, "asset", "second.png");
    repaint(&mut harness);
    let draft = &harness.frontend.presentation.widgets[&panel]
        .authoring
        .fields
        .drafts[&key];
    assert!(draft.dirty && !draft.in_flight() && draft.error.is_none());
    assert_eq!(draft.candidate.copy_text(), "second.png");
    assert_eq!(
        harness
            .application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene,
        original.workspace().active_scene,
    );
    apply_exactly(
        &mut harness,
        panel,
        "asset",
        InspectorOperation::Apply,
        encode_asset_reference_value(&AssetRef::path("second.png").unwrap()),
    );
}
