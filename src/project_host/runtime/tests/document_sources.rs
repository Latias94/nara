use super::*;
use std::thread;

use crate::project_host::authoring::AuthoringDocumentKind;
use nara_scene::{PrefabDocument, PrefabInstance};

#[test]
fn browser_prefab_save_and_reopen_keep_the_anchor_source_observation_after_close() {
    for reopen_source in [false, true] {
        let project = TestProject::new("browser-prefab-saved-observation");
        fs::write(
            project.root.join("prefabs/shared.prefab.json"),
            PrefabDocument::from_ordered_roots([editable_name_entity("original")])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let anchor_id = nara_identity::SceneEntityId::new("anchor").unwrap();
        let mut anchor = SceneEntityRecord::new(anchor_id.clone());
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::path("shared.prefab.json").unwrap(),
            overrides: Default::default(),
        });
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots([anchor])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let mut editor = open_generic_editor_with_ready_index(&project);
        let scene = editor.workspace().active_document().unwrap();
        let index = editor.project_source_index_snapshot().unwrap();
        let prefab = opened_document(editor.open_indexed_authoring_document(
            index,
            "prefabs/shared.prefab.json",
            AuthoringDocumentKind::Prefab,
        ));
        let command = if reopen_source {
            fs::write(
                project.root.join("prefabs/shared.prefab.json"),
                PrefabDocument::from_ordered_roots([editable_name_entity("saved-through-browser")])
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
            EditorPersistenceCommand::Reopen {
                document: Some(prefab),
            }
        } else {
            make_authoring_name_dirty(&mut editor, prefab, "saved-through-browser");
            EditorPersistenceCommand::Save {
                document: Some(prefab),
            }
        };
        assert_eq!(
            editor.request_persistence(command),
            nara_tooling::EditorPersistenceRequestResult::Accepted
        );
        drive_persistence_to_terminal(&mut editor);
        let result = editor.persistence_view().result();
        assert!(
            if reopen_source {
                matches!(result, Some(EditorPersistenceResult::Opened { .. }))
            } else {
                matches!(result, Some(EditorPersistenceResult::Saved { .. }))
            },
            "{result:?}"
        );
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
            nara_tooling::EditorPersistenceRequestResult::Accepted
        );
        assert_eq!(
            editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument {
                document: prefab
            }),
            nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while editor.workspace().prefab(prefab).is_some() {
            assert!(Instant::now() < deadline, "the saved Prefab did not close");
            editor.drive_editor_frame(Duration::ZERO);
        }
        let reopened = open_prefab_from_scene(&mut editor, scene, &anchor_id);
        let content = reopened.opened_document.and_then(|document| {
            editor
                .workspace()
                .prefab(document)
                .map(|slot| slot.session().document().to_json_string().unwrap())
        });
        drive_editor_close_to_completion(&mut editor);
        assert!(reopened.applied, "{:?}", reopened.diagnostics);
        assert!(content.is_some_and(|content| content.contains("saved-through-browser")));
    }
}

#[test]
fn indexed_file_alias_cannot_replace_the_open_source_or_its_unsaved_history() {
    for kind in [AuthoringDocumentKind::Scene, AuthoringDocumentKind::Prefab] {
        let project = TestProject::new("indexed-source-alias");
        let (original, alias) = match kind {
            AuthoringDocumentKind::Scene => {
                fs::write(
                    project.root.join("scenes/startup.scene.json"),
                    editable_authoring_scene("original")
                        .to_json_string()
                        .unwrap(),
                )
                .unwrap();
                ("scenes/startup.scene.json", "scenes/alias.scene.json")
            }
            AuthoringDocumentKind::Prefab => {
                fs::write(
                    project.root.join("prefabs/original.prefab.json"),
                    PrefabDocument::from_ordered_roots([editable_name_entity("original")])
                        .to_json_string()
                        .unwrap(),
                )
                .unwrap();
                ("prefabs/original.prefab.json", "prefabs/alias.prefab.json")
            }
        };
        fs::hard_link(project.root.join(original), project.root.join(alias)).unwrap();
        let mut editor = open_generic_editor_with_ready_index(&project);
        let index = editor.project_source_index_snapshot().unwrap();
        let document = opened_document(editor.open_indexed_authoring_document(
            Arc::clone(&index),
            original,
            kind,
        ));
        make_authoring_name_dirty(&mut editor, document, "unsaved");
        let before = editor.drive_editor_frame(Duration::ZERO);
        let result = editor.open_indexed_authoring_document(index, alias, kind);
        let after = editor.drive_editor_frame(Duration::ZERO);
        let unchanged = before.workspace() == after.workspace();
        drive_editor_close_to_completion(&mut editor);
        assert!(
            matches!(
                result,
                editor::EditorDocumentOpenRequestResult::Rejected {
                    code: "project.editor.document-source-already-open"
                }
            ),
            "{result:?}"
        );
        assert!(
            unchanged,
            "alias rejection must preserve the exact document and history"
        );
    }
}

#[test]
fn anchored_prefab_open_reuses_the_browser_slot_despite_locator_scope() {
    let project = TestProject::new("prefab-source-scope-reuse");
    fs::write(
        project.root.join("prefabs/shared.prefab.json"),
        PrefabDocument::from_ordered_roots([editable_name_entity("prefab")])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let anchor_id = nara_identity::SceneEntityId::new("anchor").unwrap();
    let mut anchor = SceneEntityRecord::new(anchor_id.clone());
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("shared.prefab.json").unwrap(),
        overrides: ScenePatchDocument::default(),
    });
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([anchor])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let scene = editor.workspace().active_document().unwrap();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::SelectEntity {
                document: scene,
                entity: Some(anchor_id.clone()),
            })
            .applied
    );
    let index = editor.project_source_index_snapshot().unwrap();
    let prefab = opened_document(editor.open_indexed_authoring_document(
        index,
        "prefabs/shared.prefab.json",
        AuthoringDocumentKind::Prefab,
    ));
    make_authoring_name_dirty(&mut editor, prefab, "unsaved-prefab");
    let prefab_revision = editor.workspace().prefab(prefab).unwrap().revision();
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::OpenPrefabSource {
                    document: scene,
                    base_revision: editor.workspace().scene(scene).unwrap().revision(),
                    entity: anchor_id,
                },
            )],
        ))
        .unwrap();
    let applied = matches!(&report.outcomes()[0], EditorActionOutcome::Workspace(report) if report.applied && report.active_document == Some(prefab));
    let active = editor.workspace().active_document();
    let retained = editor
        .workspace()
        .prefab(prefab)
        .map(|slot| slot.revision())
        == Some(prefab_revision);
    drive_editor_close_to_completion(&mut editor);
    assert!(applied, "{:?}", report.outcomes());
    assert_eq!(active, Some(prefab));
    assert!(
        retained,
        "source reuse must not reopen or discard the existing Prefab history"
    );
}

#[test]
fn late_alias_is_rejected_after_preparation_and_before_predecessor_retirement() {
    for waiting_for_decision in [false, true] {
        let project = TestProject::new("late-source-alias");
        let source = project.root.join("scenes/startup.scene.json");
        let candidate = project.root.join("scenes/candidate.scene.json");
        fs::write(
            &source,
            editable_authoring_scene("source").to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            &candidate,
            editable_authoring_scene("candidate")
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let mut editor = open_generic_editor_with_ready_index(&project);
        let document = editor.workspace().active_document().unwrap();
        make_authoring_name_dirty(&mut editor, document, "unsaved");
        let before = editor.drive_editor_frame(Duration::ZERO);
        let index = editor.project_source_index_snapshot().unwrap();
        let gate = (!waiting_for_decision)
            .then(|| editor.hold_next_document_open(editor::OpenTestPhase::AfterPreparation));
        if waiting_for_decision {
            assert!(
                matches!(editor.open_indexed_authoring_document(index, "scenes/candidate.scene.json", AuthoringDocumentKind::Scene), editor::EditorDocumentOpenRequestResult::AwaitingClose { document: old } if old == document)
            );
        } else {
            assert!(matches!(
                editor.request_indexed_authoring_document(
                    index,
                    "scenes/candidate.scene.json",
                    AuthoringDocumentKind::Scene
                ),
                editor::EditorDocumentOpenRequestResult::Preparing { .. }
            ));
            editor.drive_editor_frame(Duration::ZERO);
            assert!(gate.as_ref().unwrap().wait_until_started());
        }
        fs::remove_file(&candidate).unwrap();
        fs::hard_link(&source, &candidate).unwrap();
        if let Some(gate) = gate {
            gate.release();
        }
        if waiting_for_decision {
            let prompt = editor.workspace_intent_view().decision_prompt().unwrap();
            let _ =
                editor.resolve_workspace_intent(prompt, nara_tooling::EditorCloseDecision::Discard);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let result = loop {
            let frame = editor.drive_editor_frame(Duration::ZERO);
            if let Some(result) = frame.document_open().result() {
                break result;
            }
            assert!(
                Instant::now() < deadline,
                "Open did not reject the late alias"
            );
            thread::sleep(Duration::from_millis(1));
        };
        let unchanged = editor.current_frame().workspace() == before.workspace();
        drive_editor_close_to_completion(&mut editor);
        assert!(
            matches!(
                result,
                nara_tooling::EditorDocumentOpenResult::Rejected {
                    code: "project.editor.document-source-already-open",
                    ..
                }
            ),
            "{result:?}"
        );
        assert!(
            unchanged,
            "late alias rejection must precede any predecessor retirement"
        );
    }
}
