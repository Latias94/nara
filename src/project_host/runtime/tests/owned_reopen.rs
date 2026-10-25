use super::*;

use nara_tooling::{
    EditorPersistenceOperation, EditorPersistenceRejection, EditorPersistenceRequestKind,
    EditorPersistenceRequestResult, EditorPlayCommand, EditorPlayRejection,
    EditorPlayRequestResult,
};

#[test]
fn prepared_reopen_rejects_a_later_edit_without_changing_its_source_or_saved_checkpoint() {
    let project = TestProject::new("editor-reopen-stale-prepared");
    let source_path = project.root.join("scenes/startup.scene.json");
    fs::write(
        &source_path,
        editable_authoring_scene("original")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let original = fs::read(&source_path).unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    make_authoring_name_dirty(&mut editor, document, "dirty-at-request");
    fs::write(
        &source_path,
        editable_authoring_scene("reopened-candidate")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let control = editor.hold_next_reopen(editor::ReopenTestPhase::AfterPreparation);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());

    make_authoring_name_dirty(&mut editor, document, "later-edit");
    let before = editor.drive_editor_frame(Duration::ZERO);
    let before_slot = editor.workspace().scene(document).unwrap();
    let source_binding = before_slot.source_binding().cloned();
    let saved_revision = before_slot.saved_revision();
    let saved_digest = before_slot.saved_digest();
    let history = before_slot.session().history_status();
    let revision = before_slot.revision();
    assert_eq!(
        before.persistence().operation(),
        Some(EditorPersistenceOperation::Opening { document })
    );
    assert!(before.persistence().result().is_none());

    // Return the path to the original observed bytes before rejecting the stale candidate.
    // A Host incorrectly rebound to the candidate would reject the subsequent Save.
    fs::write(&source_path, &original).unwrap();
    control.release();
    let completed = drive_persistence_to_terminal(&mut editor);
    assert_eq!(
        completed.persistence().result(),
        Some(EditorPersistenceResult::Rejected {
            request: EditorPersistenceRequestKind::Reopen,
            document: Some(document),
            reason: EditorPersistenceRejection::StaleRevision,
        })
    );
    assert_eq!(completed.workspace(), before.workspace());
    let slot = editor.workspace().scene(document).unwrap();
    assert_eq!(slot.revision(), revision);
    assert_eq!(slot.source_binding(), source_binding.as_ref());
    assert_eq!(slot.saved_revision(), saved_revision);
    assert_eq!(slot.saved_digest(), saved_digest);
    assert_eq!(slot.session().history_status(), history);
    assert_eq!(scene_name_value(&editor, document), "later-edit");
    assert_eq!(fs::read(&source_path).unwrap(), original);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult {
            attempt: editor
                .persistence_view()
                .attempt()
                .expect("the test observed a terminal persistence result")
        }),
        EditorPersistenceRequestResult::Acknowledged { .. }
    ));
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    assert!(matches!(
        drive_persistence_to_terminal(&mut editor)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Saved { .. })
    ));
    assert!(
        fs::read_to_string(&source_path)
            .unwrap()
            .contains("later-edit")
    );
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[test]
fn prepared_reopen_publishes_captured_bytes_but_a_later_disk_write_conflicts_with_save() {
    let project = TestProject::new("editor-reopen-detect-only");
    let source_path = project.root.join("scenes/startup.scene.json");
    fs::write(
        &source_path,
        editable_authoring_scene("initial")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    let source_id = editor
        .workspace()
        .scene(document)
        .unwrap()
        .revision()
        .source_id();
    let captured = editable_authoring_scene("captured-a")
        .to_json_string()
        .unwrap();
    fs::write(&source_path, &captured).unwrap();
    let control = editor.hold_next_reopen(editor::ReopenTestPhase::AfterPreparation);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());
    let replacement = editable_authoring_scene("external-b")
        .to_json_string()
        .unwrap();
    fs::write(&source_path, &replacement).unwrap();
    control.release();
    let completed = drive_persistence_to_terminal(&mut editor);
    assert!(matches!(completed.persistence().result(),
        Some(EditorPersistenceResult::Opened { document: opened, .. }) if opened == document));
    assert_eq!(scene_name_value(&editor, document), "captured-a");
    let slot = editor.workspace().scene(document).unwrap();
    assert!(!slot.is_dirty());
    assert_ne!(slot.revision().source_id(), source_id);
    assert_eq!(
        slot.saved_digest(),
        Some(nara_tooling::EditorDocumentDigest::new(
            captured.len() as u64,
            *blake3::hash(captured.as_bytes()).as_bytes()
        ))
    );
    assert_eq!(fs::read_to_string(&source_path).unwrap(), replacement);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult {
            attempt: editor
                .persistence_view()
                .attempt()
                .expect("the test observed a terminal persistence result")
        }),
        EditorPersistenceRequestResult::Acknowledged { .. }
    ));
    make_authoring_name_dirty(&mut editor, document, "local-successor");
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    assert_eq!(
        drive_persistence_to_terminal(&mut editor)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Rejected {
            request: EditorPersistenceRequestKind::Save,
            document: Some(document),
            reason: EditorPersistenceRejection::TargetChanged,
        })
    );
    assert_eq!(fs::read_to_string(&source_path).unwrap(), replacement);
    assert!(editor.workspace().scene(document).unwrap().is_dirty());
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[test]
fn pending_reopen_blocks_trusted_play_admission_and_close_waits_for_the_read_owner() {
    let project = TestProject::new("editor-reopen-trusted-play-busy");
    let original = fs::read(project.root.join("scenes/startup.scene.json")).unwrap();
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let document = editor.workspace().active_document().unwrap();
    let control = editor.hold_next_reopen(editor::ReopenTestPhase::BeforeRead);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());
    for command in [
        EditorPlayCommand::Play,
        EditorPlayCommand::Restart,
        EditorPlayCommand::RunLastGood,
    ] {
        assert_eq!(
            editor.request_play(command),
            EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy)
        );
    }
    let revision = editor.workspace().scene(document).unwrap().revision();
    let frame = editor.current_frame();
    let from_frame = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Play(nara_tooling::EditorPlayAction::Play {
                document,
                base_revision: revision,
            })],
        ))
        .unwrap();
    assert!(matches!(
        from_frame.outcomes(),
        [EditorActionOutcome::Play(
            EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy)
        )]
    ));
    assert!(matches!(
        editor.advance_close(),
        editor::EditorProjectCloseProgress::Incomplete { .. }
    ));
    assert!(editor.persistence_view().result().is_none());
    control.release();
    let completed = drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        completed.persistence().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    assert_eq!(
        fs::read(project.root.join("scenes/startup.scene.json")).unwrap(),
        original
    );
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[test]
fn pending_reopen_blocks_external_play_admission_before_build_receipt_preparation() {
    let project = TestProject::new("editor-reopen-external-play-busy");
    project.configure_supervised_play("editor-reopen-external-play-busy");
    let mut host = editor::EditorProjectOpenHost::new();
    let mut attempt = host.begin(project.writable_capability(), Some("desktop".to_owned()));
    let mut editor = loop {
        match host.advance(attempt) {
            editor::EditorProjectOpenProgress::Pending(next) => attempt = next,
            editor::EditorProjectOpenProgress::Opened(editor) => break editor,
            editor::EditorProjectOpenProgress::Rejected(error) => {
                panic!("supervised project rejected: {:?}", error.diagnostics())
            }
        }
    };
    assert_eq!(
        editor.project_readiness().runtime(),
        nara_tooling::EditorRuntimeAvailability::ExternalSupervised
    );
    let document = editor.workspace().active_document().unwrap();
    let control = editor.hold_next_reopen(editor::ReopenTestPhase::BeforeRead);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());
    for command in [
        EditorPlayCommand::Play,
        EditorPlayCommand::Restart,
        EditorPlayCommand::RunLastGood,
    ] {
        assert_eq!(
            editor.request_play(command),
            EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy)
        );
    }
    assert!(editor.take_external_play_request().is_none());
    control.release();
    assert!(matches!(
        drive_persistence_to_terminal(&mut editor)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[test]
fn dropped_editor_retains_prepared_reopen_until_its_worker_physically_returns() {
    let project = TestProject::new("editor-reopen-abandoned-read");
    let directory = project.root.join("scenes");
    let source_path = directory.join("startup.scene.json");
    fs::write(
        &source_path,
        editable_authoring_scene("captured")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let source = fs::read(&source_path).unwrap();
    let entries = fs::read_dir(&directory).unwrap().count();
    let retained_before = runtime_quarantine_status().current_thread_retained();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    let control = editor.hold_next_reopen(editor::ReopenTestPhase::AfterPreparation);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());
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
    assert_eq!(fs::read(&source_path).unwrap(), source);
    control.release();
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime_quarantine_status().current_thread_retained() > retained_before {
        let _ = drive_runtime_quarantine();
        assert!(
            Instant::now() < deadline,
            "Reopen read owner did not physically retire"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        retained_before
    );
    assert_eq!(fs::read(&source_path).unwrap(), source);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), entries);
}
