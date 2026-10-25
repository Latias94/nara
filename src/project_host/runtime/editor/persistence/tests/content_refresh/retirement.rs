use super::*;
use crate::project_content::ProjectContentRevision;
use crate::project_host::runtime::editor::test_fixtures::HeldCompute;
use nara_tasks::{TaskPoolKind, TaskReturnState, TaskTerminal};
use std::sync::mpsc;

const WAIT: Duration = Duration::from_secs(5);

fn settle_authoring_retirement(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + WAIT;
    while editor.authoring_retirements.is_pending() {
        editor.advance_authoring_retirement();
        assert!(Instant::now() < deadline, "Runtime did not close");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn assert_closed_workers(
    editor: &mut EditorProjectSession,
    joined: [usize; 3],
    cleanup_failed: bool,
) {
    close(editor);
    let completed = editor.advance_close();
    let EditorProjectCloseProgress::Complete(receipt) = &completed else {
        panic!("all authoring Runtimes must physically close: {completed:?}");
    };
    let report = receipt.task_shutdown();
    assert!(report.is_complete());
    for (kind, workers) in TaskPoolKind::ALL.into_iter().zip(joined) {
        assert_eq!(
            report.for_kind(kind).joined_workers,
            workers,
            "each activated {kind} worker must be counted exactly once"
        );
    }
    assert_eq!(report.has_cleanup_failure(), cleanup_failed);
    assert_eq!(receipt.has_authoring_cleanup_failure(), cleanup_failed);
    assert_eq!(editor.advance_close(), completed);
}

fn retirement_failure_count(editor: &EditorProjectSession) -> usize {
    editor
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code().as_str() == "project.editor.authoring-retirement-failed"
        })
        .count()
}

fn content_identity(
    editor: &EditorProjectSession,
) -> (EditorProjectContentGeneration, ProjectContentRevision) {
    (
        editor.project_content.current_generation().unwrap(),
        editor.trusted_compiled().unwrap().snapshot.revision(),
    )
}

#[test]
fn successful_prefab_refresh_retains_predecessor_and_gates_the_next_saved_generation() {
    let fixture = Fixture::new();
    let (mut editor, [p, q]) = fixture.two_prefabs();
    let original = content_identity(&editor);
    let original_custody = editor.authoring_runtime.host_custody().clone();
    let mut predecessor = HeldCompute::start(editor.authoring_runtime.persistence_tasks(), || {});
    assert_eq!(predecessor.returned.state(), TaskReturnState::Pending);

    save_entity(&mut editor, p, "first-p");
    editor.advance_project_content_refresh();
    assert!(editor.project_content.is_current());
    let first = content_identity(&editor);
    assert_ne!(first.0, original.0);
    assert_ne!(first.1, original.1);
    assert!(editor.authoring_retirements.is_pending());
    assert!(!original_custody.is_accepting());
    assert!(editor.authoring_runtime.host_custody().is_accepting());

    predecessor.handle.cancel();
    assert!(matches!(
        predecessor.handle.try_take(),
        Some(TaskTerminal::Cancelled(_))
    ));
    assert_eq!(predecessor.returned.state(), TaskReturnState::Pending);

    // The successor must borrow the transferred file Host while its predecessor is still held.
    save_entity(&mut editor, q, "queued-q");
    edit(&mut editor, q, "unsaved-q");
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen { document: Some(q) }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Opened { .. }
    ));
    acknowledge(&mut editor);
    assert_eq!(
        editor
            .workspace
            .prefab(q)
            .unwrap()
            .session()
            .document()
            .roots,
        vec![SceneEntityId::new("queued-q").unwrap()]
    );

    // This one-shot hook is consumed only after another real Runtime has been constructed.
    editor.next_authoring_refresh_hook = Some(Box::new(|_| {}));
    for late_result in [false, true] {
        if late_result {
            predecessor.release_body_until_drop();
            assert!(predecessor.handle.is_finished());
            assert_eq!(predecessor.returned.state(), TaskReturnState::Pending);
        }
        for _ in 0..4 {
            let frame = editor.drive_editor_frame(Duration::ZERO);
            assert!(frame.authoring_retirement_pending());
            assert!(matches!(
                editor.project_content,
                EditorProjectContentView::RefreshPending { .. }
            ));
            assert_eq!(content_identity(&editor), first);
            assert!(editor.next_authoring_refresh_hook.is_some());
            assert_eq!(predecessor.returned.state(), TaskReturnState::Pending);
        }
    }

    predecessor.release_and_wait(TaskReturnState::Returned);
    settle_authoring_retirement(&mut editor);
    assert!(
        !editor
            .publish_editor_frame(false)
            .authoring_retirement_pending()
    );
    let mut successor = HeldCompute::start(editor.authoring_runtime.persistence_tasks(), || {});
    assert_eq!(successor.returned.state(), TaskReturnState::Pending);
    editor.advance_project_content_refresh();
    assert!(editor.project_content.is_current());
    let second = content_identity(&editor);
    assert_ne!(second.0, first.0);
    assert_ne!(second.1, first.1);
    assert!(editor.next_authoring_refresh_hook.is_none());
    successor.handle.cancel();
    successor.release_body_until_drop();
    assert!(successor.handle.is_finished());
    assert_eq!(successor.returned.state(), TaskReturnState::Pending);
    for _ in 0..4 {
        assert!(matches!(
            editor.advance_close(),
            EditorProjectCloseProgress::Incomplete { .. }
        ));
        assert!(editor.authoring_retirements.is_pending());
        assert_eq!(successor.returned.state(), TaskReturnState::Pending);
    }
    successor.release_and_wait(TaskReturnState::Returned);
    // Two generations perform I/O and held Compute; the final unused successor stays dormant.
    assert_closed_workers(&mut editor, [4, 2, 0], false);
}

#[test]
fn rejected_started_prefab_candidate_remains_owned_and_blocks_session_close_until_return() {
    let fixture = Fixture::new();
    let (mut editor, [p, _]) = fixture.two_prefabs();
    let original = content_identity(&editor);
    let original_custody = editor.authoring_runtime.host_custody().clone();
    let (candidate_tx, candidate_rx) = mpsc::channel();
    editor.next_authoring_refresh_hook = Some(Box::new(move |runtime| {
        assert!(
            candidate_tx
                .send(HeldCompute::start(runtime.persistence_tasks(), || {}))
                .is_ok()
        );
    }));
    editor.next_project_content_generation = u64::MAX;

    save_entity(&mut editor, p, "rejected-p");
    editor.advance_project_content_refresh();
    let mut candidate = candidate_rx
        .recv_timeout(WAIT)
        .expect("capacity rejection must happen after the candidate Runtime starts");
    assert_eq!(candidate.returned.state(), TaskReturnState::Pending);
    assert!(matches!(
        editor.project_content,
        EditorProjectContentView::Stale {
            failure: EditorProjectContentRefreshFailure::CapacityExceeded,
            ..
        }
    ));
    assert_eq!(content_identity(&editor), original);
    assert!(original_custody.is_accepting());
    assert!(editor.authoring_retirements.is_pending());
    assert!(
        editor
            .drive_editor_frame(Duration::ZERO)
            .authoring_retirement_pending()
    );

    candidate.handle.cancel();
    assert!(matches!(
        candidate.handle.try_take(),
        Some(TaskTerminal::Cancelled(_))
    ));
    candidate.release_body_until_drop();
    assert!(candidate.handle.is_finished());
    assert_eq!(candidate.returned.state(), TaskReturnState::Pending);
    for _ in 0..4 {
        let progress = editor.advance_close();
        assert!(matches!(
            progress,
            EditorProjectCloseProgress::Incomplete { .. }
        ));
        assert!(!progress.task_shutdown().is_complete());
        assert!(editor.authoring_retirements.is_pending());
        assert_eq!(candidate.returned.state(), TaskReturnState::Pending);
    }

    candidate.release_and_wait(TaskReturnState::Returned);
    assert_closed_workers(&mut editor, [2, 1, 0], false);
}

#[test]
fn final_custody_refusal_preserves_incumbent_catalog_and_save_reopen_through_candidate_return() {
    let fixture = Fixture::new();
    let (mut editor, [p, q]) = fixture.two_prefabs();
    let original = content_identity(&editor);
    let original_custody = editor.authoring_runtime.host_custody().clone();
    #[cfg(feature = "runtime-ui")]
    let original_images = editor.editor_authoring_image_catalog_facts();
    let (candidate_tx, candidate_rx) = mpsc::channel();
    editor.next_authoring_refresh_hook = Some(Box::new(move |runtime| {
        let held = HeldCompute::start(runtime.persistence_tasks(), || {});
        runtime.host_custody().close_admission();
        assert!(candidate_tx.send(held).is_ok());
    }));

    save_entity(&mut editor, p, "refused-p");
    editor.advance_project_content_refresh();
    let mut candidate = candidate_rx
        .recv_timeout(WAIT)
        .expect("candidate must start");
    assert_eq!(candidate.returned.state(), TaskReturnState::Pending);
    assert!(matches!(
        editor.project_content,
        EditorProjectContentView::Stale {
            failure: EditorProjectContentRefreshFailure::PreviewRejected,
            ..
        }
    ));
    assert_eq!(content_identity(&editor), original);
    assert!(original_custody.is_accepting());
    #[cfg(feature = "runtime-ui")]
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        original_images
    );

    candidate.handle.cancel();
    assert!(matches!(
        candidate.handle.try_take(),
        Some(TaskTerminal::Cancelled(_))
    ));
    candidate.release_body_until_drop();
    assert!(candidate.handle.is_finished());
    assert_eq!(candidate.returned.state(), TaskReturnState::Pending);
    save_entity(&mut editor, q, "saved-q");
    edit(&mut editor, q, "unsaved-q");
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen { document: Some(q) }),
        EditorPersistenceRequestResult::Accepted { .. }
    ));
    assert!(matches!(
        settle(&mut editor),
        EditorPersistenceResult::Opened { .. }
    ));
    acknowledge(&mut editor);
    assert_eq!(
        editor
            .workspace
            .prefab(q)
            .unwrap()
            .session()
            .document()
            .roots,
        vec![SceneEntityId::new("saved-q").unwrap()]
    );
    for _ in 0..4 {
        assert!(
            editor
                .drive_editor_frame(Duration::ZERO)
                .authoring_retirement_pending()
        );
        assert_eq!(content_identity(&editor), original);
        assert!(original_custody.is_accepting());
        assert_eq!(candidate.returned.state(), TaskReturnState::Pending);
        #[cfg(feature = "runtime-ui")]
        assert_eq!(
            editor.editor_authoring_image_catalog_facts(),
            original_images
        );
    }
    for _ in 0..4 {
        let progress = editor.advance_close();
        assert!(matches!(
            progress,
            EditorProjectCloseProgress::Incomplete { .. }
        ));
        assert!(!progress.task_shutdown().is_complete());
        assert_eq!(candidate.returned.state(), TaskReturnState::Pending);
    }
    candidate.release_and_wait(TaskReturnState::Returned);
    assert_closed_workers(&mut editor, [2, 1, 0], false);
}

#[test]
fn retired_predecessor_or_candidate_cleanup_failure_survives_slot_reuse_and_final_close() {
    for reject_candidate in [false, true] {
        let fixture = Fixture::new();
        let (mut editor, [p, q]) = fixture.two_prefabs();
        let next_generation = editor.next_project_content_generation;
        let (candidate_tx, candidate_rx) = mpsc::channel();
        let failing = if reject_candidate {
            editor.next_project_content_generation = u64::MAX;
            editor.next_authoring_refresh_hook = Some(Box::new(move |runtime| {
                assert!(
                    candidate_tx
                        .send(HeldCompute::start(runtime.persistence_tasks(), || {
                            panic!("intentional retired candidate task panic");
                        }))
                        .is_ok()
                );
            }));
            None
        } else {
            Some(HeldCompute::start(
                editor.authoring_runtime.persistence_tasks(),
                || {
                    panic!("intentional retired predecessor task panic");
                },
            ))
        };

        save_entity(&mut editor, p, "failure-p");
        editor.advance_project_content_refresh();
        assert_eq!(editor.project_content.is_current(), !reject_candidate);
        let mut failing = failing.unwrap_or_else(|| candidate_rx.recv_timeout(WAIT).unwrap());
        assert!(editor.authoring_retirements.is_pending());
        failing.handle.cancel();
        assert!(matches!(
            failing.handle.try_take(),
            Some(TaskTerminal::Cancelled(_))
        ));
        assert_eq!(failing.returned.state(), TaskReturnState::Pending);
        assert!(!editor.authoring_retirements.failed());
        failing.release_and_wait(TaskReturnState::CleanupFailed);
        settle_authoring_retirement(&mut editor);
        assert!(editor.authoring_retirements.failed());

        // A later clean retirement may reuse the slot, but cannot erase its failure evidence.
        if reject_candidate {
            editor.next_project_content_generation = next_generation;
        }
        save_entity(&mut editor, q, "later-clean-q");
        editor.advance_project_content_refresh();
        assert!(editor.project_content.is_current());
        settle_authoring_retirement(&mut editor);
        for _ in 0..4 {
            let frame = editor.drive_editor_frame(Duration::ZERO);
            assert!(!frame.authoring_retirement_pending());
            assert!(editor.authoring_retirements.failed());
        }
        assert_eq!(retirement_failure_count(&editor), 1);
        let io_workers = if reject_candidate { 2 } else { 4 };
        assert_closed_workers(&mut editor, [io_workers, 1, 0], true);
        assert_eq!(retirement_failure_count(&editor), 1);
    }
}
