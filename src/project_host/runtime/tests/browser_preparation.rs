use super::*;

use nara_tooling::{
    EditorCloseDecision, EditorDocumentId, EditorDocumentOpenId, EditorDocumentOpenPhase,
    EditorDocumentOpenResult, EditorFrameModel, EditorWorkspaceIntentPhase,
    EditorWorkspaceIntentRequestResult, EditorWorkspaceIntentResult,
};

use crate::project_host::authoring::AuthoringDocumentKind;

const SCENE: &str = "scenes/external.scene.json";
const PREFAB: &str = "prefabs/external.prefab.json";

#[test]
fn prepared_browser_open_rejects_a_later_edit_without_losing_working_history() {
    let project = project("browser-open-later-edit");
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    let control = editor.hold_next_document_open(editor::OpenTestPhase::AfterPreparation);
    let request = begin_open(&mut editor, SCENE);
    assert!(control.wait_until_started());

    make_authoring_name_dirty(&mut editor, document, "later-working-value");
    let before = editor.drive_editor_frame(Duration::ZERO);
    let slot = editor.workspace().scene(document).unwrap();
    let working = slot.session().document().clone();
    let revision = slot.revision();
    let saved_revision = slot.saved_revision();
    let saved_digest = slot.saved_digest();
    let source_binding = slot.source_binding().cloned();
    let history = slot.session().history_status();
    for _ in 0..3 {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            frame.document_open().pending(),
            Some((request, SCENE, EditorDocumentOpenPhase::Preparing))
        );
        assert!(frame.document_open().result().is_none());
        assert_eq!(frame.workspace(), before.workspace());
    }
    control.release();
    let completed = wait_open(&mut editor, request);
    assert_eq!(
        completed.document_open().result(),
        Some(EditorDocumentOpenResult::Rejected {
            request,
            code: "project.editor.document-open-stale-document",
        })
    );
    assert_eq!(completed.workspace(), before.workspace());
    let slot = editor.workspace().scene(document).unwrap();
    assert_eq!(slot.session().document(), &working);
    assert_eq!(slot.session().history_status(), history);
    assert_eq!(slot.revision(), revision);
    assert_eq!(slot.saved_revision(), saved_revision);
    assert_eq!(slot.saved_digest(), saved_digest);
    assert_eq!(slot.source_binding(), source_binding.as_ref());
    assert_eq!(scene_name_value(&editor, document), "later-working-value");
    acknowledge_open(&mut editor, request);
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::Undo {
                document,
                base_revision: revision,
            })
            .applied
    );
    assert_eq!(scene_name_value(&editor, document), "startup");
    close(&mut editor);
}

#[test]
fn prepared_browser_open_rejects_a_refreshed_index_without_replacing_the_document() {
    let project = project("browser-open-refreshed-index");
    let mut editor = open_generic_editor_with_ready_index(&project);
    let original = editor.drive_editor_frame(Duration::ZERO);
    let old_index = editor.project_source_index_snapshot().unwrap();
    let control = editor.hold_next_document_open(editor::OpenTestPhase::AfterPreparation);
    let request = begin_open(&mut editor, SCENE);
    assert!(control.wait_until_started());
    refresh_browser(&mut editor);
    assert!(!Arc::ptr_eq(
        &old_index,
        &editor.project_source_index_snapshot().unwrap()
    ));
    assert_eq!(
        editor.current_frame().document_open().pending(),
        Some((request, SCENE, EditorDocumentOpenPhase::Preparing))
    );
    control.release();
    let completed = wait_open(&mut editor, request);
    assert_eq!(
        completed.document_open().result(),
        Some(EditorDocumentOpenResult::Rejected {
            request,
            code: "project.editor.document-index-stale",
        })
    );
    assert_eq!(completed.workspace(), original.workspace());
    acknowledge_open(&mut editor, request);
    close(&mut editor);
}

#[test]
fn browser_worker_rejects_identity_replacement_and_malformed_source_without_publication() {
    for replace_identity in [false, true] {
        let project = project("browser-open-read-refusal");
        let path = project.root.join(SCENE);
        let mut editor = open_generic_editor_with_ready_index(&project);
        let before = editor.drive_editor_frame(Duration::ZERO);
        let control = editor.hold_next_document_open(editor::OpenTestPhase::BeforeRead);
        let request = begin_open(&mut editor, SCENE);
        assert!(control.wait_until_started());
        let expected = if replace_identity {
            // Keep the original file alive so the replacement cannot reuse its native identity.
            fs::rename(&path, project.root.join("scenes/previous.scene.json")).unwrap();
            fs::write(
                &path,
                editable_authoring_scene("replacement")
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
            "project.editor.document-source-identity-changed"
        } else {
            fs::write(&path, b"{").unwrap();
            "project.editor.document-source-format-invalid"
        };
        control.release();
        let completed = wait_open(&mut editor, request);
        assert_eq!(
            completed.document_open().result(),
            Some(EditorDocumentOpenResult::Rejected {
                request,
                code: expected,
            })
        );
        assert_eq!(completed.workspace(), before.workspace());
        assert!(completed.workspace_intent().intent().is_none());
        acknowledge_open(&mut editor, request);
        close(&mut editor);
    }
}

#[test]
fn browser_binding_rechecks_the_decoded_source_digest_before_retiring_its_predecessor() {
    let project = project("browser-open-binding-refusal");
    let mut editor = open_generic_editor_with_ready_index(&project);
    let before = editor.drive_editor_frame(Duration::ZERO);
    let control = editor.hold_next_document_open(editor::OpenTestPhase::BeforeBinding);
    let request = begin_open(&mut editor, SCENE);
    assert!(control.wait_until_started());
    let changed = editable_authoring_scene("changed-after-decode")
        .to_json_string()
        .unwrap();
    fs::write(project.root.join(SCENE), &changed).unwrap();
    control.release();
    let completed = wait_open(&mut editor, request);
    assert_eq!(
        completed.document_open().result(),
        Some(EditorDocumentOpenResult::Rejected {
            request,
            code: "project.editor.document-source-changed",
        })
    );
    assert_eq!(completed.workspace(), before.workspace());
    assert_eq!(
        fs::read_to_string(project.root.join(SCENE)).unwrap(),
        changed
    );
    acknowledge_open(&mut editor, request);
    close(&mut editor);
}

#[test]
fn cancelling_browser_open_waits_for_physical_return_and_requires_exact_result_acknowledgement() {
    let project = project("browser-open-cancel-and-retry");
    let mut editor = open_generic_editor_with_ready_index(&project);
    let original_document = editor.workspace().active_document().unwrap();
    let control = editor.hold_next_document_open(editor::OpenTestPhase::AfterPreparation);
    let request = begin_open(&mut editor, SCENE);
    assert!(control.wait_until_started());
    let foreign_frame = editor.drive_editor_frame(Duration::ZERO);
    let foreign = EditorDocumentOpenId::new(foreign_frame.revision(), request.get());
    assert_ne!(
        foreign, request,
        "the request is bound to its admitted frame, not just its counter"
    );
    assert_eq!(
        browser_action(
            &mut editor,
            EditorProjectBrowserAction::CancelOpen { request: foreign }
        ),
        EditorProjectBrowserActionOutcome::Rejected {
            code: "project.editor.document-open-cancel-stale"
        }
    );
    assert_eq!(
        browser_action(
            &mut editor,
            EditorProjectBrowserAction::CancelOpen { request }
        ),
        EditorProjectBrowserActionOutcome::OpenCancellationRequested { request }
    );
    for _ in 0..3 {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            frame.document_open().pending(),
            Some((request, SCENE, EditorDocumentOpenPhase::Cancelling))
        );
        assert!(frame.document_open().result().is_none());
        assert_eq!(frame.workspace().active_document, Some(original_document));
    }
    assert_eq!(
        browser_action(
            &mut editor,
            EditorProjectBrowserAction::Open {
                relative_path: Arc::from(PREFAB)
            }
        ),
        EditorProjectBrowserActionOutcome::Rejected {
            code: "project.editor.document-open-busy"
        }
    );
    control.release();
    let cancelled = wait_open(&mut editor, request);
    assert_eq!(
        cancelled.document_open().result(),
        Some(EditorDocumentOpenResult::Cancelled { request })
    );
    assert_eq!(
        cancelled.workspace().active_document,
        Some(original_document)
    );
    assert_eq!(
        browser_action(
            &mut editor,
            EditorProjectBrowserAction::AcknowledgeOpen { request: foreign }
        ),
        EditorProjectBrowserActionOutcome::Rejected {
            code: "project.editor.document-open-acknowledgement-stale"
        }
    );
    assert_eq!(
        editor
            .drive_editor_frame(Duration::ZERO)
            .document_open()
            .result(),
        Some(EditorDocumentOpenResult::Cancelled { request })
    );
    assert_eq!(
        browser_action(
            &mut editor,
            EditorProjectBrowserAction::Open {
                relative_path: Arc::from(PREFAB)
            }
        ),
        EditorProjectBrowserActionOutcome::Rejected {
            code: "project.editor.document-open-busy"
        }
    );
    acknowledge_open(&mut editor, request);

    // Both ordinary lanes consume the same public pending/result protocol after cancellation.
    let prefab_request = begin_open(&mut editor, PREFAB);
    assert_ne!(prefab_request, request);
    let prefab_document = opened(&wait_open(&mut editor, prefab_request), prefab_request);
    assert_eq!(
        editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .source_binding()
            .locator(),
        PREFAB
    );
    acknowledge_open(&mut editor, prefab_request);
    let scene_request = begin_open(&mut editor, SCENE);
    let scene_document = opened(&wait_open(&mut editor, scene_request), scene_request);
    assert_ne!(scene_document, original_document);
    assert!(editor.workspace().scene(original_document).is_none());
    assert!(editor.workspace().prefab(prefab_document).is_some());
    assert_eq!(scene_name_value(&editor, scene_document), "external");
    acknowledge_open(&mut editor, scene_request);
    assert_eq!(
        workspace_action(&mut editor, EditorWorkspaceIntentAction::AcknowledgeResult),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    close(&mut editor);
}

#[test]
fn exact_browser_request_authority_survives_old_frames_but_cannot_cross_sessions() {
    let project_a = project("browser-request-session-a");
    let project_b = project("browser-request-session-b");
    let mut session_a = open_generic_editor_with_ready_index(&project_a);
    let mut session_b = open_generic_editor_with_ready_index(&project_b);
    let control_a = session_a.hold_next_document_open(editor::OpenTestPhase::BeforeRead);
    let control_b = session_b.hold_next_document_open(editor::OpenTestPhase::BeforeRead);
    let request_a = begin_open(&mut session_a, SCENE);
    let old_frame_a = session_a.current_frame().revision();
    let request_b = begin_open(&mut session_b, SCENE);
    assert!(control_a.wait_until_started());
    assert!(control_b.wait_until_started());
    assert_eq!(request_a.get(), 1);
    assert_eq!(request_b.get(), request_a.get());
    assert_ne!(request_a, request_b);

    let pending_a = session_a.drive_editor_frame(Duration::ZERO);
    assert_ne!(pending_a.revision(), old_frame_a);
    let cancelled = session_a
        .dispatch_editor_actions(EditorActionBatch::new(
            old_frame_a,
            [EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::CancelOpen { request: request_a },
            )],
        ))
        .expect("an exact cancellation remains admissible from its older frame");
    assert_eq!(
        cancelled.outcomes(),
        [EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::OpenCancellationRequested { request: request_a },
        )]
    );
    assert_eq!(
        session_a
            .drive_editor_frame(Duration::ZERO)
            .document_open()
            .pending(),
        Some((request_a, SCENE, EditorDocumentOpenPhase::Cancelling))
    );

    let pending_b = session_b.drive_editor_frame(Duration::ZERO);
    let foreign = session_b
        .dispatch_editor_actions(EditorActionBatch::new(
            pending_b.revision(),
            [
                EditorAction::ProjectBrowser(EditorProjectBrowserAction::CancelOpen {
                    request: request_a,
                }),
                EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
                    request: request_a,
                }),
            ],
        ))
        .unwrap();
    assert_eq!(
        foreign.outcomes(),
        [
            EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Rejected {
                code: "project.editor.document-open-cancel-stale",
            }),
            EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Rejected {
                code: "project.editor.document-open-acknowledgement-stale",
            }),
        ]
    );
    assert_eq!(
        session_b.drive_editor_frame(Duration::ZERO).document_open(),
        pending_b.document_open()
    );

    control_a.release();
    let terminal_a = wait_open(&mut session_a, request_a);
    assert_eq!(
        terminal_a.document_open().result(),
        Some(EditorDocumentOpenResult::Cancelled { request: request_a })
    );
    assert_ne!(terminal_a.revision(), old_frame_a);
    let acknowledged = session_a
        .dispatch_editor_actions(EditorActionBatch::new(
            old_frame_a,
            [EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::AcknowledgeOpen { request: request_a },
            )],
        ))
        .expect("an exact acknowledgement remains admissible from its older frame");
    assert_eq!(
        acknowledged.outcomes(),
        [EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::OpenAcknowledged { request: request_a },
        )]
    );
    assert!(
        session_a
            .drive_editor_frame(Duration::ZERO)
            .document_open()
            .result()
            .is_none()
    );

    control_b.release();
    let terminal_b = wait_open(&mut session_b, request_b);
    let document_b = opened(&terminal_b, request_b);
    let foreign = session_b
        .dispatch_editor_actions(EditorActionBatch::new(
            terminal_b.revision(),
            [
                EditorAction::ProjectBrowser(EditorProjectBrowserAction::AcknowledgeOpen {
                    request: request_a,
                }),
                EditorAction::ProjectBrowser(EditorProjectBrowserAction::CancelOpen {
                    request: request_a,
                }),
            ],
        ))
        .unwrap();
    assert_eq!(
        foreign.outcomes(),
        [
            EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Rejected {
                code: "project.editor.document-open-acknowledgement-stale",
            }),
            EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Rejected {
                code: "project.editor.document-open-cancel-stale",
            }),
        ]
    );
    let still_terminal_b = session_b.drive_editor_frame(Duration::ZERO);
    assert_eq!(still_terminal_b.document_open(), terminal_b.document_open());
    assert_eq!(
        still_terminal_b.workspace().active_document,
        Some(document_b)
    );
    acknowledge_open(&mut session_b, request_b);
    assert_eq!(
        workspace_action(
            &mut session_b,
            EditorWorkspaceIntentAction::AcknowledgeResult
        ),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    close(&mut session_a);
    close(&mut session_b);
}

#[test]
fn rejected_workspace_exit_does_not_cancel_an_already_admitted_browser_open() {
    let project = project("browser-open-rejected-exit");
    let mut editor = open_generic_editor_with_ready_index(&project);
    let scene = editor.workspace().active_document().unwrap();
    let scene_revision = editor.workspace().scene(scene).unwrap().revision();
    let scene_document = editor
        .workspace()
        .scene(scene)
        .unwrap()
        .session()
        .document()
        .clone();
    let first_request = begin_open(&mut editor, PREFAB);
    let first_prefab = opened(&wait_open(&mut editor, first_request), first_request);
    acknowledge_open(&mut editor, first_request);
    assert_eq!(
        workspace_action(
            &mut editor,
            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument {
                document: first_prefab
            },)
        ),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    let closed = editor.drive_editor_frame(Duration::ZERO);
    let close_result = Some(EditorWorkspaceIntentResult::Applied {
        intent: EditorWorkspaceIntent::CloseDocument {
            document: first_prefab,
        },
    });
    assert_eq!(closed.workspace_intent().result(), close_result);
    assert!(editor.workspace().prefab(first_prefab).is_none());
    assert!(editor.workspace().scene(scene).is_some());

    // Keep the old Workspace result unacknowledged while the independent Prefab lane opens.
    let control = editor.hold_next_document_open(editor::OpenTestPhase::BeforeRead);
    let request = begin_open(&mut editor, PREFAB);
    assert!(control.wait_until_started());
    assert_eq!(
        workspace_action(
            &mut editor,
            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::Exit,)
        ),
        EditorWorkspaceIntentRequestResult::Rejected(
            nara_tooling::EditorWorkspaceIntentRejection::ResultPending,
        )
    );
    let still_preparing = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        still_preparing.document_open().pending(),
        Some((request, PREFAB, EditorDocumentOpenPhase::Preparing))
    );
    assert!(still_preparing.document_open().result().is_none());
    assert_eq!(still_preparing.workspace_intent().result(), close_result);
    assert!(still_preparing.workspace_intent().intent().is_none());
    control.release();
    let completed = wait_open(&mut editor, request);
    let reopened_prefab = opened(&completed, request);
    assert_ne!(reopened_prefab, first_prefab);
    assert_eq!(completed.workspace_intent().result(), close_result);
    assert_eq!(
        editor.workspace().scene(scene).unwrap().revision(),
        scene_revision
    );
    assert_eq!(
        editor
            .workspace()
            .scene(scene)
            .unwrap()
            .session()
            .document(),
        &scene_document
    );
    assert_eq!(
        editor
            .workspace()
            .prefab(reopened_prefab)
            .unwrap()
            .source_binding()
            .locator(),
        PREFAB
    );
    acknowledge_open(&mut editor, request);
    assert_eq!(
        workspace_action(&mut editor, EditorWorkspaceIntentAction::AcknowledgeResult),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    close(&mut editor);
}

#[test]
fn dirty_browser_replacement_rechecks_the_index_before_discarding_its_predecessor() {
    let project = project("browser-open-dirty-refresh");
    let disk = fs::read(project.root.join("scenes/startup.scene.json")).unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    make_authoring_name_dirty(&mut editor, document, "retained-dirty-value");
    let before = editor.drive_editor_frame(Duration::ZERO);
    let history = editor
        .workspace()
        .scene(document)
        .unwrap()
        .session()
        .history_status();
    let request = begin_open(&mut editor, SCENE);
    let awaiting = wait_close_prompt(&mut editor, request, document);
    let prompt = awaiting.workspace_intent().decision_prompt().unwrap();
    assert_eq!(
        browser_action(
            &mut editor,
            EditorProjectBrowserAction::CancelOpen { request }
        ),
        EditorProjectBrowserActionOutcome::Rejected {
            code: "project.editor.document-open-cancel-stale"
        }
    );
    refresh_browser(&mut editor);
    assert_eq!(
        workspace_action(&mut editor, prompt.resolve(EditorCloseDecision::Discard)),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    let completed = wait_open(&mut editor, request);
    assert_eq!(
        completed.document_open().result(),
        Some(EditorDocumentOpenResult::Rejected {
            request,
            code: "project.editor.document-index-stale",
        })
    );
    assert!(
        matches!(completed.workspace_intent().result(), Some(EditorWorkspaceIntentResult::Rejected {
        intent: EditorWorkspaceIntent::CloseDocument { document: rejected }, ..
    }) if rejected == document)
    );
    assert!(completed.workspace_intent().intent().is_none());
    assert_eq!(completed.workspace(), before.workspace());
    assert_eq!(
        editor
            .workspace()
            .scene(document)
            .unwrap()
            .session()
            .history_status(),
        history
    );
    assert_eq!(scene_name_value(&editor, document), "retained-dirty-value");
    assert_eq!(
        fs::read(project.root.join("scenes/startup.scene.json")).unwrap(),
        disk
    );
    acknowledge_open(&mut editor, request);
    assert_eq!(
        workspace_action(&mut editor, EditorWorkspaceIntentAction::AcknowledgeResult),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    close(&mut editor);
}

#[test]
fn dirty_browser_replacement_cancel_and_failed_save_preserve_the_old_document_and_history() {
    for decision in [EditorCloseDecision::Cancel, EditorCloseDecision::Save] {
        let project = project("browser-open-dirty-cancel-or-save");
        let disk = fs::read(project.root.join("scenes/startup.scene.json")).unwrap();
        let mut editor = open_generic_editor_with_ready_index(&project);
        let document = editor.workspace().active_document().unwrap();
        make_authoring_name_dirty(&mut editor, document, "retained-dirty-value");
        let before = editor.drive_editor_frame(Duration::ZERO);
        let slot = editor.workspace().scene(document).unwrap();
        let history = slot.session().history_status();
        let revision = slot.revision();
        let saved_revision = slot.saved_revision();
        let saved_digest = slot.saved_digest();
        let source_binding = slot.source_binding().cloned();
        let request = begin_open(&mut editor, SCENE);
        let awaiting = wait_close_prompt(&mut editor, request, document);
        if decision == EditorCloseDecision::Save {
            editor.test_fail_next_save(nara_tooling::EditorPersistenceFailureStage::Encode);
        }
        assert_eq!(
            workspace_action(
                &mut editor,
                awaiting
                    .workspace_intent()
                    .decision_prompt()
                    .unwrap()
                    .resolve(decision)
            ),
            EditorWorkspaceIntentRequestResult::Accepted
        );
        let completed = wait_open(&mut editor, request);
        assert_eq!(
            completed.document_open().result(),
            Some(EditorDocumentOpenResult::Cancelled { request })
        );
        assert_eq!(
            completed.workspace_intent().result(),
            Some(EditorWorkspaceIntentResult::Cancelled {
                intent: EditorWorkspaceIntent::CloseDocument { document },
            })
        );
        assert_eq!(completed.workspace(), before.workspace());
        let slot = editor.workspace().scene(document).unwrap();
        assert_eq!(slot.session().history_status(), history);
        assert_eq!(slot.revision(), revision);
        assert_eq!(slot.saved_revision(), saved_revision);
        assert_eq!(slot.saved_digest(), saved_digest);
        assert_eq!(slot.source_binding(), source_binding.as_ref());
        assert_eq!(scene_name_value(&editor, document), "retained-dirty-value");
        assert_eq!(
            fs::read(project.root.join("scenes/startup.scene.json")).unwrap(),
            disk
        );
        if decision == EditorCloseDecision::Save {
            assert_eq!(
                completed.persistence().result(),
                Some(EditorPersistenceResult::Failed {
                    request: nara_tooling::EditorPersistenceRequestKind::Save,
                    document: Some(document),
                    stage: nara_tooling::EditorPersistenceFailureStage::Encode,
                })
            );
        }
        acknowledge_open(&mut editor, request);
        assert_eq!(
            workspace_action(&mut editor, EditorWorkspaceIntentAction::AcknowledgeResult),
            EditorWorkspaceIntentRequestResult::Accepted
        );
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::Undo {
                    document,
                    base_revision: revision,
                })
                .applied
        );
        assert_eq!(scene_name_value(&editor, document), "startup");
        close(&mut editor);
    }
}

#[test]
fn close_cancels_a_held_browser_read_and_waits_for_its_physical_retirement() {
    let project = project("browser-open-close-held");
    let source = fs::read(project.root.join(SCENE)).unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let index = editor.project_source_index_snapshot().unwrap();
    let control = editor.hold_next_document_open(editor::OpenTestPhase::BeforeRead);
    let request =
        match editor.request_indexed_authoring_document(index, SCENE, AuthoringDocumentKind::Scene)
        {
            editor::EditorDocumentOpenRequestResult::Preparing { request } => request,
            other => panic!("the owned request must be admitted: {other:?}"),
        };
    assert!(control.wait_until_started());
    for _ in 0..3 {
        assert!(matches!(
            editor.advance_close(),
            editor::EditorProjectCloseProgress::Incomplete { .. }
        ));
        let frame = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            frame.document_open().pending(),
            Some((request, SCENE, EditorDocumentOpenPhase::Cancelling))
        );
        assert!(frame.document_open().result().is_none());
    }
    assert_eq!(fs::read(project.root.join(SCENE)).unwrap(), source);
    control.release();
    close(&mut editor);
    assert_eq!(fs::read(project.root.join(SCENE)).unwrap(), source);
}

#[test]
fn dropping_a_session_with_a_prepared_browser_host_retains_the_runtime_until_worker_return() {
    let project = project("browser-open-drop-held");
    let path = project.root.join(SCENE);
    let source = fs::read(&path).unwrap();
    let entries = fs::read_dir(path.parent().unwrap()).unwrap().count();
    let retained_before = runtime_quarantine_status().current_thread_retained();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let control = editor.hold_next_document_open(editor::OpenTestPhase::AfterPreparation);
    let request = begin_open(&mut editor, SCENE);
    assert!(control.wait_until_started());
    let pending = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        pending.document_open().pending(),
        Some((request, SCENE, EditorDocumentOpenPhase::Preparing))
    );
    assert!(pending.document_open().result().is_none());
    drop(editor);
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        retained_before + 1
    );
    for _ in 0..3 {
        assert_eq!(
            drive_runtime_quarantine().current_thread_retained(),
            retained_before + 1
        );
    }
    assert_eq!(fs::read(&path).unwrap(), source);
    control.release();
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime_quarantine_status().current_thread_retained() > retained_before {
        let _ = drive_runtime_quarantine();
        assert!(
            Instant::now() < deadline,
            "the held Browser owner did not physically retire"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        retained_before
    );
    assert_eq!(fs::read(&path).unwrap(), source);
    assert_eq!(
        fs::read_dir(path.parent().unwrap()).unwrap().count(),
        entries
    );
}

#[test]
fn session_close_and_drop_keep_a_held_source_scan_owned_until_physical_return() {
    use crate::project_host::authoring::ScanTestPhase;

    for (drop_session, phase) in [
        (false, ScanTestPhase::BeforeBegin),
        (true, ScanTestPhase::AfterPreparation),
    ] {
        let project = project("browser-scan-session-retirement");
        let mut editor = open_generic_editor_with_ready_index(&project);
        let original = editor.project_source_index_snapshot().unwrap();
        assert!(original.entry("scripts/later.rs").is_none());
        fs::write(project.root.join("scripts/later.rs"), b"fn later() {}\n").unwrap();
        let retained_before = runtime_quarantine_status().current_thread_retained();
        let control = editor.hold_next_source_index_scan(phase);
        assert_eq!(
            browser_action(&mut editor, EditorProjectBrowserAction::Refresh),
            EditorProjectBrowserActionOutcome::RefreshRequested
        );
        editor.drive_editor_frame(Duration::ZERO);
        assert!(control.wait_until_started());
        for _ in 0..3 {
            editor.drive_editor_frame(Duration::ZERO);
            assert!(editor.project_source_index_work_pending());
            assert!(Arc::ptr_eq(
                &original,
                &editor.project_source_index_snapshot().unwrap()
            ));
        }

        if drop_session {
            drop(editor);
            assert_eq!(
                runtime_quarantine_status().current_thread_retained(),
                retained_before + 1
            );
            for _ in 0..3 {
                assert_eq!(
                    drive_runtime_quarantine().current_thread_retained(),
                    retained_before + 1
                );
            }
            control.release();
            let deadline = Instant::now() + Duration::from_secs(5);
            while runtime_quarantine_status().current_thread_retained() > retained_before {
                let _ = drive_runtime_quarantine();
                assert!(
                    Instant::now() < deadline,
                    "the session's source scan did not physically retire"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(
                runtime_quarantine_status().current_thread_retained(),
                retained_before
            );
        } else {
            for _ in 0..3 {
                assert!(matches!(
                    editor.advance_close(),
                    editor::EditorProjectCloseProgress::Incomplete { .. }
                ));
                assert!(!editor.request_project_source_index_refresh());
                editor.drive_editor_frame(Duration::ZERO);
                assert!(editor.project_source_index_work_pending());
                assert!(Arc::ptr_eq(
                    &original,
                    &editor.project_source_index_snapshot().unwrap()
                ));
            }
            control.release();
            close(&mut editor);
            let closed = editor.drive_editor_frame(Duration::ZERO);
            assert!(!closed.project_browser().has_pending_work());
            assert!(!editor.project_source_index_work_pending());
            assert!(!editor.request_project_source_index_refresh());
            assert!(Arc::ptr_eq(
                &original,
                &editor.project_source_index_snapshot().unwrap()
            ));
        }
        assert!(original.entry("scripts/later.rs").is_none());
        assert_eq!(
            fs::read(project.root.join("scripts/later.rs")).unwrap(),
            b"fn later() {}\n"
        );
    }
}

fn project(name: &str) -> TestProject {
    let project = TestProject::new(name);
    fs::create_dir_all(project.root.join("scripts")).unwrap();
    for (path, name) in [
        ("scenes/startup.scene.json", "startup"),
        (SCENE, "external"),
    ] {
        fs::write(
            project.root.join(path),
            editable_authoring_scene(name).to_json_string().unwrap(),
        )
        .unwrap();
    }
    fs::write(
        project.root.join(PREFAB),
        editable_authoring_prefab("prefab")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    project
}

fn browser_action(
    editor: &mut editor::EditorProjectSession,
    action: EditorProjectBrowserAction,
) -> EditorProjectBrowserActionOutcome {
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::ProjectBrowser(action)],
        ))
        .unwrap();
    match report.outcomes() {
        [EditorActionOutcome::ProjectBrowser(outcome)] => *outcome,
        other => panic!("the Browser action did not produce its typed result: {other:?}"),
    }
}

fn begin_open(editor: &mut editor::EditorProjectSession, path: &str) -> EditorDocumentOpenId {
    match browser_action(
        editor,
        EditorProjectBrowserAction::Open {
            relative_path: Arc::from(path),
        },
    ) {
        EditorProjectBrowserActionOutcome::Preparing { request } => request,
        other => panic!("Browser Open must expose its pending request: {other:?}"),
    }
}

fn refresh_browser(editor: &mut editor::EditorProjectSession) {
    assert_eq!(
        browser_action(editor, EditorProjectBrowserAction::Refresh),
        EditorProjectBrowserActionOutcome::RefreshRequested
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.project_source_index_work_pending() {
        editor.drive_editor_frame(Duration::ZERO);
        assert!(
            Instant::now() < deadline,
            "Refresh must progress while Open is pending"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn workspace_action(
    editor: &mut editor::EditorProjectSession,
    action: EditorWorkspaceIntentAction,
) -> EditorWorkspaceIntentRequestResult {
    let acknowledgement = matches!(action, EditorWorkspaceIntentAction::AcknowledgeResult);
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(action)],
        ))
        .unwrap();
    match (acknowledgement, report.outcomes()) {
        (
            true,
            [
                EditorActionOutcome::Acknowledged(
                    nara_tooling::EditorActionAcknowledgement::WorkspaceIntent(true),
                ),
            ],
        ) => EditorWorkspaceIntentRequestResult::Accepted,
        (false, [EditorActionOutcome::WorkspaceIntent(outcome)]) => *outcome,
        other => panic!("the close decision did not produce its typed result: {other:?}"),
    }
}

fn wait_close_prompt(
    editor: &mut editor::EditorProjectSession,
    request: EditorDocumentOpenId,
    document: EditorDocumentId,
) -> Arc<EditorFrameModel> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        assert!(
            frame.document_open().result().is_none(),
            "preparation failed before the dirty prompt: {:?}",
            frame.document_open()
        );
        if frame.document_open().pending()
            == Some((
                request,
                SCENE,
                EditorDocumentOpenPhase::AwaitingClose { document },
            ))
        {
            assert_eq!(
                frame.workspace_intent().phase(),
                Some(EditorWorkspaceIntentPhase::AwaitingDecision)
            );
            assert_eq!(frame.workspace_intent().decision_document(), Some(document));
            assert!(frame.workspace_intent().decision_prompt().is_some());
            return frame;
        }
        assert!(
            Instant::now() < deadline,
            "prepared replacement did not reach its dirty close prompt"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn wait_open(
    editor: &mut editor::EditorProjectSession,
    request: EditorDocumentOpenId,
) -> Arc<EditorFrameModel> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        if let Some(result) = frame.document_open().result() {
            assert_eq!(result.request(), request);
            assert!(frame.document_open().pending().is_none());
            return frame;
        }
        assert!(
            Instant::now() < deadline,
            "Browser Open did not reach an observable terminal result: {:?}",
            frame.document_open()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn acknowledge_open(editor: &mut editor::EditorProjectSession, request: EditorDocumentOpenId) {
    assert_eq!(
        browser_action(
            editor,
            EditorProjectBrowserAction::AcknowledgeOpen { request }
        ),
        EditorProjectBrowserActionOutcome::OpenAcknowledged { request }
    );
    assert!(
        editor
            .drive_editor_frame(Duration::ZERO)
            .document_open()
            .result()
            .is_none()
    );
}

fn opened(frame: &EditorFrameModel, request: EditorDocumentOpenId) -> EditorDocumentId {
    match frame.document_open().result() {
        Some(EditorDocumentOpenResult::Opened {
            request: observed,
            document,
        }) if observed == request => document,
        other => panic!("Browser Open did not publish the expected document: {other:?}"),
    }
}

fn close(editor: &mut editor::EditorProjectSession) {
    assert!(matches!(
        drive_editor_close_to_completion(editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
}
