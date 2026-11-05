use super::*;

#[test]
fn catalog_uses_complete_owner_recipes_one_prefab_history_record_and_exact_source() {
    let mut fixture = Fixture::new();
    let source = fixture.source();
    let mut owner = controller();
    let frame = paint(&mut owner, &fixture, 800.0);
    let mut routing = RoutedInput::new(&frame);
    for (index, kind) in UiCatalogNodeKind::ALL.into_iter().enumerate() {
        let density = if index < 2 {
            nara_ui_widgets::UiDensity::Normal
        } else {
            nara_ui_widgets::UiDensity::Compact
        };
        owner.set_appearance(
            UiAppearance::default()
                .with_density(density)
                .with_text_scale(if index % 2 == 0 { 1.0 } else { 2.0 })
                .unwrap(),
        );
        let before = fixture.ui();
        let history = fixture.history();
        click_current(&mut owner, &mut routing, &fixture, "Add UI node");
        click_current(
            &mut owner,
            &mut routing,
            &fixture,
            &format!("Kind: {}", kind.label()),
        );
        let form = paint(&mut owner, &fixture, 800.0);
        match kind {
            UiCatalogNodeKind::Container => {}
            UiCatalogNodeKind::Text => {
                set_field(&mut owner, &mut routing, &form, "Text", "Created text");
            }
            UiCatalogNodeKind::Image => {
                assert!(!enabled(&form, "Apply structure change"));
                click_current(&mut owner, &mut routing, &fixture, "Choose image");
                click_current(
                    &mut owner,
                    &mut routing,
                    &fixture,
                    "Image: textures/player.png",
                );
            }
            UiCatalogNodeKind::Button => {
                assert!(!enabled(&form, "Apply structure change"));
                set_field(&mut owner, &mut routing, &form, "Label", "Confirm");
                let form = paint(&mut owner, &fixture, 800.0);
                set_field(&mut owner, &mut routing, &form, "Action", "ui.confirm");
            }
        }
        let intent = owner.form.as_ref().unwrap().intent().unwrap();
        let expected = fixture.ui().lower_structure_intent(&intent).unwrap();
        let prepared =
            click_current(&mut owner, &mut routing, &fixture, "Apply structure change").unwrap();
        let action = apply(&mut owner, &mut fixture, prepared, index as u64 + 1);
        assert_eq!(
            action, expected,
            "the complete owner-lowered mutation is submitted unchanged"
        );
        assert!(matches!(
            action,
            EditorDocumentMutation::ComposePrefab {
                command: PrefabCompositionCommand::CreateEntity { .. },
                ..
            }
        ));
        assert_eq!(fixture.history(), history + 1);
        assert_eq!(
            fixture.ui().revision().generation(),
            before.revision().generation() + 1
        );
        let current = fixture.ui();
        let entity = current.selection().top_entity().unwrap();
        let row = current
            .projection()
            .unwrap()
            .hierarchy()
            .iter()
            .find(|node| node.entity() == entity)
            .unwrap();
        assert_eq!(row.kind().label(), kind.label());
        assert_eq!(
            fixture
                .model()
                .workspace()
                .document(current.document())
                .unwrap()
                .kind,
            EditorDocumentKind::Prefab
        );
        assert_eq!(
            fixture.source(),
            source,
            "unsaved construction never rewrites the original Prefab file"
        );
    }
    fixture.send(EditorAction::Persistence(EditorPersistenceAction::Save {
        document: fixture.ui().document(),
        base_revision: fixture.ui().revision(),
    }));
    fixture.settle();
    let saved = fixture.source();
    assert_ne!(saved, source);
    let saved: Value = serde_json::from_slice(&saved).unwrap();
    assert_eq!(saved["kind"], "prefab");
    assert_eq!(saved["payload"]["entities"].as_array().unwrap().len(), 6);
}

#[test]
fn typed_placement_and_subtree_delete_use_original_topology_and_owner_history() {
    let project = UiProject::new();
    let ids = ["branch", "leaf", "tail"].map(|id| SceneEntityId::new(id).unwrap());
    install_tree(&project, &ids, false);
    let mut fixture = Fixture::open(project);
    fixture.select("branch");
    let source = fixture.source();
    let mut owner = controller();
    let frame = paint(&mut owner, &fixture, 800.0);
    let mut routing = RoutedInput::new(&frame);
    click_current(&mut owner, &mut routing, &fixture, "Move subtree");
    click_current(&mut owner, &mut routing, &fixture, "Choose placement");
    let choices = paint(&mut owner, &fixture, 800.0);
    assert!(
        !choices
            .semantics()
            .iter()
            .any(|node| node.name().starts_with("Placement: leaf /"))
    );
    click_current(
        &mut owner,
        &mut routing,
        &fixture,
        "Placement: ui-root / after tail",
    );
    let intent = owner.form.as_ref().unwrap().intent().unwrap();
    let expected = ScenePlacement::after(
        ScenePlacementParent::Entity(PrefabEntityTarget::source(
            SceneEntityId::new("ui-root").unwrap(),
        )),
        PrefabEntityTarget::source(ids[2].clone()),
    );
    assert_eq!(
        intent,
        UiStructureIntent::Move {
            target: ids[0].clone(),
            placement: expected.clone()
        }
    );
    let prepared =
        click_current(&mut owner, &mut routing, &fixture, "Apply structure change").unwrap();
    let action = apply(&mut owner, &mut fixture, prepared, 1);
    assert!(
        matches!(action, EditorDocumentMutation::ComposePrefab { command: PrefabCompositionCommand::MoveEntity { target, to }, .. } if target == PrefabEntityTarget::source(ids[0].clone()) && to == expected)
    );
    let ui = fixture.ui();
    assert_eq!(
        ui.projection()
            .unwrap()
            .hierarchy()
            .iter()
            .map(|node| node.entity().as_str())
            .collect::<Vec<_>>(),
        ["ui-root", "tail", "branch", "leaf"]
    );
    assert_eq!(fixture.history(), 1);
    click_current(&mut owner, &mut routing, &fixture, "Delete subtree");
    let prepared =
        click_current(&mut owner, &mut routing, &fixture, "Apply structure change").unwrap();
    apply(&mut owner, &mut fixture, prepared, 2);
    assert_eq!(fixture.history(), 2);
    assert_eq!(fixture.ui().projection().unwrap().hierarchy().len(), 2);
    let ui = fixture.ui();
    fixture.send(EditorAction::Mutate(EditorDocumentMutation::Undo {
        document: ui.document(),
        base_revision: ui.revision(),
    }));
    assert_eq!(fixture.ui().projection().unwrap().hierarchy().len(), 4);
    assert_eq!(fixture.history(), 1);
    assert_eq!(fixture.source(), source);
}

#[test]
fn not_dispatched_and_admission_refusal_preserve_form_for_explicit_retry_and_ignore_duplicates() {
    let mut fixture = Fixture::new();
    let mut owner = controller();
    let frame = paint(&mut owner, &fixture, 800.0);
    let mut routing = RoutedInput::new(&frame);
    click_current(&mut owner, &mut routing, &fixture, "Add UI node");
    click_current(&mut owner, &mut routing, &fixture, "Kind: Text");
    let form = paint(&mut owner, &fixture, 800.0);
    set_field(
        &mut owner,
        &mut routing,
        &form,
        "Text",
        "Retain this exact retry value",
    );
    let original_form = owner.form.as_ref().unwrap().identity;
    let prepared =
        click_current(&mut owner, &mut routing, &fixture, "Apply structure change").unwrap();
    prepared
        .into_action(&fixture.model(), &fixture.ui(), false)
        .unwrap();
    owner.admitted(Ok(id(1)));
    owner.set_appearance(
        UiAppearance::default()
            .with_density(nara_ui_widgets::UiDensity::Compact)
            .with_text_scale(2.0)
            .unwrap(),
    );
    let pending = paint(&mut owner, &fixture, 800.0);
    assert!(!enabled(&pending, "Apply structure change"));
    assert!(owner.escape(&pending).unwrap());
    assert_eq!(owner.form.as_ref().unwrap().identity, original_form);
    owner.action_finished(
        id(77),
        &PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing),
    );
    assert!(owner.pending.is_some());
    owner.action_finished(
        id(1),
        &PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Superseded),
    );
    assert!(owner.pending.is_none());
    assert!(owner.error.as_ref().unwrap().contains("not dispatched"));
    assert_eq!(
        owner.form.as_ref().unwrap().value(Field::Text),
        "Retain this exact retry value"
    );
    let prepared =
        click_current(&mut owner, &mut routing, &fixture, "Apply structure change").unwrap();
    prepared
        .into_action(&fixture.model(), &fixture.ui(), false)
        .unwrap();
    owner.admitted(Err(PanelSubmitError::Capacity));
    assert_eq!(owner.form.as_ref().unwrap().identity, original_form);
    assert!(owner.error.as_ref().unwrap().contains("not queued"));
    let prepared =
        click_current(&mut owner, &mut routing, &fixture, "Apply structure change").unwrap();
    let stale = paint(&mut owner, &fixture, 800.0);
    assert!(
        owner
            .activate(
                &stale,
                &button(&stale, "Apply structure change"),
                &fixture.model(),
                &fixture.ui(),
                false
            )
            .is_err()
    );
    apply(&mut owner, &mut fixture, prepared, 2);
    assert_eq!(fixture.history(), 1);
    assert!(owner.form.is_none());
    owner.action_finished(
        id(2),
        &PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing),
    );
    assert!(
        owner.error.is_none(),
        "a duplicate old failure cannot poison an accepted operation"
    );
}
