use super::*;

fn focus(
    owner: &mut OwnedHierarchy,
    frame: &Arc<UiFrame>,
    routing: &mut RoutedInput,
) -> (UiKey, UiTextInputToken) {
    let field = input(frame);
    routing.focus(owner, frame, &field);
    let token = owner
        .input_token(frame, &field)
        .unwrap()
        .expect("admitted focus opens one exact input lifetime");
    (field, token)
}

#[test]
fn committed_filter_preedit_local_undo_clipboard_and_finish_never_publish_source() {
    let (_project, application) = application(EditorDocumentKind::Scene, 4);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let mut routing = RoutedInput::new(&frame);
    let (field, token) = focus(&mut owner, &frame, &mut routing);
    owner
        .edit(
            &frame,
            &field,
            token,
            &TextEdit::Insert("ENTITY-0001".into()),
        )
        .unwrap();
    let filtered = paint(&mut owner, &model);
    assert_eq!(input(&filtered), field);
    assert_eq!(owner.input_token(&filtered, &field).unwrap(), Some(token));
    assert!(
        filtered
            .semantics()
            .iter()
            .any(|node| node.name() == "Select entity-0001")
    );
    assert!(
        !filtered
            .semantics()
            .iter()
            .any(|node| node.name() == "Select entity-0000")
    );
    let committed_rows = filtered
        .semantics()
        .iter()
        .filter(|node| node.role() == UiSemanticRole::Button && node.name().starts_with("Select "))
        .map(|node| node.name().to_owned())
        .collect::<Vec<_>>();
    let outcome = owner
        .edit(
            &filtered,
            &field,
            token,
            &TextEdit::Preedit {
                text: "uncommitted".into(),
                cursor: Some((0, 11)),
            },
        )
        .unwrap();
    assert!(!outcome.text_changed);
    let composing = paint(&mut owner, &model);
    assert!(owner.is_composing(&composing, &field, token).unwrap());
    assert_eq!(
        composing
            .semantics()
            .iter()
            .filter(
                |node| node.role() == UiSemanticRole::Button && node.name().starts_with("Select ")
            )
            .map(|node| node.name().to_owned())
            .collect::<Vec<_>>(),
        committed_rows
    );
    assert!(matches!(
        owner.selected_text(&composing, &field, token),
        Err(OwnedHierarchyError::Composing)
    ));
    assert!(matches!(
        owner.point(&composing, &field, token, UiPoint::new(10.0, 12.0), false),
        Err(OwnedHierarchyError::Composing)
    ));
    let area = owner.ime_area(&composing, &field, token).unwrap();
    assert!(area.origin().x() >= 0.0 && area.size().width() > 0.0);
    owner
        .edit(&composing, &field, token, &TextEdit::CancelComposition)
        .unwrap();
    owner
        .edit(&composing, &field, token, &TextEdit::SelectAll)
        .unwrap();
    assert_eq!(
        owner
            .selected_text(&composing, &field, token)
            .unwrap()
            .as_deref(),
        Some("ENTITY-0001")
    );
    owner
        .edit(&composing, &field, token, &TextEdit::Undo)
        .unwrap();
    assert_eq!(owner.filter.editor().text(), "");
    let unfiltered = paint(&mut owner, &model);
    assert!(
        unfiltered
            .semantics()
            .iter()
            .any(|node| node.name() == "Select entity-0000")
    );
    owner
        .edit(&unfiltered, &field, token, &TextEdit::Redo)
        .unwrap();
    let filtered = paint(&mut owner, &model);
    assert_eq!(owner.filter.editor().text(), "ENTITY-0001");
    owner.finish_input(&filtered, &field, false).unwrap();
    assert_eq!(owner.filter.editor().text(), "ENTITY-0001");
    assert_eq!(owner.input_token(&filtered, &field).unwrap(), None);
    assert!(
        owner
            .edit(&filtered, &field, token, &TextEdit::Commit("late".into()))
            .is_err()
    );
    routing.route(
        &mut owner,
        &filtered,
        UiInputEvent::SeatCancelled {
            seat: UiSeatId::PRIMARY,
        },
    );
    let current = paint(&mut owner, &model);
    let (field, next_token) = focus(&mut owner, &current, &mut routing);
    assert_ne!(next_token, token);
    owner
        .edit(
            &current,
            &field,
            next_token,
            &TextEdit::Preedit {
                text: "cancel me".into(),
                cursor: None,
            },
        )
        .unwrap();
    owner.finish_input(&current, &field, true).unwrap();
    assert_eq!(owner.filter.editor().text(), "");
    assert!(!owner.filter.editor().is_composing());
    assert!(
        owner
            .edit(
                &current,
                &field,
                next_token,
                &TextEdit::Insert("late".into())
            )
            .is_err()
    );
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
    assert_eq!(
        application.current_frame().unwrap().revision(),
        model.revision()
    );
}

#[test]
fn filter_matches_source_names_and_clear_show_selection_are_local() {
    let (_project, application) = application(EditorDocumentKind::Scene, 4);
    let original = application.current_frame().unwrap();
    let mut workspace = original.workspace().clone();
    workspace
        .active_scene
        .as_mut()
        .unwrap()
        .editor
        .inspector
        .entities[2]
        .name = Some("Orbital Station".into());
    let model = republish(&original, workspace);
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let mut routing = RoutedInput::new(&frame);
    let (field, token) = focus(&mut owner, &frame, &mut routing);
    owner
        .edit(&frame, &field, token, &TextEdit::Insert("ORBITAL".into()))
        .unwrap();
    let filtered = paint(&mut owner, &model);
    assert!(
        filtered
            .semantics()
            .iter()
            .any(|node| node.name() == "Select entity-0002")
    );
    assert!(
        !filtered
            .semantics()
            .iter()
            .any(|node| node.name() == "Select entity-0000")
    );
    assert!(activate(&mut owner, &filtered, "Show selection", &model).is_none());
    let selected = paint(&mut owner, &model);
    assert_eq!(owner.filter.editor().text(), "");
    assert!(
        selected
            .semantics()
            .iter()
            .any(|node| node.name() == "Select entity-0000")
    );
    owner
        .edit(
            &selected,
            &field,
            token,
            &TextEdit::Insert("does not match".into()),
        )
        .unwrap();
    let hidden = paint(&mut owner, &model);
    assert!(
        !hidden
            .semantics()
            .iter()
            .any(|node| node.name().starts_with("Select "))
    );
    assert!(activate(&mut owner, &hidden, "Clear filter", &model).is_none());
    assert_eq!(owner.filter.editor().text(), "");
    assert_eq!(
        model
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .selected_entity
            .as_ref(),
        Some(&entity(0))
    );
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        original.workspace()
    );
}

#[test]
fn compatible_source_revisions_keep_filter_lifetime_but_reopen_retires_ime_and_late_tokens() {
    let (_project, mut application) = application(EditorDocumentKind::Scene, 4);
    let original = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &original);
    let mut routing = RoutedInput::new(&frame);
    let (field, token) = focus(&mut owner, &frame, &mut routing);
    owner
        .edit(&frame, &field, token, &TextEdit::Insert("entity".into()))
        .unwrap();
    owner
        .edit(
            &frame,
            &field,
            token,
            &TextEdit::Preedit {
                text: "pending".into(),
                cursor: Some((0, 7)),
            },
        )
        .unwrap();
    let scene = original.workspace().active_scene.as_ref().unwrap();
    send(
        &mut application,
        authoring::compose_action(
            scene,
            SceneCompositionCommand::MoveEntity {
                target: PrefabEntityTarget::source(entity(0)),
                to: ScenePlacement::first(ScenePlacementParent::Root),
            },
        ),
    );
    let changed = application.current_frame().unwrap();
    let updated = paint(&mut owner, &changed);
    assert_eq!(input(&updated), field);
    assert_eq!(owner.input_token(&updated, &field).unwrap(), Some(token));
    assert!(owner.is_composing(&updated, &field, token).unwrap());
    owner
        .edit(&updated, &field, token, &TextEdit::Commit("-0001".into()))
        .unwrap();
    assert_eq!(owner.filter.editor().text(), "entity-0001");

    let model = application.current_frame().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            model.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: model.reopen_target(),
            })],
        ),
        Duration::ZERO,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while application
        .current_frame()
        .unwrap()
        .persistence()
        .operation()
        .is_some()
        && Instant::now() < deadline
    {
        let model = application.current_frame().unwrap();
        application.submit_and_drive(EditorActionBatch::new(model.revision(), []), Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let reopened = application.current_frame().unwrap();
    assert!(matches!(
        reopened.persistence().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    let replacement = paint(&mut owner, &reopened);
    assert_ne!(input(&replacement), field);
    assert!(!owner.filter.editor().is_composing());
    assert!(
        owner
            .edit(&updated, &field, token, &TextEdit::Commit("late".into()))
            .is_err()
    );
    assert!(owner.ime_area(&updated, &field, token).is_err());
}

#[test]
fn filter_pointer_uses_the_fields_own_scroll_geometry_and_rejects_changed_display() {
    let (_project, application) = application(EditorDocumentKind::Scene, 4);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let mut routing = RoutedInput::new(&frame);
    let (field, token) = focus(&mut owner, &frame, &mut routing);
    let long = "wide filter ".repeat(25);
    owner
        .edit(&frame, &field, token, &TextEdit::Insert(long.clone()))
        .unwrap();
    let scrolled = paint(&mut owner, &model);
    let area = owner.ime_area(&scrolled, &field, token).unwrap();
    let width = owner
        .retained(&scrolled)
        .unwrap()
        .input
        .as_ref()
        .unwrap()
        .width;
    assert!(area.origin().x() < width);
    let selection = owner.filter.editor().selection_bytes();
    owner
        .point(&scrolled, &field, token, UiPoint::new(6.0, 12.0), false)
        .unwrap();
    let moved = owner.filter.editor().selection_bytes();
    assert!(
        moved.start < selection.start,
        "field-local pointing respects the horizontally scrolled text origin"
    );
    owner
        .edit(
            &scrolled,
            &field,
            token,
            &TextEdit::Insert("changed".into()),
        )
        .unwrap();
    assert!(matches!(
        owner.point(&scrolled, &field, token, UiPoint::new(6.0, 12.0), false),
        Err(OwnedHierarchyError::ChangedProjection)
    ));
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
}

#[test]
fn long_filter_refocus_points_into_the_presented_unfocused_prefix() {
    let (project, application) = application(EditorDocumentKind::Scene, 4);
    let model = application.current_frame().unwrap();
    let source = fs::read(project.source(EditorDocumentKind::Scene)).unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let mut routing = RoutedInput::new(&frame);
    let (field, old_token) = focus(&mut owner, &frame, &mut routing);
    let long = "wide filter ".repeat(25);
    owner
        .edit(&frame, &field, old_token, &TextEdit::Insert(long.clone()))
        .unwrap();
    let focused = paint(&mut owner, &model);
    let width = owner
        .retained(&focused)
        .unwrap()
        .input
        .as_ref()
        .unwrap()
        .width;
    assert!(owner.filter.editor().layout().width() > width);
    assert_eq!(owner.filter.editor().selection_bytes().start, long.len());
    routing.focus(&mut owner, &focused, &button(&focused, "+ Root"));
    assert_eq!(owner.input_token(&focused, &field).unwrap(), None);
    let unfocused = paint(&mut owner, &model);
    let (_, token) = focus(&mut owner, &unfocused, &mut routing);
    assert_ne!(token, old_token);
    assert!(matches!(
        owner.point(
            &unfocused,
            &field,
            old_token,
            UiPoint::new(6.0, 12.0),
            false
        ),
        Err(OwnedHierarchyError::TextField(UiTextFieldError::StaleInput))
    ));
    owner
        .point(&unfocused, &field, token, UiPoint::new(6.0, 12.0), false)
        .unwrap();
    owner
        .edit(
            &unfocused,
            &field,
            token,
            &TextEdit::Insert("prefix-".into()),
        )
        .unwrap();
    assert_eq!(owner.filter.editor().text(), format!("prefix-{long}"));
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
    assert_eq!(
        application.current_frame().unwrap().revision(),
        model.revision()
    );
    assert_eq!(
        fs::read(project.source(EditorDocumentKind::Scene)).unwrap(),
        source
    );
}

#[test]
fn repeated_source_scroll_deltas_accumulate_before_repaint_and_revoke_old_buttons() {
    let (_project, application) = application(EditorDocumentKind::Scene, 100);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let mut routing = RoutedInput::new(&frame);
    let (field, token) = focus(&mut owner, &frame, &mut routing);
    owner
        .edit(&frame, &field, token, &TextEdit::Insert("entity".into()))
        .unwrap();
    let frame = paint(&mut owner, &model);
    let selection = owner.filter.editor().selection_bytes();
    let first = button(&frame, "Select entity-0000");
    assert!(owner.scroll(&frame, 48.0).unwrap());
    assert!(matches!(
        owner.activate(&frame, &first, &model, false),
        Err(OwnedHierarchyError::ChangedProjection)
    ));
    assert!(owner.scroll(&frame, 48.0).unwrap());
    assert_eq!(owner.source_scroll, 96.0);
    assert!(owner.scroll(&frame, -20.0).unwrap());
    assert_eq!(owner.source_scroll, 76.0);
    assert_eq!(owner.choice_scroll, 0.0);
    assert_eq!(owner.filter.editor().text(), "entity");
    assert_eq!(owner.filter.editor().selection_bytes(), selection);
    assert_eq!(owner.input_token(&frame, &field).unwrap(), Some(token));
    let limit = owner.retained(&frame).unwrap().source_limit;
    assert!(owner.scroll(&frame, f32::MAX).unwrap());
    assert_eq!(owner.source_scroll, limit);
    assert!(!owner.scroll(&frame, 48.0).unwrap());
    assert!(owner.scroll(&frame, -f32::MAX).unwrap());
    assert_eq!(owner.source_scroll, 0.0);
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
    assert_eq!(
        application.current_frame().unwrap().revision(),
        model.revision()
    );
}

#[test]
fn repeated_parent_and_order_scroll_deltas_accumulate_and_revoke_old_choices() {
    let (_project, application) = application(EditorDocumentKind::Scene, 100);
    let model = application.current_frame().unwrap();
    for (menu, old_choice) in [
        ("Parent", "Parent entity: entity-0001"),
        ("Order", "Order: Before entity-0001"),
    ] {
        let mut owner = controller();
        let frame = paint(&mut owner, &model);
        let mut routing = RoutedInput::new(&frame);
        let (field, token) = focus(&mut owner, &frame, &mut routing);
        owner
            .edit(&frame, &field, token, &TextEdit::Insert("entity".into()))
            .unwrap();
        let frame = paint(&mut owner, &model);
        activate(&mut owner, &frame, menu, &model);
        let choices = paint(&mut owner, &model);
        let selection = owner.filter.editor().selection_bytes();
        let key = button(&choices, old_choice);
        assert!(owner.scroll(&choices, 48.0).unwrap());
        assert!(matches!(
            owner.activate(&choices, &key, &model, false),
            Err(OwnedHierarchyError::ChangedProjection)
        ));
        assert!(owner.scroll(&choices, 48.0).unwrap());
        assert_eq!(owner.choice_scroll, 96.0, "{menu}");
        assert_eq!(owner.source_scroll, 0.0);
        let limit = owner.retained(&choices).unwrap().choice_limit;
        assert!(owner.scroll(&choices, f32::MAX).unwrap());
        assert_eq!(owner.choice_scroll, limit);
        assert!(!owner.scroll(&choices, 48.0).unwrap());
        assert!(owner.scroll(&choices, -f32::MAX).unwrap());
        assert_eq!(owner.choice_scroll, 0.0);
        assert_eq!(owner.filter.editor().text(), "entity");
        assert_eq!(owner.filter.editor().selection_bytes(), selection);
        assert_eq!(owner.filter.input_token(), None);
        assert_eq!(
            application.current_frame().unwrap().workspace(),
            model.workspace()
        );
        assert_eq!(
            application.current_frame().unwrap().revision(),
            model.revision()
        );
    }
}

#[test]
fn thousand_source_rows_and_all_parent_order_choices_compile_only_the_visible_slice() {
    let (_project, application) = application(EditorDocumentKind::Scene, 1200);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    assert!(Arc::ptr_eq(&frame, &paint(&mut owner, &model)));
    eprintln!(
        "owned Hierarchy source_count={} viewport_height={} layouts={} semantics={} hits={} live_button_states={} retained_frames={}",
        model
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .entities
            .len(),
        frame.environment().logical_size().height(),
        frame.layouts().len(),
        frame.semantics().len(),
        frame.hit_regions().len(),
        owner.controls.len(),
        owner.frames.len(),
    );
    let first = button(&frame, "Select entity-0000");
    assert!(
        frame.layouts().len() < 180,
        "bounded authored/compiled row count"
    );
    assert!(frame.hit_regions().len() < 36);
    assert!(owner.controls.len() < 36);
    assert!(owner.scroll(&frame, f32::MAX).unwrap());
    assert!(owner.activate(&frame, &first, &model, false).is_err());
    let bottom = paint(&mut owner, &model);
    assert!(
        bottom
            .semantics()
            .iter()
            .any(|node| node.name() == "Select entity-1199")
    );
    assert!(!owner.controls.contains_key(&Control::Select(entity(0))));
    assert!(bottom.layouts().len() < 180 && bottom.hit_regions().len() < 36);
    assert!(owner.scroll(&bottom, -f32::MAX).unwrap());
    let top = paint(&mut owner, &model);
    assert!(activate(&mut owner, &top, "Parent", &model).is_none());
    let parent = paint(&mut owner, &model);
    assert!(
        parent
            .semantics()
            .iter()
            .filter(|node| node.role() == UiSemanticRole::Button
                && (node.name().starts_with("Parent entity: ") || node.name() == "Parent: Root"))
            .count()
            <= 25
    );
    assert!(parent.layouts().len() < 180 && parent.hit_regions().len() < 40);
    assert!(owner.scroll(&parent, f32::MAX).unwrap());
    let last_parent = paint(&mut owner, &model);
    assert!(
        last_parent
            .semantics()
            .iter()
            .any(|node| node.name() == "Parent entity: entity-1199")
    );
    assert!(activate(&mut owner, &last_parent, "Close choices", &model).is_none());
    let closed = paint(&mut owner, &model);
    assert!(activate(&mut owner, &closed, "Order", &model).is_none());
    let order = paint(&mut owner, &model);
    assert!(
        order
            .semantics()
            .iter()
            .filter(
                |node| node.role() == UiSemanticRole::Button && node.name().starts_with("Order: ")
            )
            .count()
            <= 25
    );
    assert!(order.layouts().len() < 180 && order.hit_regions().len() < 40);
    assert!(owner.scroll(&order, f32::MAX).unwrap());
    let last_order = paint(&mut owner, &model);
    assert!(
        last_order
            .semantics()
            .iter()
            .any(|node| node.name() == "Order: After entity-1199")
    );
    assert!(
        last_order.layouts().len() < 180
            && last_order.hit_regions().len() < 40
            && owner.controls.len() < 40
    );
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
    assert!(matches!(
        owner.retained(&frame),
        Err(OwnedHierarchyError::RetiredFrame)
    ));
}

#[test]
fn filtering_scrolling_and_reordering_keep_session_selection_and_unrelated_inspector_drafts() {
    use crate::frontend::imgui::interaction_tests::RetainedInspectorDraft;

    let (_project, mut application) = application(EditorDocumentKind::Scene, 12);
    let model = application.current_frame().unwrap();
    let scene = model.workspace().active_scene.as_ref().unwrap();
    send(
        &mut application,
        authoring::compose_action(
            scene,
            SceneCompositionCommand::AddComponent {
                entity: entity(0),
                component: ComponentTypeId::new(CUSTOM_COMPONENT),
            },
        ),
    );
    let model = application.current_frame().unwrap();
    let mut inspector = RetainedInspectorDraft::new(&model, environment(800.0));

    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let mut routing = RoutedInput::new(&frame);
    let (field, token) = focus(&mut owner, &frame, &mut routing);
    owner
        .edit(
            &frame,
            &field,
            token,
            &TextEdit::Insert("entity-0009".into()),
        )
        .unwrap();
    let filtered = paint(&mut owner, &model);
    assert!(
        !filtered
            .semantics()
            .iter()
            .any(|node| node.name() == "Select entity-0000")
    );
    inspector.assert_retained();
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
    activate(&mut owner, &filtered, "Clear filter", &model);
    let frame = paint(&mut owner, &model);
    assert!(owner.scroll(&frame, f32::MAX).unwrap());
    let bottom = paint(&mut owner, &model);
    assert!(!owner.controls.contains_key(&Control::Select(entity(0))));
    assert_eq!(owner.input_token(&bottom, &field).unwrap(), Some(token));
    inspector.assert_retained();
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
    activate(&mut owner, &bottom, "Order", &model);
    let choices = paint(&mut owner, &model);
    let prepared = activate(&mut owner, &choices, "Order: First", &model).unwrap();
    apply(&mut owner, &mut application, prepared);
    let reordered = application.current_frame().unwrap();
    let scene = reordered.workspace().active_scene.as_ref().unwrap();
    assert_eq!(
        scene.editor.inspector.selected_entity.as_ref(),
        Some(&entity(0))
    );
    assert_eq!(
        scene.editor.inspector.history.undo_depth,
        model
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .history
            .undo_depth
            + 1
    );
    inspector.reconcile(&reordered, environment(800.0));
}
