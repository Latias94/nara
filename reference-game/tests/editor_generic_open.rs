use std::path::Path;

use nara::tooling::{
    EditorAuthoringReadiness, EditorProjectContentView, EditorRuntimeAvailability,
    EditorRuntimeUnavailableReason,
};
use nara_editor::{EditorApplication, EditorApplicationState, EditorLaunchOptions};

#[test]
fn installed_generic_editor_opens_reference_game_without_runtime_publication() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(root).with_profile("desktop"));

    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    let _ = application.native_shell_ready();
    for _ in 0..8 {
        match application.state() {
            EditorApplicationState::AuthoringReady
            | EditorApplicationState::AuthoringDegraded
            | EditorApplicationState::Recovery => break,
            EditorApplicationState::OpeningInitial
            | EditorApplicationState::OpeningReplacement => {
                let _ = application.advance_project_open();
            }
            EditorApplicationState::ProjectManager => {
                panic!("the native shell must arm the initial project-open request")
            }
        }
    }

    assert_eq!(
        application.state(),
        EditorApplicationState::AuthoringDegraded
    );
    let frame = application
        .current_frame()
        .expect("ordinary Generic Open must publish a Reference Game workspace");
    assert_eq!(
        frame.authoring_readiness(),
        EditorAuthoringReadiness::Degraded
    );
    assert_eq!(
        frame.runtime_availability(),
        EditorRuntimeAvailability::Unavailable(
            EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
        )
    );
    assert_eq!(
        frame.project_content(),
        EditorProjectContentView::NotInstalled
    );

}
