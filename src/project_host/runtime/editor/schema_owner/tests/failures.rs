//! Terminal failure observations retain the original source owner's evidence.

use super::*;
use crate::project_host::{
    authoring::AuthoringSchemaOwnerPreparation,
    runtime::editor::EditorSourceToolInstallationInspection,
};

#[test]
fn malformed_unknown_record_retains_the_complete_workspace_preparation_failure_without_mutation() {
    let fixture = Fixture::new();
    let mut malformed = scene("unknown-invalid-health");
    malformed.entities[0]
        .components
        .get_mut(&component())
        .unwrap()
        .value = ComponentValue::map([("health", ComponentValue::String("not an integer".into()))]);
    fs::write(
        fixture.root.join(SECOND),
        malformed.to_json_string().unwrap(),
    )
    .unwrap();
    fixture.run(|editor| {
        let active = startup(editor);
        let document = second(editor);
        name_edit(editor, document, "retained-undo");
        name_edit(editor, document, "retained-redo");
        let undone = editor.apply_workspace_command(EditorWorkspaceCommand::Undo {
            document,
            base_revision: editor.document_query_basis(document).unwrap().revision,
        });
        assert!(undone.applied, "{undone:?}");
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument {
                    document: active,
                })
                .applied
        );
        let before = source_state(editor, document);
        let active_before = source_state(editor, active);
        let basis = editor.document_query_basis(document).unwrap();
        let active_basis = editor.document_query_basis(active).unwrap();
        assert_eq!(before.history.undo_depth, 1);
        assert_eq!(before.history.redo_depth, 1);
        assert!(before.dirty);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        let bundle = fixture.bytes(BUNDLE);
        let startup_bytes = fixture.bytes(STARTUP);
        let second_bytes = fixture.bytes(SECOND);

        // Obtain the expected refusal from the owning Workspace kernel and the real installed
        // declarations. This prepares metadata only; neither source nor bundle is published.
        let input = editor.authoring_project().unwrap().schema_owner_input();
        let installed = editor
            .source_project_root()
            .strict_read_only()
            .unwrap()
            .open_directory_exact(&RelativePath::new(INSTALLED).unwrap())
            .unwrap();
        let inspection =
            EditorSourceToolInstallationInspection::read(&installed, fixture.receipt).unwrap();
        let AuthoringSchemaOwnerPreparation::Prepared { owner, .. } = input
            .prepare_owner(inspection.release().schema_owner())
            .unwrap()
        else {
            panic!("the installed owner must still be absent from the project");
        };
        let expected = editor
            .workspace()
            .capture_schema_reclassification(
                input.schema_basis(),
                owner.schema_basis(),
                Default::default(),
            )
            .unwrap()
            .prepare()
            .unwrap_err();
        assert_eq!(expected.document, Some(document));
        assert_eq!(expected.active_document, Some(active));
        assert!(!expected.applied);
        let patch = expected.patch_report.as_ref().unwrap();
        assert!(patch.diagnostics.has_errors());
        assert_eq!(patch.diagnostics, expected.diagnostics);

        let operation = editor
            .prepare_installed_schema_owner(INSTALLED, fixture.receipt)
            .unwrap();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Failed(EditorSchemaOwnerAdoptionError::Preparation)
        );
        let failure = editor.schema_owner_adoption_failure().unwrap();
        assert_eq!(failure.operation(), Some(operation));
        assert_eq!(
            failure.reason(),
            EditorSchemaOwnerAdoptionError::Preparation
        );
        assert_eq!(failure.phase(), EditorSchemaOwnerAdoptionPhase::Preparing);
        assert_eq!(failure.document(), Some(document));
        assert_eq!(failure.source_report(), Some(&expected));
        assert_eq!(failure.diagnostics(), &expected.diagnostics);
        assert_eq!(
            failure.source_report().unwrap().patch_report.as_ref(),
            Some(patch)
        );
        assert!(failure.task_rejection().is_none());
        assert!(editor.schema_owner_adoption_publication().is_none());
        let after = source_state(editor, document);
        assert_eq!(after, before);
        assert!(Arc::ptr_eq(&after.document, &before.document));
        assert_eq!(source_state(editor, active), active_before);
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(editor.document_query_basis(active).unwrap(), active_basis);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        assert_eq!(fixture.bytes(BUNDLE), bundle);
        assert_eq!(fixture.bytes(STARTUP), startup_bytes);
        assert_eq!(fixture.bytes(SECOND), second_bytes);

        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            editor
                .schema_owner_adoption_failure()
                .unwrap()
                .source_report(),
            Some(&expected)
        );
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
        assert!(editor.schema_owner_adoption_failure().is_none());
    });
}

#[test]
fn saved_startup_drift_after_review_retains_the_original_operation_source_and_publishing_phase() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "dirty-source-must-survive");
        let before = source_state(editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let bundle = fixture.bytes(BUNDLE);
        let operation = fixture.review(editor);
        let changed = scene("external-saved-source")
            .to_json_string()
            .unwrap()
            .into_bytes();
        fs::write(fixture.root.join(STARTUP), &changed).unwrap();
        editor.adopt_schema_owner(operation).unwrap();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Failed(EditorSchemaOwnerAdoptionError::Stale)
        );
        let failure = editor.schema_owner_adoption_failure().unwrap();
        assert_eq!(failure.operation(), Some(operation));
        assert_eq!(failure.reason(), EditorSchemaOwnerAdoptionError::Stale);
        assert_eq!(failure.phase(), EditorSchemaOwnerAdoptionPhase::Publishing);
        assert_eq!(failure.relative_source(), Some(STARTUP));
        assert_eq!(failure.document(), None);
        assert!(failure.source_report().is_none());
        assert!(failure.task_rejection().is_none());
        let diagnostics: Vec<_> = failure.diagnostics().iter().collect();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code().as_str(),
            "project.editor.authoring-source-changed"
        );
        let retained = failure.diagnostics().clone();
        assert!(retained.has_errors());
        assert_eq!(source_state(editor, document), before);
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        assert_eq!(fixture.bytes(BUNDLE), bundle);
        assert_eq!(fixture.bytes(STARTUP), changed);
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert_eq!(
            editor.adopt_schema_owner(operation),
            Err(EditorSchemaOwnerAdoptionError::Operation)
        );
        editor.cancel_schema_owner_adoption(operation).unwrap();
        editor.drive_editor_frame(Duration::ZERO);
        let failure = editor.schema_owner_adoption_failure().unwrap();
        assert_eq!(failure.operation(), Some(operation));
        assert_eq!(failure.diagnostics(), &retained);
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
        assert!(editor.schema_owner_adoption_failure().is_none());
    });
}

#[test]
fn installed_payload_drift_retains_the_installation_path_and_safe_diagnostic_without_publication() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "retained-on-installed-payload-drift");
        let before = source_state(editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let bundle = fixture.bytes(BUNDLE);
        let source = fixture.bytes(STARTUP);
        let operation = fixture.review(editor);
        fs::write(
            fixture.root.join(INSTALLED).join("Tool.dll"),
            b"private-installation-payload-marker",
        )
        .unwrap();
        editor.adopt_schema_owner(operation).unwrap();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Failed(EditorSchemaOwnerAdoptionError::Installation)
        );
        let failure = editor.schema_owner_adoption_failure().unwrap();
        assert_eq!(failure.operation(), Some(operation));
        assert_eq!(
            failure.reason(),
            EditorSchemaOwnerAdoptionError::Installation
        );
        assert_eq!(failure.phase(), EditorSchemaOwnerAdoptionPhase::Publishing);
        assert_eq!(failure.relative_source(), Some(INSTALLED));
        assert!(failure.document().is_none());
        assert!(failure.source_report().is_none());
        assert!(failure.task_rejection().is_none());
        let diagnostics: Vec<_> = failure.diagnostics().iter().collect();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code().as_str(),
            "project.editor.schema-owner-installation-rejected"
        );
        assert_eq!(
            diagnostics[0].summary().as_str(),
            "The selected installation changed or could not be inspected"
        );
        assert!(diagnostics[0].fields().is_empty());
        assert!(failure.diagnostics().has_errors());
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert_eq!(fixture.bytes(BUNDLE), bundle);
        assert_eq!(fixture.bytes(STARTUP), source);
        assert_eq!(source_state(editor, document), before);
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
        assert!(editor.schema_owner_adoption_failure().is_none());
    });
}

#[test]
fn cancelling_held_failed_publication_preserves_the_actual_write_failure_after_worker_return() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "retained-cancelled-write-failure");
        let before = source_state(editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let bundle = fixture.bytes(BUNDLE);
        let source = fixture.bytes(STARTUP);
        let operation = fixture.review(editor);
        let publisher = editor
            .schema_owner_adoption
            .candidate
            .as_ref()
            .unwrap()
            .metadata
            .publisher
            .as_ref()
            .unwrap()
            .clone();
        let cleanup = ReleaseCleanup(
            publisher
                .try_with(|host| host.test_fail_after_write())
                .unwrap(),
        );
        let mut held = HeldPublication::arm(editor);
        editor.adopt_schema_owner(operation).unwrap();
        let returned = publication_returned(editor);
        held.wait_until_entered();
        held.assert_pending(editor, &returned);
        assert!(publisher.try_with(|host| host.has_cleanup()).unwrap());
        assert!(editor.schema_owner_adoption_failure().is_none());
        held.poll(|| editor.cancel_schema_owner_adoption(operation))
            .unwrap();
        held.poll(|| editor.drive_editor_frame(Duration::ZERO));
        held.assert_pending(editor, &returned);
        assert!(editor.schema_owner_adoption_failure().is_none());
        held.release();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Cancelled
        );
        assert_eq!(returned.state(), TaskReturnState::Returned);
        let failure = editor.schema_owner_adoption_failure().unwrap();
        assert_eq!(failure.operation(), Some(operation));
        assert_eq!(
            failure.reason(),
            EditorSchemaOwnerAdoptionError::Publication
        );
        assert_eq!(failure.phase(), EditorSchemaOwnerAdoptionPhase::Publishing);
        assert!(failure.document().is_none());
        assert!(failure.source_report().is_none());
        assert!(failure.task_rejection().is_none());
        let diagnostics: Vec<_> = failure.diagnostics().iter().collect();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].code().as_str(),
            "project.editor.schema-owner-temporary-write-failed"
        );
        let retained = failure.diagnostics().clone();
        assert!(retained.has_errors());
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert!(editor.schema_owner_adoption.pending());
        assert_eq!(
            editor.acknowledge_schema_owner_adoption(operation),
            Err(EditorSchemaOwnerAdoptionError::Operation)
        );
        editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            editor
                .schema_owner_adoption_failure()
                .unwrap()
                .diagnostics(),
            &retained
        );
        cleanup.0.release();
        wait(editor, |editor| !editor.schema_owner_adoption.pending());
        let failure = editor.schema_owner_adoption_failure().unwrap();
        assert_eq!(failure.operation(), Some(operation));
        assert_eq!(
            failure.reason(),
            EditorSchemaOwnerAdoptionError::Publication
        );
        assert_eq!(failure.diagnostics(), &retained);
        assert_eq!(
            editor.schema_owner_adoption_view().outcome,
            Some(EditorSchemaOwnerAdoptionOutcome::Cancelled)
        );
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert_eq!(source_state(editor, document), before);
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        assert_eq!(fixture.bytes(BUNDLE), bundle);
        assert_eq!(fixture.bytes(STARTUP), source);
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
        assert!(editor.schema_owner_adoption_failure().is_none());
    });
}

#[test]
fn cancelling_successfully_returned_preparation_does_not_invent_a_stale_failure() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "retain-cancelled-successful-preparation");
        let before = source_state(editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let bundle = fixture.bytes(BUNDLE);
        let source = fixture.bytes(STARTUP);
        let operation = editor
            .prepare_installed_schema_owner(INSTALLED, fixture.receipt)
            .unwrap();
        wait(editor, |editor| {
            matches!(
                editor.schema_owner_adoption_view().phase,
                EditorSchemaOwnerAdoptionPhase::Preparing
                    | EditorSchemaOwnerAdoptionPhase::Review
                    | EditorSchemaOwnerAdoptionPhase::Terminal
                    | EditorSchemaOwnerAdoptionPhase::RequiresReconcile
            )
        });
        assert_eq!(
            editor.schema_owner_adoption_view().phase,
            EditorSchemaOwnerAdoptionPhase::Preparing
        );
        let Some(super::super::Work::Prepare(pending)) = &editor.schema_owner_adoption.work else {
            panic!("the Session must retain its actual preparation worker");
        };
        let returned = pending.task.as_ref().unwrap().return_observer();
        // Observe physical return without another owner turn consuming the successful candidate.
        let deadline = Instant::now() + WAIT;
        while returned.state() == TaskReturnState::Pending {
            assert!(
                Instant::now() < deadline,
                "the actual preparation worker did not physically return"
            );
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(returned.state(), TaskReturnState::Returned);
        {
            let Ok(job) = pending.job.try_lock() else {
                panic!("the returned preparation must release its result mutex");
            };
            assert!(matches!(job.result.as_ref(), Some(Ok(_))));
        }
        assert!(editor.schema_owner_adoption_failure().is_none());
        editor.cancel_schema_owner_adoption(operation).unwrap();
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Cancelled
        );
        assert_eq!(returned.state(), TaskReturnState::Returned);
        assert!(editor.schema_owner_adoption_failure().is_none());
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert_eq!(source_state(editor, document), before);
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        assert_eq!(fixture.bytes(BUNDLE), bundle);
        assert_eq!(fixture.bytes(STARTUP), source);
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
        assert!(editor.schema_owner_adoption_failure().is_none());
    });
}

#[test]
fn cancelling_inspection_queued_before_start_does_not_invent_a_task_failure() {
    use nara_tasks::{TaskPoolConfig, TaskPools};

    use super::super::{Pending, Work, inspect};

    let fixture = Fixture::new();
    fixture.run(|editor| {
        let document = startup(editor);
        name_edit(editor, document, "retain-cancelled-queued-inspection");
        let before = source_state(editor, document);
        let basis = editor.document_query_basis(document).unwrap();
        let bundle = fixture.bytes(BUNDLE);
        let source = fixture.bytes(STARTUP);
        let operation = editor
            .prepare_installed_schema_owner(INSTALLED, fixture.receipt)
            .unwrap();
        let pools = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
        // Schedule the Session's actual request on the deterministic Tasks driver, leaving
        // inspection queued until cancellation has reached the admitted task.
        let start = editor.schema_owner_adoption.start.take().unwrap();
        let custody = editor.authoring_runtime.host_custody().clone();
        let pending = Pending::start(&pools, move || inspect(start, custody));
        assert!(pending.rejection.is_none());
        let returned = pending.task.as_ref().unwrap().return_observer();
        assert_eq!(returned.state(), TaskReturnState::Pending);
        assert!(editor.schema_owner_adoption.work.is_none());
        editor.schema_owner_adoption.work = Some(Work::Inspect(pending));
        editor.schema_owner_adoption.view.phase = EditorSchemaOwnerAdoptionPhase::Inspecting;

        editor.cancel_schema_owner_adoption(operation).unwrap();
        let executed = pools.run_pending_for_tests();
        assert_eq!(executed.executed, 0);
        assert_eq!(executed.cancelled_before_start, 1);
        assert_eq!(returned.state(), TaskReturnState::Returned);
        {
            let Some(Work::Inspect(pending)) = &editor.schema_owner_adoption.work else {
                panic!("the Session must retain its skipped inspection until owner observation");
            };
            let Ok(job) = pending.job.try_lock() else {
                panic!("the skipped inspection must release its result mutex");
            };
            assert!(!job.started);
            assert!(job.run.is_some());
            assert!(job.result.is_none());
        }
        assert_eq!(
            terminal(editor),
            EditorSchemaOwnerAdoptionOutcome::Cancelled
        );
        assert_eq!(
            editor.schema_owner_adoption_view().operation,
            Some(operation)
        );
        assert!(editor.schema_owner_adoption_failure().is_none());
        assert!(editor.schema_owner_adoption_publication().is_none());
        assert!(!editor.schema_owner_adoption.pending());
        let after = source_state(editor, document);
        assert_eq!(after, before);
        assert!(Arc::ptr_eq(&after.document, &before.document));
        assert_eq!(editor.document_query_basis(document).unwrap(), basis);
        assert_eq!(
            readiness(editor, document),
            AuthoringRecordReadiness::UnknownSchema
        );
        assert_eq!(fixture.bytes(BUNDLE), bundle);
        assert_eq!(fixture.bytes(STARTUP), source);
        editor.acknowledge_schema_owner_adoption(operation).unwrap();
        assert!(editor.schema_owner_adoption_failure().is_none());
        assert!(pools.shutdown_blocking().unwrap().is_complete());
    });
}
