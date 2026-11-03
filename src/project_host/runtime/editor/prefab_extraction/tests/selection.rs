//! Real save-selection revisions retain the original Session inputs through physical return.

use super::*;

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

fn assert_original_options(
    editor: &EditorProjectSession,
    operation: EditorPrefabExtractionId,
    [startup, selected, inactive, guard]: [EditorDocumentId; 4],
) {
    assert!(editor.prefab_extraction_view().can_revise());
    let selection = editor.prefab_extraction_selection(operation).unwrap();
    assert_eq!(selection.documents().len(), 4);
    for (option, (document, path, kind)) in selection.documents().zip([
        (startup, STARTUP, SourceDocumentKind::Scene),
        (selected, P, SourceDocumentKind::Prefab),
        (inactive, INACTIVE, SourceDocumentKind::Scene),
        (guard, GUARD, SourceDocumentKind::Prefab),
    ]) {
        assert_eq!(option.document(), document);
        assert_eq!(option.source_key(), &key(path, kind));
        assert_eq!(option.project_path(), path);
    }
}

fn awaiting_selection(editor: &mut EditorProjectSession, operation: EditorPrefabExtractionId) {
    wait(editor, |editor| {
        !editor.prefab_extraction_view().work_pending
    });
    let view = editor.prefab_extraction_view();
    assert_eq!(view.operation, Some(operation));
    assert_eq!(view.phase, EditorPrefabExtractionPhase::AwaitingSelection);
    assert_eq!(view.outcome, None);
    assert_eq!(view.refusal, Some(EditorPrefabExtractionError::Preparation));
    assert_eq!(view.sources, SOURCES.len());
    assert_eq!(view.documents, 4);
    assert!(view.writes_fenced && view.can_cancel());
    assert!(!view.work_pending && !view.can_retry());
    assert!(!editor.current_frame().authoring_operation_work_pending());
    assert!(
        editor
            .prefab_extraction_diagnostics()
            .unwrap()
            .iter()
            .any(|diagnostic| {
                diagnostic.code().as_str() == "tooling.workspace-prefab-extraction-rejected"
            })
    );
    assert!(matches!(
        editor.prefab_extraction_review(operation),
        Err(EditorPrefabExtractionError::Operation)
    ));
    assert_eq!(
        editor.confirm_prefab_extraction(operation),
        Err(EditorPrefabExtractionError::Operation)
    );
    assert_eq!(
        editor.retry_prefab_extraction(operation),
        Err(EditorPrefabExtractionError::Operation)
    );
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

fn assert_dispositions(
    editor: &EditorProjectSession,
    operation: EditorPrefabExtractionId,
    [startup, selected, inactive, guard]: [EditorDocumentId; 4],
    save_inactive: bool,
) {
    let review = editor.prefab_extraction_review(operation).unwrap();
    assert_eq!(review.source_count(), SOURCES.len());
    assert_eq!(
        review.documents().collect::<Vec<_>>(),
        [
            (startup, EditorPrefabExtractionDisposition::ReplaceSource),
            (selected, EditorPrefabExtractionDisposition::ReplaceSource),
            (
                inactive,
                if save_inactive {
                    EditorPrefabExtractionDisposition::SaveCurrent
                } else {
                    EditorPrefabExtractionDisposition::Guard
                },
            ),
            (guard, EditorPrefabExtractionDisposition::Guard),
        ]
    );
}

fn assert_encoded_selection(editor: &EditorProjectSession, operation: EditorPrefabExtractionId) {
    let review = editor.prefab_extraction_review(operation).unwrap();
    let mut paths: Vec<_> = review
        .outputs()
        .map(|output| output.project_path().to_owned())
        .collect();
    paths.sort_unstable();
    let mut expected = [STARTUP, P, NESTED, CLOSED, DORMANT, INACTIVE, Q, Q_META];
    expected.sort_unstable();
    assert_eq!(paths, expected);
    for output in review.outputs() {
        let path = output.project_path();
        if path == Q_META {
            assert_eq!(output.source_key(), None);
            assert_eq!(output.source_encoding(), None);
            assert_eq!(
                AssetMetaCandidate::<()>::decode_json_bytes(output.bytes())
                    .unwrap()
                    .identity(),
                &metadata("extracted.prefab.json", Q_ID)
            );
            continue;
        }
        let scene = [STARTUP, INACTIVE, DORMANT].contains(&path);
        assert_eq!(
            output.source_key(),
            Some(&key(
                path,
                if scene {
                    SourceDocumentKind::Scene
                } else {
                    SourceDocumentKind::Prefab
                }
            ))
        );
        assert_eq!(
            output.source_encoding(),
            Some(if path == CLOSED {
                SceneFileEncoding::Ron
            } else {
                SceneFileEncoding::Json
            })
        );
        match path {
            STARTUP | INACTIVE => {
                let document = SceneDocumentCandidate::decode_json_bytes(output.bytes())
                    .unwrap()
                    .into_composition_source()
                    .unwrap();
                let (entity, value) = if path == STARTUP {
                    assert_routed_suppression(&document.entities, "left");
                    ("left", "dirty-startup")
                } else {
                    ("target", "dirty-inactive")
                };
                let entity = document
                    .entities
                    .iter()
                    .find(|record| record.id == id(entity))
                    .unwrap();
                assert_eq!(
                    entity.components[&name_component()].value.as_str(),
                    Some(value)
                );
            }
            P | Q => {
                let document = PrefabDocumentCandidate::decode_json_bytes(output.bytes())
                    .unwrap()
                    .into_composition_source()
                    .unwrap();
                let branch = document
                    .entities
                    .iter()
                    .find(|entity| entity.id == id("branch"))
                    .unwrap();
                if path == P {
                    assert_eq!(document.entities.len(), 3);
                    assert!(
                        !document
                            .entities
                            .iter()
                            .any(|entity| entity.id == id("child"))
                    );
                    let instance = branch.prefab.as_ref().unwrap();
                    assert_eq!(instance.source, AssetRef::StableId(Q_ID.parse().unwrap()));
                    assert_eq!(
                        instance.binding,
                        PrefabInstanceBinding::Root {
                            base: key(Q, SourceDocumentKind::Prefab),
                            root: PrefabEntityTarget::source(id("branch")),
                        }
                    );
                } else {
                    assert_eq!(document.entities.len(), 2);
                    assert!(
                        document
                            .entities
                            .iter()
                            .any(|entity| entity.id == id("child"))
                    );
                    assert_eq!(
                        branch.components[&name_component()].value.as_str(),
                        Some("dirty-branch")
                    );
                    assert!(
                        document
                            .placements
                            .contains(&SceneEntityPlacement::root(id("branch")))
                    );
                }
            }
            _ => {}
        }
    }
}

fn confirm_and_assert_saved_generation(
    editor: &mut EditorProjectSession,
    fixture: &Fixture,
    operation: EditorPrefabExtractionId,
    [startup, selected, inactive, guard]: [EditorDocumentId; 4],
    before: &[SourceState; 4],
    disk: &[(PathBuf, Vec<u8>)],
) {
    let expected = reviewed_outputs(editor, operation);
    editor.confirm_prefab_extraction(operation).unwrap();
    wait(editor, |editor| {
        !editor.prefab_extraction_view().work_pending
    });
    let view = editor.prefab_extraction_view();
    assert_eq!(view.operation, Some(operation));
    assert_eq!(
        view.phase,
        EditorPrefabExtractionPhase::Terminal,
        "{view:?}"
    );
    assert_eq!(
        view.outcome,
        Some(EditorPrefabExtractionOutcome::Adopted),
        "{view:?}; {:?}",
        editor.prefab_extraction_diagnostics()
    );
    assert_eq!(view.refusal, None);
    assert!(!view.writes_fenced && !view.can_cancel() && !view.can_retry());
    assert!(!view.can_revise());
    assert!(!editor.current_frame().authoring_operation_work_pending());
    assert!(matches!(
        editor.prefab_extraction_selection(operation),
        Err(EditorPrefabExtractionError::Operation)
    ));
    let project = editor.authoring_project().unwrap();
    assert_eq!(
        project.settings().authoring_state,
        nara_project::ProjectAuthoringState::Ready
    );
    assert_eq!(
        Some(project.schema_owner_input().schema_basis()),
        before[1].basis.as_ref()
    );
    let manifest = nara_project::ProjectManifest::parse_toml_bytes(&fixture.bytes("nara.toml"))
        .manifest
        .unwrap();
    assert_eq!(
        manifest.authoring.state,
        nara_project::ProjectAuthoringState::Ready
    );
    let original_manifest = disk
        .iter()
        .find(|(path, _)| path == &PathBuf::from("nara.toml"))
        .unwrap();
    assert_eq!(
        manifest,
        nara_project::ProjectManifest::parse_toml_bytes(&original_manifest.1)
            .manifest
            .unwrap()
    );
    for (path, bytes) in &expected {
        assert_eq!(&fixture.bytes(path), bytes, "{path}");
    }
    for (path, bytes) in disk {
        if path != &PathBuf::from("nara.toml")
            && !expected
                .iter()
                .any(|(output, _)| PathBuf::from(output) == *path)
        {
            assert_eq!(
                &fs::read(fixture.root.join(path)).unwrap(),
                bytes,
                "{path:?}"
            );
        }
    }
    assert_eq!(editor.workspace().len(), 4);
    assert_eq!(state(editor, guard), before[3]);
    for (document, path, original) in [(startup, STARTUP, &before[0]), (selected, P, &before[1])] {
        let installed = state(editor, document);
        assert!(!installed.dirty);
        assert_eq!(installed.basis, original.basis);
        assert_ne!(
            installed.revision.source_id(),
            original.revision.source_id()
        );
        assert_eq!(installed.revision, installed.saved_revision);
        assert_eq!(installed.content, installed.saved_content);
        assert_eq!(installed.history, SceneAuthoringHistoryStatus::default());
        assert_eq!(installed.usage, SceneAuthoringHistoryUsage::default());
        assert_eq!(installed.encoded.as_bytes(), fixture.bytes(path));
    }
    let saved = state(editor, inactive);
    assert!(!saved.dirty);
    assert_eq!(saved.source, before[2].source);
    assert_eq!(saved.basis, before[2].basis);
    assert_eq!(saved.revision, before[2].revision);
    assert_eq!(saved.content, before[2].content);
    assert_eq!(saved.encoded, before[2].encoded);
    assert_eq!(saved.history, before[2].history);
    assert_eq!(saved.usage, before[2].usage);
    assert_eq!(saved.reload, before[2].reload);
    assert_eq!(saved.saved_revision, before[2].revision);
    assert_eq!(saved.saved_content, before[2].content);
    assert_eq!(saved.encoded.as_bytes(), fixture.bytes(INACTIVE));
    for (document, path) in [(startup, STARTUP), (selected, P), (inactive, INACTIVE)] {
        let digest = ContentDigest::of_bytes(&fixture.bytes(path));
        assert_eq!(
            state(editor, document).saved_digest,
            Some(EditorDocumentDigest::new(
                digest.length(),
                *digest.as_bytes()
            ))
        );
    }
    // The revised SaveCurrent checkpoint preserves both original history branches.
    history(editor, inactive, true);
    assert!(state(editor, inactive).dirty);
    history(editor, inactive, false);
    assert!(!state(editor, inactive).dirty);
    history(editor, inactive, false);
    assert!(state(editor, inactive).dirty);
    assert!(state(editor, inactive).encoded.contains("redo-inactive"));
    history(editor, inactive, true);
    let restored = state(editor, inactive);
    assert!(!restored.dirty);
    // Ordinary Undo/Redo may publish another immutable document allocation while retaining
    // the same source incarnation. Pointer equality is required only before those edits.
    assert_eq!(restored.revision.source_id(), saved.revision.source_id());
    assert_eq!(restored.content, saved.content);
    assert_eq!(restored.history, saved.history);
    assert_eq!(restored.usage, saved.usage);
    assert_eq!(restored.saved_revision, saved.saved_revision);
    assert_eq!(restored.saved_content, saved.saved_content);
    assert_eq!(restored.saved_digest, saved.saved_digest);
    assert_eq!(fixture.bytes(INACTIVE), saved.encoded.as_bytes());
}

#[test]
fn invalid_initial_save_selection_keeps_original_options_until_correction_and_confirmation() {
    for duplicate in [true, false] {
        let fixture = Fixture::new();
        fixture.run(|editor| {
            let documents @ [startup, selected, inactive, _] = dirty_documents(editor);
            let before = documents.map(|document| state(editor, document));
            let disk = fixture.disk();
            let foreign = EditorDocumentId::from_raw(u64::MAX);
            assert!(editor.workspace().document_kind(foreign).is_none());
            let mut extraction = request(selected);
            extraction.save_current = if duplicate {
                vec![inactive, inactive]
            } else {
                vec![inactive, foreign]
            };
            // Four live Hosts, seven physical sources and final publication overlap. Select
            // this transaction's finite ceiling before capture, without changing defaults.
            let limits = AuthoringSourceOperationLimits::default()
                .with_open_handles(ItemLimit::new(128).unwrap());
            let operation = editor
                .begin_prefab_extraction_with_limits(extraction, limits)
                .unwrap();
            awaiting_selection(editor, operation);
            assert_eq!(editor.prefab_extraction_view().limits, limits);
            assert_original_options(editor, operation, documents);
            assert_writes_rejected(editor, startup, selected);
            assert_eq!(documents.map(|document| state(editor, document)), before);
            assert_eq!(fixture.disk(), disk);
            assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());

            editor
                .revise_prefab_extraction(operation, vec![inactive])
                .unwrap();
            reviewed(editor, operation);
            assert_eq!(editor.prefab_extraction_view().limits, limits);
            assert_original_options(editor, operation, documents);
            assert_dispositions(editor, operation, documents, true);
            assert_encoded_selection(editor, operation);
            assert_eq!(documents.map(|document| state(editor, document)), before);
            assert_eq!(fixture.disk(), disk);
            confirm_and_assert_saved_generation(
                editor, &fixture, operation, documents, &before, &disk,
            );
        });
    }
}

#[test]
fn review_save_current_on_off_and_back_changes_only_unpublished_outputs_and_dispositions() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let documents @ [startup, selected, inactive, guard] = dirty_documents(editor);
        let before = documents.map(|document| state(editor, document));
        let disk = fixture.disk();
        let mut extraction = request(selected);
        extraction.save_current.push(inactive);
        let operation = editor.begin_prefab_extraction(extraction).unwrap();
        reviewed(editor, operation);
        assert_dispositions(editor, operation, documents, true);
        let with_inactive = reviewed_outputs(editor, operation);
        for save_inactive in [false, true] {
            let selection = if save_inactive {
                vec![inactive]
            } else {
                Vec::new()
            };
            editor
                .revise_prefab_extraction(operation, selection)
                .unwrap();
            reviewed(editor, operation);
            assert_eq!(editor.prefab_extraction_view().refusal, None);
            assert_eq!(
                editor.prefab_extraction_view().limits,
                AuthoringSourceOperationLimits::default()
            );
            assert_original_options(editor, operation, documents);
            assert_dispositions(editor, operation, documents, save_inactive);
            let expected: Vec<_> = with_inactive
                .iter()
                .filter(|(path, _)| save_inactive || path != INACTIVE)
                .cloned()
                .collect();
            assert_eq!(reviewed_outputs(editor, operation), expected);
            assert_writes_rejected(editor, startup, selected);
            assert_eq!(documents.map(|document| state(editor, document)), before);
            assert_eq!(fixture.disk(), disk);
        }
        editor.cancel_prefab_extraction(operation).unwrap();
        cancelled(editor, operation);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(fixture.disk(), disk);
        for (document, original, redo) in [
            (inactive, &before[2], "redo-inactive"),
            (guard, &before[3], "redo-guard"),
        ] {
            history(editor, document, false);
            assert!(state(editor, document).encoded.contains(redo));
            history(editor, document, true);
            let restored = state(editor, document);
            assert_eq!(restored.revision.source_id(), original.revision.source_id());
            assert_eq!(restored.encoded, original.encoded);
            assert_eq!(restored.content, original.content);
            assert_eq!(restored.history, original.history);
            assert_eq!(restored.usage, original.usage);
            assert_eq!(restored.saved_revision, original.saved_revision);
            assert_eq!(restored.saved_content, original.saved_content);
            assert_eq!(restored.saved_digest, original.saved_digest);
            assert!(restored.dirty);
        }
        assert_eq!(fixture.disk(), disk);
        assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());
    });
}

#[test]
fn invalid_review_revision_corrects_with_the_same_operation_basis_and_encoded_saved_generation() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let documents @ [startup, selected, inactive, _] = dirty_documents(editor);
        let before = documents.map(|document| state(editor, document));
        let disk = fixture.disk();
        let mut extraction = request(selected);
        extraction.save_current.push(inactive);
        let limits = AuthoringSourceOperationLimits::default()
            .with_open_handles(ItemLimit::new(128).unwrap());
        let operation = editor
            .begin_prefab_extraction_with_limits(extraction, limits)
            .unwrap();
        reviewed(editor, operation);
        let outputs = reviewed_outputs(editor, operation);
        let original = editor.prefab_extraction.review.as_ref().unwrap();
        let basis = original.schema_basis().clone();
        let spent = original.host_usage().decode_work();

        editor
            .revise_prefab_extraction(operation, vec![inactive, inactive])
            .unwrap();
        awaiting_selection(editor, operation);
        assert_original_options(editor, operation, documents);
        assert_writes_rejected(editor, startup, selected);
        let failure = editor
            .prefab_extraction
            .preparation_failure
            .as_ref()
            .unwrap();
        assert_eq!(failure.schema_basis(), Some(&basis));
        let refused_work = failure.host_usage().unwrap().decode_work();
        assert!(refused_work > spent);
        assert_eq!(editor.prefab_extraction_view().limits, limits);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(fixture.disk(), disk);

        editor
            .revise_prefab_extraction(operation, vec![inactive])
            .unwrap();
        reviewed(editor, operation);
        let corrected = editor.prefab_extraction.review.as_ref().unwrap();
        assert_eq!(corrected.schema_basis(), &basis);
        assert!(corrected.host_usage().decode_work() > refused_work);
        assert_eq!(editor.prefab_extraction_view().limits, limits);
        assert_original_options(editor, operation, documents);
        assert_dispositions(editor, operation, documents, true);
        assert_eq!(reviewed_outputs(editor, operation), outputs);
        assert_encoded_selection(editor, operation);
        assert_eq!(documents.map(|document| state(editor, document)), before);
        assert_eq!(fixture.disk(), disk);
        confirm_and_assert_saved_generation(editor, &fixture, operation, documents, &before, &disk);
    });
}

#[test]
fn cancel_and_close_wait_for_the_actual_revision_worker_before_releasing_original_inputs() {
    for closing in [false, true] {
        let fixture = Fixture::new();
        fixture.run(|editor| {
            let documents @ [startup, selected, inactive, _] = dirty_documents(editor);
            let before = documents.map(|document| state(editor, document));
            let disk = fixture.disk();
            let mut extraction = request(selected);
            extraction.save_current.push(inactive);
            let operation = editor.begin_prefab_extraction(extraction).unwrap();
            reviewed(editor, operation);
            let mut held = HeldPreparation::arm(editor);
            editor
                .revise_prefab_extraction(operation, Vec::new())
                .unwrap();
            let returned = held.wait_until_entered(editor);
            held.assert_pending(editor, &returned);
            assert_eq!(editor.prefab_extraction_view().operation, Some(operation));
            assert!(!editor.prefab_extraction_view().can_revise());
            assert!(matches!(
                editor.prefab_extraction_selection(operation),
                Err(EditorPrefabExtractionError::Operation)
            ));
            assert_eq!(
                editor.confirm_prefab_extraction(operation),
                Err(EditorPrefabExtractionError::Operation)
            );
            if !closing {
                editor.cancel_prefab_extraction(operation).unwrap();
            }
            for _ in 0..3 {
                let started = Instant::now();
                if closing {
                    let progress = editor.advance_close();
                    assert!(matches!(
                        progress,
                        EditorProjectCloseProgress::Incomplete { .. }
                    ));
                    assert!(!progress.task_shutdown().is_complete());
                } else {
                    let frame = editor.drive_editor_frame(Duration::ZERO);
                    assert!(frame.authoring_operation_work_pending());
                }
                assert!(
                    started.elapsed() < Duration::from_secs(2),
                    "Cancel and Close must poll the actual revision worker without blocking"
                );
                held.assert_pending(editor, &returned);
                assert_eq!(editor.prefab_extraction_view().operation, Some(operation));
                assert_eq!(
                    editor.prefab_extraction_view().limits,
                    AuthoringSourceOperationLimits::default()
                );
                if !closing {
                    assert_writes_rejected(editor, startup, selected);
                }
                assert_eq!(documents.map(|document| state(editor, document)), before);
                assert_eq!(fixture.disk(), disk);
                assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());
            }
            held.release();
            if closing {
                close(editor);
            } else {
                wait(editor, |editor| {
                    editor.prefab_extraction_view().phase == EditorPrefabExtractionPhase::Terminal
                });
            }
            assert_eq!(returned.state(), TaskReturnState::Returned);
            cancelled(editor, operation);
            assert_eq!(documents.map(|document| state(editor, document)), before);
            assert_eq!(fixture.disk(), disk);
            assert!(!fixture.root.join(Q).exists() && !fixture.root.join(Q_META).exists());
            if closing {
                assert_eq!(
                    editor.begin_prefab_extraction(request(selected)),
                    Err(EditorPrefabExtractionError::Closing)
                );
            } else {
                assert!(!editor.current_frame().authoring_operation_work_pending());
                edit(
                    editor,
                    selected,
                    "branch",
                    "editing-after-revision-cancellation",
                );
            }
        });
    }
}
