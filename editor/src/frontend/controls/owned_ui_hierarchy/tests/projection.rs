use super::*;

#[test]
fn protected_roots_and_mixed_descendants_display_owner_reason_while_selection_remains_available() {
    let project = UiProject::new();
    let ids = ["branch", "leaf", "tail"].map(|id| SceneEntityId::new(id).unwrap());
    install_tree(&project, &ids, true);
    let mut fixture = Fixture::open(project);
    let mut owner = controller();
    fixture.select("ui-root");
    let root = paint(&mut owner, &fixture, 800.0);
    assert!(!enabled(&root, "Move subtree"));
    assert!(!enabled(&root, "Delete subtree"));
    assert!(
        root.semantics()
            .iter()
            .any(|node| node.name().contains("ProtectedRoot"))
    );
    assert!(enabled(&root, "Select UI branch"));
    let mut routing = RoutedInput::new(&root);
    let selected = click(
        &mut owner,
        &mut routing,
        &root,
        "Select UI branch",
        &fixture.model(),
        &fixture.ui(),
    )
    .unwrap();
    let action = selected
        .into_action(&fixture.model(), &fixture.ui(), false)
        .unwrap();
    owner.admitted(Ok(id(1)));
    let outcome = fixture.send(action);
    owner.action_finished(id(1), &PanelActionDispatch::Dispatched(outcome));
    let mixed = paint(&mut owner, &fixture, 800.0);
    assert!(!enabled(&mixed, "Move subtree"));
    assert!(!enabled(&mixed, "Delete subtree"));
    assert!(
        mixed
            .semantics()
            .iter()
            .any(|node| node.name().contains("OmittedSourceDescendant"))
    );
    assert!(
        !mixed
            .semantics()
            .iter()
            .any(|node| node.name() == "Select UI omitted-source-descendant")
    );
    assert!(
        mixed
            .semantics()
            .iter()
            .any(|node| node.name().starts_with("Container:")),
        "kind and source-owned display metadata survive the replacement"
    );
    let locked = owner
        .compile(
            &fixture.model(),
            &fixture.ui(),
            environment(800.0),
            UiTheme::default(),
            true,
        )
        .unwrap();
    assert!(!enabled(&locked, "Select UI tail"));
    assert_eq!(fixture.history(), 0);
}

#[test]
fn source_admitted_structure_uses_owner_recipes_without_enabling_direct_manipulation() {
    let project = UiProject::new();
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(&project.0).with_profile("desktop"));
    application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_none() {
        assert!(Instant::now() < deadline);
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
    setup_ui_prefab_source(&mut application);
    let fixture = Fixture {
        application,
        project,
    };
    assert!(fixture.ui().structure_editing_enabled());
    assert!(!fixture.ui().direct_editing_enabled());
    let mut owner = controller();
    let frame = paint(&mut owner, &fixture, 800.0);
    assert!(enabled(&frame, "Add UI node"));
    assert!(!enabled(&frame, "Move subtree"));
    assert!(enabled(&frame, "Select UI image-node"));
    let mut routing = RoutedInput::new(&frame);
    let selected = click(
        &mut owner,
        &mut routing,
        &frame,
        "Select UI image-node",
        &fixture.model(),
        &fixture.ui(),
    )
    .unwrap();
    assert!(
        matches!(selected.into_action(&fixture.model(), &fixture.ui(), false).unwrap(), EditorAction::Navigate(EditorNavigationAction::SelectEntity { entity: Some(entity), .. }) if entity.as_str() == "image-node")
    );
}

#[test]
fn scroll_builds_only_visible_source_and_placement_rows_accumulates_deltas_and_retires_old_actions()
{
    let project = UiProject::new();
    let ids = (0..64)
        .map(|index| SceneEntityId::new(format!("entity-{index:04}")).unwrap())
        .collect::<Vec<_>>();
    install_tree(&project, &ids, false);
    let fixture = Fixture::open(project);
    let mut owner = controller();
    let frame = paint(&mut owner, &fixture, 260.0);
    assert!(owner.retains_frame(&frame));
    let foreign = paint(&mut controller(), &fixture, 260.0);
    assert!(
        !owner.retains_frame(&foreign),
        "an equal-root predecessor owner cannot lend its compiled frame to this owner"
    );
    assert!(
        Arc::ptr_eq(&frame, &paint(&mut owner, &fixture, 260.0)),
        "unchanged compilation reuses the exact frame"
    );
    assert!(owner.controls.len() < 12);
    let key = button(&frame, "Select UI ui-root");
    owner.scroll(&frame, 52.0).unwrap();
    owner.scroll(&frame, 52.0).unwrap();
    assert_eq!(owner.source_scroll, 104.0);
    assert!(matches!(
        owner.activate(&frame, &key, &fixture.model(), &fixture.ui(), false),
        Err(OwnedUiHierarchyError::ChangedProjection)
    ));
    let scrolled = paint(&mut owner, &fixture, 260.0);
    assert!(owner.controls.len() < 12);
    assert!(
        owner
            .invalidated_controls(&frame, &scrolled)
            .unwrap()
            .contains(&key)
    );
    let mut routing = RoutedInput::new(&scrolled);
    click(
        &mut owner,
        &mut routing,
        &scrolled,
        "Add UI node",
        &fixture.model(),
        &fixture.ui(),
    );
    let form = paint(&mut owner, &fixture, 260.0);
    click(
        &mut owner,
        &mut routing,
        &form,
        "Choose placement",
        &fixture.model(),
        &fixture.ui(),
    );
    let choices = paint(&mut owner, &fixture, 260.0);
    assert!(owner.controls.len() < 12);
    let first = choices
        .semantics()
        .iter()
        .find(|node| node.name().starts_with("Placement: "))
        .unwrap()
        .key()
        .clone();
    owner.scroll(&choices, 30.0).unwrap();
    owner.scroll(&choices, 30.0).unwrap();
    assert_eq!(owner.choice_scroll, 60.0);
    assert!(
        owner
            .activate(&choices, &first, &fixture.model(), &fixture.ui(), false)
            .is_err()
    );
    for _ in 0..4 {
        let frame = paint(&mut owner, &fixture, 260.0);
        owner.scroll(&frame, 30.0).unwrap();
    }
    paint(&mut owner, &fixture, 260.0);
    assert!(owner.frames.len() <= RETAINED_FRAMES);
    assert!(matches!(
        owner.retained(&choices),
        Err(OwnedUiHierarchyError::RetiredFrame)
    ));
    assert!(!owner.retains_frame(&choices));
    let current = Arc::clone(owner.compiled_frame().unwrap());
    assert!(owner.scroll(&current, f32::NAN).is_err());
    assert_eq!(fixture.history(), 0);
}

#[test]
fn sixteen_kib_ids_keep_distinct_bounded_labels_full_identity_and_exact_source_navigation() {
    let project = UiProject::new();
    let prefix = "x".repeat(8192);
    let suffix = "y".repeat(8192 - 3);
    let ids = ["a", "b", "c"]
        .map(|middle| SceneEntityId::new(format!("{prefix}-{middle}-{suffix}")).unwrap());
    assert!(ids.iter().all(|id| id.as_str().len() == 16 * 1024));
    install_tree(&project, &ids, false);
    let fixture = Fixture::open(project);
    for height in [800.0, 2160.0, 4320.0] {
        let mut owner = controller();
        let frame = paint(&mut owner, &fixture, height);
        assert!(
            frame
                .semantics()
                .iter()
                .all(|node| node.name().len() <= 128)
        );
        let names = ids
            .iter()
            .map(|entity| {
                let key = &owner.controls[&Control::Select(entity.clone())].key;
                frame
                    .semantics()
                    .iter()
                    .find(|node| node.key() == key)
                    .unwrap()
                    .name()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_ne!(names[0], names[1]);
        assert!(names.iter().all(|name| name.contains("bytes, ")));
        let mut routing = RoutedInput::new(&frame);
        let prepared = click(
            &mut owner,
            &mut routing,
            &frame,
            &names[2],
            &fixture.model(),
            &fixture.ui(),
        )
        .unwrap();
        assert!(
            matches!(prepared.into_action(&fixture.model(), &fixture.ui(), false).unwrap(), EditorAction::Navigate(EditorNavigationAction::SelectEntity { entity: Some(actual), .. }) if actual == ids[2])
        );
    }
}

#[test]
fn compact_modal_apply_cancel_and_chooser_back_remain_reachable_and_retired_forms_refuse() {
    let fixture = Fixture::new();
    for height in [260.0, 160.0, 80.0] {
        let mut owner = controller();
        let tree = paint(&mut owner, &fixture, height);
        let mut routing = RoutedInput::new(&tree);
        click(
            &mut owner,
            &mut routing,
            &tree,
            "Add UI node",
            &fixture.model(),
            &fixture.ui(),
        );
        let form = paint(&mut owner, &fixture, height);
        for name in ["Apply structure change", "Cancel structure change"] {
            let key = button(&form, name);
            let rect = form
                .layouts()
                .iter()
                .find(|layout| layout.key() == Some(&key))
                .unwrap();
            assert!(rect.visible());
            assert!(rect.rect().origin().y() >= 0.0);
            assert!(rect.rect().origin().y() + rect.rect().size().height() <= height);
            assert!(enabled(&form, name));
        }
        let old_apply = button(&form, "Apply structure change");
        let identity = owner.form.as_ref().unwrap().identity;
        assert!(owner.escape(&form).unwrap());
        let tree = paint(&mut owner, &fixture, height);
        click(
            &mut owner,
            &mut routing,
            &tree,
            "Add UI node",
            &fixture.model(),
            &fixture.ui(),
        );
        assert_ne!(owner.form.as_ref().unwrap().identity, identity);
        assert!(
            owner
                .activate(&form, &old_apply, &fixture.model(), &fixture.ui(), false)
                .is_err()
        );
        let current = paint(&mut owner, &fixture, height);
        click(
            &mut owner,
            &mut routing,
            &current,
            "Cancel structure change",
            &fixture.model(),
            &fixture.ui(),
        );
        assert!(owner.form.is_none());
    }
    assert_eq!(fixture.history(), 0);
}
