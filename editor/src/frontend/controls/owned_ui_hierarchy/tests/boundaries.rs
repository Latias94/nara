use nara::{
    reflect::AuthoringSchemaBundle,
    scene::AuthoringSchemaBasis,
    tooling::{
        EditorImageColorSpace, EditorImageImportAttemptView, EditorImageImportMetadataView,
        EditorImageImportSettings, EditorPersistenceRequestResult, EditorProjectBrowserLocalAction,
    },
};

use super::*;

fn with_schema(model: &EditorFrameModel, schema: AuthoringSchemaBasis) -> EditorFrameModel {
    let mut workspace = model.workspace().clone();
    let document = workspace.active_document.unwrap();
    workspace
        .documents
        .iter_mut()
        .find(|tab| tab.document == document)
        .unwrap()
        .authoring_schema_basis = Some(schema);
    EditorFrameClock::new().publish(
        workspace,
        model.project().clone(),
        Default::default(),
        EditorReopenTarget::Unavailable,
        None,
    )
}

#[test]
fn same_revision_selection_is_checked_at_activation_and_final_consumption_without_retargeting() {
    let mut fixture = Fixture::new();
    for consume in [false, true] {
        fixture.select("image-node");
        let mut owner = controller();
        let frame = paint(&mut owner, &fixture, 800.0);
        let mut routing = RoutedInput::new(&frame);
        click_current(&mut owner, &mut routing, &fixture, "Add UI node");
        let original_model = fixture.model();
        let original_ui = fixture.ui();
        let form = paint(&mut owner, &fixture, 800.0);
        let prepared = consume.then(|| {
            click(
                &mut owner,
                &mut routing,
                &form,
                "Apply structure change",
                &original_model,
                &original_ui,
            )
            .unwrap()
        });
        fixture.select("ui-root");
        assert_eq!(fixture.ui().revision(), original_ui.revision());
        if let Some(prepared) = prepared {
            assert!(matches!(
                prepared.into_action(&fixture.model(), &fixture.ui(), false),
                Err(OwnedUiHierarchyError::ChangedProjection)
            ));
            owner.submission_failed("The original selection changed".to_owned());
        } else {
            assert!(matches!(
                owner.activate(
                    &form,
                    &button(&form, "Apply structure change"),
                    &fixture.model(),
                    &fixture.ui(),
                    false
                ),
                Err(OwnedUiHierarchyError::ChangedProjection)
            ));
        }
        paint(&mut owner, &fixture, 800.0);
        assert!(
            owner.form.is_none(),
            "the previous placement is never reinterpreted for the new selection"
        );
        assert!(owner.error.is_some());
        assert_eq!(fixture.history(), 0);
    }
}

#[test]
fn nominal_equal_catalog_schema_and_permissions_are_checked_at_both_action_boundaries() {
    let fixture = Fixture::new();
    let bundle = AuthoringSchemaBundle::from_trusted_provider_definitions(
        nara::project_host::advanced::built_in_schema_providers(),
    )
    .unwrap();
    let first = AuthoringSchemaBasis::admit(&bundle, &[]).unwrap();
    let second = AuthoringSchemaBasis::admit(&bundle, &[]).unwrap();
    assert_ne!(
        first, second,
        "equal metadata catalogs are distinct Schema admissions"
    );
    let model = with_schema(&fixture.model(), first);
    let changed = with_schema(&model, second);
    let ui = fixture.ui();
    for consume in [false, true] {
        for read_only in [false, true] {
            let mut owner = controller();
            let frame = owner
                .compile(&model, &ui, environment(800.0), UiTheme::default(), false)
                .unwrap();
            let mut routing = RoutedInput::new(&frame);
            click(&mut owner, &mut routing, &frame, "Add UI node", &model, &ui);
            let form = owner
                .compile(&model, &ui, environment(800.0), UiTheme::default(), false)
                .unwrap();
            let current = if read_only { &model } else { &changed };
            if consume {
                let prepared = click(
                    &mut owner,
                    &mut routing,
                    &form,
                    "Apply structure change",
                    &model,
                    &ui,
                )
                .unwrap();
                assert!(matches!(
                    prepared.into_action(current, &ui, read_only),
                    Err(OwnedUiHierarchyError::ChangedProjection)
                ));
            } else {
                assert!(matches!(
                    owner.activate(
                        &form,
                        &button(&form, "Apply structure change"),
                        current,
                        &ui,
                        read_only
                    ),
                    Err(OwnedUiHierarchyError::ChangedProjection)
                ));
            }
        }
    }
    assert_eq!(fixture.history(), 0);
}

fn reimport_catalog_image(fixture: &Fixture) {
    // Browser indexing and explicit image import belong to the ordinary Authoring lane.
    let mut application = EditorApplication::new(
        EditorLaunchOptions::new(&fixture.project.0).with_profile("desktop"),
    );
    application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !application.current_frame().is_some_and(|model| {
        model
            .project_browser()
            .entry("assets/textures/player.png")
            .is_some()
            && !model.project_browser().has_pending_work()
    }) {
        assert!(
            Instant::now() < deadline,
            "the real Browser must index the catalog source"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(application.apply_project_browser_local_action(
        EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/player.png"))
        }
    ));
    while !application
        .current_frame()
        .unwrap()
        .image_import()
        .is_some_and(|detail| detail.can_submit())
    {
        assert!(
            Instant::now() < deadline,
            "the real image import detail must become ready"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let model = application.current_frame().unwrap();
    let settings = EditorImageImportSettings::new(EditorImageColorSpace::Linear);
    let action = model.image_import().unwrap().submit_action(settings);
    application.submit_and_drive(
        EditorActionBatch::new(model.revision(), [EditorAction::ImageImport(action)]),
        Duration::ZERO,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let model = application.current_frame().unwrap();
        let detail = model.image_import().unwrap();
        if matches!(
            detail.attempt(),
            EditorImageImportAttemptView::Succeeded { .. }
        ) && !model.image_import_work_pending()
        {
            assert!(
                matches!(detail.metadata(), EditorImageImportMetadataView::Ready {
                committed_settings, ..
            } if committed_settings == settings)
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the actual image import must succeed: {detail:?}"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut exited = application.request_exit().exit_ready();
    while !exited {
        assert!(
            Instant::now() < deadline,
            "the image import Session must retire"
        );
        exited = application.advance(Duration::ZERO).exit_ready();
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn real_image_catalog_generation_change_at_same_revision_refuses_activation_and_queued_form() {
    for consume in [false, true] {
        let mut fixture = Fixture::new();
        let mut owner = controller();
        let frame = paint(&mut owner, &fixture, 800.0);
        let mut routing = RoutedInput::new(&frame);
        // Make Save admissible before capturing the tested form's exact source revision.
        // The trusted lane then refreshes its actual catalog from the ordinary import's files.
        click_current(&mut owner, &mut routing, &fixture, "Add UI node");
        let seed =
            click_current(&mut owner, &mut routing, &fixture, "Apply structure change").unwrap();
        apply(&mut owner, &mut fixture, seed, 1);
        let history = fixture.history();
        assert_eq!(history, 1);
        click_current(&mut owner, &mut routing, &fixture, "Add UI node");
        click_current(&mut owner, &mut routing, &fixture, "Kind: Image");
        click_current(&mut owner, &mut routing, &fixture, "Choose image");
        click_current(
            &mut owner,
            &mut routing,
            &fixture,
            "Image: textures/player.png",
        );
        let original = fixture.ui();
        let form = paint(&mut owner, &fixture, 800.0);
        let prepared = consume.then(|| {
            click(
                &mut owner,
                &mut routing,
                &form,
                "Apply structure change",
                &fixture.model(),
                &original,
            )
            .unwrap()
        });
        reimport_catalog_image(&fixture);
        let saved = fixture.send(EditorAction::Persistence(EditorPersistenceAction::Save {
            document: original.document(),
            base_revision: original.revision(),
        }));
        assert!(matches!(
            saved,
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted { .. })
        ));
        fixture.settle();
        assert_eq!(fixture.ui().revision(), original.revision());
        assert_ne!(
            fixture.ui().image_choice_generation(),
            original.image_choice_generation()
        );
        if let Some(prepared) = prepared {
            assert!(matches!(
                prepared.into_action(&fixture.model(), &fixture.ui(), false),
                Err(OwnedUiHierarchyError::ChangedProjection)
            ));
            owner.submission_failed("The original image generation changed".to_owned());
        } else {
            assert!(matches!(
                owner.activate(
                    &form,
                    &button(&form, "Apply structure change"),
                    &fixture.model(),
                    &fixture.ui(),
                    false
                ),
                Err(OwnedUiHierarchyError::ChangedProjection)
            ));
        }
        paint(&mut owner, &fixture, 800.0);
        assert!(owner.form.is_none());
        assert_eq!(fixture.history(), history);
    }
}

#[test]
fn source_reopen_retires_form_and_exact_pending_result_cannot_attach_to_successor_incarnation() {
    let mut fixture = Fixture::new();
    let mut owner = controller();
    let frame = paint(&mut owner, &fixture, 800.0);
    let mut routing = RoutedInput::new(&frame);
    click_current(&mut owner, &mut routing, &fixture, "Add UI node");
    let prepared =
        click_current(&mut owner, &mut routing, &fixture, "Apply structure change").unwrap();
    prepared
        .into_action(&fixture.model(), &fixture.ui(), false)
        .unwrap();
    owner.admitted(Ok(id(1)));
    let original = fixture.ui();
    fixture.send(EditorAction::Persistence(EditorPersistenceAction::Reopen {
        target: fixture.model().reopen_target(),
    }));
    fixture.settle();
    let successor = fixture.ui();
    assert_eq!(successor.document(), original.document());
    assert_ne!(
        successor.revision().source_id(),
        original.revision().source_id()
    );
    paint(&mut owner, &fixture, 800.0);
    assert!(owner.form.is_none());
    let error = owner.error.clone();
    owner.action_finished(
        id(1),
        &PanelActionDispatch::NotDispatched(PanelActionNotDispatched::ProjectChanged),
    );
    assert!(owner.pending.is_none());
    assert_eq!(
        owner.error, error,
        "a predecessor result never becomes a successor form error"
    );
    click_current(&mut owner, &mut routing, &fixture, "Add UI node");
    let prepared =
        click_current(&mut owner, &mut routing, &fixture, "Apply structure change").unwrap();
    prepared
        .into_action(&fixture.model(), &fixture.ui(), false)
        .unwrap();
    owner.admitted(Ok(id(2)));
    owner.action_finished(
        id(1),
        &PanelActionDispatch::NotDispatched(PanelActionNotDispatched::Closing),
    );
    assert_eq!(owner.pending.as_ref().unwrap().0, id(2));
}
