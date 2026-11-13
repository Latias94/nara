use super::*;

fn assert_no_refresh(owner: &OwnedHierarchy, before: HierarchyProjectionWork) {
    assert_eq!(owner.projection_work.refreshes, before.refreshes);
    assert_eq!(owner.projection_work.source_rows, before.source_rows);
    assert_eq!(
        owner.projection_work.filter_refreshes,
        before.filter_refreshes,
    );
    assert_eq!(owner.projection_work.filter_rows, before.filter_rows);
    assert_eq!(
        owner.projection_work.retained_index_bytes,
        before.retained_index_bytes,
    );
}

fn assert_reuses(owner: &mut OwnedHierarchy, model: &EditorFrameModel, frame: &Arc<UiFrame>) {
    let work = owner.work;
    let projection = owner.projection_work;
    let compiler = owner.compiler.work_stats();
    assert!(Arc::ptr_eq(frame, &paint(owner, model)));
    assert_eq!(owner.work.compile_calls - work.compile_calls, 1);
    assert_eq!(owner.work.built_snapshots, work.built_snapshots);
    assert_eq!(owner.work.authored_nodes, work.authored_nodes);
    assert_eq!(owner.work.reused_frames - work.reused_frames, 1);
    assert_eq!(
        owner.projection_work.early_reused_frames - projection.early_reused_frames,
        1,
    );
    assert_no_refresh(owner, projection);
    assert_eq!(owner.compiler.work_stats(), compiler);
}

fn repaint(
    owner: &mut OwnedHierarchy,
    model: &EditorFrameModel,
    previous: &Arc<UiFrame>,
) -> Arc<UiFrame> {
    let snapshots = owner.work.built_snapshots;
    let frame = paint(owner, model);
    assert!(!Arc::ptr_eq(previous, &frame));
    assert_eq!(owner.work.built_snapshots - snapshots, 1);
    assert_reuses(owner, model, &frame);
    frame
}

#[test]
fn pointer_movement_within_one_button_reuses_without_rebuilding_the_description() {
    let (_project, application) = application(EditorDocumentKind::Scene, 4);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let initial = paint(&mut owner, &model);
    let key = button(&initial, "Select entity-0001");
    let rect = initial
        .semantics()
        .iter()
        .find(|node| node.key() == &key)
        .unwrap()
        .rect();
    let center = UiPoint::new(
        rect.origin().x() + rect.size().width() * 0.5,
        rect.origin().y() + rect.size().height() * 0.5,
    );
    let pointer = |position| {
        UiInputEvent::Pointer(nara_ui_kernel::UiPointerInput::moved(
            UiSeatId::PRIMARY,
            UiPointerId::PRIMARY,
            position,
        ))
    };
    let mut routing = RoutedInput::new(&initial);
    assert!(
        routing
            .route(&mut owner, &initial, pointer(center))
            .intents()
            .is_empty()
    );
    let hovered = repaint(&mut owner, &model, &initial);
    for offset in [0.25, 0.5, 0.0, -0.25] {
        assert!(
            routing
                .route(
                    &mut owner,
                    &hovered,
                    pointer(UiPoint::new(center.x() + offset, center.y())),
                )
                .intents()
                .is_empty()
        );
        assert_reuses(&mut owner, &model, &hovered);
    }
    assert!(
        routing
            .route(&mut owner, &hovered, pointer(UiPoint::new(0.0, 0.0)))
            .intents()
            .is_empty()
    );
    let idle = repaint(&mut owner, &model, &hovered);
    routing.focus(&mut owner, &idle, &key);
    let focused = repaint(&mut owner, &model, &idle);
    routing.focus(&mut owner, &focused, &key);
    assert_reuses(&mut owner, &model, &focused);
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace(),
    );
}

#[test]
fn source_parent_and_order_scroll_repaint_without_source_or_filter_refreshes() {
    let (_project, mut application) = application(EditorDocumentKind::Scene, 128);
    select(&mut application, entity(57));
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let projection = owner.projection_work;
    assert_eq!(projection.retained_index_bytes, 0);
    assert!(owner.scroll(&frame, f32::MAX).unwrap());
    let mut frame = repaint(&mut owner, &model, &frame);
    button(&frame, "Select entity-0127");
    assert!(frame.layouts().len() < 128);
    assert_no_refresh(&owner, projection);
    let mut routing = RoutedInput::new(&frame);

    for (menu, last) in [
        ("Parent", "Parent entity: entity-0127"),
        ("Order", "Order: After entity-0127"),
    ] {
        let intents = routing.click(&mut owner, &frame, &button(&frame, menu));
        assert_eq!(intents.intents().len(), 1);
        assert!(
            owner
                .activate(&frame, intents.intents()[0].target(), &model, false)
                .unwrap()
                .is_none()
        );
        let choices = repaint(&mut owner, &model, &frame);
        assert!(owner.scroll(&choices, f32::MAX).unwrap());
        let bottom = repaint(&mut owner, &model, &choices);
        button(&bottom, last);
        assert!(bottom.layouts().len() < 128);
        assert_no_refresh(&owner, projection);
        let intents = routing.click(&mut owner, &bottom, &button(&bottom, "Close choices"));
        assert_eq!(intents.intents().len(), 1);
        assert!(
            owner
                .activate(&bottom, intents.intents()[0].target(), &model, false)
                .unwrap()
                .is_none()
        );
        frame = repaint(&mut owner, &model, &bottom);
        assert_no_refresh(&owner, projection);
    }
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace(),
    );
}

#[test]
fn committed_filter_retains_one_bounded_index_across_preedit_scroll_and_clear() {
    let (_project, application) = application(EditorDocumentKind::Scene, 128);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let field = input(&frame);
    let mut routing = RoutedInput::new(&frame);
    routing.focus(&mut owner, &frame, &field);
    let token = owner.input_token(&frame, &field).unwrap().unwrap();
    let before = owner.projection_work;
    owner
        .edit(&frame, &field, token, &TextEdit::Insert("ENTITY-00".into()))
        .unwrap();
    let filtered = repaint(&mut owner, &model, &frame);
    assert_eq!(owner.projection_work.refreshes, before.refreshes);
    assert_eq!(owner.projection_work.source_rows, before.source_rows);
    assert_eq!(
        owner.projection_work.filter_refreshes - before.filter_refreshes,
        1,
    );
    assert_eq!(owner.projection_work.filter_rows - before.filter_rows, 128);
    assert!(owner.projection_work.retained_index_bytes > 0);
    assert!(owner.projection_work.retained_index_bytes <= 128 * std::mem::size_of::<usize>());
    assert!(labels(&filtered).contains(&"100 of 128 source entities".into()));
    let projection = owner.projection_work;
    let committed_rows = labels(&filtered);
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
    let composing = repaint(&mut owner, &model, &filtered);
    assert!(owner.is_composing(&composing, &field, token).unwrap());
    assert_eq!(labels(&composing), committed_rows);
    assert_no_refresh(&owner, projection);
    assert!(owner.scroll(&composing, f32::MAX).unwrap());
    let scrolled = repaint(&mut owner, &model, &composing);
    button(&scrolled, "Select entity-0099");
    assert!(!owner.controls.contains_key(&Control::Select(entity(127))));
    assert!(scrolled.layouts().len() < 128);
    assert_no_refresh(&owner, projection);
    owner
        .edit(&scrolled, &field, token, &TextEdit::CancelComposition)
        .unwrap();
    let committed = repaint(&mut owner, &model, &scrolled);
    assert_no_refresh(&owner, projection);
    owner
        .edit(&committed, &field, token, &TextEdit::SelectAll)
        .unwrap();
    owner
        .edit(
            &committed,
            &field,
            token,
            &TextEdit::Insert("no match".into()),
        )
        .unwrap();
    let empty = repaint(&mut owner, &model, &committed);
    assert!(labels(&empty).contains(&"No matching source entities".into()));
    assert_eq!(owner.projection_work.refreshes, projection.refreshes);
    assert_eq!(owner.projection_work.source_rows, projection.source_rows);
    assert_eq!(
        owner.projection_work.filter_refreshes - projection.filter_refreshes,
        1,
    );
    assert_eq!(
        owner.projection_work.filter_rows - projection.filter_rows,
        128
    );
    assert!(owner.projection_work.retained_index_bytes <= 128 * std::mem::size_of::<usize>());
    let before_clear = owner.projection_work;
    owner.clear_filter().unwrap();
    let unfiltered = repaint(&mut owner, &model, &empty);
    assert_eq!(owner.projection_work.retained_index_bytes, 0);
    assert_eq!(owner.projection_work.refreshes, before_clear.refreshes);
    assert_eq!(owner.projection_work.source_rows, before_clear.source_rows);
    assert_eq!(
        owner.projection_work.filter_refreshes - before_clear.filter_refreshes,
        1,
    );
    assert_eq!(owner.projection_work.filter_rows, before_clear.filter_rows);
    button(&unfiltered, "Select entity-0000");
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace(),
    );
    assert_eq!(
        application.current_frame().unwrap().revision(),
        model.revision(),
    );
}

#[test]
fn new_frame_with_same_document_revision_refreshes_rows_without_retargeting_old_input() {
    let (_project, application) = application(EditorDocumentKind::Scene, 4);
    let original = application.current_frame().unwrap();
    let mut workspace = original.workspace().clone();
    workspace
        .active_scene
        .as_mut()
        .unwrap()
        .editor
        .inspector
        .entities[1]
        .name = Some("Needle".into());
    let model = republish(&original, workspace);
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let field = input(&frame);
    let mut routing = RoutedInput::new(&frame);
    routing.focus(&mut owner, &frame, &field);
    let token = owner.input_token(&frame, &field).unwrap().unwrap();
    owner
        .edit(&frame, &field, token, &TextEdit::Insert("needle".into()))
        .unwrap();
    let filtered = repaint(&mut owner, &model, &frame);
    let old_key = button(&filtered, "Select entity-0001");
    let basis = owner.basis.clone();
    let before = owner.projection_work;
    let mut workspace = model.workspace().clone();
    let rows = &mut workspace
        .active_scene
        .as_mut()
        .unwrap()
        .editor
        .inspector
        .entities;
    rows.swap(1, 2);
    rows[1].id = SceneEntityId::new("replacement").unwrap();
    rows[1].name = Some("Needle renamed".into());
    rows[1].inspectable_component_count = 7;
    rows[1].placement = Some(ScenePlacement::first(ScenePlacementParent::Root));
    rows[2].name = None;
    let changed = republish(&model, workspace);
    assert_ne!(changed.revision(), model.revision());
    assert_eq!(
        changed.workspace().active_scene.as_ref().unwrap().revision,
        model.workspace().active_scene.as_ref().unwrap().revision,
    );
    let refreshed = repaint(&mut owner, &changed, &filtered);
    assert_eq!(owner.basis, basis);
    assert_eq!(owner.projection_work.refreshes - before.refreshes, 1);
    assert_eq!(owner.projection_work.source_rows - before.source_rows, 4);
    assert_eq!(
        owner.projection_work.filter_refreshes - before.filter_refreshes,
        1,
    );
    assert_eq!(owner.projection_work.filter_rows - before.filter_rows, 4);
    assert!(owner.projection_work.retained_index_bytes <= 4 * std::mem::size_of::<usize>());
    assert_eq!(input(&refreshed), field);
    assert_eq!(owner.input_token(&refreshed, &field).unwrap(), Some(token));
    let replacement = button(&refreshed, "Select replacement");
    assert_ne!(replacement, old_key);
    let names = labels(&refreshed);
    assert!(names.contains(&"Needle renamed / 7 components".into()));
    assert!(names.contains(&authoring::placement_intent_label(Some(
        &ScenePlacement::first(ScenePlacementParent::Root),
    ))));
    assert!(!names.contains(&"Select entity-0001".into()));
    assert!(
        owner
            .invalidated_controls(&filtered, &refreshed)
            .unwrap()
            .contains(&old_key)
    );
    assert!(
        routing
            .click(&mut owner, &filtered, &old_key)
            .intents()
            .is_empty()
    );
    assert!(matches!(
        owner.activate(&filtered, &old_key, &changed, false),
        Err(OwnedHierarchyError::ChangedProjection)
    ));
    owner.clear_filter().unwrap();
    let reordered = repaint(&mut owner, &changed, &refreshed);
    let rows = reordered
        .semantics()
        .iter()
        .filter(|node| node.role() == UiSemanticRole::Button && node.name().starts_with("Select "))
        .map(|node| node.name())
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            "Select entity-0000",
            "Select replacement",
            "Select entity-0001",
            "Select entity-0003",
        ],
    );
    assert_eq!(owner.projection_work.retained_index_bytes, 0);
    let before = owner.projection_work;
    let read_only = owner
        .compile(&changed, environment(800.0), UiTheme::default(), true)
        .unwrap();
    assert!(!Arc::ptr_eq(&reordered, &read_only));
    assert_eq!(owner.projection_work.refreshes - before.refreshes, 1);
    assert_eq!(owner.projection_work.source_rows - before.source_rows, 4);
    assert_eq!(owner.projection_work.filter_rows, before.filter_rows);
    assert!(Arc::ptr_eq(
        &read_only,
        &owner
            .compile(&changed, environment(800.0), UiTheme::default(), true)
            .unwrap(),
    ));
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        original.workspace(),
    );
}

#[test]
fn field_focus_selection_point_blur_and_feedback_repaint_then_reuse_without_refiltering() {
    let (_project, application) = application(EditorDocumentKind::Scene, 4);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let before = owner.projection_work;
    let field = input(&frame);
    let mut routing = RoutedInput::new(&frame);
    routing.focus(&mut owner, &frame, &field);
    let token = owner.input_token(&frame, &field).unwrap().unwrap();
    let focused = repaint(&mut owner, &model, &frame);
    assert_no_refresh(&owner, before);
    owner
        .edit(&focused, &field, token, &TextEdit::Insert("entity".into()))
        .unwrap();
    let populated = repaint(&mut owner, &model, &focused);
    let projection = owner.projection_work;
    let outcome = owner
        .edit(&populated, &field, token, &TextEdit::SelectAll)
        .unwrap();
    assert!(!outcome.text_changed);
    assert!(outcome.changed());
    let selected = repaint(&mut owner, &model, &populated);
    assert_eq!(
        owner
            .selected_text(&selected, &field, token)
            .unwrap()
            .as_deref(),
        Some("entity"),
    );
    assert_no_refresh(&owner, projection);
    owner
        .point(&selected, &field, token, UiPoint::new(6.0, 12.0), false)
        .unwrap();
    let pointed = repaint(&mut owner, &model, &selected);
    assert_eq!(owner.selected_text(&pointed, &field, token).unwrap(), None);
    assert_no_refresh(&owner, projection);
    owner.input_error(
        &pointed,
        &field,
        token,
        "Input rejected by the hosted field".into(),
    );
    let input_error = repaint(&mut owner, &model, &pointed);
    assert!(labels(&input_error).contains(&"Input rejected by the hosted field".into()));
    assert_no_refresh(&owner, projection);
    owner.blur(&input_error, &field).unwrap();
    let blurred = repaint(&mut owner, &model, &input_error);
    assert_eq!(owner.input_token(&blurred, &field).unwrap(), None);
    assert_no_refresh(&owner, projection);
    owner.submission_failed("Original operation was refused".into());
    let refused = repaint(&mut owner, &model, &blurred);
    assert!(labels(&refused).contains(&"Original operation was refused".into()));
    assert_no_refresh(&owner, projection);
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace(),
    );
    assert_eq!(
        application.current_frame().unwrap().revision(),
        model.revision(),
    );
}
