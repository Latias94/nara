use super::*;

fn submit_named(owner: &mut OwnedHierarchy, application: &mut EditorApplication, name: &str) {
    let model = application.current_frame().unwrap();
    let frame = paint(owner, &model);
    let prepared =
        activate(owner, &frame, name, &model).expect("a source action prepares one submission");
    apply(owner, application, prepared);
}

fn place(
    owner: &mut OwnedHierarchy,
    application: &mut EditorApplication,
    menu: &str,
    choice: &str,
) {
    let model = application.current_frame().unwrap();
    let frame = paint(owner, &model);
    assert!(activate(owner, &frame, menu, &model).is_none());
    let choices = paint(owner, &model);
    let prepared = activate(owner, &choices, choice, &model).unwrap();
    apply(owner, application, prepared);
}

fn settle_persistence(application: &mut EditorApplication) {
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
    assert!(
        application
            .current_frame()
            .unwrap()
            .persistence()
            .operation()
            .is_none()
    );
}

#[test]
fn scene_and_prefab_source_workflows_use_session_commands_history_save_and_reopen() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let (project, mut application) = application(kind, 3);
        let mut owner = controller();
        let before = application.current_frame().unwrap();
        let document = before.workspace().active_document.unwrap();
        let original_ids = before
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .entities
            .iter()
            .map(|row| row.id.clone())
            .collect::<Vec<_>>();
        submit_named(&mut owner, &mut application, "+ Root");
        let model = application.current_frame().unwrap();
        let root = model
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .editor
            .inspector
            .selected_entity
            .clone()
            .unwrap();
        assert!(!original_ids.contains(&root));
        assert_eq!(
            model
                .workspace()
                .document(document)
                .unwrap()
                .history
                .undo_depth,
            1
        );
        submit_named(&mut owner, &mut application, "+ Child");
        let model = application.current_frame().unwrap();
        let scene = model.workspace().active_scene.as_ref().unwrap();
        let child = scene.editor.inspector.selected_entity.clone().unwrap();
        assert_eq!(
            scene
                .editor
                .inspector
                .entities
                .iter()
                .find(|row| row.id == child)
                .unwrap()
                .placement,
            Some(ScenePlacement::last(ScenePlacementParent::Entity(
                PrefabEntityTarget::source(root.clone())
            )))
        );

        place(
            &mut owner,
            &mut application,
            "Parent",
            "Parent entity: entity-0001",
        );
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
                .iter()
                .find(|row| row.id == child)
                .unwrap()
                .placement,
            Some(ScenePlacement::last(ScenePlacementParent::Entity(
                PrefabEntityTarget::source(entity(1))
            )))
        );
        place(&mut owner, &mut application, "Parent", "Parent: Root");
        for (name, slot) in [
            ("Order: First", ScenePlacementSlot::First),
            ("Order: Last", ScenePlacementSlot::Last),
            (
                "Order: Before entity-0001",
                ScenePlacementSlot::Before(PrefabEntityTarget::source(entity(1))),
            ),
            (
                "Order: After entity-0001",
                ScenePlacementSlot::After(PrefabEntityTarget::source(entity(1))),
            ),
        ] {
            place(&mut owner, &mut application, "Order", name);
            let model = application.current_frame().unwrap();
            let scene = model.workspace().active_scene.as_ref().unwrap();
            assert_eq!(
                scene.editor.inspector.selected_entity.as_ref(),
                Some(&child)
            );
            assert_eq!(
                scene
                    .editor
                    .inspector
                    .entities
                    .iter()
                    .find(|row| row.id == child)
                    .unwrap()
                    .placement,
                Some(ScenePlacement {
                    parent: ScenePlacementParent::Root,
                    slot
                })
            );
        }

        let model = application.current_frame().unwrap();
        let frame = paint(&mut owner, &model);
        assert!(activate(&mut owner, &frame, "Remove Entity", &model).is_none());
        let confirmation = paint(&mut owner, &model);
        assert!(
            labels(&confirmation)
                .iter()
                .any(|text| text.contains(child.as_str()))
        );
        assert!(activate(&mut owner, &confirmation, "Cancel removal", &model).is_none());
        assert_eq!(
            application.current_frame().unwrap().workspace(),
            model.workspace()
        );
        let frame = paint(&mut owner, &model);
        assert!(activate(&mut owner, &frame, "Remove Entity", &model).is_none());
        let confirmation = paint(&mut owner, &model);
        let prepared = activate(&mut owner, &confirmation, "Confirm removal", &model).unwrap();
        assert!(
            owner
                .activate(
                    &confirmation,
                    &button(&confirmation, "Confirm removal"),
                    &model,
                    false
                )
                .is_err()
        );
        apply(&mut owner, &mut application, prepared);
        let removed = application.current_frame().unwrap();
        assert!(
            !removed
                .workspace()
                .active_scene
                .as_ref()
                .unwrap()
                .editor
                .inspector
                .entities
                .iter()
                .any(|row| row.id == child)
        );
        let undo_depth = removed
            .workspace()
            .document(document)
            .unwrap()
            .history
            .undo_depth;
        send(
            &mut application,
            EditorAction::Mutate(EditorDocumentMutation::Undo {
                document,
                base_revision: removed.workspace().document(document).unwrap().revision,
            }),
        );
        let restored = application.current_frame().unwrap();
        assert_eq!(
            restored
                .workspace()
                .document(document)
                .unwrap()
                .history
                .undo_depth
                + 1,
            undo_depth
        );
        let restored_scene = restored.workspace().active_scene.as_ref().unwrap();
        assert_eq!(
            restored_scene
                .editor
                .inspector
                .entities
                .iter()
                .find(|row| row.id == child)
                .unwrap()
                .placement,
            Some(ScenePlacement {
                parent: ScenePlacementParent::Root,
                slot: ScenePlacementSlot::After(PrefabEntityTarget::source(entity(1)))
            })
        );

        application.submit_and_drive(
            EditorActionBatch::new(
                restored.revision(),
                [EditorAction::Persistence(EditorPersistenceAction::Save {
                    document,
                    base_revision: restored_scene.revision,
                })],
            ),
            Duration::ZERO,
        );
        settle_persistence(&mut application);
        let saved = application.current_frame().unwrap();
        assert!(
            matches!(saved.persistence().result(), Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
        );
        assert!(!saved.workspace().document(document).unwrap().dirty);
        let bytes = fs::read(project.source(kind)).unwrap();
        let placements = match kind {
            EditorDocumentKind::Scene => {
                SceneDocumentCandidate::decode_json_bytes(&bytes)
                    .unwrap()
                    .into_composition_source()
                    .unwrap()
                    .placements
            }
            EditorDocumentKind::Prefab => {
                nara::scene::PrefabDocumentCandidate::decode_json_bytes(&bytes)
                    .unwrap()
                    .into_composition_source()
                    .unwrap()
                    .placements
            }
            _ => unreachable!(),
        };
        let saved_placement = placements
            .iter()
            .find(|entry| entry.target == PrefabEntityTarget::source(child.clone()))
            .unwrap()
            .placement
            .clone();
        assert_eq!(
            saved_placement,
            restored_scene
                .editor
                .inspector
                .entities
                .iter()
                .find(|row| row.id == child)
                .unwrap()
                .placement
                .clone()
                .unwrap()
        );
        let attempt = saved.persistence().attempt().unwrap();
        application.submit_and_drive(
            EditorActionBatch::new(
                saved.revision(),
                [EditorAction::Persistence(
                    EditorPersistenceAction::AcknowledgeResult { attempt },
                )],
            ),
            Duration::ZERO,
        );
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
        settle_persistence(&mut application);
        let reopened = application.current_frame().unwrap();
        assert!(matches!(
            reopened.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ));
        let scene = reopened.workspace().active_scene.as_ref().unwrap();
        assert_eq!(
            scene
                .editor
                .inspector
                .entities
                .iter()
                .find(|row| row.id == child)
                .unwrap()
                .placement,
            Some(saved_placement)
        );
        assert_eq!(scene.editor.inspector.history.undo_depth, 0);
    }
}

#[test]
fn real_pointer_dispatch_updates_button_state_before_any_document_action() {
    let (_project, application) = application(EditorDocumentKind::Scene, 3);
    let model = application.current_frame().unwrap();
    let mut owner = controller();
    let frame = paint(&mut owner, &model);
    let root = button(&frame, "+ Root");
    let rect = frame
        .hit_regions()
        .iter()
        .find(|hit| hit.key() == &root)
        .unwrap()
        .rect();
    let point = UiPoint::new(
        rect.origin().x() + rect.size().width() / 2.0,
        rect.origin().y() + rect.size().height() / 2.0,
    );
    let mut routing = RoutedInput::new(&frame);
    let event = |phase| UiInputEvent::PointerButton {
        seat: UiSeatId::PRIMARY,
        pointer: UiPointerId::PRIMARY,
        button: UiPointerButton::Primary,
        phase,
        position: Some(point),
    };
    assert!(
        routing
            .route(&mut owner, &frame, event(UiButtonPhase::Pressed))
            .intents()
            .is_empty()
    );
    assert!(owner.controls[&Control::Root].state.pressed());
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
    let intents = routing.route(&mut owner, &frame, event(UiButtonPhase::Released));
    assert!(!owner.controls[&Control::Root].state.pressed());
    assert_eq!(intents.intents().len(), 1);
    let prepared = owner
        .activate(&frame, intents.intents()[0].target(), &model, false)
        .unwrap()
        .unwrap();
    assert!(prepared.into_action(&model, false).is_ok());
    assert_eq!(
        application.current_frame().unwrap().workspace(),
        model.workspace()
    );
}

#[test]
fn order_widget_declares_an_explicit_slot_without_inventing_effective_siblings() {
    for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
        let (_project, application) = application(kind, 2);
        let original = application.current_frame().unwrap();
        let mut workspace = original.workspace().clone();
        let scene = workspace.active_scene.as_mut().unwrap();
        let selected = scene.editor.inspector.selected_entity.clone().unwrap();
        let parent = ScenePlacementParent::Entity(PrefabEntityTarget {
            through: vec![PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("instance").unwrap()),
                expected_base: SourceDocumentKey::named(
                    "f9ab807a-c61e-4c92-bfee-b6fa153b8efc".parse().unwrap(),
                    "selected-source",
                    SourceDocumentKind::Prefab,
                )
                .unwrap(),
            }],
            entity: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("root").unwrap()),
        });
        scene
            .editor
            .inspector
            .entities
            .iter_mut()
            .find(|row| row.id == selected)
            .unwrap()
            .placement = Some(ScenePlacement::last(parent.clone()));
        let document = scene.document;
        let revision = scene.revision;
        let model = republish(&original, workspace);
        let mut owner = controller();
        let frame = paint(&mut owner, &model);
        assert!(
            !frame
                .semantics()
                .iter()
                .any(|node| matches!(node.name(), "Up" | "Down"))
        );
        let mut routing = RoutedInput::new(&frame);
        let order = button(&frame, "Order");
        let intents = routing.click(&mut owner, &frame, &order);
        assert_eq!(intents.intents().len(), 1);
        assert!(
            owner
                .activate(&frame, intents.intents()[0].target(), &model, false)
                .unwrap()
                .is_none()
        );
        let choices = paint(&mut owner, &model);
        let first = button(&choices, "Order: First");
        let intents = routing.click(&mut owner, &choices, &first);
        let prepared = owner
            .activate(&choices, intents.intents()[0].target(), &model, false)
            .unwrap()
            .unwrap();
        let action = prepared.into_action(&model, false).unwrap();
        let target = PrefabEntityTarget::source(selected);
        let expected = ScenePlacement::first(parent);
        match action {
            EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                document: actual_document,
                base_revision,
                command: SceneCompositionCommand::MoveEntity { target: actual, to },
            }) => {
                assert_eq!(kind, EditorDocumentKind::Scene);
                assert_eq!((actual_document, base_revision), (document, revision));
                assert_eq!((actual, to), (target, expected));
            }
            EditorAction::Mutate(EditorDocumentMutation::ComposePrefab {
                document: actual_document,
                base_revision,
                command: PrefabCompositionCommand::MoveEntity { target: actual, to },
            }) => {
                assert_eq!(kind, EditorDocumentKind::Prefab);
                assert_eq!((actual_document, base_revision), (document, revision));
                assert_eq!((actual, to), (target, expected));
            }
            other => panic!("order intent changed source authority: {other:?}"),
        }
        assert_eq!(
            application.current_frame().unwrap().workspace(),
            original.workspace()
        );
    }
}
