#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

#[path = "support/ordinary_starter_structure.rs"]
mod ordinary_starter_structure;
#[path = "support/supervised_play_project.rs"]
mod supervised_play_project;

use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use nara::tooling::{
    EditorAction, EditorActionBatch, EditorExternalPlayState, EditorFrameModel, EditorPlayAction,
    EditorPlayOperation, EditorPlayOperationResult, EditorRuntimeAvailability,
};
use nara_editor::{EditorApplication, EditorApplicationState, EditorLaunchOptions};
use supervised_play_project::CleanRoomSupervisedPlayProject;

#[test]
fn generated_starter_reaches_ready_stop_and_clean_editor_exit() {
    let project = CleanRoomSupervisedPlayProject::new();
    let mut application = open_application(project.root());
    ordinary_starter_structure::arrange_save_reopen_and_check(&project, &mut application);
    let frame = current_frame(&application);
    assert_eq!(
        frame.runtime_availability(),
        EditorRuntimeAvailability::ExternalSupervised
    );
    let scene = frame
        .workspace()
        .active_scene
        .as_ref()
        .expect("the generated starter publishes its startup Scene");
    let accepted = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Play(EditorPlayAction::Play {
                document: scene.document,
                base_revision: scene.revision,
            })],
        ),
        Duration::ZERO,
    );
    assert!(accepted.update().actions_observed());

    drive_until_external_state(
        &mut application,
        EditorExternalPlayState::Running,
        EditorPlayOperation::Play,
        Duration::from_secs(600),
    );
    let running = current_frame(&application);
    let external = running
        .play()
        .external()
        .expect("the generated starter publishes external Play state");
    assert!(external.generation().is_some());
    assert!(external.last_good_available());

    let stopping = application.submit_and_drive(
        EditorActionBatch::new(
            running.revision(),
            [EditorAction::Play(EditorPlayAction::Stop)],
        ),
        Duration::ZERO,
    );
    assert!(stopping.update().actions_observed());
    drive_until_external_state(
        &mut application,
        EditorExternalPlayState::Idle,
        EditorPlayOperation::Stop,
        Duration::from_secs(30),
    );

    let requested = application.request_exit();
    assert!(!requested.exit_ready());
    drive_until_exit_ready(&mut application, Duration::from_secs(10));
    assert!(application.current_frame().is_none());
}

fn open_application(project_root: &std::path::Path) -> EditorApplication {
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(project_root).with_profile("desktop"));
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    let _ = application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match application.state() {
            EditorApplicationState::AuthoringReady | EditorApplicationState::AuthoringDegraded => {
                return application;
            }
            EditorApplicationState::Recovery => {
                panic!("the generated starter entered Editor Recovery")
            }
            EditorApplicationState::OpeningInitial | EditorApplicationState::OpeningReplacement => {
                let _ = application.advance_project_open();
            }
            EditorApplicationState::ProjectManager => {
                panic!("the native shell must arm the generated starter open request")
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
    panic!("the generated starter did not publish an Editor workspace")
}

fn current_frame(application: &EditorApplication) -> Arc<EditorFrameModel> {
    application
        .current_frame()
        .expect("the clean-room project retains an Editor frame")
}

fn drive_until_external_state(
    application: &mut EditorApplication,
    expected_state: EditorExternalPlayState,
    expected_operation: EditorPlayOperation,
    timeout: Duration,
) {
    let deadline = Instant::now() + timeout;
    loop {
        let frame = current_frame(application);
        let external = frame
            .play()
            .external()
            .expect("the clean-room project publishes external Play state");
        if external.state() == expected_state
            && matches!(
                frame.play().result(),
                Some(EditorPlayOperationResult::Applied { operation, .. })
                    if operation == expected_operation
            )
        {
            return;
        }
        assert_ne!(
            external.state(),
            EditorExternalPlayState::Faulted,
            "the clean-room Play trajectory faulted: result={:?}, output={:#?}",
            frame.play().result(),
            external.output(),
        );
        assert!(
            Instant::now() < deadline,
            "the clean-room Play trajectory did not reach {expected_state:?}/{expected_operation:?}: state={:?}, result={:?}, output={:#?}",
            external.state(),
            frame.play().result(),
            external.output(),
        );
        let _ = application
            .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
        thread::sleep(Duration::from_millis(5));
    }
}

fn drive_until_exit_ready(application: &mut EditorApplication, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        let frame = application.current_frame();
        let update = if let Some(frame) = &frame {
            application
                .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO)
                .update()
        } else {
            application.advance_project_open()
        };
        if update.exit_ready() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the clean-room Editor exit did not complete: state={:?}, output={:#?}",
            application.state(),
            frame
                .as_ref()
                .and_then(|frame| frame.play().external())
                .map(|external| external.output()),
        );
        thread::sleep(Duration::from_millis(5));
    }
}
