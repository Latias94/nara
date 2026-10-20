#![cfg(all(feature = "runtime-2d", feature = "serde", feature = "tooling"))]

#[path = "support/editor_action_driver.rs"]
mod editor_action_driver;

#[path = "support/editor_responsiveness_measurement.rs"]
mod responsiveness_measurement;

#[cfg(feature = "presentation-2d")]
#[path = "support/fake_preview_toolkit.rs"]
mod fake_preview_toolkit;

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use editor_action_driver as editor_driver;
use nara::{
    fs::{
        CapabilityRights, DirectoryCapability, ExpectedTarget, HostCapabilityOptions,
        RelativeComponent, TrustMode,
    },
    project_host::{
        EditorProjectSession, TrustedCompiledEditorProjectIntent,
        advanced::built_in_schema_providers,
    },
    reflect::AuthoringSchemaBundle,
    scene::{
        SceneAuthoringSession, SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
        ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    },
    tooling::{
        EditorAction, EditorActionOutcome, EditorCloseDecision, EditorDocumentMutation,
        EditorPersistenceAction, EditorPersistenceCommand, EditorPersistenceCommit,
        EditorPersistenceRejection, EditorPersistenceRequestKind, EditorPersistenceRequestResult,
        EditorPersistenceResult, EditorPlayCommand, EditorPlayRequestResult, EditorPlayState,
        EditorWorkspace, EditorWorkspaceCommand, EditorWorkspaceIntent, EditorWorkspaceIntentPhase,
        EditorWorkspaceIntentRequestResult, EditorWorkspaceIntentResult,
    },
};
#[cfg(feature = "presentation-2d")]
use nara::{
    render::{Extent2d, RenderFrameTarget},
    tooling::{SceneCompositionCommand, SceneCompositionEntityKind},
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static BUILT_IN_AUTHORING_SCHEMA_BUNDLE: OnceLock<Box<[u8]>> = OnceLock::new();
const ASYNC_DRIVER_TIMEOUT: Duration = Duration::from_secs(5);
const ASYNC_POLL_INTERVAL: Duration = Duration::from_millis(1);

#[cfg(feature = "presentation-2d")]
#[test]
fn accepted_scene_revision_publishes_one_matching_preview_candidate() {
    let project = TestProject::new("scene-preview-candidate");
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the editor project should open");
    editor.configure_editor_preview(Extent2d::new(320, 180));
    let _ = editor.take_editor_preview_candidate();

    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let base_revision = frame.workspace().scene(document).unwrap().revision;
    let outcomes = editor_driver::dispatch(
        &mut editor,
        [EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
            document,
            base_revision,
            command: SceneCompositionCommand::CreateEntity {
                kind: SceneCompositionEntityKind::ColoredSprite,
                placement: ScenePlacement::last(ScenePlacementOwner::Root),
            },
        })],
    );
    assert!(matches!(
        &outcomes[0],
        EditorActionOutcome::Workspace(report) if report.applied
    ));

    let frame = editor.drive_editor_frame(Duration::ZERO);
    let revision = frame.workspace().scene(document).unwrap().revision;
    let candidate = editor
        .take_editor_preview_candidate()
        .expect("the accepted document should produce one preview candidate");
    assert_eq!(candidate.document(), document);
    assert_eq!(candidate.revision(), revision);
    let (_, _, geometry_revision, packet, batches, hits) = candidate.into_parts();
    assert_eq!(geometry_revision, 2);
    assert_eq!(batches.total_instances(), 1);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].entity(), &scene_id("editor_entity_1"));
    assert_eq!(hits[0].draw_ordinal(), 0);
    assert!(hits[0].contains_clip_point([0.0, 0.0]));
    assert!(!hits[0].contains_clip_point([2.0, 2.0]));
    assert!(matches!(
        packet.target(),
        RenderFrameTarget::Image(target) if target.extent() == Extent2d::new(320, 180).unwrap()
    ));
}

#[test]
fn persistence_authority_surface_is_opaque_and_linear() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/editor_persistence/authority_not_clone.rs");
    tests.compile_fail("tests/ui/editor_persistence/checkpoint_not_clone.rs");
    tests.compile_fail("tests/ui/editor_persistence/checkpoint_not_constructible.rs");
    tests.compile_fail("tests/ui/editor_persistence/replace_receipt_not_clone.rs");
    tests.compile_fail("tests/ui/editor_persistence/receiptless_commit.rs");
    tests.compile_fail("tests/ui/editor_persistence/workspace_saved_state_is_private.rs");
    tests.compile_fail("tests/ui/editor_persistence/workspace_not_resource.rs");
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
fn save_advances_only_the_captured_revision_then_close_releases_the_lane() {
    let project = TestProject::new("save-reopen");
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the editor project should open");
    let document = editor.current_frame().workspace().active_document.unwrap();

    assert!(apply_scene_patch(
        &mut editor,
        document,
        add_entity_patch("captured")
    ));
    let captured_revision = editor
        .current_frame()
        .workspace()
        .scene(document)
        .unwrap()
        .revision;
    let later_patch =
        add_entity_patch_at("later", ScenePlacement::first(ScenePlacementOwner::Root));
    let outcomes = editor_driver::dispatch(
        &mut editor,
        [
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: captured_revision,
            }),
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: captured_revision,
            }),
            EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
                document,
                base_revision: captured_revision,
                patch: later_patch,
            }),
        ],
    );
    assert_eq!(
        outcomes[0],
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted)
    );
    assert_eq!(
        outcomes[1],
        EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Rejected(
            EditorPersistenceRejection::Busy,
        ))
    );
    assert!(matches!(
        &outcomes[2],
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let saved_frame = editor_driver::drive_persistence_until_result(&mut editor);
    let first = saved_frame.persistence();
    assert!(matches!(
        first.result(),
        Some(EditorPersistenceResult::Saved { revision, .. }) if revision == captured_revision
    ));
    let scene = current_scene_model(&editor, document);
    assert_eq!(scene.saved_revision, captured_revision);
    assert!(scene.dirty);
    let persisted = fs::read_to_string(project.scene_path()).unwrap();
    assert!(persisted.contains("\"captured\""));
    assert!(!persisted.contains("\"later\""));

    let undo_revision = editor
        .current_frame()
        .workspace()
        .scene(document)
        .unwrap()
        .revision;
    assert!(
        editor_driver::apply_workspace_command(
            &mut editor,
            EditorWorkspaceCommand::Undo {
                document,
                base_revision: undo_revision,
            },
        )
        .applied
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert!(
        !editor
            .current_frame()
            .workspace()
            .scene(document)
            .unwrap()
            .dirty
    );
    let redo_revision = editor
        .current_frame()
        .workspace()
        .scene(document)
        .unwrap()
        .revision;
    assert!(
        editor_driver::apply_workspace_command(
            &mut editor,
            EditorWorkspaceCommand::Redo {
                document,
                base_revision: redo_revision,
            },
        )
        .applied
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert!(
        editor
            .current_frame()
            .workspace()
            .scene(document)
            .unwrap()
            .dirty
    );
    let save_revision = editor
        .current_frame()
        .workspace()
        .scene(document)
        .unwrap()
        .revision;
    let outcomes = editor_driver::dispatch(
        &mut editor,
        [
            EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult),
            EditorAction::Persistence(EditorPersistenceAction::Save {
                document,
                base_revision: save_revision,
            }),
        ],
    );
    assert_eq!(
        outcomes,
        [
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted),
            EditorActionOutcome::Persistence(EditorPersistenceRequestResult::Accepted),
        ]
    );
    assert!(matches!(
        editor_driver::drive_persistence_until_result(&mut editor)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Saved { .. })
    ));
    assert!(
        !editor
            .current_frame()
            .workspace()
            .scene(document)
            .unwrap()
            .dirty
    );
    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::AcknowledgeResult,
        ),
        EditorPersistenceRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);

    assert_eq!(
        editor_driver::request_workspace_intent(
            &mut editor,
            EditorWorkspaceIntent::CloseDocument { document },
        ),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert!(editor.current_frame().workspace().scenes.is_empty());
    assert_eq!(
        editor.current_frame().reopen_target(),
        nara::tooling::EditorReopenTarget::Unavailable
    );

    drop(editor);

    let editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("a new Editor session should reopen the persisted source");
    let reopened_document = editor.current_frame().workspace().active_document.unwrap();
    let reopened = current_scene_model(&editor, reopened_document);
    let ids = reopened
        .editor
        .inspector
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["later", "captured"]);
}

#[test]
fn external_write_rejects_save_and_preserves_dirty_document() {
    let project = TestProject::new("external-write");
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the editor project should open");
    let document = editor.current_frame().workspace().active_document.unwrap();
    let saved_revision = current_scene_model(&editor, document).saved_revision;
    assert!(apply_scene_patch(
        &mut editor,
        document,
        add_entity_patch("local")
    ));
    fs::write(
        project.scene_path(),
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("external"))])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::Save {
                document: Some(document),
            },
        ),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor_driver::drive_persistence_until_result(&mut editor)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Rejected {
            request: EditorPersistenceRequestKind::Save,
            document: Some(document),
            reason: EditorPersistenceRejection::TargetChanged,
        })
    );
    let scene = current_scene_model(&editor, document);
    assert_eq!(scene.saved_revision, saved_revision);
    assert!(scene.dirty);
    let disk = fs::read_to_string(project.scene_path()).unwrap();
    assert!(disk.contains("external"));
    assert!(!disk.contains("local"));

    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::Save {
                document: Some(document),
            },
        ),
        EditorPersistenceRequestResult::Accepted
    );
    assert!(matches!(
        editor_driver::drive_persistence_until_result(&mut editor)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Rejected {
            request: EditorPersistenceRequestKind::Save,
            document: Some(rejected_document),
            reason: EditorPersistenceRejection::TargetChanged,
        }) if rejected_document == document
    ));
}

#[test]
fn explicit_reopen_reconciles_a_dirty_document_from_persisted_bytes() {
    let project = TestProject::new("dirty-reopen");
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the editor project should open");
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert!(apply_scene_patch(
        &mut editor,
        document,
        add_entity_patch("local-only")
    ));
    assert!(
        editor
            .current_frame()
            .workspace()
            .scene(document)
            .unwrap()
            .dirty
    );

    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::Reopen {
                document: Some(document),
            },
        ),
        EditorPersistenceRequestResult::Accepted
    );
    let reopened = editor_driver::drive_persistence_until_result(&mut editor).persistence();
    assert!(matches!(
        reopened.result(),
        Some(EditorPersistenceResult::Opened {
            document: reopened_document,
            ..
        }) if reopened_document == document
    ));
    let scene = current_scene_model(&editor, document);
    assert!(scene.editor.inspector.entities.is_empty());
    assert!(!scene.dirty);
}

#[test]
fn failed_reopen_preserves_the_dirty_workspace_and_can_be_retried() {
    let project = TestProject::new("failed-reopen");
    let original_bytes = fs::read(project.scene_path()).unwrap();
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the editor project should open");
    let document = editor.current_frame().workspace().active_document.unwrap();
    let local = scene_id("local");
    assert!(apply_scene_patch(
        &mut editor,
        document,
        add_entity_patch(local.as_str())
    ));
    assert!(
        editor_driver::apply_workspace_command(
            &mut editor,
            EditorWorkspaceCommand::SelectEntity {
                document,
                entity: Some(local.clone()),
            },
        )
        .applied
    );
    editor.drive_editor_frame(Duration::ZERO);
    let before = current_scene_model(&editor, document);
    let assert_preserved = |editor: &EditorProjectSession| {
        assert_eq!(current_scene_model(editor, document), before);
        assert_eq!(before.selection.top_entity(), Some(&local));
        assert!(before.dirty);
    };

    fs::write(project.scene_path(), b"{ malformed").unwrap();
    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::Reopen {
                document: Some(document),
            },
        ),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor_driver::drive_persistence_until_result(&mut editor)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Failed {
            request: EditorPersistenceRequestKind::Reopen,
            document: Some(document),
            stage: nara::tooling::EditorPersistenceFailureStage::Decode,
        })
    );
    assert_preserved(&editor);

    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::AcknowledgeResult,
        ),
        EditorPersistenceRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    let duplicate = scene_id("duplicate");
    fs::write(
        project.scene_path(),
        SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(duplicate.clone()),
            SceneEntityRecord::new(duplicate),
        ])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::Reopen {
                document: Some(document),
            },
        ),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor_driver::drive_persistence_until_result(&mut editor)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Failed {
            request: EditorPersistenceRequestKind::Reopen,
            document: Some(document),
            stage: nara::tooling::EditorPersistenceFailureStage::Validate,
        })
    );
    assert_preserved(&editor);

    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::AcknowledgeResult,
        ),
        EditorPersistenceRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    fs::remove_file(project.scene_path()).unwrap();
    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::Reopen {
                document: Some(document),
            },
        ),
        EditorPersistenceRequestResult::Accepted
    );
    assert_eq!(
        editor_driver::drive_persistence_until_result(&mut editor)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Rejected {
            request: EditorPersistenceRequestKind::Reopen,
            document: Some(document),
            reason: EditorPersistenceRejection::TargetDeleted,
        })
    );
    assert_preserved(&editor);

    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::AcknowledgeResult,
        ),
        EditorPersistenceRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    fs::create_dir(project.scene_path()).unwrap();
    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::Reopen {
                document: Some(document),
            },
        ),
        EditorPersistenceRequestResult::Accepted
    );
    assert!(matches!(
        editor_driver::drive_persistence_until_result(&mut editor)
            .persistence()
            .result(),
        Some(EditorPersistenceResult::Failed {
            request: EditorPersistenceRequestKind::Reopen,
            document: Some(failed_document),
            stage: nara::tooling::EditorPersistenceFailureStage::OpenTarget
                | nara::tooling::EditorPersistenceFailureStage::ReadTarget,
        }) if failed_document == document
    ));
    assert_preserved(&editor);

    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::AcknowledgeResult,
        ),
        EditorPersistenceRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    fs::remove_dir(project.scene_path()).unwrap();
    fs::write(project.scene_path(), original_bytes).unwrap();
    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::Reopen {
                document: Some(document),
            },
        ),
        EditorPersistenceRequestResult::Accepted
    );
    assert!(matches!(
        editor_driver::drive_persistence_until_result(&mut editor)
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
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the editor project should open");
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert!(apply_scene_patch(
        &mut editor,
        document,
        add_entity_patch("local")
    ));
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Running);
    let generation = editor.current_frame().play().generation().unwrap();

    assert_eq!(
        editor_driver::request_workspace_intent(
            &mut editor,
            EditorWorkspaceIntent::CloseDocument { document },
        ),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Running
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor_driver::resolve_workspace_intent(&mut editor, EditorCloseDecision::Cancel),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.current_frame().play().generation(), Some(generation));
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Running
    );
    assert!(editor.current_frame().workspace().scene(document).is_some());
    assert_eq!(
        editor.current_frame().workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Cancelled {
            intent: EditorWorkspaceIntent::CloseDocument { document },
        })
    );
    assert!(editor_driver::acknowledge_workspace_intent_result(
        &mut editor
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Empty);
}

#[test]
fn dirty_close_discard_stops_play_before_removal_without_writing_disk() {
    let project = TestProject::new("dirty-close-discard");
    let original_bytes = fs::read(project.scene_path()).unwrap();
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the editor project should open");
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert!(apply_scene_patch(
        &mut editor,
        document,
        add_entity_patch("discarded")
    ));
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Running);

    assert_eq!(
        editor_driver::request_workspace_intent(
            &mut editor,
            EditorWorkspaceIntent::CloseDocument { document },
        ),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor_driver::resolve_workspace_intent(&mut editor, EditorCloseDecision::Discard),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    drive_workspace_intent_until_idle(&mut editor);

    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty
    );
    assert!(editor.current_frame().workspace().scenes.is_empty());
    assert_eq!(fs::read(project.scene_path()).unwrap(), original_bytes);
    assert_eq!(
        editor.current_frame().workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::CloseDocument { document },
        })
    );
}

#[test]
fn dirty_close_save_conflict_keeps_document_and_runtime_open() {
    let project = TestProject::new("dirty-close-save-conflict");
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the editor project should open");
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert!(apply_scene_patch(
        &mut editor,
        document,
        add_entity_patch("local")
    ));
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Running);
    let generation = editor.current_frame().play().generation();
    fs::write(
        project.scene_path(),
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("external"))])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    assert_eq!(
        editor_driver::request_workspace_intent(
            &mut editor,
            EditorWorkspaceIntent::CloseDocument { document },
        ),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor_driver::resolve_workspace_intent(&mut editor, EditorCloseDecision::Save),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor_driver::drive_persistence_until_result(&mut editor);

    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Running
    );
    assert_eq!(editor.current_frame().play().generation(), generation);
    assert!(
        editor
            .current_frame()
            .workspace()
            .scene(document)
            .unwrap()
            .dirty
    );
    assert_eq!(
        editor.current_frame().workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    assert_eq!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Rejected {
            request: EditorPersistenceRequestKind::Save,
            document: Some(document),
            reason: EditorPersistenceRejection::TargetChanged,
        })
    );

    assert_eq!(
        editor_driver::resolve_workspace_intent(&mut editor, EditorCloseDecision::Cancel),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Empty);
}

#[test]
fn reopen_rejects_while_a_runtime_owner_is_live() {
    let project = TestProject::new("reopen-runtime-active");
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the editor project should open");
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Running);

    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::Reopen { document: None },
        ),
        EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::RuntimeActive)
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Rejected {
            request: EditorPersistenceRequestKind::Reopen,
            document: editor.current_frame().workspace().active_document,
            reason: EditorPersistenceRejection::RuntimeActive,
        })
    );
    assert_eq!(
        editor_driver::request_persistence(
            &mut editor,
            EditorPersistenceCommand::AcknowledgeResult,
        ),
        EditorPersistenceRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Stop),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Empty);
}

#[test]
fn dirty_close_save_publishes_bytes_before_retiring_play() {
    let project = TestProject::new("dirty-close-save");
    let original_bytes = fs::read(project.scene_path()).unwrap();
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the editor project should open");
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert!(apply_scene_patch(
        &mut editor,
        document,
        add_entity_patch("saved-before-stop")
    ));
    assert_eq!(
        editor_driver::request_play(&mut editor, EditorPlayCommand::Play),
        EditorPlayRequestResult::Accepted
    );
    drive_play_until(&mut editor, EditorPlayState::Running);

    assert_eq!(
        editor_driver::request_workspace_intent(
            &mut editor,
            EditorWorkspaceIntent::CloseDocument { document },
        ),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor_driver::resolve_workspace_intent(&mut editor, EditorCloseDecision::Save),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Running
    );
    assert_eq!(fs::read(project.scene_path()).unwrap(), original_bytes);

    editor_driver::drive_persistence_until_result(&mut editor);
    assert!(matches!(
        editor.current_frame().persistence().result(),
        Some(EditorPersistenceResult::Saved { document: saved, .. }) if saved == document
    ));
    assert_ne!(fs::read(project.scene_path()).unwrap(), original_bytes);
    drive_workspace_intent_until_idle(&mut editor);
    assert_eq!(
        editor.current_frame().play().state(),
        EditorPlayState::Empty
    );
    assert!(editor.current_frame().workspace().scenes.is_empty());
}

#[test]
fn dirty_exit_prepares_non_destructively_before_commit() {
    let project = TestProject::new("dirty-exit");
    let original_bytes = fs::read(project.scene_path()).unwrap();
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.capability(),
        TrustedCompiledEditorProjectIntent::new(),
    )
    .expect("the editor project should open");
    let document = editor.current_frame().workspace().active_document.unwrap();
    assert!(apply_scene_patch(
        &mut editor,
        document,
        add_entity_patch("discarded-on-exit")
    ));

    assert_eq!(
        editor_driver::request_workspace_intent(&mut editor, EditorWorkspaceIntent::Exit),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.current_frame().workspace_intent().phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    assert_eq!(
        editor_driver::resolve_workspace_intent(&mut editor, EditorCloseDecision::Cancel),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert!(editor.current_frame().workspace().scene(document).is_some());
    assert!(editor_driver::acknowledge_workspace_intent_result(
        &mut editor
    ));
    editor.drive_editor_frame(Duration::ZERO);

    assert_eq!(
        editor_driver::request_workspace_intent(&mut editor, EditorWorkspaceIntent::Exit),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor_driver::resolve_workspace_intent(&mut editor, EditorCloseDecision::Discard),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    drive_workspace_intent_until_phase(&mut editor, EditorWorkspaceIntentPhase::Prepared);
    assert!(editor.current_frame().workspace().scene(document).is_some());
    assert_eq!(
        editor_driver::commit_prepared_workspace_intent(&mut editor),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert!(editor.current_frame().workspace().scene(document).is_some());
    assert!(matches!(
        editor.current_frame().workspace_intent().result(),
        Some(EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::Exit,
        })
    ));
    assert_eq!(fs::read(project.scene_path()).unwrap(), original_bytes);
}

fn drive_play_until(editor: &mut EditorProjectSession, expected: EditorPlayState) {
    let deadline = Instant::now() + ASYNC_DRIVER_TIMEOUT;
    loop {
        if editor.current_frame().play().state() == expected {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "editor Play state did not reach {expected:?}; observed {:?}",
                editor.current_frame().play().state()
            );
        }
        editor.drive_editor_frame(Duration::ZERO);
        thread::sleep(ASYNC_POLL_INTERVAL);
    }
}

fn drive_workspace_intent_until_idle(editor: &mut EditorProjectSession) {
    let deadline = Instant::now() + ASYNC_DRIVER_TIMEOUT;
    loop {
        if editor.current_frame().workspace_intent().phase().is_none() {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "workspace intent did not complete; observed {:?}",
                editor.current_frame().workspace_intent().phase()
            );
        }
        editor.drive_editor_frame(Duration::ZERO);
        thread::sleep(ASYNC_POLL_INTERVAL);
    }
}

fn drive_workspace_intent_until_phase(
    editor: &mut EditorProjectSession,
    expected: EditorWorkspaceIntentPhase,
) {
    let deadline = Instant::now() + ASYNC_DRIVER_TIMEOUT;
    loop {
        if editor.current_frame().workspace_intent().phase() == Some(expected) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "workspace intent did not reach {expected:?}; observed {:?}",
                editor.current_frame().workspace_intent().phase()
            );
        }
        editor.drive_editor_frame(Duration::ZERO);
        thread::sleep(ASYNC_POLL_INTERVAL);
    }
}

fn apply_scene_patch(
    editor: &mut EditorProjectSession,
    document: nara::tooling::EditorDocumentId,
    patch: ScenePatchDocument,
) -> bool {
    let base_revision = editor
        .current_frame()
        .workspace()
        .scene(document)
        .unwrap()
        .revision;
    let applied = editor_driver::apply_workspace_command(
        editor,
        EditorWorkspaceCommand::ApplyScenePatch {
            document,
            base_revision,
            patch,
        },
    )
    .applied;
    editor.drive_editor_frame(Duration::ZERO);
    applied
}

fn current_scene_model(
    editor: &EditorProjectSession,
    document: nara::tooling::EditorDocumentId,
) -> nara::tooling::EditorSceneModel {
    let frame = editor.current_frame();
    let scene = frame
        .workspace()
        .active_scene
        .as_ref()
        .expect("the persistence tests keep one active Scene document");
    assert_eq!(scene.document, document);
    scene.clone()
}

fn add_entity_patch(id: &str) -> ScenePatchDocument {
    add_entity_patch_at(id, ScenePlacement::last(ScenePlacementOwner::Root))
}

fn add_entity_patch_at(id: &str, placement: ScenePlacement) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(scene_id(id)),
        placement,
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
        fs::create_dir_all(root.join("schema")).unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            built_in_authoring_schema_bundle(),
        )
        .unwrap();
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

[authoring]
schema_bundle = "schema/authoring.bundle.json"

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

fn built_in_authoring_schema_bundle() -> &'static [u8] {
    BUILT_IN_AUTHORING_SCHEMA_BUNDLE.get_or_init(|| {
        AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
            .unwrap()
            .to_json_string()
            .unwrap()
            .into_bytes()
            .into_boxed_slice()
    })
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
