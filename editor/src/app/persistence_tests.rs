use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use nara::{
    scene::{
        SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
        ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
        EditorPersistenceAction, EditorPersistenceOperation, EditorPersistenceRequestResult,
        EditorPersistenceResult,
    },
};

use super::{
    EditorApplication, EditorApplicationState, EditorLaunchOptions, EditorProjectAction,
    EditorSessionRetirement, OpeningReplacementState,
};

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
        let deadline = Instant::now() + Duration::from_secs(10);
        while !exited && Instant::now() < deadline {
            assert!(!application.advance_project_open().actions_observed());
            exited = application
                .drive_editor_frame(Duration::ZERO, false, false)
                .exit_ready();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(exited);
        assert_eq!(application.state(), EditorApplicationState::ProjectManager);
        assert!(application.current_frame().is_none());
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
        while application.current_frame().is_some() && Instant::now() < deadline {
            exited |= application
                .drive_editor_frame(Duration::ZERO, false, false)
                .exit_ready();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(application.state(), EditorApplicationState::ProjectManager);
        assert_eq!(exited, retirement == EditorSessionRetirement::ExitProcess);
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
        while application.current_frame().is_some() && Instant::now() < deadline {
            exited |= application
                .drive_editor_frame(Duration::ZERO, false, false)
                .exit_ready();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(application.state(), EditorApplicationState::ProjectManager);
        assert_eq!(exited, retirement == EditorSessionRetirement::ExitProcess);
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
        if after.operation() == EditorPersistenceOperation::Idle {
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
            || before.source_asset_import_work_pending() != after.source_asset_import_work_pending()
            || before.image_import_work_pending() != after.image_import_work_pending();
        assert_eq!(update.actions_observed(), changed);
        if after.persistence().operation() == EditorPersistenceOperation::Idle {
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
            EditorPersistenceRequestResult::Accepted
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
            EditorPersistenceRequestResult::Accepted
        )]
    ));
    let project = application.state.workspace().unwrap();
    assert_eq!(
        project.session.persistence_view().operation(),
        EditorPersistenceOperation::Opening {
            document: scene.document
        }
    );
    assert_eq!(
        frame.persistence().operation(),
        EditorPersistenceOperation::Idle,
        "admission must precede publication of the Opening frame"
    );
}

fn close(application: &mut EditorApplication) {
    let mut exited = application.request_exit().exit_ready();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !exited && Instant::now() < deadline {
        exited = application
            .drive_editor_frame(Duration::ZERO, false, false)
            .exit_ready();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(exited, "the persistence fixture retained its session");
}

struct PersistenceFixture {
    root: PathBuf,
}

impl PersistenceFixture {
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
