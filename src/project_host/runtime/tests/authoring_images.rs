use super::*;

fn write_editor_tileset(project: &TestProject, image: AssetRef) {
    let source = nara_tilemap::TileSetSource::new(
        image,
        nara_tilemap::TileAtlasLayout::grid(nara_core::Vec2::ONE, 1, 1),
    );
    fs::create_dir_all(project.root.join("assets/tilesets")).unwrap();
    fs::write(
        project.root.join("assets/tilesets/terrain.tileset.ron"),
        source.encode_ron_bytes().unwrap(),
    )
    .unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str("d3d3f9a6-f7a9-4db0-ae3c-e1d4264d6f12").unwrap(),
            AssetPath::new("tilesets/terrain.tileset.ron").unwrap(),
            AssetSourceKind::Other(String::from("tileset")),
        ),
        (),
    );
    fs::write(
        project
            .root
            .join("assets/tilesets/terrain.tileset.ron.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
}

fn editor_tileset_project(name: &str) -> TestProject {
    let project = TestProject::new(name);
    project.configure_authoring_ui(name);
    let entity = SceneEntityRecord::new(nara_identity::SceneEntityId::new("terrain").unwrap())
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            identity_transform_record(),
        )
        .with_component(
            ComponentTypeId::new("nara.tilemap.Tilemap"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    (
                        "tile_size",
                        ComponentValue::map([
                            ("x", ComponentValue::f64(1.0).unwrap()),
                            ("y", ComponentValue::f64(1.0).unwrap()),
                        ]),
                    ),
                    (
                        "tileset",
                        ComponentValue::map([
                            ("kind", ComponentValue::String(String::from("path"))),
                            (
                                "value",
                                ComponentValue::String(String::from(
                                    "tilesets/terrain.tileset.ron",
                                )),
                            ),
                        ]),
                    ),
                    ("cells", ComponentValue::List(Vec::new())),
                ]),
            ),
        );
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([entity])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    project
}

#[test]
fn editor_runtime_edit_keeps_accepted_request_pending_after_post_operation_fault() {
    use nara_tooling::{EditorPlayCommand, EditorPlayRequestResult, EditorPlayState};
    use nara_tooling::{EditorRuntimeEditRequest, EditorRuntimeEditResult};

    let project = TestProject::with_image("editor-runtime-edit-post-operation-fault");
    let mut editor = super::super::editor::EditorProjectSession::open_trusted_compiled(
        project.capability(),
        super::super::editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.play_view().state() != EditorPlayState::Running {
        assert!(Instant::now() < deadline, "the fixture Play did not start");
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let request = EditorRuntimeEditRequest {
        generation: frame.play().generation().unwrap(),
        document_revision: frame.play().current_revision().unwrap(),
        entity: nara_identity::SceneEntityId::new("player").unwrap(),
        component: ComponentTypeId::new("nara.sprite.Sprite"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("layer"),
        value: ComponentValue::I64(4),
    };
    let reporter = editor.test_fault_after_next_runtime_edit();
    assert!(reporter.fault().is_none());
    let submitted = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::RuntimeEdit {
                document,
                request: request.clone(),
            }],
        ))
        .unwrap();
    assert!(matches!(
        &submitted.outcomes()[0],
        EditorActionOutcome::RuntimeEdit(Ok(()))
    ));
    let fault = reporter
        .fault()
        .expect("the accepted port must inject its fault");
    assert_eq!(fault.kind(), RuntimeFaultKind::RequiredService);
    assert_eq!(
        fault.source(),
        "nara.test.editor-runtime-edit-post-operation"
    );
    assert_eq!(
        editor.runtime_edit_result(),
        Some(&EditorRuntimeEditResult::Pending(request.clone())),
        "the Host must register the real Submitted receipt before driving the fault"
    );
    assert!(!editor.acknowledge_runtime_edit_result());

    let faulted = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(faulted.play().state(), EditorPlayState::Faulted);
    assert_eq!(
        faulted.runtime_edit_result(),
        Some(&EditorRuntimeEditResult::Cancelled(request.clone())),
        "normal fault handling must retain the accepted request in its terminal result"
    );
    assert_eq!(
        editor
            .drive_editor_frame(Duration::ZERO)
            .runtime_edit_result(),
        Some(&EditorRuntimeEditResult::Cancelled(request))
    );
    assert!(editor.acknowledge_runtime_edit_result());
    assert!(editor.runtime_edit_result().is_none());

    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.play_view().state() != EditorPlayState::Empty {
        assert!(Instant::now() < deadline, "the faulted Play did not stop");
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        super::super::editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[test]
fn generic_editor_image_transaction_records_v2_last_good_after_post_operation_fault() {
    let project = editor_tileset_project("image-transaction-post-operation-fault");
    let path = AssetPath::new("textures/a.png").unwrap();
    project.write_canonical_image(
        path.as_str(),
        "c58dfd3b-6f62-41c9-9d48-357159936609",
        ImageImportSettings::default(),
        include_bytes!("../../../../tests/fixtures/images/valid-rgba-1x1.png"),
    );
    write_editor_tileset(&project, AssetRef::path(path.as_str()).unwrap());
    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    let previous = editor.editor_image_last_good(&path).unwrap().clone();
    assert_eq!(previous.version().raw(), 1);

    let linear = ImageImportSettings::new(ImageColorSpace::Linear);
    let reporter = editor.test_fault_after_next_image_publication();
    assert!(reporter.fault().is_none());
    assert!(matches!(
        editor.request_editor_image_import(
            editor.current_frame().revision(),
            "assets/textures/a.png",
            linear,
        ),
        super::super::editor::EditorImageImportRequestResult::Started(_)
    ));
    let terminal = drive_image_import_to_terminal(&mut editor);
    assert!(
        matches!(
            terminal,
            super::super::editor::EditorImageImportState::Succeeded { version, .. }
                if version.raw() == 2
        ),
        "{terminal:?}"
    );
    drive_image_import_host_until_idle(&mut editor);
    assert_image_v2_last_good_after_post_operation_fault(&editor, &path, &previous, &reporter);
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        super::super::editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[test]
fn generic_editor_image_hydration_records_v2_last_good_after_post_operation_fault() {
    let project = editor_tileset_project("image-hydration-post-operation-fault");
    let path = AssetPath::new("textures/a.png").unwrap();
    project.write_canonical_image(
        path.as_str(),
        "c58dfd3b-6f62-41c9-9d48-357159936609",
        ImageImportSettings::default(),
        include_bytes!("../../../../tests/fixtures/images/valid-rgba-1x1.png"),
    );
    write_editor_tileset(&project, AssetRef::path(path.as_str()).unwrap());
    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    let previous = editor.editor_image_last_good(&path).unwrap().clone();
    assert_eq!(previous.version().raw(), 1);

    // Preserve the source identity: the refreshed TileSet demand must hydrate a new recipe.
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(previous.stable_id(), path.clone(), AssetSourceKind::Image),
        ImageImportSettings::new(ImageColorSpace::Linear),
    );
    fs::write(
        project.root.join("assets/textures/a.png.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    let reporter = editor.test_fault_after_next_image_publication();
    assert!(reporter.fault().is_none());
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        matches!(
            editor.editor_image_import_state(),
            super::super::editor::EditorImageImportState::Idle
        ),
        "hydration must not be replaced by an explicit import transaction"
    );
    assert_image_v2_last_good_after_post_operation_fault(&editor, &path, &previous, &reporter);
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        super::super::editor::EditorProjectCloseProgress::Complete(_)
    ));
}

fn assert_image_v2_last_good_after_post_operation_fault(
    editor: &super::super::editor::EditorProjectSession,
    path: &AssetPath,
    previous: &super::super::editor::EditorImageLastGood,
    reporter: &RuntimeFaultReporter,
) {
    let fault = reporter
        .fault()
        .expect("the real Image publication must inject its fault");
    assert_eq!(fault.kind(), RuntimeFaultKind::RequiredService);
    assert_eq!(
        fault.source(),
        "nara.test.editor-image-publication-post-operation"
    );
    let current = editor
        .editor_image_last_good(path)
        .expect("a committed v2 must remain registered as Host last-good after the fault");
    assert_eq!(current.version().raw(), 2);
    assert_eq!(current.stable_id(), previous.stable_id());
    assert_eq!(current.handle(), previous.handle());
    assert_eq!(current.source_identity(), previous.source_identity());
    assert_eq!(current.source_digest(), previous.source_digest());
    let linear = ImageImportSettings::new(ImageColorSpace::Linear);
    assert_ne!(previous.settings_hash(), linear.settings_hash());
    assert_eq!(current.settings_hash(), linear.settings_hash());
    let published = editor
        .test_image_import_asset(current.handle(), current.version())
        .expect("last-good v2 must match the actual usable Image publication");
    assert_eq!(published.source().path(), path);
    assert_eq!(published.source().stable_id(), current.stable_id());
    assert_eq!(published.source().settings(), linear);
    assert_eq!(published.pixels(), &[24, 120, 220, 255]);
    assert!(
        editor
            .test_image_import_asset(previous.handle(), previous.version())
            .is_none(),
        "the actual store must no longer expose v1 as usable"
    );
}

#[test]
fn generic_editor_reconciles_tileset_latest_dependency_and_revocation_without_a_visible_scene() {
    let project = editor_tileset_project("generic-tileset-latest-dependency");
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
    )
    .unwrap();
    project.write_canonical_image(
        "textures/a.png",
        "c58dfd3b-6f62-41c9-9d48-357159936609",
        ImageImportSettings::default(),
        &bytes,
    );
    write_editor_tileset(&project, AssetRef::path("textures/a.png").unwrap());
    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    let path = AssetPath::new("tilesets/terrain.tileset.ron").unwrap();
    let published = editor
        .test_tile_set(&path)
        .expect("TileSet publishes without a preview extent");
    let old_image = published.image_content().unwrap().clone();
    assert!(
        editor
            .editor_image_last_good(&AssetPath::new("textures/a.png").unwrap())
            .is_some()
    );

    write_editor_tileset(&project, AssetRef::path("textures/b.png").unwrap());
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        editor
            .test_tile_set(&path)
            .unwrap()
            .image_content()
            .unwrap()
            .same_allocation(&old_image)
    );
    assert!(!project.root.join("assets/textures/b.png.meta").exists());

    fs::remove_file(project.root.join("assets/textures/a.png.meta")).unwrap();
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        editor.test_tile_set(&path).is_none(),
        "published A is revoked while desired B is absent"
    );

    project.write_canonical_image(
        "textures/a.png",
        "c58dfd3b-6f62-41c9-9d48-357159936609",
        ImageImportSettings::default(),
        &bytes,
    );
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        editor.test_tile_set(&path).is_none(),
        "recovering old A cannot resurrect its publication"
    );

    project.write_canonical_image(
        "textures/b.png",
        "18c8e1c0-aeb7-43dd-bc73-09f36507812e",
        ImageImportSettings::default(),
        b"invalid PNG",
    );
    fs::remove_file(project.root.join("assets/textures/b.png")).unwrap();
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        editor.test_tile_set(&path).is_none(),
        "a sidecar alone cannot authorize missing source pixels"
    );
    fs::write(project.root.join("assets/textures/b.png"), b"invalid PNG").unwrap();
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    assert!(editor.test_tile_set(&path).is_none());
    fs::write(project.root.join("assets/textures/b.png"), &bytes).unwrap();
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    let recovered = editor
        .test_tile_set(&path)
        .expect("latest B recovers after a transient decode failure");
    assert_ne!(
        recovered.image_content().unwrap().handle(),
        old_image.handle()
    );
    assert!(
        editor
            .editor_image_last_good(&AssetPath::new("textures/b.png").unwrap())
            .is_some()
    );
}

#[test]
fn generic_editor_rejects_a_malformed_tileset_before_a_late_image_can_publish() {
    let project = editor_tileset_project("generic-tileset-malformed-late-image");
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
    )
    .unwrap();
    for (path, identity) in [
        ("textures/a.png", "c58dfd3b-6f62-41c9-9d48-357159936609"),
        ("textures/b.png", "18c8e1c0-aeb7-43dd-bc73-09f36507812e"),
    ] {
        project.write_canonical_image(path, identity, ImageImportSettings::default(), &bytes);
    }
    write_editor_tileset(&project, AssetRef::path("textures/a.png").unwrap());
    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    let path = AssetPath::new("tilesets/terrain.tileset.ron").unwrap();
    let old_image = editor
        .test_tile_set(&path)
        .unwrap()
        .image_content()
        .unwrap()
        .clone();
    let held = editor.test_hold_next_image_import_through_shutdown();
    write_editor_tileset(&project, AssetRef::path("textures/b.png").unwrap());
    assert!(editor.request_project_source_index_refresh());
    let refresh_deadline = Instant::now() + Duration::from_secs(5);
    while editor.project_source_index_work_pending() {
        assert!(Instant::now() < refresh_deadline);
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    drive_until_image_import_detail_admits(&mut editor, "assets/textures/b.png");
    assert!(held.wait_until_started());
    fs::write(
        project.root.join("assets/tilesets/terrain.tileset.ron"),
        b"invalid TileSet",
    )
    .unwrap();
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    let _ = held.release();
    assert!(
        editor
            .test_tile_set(&path)
            .unwrap()
            .image_content()
            .unwrap()
            .same_allocation(&old_image)
    );
    let frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.request_editor_image_import(
            frame.revision(),
            "assets/textures/b.png",
            ImageImportSettings::default()
        ),
        super::editor::EditorImageImportRequestResult::Started(_)
    ));
    assert!(matches!(
        drive_image_import_to_terminal(&mut editor),
        super::editor::EditorImageImportState::Succeeded { .. }
    ));
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        editor
            .test_tile_set(&path)
            .unwrap()
            .image_content()
            .unwrap()
            .same_allocation(&old_image),
        "an independently imported B cannot fulfill a rejected desired source"
    );
    write_editor_tileset(&project, AssetRef::path("textures/b.png").unwrap());
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    assert_ne!(
        editor
            .test_tile_set(&path)
            .unwrap()
            .image_content()
            .unwrap()
            .handle(),
        old_image.handle()
    );
}

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
#[cfg(feature = "presentation-2d")]
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
    let deadline = Instant::now() + Duration::from_secs(10);
    let before = loop {
        editor.drive_editor_frame(Duration::ZERO);
        if let Some(candidate) = editor.take_editor_preview_candidate() {
            break candidate;
        }
        assert!(
            Instant::now() < deadline,
            "the initial Scene capture did not publish"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
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
    let after_stamp = editor.current_editor_preview_content_stamp();
    let after = editor.take_editor_preview_candidate();
    if let Some(after) = after {
        assert_eq!(after_stamp, Some(after.content_stamp()));
        let (_, _, _, _, after_batches, _) = after.into_parts();
        assert!(
            after_batches
                .as_slice()
                .iter()
                .all(|batch| batch.image.is_none()),
            "invalid metadata must revoke Image pixels from new Scene preview packets"
        );
    } else {
        assert!(
            after_stamp.is_none(),
            "an unavailable capture cannot authorize old pixels"
        );
    }
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
        Some(before_stamp),
        after_stamp,
        "identity retirement must invalidate Scene content reuse"
    );
}

#[test]
#[cfg(feature = "presentation-2d")]
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
        let open_deadline = Instant::now() + Duration::from_secs(5);
        let mut editor = loop {
            match host.advance(attempt) {
                super::super::editor::EditorProjectOpenProgress::Pending(next) => {
                    assert!(
                        Instant::now() < open_deadline,
                        "the Image fixture did not open"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                    attempt = next;
                }
                super::super::editor::EditorProjectOpenProgress::Opened(session) => break session,
                super::super::editor::EditorProjectOpenProgress::Rejected(failure) => {
                    panic!("the Image fixture must open: {:?}", failure.diagnostics())
                }
            }
        };
        let held = editor.test_hold_next_image_import();
        let index_deadline = Instant::now() + Duration::from_secs(5);
        while editor.project_source_index_work_pending() {
            assert!(Instant::now() < index_deadline);
            editor.drive_editor_frame(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        let target = if same_source {
            "textures/a-running.png"
        } else {
            "textures/z-queued.png"
        };
        drive_until_image_import_detail_admits(&mut editor, &format!("assets/{target}"));
        let hydration_started = held.wait_until_started();
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
    let revocation_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < revocation_deadline {
        editor.drive_editor_frame(Duration::ZERO);
        if editor
            .editor_image_last_good(&AssetPath::new(path).unwrap())
            .is_none()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
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
        EditorActionOutcome::Persistence(
            nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
        )
    ));
    drive_persistence_to_terminal(&mut editor);
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

fn drive_until_image_import_detail_admits(
    editor: &mut super::editor::EditorProjectSession,
    relative_path: &str,
) {
    editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
        relative_path: Some(Arc::from(relative_path)),
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        let current_generation = editor
            .project_source_index_snapshot()
            .map(|index| index.generation().get());
        if !editor.project_source_index_work_pending()
            && frame.image_import().is_some_and(|detail| {
                detail.relative_path() == relative_path
                    && detail.can_submit()
                    && Some(detail.detail_revision().get()) == current_generation
            })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the current image catalog did not admit {relative_path}: {:?}",
            frame.image_import()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
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
