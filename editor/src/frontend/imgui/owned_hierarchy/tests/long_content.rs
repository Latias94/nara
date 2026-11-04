use super::*;
use nara::scene::SceneEntityPlacement;

#[test]
fn admitted_16k_source_ids_publish_distinct_labels_and_keep_exact_select_parent_order_actions() {
    let project = InteractionProject::new();
    let prefix = "x".repeat(8 * 1024);
    let suffix = "y".repeat(8 * 1024 - 3);
    let entities = [
        SceneEntityId::new(format!("{prefix}-a-{suffix}")).unwrap(),
        SceneEntityId::new(format!("{prefix}-b-{suffix}")).unwrap(),
    ];
    assert!(
        entities
            .iter()
            .all(|entity| entity.as_str().len() == 16 * 1024)
    );
    let source =
        SceneDocument::from_ordered_roots(entities.iter().cloned().map(SceneEntityRecord::new))
            .to_json_string()
            .unwrap();
    let path = project.source(EditorDocumentKind::Scene);
    fs::write(&path, &source).unwrap();
    let mut application = open(&project);
    select(&mut application, entities[0].clone());
    let model = application.current_frame().unwrap();
    let scene = model.workspace().active_scene.as_ref().unwrap();
    assert_eq!(scene.editor.inspector.entities.len(), 2);
    for entity in &entities {
        assert!(
            scene
                .editor
                .inspector
                .entities
                .iter()
                .any(|row| &row.id == entity)
        );
    }

    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let names = entities
        .iter()
        .map(|entity| {
            let key = &owner.controls[&Control::Select(entity.clone())].key;
            let node = frame
                .semantics()
                .iter()
                .find(|node| node.key() == key)
                .unwrap();
            assert!(node.enabled());
            assert!(node.name().starts_with("Select xxx"));
            assert!(node.name().ends_with("yyy"));
            assert!(node.name().contains("bytes, "));
            assert!(node.name().len() <= 128);
            node.name().to_owned()
        })
        .collect::<Vec<_>>();
    assert_ne!(
        names[0], names[1],
        "equal displayed ends still have distinct digests"
    );
    assert_ne!(
        owner.controls[&Control::Select(entities[0].clone())].key,
        owner.controls[&Control::Select(entities[1].clone())].key,
    );
    let field = input(&frame);
    let mut routing = RoutedInput::new(&frame);
    routing.focus(&mut owner, &frame, &field);
    let token = owner.input_token(&frame, &field).unwrap().unwrap();
    owner
        .edit(&frame, &field, token, &TextEdit::Insert("-b-".into()))
        .unwrap();
    let filtered = paint(&mut owner, &model);
    assert!(
        owner
            .controls
            .contains_key(&Control::Select(entities[1].clone()))
    );
    assert!(
        !owner
            .controls
            .contains_key(&Control::Select(entities[0].clone()))
    );
    assert_eq!(input(&filtered), field);
    let create = activate(&mut owner, &filtered, "+ Root", &model)
        .unwrap()
        .into_action(&model, false)
        .unwrap();
    assert!(matches!(
        create,
        EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
            command: SceneCompositionCommand::CreateEntity { .. },
            ..
        })
    ));

    for entity in &entities {
        let mut owner = controller();
        let frame = paint(&mut owner, &model);
        let key = owner.controls[&Control::Select(entity.clone())].key.clone();
        let action = owner
            .activate(&frame, &key, &model, false)
            .unwrap()
            .unwrap()
            .into_action(&model, false)
            .unwrap();
        assert!(matches!(
            action,
            EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                entity: Some(actual), ..
            }) if &actual == entity
        ));
    }
    let destination = PrefabEntityTarget::source(entities[1].clone());
    for (menu, control, expected) in [
        (
            "Parent",
            Control::ParentEntity(entities[1].clone()),
            ScenePlacement::last(ScenePlacementParent::Entity(destination.clone())),
        ),
        (
            "Order",
            Control::Before(entities[1].clone()),
            ScenePlacement {
                parent: ScenePlacementParent::Root,
                slot: ScenePlacementSlot::Before(destination.clone()),
            },
        ),
        (
            "Order",
            Control::After(entities[1].clone()),
            ScenePlacement {
                parent: ScenePlacementParent::Root,
                slot: ScenePlacementSlot::After(destination),
            },
        ),
    ] {
        let mut owner = controller();
        let frame = paint(&mut owner, &model);
        activate(&mut owner, &frame, menu, &model);
        let choices = paint(&mut owner, &model);
        let key = owner.controls[&control].key.clone();
        let action = owner
            .activate(&choices, &key, &model, false)
            .unwrap()
            .unwrap()
            .into_action(&model, false)
            .unwrap();
        assert!(matches!(
            action,
            EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                command: SceneCompositionCommand::MoveEntity { target, to }, ..
            }) if target == PrefabEntityTarget::source(entities[0].clone()) && to == expected
        ));
    }
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
    assert_eq!(
        application.current_frame().unwrap().revision(),
        model.revision()
    );
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
fn oversized_unicode_names_placement_and_errors_are_bounded_on_every_text_surface() {
    let (_project, application) = application(EditorDocumentKind::Scene, 2);
    let original = application.current_frame().unwrap();
    let mut workspace = original.workspace().clone();
    let unicode = "界🙂e\u{301}".repeat(2200);
    let target = SceneEntityId::new("p".repeat(16 * 1024)).unwrap();
    for row in &mut workspace
        .active_scene
        .as_mut()
        .unwrap()
        .editor
        .inspector
        .entities
    {
        row.name = Some(unicode.clone());
        row.placement = Some(ScenePlacement::last(ScenePlacementParent::Entity(
            PrefabEntityTarget::source(target.clone()),
        )));
    }
    let model = republish(&original, workspace);
    let mut owner = controller();
    paint(&mut owner, &model);
    owner.submission_failed(format!("Action refused: {unicode}"));
    let frame = Arc::clone(owner.compiled_frame().unwrap());
    let field = input(&frame);
    let mut routing = RoutedInput::new(&frame);
    routing.focus(&mut owner, &frame, &field);
    let token = owner.input_token(&frame, &field).unwrap().unwrap();
    owner.input_error(&frame, &field, token, format!("Input refused: {unicode}"));
    let frame = paint(&mut owner, &model);
    let texts = frame
        .semantics()
        .iter()
        .map(|node| node.name())
        .collect::<Vec<_>>();
    assert!(texts.iter().all(|text| text.len() <= 128));
    for prefix in ["界🙂", "Desired:", "Action refused:", "Input refused:"] {
        assert!(
            texts
                .iter()
                .any(|text| text.starts_with(prefix) && text.contains("bytes, ")),
            "missing bounded {prefix:?} in {texts:?}"
        );
    }
    assert!(texts.iter().any(|text| *text == "Select entity-0000"));
    assert!(texts.iter().any(|text| *text == "Select entity-0001"));
    assert!(texts.iter().any(|text| *text == "+ Root"));
    input(&frame);
    activate(&mut owner, &frame, "Remove Entity", &model);
    let confirmation = paint(&mut owner, &model);
    button(&confirmation, "Confirm removal");
    button(&confirmation, "Cancel removal");
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        original.workspace()
    );
    assert_eq!(
        application.current_frame().unwrap().revision(),
        original.revision()
    );
}

#[test]
fn tall_viewports_share_the_display_budget_across_all_visible_long_id_rows_and_choices() {
    let project = InteractionProject::new();
    let prefix = "x".repeat(16 * 1024 - 4);
    let entities = (0..30)
        .map(|index| SceneEntityId::new(format!("{prefix}{index:04}")).unwrap())
        .collect::<Vec<_>>();
    let root = entities[0].clone();
    let placements = entities.iter().map(|entity| {
        SceneEntityPlacement::new(
            PrefabEntityTarget::source(entity.clone()),
            ScenePlacement::last(if entity == &root {
                ScenePlacementParent::Root
            } else {
                ScenePlacementParent::Entity(PrefabEntityTarget::source(root.clone()))
            }),
        )
    });
    let source = SceneDocument::new(
        placements,
        entities.iter().cloned().map(SceneEntityRecord::new),
    )
    .to_json_string()
    .unwrap();
    let path = project.source(EditorDocumentKind::Scene);
    fs::write(&path, &source).unwrap();
    let mut application = open(&project);
    select(&mut application, root);
    let model = application.current_frame().unwrap();
    assert_eq!(
        model
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .entities
            .len(),
        30
    );
    for height in [1080.0, 2160.0, 4320.0] {
        let mut owner = controller();
        let frame = owner
            .compile(&model, environment(height), UiTheme::default(), false)
            .unwrap();
        assert!(
            owner
                .controls
                .keys()
                .filter(|control| matches!(control, Control::Select(_)))
                .count()
                > 10
        );
        button(&frame, "+ Root");
        input(&frame);
        for menu in ["Parent", "Order"] {
            let current = Arc::clone(owner.compiled_frame().unwrap());
            activate(&mut owner, &current, menu, &model);
            let choices = owner
                .compile(&model, environment(height), UiTheme::default(), false)
                .unwrap();
            button(&choices, "Close choices");
            activate(&mut owner, &choices, "Close choices", &model);
            owner
                .compile(&model, environment(height), UiTheme::default(), false)
                .unwrap();
        }
    }
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
fn oversized_filter_input_refuses_before_replacing_the_usable_local_buffer() {
    let (_project, application) = application(EditorDocumentKind::Scene, 2);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let field = input(&frame);
    let mut routing = RoutedInput::new(&frame);
    routing.focus(&mut owner, &frame, &field);
    let token = owner.input_token(&frame, &field).unwrap().unwrap();
    owner
        .edit(&frame, &field, token, &TextEdit::Insert("entity".into()))
        .unwrap();
    let frame = paint(&mut owner, &model);
    let text = owner.filter.editor().text().to_owned();
    let selection = owner.filter.editor().selection_bytes();
    for edit in [
        TextEdit::Insert("x".repeat(FILTER_TEXT_BYTES + 1)),
        TextEdit::Preedit {
            text: "x".repeat(FILTER_TEXT_BYTES + 1),
            cursor: None,
        },
    ] {
        assert!(owner.edit(&frame, &field, token, &edit).is_err());
        assert_eq!(owner.filter.editor().text(), text);
        assert_eq!(owner.filter.editor().selection_bytes(), selection);
        assert!(!owner.filter.editor().is_composing());
        let current = paint(&mut owner, &model);
        button(&current, "+ Root");
        assert_eq!(owner.input_token(&current, &field).unwrap(), Some(token));
    }
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
}
