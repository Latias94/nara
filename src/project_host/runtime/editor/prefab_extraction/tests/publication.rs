//! Actual publisher failure remains visible while the same native owner completes cleanup.

use super::*;

fn return_publication_worker(editor: &mut EditorProjectSession) {
    let mut pending = editor.prefab_extraction.publication_work.take().unwrap();
    let deadline = Instant::now() + WAIT;
    let publication = loop {
        if let Some(returned) = pending.poll() {
            break returned.expect("the actual publisher retains its original owner");
        }
        assert!(
            Instant::now() < deadline,
            "the native publication did not physically return"
        );
        thread::sleep(Duration::from_millis(1));
    };
    // This is the real returned owner and its already refunded Task storage, not a phase
    // witness. Interpose only to arrange the actual next native write's failure control.
    editor.prefab_extraction.publication = Some(publication);
}

#[test]
fn public_failure_diagnostics_distinguish_native_recovery_write_from_retained_cleanup() {
    let fixture = Fixture::new();
    fixture.run(|editor| {
        let selected = open(editor, P, AuthoringDocumentKind::Prefab);
        edit(editor, selected, "branch", "reviewed-native-failure");
        let before = state(editor, selected);
        let disk = fixture.disk();
        let operation = editor.begin_prefab_extraction(request(selected)).unwrap();
        reviewed(editor, operation);
        editor.confirm_prefab_extraction(operation).unwrap();
        let control = loop {
            if let Some(control) = editor
                .prefab_extraction
                .publication
                .as_mut()
                .and_then(|publication| publication.test_fail_recovery_write())
            {
                break control;
            }
            editor.advance_prefab_extraction_publication();
            if editor.prefab_extraction.publication_work.is_some() {
                return_publication_worker(editor);
            }
            assert!(
                editor
                    .prefab_extraction
                    .publication
                    .as_ref()
                    .is_some_and(|publication| publication.failure().is_none())
            );
        };
        let attempt = catch_unwind(AssertUnwindSafe(|| {
            wait(editor, |editor| {
                !editor.prefab_extraction_view().work_pending
            });
            let view = editor.prefab_extraction_view();
            assert_eq!(view.phase, EditorPrefabExtractionPhase::RequiresReconcile);
            assert_eq!(view.refusal, Some(EditorPrefabExtractionError::Publication));
            assert!(view.can_cancel() && view.can_retry() && view.writes_fenced);
            assert!(
                editor
                    .prefab_extraction_diagnostics()
                    .unwrap()
                    .iter()
                    .any(|diagnostic| {
                        diagnostic.code().as_str()
                            == "project.editor.prefab-extraction-recovery-publication-refused"
                    })
            );
            editor.cancel_prefab_extraction(operation).unwrap();
            wait(editor, |editor| {
                !editor.prefab_extraction_view().work_pending
            });
            let view = editor.prefab_extraction_view();
            assert_eq!(view.phase, EditorPrefabExtractionPhase::RequiresReconcile);
            assert!(view.writes_fenced && view.can_retry());
            assert!(
                editor
                    .prefab_extraction_diagnostics()
                    .unwrap()
                    .iter()
                    .any(|diagnostic| {
                        diagnostic.code().as_str()
                            == "project.editor.prefab-extraction-cleanup-pending"
                    })
            );
            editor.cancel_prefab_extraction(operation).unwrap();
            assert!(!editor.prefab_extraction_view().work_pending);
            assert!(editor.prefab_extraction_view().can_retry());
            assert_eq!(state(editor, selected), before);
        }));
        // Actual cleanup remains held even if a diagnostic assertion fails. Release that
        // native control and finish the original operation before the fixture closes.
        control.release();
        if !editor.prefab_extraction.cancelled && editor.prefab_extraction_view().can_cancel() {
            editor.cancel_prefab_extraction(operation).unwrap();
        }
        if editor.prefab_extraction_view().can_retry() {
            editor.retry_prefab_extraction(operation).unwrap();
        }
        wait(editor, |editor| {
            editor.prefab_extraction_view().phase == EditorPrefabExtractionPhase::Terminal
        });
        cancelled(editor, operation);
        assert_eq!(state(editor, selected), before);
        assert_eq!(fixture.disk(), disk);
        if let Err(payload) = attempt {
            resume_unwind(payload);
        }
    });
}
