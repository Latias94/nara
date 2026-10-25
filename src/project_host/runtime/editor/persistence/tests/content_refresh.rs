use super::*;
mod retirement;
use nara_tooling::{
    EditorProjectContentAction, EditorProjectContentRefreshFailure,
    EditorProjectContentRefreshRequestResult,
};

fn save_entity(editor: &mut EditorProjectSession, document: EditorDocumentId, entity: &str) {
    edit(editor, document, entity);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    assert!(matches!(
        settle(editor),
        EditorPersistenceResult::Saved { .. }
    ));
    acknowledge(editor);
}

fn wait_for_play(editor: &mut EditorProjectSession, state: EditorPlayState) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.play_view().state() != state {
        assert!(
            Instant::now() < deadline,
            "Play did not reach {state:?}: {:?}",
            editor.play_view()
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn retry_and_runtime_retirement_revalidate_all_sources_after_the_representative_closes() {
    for runtime_retirement in [false, true] {
        for drift_other_source in [false, true] {
            let fixture = Fixture::new();
            let (mut editor, [p, q]) = fixture.two_prefabs();
            if runtime_retirement {
                let scene = editor.scene_preview_document.unwrap();
                assert!(
                    editor
                        .apply_workspace_command(EditorWorkspaceCommand::SetActiveDocument {
                            document: scene
                        })
                        .applied
                );
                assert_eq!(
                    editor.request_play(EditorPlayCommand::Play),
                    EditorPlayRequestResult::Accepted
                );
                wait_for_play(&mut editor, EditorPlayState::Running);
                let _ = editor.request_play(EditorPlayCommand::AcknowledgeResult);
                save_entity(&mut editor, p, "first-generation-p");
                editor.advance_project_content_refresh();
                assert!(editor.project_content.is_current());
                let retirement_deadline = Instant::now() + Duration::from_secs(5);
                while editor.authoring_retirements.is_pending() {
                    assert!(
                        Instant::now() < retirement_deadline,
                        "authoring predecessor did not retire"
                    );
                    editor.advance_authoring_retirement();
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            let last_good = editor.trusted_compiled().unwrap().snapshot.revision();
            save_entity(&mut editor, p, "pending-p");
            save_entity(&mut editor, q, "pending-q");
            let startup = fixture.root.join("scenes/startup.scene.json");
            let original_scene = fs::read(&startup).unwrap();
            if !runtime_retirement {
                fs::write(&startup, b"invalid Scene JSON").unwrap();
            }
            editor.advance_project_content_refresh();
            let failure = if runtime_retirement {
                EditorProjectContentRefreshFailure::RuntimeRetirementRequired
            } else {
                EditorProjectContentRefreshFailure::InvalidContent
            };
            assert!(
                matches!(editor.project_content, EditorProjectContentView::Stale { failure: observed, .. } if observed == failure),
                "{:?}",
                editor.project_content
            );
            if !runtime_retirement {
                fs::write(startup, original_scene).unwrap();
            }
            assert_eq!(
                editor
                    .request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: p }),
                EditorWorkspaceIntentRequestResult::Accepted
            );
            editor.advance_workspace_retirement();
            assert!(editor.workspace.prefab(p).is_none());
            assert!(editor.acknowledge_workspace_intent_result());
            if drift_other_source {
                let path = fixture.root.join("prefabs/q.json");
                let mut bytes = fs::read(&path).unwrap();
                bytes.extend_from_slice(b"\n");
                fs::write(path, bytes).unwrap();
            }
            if runtime_retirement {
                assert_eq!(editor.play_view().state(), EditorPlayState::Running);
                assert_eq!(
                    editor.request_play(EditorPlayCommand::Stop),
                    EditorPlayRequestResult::Accepted
                );
                wait_for_play(&mut editor, EditorPlayState::Empty);
                editor.drive_editor_frame(Duration::ZERO);
            } else {
                assert_eq!(
                    editor
                        .request_project_content_refresh(EditorProjectContentAction::RetryRefresh),
                    EditorProjectContentRefreshRequestResult::Accepted
                );
            }
            editor.advance_project_content_refresh();
            let result = editor.project_content;
            let actual_content = editor.trusted_compiled().unwrap().snapshot.revision();
            let retirement_deadline = Instant::now() + Duration::from_secs(5);
            while editor.authoring_retirements.is_pending() {
                assert!(
                    Instant::now() < retirement_deadline,
                    "authoring predecessor did not retire"
                );
                editor.advance_project_content_refresh();
                std::thread::sleep(Duration::from_millis(1));
            }
            let generations = editor
                .trusted_compiled()
                .unwrap()
                .content_loader
                .budget_snapshot()
                .active_generations();
            close(&mut editor);
            if drift_other_source {
                assert!(
                    matches!(result, EditorProjectContentView::Stale {
                    document,
                    failure: EditorProjectContentRefreshFailure::SavedSourceChanged,
                    ..
                } if document == p),
                    "a closed representative must not hide Q's drift: {result:?}"
                );
                assert_eq!(actual_content, last_good);
            } else {
                assert!(
                    result.is_current(),
                    "all obligations must recover together: {result:?}"
                );
                assert_ne!(actual_content, last_good);
            }
            assert_eq!(
                generations, 1,
                "a retired generation or failed successor leaked"
            );
        }
    }
}
