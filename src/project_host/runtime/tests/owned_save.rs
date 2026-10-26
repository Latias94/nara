use super::*;

#[test]
fn pending_save_keeps_captured_structural_manifest_separate_from_later_moves() {
    for uncertain in [false, true] {
        assert_structural_save_snapshot(uncertain);
    }
}

fn assert_structural_save_snapshot(uncertain: bool) {
    let project = TestProject::new("editor-save-structural-manifest");
    let source_path = project.root.join("scenes/startup.scene.json");
    let id = |name: &str| nara_identity::SceneEntityId::new(name).unwrap();
    let original = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("first-parent")),
        SceneEntityRecord::new(id("second-parent")),
        SceneEntityRecord::new(id("unknown")).with_component(
            ComponentTypeId::new("game.Missing"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("preserve opaque source".into()),
            ),
        ),
    ]);
    fs::write(&source_path, original.to_json_string().unwrap()).unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    let move_to = |editor: &mut editor::EditorProjectSession, parent: &str| {
        let base_revision = editor.workspace().scene(document).unwrap().revision();
        let report = editor.apply_workspace_command(EditorWorkspaceCommand::ComposeScene {
            document,
            base_revision,
            command: nara_tooling::SceneCompositionCommand::MoveEntity {
                entity: id("unknown"),
                to: ScenePlacement::last(ScenePlacementOwner::Parent { parent: id(parent) }),
            },
        });
        assert!(report.applied, "{:?}", report.diagnostics);
    };
    move_to(&mut editor, "first-parent");
    let captured = editor.workspace().scene(document).unwrap();
    let captured_revision = captured.revision();
    let previous_saved_revision = captured.saved_revision();
    let captured_document = captured.session().document().clone();
    let captured_manifest = captured
        .session()
        .unavailable_record_manifest()
        .unwrap()
        .clone();
    if uncertain {
        editor.test_reject_next_valid_persistence_receipt(document);
    }
    let control = editor.hold_next_save(editor::SaveTestPhase::BeforePublication);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());

    move_to(&mut editor, "second-parent");
    let later = editor.workspace().scene(document).unwrap();
    let later_document = later.session().document().clone();
    let later_manifest = later
        .session()
        .unavailable_record_manifest()
        .unwrap()
        .clone();
    assert_ne!(captured_manifest, later_manifest);
    control.release();
    let completed = drive_persistence_to_terminal(&mut editor);
    let result = completed.persistence().result();
    if uncertain {
        assert!(matches!(
            result,
            Some(EditorPersistenceResult::PersistenceUncertain { .. })
        ));
    } else {
        assert!(matches!(result,
            Some(EditorPersistenceResult::Saved { revision, .. }) if revision == captured_revision));
    }
    let slot = editor.workspace().scene(document).unwrap();
    assert!(slot.is_dirty());
    assert_eq!(
        slot.saved_revision(),
        if uncertain {
            previous_saved_revision
        } else {
            captured_revision
        }
    );
    assert_eq!(slot.session().document(), &later_document);
    assert_eq!(
        slot.session().unavailable_record_manifest(),
        Some(&later_manifest)
    );
    assert_eq!(
        fs::read_to_string(&source_path).unwrap(),
        captured_document.to_json_string().unwrap()
    );

    // Exact admitted source bytes can restore the captured state despite later unsaved topology.
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult {
            attempt: editor
                .persistence_view()
                .attempt()
                .expect("the test observed a terminal persistence result")
        }),
        nara_tooling::EditorPersistenceRequestResult::Acknowledged { .. }
    ));
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document)
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    let reopened_result = drive_persistence_to_terminal(&mut editor)
        .persistence()
        .result();
    if !matches!(
        reopened_result,
        Some(EditorPersistenceResult::Opened { .. })
    ) {
        let _ = drive_editor_close_to_completion(&mut editor);
        drop(editor);
        panic!("structural Reopen failed after uncertain={uncertain}: {reopened_result:?}");
    }
    let reopened = editor.workspace().scene(document).unwrap();
    assert!(!reopened.is_dirty());
    assert_eq!(reopened.session().document(), &captured_document);
    assert_eq!(
        reopened.session().unavailable_record_manifest(),
        Some(&captured_manifest)
    );
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[cfg(feature = "tooling")]
#[test]
fn pending_save_keeps_edits_available_and_close_retains_the_unpublished_owner() {
    let project = TestProject::new("editor-save-before-publication");
    let source_path = project.root.join("scenes/startup.scene.json");
    fs::write(
        &source_path,
        editable_authoring_scene("original-value")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let original = fs::read(&source_path).unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    make_authoring_name_dirty(&mut editor, document, "captured-value");
    let captured_revision = editor.workspace().scene(document).unwrap().revision();
    let control = editor.hold_next_save(editor::SaveTestPhase::BeforePublication);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());

    make_authoring_name_dirty(&mut editor, document, "later-value");
    for _ in 0..3 {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            frame.persistence().operation(),
            Some(nara_tooling::EditorPersistenceOperation::Saving {
                document,
                captured_revision
            })
        );
        assert!(frame.persistence().result().is_none());
        assert!(frame.workspace().scene(document).unwrap().dirty);
    }
    assert_eq!(scene_name_value(&editor, document), "later-value");
    assert_eq!(fs::read(&source_path).unwrap(), original);
    assert!(matches!(
        editor.advance_close(),
        editor::EditorProjectCloseProgress::Incomplete {
            file_cleanup_pending: true,
            ..
        }
    ));
    assert!(editor.persistence_view().result().is_none());

    control.release();
    let completed = drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        completed.persistence().result(),
        Some(EditorPersistenceResult::Saved { document: saved, revision, .. })
            if saved == document && revision == captured_revision
    ));
    let slot = editor.workspace().scene(document).unwrap();
    assert_eq!(slot.saved_revision(), captured_revision);
    assert!(slot.is_dirty());
    assert_eq!(scene_name_value(&editor, document), "later-value");
    let persisted = fs::read_to_string(&source_path).unwrap();
    assert!(persisted.contains("captured-value"));
    assert!(!persisted.contains("later-value"));
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[cfg(feature = "tooling")]
#[test]
fn published_save_requires_worker_return_before_acknowledgement_and_session_close() {
    let project = TestProject::new("editor-save-after-publication");
    let source_path = project.root.join("scenes/startup.scene.json");
    fs::write(
        &source_path,
        editable_authoring_scene("original-value")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    let saved_revision = editor.workspace().scene(document).unwrap().saved_revision();
    make_authoring_name_dirty(&mut editor, document, "published-value");
    let captured_revision = editor.workspace().scene(document).unwrap().revision();
    let control = editor.hold_next_save(editor::SaveTestPhase::AfterPublication);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());
    let published = fs::read(&source_path).unwrap();
    assert!(String::from_utf8_lossy(&published).contains("published-value"));
    for _ in 0..3 {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            frame.persistence().operation(),
            Some(nara_tooling::EditorPersistenceOperation::Saving {
                document,
                captured_revision
            })
        );
        assert!(frame.persistence().result().is_none());
        let slot = editor.workspace().scene(document).unwrap();
        assert_eq!(slot.saved_revision(), saved_revision);
        assert!(slot.is_dirty());
    }
    assert!(matches!(
        editor.advance_close(),
        editor::EditorProjectCloseProgress::Incomplete {
            file_cleanup_pending: true,
            ..
        }
    ));
    assert!(editor.persistence_view().result().is_none());

    control.release();
    let close = drive_editor_close_to_completion(&mut editor);
    assert!(matches!(
        close,
        editor::EditorProjectCloseProgress::Complete(_)
    ));
    assert!(close.task_shutdown().is_complete());
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Saved { document: saved, revision, digest })
            if saved == document && revision == captured_revision
                && digest == nara_tooling::EditorDocumentDigest::new(
                    published.len() as u64, *blake3::hash(&published).as_bytes()
                )
    ));
    let slot = editor.workspace().scene(document).unwrap();
    assert_eq!(slot.saved_revision(), captured_revision);
    assert!(!slot.is_dirty());
    assert_eq!(fs::read(&source_path).unwrap(), published);
}

#[cfg(all(feature = "tooling", feature = "presentation-2d"))]
#[test]
fn pending_scene_save_preserves_preview_and_projects_later_edits() {
    let project = TestProject::new("editor-save-preview-continuity");
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let document = editor.workspace().active_document().unwrap();
    let base_revision = editor.workspace().scene(document).unwrap().revision();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ComposeScene {
                document,
                base_revision,
                command: nara_tooling::SceneCompositionCommand::CreateEntity {
                    kind: nara_tooling::SceneCompositionEntityKind::ColoredSprite,
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            })
            .applied
    );
    let captured_revision = editor.workspace().scene(document).unwrap().revision();
    editor.configure_editor_preview(nara_render::Extent2d::new(320, 180));
    let initial = editor.take_editor_preview_candidate().unwrap();
    assert_eq!(initial.hits().len(), 1);
    let control = editor.hold_next_save(editor::SaveTestPhase::BeforePublication);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());
    let pending = editor
        .take_editor_preview_candidate()
        .expect("moving the file Host to Save must preserve the Scene preview");
    assert_eq!(pending.document(), document);
    assert_eq!(pending.revision(), captured_revision);
    assert_eq!(pending.hits().len(), 1);

    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ComposeScene {
                document,
                base_revision: captured_revision,
                command: nara_tooling::SceneCompositionCommand::CreateEntity {
                    kind: nara_tooling::SceneCompositionEntityKind::ColoredSprite,
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            })
            .applied
    );
    let later_revision = editor.workspace().scene(document).unwrap().revision();
    assert_ne!(later_revision, captured_revision);
    let frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(frame.persistence().result().is_none());
    let later = editor
        .take_editor_preview_candidate()
        .expect("Scene preview must accept a later edit while Save is still held");
    assert_eq!(later.document(), document);
    assert_eq!(later.revision(), later_revision);
    assert_eq!(later.hits().len(), 2);
    assert!(later.preview_omissions().is_empty());

    control.release();
    let completed = drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        completed.persistence().result(),
        Some(EditorPersistenceResult::Saved { revision, .. }) if revision == captured_revision
    ));
    assert!(completed.workspace().scene(document).unwrap().dirty);
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
#[test]
fn legacy_prefab_source_navigation_is_busy_until_scene_save_returns_its_host() {
    let project = TestProject::new("editor-save-prefab-navigation-busy");
    let anchor = nara_identity::SceneEntityId::new("prefab-anchor").unwrap();
    let mut instance = SceneEntityRecord::new(anchor.clone());
    instance.prefab = Some(PrefabInstance {
        source: AssetRef::path("panel.prefab.json").unwrap(),
        overrides: None,
    });
    fs::write(
        project.root.join("prefabs/panel.prefab.json"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            nara_identity::SceneEntityId::new("panel").unwrap(),
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([editable_name_entity("original-value"), instance])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    make_authoring_name_dirty(&mut editor, document, "captured-value");
    let control = editor.hold_next_save(editor::SaveTestPhase::BeforePublication);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());

    let blocked = open_prefab_from_scene(&mut editor, document, &anchor);
    assert!(!blocked.applied);
    assert_eq!(
        first_code(&blocked.diagnostics),
        "project.editor.document-open-busy"
    );
    assert_eq!(editor.workspace().len(), 1);
    assert_eq!(editor.workspace().active_document(), Some(document));
    assert!(editor.persistence_view().result().is_none());

    control.release();
    let completed = drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        completed.persistence().result(),
        Some(EditorPersistenceResult::Saved { .. })
    ));
    let opened = open_prefab_from_scene(&mut editor, document, &anchor);
    assert!(opened.applied, "{:?}", opened.diagnostics);
    assert!(
        editor
            .workspace()
            .prefab(opened.opened_document.unwrap())
            .is_some()
    );
    assert_eq!(editor.workspace().len(), 2);
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[cfg(feature = "tooling")]
#[test]
fn content_refresh_retry_waits_for_the_held_save_to_return_its_host() {
    let project = TestProject::new("editor-save-deferred-content-refresh");
    let source_path = project.root.join("scenes/startup.scene.json");
    let anchor = nara_identity::SceneEntityId::new("prefab-anchor").unwrap();
    let mut instance = SceneEntityRecord::new(anchor.clone());
    instance.prefab = Some(nara_scene::PrefabInstance {
        source: AssetRef::path("panel.prefab.json").unwrap(),
        overrides: None,
    });
    fs::write(
        project.root.join("prefabs/panel.prefab.json"),
        editable_authoring_prefab("original-prefab")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    fs::write(
        &source_path,
        SceneDocument::from_ordered_roots([editable_name_entity("original-scene"), instance])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let scene_bytes = fs::read(&source_path).unwrap();
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let scene_document = editor.workspace().active_document().unwrap();
    let scene_revision = editor.workspace().scene(scene_document).unwrap().revision();
    let frame = editor.current_frame();
    let open_report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene_document,
                    entity: Some(anchor.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene_document,
                    base_revision: scene_revision,
                    entity: anchor,
                }),
            ],
        ))
        .unwrap();
    let EditorActionOutcome::Workspace(opened) = &open_report.outcomes()[1] else {
        panic!("opening the refresh fixture must return a workspace outcome");
    };
    assert!(opened.applied, "{:?}", opened.diagnostics);
    let prefab_document = opened.opened_document.unwrap();
    let prefab_revision = editor
        .workspace()
        .prefab(prefab_document)
        .unwrap()
        .revision();
    let edited_prefab = editor.apply_workspace_command(EditorWorkspaceCommand::ApplyPrefabPatch {
        document: prefab_document,
        base_revision: prefab_revision,
        patch: ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: nara_identity::SceneEntityId::new("editable-name").unwrap(),
            component: ComponentTypeId::new("nara.scene.Name"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("value"),
            value: ComponentValue::String("saved-prefab".to_owned()),
        }]),
    });
    assert!(edited_prefab.applied, "{:?}", edited_prefab.diagnostics);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(prefab_document)
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    let saved_prefab = drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        saved_prefab.persistence().result(),
        Some(EditorPersistenceResult::Saved { .. })
    ));
    assert!(matches!(
        saved_prefab.project_content(),
        EditorProjectContentView::RefreshPending { .. }
    ));
    let original_content = editor.test_project_content_revision();

    fs::write(&source_path, b"invalid scene JSON").unwrap();
    let rejected = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        rejected.project_content(),
        EditorProjectContentView::Stale {
            failure: EditorProjectContentRefreshFailure::InvalidContent,
            ..
        }
    ));
    fs::write(&source_path, &scene_bytes).unwrap();
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult {
            attempt: editor
                .persistence_view()
                .attempt()
                .expect("the test observed a terminal persistence result")
        }),
        nara_tooling::EditorPersistenceRequestResult::Acknowledged { .. }
    ));
    let frame = editor.current_frame();
    let retry = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::ProjectContent(
                nara_tooling::EditorProjectContentAction::RetryRefresh,
            )],
        ))
        .unwrap();
    assert!(matches!(
        retry.outcomes()[0],
        EditorActionOutcome::ProjectContent(
            nara_tooling::EditorProjectContentRefreshRequestResult::Accepted
        )
    ));
    let scene_revision = editor.workspace().scene(scene_document).unwrap().revision();
    let edited_scene = editor.apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
        document: scene_document,
        base_revision: scene_revision,
        patch: ScenePatchDocument::new([ScenePatchOperation::SetField {
            entity: nara_identity::SceneEntityId::new("editable-name").unwrap(),
            component: ComponentTypeId::new("nara.scene.Name"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("value"),
            value: ComponentValue::String("saved-scene".to_owned()),
        }]),
    });
    assert!(edited_scene.applied, "{:?}", edited_scene.diagnostics);
    let captured_revision = editor.workspace().scene(scene_document).unwrap().revision();
    let control = editor.hold_next_save(editor::SaveTestPhase::BeforePublication);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(scene_document)
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());
    for _ in 0..3 {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        assert!(frame.persistence().result().is_none());
        assert!(
            matches!(frame.project_content(), EditorProjectContentView::RefreshPending { document, .. }
            if document == prefab_document)
        );
        assert_eq!(editor.test_project_content_revision(), original_content);
    }

    control.release();
    let saved_scene = drive_persistence_to_terminal(&mut editor);
    assert!(
        matches!(saved_scene.persistence().result(), Some(EditorPersistenceResult::Saved { document, revision, .. })
        if document == scene_document && revision == captured_revision)
    );
    assert_eq!(editor.test_project_content_revision(), original_content);
    let refreshed = editor.drive_editor_frame(Duration::ZERO);
    assert!(
        matches!(refreshed.project_content(), EditorProjectContentView::Current { generation }
        if generation.get() == 2)
    );
    assert_ne!(editor.test_project_content_revision(), original_content);
    assert!(!editor.workspace().scene(scene_document).unwrap().is_dirty());
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[cfg(feature = "tooling")]
#[test]
fn dropped_editor_retains_published_save_custody_until_the_worker_returns() {
    let project = TestProject::new("editor-save-abandoned-custody");
    let source_directory = project.root.join("scenes");
    let source_path = source_directory.join("startup.scene.json");
    fs::write(
        &source_path,
        editable_authoring_scene("original-value")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let initial_entries = fs::read_dir(&source_directory).unwrap().count();
    let retained_before = runtime_quarantine_status().current_thread_retained();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    make_authoring_name_dirty(&mut editor, document, "published-value");
    let control = editor.hold_next_save(editor::SaveTestPhase::AfterPublication);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());
    let published = fs::read(&source_path).unwrap();
    assert!(String::from_utf8_lossy(&published).contains("published-value"));
    assert!(editor.persistence_view().result().is_none());

    drop(editor);
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        retained_before + 1
    );
    assert_eq!(
        drive_runtime_quarantine().current_thread_retained(),
        retained_before + 1
    );
    assert_eq!(fs::read(&source_path).unwrap(), published);

    control.release();
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime_quarantine_status().current_thread_retained() > retained_before {
        let _ = drive_runtime_quarantine();
        assert!(
            Instant::now() < deadline,
            "published Save custody did not retire after worker return"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        retained_before
    );
    assert_eq!(fs::read(&source_path).unwrap(), published);
    assert_eq!(
        fs::read_dir(&source_directory).unwrap().count(),
        initial_entries
    );
}
