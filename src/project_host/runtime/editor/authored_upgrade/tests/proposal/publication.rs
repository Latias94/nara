use super::*;

fn prepared(editor: &mut EditorProjectSession) -> EditorAuthoredSchemaUpgradeId {
    let operation = editor
        .begin_authored_schema_upgrade_with_limits(
            crate::project_host::AuthoringSourceOperationLimits::default()
                .with_open_handles(nara_core::ItemLimit::new(128).unwrap()),
        )
        .unwrap();
    awaiting(editor, operation);
    editor
        .prepare_authored_schema_upgrade(operation, proposal(editor, operation))
        .unwrap();
    reviewed(editor);
    operation
}

fn recovery_packages(fixture: &Fixture) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fs::read_dir(&fixture.root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(".nara-authored-upgrade-")
        })
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
}

#[test]
fn confirmed_complete_upgrade_publishes_ready_and_installs_all_dirty_and_closed_sources() {
    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let startup = editor.workspace().active_document().unwrap();
        let inactive = open(editor, INACTIVE, AuthoringDocumentKind::Scene);
        let prefab = open(editor, PREFAB, AuthoringDocumentKind::Prefab);
        let documents = [startup, inactive, prefab];
        for document in documents {
            edit(editor, document, "reviewed-dirty-name");
        }
        let original = documents.map(|document| state(editor, document));
        let operation = prepared(editor);
        let expected: Vec<_> = editor
            .authored_schema_upgrade_review(operation)
            .unwrap()
            .sources()
            .map(|source| (source.project_path.to_owned(), source.successor.to_vec()))
            .collect();
        editor.confirm_authored_schema_upgrade(operation).unwrap();
        wait(editor, |editor| {
            !editor.authored_schema_upgrade_view().work_pending
        });
        let view = editor.authored_schema_upgrade_view();
        assert_eq!(
            view.outcome,
            Some(EditorAuthoredSchemaUpgradeOutcome::Adopted),
            "{view:?}; {:?}",
            editor
                .authored_schema_upgrade
                .publication
                .as_ref()
                .and_then(|publication| publication.failure())
        );
        assert!(!view.writes_fenced);
        assert!(matches!(
            nara_project::ProjectManifest::parse_toml_bytes(&fixture.bytes("nara.toml"))
                .manifest
                .unwrap()
                .authoring
                .state,
            nara_project::ProjectAuthoringState::Ready
        ));
        for (path, bytes) in &expected {
            assert_eq!(&fixture.bytes(path), bytes, "{path}");
        }
        for (document, before) in documents.into_iter().zip(original) {
            let after = state(editor, document);
            assert!(!after.dirty);
            assert_ne!(after.content, before.content);
            assert_eq!(after.content, after.saved_content);
            assert_eq!(after.revision, after.saved_revision);
            assert_eq!(after.history, SceneAuthoringHistoryStatus::default());
            edit(editor, document, "ordinary-v2-edit");
            let report = editor.apply_workspace_command(EditorWorkspaceCommand::Undo {
                document,
                base_revision: editor.document_query_basis(document).unwrap().revision,
            });
            assert!(report.applied, "{report:?}");
            assert_eq!(state(editor, document).encoded, after.encoded);
        }
        edit(editor, startup, "saved-after-upgrade");
        let EditorPersistenceRequestResult::Accepted { attempt } =
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(startup),
            })
        else {
            panic!("the successor Scene must save through its actual replacement Host");
        };
        wait(editor, |editor| {
            editor.persistence_view().result().is_some()
        });
        assert!(
            matches!(
                editor.persistence_view().result(),
                Some(EditorPersistenceResult::Saved { .. })
            ),
            "{:?}",
            editor.persistence_view()
        );
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult { attempt }),
            EditorPersistenceRequestResult::Acknowledged { attempt }
        );
        edit(editor, startup, "discard-after-upgrade");
        assert!(matches!(
            editor.request_persistence(EditorPersistenceCommand::Reopen {
                document: Some(startup)
            }),
            EditorPersistenceRequestResult::Accepted { .. }
        ));
        wait(editor, |editor| {
            editor.persistence_view().result().is_some()
        });
        assert!(!state(editor, startup).dirty);
        assert!(
            state(editor, startup)
                .encoded
                .contains("saved-after-upgrade")
        );
    });
    // New admission observes the verified successor disk generation, independently of old memory.
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        let source = editor
            .workspace()
            .scene(document)
            .unwrap()
            .session()
            .document();
        assert!(source.to_json_string().unwrap().contains("milli"));
    });
}

#[test]
fn saved_scene_and_prefab_predecessors_finish_upgrade_after_completed_or_draining_save() {
    for drain_last_save in [false, true] {
        let fixture = enemy_fixture();
        fixture.run(|editor| {
            let scene = editor.workspace().active_document().unwrap();
            let prefab = open(editor, PREFAB, AuthoringDocumentKind::Prefab);
            for document in [scene, prefab] {
                edit(editor, document, "saved-before-upgrade");
                let EditorPersistenceRequestResult::Accepted { attempt } = editor
                    .request_persistence(EditorPersistenceCommand::Save {
                        document: Some(document),
                    })
                else {
                    panic!("the predecessor Save must be accepted");
                };
                if drain_last_save && document == prefab {
                    // Begin must drain this accepted Save before freezing the original inputs.
                    break;
                }
                wait(editor, |editor| {
                    editor.persistence_view().result().is_some()
                });
                assert!(matches!(
                    editor.persistence_view().result(),
                    Some(EditorPersistenceResult::Saved { document: saved, .. })
                        if saved == document
                ));
                assert_eq!(
                    editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult {
                        attempt,
                    }),
                    EditorPersistenceRequestResult::Acknowledged { attempt }
                );
            }
            let operation = prepared(editor);
            let expected: Vec<_> = editor
                .authored_schema_upgrade_review(operation)
                .unwrap()
                .sources()
                .map(|source| (source.project_path.to_owned(), source.successor.to_vec()))
                .collect();
            editor.confirm_authored_schema_upgrade(operation).unwrap();
            wait(editor, |editor| {
                !editor.authored_schema_upgrade_view().work_pending
            });
            let view = editor.authored_schema_upgrade_view();
            assert_eq!(
                view.outcome,
                Some(EditorAuthoredSchemaUpgradeOutcome::Adopted),
                "drain_last_save={drain_last_save}; {view:?}; {:?}",
                editor
                    .authored_schema_upgrade
                    .publication
                    .as_ref()
                    .and_then(|publication| publication.failure())
            );
            assert!(!view.writes_fenced);
            for (path, bytes) in expected {
                assert_eq!(fixture.bytes(&path), bytes, "{path}");
            }
            for document in [scene, prefab] {
                let installed = state(editor, document);
                assert!(!installed.dirty);
                assert_eq!(installed.content, installed.saved_content);
                assert_eq!(installed.history, SceneAuthoringHistoryStatus::default());
            }
        });
    }
}

#[test]
fn confirmation_then_pre_pending_cancellation_keeps_original_history_and_disk() {
    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        edit(editor, document, "dirty-before-confirmation");
        let before = state(editor, document);
        let disk = fixture.disk();
        let operation = prepared(editor);
        editor.confirm_authored_schema_upgrade(operation).unwrap();
        assert!(editor.authored_schema_upgrade_view().can_cancel());
        editor.cancel_authored_schema_upgrade(operation).unwrap();
        wait(editor, |editor| {
            editor.authored_schema_upgrade_view().phase
                == EditorAuthoredSchemaUpgradePhase::Terminal
        });
        cancelled(editor, operation);
        assert_eq!(state(editor, document), before);
        assert_eq!(fixture.disk(), disk);
    });
}

#[test]
fn complete_membership_changed_after_review_refuses_before_pending_and_retains_the_reviewed_inputs()
{
    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        let before = state(editor, document);
        let operation = prepared(editor);
        let original = fixture.bytes("nara.toml");
        fs::write(
            fixture.root.join("scenes/late.scene.json"),
            fixture.bytes(CLOSED),
        )
        .unwrap();
        editor.confirm_authored_schema_upgrade(operation).unwrap();
        wait(editor, |editor| {
            !editor.authored_schema_upgrade_view().work_pending
        });
        assert_eq!(
            editor.authored_schema_upgrade_view().phase,
            EditorAuthoredSchemaUpgradePhase::RequiresReconcile
        );
        assert!(editor.authored_schema_upgrade_view().can_cancel());
        assert_eq!(fixture.bytes("nara.toml"), original);
        assert_eq!(state(editor, document), before);
        editor.cancel_authored_schema_upgrade(operation).unwrap();
        wait(editor, |editor| {
            editor.authored_schema_upgrade_view().phase
                == EditorAuthoredSchemaUpgradePhase::Terminal
        });
        cancelled(editor, operation);
    });
}

#[test]
fn pending_prepublication_work_refusal_can_cancel_and_restart_the_identical_proposal() {
    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let document = editor.workspace().active_document().unwrap();
        edit(editor, document, "dirty-before-admission-refusal");
        let before = state(editor, document);
        let disk = fixture.disk();
        let operation = prepared(editor);
        let expected: Vec<_> = editor
            .authored_schema_upgrade_review(operation)
            .unwrap()
            .sources()
            .map(|source| (source.project_path.to_owned(), source.successor.to_vec()))
            .collect();
        editor.confirm_authored_schema_upgrade(operation).unwrap();
        editor
            .authored_schema_upgrade
            .publication
            .as_mut()
            .unwrap()
            .refuse_next_pending_admission();
        wait(editor, |editor| {
            !editor.authored_schema_upgrade_view().work_pending
        });
        let view = editor.authored_schema_upgrade_view();
        assert_eq!(
            view.phase,
            EditorAuthoredSchemaUpgradePhase::RequiresReconcile
        );
        assert!(
            view.can_cancel(),
            "a proven pre-publisher refusal keeps the old generation cancellable: {view:?}"
        );
        assert_eq!(
            view.refusal,
            Some(EditorAuthoredSchemaUpgradeError::WorkLimit)
        );
        assert!(!view.can_retry());
        let retained = recovery_packages(&fixture);
        assert_eq!(
            retained.len(),
            1,
            "recovery must precede the refused Pending"
        );
        assert_eq!(fixture.disk(), disk);
        assert_eq!(state(editor, document), before);
        editor.cancel_authored_schema_upgrade(operation).unwrap();
        wait(editor, |editor| {
            editor.authored_schema_upgrade_view().phase
                == EditorAuthoredSchemaUpgradePhase::Terminal
        });
        cancelled(editor, operation);
        assert_eq!(fixture.disk(), disk);
        assert_eq!(state(editor, document), before);
        assert_eq!(recovery_packages(&fixture), retained);

        let next_operation = prepared(editor);
        assert_ne!(next_operation, operation);
        assert_eq!(
            editor
                .authored_schema_upgrade_review(next_operation)
                .unwrap()
                .sources()
                .map(|source| (source.project_path.to_owned(), source.successor.to_vec()))
                .collect::<Vec<_>>(),
            expected
        );
        editor
            .confirm_authored_schema_upgrade(next_operation)
            .unwrap();
        wait(editor, |editor| {
            !editor.authored_schema_upgrade_view().work_pending
        });
        let view = editor.authored_schema_upgrade_view();
        assert_eq!(
            view.outcome,
            Some(EditorAuthoredSchemaUpgradeOutcome::Adopted),
            "{view:?}; {:?}",
            editor
                .authored_schema_upgrade
                .publication
                .as_ref()
                .and_then(|publication| publication.failure())
        );
        assert!(!view.writes_fenced);
        assert!(matches!(
            nara_project::ProjectManifest::parse_toml_bytes(&fixture.bytes("nara.toml"))
                .manifest
                .unwrap()
                .authoring
                .state,
            nara_project::ProjectAuthoringState::Ready
        ));
        for (path, bytes) in expected {
            assert_eq!(fixture.bytes(&path), bytes, "{path}");
        }
        assert!(!state(editor, document).dirty);
        let packages = recovery_packages(&fixture);
        assert_eq!(
            packages.len(),
            2,
            "each operation owns a distinct destination"
        );
        for (path, bytes) in &retained {
            assert_eq!(packages.get(path), Some(bytes));
        }
        let original_bytes = retained.values().next().unwrap();
        assert!(
            packages.values().all(|bytes| bytes == original_bytes),
            "identical proposals retain identical recovery bytes at distinct paths"
        );
    });
}

#[test]
fn default_handle_ceiling_refuses_the_complete_overlap_before_pending_without_widening_policy() {
    let fixture = enemy_fixture();
    fixture.run(|editor| {
        let disk = fixture.disk();
        let operation = editor.begin_authored_schema_upgrade().unwrap();
        awaiting(editor, operation);
        editor
            .prepare_authored_schema_upgrade(operation, proposal(editor, operation))
            .unwrap();
        reviewed(editor);
        editor.confirm_authored_schema_upgrade(operation).unwrap();
        wait(editor, |editor| {
            !editor.authored_schema_upgrade_view().work_pending
        });
        let view = editor.authored_schema_upgrade_view();
        assert_eq!(
            view.phase,
            EditorAuthoredSchemaUpgradePhase::RequiresReconcile
        );
        assert_eq!(view.limits.open_handles().get(), 64);
        assert_eq!(
            view.refusal,
            Some(EditorAuthoredSchemaUpgradeError::OpenHandleLimit)
        );
        assert!(view.can_cancel());
        assert!(
            editor
                .authored_schema_upgrade_diagnostics()
                .unwrap()
                .iter()
                .any(|diagnostic| diagnostic.code().as_str()
                    == "project.editor.authored-upgrade-open-handle-limit")
        );
        let error = editor
            .authored_schema_upgrade
            .publication
            .as_ref()
            .unwrap()
            .failure()
            .unwrap();
        assert!(
            format!("{error:?}").contains("PeakOpenHandlesExceeded"),
            "{error:?}"
        );
        assert_eq!(fixture.disk(), disk);
        editor.cancel_authored_schema_upgrade(operation).unwrap();
        wait(editor, |editor| {
            editor.authored_schema_upgrade_view().phase
                == EditorAuthoredSchemaUpgradePhase::Terminal
        });
        cancelled(editor, operation);
    });
}
