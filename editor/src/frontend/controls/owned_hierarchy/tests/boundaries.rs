use super::*;

fn replace_schema(model: &EditorFrameModel) -> EditorFrameModel {
    let mut workspace = model.workspace().clone();
    let document = workspace.active_document.unwrap();
    let tab = workspace
        .documents
        .iter_mut()
        .find(|tab| tab.document == document)
        .unwrap();
    let schema = tab
        .authoring_schema_basis
        .as_ref()
        .unwrap()
        .schema(&ComponentTypeId::new(CUSTOM_COMPONENT))
        .unwrap()
        .clone()
        .with_authoring_initial_value(ComponentValue::map([("value", ComponentValue::I64(99))]));
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
    assert_ne!(tab.authoring_schema_basis.as_ref().unwrap(), &successor);
    tab.authoring_schema_basis = Some(successor);
    republish(model, workspace)
}

#[test]
fn activation_and_queue_drain_refuse_same_revision_schema_replacement_and_read_only() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let (_project, application) = application(kind, 3);
        let model = application.current_frame().unwrap();
        let changed = replace_schema(&model);
        assert_eq!(
            changed.workspace().active_scene,
            model.workspace().active_scene
        );
        let mut owner = controller();
        let frame = paint(&mut owner, &model);
        let root = button(&frame, "+ Root");
        assert!(matches!(
            owner.activate(&frame, &root, &changed, false),
            Err(OwnedHierarchyError::ChangedProjection)
        ));
        assert!(matches!(
            owner.activate(&frame, &root, &model, true),
            Err(OwnedHierarchyError::ChangedProjection)
        ));
        let prepared = owner
            .activate(&frame, &root, &model, false)
            .unwrap()
            .unwrap();
        assert!(matches!(
            owner.activate(&frame, &root, &model, false),
            Err(OwnedHierarchyError::ChangedProjection)
        ));
        assert!(matches!(
            prepared.into_action(&changed, false),
            Err(OwnedHierarchyError::ChangedProjection)
        ));
        owner.submission_failed("Original Schema admission was replaced".into());
        let frame = paint(&mut owner, &model);
        let prepared = activate(&mut owner, &frame, "+ Child", &model).unwrap();
        assert!(matches!(
            prepared.into_action(&model, true),
            Err(OwnedHierarchyError::ChangedProjection)
        ));
        owner.submission_failed("The Host became read-only before queue admission".into());
        let disabled = owner
            .compile(&model, environment(800.0), UiTheme::default(), true)
            .unwrap();
        for name in [
            "+ Root",
            "+ Child",
            "Remove Entity",
            "Parent",
            "Order",
            "Select entity-0001",
        ] {
            let key = button(&disabled, name);
            assert!(
                !disabled
                    .semantics()
                    .iter()
                    .find(|node| node.key() == &key)
                    .unwrap()
                    .enabled()
            );
            assert!(matches!(
                owner.activate(&disabled, &key, &model, true),
                Err(OwnedHierarchyError::Disabled)
            ));
        }
        assert_eq!(
            application.current_frame().unwrap().workspace(),
            model.workspace()
        );
    }
}

#[test]
fn source_selection_capabilities_and_local_observation_are_exact_at_both_boundaries() {
    let (_project, mut application) = application(EditorDocumentKind::Scene, 3);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let prepared = activate(&mut owner, &frame, "+ Child", &model).unwrap();
    select(&mut application, entity(1));
    let selected = application.current_frame().unwrap();
    assert_eq!(
        selected.workspace().active_scene.as_ref().unwrap().revision,
        model.workspace().active_scene.as_ref().unwrap().revision
    );
    assert!(matches!(
        prepared.into_action(&selected, false),
        Err(OwnedHierarchyError::ChangedProjection)
    ));
    assert!(
        owner
            .activate(&frame, &button(&frame, "+ Child"), &selected, false)
            .is_err()
    );
    owner.submission_failed("Selection changed before queue admission".into());
    let frame = paint(&mut owner, &selected);
    let prepared = activate(&mut owner, &frame, "+ Root", &selected).unwrap();
    let mut workspace = selected.workspace().clone();
    workspace
        .active_scene
        .as_mut()
        .unwrap()
        .editor
        .inspector
        .composition
        .generic_authoring = false;
    let capability_change = republish(&selected, workspace);
    assert!(prepared.into_action(&capability_change, false).is_err());
    owner.submission_failed("Creation capabilities changed".into());
    let frame = paint(&mut owner, &selected);
    let prepared = activate(&mut owner, &frame, "+ Root", &selected).unwrap();
    // A local mode change spends the observation even when the source has not changed.
    owner.cancel_input().unwrap();
    assert!(prepared.into_action(&selected, false).is_err());
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        selected.workspace()
    );
}

#[test]
fn confirmation_never_retargets_after_selection_or_source_replacement() {
    let (_project, mut application) = application(EditorDocumentKind::Scene, 3);
    let original = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &original);
    assert!(activate(&mut owner, &frame, "Remove Entity", &original).is_none());
    let confirmation = paint(&mut owner, &original);
    let original_confirm = button(&confirmation, "Confirm removal");
    select(&mut application, entity(1));
    let selected = application.current_frame().unwrap();
    assert!(
        owner
            .activate(&confirmation, &original_confirm, &selected, false)
            .is_err()
    );
    let changed = paint(&mut owner, &selected);
    assert!(
        labels(&changed)
            .iter()
            .any(|text| text.contains("branch rooted at entity-0000"))
    );
    let confirm = button(&changed, "Confirm removal");
    assert!(
        !changed
            .semantics()
            .iter()
            .find(|node| node.key() == &confirm)
            .unwrap()
            .enabled()
    );
    assert!(
        owner
            .activate(&changed, &confirm, &selected, false)
            .is_err()
    );
    assert!(activate(&mut owner, &changed, "Cancel removal", &selected).is_none());
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        selected.workspace()
    );
    let frame = paint(&mut owner, &selected);
    activate(&mut owner, &frame, "Remove Entity", &selected);
    let confirmation = paint(&mut owner, &selected);
    let prepared = activate(&mut owner, &confirmation, "Confirm removal", &selected).unwrap();
    select(&mut application, entity(2));
    let current = application.current_frame().unwrap();
    assert!(prepared.into_action(&current, false).is_err());
    assert_eq!(
        current
            .workspace()
            .document(current.workspace().active_document.unwrap())
            .unwrap()
            .history
            .undo_depth,
        0
    );
    assert_eq!(
        current
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .entities
            .len(),
        3
    );
}

#[test]
fn deletion_guards_remain_visible_and_cannot_publish_removal() {
    let project = InteractionProject::new();
    let mut degraded = open(&project);
    select(&mut degraded, SceneEntityId::new("preserved").unwrap());
    let model = degraded.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let remove = button(&frame, "Remove Entity");
    assert!(
        !frame
            .semantics()
            .iter()
            .find(|node| node.key() == &remove)
            .unwrap()
            .enabled()
    );
    assert!(
        labels(&frame)
            .iter()
            .any(|text| text.contains("Complete authoring document"))
    );
    assert!(owner.activate(&frame, &remove, &model, false).is_err());
    assert_eq!(
        degraded.current_frame().unwrap().workspace(),
        model.workspace()
    );

    let (_clean_project, clean) = application(EditorDocumentKind::Scene, 3);
    let model = clean.current_frame().unwrap();
    let mut workspace = model.workspace().clone();
    workspace
        .active_scene
        .as_mut()
        .unwrap()
        .editor
        .inspector
        .entities[1]
        .has_prefab = true;
    let anchored = republish(&model, workspace);
    let mut owner = controller();
    let frame = paint(&mut owner, &anchored);
    assert!(
        labels(&frame)
            .iter()
            .any(|text| text.contains("without Prefab anchors"))
    );
    assert!(
        owner
            .activate(&frame, &button(&frame, "Remove Entity"), &anchored, false)
            .is_err()
    );
}

#[test]
fn exact_ledger_result_ids_preserve_refusal_not_dispatched_and_queue_capacity_errors() {
    let (_project, application) = application(EditorDocumentKind::Scene, 3);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let prepared = activate(&mut owner, &frame, "+ Root", &model).unwrap();
    assert!(prepared.into_action(&model, false).is_ok());
    owner.admitted(Err(PanelSubmitError::Capacity));
    assert!(
        owner
            .error
            .as_deref()
            .unwrap()
            .contains("budget is exhausted")
    );
    let frame = paint(&mut owner, &model);
    let prepared = activate(&mut owner, &frame, "+ Child", &model).unwrap();
    assert!(prepared.into_action(&model, false).is_ok());
    let own = id(11);
    let other = id(12);
    owner.admitted(Ok(own));
    owner.set_appearance(
        UiAppearance::default()
            .with_density(nara_ui_widgets::UiDensity::Compact)
            .with_text_scale(2.0)
            .unwrap(),
    );
    paint(&mut owner, &model);
    owner.action_finished(
        other,
        &PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing),
    );
    assert_eq!(owner.pending, Some(own));
    owner.action_finished(
        own,
        &PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded),
    );
    assert_eq!(owner.pending, None);
    assert!(owner.error.as_deref().unwrap().contains("Superseded"));
    let retained_error = owner.error.clone();
    let applied = PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(
        nara::tooling::EditorWorkspaceCommandReport {
            applied: true,
            ..Default::default()
        },
    ));
    owner.action_finished(other, &applied);
    assert_eq!(owner.error, retained_error);
    let frame = paint(&mut owner, &model);
    assert!(
        labels(&frame)
            .iter()
            .any(|text| text.contains("Superseded"))
    );
    assert!(activate(&mut owner, &frame, "Parent", &model).is_none());
    assert_eq!(owner.pending, None);
    assert_eq!(
        owner.error, retained_error,
        "a local menu cannot settle a document result"
    );
    let choices = paint(&mut owner, &model);
    assert!(activate(&mut owner, &choices, "Close choices", &model).is_none());
    let frame = paint(&mut owner, &model);
    let prepared = activate(&mut owner, &frame, "+ Root", &model).unwrap();
    assert!(prepared.into_action(&model, false).is_ok());
    let own = id(13);
    owner.admitted(Ok(own));
    owner.action_finished(
        own,
        &PanelActionDispatch::Dispatched(EditorActionOutcome::Workspace(Default::default())),
    );
    assert!(owner.error.as_deref().unwrap().contains("refused"));
    let refused = owner.error.clone();
    owner.action_finished(other, &applied);
    assert_eq!(owner.error, refused);
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
}

fn assert_reachable(frame: &Arc<UiFrame>, key: &UiKey) {
    let hit = frame
        .hit_regions()
        .iter()
        .find(|hit| hit.key() == key)
        .expect("the real control has a hit region");
    let rect = hit.rect();
    let center = UiPoint::new(
        rect.origin().x() + rect.size().width() * 0.5,
        rect.origin().y() + rect.size().height() * 0.5,
    );
    assert!(center.y() < frame.environment().logical_size().height());
    assert_eq!(frame.hit_test(center).map(|region| region.key()), Some(key));
}

#[test]
fn compact_confirmation_and_choices_are_reachable_without_resizing_and_retire_hidden_input() {
    let (_project, application) = application(EditorDocumentKind::Scene, 4);
    let model = application.current_frame().unwrap();
    for height in [260.0, 160.0, 80.0, 32.0] {
        let mut owner = controller();
        let frame = owner
            .compile(&model, environment(height), UiTheme::default(), false)
            .unwrap();
        let remove = button(&frame, "Remove Entity");
        assert_reachable(&frame, &remove);
        assert!(
            owner
                .activate(&frame, &remove, &model, false)
                .unwrap()
                .is_none()
        );
        let confirmation = owner
            .compile(&model, environment(height), UiTheme::default(), false)
            .unwrap();
        for name in ["Confirm removal", "Cancel removal"] {
            assert_reachable(&confirmation, &button(&confirmation, name));
        }
        assert!(activate(&mut owner, &confirmation, "Cancel removal", &model).is_none());
        let frame = owner
            .compile(&model, environment(height), UiTheme::default(), false)
            .unwrap();
        for menu in ["Parent", "Order"] {
            let key = button(&frame, menu);
            assert_reachable(&frame, &key);
        }
        assert!(activate(&mut owner, &frame, "Parent", &model).is_none());
        let choices = owner
            .compile(&model, environment(height), UiTheme::default(), false)
            .unwrap();
        let close = button(&choices, "Close choices");
        assert_reachable(&choices, &close);
        assert!(
            owner
                .activate(&choices, &close, &model, false)
                .unwrap()
                .is_none()
        );
        let frame = owner
            .compile(&model, environment(height), UiTheme::default(), false)
            .unwrap();
        activate(&mut owner, &frame, "Remove Entity", &model);
        let confirmation = owner
            .compile(&model, environment(height), UiTheme::default(), false)
            .unwrap();
        let prepared = activate(&mut owner, &confirmation, "Confirm removal", &model).unwrap();
        assert!(prepared.into_action(&model, false).is_ok());
    }
    let mut owner = controller();
    let frame = owner
        .compile(&model, environment(260.0), UiTheme::default(), false)
        .unwrap();
    let field = input(&frame);
    let mut routing = RoutedInput::new(&frame);
    routing.focus(&mut owner, &frame, &field);
    let token = owner.input_token(&frame, &field).unwrap().unwrap();
    owner
        .edit(&frame, &field, token, &TextEdit::Insert("entity".into()))
        .unwrap();
    owner
        .edit(
            &frame,
            &field,
            token,
            &TextEdit::Preedit {
                text: "draft".into(),
                cursor: None,
            },
        )
        .unwrap();
    let frame = owner
        .compile(&model, environment(260.0), UiTheme::default(), false)
        .unwrap();
    activate(&mut owner, &frame, "Parent", &model);
    let choices = owner
        .compile(&model, environment(260.0), UiTheme::default(), false)
        .unwrap();
    assert!(
        !choices
            .semantics()
            .iter()
            .any(|node| node.role() == UiSemanticRole::TextInput)
    );
    assert_eq!(owner.filter.editor().text(), "entity");
    assert!(!owner.filter.editor().is_composing());
    assert!(
        owner
            .edit(&frame, &field, token, &TextEdit::Commit("late".into()))
            .is_err()
    );
    activate(&mut owner, &choices, "Close choices", &model);
    let restored = owner
        .compile(&model, environment(260.0), UiTheme::default(), false)
        .unwrap();
    assert_eq!(input(&restored), field);
    assert_eq!(owner.input_token(&restored, &field).unwrap(), None);
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
}

#[test]
fn scroll_refuses_old_frames_after_filter_mode_source_and_geometry_changes() {
    let (_project, mut application) = application(EditorDocumentKind::Scene, 100);
    let model = application.current_frame().unwrap();
    for change in ["filter", "mode", "geometry"] {
        let mut owner = controller();
        let frame = paint(&mut owner, &model);
        assert!(owner.scroll(&frame, 48.0).unwrap());
        let presented = paint(&mut owner, &model);
        match change {
            "filter" => {
                let field = input(&presented);
                let mut routing = RoutedInput::new(&presented);
                routing.focus(&mut owner, &presented, &field);
                let token = owner.input_token(&presented, &field).unwrap().unwrap();
                owner
                    .edit(
                        &presented,
                        &field,
                        token,
                        &TextEdit::Insert("entity".into()),
                    )
                    .unwrap();
            }
            "mode" => {
                activate(&mut owner, &presented, "Parent", &model);
            }
            "geometry" => {
                owner
                    .compile(&model, environment(600.0), UiTheme::default(), false)
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let offsets = (owner.source_scroll, owner.choice_scroll);
        let text = owner.filter.editor().text().to_owned();
        let selection = owner.filter.editor().selection_bytes();
        let token = owner.filter.input_token();
        for stale in [&presented, &frame] {
            assert!(
                matches!(
                    owner.scroll(stale, 48.0),
                    Err(OwnedHierarchyError::ChangedProjection)
                ),
                "{change}"
            );
        }
        assert_eq!((owner.source_scroll, owner.choice_scroll), offsets);
        assert_eq!(owner.filter.editor().text(), text);
        assert_eq!(owner.filter.editor().selection_bytes(), selection);
        assert_eq!(owner.filter.input_token(), token);
        assert_eq!(
            application.current_frame().unwrap().workspace(),
            model.workspace()
        );
    }

    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    assert!(owner.scroll(&frame, 48.0).unwrap());
    let scene = model.workspace().active_scene.as_ref().unwrap();
    send(
        &mut application,
        authoring::compose_action(
            scene,
            SceneCompositionCommand::MoveEntity {
                target: PrefabEntityTarget::source(entity(0)),
                to: ScenePlacement::last(ScenePlacementParent::Root),
            },
        ),
    );
    let changed = application.current_frame().unwrap();
    assert_ne!(
        changed.workspace().active_scene.as_ref().unwrap().revision,
        scene.revision
    );
    paint(&mut owner, &changed);
    let offsets = (owner.source_scroll, owner.choice_scroll);
    assert!(matches!(
        owner.scroll(&frame, 48.0),
        Err(OwnedHierarchyError::ChangedProjection)
    ));
    assert_eq!((owner.source_scroll, owner.choice_scroll), offsets);
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        changed.workspace()
    );
}

#[test]
fn source_root_and_an_entity_named_root_have_distinct_actions_and_semantics() {
    let project = InteractionProject::new();
    let named_root = SceneEntityId::new("Root").unwrap();
    fs::write(
        project.source(EditorDocumentKind::Scene),
        SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(entity(0)),
            SceneEntityRecord::new(named_root.clone()),
        ])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let mut application = open(&project);
    select(&mut application, entity(0));
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    activate(&mut owner, &frame, "Parent", &model);
    let choices = paint(&mut owner, &model);
    let root = button(&choices, "Parent: Root");
    let entity_root = button(&choices, "Parent entity: Root");
    assert_ne!(root, entity_root);
    let action = owner
        .activate(&choices, &entity_root, &model, false)
        .unwrap()
        .unwrap()
        .into_action(&model, false)
        .unwrap();
    assert!(
        matches!(action, EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
        command: SceneCompositionCommand::MoveEntity { target, to }, ..
    }) if target == PrefabEntityTarget::source(entity(0)) && to == ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(named_root))))
    );
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
}
