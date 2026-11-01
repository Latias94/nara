use nara::{
    project_host::{
        EditorAuthoredSchemaUpgradeError, EditorAuthoredSchemaUpgradeId,
        EditorAuthoredSchemaUpgradeOutcome, EditorAuthoredSchemaUpgradePhase,
    },
    tooling::{
        EditorCloseDecision, EditorWorkspaceIntentPhase, EditorWorkspaceIntentRequestResult,
    },
};

use super::*;

#[path = "authored_upgrade/confirmation.rs"]
mod confirmation;

const WAIT: Duration = Duration::from_secs(10);
const SOURCE: &str = "worlds/entry.scene.json";

fn awaiting_proposal(application: &mut EditorApplication) -> EditorAuthoredSchemaUpgradeId {
    let operation = application.begin_authored_schema_upgrade().unwrap();
    let deadline = Instant::now() + WAIT;
    loop {
        let view = application.authored_schema_upgrade_view().unwrap();
        if view.phase == EditorAuthoredSchemaUpgradePhase::AwaitingProposal {
            assert_eq!(view.operation, Some(operation));
            assert!(!view.work_pending);
            assert!(view.writes_fenced);
            return operation;
        }
        assert_ne!(
            view.phase,
            EditorAuthoredSchemaUpgradePhase::Terminal,
            "{view:?}"
        );
        assert!(
            Instant::now() < deadline,
            "capture did not return: {view:?}"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn assert_cancelled(application: &EditorApplication, operation: EditorAuthoredSchemaUpgradeId) {
    let view = application.authored_schema_upgrade_view().unwrap();
    assert_eq!(view.operation, Some(operation));
    assert_eq!(view.phase, EditorAuthoredSchemaUpgradePhase::Terminal);
    assert_eq!(
        view.outcome,
        Some(EditorAuthoredSchemaUpgradeOutcome::Cancelled)
    );
    assert!(!view.work_pending);
    assert!(!view.writes_fenced);
    assert!(matches!(
        application.authored_schema_upgrade_inputs(operation),
        Err(EditorAuthoredSchemaUpgradeError::Closing | EditorAuthoredSchemaUpgradeError::Operation)
    ));
}

fn request_retirement(application: &mut EditorApplication, retirement: EditorSessionRetirement) {
    let update = match retirement {
        EditorSessionRetirement::CloseProject => {
            application.submit_project_action(EditorProjectAction::CloseProject)
        }
        EditorSessionRetirement::ExitProcess => application.request_exit(),
    };
    assert!(!update.exit_ready());
}

fn resolve_close(application: &mut EditorApplication, decision: EditorCloseDecision) {
    let frame = application.current_frame().unwrap();
    let prompt = frame.workspace_intent().decision_prompt().unwrap();
    let submission = application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::WorkspaceIntent(prompt.resolve(decision))],
        ),
        Duration::ZERO,
    );
    assert!(matches!(
        submission.actions(),
        [crate::app::EditorApplicationActionDispatch::Dispatched(
            EditorActionOutcome::WorkspaceIntent(EditorWorkspaceIntentRequestResult::Accepted)
        )]
    ));
}

fn finish_retirement(application: &mut EditorApplication, retirement: EditorSessionRetirement) {
    let deadline = Instant::now() + WAIT;
    let mut exited = false;
    while application.current_frame().is_some()
        || (retirement == EditorSessionRetirement::ExitProcess && !exited)
    {
        assert!(
            Instant::now() < deadline,
            "Application retirement did not finish"
        );
        exited |= application.advance(Duration::ZERO).exit_ready();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    assert_eq!(exited, retirement == EditorSessionRetirement::ExitProcess);
}

#[test]
fn malformed_metadata_refuses_complete_capture_and_identifies_the_actual_relative_source() {
    let fixture = PersistenceFixture::trusted();
    let mut application = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    settle_background_imports(&mut application);
    let document = application
        .current_frame()
        .unwrap()
        .workspace()
        .active_document
        .unwrap();
    let basis = application.document_query_basis(document).unwrap();
    let original = fs::read(fixture.root.join(SOURCE)).unwrap();
    let metadata = fs::read(fixture.root.join("media/icon.png.meta")).unwrap();
    let operation = application.begin_authored_schema_upgrade().unwrap();
    let deadline = Instant::now() + WAIT;
    loop {
        let view = application.authored_schema_upgrade_view().unwrap();
        if view.phase == EditorAuthoredSchemaUpgradePhase::Terminal {
            assert_eq!(view.operation, Some(operation));
            assert_eq!(
                view.outcome,
                Some(EditorAuthoredSchemaUpgradeOutcome::Failed(
                    EditorAuthoredSchemaUpgradeError::Capture
                ))
            );
            assert!(!view.work_pending && !view.writes_fenced);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the rejected capture must return"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let report = application.authored_schema_upgrade_diagnostics().unwrap();
    let diagnostic = report
        .iter()
        .find(|diagnostic| {
            diagnostic.code().as_str() == "project.editor.authored-upgrade-metadata-invalid"
        })
        .unwrap();
    assert!(diagnostic.fields().iter().any(|field| {
        field.key().as_str() == "metadata"
            && matches!(
                field.value(),
                nara::diagnostic::DiagnosticValueRef::ProjectRelative("media/icon.png.meta")
            )
    }));
    assert_eq!(application.document_query_basis(document).unwrap(), basis);
    assert_eq!(fs::read(fixture.root.join(SOURCE)).unwrap(), original);
    assert_eq!(
        fs::read(fixture.root.join("media/icon.png.meta")).unwrap(),
        metadata
    );
    close(&mut application);
}

#[test]
fn awaiting_proposal_close_and_exit_cancel_capture_before_normal_dirty_save() {
    for retirement in [
        EditorSessionRetirement::CloseProject,
        EditorSessionRetirement::ExitProcess,
    ] {
        let fixture = PersistenceFixture::authored_upgrade();
        let original = fs::read(fixture.root.join(SOURCE)).unwrap();
        let mut application =
            fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
        settle_background_imports(&mut application);
        let document = edit_save_source(&mut application);
        let operation = awaiting_proposal(&mut application);

        request_retirement(&mut application, retirement);
        assert_cancelled(&application, operation);
        let frame = application.current_frame().unwrap();
        assert_eq!(
            frame.workspace_intent().phase(),
            Some(EditorWorkspaceIntentPhase::AwaitingDecision)
        );
        assert!(frame.workspace().scene(document).unwrap().dirty);
        assert!(frame.persistence().operation().is_none());
        assert_eq!(fs::read(fixture.root.join(SOURCE)).unwrap(), original);
        assert_eq!(
            application.begin_authored_schema_upgrade(),
            Err(EditorAuthoredSchemaUpgradeError::Closing)
        );

        resolve_close(&mut application, EditorCloseDecision::Save);
        finish_retirement(&mut application, retirement);
        assert!(
            fs::read_to_string(fixture.root.join(SOURCE))
                .unwrap()
                .contains("saved-before-retirement")
        );
        close(&mut application);
    }
}

#[test]
fn waiting_close_and_exit_recheck_a_cancellable_upgrade_before_dirty_arbitration() {
    for retirement in [
        EditorSessionRetirement::CloseProject,
        EditorSessionRetirement::ExitProcess,
    ] {
        let fixture = PersistenceFixture::authored_upgrade();
        let original = fs::read(fixture.root.join(SOURCE)).unwrap();
        let manifest = fs::read(fixture.root.join("nara.toml")).unwrap();
        let mut application =
            fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
        settle_background_imports(&mut application);
        let document = edit_save_source(&mut application);
        let basis = application.document_query_basis(document).unwrap();
        let operation = awaiting_proposal(&mut application);
        assert!(
            application
                .authored_schema_upgrade_view()
                .unwrap()
                .can_cancel()
        );

        // Seed only the Application's already-accepted retirement intent. The real Session
        // supplies cancellation authority at this waiting boundary. Root separately qualifies
        // the Pending pre-publisher refusal and physical Task return that reopen that authority.
        application.session_retirement = Some(retirement);
        application.session_close_retry_at = None;
        let update = application.finish_session_retirement(false);
        assert!(update.actions_observed());
        assert!(!update.exit_ready());
        assert_eq!(application.session_retirement, Some(retirement));
        assert_cancelled(&application, operation);

        application.advance(Duration::ZERO);
        let frame = application.current_frame().unwrap();
        assert_eq!(
            frame.workspace_intent().phase(),
            Some(EditorWorkspaceIntentPhase::AwaitingDecision)
        );
        assert!(frame.workspace().scene(document).unwrap().dirty);
        assert!(frame.persistence().operation().is_none());
        assert_eq!(
            application
                .state
                .workspace()
                .unwrap()
                .session
                .document_query_basis(document)
                .unwrap(),
            basis
        );
        assert_eq!(fs::read(fixture.root.join(SOURCE)).unwrap(), original);
        assert_eq!(fs::read(fixture.root.join("nara.toml")).unwrap(), manifest);

        resolve_close(&mut application, EditorCloseDecision::Discard);
        finish_retirement(&mut application, retirement);
        assert_eq!(fs::read(fixture.root.join(SOURCE)).unwrap(), original);
        assert_eq!(fs::read(fixture.root.join("nara.toml")).unwrap(), manifest);
        close(&mut application);
    }
}

#[test]
fn cancelling_dirty_close_after_capture_cancellation_preserves_the_working_document() {
    let fixture = PersistenceFixture::authored_upgrade();
    let original = fs::read(fixture.root.join(SOURCE)).unwrap();
    let mut application = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    settle_background_imports(&mut application);
    let document = edit_save_source(&mut application);
    let basis = application.document_query_basis(document).unwrap();
    let operation = awaiting_proposal(&mut application);

    request_retirement(&mut application, EditorSessionRetirement::CloseProject);
    assert_cancelled(&application, operation);
    resolve_close(&mut application, EditorCloseDecision::Cancel);
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    assert_eq!(application.document_query_basis(document).unwrap(), basis);
    assert!(
        application
            .current_frame()
            .unwrap()
            .workspace()
            .scene(document)
            .unwrap()
            .dirty
    );
    assert_eq!(fs::read(fixture.root.join(SOURCE)).unwrap(), original);

    // A cancelled Close restores ordinary authoring admission, including a fresh capture.
    let next = awaiting_proposal(&mut application);
    assert_ne!(next, operation);
    application.cancel_authored_schema_upgrade(next).unwrap();
    assert_cancelled(&application, next);
    request_retirement(&mut application, EditorSessionRetirement::CloseProject);
    resolve_close(&mut application, EditorCloseDecision::Discard);
    finish_retirement(&mut application, EditorSessionRetirement::CloseProject);
    assert_eq!(fs::read(fixture.root.join(SOURCE)).unwrap(), original);
    close(&mut application);
}

#[test]
fn awaiting_proposal_rejects_replacement_and_keeps_the_original_cancel_facade_accessible() {
    let fixture = PersistenceFixture::authored_upgrade();
    let mut application = fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
    settle_background_imports(&mut application);
    let operation = awaiting_proposal(&mut application);
    let frame = application.current_frame().unwrap();
    let document = frame.workspace().active_document.unwrap();
    let basis = application.document_query_basis(document).unwrap();

    assert!(!application.request_project_replacement(&fixture.root, Some("desktop".to_owned())));
    let rejected = application.submit_project_action(EditorProjectAction::OpenProject {
        project_root: fixture.root.clone(),
        profile: Some("desktop".to_owned()),
    });
    assert!(!rejected.actions_observed());
    assert!(!rejected.exit_ready());
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    assert!(std::sync::Arc::ptr_eq(
        &frame,
        &application.current_frame().unwrap()
    ));
    assert_eq!(application.document_query_basis(document).unwrap(), basis);
    assert!(!application.project_open_retirement_pending());
    let inputs = application
        .authored_schema_upgrade_inputs(operation)
        .unwrap();
    assert!(
        inputs
            .sources()
            .any(|source| source.document == Some(document))
    );

    application
        .cancel_authored_schema_upgrade(operation)
        .unwrap();
    assert_cancelled(&application, operation);
    assert!(application.request_project_replacement(&fixture.root, Some("desktop".to_owned())));
    let deadline = Instant::now() + WAIT;
    while application.state() == EditorApplicationState::OpeningReplacement {
        assert!(
            Instant::now() < deadline,
            "replacement did not resume after capture cancellation"
        );
        application.advance(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
    close(&mut application);
}

#[test]
fn undriven_session_capture_rejects_replacement_and_close_uses_normal_dirty_arbitration() {
    for retirement in [
        EditorSessionRetirement::CloseProject,
        EditorSessionRetirement::ExitProcess,
    ] {
        let fixture = PersistenceFixture::authored_upgrade();
        let original = fs::read(fixture.root.join(SOURCE)).unwrap();
        let mut application =
            fixture.open_with_expected_state(EditorApplicationState::AuthoringReady);
        settle_background_imports(&mut application);
        let document = edit_save_source(&mut application);
        // Retain a real prior Close result; the new retirement must acknowledge it.
        request_retirement(&mut application, EditorSessionRetirement::CloseProject);
        resolve_close(&mut application, EditorCloseDecision::Cancel);
        let frame = application.current_frame().unwrap();

        // Use the real Session admission without its first frame drive. No capture task has
        // started here: this verifies immediate fencing, not physical worker completion.
        let operation = application
            .state
            .workspace_mut()
            .unwrap()
            .session
            .begin_authored_schema_upgrade()
            .unwrap();
        let view = application.authored_schema_upgrade_view().unwrap();
        assert_eq!(view.phase, EditorAuthoredSchemaUpgradePhase::Draining);
        assert!(view.work_pending && view.writes_fenced);
        assert!(!frame.authored_schema_upgrade_work_pending());
        assert!(
            !application.request_project_replacement(&fixture.root, Some("desktop".to_owned()))
        );
        let rejected = application.submit_project_action(EditorProjectAction::OpenProject {
            project_root: fixture.root.clone(),
            profile: Some("desktop".to_owned()),
        });
        assert!(!rejected.actions_observed());
        assert!(std::sync::Arc::ptr_eq(
            &frame,
            &application.current_frame().unwrap()
        ));
        assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
        assert!(!application.project_open_retirement_pending());

        request_retirement(&mut application, retirement);
        assert_cancelled(&application, operation);
        let frame = application.current_frame().unwrap();
        assert_eq!(
            frame.workspace_intent().phase(),
            Some(EditorWorkspaceIntentPhase::AwaitingDecision)
        );
        assert!(frame.workspace().scene(document).unwrap().dirty);
        assert!(frame.persistence().operation().is_none());
        assert_eq!(fs::read(fixture.root.join(SOURCE)).unwrap(), original);
        resolve_close(&mut application, EditorCloseDecision::Discard);
        finish_retirement(&mut application, retirement);
        assert_eq!(fs::read(fixture.root.join(SOURCE)).unwrap(), original);
        close(&mut application);
    }
}
