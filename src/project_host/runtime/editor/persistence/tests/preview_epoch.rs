//! A physically held file publication must outlive disposable Scene projection epochs.

use super::*;

fn preview_publication(
    editor: &mut EditorProjectSession,
) -> (
    nara_render::RenderFrameSourceGeneration,
    crate::project_host::EditorPreviewContentStamp,
) {
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let candidate = editor
        .take_editor_preview_candidate()
        .expect("the bound Scene publishes a preview");
    assert_eq!(frame.scene_preview_document(), Some(candidate.document()));
    assert_eq!(
        editor.workspace.document_revision(candidate.document()),
        Some(candidate.revision())
    );
    let content = candidate.content_stamp();
    (candidate.into_parts().3.source_generation(), content)
}

#[test]
fn held_save_keeps_its_captured_source_through_preview_epochs_and_reopen() {
    let fixture = Fixture::new();
    let mut editor = fixture.editor();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let document = editor.workspace.active_document().unwrap();
        editor.configure_editor_preview(nara_render::Extent2d::new(320, 180));
        edit(&mut editor, document, "captured");
        let captured_revision = editor.workspace.document_revision(document).unwrap();
        let mut previous = preview_publication(&mut editor);
        let gate = editor.hold_next_save(SaveTestPhase::BeforePublication);
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::Save {
                document: Some(document)
            }),
            EditorPersistenceRequestResult::Accepted
        );
        editor.advance_persistence();
        assert!(gate.wait_until_started());

        for epoch in 0..8 {
            edit(&mut editor, document, &format!("later-{epoch}"));
            let next = preview_publication(&mut editor);
            assert_eq!(next.0, previous.0);
            assert_ne!(next.1, previous.1);
            assert!(editor.pending_persistence.is_some());
            assert!(editor.persistence_result.is_none());
            assert!(
                !fs::read_to_string(fixture.root.join("scenes/startup.scene.json"))
                    .unwrap()
                    .contains("captured")
            );
            previous = next;
        }
        let latest = editor.workspace.document_revision(document).unwrap();
        assert_ne!(latest, captured_revision);
        gate.release();
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Saved { document: saved, revision, .. } if saved == document && revision == captured_revision)
        );
        assert_eq!(editor.workspace.document_revision(document), Some(latest));
        assert_eq!(editor.workspace.document_is_dirty(document), Some(true));
        let saved = fs::read_to_string(fixture.root.join("scenes/startup.scene.json")).unwrap();
        assert!(saved.contains("captured"));
        assert!(!saved.contains("later-"));
        assert_eq!(
            preview_publication(&mut editor),
            previous,
            "Save completion alone does not replace the accepted projection"
        );

        acknowledge(&mut editor);
        assert_eq!(
            editor.request_persistence(EditorPersistenceCommand::Reopen {
                document: Some(document)
            }),
            EditorPersistenceRequestResult::Accepted
        );
        assert!(
            matches!(settle(&mut editor), EditorPersistenceResult::Opened { document: opened, .. } if opened == document)
        );
        let reopened = preview_publication(&mut editor);
        assert_eq!(reopened.0, previous.0);
        assert_ne!(reopened.1, previous.1);
        assert_eq!(editor.workspace.document_is_dirty(document), Some(false));
        assert_eq!(preview_publication(&mut editor), reopened);
    }));
    close(&mut editor);
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}
