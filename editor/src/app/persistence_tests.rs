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
fn pending_save_replacement_retries_close_and_refreshes_the_prepared_source() {
    let fixture = PersistenceFixture::new();
    let mut application = fixture.open();
    queue_save(&mut application);
    assert!(application.request_project_replacement(&fixture.root, Some("desktop".to_owned())));
    for _ in 0..8 {
        application.advance_project_open();
        if matches!(
            application.state,
            super::EditorProductState::OpeningReplacement(OpeningReplacementState::Closing { .. })
        ) {
            break;
        }
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
        let before = application.current_frame().unwrap().persistence();
        let update = application.drive_editor_frame(Duration::ZERO, false, false);
        let after = application.current_frame().unwrap().persistence();
        assert_eq!(update.actions_observed(), before != after);
        if after.operation() == EditorPersistenceOperation::Idle {
            assert!(matches!(
                after.result(),
                Some(EditorPersistenceResult::Saved { .. })
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
        for _ in 0..8 {
            if application.current_frame().is_some() {
                break;
            }
            application.advance_project_open();
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
