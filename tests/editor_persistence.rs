#![cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara::{
    fs::{
        CapabilityRights, DirectoryCapability, ExpectedTarget, HostCapabilityOptions,
        PublicationAtomicity, RelativeComponent, TrustMode,
    },
    project_host::{EditorProjectIntent, EditorProjectSession},
    scene::{
        SceneAuthoringSession, SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
        ScenePatchOperation,
    },
    tooling::{
        EditorCloseDecision, EditorPersistenceCommand, EditorPersistenceCommit,
        EditorPersistenceOperation, EditorPersistenceRejection, EditorPersistenceRequestResult,
        EditorPersistenceResult, EditorPlayCommand, EditorPlayRequestResult, EditorPlayState,
        EditorWorkspace, EditorWorkspaceCommand, EditorWorkspaceIntent, EditorWorkspaceIntentPhase,
        EditorWorkspaceIntentRequestResult, EditorWorkspaceIntentResult,
    },
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const ASYNC_DRIVER_TIMEOUT: Duration = Duration::from_secs(5);
const ASYNC_POLL_INTERVAL: Duration = Duration::from_millis(1);

#[test]
fn persistence_authority_surface_is_opaque_and_linear() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/editor_persistence/authority_not_clone.rs");
    tests.compile_fail("tests/ui/editor_persistence/checkpoint_not_clone.rs");
    tests.compile_fail("tests/ui/editor_persistence/checkpoint_not_constructible.rs");
    tests.compile_fail("tests/ui/editor_persistence/replace_receipt_not_clone.rs");
    tests.compile_fail("tests/ui/editor_persistence/receiptless_commit.rs");
    tests.compile_fail("tests/ui/editor_persistence/workspace_saved_state_is_private.rs");
}

#[test]
fn persistence_commit_rejects_content_changed_outside_the_temporary_capability() {
    let project = TestProject::new("persistence-content-evidence");
    let target_path = project.root.join("publication.scene");
    let temporary_path = project.root.join("publication.tmp");
    fs::write(&target_path, b"old").unwrap();
    let capability = project.capability();
    let target = RelativeComponent::new("publication.scene").unwrap();
    let temporary_name = RelativeComponent::new("publication.tmp").unwrap();
    let previous = capability.open_child_file(&target).unwrap();
    let mut temporary = capability.create_temp(&temporary_name).unwrap();
    temporary.write_all(b"intended").unwrap();
    temporary.sync().unwrap();
    fs::write(&temporary_path, b"tampered").unwrap();
    let receipt = capability
        .replace_temp(
            temporary,
            &target,
            ExpectedTarget::Identity(previous.identity()),
        )
        .unwrap();

    let (mut workspace, mut authority) = EditorWorkspace::new_hosted();
    let document = workspace
        .open_scene_session(
            "publication.scene",
            SceneAuthoringSession::new(SceneDocument::default()),
        )
        .unwrap()
        .opened_document
        .unwrap();
    let checkpoint = authority.capture(&workspace, document).unwrap();

    assert!(
        EditorPersistenceCommit::from_publication(checkpoint, previous.identity(), receipt)
            .is_none()
    );
    assert_eq!(fs::read(target_path).unwrap(), b"tampered");
}

#[test]
fn save_advances_only_the_captured_revision_then_close_and_reopen_use_persisted_bytes() {
    let project = TestProject::new("save-reopen");
    let mut editor = EditorProjectSession::open(project.capability(), EditorProjectIntent::new())
        .expect("the editor project should open");
    let document = editor.workspace().active_document().unwrap();

    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch("captured"),
            })
            .applied
    );
    let captured_revision = editor.workspace().scene(document).unwrap().revision();
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor.persistence_view().operation(),
        EditorPersistenceOperation::Saving {
            document,
            captured_revision,
        }
    );
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::Busy)
    );

    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch("later"),
            })
            .applied
    );
    let first = editor
        .drive_editor_frame(std::time::Duration::ZERO)
        .persistence();
    assert!(matches!(
        first.result(),
        Some(EditorPersistenceResult::Saved { revision, .. }) if revision == captured_revision
    ));
    let slot = editor.workspace().scene(document).unwrap();
    assert_eq!(slot.saved_revision(), captured_revision);
    assert!(slot.is_dirty());
    let receipt = editor.last_persistence_receipt().unwrap();
    assert_eq!(
        receipt.publication_atomicity(),
        PublicationAtomicity::AtomicNameSwitch
    );
    assert_eq!(
        receipt.published_identity(),
        Some(receipt.candidate_identity())
    );
    assert_eq!(receipt.published_digest(), Some(receipt.digest()));

    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Accepted
    );
    assert!(matches!(
        editor
            .drive_editor_frame(std::time::Duration::ZERO)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Saved { .. })
    ));
    assert!(!editor.workspace().scene(document).unwrap().is_dirty());
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
        EditorPersistenceRequestResult::Accepted
    );

    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseScene { document }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert!(editor.workspace().is_empty());

    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Reopen { document: None }),
        EditorPersistenceRequestResult::Accepted
    );
    let reopened = editor
        .drive_editor_frame(std::time::Duration::ZERO)
        .persistence();
    let reopened_document = match reopened.result() {
        Some(EditorPersistenceResult::Opened { document, .. }) => document,
        other => panic!("expected reopened document, got {other:?}"),
    };
    let ids = editor
        .workspace()
        .scene(reopened_document)
        .unwrap()
        .session()
        .document()
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["captured", "later"]);
}

#[test]
fn external_write_rejects_save_and_preserves_dirty_document() {
    let project = TestProject::new("external-write");
    let mut editor = EditorProjectSession::open(project.capability(), EditorProjectIntent::new())
        .expect("the editor project should open");
    let document = editor.workspace().active_document().unwrap();
    let saved_revision = editor.workspace().scene(document).unwrap().saved_revision();
    let saved_digest = editor.workspace().scene(document).unwrap().saved_digest();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch("local"),
            })
            .applied
    );
    fs::write(
        project.scene_path(),
        SceneDocument::new([SceneEntityRecord::new(scene_id("external"))])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor
            .drive_editor_frame(std::time::Duration::ZERO)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Rejected {
            document: Some(document),
            reason: EditorPersistenceRejection::TargetChanged,
        })
    );
    let slot = editor.workspace().scene(document).unwrap();
    assert_eq!(slot.saved_revision(), saved_revision);
    assert_eq!(slot.saved_digest(), saved_digest);
    assert!(slot.is_dirty());
    let disk = fs::read_to_string(project.scene_path()).unwrap();
    assert!(disk.contains("external"));
    assert!(!disk.contains("local"));
}

#[test]
fn explicit_reopen_reconciles_a_dirty_document_from_persisted_bytes() {
    let project = TestProject::new("dirty-reopen");
    let mut editor = EditorProjectSession::open(project.capability(), EditorProjectIntent::new())
        .expect("the editor project should open");
    let document = editor.workspace().active_document().unwrap();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch("local-only"),
            })
            .applied
    );
    assert!(editor.workspace().scene(document).unwrap().is_dirty());

    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Accepted
    );
    let reopened = editor
        .drive_editor_frame(std::time::Duration::ZERO)
        .persistence();
    assert!(matches!(
        reopened.result(),
        Some(EditorPersistenceResult::Opened {
            document: reopened_document,
            ..
        }) if reopened_document == document
    ));
    let slot = editor.workspace().scene(document).unwrap();
    assert!(slot.session().document().entities.is_empty());
    assert!(!slot.is_dirty());
    assert!(slot.saved_digest().is_some());
}

#[test]
fn failed_reopen_preserves_the_dirty_workspace_and_can_be_retried() {
    let project = TestProject::new("failed-reopen");
    let original_bytes = fs::read(project.scene_path()).unwrap();
    let mut editor = EditorProjectSession::open(project.capability(), EditorProjectIntent::new())
        .expect("the editor project should open");
    let document = editor.workspace().active_document().unwrap();
    let local = scene_id("local");
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch(local.as_str()),
            })
            .applied
    );
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::SelectEntity {
                document: Some(document),
                entity: Some(local.clone()),
            })
            .applied
    );
    let before = editor.workspace().scene(document).unwrap();
    let before_document = before.session().document().clone();
    let before_revision = before.revision();
    let before_saved_revision = before.saved_revision();
    let before_saved_digest = before.saved_digest();
    let assert_preserved = |editor: &EditorProjectSession| {
        let slot = editor.workspace().scene(document).unwrap();
        assert_eq!(slot.session().document(), &before_document);
        assert_eq!(slot.revision(), before_revision);
        assert_eq!(slot.saved_revision(), before_saved_revision);
        assert_eq!(slot.saved_digest(), before_saved_digest);
        assert_eq!(slot.selection().top_entity(), Some(&local));
        assert!(slot.is_dirty());
    };

    fs::write(project.scene_path(), b"{ malformed").unwrap();
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor
            .drive_editor_frame(std::time::Duration::ZERO)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Failed {
            document: Some(document),
            stage: nara::tooling::EditorPersistenceFailureStage::Decode,
        })
    );
    assert_preserved(&editor);

    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
        EditorPersistenceRequestResult::Accepted
    );
    let duplicate = scene_id("duplicate");
    fs::write(
        project.scene_path(),
        SceneDocument::new([
            SceneEntityRecord::new(duplicate.clone()),
            SceneEntityRecord::new(duplicate),
        ])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor
            .drive_editor_frame(std::time::Duration::ZERO)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Failed {
            document: Some(document),
            stage: nara::tooling::EditorPersistenceFailureStage::Validate,
        })
    );
    assert_preserved(&editor);

    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
        EditorPersistenceRequestResult::Accepted
    );
    fs::remove_file(project.scene_path()).unwrap();
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor
            .drive_editor_frame(std::time::Duration::ZERO)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Rejected {
            document: Some(document),
            reason: EditorPersistenceRejection::TargetDeleted,
        })
    );
    assert_preserved(&editor);

    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
        EditorPersistenceRequestResult::Accepted
    );
    fs::create_dir(project.scene_path()).unwrap();
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Accepted
    );
    assert!(matches!(
        editor
            .drive_editor_frame(std::time::Duration::ZERO)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Failed {
            document: Some(failed_document),
            stage: nara::tooling::EditorPersistenceFailureStage::OpenTarget
                | nara::tooling::EditorPersistenceFailureStage::ReadTarget,
        }) if failed_document == document
    ));
    assert_preserved(&editor);

    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
        EditorPersistenceRequestResult::Accepted
    );
    fs::remove_dir(project.scene_path()).unwrap();
    fs::write(project.scene_path(), original_bytes).unwrap();
    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document),
        }),
        EditorPersistenceRequestResult::Accepted
    );
    assert!(matches!(
        editor
            .drive_editor_frame(std::time::Duration::ZERO)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Opened {
            document: reopened_document,
            ..
        }) if reopened_document == document
    ));
}

#[test]
fn dirty_close_cancel_preserves_the_running_owner_and_document() {
    let project = TestProject::new("dirty-close-cancel");
    let mut editor = EditorProjectSession::open(project.capability(), EditorProjectIntent::new())
        .expect("the editor project should open");
    let document = editor.workspace().active_document().unwrap();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch("local"),
            })
            .applied
    );
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Running);
    let generation = editor.play_view().generation().unwrap();

    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseScene { document }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.workspace_intent_view().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    assert_eq!(editor.play_view().state(), EditorPlayState::Running);
    assert_eq!(
        editor.resolve_workspace_intent(EditorCloseDecision::Cancel),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(editor.play_view().generation(), Some(generation));
    assert_eq!(editor.play_view().state(), EditorPlayState::Running);
    assert!(editor.workspace().scene(document).is_some());
    assert_eq!(
        editor.workspace_intent_view().result(),
        Some(EditorWorkspaceIntentResult::Cancelled {
            intent: EditorWorkspaceIntent::CloseScene { document },
        })
    );

    assert!(editor.acknowledge_workspace_intent_result());
    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Empty);
}

#[test]
fn dirty_close_discard_stops_play_before_removal_without_writing_disk() {
    let project = TestProject::new("dirty-close-discard");
    let original_bytes = fs::read(project.scene_path()).unwrap();
    let mut editor = EditorProjectSession::open(project.capability(), EditorProjectIntent::new())
        .expect("the editor project should open");
    let document = editor.workspace().active_document().unwrap();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch("discarded"),
            })
            .applied
    );
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Running);

    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseScene { document }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.resolve_workspace_intent(EditorCloseDecision::Discard),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    drive_workspace_intent_until_idle(&mut editor);

    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert!(editor.workspace().is_empty());
    assert_eq!(fs::read(project.scene_path()).unwrap(), original_bytes);
    assert_eq!(
        editor.workspace_intent_view().result(),
        Some(EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::CloseScene { document },
        })
    );
}

#[test]
fn dirty_close_save_conflict_keeps_document_and_runtime_open() {
    let project = TestProject::new("dirty-close-save-conflict");
    let mut editor = EditorProjectSession::open(project.capability(), EditorProjectIntent::new())
        .expect("the editor project should open");
    let document = editor.workspace().active_document().unwrap();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch("local"),
            })
            .applied
    );
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Running);
    let generation = editor.play_view().generation();
    fs::write(
        project.scene_path(),
        SceneDocument::new([SceneEntityRecord::new(scene_id("external"))])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseScene { document }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.resolve_workspace_intent(EditorCloseDecision::Save),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(std::time::Duration::ZERO);

    assert_eq!(editor.play_view().state(), EditorPlayState::Running);
    assert_eq!(editor.play_view().generation(), generation);
    assert!(editor.workspace().scene(document).unwrap().is_dirty());
    assert_eq!(
        editor.workspace_intent_view().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    assert_eq!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Rejected {
            document: Some(document),
            reason: EditorPersistenceRejection::TargetChanged,
        })
    );

    assert_eq!(
        editor.resolve_workspace_intent(EditorCloseDecision::Cancel),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Empty);
}

#[test]
fn reopen_rejects_while_a_runtime_owner_is_live() {
    let project = TestProject::new("reopen-runtime-active");
    let mut editor = EditorProjectSession::open(project.capability(), EditorProjectIntent::new())
        .expect("the editor project should open");
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Running);

    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::Reopen { document: None }),
        EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::RuntimeActive)
    );
    assert_eq!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Rejected {
            document: editor.workspace().active_document(),
            reason: EditorPersistenceRejection::RuntimeActive,
        })
    );

    assert_eq!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor.request_play(EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Empty);
}

#[test]
fn dirty_close_save_publishes_bytes_before_retiring_play() {
    let project = TestProject::new("dirty-close-save");
    let original_bytes = fs::read(project.scene_path()).unwrap();
    let mut editor = EditorProjectSession::open(project.capability(), EditorProjectIntent::new())
        .expect("the editor project should open");
    let document = editor.workspace().active_document().unwrap();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch("saved-before-stop"),
            })
            .applied
    );
    assert_eq!(
        editor.request_play(EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Running);

    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseScene { document }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.resolve_workspace_intent(EditorCloseDecision::Save),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(editor.play_view().state(), EditorPlayState::Running);
    assert_eq!(fs::read(project.scene_path()).unwrap(), original_bytes);

    editor.drive_editor_frame(std::time::Duration::ZERO);
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Saved { document: saved, .. }) if saved == document
    ));
    assert_ne!(fs::read(project.scene_path()).unwrap(), original_bytes);
    drive_workspace_intent_until_idle(&mut editor);
    assert_eq!(editor.play_view().state(), EditorPlayState::Empty);
    assert!(editor.workspace().is_empty());
}

#[test]
fn dirty_exit_uses_the_same_explicit_decision_before_closing() {
    let project = TestProject::new("dirty-exit");
    let original_bytes = fs::read(project.scene_path()).unwrap();
    let mut editor = EditorProjectSession::open(project.capability(), EditorProjectIntent::new())
        .expect("the editor project should open");
    let document = editor.workspace().active_document().unwrap();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch: add_entity_patch("discarded-on-exit"),
            })
            .applied
    );

    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::Exit),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.workspace_intent_view().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    assert_eq!(
        editor.resolve_workspace_intent(EditorCloseDecision::Cancel),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert!(editor.workspace().scene(document).is_some());
    assert!(editor.acknowledge_workspace_intent_result());

    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::Exit),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.resolve_workspace_intent(EditorCloseDecision::Discard),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    drive_workspace_intent_until_idle(&mut editor);
    assert!(editor.workspace().is_empty());
    assert_eq!(fs::read(project.scene_path()).unwrap(), original_bytes);
}

fn drive_play_until(editor: &mut EditorProjectSession, expected: EditorPlayState) {
    let deadline = Instant::now() + ASYNC_DRIVER_TIMEOUT;
    loop {
        if editor.play_view().state() == expected {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "editor Play state did not reach {expected:?}; observed {:?}",
                editor.play_view().state()
            );
        }
        editor.drive_editor_frame(Duration::ZERO);
        thread::sleep(ASYNC_POLL_INTERVAL);
    }
}

fn drive_workspace_intent_until_idle(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + ASYNC_DRIVER_TIMEOUT;
    loop {
        if editor.workspace_intent_view().phase().is_none() {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "workspace intent did not complete; observed {:?}",
                editor.workspace_intent_view().phase()
            );
        }
        editor.drive_editor_frame(Duration::ZERO);
        thread::sleep(ASYNC_POLL_INTERVAL);
    }
}

fn add_entity_patch(id: &str) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(scene_id(id)),
    }])
}

fn scene_id(id: &str) -> SceneEntityId {
    SceneEntityId::new(id).unwrap()
}

struct TestProject {
    root: PathBuf,
}

impl TestProject {
    fn new(name: &str) -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_editor_persistence_{}_{}",
            std::process::id(),
            sequence
        ));
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::create_dir_all(root.join("scenes")).unwrap();
        fs::create_dir_all(root.join("prefabs")).unwrap();
        fs::write(
            root.join("nara.toml"),
            format!(
                r#"schema_version = 1

[project]
name = "{name}"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d"]
"#
            ),
        )
        .unwrap();
        fs::write(
            root.join("scenes/startup.scene.json"),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        Self { root }
    }

    fn capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.root),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }

    fn scene_path(&self) -> PathBuf {
        self.root.join("scenes/startup.scene.json")
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let temporary_root = std::env::temp_dir().canonicalize().unwrap();
        let project_root = self.root.canonicalize().unwrap();
        assert!(project_root.starts_with(&temporary_root));
        assert!(
            project_root
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("nara_editor_persistence_"))
        );
        fs::remove_dir_all(&project_root).unwrap();
    }
}

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    {
        File::open(path).unwrap()
    }
}
