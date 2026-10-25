use super::*;
use nara_tooling::{EditorAction, EditorActionBatch, EditorActionOutcome, EditorPersistenceAction};

fn clean_save_attempt(editor: &mut EditorProjectSession) -> EditorPersistenceAttemptId {
    match editor.request_persistence(EditorPersistenceCommand::Save { document: None }) {
        EditorPersistenceRequestResult::Rejected {
            attempt,
            reason: EditorPersistenceRejection::NotDirty,
        } => attempt,
        other => panic!("expected a published clean Save rejection, got {other:?}"),
    }
}

#[test]
fn an_old_observer_cannot_acknowledge_a_successor_persistence_result() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor();
    let first = clean_save_attempt(&mut editor);
    let observed = editor.drive_editor_frame(Duration::ZERO);
    let delayed = EditorActionBatch::new(
        observed.revision(),
        [EditorAction::Persistence(
            EditorPersistenceAction::AcknowledgeResult { attempt: first },
        )],
    );
    acknowledge(&mut editor);
    let second = clean_save_attempt(&mut editor);
    let successor = editor.persistence_view();
    assert!(successor.result().is_some());
    editor.drive_editor_frame(Duration::ZERO);
    let report = editor.dispatch_editor_actions(delayed).unwrap();
    let after_delayed = editor.persistence_view();
    if after_delayed.result().is_some() {
        acknowledge(&mut editor);
    }
    close(&mut editor);
    assert_ne!(first, second);
    assert_eq!(after_delayed, successor);
    assert!(matches!(
        report.outcomes(),
        [EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::ResultMismatch)
        )]
    ));
}

#[test]
fn identical_retries_foreign_attempts_and_duplicate_acknowledgments_do_not_alias() {
    let fixture = Fixture::new();
    let other_fixture = Fixture::new();
    let mut editor = fixture.editor();
    let mut other = other_fixture.editor();
    let first = clean_save_attempt(&mut editor);
    let first_payload = editor.persistence_view().result();
    let second = clean_save_attempt(&mut editor);
    let successor = editor.persistence_view();
    let foreign = clean_save_attempt(&mut other);
    let mut refusals = Vec::new();
    for attempt in [first, foreign] {
        refusals.push((
            editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }),
            editor.persistence_view(),
        ));
    }
    let accepted =
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt: second });
    let duplicate =
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt: second });
    let idle = editor.persistence_view();
    acknowledge(&mut other);
    close(&mut other);
    close(&mut editor);
    assert_ne!(first, second);
    assert_ne!(first, foreign);
    assert_eq!(first.get(), foreign.get());
    assert_eq!(first_payload, successor.result());
    for (refusal, retained) in refusals {
        assert_eq!(
            refusal,
            EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::ResultMismatch)
        );
        assert_eq!(retained, successor);
    }
    assert_eq!(
        accepted,
        EditorPersistenceRequestResult::Acknowledged { attempt: second }
    );
    assert_eq!(
        duplicate,
        EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::ResultMismatch)
    );
    assert_eq!(idle, EditorPersistenceView::Idle);
}

#[test]
fn an_older_frame_can_acknowledge_the_exact_still_current_terminal() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor();
    let attempt = clean_save_attempt(&mut editor);
    let observed = editor.drive_editor_frame(Duration::ZERO);
    let newer = editor.drive_editor_frame(Duration::ZERO);
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            observed.revision(),
            [EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult { attempt },
            )],
        ))
        .unwrap();
    let allocated = editor.next_persistence_attempt;
    let idle = editor.persistence_view();
    close(&mut editor);
    assert_ne!(observed.revision(), newer.revision());
    assert!(
        matches!(report.outcomes(), [EditorActionOutcome::Persistence(
        EditorPersistenceRequestResult::Acknowledged { attempt: actual }
    )] if *actual == attempt)
    );
    assert_eq!(allocated, attempt.get());
    assert_eq!(idle, EditorPersistenceView::Idle);
}

#[test]
fn held_save_and_reopen_keep_the_admitted_attempt_until_physical_return() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor();
    let document = editor.workspace.active_document().unwrap();
    edit(&mut editor, document, "attempt-capture");
    let revision = editor.workspace.document_revision(document).unwrap();
    let gate = editor.hold_next_save(SaveTestPhase::BeforePublication);
    let EditorPersistenceRequestResult::Accepted { attempt: save } =
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document),
        })
    else {
        panic!("dirty Save must be admitted")
    };
    let saving = editor.persistence_view();
    editor.advance_persistence();
    assert!(gate.wait_until_started());
    let busy = editor.request_persistence(EditorPersistenceCommand::Save {
        document: Some(document),
    });
    let held_save = editor.persistence_view();
    gate.release();
    let saved = settle(&mut editor);
    let save_terminal = editor.persistence_view();
    acknowledge(&mut editor);
    let gate = editor.hold_next_reopen(ReopenTestPhase::AfterPreparation);
    let EditorPersistenceRequestResult::Accepted { attempt: reopen } =
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document),
        })
    else {
        panic!("Reopen must be admitted")
    };
    let opening = editor.persistence_view();
    editor.advance_persistence();
    assert!(gate.wait_until_started());
    let pending_ack =
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt: save });
    let held_reopen = editor.persistence_view();
    gate.release();
    let opened = settle(&mut editor);
    let reopen_terminal = editor.persistence_view();
    let stale_ack =
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt: save });
    let retained = editor.persistence_view();
    acknowledge(&mut editor);
    close(&mut editor);
    assert_eq!(
        saving,
        EditorPersistenceView::Pending {
            attempt: save,
            operation: EditorPersistenceOperation::Saving {
                document,
                captured_revision: revision
            }
        }
    );
    assert_eq!(held_save, saving);
    assert_eq!(
        busy,
        EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy)
    );
    assert!(
        matches!(saved, EditorPersistenceResult::Saved { document: actual, revision: captured, .. } if actual == document && captured == revision)
    );
    assert_eq!(
        save_terminal,
        EditorPersistenceView::Terminal {
            attempt: save,
            result: saved
        }
    );
    assert_ne!(save, reopen);
    assert_eq!(
        opening,
        EditorPersistenceView::Pending {
            attempt: reopen,
            operation: EditorPersistenceOperation::Opening { document }
        }
    );
    assert_eq!(held_reopen, opening);
    assert_eq!(
        pending_ack,
        EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy)
    );
    assert!(
        matches!(opened, EditorPersistenceResult::Opened { document: actual, .. } if actual == document)
    );
    assert_eq!(
        reopen_terminal,
        EditorPersistenceView::Terminal {
            attempt: reopen,
            result: opened
        }
    );
    assert_eq!(
        stale_ack,
        EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::ResultMismatch)
    );
    assert_eq!(retained, reopen_terminal);
}

#[test]
fn targetless_requests_preserve_saved_results_and_publish_distinct_rejections_only_after_consumption()
 {
    let fixture = Fixture::new();
    let mut editor = fixture.editor();
    let document = editor.workspace.active_document().unwrap();
    edit(&mut editor, document, "saved-before-close");
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    settle(&mut editor);
    let saved = editor.persistence_view();
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while !editor.workspace.is_empty() {
        editor.drive_editor_frame(Duration::ZERO);
        assert!(Instant::now() < deadline);
    }
    let refused = editor.request_persistence(EditorPersistenceCommand::Save { document: None });
    let retained = editor.persistence_view();
    acknowledge(&mut editor);
    let empty = editor.request_persistence(EditorPersistenceCommand::Save { document: None });
    let empty_terminal = editor.persistence_view();
    let active = fixture.add(&mut editor, EditorDocumentKind::Scene, "retry.scene.json");
    let active_attempt = clean_save_attempt(&mut editor);
    let active_result = editor.persistence_view().result();
    acknowledge(&mut editor);
    close(&mut editor);
    assert_eq!(
        refused,
        EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::ResultPending)
    );
    assert_eq!(retained, saved);
    let EditorPersistenceRequestResult::Rejected {
        attempt,
        reason: EditorPersistenceRejection::NoActiveDocument,
    } = empty
    else {
        panic!("empty slot must publish its own missing-target attempt")
    };
    assert_eq!(
        empty_terminal,
        EditorPersistenceView::Terminal {
            attempt,
            result: EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: None,
                reason: EditorPersistenceRejection::NoActiveDocument
            }
        }
    );
    assert_ne!(active_attempt, attempt);
    assert!(
        matches!(active_result, Some(EditorPersistenceResult::Rejected { document: Some(actual), reason: EditorPersistenceRejection::NotDirty, .. }) if actual == active)
    );
}

#[test]
fn exhausted_attempt_ids_refuse_without_overwriting_retained_observation() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor();
    let attempt = clean_save_attempt(&mut editor);
    let terminal = editor.persistence_view();
    editor.next_persistence_attempt = u64::MAX;
    let refused = editor.request_persistence(EditorPersistenceCommand::Save { document: None });
    let retained = editor.persistence_view();
    let ack = editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt });
    let exhausted_idle =
        editor.request_persistence(EditorPersistenceCommand::Save { document: None });
    let idle = editor.persistence_view();
    close(&mut editor);
    assert_eq!(
        refused,
        EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::AttemptIdExhausted)
    );
    assert_eq!(retained, terminal);
    assert_eq!(
        ack,
        EditorPersistenceRequestResult::Acknowledged { attempt }
    );
    assert_eq!(exhausted_idle, refused);
    assert_eq!(idle, EditorPersistenceView::Idle);
}

#[test]
fn workspace_close_retains_but_exit_consumes_only_its_own_saved_attempt() {
    for exit in [false, true] {
        let fixture = Fixture::new();
        let mut editor = fixture.editor();
        let document = editor.workspace.active_document().unwrap();
        edit(&mut editor, document, "intent-save");
        let intent = if exit {
            EditorWorkspaceIntent::Exit
        } else {
            EditorWorkspaceIntent::CloseDocument { document }
        };
        assert_eq!(
            editor.request_workspace_intent(intent),
            EditorWorkspaceIntentRequestResult::Accepted
        );
        let prompt = editor
            .pending_workspace_intent
            .as_ref()
            .unwrap()
            .prompt()
            .unwrap();
        let gate = editor.hold_next_save(SaveTestPhase::BeforePublication);
        assert_eq!(
            editor.resolve_workspace_intent(prompt, EditorCloseDecision::Save),
            EditorWorkspaceIntentRequestResult::Accepted
        );
        let attempt = editor.persistence_view().attempt().unwrap();
        let awaited = editor
            .pending_workspace_intent
            .as_ref()
            .unwrap()
            .save_attempt();
        editor.advance_persistence();
        assert!(gate.wait_until_started());
        gate.release();
        let deadline = Instant::now() + Duration::from_secs(5);
        while editor
            .pending_workspace_intent
            .as_ref()
            .is_some_and(|pending| pending.phase() == EditorWorkspaceIntentPhase::Saving)
        {
            editor.drive_editor_frame(Duration::ZERO);
            assert!(Instant::now() < deadline);
        }
        let terminal = editor.persistence_view();
        if !exit {
            acknowledge(&mut editor);
        }
        close(&mut editor);
        assert_eq!(awaited, Some(attempt));
        if exit {
            assert_eq!(terminal, EditorPersistenceView::Idle);
        } else {
            assert!(
                matches!(terminal, EditorPersistenceView::Terminal { attempt: actual, result: EditorPersistenceResult::Saved { document: saved, .. } } if actual == attempt && saved == document)
            );
        }
    }
}

#[test]
fn two_document_exit_consumes_each_attempt_without_aliasing_the_next_terminal() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor();
    let scene = editor.workspace.active_document().unwrap();
    let prefab = fixture.add(
        &mut editor,
        EditorDocumentKind::Prefab,
        "exit-second.prefab.json",
    );
    edit(&mut editor, scene, "exit-first");
    edit(&mut editor, prefab, "exit-second");
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::Exit),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    let pending = editor.pending_workspace_intent.as_ref().unwrap();
    let first_document = pending.decision_document().unwrap();
    let first_prompt = pending.prompt().unwrap();
    let gate = editor.hold_next_save(SaveTestPhase::BeforePublication);
    assert_eq!(
        editor.resolve_workspace_intent(first_prompt, EditorCloseDecision::Save),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    let first_attempt = editor.persistence_view().attempt().unwrap();
    editor.advance_persistence();
    assert!(gate.wait_until_started());
    gate.release();
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor
        .pending_workspace_intent
        .as_ref()
        .is_some_and(|pending| pending.phase() == EditorWorkspaceIntentPhase::Saving)
    {
        editor.drive_editor_frame(Duration::ZERO);
        assert!(Instant::now() < deadline);
    }
    let first_consumed = editor.persistence_view();
    let pending = editor.pending_workspace_intent.as_ref().unwrap();
    let second_document = pending.decision_document().unwrap();
    let second_prompt = pending.prompt().unwrap();
    let gate = editor.hold_next_save(SaveTestPhase::BeforePublication);
    assert_eq!(
        editor.resolve_workspace_intent(second_prompt, EditorCloseDecision::Save),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    let second_attempt = editor.persistence_view().attempt().unwrap();
    let awaited = editor
        .pending_workspace_intent
        .as_ref()
        .unwrap()
        .save_attempt();
    editor.advance_persistence();
    assert!(gate.wait_until_started());
    gate.release();
    // Pause between the concrete owner's publication and its ordinary drive's intent progression.
    // Public frames normally observe only after the complete drive cut.
    let second_result = settle(&mut editor);
    let second_terminal = editor.persistence_view();
    let late = editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult {
        attempt: first_attempt,
    });
    let retained = editor.persistence_view();
    editor.advance_workspace_persistence();
    let second_consumed = editor.persistence_view();
    let exit_phase = editor.pending_workspace_intent.as_ref().unwrap().phase();
    let scene_bytes = fs::read_to_string(fixture.root.join("scenes/startup.scene.json")).unwrap();
    let prefab_bytes =
        fs::read_to_string(fixture.root.join("prefabs/exit-second.prefab.json")).unwrap();
    close(&mut editor);
    assert!(
        matches!((first_document, second_document), (a, b) if a != b && [scene, prefab].contains(&a) && [scene, prefab].contains(&b))
    );
    assert_ne!(first_prompt, second_prompt);
    assert_ne!(first_attempt, second_attempt);
    assert_eq!(first_consumed, EditorPersistenceView::Idle);
    assert_eq!(awaited, Some(second_attempt));
    assert!(
        matches!(second_result, EditorPersistenceResult::Saved { document, .. } if document == second_document)
    );
    assert_eq!(
        second_terminal,
        EditorPersistenceView::Terminal {
            attempt: second_attempt,
            result: second_result
        }
    );
    assert_eq!(
        late,
        EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::ResultMismatch)
    );
    assert_eq!(retained, second_terminal);
    assert_eq!(second_consumed, EditorPersistenceView::Idle);
    assert_eq!(exit_phase, EditorWorkspaceIntentPhase::RetiringRuntime);
    assert!(scene_bytes.contains("exit-first"));
    assert!(prefab_bytes.contains("exit-second"));
}
