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
fn late_alias_is_rejected_after_preparation_before_additive_publication() {
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
    let gate = editor.hold_next_document_open(editor::OpenTestPhase::AfterPreparation);
    assert!(matches!(
        editor.request_indexed_authoring_document(
            index,
            "scenes/candidate.scene.json",
            AuthoringDocumentKind::Scene
        ),
        editor::EditorDocumentOpenRequestResult::Preparing { .. }
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert!(gate.wait_until_started());
    fs::remove_file(&candidate).unwrap();
    fs::hard_link(&source, &candidate).unwrap();
    gate.release();
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
    assert!(editor.workspace_intent_view().decision_prompt().is_none());
    assert!(editor.workspace_intent_view().result().is_none());
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
        "late alias rejection must preserve every existing document and its history"
    );
}

#[test]
fn pending_exit_rejects_anchored_prefab_activation_and_new_source_admission() {
    for requested_source in ["existing", "new"] {
        let project = TestProject::new("prefab-source-pending-exit");
        let mut entities = vec![editable_name_entity("scene")];
        for name in ["existing", "new"] {
            fs::write(
                project.root.join(format!("prefabs/{name}.prefab.json")),
                editable_authoring_prefab(name).to_json_string().unwrap(),
            )
            .unwrap();
            let entity = nara_identity::SceneEntityId::new(format!("{name}-anchor")).unwrap();
            let mut anchor = SceneEntityRecord::new(entity.clone());
            anchor.prefab = Some(PrefabInstance {
                source: AssetRef::path(format!("{name}.prefab.json")).unwrap(),
                overrides: ScenePatchDocument::default(),
            });
            entities.push(anchor);
        }
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots(entities)
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        let mut editor = open_generic_editor_with_ready_index(&project);
        let scene = editor.workspace().active_document().unwrap();
        let index = editor.project_source_index_snapshot().unwrap();
        let existing = opened_document(editor.open_indexed_authoring_document(
            index,
            "prefabs/existing.prefab.json",
            AuthoringDocumentKind::Prefab,
        ));
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument {
                    document: scene
                })
                .applied
        );
        let anchor =
            nara_identity::SceneEntityId::new(format!("{requested_source}-anchor")).unwrap();
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::SelectEntity {
                    document: scene,
                    entity: Some(anchor.clone()),
                })
                .applied
        );
        make_authoring_name_dirty(&mut editor, scene, "exit-decision-pending");
        assert_eq!(
            editor.request_workspace_intent(EditorWorkspaceIntent::Exit),
            nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
        );
        let pending = editor.workspace_intent_view();
        assert_eq!(pending.decision_document(), Some(scene));
        assert_eq!(
            pending.phase(),
            Some(nara_tooling::EditorWorkspaceIntentPhase::AwaitingDecision)
        );
        let frame = editor.drive_editor_frame(Duration::ZERO);
        let report = editor
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Navigate(
                    EditorNavigationAction::OpenPrefabSource {
                        document: scene,
                        base_revision: editor.workspace().document_revision(scene).unwrap(),
                        entity: anchor,
                    },
                )],
            ))
            .unwrap();
        let [EditorActionOutcome::Workspace(outcome)] = report.outcomes() else {
            panic!("{report:?}");
        };
        assert!(!outcome.applied);
        assert!(outcome.opened_document.is_none());
        assert!(report_has_code(
            &outcome.diagnostics,
            "project.editor.document-open-busy"
        ));
        let after = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(after.workspace(), frame.workspace());
        assert_eq!(editor.workspace_intent_view(), pending);
        assert_eq!(editor.workspace().active_document(), Some(scene));
        assert_eq!(editor.workspace().len(), 2);
        assert!(editor.workspace().prefab(existing).is_some());
        assert_eq!(
            editor.resolve_workspace_intent(
                pending.decision_prompt().unwrap(),
                nara_tooling::EditorCloseDecision::Cancel,
            ),
            nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
        );
        drive_editor_close_to_completion(&mut editor);
    }
}

#[cfg(feature = "runtime-ui")]
#[test]
fn trusted_anchored_prefabs_keep_independent_dirty_sources_and_exact_save_receipts() {
    let project = TestProject::new("trusted-two-prefab-sources");
    let mut anchors = Vec::new();
    let mut instances = Vec::new();
    for name in ["first", "second"] {
        fs::write(
            project.root.join(format!("prefabs/{name}.prefab.json")),
            editable_authoring_prefab(name).to_json_string().unwrap(),
        )
        .unwrap();
        let entity = nara_identity::SceneEntityId::new(format!("{name}-anchor")).unwrap();
        let mut instance = SceneEntityRecord::new(entity.clone());
        instance.prefab = Some(PrefabInstance {
            source: AssetRef::path(format!("{name}.prefab.json")).unwrap(),
            overrides: ScenePatchDocument::default(),
        });
        instances.push(instance);
        anchors.push(entity);
    }
    let scene_path = project.root.join("scenes/startup.scene.json");
    fs::write(
        &scene_path,
        SceneDocument::from_ordered_roots(instances)
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let scene_bytes = fs::read(&scene_path).unwrap();
    let mut editor = editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let scene = editor.workspace().active_document().unwrap();
    let mut documents = Vec::new();
    let mut first_dirty = None;
    for (name, anchor) in ["first", "second"].into_iter().zip(&anchors) {
        assert!(
            editor
                .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument {
                    document: scene
                })
                .applied
        );
        editor.drive_editor_frame(Duration::ZERO);
        let opened = open_prefab_from_scene(&mut editor, scene, anchor);
        assert!(opened.applied, "{:?}", opened.diagnostics);
        let document = opened.opened_document.unwrap();
        assert_eq!(
            editor
                .workspace()
                .prefab(document)
                .unwrap()
                .source_binding()
                .locator(),
            format!("{name}.prefab.json")
        );
        if let Some(before) = &first_dirty {
            let first = documents[0];
            let mut retained = editor.workspace_model().document(first).unwrap().clone();
            retained.active = true;
            assert_eq!(&retained, before);
        }
        let base_revision = editor.workspace().document_revision(document).unwrap();
        let report = editor.apply_workspace_command(EditorWorkspaceCommand::ApplyPrefabPatch {
            document,
            base_revision,
            patch: ScenePatchDocument::new([ScenePatchOperation::SetField {
                entity: nara_identity::SceneEntityId::new("editable-name").unwrap(),
                component: ComponentTypeId::new("nara.scene.Name"),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("value"),
                value: ComponentValue::String(format!("saved-{name}")),
            }]),
        });
        assert!(report.applied, "{:?}", report.diagnostics);
        assert!(editor.workspace().document_is_dirty(document).unwrap());
        documents.push(document);
        if first_dirty.is_none() {
            first_dirty = Some(editor.workspace_model().document(document).unwrap().clone());
        }
    }
    assert_ne!(documents[0], documents[1]);
    assert_eq!(editor.workspace().len(), 3);
    assert!(editor.workspace_intent_view().result().is_none());
    assert!(editor.workspace_intent_view().decision_prompt().is_none());
    for (index, name) in ["first", "second"].into_iter().enumerate() {
        let document = documents[index];
        let other = documents[1 - index];
        let other_name = ["first", "second"][1 - index];
        let other_path = project
            .root
            .join(format!("prefabs/{other_name}.prefab.json"));
        let other_bytes = fs::read(&other_path).unwrap();
        let other_before = editor.workspace_model().document(other).unwrap().clone();
        let revision = editor.workspace().document_revision(document).unwrap();
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(document)
            }),
            nara_tooling::EditorPersistenceRequestResult::Accepted
        );
        let saved = drive_persistence_to_terminal(&mut editor);
        let bytes = fs::read(project.root.join(format!("prefabs/{name}.prefab.json"))).unwrap();
        let digest = crate::project_host::persistence::editor_digest(
            nara_fs::ContentDigest::of_bytes(&bytes),
        );
        assert_eq!(
            saved.persistence().result(),
            Some(EditorPersistenceResult::Saved {
                document,
                revision,
                digest
            })
        );
        assert!(
            String::from_utf8(bytes)
                .unwrap()
                .contains(&format!("saved-{name}"))
        );
        let slot = editor.workspace().prefab(document).unwrap();
        assert_eq!(slot.saved_revision(), revision);
        assert_eq!(slot.saved_digest(), Some(digest));
        assert!(!slot.is_dirty());
        assert_eq!(saved.workspace().document(other), Some(&other_before));
        assert_eq!(fs::read(&other_path).unwrap(), other_bytes);
        assert_eq!(fs::read(&scene_path).unwrap(), scene_bytes);
        assert_eq!(editor.workspace().active_document(), Some(documents[1]));
        assert!(matches!(
            saved.project_content(),
            EditorProjectContentView::RefreshPending {
                document: observed, revision: observed_revision, saved_digest, ..
            } if observed == document && observed_revision == revision && saved_digest == digest
        ));
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
            nara_tooling::EditorPersistenceRequestResult::Accepted
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !editor.current_frame().project_content().is_current() {
            assert!(
                Instant::now() < deadline,
                "the saved Prefab refresh did not settle"
            );
            editor.drive_editor_frame(Duration::ZERO);
            thread::sleep(Duration::from_millis(1));
        }
    }
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        editor::EditorProjectCloseProgress::Complete(_)
    ));
}
