use super::*;
use crate::project_host::runtime::editor::test_fixtures::HeldCompute;
use crate::project_host::{
    EditorProjectOpenFailure, EditorProjectOpenFailureImpact, EditorProjectOpenFailureSource,
    EditorProjectOpenHost, EditorProjectOpenPhase, EditorProjectOpenProgress,
};
use nara_app::{RuntimeCandidateRetirementState, runtime_quarantine_status};
use nara_tasks::{TaskReturnState, TaskTerminal};
use std::sync::mpsc;

const WAIT: Duration = Duration::from_secs(5);

fn assert_unpublished_scene_failure(
    failure: &EditorProjectOpenFailure,
    diagnostics: &nara_diagnostic::DiagnosticReport,
) {
    assert_eq!(failure.phase(), EditorProjectOpenPhase::SessionPublication);
    assert_eq!(
        failure.source(),
        EditorProjectOpenFailureSource::SessionPublication
    );
    assert_eq!(
        failure.impact(),
        EditorProjectOpenFailureImpact::CandidateNotPublished
    );
    assert_eq!(failure.relative_source(), None);
    assert!(!failure.repair_hint().is_empty());
    assert_eq!(failure.diagnostics(), diagnostics);
}

#[test]
fn rejected_initial_open_retains_runtime_until_body_and_late_result_return_then_recovers() {
    let fixture = Fixture::new();
    let quarantine_before = runtime_quarantine_status().current_thread_retained();
    let startup = fixture.root.join("scenes/startup.scene.json");
    let original = fs::read(&startup).unwrap();
    let mut host = EditorProjectOpenHost::new();
    let (started_tx, started_rx) = mpsc::channel();
    host.next_authoring_publication_hook = Some(Box::new(move |runtime| {
        let held = HeldCompute::start(runtime.persistence_tasks(), || {});
        assert_eq!(held.returned.state(), TaskReturnState::Pending);
        let custody = runtime.host_custody().clone();
        assert!(started_tx.send((held, custody)).is_ok());
    }));

    let deadline = Instant::now() + WAIT;
    let mut attempt = host.begin(fixture.capability(), None);
    while attempt.phase() != EditorProjectOpenPhase::SessionPublication {
        attempt = match host.advance(attempt) {
            EditorProjectOpenProgress::Pending(next) => next,
            EditorProjectOpenProgress::Rejected(failure) => {
                panic!(
                    "startup preparation must succeed: {:?}",
                    failure.diagnostics()
                );
            }
            EditorProjectOpenProgress::Opened(mut editor) => {
                close(&mut editor);
                panic!("Open must expose the pending SessionPublication boundary");
            }
        };
        assert!(
            Instant::now() < deadline,
            "startup preparation did not return"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(host.next_authoring_publication_hook.is_some());

    // Preserve valid syntax and the native file; invalidate only the admitted source digest.
    let mut changed = original.clone();
    changed.extend_from_slice(b"\n ");
    fs::write(&startup, &changed).unwrap();
    let mut failure = match host.advance(attempt) {
        EditorProjectOpenProgress::Rejected(failure) => failure,
        EditorProjectOpenProgress::Opened(mut editor) => {
            close(&mut editor);
            panic!("changed startup bytes must prevent Session publication");
        }
        EditorProjectOpenProgress::Pending(_) => {
            panic!("the publication attempt must report its source rejection");
        }
    };
    let (mut held, custody) = started_rx
        .recv_timeout(WAIT)
        .expect("source rejection must happen after the complete authoring Runtime starts");
    assert!(host.next_authoring_publication_hook.is_none());
    assert!(custody.is_accepting());
    let diagnostics = failure.diagnostics().clone();
    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code().as_str())
            .collect::<Vec<_>>(),
        ["project.editor.scene-validation-failed"]
    );
    assert_unpublished_scene_failure(&failure, &diagnostics);
    assert_eq!(
        failure.retirement_state(),
        RuntimeCandidateRetirementState::Retiring
    );
    assert!(failure.close_evidence().is_some());
    assert_eq!(held.returned.state(), TaskReturnState::Pending);
    assert_eq!(
        failure.advance_retirement(),
        RuntimeCandidateRetirementState::Retiring
    );

    assert!(held.handle.cancel());
    assert!(matches!(
        held.handle.try_take(),
        Some(TaskTerminal::Cancelled(_))
    ));
    for late_result in [false, true] {
        if late_result {
            held.release_body_until_drop();
            assert!(held.handle.is_finished());
            assert_eq!(held.returned.state(), TaskReturnState::Pending);
        }
        for _ in 0..4 {
            let before = Instant::now();
            assert_eq!(
                failure.advance_retirement(),
                RuntimeCandidateRetirementState::Retiring
            );
            assert!(
                before.elapsed() < WAIT,
                "retirement polling must be bounded"
            );
            assert_eq!(
                failure.retirement_state(),
                RuntimeCandidateRetirementState::Retiring
            );
            assert_eq!(held.returned.state(), TaskReturnState::Pending);
            // Closing the captured custody proves this is the same complete Runtime owner.
            assert!(!custody.is_accepting());
            let evidence = failure
                .close_evidence()
                .expect("retain Runtime close evidence");
            assert!(!evidence.has_terminal_failure());
            assert!(
                evidence
                    .pending_participants()
                    .iter()
                    .any(|pending| pending.participant().as_str() == "nara.tasks.pools")
            );
            assert_unpublished_scene_failure(&failure, &diagnostics);
            assert_eq!(
                runtime_quarantine_status().current_thread_retained(),
                quarantine_before
            );
        }
    }

    held.release_and_wait(TaskReturnState::Returned);
    let deadline = Instant::now() + WAIT;
    while failure.retirement_state() != RuntimeCandidateRetirementState::Retired {
        assert!(
            Instant::now() < deadline,
            "the unpublished Runtime did not retire"
        );
        let _ = failure.advance_retirement();
        std::thread::sleep(Duration::from_millis(1));
    }
    let evidence = failure
        .close_evidence()
        .expect("completed retirement must retain its Runtime evidence")
        .clone();
    assert!(evidence.pending_participants().is_empty());
    assert!(!evidence.has_terminal_failure());
    for _ in 0..4 {
        assert_eq!(
            failure.advance_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
        assert_eq!(failure.close_evidence(), Some(&evidence));
        assert_unpublished_scene_failure(&failure, &diagnostics);
    }
    assert_eq!(fs::read(&startup).unwrap(), changed);
    drop(failure);
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        quarantine_before
    );

    fs::write(&startup, original).unwrap();
    let deadline = Instant::now() + WAIT;
    let mut attempt = host.begin(fixture.capability(), None);
    let mut editor = loop {
        match host.advance(attempt) {
            EditorProjectOpenProgress::Pending(next) => attempt = next,
            EditorProjectOpenProgress::Opened(editor) => break editor,
            EditorProjectOpenProgress::Rejected(failure) => {
                panic!("restored source must open: {:?}", failure.diagnostics());
            }
        }
        assert!(
            Instant::now() < deadline,
            "restored project Open did not return"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    let document = editor.workspace.active_document().unwrap();
    assert_eq!(
        editor.workspace.document_kind(document),
        Some(EditorDocumentKind::Scene)
    );
    edit(&mut editor, document, "saved-after-rejected-open");
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Saved { document: saved, .. } if saved == document
    ));
    acknowledge(&mut editor);
    let saved = fs::read(&startup).unwrap();
    assert_ne!(saved, changed);
    edit(&mut editor, document, "discarded-after-rejected-open");
    assert!(editor.workspace.scene(document).unwrap().is_dirty());
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document)
        }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Opened { document: opened, .. } if opened == document
    ));
    acknowledge(&mut editor);
    let scene = editor.workspace.scene(document).unwrap();
    assert_eq!(
        scene.session().document().roots,
        [SceneEntityId::new("saved-after-rejected-open").unwrap()]
    );
    assert!(!scene.is_dirty());
    assert_eq!(fs::read(&startup).unwrap(), saved);
    close(&mut editor);
    let completed = editor.advance_close();
    let EditorProjectCloseProgress::Complete(receipt) = &completed else {
        panic!("the recovered Editor must physically close: {completed:?}");
    };
    assert!(receipt.task_shutdown().is_complete());
    assert!(!receipt.task_shutdown().has_cleanup_failure());
    assert!(!receipt.has_authoring_cleanup_failure());
    assert_eq!(editor.advance_close(), completed);
    drop(editor);
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        quarantine_before
    );
}
