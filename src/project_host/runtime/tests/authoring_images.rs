use super::*;

#[test]
fn generic_editor_hydrates_only_referenced_images_and_retains_a_complete_canvas_on_failure() {
    let project = TestProject::new("generic-image-hydration");
    project.configure_authoring_ui("Generic Image Hydration");
    let source_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
    )
    .unwrap();
    let linear = ImageImportSettings::new(ImageColorSpace::Linear);
    project.write_canonical_image(
        "textures/a-valid.png",
        "10280c94-d8ce-450b-b35d-5acdfb697502",
        linear,
        &source_bytes,
    );
    project.write_canonical_image(
        "textures/m-unreferenced.png",
        "b44c013f-8ed6-470d-a389-c1ef498e5561",
        ImageImportSettings::default(),
        &source_bytes,
    );
    project.write_canonical_image(
        "textures/z-invalid.png",
        "c0d02b07-05f3-4776-972c-c92e8e57668f",
        ImageImportSettings::default(),
        b"not a PNG",
    );
    project
        .write_ui_authoring_documents(Some("textures/a-valid.png"), Some("textures/z-invalid.png"));

    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (2, vec![("textures/a-valid.png".to_owned(), linear)])
    );
    assert!(
        editor
            .editor_image_last_good(&AssetPath::new("textures/a-valid.png").unwrap())
            .is_some()
    );
    assert!(
        editor
            .editor_image_last_good(&AssetPath::new("textures/m-unreferenced.png").unwrap())
            .is_none(),
        "an unreferenced canonical image must remain lazy"
    );
    assert!(
        editor
            .editor_image_last_good(&AssetPath::new("textures/z-invalid.png").unwrap())
            .is_none()
    );
    assert!(report_has_code(
        editor.current_frame().diagnostics(),
        "project.editor.authoring-image-runtime-rejected"
    ));

    let index = editor.project_source_index_snapshot().unwrap();
    let prefab_document = opened_document(editor.open_indexed_authoring_document(
        index,
        "prefabs/ui.prefab.json",
        crate::project_host::authoring::AuthoringDocumentKind::Prefab,
    ));
    assert_eq!(
        assert_current_ui_image_canvas(
            &mut editor,
            prefab_document,
            "textures/a-valid.png",
            ImageColorSpace::Linear,
        ),
        2
    );

    let import_frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.request_editor_image_import(
            import_frame.revision(),
            "assets/textures/m-unreferenced.png",
            ImageImportSettings::default(),
        ),
        super::editor::EditorImageImportRequestResult::Started(_)
    ));
    assert!(matches!(
        drive_image_import_to_terminal(&mut editor),
        super::editor::EditorImageImportState::Succeeded { .. }
    ));
    drive_image_import_host_until_idle(&mut editor);
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (
            3,
            vec![
                ("textures/a-valid.png".to_owned(), linear),
                (
                    "textures/m-unreferenced.png".to_owned(),
                    ImageImportSettings::default(),
                ),
            ],
        )
    );
}

#[test]
fn generic_editor_retires_an_image_when_its_metadata_becomes_invalid() {
    let project = TestProject::new("generic-image-retirement");
    project.configure_authoring_ui("Generic Image Retirement");
    let source_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
    )
    .unwrap();
    let image_path = "textures/retired.png";
    let settings = ImageImportSettings::new(ImageColorSpace::Linear);
    project.write_canonical_image(
        image_path,
        "0d39d538-c070-42fb-a829-fca0630d1a0d",
        settings,
        &source_bytes,
    );
    project.write_ui_authoring_documents(Some(image_path), Some(image_path));

    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (2, vec![(image_path.to_owned(), settings)])
    );
    editor.set_editor_preview_extent(nara_render::Extent2d::new(320, 240));
    editor.drive_editor_frame(Duration::ZERO);
    let before = editor.take_editor_preview_candidate().unwrap();
    let before_stamp = before.content_stamp();
    let (_, _, _, _, before_batches, _) = before.into_parts();
    assert!(
        before_batches
            .as_slice()
            .iter()
            .any(|batch| batch.image.is_some())
    );

    fs::write(
        project.root.join("assets/textures/retired.png.meta"),
        b"not valid metadata",
    )
    .unwrap();
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);

    editor.drive_editor_frame(Duration::ZERO);
    let after = editor.take_editor_preview_candidate().unwrap();
    let after_stamp = after.content_stamp();
    let (_, _, _, _, after_batches, _) = after.into_parts();
    let close = drive_editor_close_to_completion(&mut editor);
    assert!(matches!(
        close,
        super::super::editor::EditorProjectCloseProgress::Complete(_)
    ));

    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (3, Vec::new())
    );
    assert!(
        editor
            .editor_image_last_good(&AssetPath::new(image_path).unwrap())
            .is_none(),
        "invalid metadata must retire the image from the authoring publication"
    );
    assert_ne!(
        before_stamp, after_stamp,
        "identity retirement must invalidate Scene content reuse"
    );
    assert!(
        after_batches
            .as_slice()
            .iter()
            .all(|batch| batch.image.is_none()),
        "invalid metadata must revoke Image pixels from new Scene preview packets"
    );
}

#[test]
fn generic_editor_failed_image_revocation_settles_catalog_and_blocks_metadata_writes() {
    let project = TestProject::new("generic-image-revocation-failure");
    project.configure_authoring_ui("Image Revocation Failure");
    let revoked_path = "textures/revoked.png";
    let valid_path = "textures/valid.png";
    let pixels = include_bytes!("../../../../tests/fixtures/images/valid-rgba-1x1.png");
    for (path, stable_id) in [
        (revoked_path, "0d39d538-c070-42fb-a829-fca0630d1a0d"),
        (valid_path, "10280c94-d8ce-450b-b35d-5acdfb697502"),
    ] {
        project.write_canonical_image(path, stable_id, ImageImportSettings::default(), pixels);
    }
    project.write_ui_authoring_documents(Some(revoked_path), Some(valid_path));
    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    editor.set_editor_preview_extent(nara_render::Extent2d::new(320, 240));
    editor.drive_editor_frame(Duration::ZERO);
    editor.test_exhaust_image_publication_revision();
    fs::write(
        project.root.join(format!("assets/{revoked_path}.meta")),
        b"invalid metadata",
    )
    .unwrap();
    editor.request_project_source_index_refresh();
    drive_image_import_host_until_idle(&mut editor);
    editor.drive_editor_frame(Duration::ZERO);
    let (_, choices) = editor.editor_authoring_image_catalog_facts();
    let removed = editor
        .editor_image_last_good(&AssetPath::new(revoked_path).unwrap())
        .is_none();
    let preview_retired = editor.take_editor_preview_candidate().is_none();
    let warning = report_has_code(
        editor.current_frame().diagnostics(),
        "project.editor.authoring-image-revocation-failed",
    );
    let metadata_path = project.root.join(format!("assets/{valid_path}.meta"));
    let before = fs::read(&metadata_path).unwrap();
    let request = editor.request_editor_image_import(
        editor.current_frame().revision(),
        &format!("assets/{valid_path}"),
        ImageImportSettings::new(ImageColorSpace::Linear),
    );
    let after = fs::read(&metadata_path).unwrap();
    let close = drive_editor_close_to_completion(&mut editor);
    assert!(matches!(
        close,
        super::super::editor::EditorProjectCloseProgress::Complete(_)
    ));
    assert!(removed);
    assert_eq!(
        choices,
        vec![(valid_path.to_owned(), ImageImportSettings::default())]
    );
    assert!(preview_retired);
    assert!(warning);
    assert!(
        matches!(
            request,
            super::super::editor::EditorImageImportRequestResult::Rejected(
                super::super::editor::EditorImageImportFailure::Runtime(_)
            )
        ),
        "{request:?}"
    );
    assert_eq!(before, after);
}

#[test]
fn generic_editor_explicit_image_import_is_admitted_during_background_hydration() {
    for same_source in [false, true] {
        let project = TestProject::new("generic-image-hydration-user-priority");
        project.configure_authoring_ui("Image Hydration User Priority");
        let pixels = include_bytes!("../../../../tests/fixtures/images/valid-rgba-1x1.png");
        for (path, stable_id) in [
            (
                "textures/a-running.png",
                "10280c94-d8ce-450b-b35d-5acdfb697502",
            ),
            (
                "textures/m-explicit.png",
                "b44c013f-8ed6-470d-a389-c1ef498e5561",
            ),
            (
                "textures/z-queued.png",
                "c0d02b07-05f3-4776-972c-c92e8e57668f",
            ),
        ] {
            project.write_canonical_image(path, stable_id, ImageImportSettings::default(), pixels);
        }
        project.write_ui_authoring_documents(
            Some("textures/a-running.png"),
            Some("textures/z-queued.png"),
        );
        let mut host = super::super::editor::EditorProjectOpenHost::new();
        let mut attempt = host.begin(project.writable_capability(), None);
        let mut editor = loop {
            match host.advance(attempt) {
                super::super::editor::EditorProjectOpenProgress::Pending(next) => attempt = next,
                super::super::editor::EditorProjectOpenProgress::Opened(session) => break session,
                super::super::editor::EditorProjectOpenProgress::Rejected(failure) => {
                    panic!("the Image fixture must open: {:?}", failure.diagnostics())
                }
            }
        };
        let held = editor.test_hold_next_image_import();
        for _ in 0..128 {
            editor.drive_editor_frame(Duration::ZERO);
        }
        let hydration_started = held.wait_until_started();
        let target = if same_source {
            "textures/a-running.png"
        } else {
            "textures/z-queued.png"
        };
        let result = editor.request_editor_image_import(
            editor.current_frame().revision(),
            &format!("assets/{target}"),
            ImageImportSettings::new(ImageColorSpace::Linear),
        );
        let _ = held.release();
        let terminal = if matches!(
            result,
            super::super::editor::EditorImageImportRequestResult::Started(_)
        ) {
            Some(drive_image_import_to_terminal(&mut editor))
        } else {
            None
        };
        drive_image_import_host_until_idle(&mut editor);
        let accepted = editor
            .editor_image_last_good(&AssetPath::new(target).unwrap())
            .cloned();
        let queued_restored = editor
            .editor_image_last_good(&AssetPath::new("textures/z-queued.png").unwrap())
            .is_some();
        let close = drive_editor_close_to_completion(&mut editor);
        assert!(matches!(
            close,
            super::super::editor::EditorProjectCloseProgress::Complete(_)
        ));
        assert!(
            hydration_started,
            "the regression must hold active hydration"
        );
        assert!(
            matches!(
                result,
                super::super::editor::EditorImageImportRequestResult::Started(_)
            ),
            "explicit import must be admitted while hydration is active: {result:?}"
        );
        assert!(
            matches!(
                terminal,
                Some(super::super::editor::EditorImageImportState::Succeeded { .. })
            ),
            "{terminal:?}"
        );
        assert_eq!(
            accepted.unwrap().settings_hash(),
            ImageImportSettings::new(ImageColorSpace::Linear).settings_hash()
        );
        assert!(queued_restored, "unrelated queued hydration must resume");
    }
}

#[test]
fn generic_editor_catalog_revocation_cancels_a_late_image_import_even_after_metadata_is_restored() {
    let project = TestProject::new("generic-image-revoked-late-import");
    project.configure_authoring_ui("Revoked Late Image Import");
    let path = "textures/revoked.png";
    project.write_canonical_image(
        path,
        "0d39d538-c070-42fb-a829-fca0630d1a0d",
        ImageImportSettings::default(),
        include_bytes!("../../../../tests/fixtures/images/valid-rgba-1x1.png"),
    );
    project.write_ui_authoring_documents(Some(path), Some(path));
    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    let held = editor.test_hold_next_image_import();
    let request = editor.request_editor_image_import(
        editor.current_frame().revision(),
        &format!("assets/{path}"),
        ImageImportSettings::new(ImageColorSpace::Linear),
    );
    assert!(matches!(
        request,
        super::super::editor::EditorImageImportRequestResult::Started(_)
    ));
    assert!(held.wait_until_started());
    let metadata_path = project.root.join(format!("assets/{path}.meta"));
    let admitted_metadata = fs::read(&metadata_path).unwrap();
    fs::write(&metadata_path, b"invalid metadata").unwrap();
    editor.request_project_source_index_refresh();
    for _ in 0..256 {
        editor.drive_editor_frame(Duration::ZERO);
        if editor
            .editor_image_last_good(&AssetPath::new(path).unwrap())
            .is_none()
        {
            break;
        }
    }
    let revoked_before_release = editor
        .editor_image_last_good(&AssetPath::new(path).unwrap())
        .is_none();
    fs::write(&metadata_path, admitted_metadata).unwrap();
    let _ = held.release();
    drive_image_import_host_until_idle(&mut editor);
    let still_revoked = editor
        .editor_image_last_good(&AssetPath::new(path).unwrap())
        .is_none();
    let terminal = editor.editor_image_import_state();
    let close = drive_editor_close_to_completion(&mut editor);
    assert!(matches!(
        close,
        super::super::editor::EditorProjectCloseProgress::Complete(_)
    ));
    assert!(
        revoked_before_release,
        "the refreshed catalog must observe identity revocation"
    );
    assert!(
        still_revoked,
        "a late result must not regain authority after metadata restoration"
    );
    assert!(
        matches!(
            terminal,
            super::super::editor::EditorImageImportState::Failed { .. }
        ),
        "{terminal:?}"
    );
}

#[test]
fn authoring_image_warning_omits_an_unbounded_project_relative_locator() {
    let unbounded = format!("textures/{}.png", "a".repeat(1_100));
    let report = super::super::single_warning_with_project_relative(
        "project.editor.authoring-image-source-unavailable",
        "a referenced authoring image could not be restored",
        "asset",
        &unbounded,
    );
    let diagnostic = report
        .iter()
        .next()
        .expect("the warning remains publishable without the rejected locator");
    assert_eq!(
        diagnostic.code().as_str(),
        "project.editor.authoring-image-source-unavailable"
    );
    assert!(diagnostic.fields().is_empty());
}

#[test]
fn generic_editor_saves_an_imported_asset_ref_and_hydrates_the_same_recipe_on_reopen() {
    use nara_ui::__private::{UiAuthoringFieldKind, UiAuthoringFieldValue, UiBrushAuthoringKind};

    let project = TestProject::new("generic-image-save-reopen");
    project.configure_authoring_ui("Generic Image Save And Reopen");
    project.write_ui_authoring_documents(None, None);
    fs::create_dir_all(project.root.join("assets/textures")).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
        project.root.join("assets/textures/imported.png"),
    )
    .unwrap();

    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (1, Vec::new())
    );
    let index = editor.project_source_index_snapshot().unwrap();
    let prefab_document = opened_document(editor.open_indexed_authoring_document(
        index,
        "prefabs/ui.prefab.json",
        crate::project_host::authoring::AuthoringDocumentKind::Prefab,
    ));
    editor.drive_editor_frame(Duration::ZERO);
    let stale_ui_frame = editor.current_ui_authoring_frame().unwrap();
    let image_entity = nara_identity::SceneEntityId::new("image-node").unwrap();
    let stale_brush_key = stale_ui_frame
        .projection()
        .unwrap()
        .fields()
        .iter()
        .find(|field| {
            field.key().entity() == &image_entity
                && field.key().primary_field().as_str() == "background.kind"
                && matches!(field.kind(), UiAuthoringFieldKind::Brush)
        })
        .unwrap()
        .key()
        .clone();
    assert!(stale_ui_frame.image_choices().unwrap().choices().is_empty());

    let linear = ImageImportSettings::new(ImageColorSpace::Linear);
    let import_frame = editor.current_frame();
    assert!(matches!(
        editor.request_editor_image_import(
            import_frame.revision(),
            "assets/textures/imported.png",
            linear,
        ),
        super::editor::EditorImageImportRequestResult::Started(_)
    ));
    assert!(matches!(
        drive_image_import_to_terminal(&mut editor),
        super::editor::EditorImageImportState::Succeeded { .. }
    ));
    drive_image_import_host_until_idle(&mut editor);
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (2, vec![("textures/imported.png".to_owned(), linear)])
    );

    let image_value = UiAuthoringFieldValue::Brush {
        kind: UiBrushAuthoringKind::Image,
        red: 1.0,
        green: 1.0,
        blue: 1.0,
        alpha: 1.0,
        reference: "textures/imported.png".to_owned(),
    };
    assert!(
        stale_ui_frame
            .lower_discrete_field_edit(&stale_brush_key, image_value.clone())
            .is_err(),
        "an older catalog receipt must not admit its successor image"
    );

    let edited_frame = editor.drive_editor_frame(Duration::ZERO);
    let current_ui_frame = editor.current_ui_authoring_frame().unwrap();
    assert_eq!(current_ui_frame.image_choice_generation().get(), 2);
    let current_brush_key = current_ui_frame
        .projection()
        .unwrap()
        .fields()
        .iter()
        .find(|field| {
            field.key().entity() == &image_entity
                && field.key().primary_field().as_str() == "background.kind"
                && matches!(field.kind(), UiAuthoringFieldKind::Brush)
        })
        .unwrap()
        .key()
        .clone();
    let mutation = current_ui_frame
        .lower_discrete_field_edit(&current_brush_key, image_value)
        .unwrap();
    let edited = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            edited_frame.revision(),
            [EditorAction::Mutate(mutation)],
        ))
        .unwrap();
    assert!(matches!(
        &edited.outcomes()[0],
        EditorActionOutcome::Workspace(report) if report.applied
    ));

    let save_frame = editor.drive_editor_frame(Duration::ZERO);
    let saved_revision = save_frame
        .workspace()
        .document(prefab_document)
        .unwrap()
        .revision;
    let save = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            save_frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Save {
                document: prefab_document,
                base_revision: saved_revision,
            })],
        ))
        .unwrap();
    assert!(matches!(
        save.outcomes()[0],
        EditorActionOutcome::Persistence(nara_tooling::EditorPersistenceRequestResult::Accepted)
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Saved { document, .. }) if document == prefab_document
    ));
    assert_eq!(
        ui_visual_background_image_path(
            editor
                .workspace()
                .prefab(prefab_document)
                .unwrap()
                .session()
                .document(),
            "image-node",
        ),
        Some("textures/imported.png")
    );
    drop(editor);

    let mut reopened = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut reopened);
    assert_eq!(
        reopened.editor_authoring_image_catalog_facts(),
        (2, vec![("textures/imported.png".to_owned(), linear)])
    );
    let index = reopened.project_source_index_snapshot().unwrap();
    let reopened_document = opened_document(reopened.open_indexed_authoring_document(
        index,
        "prefabs/ui.prefab.json",
        crate::project_host::authoring::AuthoringDocumentKind::Prefab,
    ));
    assert_eq!(
        ui_visual_background_image_path(
            reopened
                .workspace()
                .prefab(reopened_document)
                .unwrap()
                .session()
                .document(),
            "image-node",
        ),
        Some("textures/imported.png")
    );
    assert_eq!(
        assert_current_ui_image_canvas(
            &mut reopened,
            reopened_document,
            "textures/imported.png",
            ImageColorSpace::Linear,
        ),
        2
    );
}

fn assert_current_ui_image_canvas(
    editor: &mut super::editor::EditorProjectSession,
    document: nara_tooling::EditorDocumentId,
    expected_path: &str,
    expected_color_space: ImageColorSpace,
) -> u64 {
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let ui_frame = editor.current_ui_authoring_frame().unwrap();
    assert_eq!(ui_frame.editor_frame(), frame.revision());
    assert_eq!(ui_frame.document(), document);
    let generation = ui_frame.image_choice_generation().get();
    let choices = ui_frame.image_choices().unwrap().choices();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].project_path(), expected_path);
    let nara_ui::__private::UiAuthoringCanvasProjection::Ready(_) =
        ui_frame.projection().unwrap().canvas()
    else {
        panic!("the current image catalog did not produce a complete UI Canvas")
    };
    let candidate = editor
        .take_editor_ui_preview_candidate()
        .expect("a ready UI Canvas publishes one concrete presentation candidate");
    assert_eq!(candidate.document(), document);
    let (_, _, _, _, _, paint_packet, _) = candidate.into_parts();
    assert_eq!(paint_packet.images().len(), 1);
    assert_eq!(paint_packet.images()[0].id().as_str(), expected_path);
    assert_eq!(
        paint_packet.images()[0].snapshot().resource().color_space(),
        expected_color_space
    );
    generation
}
