//! Real Session confirmation, selective publication and pre-Pending cancellation.

use super::*;
use crate::project_host::EditorSourceOperationId;
use nara_core::ByteLimit;

fn dirty_documents(editor: &mut EditorProjectSession) -> [EditorDocumentId; 4] {
    let startup = editor.workspace().active_document().unwrap();
    let selected = open(editor, P, AuthoringDocumentKind::Prefab);
    let inactive = open(editor, INACTIVE, AuthoringDocumentKind::Scene);
    let guard = open(editor, GUARD, AuthoringDocumentKind::Prefab);
    for (document, entity, value) in [
        (startup, "left", "dirty-startup"),
        (selected, "branch", "dirty-branch"),
        (inactive, "target", "dirty-inactive"),
        (guard, "target", "dirty-guard"),
    ] {
        edit(editor, document, entity, value);
    }
    for (document, value) in [(inactive, "redo-inactive"), (guard, "redo-guard")] {
        edit(editor, document, "target", value);
        history(editor, document, true);
        let captured = state(editor, document);
        assert!(captured.dirty);
        assert_eq!(captured.history.undo_depth, 1);
        assert_eq!(captured.history.redo_depth, 1);
    }
    [startup, selected, inactive, guard]
}

fn history(editor: &mut EditorProjectSession, document: EditorDocumentId, undo: bool) {
    let base_revision = editor.document_query_basis(document).unwrap().revision;
    let command = if undo {
        EditorWorkspaceCommand::Undo {
            document,
            base_revision,
        }
    } else {
        EditorWorkspaceCommand::Redo {
            document,
            base_revision,
        }
    };
    let report = editor.apply_workspace_command(command);
    assert!(report.applied, "{report:?}");
}

fn reviewed_outputs(
    editor: &EditorProjectSession,
    operation: EditorPrefabExtractionId,
) -> Vec<(String, Vec<u8>)> {
    editor
        .prefab_extraction_review(operation)
        .unwrap()
        .outputs()
        .map(|output| (output.project_path().to_owned(), output.bytes().to_vec()))
        .collect()
}

#[test]
fn confirmed_selective_transaction_publishes_reviewed_bytes_and_keeps_each_checkpoint_contract() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let documents @ [startup, selected, inactive, guard] = dirty_documents(editor);
        let before = documents.map(|document| state(editor, document));
        let original_manifest =
            nara_project::ProjectManifest::parse_toml_bytes(&fixture.bytes("nara.toml"))
                .manifest
                .unwrap();
        let bundle = fixture.bytes(BUNDLE);
        let guard_disk = fixture.bytes(GUARD);
        let prefab_metadata = fixture.bytes(&format!("{P}.meta"));
        let basis = editor
            .authoring_project()
            .unwrap()
            .schema_owner_input()
            .schema_basis()
            .clone();
        let guard_source = editor.authoring_source_selection(guard).unwrap();
        let active = editor.workspace().active_document();
        let tabs: Vec<_> = editor
            .workspace()
            .authoring_model()
            .documents
            .iter()
            .map(|tab| tab.document)
            .collect();
        let mut extraction = request(selected);
        extraction.save_current.push(inactive);
        // This all-profile transaction retains originals, completed file results and the
        // future inventory/startup together with four live Hosts. Select its finite overlap
        // ceiling before capture; the ordinary default and all handle admission stay intact.
        let limits = AuthoringSourceOperationLimits::default()
            .with_open_handles(ItemLimit::new(128).unwrap());
        let operation = editor
            .begin_prefab_extraction_with_limits(extraction, limits)
            .unwrap();
        reviewed(editor, operation);
        assert_eq!(editor.prefab_extraction_view().limits, limits);
        let review = editor.prefab_extraction_review(operation).unwrap();
        assert_eq!(review.source_count(), SOURCES.len());
        assert_eq!(
            review.documents().collect::<Vec<_>>(),
            [
                (startup, EditorPrefabExtractionDisposition::ReplaceSource),
                (selected, EditorPrefabExtractionDisposition::ReplaceSource),
                (inactive, EditorPrefabExtractionDisposition::SaveCurrent),
                (guard, EditorPrefabExtractionDisposition::Guard),
            ]
        );
        let expected = reviewed_outputs(editor, operation);
        let mut paths: Vec<_> = expected.iter().map(|(path, _)| path.as_str()).collect();
        paths.sort_unstable();
        let mut expected_paths = [STARTUP, P, NESTED, CLOSED, DORMANT, INACTIVE, Q, Q_META];
        expected_paths.sort_unstable();
        assert_eq!(paths, expected_paths);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());

        editor.confirm_prefab_extraction(operation).unwrap();
        wait(editor, |editor| {
            !editor.prefab_extraction_view().work_pending
        });
        let view = editor.prefab_extraction_view();
        assert_eq!(view.operation, Some(operation));
        assert_eq!(
            view.phase,
            EditorPrefabExtractionPhase::Terminal,
            "{view:?}; {:?}",
            editor.prefab_extraction_diagnostics()
        );
        assert_eq!(
            view.outcome,
            Some(EditorPrefabExtractionOutcome::Adopted),
            "{view:?}; {:?}",
            editor.prefab_extraction_diagnostics()
        );
        assert_eq!(view.refusal, None);
        assert!(!view.writes_fenced && !view.can_cancel() && !view.can_retry());
        assert!(!editor.current_frame().authoring_operation_work_pending());
        assert!(matches!(
            editor.prefab_extraction_review(operation),
            Err(EditorPrefabExtractionError::Operation)
        ));
        let manifest = nara_project::ProjectManifest::parse_toml_bytes(&fixture.bytes("nara.toml"))
            .manifest
            .unwrap();
        assert_eq!(
            manifest.authoring.state,
            nara_project::ProjectAuthoringState::Ready
        );
        assert_eq!(manifest, original_manifest);
        assert!(matches!(
            editor
                .authoring_project()
                .unwrap()
                .settings()
                .authoring_state,
            nara_project::ProjectAuthoringState::Ready
        ));
        assert_eq!(
            editor
                .authoring_project()
                .unwrap()
                .schema_owner_input()
                .schema_basis(),
            &basis
        );
        assert_eq!(fixture.bytes(BUNDLE), bundle);
        assert_eq!(fixture.bytes(&format!("{P}.meta")), prefab_metadata);
        for (path, bytes) in &expected {
            assert_eq!(&fixture.bytes(path), bytes, "{path}");
        }
        assert_eq!(fixture.bytes(GUARD), guard_disk);
        assert_eq!(state(editor, guard), before[3]);
        assert!(editor.authoring_source_selection(guard).unwrap() == guard_source);
        assert_eq!(editor.workspace().len(), documents.len());
        assert_eq!(editor.workspace().active_document(), active);
        assert_eq!(
            editor
                .workspace()
                .authoring_model()
                .documents
                .iter()
                .map(|tab| tab.document)
                .collect::<Vec<_>>(),
            tabs
        );

        for (document, path, original) in
            [(startup, STARTUP, &before[0]), (selected, P, &before[1])]
        {
            let installed = state(editor, document);
            assert!(!installed.dirty);
            assert_eq!(installed.basis.as_ref(), Some(&basis));
            assert_ne!(
                installed.revision.source_id(),
                original.revision.source_id()
            );
            assert_ne!(installed.content, original.content);
            assert_eq!(installed.revision, installed.saved_revision);
            assert_eq!(installed.content, installed.saved_content);
            assert_eq!(installed.history, SceneAuthoringHistoryStatus::default());
            assert_eq!(installed.usage, SceneAuthoringHistoryUsage::default());
            assert_eq!(installed.encoded.as_bytes(), fixture.bytes(path));
        }
        let saved_current = state(editor, inactive);
        assert!(!saved_current.dirty);
        assert_eq!(saved_current.source, before[2].source);
        assert_eq!(saved_current.basis, before[2].basis);
        assert_eq!(saved_current.revision, before[2].revision);
        assert_eq!(saved_current.content, before[2].content);
        assert_eq!(saved_current.encoded, before[2].encoded);
        assert_eq!(saved_current.history, before[2].history);
        assert_eq!(saved_current.usage, before[2].usage);
        assert_eq!(saved_current.saved_revision, before[2].revision);
        assert_eq!(saved_current.saved_content, before[2].content);
        for (document, path) in [(startup, STARTUP), (selected, P), (inactive, INACTIVE)] {
            let bytes = fixture.bytes(path);
            let digest = ContentDigest::of_bytes(&bytes);
            assert_eq!(
                state(editor, document).saved_digest,
                Some(EditorDocumentDigest::new(
                    digest.length(),
                    *digest.as_bytes()
                ))
            );
            let source = editor.authoring_source_selection(document).unwrap();
            assert_eq!(source.project_path.as_ref(), path);
            assert_eq!(source.schema.as_ref(), Some(&basis));
            let receipt = source.receipt.unwrap();
            let leaf = editor
                .source_project_root()
                .open_file_exact(&RelativePath::new(path).unwrap())
                .unwrap();
            assert_eq!(receipt.identity(), leaf.identity());
            assert_eq!(receipt.digest(), digest);
        }

        history(editor, inactive, true);
        assert!(state(editor, inactive).dirty);
        history(editor, inactive, false);
        assert!(!state(editor, inactive).dirty);
        history(editor, inactive, false);
        assert!(state(editor, inactive).dirty);
        assert!(state(editor, inactive).encoded.contains("redo-inactive"));
        history(editor, inactive, true);
        let restored_current = state(editor, inactive);
        assert!(!restored_current.dirty);
        assert_eq!(restored_current.content, saved_current.content);
        assert_eq!(restored_current.history, saved_current.history);
        assert_eq!(restored_current.usage, saved_current.usage);
        assert_eq!(restored_current.saved_digest, saved_current.saved_digest);
        history(editor, guard, false);
        assert!(state(editor, guard).encoded.contains("redo-guard"));
        history(editor, guard, true);
        let restored_guard = state(editor, guard);
        assert_eq!(restored_guard.encoded, before[3].encoded);
        assert_eq!(restored_guard.content, before[3].content);
        assert_eq!(restored_guard.history, before[3].history);
        assert_eq!(restored_guard.usage, before[3].usage);
        assert_eq!(restored_guard.saved_revision, before[3].saved_revision);
        assert_eq!(restored_guard.saved_content, before[3].saved_content);
        assert_eq!(restored_guard.saved_digest, before[3].saved_digest);
        assert!(restored_guard.dirty);

        // Both replacement lane kinds must support ordinary edits and actual persistence.
        for (document, path, entity) in [(startup, STARTUP, "left"), (selected, P, "outside")] {
            let checkpoint = state(editor, document);
            edit(editor, document, entity, "ordinary-after-extraction");
            history(editor, document, true);
            assert_eq!(state(editor, document).encoded, checkpoint.encoded);
            assert!(!state(editor, document).dirty);
            edit(editor, document, entity, "saved-after-extraction");
            let saving = state(editor, document);
            let EditorPersistenceRequestResult::Accepted { attempt } =
                editor.request_persistence(EditorPersistenceCommand::Save {
                    document: Some(document),
                })
            else {
                panic!("the adopted {path} lane must accept ordinary Save");
            };
            wait(editor, |editor| {
                editor.persistence_view().result().is_some()
            });
            assert!(
                matches!(
                    editor.persistence_view().result(),
                    Some(EditorPersistenceResult::Saved { document: saved, revision, .. })
                        if saved == document && revision == saving.revision
                ),
                "{:?}",
                editor.persistence_view()
            );
            let saved = state(editor, document);
            assert!(!saved.dirty);
            assert_eq!(saved.source, saving.source);
            assert_eq!(saved.history, saving.history);
            assert_eq!(saved.encoded, saving.encoded);
            assert_eq!(saved.content, saved.saved_content);
            assert_eq!(saved.revision, saved.saved_revision);
            assert_eq!(saved.encoded.as_bytes(), fixture.bytes(path));
            assert_eq!(
                editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }),
                EditorPersistenceRequestResult::Acknowledged { attempt }
            );
            edit(editor, document, entity, "discard-on-reopen");
            let EditorPersistenceRequestResult::Accepted { attempt } =
                editor.request_persistence(EditorPersistenceCommand::Reopen {
                    document: Some(document),
                })
            else {
                panic!("the adopted {path} lane must accept ordinary Reopen");
            };
            wait(editor, |editor| {
                editor.persistence_view().result().is_some()
            });
            assert!(
                matches!(
                    editor.persistence_view().result(),
                    Some(EditorPersistenceResult::Opened { document: opened, .. })
                        if opened == document
                ),
                "{:?}",
                editor.persistence_view()
            );
            let reopened = state(editor, document);
            assert!(!reopened.dirty);
            assert_eq!(reopened.encoded, saved.encoded);
            assert_eq!(reopened.basis.as_ref(), Some(&basis));
            assert_ne!(reopened.revision.source_id(), saved.revision.source_id());
            assert_eq!(reopened.history, SceneAuthoringHistoryStatus::default());
            assert_eq!(reopened.saved_digest, saved.saved_digest);
            assert_eq!(fixture.bytes(path), saved.encoded.as_bytes());
            assert_eq!(
                editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }),
                EditorPersistenceRequestResult::Acknowledged { attempt }
            );
        }
        for path in [NESTED, CLOSED, DORMANT, INACTIVE, Q, Q_META] {
            let (_, bytes) = expected.iter().find(|(output, _)| output == path).unwrap();
            assert_eq!(&fixture.bytes(path), bytes, "{path}");
        }
        assert_eq!(fixture.bytes(GUARD), guard_disk);
        assert_eq!(fixture.bytes(BUNDLE), bundle);
    });

    // A fresh ordinary Ready admission must resolve Q and all rewritten startup dependencies.
    fixture.run(|editor| {
        let startup = editor.workspace().active_document().unwrap();
        assert!(
            state(editor, startup)
                .encoded
                .contains("saved-after-extraction")
        );
        assert_eq!(editor.workspace().len(), 1);
        let selected = open(editor, P, AuthoringDocumentKind::Prefab);
        assert!(
            state(editor, selected)
                .encoded
                .contains("saved-after-extraction")
        );
        let extracted = open(editor, Q, AuthoringDocumentKind::Prefab);
        assert!(!state(editor, extracted).dirty);
        assert!(state(editor, extracted).encoded.contains("dirty-branch"));
    });
}

#[test]
fn confirmation_then_immediate_cancellation_preserves_original_disk_and_both_history_branches() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let documents @ [_, selected, inactive, _] = dirty_documents(editor);
        let before = documents.map(|document| state(editor, document));
        let disk = fixture.disk();
        let mut extraction = request(selected);
        extraction.save_current.push(inactive);
        let operation = editor.begin_prefab_extraction(extraction).unwrap();
        reviewed(editor, operation);
        editor.confirm_prefab_extraction(operation).unwrap();
        assert!(editor.prefab_extraction_view().can_cancel());
        // No frame has advanced confirmation, so no publication worker can attempt Pending.
        editor.cancel_prefab_extraction(operation).unwrap();
        wait(editor, |editor| {
            editor.prefab_extraction_view().phase == EditorPrefabExtractionPhase::Terminal
        });
        cancelled(editor, operation);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(fixture.disk(), disk);
        for (document, original) in documents.into_iter().zip(&before) {
            history(editor, document, true);
            assert!(!state(editor, document).dirty);
            history(editor, document, false);
            if original.history.redo_depth != 0 {
                history(editor, document, false);
                history(editor, document, true);
            }
            let restored = state(editor, document);
            assert_eq!(restored.revision.source_id(), original.revision.source_id());
            assert_eq!(restored.encoded, original.encoded);
            assert_eq!(restored.content, original.content);
            assert_eq!(restored.basis, original.basis);
            assert_eq!(restored.history, original.history);
            assert_eq!(restored.usage, original.usage);
            assert_eq!(restored.saved_revision, original.saved_revision);
            assert_eq!(restored.saved_content, original.saved_content);
            assert_eq!(restored.saved_digest, original.saved_digest);
            assert!(restored.dirty);
        }
        assert_eq!(fixture.disk(), disk);
    });
}

#[test]
fn confirmed_cancellation_waits_for_the_registered_play_custodians_exact_acknowledgment() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let binding = editor.register_supervised_play_owner(73).unwrap();
        let documents @ [_, selected, inactive, _] = dirty_documents(editor);
        let before = documents.map(|document| state(editor, document));
        let disk = fixture.disk();
        // Always acknowledge the empty registered custodian before Fixture.run closes, even
        // when an assertion fails while retirement is withheld. No external child is launched.
        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut extraction = request(selected);
            extraction.save_current.push(inactive);
            let operation = editor.begin_prefab_extraction(extraction).unwrap();
            reviewed(editor, operation);
            let digest = editor.prefab_extraction.review.as_ref().unwrap().digest();
            editor.confirm_prefab_extraction(operation).unwrap();
            let retirement = editor
                .supervised_play_retirement_request(&binding)
                .unwrap()
                .unwrap();
            assert_eq!(
                retirement.operation(),
                EditorSourceOperationId::PrefabExtraction(operation)
            );
            assert_eq!(retirement.recovery_digest(), digest);
            assert_eq!(retirement.published_generation(), 73);
            editor.cancel_prefab_extraction(operation).unwrap();
            for _ in 0..3 {
                let started = Instant::now();
                let frame = editor.drive_editor_frame(Duration::ZERO);
                assert!(
                    started.elapsed() < Duration::from_secs(2),
                    "cancellation must poll the registered custodian without blocking"
                );
                assert!(frame.authoring_operation_work_pending());
                assert!(editor.supervised_play_retirement_pending());
                let repeated = editor
                    .supervised_play_retirement_request(&binding)
                    .unwrap()
                    .unwrap();
                assert!(retirement.is_same_request(&repeated));
                let view = editor.prefab_extraction_view();
                assert_eq!(view.operation, Some(operation));
                assert_eq!(view.phase, EditorPrefabExtractionPhase::Retiring);
                assert_eq!(view.outcome, None);
                assert!(view.work_pending && view.writes_fenced && view.can_cancel());
                assert!(!view.can_retry());
                assert_eq!(documents.map(|document| state(editor, document)), before);
                assert_eq!(fixture.disk(), disk);
                assert_eq!(
                    editor.request_persistence(EditorPersistenceCommand::Save {
                        document: Some(selected),
                    }),
                    EditorPersistenceRequestResult::Refused(EditorPersistenceRejection::Busy)
                );
            }
            operation
        }));
        if let Some(retirement) = editor.supervised_play_retirement_request(&binding).unwrap() {
            let acknowledgment = binding.attest_retirement(retirement).unwrap();
            editor
                .apply_supervised_play_retirement_ack(&binding, acknowledgment)
                .unwrap();
        }
        let operation = match result {
            Ok(operation) => operation,
            Err(payload) => resume_unwind(payload),
        };
        assert!(!editor.supervised_play_retirement_pending());
        assert!(editor.prefab_extraction_view().writes_fenced);
        wait(editor, |editor| {
            editor.prefab_extraction_view().phase == EditorPrefabExtractionPhase::Terminal
        });
        cancelled(editor, operation);
        assert!(
            editor
                .supervised_play_retirement_request(&binding)
                .unwrap()
                .is_none()
        );
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(fixture.disk(), disk);
        edit(
            editor,
            selected,
            "branch",
            "editing-after-acknowledged-cancellation",
        );
    });
}

#[test]
fn changed_original_disk_refuses_confirmation_and_retains_the_exact_review_without_creating_q() {
    for changed in [P, CLOSED, GUARD, DORMANT] {
        let fixture = Fixture::new();
        fixture.run(|editor| {
            let documents @ [_, selected, inactive, _] = dirty_documents(editor);
            let before = documents.map(|document| state(editor, document));
            let mut extraction = request(selected);
            extraction.save_current.push(inactive);
            let operation = editor.begin_prefab_extraction(extraction).unwrap();
            reviewed(editor, operation);
            let outputs = reviewed_outputs(editor, operation);
            let mut external = fixture.bytes(changed);
            // Keep the source valid: exact original bytes, including unchanged Guard and
            // closed/profile-specific members, must still be revalidated before retirement.
            external.push(b'\n');
            fs::write(fixture.root.join(changed), &external).unwrap();
            let disk = fixture.disk();
            assert_eq!(
                editor.confirm_prefab_extraction(operation),
                Err(EditorPrefabExtractionError::Stale),
                "{changed}"
            );
            let view = editor.prefab_extraction_view();
            assert_eq!(view.operation, Some(operation));
            assert_eq!(view.phase, EditorPrefabExtractionPhase::Review);
            assert_eq!(view.outcome, None);
            assert_eq!(view.refusal, Some(EditorPrefabExtractionError::Stale));
            assert!(view.writes_fenced && view.can_cancel());
            assert!(!view.work_pending && !view.can_retry());
            assert_eq!(reviewed_outputs(editor, operation), outputs);
            assert_eq!(documents.map(|document| state(editor, document)), before);
            assert_eq!(fixture.bytes(changed), external);
            assert_eq!(fixture.disk(), disk);
            assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());
            assert_eq!(
                editor.retry_prefab_extraction(operation),
                Err(EditorPrefabExtractionError::Operation)
            );
            editor.cancel_prefab_extraction(operation).unwrap();
            cancelled(editor, operation);
            assert_eq!(documents.map(|document| state(editor, document)), before);
            assert_eq!(fixture.disk(), disk);
        });
    }
}

#[test]
fn selected_source_limit_reports_capture_capacity_without_changing_original_inputs() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let selected = open(editor, P, AuthoringDocumentKind::Prefab);
        let before = state(editor, selected);
        let disk = fixture.disk();
        let mut extraction = request(selected);
        extraction.compilation_limits = extraction
            .compilation_limits
            .with_working_bytes(ByteLimit::ONE);
        let operation = editor.begin_prefab_extraction(extraction).unwrap();
        wait(editor, |editor| {
            !editor.prefab_extraction_view().work_pending
        });
        let view = editor.prefab_extraction_view();
        assert_eq!(view.operation, Some(operation));
        assert_eq!(view.phase, EditorPrefabExtractionPhase::Terminal);
        assert_eq!(
            view.outcome,
            Some(EditorPrefabExtractionOutcome::Failed(
                EditorPrefabExtractionError::SourceLimit,
            )),
        );
        assert!(!view.writes_fenced);
        assert!(
            editor
                .prefab_extraction_diagnostics()
                .unwrap()
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-extraction-capacity")
        );
        assert_eq!(state(editor, selected), before);
        assert_eq!(fixture.disk(), disk);
        assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());
    });
}

#[test]
fn selected_source_limit_reaches_review_but_refuses_confirmation_with_the_same_original_corpus() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let documents @ [_, selected, inactive, _] = dirty_documents(editor);
        let before = documents.map(|document| state(editor, document));
        let disk = fixture.disk();

        // Measure the real complete preparation, then select that exact cumulative Source
        // allowance for a new operation over the unchanged inputs. Confirmation needs more.
        let mut extraction = request(selected);
        extraction.save_current.push(inactive);
        let measured = editor.begin_prefab_extraction(extraction).unwrap();
        reviewed(editor, measured);
        let source_bytes = editor
            .prefab_extraction
            .review
            .as_ref()
            .unwrap()
            .test_prepared()
            .usage()
            .work_bytes;
        editor.cancel_prefab_extraction(measured).unwrap();
        cancelled(editor, measured);

        let mut extraction = request(selected);
        extraction.save_current.push(inactive);
        extraction.compilation_limits = extraction
            .compilation_limits
            .with_working_bytes(ByteLimit::new(source_bytes).unwrap());
        let operation = editor.begin_prefab_extraction(extraction).unwrap();
        reviewed(editor, operation);
        let identity = |editor: &EditorProjectSession| {
            let prepared = editor.prefab_extraction.review.as_ref().unwrap();
            (
                std::ptr::from_ref(prepared) as usize,
                prepared
                    .test_prepared()
                    .corpus()
                    .original_sources()
                    .as_ptr() as usize,
                prepared.source_count(),
                prepared.digest(),
                prepared
                    .outputs()
                    .map(|output| output.bytes().unwrap().as_ptr() as usize)
                    .collect::<Vec<_>>(),
            )
        };
        let original = identity(editor);
        let outputs = reviewed_outputs(editor, operation);
        assert_eq!(
            editor
                .prefab_extraction
                .review
                .as_ref()
                .unwrap()
                .test_prepared()
                .usage()
                .work_bytes,
            source_bytes,
        );
        assert_eq!(
            editor.confirm_prefab_extraction(operation),
            Err(EditorPrefabExtractionError::SourceLimit),
        );
        let view = editor.prefab_extraction_view();
        assert_eq!(view.operation, Some(operation));
        assert_eq!(view.phase, EditorPrefabExtractionPhase::Review);
        assert_eq!(view.outcome, None);
        assert_eq!(view.refusal, Some(EditorPrefabExtractionError::SourceLimit));
        assert!(view.writes_fenced && view.can_cancel());
        assert!(!view.work_pending && !view.can_retry());
        assert!(
            editor
                .prefab_extraction_diagnostics()
                .unwrap()
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-extraction-capacity")
        );
        assert_eq!(identity(editor), original);
        assert_eq!(reviewed_outputs(editor, operation), outputs);
        assert!(editor.prefab_extraction.publication.is_none());
        assert!(!editor.prefab_extraction.retirement_started);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(fixture.disk(), disk);
        assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());

        editor.cancel_prefab_extraction(operation).unwrap();
        cancelled(editor, operation);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(fixture.disk(), disk);
    });
}
