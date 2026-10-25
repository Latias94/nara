use std::{
    sync::mpsc::{self, Sender},
    time::{Duration, Instant},
};

use nara::project_host::EditorProjectOpenPhase;
use nara_tasks::{TaskPoolKind, TaskPools, TaskRejectReason, WorkerSpawnFailureProbe};

use super::*;

pub(crate) fn preparing_application() -> EditorApplication {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/generic-editor/known-unbound");
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(root).with_profile("desktop"));
    application.native_shell_ready();
    for _ in 0..4 {
        if matches!(&application.state, EditorProductState::OpeningInitial(opening)
            if matches!(&opening.step, OpeningProjectStep::Host(attempt)
                if attempt.phase() == EditorProjectOpenPhase::StartupClosure))
        {
            return application;
        }
        application.advance_project_open();
    }
    panic!("the fixture did not reach startup submission");
}

pub(crate) fn held_startup_application() -> (EditorApplication, Sender<()>, WorkerSpawnFailureProbe)
{
    let mut application = preparing_application();
    let (release, receiver) = mpsc::channel();
    // No spawn fails here: only this real startup worker's eventual exit is held.
    let (_, probe) =
        TaskPools::with_worker_spawn_failure_for_tests(TaskPoolKind::Io, usize::MAX, || {
            TaskPools::hold_partial_worker_exit_for_tests(receiver);
            application.advance_project_open();
        });
    finish_open(&mut application);
    assert_eq!(probe.started(), 1);
    (application, release, probe)
}

fn finish_open(application: &mut EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_none() {
        assert!(Instant::now() < deadline, "startup did not publish");
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn finish_exit(application: &mut EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !application.request_exit().exit_ready() {
        assert!(Instant::now() < deadline, "startup custody did not retire");
        application.drive_editor_frame(Duration::ZERO, false, false);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn actual_startup_failure_gates_retry_and_host_only_maintenance_allows_explicit_open() {
    let mut application = preparing_application();
    let (_, probe) = TaskPools::with_worker_spawn_failure_for_tests(TaskPoolKind::Io, 0, || {
        application.advance_project_open();
    });
    assert_eq!(probe.started(), 0);
    assert_eq!(application.state(), EditorApplicationState::Recovery);
    assert!(
        matches!(&application.state, EditorProductState::Recovery(recovery)
        if recovery.diagnostics.iter().any(|diagnostic| diagnostic.phase == "Startup preparation"))
    );
    let failure = application.failed_open.as_ref().unwrap();
    assert_eq!(
        failure.startup_task_rejection().unwrap().reason,
        TaskRejectReason::WorkerStartupFailed,
    );
    assert!(failure.close_evidence().is_none());
    assert!(application.project_open_retirement_pending());
    assert!(!application.retry_project_open());
    let request = match &application.state {
        EditorProductState::Recovery(recovery) => recovery.request.clone(),
        _ => unreachable!(),
    };
    application.submit_shell_action(EditorProjectShellAction::ReturnToProjectManager);
    application.submit_shell_action(EditorProjectShellAction::OpenProject {
        project_root: request.project_root().to_path_buf(),
        profile: request.owned_profile(),
    });
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);

    // The failure may outlive the Application's slot; Host-only maintenance still owns progress.
    let failure = application.failed_open.take().unwrap();
    assert!(application.project_open_retirement_pending());
    application.advance_project_open();
    assert!(!application.project_open_retirement_pending());
    assert!(failure.startup_tasks_close_report().unwrap().is_complete());
    assert!(application.open_host_tasks_close_report().is_complete());
    for _ in 0..3 {
        application.advance_project_open();
        application.drive_editor_frame(Duration::ZERO, false, false);
    }
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    application.submit_shell_action(EditorProjectShellAction::OpenProject {
        project_root: request.project_root().to_path_buf(),
        profile: request.owned_profile(),
    });
    finish_open(&mut application);
    assert!(!application.open_host_tasks_close_report().is_complete());
    finish_exit(&mut application);
    assert!(application.open_host_tasks_close_report().is_complete());
}

#[test]
fn healthy_open_host_final_exit_waits_in_every_shell_state_and_stays_irreversible() {
    for origin in ["manager", "recovery", "opening", "authoring"] {
        let (mut application, release, probe) = held_startup_application();
        let request = application.state.workspace().unwrap().request.clone();
        if origin != "authoring" {
            application.request_close_project();
            let deadline = Instant::now() + Duration::from_secs(10);
            while application.current_frame().is_some() {
                assert!(Instant::now() < deadline, "CloseProject did not finish");
                assert!(
                    !application
                        .drive_editor_frame(Duration::ZERO, false, false)
                        .exit_ready()
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(!application.final_exit_committed);
            assert!(!application.open_host_tasks_close_report().is_complete());
            assert!(!application.project_open_retirement_pending());
        }
        if origin == "recovery" || origin == "opening" {
            application.submit_shell_action(EditorProjectShellAction::OpenProject {
                project_root: if origin == "recovery" {
                    PathBuf::new()
                } else {
                    request.project_root().to_path_buf()
                },
                profile: request.owned_profile(),
            });
        }
        let update = if origin == "manager" {
            application.submit_shell_action(EditorProjectShellAction::Exit)
        } else {
            application.request_exit()
        };
        assert!(!update.exit_ready(), "{origin}");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !application.final_exit_committed {
            assert!(
                Instant::now() < deadline,
                "Exit was not committed: {origin}"
            );
            assert!(
                !application
                    .drive_editor_frame(Duration::ZERO, false, false)
                    .exit_ready()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        for _ in 0..3 {
            assert!(application.has_pending_exit());
            assert!(application.project_open_retirement_pending());
            assert!(!application.advance_project_open().exit_ready());
            assert!(
                !application
                    .drive_editor_frame(Duration::ZERO, false, false)
                    .exit_ready()
            );
            assert!(!application.request_exit().exit_ready());
        }
        assert_eq!(probe.exited(), 0);
        release.send(()).unwrap();
        finish_exit(&mut application);
        assert_eq!(probe.exited(), 1);
        let report = application.open_host_tasks_close_report();
        assert!(report.is_complete());
        assert_eq!(report.tasks().for_kind(TaskPoolKind::Io).joined_workers, 1);
        assert!(application.request_exit().exit_ready());
        assert!(!application.retry_project_open());
        assert!(!application.request_project_replacement(
            request.project_root().to_path_buf(),
            request.owned_profile(),
        ));
        let state = application.state();
        application.submit_shell_action(EditorProjectShellAction::OpenProject {
            project_root: request.project_root().to_path_buf(),
            profile: request.owned_profile(),
        });
        application.native_shell_ready();
        assert_eq!(application.state(), state);
        assert_eq!(application.open_host_tasks_close_report(), report);
    }
}

#[test]
fn exit_before_native_ready_cannot_rearm_the_launch_request() {
    let mut application = EditorApplication::new(EditorLaunchOptions::new("unused-project"));
    assert!(application.request_exit().exit_ready());
    assert!(!application.native_shell_ready().actions_observed());
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    assert!(application.advance_project_open().exit_ready());
}

#[test]
fn dirty_exit_cancel_keeps_healthy_startup_facilities_available() {
    use nara::{
        scene::{
            SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
            ScenePlacement, ScenePlacementOwner,
        },
        tooling::{EditorCloseDecision, EditorDocumentMutation},
    };

    let (mut application, release, probe) = held_startup_application();
    let request = application.state.workspace().unwrap().request.clone();
    let frame = application.current_frame().unwrap();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyScenePatch {
                    document: scene.document,
                    base_revision: scene.revision,
                    patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                        entity: SceneEntityRecord::new(
                            SceneEntityId::new("cancel-host-close").unwrap(),
                        ),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                },
            )],
        ),
        Duration::ZERO,
    );
    assert!(!application.request_exit().exit_ready());
    let frame = application.current_frame().unwrap();
    let prompt = frame.workspace_intent().decision_prompt().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(
                prompt.resolve(EditorCloseDecision::Cancel),
            )],
        ),
        Duration::ZERO,
    );
    for _ in 0..3 {
        application.drive_editor_frame(Duration::ZERO, false, false);
    }
    assert!(!application.has_pending_exit());
    assert!(!application.final_exit_committed);
    assert!(!application.project_open_retirement_pending());
    assert!(!application.open_host_tasks_close_report().is_complete());
    assert!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .active_scene
            .as_ref()
            .unwrap()
            .dirty
    );
    assert_eq!(probe.started(), 1);
    assert_eq!(probe.exited(), 0);

    // CloseProject retires the dirty Session, not its reusable startup facility.
    assert!(
        !application
            .submit_project_action(EditorProjectAction::CloseProject)
            .exit_ready()
    );
    let frame = application.current_frame().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(
                frame
                    .workspace_intent()
                    .decision_prompt()
                    .unwrap()
                    .resolve(EditorCloseDecision::Discard),
            )],
        ),
        Duration::ZERO,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_some() {
        assert!(Instant::now() < deadline, "CloseProject did not finish");
        assert!(
            !application
                .drive_editor_frame(Duration::ZERO, false, false)
                .exit_ready()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    assert_eq!(probe.started(), 1);
    assert_eq!(probe.exited(), 0);
    let report = application.open_host_tasks_close_report();
    assert!(!report.is_complete());
    assert_eq!(report.tasks().for_kind(TaskPoolKind::Io).joined_workers, 0);

    application.submit_shell_action(EditorProjectShellAction::OpenProject {
        project_root: request.project_root().to_path_buf(),
        profile: request.owned_profile(),
    });
    finish_open(&mut application);
    assert!(application.current_frame().is_some());
    assert!(!application.final_exit_committed);
    assert!(!application.project_open_retirement_pending());
    assert_eq!(probe.started(), 1);
    assert_eq!(probe.exited(), 0);
    let report = application.open_host_tasks_close_report();
    assert!(!report.is_complete());
    assert_eq!(report.tasks().for_kind(TaskPoolKind::Io).joined_workers, 0);

    release.send(()).unwrap();
    finish_exit(&mut application);
    assert_eq!(probe.exited(), 1);
    let report = application.open_host_tasks_close_report();
    assert!(report.is_complete());
    assert!(!report.has_cleanup_failure());
    assert_eq!(report.tasks().for_kind(TaskPoolKind::Io).joined_workers, 1);
}
