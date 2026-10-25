use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe,
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginShutdownContext,
    },
    project_host::{EditorProjectCloseReceipt, TrustedCompiledEditorProjectIntent},
    scene::{
        SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
        ScenePlacement, ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
        EditorPersistenceAction, EditorPersistenceOperation, EditorPersistenceRequestResult,
        EditorPersistenceResult, EditorPlayAction, EditorPlayRequestResult, EditorPlayState,
    },
};

use super::{
    EditorApplication, EditorApplicationState, EditorLaunchOptions, EditorProjectAction,
    EditorSessionRetirement, OpeningReplacementState,
};

fn advance_to_session_publication(application: &mut EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let opening = match &application.state {
            super::EditorProductState::OpeningInitial(opening) => opening,
            super::EditorProductState::OpeningReplacement(OpeningReplacementState::Preparing {
                opening,
                ..
            }) => opening,
            other => panic!("Open did not reach Session publication: {other:?}"),
        };
        if matches!(&opening.step, super::OpeningProjectStep::Host(attempt)
            if attempt.phase() == nara::project_host::EditorProjectOpenPhase::SessionPublication)
        {
            return;
        }
        assert!(Instant::now() < deadline, "Open did not prepare its source");
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn failed_initial_open_retains_cleanup_across_retry_manager_and_exit() {
    for return_to_manager in [false, true] {
        let fixture = PersistenceFixture::new();
        let source = fixture.root.join("worlds/entry.scene.json");
        let original = fs::read(&source).unwrap();
        let mut application =
            EditorApplication::new(EditorLaunchOptions::new(&fixture.root).with_profile("desktop"));
        application.native_shell_ready();
        advance_to_session_publication(&mut application);
        let mut changed = original.clone();
        changed.extend_from_slice(b"\n");
        fs::write(&source, changed).unwrap();
        application.advance_project_open();
        let rejected_state = application.state();
        let initially_pending = application.project_open_retirement_pending();
        let early_retry = application.retry_project_open();
        fs::write(&source, &original).unwrap();
        let mut exit_ready = false;
        if return_to_manager {
            application
                .submit_shell_action(super::EditorProjectShellAction::ReturnToProjectManager);
            let retained_in_manager = application.project_open_retirement_pending();
            exit_ready = application.request_exit().exit_ready();
            assert!(retained_in_manager);
            assert!(!exit_ready);
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while application.project_open_retirement_pending() || return_to_manager && !exit_ready {
            assert!(
                Instant::now() < deadline,
                "failed Open cleanup did not return"
            );
            exit_ready |= application.advance_project_open().exit_ready();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(rejected_state, EditorApplicationState::Recovery);
        assert!(initially_pending);
        assert!(!early_retry);
        assert!(application.current_frame().is_none());
        if return_to_manager {
            assert_eq!(application.state(), EditorApplicationState::ProjectManager);
            assert!(exit_ready);
        } else {
            assert!(application.retry_project_open());
            let deadline = Instant::now() + Duration::from_secs(10);
            while application.current_frame().is_none() {
                assert!(
                    Instant::now() < deadline,
                    "retry after physical return did not open"
                );
                application.advance_project_open();
                std::thread::sleep(Duration::from_millis(1));
            }
            queue_save(&mut application);
            close(&mut application);
        }
    }
}

#[test]
fn failed_replacement_open_preserves_the_incumbent_while_cleanup_gates_another_candidate() {
    let fixture = PersistenceFixture::new();
    let mut application = fixture.open();
    let incumbent = application.current_frame().unwrap();
    let source = fixture.root.join("worlds/entry.scene.json");
    let original = fs::read(&source).unwrap();
    assert!(application.request_project_replacement(&fixture.root, Some("desktop".to_owned())));
    advance_to_session_publication(&mut application);
    let mut changed = original.clone();
    changed.extend_from_slice(b"\n");
    fs::write(&source, changed).unwrap();
    application.advance_project_open();
    let same_incumbent = std::sync::Arc::ptr_eq(&incumbent, &application.current_frame().unwrap());
    let pending = application.project_open_retirement_pending();
    let early_replacement =
        application.request_project_replacement(&fixture.root, Some("desktop".to_owned()));
    fs::write(&source, original).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.project_open_retirement_pending() {
        assert!(
            Instant::now() < deadline,
            "failed replacement cleanup did not return"
        );
        application.drive_editor_frame(Duration::ZERO, true, false);
        std::thread::sleep(Duration::from_millis(1));
    }
    queue_save(&mut application);
    close(&mut application);
    assert!(same_incumbent);
    assert!(pending);
    assert!(!early_replacement);
}

#[test]
fn exit_upgrades_close_project_while_failed_replacement_retirement_keeps_the_session() {
    let fixture = PersistenceFixture::new();
    let mut application = fixture.open();
    let source = fixture.root.join("worlds/entry.scene.json");
    let original = fs::read(&source).unwrap();
    assert!(application.request_project_replacement(&fixture.root, Some("desktop".to_owned())));
    advance_to_session_publication(&mut application);
    let mut changed = original.clone();
    changed.extend_from_slice(b"\n");
    fs::write(&source, changed).unwrap();
    application.advance_project_open();
    fs::write(&source, original).unwrap();
    let initially_pending = application.project_open_retirement_pending();
    application.request_close_project();

    // Drive the incumbent independently. The actual rejected Open remains unpolled so its
    // separate custody deterministically keeps the physically closed Session in place.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let project = application.state.workspace_mut().unwrap();
        super::drive_project_frame(project, Duration::ZERO, false);
        let complete = matches!(
            project.session.advance_close(),
            nara::project_host::EditorProjectCloseProgress::Complete(_)
        );
        application.session_close_retry_at = None;
        application.finish_session_retirement(false);
        if complete {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the incumbent did not physically close"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let closed_incumbent_retained = application.state.workspace().is_some();
    let close_intent = application.session_retirement;
    let immediate_exit = application.request_exit().exit_ready();
    let exit_remembered = application.has_pending_exit();
    let promoted_intent = application.session_retirement;
    let mut eventual_exit = immediate_exit;
    while application.project_open_retirement_pending() || application.state.workspace().is_some() {
        assert!(
            Instant::now() < deadline,
            "the retained owners did not return"
        );
        application.session_close_retry_at = None;
        eventual_exit |= application
            .drive_editor_frame(Duration::ZERO, false, false)
            .exit_ready();
        std::thread::sleep(Duration::from_millis(1));
    }

    // Assert only after releasing both owners, including when the original Exit-loss bug fires.
    assert!(initially_pending);
    assert!(closed_incumbent_retained);
    assert_eq!(close_intent, Some(EditorSessionRetirement::CloseProject));
    assert!(!immediate_exit);
    assert!(
        exit_remembered,
        "a single process Exit must survive an existing Close Project"
    );
    assert_eq!(promoted_intent, Some(EditorSessionRetirement::ExitProcess));
    assert!(
        eventual_exit,
        "maintenance must finish the original Exit request"
    );
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
}

#[test]
fn terminal_open_cleanup_evidence_survives_clean_failure_slot_reuse_and_manager_navigation() {
    use nara::app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginShutdownContext, RuntimeCandidateRetirementState, RuntimeInstance,
    };

    struct RejectedShutdown;
    impl Plugin for RejectedShutdown {
        fn declaration() -> &'static PluginDeclaration {
            static DECLARATION: PluginDeclaration = PluginDeclaration::new(
                PluginId::new("nara.test.open-sticky-cleanup"),
                PluginCategory::Runtime,
            );
            &DECLARATION
        }
        fn build(&self, _: &mut App) -> Result<(), PluginError> {
            Ok(())
        }
        fn shutdown(&self, _: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
            Err(PluginError::SetupFailed {
                plugin: Self::declaration().id,
                message: "sticky cleanup probe".to_owned(),
            })
        }
    }

    let mut app = App::new();
    app.add_plugin(RejectedShutdown).unwrap();
    let mut retirement = RuntimeInstance::start(app).unwrap().begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    let first = retirement.close_evidence().clone();
    assert!(first.has_terminal_failure());

    let fixture = PersistenceFixture::new();
    let source = fixture.root.join("worlds/entry.scene.json");
    let original = fs::read(&source).unwrap();
    let mut application =
        EditorApplication::new(EditorLaunchOptions::new(&fixture.root).with_profile("desktop"));
    // Seed real terminal evidence at the private consumer boundary. The fixed authoring
    // composition has no public plugin-failure injection API; subsequent Open failures are real.
    application.failed_open_close_evidence = Some(first.clone());
    application.native_shell_ready();
    for return_to_manager in [false, true] {
        advance_to_session_publication(&mut application);
        let mut changed = original.clone();
        changed.extend_from_slice(b"\n");
        fs::write(&source, changed).unwrap();
        application.advance_project_open();
        fs::write(&source, &original).unwrap();
        assert!(application.project_open_retirement_pending());
        if return_to_manager {
            application
                .submit_shell_action(super::EditorProjectShellAction::ReturnToProjectManager);
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while application.project_open_retirement_pending() {
            assert!(
                Instant::now() < deadline,
                "clean failed Open did not return"
            );
            application.advance_project_open();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(application.failed_open_close_evidence(), Some(&first));
        if !return_to_manager {
            assert!(application.retry_project_open());
        }
    }
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    close(&mut application);
    assert!(application.open_host_tasks_close_report().is_complete());
    assert_eq!(application.failed_open_close_evidence(), Some(&first));
}

#[derive(Default)]
struct SessionShutdownFailure;

impl Plugin for SessionShutdownFailure {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.session-sticky-cleanup"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn shutdown(&self, _: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: Self::declaration().id,
            message: "Session cleanup probe".to_owned(),
        })
    }
}

fn start_trusted_play(application: &mut EditorApplication) {
    let frame = application.current_frame().unwrap();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    let dispatch = application.dispatch_editor_actions(EditorActionBatch::new(
        frame.revision(),
        [EditorAction::Play(EditorPlayAction::Play {
            document: scene.document,
            base_revision: scene.revision,
        })],
    ));
    assert!(matches!(
        dispatch.feedback().unwrap().unwrap().outcomes(),
        [EditorActionOutcome::Play(EditorPlayRequestResult::Accepted)]
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().unwrap().play().state() != EditorPlayState::Running {
        assert!(Instant::now() < deadline, "trusted Play did not start");
        application.drive_editor_frame(Duration::ZERO, false, true);
        std::thread::sleep(Duration::from_millis(1));
    }
}

// Shared with the native result tests: this receipt comes from public trusted construction,
// public Play dispatch and actual whole-Session close, without a seeded receipt or close hook.
pub(crate) fn failed_trusted_session_close_receipt() -> EditorProjectCloseReceipt {
    let fixture = PersistenceFixture::trusted();
    let mut application = fixture.open_with_shutdown_failure();
    assert!(application.failed_session_close_receipt().is_none());
    start_trusted_play(&mut application);
    close(&mut application);
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    let receipt = application.failed_session_close_receipt().unwrap().clone();
    assert!(receipt.has_cleanup_failure());
    assert!(receipt.has_play_cleanup_failure());
    assert!(!receipt.has_authoring_cleanup_failure());
    assert!(receipt.task_shutdown().is_complete());
    assert!(!receipt.task_shutdown().has_cleanup_failure());
    receipt
}

#[test]
fn session_cleanup_failure_survives_manager_reopen_and_clean_exit() {
    let fixture = PersistenceFixture::trusted();
    let mut application = fixture.open_with_shutdown_failure();
    start_trusted_play(&mut application);
    application.request_close_project();
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_some() {
        assert!(
            Instant::now() < deadline,
            "Session did not return to manager"
        );
        assert!(
            !application
                .drive_editor_frame(Duration::ZERO, false, true)
                .exit_ready()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(application.state(), EditorApplicationState::ProjectManager);
    let first = application.failed_session_close_receipt().unwrap().clone();
    assert!(first.has_play_cleanup_failure());
    application.submit_shell_action(super::EditorProjectShellAction::OpenProject {
        project_root: fixture.root.clone(),
        profile: Some("desktop".to_owned()),
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.current_frame().is_none() {
        assert!(Instant::now() < deadline, "manager Open did not publish");
        application.advance_project_open();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(application.failed_session_close_receipt(), Some(&first));
    close(&mut application);
    assert_eq!(application.failed_session_close_receipt(), Some(&first));
    assert!(application.request_exit().exit_ready());
}

#[test]
fn replacement_handoff_retains_session_cleanup_failure_through_a_clean_successor() {
    let fixture = PersistenceFixture::trusted();
    let mut application = fixture.open_with_shutdown_failure();
    start_trusted_play(&mut application);
    let mut first = None;
    for _ in 0..2 {
        let incumbent = application.current_frame().unwrap();
        assert!(application.request_project_replacement(&fixture.root, Some("desktop".to_owned())));
        let deadline = Instant::now() + Duration::from_secs(10);
        while application.state() == EditorApplicationState::OpeningReplacement {
            assert!(
                Instant::now() < deadline,
                "replacement handoff did not finish"
            );
            assert!(!application.advance_project_open().exit_ready());
            assert!(
                !application
                    .drive_editor_frame(Duration::ZERO, false, true)
                    .exit_ready()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(application.state(), EditorApplicationState::AuthoringReady);
        assert!(!std::sync::Arc::ptr_eq(
            &incumbent,
            &application.current_frame().unwrap()
        ));
        let receipt = application.failed_session_close_receipt().unwrap();
        assert!(receipt.has_play_cleanup_failure());
        assert!(receipt.task_shutdown().is_complete());
        if let Some(first) = &first {
            assert_eq!(receipt, first);
        } else {
            first = Some(receipt.clone());
        }
    }
    close(&mut application);
    assert_eq!(application.failed_session_close_receipt(), first.as_ref());
}

#[test]
fn startup_preparation_exit_revokes_initial_and_replacement_publication() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../reference-game");
    let source = root.join("scenes/startup.scene.json");
    let original = fs::read(&source).unwrap();
    for replacement in [false, true] {
        let mut application =
            EditorApplication::new(EditorLaunchOptions::new(&root).with_profile("desktop"));
        application.native_shell_ready();
        if replacement {
            let deadline = Instant::now() + Duration::from_secs(10);
            while application.current_frame().is_none() && Instant::now() < deadline {
                application.advance_project_open();
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(application.current_frame().is_some());
            assert!(application.request_project_replacement(&root, Some("desktop".to_owned())));
        }
        let retained = application.current_frame();
        let retained_entry = application.workspace_generation();
        let assert_no_candidate_publication = |application: &EditorApplication| {
            if let Some(generation) = application.workspace_generation() {
                assert_eq!(
                    Some(generation),
                    retained_entry,
                    "Exit published the startup candidate instead of retaining its incumbent"
                );
            }
            if !replacement {
                assert!(application.current_frame().is_none());
            }
        };
        super::tests::advance_to_startup_preparation(&mut application);
        assert!(!application.advance_project_open().actions_observed());
        if let Some(before) = retained {
            assert!(std::sync::Arc::ptr_eq(
                &before,
                &application.current_frame().unwrap()
            ));
        } else {
            assert!(application.current_frame().is_none());
        }

        // Exit revokes publication; it does not claim that the OS read has already returned.
        let mut exited = application.request_exit().exit_ready();
        assert_no_candidate_publication(&application);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !exited && Instant::now() < deadline {
            exited |= application.advance_project_open().exit_ready();
            assert_no_candidate_publication(&application);
            exited |= application
                .drive_editor_frame(Duration::ZERO, false, false)
                .exit_ready();
            assert_no_candidate_publication(&application);
            assert!(!matches!(
                application.state(),
                EditorApplicationState::OpeningInitial | EditorApplicationState::OpeningReplacement
            ));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(exited);
        assert_eq!(application.state(), EditorApplicationState::ProjectManager);
        assert!(application.current_frame().is_none());
        assert!(application.open_host_tasks_close_report().is_complete());
        assert!(!application.advance_project_open().actions_observed());
        assert!(application.current_frame().is_none());
        assert_eq!(fs::read(&source).unwrap(), original);
    }
}

#[test]
fn pending_save_retries_close_project_and_exit_after_its_receipt() {
    for retirement in [
        EditorSessionRetirement::CloseProject,
        EditorSessionRetirement::ExitProcess,
    ] {
        let fixture = PersistenceFixture::new();
        let mut application = fixture.open();
        queue_save(&mut application);

        let mut exited = match retirement {
            EditorSessionRetirement::CloseProject => application
                .submit_project_action(EditorProjectAction::CloseProject)
                .exit_ready(),
            EditorSessionRetirement::ExitProcess => application.request_exit().exit_ready(),
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while (application.current_frame().is_some()
            || (retirement == EditorSessionRetirement::ExitProcess && !exited))
            && Instant::now() < deadline
        {
            exited |= application
                .drive_editor_frame(Duration::ZERO, false, false)
                .exit_ready();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(application.state(), EditorApplicationState::ProjectManager);
        assert_eq!(exited, retirement == EditorSessionRetirement::ExitProcess);
        assert_eq!(
            application.open_host_tasks_close_report().is_complete(),
            exited
        );
        assert!(
            fs::read_to_string(fixture.root.join("worlds/entry.scene.json"))
                .unwrap()
                .contains("saved-before-retirement")
        );
    }
}

#[test]
fn pending_reopen_retries_close_project_and_exit_after_owner_publication() {
    for retirement in [
        EditorSessionRetirement::CloseProject,
        EditorSessionRetirement::ExitProcess,
    ] {
        let fixture = PersistenceFixture::new();
        let original = fs::read(fixture.root.join("worlds/entry.scene.json")).unwrap();
        let mut application = fixture.open();
        queue_reopen(&mut application);
        let mut exited = match retirement {
            EditorSessionRetirement::CloseProject => application
                .submit_project_action(EditorProjectAction::CloseProject)
                .exit_ready(),
            EditorSessionRetirement::ExitProcess => application.request_exit().exit_ready(),
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while (application.current_frame().is_some()
            || (retirement == EditorSessionRetirement::ExitProcess && !exited))
            && Instant::now() < deadline
        {
            exited |= application
                .drive_editor_frame(Duration::ZERO, false, false)
                .exit_ready();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(application.state(), EditorApplicationState::ProjectManager);
        assert_eq!(exited, retirement == EditorSessionRetirement::ExitProcess);
        assert_eq!(
            application.open_host_tasks_close_report().is_complete(),
            exited
        );
        assert_eq!(
            fs::read(fixture.root.join("worlds/entry.scene.json")).unwrap(),
            original
        );
    }
}

#[test]
fn pending_reopen_replacement_waits_for_the_original_source_publication() {
    let fixture = PersistenceFixture::new();
    let mut application = fixture.open();
    queue_reopen(&mut application);
    assert!(application.request_project_replacement(&fixture.root, Some("desktop".to_owned())));
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.state() == EditorApplicationState::OpeningReplacement
        && Instant::now() < deadline
    {
        application.advance_project_open();
        application.drive_editor_frame(Duration::ZERO, false, false);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        application.state(),
        EditorApplicationState::AuthoringDegraded
    );
    let frame = application.current_frame().unwrap();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    assert!(!scene.dirty);
    assert!(
        !scene
            .editor
            .inspector
            .entities
            .iter()
            .any(|entity| entity.id == SceneEntityId::new("discarded-by-reopen").unwrap())
    );
    close(&mut application);
}

#[test]
fn reopen_completion_is_observed_without_repainting_unchanged_polls() {
    let fixture = PersistenceFixture::new();
    let mut application = fixture.open();
    queue_reopen(&mut application);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saw_completion = false;
    while Instant::now() < deadline {
        let before = application.current_frame().unwrap().persistence();
        let update = application.drive_editor_frame(Duration::ZERO, false, false);
        let after = application.current_frame().unwrap().persistence();
        assert_eq!(update.actions_observed(), before != after);
        if after.operation().is_none() {
            assert!(matches!(
                after.result(),
                Some(EditorPersistenceResult::Opened { .. })
            ));
            saw_completion = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(saw_completion);
    assert!(
        !application
            .drive_editor_frame(Duration::ZERO, false, false)
            .actions_observed()
    );
    close(&mut application);
}

#[test]
fn pending_save_replacement_retries_close_and_refreshes_the_prepared_source() {
    let fixture = PersistenceFixture::new();
    let mut application = fixture.open();
    queue_save(&mut application);
    assert!(application.request_project_replacement(&fixture.root, Some("desktop".to_owned())));
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        application.advance_project_open();
        if matches!(
            application.state,
            super::EditorProductState::OpeningReplacement(OpeningReplacementState::Closing { .. })
        ) {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while application.state() == EditorApplicationState::OpeningReplacement
        && Instant::now() < deadline
    {
        application.advance_project_open();
        application.drive_editor_frame(Duration::ZERO, false, false);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        application.state(),
        EditorApplicationState::AuthoringDegraded
    );
    let frame = application.current_frame().unwrap();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    assert!(
        scene
            .editor
            .inspector
            .entities
            .iter()
            .any(|entity| entity.id == SceneEntityId::new("saved-before-retirement").unwrap())
    );
    close(&mut application);
}

#[test]
fn persistence_completion_is_observed_without_repainting_unchanged_polls() {
    let fixture = PersistenceFixture::new();
    let mut application = fixture.open();
    queue_save(&mut application);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut saw_completion = false;
    while Instant::now() < deadline {
        let before = application.current_frame().unwrap();
        let update = application.drive_editor_frame(Duration::ZERO, false, false);
        let after = application.current_frame().unwrap();
        // Save refresh can complete image preparation independently of its persistence receipt.
        let changed = before.persistence() != after.persistence()
            || before.source_asset_import() != after.source_asset_import()
            || before.source_asset_import_work_pending()
                != after.source_asset_import_work_pending()
            || before.image_import_work_pending() != after.image_import_work_pending();
        assert_eq!(update.actions_observed(), changed);
        if after.persistence().operation().is_none() {
            assert!(matches!(
                after.persistence().result(),
                Some(EditorPersistenceResult::Saved { .. })
            ));
            saw_completion = true;
            if !after.image_import_work_pending() && !after.source_asset_import_work_pending() {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(saw_completion);
    let before = application.current_frame().unwrap();
    let observed = application
        .drive_editor_frame(Duration::ZERO, false, false)
        .actions_observed();
    let after = application.current_frame().unwrap();
    close(&mut application);
    assert_eq!(before.persistence(), after.persistence());
    assert!(!observed);
}

fn queue_save(application: &mut EditorApplication) {
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
                            SceneEntityId::new("saved-before-retirement").unwrap(),
                        ),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                },
            )],
        ),
        Duration::ZERO,
    );
    let frame = application.current_frame().unwrap();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    assert!(scene.dirty);
    // Do not drive here: the next lifecycle request must observe the admitted Save owner.
    let dispatch = application.dispatch_editor_actions(EditorActionBatch::new(
        frame.revision(),
        [EditorAction::Persistence(EditorPersistenceAction::Save {
            document: scene.document,
            base_revision: scene.revision,
        })],
    ));
    assert!(matches!(
        dispatch.feedback().unwrap().unwrap().outcomes(),
        [EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Accepted { .. }
        )]
    ));
}

fn queue_reopen(application: &mut EditorApplication) {
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
                            SceneEntityId::new("discarded-by-reopen").unwrap(),
                        ),
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    }]),
                },
            )],
        ),
        Duration::ZERO,
    );
    let frame = application.current_frame().unwrap();
    let scene = frame.workspace().active_scene.as_ref().unwrap();
    assert!(scene.dirty);
    let dispatch = application.dispatch_editor_actions(EditorActionBatch::new(
        frame.revision(),
        [EditorAction::Persistence(EditorPersistenceAction::Reopen {
            target: frame.reopen_target(),
        })],
    ));
    assert!(matches!(
        dispatch.feedback().unwrap().unwrap().outcomes(),
        [EditorActionOutcome::Persistence(
            EditorPersistenceRequestResult::Accepted { .. }
        )]
    ));
    let project = application.state.workspace().unwrap();
    assert_eq!(
        project.session.persistence_view().operation(),
        Some(EditorPersistenceOperation::Opening {
            document: scene.document
        })
    );
    assert_eq!(
        frame.persistence().operation(),
        None,
        "admission must precede publication of the Opening frame"
    );
}

fn close(application: &mut EditorApplication) {
    let mut exited = application.request_exit().exit_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !exited && Instant::now() < deadline {
        // Workspace Exit must drive the trusted Play lifecycle before physical Session close.
        exited = application
            .drive_editor_frame(Duration::ZERO, false, true)
            .exit_ready();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(exited, "the persistence fixture retained its session");
}

struct PersistenceFixture {
    root: PathBuf,
}

impl PersistenceFixture {
    fn trusted() -> Self {
        let fixture = Self::new();
        let bundle = nara::reflect::AuthoringSchemaBundle::from_trusted_provider_definitions(
            nara::project_host::advanced::built_in_schema_providers(),
        )
        .unwrap();
        fs::write(
            fixture.root.join("catalogs/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            fixture.root.join("worlds/entry.scene.json"),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        fixture
    }

    fn open_with_shutdown_failure(&self) -> EditorApplication {
        EditorApplication::open_trusted_compiled(
            EditorLaunchOptions::new(&self.root).with_profile("desktop"),
            TrustedCompiledEditorProjectIntent::new()
                .with_profile("desktop")
                .with_recipe(
                    ProductRecipe::new()
                        .add_plugin::<SessionShutdownFailure>()
                        .unwrap(),
                ),
        )
        .unwrap()
    }

    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara-editor-persistence-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generic-editor/known-unbound");
        for relative in [
            "nara.toml",
            "Cargo.toml",
            "src/lib.rs",
            "scripts/tool.rs",
            "worlds/entry.scene.json",
            "worlds/external.scene.json",
            "worlds/notes.txt",
            "blueprints/external.prefab.json",
            "catalogs/authoring.bundle.json",
            "media/icon.png",
            "media/icon.png.meta",
        ] {
            let target = root.join(relative);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(source.join(relative), target).unwrap();
        }
        Self { root }
    }

    fn open(&self) -> EditorApplication {
        let mut application =
            EditorApplication::new(EditorLaunchOptions::new(&self.root).with_profile("desktop"));
        application.native_shell_ready();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if application.current_frame().is_some() {
                break;
            }
            application.advance_project_open();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            application.state(),
            EditorApplicationState::AuthoringDegraded
        );
        application
    }
}

impl Drop for PersistenceFixture {
    fn drop(&mut self) {
        let temporary_root = std::env::temp_dir().canonicalize().unwrap();
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(temporary_root));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara-editor-persistence-")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
