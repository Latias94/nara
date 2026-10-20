#![cfg(feature = "editor")]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

use nara::project_host::{
    EditorProjectOpenHost, EditorProjectOpenProgress, EditorProjectSession,
    admit_supervised_desktop_source_receipt,
};
use nara::tooling::{
    EditorAction, EditorActionBatch, EditorExternalPlayState, EditorPlayAction,
    EditorPlayOperation, EditorPlayOperationResult, EditorPlayRejection, EditorRuntimeAvailability,
};
use nara_editor::{EditorApplication, EditorApplicationState, EditorLaunchOptions};
use project_content_fixture::project_root_capability;

#[test]
fn generic_authoring_source_receipt_preserves_custom_runtime_components() {
    let session = open_generic_authoring_session();

    assert_eq!(
        session.project_readiness().runtime(),
        EditorRuntimeAvailability::ExternalSupervised
    );
    let encoded = session
        .supervised_desktop_source_receipt()
        .expect("generic authoring must admit custom project components into the source receipt");
    let authority = admit_supervised_desktop_source_receipt(&encoded)
        .expect("the Editor-owned source receipt must remain canonical");

    assert_eq!(authority.profile_name(), "desktop");
    assert!(authority.cargo_manifest_digest().is_some());
    assert!(authority.cargo_lock_digest().is_some());
}

#[test]
fn external_supervised_play_rejects_in_process_controls_without_starting_a_process() {
    let mut application = open_reference_game_application();

    let frame = application
        .current_frame()
        .expect("Reference Game must publish an authoring frame");
    assert_eq!(
        frame.runtime_availability(),
        EditorRuntimeAvailability::ExternalSupervised
    );
    assert!(frame.workspace().active_scene.is_some());
    let update = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Play(EditorPlayAction::Pause)],
        ),
        std::time::Duration::ZERO,
    );
    assert!(update.actions_observed());

    let frame = application
        .current_frame()
        .expect("the rejected control must retain the authoring frame");
    assert_eq!(frame.play().generation(), None);
    assert_eq!(
        frame.play().external().map(|external| external.state()),
        Some(EditorExternalPlayState::Idle)
    );
    assert_eq!(
        frame.play().result(),
        Some(EditorPlayOperationResult::Rejected {
            operation: EditorPlayOperation::Pause,
            reason: EditorPlayRejection::UnsupportedControl(
                nara::tooling::EditorPlayControl::Pause,
            ),
        })
    );
}

#[test]
#[cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
fn external_supervised_stop_then_editor_exit_retires_the_owner() {
    let mut application = open_reference_game_application();
    let frame = current_frame(&application);
    let scene = frame
        .workspace()
        .active_scene
        .as_ref()
        .expect("Reference Game must publish a startup Scene");

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
    assert!(accepted.actions_observed());

    drive_until_external_state(
        &mut application,
        EditorExternalPlayState::Running,
        EditorPlayOperation::Play,
        Duration::from_secs(600),
    );

    let first_generation = current_frame(&application)
        .play()
        .external()
        .and_then(|external| external.generation())
        .expect("the first Reference Game launch must publish a generation");
    let frame = current_frame(&application);
    let restarted = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Play(EditorPlayAction::Restart)],
        ),
        Duration::ZERO,
    );
    assert!(restarted.actions_observed());
    drive_until_external_state(
        &mut application,
        EditorExternalPlayState::Running,
        EditorPlayOperation::Restart,
        Duration::from_secs(30),
    );
    let restarted_generation = current_frame(&application)
        .play()
        .external()
        .and_then(|external| external.generation())
        .expect("Restart must publish the admitted generation");
    assert_eq!(restarted_generation, first_generation);

    let frame = current_frame(&application);
    let stopping = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Play(EditorPlayAction::Stop)],
        ),
        Duration::ZERO,
    );
    assert!(stopping.actions_observed());
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

fn open_generic_authoring_session() -> EditorProjectSession {
    let mut host = EditorProjectOpenHost::new();
    let mut attempt = host.begin(project_root_capability(), Some("desktop".to_owned()));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match host.advance(attempt) {
            EditorProjectOpenProgress::Pending(next) => {
                attempt = next;
                assert!(
                    Instant::now() < deadline,
                    "Reference Game generic Open timed out"
                );
                thread::sleep(Duration::from_millis(1));
            }
            EditorProjectOpenProgress::Opened(session) => return session,
            EditorProjectOpenProgress::Rejected(failure) => {
                panic!("Reference Game generic authoring open failed: {failure:#?}")
            }
        }
    }
}

fn open_reference_game_application() -> EditorApplication {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(root).with_profile("desktop"));
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    let _ = application.native_shell_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match application.state() {
            EditorApplicationState::AuthoringReady | EditorApplicationState::AuthoringDegraded => {
                return application;
            }
            EditorApplicationState::Recovery => {
                panic!("Reference Game generic authoring open entered Recovery")
            }
            EditorApplicationState::OpeningInitial | EditorApplicationState::OpeningReplacement => {
                let _ = application.advance_project_open();
            }
            EditorApplicationState::ProjectManager => {
                panic!("the native shell must arm the initial project-open request")
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
    panic!("Reference Game generic authoring open did not reach a published workspace")
}

fn current_frame(
    application: &EditorApplication,
) -> std::sync::Arc<nara::tooling::EditorFrameModel> {
    application
        .current_frame()
        .expect("Reference Game must retain a published workspace")
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
            .expect("Reference Game must publish external Play state");
        if external.state() == expected_state
            && matches!(
                frame.play().result(),
                Some(EditorPlayOperationResult::Applied { operation, .. })
                    if operation == expected_operation
            )
        {
            return;
        }
        assert!(
            external.state() != EditorExternalPlayState::Faulted,
            "external Play faulted before {expected_state:?}/{expected_operation:?}: result={:?}, output={:#?}",
            frame.play().result(),
            external.output(),
        );
        assert!(
            Instant::now() < deadline,
            "external Play did not reach {expected_state:?}/{expected_operation:?}: state={:?}, result={:?}, output={:#?}",
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
        let frame = current_frame(application);
        let phase = frame.workspace_intent().phase();
        let result = frame.workspace_intent().result();
        let update = application
            .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
        if update.exit_ready() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Editor exit did not complete: state={:?}, workspace_intent_phase={phase:?}, workspace_intent_result={result:?}",
            application.state(),
        );
        thread::sleep(Duration::from_millis(5));
    }
}
