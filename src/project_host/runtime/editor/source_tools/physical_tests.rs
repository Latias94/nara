use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use super::tests::fixture::{Project, wait};
use super::*;
use crate::project_host::EditorProjectCloseProgress;

const WAIT: Duration = Duration::from_secs(15);

struct FailingCapture {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}

impl Drop for FailingCapture {
    fn drop(&mut self) {
        let _ = self.entered.send(());
        let _ = self.release.recv_timeout(WAIT);
        panic!("source-tool capture cleanup failed");
    }
}

#[test]
fn process_cleanup_failure_is_terminal_and_sticky_after_actual_capture_return() {
    #[derive(Clone, Copy)]
    enum Lane {
        Probe,
        Invocation,
        Retiring,
    }

    for lane in [Lane::Probe, Lane::Invocation, Lane::Retiring] {
        let project = Project::new();
        let mut editor = project.open();
        wait(&mut editor, |editor| {
            !editor.source_tool_view().has_pending_work()
        });
        let (entered, entered_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let capture = FailingCapture {
            entered,
            release: release_rx,
        };
        let task: ProcessTask = editor
            .authoring_runtime
            .persistence_tasks()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(1, TOOL_DOMAIN),
                move |_| {
                    let _capture = &capture;
                    Ok(Vec::new())
                },
            )
            .into_handle()
            .unwrap();
        let returned = task.return_observer();
        match lane {
            Lane::Probe => {
                let attempt = EditorSourceToolAttemptId::issue();
                editor.source_tools.attempt = Some(attempt.clone());
                editor.source_tools.probe = Some(Probe {
                    attempt,
                    sequence: 1,
                    definition: project.tool(),
                    task,
                    selection: None,
                });
            }
            Lane::Invocation => {
                let definition = project.tool();
                let active = EditorSourceToolActive::admit(
                    EditorSourceToolActivationId::issue(),
                    definition.owner.clone(),
                    definition.component.clone(),
                    None,
                )
                .unwrap();
                let activation = Activation {
                    active,
                    definition,
                    origin: None,
                    requirements: Vec::new(),
                };
                let request = EditorSourceToolRequestId::issue();
                let document = editor.workspace.active_document().unwrap();
                let basis = editor.document_query_basis(document).unwrap();
                editor.source_tools.active = Some(activation.clone());
                editor.source_tools.latest = Some(request.clone());
                editor.source_tools.invocation = Some(PendingInvocation {
                    invocation: Invocation {
                        activation,
                        request,
                        basis,
                        intent: EditorSourceToolIntent::Spawn {
                            name: "cleanup failure".into(),
                        },
                    },
                    sequence: 1,
                    task,
                });
            }
            Lane::Retiring => {
                task.cancel();
                assert!(task.is_finished());
                editor.source_tools.retiring.push(task);
            }
        }
        entered_rx.recv_timeout(WAIT).unwrap();
        assert_eq!(returned.state(), TaskReturnState::Pending);
        for _ in 0..3 {
            let started = Instant::now();
            editor.advance_source_tools();
            assert!(started.elapsed() < Duration::from_secs(2));
            let view = editor.source_tool_view();
            assert!(view.physical_work_pending);
            assert!(view.has_pending_work());
            assert!(!view.process_cleanup_failed);
        }

        release.send(()).unwrap();
        wait(&mut editor, |editor| {
            let view = editor.source_tool_view();
            view.process_cleanup_failed && !view.has_pending_work()
        });
        assert_eq!(returned.state(), TaskReturnState::CleanupFailed);
        let view = editor.source_tool_view();
        assert!(!view.physical_work_pending);
        assert!(!view.has_pending_work());
        assert!(view.review.is_none());
        match lane {
            Lane::Probe => {
                assert!(view.active.is_none());
                assert_eq!(
                    view.attempt_error,
                    Some(EditorSourceToolError::TaskCleanupFailed)
                );
            }
            Lane::Invocation => {
                assert_eq!(
                    view.request_error,
                    Some(EditorSourceToolError::TaskCleanupFailed)
                );
            }
            Lane::Retiring => assert!(!view.retiring),
        }
        for _ in 0..3 {
            editor.source_tools.cancel();
            editor.advance_source_tools();
            let view = editor.source_tool_view();
            assert!(view.process_cleanup_failed);
            assert!(!view.physical_work_pending);
            assert!(!view.has_pending_work());
        }

        let deadline = Instant::now() + WAIT;
        loop {
            match editor.advance_close() {
                EditorProjectCloseProgress::Complete(receipt) => {
                    assert!(receipt.task_shutdown().is_complete());
                    assert!(receipt.task_shutdown().has_cleanup_failure());
                    let view = editor.source_tool_view();
                    assert!(view.process_cleanup_failed);
                    assert!(!view.physical_work_pending);
                    assert!(!view.has_pending_work());
                    break;
                }
                progress => assert!(
                    Instant::now() < deadline,
                    "failed process cleanup did not physically close: {progress:?}"
                ),
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}
