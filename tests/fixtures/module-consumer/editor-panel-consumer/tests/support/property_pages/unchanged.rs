use super::*;

#[test]
fn a_registered_panel_confirms_an_unchanged_scene_property_preview() {
    confirm_unchanged_note(
        EditorDocumentKind::Scene,
        "scenes/unchanged-note.scene.json",
    );
}

#[test]
fn a_registered_panel_confirms_an_unchanged_prefab_property_preview() {
    confirm_unchanged_note(
        EditorDocumentKind::Prefab,
        "prefabs/unchanged-note.prefab.json",
    );
}

fn confirm_unchanged_note(kind: EditorDocumentKind, source: &str) {
    let project = text_project::TextProject::new();
    let (mut app, document, original_source, _) = open_nonactive_source(
        &project,
        kind,
        source,
        keyed_rows_schema(),
        [keyed_rows_entity(4)],
    );
    let source_path = project.root.join(source);
    let saved_revision = basis(&app, document).revision;
    let (mut host, instance) = register_panel(&mut app);
    let replay_history = |app: &mut EditorApplication, mutation| {
        let result = submit(app, EditorAction::Mutate(mutation));
        assert!(
            matches!(
                result.actions(),
                [EditorApplicationActionDispatch::Dispatched(EditorActionOutcome::Workspace(report))]
                    if report.applied && report.document == Some(document)
                        && report.revision == Some(basis(app, document).revision)
            ),
            "{result:?}"
        );
    };

    let token = review_keyed_edit(
        &mut app,
        &mut host,
        instance,
        document,
        |context, captured| prepare_note_edit(context, captured, UPDATED_NOTE),
    );
    commit_reviewed_keyed_edit(&mut app, &mut host, &token, document);
    assert_keyed_state(&app, document, OLD_ROW_KEY, ORIGINAL_SCORE, UPDATED_NOTE);
    let edited_revision = basis(&app, document).revision;
    replay_history(
        &mut app,
        EditorDocumentMutation::Undo {
            document,
            base_revision: edited_revision,
        },
    );

    let undone = frame(&app);
    let checkpoint = undone.workspace().document(document).unwrap();
    assert_ne!(checkpoint.revision, edited_revision);
    assert_ne!(checkpoint.revision, saved_revision);
    assert_eq!(checkpoint.saved_revision, saved_revision);
    assert!(!checkpoint.dirty);
    assert_eq!(
        (checkpoint.history.undo_depth, checkpoint.history.redo_depth),
        (0, 1)
    );
    let assert_checkpoint = |app: &EditorApplication| {
        assert_eq!(
            frame(app).workspace().documents,
            undone.workspace().documents
        );
        assert_navigation(app, &undone, document);
        assert_keyed_state(app, document, OLD_ROW_KEY, ORIGINAL_SCORE, ORIGINAL_NOTE);
        assert_eq!(fs::read_to_string(&source_path).unwrap(), original_source);
    };
    assert_checkpoint(&app);

    let gesture_basis = basis(&app, document);
    let changed = review_keyed_edit(
        &mut app,
        &mut host,
        instance,
        document,
        |context, captured| {
            assert_eq!(captured, &gesture_basis);
            prepare_note_edit(context, captured, "preview only")
        },
    );
    assert_checkpoint(&app);

    let empty = ScenePatchDocument::new([]);
    let token = review_keyed_edit(
        &mut app,
        &mut host,
        instance,
        document,
        |context, captured| {
            assert!(context.matches_instance(&changed));
            assert_eq!(captured, &gesture_basis);
            let request = note_read(captured);
            let original = ComponentValue::String(ORIGINAL_NOTE.into());
            let prepared = context
                .prepare_authoring_property_edit(
                    captured,
                    &[property_edit(&request, original.clone())],
                )
                .unwrap();
            assert_eq!(prepared.patch(), &empty);
            assert_eq!(prepared.inverse(), &empty);
            assert_selected_scalar(
                &prepared.preview().read_property(request.clone()).unwrap(),
                &request,
                original.clone(),
            );
            assert_preview_leaf_page(&prepared, &request, original);
            prepared
        },
    );

    let confirmation = host
        .prepare(&app, |panel, context| {
            assert!(context.matches_instance(&token));
            assert!(panel.completion.is_none());
            context
                .commit_authoring_edit(panel.retained.take().unwrap())
                .unwrap();
        })
        .unwrap();
    assert_eq!(confirmation.submissions().len(), 1);
    let (origin, submitted) = confirmation.submissions().next().unwrap();
    assert_eq!(origin, instance);
    assert_checkpoint(&app);
    host.submit_and_drive(&mut app, confirmation, Duration::ZERO);
    host.inspect(&app, &token, |panel, _| {
        let report = panel.completed(submitted);
        assert!(!report.applied);
        assert_eq!(report.document, Some(document));
        assert_eq!(report.active_document, undone.workspace().active_document);
        assert_eq!(report.revision, Some(checkpoint.revision));
        assert_eq!(report.dirty, Some(checkpoint.dirty));
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let patch_report = report.patch_report.as_ref().unwrap();
        assert!(!patch_report.applied);
        assert_eq!(patch_report.inverse.as_ref(), Some(&empty));
        assert!(
            !patch_report.diagnostics.has_errors(),
            "{:?}",
            patch_report.diagnostics
        );
    })
    .unwrap();
    assert_checkpoint(&app);

    replay_history(
        &mut app,
        EditorDocumentMutation::Redo {
            document,
            base_revision: checkpoint.revision,
        },
    );
    assert_keyed_state(&app, document, OLD_ROW_KEY, ORIGINAL_SCORE, UPDATED_NOTE);
    let redone = frame(&app);
    let tab = redone.workspace().document(document).unwrap();
    assert_ne!(tab.revision, checkpoint.revision);
    assert_eq!(tab.saved_revision, saved_revision);
    assert!(tab.dirty);
    assert_eq!((tab.history.undo_depth, tab.history.redo_depth), (1, 0));
    assert_navigation(&app, &undone, document);
    assert_eq!(fs::read_to_string(source_path).unwrap(), original_source);
    assert!(app.close(), "the property preview project must retire");
}
