use std::time::{Duration, Instant};

use nara::{
    project_host::{
        EditorProjectCloseProgress, EditorProjectOpenHost, EditorProjectOpenProgress,
        EditorProjectSession,
    },
    tooling::{EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId},
};

use super::project::{Project, recipe};

pub(super) fn open(project: &Project) -> EditorProjectSession {
    let mut host = EditorProjectOpenHost::new().with_recipe(recipe());
    let mut attempt = host.begin(project.capability(), None);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match host.advance(attempt) {
            EditorProjectOpenProgress::Opened(editor) => loop {
                let report = host.poll_shutdown();
                if report.is_complete() {
                    assert!(!report.has_cleanup_failure(), "{report:?}");
                    return editor;
                }
                assert!(Instant::now() < deadline, "Editor Open tasks did not close");
                std::thread::sleep(Duration::from_millis(1));
            },
            EditorProjectOpenProgress::Pending(next) => attempt = next,
            EditorProjectOpenProgress::Rejected(failure) => {
                panic!("public Editor Open rejected: {:?}", failure.diagnostics());
            }
        }
        assert!(
            Instant::now() < deadline,
            "public Editor Open did not settle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn applied(outcome: EditorActionOutcome) {
    assert!(
        matches!(outcome, EditorActionOutcome::Workspace(ref report) if report.applied),
        "{outcome:?}",
    );
}

pub(super) fn dispatch(
    editor: &mut EditorProjectSession,
    action: EditorAction,
) -> EditorActionOutcome {
    let revision = editor.current_frame().revision();
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(revision, [action]))
        .unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    let mut outcomes = report.into_outcomes().into_iter();
    let outcome = outcomes.next().unwrap();
    assert!(outcomes.next().is_none());
    outcome
}

pub(super) fn history(editor: &EditorProjectSession, document: EditorDocumentId) -> (usize, usize) {
    let frame = editor.current_frame();
    let history = &frame.workspace().document(document).unwrap().history;
    (history.undo_depth, history.redo_depth)
}

pub(super) fn wait(
    editor: &mut EditorProjectSession,
    ready: impl Fn(&EditorProjectSession) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready(editor) {
        assert!(
            Instant::now() < deadline,
            "public Editor operation did not settle",
        );
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn close(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(
        editor.advance_close(),
        EditorProjectCloseProgress::Complete(_),
    ) {
        assert!(
            Instant::now() < deadline,
            "public Editor Close did not settle"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
