//! Retained live publication retries through the public Session, including actual disk effects.

use super::*;
use crate::project_host::runtime::editor::authored_upgrade::Work;
use crate::project_host::{
    AuthoringSourceOperationLimits,
    authoring::{
        AuthoredSourceUpgradePublication, AuthoredSourceUpgradePublicationFailure,
        AuthoredSourceUpgradePublicationPhase,
    },
    file_publication::{FilePublicationFailureStage, FilePublicationRejection},
};

fn limits() -> AuthoringSourceOperationLimits {
    AuthoringSourceOperationLimits::default()
        .with_open_handles(nara_core::ItemLimit::new(128).unwrap())
}

fn prepare(
    editor: &mut EditorProjectSession,
    limits: AuthoringSourceOperationLimits,
) -> EditorAuthoredSchemaUpgradeId {
    let operation = editor
        .begin_authored_schema_upgrade_with_limits(limits)
        .unwrap();
    awaiting(editor, operation);
    editor
        .prepare_authored_schema_upgrade(operation, proposal(editor, operation))
        .unwrap();
    reviewed(editor);
    operation
}

fn retained(editor: &EditorProjectSession) -> &AuthoredSourceUpgradePublication {
    editor.authored_schema_upgrade.publication.as_ref().unwrap()
}

// Park the actual move-only owner after a finite worker returns, before the next stage starts.
// All source preparation and publication still execute on the Session's existing Tasks.
fn returned_at(editor: &mut EditorProjectSession, phase: AuthoredSourceUpgradePublicationPhase) {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(Work::Publish(pending)) = editor.authored_schema_upgrade.pending.as_mut() {
            if let Some(result) = pending.poll() {
                let publication = result.expect("the actual publication worker must return");
                editor.authored_schema_upgrade.pending = None;
                let reached = publication.phase() == phase;
                assert!(
                    publication.failure().is_none(),
                    "{:?}",
                    publication.failure()
                );
                editor.authored_schema_upgrade.view.cancellation_allowed =
                    publication.cancellation_allowed();
                editor.authored_schema_upgrade.publication = Some(publication);
                if reached {
                    return;
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "the requested publisher must return"
        );
        if editor.authored_schema_upgrade.pending.is_none() {
            editor.drive_editor_frame(Duration::ZERO);
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn settled(editor: &mut EditorProjectSession) {
    wait(editor, |editor| {
        !editor.authored_schema_upgrade_view().work_pending
    });
}

fn refuse_ready_binding(editor: &mut EditorProjectSession, fixture: &Fixture) -> Vec<u8> {
    returned_at(editor, AuthoredSourceUpgradePublicationPhase::Ready);
    assert!(!editor.supervised_play_retirement_pending());
    assert!(
        !editor
            .source_tools
            .physical_work_pending(editor.authoring_runtime.host_custody())
    );
    // A borrowed Host also blocks the prerequisite retirement check. Change the actual
    // successor only after its Task returns so the final persistence binding refuses it.
    let successor = fixture.bytes(STARTUP);
    let mut changed = successor.clone();
    changed.push(b'\n');
    fs::write(fixture.root.join(STARTUP), changed).unwrap();
    settled(editor);
    assert_eq!(
        editor.authored_schema_upgrade_view().phase,
        EditorAuthoredSchemaUpgradePhase::RequiresReconcile
    );
    assert_eq!(
        retained(editor).phase(),
        AuthoredSourceUpgradePublicationPhase::Ready
    );
    assert!(matches!(
        retained(editor).failure(),
        Some(AuthoredSourceUpgradePublicationFailure::Persistence(
            EditorPersistenceRejection::TargetChanged
        ))
    ));
    assert!(editor.authored_schema_upgrade_view().can_retry());
    assert_pending(fixture);
    successor
}

fn assert_pending(fixture: &Fixture) {
    assert!(matches!(
        nara_project::ProjectManifest::parse_toml_bytes(&fixture.bytes("nara.toml"))
            .manifest
            .unwrap()
            .authoring
            .state,
        nara_project::ProjectAuthoringState::Pending(_)
    ));
}

fn assert_adopted(editor: &EditorProjectSession, fixture: &Fixture) {
    let view = editor.authored_schema_upgrade_view();
    assert_eq!(
        view.outcome,
        Some(EditorAuthoredSchemaUpgradeOutcome::Adopted),
        "{view:?}"
    );
    assert!(!view.writes_fenced && !view.work_pending && !view.can_retry());
    assert!(matches!(
        nara_project::ProjectManifest::parse_toml_bytes(&fixture.bytes("nara.toml"))
            .manifest
            .unwrap()
            .authoring
            .state,
        nara_project::ProjectAuthoringState::Ready
    ));
}

// Windows supplies the successful identity-bound native temporary unlink control.
#[cfg(windows)]
#[test]
fn source_cleanup_retry_adopts_the_reviewed_dirty_and_closed_sources_and_new_history() {
    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let startup = editor.workspace().active_document().unwrap();
        let inactive = open(editor, INACTIVE, AuthoringDocumentKind::Scene);
        let prefab = open(editor, PREFAB, AuthoringDocumentKind::Prefab);
        let documents = [startup, inactive, prefab];
        for document in documents {
            edit(editor, document, "reviewed-before-retry");
        }
        let before = documents.map(|document| state(editor, document));
        let original = fixture.disk();
        let operation = prepare(editor, limits());
        let expected: Vec<_> = editor
            .authored_schema_upgrade_review(operation)
            .unwrap()
            .sources()
            .map(|source| (source.project_path.to_owned(), source.successor.to_vec()))
            .collect();
        editor.confirm_authored_schema_upgrade(operation).unwrap();
        returned_at(editor, AuthoredSourceUpgradePublicationPhase::Sources);
        let cleanup = editor
            .authored_schema_upgrade
            .publication
            .as_mut()
            .unwrap()
            .test_fail_next_source_write();
        settled(editor);
        assert!(matches!(
            retained(editor).failure(),
            Some(AuthoredSourceUpgradePublicationFailure::Failed(
                FilePublicationFailureStage::WriteTemporary
            ))
        ));
        assert_pending(&fixture);
        assert!(retained(editor).test_cleanup_pending());
        assert!(editor.authored_schema_upgrade_view().can_retry());
        assert!(!editor.authored_schema_upgrade_view().can_cancel());
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(&fixture.disk()[1..], &original[1..]);
        let failed_disk = fixture.disk();
        let usage = retained(editor).test_usage();
        let source_work = retained(editor).work.usage();

        assert_eq!(
            editor.retry_authored_schema_upgrade(EditorAuthoredSchemaUpgradeId(operation.0 + 1)),
            Err(EditorAuthoredSchemaUpgradeError::Operation)
        );
        editor.retry_authored_schema_upgrade(operation).unwrap();
        assert!(!editor.authored_schema_upgrade_view().can_retry());
        assert!(editor.authored_schema_upgrade_view().writes_fenced);
        assert!(matches!(
            editor.retry_authored_schema_upgrade(operation),
            Err(EditorAuthoredSchemaUpgradeError::Busy)
        ));
        settled(editor);
        let after = retained(editor).test_usage();
        assert!(after.decode_work() >= usage.decode_work() + 1_024);
        assert_eq!(
            after.retained_candidate_bytes(),
            usage.retained_candidate_bytes()
        );
        assert_eq!(after.retained_open_handles(), usage.retained_open_handles());
        assert_eq!(after.peak_open_handles(), usage.peak_open_handles());
        assert_eq!(
            after.aggregate_encoded_bytes(),
            usage.aggregate_encoded_bytes()
        );
        assert_eq!(after.path_bytes(), usage.path_bytes());
        assert_eq!(retained(editor).work.usage(), source_work);
        assert_eq!(fixture.disk(), failed_disk);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert!(retained(editor).test_cleanup_pending());
        assert_writes_rejected(editor, startup);

        cleanup.release();
        editor.retry_authored_schema_upgrade(operation).unwrap();
        settled(editor);
        assert_adopted(editor, &fixture);
        for (path, bytes) in &expected {
            assert_eq!(&fixture.bytes(path), bytes, "{path}");
        }
        for document in documents {
            let after = state(editor, document);
            assert!(!after.dirty);
            assert_eq!(after.content, after.saved_content);
            assert_eq!(after.revision, after.saved_revision);
            assert_eq!(after.history, SceneAuthoringHistoryStatus::default());
            edit(editor, document, "ordinary-edit-after-retry");
            let undo = editor.apply_workspace_command(EditorWorkspaceCommand::Undo {
                document,
                base_revision: editor.document_query_basis(document).unwrap().revision,
            });
            assert!(undo.applied, "{undo:?}");
            assert_eq!(state(editor, document).encoded, after.encoded);
        }
        edit(editor, startup, "saved-through-retried-host");
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(startup)
            }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        wait(editor, |editor| {
            editor.persistence_view().result().is_some()
        });
        assert!(matches!(
            editor.persistence_view().result(),
            Some(EditorPersistenceResult::Saved { .. })
        ));
        assert!(
            String::from_utf8(fixture.bytes(STARTUP))
                .unwrap()
                .contains("saved-through-retried-host")
        );
    });
}

#[cfg(windows)]
fn failed_source(
    fixture: &Fixture,
    limits: AuthoringSourceOperationLimits,
) -> (
    EditorProjectSession,
    EditorAuthoredSchemaUpgradeId,
    crate::project_host::file_publication::CleanupTestControl,
) {
    let mut editor = fixture.open();
    wait(&mut editor, |editor| {
        !editor.project_source_index_work_pending()
    });
    let operation = prepare(&mut editor, limits);
    editor.confirm_authored_schema_upgrade(operation).unwrap();
    returned_at(&mut editor, AuthoredSourceUpgradePublicationPhase::Sources);
    let cleanup = editor
        .authored_schema_upgrade
        .publication
        .as_mut()
        .unwrap()
        .test_fail_next_source_write();
    settled(&mut editor);
    assert!(retained(&editor).test_cleanup_pending());
    assert_pending(fixture);
    (editor, operation, cleanup)
}

#[cfg(windows)]
#[test]
fn measured_cumulative_limit_refuses_released_cleanup_without_resetting_or_releasing_the_fence() {
    let measured_fixture = enemy_fixture();
    let (mut measured, operation, cleanup) = failed_source(&measured_fixture, limits());
    measured.retry_authored_schema_upgrade(operation).unwrap();
    settled(&mut measured);
    let maximum = retained(&measured).test_usage().decode_work();
    cleanup.release();
    measured.retry_authored_schema_upgrade(operation).unwrap();
    settled(&mut measured);
    assert_adopted(&measured, &measured_fixture);
    close(&mut measured);
    drop(measured);

    // Select the measured ceiling before capture. The first blocked attempt fits exactly.
    let fixture = enemy_fixture();
    let selected = limits().with_host_work(nara_core::ItemLimit::new(maximum).unwrap());
    let (mut editor, operation, cleanup) = failed_source(&fixture, selected);
    editor.retry_authored_schema_upgrade(operation).unwrap();
    settled(&mut editor);
    assert_eq!(retained(&editor).test_usage().decode_work(), maximum);
    let before = retained(&editor).test_usage();
    let source_work = retained(&editor).work.usage();
    let document = editor.workspace().active_document().unwrap();
    let source = state(&editor, document);
    let disk = fixture.disk();
    cleanup.release();
    editor.retry_authored_schema_upgrade(operation).unwrap();
    settled(&mut editor);
    assert_eq!(editor.authored_schema_upgrade_view().limits, selected);
    assert_eq!(
        editor.authored_schema_upgrade_view().refusal,
        Some(EditorAuthoredSchemaUpgradeError::WorkLimit)
    );
    assert!(!editor.authored_schema_upgrade_view().can_retry());
    assert!(editor.authored_schema_upgrade_view().writes_fenced);
    assert!(!editor.authored_schema_upgrade_view().can_cancel());
    assert!(retained(&editor).test_cleanup_pending());
    let refused = retained(&editor).test_usage();
    assert!(refused.decode_work() > maximum);
    assert_eq!(
        refused.retained_candidate_bytes(),
        before.retained_candidate_bytes()
    );
    assert_eq!(
        refused.retained_open_handles(),
        before.retained_open_handles()
    );
    assert_eq!(refused.peak_open_handles(), before.peak_open_handles());
    assert_eq!(
        refused.aggregate_encoded_bytes(),
        before.aggregate_encoded_bytes()
    );
    assert_eq!(refused.path_bytes(), before.path_bytes());
    assert_eq!(retained(&editor).work.usage(), source_work);
    assert_eq!(
        editor.retry_authored_schema_upgrade(operation),
        Err(EditorAuthoredSchemaUpgradeError::WorkLimit)
    );
    assert_eq!(retained(&editor).test_usage(), refused);
    assert_eq!(fixture.disk(), disk);
    assert_eq!(state(&editor, document), source);
    assert_pending(&fixture);
    assert_writes_rejected(&mut editor, document);

    // Fixture teardown only: the public retry above could not consume even released cleanup.
    assert!(
        editor
            .authored_schema_upgrade
            .publication
            .as_mut()
            .unwrap()
            .test_discard_source_cleanup()
    );
    editor
        .authored_schema_upgrade
        .finish(EditorAuthoredSchemaUpgradeOutcome::Failed(
            EditorAuthoredSchemaUpgradeError::WorkLimit,
        ));
    close(&mut editor);
}

#[test]
fn ready_guard_refusal_retries_the_same_candidate_and_close_waits_for_its_actual_adoption() {
    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        edit(editor, document, "retained-through-final-guard-refusal");
        let before = state(editor, document);
        let operation = prepare(editor, limits());
        editor.confirm_authored_schema_upgrade(operation).unwrap();
        let successor = refuse_ready_binding(editor, &fixture);
        assert_eq!(state(editor, document), before);
        let disk = fixture.disk();
        let candidate = retained(editor)
            .sources()
            .workspace()
            .scene(document)
            .unwrap();
        let prepared_state = (candidate.revision(), candidate.content_state());
        editor.retry_authored_schema_upgrade(operation).unwrap();
        settled(editor);
        let candidate = retained(editor)
            .sources()
            .workspace()
            .scene(document)
            .unwrap();
        assert_eq!(
            (candidate.revision(), candidate.content_state()),
            prepared_state
        );
        assert_eq!(fixture.disk(), disk);
        assert_eq!(state(editor, document), before);
        assert!(editor.authored_schema_upgrade_view().can_retry());
        assert!(matches!(
            retained(editor).failure(),
            Some(AuthoredSourceUpgradePublicationFailure::Persistence(
                EditorPersistenceRejection::TargetChanged
            ))
        ));

        assert!(matches!(
            editor.advance_close(),
            EditorProjectCloseProgress::Incomplete { .. }
        ));
        assert!(editor.authored_schema_upgrade_view().writes_fenced);
        assert!(editor.authored_schema_upgrade_view().can_retry());
        assert_eq!(
            editor.begin_authored_schema_upgrade(),
            Err(EditorAuthoredSchemaUpgradeError::Closing)
        );
        fs::write(fixture.root.join(STARTUP), &successor).unwrap();
        editor.retry_authored_schema_upgrade(operation).unwrap();
        close(editor);
        assert_adopted(editor, &fixture);
        assert_eq!(fixture.bytes(STARTUP), successor);
        let after = state(editor, document);
        assert_eq!((after.revision, after.content), prepared_state);
        assert!(!after.dirty);
        assert_eq!(after.history, SceneAuthoringHistoryStatus::default());
        assert!(
            String::from_utf8(fixture.bytes(STARTUP))
                .unwrap()
                .contains("milli")
        );
        assert_eq!(
            editor.retry_authored_schema_upgrade(operation),
            Err(EditorAuthoredSchemaUpgradeError::Closing)
        );
    });
}

#[test]
fn missing_returned_owner_and_reconcile_rejection_refuse_retry_and_keep_pending_writes_fenced() {
    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        let before = state(editor, document);
        let operation = prepare(editor, limits());
        editor.confirm_authored_schema_upgrade(operation).unwrap();
        let successor = refuse_ready_binding(editor, &fixture);
        let disk = fixture.disk();

        // Simulate an unobserved return while retaining the fixture owner solely for teardown.
        let mut owner = editor.authored_schema_upgrade.publication.take().unwrap();
        let actual_refusal = match owner.failure() {
            Some(AuthoredSourceUpgradePublicationFailure::Persistence(reason)) => *reason,
            other => panic!("the actual final binding must have refused: {other:?}"),
        };
        editor.authored_schema_upgrade.view.retry_allowed = false;
        assert_eq!(
            editor.retry_authored_schema_upgrade(operation),
            Err(EditorAuthoredSchemaUpgradeError::Task)
        );
        assert!(editor.authored_schema_upgrade_view().writes_fenced);
        assert!(!editor.authored_schema_upgrade_view().can_retry());
        assert_eq!(fixture.disk(), disk);
        assert_writes_rejected(editor, document);

        owner.record_failure(AuthoredSourceUpgradePublicationFailure::Rejected(
            FilePublicationRejection::RequiresReconcile,
        ));
        editor.authored_schema_upgrade.publication = Some(owner);
        assert_eq!(
            editor.retry_authored_schema_upgrade(operation),
            Err(EditorAuthoredSchemaUpgradeError::Publication)
        );
        assert!(editor.authored_schema_upgrade_view().writes_fenced);
        assert!(!editor.authored_schema_upgrade_view().can_retry());
        assert_eq!(state(editor, document), before);
        assert_eq!(fixture.disk(), disk);
        assert_writes_rejected(editor, document);

        // Restore the actual refusal and successor bytes; retry consumes the same candidate.
        editor
            .authored_schema_upgrade
            .publication
            .as_mut()
            .unwrap()
            .record_failure(AuthoredSourceUpgradePublicationFailure::Persistence(
                actual_refusal,
            ));
        fs::write(fixture.root.join(STARTUP), &successor).unwrap();
        editor.retry_authored_schema_upgrade(operation).unwrap();
        settled(editor);
        assert_adopted(editor, &fixture);
        assert_eq!(fixture.bytes(STARTUP), successor);
    });
}
