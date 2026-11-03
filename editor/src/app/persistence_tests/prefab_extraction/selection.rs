//! Save selection correction and retirement through the real Application and Session owners.

use nara::{
    project_host::{AuthoringSourceOperationLimits, EditorPrefabExtractionError},
    scene::{SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin},
    tooling::{
        EditorDocumentKind, EditorPersistenceRejection, EditorPrefabExtractionDisposition,
        EditorWorkspaceIntentPhase, EditorWorkspaceIntentRequestResult,
    },
};

use super::*;

const SOURCE: &str = "worlds/entry.scene.json";
const Q: &str = "blueprints/extracted.prefab.json";
const Q_META: &str = "blueprints/extracted.prefab.json.meta";

fn wait_for_phase(
    application: &mut EditorApplication,
    operation: EditorPrefabExtractionId,
    phase: EditorPrefabExtractionPhase,
) {
    let deadline = Instant::now() + WAIT;
    loop {
        let view = application.prefab_extraction_view().unwrap();
        assert_eq!(view.operation, Some(operation));
        if view.phase == phase {
            assert!(!view.work_pending, "{view:?}");
            assert!(
                !application
                    .current_frame()
                    .unwrap()
                    .authoring_operation_work_pending()
            );
            return;
        }
        assert!(
            !matches!(
                view.phase,
                EditorPrefabExtractionPhase::Terminal
                    | EditorPrefabExtractionPhase::RequiresReconcile
            ) && Instant::now() < deadline,
            "expected {phase:?}, observed {view:?}; {:?}",
            application.prefab_extraction_diagnostics(),
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn selection_documents(
    application: &EditorApplication,
    operation: EditorPrefabExtractionId,
) -> Vec<(EditorDocumentId, SourceDocumentKey, String)> {
    application
        .prefab_extraction_selection(operation)
        .unwrap()
        .documents()
        .map(|document| {
            (
                document.document(),
                document.source_key().clone(),
                document.project_path().to_owned(),
            )
        })
        .collect()
}

fn reviewed_outputs(
    application: &EditorApplication,
    operation: EditorPrefabExtractionId,
) -> Vec<(String, Vec<u8>)> {
    application
        .prefab_extraction_review(operation)
        .unwrap()
        .outputs()
        .map(|output| (output.project_path().to_owned(), output.bytes().to_vec()))
        .collect()
}

#[test]
fn invalid_save_selection_keeps_correction_available_after_rejected_open_and_adopts_exact_outputs()
{
    let fixture = fixture();
    let original_disk = [SOURCE, PREFAB, "nara.toml"]
        .map(|path| (path, fs::read(fixture.root.join(path)).unwrap()));
    let mut application = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    settle_background_imports(&mut application);
    let startup = edit_save_source(&mut application);
    let operation = review(&mut application, Some(startup));
    let original_frame = application.current_frame().unwrap();
    let original_workspace = original_frame.workspace();
    assert_eq!(original_workspace.documents.len(), 2);
    let selected = original_workspace
        .documents
        .iter()
        .find(|tab| tab.kind == EditorDocumentKind::Prefab)
        .unwrap()
        .document;
    let startup_basis = application.document_query_basis(startup).unwrap();
    let selected_basis = application.document_query_basis(selected).unwrap();
    let options = selection_documents(&application, operation);
    assert_eq!(options.len(), original_workspace.documents.len());
    for (document, path, kind) in [
        (startup, SOURCE, SourceDocumentKind::Scene),
        (selected, PREFAB, SourceDocumentKind::Prefab),
    ] {
        let (_, key, actual_path) = options
            .iter()
            .find(|(candidate, _, _)| *candidate == document)
            .unwrap();
        assert_eq!(actual_path, path);
        assert_eq!(key.kind(), kind);
        assert!(matches!(
            key.origin(),
            SourceDocumentOrigin::ProjectLocation { path: source_path, .. }
                if source_path.as_str() == path
        ));
    }
    let original_outputs = reviewed_outputs(&application, operation);
    for path in [SOURCE, PREFAB, Q, Q_META] {
        assert!(original_outputs.iter().any(|(output, _)| output == path));
    }
    let foreign = EditorDocumentId::from_raw(u64::MAX);
    assert!(original_workspace.document(foreign).is_none());

    for invalid in [vec![startup, startup], vec![foreign]] {
        application
            .revise_prefab_extraction(operation, invalid)
            .unwrap();
        wait_for_phase(
            &mut application,
            operation,
            EditorPrefabExtractionPhase::AwaitingSelection,
        );
        let view = application.prefab_extraction_view().unwrap();
        assert!(view.writes_fenced && view.can_cancel() && !view.can_retry());
        assert_eq!(view.outcome, None);
        assert_eq!(view.refusal, Some(EditorPrefabExtractionError::Preparation));
        assert_eq!(view.limits, AuthoringSourceOperationLimits::default());
        assert!(
            application
                .prefab_extraction_diagnostics()
                .unwrap()
                .has_errors()
        );
        assert!(matches!(
            application.prefab_extraction_review(operation),
            Err(EditorPrefabExtractionError::Operation)
        ));
        assert_eq!(
            application.confirm_prefab_extraction(operation),
            Err(EditorPrefabExtractionError::Operation)
        );

        let frame = application.current_frame().unwrap();
        let dispatch = application.dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Save {
                document: startup,
                base_revision: startup_basis.revision,
            })],
        ));
        assert!(matches!(
            dispatch.feedback().unwrap().unwrap().outcomes(),
            [EditorActionOutcome::Persistence(
                EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy)
            )]
        ));
        assert!(
            !application.request_project_replacement(&fixture.root, Some("desktop".to_owned()))
        );
        let rejected = application.submit_project_action(EditorProjectAction::OpenProject {
            project_root: fixture.root.clone(),
            profile: Some("desktop".to_owned()),
        });
        assert!(!rejected.actions_observed() && !rejected.exit_ready());
        assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
        assert!(std::sync::Arc::ptr_eq(
            &frame,
            &application.current_frame().unwrap()
        ));
        assert_eq!(selection_documents(&application, operation), options);
        assert_eq!(
            application.document_query_basis(startup).unwrap(),
            startup_basis
        );
        assert_eq!(
            application.document_query_basis(selected).unwrap(),
            selected_basis
        );
        assert_eq!(
            application.current_frame().unwrap().workspace().documents,
            original_workspace.documents
        );
        for (path, bytes) in &original_disk {
            assert_eq!(&fs::read(fixture.root.join(path)).unwrap(), bytes, "{path}");
        }
        assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());
    }

    application
        .revise_prefab_extraction(operation, vec![startup])
        .unwrap();
    wait_for_phase(
        &mut application,
        operation,
        EditorPrefabExtractionPhase::Review,
    );
    let view = application.prefab_extraction_view().unwrap();
    assert!(view.writes_fenced && view.can_cancel());
    assert_eq!(view.refusal, None);
    assert_eq!(view.limits, AuthoringSourceOperationLimits::default());
    assert!(application.prefab_extraction_diagnostics().is_none());
    assert_eq!(selection_documents(&application, operation), options);
    let dispositions: Vec<_> = application
        .prefab_extraction_review(operation)
        .unwrap()
        .documents()
        .collect();
    assert_eq!(dispositions.len(), 2);
    assert!(dispositions.contains(&(startup, EditorPrefabExtractionDisposition::SaveCurrent)));
    assert!(dispositions.contains(&(selected, EditorPrefabExtractionDisposition::ReplaceSource)));
    let outputs = reviewed_outputs(&application, operation);
    assert_eq!(outputs, original_outputs);
    assert_eq!(
        application.current_frame().unwrap().workspace().documents,
        original_workspace.documents
    );
    for (path, bytes) in &original_disk {
        assert_eq!(&fs::read(fixture.root.join(path)).unwrap(), bytes, "{path}");
    }
    assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());
    application.confirm_prefab_extraction(operation).unwrap();
    wait_for_phase(
        &mut application,
        operation,
        EditorPrefabExtractionPhase::Terminal,
    );
    let view = application.prefab_extraction_view().unwrap();
    assert_eq!(view.outcome, Some(EditorPrefabExtractionOutcome::Adopted));
    assert_eq!(view.refusal, None);
    assert!(!view.writes_fenced && !view.can_cancel() && !view.can_retry());
    for (path, bytes) in outputs {
        assert_eq!(fs::read(fixture.root.join(&path)).unwrap(), bytes, "{path}");
    }
    assert!(
        fs::read_to_string(fixture.root.join(SOURCE))
            .unwrap()
            .contains("saved-before-retirement")
    );
    let frame = application.current_frame().unwrap();
    let workspace = frame.workspace();
    assert_eq!(
        workspace
            .documents
            .iter()
            .map(|tab| (
                tab.document,
                tab.authoring_schema_basis.clone(),
                tab.source_binding.clone(),
            ))
            .collect::<Vec<_>>(),
        original_workspace
            .documents
            .iter()
            .map(|tab| (
                tab.document,
                tab.authoring_schema_basis.clone(),
                tab.source_binding.clone(),
            ))
            .collect::<Vec<_>>(),
        "the original tabs and nominal Schema bases survive; Q stays unopened",
    );
    let saved = workspace.document(startup).unwrap();
    assert!(!saved.dirty);
    assert_eq!(saved.revision, startup_basis.revision);
    assert_eq!(saved.saved_revision, saved.revision);
    assert_eq!(
        saved.history,
        original_workspace.document(startup).unwrap().history
    );
    assert_eq!(
        application.document_query_basis(startup).unwrap(),
        startup_basis
    );
    let replaced = workspace.document(selected).unwrap();
    assert!(!replaced.dirty);
    assert_ne!(
        replaced.revision.source_id(),
        selected_basis.revision.source_id()
    );
    assert_eq!(replaced.saved_revision, replaced.revision);
    assert_eq!(
        replaced.history,
        nara::scene::SceneAuthoringHistoryStatus::default()
    );
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    close(&mut application);
}

#[test]
fn awaiting_selection_close_and_exit_cancel_before_ordinary_dirty_discard_or_save() {
    for exit in [false, true] {
        for decision in [EditorCloseDecision::Discard, EditorCloseDecision::Save] {
            let fixture = fixture();
            let original_disk = [SOURCE, PREFAB, "nara.toml"]
                .map(|path| (path, fs::read(fixture.root.join(path)).unwrap()));
            let mut application =
                fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
            settle_background_imports(&mut application);
            let startup = edit_save_source(&mut application);
            let operation = review(&mut application, Some(startup));
            let original_frame = application.current_frame().unwrap();
            let saved_bytes = reviewed_outputs(&application, operation)
                .into_iter()
                .find(|(path, _)| path == SOURCE)
                .unwrap()
                .1;
            application
                .revise_prefab_extraction(operation, vec![startup, startup])
                .unwrap();
            wait_for_phase(
                &mut application,
                operation,
                EditorPrefabExtractionPhase::AwaitingSelection,
            );
            assert!(application.prefab_extraction_view().unwrap().can_cancel());
            let update = if exit {
                application.request_exit()
            } else {
                application.submit_project_action(EditorProjectAction::CloseProject)
            };
            assert!(!update.exit_ready());

            let deadline = Instant::now() + WAIT;
            let frame = loop {
                let frame = application.current_frame().unwrap();
                if frame.workspace_intent().decision_prompt().is_some() {
                    break frame;
                }
                assert!(
                    Instant::now() < deadline,
                    "Close/Exit did not resume dirty arbitration; {:?}",
                    application.prefab_extraction_view(),
                );
                assert!(!application.advance(Duration::ZERO).exit_ready());
                std::thread::sleep(Duration::from_millis(1));
            };
            let view = application.prefab_extraction_view().unwrap();
            assert_eq!(view.operation, Some(operation));
            assert_eq!(view.phase, EditorPrefabExtractionPhase::Terminal);
            assert_eq!(view.outcome, Some(EditorPrefabExtractionOutcome::Cancelled));
            assert!(!view.work_pending && !view.writes_fenced && !view.can_cancel());
            assert!(!frame.authoring_operation_work_pending());
            assert_eq!(
                frame.workspace_intent().phase(),
                Some(EditorWorkspaceIntentPhase::AwaitingDecision)
            );
            assert_eq!(frame.workspace_intent().decision_document(), Some(startup));
            assert!(frame.workspace().document(startup).unwrap().dirty);
            assert_eq!(
                frame.workspace().documents,
                original_frame.workspace().documents
            );
            assert!(frame.persistence().operation().is_none());
            assert!(matches!(
                application.prefab_extraction_selection(operation),
                Err(EditorPrefabExtractionError::Operation | EditorPrefabExtractionError::Closing)
            ));
            assert_eq!(
                application.revise_prefab_extraction(operation, vec![startup]),
                Err(EditorPrefabExtractionError::Closing)
            );
            for (path, bytes) in &original_disk {
                assert_eq!(&fs::read(fixture.root.join(path)).unwrap(), bytes, "{path}");
            }
            assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());

            let prompt = frame.workspace_intent().decision_prompt().unwrap();
            let submission = application.submit_and_drive(
                EditorActionBatch::new(
                    frame.revision(),
                    [EditorAction::WorkspaceIntent(prompt.resolve(decision))],
                ),
                Duration::ZERO,
            );
            assert!(matches!(
                submission.actions(),
                [crate::app::EditorApplicationActionDispatch::Dispatched(
                    EditorActionOutcome::WorkspaceIntent(
                        EditorWorkspaceIntentRequestResult::Accepted
                    )
                )]
            ));
            let mut exited = submission.update().exit_ready();
            let deadline = Instant::now() + WAIT;
            while application.current_frame().is_some() || (exit && !exited) {
                assert!(
                    Instant::now() < deadline,
                    "Close/Exit did not finish retirement"
                );
                exited |= application.advance(Duration::ZERO).exit_ready();
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(application.state(), EditorApplicationState::ProjectManager);
            assert_eq!(exited, exit);
            for (path, original) in &original_disk {
                let expected = if *path == SOURCE && decision == EditorCloseDecision::Save {
                    &saved_bytes
                } else {
                    original
                };
                assert_eq!(
                    &fs::read(fixture.root.join(path)).unwrap(),
                    expected,
                    "{path}"
                );
            }
            assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());
            close(&mut application);
        }
    }
}
